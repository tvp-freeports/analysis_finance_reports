//! [`PdfExtractLabelledColumnStandard`]: *the value written in the label's own column, joined into
//! one block*.
//!
//! [`super::labelled_value::PdfExtractLabelledValueStandard`] returns **one block per line** and
//! does not join them, because joining lines is text interpretation and not geometry. For a datum
//! written on one line that is the right answer. For a datum that **wraps**, or that the PDF
//! generator breaks into fragments, it is half the value — and in the reference collection the
//! wrapped spelling is the majority one:
//!
//! | format | how the value is written |
//! |---|---|
//! | AMUNDI-EN24 | the label on one line, the value on the **two or three** lines under it, each broken into five fragments where the font changes |
//! | AMUNDI-IT24 | the label and the first piece on one line, the rest on the **two** lines under |
//! | CARNE-EN23 | the label alone, the value on the line under, the qualification `(the "Sub-Fund")` on the next |
//! | ASTERIA-EN24 | all on one line but in **three** fragments, and on one page the colon is doubled |
//! | UBS-EN23 | all on one line, in two fragments |
//!
//! # The rule, and it is one rule
//!
//! The value is **what sits in the label's column**, from the label's own line down to the first
//! anchor that closes the block. Lines are grouped into **ordinate bands** — two fragments that
//! overlap vertically are the same line even when their `y` differ by a couple of points because
//! the font changed — ordered by abscissa inside the band, and joined: **without a space** inside a
//! band, because that is where the PDF breaks words in half (`G` + `REAT`), and **with** one
//! between bands, because that is where it breaks between words.
//!
//! # The two anchors
//!
//! The column ends where the **neighbouring** column begins, and the block ends where the next
//! heading begins. Which of the two closes the block is decided by the page and not by the format,
//! and it is measured: where the neighbour sits **under** the label (UBS-EN23, ASTERIA-EN24) it
//! closes, and taking the heading would drag the neighbour in; where it sits **beside** it
//! (AMUNDI, CARNE-EN23) there is nothing underneath until the heading, and the heading closes.
//!
//! Hence both are given and the nearer one wins, rather than the format choosing.
//!
//! # Why the label is not stripped here
//!
//! The block comes out with the label still attached: removing it is the `text_filter`'s business
//! (`R23`), which is why the filters that consume this block take a prefix to remove.

use crate::core::classes::{BlockType, PdfBlock};
use crate::core::page::Page;
use crate::core::pipeline::{PdfExtractPipe, PipeError};

use super::pdf_line::PdfLine;
use super::relative::RelativeInfo;
use super::select::relative::PdfLineSelection;

/// How far a fragment may stray and still belong: to the same band, or inside the column.
///
/// One typographic point. Measured: UBS-EN23's value begins **two tenths of a point** before the
/// right edge of its own label, and ASTERIA-EN24's one and three.
pub const DEFAULT_SLACK: f32 = 1.5;

/// A coordinate far enough away to mean "no limit on this side".
const UNBOUNDED: f32 = 1e6;

/// What a labelled-column pipe is told.
pub struct LabelledColumnArgs {
    /// How the label is recognised. Its **first** match on the page anchors everything else.
    pub label: PdfLineSelection,
    /// The neighbouring column's anchor: it bounds the column on the right when it sits on the
    /// label's own line, and closes the block when it sits below.
    pub neighbour: PdfLineSelection,
    /// The next heading: it closes the block, and never bounds the column.
    pub heading: PdfLineSelection,
    /// The kind of block the pipe emits.
    pub block_type: BlockType,
    /// The pipe's name, which is what appears in the log and in error messages.
    pub name: String,
    /// A word the document does **not** write and the rest of the report does.
    ///
    /// MEDIOLANUM-IT24.B titles its sub-funds `Fondo Mediolanum …` on the portfolio pages and
    /// `Mediolanum …` on the annex, and without putting the word back `R24` does not hold and the
    /// row is lost in silence. It goes in front of the **value** and not of the label, so it is
    /// inserted right after the label's own fragment — where a `text_filter` removing the label
    /// leaves it at the head of the value.
    pub prepend: String,
    /// See [`DEFAULT_SLACK`].
    pub slack: f32,
}

impl LabelledColumnArgs {
    pub fn new(
        label: PdfLineSelection,
        neighbour: PdfLineSelection,
        heading: PdfLineSelection,
        block_type: BlockType,
        name: impl Into<String>,
    ) -> Self {
        Self {
            label,
            neighbour,
            heading,
            block_type,
            name: name.into(),
            prepend: String::new(),
            slack: DEFAULT_SLACK,
        }
    }

    pub fn with_prepend(mut self, prepend: impl Into<String>) -> Self {
        self.prepend = prepend.into();
        self
    }
}

/// The pipe that returns the label's column as a single block. See the module documentation.
pub struct PdfExtractLabelledColumnStandard {
    args: LabelledColumnArgs,
}

impl PdfExtractLabelledColumnStandard {
    pub fn new(args: LabelledColumnArgs) -> Self {
        Self { args }
    }

    /// The lines a selection takes, in page order.
    fn select<'a>(selection: &PdfLineSelection, lines: &'a [PdfLine]) -> Vec<&'a PdfLine> {
        use crate::commons::sets::Container;
        let set = selection.clone().contextualize(lines);
        lines.iter().filter(|line| set.contains(line)).collect()
    }

    /// The lines grouped into ordinate bands, top to bottom and left to right inside each band.
    ///
    /// The criterion is the **middle** of the box, not the overlap of the boxes, and a band does
    /// **not** grow as fragments join it. Measured on AMUNDI-EN24 p777, where overlap gets it
    /// wrong: the fragment `G` hangs two points below its own line, the grown band reaches the
    /// next one, and `G` + `REAT` end up at the tail of the value instead of in the middle. Two
    /// lines' middles are a whole height apart, so half a height separates them without ambiguity.
    fn bands(lines: Vec<&PdfLine>) -> Vec<Vec<&PdfLine>> {
        let mut ordered = lines;
        ordered.sort_by(|a, b| {
            let (ax0, ay0, _, ay1) = a.bbox().as_tuple();
            let (bx0, by0, _, by1) = b.bbox().as_tuple();
            (ay0 + ay1, ax0).partial_cmp(&(by0 + by1, bx0)).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut bands: Vec<(f32, f32, Vec<&PdfLine>)> = Vec::new();
        for line in ordered {
            let (_, y0, _, y1) = line.bbox().as_tuple();
            let middle = (y0 + y1) / 2.0;
            match bands.iter_mut().find(|(centre, tolerance, _)| (middle - *centre).abs() <= *tolerance) {
                Some((_, _, members)) => members.push(line),
                None => bands.push((middle, (y1 - y0).max(1.0) / 2.0, vec![line])),
            }
        }
        bands
            .into_iter()
            .map(|(_, _, mut members)| {
                members.sort_by(|a, b| {
                    a.bbox().as_tuple().0.partial_cmp(&b.bbox().as_tuple().0).unwrap_or(std::cmp::Ordering::Equal)
                });
                members
            })
            .collect()
    }

    pub fn call(&self, page: &Page) -> Vec<PdfBlock> {
        let lines = &page.lines;
        let Some(anchor) = Self::select(&self.args.label, lines).into_iter().next() else {
            return Vec::new();
        };
        let (ax0, ay0, _, ay1) = anchor.bbox().as_tuple();

        let neighbours = Self::select(&self.args.neighbour, lines);

        // On the right: the neighbouring column, but only where it sits **on** the label's line.
        // Below, it narrows nothing — there it is an end-of-block anchor, not an edge.
        let mut right = UNBOUNDED;
        for line in &neighbours {
            let (x0, y0, _, y1) = line.bbox().as_tuple();
            if y0 < ay1 && y1 > ay0 && x0 > ax0 {
                right = right.min(x0);
            }
        }

        // Below: the first anchor beginning **after** the label's line, among the neighbouring
        // column and the headings.
        let mut bottom = UNBOUNDED;
        for line in neighbours.iter().chain(Self::select(&self.args.heading, lines).iter()) {
            let y0 = line.bbox().as_tuple().1;
            if y0 >= ay1 {
                bottom = bottom.min(y0);
            }
        }

        let slack = self.args.slack;
        let inside: Vec<&PdfLine> = lines
            .iter()
            .filter(|line| {
                let (x0, y0, x1, y1) = line.bbox().as_tuple();
                x0 >= ax0 - slack && x1 <= right + slack && y0 >= ay0 - slack && y1 <= bottom + slack
            })
            .collect();

        let anchor_box = anchor.bbox().as_tuple();
        let mut pieces = Vec::new();
        for band in Self::bands(inside) {
            let mut chunk = String::new();
            for line in band {
                chunk.push_str(line.text());
                // The comparison is on the box and not on identity: every selection builds fresh
                // objects, so two readings of one line are not the same object.
                if !self.args.prepend.is_empty() && line.bbox().as_tuple() == anchor_box {
                    chunk.push(' ');
                    chunk.push_str(&self.args.prepend);
                    chunk.push(' ');
                }
            }
            pieces.push(chunk);
        }
        let name = pieces.join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            Vec::new()
        } else {
            vec![PdfBlock::bare(self.args.block_type.clone(), name)]
        }
    }
}

impl PdfExtractPipe for PdfExtractLabelledColumnStandard {
    fn name(&self) -> &str {
        &self.args.name
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        Ok(self.call(page))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
    use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;

    fn line(text: &str, area: (f32, f32, f32, f32)) -> PdfLine {
        PdfLine::new("Helvetica", 9.0, text, area)
    }

    fn page(lines: Vec<PdfLine>) -> Page {
        Page::new(1, (600.0, 850.0), lines, Vec::new())
    }

    fn text_sel(pattern: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_text(pattern))
    }

    /// A selection no line of these pages satisfies: an anchor that is simply not there.
    fn absent() -> PdfLineSelection {
        text_sel("^no such line on any page$")
    }

    fn pipe(label: &str, neighbour: PdfLineSelection, heading: PdfLineSelection) -> PdfExtractLabelledColumnStandard {
        PdfExtractLabelledColumnStandard::new(LabelledColumnArgs::new(
            text_sel(label),
            neighbour,
            heading,
            BlockType::FUND_NAME,
            "fund",
        ))
    }

    fn content(blocks: &[PdfBlock]) -> Vec<String> {
        blocks.iter().map(|b| b.content.as_str().unwrap_or_default().to_string()).collect()
    }

    mod the_anchor {
        use super::*;

        #[test]
        fn a_page_without_the_label_yields_nothing() {
            let p = page(vec![line("Something else", (10.0, 10.0, 80.0, 20.0))]);
            assert!(pipe("^Product name", absent(), absent()).call(&p).is_empty());
        }

        #[test]
        fn the_label_alone_yields_the_label() {
            // The label is still attached on purpose: stripping it is the text_filter's business.
            let p = page(vec![line("Product name:", (10.0, 10.0, 80.0, 20.0))]);
            assert_eq!(content(&pipe("^Product name", absent(), absent()).call(&p)), ["Product name:"]);
        }

        #[test]
        fn the_first_match_anchors_the_column_when_the_label_occurs_twice() {
            // Two labels side by side: the first anchors, and the neighbour anchor is what keeps
            // the second column out. Without one the column reaches the page's edge — which is the
            // next test, and the reason the annex always states its identifier anchor.
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("FIRST", (10.0, 24.0, 80.0, 34.0)),
                line("Product name:", (300.0, 10.0, 370.0, 20.0)),
                line("SECOND", (300.0, 24.0, 370.0, 34.0)),
            ]);
            let got = content(&pipe("^Product name", text_sel("^Product name"), absent()).call(&p));
            assert_eq!(got, ["Product name: FIRST"]);
        }

        #[test]
        fn without_a_neighbour_anchor_the_column_reaches_the_page_edge() {
            // Stated rather than guarded against: the column's right edge comes from the neighbour
            // selection alone, so a pipe given none takes the whole width. Two labels on one band
            // then land in one block, which is why this pipe is never configured without one.
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("FIRST", (10.0, 24.0, 80.0, 34.0)),
                line("SECOND", (300.0, 24.0, 370.0, 34.0)),
            ]);
            let got = content(&pipe("^Product name", absent(), absent()).call(&p));
            assert_eq!(got, ["Product name: FIRSTSECOND"]);
        }
    }

    mod the_bands {
        use super::*;

        #[test]
        fn fragments_on_one_band_join_without_a_space() {
            // Where the PDF breaks a word in half: `G` + `REAT`.
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("G", (10.0, 24.0, 18.0, 34.0)),
                line("REAT FUND", (18.0, 24.0, 90.0, 34.0)),
            ]);
            assert_eq!(content(&pipe("^Product name", absent(), absent()).call(&p))[0], "Product name: GREAT FUND");
        }

        #[test]
        fn one_band_is_read_left_to_right_whatever_order_the_generator_wrote_it() {
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("REAT FUND", (18.0, 24.0, 90.0, 34.0)),
                line("G", (10.0, 24.0, 18.0, 34.0)),
            ]);
            assert_eq!(content(&pipe("^Product name", absent(), absent()).call(&p))[0], "Product name: GREAT FUND");
        }

        #[test]
        fn separate_bands_join_with_a_space() {
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("FIRST", (10.0, 24.0, 80.0, 34.0)),
                line("SECOND", (10.0, 40.0, 80.0, 50.0)),
            ]);
            assert_eq!(
                content(&pipe("^Product name", absent(), absent()).call(&p))[0],
                "Product name: FIRST SECOND"
            );
        }

        #[test]
        fn a_fragment_hanging_below_its_own_line_stays_on_it() {
            // AMUNDI-EN24 p777: `G` hangs two points below, and a band that grew to meet it would
            // put `G`+`REAT` at the tail of the name instead of in the middle.
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("G", (10.0, 23.0, 18.0, 35.0)),
                line("REAT", (18.0, 24.0, 50.0, 34.0)),
                line("SECOND", (10.0, 40.0, 80.0, 50.0)),
            ]);
            assert_eq!(
                content(&pipe("^Product name", absent(), absent()).call(&p))[0],
                "Product name: GREAT SECOND"
            );
        }
    }

    mod the_two_anchors {
        use super::*;

        #[test]
        fn the_neighbour_on_the_labels_line_bounds_the_column_on_the_right() {
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("Legal entity identifier", (200.0, 10.0, 320.0, 20.0)),
                line("THE FUND", (10.0, 24.0, 90.0, 34.0)),
                line("549300ABCDEF", (200.0, 24.0, 290.0, 34.0)),
            ]);
            let got = content(&pipe("^Product name", text_sel("^Legal entity identifier"), absent()).call(&p));
            assert_eq!(got[0], "Product name: THE FUND");
        }

        #[test]
        fn the_neighbour_below_the_label_closes_the_block_instead() {
            // UBS-EN23, ASTERIA-EN24: the identifier sits under the label, so it closes.
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("THE FUND", (10.0, 24.0, 90.0, 34.0)),
                line("Legal entity identifier", (10.0, 40.0, 130.0, 50.0)),
                line("549300ABCDEF", (10.0, 56.0, 100.0, 66.0)),
            ]);
            let got = content(&pipe("^Product name", text_sel("^Legal entity identifier"), absent()).call(&p));
            assert_eq!(got[0], "Product name: THE FUND");
        }

        #[test]
        fn the_heading_closes_the_block_when_nothing_sits_under_the_label() {
            // AMUNDI, CARNE-EN23: nothing below until the band heading.
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("THE FUND", (10.0, 24.0, 90.0, 34.0)),
                line("Sustainable investment objective", (10.0, 40.0, 200.0, 50.0)),
                line("beyond the band", (10.0, 56.0, 100.0, 66.0)),
            ]);
            let got = content(&pipe("^Product name", absent(), text_sel("^Sustainable investment objective")).call(&p));
            assert_eq!(got[0], "Product name: THE FUND");
        }

        #[test]
        fn the_nearer_of_the_two_anchors_wins() {
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("THE FUND", (10.0, 24.0, 90.0, 34.0)),
                line("Legal entity identifier", (10.0, 40.0, 130.0, 50.0)),
                line("Sustainable investment objective", (10.0, 70.0, 200.0, 80.0)),
            ]);
            let got = content(
                &pipe(
                    "^Product name",
                    text_sel("^Legal entity identifier"),
                    text_sel("^Sustainable investment objective"),
                )
                .call(&p),
            );
            assert_eq!(got[0], "Product name: THE FUND");
        }

        #[test]
        fn a_heading_beside_the_label_does_not_bound_the_column_but_is_read_with_it() {
            // The heading is an end-of-block anchor and never an edge: it narrows nothing. On the
            // label's own band it is therefore inside the column, like any other line there — the
            // name is bounded on the right by the neighbour, and by nothing else.
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("Sustainable investment objective", (100.0, 10.0, 290.0, 20.0)),
                line("A VERY LONG FUND NAME", (10.0, 24.0, 250.0, 34.0)),
            ]);
            let got = content(&pipe("^Product name", absent(), text_sel("^Sustainable investment")).call(&p));
            assert_eq!(got[0], "Product name:Sustainable investment objective A VERY LONG FUND NAME");
        }

        #[test]
        fn a_heading_below_the_label_closes_the_block_without_being_read() {
            let p = page(vec![
                line("Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("A VERY LONG FUND NAME", (10.0, 24.0, 250.0, 34.0)),
                line("Sustainable investment objective", (10.0, 48.0, 200.0, 58.0)),
            ]);
            let got = content(&pipe("^Product name", absent(), text_sel("^Sustainable investment")).call(&p));
            assert_eq!(got[0], "Product name: A VERY LONG FUND NAME");
        }
    }

    mod the_prepended_word {
        use super::*;

        #[test]
        fn it_goes_in_right_after_the_label_so_a_prefix_removal_leaves_it_at_the_head() {
            let p = page(vec![
                line("Nome del prodotto:", (10.0, 10.0, 80.0, 20.0)),
                line("Mediolanum Flessibile", (10.0, 24.0, 120.0, 34.0)),
            ]);
            let args = LabelledColumnArgs::new(
                text_sel("^Nome del prodotto"),
                absent(),
                absent(),
                BlockType::FUND_NAME,
                "fund",
            )
            .with_prepend("FONDO");
            let got = content(&PdfExtractLabelledColumnStandard::new(args).call(&p));
            assert_eq!(got[0], "Nome del prodotto: FONDO Mediolanum Flessibile");
        }

        #[test]
        fn an_empty_prepend_adds_nothing() {
            let p = page(vec![line("Product name: THE FUND", (10.0, 10.0, 180.0, 20.0))]);
            assert_eq!(content(&pipe("^Product name", absent(), absent()).call(&p))[0], "Product name: THE FUND");
        }
    }
}
