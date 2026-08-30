//! Modalità batch: un `PartialConfig` per riga di CSV.
//!
//! `M9-implementation-plan.md` §1/§2/§3 passo 10, §0 Q3. Porta `FreeportsJobConfig.__init__`
//! (`conf_parse.py`), meno `PREFIX_OUT` (eliminato, `PLAN.md` §7/target 2: colonna `Report`
//! sempre presente, nessun prefisso separato) e con supporto multi-spec via
//! `cli::conf_parse::DOC_SPEC_SEPARATOR`, esteso alla colonna `pdf` (era già così nel
//! riferimento: `if DOC_SPEC_SEPARATOR in str(config_dict["PDF"])`, generalizzato qui a
//! *qualunque* valore di quella colonna, non solo quando contiene il separatore).
//!
//! **Scelta del test-writer sul nome/forma della colonna multi-spec** (§0 Q3 non fissa un nome
//! di colonna CSV esplicito, solo "colonna `pdf`/`report` con supporto multi-spec"): questa
//! colonna è **`pdf`**, non una nuova colonna `report` separata -- stesso nome del riferimento,
//! generalizzato. Ogni valore della cella `pdf` è spezzato su `DOC_SPEC_SEPARATOR` (anche se
//! contiene un solo elemento, senza alcun separatore) e ciascun elemento passato a
//! `DocumentSpec::parse` (grammatica completa `<url>:<path>:<name>`) -- **non** più il semplice
//! path grezzo che `pdf`/`url` rappresentano in `config_locations::file`/`env`. Combinata con la
//! colonna singolare `url` tramite lo stesso `resolve_singular_and_plural_reports` di
//! `cli::partial_config` -- **segnalato come judgment call nel resoconto del test-writer**, non
//! una lettura univoca del piano.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, thiserror::Error)]
//! pub enum BatchError {
//!     Io { path: std::path::PathBuf, source: std::io::Error },
//!     Csv { path: std::path::PathBuf, source: csv::Error },
//!     UnknownColumn { path: std::path::PathBuf, column: String },
//!     InvalidReportSpecifier { row: usize, value: String, source: crate::cli::conf_parse::DocumentSpecError },
//!     ReportsConflict { row: usize, source: crate::cli::partial_config::SourceReportsConflict },
//!     InvalidValue { row: usize, column: &'static str, value: String },
//! }
//!
//! /// Righe numerate a partire da 1 (l'intestazione non conta), stesso ordine del file.
//! /// Un CSV con la sola intestazione (zero righe dati) -> `Ok(Vec::new())`, non un errore.
//! pub fn load_jobs(path: &std::path::Path) -> Result<Vec<crate::cli::partial_config::PartialConfig>, BatchError>;
//! ```
//!
//! # Colonne riconosciute (case-insensitive, spazi/underscore equivalenti, come il riferimento)
//!
//! | colonna | campo | note |
//! |---|---|---|
//! | `url` | contribuisce (con `pdf`) allo spec singolare | |
//! | `pdf` | `reports` | multi-spec via `DOC_SPEC_SEPARATOR`, ciascun elemento con la grammatica completa |
//! | `format` | `format` | |
//! | `save pdf` | `save_pdf` | booleano |
//! | `target list` | `target_lists` | un solo elemento, il valore grezzo intero -- stessa convenzione di `FREEPORTS_TARGET_LIST` |
//!
//! Una colonna sconosciuta è un errore esplicito (`BatchError::UnknownColumn`) -- stessa scelta
//! di `config_locations::file` per coerenza nel crate, non decisa esplicitamente dal piano.
//!
//! # Combinazione `url`/`pdf` (judgment call, vedi sopra)
//!
//! La colonna `pdf` è sempre spezzata su `DOC_SPEC_SEPARATOR`, anche con un solo elemento. Se ne
//! risulta **un solo** elemento, quello è trattato come lo spec "singolare" (con `url`, se
//! presente, sovrascritto sul suo campo `url`) — non un vero elenco plurale, quindi mai in
//! conflitto con `url`. Se ne risultano **più** elementi, è un vero plurale: `url` presente
//! insieme diventa un conflitto (`resolve_singular_and_plural_reports`, §0 Q3), esattamente come
//! `config_locations::env`/`file`.

use std::path::{Path, PathBuf};

use crate::cli::conf_parse::{DOC_SPEC_SEPARATOR, DocumentSpec, DocumentSpecError};
use crate::cli::partial_config::{PartialConfig, SourceReportsConflict, resolve_singular_and_plural_reports};
use crate::core::tracing_setup::log_error;

#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    #[error("cannot read {}: {source}", path.display())]
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error("cannot parse CSV {}: {source}", path.display())]
    Csv { path: PathBuf, #[source] source: csv::Error },
    #[error("{}: unknown column {column:?}", path.display())]
    UnknownColumn { path: PathBuf, column: String },
    #[error("row {row}: invalid document specifier {value:?}: {source}")]
    InvalidReportSpecifier { row: usize, value: String, source: DocumentSpecError },
    #[error("row {row}: {source}")]
    ReportsConflict { row: usize, source: SourceReportsConflict },
    #[error("row {row}: invalid value for '{column}': {value:?}")]
    InvalidValue { row: usize, column: &'static str, value: String },
}

const KNOWN_COLUMNS: [&str; 5] = ["url", "pdf", "format", "save pdf", "target list"];

fn normalize_column(name: &str) -> String {
    name.trim().to_lowercase().replace('_', " ")
}

fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

fn parse_bool(row: usize, value: &str) -> Result<bool, BatchError> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(BatchError::InvalidValue { row, column: "save pdf", value: value.to_string() }),
    }
}

fn row_to_partial_config(row: usize, fields: &std::collections::HashMap<&str, String>) -> Result<PartialConfig, BatchError> {
    let url = fields.get("url").and_then(|v| non_empty(v)).map(str::to_string);
    let pdf = fields.get("pdf").and_then(|v| non_empty(v));

    let parts: Option<Vec<&str>> = pdf.map(|v| v.split(DOC_SPEC_SEPARATOR).collect());

    let (singular, plural) = match parts {
        None => (url.map(|u| DocumentSpec { url: Some(u), path: None, name: None }), None),
        Some(parts) if parts.len() == 1 => {
            let mut spec = DocumentSpec::parse(parts[0]).map_err(|source| BatchError::InvalidReportSpecifier {
                row,
                value: parts[0].to_string(),
                source,
            })?;
            if let Some(u) = url {
                spec.url = Some(u);
            }
            (Some(spec), None)
        }
        Some(parts) => {
            let specs: Result<Vec<DocumentSpec>, BatchError> = parts
                .iter()
                .map(|p| {
                    DocumentSpec::parse(p)
                        .map_err(|source| BatchError::InvalidReportSpecifier { row, value: p.to_string(), source })
                })
                .collect();
            (url.map(|u| DocumentSpec { url: Some(u), path: None, name: None }), Some(specs?))
        }
    };

    let reports =
        resolve_singular_and_plural_reports(singular, plural).map_err(|source| BatchError::ReportsConflict { row, source })?;

    let format = fields.get("format").and_then(|v| non_empty(v)).map(str::to_string);
    let save_pdf = fields.get("save pdf").and_then(|v| non_empty(v)).map(|v| parse_bool(row, v)).transpose()?;
    let target_lists = fields.get("target list").and_then(|v| non_empty(v)).map(|v| vec![v.to_string()]);

    Ok(PartialConfig { reports, format, save_pdf, target_lists, ..PartialConfig::default() })
}

/// Batch job dispatch: wraps [`load_jobs_impl`] in its own span (`path` is the coordinate that
/// identifies this batch operation, the closest thing to a "config source path" this module has)
/// and logs the outcome exactly once -- this is the only place every `BatchError` variant is
/// actually constructed (directly, via [`batch_csv_err`], or via [`row_to_partial_config`]/
/// [`parse_bool`]).
pub fn load_jobs(path: &Path) -> Result<Vec<PartialConfig>, BatchError> {
    let span = tracing::info_span!("batch", path = %path.display());
    let _guard = span.enter();

    let result = load_jobs_impl(path);
    match &result {
        Ok(jobs) => tracing::info!(job_count = jobs.len(), "loaded batch file"),
        Err(e) => tracing::error!(error = log_error(e), "cannot load batch file: {e}"),
    }
    result
}

/// Righe numerate a partire da 1 (l'intestazione non conta), stesso ordine del file. Un CSV con
/// la sola intestazione (zero righe dati) -> `Ok(Vec::new())`, non un errore.
fn load_jobs_impl(path: &Path) -> Result<Vec<PartialConfig>, BatchError> {
    let mut reader =
        csv::ReaderBuilder::new().has_headers(true).from_path(path).map_err(|e| batch_csv_err(path, e))?;

    let headers = reader.headers().map_err(|e| batch_csv_err(path, e))?.clone();
    let mut column_map: Vec<&'static str> = Vec::with_capacity(headers.len());
    for header in headers.iter() {
        let normalized = normalize_column(header);
        match KNOWN_COLUMNS.iter().find(|&&known| known == normalized) {
            Some(&known) => column_map.push(known),
            None => return Err(BatchError::UnknownColumn { path: path.to_path_buf(), column: header.to_string() }),
        }
    }

    let mut jobs = Vec::new();
    for (row_index, record) in reader.records().enumerate() {
        let record = record.map_err(|e| batch_csv_err(path, e))?;
        let mut fields: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
        for (i, value) in record.iter().enumerate() {
            if let Some(&column) = column_map.get(i) {
                fields.insert(column, value.to_string());
            }
        }
        jobs.push(row_to_partial_config(row_index + 1, &fields)?);
    }
    Ok(jobs)
}

fn batch_csv_err(path: &Path, source: csv::Error) -> BatchError {
    if source.is_io_error() {
        BatchError::Io { path: path.to_path_buf(), source: source.into() }
    } else {
        BatchError::Csv { path: path.to_path_buf(), source }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write_csv(dir: &std::path::Path, content: &str) -> PathBuf {
        let path = dir.join("batch.csv");
        std::fs::write(&path, content).unwrap();
        path
    }

    mod row_count {
        use super::*;

        #[test]
        fn a_csv_with_n_rows_yields_n_partial_configs_in_file_order() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format\nA-EN24\nB-EN24\nC-EN24\n");
            let jobs = load_jobs(&path).unwrap();
            assert_eq!(jobs.len(), 3);
            assert_eq!(jobs[0].format.as_deref(), Some("A-EN24"));
            assert_eq!(jobs[1].format.as_deref(), Some("B-EN24"));
            assert_eq!(jobs[2].format.as_deref(), Some("C-EN24"));
        }

        #[test]
        fn a_header_only_csv_yields_an_empty_vec_not_an_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format\n");
            let jobs = load_jobs(&path).unwrap();
            assert!(jobs.is_empty());
        }
    }

    mod column_mapping {
        use super::*;

        #[test]
        fn format_column_is_mapped() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format\nACME-EN24\n");
            let jobs = load_jobs(&path).unwrap();
            assert_eq!(jobs[0].format.as_deref(), Some("ACME-EN24"));
        }

        #[test]
        fn save_pdf_column_is_mapped_as_a_boolean() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format,save pdf\nA,false\n");
            let jobs = load_jobs(&path).unwrap();
            assert_eq!(jobs[0].save_pdf, Some(false));
        }

        #[test]
        fn target_list_column_becomes_a_single_element_list() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format,target list\nA,TEST\n");
            let jobs = load_jobs(&path).unwrap();
            assert_eq!(jobs[0].target_lists, Some(vec!["TEST".to_string()]));
        }

        #[test]
        fn an_unrecognized_column_is_a_typed_error_not_a_panic() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format,not_a_real_column\nA,1\n");
            let result = std::panic::catch_unwind(|| load_jobs(&path));
            assert!(result.is_ok(), "must not panic");
            assert!(matches!(result.unwrap(), Err(BatchError::UnknownColumn { .. })));
        }
    }

    mod pdf_column_multi_spec {
        use super::*;

        #[test]
        fn a_single_specifier_with_no_separator_still_becomes_a_one_element_reports_list() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format,pdf\nA,report.pdf\n");
            let jobs = load_jobs(&path).unwrap();
            let reports = jobs[0].reports.clone().unwrap();
            assert_eq!(reports.len(), 1);
        }

        #[test]
        fn multiple_specifiers_are_split_on_the_shared_doc_spec_separator_in_order() {
            let dir = tempfile::tempdir().unwrap();
            let content = format!(
                "format,pdf\nA,report-a.pdf{sep}report-b.pdf{sep}report-c.pdf\n",
                sep = crate::cli::conf_parse::DOC_SPEC_SEPARATOR
            );
            let path = write_csv(dir.path(), &content);
            let jobs = load_jobs(&path).unwrap();
            let reports = jobs[0].reports.clone().unwrap();
            assert_eq!(reports.len(), 3);
        }

        #[test]
        fn each_element_uses_the_full_document_spec_grammar() {
            let dir = tempfile::tempdir().unwrap();
            let content = format!(
                "format,pdf\nA,report-a.pdf:Report A{sep}report-b.pdf:Report B\n",
                sep = crate::cli::conf_parse::DOC_SPEC_SEPARATOR
            );
            let path = write_csv(dir.path(), &content);
            let jobs = load_jobs(&path).unwrap();
            let reports = jobs[0].reports.clone().unwrap();
            assert_eq!(reports[0].name.as_deref(), Some("Report A"));
            assert_eq!(reports[1].name.as_deref(), Some("Report B"));
        }

        #[test]
        fn an_invalid_element_is_a_typed_error_with_the_row_number() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format,pdf\nA,ok.pdf\nB,a:b:c:d\n");
            let result = load_jobs(&path);
            match result {
                Err(BatchError::InvalidReportSpecifier { row, .. }) => assert_eq!(row, 2),
                other => panic!("expected InvalidReportSpecifier at row 2, got {other:?}"),
            }
        }
    }

    mod url_and_pdf_conflict {
        use super::*;

        #[test]
        fn url_alone_becomes_a_single_element_reports_list() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format,url\nA,https://example.com/report.pdf\n");
            let jobs = load_jobs(&path).unwrap();
            let reports = jobs[0].reports.clone().unwrap();
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/report.pdf"));
        }

        #[test]
        fn url_and_a_separator_free_pdf_value_combine_into_one_spec() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format,url,pdf\nA,https://example.com/report.pdf,local.pdf\n");
            let jobs = load_jobs(&path).unwrap();
            let reports = jobs[0].reports.clone().unwrap();
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/report.pdf"));
        }
    }

    mod ordering_and_independence {
        use super::*;

        #[test]
        fn each_row_is_independent_a_missing_optional_column_value_leaves_the_field_none() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format,save pdf\nA,true\nB,\n");
            let jobs = load_jobs(&path).unwrap();
            assert_eq!(jobs[0].save_pdf, Some(true));
            assert_eq!(jobs[1].save_pdf, None, "an empty cell means the row doesn't set this field");
        }

        #[test]
        fn rows_may_specify_different_formats() {
            let dir = tempfile::tempdir().unwrap();
            let path = write_csv(dir.path(), "format\nA-EN24\nB-EN24\n");
            let jobs = load_jobs(&path).unwrap();
            assert_ne!(jobs[0].format, jobs[1].format);
        }
    }

    mod file_errors {
        use super::*;

        #[test]
        fn a_missing_file_is_a_typed_io_error_not_a_panic() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("does-not-exist.csv");
            let result = std::panic::catch_unwind(|| load_jobs(&path));
            assert!(result.is_ok(), "must not panic");
            assert!(matches!(result.unwrap(), Err(BatchError::Io { .. })));
        }
    }
}
