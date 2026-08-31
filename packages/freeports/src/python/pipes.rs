//! The three wrappers that carry a native pipe into Python, and the conversions feeding them.
//!
//! # Why three wrappers and not twenty-one classes
//!
//! The twenty-one standard pipes are classes whose only purpose is to be built and then called. In
//! Rust they are one trait object per segment, so one Python class per segment suffices, and the
//! twenty-one public names become functions that build it. From Python the difference is invisible,
//! and in exchange the layer does not duplicate the same wrapper twenty-one times.
//!
//! # Why they matter to loading a formats repository
//!
//! The loader reads an author module's pipelines and wraps every Python callable in an adapter.
//! Were a **native** pipe to go through that, it would make a Rust-to-Python-and-back trip for
//! every block, with a shape-based conversion in between that loses the typed variants of a value.
//! These wrappers exist so the loader can recognise them and **unwrap** them, recovering the
//! original pipe instead of re-wrapping it.

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
use crate::core::tracing_setup::log_error;

/// A pipe error as a Python `RuntimeError`.
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
    /// The argument is the dict PyMuPDF returns for a page.
    fn __call__(&self, page: &Bound<'_, PyAny>) -> PyResult<Vec<PyPdfBlock>> {
        let py = page.py();
        let page = page_from_py(page)?;
        let blocks = self.0.extract(&page).map_err(|e| {
            // Called directly from Python (not through `core::algorithm`'s engine, which never
            // goes through this wrapper's `__call__`), so this is the only place a failure of
            // this specific invocation can ever be recorded before it becomes a Python
            // `RuntimeError`, invisible to this crate's own tracing/CSV pipeline.
            tracing::error!(error = log_error(&e), pipe = self.0.name(), "native pdf_extract pipe called from Python failed: {e}");
            pipe_error(e)
        })?;
        // `trace!`, not `debug!`: mirrors `formats_repo::unstructured::py_pipe`'s own pipes,
        // which run once per page (rule 2, hot per-page dispatch).
        tracing::trace!(pipe = self.0.name(), block_count = blocks.len(), "native pdf_extract pipe called from Python");
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
        let out = self.0.filter(&blocks, &data).map_err(|e| {
            tracing::error!(error = log_error(&e), pipe = self.0.name(), "native text_filter pipe called from Python failed: {e}");
            pipe_error(e)
        })?;
        tracing::trace!(pipe = self.0.name(), block_count = out.len(), "native text_filter pipe called from Python");
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
    /// Returns **one** result or nothing, not a list: it is the form author code counts on. No
    /// standard pipe produces several results from one block; were one to, this would return the
    /// first.
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        txt_blk: PyRef<'_, PyTextBlock>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let results = self.0.deserialize(&txt_blk.native(py)?).map_err(|e| {
            tracing::error!(error = log_error(&e), pipe = self.0.name(), "native deserialize pipe called from Python failed: {e}");
            pipe_error(e)
        })?;
        // `trace!`: runs once per text block, the finest granularity of the three (rule 2).
        tracing::trace!(pipe = self.0.name(), result_count = results.len(), "native deserialize pipe called from Python");
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

/// The original pipe, if the Python object is one of the wrappers above.
///
/// Returning nothing is not an error: it means "this is an author's callable", and the caller will
/// wrap it in the shape-based adapter as always.
pub fn unwrap_pdf_extract(object: &Bound<'_, PyAny>) -> Option<Arc<dyn PdfExtractPipe>> {
    object.extract::<PyRef<'_, PyPdfExtractPipe>>().ok().map(|pipe| pipe.inner())
}

/// As [`unwrap_pdf_extract`], for the filtering segment.
pub fn unwrap_text_filter(object: &Bound<'_, PyAny>) -> Option<Arc<dyn TextFilterPipe>> {
    object.extract::<PyRef<'_, PyTextFilterPipe>>().ok().map(|pipe| pipe.inner())
}

/// As [`unwrap_pdf_extract`], for the deserialization segment.
pub fn unwrap_deserialize(object: &Bound<'_, PyAny>) -> Option<Arc<dyn DeserializePipe>> {
    object.extract::<PyRef<'_, PyDeserializePipe>>().ok().map(|pipe| pipe.inner())
}

// =================================================================================================
// Conversioni
// =================================================================================================

/// A page from the dict PyMuPDF returns.
///
/// The original dict stays attached: author pipes can read it, which is why a page keeps it.
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

/// A Python list of PDF blocks as a native vector.
pub fn pdf_blocks_from_py(blocks: &Bound<'_, PyAny>) -> PyResult<Vec<PdfBlock>> {
    blocks
        .try_iter()?
        .map(|item| {
            let item = item?;
            item.extract::<PyRef<'_, PyPdfBlock>>()?.native(item.py())
        })
        .collect()
}

/// A Python list of text blocks as a native vector.
pub fn text_blocks_from_py(blocks: &Bound<'_, PyAny>) -> PyResult<Vec<TextBlock>> {
    blocks
        .try_iter()?
        .map(|item| {
            let item = item?;
            item.extract::<PyRef<'_, PyTextBlock>>()?.native(item.py())
        })
        .collect()
}

/// The target companies contained in a filter data, if there are any.
pub fn target_companies_from_py(filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<CompanyMatchInfos>> {
    let mut out = Vec::new();
    for item in filter_data.try_iter()? {
        if let Ok(company) = item?.extract::<PyRef<'_, super::input::PyCompanyMatchInfos>>() {
            out.push(company.inner().clone());
        }
    }
    Ok(out)
}

/// The results of preceding steps contained in a filter data, if there are any.
pub fn previous_results_from_py(filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Extracted>> {
    let mut out = Vec::new();
    for item in filter_data.try_iter()? {
        if let Some(extracted) = extracted_from_py(&item?)? {
            out.push(extracted);
        }
    }
    Ok(out)
}

/// The filter data matching what was passed in.
///
/// The two forms are mutually exclusive by construction: a filter data carries the target companies
/// **or** the accumulated previous results. Should it hold both — something the engine never
/// produces, but a hand-written test might — the previous results win, being the more specific
/// information.
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

/// A pipeline result as a Python object.
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

/// The promises a pipe kept, as a dict.
fn promises_to_py<'py>(py: Python<'py>, entries: &PromiseEntries) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    for (id, value) in entries.iter() {
        dict.set_item(id, super::convert::block_value_to_py(py, value)?)?;
    }
    Ok(dict.into_any())
}

/// A Python object as a pipeline result, if it is one.
///
/// Returning nothing means "this is not a result" — a page class, a block, a fund identity — not
/// "the conversion failed": it is a recognition branch, and callers skip what they do not
/// recognise.
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

    // Promises arrive as a dict of id to value.
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
