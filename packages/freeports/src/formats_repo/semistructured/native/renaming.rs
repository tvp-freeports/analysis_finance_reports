//! The YAML surface of `A7`: the renamings and mergers of a sub-fund.
//!
//! Two names come from here — `renaming` in the `text_filter` and `renaming` in the `deserialize`,
//! the same word in two segments because it is one algorithm in two halves.
//!
//! The `text_filter` has three cells: **which reader**, the two lists of patterns, and **which of
//! the two names** has to be a sub-fund of the job. The `deserialize` has one: the language the
//! document writes its months in, which is the whole of the casting `R23` leaves it.

use std::sync::Arc;

use serde::Deserialize;

use crate::formats_utils::deserialize::renaming::DeserializerRenaming;
use crate::formats_utils::text_filter::renaming::{
    DATE, DATE_INTRO, NOTE_MARKS, OfInterest, Reader, RenamingError, TextFilterRenaming,
};

use super::super::super::structured::fund_assets::DateCast;

/// What a renaming configuration can be refused for.
#[derive(Debug, thiserror::Error)]
pub enum RenamingConfigError {
    #[error(transparent)]
    Reader(#[from] RenamingError),
    #[error("'{0}' is not a date cast this level knows: {1}")]
    Cast(String, String),
    #[error("'{0}' is not a name this level knows: they are current, old, any")]
    OfInterest(String),
}

fn date() -> String {
    DATE.to_string()
}

fn date_intro() -> String {
    DATE_INTRO.to_string()
}

fn note_marks() -> String {
    NOTE_MARKS.to_string()
}

fn connector() -> String {
    " into ".to_string()
}

fn group_key() -> String {
    "group".to_string()
}

/// Which of the two shapes the page writes its changes in.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[allow(clippy::large_enum_variant)]
pub enum InputReader {
    /// A dated heading opens a list. AMUNDI-EN24.
    Listed {
        #[serde(default)]
        renamed: Vec<String>,
        #[serde(default)]
        merged: Vec<String>,
        #[serde(default = "connector")]
        connector: String,
        #[serde(default = "date")]
        date: String,
        #[serde(default = "date_intro")]
        date_intro: String,
        #[serde(default)]
        strip: Vec<String>,
        #[serde(default = "note_marks")]
        note_marks: String,
    },
    /// A sentence per event, with the three named groups `old`, `current` and `when`. ARCA-IT24 and
    /// DANSKEINVEST-EN24.
    Sentence {
        #[serde(default)]
        renamed: Vec<String>,
        #[serde(default)]
        merged: Vec<String>,
        #[serde(default = "group_key")]
        group_key: String,
        #[serde(default)]
        strip: Vec<String>,
        #[serde(default = "note_marks")]
        note_marks: String,
    },
}

/// The `text_filter` of a renamings pipeline.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRenaming {
    pub read: InputReader,
    /// `current` (the default), `old` or `any`. See
    /// [`OfInterest`](crate::formats_utils::text_filter::renaming::OfInterest).
    pub of_interest: Option<String>,
}

/// The `deserialize` of the same algorithm: the language of the months.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRenamingDeserialize {
    pub date_cast: Option<String>,
}

/// Builds the `renaming` `text_filter`.
pub fn text_filter(input: &InputRenaming) -> Result<TextFilterRenaming, RenamingConfigError> {
    let reader = match &input.read {
        InputReader::Listed { renamed, merged, connector, date, date_intro, strip, note_marks } => {
            Reader::listed(
                renamed,
                merged,
                connector.clone(),
                date,
                date_intro,
                strip,
                note_marks.clone(),
            )?
        }
        InputReader::Sentence { renamed, merged, group_key, strip, note_marks } => {
            Reader::sentence(renamed, merged, group_key.clone(), strip, note_marks.clone())?
        }
    };
    let of_interest = match input.of_interest.as_deref() {
        None | Some("current") => OfInterest::Current,
        Some("old") => OfInterest::Old,
        Some("any") => OfInterest::Any,
        Some(other) => return Err(RenamingConfigError::OfInterest(other.to_string())),
    };
    Ok(TextFilterRenaming::new(reader, of_interest))
}

/// Builds the `renaming` deserializer.
///
/// The date cast is the structured level's vocabulary, the same one `assets` uses: `en_month` is
/// the default here because that is what the template's own default was, and three of the four
/// measured sections are in English.
pub fn deserialize(
    input: &InputRenamingDeserialize,
) -> Result<DeserializerRenaming, RenamingConfigError> {
    let cast: DateCast = match &input.date_cast {
        None => DateCast::EnMonth,
        Some(name) => name.parse().map_err(|message| RenamingConfigError::Cast(name.clone(), message))?,
    };
    Ok(DeserializerRenaming::new(Arc::new(cast.converter())))
}
