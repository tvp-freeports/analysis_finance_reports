//! The structured level: pipelines built from CSV tables with native implementations.
//!
//! "Structured" means a segment's algorithm is **in the library** and the repository supplies only
//! its parameters: a structured format contains no code, only rows of CSV. It is the most
//! constrained of the three levels and the one covering most formats.
//!
//! - [`tables`] reads and validates the CSV files and joins their rows per pipe;
//! - [`investments`] and [`page_classify`] build the corresponding native pipes.

pub mod investments;
pub mod page_classify;
pub mod tables;

use crate::commons::flag_expr::FlagExprError;
use crate::formats_utils::pdf_extract::standard_funcs::PdfExtractStandardFuncsError;
use crate::formats_utils::text_filter::standard_funcs::StandardFuncsError;
use crate::input::document::selection::LineSelectionError;

use tables::TableError;

/// Failures of building the structured pipelines.
///
/// One enum for the level rather than one per file: the two builder submodules share the same
/// causes of failure — an unreadable table, a cell that is not a selection, a parameter the pipe
/// rejects — and splitting them would mean converting back and forth between two enums that say the
/// same things.
#[derive(Debug, thiserror::Error)]
pub enum StructuredError {
    #[error(transparent)]
    Table(#[from] TableError),
    /// A cell passed the shape validation but would not parse.
    ///
    /// This should not happen, the validation being the same grammar, which is why the error names
    /// both the pipe and the column: if it ever fires, the two have drifted apart.
    #[error("'{id}', column '{column}': {source}")]
    LineSelection {
        id: String,
        column: &'static str,
        #[source]
        source: LineSelectionError,
    },
    #[error("'{id}': invalid algorithm flags: {source}")]
    AlgorithmFlags {
        id: String,
        #[source]
        source: FlagExprError,
    },
    #[error("'{id}': invalid dash-as-zero flags: {source}")]
    DashAsZero {
        id: String,
        #[source]
        source: FlagExprError,
    },
    /// The `text_filter` pipe rejected the parameters read from the CSV, typically two column
    /// positions that are equal to each other.
    #[error("'{id}': {source}")]
    TextFilter {
        id: String,
        #[source]
        source: StandardFuncsError,
    },
    #[error("'{id}': {source}")]
    PdfExtract {
        id: String,
        #[source]
        source: PdfExtractStandardFuncsError,
    },
    /// The arguments table does not declare the market-value column position, the pipe's only
    /// parameter without a default.
    #[error("'{id}': column 'Market value' is required to build the text_filter segment")]
    MissingMarketValue { id: String },
}

/// Every structured pipeline of `format_name`: the investments ones and the page-classification
/// ones, merged by name.
///
/// The merge here is between the two *sublevels* of structured; the merge between structured,
/// semistructured and unstructured happens one level up.
pub fn get_pipelines(
    formats_repo_dir: &std::path::Path,
    format_name: &str,
) -> Result<std::collections::HashMap<crate::core::pipeline::PipelineName, crate::core::pipeline::Pipeline>, StructuredError>
{
    let mut merged = investments::get_pipelines(formats_repo_dir, format_name)?;
    for (name, pipeline) in page_classify::get_pipelines(formats_repo_dir, format_name)? {
        match merged.remove(&name) {
            Some(existing) => merged.insert(name, existing + pipeline),
            None => merged.insert(name, pipeline),
        };
    }
    tracing::debug!(pipeline_count = merged.len(), "built structured pipelines");
    Ok(merged)
}
