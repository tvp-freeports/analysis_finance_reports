//! Shim di `freeports.standard_funcs.text_filter`: i cinque pipe standard del secondo segmento.

use std::sync::Arc;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;

use crate::core::classes::BlockType;
use crate::formats_utils::text_filter::dash_as_zero::DashAsZero;
use crate::formats_utils::text_filter::standard_funcs::{
    StandardFuncsError, TextFilterAssetsStandard, TextFilterDocClassifyStandard, TextFilterInvestmentsStandard,
    TextFilterManagmentCompanyStandard, TextFilterPageClassifyStandard, TextFilterSfdrSignStandard,
};

use crate::python::pipes::PyTextFilterPipe;
use crate::core::tracing_setup::log_error;

/// The compiled-pattern type, for telling a compiled regex from a literal string.
fn re_pattern_type(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("re")?.getattr("Pattern")
}

/// The source of a regex compiled by Python.
fn pattern_source(object: &Bound<'_, PyAny>) -> PyResult<String> {
    object.getattr("pattern")?.extract()
}

/// The elements of an argument accepted either as a scalar or as an iterable.
///
/// A string **is** iterable in Python, so the scalar case must be recognised first: without that, a
/// single prefix would become a list of one-character prefixes.
fn scalar_or_iterable<'py>(object: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let py = object.py();
    if object.is_instance_of::<PyString>() || object.is_instance(&re_pattern_type(py)?)? {
        return Ok(vec![object.clone()]);
    }
    object.try_iter()?.collect()
}

/// `TextFilterPageClassifyStandard()`.
#[pyfunction]
#[pyo3(name = "TextFilterPageClassifyStandard")]
pub fn py_text_filter_page_classify_standard() -> PyTextFilterPipe {
    PyTextFilterPipe::new(Arc::new(TextFilterPageClassifyStandard))
}

/// `TextFilterDocClassifyStandard()`.
#[pyfunction]
#[pyo3(name = "TextFilterDocClassifyStandard")]
pub fn py_text_filter_doc_classify_standard() -> PyTextFilterPipe {
    PyTextFilterPipe::new(Arc::new(TextFilterDocClassifyStandard))
}

/// `TextFilterManagmentCompanyStandard()`.
#[pyfunction]
#[pyo3(name = "TextFilterManagmentCompanyStandard")]
pub fn py_text_filter_managment_company_standard() -> PyTextFilterPipe {
    PyTextFilterPipe::new(Arc::new(TextFilterManagmentCompanyStandard))
}

/// The pipe for the rows of an investments table.
///
/// `dash_as_zero` is the same flag expression the structured level writes in its
/// `Interpret dash as zero` column — `"MARKET_VALUE | PERC_NET_ASSETS"`, `"ALL"` — and `None` is
/// the empty set, so an unstructured format that says nothing behaves as it always has.
///
/// `remove_from_fund_regexes` is the `Remove from fund` column: patterns cut out of the fund's
/// name before it becomes the sub-fund's identity — the `(continued)` of a continuation page, or a
/// footnote mark at its end (`R16`), `r"\s*[*]*\s*$"`.
/// It accepts one pattern or an iterable of them, as a string or an already-compiled `re.Pattern`
/// — from a compiled one only its source is taken, since the compilation happens inside the
/// native type.
#[pyfunction]
#[pyo3(name = "TextFilterInvestmentsStandard")]
#[pyo3(signature = (
    market_value_pos,
    nominal_quantity_pos=None,
    perc_net_assets_pos=None,
    acquisition_currency_pos=None,
    acquisition_cost_pos=None,
    geometrical_indexes=true,
    dash_as_zero=None,
    remove_from_fund_regexes=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_text_filter_investments_standard(
    market_value_pos: i64,
    nominal_quantity_pos: Option<i64>,
    perc_net_assets_pos: Option<i64>,
    acquisition_currency_pos: Option<i64>,
    acquisition_cost_pos: Option<i64>,
    geometrical_indexes: bool,
    dash_as_zero: Option<&str>,
    remove_from_fund_regexes: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyTextFilterPipe> {
    let dash_as_zero = match dash_as_zero {
        None => DashAsZero::empty(),
        Some(expression) => DashAsZero::from_expression(expression).map_err(|e| {
            tracing::error!(error = log_error(&e), "invalid dash_as_zero expression {expression:?}: {e}");
            PyValueError::new_err(e.to_string())
        })?,
    };
    let pipe = TextFilterInvestmentsStandard::new(
        market_value_pos,
        nominal_quantity_pos,
        perc_net_assets_pos,
        acquisition_currency_pos,
        acquisition_cost_pos,
        geometrical_indexes,
        dash_as_zero,
    )
    .map_err(|e| {
        tracing::error!(error = log_error(&e), "TextFilterInvestmentsStandard construction failed: {e}");
        PyValueError::new_err(e.to_string())
    })?;
    let patterns = match remove_from_fund_regexes.filter(|object| !object.is_none()) {
        None => Vec::new(),
        Some(object) => scalar_or_iterable(object)?
            .iter()
            .map(|item| {
                if item.is_instance_of::<PyString>() { item.extract() } else { pattern_source(item) }
            })
            .collect::<PyResult<Vec<String>>>()?,
    };
    let pipe = pipe.with_remove_from_fund_regexes(&patterns).map_err(|e| {
        tracing::error!(error = log_error(&e), "invalid remove_from_fund_regexes: {e}");
        PyValueError::new_err(e.to_string())
    })?;
    Ok(PyTextFilterPipe::new(Arc::new(pipe)))
}

/// `TextFilterSfdrSignStandard(fund_prefix=None, *, fund_block_type=None,
/// demand_investment_funds_match=True)` — the tick becomes an article.
///
/// **A divergence absorbed here:** a single argument mixes literal strings and already-compiled
/// patterns, while the native type wants them separated, the first going through a plain substring
/// replacement and the second through the regex engine. The sorting happens here, and the relative
/// order within each of the two groups is preserved.
///
/// `fund_block_type` names the block the fund's own `pdf_extract` pipe produced, and defaults to
/// `FUND_NAME`, which is what `PdfExtractLabelledValueStandard` is normally told to emit.
#[pyfunction]
#[pyo3(name = "TextFilterSfdrSignStandard")]
#[pyo3(signature = (fund_prefix=None, *, fund_block_type=None, demand_investment_funds_match=true))]
pub fn py_text_filter_sfdr_sign_standard(
    fund_prefix: Option<&Bound<'_, PyAny>>,
    fund_block_type: Option<String>,
    demand_investment_funds_match: bool,
) -> PyResult<PyTextFilterPipe> {
    let mut prefix_strings = Vec::new();
    let mut prefix_patterns = Vec::new();

    if let Some(fund_prefix) = fund_prefix.filter(|object| !object.is_none()) {
        let pattern_type = re_pattern_type(fund_prefix.py())?;
        for item in scalar_or_iterable(fund_prefix)? {
            if item.is_instance_of::<PyString>() {
                prefix_strings.push(item.extract()?);
            } else if item.is_instance(&pattern_type)? {
                prefix_patterns.push(pattern_source(&item)?);
            } else {
                tracing::error!("fund_prefix item is neither a str nor a re.Pattern");
                return Err(PyValueError::new_err("fund_prefix items must be str or re.Pattern"));
            }
        }
    }

    let pipe = TextFilterSfdrSignStandard::new(
        fund_block_type.map(BlockType::from),
        prefix_strings,
        prefix_patterns,
        demand_investment_funds_match,
    )
    .map_err(|e: StandardFuncsError| {
        tracing::error!(error = log_error(&e), "TextFilterSfdrSignStandard construction failed: {e}");
        PyValueError::new_err(e.to_string())
    })?;
    Ok(PyTextFilterPipe::new(Arc::new(pipe)))
}

/// The pipe for a fund's assets.
///
/// Both arguments accept either an already-compiled regex or its source as a string: the real
/// compilation happens inside the native type, so from a compiled pattern only its source is taken.
#[pyfunction]
#[pyo3(name = "TextFilterAssetsStandard")]
#[pyo3(signature = (date_regex=None, remove_from_fund_regexes=None))]
pub fn py_text_filter_assets_standard(
    date_regex: Option<&Bound<'_, PyAny>>,
    remove_from_fund_regexes: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyTextFilterPipe> {
    let source_of = |object: &Bound<'_, PyAny>| -> PyResult<String> {
        if object.is_instance_of::<PyString>() { object.extract() } else { pattern_source(object) }
    };

    let date_regex = match date_regex.filter(|object| !object.is_none()) {
        None => None,
        Some(object) => Some(source_of(object)?),
    };

    let remove_from_fund_regexes = match remove_from_fund_regexes.filter(|object| !object.is_none()) {
        None => Vec::new(),
        Some(object) => scalar_or_iterable(object)?.iter().map(source_of).collect::<PyResult<_>>()?,
    };

    let pipe = TextFilterAssetsStandard::new(date_regex.as_deref(), remove_from_fund_regexes).map_err(|e| {
        tracing::error!(error = log_error(&e), "TextFilterAssetsStandard construction failed: {e}");
        PyValueError::new_err(e.to_string())
    })?;
    Ok(PyTextFilterPipe::new(Arc::new(pipe)))
}
