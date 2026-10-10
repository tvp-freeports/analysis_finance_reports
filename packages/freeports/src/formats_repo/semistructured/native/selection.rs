//! A line selection as a formats repository's YAML writes it.
//!
//! Two shapes of the same thing, and in YAML the **mapping** is the one to write.
//!
//! The mapping form is [`InputPdfLineSet`] — `{text, font, font_size, area, window, without}` — and
//! it now states the whole of the grammar: an area side may be a line of the page, `window` hangs a
//! band off an anchor, `without` subtracts. While it could not, a selection needing one of those
//! three had to be written as a compact string even where every selection around it was a mapping,
//! and a repository ended up with both spellings side by side for a reason no author chose. That
//! was the only argument for the string form here, and it is gone.
//!
//! The **compact grammar** stays, for two reasons that have nothing to do with expressiveness: the
//! structured level's cells live in a CSV, where a mapping cannot be written, and a one-criterion
//! selection is shorter as `'arialmt[7.5:8.5]'` than as three lines. It is read where it is found,
//! and it is **validated** before it is parsed: [`pdfline_selection_from_str`] answers an
//! expression it does not recognise with the selection of *every line*, which for a label is the
//! one failure that produces output instead of an error.
//!
//! In YAML, prefer the mapping. It says which criterion is which without a grammar to know, it
//! nests instead of quoting — an anchor inside an area inside a window needs no `<…>` and no
//! escaping — and a key it does not know it **refuses**, where a mistyped compact cell can only be
//! caught by the shape of the whole string.

use serde::de::Visitor;
use serde::de::value::MapAccessDeserializer;
use serde::{Deserialize, Deserializer};

use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;
use crate::input::document::selection::{
    InputPdfLineSet, LineSelectionError, is_pdfline_selection, pdfline_selection_from_dict,
    pdfline_selection_from_str,
};

/// What a selection written in configuration can be refused for.
#[derive(Debug, thiserror::Error)]
pub enum SelectionConfigError {
    #[error("field '{field}': {source}")]
    Malformed {
        field: &'static str,
        #[source]
        source: LineSelectionError,
    },
    /// A string the compact grammar does not read in its entirety.
    ///
    /// The mistake a selection moved out of Python makes, and it is silent:
    /// `PdfLineSelection(text="^Gestora:")` becomes the cell `"^Gestora:"` **with** its quotes,
    /// because in the grammar a bare word is a font name. Without them the cell is a font nobody
    /// has — or, worse, an expression that states no criterion and takes the whole page.
    #[error("field '{field}': '{cell}' is not a line selection written in the compact grammar")]
    NotASelection { field: &'static str, cell: String },
}

/// A line selection: the mapping form, or the compact grammar. See the module documentation.
#[derive(Debug, Clone)]
pub enum InputSelection {
    /// The compact grammar — fonts, `[size]`, areas with anchors, `window(..)`, and `A / B`.
    Compact(String),
    /// The mapping form, which states the same grammar as a tree of keys. Boxed because it is a
    /// tree: the compact form is a string, and a selection written as one must not pay for the
    /// other's size in every configuration that holds one.
    Structured(Box<InputPdfLineSet>),
}

/// Written by hand rather than derived as an untagged enum, for the error message.
///
/// `#[serde(untagged)]` tries each variant and, when none fits, says only *data did not match any
/// variant of untagged enum InputSelection* — it has thrown away which variant was nearly right
/// and why it failed. A mapping with a mistyped key is the likeliest mistake in a repository where
/// every selection is a mapping, and `unknown field 'fonts', expected one of 'text', 'font', …` is
/// the difference between a message that names the typo and one that does not.
///
/// So the variant is chosen by what **arrives** — a string is the compact form, a mapping the
/// structured one — and the mapping's own error is propagated untouched.
impl<'de> Deserialize<'de> for InputSelection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SelectionVisitor;

        impl<'de> Visitor<'de> for SelectionVisitor {
            type Value = InputSelection;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a line selection: a mapping of criteria, or a compact expression in quotes")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(InputSelection::Compact(value.to_string()))
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                InputPdfLineSet::deserialize(MapAccessDeserializer::new(map))
                    .map(|spec| InputSelection::Structured(Box::new(spec)))
            }
        }

        deserializer.deserialize_any(SelectionVisitor)
    }
}

impl InputSelection {
    /// The selection this configuration describes. `field` is the YAML key, for the message.
    pub fn build(&self, field: &'static str) -> Result<PdfLineSelection, SelectionConfigError> {
        match self {
            InputSelection::Compact(cell) => {
                if !is_pdfline_selection(cell) {
                    return Err(SelectionConfigError::NotASelection { field, cell: cell.clone() });
                }
                pdfline_selection_from_str(cell)
                    .map_err(|source| SelectionConfigError::Malformed { field, source })
            }
            InputSelection::Structured(spec) => pdfline_selection_from_dict(spec)
                .map_err(|source| SelectionConfigError::Malformed { field, source }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(yaml: &str) -> InputSelection {
        serde_yaml::from_str(yaml).expect("the test's own yaml")
    }

    mod the_compact_form {
        use super::*;

        #[test]
        fn a_font_union_a_size_and_an_anchor_are_one_cell() {
            selection("'Lato|Open Sans|Lato-Regular[6.9:7.1] \"^TOTALE\"'")
                .build("label")
                .expect("the three criteria intersect, and the fonts unite");
        }

        #[test]
        fn a_difference_is_expressible_and_the_mapping_form_cannot() {
            selection("'TrebuchetMS-Bold \"^VALORE COMPLESSIVO NETTO DEL FONDO\" / \"CLASSE\"'")
                .build("label")
                .expect("the share class is subtracted");
        }

        /// A side left **empty** is the open one, and a side written `0` is refused.
        ///
        /// `InputArea::build` validates every side it is *given* as positive, and fills in an
        /// absent one with `0.0` or `1e6` afterwards. So `((:370)(:))` is the whole-left-column
        /// selection and `((0:370)(0:))` is a configuration error — the opposite of what
        /// `PdfLineSelection.area(0.0, 0.0, 370.0, 1e6)` looks like in Python, which is where the
        /// measured cells of these statements come from.
        #[test]
        fn an_area_side_may_be_a_line_of_the_page() {
            selection("'((:<\"^Situazione a fine\">)(:))'")
                .build("value")
                .expect("the column's right edge is wherever its heading is");
        }

        #[test]
        fn a_zero_side_is_refused_and_an_empty_one_is_the_open_side() {
            let error = selection("'((0:370)(:))'")
                .build("value")
                .err()
                .expect("a side written zero is not a side left open");
            assert!(matches!(error, SelectionConfigError::Malformed { field: "value", .. }), "{error}");
            selection("'((:370)(:))'").build("value").expect("the same area, with the side left open");
        }

        #[test]
        fn an_unquoted_anchor_is_refused() {
            let error = selection("'^Gestora:'")
                .build("label")
                .err()
                .expect("a bare anchor is a font name, and a font nobody has selects nothing");
            assert!(matches!(error, SelectionConfigError::NotASelection { field: "label", .. }), "{error}");
        }
    }

    mod the_mapping_form {
        use super::*;

        #[test]
        fn keeps_its_keys() {
            selection("{font: arial, font_size: [7.5, 8.5], text: it is managed by}")
                .build("label")
                .expect("the criteria it states, intersected");
        }

        /// The three forms that used to need the compact string, now that the mapping states them.
        #[test]
        fn states_an_anchored_side_a_window_and_a_difference() {
            selection("{area: {x_max: {text: '^Situazione a fine'}}, without: [{text: ','}]}")
                .build("value")
                .expect("an anchored side and a subtraction");
            selection(
                "{window: {anchor: {font: arial, text: 'Societa di gestione:'}, dx: -0.2, dy: 1.5, width: 100.0, height: 1.2}}",
            )
            .build("label")
            .expect("a window hangs off its anchor");
        }

        /// What the hand-written `Deserialize` is for: an untagged enum would answer *data did not
        /// match any variant* and throw the typo away.
        #[test]
        fn a_mistyped_key_is_named_in_the_refusal() {
            let error = serde_yaml::from_str::<InputSelection>("{fonts: arialmt}")
                .expect_err("a key the mapping does not know must not be dropped in silence");
            let message = error.to_string();
            assert!(message.contains("unknown field `fonts`"), "{message}");
            assert!(message.contains("`font_size`"), "{message}");
        }

        #[test]
        fn a_selection_that_is_neither_a_string_nor_a_mapping_says_what_one_is() {
            let error = serde_yaml::from_str::<InputSelection>("[arialmt]")
                .expect_err("a list is not a selection");
            assert!(error.to_string().contains("a line selection"), "{error}");
        }

        #[test]
        fn a_refusal_names_the_field_it_came_from() {
            let error = selection("{font_size: [21, 18]}")
                .build("body")
                .err()
                .expect("an inverted interval is refused rather than swapped");
            assert!(matches!(error, SelectionConfigError::Malformed { field: "body", .. }), "{error}");
        }
    }
}
