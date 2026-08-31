//! Entry point del binario `freeports`.
//!
//! `M9-implementation-plan.md` §2/§3 passo 15: parsa `CliArgs` (clap), inizializza `tracing_setup`
//! "presto" (prima di ogni logica di dominio), chiama `cli::run::execute` e mappa un eventuale
//! errore su un exit code non-zero -- niente logica di dominio qui, tutta l'orchestrazione vive in
//! `cli::run`.

use clap::Parser;

use freeports::cli::config_locations::cmd::CliArgs;
use freeports::cli::{run, worker};
use freeports::core::tracing_setup::{self, Verbosity};

fn main() {
    let args = CliArgs::parse();

    // P1 (`agent-memory/P1-implementation-plan.md` §2): prima di ogni altra cosa, perche' il modo
    // worker non risolve alcuna configurazione e non inizializza il logging nella cwd -- entrambe le
    // cose gliele dice il file di richiesta che il padre gli ha scritto.
    if let Some(request_path) = args.internal_worker.as_deref() {
        if let Err(e) = worker::execute(std::path::Path::new(request_path)) {
            // Non `tracing::error!`: qui il logging puo' non essere mai stato inizializzato (la
            // richiesta illeggibile e' proprio uno dei modi di fallire). Il padre legge questa riga
            // dallo stderr ereditato, e riconosce comunque il fallimento dal referto mancante.
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
    // Always attempted, regardless of `run_result`: the diagnostic rows of a failed job are the
    // most useful ones to have on disk (`L1-implementation-plan.md` §2.4).
    let close_result = log_handle.close();

    if let Err(e) = run_result {
        // NOTE (critic 2026-08-29, `L1-implementation-plan.md` §2.4 point 2): `close()` already
        // ran on the line above, so this `tracing::error!` fires *after* the CSV buffer has been
        // flushed -- if this event ever gains a tagged field (page/coord_ref_*/coord_*), it will
        // NEVER reach `.log.csv`, only stderr/freeports.log. Harmless today (this event carries
        // no tagged field), but do not add one here without first moving this log before the
        // `close()` call above.
        //
        // Deliberately does not repeat `{e}` here (critic finding, `L2-implementation-plan.md`
        // "cli" sweep): the failing area (`cmd.rs`/`env.rs`/`file.rs`/`batch.rs`/
        // `freeports_config.rs`/`job.rs`/`output.rs`) already logged the same error with full
        // context, so this line only records the audit-trail fact that the process is exiting
        // because of it -- the `eprintln!` right below still shows `{e}` to the user on stderr.
        tracing::error!("freeports is exiting due to the error above");
        eprintln!("freeports: {e}");
        std::process::exit(1);
    }
    if let Err(e) = close_result {
        eprintln!("freeports: cannot flush the log files: {e}");
        std::process::exit(1);
    }
}
