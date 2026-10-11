//! Opening a PDF **once** and converting its pages one at a time, on request.
//!
//! [`load_document_pages`](super::load_document_pages) converts every page up front, which is what
//! a run wants: every page will be read anyway. Classifying a document does not: it reads a handful
//! of pages named by the format author — the first, the third, the second to last — and on a
//! 1824-page report converting everything to read five pages is the whole cost of the command.
//!
//! [`LazyDocument`] keeps the PyMuPDF document open between one page and the next, knows how many
//! pages it has, and converts only the page asked for — through the same conversion
//! `load_document_pages` uses, so a page read either way is the same page.
//!
//! Page numbers as an author writes them are resolved by [`resolve_page_number`]: 1-based, negative
//! counted from the end, out of range meaning *no page* rather than an error. `0` is not a page
//! number at all, and the type says so.

use std::num::NonZeroI32;
use std::path::Path;

use pyo3::prelude::*;

use super::{DocumentError, convert_page, drain_pdf_reader_warnings, open_error};
use crate::core::page::Page;

/// Resolves a page number written by a format author into a 0-based page index.
///
/// - positive numbers are **1-based**: `1` is the first page;
/// - negative numbers count **from the end**: `-1` is the last page, `-2` the second to last;
/// - a number past either end of the document resolves to `None` — asking for a page the document
///   does not have is the same as finding nothing on it, not an error.
///
/// `0` cannot be expressed: the loader rejects it before a number ever reaches here.
///
/// The arithmetic is done on the magnitude as a `usize`, never on the page count as an `i32`: the
/// count is not truncated, and `i32::MIN` has no positive counterpart to overflow into.
pub fn resolve_page_number(number: NonZeroI32, page_count: usize) -> Option<usize> {
    let magnitude = usize::try_from(number.get().unsigned_abs()).ok()?;
    if number.get() > 0 {
        let index = magnitude - 1;
        (index < page_count).then_some(index)
    } else {
        page_count.checked_sub(magnitude)
    }
}

/// A PDF opened once with PyMuPDF and kept open, whose pages are converted only when asked for.
///
/// Holding the document as a [`Py`] rather than a [`Bound`] is what keeps it `Send + Sync`: every
/// use re-attaches to the interpreter, so the handle can outlive any one `Python::attach`.
pub struct LazyDocument {
    document: Py<PyAny>,
    page_count: usize,
    auto_rotate: bool,
}

impl LazyDocument {
    /// Opens `path` with PyMuPDF and reads its page count, converting no page yet.
    ///
    /// `auto_rotate` has the meaning it has in [`load_document_pages`](super::load_document_pages),
    /// and applies to every page later converted.
    ///
    /// MuPDF's warnings are drained before returning, whether the opening succeeded or not: see
    /// `drain_pdf_reader_warnings` for why a warning left in the buffer crashes the process at exit.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Open`] if the file cannot be opened.
    pub fn open(path: &Path, auto_rotate: bool) -> Result<Self, DocumentError> {
        Python::attach(|py| {
            let fitz = PyModule::import(py, "fitz").map_err(|e| open_error(path, e))?;
            let opened = open_document(&fitz, path);
            drain_pdf_reader_warnings(&fitz);
            let (document, page_count) = opened?;
            Ok(Self { document, page_count, auto_rotate })
        })
    }

    /// The number of pages in the document.
    pub fn page_count(&self) -> usize {
        self.page_count
    }

    /// Converts the page an author's `number` designates (see [`resolve_page_number`]).
    ///
    /// `Ok(None)` when the document has no such page. The returned [`Page::number`] is the resolved
    /// 1-based number, never the negative one written by the author.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Page`] if the page exists but cannot be converted.
    pub fn page(&self, number: NonZeroI32) -> Result<Option<Page>, DocumentError> {
        let Some(index) = resolve_page_number(number, self.page_count) else { return Ok(None) };
        Python::attach(|py| {
            let page = convert_page(self.document.bind(py), index, self.auto_rotate);
            if let Ok(fitz) = PyModule::import(py, "fitz") {
                drain_pdf_reader_warnings(&fitz);
            }
            page.map(Some)
        })
    }
}

/// Closes the document while the interpreter is still certainly alive, and takes whatever MuPDF
/// said while closing it: dropping the last reference later, from wherever the value happens to
/// die, would leave both to chance.
impl Drop for LazyDocument {
    fn drop(&mut self) {
        Python::attach(|py| {
            if let Err(error) = self.document.bind(py).call_method0("close") {
                tracing::debug!(error = %error, "the pdf could not be closed");
            }
            if let Ok(fitz) = PyModule::import(py, "fitz") {
                drain_pdf_reader_warnings(&fitz);
            }
        });
    }
}

/// The part of [`LazyDocument::open`] that can fail after PyMuPDF is imported, kept apart so the
/// warnings are drained on every outcome.
fn open_document(fitz: &Bound<'_, PyModule>, path: &Path) -> Result<(Py<PyAny>, usize), DocumentError> {
    let document = fitz.call_method1("open", (path.to_string_lossy().to_string(),)).map_err(|e| open_error(path, e))?;
    let page_count = document.len().map_err(|e| open_error(path, e))?;
    Ok((document.unbind(), page_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(number: i32) -> NonZeroI32 {
        NonZeroI32::new(number).expect("test page numbers are never zero")
    }

    /// The pure resolution of an author's page number, no interpreter involved.
    mod resolve_page_number {
        use super::*;

        mod positive_numbers {
            use super::*;

            #[test]
            fn one_is_the_first_page() {
                assert_eq!(resolve_page_number(n(1), 4), Some(0));
            }

            #[test]
            fn are_one_based_throughout_the_document() {
                for (number, expected) in [(1, 0), (2, 1), (3, 2), (4, 3)] {
                    assert_eq!(resolve_page_number(n(number), 4), Some(expected), "page {number} of 4");
                }
            }

            #[test]
            fn the_page_count_itself_is_the_last_page() {
                assert_eq!(resolve_page_number(n(10), 10), Some(9));
            }

            #[test]
            fn one_on_a_single_page_document_is_that_page() {
                assert_eq!(resolve_page_number(n(1), 1), Some(0));
            }
        }

        mod negative_numbers {
            use super::*;

            #[test]
            fn minus_one_is_the_last_page() {
                assert_eq!(resolve_page_number(n(-1), 4), Some(3));
            }

            #[test]
            fn minus_two_is_the_second_to_last_page() {
                assert_eq!(resolve_page_number(n(-2), 4), Some(2));
            }

            #[test]
            fn count_from_the_end_throughout_the_document() {
                for (number, expected) in [(-1, 3), (-2, 2), (-3, 1), (-4, 0)] {
                    assert_eq!(resolve_page_number(n(number), 4), Some(expected), "page {number} of 4");
                }
            }

            #[test]
            fn minus_the_page_count_is_the_first_page() {
                assert_eq!(resolve_page_number(n(-10), 10), Some(0));
            }

            #[test]
            fn minus_one_on_a_single_page_document_is_that_page() {
                assert_eq!(resolve_page_number(n(-1), 1), Some(0));
            }
        }

        mod out_of_range {
            use super::*;

            #[test]
            fn one_past_the_last_page_is_none() {
                assert_eq!(resolve_page_number(n(5), 4), None);
            }

            #[test]
            fn one_before_the_first_page_counting_from_the_end_is_none() {
                assert_eq!(resolve_page_number(n(-5), 4), None);
            }

            #[test]
            fn far_past_the_end_is_none() {
                assert_eq!(resolve_page_number(n(1000), 4), None);
                assert_eq!(resolve_page_number(n(-1000), 4), None);
            }

            #[test]
            fn the_extremes_of_the_type_are_none_without_overflowing() {
                assert_eq!(resolve_page_number(n(i32::MAX), 4), None);
                assert_eq!(resolve_page_number(n(i32::MIN), 4), None);
            }

            #[test]
            fn minus_two_on_a_single_page_document_is_none() {
                assert_eq!(resolve_page_number(n(-2), 1), None);
            }

            #[test]
            fn a_document_with_no_pages_has_none_of_them() {
                for number in [1, 2, -1, -2, i32::MAX, i32::MIN] {
                    assert_eq!(resolve_page_number(n(number), 0), None, "page {number} of 0");
                }
            }

            #[test]
            fn a_page_count_beyond_i32_still_resolves() {
                let huge = usize::try_from(i64::from(i32::MAX) + 10).expect("a 64-bit usize");
                assert_eq!(resolve_page_number(n(1), huge), Some(0));
                assert_eq!(resolve_page_number(n(-1), huge), Some(huge - 1));
                assert_eq!(resolve_page_number(n(i32::MAX), huge), Some(usize::try_from(i32::MAX).unwrap() - 1));
                assert_eq!(resolve_page_number(n(i32::MIN), huge), Some(huge - usize::try_from(i32::MIN.unsigned_abs()).unwrap()));
            }
        }

        /// The order of the source's own example, `1, 3, 5, 2, -2`, on documents of several sizes.
        mod the_source_example_order {
            use super::*;

            const ORDER: [i32; 5] = [1, 3, 5, 2, -2];

            fn resolve_all(page_count: usize) -> Vec<Option<usize>> {
                ORDER.iter().map(|&number| resolve_page_number(n(number), page_count)).collect()
            }

            #[test]
            fn on_one_page_only_the_first_exists() {
                assert_eq!(resolve_all(1), vec![Some(0), None, None, None, None]);
            }

            #[test]
            fn on_two_pages_the_second_to_last_is_the_first() {
                assert_eq!(resolve_all(2), vec![Some(0), None, None, Some(1), Some(0)]);
            }

            #[test]
            fn on_four_pages_only_the_fifth_is_missing() {
                assert_eq!(resolve_all(4), vec![Some(0), Some(2), None, Some(1), Some(2)]);
            }

            #[test]
            fn on_ten_pages_every_page_exists() {
                assert_eq!(resolve_all(10), vec![Some(0), Some(2), Some(4), Some(1), Some(8)]);
            }
        }
    }

    /// Compile-time guarantees of the struct.
    mod thread_bounds {
        use super::*;

        fn assert_send_sync<T: Send + Sync>() {}

        #[test]
        fn lazy_document_is_send_and_sync() {
            assert_send_sync::<LazyDocument>();
        }
    }

    /// The tests that really open PDFs with PyMuPDF. They need `fitz` importable, so the
    /// development virtualenv has to be active before running them.
    ///
    /// Every PDF is **built by the test itself** with PyMuPDF, in a temporary directory, rather than
    /// committed as a binary fixture: each page carries the text `pagina N`, so which page came back
    /// is readable from the page itself.
    mod python_boundary {
        use super::*;
        use std::path::PathBuf;

        use pyo3::types::PyDict;

        use crate::input::document::load_document_pages;

        fn fitz(py: Python<'_>) -> Bound<'_, PyModule> {
            PyModule::import(py, "fitz")
                .expect("PyMuPDF (fitz) must be importable: activate venv/freeports-dev before running this test, see AGENTS.md")
        }

        /// Runs a Python snippet with `fitz`, `path` and the given extra globals bound.
        fn run_python(code: &std::ffi::CStr, path: &Path, extra: &[(&str, i64)]) {
            Python::attach(|py| {
                let globals = PyDict::new(py);
                globals.set_item("fitz", fitz(py)).unwrap();
                globals.set_item("path", path.to_str().expect("temp path must be valid utf-8")).unwrap();
                for (name, value) in extra {
                    globals.set_item(*name, *value).unwrap();
                }
                py.run(code, Some(&globals), None).expect("the fixture-building snippet must run");
            });
        }

        /// A PDF of `page_count` pages, page `N` saying `pagina N` and `200 + N` points wide, so
        /// that both its text and its size tell which page it is.
        fn plain_pdf(dir: &tempfile::TempDir, page_count: i64) -> PathBuf {
            let path = dir.path().join(format!("plain-{page_count}.pdf"));
            run_python(
                c"
doc = fitz.open()
for i in range(1, page_count + 1):
    page = doc.new_page(-1, 200.0 + i, 300.0)
    page.insert_text((20, 50), f'pagina {i}')
doc.save(path)
doc.close()
",
                &path,
                &[("page_count", page_count)],
            );
            path
        }

        /// Three pages that carry everything a [`Page`] holds: text, a raster image, a drawn
        /// rectangle, and on page 3 a line written at 90 degrees so that `auto_rotate` matters.
        fn decorated_pdf(dir: &tempfile::TempDir) -> PathBuf {
            let path = dir.path().join("decorated.pdf");
            run_python(
                c"
doc = fitz.open()
for i in range(1, 4):
    page = doc.new_page(-1, 300.0 + i, 400.0)
    page.insert_text((20, 50), f'pagina {i}')
    page.insert_text((20, 70), f'seconda riga della pagina {i}', fontsize=9)
    page.draw_rect(fitz.Rect(10, 100, 60 + 10 * i, 150), color=(0, 0, 0), fill=(1, 0, 0))
    pix = fitz.Pixmap(fitz.csRGB, fitz.IRect(0, 0, 4, 4), False)
    pix.clear_with(40 * i)
    page.insert_image(fitz.Rect(100, 100, 140, 140), pixmap=pix)
    if i == 3:
        page.insert_text((250, 300), 'testo ruotato della pagina 3', rotate=90)
doc.save(path)
doc.close()
",
                &path,
                &[],
            );
            path
        }

        /// A healthy PDF whose cross-reference offset is then broken: MuPDF still opens it, by
        /// repairing it, and complains about it in its warning buffer while doing so.
        fn broken_xref_pdf(dir: &tempfile::TempDir) -> PathBuf {
            let healthy = plain_pdf(dir, 3);
            let path = dir.path().join("broken-xref.pdf");
            let bytes = std::fs::read(&healthy).unwrap();
            let at = bytes.windows(9).rposition(|w| w == b"startxref").expect("a saved pdf has a startxref");
            let mut broken = bytes[..at].to_vec();
            broken.extend_from_slice(b"startxref\n999999\n%%EOF\n");
            std::fs::write(&path, broken).unwrap();
            path
        }

        /// Three healthy-opening pages whose page 2 has a corrupt content stream: MuPDF complains
        /// only when that page is read, not when the document is opened.
        fn broken_page_two_pdf(dir: &tempfile::TempDir) -> PathBuf {
            let path = dir.path().join("broken-page-two.pdf");
            run_python(
                c"
doc = fitz.open()
for i in range(1, 4):
    page = doc.new_page(-1, 200.0, 300.0)
    page.insert_text((20, 50), f'pagina {i}')
xref = doc[1].get_contents()[0]
doc.update_stream(xref, doc.xref_stream(xref) + b'\\n0 0 m 5 5 l Q Q Q bogusop\\n')
doc.save(path)
doc.close()
",
                &path,
                &[],
            );
            path
        }

        /// Reads, and thereby empties, MuPDF's warning buffer.
        fn take_mupdf_warnings() -> String {
            Python::attach(|py| {
                fitz(py)
                    .getattr("TOOLS")
                    .unwrap()
                    .call_method0("mupdf_warnings")
                    .unwrap()
                    .extract::<String>()
                    .unwrap()
            })
        }

        /// Everything a [`PdfLine`](crate::formats_utils::pdf_extract::pdf_line::PdfLine) holds, in
        /// comparable form: the type itself does not derive `PartialEq`.
        fn line_facts(page: &Page) -> Vec<(String, f32, String, crate::commons::geometry::Rectangle)> {
            page.lines
                .iter()
                .map(|line| (line.font().inner().to_string(), *line.font_size(), line.text().clone(), *line.bbox()))
                .collect()
        }

        fn texts(page: &Page) -> Vec<String> {
            page.lines.iter().map(|line| line.text().trim().to_string()).collect()
        }

        fn assert_is_page(page: &Page, number: u32) {
            assert_eq!(page.number, number, "the page's own number");
            let expected = format!("pagina {number}");
            assert!(texts(page).contains(&expected), "expected a line `{expected}` on page {number}, got {:?}", texts(page));
        }

        fn temp_dir() -> tempfile::TempDir {
            tempfile::tempdir().expect("could not create a temp dir for the test pdfs")
        }

        mod opening {
            use super::*;

            #[test]
            fn reports_the_page_count() {
                let dir = temp_dir();
                for page_count in [1, 2, 4, 10] {
                    let document = LazyDocument::open(&plain_pdf(&dir, page_count), true).expect("a freshly built pdf opens");
                    assert_eq!(document.page_count(), usize::try_from(page_count).unwrap());
                }
            }

            #[test]
            fn a_missing_file_is_an_open_error_naming_the_path() {
                let path = Path::new("/nonexistent/path/does-not-exist.pdf");
                match LazyDocument::open(path, true) {
                    Err(DocumentError::Open { path: reported, .. }) => assert_eq!(reported, path.display().to_string()),
                    Err(other) => panic!("expected DocumentError::Open, got {other:?}"),
                    Ok(_) => panic!("expected DocumentError::Open, got a document"),
                }
            }

            #[test]
            fn a_file_that_is_not_a_pdf_is_an_open_error() {
                let dir = temp_dir();
                let path = dir.path().join("garbage.pdf");
                std::fs::write(&path, "this is not a pdf").unwrap();
                let opened = LazyDocument::open(&path, true);
                assert!(matches!(opened, Err(DocumentError::Open { .. })), "expected DocumentError::Open");
            }

            #[test]
            fn a_directory_is_an_open_error() {
                let dir = temp_dir();
                let opened = LazyDocument::open(dir.path(), true);
                assert!(matches!(opened, Err(DocumentError::Open { .. })), "expected DocumentError::Open");
            }

            /// The file is opened once and kept open: once opened, the document no longer needs
            /// the path, so removing the file must not stop its pages from being read. A reader
            /// that reopened the path for each page would fail here.
            #[test]
            fn keeps_the_file_open_between_pages() {
                let dir = temp_dir();
                let path = plain_pdf(&dir, 3);
                let document = LazyDocument::open(&path, true).expect("a freshly built pdf opens");
                let first = document.page(n(1)).unwrap().expect("page 1 exists");
                assert_is_page(&first, 1);

                std::fs::remove_file(&path).unwrap();

                let last = document.page(n(-1)).unwrap().expect("page 3 exists");
                assert_is_page(&last, 3);
            }
        }

        mod converting_one_page {
            use super::*;

            #[test]
            fn positive_numbers_are_one_based() {
                let dir = temp_dir();
                let document = LazyDocument::open(&plain_pdf(&dir, 4), true).unwrap();
                for number in 1..=4 {
                    let page = document.page(n(number)).unwrap().unwrap_or_else(|| panic!("page {number} of 4 exists"));
                    assert_is_page(&page, u32::try_from(number).unwrap());
                }
            }

            #[test]
            fn negative_numbers_count_from_the_end_and_the_page_keeps_its_real_number() {
                let dir = temp_dir();
                let document = LazyDocument::open(&plain_pdf(&dir, 4), true).unwrap();
                for (number, real) in [(-1, 4), (-2, 3), (-3, 2), (-4, 1)] {
                    let page = document.page(n(number)).unwrap().unwrap_or_else(|| panic!("page {number} of 4 exists"));
                    assert_is_page(&page, real);
                }
            }

            #[test]
            fn the_page_has_its_own_size() {
                let dir = temp_dir();
                let document = LazyDocument::open(&plain_pdf(&dir, 4), true).unwrap();
                let page = document.page(n(3)).unwrap().unwrap();
                assert_eq!(page.size, (203.0, 300.0));
            }

            #[test]
            fn the_page_carries_the_original_pymupdf_dict() {
                let dir = temp_dir();
                let document = LazyDocument::open(&plain_pdf(&dir, 4), true).unwrap();
                let page = document.page(n(2)).unwrap().unwrap();
                Python::attach(|py| {
                    let raw = page.raw().expect("a page converted lazily must keep its pymupdf dict, as load_document_pages does");
                    let width: f64 = raw.bind(py).get_item("width").unwrap().extract().unwrap();
                    assert_eq!(width, 202.0);
                });
            }

            #[test]
            fn past_the_last_page_is_no_page_not_an_error() {
                let dir = temp_dir();
                let document = LazyDocument::open(&plain_pdf(&dir, 4), true).unwrap();
                assert!(matches!(document.page(n(5)), Ok(None)));
                assert!(matches!(document.page(n(i32::MAX)), Ok(None)));
            }

            #[test]
            fn before_the_first_page_counting_from_the_end_is_no_page_not_an_error() {
                let dir = temp_dir();
                let document = LazyDocument::open(&plain_pdf(&dir, 4), true).unwrap();
                assert!(matches!(document.page(n(-5)), Ok(None)));
                assert!(matches!(document.page(n(i32::MIN)), Ok(None)));
            }

            #[test]
            fn a_missing_page_does_not_spoil_the_next_request() {
                let dir = temp_dir();
                let document = LazyDocument::open(&plain_pdf(&dir, 2), true).unwrap();
                assert!(matches!(document.page(n(7)), Ok(None)));
                assert_is_page(&document.page(n(2)).unwrap().expect("page 2 exists"), 2);
            }

            #[test]
            fn the_same_page_can_be_asked_for_twice() {
                let dir = temp_dir();
                let document = LazyDocument::open(&plain_pdf(&dir, 3), true).unwrap();
                let once = document.page(n(2)).unwrap().unwrap();
                let twice = document.page(n(-2)).unwrap().unwrap();
                assert_is_page(&once, 2);
                assert_eq!((once.number, once.size, line_facts(&once)), (twice.number, twice.size, line_facts(&twice)));
            }
        }

        /// The order of the source's own example, `1, 3, 5, 2, -2`, asked of real documents.
        mod the_source_example_order {
            use super::*;

            const ORDER: [i32; 5] = [1, 3, 5, 2, -2];

            /// The 1-based page each request should return, or `None`.
            fn pages_returned(page_count: i64) -> Vec<Option<u32>> {
                let dir = temp_dir();
                let document = LazyDocument::open(&plain_pdf(&dir, page_count), true).unwrap();
                ORDER
                    .iter()
                    .map(|&number| {
                        document.page(n(number)).unwrap_or_else(|e| panic!("page {number} of {page_count}: {e}")).map(|page| {
                            // The page's text, not only its number, says it is the right page.
                            assert_is_page(&page, page.number);
                            page.number
                        })
                    })
                    .collect()
            }

            #[test]
            fn on_one_page() {
                assert_eq!(pages_returned(1), vec![Some(1), None, None, None, None]);
            }

            #[test]
            fn on_two_pages() {
                assert_eq!(pages_returned(2), vec![Some(1), None, None, Some(2), Some(1)]);
            }

            #[test]
            fn on_four_pages() {
                assert_eq!(pages_returned(4), vec![Some(1), Some(3), None, Some(2), Some(3)]);
            }

            #[test]
            fn on_ten_pages() {
                assert_eq!(pages_returned(10), vec![Some(1), Some(3), Some(5), Some(2), Some(9)]);
            }
        }

        /// One conversion, two ways in: a page read lazily is the page `load_document_pages` reads.
        mod same_page_as_the_full_load {
            use super::*;

            fn assert_same_page(lazy: &Page, full: &Page) {
                assert_eq!(lazy.number, full.number, "number");
                assert_eq!(lazy.size, full.size, "size of page {}", full.number);
                assert_eq!(line_facts(lazy), line_facts(full), "lines of page {}", full.number);
                assert_eq!(lazy.images, full.images, "images of page {}", full.number);
                assert_eq!(lazy.drawings, full.drawings, "drawings of page {}", full.number);
                Python::attach(|py| {
                    let lazy_raw = lazy.raw().expect("the lazy page keeps its dict").bind(py);
                    let full_raw = full.raw().expect("the fully loaded page keeps its dict").bind(py);
                    assert!(lazy_raw.eq(full_raw).unwrap(), "pymupdf dict of page {}", full.number);
                });
            }

            fn assert_every_page_coincides(auto_rotate: bool) {
                let dir = temp_dir();
                let path = decorated_pdf(&dir);
                let full = load_document_pages(&path, auto_rotate).expect("the full load succeeds");
                // The comparison would prove nothing on pages with nothing to compare.
                for page in &full {
                    assert!(!page.lines.is_empty(), "the fixture's page {} has lines", page.number);
                    assert!(!page.images.is_empty(), "the fixture's page {} has an image", page.number);
                    assert!(!page.drawings.is_empty(), "the fixture's page {} has a drawing", page.number);
                }

                let document = LazyDocument::open(&path, auto_rotate).unwrap();
                assert_eq!(document.page_count(), full.len());
                // Asked out of order and from both ends, to show nothing depends on the sequence.
                for (number, index) in [(3, 2), (1, 0), (-2, 1)] {
                    let lazy = document.page(n(number)).unwrap().unwrap();
                    assert_same_page(&lazy, &full[index]);
                }
            }

            #[test]
            fn with_auto_rotate() {
                assert_every_page_coincides(true);
            }

            #[test]
            fn without_auto_rotate() {
                assert_every_page_coincides(false);
            }

            /// `auto_rotate` is passed through to the conversion, not fixed by it: on the page with
            /// rotated text the two settings give different lines, and the lazy page follows the
            /// setting it was opened with.
            #[test]
            fn auto_rotate_is_honoured() {
                let dir = temp_dir();
                let path = decorated_pdf(&dir);
                let full_rotated = load_document_pages(&path, true).unwrap();
                let full_unrotated = load_document_pages(&path, false).unwrap();
                assert_ne!(
                    line_facts(&full_rotated[2]),
                    line_facts(&full_unrotated[2]),
                    "the fixture's page 3 must be sensitive to auto_rotate, or this test proves nothing"
                );

                let rotated = LazyDocument::open(&path, true).unwrap().page(n(3)).unwrap().unwrap();
                let unrotated = LazyDocument::open(&path, false).unwrap().page(n(3)).unwrap().unwrap();
                assert_ne!(line_facts(&rotated), line_facts(&unrotated), "the fixture's page 3 must be sensitive to auto_rotate");
            }
        }

        /// MuPDF's warnings must be taken while the interpreter is alive, on this path too: left in
        /// the buffer they crash the embedding process at exit (see `drain_pdf_reader_warnings`).
        ///
        /// MuPDF's warning buffer is **one per process**, and reading it empties it. Under the
        /// parallel test runner every other test that loads a PDF — here, in `input::document`,
        /// `cli::job`, `cli::run` — fills and drains that same buffer at any moment, so a test that
        /// inspects it in-process is nondeterministic both ways: another thread can steal the
        /// warnings the fixture just provoked, or leave its own behind before the check. Holding the
        /// GIL across the whole test does not help, because CPython hands the GIL to waiting threads
        /// every switch interval while it runs PyMuPDF's Python layer.
        ///
        /// So each test here re-runs **itself** in a child process — the test binary, filtered to
        /// that one test, single-threaded — where the buffer belongs to it alone. The parent only
        /// checks that the child ran exactly that test and that it passed. A child that crashed at
        /// exit on undrained warnings would fail the parent too.
        mod mupdf_warnings_are_drained {
            use super::*;

            /// Set in the child's environment: there, the test body runs instead of the respawn.
            const ISOLATED: &str = "FREEPORTS_TEST_IN_OWN_PROCESS";

            /// Runs `body` alone in its own process: directly when already in the child, otherwise
            /// by respawning the test binary on the test called `name` in this module.
            fn in_own_process(name: &str, body: impl FnOnce()) {
                if std::env::var_os(ISOLATED).is_some() {
                    body();
                    return;
                }
                let module = module_path!().split_once("::").expect("a module path inside the crate").1;
                let test = format!("{module}::{name}");
                let output = std::process::Command::new(std::env::current_exe().expect("the test binary's own path"))
                    .args([test.as_str(), "--exact", "--test-threads=1", "--nocapture"])
                    .env(ISOLATED, "1")
                    .output()
                    .expect("the test binary can be respawned");
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(output.status.success(), "`{test}` failed in its own process ({}):\n{stdout}\n{stderr}", output.status);
                assert!(stdout.contains("1 passed"), "`{test}` did not run exactly once in its own process:\n{stdout}\n{stderr}");
            }

            /// Guards against a vacuous pass: the fixture must really make MuPDF complain.
            fn assert_provokes_warnings(path: &Path, page_index: Option<i64>) {
                take_mupdf_warnings();
                Python::attach(|py| {
                    let doc = fitz(py).call_method1("open", (path.to_str().unwrap(),)).unwrap();
                    if let Some(index) = page_index {
                        let page = doc.call_method1("load_page", (index,)).unwrap();
                        page.call_method1("get_text", ("dict",)).unwrap();
                    }
                    doc.call_method0("close").unwrap();
                });
                assert!(!take_mupdf_warnings().is_empty(), "the fixture {} must make MuPDF complain", path.display());
            }

            #[test]
            fn after_opening_a_document_mupdf_had_to_repair() {
                in_own_process("after_opening_a_document_mupdf_had_to_repair", || {
                    let dir = temp_dir();
                    let path = broken_xref_pdf(&dir);
                    assert_provokes_warnings(&path, None);

                    let document = LazyDocument::open(&path, true).expect("MuPDF repairs the xref and opens the document");
                    assert_eq!(take_mupdf_warnings(), "", "opening must drain what MuPDF said while opening");
                    assert_eq!(document.page_count(), 3);
                });
            }

            #[test]
            fn after_converting_a_page_mupdf_complained_about() {
                in_own_process("after_converting_a_page_mupdf_complained_about", || {
                    let dir = temp_dir();
                    let path = broken_page_two_pdf(&dir);
                    assert_provokes_warnings(&path, Some(1));

                    let document = LazyDocument::open(&path, true).unwrap();
                    take_mupdf_warnings();
                    let page = document.page(n(2)).expect("a corrupt content stream is a warning, not an error");
                    assert!(page.is_some(), "page 2 exists");
                    assert_eq!(take_mupdf_warnings(), "", "converting a page must drain what MuPDF said about it");
                });
            }

            #[test]
            fn after_a_failed_open() {
                in_own_process("after_a_failed_open", || {
                    let dir = temp_dir();
                    let path = dir.path().join("garbage.pdf");
                    std::fs::write(&path, "this is not a pdf").unwrap();
                    assert_provokes_warnings_on_failed_open(&path);

                    assert!(LazyDocument::open(&path, true).is_err());
                    assert_eq!(take_mupdf_warnings(), "", "a failed open must drain what MuPDF said while failing");
                });
            }

            /// Like [`assert_provokes_warnings`], for a file PyMuPDF refuses to open at all.
            fn assert_provokes_warnings_on_failed_open(path: &Path) {
                take_mupdf_warnings();
                Python::attach(|py| {
                    assert!(fitz(py).call_method1("open", (path.to_str().unwrap(),)).is_err(), "the fixture must fail to open");
                });
                assert!(!take_mupdf_warnings().is_empty(), "the fixture {} must make MuPDF complain", path.display());
            }

            #[test]
            fn after_the_whole_cycle_of_open_convert_and_drop() {
                in_own_process("after_the_whole_cycle_of_open_convert_and_drop", || {
                    let dir = temp_dir();
                    let path = broken_page_two_pdf(&dir);
                    assert_provokes_warnings(&path, Some(1));
                    {
                        let document = LazyDocument::open(&path, true).unwrap();
                        for number in [1, -1, 2, 9] {
                            document.page(n(number)).unwrap();
                        }
                    }
                    assert_eq!(take_mupdf_warnings(), "", "nothing may be left in the buffer once the document is gone");
                });
            }
        }
    }
}
