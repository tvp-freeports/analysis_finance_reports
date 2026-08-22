//! Rust port of `formats/utils/text_filter/standard_funcs.py`.
//!
//! Scope decided explicitly by the user (2026-08-19): port the 5 "Standard" filter classes
//! (`TextFilterSfdrArticleStandard`, `TextFilterPageClassifyStandard`,
//! `TextFilterInvestmentsStandard`, `TextFilterAssetsStandard`, `TextFilterManagmentCompanyStandard`)
//! plus `extract_currency_from_text` — these mirror the already-ported `PdfExtract*Standard`
//! classes, and their job (per the user) isn't to *be* Rust classes for their own sake, it's to
//! build a `TextBlock` shaped exactly the way the corresponding `Deserializer*Standard` (already
//! ported, see `deserialize/standard_funcs.rs`) expects to consume it — so every metadata key and
//! `type_block` string below is chosen to match that consumer, not invented fresh.
//!
//! **Also ported, at the user's explicit request**: all 8 decorator factories at the top of the
//! Python original (`filter_block_type[s]`, their `_call` method-decorator variants,
//! `fund_filter_data`/`investment_fund_filter_data` and their `_call` variants) — 7 of these are
//! unused anywhere today, but the user considers them useful future format-authoring API, so they
//! aren't just dead weight to skip. Each decorator wraps an *arbitrary* Python callable (`f`);
//! that's implemented the same way every other Python-boundary crossing in this migration is —
//! `f` stays a generic `Py<PyAny>`, called back into via `call1`, never inspected. The `_call`
//! variants specifically decorate *methods*, so they additionally implement `__get__` (the
//! descriptor protocol) so that `instance.decorated_method(...)` still auto-binds `self` the way
//! it would for a plain Python function — a plain pyclass instance stored as a class attribute
//! does *not* get that binding for free the way a real Python function does, so this had to be
//! added explicitly (verified: without `__get__`, `self` would silently never be passed).
//!
//! `PdfBlocksTable` and `standard_text_filterion_loop` (the Python original's internal machinery)
//! have exactly one real caller between them — `TextFilterInvestmentsStandard` itself — so they
//! are inlined into that class's Rust port as a private, non-`pyclass` implementation detail
//! rather than kept as a second, separately-exposed generic API nothing else would ever call.
//!
//! **Bug found and fixed at the root (user confirmed, 2026-08-19)**: `extract_currency_from_text`
//! had a dead `found` flag — initialized `False`, never set `True` — so the exhaustive
//! `Currency.__members__` fallback scan always ran even after the fast regex path already found
//! a valid currency, and silently overwrote it. Verified empirically: `"Converted from USD to
//! EUR"` and `"Converted from EUR to USD"` both returned `Currency.EUR` regardless of which code
//! actually appeared first in the text (the fallback's result depends on `Currency` enum
//! declaration order, not text order). Fixed by setting `found = True` once the fast path
//! succeeds, matching the obviously intended behavior; verified the fix makes both examples above
//! return whichever currency is actually mentioned first.

use onig::Regex;
use pyo3::exceptions::{PyStopIteration, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet};

use crate::commons::consts::Currency;
use crate::core::classes::{ExpectedTextBlockNotFound, PageParseFail, PdfBlock, TextBlock};
use crate::output::fund::Fund;
use crate::output::investment::{Bond, Equity};

fn standard_txt_blks_module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    py.import("freeports._internals.formats.utils.text_filter.standard_txt_blks")
}

/// **History**: the field-extraction warnings/errors below were once routed through real Python
/// `logging` (via `text_filter_logger`/`LOG_ADAPT_INVESTMENT_INFOS`, now removed), because every
/// `.log.csv` fixture in the formats repo is built from *exactly* those calls (a
/// `logging.FileHandler` at `WARNING` level, wired up in `_internals/cli/main.py`, reading
/// `LOG_ADAPT_INVESTMENT_INFOS`'s `row`/`company`/`company_match` plus each call's own
/// `extra={...}`) — confirmed the hard way at the time: 4 formats' `test_pipeline` failed on
/// `.log.csv` row-count mismatches without them. **Removed again, deliberately, per the
/// Python-elimination plan** (`agent-memory/python-circumscription-plan.md`, Fase 2): `core/
/// logging.py` isn't ported to Rust yet, and per the user's explicit new priority, keeping the
/// Python side working at every intermediate step is no longer the goal — Rust-side logging (via
/// `tracing`, already used elsewhere in this crate) can replace this properly once `core/
/// logging.py` itself is ported, rather than round-tripping into Python `logging` in the
/// meantime. Known, deliberate consequence: `.log.csv` fixture rows are missing again until then.
fn translate(py: Python<'_>, msg: &str) -> PyResult<String> {
    py.import("freeports.i18n")?.getattr("_")?.call1((msg,))?.extract()
}

fn match_fund_class(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("freeports._internals.core.match")?.getattr("MatchFund")
}

/// Builds a real `match.MatchFund` (the Python bridge class, not the raw
/// `freeports._native.core.MatchFund` it wraps) — deliberately, even though this crate could
/// construct the raw core type directly: the sets built here (via `get_funds`/
/// `get_investment_funds`) are handed to *arbitrary* format-author code through the
/// `fund_filter_data`/`investment_fund_filter_data` decorators, and preserving the exact
/// original type means any `isinstance(x, match.MatchFund)` a format author writes keeps working.
fn make_match_fund(py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
    Ok(match_fund_class(py)?.call1((name,))?.unbind())
}

fn re_pattern_type(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("re")?.getattr("Pattern")
}

/// `set(map(MatchFund, filter(isinstance Fund, filter_data)))`.
fn get_funds(py: Python<'_>, filter_data: &Bound<'_, PyAny>) -> PyResult<Py<PySet>> {
    let set = PySet::empty(py)?;
    for item in filter_data.try_iter()? {
        let item = item?;
        if item.is_instance_of::<Fund>() {
            let name: String = item.getattr("name")?.extract()?;
            set.add(make_match_fund(py, &name)?)?;
        }
    }
    Ok(set.unbind())
}

/// `set(map(lambda f: MatchFund(f.fund), filter(isinstance (Equity, Bond), filter_data)))`.
fn get_investment_funds(py: Python<'_>, filter_data: &Bound<'_, PyAny>) -> PyResult<Py<PySet>> {
    let set = PySet::empty(py)?;
    for item in filter_data.try_iter()? {
        let item = item?;
        if item.is_instance_of::<Equity>() || item.is_instance_of::<Bond>() {
            let fund: String = item.getattr("fund")?.extract()?;
            set.add(make_match_fund(py, &fund)?)?;
        }
    }
    Ok(set.unbind())
}

fn filter_pdf_blks_by_type<'py>(py: Python<'py>, pdf_blks: &Bound<'py, PyAny>, blk_type: &str) -> PyResult<Bound<'py, PyList>> {
    let filtered = PyList::empty(py);
    for item in pdf_blks.try_iter()? {
        let item = item?;
        let type_block: String = item.getattr("type_block")?.extract()?;
        if type_block == blk_type {
            filtered.append(&item)?;
        }
    }
    Ok(filtered)
}

fn filter_pdf_blks_by_types<'py>(py: Python<'py>, pdf_blks: &Bound<'py, PyAny>, blk_types: &[String]) -> PyResult<Bound<'py, PyList>> {
    let filtered = PyList::empty(py);
    for item in pdf_blks.try_iter()? {
        let item = item?;
        let type_block: String = item.getattr("type_block")?.extract()?;
        if blk_types.iter().any(|t| t == &type_block) {
            filtered.append(&item)?;
        }
    }
    Ok(filtered)
}

/// Builds `functools.partial(applied.__call__, instance)` — the descriptor-protocol return value
/// that makes a pyclass instance behave like a bound method, the same way a plain Python function
/// stored as a class attribute would. See the module doc for why this is needed at all.
fn bind_method<'py>(py: Python<'py>, applied: &Bound<'py, PyAny>, instance: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let call = applied.getattr("__call__")?;
    py.import("functools")?.getattr("partial")?.call1((call, instance))
}

// ---------------------------------------------------------------------------------------------
// Decorators. 2-stage ones (`filter_block_type[s]`) are `blk_type(s) -> f -> new_f`; 1-stage ones
// (`fund_filter_data`, `investment_fund_filter_data`) are `f -> new_f` directly.
// ---------------------------------------------------------------------------------------------

#[pyclass(name = "filter_block_type", module = "freeports._native")]
pub struct FilterBlockType {
    blk_type: String,
}

#[pymethods]
impl FilterBlockType {
    #[new]
    fn new(blk_type: String) -> Self {
        Self { blk_type }
    }

    fn __call__(&self, f: Py<PyAny>) -> FilterBlockTypeApplied {
        FilterBlockTypeApplied { blk_type: self.blk_type.clone(), f }
    }
}

#[pyclass(module = "freeports._native")]
pub struct FilterBlockTypeApplied {
    blk_type: String,
    f: Py<PyAny>,
}

#[pymethods]
impl FilterBlockTypeApplied {
    fn __call__(&self, py: Python<'_>, pdf_blks: &Bound<'_, PyAny>, filter_data: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let filtered = filter_pdf_blks_by_type(py, pdf_blks, &self.blk_type)?;
        Ok(self.f.bind(py).call1((filtered, filter_data))?.unbind())
    }
}

#[pyclass(name = "filter_block_types", module = "freeports._native")]
pub struct FilterBlockTypes {
    blk_types: Vec<String>,
}

#[pymethods]
impl FilterBlockTypes {
    #[new]
    #[pyo3(signature = (*blk_types))]
    fn new(blk_types: Vec<String>) -> Self {
        Self { blk_types }
    }

    fn __call__(&self, f: Py<PyAny>) -> FilterBlockTypesApplied {
        FilterBlockTypesApplied { blk_types: self.blk_types.clone(), f }
    }
}

#[pyclass(module = "freeports._native")]
pub struct FilterBlockTypesApplied {
    blk_types: Vec<String>,
    f: Py<PyAny>,
}

#[pymethods]
impl FilterBlockTypesApplied {
    fn __call__(&self, py: Python<'_>, pdf_blks: &Bound<'_, PyAny>, filter_data: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let filtered = filter_pdf_blks_by_types(py, pdf_blks, &self.blk_types)?;
        Ok(self.f.bind(py).call1((filtered, filter_data))?.unbind())
    }
}

#[pyclass(name = "filter_block_type_call", module = "freeports._native")]
pub struct FilterBlockTypeCall {
    blk_type: String,
}

#[pymethods]
impl FilterBlockTypeCall {
    #[new]
    fn new(blk_type: String) -> Self {
        Self { blk_type }
    }

    fn __call__(&self, f: Py<PyAny>) -> FilterBlockTypeCallApplied {
        FilterBlockTypeCallApplied { blk_type: self.blk_type.clone(), f }
    }
}

#[pyclass(module = "freeports._native")]
pub struct FilterBlockTypeCallApplied {
    blk_type: String,
    f: Py<PyAny>,
}

#[pymethods]
impl FilterBlockTypeCallApplied {
    fn __call__(&self, py: Python<'_>, self_arg: Py<PyAny>, pdf_blks: &Bound<'_, PyAny>, filter_data: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let filtered = filter_pdf_blks_by_type(py, pdf_blks, &self.blk_type)?;
        Ok(self.f.bind(py).call1((self_arg, filtered, filter_data))?.unbind())
    }

    fn __get__<'py>(
        slf: Bound<'py, Self>,
        py: Python<'py>,
        obj: Option<Bound<'py, PyAny>>,
        _objtype: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match obj {
            None => Ok(slf.unbind().into_any()),
            Some(instance) => Ok(bind_method(py, slf.as_any(), &instance)?.unbind()),
        }
    }
}

#[pyclass(name = "filter_block_types_call", module = "freeports._native")]
pub struct FilterBlockTypesCall {
    blk_types: Vec<String>,
}

#[pymethods]
impl FilterBlockTypesCall {
    #[new]
    #[pyo3(signature = (*blk_types))]
    fn new(blk_types: Vec<String>) -> Self {
        Self { blk_types }
    }

    fn __call__(&self, f: Py<PyAny>) -> FilterBlockTypesCallApplied {
        FilterBlockTypesCallApplied { blk_types: self.blk_types.clone(), f }
    }
}

#[pyclass(module = "freeports._native")]
pub struct FilterBlockTypesCallApplied {
    blk_types: Vec<String>,
    f: Py<PyAny>,
}

#[pymethods]
impl FilterBlockTypesCallApplied {
    fn __call__(&self, py: Python<'_>, self_arg: Py<PyAny>, pdf_blks: &Bound<'_, PyAny>, filter_data: Py<PyAny>) -> PyResult<Py<PyAny>> {
        let filtered = filter_pdf_blks_by_types(py, pdf_blks, &self.blk_types)?;
        Ok(self.f.bind(py).call1((self_arg, filtered, filter_data))?.unbind())
    }

    fn __get__<'py>(
        slf: Bound<'py, Self>,
        py: Python<'py>,
        obj: Option<Bound<'py, PyAny>>,
        _objtype: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match obj {
            None => Ok(slf.unbind().into_any()),
            Some(instance) => Ok(bind_method(py, slf.as_any(), &instance)?.unbind()),
        }
    }
}

#[pyclass(name = "fund_filter_data", module = "freeports._native")]
pub struct FundFilterData {
    f: Py<PyAny>,
}

#[pymethods]
impl FundFilterData {
    #[new]
    fn new(f: Py<PyAny>) -> Self {
        Self { f }
    }

    fn __call__(&self, py: Python<'_>, pdf_blks: Py<PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let funds = get_funds(py, filter_data)?;
        Ok(self.f.bind(py).call1((pdf_blks, funds))?.unbind())
    }
}

#[pyclass(name = "fund_filter_data_call", module = "freeports._native")]
pub struct FundFilterDataCall {
    f: Py<PyAny>,
}

#[pymethods]
impl FundFilterDataCall {
    #[new]
    fn new(f: Py<PyAny>) -> Self {
        Self { f }
    }

    fn __call__(&self, py: Python<'_>, self_arg: Py<PyAny>, pdf_blks: Py<PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let funds = get_funds(py, filter_data)?;
        Ok(self.f.bind(py).call1((self_arg, pdf_blks, funds))?.unbind())
    }

    fn __get__<'py>(
        slf: Bound<'py, Self>,
        py: Python<'py>,
        obj: Option<Bound<'py, PyAny>>,
        _objtype: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match obj {
            None => Ok(slf.unbind().into_any()),
            Some(instance) => Ok(bind_method(py, slf.as_any(), &instance)?.unbind()),
        }
    }
}

#[pyclass(name = "investment_fund_filter_data", module = "freeports._native")]
pub struct InvestmentFundFilterData {
    f: Py<PyAny>,
}

#[pymethods]
impl InvestmentFundFilterData {
    #[new]
    fn new(f: Py<PyAny>) -> Self {
        Self { f }
    }

    fn __call__(&self, py: Python<'_>, pdf_blks: Py<PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let funds = get_investment_funds(py, filter_data)?;
        Ok(self.f.bind(py).call1((pdf_blks, funds))?.unbind())
    }
}

#[pyclass(name = "investment_fund_filter_data_call", module = "freeports._native")]
pub struct InvestmentFundFilterDataCall {
    f: Py<PyAny>,
}

#[pymethods]
impl InvestmentFundFilterDataCall {
    #[new]
    fn new(f: Py<PyAny>) -> Self {
        Self { f }
    }

    fn __call__(&self, py: Python<'_>, self_arg: Py<PyAny>, pdf_blks: Py<PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let funds = get_investment_funds(py, filter_data)?;
        Ok(self.f.bind(py).call1((self_arg, pdf_blks, funds))?.unbind())
    }

    fn __get__<'py>(
        slf: Bound<'py, Self>,
        py: Python<'py>,
        obj: Option<Bound<'py, PyAny>>,
        _objtype: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match obj {
            None => Ok(slf.unbind().into_any()),
            Some(instance) => Ok(bind_method(py, slf.as_any(), &instance)?.unbind()),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// extract_currency_from_text
// ---------------------------------------------------------------------------------------------

fn currency_code_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[A-Z]{3}\b").unwrap())
}

/// The Python original also accepted an already-`Currency` value as a same-type-identity
/// short-circuit (`isinstance(curr, Currency): res = curr`), despite its own `txt: str` type
/// hint. Dropped here: verified both real call sites (in `TextFilterInvestmentsStandard`/
/// `TextFilterAssetsStandard` below) and every external caller (none exist — this is exposed
/// publicly but never actually imported anywhere in the formats repo) always pass a plain string.
#[pyfunction]
#[pyo3(name = "extract_currency_from_text")]
pub fn py_extract_currency_from_text(text: &str) -> PyResult<Currency> {
    let mut candidates: Vec<&str> = Vec::new();
    for (s, e) in currency_code_re().find_iter(text) {
        candidates.push(&text[s..e]);
    }
    for cand in &candidates {
        if let Some(c) = Currency::from_name(cand) {
            return Ok(c);
        }
    }
    let upper = text.to_uppercase();
    for variant in Currency::variants() {
        if let Some(c) = try_member_name(&upper, variant.code()) {
            return Ok(c);
        }
    }
    if let Some(c) = try_member_name(&upper, "EURO") {
        return Ok(c);
    }
    Err(ExpectedTextBlockNotFound::new_err(format!("Currency not found in string: \"{upper}\"")))
}

fn try_member_name(haystack: &str, member_name: &str) -> Option<Currency> {
    let pattern = format!(r"\b{member_name}\b");
    let re = Regex::new(&pattern).ok()?;
    for (s, e) in re.find_iter(haystack) {
        if let Some(c) = Currency::from_name(&haystack[s..e]) {
            return Some(c);
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// TextFilterSfdrArticleStandard
// ---------------------------------------------------------------------------------------------

#[pyclass(module = "freeports._native")]
pub struct TextFilterSfdrArticleStandard {
    fund_prefix_strings: Vec<String>,
    fund_prefix_regexes: Vec<Py<PyAny>>,
    #[pyo3(get, set)]
    demand_investment_funds_match: bool,
}

#[pymethods]
impl TextFilterSfdrArticleStandard {
    #[new]
    #[pyo3(signature = (fund_prefix=None, demand_investment_funds_match=true))]
    fn new(py: Python<'_>, fund_prefix: Option<&Bound<'_, PyAny>>, demand_investment_funds_match: bool) -> PyResult<Self> {
        let mut fund_prefix_strings = Vec::new();
        let mut fund_prefix_regexes = Vec::new();
        if let Some(fund_prefix) = fund_prefix {
            let pattern_type = re_pattern_type(py)?;
            let items: Vec<Bound<'_, PyAny>> = if fund_prefix.is_instance_of::<pyo3::types::PyString>()
                || fund_prefix.is_instance(&pattern_type)?
            {
                vec![fund_prefix.clone()]
            } else {
                fund_prefix.try_iter()?.collect::<PyResult<_>>()?
            };
            for f in items {
                if f.is_instance_of::<pyo3::types::PyString>() {
                    fund_prefix_strings.push(f.extract()?);
                } else if f.is_instance(&pattern_type)? {
                    fund_prefix_regexes.push(f.unbind());
                }
            }
        }
        Ok(Self { fund_prefix_strings, fund_prefix_regexes, demand_investment_funds_match })
    }

    fn __call__(&self, py: Python<'_>, pdf_blks: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Py<TextBlock>>> {
        let investment_funds = get_investment_funds(py, filter_data)?;

        let blk = pdf_blks.try_iter()?.next().ok_or_else(|| PyStopIteration::new_err(()))??;
        let mut fund_name: String = blk.getattr("content")?.extract()?;
        for prefix in &self.fund_prefix_strings {
            fund_name = fund_name.replace(prefix.as_str(), "");
        }
        for regex in &self.fund_prefix_regexes {
            fund_name = regex.bind(py).call_method1("sub", ("", fund_name))?.extract()?;
        }

        let fund = make_match_fund(py, &fund_name)?;
        let matches = !self.demand_investment_funds_match || investment_funds.bind(py).contains(&fund)?;
        if matches {
            let metadata: Py<PyDict> = blk.getattr("metadata")?.extract()?;
            let content = fund_name.into_pyobject(py)?.into_any().unbind();
            let txt_blk = Py::new(py, TextBlock::from_content("SFDR_ARTICLE".to_string(), metadata, content))?;
            Ok(vec![txt_blk])
        } else {
            Ok(vec![])
        }
    }
}

// ---------------------------------------------------------------------------------------------
// TextFilterPageClassifyStandard
// ---------------------------------------------------------------------------------------------

#[pyclass(module = "freeports._native")]
pub struct TextFilterPageClassifyStandard;

#[pymethods]
impl TextFilterPageClassifyStandard {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __call__(&self, py: Python<'_>, pdf_blks: &Bound<'_, PyAny>, _filter_data: Py<PyAny>) -> PyResult<Vec<Py<TextBlock>>> {
        let mut page_classification: Option<Py<PyAny>> = None;
        let mut last_blk: Option<Py<PdfBlock>> = None;
        for item in pdf_blks.try_iter()? {
            let item = item?;
            last_blk = Some(item.extract()?);
            let page_type = item.getattr("metadata")?.get_item("page_type")?;
            if !page_type.is_none() {
                match &page_classification {
                    None => page_classification = Some(page_type.unbind()),
                    Some(existing) => {
                        let existing_repr: String = existing.bind(py).str()?.extract()?;
                        let new_repr: String = page_type.str()?.extract()?;
                        return Err(PyValueError::new_err(format!(
                            "page cannot be classified both as `{existing_repr}` and `{new_repr}`"
                        )));
                    }
                }
            }
        }
        let last_blk = last_blk.ok_or_else(|| PyValueError::new_err("pdf_blks must not be empty"))?;
        let metadata = PyDict::new(py);
        metadata.set_item("page_type", page_classification)?;
        let txt_blk = Py::new(py, TextBlock::new(py, "PAGE_CLASS".to_string(), metadata.unbind(), last_blk))?;
        Ok(vec![txt_blk])
    }
}

// ---------------------------------------------------------------------------------------------
// TextFilterManagmentCompanyStandard
// ---------------------------------------------------------------------------------------------

#[pyclass(module = "freeports._native")]
pub struct TextFilterManagmentCompanyStandard;

#[pymethods]
impl TextFilterManagmentCompanyStandard {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __call__(&self, py: Python<'_>, pdf_blks: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let filter_funds = PySet::empty(py)?;
        for item in filter_data.try_iter()? {
            let item = item?;
            if item.is_instance_of::<Fund>() {
                let name: String = item.getattr("name")?.extract()?;
                filter_funds.add(make_match_fund(py, &name)?)?;
            }
        }

        let mut manco_block: Option<Bound<'_, PyAny>> = None;
        for item in pdf_blks.try_iter()? {
            let item = item?;
            let type_block: String = item.getattr("type_block")?.extract()?;
            if type_block == "MANAGEMENT_COMPANY" {
                manco_block = Some(item);
                break;
            }
        }
        let manco_block = manco_block.ok_or_else(|| PyStopIteration::new_err(()))?;

        let factory = standard_txt_blks_module(py)?.getattr("StandardManagmentCompanyTextBlock")?;
        let txt_blk = factory.call1((manco_block, filter_funds))?;
        Ok(vec![txt_blk.unbind()])
    }
}

// ---------------------------------------------------------------------------------------------
// TextFilterAssetsStandard
// ---------------------------------------------------------------------------------------------

#[pyclass(module = "freeports._native")]
pub struct TextFilterAssetsStandard {
    date_regex: Option<Regex>,
    remove_from_fund_regexes: Vec<Regex>,
}

#[pymethods]
impl TextFilterAssetsStandard {
    #[new]
    #[pyo3(signature = (date_regex=None, remove_from_fund_regexes=None))]
    fn new(date_regex: Option<&str>, remove_from_fund_regexes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let date_regex = date_regex.map(Regex::new).transpose().map_err(|e| PyValueError::new_err(e.to_string()))?;

        let patterns: Vec<String> = match remove_from_fund_regexes {
            None => Vec::new(),
            Some(v) if v.is_instance_of::<pyo3::types::PyString>() => vec![v.extract()?],
            Some(v) => v.try_iter()?.map(|i| i?.extract()).collect::<PyResult<_>>()?,
        };
        let remove_from_fund_regexes = patterns
            .iter()
            .map(|p| Regex::new(p))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;

        Ok(Self { date_regex, remove_from_fund_regexes })
    }

    fn __call__(&self, py: Python<'_>, blks: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Py<TextBlock>>> {
        let filter_funds = PySet::empty(py)?;
        for item in filter_data.try_iter()? {
            let item = item?;
            if item.is_instance_of::<Fund>() {
                let name: String = item.getattr("name")?.extract()?;
                filter_funds.add(make_match_fund(py, &name)?)?;
            }
        }

        let mut results = Vec::new();
        for blk in blks.try_iter()? {
            let blk = blk?;
            let md = PyDict::new(py);
            for (k, v) in blk.getattr("metadata")?.cast::<PyDict>().map_err(PyErr::from)?.iter() {
                md.set_item(k, v)?;
            }

            let mut fund_name: String = md.get_item("fund")?.ok_or_else(|| PyValueError::new_err("missing 'fund' metadata"))?.extract()?;
            for regex in &self.remove_from_fund_regexes {
                fund_name = regex.replace_all(&fund_name, "");
            }
            md.set_item("fund", &fund_name)?;

            let candidate = make_match_fund(py, &fund_name)?;
            if filter_funds.contains(&candidate)? {
                if let Some(date_regex) = &self.date_regex {
                    let date_val: String = md.get_item("date")?.ok_or_else(|| PyValueError::new_err("missing 'date' metadata"))?.extract()?;
                    let captured = date_regex
                        .captures(&date_val)
                        .and_then(|c| c.at(1))
                        .ok_or_else(|| PyValueError::new_err("date_regex did not match"))?;
                    md.set_item("date", captured)?;
                }
                let currency_text: String = md.get_item("currency")?.ok_or_else(|| PyValueError::new_err("missing 'currency' metadata"))?.extract()?;
                let currency = py_extract_currency_from_text(&currency_text)?;
                md.set_item("currency", currency)?;

                let content = "".into_pyobject(py)?.into_any().unbind();
                let txt_blk = Py::new(py, TextBlock::from_content("RELEVANT_BLOCK".to_string(), md.clone().unbind(), content))?;
                results.push(txt_blk);
            }
        }
        Ok(results)
    }
}

// ---------------------------------------------------------------------------------------------
// TextFilterInvestmentsStandard — with PdfBlocksTable/standard_text_filterion_loop inlined,
// since this is their only real caller (see module doc).
// ---------------------------------------------------------------------------------------------

/// Inlined port of the Python original's `PdfBlocksTable`. Never exposed to Python — internal
/// implementation detail of `TextFilterInvestmentsStandard::__call__` only.
struct PdfBlocksTable {
    blks: Vec<Py<PyAny>>,
    /// row -> col -> flat indices into `blks` currently occupying that cell (usually 0 or 1
    /// entries; more than one is possible and handled, matching the Python original).
    table_indexes: Vec<Vec<Vec<usize>>>,
    /// row -> col -> the blocks themselves (same objects as `blks`, not copies — mutations via
    /// either view are visible from both, matching Python's aliasing).
    table: Vec<Vec<Vec<Py<PyAny>>>>,
}

enum Cell {
    Empty,
    One(Py<PyAny>),
    /// Multiple blocks share this cell — the Python original returns the raw list here, and its
    /// only two call sites either just check "is this occupied" (fine either way) or immediately
    /// do `.content` on the result, which raises `AttributeError` on a list and is caught,
    /// falling back to `None` — so `Many` is treated as "extraction failed" wherever content is
    /// read, matching that fallback exactly. The blocks themselves are kept only so this variant
    /// carries the same information the Python list would, even though nothing here reads them.
    #[allow(dead_code)]
    Many(Vec<Py<PyAny>>),
}

/// row -> col -> `(flat index, block)` entries seen for that cell while grouping the flat input
/// list, before it's turned into the dense `table`/`table_indexes` grids.
type RowColGroups = std::collections::BTreeMap<i64, std::collections::BTreeMap<i64, Vec<(usize, Py<PyAny>)>>>;

impl PdfBlocksTable {
    fn new(py: Python<'_>, pdf_blocks: &[Py<PyAny>]) -> PyResult<Self> {
        let blks: Vec<Py<PyAny>> = pdf_blocks.iter().map(|b| b.clone_ref(py)).collect();

        let mut dict_table: RowColGroups = RowColGroups::new();
        let mut col_max: i64 = 0;
        for (i, blk) in blks.iter().enumerate() {
            let metadata = blk.bind(py).getattr("metadata")?;
            let row: i64 = metadata.get_item("table-row")?.extract()?;
            let col: i64 = metadata.get_item("table-col")?.extract()?;
            col_max = col_max.max(col);
            dict_table.entry(row).or_default().entry(col).or_default().push((i, blk.clone_ref(py)));
        }

        let mut table_indexes = Vec::with_capacity(dict_table.len());
        let mut table = Vec::with_capacity(dict_table.len());
        for cols_map in dict_table.values() {
            let mut i_cols = Vec::with_capacity(col_max as usize + 1);
            let mut cols = Vec::with_capacity(col_max as usize + 1);
            for col in 0..=col_max {
                match cols_map.get(&col) {
                    Some(entries) => {
                        let idxs = entries.iter().map(|(i, _)| *i).collect();
                        let blks = entries.iter().map(|(_, b)| b.clone_ref(py)).collect();
                        i_cols.push(idxs);
                        cols.push(blks);
                    }
                    None => {
                        i_cols.push(Vec::new());
                        cols.push(Vec::new());
                    }
                }
            }
            table_indexes.push(i_cols);
            table.push(cols);
        }

        Ok(Self { blks, table_indexes, table })
    }

    fn len(&self) -> usize {
        self.blks.len()
    }

    fn n_cols(&self) -> usize {
        self.table.iter().map(|r| r.len()).max().unwrap_or(0)
    }

    /// Python's `self._blks[i]`, including negative-index wraparound (`[-1]` = last).
    fn get_flat(&self, py: Python<'_>, i: i64) -> Option<Py<PyAny>> {
        let len = self.blks.len() as i64;
        let idx = if i < 0 { i + len } else { i };
        (0..len).contains(&idx).then(|| self.blks[idx as usize].clone_ref(py))
    }

    /// Python's `self._table[row][col]`.
    fn get_cell(&self, py: Python<'_>, row: i64, col: i64) -> Cell {
        let rows = self.table.len() as i64;
        let r = if row < 0 { row + rows } else { row };
        if !(0..rows).contains(&r) {
            return Cell::Empty;
        }
        let row_vec = &self.table[r as usize];
        let cols = row_vec.len() as i64;
        let c = if col < 0 { col + cols } else { col };
        if !(0..cols).contains(&c) {
            return Cell::Empty;
        }
        let vals = &row_vec[c as usize];
        match vals.len() {
            0 => Cell::Empty,
            1 => Cell::One(vals[0].clone_ref(py)),
            _ => Cell::Many(vals.iter().map(|v| v.clone_ref(py)).collect()),
        }
    }

    fn pop(&mut self, py: Python<'_>, j: usize) -> PyResult<()> {
        let blk = self.blks.remove(j);
        let metadata = blk.bind(py).getattr("metadata")?;
        let row_del: i64 = metadata.get_item("table-row")?.extract()?;
        let col_del: i64 = metadata.get_item("table-col")?.extract()?;

        let row_idx = row_del as usize;
        let col_idx = col_del as usize;
        if let Some(jdx) = self.table_indexes[row_idx][col_idx].iter().position(|&idx| idx == j) {
            self.table_indexes[row_idx][col_idx].remove(jdx);
            self.table[row_idx][col_idx].remove(jdx);
            for row in &mut self.table_indexes {
                for col in row.iter_mut() {
                    for idx in col.iter_mut() {
                        if *idx > j {
                            *idx -= 1;
                        }
                    }
                }
            }
        }

        if self.table_indexes[row_idx].iter().all(|c| c.is_empty()) {
            self.table_indexes.remove(row_idx);
            self.table.remove(row_idx);
            for blk in &self.blks {
                let metadata = blk.bind(py).getattr("metadata")?;
                let row: i64 = metadata.get_item("table-row")?.extract()?;
                if row > row_del {
                    metadata.set_item("table-row", row - 1)?;
                }
            }
        }
        Ok(())
    }

    fn merge(&mut self, py: Python<'_>, j: usize, i: usize) -> PyResult<()> {
        let (first, last) = if i < j { (i, j) } else { (j, i) };
        let first_content: Py<PyAny> = self.blks[first].bind(py).getattr("content")?.unbind();
        let last_content: Py<PyAny> = self.blks[last].bind(py).getattr("content")?.unbind();
        let combined = first_content.bind(py).call_method1("__add__", (last_content,))?;
        self.blks[i].bind(py).setattr("content", &combined)?;
        self.pop(py, j)?;
        Ok(())
    }
}

#[pyclass(module = "freeports._native")]
pub struct TextFilterInvestmentsStandard {
    #[pyo3(get, set)]
    market_value_pos: i64,
    #[pyo3(get, set)]
    nominal_quantity_pos: Option<i64>,
    #[pyo3(get, set)]
    perc_net_assets_pos: Option<i64>,
    #[pyo3(get, set)]
    acquisition_currency_pos: Option<i64>,
    #[pyo3(get, set)]
    acquisition_cost_pos: Option<i64>,
    #[pyo3(get, set)]
    geometrical_indexes: bool,
    #[pyo3(get, set)]
    merge_prev: bool,
}

#[pymethods]
impl TextFilterInvestmentsStandard {
    #[new]
    #[pyo3(signature = (
        market_value_pos,
        nominal_quantity_pos=None,
        perc_net_assets_pos=None,
        acquisition_currency_pos=None,
        acquisition_cost_pos=None,
        geometrical_indexes=true,
        merge_prev=false,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        market_value_pos: i64,
        nominal_quantity_pos: Option<i64>,
        perc_net_assets_pos: Option<i64>,
        acquisition_currency_pos: Option<i64>,
        acquisition_cost_pos: Option<i64>,
        geometrical_indexes: bool,
        merge_prev: bool,
    ) -> PyResult<Self> {
        if let (Some(nq), Some(pna)) = (nominal_quantity_pos, perc_net_assets_pos)
            && (nq == market_value_pos || nq == pna || market_value_pos == pna) {
                return Err(PyValueError::new_err("All positions should be different"));
            }
        Ok(Self {
            market_value_pos,
            nominal_quantity_pos,
            perc_net_assets_pos,
            acquisition_currency_pos,
            acquisition_cost_pos,
            geometrical_indexes,
            merge_prev,
        })
    }

    fn __call__(&self, py: Python<'_>, pdf_blks: &Bound<'_, PyAny>, filter_data: Py<PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let mut fund_found: Option<Py<PyAny>> = None;
        let mut currency_found: Option<Currency> = None;
        let mut results: Vec<Py<PyAny>> = Vec::new();
        let mut investments_blks: Vec<Py<PyAny>> = Vec::new();

        for item in pdf_blks.try_iter()? {
            let item = item?;
            let type_block: String = item.getattr("type_block")?.extract()?;
            if type_block == "FUND_NAME" {
                if fund_found.is_some() {
                    return Err(PyValueError::new_err("Fund two subfunds in same page"));
                }
                fund_found = Some(item.getattr("content")?.unbind());
                let factory = standard_txt_blks_module(py)?.getattr("StandardFundTextBlock")?;
                results.push(factory.call1((&item,))?.unbind());
            } else if type_block == "CURRENCY_STATEMENT" {
                if currency_found.is_some() {
                    return Err(PyValueError::new_err("Fund two currency in same page"));
                }
                let content: String = item.getattr("content")?.extract()?;
                currency_found = Some(py_extract_currency_from_text(&content).map_err(|e| PageParseFail::new_err(e.to_string()))?);
            } else {
                investments_blks.push(item.unbind());
            }
        }

        let inv = self.run_loop(py, &investments_blks, filter_data.bind(py))?;
        if inv.is_empty() {
            return Ok(vec![]);
        }
        for txt_blk in &inv {
            let metadata = txt_blk.getattr("metadata")?;
            metadata.set_item("fund", &fund_found)?;
            metadata.set_item("currency", currency_found)?;
        }
        results.extend(inv.into_iter().map(|b| b.unbind().into_any()));
        Ok(results)
    }
}

impl TextFilterInvestmentsStandard {
    /// Inlined `standard_text_filterion_loop` applied to this class's own field-extraction body
    /// (the only real use of that decorator — see module doc).
    fn run_loop<'py>(&self, py: Python<'py>, pdf_blocks: &[Py<PyAny>], targets: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, TextBlock>>> {
        let mut text_part_list = Vec::new();
        if pdf_blocks.is_empty() {
            return Ok(text_part_list);
        }
        let mut table = PdfBlocksTable::new(py, pdf_blocks)?;
        let n_cols = table.n_cols() as i64;

        let mut i: i64 = 0;
        let mut col: i64 = 0;
        // `match_company` used to live in the separate `freeports_lib` crate, reached only via
        // `py.import("freeports_lib")`; merged into this crate in Fase E (see
        // `agent-memory/rust-native-binary-plan.md`). Fase E's final simplification pass replaced
        // the (still-working, but wasteful) `py.import("freeports._native")` round-trip with a
        // direct native call — this loop runs once per table row, so re-importing the module and
        // re-extracting `target_companies` from Python on every row was real, avoidable overhead,
        // not just an aesthetic round-trip. `target_companies` is extracted once, here, instead.
        let target_companies: Vec<crate::formats_utils::text_filter::matcher::CompanyMatchInfos> = targets.extract()?;
        // `LOG_ADAPT_INVESTMENT_INFOS`/`text_filter_logger` (Python `logging` module) removed
        // from this loop per the Python-elimination plan (`agent-memory/
        // python-circumscription-plan.md`, Fase 2) — `core/logging.py` isn't ported yet, and this
        // is deliberately commented out rather than ported now. Known consequence, not a silent
        // regression: the `.log.csv` fixture rows this used to produce (via a `logging.FileHandler`
        // reading these `row`/`company`/`company_match` attributes) will be missing until logging
        // is ported.

        while i < table.len() as i64 - 1 {
            let mut split = false;
            let current = table.get_flat(py, i).expect("i is always in range inside the loop");
            let next = table.get_flat(py, i + 1).expect("i+1 is always in range inside the loop");
            let current_metadata = current.bind(py).getattr("metadata")?;
            col = current_metadata.get_item("table-col")?.extract()?;
            let row: i64 = current_metadata.get_item("table-row")?.extract()?;
            let next_metadata = next.bind(py).getattr("metadata")?;
            let next_col: i64 = next_metadata.get_item("table-col")?.extract()?;
            let next_row: i64 = next_metadata.get_item("table-row")?.extract()?;
            let cell_width: bool = current_metadata.get_item("is-max-width")?.extract()?;
            let mut content: String = current.bind(py).getattr("content")?.extract()?;

            if col == next_col {
                let mut n_full_cols = 0;
                let mut empty_adj = 0;
                for c in 0..n_cols {
                    let occupied = !matches!(table.get_cell(py, if self.merge_prev { row } else { next_row }, c), Cell::Empty);
                    if occupied {
                        n_full_cols += 1;
                    } else if c == col - 1 || c == col + 1 {
                        empty_adj += 1;
                    }
                }
                if n_full_cols == 1 || empty_adj == 2 {
                    split = true;
                    if cell_width || content.ends_with(' ') || content.ends_with('\n') {
                        let next_content: String = next.bind(py).getattr("content")?.extract()?;
                        content.push_str(&next_content);
                    }
                }
            }

            let company = crate::formats_utils::text_filter::matcher::match_company_or_pyerr(py, &content, &target_companies)?;
            if let Some(company) = company {
                if split {
                    let (i_usize, i1_usize) = (i as usize, (i + 1) as usize);
                    if self.merge_prev {
                        table.merge(py, i_usize, i1_usize)?;
                    } else {
                        table.merge(py, i1_usize, i_usize)?;
                    }
                }
                match self.extract_field(py, &table, i, (row, col), n_cols) {
                    Ok(txt_blk) => {
                        let metadata = txt_blk.getattr("metadata")?;
                        metadata.set_item("company match", &content)?;
                        metadata.set_item("company", &company)?;
                        text_part_list.push(txt_blk);
                    }
                    Err(err) if err.is_instance_of::<ExpectedTextBlockNotFound>(py) => {
                        // logging removed here too, same as above
                    }
                    Err(err) => return Err(err),
                }
            }
            i += 1;
            if i >= table.len() as i64 - 1 {
                break;
            }
        }

        if i == table.len() as i64 - 1 {
            let last = table.get_flat(py, -1).expect("table is non-empty");
            let last_metadata = last.bind(py).getattr("metadata")?;
            let row: i64 = last_metadata.get_item("table-row")?.extract()?;
            let content: String = last.bind(py).getattr("content")?.extract()?;
            let company = crate::formats_utils::text_filter::matcher::match_company_or_pyerr(py, &content, &target_companies)?;
            if let Some(company) = company {
                match self.extract_field(py, &table, i, (row, col), n_cols) {
                    Ok(txt_blk) => {
                        let metadata = txt_blk.getattr("metadata")?;
                        metadata.set_item("company match", &content)?;
                        metadata.set_item("company", &company)?;
                        text_part_list.push(txt_blk);
                    }
                    Err(err) if err.is_instance_of::<ExpectedTextBlockNotFound>(py) => {
                        // logging removed here too, same as above
                    }
                    Err(err) => return Err(err),
                }
            }
        }
        Ok(text_part_list)
    }

    /// The Python original's `text_filter` inner function (the body `standard_text_filterion_loop`
    /// decorates) plus its two nested helpers (`abs_idx`, `try_extraction_of_field`), inlined.
    ///
    /// `abs_idx` itself collapses to just the two branches that are ever actually reachable: every
    /// position field (`market_value_pos` etc.) is a plain `int`, never a tuple, so the Python
    /// original's `isinstance(offset, tuple)` branch inside `abs_idx` is dead — the real behavior
    /// in geometrical mode is the *modular-wraparound* branch (`offset` is a linear distance from
    /// the current column that wraps into the next row when it overflows the table width), not a
    /// plain `(row, col)` tuple add. Verified against the source directly after an initial wrong
    /// guess here treated it as tuple-addition.
    fn extract_field<'py>(&self, py: Python<'py>, table: &PdfBlocksTable, i: i64, base: (i64, i64), n_cols: i64) -> PyResult<Bound<'py, TextBlock>> {
        let cell_content = |row: i64, col: i64| -> Option<String> {
            match table.get_cell(py, row, col) {
                Cell::One(b) => b.bind(py).getattr("content").ok()?.extract().ok(),
                _ => None,
            }
        };
        let flat_content = |idx: i64| -> Option<String> { table.get_flat(py, idx)?.bind(py).getattr("content").ok()?.extract().ok() };
        // Returns the resolved `(row, col)` alongside the content, matching the Python
        // original's own `abs_idx(pos)` result — needed for the "field not found" error's
        // `extra={"row":..., "col":...}`, not just the content itself.
        let resolve = |offset: i64| -> (Option<(i64, i64)>, Option<String>) {
            if self.geometrical_indexes {
                let (r, c) = base;
                let co = (c + offset).rem_euclid(n_cols) - c;
                let ro = (c + offset).div_euclid(n_cols);
                (Some((r + ro, c + co)), cell_content(r + ro, c + co))
            } else {
                (None, flat_content(i + offset))
            }
        };

        let metadata = PyDict::new(py);

        let anchor = if self.geometrical_indexes { table.get_cell(py, base.0, base.1) } else { table.get_flat(py, i).map(Cell::One).unwrap_or(Cell::Empty) };
        let manco = match &anchor {
            Cell::One(b) => b.bind(py).getattr("metadata")?.call_method1("get", ("manco",))?.unbind(),
            _ => {
                // logging removed, see this function's callers' own note
                return Err(ExpectedTextBlockNotFound::new_err(translate(py, "Matching text block not found")?));
            }
        };
        metadata.set_item("manco", manco)?;

        let (_, market_value) = resolve(self.market_value_pos);
        let market_value = match market_value {
            Some(v) => v,
            None => {
                // logging removed, see this function's callers' own note
                return Err(ExpectedTextBlockNotFound::new_err(()));
            }
        };
        metadata.set_item("market value", &market_value)?;

        for (pos, name) in [
            (self.perc_net_assets_pos, "% net assets"),
            (self.nominal_quantity_pos, "quantity"),
            (self.acquisition_currency_pos, "acquisition currency"),
            (self.acquisition_cost_pos, "acquisition cost"),
        ] {
            if let Some(pos) = pos {
                let (_resolved, value) = resolve(pos);
                // logging removed here too (used to log `row`/`col`/`field` via `resolved` when
                // `value.is_none()`), see this function's callers' own note
                metadata.set_item(name, value)?;
            }
        }

        let raw_content = match &anchor {
            Cell::One(b) => b.bind(py).getattr("content")?.extract::<String>()?,
            _ => unreachable!("anchor was already checked to be Cell::One above"),
        };
        let content = raw_content.replace('\n', "");
        let mut instrument = "EQUITY_TARGET".to_string();
        for pattern in PERC_REGEXES.iter() {
            if let Some(caps) = pattern.captures(&content)
                && let Some(m) = caps.at(1) {
                    instrument = "BOND_TARGET".to_string();
                    metadata.set_item("interest rate", m)?;
                    break;
                }
        }
        for pattern in DATE_REGEXES.iter() {
            if let Some(caps) = pattern.captures(&content)
                && let Some(m) = caps.at(1) {
                    instrument = "BOND_TARGET".to_string();
                    metadata.set_item("maturity", m)?;
                    break;
                }
        }

        let anchor_pdf_block: Py<PdfBlock> = match &anchor {
            Cell::One(b) => b.extract(py)?,
            _ => unreachable!(),
        };
        TextBlock::new(py, instrument, metadata.unbind(), anchor_pdf_block).into_pyobject(py)
    }
}

// `\A` prefix on every pattern: the Python original uses `re.match(reg, content)`, which only
// ever matches starting at position 0 of the string — not `re.search`'s "match anywhere".
// `onig::Regex::captures` searches anywhere by default (no automatic `re.match`-style
// anchoring), so without `\A` these would accept a match starting mid-string. Verified this
// mattered for real: `PERC_REGEXES[0]` requires a leading `[a-zA-Z]`, so on content like
// `"1,300,000.00 ITALY BTPS 3.4% ..."` (starts with a digit) `re.match` correctly never matches
// it at all, while an unanchored onig search happily matched starting at "ITALY" and produced a
// spurious `interest rate` field the real fixtures don't have (caught via ANIMA_SICAV-EN24's
// `test_text_filter[23]`/KAIROS-EN23's `test_text_filter[30]`).
static PERC_REGEXES: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"\A[a-zA-Z].*((\d+[.,]\d+)\s*%).*").unwrap(),
        Regex::new(r"\A[a-zA-Z].*((\d+[.,]\d+)\s*).*").unwrap(),
    ]
});

static DATE_REGEXES: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"\A.*(\d{2}[/\-.]\d{2}[/\-.]\d{4}).*").unwrap(),
        Regex::new(r"\A.*(\d{4}[/\-.]\d{2}[/\-.]\d{2}).*").unwrap(),
        Regex::new(r"\A.*(\d{2}[/\-.]\d{2}[/\-.]\d{2}).*").unwrap(),
        Regex::new(r"\A.*\s(\d{2}[/\-]\d{2})\s.*").unwrap(),
    ]
});

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyList;

    fn pdf_block<'py>(py: Python<'py>, type_block: &str, metadata: &Bound<'py, PyDict>, content: &str) -> Bound<'py, PdfBlock> {
        let content = content.into_pyobject(py).unwrap().into_any().unbind();
        Py::new(py, PdfBlock::new(type_block.into(), metadata.clone().unbind(), content)).unwrap().into_bound(py)
    }

    fn table_row<'py>(py: Python<'py>, row: i64, col: i64, text: &str, is_max_width: bool) -> Bound<'py, PdfBlock> {
        let metadata = PyDict::new(py);
        metadata.set_item("table-row", row).unwrap();
        metadata.set_item("table-col", col).unwrap();
        metadata.set_item("is-max-width", is_max_width).unwrap();
        pdf_block(py, "TABLE_BODY", &metadata, text)
    }

    /// Builds real `CompanyMatchInfos` (what `filter_data` actually is for
    /// `TextFilterInvestmentsStandard` in production — verified via real format fixtures, not a
    /// stand-in) via its only constructor, `compile_from_pandas_df`, using a tiny pandas
    /// DataFrame built through Python. `CompanyMatchInfos` used to live in the separate
    /// `freeports_lib` crate; merged into this crate in Fase E (see
    /// `agent-memory/rust-native-binary-plan.md`).
    fn company_match_infos<'py>(py: Python<'py>, companies: &[(&str, &str)]) -> Bound<'py, PyAny> {
        let pandas = py.import("pandas").unwrap();
        let data = PyDict::new(py);
        for (name, regex) in companies {
            let row = PyDict::new(py);
            row.set_item("Regexs", vec![*regex]).unwrap();
            row.set_item("Symbols", Vec::<String>::new()).unwrap();
            row.set_item("Buds", Vec::<String>::new()).unwrap();
            data.set_item(name, row).unwrap();
        }
        let kwargs = PyDict::new(py);
        kwargs.set_item("orient", "index").unwrap();
        let df = pandas.getattr("DataFrame").unwrap().getattr("from_dict").unwrap().call((data,), Some(&kwargs)).unwrap();
        // Native call, same crate — no `py.import` needed (Fase E's final simplification pass).
        let infos = crate::formats_utils::text_filter::matcher::CompanyMatchInfos::compile_from_pandas_df(py, df).unwrap();
        PyList::new(py, infos).unwrap().into_any()
    }

    #[test]
    fn filter_block_type_filters_by_type_and_forwards_filter_data() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let metadata = PyDict::new(py);
            let a = pdf_block(py, "A", &metadata, "");
            let b = pdf_block(py, "B", &metadata, "");
            let blks = PyList::new(py, [a, b]).unwrap();

            let f = py
                .eval(std::ffi::CString::new("lambda filtered, data: (len(filtered), filtered[0].type_block, data)").unwrap().as_c_str(), None, None)
                .unwrap();
            let stage1 = FilterBlockType::new("A".to_string());
            let stage2 = stage1.__call__(f.unbind());
            let result = stage2.__call__(py, blks.as_any(), "tag".into_pyobject(py).unwrap().into_any().unbind()).unwrap();
            let (n, type_block, tag): (usize, String, String) = result.extract(py).unwrap();
            assert_eq!((n, type_block.as_str(), tag.as_str()), (1, "A", "tag"));
        });
    }

    #[test]
    fn filter_block_type_call_binds_self_through_descriptor_protocol() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let f = py
                .eval(std::ffi::CString::new("lambda self_arg, filtered, data: (self_arg, len(filtered))").unwrap().as_c_str(), None, None)
                .unwrap();
            let stage1 = FilterBlockTypeCall::new("A".to_string());
            let applied = Py::new(py, stage1.__call__(f.unbind())).unwrap();
            let instance = "some-instance".into_pyobject(py).unwrap();
            let bound_method = FilterBlockTypeCallApplied::__get__(applied.bind(py).clone(), py, Some(instance.as_any().clone()), None).unwrap();

            let metadata = PyDict::new(py);
            let a = pdf_block(py, "A", &metadata, "");
            let blks = PyList::new(py, [a]).unwrap();
            let result = bound_method.bind(py).call1((blks, py.None())).unwrap();
            let (self_arg, n): (String, usize) = result.extract().unwrap();
            assert_eq!((self_arg.as_str(), n), ("some-instance", 1));
        });
    }

    #[test]
    fn extract_currency_from_text_prefers_first_mentioned_currency() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            assert_eq!(py_extract_currency_from_text("Converted from USD to EUR").unwrap(), Currency::USD);
            assert_eq!(py_extract_currency_from_text("Converted from EUR to USD").unwrap(), Currency::EUR);
        });
    }

    #[test]
    fn extract_currency_from_text_errors_when_no_currency_present() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let err = py_extract_currency_from_text("no currency here").unwrap_err();
            assert!(err.is_instance_of::<ExpectedTextBlockNotFound>(py));
        });
    }

    #[test]
    fn sfdr_article_strips_prefix_and_requires_investment_fund_match() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let kwargs = PyDict::new(py);
            kwargs.set_item("company", "C").unwrap();
            kwargs.set_item("company_match", "C").unwrap();
            kwargs.set_item("fund", "Acme Fund").unwrap();
            kwargs.set_item("market_value", 1.0).unwrap();
            kwargs.set_item("currency", Currency::EUR).unwrap();
            let equity = py.get_type::<crate::output::investment::Equity>().call((), Some(&kwargs)).unwrap().unbind();

            let metadata = PyDict::new(py);
            let blk = pdf_block(py, "SFDR_ARTICLE", &metadata, "Prefix: Acme Fund").unbind();
            let blks = PyList::new(py, [blk]).unwrap();

            let sfdr = TextFilterSfdrArticleStandard::new(py, Some(&"Prefix: ".into_pyobject(py).unwrap().into_any()), true).unwrap();
            let filter_data = PyList::new(py, [equity]).unwrap();
            let result = sfdr.__call__(py, blks.as_any(), filter_data.as_any()).unwrap();
            assert_eq!(result.len(), 1);
            let content: String = result[0].bind(py).getattr("content").unwrap().extract().unwrap();
            assert_eq!(content, "Acme Fund");
        });
    }

    #[test]
    fn sfdr_article_returns_empty_when_fund_not_in_investment_funds() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let metadata = PyDict::new(py);
            let blk = pdf_block(py, "SFDR_ARTICLE", &metadata, "Acme Fund").unbind();
            let blks = PyList::new(py, [blk]).unwrap();

            let sfdr = TextFilterSfdrArticleStandard::new(py, None, true).unwrap();
            let filter_data = PyList::empty(py);
            let result = sfdr.__call__(py, blks.as_any(), filter_data.as_any()).unwrap();
            assert!(result.is_empty());
        });
    }

    #[test]
    fn page_classify_consolidates_page_type_and_errors_on_conflict() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let m1 = PyDict::new(py);
            m1.set_item("page_type", "investments").unwrap();
            let m2 = PyDict::new(py);
            m2.set_item("page_type", py.None()).unwrap();
            let blk1 = pdf_block(py, "PAGE_CLASS", &m1, "").unbind();
            let blk2 = pdf_block(py, "PAGE_CLASS", &m2, "").unbind();
            let blks = PyList::new(py, [blk1.clone_ref(py), blk2]).unwrap();

            let pc = TextFilterPageClassifyStandard::new();
            let result = pc.__call__(py, blks.as_any(), py.None()).unwrap();
            let metadata = result[0].bind(py).getattr("metadata").unwrap();
            let page_type: Option<String> = metadata.get_item("page_type").unwrap().extract().unwrap();
            assert_eq!(page_type, Some("investments".to_string()));

            let m3 = PyDict::new(py);
            m3.set_item("page_type", "other").unwrap();
            let blk3 = pdf_block(py, "PAGE_CLASS", &m3, "").unbind();
            let blks2 = PyList::new(py, [blk1, blk3]).unwrap();
            let err = pc.__call__(py, blks2.as_any(), py.None()).unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py));
        });
    }

    // No direct unit test for the matched-funds happy path here: it requires round-tripping a
    // `PdfBlock` through `StandardManagmentCompanyTextBlock` (real Python code importing
    // `freeports._native.core.TextBlock`), and `cargo test`'s embedded interpreter has its *own*
    // separately-compiled copy of this crate's pyclasses distinct from the installed extension
    // module — so a `PdfBlock` built directly via `crate::core::classes::PdfBlock::new` fails a
    // strict PyO3 type-identity check when handed to that real, installed-module code
    // ("'PdfBlock' object cannot be cast as 'PdfBlock'", two distinct compiled types with the
    // same name). This exact path (matched funds, real installed module) is already verified
    // manually and via 7 real format fixtures in the full suite — see
    // `agent-memory/rust-rewrite-plan.md`.

    #[test]
    fn managment_company_raises_when_no_management_company_block() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let blks = PyList::empty(py);
            let mc = TextFilterManagmentCompanyStandard::new();
            let filter_data = PyList::empty(py);
            let err = mc.__call__(py, blks.as_any(), filter_data.as_any()).unwrap_err();
            assert!(err.is_instance_of::<PyStopIteration>(py));
        });
    }

    #[test]
    fn assets_standard_filters_by_fund_and_extracts_currency() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let name = "Acme Fund".into_pyobject(py).unwrap().into_any();
            let fund = crate::output::fund::Fund::new(&name).unwrap();
            let fund = Py::new(py, fund).unwrap();

            let metadata = PyDict::new(py);
            metadata.set_item("fund", "Prefix Acme Fund").unwrap();
            metadata.set_item("currency", "Reported in EUR").unwrap();
            metadata.set_item("tot_assets", "1000000").unwrap();
            metadata.set_item("liabilities", "200000").unwrap();
            metadata.set_item("net_assets", "800000").unwrap();
            metadata.set_item("date", "As of 31/12/2024").unwrap();
            let blk = pdf_block(py, "RELEVANT_BLOCK", &metadata, "").unbind();
            let blks = PyList::new(py, [blk]).unwrap();

            let tfa = TextFilterAssetsStandard::new(Some(r"(\d{2}/\d{2}/\d{4})"), Some(&"^Prefix ".into_pyobject(py).unwrap().into_any())).unwrap();
            let filter_data = PyList::new(py, [fund]).unwrap();
            let result = tfa.__call__(py, blks.as_any(), filter_data.as_any()).unwrap();
            assert_eq!(result.len(), 1);
            let metadata = result[0].bind(py).getattr("metadata").unwrap();
            let fund_name: String = metadata.get_item("fund").unwrap().extract().unwrap();
            let date: String = metadata.get_item("date").unwrap().extract().unwrap();
            let currency: Currency = metadata.get_item("currency").unwrap().extract().unwrap();
            assert_eq!(fund_name, "Acme Fund");
            assert_eq!(date, "31/12/2024");
            assert_eq!(currency, Currency::EUR);
        });
    }

    #[test]
    fn investments_standard_extracts_bond_target_without_spurious_interest_rate() {
        // Regression test for the `re.match` vs unanchored-onig-search bug: content starting
        // with a digit must NOT match the `[a-zA-Z]`-anchored percentage pattern, even though an
        // unanchored search would find "3.4%" further into the string (after "ITALY").
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let row0 = table_row(py, 0, 0, "1,300,000.00 ITALY BTPS 3.4% 23-28/03/2025", true).unbind();
            let blks = PyList::new(py, [row0]).unwrap();

            let inv = TextFilterInvestmentsStandard::new(0, None, None, None, None, true, false).unwrap();
            let targets = company_match_infos(py, &[("Italy btps", "italy btps")]);
            let result = inv.run_loop(py, &[blks.get_item(0).unwrap().unbind()], targets.as_any()).unwrap();

            assert_eq!(result.len(), 1);
            let metadata = result[0].getattr("metadata").unwrap();
            let instrument: String = result[0].getattr("type_block").unwrap().extract().unwrap();
            assert_eq!(instrument, "BOND_TARGET");
            assert!(metadata.get_item("interest rate").is_err(), "spurious 'interest rate' key should not be present");
            let maturity: String = metadata.get_item("maturity").unwrap().extract().unwrap();
            assert_eq!(maturity, "28/03/2025");
        });
    }

    #[test]
    fn investments_standard_returns_empty_for_no_rows() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let inv = TextFilterInvestmentsStandard::new(0, None, None, None, None, true, false).unwrap();
            let targets = company_match_infos(py, &[("Italy btps", "italy btps")]);
            let blks = PyList::empty(py);
            let result = inv.__call__(py, blks.as_any(), targets.unbind()).unwrap();
            assert!(result.is_empty());
        });
    }

    // `investments_standard_logs_skipping_line_via_python_logging_on_extraction_failure` (the
    // `.log.csv`-fixture regression test for the real Python `logging` calls this loop used to
    // make on a failed extraction) no longer applies: those calls are commented out per the
    // Python-elimination plan (`agent-memory/python-circumscription-plan.md`, Fase 2) —
    // `core/logging.py` isn't ported yet, so there's nothing left to log through. Known,
    // deliberate consequence: `.log.csv` fixture rows built from these calls will be missing
    // until logging is ported and this test (or an equivalent) is restored.
}
