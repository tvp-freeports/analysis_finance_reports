//! `formats_mapping.csv`: quale algoritmo nominato serve ciascun segmento di ciascuna pipeline.
//!
//! Quattro colonne — `ID` e i tre segmenti — e una riga per pipe. Una cella vuota significa "questo
//! segmento non è semistructured per questo pipe", non "algoritmo senza nome".
//!
//! Porting di `semistructured/formats_mapping.rs` di `freeports_core`, senza il confine PyO3.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::super::id_format::{IdFormat, derive_format_name, derive_pipeline_name, id_matches};

/// La cartella dei file semistructured dentro il repo formati.
pub const SEMISTRUCTURED_DIR: &str = "content/algorithms/semistructured";

/// Fallimenti nella lettura di `formats_mapping.csv`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatsMappingError {
    #[error("missing formats-repository CSV file: {0}")]
    MissingCsv(PathBuf),
    #[error("{path}: malformed row at line {line}: {reason}")]
    MalformedRow { path: PathBuf, line: usize, reason: String },
    #[error("{path}: missing required column '{column}'")]
    MissingColumn { path: PathBuf, column: String },
    #[error("{path}, line {line}: ID '{id}' does not match the expected ID pattern")]
    InvalidId { path: PathBuf, line: usize, id: String },
}

/// Una riga di `formats_mapping.csv`, così come sta su disco.
#[derive(Debug, Clone, Deserialize)]
struct RawRow {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "pdf_extract")]
    pdf_extract: String,
    #[serde(rename = "text_filter")]
    text_filter: String,
    #[serde(rename = "deserialize")]
    deserialize: String,
}

/// Una riga con l'identità del pipe già derivata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingRow {
    pub format_name: String,
    pub pipeline_name: String,
    /// La posizione del pipe nel suo gruppo `(formato, pipeline)`, contata su **tutto** il file:
    /// è il `cumcount()` del riferimento, e non cambia se fra due righe dello stesso gruppo se ne
    /// intromettono di altri formati.
    pub pipe_index: u32,
    pub pdf_extract: Option<String>,
    pub text_filter: Option<String>,
    pub deserialize: Option<String>,
}

/// Una cella vuota è "nessun algoritmo", non "algoritmo senza nome".
fn cell(raw: &str) -> Option<String> {
    if raw.is_empty() { None } else { Some(raw.to_string()) }
}

/// Tutte le righe del file, nell'ordine in cui compaiono.
pub fn get_formats_mapping(formats_repo_dir: &Path) -> Result<Vec<MappingRow>, FormatsMappingError> {
    let path = formats_repo_dir.join(SEMISTRUCTURED_DIR).join("formats_mapping.csv");
    if !path.is_file() {
        return Err(FormatsMappingError::MissingCsv(path));
    }
    let mut reader = csv::Reader::from_path(&path)
        .map_err(|e| FormatsMappingError::MalformedRow { path: path.clone(), line: 0, reason: e.to_string() })?;

    let mut counters: std::collections::HashMap<(String, String), u32> = std::collections::HashMap::new();
    let mut rows = Vec::new();
    for (i, record) in reader.deserialize::<RawRow>().enumerate() {
        let line = i + 1;
        let raw = record.map_err(|e| {
            let message = e.to_string();
            if let Some(rest) = message.split("missing field `").nth(1)
                && let Some(column) = rest.split('`').next()
            {
                return FormatsMappingError::MissingColumn { path: path.clone(), column: column.to_string() };
            }
            FormatsMappingError::MalformedRow { path: path.clone(), line, reason: message }
        })?;

        if !id_matches(&raw.id, IdFormat::ExpandableNoIndex) {
            return Err(FormatsMappingError::InvalidId { path, line, id: raw.id });
        }
        let format_name = derive_format_name(&raw.id);
        // Come `mapping.csv` e a differenza di `pageclassify_overwrite.csv`: un ID senza gruppo
        // `(pipeline)` appartiene alla pipeline senza nome.
        let pipeline_name = derive_pipeline_name(&raw.id, Some("")).unwrap_or_default();

        let counter = counters.entry((format_name.clone(), pipeline_name.clone())).or_insert(0);
        let pipe_index = *counter;
        *counter += 1;

        rows.push(MappingRow {
            format_name,
            pipeline_name,
            pipe_index,
            pdf_extract: cell(&raw.pdf_extract),
            text_filter: cell(&raw.text_filter),
            deserialize: cell(&raw.deserialize),
        });
    }
    Ok(rows)
}

/// Le righe di un solo formato, nell'ordine del file.
///
/// Un formato che non compare affatto non è un errore: semplicemente non usa il livello
/// semistructured. Un file mancante o una riga malformata **di qualunque formato**, invece, lo è.
pub fn rows_for_format(formats_repo_dir: &Path, format_name: &str) -> Result<Vec<MappingRow>, FormatsMappingError> {
    Ok(get_formats_mapping(formats_repo_dir)?.into_iter().filter(|r| r.format_name == format_name).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const HEADER: &str = "ID,pdf_extract,text_filter,deserialize\n";

    fn repo(csv: &str) -> TempDir {
        let dir = TempDir::new().expect("temp dir");
        fs::create_dir_all(dir.path().join(SEMISTRUCTURED_DIR)).expect("semistructured dir");
        fs::write(dir.path().join(SEMISTRUCTURED_DIR).join("formats_mapping.csv"), csv).expect("write csv");
        dir
    }

    mod reading {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn derives_the_format_and_pipeline_of_each_row() {
            let dir = repo(&format!("{HEADER}AMUNDI-IT24(investments),standard_cost_curr,,\n"));
            let rows = get_formats_mapping(dir.path()).unwrap();
            assert_eq!(rows[0].format_name, "AMUNDI-IT24");
            assert_eq!(rows[0].pipeline_name, "investments");
            assert_eq!(rows[0].pdf_extract.as_deref(), Some("standard_cost_curr"));
        }

        #[test]
        fn an_empty_cell_means_no_algorithm_for_that_segment() {
            let dir = repo(&format!("{HEADER}A-EN24(inv),standard_cost_curr,,\n"));
            let rows = get_formats_mapping(dir.path()).unwrap();
            assert!(rows[0].text_filter.is_none() && rows[0].deserialize.is_none());
        }

        #[test]
        fn an_id_without_a_pipeline_group_belongs_to_the_unnamed_pipeline() {
            let dir = repo(&format!("{HEADER}A-EN24,x,,\n"));
            assert_eq!(get_formats_mapping(dir.path()).unwrap()[0].pipeline_name, "");
        }

        #[test]
        fn rows_of_one_group_are_numbered_from_zero() {
            let dir = repo(&format!("{HEADER}A-EN24(inv),x,,\nA-EN24(inv),y,,\n"));
            let rows = get_formats_mapping(dir.path()).unwrap();
            assert_eq!(rows.iter().map(|r| r.pipe_index).collect::<Vec<_>>(), vec![0, 1]);
        }

        #[test]
        fn interleaved_rows_of_another_format_do_not_disturb_the_numbering() {
            let dir = repo(&format!("{HEADER}A-EN24(inv),x,,\nB-EN24(inv),x,,\nA-EN24(inv),y,,\n"));
            let rows = get_formats_mapping(dir.path()).unwrap();
            assert_eq!(rows.iter().map(|r| r.pipe_index).collect::<Vec<_>>(), vec![0, 0, 1]);
        }

        #[test]
        fn each_pipeline_of_a_format_has_its_own_counter() {
            let dir = repo(&format!("{HEADER}A-EN24(inv),x,,\nA-EN24(manco),x,,\n"));
            let rows = get_formats_mapping(dir.path()).unwrap();
            assert_eq!(rows.iter().map(|r| r.pipe_index).collect::<Vec<_>>(), vec![0, 0]);
        }

        #[test]
        fn an_empty_table_declares_nothing() {
            assert!(get_formats_mapping(repo(HEADER).path()).unwrap().is_empty());
        }
    }

    mod errors {
        use super::*;

        #[test]
        fn a_missing_file_is_reported_with_its_path() {
            let dir = TempDir::new().unwrap();
            assert!(matches!(get_formats_mapping(dir.path()), Err(FormatsMappingError::MissingCsv(_))));
        }

        #[test]
        fn an_id_carrying_an_index_is_rejected_with_its_line() {
            let dir = repo(&format!("{HEADER}A-EN24(inv),x,,\nA-EN24(inv)/1,x,,\n"));
            let err = get_formats_mapping(dir.path()).unwrap_err();
            assert!(matches!(err, FormatsMappingError::InvalidId { line: 2, .. }), "{err}");
        }

        #[test]
        fn a_missing_column_names_it() {
            let dir = repo("ID,pdf_extract\nA-EN24,x\n");
            let err = get_formats_mapping(dir.path()).unwrap_err();
            assert!(matches!(err, FormatsMappingError::MissingColumn { .. }), "{err}");
        }

        #[test]
        fn a_short_row_reports_its_line() {
            let dir = repo(&format!("{HEADER}A-EN24,x,,\nA-EN24,x\n"));
            let err = get_formats_mapping(dir.path()).unwrap_err();
            assert!(matches!(err, FormatsMappingError::MalformedRow { line: 2, .. }), "{err}");
        }
    }

    mod per_format_selection {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn selects_only_the_rows_of_the_requested_format() {
            let dir = repo(&format!("{HEADER}A-EN24(inv),x,,\nB-EN24(inv),y,,\n"));
            let rows = rows_for_format(dir.path(), "A-EN24").unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].pdf_extract.as_deref(), Some("x"));
        }

        #[test]
        fn a_format_with_no_rows_is_not_an_error() {
            let dir = repo(&format!("{HEADER}B-EN24(inv),y,,\n"));
            assert!(rows_for_format(dir.path(), "A-EN24").unwrap().is_empty());
        }

        #[test]
        fn a_malformed_row_of_another_format_is_still_an_error() {
            let dir = repo(&format!("{HEADER}B-EN24(inv)/1,y,,\n"));
            assert!(rows_for_format(dir.path(), "A-EN24").is_err());
        }
    }
}
