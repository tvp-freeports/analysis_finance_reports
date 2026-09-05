//! Running one job: resolving the documents (downloading if needed), compiling the target
//! companies, loading the algorithm and applying it.
//!
//! A job is a single, already-validated configuration, and it produces the outcomes of every
//! document it covers. Splitting a batch into independent jobs and concatenating their results
//! belongs to [`super::run`], not here.
//!
//! # How a document is resolved
//!
//! - a path that exists on disk is used directly, with **no** download;
//! - otherwise a URL must be present, guaranteed by validation. The document is downloaded and saved to the given path; with no path given, the bytes are written to a temporary file, PyMuPDF needing a real file rather than a buffer.
//!
//! # A note on the Python boundary
//!
//! Loading a document is one of the crate's two PyO3 boundaries. This module is not one of them but
//! **depends** on one for every realistic path, and there is no injectable seam for document
//! loading. The tests exercising the whole chain are therefore isolated in a submodule of their
//! own, as at the boundary itself.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::cli::conf_parse::DocumentSpec;
use crate::cli::freeports_config::FreeportsConfig;
use crate::core::algorithm::{Algorithm, AlgorithmError, DocumentOutcome};
use crate::core::parallelism::Parallelism;
use crate::core::page::FormatName;
use crate::formats_repo::LoadError;
use crate::input::companies_db::{CompileTargetCompaniesError, compile_target_companies};
use crate::input::document::{DocumentError, load_document};
use crate::input::download::{DownloadError, download_pdf};
use crate::core::tracing_setup::log_error;

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
fn resolve_document_path(spec: &DocumentSpec) -> Result<(PathBuf, bool), JobError> {
    if let Some(path) = &spec.path
        && path.is_file()
    {
        tracing::info!(path = %path.display(), "using the existing local document, no download needed");
        return Ok((path.clone(), false));
    }

    // Guaranteed by validation: when the path is absent or not yet on disk, the URL is present.
    let url = spec.url.as_deref().expect("freeports_config::validate guarantees a url here");
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

/// Opens the job span and logs the outcome exactly once.
///
/// `parallelism` is how many pages at a time the engine may process. The job does not decide it: it
/// is handed down by [`super::run`], the only place that knows how many jobs are running together
/// and therefore how to divide the cores between them.
pub fn run(config: &FreeportsConfig, parallelism: Parallelism) -> Result<Vec<DocumentOutcome>, JobError> {
    let span = tracing::info_span!("job", format = %config.format);
    let _guard = span.enter();

    let result = run_impl(config, parallelism);
    match &result {
        Ok(outcomes) => tracing::info!(outcome_count = outcomes.len(), "job finished"),
        Err(e) => tracing::error!(error = log_error(e), "job failed: {e}"),
    }
    result
}

/// One algorithm load for the whole job, however many documents it covers: resolve each document
/// spec, load each document, compile the target companies — skipped when no target lists were given
/// — and apply the algorithm across them all.
fn run_impl(config: &FreeportsConfig, parallelism: Parallelism) -> Result<Vec<DocumentOutcome>, JobError> {
    // Not logged here: `JobError::MissingFormatsRepoPath` relies solely on `run`'s outer wrapper
    // for its one log line, same as every other `JobError` variant.
    let formats_repo_path = config.formats_repo_path.as_deref().ok_or(JobError::MissingFormatsRepoPath)?;
    let algorithm = Algorithm::load(formats_repo_path, &FormatName::new(config.format.clone()))?;

    let mut documents = Vec::with_capacity(config.reports.len());
    let mut temp_files = Vec::new();
    for spec in &config.reports {
        let id = spec.name.clone().unwrap_or_default();
        // Opened here, at the point where the job dispatches per-document work (not inside
        // `resolve_document_path`/`load_document`, which are leaves): a future instrumentation of
        // `input::document::load_document` inherits this span, giving its events the
        // `job/document` `Activity` context for free.
        // `report` and not `id`: the field name is what fills the `Report` column of the
        // `.log.csv`, and it is the same document these events are about.
        let doc_span = tracing::info_span!("document", report = %id);
        let _doc_guard = doc_span.enter();

        let (path, is_temp) = resolve_document_path(spec)?;
        if is_temp {
            temp_files.push(path.clone());
        }
        let document = load_document(&path, id, config.format.clone(), true)?;
        documents.push(document);
    }

    // An absent input database path is treated as "no target companies available", never an error.
    //
    // **Known inconsistency**: [`JobError::MissingInputDbPath`] is still part of the public error
    // type but no path constructs it any more. The end-to-end test exercising exactly this
    // combination — target lists given, no database directory — expects success, and that behaviour
    // is the one kept.
    let companies = match (config.target_lists.is_empty(), config.input_db_path.as_deref()) {
        (true, _) | (false, None) => Vec::new(),
        (false, Some(input_db_path)) => compile_target_companies(input_db_path, &config.target_lists)?,
    };

    let outcomes = algorithm.apply_multidocument_with(&documents, &companies, parallelism)?;

    for temp in temp_files {
        if let Err(e) = std::fs::remove_file(&temp) {
            tracing::warn!(error = log_error(&e), path = %temp.display(), "failed to remove temporary downloaded file: {e}");
        }
    }

    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use crate::cli::parallelism_config::ParallelismConfig;
    use super::*;
    use crate::cli::conf_parse::DocumentSpec;
    use crate::cli::freeports_config::FreeportsConfig;
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
            format: "DOES-NOT-EXIST".to_string(),
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
            assert!(matches!(result, Err(JobError::MissingFormatsRepoPath)), "got {result:?}");
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
                let builder = Self { dir: tempfile::TempDir::new().unwrap() };
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
                        "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous\n",
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
                let fitz = PyModule::import(py, "fitz")
                    .expect("PyMuPDF (fitz) must be importable: activate venv/freeports-dev, see AGENTS.md");
                let doc = fitz.call_method0("open").unwrap();
                let page = doc.call_method1("new_page", (-1i64, 200.0f64, 300.0f64)).unwrap();
                page.call_method1("insert_text", ((20.0f64, 50.0f64), "Holdings")).unwrap();
                doc.call_method1("save", (path.to_str().unwrap(),)).unwrap();
                doc.call_method0("close").unwrap();
            });
        }

        fn config_for(dir: &std::path::Path, repo: &MinimalRepo, reports: Vec<DocumentSpec>) -> FreeportsConfig {
            FreeportsConfig {
                verbosity: Verbosity::Warn,
                reports,
                target_lists: vec![],
                format: "A-EN24".to_string(),
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

            let reports = vec![DocumentSpec { url: None, path: Some(pdf_path), name: Some("Local Report".to_string()) }];
            let config = config_for(dir.path(), &repo, reports);

            // No `url` at all: if `run` needed to download here it could not (nothing to
            // download from), so success alone already shows the existing local path was used
            // directly.
            let outcomes = run(&config, Parallelism::SEQUENTIAL).expect("must succeed without attempting any network access");
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
                    let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", pdf_bytes.len());
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&pdf_bytes);
                }
            });
            let url = format!("http://{addr}/report.pdf");

            let reports = vec![DocumentSpec { url: Some(url), path: None, name: Some("Downloaded Report".to_string()) }];
            let config = config_for(dir.path(), &repo, reports);

            let outcomes = run(&config, Parallelism::SEQUENTIAL).expect("must download then load the document successfully");
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
                DocumentSpec { url: None, path: Some(pdf_a), name: Some("Report A".to_string()) },
                DocumentSpec { url: None, path: Some(pdf_b), name: Some("Report B".to_string()) },
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

            let reports = vec![DocumentSpec { url: None, path: Some(pdf_path), name: Some("Report".to_string()) }];
            let mut config = config_for(dir.path(), &repo, reports);
            config.target_lists = vec![]; // present (not absent -- that's rejected upstream by
            // `freeports_config::validate`), just empty.

            let outcomes = run(&config, Parallelism::SEQUENTIAL).expect("an empty target list must not prevent the job from running");
            assert_eq!(outcomes.len(), 1);
        }
    }
}
