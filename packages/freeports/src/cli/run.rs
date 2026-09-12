//! [`execute`]: the whole orchestration of one command-line invocation.
//!
//! It composes every other module of this area, in this order:
//!
//! 1. read the command line and the environment;
//! 2. merge those two over the defaults, in a **first pass whose only purpose** is to discover which configuration file is in effect — the file's own path can itself be configured;
//! 3. read that file;
//! 4. do the real merge: defaults, then file, then environment, then command line;
//! 5. in batch mode, overlay each CSV row onto the merge, one job per row; otherwise validate the merge once;
//! 6. run each job, concatenating the results in order;
//! 7. write the total.
//!
//! Steps 1 to 5 are exposed as [`resolve_configs`] rather than staying private to [`execute`].
//! Without that seam, the only way to observe that the command line beats the environment beats the
//! file, field by field, would be through the side effects on disk — impracticable for the fields
//! that leave no trace in an output file.
//!
//! # Where a batch's shared decisions come from
//!
//! When several configurations resolve, the parameters that belong to the **run** rather than to a
//! job — the output path, profile and flags, and the parallelism — come from the **first** resolved
//! configuration. A batch file's columns cannot set them in any case.

use crate::cli::batch::{self, BatchError};
use crate::cli::config_locations::cmd::{CliArgs, CmdConfigError};
use crate::cli::config_locations::env::{self, EnvConfigError};
use crate::cli::config_locations::file::{self, FileConfigError};
use crate::cli::freeports_config::{self, FreeportsConfig, FreeportsConfigError};
use crate::cli::job;
use crate::cli::output::{self, OutputError};
use crate::cli::parallelism_config::ParallelismConfig;
use crate::cli::partial_config::{ConfigSource, defaults, overwrite};
use crate::cli::worker::{self, JobFailure, WorkerError};
use crate::core::algorithm::DocumentOutcome;
use crate::core::parallelism::{self, Parallelism};
use crate::core::tracing_setup::{ErrorRecord, LogHandle, TracingSetupError};

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Cmd(#[from] CmdConfigError),
    #[error(transparent)]
    Env(#[from] EnvConfigError),
    #[error(transparent)]
    File(#[from] FileConfigError),
    #[error(transparent)]
    Batch(#[from] BatchError),
    #[error(transparent)]
    Validate(#[from] FreeportsConfigError),
    #[error(transparent)]
    Output(#[from] OutputError),
    /// The log could not take its place beside the outputs — a directory that cannot be created, a
    /// file that cannot be opened. Not a job error, but not swallowed either: without the log the
    /// user loses the localised diagnostics of the very run they are launching.
    #[error(transparent)]
    Logging(#[from] TracingSetupError),
    /// **No job produced anything**, and this is the first failure in job order.
    ///
    /// Not "a job failed": a job that fails costs its own results and nothing else, and the run goes
    /// on and writes what the others produced. This variant is the floor — a run with no partial
    /// result to save — and it carries the same `JobFailure` whether the job ran in this process or
    /// in a child, so the message is byte-for-byte the one the sequential path would have printed.
    #[error(transparent)]
    Worker(#[from] JobFailure),
    /// The child-process infrastructure did not start — a work area that cannot be created, an
    /// executable that cannot be identified. Not a failed job: no job ever started.
    #[error("cannot set up the worker processes: {source}")]
    WorkerSetup {
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    WorkerRequest(#[from] WorkerError),
}

/// What a finished run has to say about itself beyond "it did not fail".
///
/// Two sentences that are not the same — *everything I looked at was fine* and *I looked at
/// everything and it was fine* — and a run that cannot tell them apart makes a batch of 904 jobs
/// with two silently missing look exactly like a batch of 904 that all worked.
///
/// It covers **jobs and documents**, deliberately not rows and pages. Those are contained at their
/// own level, happen on ordinary runs by the dozen, and are already summarised in their own
/// end-of-run event; promoting them here would change the exit status of nearly every run that
/// works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunCompleteness {
    /// Every document of every job was read, and the results were written.
    Everything,
    /// The results were written, and **not everything was read**: jobs failed, or documents could
    /// not be opened.
    NotEverything,
}

/// Resolves the configurations without running any job or writing any output.
///
/// Opens its own span. Each step already logs its own failure where the specific error is
/// constructed, so this function does not re-log a propagated error, only the resolution steps
/// genuinely local to it: which configuration file ends up in effect, whether batch mode applies,
/// how many jobs came out.
pub fn resolve_configs(args: CliArgs) -> Result<Vec<FreeportsConfig>, CliError> {
    let span = tracing::info_span!("resolve_config");
    let _guard = span.enter();

    let cmd_partial = args.to_partial_config()?;
    let env_partial = env::load()?;

    // The first pass exists only to discover which configuration file is in effect.
    let first_pass = overwrite(overwrite(defaults(), env_partial.clone(), ConfigSource::Env), cmd_partial.clone(), ConfigSource::Cmd);
    // No log in the common (`None`) branch: `file::find_config` already logs, more specifically,
    // whether/where it found a configuration file -- logging again here would just repeat it.
    // Only the override branch below adds information `find_config` never gets a chance to log
    // (it isn't even called).
    let config_file_path = match first_pass.values.config_file.clone() {
        Some(path) => {
            tracing::debug!(config_file = %path.display(), "configuration file location set via cmd/env, skipping the search tiers");
            Some(path)
        }
        None => file::find_config(),
    };
    let file_partial = file::load(config_file_path.as_deref())?;

    // Merge reale: default <- file <- env <- cmd.
    let merged = overwrite(
        overwrite(overwrite(defaults(), file_partial, ConfigSource::File), env_partial, ConfigSource::Env),
        cmd_partial,
        ConfigSource::Cmd,
    );

    let configs = match merged.values.batch_file.clone() {
        Some(batch_file) => {
            tracing::info!(batch_file = %batch_file.display(), "resolving batch configuration");
            batch::load_jobs(&batch_file)?
                .into_iter()
                .map(|row| {
                    let row_merged = overwrite(merged.clone(), row, ConfigSource::Batch);
                    freeports_config::validate(row_merged).map_err(CliError::from)
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        None => vec![freeports_config::validate(merged)?],
    };
    tracing::debug!(job_count = configs.len(), "resolved job configuration(s)");
    Ok(configs)
}

/// Resolves the configurations, runs each job, concatenates the results in order, and writes the
/// total.
///
/// Opens the outermost span, so that every nested one — and any error each logs at its own boundary
/// — carries the run's context. No error is re-logged here: each step already logs its own failure
/// once, closest to where it happened.
///
/// # A job that fails does not cost the run
///
/// The results are written **whatever the jobs did**. A batch of 904 jobs in which two fail is 902
/// jobs' worth of results that exist and are wanted; before, the first failure returned early and
/// every one of them was thrown away. The failures are reported — one summary and one event each —
/// and [`RunCompleteness`] carries the fact up to the exit status.
///
/// The one case that is still a failed run is a run that produced **nothing at all**: writing the
/// nine empty files of a batch in which every job failed is worse than not writing, because an
/// empty table and a table nobody could fill look the same on disk.
pub fn execute(args: CliArgs, log_handle: &LogHandle) -> Result<RunCompleteness, CliError> {
    let span = tracing::info_span!("run");
    let _guard = span.enter();

    let configs = resolve_configs(args)?;
    // The first moment at which the whole configuration is known, and therefore the first at which
    // either log can be settled. Both were deliberately left open until here: until this line the
    // `.log.csv` has no destination, `freeports.log.jsonl` does not know whether it exists, and
    // everything produced by resolving the configuration is held in memory.
    if let Some(first) = configs.first() {
        // Before the CSV destination, and before any job: it is what raises the two level filters
        // to the resolved verbosity, and the sooner it runs the fewer records are lost to a filter
        // the configuration file has already contradicted.
        log_handle.settle_verbosity(first.verbosity).map_err(CliError::from)?;
        log_handle.set_csv_dir(&output::log_csv_dir(first)).map_err(CliError::from)?;
    }
    let (outcomes, failures) = run_jobs(&configs, log_handle)?;
    report_failures(&configs, &failures);

    // Nothing came out of any job: there is no partial result to save, and saying so is the only
    // honest thing left. The failure reported is the **first in job order**, which is the one the
    // sequential path would have propagated.
    if outcomes.is_empty()
        && let Some(first) = failures.into_iter().next()
    {
        return Err(CliError::Worker(first));
    }

    if let Some(first) = configs.first() {
        output::write_results(first, &outcomes)?;
    }
    Ok(completeness(&configs, &outcomes))
}

/// One summary event and one event per failed job, in job order.
///
/// The summary exists for whoever watches only stderr: a run that writes its results and says
/// nothing else would let two missing jobs out of 904 pass unnoticed. Each individual event keeps
/// the failure's own message — for a domain failure, `JobFailure::Job` makes it byte-for-byte the
/// one the sequential path would have printed — and adds the index, which is the only thing telling
/// the reader *which* row of the batch file it was.
fn report_failures(configs: &[FreeportsConfig], failures: &[JobFailure]) {
    if failures.is_empty() {
        return;
    }
    tracing::warn!(
        failed = failures.len(),
        total = configs.len(),
        "{} of {} jobs produced no results; the remaining {} were written",
        failures.len(),
        configs.len(),
        configs.len().saturating_sub(failures.len())
    );
    for failure in failures {
        let index = failure.index();
        tracing::error!(
            job = index,
            format = configs.get(index).map(|c| c.format.as_str()).unwrap_or("<unknown>"),
            "job {index} failed: {failure}"
        );
    }
}

/// Whether the run read everything it was asked to read.
///
/// One comparison covers both levels, and covers them the same way whether the jobs ran in this
/// process or in children: every document named in a configuration produces exactly one outcome
/// when it is read, a skipped document produces none, and a failed job produces none for any of
/// its documents. So *documents named* against *outcomes produced* is the whole question.
fn completeness(configs: &[FreeportsConfig], outcomes: &[DocumentOutcome]) -> RunCompleteness {
    let named: usize = configs.iter().map(|config| config.reports.len()).sum();
    if outcomes.len() < named {
        RunCompleteness::NotEverything
    } else {
        RunCompleteness::Everything
    }
}

/// The parallelism section governing this run: the **first** resolved configuration's, the same one
/// that governs the write parameters. Both levels are properties of the run rather than of a job,
/// and a batch file's columns cannot set them anyway.
fn run_parallelism(configs: &[FreeportsConfig]) -> ParallelismConfig {
    configs.first().map_or_else(ParallelismConfig::default, |first| first.parallelism)
}

/// How many jobs at a time, and how many pages inside each, resolved together.
///
/// **Together and in that order**, because the second depends on the first: automatically, the
/// budget of cores is divided among the concurrent jobs, so four jobs on twenty hardware threads
/// take five each rather than twenty. With a single job — the non-batch case, which is also the one
/// where job parallelism does nothing — they all stay available, and that is where page parallelism
/// really counts.
///
/// An **explicit** page count is not divided: whoever wrote it asked for it.
fn resolve_parallelism(configs: &[FreeportsConfig]) -> (usize, Parallelism) {
    let requested = run_parallelism(configs);
    let jobs = requested.resolve_jobs(configs.len());
    let pages = requested.resolve_pages(jobs);
    if let Some(total) = ParallelismConfig::oversubscription(jobs, pages) {
        tracing::warn!(
            jobs,
            pages = pages.pages,
            total,
            available = parallelism::available_threads(),
            "the requested parallelism opens {total} workers on a machine with {} cores",
            parallelism::available_threads()
        );
    }
    tracing::debug!(
        requested_jobs = %requested.jobs,
        requested_pages = %requested.pages,
        jobs,
        pages = pages.pages,
        "parallelism resolved"
    );
    (jobs, pages)
}

/// Runs the resolved jobs, concatenating the results of those that worked and collecting the
/// failures of those that did not — both in job order.
///
/// A single job, or one worker, stays **exactly** on the sequential loop: no processes, no
/// temporary work area, nothing observably different. It is the default, and the reason someone who
/// asks for nothing sees nothing change.
///
/// The two branches produce the **same** list of failures, which is what keeps `--jobs 1` and
/// `--jobs N` the same run — the property `docs/source/reference/design/determinism.md` defends. A
/// job that fails in this process is packed into the same [`JobFailure::Job`] a child's failure
/// arrives in, so its message stays the error's own, verbatim.
///
/// The `Err` of the return type is for what belongs to the **run** rather than to a job: the worker
/// infrastructure that would not start, a request that could not be written. No job ran, so there is
/// nothing partial to keep.
fn run_jobs(
    configs: &[FreeportsConfig],
    log_handle: &LogHandle,
) -> Result<(Vec<DocumentOutcome>, Vec<JobFailure>), CliError> {
    let (jobs, pages) = resolve_parallelism(configs);
    if jobs <= 1 {
        let mut outcomes = Vec::new();
        let mut failures = Vec::new();
        for (index, config) in configs.iter().enumerate() {
            match job::run(config, pages) {
                Ok(documents) => outcomes.extend(documents),
                // Already logged with its full chain by `job::run` itself; here it is only packed,
                // in the same shape a child's failure comes back in.
                Err(error) => failures.push(JobFailure::Job {
                    index,
                    error: ErrorRecord::from_error(&error),
                }),
            }
        }
        return Ok((outcomes, failures));
    }
    run_jobs_in_processes(configs, jobs, pages, log_handle)
}

/// Running the jobs in child processes.
///
/// The only level of parallelism that gets past the GIL, and the only reason a process boundary is
/// worth paying for: the PDF loading no thread can speed up is 35-75% of a job's time.
///
/// **Careful with the current executable under a test harness**: there it is the test binary, not
/// the real one. A test triggering this branch would launch copies of the test binary, which know
/// nothing of the worker flag and exit non-zero — a clean error rather than an infinite loop, but a
/// test proving something other than it thinks. The real pool is exercised from the integration
/// tests.
fn run_jobs_in_processes(
    configs: &[FreeportsConfig],
    parallelism: usize,
    page_workers: Parallelism,
    log_handle: &LogHandle,
) -> Result<(Vec<DocumentOutcome>, Vec<JobFailure>), CliError> {
    let executable = std::env::current_exe().map_err(|source| CliError::WorkerSetup { source })?;
    // One work area per run, which cleans itself up: the children's private files do not outlive
    // the parent and never appear beside the results.
    let work_area = worker::WorkArea::create()?;
    let requests = configs
        .iter()
        .enumerate()
        .map(|(index, config)| {
            worker::prepare_request(work_area.path(), index, config, page_workers.pages)
        })
        .collect::<Result<Vec<_>, _>>()?;

    tracing::info!(
        job_count = requests.len(),
        parallelism,
        pages = page_workers.pages,
        "running jobs in worker processes"
    );
    let reports = worker::run_in_processes(&executable, &requests, parallelism);

    // Before reading the outcomes, and however they went: the work area disappears when this
    // function returns, and the logs of a **failed** job are the most useful of all to keep.
    for request in &requests {
        log_handle.absorb_worker_logs(&request.log_dir)?;
    }

    Ok(worker::collect(reports))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway log handle for the tests, with both destinations in a temporary directory. The
    /// code under test settles the CSV destination on the resolved output directory: without a real
    /// handle that could not be exercised, and with one the suite's working directory is never
    /// dirtied.
    fn test_log_handle() -> crate::core::tracing_setup::LogHandle {
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = crate::core::tracing_setup::log_handle_for_tests(dir.path())
            .expect("test log handle");
        // The temporary directory is kept alive for the test process's lifetime: the code reopens
        // the CSV in the real output directory anyway, and keeping it alive stops the fallback
        // destination disappearing from under the close.
        std::mem::forget(dir);
        handle
    }
    use crate::cli::config_locations::cmd::CliArgs;
    use clap::Parser;
    use std::sync::Mutex;

    const ALL_FREEPORTS_VARS: &[&str] = &[
        "FREEPORTS_URL",
        "FREEPORTS_PDF",
        "FREEPORTS_REPORTS",
        "FREEPORTS_VERBOSITY",
        "FREEPORTS_N_WORKERS",
        "FREEPORTS_BATCH_FILE",
        "FREEPORTS_OUT_PATH",
        "FREEPORTS_OUT_PROFILE",
        "FREEPORTS_SEPARATE_OUT",
        "FREEPORTS_ARCHIVE",
        "FREEPORTS_SAVE_PDF",
        "FREEPORTS_FORMAT",
        "FREEPORTS_CONFIG_FILE",
        "FREEPORTS_TARGET_LIST",
        "FREEPORTS_FORMATS_REPO_PATH",
        "FREEPORTS_INPUT_DB_PATH",
    ];

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Clears and restores every environment variable for the duration of a test, so that one left
    /// over from the developer's own shell cannot influence the resolution.
    struct EnvScope {
        _lock: std::sync::MutexGuard<'static, ()>,
        originals: Vec<(&'static str, Option<String>)>,
    }

    impl EnvScope {
        fn new() -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let originals = ALL_FREEPORTS_VARS.iter().map(|&k| (k, std::env::var(k).ok())).collect();
            for &k in ALL_FREEPORTS_VARS {
                unsafe { std::env::remove_var(k) };
            }
            Self { _lock: lock, originals }
        }
    }

    impl Drop for EnvScope {
        fn drop(&mut self) {
            for (k, v) in &self.originals {
                match v {
                    Some(val) => unsafe { std::env::set_var(k, val) },
                    None => unsafe { std::env::remove_var(k) },
                }
            }
        }
    }

    fn parse(args: &[&str]) -> CliArgs {
        let mut full = vec!["freeports"];
        full.extend_from_slice(args);
        CliArgs::try_parse_from(full).expect("argv must parse")
    }

    /// A real empty YAML file, passed explicitly in every test here so that resolution never falls
    /// back on the real configuration-file search, which the environment scope does not isolate.
    fn empty_config_file(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("empty.yaml");
        std::fs::write(&path, "").unwrap();
        path
    }

    /// Which of the two paths a run takes — the sequential loop or the pool of child processes —
    /// and why. The decision is entirely in the resolution, so it is checked there, where nothing
    /// needs starting; the real pool runs in the integration tests.
    mod parallelism_decides_the_path {
        use super::*;
        use crate::cli::conf_parse::DocumentSpec;
        use crate::cli::parallelism_config::Workers;
        use crate::core::tracing_setup::Verbosity;
        use crate::output::routines::write::{OutFlags, OutStructureMode};
        use std::path::PathBuf;

        fn config_with(parallelism: ParallelismConfig) -> FreeportsConfig {
            FreeportsConfig {
                verbosity: Verbosity::Warn,
                reports: vec![DocumentSpec { url: None, path: Some(PathBuf::from("/tmp/a.pdf")), name: Some("a".to_string()) }],
                target_lists: vec!["TEST".to_string()],
                format: "FMT".to_string(),
                out_path: PathBuf::from("/tmp/out"),
                out_profile: OutStructureMode::Regular,
                out_flags: OutFlags::default(),
                parallelism,
                batch_file: None,
                save_pdf: false,
                formats_repo_path: None,
                input_db_path: None,
                config_file: None,
            }
        }

        fn batch_of(count: usize, jobs: usize) -> Vec<FreeportsConfig> {
            let parallelism =
                ParallelismConfig { jobs: Workers::Fixed(jobs), pages: Workers::Fixed(1) };
            (0..count).map(|_| config_with(parallelism)).collect()
        }

        fn jobs_of(configs: &[FreeportsConfig]) -> usize {
            resolve_parallelism(configs).0
        }

        /// One job worker keeps a batch on exactly the code that predates child processes.
        #[test]
        fn one_job_worker_keeps_a_batch_sequential() {
            assert_eq!(jobs_of(&batch_of(8, 1)), 1);
        }

        #[test]
        fn a_single_job_is_sequential_however_many_workers_were_asked_for() {
            assert_eq!(jobs_of(&batch_of(1, 16)), 1);
        }

        /// The default is now automatic at both levels, so a batch uses the machine's cores without
        /// the user having to know.
        #[test]
        fn the_default_now_runs_a_batch_in_parallel() {
            let configs: Vec<FreeportsConfig> =
                (0..8).map(|_| config_with(ParallelismConfig::default())).collect();
            let expected = parallelism::available_threads().min(8);
            assert_eq!(jobs_of(&configs), expected);
        }

        /// The other half of the same default: with a single job the budget is divided with nobody,
        /// and that is where page parallelism really counts.
        #[test]
        fn the_default_gives_a_lone_job_every_core_for_its_pages() {
            let configs = vec![config_with(ParallelismConfig::default())];
            let (jobs, pages) = resolve_parallelism(&configs);
            assert_eq!(jobs, 1);
            assert_eq!(pages.pages, parallelism::available_threads());
        }

        /// One everywhere walks the sequential code at both levels, which is how determinism is
        /// checked.
        #[test]
        fn one_everywhere_is_sequential_at_both_levels() {
            let configs: Vec<FreeportsConfig> =
                (0..8).map(|_| config_with(ParallelismConfig::SEQUENTIAL)).collect();
            assert_eq!(resolve_parallelism(&configs), (1, Parallelism::SEQUENTIAL));
        }

        /// An explicit page count is not divided among the jobs. The product may exceed the cores —
        /// the request is honoured, and the resolution says so.
        #[test]
        fn an_explicit_page_count_is_not_divided_among_the_jobs() {
            let parallelism =
                ParallelismConfig { jobs: Workers::Fixed(2), pages: Workers::Fixed(7) };
            let configs: Vec<FreeportsConfig> =
                (0..4).map(|_| config_with(parallelism)).collect();
            assert_eq!(resolve_parallelism(&configs), (2, Parallelism::pages(7)));
        }

        /// Starting more processes than jobs makes no sense: they would be children born to do
        /// nothing, each paying for a Python interpreter to initialise.
        #[test]
        fn more_workers_than_jobs_are_capped_at_the_number_of_jobs() {
            assert_eq!(jobs_of(&batch_of(3, 16)), 3);
        }

        #[test]
        fn fewer_workers_than_jobs_are_taken_as_they_are() {
            assert_eq!(jobs_of(&batch_of(16, 4)), 4);
        }

        /// An empty batch — a batch file with only a header — is legitimate, and must not become a
        /// zero that propagates into a minimum or a thread count. One worker for zero jobs starts
        /// none anyway.
        #[test]
        fn an_empty_batch_never_asks_for_more_than_one_worker() {
            assert_eq!(jobs_of(&[]), 1);
        }

        /// A batch file's columns cannot set the parallelism: the value is the first resolved
        /// configuration's, the same one that governs the write parameters.
        #[test]
        fn the_value_comes_from_the_first_resolved_configuration() {
            let mut configs = batch_of(4, 3);
            configs[1].parallelism.jobs = Workers::Fixed(99);
            assert_eq!(jobs_of(&configs), 3);
        }
    }

    /// [`completeness`]: what the exit status is built on. One comparison — documents named against
    /// outcomes produced — answers both levels at once, and answers them identically whether the
    /// jobs ran in this process or in children.
    mod run_completeness {
        use super::*;
        use crate::cli::conf_parse::DocumentSpec;
        use crate::core::page::{DocumentId, FormatName};
        use crate::core::tracing_setup::Verbosity;
        use crate::output::routines::write::{OutFlags, OutStructureMode};
        use pretty_assertions::assert_eq;
        use std::path::PathBuf;

        fn config_naming(documents: usize) -> FreeportsConfig {
            FreeportsConfig {
                verbosity: Verbosity::Warn,
                reports: (0..documents)
                    .map(|i| DocumentSpec {
                        url: None,
                        path: Some(PathBuf::from(format!("/tmp/{i}.pdf"))),
                        name: Some(format!("doc-{i}")),
                    })
                    .collect(),
                target_lists: vec!["TEST".to_string()],
                format: "FMT".to_string(),
                out_path: PathBuf::from("/tmp/out"),
                out_profile: OutStructureMode::Regular,
                out_flags: OutFlags::default(),
                parallelism: ParallelismConfig::SEQUENTIAL,
                batch_file: None,
                save_pdf: false,
                formats_repo_path: None,
                input_db_path: None,
                config_file: None,
            }
        }

        fn outcomes(count: usize) -> Vec<DocumentOutcome> {
            (0..count)
                .map(|i| DocumentOutcome {
                    id: DocumentId::new(format!("doc-{i}")),
                    format: FormatName::new("FMT"),
                    pages: vec![],
                })
                .collect()
        }

        #[test]
        fn every_named_document_read_is_everything() {
            let configs = vec![config_naming(2), config_naming(1)];
            assert_eq!(completeness(&configs, &outcomes(3)), RunCompleteness::Everything);
        }

        /// One document short: the job ran and one of its documents would not open. The results are
        /// written, and the run says it did not read everything.
        #[test]
        fn one_document_missing_is_not_everything() {
            let configs = vec![config_naming(2), config_naming(1)];
            assert_eq!(completeness(&configs, &outcomes(2)), RunCompleteness::NotEverything);
        }

        /// A whole job missing is the same shape of answer, which is the point of measuring
        /// documents rather than jobs: a failed job produces no outcome for any of its documents,
        /// so the one comparison covers both levels without a second counter to keep in step.
        #[test]
        fn a_whole_job_missing_is_not_everything() {
            let configs = vec![config_naming(3), config_naming(3)];
            assert_eq!(completeness(&configs, &outcomes(3)), RunCompleteness::NotEverything);
        }

        /// A run that names no document reads everything it was asked to read, which is nothing.
        /// The empty batch must not be reported as incomplete.
        #[test]
        fn a_run_naming_no_document_is_complete() {
            assert_eq!(completeness(&[], &[]), RunCompleteness::Everything);
            assert_eq!(completeness(&[config_naming(0)], &[]), RunCompleteness::Everything);
        }

        /// Pages and rows are deliberately not part of this. An outcome exists for a document whose
        /// every page was skipped — the extraction read the document, it just produced nothing from
        /// it — so a run full of skipped pages still exits `0`, as it does today.
        #[test]
        fn a_document_read_but_empty_is_still_a_document_read() {
            let configs = vec![config_naming(1)];
            assert_eq!(completeness(&configs, &outcomes(1)), RunCompleteness::Everything);
        }
    }

    mod resolve_configs_batch_dispatch {
        use super::*;

        #[test]
        fn a_two_row_batch_file_resolves_to_two_freeports_configs_in_file_order() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf = dir.path().join("report.pdf");
            std::fs::write(&pdf, b"%PDF-1.4").unwrap();
            std::fs::create_dir_all(dir.path().join("metadata")).unwrap();
            std::fs::write(
                dir.path().join("metadata/formats.csv"),
                "Name,Locale,Year,Country,Version\nA,EN,24,,\nB,EN,24,,\n",
            )
            .unwrap();
            std::fs::write(dir.path().join("metadata/url_mapping.csv"), "Format name,Url\n").unwrap();
            let config_path = empty_config_file(dir.path());

            let batch_csv = dir.path().join("jobs.csv");
            std::fs::write(
                &batch_csv,
                format!("format,pdf\nA-EN24,{path}\nB-EN24,{path}\n", path = pdf.to_str().unwrap()),
            )
            .unwrap();

            let args = parse(&[
                "--batch",
                batch_csv.to_str().unwrap(),
                "--formats-directory",
                dir.path().to_str().unwrap(),
                "--target-list",
                "TEST",
                "--config",
                config_path.to_str().unwrap(),
            ]);
            let configs = resolve_configs(args).unwrap();
            assert_eq!(configs.len(), 2);
            assert_eq!(configs[0].format, "A-EN24");
            assert_eq!(configs[1].format, "B-EN24");
        }

        #[test]
        fn a_non_batch_invocation_resolves_to_exactly_one_config() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf = dir.path().join("report.pdf");
            std::fs::write(&pdf, b"%PDF-1.4").unwrap();
            let config_path = empty_config_file(dir.path());
            let args = parse(&[
                "--input",
                pdf.to_str().unwrap(),
                "--format",
                "F",
                "--target-list",
                "TEST",
                "--config",
                config_path.to_str().unwrap(),
            ]);
            let configs = resolve_configs(args).unwrap();
            assert_eq!(configs.len(), 1);
        }
    }

    mod error_propagation {
        use super::*;

        #[test]
        fn an_invalid_cmd_document_specifier_surfaces_as_cli_error_cmd() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let config_path = empty_config_file(dir.path());
            let args = parse(&[
                "--input",
                "a:b:c:d",
                "--target-list",
                "TEST",
                "--format",
                "F",
                "--config",
                config_path.to_str().unwrap(),
            ]);
            let handle = test_log_handle();
            let result = std::panic::catch_unwind(|| execute(args, &handle));
            assert!(result.is_ok(), "must not panic");
            assert!(matches!(result.unwrap(), Err(CliError::Cmd(_))));
        }

        #[test]
        fn a_missing_target_list_surfaces_as_cli_error_validate() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf = dir.path().join("report.pdf");
            std::fs::write(&pdf, b"%PDF-1.4").unwrap();
            let config_path = empty_config_file(dir.path());
            let args = parse(&[
                "--input",
                pdf.to_str().unwrap(),
                "--format",
                "F",
                "--formats-directory",
                dir.path().to_str().unwrap(),
                "--config",
                config_path.to_str().unwrap(),
                // Deliberately no --target-list: `require_target_lists` must reject this.
            ]);
            let result = execute(args, &test_log_handle());
            assert!(matches!(result, Err(CliError::Validate(_))), "got {result:?}");
        }

        #[test]
        fn an_unknown_format_surfaces_as_cli_error_job() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf = dir.path().join("report.pdf");
            std::fs::write(&pdf, b"%PDF-1.4").unwrap();
            std::fs::create_dir_all(dir.path().join("metadata")).unwrap();
            std::fs::write(dir.path().join("metadata/formats.csv"), "Name,Locale,Year,Country,Version\n").unwrap();
            std::fs::write(dir.path().join("metadata/url_mapping.csv"), "Format name,Url\n").unwrap();
            let config_path = empty_config_file(dir.path());

            let args = parse(&[
                "--input",
                pdf.to_str().unwrap(),
                "--format",
                "DOES-NOT-EXIST",
                "--formats-directory",
                dir.path().to_str().unwrap(),
                "--target-list",
                "TEST",
                "--config",
                config_path.to_str().unwrap(),
                // The output path is not decorative here: this is the only test in this module that
                // gets *past* configuration resolution, and so the only one where the CSV
                // destination is settled. Without it the path takes its default — the working
                // directory, which for a test binary is the package root — and the suite would
                // leave a header-only log there on every run.
                "--out",
                dir.path().join("out").to_str().unwrap(),
            ]);
            // The one job of this run is the only one there is, so its failure leaves nothing to
            // write and the run itself fails — with the job's own message, verbatim. A *batch* in
            // which this job were one of many would instead write the others and exit with 3.
            let result = execute(args, &test_log_handle());
            match result {
                Err(CliError::Worker(failure)) => {
                    assert_eq!(failure.index(), 0);
                    assert_eq!(failure.to_string(), "unknown format 'DOES-NOT-EXIST'; the repository declares 0 format(s)");
                }
                other => panic!("got {other:?}"),
            }
        }
    }

    /// One real end-to-end test: resolution, job, and writing to disk. It touches Python, with the
    /// same note as the job tests.
    mod python_boundary {
        use super::*;
        use pretty_assertions::assert_eq;
        use pyo3::prelude::*;

        /// A minimal PDF with text the fixture repository's classifier recognises.
        fn build_minimal_pdf(pdf_path: &std::path::Path) {
            Python::attach(|py| {
                let fitz = PyModule::import(py, "fitz")
                    .expect("PyMuPDF (fitz) must be importable: activate venv/freeports-dev, see AGENTS.md");
                let doc = fitz.call_method0("open").unwrap();
                let page = doc.call_method1("new_page", (-1i64, 200.0f64, 300.0f64)).unwrap();
                page.call_method1("insert_text", ((20.0f64, 50.0f64), "Holdings")).unwrap();
                doc.call_method1("save", (pdf_path.to_str().unwrap(),)).unwrap();
                doc.call_method0("close").unwrap();
            });
        }

        /// A formats repository declaring exactly one format, `A-EN24`. Returns its path.
        fn write_minimal_repo(dir: &std::path::Path) -> std::path::PathBuf {
            let repo = dir.join("formats_repo");
            for (relative, content) in [
                ("metadata/formats.csv", "Name,Locale,Year,Country,Version\nA,EN,24,,\n"),
                ("metadata/url_mapping.csv", "Format name,Url\n"),
                (
                    "content/orchestration/algorithms_schedule.csv",
                    "Format name,Page type,Filter next iteration\nA-EN24,investments,\n",
                ),
                ("content/orchestration/mapping.csv", "ID,Page type\nA-EN24(investments),investments\n"),
                ("content/orchestration/pageclassify_overwrite.csv", "ID\n"),
                (
                    "content/algorithms/structured/page_classify/args.csv",
                    "ID,Header set,Class\nA-EN24/0,\"Arial \"\"^.*$\"\"\",investments\n",
                ),
                (
                    "content/algorithms/structured/investments/args.csv",
                    "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n\
                     A-EN24,Arial,Arial,Arial,1,,,,\n",
                ),
                (
                    "content/algorithms/structured/investments/additional_args.csv",
                    "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous,Interpret dash as zero\n",
                ),
                ("content/algorithms/structured/investments/partial_pipes.csv", "ID,pdf_extract,text_filter,deserialize\n"),
                ("content/algorithms/structured/investments/deselection_lists.csv", "ID,Deselection set\n"),
                ("content/algorithms/semistructured/formats_mapping.csv", "ID,pdf_extract,text_filter,deserialize\n"),
                ("content/algorithms/semistructured/args/pdf_extract.yaml", "{}"),
                ("content/algorithms/semistructured/args/text_filter.yaml", "{}"),
                ("content/algorithms/semistructured/args/deserialize.yaml", "{}"),
            ] {
                let path = repo.join(relative);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, content).unwrap();
            }
            repo
        }

        #[test]
        fn a_full_non_batch_invocation_writes_the_regular_profile_csvs_to_disk() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf_path = dir.path().join("report.pdf");
            build_minimal_pdf(&pdf_path);
            let repo = write_minimal_repo(dir.path());

            let out_dir = dir.path().join("out");
            std::fs::create_dir_all(&out_dir).unwrap();
            let config_path = empty_config_file(dir.path());

            let args = parse(&[
                "--input",
                pdf_path.to_str().unwrap(),
                "--format",
                "A-EN24",
                "--formats-directory",
                repo.to_str().unwrap(),
                "--target-list",
                "TEST",
                "--out",
                out_dir.to_str().unwrap(),
                "--config",
                config_path.to_str().unwrap(),
            ]);

            let completeness = execute(args, &test_log_handle())
                .expect("a fully valid, self-contained invocation must succeed end to end");
            assert_eq!(completeness, RunCompleteness::Everything);
            assert!(out_dir.join("investments.csv").is_file());
            assert!(out_dir.join("funds.csv").is_file());
        }

        /// The whole plan, end to end and at its smallest: a batch of two jobs of which one cannot
        /// even load its format. Before, that one failure returned early and the other job's
        /// results — already extracted, already in memory — were thrown away with it, and nothing
        /// was written at all. Scaled up, that is 903 jobs discarded because of 1.
        #[test]
        fn a_batch_with_one_failing_job_still_writes_the_others_and_reports_not_everything() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf_path = dir.path().join("report.pdf");
            build_minimal_pdf(&pdf_path);
            let repo = write_minimal_repo(dir.path());

            let batch = dir.path().join("batch.csv");
            std::fs::write(
                &batch,
                format!(
                    "pdf,format
{path}:Sound,A-EN24
{path}:Doomed,DOES-NOT-EXIST
",
                    path = pdf_path.to_str().unwrap()
                ),
            )
            .unwrap();

            let out_dir = dir.path().join("out");
            let config_path = empty_config_file(dir.path());
            let args = parse(&[
                "--batch",
                batch.to_str().unwrap(),
                "--formats-directory",
                repo.to_str().unwrap(),
                "--target-list",
                "TEST",
                "--out",
                out_dir.to_str().unwrap(),
                "--config",
                config_path.to_str().unwrap(),
                // Sequential on purpose: the child-process path cannot be exercised from a test
                // binary, which is not the real executable. The integration tests cover that half,
                // and `run_jobs` is written so the two produce the same list of failures.
                "--workers",
                "1",
            ]);

            let completeness = execute(args, &test_log_handle())
                .expect("one failing job out of two must not fail the run");
            assert_eq!(completeness, RunCompleteness::NotEverything);
            assert!(out_dir.join("investments.csv").is_file(), "the results of the sound job must be on disk");
            assert!(out_dir.join("funds.csv").is_file());
        }

        /// And the floor: when *every* job fails there is no partial result to save, so nothing is
        /// written and the run itself fails — with the first failure's own message.
        #[test]
        fn a_batch_in_which_every_job_fails_writes_nothing_and_fails() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf_path = dir.path().join("report.pdf");
            build_minimal_pdf(&pdf_path);
            let repo = write_minimal_repo(dir.path());

            let batch = dir.path().join("batch.csv");
            std::fs::write(
                &batch,
                format!(
                    "pdf,format
{path}:First,NOPE-ONE
{path}:Second,NOPE-TWO
",
                    path = pdf_path.to_str().unwrap()
                ),
            )
            .unwrap();

            let out_dir = dir.path().join("out");
            let config_path = empty_config_file(dir.path());
            let args = parse(&[
                "--batch",
                batch.to_str().unwrap(),
                "--formats-directory",
                repo.to_str().unwrap(),
                "--target-list",
                "TEST",
                "--out",
                out_dir.to_str().unwrap(),
                "--config",
                config_path.to_str().unwrap(),
                "--workers",
                "1",
            ]);

            match execute(args, &test_log_handle()) {
                Err(CliError::Worker(failure)) => assert_eq!(failure.index(), 0, "the first failure in job order"),
                other => panic!("a run that produced nothing must fail, got {other:?}"),
            }
            assert!(!out_dir.join("investments.csv").exists(), "six empty tables are worse than none");
        }
    }
}
