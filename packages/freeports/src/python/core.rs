//! The Python shims of the engine's core types: [`PyPdfBlock`], [`PyTextBlock`], [`PyPromise`].
//!
//! These are the three types a formats repository's author code builds and receives at every pipe,
//! so in Python they must carry exactly the protocol that code expects: the block attributes,
//! equality, hashing — the development tooling compares results as sets — and a readable
//! representation, because inspecting a page prints blocks one by one.
//!
//! The metadata and content are exposed as native Python objects, not as opaque wrappers: that is
//! what author code already indexes and compares.

use std::collections::BTreeMap;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::core::classes::{BlockType, BlockValue, PdfBlock, TextBlock};
use crate::core::promise::Promise;

use super::convert::{block_value_from_py, block_value_to_py, metadata_from_py, metadata_to_py};

/// Shim Python di [`Promise`].
#[pyclass(name = "Promise", module = "freeports.core", frozen, eq, hash)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PyPromise(Promise);

impl From<Promise> for PyPromise {
    fn from(value: Promise) -> Self {
        PyPromise(value)
    }
}

impl PyPromise {
    pub fn inner(&self) -> &Promise {
        &self.0
    }
}

#[pymethods]
impl PyPromise {
    /// The two flags stay optional because the usual form is a bare `Promise("id")` with them
    /// deduced from the text's suffixes — logic that lives in the native constructor and is not
    /// duplicated here.
    #[new]
    #[pyo3(signature = (raw, strict=None, multiple=None))]
    fn new(raw: &str, strict: Option<bool>, multiple: Option<bool>) -> PyPromise {
        match (strict, multiple) {
            (None, None) => PyPromise(Promise::new(raw)),
            (strict, multiple) => PyPromise(Promise::with_flags(
                raw,
                strict.unwrap_or(false),
                multiple.unwrap_or(false),
            )),
        }
    }

    #[getter]
    fn id(&self) -> &str {
        self.0.id()
    }

    #[getter]
    fn strict(&self) -> bool {
        self.0.strict()
    }

    #[getter]
    fn multiple(&self) -> bool {
        self.0.multiple()
    }

    fn __repr__(&self) -> String {
        format!(
            "Promise(id={:?}, strict={}, multiple={})",
            self.0.id(),
            if self.0.strict() { "True" } else { "False" },
            if self.0.multiple() { "True" } else { "False" }
        )
    }
}

/// The Python shim of a PDF block.
///
/// # Why the metadata is a live Python dict, not the native map
///
/// Author code **mutates the metadata in place**: `block.metadata["fund"] = …`. Were the getter to
/// build a fresh dictionary on each read, that line would write into a throwaway object and the
/// change would vanish without an error — the worst way to diverge. The dictionary is therefore
/// **the** container of the metadata, created once at construction and always handed back the same.
/// The native map is derived from it when needed.
#[pyclass(name = "PdfBlock", module = "freeports.core", frozen)]
pub struct PyPdfBlock {
    type_block: BlockType,
    metadata: Py<PyDict>,
    content: BlockValue,
}

impl PyPdfBlock {
    /// The shim of a native block. Copies the metadata into a live Python dict.
    pub fn from_native(py: Python<'_>, block: &PdfBlock) -> PyResult<Self> {
        Ok(PyPdfBlock {
            type_block: block.type_block.clone(),
            metadata: metadata_to_py(py, &block.metadata)?.unbind(),
            content: block.content.clone(),
        })
    }

    /// The native block corresponding to the shim's **current** state, metadata included.
    pub fn native(&self, py: Python<'_>) -> PyResult<PdfBlock> {
        Ok(PdfBlock::new(
            self.type_block.clone(),
            metadata_from_py(Some(self.metadata.bind(py).as_any()))?,
            self.content.clone(),
        ))
    }
}

#[pymethods]
impl PyPdfBlock {
    /// The argument order is `(type_block, metadata, content)`: the order author code actually
    /// writes, always positionally.
    #[new]
    #[pyo3(signature = (type_block, metadata=None, content=None))]
    fn new(
        py: Python<'_>,
        type_block: &str,
        metadata: Option<&Bound<'_, PyAny>>,
        content: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyPdfBlock> {
        let content = match content {
            Some(value) => block_value_from_py(value)?,
            None => BlockValue::Null,
        };
        Ok(PyPdfBlock {
            type_block: BlockType::new(type_block.to_string()),
            metadata: metadata_dict(py, metadata)?.unbind(),
            content,
        })
    }

    #[getter]
    fn type_block(&self) -> &str {
        self.type_block.as_str()
    }

    #[getter]
    fn metadata(&self, py: Python<'_>) -> Py<PyDict> {
        self.metadata.clone_ref(py)
    }

    #[setter]
    fn set_metadata(&self, py: Python<'_>, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let replacement = metadata_dict(py, value)?;
        let current = self.metadata.bind(py);
        current.clear();
        current.update(replacement.as_mapping())?;
        Ok(())
    }

    #[getter]
    fn content<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        block_value_to_py(py, &self.content)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        let py = other.py();
        match other.extract::<PyRef<'_, PyPdfBlock>>() {
            Ok(other) => match (self.native(py), other.native(py)) {
                (Ok(a), Ok(b)) => a == b,
                _ => {
                    // Equality cannot return a failure, so a metadata value that will not convert
                    // is absorbed here into "not equal" rather than travelling up as an error — and
                    // is therefore logged before being absorbed.
                    tracing::warn!(
                        "PdfBlock.__eq__ could not convert metadata to native form; treating as not equal"
                    );
                    false
                }
            },
            Err(_) => false,
        }
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<u64> {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.native(py)?.hash(&mut hasher);
        Ok(hasher.finish())
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let native = self.native(py)?;
        Ok(format!(
            "PdfBlock(type_block={:?}, metadata={}, content={})",
            native.type_block.as_str(),
            render_metadata(&native.metadata),
            render_value(&native.content)
        ))
    }
}

/// The live metadata dict of a block, from the constructor's argument.
fn metadata_dict<'py>(py: Python<'py>, value: Option<&Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyDict>> {
    metadata_to_py(py, &metadata_from_py(value)?)
}

/// The Python shim of a text block.
///
/// Not frozen: author code rewrites the content of an already-built block. Equality and hashing are
/// therefore written by hand rather than derived — PyO3 grants hashing only to a frozen class —
/// with the usual caveat of a mutable hashable object: mutating one while it sits in a set makes it
/// unfindable.
///
/// The metadata is a live Python dict for the same reason as [`PyPdfBlock`]; see its documentation.
#[pyclass(name = "TextBlock", module = "freeports.core")]
pub struct PyTextBlock {
    type_block: BlockType,
    metadata: Py<PyDict>,
    content: BlockValue,
    pdf_block: Option<Py<PyPdfBlock>>,
}

impl PyTextBlock {
    /// The shim of a native block.
    pub fn from_native(py: Python<'_>, block: &TextBlock) -> PyResult<Self> {
        let pdf_block = match &block.pdf_block {
            Some(pdf_block) => Some(Py::new(py, PyPdfBlock::from_native(py, pdf_block)?)?),
            None => None,
        };
        Ok(PyTextBlock {
            type_block: block.type_block.clone(),
            metadata: metadata_to_py(py, &block.metadata)?.unbind(),
            content: block.content.clone(),
            pdf_block,
        })
    }

    /// The native block corresponding to the shim's **current** state.
    pub fn native(&self, py: Python<'_>) -> PyResult<TextBlock> {
        let metadata = metadata_from_py(Some(self.metadata.bind(py).as_any()))?;
        Ok(match &self.pdf_block {
            Some(pdf_block) => {
                let mut block =
                    TextBlock::new(self.type_block.clone(), metadata, pdf_block.bind(py).borrow().native(py)?);
                // The native constructor inherits the content from the PDF block; if the author
                // rewrote it afterwards, the rewrite wins.
                block.content = self.content.clone();
                block
            }
            None => TextBlock::from_content(self.type_block.clone(), metadata, self.content.clone()),
        })
    }
}

#[pymethods]
impl PyTextBlock {
    /// `TextBlock(type_block, metadata, pdf_block)`: the content **is inherited** from the
    /// originating PDF block. To build one without a PDF block there is [`Self::from_content`] —
    /// two distinct constructors, because a content and a PDF block could contradict each other.
    #[new]
    #[pyo3(signature = (type_block, metadata=None, pdf_block=None))]
    fn new(
        py: Python<'_>,
        type_block: &str,
        metadata: Option<&Bound<'_, PyAny>>,
        pdf_block: Option<Py<PyPdfBlock>>,
    ) -> PyResult<PyTextBlock> {
        let content = match &pdf_block {
            Some(block) => block.bind(py).borrow().content.clone(),
            None => BlockValue::Null,
        };
        Ok(PyTextBlock {
            type_block: BlockType::new(type_block.to_string()),
            metadata: metadata_dict(py, metadata)?.unbind(),
            content,
            pdf_block,
        })
    }

    /// A text block that comes from no PDF block.
    #[staticmethod]
    #[pyo3(signature = (type_block, metadata=None, content=None))]
    fn from_content(
        py: Python<'_>,
        type_block: &str,
        metadata: Option<&Bound<'_, PyAny>>,
        content: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyTextBlock> {
        let content = match content {
            Some(value) => block_value_from_py(value)?,
            None => BlockValue::Null,
        };
        Ok(PyTextBlock {
            type_block: BlockType::new(type_block.to_string()),
            metadata: metadata_dict(py, metadata)?.unbind(),
            content,
            pdf_block: None,
        })
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        let py = other.py();
        match other.extract::<PyRef<'_, PyTextBlock>>() {
            Ok(other) => match (self.native(py), other.native(py)) {
                (Ok(a), Ok(b)) => a == b,
                _ => {
                    // Same reason as for a PDF block: equality cannot propagate the conversion
                    // error, so it is logged before being absorbed.
                    tracing::warn!(
                        "TextBlock.__eq__ could not convert metadata to native form; treating as not equal"
                    );
                    false
                }
            },
            Err(_) => false,
        }
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<u64> {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.native(py)?.hash(&mut hasher);
        Ok(hasher.finish())
    }

    #[getter]
    fn type_block(&self) -> &str {
        self.type_block.as_str()
    }

    #[getter]
    fn metadata(&self, py: Python<'_>) -> Py<PyDict> {
        self.metadata.clone_ref(py)
    }

    #[setter]
    fn set_metadata(&mut self, py: Python<'_>, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.metadata = metadata_dict(py, value)?.unbind();
        Ok(())
    }

    #[getter]
    fn content<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        block_value_to_py(py, &self.content)
    }

    #[setter]
    fn set_content(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.content = block_value_from_py(value)?;
        Ok(())
    }

    #[getter]
    fn pdf_block(&self, py: Python<'_>) -> Option<Py<PyPdfBlock>> {
        self.pdf_block.as_ref().map(|block| block.clone_ref(py))
    }

    /// Attaching the PDF block **after** construction is the only way to have a chosen content and
    /// a provenance together: the ordinary constructor inherits the content, and whoever rebuilds
    /// an already-serialized block needs the two separately, the content possibly having been
    /// rewritten.
    #[setter]
    fn set_pdf_block(&mut self, pdf_block: Option<Py<PyPdfBlock>>) {
        self.pdf_block = pdf_block;
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let native = self.native(py)?;
        Ok(format!(
            "TextBlock(type_block={:?}, metadata={}, content={})",
            native.type_block.as_str(),
            render_metadata(&native.metadata),
            render_value(&native.content)
        ))
    }
}

/// A block value in Python notation, for the representations above.
///
/// It does not go through a conversion and a `repr()` because a representation must not be able to
/// fail, nor need to build Python objects merely to print them.
fn render_value(value: &BlockValue) -> String {
    match value {
        BlockValue::Null => "None".to_string(),
        BlockValue::Bool(v) => if *v { "True" } else { "False" }.to_string(),
        BlockValue::Int(v) => v.to_string(),
        BlockValue::Float(v) => v.into_inner().to_string(),
        BlockValue::Str(v) => format!("{v:?}"),
        BlockValue::Date(v) => format!("date({v})"),
        BlockValue::Currency(v) => format!("Currency.{}", v.code()),
        BlockValue::SfdrArticle(v) => {
            format!("SfdrArticle.{}", super::consts::PySfdrArticle::from(*v).variant_name_of())
        }
        BlockValue::FinancialInstrument(v) => format!("FinancialInstrument.{v:?}"),
        BlockValue::Promise(v) => format!("Promise({:?})", v.id()),
        BlockValue::List(items) => {
            format!("[{}]", items.iter().map(render_value).collect::<Vec<_>>().join(", "))
        }
        BlockValue::Set(items) => {
            format!("{{{}}}", items.iter().map(render_value).collect::<Vec<_>>().join(", "))
        }
        BlockValue::Map(map) => render_metadata(map),
    }
}

/// Una mappa di metadati in notazione Python.
fn render_metadata(map: &BTreeMap<String, BlockValue>) -> String {
    format!(
        "{{{}}}",
        map.iter()
            .map(|(key, value)| format!("{key:?}: {}", render_value(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

// =================================================================================================
// `PageParseFail` e `Pipeline`
// =================================================================================================

pyo3::create_exception!(
    freeports.core,
    PageParseFail,
    pyo3::exceptions::PyException,
    "Sollevata da un pipe d'autore quando la pagina non è interpretabile."
);

/// The Python shim of an author's pipeline.
///
/// # A container, not an engine
///
/// This shim does one thing: be the value an author module puts in its pipelines, from which the
/// loader reads the three segments. The loader reads them **by shape** — three attributes, each a
/// callable or an iterable of callables — so that is exactly the contract this type must satisfy,
/// and all it exposes.
///
/// Each segment is normalised to a list at construction: nothing becomes an empty list, a single
/// callable a list of one, an iterable is materialised. The loader then need not tell the three
/// cases apart, and the order of the pipes is the one the author wrote.
#[pyclass(name = "Pipeline", module = "freeports.core", frozen)]
pub struct PyPipeline {
    pdf_extract: Py<pyo3::types::PyList>,
    text_filter: Py<pyo3::types::PyList>,
    deserialize: Py<pyo3::types::PyList>,
}

/// A segment as a list: nothing becomes empty, a callable becomes one, an iterable becomes all of
/// them.
fn segment_list<'py>(
    py: Python<'py>,
    name: &str,
    value: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, pyo3::types::PyList>> {
    let list = pyo3::types::PyList::empty(py);
    let Some(value) = value.filter(|value| !value.is_none()) else { return Ok(list) };
    if value.is_callable() {
        list.append(value)?;
        return Ok(list);
    }
    let items = value.try_iter().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(format!("{name} must be a pipe or an iterable of pipes"))
    })?;
    for item in items {
        let item = item?;
        if !item.is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!("every pipe of {name} must be callable")));
        }
        list.append(item)?;
    }
    Ok(list)
}

#[pymethods]
impl PyPipeline {
    #[new]
    #[pyo3(signature = (pdf_extract=None, text_filter=None, deserialize=None))]
    fn new(
        py: Python<'_>,
        pdf_extract: Option<&Bound<'_, PyAny>>,
        text_filter: Option<&Bound<'_, PyAny>>,
        deserialize: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let pdf_extract = segment_list(py, "pdf_extract", pdf_extract)?;
        let text_filter = segment_list(py, "text_filter", text_filter)?;
        let deserialize = segment_list(py, "deserialize", deserialize)?;
        Ok(PyPipeline {
            pdf_extract: pdf_extract.unbind(),
            text_filter: text_filter.unbind(),
            deserialize: deserialize.unbind(),
        })
    }

    #[getter]
    fn pdf_extract(&self, py: Python<'_>) -> Py<pyo3::types::PyList> {
        self.pdf_extract.clone_ref(py)
    }

    #[getter]
    fn text_filter(&self, py: Python<'_>) -> Py<pyo3::types::PyList> {
        self.text_filter.clone_ref(py)
    }

    #[getter]
    fn deserialize(&self, py: Python<'_>) -> Py<pyo3::types::PyList> {
        self.deserialize.clone_ref(py)
    }

    /// True when all three segments hold at least one pipe.
    fn complete(&self, py: Python<'_>) -> bool {
        !self.pdf_extract.bind(py).is_empty()
            && !self.text_filter.bind(py).is_empty()
            && !self.deserialize.bind(py).is_empty()
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        format!(
            "Pipeline(pdf_extract={} pipes, text_filter={} pipes, deserialize={} pipes)",
            self.pdf_extract.bind(py).len(),
            self.text_filter.bind(py).len(),
            self.deserialize.bind(py).len(),
        )
    }
}
