//! The YAML surface of `A2`: *a heading opens a section*.
//!
//! One name, `section`, in the `pdf_extract` segment. It is the last of the standard extraction
//! pipes a format still had to build from Python, and the seven measured call sites differ in
//! nothing but their selections: a heading, what closes it, which lines of it count, and — for two
//! of them — how the entries inside it are numbered.
//!
//! # What each form is for, measured
//!
//! | what | which cell | who asks for it |
//! |---|---|---|
//! | the section runs to the next line of the heading's own typography | nothing | five of seven |
//! | …to the next line a selection takes | `family` | FIDEURAM-EN23, DANSKEINVEST-EN24, FINECO-EN23@IR |
//! | …and no further than an ordinate | `bottom` | FINECO-EN23@IR's table, which ends where nothing is a heading |
//! | only some lines of it count | `body` | six of seven: a column, or the all-space lines a generator leaves |
//! | the heading **carries** the datum | `include_heading` | AMUNDI-EN24 p510, whose date is in the heading |
//! | the entries are numbered by **vertical gap** | `entry_gap` | CARNE-EN23 (9.2 within, 18.4 between) |
//!
//! `rows` — the table coordinates — is **not** here. It takes a whole `TableCoordinatesConfig`, and
//! no measured site of this level asks for it: the two formats that read a table through a section
//! do it from Python, where the configuration is a page-by-page computation. The day one asks from
//! configuration, the cell is a mapping of its own and not a flag.

use std::sync::Arc;

use serde::Deserialize;

use crate::core::classes::BlockType;
use crate::core::pipeline::PdfExtractPipe;
use crate::formats_utils::pdf_extract::page_number::PdfExtractWithPrintedPage;
use crate::formats_utils::pdf_extract::section::{
    PdfExtractSectionStandard, SectionArgs, SectionEmit, SectionEnd,
};

use super::page_number::InputPrintedPage;
use super::selection::{InputSelection, SelectionConfigError};

/// What a section configuration can be refused for.
#[derive(Debug, thiserror::Error)]
pub enum SectionConfigError {
    #[error(transparent)]
    Selection(#[from] SelectionConfigError),
    /// A gap that cannot separate two entries. Refused rather than ignored: a section configured to
    /// number its entries and then numbering them all alike is worse than one that says nothing.
    #[error("field 'entry_gap': must be a positive distance, found {0}")]
    EntryGapNotPositive(f32),
}

/// The YAML configuration of `section`.
///
/// Three fields are required — the heading, the block it emits and the pipe's name — and the rest
/// default to what five of the seven measured sites use: closing on the heading's own typography,
/// keeping every line, and emitting one block per line.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSection {
    /// How the heading that opens the section is recognised.
    ///
    /// `R28`: a selection and never a literal, and constrained by **size and area** rather than by
    /// text alone — on ASTERIA-EN24 `Investment Managers` is also the footer logo.
    pub heading: InputSelection,
    /// The kind of block the pipe emits.
    pub block_type: String,
    /// The pipe's name: what the log and the error messages call it.
    pub name: String,
    /// What counts as the next heading, and so closes the section. Absent means *the next line
    /// carrying the heading's own font and size*, which is the measured default.
    #[serde(default)]
    pub family: Option<InputSelection>,
    /// A second floor, which wins if it comes first: a selection, or a bare ordinate.
    #[serde(default)]
    pub bottom: Option<InputBottom>,
    /// Which lines of the section count. Absent takes them all.
    #[serde(default)]
    pub body: Option<InputSelection>,
    /// Whether the heading itself is part of what is emitted — true wherever it **carries** the
    /// datum instead of announcing it.
    #[serde(default)]
    pub include_heading: bool,
    /// The vertical distance at or above which a line opens a **new entry**, for the sections whose
    /// typography says nothing about where one entry ends. Absent means the lines are not numbered.
    #[serde(default)]
    pub entry_gap: Option<f32>,
    /// The foot, where the section's blocks are to carry the **printed page number**.
    ///
    /// For a section whose sub-fund is named on another page: FINECO-EN23@IR's indicators are on
    /// p894 and `Product name:` on p893, and the number in the metadata is what `esg` pairs them
    /// by. Absent is six of the seven measured sections.
    #[serde(default)]
    pub page_number: Option<InputPrintedPage>,
}

/// The extra floor: a line selection, or a bare ordinate.
///
/// Untagged, so a number is a number and a mapping or a string is a selection — which is how the
/// Python surface has always taken it.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum InputBottom {
    Ordinate(f32),
    /// Boxed: a selection is now a whole tree of criteria, and a bare ordinate must not pay for it
    /// in every `InputSection` that carries one.
    Selection(Box<InputSelection>),
}

impl InputBottom {
    fn build(&self) -> Result<SectionEnd, SectionConfigError> {
        Ok(match self {
            InputBottom::Ordinate(y) => SectionEnd::Ordinate(*y),
            InputBottom::Selection(selection) => SectionEnd::Selection(selection.build("bottom")?),
        })
    }
}

impl InputSection {
    /// The pipe this configuration describes, with the printed page number added to its blocks
    /// where the entry asks for it.
    ///
    /// Separate from [`InputSection::build`] because the decorator is a different type: `build`
    /// answers *which section*, this one *which pipe*.
    pub fn pipe(&self) -> Result<Arc<dyn PdfExtractPipe>, SectionConfigError> {
        let section: Arc<dyn PdfExtractPipe> = Arc::new(self.build()?);
        let Some(printed) = &self.page_number else {
            return Ok(section);
        };
        let (selection, key) = printed.build()?;
        Ok(Arc::new(PdfExtractWithPrintedPage::new(
            section, selection, key,
        )))
    }

    /// The section this configuration describes.
    pub fn build(&self) -> Result<PdfExtractSectionStandard, SectionConfigError> {
        let emit = match self.entry_gap {
            None => SectionEmit::Lines,
            Some(gap) if gap > 0.0 => SectionEmit::Entries { gap },
            Some(gap) => return Err(SectionConfigError::EntryGapNotPositive(gap)),
        };

        Ok(PdfExtractSectionStandard::new(SectionArgs {
            heading: self.heading.build("heading")?,
            block_type: BlockType::from(self.block_type.clone()),
            name: self.name.clone(),
            // One cell and no companion flag: the default is a value of the same type, so there is
            // nothing a second cell could say. The lesson `stop` / `stop_at_label_family` taught on
            // the other side of `A1`.
            family: match &self.family {
                Some(selection) => SectionEnd::Selection(selection.build("family")?),
                None => SectionEnd::SameFamilyAsAnchor,
            },
            bottom: match &self.bottom {
                Some(bottom) => Some(bottom.build()?),
                None => None,
            },
            body: match &self.body {
                Some(selection) => Some(selection.build("body")?),
                None => None,
            },
            include_heading: self.include_heading,
            emit,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(yaml: &str) -> InputSection {
        serde_yaml::from_str(yaml).expect("the test's own yaml")
    }

    /// A configuration whose heading is a font, a size and an anchored text — the shape every
    /// measured site has.
    fn section(rest: &str) -> InputSection {
        input(&format!(
            "heading: 'lato-bold[11.5:12.5] \"^Eventi rilevanti\"'\nblock_type: RELEVANT_BLOCK\nname: merges\n{rest}"
        ))
    }

    mod the_required_fields {
        use super::*;

        #[test]
        fn three_of_them_are_enough() {
            section("")
                .build()
                .expect("a heading, a block and a name are a whole configuration");
        }

        #[test]
        fn a_missing_heading_is_refused_by_serde() {
            let error = serde_yaml::from_str::<InputSection>("block_type: X\nname: n\n")
                .expect_err("a configuration with no heading reads nothing");
            assert!(error.to_string().contains("heading"), "{error}");
        }

        #[test]
        fn an_unknown_field_is_refused_rather_than_ignored() {
            let error = serde_yaml::from_str::<InputSection>(
                "heading: '\"^A\"'\nblock_type: X\nname: n\nbody_set: arialmt\n",
            )
            .expect_err("a misspelled key must not be dropped in silence");
            assert!(error.to_string().contains("body_set"), "{error}");
        }

        #[test]
        fn an_unquoted_anchor_is_refused_like_everywhere_else() {
            let error = input("heading: '^Eventi'\nblock_type: X\nname: n\n")
                .build()
                .err()
                .expect("a bare anchor is a font name");
            assert!(
                matches!(
                    error,
                    SectionConfigError::Selection(SelectionConfigError::NotASelection { .. })
                ),
                "{error}"
            );
        }
    }

    mod what_closes_the_section {
        use super::*;

        /// FIDEURAM-EN23 p71: the whole page is arial 7.98, headings included, so the section cannot
        /// close on the typography — the line **after** the heading has the same font and the
        /// section would come out empty. `family` becomes the sentence that really closes it.
        #[test]
        fn a_selection_replaces_the_headings_own_typography() {
            section("family: '\"^The Management Company keeps the control\"'")
                .build()
                .expect("the closing sentence is a selection");
        }

        /// FINECO-EN23@IR's table ends where nothing is a heading at all.
        #[test]
        fn a_bare_ordinate_is_a_floor() {
            section("bottom: 520.0")
                .build()
                .expect("an ordinate is a floor");
        }

        #[test]
        fn a_selection_is_a_floor_too() {
            section("bottom: 'arialmt[7:8] \"^Notes\"'")
                .build()
                .expect("so is a selection");
        }

        /// They are independent on purpose: nothing says a format may not want both.
        #[test]
        fn the_two_can_be_given_together() {
            section("family: 'lato-bold'\nbottom: 700.0")
                .build()
                .expect("a heading and a floor");
        }
    }

    mod which_lines_count {
        use super::*;

        #[test]
        fn the_body_narrows_them() {
            section("body: 'lato-regular[9.5:10.5]'")
                .build()
                .expect("only the body font counts");
        }

        #[test]
        fn an_area_narrows_them_too() {
            section("body: '((50:600)(:850))'")
                .build()
                .expect("a column is an area");
        }

        /// AMUNDI-EN24 p510 puts the date in the heading and leaves twenty-one dateless rows under
        /// it, so the heading is part of what the section emits.
        #[test]
        fn the_heading_can_be_part_of_the_section() {
            section("include_heading: true")
                .build()
                .expect("the heading carries the datum");
        }
    }

    /// The printed page number in the blocks' metadata, which is what kept the seventh measured
    /// section — FINECO-EN23@IR's indicators — in Python: not the section, but the function that
    /// wrapped it.
    mod the_printed_page_number {
        use super::*;
        use crate::core::pipeline::PdfExtractPipe;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_section_may_carry_it() {
            section("page_number:\n  selection:\n    area: {y_min: 795}\n")
                .pipe()
                .expect("a foot is a selection like any other");
        }

        /// The decorator keeps the section's name, because what the log has to name is the pipe
        /// that reads the page.
        #[test]
        fn the_decorated_pipe_keeps_the_sections_name() {
            let pipe = section("page_number:\n  selection:\n    area: {y_min: 795}\n")
                .pipe()
                .expect("a well-formed entry");
            assert_eq!(PdfExtractPipe::name(pipe.as_ref()), "merges");
        }

        #[test]
        fn without_the_cell_the_pipe_is_the_section_itself() {
            let pipe = section("").pipe().expect("a well-formed entry");
            assert_eq!(PdfExtractPipe::name(pipe.as_ref()), "merges");
        }

        #[test]
        fn a_malformed_foot_is_refused_and_named() {
            let error = section("page_number:\n  selection: '^795'\n")
                .pipe()
                .err()
                .expect("a bare anchor is a font name");
            assert!(
                matches!(
                    error,
                    SectionConfigError::Selection(SelectionConfigError::NotASelection {
                        field: "page_number.selection",
                        ..
                    })
                ),
                "{error}"
            );
        }
    }

    mod numbering_the_entries {
        use super::*;
        use pretty_assertions::assert_eq;

        /// CARNE-EN23 p4: 9.2 points within one entry, 18.4 between two, so fifteen separates them.
        #[test]
        fn a_gap_numbers_them() {
            section("entry_gap: 15.0")
                .build()
                .expect("a positive gap separates two entries");
        }

        #[test]
        fn a_gap_of_zero_is_refused_rather_than_numbering_every_line_alike() {
            let error = section("entry_gap: 0.0")
                .build()
                .err()
                .expect("zero separates nothing");
            assert_eq!(
                error.to_string(),
                "field 'entry_gap': must be a positive distance, found 0"
            );
        }

        #[test]
        fn a_negative_gap_is_refused_too() {
            assert!(section("entry_gap: -3.0").build().is_err());
        }
    }
}
