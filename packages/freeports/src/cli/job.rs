//! Running one job: resolving the documents (downloading if needed), settling the format against
//! them, compiling the target companies, loading the algorithm and applying it.
//!
//! A job is a single, already-validated configuration, and it produces the outcomes of every
//! document it covers. Splitting a batch into independent jobs and concatenating their results
//! belongs to [`super::run`], not here.
//!
//! # Two stages
//!
//! 1. **Settling the format** (`prepare`): resolve each document, open it lazily and classify it
//!    with the repository's document classification, combine what the documents say, and resolve
//!    the format the job runs — configuration > algorithm > url
//!    ([`resolve_job_format`]). The classification runs even with a configured format: that is
//!    what lets the run warn when the two disagree.
//! 2. **The job proper** (`run_prepared`): load the algorithm of that format, load each document
//!    in full, apply. The pages read to classify are converted a second time here: wanted, one way
//!    into a document is simpler than sharing pages between the two stages.
//!
//! Each stage has its own `job` span, siblings: a span's label is fixed when it is created, and
//! the format is only known after stage 1. Stage 1 is `job[<configured>]`, or a bare `job` with
//! no configured format; stage 2 is `job[<resolved>]`. With a configured format every span path
//! is therefore the one a single span would give.
//!
//! # How a document is resolved
//!
//! - a path that exists on disk is used directly, with **no** download;
//! - otherwise a URL must be present, guaranteed by validation. The document is downloaded and saved to the given path; with no path given, the bytes are written to a temporary file, PyMuPDF needing a real file rather than a buffer. The temporary files are removed on every exit of the job, failures included (`TempFiles`).
//!
//! # A note on the Python boundary
//!
//! Loading a document is one of the crate's two PyO3 boundaries. This module is not one of them but
//! **depends** on one for every realistic path, and there is no injectable seam for document
//! loading. The tests exercising the whole chain are therefore isolated in a submodule of their
//! own, as at the boundary itself.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::cli::classify::label;
use crate::cli::conf_parse::DocumentSpec;
use crate::cli::freeports_config::FreeportsConfig;
use crate::core::algorithm::{Algorithm, AlgorithmError, DocumentOutcome};
use crate::core::doc_classify::{DocClassification, DocClassifyError, DocumentClassifier};
use crate::core::page::{Document, FormatName};
use crate::core::parallelism::Parallelism;
use crate::core::tracing_setup::log_error;
use crate::formats_repo::LoadError;
use crate::formats_repo::doc_classify::{self, DocClassifyLoadError};
use crate::formats_repo::doc_classify_sources::{
    ConflictingFormats, DocumentSources, DocumentSourcesError, JobFormatError, combine_documents,
    document_sources, resolve_job_format, resolve_sources, url_source,
};
use crate::input::companies_db::{CompileTargetCompaniesError, compile_target_companies};
use crate::input::document::{DocumentError, load_document};
use crate::input::download::{DownloadError, download_pdf};

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error(transparent)]
    AlgorithmLoad(#[from] LoadError),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Apply(#[from] AlgorithmError),
    #[error(transparent)]
    CompileTargetCompanies(#[from] CompileTargetCompaniesError),
    /// Target lists were given but there is no input database to read the companies from.
    #[error("target_lists is not empty, but no input_db_path was configured")]
    MissingInputDbPath,
    /// No formats repository path: there is nothing to load the algorithm from.
    #[error("no formats_repo_path was configured")]
    MissingFormatsRepoPath,
    /// Every document of the job failed to resolve or to load.
    ///
    /// A document that cannot be read costs that document and no other — see [`run_prepared`] —
    /// but when there is no document left, there is nothing smaller than the job that contains the
    /// failure, and the job is where it belongs. The alternative would be a job reported as
    /// successful with zero results, indistinguishable from one that read everything and found
    /// nothing.
    #[error("none of the {count} documents of this job could be read")]
    NoDocumentReadable { count: usize },
    /// The repository's document classification would not load, and no format was configured.
    #[error(transparent)]
    ClassifierLoad(DocClassifyLoadError),
    /// A document's classification failed at run time, and no format was configured (D5).
    #[error("document {report} could not be classified: {source}")]
    Classification {
        report: String,
        #[source]
        source: Box<DocClassifyError>,
    },
    /// The documents of the job disagree with each other on one source.
    #[error(transparent)]
    ConflictingFormats(#[from] ConflictingFormats),
    /// No configured format, and the documents do not settle a complete one. Boxed: the pattern of
    /// a partial would otherwise make every `Result<_, JobError>` large.
    #[error(transparent)]
    Format(Box<JobFormatError>),
}

/// A unique path under the system's temporary directory for a downloaded document whose spec named
/// no path — needed only because PyMuPDF requires a real file on disk.
fn temp_download_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("freeports-download-{}-{n}.pdf", std::process::id()))
}

/// Resolves a document spec to a local path ready to be loaded, downloading only if needed.
///
/// Also reports whether the path was created here, and so is to be cleaned up afterwards, rather
/// than coming from the spec.
pub(crate) fn resolve_document_path(spec: &DocumentSpec) -> Result<(PathBuf, bool), JobError> {
    if let Some(path) = &spec.path
        && path.is_file()
    {
        tracing::info!(path = %path.display(), "using the existing local document, no download needed");
        return Ok((path.clone(), false));
    }

    // Guaranteed by validation: when the path is absent or not yet on disk, the URL is present.
    let url = spec
        .url
        .as_deref()
        .expect("freeports_config::validate guarantees a url here");
    match &spec.path {
        Some(path) => {
            tracing::info!(url, path = %path.display(), "downloading document");
            download_pdf(url, Some(path))?;
            Ok((path.clone(), false))
        }
        None => {
            let temp_path = temp_download_path();
            tracing::info!(url, path = %temp_path.display(), "downloading document to a temporary file");
            download_pdf(url, Some(&temp_path))?;
            Ok((temp_path, true))
        }
    }
}

/// The temporary downloads of a job, removed when it is dropped: on **every** exit of the job,
/// since stage 1 can fail after the downloads (classification, format, documents disagreeing).
#[derive(Default)]
struct TempFiles(Vec<PathBuf>);

impl TempFiles {
    fn push(&mut self, path: PathBuf) {
        self.0.push(path);
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        for temp in self.0.drain(..) {
            if let Err(e) = std::fs::remove_file(&temp) {
                tracing::warn!(error = log_error(&e), path = %temp.display(), "failed to remove temporary downloaded file: {e}");
            }
        }
    }
}

/// Runs the two stages, each in its own `job` span, and logs the outcome exactly once.
///
/// `parallelism` is how many pages at a time the engine may process. The job does not decide it: it
/// is handed down by [`super::run`], the only place that knows how many jobs are running together
/// and therefore how to divide the cores between them.
pub fn run(
    config: &FreeportsConfig,
    parallelism: Parallelism,
) -> Result<Vec<DocumentOutcome>, JobError> {
    let mut temp_files = TempFiles::default();
    let stage1 = match &config.format {
        Some(format) => tracing::info_span!("job", format = %format),
        None => tracing::info_span!("job"),
    };
    let prepared = match stage1.in_scope(|| prepare(config, &mut temp_files)) {
        Ok(prepared) => prepared,
        Err(e) => {
            // Dropped inside the span: a failed removal is logged under the job, like its other
            // events.
            stage1.in_scope(|| {
                drop(temp_files);
                log_job_failure(&e);
            });
            return Err(e);
        }
    };

    let stage2 = tracing::info_span!("job", format = %prepared.format);
    stage2.in_scope(|| {
        let result = run_prepared(config, prepared, parallelism);
        drop(temp_files);
        match &result {
            Ok(outcomes) => tracing::info!(outcome_count = outcomes.len(), "job finished"),
            Err(e) => log_job_failure(e),
        }
        result
    })
}

fn log_job_failure(e: &JobError) {
    tracing::error!(error = log_error(e), "job failed: {e}");
}

/// A document stage 1 could read, for stage 2 to load in full.
struct ReadDocument {
    id: String,
    path: PathBuf,
}

/// What stage 1 hands to stage 2.
struct Prepared<'a> {
    formats_repo_path: &'a Path,
    format: String,
    documents: Vec<ReadDocument>,
    /// The documents stage 1 left out, for the one `skipped_documents` warning of the job.
    skipped: usize,
}

/// Stage 1: the documents, their classification, and the format the job runs.
///
/// # What costs what
///
/// A document that will not resolve or will not open costs **that document**, as in stage 2. A
/// classification that fails — the repository's (it will not load) or a document's (at run time) —
/// counts as nothing when a format is configured, with an `ERROR`; without one there is nothing
/// else to take the format from, and the job fails (D5). Documents disagreeing with each other fail
/// the job either way.
fn prepare<'a>(
    config: &'a FreeportsConfig,
    temp_files: &mut TempFiles,
) -> Result<Prepared<'a>, JobError> {
    // Not logged here: `JobError::MissingFormatsRepoPath` relies solely on `run`'s outer wrapper
    // for its one log line, same as every other `JobError` variant.
    let formats_repo_path = config
        .formats_repo_path
        .as_deref()
        .ok_or(JobError::MissingFormatsRepoPath)?;
    let configured = config.format.as_deref();
    let classifier = match doc_classify::load(formats_repo_path) {
        Ok(classifier) => Some(classifier),
        Err(e) if configured.is_some() => {
            tracing::error!(
                error = log_error(&e),
                "the document classification of this repository could not be loaded, the configured format is used unchecked: {e}"
            );
            None
        }
        Err(e) => return Err(JobError::ClassifierLoad(e)),
    };

    let mut documents = Vec::with_capacity(config.reports.len());
    let mut sources = Vec::with_capacity(config.reports.len());
    let mut skipped = 0usize;
    for spec in &config.reports {
        let id = spec.name.clone().unwrap_or_default();
        // `report` and not `id`: the field name is what fills the `Report` column of the
        // `.log.csv`, and it is the same document these events are about.
        let doc_span = tracing::info_span!("document", report = %id);
        let _doc_guard = doc_span.enter();
        match read_document(
            spec,
            id,
            classifier.as_ref(),
            formats_repo_path,
            configured.is_some(),
            temp_files,
        )? {
            Some((document, document_sources)) => {
                documents.push(document);
                sources.extend(document_sources.map(|s| (label(spec), s)));
            }
            None => skipped += 1,
        }
    }
    if documents.is_empty() && !config.reports.is_empty() {
        warn_skipped(skipped);
        return Err(JobError::NoDocumentReadable {
            count: config.reports.len(),
        });
    }

    let formats = classifier
        .as_ref()
        .map(DocumentClassifier::formats)
        .unwrap_or_default();
    let combined = match &classifier {
        Some(_) => combine_documents(&sources, formats)?,
        None => resolve_sources(None, None, formats),
    };
    let format = resolve_job_format(configured, &combined, formats)
        .map_err(|e| JobError::Format(Box::new(e)))?
        .format;
    Ok(Prepared {
        formats_repo_path,
        format,
        documents,
        skipped,
    })
}

/// One document of stage 1: resolved (downloaded if needed) and, with a classifier, classified —
/// its sources are `None` only without one. `Ok(None)`: the document is skipped, and logged so.
fn read_document(
    spec: &DocumentSpec,
    id: String,
    classifier: Option<&DocumentClassifier>,
    formats_repo_path: &Path,
    format_configured: bool,
    temp_files: &mut TempFiles,
) -> Result<Option<(ReadDocument, Option<DocumentSources>)>, JobError> {
    let path = match resolve_document_path(spec) {
        Ok((path, is_temp)) => {
            if is_temp {
                temp_files.push(path.clone());
            }
            path
        }
        Err(error) => {
            log_skipped(&error);
            return Ok(None);
        }
    };
    let Some(classifier) = classifier else {
        return Ok(Some((ReadDocument { id, path }, None)));
    };
    let sources = match document_sources(classifier, formats_repo_path, &path, spec.url.as_deref())
    {
        Ok(sources) => sources,
        Err(DocumentSourcesError::Open(error)) => {
            log_skipped(&JobError::Document(error));
            return Ok(None);
        }
        Err(DocumentSourcesError::Classify(error)) if format_configured => {
            tracing::error!(
                error = log_error(&error),
                "the document could not be classified, its classification counts as nothing: {error}"
            );
            // D5: the classification counts as nothing, the url still counts.
            DocumentSources {
                algorithm: DocClassification::Unclassified,
                url: spec
                    .url
                    .as_deref()
                    .map(|url| url_source(formats_repo_path, classifier.formats(), url)),
            }
        }
        Err(DocumentSourcesError::Classify(error)) => {
            return Err(JobError::Classification {
                report: id,
                source: Box::new(error),
            });
        }
    };
    Ok(Some((ReadDocument { id, path }, Some(sources))))
}

/// A document that will not resolve or will not open costs **that document**, not the job: the
/// others of a multi-document job are readable and there is no reason they should not be read. The
/// enclosing `document` span already carries the document, so it fills the `Report` column of the
/// `.log.csv` without the event having to name it.
fn log_skipped(error: &JobError) {
    tracing::error!(error = log_error(error), "document skipped: {error}");
}

/// One event per job, not per document: each skipped document already said so where it happened.
/// Same shape and same reason as the `skipped` pages of `apply_multidocument_with` — whoever
/// watches only stderr must not finish a job believing everything was read.
fn warn_skipped(skipped_documents: usize) {
    if skipped_documents > 0 {
        tracing::warn!(
            skipped_documents,
            "some documents could not be read and were left out of this job's results"
        );
    }
}

/// Stage 2: one algorithm load for the whole job, however many documents it covers — load each
/// document stage 1 read, compile the target companies — skipped when no target lists were given —
/// and apply the algorithm across them all.
///
/// # What costs what
///
/// A document that will not load costs **that document**: it is logged, counted and left out, and
/// the job goes on with the others. Everything else here is configuration rather than data — an
/// algorithm that will not load, target companies that will not compile — and belongs to the job,
/// because no document contains it. A job in which *no* document could be read, in either stage,
/// is [`JobError::NoDocumentReadable`], for the reason given there.
fn run_prepared(
    config: &FreeportsConfig,
    prepared: Prepared<'_>,
    parallelism: Parallelism,
) -> Result<Vec<DocumentOutcome>, JobError> {
    let Prepared {
        formats_repo_path,
        format,
        documents: read,
        mut skipped,
    } = prepared;
    let algorithm = Algorithm::load(formats_repo_path, &FormatName::new(format.clone()))?;

    let mut documents: Vec<Document> = Vec::with_capacity(read.len());
    for ReadDocument { id, path } in read {
        // Opened here, at the point where the job dispatches per-document work (not inside
        // `load_document`, which is a leaf): a future instrumentation of
        // `input::document::load_document` inherits this span, giving its events the
        // `job/document` `Activity` context for free.
        let doc_span = tracing::info_span!("document", report = %id);
        let _doc_guard = doc_span.enter();
        match load_document(&path, id, format.clone(), true) {
            Ok(document) => documents.push(document),
            Err(error) => {
                log_skipped(&JobError::Document(error));
                skipped += 1;
            }
        }
    }
    warn_skipped(skipped);
    // Not a job that succeeded with nothing in it: see `JobError::NoDocumentReadable`.
    if documents.is_empty() && !config.reports.is_empty() {
        return Err(JobError::NoDocumentReadable {
            count: config.reports.len(),
        });
    }

    // An absent input database path is treated as "no target companies available", never an error.
    //
    // **Known inconsistency**: [`JobError::MissingInputDbPath`] is still part of the public error
    // type but no path constructs it any more. The end-to-end test exercising exactly this
    // combination — target lists given, no database directory — expects success, and that behaviour
    // is the one kept.
    let companies = match (
        config.target_lists.is_empty(),
        config.input_db_path.as_deref(),
    ) {
        (true, _) | (false, None) => Vec::new(),
        (false, Some(input_db_path)) => {
            compile_target_companies(input_db_path, &config.target_lists)?
        }
    };

    Ok(algorithm.apply_multidocument_with(&documents, &companies, parallelism)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::conf_parse::DocumentSpec;
    use crate::cli::freeports_config::FreeportsConfig;
    use crate::cli::parallelism_config::ParallelismConfig;
    use crate::core::tracing_setup::Verbosity;
    use crate::output::routines::write::{OutFlags, OutStructureMode};

    /// A valid baseline configuration: no documents, and a format that does not exist. Used only by
    /// the tests checking that a load failure propagates, which never reach a document. The tests
    /// that really run a job build their own configuration with a real repository and real
    /// documents.
    fn base_config(dir: &std::path::Path) -> FreeportsConfig {
        FreeportsConfig {
            verbosity: Verbosity::Warn,
            reports: vec![],
            target_lists: vec![],
            format: Some("DOES-NOT-EXIST".to_string()),
            out_path: dir.to_path_buf(),
            out_profile: OutStructureMode::Regular,
            out_flags: OutFlags::default(),
            parallelism: ParallelismConfig::SEQUENTIAL,
            batch_file: None,
            save_pdf: true,
            formats_repo_path: Some(dir.join("formats_repo_that_does_not_exist")),
            input_db_path: None,
            config_file: None,
        }
    }

    mod algorithm_load_failure {
        use super::*;

        /// Touches no Python: the algorithm load fails before any document is even considered, so
        /// this path does not need document loading at all.
        #[test]
        fn an_unknown_formats_repo_is_a_typed_job_error_not_a_panic() {
            let dir = tempfile::tempdir().unwrap();
            let config = base_config(dir.path());
            let result = std::panic::catch_unwind(|| run(&config, Parallelism::SEQUENTIAL));
            assert!(result.is_ok(), "must not panic");
            assert!(matches!(result.unwrap(), Err(JobError::AlgorithmLoad(_))));
        }

        #[test]
        fn a_missing_formats_repo_path_is_a_typed_error() {
            let dir = tempfile::tempdir().unwrap();
            let mut config = base_config(dir.path());
            config.formats_repo_path = None;
            let result = run(&config, Parallelism::SEQUENTIAL);
            assert!(
                matches!(result, Err(JobError::MissingFormatsRepoPath)),
                "got {result:?}"
            );
        }
    }

    /// These tests build a real PDF and a real minimal formats repository and run a job end to end.
    /// See the module documentation for why this dependency on Python is unavoidable here.
    mod python_boundary {
        use super::*;
        use pyo3::prelude::*;

        /// A minimal formats repository with a single page-classify pipeline that classifies every
        /// page alike and extracts nothing — enough to run the whole algorithm end to end without
        /// depending on the PDF's content.
        struct MinimalRepo {
            dir: tempfile::TempDir,
        }

        impl MinimalRepo {
            fn build() -> Self {
                let builder = Self {
                    dir: tempfile::TempDir::new().unwrap(),
                };
                builder
                    .write("metadata/formats.csv", "Name,Locale,Year,Country,Version\nA,EN,24,,\n")
                    .write("metadata/url_mapping.csv", "Format name,Url\nA-EN24,https://example.com/a\n")
                    .write(
                        "content/orchestration/algorithms_schedule.csv",
                        "Format name,Page type,Filter next iteration\nA-EN24,investments,\n",
                    )
                    .write("content/orchestration/mapping.csv", "ID,Page type\nA-EN24(investments),investments\n")
                    .write("content/orchestration/pageclassify_overwrite.csv", "ID\n")
                    .write(
                        "content/algorithms/structured/page_classify/args.csv",
                        "ID,Header set,Class\nA-EN24/0,\"Arial \"\"^.*$\"\"\",investments\n",
                    )
                    .write(
                        "content/algorithms/structured/investments/args.csv",
                        "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n\
                         A-EN24,Arial,Arial,Arial,1,,,,\n",
                    )
                    .write(
                        "content/algorithms/structured/investments/additional_args.csv",
                        "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero\n",
                    )
                    .write("content/algorithms/structured/investments/partial_pipes.csv", "ID,pdf_extract,text_filter,deserialize\n")
                    .write("content/algorithms/structured/investments/deselection_lists.csv", "ID,Deselection set\n")
                    .write("content/algorithms/semistructured/formats_mapping.csv", "ID,pdf_extract,text_filter,deserialize\n")
                    .write("content/algorithms/semistructured/args/pdf_extract.yaml", "{}")
                    .write("content/algorithms/semistructured/args/text_filter.yaml", "{}")
                    .write("content/algorithms/semistructured/args/deserialize.yaml", "{}");
                builder
            }

            fn write(&self, relative: &str, content: &str) -> &Self {
                let path = self.dir.path().join(relative);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, content).unwrap();
                self
            }

            fn path(&self) -> std::path::PathBuf {
                self.dir.path().to_path_buf()
            }
        }

        /// Builds a minimal PDF with text the fixture's classifier recognises.
        fn build_pdf(path: &std::path::Path) {
            Python::attach(|py| {
                let fitz = PyModule::import(py, "fitz").expect(
                    "PyMuPDF (fitz) must be importable: activate venv/freeports-dev, see AGENTS.md",
                );
                let doc = fitz.call_method0("open").unwrap();
                let page = doc
                    .call_method1("new_page", (-1i64, 200.0f64, 300.0f64))
                    .unwrap();
                page.call_method1("insert_text", ((20.0f64, 50.0f64), "Holdings"))
                    .unwrap();
                doc.call_method1("save", (path.to_str().unwrap(),)).unwrap();
                doc.call_method0("close").unwrap();
            });
        }

        fn config_for(
            dir: &std::path::Path,
            repo: &MinimalRepo,
            reports: Vec<DocumentSpec>,
        ) -> FreeportsConfig {
            FreeportsConfig {
                verbosity: Verbosity::Warn,
                reports,
                target_lists: vec![],
                format: Some("A-EN24".to_string()),
                out_path: dir.to_path_buf(),
                out_profile: OutStructureMode::Regular,
                out_flags: OutFlags::default(),
                parallelism: ParallelismConfig::SEQUENTIAL,
                batch_file: None,
                save_pdf: true,
                formats_repo_path: Some(repo.path()),
                input_db_path: None,
                config_file: None,
            }
        }

        #[test]
        fn a_document_with_an_existing_local_path_never_triggers_a_download() {
            let dir = tempfile::tempdir().unwrap();
            let repo = MinimalRepo::build();
            let pdf_path = dir.path().join("local.pdf");
            build_pdf(&pdf_path);

            let reports = vec![DocumentSpec {
                url: None,
                path: Some(pdf_path),
                name: Some("Local Report".to_string()),
            }];
            let config = config_for(dir.path(), &repo, reports);

            // No `url` at all: if `run` needed to download here it could not (nothing to
            // download from), so success alone already shows the existing local path was used
            // directly.
            let outcomes = run(&config, Parallelism::SEQUENTIAL)
                .expect("must succeed without attempting any network access");
            assert_eq!(outcomes.len(), 1);
            assert_eq!(outcomes[0].id.as_str(), "Local Report");
        }

        #[test]
        fn a_document_with_only_a_url_downloads_before_loading() {
            use std::io::{Read, Write};
            use std::net::TcpListener;

            let dir = tempfile::tempdir().unwrap();
            let repo = MinimalRepo::build();

            // Serve a real, tiny valid PDF over HTTP so `input::download` + `input::document`
            // (both exercised for real here) have something loadable to work with.
            let pdf_path = dir.path().join("to_serve.pdf");
            build_pdf(&pdf_path);
            let pdf_bytes = std::fs::read(&pdf_path).unwrap();

            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            std::thread::spawn(move || {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf);
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        pdf_bytes.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&pdf_bytes);
                }
            });
            let url = format!("http://{addr}/report.pdf");

            let reports = vec![DocumentSpec {
                url: Some(url),
                path: None,
                name: Some("Downloaded Report".to_string()),
            }];
            let config = config_for(dir.path(), &repo, reports);

            let outcomes = run(&config, Parallelism::SEQUENTIAL)
                .expect("must download then load the document successfully");
            assert_eq!(outcomes.len(), 1);
            assert_eq!(outcomes[0].id.as_str(), "Downloaded Report");
        }

        #[test]
        fn multiple_documents_share_a_single_algorithm_and_propagate_their_own_ids() {
            let dir = tempfile::tempdir().unwrap();
            let repo = MinimalRepo::build();
            let pdf_a = dir.path().join("a.pdf");
            let pdf_b = dir.path().join("b.pdf");
            build_pdf(&pdf_a);
            build_pdf(&pdf_b);

            let reports = vec![
                DocumentSpec {
                    url: None,
                    path: Some(pdf_a),
                    name: Some("Report A".to_string()),
                },
                DocumentSpec {
                    url: None,
                    path: Some(pdf_b),
                    name: Some("Report B".to_string()),
                },
            ];
            let config = config_for(dir.path(), &repo, reports);

            let outcomes = run(&config, Parallelism::SEQUENTIAL).unwrap();
            assert_eq!(outcomes.len(), 2);
            let ids: Vec<&str> = outcomes.iter().map(|o| o.id.as_str()).collect();
            assert_eq!(ids, vec!["Report A", "Report B"]);
        }

        #[test]
        fn an_empty_but_present_target_lists_still_runs_and_finds_no_investments() {
            let dir = tempfile::tempdir().unwrap();
            let repo = MinimalRepo::build();
            let pdf_path = dir.path().join("local.pdf");
            build_pdf(&pdf_path);

            let reports = vec![DocumentSpec {
                url: None,
                path: Some(pdf_path),
                name: Some("Report".to_string()),
            }];
            let mut config = config_for(dir.path(), &repo, reports);
            config.target_lists = vec![]; // present (not absent -- that's rejected upstream by
            // `freeports_config::validate`), just empty.

            let outcomes = run(&config, Parallelism::SEQUENTIAL)
                .expect("an empty target list must not prevent the job from running");
            assert_eq!(outcomes.len(), 1);
        }

        /// A document that will not open costs that document, not the job. Before, the `?` on the
        /// load meant a single unreadable file in a multi-document job threw away every other
        /// document of that job as well — documents that were perfectly readable and had already
        /// been named in the configuration.
        mod document_containment {
            use super::*;
            use pretty_assertions::assert_eq;

            /// A file that exists and is not a PDF: the shape PyMuPDF refuses, which is what a
            /// truncated download or a mislabelled file really looks like.
            fn unreadable(path: &std::path::Path) -> std::path::PathBuf {
                std::fs::write(path, b"this is not a PDF at all").unwrap();
                path.to_path_buf()
            }

            #[test]
            fn an_unreadable_document_does_not_cost_the_readable_ones() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let good = dir.path().join("good.pdf");
                build_pdf(&good);
                let bad = unreadable(&dir.path().join("bad.pdf"));

                let reports = vec![
                    DocumentSpec {
                        url: None,
                        path: Some(bad),
                        name: Some("Broken".to_string()),
                    },
                    DocumentSpec {
                        url: None,
                        path: Some(good),
                        name: Some("Sound".to_string()),
                    },
                ];
                let config = config_for(dir.path(), &repo, reports);

                let outcomes = run(&config, Parallelism::SEQUENTIAL)
                    .expect("one unreadable document must not fail the job");
                assert_eq!(
                    outcomes.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
                    vec!["Sound"]
                );
            }

            /// The order is the configuration's, whichever document fails: the sound one is kept
            /// where it was, not moved to the front by the removal of the other.
            #[test]
            fn the_surviving_documents_keep_their_order() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let first = dir.path().join("first.pdf");
                let last = dir.path().join("last.pdf");
                build_pdf(&first);
                build_pdf(&last);
                let bad = unreadable(&dir.path().join("middle.pdf"));

                let reports = vec![
                    DocumentSpec {
                        url: None,
                        path: Some(first),
                        name: Some("First".to_string()),
                    },
                    DocumentSpec {
                        url: None,
                        path: Some(bad),
                        name: Some("Middle".to_string()),
                    },
                    DocumentSpec {
                        url: None,
                        path: Some(last),
                        name: Some("Last".to_string()),
                    },
                ];
                let config = config_for(dir.path(), &repo, reports);

                let outcomes = run(&config, Parallelism::SEQUENTIAL).expect("contained");
                assert_eq!(
                    outcomes.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
                    vec!["First", "Last"]
                );
            }

            /// A download that fails is the same kind of loss as a file that will not open, and is
            /// contained the same way. The port is closed, so the request cannot succeed.
            #[test]
            fn a_document_that_cannot_be_downloaded_is_skipped_like_one_that_cannot_be_opened() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let good = dir.path().join("good.pdf");
                build_pdf(&good);

                // Bound and dropped: the port is known to be free, so nothing answers on it.
                let closed = {
                    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                    listener.local_addr().unwrap()
                };
                let reports = vec![
                    DocumentSpec {
                        url: Some(format!("http://{closed}/missing.pdf")),
                        path: Some(dir.path().join("never_arrives.pdf")),
                        name: Some("Unreachable".to_string()),
                    },
                    DocumentSpec {
                        url: None,
                        path: Some(good),
                        name: Some("Sound".to_string()),
                    },
                ];
                let config = config_for(dir.path(), &repo, reports);

                let outcomes = run(&config, Parallelism::SEQUENTIAL).expect("contained");
                assert_eq!(
                    outcomes.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
                    vec!["Sound"]
                );
            }

            /// And the floor: a job in which nothing could be read is a **failed job**, not a
            /// successful one with zero results. Nothing smaller than the job contains that, and
            /// the run level above takes it from here.
            #[test]
            fn a_job_in_which_no_document_can_be_read_is_a_failed_job() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let reports = vec![
                    DocumentSpec {
                        url: None,
                        path: Some(unreadable(&dir.path().join("a.pdf"))),
                        name: Some("A".to_string()),
                    },
                    DocumentSpec {
                        url: None,
                        path: Some(unreadable(&dir.path().join("b.pdf"))),
                        name: Some("B".to_string()),
                    },
                ];
                let config = config_for(dir.path(), &repo, reports);

                match run(&config, Parallelism::SEQUENTIAL) {
                    Err(JobError::NoDocumentReadable { count }) => assert_eq!(count, 2),
                    other => panic!("expected NoDocumentReadable, found {other:?}"),
                }
            }

            /// A job that names no document at all is not the same thing, and stays what it was: a
            /// run of nothing, which the configuration layer allows and which produces nothing.
            #[test]
            fn a_job_with_no_documents_at_all_is_still_not_a_failure() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let config = config_for(dir.path(), &repo, vec![]);
                assert!(
                    run(&config, Parallelism::SEQUENTIAL)
                        .expect("no documents is not a failure")
                        .is_empty()
                );
            }
        }

        // ======================================================================================
        // Phase 11: the document classification inside the job
        // (`agent-memory/DESIGN-phase11-run-integration.md` §3, §4, §8.C)
        // ======================================================================================

        /// The phase-11 additions to [`MinimalRepo`]: more formats in `formats.csv` (only `A-EN24`
        /// has an algorithm), the author's rules, the URL mapping.
        impl MinimalRepo {
            /// `formats.csv` with exactly these complete names (`<ENTITY>-<LL><YY>` only).
            fn with_formats(self, names: &[&str]) -> Self {
                let mut csv = String::from("Name,Locale,Year,Country,Version\n");
                for name in names {
                    let (entity, rest) = name.split_once('-').expect("ENTITY-LLYY");
                    let (locale, year) = rest.split_at(2);
                    csv.push_str(&format!("{entity},{locale},{year},,\n"));
                }
                self.write("metadata/formats.csv", &csv);
                self
            }

            fn with_yaml(self, text: &str) -> Self {
                self.write("metadata/document_classify.yaml", text);
                self
            }

            fn with_python(self, text: &str) -> Self {
                self.write("metadata/document_classify.py", text);
                self
            }

            fn with_url_mapping(self, text: &str) -> Self {
                self.write("metadata/url_mapping.csv", text);
                self
            }

            /// Breaks `A-EN24`'s algorithm, and only it: `Algorithm::load` fails, the document
            /// classification still loads.
            fn with_broken_algorithm(self) -> Self {
                std::fs::remove_file(
                    self.dir
                        .path()
                        .join("content/orchestration/algorithms_schedule.csv"),
                )
                .unwrap();
                self
            }
        }

        /// `A-EN24` and `B-EN24`; only `A-EN24` can run.
        const TWO_FORMATS: &[&str] = &["A-EN24", "B-EN24"];

        /// A document whose first line is `Holdings` is `A-EN24`, `Other` is `B-EN24`.
        const YAML: &str = "1:\n  A-EN24: {text: '^Holdings$'}\n  B-EN24: {text: '^Other$'}\n";

        /// `https://example.com/a…` is `A-EN24`, `https://example.com/b…` is `B-EN24`.
        const URL_MAPPING: &str =
            "Format name,Url\nA-EN24,https://example.com/a\nB-EN24,https://example.com/b\n";

        /// One pipeline giving `A-EN24` for a `Conflict A` line and `B-EN24` for `Conflict B`: a
        /// page carrying both is a runtime classification error (D5). Elsewhere it says nothing.
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
    return {'Conflict A': 'A-EN24', 'Conflict B': 'B-EN24'}.get(block.content)

document_classifier = [(1, P(extract, keep, deserialize))]
"#;

        /// The repository most phase-11 tests use: two formats, the YAML, the URL mapping.
        fn classifying_repo() -> MinimalRepo {
            MinimalRepo::build()
                .with_formats(TWO_FORMATS)
                .with_yaml(YAML)
                .with_url_mapping(URL_MAPPING)
        }

        /// A one-page PDF, one line of text per entry of `lines`.
        fn pdf_with(path: &std::path::Path, lines: &[&str]) -> std::path::PathBuf {
            Python::attach(|py| {
                let fitz = PyModule::import(py, "fitz").expect(
                    "PyMuPDF (fitz) must be importable: activate venv/freeports-dev, see AGENTS.md",
                );
                let doc = fitz.call_method0("open").unwrap();
                let page = doc
                    .call_method1("new_page", (-1i64, 200.0f64, 300.0f64))
                    .unwrap();
                for (i, line) in lines.iter().enumerate() {
                    let y = 50.0f64 + 20.0 * i as f64;
                    page.call_method1("insert_text", ((20.0f64, y), *line))
                        .unwrap();
                }
                doc.call_method1("save", (path.to_str().unwrap(),)).unwrap();
                doc.call_method0("close").unwrap();
            });
            path.to_path_buf()
        }

        /// A local document named `name`, with no URL.
        fn local(path: std::path::PathBuf, name: &str) -> DocumentSpec {
            DocumentSpec {
                url: None,
                path: Some(path),
                name: Some(name.to_string()),
            }
        }

        /// A local document named `name` that also carries a URL: the file exists, so nothing is
        /// downloaded, and the URL is still a source of the format.
        fn local_with_url(path: std::path::PathBuf, url: &str, name: &str) -> DocumentSpec {
            DocumentSpec {
                url: Some(url.to_string()),
                path: Some(path),
                name: Some(name.to_string()),
            }
        }

        fn not_a_pdf(path: &std::path::Path) -> std::path::PathBuf {
            std::fs::write(path, b"this is not a PDF at all").unwrap();
            path.to_path_buf()
        }

        /// [`run`] with every event it emits, under the engine tests' `SERIAL` lock (see it in
        /// `core::doc_classify`'s tests). The PDFs are built **before**: the lock is never taken
        /// while attached to Python.
        fn run_captured(
            config: &FreeportsConfig,
        ) -> (
            Result<Vec<DocumentOutcome>, JobError>,
            Vec<crate::core::tracing_setup::capture::Record>,
        ) {
            let _guard = crate::core::doc_classify::tests::SERIAL
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut result = None;
            let records = crate::core::tracing_setup::capture::events(|| {
                tracing::callsite::rebuild_interest_cache();
                result = Some(run(config, Parallelism::SEQUENTIAL));
            });
            (result.expect("the job ran"), records)
        }

        fn at(
            records: &[crate::core::tracing_setup::capture::Record],
            level: tracing::Level,
        ) -> Vec<&crate::core::tracing_setup::capture::Record> {
            records.iter().filter(|r| r.level == Some(level)).collect()
        }

        /// The [`JobFormatError`] of a job that failed with [`JobError::Format`], or a panic. Written
        /// so that it holds whether the variant carries the error by value or boxed (clippy's
        /// `result_large_err` may ask for the box): `JobFormatError::clone(&e)` reads through both.
        fn format_error_of(result: Result<Vec<DocumentOutcome>, JobError>) -> JobFormatError {
            match result {
                Err(JobError::Format(e)) => JobFormatError::clone(&e),
                other => panic!("expected JobError::Format, got {other:?}"),
            }
        }

        fn formats_of(outcomes: &[DocumentOutcome]) -> Vec<&str> {
            outcomes.iter().map(|o| o.format.as_str()).collect()
        }

        /// C1: a configured format and a repository with no rules (neither YAML nor Python) is the
        /// job of today: same outcomes, nothing at `WARN` or above. The tests above stay as they
        /// were, but for `Some(..)` in their configuration.
        mod explicit_format_without_rules {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn the_job_runs_as_today_and_says_nothing_at_warn_or_above() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);

                let (result, records) = run_captured(&config);
                let outcomes = result.expect("the job of today");
                assert_eq!(formats_of(&outcomes), vec!["A-EN24"]);
                assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
                assert!(
                    at(&records, tracing::Level::ERROR).is_empty(),
                    "{records:?}"
                );
            }

            /// The URL mapping of `MinimalRepo` sends `https://example.com/a` to `A-EN24`: the
            /// configured format agrees, no warning.
            #[test]
            fn a_mapped_url_agreeing_with_the_configured_format_is_no_warning() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let config = config_for(
                    dir.path(),
                    &repo,
                    vec![local_with_url(pdf, "https://example.com/a/r.pdf", "Report")],
                );
                let (result, records) = run_captured(&config);
                assert_eq!(result.unwrap().len(), 1);
                assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
            }
        }

        /// C2–C4: no configured format, and the documents give it.
        mod detected_from_the_documents {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn a_document_the_yaml_classifies_runs_with_that_format() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let mut config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);
                config.format = None;

                let (result, records) = run_captured(&config);
                let outcomes = result.expect("the format comes from the document");
                assert_eq!(formats_of(&outcomes), vec!["A-EN24"]);
                assert_eq!(outcomes[0].id.as_str(), "Report");
                assert!(
                    at(&records, tracing::Level::INFO).iter().any(|r| {
                        r.message.contains("format detected")
                            && r.field("by") == Some("algorithm")
                            && r.field("format") == Some("A-EN24")
                    }),
                    "{records:?}"
                );
            }

            #[test]
            fn without_rules_a_mapped_url_gives_the_format() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let mut config = config_for(
                    dir.path(),
                    &repo,
                    vec![local_with_url(pdf, "https://example.com/a/r.pdf", "Report")],
                );
                config.format = None;

                let (result, records) = run_captured(&config);
                assert_eq!(formats_of(&result.expect("by the url")), vec!["A-EN24"]);
                assert!(
                    at(&records, tracing::Level::INFO).iter().any(|r| {
                        r.message.contains("format detected") && r.field("by") == Some("url")
                    }),
                    "{records:?}"
                );
            }

            /// Crossing 1: one `ERROR`, and the algorithm wins.
            #[test]
            fn the_algorithm_and_the_url_disagreeing_is_one_error_and_the_algorithm_wins() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let mut config = config_for(
                    dir.path(),
                    &repo,
                    vec![local_with_url(pdf, "https://example.com/b/r.pdf", "Report")],
                );
                config.format = None;

                let (result, records) = run_captured(&config);
                assert_eq!(
                    formats_of(&result.expect("the algorithm wins")),
                    vec!["A-EN24"]
                );
                let errors = at(&records, tracing::Level::ERROR);
                assert_eq!(errors.len(), 1, "{records:?}");
                assert_eq!(errors[0].field("algorithm"), Some("A-EN24"));
                assert_eq!(errors[0].field("url"), Some("B-EN24"));
            }
        }

        /// C5–C6: a configured format the documents contradict. The configured one runs, with a
        /// `WARN` — which proves the classification runs even with an explicit format.
        mod configured_against_the_documents {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn a_document_classified_otherwise_is_a_warning_and_the_configured_format_runs() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let pdf = pdf_with(&dir.path().join("b.pdf"), &["Other"]);
                let config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);

                let (result, records) = run_captured(&config);
                assert_eq!(
                    formats_of(&result.expect("the configured one runs")),
                    vec!["A-EN24"]
                );
                let warnings = at(&records, tracing::Level::WARN);
                assert_eq!(warnings.len(), 1, "{records:?}");
                assert_eq!(warnings[0].field("configured"), Some("A-EN24"));
                assert_eq!(warnings[0].field("algorithm"), Some("B-EN24"));
            }

            #[test]
            fn a_url_mapped_otherwise_is_a_warning_and_the_configured_format_runs() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build()
                    .with_formats(TWO_FORMATS)
                    .with_url_mapping(URL_MAPPING);
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let config = config_for(
                    dir.path(),
                    &repo,
                    vec![local_with_url(pdf, "https://example.com/b/r.pdf", "Report")],
                );

                let (result, records) = run_captured(&config);
                assert_eq!(
                    formats_of(&result.expect("the configured one runs")),
                    vec!["A-EN24"]
                );
                let warnings = at(&records, tracing::Level::WARN);
                assert_eq!(warnings.len(), 1, "{records:?}");
                assert_eq!(warnings[0].field("configured"), Some("A-EN24"));
                assert_eq!(warnings[0].field("url"), Some("B-EN24"));
            }

            #[test]
            fn a_document_confirming_the_configured_format_is_no_warning() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);

                let (result, records) = run_captured(&config);
                assert_eq!(result.unwrap().len(), 1);
                assert!(at(&records, tracing::Level::WARN).is_empty(), "{records:?}");
                assert!(
                    at(&records, tracing::Level::ERROR).is_empty(),
                    "{records:?}"
                );
            }
        }

        /// C7, C8, C15: no configured format and no complete one from the documents — the job
        /// fails with [`JobError::Format`], before any algorithm is loaded.
        mod format_not_settled {
            use super::*;
            use crate::formats_repo::doc_classify_sources::JobFormatError;
            use pretty_assertions::assert_eq;

            #[test]
            fn nothing_from_any_source_is_an_undetermined_format() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let mut config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);
                config.format = None;

                let (result, _) = run_captured(&config);
                assert_eq!(format_error_of(result), JobFormatError::Undetermined);
            }

            /// The order of the job: the format is settled before `Algorithm::load`, so a broken
            /// algorithm is never reached.
            #[test]
            fn the_algorithm_is_not_loaded_before_the_format_is_settled() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build().with_broken_algorithm();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let mut config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);
                config.format = None;

                let (result, _) = run_captured(&config);
                assert_eq!(format_error_of(result), JobFormatError::Undetermined);
            }

            /// The run executes no partial: the error carries every candidate.
            #[test]
            fn a_partial_is_an_error_naming_every_candidate() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build()
                    .with_formats(&["A-EN24", "A-EN25"])
                    .with_yaml("1:\n  A: {text: '^Holdings$'}\n");
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let mut config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);
                config.format = None;

                let (result, _) = run_captured(&config);
                match format_error_of(result) {
                    JobFormatError::Partial { candidates, .. } => {
                        assert_eq!(candidates, vec!["A-EN24".to_string(), "A-EN25".to_string()]);
                    }
                    other => panic!("expected a partial format error, got {other:?}"),
                }
            }

            #[test]
            fn the_message_of_a_partial_lists_both_candidates() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build()
                    .with_formats(&["A-EN24", "A-EN25"])
                    .with_yaml("1:\n  A: {text: '^Holdings$'}\n");
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let mut config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);
                config.format = None;

                let (result, _) = run_captured(&config);
                let message = result.expect_err("a partial is an error").to_string();
                assert!(message.contains("A-EN24"), "{message}");
                assert!(message.contains("A-EN25"), "{message}");
            }

            /// C15: with a configured format a job naming no document is a run of nothing (the
            /// existing `a_job_with_no_documents_at_all_is_still_not_a_failure`); without one there
            /// is nothing to take the format from.
            #[test]
            fn a_job_with_no_documents_and_no_format_is_an_undetermined_format() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let mut config = config_for(dir.path(), &repo, vec![]);
                config.format = None;

                let (result, _) = run_captured(&config);
                assert_eq!(format_error_of(result), JobFormatError::Undetermined);
            }

            /// Logged once, by `job failed: …`.
            #[test]
            fn the_failure_is_logged_once() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let mut config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);
                config.format = None;

                let (_, records) = run_captured(&config);
                let errors = at(&records, tracing::Level::ERROR);
                assert_eq!(errors.len(), 1, "{records:?}");
                assert!(errors[0].message.starts_with("job failed: "), "{records:?}");
            }
        }

        /// C9–C10: the documents of a job must agree with each other, with a configured format
        /// as without one (plan, point 11).
        mod documents_of_one_job {
            use super::*;
            use crate::formats_repo::doc_classify_sources::Source;
            use pretty_assertions::assert_eq;

            fn two_disagreeing(dir: &std::path::Path) -> Vec<DocumentSpec> {
                vec![
                    local(pdf_with(&dir.join("a.pdf"), &["Holdings"]), "Report A"),
                    local(pdf_with(&dir.join("b.pdf"), &["Other"]), "Report B"),
                ]
            }

            fn labels(error: &JobError) -> Vec<String> {
                match error {
                    JobError::ConflictingFormats(conflict) => conflict
                        .found
                        .iter()
                        .map(|(label, _)| label.clone())
                        .collect(),
                    other => panic!("expected ConflictingFormats, got {other:?}"),
                }
            }

            #[test]
            fn two_documents_classified_differently_are_conflicting_formats_without_a_format() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let mut config = config_for(dir.path(), &repo, two_disagreeing(dir.path()));
                config.format = None;

                let (result, _) = run_captured(&config);
                let error = result.expect_err("documents disagreeing");
                match &error {
                    JobError::ConflictingFormats(conflict) => {
                        assert_eq!(conflict.by, Source::Algorithm)
                    }
                    other => panic!("expected ConflictingFormats, got {other:?}"),
                }
                assert_eq!(labels(&error), vec!["Report A", "Report B"]);
            }

            #[test]
            fn two_documents_classified_differently_are_conflicting_formats_with_a_format() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let config = config_for(dir.path(), &repo, two_disagreeing(dir.path()));

                let (result, _) = run_captured(&config);
                let error = result.expect_err("documents disagreeing, even with a format");
                assert_eq!(labels(&error), vec!["Report A", "Report B"]);
            }

            /// User decision of 2026-10-17: the documents are named by their label, and a
            /// document skipped in between does not shift anything.
            #[test]
            fn a_skipped_document_in_between_does_not_change_the_labels() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let reports = vec![
                    local(
                        pdf_with(&dir.path().join("first.pdf"), &["Holdings"]),
                        "First",
                    ),
                    local(not_a_pdf(&dir.path().join("middle.pdf")), "Middle"),
                    local(pdf_with(&dir.path().join("last.pdf"), &["Other"]), "Last"),
                ];
                let mut config = config_for(dir.path(), &repo, reports);
                config.format = None;

                let (result, _) = run_captured(&config);
                let error = result.expect_err("documents disagreeing");
                assert_eq!(labels(&error), vec!["First", "Last"]);
                let message = error.to_string();
                assert!(message.contains("First is A-EN24"), "{message}");
                assert!(message.contains("Last is B-EN24"), "{message}");
                assert!(!message.contains("Middle"), "{message}");
            }

            /// A document nothing classifies is neutral.
            #[test]
            fn one_classified_and_one_unclassified_run_with_the_classified_format() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let reports = vec![
                    local(
                        pdf_with(&dir.path().join("a.pdf"), &["Holdings"]),
                        "Classified",
                    ),
                    local(
                        pdf_with(&dir.path().join("n.pdf"), &["Nothing here"]),
                        "Blank",
                    ),
                ];
                let mut config = config_for(dir.path(), &repo, reports);
                config.format = None;

                let (result, _) = run_captured(&config);
                let outcomes = result.expect("an unclassified document is neutral");
                assert_eq!(formats_of(&outcomes), vec!["A-EN24", "A-EN24"]);
                assert_eq!(
                    outcomes.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
                    vec!["Classified", "Blank"]
                );
            }
        }

        /// C11, D5: a classification that fails at run time.
        mod classification_failing_at_run_time {
            use super::*;
            use pretty_assertions::assert_eq;

            fn conflicting_repo() -> MinimalRepo {
                classifying_repo().with_python(CONFLICTING_PYTHON)
            }

            /// With a configured format the classification counts as nothing: an `ERROR`, and the
            /// job runs.
            #[test]
            fn with_a_format_it_is_an_error_and_the_job_runs() {
                let dir = tempfile::tempdir().unwrap();
                let repo = conflicting_repo();
                let pdf = pdf_with(&dir.path().join("c.pdf"), &["Conflict A", "Conflict B"]);
                let config = config_for(dir.path(), &repo, vec![local(pdf, "Conflicted")]);

                let (result, records) = run_captured(&config);
                let outcomes = result.expect("the configured format runs");
                assert_eq!(formats_of(&outcomes), vec!["A-EN24"]);
                let errors = at(&records, tracing::Level::ERROR);
                assert_eq!(
                    errors
                        .iter()
                        .filter(|e| e.message.contains("could not be classified"))
                        .count(),
                    1,
                    "{records:?}"
                );
                assert!(
                    !errors.iter().any(|e| e.message.starts_with("job failed")),
                    "{records:?}"
                );
            }

            /// D5: the classification counts as nothing, **the url still counts** — here it
            /// contradicts the configured format, so crossing 3 is logged.
            #[test]
            fn with_a_format_the_url_of_that_document_still_counts() {
                let dir = tempfile::tempdir().unwrap();
                let repo = conflicting_repo();
                let pdf = pdf_with(&dir.path().join("c.pdf"), &["Conflict A", "Conflict B"]);
                let config = config_for(
                    dir.path(),
                    &repo,
                    vec![local_with_url(
                        pdf,
                        "https://example.com/b/r.pdf",
                        "Conflicted",
                    )],
                );

                let (result, records) = run_captured(&config);
                assert_eq!(result.unwrap().len(), 1);
                let warnings = at(&records, tracing::Level::WARN);
                assert!(
                    warnings.iter().any(|w| w.field("url") == Some("B-EN24")
                        && w.field("configured") == Some("A-EN24")),
                    "{records:?}"
                );
            }

            #[test]
            fn without_a_format_the_job_fails_naming_the_document() {
                let dir = tempfile::tempdir().unwrap();
                let repo = conflicting_repo();
                let pdf = pdf_with(&dir.path().join("c.pdf"), &["Conflict A", "Conflict B"]);
                let mut config = config_for(dir.path(), &repo, vec![local(pdf, "Conflicted")]);
                config.format = None;

                let (result, _) = run_captured(&config);
                match result {
                    Err(JobError::Classification { report, .. }) => {
                        assert_eq!(report, "Conflicted")
                    }
                    other => panic!("expected a classification error, got {other:?}"),
                }
            }

            #[test]
            fn without_a_format_the_message_names_the_document() {
                let dir = tempfile::tempdir().unwrap();
                let repo = conflicting_repo();
                let pdf = pdf_with(&dir.path().join("c.pdf"), &["Conflict A", "Conflict B"]);
                let mut config = config_for(dir.path(), &repo, vec![local(pdf, "Conflicted")]);
                config.format = None;

                let (result, _) = run_captured(&config);
                let message = result.expect_err("D5").to_string();
                assert!(message.contains("Conflicted"), "{message}");
            }

            /// D5: no fallback on the url, even one that maps.
            #[test]
            fn without_a_format_a_mapped_url_does_not_rescue_the_job() {
                let dir = tempfile::tempdir().unwrap();
                let repo = conflicting_repo();
                let pdf = pdf_with(&dir.path().join("c.pdf"), &["Conflict A", "Conflict B"]);
                let mut config = config_for(
                    dir.path(),
                    &repo,
                    vec![local_with_url(
                        pdf,
                        "https://example.com/a/r.pdf",
                        "Conflicted",
                    )],
                );
                config.format = None;

                let (result, _) = run_captured(&config);
                assert!(
                    matches!(result, Err(JobError::Classification { .. })),
                    "{result:?}"
                );
            }
        }

        /// C12, Q1 (user decision of 2026-10-17): a document classification that will not load.
        mod classifier_that_will_not_load {
            use super::*;
            use pretty_assertions::assert_eq;

            fn broken_repo() -> MinimalRepo {
                MinimalRepo::build().with_yaml("1:\n  - [this is: not, a valid: classification\n")
            }

            #[test]
            fn with_a_format_it_is_an_error_and_the_job_runs() {
                let dir = tempfile::tempdir().unwrap();
                let repo = broken_repo();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);

                let (result, records) = run_captured(&config);
                assert_eq!(
                    formats_of(&result.expect("the configured format runs")),
                    vec!["A-EN24"]
                );
                let errors = at(&records, tracing::Level::ERROR);
                assert_eq!(
                    errors
                        .iter()
                        .filter(|e| e.message.contains("could not be loaded"))
                        .count(),
                    1,
                    "{records:?}"
                );
                assert!(
                    !errors.iter().any(|e| e.message.starts_with("job failed")),
                    "{records:?}"
                );
            }

            #[test]
            fn without_a_format_the_job_fails_with_the_load_error() {
                let dir = tempfile::tempdir().unwrap();
                let repo = broken_repo();
                let pdf = pdf_with(&dir.path().join("a.pdf"), &["Holdings"]);
                let mut config = config_for(dir.path(), &repo, vec![local(pdf, "Report")]);
                config.format = None;

                let (result, _) = run_captured(&config);
                assert!(
                    matches!(result, Err(JobError::ClassifierLoad(_))),
                    "{result:?}"
                );
            }
        }

        /// C13: a document that will not open is skipped **once**, whichever of the two stages
        /// meets it first.
        mod a_document_is_skipped_once {
            use super::*;
            use pretty_assertions::assert_eq;

            fn assert_skipped_once(records: &[crate::core::tracing_setup::capture::Record]) {
                let skipped: Vec<_> = at(records, tracing::Level::ERROR)
                    .into_iter()
                    .filter(|r| r.message.starts_with("document skipped: "))
                    .collect();
                assert_eq!(skipped.len(), 1, "{records:?}");
                let summaries: Vec<_> = at(records, tracing::Level::WARN)
                    .into_iter()
                    .filter(|r| r.field("skipped_documents").is_some())
                    .collect();
                assert_eq!(summaries.len(), 1, "{records:?}");
                assert_eq!(summaries[0].field("skipped_documents"), Some("1"));
            }

            #[test]
            fn with_a_format() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let reports = vec![
                    local(not_a_pdf(&dir.path().join("bad.pdf")), "Broken"),
                    local(pdf_with(&dir.path().join("a.pdf"), &["Holdings"]), "Sound"),
                ];
                let config = config_for(dir.path(), &repo, reports);

                let (result, records) = run_captured(&config);
                assert_eq!(
                    result
                        .unwrap()
                        .iter()
                        .map(|o| o.id.as_str())
                        .collect::<Vec<_>>(),
                    vec!["Sound"]
                );
                assert_skipped_once(&records);
            }

            #[test]
            fn without_a_format() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let reports = vec![
                    local(pdf_with(&dir.path().join("a.pdf"), &["Holdings"]), "Sound"),
                    local(not_a_pdf(&dir.path().join("bad.pdf")), "Broken"),
                ];
                let mut config = config_for(dir.path(), &repo, reports);
                config.format = None;

                let (result, records) = run_captured(&config);
                assert_eq!(
                    result
                        .unwrap()
                        .iter()
                        .map(|o| o.id.as_str())
                        .collect::<Vec<_>>(),
                    vec!["Sound"]
                );
                assert_skipped_once(&records);
            }

            /// Every document unreadable: still `NoDocumentReadable`, counted on the configuration.
            #[test]
            fn every_document_unreadable_without_a_format_is_no_document_readable() {
                let dir = tempfile::tempdir().unwrap();
                let repo = classifying_repo();
                let reports = vec![
                    local(not_a_pdf(&dir.path().join("a.pdf")), "A"),
                    local(not_a_pdf(&dir.path().join("b.pdf")), "B"),
                ];
                let mut config = config_for(dir.path(), &repo, reports);
                config.format = None;

                let (result, _) = run_captured(&config);
                match result {
                    Err(JobError::NoDocumentReadable { count }) => assert_eq!(count, 2),
                    other => panic!("expected NoDocumentReadable, got {other:?}"),
                }
            }
        }

        /// C14, Q3 (user decision of 2026-10-17): the temporary downloads are removed on **every**
        /// exit of the job, failures included.
        mod temporary_downloads {
            use super::*;
            use std::io::{Read, Write};
            use std::net::TcpListener;

            /// Serves `bytes` to the first request, at `http://<addr>/report.pdf`.
            fn serve_once(bytes: Vec<u8>) -> String {
                let listener = TcpListener::bind("127.0.0.1:0").unwrap();
                let addr = listener.local_addr().unwrap();
                std::thread::spawn(move || {
                    if let Ok((mut stream, _)) = listener.accept() {
                        let mut buf = [0u8; 1024];
                        let _ = stream.read(&mut buf);
                        let header =
                            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", bytes.len());
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&bytes);
                    }
                });
                format!("http://{addr}/report.pdf")
            }

            /// A PDF whose bytes no other test serves: its text carries a fresh nonce.
            fn unique_pdf_bytes(dir: &std::path::Path) -> Vec<u8> {
                let nonce = format!(
                    "Unclassifiable {} {:?}",
                    std::process::id(),
                    std::time::SystemTime::now()
                );
                let path = pdf_with(&dir.join("served.pdf"), &[&nonce]);
                std::fs::read(path).unwrap()
            }

            /// Every temporary download of this process still on disk with exactly these bytes.
            fn leftovers_of(bytes: &[u8]) -> Vec<std::path::PathBuf> {
                let prefix = format!("freeports-download-{}-", std::process::id());
                std::fs::read_dir(std::env::temp_dir())
                    .unwrap()
                    .filter_map(|entry| entry.ok().map(|e| e.path()))
                    .filter(|path| {
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with(&prefix))
                    })
                    .filter(|path| std::fs::read(path).is_ok_and(|content| content == bytes))
                    .collect()
            }

            fn url_only(url: String, name: &str) -> DocumentSpec {
                DocumentSpec {
                    url: Some(url),
                    path: None,
                    name: Some(name.to_string()),
                }
            }

            /// A job that fails on its format, after the download: the temporary file is gone.
            #[test]
            fn a_job_failing_on_an_undetermined_format_leaves_no_temporary_download() {
                use crate::formats_repo::doc_classify_sources::JobFormatError;

                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let bytes = unique_pdf_bytes(dir.path());
                let url = serve_once(bytes.clone());
                // Not mapped: `MinimalRepo` maps only `https://example.com/a`.
                let mut config = config_for(dir.path(), &repo, vec![url_only(url, "Downloaded")]);
                config.format = None;

                let (result, _) = run_captured(&config);
                assert_eq!(format_error_of(result), JobFormatError::Undetermined);
                assert_eq!(leftovers_of(&bytes), Vec::<std::path::PathBuf>::new());
            }

            /// A configured format with no algorithm: the job fails on `Algorithm::load`, and
            /// whatever it downloaded before is gone too.
            #[test]
            fn a_job_failing_on_its_algorithm_leaves_no_temporary_download() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build().with_formats(TWO_FORMATS);
                let bytes = unique_pdf_bytes(dir.path());
                let url = serve_once(bytes.clone());
                let mut config = config_for(dir.path(), &repo, vec![url_only(url, "Downloaded")]);
                config.format = Some("B-EN24".to_string());

                let (result, _) = run_captured(&config);
                assert!(
                    matches!(result, Err(JobError::AlgorithmLoad(_))),
                    "{result:?}"
                );
                assert_eq!(leftovers_of(&bytes), Vec::<std::path::PathBuf>::new());
            }

            /// And the success path, as today.
            #[test]
            fn a_job_that_succeeds_leaves_no_temporary_download() {
                let dir = tempfile::tempdir().unwrap();
                let repo = MinimalRepo::build();
                let bytes = unique_pdf_bytes(dir.path());
                let url = serve_once(bytes.clone());
                let config = config_for(dir.path(), &repo, vec![url_only(url, "Downloaded")]);

                let (result, _) = run_captured(&config);
                assert_eq!(result.unwrap().len(), 1);
                assert_eq!(leftovers_of(&bytes), Vec::<std::path::PathBuf>::new());
            }
        }
    }
}
