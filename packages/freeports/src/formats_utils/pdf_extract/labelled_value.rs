//! [`PdfExtractLabelledValueStandard`]: *a label declares a value right next to it*.
//!
//! The shape twenty-one of the twenty-six formats of the reference collection use to write the
//! management company, and the one the engine could not express: until now a format had to build
//! the window around the label by hand and hand the engine a ready-made selection, which is why ten
//! format modules carry geometry that says the same thing ten times.
//!
//! # Why the selection algebra could not already do this
//!
//! An area selection takes a line only when it **fully contains** its bounding box
//! (`select/pdf_line.rs`, `Container for SelectPdfLineSet`). That is the right rule for a ready-made
//! rectangle and the wrong one here: what the page actually says is *the first line below, at the
//! same abscissa* and *the line to the right, on the same band of ordinates*, and neither is a
//! containment test. Hence the multipliers of `1.1`, `2.0`, `10.0` and `100.0` written in the
//! formats: they are attempts to make a containing rectangle out of a relation that is not one.
//! This pipe works on the bounding boxes directly, so it can state the relation instead of
//! approximating it.
//!
//! # The three placements, measured
//!
//! - [`LabelledValueWhere::OnLine`] — label and value are the **same line**, and the block is that
//!   line, whole. Splitting the name out of it is interpretation of text, which by `R23` belongs to
//!   the `text_filter`; a `pdf_extract` that tried would be doing the next segment's job. Two
//!   formats, and the behaviour the engine already had;
//! - [`LabelledValueWhere::Below`] — the value is the **first line below at the same abscissa**.
//!   Thirteen formats. The abscissa is what separates it from the neighbouring column, which on
//!   these pages sits two hundred points to the right;
//! - [`LabelledValueWhere::Right`] — the value is to the **right**, on a band of ordinates that
//!   starts at the label's own. Six formats. The band is grown downwards, never upwards, because a
//!   value sits level with its label or below it, never above.
//!
//! # What "below" and "right" are measured in
//!
//! Multiples of the **label's own height** — one line of text, the same unit everywhere on the
//! page. Not of its width, which varies by a factor of six between formats (`Gestora:` is 32 points,
//! `Management Company and Global Distributor` is 200), which is exactly why the width multipliers
//! written in the formats had to range from `1.1` to `100.0` to say one thing.

use crate::core::classes::{BlockType, PdfBlock};
use crate::core::page::Page;
use crate::core::pipeline::{PdfExtractPipe, PipeError};

use super::commons::CommonsError;
use super::pdf_line::PdfLine;
use super::relative::RelativeInfo;
use super::section::SectionEnd;
use super::select::relative::PdfLineSelection;
use super::standard_funcs::PdfExtractStandardFuncsError;
use crate::commons::sets::Container;

/// Where the value sits with respect to its label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelledValueWhere {
    /// On the label's own line: the block is that line, whole. The default, and what the engine
    /// has always done.
    #[default]
    OnLine,
    /// Below the label, at the same abscissa.
    Below,
    /// To the right of the label, on the band of ordinates its box opens.
    Right,
}

/// One value or the list of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LabelledValueTake {
    /// The first candidate in page order. Finding none **fails the page**, non-fatally.
    #[default]
    First,
    /// Every candidate. Finding none is a normal answer — an empty list, not a failure.
    All,
}

/// How far from its label the value may sit, in multiples of the label's own height.
///
/// The two numbers are named for their role rather than for an axis, because which axis each one
/// measures is what [`LabelledValueWhere`] chooses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelledValueWindow {
    /// How far **along** the placement's direction: downwards for [`LabelledValueWhere::Below`],
    /// rightwards for [`LabelledValueWhere::Right`]. `None` means no limit.
    pub reach: Option<f32>,
    /// How far **across** it: the abscissa drift a candidate may have from the label for `Below`,
    /// the downward growth of the ordinate band for `Right`.
    pub across: f32,
}

impl LabelledValueWindow {
    /// The measured default for each placement.
    ///
    /// `Below` limits the reach to two lines, because past that comes either the address or the
    /// next label — on AMUNDI-EN24 the line after the value *is* the next label, which is why
    /// "every line until the font changes" is the wrong rule and "the first, nearby" the right one.
    ///
    /// `Right` sets no horizontal limit: measured over the six formats that use it, the gap between
    /// the label and its value ranges from three to sixteen line heights, and nothing but the band
    /// of ordinates ever distinguishes the value. A default that had to cover sixteen would not be
    /// a limit.
    ///
    /// `Right` also grows the band by nothing at all, because the lines of a column are
    /// **contiguous**: on FINECO-EN23@IR the address begins at the exact ordinate where the label's
    /// box ends, so growing the band by even half a line would take the address along with the
    /// name. Five of the six formats need no growth — their value overlaps the label's own box —
    /// and the sixth, EURIZON-EN23, whose columns are not row-aligned, asks for it explicitly.
    pub fn default_for(placement: LabelledValueWhere) -> Self {
        match placement {
            LabelledValueWhere::Below => Self { reach: Some(2.0), across: 0.5 },
            LabelledValueWhere::Right | LabelledValueWhere::OnLine => Self { reach: None, across: 0.0 },
        }
    }
}

/// The construction parameters of [`PdfExtractLabelledValueStandard`].
///
/// Three are required — the label, the block it emits and the pipe's name — and everything else has
/// a default, on the model of `InvestmentsStandardArgs`, so that the parameters a later format
/// needs are not paid for by the formats that do not.
pub struct LabelledValueArgs {
    /// How the label is recognised.
    pub label: PdfLineSelection,
    /// The kind of block the pipe emits.
    pub block_type: BlockType,
    /// The pipe's name, which is what appears in the log and in error messages.
    pub name: String,
    /// Where the value sits. `r#where` and not `place`: it is the word the plan, the design note and
    /// the Python keyword all use, and a raw identifier is what Rust offers for exactly that.
    pub r#where: LabelledValueWhere,
    /// How far the value may sit. `None` takes [`LabelledValueWindow::default_for`].
    pub window: Option<LabelledValueWindow>,
    /// How the value is recognised among the candidates of the window.
    ///
    /// Optional because on two of the six `Right` formats the value carries **exactly the label's
    /// font**, and only its position tells them apart; a required selection would force those two
    /// to write one that selects everything.
    pub value: Option<PdfLineSelection>,
    /// Where the label's section ends.
    pub stop: Option<SectionEnd>,
    /// One value or the list.
    pub take: LabelledValueTake,
}

impl LabelledValueArgs {
    /// The usual defaults: the label's own line, the first match, no further constraint.
    ///
    /// Those defaults are, exactly, the behaviour of the pipe that came before this one.
    pub fn new(label: PdfLineSelection, block_type: BlockType, name: impl Into<String>) -> Self {
        Self {
            label,
            block_type,
            name: name.into(),
            r#where: LabelledValueWhere::default(),
            window: None,
            value: None,
            stop: None,
            take: LabelledValueTake::default(),
        }
    }
}

/// *A label declares a value right next to it*: the pipe behind the management company, and behind
/// every other datum a report writes as a label and a value.
///
/// See the module documentation for the three placements and what they are measured in.
pub struct PdfExtractLabelledValueStandard {
    label: PdfLineSelection,
    block_type: BlockType,
    name: String,
    placement: LabelledValueWhere,
    window: LabelledValueWindow,
    value: Option<PdfLineSelection>,
    stop: Option<SectionEnd>,
    take: LabelledValueTake,
}

impl PdfExtractLabelledValueStandard {
    pub fn new(args: LabelledValueArgs) -> Self {
        let LabelledValueArgs { label, block_type, name, r#where, window, value, stop, take } = args;
        Self {
            label,
            block_type,
            name,
            placement: r#where,
            window: window.unwrap_or_else(|| LabelledValueWindow::default_for(r#where)),
            value,
            stop,
            take,
        }
    }

    /// The placement this pipe was built with.
    pub fn placement(&self) -> LabelledValueWhere {
        self.placement
    }

    /// The window this pipe resolved, defaults included.
    pub fn window(&self) -> LabelledValueWindow {
        self.window
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let candidates = self.candidates(&page.lines);
        match self.take {
            LabelledValueTake::First => match candidates.first() {
                Some(line) => {
                    tracing::debug!(found = %line.text(), "{} read next to its label", self.name);
                    Ok(vec![PdfBlock::bare(self.block_type.clone(), line.text().clone())])
                }
                // The same non-fatal page failure the engine has always raised for a single
                // expected value: a page classified for this datum that does not carry it is a page
                // classified wrongly, and staying silent about it would be worse than skipping it.
                None => Err(PdfExtractStandardFuncsError::Commons(CommonsError::PageParseFail {
                    source: Box::new(CommonsError::ExpectedTextNotFound { name: self.name.clone() }),
                })),
            },
            // An empty list is an answer, not a failure: it is the format that declares no delegate
            // manager at all, which R6 expects and which the residual rule then handles.
            LabelledValueTake::All => {
                if candidates.is_empty() {
                    tracing::debug!("{} found no value next to its label, which for a list is an answer", self.name);
                }
                Ok(candidates
                    .iter()
                    .map(|line| PdfBlock::bare(self.block_type.clone(), line.text().clone()))
                    .collect())
            }
        }
    }

    /// The lines that qualify as the value, in page order.
    fn candidates<'a>(&self, lines: &'a [PdfLine]) -> Vec<&'a PdfLine> {
        let label_set = self.label.clone().contextualize(lines);
        let labels: Vec<&PdfLine> = lines.iter().filter(|line| label_set.contains(line)).collect();
        let Some(&anchor) = labels.first() else {
            tracing::trace!("{}: the label matched no line on this page", self.name);
            return Vec::new();
        };

        let value_set = self.value.clone().map(|selection| selection.contextualize(lines));
        let bottom = self.bottom(anchor, lines);

        let placed: Vec<&PdfLine> = match self.placement {
            LabelledValueWhere::OnLine => labels,
            LabelledValueWhere::Below => {
                lines.iter().filter(|line| !std::ptr::eq(*line, anchor) && self.is_below(anchor, line)).collect()
            }
            LabelledValueWhere::Right => {
                lines.iter().filter(|line| !std::ptr::eq(*line, anchor) && self.is_right_of(anchor, line)).collect()
            }
        };

        placed
            .into_iter()
            .filter(|line| value_set.as_ref().is_none_or(|set| set.contains(*line)))
            .filter(|line| bottom.is_none_or(|limit| line.bbox().as_tuple().1 < limit))
            .collect()
    }

    /// The ordinate below which a candidate is out of the label's section, if a `stop` was given.
    ///
    /// The calculation itself lives in [`SectionEnd`], which `A2` calls with a heading where this
    /// one calls it with a label: one idea, one implementation.
    fn bottom(&self, anchor: &PdfLine, lines: &[PdfLine]) -> Option<f32> {
        Some(self.stop.as_ref()?.bottom_below(anchor, lines))
    }

    /// `line` is the value of `anchor` written underneath it: below its bottom edge, within the
    /// window's reach, and starting at the same abscissa give or take the window's drift.
    fn is_below(&self, anchor: &PdfLine, line: &PdfLine) -> bool {
        let (ax0, ay0, _, ay1) = anchor.bbox().as_tuple();
        let (x0, y0, _, _) = line.bbox().as_tuple();
        let height = ay1 - ay0;
        let far_enough_down = y0 > ay0;
        let within_reach = self.window.reach.is_none_or(|reach| y0 <= ay1 + reach * height);
        let aligned = (x0 - ax0).abs() <= self.window.across * height;
        far_enough_down && within_reach && aligned
    }

    /// `line` is the value of `anchor` written beside it: starting past its right edge, within the
    /// window's reach, and overlapping the band its box opens — grown **downwards** only, because a
    /// value sits level with its label or below it, never above.
    fn is_right_of(&self, anchor: &PdfLine, line: &PdfLine) -> bool {
        let (_, ay0, ax1, ay1) = anchor.bbox().as_tuple();
        let (x0, y0, _, y1) = line.bbox().as_tuple();
        let height = ay1 - ay0;
        let to_the_right = x0 >= ax1;
        let within_reach = self.window.reach.is_none_or(|reach| x0 <= ax1 + reach * height);
        // A slack of a tenth of a line upwards, and no more: measured, a value's box can start up
        // to 0.9 points above its label's on a line 11 points high, and nothing legitimate starts
        // higher.
        let band_top = ay0 - 0.1 * height;
        let band_bottom = ay1 + self.window.across * height;
        let in_band = y1 > band_top && y0 < band_bottom;
        to_the_right && within_reach && in_band
    }
}

impl PdfExtractPipe for PdfExtractLabelledValueStandard {
    fn name(&self) -> &str {
        &self.name
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
    use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;
    /// The geometries in these tests are the ones measured on the reports, not invented: each
    /// module names the format and the page its numbers come from, so that a test that starts
    /// failing can be checked against the document rather than against an assumption.
    fn line(font: &str, size: f32, text: &str, bbox: (f32, f32, f32, f32)) -> PdfLine {
        PdfLine::new(font, size, text, bbox)
    }

    fn page(lines: Vec<PdfLine>) -> Page {
        Page::new(1, (600.0, 850.0), lines, Vec::new())
    }

    fn text_sel(pattern: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_text(pattern))
    }

    fn font_sel(font: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_font(font))
    }

    fn args(label: PdfLineSelection) -> LabelledValueArgs {
        LabelledValueArgs::new(label, BlockType::MANAGEMENT_COMPANY, "managment company")
    }

    fn texts(blocks: &[PdfBlock]) -> Vec<String> {
        blocks.iter().map(|block| block.content.as_str().expect("a labelled value is text").to_string()).collect()
    }

    fn built(args: LabelledValueArgs) -> PdfExtractLabelledValueStandard {
        PdfExtractLabelledValueStandard::new(args)
    }

    mod on_line {
        use super::*;

        /// EURIZON-IT24/1 p7: the management company is inside a line of prose, and the whole line
        /// is what a `pdf_extract` may honestly emit — splitting the name out of it is the
        /// `text_filter`'s job, by R23.
        fn prose_page() -> Page {
            page(vec![
                line("trebuchetms-bold", 10.0, "SOCIETÀ DI GESTIONE", (70.8, 165.6, 171.0, 176.0)),
                line(
                    "trebuchetms",
                    9.3,
                    "La società di gestione EURIZON CAPITAL SGR S.p.A., di seguito \"la SGR\"",
                    (70.8, 186.2, 512.6, 195.0),
                ),
            ])
        }

        #[test]
        fn is_the_default_placement() {
            assert_eq!(built(args(text_sel("^La società"))).placement(), LabelledValueWhere::OnLine);
        }

        #[test]
        fn emits_the_label_s_own_line_whole() {
            let blocks = built(args(text_sel("^La società di gestione"))).call(&prose_page()).unwrap();
            assert_eq!(
                texts(&blocks),
                vec!["La società di gestione EURIZON CAPITAL SGR S.p.A., di seguito \"la SGR\"".to_string()]
            );
        }

        /// ANIMA_SGR-IT24/1 p3, the running head: label and value are one line, on every page.
        #[test]
        fn takes_a_running_head_that_carries_both() {
            let running_head = page(vec![
                line("lato", 7.92, "Società di Gestione del Risparmio ANIMA SGR S.p.A.", (56.6, 28.9, 243.7, 38.0)),
                line("lato", 7.92, "SISTEMA ANIMA", (476.9, 28.9, 540.6, 38.0)),
            ]);
            let blocks = built(args(text_sel("di Gestione del Risparmio"))).call(&running_head).unwrap();
            assert_eq!(texts(&blocks), vec!["Società di Gestione del Risparmio ANIMA SGR S.p.A.".to_string()]);
        }

        #[test]
        fn emits_the_block_type_it_was_given() {
            let blocks = built(args(text_sel("^La società di gestione"))).call(&prose_page()).unwrap();
            assert_eq!(blocks[0].type_block, BlockType::MANAGEMENT_COMPANY);
        }

        #[test]
        fn fails_the_page_when_the_label_matches_nothing() {
            let err = built(args(text_sel("^Nothing of the sort"))).call(&prose_page()).unwrap_err();
            assert!(matches!(
                err,
                PdfExtractStandardFuncsError::Commons(CommonsError::PageParseFail { .. })
            ));
        }

        /// ANIMA_SGR-IT24 writes the same running head on every page; a pipe asked for all of
        /// them gets one block per line, and the order is the page's.
        #[test]
        fn with_take_all_emits_one_block_per_matching_line() {
            let repeated = page(vec![
                line("lato", 7.92, "Società di Gestione del Risparmio ANIMA SGR S.p.A.", (56.6, 28.9, 243.7, 38.0)),
                line("lato", 7.92, "SISTEMA ANIMA", (476.9, 28.9, 540.6, 38.0)),
                line("lato", 7.92, "ANIMA LIQUIDITA' EURO", (56.6, 135.4, 187.5, 145.0)),
            ]);
            let mut a = args(text_sel("ANIMA"));
            a.take = LabelledValueTake::All;
            let blocks = built(a).call(&repeated).unwrap();
            assert_eq!(blocks.len(), 3);
        }
    }

    mod below {
        use super::*;

        /// ANIMA-EN23 p6: two columns, the value one line under the label at the very same
        /// abscissa, and a neighbouring column 260 points to the right that must not be taken.
        fn anima_en23() -> Page {
            page(vec![
                line("Helvetica-Bold", 8.0, "Manager, Promoter and Distributor", (43.0, 149.4, 192.7, 159.4)),
                line("Helvetica", 8.0, "Davide Sosio", (302.5, 149.2, 355.4, 159.2)),
                line("Helvetica", 8.0, "ANIMA SGR S.p.A.", (43.0, 160.3, 119.4, 170.3)),
                line("Helvetica", 8.0, "Corso Garibaldi, 99", (43.0, 171.2, 121.4, 181.2)),
            ])
        }

        fn below_args(pattern: &str) -> LabelledValueArgs {
            let mut a = args(text_sel(pattern));
            a.r#where = LabelledValueWhere::Below;
            a
        }

        #[test]
        fn takes_the_first_line_under_the_label() {
            let blocks = built(below_args("^Manager, Promoter")).call(&anima_en23()).unwrap();
            assert_eq!(texts(&blocks), vec!["ANIMA SGR S.p.A.".to_string()]);
        }

        #[test]
        fn does_not_take_the_neighbouring_column() {
            let mut a = below_args("^Manager, Promoter");
            a.take = LabelledValueTake::All;
            let blocks = built(a).call(&anima_en23()).unwrap();
            assert!(!texts(&blocks).contains(&"Davide Sosio".to_string()));
        }

        #[test]
        fn does_not_take_the_label_itself() {
            let mut a = below_args("^Manager, Promoter");
            a.take = LabelledValueTake::All;
            let blocks = built(a).call(&anima_en23()).unwrap();
            assert!(!texts(&blocks).contains(&"Manager, Promoter and Distributor".to_string()));
        }

        /// CARNE-EN23 p4: the value is *wider* than its label and two lines under it, with the
        /// right-hand column interleaved between the two.
        #[test]
        fn takes_a_value_wider_than_its_label_across_an_interleaved_column() {
            let carne = page(vec![
                line("arial-boldmt", 8.04, "Management Company", (57.6, 372.3, 145.4, 381.3)),
                line("arialmt", 8.04, "Shares of this Fund may not be offered", (291.6, 381.6, 537.9, 390.6)),
                line("arialmt", 8.04, "Carne Global Fund Managers (Luxembourg) S.A.", (57.6, 390.8, 234.6, 399.8)),
            ]);
            let blocks = built(below_args("^Management Company")).call(&carne).unwrap();
            assert_eq!(texts(&blocks), vec!["Carne Global Fund Managers (Luxembourg) S.A.".to_string()]);
        }

        /// AMUNDI-EN24 p1: under the value comes the **next label**, not an address. It is the
        /// case that makes "the first line under" right and "every line until the font changes"
        /// wrong.
        #[test]
        fn stops_before_the_next_label() {
            let amundi = page(vec![
                line("arialmt", 12.0, "Management Company", (50.0, 100.0, 160.0, 112.0)),
                line("arialmt", 12.0, "Amundi Luxembourg S.A.", (50.0, 114.0, 180.0, 126.0)),
                line("arialmt", 12.0, "Administrative Agent", (50.0, 128.0, 165.0, 140.0)),
            ]);
            let blocks = built(below_args("^Management Company")).call(&amundi).unwrap();
            assert_eq!(texts(&blocks), vec!["Amundi Luxembourg S.A.".to_string()]);
        }

        /// MEDIOLANUM-ES24.A p3: same abscissa `x=27.5`, one line down, with the depositary's
        /// column at `x=302.9` on the very same ordinates.
        #[test]
        fn keeps_the_abscissa_against_a_column_on_the_same_ordinates() {
            let mediolanum = page(vec![
                line("arial-bold", 6.5, "Gestor de Inversiones y Gestora de Tesorería", (27.5, 564.8, 220.2, 573.8)),
                line("arial", 6.5, "14, Porte de France", (302.9, 562.9, 383.1, 571.9)),
                line("arial", 6.5, "L-4360 Esch-sur-Alzette", (302.9, 573.8, 407.0, 582.8)),
                line("arial", 6.5, "Mediolanum International Funds Limited", (27.5, 576.1, 198.3, 585.1)),
            ]);
            let blocks = built(below_args("^Gestor de Inversiones")).call(&mediolanum).unwrap();
            assert_eq!(texts(&blocks), vec!["Mediolanum International Funds Limited".to_string()]);
        }

        #[test]
        fn nothing_under_the_label_fails_the_page() {
            let lonely = page(vec![line("arialmt", 10.0, "Management Company", (50.0, 100.0, 160.0, 110.0))]);
            let err = built(below_args("^Management Company")).call(&lonely).unwrap_err();
            assert!(matches!(
                err,
                PdfExtractStandardFuncsError::Commons(CommonsError::PageParseFail { .. })
            ));
        }

        #[test]
        fn a_line_above_the_label_is_never_a_candidate() {
            let upside_down = page(vec![
                line("arialmt", 10.0, "Above the label", (50.0, 80.0, 160.0, 90.0)),
                line("arialmt", 10.0, "Management Company", (50.0, 100.0, 160.0, 110.0)),
            ]);
            let err = built(below_args("^Management Company")).call(&upside_down).unwrap_err();
            assert!(matches!(
                err,
                PdfExtractStandardFuncsError::Commons(CommonsError::PageParseFail { .. })
            ));
        }
    }

    mod right {
        use super::*;

        /// FINECO-EN23@IR p3: label and value share the ordinate exactly, 188 points apart, and
        /// the font is the only other difference. These are the 53 hand-written lines of
        /// `fineco_en23_ir/managment_company.py`, as three parameters.
        fn fineco_ir() -> Page {
            page(vec![
                line("timesnewromanbold", 10.02, "Directors of the ICAV", (52.4, 157.7, 147.4, 169.2)),
                line("timesnewroman", 10.02, "Jim Firn (Irish resident)*", (279.1, 157.7, 379.6, 169.2)),
                line("timesnewromanbold", 10.02, "Manager", (52.4, 226.8, 91.4, 238.3)),
                line(
                    "timesnewroman",
                    10.02,
                    "Fineco Asset Management Designated Activity Company",
                    (279.1, 226.8, 510.3, 238.3),
                ),
                line("timesnewroman", 10.02, "6th Floor, Block A", (279.1, 238.3, 355.0, 249.8)),
                line("timesnewromanbold", 10.02, "Investment Manager & Investment Advisor", (52.4, 295.9, 239.7, 307.4)),
            ])
        }

        fn right_args(pattern: &str) -> LabelledValueArgs {
            let mut a = args(text_sel(pattern));
            a.r#where = LabelledValueWhere::Right;
            a
        }

        #[test]
        fn takes_the_line_beside_the_label() {
            let blocks = built(right_args("^Manager$")).call(&fineco_ir()).unwrap();
            assert_eq!(
                texts(&blocks),
                vec!["Fineco Asset Management Designated Activity Company".to_string()]
            );
        }

        #[test]
        fn does_not_take_the_address_under_the_value() {
            let mut a = right_args("^Manager$");
            a.take = LabelledValueTake::All;
            let blocks = built(a).call(&fineco_ir()).unwrap();
            assert_eq!(texts(&blocks).len(), 1);
        }

        #[test]
        fn does_not_take_another_section_s_value() {
            let mut a = right_args("^Manager$");
            a.take = LabelledValueTake::All;
            let blocks = built(a).call(&fineco_ir()).unwrap();
            assert!(!texts(&blocks).iter().any(|text| text.contains("Jim Firn")));
        }

        /// MEDIOLANUM-IT24.A p5 and MEDIOLANUM-IT24.C p5: the value's box starts **slightly above**
        /// its label's — by 0.5 and by 0.9 points on lines eleven points high. A band that
        /// compared top edges would lose both.
        #[test]
        fn tolerates_a_value_starting_slightly_above_its_label() {
            let mediolanum = page(vec![
                line("tt3102o00", 9.96, "SOCIETÀ DI GESTIONE E ", (28.8, 84.0, 141.5, 94.5)),
                line("tt3102o00", 9.96, "Mediolanum International Funds Limited ", (206.2, 83.1, 369.7, 93.6)),
            ]);
            let blocks = built(right_args("^SOCIETÀ DI GESTIONE")).call(&mediolanum).unwrap();
            assert_eq!(texts(&blocks), vec!["Mediolanum International Funds Limited ".to_string()]);
        }

        /// MEDIOLANUM-EN24 p3: the label wraps onto a second line, and the value is level with the
        /// **first** of the two — sharing its font exactly, so only the position tells them apart.
        #[test]
        fn takes_a_value_level_with_the_first_line_of_a_wrapped_label() {
            let mediolanum = page(vec![
                line("timesnewromanps-boldmt", 9.48, "MANAGER AND GLOBAL ", (28.9, 85.9, 147.4, 96.4)),
                line("timesnewromanps-boldmt", 9.48, "Mediolanum International Funds Limited ", (205.9, 85.9, 378.6, 96.4)),
                line("timesnewromanps-boldmt", 9.48, "DISTRIBUTOR ", (28.9, 96.8, 97.8, 107.3)),
                line("timesnewromanpsmt", 9.48, "4th Floor, The Exchange, Dublin 1.", (205.9, 96.9, 504.8, 107.4)),
            ]);
            let blocks = built(right_args("^MANAGER AND GLOBAL")).call(&mediolanum).unwrap();
            assert_eq!(texts(&blocks), vec!["Mediolanum International Funds Limited ".to_string()]);
        }

        /// MEDIOLANUM-ES24.B p1: the whole page is a label→value table, and the value is told from
        /// the label by weight alone.
        #[test]
        fn reads_a_label_value_table_row() {
            let mediolanum = page(vec![
                line("helvetica-bold", 7.85, "Gestora:", (80.6, 134.6, 112.8, 143.6)),
                line("helvetica", 7.85, "MEDIOLANUM GESTION, S.G.I.I.C., S.A.", (140.7, 134.6, 287.2, 143.6)),
                line("helvetica-bold", 7.85, "Grupo Gestora:", (80.6, 147.0, 138.5, 156.0)),
                line("helvetica", 7.85, "MEDIOLANUM", (140.7, 147.0, 193.9, 156.0)),
            ]);
            let blocks = built(right_args("^Gestora:")).call(&mediolanum).unwrap();
            assert_eq!(texts(&blocks), vec!["MEDIOLANUM GESTION, S.G.I.I.C., S.A.".to_string()]);
        }

        #[test]
        fn a_line_to_the_left_is_never_a_candidate() {
            let leftwards = page(vec![
                line("arialmt", 10.0, "To the left", (10.0, 100.0, 40.0, 110.0)),
                line("arialmt", 10.0, "Management Company", (50.0, 100.0, 160.0, 110.0)),
            ]);
            let err = built(right_args("^Management Company")).call(&leftwards).unwrap_err();
            assert!(matches!(
                err,
                PdfExtractStandardFuncsError::Commons(CommonsError::PageParseFail { .. })
            ));
        }
    }

    mod window {
        use super::*;

        fn two_lines_down() -> Page {
            page(vec![
                line("arialmt", 10.0, "Label", (50.0, 100.0, 100.0, 110.0)),
                line("arialmt", 10.0, "One line down", (50.0, 115.0, 140.0, 125.0)),
                line("arialmt", 10.0, "Four lines down", (50.0, 155.0, 150.0, 165.0)),
            ])
        }

        fn below_with(window: LabelledValueWindow) -> PdfExtractLabelledValueStandard {
            let mut a = args(text_sel("^Label$"));
            a.r#where = LabelledValueWhere::Below;
            a.window = Some(window);
            a.take = LabelledValueTake::All;
            built(a)
        }

        #[test]
        fn the_default_reach_below_is_two_lines() {
            assert_eq!(
                LabelledValueWindow::default_for(LabelledValueWhere::Below),
                LabelledValueWindow { reach: Some(2.0), across: 0.5 }
            );
        }

        #[test]
        fn the_default_reach_to_the_right_is_unlimited() {
            assert_eq!(LabelledValueWindow::default_for(LabelledValueWhere::Right).reach, None);
        }

        /// The lines of a column are contiguous: a band grown by even half a line would reach the
        /// address under the value. Growing it is EURIZON-EN23's business, and nobody else's.
        #[test]
        fn the_default_band_to_the_right_is_the_label_s_own_box() {
            assert_eq!(LabelledValueWindow::default_for(LabelledValueWhere::Right).across, 0.0);
        }

        #[test]
        fn a_reach_of_two_lines_stops_before_the_fourth() {
            let blocks = below_with(LabelledValueWindow { reach: Some(2.0), across: 0.5 })
                .call(&two_lines_down())
                .unwrap();
            assert_eq!(texts(&blocks), vec!["One line down".to_string()]);
        }

        #[test]
        fn a_longer_reach_takes_both() {
            let blocks = below_with(LabelledValueWindow { reach: Some(6.0), across: 0.5 })
                .call(&two_lines_down())
                .unwrap();
            assert_eq!(texts(&blocks).len(), 2);
        }

        #[test]
        fn no_reach_takes_everything_under_the_label() {
            let blocks = below_with(LabelledValueWindow { reach: None, across: 0.5 })
                .call(&two_lines_down())
                .unwrap();
            assert_eq!(texts(&blocks).len(), 2);
        }

        #[test]
        fn a_reach_of_zero_still_takes_a_line_touching_the_label_s_bottom_edge() {
            let touching = page(vec![
                line("arialmt", 10.0, "Label", (50.0, 100.0, 100.0, 110.0)),
                line("arialmt", 10.0, "Touching", (50.0, 110.0, 140.0, 120.0)),
            ]);
            let blocks = below_with(LabelledValueWindow { reach: Some(0.0), across: 0.5 }).call(&touching).unwrap();
            assert_eq!(texts(&blocks), vec!["Touching".to_string()]);
        }

        #[test]
        fn a_negative_reach_selects_nothing_rather_than_inverting() {
            let mut a = args(text_sel("^Label$"));
            a.r#where = LabelledValueWhere::Below;
            a.window = Some(LabelledValueWindow { reach: Some(-5.0), across: 0.5 });
            a.take = LabelledValueTake::All;
            assert!(built(a).call(&two_lines_down()).unwrap().is_empty());
        }

        #[test]
        fn a_wider_drift_lets_an_indented_value_through() {
            let indented = page(vec![
                line("arialmt", 10.0, "Label", (50.0, 100.0, 100.0, 110.0)),
                line("arialmt", 10.0, "Indented by twelve points", (62.0, 115.0, 200.0, 125.0)),
            ]);
            assert!(
                below_with(LabelledValueWindow { reach: Some(2.0), across: 0.5 })
                    .call(&indented)
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                texts(&below_with(LabelledValueWindow { reach: Some(2.0), across: 2.0 }).call(&indented).unwrap()),
                vec!["Indented by twelve points".to_string()]
            );
        }

        /// EURIZON-EN23 p8: the two columns are not row-aligned — the value sits 24 points below
        /// its label and 180 to the right. The band has to be grown downwards to reach it, which
        /// then also reaches the address: it is the one format where `value` does the last cut.
        #[test]
        fn a_downward_band_reaches_a_column_that_is_not_row_aligned() {
            let eurizon = page(vec![
                line("frutiger-lightitalic", 11.97, "PROMOTER", (45.4, 143.6, 104.6, 157.4)),
                line("frutiger-black", 8.98, "Eurizon Capital S.A.", (227.5, 167.4, 318.9, 176.4)),
                line("frutiger-light", 8.98, "28, Boulevard de Kockelscheuer", (227.5, 177.8, 349.6, 186.8)),
            ]);
            let mut a = args(text_sel("^PROMOTER"));
            a.r#where = LabelledValueWhere::Right;
            a.window = Some(LabelledValueWindow { reach: None, across: 2.0 });
            a.value = Some(font_sel("frutiger-black"));
            assert_eq!(texts(&built(a).call(&eurizon).unwrap()), vec!["Eurizon Capital S.A.".to_string()]);
        }
    }

    mod value_selection {
        use super::*;

        fn name_and_address() -> Page {
            page(vec![
                line("frutiger-lightitalic", 11.97, "PROMOTER", (45.4, 143.6, 104.6, 157.4)),
                line("frutiger-black", 8.98, "Eurizon Capital S.A.", (45.4, 167.4, 136.8, 176.4)),
                line("frutiger-light", 8.98, "28, Boulevard de Kockelscheuer", (45.4, 177.8, 167.5, 186.8)),
            ])
        }

        fn below_all(value: Option<PdfLineSelection>) -> PdfExtractLabelledValueStandard {
            let mut a = args(text_sel("^PROMOTER"));
            a.r#where = LabelledValueWhere::Below;
            a.window = Some(LabelledValueWindow { reach: Some(3.0), across: 0.5 });
            a.value = value;
            a.take = LabelledValueTake::All;
            built(a)
        }

        #[test]
        fn without_it_the_window_alone_decides() {
            assert_eq!(texts(&below_all(None).call(&name_and_address()).unwrap()).len(), 2);
        }

        #[test]
        fn the_weight_of_the_font_tells_the_name_from_the_address() {
            assert_eq!(
                texts(&below_all(Some(font_sel("frutiger-black"))).call(&name_and_address()).unwrap()),
                vec!["Eurizon Capital S.A.".to_string()]
            );
        }

        #[test]
        fn a_selection_that_matches_nothing_leaves_no_candidate() {
            assert!(below_all(Some(font_sel("nothing-like-it"))).call(&name_and_address()).unwrap().is_empty());
        }

        /// The two formats of §4.3: the value carries the label's own font, and a required `value`
        /// would have had them write a selection that selects everything.
        #[test]
        fn is_not_needed_where_the_value_shares_the_label_s_font() {
            let same_font = page(vec![
                line("tt3102o00", 9.96, "SOCIETÀ DI GESTIONE E ", (28.8, 84.0, 141.5, 94.5)),
                line("tt3102o00", 9.96, "Mediolanum International Funds Limited ", (206.2, 83.1, 369.7, 93.6)),
            ]);
            let mut a = args(text_sel("^SOCIETÀ DI GESTIONE"));
            a.r#where = LabelledValueWhere::Right;
            assert_eq!(
                texts(&built(a).call(&same_font).unwrap()),
                vec!["Mediolanum International Funds Limited ".to_string()]
            );
        }
    }

    mod take_and_stop {
        use super::*;

        /// A section of three names under one heading, followed by the next heading in the same
        /// family and one more name under it.
        fn two_sections() -> Page {
            page(vec![
                line("arial-bold", 10.0, "INVESTMENT MANAGERS", (50.0, 100.0, 200.0, 110.0)),
                line("arial", 10.0, "First Manager Ltd", (50.0, 112.0, 160.0, 122.0)),
                line("arial", 10.0, "Second Manager Ltd", (50.0, 124.0, 170.0, 134.0)),
                line("arial", 10.0, "Third Manager Ltd", (50.0, 136.0, 165.0, 146.0)),
                line("arial-bold", 10.0, "DEPOSITARY", (50.0, 150.0, 140.0, 160.0)),
                line("arial", 10.0, "A Bank S.A.", (50.0, 162.0, 130.0, 172.0)),
            ])
        }

        fn list_args(stop: Option<SectionEnd>) -> LabelledValueArgs {
            let mut a = args(text_sel("^INVESTMENT MANAGERS"));
            a.r#where = LabelledValueWhere::Below;
            a.window = Some(LabelledValueWindow { reach: None, across: 0.5 });
            a.take = LabelledValueTake::All;
            a.stop = stop;
            a
        }

        #[test]
        fn take_first_reads_one_value_of_a_list() {
            let mut a = list_args(None);
            a.take = LabelledValueTake::First;
            assert_eq!(texts(&built(a).call(&two_sections()).unwrap()), vec!["First Manager Ltd".to_string()]);
        }

        #[test]
        fn take_all_without_a_stop_runs_to_the_bottom_of_the_page() {
            let blocks = built(list_args(None)).call(&two_sections()).unwrap();
            assert_eq!(texts(&blocks).len(), 5);
        }

        #[test]
        fn a_stop_on_the_label_s_own_family_closes_the_section() {
            let blocks = built(list_args(Some(SectionEnd::SameFamilyAsAnchor)))
                .call(&two_sections())
                .unwrap();
            assert_eq!(
                texts(&blocks),
                vec![
                    "First Manager Ltd".to_string(),
                    "Second Manager Ltd".to_string(),
                    "Third Manager Ltd".to_string()
                ]
            );
        }

        #[test]
        fn a_stop_given_as_a_selection_closes_it_too() {
            let blocks = built(list_args(Some(SectionEnd::Selection(text_sel("^DEPOSITARY")))))
                .call(&two_sections())
                .unwrap();
            assert_eq!(texts(&blocks).len(), 3);
        }

        #[test]
        fn a_stop_that_matches_nothing_leaves_the_section_open() {
            let blocks = built(list_args(Some(SectionEnd::Selection(text_sel("^NOWHERE")))))
                .call(&two_sections())
                .unwrap();
            assert_eq!(texts(&blocks).len(), 5);
        }

        /// R6: a format that declares no delegate manager at all. An empty list is the answer, and
        /// failing the page instead would throw away everything else the page carries.
        #[test]
        fn take_all_finding_nothing_is_an_empty_list_not_a_failure() {
            let empty = page(vec![line("arial-bold", 10.0, "INVESTMENT MANAGERS", (50.0, 100.0, 200.0, 110.0))]);
            assert!(built(list_args(None)).call(&empty).unwrap().is_empty());
        }

        #[test]
        fn take_all_finding_no_label_is_an_empty_list_too() {
            let elsewhere = page(vec![line("arial", 10.0, "Nothing here", (50.0, 100.0, 200.0, 110.0))]);
            assert!(built(list_args(None)).call(&elsewhere).unwrap().is_empty());
        }
    }


    /// Il costruttore della società di gestione: che sia `A1` e non una seconda implementazione,
    /// e che i dieci formati che lo chiamano oggi non vedano nessuna differenza.
    mod the_managment_company_signpost {
        use super::*;
        use crate::formats_utils::pdf_extract::standard_funcs::{
            ExtractTextPdfBlockOrFailPage, pdf_extract_managment_company_standard,
        };

        fn sample() -> Page {
            page(vec![
                line("arialmt", 10.0, "Società di gestione: Acme SGR S.p.A.", (0.0, 0.0, 200.0, 10.0)),
                line("arialmt", 10.0, "Depositario: A Bank", (0.0, 12.0, 200.0, 22.0)),
            ])
        }

        /// Il tubo che c'era **è** `A1` con la posizione per difetto: non gli somiglia.
        #[test]
        fn is_the_general_algorithm_with_its_default_placement() {
            let pipe = pdf_extract_managment_company_standard(text_sel("^Società di gestione"));
            assert_eq!(pipe.placement(), LabelledValueWhere::OnLine);
        }

        #[test]
        fn keeps_the_reference_spelling_of_its_name() {
            let pipe = pdf_extract_managment_company_standard(text_sel("^Società di gestione"));
            assert_eq!(PdfExtractPipe::name(&pipe), "managment company");
        }

        #[test]
        fn keeps_its_block_type() {
            let pipe = pdf_extract_managment_company_standard(text_sel("^Società di gestione"));
            let blocks = pipe.call(&sample()).unwrap();
            assert_eq!(blocks[0].type_block, BlockType::MANAGEMENT_COMPANY);
        }

        /// A parità di ingresso, la stessa uscita del tubo che i dieci formati chiamavano. È la
        /// prova che chiede la F7 — *«i formati interessati migrano a parità di uscita
        /// verificata»* — fatta qui una volta per tutti e dieci, perché tutti e dieci passano una
        /// finestra costruita a mano esattamente come questa.
        #[test]
        fn gives_the_same_block_as_the_pipe_it_replaces() {
            let selection = text_sel("^Società di gestione");
            let before = ExtractTextPdfBlockOrFailPage::new(
                selection.clone(),
                "managment company",
                BlockType::MANAGEMENT_COMPANY,
            )
            .call(&sample())
            .unwrap();
            let after = pdf_extract_managment_company_standard(selection).call(&sample()).unwrap();
            assert_eq!(after, before);
        }

        #[test]
        fn fails_the_page_on_a_miss_exactly_as_it_did() {
            let selection = text_sel("^Nothing of the sort");
            let before = ExtractTextPdfBlockOrFailPage::new(
                selection.clone(),
                "managment company",
                BlockType::MANAGEMENT_COMPANY,
            )
            .extract(&sample())
            .unwrap_err();
            let after = pdf_extract_managment_company_standard(selection).extract(&sample()).unwrap_err();
            assert_eq!(after.is_page_failure(), before.is_page_failure());
            assert!(after.is_page_failure());
        }

        /// Il nome del fondo e la valuta **non** seguono: sulle loro pagine non c'è nessuna
        /// etichetta accanto al valore, solo un ritaglio, e il nome generale indicherebbe la strada
        /// sbagliata.
        #[test]
        fn the_fund_and_the_currency_stay_where_they_are() {
            use crate::formats_utils::pdf_extract::standard_funcs::{
                pdf_extract_currency_standard, pdf_extract_fund_standard,
            };
            let fund: ExtractTextPdfBlockOrFailPage = pdf_extract_fund_standard(text_sel("^X$"));
            let currency: ExtractTextPdfBlockOrFailPage = pdf_extract_currency_standard(text_sel("^X$"));
            assert_eq!(PdfExtractPipe::name(&fund), "fund");
            assert_eq!(PdfExtractPipe::name(&currency), "currency");
        }
    }

    mod the_pipe_surface {
        use super::*;

        fn sample() -> Page {
            page(vec![line("arialmt", 10.0, "Alpha", (0.0, 0.0, 10.0, 10.0))])
        }

        #[test]
        fn the_pipe_name_is_the_field_it_extracts() {
            assert_eq!(PdfExtractPipe::name(&built(args(text_sel("^Alpha$")))), "managment company");
        }

        #[test]
        fn a_page_failure_stays_non_fatal_once_translated_for_the_engine() {
            let err = built(args(text_sel("^Nope$"))).extract(&sample()).unwrap_err();
            assert!(err.is_page_failure());
        }

        #[test]
        fn the_resolved_window_is_the_default_of_the_placement_it_was_given() {
            let mut a = args(text_sel("^Alpha$"));
            a.r#where = LabelledValueWhere::Below;
            assert_eq!(built(a).window(), LabelledValueWindow::default_for(LabelledValueWhere::Below));
        }

        #[test]
        fn an_explicit_window_wins_over_the_default() {
            let mut a = args(text_sel("^Alpha$"));
            a.r#where = LabelledValueWhere::Below;
            a.window = Some(LabelledValueWindow { reach: Some(9.0), across: 9.0 });
            assert_eq!(built(a).window(), LabelledValueWindow { reach: Some(9.0), across: 9.0 });
        }

        #[test]
        fn an_empty_page_fails_rather_than_emitting_an_empty_block() {
            let err = built(args(text_sel("^Alpha$"))).call(&page(Vec::new())).unwrap_err();
            assert!(matches!(
                err,
                PdfExtractStandardFuncsError::Commons(CommonsError::PageParseFail { .. })
            ));
        }
    }
}
