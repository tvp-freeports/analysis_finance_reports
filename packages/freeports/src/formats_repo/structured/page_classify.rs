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
/// is how one pipeline can recognise several page classes. The partial-pipes table turns off a
/// classifier's own extraction, or the pipeline's filter or deserializer, on which its pipes agree.
pub fn get_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<HashMap<PipelineName, Pipeline>, StructuredError> {
    let configs = get_page_classify_configs(formats_repo_dir)?;
    let mut pipelines: HashMap<PipelineName, Pipeline> = HashMap::new();

    for config in configs.into_iter().filter(|c| c.id.format == format_name) {
        let segments = config.segments;
        let mut header_sets = Vec::with_capacity(config.header_sets.len());
        for raw in &config.header_sets {
            header_sets.push(pdfline_selection_from_str(raw).map_err(|source| {
                StructuredError::LineSelection {
                    id: config.id.to_string(),
                    column: "Header set",
                    source,
                }
            })?);
        }

        let name = PipelineName::new(&config.id.pipeline);
        let pipeline = pipelines.entry(name.clone()).or_insert_with(|| {
            let mut pipeline = Pipeline::new(name);
            if segments.text_filter {
                pipeline
                    .text_filter
                    .push(Arc::new(TextFilterPageClassifyStandard));
            }
            if segments.deserialize {
                pipeline
                    .deserialize
                    .push(Arc::new(DeserializerPageClassifyStandard));
            }
            pipeline
        });
        if let (true, Some(class)) = (segments.pdf_extract, &config.class) {
            pipeline
                .pdf_extract
                .push(Arc::new(PdfExtractPageClassifyStandard::new(
                    header_sets,
                    class,
                )));
        }
    }

    tracing::debug!(
        pipeline_count = pipelines.len(),
        "built page classify pipelines"
    );
    Ok(pipelines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// The sizes of the three segments of `A-EN24`'s unnamed pipeline.
    fn sizes(args: &str, partial: &str) -> (usize, usize, usize) {
        let dir = TempDir::new().expect("temp dir");
        let base = dir
            .path()
            .join(super::super::tables::STRUCTURED_DIR)
            .join("page_classify");
        fs::create_dir_all(&base).expect("page_classify dir");
        fs::write(
            base.join("args.csv"),
            format!("ID,Header set,Class\n{args}"),
        )
        .expect("args");
        fs::write(
            base.join("partial_pipes.csv"),
            format!("ID,pdf_extract,text_filter,deserialize\n{partial}"),
        )
        .expect("partial");
        let pipeline = get_pipelines(dir.path(), "A-EN24")
            .unwrap()
            .remove(&PipelineName::new(""))
            .expect("the unnamed pipeline");
        (
            pipeline.pdf_extract.len(),
            pipeline.text_filter.len(),
            pipeline.deserialize.len(),
        )
    }

    mod partial_segments {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn with_no_row_every_classifier_shares_one_filter_and_one_deserializer() {
            assert_eq!(sizes("A-EN24/0,,inv\nA-EN24/1,,manco\n", ""), (2, 1, 1));
        }

        /// The extraction is each classifier's own: turning one off leaves the other.
        #[test]
        fn a_disabled_pdf_extract_removes_that_classifier_only() {
            assert_eq!(
                sizes("A-EN24/0,,inv\nA-EN24/1,,\n", "A-EN24/1,FALSE,,\n"),
                (1, 1, 1)
            );
        }

        #[test]
        fn a_filter_and_deserializer_every_pipe_turns_off_are_left_empty() {
            assert_eq!(
                sizes(
                    "A-EN24/0,,inv\nA-EN24/1,,manco\n",
                    "A-EN24/0,,FALSE,FALSE\nA-EN24/1,,FALSE,FALSE\n"
                ),
                (2, 0, 0)
            );
        }
    }
}
