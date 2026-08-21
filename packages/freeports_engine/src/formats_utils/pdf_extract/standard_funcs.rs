//! Rust port of `formats/utils/pdf_extract/standard_funcs.py`.
//!
//! Scope decided explicitly by the user (2026-08-19): port everything, including the two classes
//! (`PdfExtractInvestmentsStandard`, `PdfExtractAssetsStandard`) that are currently mostly glue
//! around the still-Python `TablePosAlgorithm`/`get_table_coordinates`/`CollapseAlgorithm`/
//! `PdfLineSelection` (the latter is Rust — `freeports_engine.core.PdfLineSelection`, merged in
//! from the former `freeports_lib` crate in Fase E — but still reached via `py.import`, generically,
//! same as the still-Python pieces, since nothing else in this file has been ported natively yet)
//! — reasoning: once those pieces are themselves ported to Rust later in this migration, the round-trips this
//! file currently makes back into Python collapse away on their own; porting the *callers* now
//! means that collapse happens for free later instead of requiring another full pass over this
//! file. Every Python object crossing that boundary today is handled generically (`Py<PyAny>` +
//! `call_method1`/`getattr`), matching the duck-typing the Python original itself relied on.
//!
//! **Not ported**: the 3 factory classes (`PdfExtractFundStandard`, `PdfExtractCurrencyStandard`,
//! `PdfExtractManagmentCompanyStandard`) — already plain Python `__new__`-factories over the
//! Rust-backed `ExtractTextPdfBlockOrFailPage` since the `pdf_extract/common.py` port; porting
//! them further would add a second Rust type per factory for zero behavioral change.
//!
//! **Bug found and fixed at the root (user confirmed, 2026-08-19)**: `PdfExtractSfdrArticleStandard`'s
//! 3 constructor parameters defaulted to `= PdfLineSelection` — the class object itself, not an
//! instance. Verified every real caller across the formats repo passes all 3 arguments
//! explicitly, so this was dead/unreachable; fixed by making them required, with identical
//! behavior for every real caller.
//!
//! **Known dead fields, deliberately kept, not wired up (user confirmed, 2026-08-19)**:
//! `PdfExtractInvestmentsStandard.tolerance` and `.row_algorithm_flags` are stored (readable via
//! the public API for future use) but never actually consulted by `__call__` — only
//! `algorithm_flags` and `row_tolerance` feed the real `get_table_coordinates` call, exactly
//! matching the Python original. No format currently sets non-default values for either, so this
//! doesn't change behavior; the *computation* the Python original ran on `row_algorithm_flags`
//! (list-of-bools → `TablePosAlgorithm`, whose result was then discarded) is not replicated here
//! since it has zero observable effect either way — only the *field itself* needs to survive for
//! API parity, not a computation nobody can observe.
//!
//! **`PdfExtractInvestmentsStandard::new`/`PdfExtractCurrencyConstant::new` made `pub` (Milestone
//! 2 Step 2)**: needed by `formats_repo::semistructured::native::pdf_extract::standard_cost_curr`,
//! a sibling top-level module (not a descendant of this one), to construct these two pyclasses
//! directly instead of round-tripping through Python — see that module's own doc comment for why
//! the round-trip route doesn't work at all for the 3rd pipe it builds. Every other field/method
//! here keeps its original visibility; only these two constructors were loosened.

use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::commons::consts::SfdrArticle;
use crate::core::classes::{ExpectedPdfBlockNotFound, PdfBlock};
use crate::formats_utils::pdf_extract::common::SelectExpectedText;

fn pdf_blks_acquire_module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    py.import("freeports._internals.formats.utils.pdf_extract.pdf_blks_acquire")
}

fn position_module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    py.import("freeports._internals.formats.utils.pdf_extract.position")
}

fn pdf_line_selection_class(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    // `PdfLineSelection` used to live in the separate `freeports_lib` crate, reached only via
    // `py.import("freeports_lib")`; merged into this crate in Fase E (see
    // `agent-memory/rust-native-binary-plan.md`) and now exported at `freeports_engine.core`.
    py.import("freeports_engine")?.getattr("core")?.getattr("PdfLineSelection")
}

fn pdflines_from_pagedict<'py>(py: Python<'py>, dict_root: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    pdf_blks_acquire_module(py)?.call_method1("pdflines_from_pagedict", (dict_root,))
}

#[pyclass(module = "freeports_engine")]
pub struct PdfExtractSfdrArticleStandard {
    art9_selection: Py<PyAny>,
    art8_selection: Py<PyAny>,
    fund_selection: Py<PyAny>,
}

#[pymethods]
impl PdfExtractSfdrArticleStandard {
    #[new]
    fn new(art9_selection: Py<PyAny>, art8_selection: Py<PyAny>, fund_selection: Py<PyAny>) -> Self {
        Self { art9_selection, art8_selection, fund_selection }
    }

    #[getter]
    fn art9_selection(&self, py: Python<'_>) -> Py<PyAny> {
        self.art9_selection.clone_ref(py)
    }
    #[setter]
    fn set_art9_selection(&mut self, v: Py<PyAny>) {
        self.art9_selection = v;
    }
    #[getter]
    fn art8_selection(&self, py: Python<'_>) -> Py<PyAny> {
        self.art8_selection.clone_ref(py)
    }
    #[setter]
    fn set_art8_selection(&mut self, v: Py<PyAny>) {
        self.art8_selection = v;
    }
    #[getter(fund_pdflineselection)]
    fn fund_selection(&self, py: Python<'_>) -> Py<PyAny> {
        self.fund_selection.clone_ref(py)
    }
    #[setter(fund_pdflineselection)]
    fn set_fund_selection(&mut self, v: Py<PyAny>) {
        self.fund_selection = v;
    }

    fn __call__(&self, py: Python<'_>, page: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PdfBlock>>> {
        let lines = pdflines_from_pagedict(py, page)?;

        let art8_result = self.art8_selection.bind(py).call_method1("select", (&lines,))?;
        let art = if art8_result.is_truthy()? {
            SfdrArticle::ART_8
        } else {
            let art9_result = self.art9_selection.bind(py).call_method1("select", (&lines,))?;
            if art9_result.is_truthy()? { SfdrArticle::ART_9 } else { SfdrArticle::ART_6 }
        };

        let funds_blks = self.fund_selection.bind(py).call_method1("select", (&lines,))?;
        let count = funds_blks.len()?;
        let txt = if count == 0 {
            return Err(ExpectedPdfBlockNotFound::new_err("Fund name"));
        } else if count == 1 {
            let first = funds_blks.try_iter()?.next().unwrap()?;
            first.getattr("text")?.extract::<String>()?
        } else {
            let items: Vec<Bound<PyAny>> = funds_blks.try_iter()?.collect::<PyResult<_>>()?;
            let mut keyed: Vec<(f64, Bound<PyAny>)> = items
                .into_iter()
                .map(|item| -> PyResult<(f64, Bound<PyAny>)> {
                    let y: f64 = item.getattr("bbox")?.get_item(1)?.extract()?;
                    Ok((y, item))
                })
                .collect::<PyResult<_>>()?;
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut txt = String::new();
            for (_, item) in &keyed {
                txt.push_str(&item.getattr("text")?.extract::<String>()?);
            }
            txt
        };

        let metadata = PyDict::new(py);
        metadata.set_item("article", art)?;
        let content = txt.into_pyobject(py)?.into_any().unbind();
        let blk = Py::new(py, PdfBlock::new("SFDR_ARTICLE".to_string(), metadata.unbind(), content))?;
        Ok(vec![blk])
    }
}

#[pyclass(module = "freeports_engine")]
pub struct PdfExtractCurrencyConstant {
    #[pyo3(get)]
    currency: crate::commons::consts::Currency,
    blk: Py<PdfBlock>,
}

#[pymethods]
impl PdfExtractCurrencyConstant {
    #[new]
    pub fn new(py: Python<'_>, currency: crate::commons::consts::Currency) -> PyResult<Self> {
        let metadata = PyDict::new(py).unbind();
        let content = currency.code().into_pyobject(py)?.into_any().unbind();
        let blk = Py::new(py, PdfBlock::new("CURRENCY_STATEMENT".to_string(), metadata, content))?;
        Ok(Self { currency, blk })
    }

    fn __call__(&self, py: Python<'_>, _dict_root: &Bound<'_, PyAny>) -> Vec<Py<PdfBlock>> {
        vec![self.blk.clone_ref(py)]
    }
}

#[pyclass(module = "freeports_engine")]
pub struct PdfExtractPageClassifyStandard {
    header_sets: Vec<Py<PyAny>>,
    #[pyo3(get, set)]
    page_type: String,
}

#[pymethods]
impl PdfExtractPageClassifyStandard {
    #[new]
    fn new(header_sets: &Bound<'_, PyAny>, page_type: String) -> PyResult<Self> {
        let sets: Vec<Py<PyAny>> = match header_sets.try_iter() {
            Ok(iter) => iter.map(|i| i.map(Bound::unbind)).collect::<PyResult<_>>()?,
            Err(err) if err.is_instance_of::<pyo3::exceptions::PyTypeError>(header_sets.py()) => {
                vec![header_sets.clone().unbind()]
            }
            Err(err) => return Err(err),
        };
        Ok(Self { header_sets: sets, page_type })
    }

    fn __call__(&self, py: Python<'_>, dict_root: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PdfBlock>>> {
        let lines = pdflines_from_pagedict(py, dict_root)?;
        let mut page_type: Option<String> = Some(self.page_type.clone());
        for hs in &self.header_sets {
            let result = hs.bind(py).call_method1("select", (&lines,))?;
            if result.len()? == 0 {
                page_type = None;
                break;
            }
        }
        let metadata = PyDict::new(py);
        metadata.set_item("page_type", page_type)?;
        let content = "".into_pyobject(py)?.into_any().unbind();
        let blk = Py::new(py, PdfBlock::new("PAGE_CLASS".to_string(), metadata.unbind(), content))?;
        Ok(vec![blk])
    }
}

/// Replicates the Python original's `isinstance(_algorithm_flags, list)` branch: a literal list
/// of 4 bools (`[RETURN_ROWS, BIG_CELL_RULE, USE_RULER_AREA, USE_TEST_POS]` enabled/disabled) is
/// OR-ed into a `TablePosAlgorithm`; anything else (already a `TablePosAlgorithm`) passes through
/// unchanged. `TablePosAlgorithm` itself stays Python (see `pdf_extract/position.rs`'s module
/// doc) — resolved here generically via `position_module`.
fn resolve_algorithm_flags(py: Python<'_>, flags: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let bound = flags.bind(py);
    if !bound.is_instance_of::<PyList>() {
        return Ok(flags.clone_ref(py));
    }
    let table_pos_algorithm = position_module(py)?.getattr("TablePosAlgorithm")?;
    let flag_names = ["RETURN_ROWS", "BIG_CELL_RULE", "USE_RULER_AREA", "USE_TEST_POS"];
    let mut algo = table_pos_algorithm.call1((0,))?;
    for (name, enabled) in flag_names.iter().zip(bound.try_iter()?) {
        if enabled?.is_truthy()? {
            let flag = table_pos_algorithm.getattr(*name)?;
            algo = algo.call_method1("__or__", (flag,))?;
        }
    }
    Ok(algo.unbind())
}

fn table_pos_algorithm_zero(py: Python<'_>) -> PyResult<Py<PyAny>> {
    Ok(position_module(py)?.getattr("TablePosAlgorithm")?.call1((0,))?.unbind())
}

#[pyclass(module = "freeports_engine")]
pub struct PdfExtractInvestmentsStandard {
    body_set: Py<PyAny>,
    algorithm_flags: Py<PyAny>,
    #[pyo3(get, set)]
    tolerance: f64,
    row_algorithm_flags: Py<PyAny>,
    #[pyo3(get, set)]
    row_tolerance: f64,
    #[pyo3(get, set)]
    company_index: Option<i64>,
}

#[pymethods]
impl PdfExtractInvestmentsStandard {
    #[new]
    #[pyo3(signature = (
        body_set,
        manco_set=None,
        currency_set=None,
        deselection_list=Vec::new(),
        algorithm_flags=None,
        tolerance=0.0,
        row_algorithm_flags=None,
        row_tolerance=0.0,
        company_index=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        py: Python<'_>,
        body_set: Py<PyAny>,
        manco_set: Option<Py<PyAny>>,
        currency_set: Option<Py<PyAny>>,
        deselection_list: Vec<Py<PyAny>>,
        algorithm_flags: Option<Py<PyAny>>,
        tolerance: f64,
        row_algorithm_flags: Option<Py<PyAny>>,
        row_tolerance: f64,
        company_index: Option<i64>,
    ) -> PyResult<Self> {
        // Accepted for constructor-signature compatibility with real callers, never stored or
        // used — matches the Python original exactly (they're accepted params, not `self.`
        // attributes there either; the real currency/manco extraction happens via separate
        // sibling pipeline stages, e.g. `PdfExtractCurrencyStandard`).
        let _ = manco_set;
        let _ = currency_set;

        let mut body_set = body_set;
        for dl in &deselection_list {
            body_set = body_set.bind(py).call_method1("__truediv__", (dl,))?.unbind();
        }

        let algorithm_flags = match algorithm_flags {
            Some(v) => v,
            None => table_pos_algorithm_zero(py)?,
        };
        let row_algorithm_flags = match row_algorithm_flags {
            Some(v) => v,
            None => table_pos_algorithm_zero(py)?,
        };

        Ok(Self { body_set, algorithm_flags, tolerance, row_algorithm_flags, row_tolerance, company_index })
    }

    #[getter]
    fn body_set(&self, py: Python<'_>) -> Py<PyAny> {
        self.body_set.clone_ref(py)
    }
    #[setter]
    fn set_body_set(&mut self, v: Py<PyAny>) {
        self.body_set = v;
    }
    #[getter]
    fn algorithm_flags(&self, py: Python<'_>) -> Py<PyAny> {
        self.algorithm_flags.clone_ref(py)
    }
    #[setter]
    fn set_algorithm_flags(&mut self, v: Py<PyAny>) {
        self.algorithm_flags = v;
    }
    #[getter]
    fn row_algorithm_flags(&self, py: Python<'_>) -> Py<PyAny> {
        self.row_algorithm_flags.clone_ref(py)
    }
    #[setter]
    fn set_row_algorithm_flags(&mut self, v: Py<PyAny>) {
        self.row_algorithm_flags = v;
    }

    fn __call__(&self, py: Python<'_>, dict_root: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PdfBlock>>> {
        let lines = pdflines_from_pagedict(py, dict_root)?;
        let table_rows = self.body_set.bind(py).call_method1("select", (&lines,))?;
        if table_rows.len()? == 0 {
            return Ok(vec![]);
        }

        let algorithm_flags = resolve_algorithm_flags(py, &self.algorithm_flags)?;

        let position_mod = position_module(py)?;
        let table_cfg = position_mod.getattr("TableConfig")?.call0()?;
        let collapse_alg = position_mod.getattr("CollapseAlgorithm")?.getattr("GEOMETRY")?;
        let get_table_coordinates = position_mod.getattr("get_table_coordinates")?;

        let kwargs = PyDict::new(py);
        kwargs.set_item("tolerance", self.row_tolerance)?;
        kwargs.set_item("company_col", self.company_index)?;
        kwargs.set_item("collapse", false)?;
        let coords = get_table_coordinates.call(
            (table_rows.clone(), table_cfg, algorithm_flags.bind(py), collapse_alg),
            Some(&kwargs),
        )?;
        let coords: Vec<(i64, i64)> = coords.extract()?;

        let rows: Vec<Bound<PyAny>> = table_rows.try_iter()?.collect::<PyResult<_>>()?;
        let widths: Vec<f64> = rows
            .iter()
            .map(|r| -> PyResult<f64> {
                let bbox = r.getattr("bbox")?;
                let x0: f64 = bbox.get_item(0)?.extract()?;
                let x1: f64 = bbox.get_item(2)?.extract()?;
                Ok(x1 - x0)
            })
            .collect::<PyResult<_>>()?;
        let max_width = widths.iter().cloned().fold(f64::MIN, f64::max);

        let mut blocks = Vec::with_capacity(rows.len());
        for (i, row) in rows.iter().enumerate() {
            let (row_pos, col_pos) = coords[i];
            let metadata = PyDict::new(py);
            metadata.set_item("table-row", row_pos)?;
            metadata.set_item("table-col", col_pos)?;
            metadata.set_item("is-max-width", widths[i] == max_width)?;
            let text = row.getattr("text")?.unbind();
            let blk = Py::new(py, PdfBlock::new("TABLE_BODY".to_string(), metadata.unbind(), text))?;
            blocks.push(blk);
        }
        Ok(blocks)
    }
}

/// `range(0, len, step)` restricted to what this call site actually needs: `step == 0` errors
/// (matching Python), `step < 0` yields nothing (also matching Python: `range(0, len, negative)`
/// is empty whenever `len > 0`, which is always true here since callers only reach this after an
/// empty-input early return).
fn range_0_to_len_step(len: usize, step: i64) -> PyResult<Vec<usize>> {
    if step == 0 {
        return Err(PyValueError::new_err("range() arg 3 must not be zero"));
    }
    let mut out = Vec::new();
    if step > 0 {
        let mut i: i64 = 0;
        while i < len as i64 {
            out.push(i as usize);
            i += step;
        }
    }
    Ok(out)
}

#[pyclass(module = "freeports_engine")]
#[allow(clippy::too_many_arguments)]
pub struct PdfExtractAssetsStandard {
    fund_selection: Py<PyAny>,
    currency_selection: Option<Py<PyAny>>,
    #[pyo3(get, set)]
    table_condition: bool,
    #[pyo3(get, set)]
    skip_column: i64,
    tot_assets_selction: Py<PyAny>,
    liabilities_selection: Py<PyAny>,
    net_assets_selection: Py<PyAny>,
    #[pyo3(get, set)]
    tot_assets_vector: (f64, f64),
    #[pyo3(get, set)]
    liabilities_vector: (f64, f64),
    #[pyo3(get, set)]
    net_assets_vector: (f64, f64),
    #[pyo3(get, set)]
    tot_assets_width: f64,
    #[pyo3(get, set)]
    liabilities_width: f64,
    #[pyo3(get, set)]
    net_assets_width: f64,
    #[pyo3(get, set)]
    tot_assets_height: f64,
    #[pyo3(get, set)]
    liabilities_height: f64,
    #[pyo3(get, set)]
    net_assets_height: f64,
    select_date: Option<Py<PyAny>>,
}

#[pymethods]
impl PdfExtractAssetsStandard {
    #[new]
    #[pyo3(signature = (
        fund_set,
        currency_set,
        net_assets_set,
        liabilities_set,
        tot_assets_set,
        net_assets_vec=(1.2, 0.0),
        liabilities_vec=(1.2, 0.0),
        tot_assets_vec=(1.2, 0.0),
        net_assets_mult=(100.0, 1.3),
        liabilities_mult=(100.0, 1.3),
        tot_assets_mult=(100.0, 1.3),
        date_set=None,
        table_condition=false,
        skip_column=1,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        fund_set: Py<PyAny>,
        currency_set: Option<Py<PyAny>>,
        net_assets_set: Py<PyAny>,
        liabilities_set: Py<PyAny>,
        tot_assets_set: Py<PyAny>,
        net_assets_vec: (f64, f64),
        liabilities_vec: (f64, f64),
        tot_assets_vec: (f64, f64),
        net_assets_mult: (f64, f64),
        liabilities_mult: (f64, f64),
        tot_assets_mult: (f64, f64),
        date_set: Option<Py<PyAny>>,
        table_condition: bool,
        skip_column: i64,
    ) -> PyResult<Self> {
        let (fund_selection, currency_selection): (Py<PyAny>, Option<Py<PyAny>>) = if !table_condition {
            let fund_sel = Py::new(py, SelectExpectedText::new(fund_set, "fund".to_string()))?.into_any();
            let currency_sel = Py::new(
                py,
                SelectExpectedText::new(
                    currency_set.ok_or_else(|| PyValueError::new_err("currency_set is required unless table_condition is True"))?,
                    "currency".to_string(),
                ),
            )?
            .into_any();
            (fund_sel, Some(currency_sel))
        } else {
            let currency_sel = match currency_set {
                Some(cs) => Some(Py::new(py, SelectExpectedText::new(cs, "currency".to_string()))?.into_any()),
                None => None,
            };
            (fund_set, currency_sel)
        };

        let select_date = match date_set {
            Some(ds) => Some(Py::new(py, SelectExpectedText::new(ds, "fund assets date".to_string()))?.into_any()),
            None => None,
        };

        Ok(Self {
            fund_selection,
            currency_selection,
            table_condition,
            skip_column,
            tot_assets_selction: tot_assets_set,
            liabilities_selection: liabilities_set,
            net_assets_selection: net_assets_set,
            tot_assets_vector: tot_assets_vec,
            liabilities_vector: liabilities_vec,
            net_assets_vector: net_assets_vec,
            tot_assets_width: tot_assets_mult.0,
            liabilities_width: liabilities_mult.0,
            net_assets_width: net_assets_mult.0,
            tot_assets_height: tot_assets_mult.1,
            liabilities_height: liabilities_mult.1,
            net_assets_height: net_assets_mult.1,
            select_date,
        })
    }

    fn __call__(&self, py: Python<'_>, dict_root: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PdfBlock>>> {
        let raw_lines = pdflines_from_pagedict(py, dict_root)?;

        let pdf_line_selection = pdf_line_selection_class(py)?;
        let empty_text = pdf_line_selection.call_method1("text", ("",))?;
        let blank_text = pdf_line_selection.call_method1("text", ("^ $",))?;
        let filter_sel = empty_text.call_method1("__truediv__", (blank_text,))?;
        let lines = filter_sel.call_method1("select", (&raw_lines,))?;

        let select_area = |anchor: &Py<PyAny>, vector: (f64, f64), width: f64, height: f64| -> PyResult<Bound<'_, PyAny>> {
            pdf_line_selection
                .call_method1("area_from_movewindow", (anchor.bind(py), vector, width, height))?
                .call_method1("select", (&lines,))
        };

        let tot_assets = select_area(&self.tot_assets_selction, self.tot_assets_vector, self.tot_assets_width, self.tot_assets_height)?;
        let liabilities = select_area(&self.liabilities_selection, self.liabilities_vector, self.liabilities_width, self.liabilities_height)?;
        let net_assets = select_area(&self.net_assets_selection, self.net_assets_vector, self.net_assets_width, self.net_assets_height)?;

        let tot_assets_items: Vec<Bound<PyAny>> = tot_assets.try_iter()?.collect::<PyResult<_>>()?;
        let liabilities_items: Vec<Bound<PyAny>> = liabilities.try_iter()?.collect::<PyResult<_>>()?;
        let net_assets_items: Vec<Bound<PyAny>> = net_assets.try_iter()?.collect::<PyResult<_>>()?;

        let indices = range_0_to_len_step(tot_assets_items.len(), self.skip_column)?;
        let tot_assets_sub: Vec<&Bound<PyAny>> = indices.iter().map(|&i| &tot_assets_items[i]).collect();
        let liabilities_sub: Vec<&Bound<PyAny>> = indices.iter().map(|&i| &liabilities_items[i]).collect();
        let net_assets_sub: Vec<&Bound<PyAny>> = indices.iter().map(|&i| &net_assets_items[i]).collect();

        let (fund_texts, currency_texts): (Vec<String>, Vec<String>) = if !self.table_condition {
            let fund_text: String = self.fund_selection.bind(py).call1((&lines,))?.extract()?;
            let currency_text: String = self
                .currency_selection
                .as_ref()
                .expect("currency_selection is always Some when table_condition is False")
                .bind(py)
                .call1((&lines,))?
                .extract()?;
            (vec![fund_text], vec![currency_text])
        } else {
            let funds_raw = self.fund_selection.bind(py).call_method1("select", (&lines,))?;
            let position_mod = position_module(py)?;
            let table_pos_algorithm = position_mod.getattr("TablePosAlgorithm")?;
            let combined_flags = table_pos_algorithm
                .getattr("BIG_CELL_RULE")?
                .call_method1("__or__", (table_pos_algorithm.getattr("USE_RULER_AREA")?,))?;
            let get_table_coordinates = position_mod.getattr("get_table_coordinates")?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("algorithm_flags", combined_flags)?;
            let coords = get_table_coordinates.call((funds_raw.clone(),), Some(&kwargs))?;
            let cols: Vec<i64> = coords.try_iter()?.map(|c| -> PyResult<i64> { c?.get_item(1)?.extract() }).collect::<PyResult<_>>()?;
            let n_cols = cols.iter().max().copied().map(|m| m + 1).unwrap_or(0);

            let funds_items: Vec<Bound<PyAny>> = funds_raw.try_iter()?.collect::<PyResult<_>>()?;
            let mut fund_texts = Vec::with_capacity(n_cols as usize);
            for col in 0..n_cols {
                let mut parts = Vec::new();
                for (f, &c) in funds_items.iter().zip(cols.iter()) {
                    if c == col {
                        let text: String = f.getattr("text")?.extract()?;
                        parts.push(text.trim().to_string());
                    }
                }
                fund_texts.push(parts.join(" "));
            }

            let currency_texts = if let Some(currency_selection) = &self.currency_selection {
                let currency_text: String = currency_selection.bind(py).call1((&lines,))?.extract()?;
                vec![currency_text; fund_texts.len()]
            } else {
                let mut new_funds = Vec::with_capacity(fund_texts.len());
                let mut new_currencies = Vec::with_capacity(fund_texts.len());
                for f in &fund_texts {
                    let parts: Vec<&str> = f.split_whitespace().collect();
                    if parts.is_empty() {
                        return Err(PyIndexError::new_err("list index out of range"));
                    }
                    let (fund_part, currency_part) = parts.split_at(parts.len() - 1);
                    new_funds.push(fund_part.join(" "));
                    new_currencies.push(currency_part[0].to_string());
                }
                fund_texts = new_funds;
                new_currencies
            };
            (fund_texts, currency_texts)
        };

        let tot_assets_text: Vec<String> =
            tot_assets_sub.iter().map(|t| -> PyResult<String> { t.getattr("text")?.extract() }).collect::<PyResult<_>>()?;
        let liabilities_text: Vec<String> =
            liabilities_sub.iter().map(|t| -> PyResult<String> { t.getattr("text")?.extract() }).collect::<PyResult<_>>()?;
        let net_assets_text: Vec<String> =
            net_assets_sub.iter().map(|t| -> PyResult<String> { t.getattr("text")?.extract() }).collect::<PyResult<_>>()?;

        let date_val: Option<String> = match &self.select_date {
            Some(sd) => Some(sd.bind(py).call1((&lines,))?.extract()?),
            None => None,
        };

        let n_out = fund_texts
            .len()
            .min(currency_texts.len())
            .min(tot_assets_text.len())
            .min(liabilities_text.len())
            .min(net_assets_text.len());

        let mut blocks = Vec::with_capacity(n_out);
        for i in 0..n_out {
            let metadata = PyDict::new(py);
            metadata.set_item("fund", &fund_texts[i])?;
            metadata.set_item("currency", &currency_texts[i])?;
            metadata.set_item("tot_assets", &tot_assets_text[i])?;
            metadata.set_item("liabilities", &liabilities_text[i])?;
            metadata.set_item("net_assets", &net_assets_text[i])?;
            metadata.set_item("date", &date_val)?;
            let content = "".into_pyobject(py)?.into_any().unbind();
            let blk = Py::new(py, PdfBlock::new("RELEVANT_BLOCK".to_string(), metadata.unbind(), content))?;
            blocks.push(blk);
        }
        Ok(blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::ffi::c_str;

    fn pdf_line_selection<'py>(py: Python<'py>) -> Bound<'py, PyAny> {
        pdf_line_selection_class(py).unwrap()
    }

    fn text_selection<'py>(py: Python<'py>, text: &str) -> Bound<'py, PyAny> {
        pdf_line_selection(py).call_method1("text", (text,)).unwrap()
    }

    fn sample_page(py: Python<'_>) -> Bound<'_, PyAny> {
        py.eval(
            c_str!(
                "{'width': 300.0, 'height': 300.0, 'blocks': [{'type': 0, 'lines': [\
                 {'dir': (1.0, 0.0), 'bbox': (0.0, 0.0, 60.0, 10.0), 'spans': [\
                 {'font': 'Arial', 'size': 10.0, 'text': 'Article 9 disclosure', 'bbox': (0.0, 0.0, 60.0, 10.0)}\
                 ]},\
                 {'dir': (1.0, 0.0), 'bbox': (0.0, 20.0, 60.0, 30.0), 'spans': [\
                 {'font': 'Arial', 'size': 10.0, 'text': 'Product name: Fund X', 'bbox': (0.0, 20.0, 60.0, 30.0)}\
                 ]}]}]}"
            ),
            None,
            None,
        )
        .unwrap()
    }

    fn empty_page(py: Python<'_>) -> Bound<'_, PyAny> {
        py.eval(c_str!("{'width': 100.0, 'height': 100.0, 'blocks': [{'type': 0, 'lines': []}]}"), None, None).unwrap()
    }

    fn two_row_table_page(py: Python<'_>) -> Bound<'_, PyAny> {
        py.eval(
            c_str!(
                "{'width': 300.0, 'height': 300.0, 'blocks': [{'type': 0, 'lines': [\
                 {'dir': (1.0, 0.0), 'bbox': (0.0, 0.0, 20.0, 10.0), 'spans': [{'font': 'Arial', 'size': 10.0, 'text': 'Row1Col1', 'bbox': (0.0, 0.0, 20.0, 10.0)}]},\
                 {'dir': (1.0, 0.0), 'bbox': (30.0, 0.0, 50.0, 10.0), 'spans': [{'font': 'Arial', 'size': 10.0, 'text': 'Row1Col2', 'bbox': (30.0, 0.0, 50.0, 10.0)}]},\
                 {'dir': (1.0, 0.0), 'bbox': (0.0, 20.0, 20.0, 30.0), 'spans': [{'font': 'Arial', 'size': 10.0, 'text': 'Row2Col1', 'bbox': (0.0, 20.0, 20.0, 30.0)}]},\
                 {'dir': (1.0, 0.0), 'bbox': (30.0, 20.0, 50.0, 30.0), 'spans': [{'font': 'Arial', 'size': 10.0, 'text': 'Row2Col2', 'bbox': (30.0, 20.0, 50.0, 30.0)}]}\
                 ]}]}"
            ),
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn sfdr_article_detects_art9() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let sfdr = PdfExtractSfdrArticleStandard::new(
                text_selection(py, "Article 9").unbind(),
                text_selection(py, "Article 8").unbind(),
                text_selection(py, "Product name").unbind(),
            );
            let page = sample_page(py);
            let blocks = sfdr.__call__(py, &page).unwrap();
            let blk = blocks[0].bind(py);
            let metadata = blk.getattr("metadata").unwrap();
            let article: SfdrArticle = metadata.get_item("article").unwrap().extract().unwrap();
            assert_eq!(article, SfdrArticle::ART_9);
            let content: String = blk.getattr("content").unwrap().extract().unwrap();
            assert_eq!(content, "Product name: Fund X");
        });
    }

    #[test]
    fn sfdr_article_defaults_to_art6_when_no_flag_matches() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let sfdr = PdfExtractSfdrArticleStandard::new(
                text_selection(py, "nonexistent-9").unbind(),
                text_selection(py, "nonexistent-8").unbind(),
                text_selection(py, "Product name").unbind(),
            );
            let page = sample_page(py);
            let blocks = sfdr.__call__(py, &page).unwrap();
            let blk = blocks[0].bind(py);
            let metadata = blk.getattr("metadata").unwrap();
            let article: SfdrArticle = metadata.get_item("article").unwrap().extract().unwrap();
            assert_eq!(article, SfdrArticle::ART_6);
        });
    }

    #[test]
    fn sfdr_article_raises_expected_pdf_block_not_found_without_fund_match() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let sfdr = PdfExtractSfdrArticleStandard::new(
                text_selection(py, "Article 9").unbind(),
                text_selection(py, "Article 8").unbind(),
                text_selection(py, "no such text").unbind(),
            );
            let page = sample_page(py);
            let err = sfdr.__call__(py, &page).unwrap_err();
            assert!(err.is_instance_of::<ExpectedPdfBlockNotFound>(py));
        });
    }

    #[test]
    fn currency_constant_returns_same_block_every_call() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let cc = PdfExtractCurrencyConstant::new(py, crate::commons::consts::Currency::EUR).unwrap();
            let page = empty_page(py);
            let a = cc.__call__(py, &page);
            let b = cc.__call__(py, &page);
            let content_a: String = a[0].bind(py).getattr("content").unwrap().extract().unwrap();
            let type_a: String = a[0].bind(py).getattr("type_block").unwrap().extract().unwrap();
            assert_eq!(content_a, "EUR");
            assert_eq!(type_a, "CURRENCY_STATEMENT");
            assert!(a[0].bind(py).is(b[0].bind(py)));
        });
    }

    #[test]
    fn page_classify_all_headers_match_sets_page_type() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let selections = pyo3::types::PyList::new(py, [text_selection(py, "Article 9")]).unwrap();
            let pc = PdfExtractPageClassifyStandard::new(selections.as_any(), "sfdr".to_string()).unwrap();
            let page = sample_page(py);
            let blocks = pc.__call__(py, &page).unwrap();
            let metadata = blocks[0].bind(py).getattr("metadata").unwrap();
            let page_type: Option<String> = metadata.get_item("page_type").unwrap().extract().unwrap();
            assert_eq!(page_type, Some("sfdr".to_string()));
        });
    }

    #[test]
    fn page_classify_missing_header_sets_none() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let selections =
                pyo3::types::PyList::new(py, [text_selection(py, "Article 9"), text_selection(py, "nonexistent")]).unwrap();
            let pc = PdfExtractPageClassifyStandard::new(selections.as_any(), "sfdr".to_string()).unwrap();
            let page = sample_page(py);
            let blocks = pc.__call__(py, &page).unwrap();
            let metadata = blocks[0].bind(py).getattr("metadata").unwrap();
            let page_type: Option<String> = metadata.get_item("page_type").unwrap().extract().unwrap();
            assert_eq!(page_type, None);
        });
    }

    #[test]
    fn page_classify_accepts_a_single_non_iterable_selection() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let single = text_selection(py, "Article 9");
            let pc = PdfExtractPageClassifyStandard::new(&single, "sfdr".to_string()).unwrap();
            assert_eq!(pc.header_sets.len(), 1);
        });
    }

    #[test]
    fn investments_returns_empty_for_no_matching_rows() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let body_set = text_selection(py, "nonexistent").unbind();
            let inv =
                PdfExtractInvestmentsStandard::new(py, body_set, None, None, Vec::new(), None, 0.0, None, 0.0, None).unwrap();
            let page = two_row_table_page(py);
            let blocks = inv.__call__(py, &page).unwrap();
            assert!(blocks.is_empty());
        });
    }

    #[test]
    fn investments_builds_table_body_blocks_with_row_col_positions() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let body_set = pdf_line_selection(py).call_method1("font", ("Arial",)).unwrap().unbind();
            let inv =
                PdfExtractInvestmentsStandard::new(py, body_set, None, None, Vec::new(), None, 0.0, None, 0.0, None).unwrap();
            let page = two_row_table_page(py);
            let blocks = inv.__call__(py, &page).unwrap();
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

    #[test]
    fn investments_deselection_list_removes_matched_rows() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let body_set = pdf_line_selection(py).call_method1("font", ("Arial",)).unwrap().unbind();
            let deselect = text_selection(py, "Row1Col1").unbind();
            let inv = PdfExtractInvestmentsStandard::new(py, body_set, None, None, vec![deselect], None, 0.0, None, 0.0, None)
                .unwrap();
            let page = two_row_table_page(py);
            let blocks = inv.__call__(py, &page).unwrap();
            assert_eq!(blocks.len(), 3);
            for b in &blocks {
                let content: String = b.bind(py).getattr("content").unwrap().extract().unwrap();
                assert_ne!(content, "Row1Col1");
            }
        });
    }

    #[test]
    fn investments_tolerance_and_row_algorithm_flags_are_stored_but_unused() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let body_set = text_selection(py, "nonexistent").unbind();
            let inv =
                PdfExtractInvestmentsStandard::new(py, body_set, None, None, Vec::new(), None, 7.5, None, 0.0, Some(3))
                    .unwrap();
            assert_eq!(inv.tolerance, 7.5);
            assert_eq!(inv.company_index, Some(3));
            let _flags = inv.row_algorithm_flags(py);
        });
    }

    #[test]
    fn range_0_to_len_step_positive_step() {
        assert_eq!(range_0_to_len_step(10, 2).unwrap(), vec![0, 2, 4, 6, 8]);
        assert_eq!(range_0_to_len_step(5, 1).unwrap(), vec![0, 1, 2, 3, 4]);
        assert_eq!(range_0_to_len_step(3, 10).unwrap(), vec![0]);
    }

    #[test]
    fn range_0_to_len_step_zero_step_errors() {
        assert!(range_0_to_len_step(10, 0).is_err());
    }

    #[test]
    fn range_0_to_len_step_negative_step_is_empty() {
        assert_eq!(range_0_to_len_step(10, -1).unwrap(), Vec::<usize>::new());
    }

    #[test]
    fn assets_standard_requires_currency_set_unless_table_condition() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let fund_set = text_selection(py, "Fund").unbind();
            let net = text_selection(py, "Net").unbind();
            let liab = text_selection(py, "Liab").unbind();
            let tot = text_selection(py, "Tot").unbind();
            let result = PdfExtractAssetsStandard::new(
                py, fund_set, None, net, liab, tot,
                (1.2, 0.0), (1.2, 0.0), (1.2, 0.0),
                (100.0, 1.3), (100.0, 1.3), (100.0, 1.3),
                None, false, 1,
            );
            let Err(err) = result else { panic!("expected an error") };
            assert!(err.is_instance_of::<PyValueError>(py));
        });
    }

    #[test]
    fn assets_standard_allows_none_currency_when_table_condition_true() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let fund_set = text_selection(py, "Fund").unbind();
            let net = text_selection(py, "Net").unbind();
            let liab = text_selection(py, "Liab").unbind();
            let tot = text_selection(py, "Tot").unbind();
            let assets = PdfExtractAssetsStandard::new(
                py, fund_set, None, net, liab, tot,
                (1.2, 0.0), (1.2, 0.0), (1.2, 0.0),
                (100.0, 1.3), (100.0, 1.3), (100.0, 1.3),
                None, true, 1,
            )
            .unwrap();
            assert!(assets.currency_selection.is_none());
            assert!(assets.table_condition);
        });
    }
}
