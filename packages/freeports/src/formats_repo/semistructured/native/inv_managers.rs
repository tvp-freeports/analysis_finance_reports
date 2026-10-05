//! The YAML configuration of the `inv_managers` `text_filter`, and the filter built from it.
//!
//! One name for one segment: the `pdf_extract` of a managers' section is geometry, and stays
//! wherever the format already had it — `A2` in the structured level for most, an author's pipe for
//! the four pages whose typography says nothing. What is promoted here is the half `R23` assigns to
//! the `text_filter`: reading the declarations off the blocks, and the assignment of the residual.
//!
//! The configuration mirrors
//! [`TextFilterInvManagers`](crate::formats_utils::text_filter::inv_managers::TextFilterInvManagers)
//! field by field, and every field has the default the measured majority wanted, so that a format
//! writes only what distinguishes it. A reader is a one-key map naming its shape — `named: {}`,
//! `adjacent: {…}`, `connector_line: {…}`, `table: {…}` — because a list of them is what UBS-EN23
//! needs, where a list and a prose paragraph declare on the same page.

use serde::Deserialize;

use crate::core::classes::BlockType;
use crate::formats_utils::text_filter::inv_managers::reading::{
    Listing, NOTE_MARKS, Parenthetical, Reader, ReadingError, Spec, Take, Where, compile_all,
};
use crate::formats_utils::text_filter::inv_managers::{
    EmitManco, FundSource, MancoSource, TextFilterInvManagers,
};

/// A block type written by name, which is how a format names its own.
fn block_type(name: &str) -> BlockType {
    BlockType::from(name.to_string())
}

fn note_marks(given: &Option<String>) -> String {
    given.clone().unwrap_or_else(|| NOTE_MARKS.to_string())
}

/// The bracketed specification. See [`Parenthetical`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InputParenthetical {
    pub markers: Vec<String>,
    pub partial_markers: Vec<String>,
    /// Left out means `[",", " and "]`, which is how the measured majority writes a list.
    pub separators: Option<Vec<String>>,
    pub opener: Option<String>,
    pub closer: Option<String>,
    pub block_type: Option<String>,
    pub umbrella: Option<String>,
    pub note_marks: Option<String>,
}

impl InputParenthetical {
    fn build(&self) -> Parenthetical {
        let default = Parenthetical::default();
        Parenthetical {
            markers: self.markers.clone(),
            partial_markers: self.partial_markers.clone(),
            separators: self.separators.clone().unwrap_or(default.separators),
            opener: self.opener.clone().unwrap_or(default.opener),
            closer: self.closer.clone().unwrap_or(default.closer),
            block_type: self.block_type.as_deref().map(block_type),
            umbrella: self.umbrella.clone(),
            note_marks: note_marks(&self.note_marks),
        }
    }
}

/// The specification as a list, one sub-fund per line. See [`Listing`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InputListing {
    pub block_type: Option<String>,
    /// Left out means the en dash; `""` says that **every** line is an item.
    pub item_prefix: Option<String>,
    pub umbrella: Option<String>,
    pub strip: Vec<String>,
    pub note_marks: Option<String>,
}

impl InputListing {
    fn build(&self) -> Result<Listing, ReadingError> {
        let default = Listing::default();
        Ok(Listing {
            block_type: self.block_type.as_deref().map(block_type),
            item_prefix: self.item_prefix.clone().unwrap_or(default.item_prefix),
            umbrella: self.umbrella.clone(),
            strip: compile_all("strip", &self.strip)?,
            note_marks: note_marks(&self.note_marks),
        })
    }
}

/// Which of the two shapes a specification comes in: exactly one of the two keys.
///
/// Two optional keys rather than a tagged enum because that is how it reads in a configuration
/// file — `spec: {listing: {umbrella: …}}` — and because naming neither, or both, is a mistake worth
/// a sentence of its own.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InputSpec {
    pub parenthetical: Option<InputParenthetical>,
    pub listing: Option<InputListing>,
}

impl InputSpec {
    fn build(&self) -> Result<Spec, InvManagersError> {
        match (&self.parenthetical, &self.listing) {
            (Some(spec), None) => Ok(Spec::Parenthetical(spec.build())),
            (None, Some(spec)) => Ok(Spec::Listing(spec.build()?)),
            (Some(_), Some(_)) => Err(InvManagersError::UnknownChoice {
                field: "spec",
                given: "both".to_string(),
                expected: "'parenthetical' or 'listing', one of the two",
            }),
            (None, None) => Err(InvManagersError::UnknownChoice {
                field: "spec",
                given: "nothing".to_string(),
                expected: "'parenthetical' or 'listing'",
            }),
        }
    }
}

/// One reader, named by `kind`: the shape the page writes its declarations in.
///
/// Tagged **inside** the map rather than wrapping it, so that a reader is a flat block of YAML with
/// a `kind` line at the top. Each shape refuses a field it does not know, which is what catches a
/// misspelt one instead of silently taking the default.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
// The four shapes differ in size by design — `adjacent` carries a whole specification, `named`
// three lists — and this enum is built once per repository at start-up, from a configuration file.
// Boxing the largest variant would save bytes nobody is counting and cost the configuration its
// flat, readable shape.
#[allow(clippy::large_enum_variant)]
pub enum InputReader {
    /// The block **is** the declaration: a name, and no sub-fund said.
    Named {
        #[serde(default)]
        separators: Vec<String>,
        #[serde(default)]
        note_marks: Option<String>,
        #[serde(default)]
        group_key: Option<String>,
        /// The expressions that say the name goes on onto the next line of the same entry.
        #[serde(default)]
        wrapped: Vec<String>,
        #[serde(default)]
        retired: Vec<String>,
    },
    /// The specification sits next to the name. See [`Reader::Adjacent`].
    Adjacent {
        spec: InputSpec,
        /// `below` (the default) or `above`.
        #[serde(default)]
        r#where: Option<String>,
        /// `all` (the default) or `first`.
        #[serde(default)]
        take: Option<String>,
        #[serde(default)]
        group_key: Option<String>,
        #[serde(default)]
        strip: Vec<String>,
        #[serde(default)]
        note_type: Option<String>,
        #[serde(default)]
        retired: Vec<String>,
    },
    /// `<manager> for <sub-funds>`. See [`Reader::ConnectorLine`].
    ConnectorLine {
        #[serde(default)]
        connector: Option<String>,
        #[serde(default)]
        after: Option<String>,
        #[serde(default)]
        manager_separators: Option<Vec<String>>,
        #[serde(default)]
        fund_separators: Option<Vec<String>>,
        #[serde(default)]
        terminators: Option<String>,
        #[serde(default)]
        group_key: Option<String>,
        #[serde(default)]
        note_marks: Option<String>,
    },
    /// One row per sub-fund. See [`Reader::Table`].
    Table {
        fund_col: i64,
        /// Left out means column one, which is the shape with the manager next to the fund.
        #[serde(default)]
        manager_col: Option<i64>,
        #[serde(default)]
        row_key: Option<String>,
        #[serde(default)]
        col_key: Option<String>,
        #[serde(default)]
        note_marks: Option<String>,
    },
}

/// Failures of building the filter out of its configuration.
#[derive(Debug, thiserror::Error)]
pub enum InvManagersError {
    #[error(transparent)]
    Reading(#[from] ReadingError),
    #[error("'{field}' is {given:?}: it is one of {expected}")]
    UnknownChoice { field: &'static str, given: String, expected: &'static str },
}

fn choice<T: Copy>(
    field: &'static str,
    given: &Option<String>,
    expected: &'static str,
    table: &[(&str, T)],
    default: T,
) -> Result<T, InvManagersError> {
    let Some(given) = given.as_deref() else { return Ok(default) };
    table
        .iter()
        .find(|(name, _)| *name == given)
        .map(|(_, value)| *value)
        .ok_or_else(|| InvManagersError::UnknownChoice {
            field,
            given: given.to_string(),
            expected,
        })
}

impl InputReader {
    fn build(&self) -> Result<Reader, InvManagersError> {
        Ok(match self {
            InputReader::Named { separators, note_marks: marks, group_key, wrapped, retired } => {
                Reader::Named {
                    separators: separators.clone(),
                    note_marks: note_marks(marks),
                    group_key: group_key.clone().unwrap_or_else(|| "group".to_string()),
                    wrapped: compile_all("wrapped", wrapped)?,
                    retired: compile_all("retired", retired)?,
                }
            }
            InputReader::Adjacent {
                spec,
                r#where,
                take,
                group_key,
                strip,
                note_type,
                retired,
            } => Reader::Adjacent {
                spec: spec.build()?,
                where_: choice(
                    "where",
                    r#where,
                    "'below' or 'above'",
                    &[("below", Where::Below), ("above", Where::Above)],
                    Where::Below,
                )?,
                take: choice(
                    "take",
                    take,
                    "'all' or 'first'",
                    &[("all", Take::All), ("first", Take::First)],
                    Take::All,
                )?,
                group_key: group_key.clone().unwrap_or_else(|| "group".to_string()),
                strip: compile_all("strip", strip)?,
                note_type: note_type.as_deref().map(block_type),
                retired: compile_all("retired", retired)?,
            },
            InputReader::ConnectorLine {
                connector,
                after,
                manager_separators,
                fund_separators,
                terminators,
                group_key,
                note_marks: marks,
            } => Reader::ConnectorLine {
                connector: connector.clone().unwrap_or_else(|| " for ".to_string()),
                after: after.clone().unwrap_or_default(),
                manager_separators: manager_separators
                    .clone()
                    .unwrap_or_else(|| vec![" and ".to_string()]),
                fund_separators: fund_separators
                    .clone()
                    .unwrap_or_else(|| vec![",".to_string(), " and ".to_string()]),
                terminators: terminators.clone().unwrap_or_else(|| ",.;".to_string()),
                group_key: group_key.clone().unwrap_or_else(|| "group".to_string()),
                note_marks: note_marks(marks),
            },
            InputReader::Table { fund_col, manager_col, row_key, col_key, note_marks: marks } => {
                Reader::Table {
                    fund_col: *fund_col,
                    manager_col: manager_col.unwrap_or(1),
                    row_key: row_key.clone().unwrap_or_else(|| "table-row".to_string()),
                    col_key: col_key.clone().unwrap_or_else(|| "table-col".to_string()),
                    note_marks: note_marks(marks),
                }
            }
        })
    }
}

/// The whole `inv_managers` entry.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InputInvManagers {
    /// The readers, in the order they are applied to the same blocks. An empty list is the page that
    /// only closes the account for the management company.
    pub read: Vec<InputReader>,
    /// Which blocks the readers see. Left out means `INVESTMENTS_MANAGER`, and `"*"` means **every**
    /// block of the pipeline — which is what a format needs whose `pdf_extract` writes block types
    /// of its own, one for the name and one for the specification.
    pub read_block_type: Option<String>,
    /// `investments` (the default, which is `R24`) or `funds`.
    pub funds: Option<String>,
    /// `none` (the default), `blocks` or `previous_result`.
    pub manco: Option<String>,
    /// The type of the block carrying the management company, when `manco` is `blocks`.
    pub manco_block_type: Option<String>,
    /// `never` (the default), `when_present` or `on_new_funds`.
    pub emit_manco: Option<String>,
    pub manco_first: bool,
    /// Left out means `true`: the common page is the one that closes the account.
    pub settle: Option<bool>,
    pub already_assigned: bool,
    pub funds_include_assigned: bool,
    pub notes: Vec<String>,
}

/// The `read_block_type` that means "every block", written as a value because YAML's `null` cannot be
/// told apart from a field left out.
pub const EVERY_BLOCK: &str = "*";

/// Builds the filter the configuration describes.
pub fn text_filter(input: &InputInvManagers) -> Result<TextFilterInvManagers, InvManagersError> {
    let mut readers = Vec::with_capacity(input.read.len());
    for reader in &input.read {
        readers.push(reader.build()?);
    }

    let manco = match input.manco.as_deref().unwrap_or("none") {
        "none" => MancoSource::None,
        "blocks" => MancoSource::Blocks(match input.manco_block_type.as_deref() {
            Some(name) => block_type(name),
            None => BlockType::MANAGEMENT_COMPANY,
        }),
        "previous_result" => MancoSource::PreviousResult,
        given => {
            return Err(InvManagersError::UnknownChoice {
                field: "manco",
                given: given.to_string(),
                expected: "'none', 'blocks' or 'previous_result'",
            });
        }
    };

    let mut filter = TextFilterInvManagers::new(readers);
    filter.read_block_type = match input.read_block_type.as_deref() {
        None => Some(BlockType::INVESTMENTS_MANAGER),
        Some(EVERY_BLOCK) => None,
        Some(name) => Some(block_type(name)),
    };
    filter.funds = choice(
        "funds",
        &input.funds,
        "'investments' or 'funds'",
        &[("investments", FundSource::Investments), ("funds", FundSource::Funds)],
        FundSource::Investments,
    )?;
    filter.manco = manco;
    filter.emit_manco = choice(
        "emit_manco",
        &input.emit_manco,
        "'never', 'when_present' or 'on_new_funds'",
        &[
            ("never", EmitManco::Never),
            ("when_present", EmitManco::WhenPresent),
            ("on_new_funds", EmitManco::OnNewFunds),
        ],
        EmitManco::Never,
    )?;
    filter.manco_first = input.manco_first;
    filter.settle = input.settle.unwrap_or(true);
    filter.already_assigned = input.already_assigned;
    filter.funds_include_assigned = input.funds_include_assigned;
    filter.notes = input.notes.clone();
    Ok(filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::text_filter::inv_managers::reading::{Spec, Take, Where};

    fn built(yaml: &str) -> TextFilterInvManagers {
        let input: InputInvManagers =
            serde_yaml::from_str(yaml).unwrap_or_else(|e| panic!("the YAML does not parse: {e}"));
        text_filter(&input).unwrap_or_else(|e| panic!("the configuration does not build: {e}"))
    }

    fn rejected(yaml: &str) -> String {
        let input: InputInvManagers =
            serde_yaml::from_str(yaml).unwrap_or_else(|e| panic!("the YAML does not parse: {e}"));
        text_filter(&input).err().expect("this configuration is wrong").to_string()
    }

    mod the_defaults {
        use super::*;

        #[test]
        fn an_entry_that_says_nothing_is_the_simplest_measured_page() {
            let got = built("{}");
            assert!(got.readers.is_empty());
            assert_eq!(got.read_block_type, Some(BlockType::INVESTMENTS_MANAGER));
            assert_eq!(got.funds, FundSource::Investments);
            assert_eq!(got.manco, MancoSource::None);
            assert_eq!(got.emit_manco, EmitManco::Never);
            assert!(got.settle);
            assert!(!got.already_assigned);
            assert!(!got.funds_include_assigned);
            assert!(!got.manco_first);
            assert!(got.notes.is_empty());
        }

        #[test]
        fn a_reader_with_no_fields_is_the_name_alone() {
            let got = built("read:\n  - kind: named\n");
            assert!(matches!(got.readers[0], Reader::Named { .. }));
        }
    }

    mod the_four_reader_shapes {
        use super::*;

        #[test]
        fn adjacent_with_a_bracket_below_is_the_common_one() {
            let got = built(
                "read:\n  - kind: adjacent\n    spec:\n      parenthetical:\n        markers: [\"in respect of \"]\n",
            );
            let Reader::Adjacent { spec, where_, take, .. } = &got.readers[0] else {
                panic!("not an Adjacent: {:?}", got.readers[0])
            };
            assert!(matches!(spec, Spec::Parenthetical(_)));
            assert_eq!(*where_, Where::Below);
            assert_eq!(*take, Take::All);
        }

        #[test]
        fn adjacent_above_taking_the_first_is_ubs() {
            let got = built(
                "read:\n  - kind: adjacent\n    spec:\n      listing:\n        umbrella: \"BPER International SICAV\"\n    where: above\n    take: first\n    strip: [\",.*$\"]\n",
            );
            let Reader::Adjacent { spec, where_, take, strip, .. } = &got.readers[0] else {
                panic!("not an Adjacent")
            };
            assert!(matches!(spec, Spec::Listing(_)));
            assert_eq!(*where_, Where::Above);
            assert_eq!(*take, Take::First);
            assert_eq!(strip.len(), 1);
        }

        #[test]
        fn a_connector_line_keeps_the_defaults_it_does_not_mention() {
            let got = built(
                "read:\n  - kind: connector_line\n    after: \"The Management Company delegated to \"\n",
            );
            let Reader::ConnectorLine { connector, after, fund_separators, .. } = &got.readers[0]
            else {
                panic!("not a ConnectorLine")
            };
            assert_eq!(connector, " for ");
            assert_eq!(after, "The Management Company delegated to ");
            assert_eq!(fund_separators, &[",".to_string(), " and ".to_string()]);
        }

        #[test]
        fn a_table_says_only_which_columns_matter() {
            let got = built("read:\n  - kind: table\n    fund_col: 0\n    manager_col: 2\n");
            let Reader::Table { fund_col, manager_col, row_key, .. } = &got.readers[0] else {
                panic!("not a Table")
            };
            assert_eq!((*fund_col, *manager_col), (0, 2));
            assert_eq!(row_key, "table-row");
        }

        #[test]
        fn two_readers_are_two_readers() {
            let got = built("read:\n  - kind: named\n  - kind: connector_line\n");
            assert_eq!(got.readers.len(), 2);
        }
    }

    mod the_block_types {
        use super::*;

        #[test]
        fn a_format_names_its_own() {
            let got = built("read_block_type: INV_MAN\n");
            assert_eq!(got.read_block_type, Some(BlockType::from("INV_MAN".to_string())));
        }

        #[test]
        fn the_star_means_every_block_of_the_pipeline() {
            let got = built("read_block_type: \"*\"\n");
            assert_eq!(got.read_block_type, None);
        }

        #[test]
        fn the_management_companys_type_defaults_to_the_standard_one() {
            let got = built("manco: blocks\n");
            assert_eq!(got.manco, MancoSource::Blocks(BlockType::MANAGEMENT_COMPANY));
        }

        #[test]
        fn and_can_be_the_formats_own_too() {
            let got = built("manco: blocks\nmanco_block_type: MANCO\n");
            assert_eq!(got.manco, MancoSource::Blocks(BlockType::from("MANCO".to_string())));
        }
    }

    mod what_a_wrong_entry_says {
        use super::*;

        #[test]
        fn an_unknown_choice_lists_the_ones_there_are() {
            let message = rejected("funds: holdings\n");
            assert!(message.contains("'investments' or 'funds'"), "{message}");
            assert!(message.contains("holdings"), "{message}");
        }

        #[test]
        fn an_unknown_management_company_source_does_too() {
            let message = rejected("manco: page\n");
            assert!(message.contains("'none', 'blocks' or 'previous_result'"), "{message}");
        }

        #[test]
        fn an_unknown_where_names_the_field() {
            let message = rejected(
                "read:\n  - kind: adjacent\n    spec:\n      listing: {}\n    where: beside\n",
            );
            assert!(message.contains("'where'"), "{message}");
        }

        #[test]
        fn a_malformed_expression_names_the_field_it_was_written_in() {
            let message = rejected(
                "read:\n  - kind: adjacent\n    spec:\n      listing: {}\n    strip: [\"(\"]\n",
            );
            assert!(message.contains("'strip'"), "{message}");
        }

        #[test]
        fn a_specification_naming_neither_shape_is_refused() {
            let message = rejected("read:\n  - kind: adjacent\n    spec: {}\n");
            assert!(message.contains("'parenthetical' or 'listing'"), "{message}");
        }

        #[test]
        fn a_specification_naming_both_is_refused_too() {
            let message = rejected(
                "read:\n  - kind: adjacent\n    spec:\n      listing: {}\n      parenthetical: {}\n",
            );
            assert!(message.contains("one of the two"), "{message}");
        }

        #[test]
        fn an_unknown_reader_kind_is_refused_by_name() {
            let error = serde_yaml::from_str::<InputInvManagers>("read:\n  - kind: sideways\n")
                .expect_err("an unknown kind is refused");
            assert!(error.to_string().contains("sideways"), "{error}");
        }

        #[test]
        fn an_unknown_field_of_a_reader_is_a_mistake_and_not_a_silence() {
            let error =
                serde_yaml::from_str::<InputInvManagers>("read:\n  - kind: named\n    separatos: []\n")
                    .expect_err("an unknown field is refused");
            assert!(error.to_string().contains("separatos"), "{error}");
        }

        #[test]
        fn an_unknown_field_is_a_mistake_and_not_a_silence() {
            let error = serde_yaml::from_str::<InputInvManagers>("settl: false\n")
                .expect_err("an unknown field is refused");
            assert!(error.to_string().contains("settl"), "{error}");
        }
    }

    mod the_rest_of_the_knobs {
        use super::*;

        #[test]
        fn every_one_of_them_arrives_where_it_was_written() {
            let got = built(
                "funds: funds\nmanco: previous_result\nemit_manco: on_new_funds\n\
                 manco_first: true\nsettle: false\nalready_assigned: true\n\
                 funds_include_assigned: true\nnotes: [\"a note\"]\n",
            );
            assert_eq!(got.funds, FundSource::Funds);
            assert_eq!(got.manco, MancoSource::PreviousResult);
            assert_eq!(got.emit_manco, EmitManco::OnNewFunds);
            assert!(got.manco_first);
            assert!(!got.settle);
            assert!(got.already_assigned);
            assert!(got.funds_include_assigned);
            assert_eq!(got.notes, ["a note"]);
        }
    }
}
