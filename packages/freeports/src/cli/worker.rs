//! The protocol between a parent process and a child running **one** job.
//!
//! In batch mode the jobs may run in child processes instead of a sequential loop. This module
//! defines *what they exchange*, and nothing else: it starts no processes and runs no jobs.
//!
//! # Why two files and not a pipe
//!
//! A child's standard output is not a clean channel: the PDF library and author-written pipes may
//! write to it whenever they like. A dedicated file per direction has no such problem, no size
//! limit, and survives the child long enough to be read after it has exited.
//!
//! # The two directions
//!
//! - **outbound** ([`WorkerRequest`]): the job's **already resolved and validated** configuration, plus the paths the child is to use. The child does not redo the resolution: doing so would read the environment and the configuration files again, and could resolve something other than what the parent decided;
//! - **inbound** ([`WorkerReport`]): the job's results, or its error.
//!
//! # A failed job is not a failed child
//!
//! Two distinct planes, and confusing them would make "the PDF does not exist" indistinguishable
//! from "the child died of a signal". A job failing for a domain reason produces a failed
//! **report** and the child exits with **code 0**: the error is *in the payload*. A non-zero exit
//! stays reserved for protocol failures, which the parent recognises by the report file being
//! absent or unreadable.
//!
//! # What is lost crossing the boundary
//!
//! A domain error reaches the parent as a record — its debug form, its display form, and its chain
//! of causes — not as a typed error: an error enum cannot be rebuilt from a string. It is enough
//! for the message on stderr to be **identical** to the sequential case's, which uses the display
//! form alone. The full diagnosis is not lost either: the child has already recorded it in its own
//! log files, which the parent merges into its own.

use std::path::{Path, PathBuf};

use crate::cli::freeports_config::FreeportsConfig;
use crate::core::algorithm::DocumentOutcome;
use crate::core::tracing_setup::ErrorRecord;

/// What the parent asks of a child: one job, and the two places to put its outcome.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkerRequest {
    /// The configuration the parent has already resolved and validated.
    pub config: FreeportsConfig,
    /// Where the child writes its report.
    pub report_path: PathBuf,
    /// The **private** directory the child writes its logs in. Never the parent's output directory:
    /// a child's files must not appear beside the run's results.
    pub log_dir: PathBuf,
    /// How many pages at a time the child may process.
    ///
    /// Decided by the parent, not the child: it is the only one of the two that knows how many jobs
    /// are running together, and so the only one that can stop N children from each opening as many
    /// threads as there are cores. It travels in the request rather than in an environment variable
    /// for the same reason the configuration does — the child re-derives nothing the parent has
    /// already decided.
    pub page_workers: usize,
}

/// Cosa il figlio rimanda al padre.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum WorkerReport {
    /// The job succeeded. The list may be empty: a job that extracts nothing is a legitimate
    /// outcome, not an error.
    Succeeded { documents: Vec<DocumentOutcome> },
    /// The job failed for a domain reason.
    Failed { error: ErrorRecord },
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("cannot write the worker request to {}: {source}", path.display())]
    WriteRequest {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot read the worker request at {}: {source}", path.display())]
    ReadRequest {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the worker request at {} is malformed: {source}", path.display())]
    ParseRequest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("cannot write the worker report to {}: {source}", path.display())]
    WriteReport {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot read the worker report at {}: {source}", path.display())]
    ReadReport {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the worker report at {} is malformed: {source}", path.display())]
    ParseReport {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("cannot create the worker work area at {}: {source}", path.display())]
    WorkArea {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot start a worker process for job {index}: {source}")]
    Spawn {
        index: usize,
        #[source]
        source: std::io::Error,
    },
    /// The child exited non-zero or died of a signal. A job that failed for a domain reason does
    /// **not** come through here: it exits with 0 and puts the error in its report.
    #[error("the worker process for job {index} {status} without leaving a report")]
    Died { index: usize, status: String },
    /// The child could not open its log files in the private directory assigned to it. A protocol
    /// failure, not a domain one: without logs the child would run the job in silence and its
    /// diagnostics would never reach the run's merged log.
    #[error(transparent)]
    Logging(#[from] crate::core::tracing_setup::TracingSetupError),
}

/// Serialises a request. The JSON is compact: nobody reads it by hand, and on a large batch it is
/// one more file per job to write.
pub fn write_request(path: &Path, request: &WorkerRequest) -> Result<(), WorkerError> {
    let json = serde_json::to_vec(request).map_err(|e| WorkerError::WriteRequest {
        path: path.to_path_buf(),
        source: std::io::Error::other(e),
    })?;
    std::fs::write(path, json).map_err(|e| WorkerError::WriteRequest { path: path.to_path_buf(), source: e })
}

/// Reads a request back. The two ways of failing are kept apart — file missing against file
/// unreadable as JSON — because they point at different bugs: the first a path problem or a
/// temporary directory cleaned too early, the second a parent and child built from different
/// versions of the binary.
pub fn read_request(path: &Path) -> Result<WorkerRequest, WorkerError> {
    let bytes = std::fs::read(path).map_err(|e| WorkerError::ReadRequest { path: path.to_path_buf(), source: e })?;
    serde_json::from_slice(&bytes).map_err(|e| WorkerError::ParseRequest { path: path.to_path_buf(), source: e })
}

/// Serializza `report` in `path`.
pub fn write_report(path: &Path, report: &WorkerReport) -> Result<(), WorkerError> {
    let json = serde_json::to_vec(report).map_err(|e| WorkerError::WriteReport {
        path: path.to_path_buf(),
        source: std::io::Error::other(e),
    })?;
    std::fs::write(path, json).map_err(|e| WorkerError::WriteReport { path: path.to_path_buf(), source: e })
}

/// Reads a report back. A missing file here means the child never got as far as writing it — died
/// of a signal, or exited early: the protocol failure the parent tells apart from a failed job.
pub fn read_report(path: &Path) -> Result<WorkerReport, WorkerError> {
    let bytes = std::fs::read(path).map_err(|e| WorkerError::ReadReport { path: path.to_path_buf(), source: e })?;
    serde_json::from_slice(&bytes).map_err(|e| WorkerError::ParseReport { path: path.to_path_buf(), source: e })
}

/// How a job can produce no results.
///
/// The two forms are kept apart to the very end because they have different causes and different
/// readers: the first is a problem with the data or the configuration, and its message is
/// **identical** to the one the sequential case would have printed; the second is a failure of the
/// process machinery, and concerns whoever develops the engine.
#[derive(Debug, thiserror::Error)]
pub enum JobFailure {
    /// The job failed inside the child, for a domain reason. The message is the original error's
    /// display form, verbatim: whoever reads stderr must not be able to tell the job went through
    /// another process.
    #[error("{}", error.display)]
    Job { index: usize, error: ErrorRecord },
    /// The parent-child protocol broke: no readable report came back. The message names the job,
    /// because unlike a domain error the user has no other context from which to tell which batch
    /// row went wrong.
    #[error("job {index} could not be run in a worker process: {source}")]
    Protocol {
        index: usize,
        #[source]
        source: WorkerError,
    },
}

impl JobFailure {
    /// The job's position in the batch, in both forms: what the ordering is done on when choosing
    /// which failure to report.
    pub fn index(&self) -> usize {
        match self {
            JobFailure::Job { index, .. } | JobFailure::Protocol { index, .. } => *index,
        }
    }
}

/// The private work area of a run in child processes: requests, reports, and the children's logs.
///
/// Under the system temporary directory, never in the working directory and never in the output
/// directory — the same rule as for any other run artefact, and all the more so with N children.
///
/// The deletion is in `Drop` rather than at the end of a function: the area must disappear even
/// when the run exits with an error, which is exactly the case where it is easiest to forget.
#[derive(Debug)]
pub struct WorkArea {
    path: PathBuf,
}

impl WorkArea {
    pub fn create() -> Result<Self, WorkerError> {
        let path = std::env::temp_dir().join(format!("freeports-jobs-{}", std::process::id()));
        std::fs::create_dir_all(&path).map_err(|source| WorkerError::WorkArea { path: path.clone(), source })?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkArea {
    fn drop(&mut self) {
        // Best-effort: a failed deletion leaves files in the system temporary directory, which is
        // unpleasant but not a reason to fail a run that has already produced its results.
        if let Err(e) = std::fs::remove_dir_all(&self.path) {
            tracing::debug!(path = %self.path.display(), "could not remove the worker work area: {e}");
        }
    }
}

/// Prepares one job's private directory and the request describing it.
///
/// One level per job rather than files mixed in a single directory: the children's logs all have
/// the same fixed names and would otherwise overwrite each other.
pub fn prepare_request(
    work_dir: &Path,
    index: usize,
    config: &FreeportsConfig,
    page_workers: usize,
) -> Result<WorkerRequest, WorkerError> {
    let job_dir = work_dir.join(format!("job-{index}"));
    let log_dir = job_dir.join("logs");
    std::fs::create_dir_all(&log_dir).map_err(|source| WorkerError::WriteRequest { path: log_dir.clone(), source })?;
    Ok(WorkerRequest {
        config: config.clone(),
        report_path: job_dir.join("report.json"),
        log_dir,
        page_workers,
    })
}

/// Where a job's request file lives: beside its report, in the job's private directory. Not a field
/// of the request, because it would be the only field describing the container rather than the
/// content — and the child already receives the path as an argument.
fn request_path_for(request: &WorkerRequest) -> PathBuf {
    request.report_path.with_file_name("request.json")
}

/// Runs one job in a child process and reports its outcome.
///
/// Standard error is **inherited**: the lines of running jobs reach the user as they happen rather
/// than all at once at the end. They interleave between jobs, but each line stays whole and already
/// carries its own span path. Standard output is not a channel of the protocol: nothing the child
/// writes there is read.
fn run_one(executable: &Path, index: usize, request: &WorkerRequest) -> Result<WorkerReport, WorkerError> {
    let request_path = request_path_for(request);
    write_request(&request_path, request)?;

    let status = std::process::Command::new(executable)
        .arg("--internal-worker")
        .arg(&request_path)
        .status()
        .map_err(|source| WorkerError::Spawn { index, source })?;

    if !status.success() {
        return Err(WorkerError::Died { index, status: status.to_string() });
    }
    read_report(&request.report_path)
}

/// Runs the requests in child processes, at most `parallelism` at a time, returning the reports
/// **in job order**.
///
/// The pool is a sliding one rather than waves: the threads take the next index from a shared
/// counter and deposit their report in the matching slot. Each thread does nothing but start a
/// process and wait for it — no domain work runs here, so the GIL is not involved and the threads
/// contend for nothing.
///
/// The indexed slots are why the aggregated output stays identical to the sequential one however
/// many children there are: finishing early overtakes nobody.
pub fn run_in_processes(
    executable: &Path,
    requests: &[WorkerRequest],
    parallelism: usize,
) -> Vec<Result<WorkerReport, WorkerError>> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<Result<WorkerReport, WorkerError>>>> = requests.iter().map(|_| Mutex::new(None)).collect();
    let workers = parallelism.clamp(1, requests.len().max(1));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(request) = requests.get(index) else { break };
                    let report = run_one(executable, index, request);
                    *slots[index].lock().unwrap_or_else(|p| p.into_inner()) = Some(report);
                }
            });
        }
    });

    slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            slot.into_inner()
                .unwrap_or_else(|p| p.into_inner())
                .unwrap_or_else(|| panic!("job {index} was never assigned to a worker: the pool left a hole"))
        })
        .collect()
}

/// Concatenates the results of the successful jobs, or reports the **first** failure in job order.
///
/// "First in job order", not "first to arrive": that is what makes the reported error the same one
/// the sequential loop would have propagated, whichever child happened to die first.
pub fn collect(reports: Vec<Result<WorkerReport, WorkerError>>) -> Result<Vec<DocumentOutcome>, JobFailure> {
    let mut documents = Vec::new();
    for (index, report) in reports.into_iter().enumerate() {
        match report {
            Ok(WorkerReport::Succeeded { documents: mut d }) => documents.append(&mut d),
            Ok(WorkerReport::Failed { error }) => return Err(JobFailure::Job { index, error }),
            Err(source) => return Err(JobFailure::Protocol { index, source }),
        }
    }
    Ok(documents)
}

/// The exit code of a child whose **protocol** broke: an unreadable request, logs that will not
/// open, a report that cannot be written. A job that failed for a domain reason does not come
/// through here.
pub const PROTOCOL_FAILURE_EXIT_CODE: i32 = 2;

/// The body of worker mode: runs the job described by the request and deposits its report.
///
/// The order of the steps is not negotiable. The request is read **before** logging starts, because
/// it is the request that says where the logs go; and logging starts **before** the job runs,
/// because otherwise the job's instrumentation would write into nothing.
///
/// Returning successfully means "the protocol worked", not "the job succeeded": a failed job is a
/// failure report deposited successfully, which is exactly what the parent expects to find.
pub fn execute(request_path: &Path) -> Result<(), WorkerError> {
    let request = read_request(request_path)?;

    // In the child's private directory, never in the parent's output directory. The parent merges
    // them into its own at the end of the run.
    let log_handle = crate::core::tracing_setup::init(request.config.verbosity, &request.log_dir)?;
    log_handle.set_csv_dir(&request.log_dir)?;

    let parallelism = crate::core::parallelism::Parallelism::pages(request.page_workers);
    let report = match crate::cli::job::run(&request.config, parallelism) {
        Ok(documents) => WorkerReport::Succeeded { documents },
        // Already recorded with its full chain where it happened: here the error is only packed for
        // the journey back, not recorded again.
        Err(e) => WorkerReport::Failed { error: ErrorRecord::from_error(&e) },
    };

    let write_result = write_report(&request.report_path, &report);
    // Attempted regardless: the diagnostic rows of a failed job are the most useful to have on
    // disk, and without this close the parent would merge files that were never flushed.
    let close_result = log_handle.close();
    write_result?;
    close_result.map_err(WorkerError::from)
}

#[cfg(test)]
mod tests {
    use crate::cli::parallelism_config::{ParallelismConfig, Workers};
    use super::*;
    use crate::cli::conf_parse::DocumentSpec;
    use crate::core::algorithm::PageOutcome;
    use crate::core::page::{DocumentId, FormatName};
    use crate::core::pipeline::Extracted;
    use crate::core::schedule::PageClass;
    use crate::core::tracing_setup::Verbosity;
    use crate::output::classes::fund::Fund;
    use crate::output::routines::write::{OutFlags, OutStructureMode};

    fn config() -> FreeportsConfig {
        FreeportsConfig {
            verbosity: Verbosity::Warn,
            reports: vec![DocumentSpec {
                url: Some("https://example.invalid/a.pdf".to_string()),
                path: Some(PathBuf::from("/tmp/a.pdf")),
                name: Some("a".to_string()),
            }],
            target_lists: vec!["TEST".to_string()],
            format: "FMT".to_string(),
            out_path: PathBuf::from("/tmp/out"),
            out_profile: OutStructureMode::Regular,
            out_flags: OutFlags::default(),
            parallelism: ParallelismConfig { jobs: Workers::Fixed(4), pages: Workers::Auto },
            batch_file: Some(PathBuf::from("/tmp/jobs.csv")),
            save_pdf: true,
            formats_repo_path: Some(PathBuf::from("/repo")),
            input_db_path: Some(PathBuf::from("/db")),
            config_file: None,
        }
    }

    fn request() -> WorkerRequest {
        WorkerRequest {
            page_workers: 1,
            config: config(),
            report_path: PathBuf::from("/tmp/w0/report.json"),
            log_dir: PathBuf::from("/tmp/w0/logs"),
        }
    }

    fn documents() -> Vec<DocumentOutcome> {
        vec![DocumentOutcome {
            id: DocumentId::new("a"),
            format: FormatName::new("FMT"),
            pages: vec![PageOutcome {
                page: 12,
                class: PageClass::new("investments"),
                results: vec![Extracted::Fund(Fund::new("Alpha Fund"))],
            }],
        }]
    }

    /// A real error with a cause, not an invented string: the only way to prove the chain of causes
    /// crosses the boundary.
    fn an_error_with_a_source() -> WorkerError {
        WorkerError::ReadReport {
            path: PathBuf::from("/tmp/missing.json"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        }
    }

    mod request_round_trip {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_request_written_to_a_file_reads_back_identical() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("request.json");
            let original = request();
            write_request(&path, &original).expect("writing a request to a fresh temp dir must work");
            assert_eq!(read_request(&path).expect("the request just written must read back"), original);
        }

        /// The child runs exactly the job the parent resolved: were a single field lost, it would
        /// do different work without anything failing.
        #[test]
        fn every_field_of_the_configuration_crosses_the_boundary() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("request.json");
            write_request(&path, &request()).unwrap();
            let restored = read_request(&path).unwrap().config;
            assert_eq!(restored, config());
        }
    }

    mod report_round_trip {
        use super::*;
        use pretty_assertions::assert_eq;

        fn round_trip(report: &WorkerReport) -> WorkerReport {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("report.json");
            write_report(&path, report).expect("writing a report to a fresh temp dir must work");
            read_report(&path).expect("the report just written must read back")
        }

        #[test]
        fn a_successful_report_reads_back_identical() {
            let report = WorkerReport::Succeeded { documents: documents() };
            assert_eq!(round_trip(&report), report);
        }

        /// A job that extracts nothing is not an error, and it is the case a careless serialization
        /// would confuse with a failure.
        #[test]
        fn a_successful_report_with_no_documents_stays_successful() {
            let report = WorkerReport::Succeeded { documents: vec![] };
            assert_eq!(round_trip(&report), report);
        }

        /// **Bit for bit, not "close enough".**
        ///
        /// The report is JSON, and reading floating-point numbers back is exact only with the
        /// round-trip feature enabled: without it, a number whose shortest decimal representation
        /// is not the one written comes back shifted by one unit in the last place. It is an error
        /// that makes nothing fail — the child succeeds, the parent writes — and shows up only by
        /// comparing a run in processes against a sequential one. Since the process path is the
        /// default, it would not even be an edge case.
        ///
        /// The values below are chosen because they really do fail without the feature: each is one
        /// unit in the last place away from the shortest decimal representing it.
        #[test]
        fn a_float_survives_the_report_bit_for_bit() {
            use crate::commons::consts::Currency;
            use crate::core::classes::BlockValue;
            use crate::output::classes::investment::{Bond, InvestmentFields};

            for rate in [0.029_249_999_999_999_998_f64, 0.057_999_999_999_999_996_f64] {
                let fields = InvestmentFields::new(
                    "Acme Corp",
                    "Acme",
                    BlockValue::from("Alpha Fund"),
                    BlockValue::from(1000.0),
                    BlockValue::from(Currency::EUR),
                );
                let bond = Bond::build(fields, None, Some(rate)).expect("a valid bond");
                let report = WorkerReport::Succeeded {
                    documents: vec![DocumentOutcome {
                        id: DocumentId::new("a"),
                        format: FormatName::new("FMT"),
                        pages: vec![PageOutcome {
                            page: 1,
                            class: PageClass::new("investments"),
                            results: vec![Extracted::Bond(bond)],
                        }],
                    }],
                };
                let back = round_trip(&report);
                let WorkerReport::Succeeded { documents } = &back else { panic!("expected a success") };
                let Extracted::Bond(bond) = &documents[0].pages[0].results[0] else {
                    panic!("expected a bond")
                };
                let value = bond.interest_rate.expect("the rate must survive").into_inner();
                assert_eq!(
                    value.to_bits(),
                    rate.to_bits(),
                    "{value} came back from JSON as a different double than {rate}"
                );
            }
        }

        /// The state the report is really written in: promises are fulfilled by the parent, after
        /// it has collected every job, so **every** pending field of a child's results crosses this
        /// boundary. A report whose promises came back as resolved lookalikes cost a whole batch —
        /// on a typed field it was a loud parse error, on a promised name a fund silently taking
        /// the name of the promise meant to fill it in.
        #[test]
        fn an_entity_with_a_pending_promise_reads_back_still_pending() {
            use crate::core::classes::BlockValue;
            use crate::core::promisable::PromisableFields;
            use crate::core::promise::Promise;
            use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;

            let promised_fund = Fund::from_value(&BlockValue::Promise(Promise::new("fund_name")))
                .expect("a promise is an admissible name");
            let promised_article =
                FundSfdrClassification::build("Alpha Fund", &BlockValue::Promise(Promise::new("article")))
                    .expect("a promise is an admissible article");
            let report = WorkerReport::Succeeded {
                documents: vec![DocumentOutcome {
                    id: DocumentId::new("a"),
                    format: FormatName::new("FMT"),
                    pages: vec![PageOutcome {
                        page: 12,
                        class: PageClass::new("investments"),
                        results: vec![
                            Extracted::Fund(promised_fund.clone()),
                            Extracted::FundSfdrClassification(promised_article.clone()),
                        ],
                    }],
                }],
            };

            let back = round_trip(&report);
            assert_eq!(back, report);

            let WorkerReport::Succeeded { documents } = &back else { panic!("expected a success") };
            let results = &documents[0].pages[0].results;
            let Extracted::Fund(fund) = &results[0] else { panic!("expected a fund") };
            assert!(fund.pending_name().is_some(), "the fund came back as {fund:?}");
            assert_eq!(fund.pending().len(), promised_fund.pending().len());
            let Extracted::FundSfdrClassification(classification) = &results[1] else {
                panic!("expected a classification")
            };
            assert_eq!(classification.pending().len(), promised_article.pending().len());
        }

        #[test]
        fn a_failed_report_reads_back_identical() {
            let report = WorkerReport::Failed { error: ErrorRecord::from_error(&an_error_with_a_source()) };
            assert_eq!(round_trip(&report), report);
        }

        /// The display form is what the parent prints: it must be **the same** the sequential case
        /// would have printed, or the same error reaches the user in two ways depending on how many
        /// workers were asked for.
        #[test]
        fn the_display_form_of_the_error_is_preserved_verbatim() {
            let error = an_error_with_a_source();
            let report = WorkerReport::Failed { error: ErrorRecord::from_error(&error) };
            match round_trip(&report) {
                WorkerReport::Failed { error: record } => assert_eq!(record.display, error.to_string()),
                other => panic!("expected a failed report, got {other:?}"),
            }
        }

        #[test]
        fn the_source_chain_of_the_error_is_preserved() {
            let report = WorkerReport::Failed { error: ErrorRecord::from_error(&an_error_with_a_source()) };
            match round_trip(&report) {
                WorkerReport::Failed { error: record } => assert_eq!(record.source, ["no such file"]),
                other => panic!("expected a failed report, got {other:?}"),
            }
        }
    }

    /// The two ways of receiving nothing readable are kept apart because they point at different
    /// bugs, and neither panics: the parent has to be able to report them.
    mod protocol_failures {
        use super::*;

        #[test]
        fn a_missing_request_file_is_a_read_error_naming_the_path() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("absent.json");
            match read_request(&path) {
                Err(WorkerError::ReadRequest { path: reported, .. }) => assert_eq!(reported, path),
                other => panic!("expected a read error, got {other:?}"),
            }
        }

        #[test]
        fn a_malformed_request_file_is_a_parse_error_naming_the_path() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("garbage.json");
            std::fs::write(&path, b"{ this is not json").unwrap();
            match read_request(&path) {
                Err(WorkerError::ParseRequest { path: reported, .. }) => assert_eq!(reported, path),
                other => panic!("expected a parse error, got {other:?}"),
            }
        }

        /// The insidious case: valid JSON of the wrong shape. It happens when parent and child are
        /// two different builds of the binary, and it is the error an unchecked unwrap would turn
        /// into a panic inside a child process — that is, into a message nobody sees.
        #[test]
        fn well_formed_json_of_the_wrong_shape_is_a_parse_error_not_a_panic() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("wrong-shape.json");
            std::fs::write(&path, br#"{"config": 42}"#).unwrap();
            assert!(matches!(read_request(&path), Err(WorkerError::ParseRequest { .. })));
        }

        #[test]
        fn a_missing_report_file_is_a_read_error_naming_the_path() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("absent.json");
            match read_report(&path) {
                Err(WorkerError::ReadReport { path: reported, .. }) => assert_eq!(reported, path),
                other => panic!("expected a read error, got {other:?}"),
            }
        }

        #[test]
        fn a_report_with_an_unknown_outcome_tag_is_a_parse_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("unknown.json");
            std::fs::write(&path, br#"{"outcome": "exploded"}"#).unwrap();
            assert!(matches!(read_report(&path), Err(WorkerError::ParseReport { .. })));
        }

        #[test]
        fn writing_into_a_directory_that_does_not_exist_is_a_write_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("no-such-subdir").join("request.json");
            match write_request(&path, &request()) {
                Err(WorkerError::WriteRequest { path: reported, .. }) => assert_eq!(reported, path),
                other => panic!("expected a write error, got {other:?}"),
            }
        }
    }

    /// Which failure reaches the user when several jobs go wrong. The rule is "first in job order",
    /// not "first to arrive": the only one making the reported error the same the sequential loop
    /// would have propagated.
    mod collecting_reports {
        use super::*;
        use pretty_assertions::assert_eq;

        fn succeeded(name: &str) -> Result<WorkerReport, WorkerError> {
            Ok(WorkerReport::Succeeded {
                documents: vec![DocumentOutcome { id: DocumentId::new(name), format: FormatName::new("FMT"), pages: vec![] }],
            })
        }

        fn failed(message: &str) -> Result<WorkerReport, WorkerError> {
            Ok(WorkerReport::Failed {
                error: ErrorRecord { debug: format!("{message:?}"), display: message.to_string(), source: vec![] },
            })
        }

        fn broken(index: usize) -> Result<WorkerReport, WorkerError> {
            Err(WorkerError::Died { index, status: "exit status: 9".to_string() })
        }

        #[test]
        fn all_successful_jobs_concatenate_in_job_order() {
            let documents = collect(vec![succeeded("a"), succeeded("b"), succeeded("c")]).expect("no job failed");
            let ids: Vec<&str> = documents.iter().map(|d| d.id.as_str()).collect();
            assert_eq!(ids, ["a", "b", "c"]);
        }

        #[test]
        fn an_empty_batch_collects_to_no_documents() {
            assert_eq!(collect(vec![]).expect("no job failed"), vec![]);
        }

        /// A job that extracts nothing neither interrupts the concatenation nor leaves a hole.
        #[test]
        fn a_job_with_no_documents_does_not_break_the_concatenation() {
            let empty = Ok(WorkerReport::Succeeded { documents: vec![] });
            let documents = collect(vec![succeeded("a"), empty, succeeded("c")]).expect("no job failed");
            let ids: Vec<&str> = documents.iter().map(|d| d.id.as_str()).collect();
            assert_eq!(ids, ["a", "c"]);
        }

        #[test]
        fn the_first_failing_job_in_order_is_the_one_reported() {
            let failure = collect(vec![succeeded("a"), failed("second broke"), failed("third broke")])
                .expect_err("a failing job must be reported");
            assert_eq!(failure.index(), 1);
            assert_eq!(failure.to_string(), "second broke");
        }

        /// The case that separates "first in order" from "first to arrive": a later job died of a
        /// signal, an earlier one failed for a domain reason. The earlier one must win.
        #[test]
        fn an_earlier_domain_failure_wins_over_a_later_protocol_failure() {
            let failure = collect(vec![succeeded("a"), failed("second broke"), broken(2)])
                .expect_err("a failing job must be reported");
            assert!(matches!(failure, JobFailure::Job { index: 1, .. }), "got {failure:?}");
        }

        #[test]
        fn an_earlier_protocol_failure_wins_over_a_later_domain_failure() {
            let failure = collect(vec![broken(0), failed("second broke")]).expect_err("a failing job must be reported");
            assert!(matches!(failure, JobFailure::Protocol { index: 0, .. }), "got {failure:?}");
        }

        /// A domain error's message must reach the user **verbatim**: the same job, run
        /// sequentially, prints exactly this line.
        #[test]
        fn a_domain_failure_is_reported_with_the_original_message_and_nothing_else() {
            let original = "the specified path /tmp/nope.pdf does not exist";
            let failure = collect(vec![failed(original)]).expect_err("a failing job must be reported");
            assert_eq!(failure.to_string(), original);
        }

        /// A protocol failure, by contrast, **must** name its job: not being a domain error, the
        /// user has no other way of knowing which batch row went wrong.
        #[test]
        fn a_protocol_failure_names_the_job_it_belongs_to() {
            let failure = collect(vec![broken(0)]).expect_err("a broken worker must be reported");
            assert!(failure.to_string().contains("job 0"), "message does not name the job: {failure}");
        }
    }

    /// The real pool runs against the real binary in the integration tests. What is checked here is
    /// what needs no process: preparing the private directories, and that each job gets one of its
    /// own.
    mod preparing_requests {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_private_log_directory_is_created_on_disk() {
            let dir = tempfile::tempdir().unwrap();
            let request = prepare_request(dir.path(), 0, &config(), 1).expect("preparing a request must work");
            assert!(request.log_dir.is_dir(), "the log directory was not created: {}", request.log_dir.display());
        }

        /// The children's logs all have the same fixed names: without a directory per job they
        /// would overwrite each other, and the merged log would lose every row but the last.
        #[test]
        fn two_jobs_never_share_a_directory() {
            let dir = tempfile::tempdir().unwrap();
            let first = prepare_request(dir.path(), 0, &config(), 1).unwrap();
            let second = prepare_request(dir.path(), 1, &config(), 1).unwrap();
            assert_ne!(first.log_dir, second.log_dir);
            assert_ne!(first.report_path, second.report_path);
        }

        /// No file of a child ends up beside the run's results.
        #[test]
        fn nothing_is_prepared_inside_the_configured_output_directory() {
            let dir = tempfile::tempdir().unwrap();
            let request = prepare_request(dir.path(), 0, &config(), 1).unwrap();
            assert!(request.log_dir.starts_with(dir.path()));
            assert!(!request.log_dir.starts_with(&config().out_path));
            assert!(!request.report_path.starts_with(&config().out_path));
        }

        #[test]
        fn the_configuration_travels_unchanged_into_the_request() {
            let dir = tempfile::tempdir().unwrap();
            assert_eq!(prepare_request(dir.path(), 3, &config(), 1).unwrap().config, config());
        }
    }

    mod error_messages {
        use super::*;

        #[test]
        fn every_message_names_the_file_it_is_about() {
            let path = PathBuf::from("/tmp/x/report.json");
            let io = || std::io::Error::new(std::io::ErrorKind::NotFound, "boom");
            let messages = [
                WorkerError::WriteRequest { path: path.clone(), source: io() }.to_string(),
                WorkerError::ReadRequest { path: path.clone(), source: io() }.to_string(),
                WorkerError::WriteReport { path: path.clone(), source: io() }.to_string(),
                WorkerError::ReadReport { path: path.clone(), source: io() }.to_string(),
            ];
            for message in messages {
                assert!(message.contains("/tmp/x/report.json"), "message does not name the file: {message}");
            }
        }
    }
}
