//! Esecuzione di un singolo job: risoluzione dei documenti (download se serve), risoluzione delle
//! target companies, `Algorithm::load` + `apply_multidocument`.
//!
//! `M9-implementation-plan.md` §1/§3 passo 11. **Non è 1:1 con `_main_job` del riferimento**: qui
//! prende un singolo `FreeportsConfig` già completamente risolto/validato e produce
//! `Result<Vec<DocumentOutcome>, JobError>`. La suddivisione "batch = N job indipendenti, i cui
//! risultati si concatenano" resta quella del riferimento, ma vive in `cli::run`, non qui.
//!
//! **Nota sul confine Python**: `input::document::load_document` (M6) è uno dei due moduli di
//! confine PyO3 del crate (`PLAN.md` §2 principio 1, §10 D13: "i test unitari non toccano Python,
//! salvo i due moduli di confine"). `cli::job` non è uno di quei due moduli, ma **dipende**
//! direttamente da uno di essi per ogni percorso realistico ("apre un documento e ci applica
//! l'algoritmo") -- non esiste un seam iniettabile (nessun trait/mock per il caricamento
//! documento in questo crate). I test che esercitano l'intera catena sono perciò isolati in un
//! `mod python_boundary` qui sotto, stesso trattamento di `input/document.rs::tests::
//! python_boundary` -- una deviazione necessaria dalla lettera di D13, non una scorciatoia,
//! segnalata esplicitamente nel resoconto del test-writer.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, thiserror::Error)]
//! pub enum JobError {
//!     AlgorithmLoad(#[from] crate::formats_repo::LoadError),
//!     Download(#[from] crate::input::download::DownloadError),
//!     Document(#[from] crate::input::document::DocumentError),
//!     Apply(#[from] crate::core::algorithm::AlgorithmError),
//!     CompileTargetCompanies(#[from] crate::input::companies_db::CompileTargetCompaniesError),
//!     /// `target_lists` non è vuota ma `input_db_path` non è impostato -- non c'è dove leggere
//!     /// le aziende bersaglio.
//!     MissingInputDbPath,
//!     /// `formats_repo_path` non è impostato -- non c'è repo da cui caricare l'algoritmo.
//!     MissingFormatsRepoPath,
//! }
//!
//! /// Un solo `Algorithm::load` per l'intero job (anche con più documenti): risolve ogni
//! /// `DocumentSpec` in `config.reports` (scaricando via `input::download` solo se manca un path
//! /// su disco), carica ciascun `Document` con `input::document::load_document` (id = `name` dello
//! /// spec, già garantito `Some` dopo `freeports_config::validate`), compila le target companies
//! /// (skip se `target_lists` è vuota: nessuna azienda bersaglio, nessuna lettura di
//! /// `input_db_path`), poi `Algorithm::apply_multidocument`.
//! pub fn run(config: &crate::cli::freeports_config::FreeportsConfig, parallelism: crate::core::parallelism::Parallelism) -> Result<Vec<crate::core::algorithm::DocumentOutcome>, JobError>;
//! ```
//!
//! # Regola di risoluzione di un documento
//!
//! - `path` presente e il file esiste su disco -> usato direttamente, **nessuna** chiamata a
//!   `input::download`.
//! - `path` assente, oppure presente ma il file non esiste ancora -> `url` deve essere presente
//!   (garantito da `freeports_config::validate`); scarica con `input::download::download_pdf`,
//!   salva a `path` se presente (altrimenti tiene solo i byte in memoria e li scrive in un file
//!   temporaneo prima di aprirlo con PyMuPDF, che richiede un path reale).

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
    /// `target_lists` non è vuota ma `input_db_path` non è impostato -- non c'è dove leggere le
    /// aziende bersaglio.
    #[error("target_lists is not empty, but no input_db_path was configured")]
    MissingInputDbPath,
    /// `formats_repo_path` non è impostato -- non c'è repo da cui caricare l'algoritmo.
    #[error("no formats_repo_path was configured")]
    MissingFormatsRepoPath,
}

/// Genera un path univoco sotto la directory temporanea di sistema per un documento scaricato di
/// cui lo spec non indicava un path -- serve solo perché PyMuPDF richiede un file reale su disco,
/// non un buffer in memoria.
fn temp_download_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("freeports-download-{}-{n}.pdf", std::process::id()))
}

/// Risolve un `DocumentSpec` a un path locale pronto per `input::document::load_document`,
/// scaricando solo se serve. Ritorna anche se il path è stato creato qui (per essere ripulito
/// dopo l'uso) invece di venire dallo spec originale.
fn resolve_document_path(spec: &DocumentSpec) -> Result<(PathBuf, bool), JobError> {
    if let Some(path) = &spec.path
        && path.is_file()
    {
        tracing::info!(path = %path.display(), "using the existing local document, no download needed");
        return Ok((path.clone(), false));
    }

    // Garantito da `freeports_config::validate` (`input_should_be_specified` + `pdf_path_validation`):
    // se il path è assente o non ancora su disco, l'url è presente.
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

/// Job dispatch: opens the `job` span (`format` is the closest thing to a job identity this
/// module has -- a job is a single, already-validated `FreeportsConfig`) around
/// [`run_impl`] and logs the outcome exactly once.
///
/// `parallelism` e' quante pagine alla volta il motore puo' elaborare (P2). Il job non lo decide:
/// glielo passa `cli::run`, che e' l'unico posto che sa quanti job stanno girando insieme e quindi
/// come dividere i core fra loro.
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

/// Un solo `Algorithm::load` per l'intero job (anche con più documenti): risolve ogni
/// `DocumentSpec` in `config.reports` (scaricando via `input::download` solo se manca un path su
/// disco), carica ciascun `Document` con `input::document::load_document` (id = `name` dello
/// spec, già garantito `Some` dopo `freeports_config::validate`), compila le target companies
/// (skip se `target_lists` è vuota: nessuna azienda bersaglio, nessuna lettura di
/// `input_db_path`), poi `Algorithm::apply_multidocument`.
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
        let doc_span = tracing::info_span!("document", id = %id);
        let _doc_guard = doc_span.enter();

        let (path, is_temp) = resolve_document_path(spec)?;
        if is_temp {
            temp_files.push(path.clone());
        }
        let document = load_document(&path, id, config.format.clone(), true)?;
        documents.push(document);
    }

    // Nota di implementazione: il doc-comment del contratto descriveva `MissingInputDbPath` come
    // scattante ogni volta che `target_lists` non è vuota ma `input_db_path` è assente. Il test
    // end-to-end `cli::run::tests::python_boundary::
    // a_full_non_batch_invocation_writes_the_regular_profile_csvs_to_disk` esercita esattamente
    // questa combinazione (`--target-list TEST` senza `--db-directory`) e si aspetta successo, non
    // un errore -- i test vincono sul commento del contratto (vedi il doc-comment del modulo). Un
    // `input_db_path` assente è quindi trattato come "nessuna azienda bersaglio disponibile"
    // (lista vuota), mai un errore; `JobError::MissingInputDbPath` resta nell'enum per la forma
    // pubblica del contratto ma non è più costruito da nessun percorso -- segnalato nel resoconto
    // finale come contraddizione fra il commento e il test eseguibile.
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

    /// Una `FreeportsConfig` valida di base: nessun documento, formato inesistente (usato solo
    /// dai test che verificano la propagazione dell'errore di `Algorithm::load`, che non arrivano
    /// mai a toccare un documento). I test che eseguono davvero il job (`mod python_boundary`)
    /// costruiscono la propria configurazione con un repo formati/documenti reali.
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

        /// Non tocca Python: `Algorithm::load` fallisce prima che qualunque documento venga
        /// anche solo considerato (repo formati inesistente), quindi questo percorso non richiede
        /// affatto `input::document`.
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

    /// I test qui sotto costruiscono un vero PDF (via `fitz`, stessa tecnica di
    /// `input/document.rs::tests::python_boundary`) e un vero repo formati minimale, ed
    /// eseguono `job::run` end-to-end -- vedi il doc-comment del modulo sul perché questa
    /// dipendenza da Python è necessaria qui, non evitabile.
    mod python_boundary {
        use super::*;
        use pyo3::prelude::*;

        /// Un repo formati minimo con una sola pipeline (page-classify), che classifica ogni
        /// pagina come `"any"` e non estrae nulla -- sufficiente a far girare
        /// `Algorithm::apply_multidocument` end-to-end senza dipendere dal contenuto del PDF.
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

        /// Costruisce un PDF minimo con `fitz` a `path`, con del testo che il classificatore
        /// della fixture riconosce (font `Arial`, qualunque contenuto: il pattern è `^.*$`).
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
