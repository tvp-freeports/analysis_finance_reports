//! One row per block: an ESG indicator's name and value, as the page writes them.
//!
//! The `deserialize` of `A6` does nothing more than build the entity, which is all `R23` leaves it:
//! the reading was the `text_filter`'s, and there is no cast to do — an indicator's value is **text**
//! in the output schema, because the pages write it in units that are part of the measurement
//! (`26.81 tons CO₂ per million dollars revenues`) and a number alone would lose them.
//!
//! The `fund` may be a promise, which is why it goes through `BlockValue` rather than a string:
//! three of the five measured formats name the sub-fund on another page.

use crate::core::classes::TextBlock;
use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
use crate::output::classes::fund_esg_indicator::FundEsgIndicator;

/// Builds a `FundEsgIndicator` from a block `TextFilterEsg` produced.
pub struct DeserializerEsgIndicator;

impl DeserializePipe for DeserializerEsgIndicator {
    fn name(&self) -> &str {
        "DeserializerEsgIndicator"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let missing = |field: &str| PipeError::Extraction {
            pipe: DeserializePipe::name(self).to_string(),
            message: format!("an indicator block carries no '{field}'"),
        };
        let text = |field: &'static str| -> Result<String, PipeError> {
            block
                .metadata
                .get(field)
                .ok_or_else(|| missing(field))?
                .str_or_fail(field)
                .map(str::to_string)
                .map_err(|source| PipeError::Value {
                    pipe: DeserializePipe::name(self).to_string(),
                    source,
                })
        };
        let fund = block.metadata.get("fund").ok_or_else(|| missing("fund"))?;
        let indicator = FundEsgIndicator::build(fund, text("name")?, text("value")?).map_err(|e| {
            PipeError::Cast {
                pipe: DeserializePipe::name(self).to_string(),
                field: "fund".to_string(),
                message: e.to_string(),
            }
        })?;
        Ok(vec![Extracted::FundEsgIndicator(indicator)])
    }
}
