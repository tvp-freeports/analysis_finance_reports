//! `FreeportsConfig`: configurazione completa e validata di un job (o di una riga di batch).
//!
//! `M9-implementation-plan.md` §1/§3 passo 9, §0 Q8. Porta le validazioni
//! `@model_validator(mode="after")` del riferimento (`conf_parse.py::FreeportsConfig`) più la
//! nuova regola di §0 Q8, in quest'ordine (rilevante, vedi sotto):
//!
//! 1. `require_target_lists` (nuova, §0 Q8) — fallisce veloce, controllo di presenza puro.
//! 2. `detect_format`
//! 3. `validate_document_specs` (`pdf_path_validation` di `targets/conf_parse.md`, **non** la
//!    versione buggata del riferimento — vedi il caveat in testa a `M9-implementation-plan.md`).
//! 4. `set_compress_flag` — deve precedere 5 (può cambiare `OUT_PATH`).
//! 5. `out_path_exists`
//! 6. `out_path_single_file`
//!
//! **Nota di scope del test-writer sulla regola 3** (`validate_document_specs`): `conf_parse.py`
//! è verificato bacato su questa funzione (`d.is_dir()`/`d.parent`/`d.exist()` chiamati su un
//! `DocumentSpec`, non su un `Path` — non esistono, righe mai eseguite con successo), e
//! `targets/conf_parse.md` descrive per esteso solo il ramo "url presente" (l'ultimo paragrafo
//! del file lo conferma: parla esplicitamente dell'intenzione di scaricare quando si specifica
//! `save_pdf`+`url`(+`path`)). Il ramo "solo path, nessun url" (espansione di una directory in
//! `*.pdf` multipli) resta quello del riferimento, non contraddetto da `conf_parse.md`. Restano
//! **genuinamente ambigue**, e quindi **non testate qui** (segnalate nel resoconto finale, non
//! indovinate silenziosamente):
//! - l'effetto collaterale del riferimento che spegne **globalmente** `SAVE_PDF` quando un
//!   singolo documento non ha un path selezionabile (`self.SAVE_PDF = False` dentro un ciclo su
//!   *tutti* i documenti — comportamento pre-multi-documento, mai riconciliato con una lista);
//! - il ramo "directory + `save_pdf=false`": se il glob `*.pdf` vada comunque espanso, o se la
//!   regola sia invece "esiste `dir/report.pdf`?" come lascia intendere la frase di
//!   `targets/conf_parse.md` sul caso directory (scritta nel contesto del ramo con url, non
//!   chiarita per il ramo senza url).
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, Clone, PartialEq)]
//! pub struct FreeportsConfig {
//!     pub verbosity: crate::core::tracing_setup::Verbosity,
//!     pub reports: Vec<crate::cli::conf_parse::DocumentSpec>,
//!     pub target_lists: Vec<String>,
//!     pub format: String,
//!     pub out_path: std::path::PathBuf,
//!     pub out_profile: crate::output::routines::write::OutStructureMode,
//!     pub out_flags: crate::output::routines::write::OutFlags,
//!     pub n_workers: usize,
//!     pub batch_file: Option<std::path::PathBuf>,
//!     pub save_pdf: bool,
//!     pub formats_repo_path: Option<std::path::PathBuf>,
//!     pub input_db_path: Option<std::path::PathBuf>,
//!     pub config_file: Option<std::path::PathBuf>,
//! }
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum FreeportsConfigError {
//!     NoTargetLists,                                            // §0 Q8
//!     NoFormatSpecifiedOrDetected,
//!     ConflictingDetectedFormats { detected: Vec<String> },
//!     InputNotSpecified { specifier_index: usize },              // da DocumentSpec::input_should_be_specified
//!     DocumentPathDoesNotExist { path: std::path::PathBuf },
//!     DocumentDirectoryDoesNotExist { path: std::path::PathBuf },
//!     DocumentParentDirectoryDoesNotExist { path: std::path::PathBuf },
//!     OutPathParentDoesNotExist { path: std::path::PathBuf },
//! }
//!
//! pub fn validate(merged: crate::cli::partial_config::MergedConfig) -> Result<FreeportsConfig, FreeportsConfigError>;
//! ```

use std::path::PathBuf;

use crate::cli::conf_parse::{DocumentSpec, DocumentSpecError};
use crate::cli::partial_config::MergedConfig;
use crate::core::tracing_setup::Verbosity;
use crate::formats_repo::metadata::{get_formats, url_to_format};
use crate::output::routines::write::{OutFlags, OutStructureMode};
use crate::core::tracing_setup::log_error;

#[derive(Debug, Clone, PartialEq)]
pub struct FreeportsConfig {
    pub verbosity: Verbosity,
    pub reports: Vec<DocumentSpec>,
    pub target_lists: Vec<String>,
    pub format: String,
    pub out_path: PathBuf,
    pub out_profile: OutStructureMode,
    pub out_flags: OutFlags,
    pub n_workers: usize,
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

/// Un percorso senza estensione è trattato come una directory anche quando non esiste ancora sul
/// disco (necessario per distinguere `DocumentDirectoryDoesNotExist` da
/// `DocumentParentDirectoryDoesNotExist` su un percorso mancante -- vedi i due casi corrispondenti
/// in `mod validate_document_specs` dei test): un percorso *con* estensione (tipicamente `.pdf`)
/// è invece trattato come un file da scaricare, la cui cartella padre deve esistere.
fn looks_like_a_directory(path: &std::path::Path) -> bool {
    path.extension().is_none()
}

/// `pdf_path_validation` di `targets/conf_parse.md` -- vedi il caveat nel doc-comment del modulo
/// sui rami genuinamente ambigui (non testati qui, segnalati nel resoconto finale).
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
                        // `PLAN.md` §1: directory + `save_pdf=false` -> espande `*.pdf` (stesso
                        // trattamento del ramo senza url).
                        for pdf in glob_pdf_files(&path) {
                            let file_name = pdf.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                            let entry_name = name.clone().map(|n| format!("{n}/{file_name}"));
                            result.push(DocumentSpec { url: Some(url.clone()), path: Some(pdf), name: entry_name });
                        }
                    }
                } else if path.is_file() {
                    result.push(DocumentSpec { url: Some(url), path: Some(path), name });
                } else if looks_like_a_directory(&path) {
                    // Un percorso senza estensione, mai visto sul disco: trattato come una
                    // directory che avrebbe dovuto esistere già (`targets/conf_parse.md`: "essa
                    // deve esistere"), non come un file da scaricare.
                    if save_pdf {
                        return Err(FreeportsConfigError::DocumentDirectoryDoesNotExist { path });
                    }
                    tracing::warn!(path = %path.display(), "invalid directory specified with save_pdf=false and url present, falling back to url");
                    result.push(DocumentSpec { url: Some(url), path: Some(path), name });
                } else if save_pdf {
                    // File non esistente: `save_pdf=true` richiede solo che la cartella padre
                    // esista (verrà scaricato lì).
                    let parent_exists = path.parent().is_some_and(|p| p.as_os_str().is_empty() || p.is_dir());
                    if !parent_exists {
                        return Err(FreeportsConfigError::DocumentParentDirectoryDoesNotExist { path });
                    }
                    result.push(DocumentSpec { url: Some(url), path: Some(path), name });
                } else {
                    // `save_pdf=false`: avvisa e fa fallback sull'url, mai un errore
                    // (`targets/conf_parse.md`).
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

/// Wraps [`validate_impl`] to log the outcome exactly once -- this is the only place every
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

    // 4. `set_compress_flag` -- deve precedere 5 (può cambiare `OUT_PATH`).
    let out_path = values.out_path.unwrap_or_else(|| PathBuf::from("."));
    let out_flags = values.out_flags.unwrap_or_default();
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
        n_workers: values.n_workers.unwrap_or(1),
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

    /// Un `MergedConfig` valido di base, costruito su un `TempDir` reale (un pdf esistente, una
    /// directory di output esistente): ogni test parte da qui e sovrascrive un solo campo, così un
    /// test che rompe `out_path` dice, per costruzione, che è *quello* a rompersi.
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
                out_flags: Some(OutFlags::default()),
                n_workers: Some(1),
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
            // §0 Q8: the rule is about *absence of a source*, not about the list's content -- a
            // user may deliberately choose zero target lists.
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
            // targets/conf_parse.md, last paragraph: "se si mette solo url save_pdf salva un file
            // report.pdf nella cartella corrente".
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
            config.merged.values.out_flags = Some(OutFlags::default());
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
            config.merged.values.out_flags = Some(OutFlags { compressed: true, ..OutFlags::default() });
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
}
