//! The two pipes that read one datum off a page and fail it when the datum is not there:
//! `fund` and `currency`.
//!
//! Both are [`ExtractTextPdfBlockOrFailPage`], which differs from `A1` in what it is given rather
//! than in what it does: `A1` is told where a **label** is and finds the value next to it, these
//! two are told where the **value** is. That is why they stay two names and are not `labelled_value`
//! with a placement: a heading that *is* the fund's name has no label to hang off.
//!
//! # Why they are promoted together with `A1`
//!
//! Not because they were a burden — a call site is one line — but because of the **order of the
//! blocks**. The three levels sum for one pipeline name, and a semistructured pipe is appended
//! before the author's: moving one pipe of a statement pipeline and leaving its two neighbours in
//! Python would reorder the page's blocks, and the order is what the page fixtures freeze. A
//! statement's `pdf_extract` therefore moves whole or not at all, and these two are what the
//! eight measured statements have beside their labels.
//!
//! `joining` is the one knob, and it is the parameter that already existed: a PDF generator splits
//! one centred title into two text objects on the same ordinate when a long dash falls in the
//! middle, and half a name loses every row of that fund (`R24`).

use serde::Deserialize;

use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;
use crate::formats_utils::pdf_extract::standard_funcs::{
    ExtractTextPdfBlockOrFailPage, pdf_extract_currency_standard, pdf_extract_fund_standard,
};

use super::selection::{InputSelection, SelectionConfigError};

/// The YAML configuration of `fund` and of `currency`.
///
/// One required field. `join` is absent in eighteen of the nineteen measured call sites, which is
/// why it is a knob and not a cell every statement has to write.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSelectedText {
    /// Where the datum is written.
    pub selection: InputSelection,
    /// The separator that joins **every** selected line into one block, where the page splits the
    /// datum into several text objects. Absent means the first line alone.
    #[serde(default)]
    pub join: Option<String>,
}

impl InputSelectedText {
    /// The pipe this configuration describes, named and typed by its caller.
    fn build(
        &self,
        factory: fn(PdfLineSelection) -> ExtractTextPdfBlockOrFailPage,
    ) -> Result<ExtractTextPdfBlockOrFailPage, SelectionConfigError> {
        let pipe = factory(self.selection.build("selection")?);
        Ok(match &self.join {
            Some(separator) => pipe.joining(separator.clone()),
            None => pipe,
        })
    }

    /// The pipe reading the sub-fund's name.
    pub fn fund(&self) -> Result<ExtractTextPdfBlockOrFailPage, SelectionConfigError> {
        self.build(pdf_extract_fund_standard)
    }

    /// The pipe reading the currency the statement is expressed in.
    pub fn currency(&self) -> Result<ExtractTextPdfBlockOrFailPage, SelectionConfigError> {
        self.build(pdf_extract_currency_standard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::BlockType;
    use crate::core::pipeline::PdfExtractPipe;

    fn input(yaml: &str) -> InputSelectedText {
        serde_yaml::from_str(yaml).expect("the test's own yaml")
    }

    mod what_each_name_builds {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_fund_pipe_is_called_fund() {
            let pipe = input("selection: 'Arial,Bold ((:)(:60))'").fund().expect("a selection is enough");
            assert_eq!(PdfExtractPipe::name(&pipe), "fund");
        }

        #[test]
        fn the_currency_pipe_is_called_currency() {
            let pipe = input("selection: '\"Controvalore in \"'").currency().expect("a selection is enough");
            assert_eq!(PdfExtractPipe::name(&pipe), "currency");
        }

        #[test]
        fn the_block_types_are_the_two_the_statement_gathers() {
            assert_eq!(BlockType::FUND_NAME.as_str(), "FUND_NAME");
            assert_eq!(BlockType::CURRENCY_STATEMENT.as_str(), "CURRENCY_STATEMENT");
        }
    }

    mod the_one_knob {
        use super::*;

        #[test]
        fn a_separator_is_accepted() {
            input("selection: 'arialmt'\njoin: ' '").fund().expect("a title split in two is one name");
        }

        #[test]
        fn an_unknown_field_is_refused_rather_than_ignored() {
            let error = serde_yaml::from_str::<InputSelectedText>("selection: 'arialmt'\njoin_with: ' '\n")
                .expect_err("a misspelled key must not be dropped in silence");
            assert!(error.to_string().contains("join_with"), "{error}");
        }

        #[test]
        fn a_cell_that_is_not_a_selection_is_refused() {
            let error = input("selection: '^Total'")
                .fund()
                .err()
                .expect("an unquoted anchor is a font name");
            assert!(matches!(error, SelectionConfigError::NotASelection { field: "selection", .. }), "{error}");
        }
    }
}
