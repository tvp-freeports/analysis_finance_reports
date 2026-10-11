//! Lines of a page, **grouped by the gap between them**: the shape three measured pipelines share.
//!
//! A selection, and then *where the lines stop being one thing and become the next*. It is not a
//! [`section`](super::section): a section is opened by a **heading** and closed by the next one,
//! while here there is no heading at all — the band is given by the selection, often by two text
//! anchors, and what separates one entry from the next is only the vertical distance.
//!
//! # The three sites, and the two ways they end
//!
//! | who | the band | the gap | what comes out |
//! |---|---|---|---|
//! | ANIMA-EN23, the merges | between `Funds merged during the financial year` and `Dividends Paid` | 20 | **one block per group**, its lines joined by a space |
//! | KAIROS-EN23, the merges | between `merged during the year` and `Fund's shares`, less the all-space lines | 20 | **the first group only**, joined by nothing |
//! | FINECO-EN23@LUX, the managers | a column of the page | 20 | **one block per line**, each carrying its group number |
//!
//! So two cells decide the ending: `join`, which is the separator when a group becomes one block,
//! and `take`, which says whether every group counts or only the first. Without `join` the lines
//! come out one by one and the group number travels in the metadata, which is what a `text_filter`
//! that reads entries off those numbers needs.
//!
//! # The lines are sorted before they are grouped
//!
//! [`get_groups`] follows the order of the **sorted** keys, and the author modules zipped its
//! result against the selection order — which agrees only while the band is a single column read
//! top to bottom. Here the selected lines are sorted by ordinate, then by abscissa, before being
//! grouped, so the group a line is given is its own in every layout.

use std::collections::BTreeMap;

use crate::commons::sets::Container;
use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock};
use crate::core::page::Page;
use crate::core::pipeline::{PdfExtractPipe, PipeError};

use super::pdf_line::PdfLine;
use super::position::get_groups;
use super::relative::RelativeInfo;
use super::select::relative::PdfLineSelection;

/// How many of the groups a page's blocks come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupedTake {
    /// Every group. The default, and what a page with several entries wants.
    #[default]
    All,
    /// The first group alone, for a band whose first entry is the datum and whose rest is the
    /// prose that follows it.
    First,
}

/// What the pipe is told.
///
/// No `Debug`: a selection is a tree of closures and does not derive one, as every other pipe's
/// arguments in this tree.
#[derive(Clone)]
pub struct GroupedLinesArgs {
    pub selection: PdfLineSelection,
    pub block_type: BlockType,
    pub name: String,
    /// The vertical distance at or above which a line starts a **new** group.
    pub gap: f32,
    /// The separator that makes a group into **one** block. Absent emits one block per line, each
    /// carrying its group number under `group`.
    pub join: Option<String>,
    pub take: GroupedTake,
}

impl GroupedLinesArgs {
    pub fn new(
        selection: PdfLineSelection,
        block_type: BlockType,
        name: impl Into<String>,
        gap: f32,
    ) -> Self {
        Self {
            selection,
            block_type,
            name: name.into(),
            gap,
            join: None,
            take: GroupedTake::All,
        }
    }
}

/// The metadata key a line's group travels under.
pub const GROUP_KEY: &str = "group";

/// The pipe itself. See the module documentation.
pub struct PdfExtractGroupedLinesStandard {
    args: GroupedLinesArgs,
}

impl PdfExtractGroupedLinesStandard {
    pub fn new(args: GroupedLinesArgs) -> Self {
        Self { args }
    }

    /// The selected lines, in reading order, with the group each one belongs to.
    fn grouped<'a>(&self, page: &'a Page) -> Vec<(i64, &'a PdfLine)> {
        let set = self.args.selection.clone().contextualize(&page.lines);
        let mut lines: Vec<&PdfLine> = page
            .lines
            .iter()
            .filter(|line| set.contains(line))
            .collect();
        lines.sort_by(|a, b| {
            let (ax0, ay0, ..) = a.bbox().as_tuple();
            let (bx0, by0, ..) = b.bbox().as_tuple();
            ay0.total_cmp(&by0).then(ax0.total_cmp(&bx0))
        });
        let owned: Vec<PdfLine> = lines.iter().map(|line| (*line).clone()).collect();
        // `get_groups` refuses an empty page, which here is not an error: a page the selection
        // finds nothing on simply has no entry.
        match get_groups(&owned, self.args.gap, true) {
            Err(_) => Vec::new(),
            Ok(groups) => groups.into_iter().zip(lines).collect(),
        }
    }
}

impl PdfExtractPipe for PdfExtractGroupedLinesStandard {
    fn name(&self) -> &str {
        &self.args.name
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        let grouped = self.grouped(page);
        if grouped.is_empty() {
            return Ok(Vec::new());
        }

        let Some(separator) = &self.args.join else {
            // One block per line, the group in the metadata.
            return Ok(grouped
                .into_iter()
                .map(|(group, line)| {
                    PdfBlock::new(
                        self.args.block_type.clone(),
                        BTreeMap::from([(GROUP_KEY.to_string(), BlockValue::Int(group))]),
                        line.text().as_str(),
                    )
                })
                .collect());
        };

        // One block per group, its lines joined — in the order they were read.
        let mut texts: BTreeMap<i64, Vec<&str>> = BTreeMap::new();
        for (group, line) in grouped {
            texts.entry(group).or_default().push(line.text().as_str());
        }
        let groups: Vec<(i64, Vec<&str>)> = match self.args.take {
            GroupedTake::All => texts.into_iter().collect(),
            GroupedTake::First => texts.into_iter().take(1).collect(),
        };
        Ok(groups
            .into_iter()
            .map(|(_, lines)| PdfBlock::bare(self.args.block_type.clone(), lines.join(separator)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
    use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;

    fn line(text: &str, y: f32) -> PdfLine {
        PdfLine::new("arialnarrow", 11.0, text, (320.0, y, 500.0, y + 10.0))
    }

    fn page(lines: Vec<PdfLine>) -> Page {
        Page::new(1, (600.0, 850.0), lines, Vec::new())
    }

    fn font() -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_font("arialnarrow"))
    }

    /// Two entries, three lines each the first and one the second: FINECO-EN23@LUX's shape.
    fn two_entries() -> Page {
        page(vec![
            line("Pictet Asset Management", 100.0),
            line("(the Investment Manager of", 112.0),
            line("the CoRe Series Fund)", 124.0),
            line("Candriam", 180.0),
        ])
    }

    fn args(gap: f32) -> GroupedLinesArgs {
        GroupedLinesArgs::new(font(), BlockType::new("INV_MAN"), "inv managers", gap)
    }

    fn run(args: GroupedLinesArgs, page: &Page) -> Vec<PdfBlock> {
        PdfExtractGroupedLinesStandard::new(args)
            .extract(page)
            .expect("the pipe reads the page")
    }

    mod one_block_per_line {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn each_line_carries_the_group_it_belongs_to() {
            let blocks = run(args(20.0), &two_entries());
            let groups: Vec<i64> = blocks
                .iter()
                .map(|b| b.metadata[GROUP_KEY].as_int().expect("a group is a number"))
                .collect();
            assert_eq!(
                groups,
                [0, 0, 0, 1],
                "three lines of the first entry, then the second"
            );
        }

        #[test]
        fn the_text_is_the_lines_own() {
            let blocks = run(args(20.0), &two_entries());
            assert_eq!(blocks[3].content.as_str(), Some("Candriam"));
        }

        /// The gap is the only thing that separates two entries, so it decides how many there are.
        #[test]
        fn the_gap_decides_where_an_entry_ends() {
            let blocks = run(args(5.0), &two_entries());
            let groups: Vec<i64> = blocks
                .iter()
                .map(|b| b.metadata[GROUP_KEY].as_int().expect("a group"))
                .collect();
            assert_eq!(groups, [0, 1, 2, 3], "every line is its own entry");
        }
    }

    mod one_block_per_group {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_lines_of_a_group_are_joined_in_reading_order() {
            let mut args = args(20.0);
            args.join = Some(" ".to_string());
            let blocks = run(args, &two_entries());
            assert_eq!(blocks.len(), 2);
            assert_eq!(
                blocks[0].content.as_str(),
                Some("Pictet Asset Management (the Investment Manager of the CoRe Series Fund)")
            );
            assert_eq!(blocks[1].content.as_str(), Some("Candriam"));
            assert!(
                blocks[0].metadata.is_empty(),
                "a joined group is one datum, not an entry number"
            );
        }

        #[test]
        fn the_separator_may_be_nothing() {
            let mut args = args(20.0);
            args.join = Some(String::new());
            let blocks = run(args, &two_entries());
            assert!(
                blocks[0]
                    .content
                    .as_str()
                    .expect("text")
                    .starts_with("Pictet Asset Management(the")
            );
        }

        /// KAIROS-EN23: the first group is the datum and what follows it is prose.
        #[test]
        fn take_first_keeps_one_group() {
            let mut args = args(20.0);
            args.join = Some(" ".to_string());
            args.take = GroupedTake::First;
            let blocks = run(args, &two_entries());
            assert_eq!(blocks.len(), 1);
            assert_eq!(
                blocks[0].content.as_str(),
                Some("Pictet Asset Management (the Investment Manager of the CoRe Series Fund)")
            );
        }
    }

    mod what_an_empty_page_does {
        use super::*;

        /// A page the selection finds nothing on has no entry, and that is not a failure: the
        /// grouping function refuses an empty list, and the pipe answers with no blocks.
        #[test]
        fn no_selected_line_is_no_block_and_no_error() {
            let empty = page(vec![PdfLine::new(
                "times",
                11.0,
                "another family",
                (0.0, 0.0, 10.0, 10.0),
            )]);
            assert!(run(args(20.0), &empty).is_empty());
        }
    }

    mod the_order_the_groups_are_assigned_in {
        use super::*;
        use pretty_assertions::assert_eq;

        /// `get_groups` follows the order of the **sorted** keys, and the author modules zipped it
        /// against the selection order. The pipe sorts first, so a band whose lines do not arrive
        /// in reading order still gets the right groups.
        #[test]
        fn lines_are_sorted_before_they_are_grouped() {
            let shuffled = page(vec![
                line("second entry", 180.0),
                line("first entry", 100.0),
            ]);
            let blocks = run(args(20.0), &shuffled);
            assert_eq!(blocks[0].content.as_str(), Some("first entry"));
            assert_eq!(blocks[0].metadata[GROUP_KEY].as_int(), Some(0));
            assert_eq!(blocks[1].metadata[GROUP_KEY].as_int(), Some(1));
        }
    }
}
