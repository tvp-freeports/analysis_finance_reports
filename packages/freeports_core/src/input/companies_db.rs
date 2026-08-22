//! Rust port of `_internals/input/companies_db.py`.
//!
//! **Design decision (user confirmed, 2026-08-19)**: like `files_schema.rs`, this replaces
//! `pandera.DataFrameSchema` runtime validation with typed Rust rows validated at construction —
//! CSVs are read via `polars` (the IO boundary this migration already committed to), but every
//! table becomes a plain `Vec<Row>` immediately, not a DataFrame carried through the rest of the
//! module.
//!
//! **`get_target_companies` used to call into the separate `freeports_lib` crate through
//! `py.import`, never a Cargo dependency — history kept here since it explains why
//! `compile_from_rows` exists as a real `#[pymethods]` entry point instead of everything going
//! through the Rust-only `compile_from_target_companies` it delegates to.** PyO3 registers a
//! `#[pyclass]` per compiled extension module, so back when `freeports_lib` was its own crate,
//! `CompanyMatchInfos` instances built by code statically linked into `freeports_engine.so` were a
//! *different, incompatible* Python type from the ones format-author code got via `import
//! freeports_lib` (the standalone `freeports_lib.so`) — confirmed by a real `TypeError:
//! 'CompanyMatchInfos' object cannot be cast as 'CompanyMatchInfos'` the moment such an object
//! reached a text_filter pipe doing that import. Routing through `compile_from_rows` via
//! `py.import` (never a direct Rust call) was the fix at the time — see the doc comment on
//! `TargetCompanyInput` in that module for the full original explanation. Since Fase E merged
//! `freeports_lib`'s code into this same crate (`agent-memory/rust-native-binary-plan.md`), there
//! is only one compiled module left, so the trap no longer applies and `compile_from_rows` is
//! called as a plain native Rust function below — verified concretely (not assumed) by
//! `py_get_target_companies_returns_real_company_match_infos_instances` still passing.
//!
//! **Company identity, confirmed intentional (not a bug) by the user, 2026-08-19**:
//! `get_companies` sets the DataFrame index from `companies.csv`'s raw `Name` column *before*
//! normalizing that column — so every join/`isin`-check/cross-reference in this module (and the
//! `name` `CompanyMatchInfos` is ultimately built from) uses the **raw** name, exactly as every
//! other CSV in `input_db` (`companies_additional_buds.csv`, `tickers.csv`, `company_to_list.csv`,
//! ...) already references companies by their raw name. The *normalized* form is computed
//! separately, downstream, purely for the matching algorithm itself (`CompanyMatchInfos.n_name`,
//! via `formats_utils::text_filter::matcher::normalize_string` — a distinct function from this
//! crate's own `deep_normalize_string`, used only for the containment/regex-match *validation*
//! checks below, matching the Python original's `_stem_contained_in_name`/`_regex_match_name`).
//! This module therefore never stores a normalized company name — `compile_from_target_companies`
//! computes it on the fly.
//!
//! **Not ported**: `Institution`/`Date` from `lists.csv` are validated (must be present and
//! well-formed) but never read again afterward in the Python original either — `get_lists` only
//! ever contributes `lists_df.index.to_list()` (the list *names*) to the rest of the pipeline, so
//! this port validates those two columns' shape without keeping their values.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use polars::prelude::*;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::core::normalization;

const COMPANIES_DIR: &str = "companies";
const LISTS_DIR: &str = "lists";

/// Native replacement for the `PyValueError`s this module used to raise directly for every
/// `input_db` loading/validation failure — mirrors `output::files_schema::SchemaError` (one
/// variant per distinct failure shape, carrying its own context, with a prose `Display`). Nothing
/// in this module actually calls into Python except the one real PyO3 boundary,
/// `py_get_target_companies` — everywhere else, a `PyResult` was pure overhead. Converted to a
/// `PyErr` exactly once, at that boundary, via [`companies_db_err`].
#[derive(Debug, Clone, PartialEq)]
pub enum CompaniesDbError {
    ReadCsv { path: PathBuf, message: String },
    MissingColumn { path: PathBuf, column: &'static str },
    ColumnNotText { path: PathBuf, column: &'static str, message: String },
    EmptyValue { path: PathBuf, column: &'static str, row: usize },
    NotNormalized { context: String, field: &'static str, value: String },
    BudNotContained { bud: String, name: String, normalized: String },
    InvalidRegex { pattern: String, name: String, message: String },
    RegexNotMatching { pattern: String, name: String, normalized: String },
    InvalidDate { context: String, value: String },
    Duplicate { path: PathBuf, kind: &'static str, value: String },
    UnknownReference { path: PathBuf, kind: &'static str, value: String },
    InvalidTickerSymbol { path: PathBuf, symbol: String },
}

impl fmt::Display for CompaniesDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompaniesDbError::ReadCsv { path, message } => write!(f, "cannot read {}: {message}", path.display()),
            CompaniesDbError::MissingColumn { path, column } => write!(f, "{}: missing required column '{column}'", path.display()),
            CompaniesDbError::ColumnNotText { path, column, message } => {
                write!(f, "{}: column '{column}' is not text: {message}", path.display())
            }
            CompaniesDbError::EmptyValue { path, column, row } => write!(f, "{}: row {row} has an empty '{column}'", path.display()),
            CompaniesDbError::NotNormalized { context, field, value } => {
                write!(f, "{context}: {field} '{value}' is not already normalized")
            }
            CompaniesDbError::BudNotContained { bud, name, normalized } => {
                write!(f, "Bud '{bud}' is not contained in company name '{name}' (normalized: '{normalized}')")
            }
            CompaniesDbError::InvalidRegex { pattern, name, message } => {
                write!(f, "invalid regex '{pattern}' for company '{name}': {message}")
            }
            CompaniesDbError::RegexNotMatching { pattern, name, normalized } => {
                write!(f, "regex '{pattern}' does not match company name '{name}' (normalized: '{normalized}')")
            }
            CompaniesDbError::InvalidDate { context, value } => write!(f, "{context}: '{value}' is not a valid YYYY-MM-DD date"),
            CompaniesDbError::Duplicate { path, kind, value } => write!(f, "{}: duplicate {kind} '{value}'", path.display()),
            CompaniesDbError::UnknownReference { path, kind, value } => write!(f, "{}: '{value}' is not a known {kind}", path.display()),
            CompaniesDbError::InvalidTickerSymbol { path, symbol } => {
                write!(f, "{}: '{symbol}' is not a valid ticker symbol (expected 2-6 uppercase letters)", path.display())
            }
        }
    }
}

impl std::error::Error for CompaniesDbError {}

fn companies_db_err(e: CompaniesDbError) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn read_csv(path: &Path) -> Result<DataFrame, CompaniesDbError> {
    CsvReadOptions::default()
        .with_infer_schema_length(Some(0)) // every column read as a plain string; we validate/convert ourselves
        .with_path(Some(path.to_path_buf()))
        .try_into_reader_with_file_path(None)
        .and_then(|reader| reader.finish())
        .map_err(|e| CompaniesDbError::ReadCsv { path: path.to_path_buf(), message: e.to_string() })
}

fn required_str_column(df: &DataFrame, name: &'static str, path: &Path) -> Result<Vec<String>, CompaniesDbError> {
    let series = df
        .column(name)
        .map_err(|_| CompaniesDbError::MissingColumn { path: path.to_path_buf(), column: name })?
        .str()
        .map_err(|e| CompaniesDbError::ColumnNotText { path: path.to_path_buf(), column: name, message: e.to_string() })?;
    series
        .into_iter()
        .enumerate()
        .map(|(i, v)| v.map(str::to_string).ok_or_else(|| CompaniesDbError::EmptyValue { path: path.to_path_buf(), column: name, row: i + 1 }))
        .collect()
}

fn optional_str_column(df: &DataFrame, name: &'static str, len: usize, path: &Path) -> Result<Vec<Option<String>>, CompaniesDbError> {
    match df.column(name) {
        Err(_) => Ok(vec![None; len]),
        Ok(col) => {
            let series = col.str().map_err(|e| CompaniesDbError::ColumnNotText { path: path.to_path_buf(), column: name, message: e.to_string() })?;
            Ok(series.into_iter().map(|v| v.map(str::to_string)).collect())
        }
    }
}

fn require_already_normalized(field: &'static str, value: &str, context: &str) -> Result<(), CompaniesDbError> {
    if normalization::deep_normalize_string(value) != value {
        return Err(CompaniesDbError::NotNormalized { context: context.to_string(), field, value: value.to_string() });
    }
    Ok(())
}

fn require_bud_contained_in_name(bud: &str, name: &str) -> Result<(), CompaniesDbError> {
    let n_name = normalization::deep_normalize_string(name);
    if !n_name.contains(bud) {
        return Err(CompaniesDbError::BudNotContained { bud: bud.to_string(), name: name.to_string(), normalized: n_name });
    }
    Ok(())
}

/// **Bug found and fixed at the root (user confirmed, 2026-08-19)**: the Python original,
/// `_regex_match_name`, used `re.match` (anchored at position 0) — but every real consumer of
/// this same "own Regex" value (`CompanyMatchInfos::compile_from_target_companies`/
/// `compile_from_pandas_df`, via `compile_regex_pattern`) wraps it as `.*pattern.*` specifically
/// to search *anywhere*, unanchored. That mismatch made this validation reject real, correctly-
/// authored patterns (e.g. `\bmaersk` for "ap moller maersk", where "maersk" is the *last* word,
/// not the first) — masked in Python by a second, independent bug (`_regex_match_name`'s check
/// used `pandas.Series.all()` with its default `skipna=True`, which silently drops `None` —
/// `re.match`'s failure return value — instead of treating it as a failed check, so this
/// validation was actually a complete no-op there; verified empirically against real
/// `input_db` data before concluding this, not assumed). This port both fixes the skip bug (a
/// real `Err` here actually stops loading) and searches unanchored, matching how the pattern is
/// actually used — consistent with every real pattern in `input_db/companies/companies.csv`
/// except one genuine data mistake (Airbnb's own `Regex` referenced its ticker, `\babnb`, instead
/// of its name — fixed in the fixture: moved to `companies_additional_regexs.csv`, replaced with
/// `\bairbnb\b`).
fn require_regex_matches_name(pattern: &str, name: &str) -> Result<(), CompaniesDbError> {
    let n_name = normalization::deep_normalize_string(name);
    let re = onig::Regex::new(pattern)
        .map_err(|e| CompaniesDbError::InvalidRegex { pattern: pattern.to_string(), name: name.to_string(), message: e.to_string() })?;
    if re.find(&n_name).is_none() {
        return Err(CompaniesDbError::RegexNotMatching { pattern: pattern.to_string(), name: name.to_string(), normalized: n_name });
    }
    Ok(())
}

fn require_valid_date(value: &str, context: &str) -> Result<(), CompaniesDbError> {
    let parts: Vec<&str> = value.split('-').collect();
    let malformed = || CompaniesDbError::InvalidDate { context: context.to_string(), value: value.to_string() };
    let [y, m, d] = parts[..].try_into().map_err(|_| malformed())?;
    let (y, m, d): (i32, u32, u32) = (
        y.parse().map_err(|_| malformed())?,
        m.parse().map_err(|_| malformed())?,
        d.parse().map_err(|_| malformed())?,
    );
    if y < 1 || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(malformed());
    }
    Ok(())
}

struct CompanyRow {
    name: String,
    bud: Option<String>,
    regex: Option<String>,
}

/// Local equivalent of `freeports_lib::text_filter::matcher::TargetCompanyInput` — kept as a
/// plain local struct rather than importing that type, so this module never links `freeports_lib`
/// as a Rust dependency at all (nothing else here needs the crate, so there's no reason to invite
/// the module-identity trap `compile_from_rows`'s doc comment describes back into `companies_db`
/// just for this shape). Converted to the plain-tuple form `compile_from_rows` actually accepts
/// right before the one call that crosses into Python, in `py_get_target_companies`.
struct TargetCompanyInput {
    name: String,
    regexs: Vec<String>,
    symbols: Vec<String>,
    buds: Vec<String>,
}

fn load_companies(input_db_directory: &Path) -> Result<Vec<CompanyRow>, CompaniesDbError> {
    let path = input_db_directory.join(COMPANIES_DIR).join("companies.csv");
    let df = read_csv(&path)?;
    let names = required_str_column(&df, "Name", &path)?;
    let buds = optional_str_column(&df, "Bud", names.len(), &path)?;
    let regexs = optional_str_column(&df, "Regex", names.len(), &path)?;

    let mut seen = HashSet::new();
    let mut rows = Vec::with_capacity(names.len());
    for i in 0..names.len() {
        let name = names[i].clone();
        if !seen.insert(name.clone()) {
            return Err(CompaniesDbError::Duplicate { path: path.clone(), kind: "company name", value: name });
        }
        if let Some(bud) = &buds[i] {
            require_already_normalized("Bud", bud, &format!("{}, company '{name}'", path.display()))?;
            require_bud_contained_in_name(bud, &name)?;
        }
        if let Some(regex) = &regexs[i] {
            require_regex_matches_name(regex, &name)?;
        }
        rows.push(CompanyRow { name, bud: buds[i].clone(), regex: regexs[i].clone() });
    }
    Ok(rows)
}

/// Shared by `companies_additional_buds.csv` and `companies_additional_regexs.csv` — same shape
/// (`Company name` index column + one value column), same `isin(company_names)` cross-check.
fn load_additional(
    input_db_directory: &Path,
    subdir: &str,
    file_name: &str,
    value_column: &'static str,
    company_names: &HashSet<String>,
) -> Result<Vec<(String, String)>, CompaniesDbError> {
    let path = input_db_directory.join(subdir).join(file_name);
    let df = read_csv(&path)?;
    let companies = required_str_column(&df, "Company name", &path)?;
    let values = required_str_column(&df, value_column, &path)?;
    companies
        .into_iter()
        .zip(values)
        .map(|(company_name, value)| {
            if !company_names.contains(&company_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "company", value: company_name });
            }
            Ok((company_name, value))
        })
        .collect()
}

fn load_lists(input_db_directory: &Path) -> Result<HashSet<String>, CompaniesDbError> {
    let path = input_db_directory.join(LISTS_DIR).join("lists.csv");
    let df = read_csv(&path)?;
    let names = required_str_column(&df, "Name", &path)?;
    let institutions = required_str_column(&df, "Institution", &path)?;
    let dates = required_str_column(&df, "Date", &path)?;

    let mut seen = HashSet::new();
    for (i, name) in names.iter().enumerate() {
        if !seen.insert(name.clone()) {
            return Err(CompaniesDbError::Duplicate { path: path.clone(), kind: "list name", value: name.clone() });
        }
        require_valid_date(&dates[i], &format!("{}, list '{name}'", path.display()))?;
        let _ = &institutions[i]; // presence/type already enforced by `required_str_column`
    }
    Ok(seen)
}

fn load_company_to_list(
    input_db_directory: &Path,
    list_names: &HashSet<String>,
    company_names: &HashSet<String>,
) -> Result<Vec<(String, String)>, CompaniesDbError> {
    let path = input_db_directory.join(LISTS_DIR).join("company_to_list.csv");
    let df = read_csv(&path)?;
    let lists = required_str_column(&df, "List name", &path)?;
    let companies = required_str_column(&df, "Company name", &path)?;
    lists
        .into_iter()
        .zip(companies)
        .map(|(list_name, company_name)| {
            if !list_names.contains(&list_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "list", value: list_name });
            }
            if !company_names.contains(&company_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "company", value: company_name });
            }
            Ok((list_name, company_name))
        })
        .collect()
}

fn load_markets(input_db_directory: &Path) -> Result<HashSet<String>, CompaniesDbError> {
    let path = input_db_directory.join(COMPANIES_DIR).join("markets.csv");
    let df = read_csv(&path)?;
    let names = required_str_column(&df, "Name", &path)?;
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name.clone()) {
            return Err(CompaniesDbError::Duplicate { path: path.clone(), kind: "market name", value: name });
        }
    }
    Ok(seen)
}

fn load_tickers(
    input_db_directory: &Path,
    market_names: &HashSet<String>,
    company_names: &HashSet<String>,
) -> Result<Vec<(String, String, String)>, CompaniesDbError> {
    let path = input_db_directory.join(COMPANIES_DIR).join("tickers.csv");
    let df = read_csv(&path)?;
    let markets = required_str_column(&df, "Market name", &path)?;
    let companies = required_str_column(&df, "Company name", &path)?;
    let symbols = required_str_column(&df, "Symbol", &path)?;

    let symbol_re = onig::Regex::new(r"\A[A-Z]{2,6}\z").unwrap();
    (0..markets.len())
        .map(|i| {
            let (market_name, company_name, symbol) = (markets[i].clone(), companies[i].clone(), symbols[i].clone());
            if !market_names.contains(&market_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "market", value: market_name });
            }
            if !company_names.contains(&company_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "company", value: company_name });
            }
            if symbol_re.find(&symbol).is_none() {
                return Err(CompaniesDbError::InvalidTickerSymbol { path: path.clone(), symbol });
            }
            Ok((market_name, company_name, symbol))
        })
        .collect()
}

#[derive(Default)]
struct CompanyAggregate {
    buds: Vec<String>,
    regexs: Vec<String>,
    list_names: Vec<String>,
    symbols: Vec<String>,
}

/// Rust port of `get_companies_data` + the target-list filter from `get_target_companies`,
/// merged into one pass since nothing outside this module consumes the unfiltered form.
fn load_target_companies(input_db_directory: &Path, target_lists: &[String]) -> Result<Vec<TargetCompanyInput>, CompaniesDbError> {
    let companies = load_companies(input_db_directory)?;
    let company_names: HashSet<String> = companies.iter().map(|c| c.name.clone()).collect();

    let additional_buds = load_additional(input_db_directory, COMPANIES_DIR, "companies_additional_buds.csv", "Bud", &company_names)?;
    let additional_regexs = load_additional(input_db_directory, COMPANIES_DIR, "companies_additional_regexs.csv", "Regex", &company_names)?;
    let list_names = load_lists(input_db_directory)?;
    let company_to_list = load_company_to_list(input_db_directory, &list_names, &company_names)?;
    let market_names = load_markets(input_db_directory)?;
    let tickers = load_tickers(input_db_directory, &market_names, &company_names)?;

    let mut aggregates: HashMap<String, CompanyAggregate> = company_names.iter().map(|n| (n.clone(), CompanyAggregate::default())).collect();
    for company in &companies {
        let agg = aggregates.get_mut(&company.name).unwrap();
        if let Some(bud) = &company.bud {
            agg.buds.push(bud.clone());
        }
        if let Some(regex) = &company.regex {
            agg.regexs.push(regex.clone());
        }
    }
    for (company_name, bud) in additional_buds {
        aggregates.get_mut(&company_name).unwrap().buds.push(bud);
    }
    for (company_name, regex) in additional_regexs {
        aggregates.get_mut(&company_name).unwrap().regexs.push(regex);
    }
    for (list_name, company_name) in company_to_list {
        aggregates.get_mut(&company_name).unwrap().list_names.push(list_name);
    }
    for (_market_name, company_name, symbol) in tickers {
        aggregates.get_mut(&company_name).unwrap().symbols.push(symbol);
    }

    // **Bug found and fixed (2026-08-20)**: this used to `.sort_by(name)` here "for determinism" —
    // wrong, and a real regression caught by the full fixture suite (`Israel Government
    // International` bonds matched to the wrong company, `Israel Government`). Company order is
    // not cosmetic: `match_company`/`match_fast` (`freeports_lib::text_filter::matcher`) is a
    // first-substring-match-wins algorithm, so a shorter name that's a prefix of a longer one
    // (`"Israel Government"` inside `"Israel Government International"`) MUST be checked *after*
    // the longer, more specific one, or it wins incorrectly. The original pandas pipeline
    // preserves `companies.csv`'s own row order through its join chain, and that file happens to
    // list the more specific "International" entry first — sorting alphabetically silently
    // reversed that. `companies` here is already in `companies.csv`'s file order (from
    // `load_companies`), so the fix is simply to keep that order, not impose a new one.
    let target_lists: HashSet<&str> = target_lists.iter().map(String::as_str).collect();
    let result: Vec<TargetCompanyInput> = companies
        .into_iter()
        .filter_map(|company| {
            let agg = aggregates.remove(&company.name).unwrap();
            if !agg.list_names.iter().any(|l| target_lists.contains(l.as_str())) {
                return None;
            }
            Some(TargetCompanyInput { name: company.name, regexs: agg.regexs, symbols: agg.symbols, buds: agg.buds })
        })
        .collect();
    Ok(result)
}

/// `get_target_companies(input_db_directory, target_lists) -> List[CompanyMatchInfos]`. Unlike
/// the Python original (which returns a `pd.DataFrame` that `Algorithm.__call__` then compiles
/// via `compile_from_pandas_df`), this returns the already-compiled matchers directly — see the
/// corresponding simplification in `pipeline.rs`'s `Algorithm::call`.
///
/// `CompanyMatchInfos::compile_from_rows` used to live in the separate `freeports_lib` crate,
/// reached only via `py.import("freeports_lib")...` — never a direct Rust call — specifically to
/// avoid PyO3's per-compiled-module pyclass identity trap (see this module's own doc comment for
/// the full history). Now that `freeports_lib`'s code has been merged into this same crate (Fase
/// E, `agent-memory/rust-native-binary-plan.md`), that trap no longer applies — verified
/// concretely, not assumed, by `py_get_target_companies_returns_instances_of_the_real_standalone_freeports_lib_type`
/// below still passing with a *native* call — so this calls `compile_from_rows` directly.
#[pyfunction]
#[pyo3(name = "get_target_companies", signature = (input_db_directory, target_lists))]
pub fn py_get_target_companies(py: Python<'_>, input_db_directory: PathBuf, target_lists: &Bound<'_, PyAny>) -> PyResult<Py<PyList>> {
    let target_lists: Vec<String> = if let Ok(s) = target_lists.extract::<String>() {
        vec![s]
    } else {
        target_lists.extract()?
    };
    let companies = load_target_companies(&input_db_directory, &target_lists).map_err(companies_db_err)?;
    let rows: Vec<crate::formats_utils::text_filter::matcher::CompanyRowForCompilation> =
        companies.into_iter().map(|c| (c.name, c.regexs, c.symbols, c.buds)).collect();
    // `compile_from_rows` is pure Rust (no Python call inside it — see this module's doc comment)
    // but is a `#[pymethods]` entry point, so its one real failure mode (an invalid Oniguruma
    // pattern in a company's Regex/Symbol list) surfaces as a hand-built `PyException` carrying a
    // raw `(pattern, description)` tuple, not prose. This *is* a genuine, already-constructed
    // `PyErr` crossing a module boundary — unlike every error above, there's no native Rust
    // variant to convert here, so it's handled explicitly by matching the tuple shape and
    // reformatting it, rather than just `?`-propagating the raw tuple as the exception message.
    let compiled = match crate::formats_utils::text_filter::matcher::CompanyMatchInfos::compile_from_rows(rows) {
        Ok(compiled) => compiled,
        Err(e) => {
            return Err(match e.value(py).extract::<(String, String)>() {
                Ok((pattern, message)) => PyValueError::new_err(format!("invalid regex '{pattern}' while compiling target company matchers: {message}")),
                Err(_) => e,
            });
        }
    };
    let items: Vec<Py<crate::formats_utils::text_filter::matcher::CompanyMatchInfos>> =
        compiled.into_iter().map(|c| Py::new(py, c)).collect::<PyResult<_>>()?;
    Ok(PyList::new(py, items)?.unbind())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid `input_db` directory with 2 companies ("Coca Cola" in list TEST, "BlackRock" in
    /// list OTHER, each with an own + an additional Bud/Regex, plus one ticker each) — individual
    /// tests overwrite one file at a time to exercise a specific validation branch, rather than
    /// each hand-building a full fixture from scratch.
    struct Fixture {
        dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(dir.path().join(COMPANIES_DIR)).unwrap();
            std::fs::create_dir_all(dir.path().join(LISTS_DIR)).unwrap();
            let f = Fixture { dir };
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,,\nBlackRock,black,^black ?rock\n");
            f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\nBlackRock,rock\n");
            f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\nBlackRock,rock\n");
            f.write(COMPANIES_DIR, "markets.csv", "Name\nprimary\n");
            f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\nprimary,Coca Cola,COC\nprimary,BlackRock,BLK\n");
            f.write(LISTS_DIR, "lists.csv", "Name,Institution,Date\nTEST,FREEPORTS,2025-01-01\nOTHER,FREEPORTS,2025-01-01\n");
            f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,Coca Cola\nOTHER,BlackRock\n");
            f
        }

        fn write(&self, subdir: &str, file: &str, content: &str) {
            std::fs::write(self.dir.path().join(subdir).join(file), content).unwrap();
        }

        fn root(&self) -> PathBuf {
            self.dir.path().to_path_buf()
        }
    }

    fn names(companies: &[TargetCompanyInput]) -> Vec<&str> {
        companies.iter().map(|c| c.name.as_str()).collect()
    }

    // --- Happy path: filtering + aggregation ---

    #[test]
    fn filters_to_only_companies_in_the_target_list() {
        let f = Fixture::new();
        let result = load_target_companies(&f.root(), &["TEST".to_string()]).unwrap();
        assert_eq!(names(&result), vec!["Coca Cola"]);
    }

    #[test]
    fn own_and_additional_buds_and_regexs_are_both_included() {
        let f = Fixture::new();
        let result = load_target_companies(&f.root(), &["OTHER".to_string()]).unwrap();
        let blackrock = &result[0];
        assert_eq!(blackrock.buds, vec!["black".to_string(), "rock".to_string()]);
        assert_eq!(blackrock.regexs, vec!["^black ?rock".to_string(), "rock".to_string()]);
    }

    #[test]
    fn ticker_symbols_are_aggregated() {
        let f = Fixture::new();
        let result = load_target_companies(&f.root(), &["TEST".to_string()]).unwrap();
        assert_eq!(result[0].symbols, vec!["COC".to_string()]);
    }

    #[test]
    fn multiple_target_lists_include_companies_from_either_without_duplicates() {
        let f = Fixture::new();
        let result = load_target_companies(&f.root(), &["TEST".to_string(), "OTHER".to_string()]).unwrap();
        // companies.csv's own file order (Coca Cola, then BlackRock) — NOT alphabetical. Order is
        // load-bearing for match correctness (see the comment on the removed `.sort_by` in
        // `load_target_companies`), so this pins the real invariant, not just "some order".
        assert_eq!(names(&result), vec!["Coca Cola", "BlackRock"]);
    }

    #[test]
    fn companies_are_returned_in_companies_csv_file_order_not_alphabetical() {
        // Regression test for the fix above: a shorter name that's a prefix of a longer one
        // (here "Israel Government" / "Israel Government International") must stay in the same
        // relative order the CSV declares them in, because `match_company`'s first-substring-
        // match-wins algorithm depends on it — sorting alphabetically would put the shorter
        // prefix first and break real matching (caught by the full fixture suite, not by any
        // test in this file before this one was added).
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nIsrael Government International,,\nIsrael Government,,\n");
        f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\n");
        f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\n");
        f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\n");
        f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,Israel Government International\nTEST,Israel Government\n");
        let result = load_target_companies(&f.root(), &["TEST".to_string()]).unwrap();
        assert_eq!(names(&result), vec!["Israel Government International", "Israel Government"]);
    }

    #[test]
    fn a_target_list_with_no_member_companies_returns_empty() {
        let f = Fixture::new();
        f.write(LISTS_DIR, "lists.csv", "Name,Institution,Date\nEMPTY,FREEPORTS,2025-01-01\n");
        f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\n");
        let result = load_target_companies(&f.root(), &["EMPTY".to_string()]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn a_company_never_referenced_in_company_to_list_is_simply_never_selected() {
        let f = Fixture::new();
        // BlackRock (in OTHER) isn't in TEST, so it must be absent, not error.
        let result = load_target_companies(&f.root(), &["TEST".to_string()]).unwrap();
        assert!(!names(&result).contains(&"BlackRock"));
    }

    #[test]
    fn accented_mixed_case_names_pass_bud_and_regex_validation() {
        // Mirrors the real fixture data (`AP Møller Mærsk` / bud `maersk` / regex `\bmaersk`) —
        // the containment/regex checks operate on the *normalized* form of the name, not the raw
        // identity string, so this must succeed despite the raw name having accents/mixed case.
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nAP Møller Mærsk,maersk,\\bmaersk\n");
        f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\n");
        f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\n");
        f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\n");
        f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,AP Møller Mærsk\n");
        let result = load_target_companies(&f.root(), &["TEST".to_string()]).unwrap();
        assert_eq!(names(&result), vec!["AP Møller Mærsk"]);
    }

    // --- companies.csv validation ---

    #[test]
    fn duplicate_company_name_is_rejected() {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,,\nCoca Cola,,\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn bud_not_already_normalized_is_rejected() {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,COCA,\n"); // uppercase, not normalized
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn bud_not_contained_in_name_is_rejected() {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,pepsi,\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn regex_not_matching_name_is_rejected() {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,,\\bpepsi\\b\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn regex_matching_only_a_later_word_in_the_name_is_accepted() {
        // Pins the fix for the anchoring bug: `\bmaersk` must be accepted for a company whose
        // normalized name is "ap moller maersk" — "maersk" is the *last* word, so this would be
        // rejected by a position-0-anchored match (the Python original's actual, if silently
        // broken, behavior) but must pass under unanchored search, matching how the pattern is
        // really used later (`.*maersk.*`, see `compile_regex_pattern` in `freeports_lib`).
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nAP Moller Maersk,,\\bmaersk\n");
        f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\n");
        f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\n");
        f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\n");
        f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,AP Moller Maersk\n");
        let result = load_target_companies(&f.root(), &["TEST".to_string()]).unwrap();
        assert_eq!(names(&result), vec!["AP Moller Maersk"]);
    }

    // --- cross-reference (isin) checks ---

    #[test]
    fn additional_bud_referencing_an_unknown_company_is_rejected() {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\nNonexistent Co,foo\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn additional_regex_referencing_an_unknown_company_is_rejected() {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\nNonexistent Co,foo\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn company_to_list_referencing_an_unknown_list_is_rejected() {
        let f = Fixture::new();
        f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nNOPE,Coca Cola\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn company_to_list_referencing_an_unknown_company_is_rejected() {
        let f = Fixture::new();
        f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,Nonexistent Co\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn ticker_referencing_an_unknown_market_is_rejected() {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\nnowhere,Coca Cola,COC\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn ticker_referencing_an_unknown_company_is_rejected() {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\nprimary,Nonexistent Co,COC\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    // --- lists.csv / markets.csv structural validation ---

    #[test]
    fn duplicate_list_name_is_rejected() {
        let f = Fixture::new();
        f.write(LISTS_DIR, "lists.csv", "Name,Institution,Date\nTEST,A,2025-01-01\nTEST,B,2025-01-01\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test]
    fn duplicate_market_name_is_rejected() {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "markets.csv", "Name\nprimary\nprimary\n");
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    #[test_case::test_case("2025-13-01"; "month out of range")]
    #[test_case::test_case("2025-01-32"; "day out of range")]
    #[test_case::test_case("not-a-date"; "not numeric")]
    #[test_case::test_case("2025-01"; "missing day")]
    fn invalid_dates_are_rejected(bad_date: &str) {
        let f = Fixture::new();
        f.write(LISTS_DIR, "lists.csv", &format!("Name,Institution,Date\nTEST,FREEPORTS,{bad_date}\nOTHER,FREEPORTS,2025-01-01\n"));
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    // --- ticker symbol shape ---

    #[test_case::test_case("COC"; "valid uppercase")]
    #[test_case::test_case("AB"; "valid at minimum length")]
    #[test_case::test_case("ABCDEF"; "valid at maximum length")]
    fn valid_ticker_symbols_are_accepted(symbol: &str) {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "tickers.csv", &format!("Market name,Company name,Symbol\nprimary,Coca Cola,{symbol}\n"));
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_ok());
    }

    #[test_case::test_case("coc"; "lowercase")]
    #[test_case::test_case("A"; "too short")]
    #[test_case::test_case("ABCDEFG"; "too long")]
    #[test_case::test_case("CO1"; "contains a digit")]
    fn invalid_ticker_symbols_are_rejected(symbol: &str) {
        let f = Fixture::new();
        f.write(COMPANIES_DIR, "tickers.csv", &format!("Market name,Company name,Symbol\nprimary,Coca Cola,{symbol}\n"));
        assert!(load_target_companies(&f.root(), &["TEST".to_string()]).is_err());
    }

    // --- py_get_target_companies (the PyO3-facing entry point) ---

    #[test]
    fn py_get_target_companies_accepts_a_single_string_target_list() {
        let f = Fixture::new();
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let target = "TEST".into_pyobject(py).unwrap().into_any();
            let result = py_get_target_companies(py, f.root(), &target).unwrap();
            assert_eq!(result.bind(py).len(), 1);
        });
    }

    #[test]
    fn py_get_target_companies_accepts_a_list_of_target_lists() {
        let f = Fixture::new();
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let target = PyList::new(py, ["TEST", "OTHER"]).unwrap();
            let result = py_get_target_companies(py, f.root(), target.as_any()).unwrap();
            assert_eq!(result.bind(py).len(), 2);
        });
    }

    #[test]
    fn py_get_target_companies_returns_real_company_match_infos_instances() {
        // Pre-Fase-E (when `freeports_lib` was a separate crate), this test had to fetch the
        // class object via `py.import("freeports_lib")...` and check `is()` identity, because a
        // `CompanyMatchInfos` built by code statically linked into `freeports_engine.so` used to
        // be a *different, incompatible* type from the one `import freeports_lib` returned (see
        // this module's doc comment). Now that both are the same compiled crate,
        // `is_instance_of::<CompanyMatchInfos>()` is the real, direct check — no cross-module
        // identity trap left to route around.
        let f = Fixture::new();
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let target = "TEST".into_pyobject(py).unwrap().into_any();
            let result = py_get_target_companies(py, f.root(), &target).unwrap();
            let first = result.bind(py).get_item(0).unwrap();
            assert!(
                first.is_instance_of::<crate::formats_utils::text_filter::matcher::CompanyMatchInfos>(),
                "expected an instance of CompanyMatchInfos"
            );
        });
    }

    // --- a reasonable stress test ---

    #[test]
    fn stress_500_companies_across_10_lists_filters_and_aggregates_correctly() {
        let f = Fixture::new();
        let mut companies_csv = String::from("Name,Bud,Regex\n");
        let mut company_to_list_csv = String::from("List name,Company name\n");
        let mut lists_csv = String::from("Name,Institution,Date\n");
        for l in 0..10 {
            lists_csv.push_str(&format!("List{l},FREEPORTS,2025-01-01\n"));
        }
        for i in 0..500 {
            let name = format!("Company{i}");
            companies_csv.push_str(&format!("{name},,\n"));
            company_to_list_csv.push_str(&format!("List{},{name}\n", i % 10));
        }
        f.write(COMPANIES_DIR, "companies.csv", &companies_csv);
        f.write(LISTS_DIR, "lists.csv", &lists_csv);
        f.write(LISTS_DIR, "company_to_list.csv", &company_to_list_csv);
        f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\n");
        f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\n");
        f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\n");

        let result = load_target_companies(&f.root(), &["List3".to_string()]).unwrap();
        assert_eq!(result.len(), 50); // every 10th company, 500/10
        assert!(result.iter().all(|c| c.name.starts_with("Company")));
        assert!(result.iter().any(|c| c.name == "Company3")); // 3 % 10 == 3, so it's in List3
        assert!(!result.iter().any(|c| c.name == "Company4")); // 4 % 10 == 4, so it's in List4, not List3
    }
}
