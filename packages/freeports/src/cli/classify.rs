//! `freeports classify`: the format of one or more documents, without extracting anything.
//!
//! Design: `agent-memory/DESIGN-phase10-classify.md` §4–§6. The configuration comes from the same
//! three tiers as the run ([`resolve_config`], twin of [`super::run::resolve_configs`]); the
//! documents are resolved as in the run (downloaded if needed, saved when `save_pdf` says so); each
//! is classified with [`document_sources`](crate::formats_repo::doc_classify_sources::document_sources),
//! the set is combined with
//! [`combine_documents`](crate::formats_repo::doc_classify_sources::combine_documents), and the
//! outcome goes to **stdout**, at every verbosity ([`render`]). The log goes to stderr.
//!
//! # Exit codes
//!
//! The table of `main.rs`, with the same meanings: `0` every document read (also "not classified",
//! a partial, or algorithm and URL disagreeing); `1` an illegal configuration, a classifier that
//! will not load, documents disagreeing with each other, no document readable; `3` some documents
//! read and some not.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::cli::conf_parse::DocumentSpec;
use crate::cli::config_locations::cmd::ClassifyArgs;
use crate::cli::freeports_config::validate_document_specs;
use crate::cli::job::{JobError, resolve_document_path};
use crate::cli::partial_config::PartialConfig;
use crate::cli::run::{CliError, RunCompleteness, merge_sources};
use crate::core::doc_classify::{
    CompleteClassification, DecidedBy, DocClassification, DocumentClassifier,
};
use crate::core::tracing_setup::{LogHandle, Verbosity, log_error};
use crate::formats_repo::doc_classify::{self, DocClassifyLoadError};
use crate::formats_repo::doc_classify_sources::{
    Agreement, ConflictingFormats, DocumentSources, DocumentSourcesError, FormatOutcome,
    SourcesFormat, combine_documents, document_sources,
};

/// The resolved configuration of `freeports classify`: the only settings it reads.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifyConfig {
    pub verbosity: Verbosity,
    /// Already through `validate_document_specs`: directories expanded, a URL-only document given
    /// its download path when `save_pdf` is on.
    pub reports: Vec<DocumentSpec>,
    pub formats_repo_path: PathBuf,
    pub save_pdf: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ClassifyError {
    #[error("no document to classify was given by any configuration source")]
    NoDocuments,
    #[error("no formats_repo_path was configured")]
    MissingFormatsRepoPath,
    #[error(transparent)]
    Load(#[from] DocClassifyLoadError),
    #[error(transparent)]
    Conflicting(#[from] ConflictingFormats),
    #[error("none of the {count} documents could be read")]
    NoDocumentReadable { count: usize },
}

/// What `execute` found, as [`render`] prints it.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifyReport {
    /// The formats of `formats.csv`, in file order: the candidates of a partial follow it.
    pub formats: Vec<String>,
    /// One per document, in the order given.
    pub documents: Vec<DocumentReport>,
    /// The combination of the readable documents. Printed as the `format:` line only with more
    /// than one document; `None` when the documents disagree with each other.
    pub combined: Option<SourcesFormat>,
}

/// One document of a [`ClassifyReport`].
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentReport {
    /// The spec's `name` if any, otherwise its path, otherwise its URL.
    pub label: String,
    /// The two sources, or why the document could not be read or classified (the message).
    pub outcome: Result<DocumentSources, String>,
}

/// The seam of the tier-precedence tests, twin of [`super::run::resolve_configs`].
pub fn resolve_config(args: ClassifyArgs) -> Result<ClassifyConfig, CliError> {
    let span = tracing::info_span!("resolve_config");
    let _guard = span.enter();

    // Every other setting of the merge — format, batch file, target lists, output, parallelism —
    // is the run's: ignored here, never an error.
    let values = merge_sources(args.to_partial_config()?)?.values;
    let config = validate(values)?;
    tracing::debug!(
        document_count = config.reports.len(),
        formats_repo = %config.formats_repo_path.display(),
        "classify configuration resolved"
    );
    Ok(config)
}

fn validate(values: PartialConfig) -> Result<ClassifyConfig, CliError> {
    let reports = values
        .reports
        .filter(|reports| !reports.is_empty())
        .ok_or_else(|| logged(ClassifyError::NoDocuments))?;
    let formats_repo_path = values
        .formats_repo_path
        .ok_or_else(|| logged(ClassifyError::MissingFormatsRepoPath))?;
    let save_pdf = values.save_pdf.unwrap_or(true);
    let reports = validate_document_specs(reports, save_pdf).map_err(|e| {
        tracing::error!(error = log_error(&e), "cannot validate configuration: {e}");
        CliError::from(e)
    })?;
    Ok(ClassifyConfig {
        verbosity: values.verbosity.unwrap_or(Verbosity::Warn),
        reports,
        formats_repo_path,
        save_pdf,
    })
}

/// Logs `error` once, where it is constructed, as every other area of the CLI does: `main` only
/// prints it.
fn logged(error: ClassifyError) -> CliError {
    tracing::error!(error = log_error(&error), "cannot classify: {error}");
    CliError::Classify(error)
}

/// Resolves the configuration, settles the verbosity (never a `.log.csv` destination), loads the
/// classifier, and for each document: resolves its path (downloading if needed), reads its sources
/// inside a `document` span; removes the temporary downloads; combines; prints on stdout; returns
/// the completeness.
pub fn execute(args: ClassifyArgs, log_handle: &LogHandle) -> Result<RunCompleteness, CliError> {
    let span = tracing::info_span!("classify");
    let _guard = span.enter();

    let config = resolve_config(args)?;
    // Never `set_csv_dir`: `classify` has no output directory, and leaves no `.log.csv` behind.
    log_handle.settle_verbosity(config.verbosity)?;
    let classifier = doc_classify::load(&config.formats_repo_path)
        .map_err(|e| logged(ClassifyError::Load(e)))?;

    let mut temp_files = Vec::new();
    let documents: Vec<DocumentReport> = config
        .reports
        .iter()
        .map(|spec| {
            read_document(
                &classifier,
                &config.formats_repo_path,
                spec,
                &mut temp_files,
            )
        })
        .collect();
    remove_temp_files(temp_files);

    let readable: Vec<(usize, DocumentSources)> = documents
        .iter()
        .enumerate()
        .filter_map(|(index, document)| Some((index, document.outcome.as_ref().ok()?.clone())))
        .collect();
    let mut report = ClassifyReport {
        formats: classifier.formats().to_vec(),
        documents,
        combined: None,
    };
    let unread = report.documents.len() - readable.len();
    if unread > 0 {
        tracing::warn!(
            skipped_documents = unread,
            "some documents could not be read and were left out of the format of the set"
        );
    }
    // The documents' lines are printed whatever follows: they are what tells the user which
    // documents failed, or which disagree.
    if readable.is_empty() {
        print!("{}", render(&report, config.verbosity));
        return Err(logged(ClassifyError::NoDocumentReadable {
            count: report.documents.len(),
        }));
    }
    let sources: Vec<DocumentSources> = readable.iter().map(|(_, s)| s.clone()).collect();
    match combine_documents(&sources, &report.formats) {
        Ok(combined) => report.combined = Some(combined),
        Err(mut conflict) => {
            print!("{}", render(&report, config.verbosity));
            // Back to the positions the user gave the documents in, unreadable ones included.
            for (index, _) in &mut conflict.found {
                *index = readable[*index].0;
            }
            return Err(logged(ClassifyError::Conflicting(conflict)));
        }
    }
    print!("{}", render(&report, config.verbosity));

    Ok(if unread > 0 {
        RunCompleteness::NotEverything
    } else {
        RunCompleteness::Everything
    })
}

/// Why one document has no sources: it could not be fetched, opened or classified.
#[derive(Debug, thiserror::Error)]
enum DocumentFailure {
    #[error(transparent)]
    Resolve(#[from] JobError),
    #[error(transparent)]
    Sources(#[from] DocumentSourcesError),
}

/// One document, inside its own `document` span: a failure costs that document only.
fn read_document(
    classifier: &DocumentClassifier,
    formats_repo_path: &Path,
    spec: &DocumentSpec,
    temp_files: &mut Vec<PathBuf>,
) -> DocumentReport {
    let label = label(spec);
    // `report`, as in the job: the field that fills the `Report` column of the log.
    let span = tracing::info_span!("document", report = %label);
    let _guard = span.enter();

    let outcome = sources_of(classifier, formats_repo_path, spec, temp_files).map_err(|error| {
        tracing::error!(
            error = log_error(&error),
            "document could not be read: {error}"
        );
        error.to_string()
    });
    DocumentReport { label, outcome }
}

// Large for carrying a `DocClassifyError` by value: see `formats_repo::doc_classify_sources`.
#[allow(clippy::result_large_err)]
fn sources_of(
    classifier: &DocumentClassifier,
    formats_repo_path: &Path,
    spec: &DocumentSpec,
    temp_files: &mut Vec<PathBuf>,
) -> Result<DocumentSources, DocumentFailure> {
    let (path, is_temp) = resolve_document_path(spec)?;
    if is_temp {
        temp_files.push(path.clone());
    }
    Ok(document_sources(
        classifier,
        formats_repo_path,
        &path,
        spec.url.as_deref(),
    )?)
}

fn remove_temp_files(temp_files: Vec<PathBuf>) {
    for temp in temp_files {
        if let Err(e) = std::fs::remove_file(&temp) {
            tracing::warn!(error = log_error(&e), path = %temp.display(), "failed to remove temporary downloaded file: {e}");
        }
    }
}

/// The spec's `name` if any, otherwise its path, otherwise its URL.
fn label(spec: &DocumentSpec) -> String {
    spec.name
        .clone()
        .or_else(|| spec.path.as_ref().map(|path| path.display().to_string()))
        .or_else(|| spec.url.clone())
        .unwrap_or_default()
}

/// Pure: what `execute` writes on stdout.
///
/// Choices the design leaves open:
/// - a failed document has no detail lines: its message is already on its line;
/// - the `sources:` line is always there in the detail, `neither` when no source classifies;
/// - the `format:` line counts every document given, so it follows two documents of which one
///   failed: it is then the format of the readable one, as the run would use it;
/// - with no readable document there is no `format:` line ([`ClassifyReport::combined`] is `None`).
pub fn render(report: &ClassifyReport, verbosity: Verbosity) -> String {
    let detailed = matches!(
        verbosity,
        Verbosity::Info | Verbosity::Debug | Verbosity::Trace
    );
    let formats = &report.formats;
    let mut out = String::new();
    for document in &report.documents {
        match &document.outcome {
            Err(message) => {
                let _ = writeln!(out, "{}: error: {message}", document.label);
            }
            Ok(sources) => {
                let resolved = sources.resolve(formats);
                let exclusion = match &sources.algorithm {
                    DocClassification::Complete(CompleteClassification {
                        by_exclusion: true,
                        ..
                    }) => " (by exclusion)",
                    _ => "",
                };
                let _ = writeln!(
                    out,
                    "{}: {}{exclusion}",
                    document.label,
                    outcome_text(&resolved.outcome(formats))
                );
                if detailed {
                    write_detail(&mut out, sources, resolved.agreement);
                }
            }
        }
    }
    if report.documents.len() > 1
        && let Some(combined) = &report.combined
    {
        let _ = writeln!(out, "format: {}", outcome_text(&combined.outcome(formats)));
    }
    out
}

fn outcome_text(outcome: &FormatOutcome) -> String {
    match outcome {
        FormatOutcome::Complete(format) => format.to_string(),
        FormatOutcome::Partial {
            pattern,
            candidates,
        } => format!("partial {pattern} — candidates: {}", candidates.join(", ")),
        FormatOutcome::None => "not classified".to_string(),
    }
}

fn write_detail(out: &mut String, sources: &DocumentSources, agreement: Agreement) {
    let algorithm = match &sources.algorithm {
        DocClassification::Complete(complete) => complete_detail(complete),
        DocClassification::Partial { pattern, .. } => format!("partial {pattern}"),
        DocClassification::Unclassified => "not classified".to_string(),
    };
    let _ = writeln!(out, "  algorithm: {algorithm}");
    if let Some(url) = &sources.url {
        let pattern = url
            .pattern
            .as_ref()
            .map_or_else(|| "not mapped".to_string(), ToString::to_string);
        let _ = writeln!(out, "  url: {pattern} — {}", url.url);
    }
    let agreement = match agreement {
        Agreement::Neither => "neither",
        Agreement::AlgorithmOnly => "algorithm only",
        Agreement::UrlOnly => "url only",
        Agreement::Agree => "agree",
        Agreement::Disagree => "disagree, the algorithm wins",
    };
    let _ = writeln!(out, "  sources: {agreement}");
}

fn complete_detail(complete: &CompleteClassification) -> String {
    let CompleteClassification {
        format,
        page,
        entry,
        decided_by,
        by_exclusion,
    } = complete;
    let decided_by = match decided_by {
        DecidedBy::Python { pipeline } => format!("Python entry {entry}, pipeline `{pipeline}`"),
        DecidedBy::Yaml { step, path } => format!("YAML entry {entry}, step `{step}` {path:?}"),
    };
    let exclusion = if *by_exclusion { " (by exclusion)" } else { "" };
    format!("{format} — page {page}, {decided_by}{exclusion}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroI32;

    use crate::core::doc_classify::{CompleteClassification, DecidedBy, DocClassification};
    use crate::core::pipeline::PipelineName;
    use crate::formats_repo::doc_classify_sources::{UrlSource, resolve_sources};
    use crate::formats_repo::format_pattern::FormatPattern;

    // ------------------------------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------------------------------

    const FORMATS: [&str; 5] = [
        "AMUNDI-EN24",
        "EURIZON-EN23",
        "MEDIOLANUM-EN23",
        "MEDIOLANUM-IT24.A",
        "MEDIOLANUM-IT24.B",
    ];

    fn formats() -> Vec<String> {
        FORMATS.iter().map(|f| f.to_string()).collect()
    }

    fn p(text: &str) -> FormatPattern {
        text.parse().expect("fixture pattern must be valid")
    }

    fn n(number: i32) -> NonZeroI32 {
        NonZeroI32::new(number).unwrap()
    }

    /// A complete outcome decided by a YAML step.
    fn by_yaml(
        format: &str,
        page: i32,
        entry: usize,
        step: &str,
        path: &[usize],
        by_exclusion: bool,
    ) -> DocClassification {
        DocClassification::Complete(CompleteClassification {
            format: p(format),
            page: n(page),
            entry,
            decided_by: DecidedBy::Yaml {
                step: step.to_string(),
                path: path.to_vec(),
            },
            by_exclusion,
        })
    }

    /// A complete outcome decided by a Python entry.
    fn by_python(format: &str, page: i32, entry: usize, pipeline: &str) -> DocClassification {
        DocClassification::Complete(CompleteClassification {
            format: p(format),
            page: n(page),
            entry,
            decided_by: DecidedBy::Python {
                pipeline: PipelineName::new(pipeline),
            },
            by_exclusion: false,
        })
    }

    /// The plain complete outcome most tests need.
    fn complete(format: &str) -> DocClassification {
        by_yaml(format, 1, 0, format, &[0], false)
    }

    fn partial(pattern: &str) -> DocClassification {
        DocClassification::Partial {
            pattern: p(pattern),
            candidates: p(pattern).expand(&formats()),
        }
    }

    fn read(label: &str, algorithm: DocClassification, url: Option<UrlSource>) -> DocumentReport {
        DocumentReport {
            label: label.to_string(),
            outcome: Ok(DocumentSources { algorithm, url }),
        }
    }

    fn failed(label: &str, message: &str) -> DocumentReport {
        DocumentReport {
            label: label.to_string(),
            outcome: Err(message.to_string()),
        }
    }

    fn url(url: &str, pattern: Option<&str>) -> Option<UrlSource> {
        Some(UrlSource {
            url: url.to_string(),
            pattern: pattern.map(p),
        })
    }

    fn report(documents: Vec<DocumentReport>) -> ClassifyReport {
        ClassifyReport {
            formats: formats(),
            documents,
            combined: None,
        }
    }

    fn lines(text: &str) -> Vec<&str> {
        text.lines().collect()
    }

    const QUIET_LEVELS: [Verbosity; 3] = [Verbosity::Silent, Verbosity::ErrorOnly, Verbosity::Warn];
    const DETAILED_LEVELS: [Verbosity; 3] = [Verbosity::Info, Verbosity::Debug, Verbosity::Trace];

    // ==========================================================================================
    // The tests: [`render`], pure
    // ==========================================================================================

    /// The one line per document of the default verbosity (§5).
    mod default_lines {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_complete_format() {
            let out = render(
                &report(vec![read("a.pdf", complete("MEDIOLANUM-IT24.A"), None)]),
                Verbosity::Warn,
            );
            assert_eq!(lines(&out), vec!["a.pdf: MEDIOLANUM-IT24.A"]);
        }

        /// D6: a format decided by an unconditional step says so.
        #[test]
        fn a_complete_format_by_exclusion() {
            let out = render(
                &report(vec![read(
                    "b.pdf",
                    by_yaml(
                        "MEDIOLANUM-IT24.A",
                        1,
                        0,
                        "MEDIOLANUM-IT24.A",
                        &[0, 1],
                        true,
                    ),
                    None,
                )]),
                Verbosity::Warn,
            );
            assert_eq!(lines(&out), vec!["b.pdf: MEDIOLANUM-IT24.A (by exclusion)"]);
        }

        #[test]
        fn a_partial_with_its_candidates_in_formats_csv_order() {
            let out = render(
                &report(vec![read("c.pdf", partial("MEDIOLANUM-IT24.*"), None)]),
                Verbosity::Warn,
            );
            assert_eq!(
                lines(&out),
                vec![
                    "c.pdf: partial MEDIOLANUM-IT24.* — candidates: MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.B"
                ]
            );
        }

        #[test]
        fn not_classified() {
            let out = render(
                &report(vec![read("d.pdf", DocClassification::Unclassified, None)]),
                Verbosity::Warn,
            );
            assert_eq!(lines(&out), vec!["d.pdf: not classified"]);
        }

        #[test]
        fn an_error() {
            let out = render(
                &report(vec![failed("e.pdf", "page 1 could not be read: boom")]),
                Verbosity::Warn,
            );
            assert_eq!(
                lines(&out),
                vec!["e.pdf: error: page 1 could not be read: boom"]
            );
        }

        /// The line is the document's [`resolve_sources`]: the URL alone decides when the algorithm
        /// does not classify.
        #[test]
        fn a_format_only_the_url_gives() {
            let out = render(
                &report(vec![read(
                    "f.pdf",
                    DocClassification::Unclassified,
                    url("https://x.example/f.pdf", Some("EURIZON-EN23")),
                )]),
                Verbosity::Warn,
            );
            assert_eq!(lines(&out), vec!["f.pdf: EURIZON-EN23"]);
        }

        /// The URL completes a partial of the algorithm: the line is the meet.
        #[test]
        fn a_partial_the_url_completes() {
            let out = render(
                &report(vec![read(
                    "g.pdf",
                    partial("MEDIOLANUM-IT24.*"),
                    url("https://x.example/g.pdf", Some("MEDIOLANUM-IT24.B")),
                )]),
                Verbosity::Warn,
            );
            assert_eq!(lines(&out), vec!["g.pdf: MEDIOLANUM-IT24.B"]);
        }

        /// Disagreeing sources: the algorithm wins on stdout (the `ERROR` is on stderr).
        #[test]
        fn disagreeing_sources_print_the_algorithms_format() {
            let out = render(
                &report(vec![read(
                    "h.pdf",
                    complete("AMUNDI-EN24"),
                    url("https://x.example/h.pdf", Some("EURIZON-EN23")),
                )]),
                Verbosity::Warn,
            );
            assert_eq!(lines(&out), vec!["h.pdf: AMUNDI-EN24"]);
        }

        #[test]
        fn one_line_per_document_in_the_order_given() {
            let out = render(
                &report(vec![
                    read("a.pdf", complete("AMUNDI-EN24"), None),
                    failed("e.pdf", "boom"),
                    read("d.pdf", DocClassification::Unclassified, None),
                ]),
                Verbosity::Warn,
            );
            assert_eq!(
                lines(&out),
                vec![
                    "a.pdf: AMUNDI-EN24",
                    "e.pdf: error: boom",
                    "d.pdf: not classified"
                ]
            );
        }
    }

    /// The detail lines: at `Info` and above, under each document; never below.
    mod detail_by_verbosity {
        use super::*;
        use pretty_assertions::assert_eq;

        fn sample() -> ClassifyReport {
            report(vec![read(
                "a.pdf",
                complete("MEDIOLANUM-IT24.A"),
                url("https://x.example/a.pdf", Some("MEDIOLANUM")),
            )])
        }

        #[test]
        fn below_info_there_is_only_the_line_of_the_document() {
            for verbosity in QUIET_LEVELS {
                assert_eq!(
                    lines(&render(&sample(), verbosity)),
                    vec!["a.pdf: MEDIOLANUM-IT24.A"],
                    "{verbosity:?}"
                );
            }
        }

        #[test]
        fn from_info_up_the_detail_follows_the_line_of_the_document() {
            for verbosity in DETAILED_LEVELS {
                let out = render(&sample(), verbosity);
                let lines = lines(&out);
                assert_eq!(lines[0], "a.pdf: MEDIOLANUM-IT24.A", "{verbosity:?}");
                assert!(
                    lines[1..].iter().all(|line| line.starts_with("  ")),
                    "{verbosity:?}: {lines:?}"
                );
                assert!(
                    lines.iter().any(|line| line.starts_with("  algorithm: ")),
                    "{verbosity:?}: {lines:?}"
                );
                assert!(
                    lines.iter().any(|line| line.starts_with("  url: ")),
                    "{verbosity:?}: {lines:?}"
                );
                assert!(
                    lines.iter().any(|line| line.starts_with("  sources: ")),
                    "{verbosity:?}: {lines:?}"
                );
            }
        }

        /// Debug and Trace add nothing on stdout: more goes to the log, not to the outcome.
        #[test]
        fn debug_and_trace_print_the_same_as_info() {
            let info = render(&sample(), Verbosity::Info);
            assert_eq!(render(&sample(), Verbosity::Debug), info);
            assert_eq!(render(&sample(), Verbosity::Trace), info);
        }

        #[test]
        fn the_detail_stays_under_its_own_document() {
            let out = render(
                &report(vec![
                    read("a.pdf", complete("AMUNDI-EN24"), None),
                    read("b.pdf", complete("EURIZON-EN23"), None),
                ]),
                Verbosity::Info,
            );
            let lines = lines(&out);
            let a = lines
                .iter()
                .position(|l| *l == "a.pdf: AMUNDI-EN24")
                .unwrap();
            let b = lines
                .iter()
                .position(|l| *l == "b.pdf: EURIZON-EN23")
                .unwrap();
            assert!(a < b, "{lines:?}");
            assert!(
                lines[a + 1..b]
                    .iter()
                    .any(|l| l.starts_with("  algorithm: AMUNDI-EN24")),
                "{lines:?}"
            );
            assert!(
                lines[b + 1..]
                    .iter()
                    .any(|l| l.starts_with("  algorithm: EURIZON-EN23")),
                "{lines:?}"
            );
        }
    }

    /// What the detail says: page, entry, Python pipeline or YAML step with its path; the forms
    /// of the `url` line; the four words of `sources`.
    mod detail_content {
        use super::*;

        fn detail_of(document: DocumentReport) -> Vec<String> {
            render(&report(vec![document]), Verbosity::Info)
                .lines()
                .skip(1)
                .map(str::to_string)
                .collect()
        }

        fn has(detail: &[String], line: &str) {
            assert!(
                detail.iter().any(|l| l == line),
                "expected the line {line:?} in {detail:#?}"
            );
        }

        #[test]
        fn a_yaml_step_by_exclusion_names_page_entry_step_and_path() {
            let detail = detail_of(read(
                "a.pdf",
                by_yaml(
                    "MEDIOLANUM-IT24.A",
                    1,
                    0,
                    "MEDIOLANUM-IT24.A",
                    &[0, 1],
                    true,
                ),
                None,
            ));
            has(
                &detail,
                "  algorithm: MEDIOLANUM-IT24.A — page 1, YAML entry 0, step `MEDIOLANUM-IT24.A` [0, 1] (by exclusion)",
            );
        }

        #[test]
        fn a_yaml_step_with_criteria_has_no_exclusion_note() {
            let detail = detail_of(read(
                "a.pdf",
                by_yaml("AMUNDI-EN24", 2, 1, "AMUNDI-EN24", &[3], false),
                None,
            ));
            has(
                &detail,
                "  algorithm: AMUNDI-EN24 — page 2, YAML entry 1, step `AMUNDI-EN24` [3]",
            );
        }

        #[test]
        fn a_python_entry_names_page_entry_and_pipeline() {
            let detail = detail_of(read(
                "a.pdf",
                by_python("MEDIOLANUM-IT24.A", -1, 2, "mediolanum"),
                None,
            ));
            has(
                &detail,
                "  algorithm: MEDIOLANUM-IT24.A — page -1, Python entry 2, pipeline `mediolanum`",
            );
        }

        /// A partial does not record its page: none is printed.
        #[test]
        fn a_partial_algorithm() {
            let detail = detail_of(read("a.pdf", partial("MEDIOLANUM"), None));
            has(&detail, "  algorithm: partial MEDIOLANUM");
        }

        #[test]
        fn an_unclassified_algorithm() {
            let detail = detail_of(read("a.pdf", DocClassification::Unclassified, None));
            has(&detail, "  algorithm: not classified");
        }

        #[test]
        fn a_mapped_url() {
            let detail = detail_of(read(
                "a.pdf",
                complete("MEDIOLANUM-IT24.A"),
                url("https://x.example/a.pdf", Some("MEDIOLANUM")),
            ));
            has(&detail, "  url: MEDIOLANUM — https://x.example/a.pdf");
        }

        #[test]
        fn an_unmapped_url() {
            let detail = detail_of(read(
                "a.pdf",
                complete("AMUNDI-EN24"),
                url("https://x.example/a.pdf", None),
            ));
            has(&detail, "  url: not mapped — https://x.example/a.pdf");
        }

        #[test]
        fn no_url_line_without_a_url() {
            let detail = detail_of(read("a.pdf", complete("AMUNDI-EN24"), None));
            assert!(
                !detail.iter().any(|l| l.starts_with("  url:")),
                "{detail:#?}"
            );
        }

        #[test]
        fn sources_agree() {
            let detail = detail_of(read(
                "a.pdf",
                complete("MEDIOLANUM-IT24.A"),
                url("https://x.example/a.pdf", Some("MEDIOLANUM")),
            ));
            has(&detail, "  sources: agree");
        }

        #[test]
        fn sources_algorithm_only() {
            let detail = detail_of(read("a.pdf", complete("AMUNDI-EN24"), None));
            has(&detail, "  sources: algorithm only");
        }

        #[test]
        fn sources_algorithm_only_when_the_url_is_not_mapped() {
            let detail = detail_of(read(
                "a.pdf",
                complete("AMUNDI-EN24"),
                url("https://x.example/a.pdf", None),
            ));
            has(&detail, "  sources: algorithm only");
        }

        #[test]
        fn sources_url_only() {
            let detail = detail_of(read(
                "a.pdf",
                DocClassification::Unclassified,
                url("https://x.example/a.pdf", Some("EURIZON-EN23")),
            ));
            has(&detail, "  sources: url only");
        }

        #[test]
        fn sources_disagree() {
            let detail = detail_of(read(
                "a.pdf",
                complete("AMUNDI-EN24"),
                url("https://x.example/a.pdf", Some("EURIZON-EN23")),
            ));
            has(&detail, "  sources: disagree, the algorithm wins");
        }
    }

    /// The `format:` line: the combination, only with more than one document.
    mod format_line {
        use super::*;
        use pretty_assertions::assert_eq;

        fn combined(algorithm: Option<&str>, url: Option<&str>) -> Option<SourcesFormat> {
            Some(resolve_sources(
                algorithm.map(p).as_ref(),
                url.map(p).as_ref(),
                &formats(),
            ))
        }

        #[test]
        fn two_documents_are_followed_by_the_format_of_the_set() {
            let mut report = report(vec![
                read("a.pdf", partial("MEDIOLANUM"), None),
                read("b.pdf", complete("MEDIOLANUM-IT24.A"), None),
            ]);
            report.combined = combined(Some("MEDIOLANUM-IT24.A"), None);
            assert_eq!(
                lines(&render(&report, Verbosity::Warn)),
                vec![
                    "a.pdf: partial MEDIOLANUM — candidates: MEDIOLANUM-EN23, MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.B",
                    "b.pdf: MEDIOLANUM-IT24.A",
                    "format: MEDIOLANUM-IT24.A",
                ]
            );
        }

        #[test]
        fn the_format_line_has_the_same_forms_partial_included() {
            let mut report = report(vec![
                read("a.pdf", partial("MEDIOLANUM-IT24.*"), None),
                read("b.pdf", DocClassification::Unclassified, None),
            ]);
            report.combined = combined(Some("MEDIOLANUM-IT24.*"), None);
            let out = render(&report, Verbosity::Warn);
            assert_eq!(
                lines(&out).last().copied(),
                Some(
                    "format: partial MEDIOLANUM-IT24.* — candidates: MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.B"
                )
            );
        }

        #[test]
        fn a_set_nothing_classifies_is_not_classified() {
            let mut report = report(vec![
                read("a.pdf", DocClassification::Unclassified, None),
                read("b.pdf", DocClassification::Unclassified, None),
            ]);
            report.combined = combined(None, None);
            let out = render(&report, Verbosity::Warn);
            assert_eq!(lines(&out).last().copied(), Some("format: not classified"));
        }

        #[test]
        fn a_single_document_has_no_format_line() {
            let mut report = report(vec![read("a.pdf", complete("AMUNDI-EN24"), None)]);
            report.combined = combined(Some("AMUNDI-EN24"), None);
            for verbosity in Verbosity::ORDER {
                let out = render(&report, verbosity);
                assert!(
                    !out.lines().any(|l| l.starts_with("format:")),
                    "{verbosity:?}: {out}"
                );
            }
        }

        /// Documents disagreeing with each other: no `format:` line (the error goes to stderr from
        /// `main`).
        #[test]
        fn documents_disagreeing_have_no_format_line() {
            let report = report(vec![
                read("a.pdf", complete("AMUNDI-EN24"), None),
                read("b.pdf", complete("EURIZON-EN23"), None),
            ]);
            let out = render(&report, Verbosity::Warn);
            assert_eq!(
                lines(&out),
                vec!["a.pdf: AMUNDI-EN24", "b.pdf: EURIZON-EN23"]
            );
        }

        #[test]
        fn the_format_line_is_there_at_every_verbosity() {
            let mut report = report(vec![
                read("a.pdf", complete("AMUNDI-EN24"), None),
                read("b.pdf", complete("AMUNDI-EN24"), None),
            ]);
            report.combined = combined(Some("AMUNDI-EN24"), None);
            for verbosity in Verbosity::ORDER {
                let out = render(&report, verbosity);
                assert!(
                    out.lines().any(|l| l == "format: AMUNDI-EN24"),
                    "{verbosity:?}: {out}"
                );
            }
        }
    }
}
