//! The three pipe traits, and [`Segment<P>`], the ordered deduplicated collection holding them.
//!
//! # A pipe is a trait, not a callable
//!
//! Native pipes and pipes written by a format author implement the *same* trait, so the engine does
//! not know whether a given pipe is Rust or Python — and a pipe that fails is identifiable, because
//! it has a `name()`. A segment made of opaque callables can report neither.
//!
//! # One generic [`Segment<P>`], not three copies
//!
//! Deduplication, union and iteration are written once. Adding a fourth segment costs a trait, an
//! alias and a field.
//!
//! # Order and deduplication
//!
//! Pipes run in **insertion order**, which is deterministic and makes tests reproducible.
//!
//! They are deduplicated **by identity** ([`Arc::ptr_eq`]), not by value: two pipes configured
//! identically but built separately are two distinct pipes and both run. Configuration equality
//! would be the wrong test, since a pipe's configuration is not always comparable and running the
//! same recipe twice is sometimes exactly what a format wants.
//!
//! `Send + Sync` is required on all three traits, which is what makes per-page and per-document
//! parallelism possible without a redesign: Python pipes stay serialised by the GIL, everything
//! else scales.

use std::sync::Arc;

use crate::core::classes::{BlockValue, PdfBlock, TextBlock};
use crate::core::page::Page;

use super::data::{Extracted, FilterData, PipeError};

/// First segment: from the page to raw PDF blocks.
pub trait PdfExtractPipe: Send + Sync {
    /// The pipe's name, for logging and error messages.
    fn name(&self) -> &str;
    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError>;

    /// `false` when spreading this pipe across threads cannot pay off.
    ///
    /// The only real case is a pipe written by a format author: every call takes the GIL back, so N
    /// threads re-serialise against each other and all that is left is the cost of distributing
    /// them. A pipe answering `false` is not *forbidden* on a thread — what is avoided is
    /// parallelism for the bundle containing it.
    ///
    /// The default is `true`: a pure Rust pipe has nothing to declare.
    fn scales_with_threads(&self) -> bool {
        true
    }
}

/// Second segment: from PDF blocks to the selected text blocks.
pub trait TextFilterPipe: Send + Sync {
    fn name(&self) -> &str;
    fn filter(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError>;

    /// `false` when spreading this pipe across threads cannot pay off.
    ///
    /// The only real case is a pipe written by a format author: every call takes the GIL back, so N
    /// threads re-serialise against each other and all that is left is the cost of distributing
    /// them. A pipe answering `false` is not *forbidden* on a thread — what is avoided is
    /// parallelism for the bundle containing it.
    ///
    /// The default is `true`: a pure Rust pipe has nothing to declare.
    fn scales_with_threads(&self) -> bool {
        true
    }
}

/// Third segment: from a text block to extracted entities.
///
/// Takes **one** block at a time rather than the list: a deserializer is invoked once per block.
pub trait DeserializePipe: Send + Sync {
    fn name(&self) -> &str;
    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError>;

    /// `false` when spreading this pipe across threads cannot pay off.
    ///
    /// The only real case is a pipe written by a format author: every call takes the GIL back, so N
    /// threads re-serialise against each other and all that is left is the cost of distributing
    /// them. A pipe answering `false` is not *forbidden* on a thread — what is avoided is
    /// parallelism for the bundle containing it.
    ///
    /// The default is `true`: a pure Rust pipe has nothing to declare.
    fn scales_with_threads(&self) -> bool {
        true
    }
}

/// An ordered, identity-deduplicated collection of pipes of the same segment.
pub struct Segment<P: ?Sized>(Vec<Arc<P>>);

impl<P: ?Sized> Segment<P> {
    pub fn new() -> Self {
        Segment(Vec::new())
    }

    /// Appends a pipe unless it is already present **by identity**. Returns whether it was actually
    /// added.
    pub fn push(&mut self, pipe: Arc<P>) -> bool {
        if self.0.iter().any(|existing| Arc::ptr_eq(existing, &pipe)) {
            return false;
        }
        self.0.push(pipe);
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<P>> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Union preserving order: `self`'s pipes first, then `other`'s that are not already present by
    /// identity.
    pub fn union(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for pipe in &other.0 {
            merged.push(Arc::clone(pipe));
        }
        merged
    }
}

// `#[derive(Default)]` and `#[derive(Clone)]` would add a `P: Default` / `P: Clone` bound, which a
// `dyn Trait` cannot satisfy: both have to be written by hand.
impl<P: ?Sized> Default for Segment<P> {
    fn default() -> Self {
        Segment::new()
    }
}

impl<P: ?Sized> Clone for Segment<P> {
    fn clone(&self) -> Self {
        Segment(self.0.iter().map(Arc::clone).collect())
    }
}

impl<P: ?Sized> std::fmt::Debug for Segment<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Segment({} pipes)", self.0.len())
    }
}

impl<P: ?Sized> std::ops::Add for Segment<P> {
    type Output = Segment<P>;

    /// This is how a format's structured, semistructured and unstructured layers combine.
    fn add(self, rhs: Self) -> Self::Output {
        self.union(&rhs)
    }
}

impl<P: ?Sized> FromIterator<Arc<P>> for Segment<P> {
    fn from_iter<I: IntoIterator<Item = Arc<P>>>(iter: I) -> Self {
        let mut segment = Segment::new();
        for pipe in iter {
            segment.push(pipe);
        }
        segment
    }
}

/// A short excerpt of a block's content, meant for one line of a log: the **text** a format author
/// can paste into a PDF viewer's search box to find the spot again.
///
/// It is why the three segments no longer merely count the blocks they produced. A `blocks=12` is
/// anchored to nothing, while the first line of extracted text says at once *where* it happened.
/// The count stays, as a secondary field.
///
/// A container (`List`, `Set`, `Map`) reduces to its first element, recursively: a pipe producing a
/// table should show the first cell, not `List([…])`.
fn searchable_excerpt(value: &BlockValue) -> String {
    /// Past this threshold the text is truncated with an ellipsis: a line of log has to stay one
    /// line.
    const MAX_CHARS: usize = 60;

    let raw = match value {
        BlockValue::Null => String::new(),
        BlockValue::Str(text) => text.clone(),
        BlockValue::List(items) => items.first().map(searchable_excerpt).unwrap_or_default(),
        BlockValue::Set(items) => items.iter().next().map(searchable_excerpt).unwrap_or_default(),
        BlockValue::Map(entries) => {
            entries.values().next().map(searchable_excerpt).unwrap_or_default()
        }
        other => format!("{other:?}"),
    };
    if raw.chars().count() <= MAX_CHARS {
        raw
    } else {
        format!("{}…", raw.chars().take(MAX_CHARS).collect::<String>())
    }
}

/// A segment's log line, emitted **whenever there is a place to anchor it to**: a non-empty
/// excerpt, whether or not anything came out.
///
/// The non-empty requirement is not fussiness. `PdfExtractPageClassifyStandard` always returns
/// exactly one block, even when the page does not belong to its page class, and that block has
/// empty content — counting blocks produced 11,259 identical, contentless rows on a single
/// document, half of the whole `.log.csv` at `-vv`. That case is still suppressed, by the excerpt
/// and not by the count.
///
/// A count of zero, on the other hand, has to be said. `pdf_extract` and `text_filter` sample
/// their own output, so an empty one has no excerpt and stays silent as before; `deserialize`
/// samples its **input**, so a page that had blocks and yielded no entity now says so. Staying
/// quiet there made a page that produced nothing indistinguishable from a page that failed, which
/// is precisely the pair a reader of the log needs to tell apart.
fn log_segment_output(message: &'static str, produced: usize, sample: Option<&BlockValue>) {
    let Some(excerpt) = sample.map(searchable_excerpt).filter(|text| !text.is_empty()) else {
        return;
    };
    tracing::debug!(found = %excerpt, produced, "{}", message);
}

pub type PdfExtractSegment = Segment<dyn PdfExtractPipe>;
pub type TextFilterSegment = Segment<dyn TextFilterPipe>;
pub type DeserializeSegment = Segment<dyn DeserializePipe>;

impl PdfExtractSegment {
    /// Concatenates the blocks produced by each pipe, in insertion order.
    pub fn apply(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        let segment_span = tracing::info_span!("pdf_extract");
        let _segment_guard = segment_span.enter();

        let mut out = Vec::new();
        for pipe in self.iter() {
            // The innermost `Activity` span: it wraps a single call to a pipe, not the whole
            // segment.
            let pipe_span = tracing::info_span!("pipe", pipe = pipe.name());
            let _pipe_guard = pipe_span.enter();
            let blocks = pipe.extract(page)?;
            // A pipe that does not apply to this page is the normal case — every page class is
            // tried against every page — and it produces nothing to anchor a line to, so it says
            // nothing: its empty row was on its own half of the `.log.csv` at `-vv`.
            log_segment_output("pdf blocks extracted", blocks.len(), blocks.first().map(|b| &b.content));
            out.extend(blocks);
        }
        Ok(out)
    }
}

impl TextFilterSegment {
    /// Concatenates the text blocks produced by each pipe, in insertion order.
    pub fn apply(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        let segment_span = tracing::info_span!("text_filter");
        let _segment_guard = segment_span.enter();

        let mut out = Vec::new();
        for pipe in self.iter() {
            let pipe_span = tracing::info_span!("pipe", pipe = pipe.name());
            let _pipe_guard = pipe_span.enter();
            let filtered = pipe.filter(blocks, data)?;
            log_segment_output("text blocks kept", filtered.len(), filtered.first().map(|b| &b.content));
            out.extend(filtered);
        }
        Ok(out)
    }
}

impl DeserializeSegment {
    /// Iterates **pipe × blocks**: for each pipe, all the blocks.
    ///
    /// A pipe with nothing to say returns an empty vector, so there is no sentinel to filter out
    /// afterwards. The one distinction such a sentinel would have had to carry in classification —
    /// "a classification happened, and it is *no class*" — is not lost: it is
    /// [`Extracted::PageClass(None)`](crate::core::pipeline::Extracted::PageClass), an explicit
    /// variant.
    pub fn apply(&self, blocks: &[TextBlock]) -> Result<Vec<Extracted>, PipeError> {
        let segment_span = tracing::info_span!("deserialize");
        let _segment_guard = segment_span.enter();

        let mut out = Vec::new();
        for pipe in self.iter() {
            // The count is logged once per pipe, not per block: a deserialize pipe runs over every
            // block of the page, and nothing above `trace` belongs in a hot loop. The `pipe` span
            // still wraps each individual call, as the `Activity` vocabulary requires.
            let mut produced = 0usize;
            for block in blocks {
                let pipe_span = tracing::info_span!("pipe", pipe = pipe.name());
                let _pipe_guard = pipe_span.enter();
                let results = pipe.deserialize(block)?;
                produced += results.len();
                out.extend(results);
            }
            log_segment_output("entities deserialized", produced, blocks.first().map(|b| &b.content));
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod test_pipes {
    //! Fake pipes shared by the tests of `segment`, `pipeline`, `bundle` and `algorithm`.
    //!
    //! The engine's tests check orchestration, not the pipes themselves, so these doubles make
    //! explicit *what* the engine guarantees regardless of what any real pipe does.

    use super::*;
    use crate::core::classes::{BlockType, TextBlock};
    use crate::core::page::PageError;
    use crate::core::schedule::PageClass;
    use std::sync::Mutex;

    /// Extracts one block per line of the page, with the line's text as content.
    pub(crate) struct LinesToBlocks {
        pub(crate) name: String,
        pub(crate) type_block: BlockType,
    }

    impl LinesToBlocks {
        pub(crate) fn pipe(name: &str) -> Arc<dyn PdfExtractPipe> {
            Arc::new(LinesToBlocks {
                name: name.to_string(),
                type_block: BlockType::RELEVANT_BLOCK,
            })
        }
    }

    impl PdfExtractPipe for LinesToBlocks {
        fn name(&self) -> &str {
            &self.name
        }

        fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
            Ok(page
                .lines
                .iter()
                .map(|line| PdfBlock::bare(self.type_block.clone(), line.text().clone()))
                .collect())
        }
    }

    /// Always fails, with the error it was given — used to tell an absorbable failure (page
    /// skipped) from a fatal one.
    pub(crate) struct FailingExtract {
        pub(crate) name: String,
        pub(crate) error: PipeError,
    }

    impl FailingExtract {
        pub(crate) fn page_parse(name: &str) -> Arc<dyn PdfExtractPipe> {
            Arc::new(FailingExtract {
                name: name.to_string(),
                error: PipeError::page_parse(name, PageError::ParseFail {
                    message: "unreadable page".to_string(),
                }),
            })
        }

        pub(crate) fn fatal(name: &str) -> Arc<dyn PdfExtractPipe> {
            Arc::new(FailingExtract {
                name: name.to_string(),
                error: PipeError::extraction(name, "boom"),
            })
        }
    }

    impl PdfExtractPipe for FailingExtract {
        fn name(&self) -> &str {
            &self.name
        }

        fn extract(&self, _page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
            Err(self.error.clone())
        }
    }

    /// Fails **only** on the pages listed — used to pin down which failure is reported when several
    /// pages fail within the same step.
    pub(crate) struct FailingOnPages {
        pub(crate) name: String,
        pub(crate) pages: Vec<u32>,
    }

    impl FailingOnPages {
        pub(crate) fn fatal(name: &str, pages: &[u32]) -> Arc<dyn PdfExtractPipe> {
            Arc::new(FailingOnPages { name: name.to_string(), pages: pages.to_vec() })
        }
    }

    impl PdfExtractPipe for FailingOnPages {
        fn name(&self) -> &str {
            &self.name
        }

        fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
            if self.pages.contains(&page.number) {
                return Err(PipeError::extraction(&self.name, format!("page {} is doomed", page.number)));
            }
            Ok(page
                .lines
                .iter()
                .map(|line| PdfBlock::bare(BlockType::RELEVANT_BLOCK, line.text().clone()))
                .collect())
        }
    }

    /// A pipe declaring that it does **not** scale with threads, like the adapters for
    /// author-written Python pipes, which take the GIL back on every call. It exercises the
    /// degradation to sequential without involving Python at all.
    pub(crate) struct GilBoundExtract {
        pub(crate) name: String,
    }

    impl GilBoundExtract {
        pub(crate) fn pipe(name: &str) -> Arc<dyn PdfExtractPipe> {
            Arc::new(GilBoundExtract { name: name.to_string() })
        }
    }

    impl PdfExtractPipe for GilBoundExtract {
        fn name(&self) -> &str {
            &self.name
        }

        fn scales_with_threads(&self) -> bool {
            false
        }

        fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
            Ok(page
                .lines
                .iter()
                .map(|line| PdfBlock::bare(BlockType::RELEVANT_BLOCK, line.text().clone()))
                .collect())
        }
    }

    /// Records which thread it ran on and, if `wanted` is greater than one, does not return until
    /// `wanted` calls have arrived or the wait times out.
    ///
    /// The timeout is deliberate: a real `Barrier` would block forever if the engine ran the pages
    /// sequentially, and a test that hangs says nothing about what went wrong. With a deadline, the
    /// sequential case **fails** instead of wedging.
    pub(crate) struct ThreadWitness {
        pub(crate) name: String,
        pub(crate) wanted: usize,
        pub(crate) arrived: std::sync::atomic::AtomicUsize,
        pub(crate) threads: Mutex<Vec<std::thread::ThreadId>>,
    }

    impl ThreadWitness {
        pub(crate) fn new(name: &str, wanted: usize) -> Arc<ThreadWitness> {
            Arc::new(ThreadWitness {
                name: name.to_string(),
                wanted,
                arrived: std::sync::atomic::AtomicUsize::new(0),
                threads: Mutex::new(Vec::new()),
            })
        }

        /// How many distinct threads ran this pipe.
        pub(crate) fn distinct_threads(&self) -> usize {
            // `ThreadId` is `Hash` but not `Ord`: deduplicate with a set, not by sorting.
            let threads = self.threads.lock().expect("test-only mutex is never poisoned");
            threads.iter().copied().collect::<std::collections::HashSet<_>>().len()
        }
    }

    impl TextFilterPipe for ThreadWitness {
        fn name(&self) -> &str {
            &self.name
        }

        fn filter(
            &self,
            blocks: &[PdfBlock],
            _data: &FilterData<'_>,
        ) -> Result<Vec<TextBlock>, PipeError> {
            self.threads
                .lock()
                .expect("test-only mutex is never poisoned")
                .push(std::thread::current().id());
            self.arrived.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while self.arrived.load(std::sync::atomic::Ordering::SeqCst) < self.wanted
                && std::time::Instant::now() < deadline
            {
                std::thread::yield_now();
            }
            Ok(blocks
                .iter()
                .map(|b| TextBlock::new(BlockType::PAGE_CLASS, b.metadata.clone(), b.clone()))
                .collect())
        }
    }

    /// Turns every PDF block into a text block, and records which `FilterData` it saw.
    pub(crate) struct RecordingFilter {
        pub(crate) name: String,
        pub(crate) seen: Mutex<Vec<(usize, usize)>>,
    }

    impl RecordingFilter {
        pub(crate) fn new(name: &str) -> Arc<RecordingFilter> {
            Arc::new(RecordingFilter { name: name.to_string(), seen: Mutex::new(Vec::new()) })
        }

        /// `(number of target companies, number of previous results)` seen at each call.
        pub(crate) fn seen(&self) -> Vec<(usize, usize)> {
            self.seen.lock().expect("test-only mutex is never poisoned").clone()
        }
    }

    impl TextFilterPipe for RecordingFilter {
        fn name(&self) -> &str {
            &self.name
        }

        fn filter(
            &self,
            blocks: &[PdfBlock],
            data: &FilterData<'_>,
        ) -> Result<Vec<TextBlock>, PipeError> {
            self.seen
                .lock()
                .expect("test-only mutex is never poisoned")
                .push((data.target_companies().len(), data.previous().len()));
            Ok(blocks
                .iter()
                .map(|b| TextBlock::new(BlockType::PAGE_CLASS, b.metadata.clone(), b.clone()))
                .collect())
        }
    }

    /// Classifies every block with the fixed class it was given.
    pub(crate) struct ConstantClassifier {
        pub(crate) name: String,
        pub(crate) class: Option<PageClass>,
    }

    impl ConstantClassifier {
        pub(crate) fn pipe(name: &str, class: Option<&str>) -> Arc<dyn DeserializePipe> {
            Arc::new(ConstantClassifier {
                name: name.to_string(),
                class: class.map(PageClass::new),
            })
        }
    }

    impl DeserializePipe for ConstantClassifier {
        fn name(&self) -> &str {
            &self.name
        }

        fn deserialize(&self, _block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
            Ok(vec![Extracted::PageClass(self.class.clone())])
        }
    }

    /// Deposits one promise per block received.
    pub(crate) struct PromiseDepositor {
        pub(crate) name: String,
        pub(crate) id: String,
    }

    impl PromiseDepositor {
        pub(crate) fn pipe(name: &str, id: &str) -> Arc<dyn DeserializePipe> {
            Arc::new(PromiseDepositor { name: name.to_string(), id: id.to_string() })
        }
    }

    impl DeserializePipe for PromiseDepositor {
        fn name(&self) -> &str {
            &self.name
        }

        fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
            let entries = [(self.id.clone(), block.content.clone())].into_iter().collect();
            Ok(vec![Extracted::Promises(entries)])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_pipes::*;
    use super::*;
    use crate::core::classes::BlockType;
    use crate::core::page::Page;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;

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

    mod dedup_and_order {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn pipes_run_in_insertion_order() {
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("first"));
            segment.push(LinesToBlocks::pipe("second"));

            let names: Vec<&str> = segment.iter().map(|p| p.name()).collect();
            assert_eq!(names, vec!["first", "second"]);
        }

        #[test]
        fn the_same_pipe_added_twice_is_kept_once() {
            let pipe = LinesToBlocks::pipe("only");
            let mut segment = PdfExtractSegment::new();
            assert!(segment.push(Arc::clone(&pipe)));
            assert!(!segment.push(pipe));
            assert_eq!(segment.len(), 1);
        }

        #[test]
        fn two_identically_configured_pipes_are_distinct() {
            // Deduplication is by identity, not by value.
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("same"));
            segment.push(LinesToBlocks::pipe("same"));
            assert_eq!(segment.len(), 2);
        }

        #[test]
        fn a_duplicate_does_not_move_the_original_to_the_back() {
            let first = LinesToBlocks::pipe("first");
            let mut segment = PdfExtractSegment::new();
            segment.push(Arc::clone(&first));
            segment.push(LinesToBlocks::pipe("second"));
            segment.push(first);

            let names: Vec<&str> = segment.iter().map(|p| p.name()).collect();
            assert_eq!(names, vec!["first", "second"]);
        }

        #[test]
        fn a_fresh_segment_is_empty() {
            let segment = PdfExtractSegment::new();
            assert!(segment.is_empty());
            assert_eq!(segment.len(), 0);
            assert_eq!(PdfExtractSegment::default().len(), 0);
        }

        #[test]
        fn collecting_from_an_iterator_deduplicates_too() {
            let pipe = LinesToBlocks::pipe("only");
            let segment: PdfExtractSegment =
                [Arc::clone(&pipe), pipe].into_iter().collect();
            assert_eq!(segment.len(), 1);
        }
    }

    mod union {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn adding_two_segments_concatenates_them_in_order() {
            let mut left = PdfExtractSegment::new();
            left.push(LinesToBlocks::pipe("a"));
            let mut right = PdfExtractSegment::new();
            right.push(LinesToBlocks::pipe("b"));

            let merged = left + right;
            let names: Vec<&str> = merged.iter().map(|p| p.name()).collect();
            assert_eq!(names, vec!["a", "b"]);
        }

        #[test]
        fn a_pipe_present_in_both_segments_appears_once() {
            let shared = LinesToBlocks::pipe("shared");
            let mut left = PdfExtractSegment::new();
            left.push(Arc::clone(&shared));
            let mut right = PdfExtractSegment::new();
            right.push(Arc::clone(&shared));
            right.push(LinesToBlocks::pipe("extra"));

            let merged = left + right;
            let names: Vec<&str> = merged.iter().map(|p| p.name()).collect();
            assert_eq!(names, vec!["shared", "extra"]);
        }

        #[test]
        fn union_leaves_both_operands_untouched() {
            let mut left = PdfExtractSegment::new();
            left.push(LinesToBlocks::pipe("a"));
            let mut right = PdfExtractSegment::new();
            right.push(LinesToBlocks::pipe("b"));

            let merged = left.union(&right);
            assert_eq!(merged.len(), 2);
            assert_eq!(left.len(), 1);
            assert_eq!(right.len(), 1);
        }

        #[test]
        fn adding_an_empty_segment_changes_nothing() {
            let mut left = PdfExtractSegment::new();
            left.push(LinesToBlocks::pipe("a"));
            let merged = left + PdfExtractSegment::new();
            assert_eq!(merged.len(), 1);
        }

        #[test]
        fn cloning_a_segment_shares_the_same_pipes() {
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("a"));
            let copy = segment.clone();
            // The same `Arc`s: the union of the two deduplicates down to one.
            assert_eq!((segment + copy).len(), 1);
        }
    }

    mod pdf_extract_application {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_blocks_of_every_pipe_are_concatenated_in_order() {
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("first"));
            segment.push(LinesToBlocks::pipe("second"));

            let blocks = segment.apply(&page_with(&["alpha"])).unwrap();
            let contents: Vec<&str> =
                blocks.iter().map(|b| b.content.as_str().unwrap()).collect();
            assert_eq!(contents, vec!["alpha", "alpha"]);
        }

        #[test]
        fn an_empty_segment_extracts_nothing() {
            let blocks = PdfExtractSegment::new().apply(&page_with(&["alpha"])).unwrap();
            assert!(blocks.is_empty());
        }

        #[test]
        fn a_page_with_no_lines_yields_no_blocks() {
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("only"));
            assert!(segment.apply(&page_with(&[])).unwrap().is_empty());
        }

        #[test]
        fn the_first_failing_pipe_stops_the_segment() {
            let mut segment = PdfExtractSegment::new();
            segment.push(FailingExtract::fatal("boom"));
            segment.push(LinesToBlocks::pipe("never reached"));

            let err = segment.apply(&page_with(&["alpha"])).unwrap_err();
            assert_eq!(err.pipe(), "boom");
        }

        #[test]
        fn a_page_failure_travels_out_of_the_segment_unchanged() {
            let mut segment = PdfExtractSegment::new();
            segment.push(FailingExtract::page_parse("skipper"));
            let err = segment.apply(&page_with(&["alpha"])).unwrap_err();
            assert!(err.is_page_failure());
        }
    }

    mod text_filter_application {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_pipe_sees_the_same_blocks_and_the_same_filter_data() {
            let first = RecordingFilter::new("first");
            let second = RecordingFilter::new("second");
            let mut segment = TextFilterSegment::new();
            segment.push(Arc::clone(&first) as Arc<dyn TextFilterPipe>);
            segment.push(Arc::clone(&second) as Arc<dyn TextFilterPipe>);

            let blocks = vec![PdfBlock::bare(BlockType::RELEVANT_BLOCK, "x")];
            let previous = vec![Extracted::PageClass(None)];
            let out = segment.apply(&blocks, &FilterData::Previous(&previous)).unwrap();

            assert_eq!(out.len(), 2);
            assert_eq!(first.seen(), vec![(0, 1)]);
            assert_eq!(second.seen(), vec![(0, 1)]);
        }

        #[test]
        fn an_empty_segment_filters_nothing() {
            let out = TextFilterSegment::new().apply(&[], &FilterData::EMPTY).unwrap();
            assert!(out.is_empty());
        }
    }

    /// A segment says what it did with a page, and "nothing came out of it" is a thing it did.
    /// Before, a page that produced no entity and a page whose pipe blew up looked exactly alike in
    /// the log, which made a failure impossible to locate without re-running the job.
    mod segment_logging {
        use super::*;
        use std::sync::{Arc as StdArc, Mutex};
        use tracing::field::{Field, Visit};
        use tracing_subscriber::Registry;
        use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

        /// One captured event: its message and, when it carries them, the `found` excerpt and the
        /// `produced` count.
        #[derive(Default, Clone, Debug)]
        struct Record {
            message: String,
            found: Option<String>,
            produced: Option<u64>,
        }

        impl Visit for Record {
            fn record_u64(&mut self, field: &Field, value: u64) {
                if field.name() == "produced" {
                    self.produced = Some(value);
                }
            }

            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                match field.name() {
                    "message" => self.message = format!("{value:?}"),
                    "found" => self.found = Some(format!("{value:?}")),
                    _ => {}
                }
            }
        }

        #[derive(Clone, Default)]
        struct CapturingLayer {
            records: StdArc<Mutex<Vec<Record>>>,
        }

        impl<S: tracing::Subscriber> Layer<S> for CapturingLayer {
            fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
                let mut record = Record::default();
                event.record(&mut record);
                self.records.lock().unwrap().push(record);
            }
        }

        fn records_of(f: impl FnOnce()) -> Vec<Record> {
            let layer = CapturingLayer::default();
            let subscriber = Registry::default().with(layer.clone());
            tracing::subscriber::with_default(subscriber, f);
            let records = layer.records.lock().unwrap();
            records.clone()
        }

        /// A deserializer with nothing to say about any block: the ordinary case of a pipe whose
        /// block type is not the one on this page.
        struct SaysNothing;

        impl DeserializePipe for SaysNothing {
            fn name(&self) -> &str {
                "says-nothing"
            }

            fn deserialize(&self, _block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
                Ok(Vec::new())
            }
        }

        fn text_block(content: &str) -> TextBlock {
            TextBlock::from_content(BlockType::PAGE_CLASS, std::collections::BTreeMap::new(), content)
        }

        #[test]
        fn a_page_that_yielded_no_entity_says_so() {
            let mut segment = DeserializeSegment::new();
            segment.push(Arc::new(SaysNothing) as Arc<dyn DeserializePipe>);

            let records = records_of(|| {
                let _ = segment.apply(&[text_block("ACME CORP 1.000 EUR")]);
            });
            let line = records
                .iter()
                .find(|r| r.message.contains("entities deserialized"))
                .unwrap_or_else(|| panic!("no line about the segment's output: {records:?}"));
            assert_eq!(line.produced, Some(0));
            assert!(line.found.as_deref().unwrap_or_default().contains("ACME CORP"), "{line:?}");
        }

        #[test]
        fn a_page_that_yielded_entities_still_says_how_many() {
            let mut segment = DeserializeSegment::new();
            segment.push(ConstantClassifier::pipe("a", Some("x")));

            let records = records_of(|| {
                let _ = segment.apply(&[text_block("ACME CORP")]);
            });
            let line = records.iter().find(|r| r.message.contains("entities deserialized")).unwrap();
            assert_eq!(line.produced, Some(1));
        }

        #[test]
        fn a_block_with_nothing_to_quote_is_still_not_worth_a_line() {
            // The suppression that matters is the one on the excerpt, not the one on the count:
            // the page-classify pipe returns one contentless block per page, and counting those
            // once filled half the `.log.csv` of a single document.
            let mut segment = DeserializeSegment::new();
            segment.push(Arc::new(SaysNothing) as Arc<dyn DeserializePipe>);

            let records = records_of(|| {
                let _ = segment.apply(&[text_block("")]);
            });
            assert!(
                !records.iter().any(|r| r.message.contains("entities deserialized")),
                "{records:?}"
            );
        }

        #[test]
        fn a_segment_that_extracted_nothing_has_nothing_to_anchor_a_line_to() {
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("lines"));

            let records = records_of(|| {
                let _ = segment.apply(&page_with(&[]));
            });
            assert!(!records.iter().any(|r| r.message.contains("pdf blocks extracted")), "{records:?}");
        }
    }

    mod deserialize_application {
        use super::*;
        use pretty_assertions::assert_eq;

        fn text_blocks(n: usize) -> Vec<TextBlock> {
            (0..n)
                .map(|i| {
                    TextBlock::from_content(
                        BlockType::PAGE_CLASS,
                        std::collections::BTreeMap::new(),
                        format!("blk{i}"),
                    )
                })
                .collect()
        }

        #[test]
        fn each_pipe_is_applied_to_every_block() {
            let mut segment = DeserializeSegment::new();
            segment.push(ConstantClassifier::pipe("a", Some("x")));
            segment.push(ConstantClassifier::pipe("b", Some("y")));

            let out = segment.apply(&text_blocks(2)).unwrap();
            assert_eq!(out.len(), 4);
        }

        #[test]
        fn pipes_are_the_outer_loop_and_blocks_the_inner_one() {
            // All the blocks of the first pipe, then those of the second.
            let mut segment = DeserializeSegment::new();
            segment.push(ConstantClassifier::pipe("a", Some("x")));
            segment.push(ConstantClassifier::pipe("b", Some("y")));

            let out = segment.apply(&text_blocks(2)).unwrap();
            let classes: Vec<String> = out
                .iter()
                .map(|e| e.as_page_class().unwrap().as_ref().unwrap().to_string())
                .collect();
            assert_eq!(classes, vec!["x", "x", "y", "y"]);
        }

        #[test]
        fn an_unclassified_result_is_an_explicit_variant_not_an_absence() {
            let mut segment = DeserializeSegment::new();
            segment.push(ConstantClassifier::pipe("a", None));

            let out = segment.apply(&text_blocks(1)).unwrap();
            assert_eq!(out, vec![Extracted::PageClass(None)]);
        }

        #[test]
        fn no_blocks_means_no_results_even_with_pipes() {
            let mut segment = DeserializeSegment::new();
            segment.push(ConstantClassifier::pipe("a", Some("x")));
            assert!(segment.apply(&[]).unwrap().is_empty());
        }

        #[test]
        fn an_empty_segment_deserializes_nothing() {
            assert!(DeserializeSegment::new().apply(&text_blocks(2)).unwrap().is_empty());
        }
    }
}
