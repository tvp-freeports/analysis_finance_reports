//! Rust port of `_internals/cli/conf_parse.py`'s `FreeportsConfig` — the final, fully validated
//! configuration `cmd.py`/`main.py` actually run against, built from a [`PartialConfig`] that's
//! already gone through the default → file → env → cmd precedence chain. See
//! `agent-memory/rust-native-binary-plan.md`, Fase E, punto 3b-vii.
//!
//! Runs the same 6 validation steps as the Python original, in the same order (each can see the
//! previous step's mutations, exactly like Pydantic's `model_validator(mode="after")` chain
//! running in declaration order): [`set_compress_flag`], [`detect_format`],
//! [`right_out_profile_type`], [`out_path_exists`], [`out_path_single_file`],
//! [`validate_document_specs`]. Two of them fix confirmed bugs rather than reproducing them —
//! see each function's doc comment for the empirical evidence.

use std::path::{Path, PathBuf};

use super::conf_parse::{DocumentSpec, OutFlags, OutStructureMode, Verbosity};
use super::partial_config::PartialConfig;
use crate::formats_repo::metadata::{get_formats, url_to_format, MetadataError};

/// `MissingTargetLists`/`MissingInputDbPath`/`MissingFormatsRepoPath` are the only fields
/// [`PartialConfig::defaults`] leaves genuinely unset (see that function's doc comment for why —
/// there's no sensible universal default for "where is your formats repo"), so they're the only
/// ones [`FreeportsConfig::build`] can still fail to find a value for; every other field either
/// falls back to its [`PartialConfig::defaults`] value or has its own dedicated error variant
/// ([`FreeportsConfigError::NoInputReports`] for `INPUT_REPORTS`).
#[derive(Debug, Clone, PartialEq)]
pub enum FreeportsConfigError {
    MissingTargetLists,
    MissingInputDbPath,
    MissingFormatsRepoPath,
    NoInputReports,
    MissingPathOnDocument,
    PathDoesNotExist(PathBuf),
    ConflictingDetectedFormat { previous: String, detected: String },
    FormatNotSpecifiedOrDetected,
    BatchFileDoesNotExist(PathBuf),
    SeparateOutFilesRequiresBatchMode,
    OutPathParentDoesNotExist(PathBuf),
    Metadata(MetadataError),
}

impl std::fmt::Display for FreeportsConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FreeportsConfigError::MissingTargetLists => write!(f, "`TARGET_LISTS` is required but was not set by any config source"),
            FreeportsConfigError::MissingInputDbPath => write!(f, "`INPUT_DB_PATH` is required but was not set by any config source"),
            FreeportsConfigError::MissingFormatsRepoPath => {
                write!(f, "`FORMATS_REPO_PATH` is required but was not set by any config source")
            }
            FreeportsConfigError::NoInputReports => write!(f, "at least one input document must be specified"),
            FreeportsConfigError::MissingPathOnDocument => write!(f, "a document with no URL must have a path"),
            FreeportsConfigError::PathDoesNotExist(path) => write!(f, "the specified path `{}` does not exist", path.display()),
            FreeportsConfigError::ConflictingDetectedFormat { previous, detected } => write!(
                f,
                "detected format different across input reports, previous detected was {previous}, this is {detected}"
            ),
            FreeportsConfigError::FormatNotSpecifiedOrDetected => write!(f, "format has to be specified or detected"),
            FreeportsConfigError::BatchFileDoesNotExist(path) => write!(f, "insert a valid batch file name, not `{}`", path.display()),
            FreeportsConfigError::SeparateOutFilesRequiresBatchMode => {
                write!(f, "SEPARATE_OUT_FILES may only be set in batch mode (BATCH_FILE must be set)")
            }
            FreeportsConfigError::OutPathParentDoesNotExist(path) => {
                write!(f, "out path is not valid because directory `{}` doesn't exist", path.display())
            }
            FreeportsConfigError::Metadata(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FreeportsConfigError {}

/// [`detect_format`]'s CSV-reading failures (bad `formats.csv`/`url_mapping.csv`) get their own
/// typed variant — `formats_repo::metadata`'s functions are plain Rust now, not Python, so there's
/// no `PyErr` to convert in the first place.
impl From<MetadataError> for FreeportsConfigError {
    fn from(e: MetadataError) -> Self {
        FreeportsConfigError::Metadata(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FreeportsConfig {
    pub verbosity: Verbosity,
    pub n_workers: u32,
    pub batch_file: Option<PathBuf>,
    pub save_pdf: bool,
    pub input_reports: Vec<DocumentSpec>,
    pub format: Option<String>,
    pub config_file: Option<PathBuf>,
    pub target_lists: Vec<String>,
    pub out_profile: OutStructureMode,
    pub out_flags: OutFlags,
    pub out_path: PathBuf,
    pub input_db_path: PathBuf,
    pub formats_repo_path: PathBuf,
}

/// Mirrors `set_compress_flag`: an `OUT_PATH` ending in `.tar.gz` implies `COMPRESSED` and has
/// that suffix stripped back off, so downstream code always writes to the uncompressed path and
/// archives it afterward.
fn set_compress_flag(out_path: PathBuf, out_flags: OutFlags) -> (PathBuf, OutFlags) {
    let name = out_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if let Some(stripped) = name.strip_suffix(".tar.gz") {
        let new_path = out_path.with_file_name(stripped);
        (new_path, out_flags | OutFlags::COMPRESSED)
    } else {
        (out_path, out_flags)
    }
}

/// Mirrors `detect_format`. **Fixes a confirmed bug**: the Python original writes
/// `if detect_format is None:` — referencing the *enclosing method's own name* (missing the
/// "ed"), not the `detected_format` local variable. A bare method name isn't resolvable
/// unqualified inside its own body in Python (not local, not global), so this raised `NameError`
/// at runtime the moment a format was actually detected from a URL — auto-detection could never
/// succeed. Fixed here by simply tracking `detected_format` correctly; the actual per-URL
/// matching is now a direct, in-crate call into [`crate::formats_repo::metadata`]'s native Rust
/// port of `freeports._internals.formats.repo.metadata` (see
/// `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`, Milestone 1 Step 1.3) —
/// no `py.import`/Python round-trip at all, so this function is plain sync Rust, no `Python<'_>`
/// token needed anywhere in its body.
fn detect_format(
    format: Option<String>,
    input_reports: &[DocumentSpec],
    formats_repo_path: &Path,
) -> Result<String, FreeportsConfigError> {
    let mut detected_format: Option<String> = None;
    let mut format_names_cache: Option<Vec<String>> = None;

    for doc in input_reports {
        let Some(url) = &doc.url else { continue };
        let format_names = match &format_names_cache {
            Some(names) => names.clone(),
            None => {
                let names = get_formats(formats_repo_path)?;
                format_names_cache = Some(names.clone());
                names
            }
        };
        let detected = url_to_format(formats_repo_path, &format_names, url.as_ref())?;
        if let Some(detected) = detected {
            match &detected_format {
                None => detected_format = Some(detected),
                Some(previous) if *previous != detected => {
                    return Err(FreeportsConfigError::ConflictingDetectedFormat { previous: previous.clone(), detected });
                }
                Some(_) => {}
            }
        }
    }

    let format = match (format, detected_format) {
        (Some(explicit), Some(detected)) if explicit != detected => {
            tracing::warn!(explicit, detected, "selected format is different from detected one");
            Some(explicit)
        }
        (Some(explicit), _) => Some(explicit),
        (None, Some(detected)) => Some(detected),
        (None, None) => None,
    };

    format.ok_or(FreeportsConfigError::FormatNotSpecifiedOrDetected)
}

/// Mirrors `right_out_profile_type`. `BATCH_FILE` existence is already guaranteed by the point
/// this runs (it's checked wherever a `PathBuf` for it is accepted — mirroring Pydantic's
/// `FilePath` field validation, which the Python original then redundantly checks *again* here;
/// not reproduced, since it can't ever fail once the field itself is already validated).
/// [`super::conf_parse::OutStructureMode`]/[`OutFlags`] no longer need a mode-specific *type* to
/// enforce `SEPARATE_OUT_FILES` is batch-only (see those types' doc comments) — enforced directly
/// here instead.
fn right_out_profile_type(batch_file: Option<&Path>, out_flags: OutFlags) -> Result<(), FreeportsConfigError> {
    if out_flags.contains(OutFlags::SEPARATE_OUT_FILES) && batch_file.is_none() {
        return Err(FreeportsConfigError::SeparateOutFilesRequiresBatchMode);
    }
    Ok(())
}

/// Mirrors `out_path_exists`.
fn out_path_exists(out_path: &Path) -> Result<(), FreeportsConfigError> {
    let parent = out_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        return Err(FreeportsConfigError::OutPathParentDoesNotExist(parent.to_path_buf()));
    }
    Ok(())
}

/// Mirrors `out_path_single_file`.
fn out_path_single_file(out_path: PathBuf, out_profile: OutStructureMode) -> PathBuf {
    if out_profile == OutStructureMode::SingleFile {
        let ends_in_csv = out_path.extension().and_then(|e| e.to_str()) == Some("csv");
        if !ends_in_csv {
            return out_path.join("out.csv");
        }
    }
    out_path
}

fn glob_pdfs(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("pdf"))
        .collect();
    paths.sort();
    Ok(paths)
}

/// Mirrors `validate_document_specs`. **Fixes 3 confirmed typos** in the "document has a URL"
/// branch: the original calls `d.is_dir()` / `d.parent.exist()` / `d.exist()` directly on the
/// `DocumentSpec` (which has no such methods) instead of on `d.path` (`d.path.is_dir()` /
/// `d.path.parent.exists()` / `d.path.exists()`) — an `AttributeError` at runtime whenever a
/// document specified both a URL and a local path. Fixed by operating on `.path` throughout, as
/// the surrounding logic (and the "no URL" branch, which doesn't have this bug) clearly intends.
fn validate_document_specs(input_reports: Vec<DocumentSpec>, save_pdf: bool) -> Result<(Vec<DocumentSpec>, bool), FreeportsConfigError> {
    let mut new_docs = Vec::new();
    let mut save_pdf = save_pdf;

    for d in input_reports {
        if d.url.is_none() {
            let Some(path) = &d.path else {
                return Err(FreeportsConfigError::MissingPathOnDocument);
            };
            if path.is_dir() {
                for r in glob_pdfs(path).map_err(|_| FreeportsConfigError::PathDoesNotExist(path.clone()))? {
                    let file_name = r.file_name().map(PathBuf::from).unwrap_or_default();
                    let name = Path::new(d.name.as_deref().unwrap_or_default()).join(&file_name);
                    new_docs.push(DocumentSpec { url: None, path: Some(r), name: Some(name.display().to_string()) });
                }
            } else if path.is_file() {
                new_docs.push(d);
            } else {
                return Err(FreeportsConfigError::PathDoesNotExist(path.clone()));
            }
        } else {
            match &d.path {
                None => {
                    if save_pdf {
                        tracing::warn!("SAVE_PDF was set but no path was selected, so the option is ignored");
                        save_pdf = false;
                    }
                    new_docs.push(d);
                }
                Some(path) if path.is_dir() => {
                    if save_pdf {
                        let new_path = path.join("report.pdf");
                        let mut d = d;
                        d.path = Some(new_path);
                        new_docs.push(d);
                    } else {
                        for r in glob_pdfs(path).map_err(|_| FreeportsConfigError::PathDoesNotExist(path.clone()))? {
                            new_docs.push(DocumentSpec { url: d.url.clone(), path: Some(r.clone()), name: Some(r.display().to_string()) });
                        }
                    }
                }
                Some(path) if path.parent().is_some_and(Path::exists) => {
                    if path.exists() {
                        save_pdf = false;
                    } else if !save_pdf {
                        tracing::warn!(path = %path.display(), "invalid file specified with SAVE_PDF=false and URL present, falling back to URL");
                    }
                    new_docs.push(d);
                }
                Some(path) => {
                    tracing::warn!(path = %path.display(), "invalid file specified with SAVE_PDF=false and URL present, falling back to URL");
                    new_docs.push(d);
                }
            }
        }
    }

    Ok((new_docs, save_pdf))
}

impl FreeportsConfig {
    pub fn build(config: PartialConfig) -> Result<Self, FreeportsConfigError> {
        let defaults = PartialConfig::defaults();
        let verbosity = config.verbosity.or(defaults.verbosity).expect("PartialConfig::defaults() always sets `verbosity`");
        let n_workers = config.n_workers.or(defaults.n_workers).expect("PartialConfig::defaults() always sets `n_workers`");
        let out_profile =
            config.out_profile.or(defaults.out_profile).expect("PartialConfig::defaults() always sets `out_profile`");
        let out_flags = config.out_flags.or(defaults.out_flags).expect("PartialConfig::defaults() always sets `out_flags`");
        let out_path = config.out_path.or(defaults.out_path).expect("PartialConfig::defaults() always sets `out_path`");
        let save_pdf = config.save_pdf.or(defaults.save_pdf).expect("PartialConfig::defaults() always sets `save_pdf`");

        let target_lists = config.target_lists.ok_or(FreeportsConfigError::MissingTargetLists)?;
        let input_db_path = config.input_db_path.ok_or(FreeportsConfigError::MissingInputDbPath)?;
        let formats_repo_path = config.formats_repo_path.ok_or(FreeportsConfigError::MissingFormatsRepoPath)?;
        let input_reports = config.input_reports.filter(|d| !d.is_empty()).ok_or(FreeportsConfigError::NoInputReports)?;
        let batch_file = config.batch_file;

        if let Some(batch_file) = &batch_file
            && !batch_file.is_file()
        {
            return Err(FreeportsConfigError::BatchFileDoesNotExist(batch_file.clone()));
        }

        let (out_path, out_flags) = set_compress_flag(out_path, out_flags);
        let format = detect_format(config.format, &input_reports, &formats_repo_path)?;
        right_out_profile_type(batch_file.as_deref(), out_flags)?;
        out_path_exists(&out_path)?;
        let out_path = out_path_single_file(out_path, out_profile);
        let (input_reports, save_pdf) = validate_document_specs(input_reports, save_pdf)?;

        Ok(Self {
            verbosity,
            n_workers,
            batch_file,
            save_pdf,
            input_reports,
            format: Some(format),
            config_file: config.config_file,
            target_lists,
            out_profile,
            out_flags,
            out_path,
            input_db_path,
            formats_repo_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    fn minimal_config(dir: &Path) -> PartialConfig {
        let doc = DocumentSpec::new(None, Some(dir.join("input.pdf")), None).unwrap();
        std::fs::write(dir.join("input.pdf"), b"%PDF-1.4").unwrap();
        PartialConfig {
            input_reports: Some(vec![doc]),
            format: Some("my-format".to_string()),
            target_lists: Some(vec!["TEST".to_string()]),
            input_db_path: Some(dir.to_path_buf()),
            formats_repo_path: Some(dir.to_path_buf()),
            ..PartialConfig::defaults()
        }
    }

    #[test]
    fn set_compress_flag_strips_tar_gz_and_sets_compressed() {
        let (path, flags) = set_compress_flag(PathBuf::from("/tmp/out.tar.gz"), OutFlags::NONE);
        assert_eq!(path, PathBuf::from("/tmp/out"));
        assert!(flags.contains(OutFlags::COMPRESSED));
    }

    #[test]
    fn set_compress_flag_leaves_other_paths_untouched() {
        let (path, flags) = set_compress_flag(PathBuf::from("/tmp/out.csv"), OutFlags::NONE);
        assert_eq!(path, PathBuf::from("/tmp/out.csv"));
        assert!(!flags.contains(OutFlags::COMPRESSED));
    }

    #[test]
    fn right_out_profile_type_rejects_separate_out_files_without_batch_mode() {
        assert_eq!(
            right_out_profile_type(None, OutFlags::SEPARATE_OUT_FILES),
            Err(FreeportsConfigError::SeparateOutFilesRequiresBatchMode)
        );
    }

    #[test]
    fn right_out_profile_type_allows_separate_out_files_with_batch_mode() {
        assert!(right_out_profile_type(Some(Path::new("/tmp/jobs.csv")), OutFlags::SEPARATE_OUT_FILES).is_ok());
    }

    #[test]
    fn right_out_profile_type_allows_compressed_without_batch_mode() {
        assert!(right_out_profile_type(None, OutFlags::COMPRESSED).is_ok());
    }

    #[test]
    fn out_path_exists_rejects_a_missing_parent_directory() {
        let path = PathBuf::from("/definitely/does/not/exist/out.csv");
        assert_eq!(
            out_path_exists(&path),
            Err(FreeportsConfigError::OutPathParentDoesNotExist(PathBuf::from("/definitely/does/not/exist")))
        );
    }

    #[test]
    fn out_path_exists_accepts_an_existing_parent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(out_path_exists(&dir.path().join("out.csv")).is_ok());
    }

    #[test]
    fn out_path_single_file_appends_out_csv_when_out_path_is_a_directory() {
        let result = out_path_single_file(PathBuf::from("/tmp/results"), OutStructureMode::SingleFile);
        assert_eq!(result, PathBuf::from("/tmp/results/out.csv"));
    }

    #[test]
    fn out_path_single_file_leaves_an_already_csv_path_untouched() {
        let result = out_path_single_file(PathBuf::from("/tmp/out.csv"), OutStructureMode::SingleFile);
        assert_eq!(result, PathBuf::from("/tmp/out.csv"));
    }

    #[test]
    fn out_path_single_file_does_nothing_outside_single_file_mode() {
        let result = out_path_single_file(PathBuf::from("/tmp/results"), OutStructureMode::Regular);
        assert_eq!(result, PathBuf::from("/tmp/results"));
    }

    #[test]
    fn validate_document_specs_expands_a_directory_of_pdfs_with_no_url() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.pdf"), b"x").unwrap();
        std::fs::write(dir.path().join("b.pdf"), b"x").unwrap();
        std::fs::write(dir.path().join("c.txt"), b"x").unwrap();
        let spec = DocumentSpec::new(None, Some(dir.path().to_path_buf()), Some("MyDocs".to_string())).unwrap();
        let (docs, save_pdf) = validate_document_specs(vec![spec], true).unwrap();
        assert_eq!(docs.len(), 2);
        assert!(save_pdf);
        for d in &docs {
            assert!(d.name.as_ref().unwrap().starts_with("MyDocs"));
        }
    }

    #[test]
    fn validate_document_specs_keeps_an_existing_file_as_is() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("report.pdf");
        std::fs::write(&file, b"x").unwrap();
        let spec = DocumentSpec::new(None, Some(file.clone()), None).unwrap();
        let (docs, _) = validate_document_specs(vec![spec], true).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path.as_ref().unwrap(), &file);
    }

    #[test]
    fn validate_document_specs_rejects_a_nonexistent_local_path_with_no_url() {
        let spec = DocumentSpec::new(None, Some(PathBuf::from("/does/not/exist.pdf")), None).unwrap();
        assert_eq!(
            validate_document_specs(vec![spec], true),
            Err(FreeportsConfigError::PathDoesNotExist(PathBuf::from("/does/not/exist.pdf")))
        );
    }

    /// Regression pin for the `d.is_dir()` typo bug: a URL + an existing directory path used to
    /// crash with `AttributeError`. Now it appends `report.pdf` under that directory when
    /// `SAVE_PDF` is true, exactly like the URL-less directory case's sibling logic intends.
    #[test]
    fn validate_document_specs_url_plus_directory_path_appends_report_pdf_when_save_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let url = "http://example.com/report.pdf".parse().unwrap();
        let spec = DocumentSpec::new(Some(url), Some(dir.path().to_path_buf()), None).unwrap();
        let (docs, save_pdf) = validate_document_specs(vec![spec], true).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path.as_ref().unwrap(), &dir.path().join("report.pdf"));
        assert!(save_pdf);
    }

    /// Regression pin for the `d.parent.exist()`/`d.exist()` typo bugs: a URL + a path whose
    /// parent exists but the file itself doesn't used to crash with `AttributeError`.
    #[test]
    fn validate_document_specs_url_plus_missing_file_in_existing_dir_keeps_the_spec_for_download() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("not_yet_downloaded.pdf");
        let url = "http://example.com/report.pdf".parse().unwrap();
        let spec = DocumentSpec::new(Some(url), Some(target.clone()), None).unwrap();
        let (docs, save_pdf) = validate_document_specs(vec![spec], true).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path.as_ref().unwrap(), &target);
        assert!(save_pdf, "SAVE_PDF should be untouched when the file doesn't exist yet");
    }

    #[test]
    fn validate_document_specs_url_plus_existing_file_disables_save_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("already_downloaded.pdf");
        std::fs::write(&target, b"x").unwrap();
        let url = "http://example.com/report.pdf".parse().unwrap();
        let spec = DocumentSpec::new(Some(url), Some(target.clone()), None).unwrap();
        let (docs, save_pdf) = validate_document_specs(vec![spec], true).unwrap();
        assert_eq!(docs.len(), 1);
        assert!(!save_pdf);
    }

    #[test]
    fn validate_document_specs_url_with_no_path_and_save_pdf_true_disables_it_with_a_warning() {
        let url = "http://example.com/report.pdf".parse().unwrap();
        let spec = DocumentSpec::new(Some(url), None, None).unwrap();
        let (docs, save_pdf) = validate_document_specs(vec![spec], true).unwrap();
        assert_eq!(docs.len(), 1);
        assert!(!save_pdf);
    }

    #[test_case(true; "save pdf true")]
    #[test_case(false; "save pdf false")]
    fn build_end_to_end_with_a_minimal_valid_config(initial_save_pdf: bool) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = minimal_config(dir.path());
        config.save_pdf = Some(initial_save_pdf);
        let result = FreeportsConfig::build(config).unwrap();
        assert_eq!(result.format.as_deref(), Some("my-format"));
        assert_eq!(result.target_lists, vec!["TEST".to_string()]);
        assert_eq!(result.input_reports.len(), 1);
    }

    #[test]
    fn build_rejects_missing_input_reports() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = minimal_config(dir.path());
        config.input_reports = None;
        assert_eq!(FreeportsConfig::build(config), Err(FreeportsConfigError::NoInputReports));
    }

    #[test]
    fn build_rejects_missing_target_lists() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = minimal_config(dir.path());
        config.target_lists = None;
        assert_eq!(FreeportsConfig::build(config), Err(FreeportsConfigError::MissingTargetLists));
    }

    #[test]
    fn build_rejects_missing_input_db_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = minimal_config(dir.path());
        config.input_db_path = None;
        assert_eq!(FreeportsConfig::build(config), Err(FreeportsConfigError::MissingInputDbPath));
    }

    #[test]
    fn build_rejects_missing_formats_repo_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = minimal_config(dir.path());
        config.formats_repo_path = None;
        assert_eq!(FreeportsConfig::build(config), Err(FreeportsConfigError::MissingFormatsRepoPath));
    }

    /// `minimal_config` always spreads `..PartialConfig::defaults()`, so this pins the fallback
    /// behavior directly: a genuinely bare `PartialConfig` (no `defaults()` applied at all, as
    /// `build()` would never actually see through the real `resolve_partial_config`/batch-row
    /// paths, both of which always seed from `PartialConfig::defaults()` first) still resolves
    /// `verbosity`/`n_workers`/`out_profile`/`out_flags`/`out_path`/`save_pdf` to exactly the same
    /// values `PartialConfig::defaults()` provides, rather than erroring.
    #[test]
    fn build_falls_back_to_partial_config_defaults_for_a_genuinely_bare_config() {
        let dir = tempfile::tempdir().unwrap();
        let doc = DocumentSpec::new(None, Some(dir.path().join("input.pdf")), None).unwrap();
        std::fs::write(dir.path().join("input.pdf"), b"%PDF-1.4").unwrap();
        let config = PartialConfig {
            input_reports: Some(vec![doc]),
            format: Some("my-format".to_string()),
            target_lists: Some(vec!["TEST".to_string()]),
            input_db_path: Some(dir.path().to_path_buf()),
            formats_repo_path: Some(dir.path().to_path_buf()),
            ..PartialConfig::default()
        };
        let result = FreeportsConfig::build(config).unwrap();
        let defaults = PartialConfig::defaults();
        assert_eq!(result.verbosity, defaults.verbosity.unwrap());
        assert_eq!(result.n_workers, defaults.n_workers.unwrap());
        assert_eq!(result.out_profile, defaults.out_profile.unwrap());
        assert_eq!(result.out_flags, defaults.out_flags.unwrap());
        assert_eq!(result.save_pdf, defaults.save_pdf.unwrap());
    }

    #[test]
    fn build_rejects_missing_format_when_none_detected() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = minimal_config(dir.path());
        config.format = None;
        assert_eq!(FreeportsConfig::build(config), Err(FreeportsConfigError::FormatNotSpecifiedOrDetected));
    }

    /// Minimal on-disk `formats_repo` fixture (`metadata/formats.csv` + `metadata/url_mapping.csv`)
    /// matching the schema [`crate::formats_repo::metadata::get_formats`]/[`url_to_format`]
    /// actually validate against — exercises `detect_format`'s native call into that module.
    fn formats_repo_fixture(dir: &Path, format_name_pieces: (&str, &str, &str), url_prefix: &str) {
        let (name, locale, year) = format_name_pieces;
        let metadata_dir = dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        std::fs::write(
            metadata_dir.join("formats.csv"),
            format!("Name,Locale,Year,Country,Version\n{name},{locale},{year},,\n"),
        )
        .unwrap();
        let format_name = format!("{name}-{locale}{}", &year[year.len() - 2..]);
        std::fs::write(
            metadata_dir.join("url_mapping.csv"),
            format!("Format name,Url\n{format_name},{url_prefix}\n"),
        )
        .unwrap();
    }

    #[test]
    fn detect_format_auto_detects_from_a_url_via_the_native_metadata_module() {
        let dir = tempfile::tempdir().unwrap();
        formats_repo_fixture(dir.path(), ("TestFmt", "EN", "2024"), "http://example.com/");
        let url = "http://example.com/report.pdf".parse().unwrap();
        let doc = DocumentSpec::new(Some(url), None, None).unwrap();
        let format = detect_format(None, &[doc], dir.path()).unwrap();
        assert_eq!(format, "TestFmt-EN24");
    }

    #[test]
    fn detect_format_keeps_an_explicit_format_when_no_url_matches() {
        let dir = tempfile::tempdir().unwrap();
        formats_repo_fixture(dir.path(), ("TestFmt", "EN", "2024"), "http://other.example.com/");
        let url = "http://example.com/report.pdf".parse().unwrap();
        let doc = DocumentSpec::new(Some(url), None, None).unwrap();
        let format = detect_format(Some("explicit-format".to_string()), &[doc], dir.path()).unwrap();
        assert_eq!(format, "explicit-format");
    }

    #[test]
    fn detect_format_reports_no_format_specified_or_detected_when_neither_is_available() {
        let dir = tempfile::tempdir().unwrap();
        formats_repo_fixture(dir.path(), ("TestFmt", "EN", "2024"), "http://other.example.com/");
        let url = "http://example.com/report.pdf".parse().unwrap();
        let doc = DocumentSpec::new(Some(url), None, None).unwrap();
        assert_eq!(detect_format(None, &[doc], dir.path()), Err(FreeportsConfigError::FormatNotSpecifiedOrDetected));
    }

    #[test]
    fn build_end_to_end_auto_detects_format_from_a_real_formats_repo() {
        let dir = tempfile::tempdir().unwrap();
        formats_repo_fixture(dir.path(), ("TestFmt", "EN", "2024"), "http://example.com/");
        let url = "http://example.com/report.pdf".parse().unwrap();
        let doc = DocumentSpec::new(Some(url), None, None).unwrap();
        let config = PartialConfig {
            input_reports: Some(vec![doc]),
            format: None,
            target_lists: Some(vec!["TEST".to_string()]),
            input_db_path: Some(dir.path().to_path_buf()),
            formats_repo_path: Some(dir.path().to_path_buf()),
            ..PartialConfig::defaults()
        };
        let result = FreeportsConfig::build(config).unwrap();
        assert_eq!(result.format.as_deref(), Some("TestFmt-EN24"));
    }

    #[test]
    fn build_rejects_a_batch_file_that_does_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = minimal_config(dir.path());
        config.batch_file = Some(PathBuf::from("/does/not/exist.csv"));
        assert_eq!(FreeportsConfig::build(config), Err(FreeportsConfigError::BatchFileDoesNotExist(PathBuf::from("/does/not/exist.csv"))));
    }

    #[test]
    fn build_applies_single_file_out_path_after_compress_flag_and_validation() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = minimal_config(dir.path());
        config.out_profile = Some(OutStructureMode::SingleFile);
        config.out_path = Some(dir.path().to_path_buf());
        let result = FreeportsConfig::build(config).unwrap();
        assert_eq!(result.out_path, dir.path().join("out.csv"));
    }
}
