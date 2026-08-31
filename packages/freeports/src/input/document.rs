//! Loading a PDF document: one of the crate's three PyO3 boundary modules.
//!
//! PyMuPDF is called here, once per document, and its output becomes a native [`Page`] straight
//! away. The original dict is kept on the page for the pipes that expect it, but nothing downstream
//! has to know a PDF reader was involved.

pub mod page_dict;
pub mod selection;

pub use page_dict::{PageDict, PageDictBlock, PageDictLine, PageDictSpan, pdfimages_from_pagedict, pdflines_from_pagedict};
pub use selection::{FontCriterion, InputAreaSpec, InputPdfLineSet, LineSelectionError, pdfline_selection_from_dict, pdfline_selection_from_str};

use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::core::page::{Document, DocumentId, FormatName, Page, PageError};

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("could not open `{path}` with PyMuPDF: {message}")]
    Open { path: String, message: String },
    #[error("page {number}: {source}")]
    Page {
        number: u32,
        #[source]
        source: PageError,
    },
}

fn open_error(path: &Path, message: impl std::fmt::Display) -> DocumentError {
    DocumentError::Open { path: path.display().to_string(), message: message.to_string() }
}

/// Opens `path`, iterates its pages, extracts each one's text dict and converts it immediately into
/// a native [`Page`].
///
/// Page numbers are 1-based, matching [`Page::number`].
///
/// # Errors
///
/// [`DocumentError::Open`] if the file cannot be opened, and the parse errors of the pages
/// themselves.
pub fn load_document_pages(path: &Path, auto_rotate: bool) -> Result<Vec<Page>, DocumentError> {
    Python::attach(|py| {
        let fitz = PyModule::import(py, "fitz").map_err(|e| open_error(path, e))?;
        let path_str = path.to_string_lossy().to_string();
        let doc = fitz.call_method1("open", (path_str,)).map_err(|e| open_error(path, e))?;
        let page_count = doc.len().map_err(|e| open_error(path, e))?;

        let mut pages = Vec::with_capacity(page_count);
        for i in 0..page_count {
            // [`Page::number`] is 1-based, while PyMuPDF indexes pages from zero.
            let page_number = u32::try_from(i + 1).expect("a pdf with more than u32::MAX pages does not occur in practice");
            // Groups the sub-steps below — load, extract, parse, build — under one span per page,
            // so their events share a `page` coordinate under the same field name the engine's own
            // page span uses later.
            let page_span = tracing::info_span!("page", page = page_number);
            let _page_guard = page_span.enter();

            let py_page = doc.call_method1("load_page", (i,)).map_err(|e| DocumentError::Page {
                number: page_number,
                source: PageError::ParseFail { message: e.to_string() },
            })?;
            let text_dict = py_page.call_method1("get_text", ("dict",)).map_err(|e| DocumentError::Page {
                number: page_number,
                source: PageError::ParseFail { message: e.to_string() },
            })?;
            let dict = text_dict.cast::<PyDict>().map_err(|e| DocumentError::Page {
                number: page_number,
                source: PageError::ParseFail { message: e.to_string() },
            })?;

            let page_dict = PageDict::from_py(dict).map_err(|source| DocumentError::Page { number: page_number, source })?;
            let lines = pdflines_from_pagedict(&page_dict, auto_rotate);
            let images = pdfimages_from_pagedict(&page_dict);
            let raw = dict.clone().into_any().unbind();

            // The page's first line of text, not just the counts: at `-vv` this one line per page
            // becomes an index of the document to navigate by, instead of a thousand
            // indistinguishable rows.
            tracing::debug!(
                found = %lines.first().map(|line| line.text().clone()).unwrap_or_default(),
                line_count = lines.len(),
                image_count = images.len(),
                "page loaded"
            );
            pages.push(Page::new(page_number, (page_dict.width, page_dict.height), lines, images).with_raw(raw));
        }

        tracing::info!(path = %path.display(), page_count = pages.len(), "document loaded");
        Ok(pages)
    })
}

/// Like [`load_document_pages`], but wrapped in a [`Document`]. The id and format are supplied by
/// the caller; detecting the format is `formats_repo`'s job.
pub fn load_document(path: &Path, id: impl Into<DocumentId>, format: impl Into<FormatName>, auto_rotate: bool) -> Result<Document, DocumentError> {
    let pages = load_document_pages(path, auto_rotate)?;
    Ok(Document::new(id, format, pages))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The **only** submodule in this file that really touches PyMuPDF. It needs `fitz` importable,
    /// so the development virtualenv has to be active before running the tests.
    mod python_boundary {
        use super::*;
        use pyo3::types::PyList;

        /// Builds a minimal PDF **with PyMuPDF itself** — a new document, one page, some text —
        /// into a temporary file, rather than committing a binary fixture. It is one test rather
        /// than several so that it stays the single test touching PyMuPDF:
        ///
        /// 1. loads that PDF and checks pages, dimensions, text and the retained dict;
        /// 2. exercises the page-dict parsing on its two non-blocking edge cases (a text block with no lines key, a block with no type key) using hand-built dicts;
        /// 3. checks the error for a path that does not exist.
        #[test]
        fn loads_lines_and_images_from_a_real_pymupdf_document() {
            let tmp = tempfile::Builder::new().suffix(".pdf").tempfile().expect("could not create a temp file for the test pdf");
            let pdf_path = tmp.path().to_path_buf();

            Python::attach(|py| {
                let fitz = PyModule::import(py, "fitz")
                    .expect("PyMuPDF (fitz) must be importable: activate venv/freeports-dev before running this test, see AGENTS.md");
                let doc = fitz.call_method0("open").expect("fitz.open() with no arguments creates a new, empty document");
                let page = doc.call_method1("new_page", (-1i64, 200.0f64, 300.0f64)).expect("Document.new_page(pno, width, height)");
                page.call_method1("insert_text", ((20.0f64, 50.0f64), "Hello World")).expect("Page.insert_text(point, text)");
                doc.call_method1("save", (pdf_path.to_str().expect("temp path must be valid utf-8"),)).expect("Document.save(path)");
                doc.call_method0("close").expect("Document.close()");
            });

            let document =
                load_document(&pdf_path, "doc-id", "FMT-TEST", true).expect("loading a real, freshly-built single-page pdf must succeed");

            assert_eq!(document.id, DocumentId::new("doc-id"));
            assert_eq!(document.format, FormatName::new("FMT-TEST"));
            assert_eq!(document.pages.len(), 1);

            let page = document.page(1).expect("a freshly-built single-page pdf has page number 1 (1-based)");
            assert_eq!(page.size, (200.0, 300.0));
            assert!(
                page.lines.iter().any(|l| l.text().contains("Hello World")),
                "expected at least one PdfLine containing the inserted text, got {:?}",
                page.lines.iter().map(|l| l.text()).collect::<Vec<_>>()
            );

            Python::attach(|py| {
                let raw = page.raw().expect("input::document must attach the original PyMuPDF dict via Page::with_raw (M5 D-M5-2)");
                let width: f64 = raw.bind(py).get_item("width").expect("the attached dict must be the pymupdf page dict").extract().unwrap();
                assert_eq!(width, 200.0);
            });

            // The edge cases of the page-dict parsing, exercised here with hand-built dicts inside
            // the interpreter already attached above.
            Python::attach(|py| {
                let block_without_lines_key = PyDict::new(py);
                block_without_lines_key.set_item("type", 0).unwrap();
                let blocks = PyList::empty(py);
                blocks.append(&block_without_lines_key).unwrap();
                let page_dict = PyDict::new(py);
                page_dict.set_item("width", 100.0).unwrap();
                page_dict.set_item("height", 100.0).unwrap();
                page_dict.set_item("blocks", &blocks).unwrap();
                let parsed =
                    PageDict::from_py(&page_dict).expect("a type==0 block with no 'lines' key is not an error, just an empty Text block");
                assert_eq!(parsed.blocks, vec![PageDictBlock::Text { lines: vec![] }]);

                let block_without_type_key = PyDict::new(py);
                let blocks = PyList::empty(py);
                blocks.append(&block_without_type_key).unwrap();
                let page_dict = PyDict::new(py);
                page_dict.set_item("width", 100.0).unwrap();
                page_dict.set_item("height", 100.0).unwrap();
                page_dict.set_item("blocks", &blocks).unwrap();
                let err = PageDict::from_py(&page_dict).expect_err("a block with no 'type' key at all is not a pymupdf page dict");
                assert!(matches!(err, PageError::ParseFail { .. }), "expected PageError::ParseFail, got {err:?}");
            });

            let missing = load_document(Path::new("/nonexistent/path/does-not-exist.pdf"), "doc-id", "FMT-TEST", true);
            assert!(matches!(missing, Err(DocumentError::Open { .. })), "expected DocumentError::Open, got {missing:?}");
        }
    }
}
