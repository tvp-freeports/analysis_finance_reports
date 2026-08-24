//! Shim di `freeports.standard_funcs.text_filter`: i cinque pipe standard del secondo segmento.

use std::sync::Arc;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;

use crate::formats_utils::text_filter::standard_funcs::{
    TextFilterAssetsStandard, TextFilterInvestmentsStandard, TextFilterManagmentCompanyStandard,
    TextFilterPageClassifyStandard, TextFilterSfdrArticleStandard,
};

use crate::python::pipes::PyTextFilterPipe;

/// Il tipo `re.Pattern`, per distinguere una regex compilata da una stringa letterale.
fn re_pattern_type(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    py.import("re")?.getattr("Pattern")
}

/// Il sorgente di una regex compilata da Python (`pattern.pattern`).
fn pattern_source(object: &Bound<'_, PyAny>) -> PyResult<String> {
    object.getattr("pattern")?.extract()
}

/// Gli elementi di un argomento che il riferimento accetta come scalare o come iterabile.
///
/// Una stringa **è** iterabile in Python, quindi il caso scalare va riconosciuto prima: senza
/// questo, `TextFilterSfdrArticleStandard("Nome del prodotto: ")` diventerebbe una lista di
/// diciannove prefissi di un carattere.
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

/// `TextFilterManagmentCompanyStandard()`.
#[pyfunction]
#[pyo3(name = "TextFilterManagmentCompanyStandard")]
pub fn py_text_filter_managment_company_standard() -> PyTextFilterPipe {
    PyTextFilterPipe::new(Arc::new(TextFilterManagmentCompanyStandard))
}

/// `TextFilterInvestmentsStandard(...)` — le righe della tabella degli investimenti.
#[pyfunction]
#[pyo3(name = "TextFilterInvestmentsStandard")]
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
pub fn py_text_filter_investments_standard(
    market_value_pos: i64,
    nominal_quantity_pos: Option<i64>,
    perc_net_assets_pos: Option<i64>,
    acquisition_currency_pos: Option<i64>,
    acquisition_cost_pos: Option<i64>,
    geometrical_indexes: bool,
    merge_prev: bool,
) -> PyResult<PyTextFilterPipe> {
    let pipe = TextFilterInvestmentsStandard::new(
        market_value_pos,
        nominal_quantity_pos,
        perc_net_assets_pos,
        acquisition_currency_pos,
        acquisition_cost_pos,
        geometrical_indexes,
        merge_prev,
    )
    .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyTextFilterPipe::new(Arc::new(pipe)))
}

/// `TextFilterSfdrArticleStandard(fund_prefix=None, demand_investment_funds_match=True)`.
///
/// **Divergenza assorbita qui:** il riferimento ha un solo argomento `fund_prefix` che mescola
/// stringhe letterali e `re.Pattern` già compilati; il tipo nativo li vuole separati, perché i
/// primi passano da `str::replace` e i secondi da Oniguruma. Lo smistamento avviene qui, e
/// l'ordine relativo all'interno di ciascuno dei due gruppi è conservato.
#[pyfunction]
#[pyo3(name = "TextFilterSfdrArticleStandard")]
#[pyo3(signature = (fund_prefix=None, demand_investment_funds_match=true))]
pub fn py_text_filter_sfdr_article_standard(
    fund_prefix: Option<&Bound<'_, PyAny>>,
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
                return Err(PyValueError::new_err("fund_prefix items must be str or re.Pattern"));
            }
        }
    }

    let pipe = TextFilterSfdrArticleStandard::new(prefix_strings, prefix_patterns, demand_investment_funds_match)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyTextFilterPipe::new(Arc::new(pipe)))
}

/// `TextFilterAssetsStandard(date_regex=None, remove_from_fund_regexes=None)`.
///
/// Entrambi gli argomenti accettano tanto una regex già compilata quanto il suo sorgente come
/// stringa: la compilazione vera la fa Oniguruma dentro il tipo nativo, quindi da un `re.Pattern`
/// si prende solo `.pattern`.
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

    let pipe = TextFilterAssetsStandard::new(date_regex.as_deref(), remove_from_fund_regexes)
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyTextFilterPipe::new(Arc::new(pipe)))
}
