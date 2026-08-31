//! The adapters that turn a Python callable into a pipe like any other.
//!
//! One of the crate's two points of contact with Python. A pipe written by a format's author
//! implements the same traits as a native one, so the engine does not know — and must not need to
//! know — whether a pipe is Rust or Python.
//!
//! # What crosses the boundary
//!
//! An author's pipe receives and returns the **real classes** of [`crate::python`]: blocks,
//! promises, the output entities, the domain enums. That is what author modules import and build,
//! so that is what they must be handed.
//!
//! A looser, shape-based form — a mapping with the right keys instead of a block — is still
//! accepted **on the way in**, as a safety net for a pipe that constructs its results by hand. It
//! is no longer what goes out: an author module writing `pdf_blks[0].content`, and most of them do,
//! would fail on a mapping.
//!
//! Because the real classes cross, the typed variants of a value — dates, currencies, SFDR
//! articles, instrument kinds — arrive as themselves rather than degraded to strings.
//!
//! # Errors
//!
//! A Python exception escaping an author's pipe is logged with its traceback and converted at the
//! boundary: no `PyErr` travels beyond this module.

use std::collections::{BTreeMap, BTreeSet};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFrozenSet, PyList, PySet, PyTuple};

use crate::core::classes::{BlockType, BlockValue, PdfBlock, TextBlock};
use crate::core::page::Page;
use crate::core::pipeline::{
    DeserializePipe, Extracted, FilterData, PdfExtractPipe, PipeError, PromiseEntries, TextFilterPipe,
};
use crate::core::promise::Promise;

/// The three attributes identifying a promise, on the Python side.
const PROMISE_FIELDS: [&str; 3] = ["id", "strict", "multiple"];

/// Reads an attribute, accepting an instance attribute or a mapping key indifferently.
///
/// This is the heart of the shape-based reading: a class exposes attributes, a hand-written
/// dictionary in an author module exposes keys, and for this module the two are the same thing.
fn field<'py>(object: &Bound<'py, PyAny>, name: &str) -> Option<Bound<'py, PyAny>> {
    if let Ok(value) = object.getattr(name) {
        return Some(value);
    }
    object.get_item(name).ok()
}

/// Whether the object has all three attributes of a promise.
fn looks_like_promise(object: &Bound<'_, PyAny>) -> bool {
    PROMISE_FIELDS.iter().all(|name| field(object, name).is_some())
}

/// A [`BlockValue`] from any Python object. See the module documentation for the contract.
pub fn block_value_from_py(object: &Bound<'_, PyAny>) -> PyResult<BlockValue> {
    if object.is_none() {
        return Ok(BlockValue::Null);
    }
    // `bool` before `int`: in Python `True` **is** an integer, and swapping the two branches would
    // make every boolean arrive as an integer.
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
    // A promise must be recognised **before** a mapping: its dictionary form is a dictionary in
    // every respect, and the generic branch would swallow it.
    if looks_like_promise(object) {
        return Ok(BlockValue::Promise(promise_from_py(object)?));
    }
    // The real classes: after the promise branch, because that one also recognises the dictionary
    // *shape* which the class converter would see as an ordinary mapping; before the generic
    // branches, because an enum instance is neither a string nor a mapping and would fall through
    // to the final error.
    if let Ok(value) = crate::python::convert::block_value_from_py(object) {
        return Ok(value);
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

/// A promise from an object shaped like one.
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

/// A block's metadata: always a mapping from string to value.
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

/// The three fields common to a PDF block and a text block.
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

/// A [`PdfBlock`] from an object shaped like one.
pub fn pdf_block_from_py(object: &Bound<'_, PyAny>) -> PyResult<PdfBlock> {
    if let Ok(block) = object.extract::<PyRef<'_, crate::python::core::PyPdfBlock>>() {
        return block.native(object.py());
    }
    let (type_block, metadata, content) = block_parts(object)?;
    Ok(PdfBlock::new(type_block, metadata, content))
}

/// A [`TextBlock`] from an object shaped like one. The originating PDF block is optional.
pub fn text_block_from_py(object: &Bound<'_, PyAny>) -> PyResult<TextBlock> {
    if let Ok(block) = object.extract::<PyRef<'_, crate::python::core::PyTextBlock>>() {
        return block.native(object.py());
    }
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

/// A Python value from a [`BlockValue`], for passing data *to* an author's pipe.
pub fn block_value_to_py<'py>(py: Python<'py>, value: &BlockValue) -> PyResult<Bound<'py, PyAny>> {
    Ok(match value {
        BlockValue::Null => py.None().into_bound(py),
        BlockValue::Bool(v) => v.into_pyobject(py)?.to_owned().into_any(),
        BlockValue::Int(v) => v.into_pyobject(py)?.into_any(),
        BlockValue::Float(v) => v.into_inner().into_pyobject(py)?.into_any(),
        BlockValue::Str(v) => v.into_pyobject(py)?.into_any(),
        // The typed variants become their textual form.
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

/// A [`PdfBlock`] as a Python object.
pub fn pdf_block_to_py<'py>(py: Python<'py>, block: &PdfBlock) -> PyResult<Bound<'py, PyAny>> {
    Ok(Bound::new(py, crate::python::core::PyPdfBlock::from_native(py, block)?)?.into_any())
}

/// A [`TextBlock`] as a Python object, for passing to an author's pipe.
pub fn text_block_to_py<'py>(py: Python<'py>, block: &TextBlock) -> PyResult<Bound<'py, PyAny>> {
    Ok(Bound::new(py, crate::python::core::PyTextBlock::from_native(py, block)?)?.into_any())
}

/// The result of an author's deserialize pipe as a list of Python objects, without converting them.
///
/// **Only** lists and tuples are unpacked, not any iterable. A deserialize pipe may legitimately
/// return a `dict` — that is how an author declares a map of promises — and a `dict` in Python is
/// iterable *over its keys*. Treating it as a sequence would take it apart into strings.
fn flatten<'py>(result: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    if result.is_none() {
        return Ok(Vec::new());
    }
    if result.is_instance_of::<PyList>() || result.is_instance_of::<PyTuple>() {
        return result.try_iter()?.collect();
    }
    Ok(vec![result.clone()])
}

/// The result of an author's callable, flattened into a list.
///
/// A pipe may return nothing, a single element, or an iterable; all three are treated alike.
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
        // Not iterable: it is a single block.
        Err(_) => Ok(vec![convert(result)?]),
    }
}

/// Logs the traceback and converts the author's error. No `PyErr` leaves this module.
fn author_error(py: Python<'_>, pipeline: &str, pipe: &str, error: PyErr) -> PipeError {
    let message = error.to_string();
    if error.is_instance_of::<crate::python::core::PageParseFail>(py) {
        // Non-fatal: whoever absorbs this error skips the page and carries on, which loses a result
        // — a warning, not merely informational.
        tracing::warn!(pipeline, pipe, "author pipe could not parse the page: {message}");
        return PipeError::page_parse(pipe, crate::core::page::PageError::ParseFail { message });
    }
    tracing::error!(pipeline, pipe, "author pipe raised: {message}");
    error.print(py);
    PipeError::author(pipeline, pipe, message)
}

/// An extraction pipe written by the format's author.
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

    /// Every call takes the GIL back: across N threads these pipes re-serialise against each other
    /// and only the cost of distributing them would remain.
    fn scales_with_threads(&self) -> bool {
        false
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        // An author's pipe expects the original PyMuPDF dict rather than the native page: that is
        // why a page keeps it.
        let raw = page.raw().ok_or_else(|| {
            PipeError::author(&self.pipeline, &self.name, "the page carries no PyMuPDF dictionary")
        })?;
        Python::attach(|py| {
            let result = self
                .func
                .bind(py)
                .call1((raw.bind(py),))
                .map_err(|e| author_error(py, &self.pipeline, &self.name, e))?;
            let blocks =
                each(&result, pdf_block_from_py).map_err(|e| author_error(py, &self.pipeline, &self.name, e))?;
            // `trace!`, not `debug!`: this pipe runs once per page, in a hot loop.
            tracing::trace!(
                pipeline = %self.pipeline,
                pipe = %self.name,
                block_count = blocks.len(),
                "author pdf_extract pipe produced blocks"
            );
            Ok(blocks)
        })
    }
}

/// A filtering pipe written by the format's author.
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

    /// Every call takes the GIL back: across N threads these pipes re-serialise against each other
    /// and only the cost of distributing them would remain.
    fn scales_with_threads(&self) -> bool {
        false
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        Python::attach(|py| {
            let convert = || -> PyResult<Bound<'_, PyAny>> {
                let py_blocks = PyList::empty(py);
                for block in blocks {
                    py_blocks.append(pdf_block_to_py(py, block)?)?;
                }
                // The filter data is what the author's code receives as its second argument: at the
                // first step of the schedule the target companies, afterwards the accumulated
                // results of the preceding steps. Both as real objects, because that is what author
                // modules do things with — reading a company's name, testing an entity's type,
                // reaching its attributes.
                let py_data = PyList::empty(py);
                match data {
                    FilterData::TargetCompanies(companies) => {
                        for company in *companies {
                            py_data.append(Bound::new(
                                py,
                                crate::python::input::PyCompanyMatchInfos::from(company.clone()),
                            )?)?;
                        }
                    }
                    FilterData::Previous(previous) => {
                        for extracted in *previous {
                            py_data.append(crate::python::pipes::extracted_to_py(py, extracted)?)?;
                        }
                    }
                }
                self.func.bind(py).call1((py_blocks, py_data))
            };
            let result = convert().map_err(|e| author_error(py, &self.pipeline, &self.name, e))?;
            let blocks =
                each(&result, text_block_from_py).map_err(|e| author_error(py, &self.pipeline, &self.name, e))?;
            // `trace!`: runs once per page, like the extraction pipe.
            tracing::trace!(
                pipeline = %self.pipeline,
                pipe = %self.name,
                block_count = blocks.len(),
                "author text_filter pipe produced blocks"
            );
            Ok(blocks)
        })
    }
}

/// A deserialization pipe written by the format's author.
///
/// It may return the real output entities, or a **map of promises** — an id-to-value mapping —
/// which is how an author declares a value to be resolved later in the schedule. A result of any
/// other shape is an explicit error rather than something silently dropped: a pipe that produced
/// *something* and had it discarded without a word is the hardest kind of bug to find in a format.
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

    /// Every call takes the GIL back: across N threads these pipes re-serialise against each other
    /// and only the cost of distributing them would remain.
    fn scales_with_threads(&self) -> bool {
        false
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        Python::attach(|py| {
            let call = || -> PyResult<Vec<Extracted>> {
                let py_block = text_block_to_py(py, block)?;
                let result = self.func.bind(py).call1((py_block,))?;
                let mut out = Vec::new();
                for item in flatten(&result)? {
                    if item.is_none() {
                        continue;
                    }
                    // A real entity is what an author's pipe returns nearly always.
                    if let Some(extracted) = crate::python::pipes::extracted_from_py(&item)? {
                        out.push(extracted);
                        continue;
                    }
                    // Otherwise the map-of-promises shape, which is how an author declares a value
                    // to be resolved later in the schedule.
                    let BlockValue::Map(map) = block_value_from_py(&item)? else {
                        return Err(pyo3::exceptions::PyTypeError::new_err(
                            "an author deserialize pipe must return an output entity, a promise mapping, or None",
                        ));
                    };
                    let mut entries = PromiseEntries::new();
                    for (id, value) in map {
                        entries.push(id, value);
                    }
                    out.push(Extracted::Promises(entries));
                }
                Ok(out)
            };
            let extracted = call().map_err(|e| author_error(py, &self.pipeline, &self.name, e))?;
            // `trace!`: runs once per block, more often still than the per-page dispatch of the
            // other two segments.
            tracing::trace!(
                pipeline = %self.pipeline,
                pipe = %self.name,
                extracted_count = extracted.len(),
                "author deserialize pipe produced results"
            );
            Ok(extracted)
        })
    }
}
