//! Building a [`PdfLineSelection`] from external configuration: [`pdfline_selection_from_dict`] and
//! [`pdfline_selection_from_str`].
//!
//! A format author writes a selection either as a structured mapping — font, size, area, text — or
//! in a compact one-line grammar. Both end up as the same absolute selection, the string form
//! parsing into the structured one and delegating, so there is a single place where a selection is
//! built.
//!
//! Neither has anything to do with PyMuPDF; they live under `input::document` because that is where
//! the public API places them.

use once_cell::sync::Lazy;
use onig::Regex;

use crate::commons::geometry::RectangleBuildError;
// Used only by the tests below, which call `contains` in a helper; the `cfg(test)` avoids an
// unused-import warning in the normal build.
#[cfg(test)]
use crate::commons::sets::Container;
use crate::formats_utils::pdf_extract::position::{InputArea, PositionError};
use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
use crate::formats_utils::pdf_extract::select::pdf_line::area::Area;
use crate::formats_utils::pdf_extract::select::pdf_line::font::FontSet;
use crate::formats_utils::pdf_extract::select::pdf_line::font_size::FontSizeInterval;
use crate::formats_utils::pdf_extract::select::pdf_line::text::TextSet;
use crate::formats_utils::pdf_extract::select::pdf_line::{PdfLineSet, SelectPdfLineSet};
use crate::formats_utils::pdf_extract::select::relative::{
    PdfLineSelection, RelativePdfLineSet, RelativeSelectPdfLineSet,
};

/// A single font or a list of them. Untagged for serde, so a scalar becomes `Single` and a sequence
/// `Multiple`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(untagged)]
pub enum FontCriterion {
    Single(String),
    Multiple(Vec<String>),
}

/// One side of an area, in the structured form: a coordinate, or a line of the page.
///
/// Untagged, so a number stays a number and a mapping is a **nested selection** — the structured
/// spelling of the compact grammar's `<expression>` bound. `{x_max: {text: "^Situazione a fine"}}`
/// is *as far right as that heading*, which is what a statement whose columns move between
/// documents of the same format has to say.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(untagged)]
pub enum InputBound {
    Number(f32),
    Anchor(Box<InputPdfLineSet>),
}

impl InputBound {
    /// The side as a coordinate, or `None` when it is an anchor — the honest answer for a side
    /// whose value depends on a page this function never sees.
    fn number(&self) -> Option<f32> {
        match self {
            InputBound::Number(value) => Some(*value),
            InputBound::Anchor(_) => None,
        }
    }

    fn is_anchor(&self) -> bool {
        matches!(self, InputBound::Anchor(_))
    }
}

/// The deserialization-side mirror of `InputArea`: four optional bounds, not yet validated.
/// `InputArea::build` stays the single point of validation.
#[derive(Debug, Clone, PartialEq, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputAreaSpec {
    #[serde(default)]
    pub x_min: Option<InputBound>,
    #[serde(default)]
    pub x_max: Option<InputBound>,
    #[serde(default)]
    pub y_min: Option<InputBound>,
    #[serde(default)]
    pub y_max: Option<InputBound>,
}

impl InputAreaSpec {
    /// Whether any side is an anchor, which is what makes the selection relative.
    fn has_anchor(&self) -> bool {
        self.sides()
            .iter()
            .any(|side| side.as_ref().is_some_and(InputBound::is_anchor))
    }

    fn sides(&self) -> [&Option<InputBound>; 4] {
        [&self.x_min, &self.x_max, &self.y_min, &self.y_max]
    }

    /// The four sides as the coordinates they are, an anchor counting as absent. What
    /// `InputArea::build` validates: a side it is not given is a side it cannot compare.
    fn numbers(&self) -> [Option<f32>; 4] {
        self.sides()
            .map(|side| side.as_ref().and_then(InputBound::number))
    }
}

/// A `window(..)` term in the structured form: *the box of the line this anchor picks, moved and
/// scaled*.
///
/// Four numbers and an anchor, none of them optional — the same shape and the same reading as the
/// compact grammar's term, for which see [`WindowSpec`]. The anchor is a whole selection because
/// every measured one is qualified by a font: the word that anchors it occurs in the body of the
/// page too.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputWindowSpec {
    /// The line the window hangs from.
    pub anchor: Box<InputPdfLineSet>,
    /// The displacement of the window's top-left corner from the anchor's, in anchor widths.
    pub dx: f32,
    /// The same, in anchor heights.
    pub dy: f32,
    /// The window's width, in anchor widths.
    pub width: f32,
    /// Its height, in anchor heights.
    pub height: f32,
}

/// A font size written as external configuration: one exact size, or an interval.
///
/// Untagged for serde, so a scalar becomes `Exact` and a two-element sequence `Interval`. The
/// exact form is not an interval with equal bounds: it widens by [`FONT_SIZE_PRECISION`], because
/// a size written by hand never matches the stored float to the last bit — `6` against `5.99814`
/// is a real measurement from EURIZON-IT24, and it cost a whole page.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(untagged)]
pub enum FontSizeCriterion {
    Exact(f32),
    Interval(f32, f32),
}

/// The structured form of a line selection: the criteria it states, intersected, minus what
/// `without` subtracts.
///
/// Nine keys, and the whole of the compact grammar is among them — which is the point. The two
/// forms were not equally expressive while the area took four numbers and nothing else: a
/// selection needing an anchored side, a window or a difference had to be written as a string,
/// and a formats repository ended up with both spellings side by side for no reason the author
/// chose. The three forms the mapping could not state are `area`'s [`InputBound`], `window` and
/// `without`; `any_of`, `all_of` and `font_of` are three more that the compact grammar does not
/// have either, and the mapping is now strictly the richer of the two.
///
/// Every spec written before those keys existed reads exactly as it did: they are optional, and a
/// numeric side is still a number.
#[derive(Debug, Clone, PartialEq, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPdfLineSet {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub font: Option<FontCriterion>,
    #[serde(default)]
    pub font_size: Option<FontSizeCriterion>,
    #[serde(default)]
    pub area: Option<InputAreaSpec>,
    /// A band measured off the line an anchor picks, intersected with the rest like any other
    /// criterion.
    #[serde(default)]
    pub window: Option<InputWindowSpec>,
    /// The font **taken from another line**: whichever family the line this anchor picks is set
    /// in, intersected with the rest like any other criterion.
    ///
    /// It is the one criterion that cannot be written down, because it is not a property of the
    /// lines being selected but of a line somewhere else on the same page. MEDIOLANUM-ES24.A names
    /// no font anywhere: its portfolio is typeset in whatever family the column heading
    /// `Descripción de la cartera` happens to use, and three of its selections — the body, the
    /// fund heading, the currency — say exactly that. Writing a family instead would pin the
    /// format to one typesetting of one document.
    ///
    /// The engine has had [`RelativeSelectPdfLineSet::select_font_of`] all along; this is its
    /// spelling in configuration. The compact grammar has none, which is why a selection using it
    /// is written as a mapping — and the mapping is the form to write anyway.
    #[serde(default)]
    pub font_of: Option<Box<InputPdfLineSet>>,
    /// The **union** of several selections, intersected with whatever else this spec states.
    ///
    /// The one form that has no compact spelling, and the reason it exists is measured: a report
    /// paginated as a book prints the page number on the left of an even page and on the right of
    /// an odd one, so the number is *in either of two areas* and in no single one. The compact
    /// grammar unites fonts (`R8`) and nothing else, and writing two separate selection cells
    /// instead would say *two selections* where the format means one.
    ///
    /// An empty list means the same as leaving the key out.
    #[serde(default)]
    pub any_of: Vec<InputPdfLineSet>,
    /// Several selections **intersected**, beside the ones this spec's own keys state.
    ///
    /// The mapping's keys already intersect, so this is for the case they cannot reach: **two
    /// criteria of the same kind**. FINECO-EN23@IR's table is bounded above by a heading *and* by
    /// an ordinate — the heading closes the table where there is one, the ordinate keeps the page
    /// foot out where there is not — and that is two `area`s, which one `area` key cannot be.
    ///
    /// An empty list means the same as leaving the key out.
    #[serde(default)]
    pub all_of: Vec<InputPdfLineSet>,
    /// What is **taken out** of this selection: the compact grammar's ` / `, as a list because the
    /// operator folds. Each element is a whole selection, and may subtract in its turn.
    ///
    /// Applied last, as the outermost operation: `(criteria & all_of & any_of) / without`.
    #[serde(default)]
    pub without: Vec<InputPdfLineSet>,
}

impl InputPdfLineSet {
    /// Whether the absolute construction path can build this spec: no anchored side, no window,
    /// nothing subtracted.
    fn is_absolute(&self) -> bool {
        self.window.is_none()
            && self.font_of.is_none()
            && self.without.is_empty()
            && self.any_of.is_empty()
            && self.all_of.is_empty()
            && !self.area.as_ref().is_some_and(InputAreaSpec::has_anchor)
    }

    /// Whether the spec states any criterion of its **own** — a union aside.
    fn states_own_criterion(&self) -> bool {
        self.text.is_some()
            || self.font.is_some()
            || self.font_size.is_some()
            || self.area.is_some()
            || self.window.is_some()
            || self.font_of.is_some()
    }

    /// Whether the spec states any criterion at all.
    ///
    /// A spec that states none is the selection of *every* line, and a cell is allowed to say so —
    /// but not where it would be a silent disaster: an anchor, or an operand of a difference. See
    /// [`LineSelectionError::NoCriterion`]. `without` alone does not count: subtracting from
    /// everything is the shape the compact grammar refuses as a first operand. A union does, since
    /// a union of selections that each state something states something.
    pub(crate) fn states_a_criterion(&self) -> bool {
        self.states_own_criterion() || !self.any_of.is_empty() || !self.all_of.is_empty()
    }

    /// The spec's criteria in the form the two construction paths share, its `without` left aside.
    fn parts(&self) -> SelectionParts {
        let bound = |side: &Option<InputBound>| match side {
            None => BoundSpec::Absent,
            Some(InputBound::Number(value)) => BoundSpec::Number(*value),
            Some(InputBound::Anchor(spec)) => BoundSpec::Spec(spec.clone()),
        };
        SelectionParts {
            font: self.font.clone(),
            font_of: self.font_of.clone(),
            font_size: self.font_size,
            text: self.text.clone(),
            sides: self.area.as_ref().map(|area| {
                (
                    bound(&area.x_min),
                    bound(&area.x_max),
                    bound(&area.y_min),
                    bound(&area.y_max),
                )
            }),
        }
    }

    /// The spec's window as the one the compact path builds, so that both spellings of a window go
    /// through the same leaf.
    fn windows(&self) -> Vec<WindowSpec> {
        self.window
            .iter()
            .map(|window| WindowSpec {
                anchor: WindowAnchor::Spec(window.anchor.clone()),
                displacement: (window.dx, window.dy),
                width: window.width,
                height: window.height,
            })
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LineSelectionError {
    #[error("font_size must be positive, found {0}")]
    FontSizeNotPositive(f32),
    /// An interval whose upper bound is not above its lower one. Rejected rather than silently
    /// swapped: a cell written `[21:18]` says something its author did not mean, and a selection
    /// that matches nothing is the hardest kind of configuration defect to find.
    #[error("font_size interval must go upwards, found [{0}:{1}]")]
    FontSizeIntervalInverted(f32, f32),
    /// `font` is present but empty. Reducing an empty list of alternatives has no meaningful
    /// answer, so it is a typed error rather than a panic.
    #[error("font list must not be empty when provided")]
    EmptyFontList,
    #[error(transparent)]
    Area(#[from] PositionError),
    /// The four sides passed `InputArea`'s own validation and still make no rectangle.
    ///
    /// `InputArea::build` checks each side it was *given* — positive, and `max` above `min` — but a
    /// side left out is filled in here with `0.0` or `1e6`, and those are not compared against
    /// anything. An `x_min` above `1e6` with no `x_max` is the shape that gets through: both checks
    /// pass, and the rectangle comes out inverted.
    #[error("the area of this selection is not a rectangle: {0}")]
    DegenerateArea(#[from] RectangleBuildError),
    /// A `window(..)` term that is not an anchor followed by four numbers.
    #[error(
        "this window term is not an anchor followed by four numbers ({reason}): window({term})"
    )]
    Window { term: String, reason: String },
    /// An expression that states no criterion at all, where one is required.
    ///
    /// The grammar answers an expression it does not recognise with the selection of *every* line,
    /// and for a cell that leaves a criterion out that is the honest answer: `ArialMT` with no area
    /// means every ArialMT line, and a cell is allowed to say so. In two places it would be a
    /// silent disaster instead — the anchor of a `window(..)`, which would hang the window off
    /// whichever line happens to come first on the page, and an operand of a difference, which
    /// would subtract the whole page from it. Both are refused here.
    #[error("this expression states no criterion, so it would select every line: {0}")]
    NoCriterion(String),
}

/// The precision of the font-size interval built around an exact size.
const FONT_SIZE_PRECISION: f32 = 1e-3;

/// The font-size interval a criterion describes, with the two refusals it can carry.
///
/// Shared by the two construction paths of the compact grammar — the absolute one through
/// [`pdfline_selection_from_dict`] and the relative one through [`relative_bounds_selection`] — so
/// that a size refused in one is refused in the other, and in the same words.
fn font_size_interval(
    criterion: FontSizeCriterion,
) -> Result<FontSizeInterval, LineSelectionError> {
    match criterion {
        FontSizeCriterion::Exact(size) if size <= 0.0 => {
            Err(LineSelectionError::FontSizeNotPositive(size))
        }
        FontSizeCriterion::Exact(size) => {
            Ok(FontSizeInterval::from_precision(size, FONT_SIZE_PRECISION))
        }
        FontSizeCriterion::Interval(low, _) if low <= 0.0 => {
            Err(LineSelectionError::FontSizeNotPositive(low))
        }
        FontSizeCriterion::Interval(low, high) if high <= low => {
            Err(LineSelectionError::FontSizeIntervalInverted(low, high))
        }
        FontSizeCriterion::Interval(low, high) => Ok(FontSizeInterval::new(low, high)),
    }
}

/// The font set a criterion describes: one name, or the union of the names observed (`R8`).
///
/// Shared by the two construction paths. See [`font_size_interval`].
fn font_set(criterion: &FontCriterion) -> Result<FontSet, LineSelectionError> {
    match criterion {
        FontCriterion::Single(font) => Ok(FontSet::new(font)),
        FontCriterion::Multiple(fonts) => fonts
            .iter()
            .map(|font| FontSet::new(font))
            .reduce(|a, b| a | b)
            .ok_or(LineSelectionError::EmptyFontList),
    }
}

/// Builds a [`PdfLineSelection`] from the structured form.
///
/// Absolute wherever the spec is — the path every spec written before `area`'s anchors, `window`
/// and `without` existed takes, unchanged, down to the order the criteria intersect in — and
/// relative as soon as one of those three appears, for the same reason the compact grammar's are:
/// an anchor names a line of a page this function never sees.
pub fn pdfline_selection_from_dict(
    data: &InputPdfLineSet,
) -> Result<PdfLineSelection, LineSelectionError> {
    // Called once per selection spec while a formats repository loads, potentially thousands of
    // times across a whole repository: never above `trace!`.
    tracing::trace!(?data, "building a pdf line selection from a dict spec");

    if !data.is_absolute() {
        return Ok(OptionallyRelative::Relative(dict_expression(data)?));
    }
    absolute_from_dict(data)
}

/// The structured form as a relative tree: `(criteria & all_of & any_of) / without`.
///
/// The difference folds left, as ` / ` does; an operand carrying a `without` or an `any_of` of its
/// own nests, which the compact grammar cannot write and which costs nothing to allow here.
fn dict_expression(data: &InputPdfLineSet) -> Result<RelativePdfLineSet, LineSelectionError> {
    let own = match data.states_own_criterion() {
        true => Some(selection_node(data.parts(), data.windows(), || {
            describe(data)
        })?),
        false => None,
    };

    let mut intersection = None;
    for operand in &data.all_of {
        let one = dict_expression(operand)?;
        intersection = Some(match intersection {
            None => one,
            Some(accumulated) => accumulated & one,
        });
    }

    let mut union = None;
    for operand in &data.any_of {
        let one = dict_expression(operand)?;
        union = Some(match union {
            None => one,
            Some(accumulated) => accumulated | one,
        });
    }

    let stated = [own, intersection, union]
        .into_iter()
        .flatten()
        .reduce(|a, b| a & b);
    // Unreachable through `pdfline_selection_from_dict`, which sends a spec stating nothing down
    // the absolute path, and raised for an anchor or an operand — the two places where *every
    // line* must not pass for an answer.
    let mut node = stated.ok_or_else(|| LineSelectionError::NoCriterion(describe(data)))?;
    for operand in &data.without {
        node = node / dict_expression(operand)?;
    }
    Ok(node)
}

/// A spec in the one place *every line* must not pass for an answer: an anchor.
///
/// The refusal is here and not in [`pdfline_selection_from_dict`], because an empty spec at the
/// top level is a legitimate way to say *the whole page* and has always built one.
fn anchor_from_dict(data: &InputPdfLineSet) -> Result<PdfLineSelection, LineSelectionError> {
    if !data.states_a_criterion() {
        return Err(LineSelectionError::NoCriterion(describe(data)));
    }
    pdfline_selection_from_dict(data)
}

/// A spec as a refusal quotes it: the keys it states, which is what its author can look for.
///
/// The derived `Debug` would name the five it does not state as well, and the one that matters
/// would be the hardest word in the line to find.
fn describe(data: &InputPdfLineSet) -> String {
    let mut keys = Vec::new();
    if data.text.is_some() {
        keys.push("text");
    }
    if data.font.is_some() {
        keys.push("font");
    }
    if data.font_size.is_some() {
        keys.push("font_size");
    }
    if data.area.is_some() {
        keys.push("area");
    }
    if data.window.is_some() {
        keys.push("window");
    }
    if data.font_of.is_some() {
        keys.push("font_of");
    }
    if !data.all_of.is_empty() {
        keys.push("all_of");
    }
    if !data.any_of.is_empty() {
        keys.push("any_of");
    }
    if !data.without.is_empty() {
        keys.push("without");
    }
    if keys.is_empty() {
        return "a selection spec stating nothing".to_string();
    }
    format!("a selection spec stating {}", keys.join(", "))
}

/// The original construction path: the criteria that are present, intersected, all absolute.
fn absolute_from_dict(data: &InputPdfLineSet) -> Result<PdfLineSelection, LineSelectionError> {
    let font_size_set = match data.font_size {
        None => None,
        Some(criterion) => Some(font_size_interval(criterion)?),
    };

    let area_set = match &data.area {
        Some(spec) => {
            let [x_min, x_max, y_min, y_max] = spec.numbers();
            let input_area = InputArea::build(x_min, x_max, y_min, y_max)?;
            let x_min = input_area.x_min().unwrap_or(0.0);
            let y_min = input_area.y_min().unwrap_or(0.0);
            let x_max = input_area.x_max().unwrap_or(1e6);
            let y_max = input_area.y_max().unwrap_or(1e6);
            // `build`, not `new`: these four numbers are written by hand in a formats repository,
            // and two of them may be the fallbacks above rather than anything the author chose. A
            // selection nobody can satisfy is a defect in that repository and is reported as such —
            // it must not be a panic, which would cost the whole run for one line of CSV.
            Some(Area::build(x_min, y_min, x_max, y_max)?)
        }
        None => None,
    };

    let text_set = data.text.as_deref().map(TextSet::new);

    let base = PdfLineSet::from_sets(None, font_size_set, text_set, area_set);

    let selection = match &data.font {
        None => base,
        Some(criterion) => PdfLineSet::font(font_set(criterion)?) & base,
    };

    Ok(OptionallyRelative::Absolute(selection))
}

// The compact selection grammar: font, then `[font_size]`, then an area — either a bare vertical
// range or a full area wrapped in an extra pair of brackets — then quoted text, each separated by
// an optional space and every group optional.
//
// Three of the groups accept more than one shape, and all three arrived after the grammar did:
//
// - **the font may be a union**, written with `|` — `Lato,Bold|TrebuchetMS-Bold|Open Sans,Bold`.
//   The separator is `|` and not a comma because a comma is part of a real font name
//   (`TimesNewRoman,Bold`), while `|` cannot occur in one. It is `R8` written in a cell: the font
//   name enters a selection as the union of the variants actually observed, and a format whose
//   statement is typeset in four families cannot name one;
// - **the size may be an interval**, written `[low:high]`. `[size]` keeps meaning exactly what it
//   meant — one size widened by `FONT_SIZE_PRECISION` — and the colon is what tells the two
//   apart, so no cell written before this existed changes meaning;
// - **an area bound may be a text anchor** instead of a number, written in quotes:
//   `(("Condensed Schedule":"Domicile")(1:800))` means *from the line reading "Condensed Schedule"
//   to the one reading "Domicile"*. See [`BoundSpec`] for what each side takes from its anchor and
//   [`relative_bounds_selection`] for why it is a second construction path rather than a third
//   shape of the first one.
//
// The five useful groups are captured by position rather than by name. Anchored at the start,
// because the matcher searches anywhere by default.

/// One side of an area in the grammar: digits, a quoted text anchor, or nothing.
///
/// Written once and substituted into both patterns, which differ only in their final anchor. A
/// quoted anchor may contain anything but a quote — a colon and a bracket included, which is why
/// the pair is split by [`split_outside_quotes`] and not by `splitn`.
///
/// Surrounding spaces are allowed and dropped: an anchor is long enough that
/// `(("Condensed Schedule" : "Domicile")(1:800))` is worth being able to write.
///
/// An anchor comes in two shapes. `"text"` is the short one, for the case where the text alone
/// identifies the line. `<…>` holds a **whole selection written in this same grammar**, which is
/// what the measured formats need: every anchor of the eight statements of net assets is qualified
/// by a font, because the word that anchors them — `Notes`, `Assets` — occurs in the body of the
/// page too, in another family. Hence `<helvetica-bold[7.4:7.6] "Notes">`; and `<"Notes">` means
/// exactly what `"Notes"` means.
const BOUND: &str = r#" *(?:[0-9]+(?:\.[0-9]+)?|"[^"]*"|<[^<>]*>)? *"#;

/// The grammar's pattern, with `tail` as its final anchor (empty, or `\z`).
fn line_set_pattern(tail: &str) -> String {
    let b = BOUND;
    format!(
        r#"\A([\w\-, |]+)? ?(?:\[([0-9]+(?:\.[0-9]+)?(?::[0-9]+(?:\.[0-9]+)?)?)\])? ?(?:(\({b}:{b}\))|\((\({b}:{b}\)\({b}:{b}\))\))? ?(?:"(.*)")?{tail}"#
    )
}

static LINE_SET_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&line_set_pattern("")).expect("fixed, hand-written pattern, valid onig regex")
});

/// The same pattern as [`LINE_SET_REGEXP`], anchored **at the end too**.
///
/// It exists for validating a formats repository's tables, not for parsing. Every group of the
/// pattern is optional, so the un-anchored version matches *any* string and
/// [`pdfline_selection_from_str`] never rejects anything — a mistyped cell would silently produce
/// an empty selection. Validation needs to be able to say "this cell is not a selection", which is
/// what this pattern answers.
static LINE_SET_ANCHORED_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&line_set_pattern(r"\z")).expect("fixed, hand-written pattern, valid onig regex")
});

/// Whether `input` is, in its entirety, a line selection written in the compact grammar.
///
/// See `LINE_SET_ANCHORED_REGEXP` for why this is not the same question as whether
/// [`pdfline_selection_from_str`] returned `Ok`.
///
/// The pattern alone is not the whole answer any more, because two of the grammar's forms are not
/// in it: a `window(..)` term, which is taken out of the string before the pattern runs, and the
/// difference, which separates two whole expressions. So the question is asked of each operand of
/// the difference, with its windows removed — and of each window's anchor in turn, which is an
/// expression of this same grammar.
pub fn is_pdfline_selection(input: &str) -> bool {
    split_difference(input)
        .into_iter()
        .all(|operand| match Operand::parse(operand) {
            Ok(operand) => operand.is_well_formed(),
            Err(_) => false,
        })
}

/// One side of an area, as the cell wrote it.
///
/// `Anchor` is what makes a selection **relative**: the side is not a coordinate but *whichever
/// line of this page reads that text*, and which of its edges is taken depends on the side —
/// [`crate::formats_utils::pdf_extract::select::relative::RelativeArea`] takes the **inner** one,
/// so a left side takes the anchor's right edge and a right side its left edge. That is what makes
/// `(("Total":"Domicile")(1:800))` read as *between the two columns* rather than *over them*.
#[derive(Debug, Clone, PartialEq)]
enum BoundSpec {
    /// The side was left empty: the area is unbounded there.
    Absent,
    Number(f32),
    /// `"text"`: the line reading that text, whatever its typography.
    Text(String),
    /// `<expression>`: the line a selection written in this same grammar picks. The form the
    /// measured statements need, because their anchors are qualified by a font.
    Expression(String),
    /// The same anchor, written as a nested [`InputPdfLineSet`] — what a side of the structured
    /// form's `area` holds. Not reachable from the compact grammar: the two spellings of an
    /// anchored side meet one step further down, at [`BoundSpec::to_bound`].
    Spec(Box<InputPdfLineSet>),
}

impl BoundSpec {
    /// The side as the cell wrote it. The pattern has already guaranteed the shape.
    fn parse(text: &str) -> Self {
        let text = text.trim();
        if text.is_empty() {
            return BoundSpec::Absent;
        }
        if let Some(anchor) = text
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        {
            return BoundSpec::Text(anchor.to_string());
        }
        if let Some(expression) = text
            .strip_prefix('<')
            .and_then(|rest| rest.strip_suffix('>'))
        {
            return BoundSpec::Expression(expression.trim().to_string());
        }
        BoundSpec::Number(
            text.parse::<f32>()
                .expect("digits matched by LINE_SET_REGEXP always parse as f32"),
        )
    }

    fn is_anchor(&self) -> bool {
        matches!(
            self,
            BoundSpec::Text(_) | BoundSpec::Expression(_) | BoundSpec::Spec(_)
        )
    }

    fn number(&self) -> Option<f32> {
        match self {
            BoundSpec::Number(v) => Some(*v),
            _ => None,
        }
    }

    /// The side in the form [`RelativeSelectPdfLineSet::area_from_bounds`] takes, `fallback`
    /// standing in for an absent one — the same fallbacks [`pdfline_selection_from_dict`] uses.
    ///
    /// An `Expression` is parsed by this same grammar, which is what makes the two anchor shapes
    /// one thing: `"text"` is the case the short form covers, and the long one covers the rest
    /// without a second syntax to learn. The recursion terminates because each level strips a pair
    /// of angle brackets and the pattern forbids a nested one.
    fn to_bound(
        &self,
        fallback: f32,
    ) -> Result<OptionallyRelative<f32, PdfLineSelection>, LineSelectionError> {
        Ok(match self {
            BoundSpec::Absent => OptionallyRelative::Absolute(fallback),
            BoundSpec::Number(v) => OptionallyRelative::Absolute(*v),
            BoundSpec::Text(text) => OptionallyRelative::Relative(OptionallyRelative::Absolute(
                PdfLineSet::text(TextSet::new(text)),
            )),
            BoundSpec::Expression(expression) => {
                OptionallyRelative::Relative(pdfline_selection_from_str(expression)?)
            }
            BoundSpec::Spec(spec) => OptionallyRelative::Relative(anchor_from_dict(spec)?),
        })
    }
}

/// Splits an `"a:b"` pair, without its brackets, on the colon that separates the two **sides**.
///
/// A colon occurs inside a side twice over, so a plain `splitn` is wrong twice:
///
/// - inside a quoted anchor — `"Product name:"` is how most labels are written;
/// - inside a nested expression — `<arialnarrow-bold[8.9:9.1] "Notes">` carries a size interval,
///   and its colon is not inside quotes.
///
/// Hence both a quote state and an angle depth. The one that bit first was the second: a cell that
/// parses as a regex is not a cell that parses as a grammar.
fn split_outside_quotes(text: &str) -> (&str, &str) {
    let mut in_quotes = false;
    let mut depth = 0usize;
    for (index, character) in text.char_indices() {
        match character {
            '"' => in_quotes = !in_quotes,
            '<' if !in_quotes => depth += 1,
            '>' if !in_quotes => depth = depth.saturating_sub(1),
            ':' if !in_quotes && depth == 0 => return (&text[..index], &text[index + 1..]),
            _ => {}
        }
    }
    (text, "")
}

/// Splits an `"a:b"` pair into the two sides as the cell wrote them.
fn parse_bound_pair_spec(text: &str) -> (BoundSpec, BoundSpec) {
    let (a, b) = split_outside_quotes(text);
    (BoundSpec::parse(a), BoundSpec::parse(b))
}

/// Parses the compact grammar into a selection.
///
/// Three steps, in this order, because each one leaves the next exactly what it knew before:
///
/// 1. the **difference** splits the string into operands, on a ` / ` that sits outside quotes and
///    outside brackets. Spaces are required around it so that no cell written before this existed
///    can change meaning;
/// 2. every **`window(..)`** term is taken out of each operand, leaving a string the positional
///    pattern reads exactly as it always did;
/// 3. what is left goes through the pattern, and then either through
///    [`pdfline_selection_from_dict`] — the original path, for a wholly absolute cell — or through
///    [`Operand::node`], which builds the relative tree.
///
/// An expression with a difference or a window is **relative** even when every criterion in it is
/// absolute, because the tree is the only form that can hold a set operation. That is invisible
/// downstream: a relative selection with absolute leaves contextualises to what the absolute one
/// would have been.
pub fn pdfline_selection_from_str(input: &str) -> Result<PdfLineSelection, LineSelectionError> {
    // Same volume caveat as [`pdfline_selection_from_dict`]: `trace!` only.
    tracing::trace!(input, "parsing a compact pdf line selection expression");

    let operands = split_difference(input);
    let (first, rest) = operands
        .split_first()
        .expect("split_difference always yields one operand");
    if !rest.is_empty() {
        return Ok(OptionallyRelative::Relative(relative_expression(input)?));
    }

    let operand = Operand::parse(first)?;
    if operand.is_absolute() {
        return pdfline_selection_from_dict(&operand.into_dict());
    }
    Ok(OptionallyRelative::Relative(operand.node(first)?))
}

/// The whole expression as a relative tree: the difference folded over its operands, left to
/// right, so that `A / B / C` means `(A / B) / C`.
///
/// Also the way an **anchor** is built, wherever one appears — the bound of an area or the target
/// of a window — which is what lets an anchor be any expression of this grammar, difference
/// included, and what makes the refusal of an anchor that states nothing apply to every one of
/// them.
fn relative_expression(input: &str) -> Result<RelativePdfLineSet, LineSelectionError> {
    let operands = split_difference(input);
    let (first, rest) = operands
        .split_first()
        .expect("split_difference always yields one operand");
    let mut node = Operand::parse(first)?.node(first)?;
    for operand in rest {
        node = node / Operand::parse(operand)?.node(operand)?;
    }
    Ok(node)
}

/// Splits `"(a:b)(c:d)"` into its two pairs, each without its brackets.
///
/// The separator is the `)(` that sits **outside** quotes: an anchor may carry brackets of its own
/// — `"Regulation (EU) 2020/852"` is one of the measured ones — and splitting on the first `)(`
/// found would cut it in half.
fn split_area_pairs(area_text: &str) -> (&str, &str) {
    let bytes = area_text.as_bytes();
    let mut in_quotes = false;
    let mut depth = 0usize;
    for index in 0..bytes.len().saturating_sub(1) {
        match bytes[index] {
            b'"' => in_quotes = !in_quotes,
            b'<' if !in_quotes => depth += 1,
            b'>' if !in_quotes => depth = depth.saturating_sub(1),
            b')' if !in_quotes && depth == 0 && bytes[index + 1] == b'(' => {
                return (
                    area_text[..index].trim_start_matches('('),
                    area_text[index + 2..].trim_end_matches(')'),
                );
            }
            _ => {}
        }
    }
    // Unreachable through LINE_SET_REGEXP, which only ever hands over a well-formed pair of pairs.
    (area_text.trim_start_matches('('), "")
}

// ---------------------------------------------------------------------------------------------
// The structural scanners, and the two forms that are not in the pattern
// ---------------------------------------------------------------------------------------------

/// How deep a scan over an expression currently is: inside quotes, inside `<…>`, inside `(…)`.
///
/// The three new splitters below ask the same question of every character — *is this one
/// structure, or is it part of something else?* — so the bookkeeping is written once here. The two
/// splitters of a bound pair do not use it: they run on the **contents** of a bracket pair, where
/// `split_area_pairs` reads a round bracket as the structure it is looking for rather than as
/// nesting.
#[derive(Default)]
struct Nesting {
    in_quotes: bool,
    angles: usize,
    rounds: usize,
}

impl Nesting {
    /// Takes one character in, and answers whether it sat at **top level**, which is the state as
    /// it was *before* that character: a `(` is a character of the depth it opens from, not of the
    /// one it opens.
    fn step(&mut self, character: char) -> bool {
        let top = self.at_top();
        match character {
            '"' => self.in_quotes = !self.in_quotes,
            '<' if !self.in_quotes => self.angles += 1,
            '>' if !self.in_quotes => self.angles = self.angles.saturating_sub(1),
            '(' if !self.in_quotes => self.rounds += 1,
            ')' if !self.in_quotes => self.rounds = self.rounds.saturating_sub(1),
            _ => {}
        }
        top
    }

    fn at_top(&self) -> bool {
        !self.in_quotes && self.angles == 0 && self.rounds == 0
    }
}

/// Splits an expression on the differences at top level, yielding one operand or several.
///
/// The operator is a `/`, as it is in the Python the formats are written in today, and it has to
/// be **surrounded by spaces**. The spaces are not decoration: a slash with no space around it
/// occurs inside the text of a real selection — `"Regulation (EU) 2020/852"` is one of the
/// measured ones — and although that one is inside quotes and the scan knows it, requiring the
/// spaces means no cell that existed before this did can change meaning for a reason that depends
/// on the scan being right.
fn split_difference(input: &str) -> Vec<&str> {
    let mut operands = Vec::new();
    let mut start = 0usize;
    let mut nesting = Nesting::default();
    let mut previous = None;
    let mut characters = input.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        let top = nesting.step(character);
        let spaced = previous == Some(' ') && characters.peek().map(|(_, next)| *next) == Some(' ');
        if top && character == '/' && spaced {
            operands.push(input[start..index].trim());
            start = index + 1;
        }
        previous = Some(character);
    }
    operands.push(input[start..].trim());
    operands
}

/// The byte offsets of the commas at top level.
fn top_level_commas(text: &str) -> Vec<usize> {
    let mut nesting = Nesting::default();
    let mut found = Vec::new();
    for (index, character) in text.char_indices() {
        if nesting.step(character) && character == ',' {
            found.push(index);
        }
    }
    found
}

/// The offset of the `)` that closes the bracket at `open`, or `None` if nothing does.
fn matching_bracket(text: &str, open: usize) -> Option<usize> {
    let mut nesting = Nesting::default();
    for (index, character) in text[open..].char_indices() {
        nesting.step(character);
        if character == ')' && nesting.at_top() {
            return Some(open + index);
        }
    }
    None
}

/// An expression with its surrounding angle brackets removed, if it had any.
///
/// A window's anchor does not need them — the brackets of the term and the comma already say where
/// it ends — but an author who has written a bound anchor writes `<…>` out of habit, and the habit
/// is right about everything except the brackets.
fn strip_angles(text: &str) -> &str {
    text.strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(text)
        .trim()
}

/// The keyword that opens a window term, its bracket included.
const WINDOW_KEYWORD: &str = "window(";

/// The line a window hangs from, in whichever of the two spellings its configuration used.
///
/// One enum rather than two window types: everything else about a window — the four numbers, the
/// leaf it becomes, the refusal of an anchor that states nothing — is the same on both sides, and
/// the anchor is the only part the spelling reaches.
#[derive(Debug, Clone, PartialEq)]
enum WindowAnchor {
    /// An expression of the compact grammar, its angle brackets stripped.
    Compact(String),
    /// A nested structured spec.
    Spec(Box<InputPdfLineSet>),
}

impl WindowAnchor {
    /// The anchor as the selection that will pick its line.
    ///
    /// Both arms refuse an anchor that states no criterion — the compact one inside
    /// [`relative_expression`], the structured one inside [`anchor_from_dict`] — because an anchor
    /// is precisely the place where *every line* must not be allowed to pass for an answer.
    fn selection(&self) -> Result<PdfLineSelection, LineSelectionError> {
        match self {
            WindowAnchor::Compact(expression) => Ok(OptionallyRelative::Relative(
                relative_expression(expression)?,
            )),
            WindowAnchor::Spec(spec) => anchor_from_dict(spec),
        }
    }

    /// Whether the anchor is, in its entirety, written in the grammar it claims. A structured spec
    /// has no spelling to get wrong: serde has already read it.
    fn is_well_formed(&self) -> bool {
        match self {
            WindowAnchor::Compact(expression) => is_pdfline_selection(expression),
            WindowAnchor::Spec(_) => true,
        }
    }
}

/// A `window(..)` term: *the box of the line this anchor picks, moved and scaled*.
///
/// It is the grammar's form for [`RelativeSelectPdfLineSet::area_from_movewindow`], and it is a
/// new form rather than one more kind of bound because it takes four numbers **and** an anchor,
/// where a bound takes one of either. The anchor's own bounding box is the unit of measure: the
/// window starts at the anchor's top-left corner displaced by `(dx·w, dy·h)` and measures `width·w`
/// by `height·h`. So
///
/// ```text
/// window("(valori espressi in", 1.2, -0.1, 100.0, 2.3)
/// ```
///
/// reads *a band far to the right of that label, starting a tenth of a line above it and two lines
/// and a third tall* — which is how MEDIOLANUM-IT24.A names the fund a statement of net assets
/// belongs to. It is the form the author modules reach for most often after the area, and the one
/// thing three of the four statements of net assets still written in Python need.
///
/// Four numbers, none of them optional. The window of a fund-assets column has defaults because
/// one shape of statement is overwhelmingly the common one and a column that follows it should
/// cost nothing to declare; an expression written out in the grammar is read by whoever maintains
/// the format, and there a number that is not written is a number that cannot be read.
#[derive(Debug, Clone, PartialEq)]
struct WindowSpec {
    /// The anchor as the configuration wrote it. Resolved, not stored resolved, because resolving
    /// it means building a selection one level down.
    anchor: WindowAnchor,
    displacement: (f32, f32),
    width: f32,
    height: f32,
}

impl WindowSpec {
    /// Reads the contents of a window term: an anchor, then four numbers.
    ///
    /// Split **from the right**, on the last four commas at top level, because the anchor may
    /// carry commas of its own and a number never does: a font name has one — `TimesNewRoman,Bold`
    /// — and the grammar's font union is a comma-bearing list. So the four numbers are found first
    /// and whatever precedes them is the anchor, rather than the other way round.
    fn parse(term: &str) -> Result<Self, LineSelectionError> {
        let refuse = |reason: &str| LineSelectionError::Window {
            term: term.to_string(),
            reason: reason.to_string(),
        };
        let commas = top_level_commas(term);
        if commas.len() < 4 {
            return Err(refuse(
                "it takes a displacement, a width and a height after the anchor",
            ));
        }
        let cut = commas[commas.len() - 4];
        let numbers: Vec<&str> = term[cut + 1..].split(',').map(str::trim).collect();
        if numbers.len() != 4 {
            // Reachable only through a comma that `top_level_commas` passed over and `split` did
            // not — one inside quotes, among the numbers.
            return Err(refuse("one of the four numbers carries a comma"));
        }
        let mut read = [0.0f32; 4];
        for (slot, text) in read.iter_mut().zip(&numbers) {
            *slot = text
                .parse::<f32>()
                .map_err(|_| refuse(&format!("{text} is not a number")))?;
        }
        let anchor = WindowAnchor::Compact(strip_angles(term[..cut].trim()).to_string());
        Ok(Self {
            anchor,
            displacement: (read[0], read[1]),
            width: read[2],
            height: read[3],
        })
    }

    /// The window as the leaf [`RelativeSelectPdfLineSet`] it is.
    ///
    /// The anchor always goes through [`relative_expression`], even when every criterion in it is
    /// absolute, because that is where an anchor stating nothing is refused — and an anchor is
    /// precisely the place where *every line* must not be allowed to pass for an answer.
    fn leaf(&self) -> Result<RelativeSelectPdfLineSet, LineSelectionError> {
        let anchor = self.anchor.selection()?;
        Ok(RelativeSelectPdfLineSet::area_from_movewindow(
            anchor,
            self.displacement,
            self.width,
            self.height,
        ))
    }
}

/// What the positional pattern read out of one operand: the four criteria, none resolved yet.
#[derive(Debug, Clone, PartialEq, Default)]
struct SelectionParts {
    font: Option<FontCriterion>,
    /// The font taken from another line. Only the structured form can state it: the compact
    /// grammar has no spelling for it, so this is always `None` on that path.
    font_of: Option<Box<InputPdfLineSet>>,
    font_size: Option<FontSizeCriterion>,
    text: Option<String>,
    sides: Option<(BoundSpec, BoundSpec, BoundSpec, BoundSpec)>,
}

impl SelectionParts {
    /// Runs the pattern. It matches any string, so this never fails: a cell the grammar does not
    /// recognise comes back with every criterion absent, which is what
    /// [`is_pdfline_selection`] exists to catch.
    fn parse(input: &str) -> Self {
        let captures = LINE_SET_REGEXP
            .captures(input)
            .expect("every group in LINE_SET_REGEXP is optional, so it matches any string, including the empty one");

        // `|` separates alternatives; one name with no separator stays `Single`, so the common case
        // builds exactly what it built before.
        let font = captures.at(1).map(|f| {
            let names: Vec<String> = f.split('|').map(|n| n.trim().to_string()).collect();
            match names.as_slice() {
                [single] => FontCriterion::Single(single.clone()),
                _ => FontCriterion::Multiple(names),
            }
        });
        let number = |s: &str| {
            s.parse::<f32>()
                .expect("digits matched by LINE_SET_REGEXP always parse as f32")
        };
        let font_size = captures.at(2).map(|s| match s.split_once(':') {
            None => FontSizeCriterion::Exact(number(s)),
            Some((low, high)) => FontSizeCriterion::Interval(number(low), number(high)),
        });

        // The four sides as the cell wrote them, before deciding which construction path they need.
        let sides = if let Some(y_range) = captures.at(3) {
            // The vertical range is the whole `"(a:b)"` string, brackets included.
            let (y_min, y_max) = parse_bound_pair_spec(&y_range[1..y_range.len() - 1]);
            Some((BoundSpec::Absent, BoundSpec::Absent, y_min, y_max))
        } else if let Some(area_text) = captures.at(4) {
            // The area text is `"(a:b)(c:d)"` without the outer wrapping brackets. Splitting on
            // `")("` separates the two pairs — and it is safe although a bound may now carry a
            // bracket, because a bracket inside an anchor is inside quotes and `")("` is not.
            let (x_part, y_part) = split_area_pairs(area_text);
            let (x_min, x_max) = parse_bound_pair_spec(x_part);
            let (y_min, y_max) = parse_bound_pair_spec(y_part);
            Some((x_min, x_max, y_min, y_max))
        } else {
            None
        };

        let text = captures.at(5).map(|t| t.to_string());

        Self {
            font,
            font_of: None,
            font_size,
            text,
            sides,
        }
    }

    fn is_empty(&self) -> bool {
        self.font.is_none()
            && self.font_of.is_none()
            && self.font_size.is_none()
            && self.text.is_none()
            && self.sides.is_none()
    }

    /// Whether any side of the area is an anchor, which is what makes the selection relative.
    fn has_anchor(&self) -> bool {
        self.sides
            .as_ref()
            .is_some_and(|(x_min, x_max, y_min, y_max)| {
                [x_min, x_max, y_min, y_max]
                    .iter()
                    .any(|side| side.is_anchor())
            })
    }
}

/// One operand of the grammar: what the pattern read, plus the window terms taken out before it
/// ran.
struct Operand {
    /// The operand with every window term removed — the string the pattern saw, kept because it is
    /// also the string [`is_pdfline_selection`] has to validate.
    plain: String,
    parts: SelectionParts,
    windows: Vec<WindowSpec>,
}

impl Operand {
    fn parse(input: &str) -> Result<Self, LineSelectionError> {
        let (plain, windows) = extract_windows(input)?;
        let parts = SelectionParts::parse(&plain);
        Ok(Self {
            plain,
            parts,
            windows,
        })
    }

    /// Whether the original, absolute construction path can build it: no window, no anchor.
    fn is_absolute(&self) -> bool {
        self.windows.is_empty() && !self.parts.has_anchor()
    }

    /// Whether the operand is, in its entirety, written in the grammar. See
    /// [`is_pdfline_selection`], whose question this answers for one operand.
    fn is_well_formed(&self) -> bool {
        LINE_SET_ANCHORED_REGEXP.find(&self.plain).is_some()
            && self
                .windows
                .iter()
                .all(|window| window.anchor.is_well_formed())
    }

    /// The structured form, for the absolute path.
    fn into_dict(self) -> InputPdfLineSet {
        let Self { parts, .. } = self;
        let area = parts
            .sides
            .map(|(x_min, x_max, y_min, y_max)| InputAreaSpec {
                x_min: x_min.number().map(InputBound::Number),
                x_max: x_max.number().map(InputBound::Number),
                y_min: y_min.number().map(InputBound::Number),
                y_max: y_max.number().map(InputBound::Number),
            });
        InputPdfLineSet {
            text: parts.text,
            font: parts.font,
            font_size: parts.font_size,
            area,
            window: None,
            font_of: None,
            all_of: Vec::new(),
            any_of: Vec::new(),
            without: Vec::new(),
        }
    }

    /// The relative construction path: the operand as a tree of leaves, intersected.
    ///
    /// It exists because the whole of the rest of the grammar builds *absolute* selections —
    /// [`pdfline_selection_from_dict`] returns `OptionallyRelative::Absolute`, and nothing
    /// downstream can tell an absolute selection from a resolved relative one. An anchor cannot be
    /// resolved here: it names a line of a page this function never sees, so the selection has to
    /// stay relative until a page contextualises it. That is one construction path more, not one
    /// regex group more, and it is the reason text anchors did not arrive with the font union and
    /// the size interval.
    ///
    /// `input` is carried only to be quoted in the one refusal this path can raise.
    fn node(self, input: &str) -> Result<RelativePdfLineSet, LineSelectionError> {
        let Self { parts, windows, .. } = self;
        selection_node(parts, windows, || input.to_string())
    }
}

/// The relative construction path, shared by the two spellings.
///
/// `description` is called only to quote the offending configuration in the one refusal this path
/// can raise, which is why it is a closure: rendering a structured spec costs an allocation, and
/// the overwhelming majority of selections are well formed.
fn selection_node(
    parts: SelectionParts,
    windows: Vec<WindowSpec>,
    description: impl FnOnce() -> String,
) -> Result<RelativePdfLineSet, LineSelectionError> {
    {
        if windows.is_empty() && parts.is_empty() {
            return Err(LineSelectionError::NoCriterion(description()));
        }
        let SelectionParts {
            font,
            font_of,
            font_size,
            text,
            sides,
        } = parts;

        let mut node = None;
        if let Some((x_min, x_max, y_min, y_max)) = &sides {
            // The numeric sides go through the same validation as a wholly numeric cell: a
            // non-positive coordinate is refused, and so is a pair written back to front.
            // `InputArea` is handed `None` for an anchored side, which is the honest answer — a
            // side whose value this function cannot know is a side it cannot compare.
            InputArea::build(
                x_min.number(),
                x_max.number(),
                y_min.number(),
                y_max.number(),
            )?;
            let area = RelativeSelectPdfLineSet::area_from_bounds(
                x_min.to_bound(0.0)?,
                y_min.to_bound(0.0)?,
                x_max.to_bound(1e6)?,
                y_max.to_bound(1e6)?,
            );
            node = intersect_leaf(node, OptionallyRelative::Relative(area));
        }
        for window in &windows {
            node = intersect_leaf(node, OptionallyRelative::Relative(window.leaf()?));
        }

        // Only the criteria the cell states are added — an absent one would mean "every line", and
        // intersecting with that is noise. The order is the one the absolute path intersects in.
        if let Some(criterion) = text {
            let leaf = SelectPdfLineSet::Text(TextSet::new(&criterion));
            node = intersect_leaf(node, OptionallyRelative::Absolute(leaf));
        }
        if let Some(criterion) = font_size {
            let leaf = SelectPdfLineSet::FontSize(font_size_interval(criterion)?);
            node = intersect_leaf(node, OptionallyRelative::Absolute(leaf));
        }
        if let Some(criterion) = font {
            let leaf = SelectPdfLineSet::Font(font_set(&criterion)?);
            node = intersect_leaf(node, OptionallyRelative::Absolute(leaf));
        }
        // Through `anchor_from_dict`, like every other anchor: a `font_of` whose spec states
        // nothing would take the font of whichever line comes first on the page.
        if let Some(spec) = font_of {
            let leaf = RelativeSelectPdfLineSet::select_font_of(anchor_from_dict(&spec)?);
            node = intersect_leaf(node, OptionallyRelative::Relative(leaf));
        }
        Ok(node.expect("states_a_criterion guarantees at least one leaf"))
    }
}

/// Intersects one more leaf into a tree that may not have been started yet.
///
/// One leaf per criterion, which is what the tree takes: a whole `PdfLineSet` is itself a tree and
/// cannot be a leaf.
fn intersect_leaf(
    node: Option<RelativePdfLineSet>,
    leaf: OptionallyRelative<SelectPdfLineSet, RelativeSelectPdfLineSet>,
) -> Option<RelativePdfLineSet> {
    let leaf = RelativePdfLineSet::from_leaf(leaf);
    Some(match node {
        None => leaf,
        Some(node) => leaf & node,
    })
}

/// Takes every `window(..)` term out of an operand, returning the rest of it and the terms.
///
/// The terms come out *before* the positional pattern runs, rather than going into it as a third
/// shape of area. The pattern is one line long and brittle — each of its three earlier additions
/// cost a correction, and the last one cost a panic on a real format — while a window carries a
/// quoted anchor, four numbers and a bracket pair of its own. Out here it is found by a scan that
/// already knows what a quote and a bracket are, and the pattern goes on reading exactly what it
/// read before.
///
/// The keyword is only a keyword at top level: inside quotes it is part of a text, and inside
/// `<…>` it belongs to the nested expression, which will find it when its own turn comes.
fn extract_windows(input: &str) -> Result<(String, Vec<WindowSpec>), LineSelectionError> {
    let mut pieces = Vec::new();
    let mut windows = Vec::new();
    let mut remainder = input;
    while let Some(start) = find_window(remainder) {
        let open = start + WINDOW_KEYWORD.len() - 1;
        let close =
            matching_bracket(remainder, open).ok_or_else(|| LineSelectionError::Window {
                term: remainder[open + 1..].to_string(),
                reason: "its bracket never closes".to_string(),
            })?;
        pieces.push(remainder[..start].trim());
        windows.push(WindowSpec::parse(&remainder[open + 1..close])?);
        remainder = &remainder[close + 1..];
    }
    pieces.push(remainder.trim());

    // Rejoined with one space: the pattern allows one between its groups and not two, and a term
    // removed from the middle of an operand leaves a gap. Trimming each piece can only touch
    // whitespace that was outside quotes, because that is where the term was cut out.
    let plain = pieces
        .into_iter()
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Ok((plain, windows))
}

/// The offset of the next window keyword at top level, if there is one.
///
/// A font name is made of word characters, so `window` has to be a word of its own: a cell naming
/// a font that ends in those six letters is not opening a term.
fn find_window(text: &str) -> Option<usize> {
    let mut nesting = Nesting::default();
    let mut previous = None;
    for (index, character) in text.char_indices() {
        let top = nesting.step(character);
        let standalone = !previous.is_some_and(|c: char| c.is_alphanumeric() || c == '_');
        if top && standalone && text[index..].starts_with(WINDOW_KEYWORD) {
            return Some(index);
        }
        previous = Some(character);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::pdf_extract::relative::{OptionallyRelative, RelativeInfo};

    fn line(font: &str, size: f32, text: &str, bbox: (f32, f32, f32, f32)) -> PdfLine {
        PdfLine::new(font, size, text, bbox)
    }

    /// A [`PdfLineSelection`] does not derive `Debug`, which `unwrap_err` would require for its
    /// panic message on the `Ok` branch, so the error is taken with an explicit match instead.
    fn expect_err(result: Result<PdfLineSelection, LineSelectionError>) -> LineSelectionError {
        match result {
            Ok(_) => panic!("expected a LineSelectionError, got Ok(..)"),
            Err(e) => e,
        }
    }

    /// Both constructors always produce an absolute selection; this helper assumes so and fails
    /// loudly if that ever stops being true, rather than comparing selections directly.
    fn selects(selection: &PdfLineSelection, probe: &PdfLine) -> bool {
        match selection {
            OptionallyRelative::Absolute(set) => set.contains(probe),
            OptionallyRelative::Relative(_) => {
                panic!(
                    "pdfline_selection_from_dict/_from_str must never build a Relative selection"
                )
            }
        }
    }

    mod pdfline_selection_from_dict_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn with_every_field_absent_it_accepts_any_line() {
            let selection = pdfline_selection_from_dict(&InputPdfLineSet::default()).unwrap();
            assert!(selects(
                &selection,
                &line("Arial", 12.0, "anything", (0.0, 0.0, 10.0, 10.0))
            ));
            assert!(selects(
                &selection,
                &line("Times New Roman", 999.0, "", (500.0, 500.0, 600.0, 600.0))
            ));
        }

        #[test]
        fn a_single_font_accepts_only_that_font() {
            let data = InputPdfLineSet {
                font: Some(FontCriterion::Single("Arial".to_string())),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(
                &selection,
                &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))
            ));
            assert!(!selects(
                &selection,
                &line("Times", 10.0, "x", (0.0, 0.0, 1.0, 1.0))
            ));
        }

        #[test]
        fn a_list_of_two_or_more_fonts_accepts_the_union() {
            let data = InputPdfLineSet {
                font: Some(FontCriterion::Multiple(vec![
                    "Arial".to_string(),
                    "Times".to_string(),
                ])),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(
                &selection,
                &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))
            ));
            assert!(selects(
                &selection,
                &line("Times", 10.0, "x", (0.0, 0.0, 1.0, 1.0))
            ));
            assert!(!selects(
                &selection,
                &line("Courier", 10.0, "x", (0.0, 0.0, 1.0, 1.0))
            ));
        }

        #[test]
        fn font_size_selects_a_narrow_interval_around_the_given_value() {
            let data = InputPdfLineSet {
                font_size: Some(FontSizeCriterion::Exact(12.0)),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(
                selects(
                    &selection,
                    &line("Arial", 12.0005, "x", (0.0, 0.0, 1.0, 1.0))
                ),
                "just inside [11.999, 12.001]"
            );
            assert!(
                !selects(
                    &selection,
                    &line("Arial", 12.005, "x", (0.0, 0.0, 1.0, 1.0))
                ),
                "clearly outside [11.999, 12.001]"
            );
        }

        #[test]
        fn area_with_partial_bounds_defaults_missing_bounds_to_zero_and_one_million() {
            // The absent bounds are replaced by the defaults.
            let data = InputPdfLineSet {
                area: Some(InputAreaSpec {
                    x_min: Some(InputBound::Number(5.0)),
                    x_max: None,
                    y_min: None,
                    y_max: Some(InputBound::Number(50.0)),
                }),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            // Inside (5.0, 0.0, 1e6, 50.0).
            assert!(selects(
                &selection,
                &line("Arial", 10.0, "x", (10.0, 10.0, 20.0, 20.0))
            ));
            // Outside: x0 below 5.0.
            assert!(!selects(
                &selection,
                &line("Arial", 10.0, "x", (1.0, 10.0, 4.0, 20.0))
            ));
            // Outside: y1 above 50.0.
            assert!(!selects(
                &selection,
                &line("Arial", 10.0, "x", (10.0, 10.0, 20.0, 60.0))
            ));
        }

        #[test]
        fn text_is_passed_through_to_the_text_set_unchanged() {
            let data = InputPdfLineSet {
                text: Some("^foo".to_string()),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(
                &selection,
                &line("Arial", 10.0, "foobar", (0.0, 0.0, 1.0, 1.0))
            ));
            assert!(!selects(
                &selection,
                &line("Arial", 10.0, "barfoo", (0.0, 0.0, 1.0, 1.0))
            ));
        }

        #[test]
        fn two_criteria_present_together_intersect_rather_than_union() {
            let data = InputPdfLineSet {
                font: Some(FontCriterion::Single("Arial".to_string())),
                font_size: Some(FontSizeCriterion::Exact(12.0)),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(
                selects(&selection, &line("Arial", 12.0, "x", (0.0, 0.0, 1.0, 1.0))),
                "matches both"
            );
            assert!(
                !selects(&selection, &line("Arial", 50.0, "x", (0.0, 0.0, 1.0, 1.0))),
                "font matches, size doesn't"
            );
            assert!(
                !selects(&selection, &line("Times", 12.0, "x", (0.0, 0.0, 1.0, 1.0))),
                "size matches, font doesn't"
            );
        }

        #[test]
        fn a_non_positive_font_size_is_rejected() {
            let zero = InputPdfLineSet {
                font_size: Some(FontSizeCriterion::Exact(0.0)),
                ..Default::default()
            };
            let err = expect_err(pdfline_selection_from_dict(&zero));
            let LineSelectionError::FontSizeNotPositive(v) = err else {
                panic!("expected FontSizeNotPositive, got {err:?}")
            };
            assert_eq!(v, 0.0);

            let negative = InputPdfLineSet {
                font_size: Some(FontSizeCriterion::Exact(-3.0)),
                ..Default::default()
            };
            let err = expect_err(pdfline_selection_from_dict(&negative));
            let LineSelectionError::FontSizeNotPositive(v) = err else {
                panic!("expected FontSizeNotPositive, got {err:?}")
            };
            assert_eq!(v, -3.0);
        }

        #[test]
        fn an_explicitly_empty_font_list_is_rejected() {
            let data = InputPdfLineSet {
                font: Some(FontCriterion::Multiple(vec![])),
                ..Default::default()
            };
            let err = expect_err(pdfline_selection_from_dict(&data));
            assert!(matches!(err, LineSelectionError::EmptyFontList));
        }

        #[test]
        fn an_invalid_area_bubbles_up_a_position_error() {
            let data = InputPdfLineSet {
                area: Some(InputAreaSpec {
                    x_min: Some(InputBound::Number(0.0)),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let err = expect_err(pdfline_selection_from_dict(&data));
            let LineSelectionError::Area(PositionError::XMinNotPositive(v)) = err else {
                panic!("expected Area(XMinNotPositive), got {err:?}")
            };
            assert_eq!(v, 0.0);
        }
    }

    mod pdfline_selection_from_str_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_bare_font_selects_only_that_font() {
            let selection = pdfline_selection_from_str("Arial").unwrap();
            assert!(selects(
                &selection,
                &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))
            ));
            assert!(!selects(
                &selection,
                &line("Times", 10.0, "x", (0.0, 0.0, 1.0, 1.0))
            ));
        }

        #[test]
        fn a_bracketed_number_alone_selects_that_font_size() {
            let selection = pdfline_selection_from_str("[12.0]").unwrap();
            assert!(selects(
                &selection,
                &line("Arial", 12.0005, "x", (0.0, 0.0, 1.0, 1.0))
            ));
            assert!(
                selects(
                    &selection,
                    &line("Times", 12.0005, "x", (0.0, 0.0, 1.0, 1.0))
                ),
                "font unrestricted"
            );
            assert!(!selects(
                &selection,
                &line("Arial", 12.005, "x", (0.0, 0.0, 1.0, 1.0))
            ));
        }

        /// The full-area grammar needs a pair of ranges wrapped in an *extra* pair of brackets, not
        /// a bare `(x0:x1)(y0:y1)`. Without the outer brackets the pattern matches only the first
        /// range, silently ignoring the rest — which is why this is pinned rather than assumed.
        #[test]
        fn a_double_parenthesized_range_pair_selects_the_full_area() {
            let selection = pdfline_selection_from_str("((1:10)(2:20))").unwrap();
            assert!(
                selects(&selection, &line("Arial", 10.0, "x", (3.0, 5.0, 6.0, 8.0))),
                "strictly inside (1,2,10,20)"
            );
            assert!(
                !selects(
                    &selection,
                    &line("Arial", 10.0, "x", (50.0, 5.0, 60.0, 8.0))
                ),
                "outside on x"
            );
        }

        #[test]
        fn a_single_parenthesized_range_selects_a_vertical_band() {
            let selection = pdfline_selection_from_str("(2:20)").unwrap();
            // The horizontal axis is unbounded and only the vertical one is constrained.
            assert!(selects(
                &selection,
                &line("Arial", 10.0, "x", (100.0, 5.0, 200.0, 10.0))
            ));
            assert!(!selects(
                &selection,
                &line("Arial", 10.0, "x", (100.0, 30.0, 200.0, 40.0))
            ));
        }

        #[test]
        fn a_quoted_string_alone_selects_that_text() {
            let selection = pdfline_selection_from_str("\"foo\"").unwrap();
            assert!(selects(
                &selection,
                &line("Arial", 10.0, "a foo b", (0.0, 0.0, 1.0, 1.0))
            ));
            assert!(!selects(
                &selection,
                &line("Arial", 10.0, "bar", (0.0, 0.0, 1.0, 1.0))
            ));
        }

        #[test]
        fn an_empty_string_selects_everything() {
            let selection = pdfline_selection_from_str("").unwrap();
            assert!(selects(
                &selection,
                &line("Whatever", 999.0, "anything", (500.0, 500.0, 600.0, 600.0))
            ));
        }

        #[test]
        fn groups_separated_by_spaces_combine_with_intersection() {
            let selection =
                pdfline_selection_from_str("Arial [12.0] ((1:10)(2:20)) \"foo\"").unwrap();
            assert!(
                selects(
                    &selection,
                    &line("Arial", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0))
                ),
                "matches all four criteria"
            );
            assert!(
                !selects(
                    &selection,
                    &line("Times", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0))
                ),
                "font alone fails"
            );
            assert!(
                !selects(
                    &selection,
                    &line("Arial", 12.0, "bar", (3.0, 5.0, 6.0, 8.0))
                ),
                "text alone fails"
            );
        }

        /// The font character class includes the comma, so a literal comma in a font name does not
        /// split the capture into two criteria; it stays one. Were it wrongly treated as a union of
        /// two fonts, a line in only the second one would pass, and it must not.
        #[test]
        fn a_font_containing_a_literal_comma_is_a_single_criterion_not_split_on_the_comma() {
            let selection = pdfline_selection_from_str("Arial,Bold").unwrap();
            assert!(selects(
                &selection,
                &line("Arial,Bold", 10.0, "x", (0.0, 0.0, 1.0, 1.0))
            ));
            assert!(
                !selects(&selection, &line("Bold", 10.0, "x", (0.0, 0.0, 1.0, 1.0))),
                "must not be treated as font Arial OR font Bold"
            );
            assert!(
                !selects(&selection, &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))),
                "must not be treated as font Arial OR font Bold"
            );
        }

        #[test]
        fn agrees_with_from_dict_on_an_equivalent_hand_built_criterion_set() {
            let via_str =
                pdfline_selection_from_str("Arial [12.0] ((1:10)(2:20)) \"foo\"").unwrap();
            let via_dict = pdfline_selection_from_dict(&InputPdfLineSet {
                font: Some(FontCriterion::Single("Arial".to_string())),
                font_size: Some(FontSizeCriterion::Exact(12.0)),
                area: Some(InputAreaSpec {
                    x_min: Some(InputBound::Number(1.0)),
                    x_max: Some(InputBound::Number(10.0)),
                    y_min: Some(InputBound::Number(2.0)),
                    y_max: Some(InputBound::Number(20.0)),
                }),
                text: Some("foo".to_string()),
                ..Default::default()
            })
            .unwrap();

            let probes = [
                line("Arial", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0)),
                line("Times", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0)),
                line("Arial", 50.0, "a foo b", (3.0, 5.0, 6.0, 8.0)),
                line("Arial", 12.0, "bar", (3.0, 5.0, 6.0, 8.0)),
                line("Arial", 12.0, "a foo b", (50.0, 50.0, 60.0, 60.0)),
            ];
            for probe in probes {
                assert_eq!(
                    selects(&via_str, &probe),
                    selects(&via_dict, &probe),
                    "mismatch for {probe:?}"
                );
            }
        }

        #[test]
        fn a_non_positive_font_size_error_propagates_from_from_dict() {
            let err = expect_err(pdfline_selection_from_str("[0.0]"));
            let LineSelectionError::FontSizeNotPositive(v) = err else {
                panic!("expected FontSizeNotPositive, got {err:?}")
            };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn an_invalid_area_error_propagates_from_from_dict() {
            let err = expect_err(pdfline_selection_from_str("((0:10)(2:20))"));
            let LineSelectionError::Area(PositionError::XMinNotPositive(v)) = err else {
                panic!("expected Area(XMinNotPositive), got {err:?}")
            };
            assert_eq!(v, 0.0);
        }
    }

    /// The font union, written with `|` — `R8` in a cell.
    ///
    /// Every example is measured: `Lato,Bold|TrebuchetMS-Bold|Open Sans,Bold` is the heading of
    /// ANIMA_SGR-IT24's holdings table, typeset in three families across the five documents of
    /// that one format.
    mod a_font_union {
        use super::*;

        fn probe(font: &str) -> PdfLine {
            line(font, 10.0, "x", (0.0, 0.0, 1.0, 1.0))
        }

        #[test]
        fn each_alternative_is_selected() {
            let selection =
                pdfline_selection_from_str("Lato,Bold|TrebuchetMS-Bold|Open Sans,Bold").unwrap();
            for font in ["Lato,Bold", "TrebuchetMS-Bold", "Open Sans,Bold"] {
                assert!(
                    selects(&selection, &probe(font)),
                    "{font} is one of the alternatives"
                );
            }
        }

        #[test]
        fn a_font_outside_the_union_is_not_selected() {
            let selection = pdfline_selection_from_str("Lato,Bold|TrebuchetMS-Bold").unwrap();
            assert!(!selects(&selection, &probe("Open Sans,Bold")));
        }

        /// The comma belongs to the name, the bar separates names. Getting this backwards would
        /// turn `TimesNewRoman,Bold` into two fonts nobody has.
        #[test]
        fn a_comma_stays_inside_one_name() {
            let selection = pdfline_selection_from_str("TimesNewRoman,Bold").unwrap();
            assert!(selects(&selection, &probe("TimesNewRoman,Bold")));
            assert!(!selects(&selection, &probe("TimesNewRoman")));
        }

        #[test]
        fn one_name_with_no_bar_parses_as_a_single_font() {
            let selection = pdfline_selection_from_str("Arial").unwrap();
            assert!(selects(&selection, &probe("Arial")));
            assert!(!selects(&selection, &probe("Arial,Bold")));
        }

        #[test]
        fn the_space_around_a_bar_is_not_part_of_the_name() {
            let selection = pdfline_selection_from_str("Lato,Bold | Open Sans,Bold").unwrap();
            assert!(selects(&selection, &probe("Open Sans,Bold")));
        }

        #[test]
        fn a_union_combines_with_the_other_criteria() {
            let selection =
                pdfline_selection_from_str(r#"Lato,Bold|Open Sans,Bold [10] "Titoli""#).unwrap();
            assert!(selects(
                &selection,
                &line("Open Sans,Bold", 10.0, "Titoli", (0.0, 0.0, 1.0, 1.0))
            ));
            assert!(
                !selects(
                    &selection,
                    &line("Open Sans,Bold", 12.0, "Titoli", (0.0, 0.0, 1.0, 1.0))
                ),
                "the size still restricts"
            );
            assert!(
                !selects(
                    &selection,
                    &line("Lato,Bold", 10.0, "Divisa", (0.0, 0.0, 1.0, 1.0))
                ),
                "the text still restricts"
            );
        }
    }

    /// The font-size **interval**, `[low:high]`, beside the exact `[size]` the grammar already had.
    ///
    /// The measured case is ANIMA_SGR-IT24's `INDICE` heading: `18` to `21`, because the same word
    /// is set at three sizes in three families, and nothing else on the page goes above `11.04`.
    mod a_font_size_interval {
        use super::*;
        use pretty_assertions::assert_eq;

        fn probe(size: f32) -> PdfLine {
            line("Arial", size, "x", (0.0, 0.0, 1.0, 1.0))
        }

        #[test]
        fn a_size_inside_the_interval_is_selected() {
            let selection = pdfline_selection_from_str("[18:21]").unwrap();
            for size in [18.96, 19.5, 20.04] {
                assert!(
                    selects(&selection, &probe(size)),
                    "{size} is inside [18:21]"
                );
            }
        }

        #[test]
        fn a_size_outside_the_interval_is_not() {
            let selection = pdfline_selection_from_str("[18:21]").unwrap();
            assert!(!selects(&selection, &probe(11.04)));
            assert!(!selects(&selection, &probe(24.0)));
        }

        /// The one thing this addition must not do: change what a cell written before it meant.
        #[test]
        fn a_single_size_still_means_one_size_widened_by_the_precision() {
            let selection = pdfline_selection_from_str("[12.0]").unwrap();
            assert!(selects(&selection, &probe(12.0005)));
            assert!(!selects(&selection, &probe(12.005)));
        }

        #[test]
        fn an_interval_combines_with_a_font_and_a_text() {
            let selection = pdfline_selection_from_str(
                r#"timesnewromanbolditalic[9.5:10.5] "prestazione degli indicatori""#,
            )
            .unwrap();
            assert!(selects(
                &selection,
                &line(
                    "timesnewromanbolditalic",
                    10.0,
                    "la prestazione degli indicatori",
                    (0.0, 0.0, 1.0, 1.0)
                )
            ));
            assert!(!selects(
                &selection,
                &line(
                    "timesnewromanbolditalic",
                    12.0,
                    "la prestazione degli indicatori",
                    (0.0, 0.0, 1.0, 1.0)
                )
            ));
        }

        #[test]
        fn an_inverted_interval_is_refused_rather_than_swapped() {
            let err = expect_err(pdfline_selection_from_str("[21:18]"));
            let LineSelectionError::FontSizeIntervalInverted(low, high) = err else {
                panic!("expected FontSizeIntervalInverted, got {err:?}")
            };
            assert_eq!((low, high), (21.0, 18.0));
        }

        #[test]
        fn an_interval_starting_at_zero_is_refused() {
            let err = expect_err(pdfline_selection_from_str("[0:21]"));
            let LineSelectionError::FontSizeNotPositive(v) = err else {
                panic!("expected FontSizeNotPositive, got {err:?}")
            };
            assert_eq!(v, 0.0);
        }

        /// An interval of zero width is inverted by the `high <= low` test, and that is the
        /// intended answer: whoever means one size writes `[size]`, which widens by the precision.
        #[test]
        fn an_interval_of_zero_width_is_refused_and_the_exact_form_is_the_answer() {
            assert!(pdfline_selection_from_str("[12:12]").is_err());
            assert!(pdfline_selection_from_str("[12]").is_ok());
        }

        #[test]
        fn both_shapes_are_recognised_as_selections_by_the_validator() {
            assert!(is_pdfline_selection("[18:21]"));
            assert!(is_pdfline_selection("[12]"));
            assert!(is_pdfline_selection("Lato,Bold|Open Sans,Bold"));
        }
    }

    mod line_selection_error_display {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn font_size_not_positive_displays_the_offending_value() {
            assert_eq!(
                LineSelectionError::FontSizeNotPositive(-2.5).to_string(),
                "font_size must be positive, found -2.5"
            );
        }

        #[test]
        fn empty_font_list_displays_a_fixed_message() {
            assert_eq!(
                LineSelectionError::EmptyFontList.to_string(),
                "font list must not be empty when provided"
            );
        }

        #[test]
        fn area_displays_transparently_as_the_wrapped_position_error() {
            let inner = PositionError::XMinNotPositive(0.0);
            let expected = inner.to_string();
            assert_eq!(LineSelectionError::Area(inner).to_string(), expected);
        }

        #[test]
        fn degenerate_area_names_the_geometry_defect_it_wraps() {
            let inner = Area::build(10.0, 0.0, 5.0, 10.0).expect_err("inverted on x");
            assert_eq!(
                LineSelectionError::DegenerateArea(inner).to_string(),
                format!("the area of this selection is not a rectangle: {inner}")
            );
        }
    }

    /// `InputArea::build` checks each side it is *given*; the sides it is not given are filled in
    /// here with `0.0` and `1e6`, and those are compared against nothing. A selection spec can
    /// therefore pass validation and still describe no rectangle — which used to abort the run
    /// rather than reporting a defect in the formats repository.
    mod area_beyond_what_input_area_validates {
        use super::*;
        use pretty_assertions::assert_eq;

        fn spec(
            x_min: Option<f32>,
            x_max: Option<f32>,
            y_min: Option<f32>,
            y_max: Option<f32>,
        ) -> InputPdfLineSet {
            let side = |value: Option<f32>| value.map(InputBound::Number);
            InputPdfLineSet {
                area: Some(InputAreaSpec {
                    x_min: side(x_min),
                    x_max: side(x_max),
                    y_min: side(y_min),
                    y_max: side(y_max),
                }),
                ..Default::default()
            }
        }

        /// The shape that gets through: `x_min` above the `1e6` that stands in for an absent
        /// `x_max`. Both of `InputArea`'s checks pass — the value is positive, and there is no
        /// `x_max` to compare it with.
        #[test]
        fn an_x_min_above_the_absent_x_max_fallback_is_a_degenerate_area_not_a_panic() {
            match expect_err(pdfline_selection_from_dict(&spec(
                Some(2e6),
                None,
                None,
                None,
            ))) {
                LineSelectionError::DegenerateArea(_) => {}
                other => panic!("expected DegenerateArea, found {other}"),
            }
        }

        #[test]
        fn the_same_on_the_vertical_axis() {
            match expect_err(pdfline_selection_from_dict(&spec(
                None,
                None,
                Some(2e6),
                None,
            ))) {
                LineSelectionError::DegenerateArea(_) => {}
                other => panic!("expected DegenerateArea, found {other}"),
            }
        }

        /// What `InputArea` does catch stays caught by `InputArea`, with its own message: this
        /// variant is the gap-filler, not a replacement.
        #[test]
        fn an_inverted_pair_both_sides_given_is_still_reported_by_input_area() {
            match expect_err(pdfline_selection_from_dict(&spec(
                Some(90.0),
                Some(10.0),
                None,
                None,
            ))) {
                LineSelectionError::Area(PositionError::XBoundsInverted { x_min, x_max }) => {
                    assert_eq!((x_min, x_max), (90.0, 10.0));
                }
                other => panic!("expected XBoundsInverted, found {other}"),
            }
        }

        /// And an ordinary spec still builds: the new fallible path must not have narrowed what a
        /// formats repository is allowed to write.
        #[test]
        fn an_area_with_only_a_lower_bound_still_builds() {
            let selection = pdfline_selection_from_dict(&spec(Some(10.0), None, None, None))
                .expect("a half-open area is legitimate");
            assert!(selects(
                &selection,
                &line("Arial", 12.0, "right of x_min", (20.0, 0.0, 30.0, 10.0))
            ));
            assert!(!selects(
                &selection,
                &line("Arial", 12.0, "left of x_min", (1.0, 0.0, 5.0, 10.0))
            ));
        }
    }

    /// The lines a cell selects on a page, with the relative part resolved against it.
    ///
    /// The `selects` helper above refuses a relative selection on purpose, so the three submodules
    /// about the relative forms need their own: contextualising is exactly what is under test
    /// there.
    fn selected<'a>(cell: &str, page: &'a [PdfLine]) -> Vec<&'a str> {
        let set = pdfline_selection_from_str(cell)
            .expect("a well-formed cell")
            .contextualize(page);
        page.iter()
            .filter(|l| set.contains(l))
            .map(|l| l.text().as_str())
            .collect()
    }

    /// Whether a cell built a relative selection, which is what an anchor, a window and a
    /// difference each produce and nothing else does.
    fn is_relative(cell: &str) -> bool {
        matches!(
            pdfline_selection_from_str(cell).expect("a well-formed cell"),
            OptionallyRelative::Relative(_)
        )
    }

    mod an_area_bounded_by_text_anchors {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Two columns of a statement, side by side, with a heading over each.
        fn two_columns() -> Vec<PdfLine> {
            vec![
                line("Arial", 8.0, "Condensed Schedule", (10.0, 10.0, 90.0, 20.0)),
                line("Arial", 8.0, "Domicile", (200.0, 10.0, 250.0, 20.0)),
                line(
                    "Arial",
                    8.0,
                    "inside the first column",
                    (95.0, 40.0, 180.0, 50.0),
                ),
                line(
                    "Arial",
                    8.0,
                    "left of the first heading",
                    (0.0, 40.0, 8.0, 50.0),
                ),
                line(
                    "Arial",
                    8.0,
                    "right of the second heading",
                    (260.0, 40.0, 300.0, 50.0),
                ),
            ]
        }

        #[test]
        fn a_numeric_cell_is_still_absolute() {
            // The guarantee the rest of the grammar rests on: nothing written before anchors
            // existed changes construction path.
            // `((1:100)(0:50))` and not `((0:…)…)`: `InputArea` refuses a non-positive
            // `x_min`, which is a rule of its own and older than this grammar.
            assert!(!is_relative(r#"Arial[8]((1:100)(1:50)) "x""#));
            assert!(!is_relative("Arial(1:100)"));
            assert!(!is_relative(""));
        }

        #[test]
        fn an_anchor_among_the_bounds_makes_the_cell_relative() {
            assert!(is_relative(
                r#"((  "Condensed Schedule"  :"Domicile")(1:800))"#
            ));
            assert!(is_relative(r#"("Condensed Schedule":)"#));
        }

        #[test]
        fn each_side_takes_its_anchors_inner_edge() {
            // Between the two headings: the left side takes the first heading's **right** edge and
            // the right side the second's **left** edge, so neither heading is inside and what
            // falls between them is.
            let page = two_columns();
            let got = selected(r#"(("Condensed Schedule":"Domicile")(1:800))"#, &page);
            assert_eq!(got, ["inside the first column"]);
        }

        #[test]
        fn the_vertical_range_form_takes_anchors_too() {
            let page = vec![
                line(
                    "Arial",
                    8.0,
                    "Statement of Net Assets",
                    (10.0, 10.0, 120.0, 20.0),
                ),
                line(
                    "Arial",
                    8.0,
                    "a row of the statement",
                    (10.0, 30.0, 120.0, 40.0),
                ),
                line(
                    "Arial",
                    8.0,
                    "Notes to the accounts",
                    (10.0, 60.0, 120.0, 70.0),
                ),
                line("Arial", 8.0, "a note", (10.0, 80.0, 120.0, 90.0)),
            ];
            let got = selected(
                r#"("Statement of Net Assets":"Notes to the accounts")"#,
                &page,
            );
            assert_eq!(got, ["a row of the statement"]);
        }

        #[test]
        fn a_colon_inside_an_anchor_does_not_split_the_pair() {
            // `Product name:` is how most labels are written, so this is the common case and not
            // an exotic one.
            let page = vec![
                line("Arial", 8.0, "Product name:", (10.0, 10.0, 80.0, 20.0)),
                line("Arial", 8.0, "THE FUND", (90.0, 10.0, 160.0, 20.0)),
                line("Arial", 8.0, "far left", (0.0, 10.0, 5.0, 20.0)),
            ];
            let got = selected(r#"(("Product name:":)(1:800))"#, &page);
            assert_eq!(got, ["THE FUND"]);
        }

        #[test]
        fn brackets_inside_an_anchor_do_not_split_the_two_pairs() {
            // `Regulation (EU) 2020/852` is one of the measured anchors: splitting on the first
            // `)(` found would cut it in half.
            let page = vec![
                line(
                    "Arial",
                    8.0,
                    "Regulation (EU) 2020/852",
                    (10.0, 10.0, 140.0, 20.0),
                ),
                line(
                    "Arial",
                    8.0,
                    "under the regulation line",
                    (10.0, 30.0, 140.0, 40.0),
                ),
                line("Arial", 8.0, "above it", (10.0, 0.0, 140.0, 5.0)),
            ];
            let got = selected(r#"((1:800)("Regulation (EU) 2020/852":))"#, &page);
            assert_eq!(got, ["under the regulation line"]);
        }

        #[test]
        fn the_other_criteria_are_intersected_with_the_area() {
            let page = vec![
                line("Arial", 8.0, "Condensed Schedule", (10.0, 10.0, 90.0, 20.0)),
                line(
                    "Arial",
                    8.0,
                    "in the area, right font",
                    (95.0, 40.0, 180.0, 50.0),
                ),
                line(
                    "Helvetica",
                    8.0,
                    "in the area, wrong font",
                    (95.0, 60.0, 180.0, 70.0),
                ),
                line("Arial", 8.0, "right font, outside", (0.0, 40.0, 8.0, 50.0)),
            ];
            let got = selected(r#"Arial[8](("Condensed Schedule":)(1:800))"#, &page);
            assert_eq!(got, ["in the area, right font"]);
        }

        #[test]
        fn a_trailing_text_is_read_as_the_text_criterion_and_not_as_a_bound() {
            let page = vec![
                line("Arial", 8.0, "Condensed Schedule", (10.0, 10.0, 90.0, 20.0)),
                line("Arial", 8.0, "Total assets", (95.0, 40.0, 180.0, 50.0)),
                line("Arial", 8.0, "something else", (95.0, 60.0, 180.0, 70.0)),
            ];
            let got = selected(r#"(("Condensed Schedule":)(1:800)) "^Total""#, &page);
            assert_eq!(got, ["Total assets"]);
        }

        #[test]
        fn an_anchor_no_line_satisfies_widens_the_side_instead_of_failing() {
            // The engine's own `Bounds` behaviour, stated here because a cell can now reach it: a
            // page that simply does not carry the anchor is a normal occurrence, and the side falls
            // back to the page edge rather than failing the page.
            let page = vec![line("Arial", 8.0, "only line", (10.0, 10.0, 90.0, 20.0))];
            let got = selected(r#"(("no such heading":)(1:800))"#, &page);
            assert_eq!(got, ["only line"]);
        }

        #[test]
        fn validation_accepts_the_new_form_in_its_entirety() {
            for cell in [
                r#"(("Condensed Schedule":"Domicile")(1:800))"#,
                r#"("Statement":"Notes")"#,
                r#"Arial[8](("Product name:":)(1:800)) "^Total""#,
                r#"((0:372.94)("Regulation (EU) 2020/852":"Did this financial product"))"#,
            ] {
                assert!(is_pdfline_selection(cell), "{cell}");
            }
        }

        #[test]
        fn validation_still_rejects_what_is_not_a_selection() {
            // The reason the anchored pattern exists: a bound must be digits or a quoted text, and
            // an unterminated quote is neither.
            assert!(!is_pdfline_selection(r#"(("unterminated:)(1:800))"#));
            assert!(!is_pdfline_selection(r#"(("a":"b")"#));
            // Prose made of word characters, spaces, commas and hyphens is **not** rejected, and
            // never was: those are exactly the characters a font name is made of, so the pattern
            // reads it as one. Stated here so the limit of the validation is on the record.
            assert!(is_pdfline_selection("this is prose, not a selection"));
        }

        #[test]
        fn an_anchor_may_be_a_whole_selection_so_it_can_carry_a_font() {
            // The case the measured statements need: `Notes` also occurs in the body of the page,
            // in another family, so the anchor that bounds the fund column is the **bold** one.
            let page = vec![
                line("helvetica-bold", 7.5, "Notes", (100.0, 10.0, 130.0, 20.0)),
                line(
                    "helvetica",
                    7.5,
                    "see the Notes to the accounts",
                    (10.0, 60.0, 200.0, 70.0),
                ),
                line(
                    "helvetica",
                    7.5,
                    "the fund's name",
                    (10.0, 30.0, 90.0, 40.0),
                ),
            ];
            let got = selected(r#"((:<helvetica-bold[7.5] "Notes">)(1:50))"#, &page);
            assert_eq!(got, ["the fund's name"]);
        }

        #[test]
        fn the_short_form_and_the_long_one_agree_when_the_text_is_enough() {
            let page = two_columns();
            let short = selected(r#"(("Condensed Schedule":"Domicile")(1:800))"#, &page);
            let long = selected(r#"((<"Condensed Schedule">:<"Domicile">)(1:800))"#, &page);
            assert_eq!(short, long);
        }

        #[test]
        fn a_nested_expression_may_carry_a_size_interval_whose_colon_is_not_a_separator() {
            // The case FINECO-EN23@LUX brought: `<arialnarrow-bold[8.9:9.1] "Notes">` holds a colon
            // that is inside no quotes, so splitting the pair on the first colon outside quotes cut
            // the expression in half and the half fell through to the numeric branch. A cell that
            // the pattern accepts is not yet a cell the parser reads.
            let page = vec![
                line("arialnarrow-bold", 9.0, "Notes", (100.0, 10.0, 130.0, 20.0)),
                line(
                    "arialnarrow",
                    9.0,
                    "see the Notes",
                    (10.0, 60.0, 90.0, 70.0),
                ),
                line(
                    "arialnarrow",
                    9.0,
                    "the fund's name",
                    (10.0, 30.0, 90.0, 40.0),
                ),
            ];
            let got = selected(r#"((:<arialnarrow-bold[8.9:9.1] "Notes">)(1:50))"#, &page);
            assert_eq!(got, ["the fund's name"]);
        }

        #[test]
        fn a_nested_expression_may_carry_an_area_whose_brackets_are_not_the_pair_separator() {
            let page = vec![
                line("Arial", 8.0, "heading", (100.0, 10.0, 140.0, 20.0)),
                line("Arial", 8.0, "wanted", (10.0, 30.0, 90.0, 40.0)),
            ];
            // The nested expression carries `(1:25)`, whose `)(` would otherwise look like the
            // separator between the two pairs of the outer area.
            let got = selected(r#"((:<Arial(1:25) "heading">)(1:50))"#, &page);
            assert_eq!(got, ["wanted"]);
        }

        #[test]
        fn a_nested_expression_that_does_not_parse_is_reported_and_not_swallowed() {
            // The inner expression goes through the same validation, so its refusal travels out.
            let cell = r#"((<Arial[0] "Notes">:)(1:800))"#;
            assert!(matches!(
                expect_err(pdfline_selection_from_str(cell)),
                LineSelectionError::FontSizeNotPositive(_)
            ));
        }

        #[test]
        fn validation_accepts_the_nested_form_too() {
            assert!(is_pdfline_selection(
                r#"((<helvetica-bold[7.4:7.6] "Notes">:1000000)(75:800))"#
            ));
            assert!(is_pdfline_selection(
                r#"((:<frutiger-black "STATEMENT OF NET ASSETS AS AT">)(1:800))"#
            ));
        }

        #[test]
        fn a_size_refused_on_the_absolute_path_is_refused_here_too() {
            // The two construction paths share their validators, so the refusal is the same.
            let cell = r#"Arial[0](("Total":)(1:800))"#;
            assert!(matches!(
                expect_err(pdfline_selection_from_str(cell)),
                LineSelectionError::FontSizeNotPositive(_)
            ));
        }
    }

    mod a_window_that_hangs_from_an_anchor {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The head of a statement of net assets, in the shape CARNE-EN23 writes it: the label
        /// `Assets`, the currency one line above it to the right, and the date on the label's own
        /// line.
        fn statement_head() -> Vec<PdfLine> {
            vec![
                line("Arial-Bold", 8.0, "Assets", (100.0, 50.0, 140.0, 60.0)),
                line("Arial-Bold", 8.0, "EUR", (160.0, 40.0, 190.0, 50.0)),
                line("Arial-Bold", 8.0, "31.12.2023", (160.0, 50.0, 220.0, 60.0)),
                line("Arial", 8.0, "left of the anchor", (10.0, 40.0, 50.0, 50.0)),
            ]
        }

        #[test]
        fn the_window_is_the_anchors_own_box_moved_and_scaled() {
            // The anchor measures 40 by 10, so `1.2` to the right of its left edge is `x=148` and
            // `-1.0` of its height up is `y=40`: the line above it, to its right, and nothing else.
            let page = statement_head();
            let got = selected(
                r#"window(<Arial-Bold "^Assets">, 1.2, -1.0, 100.0, 1.5)"#,
                &page,
            );
            assert_eq!(got, ["EUR"]);
        }

        #[test]
        fn the_four_numbers_are_a_displacement_and_a_scale_and_each_one_counts() {
            // The same anchor and the same width, the ordinate left where it is and the height a
            // single line: the date, which sits on the label's own row. It is the pair of cells
            // CARNE-EN23 needs, and the reason the form takes four numbers rather than two.
            let page = statement_head();
            let got = selected(
                r#"window(<Arial-Bold "^Assets">, 1.2, 0.0, 100.0, 1.0)"#,
                &page,
            );
            assert_eq!(got, ["31.12.2023"]);
        }

        #[test]
        fn a_window_makes_the_cell_relative() {
            assert!(is_relative(r#"window("^Assets", 1.2, -1.0, 100.0, 1.5)"#));
            assert!(!is_relative("Arial[8]"));
        }

        #[test]
        fn the_anchor_may_be_written_bare_or_in_angle_brackets() {
            // The brackets are redundant inside a term that already delimits its anchor, but an
            // author who has written a bound anchor writes them out of habit.
            let page = statement_head();
            let bare = selected(
                r#"window(Arial-Bold "^Assets", 1.2, -1.0, 100.0, 1.5)"#,
                &page,
            );
            let bracketed = selected(
                r#"window(<Arial-Bold "^Assets">, 1.2, -1.0, 100.0, 1.5)"#,
                &page,
            );
            assert_eq!(bare, bracketed);
            assert_eq!(bare, ["EUR"]);
        }

        #[test]
        fn the_other_criteria_are_intersected_with_the_window() {
            let page = vec![
                line("Arial-Bold", 8.0, "Assets", (100.0, 50.0, 140.0, 60.0)),
                line("Arial-Bold", 8.0, "EUR", (160.0, 40.0, 190.0, 50.0)),
                line(
                    "Helvetica",
                    8.0,
                    "another family, same band",
                    (300.0, 40.0, 340.0, 50.0),
                ),
            ];
            let got = selected(
                r#"Helvetica window(<Arial-Bold "^Assets">, 1.2, -1.0, 100.0, 1.5)"#,
                &page,
            );
            assert_eq!(got, ["another family, same band"]);
        }

        #[test]
        fn a_window_may_be_intersected_with_an_area_of_its_own() {
            let page = statement_head();
            // The band of the first test, cut to the ordinates of the second: nothing is left.
            let got = selected(
                r#"(52:800) window(<Arial-Bold "^Assets">, 1.2, -1.0, 100.0, 1.5)"#,
                &page,
            );
            assert!(got.is_empty(), "{got:?}");
        }

        #[test]
        fn an_anchor_may_carry_a_comma_in_its_font_name() {
            // Why the four numbers are taken from the **right**: a font name has a comma in it, and
            // the grammar's font union is a comma-bearing list, so the anchor cannot be whatever
            // precedes the first comma.
            let page = vec![
                line(
                    "TimesNewRoman,Bold",
                    8.0,
                    "Assets",
                    (100.0, 50.0, 140.0, 60.0),
                ),
                line("TimesNewRoman,Bold", 8.0, "EUR", (160.0, 40.0, 190.0, 50.0)),
            ];
            let got = selected(
                r#"window(TimesNewRoman,Bold "^Assets", 1.2, -1.0, 100.0, 1.5)"#,
                &page,
            );
            assert_eq!(got, ["EUR"]);
        }

        #[test]
        fn a_comma_inside_the_anchors_text_is_not_a_separator() {
            let page = vec![
                line(
                    "Arial-Bold",
                    8.0,
                    "Assets, total",
                    (100.0, 50.0, 140.0, 60.0),
                ),
                line("Arial-Bold", 8.0, "EUR", (160.0, 40.0, 190.0, 50.0)),
            ];
            let got = selected(r#"window("^Assets, total", 1.2, -1.0, 100.0, 1.5)"#, &page);
            assert_eq!(got, ["EUR"]);
        }

        #[test]
        fn the_keyword_is_only_a_keyword_outside_quotes() {
            // Inside the text criterion it is six letters and a bracket, and the cell stays
            // absolute because nothing in it is relative.
            let page = vec![line(
                "Arial",
                8.0,
                "has window(x) in it",
                (10.0, 10.0, 90.0, 20.0),
            )];
            // The text criterion is a substring with `^` and `$` for anchors, not a regex, so
            // the brackets need no escape.
            assert!(!is_relative(r#""window(x)""#));
            assert_eq!(selected(r#""window(x)""#, &page), ["has window(x) in it"]);
        }

        #[test]
        fn an_anchor_no_line_satisfies_selects_nothing() {
            // The opposite of what an unsatisfied **bound** does, and deliberately so: a bound that
            // cannot be resolved widens its side, while a window that has nothing to hang from has
            // no position at all, so it collapses. Both are the engine's own behaviour, stated here
            // because a cell can now reach them.
            let page = statement_head();
            let got = selected(r#"window("^No such label", 1.2, -1.0, 100.0, 1.5)"#, &page);
            assert!(got.is_empty(), "{got:?}");
        }

        #[test]
        fn fewer_than_four_numbers_is_refused() {
            let error = expect_err(pdfline_selection_from_str(r#"window("^Assets", 1.0, 2.0)"#));
            assert!(
                matches!(error, LineSelectionError::Window { .. }),
                "{error}"
            );
            assert!(error.to_string().contains("displacement"), "{error}");
        }

        #[test]
        fn a_number_that_is_not_a_number_is_refused_by_name() {
            let error = expect_err(pdfline_selection_from_str(
                r#"window("^Assets", 1.0, 2.0, wide, 4.0)"#,
            ));
            assert!(
                error.to_string().contains("wide is not a number"),
                "{error}"
            );
        }

        #[test]
        fn a_bracket_that_never_closes_is_refused() {
            let error = expect_err(pdfline_selection_from_str(
                r#"window("^Assets", 1, 2, 3, 4"#,
            ));
            assert!(error.to_string().contains("never closes"), "{error}");
        }

        #[test]
        fn an_anchor_that_states_no_criterion_is_refused() {
            // Left to the parser alone this would be the selection of every line, so the window
            // would hang off whichever line comes first on the page — a page of plausible nonsense
            // instead of an error. See `LineSelectionError::NoCriterion`.
            let error = expect_err(pdfline_selection_from_str("window(, 1, 2, 3, 4)"));
            assert!(
                matches!(error, LineSelectionError::NoCriterion(_)),
                "{error}"
            );
        }

        #[test]
        fn a_refusal_inside_the_anchor_travels_out() {
            let error = expect_err(pdfline_selection_from_str(
                r#"window(<Arial[0] "^Assets">, 1, 2, 3, 4)"#,
            ));
            assert!(
                matches!(error, LineSelectionError::FontSizeNotPositive(_)),
                "{error}"
            );
        }

        #[test]
        fn validation_accepts_the_window_form_and_rejects_a_malformed_one() {
            assert!(is_pdfline_selection(
                r#"window(<Arial-Bold "^Assets">, 1.2, -1.0, 100.0, 1.5)"#
            ));
            assert!(is_pdfline_selection(
                r#"ArialMT[6.95:6.97] window(<ArialMT[6.96] "^Annual report including">, 0.1, 0.8, 2.0, 1.4)"#
            ));
            assert!(!is_pdfline_selection(r#"window("^Assets", 1.0, 2.0)"#));
            assert!(!is_pdfline_selection(
                r#"window(<"unterminated>, 1, 2, 3, 4)"#
            ));
        }

        #[test]
        fn the_measured_shape_of_mediolanum_it24_a() {
            // `pdf_extract_funds` of that format, word for word: the fund's name is the band to the
            // right of the line that declares the currency the amounts are expressed in.
            let page = vec![
                line(
                    "Arial",
                    10.0,
                    "(valori espressi in Euro)",
                    (50.0, 100.0, 150.0, 110.0),
                ),
                line(
                    "Arial",
                    10.0,
                    "MEDIOLANUM FLESSIBILE SVILUPPO ITALIA",
                    (175.0, 99.0, 400.0, 109.0),
                ),
                line("Arial", 10.0, "far below", (175.0, 300.0, 400.0, 310.0)),
            ];
            let got = selected(
                r#"window("(valori espressi in", 1.2, -0.1, 100.0, 2.3)"#,
                &page,
            );
            assert_eq!(got, ["MEDIOLANUM FLESSIBILE SVILUPPO ITALIA"]);
        }
    }

    mod the_difference_of_two_expressions {
        use super::*;
        use pretty_assertions::assert_eq;

        fn two_rows() -> Vec<PdfLine> {
            vec![
                line("Arial", 8.0, "keep me", (10.0, 10.0, 90.0, 20.0)),
                line("Arial", 8.0, "drop me", (10.0, 30.0, 90.0, 40.0)),
            ]
        }

        #[test]
        fn the_second_expression_is_taken_out_of_the_first() {
            assert_eq!(selected(r#"Arial / "^drop""#, &two_rows()), ["keep me"]);
        }

        #[test]
        fn it_is_folded_left_to_right() {
            let page = vec![
                line("Arial", 8.0, "keep me", (10.0, 10.0, 90.0, 20.0)),
                line("Arial", 8.0, "drop me", (10.0, 30.0, 90.0, 40.0)),
                line("Arial", 8.0, "and me too", (10.0, 50.0, 90.0, 60.0)),
            ];
            assert_eq!(selected(r#"Arial / "^drop" / "^and""#, &page), ["keep me"]);
        }

        #[test]
        fn a_difference_makes_the_cell_relative() {
            assert!(is_relative(r#"Arial / "^drop""#));
        }

        #[test]
        fn a_slash_inside_quotes_is_not_the_operator() {
            // `Regulation (EU) 2020/852` is a measured anchor, and the shape that makes requiring
            // spaces around the operator worth it.
            let page = vec![line(
                "Arial",
                8.0,
                "Regulation (EU) 2020/852",
                (10.0, 10.0, 140.0, 20.0),
            )];
            assert!(!is_relative(r#""2020/852""#));
            assert_eq!(
                selected(r#""2020/852""#, &page),
                ["Regulation (EU) 2020/852"]
            );
        }

        #[test]
        fn a_slash_inside_a_window_term_is_not_the_operator_either() {
            // The scan counts brackets as well as quotes, so a slash inside a term belongs to the
            // term.
            let page = vec![
                line(
                    "Arial-Bold",
                    8.0,
                    "Assets/Liabilities",
                    (100.0, 50.0, 140.0, 60.0),
                ),
                line("Arial-Bold", 8.0, "EUR", (160.0, 40.0, 190.0, 50.0)),
            ];
            let got = selected(
                r#"window("^Assets/Liabilities", 1.2, -1.0, 100.0, 1.5)"#,
                &page,
            );
            assert_eq!(got, ["EUR"]);
        }

        #[test]
        fn an_operand_that_states_no_criterion_is_refused() {
            // A difference with nothing on one side would either subtract the whole page or keep
            // it, silently. See `LineSelectionError::NoCriterion`.
            for cell in ["Arial / ", " / Arial"] {
                let error = expect_err(pdfline_selection_from_str(cell));
                assert!(
                    matches!(error, LineSelectionError::NoCriterion(_)),
                    "{cell}: {error}"
                );
            }
        }

        #[test]
        fn a_refusal_inside_an_operand_travels_out() {
            let error = expect_err(pdfline_selection_from_str(r#"Arial[0] / "^drop""#));
            assert!(
                matches!(error, LineSelectionError::FontSizeNotPositive(_)),
                "{error}"
            );
        }

        #[test]
        fn validation_accepts_a_difference() {
            assert!(is_pdfline_selection(r#"Arial / "^drop""#));
            assert!(!is_pdfline_selection(r#"Arial / window("^Assets", 1, 2)"#));
        }

        #[test]
        fn the_measured_shape_of_fideuram_en23() {
            // `assets_fund_set` of that format: the head of the statement down to `ASSETS:`, less
            // the row that carries the currency — which is the last row of the head, and is found
            // by hanging a window off `ASSETS:` rather than by an ordinate, because the whole head
            // moves by eight points and a half from one page to the next.
            let page = vec![
                line("arial-bold", 7.0, "ASSETS:", (180.0, 200.0, 220.0, 208.0)),
                line(
                    "arial-bold",
                    6.5,
                    "FIDELITY USD BOND",
                    (185.0, 160.0, 260.0, 168.0),
                ),
                line("arial-bold", 6.5, "USD", (185.0, 181.0, 205.0, 189.0)),
                line(
                    "arial-bold",
                    6.5,
                    "below the label",
                    (185.0, 240.0, 260.0, 248.0),
                ),
            ];
            let head = r#"arial-bold[6.3:6.6]((180:)(110:<arial-bold[6.8:7.1] "^ASSETS:">))"#;
            let currency = r#"arial-bold[6.3:6.6] window(<arial-bold[6.8:7.1] "^ASSETS:">, 0.0, -2.5, 100.0, 1.6)"#;

            // The head holds both rows, and the window holds the currency alone.
            assert_eq!(selected(head, &page), ["FIDELITY USD BOND", "USD"]);
            assert_eq!(selected(currency, &page), ["USD"]);
            // The difference is what leaves the names.
            assert_eq!(
                selected(&format!("{head} / {currency}"), &page),
                ["FIDELITY USD BOND"]
            );
        }
    }

    /// The structured form, now that it states the whole grammar.
    ///
    /// The three forms under test here — an anchored side, a `window` and a `without` — arrived
    /// after the mapping did, which is why a formats repository used to write a selection needing
    /// one of them as a compact string even where every neighbouring selection was a mapping.
    ///
    /// Most of what follows is one assertion: **the two spellings select the same lines**. That is
    /// the property worth freezing, because the structured path exists to be an alternative
    /// spelling and not a second semantics — and it is cheap to state, since the compact form of
    /// each of these shapes is already measured against a page in the three modules above.
    mod the_structured_form_states_the_whole_grammar {
        use super::*;

        fn spec(yaml: &str) -> InputPdfLineSet {
            serde_yaml::from_str(yaml).expect("the test's own yaml")
        }

        /// The lines a structured spec selects on a page, resolved against it.
        fn selected_by<'a>(yaml: &str, page: &'a [PdfLine]) -> Vec<&'a str> {
            let set = pdfline_selection_from_dict(&spec(yaml))
                .expect("a well-formed spec")
                .contextualize(page);
            page.iter()
                .filter(|l| set.contains(l))
                .map(|l| l.text().as_str())
                .collect()
        }

        /// A page with two columns and a heading in each, which is enough to tell an anchored side
        /// from a numeric one and a difference from an intersection.
        fn two_columns() -> Vec<PdfLine> {
            vec![
                line(
                    "Arial-Bold",
                    8.0,
                    "Situazione a fine",
                    (300.0, 40.0, 380.0, 50.0),
                ),
                line(
                    "Arial",
                    8.0,
                    "left of the heading",
                    (10.0, 60.0, 90.0, 70.0),
                ),
                line("Arial", 8.0, "1,25", (120.0, 60.0, 160.0, 70.0)),
                line(
                    "Arial",
                    8.0,
                    "right of the heading",
                    (400.0, 60.0, 480.0, 70.0),
                ),
            ]
        }

        mod an_anchored_side {
            use super::*;
            use pretty_assertions::assert_eq;

            /// `{x_max: {text: ".."}}` is the mapping spelling of `((:<"..">)(:))`.
            #[test]
            fn a_side_may_be_a_nested_selection_instead_of_a_number() {
                let page = two_columns();
                let got = selected_by(
                    "font: Arial\narea:\n  x_max:\n    text: '^Situazione a fine'\n",
                    &page,
                );
                assert_eq!(got, ["left of the heading", "1,25"]);
            }

            #[test]
            fn it_agrees_with_the_compact_spelling() {
                let page = two_columns();
                assert_eq!(
                    selected_by(
                        "font: Arial\narea:\n  x_max:\n    text: '^Situazione a fine'\n",
                        &page
                    ),
                    selected(r#"Arial ((:<"^Situazione a fine">)(:))"#, &page),
                );
            }

            /// The anchor is a whole selection, so it carries its own font — which is the shape
            /// every measured one has, the anchoring word occurring in the body of the page too.
            #[test]
            fn the_anchor_carries_its_own_criteria() {
                let page = vec![
                    line("Arial-Bold", 8.0, "Notes", (300.0, 40.0, 340.0, 50.0)),
                    line("Arial", 8.0, "Notes", (10.0, 40.0, 50.0, 50.0)),
                    line("Arial", 8.0, "a value", (120.0, 60.0, 160.0, 70.0)),
                ];
                let got = selected_by(
                    "font: Arial\narea:\n  x_max:\n    font: Arial-Bold\n    text: '^Notes'\n",
                    &page,
                );
                assert_eq!(
                    got,
                    ["Notes", "a value"],
                    "the bold heading is the side, not the plain one"
                );
            }

            #[test]
            fn a_numeric_side_still_reads_as_a_number() {
                let page = two_columns();
                assert_eq!(
                    selected_by("font: Arial\narea:\n  x_max: 200\n", &page),
                    selected("Arial ((:200)(:))", &page),
                );
            }

            #[test]
            fn an_anchor_stating_nothing_is_refused_rather_than_taking_the_whole_page() {
                let error = expect_err(pdfline_selection_from_dict(&spec(
                    "font: Arial\narea:\n  x_max: {}\n",
                )));
                assert!(
                    matches!(error, LineSelectionError::NoCriterion(_)),
                    "{error:?}"
                );
            }
        }

        mod a_window {
            use super::*;
            use pretty_assertions::assert_eq;

            fn statement_head() -> Vec<PdfLine> {
                vec![
                    line("Arial-Bold", 8.0, "Assets", (100.0, 50.0, 140.0, 60.0)),
                    line("Arial-Bold", 8.0, "EUR", (160.0, 40.0, 190.0, 50.0)),
                    line("Arial", 8.0, "left of the anchor", (10.0, 40.0, 50.0, 50.0)),
                ]
            }

            const WINDOW: &str = "window:\n  anchor:\n    font: Arial-Bold\n    text: '^Assets'\n  dx: 1.2\n  dy: -1.0\n  width: 100.0\n  height: 1.5\n";

            #[test]
            fn the_four_numbers_and_the_anchor_read_as_they_do_in_the_grammar() {
                assert_eq!(selected_by(WINDOW, &statement_head()), ["EUR"]);
            }

            #[test]
            fn it_agrees_with_the_compact_spelling() {
                let page = statement_head();
                assert_eq!(
                    selected_by(WINDOW, &page),
                    selected(
                        r#"window(<Arial-Bold "^Assets">, 1.2, -1.0, 100.0, 1.5)"#,
                        &page
                    ),
                );
            }

            #[test]
            fn the_other_criteria_are_intersected_with_it() {
                let page = vec![
                    line("Arial-Bold", 8.0, "Assets", (100.0, 50.0, 140.0, 60.0)),
                    line("Arial-Bold", 8.0, "EUR", (160.0, 40.0, 190.0, 50.0)),
                    line(
                        "Helvetica",
                        8.0,
                        "another family, same band",
                        (300.0, 40.0, 340.0, 50.0),
                    ),
                ];
                let got = selected_by(&format!("font: Helvetica\n{WINDOW}"), &page);
                assert_eq!(got, ["another family, same band"]);
            }

            /// Four numbers, none of them optional: an expression written out is read by whoever
            /// maintains the format, and there a number that is not written cannot be read.
            #[test]
            fn none_of_the_four_numbers_may_be_left_out() {
                let error = serde_yaml::from_str::<InputPdfLineSet>(
                    "window:\n  anchor:\n    text: '^Assets'\n  dx: 1.2\n  dy: -1.0\n  width: 100.0\n",
                )
                .expect_err("a window with three numbers is not a window");
                assert!(error.to_string().contains("height"), "{error}");
            }

            #[test]
            fn an_anchor_stating_nothing_is_refused() {
                let error = expect_err(pdfline_selection_from_dict(&spec(
                    "window:\n  anchor: {}\n  dx: 1.0\n  dy: 0.0\n  width: 2.0\n  height: 1.0\n",
                )));
                assert!(
                    matches!(error, LineSelectionError::NoCriterion(_)),
                    "{error:?}"
                );
            }
        }

        /// The union, which has no compact spelling at all.
        ///
        /// It is the form `P10` needed and the reason it exists: a report paginated as a book
        /// prints the page number on the left of an even page and on the right of an odd one, so
        /// the number is in *either* of two areas. The compact grammar unites fonts and nothing
        /// else, and the mapping is where new expressiveness belongs now that it is the form to
        /// write.
        mod a_union {
            use super::*;
            use pretty_assertions::assert_eq;

            /// The two bands of a page numbered as a book, and a line in each.
            fn book_foot() -> Vec<PdfLine> {
                vec![
                    line("Arial", 7.0, "12", (40.0, 800.0, 55.0, 810.0)),
                    line("Arial", 7.0, "13", (540.0, 800.0, 555.0, 810.0)),
                    line("Arial", 7.0, "the fund name", (200.0, 800.0, 400.0, 810.0)),
                ]
            }

            #[test]
            fn any_of_takes_the_lines_of_either_operand() {
                let page = book_foot();
                let got = selected_by(
                    "any_of:\n  - area: {x_max: 100}\n  - area: {x_min: 500}\n",
                    &page,
                );
                assert_eq!(
                    got,
                    ["12", "13"],
                    "both margins, and not what sits between them"
                );
            }

            #[test]
            fn the_spec_s_own_criteria_are_intersected_with_the_union() {
                let page = vec![
                    line("Arial", 7.0, "12", (40.0, 800.0, 55.0, 810.0)),
                    line("Helvetica", 7.0, "13", (540.0, 800.0, 555.0, 810.0)),
                ];
                let got = selected_by(
                    "font: Arial\nany_of:\n  - area: {x_max: 100}\n  - area: {x_min: 500}\n",
                    &page,
                );
                assert_eq!(
                    got,
                    ["12"],
                    "the union narrows the area, the font narrows the family"
                );
            }

            #[test]
            fn a_union_of_one_is_that_one() {
                let page = book_foot();
                assert_eq!(
                    selected_by("any_of:\n  - area: {x_max: 100}\n", &page),
                    selected_by("area: {x_max: 100}\n", &page),
                );
            }

            #[test]
            fn it_makes_the_spec_relative() {
                let built = pdfline_selection_from_dict(&spec("any_of:\n  - text: '^A'\n"))
                    .expect("a well-formed spec");
                assert!(matches!(built, OptionallyRelative::Relative(_)));
            }

            #[test]
            fn an_operand_stating_nothing_is_refused_rather_than_uniting_the_whole_page() {
                let error = expect_err(pdfline_selection_from_dict(&spec(
                    "any_of:\n  - text: '^A'\n  - {}\n",
                )));
                assert!(
                    matches!(error, LineSelectionError::NoCriterion(_)),
                    "{error:?}"
                );
            }

            /// The union is inside the difference, which is what `(criteria & any_of) / without`
            /// means: the subtraction applies to the whole union and not to its first operand.
            #[test]
            fn what_is_subtracted_is_taken_off_the_whole_union() {
                let page = vec![
                    line("Arial", 7.0, "12", (40.0, 800.0, 55.0, 810.0)),
                    line("Arial", 7.0, "13", (540.0, 800.0, 555.0, 810.0)),
                ];
                let got = selected_by(
                    "any_of:\n  - area: {x_max: 100}\n  - area: {x_min: 500}\nwithout:\n  - text: '^13$'\n",
                    &page,
                );
                assert_eq!(got, ["12"]);
            }
        }

        /// The intersection, for the case the mapping's own keys cannot reach: **two criteria of
        /// the same kind**.
        mod an_intersection {
            use super::*;
            use pretty_assertions::assert_eq;

            /// FINECO-EN23@IR's table: bounded above by a heading **and** by an ordinate, which is
            /// two areas and therefore cannot be one `area` key.
            #[test]
            fn two_areas_of_the_same_spec_are_both_applied() {
                let page = vec![
                    line("Arial", 7.0, "a holding", (140.0, 300.0, 300.0, 310.0)),
                    line("Arial", 7.0, "SWAPS", (140.0, 600.0, 200.0, 610.0)),
                    line(
                        "Arial",
                        7.0,
                        "below the heading",
                        (140.0, 700.0, 300.0, 710.0),
                    ),
                    line("Arial", 7.0, "the page foot", (140.0, 800.0, 300.0, 810.0)),
                ];
                let got = selected_by(
                    "font: Arial\nall_of:\n  - area: {y_max: {text: '^SWAPS$'}}\n  - area: {y_max: 750}\n",
                    &page,
                );
                assert_eq!(
                    got,
                    ["a holding"],
                    "the heading closes it, and the ordinate keeps the foot out"
                );
            }

            /// With the heading absent the anchored side widens, and the ordinate is what is left:
            /// the reason the two are written together rather than one instead of the other.
            #[test]
            fn the_second_still_holds_when_the_first_cannot_resolve() {
                let page = vec![
                    line("Arial", 7.0, "a holding", (140.0, 300.0, 300.0, 310.0)),
                    line("Arial", 7.0, "the page foot", (140.0, 800.0, 300.0, 810.0)),
                ];
                let got = selected_by(
                    "font: Arial\nall_of:\n  - area: {y_max: {text: '^SWAPS$'}}\n  - area: {y_max: 750}\n",
                    &page,
                );
                assert_eq!(got, ["a holding"]);
            }

            #[test]
            fn it_makes_the_spec_relative() {
                let built = pdfline_selection_from_dict(&spec("all_of:\n  - text: '^A'\n"))
                    .expect("a well-formed spec");
                assert!(matches!(built, OptionallyRelative::Relative(_)));
            }

            #[test]
            fn an_operand_stating_nothing_is_refused() {
                let error = expect_err(pdfline_selection_from_dict(&spec(
                    "all_of:\n  - text: '^A'\n  - {}\n",
                )));
                assert!(
                    matches!(error, LineSelectionError::NoCriterion(_)),
                    "{error:?}"
                );
            }

            /// The three operations compose in one order: the keys and `all_of` and `any_of`
            /// intersect, and `without` is taken off the lot.
            #[test]
            fn the_three_operations_compose() {
                let page = vec![
                    line("Arial", 7.0, "12", (40.0, 800.0, 55.0, 810.0)),
                    line("Arial", 7.0, "13", (540.0, 800.0, 555.0, 810.0)),
                    line(
                        "Helvetica",
                        7.0,
                        "another family",
                        (40.0, 800.0, 55.0, 810.0),
                    ),
                ];
                let got = selected_by(
                    "all_of:\n  - font: Arial\nany_of:\n  - area: {x_max: 100}\n  - area: {x_min: 500}\nwithout:\n  - text: '^13$'\n",
                    &page,
                );
                assert_eq!(got, ["12"]);
            }
        }

        mod a_difference {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn without_subtracts_what_it_lists() {
                let page = two_columns();
                let got = selected_by("font: Arial\nwithout:\n  - text: ','\n", &page);
                assert_eq!(got, ["left of the heading", "right of the heading"]);
            }

            #[test]
            fn it_agrees_with_the_compact_spelling() {
                let page = two_columns();
                assert_eq!(
                    selected_by("font: Arial\nwithout:\n  - text: ','\n", &page),
                    selected("Arial / \",\"", &page),
                );
            }

            /// The list folds, as ` / ` does.
            #[test]
            fn several_operands_are_taken_out_in_turn() {
                let page = two_columns();
                let got = selected_by(
                    "font: Arial\nwithout:\n  - text: ','\n  - text: '^right'\n",
                    &page,
                );
                assert_eq!(got, ["left of the heading"]);
            }

            /// Which the compact grammar cannot write: its difference only ever folds left.
            #[test]
            fn an_operand_may_subtract_in_its_turn() {
                let page = two_columns();
                let got = selected_by(
                    "font: Arial\nwithout:\n  - text: 'of the heading'\n    without:\n      - text: '^left'\n",
                    &page,
                );
                assert_eq!(
                    got,
                    ["left of the heading", "1,25"],
                    "only the right-hand line is taken out"
                );
            }

            #[test]
            fn an_operand_stating_nothing_is_refused_rather_than_subtracting_the_whole_page() {
                let error = expect_err(pdfline_selection_from_dict(&spec(
                    "font: Arial\nwithout:\n  - {}\n",
                )));
                assert!(
                    matches!(error, LineSelectionError::NoCriterion(_)),
                    "{error:?}"
                );
            }

            /// `without` alone states no criterion of its own, which is the shape the compact
            /// grammar refuses as a first operand.
            #[test]
            fn subtracting_from_nothing_is_refused_too() {
                let error = expect_err(pdfline_selection_from_dict(&spec(
                    "without:\n  - text: ','\n",
                )));
                assert!(
                    matches!(error, LineSelectionError::NoCriterion(_)),
                    "{error:?}"
                );
            }
        }

        /// `font_of`: the family is not written down, it is read off another line of the page.
        mod a_font_taken_from_another_line {
            use super::*;
            use pretty_assertions::assert_eq;

            /// A page typeset in a family no selection names: the heading and the body share it,
            /// and a second family is there to be excluded.
            fn one_unnamed_family() -> Vec<PdfLine> {
                vec![
                    line(
                        "GillSans-Light",
                        9.0,
                        "Descripcion de la cartera",
                        (10.0, 40.0, 120.0, 50.0),
                    ),
                    line(
                        "GillSans-Light",
                        8.0,
                        "BOND 2029 4,5%",
                        (10.0, 60.0, 120.0, 70.0),
                    ),
                    line(
                        "Courier",
                        8.0,
                        "a note in another family",
                        (10.0, 80.0, 120.0, 90.0),
                    ),
                ]
            }

            #[test]
            fn the_font_is_the_one_of_the_line_the_anchor_picks() {
                let page = one_unnamed_family();
                let got = selected_by("font_of:\n  text: 'de la cartera'\n", &page);
                assert_eq!(got, ["Descripcion de la cartera", "BOND 2029 4,5%"]);
            }

            /// Intersected with the spec's own keys, like every other criterion: the anchor's own
            /// line is outside the area and the other family is outside the font.
            #[test]
            fn it_intersects_the_rest_of_the_spec() {
                let page = one_unnamed_family();
                let got = selected_by(
                    "area:\n  y_min: 55.0\nfont_of:\n  text: 'de la cartera'\n",
                    &page,
                );
                assert_eq!(got, ["BOND 2029 4,5%"]);
            }

            /// A page the anchor is not on resolves to the empty font set, so the selection is
            /// empty — not the whole page. A portfolio page without its heading is not a portfolio.
            #[test]
            fn an_anchor_the_page_does_not_carry_selects_nothing() {
                let page = one_unnamed_family();
                let got = selected_by("font_of:\n  text: '^no such heading$'\n", &page);
                assert_eq!(got, Vec::<&str>::new());
            }

            /// The same refusal every anchor gets: a spec stating nothing would take the font of
            /// whichever line comes first on the page.
            #[test]
            fn an_anchor_stating_nothing_is_refused() {
                let error = expect_err(pdfline_selection_from_dict(&spec("font_of: {}\n")));
                assert!(
                    matches!(error, LineSelectionError::NoCriterion(_)),
                    "{error:?}"
                );
            }

            /// `font_of` alone is a criterion, so a spec stating only it is not "every line".
            #[test]
            fn it_counts_as_a_criterion_of_its_own() {
                let got = spec("font_of:\n  text: 'x'\n");
                assert!(!got.is_absolute(), "a font read off the page is relative");
                assert!(got.states_a_criterion(), "font_of is a criterion");
            }

            /// It is in the refusal's list of stated keys, which is what its author looks for.
            #[test]
            fn a_refusal_names_the_key() {
                let described = describe(&spec("font_of:\n  text: 'x'\n"));
                assert!(described.contains("font_of"), "{described}");
            }
        }

        mod what_the_three_forms_do_not_change {
            use super::*;
            use pretty_assertions::assert_eq;

            /// An empty spec at the top level is the whole page, as it has always been. The
            /// refusal above belongs to anchors and operands, not to a selection.
            #[test]
            fn an_empty_spec_is_still_every_line() {
                let selection = pdfline_selection_from_dict(&InputPdfLineSet::default())
                    .expect("an empty spec is the whole page");
                assert!(selects(
                    &selection,
                    &line("Arial", 10.0, "anything", (1.0, 1.0, 2.0, 2.0))
                ));
            }

            /// A spec with none of the three stays **absolute**, which is what every spec written
            /// before them was: the fast path is still the path they take.
            #[test]
            fn a_spec_with_none_of_them_is_still_absolute() {
                let absolute =
                    pdfline_selection_from_dict(&spec("font: Arial\narea:\n  x_max: 200\n"))
                        .expect("a well-formed spec");
                assert!(matches!(absolute, OptionallyRelative::Absolute(_)));
            }

            #[test]
            fn each_of_the_three_makes_it_relative() {
                for yaml in [
                    "font: Arial\narea:\n  x_max:\n    text: '^A'\n",
                    "window:\n  anchor:\n    text: '^A'\n  dx: 1.0\n  dy: 0.0\n  width: 2.0\n  height: 1.0\n",
                    "font: Arial\nwithout:\n  - text: ','\n",
                ] {
                    let built =
                        pdfline_selection_from_dict(&spec(yaml)).expect("a well-formed spec");
                    assert!(matches!(built, OptionallyRelative::Relative(_)), "{yaml}");
                }
            }

            /// The numeric sides go through the same validation they always did, anchors or no
            /// anchors: a side written `0` is refused here as it is in a cell.
            #[test]
            fn a_numeric_side_is_validated_beside_an_anchored_one() {
                let error = expect_err(pdfline_selection_from_dict(&spec(
                    "area:\n  x_min: 0\n  x_max:\n    text: '^A'\n",
                )));
                assert!(
                    matches!(
                        error,
                        LineSelectionError::Area(PositionError::XMinNotPositive(_))
                    ),
                    "{error:?}"
                );
            }

            /// A refusal names the keys the spec states, so that its author can find it. The
            /// derived `Debug` would name the five it does not state as well.
            #[test]
            fn a_refusal_quotes_the_keys_the_spec_states() {
                let error = expect_err(pdfline_selection_from_dict(&spec(
                    "font: Arial\nwithout:\n  - {}\n",
                )));
                assert_eq!(
                    error.to_string(),
                    "this expression states no criterion, so it would select every line: a selection spec stating nothing"
                );
            }
        }
    }
}
