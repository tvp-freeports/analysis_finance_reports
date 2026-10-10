//! A line selection as a formats repository's YAML writes it.
//!
//! Two shapes, because a selection is written for two different jobs. The mapping form —
//! [`InputPdfLineSet`], `{font, font_size, text, area}` — intersects the criteria it is given and
//! reads well where there are two or three of them. The **compact grammar**, the one the structured
//! level's cells are written in, expresses what the mapping cannot: a font **union** (`R8`), an
//! area whose side is a line of the page, a `window(..)`, and the difference `A / B` — and a
//! measured label needs one of those four often enough that offering only the mapping would push
//! the selection back into Python.
//!
//! So a selection here is either, and a string is **validated** before it is parsed:
//! [`pdfline_selection_from_str`] answers an expression it does not recognise with the selection of
//! *every line*, which for a label is the one failure that produces output instead of an error.

use serde::Deserialize;

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

/// A line selection: the compact grammar, or the mapping form. See the module documentation.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum InputSelection {
    /// The compact grammar — fonts, `[size]`, areas with text anchors, `window(..)`, and `A / B`.
    Compact(String),
    /// Four optional criteria, intersected.
    Structured(InputPdfLineSet),
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
                .expect("four optional criteria, intersected");
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
