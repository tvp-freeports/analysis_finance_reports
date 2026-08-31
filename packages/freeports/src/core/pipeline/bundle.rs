//! [`PipelinesBundle`]: the pipelines applied to one and the same page.
//!
//! A bundle is what a page class is *worth*: the set of pipelines that process the pages of that
//! class. [`Algorithm`](crate::core::algorithm::Algorithm) holds one per page class, plus one for
//! classification itself.
//!
//! Pipelines are **deduplicated by name**. They always arrive from a name-to-pipeline map, so
//! "the same pipeline" and "the same name" mean the same thing here, and the name is also what
//! makes an error raised from inside one readable.

use crate::core::classes::{PdfBlock, TextBlock};
use crate::core::page::Page;

use super::data::{Extracted, FilterData, PipeError};
use super::{Pipeline, PipelineName};

/// An ordered, name-deduplicated set of [`Pipeline`]s run over the same page.
#[derive(Debug, Clone, Default)]
pub struct PipelinesBundle(Vec<Pipeline>);

impl PipelinesBundle {
    pub fn new() -> Self {
        PipelinesBundle::default()
    }

    /// Appends a pipeline unless one with the same name is already present. Returns whether it was
    /// actually added.
    pub fn push(&mut self, pipeline: Pipeline) -> bool {
        if self.0.iter().any(|existing| existing.name == pipeline.name) {
            return false;
        }
        self.0.push(pipeline);
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = &Pipeline> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The pipeline with this name, if the bundle holds it.
    pub fn get(&self, name: &PipelineName) -> Option<&Pipeline> {
        self.0.iter().find(|p| &p.name == name)
    }

    /// Whether **every** pipeline in the bundle is complete.
    ///
    /// An empty bundle is vacuously complete: deciding whether an empty bundle is acceptable
    /// belongs to `Algorithm::new`, not here.
    pub fn is_complete(&self) -> bool {
        self.0.iter().all(Pipeline::is_complete)
    }

    /// The names of the incomplete pipelines, for the error messages of whoever loads a formats
    /// repository.
    pub fn incomplete(&self) -> Vec<&PipelineName> {
        self.0.iter().filter(|p| !p.is_complete()).map(|p| &p.name).collect()
    }

    /// Whether **every** pipeline in the bundle scales with threads.
    ///
    /// This is the question [`Algorithm`](crate::core::algorithm::Algorithm) asks before spreading
    /// the pages of a step across threads. It is answered per bundle rather than per format on
    /// purpose: a format may classify its pages in Python and run its steps in pure Rust, and
    /// degrading the whole format would remove the gain exactly where it is largest.
    pub fn scales_with_threads(&self) -> bool {
        self.0.iter().all(|pipeline| pipeline.scales_with_threads())
    }

    /// The full chain of every pipeline in the bundle, concatenated in insertion order.
    pub fn apply(&self, page: &Page, data: &FilterData<'_>) -> Result<Vec<Extracted>, PipeError> {
        let mut out = Vec::new();
        for pipeline in &self.0 {
            out.extend(pipeline.apply(page, data)?);
        }
        Ok(out)
    }

    /// The first segment of every pipeline — the per-segment API the format development tooling
    /// drives.
    pub fn apply_pdf_extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        let mut out = Vec::new();
        for pipeline in &self.0 {
            out.extend(pipeline.apply_pdf_extract(page)?);
        }
        Ok(out)
    }

    /// The first two segments of every pipeline — the per-segment API the format development
    /// tooling drives.
    pub fn apply_text_filter(
        &self,
        page: &Page,
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        let mut out = Vec::new();
        for pipeline in &self.0 {
            out.extend(pipeline.apply_text_filter(page, data)?);
        }
        Ok(out)
    }

    /// The full chain — the per-segment API the format development tooling drives.
    ///
    /// Identical to [`PipelinesBundle::apply`]. It exists as a separate name only because it is one
    /// of the three partial APIs the tooling calls by segment.
    pub fn apply_deserialize(
        &self,
        page: &Page,
        data: &FilterData<'_>,
    ) -> Result<Vec<Extracted>, PipeError> {
        self.apply(page, data)
    }
}

impl FromIterator<Pipeline> for PipelinesBundle {
    fn from_iter<I: IntoIterator<Item = Pipeline>>(iter: I) -> Self {
        let mut bundle = PipelinesBundle::new();
        for pipeline in iter {
            bundle.push(pipeline);
        }
        bundle
    }
}

#[cfg(test)]
mod tests {
    use super::super::segment::test_pipes::*;
    use super::*;
    use crate::core::pipeline::TextFilterPipe;
    use crate::core::schedule::PageClass;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use std::sync::Arc;

    fn page_with(texts: &[&str]) -> Page {
        let lines = texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let y = i as f32 * 10.0;
                PdfLine::new("Arial", 10.0, t, (0.0, y, 10.0, y + 10.0))
            })
            .collect();
        Page::new(1, (100.0, 100.0), lines, vec![])
    }

    fn classifying_pipeline(name: &str, class: Option<&str>) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(ConstantClassifier::pipe("classify", class));
        pipeline
    }

    mod membership {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn pipelines_keep_their_insertion_order() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("a", None), classifying_pipeline("b", None)]
                    .into_iter()
                    .collect();
            let names: Vec<&str> = bundle.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, vec!["a", "b"]);
        }

        #[test]
        fn a_second_pipeline_with_the_same_name_is_rejected() {
            let mut bundle = PipelinesBundle::new();
            assert!(bundle.push(classifying_pipeline("a", None)));
            assert!(!bundle.push(classifying_pipeline("a", Some("other"))));
            assert_eq!(bundle.len(), 1);
        }

        #[test]
        fn a_pipeline_is_retrievable_by_name() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("a", None)].into_iter().collect();
            assert!(bundle.get(&PipelineName::new("a")).is_some());
            assert!(bundle.get(&PipelineName::new("b")).is_none());
        }

        #[test]
        fn a_fresh_bundle_is_empty() {
            assert!(PipelinesBundle::new().is_empty());
            assert_eq!(PipelinesBundle::default().len(), 0);
        }
    }

    mod completeness {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_bundle_of_complete_pipelines_is_complete() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("a", None)].into_iter().collect();
            assert!(bundle.is_complete());
            assert!(bundle.incomplete().is_empty());
        }

        #[test]
        fn an_incomplete_pipeline_is_reported_by_name() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("good", None), Pipeline::new("bad")].into_iter().collect();
            assert!(!bundle.is_complete());
            assert_eq!(bundle.incomplete(), vec![&PipelineName::new("bad")]);
        }

        #[test]
        fn an_empty_bundle_is_vacuously_complete() {
            assert!(PipelinesBundle::new().is_complete());
        }
    }

    mod application {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_results_of_every_pipeline_are_concatenated_in_order() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("a", Some("x")), classifying_pipeline("b", Some("y"))]
                    .into_iter()
                    .collect();

            let out = bundle.apply(&page_with(&["line"]), &FilterData::EMPTY).unwrap();
            assert_eq!(
                out,
                vec![
                    Extracted::PageClass(Some(PageClass::new("x"))),
                    Extracted::PageClass(Some(PageClass::new("y"))),
                ]
            );
        }

        #[test]
        fn an_empty_bundle_produces_nothing() {
            let out = PipelinesBundle::new().apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap();
            assert!(out.is_empty());
        }

        #[test]
        fn a_failing_pipeline_stops_the_bundle() {
            let mut failing = classifying_pipeline("bad", None);
            failing.pdf_extract = Default::default();
            failing.pdf_extract.push(FailingExtract::fatal("boom"));

            let bundle: PipelinesBundle =
                [failing, classifying_pipeline("never reached", None)].into_iter().collect();
            let err = bundle.apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap_err();
            assert_eq!(err.pipe(), "boom");
        }

        #[test]
        fn a_page_failure_travels_out_of_the_bundle_unchanged() {
            let mut failing = classifying_pipeline("bad", None);
            failing.pdf_extract = Default::default();
            failing.pdf_extract.push(FailingExtract::page_parse("skipper"));

            let bundle: PipelinesBundle = [failing].into_iter().collect();
            let err = bundle.apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap_err();
            assert!(err.is_page_failure());
        }
    }

    mod per_segment_api {
        use super::*;
        use pretty_assertions::assert_eq;

        fn two_pipeline_bundle() -> PipelinesBundle {
            [classifying_pipeline("a", Some("x")), classifying_pipeline("b", Some("y"))]
                .into_iter()
                .collect()
        }

        #[test]
        fn apply_pdf_extract_concatenates_the_first_segment_of_every_pipeline() {
            let blocks = two_pipeline_bundle().apply_pdf_extract(&page_with(&["l"])).unwrap();
            assert_eq!(blocks.len(), 2);
        }

        #[test]
        fn apply_text_filter_concatenates_the_first_two_segments() {
            let blocks = two_pipeline_bundle()
                .apply_text_filter(&page_with(&["l"]), &FilterData::EMPTY)
                .unwrap();
            assert_eq!(blocks.len(), 2);
        }

        #[test]
        fn apply_deserialize_matches_apply_because_there_are_no_nones_to_filter() {
            let bundle = two_pipeline_bundle();
            let page = page_with(&["l"]);
            assert_eq!(
                bundle.apply_deserialize(&page, &FilterData::EMPTY).unwrap(),
                bundle.apply(&page, &FilterData::EMPTY).unwrap()
            );
        }
    }

    mod thread_scaling {
        use super::*;

        fn rust_pipeline(name: &str) -> Pipeline {
            let mut pipeline = Pipeline::new(name);
            pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
            pipeline
        }

        fn author_pipeline(name: &str) -> Pipeline {
            let mut pipeline = Pipeline::new(name);
            pipeline.pdf_extract.push(GilBoundExtract::pipe("author"));
            pipeline
        }

        #[test]
        fn an_empty_bundle_scales() {
            assert!(PipelinesBundle::new().scales_with_threads());
        }

        #[test]
        fn a_bundle_of_rust_pipelines_scales() {
            let mut bundle = PipelinesBundle::new();
            bundle.push(rust_pipeline("a"));
            bundle.push(rust_pipeline("b"));
            assert!(bundle.scales_with_threads());
        }

        #[test]
        fn one_author_pipeline_is_enough_to_stop_the_bundle() {
            let mut bundle = PipelinesBundle::new();
            bundle.push(rust_pipeline("a"));
            bundle.push(author_pipeline("b"));
            assert!(
                !bundle.scales_with_threads(),
                "every pipeline of the bundle runs on the same page: one of them taking the GIL \
                 serializes that page anyway"
            );
        }
    }
}
