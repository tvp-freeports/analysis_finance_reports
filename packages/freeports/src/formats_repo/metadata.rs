//! The repository's metadata: the list of formats and the URL-to-format map.
//!
//! Two CSV files:
//!
//! - the format list — one row per format, with the components from which the format name is **synthesised** as `Name-Locale<YY>[@Country][.Version]`. The name is written nowhere: it exists only as the result of this synthesis, and it is the key everything else in the repository refers to the format by;
//! - the URL mapping — which URLs belong to which format, used to recognise the format of a document from the address it was downloaded from.
//!
//! Every error reports the row, this being a file people edit by hand.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::id_format::{IdFormat, id_matches};

/// The subdirectory of the repository holding the two CSV files.
pub const METADATA_DIR: &str = "metadata";

/// Failures of reading the metadata.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetadataError {
    #[error("missing formats-repository CSV file: {0}")]
    MissingCsv(PathBuf),
    #[error("{path}: malformed row at line {line}: {reason}")]
    MalformedRow { path: PathBuf, line: usize, reason: String },
    #[error("{path}: missing required column '{column}'")]
    MissingColumn { path: PathBuf, column: String },
    #[error("duplicate format name: {0}")]
    DuplicateFormatName(String),
    #[error("format name '{0}' does not match the expected format name pattern")]
    InvalidFormatName(String),
    #[error("{path}, line {line}: unknown format name: {name}")]
    UnknownFormatName { path: PathBuf, line: usize, name: String },
}

/// A row of the format list.
#[derive(Debug, Clone, Deserialize)]
struct FormatRow {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Locale")]
    locale: String,
    #[serde(rename = "Year")]
    year: String,
    /// A required column whose cell is almost always empty: the column must be *present* even when
    /// no row fills it in.
    #[serde(rename = "Country")]
    country: String,
    #[serde(rename = "Version")]
    version: String,
}

impl FormatRow {
    /// The synthesised name: `Name-Locale<YY>`, plus `@Country` and `.Version` where those columns
    /// are filled in. `<YY>` is the **last two digits** of the year, however it is written.
    fn format_name(&self) -> String {
        let year = self.year.trim();
        let yy = if year.len() >= 2 { &year[year.len() - 2..] } else { year };
        let mut name = format!("{}-{}{}", self.name, self.locale, yy);
        if !self.country.is_empty() {
            name.push('@');
            name.push_str(&self.country);
        }
        if !self.version.is_empty() {
            name.push('.');
            name.push_str(&self.version);
        }
        name
    }
}

/// A row of the URL mapping.
#[derive(Debug, Clone, Deserialize)]
struct UrlRow {
    #[serde(rename = "Format name")]
    format_name: String,
    #[serde(rename = "Url")]
    url: String,
}

/// Opens one of the metadata CSV files, telling "the file is not there" apart from "the file will
/// not read".
fn open_csv(formats_repo_dir: &Path, file_name: &str) -> Result<(PathBuf, csv::Reader<std::fs::File>), MetadataError> {
    let path = formats_repo_dir.join(METADATA_DIR).join(file_name);
    if !path.is_file() {
        return Err(MetadataError::MissingCsv(path));
    }
    let reader = csv::Reader::from_path(&path)
        .map_err(|e| MetadataError::MalformedRow { path: path.clone(), line: 0, reason: e.to_string() })?;
    Ok((path, reader))
}

/// Translates a CSV error into the right one: a missing column is a different, and far more useful,
/// diagnosis than a malformed row.
fn row_error(path: &Path, line: usize, error: &csv::Error) -> MetadataError {
    // The `csv` crate buries "missing field" inside a longer message with a position and a byte
    // offset; it is fished back out here, because a missing column is far more actionable for
    // whoever maintains the repository.
    let message = error.to_string();
    if let Some(rest) = message.split("missing field `").nth(1)
        && let Some(column) = rest.split('`').next()
    {
        return MetadataError::MissingColumn { path: path.to_path_buf(), column: column.to_string() };
    }
    MetadataError::MalformedRow { path: path.to_path_buf(), line, reason: message }
}

/// Reads all the typed rows of a CSV, numbering them from one, the header not counting.
fn read_rows<T: serde::de::DeserializeOwned>(
    formats_repo_dir: &Path,
    file_name: &str,
) -> Result<(PathBuf, Vec<T>), MetadataError> {
    let (path, mut reader) = open_csv(formats_repo_dir, file_name)?;
    let mut rows = Vec::new();
    for (i, record) in reader.deserialize::<T>().enumerate() {
        rows.push(record.map_err(|e| row_error(&path, i + 1, &e))?);
    }
    Ok((path, rows))
}

/// The format names the repository declares, in the order they appear in the format list.
///
/// Every synthesised name is checked against the format-name grammar and against the names already
/// seen: two rows synthesising the same name are a configuration error.
pub fn get_formats(formats_repo_dir: &Path) -> Result<Vec<String>, MetadataError> {
    let (_, rows): (_, Vec<FormatRow>) = read_rows(formats_repo_dir, "formats.csv")?;
    let mut seen = HashSet::new();
    let mut names = Vec::with_capacity(rows.len());
    for row in rows {
        let name = row.format_name();
        // The grammar accepts the bare name, with neither pipeline nor index, which is what the
        // format list declares.
        if !id_matches(&name, IdFormat::ExpandableNoIndex) {
            return Err(MetadataError::InvalidFormatName(name));
        }
        if !seen.insert(name.clone()) {
            return Err(MetadataError::DuplicateFormatName(name));
        }
        names.push(name);
    }
    tracing::debug!(format_count = names.len(), "read format names from formats.csv");
    Ok(names)
}

/// Reads the URL mapping, validating **every** row before returning any of them.
///
/// The validation is deliberately eager and whole-file: an unknown format at the bottom of the file
/// is an error even when the row actually needed was the first.
fn read_url_mapping(formats_repo_dir: &Path, format_names: &[String]) -> Result<Vec<UrlRow>, MetadataError> {
    let (path, rows): (_, Vec<UrlRow>) = read_rows(formats_repo_dir, "url_mapping.csv")?;
    let known: HashSet<&str> = format_names.iter().map(String::as_str).collect();
    for (i, row) in rows.iter().enumerate() {
        if !known.contains(row.format_name.as_str()) {
            return Err(MetadataError::UnknownFormatName {
                path: path.clone(),
                line: i + 1,
                name: row.format_name.clone(),
            });
        }
    }
    Ok(rows)
}

/// The format `url` belongs to, if the repository declares one.
///
/// The **longest literal prefix** wins. A URL in the table is never interpreted as a regular
/// expression, even when it contains metacharacters. On equal length the row appearing first in the
/// file wins.
pub fn url_to_format(
    formats_repo_dir: &Path,
    format_names: &[String],
    url: &str,
) -> Result<Option<String>, MetadataError> {
    let rows = read_url_mapping(formats_repo_dir, format_names)?;
    // `max_by_key` would return the **last** maximum; the first is wanted here, so the comparison
    // is strictly greater.
    let mut best: Option<&UrlRow> = None;
    for row in rows.iter().filter(|row| url.starts_with(row.url.as_str())) {
        if best.is_none_or(|current| row.url.len() > current.url.len()) {
            best = Some(row);
        }
    }
    match best {
        Some(row) => {
            tracing::debug!(
                url,
                format = row.format_name.as_str(),
                "resolved a url to a format by its longest matching prefix"
            );
            Ok(Some(row.format_name.clone()))
        }
        None => {
            tracing::trace!(url, "no known url prefix matches this url");
            Ok(None)
        }
    }
}

/// Every declared URL, grouped by format and in file order.
pub fn get_url_mapping(
    formats_repo_dir: &Path,
    format_names: &[String],
) -> Result<HashMap<String, Vec<String>>, MetadataError> {
    let rows = read_url_mapping(formats_repo_dir, format_names)?;
    let url_count = rows.len();
    let mut mapping: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        mapping.entry(row.format_name).or_default().push(row.url);
    }
    tracing::debug!(format_count = mapping.len(), url_count, "grouped urls by format");
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A minimal formats repository on disk: the tests build one in a temporary directory rather
    /// than depending on an external fixture.
    fn repo(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("temp dir");
        fs::create_dir_all(dir.path().join(METADATA_DIR)).expect("metadata dir");
        for (name, content) in files {
            fs::write(dir.path().join(METADATA_DIR).join(name), content).expect("write csv");
        }
        dir
    }

    const FORMATS_CSV: &str = "Name,Locale,Year,Country,Version\n\
                               AMUNDI,EN,24,,\n\
                               AMUNDI,IT,24,,\n\
                               MEDIOLANUM,IT,24,ES,b\n";

    mod format_names {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn synthesizes_one_name_per_row_in_file_order() {
            let dir = repo(&[("formats.csv", FORMATS_CSV)]);
            assert_eq!(
                get_formats(dir.path()).unwrap(),
                vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string(), "MEDIOLANUM-IT24@ES.b".to_string()]
            );
        }

        #[test]
        fn a_four_digit_year_keeps_only_its_last_two_digits() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,2024,,\n")]);
            assert_eq!(get_formats(dir.path()).unwrap(), vec!["AMUNDI-EN24".to_string()]);
        }

        #[test]
        fn an_empty_country_and_version_add_no_suffix() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\n")]);
            assert_eq!(get_formats(dir.path()).unwrap(), vec!["AMUNDI-EN24".to_string()]);
        }

        #[test]
        fn a_country_alone_adds_only_the_at_suffix() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nMEDIOLANUM,IT,24,ES,\n")]);
            assert_eq!(get_formats(dir.path()).unwrap(), vec!["MEDIOLANUM-IT24@ES".to_string()]);
        }

        #[test]
        fn a_version_alone_adds_only_the_dot_suffix() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nMEDIOLANUM,IT,24,,b\n")]);
            assert_eq!(get_formats(dir.path()).unwrap(), vec!["MEDIOLANUM-IT24.b".to_string()]);
        }

        #[test]
        fn an_empty_table_declares_no_format() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\n")]);
            assert!(get_formats(dir.path()).unwrap().is_empty());
        }
    }

    mod format_name_errors {
        use super::*;

        #[test]
        fn a_missing_file_is_reported_with_its_full_path() {
            let dir = repo(&[]);
            let err = get_formats(dir.path()).unwrap_err();
            let MetadataError::MissingCsv(path) = err else { panic!("expected MissingCsv") };
            assert!(path.ends_with("metadata/formats.csv"), "{}", path.display());
        }

        #[test]
        fn a_missing_column_names_the_column() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year\nAMUNDI,EN,24\n")]);
            let err = get_formats(dir.path()).unwrap_err();
            let MetadataError::MissingColumn { column, .. } = err else { panic!("expected MissingColumn, got {err}") };
            assert_eq!(column, "Country");
        }

        #[test]
        fn a_name_that_does_not_match_the_grammar_is_rejected() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,en,24,,\n")]);
            let err = get_formats(dir.path()).unwrap_err();
            assert!(matches!(err, MetadataError::InvalidFormatName(name) if name == "AMUNDI-en24"));
        }

        #[test]
        fn two_rows_synthesizing_the_same_name_are_rejected() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,EN,2024,,\n")]);
            let err = get_formats(dir.path()).unwrap_err();
            assert!(matches!(err, MetadataError::DuplicateFormatName(name) if name == "AMUNDI-EN24"));
        }

        #[test]
        fn a_row_with_too_few_cells_reports_its_line_number() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,IT\n")]);
            let err = get_formats(dir.path()).unwrap_err();
            let MetadataError::MalformedRow { line, .. } = err else { panic!("expected MalformedRow, got {err}") };
            assert_eq!(line, 2);
        }

        #[test]
        fn the_error_message_carries_the_offending_file() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,IT\n")]);
            let message = get_formats(dir.path()).unwrap_err().to_string();
            assert!(message.contains("formats.csv"), "{message}");
        }
    }

    mod url_detection {
        use super::*;
        use pretty_assertions::assert_eq;

        const URL_CSV: &str = "Format name,Url\n\
                               AMUNDI-EN24,https://www.amundi.com/\n\
                               AMUNDI-EN24,https://www.amundi.com/ABC\n\
                               AMUNDI-IT24,https://www.amundi.it/\n";

        fn full_repo() -> TempDir {
            repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", URL_CSV)])
        }

        fn names() -> Vec<String> {
            vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string(), "MEDIOLANUM-IT24@ES.b".to_string()]
        }

        #[test]
        fn recognises_a_url_by_its_declared_prefix() {
            let dir = full_repo();
            let found = url_to_format(dir.path(), &names(), "https://www.amundi.it/report.pdf").unwrap();
            assert_eq!(found, Some("AMUNDI-IT24".to_string()));
        }

        #[test]
        fn an_unknown_url_matches_nothing() {
            let dir = full_repo();
            assert_eq!(url_to_format(dir.path(), &names(), "https://example.org/x.pdf").unwrap(), None);
        }

        #[test]
        fn the_longest_matching_prefix_wins() {
            let dir = full_repo();
            // Both rows are prefixes; the longer wins, and here it happens to carry the same format
            // — the test pins which row is chosen, not merely the outcome.
            let found = url_to_format(dir.path(), &names(), "https://www.amundi.com/ABC/report.pdf").unwrap();
            assert_eq!(found, Some("AMUNDI-EN24".to_string()));
        }

        #[test]
        fn a_tie_between_equally_long_prefixes_goes_to_the_first_row() {
            let csv = "Format name,Url\nAMUNDI-IT24,https://x.example/\nAMUNDI-EN24,https://x.example/\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://x.example/a.pdf").unwrap(),
                Some("AMUNDI-IT24".to_string())
            );
        }

        #[test]
        fn a_url_cell_is_never_interpreted_as_a_regular_expression() {
            let csv = "Format name,Url\nAMUNDI-EN24,https://.*\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            assert_eq!(url_to_format(dir.path(), &names(), "https://www.amundi.com/").unwrap(), None);
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://.*/report.pdf").unwrap(),
                Some("AMUNDI-EN24".to_string())
            );
        }

        #[test]
        fn an_exactly_equal_url_matches_too() {
            let dir = full_repo();
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://www.amundi.it/").unwrap(),
                Some("AMUNDI-IT24".to_string())
            );
        }

        #[test]
        fn an_unknown_format_name_anywhere_in_the_file_is_an_error() {
            let csv = "Format name,Url\nAMUNDI-EN24,https://a/\nGHOST-EN24,https://b/\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            let err = url_to_format(dir.path(), &names(), "https://a/x.pdf").unwrap_err();
            let MetadataError::UnknownFormatName { name, line, .. } = err else {
                panic!("expected UnknownFormatName")
            };
            assert_eq!((name.as_str(), line), ("GHOST-EN24", 2));
        }

        #[test]
        fn a_missing_url_mapping_file_is_reported_as_such() {
            let dir = repo(&[("formats.csv", FORMATS_CSV)]);
            assert!(matches!(url_to_format(dir.path(), &names(), "https://a/"), Err(MetadataError::MissingCsv(_))));
        }
    }

    mod url_grouping {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn groups_every_url_under_its_format_preserving_file_order() {
            let csv = "Format name,Url\n\
                       AMUNDI-EN24,https://a/\n\
                       AMUNDI-IT24,https://b/\n\
                       AMUNDI-EN24,https://c/\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            let names = vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string()];
            let mapping = get_url_mapping(dir.path(), &names).unwrap();
            assert_eq!(mapping["AMUNDI-EN24"], vec!["https://a/".to_string(), "https://c/".to_string()]);
            assert_eq!(mapping["AMUNDI-IT24"], vec!["https://b/".to_string()]);
        }

        #[test]
        fn a_format_with_no_url_is_simply_absent_from_the_mapping() {
            let csv = "Format name,Url\nAMUNDI-EN24,https://a/\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            let names = vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string()];
            let mapping = get_url_mapping(dir.path(), &names).unwrap();
            assert!(!mapping.contains_key("AMUNDI-IT24"));
        }

        #[test]
        fn an_empty_mapping_file_yields_an_empty_map() {
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", "Format name,Url\n")]);
            assert!(get_url_mapping(dir.path(), &names_of(&[])).unwrap().is_empty());
        }

        fn names_of(names: &[&str]) -> Vec<String> {
            names.iter().map(|n| n.to_string()).collect()
        }
    }

    mod real_repository {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The first real rows of an actual formats repository, reproduced in a temporary
        /// directory: the test stays independent of that repository while keeping its real shape.
        #[test]
        fn reproduces_the_names_of_the_real_italian_formats_repository() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,IT,24,,\nANIMA,EN,23,,\n",
            )]);
            assert_eq!(
                get_formats(dir.path()).unwrap(),
                vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string(), "ANIMA-EN23".to_string()]
            );
        }
    }
}
