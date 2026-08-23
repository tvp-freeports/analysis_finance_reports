//! `input::document` — uno dei tre moduli di confine PyO3 (`PLAN.md` §2 principio 1, §3): PyMuPDF
//! si chiama qui, una volta per documento, e il risultato diventa subito `core::page::Page`
//! nativo (`Page::raw` conserva comunque il dict originale, M5 D-M5-2, per i pipe Python di M7).
//!
//! Contratto: `agent-memory/M6-implementation-plan.md` §3.3. **Q2** (confermata dall'utente,
//! stesso file §0): `load_document`/`load_document_pages` non sono elencate da `PLAN.md` §9, ma
//! sono comunque in scope per questa milestone — il buco va documentato (stesso trattamento di
//! `TablePosMeasureUnit`), non lasciato silenzioso (`STATUS.md`).

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

/// Apre `path` con `fitz`, itera le sue pagine (1-based, come `Page::number`), estrae
/// `page.get_text("dict")` per ciascuna e la converte subito in `Page` nativa (`Page::raw` tiene
/// il dict originale). **Q2**: non elencata da `PLAN.md` §9, proposta comunque — vedi il
/// doc-comment del modulo.
pub fn load_document_pages(path: &Path, auto_rotate: bool) -> Result<Vec<Page>, DocumentError> {
    Python::attach(|py| {
        let fitz = PyModule::import(py, "fitz").map_err(|e| open_error(path, e))?;
        let path_str = path.to_string_lossy().to_string();
        let doc = fitz.call_method1("open", (path_str,)).map_err(|e| open_error(path, e))?;
        let page_count = doc.len().map_err(|e| open_error(path, e))?;

        let mut pages = Vec::with_capacity(page_count);
        for i in 0..page_count {
            // `Page::number` e' 1-based, mentre `fitz`/PyMuPDF indicizza le pagine da 0.
            let page_number = u32::try_from(i + 1).expect("a pdf with more than u32::MAX pages does not occur in practice");

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

            pages.push(Page::new(page_number, (page_dict.width, page_dict.height), lines, images).with_raw(raw));
        }

        Ok(pages)
    })
}

/// Come [`load_document_pages`], ma wrappato in un `Document` — `id`/`format` sono forniti dal
/// chiamante (non rilevati qui: la rilevazione del formato è `formats_repo::id_format`, M7).
pub fn load_document(path: &Path, id: impl Into<DocumentId>, format: impl Into<FormatName>, auto_rotate: bool) -> Result<Document, DocumentError> {
    let pages = load_document_pages(path, auto_rotate)?;
    Ok(Document::new(id, format, pages))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Unico** sottomodulo che tocca davvero PyMuPDF in tutta `input::document` (D-M6-3,
    /// `PLAN.md` §11/§10 D13). Richiede `fitz` importabile: attivare `venv/freeports-dev`
    /// (`AGENTS.md`) prima di `cargo test`, come per ogni altro test che tocca Python in questo
    /// crate.
    mod python_boundary {
        use super::*;
        use pyo3::types::PyList;

        /// Costruisce un PDF minimo **con fitz stesso** (nuovo documento, una pagina, del testo
        /// inserito) in un file temporaneo, invece di un fixture binario nel repo. In un unico
        /// `#[test] fn` (non uno per fase, per restare l'unico test che tocca PyMuPDF, D-M6-3):
        ///
        /// 1. carica quel PDF con `load_document` e verifica pagine/dimensioni/testo/`raw`;
        /// 2. esercita transitivamente `PageDict::from_py` sui due casi limite non bloccanti di
        ///    `agent-memory/M6-implementation-plan.md` §3.1 (blocco `Text` senza chiave "lines",
        ///    blocco senza chiave "type") con dict costruiti a mano — nessun test dedicato per
        ///    `from_py` altrove (D-M6-3);
        /// 3. verifica `DocumentError::Open` per un path inesistente.
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

            // Casi limite non bloccanti di `PageDict::from_py` (agent-memory/M6-implementation-plan.md
            // §3.1): esercitati solo qui, con dict costruiti a mano nello stesso interprete gia'
            // attaccato sopra — nessun test dedicato altrove (D-M6-3).
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
