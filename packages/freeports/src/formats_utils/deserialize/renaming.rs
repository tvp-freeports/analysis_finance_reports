//! One row per block: a renaming or a merger, with the date as the only cast.
//!
//! Which of the two entities comes out is the `merge` flag the `text_filter` wrote, because telling
//! a renaming from a merger is reading the page's verb and `R23` puts that in the segment before.
//! Here there is one decision left, and it is a conversion: the language the document writes its
//! months in.

use std::sync::Arc;

use crate::commons::date::Date;
use crate::core::classes::TextBlock;
use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
use crate::formats_utils::deserialize::cast::CastError;
use crate::core::classes::value::BlockValue;
use crate::output::classes::OutputClassError;
use crate::output::classes::fund_change_name::{FundMerge, FundRename};

/// The converter of the language a document writes its months in.
pub type DateConverter = Arc<dyn Fn(&str) -> Result<Date, CastError> + Send + Sync>;

/// Builds the `FundRename` or the `FundMerge` a block declares.
pub struct DeserializerRenaming {
    to_date: DateConverter,
}

impl DeserializerRenaming {
    pub fn new(to_date: DateConverter) -> Self {
        Self { to_date }
    }
}

impl DeserializePipe for DeserializerRenaming {
    fn name(&self) -> &str {
        "DeserializerRenaming"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let pipe = || DeserializePipe::name(self).to_string();
        let missing = |field: &str| PipeError::Extraction {
            pipe: pipe(),
            message: format!("a change block carries no '{field}'"),
        };
        let text = |field: &'static str| -> Result<String, PipeError> {
            block
                .metadata
                .get(field)
                .ok_or_else(|| missing(field))?
                .str_or_fail(field)
                .map(str::to_string)
                .map_err(|source| PipeError::Value { pipe: pipe(), source })
        };
        let merge = block
            .metadata
            .get("merge")
            .ok_or_else(|| missing("merge"))?
            .bool_or_fail("merge")
            .map_err(|source| PipeError::Value { pipe: pipe(), source })?;
        let when = text("when")?;
        let date = (self.to_date)(&when).map_err(|e| PipeError::Cast {
            pipe: pipe(),
            field: "date".to_string(),
            message: e.to_string(),
        })?;
        let (old_name, current_name) = (text("old_name")?, text("current_name")?);
        let built = |e: OutputClassError| PipeError::Cast {
            pipe: pipe(),
            field: "name".to_string(),
            message: e.to_string(),
        };
        let date = BlockValue::Date(date);
        Ok(vec![if merge {
            Extracted::FundMerge(FundMerge::build(&old_name, &current_name, &date).map_err(built)?)
        } else {
            Extracted::FundRename(FundRename::build(&old_name, &current_name, &date).map_err(built)?)
        }])
    }
}
