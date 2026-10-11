//! The repository's metadata: the list of formats and the URL-to-format map.
//!
//! Two CSV files:
//!
//! - the format list — one row per format, with the components from which the format name is **synthesised** as `Name-Locale<YY>[@Country][.Version]`. The name is written nowhere: it exists only as the result of this synthesis, and it is the key everything else in the repository refers to the format by;
//! - the URL mapping — which URLs belong to which format, used to recognise the format of a document from the address it was downloaded from. Its `Format name` column takes a partial format pattern too (`MEDIOLANUM-IT24.*`; §1 and §7 of `agent-memory/SPEC-document-classification.md`, phase 9), with the YAML's rules: a complete name must be declared, a partial must be refined by some declared format, and `*` as entity is not allowed.
//!
//! Every error reports the row, this being a file people edit by hand.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::format_pattern::{FormatPattern, FormatPatternError};
use super::id_format::{IdFormat, id_matches};

/// The subdirectory of the repository holding the two CSV files.
pub const METADATA_DIR: &str = "metadata";

/// Failures of reading the metadata.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetadataError {
    #[error("missing formats-repository CSV file: {0}")]
    MissingCsv(PathBuf),
    #[error("{path}: malformed row at line {line}: {reason}")]
    MalformedRow {
        path: PathBuf,
        line: usize,
        reason: String,
    },
    #[error("{path}: missing required column '{column}'")]
    MissingColumn { path: PathBuf, column: String },
    #[error("duplicate format name: {0}")]
    DuplicateFormatName(String),
    #[error("format name '{0}' does not match the expected format name pattern")]
    InvalidFormatName(String),
    /// A synthesised name containing `*`, which is reserved for partial format patterns and
    /// templates (`MEDIOLANUM-IT24.*`, `*-EN23`): a format declared that way would be
    /// indistinguishable from a partial.
    #[error("format name '{0}' contains '*', which is reserved for partial format patterns")]
    WildcardInFormatName(String),
    /// A complete name in the URL mapping that `formats.csv` does not declare (§1.3: also one
    /// that only exists with variants or locations).
    #[error("{path}, line {line}: unknown format name: {name}")]
    UnknownFormatName {
        path: PathBuf,
        line: usize,
        name: String,
    },
    /// A partial in the URL mapping that no format of `formats.csv` refines.
    #[error("{path}, line {line}: no format refines the partial format name: {name}")]
    PartialMatchesNoFormat {
        path: PathBuf,
        line: usize,
        name: String,
    },
    /// A `Format name` cell that is not a format pattern — `*` as entity included: a template is
    /// a YAML step, not a value.
    #[error("{path}, line {line}: not a format pattern: {name}: {reason}")]
    InvalidFormatPattern {
        path: PathBuf,
        line: usize,
        name: String,
        reason: String,
    },
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
        let yy = if year.len() >= 2 {
            &year[year.len() - 2..]
        } else {
            year
        };
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
fn open_csv(
    formats_repo_dir: &Path,
    file_name: &str,
) -> Result<(PathBuf, csv::Reader<std::fs::File>), MetadataError> {
    let path = formats_repo_dir.join(METADATA_DIR).join(file_name);
    if !path.is_file() {
        return Err(MetadataError::MissingCsv(path));
    }
    let reader = csv::Reader::from_path(&path).map_err(|e| MetadataError::MalformedRow {
        path: path.clone(),
        line: 0,
        reason: e.to_string(),
    })?;
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
        return MetadataError::MissingColumn {
            path: path.to_path_buf(),
            column: column.to_string(),
        };
    }
    MetadataError::MalformedRow {
        path: path.to_path_buf(),
        line,
        reason: message,
    }
}

/// Reads all the typed rows of a CSV, numbering them from one, the header not counting.
fn read_rows<T: serde::de::DeserializeOwned>(
    formats_repo_dir: &Path,
    file_name: &str,
) -> Result<(PathBuf, Vec<T>), MetadataError> {
    let (path, mut reader) = open_csv(formats_repo_dir, file_name)?;
    let headers = reader.headers().map_err(|e| MetadataError::MalformedRow {
        path: path.clone(),
        line: 0,
        reason: e.to_string(),
    })?;
    let known = super::csv_columns::declared::<T>();
    if let Some(column) = super::csv_columns::first_unknown(headers.iter(), known) {
        return Err(MetadataError::MalformedRow {
            path: path.clone(),
            line: 0,
            reason: super::csv_columns::unknown_column_message(column, known),
        });
    }
    let mut rows = Vec::new();
    for (i, record) in reader.deserialize::<T>().enumerate() {
        rows.push(record.map_err(|e| row_error(&path, i + 1, &e))?);
    }
    Ok((path, rows))
}

/// The format names the repository declares, in the order they appear in the format list.
///
/// Every synthesised name is checked for the `*` reserved to partial patterns, against the
/// format-name grammar and against the names already seen: two rows synthesising the same name are
/// a configuration error.
pub fn get_formats(formats_repo_dir: &Path) -> Result<Vec<String>, MetadataError> {
    let (_, rows): (_, Vec<FormatRow>) = read_rows(formats_repo_dir, "formats.csv")?;
    let mut seen = HashSet::new();
    let mut names = Vec::with_capacity(rows.len());
    for row in rows {
        let name = row.format_name();
        // Checked before the grammar, which accepts some names with `*` and rejects others: either
        // way the reserved wildcard is the more precise diagnosis.
        if name.contains('*') {
            return Err(MetadataError::WildcardInFormatName(name));
        }
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
    tracing::debug!(
        format_count = names.len(),
        "read format names from formats.csv"
    );
    Ok(names)
}

/// A row of the URL mapping, with its format read as a pattern.
struct UrlEntry {
    row: UrlRow,
    pattern: FormatPattern,
}

/// Reads the URL mapping, validating **every** row before returning any of them.
///
/// The validation is deliberately eager and whole-file: a format that cannot exist at the bottom of
/// the file is an error even when the row actually needed was the first. A complete name must be
/// declared, a partial must be refined by some declared format (§1.3 and error 1 of §5, as for the
/// YAML).
fn read_url_mapping(
    formats_repo_dir: &Path,
    format_names: &[String],
) -> Result<Vec<UrlEntry>, MetadataError> {
    let (path, rows): (_, Vec<UrlRow>) = read_rows(formats_repo_dir, "url_mapping.csv")?;
    let mut entries = Vec::with_capacity(rows.len());
    for (i, row) in rows.into_iter().enumerate() {
        let line = i + 1;
        let name = row.format_name.clone();
        let pattern: FormatPattern =
            name.parse().map_err(
                |e: FormatPatternError| MetadataError::InvalidFormatPattern {
                    path: path.clone(),
                    line,
                    name: name.clone(),
                    reason: e.reason,
                },
            )?;
        if pattern.expand(format_names).is_empty() {
            let path = path.clone();
            return Err(match pattern.is_complete() {
                true => MetadataError::UnknownFormatName { path, line, name },
                false => MetadataError::PartialMatchesNoFormat { path, line, name },
            });
        }
        entries.push(UrlEntry { row, pattern });
    }
    Ok(entries)
}

/// The format `url` belongs to, possibly partial, if the repository declares one.
///
/// Among the rows whose URL is a prefix of `url` (D3, §7): the **longest literal prefix** wins; on
/// equal length the **more specific** format ([`FormatPattern::specificity`]); on equal specificity
/// too, the row appearing first in the file. A URL in the table is never interpreted as a regular
/// expression, even when it contains metacharacters.
pub fn url_to_format(
    formats_repo_dir: &Path,
    format_names: &[String],
    url: &str,
) -> Result<Option<FormatPattern>, MetadataError> {
    let entries = read_url_mapping(formats_repo_dir, format_names)?;
    let rank = |entry: &UrlEntry| (entry.row.url.len(), entry.pattern.specificity());
    // `max_by_key` would return the **last** maximum; the first is wanted here, so the comparison
    // is strictly greater.
    let mut best: Option<&UrlEntry> = None;
    for entry in entries
        .iter()
        .filter(|entry| url.starts_with(entry.row.url.as_str()))
    {
        if best.is_none_or(|current| rank(entry) > rank(current)) {
            best = Some(entry);
        }
    }
    match best {
        Some(entry) => {
            tracing::debug!(
                url,
                format = %entry.pattern,
                prefix = entry.row.url.as_str(),
                "resolved a url to a format by its longest matching prefix"
            );
            Ok(Some(entry.pattern.clone()))
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
    let entries = read_url_mapping(formats_repo_dir, format_names)?;
    let url_count = entries.len();
    let mut mapping: HashMap<String, Vec<String>> = HashMap::new();
    for UrlEntry { row, .. } in entries {
        mapping.entry(row.format_name).or_default().push(row.url);
    }
    tracing::debug!(
        format_count = mapping.len(),
        url_count,
        "grouped urls by format"
    );
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

    fn p(text: &str) -> FormatPattern {
        text.parse().expect("fixture pattern must be valid")
    }

    mod format_names {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn synthesizes_one_name_per_row_in_file_order() {
            let dir = repo(&[("formats.csv", FORMATS_CSV)]);
            assert_eq!(
                get_formats(dir.path()).unwrap(),
                vec![
                    "AMUNDI-EN24".to_string(),
                    "AMUNDI-IT24".to_string(),
                    "MEDIOLANUM-IT24@ES.b".to_string()
                ]
            );
        }

        #[test]
        fn a_four_digit_year_keeps_only_its_last_two_digits() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nAMUNDI,EN,2024,,\n",
            )]);
            assert_eq!(
                get_formats(dir.path()).unwrap(),
                vec!["AMUNDI-EN24".to_string()]
            );
        }

        #[test]
        fn an_empty_country_and_version_add_no_suffix() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\n",
            )]);
            assert_eq!(
                get_formats(dir.path()).unwrap(),
                vec!["AMUNDI-EN24".to_string()]
            );
        }

        #[test]
        fn a_country_alone_adds_only_the_at_suffix() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nMEDIOLANUM,IT,24,ES,\n",
            )]);
            assert_eq!(
                get_formats(dir.path()).unwrap(),
                vec!["MEDIOLANUM-IT24@ES".to_string()]
            );
        }

        #[test]
        fn a_version_alone_adds_only_the_dot_suffix() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nMEDIOLANUM,IT,24,,b\n",
            )]);
            assert_eq!(
                get_formats(dir.path()).unwrap(),
                vec!["MEDIOLANUM-IT24.b".to_string()]
            );
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
            let MetadataError::MissingCsv(path) = err else {
                panic!("expected MissingCsv")
            };
            assert!(path.ends_with("metadata/formats.csv"), "{}", path.display());
        }

        #[test]
        fn a_missing_column_names_the_column() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year\nAMUNDI,EN,24\n")]);
            let err = get_formats(dir.path()).unwrap_err();
            let MetadataError::MissingColumn { column, .. } = err else {
                panic!("expected MissingColumn, got {err}")
            };
            assert_eq!(column, "Country");
        }

        #[test]
        fn a_name_that_does_not_match_the_grammar_is_rejected() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nAMUNDI,en,24,,\n",
            )]);
            let err = get_formats(dir.path()).unwrap_err();
            assert!(matches!(err, MetadataError::InvalidFormatName(name) if name == "AMUNDI-en24"));
        }

        /// R11: `*` is reserved for partial patterns and templates, wherever it lands in the name.
        mod reserved_wildcard {
            use super::*;

            fn wildcard_error(row: &str) -> MetadataError {
                let dir = repo(&[(
                    "formats.csv",
                    &format!("Name,Locale,Year,Country,Version\n{row}\n"),
                )]);
                get_formats(dir.path()).unwrap_err()
            }

            #[test]
            fn a_star_as_the_whole_name_column_is_rejected() {
                // `*-EN23` matches today's grammar, whose entity opens with `.+`.
                let err = wildcard_error("*,EN,23,,");
                assert!(
                    matches!(&err, MetadataError::WildcardInFormatName(name) if name == "*-EN23"),
                    "{err}"
                );
            }

            #[test]
            fn a_star_inside_the_name_column_is_rejected() {
                let err = wildcard_error("MEDIO*LANUM,IT,24,,");
                assert!(
                    matches!(&err, MetadataError::WildcardInFormatName(name) if name == "MEDIO*LANUM-IT24"),
                    "{err}"
                );
            }

            #[test]
            fn a_star_as_the_version_is_rejected() {
                // `X-EN23.*` matches today's grammar, whose variant is `[^./]+`.
                let err = wildcard_error("X,EN,23,,*");
                assert!(
                    matches!(&err, MetadataError::WildcardInFormatName(name) if name == "X-EN23.*"),
                    "{err}"
                );
            }

            #[test]
            fn a_star_as_the_country_is_rejected_as_a_wildcard_too() {
                // `X-EN23@*` already fails the grammar; the more precise diagnosis wins.
                let err = wildcard_error("X,EN,23,*,");
                assert!(
                    matches!(&err, MetadataError::WildcardInFormatName(name) if name == "X-EN23@*"),
                    "{err}"
                );
            }

            #[test]
            fn the_message_says_the_star_is_reserved() {
                let message = wildcard_error("*,EN,23,,").to_string();
                assert!(
                    message.contains("*-EN23") && message.contains("'*'"),
                    "{message}"
                );
            }

            #[test]
            fn a_name_without_a_star_is_unaffected() {
                let dir = repo(&[(
                    "formats.csv",
                    "Name,Locale,Year,Country,Version\nX,EN,23,LUX,A\n",
                )]);
                assert_eq!(
                    get_formats(dir.path()).unwrap(),
                    vec!["X-EN23@LUX.A".to_string()]
                );
            }
        }

        #[test]
        fn two_rows_synthesizing_the_same_name_are_rejected() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,EN,2024,,\n",
            )]);
            let err = get_formats(dir.path()).unwrap_err();
            assert!(
                matches!(err, MetadataError::DuplicateFormatName(name) if name == "AMUNDI-EN24")
            );
        }

        #[test]
        fn a_row_with_too_few_cells_reports_its_line_number() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,IT\n",
            )]);
            let err = get_formats(dir.path()).unwrap_err();
            let MetadataError::MalformedRow { line, .. } = err else {
                panic!("expected MalformedRow, got {err}")
            };
            assert_eq!(line, 2);
        }

        #[test]
        fn the_error_message_carries_the_offending_file() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,IT\n",
            )]);
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
            vec![
                "AMUNDI-EN24".to_string(),
                "AMUNDI-IT24".to_string(),
                "MEDIOLANUM-IT24@ES.b".to_string(),
            ]
        }

        #[test]
        fn recognises_a_url_by_its_declared_prefix() {
            let dir = full_repo();
            let found =
                url_to_format(dir.path(), &names(), "https://www.amundi.it/report.pdf").unwrap();
            assert_eq!(found, Some(p("AMUNDI-IT24")));
        }

        #[test]
        fn an_unknown_url_matches_nothing() {
            let dir = full_repo();
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://example.org/x.pdf").unwrap(),
                None
            );
        }

        #[test]
        fn the_longest_matching_prefix_wins() {
            let dir = full_repo();
            // Both rows are prefixes; the longer wins, and here it happens to carry the same format
            // — the test pins which row is chosen, not merely the outcome.
            let found = url_to_format(
                dir.path(),
                &names(),
                "https://www.amundi.com/ABC/report.pdf",
            )
            .unwrap();
            assert_eq!(found, Some(p("AMUNDI-EN24")));
        }

        #[test]
        fn a_tie_between_equally_long_prefixes_goes_to_the_first_row() {
            let csv =
                "Format name,Url\nAMUNDI-IT24,https://x.example/\nAMUNDI-EN24,https://x.example/\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://x.example/a.pdf").unwrap(),
                Some(p("AMUNDI-IT24"))
            );
        }

        #[test]
        fn a_url_cell_is_never_interpreted_as_a_regular_expression() {
            let csv = "Format name,Url\nAMUNDI-EN24,https://.*\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://www.amundi.com/").unwrap(),
                None
            );
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://.*/report.pdf").unwrap(),
                Some(p("AMUNDI-EN24"))
            );
        }

        #[test]
        fn an_exactly_equal_url_matches_too() {
            let dir = full_repo();
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://www.amundi.it/").unwrap(),
                Some(p("AMUNDI-IT24"))
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
            assert!(matches!(
                url_to_format(dir.path(), &names(), "https://a/"),
                Err(MetadataError::MissingCsv(_))
            ));
        }
    }

    /// Phase 9, §7 and D3: the `Format name` column takes partial patterns, with the YAML's
    /// rules; among the matching rows the longest prefix wins, then the more specific format, then
    /// the first row.
    mod url_partials {
        use super::*;

        const PARTIALS_FORMATS_CSV: &str = "Name,Locale,Year,Country,Version\n\
                                            AMUNDI,EN,24,,\n\
                                            AMUNDI,IT,24,,\n\
                                            MEDIOLANUM,IT,24,,A\n\
                                            MEDIOLANUM,IT,24,,B\n\
                                            MEDIOLANUM,ES,24,,A\n\
                                            MEDIOLANUM,ES,21,,\n\
                                            FIDEURAM,IT,23,LUX,\n";

        fn with_urls(url_csv: &str) -> TempDir {
            repo(&[
                ("formats.csv", PARTIALS_FORMATS_CSV),
                ("url_mapping.csv", &format!("Format name,Url\n{url_csv}")),
            ])
        }

        fn found(dir: &TempDir, url: &str) -> Option<FormatPattern> {
            let names = get_formats(dir.path()).unwrap();
            url_to_format(dir.path(), &names, url).unwrap()
        }

        fn refused(dir: &TempDir, url: &str) -> MetadataError {
            let names = get_formats(dir.path()).unwrap();
            url_to_format(dir.path(), &names, url).unwrap_err()
        }

        mod accepted {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn an_open_variant_is_returned_as_the_partial() {
                let dir = with_urls("MEDIOLANUM-IT24.*,https://m.it/\n");
                assert_eq!(
                    found(&dir, "https://m.it/r.pdf"),
                    Some(p("MEDIOLANUM-IT24.*"))
                );
            }

            #[test]
            fn an_entity_alone_is_returned_as_the_partial() {
                let dir = with_urls("MEDIOLANUM,https://m.com/\n");
                assert_eq!(found(&dir, "https://m.com/r.pdf"), Some(p("MEDIOLANUM")));
            }

            #[test]
            fn a_language_alone_is_returned_as_the_partial() {
                let dir = with_urls("MEDIOLANUM-ES,https://m.es/\n");
                assert_eq!(found(&dir, "https://m.es/r.pdf"), Some(p("MEDIOLANUM-ES")));
            }

            #[test]
            fn an_open_location_is_returned_as_the_partial() {
                let dir = with_urls("FIDEURAM-IT23@*,https://f.lu/\n");
                assert_eq!(
                    found(&dir, "https://f.lu/r.pdf"),
                    Some(p("FIDEURAM-IT23@*"))
                );
            }

            #[test]
            fn a_complete_row_is_still_returned_complete() {
                let dir = with_urls("AMUNDI-EN24,https://a.com/\n");
                let format = found(&dir, "https://a.com/r.pdf").expect("the row matches");
                assert_eq!(format, p("AMUNDI-EN24"));
                assert!(format.is_complete());
            }

            #[test]
            fn partial_and_complete_rows_live_together() {
                let dir =
                    with_urls("AMUNDI-EN24,https://a.com/\nMEDIOLANUM-IT24.*,https://m.it/\n");
                assert_eq!(found(&dir, "https://a.com/r.pdf"), Some(p("AMUNDI-EN24")));
                assert_eq!(
                    found(&dir, "https://m.it/r.pdf"),
                    Some(p("MEDIOLANUM-IT24.*"))
                );
            }
        }

        /// The YAML's error 1 (§5.3): what cannot exist is refused, eagerly, on every row.
        mod rejected {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn a_partial_no_format_refines_is_an_error_naming_its_line_even_when_another_row_matches()
             {
                let dir = with_urls("AMUNDI-EN24,https://a.com/\nMEDIOLANUM-FR,https://m.fr/\n");
                let err = refused(&dir, "https://a.com/r.pdf");
                let MetadataError::PartialMatchesNoFormat { name, line, .. } = err else {
                    panic!("expected PartialMatchesNoFormat, got {err:?}")
                };
                assert_eq!((name.as_str(), line), ("MEDIOLANUM-FR", 2));
            }

            #[test]
            fn an_entity_no_format_has_is_a_partial_refined_by_nothing() {
                let dir = with_urls("GHOST,https://g/\n");
                let err = refused(&dir, "https://g/r.pdf");
                assert!(
                    matches!(&err, MetadataError::PartialMatchesNoFormat { name, line: 1, .. } if name == "GHOST"),
                    "{err:?}"
                );
            }

            #[test]
            fn an_open_location_on_formats_without_one_is_a_partial_refined_by_nothing() {
                let dir = with_urls("MEDIOLANUM-IT24@*,https://m/\n");
                let err = refused(&dir, "https://m/r.pdf");
                assert!(
                    matches!(&err, MetadataError::PartialMatchesNoFormat { line: 1, .. }),
                    "{err:?}"
                );
            }

            /// §1.3: `MEDIOLANUM-IT24` is complete, and only `.A`/`.B` exist.
            #[test]
            fn a_complete_name_that_exists_only_with_variants_is_an_unknown_format_name() {
                let dir = with_urls("MEDIOLANUM-IT24,https://m/\n");
                let err = refused(&dir, "https://m/r.pdf");
                assert!(
                    matches!(&err, MetadataError::UnknownFormatName { name, line: 1, .. } if name == "MEDIOLANUM-IT24"),
                    "{err:?}"
                );
            }

            #[test]
            fn a_complete_name_that_exists_only_with_a_location_is_an_unknown_format_name() {
                let dir = with_urls("FIDEURAM-IT23,https://f/\n");
                let err = refused(&dir, "https://f/r.pdf");
                assert!(
                    matches!(&err, MetadataError::UnknownFormatName { line: 1, .. }),
                    "{err:?}"
                );
            }

            /// §1.4: from `url_mapping.csv` come values, not templates.
            #[test]
            fn a_template_is_not_a_format_pattern() {
                let dir = with_urls("AMUNDI-EN24,https://a/\n*-EN24,https://x/\n");
                let err = refused(&dir, "https://a/r.pdf");
                assert!(
                    matches!(&err, MetadataError::InvalidFormatPattern { name, line: 2, .. } if name == "*-EN24"),
                    "{err:?}"
                );
            }

            #[test]
            fn a_cell_that_is_not_a_pattern_is_an_error() {
                let dir = with_urls("-IT24,https://x/\n");
                let err = refused(&dir, "https://x/r.pdf");
                assert!(
                    matches!(&err, MetadataError::InvalidFormatPattern { name, line: 1, .. } if name == "-IT24"),
                    "{err:?}"
                );
            }

            #[test]
            fn the_first_defective_row_is_the_one_reported() {
                let dir = with_urls("GHOST,https://g/\nMEDIOLANUM-IT24,https://m/\n");
                let err = refused(&dir, "https://g/r.pdf");
                assert!(
                    matches!(&err, MetadataError::PartialMatchesNoFormat { line: 1, .. }),
                    "{err:?}"
                );
            }

            #[test]
            fn the_message_names_the_file_the_line_and_the_name() {
                let dir = with_urls("AMUNDI-EN24,https://a/\nMEDIOLANUM-FR,https://m/\n");
                let message = refused(&dir, "https://a/r.pdf").to_string();
                assert!(message.contains("url_mapping.csv"), "{message}");
                assert!(message.contains("line 2"), "{message}");
                assert!(message.contains("MEDIOLANUM-FR"), "{message}");
            }
        }

        mod choosing_a_row {
            use super::*;
            use pretty_assertions::assert_eq;

            /// D3, the example of §10.5: the row written for the sub-folder wins over a more
            /// specific row with a shorter prefix.
            #[test]
            fn the_longest_prefix_wins_over_a_more_specific_format() {
                let dir =
                    with_urls("MEDIOLANUM-IT24.A,https://x.it/\nMEDIOLANUM-ES,https://x.it/es/\n");
                assert_eq!(
                    found(&dir, "https://x.it/es/doc.pdf"),
                    Some(p("MEDIOLANUM-ES"))
                );
                assert_eq!(
                    found(&dir, "https://x.it/doc.pdf"),
                    Some(p("MEDIOLANUM-IT24.A"))
                );
            }

            #[test]
            fn the_longest_prefix_wins_whatever_the_row_order() {
                let dir =
                    with_urls("MEDIOLANUM-ES,https://x.it/es/\nMEDIOLANUM-IT24.A,https://x.it/\n");
                assert_eq!(
                    found(&dir, "https://x.it/es/doc.pdf"),
                    Some(p("MEDIOLANUM-ES"))
                );
                assert_eq!(
                    found(&dir, "https://x.it/doc.pdf"),
                    Some(p("MEDIOLANUM-IT24.A"))
                );
            }

            #[test]
            fn on_equal_prefixes_the_more_specific_format_wins_even_written_second() {
                let dir = with_urls("MEDIOLANUM,https://y.it/\nMEDIOLANUM-IT24.A,https://y.it/\n");
                assert_eq!(
                    found(&dir, "https://y.it/r.pdf"),
                    Some(p("MEDIOLANUM-IT24.A"))
                );
            }

            #[test]
            fn on_equal_prefixes_the_more_specific_format_wins_written_first() {
                let dir = with_urls("MEDIOLANUM-IT24.A,https://y.it/\nMEDIOLANUM,https://y.it/\n");
                assert_eq!(
                    found(&dir, "https://y.it/r.pdf"),
                    Some(p("MEDIOLANUM-IT24.A"))
                );
            }

            /// §1.5: a `Required` slot (`.*`) is not specific.
            #[test]
            fn an_open_variant_is_less_specific_than_a_fixed_one() {
                let dir =
                    with_urls("MEDIOLANUM-IT24.*,https://y.it/\nMEDIOLANUM-IT24.B,https://y.it/\n");
                assert_eq!(
                    found(&dir, "https://y.it/r.pdf"),
                    Some(p("MEDIOLANUM-IT24.B"))
                );
            }

            #[test]
            fn on_equal_prefixes_a_complete_name_wins_over_a_partial() {
                let dir = with_urls("MEDIOLANUM-ES,https://w/\nAMUNDI-EN24,https://w/\n");
                assert_eq!(found(&dir, "https://w/r.pdf"), Some(p("AMUNDI-EN24")));
            }

            #[test]
            fn on_equal_prefixes_and_specificity_the_first_row_wins() {
                let dir = with_urls("MEDIOLANUM-IT,https://z/\nMEDIOLANUM-ES,https://z/\n");
                assert_eq!(found(&dir, "https://z/r.pdf"), Some(p("MEDIOLANUM-IT")));
                let dir = with_urls("MEDIOLANUM-ES,https://z/\nMEDIOLANUM-IT,https://z/\n");
                assert_eq!(found(&dir, "https://z/r.pdf"), Some(p("MEDIOLANUM-ES")));
            }

            /// Specificity only breaks ties: a longer prefix is never beaten by it.
            #[test]
            fn a_more_specific_row_with_a_prefix_that_does_not_match_does_not_count() {
                let dir = with_urls("MEDIOLANUM,https://v/\nMEDIOLANUM-IT24.A,https://v/it/\n");
                assert_eq!(found(&dir, "https://v/es/r.pdf"), Some(p("MEDIOLANUM")));
            }
        }

        mod grouping {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn a_partial_row_is_grouped_under_its_name() {
                let dir = with_urls(
                    "MEDIOLANUM-IT24.*,https://a/\nAMUNDI-EN24,https://b/\nMEDIOLANUM-IT24.*,https://c/\n",
                );
                let names = get_formats(dir.path()).unwrap();
                let mapping = get_url_mapping(dir.path(), &names).unwrap();
                assert_eq!(
                    mapping["MEDIOLANUM-IT24.*"],
                    vec!["https://a/".to_string(), "https://c/".to_string()]
                );
                assert_eq!(mapping["AMUNDI-EN24"], vec!["https://b/".to_string()]);
            }

            #[test]
            fn a_partial_no_format_refines_is_an_error_for_the_grouping_too() {
                let dir = with_urls("MEDIOLANUM-FR,https://a/\n");
                let names = get_formats(dir.path()).unwrap();
                let err = get_url_mapping(dir.path(), &names).unwrap_err();
                assert!(
                    matches!(&err, MetadataError::PartialMatchesNoFormat { line: 1, .. }),
                    "{err:?}"
                );
            }
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
            assert_eq!(
                mapping["AMUNDI-EN24"],
                vec!["https://a/".to_string(), "https://c/".to_string()]
            );
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
            let dir = repo(&[
                ("formats.csv", FORMATS_CSV),
                ("url_mapping.csv", "Format name,Url\n"),
            ]);
            assert!(
                get_url_mapping(dir.path(), &names_of(&[]))
                    .unwrap()
                    .is_empty()
            );
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
                vec![
                    "AMUNDI-EN24".to_string(),
                    "AMUNDI-IT24".to_string(),
                    "ANIMA-EN23".to_string()
                ]
            );
        }
    }
}
