//! Entry point del binario `freeports`.
//!
//! `M9-implementation-plan.md` §2/§3 passo 15: parsa `CliArgs` (clap), inizializza `tracing_setup`
//! "presto" (prima di ogni logica di dominio), chiama `cli::run::execute` e mappa un eventuale
//! errore su un exit code non-zero -- niente logica di dominio qui, tutta l'orchestrazione vive in
//! `cli::run`.

use clap::Parser;

use freeports::cli::config_locations::cmd::CliArgs;
use freeports::cli::run;
use freeports::core::tracing_setup::{self, Verbosity};

fn main() {
    let args = CliArgs::parse();

    let verbosity = Verbosity::from_verbose_and_quiet_counts(args.verbose, args.quiet);
    let log_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if let Err(e) = tracing_setup::init(verbosity, &log_dir) {
        eprintln!("freeports: cannot initialize logging: {e}");
        std::process::exit(1);
    }

    if let Err(e) = run::execute(args) {
        tracing::error!("{e}");
        eprintln!("freeports: {e}");
        std::process::exit(1);
    }
}
