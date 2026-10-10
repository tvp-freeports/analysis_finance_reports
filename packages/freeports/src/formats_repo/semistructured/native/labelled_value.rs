//! The YAML surface of `A1`: *a label declares a value right next to it*.
//!
//! One name, `labelled_value`, in the `pdf_extract` segment. It is the most used pipe of the
//! collection — forty-eight call sites in sixteen author modules before this — and the last one
//! that kept a `pdf_extract` written in Python for a datum that is not geometry of its own but a
//! **relation between two lines**, which [`PdfExtractLabelledValueStandard`] already states.
//!
//! # What a label takes to state
//!
//! A label is almost never one criterion. Measured over the forty-eight sites, it is a font (often
//! a **union** of families, `R8`), a size interval, and an anchored text — and in four sites a
//! **difference**, because the share class of a statement begins with the same words as the fund
//! (`VALORE COMPLESSIVO NETTO DEL FONDO` against `… CLASSE AM`). Eight more hang a `window(..)` off
//! the label, and three bound a column by a heading somewhere else on the page.
//!
//! Every one of those is a key of the mapping form, which is the one to write here: see
//! [`super::selection`] for the choice and [`InputSelection`] for the two shapes it admits.
//!
//! # What stays in Python
//!
//! Nothing of `A1` itself. What a format still writes by hand is a `pdf_extract` that is **not** a
//! labelled value: a page number read off the foot, a table, a section numbered by vertical gap.

use serde::Deserialize;

use crate::core::classes::BlockType;
use crate::formats_utils::pdf_extract::labelled_value::{
    LabelledValueArgs, LabelledValueTake, LabelledValueWhere, LabelledValueWindow,
    PdfExtractLabelledValueStandard,
};
use crate::formats_utils::pdf_extract::section::SectionEnd;
use super::selection::{InputSelection, SelectionConfigError};

/// What a labelled-value configuration can be refused for.
#[derive(Debug, thiserror::Error)]
pub enum LabelledValueConfigError {
    #[error(transparent)]
    Selection(#[from] SelectionConfigError),
    #[error("'{0}' is not a placement: on_line, below or right")]
    UnknownWhere(String),
    #[error("'{0}' is not a way of taking the value: first or all")]
    UnknownTake(String),
    /// Both ways of closing the label's section at once. The same refusal the Python surface
    /// carries: there is no way to honour both, and resolving it by precedence would hide a wiring
    /// mistake.
    #[error("'stop' and 'stop_at_label_family' are alternatives, not a pair")]
    TwoStops,
}

/// The YAML configuration of `labelled_value`.
///
/// Three fields are required — the label, the block it emits and the pipe's name — and the rest
/// default to the behaviour of a label that declares its value **on its own line**, which is what
/// twenty-one formats of the collection write the management company as.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputLabelledValue {
    /// How the label is recognised.
    pub label: InputSelection,
    /// The kind of block the pipe emits.
    pub block_type: String,
    /// The pipe's name: what the log and the error messages call it.
    pub name: String,
    /// `on_line` (the default), `below` or `right`.
    #[serde(default)]
    pub r#where: Option<String>,
    /// How far **along** the placement's direction, in multiples of the label's own height.
    #[serde(default)]
    pub reach: Option<f32>,
    /// How far **across** it, in the same unit.
    #[serde(default)]
    pub across: Option<f32>,
    /// How the value is recognised among the candidates of the window. Optional, because on two of
    /// the six measured `right` formats the value carries exactly the label's font and only its
    /// position tells them apart.
    #[serde(default)]
    pub value: Option<InputSelection>,
    /// Where the label's section ends: the next line this selection takes.
    #[serde(default)]
    pub stop: Option<InputSelection>,
    /// The alternative to `stop`: the section ends at the next line carrying the label's own font
    /// and size.
    #[serde(default)]
    pub stop_at_label_family: bool,
    /// `first` (the default, and a page that carries none fails) or `all` (and none is an answer).
    #[serde(default)]
    pub take: Option<String>,
}

/// The placement a configuration names.
fn placement(name: Option<&str>) -> Result<LabelledValueWhere, LabelledValueConfigError> {
    match name.unwrap_or("on_line") {
        "on_line" => Ok(LabelledValueWhere::OnLine),
        "below" => Ok(LabelledValueWhere::Below),
        "right" => Ok(LabelledValueWhere::Right),
        other => Err(LabelledValueConfigError::UnknownWhere(other.to_string())),
    }
}

/// How many values a configuration asks for.
fn take(name: Option<&str>) -> Result<LabelledValueTake, LabelledValueConfigError> {
    match name.unwrap_or("first") {
        "first" => Ok(LabelledValueTake::First),
        "all" => Ok(LabelledValueTake::All),
        other => Err(LabelledValueConfigError::UnknownTake(other.to_string())),
    }
}

impl InputLabelledValue {
    /// The pipe this configuration describes.
    pub fn build(&self) -> Result<PdfExtractLabelledValueStandard, LabelledValueConfigError> {
        let placement = placement(self.r#where.as_deref())?;
        let stop = match (&self.stop, self.stop_at_label_family) {
            (Some(_), true) => return Err(LabelledValueConfigError::TwoStops),
            (Some(selection), false) => Some(SectionEnd::Selection(selection.build("stop")?)),
            (None, true) => Some(SectionEnd::SameFamilyAsAnchor),
            (None, false) => None,
        };

        Ok(PdfExtractLabelledValueStandard::new(LabelledValueArgs {
            label: self.label.build("label")?,
            block_type: BlockType::from(self.block_type.clone()),
            name: self.name.clone(),
            r#where: placement,
            // A window only where the configuration asked for one, so that the unset side keeps the
            // default its placement carries rather than an arbitrary zero. The same rule the Python
            // surface had, and the reason `reach` and `across` are two cells and not one pair.
            window: (self.reach.is_some() || self.across.is_some()).then(|| {
                let default = LabelledValueWindow::default_for(placement);
                LabelledValueWindow {
                    reach: self.reach.or(default.reach),
                    across: self.across.unwrap_or(default.across),
                }
            }),
            value: match &self.value {
                Some(selection) => Some(selection.build("value")?),
                None => None,
            },
            stop,
            take: take(self.take.as_deref())?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(yaml: &str) -> InputLabelledValue {
        serde_yaml::from_str(yaml).expect("the test's own yaml")
    }

    /// A configuration whose label is an anchored text and nothing else.
    ///
    /// The quotes are **inside** the YAML string on purpose: in the compact grammar a bare word is
    /// a font name, and a text criterion is quoted. A cell that forgets them is the mistake
    /// [`is_pdfline_selection`] catches, and `a_text_criterion_has_to_be_quoted` freezes it.
    fn labelled(label: &str, rest: &str) -> InputLabelledValue {
        input(&format!("label: '\"{label}\"'\nblock_type: X\nname: n\n{rest}"))
    }

    mod the_required_fields {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn three_of_them_are_enough() {
            let pipe = labelled("^Management Company", "")
                .build()
                .expect("a label, a block and a name are a whole configuration");
            assert_eq!(pipe.placement(), LabelledValueWhere::OnLine);
            assert_eq!(pipe.window().reach, None);
        }

        #[test]
        fn a_missing_label_is_refused_by_serde() {
            let error = serde_yaml::from_str::<InputLabelledValue>("block_type: X\nname: n\n")
                .expect_err("a configuration with no label reads nothing");
            assert!(error.to_string().contains("label"), "{error}");
        }

        #[test]
        fn an_unknown_field_is_refused_rather_than_ignored() {
            let error = serde_yaml::from_str::<InputLabelledValue>(
                "label: '\"^A\"'\nblock_type: X\nname: n\nwhere_: below\n",
            )
            .expect_err("a misspelled key must not be dropped in silence");
            assert!(error.to_string().contains("where_"), "{error}");
        }
    }

    mod the_two_shapes_of_a_selection {
        use super::*;

        #[test]
        fn a_string_goes_through_the_compact_grammar() {
            input("label: 'Lato|TrebuchetMS[6.9:7.1] \"^TOTALE ATTIVITA\"'\nblock_type: X\nname: n\n")
                .build()
                .expect("a font union, a size interval and an anchor are one cell");
        }

        #[test]
        fn a_mapping_goes_through_the_dict_form() {
            input("label:\n  font: arial\n  font_size: [7.5, 8.5]\n  text: it is managed by\nblock_type: X\nname: n\n")
                .build()
                .expect("the mapping form keeps its keys");
        }

        #[test]
        fn a_difference_is_the_reason_the_string_form_exists() {
            input(
                "label: 'TrebuchetMS-Bold[6.9:7.0] \"^VALORE COMPLESSIVO NETTO DEL FONDO\" / \"CLASSE\"'\nblock_type: X\nname: n\n",
            )
            .build()
            .expect("the share class is subtracted from the fund's own statement");
        }

        #[test]
        fn a_window_is_a_term_of_the_same_grammar() {
            input("label: 'window(\"Gestora:\", 1.0, -0.5, 100.0, 2.0)'\nblock_type: X\nname: n\n")
                .build()
                .expect("a moved window is a cell too");
        }

        #[test]
        fn a_cell_the_grammar_does_not_read_whole_is_refused() {
            let error = input("label: 'not a selection (at all'\nblock_type: X\nname: n\n")
                .build()
                .err()
                .expect("an unreadable cell would otherwise select every line of the page");
            assert!(
                matches!(
                    error,
                    LabelledValueConfigError::Selection(SelectionConfigError::NotASelection {
                        field: "label",
                        ..
                    })
                ),
                "{error}"
            );
        }

        /// The one mistake a label moved out of Python can make, and it is silent.
        ///
        /// `PdfLineSelection(text="^Gestora:")` becomes the cell `"^Gestora:"` **with** its quotes:
        /// without them the grammar reads a font name, and a font nobody has selects nothing. It is
        /// the same family of defect as `(?P<name>…)` against Oniguruma, and it is refused here for
        /// the same reason — a cell that is not a selection must not become one that matches
        /// nothing, or everything.
        #[test]
        fn a_text_criterion_has_to_be_quoted() {
            let error = input("label: '^Gestora:'\nblock_type: X\nname: n\n")
                .build()
                .err()
                .expect("a bare anchor is not a selection: a font name has no '^' in it");
            assert!(
                matches!(error, LabelledValueConfigError::Selection(SelectionConfigError::NotASelection { .. })),
                "{error}"
            );
        }
    }

    mod the_placement_and_its_window {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn below_carries_its_measured_default() {
            let pipe = labelled("^Situazione al", "where: below\n").build().expect("below needs nothing else");
            assert_eq!(pipe.placement(), LabelledValueWhere::Below);
            assert_eq!(pipe.window(), LabelledValueWindow::default_for(LabelledValueWhere::Below));
        }

        #[test]
        fn right_sets_no_reach_at_all() {
            let pipe = labelled("^Total Assets", "where: right\n").build().expect("right needs nothing else");
            assert_eq!(pipe.window().reach, None);
            assert_eq!(pipe.window().across, 0.0);
        }

        #[test]
        fn one_side_given_leaves_the_other_at_its_placements_default() {
            let pipe = labelled("^Management Company", "where: below\nreach: 12.0\n")
                .build()
                .expect("a reach alone is a legitimate configuration");
            assert_eq!(pipe.window().reach, Some(12.0));
            assert_eq!(pipe.window().across, 0.5, "the across of `below`, not zero");
        }

        #[test]
        fn an_unknown_placement_names_the_three_that_exist() {
            let error =
                labelled("^A", "where: beside\n").build().err().expect("there are three placements");
            assert_eq!(error.to_string(), "'beside' is not a placement: on_line, below or right");
        }
    }

    mod the_section_end {
        use super::*;

        #[test]
        fn a_selection_closes_it() {
            labelled("^Management Company", "stop: 'arialmt[7:8]'\n")
                .build()
                .expect("the next label closes the section");
        }

        #[test]
        fn the_label_family_closes_it_too() {
            labelled("^Management Company", "stop_at_label_family: true\n")
                .build()
                .expect("the anchor's own typography is the other way");
        }

        #[test]
        fn both_at_once_are_refused() {
            let error = labelled("^A", "stop: 'arialmt'\nstop_at_label_family: true\n")
                .build()
                .err()
                .expect("there is no way to honour both");
            assert!(matches!(error, LabelledValueConfigError::TwoStops), "{error}");
        }
    }

    mod how_many_values {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn all_is_a_word_of_the_configuration() {
            labelled("^Management Company", "take: all\n")
                .build()
                .expect("a list of values is a shape two formats need");
        }

        #[test]
        fn an_unknown_word_is_refused() {
            let error =
                labelled("^A", "take: some\n").build().err().expect("there are two ways of taking a value");
            assert_eq!(error.to_string(), "'some' is not a way of taking the value: first or all");
        }
    }
}
