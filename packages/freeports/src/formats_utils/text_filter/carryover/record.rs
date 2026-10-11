//! A page's labels, gathered into one statement.
//!
//! `A1` reads **one** value beside its label and makes a **bare** block of it: no metadata, just
//! the row's text. A page declaring three totals therefore has three `A1` pipes in its
//! `pdf_extract`, told apart by block type, and somebody has to put them back together — because
//! the assets `text_filter` reads **one** block carrying `fund`, `currency` and the amounts in its
//! metadata, not three blocks carrying a number each.
//!
//! # Why this is not the `pdf_extract`'s work
//!
//! Gathering is not geometry. The `pdf_extract` has already said **where** the rows were, in the
//! only way that is its business: it marked them with a type. Saying that the block marked
//! `TOT_ASSETS` is *this statement's total assets* is attributing a meaning, which by `R23` is the
//! `text_filter`'s. The cost of the wrong segment is concrete: a `pdf_extract` that gathered would
//! not know what to do with a page carrying two statements side by side, and the segment after it
//! could no longer tell them apart.
//!
//! # One page, one statement
//!
//! `R7` says the datum is the statement **per sub-fund**, and the family this serves — the Italian
//! two-page statements of net assets — publishes **one report per fund**: the page carries one
//! statement and nothing else. So a [`Record`] produces **one** block, and two blocks of the same
//! type on one page are a warning, not a second statement. The day a page really carries two, the
//! parameter that separates them is measured on that page and added here.

use std::collections::BTreeMap;

use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock};

/// The bare blocks several `A1` pipes left on one page, gathered into one block.
///
/// Each field is **a metadata key and the block type that fills it**:
///
/// ```text
/// fields: { fund: FUND_NAME, currency: CURRENCY_STATEMENT, tot_assets: TOT_ASSETS }
/// ```
///
/// The block type is a [`BlockType`] built from a string and not a fixed vocabulary, because it is
/// **the** parameter a format chooses: it is the name the format gave its own `A1`, and two formats
/// of one family may spell it differently without anything here changing.
///
/// # The order of the fields, and the one thing it decides
///
/// The fields are held in a [`BTreeMap`], so they are visited in **key** order rather than in the
/// order the configuration wrote them. The only thing that order can be seen in is the sequence of
/// the warnings a page with several missing blocks leaves, and no reference run carries even one of
/// them: these are failure paths the measured documents never take. Were a format ever to depend on
/// it, the fix is an ordered sequence here, not a different warning.
#[derive(Debug, Clone)]
pub struct Record {
    /// The type of the block that comes out, which is what the standard `text_filter`s expect.
    pub block_type: BlockType,
    /// Metadata key → the block type that fills it.
    pub fields: BTreeMap<String, BlockType>,
}

/// What a [`Record`] refuses at build time.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    #[error("a record with no field would gather nothing")]
    NoField,
}

impl Record {
    pub fn new(
        block_type: BlockType,
        fields: BTreeMap<String, BlockType>,
    ) -> Result<Self, RecordError> {
        if fields.is_empty() {
            return Err(RecordError::NoField);
        }
        Ok(Self { block_type, fields })
    }

    /// The page's blocks, plus `also` — what the page does not write and the job already knows.
    ///
    /// `also` serves the pages that do **not name** their own sub-fund: EURIZON-IT24 writes its
    /// statement of net assets without repeating which fund it is, because the report carries one
    /// only and the name is on another page.
    ///
    /// It is a **fallback**, not an override: what the page declares always wins, and a field `also`
    /// brings is not reported as missing — it is not missing, it arrived from somewhere else.
    pub fn call(&self, blocks: &[PdfBlock], also: &BTreeMap<String, BlockValue>) -> Vec<PdfBlock> {
        let mut metadata = also.clone();
        for (key, block_type) in &self.fields {
            let taken: Vec<&PdfBlock> = blocks
                .iter()
                .filter(|blk| &blk.type_block == block_type)
                .collect();
            let [first, rest @ ..] = taken.as_slice() else {
                if !metadata.contains_key(key) {
                    tracing::warn!(
                        coord_ref_2 = key.as_str(),
                        "no '{block_type}' block on this page: the statement is read without its {key}"
                    );
                }
                continue;
            };
            if !rest.is_empty() {
                tracing::warn!(
                    coord_ref_2 = key.as_str(),
                    "{} blocks of type '{block_type}' where the page declares one statement: \
                     the first in page order is read as its {key}",
                    taken.len()
                );
            }
            metadata.insert(
                key.clone(),
                BlockValue::from(as_text(&first.content).as_str()),
            );
        }
        if metadata.is_empty() {
            tracing::warn!("none of the expected blocks is on this page: no statement is read");
            return Vec::new();
        }
        vec![PdfBlock::new(
            self.block_type.clone(),
            metadata,
            BlockValue::from(""),
        )]
    }
}

/// A block's content as the text the Python template's `str()` produced.
///
/// `A1` always writes a string, so every measured call site takes the first arm; the numeric arms
/// are there because a value is a `BlockValue` and silently turning a number into an empty string
/// would be the kind of hole `R12` is about.
pub(crate) fn as_text(value: &BlockValue) -> String {
    match value {
        BlockValue::Str(found) => found.clone(),
        BlockValue::Int(found) => found.to_string(),
        BlockValue::Float(found) => found.to_string(),
        other => {
            tracing::debug!(
                ?other,
                "a gathered block carries a value that is not text: read as empty"
            );
            String::new()
        }
    }
}
