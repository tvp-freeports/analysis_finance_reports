//! The shims of the eight standard pipes of the deserialization segment.

use std::sync::Arc;

use pyo3::prelude::*;

use crate::commons::date::Date;
use crate::formats_utils::deserialize::cast::{self, CastError};
use crate::formats_utils::deserialize::standard_funcs::{
    DateConverter, DeserializeSfdrArticleStandard, DeserializerAssetsStandard, DeserializerFundStandard,
    DeserializerInvestmentStandard, DeserializerInvestmentsManagerFromManco,
    DeserializerInvestmentsManagerStandard, DeserializerManagmentCompanyStandard,
    DeserializerPageClassifyStandard, NumConverter,
};

use crate::python::convert::date_from_py;
use crate::python::pipes::PyDeserializePipe;
use crate::core::tracing_setup::log_error;

/// `DeserializerPageClassifyStandard()`.
#[pyfunction]
#[pyo3(name = "DeserializerPageClassifyStandard")]
pub fn py_deserializer_page_classify_standard() -> PyDeserializePipe {
    PyDeserializePipe::new(Arc::new(DeserializerPageClassifyStandard))
}

/// `DeserializerFundStandard()`.
#[pyfunction]
#[pyo3(name = "DeserializerFundStandard")]
pub fn py_deserializer_fund_standard() -> PyDeserializePipe {
    PyDeserializePipe::new(Arc::new(DeserializerFundStandard))
}

/// `DeserializerManagmentCompanyStandard()`.
#[pyfunction]
#[pyo3(name = "DeserializerManagmentCompanyStandard")]
pub fn py_deserializer_managment_company_standard() -> PyDeserializePipe {
    PyDeserializePipe::new(Arc::new(DeserializerManagmentCompanyStandard))
}

/// `DeserializerInvestmentsManagerStandard()` — legge un blocco `INVESTMENTS_MANAGER`.
#[pyfunction]
#[pyo3(name = "DeserializerInvestmentsManagerStandard")]
pub fn py_deserializer_investments_manager_standard() -> PyDeserializePipe {
    PyDeserializePipe::new(Arc::new(DeserializerInvestmentsManagerStandard))
}

/// `DeserializerInvestmentsManagerFromManco()` — legge un blocco `MANAGEMENT_COMPANY` e ne ricava
/// comunque un `InvestmentsManager`.
#[pyfunction]
#[pyo3(name = "DeserializerInvestmentsManagerFromManco")]
pub fn py_deserializer_investments_manager_from_manco() -> PyDeserializePipe {
    PyDeserializePipe::new(Arc::new(DeserializerInvestmentsManagerFromManco))
}

/// `DeserializeSfdrArticleStandard()`.
#[pyfunction]
#[pyo3(name = "DeserializeSfdrArticleStandard")]
pub fn py_deserialize_sfdr_article_standard() -> PyDeserializePipe {
    PyDeserializePipe::new(Arc::new(DeserializeSfdrArticleStandard))
}

/// `DeserializerInvestmentStandard(cost_and_value_interpret_int=True,
/// quantity_interpret_float=False)`.
#[pyfunction]
#[pyo3(name = "DeserializerInvestmentStandard")]
#[pyo3(signature = (cost_and_value_interpret_int=true, quantity_interpret_float=false))]
pub fn py_deserializer_investment_standard(
    cost_and_value_interpret_int: bool,
    quantity_interpret_float: bool,
) -> PyDeserializePipe {
    PyDeserializePipe::new(Arc::new(DeserializerInvestmentStandard::new(
        cost_and_value_interpret_int,
        quantity_interpret_float,
    )))
}

/// A Python callable as a native amount converter.
///
/// An exception raised by the callable becomes a not-a-number error: from the deserializer's point
/// of view that is exactly what it means, and it is the error the built-in branch would produce in
/// the same case.
fn num_converter_of(callable: Py<PyAny>) -> NumConverter {
    Arc::new(move |text: &str| {
        Python::attach(|py| {
            callable.bind(py).call1((text,)).and_then(|value| value.extract::<f64>()).map_err(|err| {
                // The Python exception's own detail is discarded past this point (rule 1 of L2):
                // logged here, same "cast failed" severity as
                // `python::utils::deserialize::cast_error`.
                tracing::warn!(error = log_error(&err), data = text, "custom num_converter callable failed: {err}");
                CastError::NotANumber { data: text.to_string() }
            })
        })
    })
}

/// A Python callable as a native date converter.
///
/// Accepts what the utility date functions return, and, for tolerance, an ISO string too — the same
/// pair of forms recognised everywhere else at this boundary.
fn date_converter_of(callable: Py<PyAny>) -> DateConverter {
    Arc::new(move |text: &str| {
        Python::attach(|py| {
            let unrecognized = |detail: Option<String>| {
                // Same rationale as `num_converter_of`: the Python-side detail (exception or
                // unrecognized return value) is discarded past this point, so it is logged here.
                match detail {
                    Some(detail) => tracing::warn!(data = text, "custom date_converter callable failed: {detail}"),
                    None => tracing::warn!(data = text, "custom date_converter callable returned an unrecognized date value"),
                }
                CastError::UnrecognizedDateFormat { data: text.to_string() }
            };
            let value = callable.bind(py).call1((text,)).map_err(|err| unrecognized(Some(err.to_string())))?;
            if let Ok(Some(date)) = date_from_py(&value) {
                return Ok(date);
            }
            value
                .extract::<String>()
                .ok()
                .and_then(|iso| iso.parse::<Date>().ok())
                .ok_or_else(|| unrecognized(None))
        })
    })
}

/// `DeserializerAssetsStandard(num_converter, date_converter=to_date)`.
///
/// **A divergence absorbed here:** the Python signature accepts two arbitrary *callables*, and
/// author modules really do use that — something along the lines of "treat a dash as zero,
/// otherwise parse an integer". The bridge does not try to guess which built-in function a
/// callable corresponds to: it calls it, through `num_converter_of` and `date_converter_of`,
/// which is the only translation that loses no behaviour.
#[pyfunction]
#[pyo3(name = "DeserializerAssetsStandard")]
#[pyo3(signature = (num_converter, date_converter=None))]
pub fn py_deserializer_assets_standard(
    num_converter: Py<PyAny>,
    date_converter: Option<Py<PyAny>>,
) -> PyDeserializePipe {
    let date_converter: DateConverter = match date_converter {
        Some(callable) => date_converter_of(callable),
        None => Arc::new(cast::to_date),
    };
    PyDeserializePipe::new(Arc::new(DeserializerAssetsStandard::with_converters(
        num_converter_of(num_converter),
        date_converter,
    )))
}
