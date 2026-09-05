//! [`Algorithm`]: page classification, the schedule, and dispatch to a bundle per page class.
//!
//! This is the layer that holds the rest of the engine together: it knows which pipelines classify
//! pages, in what order the page classes are to be processed, and which bundle each of them gets.
//!
//! # Multi-document from the start
//!
//! [`Algorithm::apply`] is [`Algorithm::apply_multidocument`] with one document, not a second
//! implementation. Classification happens **per document** — the finalizer runs once per document,
//! not once over all the pages together — while the schedule works on the **union** of every
//! document's pages.
//!
//! # Results accumulate per page
//!
//! If a page class appears in two steps, the same page is processed twice. Its results
//! **accumulate** in [`PageOutcome::results`] rather than the later step overwriting the earlier
//! one. In the ordinary case — a page class in a single step — the two behaviours coincide; they
//! differ only where overwriting would silently drop data that a step had already produced and that
//! had already fed the next step's `filter_data`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::core::classes::{PdfBlock, TextBlock};
use crate::core::page::{Document, DocumentId, FormatName, Page};
use crate::core::parallelism::{self, Parallelism};
use crate::core::pipeline::bundle::PipelinesBundle;
use crate::core::pipeline::{Extracted, FilterData, Pipeline, PipeError, PipelineName};
use crate::core::schedule::{PageClass, Schedule, ScheduledPage, ScheduleError};
use crate::formats_utils::text_filter::matcher::CompanyMatchInfos;
use crate::core::tracing_setup::log_error;

/// Whoever has the last word on the classification of a document's pages.
///
/// Receives the raw contributions produced by the classification pipelines — which may differ in
/// number from the pages — and must return **exactly one** class per page.
pub trait PageClassFinalize: Send + Sync {
    fn finalize(
        &self,
        classes: Vec<Option<PageClass>>,
    ) -> Result<Vec<Option<PageClass>>, PipeError>;
}

/// A format's finalizer: either the trivial one, or one written by the format author.
#[derive(Clone)]
pub enum PageClassFinalizer {
    /// No finalization: the contributions of the classification pipelines are already the final
    /// classification, one per page.
    Identity,
    /// A finalizer supplied by the format.
    Custom(Arc<dyn PageClassFinalize>),
}

impl PageClassFinalizer {
    pub fn finalize(
        &self,
        classes: Vec<Option<PageClass>>,
    ) -> Result<Vec<Option<PageClass>>, PipeError> {
        match self {
            PageClassFinalizer::Identity => Ok(classes),
            PageClassFinalizer::Custom(finalizer) => finalizer.finalize(classes),
        }
    }
}

impl std::fmt::Debug for PageClassFinalizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageClassFinalizer::Identity => f.write_str("PageClassFinalizer::Identity"),
            PageClassFinalizer::Custom(_) => f.write_str("PageClassFinalizer::Custom(..)"),
        }
    }
}

/// The results of **one** page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PageOutcome {
    /// 1-based page number.
    pub page: u32,
    /// The class the page was scheduled under.
    pub class: PageClass,
    /// The results produced. Empty if the page was skipped because of a non-fatal failure, or if
    /// the pipes had nothing to say.
    pub results: Vec<Extracted>,
}

/// The results of **one** document.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DocumentOutcome {
    pub id: DocumentId,
    pub format: FormatName,
    /// Only the pages actually **scheduled**, in page-number order. An unclassified page enters no
    /// step and does not appear here.
    pub pages: Vec<PageOutcome>,
}

/// A format's extraction algorithm.
#[derive(Debug, Clone)]
pub struct Algorithm {
    format: FormatName,
    page_classify: PipelinesBundle,
    page_class_finalizer: PageClassFinalizer,
    schedule: Schedule,
    bundles: BTreeMap<PageClass, PipelinesBundle>,
}

impl Algorithm {
    /// Builds the algorithm from already-resolved pipelines, applying three validations:
    ///
    /// 1. every classification pipeline has an implementation;
    /// 2. the page classes of the schedule and those of the mapping coincide **exactly**;
    /// 3. there are neither pipelines without an implementation nor implementations never used.
    ///
    /// The second is the one that catches the common mistake: a format whose classifier emits a
    /// class the schedule never names would otherwise drop those pages without a word.
    pub fn new(
        format: impl Into<FormatName>,
        pipelines: BTreeMap<PipelineName, Pipeline>,
        page_classify_pipelines: &[PipelineName],
        page_class_finalizer: PageClassFinalizer,
        schedule: Schedule,
        mapping: BTreeMap<PageClass, Vec<PipelineName>>,
    ) -> Result<Self, AlgorithmError> {
        let known: BTreeSet<PipelineName> = pipelines.keys().cloned().collect();
        let classify_names: BTreeSet<PipelineName> =
            page_classify_pipelines.iter().cloned().collect();

        let unknown: Vec<PipelineName> = classify_names.difference(&known).cloned().collect();
        if !unknown.is_empty() {
            return Err(AlgorithmError::UnknownPageClassifyPipelines { unknown });
        }

        let scheduled_classes = schedule.page_classes();
        let mapped_classes: BTreeSet<PageClass> = mapping.keys().cloned().collect();
        if scheduled_classes != mapped_classes {
            let difference: Vec<PageClass> =
                scheduled_classes.symmetric_difference(&mapped_classes).cloned().collect();
            return Err(AlgorithmError::ScheduleMappingMismatch { difference });
        }

        let mut used: BTreeSet<PipelineName> = classify_names.clone();
        for names in mapping.values() {
            used.extend(names.iter().cloned());
        }
        if used != known {
            return Err(AlgorithmError::PipelineNamesMismatch {
                unmapped: used.difference(&known).cloned().collect(),
                unused: known.difference(&used).cloned().collect(),
            });
        }

        let resolve = |names: &[PipelineName]| -> PipelinesBundle {
            names
                .iter()
                .map(|n| pipelines.get(n).expect("membership just validated above").clone())
                .collect()
        };

        let page_classify = resolve(page_classify_pipelines);
        let bundles = mapping
            .iter()
            .map(|(class, names)| (class.clone(), resolve(names)))
            .collect();

        Ok(Algorithm {
            format: format.into(),
            page_classify,
            page_class_finalizer,
            schedule,
            bundles,
        })
    }

    pub fn format(&self) -> &FormatName {
        &self.format
    }

    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// The bundle of a page class, or the error saying there is none.
    fn bundle(&self, class: &PageClass) -> Result<&PipelinesBundle, AlgorithmError> {
        self.bundles
            .get(class)
            .ok_or_else(|| AlgorithmError::UnmappedPageClass { class: class.clone() })
    }

    /// Classifies the pages of **one** document and applies the finalizer.
    pub fn classify_pages(
        &self,
        doc: &Document,
    ) -> Result<Vec<Option<PageClass>>, AlgorithmError> {
        self.classify_pages_with(doc, Parallelism::SEQUENTIAL)
    }

    /// [`Self::classify_pages`] with the pages spread across threads.
    ///
    /// Classification is worth between a eighth and a hundred-and-fiftieth of what the steps cost,
    /// and it weighs anything only where it is written in Python — where the GIL re-serialises it
    /// and [`PipelinesBundle::scales_with_threads`] degrades it to sequential anyway. The method
    /// exists for the case of a pure Rust classifier over a very large document, not because much
    /// is expected of it in general.
    pub fn classify_pages_with(
        &self,
        doc: &Document,
        parallelism: Parallelism,
    ) -> Result<Vec<Option<PageClass>>, AlgorithmError> {
        // The document is known here and nowhere below: it is what fills the `Report` column of
        // the `.log.csv` for every classification event, none of which could name it on its own.
        let document_span = tracing::info_span!("document", report = %doc.id);
        let _document_guard = document_span.enter();
        let classify_span = tracing::info_span!("classify");
        let _classify_guard = classify_span.enter();

        let mut raw = Vec::with_capacity(doc.pages.len());
        // Contributions are collected per page and validated **afterwards**, in page order: that is
        // what makes what is reported identical to the sequential loop's even when several pages
        // fail together.
        for (page, contributions) in doc.pages.iter().zip(self.classify_each_page(&doc.pages, parallelism)) {
            // A page whose classification fails is left without a class: no step will name it, so
            // nothing will be extracted from it, which is the right outcome — and the other pages
            // of the document keep theirs. The `None` is not optional: the count check below reads
            // one contribution per page.
            let contributions = match contributions {
                Ok(contributions) => contributions,
                Err(error) => {
                    tracing::error!(
                        document = %doc.id,
                        page = page.number,
                        error = log_error(&error),
                        "classification failed: {error} - page left unclassified"
                    );
                    raw.push(None);
                    continue;
                }
            };
            for result in contributions {
                match result {
                    Extracted::PageClass(class) => raw.push(class),
                    other => {
                        return Err(AlgorithmError::NotAPageClassification {
                            document: doc.id.to_string(),
                            page: page.number,
                            found: format!("{other:?}"),
                        });
                    }
                }
            }
        }

        let classified = self.page_class_finalizer.finalize(raw)?;
        if classified.len() != doc.pages.len() {
            return Err(AlgorithmError::ClassificationCountMismatch {
                document: doc.id.to_string(),
                pages: doc.pages.len(),
                classifications: classified.len(),
            });
        }
        tracing::debug!(pages = classified.len(), "page classes assigned");
        Ok(classified)
    }

    /// Classifies several documents. The finalizer runs **per document**, not over the union of the
    /// pages.
    pub fn classify_pages_multidocument(
        &self,
        docs: &[Document],
    ) -> Result<Vec<Vec<Option<PageClass>>>, AlgorithmError> {
        self.classify_pages_multidocument_with(docs, Parallelism::SEQUENTIAL)
    }

    /// [`Self::classify_pages_multidocument`] with per-page parallelism.
    ///
    /// The documents stay sequential with respect to each other: parallelising them *as well* would
    /// nest rayon inside rayon for a gain that running jobs in separate processes already covers
    /// better, since that also gets past the GIL held during loading.
    pub fn classify_pages_multidocument_with(
        &self,
        docs: &[Document],
        parallelism: Parallelism,
    ) -> Result<Vec<Vec<Option<PageClass>>>, AlgorithmError> {
        docs.iter().map(|doc| self.classify_pages_with(doc, parallelism)).collect()
    }

    /// Applies the classification pipelines to **every** page, in parallel when it pays.
    ///
    /// Returns one result per page, in page order; what to do with them is the caller's business.
    /// Three conditions must hold together for work to really be spread — more than one page and
    /// more than one thread ([`Parallelism::is_worth_it`]), pipes that scale with threads, and an
    /// available pool. If one is missing the sequential loop runs, which is the same code:
    /// `classify_one` is written once.
    fn classify_each_page(
        &self,
        pages: &[Page],
        parallelism: Parallelism,
    ) -> Vec<Result<Vec<Extracted>, PipeError>> {
        // The `page` span is needed here just as in the step loop below. Without it every event
        // from the three segments of classification — the majority of the events of a run — reached
        // the `.log.csv` with no page number, and no pipe can know it on its own.
        let classify_one = |page: &Page| {
            let page_span = tracing::info_span!("page", page = page.number);
            page_span.in_scope(|| self.page_classify.apply(page, &FilterData::EMPTY))
        };

        if !parallelism.is_worth_it(pages.len()) || !self.page_classify.scales_with_threads() {
            return pages.iter().map(classify_one).collect();
        }
        let Some(pool) = parallelism::pool(parallelism) else {
            return pages.iter().map(classify_one).collect();
        };
        // A span does not cross a thread boundary by itself: without re-attaching the caller's,
        // `Activity` would lose `run/job/document/classify` on exactly the events that need it
        // most.
        let parent = tracing::Span::current();
        pool.install(|| pages.par_iter().map(|page| parent.in_scope(|| classify_one(page))).collect())
    }

    /// The whole pipeline over a single document — a special case of
    /// [`Algorithm::apply_multidocument`].
    pub fn apply(
        &self,
        doc: &Document,
        companies: &[CompanyMatchInfos],
    ) -> Result<DocumentOutcome, AlgorithmError> {
        self.apply_with(doc, companies, Parallelism::SEQUENTIAL)
    }

    /// [`Self::apply`] with per-page parallelism.
    pub fn apply_with(
        &self,
        doc: &Document,
        companies: &[CompanyMatchInfos],
        parallelism: Parallelism,
    ) -> Result<DocumentOutcome, AlgorithmError> {
        let mut outcomes =
            self.apply_multidocument_with(std::slice::from_ref(doc), companies, parallelism)?;
        Ok(outcomes.remove(0))
    }

    /// Classification **per document**, schedule over the **union** of the pages.
    pub fn apply_multidocument(
        &self,
        docs: &[Document],
        companies: &[CompanyMatchInfos],
    ) -> Result<Vec<DocumentOutcome>, AlgorithmError> {
        self.apply_multidocument_with(docs, companies, Parallelism::SEQUENTIAL)
    }

    /// [`Self::apply_multidocument`] with the pages of a step spread across threads.
    ///
    /// This is where the gain is. Inside the per-page loop of a step lives the text filtering that
    /// accounts for most of the engine's work, in pure Rust and free of the GIL. The pages of one
    /// step are independent by construction —
    /// `pages_of_the_same_step_do_not_see_each_others_results` pins that down — and stay so; the
    /// steps themselves remain sequential, because each reads the results of all the ones before
    /// it.
    ///
    /// The ceiling is Amdahl's: with PyMuPDF loading taking 35-75% of a job's time, a single
    /// document does not go past roughly 1.5x-2.9x however many cores there are. Getting past that
    /// means running whole jobs in separate processes.
    pub fn apply_multidocument_with(
        &self,
        docs: &[Document],
        companies: &[CompanyMatchInfos],
        parallelism: Parallelism,
    ) -> Result<Vec<DocumentOutcome>, AlgorithmError> {
        let classifications = self.classify_pages_multidocument_with(docs, parallelism)?;
        let scheduled = self.schedule.assign(docs, &classifications)?;

        // Pages left out of the results, counted across every step. Atomic because the per-page
        // loop of a step runs on a rayon pool; `Relaxed` is enough, since nothing is ordered
        // against it and it is read once, after every thread has joined.
        let skipped = AtomicUsize::new(0);

        // `(document index, page number) -> (class, accumulated results)`. The key is the index
        // rather than the id because two documents may legitimately share an id.
        let mut per_page: BTreeMap<(usize, u32), (PageClass, Vec<Extracted>)> = BTreeMap::new();
        // The `filter_data` of every step after the first: the accumulation of *all* preceding
        // steps, not only the last one.
        let mut previous: Vec<Extracted> = Vec::new();

        for (step_index, step_pages) in scheduled.iter().enumerate() {
            // The step's orchestration span: `class` and `page` below, and `pipeline`, the three
            // segments and `pipe` inside `bundle.apply`, all nest under it, which is what gives
            // every event produced during this step its place in the `Activity` column of the
            // `.log.csv`.
            let step_span = tracing::info_span!("step", step = step_index);
            let _step_guard = step_span.enter();
            tracing::info!(pages = step_pages.len(), "step started");

            let mut produced_in_this_step: Vec<Extracted> = Vec::new();
            // The pages of a step are already grouped by class in contiguous order, since
            // `Schedule::assign` builds them class by class: `chunk_by` isolates each group without
            // reordering or comparing by hand.
            for class_group in step_pages.chunk_by(|a, b| a.class == b.class) {
                let class_span = tracing::info_span!("class", class = %class_group[0].class);
                let _class_guard = class_span.enter();

                // The pages of a group share the class, hence the bundle and the `filter_data`:
                // resolving them once per group instead of once per page is also what keeps the `?`
                // outside the parallel loop.
                let bundle = self.bundle(&class_group[0].class)?;
                let data = if step_index == 0 {
                    FilterData::TargetCompanies(companies)
                } else {
                    FilterData::Previous(&previous)
                };

                let results_per_page =
                    self.apply_each_page(bundle, class_group, &data, parallelism, &skipped);
                // Recomposition is sequential and in page order, so `produced_in_this_step` and
                // `per_page` come out **identical** to the sequential case, not merely equivalent.
                for (scheduled_page, results) in class_group.iter().zip(results_per_page) {
                    let results = results?;
                    produced_in_this_step.extend(results.iter().cloned());
                    let entry = per_page
                        .entry((scheduled_page.doc_index, scheduled_page.page.number))
                        .or_insert_with(|| (scheduled_page.class.clone(), Vec::new()));
                    entry.1.extend(results);
                }
            }
            tracing::info!(produced = produced_in_this_step.len(), "step finished");
            previous.extend(produced_in_this_step);
        }

        // One event per run, not per page: each skipped page already said so where it happened.
        // This exists so that whoever watches only stderr at the default verbosity cannot finish a
        // run believing everything was read.
        let skipped = skipped.into_inner();
        if skipped > 0 {
            tracing::warn!(skipped, "some pages could not be processed and were left out of the results");
        }

        let mut outcomes: Vec<DocumentOutcome> = docs
            .iter()
            .map(|doc| DocumentOutcome {
                id: doc.id.clone(),
                format: doc.format.clone(),
                pages: Vec::new(),
            })
            .collect();
        // `per_page` is a `BTreeMap`: iterating it yields the pages already ordered by
        // `(document, page number)`, which is the order they are to be written in.
        for ((doc_index, page), (class, results)) in per_page {
            outcomes[doc_index].pages.push(PageOutcome { page, class, results });
        }
        Ok(outcomes)
    }

    /// Applies the bundle to every scheduled page of the group, in parallel when it pays.
    ///
    /// Returns one result per page, in the same order: recomposition — and therefore the order of
    /// the results — stays with the caller, sequentially.
    ///
    /// **Every** failure of a page stays an `Ok(vec![])` with its event and a tick of `skipped`: a
    /// page is the largest thing a page's failure can cost. The severity still distinguishes the
    /// two kinds — a page the pipes declare unreadable is a `warn`, anything else an `error` — but
    /// neither reaches the job.
    ///
    /// The `Result` in the return type therefore has no way of being an `Err` today. It is kept
    /// because the caller recomposes sequentially in page order, and because a genuinely fatal
    /// per-page condition would have nowhere else to go.
    fn apply_each_page(
        &self,
        bundle: &PipelinesBundle,
        pages: &[ScheduledPage<'_>],
        data: &FilterData<'_>,
        parallelism: Parallelism,
        skipped: &AtomicUsize,
    ) -> Vec<Result<Vec<Extracted>, AlgorithmError>> {
        let apply_one = |scheduled_page: &ScheduledPage<'_>| {
            // The document cannot be named any higher than this: a `class` group holds the pages of
            // *every* document that has pages of that class, so which document a page belongs to is
            // known one page at a time. It fills the `Report` column.
            let document_span = tracing::info_span!("document", report = %scheduled_page.doc.id);
            let _document_guard = document_span.enter();
            // This span gives every event produced by this page's pipes the page number, which is
            // the `Page` column of the `.log.csv`. No pipe knows it on its own, and threading it
            // through by hand would mean adding it to every signature.
            let page_span = tracing::info_span!("page", page = scheduled_page.page.number);
            page_span.in_scope(|| match bundle.apply(scheduled_page.page, data) {
                Ok(results) => Ok(results),
                Err(error) if error.is_page_failure() => {
                    // Non-fatal: log and carry on. The document is not spelled out: the enclosing
                    // span carries it, into the `Report` column and into `Activity` alike.
                    tracing::warn!(
                        page = scheduled_page.page.number,
                        error = log_error(&error),
                        "page skipped: {error}"
                    );
                    skipped.fetch_add(1, Ordering::Relaxed);
                    Ok(Vec::new())
                }
                Err(error) => {
                    // `page` is spelled out although the span already carries it: a `.log.csv` row
                    // exists only if the *event* names a page or a coordinate, and this is the one
                    // event a reader will look for.
                    tracing::error!(
                        page = scheduled_page.page.number,
                        error = log_error(&error),
                        "page failed: {error} - page skipped"
                    );
                    skipped.fetch_add(1, Ordering::Relaxed);
                    Ok(Vec::new())
                }
            })
        };

        if !parallelism.is_worth_it(pages.len()) || !bundle.scales_with_threads() {
            return pages.iter().map(apply_one).collect();
        }
        let Some(pool) = parallelism::pool(parallelism) else {
            return pages.iter().map(apply_one).collect();
        };
        // As in `classify_each_page`, the caller's span has to be re-attached on each thread, or
        // `Activity` loses `run/job/document/step/class`.
        let parent = tracing::Span::current();
        pool.install(|| pages.par_iter().map(|page| parent.in_scope(|| apply_one(page))).collect())
    }

    /// Per-segment API: `pdf_extract` alone, for the given page class.
    pub fn apply_pdf_extract(
        &self,
        page: &Page,
        class: &PageClass,
    ) -> Result<Vec<PdfBlock>, AlgorithmError> {
        Ok(self.bundle(class)?.apply_pdf_extract(page)?)
    }

    /// Per-segment API: `pdf_extract` + `text_filter`, for the given page class.
    pub fn apply_text_filter(
        &self,
        page: &Page,
        class: &PageClass,
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, AlgorithmError> {
        Ok(self.bundle(class)?.apply_text_filter(page, data)?)
    }

    /// Per-segment API: the **full** chain of every pipeline of the given page class.
    ///
    /// This is not the same as chaining [`Self::apply_text_filter`] and
    /// [`Self::apply_deserializer`] by hand, and the difference matters when a page class maps
    /// **more than one** pipeline. Chained by hand, the text blocks of *all* the pipelines land in
    /// one heap and every `deserialize` pipe sees them all, including those that are not its own:
    /// two events become four entities. Here each pipeline stays a closed chain, as in the real
    /// pipeline ([`Self::apply`]).
    pub fn apply_deserialize(
        &self,
        page: &Page,
        class: &PageClass,
        data: &FilterData<'_>,
    ) -> Result<Vec<Extracted>, AlgorithmError> {
        Ok(self.bundle(class)?.apply_deserialize(page, data)?)
    }

    /// Per-segment API: `deserialize` alone, starting from text blocks that are already prepared.
    ///
    /// Taking blocks as input rather than starting again from the page is what makes the three
    /// methods genuinely decompose the chain into three composable pieces, instead of re-running
    /// the upstream segments twice.
    pub fn apply_deserializer(
        &self,
        blocks: &[TextBlock],
        class: &PageClass,
    ) -> Result<Vec<Extracted>, AlgorithmError> {
        let mut out = Vec::new();
        for pipeline in self.bundle(class)?.iter() {
            // This bypasses `Pipeline::apply`, which opens the span itself: the per-segment API
            // calls the segment directly, so `pipeline[<name>]` has to be opened here by hand.
            let pipeline_span = tracing::info_span!("pipeline", pipeline = %pipeline.name);
            let _pipeline_guard = pipeline_span.enter();
            out.extend(pipeline.deserialize.apply(blocks)?);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AlgorithmError {
    #[error(transparent)]
    Pipe(#[from] PipeError),
    #[error(transparent)]
    Schedule(#[from] ScheduleError),
    #[error("some page classify pipelines have no mapping to a pipeline implementation: {unknown:?}")]
    UnknownPageClassifyPipelines { unknown: Vec<PipelineName> },
    #[error("page classes in the schedule have to be mapped to pipeline names; the difference is {difference:?}")]
    ScheduleMappingMismatch { difference: Vec<PageClass> },
    #[error(
        "there are pipeline names not mapped to an implementation or mapped and never used. Unmapped: {unmapped:?} Not used: {unused:?}"
    )]
    PipelineNamesMismatch { unmapped: Vec<PipelineName>, unused: Vec<PipelineName> },
    #[error(
        "document `{document}` has {pages} pages but the finalizer returned {classifications} classifications"
    )]
    ClassificationCountMismatch { document: String, pages: usize, classifications: usize },
    #[error("page {page} of document `{document}`: a page classify pipeline returned {found}, not a page class")]
    NotAPageClassification { document: String, page: u32, found: String },
    #[error("page class `{class}` has no pipelines bundle")]
    UnmappedPageClass { class: PageClass },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::BlockType;
    use crate::core::pipeline::TextFilterPipe;
    use crate::core::pipeline::segment::test_pipes::*;
    use crate::core::schedule::ScheduleStep;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::text_filter::matcher::TargetCompanyInput;

    fn page(number: u32, texts: &[&str]) -> Page {
        let lines = texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let y = i as f32 * 10.0;
                PdfLine::new("Arial", 10.0, t, (0.0, y, 10.0, y + 10.0))
            })
            .collect();
        Page::new(number, (100.0, 100.0), lines, vec![])
    }

    fn doc(id: &str, pages: Vec<Page>) -> Document {
        Document::new(id, "FMT", pages)
    }

    fn companies() -> Vec<CompanyMatchInfos> {
        CompanyMatchInfos::compile_from_target_companies(vec![TargetCompanyInput {
            name: "Acme".to_string(),
            regexs: vec![],
            symbols: vec![],
            buds: vec![],
        }])
        .expect("fixed, valid input")
    }

    /// A complete pipeline that classifies every block as `class`.
    fn classifying_pipeline(name: &str, class: Option<&str>) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(ConstantClassifier::pipe("classify", class));
        pipeline
    }

    /// A complete pipeline that deposits one promise per block.
    fn promising_pipeline(name: &str, id: &str) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(PromiseDepositor::pipe("promise", id));
        pipeline
    }

    fn step(classes: &[&str]) -> ScheduleStep {
        classes.iter().copied().collect()
    }

    /// The minimal algorithm most tests use: the `classify` pipeline classifies everything as
    /// `"a"`, the `work` pipeline processes the pages of class `"a"`.
    fn simple_algorithm() -> Algorithm {
        Algorithm::new(
            "FMT",
            BTreeMap::from([
                (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                (PipelineName::new("work"), promising_pipeline("work", "id")),
            ]),
            &[PipelineName::new("classify")],
            PageClassFinalizer::Identity,
            Schedule::new(vec![step(&["a"])]),
            BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
        )
        .expect("fixture is consistent")
    }

    mod construction_validation {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_consistent_configuration_is_accepted() {
            assert_eq!(simple_algorithm().format(), &FormatName::new("FMT"));
        }

        #[test]
        fn a_page_classify_pipeline_without_implementation_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([(PipelineName::new("work"), promising_pipeline("work", "id"))]),
                &[PipelineName::new("ghost")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::UnknownPageClassifyPipelines {
                    unknown: vec![PipelineName::new("ghost")]
                }
            );
        }

        #[test]
        fn a_page_class_in_the_schedule_but_not_in_the_mapping_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a", "b"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::ScheduleMappingMismatch {
                    difference: vec![PageClass::new("b")]
                }
            );
        }

        #[test]
        fn a_page_class_in_the_mapping_but_not_in_the_schedule_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([
                    (PageClass::new("a"), vec![PipelineName::new("work")]),
                    (PageClass::new("b"), vec![PipelineName::new("work")]),
                ]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::ScheduleMappingMismatch {
                    difference: vec![PageClass::new("b")]
                }
            );
        }

        #[test]
        fn a_pipeline_implementation_nobody_uses_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                    (PipelineName::new("idle"), promising_pipeline("idle", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::PipelineNamesMismatch {
                    unmapped: vec![],
                    unused: vec![PipelineName::new("idle")],
                }
            );
        }

        #[test]
        fn a_mapped_pipeline_without_implementation_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([(
                    PipelineName::new("classify"),
                    classifying_pipeline("classify", Some("a")),
                )]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("ghost")])]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::PipelineNamesMismatch {
                    unmapped: vec![PipelineName::new("ghost")],
                    unused: vec![],
                }
            );
        }

        #[test]
        fn a_pipeline_may_serve_both_classification_and_a_page_class() {
            // None of the three validations forbids this: the pipeline appears both among the
            // classifying ones and in the mapping.
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([(
                    PipelineName::new("both"),
                    classifying_pipeline("both", Some("a")),
                )]),
                &[PipelineName::new("both")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("both")])]),
            );
            assert!(algorithm.is_ok());
        }
    }

    mod classification {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_page_gets_the_class_its_pipelines_produced() {
            let algorithm = simple_algorithm();
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"])]);
            assert_eq!(
                algorithm.classify_pages(&document).unwrap(),
                vec![Some(PageClass::new("a")), Some(PageClass::new("a"))]
            );
        }

        #[test]
        fn a_page_the_pipelines_could_not_classify_stays_unclassified() {
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", None)),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();
            assert_eq!(algorithm.classify_pages(&doc("d", vec![page(1, &["x"])])).unwrap(), vec![None]);
        }

        #[test]
        fn a_document_with_no_pages_classifies_to_nothing() {
            assert!(simple_algorithm().classify_pages(&doc("d", vec![])).unwrap().is_empty());
        }

        #[test]
        fn one_classification_per_page_is_required_after_the_finalizer() {
            // Two lines per page means two contributions per page: with the identity finalizer the
            // count does not add up, and that is an error.
            let algorithm = simple_algorithm();
            let err = algorithm.classify_pages(&doc("d", vec![page(1, &["x", "y"])])).unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::ClassificationCountMismatch {
                    document: "d".to_string(),
                    pages: 1,
                    classifications: 2,
                }
            );
        }

        #[test]
        fn a_custom_finalizer_can_reconcile_the_count() {
            struct KeepFirstPerPage;
            impl PageClassFinalize for KeepFirstPerPage {
                fn finalize(
                    &self,
                    classes: Vec<Option<PageClass>>,
                ) -> Result<Vec<Option<PageClass>>, PipeError> {
                    Ok(classes.into_iter().step_by(2).collect())
                }
            }

            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Custom(Arc::new(KeepFirstPerPage)),
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            assert_eq!(
                algorithm.classify_pages(&doc("d", vec![page(1, &["x", "y"])])).unwrap(),
                vec![Some(PageClass::new("a"))]
            );
        }

        #[test]
        fn a_failing_finalizer_surfaces_its_error() {
            struct AlwaysFails;
            impl PageClassFinalize for AlwaysFails {
                fn finalize(
                    &self,
                    _classes: Vec<Option<PageClass>>,
                ) -> Result<Vec<Option<PageClass>>, PipeError> {
                    Err(PipeError::author("classify", "compute_page_class", "KeyError"))
                }
            }

            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Custom(Arc::new(AlwaysFails)),
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let err = algorithm.classify_pages(&doc("d", vec![page(1, &["x"])])).unwrap_err();
            assert!(matches!(err, AlgorithmError::Pipe(PipeError::Author { .. })));
        }

        #[test]
        fn a_non_classification_result_from_a_classify_pipeline_is_an_error() {
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    // The classification pipeline deposits promises instead of classifying.
                    (PipelineName::new("classify"), promising_pipeline("classify", "id")),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let err = algorithm.classify_pages(&doc("d", vec![page(1, &["x"])])).unwrap_err();
            assert!(matches!(err, AlgorithmError::NotAPageClassification { page: 1, .. }));
        }

        #[test]
        fn each_document_is_finalized_on_its_own() {
            // The finalizer records how many classifications it receives per call: two one-page
            // documents must produce two calls of one, not one call of two.
            struct RecordingFinalizer(std::sync::Mutex<Vec<usize>>);
            impl PageClassFinalize for RecordingFinalizer {
                fn finalize(
                    &self,
                    classes: Vec<Option<PageClass>>,
                ) -> Result<Vec<Option<PageClass>>, PipeError> {
                    self.0.lock().expect("test-only mutex").push(classes.len());
                    Ok(classes)
                }
            }

            let finalizer = Arc::new(RecordingFinalizer(std::sync::Mutex::new(Vec::new())));
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Custom(Arc::clone(&finalizer) as Arc<dyn PageClassFinalize>),
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let docs = vec![doc("one", vec![page(1, &["x"])]), doc("two", vec![page(1, &["y"])])];
            algorithm.classify_pages_multidocument(&docs).unwrap();
            assert_eq!(*finalizer.0.lock().expect("test-only mutex"), vec![1, 1]);
        }
    }

    mod single_document_application {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_scheduled_page_produces_an_outcome() {
            let algorithm = simple_algorithm();
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"])]);
            let outcome = algorithm.apply(&document, &companies()).unwrap();

            assert_eq!(outcome.id, DocumentId::new("d"));
            assert_eq!(outcome.format, FormatName::new("FMT"));
            let numbers: Vec<u32> = outcome.pages.iter().map(|p| p.page).collect();
            assert_eq!(numbers, vec![1, 2]);
        }

        #[test]
        fn the_outcome_carries_the_class_the_page_was_scheduled_with() {
            let outcome =
                simple_algorithm().apply(&doc("d", vec![page(1, &["x"])]), &companies()).unwrap();
            assert_eq!(outcome.pages[0].class, PageClass::new("a"));
        }

        #[test]
        fn an_unclassified_page_produces_no_outcome_at_all() {
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", None)),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let outcome =
                algorithm.apply(&doc("d", vec![page(1, &["x"])]), &companies()).unwrap();
            assert!(outcome.pages.is_empty());
        }

        #[test]
        fn a_document_with_no_pages_yields_an_empty_outcome() {
            let outcome = simple_algorithm().apply(&doc("d", vec![]), &companies()).unwrap();
            assert!(outcome.pages.is_empty());
        }

        #[test]
        fn the_pages_of_an_outcome_are_ordered_by_page_number() {
            // The schedule visits them by class, not by number: the final order must still be page
            // order.
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), alternating_classifier()),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                // `b` before `a`: page 2, of class `b`, is scheduled first.
                Schedule::new(vec![step(&["b", "a"])]),
                BTreeMap::from([
                    (PageClass::new("a"), vec![PipelineName::new("work")]),
                    (PageClass::new("b"), vec![PipelineName::new("work")]),
                ]),
            )
            .unwrap();

            let document = doc("d", vec![page(1, &["a"]), page(2, &["b"])]);
            let outcome = algorithm.apply(&document, &companies()).unwrap();
            let numbers: Vec<u32> = outcome.pages.iter().map(|p| p.page).collect();
            assert_eq!(numbers, vec![1, 2]);
        }
    }

    /// Classifies a page by the text of its first line: `"a"` gives class `a`, anything else class
    /// `b`.
    fn alternating_classifier() -> Pipeline {
        struct ByFirstLine;
        impl crate::core::pipeline::DeserializePipe for ByFirstLine {
            fn name(&self) -> &str {
                "by-first-line"
            }
            fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
                let class = match block.content.as_str() {
                    Some("a") => PageClass::new("a"),
                    _ => PageClass::new("b"),
                };
                Ok(vec![Extracted::PageClass(Some(class))])
            }
        }

        let mut pipeline = Pipeline::new("classify");
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(Arc::new(ByFirstLine));
        pipeline
    }

    mod filter_data_across_steps {
        use super::*;
        use pretty_assertions::assert_eq;

        /// A two-step algorithm: page 1 (class `a`) in the first, page 2 (class `b`) in the second.
        /// The recording filter is shared, so what it saw can be read back.
        fn two_step_algorithm(filter: Arc<RecordingFilter>) -> Algorithm {
            let mut work_a = Pipeline::new("work_a");
            work_a.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work_a.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            work_a.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            let mut work_b = Pipeline::new("work_b");
            work_b.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work_b.text_filter.push(filter as Arc<dyn TextFilterPipe>);
            work_b.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), alternating_classifier()),
                    (PipelineName::new("work_a"), work_a),
                    (PipelineName::new("work_b"), work_b),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"]), step(&["b"])]),
                BTreeMap::from([
                    (PageClass::new("a"), vec![PipelineName::new("work_a")]),
                    (PageClass::new("b"), vec![PipelineName::new("work_b")]),
                ]),
            )
            .unwrap()
        }

        #[test]
        fn the_first_step_sees_the_target_companies_and_the_next_one_sees_the_results() {
            let filter = RecordingFilter::new("filter");
            let algorithm = two_step_algorithm(Arc::clone(&filter));
            let document = doc("d", vec![page(1, &["a"]), page(2, &["b"])]);

            algorithm.apply(&document, &companies()).unwrap();
            // First call (step 0): one target company, zero previous results.
            // Second call (step 1): zero target companies, the result of step 0.
            assert_eq!(filter.seen(), vec![(1, 0), (0, 1)]);
        }

        #[test]
        fn the_results_of_all_earlier_steps_accumulate_not_just_the_last_one() {
            let filter = RecordingFilter::new("filter");
            let mut work_c = Pipeline::new("work_c");
            work_c.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work_c.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            work_c.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            let mut work_ab = Pipeline::new("work_ab");
            work_ab.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work_ab.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            work_ab.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), alternating_classifier()),
                    (PipelineName::new("work_ab"), work_ab),
                    (PipelineName::new("work_c"), work_c),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"]), step(&["b"])]),
                BTreeMap::from([
                    (PageClass::new("a"), vec![PipelineName::new("work_ab")]),
                    (PageClass::new("b"), vec![PipelineName::new("work_c")]),
                ]),
            )
            .unwrap();

            // Two pages of class `a` in the first step, one of class `b` in the second: the `b`
            // page must see both previous results.
            let document = doc("d", vec![page(1, &["a"]), page(2, &["a"]), page(3, &["b"])]);
            algorithm.apply(&document, &companies()).unwrap();
            assert_eq!(filter.seen(), vec![(1, 0), (1, 0), (0, 2)]);
        }

        #[test]
        fn pages_of_the_same_step_do_not_see_each_others_results() {
            // The results of a step enter the `filter_data` only at the following step.
            let filter = RecordingFilter::new("filter");
            let algorithm = two_step_algorithm(Arc::clone(&filter));
            let document = doc("d", vec![page(1, &["a"]), page(2, &["a"])]);

            algorithm.apply(&document, &companies()).unwrap();
            assert_eq!(filter.seen(), vec![(1, 0), (1, 0)]);
        }
    }

    mod multidocument {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn each_document_gets_its_own_outcome_in_input_order() {
            let algorithm = simple_algorithm();
            let docs = vec![
                doc("second", vec![page(1, &["x"])]),
                doc("first", vec![page(1, &["y"])]),
            ];
            let outcomes = algorithm.apply_multidocument(&docs, &companies()).unwrap();
            let ids: Vec<String> = outcomes.iter().map(|o| o.id.to_string()).collect();
            assert_eq!(ids, vec!["second", "first"]);
        }

        #[test]
        fn no_documents_yields_no_outcomes() {
            assert!(simple_algorithm().apply_multidocument(&[], &companies()).unwrap().is_empty());
        }

        #[test]
        fn two_documents_with_the_same_id_stay_separate() {
            let algorithm = simple_algorithm();
            let docs = vec![doc("same", vec![page(1, &["x"])]), doc("same", vec![page(1, &["y"])])];
            let outcomes = algorithm.apply_multidocument(&docs, &companies()).unwrap();
            assert_eq!(outcomes.len(), 2);
            assert_eq!(outcomes[0].pages.len(), 1);
            assert_eq!(outcomes[1].pages.len(), 1);
        }

        #[test]
        fn the_schedule_spans_the_union_of_the_pages_of_every_document() {
            // The filter sees one call per page, across *all* documents, within the same step: the
            // schedule does not start over for each document.
            let filter = RecordingFilter::new("filter");
            let mut work = Pipeline::new("work");
            work.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            work.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), work),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let docs = vec![doc("one", vec![page(1, &["x"])]), doc("two", vec![page(1, &["y"])])];
            algorithm.apply_multidocument(&docs, &companies()).unwrap();
            assert_eq!(filter.seen(), vec![(1, 0), (1, 0)]);
        }
    }

    mod page_failures {
        use super::*;
        use pretty_assertions::assert_eq;

        fn algorithm_whose_work_pipeline_fails(page_failure: bool) -> Algorithm {
            let mut work = Pipeline::new("work");
            work.pdf_extract.push(if page_failure {
                FailingExtract::page_parse("skipper")
            } else {
                FailingExtract::fatal("boom")
            });
            work.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            work.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), work),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap()
        }

        #[test]
        fn a_page_failure_is_absorbed_and_the_page_keeps_an_empty_outcome() {
            let algorithm = algorithm_whose_work_pipeline_fails(true);
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"])]);
            let outcome = algorithm.apply(&document, &companies()).unwrap();

            assert_eq!(outcome.pages.len(), 2);
            assert!(outcome.pages.iter().all(|p| p.results.is_empty()));
        }

        #[test]
        fn a_failure_of_any_other_kind_costs_the_page_too() {
            // The severity differs — this one is an `error`, the absorbed one a `warn` — but the
            // cost is the same: a page. Only a configuration problem, which belongs to no page,
            // stops the run.
            let algorithm = algorithm_whose_work_pipeline_fails(false);
            let outcome = algorithm.apply(&doc("d", vec![page(1, &["x"])]), &companies()).unwrap();
            assert_eq!(outcome.pages.len(), 1);
            assert!(outcome.pages[0].results.is_empty());
        }
    }

    /// A failure belongs to the smallest thing that contains it. A page that cannot be read costs
    /// that page; the pages beside it, the document and the run are unaffected. What still stops a
    /// run is configuration — a class nobody maps, a classifier returning the wrong kind of thing —
    /// because containing that would produce an empty run full of warnings instead of one readable
    /// error.
    mod containment {
        use super::*;
        use pretty_assertions::assert_eq;

        /// A work pipeline that explodes — fatally, not absorbably — on the named pages only.
        fn algorithm_failing_on_pages(pages: &'static [u32]) -> Algorithm {
            let mut work = Pipeline::new("work");
            work.pdf_extract.push(FailingOnPages::fatal("boom", pages));
            work.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            work.deserialize.push(PromiseDepositor::pipe("promise", "id"));
            Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), work),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .expect("fixture is consistent")
        }

        fn classifier_failing_on_pages(pages: &'static [u32]) -> Algorithm {
            let mut classify = Pipeline::new("classify");
            classify.pdf_extract.push(FailingOnPages::fatal("boom", pages));
            classify.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            classify.deserialize.push(ConstantClassifier::pipe("classify", Some("a")));
            Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classify),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .expect("fixture is consistent")
        }

        #[test]
        fn one_doomed_page_does_not_stop_the_others_of_its_step() {
            let algorithm = algorithm_failing_on_pages(&[2]);
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"]), page(3, &["z"])]);
            let outcome = algorithm.apply(&document, &companies()).unwrap();

            let produced: Vec<_> = outcome.pages.iter().map(|p| (p.page, p.results.len())).collect();
            assert_eq!(produced, vec![(1, 1), (2, 0), (3, 1)]);
        }

        #[test]
        fn the_multidocument_entry_point_returns_ok_with_pages_missing() {
            let algorithm = algorithm_failing_on_pages(&[1]);
            let documents = [doc("d", vec![page(1, &["x"])])];
            assert!(algorithm.apply_multidocument(&documents, &companies()).is_ok());
        }

        #[test]
        fn a_document_whose_every_page_fails_still_produces_an_outcome() {
            let algorithm = algorithm_failing_on_pages(&[1, 2]);
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"])]);
            let outcome = algorithm.apply(&document, &companies()).unwrap();
            assert!(outcome.pages.iter().all(|p| p.results.is_empty()));
        }

        #[test]
        fn a_classification_that_fails_leaves_that_page_without_a_class() {
            let algorithm = classifier_failing_on_pages(&[2]);
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"]), page(3, &["z"])]);
            let classes = algorithm.classify_pages(&document).unwrap();

            assert_eq!(classes, vec![Some(PageClass::new("a")), None, Some(PageClass::new("a"))]);
        }

        #[test]
        fn the_contribution_count_still_matches_the_page_count() {
            // What makes the `None` mandatory: the check comparing classifications to pages runs
            // on this vector, and a missing entry would turn a skipped page into a fatal mismatch.
            let algorithm = classifier_failing_on_pages(&[1, 3]);
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"]), page(3, &["z"])]);
            assert_eq!(algorithm.classify_pages(&document).unwrap().len(), document.pages.len());
        }

        #[test]
        fn an_unclassified_page_is_simply_never_scheduled() {
            let algorithm = classifier_failing_on_pages(&[2]);
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"])]);
            let outcome = algorithm.apply(&document, &companies()).unwrap();

            assert_eq!(outcome.pages.iter().map(|p| p.page).collect::<Vec<_>>(), vec![1]);
        }

        #[test]
        fn a_class_the_schedule_never_heard_of_still_stops_the_run() {
            // Configuration, not data: it belongs to no page, so there is nothing to contain it in.
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("b"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .expect("fixture is consistent");

            let err = algorithm.apply(&doc("d", vec![page(1, &["x"])]), &companies()).unwrap_err();
            assert!(matches!(err, AlgorithmError::Schedule(_)), "{err:?}");
        }

        #[test]
        fn a_classifier_returning_the_wrong_kind_of_thing_still_stops_the_run() {
            // Also configuration: a classification pipeline that deserializes something which is
            // not a page class is a format that cannot work on any page.
            let mut classify = Pipeline::new("classify");
            classify.pdf_extract.push(LinesToBlocks::pipe("lines"));
            classify.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            classify.deserialize.push(PromiseDepositor::pipe("promise", "id"));
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classify),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .expect("fixture is consistent");

            let err = algorithm.classify_pages(&doc("d", vec![page(1, &["x"])])).unwrap_err();
            assert!(matches!(err, AlgorithmError::NotAPageClassification { .. }), "{err:?}");
        }
    }

    mod results_accumulate_across_steps {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_page_class_named_by_two_steps_keeps_the_results_of_both() {
            // The second step accumulates onto the first's results instead of overwriting them.
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"]), step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let outcome =
                algorithm.apply(&doc("d", vec![page(1, &["x"])]), &companies()).unwrap();
            assert_eq!(outcome.pages.len(), 1);
            assert_eq!(outcome.pages[0].results.len(), 2);
        }
    }

    mod per_segment_api {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn apply_pdf_extract_runs_only_the_first_segment_of_the_class_bundle() {
            let algorithm = simple_algorithm();
            let blocks =
                algorithm.apply_pdf_extract(&page(1, &["x", "y"]), &PageClass::new("a")).unwrap();
            let contents: Vec<&str> =
                blocks.iter().map(|b| b.content.as_str().unwrap()).collect();
            assert_eq!(contents, vec!["x", "y"]);
        }

        #[test]
        fn apply_text_filter_runs_the_first_two_segments() {
            let algorithm = simple_algorithm();
            let blocks = algorithm
                .apply_text_filter(&page(1, &["x"]), &PageClass::new("a"), &FilterData::EMPTY)
                .unwrap();
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].type_block, BlockType::PAGE_CLASS);
        }

        #[test]
        fn apply_deserializer_starts_from_ready_made_text_blocks() {
            let algorithm = simple_algorithm();
            let blocks = vec![TextBlock::from_content(
                BlockType::PAGE_CLASS,
                std::collections::BTreeMap::new(),
                "content",
            )];
            let out = algorithm.apply_deserializer(&blocks, &PageClass::new("a")).unwrap();
            assert_eq!(out.len(), 1);
            assert!(out[0].as_promises().is_some());
        }

        /// Regression: when a page class maps **two** pipelines, the full chain is not the
        /// hand-made composition of `apply_text_filter` and `apply_deserializer`. Chained by hand,
        /// every `deserialize` pipe also sees the other pipeline's blocks and two events become
        /// four entities.
        #[test]
        fn apply_deserialize_keeps_each_pipeline_a_closed_chain() {
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("one"), promising_pipeline("one", "id-one")),
                    (PipelineName::new("two"), promising_pipeline("two", "id-two")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(
                    PageClass::new("a"),
                    vec![PipelineName::new("one"), PipelineName::new("two")],
                )]),
            )
            .expect("fixture is consistent");

            let page = page(1, &["x"]);
            let class = PageClass::new("a");

            let chained = algorithm.apply_deserialize(&page, &class, &FilterData::EMPTY).unwrap();
            assert_eq!(chained.len(), 2, "una entita' per pipeline, non il prodotto incrociato");

            // The hand-made composition is precisely what must not be done: it is exercised here to
            // pin the difference down, not because it is an acceptable alternative.
            let blocks =
                algorithm.apply_text_filter(&page, &class, &FilterData::EMPTY).unwrap();
            let crossed = algorithm.apply_deserializer(&blocks, &class).unwrap();
            assert_eq!(crossed.len(), 4, "la composizione a mano incrocia le pipeline");
        }

        #[test]
        fn an_unmapped_page_class_is_an_error_in_all_three() {
            let algorithm = simple_algorithm();
            let ghost = PageClass::new("ghost");
            let expected = AlgorithmError::UnmappedPageClass { class: ghost.clone() };

            assert_eq!(algorithm.apply_pdf_extract(&page(1, &["x"]), &ghost).unwrap_err(), expected);
            assert_eq!(
                algorithm
                    .apply_text_filter(&page(1, &["x"]), &ghost, &FilterData::EMPTY)
                    .unwrap_err(),
                expected
            );
            assert_eq!(algorithm.apply_deserializer(&[], &ghost).unwrap_err(), expected);
        }
    }

    /// `Vec<DocumentOutcome>` is the payload a worker job sends back to the parent — the job's
    /// entire result. If something is lost here, the parent writes an incomplete CSV without
    /// anything failing.
    mod serde_round_trip {
        use super::*;
        use crate::core::pipeline::Extracted;
        use crate::output::classes::fund::Fund;

        fn outcomes() -> Vec<DocumentOutcome> {
            vec![
                DocumentOutcome {
                    id: DocumentId::new("first-report"),
                    format: FormatName::new("FMT-A"),
                    pages: vec![
                        PageOutcome {
                            page: 1,
                            class: PageClass::new("investments"),
                            results: vec![Extracted::Fund(Fund::new("Alpha Fund")), Extracted::PageClass(None)],
                        },
                        PageOutcome { page: 7, class: PageClass::new("assets"), results: vec![] },
                    ],
                },
                DocumentOutcome {
                    id: DocumentId::new("second-report"),
                    format: FormatName::new("FMT-B"),
                    pages: vec![PageOutcome {
                        page: 353,
                        class: PageClass::new("investments"),
                        results: vec![Extracted::Fund(Fund::new("Beta Fund"))],
                    }],
                },
            ]
        }

        fn round_trip(v: &[DocumentOutcome]) -> Vec<DocumentOutcome> {
            let json = serde_json::to_string(v).expect("a job payload must serialize");
            serde_json::from_str(&json).expect("a serialized job payload must deserialize back")
        }

        #[test]
        fn a_multi_document_payload_survives_a_json_round_trip_unchanged() {
            let v = outcomes();
            assert_eq!(round_trip(&v), v);
        }

        /// A job that extracted nothing is a legitimate outcome, not an error: it must cross the
        /// boundary as such rather than become a malformed payload.
        #[test]
        fn an_empty_payload_survives() {
            let v: Vec<DocumentOutcome> = vec![];
            assert_eq!(round_trip(&v), v);
        }

        /// The order of the documents, and of the pages within each, is why the parent can
        /// concatenate the results and obtain the same file as the sequential case.
        #[test]
        fn the_order_of_documents_and_of_pages_is_preserved() {
            let restored = round_trip(&outcomes());
            let ids: Vec<&str> = restored.iter().map(|o| o.id.as_str()).collect();
            assert_eq!(ids, ["first-report", "second-report"]);
            let pages: Vec<u32> = restored[0].pages.iter().map(|p| p.page).collect();
            assert_eq!(pages, [1, 7]);
        }

        /// The real page numbers of large reports fit in a `u32`, but they travel through JSON,
        /// where numbers are `f64`: this is the proof that no truncation happens.
        #[test]
        fn a_high_page_number_survives_the_json_number_representation() {
            let mut v = outcomes();
            v[0].pages[0].page = 1_824;
            assert_eq!(round_trip(&v)[0].pages[0].page, 1_824);
        }
    }

    mod error_messages {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_unmapped_page_class_names_the_class() {
            let err = AlgorithmError::UnmappedPageClass { class: PageClass::new("ghost") };
            assert_eq!(err.to_string(), "page class `ghost` has no pipelines bundle");
        }

        #[test]
        fn a_classification_count_mismatch_reports_both_counts() {
            let err = AlgorithmError::ClassificationCountMismatch {
                document: "d".to_string(),
                pages: 3,
                classifications: 5,
            };
            assert_eq!(
                err.to_string(),
                "document `d` has 3 pages but the finalizer returned 5 classifications"
            );
        }

        #[test]
        fn a_pipe_error_is_forwarded_verbatim() {
            let pipe_error = PipeError::extraction("p", "boom");
            let err: AlgorithmError = pipe_error.clone().into();
            assert_eq!(err.to_string(), pipe_error.to_string());
        }

        #[test]
        fn a_schedule_error_is_forwarded_verbatim() {
            let schedule_error = ScheduleError::UnknownPageClass {
                document: "d".to_string(),
                class: PageClass::new("ghost"),
            };
            let err: AlgorithmError = schedule_error.clone().into();
            assert_eq!(err.to_string(), schedule_error.to_string());
        }
    }

    /// The pages of a step — and of classification — spread across threads.
    ///
    /// Every test here has the same criterion: **the result must not change**. Parallelism is an
    /// execution detail, and the only test that really observes it is
    /// `two_pages_of_the_same_step_really_run_on_two_threads`; the others just compare sequential
    /// against parallel.
    mod parallel_pages {
        use super::*;

        /// A document with `count` pages, each carrying **one** distinguishable line.
        ///
        /// The single line is not cosmetic: `classifying_pipeline` produces one class contribution
        /// per *block*, and a block comes from a line — two lines per page would give two classes
        /// per page and the identity finalizer would refuse them.
        fn wide_document(count: u32) -> Document {
            let pages = (1..=count).map(|n| page(n, &[&format!("page {n}")])).collect();
            doc("wide", pages)
        }

        fn parallel() -> Parallelism {
            Parallelism::pages(4)
        }

        mod equivalence {
            use super::*;

            #[test]
            fn the_outcome_of_a_wide_document_is_identical_sequential_or_parallel() {
                let algorithm = simple_algorithm();
                let document = wide_document(64);
                let sequential = algorithm.apply(&document, &companies()).unwrap();
                let concurrent = algorithm.apply_with(&document, &companies(), parallel()).unwrap();
                assert_eq!(sequential, concurrent);
            }

            #[test]
            fn a_multi_step_schedule_accumulates_the_same_results_either_way() {
                // Two steps, two classes: the second step reads the first's results, and the order
                // in which it reads them is what parallelism must not be able to change.
                let algorithm = Algorithm::new(
                    "FMT",
                    BTreeMap::from([
                        (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                        (PipelineName::new("first"), promising_pipeline("first", "one")),
                        (PipelineName::new("second"), promising_pipeline("second", "two")),
                    ]),
                    &[PipelineName::new("classify")],
                    PageClassFinalizer::Identity,
                    Schedule::new(vec![step(&["a"]), step(&["a"])]),
                    BTreeMap::from([(
                        PageClass::new("a"),
                        vec![PipelineName::new("first"), PipelineName::new("second")],
                    )]),
                )
                .expect("fixture is consistent");
                let document = wide_document(32);
                assert_eq!(
                    algorithm.apply(&document, &companies()).unwrap(),
                    algorithm.apply_with(&document, &companies(), parallel()).unwrap()
                );
            }

            #[test]
            fn several_documents_stay_separate_and_ordered_under_parallelism() {
                let algorithm = simple_algorithm();
                let docs = vec![wide_document(8), doc("other", vec![page(1, &["x"]), page(2, &["y"])])];
                assert_eq!(
                    algorithm.apply_multidocument(&docs, &companies()).unwrap(),
                    algorithm.apply_multidocument_with(&docs, &companies(), parallel()).unwrap()
                );
            }

            #[test]
            fn classification_gives_the_same_classes_either_way() {
                let algorithm = simple_algorithm();
                let document = wide_document(48);
                assert_eq!(
                    algorithm.classify_pages(&document).unwrap(),
                    algorithm.classify_pages_with(&document, parallel()).unwrap()
                );
            }

            #[test]
            fn a_document_without_pages_is_not_a_special_case() {
                let algorithm = simple_algorithm();
                let empty = doc("empty", vec![]);
                assert_eq!(
                    algorithm.apply(&empty, &companies()).unwrap(),
                    algorithm.apply_with(&empty, &companies(), parallel()).unwrap()
                );
            }
        }

        mod failures {
            use super::*;

            /// With several pages failing, the surviving pages and their order are the same
            /// whether the step ran on one thread or on many: containment does not depend on how
            /// the work was spread.
            #[test]
            fn the_pages_that_survive_are_the_same_either_way() {
                let mut work = Pipeline::new("work");
                work.pdf_extract.push(FailingOnPages::fatal("boom", &[7, 3, 9]));
                work.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
                work.deserialize.push(PromiseDepositor::pipe("promise", "id"));
                let algorithm = Algorithm::new(
                    "FMT",
                    BTreeMap::from([
                        (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                        (PipelineName::new("work"), work),
                    ]),
                    &[PipelineName::new("classify")],
                    PageClassFinalizer::Identity,
                    Schedule::new(vec![step(&["a"])]),
                    BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
                )
                .expect("fixture is consistent");
                let document = wide_document(12);

                let sequential = algorithm.apply(&document, &companies()).unwrap();
                let concurrent = algorithm.apply_with(&document, &companies(), parallel()).unwrap();
                assert_eq!(sequential, concurrent);

                let empty: Vec<_> =
                    concurrent.pages.iter().filter(|p| p.results.is_empty()).map(|p| p.page).collect();
                assert_eq!(empty, vec![3, 7, 9]);
            }

            /// An **absorbable** failure is not a job error: the page comes out empty, and that
            /// does not change under parallelism.
            #[test]
            fn an_absorbed_page_failure_leaves_the_same_empty_outcome() {
                let mut work = Pipeline::new("work");
                work.pdf_extract.push(FailingExtract::page_parse("soft"));
                work.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
                work.deserialize.push(PromiseDepositor::pipe("promise", "id"));
                let algorithm = Algorithm::new(
                    "FMT",
                    BTreeMap::from([
                        (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                        (PipelineName::new("work"), work),
                    ]),
                    &[PipelineName::new("classify")],
                    PageClassFinalizer::Identity,
                    Schedule::new(vec![step(&["a"])]),
                    BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
                )
                .expect("fixture is consistent");
                let document = wide_document(16);

                let concurrent = algorithm.apply_with(&document, &companies(), parallel()).unwrap();
                assert_eq!(algorithm.apply(&document, &companies()).unwrap(), concurrent);
                assert!(concurrent.pages.iter().all(|p| p.results.is_empty()));
            }
        }

        mod degradation {
            use super::*;

            /// Author-written pipes take the GIL back on every call: a bundle containing one stays
            /// sequential, and the result is the usual one.
            #[test]
            fn a_gil_bound_bundle_produces_the_same_outcome_on_one_thread() {
                let mut work = Pipeline::new("work");
                work.pdf_extract.push(GilBoundExtract::pipe("author"));
                work.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
                work.deserialize.push(PromiseDepositor::pipe("promise", "id"));
                assert!(!work.scales_with_threads());
                let algorithm = Algorithm::new(
                    "FMT",
                    BTreeMap::from([
                        (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                        (PipelineName::new("work"), work),
                    ]),
                    &[PipelineName::new("classify")],
                    PageClassFinalizer::Identity,
                    Schedule::new(vec![step(&["a"])]),
                    BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
                )
                .expect("fixture is consistent");
                let document = wide_document(16);
                assert_eq!(
                    algorithm.apply(&document, &companies()).unwrap(),
                    algorithm.apply_with(&document, &companies(), parallel()).unwrap()
                );
            }

            /// `pages = 1` is not "one thread": it is the sequential code, with no pool.
            #[test]
            fn one_page_at_a_time_is_the_sequential_path() {
                let witness = ThreadWitness::new("witness", 1);
                let mut work = Pipeline::new("work");
                work.pdf_extract.push(LinesToBlocks::pipe("extract"));
                work.text_filter.push(Arc::clone(&witness) as Arc<dyn TextFilterPipe>);
                work.deserialize.push(PromiseDepositor::pipe("promise", "id"));
                let algorithm = Algorithm::new(
                    "FMT",
                    BTreeMap::from([
                        (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                        (PipelineName::new("work"), work),
                    ]),
                    &[PipelineName::new("classify")],
                    PageClassFinalizer::Identity,
                    Schedule::new(vec![step(&["a"])]),
                    BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
                )
                .expect("fixture is consistent");

                algorithm
                    .apply_with(&wide_document(16), &companies(), Parallelism::SEQUENTIAL)
                    .unwrap();
                assert_eq!(
                    witness.distinct_threads(),
                    1,
                    "`pages = 1` must never hand a page to another thread"
                );
            }
        }

        mod threads_really_used {
            use super::*;

            /// The only test that looks at *how* the work is executed and not merely at what it
            /// produces.
            ///
            /// The pipe does not return until two calls have arrived, with a deadline so that the
            /// sequential case fails instead of wedging: were the pages of one step to run one
            /// after the other, the number of distinct threads would be one.
            #[test]
            fn two_pages_of_the_same_step_really_run_on_two_threads() {
                let witness = ThreadWitness::new("witness", 2);
                let mut work = Pipeline::new("work");
                work.pdf_extract.push(LinesToBlocks::pipe("extract"));
                work.text_filter.push(Arc::clone(&witness) as Arc<dyn TextFilterPipe>);
                work.deserialize.push(PromiseDepositor::pipe("promise", "id"));
                let algorithm = Algorithm::new(
                    "FMT",
                    BTreeMap::from([
                        (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                        (PipelineName::new("work"), work),
                    ]),
                    &[PipelineName::new("classify")],
                    PageClassFinalizer::Identity,
                    Schedule::new(vec![step(&["a"])]),
                    BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
                )
                .expect("fixture is consistent");

                algorithm.apply_with(&wide_document(2), &companies(), Parallelism::pages(2)).unwrap();
                assert!(
                    witness.distinct_threads() >= 2,
                    "the pages of one step must be handed to more than one thread"
                );
            }
        }
    }
}
