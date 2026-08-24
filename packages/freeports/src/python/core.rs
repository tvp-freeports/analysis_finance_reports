//! Shim di `core::classes` e `core::promise`: `PdfBlock`, `TextBlock`, `Promise`.
//!
//! Sono i tre tipi che il codice d'autore di un repo formati costruisce e riceve a ogni pipe,
//! quindi devono avere in Python esattamente il protocollo che il riferimento aveva: attributi
//! `type_block`/`metadata`/`content` (più `pdf_block` sul solo `TextBlock`), uguaglianza, hash —
//! `freeports_dev` confronta i risultati con `frozenset(...) == frozenset(...)` — e un `repr`
//! leggibile, perché `freeports-dev inspect-page` stampa i blocchi uno per uno.
//!
//! `metadata` e `content` sono esposti come oggetti Python nativi (dict/list/set/scalari), non
//! come `BlockValue` opachi: è ciò che il codice d'autore già indicizza e confronta.

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
    /// `strict`/`multiple` restano opzionali perché nel riferimento la forma normale è
    /// `Promise("id")` con i due flag dedotti dai suffissi `!`/`[]` del testo — logica che vive
    /// in [`Promise::new`] e che questo shim non duplica.
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

/// Shim Python di [`PdfBlock`].
///
/// # Perché i metadati sono un `dict` Python e non la mappa nativa
///
/// Il codice d'autore **muta i metadati in place**: `txt_blk.metadata["fund"] = ...`
/// (`anima_sicav_en24.py`, `kairos_en23.py`). Se il getter costruisse un dizionario nuovo a ogni
/// lettura, quella riga scriverebbe su un oggetto usa-e-getta e la modifica sparirebbe senza un
/// errore — il modo peggiore di divergere. Il dizionario è quindi **il** contenitore dei
/// metadati, creato una volta alla costruzione e restituito sempre lo stesso, esattamente come
/// nel riferimento (che teneva un `Py<PyDict>`). La mappa nativa si ricava da lì quando serve,
/// con [`PyPdfBlock::native`].
#[pyclass(name = "PdfBlock", module = "freeports.core", frozen)]
pub struct PyPdfBlock {
    type_block: BlockType,
    metadata: Py<PyDict>,
    content: BlockValue,
}

impl PyPdfBlock {
    /// Lo shim di un blocco nativo. Copia i metadati in un `dict` Python vivo.
    pub fn from_native(py: Python<'_>, block: &PdfBlock) -> PyResult<Self> {
        Ok(PyPdfBlock {
            type_block: block.type_block.clone(),
            metadata: metadata_to_py(py, &block.metadata)?.unbind(),
            content: block.content.clone(),
        })
    }

    /// Il blocco nativo corrispondente allo stato **attuale** dello shim, metadati inclusi.
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
    /// L'ordine degli argomenti è `(type_block, metadata, content)`, quello del riferimento: è
    /// come lo scrive il codice d'autore, sempre posizionale
    /// (`PdfBlock(OnePdfBlockType.RELEVANT_BLOCK.name, {}, text)`).
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
                _ => false,
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

/// Il `dict` vivo dei metadati di un blocco, a partire dall'argomento del costruttore.
fn metadata_dict<'py>(py: Python<'py>, value: Option<&Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyDict>> {
    metadata_to_py(py, &metadata_from_py(value)?)
}

/// Shim Python di [`TextBlock`].
///
/// Non è `frozen`: il codice d'autore riscrive il contenuto di un blocco già costruito
/// (`txt_blk.content = fund_remove_regex.sub("", txt_blk.content)` in `anima_sicav_en24.py`, lo
/// stesso in `kairos_en23.py`). Uguaglianza e hash sono perciò scritti a mano invece che derivati
/// — PyO3 concede `hash` solo a una classe `frozen` — con l'avvertenza consueta di un oggetto
/// mutabile e hashabile: mutarlo mentre è dentro un set lo rende irreperibile. È esattamente la
/// situazione del riferimento, dove `TextBlock` era una classe Python normale con gli stessi
/// setter.
///
/// I metadati sono un `dict` Python vivo per la stessa ragione di [`PyPdfBlock`]: vedi il suo
/// doc-comment.
#[pyclass(name = "TextBlock", module = "freeports.core")]
pub struct PyTextBlock {
    type_block: BlockType,
    metadata: Py<PyDict>,
    content: BlockValue,
    pdf_block: Option<Py<PyPdfBlock>>,
}

impl PyTextBlock {
    /// Lo shim di un blocco nativo.
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

    /// Il blocco nativo corrispondente allo stato **attuale** dello shim.
    pub fn native(&self, py: Python<'_>) -> PyResult<TextBlock> {
        let metadata = metadata_from_py(Some(self.metadata.bind(py).as_any()))?;
        Ok(match &self.pdf_block {
            Some(pdf_block) => {
                let mut block =
                    TextBlock::new(self.type_block.clone(), metadata, pdf_block.bind(py).borrow().native(py)?);
                // `TextBlock::new` eredita il contenuto dal blocco PDF; se l'autore lo ha
                // riscritto dopo, vince la riscrittura.
                block.content = self.content.clone();
                block
            }
            None => TextBlock::from_content(self.type_block.clone(), metadata, self.content.clone()),
        })
    }
}

#[pymethods]
impl PyTextBlock {
    /// `TextBlock(type_block, metadata, pdf_block)` — la firma del riferimento: il contenuto
    /// **si eredita** dal blocco PDF di provenienza. Per costruirne uno senza blocco PDF c'è
    /// [`Self::from_content`], di nuovo come nel riferimento: due costruttori distinti, perché un
    /// `content` e un `pdf_block` potrebbero contraddirsi.
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

    /// `TextBlock.from_content(type_block, metadata, content)` — un blocco di testo che non viene
    /// da nessun blocco PDF.
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
                _ => false,
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

/// Un `BlockValue` in notazione Python, per i `__repr__` qui sopra.
///
/// Non passa da `block_value_to_py` + `repr()` perché un `__repr__` non deve poter fallire né
/// aver bisogno di costruire oggetti Python solo per stamparli.
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

/// Shim Python di una pipeline d'autore.
///
/// # Perché è un contenitore e non un motore
///
/// Nel riferimento `Pipeline` sapeva anche *eseguirsi* (`__call__`), perché il motore era in
/// Python e passava di lì. Qui il motore è [`crate::core::pipeline::Pipeline`], e questo shim
/// serve a una cosa sola: essere il valore che un modulo d'autore mette in `pipelines`, da cui
/// `formats_repo::unstructured::loader` legge i tre segmenti. Il loader li legge **per forma** —
/// gli attributi `pdf_extract`/`text_filter`/`deserialize`, ciascuno un callable o un iterabile di
/// callable — quindi il contratto che questo tipo deve soddisfare è esattamente quello, ed è
/// tutto quello che espone.
///
/// Ogni segmento è normalizzato a lista in costruzione: `None` diventa lista vuota, un callable
/// solo diventa una lista di uno, un iterabile viene materializzato. Così il loader non deve
/// distinguere i tre casi e l'ordine dei pipe è quello scritto dall'autore — il riferimento
/// teneva i pipe in un `set`, e l'ordine ci si perdeva.
#[pyclass(name = "Pipeline", module = "freeports.core", frozen)]
pub struct PyPipeline {
    pdf_extract: Py<pyo3::types::PyList>,
    text_filter: Py<pyo3::types::PyList>,
    deserialize: Py<pyo3::types::PyList>,
}

/// Un segmento come lista: `None` -> vuota, un callable -> uno, un iterabile -> tutti.
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
        Ok(PyPipeline {
            pdf_extract: segment_list(py, "pdf_extract", pdf_extract)?.unbind(),
            text_filter: segment_list(py, "text_filter", text_filter)?.unbind(),
            deserialize: segment_list(py, "deserialize", deserialize)?.unbind(),
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

    /// Vero quando tutti e tre i segmenti hanno almeno un pipe — il `complete` del riferimento.
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
