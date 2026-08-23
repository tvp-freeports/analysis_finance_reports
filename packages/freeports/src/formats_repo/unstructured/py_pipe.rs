//! Gli adattatori che fanno di un callable Python un pipe come tutti gli altri.
//!
//! È uno dei due soli punti di contatto con Python del crate (`PLAN.md` §3): i pipe definiti
//! dall'autore di un formato implementano gli stessi trait dei pipe nativi, quindi il motore non
//! sa — e non deve sapere — se un pipe è Rust o Python.
//!
//! # Il contratto verso Python, e perché è duck-typed
//!
//! In questa fase il crate **non** espone alcuna API Python (`PLAN.md` §0: nessun binding, niente
//! `cdylib`), quindi non esistono classi `PdfBlock`/`TextBlock`/`Promise` che il codice d'autore
//! possa importare. La conversione è perciò definita per *forma*, non per tipo — decisione
//! **D-M7-3** dell'utente (2026-08-23):
//!
//! | Concetto | Forma accettata da Python |
//! |---|---|
//! | `Promise` | un oggetto con gli attributi `id`, `strict`, `multiple`, oppure una mappa con esattamente quelle tre chiavi |
//! | `PdfBlock` | un oggetto o una mappa con `type_block`, `metadata`, `content` |
//! | `TextBlock` | come sopra, più un `pdf_block` facoltativo |
//! | `BlockValue` | i tipi primitivi Python (`None`, `bool`, `int`, `float`, `str`), più `list`/`tuple`, `set`/`frozenset`, `dict` |
//!
//! Gli attributi scelti **non** sono inventati: sono esattamente quelli che le classi
//! corrispondenti espongono già in `freeports_core` (i getter `id`/`strict`/`multiple` di
//! `Promise`, i campi `type_block`/`metadata`/`content` di `PdfBlock`). Quando i binding
//! arriveranno, le classi vere soddisferanno questo protocollo senza che qui cambi nulla — che è
//! la ragione per cui è stato scelto così invece di aspettare i binding.
//!
//! **Limite noto e accettato**: le varianti tipizzate di `BlockValue` (`Date`, `Currency`,
//! `SfdrArticle`, `FinancialInstrument`) non hanno una forma Python in questa fase — nessun
//! codice d'autore può costruirle senza le classi — e arrivano quindi come stringhe, che è ciò
//! che i pipe `deserialize` a valle già sanno convertire.
//!
//! # Errori
//!
//! Un `PyErr` che esce da un pipe d'autore viene loggato con il traceback e convertito in
//! [`PipeError::author`] al confine: nessun `PyErr` risale oltre questo modulo (`PLAN.md` §3).

use std::collections::{BTreeMap, BTreeSet};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFrozenSet, PyList, PySet, PyTuple};

use crate::core::classes::{BlockType, BlockValue, PdfBlock, TextBlock};
use crate::core::page::Page;
use crate::core::pipeline::{
    DeserializePipe, Extracted, FilterData, PdfExtractPipe, PipeError, PromiseEntries, TextFilterPipe,
};
use crate::core::promise::Promise;

/// I tre attributi che identificano una promessa, in Python.
const PROMISE_FIELDS: [&str; 3] = ["id", "strict", "multiple"];

/// Legge un attributo, accettando indifferentemente un attributo d'istanza o una chiave di mappa.
///
/// È il cuore del duck-typing: un `#[pyclass]` futuro esporrà attributi, un dizionario scritto a
/// mano in un modulo d'autore espone chiavi, e per questo modulo le due cose sono la stessa.
fn field<'py>(object: &Bound<'py, PyAny>, name: &str) -> Option<Bound<'py, PyAny>> {
    if let Ok(value) = object.getattr(name) {
        return Some(value);
    }
    object.get_item(name).ok()
}

/// `true` se l'oggetto ha tutti e tre gli attributi di una promessa.
fn looks_like_promise(object: &Bound<'_, PyAny>) -> bool {
    PROMISE_FIELDS.iter().all(|name| field(object, name).is_some())
}

/// Un `BlockValue` a partire da un qualunque oggetto Python. Vedi il doc-comment del modulo per
/// il contratto.
pub fn block_value_from_py(object: &Bound<'_, PyAny>) -> PyResult<BlockValue> {
    if object.is_none() {
        return Ok(BlockValue::Null);
    }
    // `bool` prima di `int`: in Python `True` **è** un intero, e invertire i due rami farebbe
    // arrivare ogni booleano come `Int(1)`.
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
    // La promessa va riconosciuta **prima** della mappa: la sua forma dizionario è un dizionario a
    // tutti gli effetti, e il ramo generico se la mangerebbe.
    if looks_like_promise(object) {
        return Ok(BlockValue::Promise(promise_from_py(object)?));
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

/// Una promessa da un oggetto che ne ha la forma.
fn promise_from_py(object: &Bound<'_, PyAny>) -> PyResult<Promise> {
    let id: String = field(object, "id")
        .ok_or_else(|| pyo3::exceptions::PyAttributeError::new_err("promise has no 'id'"))?
        .extract()?;
    let strict: bool = field(object, "strict")
        .ok_or_else(|| pyo3::exceptions::PyAttributeError::new_err("promise has no 'strict'"))?
        .extract()?;
    let multiple: bool = field(object, "multiple")
        .ok_or_else(|| pyo3::exceptions::PyAttributeError::new_err("promise has no 'multiple'"))?
        .extract()?;
    Ok(Promise::with_flags(&id, strict, multiple))
}

/// I metadati di un blocco: sempre una mappa da stringa a valore.
fn metadata_from_py(object: &Bound<'_, PyAny>) -> PyResult<BTreeMap<String, BlockValue>> {
    match block_value_from_py(object)? {
        BlockValue::Map(map) => Ok(map),
        BlockValue::Null => Ok(BTreeMap::new()),
        other => Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "block metadata must be a mapping, found a {}",
            other.kind()
        ))),
    }
}

/// I tre campi comuni a `PdfBlock` e `TextBlock`.
fn block_parts(object: &Bound<'_, PyAny>) -> PyResult<(BlockType, BTreeMap<String, BlockValue>, BlockValue)> {
    let missing = |name: &str| pyo3::exceptions::PyAttributeError::new_err(format!("block has no '{name}'"));
    let type_block: String = field(object, "type_block").ok_or_else(|| missing("type_block"))?.extract()?;
    let metadata = match field(object, "metadata") {
        Some(value) => metadata_from_py(&value)?,
        None => BTreeMap::new(),
    };
    let content = match field(object, "content") {
        Some(value) => block_value_from_py(&value)?,
        None => BlockValue::Null,
    };
    Ok((BlockType::new(type_block), metadata, content))
}

/// Un `PdfBlock` da un oggetto che ne ha la forma.
pub fn pdf_block_from_py(object: &Bound<'_, PyAny>) -> PyResult<PdfBlock> {
    let (type_block, metadata, content) = block_parts(object)?;
    Ok(PdfBlock::new(type_block, metadata, content))
}

/// Un `TextBlock` da un oggetto che ne ha la forma. Il `pdf_block` è facoltativo.
pub fn text_block_from_py(object: &Bound<'_, PyAny>) -> PyResult<TextBlock> {
    let (type_block, metadata, content) = block_parts(object)?;
    let pdf_block = match field(object, "pdf_block") {
        Some(value) if !value.is_none() => Some(pdf_block_from_py(&value)?),
        _ => None,
    };
    Ok(match pdf_block {
        Some(pdf_block) => TextBlock::new(type_block, metadata, pdf_block),
        None => TextBlock::from_content(type_block, metadata, content),
    })
}

/// Un valore Python a partire da un [`BlockValue`], per passare dati *verso* un pipe d'autore.
pub fn block_value_to_py<'py>(py: Python<'py>, value: &BlockValue) -> PyResult<Bound<'py, PyAny>> {
    Ok(match value {
        BlockValue::Null => py.None().into_bound(py),
        BlockValue::Bool(v) => v.into_pyobject(py)?.to_owned().into_any(),
        BlockValue::Int(v) => v.into_pyobject(py)?.into_any(),
        BlockValue::Float(v) => v.into_inner().into_pyobject(py)?.into_any(),
        BlockValue::Str(v) => v.into_pyobject(py)?.into_any(),
        // Le varianti tipizzate diventano la loro forma testuale: vedi il limite noto documentato
        // in testa al modulo.
        BlockValue::Date(v) => v.to_string().into_pyobject(py)?.into_any(),
        BlockValue::Currency(v) => v.code().into_pyobject(py)?.into_any(),
        BlockValue::SfdrArticle(v) => format!("{v:?}").into_pyobject(py)?.into_any(),
        BlockValue::FinancialInstrument(v) => format!("{v:?}").into_pyobject(py)?.into_any(),
        BlockValue::Promise(promise) => {
            let dict = PyDict::new(py);
            dict.set_item("id", promise.id())?;
            dict.set_item("strict", promise.strict())?;
            dict.set_item("multiple", promise.multiple())?;
            dict.into_any()
        }
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
    })
}

/// Un `PdfBlock` come dizionario Python.
pub fn pdf_block_to_py<'py>(py: Python<'py>, block: &PdfBlock) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("type_block", block.type_block.as_str())?;
    dict.set_item("metadata", block_value_to_py(py, &BlockValue::Map(block.metadata.clone()))?)?;
    dict.set_item("content", block_value_to_py(py, &block.content)?)?;
    Ok(dict.into_any())
}

/// Un `TextBlock` come dizionario Python.
pub fn text_block_to_py<'py>(py: Python<'py>, block: &TextBlock) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("type_block", block.type_block.as_str())?;
    dict.set_item("metadata", block_value_to_py(py, &BlockValue::Map(block.metadata.clone()))?)?;
    dict.set_item("content", block_value_to_py(py, &block.content)?)?;
    match &block.pdf_block {
        Some(pdf_block) => dict.set_item("pdf_block", pdf_block_to_py(py, pdf_block)?)?,
        None => dict.set_item("pdf_block", py.None())?,
    }
    Ok(dict.into_any())
}

/// Il risultato di un callable d'autore, appiattito in una lista.
///
/// Un pipe può restituire `None` (niente da dire), un singolo elemento, o un iterabile: sono le
/// tre forme che il riferimento accetta, e la conversione le tratta tutte allo stesso modo.
fn each<'py, T>(
    result: &Bound<'py, PyAny>,
    convert: impl Fn(&Bound<'py, PyAny>) -> PyResult<T>,
) -> PyResult<Vec<T>> {
    if result.is_none() {
        return Ok(Vec::new());
    }
    match result.try_iter() {
        Ok(iterator) => {
            let mut out = Vec::new();
            for item in iterator {
                out.push(convert(&item?)?);
            }
            Ok(out)
        }
        // Non iterabile: è un blocco solo.
        Err(_) => Ok(vec![convert(result)?]),
    }
}

/// Logga il traceback e converte l'errore d'autore. Nessun `PyErr` esce da questo modulo.
fn author_error(py: Python<'_>, pipeline: &str, pipe: &str, error: PyErr) -> PipeError {
    let message = error.to_string();
    tracing::error!(pipeline, pipe, "author pipe raised: {message}");
    error.print(py);
    PipeError::author(pipeline, pipe, message)
}

/// Un pipe `pdf_extract` definito dall'autore del formato.
pub struct PyPdfExtractPipe {
    pipeline: String,
    name: String,
    func: Py<PyAny>,
}

impl PyPdfExtractPipe {
    pub fn new(pipeline: impl Into<String>, name: impl Into<String>, func: Py<PyAny>) -> Self {
        Self { pipeline: pipeline.into(), name: name.into(), func }
    }
}

impl PdfExtractPipe for PyPdfExtractPipe {
    fn name(&self) -> &str {
        &self.name
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        // Il pipe d'autore si aspetta il dizionario PyMuPDF originale, non la `Page` nativa: è la
        // ragione per cui `Page` lo conserva (`PLAN.md` §3).
        let raw = page.raw().ok_or_else(|| {
            PipeError::author(&self.pipeline, &self.name, "the page carries no PyMuPDF dictionary")
        })?;
        Python::attach(|py| {
            let result = self
                .func
                .bind(py)
                .call1((raw.bind(py),))
                .map_err(|e| author_error(py, &self.pipeline, &self.name, e))?;
            each(&result, pdf_block_from_py).map_err(|e| author_error(py, &self.pipeline, &self.name, e))
        })
    }
}

/// Un pipe `text_filter` definito dall'autore del formato.
pub struct PyTextFilterPipe {
    pipeline: String,
    name: String,
    func: Py<PyAny>,
}

impl PyTextFilterPipe {
    pub fn new(pipeline: impl Into<String>, name: impl Into<String>, func: Py<PyAny>) -> Self {
        Self { pipeline: pipeline.into(), name: name.into(), func }
    }
}

impl TextFilterPipe for PyTextFilterPipe {
    fn name(&self) -> &str {
        &self.name
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        Python::attach(|py| {
            let convert = || -> PyResult<Bound<'_, PyAny>> {
                let py_blocks = PyList::empty(py);
                for block in blocks {
                    py_blocks.append(pdf_block_to_py(py, block)?)?;
                }
                // Le società bersaglio arrivano come lista di stringhe; i risultati degli step
                // precedenti non hanno ancora una forma Python (le entità di `output::classes`
                // sono ancora in gran parte M8) e arrivano come lista vuota.
                let py_data = PyList::empty(py);
                if let FilterData::TargetCompanies(companies) = data {
                    for company in *companies {
                        py_data.append(company.name())?;
                    }
                }
                self.func.bind(py).call1((py_blocks, py_data))
            };
            let result = convert().map_err(|e| author_error(py, &self.pipeline, &self.name, e))?;
            each(&result, text_block_from_py).map_err(|e| author_error(py, &self.pipeline, &self.name, e))
        })
    }
}

/// Un pipe `deserialize` definito dall'autore del formato.
///
/// In questa fase un pipe d'autore può restituire soltanto **promesse** (una mappa da id a
/// valore): le dieci entità di `output::classes` non hanno una forma Python finché i binding non
/// esistono, e M7 ne ha portate solo tre (D-M7-2). Un risultato di forma diversa è un errore
/// esplicito, non un risultato silenziosamente scartato.
pub struct PyDeserializePipe {
    pipeline: String,
    name: String,
    func: Py<PyAny>,
}

impl PyDeserializePipe {
    pub fn new(pipeline: impl Into<String>, name: impl Into<String>, func: Py<PyAny>) -> Self {
        Self { pipeline: pipeline.into(), name: name.into(), func }
    }
}

impl DeserializePipe for PyDeserializePipe {
    fn name(&self) -> &str {
        &self.name
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        Python::attach(|py| {
            let call = || -> PyResult<Vec<BlockValue>> {
                let py_block = text_block_to_py(py, block)?;
                let result = self.func.bind(py).call1((py_block,))?;
                each(&result, block_value_from_py)
            };
            let values = call().map_err(|e| author_error(py, &self.pipeline, &self.name, e))?;

            let mut out = Vec::new();
            for value in values {
                let BlockValue::Map(map) = value else {
                    return Err(PipeError::author(
                        &self.pipeline,
                        &self.name,
                        "an author deserialize pipe must return promise mappings in this phase",
                    ));
                };
                let mut entries = PromiseEntries::new();
                for (id, item) in map {
                    entries.push(id, item);
                }
                out.push(Extracted::Promises(entries));
            }
            Ok(out)
        })
    }
}
