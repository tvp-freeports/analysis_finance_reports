//! Loading a formats repository and building a format's algorithm.
//!
//! A formats repository is a directory of CSV files, YAML files and — for the most irregular
//! formats — Python code. This module reads it and produces an [`Algorithm`] ready to apply to a
//! document.
//!
//! # The three levels
//!
//! A format's pipelines are composed from three levels of increasing specificity, which **add up**
//! rather than exclude one another:
//!
//! | Level | Where the algorithm lives | Where the parameters live |
//! |---|---|---|
//! | [`structured`] | in the library, fixed | CSV columns |
//! | [`semistructured`] | in the library (by name) or in the repository | YAML |
//! | [`unstructured`] | in the repository, in Python | in the code itself |
//!
//! A format may use one, two or all three, and mixing them is the normal case: extraction
//! structured and filtering unstructured, because what is irregular about a document is rarely
//! irregular in every segment. The merge happens in [`load_pipelines`], summing the **same-named**
//! pipelines of the three levels, and it is the only place the three meet — no other module knows
//! they exist.
//!
//! # The files read
//!
//! - [`metadata`] — the format list and the URL mapping;
//! - [`orchestration`] — the schedule and page-class tables;
//! - [`structured`], [`semistructured`], [`unstructured`] — one subtree each.
//!
//! [`id_format`] reads nothing: it is the grammar of identifiers the others share.

pub mod id_format;
pub mod metadata;
pub mod orchestration;
pub mod semistructured;
pub mod structured;
pub mod unstructured;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::core::algorithm::{Algorithm, AlgorithmError};
use crate::core::page::FormatName;
use crate::core::pipeline::{Pipeline, PipelineName};

/// Failures of loading a formats repository.
///
/// One enum for the whole area, collecting the submodules': a caller of [`Algorithm::load`] has no
/// way to react differently to a malformed CSV and a malformed YAML, so distinguishing them in the
/// *type* rather than in the message would not help it.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The requested format is not among those the repository declares.
    #[error("unknown format '{format}'; the repository declares {known} format(s)")]
    UnknownFormat { format: String, known: usize },
    /// A pipeline lacks one of the three segments: none of the three levels supplied it.
    #[error("pipeline '{pipeline}' is incomplete: missing the {missing} segment(s)")]
    IncompletePipeline { pipeline: String, missing: String },
    #[error(transparent)]
    Metadata(#[from] metadata::MetadataError),
    #[error(transparent)]
    Orchestration(#[from] orchestration::OrchestrationError),
    #[error(transparent)]
    Structured(#[from] structured::StructuredError),
    #[error(transparent)]
    Semistructured(#[from] semistructured::SemistructuredError),
    #[error(transparent)]
    Unstructured(#[from] unstructured::loader::UnstructuredError),
    #[error(transparent)]
    Algorithm(#[from] AlgorithmError),
}

/// Adds `incoming` to the pipeline already present under the same name, if any.
fn merge_into(pipelines: &mut HashMap<PipelineName, Pipeline>, name: PipelineName, incoming: Pipeline) {
    match pipelines.remove(&name) {
        Some(existing) => pipelines.insert(name, existing + incoming),
        None => pipelines.insert(name, incoming),
    };
}

/// The names of a pipeline's empty segments, for the error message.
fn missing_segments(pipeline: &Pipeline) -> String {
    let mut missing = Vec::new();
    if pipeline.pdf_extract.is_empty() {
        missing.push("pdf_extract");
    }
    if pipeline.text_filter.is_empty() {
        missing.push("text_filter");
    }
    if pipeline.deserialize.is_empty() {
        missing.push("deserialize");
    }
    missing.join(", ")
}

/// Every pipeline of `format_name`, with the three levels already merged.
///
/// `allow_partial` exists for the development tooling, which tries one segment at a time. In
/// production an incomplete pipeline can produce nothing, and letting one through would mean
/// discovering the problem with a document already open rather than at load time.
pub fn load_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
    allow_partial: bool,
) -> Result<HashMap<PipelineName, Pipeline>, LoadError> {
    let mut pipelines = structured::get_pipelines(formats_repo_dir, format_name)?;
    for (name, pipeline) in semistructured::get_pipelines(formats_repo_dir, format_name)? {
        merge_into(&mut pipelines, name, pipeline);
    }
    for (name, pipeline) in unstructured::loader::get_pipelines(formats_repo_dir, format_name)? {
        merge_into(&mut pipelines, name, pipeline);
    }

    if !allow_partial {
        // Sorted: with a hash map the first incomplete pipeline found would depend on hash order,
        // and the error message would change from one run to the next.
        let mut names: Vec<&PipelineName> = pipelines.keys().collect();
        names.sort();
        for name in names {
            let pipeline = &pipelines[name];
            if !pipeline.is_complete() {
                return Err(LoadError::IncompletePipeline {
                    pipeline: name.as_str().to_string(),
                    missing: missing_segments(pipeline),
                });
            }
        }
    }
    Ok(pipelines)
}

impl Algorithm {
    /// Loads the algorithm of `format_name` from a formats repository.
    ///
    /// # Why this impl lives here and not with [`Algorithm`]
    ///
    /// The constructor needs the whole of `formats_repo`, while `formats_repo` is built on top of
    /// `core`. Putting it there would invert the layering. An inherent impl may live in any module
    /// of the crate that defines the type, so the public API stays `Algorithm::load` without the
    /// dependency running both ways.
    pub fn load(formats_repo_dir: &Path, format_name: &FormatName) -> Result<Algorithm, LoadError> {
        // The two nested spans for the loading branch: no other point in the crate opens one for a
        // repository or a format, so the coordinates of every event emitted while loading — from
        // the metadata CSV to the last unstructured pipeline — come from here.
        let repo_span = tracing::info_span!("formats_repo", path = %formats_repo_dir.display());
        let _repo_guard = repo_span.enter();
        let name = format_name.as_str();
        let format_span = tracing::info_span!("format", format = name);
        let _format_guard = format_span.enter();

        let known_formats = metadata::get_formats(formats_repo_dir)?;
        if !known_formats.iter().any(|f| f == name) {
            return Err(LoadError::UnknownFormat { format: name.to_string(), known: known_formats.len() });
        }

        let pipelines = load_pipelines(formats_repo_dir, name, false)?;
        let defined: HashSet<PipelineName> = pipelines.keys().cloned().collect();

        let classifiers = orchestration::get_pageclassify_pipelines(formats_repo_dir, name, &known_formats)?;
        let mapping = orchestration::get_mapping(formats_repo_dir, name, &known_formats, &defined)?;
        let schedule = orchestration::get_schedule(formats_repo_dir, name, &known_formats, &defined)?;
        let finalizer = unstructured::loader::get_page_class_finalizer(formats_repo_dir, name)?;

        // Sorted everywhere: an [`Algorithm`] keeps the order it is given, and the order in which
        // the pipes run follows from it.
        let mut classify_names: Vec<PipelineName> = classifiers.into_iter().collect();
        classify_names.sort();
        let sorted_pipelines: BTreeMap<PipelineName, Pipeline> = pipelines.into_iter().collect();
        let sorted_mapping: BTreeMap<crate::core::schedule::PageClass, Vec<PipelineName>> = mapping
            .into_iter()
            .map(|(class, names)| {
                let mut names: Vec<PipelineName> = names.into_iter().collect();
                names.sort();
                (class, names)
            })
            .collect();

        let pipeline_count = sorted_pipelines.len();
        let step_count = schedule.steps().len();
        let algorithm = Algorithm::new(
            format_name.clone(),
            sorted_pipelines,
            &classify_names,
            finalizer,
            schedule,
            sorted_mapping,
        )?;
        tracing::info!(pipeline_count, step_count, "format algorithm loaded");
        Ok(algorithm)
    }
}
