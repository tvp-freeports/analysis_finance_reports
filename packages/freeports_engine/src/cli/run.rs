//! Ties `cli::cmd`'s config resolution to `job`/`batch`/`output` execution — the whole CLI run
//! from parsed `CliArgs` through written output, mirroring `main.py`'s `main()` (single job) and
//! `_legacy_main`'s batch dispatch. Kept here rather than in `src/main.rs` (the `freeports`
//! binary's own entry point) specifically so `cargo test --lib` covers it like everything else in
//! this crate — `main.rs` calls only [`execute`] and handles the resulting `Err` by printing and
//! exiting the process, the one piece of this that genuinely can't be unit-tested (it kills the
//! test process) and has to stay in the binary.
//!
//! [`resolve_jobs`] is covered below; [`run_jobs`]/[`execute`] are not, deliberately, for the same
//! reason `job.rs`/`batch.rs`/`output.rs` themselves have no `#[cfg(test)]` module — exercising
//! them means running a real `Algorithm` against a real formats-repo entry, which is exactly what
//! `pipeline.rs`'s own extensive test suite already covers, plus the real end-to-end `freeports`
//! binary runs and the 259-fixture Python suite this whole port is verified against (see
//! `agent-memory/rust-native-binary-plan.md`, Fase E, punto 3d) — a from-scratch fixture here would
//! duplicate that coverage rather than add to it.

use pyo3::prelude::*;

use super::cmd::{self, CmdError};
use super::cmd_config::CliArgs;
use super::freeports_config::{FreeportsConfig, FreeportsConfigError};
use super::{batch, job, output};
use batch::BatchError;
use job::PyStepFailed;

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

/// Everything [`run_jobs`] can fail with. `Step`'s `Display` is deliberately empty: a
/// [`PyStepFailed`] means a `PyErr` was already printed in full right where it was generated
/// (inside [`job::run_job`]/[`output::write_results`]) — there's nothing left to add, and neither
/// this type nor [`ExecuteError`] above it ever prepends a prefix of their own, so that emptiness
/// reaches `main()` cleanly with nothing dangling in front of it.
#[derive(Debug)]
pub enum RunJobsError {
    NoJobs,
    Step(PyStepFailed),
}

impl std::fmt::Display for RunJobsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunJobsError::NoJobs => write!(f, "no jobs to run"),
            RunJobsError::Step(_) => Ok(()),
        }
    }
}

impl std::error::Error for RunJobsError {}

impl From<PyStepFailed> for RunJobsError {
    fn from(e: PyStepFailed) -> Self {
        RunJobsError::Step(e)
    }
}

/// Mirrors `main()`: runs every job, accumulating `DocumentResults` across all of them (matching
/// `results_documents.extend(...)`), then writes the combined output once. `OUT_PATH`/
/// `OUT_PROFILE`/`OUT_FLAGS` are the same across every job (part of the shared base config, never
/// overridden per batch row), so it's safe to read them from the first job.
///
/// Unlike `main.py`, there's no separate `mkdir(exist_ok=True)` step here: `FreeportsConfig::build`
/// already validated `OUT_PATH`'s parent exists (`out_path_exists`), and `write_regular`/
/// `write_structured` (Fase C) already create their own output directory internally — the
/// original needed its own `mkdir` only because it juggled a pre-validation raw dict alongside a
/// separately-reconstructed validated `FreeportsConfig`; this port only ever has the validated one.
///
/// No `py: Python<'_>` parameter, and no `Python::attach` held across the loop either: every
/// job's results now come back as owned `Py<PyAny>` (see `job::run_job`'s own doc comment), so
/// this function only needs a token right at the end, to rebind them all and hand them to
/// `output::write_results` in one call — neither [`execute`] nor `main.rs` above it ever touches
/// a Python object themselves, so neither has to hold one just to forward it down here.
fn run_jobs(jobs: Vec<FreeportsConfig>) -> Result<(), RunJobsError> {
    let Some(first) = jobs.first() else {
        return Err(RunJobsError::NoJobs);
    };
    let is_batch = first.batch_file.is_some();
    let out_path = first.out_path.clone();
    let out_profile = first.out_profile;
    let out_flags = first.out_flags;

    // TODO(Fase E, punto 3d-iv): parallelize this loop once the process-vs-thread design is
    // worked out (see `agent-memory/rust-native-binary-plan.md`'s risk notes on the GIL/embedded
    // interpreter). Sequential for now, per explicit user direction (2026-08-20).
    let mut all_results = Vec::new();
    for job_config in &jobs {
        all_results.extend(job::run_job(job_config)?);
    }

    Python::attach(|py| {
        let all_results = all_results.into_iter().map(|r| r.into_bound(py)).collect();
        Ok(output::write_results(py, all_results, &out_path, out_profile, out_flags, is_batch)?)
    })
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
    use super::super::job_config::JobConfigError;

    fn parse(argv: &[&str]) -> CliArgs {
        let mut full = vec!["freeports"];
        full.extend_from_slice(argv);
        CliArgs::parse_from(full)
    }

    /// Same isolation as `cmd.rs`'s own tests (real machine state under `XDG_CONFIG_HOME` would
    /// otherwise leak in) — see that module's `resolve_isolated` for why.
    fn resolve_jobs_isolated(argv: &[&str]) -> Result<Vec<FreeportsConfig>, ResolveJobsError> {
        let _env_lock = super::super::env_config::ENV_LOCK.lock().unwrap();
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
}
