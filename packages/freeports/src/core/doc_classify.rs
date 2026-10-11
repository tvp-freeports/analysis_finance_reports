//! Running the classification of a **document**: which format, possibly partial, its pages say it
//! is in.
//!
//! This is the execution side only. The loaders (`formats_repo`, phases 7–8 of
//! `agent-memory/PLAN-document-classification.md`) build a [`DocumentClassifier`] from
//! `metadata/document_classify.yaml` and `metadata/document_classify.py`; this module walks it over
//! a document's pages.
//!
//! The normative text is §3 (the state machine), §3.4 (the outcome), §6 (the Python contract and
//! the precedence between the two sources) and §2.1 (the verbose form the steps arrive in) of
//! `agent-memory/SPEC-document-classification.md`.
//!
//! # One way in
//!
//! Pages are read through a [`PageSource`], which [`LazyDocument`] implements: one page converted
//! at a time, and only when an entry asks for it. The trait exists for testability alone — it is
//! what lets the tests count the conversions (early exit, conversion once per page) without
//! building a PDF for each case. There is no second implementation in production code.

// The errors name the entry, its pipeline, its page and the patterns involved, by value, so that a
// caller can match on them; a classification fails at most once per document, so their size on the
// error path costs nothing worth boxing them for.
#![allow(clippy::result_large_err)]

use std::collections::HashMap;
use std::num::NonZeroI32;

use crate::core::page::Page;
use crate::core::pipeline::{Extracted, FilterData, PipeError, Pipeline, PipelineName};
use crate::formats_repo::format_pattern::{FormatPattern, FormatTemplate};
use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;
use crate::formats_utils::pdf_extract::standard_funcs::first_line_if_every_selection_matches;
use crate::input::document::DocumentError;
use crate::input::document::lazy::{LazyDocument, resolve_page_number};

/// Where the pages of the document being classified come from.
///
/// `number` is a page number as an author writes it: 1-based, negative counted from the end. A
/// number past either end of the document is `Ok(None)`, not an error.
///
/// `page_count` lets the engine resolve a written number to the physical page before converting
/// it: `1` and `-10` of a 10-page document are one page, converted once.
pub trait PageSource {
    fn page_count(&self) -> usize;
    fn page(&self, number: NonZeroI32) -> Result<Option<Page>, DocumentError>;
}

impl PageSource for LazyDocument {
    fn page_count(&self) -> usize {
        LazyDocument::page_count(self)
    }

    fn page(&self, number: NonZeroI32) -> Result<Option<Page>, DocumentError> {
        LazyDocument::page(self, number)
    }
}

/// What a YAML step names: a pattern, or (top level only) a `*` template instantiated with the
/// entity of the accumulated partial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKey {
    Pattern(FormatPattern),
    Template(FormatTemplate),
}

impl std::fmt::Display for StepKey {
    /// The key as written: the pattern's canonical form, or the template's (`*-EN23`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepKey::Pattern(pattern) => pattern.fmt(f),
            StepKey::Template(template) => template.fmt(f),
        }
    }
}

/// One YAML step, already normalized to the verbose form (§2.1).
///
/// A child's key is its **full** pattern: the loader composes the suffix written in the YAML with
/// the parent's pattern, so the engine never sees a suffix.
#[derive(Clone)]
pub struct Step {
    pub key: StepKey,
    /// Every one of them must find at least one line on the page (AND). Empty: the step has no
    /// criteria of its own — a prefix if it has children, unconditional if it has none.
    pub criteria: Vec<PdfLineSelection>,
    /// In order: the order is semantics (depth-first preorder, §3).
    pub children: Vec<Step>,
}

impl Step {
    /// No criteria of its own but children: it only scopes them, and is never evaluated (§3.2).
    pub(crate) fn is_prefix(&self) -> bool {
        self.criteria.is_empty() && !self.children.is_empty()
    }
}

/// One entry of the YAML: a page, and the steps tried on it.
#[derive(Clone)]
pub struct YamlEntry {
    pub page: NonZeroI32,
    pub steps: Vec<Step>,
}

/// One entry of `document_classifier`: a page, and the pipeline run on it.
#[derive(Debug, Clone)]
pub struct PythonEntry {
    pub page: NonZeroI32,
    pub pipeline: Pipeline,
}

impl PythonEntry {
    /// What the pipeline says of `page` (§6): its results with the `None`s dropped and the repeats
    /// merged, which must leave at most one pattern — and one the repository has, since the Python
    /// cannot be checked when it is loaded. `index` is the entry's position, for the errors.
    fn outcome(
        &self,
        index: usize,
        page: &Page,
        formats: &[String],
    ) -> Result<Option<FormatPattern>, DocClassifyError> {
        let pipeline = || self.pipeline.name.clone();
        let results = self
            .pipeline
            .apply(page, &FilterData::EMPTY)
            .map_err(|source| DocClassifyError::Pipe {
                entry: index,
                pipeline: pipeline(),
                page: self.page,
                source,
            })?;
        let mut outcome: Option<FormatPattern> = None;
        for result in results {
            let Extracted::DocClass(pattern) = result else {
                return Err(DocClassifyError::NotADocClassification {
                    entry: index,
                    pipeline: pipeline(),
                    page: self.page,
                    found: format!("{result:?}"),
                });
            };
            let Some(pattern) = pattern else { continue };
            match &outcome {
                None => outcome = Some(pattern),
                Some(first) if *first != pattern => {
                    return Err(DocClassifyError::ConflictingDocClass {
                        entry: index,
                        pipeline: pipeline(),
                        page: self.page,
                        first: first.clone(),
                        second: pattern,
                    });
                }
                Some(_) => {}
            }
        }
        match outcome {
            Some(pattern) if !in_repository(&pattern, formats) => Err(if pattern.is_complete() {
                DocClassifyError::UnknownFormat {
                    entry: index,
                    pipeline: pipeline(),
                    page: self.page,
                    format: pattern,
                }
            } else {
                DocClassifyError::PartialMatchesNoFormat {
                    entry: index,
                    pipeline: pipeline(),
                    page: self.page,
                    pattern,
                }
            }),
            outcome => Ok(outcome),
        }
    }
}

/// A loaded document classification: the formats of the repository, the Python entries and the
/// YAML entries. The Python entries run first, then the YAML ones (§6).
#[derive(Clone)]
pub struct DocumentClassifier {
    formats: Vec<String>,
    python: Vec<PythonEntry>,
    yaml: Vec<YamlEntry>,
}

impl DocumentClassifier {
    /// `formats` are the complete names declared in `formats.csv`, in file order: the candidates of
    /// a partial outcome follow that order.
    pub fn new(formats: Vec<String>, python: Vec<PythonEntry>, yaml: Vec<YamlEntry>) -> Self {
        DocumentClassifier {
            formats,
            python,
            yaml,
        }
    }

    /// The complete names of `formats.csv`, in file order: what the sources of a document are
    /// combined against (phase 10, `formats_repo::doc_classify_sources`).
    pub fn formats(&self) -> &[String] {
        &self.formats
    }

    /// Runs the classification over the pages of `source`, converting each page at most once and
    /// none after a complete outcome.
    ///
    /// The accumulated partial `A` is carried through every entry, Python first and then YAML: a
    /// partial the Python found is where the YAML starts from (§3.3, §6).
    pub fn classify(&self, source: &dyn PageSource) -> Result<DocClassification, DocClassifyError> {
        let mut pages = PageCache {
            source,
            pages: HashMap::new(),
        };
        let mut accumulated: Option<FormatPattern> = None;

        for (index, entry) in self.python.iter().enumerate() {
            let Some(page) = pages.get(entry.page)? else {
                continue;
            };
            let Some(pattern) = entry.outcome(index, page, &self.formats)? else {
                continue;
            };
            match &accumulated {
                // The counterpart of a YAML step that is not candidable (§6, D4).
                Some(found) if !adds_to(&pattern, Some(found)) => {
                    ignore_python_outcome(index, entry, &pattern, found)
                }
                _ if pattern.is_complete() => {
                    let complete = CompleteClassification {
                        format: pattern,
                        page: entry.page,
                        entry: index,
                        decided_by: DecidedBy::Python {
                            pipeline: entry.pipeline.name.clone(),
                        },
                        by_exclusion: false,
                    };
                    announce(&complete, None);
                    return Ok(DocClassification::Complete(complete));
                }
                _ => {
                    tracing::debug!(page = entry.page, pipeline = %entry.pipeline.name, "partial format '{pattern}' found");
                    accumulated = Some(pattern);
                }
            }
        }

        for (index, entry) in self.yaml.iter().enumerate() {
            let Some(page) = pages.get(entry.page)? else {
                continue;
            };
            let mut walk = YamlWalk {
                formats: &self.formats,
                page,
                written_page: entry.page,
                entry: index,
                accumulated: &mut accumulated,
                path: Vec::new(),
            };
            if let Some((complete, found)) = walk.steps(&entry.steps, None) {
                announce(&complete, found.as_deref());
                return Ok(DocClassification::Complete(complete));
            }
        }

        Ok(match accumulated {
            Some(pattern) => DocClassification::Partial {
                candidates: pattern.expand(&self.formats),
                pattern,
            },
            None => DocClassification::Unclassified,
        })
    }
}

/// The pages converted so far, by **physical** index: one entry may write `1` and another `-10`
/// for the same page of a 10-page document, and that page is converted once.
struct PageCache<'s> {
    source: &'s dyn PageSource,
    pages: HashMap<usize, Page>,
}

impl PageCache<'_> {
    /// The page `number` designates, converted on its first request; `None` past either end.
    ///
    /// A page that cannot be converted stops the classification rather than counting as a page
    /// that says nothing: the outcome would otherwise depend silently on a page nobody read.
    fn get(&mut self, number: NonZeroI32) -> Result<Option<&Page>, DocClassifyError> {
        let index = resolve_page_number(number, self.source.page_count());
        if let Some(index) = index
            && self.pages.contains_key(&index)
        {
            return Ok(self.pages.get(&index));
        }
        let page = self
            .source
            .page(number)
            .map_err(|source| DocClassifyError::Document {
                page: number,
                source,
            })?;
        Ok(match (index, page) {
            (Some(index), Some(page)) => Some(self.pages.entry(index).or_insert(page)),
            _ => None,
        })
    }
}

/// Rule 3.1.1, shared by the YAML steps and the Python outcomes: with nothing accumulated anything
/// counts; otherwise only what strictly refines the accumulated partial — what adds nothing or
/// contradicts it does not, so `A` can only ever be refined, never replaced (§3.3).
fn adds_to(pattern: &FormatPattern, accumulated: Option<&FormatPattern>) -> bool {
    accumulated.is_none_or(|found| pattern.strictly_refines(found))
}

/// Rules 3.1.1 and 3.1.2 for a step of pattern `pattern`, with `accumulated` as `A`: the **one**
/// definition of candidability, shared by the run ([`YamlWalk`]) and by the load-time reachability
/// check of `formats_repo::doc_classify_validate` (§5), which must not keep a copy of it.
///
/// `root` is `None` for a top-level step; for a nested one it is the pattern `A` must already
/// refine (D2: the nearest ancestor with criteria of its own, or the top-level key of a chain of
/// prefixes).
pub(crate) fn is_candidable(
    pattern: &FormatPattern,
    root: Option<&FormatPattern>,
    accumulated: Option<&FormatPattern>,
) -> bool {
    let root_found = root.is_none_or(|root| accumulated.is_some_and(|found| found.refines(root)));
    root_found && adds_to(pattern, accumulated)
}

/// Rule 3.1.3 for a `*` step: the template instantiated with the entity of `accumulated`, if there
/// is one and the repository has a format for it (the instance is one, or expands to some) — so the
/// YAML can never produce a name that does not exist. Shared with the load-time validation like
/// [`is_candidable`].
pub(crate) fn template_instance(
    template: &FormatTemplate,
    accumulated: Option<&FormatPattern>,
    formats: &[String],
) -> Option<FormatPattern> {
    let instance = template.instantiate(accumulated?.entity());
    in_repository(&instance, formats).then_some(instance)
}

/// Some format of the repository refines `pattern`: for a complete name, it is declared.
fn in_repository(pattern: &FormatPattern, formats: &[String]) -> bool {
    !pattern.expand(formats).is_empty()
}

/// D4: an outcome the accumulated partial already refines repeats what is known and is only
/// `debug`; any other — incompatible, or compatible but not comparable — means two pipelines of
/// the author disagree, which must show without stopping the classification.
fn ignore_python_outcome(
    index: usize,
    entry: &PythonEntry,
    pattern: &FormatPattern,
    found: &FormatPattern,
) {
    if found.refines(pattern) {
        tracing::debug!(
            entry = index,
            page = entry.page,
            pipeline = %entry.pipeline.name,
            "outcome '{pattern}' adds nothing to the format already found '{found}' - ignored"
        );
    } else {
        tracing::warn!(
            entry = index,
            page = entry.page,
            pipeline = %entry.pipeline.name,
            "outcome '{pattern}' does not refine the format already found '{found}' - ignored"
        );
    }
}

/// The one event of a classification that succeeded: the format, the page and what decided it,
/// with the line its criteria found when there is one — what to search for in the PDF.
fn announce(complete: &CompleteClassification, found: Option<&str>) {
    let format = &complete.format;
    match (&complete.decided_by, found) {
        (DecidedBy::Python { pipeline }, _) => {
            tracing::info!(page = complete.page, pipeline = %pipeline, "document classified as '{format}'");
        }
        (DecidedBy::Yaml { step, .. }, Some(found)) => {
            tracing::info!(page = complete.page, step = %step, found = ?found, "document classified as '{format}'");
        }
        // D6: not a warning — the format is not a value of the output — but it must be visible
        // that the document never said it.
        (DecidedBy::Yaml { step, .. }, None) => {
            tracing::info!(
                page = complete.page,
                step = %step,
                "document classified as '{format}' by exclusion: step '{step}' has no criteria of its own"
            );
        }
    }
}

/// One YAML entry walked over its page in depth-first preorder (§3), refining the accumulated
/// partial it shares with the other entries.
struct YamlWalk<'a> {
    formats: &'a [String],
    page: &'a Page,
    written_page: NonZeroI32,
    entry: usize,
    accumulated: &'a mut Option<FormatPattern>,
    /// The position of the step being visited, as [`DecidedBy::Yaml`] reports it.
    path: Vec<usize>,
}

impl YamlWalk<'_> {
    /// Visits `steps` in order, stopping at the first complete outcome, which comes with the line
    /// its criteria found (`None` for an unconditional step).
    ///
    /// `context` is `None` at the top level; below it, the pattern `A` must already refine for a
    /// nested step to be candidable (rule 3.1.2).
    fn steps(
        &mut self,
        steps: &[Step],
        context: Option<&FormatPattern>,
    ) -> Option<(CompleteClassification, Option<String>)> {
        for (position, step) in steps.iter().enumerate() {
            self.path.push(position);
            let decision = self.step(step, context);
            self.path.pop();
            if decision.is_some() {
                return decision;
            }
        }
        None
    }

    /// One step, then its children. Candidability is the step's own (R1): a step that is not
    /// evaluated does not close its subtree; only criteria that were evaluated and failed do
    /// (rule 3.1.4).
    fn step(
        &mut self,
        step: &Step,
        context: Option<&FormatPattern>,
    ) -> Option<(CompleteClassification, Option<String>)> {
        let pattern = match &step.key {
            StepKey::Pattern(pattern) => pattern.clone(),
            StepKey::Template(template) => {
                template_instance(template, self.accumulated.as_ref(), self.formats)?
            }
        };
        if !step.is_prefix() && is_candidable(&pattern, context, self.accumulated.as_ref()) {
            let found = first_line_if_every_selection_matches(&step.criteria, &self.page.lines)?;
            let found = (!step.criteria.is_empty()).then_some(found);
            if pattern.is_complete() {
                return Some((self.complete(step, pattern, found.is_none()), found));
            }
            tracing::debug!(page = self.written_page, step = %step.key, found = ?found, "partial format '{pattern}' found");
            *self.accumulated = Some(pattern.clone());
        }
        // D2: the root of the children is the nearest ancestor with criteria of its own, or the
        // top-level key of a chain of prefixes — so adding or removing a prefix changes nothing.
        let root = if step.criteria.is_empty() {
            context.unwrap_or(&pattern)
        } else {
            &pattern
        };
        self.steps(&step.children, Some(root))
    }

    fn complete(
        &self,
        step: &Step,
        format: FormatPattern,
        by_exclusion: bool,
    ) -> CompleteClassification {
        CompleteClassification {
            format,
            page: self.written_page,
            entry: self.entry,
            decided_by: DecidedBy::Yaml {
                step: step.key.to_string(),
                path: self.path.clone(),
            },
            by_exclusion,
        }
    }
}

/// Who decided a complete outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecidedBy {
    /// A Python entry, by the name of its pipeline.
    Python { pipeline: PipelineName },
    /// A YAML step: its key as written (`*-EN23` for a template, the full pattern otherwise), and
    /// its position — the index among the entry's steps, then among each level's children.
    Yaml { step: String, path: Vec<usize> },
}

/// A complete outcome, with what decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteClassification {
    pub format: FormatPattern,
    /// The page number of the deciding entry, as written.
    pub page: NonZeroI32,
    /// The index of the deciding entry in its own list (Python entries or YAML entries).
    pub entry: usize,
    pub decided_by: DecidedBy,
    /// Decided by an unconditional step (§3.2, D6): the document does not say it, it is the
    /// "otherwise" of a block.
    pub by_exclusion: bool,
}

/// The outcome of a document classification (§3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocClassification {
    Complete(CompleteClassification),
    /// The accumulated partial, and the formats of the repository refining it (`expand`), in
    /// `formats.csv` order.
    Partial {
        pattern: FormatPattern,
        candidates: Vec<String>,
    },
    Unclassified,
}

/// A classification that could not be carried out. Every Python variant names the entry, its
/// pipeline and its page.
#[derive(Debug, thiserror::Error)]
pub enum DocClassifyError {
    #[error("page {page} could not be read: {source}")]
    Document {
        page: NonZeroI32,
        #[source]
        source: DocumentError,
    },
    #[error(
        "document classifier entry {entry} (pipeline `{pipeline}`, page {page}) failed: {source}"
    )]
    Pipe {
        entry: usize,
        pipeline: PipelineName,
        page: NonZeroI32,
        #[source]
        source: PipeError,
    },
    #[error(
        "document classifier entry {entry} (pipeline `{pipeline}`, page {page}) gave two different formats: '{first}' and '{second}'"
    )]
    ConflictingDocClass {
        entry: usize,
        pipeline: PipelineName,
        page: NonZeroI32,
        first: FormatPattern,
        second: FormatPattern,
    },
    #[error(
        "document classifier entry {entry} (pipeline `{pipeline}`, page {page}) returned {found}, not a document class"
    )]
    NotADocClassification {
        entry: usize,
        pipeline: PipelineName,
        page: NonZeroI32,
        found: String,
    },
    #[error(
        "document classifier entry {entry} (pipeline `{pipeline}`, page {page}) gave '{format}', which is not a format of the repository"
    )]
    UnknownFormat {
        entry: usize,
        pipeline: PipelineName,
        page: NonZeroI32,
        format: FormatPattern,
    },
    #[error(
        "document classifier entry {entry} (pipeline `{pipeline}`, page {page}) gave '{pattern}', which no format of the repository refines"
    )]
    PartialMatchesNoFormat {
        entry: usize,
        pipeline: PipelineName,
        page: NonZeroI32,
        pattern: FormatPattern,
    },
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::core::classes::{BlockType, PdfBlock, TextBlock};
    use crate::core::pipeline::segment::test_pipes::RecordingFilter;
    use crate::core::pipeline::{DeserializePipe, Extracted, PdfExtractPipe, TextFilterPipe};
    use crate::formats_utils::deserialize::standard_funcs::DeserializerDocClassifyStandard;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
    use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;
    use crate::formats_utils::pdf_extract::standard_funcs::PdfExtractDocClassifyStandard;
    use crate::formats_utils::text_filter::standard_funcs::TextFilterDocClassifyStandard;
    use crate::input::document::lazy::resolve_page_number;

    // ------------------------------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------------------------------

    /// A small repository: enough entities, languages, years, locations and variants for every
    /// rule of §3 to have something to bite on.
    const FORMATS: &[&str] = &[
        "AMUNDI-EN24",
        "ASTERIA-EN23",
        "ASTERIA-EN24",
        "EURIZON-EN23",
        "FIDEURAM-IT21",
        "FIDEURAM-IT23@LUX",
        "MEDIOLANUM-EN23",
        "MEDIOLANUM-ES21",
        "MEDIOLANUM-ES24.A",
        "MEDIOLANUM-ES24.B",
        "MEDIOLANUM-IT23",
        "MEDIOLANUM-IT24.A",
        "MEDIOLANUM-IT24.B",
    ];

    fn formats() -> Vec<String> {
        FORMATS.iter().map(|f| f.to_string()).collect()
    }

    fn formats_without(excluded: &str) -> Vec<String> {
        FORMATS
            .iter()
            .filter(|f| **f != excluded)
            .map(|f| f.to_string())
            .collect()
    }

    fn n(number: i32) -> NonZeroI32 {
        NonZeroI32::new(number).expect("test page numbers are never zero")
    }

    fn p(text: &str) -> FormatPattern {
        text.parse().expect("fixture pattern must be valid")
    }

    fn t(text: &str) -> FormatTemplate {
        text.parse().expect("fixture template must be valid")
    }

    fn text(pattern: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_text(pattern))
    }

    fn criteria(patterns: &[&str]) -> Vec<PdfLineSelection> {
        patterns.iter().map(|pattern| text(pattern)).collect()
    }

    /// A step with criteria and no children.
    fn leaf(key: &str, patterns: &[&str]) -> Step {
        Step {
            key: StepKey::Pattern(p(key)),
            criteria: criteria(patterns),
            children: vec![],
        }
    }

    /// A step with (possibly no) criteria and children.
    fn block(key: &str, patterns: &[&str], children: Vec<Step>) -> Step {
        Step {
            key: StepKey::Pattern(p(key)),
            criteria: criteria(patterns),
            children,
        }
    }

    /// No criteria, no children: the "otherwise" (§3.2).
    fn unconditional(key: &str) -> Step {
        Step {
            key: StepKey::Pattern(p(key)),
            criteria: vec![],
            children: vec![],
        }
    }

    fn template(key: &str, patterns: &[&str]) -> Step {
        Step {
            key: StepKey::Template(t(key)),
            criteria: criteria(patterns),
            children: vec![],
        }
    }

    fn entry(page: i32, steps: Vec<Step>) -> YamlEntry {
        YamlEntry {
            page: n(page),
            steps,
        }
    }

    fn py(page: i32, pipeline: Pipeline) -> PythonEntry {
        PythonEntry {
            page: n(page),
            pipeline,
        }
    }

    fn yaml_only(yaml: Vec<YamlEntry>) -> DocumentClassifier {
        DocumentClassifier::new(formats(), vec![], yaml)
    }

    fn python_only(python: Vec<PythonEntry>) -> DocumentClassifier {
        DocumentClassifier::new(formats(), python, vec![])
    }

    /// The pipeline a Python author writes with the three standard bricks of phase 5.
    fn standard_pipeline(name: &str, patterns: &[&str], pattern: &str) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline
            .pdf_extract
            .push(Arc::new(PdfExtractDocClassifyStandard::new(
                criteria(patterns),
                p(pattern),
            )));
        pipeline
            .text_filter
            .push(Arc::new(TextFilterDocClassifyStandard));
        pipeline
            .deserialize
            .push(Arc::new(DeserializerDocClassifyStandard));
        pipeline
    }

    /// One standard pipeline holding several extraction pipes, each with its own pattern: the
    /// standard filter reduces their contributions, and fails on two different values.
    fn standard_pipeline_with(name: &str, pipes: &[(&[&str], &str)]) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        for (patterns, pattern) in pipes {
            pipeline
                .pdf_extract
                .push(Arc::new(PdfExtractDocClassifyStandard::new(
                    criteria(patterns),
                    p(pattern),
                )));
        }
        pipeline
            .text_filter
            .push(Arc::new(TextFilterDocClassifyStandard));
        pipeline
            .deserialize
            .push(Arc::new(DeserializerDocClassifyStandard));
        pipeline
    }

    /// Exactly one block per page, whatever the page holds: the deserializer below runs once.
    struct OneBlock;

    impl PdfExtractPipe for OneBlock {
        fn name(&self) -> &str {
            "one-block"
        }

        fn extract(&self, _page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
            Ok(vec![PdfBlock::bare(BlockType::DOC_CLASS, "")])
        }
    }

    /// Returns the given results, whatever the block: an author deserializer saying exactly this.
    struct Emits(Vec<Extracted>);

    impl DeserializePipe for Emits {
        fn name(&self) -> &str {
            "emits"
        }

        fn deserialize(&self, _block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
            Ok(self.0.clone())
        }
    }

    fn pipeline_emitting(name: &str, results: Vec<Extracted>) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline.pdf_extract.push(Arc::new(OneBlock));
        pipeline
            .text_filter
            .push(RecordingFilter::new("pass") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(Arc::new(Emits(results)));
        pipeline
    }

    fn doc_class(pattern: Option<&str>) -> Extracted {
        Extracted::DocClass(pattern.map(p))
    }

    /// A pipeline whose outcome is `pattern` on every page that exists.
    fn says(name: &str, pattern: &str) -> Pipeline {
        pipeline_emitting(name, vec![doc_class(Some(pattern))])
    }

    /// A document held in memory, which records every page number it is asked for, and fails
    /// to convert the physical pages listed in `broken` (1-based).
    struct FakeDocument {
        pages: Vec<Vec<String>>,
        requests: Mutex<Vec<i32>>,
        broken: Vec<u32>,
    }

    impl FakeDocument {
        fn new(pages: &[&[&str]]) -> Self {
            FakeDocument {
                pages: pages
                    .iter()
                    .map(|lines| lines.iter().map(|l| l.to_string()).collect())
                    .collect(),
                requests: Mutex::new(Vec::new()),
                broken: Vec::new(),
            }
        }

        /// `count` pages with no text.
        fn blank(count: usize) -> Self {
            FakeDocument {
                pages: vec![Vec::new(); count],
                requests: Mutex::new(Vec::new()),
                broken: Vec::new(),
            }
        }

        /// The same document, with physical page `number` failing to convert.
        fn breaking(mut self, number: u32) -> Self {
            self.broken.push(number);
            self
        }

        /// Every page number asked for, in order, as written.
        fn requests(&self) -> Vec<i32> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl PageSource for FakeDocument {
        fn page_count(&self) -> usize {
            self.pages.len()
        }

        fn page(&self, number: NonZeroI32) -> Result<Option<Page>, DocumentError> {
            self.requests.lock().unwrap().push(number.get());
            if let Some(index) = resolve_page_number(number, self.pages.len()) {
                let physical = index as u32 + 1;
                if self.broken.contains(&physical) {
                    return Err(DocumentError::Page {
                        number: physical,
                        source: crate::core::page::PageError::ParseFail {
                            message: "broken page".to_string(),
                        },
                    });
                }
            }
            Ok(resolve_page_number(number, self.pages.len()).map(|index| {
                let lines = self.pages[index]
                    .iter()
                    .enumerate()
                    .map(|(i, text)| {
                        let y = i as f32 * 12.0;
                        PdfLine::new("Arial", 10.0, text, (0.0, y, 200.0, y + 10.0))
                    })
                    .collect();
                Page::new(index as u32 + 1, (595.0, 842.0), lines, vec![])
            }))
        }
    }

    fn complete(outcome: &DocClassification) -> &CompleteClassification {
        match outcome {
            DocClassification::Complete(complete) => complete,
            other => panic!("expected a complete outcome, got {other:?}"),
        }
    }

    fn assert_complete(outcome: &DocClassification, format: &str) {
        assert_eq!(complete(outcome).format, p(format), "outcome: {outcome:?}");
    }

    fn assert_partial(outcome: &DocClassification, pattern: &str) {
        match outcome {
            DocClassification::Partial { pattern: found, .. } => assert_eq!(found, &p(pattern)),
            other => panic!("expected the partial '{pattern}', got {other:?}"),
        }
    }

    fn yaml_step(outcome: &DocClassification) -> (String, Vec<usize>) {
        match &complete(outcome).decided_by {
            DecidedBy::Yaml { step, path } => (step.clone(), path.clone()),
            other => panic!("expected a YAML step, got {other:?}"),
        }
    }

    // ------------------------------------------------------------------------------------------
    // Log capture, as in `core::pipeline::segment` and `formats_utils::deserialize::cast`
    // ------------------------------------------------------------------------------------------

    mod capture {
        use std::sync::{Arc, Mutex};
        use tracing::field::{Field, Visit};
        use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
        use tracing_subscriber::registry::Registry;

        /// One event: its level, and its message and fields rendered as `name=value`.
        #[derive(Debug, Clone)]
        pub(super) struct Event {
            pub(super) level: tracing::Level,
            pub(super) text: String,
        }

        #[derive(Default)]
        struct Render(String);

        impl Visit for Render {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                self.0.push_str(&format!(" {}={:?}", field.name(), value));
            }

            fn record_str(&mut self, field: &Field, value: &str) {
                self.0.push_str(&format!(" {}={}", field.name(), value));
            }
        }

        #[derive(Clone, Default)]
        struct CapturingLayer {
            events: Arc<Mutex<Vec<Event>>>,
        }

        impl<S: tracing::Subscriber> Layer<S> for CapturingLayer {
            fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
                let mut render = Render::default();
                event.record(&mut render);
                self.events.lock().unwrap().push(Event {
                    level: *event.metadata().level(),
                    text: render.0,
                });
            }
        }

        /// Runs `f` under a dedicated subscriber; returns its result and every event it emitted.
        ///
        /// Holds [`super::SERIAL`] for the whole capture, and rebuilds the callsite interest cache
        /// once the capturing dispatcher is installed: see `SERIAL` for the race this closes.
        pub(super) fn events_of<T>(f: impl FnOnce() -> T) -> (T, Vec<Event>) {
            let _guard = super::SERIAL
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let layer = CapturingLayer::default();
            let subscriber = Registry::default().with(layer.clone());
            let result = tracing::subscriber::with_default(subscriber, || {
                tracing::callsite::rebuild_interest_cache();
                f()
            });
            let events = layer.events.lock().unwrap().clone();
            (result, events)
        }

        pub(super) fn at(events: &[Event], level: tracing::Level) -> Vec<Event> {
            events
                .iter()
                .filter(|event| event.level == level)
                .cloned()
                .collect()
        }
    }

    use capture::{at, events_of};

    /// Serializes **every** test of this module that runs a classification, capturing logs or not.
    ///
    /// The race is the process-wide one documented on `SERIAL` in `core::tracing_setup`: a
    /// `tracing_core` callsite caches its `Interest` from the dispatchers alive when it is first
    /// hit, on any thread. A test that does not capture, running the engine on another thread at
    /// the moment a capturing test installs its dispatcher, could leave the engine's own `INFO` and
    /// `WARN` callsites cached as uninteresting — the capture then saw `[]` (observed: about eight
    /// runs in twenty for the D4 warning test, run in parallel). One lock for every run of the
    /// engine here removes the overlap instead of narrowing it.
    pub(crate) static SERIAL: Mutex<()> = Mutex::new(());

    /// [`DocumentClassifier::classify`] under [`SERIAL`]. Every test calls the engine through this,
    /// except inside [`events_of`], which already holds the lock.
    trait ClassifySerialized {
        fn classify_serialized(
            &self,
            source: &dyn PageSource,
        ) -> Result<DocClassification, DocClassifyError>;
    }

    impl ClassifySerialized for DocumentClassifier {
        fn classify_serialized(
            &self,
            source: &dyn PageSource,
        ) -> Result<DocClassification, DocClassifyError> {
            let _guard = SERIAL
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.classify(source)
        }
    }

    // ==========================================================================================
    // The tests
    // ==========================================================================================

    /// What can be asked of an empty classifier and of the step key alone.
    mod basics {
        use super::*;

        #[test]
        fn an_empty_classifier_leaves_the_document_unclassified_and_reads_no_page() {
            let document = FakeDocument::blank(3);
            let outcome = DocumentClassifier::new(formats(), vec![], vec![])
                .classify_serialized(&document)
                .unwrap();
            assert_eq!(outcome, DocClassification::Unclassified);
            assert_eq!(document.requests(), Vec::<i32>::new());
        }

        #[test]
        fn a_step_key_displays_as_written() {
            assert_eq!(
                StepKey::Pattern(p("MEDIOLANUM-IT24.*")).to_string(),
                "MEDIOLANUM-IT24.*"
            );
            assert_eq!(
                StepKey::Pattern(p("FIDEURAM-IT23@*")).to_string(),
                "FIDEURAM-IT23@*"
            );
            assert_eq!(StepKey::Template(t("*-EN23")).to_string(), "*-EN23");
        }

        /// The formats are given back as they were given, in `formats.csv` order: the combination
        /// of the sources (phase 10) lists candidates in that order.
        #[test]
        fn the_formats_are_those_given_in_their_order() {
            let given = vec![
                "MEDIOLANUM-IT24.B".to_string(),
                "AMUNDI-EN24".to_string(),
                "MEDIOLANUM-IT24.A".to_string(),
            ];
            let classifier = DocumentClassifier::new(given.clone(), vec![], vec![]);
            assert_eq!(classifier.formats(), given.as_slice());
        }

        #[test]
        fn lazy_document_is_a_page_source() {
            fn is_page_source<T: PageSource>() {}
            is_page_source::<LazyDocument>();
        }
    }

    /// Rules 3.1.1–3.1.3 as the free functions the run and the load-time validation share (phase
    /// 9: the reachability check must use the engine's own definition of candidability).
    mod shared_rules {
        use super::*;

        mod is_candidable_at_the_top_level {
            use super::*;

            #[test]
            fn anything_is_candidable_with_nothing_accumulated() {
                assert!(is_candidable(&p("AMUNDI-EN24"), None, None));
                assert!(is_candidable(&p("MEDIOLANUM"), None, None));
            }

            #[test]
            fn a_strict_refinement_of_the_accumulated_partial_is_candidable() {
                assert!(is_candidable(
                    &p("MEDIOLANUM-IT24.A"),
                    None,
                    Some(&p("MEDIOLANUM"))
                ));
                assert!(is_candidable(
                    &p("MEDIOLANUM-IT24.A"),
                    None,
                    Some(&p("MEDIOLANUM-IT24.*"))
                ));
            }

            #[test]
            fn the_accumulated_partial_itself_is_not_candidable() {
                assert!(!is_candidable(
                    &p("MEDIOLANUM"),
                    None,
                    Some(&p("MEDIOLANUM"))
                ));
            }

            #[test]
            fn a_pattern_the_accumulated_partial_already_refines_is_not_candidable() {
                assert!(!is_candidable(
                    &p("MEDIOLANUM"),
                    None,
                    Some(&p("MEDIOLANUM-IT"))
                ));
            }

            #[test]
            fn an_incompatible_pattern_is_not_candidable() {
                assert!(!is_candidable(
                    &p("AMUNDI-EN24"),
                    None,
                    Some(&p("MEDIOLANUM"))
                ));
                assert!(!is_candidable(
                    &p("MEDIOLANUM-EN23"),
                    None,
                    Some(&p("MEDIOLANUM-IT"))
                ));
            }

            /// Compatible but not comparable: neither refines the other.
            #[test]
            fn a_compatible_but_not_comparable_pattern_is_not_candidable() {
                assert!(!is_candidable(
                    &p("X-IT23@*.B"),
                    None,
                    Some(&p("X-IT23@LUX.*"))
                ));
            }
        }

        mod is_candidable_when_nested {
            use super::*;

            /// Rule 3.1.2: a nested step needs its root already found.
            #[test]
            fn nothing_accumulated_never_makes_a_nested_step_candidable() {
                assert!(!is_candidable(
                    &p("MEDIOLANUM-EN23"),
                    Some(&p("MEDIOLANUM")),
                    None
                ));
            }

            #[test]
            fn the_root_found_makes_a_strict_refinement_candidable() {
                assert!(is_candidable(
                    &p("MEDIOLANUM-EN23"),
                    Some(&p("MEDIOLANUM")),
                    Some(&p("MEDIOLANUM"))
                ));
            }

            #[test]
            fn a_refinement_of_the_root_found_counts_as_the_root_found() {
                assert!(is_candidable(
                    &p("MEDIOLANUM-ES24.A"),
                    Some(&p("MEDIOLANUM")),
                    Some(&p("MEDIOLANUM-ES"))
                ));
            }

            /// D2: below `MEDIOLANUM: {text, ES: {text, 24.*: {A: …}}}` the root of `24.A` is
            /// `MEDIOLANUM-ES`, and `A = MEDIOLANUM` is not enough.
            #[test]
            fn an_accumulated_partial_coarser_than_the_root_does_not_make_it_candidable() {
                assert!(!is_candidable(
                    &p("MEDIOLANUM-ES24.A"),
                    Some(&p("MEDIOLANUM-ES")),
                    Some(&p("MEDIOLANUM"))
                ));
            }

            #[test]
            fn the_root_found_does_not_lift_rule_3_1_1() {
                assert!(!is_candidable(
                    &p("MEDIOLANUM-ES"),
                    Some(&p("MEDIOLANUM")),
                    Some(&p("MEDIOLANUM-ES"))
                ));
                assert!(!is_candidable(
                    &p("MEDIOLANUM-IT23"),
                    Some(&p("MEDIOLANUM")),
                    Some(&p("MEDIOLANUM-ES"))
                ));
            }

            #[test]
            fn an_accumulated_partial_of_another_entity_does_not_make_it_candidable() {
                assert!(!is_candidable(
                    &p("MEDIOLANUM-EN23"),
                    Some(&p("MEDIOLANUM")),
                    Some(&p("AMUNDI"))
                ));
            }
        }

        mod the_template_instance {
            use super::*;

            #[test]
            fn nothing_accumulated_instantiates_nothing() {
                assert_eq!(template_instance(&t("*-EN23"), None, &formats()), None);
            }

            #[test]
            fn a_complete_instance_the_repository_has_is_the_instance() {
                assert_eq!(
                    template_instance(&t("*-EN23"), Some(&p("MEDIOLANUM")), &formats()),
                    Some(p("MEDIOLANUM-EN23"))
                );
            }

            /// The entity is the accumulated partial's, however refined that partial is.
            #[test]
            fn the_instance_takes_only_the_entity_of_the_accumulated_partial() {
                assert_eq!(
                    template_instance(&t("*-EN23"), Some(&p("MEDIOLANUM-IT")), &formats()),
                    Some(p("MEDIOLANUM-EN23"))
                );
            }

            /// The user's rule (§5, 2026-10-11): the `*` simply does not start.
            #[test]
            fn a_complete_instance_the_repository_lacks_is_none() {
                assert_eq!(
                    template_instance(
                        &t("*-EN23"),
                        Some(&p("MEDIOLANUM")),
                        &formats_without("MEDIOLANUM-EN23")
                    ),
                    None
                );
            }

            #[test]
            fn a_partial_instance_expanding_to_some_format_is_the_instance() {
                assert_eq!(
                    template_instance(&t("*-ES"), Some(&p("MEDIOLANUM")), &formats()),
                    Some(p("MEDIOLANUM-ES"))
                );
                assert_eq!(
                    template_instance(&t("*-IT23@*"), Some(&p("FIDEURAM")), &formats()),
                    Some(p("FIDEURAM-IT23@*"))
                );
            }

            #[test]
            fn a_partial_instance_expanding_to_nothing_is_none() {
                assert_eq!(
                    template_instance(&t("*-FR"), Some(&p("MEDIOLANUM")), &formats()),
                    None
                );
            }

            /// Rule 3.1.3 only: whether the instance strictly refines `A` is rule 3.1.1's business.
            #[test]
            fn an_instance_that_does_not_refine_the_accumulated_partial_is_still_the_instance() {
                assert_eq!(
                    template_instance(&t("*-EN23"), Some(&p("MEDIOLANUM-EN23")), &formats()),
                    Some(p("MEDIOLANUM-EN23"))
                );
            }
        }
    }

    /// Which pages are converted, in which order, and how many times (§2.1, plan phase 6).
    mod page_access {
        use super::*;

        fn never_matching(page: i32) -> YamlEntry {
            entry(page, vec![leaf("EURIZON-EN23", &["^never on any page$"])])
        }

        #[test]
        fn pages_are_read_in_the_order_the_entries_are_written() {
            let document = FakeDocument::blank(10);
            let classifier = yaml_only([1, 3, 5, 2, -2].into_iter().map(never_matching).collect());
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
            assert_eq!(document.requests(), vec![1, 3, 5, 2, -2]);
        }

        #[test]
        fn a_document_shorter_than_the_pages_asked_for_is_still_asked_in_order() {
            // Pages past the end are asked for (the source answers `Ok(None)`) and never cached;
            // a page that exists is converted once, however it is written. So `-2` is asked for
            // only on 1 page (where it does not exist); on 2 pages it is page 1, on 4 pages page 3,
            // both already converted.
            for (page_count, expected) in [
                (1, vec![1, 3, 5, 2, -2]),
                (2, vec![1, 3, 5, 2]),
                (4, vec![1, 3, 5, 2]),
            ] {
                let document = FakeDocument::blank(page_count);
                let classifier =
                    yaml_only([1, 3, 5, 2, -2].into_iter().map(never_matching).collect());
                assert_eq!(
                    classifier.classify_serialized(&document).unwrap(),
                    DocClassification::Unclassified
                );
                assert_eq!(document.requests(), expected, "{page_count} pages");
            }
        }

        #[test]
        fn a_page_past_the_end_is_nothing_for_its_entry_and_the_next_entries_still_run() {
            let document = FakeDocument::new(&[&["^cover"], &["Eurizon Fund"]]);
            let classifier = yaml_only(vec![
                entry(5, vec![leaf("AMUNDI-EN24", &["Eurizon Fund"])]),
                entry(-2, vec![leaf("AMUNDI-EN24", &["^Eurizon Fund$"])]),
                entry(-1, vec![leaf("EURIZON-EN23", &["^Eurizon Fund$"])]),
            ]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "EURIZON-EN23");
            assert_eq!(complete(&outcome).page, n(-1));
            assert_eq!(complete(&outcome).entry, 2);
        }

        #[test]
        fn an_unconditional_step_on_a_missing_page_does_not_match() {
            let document = FakeDocument::blank(2);
            let classifier = yaml_only(vec![entry(5, vec![unconditional("ASTERIA-EN23")])]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn a_python_entry_on_a_missing_page_is_nothing_and_its_pipeline_never_runs() {
            let document = FakeDocument::blank(2);
            // Were the pipeline run, its outcome would be a complete name.
            let classifier = python_only(vec![py(7, says("p", "AMUNDI-EN24"))]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
            assert_eq!(document.requests(), vec![7]);
        }

        #[test]
        fn the_same_page_in_several_yaml_entries_is_converted_once() {
            let document = FakeDocument::blank(4);
            let classifier = yaml_only(vec![
                never_matching(1),
                never_matching(2),
                never_matching(1),
                never_matching(1),
            ]);
            classifier.classify_serialized(&document).unwrap();
            assert_eq!(document.requests(), vec![1, 2]);
        }

        #[test]
        fn a_page_written_positive_and_negative_is_converted_once() {
            let document = FakeDocument::blank(10);
            let classifier = yaml_only(vec![
                never_matching(1),
                never_matching(-10),
                never_matching(1),
            ]);
            classifier.classify_serialized(&document).unwrap();
            assert_eq!(document.requests(), vec![1]);
        }

        #[test]
        fn the_last_page_written_both_ways_is_converted_once() {
            let document = FakeDocument::blank(10);
            let classifier = yaml_only(vec![
                never_matching(-1),
                never_matching(3),
                never_matching(10),
                never_matching(-8),
            ]);
            classifier.classify_serialized(&document).unwrap();
            assert_eq!(document.requests(), vec![-1, 3]);
        }

        #[test]
        fn the_physical_page_is_shared_between_python_and_yaml_entries() {
            let document = FakeDocument::blank(4);
            let classifier = DocumentClassifier::new(
                formats(),
                vec![py(-4, pipeline_emitting("p", vec![doc_class(None)]))],
                vec![never_matching(1), never_matching(2), never_matching(-3)],
            );
            classifier.classify_serialized(&document).unwrap();
            assert_eq!(document.requests(), vec![-4, 2]);
        }

        #[test]
        fn what_a_page_says_does_not_depend_on_how_its_number_is_written() {
            let document = FakeDocument::new(&[&["Mediolanum"], &["Best Brands"]]);
            let classifier = yaml_only(vec![
                entry(1, vec![leaf("MEDIOLANUM", &["^Mediolanum$"])]),
                entry(-2, vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])]),
                entry(-1, vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])]),
            ]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "MEDIOLANUM-IT24.A");
            assert_eq!(complete(&outcome).page, n(-1));
            assert_eq!(document.requests(), vec![1, -1]);
        }

        #[test]
        fn the_same_page_in_python_and_yaml_entries_is_converted_once() {
            let document = FakeDocument::blank(4);
            let classifier = DocumentClassifier::new(
                formats(),
                vec![
                    py(1, pipeline_emitting("p", vec![doc_class(None)])),
                    py(3, pipeline_emitting("q", vec![doc_class(None)])),
                ],
                vec![never_matching(3), never_matching(1)],
            );
            classifier.classify_serialized(&document).unwrap();
            assert_eq!(document.requests(), vec![1, 3]);
        }

        #[test]
        fn a_complete_yaml_outcome_stops_before_the_next_page_is_converted() {
            let document = FakeDocument::new(&[&["Eurizon Fund"], &["anything"], &["anything"]]);
            let classifier = yaml_only(vec![
                entry(1, vec![leaf("EURIZON-EN23", &["^Eurizon Fund$"])]),
                entry(2, vec![leaf("AMUNDI-EN24", &["anything"])]),
                entry(3, vec![leaf("AMUNDI-EN24", &["anything"])]),
            ]);
            assert_complete(
                &classifier.classify_serialized(&document).unwrap(),
                "EURIZON-EN23",
            );
            assert_eq!(document.requests(), vec![1]);
        }

        #[test]
        fn a_complete_python_outcome_stops_before_the_next_page_is_converted() {
            let document = FakeDocument::blank(3);
            let classifier = python_only(vec![
                py(1, says("p", "AMUNDI-EN24")),
                py(2, says("q", "EURIZON-EN23")),
            ]);
            assert_complete(
                &classifier.classify_serialized(&document).unwrap(),
                "AMUNDI-EN24",
            );
            assert_eq!(document.requests(), vec![1]);
        }

        #[test]
        fn a_complete_step_ends_the_page_too_later_steps_of_the_same_entry_do_not_matter() {
            let document = FakeDocument::new(&[&["Eurizon Fund", "Amundi Funds"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("EURIZON-EN23", &["^Eurizon Fund$"]),
                    leaf("AMUNDI-EN24", &["^Amundi Funds$"]),
                ],
            )]);
            assert_complete(
                &classifier.classify_serialized(&document).unwrap(),
                "EURIZON-EN23",
            );
        }
    }

    /// The pages of a real PDF, through `LazyDocument`: the one way in.
    mod through_lazy_document {
        use super::*;
        use pyo3::prelude::*;
        use pyo3::types::PyDict;

        /// `page_count` pages, page `N` saying `pagina N`.
        fn plain_pdf(dir: &tempfile::TempDir, page_count: i64) -> std::path::PathBuf {
            let path = dir.path().join(format!("plain-{page_count}.pdf"));
            Python::attach(|py| {
                let fitz = PyModule::import(py, "fitz")
                    .expect("PyMuPDF (fitz) must be importable: activate venv/freeports-dev before running this test");
                let globals = PyDict::new(py);
                globals.set_item("fitz", fitz).unwrap();
                globals.set_item("path", path.to_str().unwrap()).unwrap();
                globals.set_item("page_count", page_count).unwrap();
                py.run(
                    c"
doc = fitz.open()
for i in range(1, page_count + 1):
    page = doc.new_page(-1, 200.0, 300.0)
    page.insert_text((20, 50), f'pagina {i}')
doc.save(path)
doc.close()
",
                    Some(&globals),
                    None,
                )
                .expect("the fixture-building snippet must run");
            });
            path
        }

        #[test]
        fn a_step_on_the_last_page_of_a_real_pdf_classifies_it() {
            let dir = tempfile::tempdir().unwrap();
            let document = LazyDocument::open(&plain_pdf(&dir, 3), true).unwrap();
            let classifier = yaml_only(vec![
                entry(1, vec![leaf("AMUNDI-EN24", &["^pagina 3$"])]),
                entry(-1, vec![leaf("EURIZON-EN23", &["^pagina 3$"])]),
            ]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "EURIZON-EN23");
            assert_eq!(complete(&outcome).page, n(-1));
        }

        #[test]
        fn a_page_past_the_end_of_a_real_pdf_is_nothing() {
            let dir = tempfile::tempdir().unwrap();
            let document = LazyDocument::open(&plain_pdf(&dir, 2), true).unwrap();
            let classifier = yaml_only(vec![entry(5, vec![unconditional("ASTERIA-EN23")])]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }
    }

    /// A YAML step with criteria (§3.2): every criterion must find a line.
    mod yaml_criteria {
        use super::*;

        #[test]
        fn a_complete_step_whose_criteria_all_find_a_line_classifies() {
            let document = FakeDocument::new(&[&["Eurizon Fund", "Annual Report 2023"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![leaf("EURIZON-EN23", &["^Eurizon Fund$", "^Annual Report"])],
            )]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_eq!(
                outcome,
                DocClassification::Complete(CompleteClassification {
                    format: p("EURIZON-EN23"),
                    page: n(1),
                    entry: 0,
                    decided_by: DecidedBy::Yaml {
                        step: "EURIZON-EN23".to_string(),
                        path: vec![0]
                    },
                    by_exclusion: false,
                })
            );
        }

        #[test]
        fn one_criterion_without_a_line_is_enough_to_fail() {
            let document = FakeDocument::new(&[&["Eurizon Fund"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![leaf("EURIZON-EN23", &["^Eurizon Fund$", "^Annual Report"])],
            )]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn a_page_without_text_matches_no_step_with_criteria() {
            let document = FakeDocument::blank(1);
            let classifier = yaml_only(vec![entry(1, vec![leaf("EURIZON-EN23", &[".*"])])]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn the_criteria_may_be_found_on_different_lines() {
            let document = FakeDocument::new(&[&["first", "Eurizon Fund", "other", "2023"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![leaf("EURIZON-EN23", &["^Eurizon Fund$", "^2023$"])],
            )]);
            assert_complete(
                &classifier.classify_serialized(&document).unwrap(),
                "EURIZON-EN23",
            );
        }
    }

    /// Partial outcomes and how `A` accumulates (§3.1.1, §3.2, §3.3).
    mod partials {
        use super::*;

        #[test]
        fn a_matching_partial_step_alone_gives_a_partial_outcome_with_its_candidates() {
            let document = FakeDocument::new(&[&["Mediolanum"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![leaf("MEDIOLANUM-IT24.*", &["^Mediolanum$"])],
            )]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Partial {
                    pattern: p("MEDIOLANUM-IT24.*"),
                    candidates: vec![
                        "MEDIOLANUM-IT24.A".to_string(),
                        "MEDIOLANUM-IT24.B".to_string()
                    ],
                }
            );
        }

        #[test]
        fn the_candidates_follow_the_order_of_the_formats() {
            let document = FakeDocument::new(&[&["Mediolanum"]]);
            let classifier = yaml_only(vec![entry(1, vec![leaf("MEDIOLANUM", &["^Mediolanum$"])])]);
            let expected: Vec<String> = FORMATS
                .iter()
                .filter(|f| f.starts_with("MEDIOLANUM-"))
                .map(|f| f.to_string())
                .collect();
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Partial {
                    pattern: p("MEDIOLANUM"),
                    candidates: expected
                }
            );
        }

        #[test]
        fn a_later_partial_that_strictly_refines_the_accumulated_one_replaces_it() {
            let document = FakeDocument::new(&[&["Mediolanum", "Relazione"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    leaf("MEDIOLANUM-IT24.*", &["^Relazione$"]),
                ],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM-IT24.*",
            );
        }

        #[test]
        fn a_partial_of_another_entity_is_not_evaluated_once_one_is_accumulated() {
            let document = FakeDocument::new(&[&["Mediolanum", "Amundi"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    leaf("AMUNDI", &["^Amundi$"]),
                ],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }

        #[test]
        fn a_complete_step_that_does_not_refine_the_accumulated_partial_is_not_evaluated() {
            let document = FakeDocument::new(&[&["Mediolanum", "Amundi Funds"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    leaf("AMUNDI-EN24", &["^Amundi Funds$"]),
                ],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }

        #[test]
        fn a_coarser_partial_after_a_finer_one_does_not_widen_it() {
            let document = FakeDocument::new(&[&["Relazione", "Mediolanum"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM-IT24.*", &["^Relazione$"]),
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                ],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM-IT24.*",
            );
        }

        #[test]
        fn a_complete_step_refining_the_accumulated_partial_ends_the_classification() {
            let document = FakeDocument::new(&[&["Mediolanum", "Best Brands"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"]),
                ],
            )]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "MEDIOLANUM-IT24.A");
            assert_eq!(
                yaml_step(&outcome),
                ("MEDIOLANUM-IT24.A".to_string(), vec![1])
            );
        }

        #[test]
        fn the_accumulated_partial_is_inherited_by_the_next_page() {
            let document = FakeDocument::new(&[&["Mediolanum"], &["Amundi Funds", "Relazione"]]);
            let classifier = yaml_only(vec![
                entry(1, vec![leaf("MEDIOLANUM", &["^Mediolanum$"])]),
                // `AMUNDI-EN24` would match page 2, but `A = MEDIOLANUM` comes from page 1.
                entry(
                    2,
                    vec![
                        leaf("AMUNDI-EN24", &["^Amundi Funds$"]),
                        leaf("MEDIOLANUM-IT24.*", &["^Relazione$"]),
                    ],
                ),
            ]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM-IT24.*",
            );
        }

        #[test]
        fn partials_found_on_three_pages_refine_into_a_complete_name() {
            let document = FakeDocument::new(&[&["Mediolanum"], &["Relazione"], &["Best Brands"]]);
            let classifier = yaml_only(vec![
                entry(1, vec![leaf("MEDIOLANUM", &["^Mediolanum$"])]),
                entry(2, vec![leaf("MEDIOLANUM-IT24.*", &["^Relazione$"])]),
                entry(3, vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])]),
            ]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "MEDIOLANUM-IT24.A");
            assert_eq!(complete(&outcome).page, n(3));
            assert_eq!(complete(&outcome).entry, 2);
        }

        #[test]
        fn a_partial_accumulated_on_an_earlier_page_survives_pages_that_add_nothing() {
            let document = FakeDocument::new(&[&["Mediolanum"], &["nothing"], &["nothing"]]);
            let classifier = yaml_only(vec![
                entry(1, vec![leaf("MEDIOLANUM", &["^Mediolanum$"])]),
                entry(2, vec![leaf("MEDIOLANUM-IT24.*", &["^Relazione$"])]),
                entry(3, vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])]),
            ]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }
    }

    /// Nested steps: §3.1.2 (D2), §3.1.4, D1, and the candidability of the single step (R1).
    mod nesting {
        use super::*;

        #[test]
        fn a_root_with_criteria_that_match_lets_its_children_be_evaluated() {
            // `MEDIOLANUM` → `ES` → `21`, the case named by the plan: `ES` is a prefix.
            let document = FakeDocument::new(&[&["Mediolanum", "Informe 2021"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![block(
                    "MEDIOLANUM",
                    &["^Mediolanum$"],
                    vec![block(
                        "MEDIOLANUM-ES",
                        &[],
                        vec![leaf("MEDIOLANUM-ES21", &["^Informe 2021$"])],
                    )],
                )],
            )]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "MEDIOLANUM-ES21");
            assert_eq!(
                yaml_step(&outcome),
                ("MEDIOLANUM-ES21".to_string(), vec![0, 0, 0])
            );
        }

        #[test]
        fn a_root_with_criteria_that_fail_skips_its_whole_subtree() {
            // §3.1.4: the child's criteria would match, but its root failed.
            let document = FakeDocument::new(&[&["Informe 2021"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![block(
                    "MEDIOLANUM",
                    &["^Mediolanum$"],
                    vec![block(
                        "MEDIOLANUM-ES",
                        &[],
                        vec![leaf("MEDIOLANUM-ES21", &["^Informe 2021$"])],
                    )],
                )],
            )]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn a_nested_ancestor_with_criteria_that_fail_skips_its_subtree_even_with_the_root_found() {
            // R7's example: `MEDIOLANUM: {ES: {text: x, 24.B: ~}}` with `ES` failing — the
            // unconditional `24.B` is never reached and `A` stays `MEDIOLANUM`.
            let document = FakeDocument::new(&[&["Mediolanum"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![block(
                    "MEDIOLANUM",
                    &["^Mediolanum$"],
                    vec![block(
                        "MEDIOLANUM-ES",
                        &["^Informe$"],
                        vec![unconditional("MEDIOLANUM-ES24.B")],
                    )],
                )],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }

        #[test]
        fn a_prefix_without_criteria_filters_nothing() {
            let document = FakeDocument::new(&[&["Mediolanum", "GAMAX"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![block(
                    "MEDIOLANUM",
                    &["^Mediolanum$"],
                    vec![block(
                        "MEDIOLANUM-ES",
                        &[],
                        vec![block(
                            "MEDIOLANUM-ES24.*",
                            &[],
                            vec![leaf("MEDIOLANUM-ES24.A", &["^GAMAX$"])],
                        )],
                    )],
                )],
            )]);
            assert_complete(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM-ES24.A",
            );
        }

        #[test]
        fn a_nested_step_is_never_evaluated_with_nothing_accumulated() {
            // §3.1.2: a block without criteria at the top, `A` null — the root was never found.
            let document = FakeDocument::new(&[&["Annual Report"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![block(
                    "MEDIOLANUM",
                    &[],
                    vec![leaf("MEDIOLANUM-EN", &["^Annual Report$"])],
                )],
            )]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn a_top_level_prefix_does_not_set_the_accumulated_partial() {
            let document = FakeDocument::new(&[&["nothing"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![block(
                    "MEDIOLANUM",
                    &[],
                    vec![leaf("MEDIOLANUM-EN", &["^Annual Report$"])],
                )],
            )]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn a_prefix_block_after_its_root_was_found_by_an_earlier_step_is_visited() {
            // R1: the block has `P = A`, so it is not candidable itself; its children are.
            let document = FakeDocument::new(&[&["Mediolanum", "Annual Report"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    block(
                        "MEDIOLANUM",
                        &[],
                        vec![leaf("MEDIOLANUM-EN", &["^Annual Report$"])],
                    ),
                ],
            )]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Partial {
                    pattern: p("MEDIOLANUM-EN"),
                    candidates: vec!["MEDIOLANUM-EN23".to_string()]
                }
            );
        }

        #[test]
        fn a_partial_child_becomes_the_context_of_its_own_siblings() {
            // After `EN` matched, `A = MEDIOLANUM-EN`: the sibling `IT23` does not refine it.
            let document = FakeDocument::new(&[&["Mediolanum", "Annual Report", "Relazione 2023"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![block(
                    "MEDIOLANUM",
                    &["^Mediolanum$"],
                    vec![
                        leaf("MEDIOLANUM-EN", &["^Annual Report$"]),
                        leaf("MEDIOLANUM-IT23", &["^Relazione 2023$"]),
                    ],
                )],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM-EN",
            );
        }

        #[test]
        fn a_failing_child_does_not_stop_its_siblings() {
            let document = FakeDocument::new(&[&["Mediolanum", "Relazione 2023"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![block(
                    "MEDIOLANUM",
                    &["^Mediolanum$"],
                    vec![
                        leaf("MEDIOLANUM-EN", &["^Annual Report$"]),
                        leaf("MEDIOLANUM-IT23", &["^Relazione 2023$"]),
                    ],
                )],
            )]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "MEDIOLANUM-IT23");
            assert_eq!(
                yaml_step(&outcome),
                ("MEDIOLANUM-IT23".to_string(), vec![0, 1])
            );
        }

        #[test]
        fn a_block_of_another_entity_is_skipped_with_a_partial_accumulated() {
            let document = FakeDocument::new(&[&["Mediolanum", "Lux"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    block(
                        "FIDEURAM-IT23@*",
                        &[],
                        vec![leaf("FIDEURAM-IT23@LUX", &["^Lux$"])],
                    ),
                ],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }

        mod d1_context_already_acquired {
            use super::*;

            #[test]
            fn a_root_inherited_from_an_earlier_page_does_not_recheck_its_criteria() {
                let document = FakeDocument::new(&[&["Mediolanum"], &["Best Brands"]]);
                let classifier = yaml_only(vec![
                    entry(1, vec![leaf("MEDIOLANUM", &["^Mediolanum$"])]),
                    entry(
                        2,
                        vec![block(
                            "MEDIOLANUM",
                            &["^Mediolanum$"],
                            vec![block(
                                "MEDIOLANUM-IT24.*",
                                &[],
                                vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])],
                            )],
                        )],
                    ),
                ]);
                let outcome = classifier.classify_serialized(&document).unwrap();
                assert_complete(&outcome, "MEDIOLANUM-IT24.A");
                assert_eq!(complete(&outcome).page, n(2));
            }

            #[test]
            fn a_root_already_refined_by_the_accumulated_partial_lets_the_children_run() {
                // `A = MEDIOLANUM-IT24.*`, strictly finer than the root `MEDIOLANUM`.
                let document = FakeDocument::new(&[&["Relazione"], &["Best Brands"]]);
                let classifier = yaml_only(vec![
                    entry(1, vec![leaf("MEDIOLANUM-IT24.*", &["^Relazione$"])]),
                    entry(
                        2,
                        vec![block(
                            "MEDIOLANUM",
                            &["^Mediolanum$"],
                            vec![block(
                                "MEDIOLANUM-IT24.*",
                                &[],
                                vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])],
                            )],
                        )],
                    ),
                ]);
                assert_complete(
                    &classifier.classify_serialized(&document).unwrap(),
                    "MEDIOLANUM-IT24.A",
                );
            }

            #[test]
            fn a_root_incompatible_with_the_accumulated_partial_keeps_its_subtree_out() {
                let document = FakeDocument::new(&[&["Amundi"], &["Amundi Funds"]]);
                let classifier = yaml_only(vec![
                    entry(1, vec![leaf("AMUNDI", &["^Amundi$"])]),
                    entry(
                        2,
                        vec![block(
                            "MEDIOLANUM",
                            &["^Mediolanum$"],
                            vec![leaf("MEDIOLANUM-EN23", &["^Amundi Funds$"])],
                        )],
                    ),
                ]);
                assert_partial(
                    &classifier.classify_serialized(&document).unwrap(),
                    "AMUNDI",
                );
            }
        }

        mod d2_nearest_ancestor_with_criteria {
            use super::*;

            /// `MEDIOLANUM: {text: …, ES: {text: 'Informe', 24.*: {A: {text: 'GAMAX'}}}}`
            fn d2_classifier() -> DocumentClassifier {
                yaml_only(vec![entry(
                    1,
                    vec![block(
                        "MEDIOLANUM",
                        &["^Mediolanum$"],
                        vec![block(
                            "MEDIOLANUM-ES",
                            &["^Informe$"],
                            vec![block(
                                "MEDIOLANUM-ES24.*",
                                &[],
                                vec![leaf("MEDIOLANUM-ES24.A", &["^GAMAX$"])],
                            )],
                        )],
                    )],
                )])
            }

            #[test]
            fn the_leaf_classifies_once_its_nearest_ancestor_with_criteria_was_found() {
                let document = FakeDocument::new(&[&["Mediolanum", "Informe", "GAMAX"]]);
                let outcome = d2_classifier().classify_serialized(&document).unwrap();
                assert_complete(&outcome, "MEDIOLANUM-ES24.A");
                assert_eq!(
                    yaml_step(&outcome),
                    ("MEDIOLANUM-ES24.A".to_string(), vec![0, 0, 0, 0])
                );
            }

            #[test]
            fn finding_only_the_top_level_root_is_not_enough() {
                let document = FakeDocument::new(&[&["Mediolanum", "GAMAX"]]);
                assert_partial(
                    &d2_classifier().classify_serialized(&document).unwrap(),
                    "MEDIOLANUM",
                );
            }

            #[test]
            fn the_intermediate_context_inherited_from_an_earlier_page_is_enough() {
                // Page 1 found `MEDIOLANUM-ES`; on page 2 neither ancestor's criteria hold, and
                // neither is rechecked (D1).
                let document = FakeDocument::new(&[&["Mediolanum", "Informe"], &["GAMAX"]]);
                let mut classifier = d2_classifier();
                classifier.yaml.push(YamlEntry {
                    page: n(2),
                    steps: d2_classifier().yaml[0].steps.clone(),
                });
                let outcome = classifier.classify_serialized(&document).unwrap();
                assert_complete(&outcome, "MEDIOLANUM-ES24.A");
                assert_eq!(complete(&outcome).page, n(2));
            }

            #[test]
            fn with_a_chain_of_prefixes_the_root_is_the_top_level_key() {
                // R12, form 1: `A = MEDIOLANUM` satisfies the top-level key of a chain of prefixes.
                let document = FakeDocument::new(&[&["Mediolanum", "Best Brands"]]);
                let classifier = yaml_only(vec![entry(
                    1,
                    vec![
                        leaf("MEDIOLANUM", &["^Mediolanum$"]),
                        block(
                            "MEDIOLANUM",
                            &[],
                            vec![block(
                                "MEDIOLANUM-IT24.*",
                                &[],
                                vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])],
                            )],
                        ),
                    ],
                )]);
                assert_complete(
                    &classifier.classify_serialized(&document).unwrap(),
                    "MEDIOLANUM-IT24.A",
                );
            }

            #[test]
            fn a_flattened_prefix_root_must_itself_be_refined_by_the_accumulated_partial() {
                // R12, form 2: the root is `MEDIOLANUM-IT24.*`, and `A = MEDIOLANUM` does not
                // refine it — flattening the chain changes the outcome, as the specification says.
                let document = FakeDocument::new(&[&["Mediolanum", "Best Brands"]]);
                let classifier = yaml_only(vec![entry(
                    1,
                    vec![
                        leaf("MEDIOLANUM", &["^Mediolanum$"]),
                        block(
                            "MEDIOLANUM-IT24.*",
                            &[],
                            vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])],
                        ),
                    ],
                )]);
                assert_partial(
                    &classifier.classify_serialized(&document).unwrap(),
                    "MEDIOLANUM",
                );
            }

            #[test]
            fn a_prefix_root_found_by_a_template_step_lets_its_children_run() {
                // R7 (1): `"*-ES"` turns `A` into `MEDIOLANUM-ES`, the root of the next block.
                let document = FakeDocument::new(&[&["Mediolanum", "Informe", "GAMAX"]]);
                let classifier = yaml_only(vec![entry(
                    1,
                    vec![
                        leaf("MEDIOLANUM", &["^Mediolanum$"]),
                        template("*-ES", &["^Informe$"]),
                        block(
                            "MEDIOLANUM-ES",
                            &[],
                            vec![leaf("MEDIOLANUM-ES24.A", &["^GAMAX$"])],
                        ),
                    ],
                )]);
                assert_complete(
                    &classifier.classify_serialized(&document).unwrap(),
                    "MEDIOLANUM-ES24.A",
                );
            }
        }

        mod depth_first_preorder {
            use super::*;

            #[test]
            fn children_are_visited_before_the_next_sibling_r3() {
                // R3: depth-first gives the partial `MEDIOLANUM-EN`; breadth-first would have
                // given the complete `MEDIOLANUM-IT24.A`.
                let document =
                    FakeDocument::new(&[&["Mediolanum", "Annual Report", "Best Brands"]]);
                let classifier = yaml_only(vec![entry(
                    1,
                    vec![
                        block(
                            "MEDIOLANUM",
                            &["^Mediolanum$"],
                            vec![leaf("MEDIOLANUM-EN", &["^Annual Report$"])],
                        ),
                        leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"]),
                    ],
                )]);
                assert_partial(
                    &classifier.classify_serialized(&document).unwrap(),
                    "MEDIOLANUM-EN",
                );
            }

            #[test]
            fn a_deeper_complete_child_wins_over_a_later_sibling() {
                let document =
                    FakeDocument::new(&[&["Mediolanum", "Annual Report", "Eurizon Fund"]]);
                let classifier = yaml_only(vec![entry(
                    1,
                    vec![
                        block(
                            "MEDIOLANUM",
                            &["^Mediolanum$"],
                            vec![leaf("MEDIOLANUM-EN23", &["^Annual Report$"])],
                        ),
                        leaf("EURIZON-EN23", &["^Eurizon Fund$"]),
                    ],
                )]);
                assert_complete(
                    &classifier.classify_serialized(&document).unwrap(),
                    "MEDIOLANUM-EN23",
                );
            }

            #[test]
            fn after_a_failed_subtree_the_next_sibling_is_tried() {
                let document = FakeDocument::new(&[&["Eurizon Fund", "Annual Report"]]);
                let classifier = yaml_only(vec![entry(
                    1,
                    vec![
                        block(
                            "MEDIOLANUM",
                            &["^Mediolanum$"],
                            vec![leaf("MEDIOLANUM-EN23", &["^Annual Report$"])],
                        ),
                        leaf("EURIZON-EN23", &["^Eurizon Fund$"]),
                    ],
                )]);
                assert_complete(
                    &classifier.classify_serialized(&document).unwrap(),
                    "EURIZON-EN23",
                );
            }
        }
    }

    /// The `*` as entity (§1.4, §3.1.3).
    mod templates {
        use super::*;

        #[test]
        fn a_template_is_instantiated_with_the_entity_of_the_accumulated_partial() {
            let document = FakeDocument::new(&[&["Mediolanum", "Annual Report 2023"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    template("*-EN23", &["^Annual Report 2023$"]),
                ],
            )]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "MEDIOLANUM-EN23");
            assert_eq!(yaml_step(&outcome), ("*-EN23".to_string(), vec![1]));
        }

        #[test]
        fn a_template_is_not_evaluated_with_nothing_accumulated() {
            let document = FakeDocument::new(&[&["Annual Report 2023"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![template("*-EN23", &["^Annual Report 2023$"])],
            )]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn a_complete_instance_the_repository_lacks_is_skipped() {
            let document = FakeDocument::new(&[&["Mediolanum", "Annual Report 2023"]]);
            let classifier = DocumentClassifier::new(
                formats_without("MEDIOLANUM-EN23"),
                vec![],
                vec![entry(
                    1,
                    vec![
                        leaf("MEDIOLANUM", &["^Mediolanum$"]),
                        template("*-EN23", &["^Annual Report 2023$"]),
                    ],
                )],
            );
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }

        #[test]
        fn a_partial_instance_is_accumulated_when_it_expands_to_some_format() {
            let document = FakeDocument::new(&[&["Mediolanum", "Informe"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    template("*-ES", &["^Informe$"]),
                ],
            )]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Partial {
                    pattern: p("MEDIOLANUM-ES"),
                    candidates: vec![
                        "MEDIOLANUM-ES21".to_string(),
                        "MEDIOLANUM-ES24.A".to_string(),
                        "MEDIOLANUM-ES24.B".to_string()
                    ],
                }
            );
        }

        #[test]
        fn a_partial_instance_that_expands_to_nothing_is_skipped() {
            let document = FakeDocument::new(&[&["Eurizon", "Rapport"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("EURIZON", &["^Eurizon$"]),
                    template("*-FR", &["^Rapport$"]),
                ],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "EURIZON",
            );
        }

        #[test]
        fn an_instance_that_does_not_strictly_refine_the_accumulated_partial_is_not_evaluated() {
            // `A = MEDIOLANUM-IT`: `*-EN23` instantiates to `MEDIOLANUM-EN23`, incompatible.
            let document = FakeDocument::new(&[&["Mediolanum", "Annual Report 2023"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM-IT", &["^Mediolanum$"]),
                    template("*-EN23", &["^Annual Report 2023$"]),
                ],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM-IT",
            );
        }

        #[test]
        fn the_instance_uses_the_entity_of_whatever_partial_was_accumulated() {
            let document = FakeDocument::new(&[&["Asteria Funds", "Annual Report 2023"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    leaf("ASTERIA", &["^Asteria Funds$"]),
                    template("*-EN23", &["^Annual Report 2023$"]),
                ],
            )]);
            assert_complete(
                &classifier.classify_serialized(&document).unwrap(),
                "ASTERIA-EN23",
            );
        }

        #[test]
        fn a_template_whose_criteria_fail_changes_nothing() {
            let document = FakeDocument::new(&[&["Mediolanum"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    template("*-EN23", &["^Annual Report 2023$"]),
                ],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }
    }

    /// Steps with neither criteria nor children: they always match (§3.2), and a complete one is
    /// an outcome "by exclusion" (§3.4, D6).
    mod unconditional_steps {
        use super::*;

        /// ASTERIA from §8: `ASTERIA: {text: …, EN24: {text: …}, EN23: ~}`.
        fn asteria() -> DocumentClassifier {
            yaml_only(vec![entry(
                1,
                vec![block(
                    "ASTERIA",
                    &["^Asteria Funds$"],
                    vec![
                        leaf("ASTERIA-EN24", &["^Investment Managers$"]),
                        unconditional("ASTERIA-EN23"),
                    ],
                )],
            )])
        }

        #[test]
        fn the_otherwise_of_a_block_decides_by_exclusion() {
            let document = FakeDocument::new(&[&["Asteria Funds"]]);
            let outcome = asteria().classify_serialized(&document).unwrap();
            assert_eq!(
                outcome,
                DocClassification::Complete(CompleteClassification {
                    format: p("ASTERIA-EN23"),
                    page: n(1),
                    entry: 0,
                    decided_by: DecidedBy::Yaml {
                        step: "ASTERIA-EN23".to_string(),
                        path: vec![0, 1]
                    },
                    by_exclusion: true,
                })
            );
        }

        #[test]
        fn a_sibling_with_criteria_that_match_first_is_not_by_exclusion() {
            let document = FakeDocument::new(&[&["Asteria Funds", "Investment Managers"]]);
            let outcome = asteria().classify_serialized(&document).unwrap();
            assert_complete(&outcome, "ASTERIA-EN24");
            assert!(!complete(&outcome).by_exclusion);
        }

        #[test]
        fn the_otherwise_is_not_reached_when_its_root_fails() {
            let document = FakeDocument::new(&[&["Investment Managers"]]);
            assert_eq!(
                asteria().classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn a_top_level_unconditional_complete_step_decides_any_existing_page() {
            let document = FakeDocument::blank(1);
            let classifier = yaml_only(vec![entry(1, vec![unconditional("ASTERIA-EN23")])]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "ASTERIA-EN23");
            assert!(complete(&outcome).by_exclusion);
        }

        #[test]
        fn a_top_level_unconditional_partial_step_sets_the_accumulated_partial() {
            let document = FakeDocument::blank(1);
            let classifier = yaml_only(vec![entry(
                1,
                vec![unconditional("MEDIOLANUM"), leaf("AMUNDI-EN24", &[".*"])],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }

        #[test]
        fn a_complete_outcome_reached_after_an_unconditional_partial_is_not_by_exclusion() {
            let document = FakeDocument::new(&[&["Best Brands"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    unconditional("MEDIOLANUM-IT24.*"),
                    leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"]),
                ],
            )]);
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "MEDIOLANUM-IT24.A");
            assert!(!complete(&outcome).by_exclusion);
        }

        #[test]
        fn an_unconditional_step_is_still_subject_to_candidability() {
            // `A = MEDIOLANUM`: the unconditional `ASTERIA-EN23` does not refine it.
            let document = FakeDocument::new(&[&["Mediolanum"]]);
            let classifier = yaml_only(vec![entry(
                1,
                vec![
                    leaf("MEDIOLANUM", &["^Mediolanum$"]),
                    unconditional("ASTERIA-EN23"),
                ],
            )]);
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }

        #[test]
        fn an_outcome_by_exclusion_is_logged_at_info_naming_the_step() {
            let document = FakeDocument::new(&[&["Asteria Funds"]]);
            let (outcome, events) = events_of(|| asteria().classify(&document).unwrap());
            assert!(complete(&outcome).by_exclusion);
            let infos = at(&events, tracing::Level::INFO);
            assert!(
                infos
                    .iter()
                    .any(|event| event.text.contains("ASTERIA-EN23")),
                "expected an INFO event naming the step ASTERIA-EN23, got {events:?}"
            );
            assert!(
                at(&events, tracing::Level::WARN).is_empty(),
                "by exclusion is not a warning: {events:?}"
            );
        }
    }

    /// §3.5: the examples of the source, re-read.
    mod source_examples {
        use super::*;

        /// Page 1 of the source's example, in the verbose form.
        fn source_example(formats: Vec<String>) -> DocumentClassifier {
            DocumentClassifier::new(
                formats,
                vec![],
                vec![entry(
                    1,
                    vec![
                        leaf("EURIZON-EN23", &["^Eurizon Fund$", "^ciao$"]),
                        leaf("FIDEURAM-IT21", &["^Fideuram$"]),
                        leaf("EURIZON-EN23", &["^Eurizon Fund 2$"]),
                        leaf("MEDIOLANUM", &["^Mediolanum$"]),
                        leaf("AMUNDI-EN24", &["^Amundi Funds$"]),
                        block(
                            "MEDIOLANUM",
                            &[],
                            vec![
                                leaf("MEDIOLANUM-EN", &["^Annual Report$"]),
                                leaf("MEDIOLANUM-IT23", &["^Relazione 2023$"]),
                                leaf("MEDIOLANUM-ES", &["^Informe$"]),
                            ],
                        ),
                        template("*-EN23", &["^2023$"]),
                        block(
                            "FIDEURAM-IT23@*",
                            &[],
                            vec![leaf("FIDEURAM-IT23@LUX", &["^Luxembourg$"])],
                        ),
                    ],
                )],
            )
        }

        #[test]
        fn the_first_complete_step_whose_criteria_match_ends_it() {
            let document = FakeDocument::new(&[&["Eurizon Fund", "ciao"]]);
            let outcome = source_example(formats())
                .classify_serialized(&document)
                .unwrap();
            assert_complete(&outcome, "EURIZON-EN23");
            assert_eq!(yaml_step(&outcome), ("EURIZON-EN23".to_string(), vec![0]));
        }

        #[test]
        fn the_repeated_key_is_an_or_tried_after_the_steps_between() {
            let document = FakeDocument::new(&[&["Eurizon Fund 2"]]);
            let outcome = source_example(formats())
                .classify_serialized(&document)
                .unwrap();
            assert_complete(&outcome, "EURIZON-EN23");
            assert_eq!(yaml_step(&outcome), ("EURIZON-EN23".to_string(), vec![2]));
        }

        #[test]
        fn an_earlier_step_wins_over_the_or_written_after_it() {
            let document = FakeDocument::new(&[&["Fideuram", "Eurizon Fund 2"]]);
            assert_complete(
                &source_example(formats())
                    .classify_serialized(&document)
                    .unwrap(),
                "FIDEURAM-IT21",
            );
        }

        #[test]
        fn mediolanum_then_en_then_the_template_completes_the_name() {
            // `AMUNDI` and `IT23` would match the page and are not evaluated.
            let document = FakeDocument::new(&[&[
                "Mediolanum",
                "Amundi Funds",
                "Annual Report",
                "Relazione 2023",
                "2023",
                "Luxembourg",
            ]]);
            let outcome = source_example(formats())
                .classify_serialized(&document)
                .unwrap();
            assert_complete(&outcome, "MEDIOLANUM-EN23");
            assert_eq!(yaml_step(&outcome), ("*-EN23".to_string(), vec![6]));
        }

        #[test]
        fn without_the_template_matching_the_outcome_is_the_partial_with_its_candidates() {
            let document = FakeDocument::new(&[&["Mediolanum", "Annual Report", "Relazione 2023"]]);
            assert_eq!(
                source_example(formats())
                    .classify_serialized(&document)
                    .unwrap(),
                DocClassification::Partial {
                    pattern: p("MEDIOLANUM-EN"),
                    candidates: vec!["MEDIOLANUM-EN23".to_string()]
                }
            );
        }

        #[test]
        fn the_template_is_skipped_when_the_repository_lacks_its_instance() {
            let document = FakeDocument::new(&[&["Mediolanum", "Annual Report", "2023"]]);
            let outcome = source_example(formats_without("MEDIOLANUM-EN23"))
                .classify_serialized(&document)
                .unwrap();
            assert_eq!(
                outcome,
                DocClassification::Partial {
                    pattern: p("MEDIOLANUM-EN"),
                    candidates: vec![]
                }
            );
        }

        #[test]
        fn a_sibling_of_the_block_is_reached_when_the_first_child_fails() {
            let document = FakeDocument::new(&[&["Mediolanum", "Relazione 2023"]]);
            assert_complete(
                &source_example(formats())
                    .classify_serialized(&document)
                    .unwrap(),
                "MEDIOLANUM-IT23",
            );
        }

        #[test]
        fn without_the_first_partial_the_mediolanum_block_is_never_evaluated() {
            let document = FakeDocument::new(&[&["Annual Report", "Relazione 2023", "Informe"]]);
            assert_eq!(
                source_example(formats())
                    .classify_serialized(&document)
                    .unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn amundi_is_not_evaluated_once_mediolanum_is_found() {
            let document = FakeDocument::new(&[&["Mediolanum", "Amundi Funds"]]);
            assert_partial(
                &source_example(formats())
                    .classify_serialized(&document)
                    .unwrap(),
                "MEDIOLANUM",
            );
        }

        #[test]
        fn amundi_is_evaluated_when_nothing_was_accumulated() {
            let document = FakeDocument::new(&[&["Amundi Funds"]]);
            assert_complete(
                &source_example(formats())
                    .classify_serialized(&document)
                    .unwrap(),
                "AMUNDI-EN24",
            );
        }

        #[test]
        fn the_fideuram_block_without_criteria_is_never_evaluated_from_nothing() {
            let document = FakeDocument::new(&[&["Luxembourg"]]);
            assert_eq!(
                source_example(formats())
                    .classify_serialized(&document)
                    .unwrap(),
                DocClassification::Unclassified
            );
        }

        #[test]
        fn the_fideuram_block_is_never_evaluated_under_another_entity() {
            let document = FakeDocument::new(&[&["Mediolanum", "Luxembourg"]]);
            assert_partial(
                &source_example(formats())
                    .classify_serialized(&document)
                    .unwrap(),
                "MEDIOLANUM",
            );
        }
    }

    /// The Python entries (§6).
    mod python {
        use super::*;

        mod outcome_of_a_pipeline {
            use super::*;

            #[test]
            fn a_pipeline_that_only_gives_none_leaves_the_document_unclassified() {
                let document = FakeDocument::blank(2);
                let classifier = python_only(vec![
                    py(1, pipeline_emitting("p", vec![doc_class(None)])),
                    py(
                        2,
                        pipeline_emitting("q", vec![doc_class(None), doc_class(None)]),
                    ),
                ]);
                assert_eq!(
                    classifier.classify_serialized(&document).unwrap(),
                    DocClassification::Unclassified
                );
            }

            #[test]
            fn a_pipeline_that_gives_no_result_at_all_is_none() {
                let document = FakeDocument::blank(1);
                let classifier = python_only(vec![py(1, pipeline_emitting("p", vec![]))]);
                assert_eq!(
                    classifier.classify_serialized(&document).unwrap(),
                    DocClassification::Unclassified
                );
            }

            #[test]
            fn nones_are_dropped_and_equal_values_deduplicated() {
                let document = FakeDocument::blank(1);
                let classifier = python_only(vec![py(
                    1,
                    pipeline_emitting(
                        "p",
                        vec![
                            doc_class(None),
                            doc_class(Some("AMUNDI-EN24")),
                            doc_class(None),
                            doc_class(Some("AMUNDI-EN24")),
                        ],
                    ),
                )]);
                assert_complete(
                    &classifier.classify_serialized(&document).unwrap(),
                    "AMUNDI-EN24",
                );
            }

            #[test]
            fn two_different_values_from_one_pipeline_are_an_error() {
                let document = FakeDocument::blank(1);
                let classifier = python_only(vec![py(
                    1,
                    pipeline_emitting(
                        "p",
                        vec![
                            doc_class(Some("MEDIOLANUM")),
                            doc_class(Some("MEDIOLANUM-IT24.A")),
                        ],
                    ),
                )]);
                match classifier.classify_serialized(&document) {
                    Err(DocClassifyError::ConflictingDocClass {
                        entry,
                        pipeline,
                        page,
                        first,
                        second,
                    }) => {
                        assert_eq!(entry, 0);
                        assert_eq!(pipeline, PipelineName::new("p"));
                        assert_eq!(page, n(1));
                        assert_eq!((first, second), (p("MEDIOLANUM"), p("MEDIOLANUM-IT24.A")));
                    }
                    other => panic!("expected ConflictingDocClass, got {other:?}"),
                }
            }

            #[test]
            fn two_different_values_from_the_standard_pipes_are_the_phase_5_error() {
                let document = FakeDocument::new(&[&["Mediolanum", "Amundi"]]);
                let classifier = python_only(vec![py(
                    1,
                    standard_pipeline_with(
                        "p",
                        &[(&["^Mediolanum$"], "MEDIOLANUM"), (&["^Amundi$"], "AMUNDI")],
                    ),
                )]);
                match classifier.classify_serialized(&document) {
                    Err(DocClassifyError::Pipe {
                        entry,
                        pipeline,
                        page,
                        source,
                    }) => {
                        assert_eq!((entry, page), (0, n(1)));
                        assert_eq!(pipeline, PipelineName::new("p"));
                        assert_eq!(source.pipe(), "TextFilterDocClassifyStandard");
                    }
                    other => {
                        panic!("expected the pipe error of the standard filter, got {other:?}")
                    }
                }
            }

            #[test]
            fn a_failing_pipe_is_an_error_naming_entry_pipeline_and_page() {
                let document = FakeDocument::blank(3);
                let mut pipeline = Pipeline::new("broken");
                pipeline.pdf_extract.push(
                    crate::core::pipeline::segment::test_pipes::FailingExtract::fatal("boom"),
                );
                pipeline
                    .text_filter
                    .push(RecordingFilter::new("pass") as Arc<dyn TextFilterPipe>);
                pipeline.deserialize.push(Arc::new(Emits(vec![])));
                let classifier = python_only(vec![
                    py(1, pipeline_emitting("ok", vec![doc_class(None)])),
                    py(-1, pipeline),
                ]);
                match classifier.classify_serialized(&document) {
                    Err(DocClassifyError::Pipe {
                        entry,
                        pipeline,
                        page,
                        ..
                    }) => {
                        assert_eq!((entry, page), (1, n(-1)));
                        assert_eq!(pipeline, PipelineName::new("broken"));
                    }
                    other => panic!("expected a pipe error, got {other:?}"),
                }
            }

            #[test]
            fn a_result_that_is_not_a_document_class_is_an_error() {
                let document = FakeDocument::blank(1);
                let classifier = python_only(vec![py(
                    1,
                    pipeline_emitting("p", vec![Extracted::PageClass(None)]),
                )]);
                assert!(
                    matches!(
                        classifier.classify_serialized(&document),
                        Err(DocClassifyError::NotADocClassification { entry: 0, ref pipeline, .. }) if *pipeline == PipelineName::new("p")
                    ),
                    "expected NotADocClassification"
                );
            }

            #[test]
            fn the_standard_pipes_classify_as_the_yaml_would() {
                let document = FakeDocument::new(&[&["Eurizon Fund"]]);
                let classifier = python_only(vec![py(
                    1,
                    standard_pipeline("eurizon", &["^Eurizon Fund$"], "EURIZON-EN23"),
                )]);
                assert_eq!(
                    classifier.classify_serialized(&document).unwrap(),
                    DocClassification::Complete(CompleteClassification {
                        format: p("EURIZON-EN23"),
                        page: n(1),
                        entry: 0,
                        decided_by: DecidedBy::Python {
                            pipeline: PipelineName::new("eurizon")
                        },
                        by_exclusion: false,
                    })
                );
            }

            #[test]
            fn the_standard_pipes_give_none_when_a_criterion_fails() {
                let document = FakeDocument::new(&[&["Amundi"]]);
                let classifier = python_only(vec![py(
                    1,
                    standard_pipeline("eurizon", &["^Eurizon Fund$"], "EURIZON-EN23"),
                )]);
                assert_eq!(
                    classifier.classify_serialized(&document).unwrap(),
                    DocClassification::Unclassified
                );
            }
        }

        mod runtime_validation {
            use super::*;

            #[test]
            fn a_complete_name_the_repository_does_not_declare_is_an_error() {
                let document = FakeDocument::blank(2);
                let classifier = python_only(vec![py(2, says("p", "MEDIOLANUM-IT24"))]);
                match classifier.classify_serialized(&document) {
                    Err(DocClassifyError::UnknownFormat {
                        entry,
                        pipeline,
                        page,
                        format,
                    }) => {
                        assert_eq!((entry, page), (0, n(2)));
                        assert_eq!(pipeline, PipelineName::new("p"));
                        assert_eq!(format, p("MEDIOLANUM-IT24"));
                    }
                    other => panic!("expected UnknownFormat, got {other:?}"),
                }
            }

            #[test]
            fn a_partial_no_format_of_the_repository_refines_is_an_error() {
                let document = FakeDocument::blank(1);
                let classifier = python_only(vec![py(1, says("p", "MEDIOLANUM-DE"))]);
                match classifier.classify_serialized(&document) {
                    Err(DocClassifyError::PartialMatchesNoFormat {
                        entry,
                        pipeline,
                        page,
                        pattern,
                    }) => {
                        assert_eq!((entry, page), (0, n(1)));
                        assert_eq!(pipeline, PipelineName::new("p"));
                        assert_eq!(pattern, p("MEDIOLANUM-DE"));
                    }
                    other => panic!("expected PartialMatchesNoFormat, got {other:?}"),
                }
            }

            #[test]
            fn an_unknown_entity_is_an_error_too() {
                let document = FakeDocument::blank(1);
                let classifier = python_only(vec![py(1, says("p", "NOBODY"))]);
                assert!(matches!(
                    classifier.classify_serialized(&document),
                    Err(DocClassifyError::PartialMatchesNoFormat { .. })
                ));
            }

            #[test]
            fn the_error_stops_the_classification_before_later_entries() {
                let document = FakeDocument::blank(3);
                let classifier = python_only(vec![
                    py(1, says("p", "NOBODY-EN23")),
                    py(2, says("q", "AMUNDI-EN24")),
                ]);
                assert!(matches!(
                    classifier.classify_serialized(&document),
                    Err(DocClassifyError::UnknownFormat { .. })
                ));
                assert_eq!(document.requests(), vec![1]);
            }
        }

        mod state_machine {
            use super::*;

            #[test]
            fn a_complete_outcome_with_nothing_accumulated_ends_the_classification() {
                let document = FakeDocument::blank(2);
                let classifier = python_only(vec![
                    py(1, pipeline_emitting("none", vec![doc_class(None)])),
                    py(2, says("p", "AMUNDI-EN24")),
                ]);
                assert_eq!(
                    classifier.classify_serialized(&document).unwrap(),
                    DocClassification::Complete(CompleteClassification {
                        format: p("AMUNDI-EN24"),
                        page: n(2),
                        entry: 1,
                        decided_by: DecidedBy::Python {
                            pipeline: PipelineName::new("p")
                        },
                        by_exclusion: false,
                    })
                );
            }

            #[test]
            fn partials_accumulate_across_entries() {
                let document = FakeDocument::blank(3);
                let classifier = python_only(vec![
                    py(1, says("p", "MEDIOLANUM")),
                    py(2, says("q", "MEDIOLANUM-IT24.*")),
                    py(3, says("r", "MEDIOLANUM-IT24.A")),
                ]);
                let outcome = classifier.classify_serialized(&document).unwrap();
                assert_complete(&outcome, "MEDIOLANUM-IT24.A");
                assert_eq!(complete(&outcome).entry, 2);
            }

            #[test]
            fn an_outcome_the_accumulated_partial_already_refines_is_ignored() {
                let document = FakeDocument::blank(3);
                let classifier = python_only(vec![
                    py(1, says("p", "MEDIOLANUM-IT24.*")),
                    py(2, says("q", "MEDIOLANUM")),
                    py(3, says("r", "MEDIOLANUM-IT24.*")),
                ]);
                assert_partial(
                    &classifier.classify_serialized(&document).unwrap(),
                    "MEDIOLANUM-IT24.*",
                );
            }

            #[test]
            fn an_outcome_the_accumulated_partial_already_refines_is_logged_at_debug_only() {
                let document = FakeDocument::blank(2);
                let classifier = python_only(vec![
                    py(1, says("p", "MEDIOLANUM-IT24.*")),
                    py(2, says("ignored-one", "MEDIOLANUM")),
                ]);
                let (outcome, events) = events_of(|| classifier.classify(&document).unwrap());
                assert_partial(&outcome, "MEDIOLANUM-IT24.*");
                assert!(
                    at(&events, tracing::Level::WARN).is_empty(),
                    "no warning expected: {events:?}"
                );
                assert!(
                    at(&events, tracing::Level::DEBUG)
                        .iter()
                        .any(|event| event.text.contains("MEDIOLANUM")),
                    "expected a DEBUG event naming the ignored outcome, got {events:?}"
                );
            }

            #[test]
            fn an_incompatible_partial_outcome_is_ignored() {
                let document = FakeDocument::blank(3);
                let classifier = python_only(vec![
                    py(1, says("p", "MEDIOLANUM")),
                    py(2, says("q", "AMUNDI")),
                    py(3, says("r", "MEDIOLANUM-EN")),
                ]);
                assert_partial(
                    &classifier.classify_serialized(&document).unwrap(),
                    "MEDIOLANUM-EN",
                );
            }

            #[test]
            fn an_incompatible_complete_outcome_is_ignored_and_the_next_entries_run() {
                let document = FakeDocument::blank(3);
                let classifier = python_only(vec![
                    py(1, says("p", "MEDIOLANUM")),
                    py(2, says("q", "EURIZON-EN23")),
                    py(3, says("r", "MEDIOLANUM-EN23")),
                ]);
                let outcome = classifier.classify_serialized(&document).unwrap();
                assert_complete(&outcome, "MEDIOLANUM-EN23");
                assert_eq!(document.requests(), vec![1, 2, 3]);
            }

            #[test]
            fn an_incompatible_outcome_is_a_warning_naming_entry_page_outcome_and_partial() {
                // D4.
                let document = FakeDocument::blank(7);
                let classifier = python_only(vec![
                    py(1, says("first", "MEDIOLANUM")),
                    py(7, says("contradicts", "EURIZON-EN23")),
                ]);
                let (outcome, events) = events_of(|| classifier.classify(&document).unwrap());
                assert_partial(&outcome, "MEDIOLANUM");
                let warnings = at(&events, tracing::Level::WARN);
                assert_eq!(
                    warnings.len(),
                    1,
                    "exactly one warning expected: {events:?}"
                );
                let warning = &warnings[0].text;
                for expected in ["contradicts", "7", "EURIZON-EN23", "MEDIOLANUM"] {
                    assert!(
                        warning.contains(expected),
                        "the warning must mention '{expected}': {warning}"
                    );
                }
            }

            /// A repository where `X-IT23@LUX.*` and `X-IT23@*.B` both expand to something, while
            /// their meet `X-IT23@LUX.B` is not a format.
            fn incomparable_repo(python: Vec<PythonEntry>) -> DocumentClassifier {
                DocumentClassifier::new(
                    vec!["X-IT23@LUX.A".to_string(), "X-IT23@ROM.B".to_string()],
                    python,
                    vec![],
                )
            }

            #[test]
            fn a_compatible_but_incomparable_outcome_is_ignored() {
                let document = FakeDocument::blank(2);
                let classifier = incomparable_repo(vec![
                    py(1, says("p", "X-IT23@LUX.*")),
                    py(2, says("q", "X-IT23@*.B")),
                ]);
                assert_eq!(
                    classifier.classify_serialized(&document).unwrap(),
                    DocClassification::Partial {
                        pattern: p("X-IT23@LUX.*"),
                        candidates: vec!["X-IT23@LUX.A".to_string()]
                    }
                );
            }

            #[test]
            fn a_compatible_but_incomparable_outcome_is_a_warning_naming_entry_page_outcome_and_partial()
             {
                let document = FakeDocument::blank(7);
                let classifier = incomparable_repo(vec![
                    py(1, says("first", "X-IT23@LUX.*")),
                    py(7, says("sideways", "X-IT23@*.B")),
                ]);
                let (outcome, events) = events_of(|| classifier.classify(&document).unwrap());
                assert_partial(&outcome, "X-IT23@LUX.*");
                let warnings = at(&events, tracing::Level::WARN);
                assert_eq!(
                    warnings.len(),
                    1,
                    "exactly one warning expected: {events:?}"
                );
                let warning = &warnings[0].text;
                for expected in ["sideways", "7", "X-IT23@*.B", "X-IT23@LUX.*"] {
                    assert!(
                        warning.contains(expected),
                        "the warning must mention '{expected}': {warning}"
                    );
                }
            }

            #[test]
            fn an_outcome_on_a_page_that_adds_nothing_does_not_end_the_python_entries() {
                let document = FakeDocument::blank(2);
                let classifier = python_only(vec![
                    py(1, says("p", "MEDIOLANUM")),
                    py(2, says("q", "MEDIOLANUM")),
                ]);
                assert_partial(
                    &classifier.classify_serialized(&document).unwrap(),
                    "MEDIOLANUM",
                );
                assert_eq!(document.requests(), vec![1, 2]);
            }
        }
    }

    /// A page that cannot be converted is an error of the classification, which stops there.
    mod page_conversion_errors {
        use super::*;

        fn assert_document_error(
            result: Result<DocClassification, DocClassifyError>,
            written: i32,
        ) {
            match result {
                Err(DocClassifyError::Document {
                    page,
                    source: DocumentError::Page { .. },
                }) => assert_eq!(page, n(written)),
                other => panic!("expected a Document error on page {written}, got {other:?}"),
            }
        }

        #[test]
        fn a_yaml_entry_on_a_broken_page_is_an_error_naming_the_page() {
            let document = FakeDocument::blank(3).breaking(2);
            let classifier = yaml_only(vec![entry(2, vec![unconditional("ASTERIA-EN23")])]);
            assert_document_error(classifier.classify_serialized(&document), 2);
        }

        #[test]
        fn a_python_entry_on_a_broken_page_is_an_error_naming_the_page() {
            let document = FakeDocument::blank(3).breaking(3);
            let classifier = python_only(vec![py(-1, says("p", "AMUNDI-EN24"))]);
            assert_document_error(classifier.classify_serialized(&document), -1);
        }

        #[test]
        fn the_error_stops_the_classification_even_if_a_later_entry_would_classify() {
            let document = FakeDocument::new(&[&["Eurizon Fund"], &["Eurizon Fund"]]).breaking(1);
            let classifier = yaml_only(vec![
                entry(1, vec![leaf("EURIZON-EN23", &["^Eurizon Fund$"])]),
                entry(2, vec![leaf("EURIZON-EN23", &["^Eurizon Fund$"])]),
            ]);
            assert_document_error(classifier.classify_serialized(&document), 1);
            assert_eq!(document.requests(), vec![1]);
        }

        #[test]
        fn the_error_stops_the_classification_even_with_a_partial_accumulated() {
            let document =
                FakeDocument::new(&[&["Mediolanum"], &["x"], &["Best Brands"]]).breaking(2);
            let classifier = yaml_only(vec![
                entry(1, vec![leaf("MEDIOLANUM", &["^Mediolanum$"])]),
                entry(2, vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])]),
                entry(3, vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])]),
            ]);
            assert_document_error(classifier.classify_serialized(&document), 2);
            assert_eq!(document.requests(), vec![1, 2]);
        }

        #[test]
        fn a_broken_python_page_stops_the_yaml_from_running() {
            let document = FakeDocument::new(&[&["Eurizon Fund"], &["x"]]).breaking(2);
            let classifier = DocumentClassifier::new(
                formats(),
                vec![py(2, says("p", "MEDIOLANUM"))],
                vec![entry(1, vec![leaf("EURIZON-EN23", &["^Eurizon Fund$"])])],
            );
            assert_document_error(classifier.classify_serialized(&document), 2);
            assert_eq!(document.requests(), vec![2]);
        }

        #[test]
        fn a_page_past_the_end_is_still_nothing_not_an_error() {
            let document = FakeDocument::blank(2).breaking(2);
            let classifier = yaml_only(vec![entry(5, vec![unconditional("ASTERIA-EN23")])]);
            assert_eq!(
                classifier.classify_serialized(&document).unwrap(),
                DocClassification::Unclassified
            );
        }
    }

    /// Python first, then YAML (§6, precedence [U]).
    mod precedence {
        use super::*;

        #[test]
        fn a_complete_python_outcome_means_the_yaml_never_runs() {
            let document = FakeDocument::new(&[&["Amundi Funds"], &["Eurizon Fund"]]);
            let classifier = DocumentClassifier::new(
                formats(),
                vec![py(1, says("p", "AMUNDI-EN24"))],
                vec![
                    entry(1, vec![leaf("EURIZON-EN23", &[".*"])]),
                    entry(2, vec![leaf("EURIZON-EN23", &[".*"])]),
                ],
            );
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "AMUNDI-EN24");
            assert!(matches!(
                complete(&outcome).decided_by,
                DecidedBy::Python { .. }
            ));
            assert_eq!(document.requests(), vec![1]);
        }

        #[test]
        fn a_partial_python_outcome_is_where_the_yaml_starts_from() {
            let document = FakeDocument::new(&[&["Amundi Funds", "Annual Report 2023"]]);
            let classifier = DocumentClassifier::new(
                formats(),
                vec![py(1, says("p", "MEDIOLANUM"))],
                vec![entry(
                    1,
                    vec![
                        leaf("AMUNDI-EN24", &["^Amundi Funds$"]),
                        template("*-EN23", &["^Annual Report 2023$"]),
                    ],
                )],
            );
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "MEDIOLANUM-EN23");
            assert_eq!(yaml_step(&outcome), ("*-EN23".to_string(), vec![1]));
        }

        #[test]
        fn a_partial_python_outcome_lets_a_yaml_block_refine_it_without_rechecking_the_root() {
            // D1 with the Python: the YAML root's criteria are not on the page.
            let document = FakeDocument::new(&[&["Best Brands"]]);
            let classifier = DocumentClassifier::new(
                formats(),
                vec![py(1, says("p", "MEDIOLANUM"))],
                vec![entry(
                    1,
                    vec![block(
                        "MEDIOLANUM",
                        &["^Mediolanum$"],
                        vec![block(
                            "MEDIOLANUM-IT24.*",
                            &[],
                            vec![leaf("MEDIOLANUM-IT24.A", &["^Best Brands$"])],
                        )],
                    )],
                )],
            );
            assert_complete(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM-IT24.A",
            );
        }

        #[test]
        fn a_partial_python_outcome_survives_a_yaml_that_adds_nothing() {
            let document = FakeDocument::new(&[&["Eurizon Fund"]]);
            let classifier = DocumentClassifier::new(
                formats(),
                vec![py(1, says("p", "MEDIOLANUM"))],
                vec![entry(1, vec![leaf("EURIZON-EN23", &["^Eurizon Fund$"])])],
            );
            assert_partial(
                &classifier.classify_serialized(&document).unwrap(),
                "MEDIOLANUM",
            );
        }

        #[test]
        fn no_python_outcome_means_the_yaml_starts_from_nothing() {
            let document = FakeDocument::new(&[&["Eurizon Fund"]]);
            let classifier = DocumentClassifier::new(
                formats(),
                vec![py(1, pipeline_emitting("p", vec![doc_class(None)]))],
                vec![entry(1, vec![leaf("EURIZON-EN23", &["^Eurizon Fund$"])])],
            );
            let outcome = classifier.classify_serialized(&document).unwrap();
            assert_complete(&outcome, "EURIZON-EN23");
            assert_eq!(complete(&outcome).entry, 0);
            assert!(matches!(
                complete(&outcome).decided_by,
                DecidedBy::Yaml { .. }
            ));
        }

        #[test]
        fn every_python_entry_runs_before_the_first_yaml_entry() {
            let document = FakeDocument::blank(5);
            let classifier = DocumentClassifier::new(
                formats(),
                vec![
                    py(4, pipeline_emitting("p", vec![doc_class(None)])),
                    py(5, pipeline_emitting("q", vec![doc_class(None)])),
                ],
                vec![
                    entry(1, vec![leaf("EURIZON-EN23", &["^x$"])]),
                    entry(2, vec![leaf("EURIZON-EN23", &["^x$"])]),
                ],
            );
            classifier.classify_serialized(&document).unwrap();
            assert_eq!(document.requests(), vec![4, 5, 1, 2]);
        }
    }

    /// The ordinary case is quiet (plan phase 6: "il non-match è il caso normale e non si logga").
    mod quiet_on_no_match {
        use super::*;

        #[test]
        fn a_document_nothing_recognises_raises_no_warning() {
            let document = FakeDocument::new(&[&["nothing here"], &["nor here"]]);
            let classifier = yaml_only(vec![
                entry(
                    1,
                    vec![
                        leaf("EURIZON-EN23", &["^Eurizon Fund$"]),
                        block(
                            "MEDIOLANUM",
                            &["^Mediolanum$"],
                            vec![leaf("MEDIOLANUM-EN23", &["^x$"])],
                        ),
                    ],
                ),
                entry(2, vec![leaf("AMUNDI-EN24", &["^Amundi Funds$"])]),
            ]);
            let (outcome, events) = events_of(|| classifier.classify(&document).unwrap());
            assert_eq!(outcome, DocClassification::Unclassified);
            assert!(at(&events, tracing::Level::WARN).is_empty(), "{events:?}");
            assert!(at(&events, tracing::Level::ERROR).is_empty(), "{events:?}");
        }
    }
}
