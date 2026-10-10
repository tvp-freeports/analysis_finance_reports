//! The YAML surface of the **printed page number**: the name `page_number`, and the cell that puts
//! the number into another pipe's blocks.
//!
//! It is `P10`, and it was the last `pdf_extract` the engine could not read. What kept it in a
//! format's Python was not the rule — *the lowest selected line that is a number* — but the
//! **selection**: the number sits on the left of an even page and on the right of an odd one in a
//! document paginated as a book, and that is a **union of two areas**. The compact grammar unites
//! fonts and nothing else, so the mapping form had to state a union first: it is the `any_of` key,
//! and this is what asked for it.
//!
//! # Two forms, because the number is used in two ways
//!
//! | | the cell | who asks |
//! |---|---|---|
//! | the number **is** the block | the name `page_number` | UBS-EN23's statement, whose `text_filter` looks for the nearest page before this one |
//! | the number goes in the **metadata** of what another pipe produced | `page_number:` on that pipe | FINECO-EN23@IR's section of indicators, whose sub-fund is named on the page before |
//!
//! The second is a cell on the pipe that produces the blocks rather than a pipe of its own,
//! because a segment's pipes are **concatenated and not chained**.

use serde::Deserialize;

use crate::core::classes::BlockType;
use crate::formats_utils::pdf_extract::page_number::PdfExtractPageNumberStandard;
use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;

use super::selection::{InputSelection, SelectionConfigError};

/// The YAML configuration of `page_number`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPageNumber {
    /// Where the foot is. Every measured one is a band — or two — at the bottom of the page,
    /// qualified by the typography the foot is set in.
    pub selection: InputSelection,
    /// The kind of block the number comes out as.
    pub block_type: String,
    /// The pipe's name: what the log and the error messages call it.
    pub name: String,
}

impl InputPageNumber {
    pub fn build(&self) -> Result<PdfExtractPageNumberStandard, SelectionConfigError> {
        Ok(PdfExtractPageNumberStandard::new(
            self.selection.build("selection")?,
            BlockType::from(self.block_type.clone()),
            self.name.clone(),
        ))
    }
}

/// The cell that puts the printed page number into the metadata of a pipe's blocks.
///
/// Shared by the pipes that need it, so that the foot is described once per format in one shape and
/// the key cannot be spelled two ways.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPrintedPage {
    /// Where the foot is.
    pub selection: InputSelection,
    /// The metadata key. `page` in every measured pipeline, and a parameter because the reader of
    /// it — `page_key` of `esg` — has always been one.
    #[serde(default = "page_key")]
    pub key: String,
}

fn page_key() -> String {
    "page".to_string()
}

impl InputPrintedPage {
    /// The foot's selection and the key, for the decorator.
    pub fn build(&self) -> Result<(PdfLineSelection, String), SelectionConfigError> {
        Ok((self.selection.build("page_number.selection")?, self.key.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(yaml: &str) -> InputPageNumber {
        serde_yaml::from_str(yaml).expect("the test's own yaml")
    }

    mod the_pipe_whose_block_is_the_number {
        use super::*;
        use crate::core::pipeline::PdfExtractPipe;
        use pretty_assertions::assert_eq;

        /// UBS-EN23's foot: two bands, because the document is paginated as a book — the form the
        /// mapping had to gain `any_of` for.
        const FOOT: &str = "selection:\n  font: frutiger45light\n  font_size: [6.5, 7.5]\n  \
                            any_of:\n    - area: {x_max: 200, y_min: 785}\n    - area: {x_min: 450, y_min: 785}\n";

        #[test]
        fn the_foot_may_be_a_union_of_two_bands() {
            let pipe = input(&format!("{FOOT}block_type: ASSETS_PAGE\nname: assets page\n"))
                .build()
                .expect("a union of two areas is one selection");
            assert_eq!(PdfExtractPipe::name(&pipe), "assets page");
        }

        #[test]
        fn a_centred_foot_is_one_band() {
            input("selection:\n  area: {y_min: 795}\nblock_type: ASSETS_PAGE\nname: page\n")
                .build()
                .expect("one band is a selection too");
        }

        #[test]
        fn an_unknown_field_is_refused_rather_than_ignored() {
            let error = serde_yaml::from_str::<InputPageNumber>(
                "selection: {area: {y_min: 795}}\nblock_type: X\nname: n\nfoot: 795\n",
            )
            .expect_err("a misspelled key must not be dropped in silence");
            assert!(error.to_string().contains("foot"), "{error}");
        }

        #[test]
        fn the_three_fields_are_all_required() {
            for yaml in [
                "block_type: X\nname: n\n",
                "selection: {area: {y_min: 795}}\nname: n\n",
                "selection: {area: {y_min: 795}}\nblock_type: X\n",
            ] {
                assert!(serde_yaml::from_str::<InputPageNumber>(yaml).is_err(), "{yaml}");
            }
        }
    }

    mod the_cell_that_puts_it_in_the_metadata {
        use super::*;
        use pretty_assertions::assert_eq;

        fn printed(yaml: &str) -> InputPrintedPage {
            serde_yaml::from_str(yaml).expect("the test's own yaml")
        }

        #[test]
        fn the_key_defaults_to_the_one_every_measured_pipeline_uses() {
            let (_, key) = printed("selection: {area: {y_min: 795}}\n").build().expect("a foot");
            assert_eq!(key, "page");
        }

        #[test]
        fn the_key_can_be_named() {
            let (_, key) = printed("selection: {area: {y_min: 795}}\nkey: sheet\n").build().expect("a foot");
            assert_eq!(key, "sheet");
        }

        /// The refusal names the cell it came from, and not just `selection`: a format's foot is
        /// one of several selections in the same entry.
        #[test]
        fn a_malformed_foot_is_named_by_its_cell() {
            let error = printed("selection: '^795'\n").build().err().expect("a bare anchor is a font name");
            assert!(
                matches!(error, SelectionConfigError::NotASelection { field: "page_number.selection", .. }),
                "{error}"
            );
        }
    }
}
