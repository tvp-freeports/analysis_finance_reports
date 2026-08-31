//! The structured page-classification pipelines.
//!
//! A classification pipe declares a page class and a list of headers to look for: if the page
//! contains all of them, it is of that class. Each row of the arguments table carries one header,
//! and rows sharing an id describe the same pipe — the join has already been done by
//! [`super::tables`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::core::pipeline::{Pipeline, PipelineName};
use crate::formats_utils::deserialize::standard_funcs::DeserializerPageClassifyStandard;
use crate::formats_utils::pdf_extract::standard_funcs::PdfExtractPageClassifyStandard;
use crate::formats_utils::text_filter::standard_funcs::TextFilterPageClassifyStandard;
use crate::input::document::selection::pdfline_selection_from_str;

use super::StructuredError;
use super::tables::get_page_classify_configs;

/// The page-classification pipelines the repository defines for `format_name`.
///
/// Each pipeline gets **one** pipe for each of the filtering and deserialization segments, those
/// being parameterless and always the same, and one extraction pipe per declared classifier — which
/// is how one pipeline can recognise several page classes.
pub fn get_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<HashMap<PipelineName, Pipeline>, StructuredError> {
    let configs = get_page_classify_configs(formats_repo_dir)?;
    let mut pipelines: HashMap<PipelineName, Pipeline> = HashMap::new();

    for config in configs.into_iter().filter(|c| c.id.format == format_name) {
        let mut header_sets = Vec::with_capacity(config.header_sets.len());
        for raw in &config.header_sets {
            header_sets.push(pdfline_selection_from_str(raw).map_err(|source| StructuredError::LineSelection {
                id: config.id.to_string(),
                column: "Header set",
                source,
            })?);
        }

        let name = PipelineName::new(&config.id.pipeline);
        let pipeline = pipelines.entry(name.clone()).or_insert_with(|| {
            let mut pipeline = Pipeline::new(name);
            pipeline.text_filter.push(Arc::new(TextFilterPageClassifyStandard));
            pipeline.deserialize.push(Arc::new(DeserializerPageClassifyStandard));
            pipeline
        });
        pipeline
            .pdf_extract
            .push(Arc::new(PdfExtractPageClassifyStandard::new(header_sets, &config.class)));
    }

    tracing::debug!(pipeline_count = pipelines.len(), "built page classify pipelines");
    Ok(pipelines)
}
