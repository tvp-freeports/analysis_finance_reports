use std::collections::BTreeMap;

use super::super::conf_parse::{
    DocumentSpecError,
    ConfigError,
    DocumentSpec,
    parse_bool_alias
};
use super::super::partial_config::PartialConfig;


pub const JOB_DOCUMENT_SEPARATOR: char = ';';

#[derive(Debug, Clone, PartialEq)]
pub enum JobConfigError {
    UnknownColumn(String),
    MissingFormat,
    InvalidField { column: &'static str, source: ConfigError },
}

impl std::fmt::Display for JobConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobConfigError::UnknownColumn(name) => write!(f, "unknown batch file column `{name}`"),
            JobConfigError::MissingFormat => write!(f, "batch file row is missing a required `format` column"),
            JobConfigError::InvalidField { column, source } => write!(f, "invalid value in column `{column}`: {source}"),
        }
    }
}

impl std::error::Error for JobConfigError {}

fn parse_documents(cell: &str, column: &'static str) -> Result<Vec<DocumentSpec>, JobConfigError> {
    cell.split(JOB_DOCUMENT_SEPARATOR)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<DocumentSpec>().map_err(|source: DocumentSpecError| JobConfigError::InvalidField { column, source: source.into() }))
        .collect()
}

fn parse_bool(value: &str, column: &'static str) -> Result<bool, JobConfigError> {
    parse_bool_alias(value).map_err(|source| JobConfigError::InvalidField { column, source })
}

pub fn parse_row(row: &BTreeMap<String, String>) -> Result<PartialConfig, JobConfigError> {
    let mut normalized: BTreeMap<String, &str> = BTreeMap::new();
    for (raw_key, value) in row {
        let key = raw_key.trim().to_lowercase();
        if value.trim().is_empty() {
            continue;
        }
        match key.as_str() {
            "input" | "format" | "save pdf" | "target list" => {
                normalized.insert(key, value.as_str());
            }
            other => return Err(JobConfigError::UnknownColumn(other.to_string())),
        }
    }

    let documents = normalized.get("input").map(|v| parse_documents(v, "input")).transpose()?;

    let format = normalized
        .get("format")
        .map(|v| v.trim().to_string())
        .ok_or(JobConfigError::MissingFormat)?;
    let save_pdf = normalized.get("save pdf").map(|v| parse_bool(v, "save pdf")).transpose()?;
    let target_lists = normalized.get("target list").map(|v| vec![v.trim().to_string()]);

    Ok(PartialConfig {
        input_reports: documents,
        format: Some(format),
        save_pdf,
        target_lists,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn row(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn minimal_row_needs_only_format() {
        let config = parse_row(&row(&[("format", "my-format")])).unwrap();
        assert_eq!(config.format, Some("my-format".to_string()));
        assert_eq!(config.input_reports, None);
    }

    #[test]
    fn missing_format_is_an_error() {
        assert_eq!(parse_row(&row(&[("input", "http://example.com/a.pdf")])), Err(JobConfigError::MissingFormat));
    }

    #[test]
    fn input_column_becomes_input_reports() {
        let config = parse_row(&row(&[("format", "f"), ("input", "http://example.com/a.pdf")])).unwrap();
        let docs = config.input_reports.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].url.as_ref().unwrap().to_string(), "http://example.com/a.pdf");
    }

    #[test]
    fn input_column_supports_a_full_document_spec() {
        let config = parse_row(&row(&[("format", "f"), ("input", "http://example.com/a.pdf|local.pdf|MyName")])).unwrap();
        let docs = config.input_reports.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].name.as_deref(), Some("MyName"));
    }

    #[test]
    fn input_column_splits_multiple_documents_on_semicolon() {
        let config = parse_row(&row(&[("format", "f"), ("input", "a.pdf;b.pdf;c.pdf")])).unwrap();
        let docs = config.input_reports.unwrap();
        assert_eq!(docs.len(), 3);
        assert!(docs[0].path.as_ref().unwrap().ends_with("a.pdf"));
        assert!(docs[1].path.as_ref().unwrap().ends_with("b.pdf"));
        assert!(docs[2].path.as_ref().unwrap().ends_with("c.pdf"));
    }

    #[test]
    fn save_pdf_and_target_list_are_parsed() {
        let config = parse_row(&row(&[("format", "f"), ("save pdf", "no"), ("target list", "TEST")])).unwrap();
        assert_eq!(config.save_pdf, Some(false));
        assert_eq!(config.target_lists, Some(vec!["TEST".to_string()]));
    }

    #[test]
    fn column_names_are_matched_case_insensitively_and_trimmed() {
        let config = parse_row(&row(&[(" FORMAT ", "f"), ("  Save PDF  ", "yes")])).unwrap();
        assert_eq!(config.format, Some("f".to_string()));
        assert_eq!(config.save_pdf, Some(true));
    }

    #[test]
    fn empty_cells_are_treated_as_not_given() {
        let config = parse_row(&row(&[("format", "f"), ("input", ""), ("target list", "")])).unwrap();
        assert_eq!(config.input_reports, None);
        assert_eq!(config.target_lists, None);
    }

    #[test]
    fn unknown_column_is_a_clean_error_not_a_crash() {
        assert_eq!(
            parse_row(&row(&[("format", "f"), ("report", "MyPrefix")])),
            Err(JobConfigError::UnknownColumn("report".to_string()))
        );
        assert_eq!(
            parse_row(&row(&[("format", "f"), ("prefix out", "MyPrefix")])),
            Err(JobConfigError::UnknownColumn("prefix out".to_string()))
        );
        assert_eq!(
            parse_row(&row(&[("format", "f"), ("url", "http://example.com/a.pdf")])),
            Err(JobConfigError::UnknownColumn("url".to_string()))
        );
        assert_eq!(
            parse_row(&row(&[("format", "f"), ("pdf", "a.pdf")])),
            Err(JobConfigError::UnknownColumn("pdf".to_string()))
        );
    }

    #[test]
    fn invalid_document_spec_is_a_clean_error() {
        assert!(matches!(
            parse_row(&row(&[("format", "f"), ("input", "a|b|c")])),
            Err(JobConfigError::InvalidField { column: "input", .. })
        ));
    }

    #[test]
    fn invalid_save_pdf_carries_the_shared_config_error() {
        assert_eq!(
            parse_row(&row(&[("format", "f"), ("save pdf", "maybe")])),
            Err(JobConfigError::InvalidField { column: "save pdf", source: ConfigError::InvalidBool("maybe".to_string()) })
        );
    }
}
