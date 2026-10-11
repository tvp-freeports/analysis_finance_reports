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
pub fn select_expected_text(
    selection: &PdfLineSet,
    lines: &[PdfLine],
    name: &str,
) -> Result<String, CommonsError> {
    let found = lines
        .iter()
        .find(|line| selection.contains(line))
        .map(|line| line.text().clone());
    // The chosen text, not a boolean: it is the line a format author goes looking for in the PDF.
    match &found {
        Some(text) => {
            tracing::trace!(coord_ref_2 = name, found = %text, "selection resolved to a line")
        }
        None => tracing::debug!(coord_ref_2 = name, "selection matched no line on this page"),
    }
    found.ok_or_else(|| CommonsError::ExpectedTextNotFound {
        name: name.to_string(),
    })
}

/// How many of the selected lines a block's text is made of.
///
/// Named cases rather than an `Option<String>` because the two are different readings of the page
/// and not one value with a default: *the line that says it* and *the line that is written in
/// pieces*. The Python surface still takes a `join=` keyword, the way `A4`'s `Unbalanced` enum is
/// exposed as `drop_unbalanced_row=`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SelectedText {
    /// The **first** selected line, in the order the page gives them, with its text untouched. What
    /// every caller that does not ask otherwise gets.
    #[default]
    FirstLine,
    /// **Every** selected line, in reading order, joined by this separator and squeezed. See
    /// [`select_every_text`].
    EveryLine { separator: String },
}

/// The text of **every** line `selection` selects, in reading order, joined by `separator`.
///
/// Reading order is `(ordinate, abscissa)` and not the ordinate alone: the lines to be joined
/// usually sit **side by side**, so ordering them by ordinate alone would leave them in the order
/// the PDF generator happened to write them, which is not guaranteed to be the order they are read
/// in.
///
/// The result has its whitespace squeezed — runs collapsed, edges trimmed — because the same
/// generator lays the same split title out in two ways, with and without a trailing space on the
/// first piece, and those must not become two different names: the uniqueness key of a fund is its
/// name, so a fund spelled two ways is two funds and every row of one of them is lost (`R24`).
///
/// # Errors
///
/// [`CommonsError::ExpectedTextNotFound`] if no line matches, exactly as [`select_expected_text`]:
/// a page classified to carry a name and not carrying one is a page classified wrongly, and that is
/// worth saying rather than carrying on with an empty name.
pub fn select_every_text(
    selection: &PdfLineSet,
    lines: &[PdfLine],
    name: &str,
    separator: &str,
) -> Result<String, CommonsError> {
    let mut selected: Vec<&PdfLine> = lines
        .iter()
        .filter(|line| selection.contains(line))
        .collect();
    selected.sort_by(|a, b| {
        let (ax, ay) = (a.bbox().as_tuple().0, a.bbox().as_tuple().1);
        let (bx, by) = (b.bbox().as_tuple().0, b.bbox().as_tuple().1);
        ay.total_cmp(&by).then(ax.total_cmp(&bx))
    });
    if selected.is_empty() {
        tracing::debug!(coord_ref_2 = name, "selection matched no line on this page");
        return Err(CommonsError::ExpectedTextNotFound {
            name: name.to_string(),
        });
    }
    let joined = selected
        .iter()
        .map(|line| line.text().as_str())
        .collect::<Vec<_>>()
        .join(separator);
    let squeezed = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    // The pieces as well as the result: a name that came out wrong came out of these, and which
    // fragments were picked up is the thing a format author needs to see.
    tracing::trace!(
        coord_ref_2 = name,
        pieces = selected.len(),
        found = %squeezed,
        "selection resolved to one text joined from several lines"
    );
    Ok(squeezed)
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
    text: &SelectedText,
) -> Result<Vec<PdfBlock>, CommonsError> {
    let found = match text {
        SelectedText::FirstLine => select_expected_text(selection, lines, name),
        SelectedText::EveryLine { separator } => {
            select_every_text(selection, lines, name, separator)
        }
    };
    match found {
        Ok(text) => Ok(vec![PdfBlock::bare(type_block, text)]),
        Err(source) => Err(CommonsError::PageParseFail {
            source: Box::new(source),
        }),
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
            assert_eq!(
                select_expected_text(&selection, &ls, "field").unwrap(),
                "Alpha"
            );
        }

        #[test]
        fn returns_the_first_match_by_input_order_when_several_lines_match() {
            let ls = vec![
                PdfLine::new("Arial", 8.0, "Skip", (0.0, 0.0, 10.0, 10.0)),
                PdfLine::new("Arial", 12.0, "First", (0.0, 10.0, 10.0, 20.0)),
                PdfLine::new("Arial", 12.0, "Second", (0.0, 20.0, 10.0, 30.0)),
            ];
            let selection = PdfLineSet::select_fontsize(10.0, 14.0);
            assert_eq!(
                select_expected_text(&selection, &ls, "field").unwrap(),
                "First"
            );
        }

        #[test]
        fn errors_with_expected_text_not_found_when_nothing_matches() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Gamma$");
            let err = select_expected_text(&selection, &ls, "the field").unwrap_err();
            let CommonsError::ExpectedTextNotFound { name } = err else {
                panic!("expected ExpectedTextNotFound")
            };
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

    /// The behaviour the formats repository had written as a Python shim of its own, moved here
    /// when the two algorithms were merged: every measured case of CARNE-EN23's split title.
    mod select_every_text {
        use super::*;

        /// A line and the box it sits at, as these tests write them.
        type Piece<'a> = (&'a str, (f32, f32, f32, f32));

        /// A page carrying only the lines given, each at the box given.
        fn page(pieces: &[Piece<'_>]) -> Vec<PdfLine> {
            pieces
                .iter()
                .map(|(text, bbox)| PdfLine::new("Arial", 8.0, text, *bbox))
                .collect()
        }

        fn everything() -> PdfLineSet {
            PdfLineSet::select_fontsize(0.0, 1e6)
        }

        #[test]
        fn two_objects_on_one_line_become_one_name() {
            // CARNE-EN23 p65: the title is split where the long dash falls, and the first piece
            // keeps the trailing space — which the squeezing is there to absorb.
            let lines = page(&[
                ("Sidera Funds SICAV – ", (413.4, 68.6, 485.7, 76.0)),
                ("Euro ESG Credit", (485.6, 68.6, 537.8, 76.0)),
            ]);
            assert_eq!(
                select_every_text(&everything(), &lines, "fund", " ").unwrap(),
                "Sidera Funds SICAV – Euro ESG Credit"
            );
        }

        #[test]
        fn reading_order_is_abscissa_within_one_ordinate() {
            // Joined the other way round it would be a name that is not the one printed, and the
            // order the generator wrote them in is not guaranteed.
            let lines = page(&[
                ("Euro ESG Credit", (485.6, 68.6, 537.8, 76.0)),
                ("Sidera Funds SICAV – ", (413.4, 68.6, 485.7, 76.0)),
            ]);
            assert_eq!(
                select_every_text(&everything(), &lines, "fund", " ").unwrap(),
                "Sidera Funds SICAV – Euro ESG Credit"
            );
        }

        #[test]
        fn a_name_on_two_ordinates_reads_top_down() {
            let lines = page(&[
                ("Global High Yield", (413.4, 79.0, 485.7, 87.0)),
                ("Sidera Funds SICAV –", (413.4, 68.6, 485.7, 76.0)),
            ]);
            assert_eq!(
                select_every_text(&everything(), &lines, "fund", " ").unwrap(),
                "Sidera Funds SICAV – Global High Yield"
            );
        }

        #[test]
        fn the_spacing_of_a_single_object_is_squeezed() {
            // A name with a leading space is a different name under the uniqueness key (`R24`).
            let lines = page(&[(
                " Sidera Funds SICAV –  Balanced Growth",
                (413.4, 68.6, 540.0, 76.0),
            )]);
            assert_eq!(
                select_every_text(&everything(), &lines, "fund", " ").unwrap(),
                "Sidera Funds SICAV – Balanced Growth"
            );
        }

        #[test]
        fn an_empty_separator_joins_without_a_space() {
            // A name split **inside a word**, which is the other layout a generator produces.
            let lines = page(&[
                ("Challen", (413.4, 68.6, 450.0, 76.0)),
                ("ge Equity", (450.0, 68.6, 485.7, 76.0)),
            ]);
            assert_eq!(
                select_every_text(&everything(), &lines, "fund", "").unwrap(),
                "Challenge Equity"
            );
        }

        #[test]
        fn a_page_the_selection_takes_nothing_from_is_an_error_like_the_first_line_reading() {
            let lines = page(&[("Sidera Funds SICAV", (413.4, 68.6, 485.7, 76.0))]);
            let selection = PdfLineSet::select_text("^not on this page$");
            let err = select_every_text(&selection, &lines, "the fund", " ").unwrap_err();
            let CommonsError::ExpectedTextNotFound { name } = err else {
                panic!("expected ExpectedTextNotFound")
            };
            assert_eq!(name, "the fund");
        }

        #[test]
        fn the_first_line_reading_is_left_exactly_as_it_was() {
            // The guarantee the nineteen call sites of `new` rest on: squeezing belongs to the
            // joining mode, and a single untouched line is what the other mode returns.
            let lines = page(&[(" Sidera  Funds ", (413.4, 68.6, 485.7, 76.0))]);
            assert_eq!(
                select_expected_text(&everything(), &lines, "fund").unwrap(),
                " Sidera  Funds "
            );
        }
    }

    mod extract_text_pdf_block_or_fail_page {
        use super::*;

        #[test]
        fn the_joining_mode_reaches_the_block() {
            let lines = vec![
                PdfLine::new(
                    "Arial",
                    8.0,
                    "Sidera Funds SICAV – ",
                    (413.4, 68.6, 485.7, 76.0),
                ),
                PdfLine::new("Arial", 8.0, "Euro ESG Credit", (485.6, 68.6, 537.8, 76.0)),
            ];
            let selection = PdfLineSet::select_fontsize(0.0, 1e6);
            let mode = SelectedText::EveryLine {
                separator: " ".to_string(),
            };
            let blocks = extract_text_pdf_block_or_fail_page(
                &selection,
                &lines,
                "fund",
                BlockType::new("FUND"),
                &mode,
            )
            .unwrap();
            assert_eq!(
                blocks,
                vec![PdfBlock::bare(
                    BlockType::new("FUND"),
                    "Sidera Funds SICAV – Euro ESG Credit"
                )]
            );
        }

        #[test]
        fn builds_a_single_bare_pdfblock_from_the_matched_text() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Alpha$");
            let blocks = extract_text_pdf_block_or_fail_page(
                &selection,
                &ls,
                "field",
                BlockType::new("FUND"),
                &SelectedText::FirstLine,
            )
            .unwrap();
            assert_eq!(
                blocks,
                vec![PdfBlock::bare(BlockType::new("FUND"), "Alpha")]
            );
        }

        #[test]
        fn wraps_not_found_into_a_page_parse_fail_carrying_the_leaf_as_source() {
            let ls: Vec<PdfLine> = vec![];
            let selection = PdfLineSet::select_text("^Anything$");
            let err = extract_text_pdf_block_or_fail_page(
                &selection,
                &ls,
                "field",
                BlockType::new("FUND"),
                &SelectedText::FirstLine,
            )
            .unwrap_err();
            let CommonsError::PageParseFail { source } = &err else {
                panic!("expected PageParseFail")
            };
            assert!(matches!(
                &**source,
                CommonsError::ExpectedTextNotFound { .. }
            ));
        }

        #[test]
        fn page_parse_fail_message_matches_its_source_message() {
            let ls: Vec<PdfLine> = vec![];
            let selection = PdfLineSet::select_text("^Anything$");
            let err = extract_text_pdf_block_or_fail_page(
                &selection,
                &ls,
                "field",
                BlockType::new("FUND"),
                &SelectedText::FirstLine,
            )
            .unwrap_err();
            let CommonsError::PageParseFail { source } = &err else {
                panic!("expected PageParseFail")
            };
            assert_eq!(err.to_string(), source.to_string());
        }

        #[test]
        fn page_parse_fail_exposes_the_leaf_via_std_error_source() {
            use std::error::Error;
            let ls: Vec<PdfLine> = vec![];
            let selection = PdfLineSet::select_text("^Anything$");
            let err = extract_text_pdf_block_or_fail_page(
                &selection,
                &ls,
                "field",
                BlockType::new("FUND"),
                &SelectedText::FirstLine,
            )
            .unwrap_err();
            assert!(err.source().is_some());
        }
    }
}
