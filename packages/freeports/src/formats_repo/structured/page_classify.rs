//! Le pipeline structured di classificazione pagina.
//!
//! Un pipe di classificazione dichiara una page class e un elenco di header da cercare: se la
//! pagina li contiene tutti, è di quella class. Ogni riga di `page_classify/args.csv` porta un
//! header, e le righe con lo stesso ID descrivono lo stesso pipe — l'unione l'ha già fatta
//! [`super::tables::get_page_classify_configs`].
//!
//! Porting di `structured/pipelines/page_classify.py::get_pipelines`.

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

/// Le pipeline di classificazione pagina che il repo definisce per `format_name`.
///
/// Ogni pipeline riceve **un solo** pipe per ciascuno dei segmenti `text_filter` e `deserialize`
/// (sono senza parametri e sempre gli stessi), e un pipe `pdf_extract` per ogni classificatore
/// dichiarato: è così che una pipeline può riconoscere più page class.
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
