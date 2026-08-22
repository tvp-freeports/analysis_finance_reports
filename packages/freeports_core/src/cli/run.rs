
use std::path::{Path, PathBuf};

use pyo3::prelude::*;

use super::cmd::{self, CmdError};
use super::conf_parse::OutStructureMode;
use super::config_locations::cmd::CliArgs;
use super::freeports_config::{FreeportsConfig, FreeportsConfigError};
use super::{batch, job, output};
use batch::BatchError;
use crate::pyerr::PyStepFailed;

const LOG_CSV_HEADER: &str = "Page,Matched Company,Company,Field name,Row,Column,Message\n";

/// Everything [`resolve_jobs`] can fail with — each inner error already has its own descriptive
/// `Display` (including a "which stage" prefix, e.g. `command-line arguments: `/
/// `batch job configuration: `), so this just delegates, adding nothing of its own.
#[derive(Debug)]
pub enum ResolveJobsError {
    Cmd(CmdError),
    Config(FreeportsConfigError),
    Batch(BatchError),
}

impl std::fmt::Display for ResolveJobsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveJobsError::Cmd(e) => write!(f, "{e}"),
            ResolveJobsError::Config(e) => write!(f, "{e}"),
            ResolveJobsError::Batch(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ResolveJobsError {}

impl From<CmdError> for ResolveJobsError {
    fn from(e: CmdError) -> Self {
        ResolveJobsError::Cmd(e)
    }
}
impl From<FreeportsConfigError> for ResolveJobsError {
    fn from(e: FreeportsConfigError) -> Self {
        ResolveJobsError::Config(e)
    }
}
impl From<BatchError> for ResolveJobsError {
    fn from(e: BatchError) -> Self {
        ResolveJobsError::Batch(e)
    }
}

/// Resolves either one job (no `--batch`) or every row of the batch file, sharing the same
/// default→file→env→cmd base config either way — mirrors `cmd()` + (for batch mode)
/// `batch_job_confs`. No `py: Python<'_>` parameter: neither branch needs one directly anymore —
/// [`FreeportsConfig::build`]/[`batch::load_batch_jobs`] attach their own where they actually
/// touch Python (see their doc comments).
fn resolve_jobs(cli_args: CliArgs) -> Result<Vec<FreeportsConfig>, ResolveJobsError> {
    let merged = cmd::resolve_partial_config(cli_args)?;
    match &merged.batch_file {
        None => {
            let config = FreeportsConfig::build(merged)?;
            Ok(vec![config])
        }
        Some(batch_file) => {
            let batch_file = batch_file.clone();
            Ok(batch::load_batch_jobs(&merged, &batch_file)?)
        }
    }
}

/// Everything [`run_jobs`] can fail with. `Step`'s and `Write`'s `Display` are both deliberately
/// empty: a [`PyStepFailed`] means a `PyErr` was already printed in full right where it was
/// generated (inside [`job::run_job`]), and an [`output::WriteResultsFailed`] means
/// [`output::write_results`] already printed its own failure the same way — either a genuine
/// `PyErr` bubbled from `transform_to_files_schema`, or a plain `WriteFilesError` from
/// `write_files` (see `output::WriteResultsFailed`'s own doc comment for why that's not also a
/// `PyStepFailed`). Either way there's nothing left to add, and neither this type nor
/// [`ExecuteError`] above it ever prepends a prefix of their own, so that emptiness reaches
/// `main()` cleanly with nothing dangling in front of it.
///
/// [`Log`](RunJobsError::Log) is the one exception, and deliberately not opaque: a `.log.csv`
/// mkdir/open/write failure is a plain Rust `io::Error` that never touches Python at all, so
/// there's no "already printed elsewhere" to defer to — its `Display` carries the real message,
/// the same class of thing `FreeportsConfigError`/`CmdError` already are elsewhere in this crate.
#[derive(Debug)]
pub enum RunJobsError {
    NoJobs,
    Step(PyStepFailed),
    Write(output::WriteResultsFailed),
    Log(std::io::Error),
}

impl std::fmt::Display for RunJobsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunJobsError::NoJobs => write!(f, "no jobs to run"),
            RunJobsError::Step(_) => Ok(()),
            RunJobsError::Write(_) => Ok(()),
            RunJobsError::Log(e) => write!(f, "opening `.log.csv`: {e}"),
        }
    }
}

impl std::error::Error for RunJobsError {}

impl From<PyStepFailed> for RunJobsError {
    fn from(e: PyStepFailed) -> Self {
        RunJobsError::Step(e)
    }
}

impl From<output::WriteResultsFailed> for RunJobsError {
    fn from(e: output::WriteResultsFailed) -> Self {
        RunJobsError::Write(e)
    }
}

/// Mirrors `main()`: runs every job, accumulating `DocumentResults` across all of them (matching
/// `results_documents.extend(...)`), then writes the combined output once. `OUT_PATH`/
/// `OUT_PROFILE`/`OUT_FLAGS` are the same across every job (part of the shared base config, never
/// overridden per batch row), so it's safe to read them from the first job.
///
/// `pub(crate)`, not private: [`cli::py_run_job`](super::py_run_job)'s bridge calls this directly
/// (bypassing [`resolve_jobs`]'s `CliArgs`-based path, which doesn't apply there — there's no argv
/// to parse for an in-process Python caller).
///
/// **`.log.csv` header/mkdir** (mirrors `main()`'s own `OUT_PATH.mkdir(exist_ok=True)` +
/// `csv.writer(...).writerow([...])`, `main.py:230-244`): computed once, before the job loop, from
/// the same shared `out_path`/`out_profile` every job already reads below. `log_dir` mirrors
/// `_main_job`'s own conditional (`main.py:124-128`): `out_path`'s parent in `SingleFile` mode
/// (`FreeportsConfig::build`'s `out_path_single_file` step has already turned `out_path` into a
/// `.csv` *file* path by the time this runs), `out_path` itself otherwise. Every job gets the same
/// `log_dir` passed down to [`job::run_job`], which attaches/detaches its own per-job handler.
pub(crate) fn run_jobs(jobs: Vec<FreeportsConfig>) -> Result<(), RunJobsError> {
    let Some(first) = jobs.first() else {
        return Err(RunJobsError::NoJobs);
    };
    let is_batch = first.batch_file.is_some();
    let out_path = first.out_path.clone();
    let out_profile = first.out_profile;
    let out_flags = first.out_flags;

    let log_dir = if out_profile == OutStructureMode::SingleFile {
        out_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
    } else {
        out_path.clone()
    };
    std::fs::create_dir_all(&log_dir).map_err(RunJobsError::Log)?;
    std::fs::write(log_dir.join(".log.csv"), LOG_CSV_HEADER).map_err(RunJobsError::Log)?;

    // TODO(Fase E, punto 3d-iv): parallelize this loop once the process-vs-thread design is
    // worked out (see `agent-memory/rust-native-binary-plan.md`'s risk notes on the GIL/embedded
    // interpreter). Sequential for now, per explicit user direction (2026-08-20).
    let mut all_results = Vec::new();
    for job_config in &jobs {
        all_results.extend(job::run_job(job_config, &log_dir)?);
    }

    output::write_results(all_results, &out_path, out_profile, out_flags, is_batch)?;
    Ok(())
}

/// Everything [`execute`] can fail with — no prefix added at this level either, same reasoning as
/// [`ResolveJobsError`].
#[derive(Debug)]
pub enum ExecuteError {
    Resolve(ResolveJobsError),
    Run(RunJobsError),
}

impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecuteError::Resolve(e) => write!(f, "{e}"),
            ExecuteError::Run(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ExecuteError {}

impl From<ResolveJobsError> for ExecuteError {
    fn from(e: ResolveJobsError) -> Self {
        ExecuteError::Resolve(e)
    }
}
impl From<RunJobsError> for ExecuteError {
    fn from(e: RunJobsError) -> Self {
        ExecuteError::Run(e)
    }
}

/// Full CLI run: parsed `CliArgs` through written output. `src/main.rs` calls only this — see its
/// module doc for how it turns an `Err` here into a printed message (only when one is actually
/// still owed; see [`RunJobsError::Step`]) and a process exit. No `py: Python<'_>` parameter:
/// neither [`resolve_jobs`] nor [`run_jobs`] needs one from its caller — each attaches its own
/// exactly where it's actually needed (see their own doc comments) — so `main.rs` doesn't need to
/// acquire one just to hand it down through here.
pub fn execute(cli_args: CliArgs) -> Result<(), ExecuteError> {
    let jobs = resolve_jobs(cli_args)?;
    run_jobs(jobs)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use pretty_assertions::assert_eq;
    use super::super::config_locations::job::JobConfigError;
    use super::super::conf_parse::{DocumentSpec, OutFlags, OutStructureMode, Verbosity};
    use std::path::{Path, PathBuf};

    fn parse(argv: &[&str]) -> CliArgs {
        let mut full = vec!["freeports"];
        full.extend_from_slice(argv);
        CliArgs::parse_from(full)
    }

    /// Same isolation as `cmd.rs`'s own tests (real machine state under `XDG_CONFIG_HOME` would
    /// otherwise leak in) — see that module's `resolve_isolated` for why.
    fn resolve_jobs_isolated(argv: &[&str]) -> Result<Vec<FreeportsConfig>, ResolveJobsError> {
        let _env_lock = super::super::config_locations::env::ENV_LOCK.lock().unwrap();
        let empty_xdg = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("XDG_CONFIG_HOME", empty_xdg.path()) };
        Python::attach(crate::test_support::ensure_freeports_imported);
        let result = resolve_jobs(parse(argv));
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        result
    }

    #[test]
    fn no_batch_file_resolves_to_exactly_one_job() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("report.pdf");
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();

        let jobs = resolve_jobs_isolated(&[
            "--input",
            pdf.to_str().unwrap(),
            "--format",
            "my-format",
            "--target-list",
            "TEST",
            "--db-directory",
            dir.path().to_str().unwrap(),
            "--formats-directory",
            dir.path().to_str().unwrap(),
            "--out",
            dir.path().to_str().unwrap(),
        ])
        .unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].format.as_deref(), Some("my-format"));
        assert_eq!(jobs[0].batch_file, None);
    }

    #[test]
    fn batch_file_resolves_to_one_job_per_row_overlaid_on_the_shared_base_config() {
        let dir = tempfile::tempdir().unwrap();
        let pdf_a = dir.path().join("a.pdf");
        let pdf_b = dir.path().join("b.pdf");
        std::fs::write(&pdf_a, b"%PDF-1.4").unwrap();
        std::fs::write(&pdf_b, b"%PDF-1.4").unwrap();

        let batch_file = dir.path().join("batch.csv");
        std::fs::write(
            &batch_file,
            format!("format,input\nrow-format-a,{}\nrow-format-b,{}\n", pdf_a.display(), pdf_b.display()),
        )
        .unwrap();

        let jobs = resolve_jobs_isolated(&[
            "--batch",
            batch_file.to_str().unwrap(),
            "--target-list",
            "TEST",
            "--db-directory",
            dir.path().to_str().unwrap(),
            "--formats-directory",
            dir.path().to_str().unwrap(),
            "--out",
            dir.path().to_str().unwrap(),
        ])
        .unwrap();

        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].format.as_deref(), Some("row-format-a"));
        assert_eq!(jobs[1].format.as_deref(), Some("row-format-b"));
        // Base config (not overridden per row) still applies to every job.
        assert_eq!(jobs[0].target_lists, vec!["TEST".to_string()]);
        assert_eq!(jobs[1].target_lists, vec!["TEST".to_string()]);
        assert!(jobs[0].batch_file.is_some());
    }

    #[test]
    fn unresolvable_config_surfaces_the_first_missing_required_field() {
        assert!(matches!(
            resolve_jobs_isolated(&[]),
            Err(ResolveJobsError::Config(FreeportsConfigError::MissingTargetLists))
        ));
    }

    #[test]
    fn nonexistent_batch_file_surfaces_as_a_batch_csv_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_jobs_isolated(&[
            "--batch",
            dir.path().join("does-not-exist.csv").to_str().unwrap(),
            "--target-list",
            "TEST",
            "--db-directory",
            dir.path().to_str().unwrap(),
            "--formats-directory",
            dir.path().to_str().unwrap(),
            "--out",
            dir.path().to_str().unwrap(),
        ]);
        assert!(matches!(result, Err(ResolveJobsError::Batch(BatchError::Csv(_)))));
    }

    #[test]
    fn batch_row_with_an_unknown_column_surfaces_as_a_batch_row_error() {
        let dir = tempfile::tempdir().unwrap();
        let batch_file = dir.path().join("batch.csv");
        std::fs::write(&batch_file, "format,bogus_column\nsome-format,whatever\n").unwrap();

        let result = resolve_jobs_isolated(&[
            "--batch",
            batch_file.to_str().unwrap(),
            "--target-list",
            "TEST",
            "--db-directory",
            dir.path().to_str().unwrap(),
            "--formats-directory",
            dir.path().to_str().unwrap(),
            "--out",
            dir.path().to_str().unwrap(),
        ]);
        assert!(matches!(result, Err(ResolveJobsError::Batch(BatchError::Row(JobConfigError::UnknownColumn(_))))));
    }

    // ============================================================
    // `run_jobs`'s `.log.csv` wiring (pytest-plugin-rust-swap-implementation-plan.md, File 3).
    // These tests call `run_jobs` directly with a hand-built `FreeportsConfig` (every field is
    // `pub`, see `freeports_config.rs`), bypassing `FreeportsConfig::build`'s own validators
    // entirely -- deliberately, so each test can isolate exactly the `.log.csv`
    // header/mkdir/log_dir behavior under test without needing a real, fully-validated config.
    // Every config below gives `input_reports` a single document whose local `path` does not
    // exist and has no `url` -- `job::run_job`'s own `resolve_documents` step fails fast on that
    // (`ResolveDocumentsError::MissingPath`, well before ever touching `formats_repo_path`/
    // `Algorithm::load`), so `run_jobs` overall returns `Err(RunJobsError::Step(_))` for every one
    // of these -- expected and irrelevant to what's being asserted: the `.log.csv` header/mkdir
    // logic runs *before* the per-job loop (see `run_jobs`'s own doc comment / the plan's File 3),
    // so the header's presence and location don't depend on any job actually succeeding.
    // ============================================================

    fn config_with_a_doomed_job(dir: &Path, out_path: PathBuf, out_profile: OutStructureMode) -> FreeportsConfig {
        let missing_doc_path = dir.join("does_not_exist.pdf");
        let doc = DocumentSpec::new(None, Some(missing_doc_path), Some("missing".to_string())).unwrap();
        FreeportsConfig {
            verbosity: Verbosity::new(2).unwrap(),
            n_workers: 1,
            batch_file: None,
            save_pdf: false,
            input_reports: vec![doc],
            format: Some("whatever-format".to_string()),
            config_file: None,
            target_lists: vec!["TEST".to_string()],
            out_profile,
            out_flags: OutFlags::NONE,
            out_path,
            input_db_path: dir.to_path_buf(),
            formats_repo_path: dir.to_path_buf(),
        }
    }

    const LOG_CSV_HEADER: &str = "Page,Matched Company,Company,Field name,Row,Column,Message";

    #[test]
    fn run_jobs_writes_log_csv_header_directly_under_out_path_in_regular_mode() {
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("out");
        std::fs::create_dir_all(&out_dir).unwrap();
        let config = config_with_a_doomed_job(dir.path(), out_dir.clone(), OutStructureMode::Regular);

        let _ = run_jobs(vec![config]);

        let content = std::fs::read_to_string(out_dir.join(".log.csv")).unwrap();
        assert_eq!(content.lines().next().unwrap(), LOG_CSV_HEADER);
    }

    #[test]
    fn run_jobs_writes_log_csv_header_in_out_paths_parent_in_single_file_mode() {
        let dir = tempfile::tempdir().unwrap();
        // Mirrors what `FreeportsConfig::build`'s own `out_path_single_file` step would have
        // already turned `out_path` into by the time `run_jobs` ever sees it: a `.csv` *file*
        // path, not a directory.
        let out_csv = dir.path().join("results.csv");
        let config = config_with_a_doomed_job(dir.path(), out_csv, OutStructureMode::SingleFile);

        let _ = run_jobs(vec![config]);

        // log_dir = out_path.parent() in SINGLE_FILE mode -- i.e. `dir`, not `dir/results.csv`.
        let content = std::fs::read_to_string(dir.path().join(".log.csv")).unwrap();
        assert_eq!(content.lines().next().unwrap(), LOG_CSV_HEADER);
    }

    #[test]
    fn run_jobs_creates_the_log_dir_when_it_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("nested").join("out"); // does not exist yet
        assert!(!out_dir.exists());
        let config = config_with_a_doomed_job(dir.path(), out_dir.clone(), OutStructureMode::Regular);

        let _ = run_jobs(vec![config]);

        assert!(out_dir.join(".log.csv").exists());
    }

    #[test]
    fn run_jobs_truncates_pre_existing_log_csv_content_on_each_run() {
        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("out");
        std::fs::create_dir_all(&out_dir).unwrap();
        std::fs::write(out_dir.join(".log.csv"), "leftover content from a previous run\n").unwrap();
        let config = config_with_a_doomed_job(dir.path(), out_dir.clone(), OutStructureMode::Regular);

        let _ = run_jobs(vec![config]);

        let content = std::fs::read_to_string(out_dir.join(".log.csv")).unwrap();
        assert!(!content.contains("leftover content"), "expected the pre-existing content to be truncated away, got:\n{content}");
        assert_eq!(content.lines().next().unwrap(), LOG_CSV_HEADER);
    }

    #[test]
    fn run_jobs_surfaces_an_io_failure_opening_log_csv_as_run_jobs_error_log() {
        let dir = tempfile::tempdir().unwrap();
        // A plain file where `create_dir_all(log_dir)` needs a directory -- `log_dir` becomes
        // `blocker/subdir`, and `blocker` itself is a regular file, so creating a directory under
        // it must fail.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let out_path = blocker.join("subdir");
        let config = config_with_a_doomed_job(dir.path(), out_path, OutStructureMode::Regular);

        let result = run_jobs(vec![config]);

        assert!(matches!(result, Err(RunJobsError::Log(_))), "expected Err(RunJobsError::Log(_)), got {result:?}");
    }

    #[test]
    fn run_jobs_error_log_display_is_not_empty_unlike_step_and_write() {
        // Pins the plan's explicit contrast: `RunJobsError::Log`'s `io::Error` message is real and
        // non-empty (unlike `Step`/`Write`, whose `Display` is deliberately empty because their
        // failure was already printed in full elsewhere) -- this is what lets
        // `cli::py_run_job::py_run_job`'s error-mapping closure surface a real message for this
        // one failure source instead of falling back to its generic text.
        let io_err = std::io::Error::other("disk full");
        let err = RunJobsError::Log(io_err);
        assert!(!err.to_string().is_empty());
    }
}
