//! The YAML surface of `U7`: the statement written as labels, and what one page leaves to the next.
//!
//! Three names come from here, and they are three because they sit in two different segments and
//! one pipeline uses two of them at once:
//!
//! - `labelled_assets` (`text_filter`) — gather the page's labels into one statement, filter it
//!   against the funds of interest, and resume what the page before promised;
//! - `carryover_deposit` (`deserialize`) — the **first** of two pages, which builds no entity and
//!   deposits instead;
//! - `assets` (`deserialize`) — the ordinary builder of a `FundAssets`, named here for the first
//!   time because the structured level reached it through its own table and the semistructured one
//!   never needed it before.
//!
//! # The casts are the structured level's, on purpose
//!
//! `amount_cast` and `date_cast` reuse [`AmountCast`] and [`DateCast`] from
//! [`super::super::super::structured::fund_assets`], word for word. A second vocabulary for the
//! same two conversions would be two places to add `es_month` to the next time a format writes its
//! dates in a new language — which is exactly what happened here.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::core::classes::BlockType;
use crate::formats_utils::deserialize::carryover::DeserializeCarryoverDeposit;
use crate::formats_utils::deserialize::standard_funcs::DeserializerAssetsStandard;
use crate::formats_utils::text_filter::carryover::page::{PageCarryover, PageCarryoverError};
use crate::formats_utils::text_filter::carryover::promise::{Carryover, CarryoverError};
use crate::formats_utils::text_filter::carryover::record::{Record, RecordError};
use crate::formats_utils::text_filter::carryover::{FundFrom, TextFilterLabelledAssets};
use crate::formats_utils::text_filter::standard_funcs::{StandardFuncsError, TextFilterAssetsStandard};

use super::super::super::structured::fund_assets::{AmountCast, DateCast};

/// What a carryover configuration can be refused for.
#[derive(Debug, thiserror::Error)]
pub enum CarryoverConfigError {
    #[error(transparent)]
    Record(#[from] RecordError),
    #[error(transparent)]
    Carryover(#[from] CarryoverError),
    #[error(transparent)]
    Page(#[from] PageCarryoverError),
    #[error(transparent)]
    Assets(#[from] StandardFuncsError),
    #[error("'{0}' is not an amount or date cast this level knows: {1}")]
    Cast(String, String),
}

/// Where the sub-fund's name comes from, when the page does not simply write it.
///
/// Absent means the page writes it, which is seven of the nine measured pipelines.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputFundFrom {
    /// Ask the document for its one fund, **only** where the page carries none of its own (`C23`).
    SoleIfAbsent,
    /// Take it from the nearest page before this one that left one. UBS-EN23 only.
    NearestPage {
        /// The promise's field name, which has to match the depositing pipeline's.
        field: String,
        /// The block whose content is this page's printed number.
        page_block_type: String,
        #[serde(default = "page_separator")]
        separator: String,
    },
}

fn page_separator() -> String {
    "-page-".to_string()
}

fn fund_separator() -> String {
    "@".to_string()
}

fn fund_key() -> String {
    "fund".to_string()
}

/// The `text_filter` of a statement written as labels.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputLabelledAssets {
    /// Metadata key → the block type that fills it. The one required field.
    pub gather: BTreeMap<String, String>,
    /// The type of the gathered block. `RELEVANT_BLOCK` unless a format says otherwise.
    pub block_type: Option<String>,
    pub fund: Option<InputFundFrom>,
    #[serde(default = "fund_key")]
    pub fund_key: String,
    /// `TextFilterAssetsStandard`'s three parameters, with the names the structured table uses.
    pub date_regex: Option<String>,
    #[serde(default)]
    pub note_marks: String,
    #[serde(default)]
    pub remove_from_fund: Vec<String>,
    /// The fields the **page before** promised, which this page resumes. Absent means this page is
    /// not the second of two.
    #[serde(default)]
    pub resume: Vec<String>,
    #[serde(default = "fund_separator")]
    pub separator: String,
}

/// The `deserialize` of the first of two pages.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputCarryoverDeposit {
    /// The fields this page promises to the next.
    pub fields: Vec<String>,
    #[serde(default = "fund_key")]
    pub fund_key: String,
    #[serde(default = "fund_separator")]
    pub separator: String,
}

/// The `deserialize` that builds the `FundAssets`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputAssets {
    pub amount_cast: Option<String>,
    pub date_cast: Option<String>,
}

fn record_of(input: &InputLabelledAssets) -> Result<Record, CarryoverConfigError> {
    let fields = input
        .gather
        .iter()
        .map(|(key, block_type)| (key.clone(), BlockType::from(block_type.clone())))
        .collect();
    let block_type = input
        .block_type
        .clone()
        .map(BlockType::from)
        .unwrap_or(BlockType::RELEVANT_BLOCK);
    Ok(Record::new(block_type, fields)?)
}

fn fund_from(input: &InputLabelledAssets) -> Result<FundFrom, CarryoverConfigError> {
    Ok(match &input.fund {
        None => FundFrom::Page,
        Some(InputFundFrom::SoleIfAbsent) => FundFrom::SoleIfAbsent,
        Some(InputFundFrom::NearestPage { field, page_block_type, separator }) => FundFrom::NearestPage {
            carry: PageCarryover::new(field.clone(), separator.clone())?,
            page_block_type: BlockType::from(page_block_type.clone()),
        },
    })
}

/// Builds the `labelled_assets` pipe.
pub fn text_filter(input: &InputLabelledAssets) -> Result<TextFilterLabelledAssets, CarryoverConfigError> {
    let assets = TextFilterAssetsStandard::new(input.date_regex.as_deref(), input.remove_from_fund.clone())?
        .with_note_marks(input.note_marks.clone());
    let resume = if input.resume.is_empty() {
        None
    } else {
        Some(Carryover::new(input.resume.clone(), input.fund_key.clone(), input.separator.clone())?)
    };
    Ok(TextFilterLabelledAssets::new(
        record_of(input)?,
        fund_from(input)?,
        input.fund_key.clone(),
        assets,
        resume,
    ))
}

/// Builds the `carryover_deposit` pipe.
pub fn deposit(input: &InputCarryoverDeposit) -> Result<DeserializeCarryoverDeposit, CarryoverConfigError> {
    let carry = Carryover::new(input.fields.clone(), input.fund_key.clone(), input.separator.clone())?;
    Ok(DeserializeCarryoverDeposit::new(carry))
}

/// Builds the `assets` deserializer.
pub fn assets(input: &InputAssets) -> Result<DeserializerAssetsStandard, CarryoverConfigError> {
    let amount: AmountCast = match &input.amount_cast {
        None => AmountCast::default(),
        Some(name) => name.parse().map_err(|message| CarryoverConfigError::Cast(name.clone(), message))?,
    };
    let date: DateCast = match &input.date_cast {
        None => DateCast::default(),
        Some(name) => name.parse().map_err(|message| CarryoverConfigError::Cast(name.clone(), message))?,
    };
    Ok(DeserializerAssetsStandard::with_converters(
        amount.converter(),
        std::sync::Arc::new(date.converter()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(yaml: &str) -> InputLabelledAssets {
        serde_yaml::from_str(yaml).expect("a configuration these tests spell right")
    }

    mod the_gathering {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The shortest configuration that works: ARCA-IT24's first page, which is also eight
        /// formats' shape.
        #[test]
        fn the_fields_are_the_only_thing_a_statement_must_say() {
            let input = parsed(
                "gather:\n  fund: FUND_NAME\n  currency: CURRENCY_STATEMENT\n  tot_assets: TOT_ASSETS\n",
            );
            assert_eq!(input.gather.len(), 3);
            assert!(input.block_type.is_none());
            assert!(input.fund.is_none());
            assert_eq!(input.fund_key, "fund");
            assert!(input.resume.is_empty());
            assert!(text_filter(&input).is_ok());
        }

        #[test]
        fn a_record_with_no_field_is_refused_when_the_repository_loads() {
            let input = parsed("gather: {}\n");
            assert!(matches!(text_filter(&input), Err(CarryoverConfigError::Record(_))));
        }

        #[test]
        fn an_unknown_cell_is_refused_rather_than_ignored() {
            let refused: Result<InputLabelledAssets, _> =
                serde_yaml::from_str("gather:\n  fund: FUND_NAME\ngatherr: 1\n");
            assert!(refused.is_err());
        }
    }

    mod where_the_fund_comes_from {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn absent_means_the_page_writes_it() {
            let input = parsed("gather:\n  fund: FUND_NAME\n");
            assert!(matches!(fund_from(&input).unwrap(), FundFrom::Page));
        }

        #[test]
        fn sole_if_absent_asks_the_document_only_when_the_page_is_silent() {
            let input = parsed("gather:\n  fund: FUND_NAME\nfund:\n  kind: sole_if_absent\n");
            assert!(matches!(fund_from(&input).unwrap(), FundFrom::SoleIfAbsent));
        }

        #[test]
        fn nearest_page_carries_the_field_and_the_block_that_numbers_the_page() {
            let input = parsed(
                "gather:\n  currency: CURRENCY_STATEMENT\n\
                 fund:\n  kind: nearest_page\n  field: ubs-fund\n  page_block_type: ASSETS_PAGE\n",
            );
            match fund_from(&input).unwrap() {
                FundFrom::NearestPage { carry, page_block_type } => {
                    assert_eq!(carry.promise_id(14), "ubs-fund-page-14");
                    assert_eq!(page_block_type, BlockType::from("ASSETS_PAGE".to_string()));
                }
                other => panic!("{other:?}"),
            }
        }
    }

    mod the_promise {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn resume_names_the_fields_the_page_before_promised() {
            let input = parsed("gather:\n  fund: FUND_NAME\nresume:\n  - tot_assets\n");
            assert_eq!(input.resume, vec!["tot_assets".to_string()]);
            assert!(text_filter(&input).is_ok());
        }

        /// The identifier is written once: the depositing cell and the resuming cell name the same
        /// field and the same separator, and neither composes the key itself.
        #[test]
        fn the_deposit_and_the_resume_agree_on_the_identifier() {
            let deposited: InputCarryoverDeposit =
                serde_yaml::from_str("fields:\n  - tot_assets\n").unwrap();
            assert_eq!(deposited.fund_key, "fund");
            assert_eq!(deposited.separator, "@");
            assert!(deposit(&deposited).is_ok());
        }

        #[test]
        fn a_deposit_with_no_field_is_refused_when_the_repository_loads() {
            let deposited: InputCarryoverDeposit = serde_yaml::from_str("fields: []\n").unwrap();
            assert!(matches!(deposit(&deposited), Err(CarryoverConfigError::Carryover(_))));
        }
    }

    mod the_casts {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_empty_configuration_is_the_engines_own_choice() {
            assert!(assets(&InputAssets::default()).is_ok());
        }

        /// The two the measured formats need beyond the default: UBS-EN23's sign, which is
        /// typography (`R31`), and MEDIOLANUM-ES24.A's Spanish month.
        #[test]
        fn the_two_the_measured_formats_name() {
            let magnitude: InputAssets = serde_yaml::from_str("amount_cast: float + magnitude\n").unwrap();
            assert!(assets(&magnitude).is_ok());
            let spanish: InputAssets = serde_yaml::from_str("date_cast: es_month\n").unwrap();
            assert!(assets(&spanish).is_ok());
        }

        #[test]
        fn a_cast_nobody_knows_is_refused_by_name() {
            let unknown: InputAssets = serde_yaml::from_str("date_cast: fr_month\n").unwrap();
            match assets(&unknown).err() {
                Some(CarryoverConfigError::Cast(name, _)) => assert_eq!(name, "fr_month"),
                other => panic!("a cast nobody knows must be refused by name, got {other:?}"),
            }
        }
    }
}
