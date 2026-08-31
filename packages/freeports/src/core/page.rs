//! The native model of a [`Page`] and a [`Document`]: lines, images, geometry, provenance.
//!
//! This module defines **data only**. Building a page out of a PyMuPDF dict — rotating bounding
//! boxes, collapsing spans, pulling out images — is `input::document`'s job. The separation matters
//! because [`Page`] is the type the three pipe traits are written against: every pipe in the engine
//! receives a `&Page`, and none of them needs to know a PDF reader exists.
//!
//! # The one place PyO3 leaks in: [`Page::raw`]
//!
//! A page keeps the original PyMuPDF dict alongside its native form, because pipes written by a
//! format author expect that dict rather than the native [`Page`]. It is deliberately a private
//! field with a single accessor, read only by the adapters at the Python boundary; everywhere else
//! in the crate a [`Page`] is an ordinary Rust struct.
//!
//! It is also why [`Page`] derives neither `Clone` nor `PartialEq`: `Py<T>` is `Clone` only under
//! PyO3's `py-clone` feature, which this crate does not enable. Nothing in the engine may therefore
//! be built on cloning a page — an outcome worth knowing about up front rather than discovering
//! halfway through.
//!
//! [`Page`] is nonetheless `Send + Sync`, which is what allows pages and documents to be processed
//! in parallel without redesigning anything.

use pyo3::prelude::*;

use crate::commons::geometry::Rectangle;
use crate::formats_utils::pdf_extract::pdf_line::PdfLine;

/// A document's identifier: a short name, a path, or a URL.
///
/// A newtype rather than a bare `String`, for the same reason as
/// [`BlockType`](crate::core::classes::BlockType): it gives the behaviour somewhere to live, and it
/// stops a document id and a [`FormatName`] from being swapped in a signature. It is also the key
/// results are grouped by when a run covers several documents.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new(id: impl Into<String>) -> Self {
        DocumentId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for DocumentId {
    fn from(value: &str) -> Self {
        DocumentId(value.to_string())
    }
}

impl From<String> for DocumentId {
    fn from(value: String) -> Self {
        DocumentId(value)
    }
}

/// The name of a format in a formats repository, for example `EURIZON-EN23`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FormatName(String);

impl FormatName {
    pub fn new(name: impl Into<String>) -> Self {
        FormatName(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FormatName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for FormatName {
    fn from(value: &str) -> Self {
        FormatName(value.to_string())
    }
}

impl From<String> for FormatName {
    fn from(value: String) -> Self {
        FormatName(value)
    }
}

/// A raster image of a page, **left undecoded**.
///
/// The bytes are kept exactly as PyMuPDF returns them, with the extension it declares. Decoding
/// them would mean taking on an image-processing dependency for a payload no standard pipe reads;
/// keeping them raw costs nothing and loses nothing, and a consumer that eventually needs pixels
/// can decode from here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    /// The rectangle the image occupies on the page.
    pub bbox: Rectangle,
    /// The image format extension as declared by PyMuPDF (`"png"`, `"jpeg"`, …).
    pub ext: String,
    /// The image's raw bytes, exactly as PyMuPDF returns them.
    pub data: Vec<u8>,
}

/// One page of a PDF document, already converted to native data.
///
/// Derives neither `Clone` nor `PartialEq`; see the note on [`Page::raw`] in the module
/// documentation for why.
#[derive(Debug)]
pub struct Page {
    /// Page number, **1-based**.
    pub number: u32,
    /// `(width, height)` in PDF points.
    pub size: (f32, f32),
    /// Text lines, in the order `input::document` extracted them.
    pub lines: Vec<PdfLine>,
    /// The page's raster images.
    pub images: Vec<PageImage>,
    /// The original PyMuPDF dict, kept for pipes written by a format author.
    raw: Option<Py<PyAny>>,
}

impl Page {
    /// Builds a native page with no PyMuPDF dict attached — the form used by everything that is not
    /// the Python boundary, tests included.
    pub fn new(number: u32, size: (f32, f32), lines: Vec<PdfLine>, images: Vec<PageImage>) -> Self {
        Page { number, size, lines, images, raw: None }
    }

    /// Attaches the original PyMuPDF dict. Called only from `input::document`.
    pub fn with_raw(mut self, raw: Py<PyAny>) -> Self {
        self.raw = Some(raw);
        self
    }

    /// The original PyMuPDF dict, if there is one.
    ///
    /// The only accessor in `core` that names PyO3, and it is read only by the adapters for
    /// author-written pipes.
    pub fn raw(&self) -> Option<&Py<PyAny>> {
        self.raw.as_ref()
    }
}

/// A document: its identity, the format it should be read with, and its pages.
#[derive(Debug)]
pub struct Document {
    pub id: DocumentId,
    pub format: FormatName,
    pub pages: Vec<Page>,
}

impl Document {
    pub fn new(id: impl Into<DocumentId>, format: impl Into<FormatName>, pages: Vec<Page>) -> Self {
        Document { id: id.into(), format: format.into(), pages }
    }

    /// The page with the given **1-based** number, looked up by [`Page::number`] rather than by
    /// position: a document may legitimately hold a non-contiguous subset of its pages.
    pub fn page(&self, number: u32) -> Option<&Page> {
        self.pages.iter().find(|p| p.number == number)
    }
}

/// Failures that concern a page as a whole rather than a single pipe.
///
/// A [`PageError`] travelling up through a pipe becomes
/// [`PipeError::PageParse`](crate::core::pipeline::PipeError::PageParse), which
/// [`Algorithm`](crate::core::algorithm::Algorithm) treats as **non-fatal**: the page is skipped
/// and the run carries on. A malformed page in a thousand-page report should cost that page, not
/// the report.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PageError {
    #[error("{message}")]
    ParseFail { message: String },
    #[error("{message}")]
    LineParseFail { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> PdfLine {
        PdfLine::new("Arial", 10.0, text, (0.0, 0.0, 10.0, 10.0))
    }

    fn image() -> PageImage {
        PageImage {
            bbox: Rectangle::new(0.0, 0.0, 4.0, 4.0),
            ext: "png".to_string(),
            data: vec![1, 2, 3],
        }
    }

    mod identifiers {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_document_id_round_trips_through_its_accessor() {
            assert_eq!(DocumentId::new("report-2023").as_str(), "report-2023");
        }

        #[test]
        fn a_document_id_displays_as_its_bare_string() {
            assert_eq!(DocumentId::new("a/b/c.pdf").to_string(), "a/b/c.pdf");
        }

        #[test]
        fn a_document_id_is_built_from_both_str_and_string() {
            assert_eq!(DocumentId::from("x"), DocumentId::from("x".to_string()));
        }

        #[test]
        fn document_ids_order_and_hash_by_their_string() {
            let mut ids = vec![DocumentId::new("b"), DocumentId::new("a")];
            ids.sort();
            assert_eq!(ids, vec![DocumentId::new("a"), DocumentId::new("b")]);

            let set: std::collections::HashSet<_> =
                [DocumentId::new("a"), DocumentId::new("a")].into_iter().collect();
            assert_eq!(set.len(), 1);
        }

        #[test]
        fn a_format_name_round_trips_and_displays() {
            assert_eq!(FormatName::new("EURIZON-EN23").as_str(), "EURIZON-EN23");
            assert_eq!(FormatName::from("EURIZON-EN23").to_string(), "EURIZON-EN23");
        }

        #[test]
        fn a_format_name_is_built_from_both_str_and_string() {
            assert_eq!(FormatName::from("f"), FormatName::from("f".to_string()));
        }
    }

    mod page_construction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_page_keeps_the_lines_in_the_order_given() {
            let page = Page::new(1, (595.0, 842.0), vec![line("first"), line("second")], vec![]);
            let texts: Vec<&str> = page.lines.iter().map(|l| l.text().as_str()).collect();
            assert_eq!(texts, vec!["first", "second"]);
        }

        #[test]
        fn a_page_keeps_its_number_and_size() {
            let page = Page::new(7, (595.0, 842.0), vec![], vec![]);
            assert_eq!(page.number, 7);
            assert_eq!(page.size, (595.0, 842.0));
        }

        #[test]
        fn a_page_built_natively_carries_no_pymupdf_dict() {
            let page = Page::new(1, (1.0, 1.0), vec![], vec![]);
            assert!(page.raw().is_none());
        }

        #[test]
        fn a_page_keeps_its_images_undecoded() {
            let page = Page::new(1, (1.0, 1.0), vec![], vec![image()]);
            assert_eq!(page.images.len(), 1);
            assert_eq!(page.images[0].ext, "png");
            assert_eq!(page.images[0].data, vec![1, 2, 3]);
        }

        #[test]
        fn a_page_with_neither_lines_nor_images_is_legal() {
            let page = Page::new(1, (0.0, 0.0), vec![], vec![]);
            assert!(page.lines.is_empty());
            assert!(page.images.is_empty());
        }
    }

    mod document_construction {
        use super::*;
        use pretty_assertions::assert_eq;

        fn doc() -> Document {
            Document::new(
                "report",
                "FMT-EN23",
                vec![Page::new(1, (1.0, 1.0), vec![], vec![]), Page::new(3, (1.0, 1.0), vec![], vec![])],
            )
        }

        #[test]
        fn a_document_keeps_id_and_format() {
            let d = doc();
            assert_eq!(d.id, DocumentId::new("report"));
            assert_eq!(d.format, FormatName::new("FMT-EN23"));
        }

        #[test]
        fn a_page_is_looked_up_by_its_number_not_by_position() {
            // Pages 1 and 3: page 3 sits at index 1, so looking up by position would give the wrong
            // answer.
            let d = doc();
            assert_eq!(d.page(3).map(|p| p.number), Some(3));
        }

        #[test]
        fn looking_up_an_absent_page_yields_none() {
            assert!(doc().page(2).is_none());
        }

        #[test]
        fn a_document_with_no_pages_is_legal() {
            let d = Document::new("empty", "FMT", vec![]);
            assert!(d.pages.is_empty());
            assert!(d.page(1).is_none());
        }
    }

    mod page_errors {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_parse_failure_displays_its_bare_message() {
            let err = PageError::ParseFail { message: "no table found".to_string() };
            assert_eq!(err.to_string(), "no table found");
        }

        #[test]
        fn a_line_parse_failure_displays_its_bare_message() {
            let err = PageError::LineParseFail { message: "bad span".to_string() };
            assert_eq!(err.to_string(), "bad span");
        }

        #[test]
        fn the_two_variants_are_distinguishable() {
            assert_ne!(
                PageError::ParseFail { message: "x".to_string() },
                PageError::LineParseFail { message: "x".to_string() }
            );
        }
    }

    /// The only submodule that attaches to the interpreter: it checks that an attached PyMuPDF dict
    /// is really kept and handed back. Nothing else in `core` touches Python.
    mod python_boundary {
        use super::*;

        #[test]
        fn an_attached_pymupdf_dict_is_kept_and_returned() {
            Python::attach(|py| {
                let raw = pyo3::types::PyDict::new(py);
                raw.set_item("width", 595).unwrap();
                let page = Page::new(1, (595.0, 842.0), vec![], vec![])
                    .with_raw(raw.clone().into_any().unbind());

                let kept = page.raw().expect("raw was just attached");
                let width: i64 = kept.bind(py).get_item("width").unwrap().extract().unwrap();
                assert_eq!(width, 595);
            });
        }
    }
}
