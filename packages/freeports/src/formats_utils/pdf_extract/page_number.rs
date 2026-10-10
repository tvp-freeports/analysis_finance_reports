//! The **printed** page number, and the pipes that read it.
//!
//! `U7` pairs two pages of one document by the number the page carries in its foot. The resolution
//! has been the engine's since the `carryover` promotion — `fund: {kind: page_promise}` of `esg`,
//! `fund: {kind: nearest_page}` of `labelled_assets` — and what stayed in a format's Python was the
//! **reading**: twenty lines in `content/templates/carryover/`, called by three formats.
//!
//! # Why the number is read off the page and not taken from the index
//!
//! A `pdf_extract` receives the page's dictionary, not its position in the file, and that is not
//! the reason: the two pipelines that have to meet only need to compute the **same** key, and the
//! foot is the one thing both of them see. On UBS-EN23 the printed number differs from the file's
//! — page 16 of the PDF carries `14` — and nothing downstream cares.
//!
//! # The two shapes a format needs
//!
//! | | what it is | who asks for it |
//! |---|---|---|
//! | [`PdfExtractPageNumberStandard`] | the number **is** the block | UBS-EN23's statement, whose `text_filter` looks for the nearest page before |
//! | [`PdfExtractWithPrintedPage`] | the number goes in the **metadata** of another pipe's blocks | the three pipelines that deposit a promise, and FINECO-EN23@IR's section |
//!
//! The second is a decorator and not a pipe of its own because a segment's pipes are
//! **concatenated and not chained**: a pipeline cannot say *read the section, then add the number
//! to what it produced* in two rows of configuration. It is the same lesson the `labelled_assets`
//! promotion wrote down, and the author modules had written the composition by hand.
//!
//! # The rule for choosing the line
//!
//! Of the selected lines, the **lowest one that is a number**. A foot carries the fund's name or
//! the date as often as not, and discarding what is not a number costs less than describing the
//! foot of every format — which is what the three measured selections do anyway, each in its own
//! way: two bands for a document paginated as a book, one centred band for the other two.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::commons::sets::Container;
use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock};
use crate::core::page::Page;
use crate::core::pipeline::{PdfExtractPipe, PipeError};

use super::pdf_line::PdfLine;
use super::relative::RelativeInfo;
use super::select::relative::PdfLineSelection;

/// The printed number of this page, or `None` when it carries none.
///
/// The lowest selected line whose text, trimmed, is all digits. `None` is not an error: a page
/// without a number simply has nothing to leave to another, and the pipes above say so in their own
/// terms.
pub fn printed_page_number(selection: &PdfLineSelection, lines: &[PdfLine]) -> Option<i64> {
    let set = selection.clone().contextualize(lines);
    let mut candidates: Vec<&PdfLine> = lines.iter().filter(|line| set.contains(line)).collect();
    // By the top of the box, so that "lowest" is the last of them: a foot's lines share an ordinate
    // band but not an ordinate.
    candidates.sort_by(|a, b| a.bbox().as_tuple().1.total_cmp(&b.bbox().as_tuple().1));
    for line in candidates.iter().rev() {
        let text = line.text().trim();
        if !text.is_empty()
            && text.chars().all(|character| character.is_ascii_digit())
            && let Ok(number) = text.parse::<i64>()
        {
            return Some(number);
        }
    }
    tracing::debug!("this page carries no printed page number: nothing can be carried over from it");
    None
}

/// The pipe whose block **is** the printed page number.
///
/// Emits one block, whose content is the number as it is written, or none at all. The content is a
/// string and not an integer because that is what the reader of it parses and what every frozen
/// page carries: the type here would be a difference nothing downstream can use.
pub struct PdfExtractPageNumberStandard {
    selection: PdfLineSelection,
    type_block: BlockType,
    name: String,
}

impl PdfExtractPageNumberStandard {
    pub fn new(selection: PdfLineSelection, type_block: BlockType, name: impl Into<String>) -> Self {
        Self { selection, type_block, name: name.into() }
    }
}

impl PdfExtractPipe for PdfExtractPageNumberStandard {
    fn name(&self) -> &str {
        &self.name
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        Ok(match printed_page_number(&self.selection, &page.lines) {
            None => Vec::new(),
            Some(number) => vec![PdfBlock::bare(self.type_block.clone(), number.to_string())],
        })
    }
}

/// Another pipe's blocks, each carrying the printed page number in its metadata.
///
/// # What happens when the page carries no number
///
/// The blocks come out **anyway**, without the key, and the pipe says so at `warn`. The author
/// modules dropped them, and that is the one behaviour this decorator deliberately does not keep:
/// a block dropped here is a sub-fund, a statement or nineteen indicators gone with nothing in the
/// log, while a block that arrives without its key reaches a deposit that already reports exactly
/// that — and a real datum is not to be discarded as a precaution.
pub struct PdfExtractWithPrintedPage {
    inner: Arc<dyn PdfExtractPipe>,
    selection: PdfLineSelection,
    /// The metadata key, `page` in all three measured pipelines and a parameter because the reader
    /// of it — `page_key` of `esg` — has always been one.
    key: String,
}

impl PdfExtractWithPrintedPage {
    pub fn new(inner: Arc<dyn PdfExtractPipe>, selection: PdfLineSelection, key: impl Into<String>) -> Self {
        Self { inner, selection, key: key.into() }
    }
}

impl PdfExtractPipe for PdfExtractWithPrintedPage {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        let mut blocks = self.inner.extract(page)?;
        if blocks.is_empty() {
            return Ok(blocks);
        }
        let Some(number) = printed_page_number(&self.selection, &page.lines) else {
            tracing::warn!(
                pipe = self.inner.name(),
                blocks = blocks.len(),
                "the page carries no printed number in the configured foot: its blocks keep \
                 everything else and cannot be paired with another page"
            );
            return Ok(blocks);
        };
        for block in &mut blocks {
            block.metadata.insert(self.key.clone(), BlockValue::Int(number));
        }
        Ok(blocks)
    }

    /// The decorator adds nothing to the cost: whether it pays to spread is the inner pipe's
    /// answer, and on an author's pipe it is `false`.
    fn scales_with_threads(&self) -> bool {
        self.inner.scales_with_threads()
    }
}

/// The metadata of a block that carries a printed page number, for a caller building one by hand.
pub fn page_metadata(key: &str, number: i64) -> BTreeMap<String, BlockValue> {
    BTreeMap::from([(key.to_string(), BlockValue::Int(number))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
    use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;
    use crate::formats_utils::pdf_extract::select::pdf_line::area::Area;

    fn line(text: &str, bbox: (f32, f32, f32, f32)) -> PdfLine {
        PdfLine::new("frutiger45light", 7.0, text, bbox)
    }

    fn page(lines: Vec<PdfLine>) -> Page {
        Page::new(1, (600.0, 850.0), lines, Vec::new())
    }

    /// The whole foot of the page: the band every measured selection narrows in its own way.
    fn foot() -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::area(Area::new(0.0, 780.0, 1e6, 1e6)))
    }

    /// UBS-EN23's foot, which is a **union of two bands**: the document is paginated as a book, so
    /// the number is on the left of an even page and on the right of an odd one.
    fn two_bands() -> PdfLineSelection {
        let left = PdfLineSet::area(Area::new(0.0, 785.0, 200.0, 1e6));
        let right = PdfLineSet::area(Area::new(450.0, 785.0, 1e6, 1e6));
        OptionallyRelative::Absolute(left | right)
    }

    mod which_line_is_the_number {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The measured shape of a foot: the fund's name beside the number, and sometimes the date.
        fn ubs_foot() -> Page {
            page(vec![
                line("UBS (Lux) Equity Fund", (60.0, 790.0, 260.0, 800.0)),
                line("31 December 2023", (60.0, 800.0, 160.0, 810.0)),
                line("14", (60.0, 812.0, 70.0, 822.0)),
            ])
        }

        #[test]
        fn it_is_the_lowest_line_that_is_a_number() {
            assert_eq!(printed_page_number(&foot(), &ubs_foot().lines), Some(14));
        }

        /// Not the first one found, and not the last line: the rule is both at once, and a foot
        /// that prints the year below the number is the case that tells them apart.
        #[test]
        fn a_number_below_it_that_is_not_a_page_is_still_a_number() {
            let page = page(vec![
                line("14", (60.0, 800.0, 70.0, 810.0)),
                line("2023", (60.0, 812.0, 90.0, 822.0)),
            ]);
            assert_eq!(
                printed_page_number(&foot(), &page.lines),
                Some(2023),
                "the rule is the lowest, and it cannot tell a year from a page: the selection has to"
            );
        }

        #[test]
        fn a_page_with_no_number_in_the_foot_has_none() {
            let page = page(vec![line("UBS (Lux) Equity Fund", (60.0, 790.0, 260.0, 800.0))]);
            assert_eq!(printed_page_number(&foot(), &page.lines), None);
        }

        #[test]
        fn a_number_with_anything_else_in_it_is_not_one() {
            let page = page(vec![
                line("page 14", (60.0, 790.0, 120.0, 800.0)),
                line("14.", (60.0, 800.0, 80.0, 810.0)),
                line("1 4", (60.0, 810.0, 80.0, 820.0)),
            ]);
            assert_eq!(printed_page_number(&foot(), &page.lines), None);
        }

        #[test]
        fn the_surrounding_spaces_do_not_count() {
            let page = page(vec![line("  14 ", (60.0, 790.0, 80.0, 800.0))]);
            assert_eq!(printed_page_number(&foot(), &page.lines), Some(14));
        }

        /// What `P10` was waiting for: with one band, six sections out of fourteen found no number.
        #[test]
        fn the_union_of_two_bands_finds_the_number_on_either_margin() {
            let even = page(vec![line("14", (60.0, 812.0, 70.0, 822.0))]);
            let odd = page(vec![line("15", (540.0, 812.0, 550.0, 822.0))]);
            assert_eq!(printed_page_number(&two_bands(), &even.lines), Some(14));
            assert_eq!(printed_page_number(&two_bands(), &odd.lines), Some(15));
        }

        #[test]
        fn what_sits_between_the_two_bands_is_not_the_number() {
            let page = page(vec![
                line("99", (300.0, 812.0, 310.0, 822.0)),
                line("14", (60.0, 812.0, 70.0, 822.0)),
            ]);
            assert_eq!(printed_page_number(&two_bands(), &page.lines), Some(14));
        }
    }

    mod the_pipe_whose_block_is_the_number {
        use super::*;
        use pretty_assertions::assert_eq;

        fn pipe() -> PdfExtractPageNumberStandard {
            PdfExtractPageNumberStandard::new(foot(), BlockType::new("ASSETS_PAGE"), "assets page")
        }

        #[test]
        fn one_block_carrying_the_number_as_text() {
            let page = page(vec![line("14", (60.0, 812.0, 70.0, 822.0))]);
            let blocks = pipe().extract(&page).expect("the pipe reads the page");
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].content.as_str(), Some("14"));
            assert_eq!(blocks[0].type_block, BlockType::new("ASSETS_PAGE"));
            assert!(blocks[0].metadata.is_empty(), "the number is the content, not a key");
        }

        /// No block, and no error: a page without a number has nothing to pair, and the
        /// `text_filter` that looks for this block already says so when it is missing.
        #[test]
        fn no_number_is_no_block_and_no_failure() {
            let page = page(vec![line("UBS (Lux) Equity Fund", (60.0, 790.0, 260.0, 800.0))]);
            assert!(pipe().extract(&page).expect("no number is not a failure").is_empty());
        }
    }

    mod the_decorator {
        use super::*;
        use pretty_assertions::assert_eq;

        /// A pipe standing in for whatever produces the blocks — a section, a joined name.
        struct Inner(Vec<PdfBlock>);

        impl PdfExtractPipe for Inner {
            fn name(&self) -> &str {
                "the inner pipe"
            }

            fn extract(&self, _page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
                Ok(self.0.clone())
            }
        }

        fn decorated(blocks: Vec<PdfBlock>) -> PdfExtractWithPrintedPage {
            PdfExtractWithPrintedPage::new(Arc::new(Inner(blocks)), foot(), "page")
        }

        fn two_blocks() -> Vec<PdfBlock> {
            vec![
                PdfBlock::bare(BlockType::new("RELEVANT_BLOCK"), "the first indicator"),
                PdfBlock::new(
                    BlockType::new("RELEVANT_BLOCK"),
                    BTreeMap::from([("entry".to_string(), BlockValue::Int(2))]),
                    "the second",
                ),
            ]
        }

        #[test]
        fn every_block_gains_the_number_and_keeps_what_it_had() {
            let page = page(vec![line("894", (60.0, 812.0, 80.0, 822.0))]);
            let blocks = decorated(two_blocks()).extract(&page).expect("the pipe reads the page");
            assert_eq!(blocks.len(), 2);
            for block in &blocks {
                assert_eq!(block.metadata.get("page"), Some(&BlockValue::Int(894)));
            }
            assert_eq!(blocks[1].metadata.get("entry"), Some(&BlockValue::Int(2)));
        }

        #[test]
        fn the_key_is_the_one_the_entry_names() {
            let page = page(vec![line("894", (60.0, 812.0, 80.0, 822.0))]);
            let pipe = PdfExtractWithPrintedPage::new(Arc::new(Inner(two_blocks())), foot(), "sheet");
            let blocks = pipe.extract(&page).expect("the pipe reads the page");
            assert_eq!(blocks[0].metadata.get("sheet"), Some(&BlockValue::Int(894)));
            assert_eq!(blocks[0].metadata.get("page"), None);
        }

        /// The one behaviour this decorator does not keep from the author modules, which dropped
        /// the blocks: nineteen indicators gone with nothing in the log is worse than a block whose
        /// key is missing, and the deposit that reads the key already reports it.
        #[test]
        fn a_page_without_a_number_keeps_its_blocks() {
            let page = page(vec![line("UBS (Lux) Equity Fund", (60.0, 790.0, 260.0, 800.0))]);
            let blocks = decorated(two_blocks()).extract(&page).expect("the pipe reads the page");
            assert_eq!(blocks.len(), 2);
            assert!(blocks.iter().all(|block| !block.metadata.contains_key("page")));
        }

        #[test]
        fn a_pipe_that_produced_nothing_is_left_alone() {
            let page = page(vec![line("894", (60.0, 812.0, 80.0, 822.0))]);
            assert!(decorated(Vec::new()).extract(&page).expect("nothing to decorate").is_empty());
        }

        /// The log and the error messages keep naming the pipe that reads the page, not the
        /// decorator: what failed is the section, and that is what has to be looked for.
        #[test]
        fn the_name_is_the_inner_pipes() {
            assert_eq!(PdfExtractPipe::name(&decorated(Vec::new())), "the inner pipe");
        }
    }
}
