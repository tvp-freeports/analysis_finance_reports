//! Rust port of `formats/utils/pdf_extract/common.py`'s `SelectExpectedText`/
//! `ExtractTextPdfBlockOrFailPage`.
//!
//! `selection` stays a generic `Py<PyAny>` rather than a concrete `freeports_lib` type: the
//! Python original never constrains it beyond duck-typing a `.select(lines) -> list` method
//! (`PdfLineSelection` from `freeports_lib.pdf_extract.select` in practice, but nothing here
//! checks that), and calling across the Python boundary this way avoids adding `freeports_lib`
//! as a Rust-level dependency of this crate just for one method call — its `select` pymethod is
//! reachable through Python's own object model regardless.
//!
//! **Bug found and fixed at the root (user confirmed, 2026-08-19)**: the Python original
//! referenced `logger`, `ExpectedPdfBlockNotFound`, and `PageParseFail` without importing any of
//! them. Verified empirically that hitting the "no line matched" fallback path raised
//! `NameError: name 'logger' is not defined` instead of the intended typed exception — a
//! completely dead/broken failure path. Fixed in `pdf_extract/common.py` (added the missing
//! imports, following the `logging.getLogger(__name__)` pattern already used by sibling files
//! like `pdf_extract/standard_funcs.py`) before porting; this Rust port implements the corrected
//! behavior.
//!
//! Logging here uses `tracing` (task-phase spans/events), not a translation of Python's
//! per-file `logging.getLogger(__name__)` call — same OS/i18n/logging-stays-conceptually-Python
//! split as the rest of this migration (see `core/tracing_setup.rs`). `block_extraction` is the
//! span name that module's own doc comment names as the first real example of a "task boundary"
//! worth instrumenting — this is that moment: `ExtractTextPdfBlockOrFailPage::call` is a
//! page-level extraction step that can fail with `PageParseFail`, not a leaf utility.

use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::core::classes::{ExpectedPdfBlockNotFound, PageParseFail, PdfBlock};

fn pdf_blks_acquire_module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    py.import("freeports._internals.formats.utils.pdf_extract.pdf_blks_acquire")
}

#[pyclass(module = "freeports_engine")]
pub struct SelectExpectedText {
    selection: Py<PyAny>,
    #[pyo3(get, set)]
    name: String,
}

#[pymethods]
impl SelectExpectedText {
    #[new]
    #[pyo3(signature = (selection, name = "expected text".to_string()))]
    pub fn new(selection: Py<PyAny>, name: String) -> Self {
        Self { selection, name }
    }

    #[getter]
    fn selection(&self, py: Python<'_>) -> Py<PyAny> {
        self.selection.clone_ref(py)
    }

    #[setter]
    fn set_selection(&mut self, selection: Py<PyAny>) {
        self.selection = selection;
    }

    #[pyo3(name = "__call__")]
    pub fn call(&self, py: Python<'_>, lines: &Bound<'_, PyAny>) -> PyResult<String> {
        let selected = self.selection.bind(py).call_method1("select", (lines,))?;
        match selected.get_item(0) {
            Ok(line) => line.getattr("text")?.extract(),
            Err(err) if err.is_instance_of::<PyIndexError>(py) => {
                tracing::error!(name = %self.name, error = %err, "expected pdf block not found");
                if let Ok(iter) = lines.try_iter() {
                    let first_texts: Vec<String> = iter
                        .take(10)
                        .filter_map(|item| item.ok()?.getattr("text").ok()?.extract().ok())
                        .collect();
                    tracing::debug!(?first_texts, "first lines were");
                }
                Err(ExpectedPdfBlockNotFound::new_err(format!(
                    "Pdf block during extraction of \"{}\" not found",
                    self.name
                )))
            }
            Err(err) => Err(err),
        }
    }
}

#[pyclass(module = "freeports_engine")]
pub struct ExtractTextPdfBlockOrFailPage {
    extractor: Py<SelectExpectedText>,
    #[pyo3(get, set)]
    type_block: String,
}

#[pymethods]
impl ExtractTextPdfBlockOrFailPage {
    #[new]
    pub fn new(py: Python<'_>, selection: Py<PyAny>, name: String, type_block: String) -> PyResult<Self> {
        let extractor = Py::new(py, SelectExpectedText::new(selection, name))?;
        Ok(Self { extractor, type_block })
    }

    #[getter]
    fn extractor(&self, py: Python<'_>) -> Py<SelectExpectedText> {
        self.extractor.clone_ref(py)
    }

    #[setter]
    fn set_extractor(&mut self, extractor: Py<SelectExpectedText>) {
        self.extractor = extractor;
    }

    #[pyo3(name = "__call__")]
    pub fn call(&self, py: Python<'_>, dict_root: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PdfBlock>>> {
        let span = tracing::info_span!("block_extraction", type_block = %self.type_block);
        let _enter = span.enter();

        let lines = pdf_blks_acquire_module(py)?.call_method1("pdflines_from_pagedict", (dict_root,))?;

        let text = {
            let extractor = self.extractor.bind(py).borrow();
            extractor.call(py, &lines)
        };
        let text = match text {
            Ok(t) => t,
            Err(err) if err.is_instance_of::<ExpectedPdfBlockNotFound>(py) => {
                let msg = err.value(py).str()?.to_string();
                let new_err = PageParseFail::new_err(msg);
                new_err.set_cause(py, Some(err));
                return Err(new_err);
            }
            Err(err) => return Err(err),
        };

        let metadata = PyDict::new(py).unbind();
        let content = text.into_pyobject(py)?.into_any().unbind();
        let blk = Py::new(py, PdfBlock::new(self.type_block.clone(), metadata, content))?;
        Ok(vec![blk])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::ffi::c_str;
    use std::ffi::CString;

    fn make_line<'py>(py: Python<'py>, text: &str) -> Bound<'py, PyAny> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("text", text).unwrap();
        py.import("types")
            .unwrap()
            .getattr("SimpleNamespace")
            .unwrap()
            .call((), Some(&kwargs))
            .unwrap()
    }

    fn make_selection<'py>(py: Python<'py>, select_body: &str) -> Bound<'py, PyAny> {
        let src = CString::new(format!("lambda lines: {select_body}")).unwrap();
        let select_fn = py.eval(&src, None, None).unwrap();
        let kwargs = PyDict::new(py);
        kwargs.set_item("select", select_fn).unwrap();
        py.import("types")
            .unwrap()
            .getattr("SimpleNamespace")
            .unwrap()
            .call((), Some(&kwargs))
            .unwrap()
    }

    fn sample_page(py: Python<'_>) -> Bound<'_, PyAny> {
        py.eval(
            c_str!(
                "{'width': 100.0, 'height': 100.0, 'blocks': [{'type': 0, 'lines': [\
                 {'dir': (1.0, 0.0), 'bbox': (0.0, 0.0, 10.0, 10.0), 'spans': [\
                 {'font': 'Arial', 'size': 10.0, 'text': 'Hello', 'bbox': (0.0, 0.0, 10.0, 10.0)}\
                 ]}]}]}"
            ),
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn select_expected_text_returns_matched_line_text() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let lines = pyo3::types::PyList::new(py, [make_line(py, "Found")]).unwrap();
            let selection = make_selection(py, "[lines[0]]");
            let selector = SelectExpectedText::new(selection.unbind(), "field".into());
            let result = selector.call(py, lines.as_any()).unwrap();
            assert_eq!(result, "Found");
        });
    }

    #[test]
    fn select_expected_text_raises_expected_pdf_block_not_found_on_empty_selection() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let lines = pyo3::types::PyList::new(py, [make_line(py, "Found")]).unwrap();
            let selection = make_selection(py, "[]");
            let selector = SelectExpectedText::new(selection.unbind(), "some field".into());
            let err = selector.call(py, lines.as_any()).unwrap_err();
            assert!(err.is_instance_of::<ExpectedPdfBlockNotFound>(py));
            assert!(err.value(py).str().unwrap().to_string().contains("some field"));
        });
    }

    #[test]
    fn select_expected_text_propagates_unrelated_errors_unwrapped() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let lines = pyo3::types::PyList::new(py, [make_line(py, "Found")]).unwrap();
            let selection = make_selection(py, "exec(\"raise ValueError('boom')\")");
            let selector = SelectExpectedText::new(selection.unbind(), "field".into());
            let err = selector.call(py, lines.as_any()).unwrap_err();
            assert!(!err.is_instance_of::<ExpectedPdfBlockNotFound>(py));
            assert!(err.is_instance_of::<pyo3::exceptions::PyValueError>(py));
        });
    }

    #[test]
    fn extract_text_pdf_block_or_fail_page_returns_pdf_block_with_matched_text() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let selection = make_selection(py, "list(lines)[:1]");
            let extractor = ExtractTextPdfBlockOrFailPage::new(py, selection.unbind(), "field".into(), "FUND".into()).unwrap();
            let page = sample_page(py);
            let blocks = extractor.call(py, &page).unwrap();
            assert_eq!(blocks.len(), 1);
            let blk = blocks[0].bind(py);
            let type_block: String = blk.getattr("type_block").unwrap().extract().unwrap();
            assert_eq!(type_block, "FUND");
            let content: String = blk.getattr("content").unwrap().extract().unwrap();
            assert_eq!(content, "Hello");
        });
    }

    #[test]
    fn extract_text_pdf_block_or_fail_page_wraps_not_found_as_page_parse_fail() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let selection = make_selection(py, "[]");
            let extractor = ExtractTextPdfBlockOrFailPage::new(py, selection.unbind(), "missing field".into(), "FUND".into()).unwrap();
            let page = sample_page(py);
            let err = extractor.call(py, &page).unwrap_err();
            assert!(err.is_instance_of::<PageParseFail>(py));
            assert!(!err.is_instance_of::<ExpectedPdfBlockNotFound>(py));
            assert!(err.value(py).str().unwrap().to_string().contains("missing field"));
            let cause = err.value(py).getattr("__cause__").unwrap();
            assert!(cause.is_instance_of::<ExpectedPdfBlockNotFound>());
        });
    }
}
