//! The `deserialize` of the **first** of two pages: the promise it leaves to the second.
//!
//! A `deserialize` pipe that returns promises instead of an entity is not a special case — it is
//! what [`Extracted::Promises`](crate::core::pipeline::Extracted::Promises) is for. The first page
//! of a two-page statement of net assets builds no entity, because `FundAssets` needs all three
//! amounts and it has one: what it has is deposited, and the page that completes the statement
//! builds the row.
//!
//! The key is **not** defined here. It comes from
//! [`Carryover`](crate::formats_utils::text_filter::carryover::promise::Carryover), in the
//! `text_filter` tree, because pairing two pages by the name of a sub-fund is an interpretation and
//! `R23` puts interpretations there. One identifier, one place: this pipe and the `text_filter` that
//! resumes cannot spell it differently.

use crate::core::classes::TextBlock;
use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
use crate::formats_utils::text_filter::carryover::promise::Carryover;

/// Deposits what this page promises to a later one.
pub struct DeserializeCarryoverDeposit {
    carry: Carryover,
}

impl DeserializeCarryoverDeposit {
    pub fn new(carry: Carryover) -> Self {
        Self { carry }
    }
}

impl DeserializePipe for DeserializeCarryoverDeposit {
    fn name(&self) -> &str {
        "DeserializeCarryoverDeposit"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let entries = self.carry.deposit(&block.metadata);
        // Nothing promised is nothing to pour: an empty `Promises` would reach the multimap as a
        // merge of no entries, which is the same thing said with one more result to carry.
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![Extracted::Promises(entries)])
    }
}
