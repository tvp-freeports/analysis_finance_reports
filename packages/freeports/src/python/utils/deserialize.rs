//! Shim di `freeports.utils.deserialize`: i cast e i due decoratori di tipo blocco.
//!
//! I cast sono funzioni pure di `formats_utils::deserialize::cast`; i decoratori
//! (`deserialize_block_type`/`deserialize_block_types`) non hanno un corrispettivo nativo, e non
//! l'avranno: restringono un callable **Python** a certi tipi di blocco, cioè fanno una cosa che
//! ha senso solo da questo lato del confine. Nel riferimento erano tre righe di Python; qui sono
//! due classi-decoratore, la stessa forma già usata da `text_filter`'s `fund_filter_data`.

use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::commons::date::Date;
use crate::formats_utils::deserialize::cast;

use crate::python::consts::PyCurrency;

/// Un errore di cast come `ValueError` Python — è come il riferimento li faceva arrivare.
fn cast_error(error: cast::CastError) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(error.to_string())
}

/// Una [`Date`] nativa come `datetime.date` Python.
fn date_to_py<'py>(py: Python<'py>, date: Date) -> PyResult<Bound<'py, PyAny>> {
    py.import("datetime")?.getattr("date")?.call_method1("fromisoformat", (date.to_string(),))
}

/// Un numero in virgola mobile da testo. `keep_sign` conserva il segno meno iniziale.
#[pyfunction]
#[pyo3(name = "to_float", signature = (data, keep_sign=false))]
pub fn py_to_float(data: &str, keep_sign: bool) -> PyResult<f64> {
    cast::to_float(data, keep_sign).map_err(cast_error)
}

/// Un intero da testo.
#[pyfunction]
#[pyo3(name = "to_int", signature = (data, keep_sign=false))]
pub fn py_to_int(data: &str, keep_sign: bool) -> PyResult<i64> {
    cast::to_int(data, keep_sign).map_err(cast_error)
}

/// Una percentuale come frazione (`norm`) o come numero puro.
#[pyfunction]
#[pyo3(name = "perc_to_float", signature = (perc, norm=true, keep_sign=false))]
pub fn py_perc_to_float(perc: &str, norm: bool, keep_sign: bool) -> PyResult<f64> {
    cast::perc_to_float(perc, norm, keep_sign).map_err(cast_error)
}

/// Il testo ripulito.
#[pyfunction]
#[pyo3(name = "to_str", signature = (data))]
pub fn py_to_str(data: &str) -> String {
    cast::to_str(data)
}

/// Una valuta da testo.
#[pyfunction]
#[pyo3(name = "to_currency", signature = (data))]
pub fn py_to_currency(data: &str) -> PyResult<PyCurrency> {
    cast::to_currency(data).map(PyCurrency::from).map_err(cast_error)
}

/// Una data da testo in formato numerico.
#[pyfunction]
#[pyo3(name = "to_date", signature = (data))]
pub fn py_to_date<'py>(py: Python<'py>, data: &str) -> PyResult<Bound<'py, PyAny>> {
    date_to_py(py, cast::to_date(data).map_err(cast_error)?)
}

/// Il numero del mese da un nome di mese inglese.
#[pyfunction]
#[pyo3(name = "to_int_en_month", signature = (text))]
pub fn py_to_int_en_month(text: &str) -> PyResult<u8> {
    cast::to_int_en_month(text).map_err(cast_error)
}

/// Il numero del mese da un nome di mese italiano.
#[pyfunction]
#[pyo3(name = "to_int_it_month", signature = (text))]
pub fn py_to_int_it_month(text: &str) -> PyResult<u8> {
    cast::to_int_it_month(text).map_err(cast_error)
}

/// Una data il cui mese è scritto in inglese.
#[pyfunction]
#[pyo3(name = "to_date_with_en_month", signature = (text))]
pub fn py_to_date_with_en_month<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    date_to_py(py, cast::to_date_with_en_month(text).map_err(cast_error)?)
}

/// Una data il cui mese è scritto in italiano.
#[pyfunction]
#[pyo3(name = "to_date_with_it_month", signature = (text))]
pub fn py_to_date_with_it_month<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    date_to_py(py, cast::to_date_with_it_month(text).map_err(cast_error)?)
}

/// `True` se il testo ha la forma di un numero.
#[pyfunction]
#[pyo3(name = "is_numeric_shape", signature = (data))]
pub fn py_is_numeric_shape(data: &str) -> bool {
    cast::is_numeric_shape(data)
}

/// I due decoratori che restringono un deserializer ai blocchi di certi tipi.
///
/// Il decorato riceve il blocco solo se il suo `type_block` è fra quelli dichiarati; altrimenti
/// il risultato è `None`, che i segmenti a valle sanno già ignorare. Sono decoratori
/// *parametrici* — `@deserialize_block_type("FUND")`, non `@deserialize_block_type` — quindi
/// chiamarli costruisce la fabbrica, e chiamare la fabbrica avvolge la funzione.
macro_rules! block_type_decorator {
    ($factory:ident, $wrapper:ident, $py_name:literal, $doc:literal) => {
        #[doc = $doc]
        #[pyclass(name = $py_name, module = "freeports.utils.deserialize", frozen)]
        pub struct $factory {
            types: Vec<String>,
        }

        #[pymethods]
        impl $factory {
            /// Un solo tipo o molti: la stessa firma variadica copre entrambi i decoratori, e
            /// `deserialize_block_type("FUND")` è semplicemente il caso a un argomento.
            #[new]
            #[pyo3(signature = (*blk_types))]
            fn new(blk_types: &Bound<'_, PyTuple>) -> PyResult<$factory> {
                Ok($factory {
                    types: blk_types.iter().map(|t| t.extract::<String>()).collect::<PyResult<_>>()?,
                })
            }

            fn __call__(&self, wrapped: Py<PyAny>) -> $wrapper {
                $wrapper { types: self.types.clone(), wrapped }
            }
        }

        #[doc = concat!("La funzione avvolta da `", $py_name, "`.")]
        #[pyclass(module = "freeports.utils.deserialize", frozen)]
        pub struct $wrapper {
            types: Vec<String>,
            wrapped: Py<PyAny>,
        }

        #[pymethods]
        impl $wrapper {
            /// Gli argomenti sono passati alla cieca: così il decoratore vale sia per una
            /// funzione `(txt_blk)` sia per un metodo `(self, txt_blk)`, senza due varianti come
            /// nel riferimento. Il blocco è sempre l'ultimo argomento.
            #[pyo3(signature = (*args))]
            fn __call__<'py>(
                &self,
                py: Python<'py>,
                args: &Bound<'py, PyTuple>,
            ) -> PyResult<Bound<'py, PyAny>> {
                let Some(block) = args.iter().last() else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "a block-type decorator needs the text block as its last argument",
                    ));
                };
                let type_block: String = block.getattr("type_block")?.extract()?;
                if self.types.iter().any(|t| *t == type_block) {
                    self.wrapped.bind(py).call1(args)
                } else {
                    Ok(py.None().into_bound(py))
                }
            }
        }
    };
}

block_type_decorator!(
    PyDeserializeBlockType,
    PyDeserializeBlockTypeWrapper,
    "deserialize_block_type",
    "Decoratore parametrico: restringe un deserializer a **un** tipo di blocco."
);
block_type_decorator!(
    PyDeserializeBlockTypes,
    PyDeserializeBlockTypesWrapper,
    "deserialize_block_types",
    "Decoratore parametrico: restringe un deserializer a **piu'** tipi di blocco."
);
