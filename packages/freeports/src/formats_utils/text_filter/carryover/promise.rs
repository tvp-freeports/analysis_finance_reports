//! `U7` — the datum one page leaves to the next, keyed by the **fund's name**.
//!
//! Six Italian formats write the assets on one page and the liabilities with the net assets on the
//! next. [`FundAssets`](crate::output::classes::fund_assets::FundAssets) carries the three amounts
//! together and checks them together (`R32`), so neither page alone can build the row: the total
//! assets must **cross** the boundary.
//!
//! # Why a promise, and not a promisable field
//!
//! This is Road 1 (`CLUSTERS-format-data-coverage.md` §11), and the reason is not cost. Making the
//! three amounts promisable would move the check of the equation **after** resolution, into
//! `accumulate.rs::resolve`, which runs outside the `job`, `document` and `page` spans: with `R32`
//! an equation that does not add up would come out as an error **without saying in which document
//! and on which page**. Building the entity is the last point at which the diagnostic context still
//! exists, and that is where the check must stay.
//!
//! So the promise is never resolved by the engine: it is only the **vehicle**. The first page
//! deposits it from its `deserialize`; the second finds it in `data.previous()` and makes it a
//! metadatum, and its deserializer builds the entity with all three amounts already concrete.
//!
//! # Why the identifier lives in the `text_filter` tree
//!
//! Pairing two pages by the name of a sub-fund is an **interpretation**, and by `R23` those live in
//! the `text_filter`. The deposit runs in the `deserialize` segment, so
//! [`DeserializeCarryoverDeposit`](crate::formats_utils::deserialize::carryover::DeserializeCarryoverDeposit)
//! imports [`Carryover`] from here instead of defining the key a second time. One identifier, one
//! place: the two pages cannot call it two different things.
//!
//! # The pairing is by name, and the name compares normalised
//!
//! The two pages pair **by the fund's name**, not by the contiguity of their page numbers: the
//! liabilities of one sub-fund may sit ten pages from its assets. The comparison goes through
//! [`MatchFund`], which normalises deeply, so an accent or a double space written differently on
//! the two pages does not lose the row.

use std::collections::BTreeMap;

use crate::core::classes::value::BlockValue;
use crate::core::match_fund::MatchFund;
use crate::core::pipeline::{Extracted, FilterData, PromiseEntries};

/// The fields the first of two pages leaves to the second.
///
/// Built **once per format** and used by both pipelines — the first through the deposit pipe, the
/// second through [`Self::resume`]. That is the point of the design: the promise's identifier is
/// written in one place only.
#[derive(Debug, Clone)]
pub struct Carryover {
    fields: Vec<String>,
    fund_key: String,
    separator: String,
}

/// What a [`Carryover`] refuses at build time.
#[derive(Debug, thiserror::Error)]
pub enum CarryoverError {
    #[error("a carryover with no field would carry nothing")]
    NoField,
    #[error("an empty separator would make the field and the fund unsplittable")]
    EmptySeparator,
}

impl Carryover {
    pub fn new(
        fields: Vec<String>,
        fund_key: String,
        separator: String,
    ) -> Result<Self, CarryoverError> {
        if fields.is_empty() {
            return Err(CarryoverError::NoField);
        }
        if separator.is_empty() {
            return Err(CarryoverError::EmptySeparator);
        }
        Ok(Self {
            fields,
            fund_key,
            separator,
        })
    }

    pub fn fund_key(&self) -> &str {
        &self.fund_key
    }

    /// The identifier, written here and nowhere else.
    ///
    /// It holds **inside a job** and no further: promise resolution is per job, so
    /// `tot_assets@<fund>` cannot collide with another row of the batch file.
    pub fn promise_id(&self, field: &str, fund: &str) -> String {
        format!("{field}{}{fund}", self.separator)
    }

    /// The first page's `deserialize`: what it leaves to the second.
    pub fn deposit(&self, metadata: &BTreeMap<String, BlockValue>) -> PromiseEntries {
        let Some(fund) = metadata
            .get(&self.fund_key)
            .map(super::record::as_text)
            .filter(|f| !f.is_empty())
        else {
            tracing::error!(
                "a block with no '{}' cannot leave anything to the next page: it is dropped",
                self.fund_key
            );
            return PromiseEntries::new();
        };
        let mut entries = PromiseEntries::new();
        for field in &self.fields {
            let Some(value) = metadata.get(field) else {
                tracing::warn!(
                    coord_ref_2 = field.as_str(),
                    "'{field}' is not on this page: the next one will have no {field} for '{fund}'"
                );
                continue;
            };
            entries.push(self.promise_id(field, &fund), value.clone());
        }
        entries
    }

    /// What the preceding pages of this job left: `fund → {field: value}`.
    ///
    /// Keys that do not begin with one of **this** carryover's fields belong to somebody else — a
    /// second datum crossing pages in the same run — and are left alone.
    pub fn promised(
        &self,
        data: &FilterData<'_>,
    ) -> BTreeMap<MatchFund, BTreeMap<String, BlockValue>> {
        let mut out: BTreeMap<MatchFund, BTreeMap<String, BlockValue>> = BTreeMap::new();
        for entries in data.previous().iter().filter_map(Extracted::as_promises) {
            for (key, value) in entries.iter() {
                let Some((field, fund)) = self.split(key) else {
                    continue;
                };
                out.entry(MatchFund::new(fund))
                    .or_default()
                    .insert(field.to_string(), value.clone());
            }
        }
        out
    }

    /// The field and the sub-fund written in an identifier, or `None` if it is not ours.
    ///
    /// Split **by known prefix** and not on the separator, because a sub-fund's name may contain
    /// the separator and a field's name may not.
    fn split<'k>(&self, key: &'k str) -> Option<(&str, &'k str)> {
        self.fields.iter().find_map(|field| {
            let prefix = format!("{field}{}", self.separator);
            key.strip_prefix(&prefix).map(|fund| (field.as_str(), fund))
        })
    }

    /// The last step of the second page's `text_filter`: every block receives in its metadata what
    /// the first page promised for its sub-fund.
    ///
    /// A block whose promise is missing **does not pass**, and says so: having got this far its
    /// sub-fund is one of interest, so either the assets page was never classified or `R24` is
    /// broken and the two pages spell the name differently — and in both cases the missing row is a
    /// datum lost, not a datum that was not there (`R12`).
    ///
    /// # Why it runs last
    ///
    /// Placed after the filter on the funds of interest, every block reaching it speaks of a
    /// sub-fund of interest. Placed before, the same absence would be the ordinary case of a fund
    /// nobody asked about, and the two cases could no longer be told apart.
    pub fn resume(
        &self,
        blocks: Vec<crate::core::classes::TextBlock>,
        data: &FilterData<'_>,
    ) -> Vec<crate::core::classes::TextBlock> {
        let available = self.promised(data);
        let mut out = Vec::with_capacity(blocks.len());
        for mut block in blocks {
            let fund = block
                .metadata
                .get(&self.fund_key)
                .map(super::record::as_text)
                .unwrap_or_default();
            let carried = (!fund.is_empty())
                .then(|| available.get(&MatchFund::new(&fund)))
                .flatten();
            let missing: Vec<&str> = self
                .fields
                .iter()
                .filter(|field| carried.is_none_or(|got| !got.contains_key(field.as_str())))
                .map(String::as_str)
                .collect();
            if !missing.is_empty() {
                tracing::error!(
                    coord_ref_1 = fund.as_str(),
                    "nothing was carried over for '{fund}': {} of this statement {} on the page that \
                     was never read, and the row is dropped",
                    missing.join(", "),
                    if missing.len() == 1 { "stays" } else { "stay" }
                );
                continue;
            }
            // `carried` is `Some` here: an empty `missing` on a `None` would mean a carryover with
            // no fields, which the constructor refuses.
            for (field, value) in
                carried.expect("an empty missing list means the promise was found")
            {
                block.metadata.insert(field.clone(), value.clone());
            }
            out.push(block);
        }
        out
    }
}
