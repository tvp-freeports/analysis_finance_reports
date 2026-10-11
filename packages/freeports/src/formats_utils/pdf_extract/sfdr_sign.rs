//! [`PdfExtractSfdrSignStandard`]: *the tick in the box, assigned by position*.
//!
//! The SFDR periodic disclosure asks, first of all, *did this financial product have a sustainable
//! investment objective?*, and offers two boxes. The ticked one is the answer: affirmative means
//! article 9, negative article 8. Until this pipe existed the engine did not read the box at all —
//! it read the **title** of the annex, matching `…referred to in Article 8…` as a substring, which
//! works only where the title is there and says so. Measured over the reference collection and the
//! 905-document corpus, the title is missing on **236** disclosures, two hundred of them in the
//! format with the most documents of all.
//!
//! # What this pipe decides, and what it refuses to decide
//!
//! It decides **where the tick is** and **which of the two labels it belongs to** — geometry. It
//! does not name an article: mapping *affirmative* to article 9 is an interpretation of what the
//! question means, and by `R23` that belongs to the `text_filter`. So the block carries an
//! [`SfdrAnswer`], the recognizer that found it, and the title line as it stands, and the next
//! segment turns those into a classification.
//!
//! # The five recognizers, and why they are one pipe
//!
//! The mechanism is a property of the **disclosure**, not of the format: four documents of the
//! reference collection use more than one, and `FINECO-EN23@LUX` uses three. So the recognizers are
//! five ways of finding a candidate on the same band of the same page, tried in order, and a format
//! enables them all unless it has a reason not to.
//!
//! | # | recognizer | what it looks for |
//! |---|---|---|
//! | 1 | [`SfdrSignKind::TextMark`] | a line that **is** a mark, or **begins** with one — `X`, `X Oui`, `X No` |
//! | 2 | [`SfdrSignKind::SymbolGlyph`] | one character of a symbol font that the mark table calls ticked |
//! | 3 | [`SfdrSignKind::Image`] | a small raster image on the answer row: alone it is the tick, in a pair the heavier one is |
//! | 4 | [`SfdrSignKind::OcrGlyph`] | the same as 1 and 2, but in a `HiddenHorz…` font, i.e. written by an OCR pass |
//! | 5 | [`SfdrSignKind::VectorPath`] | a small, dark, many-segmented drawn path |
//!
//! # Why the marks have to be named
//!
//! A recognizer that looked for "a character outside ordinary text, or a dingbat font" would be
//! wrong on the second-largest block of disclosures in the corpus. `funddsymbol` writes the **empty**
//! box as the letter `U` and the **ticked** one as the letter `C`: two ordinary ASCII letters,
//! indistinguishable by codepoint or by size, and the empty one comes first on the page. Taking the
//! first symbolic character there reads 131 disclosures out of 133 backwards. Hence
//! [`SfdrMarks`], which names what counts as ticked and what counts as blank, carries the measured
//! defaults, and lets a format add to all three parts when a new typography turns up.
//!
//! # The row below, when the first question is not ticked
//!
//! Two disclosures out of the 838 measured leave both boxes of the first question empty and answer
//! only the row underneath — and on one of them the missing tick is visibly **misprinted onto the
//! question's own text**, twenty-three points above the box it belongs to. Rather than lose the
//! row, the pipe retries on a second pair of labels when the format supplies one
//! ([`SfdrSignArgs::fallback_labels`]: *«it made sustainable investments…»* and *«it promoted
//! environmental/social characteristics…»*), warns that it did so, and records it in the block.
//! The mapping is the same one, so the `text_filter` needs no second rule.
//!
//! # What the band is measured in
//!
//! The **label's own height** — one line of text — and never a number of points. It is what
//! separates the first question's row from the one below it on every page measured (twenty points
//! apart on `UBS-EN23`, twenty-four on `ASTERIA-EN24`, thirty-four on `DANSKEINVEST-EN24`), and it
//! is what keeps the boxes of `EURIZON-IT24`, twenty by twenty-four points, from being thrown away
//! by a fixed maximum. A fixed maximum is precisely what once made the engine look blind to them.

use std::collections::{BTreeMap, BTreeSet};

use crate::commons::geometry::Rectangle;
use crate::commons::sets::Container;
use crate::core::classes::{BlockType, BlockValue, PdfBlock};
use crate::core::page::{Page, PageDrawing, PageImage};
use crate::core::pipeline::{PdfExtractPipe, PipeError};

use super::pdf_line::PdfLine;
use super::relative::RelativeInfo;
use super::select::relative::PdfLineSelection;
use super::standard_funcs::PdfExtractStandardFuncsError;

/// The space of a **symbol font**, which a PDF maps into the private-use area together with the
/// rest of the font: `U+F020` is byte `0x20` in Wingdings, and it is a space in every sense except
/// Unicode's own.
///
/// `str::trim` does not remove it, because `char::is_whitespace` answers for Unicode and not for a
/// font's encoding. A disclosure that writes its answer as *space, tick* in one text object —
/// BLACKROCK-EN25 writes all four of its own that way — therefore arrives here as a line whose
/// first character is not a mark, and goes unread: four articles lost with no trace but the pipe's
/// own warning.
const SYMBOL_SPACE: char = '\u{F020}';

/// Whitespace, plus the symbol-font space above.
fn is_space(character: char) -> bool {
    character.is_whitespace() || character == SYMBOL_SPACE
}

/// Which of the two boxes carries the tick.
///
/// Named for the **question**, not for the article, because the pipe does not know what the question
/// means: `labels.0` is whatever the format measured as the affirmative answer, in whatever language
/// the page happens to use, and `labels.1` the negative one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfdrAnswer {
    Affirmative,
    Negative,
}

impl SfdrAnswer {
    /// The word written into the block's metadata, and read back by the `text_filter`.
    pub fn as_str(self) -> &'static str {
        match self {
            SfdrAnswer::Affirmative => "affirmative",
            SfdrAnswer::Negative => "negative",
        }
    }
}

/// The five ways a disclosure marks its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SfdrSignKind {
    /// A mark character alone on its line, or at the head of the label's own line.
    TextMark,
    /// One character of a symbol font that the mark table calls ticked.
    SymbolGlyph,
    /// A small raster image on the answer row.
    Image,
    /// A mark character written by an OCR pass, in a `HiddenHorz…` font.
    OcrGlyph,
    /// A small, dark, many-segmented vector path.
    VectorPath,
}

impl SfdrSignKind {
    /// Every recognizer, in the order they are tried — which is the order of how much they cover.
    pub const ALL: [SfdrSignKind; 5] = [
        SfdrSignKind::TextMark,
        SfdrSignKind::SymbolGlyph,
        SfdrSignKind::Image,
        SfdrSignKind::OcrGlyph,
        SfdrSignKind::VectorPath,
    ];

    /// The word written into the block's metadata.
    pub fn as_str(self) -> &'static str {
        match self {
            SfdrSignKind::TextMark => "text-mark",
            SfdrSignKind::SymbolGlyph => "symbol-glyph",
            SfdrSignKind::Image => "image",
            SfdrSignKind::OcrGlyph => "ocr-glyph",
            SfdrSignKind::VectorPath => "vector-path",
        }
    }
}

/// Which row answered: the question itself, or the pair of options under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SfdrAnsweredBy {
    Question,
    RowBelow,
}

impl SfdrAnsweredBy {
    pub fn as_str(self) -> &'static str {
        match self {
            SfdrAnsweredBy::Question => "question",
            SfdrAnsweredBy::RowBelow => "row-below",
        }
    }
}

/// What counts as a tick, and what counts as an empty box.
///
/// Three parts rather than one, because the same character means opposite things in different
/// typographies. [`SfdrMarks::by_font`] wins over everything else for the fonts it names: in
/// `funddsymbol` only `C` is a tick, so `U`, `B` and `o` — which the same font also draws on the
/// same row — are not candidates at all, and no global list has to know about them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SfdrMarks {
    /// Characters that mean *ticked* in any font the table does not name.
    pub ticked: BTreeSet<char>,
    /// Characters that mean *empty box* or *bullet*, in any font. Never a candidate.
    pub blank: BTreeSet<char>,
    /// Per-font tick characters, by normalised font name. A font listed here admits **only** what
    /// is listed for it.
    pub by_font: BTreeMap<String, BTreeSet<char>>,
}

impl Default for SfdrMarks {
    /// The measured table: every tick and every empty box seen in 838 disclosures.
    fn default() -> Self {
        Self {
            ticked: "Xx\u{2713}\u{2714}\u{2717}\u{2718}\u{2612}\u{25A0}\u{F0FE}\u{F0FC}\u{F0FD}"
                .chars()
                .collect(),
            blank: "\u{2610}\u{25A1}\u{2022}\u{26AB}\u{F06C}\u{F0A7}\u{F0B7}\u{F0A8}\u{F06F}"
                .chars()
                .collect(),
            by_font: BTreeMap::from([
                // Eurizon's and Fineco's annexes: `C` is the ticked box, `U` the empty one.
                ("funddsymbol".to_string(), BTreeSet::from(['C'])),
                // Wingdings 2 draws the ticked box on the digit three.
                ("wingdings2".to_string(), BTreeSet::from(['3'])),
            ]),
        }
    }
}

impl SfdrMarks {
    /// Whether `character`, written in `font`, is a tick.
    fn is_ticked(&self, character: char, font: &str) -> bool {
        if self.blank.contains(&character) {
            return false;
        }
        match self.by_font.get(font) {
            Some(allowed) => allowed.contains(&character),
            None => self.ticked.contains(&character),
        }
    }

    /// Adds to the table without replacing it — what a format's `marks=` does.
    pub fn extended(
        mut self,
        ticked: impl IntoIterator<Item = char>,
        blank: impl IntoIterator<Item = char>,
        by_font: impl IntoIterator<Item = (String, BTreeSet<char>)>,
    ) -> Self {
        self.ticked.extend(ticked);
        self.blank.extend(blank);
        for (font, characters) in by_font {
            self.by_font.entry(font).or_default().extend(characters);
        }
        self
    }
}

/// How a format configures [`PdfExtractSfdrSignStandard`].
#[derive(Clone)]
pub struct SfdrSignArgs {
    /// The **first** question of the disclosure. The answer row opens under its box.
    pub question: PdfLineSelection,
    /// The two answer labels, **affirmative first**, measured on the page and not deduced from the
    /// document's language: one `AMUNDI-EN24` report writes `Yes`/`No` on a hundred disclosures and
    /// `X Oui`/`Non` on two.
    pub labels: (PdfLineSelection, PdfLineSelection),
    /// The two options of the row below, used only when the question's own row carries no tick.
    pub fallback_labels: Option<(PdfLineSelection, PdfLineSelection)>,
    /// The line that names the annex, carried down untouched for the `text_filter` to check.
    pub title: Option<PdfLineSelection>,
    /// What counts as a tick. [`SfdrMarks::default`] unless the format adds to it.
    pub marks: SfdrMarks,
    /// Which recognizers to try. All five unless the format says otherwise.
    pub signs: Vec<SfdrSignKind>,
}

impl SfdrSignArgs {
    /// The common case: a question, two labels, and everything else at its default.
    pub fn new(question: PdfLineSelection, labels: (PdfLineSelection, PdfLineSelection)) -> Self {
        Self {
            question,
            labels,
            fallback_labels: None,
            title: None,
            marks: SfdrMarks::default(),
            signs: SfdrSignKind::ALL.to_vec(),
        }
    }
}

/// How far to either side of the labels a tick may sit, in label heights.
///
/// Four, measured: the widest gap between a label and its own box is forty-five points on
/// `ASTERIA-EN24`, whose labels are twelve points high.
const SIDE_REACH: f32 = 4.0;

/// How far above and below the answer row a candidate may sit, in label heights. One: the next
/// question's row is never closer than one and a half.
const ROW_REACH: f32 = 1.0;

/// How wide a raster image may be before it is decoration rather than a tick, as a fraction of the
/// distance between the two labels.
///
/// The European template's own icon is seventy-two points wide where a tick is ten, and it sits on
/// the same band; a third of the distance between the labels separates them on every page measured,
/// without writing a size in points that `EURIZON-IT24`'s twenty-by-twenty-four boxes would fail.
const IMAGE_WIDTH_SHARE: f32 = 3.0;

/// A tick found on the page: where it is, and which recognizer found it.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    x0: f32,
    kind: SfdrSignKind,
}

/// The geometry of one answer row: its two labels and the window a tick may occupy.
#[derive(Clone, Copy, Debug)]
struct AnswerRow {
    middle: f32,
    top: f32,
    bottom: f32,
    left: f32,
    right: f32,
    label_span: f32,
    /// The bottom of the labels' own boxes, before the search band grew around them.
    ///
    /// Kept apart from `bottom` because the two answer different questions. `bottom` is *how far
    /// down a tick may still belong to this row*, and it reaches a whole line below the labels;
    /// this is *where this row's text ends*, and it is the only honest floor for the row that comes
    /// after. Measured on UBS-EN23 p189: the band ends at 293.4 and the next row begins at 288.7,
    /// so a fallback searched below the band finds nothing while the row is plainly there.
    labels_bottom: f32,
}

/// *The tick in the box, assigned by position.*
///
/// Replaces `PdfExtractSfdrArticleStandard`, which deduced the article from the presence of a text
/// fragment in the annex's title. See the module documentation for what it reads and what it leaves
/// to the `text_filter`.
pub struct PdfExtractSfdrSignStandard {
    args: SfdrSignArgs,
}

impl PdfExtractSfdrSignStandard {
    pub fn new(args: SfdrSignArgs) -> Self {
        Self { args }
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let lines = &page.lines;
        let question = self
            .topmost(&self.args.question, lines, None, None)
            .ok_or_else(|| PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound {
                name: "SFDR question".to_string(),
            })?;
        let question_bottom = question.bbox().as_tuple().3;

        let row = self.row(&self.args.labels, lines, question_bottom).ok_or_else(|| {
            PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { name: "SFDR answer labels".to_string() }
        })?;

        let (answer, kind, answered_by) = match self.answer(page, &row) {
            Some((answer, kind)) => (answer, kind, SfdrAnsweredBy::Question),
            None => {
                let Some(fallback) = &self.args.fallback_labels else {
                    tracing::warn!(
                        "the first question of this disclosure carries no tick, and the format \
                         declares no second row to fall back on: the article is not read"
                    );
                    return Ok(Vec::new());
                };
                let Some(below) = self.row(fallback, lines, row.labels_bottom) else {
                    tracing::warn!(
                        "the first question of this disclosure carries no tick and the row below it \
                         is not on the page: the article is not read"
                    );
                    return Ok(Vec::new());
                };
                match self.answer(page, &below) {
                    Some((answer, kind)) => {
                        tracing::warn!(
                            "the first question of this disclosure has both boxes empty: the article \
                             is read from the row below it, which is ticked"
                        );
                        (answer, kind, SfdrAnsweredBy::RowBelow)
                    }
                    None => {
                        tracing::warn!(
                            "neither the first question of this disclosure nor the row below it \
                             carries a tick: the article is not read"
                        );
                        return Ok(Vec::new());
                    }
                }
            }
        };

        let mut metadata = BTreeMap::from([
            ("answer".to_string(), BlockValue::from(answer.as_str())),
            ("sign".to_string(), BlockValue::from(kind.as_str())),
            ("answered_by".to_string(), BlockValue::from(answered_by.as_str())),
        ]);
        if let Some(title) = &self.args.title {
            // Above the question, so that what is checked is the annex's own title and not a line
            // further down the page — the body of a disclosure names its article more than once.
            let question_top = question.bbox().as_tuple().1;
            if let Some(line) = self.topmost(title, lines, None, Some(question_top)) {
                metadata.insert("title".to_string(), BlockValue::from(line.text().as_str()));
            }
        }
        tracing::debug!(sign = kind.as_str(), answer = answer.as_str(), "SFDR tick found");
        Ok(vec![PdfBlock::new(BlockType::SFDR_ARTICLE, metadata, BlockValue::from(""))])
    }

    /// The topmost line the selection takes, optionally restricted to the band between `floor` and
    /// `ceiling`.
    fn topmost<'a>(
        &self,
        selection: &PdfLineSelection,
        lines: &'a [PdfLine],
        floor: Option<f32>,
        ceiling: Option<f32>,
    ) -> Option<&'a PdfLine> {
        let set = selection.clone().contextualize(lines);
        lines
            .iter()
            .filter(|line| set.contains(*line))
            .filter(|line| {
                let top = line.bbox().as_tuple().1;
                floor.is_none_or(|floor| top >= floor) && ceiling.is_none_or(|ceiling| top < ceiling)
            })
            .min_by(|a, b| {
                a.bbox().as_tuple().1.partial_cmp(&b.bbox().as_tuple().1).unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// The geometry of the answer row whose labels are `labels`, taken below `floor`.
    ///
    /// The floor is the bottom of the question's box, less one label height: on one format the
    /// labels sit level with the text above them rather than under it, and a strict "below" would
    /// find nothing.
    fn row(
        &self,
        labels: &(PdfLineSelection, PdfLineSelection),
        lines: &[PdfLine],
        floor: f32,
    ) -> Option<AnswerRow> {
        let affirmative = self.topmost(&labels.0, lines, Some(floor - 1.0), None)?;
        let negative = self.topmost(&labels.1, lines, Some(floor - 1.0), None)?;
        let (ax0, ay0, ax1, ay1) = affirmative.bbox().as_tuple();
        let (nx0, ny0, nx1, ny1) = negative.bbox().as_tuple();
        let top = ay0.min(ny0);
        let bottom = ay1.max(ny1);
        let height = (bottom - top).max(1.0);
        let label_span = (nx1.max(ax1) - nx0.min(ax0)).abs();
        Some(AnswerRow {
            middle: (ax0 + nx0) / 2.0,
            top: top - ROW_REACH * height,
            bottom: bottom + ROW_REACH * height,
            left: ax0.min(nx0) - SIDE_REACH * height,
            right: ax1.max(nx1) + SIDE_REACH * height,
            label_span,
            labels_bottom: bottom,
        })
    }

    fn inside(&self, row: &AnswerRow, x0: f32, centre_y: f32) -> bool {
        row.top <= centre_y && centre_y <= row.bottom && row.left <= x0 && x0 <= row.right
    }

    /// Runs the enabled recognizers in order and returns the first answer one of them gives.
    fn answer(&self, page: &Page, row: &AnswerRow) -> Option<(SfdrAnswer, SfdrSignKind)> {
        let mut found: BTreeMap<SfdrSignKind, Vec<Candidate>> = BTreeMap::new();
        for line in &page.lines {
            if let Some(candidate) = self.line_candidate(line, row) {
                found.entry(candidate.kind).or_default().push(candidate);
            }
        }
        found.entry(SfdrSignKind::Image).or_default().extend(self.image_candidates(&page.images, row));
        found.entry(SfdrSignKind::VectorPath).or_default().extend(self.drawing_candidates(&page.drawings, row));

        for kind in &self.args.signs {
            let Some(candidates) = found.get(kind).filter(|candidates| !candidates.is_empty()) else {
                continue;
            };
            if let Some(answer) = self.assign(candidates, row) {
                return Some((answer, *kind));
            }
        }
        None
    }

    /// The side the candidates fall on, or nothing when they contradict each other.
    fn assign(&self, candidates: &[Candidate], row: &AnswerRow) -> Option<SfdrAnswer> {
        let affirmative = candidates.iter().any(|candidate| candidate.x0 < row.middle);
        let negative = candidates.iter().any(|candidate| candidate.x0 >= row.middle);
        match (affirmative, negative) {
            (true, false) => Some(SfdrAnswer::Affirmative),
            (false, true) => Some(SfdrAnswer::Negative),
            // Both boxes ticked: the page answers its own question twice, and choosing one of the
            // two would be a guess. Never seen in 838 disclosures, and worth saying out loud.
            (true, true) => {
                tracing::warn!(
                    "both answers of this question carry a tick: the disclosure contradicts itself \
                     and the article is not read from it"
                );
                None
            }
            (false, false) => None,
        }
    }

    /// A tick written as text: a mark alone on its line, or at the head of the label's own line.
    fn line_candidate(&self, line: &PdfLine, row: &AnswerRow) -> Option<Candidate> {
        let (x0, y0, _, y1) = line.bbox().as_tuple();
        let centre_y = (y0 + y1) / 2.0;
        if !self.inside(row, x0, centre_y) {
            return None;
        }
        let text = line.text().trim_matches(is_space);
        let first = text.chars().next()?;
        let font = line.font().inner();
        // A mark is the whole line, or its head followed by a space: `X`, `X Oui`, `X No`. Anything
        // else beginning with an `x` is a word.
        let heads_the_line = text.chars().count() == 1 || text.chars().nth(1).is_some_and(is_space);
        if !heads_the_line || !self.args.marks.is_ticked(first, font) {
            return None;
        }
        let kind = if font.contains("hiddenhorz") {
            SfdrSignKind::OcrGlyph
        } else if first.is_ascii() && !self.args.marks.by_font.contains_key(font) {
            SfdrSignKind::TextMark
        } else {
            SfdrSignKind::SymbolGlyph
        };
        Some(Candidate { x0, kind })
    }

    /// A tick stamped as a raster image.
    ///
    /// Alone on the row it **is** the tick, because the empty box next to it is drawn rather than
    /// stamped — which is how three formats of the reference collection write it. In a pair the
    /// heavier one is the tick; two images of the same weight are two empty boxes and answer
    /// nothing.
    fn image_candidates(&self, images: &[PageImage], row: &AnswerRow) -> Vec<Candidate> {
        let candidates: Vec<&PageImage> = images
            .iter()
            .filter(|image| {
                let (x0, y0, x1, y1) = image.bbox.as_tuple();
                self.inside(row, x0, (y0 + y1) / 2.0)
                    && x1 - x0 < (row.label_span / IMAGE_WIDTH_SHARE).max(1.0)
            })
            .collect();
        let heaviest = candidates.iter().map(|image| image.data.len()).max();
        let lightest = candidates.iter().map(|image| image.data.len()).min();
        match (candidates.len(), heaviest, lightest) {
            (0, _, _) => Vec::new(),
            (_, Some(heaviest), Some(lightest)) if heaviest == lightest && candidates.len() > 1 => Vec::new(),
            (_, Some(heaviest), _) => candidates
                .iter()
                .filter(|image| image.data.len() == heaviest)
                .map(|image| Self::image_candidate(&image.bbox))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn image_candidate(bbox: &Rectangle) -> Candidate {
        Candidate { x0: bbox.as_tuple().0, kind: SfdrSignKind::Image }
    }

    /// A tick drawn as a vector path: dark, small, and made of several segments.
    ///
    /// The empty box is a path too, and on the same row — it is told apart by its fill, which is
    /// white where the tick's is black, and by its shape, which is one or four commands where a
    /// tick is a dozen.
    fn drawing_candidates(&self, drawings: &[PageDrawing], row: &AnswerRow) -> Vec<Candidate> {
        let limit = (row.bottom - row.top).max(1.0);
        drawings
            .iter()
            .filter(|drawing| {
                let (x0, y0, x1, y1) = drawing.bbox.as_tuple();
                let dark = drawing.fill.is_some_and(|(r, g, b)| r + g + b < 0.6);
                dark && drawing.items >= 3
                    && x1 - x0 <= limit
                    && y1 - y0 <= limit
                    && self.inside(row, x0, (y0 + y1) / 2.0)
            })
            .map(|drawing| {
                let x0 = drawing.bbox.as_tuple().0;
                Candidate { x0, kind: SfdrSignKind::VectorPath }
            })
            .collect()
    }
}

impl PdfExtractPipe for PdfExtractSfdrSignStandard {
    fn name(&self) -> &str {
        "PdfExtractSfdrSignStandard"
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

    fn text_sel(pattern: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_text(pattern))
    }

    fn line(text: &str, x0: f32, y0: f32) -> PdfLine {
        PdfLine::new("Arial", 10.0, text, (x0, y0, x0 + 10.0 * text.chars().count() as f32, y0 + 10.0))
    }

    fn glyph(font: &str, text: &str, x0: f32, y0: f32) -> PdfLine {
        PdfLine::new(font, 10.0, text, (x0, y0, x0 + 10.0, y0 + 10.0))
    }

    fn image(x0: f32, y0: f32, width: f32, weight: usize) -> PageImage {
        PageImage {
            bbox: Rectangle::new(x0, y0, x0 + width, y0 + width),
            ext: "png".to_string(),
            data: vec![0u8; weight],
        }
    }

    fn drawing(x0: f32, y0: f32, fill: Option<(f32, f32, f32)>, items: usize) -> PageDrawing {
        PageDrawing { bbox: Rectangle::new(x0, y0, x0 + 9.0, y0 + 9.0), fill, items }
    }

    /// The skeleton every test starts from: the question at the top, `Yes` at `x=100` and `No` at
    /// `x=300` twenty points below it, and the row of follow-up options twenty points below that.
    fn skeleton() -> Vec<PdfLine> {
        vec![
            line("Did this financial product have a sustainable investment objective?", 100.0, 0.0),
            line("Yes", 100.0, 20.0),
            line("No", 300.0, 20.0),
            line("It made sustainable investments", 100.0, 50.0),
            line("It promoted Environmental", 300.0, 50.0),
        ]
    }

    fn args() -> SfdrSignArgs {
        SfdrSignArgs::new(text_sel("sustainable investment objective"), (text_sel("^Yes"), text_sel("^No")))
    }

    fn with_fallback(args: SfdrSignArgs) -> SfdrSignArgs {
        SfdrSignArgs {
            fallback_labels: Some((text_sel("^It made"), text_sel("^It promoted"))),
            ..args
        }
    }

    fn page(lines: Vec<PdfLine>) -> Page {
        Page::new(1, (600.0, 400.0), lines, Vec::new())
    }

    fn read(args: SfdrSignArgs, page: &Page) -> Vec<PdfBlock> {
        PdfExtractSfdrSignStandard::new(args).call(page).unwrap()
    }

    fn metadata(block: &PdfBlock, key: &str) -> String {
        block.metadata.get(key).and_then(BlockValue::as_str).unwrap_or_default().to_string()
    }

    mod the_five_recognizers {
        use super::*;

        #[test]
        fn a_mark_alone_on_its_line_is_read_by_its_abscissa() {
            let mut lines = skeleton();
            lines.push(line("X", 280.0, 21.0));
            let blocks = read(args(), &page(lines));
            assert_eq!(metadata(&blocks[0], "answer"), "negative");
            assert_eq!(metadata(&blocks[0], "sign"), "text-mark");
        }

        #[test]
        fn a_mark_fused_into_the_label_is_read_from_the_label_s_own_abscissa() {
            // `X Oui`, `X No`: two formats write the tick inside the label's line, so the candidate
            // and the label are the same line and the abscissa is the label's. The label selection
            // then has to be written as a suffix — `Yes$` and not `^Yes` — which is the one thing a
            // format author must notice about this layout.
            let lines = vec![
                line("Did this financial product have a sustainable investment objective?", 100.0, 0.0),
                line("X Yes", 100.0, 20.0),
                line("No", 300.0, 20.0),
            ];
            let fused = SfdrSignArgs::new(
                text_sel("sustainable investment objective"),
                (text_sel("Yes$"), text_sel("No$")),
            );
            let blocks = read(fused, &page(lines));
            assert_eq!(metadata(&blocks[0], "answer"), "affirmative");
        }

        #[test]
        fn a_word_beginning_with_an_x_is_not_a_mark() {
            let mut lines = skeleton();
            lines.push(line("Xenon holdings", 280.0, 21.0));
            assert!(read(args(), &page(lines)).is_empty());
        }

        #[test]
        fn a_symbol_font_glyph_is_read() {
            let mut lines = skeleton();
            lines.push(glyph("Wingdings", "\u{F0FE}", 280.0, 21.0));
            let blocks = read(args(), &page(lines));
            assert_eq!(metadata(&blocks[0], "answer"), "negative");
            assert_eq!(metadata(&blocks[0], "sign"), "symbol-glyph");
        }

        #[test]
        fn an_image_alone_on_the_row_is_the_tick() {
            // Three formats stamp the ticked box and draw the empty one, so there is no pair to
            // compare and the single image is the answer.
            let images = vec![image(280.0, 21.0, 10.0, 1047)];
            let page = Page::new(1, (600.0, 400.0), skeleton(), images);
            let blocks = read(args(), &page);
            assert_eq!(metadata(&blocks[0], "answer"), "negative");
            assert_eq!(metadata(&blocks[0], "sign"), "image");
        }

        #[test]
        fn of_two_images_the_heavier_one_is_the_tick() {
            let images = vec![image(80.0, 21.0, 11.0, 777), image(280.0, 21.0, 11.0, 2797)];
            let page = Page::new(1, (600.0, 400.0), skeleton(), images);
            assert_eq!(metadata(&read(args(), &page)[0], "answer"), "negative");
        }

        #[test]
        fn two_images_of_the_same_weight_are_two_empty_boxes() {
            let images = vec![image(80.0, 21.0, 11.0, 229), image(280.0, 21.0, 11.0, 229)];
            let page = Page::new(1, (600.0, 400.0), skeleton(), images);
            assert!(read(args(), &page).is_empty());
        }

        #[test]
        fn a_glyph_in_an_ocr_font_is_its_own_recognizer() {
            let mut lines = skeleton();
            lines.push(glyph("HiddenHorzOCR", "\u{2713}", 280.0, 21.0));
            let blocks = read(args(), &page(lines));
            assert_eq!(metadata(&blocks[0], "sign"), "ocr-glyph");
        }

        #[test]
        fn a_dark_many_segmented_path_is_the_tick() {
            let page = page(skeleton()).with_drawings(vec![drawing(280.0, 21.0, Some((0.0, 0.0, 0.0)), 12)]);
            let blocks = read(args(), &page);
            assert_eq!(metadata(&blocks[0], "answer"), "negative");
            assert_eq!(metadata(&blocks[0], "sign"), "vector-path");
        }

        #[test]
        fn a_white_path_is_the_empty_box_next_to_it() {
            let page = page(skeleton()).with_drawings(vec![drawing(280.0, 21.0, Some((1.0, 1.0, 1.0)), 12)]);
            assert!(read(args(), &page).is_empty());
        }

        #[test]
        fn a_dark_path_of_one_command_is_a_rule_not_a_tick() {
            let page = page(skeleton()).with_drawings(vec![drawing(280.0, 21.0, Some((0.0, 0.0, 0.0)), 1)]);
            assert!(read(args(), &page).is_empty());
        }
    }

    mod the_marks_have_to_be_named {
        use super::*;

        /// `funddsymbol` writes the empty box as `U` and the ticked one as `C`, and draws the empty
        /// one first. Taking "the first symbolic character" here is what read a hundred and
        /// thirty-one disclosures backwards.
        fn eurizon_row() -> Vec<PdfLine> {
            let mut lines = skeleton();
            lines.push(glyph("FunddSymbol", "U", 80.0, 21.0));
            lines.push(glyph("FunddSymbol", "C", 280.0, 21.0));
            lines
        }

        #[test]
        fn the_empty_box_of_a_symbol_font_is_not_a_candidate() {
            let blocks = read(args(), &page(eurizon_row()));
            assert_eq!(metadata(&blocks[0], "answer"), "negative");
        }

        #[test]
        fn a_font_named_in_the_table_admits_only_what_the_table_lists() {
            let mut lines = skeleton();
            lines.push(glyph("FunddSymbol", "B", 80.0, 21.0));
            lines.push(glyph("FunddSymbol", "o", 90.0, 21.0));
            assert!(read(args(), &page(lines)).is_empty());
        }

        #[test]
        fn an_empty_box_glyph_is_never_a_tick() {
            let mut lines = skeleton();
            lines.push(glyph("MSGothic", "\u{2610}", 80.0, 21.0));
            lines.push(glyph("Wingdings", "\u{F0FE}", 280.0, 21.0));
            assert_eq!(metadata(&read(args(), &page(lines))[0], "answer"), "negative");
        }

        #[test]
        fn the_template_s_bullets_are_not_ticks() {
            let mut lines = skeleton();
            lines.push(glyph("Helvetica", "\u{2022}", 80.0, 21.0));
            assert!(read(args(), &page(lines)).is_empty());
        }

        /// BLACKROCK-EN25 writes the answer as **two characters in one text object**: the symbol
        /// font's own space and then the mark. `U+F020` is not Unicode whitespace, so `trim` left
        /// it in place and the line's first character was not a mark — all four of the report's
        /// disclosures went unread, with nothing but the pipe's warning to say so.
        #[test]
        fn a_mark_behind_the_symbol_font_s_own_space_is_still_the_mark() {
            let mut lines = skeleton();
            lines.push(glyph("Wingdings", "\u{F020}\u{F0FD}", 280.0, 21.0));
            assert_eq!(metadata(&read(args(), &page(lines))[0], "answer"), "negative");
        }

        /// And the guard that the space was there for still holds: a word beginning with an `x` is
        /// a word, whichever kind of space does or does not follow it.
        #[test]
        fn a_word_beginning_with_a_mark_is_still_a_word() {
            let mut lines = skeleton();
            lines.push(glyph("Arial", "Xerox", 280.0, 21.0));
            assert!(read(args(), &page(lines)).is_empty());
        }

        /// Wingdings draws four marks on four consecutive codepoints — `U+F0FC` a check, `U+F0FD`
        /// a cross, `U+F0FE` a boxed check, `U+F0FF` a boxed cross — and the table had the first
        /// and the third. BlackRock's annex answers with the second, which is the same statement:
        /// the plain cross `U+2717` and the boxed one `U+2612` are already ticks, so a cross in a
        /// dingbat font is not a different kind of answer, it is the same answer in a font the
        /// table had not met.
        #[test]
        fn the_wingdings_cross_is_a_tick() {
            let mut lines = skeleton();
            lines.push(glyph("Wingdings", "\u{F0FD}", 280.0, 21.0));
            assert_eq!(metadata(&read(args(), &page(lines))[0], "answer"), "negative");
        }

        #[test]
        fn a_format_can_add_a_tick_the_table_does_not_know() {
            let mut lines = skeleton();
            lines.push(glyph("NewSymbol", "\u{2756}", 280.0, 21.0));
            let plain = SfdrSignArgs { marks: SfdrMarks::default(), ..args() };
            assert!(read(plain, &page(lines.clone())).is_empty());
            let extended = SfdrSignArgs {
                marks: SfdrMarks::default().extended(['\u{2756}'], [], []),
                ..args()
            };
            assert_eq!(metadata(&read(extended, &page(lines))[0], "answer"), "negative");
        }

        #[test]
        fn a_format_can_add_a_font_of_its_own() {
            let mut lines = skeleton();
            lines.push(glyph("HouseDingbats", "Q", 280.0, 21.0));
            let extended = SfdrSignArgs {
                marks: SfdrMarks::default().extended(
                    [],
                    [],
                    [("housedingbats".to_string(), BTreeSet::from(['Q']))],
                ),
                ..args()
            };
            assert_eq!(metadata(&read(extended, &page(lines))[0], "answer"), "negative");
        }
    }

    mod the_band_of_the_first_question {
        use super::*;

        #[test]
        fn the_tick_of_the_second_row_is_not_the_answer_to_the_first() {
            let mut lines = skeleton();
            lines.push(line("X", 120.0, 51.0));
            assert!(read(args(), &page(lines)).is_empty());
        }

        #[test]
        fn a_tick_far_to_the_left_of_its_label_is_still_its_tick() {
            // Every format draws the box before the word; the widest gap measured is four label
            // heights.
            let mut lines = skeleton();
            lines.push(line("X", 270.0, 21.0));
            assert_eq!(metadata(&read(args(), &page(lines))[0], "answer"), "negative");
        }

        #[test]
        fn a_mark_outside_the_two_labels_entirely_is_not_a_candidate() {
            let mut lines = skeleton();
            lines.push(line("X", 500.0, 21.0));
            assert!(read(args(), &page(lines)).is_empty());
        }

        #[test]
        fn a_wide_image_is_the_template_s_own_icon_and_not_a_tick() {
            let images = vec![image(150.0, 21.0, 72.0, 5000)];
            let page = Page::new(1, (600.0, 400.0), skeleton(), images);
            assert!(read(args(), &page).is_empty());
        }

        #[test]
        fn both_boxes_ticked_answers_nothing() {
            let mut lines = skeleton();
            lines.push(line("X", 80.0, 21.0));
            lines.push(line("X", 280.0, 21.0));
            assert!(read(args(), &page(lines)).is_empty());
        }
    }

    mod the_labels_are_measured_not_deduced {
        use super::*;

        #[test]
        fn a_pair_of_selections_may_be_a_union_of_two_languages() {
            // One AMUNDI report writes `Yes`/`No` on a hundred disclosures and `X Oui`/`Non` on two.
            let labels = (
                OptionallyRelative::Absolute(PdfLineSet::select_text("^Yes") | PdfLineSet::select_text("Oui$")),
                OptionallyRelative::Absolute(PdfLineSet::select_text("^No") | PdfLineSet::select_text("^Non")),
            );
            let lines = vec![
                line("Did this financial product have a sustainable investment objective?", 100.0, 0.0),
                line("X Oui", 100.0, 20.0),
                line("Non", 300.0, 20.0),
            ];
            let args = SfdrSignArgs::new(text_sel("sustainable investment objective"), labels);
            assert_eq!(metadata(&read(args, &page(lines))[0], "answer"), "affirmative");
        }

        #[test]
        fn a_page_without_the_question_fails_the_page() {
            let page = page(vec![line("Yes", 100.0, 20.0), line("No", 300.0, 20.0)]);
            let error = PdfExtractSfdrSignStandard::new(args()).call(&page).unwrap_err();
            assert!(matches!(error, PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { .. }));
        }

        #[test]
        fn a_page_without_the_labels_fails_the_page() {
            let page = page(vec![line(
                "Did this financial product have a sustainable investment objective?",
                100.0,
                0.0,
            )]);
            assert!(PdfExtractSfdrSignStandard::new(args()).call(&page).is_err());
        }
    }

    mod the_row_below_when_the_question_is_blank {
        use super::*;

        #[test]
        fn the_option_on_the_right_means_the_same_as_the_negative_box() {
            let mut lines = skeleton();
            lines.push(line("X", 280.0, 51.0));
            let blocks = read(with_fallback(args()), &page(lines));
            assert_eq!(metadata(&blocks[0], "answer"), "negative");
            assert_eq!(metadata(&blocks[0], "answered_by"), "row-below");
        }

        #[test]
        fn the_option_on_the_left_means_the_same_as_the_affirmative_box() {
            let mut lines = skeleton();
            lines.push(line("X", 80.0, 51.0));
            assert_eq!(metadata(&read(with_fallback(args()), &page(lines))[0], "answer"), "affirmative");
        }

        #[test]
        fn the_first_question_still_wins_when_it_is_ticked() {
            let mut lines = skeleton();
            lines.push(line("X", 80.0, 21.0));
            lines.push(line("X", 280.0, 51.0));
            let blocks = read(with_fallback(args()), &page(lines));
            assert_eq!(metadata(&blocks[0], "answer"), "affirmative");
            assert_eq!(metadata(&blocks[0], "answered_by"), "question");
        }

        /// UBS-EN23 p189, the page that cost a row. `Yes` and `No` sit at `y=270.0` and the
        /// follow-up options at `y=288.7`, while the first question's search band reaches `293.4`:
        /// the second row begins **inside** the band. Searched below the band it is not there;
        /// searched below the labels' own boxes it is, and the Wingdings 2 tick on it is read.
        #[test]
        fn finds_a_second_row_that_begins_inside_the_first_question_s_band() {
            let lines = vec![
                line("Did this financial product have a sustainable investment objective?", 204.6, 250.3),
                line("Yes", 282.0, 270.0),
                line("No", 455.1, 270.0),
                line("It made sustainable investments", 217.7, 288.7),
                line("It promoted Environmental", 388.1, 288.7),
                line("X", 380.0, 291.0),
            ];
            let blocks = read(with_fallback(args()), &page(lines));
            assert_eq!(metadata(&blocks[0], "answer"), "negative");
            assert_eq!(metadata(&blocks[0], "answered_by"), "row-below");
        }

        /// The floor is still a floor: a line **level with** the answer labels is part of the
        /// first row, not the row after it. Here the options begin at `y=277.0`, inside the
        /// labels' own boxes, and are refused.
        #[test]
        fn a_row_level_with_the_labels_is_not_a_second_row() {
            let lines = vec![
                line("Did this financial product have a sustainable investment objective?", 204.6, 250.3),
                line("Yes", 282.0, 270.0),
                line("No", 455.1, 270.0),
                line("It made sustainable investments", 217.7, 277.0),
                line("It promoted Environmental", 388.1, 277.0),
                line("X", 380.0, 291.0),
            ];
            assert!(read(with_fallback(args()), &page(lines)).is_empty());
        }

        #[test]
        fn a_format_that_declares_no_second_row_simply_reads_nothing() {
            let mut lines = skeleton();
            lines.push(line("X", 280.0, 51.0));
            assert!(read(args(), &page(lines)).is_empty());
        }

        #[test]
        fn a_page_where_neither_row_is_ticked_reads_nothing() {
            assert!(read(with_fallback(args()), &page(skeleton())).is_empty());
        }
    }

    mod what_the_block_carries {
        use super::*;

        #[test]
        fn the_title_line_is_carried_down_untouched() {
            let mut lines = skeleton();
            lines.insert(0, line("Template periodic disclosure ... referred to in Article 8", 100.0, -20.0));
            lines.push(line("X", 280.0, 21.0));
            let args = SfdrSignArgs { title: Some(text_sel("referred to in Article")), ..args() };
            let blocks = read(args, &page(lines));
            assert!(metadata(&blocks[0], "title").contains("Article 8"));
        }

        #[test]
        fn a_line_below_the_question_is_not_the_title() {
            // The body of a disclosure names its own article more than once; only what stands above
            // the first question is the annex's title.
            let mut lines = skeleton();
            lines.push(line("... as set out in Article 9 of the Regulation", 100.0, 80.0));
            lines.push(line("X", 280.0, 21.0));
            let args = SfdrSignArgs { title: Some(text_sel("Article")), ..args() };
            assert!(!read(args, &page(lines))[0].metadata.contains_key("title"));
        }

        #[test]
        fn no_title_selection_means_no_title_in_the_metadata() {
            let mut lines = skeleton();
            lines.push(line("X", 280.0, 21.0));
            assert!(!read(args(), &page(lines))[0].metadata.contains_key("title"));
        }

        #[test]
        fn the_block_is_typed_as_sfdr_article_and_carries_no_fund() {
            let mut lines = skeleton();
            lines.push(line("X", 280.0, 21.0));
            let blocks = read(args(), &page(lines));
            assert_eq!(blocks[0].type_block, BlockType::SFDR_ARTICLE);
            assert_eq!(blocks[0].content, BlockValue::from(""));
        }

        #[test]
        fn the_pipe_never_names_an_article() {
            let mut lines = skeleton();
            lines.push(line("X", 280.0, 21.0));
            assert!(!read(args(), &page(lines))[0].metadata.contains_key("article"));
        }
    }

    mod which_recognizers_are_enabled {
        use super::*;

        #[test]
        fn a_format_may_switch_one_off() {
            let mut lines = skeleton();
            lines.push(line("X", 280.0, 21.0));
            let only_images = SfdrSignArgs { signs: vec![SfdrSignKind::Image], ..args() };
            assert!(read(only_images, &page(lines)).is_empty());
        }

        #[test]
        fn all_five_are_on_by_default() {
            assert_eq!(args().signs.len(), 5);
        }

        #[test]
        fn the_order_is_the_order_of_coverage() {
            assert_eq!(SfdrSignKind::ALL[0], SfdrSignKind::TextMark);
            assert_eq!(SfdrSignKind::ALL[2], SfdrSignKind::Image);
            assert_eq!(SfdrSignKind::ALL[4], SfdrSignKind::VectorPath);
        }

        #[test]
        fn the_pipe_name_identifies_it_in_errors() {
            assert_eq!(PdfExtractPipe::name(&PdfExtractSfdrSignStandard::new(args())), "PdfExtractSfdrSignStandard");
        }
    }
}
