//! Helpers shared by the `pdf_extract` pipes: selecting expected text, or failing the page.
//!
//! The selection handed in is an already-resolved [`PdfLineSet`], not a `PdfLineSelection`, which
//! may still be relative and need contextualising against the page's lines first. Resolving that is
//! the caller's job, which keeps this module free of any dependency on the relative-selection
//! machinery.

use crate::commons::sets::Container;
use crate::core::classes::{BlockType, PdfBlock};

use super::pdf_line::PdfLine;
use super::select::pdf_line::PdfLineSet;

#[derive(Debug, thiserror::Error)]
pub enum CommonsError {
    #[error("Pdf block during extraction of \"{name}\" not found")]
    ExpectedTextNotFound { name: String },
    #[error("{source}")]
    PageParseFail {
        #[source]
        source: Box<CommonsError>,
    },
}

/// The text of the first line of `lines`, in the order given, that `selection` selects.
///
/// # Errors
///
/// [`CommonsError::ExpectedTextNotFound`] if no line matches, the empty list included.
pub fn select_expected_text(selection: &PdfLineSet, lines: &[PdfLine], name: &str) -> Result<String, CommonsError> {
    let found = lines.iter().find(|line| selection.contains(line)).map(|line| line.text().clone());
    // The chosen text, not a boolean: it is the line a format author goes looking for in the PDF.
    match &found {
        Some(text) => tracing::trace!(coord_ref_2 = name, found = %text, "selection resolved to a line"),
        None => tracing::debug!(coord_ref_2 = name, "selection matched no line on this page"),
    }
    found.ok_or_else(|| CommonsError::ExpectedTextNotFound { name: name.to_string() })
}

/// Extracts a single bare [`PdfBlock`] from the selected text, or fails the whole page.
///
/// # Errors
///
/// [`CommonsError::PageParseFail`] wrapping the not-found error. The wrapper keeps the cause's
/// message unchanged, so the page-level failure reads as the reason it happened rather than as a
/// generic one.
pub fn extract_text_pdf_block_or_fail_page(
    selection: &PdfLineSet,
    lines: &[PdfLine],
    name: &str,
    type_block: BlockType,
) -> Result<Vec<PdfBlock>, CommonsError> {
    match select_expected_text(selection, lines, name) {
        Ok(text) => Ok(vec![PdfBlock::bare(type_block, text)]),
        Err(source) => Err(CommonsError::PageParseFail { source: Box::new(source) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::{BlockType, PdfBlock};
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;

    fn lines() -> Vec<PdfLine> {
        vec![
            PdfLine::new("Arial", 10.0, "Alpha", (0.0, 0.0, 10.0, 10.0)),
            PdfLine::new("Arial", 10.0, "Beta", (0.0, 10.0, 10.0, 20.0)),
            PdfLine::new("Arial", 10.0, "Beta", (0.0, 20.0, 10.0, 30.0)),
        ]
    }

    mod select_expected_text {
        use super::*;

        #[test]
        fn returns_the_text_of_the_matching_line() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Alpha$");
            assert_eq!(select_expected_text(&selection, &ls, "field").unwrap(), "Alpha");
        }

        #[test]
        fn returns_the_first_match_by_input_order_when_several_lines_match() {
            let ls = vec![
                PdfLine::new("Arial", 8.0, "Skip", (0.0, 0.0, 10.0, 10.0)),
                PdfLine::new("Arial", 12.0, "First", (0.0, 10.0, 10.0, 20.0)),
                PdfLine::new("Arial", 12.0, "Second", (0.0, 20.0, 10.0, 30.0)),
            ];
            let selection = PdfLineSet::select_fontsize(10.0, 14.0);
            assert_eq!(select_expected_text(&selection, &ls, "field").unwrap(), "First");
        }

        #[test]
        fn errors_with_expected_text_not_found_when_nothing_matches() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Gamma$");
            let err = select_expected_text(&selection, &ls, "the field").unwrap_err();
            let CommonsError::ExpectedTextNotFound { name } = err else { panic!("expected ExpectedTextNotFound") };
            assert_eq!(name, "the field");
        }

        #[test]
        fn error_message_names_the_field() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Gamma$");
            let err = select_expected_text(&selection, &ls, "the field").unwrap_err();
            assert!(err.to_string().contains("the field"));
        }

        #[test]
        fn errors_on_an_empty_list_of_lines() {
            let selection = PdfLineSet::select_text("^Anything$");
            let err = select_expected_text(&selection, &[], "field").unwrap_err();
            assert!(matches!(err, CommonsError::ExpectedTextNotFound { .. }));
        }
    }

    mod extract_text_pdf_block_or_fail_page {
        use super::*;

        #[test]
        fn builds_a_single_bare_pdfblock_from_the_matched_text() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Alpha$");
            let blocks = extract_text_pdf_block_or_fail_page(&selection, &ls, "field", BlockType::new("FUND")).unwrap();
            assert_eq!(blocks, vec![PdfBlock::bare(BlockType::new("FUND"), "Alpha")]);
        }

        #[test]
        fn wraps_not_found_into_a_page_parse_fail_carrying_the_leaf_as_source() {
            let ls: Vec<PdfLine> = vec![];
            let selection = PdfLineSet::select_text("^Anything$");
            let err = extract_text_pdf_block_or_fail_page(&selection, &ls, "field", BlockType::new("FUND")).unwrap_err();
            let CommonsError::PageParseFail { source } = &err else { panic!("expected PageParseFail") };
            assert!(matches!(&**source, CommonsError::ExpectedTextNotFound { .. }));
        }

        #[test]
        fn page_parse_fail_message_matches_its_source_message() {
            let ls: Vec<PdfLine> = vec![];
            let selection = PdfLineSet::select_text("^Anything$");
            let err = extract_text_pdf_block_or_fail_page(&selection, &ls, "field", BlockType::new("FUND")).unwrap_err();
            let CommonsError::PageParseFail { source } = &err else { panic!("expected PageParseFail") };
            assert_eq!(err.to_string(), source.to_string());
        }

        #[test]
        fn page_parse_fail_exposes_the_leaf_via_std_error_source() {
            use std::error::Error;
            let ls: Vec<PdfLine> = vec![];
            let selection = PdfLineSet::select_text("^Anything$");
            let err = extract_text_pdf_block_or_fail_page(&selection, &ls, "field", BlockType::new("FUND")).unwrap_err();
            assert!(err.source().is_some());
        }
    }
}
