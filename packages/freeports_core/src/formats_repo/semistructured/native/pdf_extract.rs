//! Native port of `packages/freeports_core/src/freeports/_internals/formats/repo/algorithms/
//! semistructured/pdf_extract.py`'s `InputStandardCostCurr` + `standard_cost_curr` — the one
//! built-in semistructured `pdf_extract` algorithm that exists today (see
//! `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`, Milestone 2's
//! "standard_cost_curr's own native port" paragraph under Decision 5, and sequencing item 2).
//!
//! # What this builds
//!
//! `standard_cost_curr` builds the same 3 pipe objects the Python original does, from one
//! `InputStandardCostCurr`:
//! 1. [`crate::formats_utils::pdf_extract::standard_funcs::PdfExtractInvestmentsStandard`] — a
//!    native `#[pyclass]` in this same crate already, constructed directly in Rust, no Python
//!    round-trip. Mirrors the Python original's
//!    `PdfExtractInvestmentsStandard(deselection_list=[...], body_set=..., currency_set=arg.currency,
//!    algorithm_flags=arg.algorithm_flags, tolerance=arg.tolerance,
//!    row_algorithm_flags=arg.row_algorithm_flags, row_tolerance=arg.row_tolerance)` exactly —
//!    including passing `currency` through as `currency_set`, a parameter that class's own
//!    constructor accepts but never stores/uses (see that file's own "Known dead fields" doc
//!    comment; this port must not invent a new use for it here either).
//! 2. What `PdfExtractFundStandard.__new__` itself returns:
//!    `ExtractTextPdfBlockOrFailPage(selection=subfund_set, name="fund",
//!    type_block=ResultStandardExtraction.FUND_NAME.name)` (`"FUND_NAME"`) — `PdfExtractFundStandard`
//!    itself is a thin Python `__new__`-factory (per that class's own doc comment in
//!    `standard_funcs.py`; not ported natively, nothing anywhere does
//!    `isinstance(x, PdfExtractFundStandard)`), but `ExtractTextPdfBlockOrFailPage` it returns is
//!    already a native `#[pyclass]` in this crate (`formats_utils::pdf_extract::common`) —
//!    constructed directly here, in Rust, rather than round-tripped through
//!    `py.import(...).getattr("PdfExtractFundStandard").call1(...)`. That generic-import route was
//!    tried first and rejected: `cargo test --lib` embeds its own interpreter and the installed
//!    `freeports._native` extension module it imports is a *separately compiled* shared object —
//!    same source, but PyO3 registers `#[pyclass]`/`create_exception!` types per compiled binary,
//!    so the `ExtractTextPdfBlockOrFailPage`/`PageParseFail` that factory-via-import would produce
//!    are never `is_instance_of`/`extract::<Py<PdfBlock>>()`-compatible with this crate's own
//!    (verified empirically — `PyObject_IsInstance` compares actual type-object identity, not
//!    qualified name). Constructing natively side-steps the whole issue and is behaviorally
//!    identical to the Python original either way.
//! 3. [`crate::formats_utils::pdf_extract::standard_funcs::PdfExtractCurrencyConstant`] — also a
//!    native `#[pyclass]` already, constructed directly from `arg.currency`.
//!
//! `body_set`/`subfund_set`/each `deselection_list` entry are opaque `Py<PyAny>` Python dicts,
//! shaped like `InputPdfLineSet` (keys: `text`, `font`, `font_size`, `area`) — each one is handed,
//! unmodified, to `pdfline_selection_from_dict` (still Python, permanently — see
//! `pdf_blks_acquire.py`), never re-implemented as a strongly-typed Rust struct, since that
//! function does its own pydantic validation internally on whatever dict it's given.
//! `algorithm_flags`/`row_algorithm_flags` are opaque `Option<Py<PyAny>>` passthroughs (still-Python
//! `TablePosAlgorithm`) — `None` is passed straight through unchanged, relying on
//! `PdfExtractInvestmentsStandard::new`'s own existing `None` default (a `TablePosAlgorithm(0)`),
//! not a second default invented here.
//!
//! `currency` is native: `crate::commons::consts::Currency`, parsed nowhere in this module (it
//! arrives already as a `Currency` value — this module isn't responsible for coercing a raw
//! string/YAML value into one; that's the dispatch layer's job, not yet written, see the plan's
//! sequencing item 3).
use pyo3::prelude::*;

use crate::commons::consts::Currency;
use crate::formats_utils::pdf_extract::common::ExtractTextPdfBlockOrFailPage;
use crate::formats_utils::pdf_extract::standard_funcs::{PdfExtractCurrencyConstant, PdfExtractInvestmentsStandard};

/// Mirrors the Python original's `InputStandardCostCurr` pydantic model. See this module's own
/// doc comment for why `deselection_list`/`body_set`/`subfund_set` stay opaque `Py<PyAny>` dicts
/// (`InputPdfLineSet`-shaped) rather than a strongly-typed Rust re-implementation, and why
/// `algorithm_flags`/`row_algorithm_flags` stay opaque `Option<Py<PyAny>>` (`TablePosAlgorithm`,
/// still Python).
pub struct InputStandardCostCurr {
    /// Line sets to exclude from `body_set`'s matches. Defaults to empty (mirrors the Python
    /// original's `= []` default) — the real `FIDEURAM-IT24(investments)` args entry omits this
    /// key entirely.
    pub deselection_list: Vec<Py<PyAny>>,
    /// Line set representing the main body content (the investments table rows).
    pub body_set: Py<PyAny>,
    /// Line set representing subfund information.
    pub subfund_set: Py<PyAny>,
    /// Currency type to filter for. Native (see this module's own doc comment) — also fed,
    /// unused, into `PdfExtractInvestmentsStandard`'s own dead `currency_set` parameter, exactly
    /// mirroring the Python original's call shape.
    pub currency: Currency,
    /// Algorithm flags for table position detection. `None` mirrors the Python original's
    /// `TablePosAlgorithm(0)` default — but that default is applied by
    /// `PdfExtractInvestmentsStandard::new` itself, not re-implemented here; `None` is passed
    /// straight through unchanged.
    pub algorithm_flags: Option<Py<PyAny>>,
    /// Tolerance value for position matching. Defaults to `0.0` (mirrors the Python original's
    /// `Optional[float] = 0.0`).
    pub tolerance: f64,
    /// Algorithm flags for row position detection. Same `None`-passthrough contract as
    /// `algorithm_flags`.
    pub row_algorithm_flags: Option<Py<PyAny>>,
    /// Tolerance value for row position matching. Defaults to `0.0`.
    pub row_tolerance: f64,
}

/// Converts one `InputPdfLineSet`-shaped dict into a `PdfLineSelection` via the still-Python
/// `pdfline_selection_from_dict` (see this module's own doc comment for why that function stays
/// Python and never gets a strongly-typed Rust counterpart).
fn pdfline_selection_from_dict(py: Python<'_>, dict: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    py.import("freeports._internals.formats.utils.pdf_extract.pdf_blks_acquire")?
        .call_method1("pdfline_selection_from_dict", (dict.bind(py),))
        .map(Bound::unbind)
}

/// Rust port of `pdf_extract.py`'s `standard_cost_curr`. See this module's own doc comment for
/// the full mapping of the 3 returned pipes. Returns a plain `PyResult` — this function is pure
/// glue over already-fallible PyO3 calls (constructing all 3 pipes natively, plus one generic
/// `py.import(...).call1(...)` per `deselection_list`/`body_set`/`subfund_set` entry for the
/// still-Python `pdfline_selection_from_dict`), unlike `formats_mapping.rs`/`metadata.rs`/
/// `orchestration.rs`, which each have their own dedicated CSV-parsing error enum — there is no
/// CSV parsing here, so no dedicated error type is warranted.
pub fn standard_cost_curr(
    py: Python<'_>,
    arg: InputStandardCostCurr,
) -> PyResult<(PdfExtractInvestmentsStandard, Py<PyAny>, PdfExtractCurrencyConstant)> {
    let deselection_list = arg
        .deselection_list
        .iter()
        .map(|dl| pdfline_selection_from_dict(py, dl))
        .collect::<PyResult<Vec<_>>>()?;
    let body_set = pdfline_selection_from_dict(py, &arg.body_set)?;
    let subfund_set = pdfline_selection_from_dict(py, &arg.subfund_set)?;
    let currency_set = arg.currency.into_pyobject(py)?.into_any().unbind();

    let investments = PdfExtractInvestmentsStandard::new(
        py,
        body_set,
        None,
        Some(currency_set),
        deselection_list,
        arg.algorithm_flags,
        arg.tolerance,
        arg.row_algorithm_flags,
        arg.row_tolerance,
        None,
    )?;

    let fund =
        Py::new(py, ExtractTextPdfBlockOrFailPage::new(py, subfund_set, "fund".to_string(), "FUND_NAME".to_string())?)?
            .into_any();

    let currency_pipe = PdfExtractCurrencyConstant::new(py, arg.currency)?;

    Ok((investments, fund, currency_pipe))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    use crate::core::classes::{PageParseFail, PdfBlock};

    // ============================================================
    // Fixture helpers
    // ============================================================

    /// Evaluates `src` as a Python literal (dict, in every use below) and returns it unbound.
    /// Same `py.eval` idiom `standard_funcs.rs`'s own tests already use for building sample
    /// Python values inline.
    fn py_value(py: Python<'_>, src: &str) -> Py<PyAny> {
        let code = CString::new(src).unwrap();
        py.eval(&code, None, None).unwrap().unbind()
    }

    /// One PDF line's raw dict shape, as consumed by `pdflines_from_pagedict` (see
    /// `pdf_blks_acquire.py`/`standard_funcs.rs`'s own test fixtures): a single span, `bbox` used
    /// for both the line and its one span (collapses to one span either way).
    fn line_src(font: &str, size: f64, text: &str, bbox: (f64, f64, f64, f64)) -> String {
        format!(
            "{{'dir': (1.0, 0.0), 'bbox': {bbox:?}, 'spans': [\
             {{'font': '{font}', 'size': {size:?}, 'text': '{text}', 'bbox': {bbox:?}}}]}}"
        )
    }

    /// Wraps comma-joined `line_src(...)` outputs (may be empty) into a full page dict.
    fn page_with_lines<'py>(py: Python<'py>, lines_src: &str) -> Bound<'py, PyAny> {
        let src = format!("{{'width': 300.0, 'height': 300.0, 'blocks': [{{'type': 0, 'lines': [{lines_src}]}}]}}");
        py.eval(&CString::new(src).unwrap(), None, None).unwrap()
    }

    /// A classic 2-row/2-col table, same shape as `standard_funcs.rs`'s own
    /// `two_row_table_page`, parameterized by font so it can be reused against differently-fonted
    /// `body_set`s.
    fn two_row_table_page<'py>(py: Python<'py>, font: &str) -> Bound<'py, PyAny> {
        let lines = [
            line_src(font, 10.0, "Row1Col1", (0.0, 0.0, 20.0, 10.0)),
            line_src(font, 10.0, "Row1Col2", (30.0, 0.0, 50.0, 10.0)),
            line_src(font, 10.0, "Row2Col1", (0.0, 20.0, 20.0, 30.0)),
            line_src(font, 10.0, "Row2Col2", (30.0, 20.0, 50.0, 30.0)),
        ]
        .join(",");
        page_with_lines(py, &lines)
    }

    /// Verbatim shape (as Python dicts) of
    /// `analysis_finance_reports_formats/content/algorithms/semistructured/args/pdf_extract.yaml`'s
    /// `AMUNDI-IT24(investments)` entry.
    fn amundi_investments_input(py: Python<'_>) -> InputStandardCostCurr {
        InputStandardCostCurr {
            deselection_list: vec![py_value(py, "{'font': 'TrebuchetMS', 'text': '^ '}")],
            body_set: py_value(py, "{'font': 'TrebuchetMS'}"),
            subfund_set: py_value(py, "{'font': 'Arial-BoldItalicMT', 'area': {'y_max': 60}}"),
            currency: Currency::EUR,
            algorithm_flags: None,
            tolerance: 1.0,
            row_algorithm_flags: None,
            row_tolerance: 0.5,
        }
    }

    /// Verbatim shape of the same YAML file's `FIDEURAM-IT24(investments)` entry — no
    /// `deselection_list` key at all, no explicit `tolerance`/`row_tolerance`.
    fn fideuram_investments_input(py: Python<'_>) -> InputStandardCostCurr {
        InputStandardCostCurr {
            deselection_list: Vec::new(),
            body_set: py_value(py, "{'font': 'Tahoma', 'font_size': 6.96}"),
            subfund_set: py_value(py, "{'font': 'Arial,Bold', 'area': {'y_max': 73}}"),
            currency: Currency::EUR,
            algorithm_flags: None,
            tolerance: 0.0,
            row_algorithm_flags: None,
            row_tolerance: 0.0,
        }
    }

    fn f64_attr(obj: &Bound<'_, PyAny>, name: &str) -> f64 {
        obj.getattr(name).unwrap().extract().unwrap()
    }

    fn call_pipe<'py>(pipe: &Bound<'py, PyAny>, page: &Bound<'py, PyAny>) -> PyResult<Vec<Py<PdfBlock>>> {
        pipe.call_method1("__call__", (page,))?.extract()
    }

    // ============================================================
    // Case 1 (AMUNDI-IT24-shaped): deselection_list present, no font_size, currency EUR,
    // tolerance=1.0, row_tolerance=0.5.
    // ============================================================

    #[test]
    fn amundi_shaped_input_produces_an_investments_pipe_with_the_right_tolerances() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (investments, _fund, _currency) = standard_cost_curr(py, amundi_investments_input(py)).unwrap();
            let investments = Py::new(py, investments).unwrap();
            let bound = investments.bind(py);
            assert_eq!(f64_attr(bound, "tolerance"), 1.0);
            assert_eq!(f64_attr(bound, "row_tolerance"), 0.5);
        });
    }

    #[test]
    fn amundi_shaped_input_investments_pipe_matches_the_trebuchetms_body_set() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (investments, _fund, _currency) = standard_cost_curr(py, amundi_investments_input(py)).unwrap();
            let investments = Py::new(py, investments).unwrap();
            let bound = investments.bind(py);
            let page = two_row_table_page(py, "TrebuchetMS");
            let blocks = call_pipe(bound, &page).unwrap();
            assert_eq!(blocks.len(), 4);
            let type_block: String = blocks[0].bind(py).getattr("type_block").unwrap().extract().unwrap();
            assert_eq!(type_block, "TABLE_BODY");
        });
    }

    #[test]
    fn amundi_shaped_input_produces_a_fund_pipe_with_the_fund_name_type_block() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (_investments, fund, _currency) = standard_cost_curr(py, amundi_investments_input(py)).unwrap();
            let fund = fund.bind(py);
            let type_block: String = fund.getattr("type_block").unwrap().extract().unwrap();
            assert_eq!(type_block, "FUND_NAME");
        });
    }

    #[test]
    fn amundi_shaped_input_produces_a_currency_constant_pipe_that_always_returns_eur() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (_investments, _fund, currency_pipe) = standard_cost_curr(py, amundi_investments_input(py)).unwrap();
            let currency_pipe = Py::new(py, currency_pipe).unwrap();
            let bound = currency_pipe.bind(py);
            let currency: Currency = bound.getattr("currency").unwrap().extract().unwrap();
            assert_eq!(currency, Currency::EUR);

            let empty_page = page_with_lines(py, "");
            let blocks = call_pipe(bound, &empty_page).unwrap();
            assert_eq!(blocks.len(), 1);
            let content: String = blocks[0].bind(py).getattr("content").unwrap().extract().unwrap();
            assert_eq!(content, "EUR");
            let type_block: String = blocks[0].bind(py).getattr("type_block").unwrap().extract().unwrap();
            assert_eq!(type_block, "CURRENCY_STATEMENT");
        });
    }

    // ============================================================
    // Case 2 (FIDEURAM-IT24-shaped): no deselection_list key at all (defaults to empty), no
    // explicit tolerance/row_tolerance (defaults to 0.0), body_set has a font_size constraint.
    // ============================================================

    #[test]
    fn fideuram_shaped_input_defaults_tolerance_and_row_tolerance_to_zero() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (investments, _fund, _currency) = standard_cost_curr(py, fideuram_investments_input(py)).unwrap();
            let investments = Py::new(py, investments).unwrap();
            let bound = investments.bind(py);
            assert_eq!(f64_attr(bound, "tolerance"), 0.0);
            assert_eq!(f64_attr(bound, "row_tolerance"), 0.0);
        });
    }

    #[test]
    fn fideuram_shaped_input_investments_pipe_honors_the_font_size_constraint() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (investments, _fund, _currency) = standard_cost_curr(py, fideuram_investments_input(py)).unwrap();
            let investments = Py::new(py, investments).unwrap();
            let bound = investments.bind(py);

            // Matching font AND matching font_size (6.96, within the from_dict tolerance band):
            // selected.
            let matching_page = page_with_lines(py, &line_src("Tahoma", 6.96, "Cost row", (0.0, 0.0, 20.0, 10.0)));
            let matching_blocks = call_pipe(bound, &matching_page).unwrap();
            assert_eq!(matching_blocks.len(), 1);

            // Matching font but a font_size well outside the constraint: not selected.
            let mismatched_page = page_with_lines(py, &line_src("Tahoma", 20.0, "Cost row", (0.0, 0.0, 20.0, 10.0)));
            let mismatched_blocks = call_pipe(bound, &mismatched_page).unwrap();
            assert!(mismatched_blocks.is_empty());
        });
    }

    #[test]
    fn fideuram_shaped_input_produces_a_currency_constant_pipe_for_eur() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (_investments, _fund, currency_pipe) = standard_cost_curr(py, fideuram_investments_input(py)).unwrap();
            let currency_pipe = Py::new(py, currency_pipe).unwrap();
            let currency: Currency = currency_pipe.bind(py).getattr("currency").unwrap().extract().unwrap();
            assert_eq!(currency, Currency::EUR);
        });
    }

    // ============================================================
    // Case 3: algorithm_flags/row_algorithm_flags both None flow through unchanged, producing
    // the same __call__ behavior PdfExtractInvestmentsStandard already has when constructed
    // directly with None (mirrors standard_funcs.rs's own
    // investments_builds_table_body_blocks_with_row_col_positions).
    // ============================================================

    #[test]
    fn none_algorithm_flags_flow_through_and_behave_like_direct_construction() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let arg = InputStandardCostCurr {
                deselection_list: Vec::new(),
                body_set: py_value(py, "{'font': 'Arial'}"),
                subfund_set: py_value(py, "{'font': 'Arial-BoldItalicMT'}"),
                currency: Currency::EUR,
                algorithm_flags: None,
                tolerance: 0.0,
                row_algorithm_flags: None,
                row_tolerance: 0.0,
            };
            let (investments, _fund, _currency) = standard_cost_curr(py, arg).unwrap();
            let investments = Py::new(py, investments).unwrap();
            let bound = investments.bind(py);
            let page = two_row_table_page(py, "Arial");
            let blocks = call_pipe(bound, &page).unwrap();
            assert_eq!(blocks.len(), 4);
            let first = blocks[0].bind(py);
            let type_block: String = first.getattr("type_block").unwrap().extract().unwrap();
            assert_eq!(type_block, "TABLE_BODY");
            let metadata = first.getattr("metadata").unwrap();
            let row: i64 = metadata.get_item("table-row").unwrap().extract().unwrap();
            let col: i64 = metadata.get_item("table-col").unwrap().extract().unwrap();
            assert_eq!((row, col), (0, 0));
        });
    }

    // ============================================================
    // Case 4: deselection_list entries actually reach the investments pipe's body_set and filter
    // matched rows (mirrors standard_funcs.rs's own
    // investments_deselection_list_removes_matched_rows, but through standard_cost_curr).
    // ============================================================

    #[test]
    fn deselection_list_entries_reach_the_investments_pipe_and_filter_matched_rows() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let arg = InputStandardCostCurr {
                deselection_list: vec![py_value(py, "{'text': 'Row1Col1'}")],
                body_set: py_value(py, "{'font': 'Arial'}"),
                subfund_set: py_value(py, "{'font': 'Arial-BoldItalicMT'}"),
                currency: Currency::EUR,
                algorithm_flags: None,
                tolerance: 0.0,
                row_algorithm_flags: None,
                row_tolerance: 0.0,
            };
            let (investments, _fund, _currency) = standard_cost_curr(py, arg).unwrap();
            let investments = Py::new(py, investments).unwrap();
            let bound = investments.bind(py);
            let page = two_row_table_page(py, "Arial");
            let blocks = call_pipe(bound, &page).unwrap();
            assert_eq!(blocks.len(), 3);
            for b in &blocks {
                let content: String = b.bind(py).getattr("content").unwrap().extract().unwrap();
                assert_ne!(content, "Row1Col1");
            }
        });
    }

    #[test]
    fn deselection_list_defaults_to_empty_when_not_provided() {
        // FIDEURAM-IT24(investments) has no deselection_list key at all - a row that would have
        // been excluded by AMUNDI's own deselection rule ("^ " text) must survive untouched here.
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (investments, _fund, _currency) = standard_cost_curr(py, fideuram_investments_input(py)).unwrap();
            let investments = Py::new(py, investments).unwrap();
            let bound = investments.bind(py);
            let page = page_with_lines(py, &line_src("Tahoma", 6.96, " leading space text", (0.0, 0.0, 20.0, 10.0)));
            let blocks = call_pipe(bound, &page).unwrap();
            assert_eq!(blocks.len(), 1);
        });
    }

    // ============================================================
    // Case 5: subfund_set's dict correctly reaches pdfline_selection_from_dict - a behavioral
    // check (area-constrained matching), not just "no error was raised".
    // ============================================================

    #[test]
    fn subfund_set_area_constraint_selects_only_the_line_inside_the_area() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            // AMUNDI-IT24's subfund_set: font Arial-BoldItalicMT, area y_max=60.
            let (_investments, fund, _currency) = standard_cost_curr(py, amundi_investments_input(py)).unwrap();
            let fund = fund.bind(py);
            let lines = [
                line_src("Arial-BoldItalicMT", 10.0, "Inside Subfund", (0.0, 10.0, 40.0, 20.0)),
                line_src("Arial-BoldItalicMT", 10.0, "Outside Subfund", (0.0, 200.0, 40.0, 210.0)),
            ]
            .join(",");
            let page = page_with_lines(py, &lines);
            let blocks = call_pipe(fund, &page).unwrap();
            assert_eq!(blocks.len(), 1);
            let content: String = blocks[0].bind(py).getattr("content").unwrap().extract().unwrap();
            assert_eq!(content, "Inside Subfund");
        });
    }

    #[test]
    fn subfund_set_area_constraint_excludes_a_line_outside_the_area_entirely() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (_investments, fund, _currency) = standard_cost_curr(py, amundi_investments_input(py)).unwrap();
            let fund = fund.bind(py);
            let page = page_with_lines(
                py,
                &line_src("Arial-BoldItalicMT", 10.0, "Outside Subfund", (0.0, 200.0, 40.0, 210.0)),
            );
            // No line is left after the area filter - ExtractTextPdfBlockOrFailPage wraps that as
            // PageParseFail, proving the area constraint from `subfund_set`'s dict actually
            // reached pdfline_selection_from_dict rather than being dropped/ignored.
            let err = call_pipe(fund, &page).unwrap_err();
            assert!(err.is_instance_of::<PageParseFail>(py));
        });
    }

    #[test]
    fn subfund_set_font_constraint_excludes_lines_with_a_different_font() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let (_investments, fund, _currency) = standard_cost_curr(py, fideuram_investments_input(py)).unwrap();
            let fund = fund.bind(py);
            // FIDEURAM-IT24's subfund_set font is "Arial,Bold" - a differently-fonted line inside
            // the area must not match.
            let page = page_with_lines(py, &line_src("Tahoma", 10.0, "Not a subfund", (0.0, 0.0, 40.0, 10.0)));
            let err = call_pipe(fund, &page).unwrap_err();
            assert!(err.is_instance_of::<PageParseFail>(py));
        });
    }
}
