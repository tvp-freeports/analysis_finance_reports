//! A pipeline: the three segments `pdf_extract` → `text_filter` → `deserialize`.
//!
//! The three answer three separable questions about a page — *what is on it*, *does any of it
//! concern the funds we are looking for*, *what do the survivors mean* — and keeping them apart is
//! what lets a format author replace one without understanding the other two.
//!
//! A [`Pipeline`] carries its own **name**. Holding the name only as the key of the map that
//! contains it would make every error message from inside a pipeline anonymous, which is precisely
//! when the name is wanted.
//!
//! This module is also the engine's vocabulary entry point: it re-exports
//! [`FilterData`]/[`Extracted`]/[`PipeError`] from [`data`] and the three pipe traits with their
//! segments from [`segment`], so that a consumer imports from `core::pipeline` and never has to
//! know how the code is split across files.

pub mod bundle;
pub mod data;
pub mod segment;

pub use data::{Extracted, FilterData, PipeError, PromiseEntries};
pub use segment::{
    DeserializePipe, DeserializeSegment, PdfExtractPipe, PdfExtractSegment, Segment,
    TextFilterPipe, TextFilterSegment,
};

use crate::core::classes::{PdfBlock, TextBlock};
use crate::core::page::Page;

/// The name of a pipeline within a format — the parenthesised part of
/// `<format>(<pipeline>)/<index>`.
///
/// The empty string is a legitimate name: in a formats repository it identifies the "no group"
/// pipeline.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PipelineName(String);

impl PipelineName {
    pub fn new(name: impl Into<String>) -> Self {
        PipelineName(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PipelineName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for PipelineName {
    fn from(value: &str) -> Self {
        PipelineName(value.to_string())
    }
}

impl From<String> for PipelineName {
    fn from(value: String) -> Self {
        PipelineName(value)
    }
}

/// The three segments, applied to a page in order.
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub name: PipelineName,
    pub pdf_extract: PdfExtractSegment,
    pub text_filter: TextFilterSegment,
    pub deserialize: DeserializeSegment,
}

impl Pipeline {
    /// A pipeline with the given name and three empty segments.
    pub fn new(name: impl Into<PipelineName>) -> Self {
        Pipeline {
            name: name.into(),
            pdf_extract: PdfExtractSegment::new(),
            text_filter: TextFilterSegment::new(),
            deserialize: DeserializeSegment::new(),
        }
    }

    /// Whether **all three** segments hold at least one pipe.
    ///
    /// Only a complete pipeline can produce anything, so `Algorithm::load` rejects incomplete ones.
    /// It is not an error for a pipeline to be incomplete mid-construction: a format is assembled
    /// by summing partial pipelines from several sources, and only the sum has to be whole.
    pub fn is_complete(&self) -> bool {
        !self.pdf_extract.is_empty() && !self.text_filter.is_empty() && !self.deserialize.is_empty()
    }

    /// Extract, filter, deserialize: the whole chain over one page.
    pub fn apply(
        &self,
        page: &Page,
        data: &FilterData<'_>,
    ) -> Result<Vec<Extracted>, PipeError> {
        // Where the `Activity` vocabulary is orchestrated: the three segments — opened inside
        // `Segment::apply` — and the `pipe[<name>]` of each nest below this span.
        let pipeline_span = tracing::info_span!("pipeline", pipeline = %self.name);
        let _pipeline_guard = pipeline_span.enter();

        let pdf_blocks = self.pdf_extract.apply(page)?;
        let text_blocks = self.text_filter.apply(&pdf_blocks, data)?;
        self.deserialize.apply(&text_blocks)
    }

    /// Whether every pipe in all three segments scales with threads.
    ///
    /// A single author-written pipe is enough to answer `false`: the segments run in a chain, so
    /// the GIL taken by one of them serialises the page's whole chain anyway.
    pub fn scales_with_threads(&self) -> bool {
        self.pdf_extract.iter().all(|pipe| pipe.scales_with_threads())
            && self.text_filter.iter().all(|pipe| pipe.scales_with_threads())
            && self.deserialize.iter().all(|pipe| pipe.scales_with_threads())
    }

    /// The first segment alone — the per-segment API the format development tooling drives.
    pub fn apply_pdf_extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        let pipeline_span = tracing::info_span!("pipeline", pipeline = %self.name);
        let _pipeline_guard = pipeline_span.enter();
        self.pdf_extract.apply(page)
    }

    /// The first two segments — the per-segment API the format development tooling drives.
    pub fn apply_text_filter(
        &self,
        page: &Page,
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        let pipeline_span = tracing::info_span!("pipeline", pipeline = %self.name);
        let _pipeline_guard = pipeline_span.enter();
        let pdf_blocks = self.pdf_extract.apply(page)?;
        self.text_filter.apply(&pdf_blocks, data)
    }
}

impl std::ops::Add for Pipeline {
    type Output = Pipeline;

    /// Merges the three segments one by one; this is how a format's structured, semistructured and
    /// unstructured layers combine into one pipeline.
    ///
    /// The name kept is the **left** operand's. `Algorithm::load` only ever sums pipelines it has
    /// already grouped by name, so in real use the two names are equal; summing differently named
    /// pipelines is a caller error that `Add` has no way to report, since it cannot return a
    /// `Result`.
    fn add(self, rhs: Self) -> Self::Output {
        Pipeline {
            name: self.name,
            pdf_extract: self.pdf_extract + rhs.pdf_extract,
            text_filter: self.text_filter + rhs.text_filter,
            deserialize: self.deserialize + rhs.deserialize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::segment::test_pipes::*;
    use super::*;
    use crate::core::classes::BlockType;
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

    /// A complete pipeline that classifies every page as `class`.
    fn classifying_pipeline(name: &str, class: Option<&str>) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(ConstantClassifier::pipe("classify", class));
        pipeline
    }

    mod pipeline_name {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn round_trips_and_displays() {
            assert_eq!(PipelineName::new("investments").as_str(), "investments");
            assert_eq!(PipelineName::from("investments").to_string(), "investments");
        }

        #[test]
        fn the_empty_name_is_legal_and_is_the_default() {
            // In a formats repository this identifies the "no group" pipeline.
            assert_eq!(PipelineName::default(), PipelineName::new(""));
            assert_eq!(PipelineName::new("").to_string(), "");
        }

        #[test]
        fn is_built_from_both_str_and_string() {
            assert_eq!(PipelineName::from("p"), PipelineName::from("p".to_string()));
        }
    }

    mod completeness {
        use super::*;

        #[test]
        fn a_fresh_pipeline_is_incomplete() {
            assert!(!Pipeline::new("p").is_complete());
        }

        #[test]
        fn all_three_segments_are_required() {
            let full = classifying_pipeline("p", Some("x"));
            assert!(full.is_complete());

            let mut no_extract = full.clone();
            no_extract.pdf_extract = PdfExtractSegment::new();
            assert!(!no_extract.is_complete());

            let mut no_filter = full.clone();
            no_filter.text_filter = TextFilterSegment::new();
            assert!(!no_filter.is_complete());

            let mut no_deserialize = full;
            no_deserialize.deserialize = DeserializeSegment::new();
            assert!(!no_deserialize.is_complete());
        }
    }

    mod chaining {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_three_segments_run_in_order_and_feed_each_other() {
            let pipeline = classifying_pipeline("p", Some("investments"));
            let out = pipeline.apply(&page_with(&["a", "b"]), &FilterData::EMPTY).unwrap();
            // Two lines -> two pdf blocks -> two text blocks -> two classifications.
            assert_eq!(
                out,
                vec![
                    Extracted::PageClass(Some(PageClass::new("investments"))),
                    Extracted::PageClass(Some(PageClass::new("investments"))),
                ]
            );
        }

        #[test]
        fn an_incomplete_pipeline_simply_yields_nothing() {
            // No deserialize pipe: the chain runs to the end and produces zero results rather than
            // an error. Rejecting incomplete pipelines is `Algorithm::load`'s job, not the chain's.
            let mut pipeline = Pipeline::new("p");
            pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
            pipeline
                .text_filter
                .push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            assert!(pipeline.apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap().is_empty());
        }

        #[test]
        fn a_failure_in_the_first_segment_stops_the_chain() {
            let mut pipeline = classifying_pipeline("p", Some("x"));
            pipeline.pdf_extract = PdfExtractSegment::new();
            pipeline.pdf_extract.push(FailingExtract::fatal("boom"));

            let err = pipeline.apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap_err();
            assert_eq!(err.pipe(), "boom");
        }

        #[test]
        fn the_filter_data_reaches_the_text_filter_segment() {
            let filter = RecordingFilter::new("filter");
            let mut pipeline = Pipeline::new("p");
            pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
            pipeline.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            pipeline.deserialize.push(ConstantClassifier::pipe("classify", None));

            let previous = vec![Extracted::PageClass(None), Extracted::PageClass(None)];
            pipeline.apply(&page_with(&["a"]), &FilterData::Previous(&previous)).unwrap();
            assert_eq!(filter.seen(), vec![(0, 2)]);
        }
    }

    mod per_segment_api {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn apply_pdf_extract_stops_after_the_first_segment() {
            let pipeline = classifying_pipeline("p", Some("x"));
            let blocks = pipeline.apply_pdf_extract(&page_with(&["a", "b"])).unwrap();
            let contents: Vec<&str> =
                blocks.iter().map(|b| b.content.as_str().unwrap()).collect();
            assert_eq!(contents, vec!["a", "b"]);
        }

        #[test]
        fn apply_text_filter_stops_after_the_second_segment() {
            let pipeline = classifying_pipeline("p", Some("x"));
            let blocks =
                pipeline.apply_text_filter(&page_with(&["a", "b"]), &FilterData::EMPTY).unwrap();
            assert_eq!(blocks.len(), 2);
            assert_eq!(blocks[0].type_block, BlockType::PAGE_CLASS);
        }
    }

    mod merging {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn merging_unions_all_three_segments() {
            let left = classifying_pipeline("p", Some("x"));
            let right = classifying_pipeline("p", Some("y"));

            let merged = left + right;
            assert_eq!(merged.pdf_extract.len(), 2);
            assert_eq!(merged.text_filter.len(), 2);
            assert_eq!(merged.deserialize.len(), 2);
        }

        #[test]
        fn merging_keeps_the_left_hand_name() {
            let merged = Pipeline::new("left") + Pipeline::new("right");
            assert_eq!(merged.name, PipelineName::new("left"));
        }

        #[test]
        fn a_pipe_shared_by_both_pipelines_is_kept_once() {
            let shared = LinesToBlocks::pipe("shared");
            let mut left = Pipeline::new("p");
            left.pdf_extract.push(Arc::clone(&shared));
            let mut right = Pipeline::new("p");
            right.pdf_extract.push(shared);

            assert_eq!((left + right).pdf_extract.len(), 1);
        }

        #[test]
        fn merging_an_incomplete_pipeline_can_complete_it() {
            // The real merging case: one layer of the format contributes one segment, another layer
            // the next, and only the sum is complete.
            let mut structured = Pipeline::new("p");
            structured.pdf_extract.push(LinesToBlocks::pipe("extract"));
            let mut semistructured = Pipeline::new("p");
            semistructured
                .text_filter
                .push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            semistructured.deserialize.push(ConstantClassifier::pipe("classify", None));

            assert!(!structured.is_complete());
            assert!(!semistructured.is_complete());
            assert!((structured + semistructured).is_complete());
        }

        #[test]
        fn merging_two_empty_pipelines_yields_an_empty_one() {
            let merged = Pipeline::new("p") + Pipeline::new("p");
            assert!(!merged.is_complete());
            assert_eq!(merged.pdf_extract.len(), 0);
        }
    }

    mod thread_scaling {
        use super::*;

        fn complete_pipeline() -> Pipeline {
            let mut pipeline = Pipeline::new("p");
            pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
            pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            pipeline.deserialize.push(ConstantClassifier::pipe("classify", Some("a")));
            pipeline
        }

        #[test]
        fn a_pipeline_of_plain_rust_pipes_scales() {
            assert!(complete_pipeline().scales_with_threads());
        }

        #[test]
        fn an_empty_pipeline_scales_there_is_nothing_that_does_not() {
            assert!(Pipeline::new("empty").scales_with_threads());
        }

        #[test]
        fn a_single_gil_bound_pipe_stops_the_whole_pipeline() {
            let mut pipeline = complete_pipeline();
            pipeline.pdf_extract.push(GilBoundExtract::pipe("author"));
            assert!(
                !pipeline.scales_with_threads(),
                "the three segments are a chain: a GIL taken in one serializes the whole page"
            );
        }
    }
}
