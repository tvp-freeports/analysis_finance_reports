//! Rust port of `PdfBlock`/`TextBlock` and their five marker exceptions from
//! `packages/freeports_core/src/freeports/_internals/core/classes.py`.
//!
//! Both classes were blocked from being ported earlier this migration by `TextBlock` being
//! subclassed 3 times in `formats/utils/text_filter/standard_txt_blks.py`
//! (`StandardManagmentCompanyTextBlock`/`StandardInvestmentsMangerTextBlock`/
//! `StandardFundTextBlock`) — a PyO3 pyclass can't be subclassed from pure Python without
//! `#[pyclass(subclass)]`, and even then keeping arbitrary Python subclass state is awkward.
//! Re-examined on explicit user instruction: those three subclasses add no fields and override no
//! behavior — they exist purely as convenience constructors hardcoding a `type_block` string and
//! a metadata shape (verified: no `isinstance()` check anywhere depends on their being distinct
//! Python types, unlike e.g. `FundRename`/`FundMerge`, which at least needed distinguishable
//! identity for `isinstance()` dispatch in `routines.py`). So they're ported as plain functions
//! instead (see `text_filter/standard_txt_blks.rs`), and `TextBlock` itself is a normal,
//! non-subclassable pyclass here — `PdfBlock` was never subclassed anywhere and had no such
//! blocker to begin with.
//!
//! **Deliberately not ported**: `PdfBlock.__str__`/`TextBlock.__str__` (via the shared
//! `_str_blocks` helper) build an i18n-translated (`_()`) string — same reasoning as
//! `Investment.__str__` (`output/investment.rs`): verified unused anywhere outside
//! `classes.py` itself, and translating in Rust would violate the OS/i18n/logging-stays-Python
//! split this migration keeps throughout. `__repr__`/`__str__` here are a plain untranslated
//! equivalent.
//!
//! **A genuinely surprising piece of the original, replicated exactly, not "fixed"**:
//! `__hash__` mutates `self.metadata` in place as a side effect — any `set`/`list`-valued entry
//! is replaced with a `frozenset` (so `frozenset(self.metadata.items())` doesn't blow up on an
//! unhashable value) — and since `__eq__` is defined as `hash(self) == hash(other)`, comparing
//! two blocks with `==` *also* triggers this mutation on both sides. This is preserved exactly,
//! not cleaned up: it's a real (if surprising) behavior of the original, not an accident of this
//! port, and this migration's methodology is to fix bugs it *finds*, not behaviors the original
//! author evidently intended.

use pyo3::create_exception;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFrozenSet, PyList, PySet, PyTuple};

create_exception!(freeports._native, ExpectedPdfBlockNotFound, pyo3::exceptions::PyException, "Raised when a required PdfBlock is not found during processing.");
create_exception!(freeports._native, ExpectedTextBlockNotFound, pyo3::exceptions::PyException, "Raised when a required TextBlock is not found during processing.");
create_exception!(freeports._native, PageParseFail, pyo3::exceptions::PyException, "Raised when the algorithm is unable to parse a page.");
create_exception!(freeports._native, LineParseFail, pyo3::exceptions::PyException, "Raised when the algorithm is unable to parse a line.");
create_exception!(freeports._native, ExtractionFieldFail, pyo3::exceptions::PyException, "Raised when the algorithm is unable to parse a field.");

/// Shared by `PdfBlock`/`TextBlock`: normalizes `metadata` in place (`set`/`list` values become
/// `frozenset`, matching the Python original exactly) and returns
/// `hash((type_block, frozenset(metadata.items()), content))`.
fn normalize_metadata_and_hash(
    py: Python<'_>,
    type_block: &str,
    metadata: &Py<PyDict>,
    content: &Py<PyAny>,
) -> PyResult<isize> {
    let metadata = metadata.bind(py);
    let entries: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = metadata.iter().collect();
    for (k, v) in &entries {
        if v.is_instance_of::<PySet>() || v.is_instance_of::<PyList>() {
            let items: Vec<Bound<'_, PyAny>> = v.try_iter()?.collect::<PyResult<_>>()?;
            metadata.set_item(k, PyFrozenSet::new(py, items)?)?;
        }
    }
    let mut item_tuples = Vec::with_capacity(metadata.len());
    for (k, v) in metadata.iter() {
        item_tuples.push(PyTuple::new(py, [k, v])?);
    }
    let frozen_items = PyFrozenSet::new(py, item_tuples)?;
    let tuple = PyTuple::new(
        py,
        [type_block.into_pyobject(py)?.into_any(), frozen_items.into_any(), content.bind(py).clone()],
    )?;
    tuple.hash()
}

/// `to_dict`/`from_dict` on both classes just delegate to `core/serialization.py`'s
/// `to_serializable`/`from_serializable` — imported here at call time (not at module load),
/// mirroring the Python original's own local import inside the method: `serialization.py`
/// imports `PdfBlock`/`TextBlock` from this module (to recognize them via `isinstance`), so a
/// module-level import here would be circular either way, Python or Rust.
fn serialization_module<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
    py.import("freeports._internals.core.serialization")
}

fn to_dict_via_serialization(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyDict>> {
    let result = serialization_module(py)?.call_method1("to_serializable", (obj,))?;
    result.extract().map_err(Into::into)
}

fn from_dict_via_serialization(py: Python<'_>, data: &Py<PyDict>) -> PyResult<Py<PyAny>> {
    let result = serialization_module(py)?.call_method1("from_serializable", (data,))?;
    result.extract().map_err(Into::into)
}

/// Plain, untranslated equivalent of `_str_blocks` — see module docs for why the translated
/// original isn't reproduced here.
fn describe(class_name: &str, type_block: &str, metadata: &Py<PyDict>, content: &Py<PyAny>, py: Python<'_>) -> PyResult<String> {
    let content_repr = content.bind(py).str()?;
    Ok(format!(
        "{class_name}: ({type_block} type)\n\tmetadata {}\n\t{content_repr:?}",
        metadata.bind(py).repr()?
    ))
}

#[pyclass(module = "freeports._native")]
pub struct PdfBlock {
    #[pyo3(get, set)]
    type_block: String,
    metadata: Py<PyDict>,
    content: Py<PyAny>,
}

#[pymethods]
impl PdfBlock {
    #[new]
    pub fn new(type_block: String, metadata: Py<PyDict>, text: Py<PyAny>) -> Self {
        Self { type_block, metadata, content: text }
    }

    #[getter]
    fn metadata(&self, py: Python<'_>) -> Py<PyDict> {
        self.metadata.clone_ref(py)
    }

    #[setter]
    fn set_metadata(&mut self, metadata: Py<PyDict>) {
        self.metadata = metadata;
    }

    #[getter]
    fn content(&self, py: Python<'_>) -> Py<PyAny> {
        self.content.clone_ref(py)
    }

    #[setter]
    fn set_content(&mut self, content: Py<PyAny>) {
        self.content = content;
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        normalize_metadata_and_hash(py, &self.type_block, &self.metadata, &self.content)
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = other.extract::<PyRef<'_, Self>>() else {
            return Ok(false);
        };
        Ok(self.__hash__(py)? == other.__hash__(py)?)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        describe("PdfBlock", &self.type_block, &self.metadata, &self.content, py)
    }

    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        self.__repr__(py)
    }

    fn to_dict(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyDict>> {
        to_dict_via_serialization(py, slf.as_any())
    }

    #[staticmethod]
    fn from_dict(py: Python<'_>, data: Py<PyDict>) -> PyResult<Py<PyAny>> {
        from_dict_via_serialization(py, &data)
    }
}

#[pyclass(module = "freeports._native")]
pub struct TextBlock {
    #[pyo3(get, set)]
    type_block: String,
    metadata: Py<PyDict>,
    content: Py<PyAny>,
    pdf_block: Option<Py<PdfBlock>>,
}

#[pymethods]
impl TextBlock {
    #[new]
    pub fn new(py: Python<'_>, type_block: String, metadata: Py<PyDict>, pdf_block: Py<PdfBlock>) -> Self {
        let content = pdf_block.borrow(py).content.clone_ref(py);
        Self { type_block, metadata, content, pdf_block: Some(pdf_block) }
    }

    /// Bypasses the main constructor entirely (matches the Python original's
    /// `cls.__new__(cls)` + manual field assignment): `content` is taken directly instead of
    /// being derived from a `pdf_block`, and `pdf_block` is `None`.
    #[staticmethod]
    pub fn from_content(type_block: String, metadata: Py<PyDict>, content: Py<PyAny>) -> Self {
        Self { type_block, metadata, content, pdf_block: None }
    }

    #[getter]
    fn metadata(&self, py: Python<'_>) -> Py<PyDict> {
        self.metadata.clone_ref(py)
    }

    #[setter]
    fn set_metadata(&mut self, metadata: Py<PyDict>) {
        self.metadata = metadata;
    }

    #[getter]
    fn content(&self, py: Python<'_>) -> Py<PyAny> {
        self.content.clone_ref(py)
    }

    #[setter]
    fn set_content(&mut self, content: Py<PyAny>) {
        self.content = content;
    }

    #[getter]
    fn pdf_block(&self, py: Python<'_>) -> Option<Py<PdfBlock>> {
        self.pdf_block.as_ref().map(|b| b.clone_ref(py))
    }

    #[setter]
    fn set_pdf_block(&mut self, pdf_block: Option<Py<PdfBlock>>) {
        self.pdf_block = pdf_block;
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        normalize_metadata_and_hash(py, &self.type_block, &self.metadata, &self.content)
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = other.extract::<PyRef<'_, Self>>() else {
            return Ok(false);
        };
        Ok(self.__hash__(py)? == other.__hash__(py)?)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        describe("TextBlock", &self.type_block, &self.metadata, &self.content, py)
    }

    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        self.__repr__(py)
    }

    fn to_dict(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyDict>> {
        to_dict_via_serialization(py, slf.as_any())
    }

    #[staticmethod]
    fn from_dict(py: Python<'_>, data: Py<PyDict>) -> PyResult<Py<PyAny>> {
        from_dict_via_serialization(py, &data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pdf_block(py: Python<'_>, type_block: &str, metadata: &Bound<'_, PyDict>, text: &str) -> Py<PdfBlock> {
        let content = text.into_pyobject(py).unwrap().into_any().unbind();
        Py::new(py, PdfBlock::new(type_block.into(), metadata.clone().unbind(), content)).unwrap()
    }

    #[test]
    fn constructs_and_reads_fields() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            metadata.set_item("a", 1).unwrap();
            let blk = make_pdf_block(py, "FUND", &metadata, "hello");
            let bound = blk.bind(py);
            assert_eq!(bound.borrow().type_block, "FUND");
            let content: String = bound.borrow().content(py).extract(py).unwrap();
            assert_eq!(content, "hello");
        });
    }

    #[test]
    fn hash_normalizes_set_and_list_metadata_values_in_place() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            let set_val = PySet::new(py, ["x", "y"]).unwrap();
            let list_val = PyList::new(py, [1, 2, 3]).unwrap();
            metadata.set_item("s", set_val).unwrap();
            metadata.set_item("l", list_val).unwrap();
            let blk = make_pdf_block(py, "FUND", &metadata, "hello");
            let bound = blk.bind(py);
            let _ = bound.borrow().__hash__(py).unwrap();
            let normalized = bound.borrow().metadata(py);
            let normalized = normalized.bind(py);
            assert!(normalized.get_item("s").unwrap().unwrap().is_instance_of::<PyFrozenSet>());
            assert!(normalized.get_item("l").unwrap().unwrap().is_instance_of::<PyFrozenSet>());
        });
    }

    #[test]
    fn equal_pdf_blocks_have_equal_hash() {
        Python::attach(|py| {
            let m1 = PyDict::new(py);
            m1.set_item("a", 1).unwrap();
            let m2 = PyDict::new(py);
            m2.set_item("a", 1).unwrap();
            let a = make_pdf_block(py, "FUND", &m1, "hello");
            let b = make_pdf_block(py, "FUND", &m2, "hello");
            assert!(a.bind(py).borrow().__eq__(py, b.bind(py)).unwrap());
        });
    }

    #[test]
    fn different_type_block_means_not_equal() {
        Python::attach(|py| {
            let m1 = PyDict::new(py);
            let m2 = PyDict::new(py);
            let a = make_pdf_block(py, "FUND", &m1, "hello");
            let b = make_pdf_block(py, "CURRENCY", &m2, "hello");
            assert!(!a.bind(py).borrow().__eq__(py, b.bind(py)).unwrap());
        });
    }

    #[test]
    fn text_block_derives_content_from_pdf_block() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            let pdf_blk = make_pdf_block(py, "FUND", &metadata, "Café Fund");
            let txt_metadata = PyDict::new(py);
            let txt_blk = TextBlock::new(py, "FUND".into(), txt_metadata.unbind(), pdf_blk.clone_ref(py));
            let content: String = txt_blk.content(py).extract(py).unwrap();
            assert_eq!(content, "Café Fund");
            assert!(txt_blk.pdf_block(py).is_some());
        });
    }

    #[test]
    fn text_block_from_content_has_no_pdf_block() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            let content = "Acme Manager".into_pyobject(py).unwrap().into_any().unbind();
            let txt_blk = TextBlock::from_content("MANAGEMENT_COMPANY".into(), metadata.unbind(), content);
            assert!(txt_blk.pdf_block(py).is_none());
        });
    }

    #[test]
    fn text_block_content_can_be_a_promise() {
        Python::attach(|py| {
            use crate::core::promise::Promise;
            let metadata = PyDict::new(py);
            let promise = Promise::from_parts("fund", false, false).into_pyobject(py).unwrap().into_any().unbind();
            let txt_blk = TextBlock::from_content("FUND".into(), metadata.unbind(), promise);
            let content = txt_blk.content(py);
            assert!(content.bind(py).extract::<Promise>().is_ok());
        });
    }
}
