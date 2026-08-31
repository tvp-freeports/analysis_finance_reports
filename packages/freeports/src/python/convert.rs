//! Conversions between the Python types and the crate's own.
//!
//! The heart of the shim layer: every function here **turns Python arguments into Rust ones**, and
//! back, with no domain logic. The crate proper does not know this module exists.

use std::collections::{BTreeMap, BTreeSet};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFrozenSet, PyList, PySet, PyTuple};

use crate::commons::date::Date;
use crate::core::classes::BlockValue;
use crate::core::promise::Promise;

use super::consts::{PyCurrency, PyFinancialInstrument, PySfdrArticle};
use super::core::PyPromise;

/// A block value from any Python object.
///
/// The order of the branches is not arbitrary:
///
/// - `bool` **before** `int`, because in Python `True` is an integer and swapping them would make every boolean arrive as an integer;
/// - the typed shims **before** the containers, being opaque classes the generic branches would not recognise;
/// - `dict` before list and set, for symmetry with the reverse conversion.
pub fn block_value_from_py(object: &Bound<'_, PyAny>) -> PyResult<BlockValue> {
    if object.is_none() {
        return Ok(BlockValue::Null);
    }
    if let Ok(value) = object.extract::<bool>() {
        return Ok(BlockValue::Bool(value));
    }
    if let Ok(value) = object.extract::<i64>() {
        return Ok(BlockValue::Int(value));
    }
    if let Ok(value) = object.extract::<f64>() {
        return Ok(BlockValue::from(value));
    }
    if let Ok(value) = object.extract::<String>() {
        return Ok(BlockValue::Str(value));
    }
    if let Ok(currency) = object.extract::<PyRef<'_, PyCurrency>>() {
        return Ok(BlockValue::Currency(currency.inner()));
    }
    if let Ok(article) = object.extract::<PyRef<'_, PySfdrArticle>>() {
        return Ok(BlockValue::SfdrArticle(article.inner()));
    }
    if let Ok(instrument) = object.extract::<PyRef<'_, PyFinancialInstrument>>() {
        return Ok(BlockValue::FinancialInstrument(instrument.inner()));
    }
    if let Ok(promise) = object.extract::<PyRef<'_, PyPromise>>() {
        return Ok(BlockValue::Promise(promise.inner().clone()));
    }
    if let Some(date) = date_from_py(object)? {
        return Ok(BlockValue::Date(date));
    }
    if let Ok(dict) = object.cast::<PyDict>() {
        let mut map = BTreeMap::new();
        for (key, value) in dict.iter() {
            map.insert(key.extract::<String>()?, block_value_from_py(&value)?);
        }
        return Ok(BlockValue::Map(map));
    }
    if object.is_instance_of::<PySet>() || object.is_instance_of::<PyFrozenSet>() {
        let mut set = BTreeSet::new();
        for item in object.try_iter()? {
            set.insert(block_value_from_py(&item?)?);
        }
        return Ok(BlockValue::Set(set));
    }
    if object.is_instance_of::<PyList>() || object.is_instance_of::<PyTuple>() {
        let mut items = Vec::new();
        for item in object.try_iter()? {
            items.push(block_value_from_py(&item?)?);
        }
        return Ok(BlockValue::List(items));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "cannot convert a Python {} into a block value",
        object.get_type().name()?
    )))
}

/// A Python date — but not a datetime, which subclasses it and carries a time the native type
/// cannot represent — as a native date.
///
/// Returning nothing means "this is not a date", not "the conversion failed": it is a recognition
/// branch, not an error.
///
/// The bridge between the two is the ISO string rather than the three fields: the native date keeps
/// them private, and adding accessors would be a change to existing code this layer has undertaken
/// not to make. Its display and parse forms are already exactly the ISO form Python produces and
/// accepts, so the trip loses nothing.
pub fn date_from_py(object: &Bound<'_, PyAny>) -> PyResult<Option<Date>> {
    // Un `datetime.datetime` ha `hour`; escluderlo qui evita di troncare silenziosamente un'ora.
    if object.hasattr("hour")? {
        return Ok(None);
    }
    let Ok(isoformat) = object.call_method0("isoformat") else {
        return Ok(None);
    };
    let date = isoformat
        .extract::<String>()?
        .parse::<Date>()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(Some(date))
}

/// The inverse: a block value as a Python object.
///
/// The four typed variants come back as the corresponding shims — **not** as strings, which is the
/// difference from the shape-based boundary that predates the shims.
pub fn block_value_to_py<'py>(py: Python<'py>, value: &BlockValue) -> PyResult<Bound<'py, PyAny>> {
    let object = match value {
        BlockValue::Null => py.None().into_bound(py),
        BlockValue::Bool(v) => v.into_pyobject(py)?.to_owned().into_any(),
        BlockValue::Int(v) => v.into_pyobject(py)?.into_any(),
        BlockValue::Float(v) => v.into_inner().into_pyobject(py)?.into_any(),
        BlockValue::Str(v) => v.into_pyobject(py)?.into_any(),
        BlockValue::Date(v) => date_to_py(py, v)?,
        BlockValue::Currency(v) => Bound::new(py, PyCurrency::from(*v))?.into_any(),
        BlockValue::SfdrArticle(v) => Bound::new(py, PySfdrArticle::from(*v))?.into_any(),
        BlockValue::FinancialInstrument(v) => {
            Bound::new(py, PyFinancialInstrument::from(*v))?.into_any()
        }
        BlockValue::Promise(v) => Bound::new(py, PyPromise::from(v.clone()))?.into_any(),
        BlockValue::List(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(block_value_to_py(py, item)?)?;
            }
            list.into_any()
        }
        BlockValue::Set(items) => {
            let set = PySet::empty(py)?;
            for item in items {
                set.add(block_value_to_py(py, item)?)?;
            }
            set.into_any()
        }
        BlockValue::Map(map) => {
            let dict = PyDict::new(py);
            for (key, item) in map {
                dict.set_item(key, block_value_to_py(py, item)?)?;
            }
            dict.into_any()
        }
    };
    Ok(object)
}

/// A native date as a Python date, by the same ISO route and for the same reason.
fn date_to_py<'py>(py: Python<'py>, date: &Date) -> PyResult<Bound<'py, PyAny>> {
    py.import("datetime")?.getattr("date")?.call_method1("fromisoformat", (date.to_string(),))
}

/// A block's metadata: always a map from string to value. Nothing means the empty map, so that a
/// block can be written without metadata at all.
pub fn metadata_from_py(
    object: Option<&Bound<'_, PyAny>>,
) -> PyResult<BTreeMap<String, BlockValue>> {
    let Some(object) = object else { return Ok(BTreeMap::new()) };
    match block_value_from_py(object)? {
        BlockValue::Map(map) => Ok(map),
        BlockValue::Null => Ok(BTreeMap::new()),
        other => Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "block metadata must be a mapping, found a {}",
            other.kind()
        ))),
    }
}

/// A metadata map as a Python dict.
pub fn metadata_to_py<'py>(
    py: Python<'py>,
    metadata: &BTreeMap<String, BlockValue>,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (key, value) in metadata {
        dict.set_item(key, block_value_to_py(py, value)?)?;
    }
    Ok(dict)
}

/// A promise built from Python: accepts the real shim, or the shape-based form that author modules
/// used before the shims existed.
pub fn promise_from_py(object: &Bound<'_, PyAny>) -> PyResult<Promise> {
    if let Ok(promise) = object.extract::<PyRef<'_, PyPromise>>() {
        return Ok(promise.inner().clone());
    }
    let id: String = object.getattr("id")?.extract()?;
    let strict: bool = object.getattr("strict")?.extract()?;
    let multiple: bool = object.getattr("multiple")?.extract()?;
    Ok(Promise::with_flags(&id, strict, multiple))
}
