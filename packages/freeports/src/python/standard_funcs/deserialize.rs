//! Shim di `freeports.standard_funcs.deserialize`: gli otto pipe standard del terzo segmento.

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

/// `DeserializerInvestmentStandard(cost_and_value_interpret_int=True, quantity_interpret_float=False)`.
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

/// Un callable Python come convertitore d'importo nativo.
///
/// Un'eccezione sollevata dal callable diventa [`CastError::NotANumber`]: dal punto di vista del
/// deserializer è esattamente ciò che significa (questa stringa non si converte in un numero), ed
/// è l'errore che il ramo predefinito produrrebbe nello stesso caso.
fn num_converter_of(callable: Py<PyAny>) -> NumConverter {
    Arc::new(move |text: &str| {
        Python::attach(|py| {
            callable.bind(py).call1((text,)).and_then(|value| value.extract::<f64>()).map_err(|_| {
                CastError::NotANumber { data: text.to_string() }
            })
        })
    })
}

/// Un callable Python come convertitore di date nativo.
///
/// Accetta ciò che i `to_date*` di `freeports.utils.deserialize` restituiscono (un
/// `datetime.date`) e, per tolleranza, anche una stringa ISO — la stessa coppia di forme che
/// [`crate::python::convert`] riconosce ovunque.
fn date_converter_of(callable: Py<PyAny>) -> DateConverter {
    Arc::new(move |text: &str| {
        Python::attach(|py| {
            let unrecognized = || CastError::UnrecognizedDateFormat { data: text.to_string() };
            let value = callable.bind(py).call1((text,)).map_err(|_| unrecognized())?;
            if let Ok(Some(date)) = date_from_py(&value) {
                return Ok(date);
            }
            value.extract::<String>().ok().and_then(|iso| iso.parse::<Date>().ok()).ok_or_else(unrecognized)
        })
    })
}

/// `DeserializerAssetsStandard(num_converter, date_converter=to_date)`.
///
/// **Divergenza assorbita qui:** il tipo nativo sceglie fra `to_int` e `to_float` con un `bool`,
/// mentre il riferimento accetta due *callable* qualunque — e i moduli d'autore ne approfittano
/// davvero (`num_converter=lambda txt: 0 if txt == "-" else to_int(txt)`). Il ponte non prova a
/// indovinare quale funzione predefinita corrisponda al callable: lo chiama, tramite
/// [`num_converter_of`] e [`date_converter_of`], che è l'unica traduzione che non perde
/// comportamento. Il costruttore nativo che accetta convertitori arbitrari è
/// [`DeserializerAssetsStandard::with_converters`].
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
