//! The YAML surface of `grouped_lines`: *a band of lines, cut into entries by the gap between them*.
//!
//! Three measured pipelines of three formats had written the same twelve lines of Python: select a
//! band, group its lines by vertical distance, and emit either one block per group or one block per
//! line with its group number. See [`PdfExtractGroupedLinesStandard`] for the three and for what
//! distinguishes this from a section.

use serde::Deserialize;

use crate::core::classes::BlockType;
use crate::formats_utils::pdf_extract::grouped_lines::{
    GroupedLinesArgs, GroupedTake, PdfExtractGroupedLinesStandard,
};

use super::selection::{InputSelection, SelectionConfigError};

/// What a grouped-lines configuration can be refused for.
#[derive(Debug, thiserror::Error)]
pub enum GroupedLinesConfigError {
    #[error(transparent)]
    Selection(#[from] SelectionConfigError),
    /// A gap that separates nothing. Refused rather than ignored: a band configured to be cut into
    /// entries and then coming out as one is the kind of defect that shows up as a missing row and
    /// not as an error.
    #[error("field 'gap': must be a positive distance, found {0}")]
    GapNotPositive(f32),
    #[error("field 'take': must be 'all' or 'first', found '{0}'")]
    Take(String),
    /// `take: first` with nothing to join means *the first group, one block per line*, which is
    /// the same as `all` for every caller downstream and almost certainly not what was meant.
    #[error("field 'take': 'first' needs 'join', because one block per line carries its group number already")]
    TakeWithoutJoin,
}

/// The YAML configuration of `grouped_lines`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputGroupedLines {
    /// The band the lines are taken from — a column, or the space between two text anchors.
    pub selection: InputSelection,
    pub block_type: String,
    pub name: String,
    /// The vertical distance at or above which a line starts a **new** entry.
    pub gap: f32,
    /// The separator that makes a group into **one** block. Absent emits one block per line, each
    /// carrying its group number under `group`.
    #[serde(default)]
    pub join: Option<String>,
    /// `all` (the default) or `first`.
    #[serde(default)]
    pub take: Option<String>,
}

impl InputGroupedLines {
    pub fn build(&self) -> Result<PdfExtractGroupedLinesStandard, GroupedLinesConfigError> {
        if self.gap <= 0.0 {
            return Err(GroupedLinesConfigError::GapNotPositive(self.gap));
        }
        let take = match self.take.as_deref() {
            None | Some("all") => GroupedTake::All,
            Some("first") => GroupedTake::First,
            Some(other) => return Err(GroupedLinesConfigError::Take(other.to_string())),
        };
        if take == GroupedTake::First && self.join.is_none() {
            return Err(GroupedLinesConfigError::TakeWithoutJoin);
        }
        Ok(PdfExtractGroupedLinesStandard::new(GroupedLinesArgs {
            join: self.join.clone(),
            take,
            ..GroupedLinesArgs::new(
                self.selection.build("selection")?,
                BlockType::from(self.block_type.clone()),
                self.name.clone(),
                self.gap,
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(yaml: &str) -> InputGroupedLines {
        serde_yaml::from_str(yaml).expect("the test's own yaml")
    }

    fn entry(rest: &str) -> InputGroupedLines {
        input(&format!(
            "selection:\n  font: arialnarrow\n  font_size: [10.9, 11.1]\nblock_type: INV_MAN\nname: inv managers\ngap: 20.0\n{rest}"
        ))
    }

    mod the_required_fields {
        use super::*;

        #[test]
        fn four_of_them_are_enough() {
            entry("").build().expect("a band, a block, a name and a gap");
        }

        #[test]
        fn each_one_is_demanded() {
            for yaml in [
                "block_type: X\nname: n\ngap: 20.0\n",
                "selection: {font: x}\nname: n\ngap: 20.0\n",
                "selection: {font: x}\nblock_type: X\ngap: 20.0\n",
                "selection: {font: x}\nblock_type: X\nname: n\n",
            ] {
                assert!(serde_yaml::from_str::<InputGroupedLines>(yaml).is_err(), "{yaml}");
            }
        }

        #[test]
        fn an_unknown_field_is_refused_rather_than_ignored() {
            let error = serde_yaml::from_str::<InputGroupedLines>(
                "selection: {font: x}\nblock_type: X\nname: n\ngap: 20.0\nthreshold: 20\n",
            )
            .expect_err("a misspelled key must not be dropped in silence");
            assert!(error.to_string().contains("threshold"), "{error}");
        }
    }

    mod the_gap {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn zero_is_refused_rather_than_making_one_entry_of_the_band() {
            let error = entry("").build().err().map(|e| e.to_string());
            assert_eq!(error, None, "twenty is positive");
            let input = input("selection: {font: x}\nblock_type: X\nname: n\ngap: 0.0\n");
            assert_eq!(
                input.build().err().expect("zero separates nothing").to_string(),
                "field 'gap': must be a positive distance, found 0"
            );
        }

        #[test]
        fn a_negative_gap_is_refused_too() {
            assert!(input("selection: {font: x}\nblock_type: X\nname: n\ngap: -3.0\n").build().is_err());
        }
    }

    mod how_the_entries_come_out {
        use super::*;

        #[test]
        fn without_join_one_block_per_line() {
            entry("").build().expect("the group number travels in the metadata");
        }

        #[test]
        fn with_join_one_block_per_group() {
            entry("join: ' '").build().expect("a group is one datum");
        }

        #[test]
        fn take_first_keeps_the_first_group() {
            entry("join: ''\ntake: first").build().expect("the first group alone");
        }

        /// `first` without `join` would mean *the first group, one block per line*, which is what
        /// `all` already gives every caller: the group number is in the metadata either way.
        #[test]
        fn take_first_without_join_is_refused() {
            let error = entry("take: first").build().err().expect("a combination that says nothing");
            assert!(matches!(error, GroupedLinesConfigError::TakeWithoutJoin), "{error}");
        }

        #[test]
        fn an_unknown_take_is_named_in_the_refusal() {
            let error = entry("join: ' '\ntake: last").build().err().expect("'last' is not a word here");
            assert!(error.to_string().contains("'last'"), "{error}");
        }
    }
}
