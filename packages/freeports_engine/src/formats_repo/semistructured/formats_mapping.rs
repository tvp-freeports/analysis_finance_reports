//! Rust port of the CSV-reading/ID-derivation layer inside `packages/freeports_core/src/freeports/
//! _internals/formats/repo/algorithms/semistructured/acquisition.py`'s `get_formats_mapping` (plus
//! the `pipes_mapping` construction at the top of that same file's `get_pipelines`) —
//! `content/algorithms/semistructured/formats_mapping.csv`'s row-by-row parsing and `Format name`/
//! `Pipeline name`/`Pipe index` derivation.
//!
//! See `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`, Milestone 2's
//! "Sequencing" step 1, for the full design context. This module is deliberately narrow, per the
//! plan's own module boundary: it reads `formats_mapping.csv` and derives IDs, nothing more. It
//! does **not** resolve `pdf_extract`/`text_filter`/`deserialize` cell values to native or
//! author-provided algorithms (that is `formats_repo::semistructured::{resolve, get_pipelines}`'s
//! job, a later, not-yet-written file in this same sequencing — see Decision 5 of the plan) — a
//! [`MappingRow`] simply carries each segment cell's *requested name* (or `None`, if the cell was
//! blank) forward, unresolved.
//!
//! # Ground truth read directly off `acquisition.py`/`pipelines_definition.py` (not guessed)
//!
//! - **The CSV's real, literal columns are exactly 4**: `ID`, `pdf_extract`, `text_filter`,
//!   `deserialize` (confirmed against the real, checked-in
//!   `analysis_finance_reports_formats/content/algorithms/semistructured/formats_mapping.csv`).
//!   `acquisition.py`'s `formats_mapping_schema` also declares 3 more columns —
//!   `InputPdfExtract`/`InputTextFilter`/`InputDeserialize` — but those are computed *after*
//!   loading, via `df.assign(...)` from the corresponding segment column
//!   (`"Input" + x.str.title().str.replace("_", "")`), never read from the CSV itself. Per Decision
//!   5 of the plan, resolving an algorithm name to its matching `Input*` class name is `resolve()`'s
//!   job, not this module's — so a [`MappingRow`] does not carry `Input*` fields at all.
//! - **The pipe index is *always* inferred via a whole-file, per-`(Format name, Pipeline name)`
//!   group row-count (pandas' `groupby(...).cumcount()`) — never read from a literal digit in the
//!   `ID` string, and a literal trailing `/<digits>` suffix on this CSV's `ID` column is not merely
//!   "not used", it is **invalid input** (rejected as [`FormatsMappingError::InvalidId`]).** This
//!   refines (and narrows) the framing task-writer inherited from Step 1.1's own doc comment
//!   (`id_format.rs`'s `derive_pipe_index`, which extracts an *explicit* trailing index and leaves
//!   any group-level inference fallback "to formats_mapping.rs"): reading `acquisition.py`'s
//!   `get_formats_mapping` directly shows it calls
//!   `create_index_format_name_pipe(df, "", FKRelation.ONE_TO_ONE)`, and `add_pipe_index`
//!   (`pipelines_definition.py`) computes `mode = PipeIndexMode.INFER if relation_to_principal ==
//!   FKRelation.ONE_TO_ONE else PipeIndexMode.EXPLICIT` — i.e. `ONE_TO_ONE` **unconditionally**
//!   selects the `INFER` branch (`df.groupby(["Format name", "Pipeline name"]).cumcount()`), which
//!   never even looks at `index_regexp`/a trailing digit at all. Separately,
//!   `column_id_format_pipe(FKRelation.ONE_TO_ONE)` validates this CSV's `ID` column against
//!   `IDFormat.EXPANDIBLE_NO_INDEX` (`id_format.rs`'s [`crate::formats_repo::id_format::
//!   id_matches_expandable_no_index`]), whose pattern has **no** trailing `index_regexp` group at
//!   all — so an `ID` like `"AMUNDI-IT24(investments)/0"` fails schema validation outright before
//!   pipe-index derivation would even matter. Net effect: [`crate::formats_repo::id_format::
//!   derive_pipe_index`] (the Step 1.1 primitive) is **not used anywhere in this file** — there is
//!   no "explicit index, with a group-level fallback for missing ones" case for this particular CSV,
//!   only "always group-inferred, explicit indices are a validation error".
//! - **No `format_names` cross-check.** Unlike every one of `orchestration.rs`'s 3 functions
//!   (Milestone 1 Step 1.5), `formats_mapping_schema`'s index is built via a bare
//!   `index_format_pipe()` call — **no `id_principal_table` argument** — so there is no
//!   `pa.Check.isin(format_names)` anywhere in this schema. A row naming a `Format name` that isn't
//!   a real, known format is not an error here; nothing downstream in `acquisition.py` cross-checks
//!   it either. This module's functions therefore take no `format_names` parameter at all,
//!   deliberately, unlike `orchestration.rs`'s equivalents.
//! - **A format with zero matching rows is not an error.** `get_pipelines`'s own construction
//!   (`try: selected_row = get_formats_mapping(formats_repo_dir).loc[format_name] ... except
//!   KeyError: pipes_mapping = []`) only ever catches "this format has no rows" and turns it into an
//!   empty list — silently, with no error — matching `get_pipelines`'s own docstring: "Returns empty
//!   dictionaries if the format name is not found in the mapping." [`rows_for_format`] mirrors this
//!   exactly: `Ok(vec![])`, not an error, for a `format_name` absent from the file (as long as the
//!   file itself exists and parses). Only a missing/malformed *file* is an error (`pd.read_csv`'s
//!   `FileNotFoundError`, and any parse/schema failure, both propagate uncaught out of
//!   `get_pipelines` — only the later `.loc[format_name]` `KeyError` is caught).
//!
//! # Pre-implementation scaffolding note (test-writer phase)
//!
//! Every function body below is a `todo!()` stub — this file's job at this stage is only to give
//! the test suite below a real type/signature surface to compile against (`cargo test --lib` must
//! compile cleanly even though every test currently panics/fails). `implementer` fills these in;
//! per this workspace's TDD discipline, tests are the contract and must not be edited to make them
//! pass.

use std::path::{Path, PathBuf};

use crate::formats_repo::id_format::{derive_format_name, derive_pipeline_name, id_matches_expandable_no_index};

const CONTENT_DIR: &str = "content";
const ALGORITHMS_DIR: &str = "algorithms";
const SEMISTRUCTURED_DIR: &str = "semistructured";
const FORMATS_MAPPING_FILE: &str = "formats_mapping.csv";

/// One row of `content/algorithms/semistructured/formats_mapping.csv`, after `ID` derivation.
///
/// Deliberately narrow — carries only what a downstream dispatch layer
/// (`formats_repo::semistructured::{resolve, get_pipelines}`, not yet written, see this module's
/// own doc comment) needs to iterate over: which pipeline/pipe-index this row belongs to, and which
/// (unresolved) algorithm name, if any, is requested for each of the 3 segment types. `None` means
/// the CSV cell was blank for that segment (mirrors a `NaN` cell in the Python original) — not
/// every row defines all 3 segments (the real fixture's `AMUNDI-IT24`/`FIDEURAM-IT24` rows only
/// ever fill in `pdf_extract`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingRow {
    pub format_name: String,
    pub pipeline_name: String,
    /// Position of this row within its `(format_name, pipeline_name)` group, in CSV row order,
    /// starting at `0` — mirrors pandas' `groupby(["Format name", "Pipeline name"]).cumcount()`.
    /// See this module's own doc comment for why this is *always* how the index is derived for
    /// this particular CSV (never a literal digit read from `ID`).
    pub pipe_index: u32,
    pub pdf_extract: Option<String>,
    pub text_filter: Option<String>,
    pub deserialize: Option<String>,
}

/// Mirrors the union of failure modes `formats_mapping_schema.validate` (plus the uncaught
/// `FileNotFoundError`/`KeyError`s a missing file or missing required column raises) can produce in
/// the Python original, as plain Rust variants rather than an attempt at pandera-shape fidelity
/// (same confirmed policy as [`crate::formats_repo::metadata::MetadataError`]/
/// [`crate::formats_repo::orchestration::OrchestrationError`]).
#[derive(Debug, Clone, PartialEq)]
pub enum FormatsMappingError {
    /// `content/algorithms/semistructured/formats_mapping.csv` doesn't exist under the given
    /// `formats_repo_dir`. Carries the full path that was missing.
    MissingCsv(PathBuf),
    /// A CSV row (or the header itself) couldn't be read/interpreted at all — a required column
    /// missing entirely, or a row with the wrong number of fields. `line` is the 1-based position
    /// of the offending row within the CSV (header line excluded, i.e. the first data row is line
    /// 1; `0` for a whole-file/header-level problem), `reason` is a short, human-readable
    /// explanation.
    MalformedRow { line: usize, reason: String },
    /// A row's `ID` doesn't match the expected one-to-one, no-index `ID` shape (mirrors
    /// `column_id_format_pipe(FKRelation.ONE_TO_ONE)`'s check failing — e.g. a trailing
    /// `/<digits>` index suffix, which this relation forbids outright for this CSV, or a string
    /// that isn't a valid format name shape at all).
    InvalidId(String),
}

impl std::fmt::Display for FormatsMappingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatsMappingError::MissingCsv(path) => {
                write!(f, "missing formats-repository CSV file: {}", path.display())
            }
            FormatsMappingError::MalformedRow { line, reason } => {
                write!(f, "malformed row at line {line}: {reason}")
            }
            FormatsMappingError::InvalidId(id) => {
                write!(f, "ID '{id}' does not match the expected ID pattern")
            }
        }
    }
}

impl std::error::Error for FormatsMappingError {}

/// Opens `<formats_repo_dir>/content/algorithms/semistructured/formats_mapping.csv` as a
/// `csv::Reader`, or `MissingCsv` if it doesn't exist on disk. Mirrors `metadata.rs`'s/
/// `orchestration.rs`'s own `open_csv` helpers (strict mode — every column here matters
/// individually, no genuinely-optional trailing column like `orchestration.rs`'s
/// `algorithms_schedule.csv` has).
fn open_csv(formats_repo_dir: &Path) -> Result<csv::Reader<std::fs::File>, FormatsMappingError> {
    let path = formats_repo_dir.join(CONTENT_DIR).join(ALGORITHMS_DIR).join(SEMISTRUCTURED_DIR).join(FORMATS_MAPPING_FILE);
    if !path.exists() {
        return Err(FormatsMappingError::MissingCsv(path));
    }
    csv::Reader::from_path(&path).map_err(|e| FormatsMappingError::MalformedRow { line: 0, reason: e.to_string() })
}

/// Looks up a required column's index in `headers`, or a `MalformedRow` naming the missing column.
/// Mirrors `metadata.rs`'s/`orchestration.rs`'s own `required_column` helpers.
fn required_column(headers: &csv::StringRecord, name: &str) -> Result<usize, FormatsMappingError> {
    headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| FormatsMappingError::MalformedRow { line: 0, reason: format!("missing required column '{name}'") })
}

/// Rust port of `acquisition.py`'s `get_formats_mapping`: loads and validates every row of
/// `content/algorithms/semistructured/formats_mapping.csv`, deriving `Format name`/`Pipeline name`
/// from each row's `ID` (mirrors `add_format_name`/`add_pipeline_name(df, default="")` — a missing
/// pipeline group defaults to the empty string, same as `orchestration.rs`'s `mapping.csv`, not
/// rejected like `pageclassify_overwrite.csv`), then assigns each row's `Pipe index` as its ordinal
/// position within its own `(Format name, Pipeline name)` group, in CSV row order, starting at `0`
/// (mirrors `add_pipe_index`'s `PipeIndexMode.INFER` branch — see this module's own doc comment for
/// why this CSV always takes that branch, never the explicit-digit one). A blank `pdf_extract`/
/// `text_filter`/`deserialize` cell is `None` (mirrors a `NaN` cell). Rows are returned in CSV row
/// order. See this module's own doc comment for why there is no `format_names` cross-check
/// (unlike every function in `orchestration.rs`).
pub fn get_formats_mapping(formats_repo_dir: &Path) -> Result<Vec<MappingRow>, FormatsMappingError> {
    let mut reader = open_csv(formats_repo_dir)?;
    let headers = reader
        .headers()
        .map_err(|e| FormatsMappingError::MalformedRow { line: 0, reason: e.to_string() })?
        .clone();
    let id_idx = required_column(&headers, "ID")?;
    let pdf_extract_idx = required_column(&headers, "pdf_extract")?;
    let text_filter_idx = required_column(&headers, "text_filter")?;
    let deserialize_idx = required_column(&headers, "deserialize")?;

    let mut rows = Vec::new();
    // (Format name, Pipeline name) -> next pipe index, mirrors pandas'
    // groupby(["Format name", "Pipeline name"]).cumcount().
    let mut next_pipe_index: std::collections::HashMap<(String, String), u32> = std::collections::HashMap::new();
    for (i, record) in reader.records().enumerate() {
        let line = i + 1;
        let record = record.map_err(|e| FormatsMappingError::MalformedRow { line, reason: e.to_string() })?;
        let id = record.get(id_idx).unwrap_or("").to_string();
        if !id_matches_expandable_no_index(&id) {
            return Err(FormatsMappingError::InvalidId(id));
        }
        let format_name = derive_format_name(&id);
        // A missing "(pipeline)" group defaults to the empty string (mirrors
        // add_pipeline_name(df, default="")), unlike pageclassify_overwrite.csv's rejection.
        let pipeline_name = derive_pipeline_name(&id, Some("")).unwrap_or_default();
        let pdf_extract = parse_segment_cell(record.get(pdf_extract_idx).unwrap_or(""));
        let text_filter = parse_segment_cell(record.get(text_filter_idx).unwrap_or(""));
        let deserialize = parse_segment_cell(record.get(deserialize_idx).unwrap_or(""));

        let counter = next_pipe_index.entry((format_name.clone(), pipeline_name.clone())).or_insert(0);
        let pipe_index = *counter;
        *counter += 1;

        rows.push(MappingRow { format_name, pipeline_name, pipe_index, pdf_extract, text_filter, deserialize });
    }
    Ok(rows)
}

/// A blank CSV cell is `None` (mirrors a `NaN` cell); any other content is `Some`, unmodified.
fn parse_segment_cell(raw: &str) -> Option<String> {
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
    }
}

/// The per-format row selection `get_pipelines` performs inline at its own top
/// (`get_formats_mapping(formats_repo_dir).loc[format_name]`, with the `except KeyError:
/// pipes_mapping = []` fallback) — filters [`get_formats_mapping`]'s full result down to just
/// `format_name`'s own rows, preserving their relative CSV row order (and, since `Pipe index` is
/// computed over the *whole* file before this filter is applied, any interleaving of other formats'
/// rows in between does not affect the surviving rows' indices). Returns `Ok(vec![])` — **not** an
/// error — when `format_name` has no rows at all, mirroring `get_pipelines`'s own documented
/// contract ("Returns empty dictionaries if the format name is not found in the mapping."). A
/// missing/malformed file, or a malformed/invalid row anywhere in the file (even for a different
/// format than the one requested here), still propagates as an error — only "this specific format
/// has zero rows" is silently swallowed, mirroring the Python original's own narrow `except
/// KeyError` (it does not catch `FileNotFoundError` or a schema validation failure).
pub fn rows_for_format(formats_repo_dir: &Path, format_name: &str) -> Result<Vec<MappingRow>, FormatsMappingError> {
    let rows = get_formats_mapping(formats_repo_dir)?;
    Ok(rows.into_iter().filter(|r| r.format_name == format_name).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    // ============================================================
    // Fixture helpers
    // ============================================================

    /// Writes `<dir>/content/algorithms/semistructured/formats_mapping.csv` with the given raw CSV
    /// text, creating the `content/algorithms/semistructured/` subfolder as needed. Deliberately
    /// takes raw CSV text (rather than baking in one row shape) since this module's tests need many
    /// different row shapes, including malformed ones a stricter helper couldn't produce — same
    /// discipline as `metadata.rs`'s/`orchestration.rs`'s own fixture-writer helpers.
    fn write_formats_mapping_csv(dir: &Path, csv_text: &str) {
        let semistructured_dir = dir.join("content").join("algorithms").join("semistructured");
        std::fs::create_dir_all(&semistructured_dir).unwrap();
        std::fs::write(semistructured_dir.join("formats_mapping.csv"), csv_text).unwrap();
    }

    const HEADER: &str = "ID,pdf_extract,text_filter,deserialize";

    fn row(format_name: &str, pipeline_name: &str, pipe_index: u32, pdf_extract: Option<&str>, text_filter: Option<&str>, deserialize: Option<&str>) -> MappingRow {
        MappingRow {
            format_name: format_name.to_string(),
            pipeline_name: pipeline_name.to_string(),
            pipe_index,
            pdf_extract: pdf_extract.map(str::to_string),
            text_filter: text_filter.map(str::to_string),
            deserialize: deserialize.map(str::to_string),
        }
    }

    // ============================================================
    // get_formats_mapping - file/column-level errors
    // ============================================================

    #[test]
    fn get_formats_mapping_errors_when_the_csv_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let expected_path =
            dir.path().join("content").join("algorithms").join("semistructured").join("formats_mapping.csv");
        assert_eq!(get_formats_mapping(dir.path()), Err(FormatsMappingError::MissingCsv(expected_path)));
    }

    #[test]
    fn get_formats_mapping_returns_empty_vec_for_a_header_only_csv() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(dir.path(), &format!("{HEADER}\n"));
        assert_eq!(get_formats_mapping(dir.path()), Ok(vec![]));
    }

    #[test_case("pdf_extract,text_filter,deserialize"; "missing ID column")]
    #[test_case("ID,text_filter,deserialize"; "missing pdf_extract column")]
    #[test_case("ID,pdf_extract,deserialize"; "missing text_filter column")]
    #[test_case("ID,pdf_extract,text_filter"; "missing deserialize column")]
    fn get_formats_mapping_errors_on_a_missing_required_column(header: &str) {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(dir.path(), &format!("{header}\n"));
        assert!(matches!(get_formats_mapping(dir.path()), Err(FormatsMappingError::MalformedRow { .. })));
    }

    #[test]
    fn get_formats_mapping_errors_on_a_row_with_the_wrong_number_of_fields() {
        let dir = tempfile::tempdir().unwrap();
        // Header has 4 columns, data row only has 2.
        write_formats_mapping_csv(dir.path(), &format!("{HEADER}\nAMUNDI-IT24(investments),standard_cost_curr\n"));
        assert!(matches!(get_formats_mapping(dir.path()), Err(FormatsMappingError::MalformedRow { .. })));
    }

    // ============================================================
    // get_formats_mapping - ID validation (ONE_TO_ONE / EXPANDIBLE_NO_INDEX: no index suffix
    // allowed at all on this CSV's ID column, unlike e.g. structured additional_args.csv)
    // ============================================================

    #[test]
    fn get_formats_mapping_errors_when_an_id_has_an_index_suffix() {
        let dir = tempfile::tempdir().unwrap();
        // This CSV's ID schema is EXPANDIBLE_NO_INDEX - a trailing "/<digits>" is invalid input
        // here, not merely "not used for pipe-index derivation" (see this module's own doc
        // comment).
        write_formats_mapping_csv(dir.path(), &format!("{HEADER}\nAMUNDI-IT24(investments)/0,standard_cost_curr,,\n"));
        let result = get_formats_mapping(dir.path());
        assert_eq!(result, Err(FormatsMappingError::InvalidId("AMUNDI-IT24(investments)/0".to_string())));
    }

    #[test]
    fn get_formats_mapping_errors_when_an_id_is_not_a_valid_format_name_shape_at_all() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(dir.path(), &format!("{HEADER}\nnot-a-format,standard_cost_curr,,\n"));
        let result = get_formats_mapping(dir.path());
        assert_eq!(result, Err(FormatsMappingError::InvalidId("not-a-format".to_string())));
    }

    #[test]
    fn get_formats_mapping_accepts_an_id_with_no_pipeline_group_at_all() {
        // Unlike pageclassify_overwrite.csv (orchestration.rs), a bare ID with no "(pipeline)"
        // group is valid here - it defaults to the empty-string pipeline name, mirroring
        // create_index_format_name_pipe(df, "", ...)'s pipeline_default="".
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(dir.path(), &format!("{HEADER}\nCARNE-EN23,standard_cost_curr,,\n"));
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(result, vec![row("CARNE-EN23", "", 0, Some("standard_cost_curr"), None, None)]);
    }

    #[test]
    fn get_formats_mapping_treats_explicit_empty_parens_the_same_as_a_bare_id() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(dir.path(), &format!("{HEADER}\nCARNE-EN23(),standard_cost_curr,,\n"));
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(result, vec![row("CARNE-EN23", "", 0, Some("standard_cost_curr"), None, None)]);
    }

    // ============================================================
    // get_formats_mapping - segment-column nullability
    // ============================================================

    #[test]
    fn get_formats_mapping_treats_a_blank_segment_cell_as_none() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(dir.path(), &format!("{HEADER}\nAMUNDI-IT24(investments),standard_cost_curr,,\n"));
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(
            result,
            vec![row("AMUNDI-IT24", "investments", 0, Some("standard_cost_curr"), None, None)]
        );
    }

    #[test]
    fn get_formats_mapping_reads_all_three_segment_columns_when_populated() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            &format!("{HEADER}\nCARNE-EN23(investments),my_pdf_extract,my_text_filter,my_deserialize\n"),
        );
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(
            result,
            vec![row(
                "CARNE-EN23",
                "investments",
                0,
                Some("my_pdf_extract"),
                Some("my_text_filter"),
                Some("my_deserialize")
            )]
        );
    }

    // ============================================================
    // get_formats_mapping - Pipe index: always group-inferred (cumcount), never read from ID
    // ============================================================

    #[test]
    fn get_formats_mapping_assigns_pipe_index_zero_to_a_lone_row_in_its_group() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(dir.path(), &format!("{HEADER}\nAMUNDI-IT24(investments),standard_cost_curr,,\n"));
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(result[0].pipe_index, 0);
    }

    #[test]
    fn get_formats_mapping_increments_pipe_index_within_the_same_format_and_pipeline_group() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            &format!(
                "{HEADER}\nCARNE-EN23(investments),alg_a,,\nCARNE-EN23(investments),alg_b,,\nCARNE-EN23(investments),alg_c,,\n"
            ),
        );
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(
            result,
            vec![
                row("CARNE-EN23", "investments", 0, Some("alg_a"), None, None),
                row("CARNE-EN23", "investments", 1, Some("alg_b"), None, None),
                row("CARNE-EN23", "investments", 2, Some("alg_c"), None, None),
            ]
        );
    }

    #[test]
    fn get_formats_mapping_keeps_a_separate_pipe_index_sequence_per_pipeline_name() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            &format!(
                "{HEADER}\nCARNE-EN23(investments),alg_a,,\nCARNE-EN23(renaming),alg_b,,\nCARNE-EN23(investments),alg_c,,\n"
            ),
        );
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(
            result,
            vec![
                row("CARNE-EN23", "investments", 0, Some("alg_a"), None, None),
                row("CARNE-EN23", "renaming", 0, Some("alg_b"), None, None),
                row("CARNE-EN23", "investments", 1, Some("alg_c"), None, None),
            ]
        );
    }

    #[test]
    fn get_formats_mapping_keeps_a_separate_pipe_index_sequence_per_format_even_with_the_same_pipeline_name() {
        // Two different formats both using pipeline name "investments" must not share one
        // cumcount sequence - the group key is (Format name, Pipeline name), not Pipeline name
        // alone.
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            &format!(
                "{HEADER}\nAMUNDI-IT24(investments),alg_a,,\nFIDEURAM-IT24(investments),alg_b,,\nAMUNDI-IT24(investments),alg_c,,\n"
            ),
        );
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(
            result,
            vec![
                row("AMUNDI-IT24", "investments", 0, Some("alg_a"), None, None),
                row("FIDEURAM-IT24", "investments", 0, Some("alg_b"), None, None),
                row("AMUNDI-IT24", "investments", 1, Some("alg_c"), None, None),
            ]
        );
    }

    #[test]
    fn get_formats_mapping_preserves_csv_row_order_in_its_returned_vec() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            &format!("{HEADER}\nZETA-EN24,alg_z,,\nALPHA-EN24,alg_a,,\n"),
        );
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(result[0].format_name, "ZETA-EN24");
        assert_eq!(result[1].format_name, "ALPHA-EN24");
    }

    // ============================================================
    // get_formats_mapping - real-shaped fixture (AMUNDI-IT24/FIDEURAM-IT24, per the plan's own
    // named acceptance bar)
    // ============================================================

    #[test]
    fn get_formats_mapping_reads_the_real_amundi_and_fideuram_rows() {
        // Verbatim shape from analysis_finance_reports_formats/content/algorithms/semistructured/
        // formats_mapping.csv.
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            "ID,pdf_extract,text_filter,deserialize\nAMUNDI-IT24(investments),standard_cost_curr,,\nFIDEURAM-IT24(investments),standard_cost_curr,,\n",
        );
        let result = get_formats_mapping(dir.path()).unwrap();
        assert_eq!(
            result,
            vec![
                row("AMUNDI-IT24", "investments", 0, Some("standard_cost_curr"), None, None),
                row("FIDEURAM-IT24", "investments", 0, Some("standard_cost_curr"), None, None),
            ]
        );
    }

    // ============================================================
    // rows_for_format
    // ============================================================

    #[test]
    fn rows_for_format_returns_empty_vec_not_an_error_when_the_format_has_no_rows() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(dir.path(), &format!("{HEADER}\nAMUNDI-IT24(investments),standard_cost_curr,,\n"));
        // GHOST-EN24 is a syntactically valid format name shape but simply has no rows in this
        // file - mirrors get_pipelines's own documented "returns empty dictionaries" contract, not
        // an error, unlike orchestration.rs's UnknownFormatName (this CSV has no format_names
        // cross-check at all, see this module's own doc comment).
        let result = rows_for_format(dir.path(), "GHOST-EN24");
        assert_eq!(result, Ok(vec![]));
    }

    #[test]
    fn rows_for_format_still_propagates_a_missing_csv_error() {
        let dir = tempfile::tempdir().unwrap();
        let expected_path =
            dir.path().join("content").join("algorithms").join("semistructured").join("formats_mapping.csv");
        assert_eq!(rows_for_format(dir.path(), "AMUNDI-IT24"), Err(FormatsMappingError::MissingCsv(expected_path)));
    }

    #[test]
    fn rows_for_format_still_propagates_an_invalid_id_error_even_for_a_different_formats_row() {
        // A malformed row for a *different* format than the one queried still fails the whole
        // file's parse - mirrors get_pipelines's own narrow `except KeyError` (it does not catch a
        // schema validation failure, only "this format has zero rows").
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            &format!("{HEADER}\nAMUNDI-IT24(investments),standard_cost_curr,,\nGHOST-EN24(investments)/0,standard_cost_curr,,\n"),
        );
        let result = rows_for_format(dir.path(), "AMUNDI-IT24");
        assert_eq!(result, Err(FormatsMappingError::InvalidId("GHOST-EN24(investments)/0".to_string())));
    }

    #[test]
    fn rows_for_format_filters_to_only_the_requested_formats_rows() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            &format!("{HEADER}\nAMUNDI-IT24(investments),standard_cost_curr,,\nFIDEURAM-IT24(investments),standard_cost_curr,,\n"),
        );
        let result = rows_for_format(dir.path(), "AMUNDI-IT24");
        assert_eq!(
            result,
            Ok(vec![row("AMUNDI-IT24", "investments", 0, Some("standard_cost_curr"), None, None)])
        );
    }

    #[test]
    fn rows_for_format_pipe_index_is_unaffected_by_interleaved_rows_from_other_formats() {
        // Pipe index is computed over the *whole* file's (Format name, Pipeline name) groups
        // before this function's own filter is applied - an interleaved OTHER-FMT row using the
        // same pipeline name must not perturb AMUNDI-IT24's own 0/1 sequence.
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            &format!(
                "{HEADER}\nAMUNDI-IT24(investments),alg_a,,\nOTHER-FMT-EN24(investments),alg_x,,\nAMUNDI-IT24(investments),alg_b,,\n"
            ),
        );
        let result = rows_for_format(dir.path(), "AMUNDI-IT24");
        assert_eq!(
            result,
            Ok(vec![
                row("AMUNDI-IT24", "investments", 0, Some("alg_a"), None, None),
                row("AMUNDI-IT24", "investments", 1, Some("alg_b"), None, None),
            ])
        );
    }

    #[test]
    fn rows_for_format_returns_multiple_pipelines_for_the_same_format() {
        let dir = tempfile::tempdir().unwrap();
        write_formats_mapping_csv(
            dir.path(),
            &format!("{HEADER}\nCARNE-EN23(investments),alg_a,,\nCARNE-EN23(renaming),alg_b,,\n"),
        );
        let result = rows_for_format(dir.path(), "CARNE-EN23");
        assert_eq!(
            result,
            Ok(vec![
                row("CARNE-EN23", "investments", 0, Some("alg_a"), None, None),
                row("CARNE-EN23", "renaming", 0, Some("alg_b"), None, None),
            ])
        );
    }

    // ============================================================
    // FormatsMappingError Display - loose, content-only checks (no pandera-shape fidelity
    // required, same policy as MetadataError's/OrchestrationError's own Display tests).
    // ============================================================

    #[test]
    fn formats_mapping_error_missing_csv_display_mentions_the_path() {
        let path = PathBuf::from("/some/repo/content/algorithms/semistructured/formats_mapping.csv");
        let message = FormatsMappingError::MissingCsv(path.clone()).to_string();
        assert!(message.contains(&path.display().to_string()));
    }

    #[test]
    fn formats_mapping_error_malformed_row_display_mentions_the_line_and_reason() {
        let message = (FormatsMappingError::MalformedRow { line: 2, reason: "missing column".to_string() }).to_string();
        assert!(message.contains('2'));
        assert!(message.contains("missing column"));
    }

    #[test]
    fn formats_mapping_error_invalid_id_display_mentions_the_id() {
        let message = FormatsMappingError::InvalidId("GHOST-EN24(pipe)/0".to_string()).to_string();
        assert!(message.contains("GHOST-EN24(pipe)/0"));
    }
}
