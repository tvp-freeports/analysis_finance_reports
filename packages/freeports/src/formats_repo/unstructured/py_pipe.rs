//! Gli adattatori che fanno di un callable Python un pipe come tutti gli altri.
//!
//! È uno dei due soli punti di contatto con Python del crate (`PLAN.md` §3): i pipe definiti
//! dall'autore di un formato implementano gli stessi trait dei pipe nativi, quindi il motore non
//! sa — e non deve sapere — se un pipe è Rust o Python.
//!
//! # Il contratto verso Python
//!
//! Un pipe d'autore riceve e restituisce le **classi vere** di [`crate::python`]: `PdfBlock`,
//! `TextBlock`, `Promise`, le entità di `freeports.output`, gli enum di `freeports.consts`. È ciò
//! che i moduli d'autore importano e costruiscono (`from freeports.core import PdfBlock`), quindi
//! è ciò che devono ricevere.
//!
//! Fino a M9 non era possibile: il crate non esponeva alcuna API Python, e il confine era definito
//! per *forma* invece che per tipo (decisione **D-M7-3**, 2026-08-23) — un `dict` con le chiavi
//! `type_block`/`metadata`/`content` al posto di un `PdfBlock`. Quella forma resta accettata **in
//! entrata**, come rete di sicurezza per un pipe che costruisca i propri risultati a mano, ma non
//! è più ciò che viene passato in uscita: un modulo d'autore che scrive `pdf_blks[0].content` —
//! e sono la maggioranza — su un `dict` andrebbe in `AttributeError`.
//!
//! Con le classi vere sparisce anche il limite che D-M7-3 si portava dietro: le varianti tipizzate
//! di `BlockValue` (`Date`, `Currency`, `SfdrArticle`, `FinancialInstrument`) arrivano ora come i
//! rispettivi shim e non degradate a stringa.
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
    // Le classi vere (`Currency`, `SfdrArticle`, `FinancialInstrument`, `datetime.date`, ...):
    // dopo il ramo promessa, perché quello riconosce anche la *forma* dizionario che il
    // convertitore degli shim vedrebbe come una mappa qualunque; prima dei rami generici, perché
    // uno shim di enum non è né una stringa né una mappa e cadrebbe nell'errore finale.
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
    if let Ok(block) = object.extract::<PyRef<'_, crate::python::core::PyPdfBlock>>() {
        return block.native(object.py());
    }
    let (type_block, metadata, content) = block_parts(object)?;
    Ok(PdfBlock::new(type_block, metadata, content))
}

/// Un `TextBlock` da un oggetto che ne ha la forma. Il `pdf_block` è facoltativo.
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
    Ok(Bound::new(py, crate::python::core::PyPdfBlock::from_native(py, block)?)?.into_any())
}

/// Un `TextBlock` come oggetto Python, per passarlo a un pipe d'autore.
pub fn text_block_to_py<'py>(py: Python<'py>, block: &TextBlock) -> PyResult<Bound<'py, PyAny>> {
    Ok(Bound::new(py, crate::python::core::PyTextBlock::from_native(py, block)?)?.into_any())
}

/// Il risultato di un pipe `deserialize` d'autore come lista di oggetti Python, senza convertirli.
///
/// Sono spacchettate **solo** liste e tuple, non un iterabile qualunque: un pipe `deserialize` può
/// legittimamente restituire un `dict` — è la forma con cui un autore dichiara una mappa di
/// promesse — e un `dict` in Python è iterabile *sulle sue chiavi*. Trattarlo come sequenza lo
/// smonterebbe in stringhe. È la stessa regola del riferimento, che distingue esplicitamente
/// `list`/`tuple` da tutto il resto.
fn flatten<'py>(result: &Bound<'py, PyAny>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    if result.is_none() {
        return Ok(Vec::new());
    }
    if result.is_instance_of::<PyList>() || result.is_instance_of::<PyTuple>() {
        return result.try_iter()?.collect();
    }
    Ok(vec![result.clone()])
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
///
/// `PageParseFail` è l'unica eccezione con un significato concordato: l'autore la solleva per dire
/// "questa pagina non è interpretabile", e nel riferimento l'algoritmo la assorbe saltando la
/// pagina invece di interrompersi. Diventa perciò [`PipeError::PageParse`], che è l'unica variante
/// non fatale; ogni altra eccezione resta un fallimento d'autore.
fn author_error(py: Python<'_>, pipeline: &str, pipe: &str, error: PyErr) -> PipeError {
    let message = error.to_string();
    if error.is_instance_of::<crate::python::core::PageParseFail>(py) {
        tracing::info!(pipeline, pipe, "author pipe could not parse the page: {message}");
        return PipeError::page_parse(pipe, crate::core::page::PageError::ParseFail { message });
    }
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
                // Il `filter_data` è ciò che il codice d'autore riceve come secondo argomento:
                // al primo step dello schedule le società bersaglio, dopo l'accumulo dei
                // risultati degli step precedenti. Entrambi come oggetti veri — le società come
                // `CompanyMatchInfos`, i risultati come le entità di `freeports.output` — perché
                // è ciò che i moduli d'autore ci fanno sopra (`c.name`, `isinstance`, gli
                // attributi delle entità).
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
            let call = || -> PyResult<Vec<Extracted>> {
                let py_block = text_block_to_py(py, block)?;
                let result = self.func.bind(py).call1((py_block,))?;
                let mut out = Vec::new();
                for item in flatten(&result)? {
                    if item.is_none() {
                        continue;
                    }
                    // Un'entità vera (`Fund`, `Equity`, `FundAssets`, ...) è ciò che un pipe
                    // d'autore restituisce quasi sempre.
                    if let Some(extracted) = crate::python::pipes::extracted_from_py(&item)? {
                        out.push(extracted);
                        continue;
                    }
                    // Altrimenti resta la forma "mappa di promesse", che è come un autore
                    // dichiara un valore da risolvere più avanti nello schedule.
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
            call().map_err(|e| author_error(py, &self.pipeline, &self.name, e))
        })
    }
}
