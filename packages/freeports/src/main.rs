//! Entry point of the `freeports` binary.
//!
//! Four steps and no domain logic: parse the arguments, bring logging up *before* anything else
//! can want to log, hand over to `cli::run::execute`, map its answer onto an exit code.
//! Everything else — resolving the configuration, running the jobs, writing the results — lives
//! in `cli::run`, so that an embedding program can do the same work without going through `main`.
//!
//! # The three exit codes
//!
//! | Code | Meaning |
//! |---|---|
//! | `0` | every job produced its results, and they were written |
//! | `1` | the run produced nothing: an illegal configuration, or no job succeeded |
//! | `3` | the results were written, but **not everything was read**: jobs or documents were skipped |
//!
//! `3` is deliberately the same number `freeports-validate check-grants` uses for the same idea —
//! *nothing turned out to be wrong, and I could not look at everything*. Two commands of one suite
//! must not ask their user to remember two tables.
//!
//! Rows and pages skipped do **not** raise the code: they are contained at their own level, happen
//! on ordinary runs by the dozen, and already have their own end-of-run summary. See
//! [`RunCompleteness`](freeports::cli::run::RunCompleteness).

use clap::Parser;

use freeports::cli::config_locations::cmd::CliArgs;
use freeports::cli::run::RunCompleteness;
use freeports::cli::{run, worker};
use freeports::core::tracing_setup::{self, Verbosity, panic_message};

fn main() {
    let args = CliArgs::parse();

    // Checked before anything else: a worker process resolves no configuration and starts no
    // logging of its own in the current directory. Both were already decided by the parent, and
    // reach the child through the request file named on the command line.
    if let Some(request_path) = args.internal_worker.as_deref() {
        if let Err(e) = worker::execute(std::path::Path::new(request_path)) {
            // Not `tracing::error!`: logging may never have started here, since an unreadable
            // request is one of the ways this can fail. The parent sees this line on the stderr it
            // shares, and recognises the failure anyway from the missing report file.
            eprintln!("freeports worker: {e}");
            std::process::exit(worker::PROTOCOL_FAILURE_EXIT_CODE);
        }
        return;
    }

    let verbosity = Verbosity::from_verbose_and_quiet_counts(args.verbose, args.quiet);
    let log_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let log_handle = match tracing_setup::init(verbosity, &log_dir) {
        Ok(handle) => handle,
        Err(e) => {
            eprintln!("freeports: cannot initialize logging: {e}");
            std::process::exit(1);
        }
    };

    // From here on panics are routed through `tracing`: the default hook's direct write to stderr
    // is replaced by a `trace!` carrying the location and the backtrace, and whoever catches the
    // panic reports it once, inside the spans that say where the run was. Installed after logging,
    // because the hook logs.
    tracing_setup::install_panic_hook();

    // The last net of the four. `Algorithm` contains a panic at the page, `worker::execute`
    // contains a child's at the job, and this one contains everything else — configuration
    // resolution, the formats repository, the writing of the results — so that a panic anywhere
    // still closes the log files below instead of taking them down with it.
    let run_result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run::execute(args, &log_handle)
    })) {
        Ok(result) => result,
        Err(payload) => {
            let message = panic_message(payload.as_ref());
            tracing::error!("the engine panicked outside any page: {message}");
            eprintln!("freeports: {message}");
            // Closed before exiting, and the failure of the close deliberately ignored: the run's
            // diagnostic rows are held in memory until here, so skipping this would lose the log of
            // precisely the run whose log is worth reading.
            let _ = log_handle.close();
            std::process::exit(1);
        }
    };
    // Always attempted, whatever `run_result` says: the diagnostic rows of a job that failed are
    // exactly the ones worth having on disk.
    let close_result = log_handle.close();

    let completeness = match run_result {
        Ok(completeness) => completeness,
        Err(e) => {
            // WARNING: `close()` already ran on the line above, so this event fires *after* the CSV
            // buffer has been flushed. If it ever gains a tagged field (`page`, `coord_ref_*`,
            // `coord_*`) that field will never reach `.log.csv`, only stderr and the structured log.
            // Harmless as written; adding one means first moving this call above `close()`.
            //
            // It also deliberately omits `{e}`: whichever area failed has already logged the same
            // error with far more context, so this line only records the audit-trail fact that the
            // process is exiting because of it. The `eprintln!` below still shows it to the user.
            tracing::error!("freeports is exiting due to the error above");
            eprintln!("freeports: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = close_result {
        eprintln!("freeports: cannot flush the log files: {e}");
        std::process::exit(1);
    }
    // The results are on disk either way; the code is what says whether they are all of them.
    if completeness == RunCompleteness::NotEverything {
        std::process::exit(3);
    }
}
