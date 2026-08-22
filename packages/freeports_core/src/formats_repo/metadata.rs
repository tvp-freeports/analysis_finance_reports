//! Rust port of `packages/freeports_core/src/freeports/_internals/formats/repo/metadata.py`'s
//! `get_formats`/`url_to_format`/`get_url_mapping` (plus the private `_get_url_mapping` they both
//! build on) — the formats-repository's CSV-backed format-name/URL-detection layer.
//!
//! See `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`, Milestone 1
//! Step 1.2, for the full design context, and `metadata.py` itself for ground-truth semantics
//! (`get_formats`, `url_to_format`, `_get_url_mapping`, `get_url_mapping`, `formats_schema`,
//! `_url_mapping_schema`). This is an **independent Rust reimplementation**, not a call into
//! Python — `metadata.py`'s `FORMAT_NAME_REGEXP` constant stays exactly as it is (still imported
//! directly by live format-authoring code, `pipelines_definition.py`), per
//! `agent-memory/detect-format-metadata-rust-port-requirements.md`'s explicit constraint. The
//! regex itself is not reimplemented here either — see [`FormatRow`] type-choice note below and
//! reuse [`crate::formats_repo::id_format::FORMAT_NAME_REGEXP`] (Step 1.1) for the whole-string
//! validity check `get_formats` needs to perform on every synthesized name.
//!
//! Uses the `csv` crate (confirmed no-join subset of pandas' CSV+`assign`+`groupby` behavior,
//! per the requirements note's crate-choice section) — plain header/record reading, the same
//! idiom `cli/batch.rs`'s `load_batch_jobs` already uses in this crate (`csv::Reader::from_path`,
//! zip headers with each record), not `serde`-derived row structs (this crate's `csv` dependency
//! doesn't enable the `serde` feature).
//!
//! **Type-choice note — why `get_formats` returns `Vec<String>`, not a richer `FormatRow`
//! struct**: `metadata.py`'s `get_formats` returns a whole `pd.DataFrame` (index = synthesized
//! `Format name` strings, columns = `Name`/`Locale`/`Year`/`Country`/`Version`), but grepping
//! every real call site (`freeports_config.rs`'s `detect_format`, `job.rs`'s `Algorithm::load`
//! call to the Python original today) shows only `.index.to_list()` is ever consumed — the
//! `Name`/`Locale`/`Year`/`Country`/`Version` columns are read only to *synthesize and validate*
//! the index, then discarded by every caller. A `FormatRow` struct carrying those columns forward
//! would be dead weight with zero consumers, so this port narrows the return type to just the
//! validated, duplicate-checked `Vec<String>` of format names — exactly the shape
//! `url_to_format`/`get_url_mapping`'s own `format_names: &[String]` parameter already expects,
//! so `get_formats`'s output plugs directly into them with no conversion step.
//!
//! **Pre-implementation scaffolding note (test-writer phase)**: every function/impl body below is
//! a `todo!()` stub — this file's job at this stage is only to give the test suite below a real
//! type/signature surface to compile against (`cargo test --lib` must compile cleanly even though
//! every test currently panics/fails). `implementer` fills these in; per this workspace's TDD
//! discipline, tests are the contract and must not be edited to make them pass.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::formats_repo::id_format::FORMAT_NAME_REGEXP;

const METADATA_DIR: &str = "metadata";

/// Mirrors the union of failure modes `formats_schema.validate`/`_url_mapping_schema.validate`
/// (via `get_url_mapping_schema`'s `isin` check) can raise in the Python original, but as plain
/// Rust variants rather than an attempt at pandera-shape fidelity — a clear, specific message per
/// variant is enough (confirmed answer #4 in
/// `agent-memory/detect-format-metadata-rust-port-requirements.md`).
#[derive(Debug, Clone, PartialEq)]
pub enum MetadataError {
    /// `metadata/formats.csv` or `metadata/url_mapping.csv` doesn't exist under the given
    /// `formats_repo_dir`. Carries the full path that was missing.
    MissingCsv(PathBuf),
    /// A CSV row couldn't be read/parsed at all — wrong/missing columns, or a `Year` value that
    /// doesn't parse as an integer. `line` is the 1-based position of the offending row within
    /// the CSV (header line excluded, i.e. the first data row is line 1), `reason` is a short,
    /// human-readable explanation of what went wrong.
    MalformedRow { line: usize, reason: String },
    /// `formats.csv` produced the same synthesized `Format name` for two different rows.
    DuplicateFormatName(String),
    /// A synthesized `Format name` doesn't match
    /// [`crate::formats_repo::id_format::FORMAT_NAME_REGEXP`] (mirrors `formats_schema`'s index
    /// `pa.Check` failing).
    InvalidFormatName(String),
    /// A row in `url_mapping.csv` names a `Format name` that isn't present in the `format_names`
    /// slice passed in (mirrors `get_url_mapping_schema`'s `pa.Check.isin(format_names)` failing).
    UnknownFormatName(String),
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetadataError::MissingCsv(path) => write!(f, "missing formats-repository CSV file: {}", path.display()),
            MetadataError::MalformedRow { line, reason } => write!(f, "malformed row at line {line}: {reason}"),
            MetadataError::DuplicateFormatName(name) => write!(f, "duplicate format name: {name}"),
            MetadataError::InvalidFormatName(name) => {
                write!(f, "format name '{name}' does not match the expected format name pattern")
            }
            MetadataError::UnknownFormatName(name) => write!(f, "unknown format name: {name}"),
        }
    }
}

impl std::error::Error for MetadataError {}

/// Whole-string match against [`FORMAT_NAME_REGEXP`] — the constant itself is an unanchored
/// fragment (see its own doc comment), so a "is this whole string a valid format name" check needs
/// the match span to cover the entire input, exactly like `id_format.rs`'s own test-only
/// `matches_whole` helper.
fn is_valid_format_name(name: &str) -> bool {
    FORMAT_NAME_REGEXP.find(name).is_some_and(|(start, end)| start == 0 && end == name.len())
}

/// Opens `<formats_repo_dir>/metadata/<file_name>` as a `csv::Reader`, or `MissingCsv` if it
/// doesn't exist on disk.
fn open_csv(formats_repo_dir: &Path, file_name: &str) -> Result<csv::Reader<std::fs::File>, MetadataError> {
    let path = formats_repo_dir.join(METADATA_DIR).join(file_name);
    if !path.exists() {
        return Err(MetadataError::MissingCsv(path));
    }
    csv::Reader::from_path(&path).map_err(|e| MetadataError::MalformedRow { line: 0, reason: e.to_string() })
}

/// Looks up a required column's index in `headers`, or a `MalformedRow` naming the missing column.
fn required_column(headers: &csv::StringRecord, name: &str) -> Result<usize, MetadataError> {
    headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| MetadataError::MalformedRow { line: 0, reason: format!("missing required column '{name}'") })
}

/// Rust port of `metadata.py`'s `get_formats`: loads `<formats_repo_dir>/metadata/formats.csv`,
/// synthesizes each row's `Format name` (`Name-Locale<YY>[@Country][.Version]`, where `YY` is the
/// last two characters of `Year`'s string form, and `@Country`/`.Version` are appended only when
/// the respective column is present *and* non-empty), validates every synthesized name against
/// [`crate::formats_repo::id_format::FORMAT_NAME_REGEXP`] and for index-wide uniqueness (mirrors
/// `formats_schema`'s `unique=True` index), and returns them in CSV row order.
///
/// See this module's doc comment for why the return type is `Vec<String>` rather than a richer
/// per-row struct.
pub fn get_formats(formats_repo_dir: &Path) -> Result<Vec<String>, MetadataError> {
    let mut reader = open_csv(formats_repo_dir, "formats.csv")?;
    let headers = reader
        .headers()
        .map_err(|e| MetadataError::MalformedRow { line: 0, reason: e.to_string() })?
        .clone();
    let name_idx = required_column(&headers, "Name")?;
    let locale_idx = required_column(&headers, "Locale")?;
    let year_idx = required_column(&headers, "Year")?;
    let country_idx = required_column(&headers, "Country")?;
    let version_idx = required_column(&headers, "Version")?;

    let mut seen = std::collections::HashSet::new();
    let mut format_names = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let line = i + 1;
        let record = record.map_err(|e| MetadataError::MalformedRow { line, reason: e.to_string() })?;
        let get = |idx: usize| record.get(idx).unwrap_or("");
        let name = get(name_idx);
        let locale = get(locale_idx);
        let year_field = get(year_idx);
        let country = get(country_idx);
        let version = get(version_idx);

        let year: i64 = year_field
            .trim()
            .parse()
            .map_err(|_| MetadataError::MalformedRow { line, reason: format!("invalid Year value '{year_field}'") })?;
        let year_str = year.to_string();
        let yy = if year_str.len() >= 2 { &year_str[year_str.len() - 2..] } else { year_str.as_str() };

        let mut format_name = format!("{name}-{locale}{yy}");
        if !country.is_empty() {
            format_name.push('@');
            format_name.push_str(country);
        }
        if !version.is_empty() {
            format_name.push('.');
            format_name.push_str(version);
        }

        if !is_valid_format_name(&format_name) {
            return Err(MetadataError::InvalidFormatName(format_name));
        }
        if !seen.insert(format_name.clone()) {
            return Err(MetadataError::DuplicateFormatName(format_name));
        }
        format_names.push(format_name);
    }
    Ok(format_names)
}

/// Rust port of `metadata.py`'s `url_to_format`: loads `<formats_repo_dir>/metadata/
/// url_mapping.csv`, validates every row's `Format name` against `format_names` (mirrors
/// `_get_url_mapping`'s `isin` schema check — validation happens over the *whole* file eagerly,
/// before any prefix matching, exactly like `pa.DataFrameSchema.validate` validates a whole
/// DataFrame up front), then finds the row whose `Url` is the **longest literal string prefix**
/// (`url.starts_with(row_url)` — plain string comparison, never treated as a regex even if the
/// `Url` cell itself contains regex metacharacters like `.*`) of `url`. Ties (equal-length
/// matching prefixes) are broken by first-row-in-CSV-order, mirroring pandas' `idxmax()`'s stable
/// first-occurrence-on-tie behavior. Returns `Ok(None)` when no row's `Url` is a prefix of `url`.
/// Shared by [`url_to_format`] and [`get_url_mapping`]: loads `url_mapping.csv` and validates
/// every row's `Format name` against `format_names` eagerly, over the whole file, before returning
/// — mirrors `_get_url_mapping`'s `pa.DataFrameSchema.validate` call validating the whole
/// DataFrame up front. Rows are returned in CSV row order.
fn read_url_mapping(
    formats_repo_dir: &Path,
    format_names: &[String],
) -> Result<Vec<(String, String)>, MetadataError> {
    let mut reader = open_csv(formats_repo_dir, "url_mapping.csv")?;
    let headers = reader
        .headers()
        .map_err(|e| MetadataError::MalformedRow { line: 0, reason: e.to_string() })?
        .clone();
    let format_name_idx = required_column(&headers, "Format name")?;
    let url_idx = required_column(&headers, "Url")?;

    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let line = i + 1;
        let record = record.map_err(|e| MetadataError::MalformedRow { line, reason: e.to_string() })?;
        let format_name = record.get(format_name_idx).unwrap_or("").to_string();
        let url = record.get(url_idx).unwrap_or("").to_string();
        rows.push((format_name, url));
    }

    for (format_name, _) in &rows {
        if !format_names.iter().any(|n| n == format_name) {
            return Err(MetadataError::UnknownFormatName(format_name.clone()));
        }
    }

    Ok(rows)
}

pub fn url_to_format(
    formats_repo_dir: &Path,
    format_names: &[String],
    url: &str,
) -> Result<Option<String>, MetadataError> {
    let rows = read_url_mapping(formats_repo_dir, format_names)?;

    let mut best: Option<(usize, usize)> = None; // (row index, matched prefix length)
    for (i, (_, row_url)) in rows.iter().enumerate() {
        if url.starts_with(row_url.as_str()) {
            let len = row_url.len();
            let is_better = match best {
                Some((_, best_len)) => len > best_len,
                None => true,
            };
            if is_better {
                best = Some((i, len));
            }
        }
    }

    Ok(best.map(|(i, _)| rows[i].0.clone()))
}

/// Rust port of `metadata.py`'s `get_url_mapping`: loads and validates `url_mapping.csv` exactly
/// like [`url_to_format`] does (same eager `format_names` validation), then groups all `Url`
/// values by `Format name`, preserving each format's URLs in CSV row order within its `Vec`.
pub fn get_url_mapping(
    formats_repo_dir: &Path,
    format_names: &[String],
) -> Result<HashMap<String, Vec<String>>, MetadataError> {
    let rows = read_url_mapping(formats_repo_dir, format_names)?;
    let mut mapping: HashMap<String, Vec<String>> = HashMap::new();
    for (format_name, url) in rows {
        mapping.entry(format_name).or_default().push(url);
    }
    Ok(mapping)
}

/// PyO3-facing entry point for [`get_formats`], exported as `freeports._native.core.get_formats`
/// (see `lib.rs`'s `mod core` block). `formats_repo_dir` is a `PathBuf` (accepts a `pathlib.Path`
/// or `str` from Python automatically, same convention as `input/companies_db.rs`'s
/// `py_get_target_companies`).
#[pyfunction]
#[pyo3(name = "get_formats")]
pub fn py_get_formats(formats_repo_dir: PathBuf) -> PyResult<Vec<String>> {
    get_formats(&formats_repo_dir).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// PyO3-facing entry point for [`url_to_format`], exported as
/// `freeports._native.core.url_to_format`.
#[pyfunction]
#[pyo3(name = "url_to_format")]
pub fn py_url_to_format(formats_repo_dir: PathBuf, format_names: Vec<String>, url: String) -> PyResult<Option<String>> {
    url_to_format(&formats_repo_dir, &format_names, &url).map_err(|e| PyValueError::new_err(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ============================================================
    // Fixture helpers
    // ============================================================

    /// Writes `<dir>/metadata/formats.csv` with the given raw CSV text, creating the `metadata/`
    /// subfolder as needed. Deliberately takes raw CSV text (rather than baking in one row shape
    /// like `freeports_config.rs`'s `formats_repo_fixture`) since this module's tests need many
    /// different row shapes, including malformed ones that helper can't produce.
    fn write_formats_csv(dir: &Path, csv_text: &str) {
        let metadata_dir = dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        std::fs::write(metadata_dir.join("formats.csv"), csv_text).unwrap();
    }

    /// Writes `<dir>/metadata/url_mapping.csv` with the given raw CSV text, creating the
    /// `metadata/` subfolder as needed.
    fn write_url_mapping_csv(dir: &Path, csv_text: &str) {
        let metadata_dir = dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        std::fs::write(metadata_dir.join("url_mapping.csv"), csv_text).unwrap();
    }

    const FORMATS_HEADER: &str = "Name,Locale,Year,Country,Version";
    const URL_MAPPING_HEADER: &str = "Format name,Url";

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    // ============================================================
    // get_formats
    // ============================================================

    #[test]
    fn get_formats_errors_when_formats_csv_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        // No metadata/ folder at all created.
        let expected_path = dir.path().join("metadata").join("formats.csv");
        assert_eq!(get_formats(dir.path()), Err(MetadataError::MissingCsv(expected_path)));
    }

    #[test]
    fn get_formats_returns_empty_vec_for_a_header_only_csv() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\n"));
        assert_eq!(get_formats(dir.path()), Ok(vec![]));
    }

    #[test]
    fn get_formats_synthesizes_a_plain_name_with_no_country_or_version() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nAMUNDI,EN,2024,,\n"));
        assert_eq!(get_formats(dir.path()), Ok(vec!["AMUNDI-EN24".to_string()]));
    }

    #[test]
    fn get_formats_synthesizes_a_name_with_country_only() {
        let dir = tempfile::tempdir().unwrap();
        // Real row shape from analysis_finance_reports_formats/metadata/formats.csv.
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nFINECO,EN,23,IR,\n"));
        assert_eq!(get_formats(dir.path()), Ok(vec!["FINECO-EN23@IR".to_string()]));
    }

    #[test]
    fn get_formats_synthesizes_a_name_with_version_only() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nMEDIOLANUM,ES,24,,B\n"));
        assert_eq!(get_formats(dir.path()), Ok(vec!["MEDIOLANUM-ES24.B".to_string()]));
    }

    #[test]
    fn get_formats_synthesizes_a_name_with_both_country_and_version() {
        // Taken verbatim from metadata.py's own get_formats() docstring example.
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nEurizon,IT,24,IT,v2\n"));
        assert_eq!(get_formats(dir.path()), Ok(vec!["Eurizon-IT24@IT.v2".to_string()]));
    }

    #[test]
    fn get_formats_treats_a_literal_empty_string_the_same_as_a_missing_country_or_version() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nAMUNDI,EN,2024,,\n"));
        // No "@" / "." suffix at all - an empty Country/Version cell must not synthesize
        // "AMUNDI-EN24@" or "AMUNDI-EN24.".
        assert_eq!(get_formats(dir.path()), Ok(vec!["AMUNDI-EN24".to_string()]));
    }

    #[test]
    fn get_formats_uses_only_the_last_two_characters_of_a_four_digit_year() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nCARNE,EN,2023,,\n"));
        assert_eq!(get_formats(dir.path()), Ok(vec!["CARNE-EN23".to_string()]));
    }

    #[test]
    fn get_formats_preserves_csv_row_order() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(
            dir.path(),
            &format!("{FORMATS_HEADER}\nZETA,EN,24,,\nALPHA,EN,24,,\nMID,EN,24,,\n"),
        );
        assert_eq!(
            get_formats(dir.path()),
            Ok(vec!["ZETA-EN24".to_string(), "ALPHA-EN24".to_string(), "MID-EN24".to_string()])
        );
    }

    #[test]
    fn get_formats_errors_on_duplicate_synthesized_format_names() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nAMUNDI,EN,2024,,\nAMUNDI,EN,2024,,\n"));
        assert_eq!(get_formats(dir.path()), Err(MetadataError::DuplicateFormatName("AMUNDI-EN24".to_string())));
    }

    #[test]
    fn get_formats_errors_when_the_locale_is_lowercase() {
        // Synthesized "amundi-en24" fails FORMAT_NAME_REGEXP's `[A-Z]{2}` locale group.
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nAMUNDI,en,2024,,\n"));
        assert_eq!(get_formats(dir.path()), Err(MetadataError::InvalidFormatName("AMUNDI-en24".to_string())));
    }

    #[test]
    fn get_formats_errors_when_the_year_has_only_a_single_digit() {
        // `Year=5` -> `str(5)[-2:]` is just "5" (Python slicing past the start of a short string
        // returns what's available, it doesn't error) - the synthesized "AMUNDI-EN5" then fails
        // FORMAT_NAME_REGEXP's `\d{2}` requirement.
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nAMUNDI,EN,5,,\n"));
        assert_eq!(get_formats(dir.path()), Err(MetadataError::InvalidFormatName("AMUNDI-EN5".to_string())));
    }

    #[test]
    fn get_formats_errors_on_a_missing_required_column() {
        let dir = tempfile::tempdir().unwrap();
        // No "Year" column at all.
        write_formats_csv(dir.path(), "Name,Locale,Country,Version\nAMUNDI,EN,,\n");
        assert!(matches!(get_formats(dir.path()), Err(MetadataError::MalformedRow { .. })));
    }

    #[test]
    fn get_formats_errors_on_a_row_with_the_wrong_number_of_fields() {
        let dir = tempfile::tempdir().unwrap();
        // Header has 5 columns, data row only has 3.
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nAMUNDI,EN,2024\n"));
        assert!(matches!(get_formats(dir.path()), Err(MetadataError::MalformedRow { .. })));
    }

    #[test]
    fn get_formats_errors_on_an_unparseable_year() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(dir.path(), &format!("{FORMATS_HEADER}\nAMUNDI,EN,not-a-year,,\n"));
        assert!(matches!(get_formats(dir.path()), Err(MetadataError::MalformedRow { .. })));
    }

    // ============================================================
    // url_to_format
    // ============================================================

    #[test]
    fn url_to_format_returns_none_when_no_prefix_matches() {
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(dir.path(), &format!("{URL_MAPPING_HEADER}\nAMUNDI-EN24,https://www.amundi.com/\n"));
        let result = url_to_format(dir.path(), &names(&["AMUNDI-EN24"]), "https://www.other.com/report.pdf");
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn url_to_format_matches_a_single_prefix() {
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(dir.path(), &format!("{URL_MAPPING_HEADER}\nAMUNDI-EN24,https://www.amundi.com/\n"));
        let result = url_to_format(dir.path(), &names(&["AMUNDI-EN24"]), "https://www.amundi.com/report.pdf");
        assert_eq!(result, Ok(Some("AMUNDI-EN24".to_string())));
    }

    #[test]
    fn url_to_format_prefers_the_longest_matching_prefix() {
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(
            dir.path(),
            &format!(
                "{URL_MAPPING_HEADER}\nGENERAL-EN24,https://www.example.com/\nSPECIFIC-EN24,https://www.example.com/sub/\n"
            ),
        );
        let result = url_to_format(
            dir.path(),
            &names(&["GENERAL-EN24", "SPECIFIC-EN24"]),
            "https://www.example.com/sub/report.pdf",
        );
        // Both rows' Url is a literal prefix of the query url; the longer, more specific one wins.
        assert_eq!(result, Ok(Some("SPECIFIC-EN24".to_string())));
    }

    #[test]
    fn url_to_format_still_prefers_the_longest_prefix_when_the_more_specific_row_comes_first() {
        // Same as above but with row order swapped, to confirm the winner is chosen by prefix
        // length, not merely "whichever row comes first".
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(
            dir.path(),
            &format!(
                "{URL_MAPPING_HEADER}\nSPECIFIC-EN24,https://www.example.com/sub/\nGENERAL-EN24,https://www.example.com/\n"
            ),
        );
        let result = url_to_format(
            dir.path(),
            &names(&["GENERAL-EN24", "SPECIFIC-EN24"]),
            "https://www.example.com/sub/report.pdf",
        );
        assert_eq!(result, Ok(Some("SPECIFIC-EN24".to_string())));
    }

    #[test]
    fn url_to_format_breaks_ties_by_first_csv_row_when_prefixes_are_equal_length() {
        let dir = tempfile::tempdir().unwrap();
        // Two different format names mapped to the exact same Url string - both match with equal
        // length, so the first row in CSV order (FIRST-EN24) must win, mirroring pandas'
        // `idxmax()`'s stable first-occurrence-on-tie behavior.
        write_url_mapping_csv(
            dir.path(),
            &format!(
                "{URL_MAPPING_HEADER}\nFIRST-EN24,https://www.example.com/\nSECOND-EN24,https://www.example.com/\n"
            ),
        );
        let result = url_to_format(
            dir.path(),
            &names(&["FIRST-EN24", "SECOND-EN24"]),
            "https://www.example.com/report.pdf",
        );
        assert_eq!(result, Ok(Some("FIRST-EN24".to_string())));
    }

    #[test]
    fn url_to_format_treats_a_dot_star_url_prefix_as_literal_text_not_a_regex_wildcard() {
        // Real row from analysis_finance_reports_formats/metadata/url_mapping.csv - the literal
        // ".*" characters must be matched as plain text, not interpreted as "any characters".
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(
            dir.path(),
            &format!("{URL_MAPPING_HEADER}\nASTERIA-EN23,https://www.fundsquare.net/.*aKxRJLzQbOPf89pUHU0Jbso\n"),
        );
        let format_names = names(&["ASTERIA-EN23"]);

        // Literally starts with the exact prefix text, including the literal ".*" - must match.
        let literal_match = url_to_format(
            dir.path(),
            &format_names,
            "https://www.fundsquare.net/.*aKxRJLzQbOPf89pUHU0Jbso/report.pdf",
        );
        assert_eq!(literal_match, Ok(Some("ASTERIA-EN23".to_string())));

        // Would match if ".*" were a regex wildcard (any characters in place of ".*"), but does
        // NOT literally start with the prefix text - must NOT match.
        let would_match_only_as_regex =
            url_to_format(dir.path(), &format_names, "https://www.fundsquare.net/XYaKxRJLzQbOPf89pUHU0Jbso/report.pdf");
        assert_eq!(would_match_only_as_regex, Ok(None));
    }

    #[test]
    fn url_to_format_errors_when_url_mapping_references_an_unknown_format_name() {
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(dir.path(), &format!("{URL_MAPPING_HEADER}\nGHOST-EN24,https://www.example.com/\n"));
        let result = url_to_format(dir.path(), &names(&["AMUNDI-EN24"]), "https://www.example.com/report.pdf");
        assert_eq!(result, Err(MetadataError::UnknownFormatName("GHOST-EN24".to_string())));
    }

    #[test]
    fn url_to_format_errors_eagerly_even_when_the_offending_row_would_not_have_matched() {
        // The whole url_mapping.csv is schema-validated up front (mirrors pa.DataFrameSchema's
        // whole-DataFrame validation), before any prefix matching happens - an unknown format
        // name elsewhere in the file must error even though the query url only overlaps with a
        // different, perfectly valid row.
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(
            dir.path(),
            &format!(
                "{URL_MAPPING_HEADER}\nAMUNDI-EN24,https://www.amundi.com/\nGHOST-EN24,https://www.unrelated.com/\n"
            ),
        );
        let result = url_to_format(dir.path(), &names(&["AMUNDI-EN24"]), "https://www.amundi.com/report.pdf");
        assert_eq!(result, Err(MetadataError::UnknownFormatName("GHOST-EN24".to_string())));
    }

    #[test]
    fn url_to_format_errors_when_url_mapping_csv_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let expected_path = dir.path().join("metadata").join("url_mapping.csv");
        let result = url_to_format(dir.path(), &names(&["AMUNDI-EN24"]), "https://www.amundi.com/report.pdf");
        assert_eq!(result, Err(MetadataError::MissingCsv(expected_path)));
    }

    #[test]
    fn url_to_format_errors_on_a_malformed_url_mapping_row() {
        let dir = tempfile::tempdir().unwrap();
        // Missing the "Url" column entirely.
        write_url_mapping_csv(dir.path(), "Format name\nAMUNDI-EN24\n");
        let result = url_to_format(dir.path(), &names(&["AMUNDI-EN24"]), "https://www.amundi.com/report.pdf");
        assert!(matches!(result, Err(MetadataError::MalformedRow { .. })));
    }

    // ============================================================
    // get_url_mapping
    // ============================================================

    #[test]
    fn get_url_mapping_returns_an_empty_map_for_a_header_only_csv() {
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(dir.path(), &format!("{URL_MAPPING_HEADER}\n"));
        assert_eq!(get_url_mapping(dir.path(), &names(&["AMUNDI-EN24"])), Ok(HashMap::new()));
    }

    #[test]
    fn get_url_mapping_groups_multiple_urls_under_the_same_format_name() {
        // Real shape from analysis_finance_reports_formats/metadata/url_mapping.csv
        // (AMUNDI-EN24 has two Url rows).
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(
            dir.path(),
            &format!("{URL_MAPPING_HEADER}\nAMUNDI-EN24,https://www.amundi.com/\nAMUNDI-EN24,https://www.amundi.com/ABC\n"),
        );
        let result = get_url_mapping(dir.path(), &names(&["AMUNDI-EN24"])).unwrap();
        assert_eq!(
            result.get("AMUNDI-EN24"),
            Some(&vec!["https://www.amundi.com/".to_string(), "https://www.amundi.com/ABC".to_string()])
        );
    }

    #[test]
    fn get_url_mapping_keeps_separate_lists_per_format_name() {
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(
            dir.path(),
            &format!("{URL_MAPPING_HEADER}\nAMUNDI-EN24,https://www.amundi.com/\nARCA-IT24,https://docs.arcafondi.it/\n"),
        );
        let result = get_url_mapping(dir.path(), &names(&["AMUNDI-EN24", "ARCA-IT24"])).unwrap();
        let mut expected = HashMap::new();
        expected.insert("AMUNDI-EN24".to_string(), vec!["https://www.amundi.com/".to_string()]);
        expected.insert("ARCA-IT24".to_string(), vec!["https://docs.arcafondi.it/".to_string()]);
        assert_eq!(result, expected);
    }

    #[test]
    fn get_url_mapping_errors_on_an_unknown_format_name() {
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(dir.path(), &format!("{URL_MAPPING_HEADER}\nGHOST-EN24,https://www.example.com/\n"));
        let result = get_url_mapping(dir.path(), &names(&["AMUNDI-EN24"]));
        assert_eq!(result, Err(MetadataError::UnknownFormatName("GHOST-EN24".to_string())));
    }

    #[test]
    fn get_url_mapping_errors_when_url_mapping_csv_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let expected_path = dir.path().join("metadata").join("url_mapping.csv");
        assert_eq!(get_url_mapping(dir.path(), &names(&["AMUNDI-EN24"])), Err(MetadataError::MissingCsv(expected_path)));
    }

    #[test]
    fn get_url_mapping_errors_on_a_malformed_row() {
        let dir = tempfile::tempdir().unwrap();
        write_url_mapping_csv(dir.path(), "Format name\nAMUNDI-EN24\n");
        let result = get_url_mapping(dir.path(), &names(&["AMUNDI-EN24"]));
        assert!(matches!(result, Err(MetadataError::MalformedRow { .. })));
    }

    // ============================================================
    // get_formats -> url_to_format round trip, grounded in real sibling-repo data
    // ============================================================

    #[test]
    fn get_formats_then_url_to_format_round_trip_with_a_real_shaped_fixture() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_csv(
            dir.path(),
            &format!("{FORMATS_HEADER}\nASTERIA,EN,23,,\nFINECO,EN,23,IR,\n"),
        );
        write_url_mapping_csv(
            dir.path(),
            &format!(
                "{URL_MAPPING_HEADER}\nASTERIA-EN23,https://www.fundsquare.net/.*aKxRJLzQbOPf89pUHU0Jbso\nFINECO-EN23@IR,https://www.fineco.ie/\n"
            ),
        );
        let format_names = get_formats(dir.path()).unwrap();
        assert_eq!(format_names, vec!["ASTERIA-EN23".to_string(), "FINECO-EN23@IR".to_string()]);

        let detected = url_to_format(
            dir.path(),
            &format_names,
            "https://www.fundsquare.net/.*aKxRJLzQbOPf89pUHU0Jbso/report.pdf",
        );
        assert_eq!(detected, Ok(Some("ASTERIA-EN23".to_string())));
    }

    // ============================================================
    // MetadataError Display - loose, content-only checks (no pandera-shape fidelity required,
    // per the requirements note's confirmed answer #4 - a clear message is enough, exact wording
    // is implementer's call).
    // ============================================================

    #[test]
    fn metadata_error_missing_csv_display_mentions_the_path() {
        let path = PathBuf::from("/some/repo/metadata/formats.csv");
        let message = MetadataError::MissingCsv(path.clone()).to_string();
        assert!(message.contains(&path.display().to_string()));
    }

    #[test]
    fn metadata_error_duplicate_format_name_display_mentions_the_name() {
        let message = MetadataError::DuplicateFormatName("AMUNDI-EN24".to_string()).to_string();
        assert!(message.contains("AMUNDI-EN24"));
    }

    #[test]
    fn metadata_error_invalid_format_name_display_mentions_the_name() {
        let message = MetadataError::InvalidFormatName("amundi-en24".to_string()).to_string();
        assert!(message.contains("amundi-en24"));
    }

    #[test]
    fn metadata_error_unknown_format_name_display_mentions_the_name() {
        let message = MetadataError::UnknownFormatName("GHOST-EN24".to_string()).to_string();
        assert!(message.contains("GHOST-EN24"));
    }

    #[test]
    fn metadata_error_malformed_row_display_mentions_the_line_and_reason() {
        let message = (MetadataError::MalformedRow { line: 3, reason: "bad Year value".to_string() }).to_string();
        assert!(message.contains('3'));
        assert!(message.contains("bad Year value"));
    }
}
