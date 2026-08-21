//! Rust port of `packages/freeports_core/src/freeports/_internals/formats/repo/orchestration.py`'s
//! `get_schedule`/`get_pageclassify_pipelines`/`get_mapping` — the formats-repository's
//! `content/orchestration/*.csv`-backed page-classification-to-pipeline mapping and page-processing
//! order.
//!
//! See `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`, Milestone 1
//! Step 1.5, for the full design context, and `orchestration.py` itself for ground-truth semantics
//! (`get_schedule`, `get_algorithms_schedule`(_schema), `get_pageclassify_pipelines`,
//! `get_pageclassify_overwrite`(_schema), `get_mapping`, `get_mapping_table`(_schema)). This is an
//! **independent Rust reimplementation**, not a call into Python, for everything except
//! `get_mapping`'s `KeyError` fallback branch — see below.
//!
//! Reuses [`crate::formats_repo::id_format::derive_format_name`]/
//! [`crate::formats_repo::id_format::derive_pipeline_name`] (Step 1.1) for `pageclassify_overwrite
//! .csv`'s and `mapping.csv`'s `ID` column, mirroring `pipelines_definition.py`'s
//! `add_format_name`/`add_pipeline_name` — `algorithms_schedule.csv` needs neither, it has a
//! literal `Format name` column already (confirmed by reading `orchestration.py` directly, per the
//! requirements note).
//!
//! **`get_mapping`'s `KeyError` fallback branch is the one place this module still touches
//! Python**: when `format_name` has no rows in `mapping.csv`, the Python original falls back to
//! `pipelines_acquisition.get_pipelines(formats_repo_dir, format_name)` (structured/semistructured/
//! unstructured pipeline acquisition — not native, and not in scope for this port; see the
//! requirements note's Milestone 2 discussion) filtered against
//! [`get_pageclassify_pipelines`]'s own result. This one call self-attaches (`Python::attach`),
//! mirroring `detect_format`'s pre-Step-1.3 pattern, rather than taking a `Python<'_>` token from
//! callers — nothing above `get_mapping` needs a Python object back.
//!
//! **Pre-implementation scaffolding note (test-writer phase)**: every function body below is a
//! `todo!()` stub — this file's job at this stage is only to give the test suite below a real
//! type/signature surface to compile against (`cargo test --lib` must compile cleanly even though
//! every test currently panics/fails). `implementer` fills these in (including the private
//! `read_algorithms_schedule`/`read_pageclassify_overwrite`/`read_mapping_table` loaders the plan
//! calls for — left undeclared here since no test calls them directly, exactly like `metadata.rs`'s
//! own private `open_csv`/`required_column` helpers); per this workspace's TDD discipline, tests
//! are the contract and must not be edited to make them pass.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::formats_repo::id_format;

const CONTENT_DIR: &str = "content";
const ORCHESTRATION_DIR: &str = "orchestration";

/// Mirrors the union of failure modes `_algorithms_schedule_schema`/`_pageclassify_overwrite_schema`/
/// `_mapping_schema`'s `.validate()` calls (plus the uncaught `KeyError`s a missing required column
/// raises) can produce in the Python original, as plain Rust variants rather than an attempt at
/// pandera-shape fidelity (same confirmed policy as [`crate::formats_repo::metadata::MetadataError`]).
#[derive(Debug, Clone, PartialEq)]
pub enum OrchestrationError {
    /// One of `content/orchestration/{algorithms_schedule,pageclassify_overwrite,mapping}.csv`
    /// doesn't exist under the given `formats_repo_dir`. Carries the full path that was missing.
    MissingCsv(PathBuf),
    /// A CSV row (or the header itself) couldn't be read/interpreted at all — a required column
    /// missing entirely (mirrors an uncaught Python `KeyError` from e.g.
    /// `df["Filter next iteration"]` or `df.set_index(["Format name", "Page type"])`), or a row
    /// with the wrong number of fields. `line` is the 1-based position of the offending row within
    /// the CSV (header line excluded, i.e. the first data row is line 1; `0` for a whole-file/
    /// header-level problem), `reason` is a short, human-readable explanation.
    MalformedRow { line: usize, reason: String },
    /// A row names a `Format name` (literal, for `algorithms_schedule.csv`; derived from `ID` for
    /// the other two) that isn't present in the `format_names` slice passed in (mirrors each
    /// schema's `pa.Check.isin(format_names)` failing on its index).
    UnknownFormatName(String),
    /// A `pageclassify_overwrite.csv`/`mapping.csv` row's `ID` doesn't match the expected
    /// one-to-one, no-index `ID` shape (mirrors `column_id_format_pipe(FKRelation.ONE_TO_ONE)`'s
    /// check failing — e.g. a trailing `/<digits>` index suffix, which this relation forbids).
    InvalidId(String),
    /// A `pageclassify_overwrite.csv` row's `ID` has no `(pipeline)` group at all, so the derived
    /// `Pipeline name` is null — mirrors `_pageclassify_overwrite_schema`'s `Pipeline name` column
    /// being `nullable=False` (unlike `mapping.csv`, whose `get_mapping_table` explicitly
    /// `fillna("")`s this same situation instead of rejecting it).
    InvalidPipelineName(String),
    /// [`get_mapping`]'s Python fallback (`pipelines_acquisition.get_pipelines`) raised. Carries
    /// the Python exception's string form; the full traceback is already printed (`err.print(py)`)
    /// at the point of failure, mirroring `FreeportsConfigError::Python`'s pre-removal convention —
    /// this is a **different module**, so a `Python` variant here is not in tension with that
    /// removal (Step 1.7, scoped to `freeports_config.rs` only).
    Python(String),
}

impl std::fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrchestrationError::MissingCsv(path) => write!(f, "missing formats-repository CSV file: {}", path.display()),
            OrchestrationError::MalformedRow { line, reason } => write!(f, "malformed row at line {line}: {reason}"),
            OrchestrationError::UnknownFormatName(name) => write!(f, "unknown format name: {name}"),
            OrchestrationError::InvalidId(id) => write!(f, "ID '{id}' does not match the expected ID pattern"),
            OrchestrationError::InvalidPipelineName(id) => {
                write!(f, "ID '{id}' has no pipeline name and none was defaulted")
            }
            // The full traceback is already printed (`err.print(py)`) right where this was
            // generated — see `From<PyErr>`'s doc comment below — so this is a short, deliberately
            // redundant recap, matching `FreeportsConfigError::Python`'s own convention.
            OrchestrationError::Python(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for OrchestrationError {}

/// The one place a `PyErr` surfacing anywhere in this module turns into this crate's own error
/// type — printed here (via a fresh, cheap re-`attach` on the same thread; PyO3 attaches are
/// reentrant) rather than at every individual `?` site upstream, mirroring
/// `FreeportsConfigError`'s own `From<PyErr>` (`cli/freeports_config.rs`) — the same convention,
/// in a different module, per this module's own doc comment.
impl From<PyErr> for OrchestrationError {
    fn from(e: PyErr) -> Self {
        Python::attach(|py| e.print(py));
        OrchestrationError::Python(e.to_string())
    }
}

/// Rust port of `orchestration.py`'s `get_schedule`: builds the ordered list of page-type groups
/// defining `format_name`'s processing steps from `content/orchestration/algorithms_schedule.csv`
/// (literal `Format name`/`Page type`/`Filter next iteration` columns, no `ID` derivation). Each
/// row's `Page type` is added to the current (last) group; a truthy `Filter next iteration` (a
/// blank cell defaults to `false`, matching `df["Filter next iteration"].fillna(False)`) starts a
/// new, initially empty, group for subsequent rows — including, if the *last* row filters, leaving
/// a trailing empty group in the result. When `format_name` has no rows in the schedule at all
/// (mirrors the Python original's `except KeyError`), falls back to a single group containing every
/// page type in [`get_mapping`]'s result for `format_name`.
/// Opens `<formats_repo_dir>/content/orchestration/<file_name>` as a `csv::Reader`, or
/// `MissingCsv` if it doesn't exist on disk. Mirrors `metadata.rs`'s own `open_csv` helper.
///
/// `flexible` controls whether a row with fewer fields than the header/first record is tolerated
/// (mirroring pandas' own tolerance for short trailing rows, `NaN`-filling the missing cells) or
/// rejected outright (`csv::Reader`'s strict-mode default). Only pass `true` for a CSV whose
/// trailing column(s) are genuinely optional end-to-end (i.e. every reader of it already treats a
/// present-but-empty cell the same as an absent one) — a `false` caller relies on this function to
/// keep catching a row that's missing a *required* trailing field, not just to preserve today's
/// default.
fn open_csv(formats_repo_dir: &Path, file_name: &str, flexible: bool) -> Result<csv::Reader<std::fs::File>, OrchestrationError> {
    let path = formats_repo_dir.join(CONTENT_DIR).join(ORCHESTRATION_DIR).join(file_name);
    if !path.exists() {
        return Err(OrchestrationError::MissingCsv(path));
    }
    csv::ReaderBuilder::new()
        .flexible(flexible)
        .from_path(&path)
        .map_err(|e| OrchestrationError::MalformedRow { line: 0, reason: e.to_string() })
}

/// Looks up a required column's index in `headers`, or a `MalformedRow` naming the missing
/// column. Mirrors `metadata.rs`'s own `required_column` helper.
fn required_column(headers: &csv::StringRecord, name: &str) -> Result<usize, OrchestrationError> {
    headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| OrchestrationError::MalformedRow { line: 0, reason: format!("missing required column '{name}'") })
}

/// Checks a row's derived/literal `Format name` against the known `format_names` slice, mirroring
/// every one of the 3 CSV schemas' index `pa.Check.isin(format_names)`.
fn check_known_format_name(format_name: &str, format_names: &[String]) -> Result<(), OrchestrationError> {
    if format_names.iter().any(|n| n == format_name) {
        Ok(())
    } else {
        Err(OrchestrationError::UnknownFormatName(format_name.to_string()))
    }
}

/// Mirrors `df["Filter next iteration"].fillna(False)` followed by pandera's `pd.BooleanDtype`
/// coercion: a blank cell is `false`, `"TRUE"`/`"FALSE"` (case-insensitive) parse as expected.
fn parse_filter_next_iteration(raw: &str, line: usize) -> Result<bool, OrchestrationError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => Ok(false),
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(OrchestrationError::MalformedRow {
            line,
            reason: format!("invalid 'Filter next iteration' value '{other}'"),
        }),
    }
}

/// One row of `content/orchestration/algorithms_schedule.csv`.
struct ScheduleRow {
    format_name: String,
    page_type: String,
    filter_next_iteration: bool,
}

/// Rust port of `orchestration.py`'s `get_algorithms_schedule`: loads and validates every row of
/// `content/orchestration/algorithms_schedule.csv` (literal `Format name`/`Page type`/`Filter next
/// iteration` columns), eagerly checking every row's `Format name` against `format_names` up
/// front, mirroring the whole-DataFrame `pa.DataFrameSchema.validate` call in the Python original.
fn read_algorithms_schedule(
    formats_repo_dir: &Path,
    format_names: &[String],
) -> Result<Vec<ScheduleRow>, OrchestrationError> {
    // `Filter next iteration` is the one, genuinely optional trailing column here (mirrors
    // `df["Filter next iteration"].fillna(False)`; `parse_filter_next_iteration` below already
    // treats a present-but-empty cell and an absent one identically), so a ragged row missing
    // just this cell is legal, matching pandas' own tolerance for short trailing rows — the real,
    // checked-in `algorithms_schedule.csv` has exactly one such row.
    let mut reader = open_csv(formats_repo_dir, "algorithms_schedule.csv", true)?;
    let headers = reader
        .headers()
        .map_err(|e| OrchestrationError::MalformedRow { line: 0, reason: e.to_string() })?
        .clone();
    let format_name_idx = required_column(&headers, "Format name")?;
    let page_type_idx = required_column(&headers, "Page type")?;
    let filter_idx = required_column(&headers, "Filter next iteration")?;

    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let line = i + 1;
        let record = record.map_err(|e| OrchestrationError::MalformedRow { line, reason: e.to_string() })?;
        let format_name = record.get(format_name_idx).unwrap_or("").to_string();
        check_known_format_name(&format_name, format_names)?;
        let page_type = record.get(page_type_idx).unwrap_or("").to_string();
        let filter_next_iteration = parse_filter_next_iteration(record.get(filter_idx).unwrap_or(""), line)?;
        rows.push(ScheduleRow { format_name, page_type, filter_next_iteration });
    }
    Ok(rows)
}

pub fn get_schedule(
    formats_repo_dir: &Path,
    format_name: &str,
    format_names: &[String],
) -> Result<Vec<HashSet<String>>, OrchestrationError> {
    let rows = read_algorithms_schedule(formats_repo_dir, format_names)?;
    let matching: Vec<&ScheduleRow> = rows.iter().filter(|r| r.format_name == format_name).collect();

    if matching.is_empty() {
        let mapping = get_mapping(formats_repo_dir, format_name, format_names)?;
        return Ok(vec![mapping.into_keys().collect()]);
    }

    let mut schedule = vec![HashSet::new()];
    for row in matching {
        schedule.last_mut().expect("schedule always has at least one group").insert(row.page_type.clone());
        if row.filter_next_iteration {
            schedule.push(HashSet::new());
        }
    }
    Ok(schedule)
}

/// Rust port of `orchestration.py`'s `get_pageclassify_pipelines`: the set of pipeline names used
/// to override page classification for `format_name`, read from
/// `content/orchestration/pageclassify_overwrite.csv` (`ID` column only; `Format name`/
/// `Pipeline name` are both derived from it via [`crate::formats_repo::id_format::derive_format_name`]/
/// [`derive_pipeline_name`](crate::formats_repo::id_format::derive_pipeline_name)), grouped by
/// `Format name` and aggregated into a set (mirrors `df.groupby(by="Format name").agg({"Pipeline
/// name": set})`). When `format_name` has no rows at all (mirrors the Python original's `except
/// KeyError`) — which, as of this port, is *every* format, since the real formats repo's
/// `pageclassify_overwrite.csv` is currently header-only — returns the **singleton set containing
/// the empty string** (`{""}`, mirroring `set([""])`), not an empty set.
/// Rust port of `orchestration.py`'s `get_pageclassify_overwrite`: loads and validates every row
/// of `content/orchestration/pageclassify_overwrite.csv` (`ID` column only), deriving `Format
/// name`/`Pipeline name` from it (mirrors `add_format_name`/`add_pipeline_name(df)`, no default —
/// a missing pipeline group is rejected, not defaulted, unlike `mapping.csv`). Returns
/// `(Format name, Pipeline name)` pairs in CSV row order.
fn read_pageclassify_overwrite(
    formats_repo_dir: &Path,
    format_names: &[String],
) -> Result<Vec<(String, String)>, OrchestrationError> {
    // No optional trailing column here (`ID` is the only one) — keep strict mode.
    let mut reader = open_csv(formats_repo_dir, "pageclassify_overwrite.csv", false)?;
    let headers = reader
        .headers()
        .map_err(|e| OrchestrationError::MalformedRow { line: 0, reason: e.to_string() })?
        .clone();
    let id_idx = required_column(&headers, "ID")?;

    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let line = i + 1;
        let record = record.map_err(|e| OrchestrationError::MalformedRow { line, reason: e.to_string() })?;
        let id = record.get(id_idx).unwrap_or("").to_string();
        if !id_format::id_matches_expandable_no_index(&id) {
            return Err(OrchestrationError::InvalidId(id));
        }
        let format_name = id_format::derive_format_name(&id);
        let pipeline_name = id_format::derive_pipeline_name(&id, None)
            .ok_or_else(|| OrchestrationError::InvalidPipelineName(id.clone()))?;
        check_known_format_name(&format_name, format_names)?;
        rows.push((format_name, pipeline_name));
    }
    Ok(rows)
}

pub fn get_pageclassify_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
    format_names: &[String],
) -> Result<HashSet<String>, OrchestrationError> {
    let rows = read_pageclassify_overwrite(formats_repo_dir, format_names)?;

    let mut pipeline_names = HashSet::new();
    let mut found = false;
    for (row_format_name, pipeline_name) in rows {
        if row_format_name == format_name {
            found = true;
            pipeline_names.insert(pipeline_name);
        }
    }

    if !found {
        return Ok(HashSet::from([String::new()]));
    }
    Ok(pipeline_names)
}

/// Rust port of `orchestration.py`'s `get_mapping`: the mapping from page type to the set of
/// pipeline names responsible for it, for `format_name`, read from
/// `content/orchestration/mapping.csv` (`ID` + literal `Page type` columns; `Format name`/
/// `Pipeline name` are derived from `ID`, with a missing pipeline group defaulting to the empty
/// string rather than being rejected — mirrors `df["Pipeline name"].fillna("")`), grouped by
/// `(Format name, Page type)` and aggregated into a set per page type (mirrors
/// `df.groupby(["Format name", "Page type"]).agg({"Pipeline name": set})`).
///
/// When `format_name` has no rows at all (mirrors the Python original's `except KeyError`), falls
/// back to Python: calls `pipelines_acquisition.get_pipelines(formats_repo_dir, format_name)` for
/// the set of pipeline names actually defined for this format, then returns `{pipeline_name:
/// {pipeline_name}}` for every one of those names that is **not** already in
/// [`get_pageclassify_pipelines`]'s result for this format (mirrors `{pn: set([pn]) for pn in pp if
/// pn not in pcpp}`) — see this module's doc comment for why this one branch touches Python.
/// Rust port of `orchestration.py`'s `get_mapping_table`: loads and validates every row of
/// `content/orchestration/mapping.csv` (`ID` + literal `Page type` columns), deriving `Format
/// name`/`Pipeline name` from `ID` (mirrors `add_format_name`/`add_pipeline_name(df)` +
/// `df["Pipeline name"].fillna("")` — unlike `pageclassify_overwrite.csv`, a missing pipeline
/// group defaults to the empty string rather than being rejected). Returns `(Format name, Page
/// type, Pipeline name)` triples in CSV row order.
fn read_mapping_table(
    formats_repo_dir: &Path,
    format_names: &[String],
) -> Result<Vec<(String, String, String)>, OrchestrationError> {
    // `Page type` is a required literal column here (unlike algorithms_schedule.csv's `Filter
    // next iteration`) — a row ragged on it should still error, not silently default to "", so
    // keep strict mode.
    let mut reader = open_csv(formats_repo_dir, "mapping.csv", false)?;
    let headers = reader
        .headers()
        .map_err(|e| OrchestrationError::MalformedRow { line: 0, reason: e.to_string() })?
        .clone();
    let id_idx = required_column(&headers, "ID")?;
    let page_type_idx = required_column(&headers, "Page type")?;

    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let line = i + 1;
        let record = record.map_err(|e| OrchestrationError::MalformedRow { line, reason: e.to_string() })?;
        let id = record.get(id_idx).unwrap_or("").to_string();
        if !id_format::id_matches_expandable_no_index(&id) {
            return Err(OrchestrationError::InvalidId(id));
        }
        let format_name = id_format::derive_format_name(&id);
        let pipeline_name = id_format::derive_pipeline_name(&id, Some("")).unwrap_or_default();
        let page_type = record.get(page_type_idx).unwrap_or("").to_string();
        check_known_format_name(&format_name, format_names)?;
        rows.push((format_name, page_type, pipeline_name));
    }
    Ok(rows)
}

/// [`get_mapping`]'s Python fallback: calls `pipelines_acquisition.get_pipelines(formats_repo_dir,
/// format_name)` and returns the set of pipeline names it defines for `format_name` (mirrors `pp =
/// get_pipelines(formats_repo_dir, format_name)` then iterating `pp`'s dict keys). Self-attaches
/// (`Python::attach`), mirroring `detect_format`'s pre-Step-1.3 pattern — see this module's own
/// doc comment for why.
fn python_pipeline_names(formats_repo_dir: &Path, format_name: &str) -> Result<HashSet<String>, OrchestrationError> {
    Python::attach(|py| -> Result<HashSet<String>, OrchestrationError> {
        let pipelines_acquisition =
            py.import("freeports._internals.formats.repo.algorithms.pipelines_acquisition")?;
        let pipelines_map = pipelines_acquisition.call_method1("get_pipelines", (formats_repo_dir, format_name))?;
        let pipelines_map = pipelines_map.cast::<PyDict>().map_err(PyErr::from)?;

        let mut pipeline_names = HashSet::new();
        for key in pipelines_map.keys().iter() {
            pipeline_names.insert(key.extract::<String>()?);
        }
        Ok(pipeline_names)
    })
}

pub fn get_mapping(
    formats_repo_dir: &Path,
    format_name: &str,
    format_names: &[String],
) -> Result<HashMap<String, HashSet<String>>, OrchestrationError> {
    let rows = read_mapping_table(formats_repo_dir, format_names)?;

    let mut mapping: HashMap<String, HashSet<String>> = HashMap::new();
    let mut found = false;
    for (row_format_name, page_type, pipeline_name) in rows {
        if row_format_name == format_name {
            found = true;
            mapping.entry(page_type).or_default().insert(pipeline_name);
        }
    }

    if !found {
        let pipeline_names = python_pipeline_names(formats_repo_dir, format_name)?;
        let page_classify_pipelines = get_pageclassify_pipelines(formats_repo_dir, format_name, format_names)?;
        return Ok(pipeline_names
            .into_iter()
            .filter(|pn| !page_classify_pipelines.contains(pn))
            .map(|pn| (pn.clone(), HashSet::from([pn])))
            .collect());
    }
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ============================================================
    // Fixture helpers
    // ============================================================

    /// Writes `<dir>/content/orchestration/<file_name>` with the given raw CSV text, creating the
    /// `content/orchestration/` subfolder as needed. Deliberately takes raw CSV text (rather than
    /// baking in one row shape) since this module's tests need many different row shapes,
    /// including malformed ones a stricter helper couldn't produce — same discipline as
    /// `metadata.rs`'s `write_formats_csv`/`write_url_mapping_csv`.
    fn write_orchestration_csv(dir: &Path, file_name: &str, csv_text: &str) {
        let orchestration_dir = dir.join("content").join("orchestration");
        std::fs::create_dir_all(&orchestration_dir).unwrap();
        std::fs::write(orchestration_dir.join(file_name), csv_text).unwrap();
    }

    fn write_algorithms_schedule_csv(dir: &Path, csv_text: &str) {
        write_orchestration_csv(dir, "algorithms_schedule.csv", csv_text);
    }

    fn write_pageclassify_overwrite_csv(dir: &Path, csv_text: &str) {
        write_orchestration_csv(dir, "pageclassify_overwrite.csv", csv_text);
    }

    fn write_mapping_csv(dir: &Path, csv_text: &str) {
        write_orchestration_csv(dir, "mapping.csv", csv_text);
    }

    const ALGORITHMS_SCHEDULE_HEADER: &str = "Format name,Page type,Filter next iteration";
    const PAGECLASSIFY_OVERWRITE_HEADER: &str = "ID";
    const MAPPING_HEADER: &str = "ID,Page type";

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    /// Builds the minimal on-disk `content/algorithms/{structured,semistructured}` layout needed
    /// for `pipelines_acquisition.get_pipelines` (the function [`get_mapping`]'s fallback branch
    /// calls) to run to completion **without erroring**, for a repo where every format has zero
    /// pipelines defined anywhere. Every file here is header-only/empty, so on its own this fixture
    /// makes the fallback branch resolve to an empty map — see
    /// [`add_structured_investments_pipeline_row`] to make one format resolve to a real,
    /// non-trivial pipeline instead.
    ///
    /// Ground truth for exactly which files are required (read directly off
    /// `pipelines_acquisition.get_pipelines`'s 3 callees — `structured`/`semistructured`/
    /// `unstructured` `acquisition.py`, not guessed):
    /// - **`structured`**: `structured/acquisition.py` unconditionally `pd.read_csv`s
    ///   `content/algorithms/structured/investments/{args,additional_args,deselection_lists,
    ///   partial_pipes}.csv` and `content/algorithms/structured/page_classify/args.csv` with no
    ///   existence check first — a missing file is an uncaught `FileNotFoundError` (the `except
    ///   KeyError` around the later `.loc[format_name]` lookup only catches "format not present",
    ///   not "file missing"), so all 5 must exist; header-only is enough.
    /// - **`semistructured`**: `semistructured/acquisition.py` likewise unconditionally reads
    ///   `content/algorithms/semistructured/formats_mapping.csv`, and — for *every* one of the 3
    ///   segment types, before the per-pipeline loop even runs — `content/algorithms/
    ///   semistructured/args/{pdf_extract,text_filter,deserialize}.yaml` via a bare
    ///   `yaml.safe_load(path.open("r"))`; all 4 files must exist, though the YAML ones can be
    ///   empty (an empty file parses to `None`, never indexed into when there are zero pipeline
    ///   rows for any format).
    /// - **`unstructured`**: needs **no on-disk presence at all**. `unstructured/acquisition.py`'s
    ///   `get_module` only ever calls `.is_file()` on two candidate paths and returns `None` if
    ///   neither exists — no directory listing, no read attempt — so `content/algorithms/
    ///   unstructured/` doesn't even need to exist. (Confirmed empirically and by reading
    ///   `unstructured/acquisition.py` directly; the implementation plan's own phrasing of this
    ///   fixture requirement as "`content/algorithms/{structured,unstructured}`" turned out to have
    ///   missed `semistructured`, the one directory that actually needs anything beyond
    ///   `structured`.)
    fn python_acquisition_fixture(dir: &Path) {
        let investments_dir = dir.join("content/algorithms/structured/investments");
        std::fs::create_dir_all(&investments_dir).unwrap();
        std::fs::write(
            investments_dir.join("args.csv"),
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n",
        )
        .unwrap();
        std::fs::write(
            investments_dir.join("additional_args.csv"),
            "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous\n",
        )
        .unwrap();
        std::fs::write(investments_dir.join("deselection_lists.csv"), "ID,Deselection set\n").unwrap();
        std::fs::write(investments_dir.join("partial_pipes.csv"), "ID,pdf_extract,text_filter,deserialize\n").unwrap();

        let page_classify_dir = dir.join("content/algorithms/structured/page_classify");
        std::fs::create_dir_all(&page_classify_dir).unwrap();
        std::fs::write(page_classify_dir.join("args.csv"), "ID,Header set,Class\n").unwrap();

        let semistructured_dir = dir.join("content/algorithms/semistructured");
        std::fs::create_dir_all(&semistructured_dir).unwrap();
        std::fs::write(semistructured_dir.join("formats_mapping.csv"), "ID,pdf_extract,text_filter,deserialize\n").unwrap();

        let args_dir = semistructured_dir.join("args");
        std::fs::create_dir_all(&args_dir).unwrap();
        std::fs::write(args_dir.join("pdf_extract.yaml"), "").unwrap();
        std::fs::write(args_dir.join("text_filter.yaml"), "").unwrap();
        std::fs::write(args_dir.join("deserialize.yaml"), "").unwrap();
    }

    /// Appends one real-shaped row to [`python_acquisition_fixture`]'s `investments/args.csv`, so
    /// `pipelines_acquisition.get_pipelines(dir, id)` resolves a genuine, non-empty `"investments"`
    /// pipeline for `id` (the bare `ID`, with no `(pipeline)` group, defaults to pipeline name
    /// `"investments"` — `structured/pipelines/investments.py`'s own `pipeline_default`) instead of
    /// the trivial empty-everywhere case [`python_acquisition_fixture`] produces on its own. Line
    /// shape (`Subfund set`/`Currency set`/`Body set` values) copied from a real row in
    /// `analysis_finance_reports_formats/content/algorithms/structured/investments/args.csv`, since
    /// an empty/`NaN` `Body set` cell crashes `pdfline_selection_from_str` with a `TypeError`
    /// before it ever reaches the `KeyError`-catching code this fixture is built to exercise.
    fn add_structured_investments_pipeline_row(dir: &Path, id: &str) {
        let args_path = dir.join("content/algorithms/structured/investments/args.csv");
        let mut contents = std::fs::read_to_string(&args_path).unwrap();
        contents.push_str(&format!("{id},ArialMT,ArialNarrow,ArialNarrow,1,2,3,,\n"));
        std::fs::write(&args_path, contents).unwrap();
    }

    fn hashset(values: &[&str]) -> HashSet<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    // ============================================================
    // get_schedule
    // ============================================================

    #[test]
    fn get_schedule_errors_when_algorithms_schedule_csv_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let expected_path = dir.path().join("content").join("orchestration").join("algorithms_schedule.csv");
        let result = get_schedule(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Err(OrchestrationError::MissingCsv(expected_path)));
    }

    #[test]
    fn get_schedule_errors_on_a_missing_filter_next_iteration_column() {
        let dir = tempfile::tempdir().unwrap();
        // Real Python behaviour: an uncaught KeyError from `df["Filter next iteration"]`.
        write_algorithms_schedule_csv(dir.path(), "Format name,Page type\nTESTFMT-EN24,pageA\n");
        let result = get_schedule(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert!(matches!(result, Err(OrchestrationError::MalformedRow { .. })));
    }

    #[test]
    fn get_schedule_errors_when_a_row_names_an_unknown_format() {
        let dir = tempfile::tempdir().unwrap();
        write_algorithms_schedule_csv(
            dir.path(),
            &format!("{ALGORITHMS_SCHEDULE_HEADER}\nGHOST-EN24,pageA,\n"),
        );
        let result = get_schedule(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Err(OrchestrationError::UnknownFormatName("GHOST-EN24".to_string())));
    }

    #[test]
    fn get_schedule_groups_every_page_into_one_step_when_nothing_filters_the_next_iteration() {
        let dir = tempfile::tempdir().unwrap();
        write_algorithms_schedule_csv(
            dir.path(),
            &format!("{ALGORITHMS_SCHEDULE_HEADER}\nTESTFMT-EN24,pageA,\nTESTFMT-EN24,pageB,\n"),
        );
        let result = get_schedule(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"])).unwrap();
        assert_eq!(result, vec![hashset(&["pageA", "pageB"])]);
    }

    #[test]
    fn get_schedule_starts_a_new_step_after_a_row_that_filters_the_next_iteration() {
        let dir = tempfile::tempdir().unwrap();
        write_algorithms_schedule_csv(
            dir.path(),
            &format!("{ALGORITHMS_SCHEDULE_HEADER}\nTESTFMT-EN24,pageA,TRUE\nTESTFMT-EN24,pageB,\n"),
        );
        let result = get_schedule(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"])).unwrap();
        assert_eq!(result, vec![hashset(&["pageA"]), hashset(&["pageB"])]);
    }

    #[test]
    fn get_schedule_treats_a_blank_filter_next_iteration_cell_as_false() {
        let dir = tempfile::tempdir().unwrap();
        // A blank cell in the middle of the schedule must not start a new step - only an explicit
        // TRUE does (fillna(False), not fillna(True)).
        write_algorithms_schedule_csv(
            dir.path(),
            &format!(
                "{ALGORITHMS_SCHEDULE_HEADER}\nTESTFMT-EN24,pageA,TRUE\nTESTFMT-EN24,pageB,\nTESTFMT-EN24,pageC,FALSE\nTESTFMT-EN24,pageD,\n"
            ),
        );
        let result = get_schedule(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"])).unwrap();
        assert_eq!(result, vec![hashset(&["pageA"]), hashset(&["pageB", "pageC", "pageD"])]);
    }

    #[test]
    fn get_schedule_tolerates_a_ragged_row_missing_the_trailing_filter_next_iteration_cell() {
        // Regression pin for a real bug found during Milestone 1 Step 1.8's full verification:
        // the real, checked-in analysis_finance_reports_formats/content/orchestration/
        // algorithms_schedule.csv has one legal-but-ragged row (line 39,
        // "MEDIOLANUM-IT24.A,inv_managers_begin") that omits the trailing "Filter next
        // iteration" cell *and its comma* entirely - not just an empty final field. This is a
        // pandas-tolerated CSV shorthand: pd.read_csv fills the missing cell with NaN, and
        // get_algorithms_schedule's own `.fillna(False)` (mirrored here by
        // parse_filter_next_iteration's own "" -> false case, exercised by the sibling
        // *_treats_a_blank_filter_next_iteration_cell_as_false test above) treats that the same
        // as an explicit blank/FALSE cell. csv::Reader's default *strict* mode instead rejects
        // any row with fewer fields than the first record established, breaking every format's
        // get_schedule/Algorithm::load, not just this one row's format - reproduced here with a
        // 2-field row amid otherwise-3-field rows, deliberately not just an empty trailing
        // field, since that shape parses fine even in strict mode and would not have caught the
        // regression.
        let dir = tempfile::tempdir().unwrap();
        write_algorithms_schedule_csv(
            dir.path(),
            &format!("{ALGORITHMS_SCHEDULE_HEADER}\nTESTFMT-EN24,pageA,TRUE\nTESTFMT-EN24,pageB\n"),
        );
        let result = get_schedule(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Ok(vec![hashset(&["pageA"]), hashset(&["pageB"])]));
    }

    #[test]
    fn get_schedule_produces_a_trailing_empty_step_when_the_last_row_filters_the_next_iteration() {
        let dir = tempfile::tempdir().unwrap();
        write_algorithms_schedule_csv(dir.path(), &format!("{ALGORITHMS_SCHEDULE_HEADER}\nTESTFMT-EN24,pageA,TRUE\n"));
        let result = get_schedule(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"])).unwrap();
        assert_eq!(result, vec![hashset(&["pageA"]), HashSet::new()]);
    }

    #[test]
    fn get_schedule_falls_back_to_get_mappings_page_types_when_the_format_has_no_schedule_rows() {
        let dir = tempfile::tempdir().unwrap();
        // Schedule has rows only for a different format; mapping.csv has real rows for the format
        // being queried, so get_schedule's own `except KeyError` -> get_mapping fallback is
        // exercised without needing the deeper pipelines_acquisition Python fallback too.
        write_algorithms_schedule_csv(dir.path(), &format!("{ALGORITHMS_SCHEDULE_HEADER}\nOTHER-EN24,pageZ,\n"));
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        write_mapping_csv(
            dir.path(),
            &format!("{MAPPING_HEADER}\nTESTFMT-EN24(investments),cover\nTESTFMT-EN24(renaming),cover\n"),
        );
        let result = get_schedule(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24", "OTHER-EN24"])).unwrap();
        // A single step containing every page type from get_mapping's result (just "cover" here).
        assert_eq!(result, vec![hashset(&["cover"])]);
    }

    #[test]
    fn get_schedule_matches_a_real_multi_step_schedule_from_the_formats_repo() {
        // Real rows from
        // analysis_finance_reports_formats/content/orchestration/algorithms_schedule.csv.
        let dir = tempfile::tempdir().unwrap();
        write_algorithms_schedule_csv(
            dir.path(),
            &format!(
                "{ALGORITHMS_SCHEDULE_HEADER}\nCARNE-EN23,investments,TRUE\nCARNE-EN23,fund_assets,\nCARNE-EN23,manco,\n"
            ),
        );
        let result = get_schedule(dir.path(), "CARNE-EN23", &names(&["CARNE-EN23"])).unwrap();
        assert_eq!(result, vec![hashset(&["investments"]), hashset(&["fund_assets", "manco"])]);
    }

    // ============================================================
    // get_pageclassify_pipelines
    // ============================================================

    #[test]
    fn get_pageclassify_pipelines_errors_when_the_csv_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let expected_path = dir.path().join("content").join("orchestration").join("pageclassify_overwrite.csv");
        let result = get_pageclassify_pipelines(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Err(OrchestrationError::MissingCsv(expected_path)));
    }

    #[test]
    fn get_pageclassify_pipelines_errors_on_a_missing_id_column() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), "NotID\nfoo\n");
        let result = get_pageclassify_pipelines(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert!(matches!(result, Err(OrchestrationError::MalformedRow { .. })));
    }

    #[test]
    fn get_pageclassify_pipelines_falls_back_to_the_empty_string_singleton_when_there_are_no_rows_at_all() {
        // Matches the real formats repo's current (header-only) pageclassify_overwrite.csv shape -
        // every format falls back to this today.
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        let result = get_pageclassify_pipelines(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Ok(hashset(&[""])));
    }

    #[test]
    fn get_pageclassify_pipelines_falls_back_to_the_empty_string_singleton_when_rows_exist_only_for_other_formats() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\nOTHER-EN24(my_pipe)\n"));
        let result =
            get_pageclassify_pipelines(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24", "OTHER-EN24"]));
        assert_eq!(result, Ok(hashset(&[""])));
    }

    #[test]
    fn get_pageclassify_pipelines_aggregates_multiple_pipeline_names_for_the_same_format() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(
            dir.path(),
            &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\nTESTFMT-EN24(pipe_a)\nTESTFMT-EN24(pipe_b)\n"),
        );
        let result = get_pageclassify_pipelines(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Ok(hashset(&["pipe_a", "pipe_b"])));
    }

    #[test]
    fn get_pageclassify_pipelines_keeps_separate_sets_per_format() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(
            dir.path(),
            &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\nTESTFMT-EN24(pipe_a)\nOTHER-EN24(pipe_b)\n"),
        );
        let format_names = names(&["TESTFMT-EN24", "OTHER-EN24"]);
        assert_eq!(get_pageclassify_pipelines(dir.path(), "TESTFMT-EN24", &format_names), Ok(hashset(&["pipe_a"])));
        assert_eq!(get_pageclassify_pipelines(dir.path(), "OTHER-EN24", &format_names), Ok(hashset(&["pipe_b"])));
    }

    #[test]
    fn get_pageclassify_pipelines_errors_when_an_id_has_an_index_suffix() {
        let dir = tempfile::tempdir().unwrap();
        // ONE_TO_ONE relation forbids a trailing "/<digits>" index suffix on this ID.
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\nTESTFMT-EN24(my_pipe)/0\n"));
        let result = get_pageclassify_pipelines(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Err(OrchestrationError::InvalidId("TESTFMT-EN24(my_pipe)/0".to_string())));
    }

    #[test]
    fn get_pageclassify_pipelines_errors_when_an_id_has_no_pipeline_group() {
        let dir = tempfile::tempdir().unwrap();
        // No "(...)" group at all -> derived Pipeline name is null, which this CSV's schema
        // rejects (unlike mapping.csv, which fills it with "" instead).
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\nTESTFMT-EN24\n"));
        let result = get_pageclassify_pipelines(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Err(OrchestrationError::InvalidPipelineName("TESTFMT-EN24".to_string())));
    }

    #[test]
    fn get_pageclassify_pipelines_errors_when_the_format_name_is_not_a_known_format() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\nGHOST-EN24(my_pipe)\n"));
        let result = get_pageclassify_pipelines(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Err(OrchestrationError::UnknownFormatName("GHOST-EN24".to_string())));
    }

    // ============================================================
    // get_mapping
    // ============================================================

    #[test]
    fn get_mapping_errors_when_the_csv_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let expected_path = dir.path().join("content").join("orchestration").join("mapping.csv");
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Err(OrchestrationError::MissingCsv(expected_path)));
    }

    #[test]
    fn get_mapping_errors_on_a_missing_id_column() {
        let dir = tempfile::tempdir().unwrap();
        write_mapping_csv(dir.path(), "NotID,Page type\nfoo,cover\n");
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert!(matches!(result, Err(OrchestrationError::MalformedRow { .. })));
    }

    #[test]
    fn get_mapping_errors_on_a_missing_page_type_column() {
        let dir = tempfile::tempdir().unwrap();
        // "Page type" is a literal required column here (unlike "Format name"/"Pipeline name",
        // which are derived from "ID") - an uncaught KeyError from
        // `df.set_index(["Format name", "Page type"])` in the Python original.
        write_mapping_csv(dir.path(), "ID\nTESTFMT-EN24(investments)\n");
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert!(matches!(result, Err(OrchestrationError::MalformedRow { .. })));
    }

    #[test]
    fn get_mapping_builds_a_page_type_to_pipeline_names_mapping() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        write_mapping_csv(dir.path(), &format!("{MAPPING_HEADER}\nTESTFMT-EN24(investments),cover\n"));
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        let mut expected = HashMap::new();
        expected.insert("cover".to_string(), hashset(&["investments"]));
        assert_eq!(result, Ok(expected));
    }

    #[test]
    fn get_mapping_aggregates_multiple_pipeline_names_for_the_same_page_type() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        write_mapping_csv(
            dir.path(),
            &format!(
                "{MAPPING_HEADER}\nTESTFMT-EN24(investments),cover\nTESTFMT-EN24(renaming),cover\nTESTFMT-EN24(sfdr),classification\n"
            ),
        );
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"])).unwrap();
        let mut expected = HashMap::new();
        expected.insert("cover".to_string(), hashset(&["investments", "renaming"]));
        expected.insert("classification".to_string(), hashset(&["sfdr"]));
        assert_eq!(result, expected);
    }

    #[test]
    fn get_mapping_defaults_to_the_empty_string_pipeline_name_when_an_id_has_no_pipeline_group() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        // Unlike pageclassify_overwrite.csv, a bare ID with no "(pipeline)" group is valid here -
        // fillna("") applies instead of a schema rejection.
        write_mapping_csv(dir.path(), &format!("{MAPPING_HEADER}\nTESTFMT-EN24,cover\n"));
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        let mut expected = HashMap::new();
        expected.insert("cover".to_string(), hashset(&[""]));
        assert_eq!(result, Ok(expected));
    }

    #[test]
    fn get_mapping_errors_when_an_id_has_an_index_suffix() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        write_mapping_csv(dir.path(), &format!("{MAPPING_HEADER}\nTESTFMT-EN24(pipe)/0,cover\n"));
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Err(OrchestrationError::InvalidId("TESTFMT-EN24(pipe)/0".to_string())));
    }

    #[test]
    fn get_mapping_errors_when_the_format_name_is_not_a_known_format() {
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        write_mapping_csv(dir.path(), &format!("{MAPPING_HEADER}\nGHOST-EN24(investments),cover\n"));
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24"]));
        assert_eq!(result, Err(OrchestrationError::UnknownFormatName("GHOST-EN24".to_string())));
    }

    #[test]
    fn get_mapping_real_shaped_mapping_from_the_formats_repo() {
        // Real rows from analysis_finance_reports_formats/content/orchestration/mapping.csv
        // (EURIZON-EN23) - two different pipelines ("renaming" and "merging") both feed the
        // "renaming" page type, and "merging" alone also feeds "subsequent_events".
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        write_mapping_csv(
            dir.path(),
            &format!(
                "{MAPPING_HEADER}\nEURIZON-EN23(renaming),renaming\nEURIZON-EN23(merging),renaming\nEURIZON-EN23(merging),subsequent_events\n"
            ),
        );
        let result = get_mapping(dir.path(), "EURIZON-EN23", &names(&["EURIZON-EN23"])).unwrap();
        let mut expected = HashMap::new();
        expected.insert("renaming".to_string(), hashset(&["renaming", "merging"]));
        expected.insert("subsequent_events".to_string(), hashset(&["merging"]));
        assert_eq!(result, expected);
    }

    // ============================================================
    // get_mapping - Python fallback (pipelines_acquisition.get_pipelines), the one place this
    // module still touches Python. Each test self-attaches to serialize the first `freeports`
    // import, exactly like `input/companies_db.rs`'s Python-touching tests.
    // ============================================================

    #[test]
    fn get_mapping_falls_back_to_an_empty_map_when_pipelines_acquisition_also_has_nothing_for_the_format() {
        pyo3::Python::attach(|py| crate::test_support::ensure_freeports_imported(py));
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        // mapping.csv has rows, but none for the format being queried, and the (baseline, empty)
        // Python-acquisition fixture has no pipelines for it either.
        write_mapping_csv(dir.path(), &format!("{MAPPING_HEADER}\nOTHER-EN24(investments),cover\n"));
        python_acquisition_fixture(dir.path());
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24", "OTHER-EN24"]));
        assert_eq!(result, Ok(HashMap::new()));
    }

    #[test]
    fn get_mapping_falls_back_to_pipelines_acquisitions_pipeline_names_when_the_format_has_real_pipelines() {
        pyo3::Python::attach(|py| crate::test_support::ensure_freeports_imported(py));
        let dir = tempfile::tempdir().unwrap();
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\n"));
        write_mapping_csv(dir.path(), &format!("{MAPPING_HEADER}\nOTHER-EN24(investments),cover\n"));
        python_acquisition_fixture(dir.path());
        // A bare ID (no "(pipeline)" group) defaults to pipeline name "investments".
        add_structured_investments_pipeline_row(dir.path(), "TESTFMT-EN24");
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24", "OTHER-EN24"]));
        let mut expected = HashMap::new();
        expected.insert("investments".to_string(), hashset(&["investments"]));
        assert_eq!(result, Ok(expected));
    }

    #[test]
    fn get_mapping_python_fallback_excludes_pipeline_names_already_used_for_page_classification() {
        pyo3::Python::attach(|py| crate::test_support::ensure_freeports_imported(py));
        let dir = tempfile::tempdir().unwrap();
        // Same "investments" pipeline the structured fixture below defines is also declared as a
        // page-classification override for this format - the fallback must exclude it, leaving an
        // empty map, rather than reporting it as a page-type mapping too.
        write_pageclassify_overwrite_csv(dir.path(), &format!("{PAGECLASSIFY_OVERWRITE_HEADER}\nTESTFMT-EN24(investments)\n"));
        write_mapping_csv(dir.path(), &format!("{MAPPING_HEADER}\nOTHER-EN24(investments),cover\n"));
        python_acquisition_fixture(dir.path());
        add_structured_investments_pipeline_row(dir.path(), "TESTFMT-EN24");
        let result = get_mapping(dir.path(), "TESTFMT-EN24", &names(&["TESTFMT-EN24", "OTHER-EN24"]));
        assert_eq!(result, Ok(HashMap::new()));
    }

    // ============================================================
    // OrchestrationError Display - loose, content-only checks (no pandera-shape fidelity
    // required, same policy as MetadataError's own Display tests).
    // ============================================================

    #[test]
    fn orchestration_error_missing_csv_display_mentions_the_path() {
        let path = PathBuf::from("/some/repo/content/orchestration/mapping.csv");
        let message = OrchestrationError::MissingCsv(path.clone()).to_string();
        assert!(message.contains(&path.display().to_string()));
    }

    #[test]
    fn orchestration_error_unknown_format_name_display_mentions_the_name() {
        let message = OrchestrationError::UnknownFormatName("GHOST-EN24".to_string()).to_string();
        assert!(message.contains("GHOST-EN24"));
    }

    #[test]
    fn orchestration_error_invalid_id_display_mentions_the_id() {
        let message = OrchestrationError::InvalidId("TESTFMT-EN24(pipe)/0".to_string()).to_string();
        assert!(message.contains("TESTFMT-EN24(pipe)/0"));
    }

    #[test]
    fn orchestration_error_invalid_pipeline_name_display_mentions_the_id() {
        let message = OrchestrationError::InvalidPipelineName("TESTFMT-EN24".to_string()).to_string();
        assert!(message.contains("TESTFMT-EN24"));
    }

    #[test]
    fn orchestration_error_malformed_row_display_mentions_the_line_and_reason() {
        let message = (OrchestrationError::MalformedRow { line: 3, reason: "missing column".to_string() }).to_string();
        assert!(message.contains('3'));
        assert!(message.contains("missing column"));
    }

    #[test]
    fn orchestration_error_python_display_mentions_the_underlying_message() {
        let message = OrchestrationError::Python("boom".to_string()).to_string();
        assert!(message.contains("boom"));
    }
}
