//! Entry point of the `freeports` binary.
//!
//! Four steps and no domain logic: parse the arguments, bring logging up *before* anything else
//! can want to log, hand over to `cli::run::execute`, map a failure onto a non-zero exit code.
//! Everything else — resolving the configuration, running the jobs, writing the results — lives
//! in `cli::run`, so that an embedding program can do the same work without going through `main`.

use clap::Parser;

use freeports::cli::config_locations::cmd::CliArgs;
use freeports::cli::{run, worker};
use freeports::core::tracing_setup::{self, Verbosity};

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

    let run_result = run::execute(args, &log_handle);
    // Always attempted, whatever `run_result` says: the diagnostic rows of a job that failed are
    // exactly the ones worth having on disk.
    let close_result = log_handle.close();

    if let Err(e) = run_result {
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
    if let Err(e) = close_result {
        eprintln!("freeports: cannot flush the log files: {e}");
        std::process::exit(1);
    }
}
