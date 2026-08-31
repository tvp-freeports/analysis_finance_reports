//! The adapter from a resolved configuration to writing the output files.
//!
//! Accumulates a job's results — or those of every job of a batch, concatenated — and writes them
//! to disk. Pure glue: it reimplements none of the rules of accumulation or writing, it passes the
//! right parameters in the right order.

use crate::cli::freeports_config::FreeportsConfig;
use crate::core::algorithm::DocumentOutcome;
use crate::output::routines::accumulate::{AccumulateError, accumulate};
use crate::output::routines::write::{OutStructureMode, WriteFilesError, write_files};
use crate::core::tracing_setup::log_error;

#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error(transparent)]
    Accumulate(#[from] AccumulateError),
    #[error(transparent)]
    Write(#[from] WriteFilesError),
}

/// The directory `.log.csv` goes in for a resolved configuration: **beside the outputs**.
///
/// In the single-file profile the output path is the file rather than the directory, so its parent
/// is taken. One function serves both entry points — the command line and the Python API — which
/// previously duplicated the choice: diverging here would mean the same job writing its log in two
/// different places depending on how it was launched.
pub fn log_csv_dir(config: &FreeportsConfig) -> std::path::PathBuf {
    if config.out_profile == OutStructureMode::SingleFile {
        config.out_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf()
    } else {
        config.out_path.clone()
    }
}

/// Opens the output span around the write and logs the outcome exactly once. The output path is the
/// coordinate identifying this write, so it goes on the span rather than on the individual events.
pub fn write_results(config: &FreeportsConfig, outcomes: &[DocumentOutcome]) -> Result<(), OutputError> {
    let span = tracing::info_span!("output", out_path = %config.out_path.display());
    let _guard = span.enter();

    let result = write_results_impl(config, outcomes);
    match &result {
        Ok(()) => tracing::info!(document_count = outcomes.len(), "wrote results to disk"),
        Err(e) => tracing::error!(error = log_error(e), "failed to write results: {e}"),
    }
    result
}

/// `accumulate(outcomes)` poi `write_files(&tables, &config.out_path, config.out_profile,
/// config.out_flags)` -- nessun'altra logica.
fn write_results_impl(config: &FreeportsConfig, outcomes: &[DocumentOutcome]) -> Result<(), OutputError> {
    let tables = accumulate(outcomes)?;
    write_files(&tables, &config.out_path, config.out_profile, config.out_flags)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::cli::parallelism_config::ParallelismConfig;
    use super::*;
    use crate::cli::freeports_config::FreeportsConfig;
    use crate::core::algorithm::{DocumentOutcome, PageOutcome};
    use crate::core::pipeline::Extracted;
    use crate::core::schedule::PageClass;
    use crate::core::tracing_setup::Verbosity;
    use crate::output::classes::fund::Fund;
    use crate::output::routines::write::{OutFlags, OutStructureMode};

    fn config_at(dir: &std::path::Path, profile: OutStructureMode) -> FreeportsConfig {
        FreeportsConfig {
            verbosity: Verbosity::Warn,
            reports: vec![],
            target_lists: vec![],
            format: "FMT".to_string(),
            out_path: dir.to_path_buf(),
            out_profile: profile,
            out_flags: OutFlags::default(),
            parallelism: ParallelismConfig::SEQUENTIAL,
            batch_file: None,
            save_pdf: true,
            formats_repo_path: None,
            input_db_path: None,
            config_file: None,
        }
    }

    mod happy_path {
        use super::*;

        #[test]
        fn empty_outcomes_still_produce_the_regular_profile_files_header_only() {
            let dir = tempfile::tempdir().unwrap();
            let config = config_at(dir.path(), OutStructureMode::Regular);
            write_results(&config, &[]).unwrap();
            assert!(dir.path().join("investments.csv").is_file());
            assert!(dir.path().join("funds.csv").is_file());
        }

        #[test]
        fn accumulated_results_flow_through_into_the_written_csv() {
            let dir = tempfile::tempdir().unwrap();
            let config = config_at(dir.path(), OutStructureMode::Regular);
            let outcomes = vec![DocumentOutcome {
                id: "Report A".into(),
                format: "FMT".into(),
                pages: vec![PageOutcome {
                    page: 1,
                    class: PageClass::new("fund_info"),
                    results: vec![Extracted::Fund(Fund::new("Alpha Fund"))],
                }],
            }];
            write_results(&config, &outcomes).unwrap();
            let content = std::fs::read_to_string(dir.path().join("funds.csv")).unwrap();
            assert!(content.contains("ALPHA FUND"), "expected the fund's normalized name in funds.csv, got:\n{content}");
        }

        #[test]
        fn out_profile_and_out_flags_from_the_config_are_honored() {
            let dir = tempfile::tempdir().unwrap();
            let config = config_at(dir.path().join("out.csv").as_path(), OutStructureMode::SingleFile);
            write_results(&config, &[]).unwrap();
            assert!(dir.path().join("out.csv").is_file());
            assert!(!dir.path().join("investments.csv").exists(), "SingleFile must not also produce the Regular layout");
        }
    }

    mod error_propagation {
        use super::*;

        #[test]
        fn a_write_failure_is_wrapped_as_output_error_write() {
            // `SingleFile` writes to `out_path` treated as a *file* path; giving it a path whose
            // parent does not exist forces the underlying `write_files` I/O to fail, which must
            // surface here as `OutputError::Write`, not a panic.
            let dir = tempfile::tempdir().unwrap();
            let config = config_at(&dir.path().join("missing_subdir").join("out.csv"), OutStructureMode::SingleFile);
            let result = std::panic::catch_unwind(|| write_results(&config, &[]));
            assert!(result.is_ok(), "must not panic");
            assert!(matches!(result.unwrap(), Err(OutputError::Write(_))));
        }
    }
}
