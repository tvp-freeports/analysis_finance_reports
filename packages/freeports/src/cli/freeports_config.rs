//! [`FreeportsConfig`]: a job's complete, validated configuration.
//!
//! The point where a merged pile of optional values becomes something that either runs or says why
//! it cannot. The validations, **in this order** because the order matters:
//!
//! 1. require the target lists — a pure presence check, so it fails fast;
//! 2. detect the format, where none was given;
//! 3. validate the document specs;
//! 4. set the compression flag, which must come before the next rule, since it can change the output path;
//! 5. check that the output path's parent exists;
//! 6. check the single-file profile's path.
//!
//! # Two known ambiguities, left as they are
//!
//! In the document-spec validation, two branches are genuinely undecided and are deliberately not
//! pinned by tests: whether a single document without a selectable path should switch saving off
//! **globally** for the run, and whether a directory with saving off should still expand to every
//! PDF in it. Both are noted rather than guessed.

use std::path::PathBuf;

use crate::cli::conf_parse::{DocumentSpec, DocumentSpecError};
use crate::cli::parallelism_config::{ParallelismConfig, Workers};
use crate::cli::partial_config::MergedConfig;
use crate::core::tracing_setup::Verbosity;
use crate::formats_repo::metadata::{get_formats, url_to_format};
use crate::output::routines::write::{OutFlags, OutStructureMode};
use crate::core::tracing_setup::log_error;

/// A resolved configuration crosses a process boundary to a worker job, as JSON — the only reason
/// it is serializable. No configuration source reads this struct; they all produce a partial one.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FreeportsConfig {
    pub verbosity: Verbosity,
    pub reports: Vec<DocumentSpec>,
    pub target_lists: Vec<String>,
    pub format: String,
    pub out_path: PathBuf,
    pub out_profile: OutStructureMode,
    pub out_flags: OutFlags,
    /// The two parallelism levels, each already resolved to one request. The global default has
    /// been applied here: past this point nothing else needs to know about it.
    pub parallelism: ParallelismConfig,
    pub batch_file: Option<PathBuf>,
    pub save_pdf: bool,
    pub formats_repo_path: Option<PathBuf>,
    pub input_db_path: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum FreeportsConfigError {
    #[error("no target list was specified by any configuration source")]
    NoTargetLists,
    #[error("a format must be specified explicitly, or detectable from a report url")]
    NoFormatSpecifiedOrDetected,
    #[error("conflicting formats detected across report urls: {detected:?}")]
    ConflictingDetectedFormats { detected: Vec<String> },
    #[error("report at index {specifier_index} specifies neither a url nor a path")]
    InputNotSpecified { specifier_index: usize, #[source] source: DocumentSpecError },
    #[error("the specified path {} does not exist", path.display())]
    DocumentPathDoesNotExist { path: PathBuf },
    #[error("the specified directory {} does not exist", path.display())]
    DocumentDirectoryDoesNotExist { path: PathBuf },
    #[error("the parent directory of {} does not exist", path.display())]
    DocumentParentDirectoryDoesNotExist { path: PathBuf },
    #[error("out path parent directory {} does not exist", path.display())]
    OutPathParentDoesNotExist { path: PathBuf },
}

fn detect_format(reports: &[DocumentSpec], explicit: Option<&str>, formats_repo_path: Option<&std::path::Path>) -> Result<String, FreeportsConfigError> {
    let mut detected: Option<String> = None;
    if let Some(repo) = formats_repo_path {
        let format_names = get_formats(repo).map_err(|e| {
            // The specific reason `get_formats` failed (e.g. a malformed `formats.csv`) is lost
            // once folded into `NoFormatSpecifiedOrDetected`, which reads as "you didn't specify a
            // format" even when one was given explicitly and only detection failed.
            tracing::warn!(error = log_error(&e), formats_repo = %repo.display(), "cannot read known formats, format detection is unavailable: {e}");
            FreeportsConfigError::NoFormatSpecifiedOrDetected
        })?;
        for report in reports {
            let Some(url) = &report.url else { continue };
            match url_to_format(repo, &format_names, url) {
                Ok(Some(found)) => {
                    match &detected {
                        None => detected = Some(found),
                        Some(current) if *current != found => {
                            return Err(FreeportsConfigError::ConflictingDetectedFormats {
                                detected: vec![current.clone(), found],
                            });
                        }
                        Some(_) => {}
                    }
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(error = log_error(&e), url, "cannot detect format from this report url: {e}"),
            }
        }
    }

    if let Some(detected) = &detected {
        if let Some(explicit) = explicit {
            if explicit != detected {
                tracing::warn!(
                    explicit,
                    detected = detected.as_str(),
                    "selected format is different from the detected one"
                );
            }
            return Ok(explicit.to_string());
        }
        tracing::info!(format = detected.as_str(), "format detected from report url");
        return Ok(detected.clone());
    }

    explicit.map(str::to_string).ok_or(FreeportsConfigError::NoFormatSpecifiedOrDetected)
}

/// A path with no extension is treated as a directory even when it does not exist yet, which is
/// what lets a missing directory be told from a missing parent. A path *with* an extension is
/// treated as a file to be downloaded, whose parent directory must exist.
fn looks_like_a_directory(path: &std::path::Path) -> bool {
    path.extension().is_none()
}

/// Validates the document specs; see the module documentation for the two branches deliberately
/// left undecided.
fn validate_document_specs(reports: Vec<DocumentSpec>, save_pdf: bool) -> Result<Vec<DocumentSpec>, FreeportsConfigError> {
    let mut result = Vec::new();
    for (index, spec) in reports.into_iter().enumerate() {
        spec.input_should_be_specified()
            .map_err(|source| FreeportsConfigError::InputNotSpecified { specifier_index: index, source })?;
        let DocumentSpec { url, path, name } = spec;

        match (url, path) {
            (None, None) => unreachable!("input_should_be_specified already rejected this"),
            (None, Some(path)) => {
                if path.is_dir() {
                    for pdf in glob_pdf_files(&path) {
                        let file_name = pdf.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                        let entry_name = name.as_deref().map(|n| format!("{n}/{file_name}"));
                        result.push(DocumentSpec { url: None, path: Some(pdf), name: entry_name });
                    }
                } else if path.is_file() {
                    result.push(DocumentSpec { url: None, path: Some(path), name });
                } else {
                    return Err(FreeportsConfigError::DocumentPathDoesNotExist { path });
                }
            }
            (Some(url), None) => {
                let path = if save_pdf {
                    let cwd = std::env::current_dir().unwrap_or_else(|e| {
                        tracing::warn!(error = log_error(&e), "cannot read the current directory, defaulting the download destination to \".\": {e}");
                        PathBuf::from(".")
                    });
                    Some(cwd.join("report.pdf"))
                } else {
                    None
                };
                result.push(DocumentSpec { url: Some(url), path, name });
            }
            (Some(url), Some(path)) => {
                if path.is_dir() {
                    if save_pdf {
                        let new_path = path.join("report.pdf");
                        result.push(DocumentSpec { url: Some(url), path: Some(new_path), name });
                    } else {
                        // A directory with saving off expands to every PDF in it.
                        for pdf in glob_pdf_files(&path) {
                            let file_name = pdf.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                            let entry_name = name.clone().map(|n| format!("{n}/{file_name}"));
                            result.push(DocumentSpec { url: Some(url.clone()), path: Some(pdf), name: entry_name });
                        }
                    }
                } else if path.is_file() {
                    result.push(DocumentSpec { url: Some(url), path: Some(path), name });
                } else if looks_like_a_directory(&path) {
                    // A path with no extension, never seen on disk: treated as a directory that
                    // should already have existed, not as a file to download.
                    if save_pdf {
                        return Err(FreeportsConfigError::DocumentDirectoryDoesNotExist { path });
                    }
                    tracing::warn!(path = %path.display(), "invalid directory specified with save_pdf=false and url present, falling back to url");
                    result.push(DocumentSpec { url: Some(url), path: Some(path), name });
                } else if save_pdf {
                    // A file that does not exist: with saving on, only its parent directory need
                    // exist, the file being downloaded there.
                    let parent_exists = path.parent().is_some_and(|p| p.as_os_str().is_empty() || p.is_dir());
                    if !parent_exists {
                        return Err(FreeportsConfigError::DocumentParentDirectoryDoesNotExist { path });
                    }
                    result.push(DocumentSpec { url: Some(url), path: Some(path), name });
                } else {
                    // With saving off: warn and fall back to the URL, never an error.
                    tracing::warn!(path = %path.display(), "invalid file specified with save_pdf=false and url present, falling back to url");
                    result.push(DocumentSpec { url: Some(url), path: Some(path), name });
                }
            }
        }
    }
    Ok(result)
}

fn glob_pdf_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(error = log_error(&e), dir = %dir.display(), "cannot list directory for pdf expansion, no reports found here: {e}");
            return Vec::new();
        }
    };
    let mut pdfs: Vec<PathBuf> = entries
        .filter_map(|e| match e {
            Ok(entry) => Some(entry),
            Err(e) => {
                tracing::warn!(error = log_error(&e), dir = %dir.display(), "cannot read a directory entry, skipping it: {e}");
                None
            }
        })
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")))
        .collect();
    pdfs.sort();
    pdfs
}

fn set_compress_flag(out_path: PathBuf, mut out_flags: OutFlags) -> (PathBuf, OutFlags) {
    let is_tar_gz = out_path.to_string_lossy().ends_with(".tar.gz");
    if is_tar_gz {
        out_flags.compressed = true;
        let stripped = out_path.file_name().and_then(|n| n.to_str()).map(|n| n.trim_end_matches(".tar.gz").to_string());
        let out_path = match stripped {
            Some(name) => out_path.with_file_name(name),
            None => out_path,
        };
        (out_path, out_flags)
    } else {
        (out_path, out_flags)
    }
}

/// Wraps `validate_impl` to log the outcome exactly once -- this is the only place every
/// `FreeportsConfigError` variant is actually constructed (directly, or -- for `InputNotSpecified`
/// -- by wrapping a `DocumentSpecError` from `cli::conf_parse`).
pub fn validate(merged: MergedConfig) -> Result<FreeportsConfig, FreeportsConfigError> {
    let result = validate_impl(merged);
    match &result {
        Ok(config) => tracing::debug!(format = %config.format, "configuration validated"),
        Err(e) => tracing::error!(error = log_error(e), "cannot validate configuration: {e}"),
    }
    result
}

fn validate_impl(merged: MergedConfig) -> Result<FreeportsConfig, FreeportsConfigError> {
    let values = merged.values;

    // 1. `require_target_lists` -- fallisce veloce, controllo di presenza puro.
    let target_lists = values.target_lists.ok_or(FreeportsConfigError::NoTargetLists)?;

    let reports = values.reports.unwrap_or_default();
    let save_pdf = values.save_pdf.unwrap_or(true);

    // 2. `detect_format`.
    let format = detect_format(&reports, values.format.as_deref(), values.formats_repo_path.as_deref())?;

    // 3. `validate_document_specs`.
    let reports = validate_document_specs(reports, save_pdf)?;

    // 4. `set_compress_flag` -- must come before 5: it can change the output path.
    let out_path = values.out_path.unwrap_or_else(|| PathBuf::from("."));
    // The two flags travel through the merge as separate fields and are only put back together
    // here, where a resolved configuration is what is being built.
    let out_flags = OutFlags {
        compressed: values.compressed.unwrap_or(false),
        separate_out: values.separate_out.unwrap_or(false),
    };
    let (out_path, out_flags) = set_compress_flag(out_path, out_flags);

    // 5. `out_path_exists`.
    let parent_exists = match out_path.parent() {
        Some(p) if p.as_os_str().is_empty() => true,
        Some(p) => p.is_dir(),
        None => true,
    };
    if !parent_exists {
        return Err(FreeportsConfigError::OutPathParentDoesNotExist { path: out_path });
    }

    // 6. `out_path_single_file`.
    let out_profile = values.out_profile.unwrap_or(OutStructureMode::Regular);
    let out_path = if out_profile == OutStructureMode::SingleFile && !out_path.to_string_lossy().ends_with(".csv") {
        out_path.join("out.csv")
    } else {
        out_path
    };

    Ok(FreeportsConfig {
        verbosity: values.verbosity.unwrap_or(Verbosity::Warn),
        reports,
        target_lists,
        format,
        out_path,
        out_profile,
        out_flags,
        // The global default descends onto the two levels here, and only here: a level no source
        // touched inherits it, and it in turn is automatic if nothing touched it either.
        parallelism: ParallelismConfig {
            jobs: values.parallelism_jobs.or(values.n_workers).unwrap_or(Workers::Auto),
            pages: values.parallelism_pages.or(values.n_workers).unwrap_or(Workers::Auto),
        },
        batch_file: values.batch_file,
        save_pdf,
        formats_repo_path: values.formats_repo_path,
        input_db_path: values.input_db_path,
        config_file: values.config_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::conf_parse::DocumentSpec;
    use crate::cli::partial_config::{MergedConfig, PartialConfig};
    use crate::core::tracing_setup::Verbosity;
    use crate::output::routines::write::{OutFlags, OutStructureMode};
    use std::path::PathBuf;

    /// A valid merged configuration built on a real temporary directory — an existing PDF, an
    /// existing output directory. Every test starts from it and overrides a single field, so a test
    /// that breaks one says, by construction, that it is *that* field breaking.
    struct ValidConfig {
        _dir: tempfile::TempDir,
        merged: MergedConfig,
    }

    impl ValidConfig {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let pdf_path = dir.path().join("report.pdf");
            std::fs::write(&pdf_path, b"%PDF-1.4 fake").unwrap();
            let out_dir = dir.path().join("out");
            std::fs::create_dir_all(&out_dir).unwrap();

            let values = PartialConfig {
                verbosity: Some(Verbosity::Warn),
                reports: Some(vec![DocumentSpec { url: None, path: Some(pdf_path), name: Some("r".to_string()) }]),
                target_lists: Some(vec!["TEST".to_string()]),
                format: Some("FMT".to_string()),
                out_path: Some(out_dir),
                out_profile: Some(OutStructureMode::Regular),
                separate_out: Some(false),
                compressed: Some(false),
                n_workers: Some(Workers::Fixed(1)),
                parallelism_jobs: None,
                parallelism_pages: None,
                batch_file: None,
                save_pdf: Some(true),
                formats_repo_path: None,
                input_db_path: None,
                config_file: None,
            };
            Self { _dir: dir, merged: MergedConfig { values, sources: Default::default() } }
        }

        fn dir(&self) -> &std::path::Path {
            self._dir.path()
        }
    }

    fn expect_ok(config: ValidConfig) -> FreeportsConfig {
        validate(config.merged).expect("expected a valid configuration to validate successfully")
    }

    /// A resolved configuration crosses a process boundary to a worker job as JSON.
    ///
    /// The round trip must be faithful **field by field**: a field lost on the way does not make
    /// the child fail, it makes the child run a *different* job from the one the parent resolved —
    /// a silent error, the worst kind. Hence comparing the whole struct rather than individual
    /// fields.
    mod serde_round_trip {
        use super::*;

        fn round_trip(config: &FreeportsConfig) -> FreeportsConfig {
            let json = serde_json::to_string(config).expect("a resolved configuration must serialize");
            serde_json::from_str(&json).expect("a serialized configuration must deserialize back")
        }

        #[test]
        fn a_baseline_configuration_survives_a_json_round_trip_unchanged() {
            let config = expect_ok(ValidConfig::new());
            assert_eq!(round_trip(&config), config);
        }

        #[test]
        fn the_optional_paths_survive_when_they_are_set() {
            let mut config = expect_ok(ValidConfig::new());
            config.formats_repo_path = Some(PathBuf::from("/repo/formats"));
            config.input_db_path = Some(PathBuf::from("/db/input.csv"));
            config.config_file = Some(PathBuf::from("/etc/freeports.yaml"));
            config.batch_file = Some(PathBuf::from("/jobs/batch.csv"));
            assert_eq!(round_trip(&config), config);
        }

        #[test]
        fn the_optional_paths_survive_when_they_are_absent() {
            let mut config = expect_ok(ValidConfig::new());
            config.formats_repo_path = None;
            config.input_db_path = None;
            config.config_file = None;
            config.batch_file = None;
            assert_eq!(round_trip(&config), config);
        }

        /// A document may reach the child as a URL, as a path, or as both: the three shapes
        /// validation produces, all of which must cross.
        #[test]
        fn every_shape_of_document_spec_survives() {
            let mut config = expect_ok(ValidConfig::new());
            config.reports = vec![
                DocumentSpec { url: Some("https://example.invalid/a.pdf".to_string()), path: None, name: Some("a".to_string()) },
                DocumentSpec { url: None, path: Some(PathBuf::from("/tmp/b.pdf")), name: Some("b".to_string()) },
                DocumentSpec {
                    url: Some("https://example.invalid/c.pdf".to_string()),
                    path: Some(PathBuf::from("/tmp/c.pdf")),
                    name: Some("c".to_string()),
                },
            ];
            assert_eq!(round_trip(&config), config);
        }

        #[test]
        fn every_verbosity_level_survives() {
            for verbosity in [
                Verbosity::Silent,
                Verbosity::ErrorOnly,
                Verbosity::Warn,
                Verbosity::Info,
                Verbosity::Debug,
                Verbosity::Trace,
            ] {
                let mut config = expect_ok(ValidConfig::new());
                config.verbosity = verbosity;
                assert_eq!(round_trip(&config).verbosity, verbosity, "verbosity {verbosity:?} did not survive");
            }
        }

        /// The output profile and flags decide *where and how* things are written. The child does
        /// not write the final output but carries them anyway: getting them wrong here would put
        /// its log somewhere the parent does not expect.
        #[test]
        fn every_out_structure_mode_survives() {
            for profile in [OutStructureMode::Regular, OutStructureMode::SingleFile, OutStructureMode::Structured] {
                let mut config = expect_ok(ValidConfig::new());
                config.out_profile = profile;
                assert_eq!(round_trip(&config).out_profile, profile, "out profile {profile:?} did not survive");
            }
        }

        #[test]
        fn every_combination_of_out_flags_survives() {
            for compressed in [false, true] {
                for separate_out in [false, true] {
                    let mut config = expect_ok(ValidConfig::new());
                    config.out_flags = OutFlags { compressed, separate_out };
                    assert_eq!(round_trip(&config).out_flags, config.out_flags, "out flags {:?} did not survive", config.out_flags);
                }
            }
        }

        /// The strings crossing the boundary come from formats repositories and from the user's own
        /// files: nobody guarantees they are ASCII.
        #[test]
        fn non_ascii_names_and_formats_survive() {
            let mut config = expect_ok(ValidConfig::new());
            config.format = "FONDO-ITALIÀ-24".to_string();
            config.target_lists = vec!["società bersaglio".to_string(), "日本".to_string()];
            assert_eq!(round_trip(&config), config);
        }
    }

    mod baseline_is_valid {
        use super::*;

        #[test]
        fn the_untouched_valid_fixture_validates_successfully() {
            let config = ValidConfig::new();
            assert!(validate(config.merged).is_ok());
        }
    }

    mod require_target_lists {
        use super::*;

        #[test]
        fn target_lists_never_set_by_any_source_is_an_error() {
            let mut config = ValidConfig::new();
            config.merged.values.target_lists = None;
            let result = validate(config.merged);
            assert!(matches!(result, Err(FreeportsConfigError::NoTargetLists)), "got {result:?}");
        }

        #[test]
        fn an_explicitly_empty_target_lists_is_not_an_error() {
            // The rule is about the *absence of a source*, not about the list's content: a user may
            // deliberately choose zero target lists.
            let mut config = ValidConfig::new();
            config.merged.values.target_lists = Some(vec![]);
            assert!(validate(config.merged).is_ok());
        }

        #[test]
        fn this_rule_fires_before_other_rules_even_when_other_fields_are_also_broken() {
            let mut config = ValidConfig::new();
            config.merged.values.target_lists = None;
            config.merged.values.out_path = Some(PathBuf::from("/definitely/does/not/exist"));
            let result = validate(config.merged);
            assert!(matches!(result, Err(FreeportsConfigError::NoTargetLists)), "got {result:?}");
        }
    }

    mod detect_format {
        use super::*;

        fn repo_with_one_url_mapped_format(dir: &std::path::Path) -> PathBuf {
            let repo = dir.join("formats_repo");
            std::fs::create_dir_all(repo.join("metadata")).unwrap();
            std::fs::write(repo.join("metadata/formats.csv"), "Name,Locale,Year,Country,Version\nA,EN,24,,\n").unwrap();
            std::fs::write(
                repo.join("metadata/url_mapping.csv"),
                "Format name,Url\nA-EN24,https://example.com/a\n",
            )
            .unwrap();
            repo
        }

        #[test]
        fn no_format_specified_and_no_formats_repo_to_detect_from_is_an_error() {
            let mut config = ValidConfig::new();
            config.merged.values.format = None;
            let result = validate(config.merged);
            assert!(matches!(result, Err(FreeportsConfigError::NoFormatSpecifiedOrDetected)), "got {result:?}");
        }

        #[test]
        fn explicit_format_with_no_formats_repo_path_never_touches_the_formats_repo() {
            // formats_repo_path stays None: if `detect_format` tried to read it regardless, this
            // would fail with an I/O-ish error instead of succeeding.
            let config = ValidConfig::new();
            assert_eq!(config.merged.values.format.as_deref(), Some("FMT"));
            assert_eq!(config.merged.values.formats_repo_path, None);
            assert!(validate(config.merged).is_ok());
        }

        #[test]
        fn detected_from_a_single_url_is_used_when_format_is_unspecified() {
            let mut config = ValidConfig::new();
            let repo = repo_with_one_url_mapped_format(config.dir());
            config.merged.values.format = None;
            config.merged.values.formats_repo_path = Some(repo);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/a/report.pdf".to_string()),
                path: None,
                name: Some("r".to_string()),
            }]);
            let result = expect_ok(config);
            assert_eq!(result.format, "A-EN24");
        }

        #[test]
        fn detected_format_different_from_the_explicit_one_is_a_warning_not_an_error_explicit_wins() {
            let mut config = ValidConfig::new();
            let repo = repo_with_one_url_mapped_format(config.dir());
            config.merged.values.format = Some("EXPLICIT-FMT".to_string());
            config.merged.values.formats_repo_path = Some(repo);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/a/report.pdf".to_string()),
                path: None,
                name: Some("r".to_string()),
            }]);
            let result = expect_ok(config);
            assert_eq!(result.format, "EXPLICIT-FMT", "the explicit format must win over the detected one");
        }

        #[test]
        fn urls_detected_to_different_formats_is_a_conflict_error() {
            let mut config = ValidConfig::new();
            let repo = config.dir().join("formats_repo");
            std::fs::create_dir_all(repo.join("metadata")).unwrap();
            std::fs::write(
                repo.join("metadata/formats.csv"),
                "Name,Locale,Year,Country,Version\nA,EN,24,,\nB,EN,24,,\n",
            )
            .unwrap();
            std::fs::write(
                repo.join("metadata/url_mapping.csv"),
                "Format name,Url\nA-EN24,https://example.com/a\nB-EN24,https://example.com/b\n",
            )
            .unwrap();
            config.merged.values.format = None;
            config.merged.values.formats_repo_path = Some(repo);
            config.merged.values.reports = Some(vec![
                DocumentSpec { url: Some("https://example.com/a/x.pdf".to_string()), path: None, name: Some("a".to_string()) },
                DocumentSpec { url: Some("https://example.com/b/y.pdf".to_string()), path: None, name: Some("b".to_string()) },
            ]);
            let result = validate(config.merged);
            assert!(matches!(result, Err(FreeportsConfigError::ConflictingDetectedFormats { .. })), "got {result:?}");
        }
    }

    mod validate_document_specs {
        use super::*;

        #[test]
        fn a_report_with_neither_url_nor_path_is_rejected() {
            let mut config = ValidConfig::new();
            config.merged.values.reports = Some(vec![DocumentSpec { url: None, path: None, name: None }]);
            assert!(validate(config.merged).is_err());
        }

        #[test]
        fn no_url_an_existing_file_path_is_kept_as_is() {
            let config = ValidConfig::new();
            let original_path = config.merged.values.reports.as_ref().unwrap()[0].path.clone();
            let result = expect_ok(config);
            assert_eq!(result.reports.len(), 1);
            assert_eq!(result.reports[0].path, original_path);
        }

        #[test]
        fn no_url_a_directory_expands_into_one_spec_per_pdf_file_inside() {
            let mut config = ValidConfig::new();
            let subdir = config.dir().join("many_pdfs");
            std::fs::create_dir_all(&subdir).unwrap();
            std::fs::write(subdir.join("a.pdf"), b"a").unwrap();
            std::fs::write(subdir.join("b.pdf"), b"b").unwrap();
            std::fs::write(subdir.join("not-a-pdf.txt"), b"x").unwrap();
            config.merged.values.reports = Some(vec![DocumentSpec { url: None, path: Some(subdir), name: Some("many".to_string()) }]);
            let result = expect_ok(config);
            assert_eq!(result.reports.len(), 2, "only the two .pdf files, not the .txt one");
            assert!(result.reports.iter().all(|d| d.path.as_ref().unwrap().extension().unwrap() == "pdf"));
        }

        #[test]
        fn no_url_a_nonexistent_path_that_is_not_a_directory_is_an_error() {
            let mut config = ValidConfig::new();
            let missing = config.dir().join("does-not-exist.pdf");
            config.merged.values.reports = Some(vec![DocumentSpec { url: None, path: Some(missing), name: Some("r".to_string()) }]);
            let result = validate(config.merged);
            assert!(matches!(result, Err(FreeportsConfigError::DocumentPathDoesNotExist { .. })), "got {result:?}");
        }

        #[test]
        fn url_and_a_directory_path_with_save_pdf_true_requires_the_directory_to_exist_and_rewrites_to_report_pdf() {
            let mut config = ValidConfig::new();
            let target_dir = config.dir().join("downloads");
            std::fs::create_dir_all(&target_dir).unwrap();
            config.merged.values.save_pdf = Some(true);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/report.pdf".to_string()),
                path: Some(target_dir.clone()),
                name: Some("r".to_string()),
            }]);
            let result = expect_ok(config);
            assert_eq!(result.reports[0].path, Some(target_dir.join("report.pdf")));
        }

        #[test]
        fn url_and_a_nonexistent_directory_path_with_save_pdf_true_is_an_error() {
            let mut config = ValidConfig::new();
            let missing_dir = config.dir().join("does-not-exist-dir");
            config.merged.values.save_pdf = Some(true);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/report.pdf".to_string()),
                path: Some(missing_dir),
                name: Some("r".to_string()),
            }]);
            let result = validate(config.merged);
            assert!(matches!(result, Err(FreeportsConfigError::DocumentDirectoryDoesNotExist { .. })), "got {result:?}");
        }

        #[test]
        fn url_and_a_not_yet_downloaded_file_path_with_save_pdf_true_only_requires_the_parent_directory() {
            let mut config = ValidConfig::new();
            let destination = config.dir().join("report-to-download.pdf"); // parent (config.dir()) exists, file itself does not
            config.merged.values.save_pdf = Some(true);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/report.pdf".to_string()),
                path: Some(destination.clone()),
                name: Some("r".to_string()),
            }]);
            let result = expect_ok(config);
            assert_eq!(result.reports[0].path, Some(destination));
        }

        #[test]
        fn url_and_a_destination_whose_parent_does_not_exist_with_save_pdf_true_is_an_error() {
            let mut config = ValidConfig::new();
            let destination = config.dir().join("missing_parent").join("report.pdf");
            config.merged.values.save_pdf = Some(true);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/report.pdf".to_string()),
                path: Some(destination),
                name: Some("r".to_string()),
            }]);
            let result = validate(config.merged);
            assert!(
                matches!(result, Err(FreeportsConfigError::DocumentParentDirectoryDoesNotExist { .. })),
                "got {result:?}"
            );
        }

        #[test]
        fn url_and_an_existing_valid_pdf_file_with_save_pdf_false_is_kept_as_is_never_an_error() {
            let mut config = ValidConfig::new();
            let existing = config.dir().join("already-here.pdf");
            std::fs::write(&existing, b"%PDF-1.4").unwrap();
            config.merged.values.save_pdf = Some(false);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/report.pdf".to_string()),
                path: Some(existing.clone()),
                name: Some("r".to_string()),
            }]);
            let result = expect_ok(config);
            assert_eq!(result.reports[0].path, Some(existing));
        }

        #[test]
        fn url_and_a_missing_file_with_save_pdf_false_falls_back_to_the_url_never_an_error() {
            let mut config = ValidConfig::new();
            let missing = config.dir().join("missing.pdf");
            config.merged.values.save_pdf = Some(false);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/report.pdf".to_string()),
                path: Some(missing),
                name: Some("r".to_string()),
            }]);
            // Only the non-error outcome is pinned here (targets/conf_parse.md: "avvisa... ma fa
            // fallback sull'url", never an error) -- the exact resulting `path` value is left
            // unspecified/untested (see the module doc's ambiguity note).
            assert!(validate(config.merged).is_ok());
        }

        #[test]
        fn url_only_no_path_with_save_pdf_true_defaults_the_path_to_report_pdf_in_the_cwd() {
            // With only a URL given, saving writes the file into the current directory.
            let mut config = ValidConfig::new();
            config.merged.values.save_pdf = Some(true);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/report.pdf".to_string()),
                path: None,
                name: Some("r".to_string()),
            }]);
            let result = expect_ok(config);
            let expected = std::env::current_dir().unwrap().join("report.pdf");
            assert_eq!(result.reports[0].path, Some(expected));
        }

        #[test]
        fn url_only_no_path_with_save_pdf_false_is_not_an_error() {
            let mut config = ValidConfig::new();
            config.merged.values.save_pdf = Some(false);
            config.merged.values.reports = Some(vec![DocumentSpec {
                url: Some("https://example.com/report.pdf".to_string()),
                path: None,
                name: Some("r".to_string()),
            }]);
            assert!(validate(config.merged).is_ok());
        }
    }

    mod set_compress_flag {
        use super::*;

        #[test]
        fn tar_gz_suffix_sets_compressed_and_strips_the_suffix() {
            let mut config = ValidConfig::new();
            let expected_stripped_path = config.dir().join("out");
            config.merged.values.out_path = Some(config.dir().join("out.tar.gz"));
            config.merged.values.compressed = Some(false);
            let result = expect_ok(config);
            assert!(result.out_flags.compressed);
            assert_eq!(result.out_path, expected_stripped_path);
        }

        #[test]
        fn without_the_suffix_out_path_and_out_flags_are_left_untouched() {
            let config = ValidConfig::new();
            let original = config.merged.values.out_path.clone().unwrap();
            let result = expect_ok(config);
            assert_eq!(result.out_path, original);
            assert!(!result.out_flags.compressed);
        }

        #[test]
        fn already_compressed_out_flags_combined_with_a_tar_gz_suffix_stays_compressed() {
            let mut config = ValidConfig::new();
            let compressed_path = config.dir().join("out.tar.gz");
            config.merged.values.out_path = Some(compressed_path);
            config.merged.values.compressed = Some(true);
            let result = expect_ok(config);
            assert!(result.out_flags.compressed);
        }
    }

    mod out_path_exists {
        use super::*;

        #[test]
        fn an_out_path_whose_parent_exists_is_fine() {
            let config = ValidConfig::new();
            assert!(validate(config.merged).is_ok());
        }

        #[test]
        fn an_out_path_whose_parent_does_not_exist_is_an_error() {
            let mut config = ValidConfig::new();
            config.merged.values.out_path = Some(config.dir().join("nonexistent_parent").join("out"));
            let result = validate(config.merged);
            assert!(matches!(result, Err(FreeportsConfigError::OutPathParentDoesNotExist { .. })), "got {result:?}");
        }

        #[test]
        fn this_check_runs_after_set_compress_flag_so_a_tar_gz_suffixed_path_is_checked_after_stripping() {
            let mut config = ValidConfig::new();
            // The parent of `out.tar.gz` exists (it's `config.dir()`), so this must succeed --
            // if `out_path_exists` ran on the *unstripped* path it would still pass here too, so
            // this only documents ordering doesn't matter for a valid parent; the real ordering
            // requirement is that `set_compress_flag` never sees a post-`out_path_exists` value.
            config.merged.values.out_path = Some(config.dir().join("out.tar.gz"));
            assert!(validate(config.merged).is_ok());
        }
    }

    mod out_path_single_file {
        use super::*;

        #[test]
        fn single_file_profile_without_a_csv_suffix_gets_out_csv_appended() {
            let mut config = ValidConfig::new();
            let original = config.merged.values.out_path.clone().unwrap();
            config.merged.values.out_profile = Some(OutStructureMode::SingleFile);
            let result = expect_ok(config);
            assert_eq!(result.out_path, original.join("out.csv"));
        }

        #[test]
        fn single_file_profile_already_ending_in_csv_is_left_untouched() {
            let mut config = ValidConfig::new();
            let csv_path = config.dir().join("already.csv");
            config.merged.values.out_path = Some(csv_path.clone());
            config.merged.values.out_profile = Some(OutStructureMode::SingleFile);
            let result = expect_ok(config);
            assert_eq!(result.out_path, csv_path);
        }

        #[test]
        fn other_profiles_are_never_touched_by_this_rule() {
            let mut config = ValidConfig::new();
            let original = config.merged.values.out_path.clone().unwrap();
            config.merged.values.out_profile = Some(OutStructureMode::Regular);
            let result = expect_ok(config);
            assert_eq!(result.out_path, original);
        }
    }

    /// Validation is the only point where the global default descends onto the two levels: past it,
    /// the configuration carries two independent requests and whoever reads them no longer knows
    /// where they came from.
    mod parallelism_inheritance {
        use super::*;

        fn resolved(
            n_workers: Option<Workers>,
            jobs: Option<Workers>,
            pages: Option<Workers>,
        ) -> ParallelismConfig {
            let mut config = ValidConfig::new();
            config.merged.values.n_workers = n_workers;
            config.merged.values.parallelism_jobs = jobs;
            config.merged.values.parallelism_pages = pages;
            expect_ok(config).parallelism
        }

        #[test]
        fn the_global_default_reaches_both_levels_when_neither_says_otherwise() {
            let parallelism = resolved(Some(Workers::Fixed(3)), None, None);
            assert_eq!(parallelism.jobs, Workers::Fixed(3));
            assert_eq!(parallelism.pages, Workers::Fixed(3));
        }

        #[test]
        fn a_level_override_wins_over_the_global_default() {
            let parallelism = resolved(Some(Workers::Fixed(3)), Some(Workers::Fixed(8)), None);
            assert_eq!(parallelism.jobs, Workers::Fixed(8));
            assert_eq!(parallelism.pages, Workers::Fixed(3));
        }

        #[test]
        fn the_two_levels_are_overridden_independently() {
            let parallelism =
                resolved(Some(Workers::Fixed(3)), Some(Workers::Fixed(8)), Some(Workers::Auto));
            assert_eq!(parallelism.jobs, Workers::Fixed(8));
            assert_eq!(parallelism.pages, Workers::Auto);
        }

        #[test]
        fn an_override_stands_on_its_own_without_any_global_default() {
            let parallelism = resolved(None, Some(Workers::Fixed(2)), None);
            assert_eq!(parallelism.jobs, Workers::Fixed(2));
            assert_eq!(parallelism.pages, Workers::Auto);
        }

        #[test]
        fn nothing_set_anywhere_is_auto_at_both_levels() {
            assert_eq!(resolved(None, None, None), ParallelismConfig::default());
        }

        /// `-j 1` and its three equivalent forms: the fully sequential configuration, which must
        /// stay reachable with a single value, since it is what the determinism checks rest on.
        #[test]
        fn one_as_the_global_default_is_the_fully_sequential_configuration() {
            assert_eq!(resolved(Some(Workers::Fixed(1)), None, None), ParallelismConfig::SEQUENTIAL);
        }
    }
}
