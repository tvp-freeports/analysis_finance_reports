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

// Used only by the tests below, which call `contains` in a helper; the `cfg(test)` avoids an
// unused-import warning in the normal build.
#[cfg(test)]
use crate::commons::sets::Container;
use crate::formats_utils::pdf_extract::position::{InputArea, PositionError};
use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;
use crate::formats_utils::pdf_extract::select::pdf_line::area::Area;
use crate::formats_utils::pdf_extract::select::pdf_line::font::FontSet;
use crate::formats_utils::pdf_extract::select::pdf_line::font_size::FontSizeInterval;
use crate::formats_utils::pdf_extract::select::pdf_line::text::TextSet;
use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;

/// A single font or a list of them. Untagged for serde, so a scalar becomes `Single` and a sequence
/// `Multiple`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(untagged)]
pub enum FontCriterion {
    Single(String),
    Multiple(Vec<String>),
}

/// The deserialization-side mirror of `InputArea`: four optional bounds, not yet validated.
/// `InputArea::build` stays the single point of validation.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Deserialize)]
pub struct InputAreaSpec {
    #[serde(default)]
    pub x_min: Option<f32>,
    #[serde(default)]
    pub x_max: Option<f32>,
    #[serde(default)]
    pub y_min: Option<f32>,
    #[serde(default)]
    pub y_max: Option<f32>,
}

/// The structured form of a line selection: four optional criteria, intersected.
#[derive(Debug, Clone, PartialEq, Default, serde::Deserialize)]
pub struct InputPdfLineSet {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub font: Option<FontCriterion>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub area: Option<InputAreaSpec>,
}

#[derive(Debug, thiserror::Error)]
pub enum LineSelectionError {
    #[error("font_size must be positive, found {0}")]
    FontSizeNotPositive(f32),
    /// `font` is present but empty. Reducing an empty list of alternatives has no meaningful
    /// answer, so it is a typed error rather than a panic.
    #[error("font list must not be empty when provided")]
    EmptyFontList,
    #[error(transparent)]
    Area(#[from] PositionError),
}

/// The precision of the font-size interval built around an exact size.
const FONT_SIZE_PRECISION: f32 = 1e-3;

/// Builds an always-absolute [`PdfLineSelection`] by intersecting the criteria that are present.
pub fn pdfline_selection_from_dict(data: &InputPdfLineSet) -> Result<PdfLineSelection, LineSelectionError> {
    // Called once per selection spec while a formats repository loads, potentially thousands of
    // times across a whole repository: never above `trace!`.
    tracing::trace!(?data, "building a pdf line selection from a dict spec");

    let font_size_set = match data.font_size {
        Some(fs) if fs <= 0.0 => return Err(LineSelectionError::FontSizeNotPositive(fs)),
        Some(fs) => Some(FontSizeInterval::from_precision(fs, FONT_SIZE_PRECISION)),
        None => None,
    };

    let area_set = match &data.area {
        Some(spec) => {
            let input_area = InputArea::build(spec.x_min, spec.x_max, spec.y_min, spec.y_max)?;
            let x_min = input_area.x_min().unwrap_or(0.0);
            let y_min = input_area.y_min().unwrap_or(0.0);
            let x_max = input_area.x_max().unwrap_or(1e6);
            let y_max = input_area.y_max().unwrap_or(1e6);
            Some(Area::new(x_min, y_min, x_max, y_max))
        }
        None => None,
    };

    let text_set = data.text.as_deref().map(TextSet::new);

    let base = PdfLineSet::from_sets(None, font_size_set, text_set, area_set);

    let selection = match &data.font {
        None => base,
        Some(FontCriterion::Single(font)) => PdfLineSet::font(FontSet::new(font)) & base,
        Some(FontCriterion::Multiple(fonts)) => {
            let font_set = fonts
                .iter()
                .map(|f| FontSet::new(f))
                .reduce(|a, b| a | b)
                .ok_or(LineSelectionError::EmptyFontList)?;
            PdfLineSet::font(font_set) & base
        }
    };

    Ok(OptionallyRelative::Absolute(selection))
}

// The compact selection grammar: font, then `[font_size]`, then an area — either a bare vertical
// range or a full area wrapped in an extra pair of brackets — then quoted text, each separated by
// an optional space and every group optional.
//
// The five useful groups are captured by position rather than by name. Anchored at the start,
// because the matcher searches anywhere by default.
static LINE_SET_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"\A([\w\-, ]+)? ?(?:\[([0-9]+(?:\.[0-9]+)?)\])? ?(?:(\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\))|\((\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\)\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\))\))? ?(?:"(.*)")?"#,
    )
    .expect("fixed, hand-written pattern, valid onig regex")
});

/// The same pattern as [`LINE_SET_REGEXP`], anchored **at the end too**.
///
/// It exists for validating a formats repository's tables, not for parsing. Every group of the
/// pattern is optional, so the un-anchored version matches *any* string and
/// [`pdfline_selection_from_str`] never rejects anything — a mistyped cell would silently produce
/// an empty selection. Validation needs to be able to say "this cell is not a selection", which is
/// what this pattern answers.
static LINE_SET_ANCHORED_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"\A([\w\-, ]+)? ?(?:\[([0-9]+(?:\.[0-9]+)?)\])? ?(?:(\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\))|\((\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\)\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\))\))? ?(?:"(.*)")?\z"#,
    )
    .expect("fixed, hand-written pattern, valid onig regex")
});

/// Whether `input` is, in its entirety, a line selection written in the compact grammar.
///
/// See `LINE_SET_ANCHORED_REGEXP` for why this is not the same question as whether
/// [`pdfline_selection_from_str`] returned `Ok`.
pub fn is_pdfline_selection(input: &str) -> bool {
    LINE_SET_ANCHORED_REGEXP.find(input).is_some()
}

/// Splits an `"a:b"` pair, without its brackets, into two optional bounds; an absent side stays
/// `None`.
fn parse_bound_pair(text: &str) -> (Option<f32>, Option<f32>) {
    let mut parts = text.splitn(2, ':');
    let a = parts.next().unwrap_or("");
    let b = parts.next().unwrap_or("");
    let parse = |s: &str| (!s.is_empty()).then(|| s.parse::<f32>().expect("digits matched by LINE_SET_REGEXP always parse as f32"));
    (parse(a), parse(b))
}

/// Parses the compact grammar into the structured form, then **delegates** to
/// [`pdfline_selection_from_dict`].
pub fn pdfline_selection_from_str(input: &str) -> Result<PdfLineSelection, LineSelectionError> {
    // Same volume caveat as [`pdfline_selection_from_dict`]: `trace!` only.
    tracing::trace!(input, "parsing a compact pdf line selection expression");

    let captures = LINE_SET_REGEXP
        .captures(input)
        .expect("every group in LINE_SET_REGEXP is optional, so it matches any string, including the empty one");

    let font = captures.at(1).map(|f| FontCriterion::Single(f.trim().to_string()));
    let font_size =
        captures.at(2).map(|s| s.parse::<f32>().expect("digits matched by LINE_SET_REGEXP always parse as f32"));

    let area = if let Some(y_range) = captures.at(3) {
        // The vertical range is the whole `"(a:b)"` string, brackets included.
        let (y_min, y_max) = parse_bound_pair(&y_range[1..y_range.len() - 1]);
        Some(InputAreaSpec { x_min: None, x_max: None, y_min, y_max })
    } else if let Some(area_text) = captures.at(4) {
        // The area text is `"(a:b)(c:d)"` without the outer wrapping brackets; splitting on `")("`
        // separates the two pairs.
        let (x_part, y_part) = area_text
            .split_once(")(")
            .expect("area_text is always shaped \"(a:b)(c:d)\" by construction of LINE_SET_REGEXP");
        let (x_min, x_max) = parse_bound_pair(x_part.trim_start_matches('('));
        let (y_min, y_max) = parse_bound_pair(y_part.trim_end_matches(')'));
        Some(InputAreaSpec { x_min, x_max, y_min, y_max })
    } else {
        None
    };

    let text = captures.at(5).map(|t| t.to_string());

    pdfline_selection_from_dict(&InputPdfLineSet { text, font, font_size, area })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::pdf_extract::relative::OptionallyRelative;

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
                panic!("pdfline_selection_from_dict/_from_str must never build a Relative selection")
            }
        }
    }

    mod pdfline_selection_from_dict_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn with_every_field_absent_it_accepts_any_line() {
            let selection = pdfline_selection_from_dict(&InputPdfLineSet::default()).unwrap();
            assert!(selects(&selection, &line("Arial", 12.0, "anything", (0.0, 0.0, 10.0, 10.0))));
            assert!(selects(&selection, &line("Times New Roman", 999.0, "", (500.0, 500.0, 600.0, 600.0))));
        }

        #[test]
        fn a_single_font_accepts_only_that_font() {
            let data = InputPdfLineSet { font: Some(FontCriterion::Single("Arial".to_string())), ..Default::default() };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Times", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn a_list_of_two_or_more_fonts_accepts_the_union() {
            let data = InputPdfLineSet {
                font: Some(FontCriterion::Multiple(vec!["Arial".to_string(), "Times".to_string()])),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(selects(&selection, &line("Times", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Courier", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn font_size_selects_a_narrow_interval_around_the_given_value() {
            let data = InputPdfLineSet { font_size: Some(12.0), ..Default::default() };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 12.0005, "x", (0.0, 0.0, 1.0, 1.0))), "just inside [11.999, 12.001]");
            assert!(!selects(&selection, &line("Arial", 12.005, "x", (0.0, 0.0, 1.0, 1.0))), "clearly outside [11.999, 12.001]");
        }

        #[test]
        fn area_with_partial_bounds_defaults_missing_bounds_to_zero_and_one_million() {
            // The absent bounds are replaced by the defaults.
            let data = InputPdfLineSet {
                area: Some(InputAreaSpec { x_min: Some(5.0), x_max: None, y_min: None, y_max: Some(50.0) }),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            // Inside (5.0, 0.0, 1e6, 50.0).
            assert!(selects(&selection, &line("Arial", 10.0, "x", (10.0, 10.0, 20.0, 20.0))));
            // Outside: x0 below 5.0.
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (1.0, 10.0, 4.0, 20.0))));
            // Outside: y1 above 50.0.
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (10.0, 10.0, 20.0, 60.0))));
        }

        #[test]
        fn text_is_passed_through_to_the_text_set_unchanged() {
            let data = InputPdfLineSet { text: Some("^foo".to_string()), ..Default::default() };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "foobar", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Arial", 10.0, "barfoo", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn two_criteria_present_together_intersect_rather_than_union() {
            let data = InputPdfLineSet { font: Some(FontCriterion::Single("Arial".to_string())), font_size: Some(12.0), ..Default::default() };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 12.0, "x", (0.0, 0.0, 1.0, 1.0))), "matches both");
            assert!(!selects(&selection, &line("Arial", 50.0, "x", (0.0, 0.0, 1.0, 1.0))), "font matches, size doesn't");
            assert!(!selects(&selection, &line("Times", 12.0, "x", (0.0, 0.0, 1.0, 1.0))), "size matches, font doesn't");
        }

        #[test]
        fn a_non_positive_font_size_is_rejected() {
            let zero = InputPdfLineSet { font_size: Some(0.0), ..Default::default() };
            let err = expect_err(pdfline_selection_from_dict(&zero));
            let LineSelectionError::FontSizeNotPositive(v) = err else { panic!("expected FontSizeNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);

            let negative = InputPdfLineSet { font_size: Some(-3.0), ..Default::default() };
            let err = expect_err(pdfline_selection_from_dict(&negative));
            let LineSelectionError::FontSizeNotPositive(v) = err else { panic!("expected FontSizeNotPositive, got {err:?}") };
            assert_eq!(v, -3.0);
        }

        #[test]
        fn an_explicitly_empty_font_list_is_rejected() {
            let data = InputPdfLineSet { font: Some(FontCriterion::Multiple(vec![])), ..Default::default() };
            let err = expect_err(pdfline_selection_from_dict(&data));
            assert!(matches!(err, LineSelectionError::EmptyFontList));
        }

        #[test]
        fn an_invalid_area_bubbles_up_a_position_error() {
            let data = InputPdfLineSet { area: Some(InputAreaSpec { x_min: Some(0.0), ..Default::default() }), ..Default::default() };
            let err = expect_err(pdfline_selection_from_dict(&data));
            let LineSelectionError::Area(PositionError::XMinNotPositive(v)) = err else { panic!("expected Area(XMinNotPositive), got {err:?}") };
            assert_eq!(v, 0.0);
        }
    }

    mod pdfline_selection_from_str_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_bare_font_selects_only_that_font() {
            let selection = pdfline_selection_from_str("Arial").unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Times", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn a_bracketed_number_alone_selects_that_font_size() {
            let selection = pdfline_selection_from_str("[12.0]").unwrap();
            assert!(selects(&selection, &line("Arial", 12.0005, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(selects(&selection, &line("Times", 12.0005, "x", (0.0, 0.0, 1.0, 1.0))), "font unrestricted");
            assert!(!selects(&selection, &line("Arial", 12.005, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        /// The full-area grammar needs a pair of ranges wrapped in an *extra* pair of brackets, not
        /// a bare `(x0:x1)(y0:y1)`. Without the outer brackets the pattern matches only the first
        /// range, silently ignoring the rest — which is why this is pinned rather than assumed.
        #[test]
        fn a_double_parenthesized_range_pair_selects_the_full_area() {
            let selection = pdfline_selection_from_str("((1:10)(2:20))").unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "x", (3.0, 5.0, 6.0, 8.0))), "strictly inside (1,2,10,20)");
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (50.0, 5.0, 60.0, 8.0))), "outside on x");
        }

        #[test]
        fn a_single_parenthesized_range_selects_a_vertical_band() {
            let selection = pdfline_selection_from_str("(2:20)").unwrap();
            // The horizontal axis is unbounded and only the vertical one is constrained.
            assert!(selects(&selection, &line("Arial", 10.0, "x", (100.0, 5.0, 200.0, 10.0))));
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (100.0, 30.0, 200.0, 40.0))));
        }

        #[test]
        fn a_quoted_string_alone_selects_that_text() {
            let selection = pdfline_selection_from_str("\"foo\"").unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "a foo b", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Arial", 10.0, "bar", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn an_empty_string_selects_everything() {
            let selection = pdfline_selection_from_str("").unwrap();
            assert!(selects(&selection, &line("Whatever", 999.0, "anything", (500.0, 500.0, 600.0, 600.0))));
        }

        #[test]
        fn groups_separated_by_spaces_combine_with_intersection() {
            let selection = pdfline_selection_from_str("Arial [12.0] ((1:10)(2:20)) \"foo\"").unwrap();
            assert!(selects(&selection, &line("Arial", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0))), "matches all four criteria");
            assert!(!selects(&selection, &line("Times", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0))), "font alone fails");
            assert!(!selects(&selection, &line("Arial", 12.0, "bar", (3.0, 5.0, 6.0, 8.0))), "text alone fails");
        }

        /// The font character class includes the comma, so a literal comma in a font name does not
        /// split the capture into two criteria; it stays one. Were it wrongly treated as a union of
        /// two fonts, a line in only the second one would pass, and it must not.
        #[test]
        fn a_font_containing_a_literal_comma_is_a_single_criterion_not_split_on_the_comma() {
            let selection = pdfline_selection_from_str("Arial,Bold").unwrap();
            assert!(selects(&selection, &line("Arial,Bold", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Bold", 10.0, "x", (0.0, 0.0, 1.0, 1.0))), "must not be treated as font Arial OR font Bold");
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))), "must not be treated as font Arial OR font Bold");
        }

        #[test]
        fn agrees_with_from_dict_on_an_equivalent_hand_built_criterion_set() {
            let via_str = pdfline_selection_from_str("Arial [12.0] ((1:10)(2:20)) \"foo\"").unwrap();
            let via_dict = pdfline_selection_from_dict(&InputPdfLineSet {
                font: Some(FontCriterion::Single("Arial".to_string())),
                font_size: Some(12.0),
                area: Some(InputAreaSpec { x_min: Some(1.0), x_max: Some(10.0), y_min: Some(2.0), y_max: Some(20.0) }),
                text: Some("foo".to_string()),
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
                assert_eq!(selects(&via_str, &probe), selects(&via_dict, &probe), "mismatch for {probe:?}");
            }
        }

        #[test]
        fn a_non_positive_font_size_error_propagates_from_from_dict() {
            let err = expect_err(pdfline_selection_from_str("[0.0]"));
            let LineSelectionError::FontSizeNotPositive(v) = err else { panic!("expected FontSizeNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn an_invalid_area_error_propagates_from_from_dict() {
            let err = expect_err(pdfline_selection_from_str("((0:10)(2:20))"));
            let LineSelectionError::Area(PositionError::XMinNotPositive(v)) = err else { panic!("expected Area(XMinNotPositive), got {err:?}") };
            assert_eq!(v, 0.0);
        }
    }

    mod line_selection_error_display {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn font_size_not_positive_displays_the_offending_value() {
            assert_eq!(LineSelectionError::FontSizeNotPositive(-2.5).to_string(), "font_size must be positive, found -2.5");
        }

        #[test]
        fn empty_font_list_displays_a_fixed_message() {
            assert_eq!(LineSelectionError::EmptyFontList.to_string(), "font list must not be empty when provided");
        }

        #[test]
        fn area_displays_transparently_as_the_wrapped_position_error() {
            let inner = PositionError::XMinNotPositive(0.0);
            let expected = inner.to_string();
            assert_eq!(LineSelectionError::Area(inner).to_string(), expected);
        }
    }
}
