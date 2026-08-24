//! Conversioni fra i tipi Python e i tipi del crate.
//!
//! È il cuore del layer di shim (`M10-implementation-plan.md`, passo P2): ogni funzione qui
//! **trasforma argomenti Python in argomenti Rust** e viceversa, senza contenere logica di
//! dominio. Il crate vero e proprio non sa che questo modulo esiste — la regola di `PLAN.md` §3
//! (PyO3 solo al confine) resta rispettata, perché tutto ciò che usa `pyo3` fuori dai due moduli
//! di confine storici vive sotto `crate::python`.
//!
//! # Perché non si annota `#[pyclass]` sui tipi interni
//!
//! Sarebbe stato meno codice, ma avrebbe sparso attributi PyO3 su `core::classes`,
//! `commons::consts`, `output::classes` e via dicendo — esattamente ciò che `PLAN.md` §14
//! chiama «un errore di design, non una scorciatoia». Gli shim sono newtype che avvolgono il
//! valore Rust: il codice esistente non cambia di una riga.

use std::collections::{BTreeMap, BTreeSet};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFrozenSet, PyList, PySet, PyTuple};

use crate::commons::date::Date;
use crate::core::classes::BlockValue;
use crate::core::promise::Promise;

use super::consts::{PyCurrency, PyFinancialInstrument, PySfdrArticle};
use super::core::PyPromise;

/// Un `BlockValue` a partire da un qualunque oggetto Python.
///
/// L'ordine dei rami non è arbitrario:
///
/// - `bool` **prima** di `int`, perché in Python `True` è un intero e invertirli farebbe
///   arrivare ogni booleano come `Int(1)`;
/// - gli shim tipizzati (`Currency`/`SfdrArticle`/`FinancialInstrument`/`Promise`/`date`)
///   **prima** dei contenitori, perché sono `#[pyclass]` opachi che i rami generici non
///   riconoscerebbero;
/// - `dict` **prima** di list/set, per simmetria con `block_value_to_py`.
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

/// Un `datetime.date` Python (ma non un `datetime.datetime`, che ne è sottoclasse e porta
/// un'ora che `commons::date::Date` non sa rappresentare) come [`Date`] nativa.
///
/// `Ok(None)` significa "non è una data", non "conversione fallita": è il ramo di
/// riconoscimento di [`block_value_from_py`], non un errore.
///
/// Il ponte fra i due tipi è la stringa ISO `YYYY-MM-DD`, non i tre campi: `commons::date::Date`
/// tiene `year`/`month`/`day` privati e non espone accessori, e aggiungerne sarebbe una modifica
/// al codice esistente che questo layer si è imposto di non fare. `Display`/`FromStr` di `Date`
/// sono già esattamente l'ISO che `datetime.date.isoformat()` produce e che
/// `datetime.date.fromisoformat()` accetta, quindi il giro non perde nulla.
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

/// L'inverso di [`block_value_from_py`]: un `BlockValue` come oggetto Python.
///
/// Le quattro varianti tipizzate tornano come gli shim corrispondenti — **non** come stringhe:
/// è la differenza rispetto al confine duck-typed di `formats_repo::unstructured::py_pipe`,
/// che quegli shim non li aveva ancora a disposizione.
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

/// Una [`Date`] nativa come `datetime.date` Python — stessa strada ISO di [`date_from_py`],
/// per la stessa ragione (i campi di `Date` sono privati).
fn date_to_py<'py>(py: Python<'py>, date: &Date) -> PyResult<Bound<'py, PyAny>> {
    py.import("datetime")?.getattr("date")?.call_method1("fromisoformat", (date.to_string(),))
}

/// I metadati di un blocco: sempre una mappa da stringa a valore. `None` vale mappa vuota, così
/// che `PdfBlock("x", content=...)` senza metadati sia scrivibile dal codice d'autore.
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

/// Una mappa di metadati come `dict` Python.
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

/// Una promessa costruita da Python: accetta lo shim vero, oppure la forma duck-typed
/// (`id`/`strict`/`multiple`) che i moduli d'autore usavano prima che gli shim esistessero.
pub fn promise_from_py(object: &Bound<'_, PyAny>) -> PyResult<Promise> {
    if let Ok(promise) = object.extract::<PyRef<'_, PyPromise>>() {
        return Ok(promise.inner().clone());
    }
    let id: String = object.getattr("id")?.extract()?;
    let strict: bool = object.getattr("strict")?.extract()?;
    let multiple: bool = object.getattr("multiple")?.extract()?;
    Ok(Promise::with_flags(&id, strict, multiple))
}
