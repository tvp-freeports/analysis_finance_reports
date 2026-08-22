//! Logging infrastructure for the Rust side of the migration: `tracing`, not Python's
//! `logging` module and not the `log` crate — see the "Scelte tecniche Rust" section of
//! `analysis_finance_reports/agent-memory/rust-rewrite-plan.md`.
//!
//! # The span-naming convention (read this before adding a span anywhere)
//!
//! Today's Python logging (`_internals/core/logging.py`, `logging.getLogger(__name__)`) is
//! hierarchical **by source file** — the logger name mirrors where in the code a message was
//! emitted. That is *not* what the Rust side should do. Spans here must be named after the
//! **task/phase currently executing**, so that when something fails, the active span stack
//! reads like a sentence describing what the program was doing — not which file the failing
//! line lives in. For example, a page-processing failure should show something like
//! `document_ingest{doc="EURIZON-EN23.A"} > page_classification{page=12} > block_extraction`,
//! regardless of how many files/functions those three phases are spread across.
//!
//! Concretely: name a span after the *pipeline phase* (`document_ingest`,
//! `page_classification`, `block_extraction`, `promise_resolution`, `csv_write`, …), attach the
//! identifying data as span *fields* (`#[tracing::instrument(fields(page = %page_num))]` or
//! `tracing::info_span!("page_classification", page = page_num)`), and nest spans the way
//! execution actually nests — not the way modules happen to import each other. A small utility
//! function (like the ones in `core/normalization.rs` or `core/flag_expr.rs`) generally does
//! *not* need its own span — it's not a task, it's a building block called from within one; wrap
//! the task that calls it, not every function transitively involved. Reserve spans for
//! boundaries a human would actually want to see in a failure trace.
//!
//! No pipeline-phase code has been ported to Rust yet (Fase 1 so far is leaf utilities:
//! normalization, matching, consts, flag expressions) — there is deliberately no demonstration
//! span in this module. Adding one on a leaf utility would set the wrong precedent (spans on
//! every trivial function, not on task boundaries). The first real instrumentation should happen
//! naturally when Fase 2+ ports something that *is* a task (promise resolution, page
//! classification, …).

use std::sync::Once;

use pyo3::prelude::*;
use tracing_subscriber::EnvFilter;

static INIT: Once = Once::new();

/// Installs the global `tracing` subscriber, once per process. Safe to call multiple times
/// (subsequent calls are no-ops) — Python's import system can re-trigger module-level setup
/// code more often than a naive `set_global_default` would tolerate (it panics on a second
/// call), so this guards with `std::sync::Once` instead of assuming single-call discipline.
///
/// Respects `RUST_LOG` (standard `tracing-subscriber` `EnvFilter` syntax, e.g.
/// `RUST_LOG=_native=debug`, matching `Cargo.toml`'s `[lib] name = "_native"` — that's also
/// this crate's `--crate-name`, which seeds `tracing`'s default target namespace since no
/// `target = "..."` override exists anywhere in this crate) for verbosity; defaults to `info`
/// when unset. Writes
/// formatted spans/events to stderr, so they interleave naturally with Python's own stderr
/// output rather than needing separate log-file plumbing today (the CLI's actual log
/// destination handling stays whatever `core/logging.py` already does, until that module itself
/// is ported).
#[pyfunction]
#[pyo3(name = "init_tracing")]
pub fn py_init_tracing() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .try_init();
    });
}
