//! The shims of the casts and the two block-type decorators.

use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::commons::date::Date;
use crate::formats_utils::deserialize::cast;

use crate::python::consts::PyCurrency;
use crate::core::tracing_setup::log_error;

/// A cast error as a Python `ValueError`.
///
/// Logged before the conversion: past this point the error lives only as a Python exception,
/// invisible to this crate's own logging. A warning rather than an error, because it is the
/// canonical failed cast — much author code catches it and carries on.
fn cast_error(error: cast::CastError) -> PyErr {
    tracing::warn!(error = log_error(&error), "cast failed: {error}");
    pyo3::exceptions::PyValueError::new_err(error.to_string())
}

/// A native date as a Python date.
fn date_to_py<'py>(py: Python<'py>, date: Date) -> PyResult<Bound<'py, PyAny>> {
    py.import("datetime")?.getattr("date")?.call_method1("fromisoformat", (date.to_string(),))
}

/// A floating-point number from text. A leading minus is a sign and is always honoured; wrap the
/// call in `abs` where the report writes a magnitude with one.
#[pyfunction]
#[pyo3(name = "to_float", signature = (data))]
pub fn py_to_float(data: &str) -> PyResult<f64> {
    cast::to_float(data).map_err(cast_error)
}

/// Un intero da testo.
#[pyfunction]
#[pyo3(name = "to_int", signature = (data))]
pub fn py_to_int(data: &str) -> PyResult<i64> {
    cast::to_int(data).map_err(cast_error)
}

/// A percentage as a fraction, or as a plain number.
#[pyfunction]
#[pyo3(name = "perc_to_float", signature = (perc, norm=true))]
pub fn py_perc_to_float(perc: &str, norm: bool) -> PyResult<f64> {
    cast::perc_to_float(perc, norm).map_err(cast_error)
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

/// A date, from text to a numeric form.
#[pyfunction]
#[pyo3(name = "to_date", signature = (data))]
pub fn py_to_date<'py>(py: Python<'py>, data: &str) -> PyResult<Bound<'py, PyAny>> {
    date_to_py(py, cast::to_date(data).map_err(cast_error)?)
}

/// The month number from an English month name.
#[pyfunction]
#[pyo3(name = "to_int_en_month", signature = (text))]
pub fn py_to_int_en_month(text: &str) -> PyResult<u8> {
    cast::to_int_en_month(text).map_err(cast_error)
}

/// The month number from an Italian month name.
#[pyfunction]
#[pyo3(name = "to_int_it_month", signature = (text))]
pub fn py_to_int_it_month(text: &str) -> PyResult<u8> {
    cast::to_int_it_month(text).map_err(cast_error)
}

/// A date whose month is written in English.
#[pyfunction]
#[pyo3(name = "to_date_with_en_month", signature = (text))]
pub fn py_to_date_with_en_month<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    date_to_py(py, cast::to_date_with_en_month(text).map_err(cast_error)?)
}

/// A date whose month is written in Italian.
#[pyfunction]
#[pyo3(name = "to_date_with_it_month", signature = (text))]
pub fn py_to_date_with_it_month<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    date_to_py(py, cast::to_date_with_it_month(text).map_err(cast_error)?)
}

/// Whether the text has the shape of a number.
#[pyfunction]
#[pyo3(name = "is_numeric_shape", signature = (data))]
pub fn py_is_numeric_shape(data: &str) -> bool {
    cast::is_numeric_shape(data)
}

/// The two decorators that restrict a deserializer to blocks of certain types.
///
/// The decorated function receives the block only if its type is among those declared; otherwise
/// the result is nothing, which the downstream segments already know to ignore. They are
/// *parametric* decorators, so calling one builds the factory and calling the factory wraps the
/// function.
macro_rules! block_type_decorator {
    ($factory:ident, $wrapper:ident, $py_name:literal, $doc:literal) => {
        #[doc = $doc]
        #[pyclass(name = $py_name, module = "freeports.utils.deserialize", frozen)]
        pub struct $factory {
            types: Vec<String>,
        }

        #[pymethods]
        impl $factory {
            /// One type or many: the same variadic signature covers both decorators, a single type
            /// simply being the one-argument case.
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
            /// The arguments are passed through blindly, so that the decorator works both for a
            /// plain function and for a method, without two variants. The block is always the last
            /// argument.
            #[pyo3(signature = (*args))]
            fn __call__<'py>(
                &self,
                py: Python<'py>,
                args: &Bound<'py, PyTuple>,
            ) -> PyResult<Bound<'py, PyAny>> {
                let Some(block) = args.iter().last() else {
                    // A wiring mistake in the format's own code, not a per-value cast failure:
                    // the decorated deserializer was called with no arguments at all.
                    tracing::error!("block-type decorator called with no arguments, no text block to read");
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
