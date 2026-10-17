//! Combining what the sources say of a document's format: the algorithm (the document
//! classification) and the URL (`url_mapping.csv`), for one document and for a set of documents.
//!
//! Normative text: §1.5 (the relations) and §7 (the combination of the sources) of
//! `agent-memory/SPEC-document-classification.md`; the design is
//! `agent-memory/DESIGN-phase10-classify.md` §3. The `classify` subcommand and the run both use it;
//! the run adds the configuration on top of [`SourcesFormat`] with [`resolve_job_format`].
//!
//! One rule for every pair of patterns — source against source, document against document: two
//! patterns **agree** when they are compatible in the repository
//! ([`FormatPattern::meet_in_repo`]), and the combined result is their meet.

// `DocumentSourcesError::Classify` carries a `DocClassifyError` by value, as `core::doc_classify`
// does and for its reason: callers match on it, and it is built at most once per document.
#![allow(clippy::result_large_err, clippy::large_enum_variant)]

use std::path::Path;

use crate::core::doc_classify::{DocClassification, DocClassifyError, DocumentClassifier};
use crate::core::tracing_setup::log_error;
use crate::formats_repo::format_pattern::FormatPattern;
use crate::formats_repo::metadata::url_to_format;
use crate::input::document::DocumentError;
use crate::input::document::lazy::LazyDocument;

/// One of the two sources of a document's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Algorithm,
    Url,
}

impl std::fmt::Display for Source {
    /// `algorithm` or `url`, as the error messages say it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Source::Algorithm => "algorithm",
            Source::Url => "url",
        })
    }
}

/// What a document says of its format, source by source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSources {
    pub algorithm: DocClassification,
    /// `None` when the document spec has no URL.
    pub url: Option<UrlSource>,
}

impl DocumentSources {
    /// The algorithm's pattern: the complete format, the partial, or nothing.
    pub fn algorithm_pattern(&self) -> Option<&FormatPattern> {
        match &self.algorithm {
            DocClassification::Complete(complete) => Some(&complete.format),
            DocClassification::Partial { pattern, .. } => Some(pattern),
            DocClassification::Unclassified => None,
        }
    }

    /// The URL's pattern: nothing without a URL, or with one `url_mapping.csv` does not map.
    pub fn url_pattern(&self) -> Option<&FormatPattern> {
        self.url.as_ref().and_then(|url| url.pattern.as_ref())
    }

    /// [`resolve_sources`] on this document alone.
    pub fn resolve(&self, formats: &[String]) -> SourcesFormat {
        resolve_sources(self.algorithm_pattern(), self.url_pattern(), formats)
    }
}

/// A document's URL, and the format `url_mapping.csv` gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlSource {
    pub url: String,
    /// `None` if no row of `url_mapping.csv` matches, or if the table cannot be read (a `WARN`).
    pub pattern: Option<FormatPattern>,
}

/// Why the sources of a document could not be read.
#[derive(Debug, thiserror::Error)]
pub enum DocumentSourcesError {
    /// The document would not open (the run: the document is skipped, as today).
    #[error(transparent)]
    Open(DocumentError),
    /// The classification failed at run time (D5).
    #[error(transparent)]
    Classify(DocClassifyError),
}

/// Opens `pdf` lazily (`LazyDocument::open(pdf, true)`, as `load_document` in the job), classifies
/// it, and resolves `url` with `url_to_format`. The pages read here are converted again by whoever
/// loads the document afterwards: wanted (decision of 2026-10-10).
pub fn document_sources(
    classifier: &DocumentClassifier,
    formats_repo_dir: &Path,
    pdf: &Path,
    url: Option<&str>,
) -> Result<DocumentSources, DocumentSourcesError> {
    let document = LazyDocument::open(pdf, true).map_err(DocumentSourcesError::Open)?;
    let algorithm = classifier
        .classify(&document)
        .map_err(DocumentSourcesError::Classify)?;
    let url = url.map(|url| url_source(formats_repo_dir, classifier.formats(), url));
    Ok(DocumentSources { algorithm, url })
}

/// An unreadable `url_mapping.csv` costs the URL source, not the document.
fn url_pattern(formats_repo_dir: &Path, formats: &[String], url: &str) -> Option<FormatPattern> {
    url_to_format(formats_repo_dir, formats, url).unwrap_or_else(|e| {
        tracing::warn!(
            error = log_error(&e),
            url,
            "cannot detect format from this report url: {e}"
        );
        None
    })
}

/// How the two sources relate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agreement {
    Neither,
    AlgorithmOnly,
    UrlOnly,
    Agree,
    Disagree,
}

/// The two sources side by side, and what follows from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcesFormat {
    pub algorithm: Option<FormatPattern>,
    pub url: Option<FormatPattern>,
    pub agreement: Agreement,
    /// `Neither` → `None`; `*Only` → that source; `Agree` → the meet; `Disagree` → the algorithm.
    pub resolved: Option<FormatPattern>,
}

/// The outcome of a [`SourcesFormat`] against the formats of the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatOutcome {
    Complete(FormatPattern),
    /// The partial, and the formats refining it in `formats.csv` order.
    Partial {
        pattern: FormatPattern,
        candidates: Vec<String>,
    },
    None,
}

impl SourcesFormat {
    /// Complete / partial with its expansion / nothing — for the printout and for the error of
    /// phase 11.
    pub fn outcome(&self, formats: &[String]) -> FormatOutcome {
        match &self.resolved {
            None => FormatOutcome::None,
            Some(pattern) if pattern.is_complete() => FormatOutcome::Complete(pattern.clone()),
            Some(pattern) => FormatOutcome::Partial {
                pattern: pattern.clone(),
                candidates: pattern.expand(formats),
            },
        }
    }
}

/// Pure, and **does not log**: the per-document printout uses it too.
pub fn resolve_sources(
    algorithm: Option<&FormatPattern>,
    url: Option<&FormatPattern>,
    formats: &[String],
) -> SourcesFormat {
    let (agreement, resolved) = match (algorithm, url) {
        (None, None) => (Agreement::Neither, None),
        (Some(algorithm), None) => (Agreement::AlgorithmOnly, Some(algorithm.clone())),
        (None, Some(url)) => (Agreement::UrlOnly, Some(url.clone())),
        (Some(algorithm), Some(url)) => match algorithm.meet_in_repo(url, formats) {
            Some(meet) => (Agreement::Agree, Some(meet)),
            None => (Agreement::Disagree, Some(algorithm.clone())),
        },
    };
    SourcesFormat {
        algorithm: algorithm.cloned(),
        url: url.cloned(),
        agreement,
        resolved,
    }
}

/// The format of a set of documents (a job; the non-batch input of the subcommand).
///
/// First **per source**, folding the documents' patterns with [`FormatPattern::meet_in_repo`] (a
/// document without a pattern for that source is neutral; an empty meet is
/// [`ConflictingFormats`]); then [`resolve_sources`] between the two folded patterns (user decision
/// Q1, 2026-10-11). Logs **one** `ERROR` when the two sources disagree, naming both patterns.
///
/// Each document comes with its **label** — the spec's name, otherwise its path, otherwise its URL,
/// as `freeports classify` prints it — which is how [`ConflictingFormats`] names the documents
/// (user decision of 2026-10-17: labels, not indices).
pub fn combine_documents(
    documents: &[(String, DocumentSources)],
    formats: &[String],
) -> Result<SourcesFormat, ConflictingFormats> {
    let algorithm = fold_source(
        Source::Algorithm,
        documents
            .iter()
            .map(|(label, sources)| (label, sources.algorithm_pattern())),
        formats,
    )?;
    let url = fold_source(
        Source::Url,
        documents
            .iter()
            .map(|(label, sources)| (label, sources.url_pattern())),
        formats,
    )?;
    let combined = resolve_sources(algorithm.as_ref(), url.as_ref(), formats);
    if let (Agreement::Disagree, Some(algorithm), Some(url)) =
        (combined.agreement, &combined.algorithm, &combined.url)
    {
        tracing::error!(
            algorithm = %algorithm,
            url = %url,
            "the algorithm and the url disagree on the format"
        );
    }
    Ok(combined)
}

/// The meet of every pattern `by` gives, the documents without one being neutral. The meet of all
/// is the intersection of their expansions, so the order of the documents does not matter.
fn fold_source<'a>(
    by: Source,
    patterns: impl Iterator<Item = (&'a String, Option<&'a FormatPattern>)>,
    formats: &[String],
) -> Result<Option<FormatPattern>, ConflictingFormats> {
    let found: Vec<(String, FormatPattern)> = patterns
        .filter_map(|(label, pattern)| pattern.map(|pattern| (label.clone(), pattern.clone())))
        .collect();
    let Some(((_, first), rest)) = found.split_first() else {
        return Ok(None);
    };
    rest.iter()
        .try_fold(first.clone(), |meet, (_, pattern)| {
            meet.meet_in_repo(pattern, formats)
        })
        .map(Some)
        .ok_or_else(|| ConflictingFormats {
            by,
            found: found.clone(),
        })
}

/// Documents of one input disagreeing with each other on one source.
///
/// The field is `by` rather than the design's `source`: `thiserror` takes a field named `source`
/// for the error's cause.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("documents disagree on the {by} format: {}", found_list(found))]
pub struct ConflictingFormats {
    pub by: Source,
    /// (label of the document, pattern) of **every** document that source classified, in the
    /// order given.
    pub found: Vec<(String, FormatPattern)>,
}

fn found_list(found: &[(String, FormatPattern)]) -> String {
    found
        .iter()
        .map(|(label, pattern)| format!("document {label} is {pattern}"))
        .collect::<Vec<_>>()
        .join(", ")
}

// ------------------------------------------------------------------------------------------------
// Phase 11: the run's priority, configuration > algorithm > url
// (`agent-memory/DESIGN-phase11-run-integration.md` §2.2, §4)
// ------------------------------------------------------------------------------------------------

/// Where the format a job runs came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatOrigin {
    Configuration,
    Algorithm,
    Url,
    AlgorithmAndUrl,
}

/// The format a job runs, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobFormat {
    /// Always a complete format name.
    pub format: String,
    pub origin: FormatOrigin,
}

/// Why no format could be settled for a job without a configured one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JobFormatError {
    #[error(
        "no format was configured, and neither the document classification nor the report url determines one"
    )]
    Undetermined,
    #[error(
        "the format is only partly determined, {pattern}: set it explicitly; the candidates are {}",
        candidates.join(", ")
    )]
    Partial {
        pattern: FormatPattern,
        candidates: Vec<String>,
    },
}

/// The run's priority, configuration > algorithm > url, on top of what [`combine_documents`]
/// found. Logs the configuration crossings (design §4) and the detected format; never the
/// algorithm–url crossing, which [`combine_documents`] already logged.
pub fn resolve_job_format(
    configured: Option<&str>,
    sources: &SourcesFormat,
    formats: &[String],
) -> Result<JobFormat, JobFormatError> {
    match configured {
        Some(configured) => {
            check_configured(configured, sources, formats);
            Ok(JobFormat {
                format: configured.to_string(),
                origin: FormatOrigin::Configuration,
            })
        }
        None => detected_format(sources, formats),
    }
}

impl FormatOrigin {
    /// How the `by` field of the run's events names it.
    fn by(self) -> &'static str {
        match self {
            FormatOrigin::Configuration => "configuration",
            FormatOrigin::Algorithm => "algorithm",
            FormatOrigin::Url => "url",
            FormatOrigin::AlgorithmAndUrl => "algorithm and url",
        }
    }
}

/// Crossings 2 and 3: one `WARN` per source the configured format does not meet in the
/// repository. A configured format that is not even a pattern meets nothing; its real error comes
/// later, from `Algorithm::load`.
fn check_configured(configured: &str, sources: &SourcesFormat, formats: &[String]) {
    let pattern = configured.parse::<FormatPattern>().ok();
    let agrees = |source: &FormatPattern| {
        pattern
            .as_ref()
            .is_some_and(|pattern| pattern.meet_in_repo(source, formats).is_some())
    };
    let mut contradicted = false;
    if let Some(algorithm) = sources.algorithm.as_ref().filter(|a| !agrees(a)) {
        contradicted = true;
        tracing::warn!(
            configured,
            algorithm = %algorithm,
            "the configured format differs from the one the document classification finds; the configured one is used"
        );
    }
    if let Some(url) = sources.url.as_ref().filter(|u| !agrees(u)) {
        contradicted = true;
        tracing::warn!(
            configured,
            url = %url,
            "the configured format differs from the one the report url maps to; the configured one is used"
        );
    }
    let answering = match (&sources.algorithm, &sources.url) {
        (None, None) => None,
        (Some(_), None) => Some(FormatOrigin::Algorithm),
        (None, Some(_)) => Some(FormatOrigin::Url),
        (Some(_), Some(_)) => Some(FormatOrigin::AlgorithmAndUrl),
    };
    if let Some(by) = answering
        && !contradicted
    {
        tracing::debug!(
            configured,
            by = by.by(),
            "configured format confirmed by the document"
        );
    }
}

/// Crossing 4: no configured format, so the documents' — complete, or an error of the job, which
/// the job logs once.
fn detected_format(
    sources: &SourcesFormat,
    formats: &[String],
) -> Result<JobFormat, JobFormatError> {
    match sources.outcome(formats) {
        FormatOutcome::Complete(format) => {
            let origin = match sources.agreement {
                Agreement::AlgorithmOnly | Agreement::Disagree => FormatOrigin::Algorithm,
                Agreement::UrlOnly => FormatOrigin::Url,
                Agreement::Agree => FormatOrigin::AlgorithmAndUrl,
                Agreement::Neither => unreachable!("a resolved format comes from a source"),
            };
            tracing::info!(format = %format, by = origin.by(), "format detected");
            Ok(JobFormat {
                format: format.to_string(),
                origin,
            })
        }
        FormatOutcome::Partial {
            pattern,
            candidates,
        } => Err(JobFormatError::Partial {
            pattern,
            candidates,
        }),
        FormatOutcome::None => Err(JobFormatError::Undetermined),
    }
}

/// What the url says, alone: [`document_sources`] uses it, and the job uses it for a document
/// whose classification failed with a configured format (D5: the classification counts as
/// nothing, the url still counts).
pub(crate) fn url_source(formats_repo_dir: &Path, formats: &[String], url: &str) -> UrlSource {
    UrlSource {
        url: url.to_string(),
        pattern: url_pattern(formats_repo_dir, formats, url),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroI32;

    use crate::core::doc_classify::{CompleteClassification, DecidedBy};
    use crate::core::tracing_setup::capture::{Record, events};

    // ------------------------------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------------------------------

    /// `agent-memory/DESIGN-phase10-classify.md` §7: the five formats of the
    /// `formats_repo::doc_classify` tests, plus `X-IT23@LUX.A` and `X-IT23@ES.B` for R10.
    const FORMATS: [&str; 7] = [
        "AMUNDI-EN24",
        "EURIZON-EN23",
        "MEDIOLANUM-EN23",
        "MEDIOLANUM-IT24.A",
        "MEDIOLANUM-IT24.B",
        "X-IT23@LUX.A",
        "X-IT23@ES.B",
    ];

    fn formats() -> Vec<String> {
        FORMATS.iter().map(|f| f.to_string()).collect()
    }

    fn p(text: &str) -> FormatPattern {
        text.parse().expect("fixture pattern must be valid")
    }

    fn resolve(algorithm: Option<&str>, url: Option<&str>) -> SourcesFormat {
        resolve_sources(algorithm.map(p).as_ref(), url.map(p).as_ref(), &formats())
    }

    /// A complete algorithm outcome, decided by a YAML step on page 1.
    fn complete(format: &str) -> DocClassification {
        DocClassification::Complete(CompleteClassification {
            format: p(format),
            page: NonZeroI32::new(1).unwrap(),
            entry: 0,
            decided_by: DecidedBy::Yaml {
                step: format.to_string(),
                path: vec![0],
            },
            by_exclusion: false,
        })
    }

    fn partial(pattern: &str) -> DocClassification {
        DocClassification::Partial {
            pattern: p(pattern),
            candidates: p(pattern).expand(&formats()),
        }
    }

    /// A document whose algorithm says `algorithm` and which has no URL.
    fn algorithm_only(algorithm: DocClassification) -> DocumentSources {
        DocumentSources {
            algorithm,
            url: None,
        }
    }

    /// A document whose algorithm says `algorithm` and whose URL maps to `url` (`None`: not mapped).
    fn with_url(algorithm: DocClassification, url: Option<&str>) -> DocumentSources {
        DocumentSources {
            algorithm,
            url: Some(UrlSource {
                url: "https://example.com/report.pdf".to_string(),
                pattern: url.map(p),
            }),
        }
    }

    /// A document only its URL classifies.
    fn url_only(url: &str) -> DocumentSources {
        with_url(DocClassification::Unclassified, Some(url))
    }

    /// The documents of a set, each labelled `doc<i>.pdf` by its position: the labels
    /// [`ConflictingFormats`] names them by.
    fn labelled(documents: &[DocumentSources]) -> Vec<(String, DocumentSources)> {
        documents
            .iter()
            .enumerate()
            .map(|(i, sources)| (format!("doc{i}.pdf"), sources.clone()))
            .collect()
    }

    /// Every event `f` emits, under the engine tests' `SERIAL` lock (see it in
    /// `core::doc_classify`'s tests: the process-wide `tracing` callsite race), with the interest
    /// cache rebuilt once the capturing dispatcher is in place.
    fn captured<T>(f: impl FnOnce() -> T) -> (T, Vec<Record>) {
        let _guard = crate::core::doc_classify::tests::SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut result = None;
        let records = events(|| {
            tracing::callsite::rebuild_interest_cache();
            result = Some(f());
        });
        (result.expect("the closure ran"), records)
    }

    fn at(records: &[Record], level: tracing::Level) -> Vec<&Record> {
        records.iter().filter(|r| r.level == Some(level)).collect()
    }

    /// The message and every field value of an event, for "does it name X" assertions.
    fn text_of(record: &Record) -> String {
        let mut text = record.message.clone();
        for value in record.fields.values() {
            text.push(' ');
            text.push_str(value);
        }
        text
    }

    // ==========================================================================================
    // The tests
    // ==========================================================================================

    /// [`resolve_sources`]: the four combinations of the two sources, and the partial cases.
    mod resolve_sources_combinations {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn neither_source_resolves_to_nothing() {
            assert_eq!(
                resolve(None, None),
                SourcesFormat {
                    algorithm: None,
                    url: None,
                    agreement: Agreement::Neither,
                    resolved: None,
                }
            );
        }

        #[test]
        fn the_algorithm_alone_is_taken() {
            assert_eq!(
                resolve(Some("AMUNDI-EN24"), None),
                SourcesFormat {
                    algorithm: Some(p("AMUNDI-EN24")),
                    url: None,
                    agreement: Agreement::AlgorithmOnly,
                    resolved: Some(p("AMUNDI-EN24")),
                }
            );
        }

        #[test]
        fn the_url_alone_is_taken() {
            assert_eq!(
                resolve(None, Some("EURIZON-EN23")),
                SourcesFormat {
                    algorithm: None,
                    url: Some(p("EURIZON-EN23")),
                    agreement: Agreement::UrlOnly,
                    resolved: Some(p("EURIZON-EN23")),
                }
            );
        }

        #[test]
        fn a_partial_alone_stays_partial() {
            let resolved = resolve(Some("MEDIOLANUM-IT24.*"), None);
            assert_eq!(resolved.agreement, Agreement::AlgorithmOnly);
            assert_eq!(resolved.resolved, Some(p("MEDIOLANUM-IT24.*")));
        }

        #[test]
        fn two_equal_complete_names_agree() {
            assert_eq!(
                resolve(Some("AMUNDI-EN24"), Some("AMUNDI-EN24")),
                SourcesFormat {
                    algorithm: Some(p("AMUNDI-EN24")),
                    url: Some(p("AMUNDI-EN24")),
                    agreement: Agreement::Agree,
                    resolved: Some(p("AMUNDI-EN24")),
                }
            );
        }

        /// The complete refines the partial: they agree, and the result is the complete one.
        #[test]
        fn a_partial_url_and_a_complete_algorithm_refining_it_agree_on_the_complete() {
            assert_eq!(
                resolve(Some("MEDIOLANUM-IT24.A"), Some("MEDIOLANUM")),
                SourcesFormat {
                    algorithm: Some(p("MEDIOLANUM-IT24.A")),
                    url: Some(p("MEDIOLANUM")),
                    agreement: Agreement::Agree,
                    resolved: Some(p("MEDIOLANUM-IT24.A")),
                }
            );
        }

        /// Not "the URL wins": the meet is more specific than both and contradicts neither.
        #[test]
        fn the_url_can_complete_a_partial_of_the_algorithm() {
            assert_eq!(
                resolve(Some("MEDIOLANUM-IT24.*"), Some("MEDIOLANUM-IT24.B")),
                SourcesFormat {
                    algorithm: Some(p("MEDIOLANUM-IT24.*")),
                    url: Some(p("MEDIOLANUM-IT24.B")),
                    agreement: Agreement::Agree,
                    resolved: Some(p("MEDIOLANUM-IT24.B")),
                }
            );
        }

        #[test]
        fn two_agreeing_partials_resolve_to_their_partial_meet() {
            let resolved = resolve(Some("MEDIOLANUM-IT"), Some("MEDIOLANUM-IT24.*"));
            assert_eq!(resolved.agreement, Agreement::Agree);
            assert_eq!(resolved.resolved, Some(p("MEDIOLANUM-IT24.*")));
        }

        /// Two incomparable partials whose meet the repository has: the meet, complete here.
        #[test]
        fn two_incomparable_partials_with_a_meet_in_the_repository_agree_on_it() {
            let resolved = resolve(Some("X-IT23@LUX.*"), Some("X-IT23@*.A"));
            assert_eq!(resolved.agreement, Agreement::Agree);
            assert_eq!(resolved.resolved, Some(p("X-IT23@LUX.A")));
        }

        #[test]
        fn different_entities_disagree_and_the_algorithm_wins() {
            assert_eq!(
                resolve(Some("AMUNDI-EN24"), Some("EURIZON-EN23")),
                SourcesFormat {
                    algorithm: Some(p("AMUNDI-EN24")),
                    url: Some(p("EURIZON-EN23")),
                    agreement: Agreement::Disagree,
                    resolved: Some(p("AMUNDI-EN24")),
                }
            );
        }

        #[test]
        fn two_complete_variants_of_one_format_disagree() {
            let resolved = resolve(Some("MEDIOLANUM-IT24.A"), Some("MEDIOLANUM-IT24.B"));
            assert_eq!(resolved.agreement, Agreement::Disagree);
            assert_eq!(resolved.resolved, Some(p("MEDIOLANUM-IT24.A")));
        }

        /// R10: compatible in the abstract, but `X-IT23@LUX.B` is not in the repository.
        #[test]
        fn an_abstract_meet_outside_the_repository_is_a_disagreement() {
            let resolved = resolve(Some("X-IT23@LUX.*"), Some("X-IT23@*.B"));
            assert_eq!(resolved.agreement, Agreement::Disagree);
            assert_eq!(resolved.resolved, Some(p("X-IT23@LUX.*")));
        }

        /// The algorithm wins whichever of the two is the partial.
        #[test]
        fn a_disagreeing_partial_algorithm_still_wins_over_a_complete_url() {
            let resolved = resolve(Some("EURIZON"), Some("AMUNDI-EN24"));
            assert_eq!(resolved.agreement, Agreement::Disagree);
            assert_eq!(resolved.resolved, Some(p("EURIZON")));
        }
    }

    /// The per-document printout calls [`resolve_sources`] too: it must not log, or a disagreement
    /// would be reported once per document and once more for the set.
    mod resolve_sources_is_silent {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_disagreement_emits_no_event() {
            let (resolved, records) =
                captured(|| resolve(Some("AMUNDI-EN24"), Some("EURIZON-EN23")));
            assert_eq!(resolved.agreement, Agreement::Disagree);
            assert!(records.is_empty(), "{records:?}");
        }

        #[test]
        fn an_agreement_emits_no_event() {
            let (resolved, records) =
                captured(|| resolve(Some("MEDIOLANUM-IT24.A"), Some("MEDIOLANUM")));
            assert_eq!(resolved.agreement, Agreement::Agree);
            assert!(records.is_empty(), "{records:?}");
        }

        #[test]
        fn neither_source_emits_no_event() {
            let (_, records) = captured(|| resolve(None, None));
            assert!(records.is_empty(), "{records:?}");
        }
    }

    /// [`combine_documents`]: per source over every document, then source against source.
    mod combine_documents_rules {
        use super::*;
        use pretty_assertions::assert_eq;

        fn combine(documents: &[DocumentSources]) -> Result<SourcesFormat, ConflictingFormats> {
            // Under the lock: it may log, and an event in flight while another test captures is
            // the race `SERIAL` exists for.
            captured(|| combine_documents(&labelled(documents), &formats())).0
        }

        #[test]
        fn no_document_is_neither() {
            let combined = combine(&[]).unwrap();
            assert_eq!(combined.agreement, Agreement::Neither);
            assert_eq!(combined.resolved, None);
        }

        #[test]
        fn a_single_document_is_its_own_resolution() {
            let combined = combine(&[with_url(complete("AMUNDI-EN24"), Some("AMUNDI"))]).unwrap();
            assert_eq!(
                combined,
                resolve(Some("AMUNDI-EN24"), Some("AMUNDI")),
                "one document combined is that document's resolve_sources"
            );
        }

        #[test]
        fn agreeing_complete_documents_resolve_to_their_format() {
            let combined = combine(&[
                algorithm_only(complete("AMUNDI-EN24")),
                algorithm_only(complete("AMUNDI-EN24")),
            ])
            .unwrap();
            assert_eq!(combined.agreement, Agreement::AlgorithmOnly);
            assert_eq!(combined.algorithm, Some(p("AMUNDI-EN24")));
            assert_eq!(combined.resolved, Some(p("AMUNDI-EN24")));
        }

        /// §7: "a document classified `MEDIOLANUM-IT24.A` and one only `MEDIOLANUM` agree, and the
        /// format is `MEDIOLANUM-IT24.A`".
        #[test]
        fn a_complete_and_an_agreeing_partial_resolve_to_their_meet() {
            let combined = combine(&[
                algorithm_only(partial("MEDIOLANUM")),
                algorithm_only(complete("MEDIOLANUM-IT24.A")),
            ])
            .unwrap();
            assert_eq!(combined.algorithm, Some(p("MEDIOLANUM-IT24.A")));
            assert_eq!(combined.resolved, Some(p("MEDIOLANUM-IT24.A")));
        }

        #[test]
        fn two_agreeing_partials_stay_a_partial() {
            let combined = combine(&[
                algorithm_only(partial("MEDIOLANUM-IT")),
                algorithm_only(partial("MEDIOLANUM-IT24.*")),
            ])
            .unwrap();
            assert_eq!(combined.resolved, Some(p("MEDIOLANUM-IT24.*")));
        }

        #[test]
        fn an_unclassified_document_is_neutral() {
            let combined = combine(&[
                algorithm_only(DocClassification::Unclassified),
                algorithm_only(complete("EURIZON-EN23")),
                algorithm_only(DocClassification::Unclassified),
            ])
            .unwrap();
            assert_eq!(combined.agreement, Agreement::AlgorithmOnly);
            assert_eq!(combined.resolved, Some(p("EURIZON-EN23")));
        }

        #[test]
        fn a_document_without_url_or_with_an_unmapped_url_is_neutral_for_the_url() {
            let combined = combine(&[
                algorithm_only(DocClassification::Unclassified),
                with_url(DocClassification::Unclassified, None),
                url_only("EURIZON-EN23"),
            ])
            .unwrap();
            assert_eq!(combined.agreement, Agreement::UrlOnly);
            assert_eq!(combined.url, Some(p("EURIZON-EN23")));
            assert_eq!(combined.resolved, Some(p("EURIZON-EN23")));
        }

        #[test]
        fn documents_no_source_classifies_are_neither() {
            let combined = combine(&[
                algorithm_only(DocClassification::Unclassified),
                with_url(DocClassification::Unclassified, None),
            ])
            .unwrap();
            assert_eq!(combined.agreement, Agreement::Neither);
            assert_eq!(combined.resolved, None);
        }

        #[test]
        fn documents_disagreeing_on_the_algorithm_are_an_error_naming_every_classified_one() {
            let error = combine(&[
                algorithm_only(complete("AMUNDI-EN24")),
                algorithm_only(DocClassification::Unclassified),
                algorithm_only(complete("EURIZON-EN23")),
            ])
            .unwrap_err();
            assert_eq!(
                error,
                ConflictingFormats {
                    by: Source::Algorithm,
                    found: vec![
                        ("doc0.pdf".to_string(), p("AMUNDI-EN24")),
                        ("doc2.pdf".to_string(), p("EURIZON-EN23")),
                    ],
                }
            );
        }

        /// The labels in the error are the ones the caller gave, not positions.
        #[test]
        fn the_error_carries_the_labels_given_by_the_caller() {
            let documents = vec![
                (
                    "Amundi".to_string(),
                    algorithm_only(complete("AMUNDI-EN24")),
                ),
                (
                    "Blank".to_string(),
                    algorithm_only(DocClassification::Unclassified),
                ),
                (
                    "/x/eurizon.pdf".to_string(),
                    algorithm_only(complete("EURIZON-EN23")),
                ),
            ];
            let error = captured(|| combine_documents(&documents, &formats()))
                .0
                .unwrap_err();
            assert_eq!(
                error.found,
                vec![
                    ("Amundi".to_string(), p("AMUNDI-EN24")),
                    ("/x/eurizon.pdf".to_string(), p("EURIZON-EN23")),
                ]
            );
        }

        #[test]
        fn documents_disagreeing_on_the_url_are_an_error_naming_every_mapped_one() {
            let error = combine(&[url_only("AMUNDI-EN24"), url_only("EURIZON-EN23")]).unwrap_err();
            assert_eq!(
                error,
                ConflictingFormats {
                    by: Source::Url,
                    found: vec![
                        ("doc0.pdf".to_string(), p("AMUNDI-EN24")),
                        ("doc1.pdf".to_string(), p("EURIZON-EN23")),
                    ],
                }
            );
        }

        /// Each of the last two is compatible with the first, but not with each other: what
        /// matters is the intersection of all the expansions, not the pairs with the first.
        #[test]
        fn documents_each_compatible_with_the_first_but_with_an_empty_intersection_disagree() {
            let error = combine(&[
                algorithm_only(partial("MEDIOLANUM")),
                algorithm_only(complete("MEDIOLANUM-IT24.A")),
                algorithm_only(complete("MEDIOLANUM-IT24.B")),
            ])
            .unwrap_err();
            assert_eq!(error.by, Source::Algorithm);
            assert_eq!(
                error.found,
                vec![
                    ("doc0.pdf".to_string(), p("MEDIOLANUM")),
                    ("doc1.pdf".to_string(), p("MEDIOLANUM-IT24.A")),
                    ("doc2.pdf".to_string(), p("MEDIOLANUM-IT24.B")),
                ]
            );
        }

        /// R10 between documents: an abstract meet the repository does not have.
        #[test]
        fn documents_whose_meet_is_outside_the_repository_disagree() {
            let error = combine(&[
                algorithm_only(partial("X-IT23@LUX.*")),
                algorithm_only(partial("X-IT23@*.B")),
            ])
            .unwrap_err();
            assert_eq!(error.by, Source::Algorithm);
        }

        /// The result is the intersection of the expansions, whatever the order of the documents.
        #[test]
        fn every_permutation_of_agreeing_documents_gives_the_same_outcome() {
            let documents = [
                algorithm_only(partial("MEDIOLANUM")),
                algorithm_only(partial("MEDIOLANUM-IT24.*")),
                with_url(complete("MEDIOLANUM-IT24.A"), Some("MEDIOLANUM-IT")),
            ];
            let expected = combine(&documents).unwrap();
            assert_eq!(expected.resolved, Some(p("MEDIOLANUM-IT24.A")));
            for order in [[0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
                let permuted: Vec<DocumentSources> =
                    order.iter().map(|&i| documents[i].clone()).collect();
                assert_eq!(combine(&permuted).unwrap(), expected, "order {order:?}");
            }
        }

        #[test]
        fn every_permutation_of_disagreeing_documents_is_the_same_error_source() {
            let documents = [
                algorithm_only(partial("MEDIOLANUM")),
                algorithm_only(complete("MEDIOLANUM-IT24.A")),
                algorithm_only(complete("MEDIOLANUM-IT24.B")),
            ];
            for order in [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ] {
                let permuted: Vec<DocumentSources> =
                    order.iter().map(|&i| documents[i].clone()).collect();
                let error = combine(&permuted).unwrap_err();
                assert_eq!(error.by, Source::Algorithm, "order {order:?}");
                assert_eq!(error.found.len(), 3, "order {order:?}");
            }
        }

        /// User decision Q1: per source first, then source against source. Document 1 is
        /// classified only by the algorithm, document 2 only by its URL, and the two disagree: the
        /// sources disagree (the algorithm wins), not the documents.
        #[test]
        fn one_document_by_algorithm_and_another_by_url_disagreeing_is_a_source_disagreement() {
            let combined = combine(&[
                algorithm_only(complete("AMUNDI-EN24")),
                url_only("EURIZON-EN23"),
            ])
            .unwrap();
            assert_eq!(
                combined,
                SourcesFormat {
                    algorithm: Some(p("AMUNDI-EN24")),
                    url: Some(p("EURIZON-EN23")),
                    agreement: Agreement::Disagree,
                    resolved: Some(p("AMUNDI-EN24")),
                }
            );
        }

        #[test]
        fn the_folded_url_can_complete_the_folded_algorithm() {
            let combined = combine(&[
                algorithm_only(partial("MEDIOLANUM-IT24.*")),
                url_only("MEDIOLANUM-IT24.B"),
            ])
            .unwrap();
            assert_eq!(combined.agreement, Agreement::Agree);
            assert_eq!(combined.resolved, Some(p("MEDIOLANUM-IT24.B")));
        }
    }

    /// What [`combine_documents`] logs: the disagreement of the sources, once.
    mod combine_documents_logging {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_disagreement_of_the_sources_is_one_error_naming_both_patterns() {
            let (combined, records) = captured(|| {
                combine_documents(
                    &labelled(&[with_url(complete("AMUNDI-EN24"), Some("EURIZON-EN23"))]),
                    &formats(),
                )
            });
            assert_eq!(combined.unwrap().agreement, Agreement::Disagree);
            let errors = at(&records, tracing::Level::ERROR);
            assert_eq!(errors.len(), 1, "{records:?}");
            let text = text_of(errors[0]);
            assert!(text.contains("AMUNDI-EN24"), "{text}");
            assert!(text.contains("EURIZON-EN23"), "{text}");
        }

        /// Phase 11, §4.3: in the run with a configured format the algorithm does not win, so the
        /// `ERROR` says only that the two disagree — whoever decides says so on its own.
        #[test]
        fn the_disagreement_error_does_not_say_who_wins() {
            let (_, records) = captured(|| {
                combine_documents(
                    &labelled(&[with_url(complete("AMUNDI-EN24"), Some("EURIZON-EN23"))]),
                    &formats(),
                )
            });
            let errors = at(&records, tracing::Level::ERROR);
            assert_eq!(errors.len(), 1, "{records:?}");
            let message = &errors[0].message;
            assert!(
                message.contains("the algorithm and the url disagree on the format"),
                "{message}"
            );
            assert!(!message.contains("taken"), "{message}");
            assert!(!message.contains("wins"), "{message}");
            assert_eq!(errors[0].field("algorithm"), Some("AMUNDI-EN24"));
            assert_eq!(errors[0].field("url"), Some("EURIZON-EN23"));
        }

        #[test]
        fn an_agreement_is_neither_an_error_nor_a_warning() {
            let (combined, records) = captured(|| {
                combine_documents(
                    &labelled(&[with_url(complete("MEDIOLANUM-IT24.A"), Some("MEDIOLANUM"))]),
                    &formats(),
                )
            });
            assert_eq!(combined.unwrap().agreement, Agreement::Agree);
            assert!(
                at(&records, tracing::Level::ERROR).is_empty(),
                "{records:?}"
            );
            assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
        }

        #[test]
        fn a_single_source_is_neither_an_error_nor_a_warning() {
            let (_, records) = captured(|| {
                combine_documents(
                    &labelled(&[algorithm_only(complete("AMUNDI-EN24"))]),
                    &formats(),
                )
            });
            assert!(
                at(&records, tracing::Level::ERROR).is_empty(),
                "{records:?}"
            );
            assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
        }
    }

    /// [`ConflictingFormats`]: the message `main` prints for documents disagreeing.
    mod conflicting_formats_message {
        use super::*;
        use pretty_assertions::assert_eq;

        fn conflict(found: &[(&str, &str)]) -> ConflictingFormats {
            ConflictingFormats {
                by: Source::Url,
                found: found
                    .iter()
                    .map(|(label, pattern)| (label.to_string(), p(pattern)))
                    .collect(),
            }
        }

        #[test]
        fn it_names_the_source_and_every_pattern_found() {
            let message =
                conflict(&[("a.pdf", "AMUNDI-EN24"), ("d.pdf", "EURIZON-EN23")]).to_string();
            assert!(
                message.starts_with("documents disagree on the url format"),
                "{message}"
            );
            assert!(message.contains("AMUNDI-EN24"), "{message}");
            assert!(message.contains("EURIZON-EN23"), "{message}");
        }

        /// User decision of 2026-10-17: the documents are named by their label (the path or the
        /// name given with `-i`, as `classify` prints them), each beside its own pattern.
        #[test]
        fn it_names_every_document_by_its_label_beside_its_pattern() {
            let message = conflict(&[
                ("/reports/amundi.pdf", "AMUNDI-EN24"),
                ("Eurizon 2023", "EURIZON-EN23"),
            ])
            .to_string();
            assert!(
                message.contains("/reports/amundi.pdf is AMUNDI-EN24"),
                "{message}"
            );
            assert!(
                message.contains("Eurizon 2023 is EURIZON-EN23"),
                "{message}"
            );
        }

        /// No position: the old wording `document 0 is …` is gone.
        #[test]
        fn it_does_not_name_documents_by_index() {
            let message =
                conflict(&[("a.pdf", "AMUNDI-EN24"), ("b.pdf", "EURIZON-EN23")]).to_string();
            assert!(!message.contains("document 0"), "{message}");
            assert!(!message.contains("document 1"), "{message}");
        }

        /// The labels follow the order of the documents, whatever it is.
        #[test]
        fn the_labels_come_in_the_order_of_the_documents() {
            let message =
                conflict(&[("zeta.pdf", "AMUNDI-EN24"), ("alpha.pdf", "EURIZON-EN23")]).to_string();
            let zeta = message.find("zeta.pdf").expect("zeta named");
            let alpha = message.find("alpha.pdf").expect("alpha named");
            assert!(zeta < alpha, "{message}");
        }

        #[test]
        fn the_algorithm_source_reads_as_algorithm() {
            assert_eq!(Source::Algorithm.to_string(), "algorithm");
            assert_eq!(Source::Url.to_string(), "url");
        }
    }

    /// [`SourcesFormat::outcome`]: complete, partial with its candidates, nothing.
    mod outcome {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_complete_resolution_is_complete() {
            assert_eq!(
                resolve(Some("AMUNDI-EN24"), None).outcome(&formats()),
                FormatOutcome::Complete(p("AMUNDI-EN24"))
            );
        }

        #[test]
        fn a_partial_resolution_lists_its_candidates_in_formats_csv_order() {
            // Not alphabetical: the order of `formats.csv`, here with B before A.
            let formats: Vec<String> = ["MEDIOLANUM-IT24.B", "AMUNDI-EN24", "MEDIOLANUM-IT24.A"]
                .iter()
                .map(|f| f.to_string())
                .collect();
            let resolved = resolve_sources(Some(&p("MEDIOLANUM-IT24.*")), None, &formats);
            assert_eq!(
                resolved.outcome(&formats),
                FormatOutcome::Partial {
                    pattern: p("MEDIOLANUM-IT24.*"),
                    candidates: vec![
                        "MEDIOLANUM-IT24.B".to_string(),
                        "MEDIOLANUM-IT24.A".to_string()
                    ],
                }
            );
        }

        /// Plan §2: no automatic attempt, even with a single candidate.
        #[test]
        fn a_partial_with_a_single_candidate_stays_partial() {
            assert_eq!(
                resolve(Some("MEDIOLANUM-EN"), None).outcome(&formats()),
                FormatOutcome::Partial {
                    pattern: p("MEDIOLANUM-EN"),
                    candidates: vec!["MEDIOLANUM-EN23".to_string()],
                }
            );
        }

        #[test]
        fn no_resolution_is_none() {
            assert_eq!(resolve(None, None).outcome(&formats()), FormatOutcome::None);
        }

        /// A disagreement's outcome is the algorithm's.
        #[test]
        fn a_disagreement_has_the_outcome_of_the_algorithm() {
            assert_eq!(
                resolve(Some("AMUNDI-EN24"), Some("EURIZON-EN23")).outcome(&formats()),
                FormatOutcome::Complete(p("AMUNDI-EN24"))
            );
        }
    }

    /// Phase 11, [`resolve_job_format`]: the run's priority — configuration > algorithm > url — on
    /// top of the job's combined [`SourcesFormat`], and the events of the crossings
    /// (`agent-memory/DESIGN-phase11-run-integration.md` §2.2, §4, §8.B).
    mod job_format {
        use super::*;

        /// The job's combined sources, as [`combine_documents`] would give them.
        fn sources(algorithm: Option<&str>, url: Option<&str>) -> SourcesFormat {
            resolve(algorithm, url)
        }

        /// [`resolve_job_format`] against the fixture's formats, with every event it emits.
        fn job(
            configured: Option<&str>,
            algorithm: Option<&str>,
            url: Option<&str>,
        ) -> (Result<JobFormat, JobFormatError>, Vec<Record>) {
            let combined = sources(algorithm, url);
            captured(|| resolve_job_format(configured, &combined, &formats()))
        }

        fn configured(format: &str) -> JobFormat {
            JobFormat {
                format: format.to_string(),
                origin: FormatOrigin::Configuration,
            }
        }

        /// The events at `INFO` or above: `INFO`, `WARN`, `ERROR`.
        fn at_info_or_above(records: &[Record]) -> Vec<&Record> {
            records
                .iter()
                .filter(|r| {
                    matches!(
                        r.level,
                        Some(tracing::Level::INFO | tracing::Level::WARN | tracing::Level::ERROR)
                    )
                })
                .collect()
        }

        /// A configured format and no source: nothing to compare, nothing to say.
        mod configured_without_sources {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn the_configured_format_is_taken() {
                let (result, _) = job(Some("AMUNDI-EN24"), None, None);
                assert_eq!(result, Ok(configured("AMUNDI-EN24")));
            }

            #[test]
            fn nothing_is_logged_at_info_or_above() {
                let (_, records) = job(Some("AMUNDI-EN24"), None, None);
                assert!(at_info_or_above(&records).is_empty(), "{records:?}");
            }
        }

        /// A configured format that every present source confirms: a `DEBUG`, nothing louder.
        mod configured_and_confirmed {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn the_configured_format_is_taken() {
                let (result, _) = job(
                    Some("AMUNDI-EN24"),
                    Some("AMUNDI-EN24"),
                    Some("AMUNDI-EN24"),
                );
                assert_eq!(result, Ok(configured("AMUNDI-EN24")));
            }

            #[test]
            fn there_is_no_warning_and_no_error() {
                let (_, records) = job(
                    Some("AMUNDI-EN24"),
                    Some("AMUNDI-EN24"),
                    Some("AMUNDI-EN24"),
                );
                assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
                assert!(
                    at(&records, tracing::Level::ERROR).is_empty(),
                    "{records:?}"
                );
            }

            #[test]
            fn one_debug_event_confirms_the_configured_format() {
                let (_, records) = job(
                    Some("AMUNDI-EN24"),
                    Some("AMUNDI-EN24"),
                    Some("AMUNDI-EN24"),
                );
                let confirmations: Vec<&Record> = at(&records, tracing::Level::DEBUG)
                    .into_iter()
                    .filter(|r| {
                        r.message
                            .contains("configured format confirmed by the document")
                    })
                    .collect();
                assert_eq!(confirmations.len(), 1, "{records:?}");
            }

            /// §1.5: agreeing is being compatible in the repository, not being equal. A partial
            /// the configured format refines confirms it.
            #[test]
            fn a_partial_algorithm_the_configured_format_refines_is_no_warning() {
                let (result, records) = job(Some("MEDIOLANUM-IT24.A"), Some("MEDIOLANUM"), None);
                assert_eq!(result, Ok(configured("MEDIOLANUM-IT24.A")));
                assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
            }

            #[test]
            fn a_partial_url_the_configured_format_refines_is_no_warning() {
                let (result, records) = job(Some("MEDIOLANUM-IT24.B"), None, Some("MEDIOLANUM-IT"));
                assert_eq!(result, Ok(configured("MEDIOLANUM-IT24.B")));
                assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
            }
        }

        /// The configuration against a source that says something else: the configuration wins,
        /// with a `WARN` that no setting silences (crossings 2 and 3 of §4).
        mod configured_and_contradicted {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn an_algorithm_disagreeing_is_one_warning_naming_both_and_the_configured_one_wins() {
                let (result, records) = job(Some("AMUNDI-EN24"), Some("EURIZON-EN23"), None);
                assert_eq!(result, Ok(configured("AMUNDI-EN24")));
                let warnings = at(&records, tracing::Level::WARN);
                assert_eq!(warnings.len(), 1, "{records:?}");
                assert_eq!(warnings[0].field("configured"), Some("AMUNDI-EN24"));
                assert_eq!(warnings[0].field("algorithm"), Some("EURIZON-EN23"));
                assert!(
                    warnings[0].message.contains("document classification"),
                    "{}",
                    warnings[0].message
                );
                assert!(
                    warnings[0].message.contains("the configured one is used"),
                    "{}",
                    warnings[0].message
                );
            }

            #[test]
            fn a_url_disagreeing_is_one_warning_naming_both_and_the_configured_one_wins() {
                let (result, records) = job(Some("AMUNDI-EN24"), None, Some("EURIZON-EN23"));
                assert_eq!(result, Ok(configured("AMUNDI-EN24")));
                let warnings = at(&records, tracing::Level::WARN);
                assert_eq!(warnings.len(), 1, "{records:?}");
                assert_eq!(warnings[0].field("configured"), Some("AMUNDI-EN24"));
                assert_eq!(warnings[0].field("url"), Some("EURIZON-EN23"));
                assert!(
                    warnings[0].message.contains("report url"),
                    "{}",
                    warnings[0].message
                );
                assert!(
                    warnings[0].message.contains("the configured one is used"),
                    "{}",
                    warnings[0].message
                );
            }

            /// The message of today's `detect_format` is gone: crossing 3 replaces it.
            #[test]
            fn the_old_selected_format_message_is_not_used() {
                let (_, records) = job(Some("AMUNDI-EN24"), None, Some("EURIZON-EN23"));
                assert!(
                    !records
                        .iter()
                        .any(|r| r.message.contains("selected format is different")),
                    "{records:?}"
                );
            }

            /// Two different facts, two lines: and since algorithm and url agree with each other,
            /// no `ERROR`.
            #[test]
            fn both_sources_disagreeing_are_two_warnings_and_no_error() {
                let (result, records) = job(
                    Some("AMUNDI-EN24"),
                    Some("EURIZON-EN23"),
                    Some("EURIZON-EN23"),
                );
                assert_eq!(result, Ok(configured("AMUNDI-EN24")));
                let warnings = at(&records, tracing::Level::WARN);
                assert_eq!(warnings.len(), 2, "{records:?}");
                assert!(
                    warnings
                        .iter()
                        .any(|w| w.field("algorithm") == Some("EURIZON-EN23")),
                    "{records:?}"
                );
                assert!(
                    warnings
                        .iter()
                        .any(|w| w.field("url") == Some("EURIZON-EN23")),
                    "{records:?}"
                );
                assert!(
                    at(&records, tracing::Level::ERROR).is_empty(),
                    "{records:?}"
                );
            }

            /// One source agreeing does not excuse the other.
            #[test]
            fn one_source_agreeing_and_one_disagreeing_is_one_warning_for_the_disagreeing_one() {
                let (_, records) = job(
                    Some("AMUNDI-EN24"),
                    Some("AMUNDI-EN24"),
                    Some("EURIZON-EN23"),
                );
                let warnings = at(&records, tracing::Level::WARN);
                assert_eq!(warnings.len(), 1, "{records:?}");
                assert_eq!(warnings[0].field("url"), Some("EURIZON-EN23"));
                assert_eq!(warnings[0].field("algorithm"), None);
            }

            #[test]
            fn an_incompatible_partial_algorithm_is_a_warning() {
                let (result, records) = job(Some("MEDIOLANUM-IT24.A"), Some("MEDIOLANUM-EN"), None);
                assert_eq!(result, Ok(configured("MEDIOLANUM-IT24.A")));
                assert_eq!(at(&records, tracing::Level::WARN).len(), 1, "{records:?}");
            }

            /// R10: compatible in the abstract, but the meet is not a format of the repository.
            #[test]
            fn a_configured_format_outside_the_repository_disagrees_with_a_compatible_partial() {
                let (result, records) = job(Some("MEDIOLANUM-IT24.Z"), Some("MEDIOLANUM"), None);
                assert_eq!(result, Ok(configured("MEDIOLANUM-IT24.Z")));
                assert_eq!(at(&records, tracing::Level::WARN).len(), 1, "{records:?}");
            }

            /// Not even a pattern: it disagrees with every source that answered, and is still the
            /// format of the job — the real error comes from `Algorithm::load`, as today.
            #[test]
            fn a_configured_format_that_is_not_a_pattern_disagrees_with_the_algorithm() {
                assert!(
                    "DOES-NOT-EXIST".parse::<FormatPattern>().is_err(),
                    "the fixture relies on DOES-NOT-EXIST not being a pattern"
                );
                let (result, records) = job(Some("DOES-NOT-EXIST"), Some("AMUNDI-EN24"), None);
                assert_eq!(result, Ok(configured("DOES-NOT-EXIST")));
                assert_eq!(at(&records, tracing::Level::WARN).len(), 1, "{records:?}");
            }

            #[test]
            fn a_configured_format_that_is_not_a_pattern_and_no_source_says_nothing() {
                let (result, records) = job(Some("DOES-NOT-EXIST"), None, None);
                assert_eq!(result, Ok(configured("DOES-NOT-EXIST")));
                assert!(at_info_or_above(&records).is_empty(), "{records:?}");
            }

            /// The algorithm–url crossing is `combine_documents`' to log, already: never twice.
            #[test]
            fn algorithm_and_url_disagreeing_with_each_other_is_no_error_here() {
                let (_, records) = job(
                    Some("AMUNDI-EN24"),
                    Some("AMUNDI-EN24"),
                    Some("EURIZON-EN23"),
                );
                assert!(
                    at(&records, tracing::Level::ERROR).is_empty(),
                    "{records:?}"
                );
            }
        }

        /// No configured format: the documents' format, or an error of the job (crossing 4).
        mod detected {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn a_complete_algorithm_is_the_format_by_the_algorithm() {
                let (result, _) = job(None, Some("AMUNDI-EN24"), None);
                assert_eq!(
                    result,
                    Ok(JobFormat {
                        format: "AMUNDI-EN24".to_string(),
                        origin: FormatOrigin::Algorithm,
                    })
                );
            }

            #[test]
            fn a_format_detected_by_the_algorithm_is_one_info_event_saying_by_what() {
                let (_, records) = job(None, Some("AMUNDI-EN24"), None);
                let detected: Vec<&Record> = at(&records, tracing::Level::INFO)
                    .into_iter()
                    .filter(|r| r.message.contains("format detected"))
                    .collect();
                assert_eq!(detected.len(), 1, "{records:?}");
                assert_eq!(detected[0].field("format"), Some("AMUNDI-EN24"));
                assert_eq!(detected[0].field("by"), Some("algorithm"));
                assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
            }

            #[test]
            fn the_url_alone_is_the_format_by_the_url() {
                let (result, records) = job(None, None, Some("EURIZON-EN23"));
                assert_eq!(
                    result,
                    Ok(JobFormat {
                        format: "EURIZON-EN23".to_string(),
                        origin: FormatOrigin::Url,
                    })
                );
                assert!(
                    at(&records, tracing::Level::INFO)
                        .iter()
                        .any(|r| r.message.contains("format detected")
                            && r.field("by") == Some("url")),
                    "{records:?}"
                );
            }

            #[test]
            fn agreeing_sources_are_the_format_by_both() {
                let (result, records) = job(None, Some("AMUNDI-EN24"), Some("AMUNDI-EN24"));
                assert_eq!(
                    result,
                    Ok(JobFormat {
                        format: "AMUNDI-EN24".to_string(),
                        origin: FormatOrigin::AlgorithmAndUrl,
                    })
                );
                assert!(
                    at(&records, tracing::Level::INFO).iter().any(|r| {
                        r.message.contains("format detected")
                            && r.field("by") == Some("algorithm and url")
                    }),
                    "{records:?}"
                );
            }

            /// The url completes a partial of the algorithm: their meet, by both.
            #[test]
            fn a_complete_url_completing_a_partial_algorithm_is_their_meet_by_both() {
                let (result, _) = job(None, Some("MEDIOLANUM-IT24.*"), Some("MEDIOLANUM-IT24.B"));
                assert_eq!(
                    result,
                    Ok(JobFormat {
                        format: "MEDIOLANUM-IT24.B".to_string(),
                        origin: FormatOrigin::AlgorithmAndUrl,
                    })
                );
            }

            /// Disagreeing sources: the algorithm wins, and the `ERROR` is `combine_documents`'
            /// (already emitted), not this function's.
            #[test]
            fn disagreeing_sources_are_the_algorithms_format_and_no_error_here() {
                let (result, records) = job(None, Some("AMUNDI-EN24"), Some("EURIZON-EN23"));
                assert_eq!(
                    result,
                    Ok(JobFormat {
                        format: "AMUNDI-EN24".to_string(),
                        origin: FormatOrigin::Algorithm,
                    })
                );
                assert!(
                    at(&records, tracing::Level::ERROR).is_empty(),
                    "{records:?}"
                );
            }

            /// The run executes no partial: an error with the candidates, in `formats.csv` order.
            #[test]
            fn a_partial_is_an_error_with_the_candidates_in_formats_csv_order() {
                let formats: Vec<String> =
                    ["MEDIOLANUM-IT24.B", "AMUNDI-EN24", "MEDIOLANUM-IT24.A"]
                        .iter()
                        .map(|f| f.to_string())
                        .collect();
                let combined = resolve_sources(Some(&p("MEDIOLANUM-IT24.*")), None, &formats);
                let (result, _) = captured(|| resolve_job_format(None, &combined, &formats));
                assert_eq!(
                    result,
                    Err(JobFormatError::Partial {
                        pattern: p("MEDIOLANUM-IT24.*"),
                        candidates: vec![
                            "MEDIOLANUM-IT24.B".to_string(),
                            "MEDIOLANUM-IT24.A".to_string(),
                        ],
                    })
                );
            }

            /// Plan §2: no automatic attempt, even with a single candidate.
            #[test]
            fn a_partial_with_a_single_candidate_is_still_an_error() {
                let (result, _) = job(None, Some("MEDIOLANUM-EN"), None);
                assert_eq!(
                    result,
                    Err(JobFormatError::Partial {
                        pattern: p("MEDIOLANUM-EN"),
                        candidates: vec!["MEDIOLANUM-EN23".to_string()],
                    })
                );
            }

            #[test]
            fn nothing_from_any_source_is_undetermined() {
                let (result, _) = job(None, None, None);
                assert_eq!(result, Err(JobFormatError::Undetermined));
            }

            /// The error is logged once, by the job (`job failed: …`): not here as well.
            #[test]
            fn the_errors_are_not_logged_here() {
                for (algorithm, url) in [(None, None), (Some("MEDIOLANUM-IT24.*"), None)] {
                    let (result, records) = job(None, algorithm, url);
                    assert!(result.is_err(), "{algorithm:?} {url:?}");
                    assert!(
                        at(&records, tracing::Level::ERROR).is_empty(),
                        "{records:?}"
                    );
                    assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
                }
            }
        }

        /// [`JobFormatError`]: the messages `main` prints for a job that cannot settle its format.
        mod job_format_error_message {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn undetermined_says_no_source_determines_the_format() {
                assert_eq!(
                    JobFormatError::Undetermined.to_string(),
                    "no format was configured, and neither the document classification nor the report url determines one"
                );
            }

            #[test]
            fn partial_names_the_pattern_and_lists_every_candidate() {
                let message = JobFormatError::Partial {
                    pattern: p("MEDIOLANUM-IT24.*"),
                    candidates: vec![
                        "MEDIOLANUM-IT24.A".to_string(),
                        "MEDIOLANUM-IT24.B".to_string(),
                    ],
                }
                .to_string();
                assert!(message.contains("MEDIOLANUM-IT24.*"), "{message}");
                assert!(message.contains("set it explicitly"), "{message}");
                assert!(
                    message.contains("MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.B"),
                    "{message}"
                );
            }
        }
    }

    /// Phase 11, [`url_source`]: what the url says alone, for the job's documents whose
    /// classification failed with a configured format (D5). No Python: only the CSV tables.
    mod url_source_alone {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::fs;

        use crate::formats_repo::metadata::METADATA_DIR;

        const URL_FORMATS_CSV: &str = "Name,Locale,Year,Country,Version\n\
                                       AMUNDI,EN,24,,\n\
                                       EURIZON,EN,23,,\n";

        fn repo(url_mapping: Option<&str>) -> tempfile::TempDir {
            let dir = tempfile::TempDir::new().unwrap();
            let metadata = dir.path().join(METADATA_DIR);
            fs::create_dir_all(&metadata).unwrap();
            fs::write(metadata.join("formats.csv"), URL_FORMATS_CSV).unwrap();
            if let Some(mapping) = url_mapping {
                fs::write(metadata.join("url_mapping.csv"), mapping).unwrap();
            }
            dir
        }

        fn url_formats() -> Vec<String> {
            vec!["AMUNDI-EN24".to_string(), "EURIZON-EN23".to_string()]
        }

        const MAPPING: &str = "Format name,Url\nAMUNDI-EN24,https://amundi.example/\n";

        #[test]
        fn a_mapped_url_gives_its_pattern() {
            let repo = repo(Some(MAPPING));
            let url = "https://amundi.example/2024/report.pdf";
            let (source, _) = captured(|| url_source(repo.path(), &url_formats(), url));
            assert_eq!(
                source,
                UrlSource {
                    url: url.to_string(),
                    pattern: Some(p("AMUNDI-EN24")),
                }
            );
        }

        #[test]
        fn an_unmapped_url_is_kept_without_a_pattern() {
            let repo = repo(Some(MAPPING));
            let url = "https://elsewhere.example/report.pdf";
            let (source, records) = captured(|| url_source(repo.path(), &url_formats(), url));
            assert_eq!(
                source,
                UrlSource {
                    url: url.to_string(),
                    pattern: None,
                }
            );
            assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
        }

        /// As in [`document_sources`]: an unreadable mapping costs the url source, with a warning.
        #[test]
        fn an_unreadable_url_mapping_is_one_warning_and_no_pattern() {
            let repo = repo(None);
            let url = "https://amundi.example/report.pdf";
            let (source, records) = captured(|| url_source(repo.path(), &url_formats(), url));
            assert_eq!(source.pattern, None);
            assert_eq!(source.url, url);
            assert_eq!(at(&records, tracing::Level::WARN).len(), 1, "{records:?}");
        }
    }

    /// [`document_sources`] on real PDFs: PyMuPDF and the author's Python. Needs the venv
    /// (`venv/freeports-dev`, see AGENTS.md).
    mod python_boundary {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::fs;
        use std::path::PathBuf;

        use pyo3::prelude::*;
        use tempfile::TempDir;

        use crate::formats_repo::metadata::METADATA_DIR;
        use crate::formats_repo::unstructured::loader::TEMPLATES_DIR;

        const FORMATS_CSV: &str = "Name,Locale,Year,Country,Version\n\
                                   AMUNDI,EN,24,,\n\
                                   EURIZON,EN,23,,\n\
                                   MEDIOLANUM,EN,23,,\n\
                                   MEDIOLANUM,IT,24,,A\n\
                                   MEDIOLANUM,IT,24,,B\n";

        const YAML: &str = "1:\n  AMUNDI-EN24: {text: '^Amundi Funds$'}\n";

        /// One pipeline reading every line beginning with `Conflict`, and giving
        /// `AMUNDI-EN24` for `Conflict A` and `EURIZON-EN23` for `Conflict B`: a page carrying both
        /// is a runtime classification error (two different values, D5).
        const CONFLICTING_PYTHON: &str = r#"
class P:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def texts(page):
    return [s['text'] for b in page['blocks'] for l in b.get('lines', []) for s in l['spans']]

def extract(page):
    return [{'type_block': 'DOC_CLASS', 'metadata': {}, 'content': t} for t in texts(page) if t.startswith('Conflict')]

def keep(blocks, data):
    return [{'type_block': 'DOC_CLASS', 'metadata': {}, 'content': b.content} for b in blocks]

def deserialize(block):
    return {'Conflict A': 'AMUNDI-EN24', 'Conflict B': 'EURIZON-EN23'}.get(block.content)

document_classifier = [(1, P(extract, keep, deserialize))]
"#;

        struct Repo {
            dir: TempDir,
        }

        impl Repo {
            /// `formats.csv`, the one-step YAML, and a URL mapping sending `https://amundi.example/`
            /// to `AMUNDI-EN24`.
            fn new() -> Self {
                let dir = TempDir::new().expect("temp dir");
                let metadata = dir.path().join(METADATA_DIR);
                fs::create_dir_all(&metadata).unwrap();
                fs::create_dir_all(dir.path().join(TEMPLATES_DIR)).unwrap();
                fs::write(metadata.join("formats.csv"), FORMATS_CSV).unwrap();
                fs::write(metadata.join("document_classify.yaml"), YAML).unwrap();
                fs::write(
                    metadata.join("url_mapping.csv"),
                    "Format name,Url\nAMUNDI-EN24,https://amundi.example/\n",
                )
                .unwrap();
                Repo { dir }
            }

            fn without_url_mapping(self) -> Self {
                fs::remove_file(self.dir.path().join(METADATA_DIR).join("url_mapping.csv"))
                    .unwrap();
                self
            }

            fn with_python(self, source: &str) -> Self {
                fs::write(
                    self.dir
                        .path()
                        .join(METADATA_DIR)
                        .join("document_classify.py"),
                    source,
                )
                .unwrap();
                self
            }

            fn path(&self) -> &Path {
                self.dir.path()
            }

            fn classifier(&self) -> DocumentClassifier {
                crate::formats_repo::doc_classify::load(self.path())
                    .unwrap_or_else(|e| panic!("the fixture classification must load: {e}"))
            }

            /// A one-page PDF in the repository's temporary directory, one line of text per entry.
            fn pdf(&self, name: &str, lines: &[&str]) -> PathBuf {
                let path = self.dir.path().join(name);
                Python::attach(|py| {
                    let fitz = PyModule::import(py, "fitz").expect(
                        "PyMuPDF (fitz) must be importable: activate venv/freeports-dev, see AGENTS.md",
                    );
                    let doc = fitz.call_method0("open").unwrap();
                    let page = doc
                        .call_method1("new_page", (-1i64, 300.0f64, 400.0f64))
                        .unwrap();
                    for (i, line) in lines.iter().enumerate() {
                        let y = 50.0f64 + 20.0 * i as f64;
                        page.call_method1("insert_text", ((20.0f64, y), *line))
                            .unwrap();
                    }
                    doc.call_method1("save", (path.to_str().unwrap(),)).unwrap();
                    doc.call_method0("close").unwrap();
                });
                path
            }

            /// [`document_sources`] under the engine tests' lock, never held while attached to
            /// Python.
            fn sources(
                &self,
                pdf: &Path,
                url: Option<&str>,
            ) -> (Result<DocumentSources, DocumentSourcesError>, Vec<Record>) {
                let classifier = self.classifier();
                captured(|| document_sources(&classifier, self.path(), pdf, url))
            }
        }

        fn complete_format(classification: &DocClassification) -> &FormatPattern {
            match classification {
                DocClassification::Complete(complete) => &complete.format,
                other => panic!("expected a complete classification, got {other:?}"),
            }
        }

        #[test]
        fn a_local_pdf_the_yaml_classifies_has_a_complete_algorithm_and_no_url() {
            let repo = Repo::new();
            let pdf = repo.pdf("amundi.pdf", &["Amundi Funds"]);
            let sources = repo.sources(&pdf, None).0.unwrap();
            assert_eq!(complete_format(&sources.algorithm), &p("AMUNDI-EN24"));
            assert_eq!(sources.url, None);
        }

        #[test]
        fn a_pdf_nothing_recognises_is_unclassified() {
            let repo = Repo::new();
            let pdf = repo.pdf("blank.pdf", &["Nothing to see"]);
            let sources = repo.sources(&pdf, None).0.unwrap();
            assert_eq!(sources.algorithm, DocClassification::Unclassified);
        }

        #[test]
        fn a_mapped_url_gives_its_pattern() {
            let repo = Repo::new();
            let pdf = repo.pdf("blank.pdf", &["Nothing to see"]);
            let url = "https://amundi.example/2024/report.pdf";
            let sources = repo.sources(&pdf, Some(url)).0.unwrap();
            assert_eq!(
                sources.url,
                Some(UrlSource {
                    url: url.to_string(),
                    pattern: Some(p("AMUNDI-EN24")),
                })
            );
            assert_eq!(sources.algorithm, DocClassification::Unclassified);
        }

        #[test]
        fn an_unmapped_url_is_kept_without_a_pattern() {
            let repo = Repo::new();
            let pdf = repo.pdf("amundi.pdf", &["Amundi Funds"]);
            let url = "https://elsewhere.example/report.pdf";
            let sources = repo.sources(&pdf, Some(url)).0.unwrap();
            assert_eq!(
                sources.url,
                Some(UrlSource {
                    url: url.to_string(),
                    pattern: None,
                })
            );
            assert_eq!(complete_format(&sources.algorithm), &p("AMUNDI-EN24"));
        }

        /// As `detect_format` today: a URL mapping that cannot be read is a warning and no
        /// pattern, not a failure of the document.
        #[test]
        fn an_unreadable_url_mapping_is_one_warning_and_no_pattern() {
            let repo = Repo::new().without_url_mapping();
            let pdf = repo.pdf("amundi.pdf", &["Amundi Funds"]);
            let url = "https://amundi.example/report.pdf";
            let (result, records) = repo.sources(&pdf, Some(url));
            let sources = result.expect("an unreadable mapping is not an error");
            assert_eq!(
                sources.url,
                Some(UrlSource {
                    url: url.to_string(),
                    pattern: None,
                })
            );
            assert_eq!(complete_format(&sources.algorithm), &p("AMUNDI-EN24"));
            assert_eq!(at(&records, tracing::Level::WARN).len(), 1, "{records:?}");
        }

        /// Without a URL the mapping is never read: a missing one says nothing.
        #[test]
        fn without_a_url_an_unreadable_url_mapping_is_not_even_read() {
            let repo = Repo::new().without_url_mapping();
            let pdf = repo.pdf("amundi.pdf", &["Amundi Funds"]);
            let (result, records) = repo.sources(&pdf, None);
            assert_eq!(result.unwrap().url, None);
            assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
        }

        #[test]
        fn a_file_that_is_not_a_pdf_is_an_open_error() {
            let repo = Repo::new();
            let path = repo.path().join("broken.pdf");
            fs::write(&path, b"this is not a PDF at all").unwrap();
            let (result, _) = repo.sources(&path, None);
            assert!(
                matches!(result, Err(DocumentSourcesError::Open(_))),
                "{result:?}"
            );
        }

        #[test]
        fn a_missing_file_is_an_open_error() {
            let repo = Repo::new();
            let (result, _) = repo.sources(&repo.path().join("missing.pdf"), None);
            assert!(
                matches!(result, Err(DocumentSourcesError::Open(_))),
                "{result:?}"
            );
        }

        /// D5: a Python pipeline giving two different values is the document's classification
        /// error, not a fallback on the URL.
        #[test]
        fn a_python_pipeline_giving_two_values_is_a_classification_error() {
            let repo = Repo::new().with_python(CONFLICTING_PYTHON);
            let pdf = repo.pdf("conflict.pdf", &["Conflict A", "Conflict B"]);
            let (result, _) = repo.sources(&pdf, Some("https://amundi.example/report.pdf"));
            assert!(
                matches!(
                    result,
                    Err(DocumentSourcesError::Classify(
                        DocClassifyError::ConflictingDocClass { .. }
                    ))
                ),
                "{result:?}"
            );
        }

        /// The same Python, on a page without the two lines, says nothing, and the YAML decides.
        #[test]
        fn the_python_fixture_says_nothing_on_other_pages() {
            let repo = Repo::new().with_python(CONFLICTING_PYTHON);
            let pdf = repo.pdf("amundi.pdf", &["Amundi Funds"]);
            let sources = repo.sources(&pdf, None).0.unwrap();
            assert_eq!(complete_format(&sources.algorithm), &p("AMUNDI-EN24"));
        }
    }
}
