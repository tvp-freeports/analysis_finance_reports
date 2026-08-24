//! I tre involucri che portano un pipe nativo in Python, e le conversioni che li alimentano.
//!
//! # Perché tre involucri e non ventuno classi
//!
//! I ventuno pipe standard del riferimento sono classi il cui unico scopo è essere costruite e
//! poi chiamate. In Rust sono `Arc<dyn PdfExtractPipe>` (o `TextFilterPipe`/`DeserializePipe`),
//! cioè un solo tipo per segmento: qui basta quindi **un** `#[pyclass]` per segmento, e i ventuno
//! nomi pubblici diventano funzioni che lo costruiscono (`super::standard_funcs`). Da Python la
//! differenza non si vede — `PdfExtractCurrencyStandard(sel)` costruisce un oggetto chiamabile in
//! entrambi i casi — e in cambio il layer non duplica ventuno volte lo stesso involucro.
//!
//! # Perché contano per il caricamento di un repo formati
//!
//! `formats_repo::unstructured::loader` legge il valore `pipelines` di un modulo d'autore e
//! avvolge ogni callable Python in un adattatore (`unstructured::py_pipe`). Se un pipe **nativo**
//! passasse di lì, farebbe un giro Rust → Python → Rust a ogni blocco, con una conversione
//! duck-typed in mezzo che perde le varianti tipizzate di `BlockValue`. Questi involucri esistono
//! perché il loader possa riconoscerli e **spacchettarli**, recuperando l'`Arc` originale invece
//! di riavvolgerlo: vedi [`unwrap_pdf_extract`] e le due sorelle.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::core::classes::{PdfBlock, TextBlock};
use crate::core::page::Page;
use crate::core::pipeline::{
    DeserializePipe, Extracted, FilterData, PdfExtractPipe, PromiseEntries, TextFilterPipe,
};
use crate::formats_utils::text_filter::matcher::CompanyMatchInfos;
use crate::input::document::page_dict::{self, PageDict};

use super::core::{PyPdfBlock, PyTextBlock};

/// Un errore di pipe come `RuntimeError` Python.
fn pipe_error<E: std::fmt::Display>(error: E) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
}

// =================================================================================================
// Gli involucri
// =================================================================================================

/// Un pipe `pdf_extract` nativo, chiamabile da Python.
#[pyclass(name = "PdfExtractPipe", module = "freeports.standard_funcs", frozen)]
#[derive(Clone)]
pub struct PyPdfExtractPipe(Arc<dyn PdfExtractPipe>);

impl PyPdfExtractPipe {
    pub fn new(pipe: Arc<dyn PdfExtractPipe>) -> Self {
        PyPdfExtractPipe(pipe)
    }

    pub fn inner(&self) -> Arc<dyn PdfExtractPipe> {
        Arc::clone(&self.0)
    }
}

#[pymethods]
impl PyPdfExtractPipe {
    /// L'argomento è il dict che PyMuPDF restituisce per una pagina, come nel riferimento.
    fn __call__(&self, page: &Bound<'_, PyAny>) -> PyResult<Vec<PyPdfBlock>> {
        let py = page.py();
        let page = page_from_py(page)?;
        let blocks = self.0.extract(&page).map_err(pipe_error)?;
        blocks.iter().map(|block| PyPdfBlock::from_native(py, block)).collect()
    }

    #[getter]
    fn name(&self) -> String {
        self.0.name().to_string()
    }

    fn __repr__(&self) -> String {
        format!("<pdf_extract pipe {:?}>", self.0.name())
    }
}

/// Un pipe `text_filter` nativo, chiamabile da Python.
#[pyclass(name = "TextFilterPipe", module = "freeports.standard_funcs", frozen)]
#[derive(Clone)]
pub struct PyTextFilterPipe(Arc<dyn TextFilterPipe>);

impl PyTextFilterPipe {
    pub fn new(pipe: Arc<dyn TextFilterPipe>) -> Self {
        PyTextFilterPipe(pipe)
    }

    pub fn inner(&self) -> Arc<dyn TextFilterPipe> {
        Arc::clone(&self.0)
    }
}

#[pymethods]
impl PyTextFilterPipe {
    fn __call__(
        &self,
        pdf_blks: &Bound<'_, PyAny>,
        filter_data: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<PyTextBlock>> {
        let py = pdf_blks.py();
        let blocks = pdf_blocks_from_py(pdf_blks)?;
        let companies = target_companies_from_py(filter_data)?;
        let previous = previous_results_from_py(filter_data)?;
        let data = filter_data_of(&companies, &previous);
        let out = self.0.filter(&blocks, &data).map_err(pipe_error)?;
        out.iter().map(|block| PyTextBlock::from_native(py, block)).collect()
    }

    #[getter]
    fn name(&self) -> String {
        self.0.name().to_string()
    }

    fn __repr__(&self) -> String {
        format!("<text_filter pipe {:?}>", self.0.name())
    }
}

/// Un pipe `deserialize` nativo, chiamabile da Python.
#[pyclass(name = "DeserializePipe", module = "freeports.standard_funcs", frozen)]
#[derive(Clone)]
pub struct PyDeserializePipe(Arc<dyn DeserializePipe>);

impl PyDeserializePipe {
    pub fn new(pipe: Arc<dyn DeserializePipe>) -> Self {
        PyDeserializePipe(pipe)
    }

    pub fn inner(&self) -> Arc<dyn DeserializePipe> {
        Arc::clone(&self.0)
    }
}

#[pymethods]
impl PyDeserializePipe {
    /// Restituisce **un** risultato o `None`, non una lista: è la forma del riferimento, e il
    /// codice d'autore ci conta (`blk = std(txt_blk); if blk is not None: ...`). Un pipe che
    /// producesse più risultati da un solo blocco non esiste fra quelli standard; se mai
    /// esistesse, qui tornerebbe il primo, che è ciò che faceva anche il riferimento.
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        txt_blk: PyRef<'_, PyTextBlock>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let results = self.0.deserialize(&txt_blk.native(py)?).map_err(pipe_error)?;
        match results.into_iter().next() {
            Some(extracted) => extracted_to_py(py, &extracted),
            None => Ok(py.None().into_bound(py)),
        }
    }

    #[getter]
    fn name(&self) -> String {
        self.0.name().to_string()
    }

    fn __repr__(&self) -> String {
        format!("<deserialize pipe {:?}>", self.0.name())
    }
}

// =================================================================================================
// Spacchettamento, per `unstructured::loader`
// =================================================================================================

/// L'`Arc` originale, se l'oggetto Python è uno degli involucri qui sopra.
///
/// `None` non è un errore: significa "questo è un callable d'autore", e il chiamante lo avvolgerà
/// nell'adattatore duck-typed come ha sempre fatto.
pub fn unwrap_pdf_extract(object: &Bound<'_, PyAny>) -> Option<Arc<dyn PdfExtractPipe>> {
    object.extract::<PyRef<'_, PyPdfExtractPipe>>().ok().map(|pipe| pipe.inner())
}

/// Come [`unwrap_pdf_extract`], per il segmento `text_filter`.
pub fn unwrap_text_filter(object: &Bound<'_, PyAny>) -> Option<Arc<dyn TextFilterPipe>> {
    object.extract::<PyRef<'_, PyTextFilterPipe>>().ok().map(|pipe| pipe.inner())
}

/// Come [`unwrap_pdf_extract`], per il segmento `deserialize`.
pub fn unwrap_deserialize(object: &Bound<'_, PyAny>) -> Option<Arc<dyn DeserializePipe>> {
    object.extract::<PyRef<'_, PyDeserializePipe>>().ok().map(|pipe| pipe.inner())
}

// =================================================================================================
// Conversioni
// =================================================================================================

/// Una [`Page`] dal dict che PyMuPDF restituisce.
///
/// Il dict originale resta allegato (`with_raw`): i pipe d'autore possono leggerlo, ed è la
/// ragione per cui `Page` lo conserva.
pub fn page_from_py(page: &Bound<'_, PyAny>) -> PyResult<Page> {
    let dict = page.cast::<PyDict>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("a page must be the dict returned by page.get_text(\"dict\")")
    })?;
    let parsed = PageDict::from_py(dict)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let lines = page_dict::pdflines_from_pagedict(&parsed, true);
    let images = page_dict::pdfimages_from_pagedict(&parsed);
    Ok(Page::new(1, (parsed.width, parsed.height), lines, images).with_raw(page.clone().unbind()))
}

/// Una lista Python di `PdfBlock` come vettore nativo.
pub fn pdf_blocks_from_py(blocks: &Bound<'_, PyAny>) -> PyResult<Vec<PdfBlock>> {
    blocks
        .try_iter()?
        .map(|item| {
            let item = item?;
            item.extract::<PyRef<'_, PyPdfBlock>>()?.native(item.py())
        })
        .collect()
}

/// Una lista Python di `TextBlock` come vettore nativo.
pub fn text_blocks_from_py(blocks: &Bound<'_, PyAny>) -> PyResult<Vec<TextBlock>> {
    blocks
        .try_iter()?
        .map(|item| {
            let item = item?;
            item.extract::<PyRef<'_, PyTextBlock>>()?.native(item.py())
        })
        .collect()
}

/// Le target companies contenute in un `filter_data`, se ce ne sono.
pub fn target_companies_from_py(filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<CompanyMatchInfos>> {
    let mut out = Vec::new();
    for item in filter_data.try_iter()? {
        if let Ok(company) = item?.extract::<PyRef<'_, super::input::PyCompanyMatchInfos>>() {
            out.push(company.inner().clone());
        }
    }
    Ok(out)
}

/// I risultati di step precedenti contenuti in un `filter_data`, se ce ne sono.
pub fn previous_results_from_py(filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Extracted>> {
    let mut out = Vec::new();
    for item in filter_data.try_iter()? {
        if let Some(extracted) = extracted_from_py(&item?)? {
            out.push(extracted);
        }
    }
    Ok(out)
}

/// Il `FilterData` che corrisponde a ciò che il `filter_data` conteneva.
///
/// Le due varianti sono mutuamente esclusive per costruzione (`PLAN.md` §13): un `filter_data`
/// porta le target companies **oppure** l'accumulo degli step precedenti. Se contiene entrambe le
/// cose — cosa che il motore non produce mai, ma un test scritto a mano potrebbe — vincono i
/// risultati precedenti, che sono l'informazione più specifica.
pub fn filter_data_of<'a>(
    companies: &'a [CompanyMatchInfos],
    previous: &'a [Extracted],
) -> FilterData<'a> {
    if previous.is_empty() {
        FilterData::TargetCompanies(companies)
    } else {
        FilterData::Previous(previous)
    }
}

/// Un risultato di pipeline come oggetto Python.
pub fn extracted_to_py<'py>(py: Python<'py>, extracted: &Extracted) -> PyResult<Bound<'py, PyAny>> {
    use super::output::*;
    let object = match extracted {
        Extracted::PageClass(class) => match class {
            Some(class) => class.as_str().into_pyobject(py)?.into_any(),
            None => py.None().into_bound(py),
        },
        Extracted::Promises(entries) => promises_to_py(py, entries)?,
        Extracted::Fund(v) => Bound::new(py, PyFund::from(v.clone()))?.into_any(),
        Extracted::Equity(v) => Bound::new(py, PyEquity::from(v.clone()))?.into_any(),
        Extracted::Bond(v) => Bound::new(py, PyBond::from(v.clone()))?.into_any(),
        Extracted::FundAssets(v) => Bound::new(py, PyFundAssets::from(v.clone()))?.into_any(),
        Extracted::FundSfdrClassification(v) => {
            Bound::new(py, PyFundSfdrClassification::from(v.clone()))?.into_any()
        }
        Extracted::FundEsgIndicator(v) => {
            Bound::new(py, PyFundEsgIndicator::from(v.clone()))?.into_any()
        }
        Extracted::FundRename(v) => Bound::new(py, PyFundRename::from(v.clone()))?.into_any(),
        Extracted::FundMerge(v) => Bound::new(py, PyFundMerge::from(v.clone()))?.into_any(),
        Extracted::ManagementCompany(v) => {
            Bound::new(py, PyManagementCompany::from(v.clone()))?.into_any()
        }
        Extracted::InvestmentsManager(v) => {
            Bound::new(py, PyInvestmentsManager::from(v.clone()))?.into_any()
        }
    };
    Ok(object)
}

/// Le promesse mantenute da un pipe, come `dict` — la forma che il riferimento usava.
fn promises_to_py<'py>(py: Python<'py>, entries: &PromiseEntries) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    for (id, value) in entries.iter() {
        dict.set_item(id, super::convert::block_value_to_py(py, value)?)?;
    }
    Ok(dict.into_any())
}

/// Un oggetto Python come risultato di pipeline, se lo è.
///
/// `Ok(None)` significa "non è un risultato" (una page class, un blocco, un `MatchFund`...), non
/// "conversione fallita": è un ramo di riconoscimento, e i chiamanti saltano ciò che non
/// riconoscono.
pub fn extracted_from_py(object: &Bound<'_, PyAny>) -> PyResult<Option<Extracted>> {
    use super::output::*;
    macro_rules! try_variant {
        ($shim:ty, $variant:ident) => {
            if let Ok(value) = object.extract::<PyRef<'_, $shim>>() {
                return Ok(Some(Extracted::$variant(value.inner().clone())));
            }
        };
    }
    try_variant!(PyFund, Fund);
    try_variant!(PyEquity, Equity);
    try_variant!(PyBond, Bond);
    try_variant!(PyFundAssets, FundAssets);
    try_variant!(PyFundSfdrClassification, FundSfdrClassification);
    try_variant!(PyFundEsgIndicator, FundEsgIndicator);
    try_variant!(PyFundRename, FundRename);
    try_variant!(PyFundMerge, FundMerge);
    try_variant!(PyManagementCompany, ManagementCompany);
    try_variant!(PyInvestmentsManager, InvestmentsManager);

    // Le promesse arrivano come dict `{id: valore}`.
    if let Ok(dict) = object.cast::<PyDict>() {
        let mut entries = PromiseEntries::new();
        for (key, value) in dict.iter() {
            entries.push(key.extract::<String>()?, super::convert::block_value_from_py(&value)?);
        }
        return Ok(Some(Extracted::Promises(entries)));
    }
    let _ = PyList::empty(object.py());
    Ok(None)
}
