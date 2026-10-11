//! [`PdfExtractSectionStandard`]: *a heading opens a section, and underneath it there is the list*
//! — and [`SectionEnd`], the answer to *where does it end*, which two algorithms need.
//!
//! A report writes its lists under a heading, and **where the list ends is never written down**: it
//! ends where the next heading begins. That calculation is the same one in two places — it is
//! `stop` in [`super::labelled_value::PdfExtractLabelledValueStandard`] and the section's family
//! here — so it is written once, in [`SectionEnd`], and neither algorithm can drift from the other.
//!
//! # Where a section starts, which is not where it looks
//!
//! Not *below* the heading: **level with it or below**. On FINECO-EN23@IR p3 the value of the
//! `Manager` section sits at `y=226.8`, the same ordinate as the heading beside it, because the
//! page is two columns and not a list — so a strict "below" drops exactly the line the section
//! exists for, keeps the address, and looks perfectly correct on every format whose list really is
//! underneath. The floor stays strict, because the next heading is not part of the section: two
//! different comparisons on the two ends, each with its own measurement.
//!
//! # Why the selection algebra cannot express it
//!
//! `PdfLineSelection::area_from_bounds` takes, for each side, either a coordinate or one read off
//! **the first line of its sub-selection in page order** (`select/relative.rs`). "First in page
//! order" is not "first *below the anchor*", and on FINECO-EN23@IR p3 the difference is the whole
//! answer: the ten bold headings of the left margin start at `y=78.6`, the anchor `Manager` is at
//! `y=226.8`, and the heading that closes its section is at `y=295.9`. Asking the algebra for the
//! bound gives 78.6 — above the anchor, so the section comes out empty. That is why the author of
//! `fineco_en23_ir/managment_company.py` wrote the `min(...)` by hand.

use std::collections::BTreeMap;

use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock};
use crate::core::page::Page;
use crate::core::pipeline::{PdfExtractPipe, PipeError};

use super::pdf_line::PdfLine;
use super::position::get_groups;
use super::relative::RelativeInfo;
use super::select::relative::PdfLineSelection;
use super::standard_funcs::PdfExtractStandardFuncsError;
use super::tabularizer::{TableCoordinatesConfig, get_table_coordinates_from_lines};
use crate::commons::sets::Container;

/// What closes the section an anchor opens.
///
/// Shared by the two algorithms because it is one idea: *the next line that counts as a heading*.
/// The anchor is a label for `A1` and a section heading for `A2`, and nothing else differs.
#[derive(Clone, Default)]
pub enum SectionEnd {
    /// The next line carrying the anchor's own `(font, size)` pair.
    ///
    /// The pair rather than the font alone, measured: on EURIZON-EN23 p9 the heading
    /// `INVESTMENT MANAGERS` is frutiger-lightitalic 11.97 and the parenthesised specifications
    /// inside its own section are frutiger-lightitalic **8.98** — the same font name. A boundary
    /// computed on the font would close the section after the first manager.
    #[default]
    SameFamilyAsAnchor,
    /// The next line this selection takes.
    ///
    /// Needed wherever the heading is not typographically distinct from its list. On
    /// AMUNDI-EN24 p510 four sections (`launched`, `renamed`, `liquidated`, `merged`) share arialmt
    /// 7 with all of their rows, and the only thing that tells a heading from a row is its text.
    Selection(PdfLineSelection),
    /// A bare ordinate, the same on every page.
    ///
    /// The crude one, and the one two formats already use: `fineco_en23_ir/inv_managers.py` closes
    /// its table at `y1=520.0` because what follows is not a heading of any kind.
    Ordinate(f32),
}

impl SectionEnd {
    /// The ordinate below which a line is out of the anchor's section.
    ///
    /// `f32::INFINITY` when nothing closes it: a section that runs to the foot of the page is the
    /// normal case on the opening page of a list that continues on the next one.
    pub fn bottom_below(&self, anchor: &PdfLine, lines: &[PdfLine]) -> f32 {
        let end = match self {
            SectionEnd::SameFamilyAsAnchor => section_bottom(anchor, lines, |line| {
                line.font() == anchor.font() && line.font_size() == anchor.font_size()
            }),
            SectionEnd::Selection(selection) => {
                let set = selection.clone().contextualize(lines);
                section_bottom(anchor, lines, |line| set.contains(line))
            }
            SectionEnd::Ordinate(y) => return *y,
        };
        end.map_or(f32::INFINITY, |line| line.bbox().as_tuple().1)
    }
}

/// The bottom of the section a line opens: the first line **strictly below** it that belongs to
/// `family`, or `None` when there is none.
///
/// "Strictly below" is on the **top edge**: a line whose box merely overlaps the anchor's is beside
/// it, not under it. The minimum rather than the first met, because a page's lines are not
/// guaranteed to arrive sorted.
pub fn section_bottom<'a, F>(
    anchor: &PdfLine,
    lines: &'a [PdfLine],
    belongs: F,
) -> Option<&'a PdfLine>
where
    F: Fn(&PdfLine) -> bool,
{
    let anchor_top = anchor.bbox().as_tuple().1;
    lines
        .iter()
        .filter(|line| line.bbox().as_tuple().1 > anchor_top && belongs(line))
        .min_by(|a, b| {
            a.bbox()
                .as_tuple()
                .1
                .partial_cmp(&b.bbox().as_tuple().1)
                .expect("page ordinates are finite")
        })
}

/// What the pipe puts in each block: the bare line, or the line with its place in a table.
#[derive(Clone, Default)]
pub enum SectionEmit {
    /// One block per line, with no metadata. The default, and what a list needs.
    #[default]
    Lines,
    /// One block per line, carrying `table-row` and `table-col` — the same two keys
    /// `TextFilterInvestmentsStandard` already reads.
    ///
    /// Needed because a pipe that returned blocks would otherwise put a table out of reach: the
    /// coordinates are computed on the **lines**, and by then a format has none left. Two of the
    /// sixteen manager formats read a table with one row per fund (FINECO-EN23@IR note 1,
    /// DANSKEINVEST-EN24 p352), and the ESG tables are in the same position.
    Rows(Box<TableCoordinatesConfig>),
    /// One block per line, carrying `group` — the index of the **entry** the line belongs to,
    /// counted from the top of the section.
    ///
    /// For the sections where the typography says nothing: a name, its address and its
    /// qualification are the same font as the next name, and the only thing that separates one
    /// entry from the next is the **vertical gap**. Measured on DANSKEINVEST-EN24 p5–6 (10.8
    /// points between two lines of one entry, 19.2 to 21.6 between two entries) and on CARNE-EN23
    /// p4 (9.2 within, 18.4 between): `gap` is the distance at or above which a line opens a new
    /// entry, so it goes between the two — 15.0 on both.
    ///
    /// The blocks come out sorted by ordinate and then by abscissa, which is what the readers of
    /// the `inv_managers` template expect: they walk the blocks in page order and start a new
    /// declaration whenever `group` changes.
    Entries { gap: f32 },
}

/// The construction parameters of [`PdfExtractSectionStandard`].
///
/// Three are required — the heading, the block it emits and the pipe's name — and everything else
/// has a default, on the model of `InvestmentsStandardArgs` and [`super::labelled_value::LabelledValueArgs`].
pub struct SectionArgs {
    /// How the heading that opens the section is recognised (`R28`: a selection, never a literal).
    ///
    /// Constrain it by **size and area**, not by text alone. Measured on ASTERIA-EN24, where
    /// `Investment Managers` appears on every page in helvetica 4.88 at `y=807.6` — the footer logo
    /// — while the datum is `Investment Manager and Global Distributor:` in centurygothic-bold 9.98
    /// at `y=101.7`. A heading matched by text would take the footer, and the section would come out
    /// empty with nobody the wiser.
    pub heading: PdfLineSelection,
    /// The kind of block the pipe emits.
    pub block_type: BlockType,
    /// The pipe's name, which is what appears in the log and in error messages.
    pub name: String,
    /// What counts as the next heading, and so closes the section.
    pub family: SectionEnd,
    /// A second floor, which wins if it comes first. Independent of [`Self::family`] on purpose:
    /// `eurizon_en23` closes on a heading, `fineco_en23_ir` on a bare ordinate, and nothing says a
    /// format may not want both.
    pub bottom: Option<SectionEnd>,
    /// Which lines of the section count. `None` takes them all.
    ///
    /// The analogue of `value` in `A1`, and needed in two measured forms: a column (FINECO-EN23@IR
    /// p3 has its headings at `x=52.4` and their values at `x=279.1`) and noise (the single-space
    /// line at `y=270.4` of AMUNDI-EN24 p510, which formats remove today with a hand-written
    /// deselection).
    pub body: Option<PdfLineSelection>,
    /// Whether the heading itself is part of what is emitted.
    ///
    /// It is, wherever the heading **carries** the datum instead of announcing it: AMUNDI-EN24 p510
    /// puts the date in `The following Sub-Funds have been renamed on 15 March 2024:` and leaves
    /// twenty-one dateless rows under it, and on p360 the bold `Assets` carries the four totals on
    /// its own line.
    pub include_heading: bool,
    /// What each block carries.
    pub emit: SectionEmit,
}

impl SectionArgs {
    /// The usual defaults: the section closes at the next heading of the heading's own typographic
    /// family, every line of it counts, the heading itself does not, and each line is a bare block.
    pub fn new(heading: PdfLineSelection, block_type: BlockType, name: impl Into<String>) -> Self {
        Self {
            heading,
            block_type,
            name: name.into(),
            family: SectionEnd::default(),
            bottom: None,
            body: None,
            include_heading: false,
            emit: SectionEmit::default(),
        }
    }
}

/// *A heading opens a section, and underneath it there is the list.*
///
/// The direct application of `R13`: a heading does not travel between formats, it is **found** — by
/// keywords and page structure — and so it is a parameter.
///
/// # Two pipes rather than one parameter
///
/// A section whose lines play **different roles** — EURIZON-EN23 p9 writes the managers in
/// frutiger-black and the sub-fund specifications in frutiger-lightitalic, and the `text_filter`
/// needs to tell them apart — is read by **two of these pipes on the same page**: same `heading`,
/// same `family`, different `body` and `block_type`. That is `R27` applied inside a segment, and it
/// is what `fineco_en23_ir/investments` already does with three `pdf_extract`. It costs finding the
/// heading twice, which is a minimum over one page.
pub struct PdfExtractSectionStandard {
    heading: PdfLineSelection,
    block_type: BlockType,
    name: String,
    family: SectionEnd,
    bottom: Option<SectionEnd>,
    body: Option<PdfLineSelection>,
    include_heading: bool,
    emit: SectionEmit,
}

impl PdfExtractSectionStandard {
    pub fn new(args: SectionArgs) -> Self {
        let SectionArgs {
            heading,
            block_type,
            name,
            family,
            bottom,
            body,
            include_heading,
            emit,
        } = args;
        Self {
            heading,
            block_type,
            name,
            family,
            bottom,
            body,
            include_heading,
            emit,
        }
    }

    /// The heading that opens the section: the **topmost** line the selection takes.
    ///
    /// Topmost rather than first in extraction order, because a page's lines arrive in no
    /// guaranteed order and the section that opens highest is the one a reader would call first.
    fn heading_of<'a>(&self, lines: &'a [PdfLine]) -> Option<&'a PdfLine> {
        let set = self.heading.clone().contextualize(lines);
        lines
            .iter()
            .filter(|line| set.contains(*line))
            .min_by(|a, b| {
                a.bbox()
                    .as_tuple()
                    .1
                    .partial_cmp(&b.bbox().as_tuple().1)
                    .expect("page ordinates are finite")
            })
    }

    /// The lines of the section, heading included or not, in page order.
    fn section<'a>(&self, heading: &PdfLine, lines: &'a [PdfLine]) -> Vec<&'a PdfLine> {
        let floor = self
            .bottom
            .as_ref()
            .map_or(f32::INFINITY, |end| end.bottom_below(heading, lines))
            .min(self.family.bottom_below(heading, lines));
        let top = heading.bbox().as_tuple().1;
        let body = self
            .body
            .clone()
            .map(|selection| selection.contextualize(lines));

        lines
            .iter()
            // Level with the heading counts, and the measurement that settles it is FINECO-EN23@IR
            // p3: the section's own value sits at `y=226.8`, the **same** ordinate as the `Manager`
            // heading beside it, because the page is two columns and not a list. A strict "below"
            // would drop exactly the line the section exists for.
            .filter(|line| line.bbox().as_tuple().1 >= top && line.bbox().as_tuple().1 < floor)
            .filter(|line| self.include_heading || !std::ptr::eq(*line, heading))
            .filter(|line| body.as_ref().is_none_or(|set| set.contains(*line)))
            .collect()
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        // A section that is not on this page is a normal answer, not a failure: the same reading
        // `take=ALL` has in `A1`, and the one the three-page-class split of a long list depends on.
        let Some(heading) = self.heading_of(&page.lines) else {
            tracing::debug!("{}: no heading on this page, so no section", self.name);
            return Ok(Vec::new());
        };
        let section = self.section(heading, &page.lines);
        tracing::debug!(heading = %heading.text(), lines = section.len(), "{} read a section", self.name);
        if section.is_empty() {
            // A heading with nothing under it: the same normal answer as a page without the
            // heading, and the guard the entry grouping needs, since grouping nothing is an error.
            return Ok(Vec::new());
        }

        match &self.emit {
            SectionEmit::Lines => Ok(section
                .iter()
                .map(|line| PdfBlock::bare(self.block_type.clone(), line.text().as_str()))
                .collect()),
            SectionEmit::Entries { gap } => {
                // Sorted before grouping, and not after: `get_groups` returns one id per line **in
                // the order of the sorted keys**, so the two orders have to be the same one or
                // every id lands on the wrong line.
                let mut sorted = section;
                sorted.sort_by(|a, b| {
                    let (ax, ay) = (a.bbox().as_tuple().0, a.bbox().as_tuple().1);
                    let (bx, by) = (b.bbox().as_tuple().0, b.bbox().as_tuple().1);
                    (ay, ax)
                        .partial_cmp(&(by, bx))
                        .expect("page coordinates are finite")
                });
                let owned: Vec<PdfLine> = sorted.iter().map(|line| (*line).clone()).collect();
                let groups = get_groups(&owned, *gap, true).map_err(|_| {
                    // The only error `get_groups` has is the empty input, which the early return
                    // above has already excluded.
                    PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound {
                        name: self.name.clone(),
                    }
                })?;
                Ok(sorted
                    .iter()
                    .zip(groups)
                    .map(|(line, group)| {
                        let metadata =
                            BTreeMap::from([("group".to_string(), BlockValue::from(group))]);
                        PdfBlock::new(
                            self.block_type.clone(),
                            metadata,
                            BlockValue::from(line.text().as_str()),
                        )
                    })
                    .collect())
            }
            SectionEmit::Rows(config) => {
                let owned: Vec<PdfLine> = section.iter().map(|line| (*line).clone()).collect();
                let coords = get_table_coordinates_from_lines(&owned, config)
                    .map_err(PdfExtractStandardFuncsError::from)?;
                Ok(section
                    .iter()
                    .zip(coords)
                    .map(|(line, (row, col))| {
                        let metadata = BTreeMap::from([
                            ("table-row".to_string(), BlockValue::from(row as i64)),
                            ("table-col".to_string(), BlockValue::from(col as i64)),
                        ]);
                        PdfBlock::new(
                            self.block_type.clone(),
                            metadata,
                            BlockValue::from(line.text().as_str()),
                        )
                    })
                    .collect())
            }
        }
    }
}

impl PdfExtractPipe for PdfExtractSectionStandard {
    fn name(&self) -> &str {
        &self.name
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page)
            .map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
    use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;

    fn line(font: &str, size: f32, text: &str, bbox: (f32, f32, f32, f32)) -> PdfLine {
        PdfLine::new(font, size, text, bbox)
    }

    fn text_sel(pattern: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_text(pattern))
    }

    fn font_sel(font: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_font(font))
    }

    fn page(lines: Vec<PdfLine>) -> Page {
        Page::new(1, (600.0, 850.0), lines, Vec::new())
    }

    fn args(heading: PdfLineSelection) -> SectionArgs {
        SectionArgs::new(heading, BlockType::new("SECTION_LINE"), "a section")
    }

    fn texts(blocks: &[PdfBlock]) -> Vec<String> {
        blocks
            .iter()
            .map(|block| {
                block
                    .content
                    .as_str()
                    .expect("a section line is text")
                    .to_string()
            })
            .collect()
    }

    fn run(args: SectionArgs, page: &Page) -> Vec<PdfBlock> {
        PdfExtractSectionStandard::new(args)
            .call(page)
            .expect("the pipe reads the page")
    }

    /// FINECO-EN23@IR p3, measured: ten bold headings down the left margin at `x≈52`, each one's
    /// value beside it at `x=279.1`, **on the same ordinate**.
    fn fineco_ir_p3() -> Page {
        page(vec![
            line(
                "timesnewromanbold",
                10.02,
                "Directors and Other Information",
                (52.5, 78.6, 194.4, 88.6),
            ),
            line(
                "timesnewromanbold",
                10.02,
                "Registered Office of the ICAV",
                (52.4, 111.6, 181.7, 121.6),
            ),
            line(
                "timesnewroman",
                10.02,
                "32 Molesworth Street",
                (279.1, 111.6, 366.1, 121.6),
            ),
            line(
                "timesnewromanbold",
                10.02,
                "Manager",
                (52.4, 226.8, 91.4, 236.8),
            ),
            line(
                "timesnewroman",
                10.02,
                "Fineco Asset Management Designated Activity Company",
                (279.1, 226.8, 510.3, 236.8),
            ),
            line(
                "timesnewroman",
                10.02,
                "6th Floor, Block A",
                (279.1, 238.3, 355.0, 248.3),
            ),
            line(
                "timesnewroman",
                10.02,
                "Georges Quay Plaza",
                (279.1, 249.8, 361.0, 259.8),
            ),
            line(
                "timesnewroman",
                10.02,
                "Dublin 2",
                (279.1, 261.3, 314.5, 271.3),
            ),
            line(
                "timesnewroman",
                10.02,
                "Ireland",
                (279.1, 272.9, 307.5, 282.9),
            ),
            line(
                "timesnewromanbold",
                10.02,
                "Investment Manager & Investment Advisor",
                (52.4, 295.9, 239.7, 305.9),
            ),
            line(
                "timesnewroman",
                10.02,
                "See Note 1 for the list of Sub-Funds' Investment Managers",
                (279.1, 295.9, 533.4, 305.9),
            ),
        ])
    }

    /// EURIZON-EN23/1 p9, measured: the heading in the left column, the managers in
    /// frutiger-black and their specifications in frutiger-lightitalic **8.98** — the heading's own
    /// font name at another size.
    fn eurizon_en23_p9() -> Page {
        page(vec![
            line(
                "frutiger-black",
                11.97,
                "ORGANISATION OF THE FUND",
                (42.9, 40.3, 220.8, 52.3),
            ),
            line(
                "frutiger-lightitalic",
                11.97,
                "INVESTMENT MANAGERS",
                (45.4, 88.2, 177.1, 100.2),
            ),
            line(
                "frutiger-black",
                8.98,
                "Eurizon Capital S.A.",
                (228.5, 111.0, 319.9, 120.0),
            ),
            line(
                "frutiger-light",
                8.98,
                "28, Boulevard de Kockelscheuer",
                (228.5, 122.5, 350.6, 131.5),
            ),
            line(
                "frutiger-lightitalic",
                8.98,
                "(the Sub-Funds Equity China A and Asian Equity Opportunities …)",
                (228.5, 154.4, 549.3, 163.4),
            ),
            line(
                "frutiger-black",
                8.98,
                "Eurizon Capital SGR S.p.A.",
                (228.5, 186.3, 349.8, 195.3),
            ),
            line(
                "frutiger-lightitalic",
                8.98,
                "(this function has been delegated … for the Sub-Funds …)",
                (228.5, 229.7, 549.4, 238.7),
            ),
            line(
                "frutiger-lightitalic",
                11.97,
                "INDEPENDENT AUDITOR OF THE INVESTMENT FUND",
                (45.4, 339.6, 521.5, 351.6),
            ),
            line(
                "frutiger-black",
                8.98,
                "Ernst & Young S.A.",
                (223.5, 363.4, 310.5, 372.4),
            ),
        ])
    }

    /// AMUNDI-EN24 p510, measured: four sections whose headings and rows share arialmt 7, and a
    /// single-space line between two of them.
    fn amundi_en24_p510() -> Page {
        page(vec![
            line(
                "arialmt",
                7.0,
                "The following Sub-Funds have been launched during the period:",
                (42.5, 198.4, 243.3, 205.4),
            ),
            line(
                "arialmt",
                7.0,
                "Amundi Funds Climate Transition Global Equity on 13 September 2023",
                (42.5, 207.4, 264.3, 214.4),
            ),
            line("arialmt", 7.0, " ", (42.5, 270.4, 44.5, 277.4)),
            line(
                "arialmt",
                7.0,
                "The following Sub-Funds have been renamed on 15 March 2024:",
                (42.5, 279.4, 246.4, 286.4),
            ),
            line(
                "arialmt",
                7.0,
                "Amundi Funds European Equity Green Impact into Amundi Funds European Equity Green Technology",
                (42.5, 288.4, 359.9, 295.4),
            ),
            line(
                "arialmt",
                7.0,
                "Amundi Funds Pioneer Global Equity into Amundi Funds Global Equity",
                (42.5, 297.4, 362.0, 304.4),
            ),
            line(
                "arialmt",
                7.0,
                "The following Sub-Funds have been liquidated during the year:",
                (42.5, 486.4, 246.4, 493.4),
            ),
            line(
                "arialmt",
                7.0,
                "Amundi Funds Something on 12 April 2024",
                (42.5, 495.4, 246.4, 502.4),
            ),
        ])
    }

    mod the_primitive {
        use super::*;

        fn lines() -> Vec<PdfLine> {
            vec![
                line("arial-bold", 10.0, "Heading", (50.0, 100.0, 120.0, 110.0)),
                line("arial", 10.0, "Body", (50.0, 112.0, 120.0, 122.0)),
                line(
                    "arial-bold",
                    10.0,
                    "Next heading",
                    (50.0, 130.0, 150.0, 140.0),
                ),
                line(
                    "arial-bold",
                    10.0,
                    "Later heading",
                    (50.0, 160.0, 150.0, 170.0),
                ),
            ]
        }

        fn bold(line: &PdfLine) -> bool {
            line.font().inner() == "arial-bold"
        }

        #[test]
        fn finds_the_next_member_of_the_family_below() {
            let lines = lines();
            let found = section_bottom(&lines[0], &lines, bold).unwrap();
            assert_eq!(found.text(), "Next heading");
        }

        #[test]
        fn is_none_when_the_family_has_no_member_below() {
            let lines = lines();
            assert!(section_bottom(&lines[3], &lines, bold).is_none());
        }

        #[test]
        fn never_returns_the_anchor_itself() {
            let lines = lines();
            let found = section_bottom(&lines[0], &lines, bold).unwrap();
            assert_ne!(found.text(), "Heading");
        }

        #[test]
        fn takes_the_nearest_of_several_below() {
            let lines = lines();
            let found = section_bottom(&lines[0], &lines, bold).unwrap();
            assert_ne!(found.text(), "Later heading");
        }

        /// The lines of a page are not guaranteed sorted, so the primitive takes the minimum rather
        /// than the first it meets.
        #[test]
        fn takes_the_nearest_even_when_the_lines_are_out_of_order() {
            let mut shuffled = lines();
            shuffled.reverse();
            let anchor = line("arial-bold", 10.0, "Heading", (50.0, 100.0, 120.0, 110.0));
            let found = section_bottom(&anchor, &shuffled, bold).unwrap();
            assert_eq!(found.text(), "Next heading");
        }

        #[test]
        fn a_line_beside_the_anchor_is_not_below_it() {
            let beside = vec![
                line("arial-bold", 10.0, "Heading", (50.0, 100.0, 120.0, 110.0)),
                line("arial-bold", 10.0, "Beside", (300.0, 100.0, 380.0, 110.0)),
            ];
            assert!(section_bottom(&beside[0], &beside, bold).is_none());
        }

        /// FINECO-EN23@IR p3: the ten bold headings of the left margin, the anchor in the middle of
        /// them. The answer the selection algebra would give is the first, `y=78.6`.
        #[test]
        fn the_bound_is_below_the_anchor_not_first_on_the_page() {
            let page = vec![
                line(
                    "timesnewromanbold",
                    10.02,
                    "Directors and Other Information",
                    (52.5, 78.6, 194.4, 88.6),
                ),
                line(
                    "timesnewromanbold",
                    10.02,
                    "Registered Office of the ICAV",
                    (52.4, 111.6, 181.7, 121.6),
                ),
                line(
                    "timesnewromanbold",
                    10.02,
                    "Manager",
                    (52.4, 226.8, 91.4, 236.8),
                ),
                line(
                    "timesnewromanbold",
                    10.02,
                    "Investment Manager & Investment Advisor",
                    (52.4, 295.9, 239.7, 305.9),
                ),
                line(
                    "timesnewromanbold",
                    10.02,
                    "Distributor and Promoter",
                    (52.4, 330.5, 163.3, 340.5),
                ),
            ];
            let found =
                section_bottom(&page[2], &page, |l| l.font().inner() == "timesnewromanbold")
                    .unwrap();
            assert_eq!(found.text(), "Investment Manager & Investment Advisor");
        }
    }

    mod the_family {
        use super::*;

        /// EURIZON-EN23 p9: the heading and the specifications inside its own section share a font
        /// name and differ only in size. The pair is what keeps the section open.
        #[test]
        fn the_family_is_the_pair_and_not_the_font_alone() {
            let page = vec![
                line(
                    "frutiger-lightitalic",
                    11.97,
                    "INVESTMENT MANAGERS",
                    (45.4, 88.2, 177.1, 100.2),
                ),
                line(
                    "frutiger-black",
                    8.98,
                    "Eurizon Capital S.A.",
                    (228.5, 111.0, 319.9, 120.0),
                ),
                line(
                    "frutiger-lightitalic",
                    8.98,
                    "(the Sub-Funds … are managed by …)",
                    (228.5, 154.4, 549.3, 163.4),
                ),
                line(
                    "frutiger-lightitalic",
                    11.97,
                    "INDEPENDENT AUDITOR",
                    (45.4, 339.6, 521.5, 351.6),
                ),
            ];
            let bottom = SectionEnd::SameFamilyAsAnchor.bottom_below(&page[0], &page);
            assert_eq!(bottom, 339.6);
        }

        /// AMUNDI-EN24 p510: four sections, and heading and rows share arialmt 7. Only the text
        /// tells them apart, which is why the family has to be expressible as a selection.
        #[test]
        fn a_selection_closes_a_section_its_own_typography_cannot() {
            let page = vec![
                line(
                    "arialmt",
                    7.0,
                    "The following Sub-Funds have been launched during the period:",
                    (42.5, 198.4, 243.3, 205.4),
                ),
                line(
                    "arialmt",
                    7.0,
                    "Amundi Funds Climate Transition Global Equity on 13 September 2023",
                    (42.5, 207.4, 264.3, 214.4),
                ),
                line(
                    "arialmt",
                    7.0,
                    "The following Sub-Funds have been renamed on 15 March 2024:",
                    (42.5, 279.4, 246.4, 286.4),
                ),
                line(
                    "arialmt",
                    7.0,
                    "Amundi Funds European Equity Green Impact into …",
                    (42.5, 288.4, 359.9, 295.4),
                ),
            ];
            let family = SectionEnd::Selection(text_sel("^The following Sub-Funds have been"));

            assert_eq!(family.bottom_below(&page[0], &page), 279.4);
            assert_eq!(
                SectionEnd::SameFamilyAsAnchor.bottom_below(&page[0], &page),
                207.4
            );
        }

        #[test]
        fn nothing_below_leaves_the_section_open_to_the_foot_of_the_page() {
            let page = vec![
                line(
                    "frutiger-lightitalic",
                    11.97,
                    "INVESTMENT MANAGERS",
                    (45.4, 88.2, 177.1, 100.2),
                ),
                line(
                    "frutiger-black",
                    8.98,
                    "Eurizon Capital S.A.",
                    (228.5, 111.0, 319.9, 120.0),
                ),
            ];
            assert_eq!(
                SectionEnd::SameFamilyAsAnchor.bottom_below(&page[0], &page),
                f32::INFINITY
            );
        }
    }
    mod the_heading {
        use super::*;

        #[test]
        fn a_section_that_is_not_on_this_page_is_an_empty_answer_not_a_failure() {
            let blocks = run(args(text_sel("^PORTFOLIO MANAGERS$")), &eurizon_en23_p9());
            assert!(blocks.is_empty());
        }

        /// ASTERIA-EN24, measured: `Investment Managers` in helvetica 4.88 at `y=807.6` is the
        /// footer logo, on every page, and the datum is the centurygothic-bold label at `y=101.7`.
        /// The topmost match is the one a reader would call first, and here it is also the right one.
        #[test]
        fn the_footer_logo_does_not_open_the_section() {
            let asteria = page(vec![
                line(
                    "centurygothic-bold",
                    9.98,
                    "Investment Manager and Global Distributor:",
                    (302.0, 101.7, 512.5, 111.7),
                ),
                line(
                    "centurygothic",
                    8.98,
                    "ASTERIA Investment Managers S.A.",
                    (302.0, 115.7, 451.1, 124.7),
                ),
                line(
                    "centurygothic-bold",
                    9.98,
                    "Representative and Paying Agent",
                    (302.0, 180.3, 531.9, 190.3),
                ),
                line(
                    "helvetica",
                    4.88,
                    "Investment Managers",
                    (41.4, 807.6, 91.5, 812.5),
                ),
            ]);
            let blocks = run(args(text_sel("^Investment Manager")), &asteria);
            assert_eq!(
                texts(&blocks),
                vec!["ASTERIA Investment Managers S.A.".to_string()]
            );
        }
    }

    mod the_section {
        use super::*;

        /// The measurement that forbids a strict "below": on FINECO-EN23@IR p3 the value of the
        /// `Manager` section sits at `y=226.8`, **the same ordinate as the heading**, because the
        /// page is two columns and not a list.
        #[test]
        fn a_line_level_with_the_heading_is_part_of_the_section() {
            let blocks = run(args(text_sel("^Manager$")), &fineco_ir_p3());
            assert_eq!(
                texts(&blocks)[0],
                "Fineco Asset Management Designated Activity Company"
            );
        }

        /// The whole of the `Manager` section and nothing of the next one, which is the block
        /// `fineco_en23_ir/managment_company.py` builds today in fourteen hand-written lines.
        #[test]
        fn the_family_closes_it_at_the_next_heading_below() {
            let blocks = run(args(text_sel("^Manager$")), &fineco_ir_p3());
            assert_eq!(
                texts(&blocks),
                vec![
                    "Fineco Asset Management Designated Activity Company".to_string(),
                    "6th Floor, Block A".to_string(),
                    "Georges Quay Plaza".to_string(),
                    "Dublin 2".to_string(),
                    "Ireland".to_string(),
                ]
            );
        }

        #[test]
        fn the_heading_itself_stays_out_unless_asked_for() {
            let blocks = run(args(text_sel("^Manager$")), &fineco_ir_p3());
            assert!(!texts(&blocks).contains(&"Manager".to_string()));
        }

        /// EURIZON-EN23 p9: a boundary computed on the font alone would close the section at the
        /// first parenthesised specification, one line after the first manager.
        #[test]
        fn a_line_of_the_headings_font_at_another_size_does_not_close_it() {
            let mut a = args(text_sel("^INVESTMENT MANAGERS$"));
            a.body = Some(font_sel("frutiger-black"));
            let blocks = run(a, &eurizon_en23_p9());
            assert_eq!(
                texts(&blocks),
                vec![
                    "Eurizon Capital S.A.".to_string(),
                    "Eurizon Capital SGR S.p.A.".to_string()
                ]
            );
        }

        /// The section runs to the foot of the page when nothing closes it — the opening page of a
        /// list that continues on the next one.
        #[test]
        fn nothing_below_leaves_it_open_to_the_foot_of_the_page() {
            let open = page(vec![
                line(
                    "frutiger-lightitalic",
                    11.97,
                    "INVESTMENT MANAGERS",
                    (45.4, 88.2, 177.1, 100.2),
                ),
                line(
                    "frutiger-black",
                    8.98,
                    "Eurizon Capital S.A.",
                    (228.5, 111.0, 319.9, 120.0),
                ),
                line(
                    "frutiger-black",
                    8.98,
                    "Eurizon SLJ Capital LTD",
                    (228.5, 566.5, 335.1, 575.5),
                ),
            ]);
            let blocks = run(args(text_sel("^INVESTMENT MANAGERS$")), &open);
            assert_eq!(blocks.len(), 2);
        }
    }

    mod the_family_parameter {
        use super::*;

        /// AMUNDI-EN24 p510: the typographic default closes the section at the first row, because
        /// the rows *are* the heading's family. The textual family is what makes the page readable.
        #[test]
        fn a_textual_family_reads_a_section_its_typography_cannot() {
            let mut a = args(text_sel("^The following Sub-Funds have been renamed"));
            a.family = SectionEnd::Selection(text_sel("^The following Sub-Funds have been"));
            let blocks = run(a, &amundi_en24_p510());
            assert_eq!(
                texts(&blocks),
                vec![
                    "Amundi Funds European Equity Green Impact into Amundi Funds European Equity Green Technology".to_string(),
                    "Amundi Funds Pioneer Global Equity into Amundi Funds Global Equity".to_string(),
                ]
            );
        }

        #[test]
        fn the_typographic_default_would_stop_at_the_first_row_here() {
            let blocks = run(
                args(text_sel("^The following Sub-Funds have been renamed")),
                &amundi_en24_p510(),
            );
            assert!(blocks.is_empty(), "found {:?}", texts(&blocks));
        }
    }

    mod the_bottom {
        use super::*;

        /// `fineco_en23_ir/inv_managers.py` closes its table at `y1=520.0`, because what follows is
        /// not a heading of any kind.
        #[test]
        fn a_bare_ordinate_closes_the_section() {
            let mut a = args(text_sel("^INVESTMENT MANAGERS$"));
            a.bottom = Some(SectionEnd::Ordinate(150.0));
            let blocks = run(a, &eurizon_en23_p9());
            assert_eq!(blocks.len(), 2);
        }

        #[test]
        fn whichever_of_the_two_comes_first_wins() {
            let far = {
                let mut a = args(text_sel("^INVESTMENT MANAGERS$"));
                a.bottom = Some(SectionEnd::Ordinate(1000.0));
                run(a, &eurizon_en23_p9())
            };
            let near = {
                let mut a = args(text_sel("^INVESTMENT MANAGERS$"));
                a.bottom = Some(SectionEnd::Ordinate(150.0));
                run(a, &eurizon_en23_p9())
            };
            // The family stops at INDEPENDENT AUDITOR (y=339.6), so the far floor never applies.
            assert_eq!(far.len(), 5);
            assert_eq!(near.len(), 2);
        }
    }

    mod the_body {
        use super::*;

        /// The single-space line at `y=270.4` of AMUNDI-EN24 p510, which formats remove today with
        /// a hand-written deselection.
        #[test]
        fn a_body_selection_keeps_the_noise_out() {
            let mut a = args(text_sel("^The following Sub-Funds have been launched"));
            a.family = SectionEnd::Selection(text_sel("^The following Sub-Funds have been"));
            let with_noise = run(a, &amundi_en24_p510());
            assert_eq!(with_noise.len(), 2, "found {:?}", texts(&with_noise));

            let mut a = args(text_sel("^The following Sub-Funds have been launched"));
            a.family = SectionEnd::Selection(text_sel("^The following Sub-Funds have been"));
            a.body = Some(OptionallyRelative::Absolute(PdfLineSet::select_text(
                "Amundi",
            )));
            let clean = run(a, &amundi_en24_p510());
            assert_eq!(
                texts(&clean),
                vec![
                    "Amundi Funds Climate Transition Global Equity on 13 September 2023"
                        .to_string()
                ]
            );
        }

        /// EURIZON-EN23 p9 needs the managers and their specifications as **different** kinds of
        /// block. The answer is two pipes on the same page, not a parameter.
        #[test]
        fn two_pipes_read_two_roles_out_of_one_section() {
            let page = eurizon_en23_p9();
            let mut names = args(text_sel("^INVESTMENT MANAGERS$"));
            names.body = Some(font_sel("frutiger-black"));
            names.block_type = BlockType::new("INV_MAN");

            let mut specs = args(text_sel("^INVESTMENT MANAGERS$"));
            specs.body = Some(font_sel("frutiger-lightitalic"));
            specs.block_type = BlockType::new("SUB");

            let names = run(names, &page);
            let specs = run(specs, &page);

            assert_eq!(names.len(), 2);
            assert_eq!(specs.len(), 2);
            assert_eq!(names[0].type_block, BlockType::new("INV_MAN"));
            assert_eq!(specs[0].type_block, BlockType::new("SUB"));
        }
    }

    mod including_the_heading {
        use super::*;

        /// AMUNDI-EN24 p510: twenty-one rows, one date, and the date is in the heading.
        #[test]
        fn the_heading_comes_first_when_it_carries_the_datum() {
            let mut a = args(text_sel("^The following Sub-Funds have been renamed"));
            a.family = SectionEnd::Selection(text_sel("^The following Sub-Funds have been"));
            a.include_heading = true;
            let blocks = run(a, &amundi_en24_p510());
            assert_eq!(blocks.len(), 3);
            assert_eq!(
                texts(&blocks)[0],
                "The following Sub-Funds have been renamed on 15 March 2024:"
            );
        }

        /// AMUNDI-EN24 p360: the bold `Assets` does not announce the total, it **carries** it, in
        /// four columns on its own line. The `Assets` row is the measured one; the floor is given
        /// as an ordinate rather than inventing the heading that follows it.
        #[test]
        fn a_heading_that_carries_its_numbers_comes_with_them() {
            let amundi_p360 = page(vec![
                line("verdana-bold", 6.0, "Assets", (42.5, 192.1, 64.6, 198.1)),
                line(
                    "verdana-bold",
                    6.0,
                    "2,384,886",
                    (337.4, 192.1, 371.6, 198.1),
                ),
                line(
                    "verdana-bold",
                    6.0,
                    "4,941,061,439",
                    (444.8, 192.1, 493.9, 198.1),
                ),
                line(
                    "verdana",
                    6.0,
                    "Securities portfolio at market value",
                    (42.5, 202.7, 148.9, 208.7),
                ),
            ]);
            let mut a = args(text_sel("^Assets$"));
            a.include_heading = true;
            a.bottom = Some(SectionEnd::Ordinate(200.0));
            let blocks = run(a, &amundi_p360);
            assert_eq!(
                texts(&blocks),
                vec![
                    "Assets".to_string(),
                    "2,384,886".to_string(),
                    "4,941,061,439".to_string()
                ]
            );
        }
    }

    mod emitting_rows {
        use super::*;

        /// The table of FINECO-EN23@IR note 1, in miniature: one row per fund, three columns.
        #[test]
        fn rows_carry_the_two_keys_the_text_filter_reads() {
            let table = page(vec![
                line(
                    "timesnewromanbold",
                    10.02,
                    "Date of Commencement",
                    (52.4, 100.0, 180.0, 110.0),
                ),
                line(
                    "timesnewroman",
                    10.02,
                    "Alpha Fund",
                    (52.4, 120.0, 120.0, 130.0),
                ),
                line(
                    "timesnewroman",
                    10.02,
                    "01/01/2023",
                    (200.0, 120.0, 260.0, 130.0),
                ),
                line(
                    "timesnewroman",
                    10.02,
                    "Acme IM",
                    (320.0, 120.0, 380.0, 130.0),
                ),
                line(
                    "timesnewroman",
                    10.02,
                    "Beta Fund",
                    (52.4, 140.0, 120.0, 150.0),
                ),
                line(
                    "timesnewroman",
                    10.02,
                    "02/02/2023",
                    (200.0, 140.0, 260.0, 150.0),
                ),
                line(
                    "timesnewroman",
                    10.02,
                    "Beta IM",
                    (320.0, 140.0, 380.0, 150.0),
                ),
            ]);
            let mut a = args(text_sel("^Date of Commencement$"));
            a.emit = SectionEmit::Rows(Box::default());
            let blocks = run(a, &table);

            assert_eq!(blocks.len(), 6);
            let coords: Vec<(i64, i64)> = blocks
                .iter()
                .map(|b| {
                    let row = b.metadata["table-row"].as_int().expect("an integer row");
                    let col = b.metadata["table-col"].as_int().expect("an integer column");
                    (row, col)
                })
                .collect();
            assert_eq!(coords, vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)]);
        }

        #[test]
        fn lines_is_the_default_and_carries_no_metadata() {
            let blocks = run(args(text_sel("^Manager$")), &fineco_ir_p3());
            assert!(blocks.iter().all(|b| b.metadata.is_empty()));
        }
    }

    /// CARNE-EN23 p4, measured: the `Investment Managers` section at the foot of the left column,
    /// two entries whose name, specification and address are all arialmt 8.04 — 9.2 points between
    /// two lines of one entry, 18.4 between the last line of one and the name of the next.
    fn carne_en23_p4() -> Page {
        page(vec![
            line(
                "arial-boldmt",
                8.04,
                "Administrator, Domiciliary and Transfer Agent ",
                (57.6, 501.0, 233.3, 509.0),
            ),
            line(
                "arialmt",
                8.04,
                "The Bank of New York Mellon SA/NV ",
                (57.6, 519.5, 192.0, 527.5),
            ),
            line(
                "arialmt",
                8.04,
                "Luxembourg Branch ",
                (57.6, 528.8, 131.8, 536.8),
            ),
            line(
                "arial-boldmt",
                8.04,
                "Investment Managers ",
                (57.6, 574.6, 139.6, 582.6),
            ),
            line(
                "arialmt",
                8.04,
                "ARCA Fondi SGR S.p.A. ",
                (57.6, 593.2, 147.9, 601.2),
            ),
            line(
                "arialmt",
                8.04,
                "Via Disciplini 3 ",
                (57.6, 602.3, 111.9, 610.3),
            ),
            line(
                "arialmt",
                8.04,
                "I-20123 Milan ",
                (57.6, 611.6, 108.3, 619.6),
            ),
            line("arialmt", 8.04, "Italy ", (57.6, 620.8, 74.5, 628.8)),
            line(
                "arialmt",
                8.04,
                "Goldman Sachs Asset Management International Ltd. ",
                (57.6, 639.2, 249.8, 647.2),
            ),
            line(
                "arial-italicmt",
                8.04,
                "(in charge for the USD nominated securities in the ",
                (57.6, 648.7, 234.7, 656.7),
            ),
            line(
                "arialmt",
                8.04,
                "subfund Sidera Funds SICAV – Global High Yield) ",
                (57.6, 657.5, 237.8, 665.5),
            ),
            line(
                "arialmt",
                8.04,
                "Plumtree Court, 25 Shoe Lane ",
                (57.6, 666.8, 168.0, 674.8),
            ),
            line(
                "arialmt",
                8.04,
                "London EC4A 4AU ",
                (57.6, 676.0, 129.7, 684.0),
            ),
            line(
                "arialmt",
                8.04,
                "United Kingdom ",
                (57.6, 685.1, 116.7, 693.1),
            ),
        ])
    }

    mod emitting_entries {
        use super::*;

        fn groups(blocks: &[PdfBlock]) -> Vec<i64> {
            blocks
                .iter()
                .map(|b| b.metadata["group"].as_int().expect("an integer group"))
                .collect()
        }

        fn carne_args() -> SectionArgs {
            let mut a = args(text_sel("^Investment Managers"));
            a.emit = SectionEmit::Entries { gap: 15.0 };
            a
        }

        #[test]
        fn the_vertical_gap_cuts_the_section_into_its_entries() {
            let blocks = run(carne_args(), &carne_en23_p4());
            assert_eq!(groups(&blocks), vec![0, 0, 0, 0, 1, 1, 1, 1, 1, 1]);
        }

        #[test]
        fn the_lines_of_an_entry_come_out_in_page_order() {
            let blocks = run(carne_args(), &carne_en23_p4());
            assert_eq!(
                texts(&blocks)[..2].to_vec(),
                vec![
                    "ARCA Fondi SGR S.p.A. ".to_string(),
                    "Via Disciplini 3 ".to_string()
                ]
            );
        }

        /// The grouping must not depend on the order the page hands the lines over: a PDF makes no
        /// promise about it, and `get_groups` numbers the **sorted** keys.
        #[test]
        fn the_numbering_does_not_follow_the_order_the_page_arrives_in() {
            let shuffled = {
                let mut lines = carne_en23_p4().lines;
                lines.reverse();
                page(lines)
            };
            let blocks = run(carne_args(), &shuffled);
            assert_eq!(groups(&blocks), vec![0, 0, 0, 0, 1, 1, 1, 1, 1, 1]);
            assert_eq!(texts(&blocks)[0], "ARCA Fondi SGR S.p.A. ".to_string());
        }

        /// The section above `Investment Managers` has the same typography and a gap of its own:
        /// what keeps it out is the heading, not the grouping.
        #[test]
        fn only_the_lines_of_this_section_are_numbered() {
            let blocks = run(carne_args(), &carne_en23_p4());
            assert!(
                !texts(&blocks)
                    .iter()
                    .any(|t| t.contains("Bank of New York Mellon"))
            );
        }

        /// A gap wider than any in the section leaves one entry; a gap narrower than the line
        /// spacing makes every line one.
        #[test]
        fn the_gap_is_what_decides_and_nothing_else() {
            let mut wide = carne_args();
            wide.emit = SectionEmit::Entries { gap: 100.0 };
            assert_eq!(groups(&run(wide, &carne_en23_p4())), vec![0; 10]);

            let mut narrow = carne_args();
            narrow.emit = SectionEmit::Entries { gap: 1.0 };
            assert_eq!(
                groups(&run(narrow, &carne_en23_p4())),
                (0..10).collect::<Vec<i64>>()
            );
        }

        #[test]
        fn a_heading_with_nothing_under_it_gives_no_block() {
            let bare = page(vec![line(
                "arial-boldmt",
                8.04,
                "Investment Managers ",
                (57.6, 574.6, 139.6, 582.6),
            )]);
            assert!(run(carne_args(), &bare).is_empty());
        }
    }
}
