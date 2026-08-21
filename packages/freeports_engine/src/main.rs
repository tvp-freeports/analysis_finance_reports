//! Fase E's native entry point (`agent-memory/rust-native-binary-plan.md`, punto 3d) — replaces
//! Phase A's shim, which called `freeports._internals.cli.cmd.cmd()` unchanged. This binary now
//! owns the control flow (config resolution, per-job dispatch, output writing) as real Rust.
//!
//! Lives at `src/main.rs` rather than under `src/bin/<name>/` — Cargo's `src/bin/` convention is
//! for a package that ships *several* binaries; there's only one planned here, so the plain
//! single-binary layout (this file + an explicit `[[bin]]` entry in `Cargo.toml` to keep the
//! binary named `freeports` rather than the package's own `freeports-engine`) is the better fit,
//! per explicit user direction (2026-08-20).
//!
//! Deliberately as little code as possible here: everything beyond argv parsing and the
//! attach/exit boilerplate — config resolution, batch dispatch, job execution, output writing —
//! lives in `freeports_engine::cli::run` and the modules it's built from (`job`/`batch`/`output`,
//! also under `src/cli/`), so `cargo test --lib` can reach it. What's left below is the one thing
//! that can't be moved: turning a failure into a printed message and a process exit is inherently
//! a "this is a real binary" concern, and calling `std::process::exit` from a unit test would kill
//! the test process — see `cli::run`'s own doc comment for the exact split and why it stops there.
//! `Python::attach` itself is likewise not something this function holds open across that whole
//! call: `cli::run::execute` takes no `py: Python<'_>` at all, so the one `Python::attach` left
//! here is scoped to just the bootstrap `freeports` import below — everything downstream attaches
//! its own, exactly where it's actually needed (see `cli::run::run_jobs`'s doc comment for the one
//! spot that genuinely has to hold a token across more than a single call).
//!
//! **`main` doesn't return `Result`, and deliberately calls `std::process::exit(1)` itself instead
//! of letting the two failure branches below bubble out through a return type.** An earlier version
//! returned `Result<(), MainError>` from `main` so the two failure paths could just `return Err(..)`
//! — but `Result<(), E: Debug>`'s `Termination` impl prints its own `Error: {:?}` line to stderr on
//! top of whatever this function already printed, e.g. `Error: Run` trailing right after a real,
//! already-user-facing message. Calling `std::process::exit` explicitly is the idiomatic way to
//! suppress that and keep stderr to exactly the one line each failure actually earns.
//!
//! **Errors reach here already printed, not as raw `PyErr`.** Every `PyErr` this whole call chain
//! can produce is caught and printed (`err.print(py)`, a real Python traceback) right where it's
//! generated — `cli::run::execute` and everything under it never lets a `PyErr` itself propagate,
//! only plain Rust error types (see `cli::run::ExecuteError` and its own doc comment for the full
//! chain), so nothing here ever holds a `PyErr` (or anything else worth re-dumping) to begin with —
//! each branch below only needs to know *whether* to exit, not carry a value out. The one exception
//! is `py.import("freeports")` just below: that call *is* this function's own, so there's nowhere
//! closer to the source to push the printing to.
//!
//! **Cross-module PyO3 identity constraint (read before adding a call into `freeports_engine`
//! here)**: this crate Cargo-depends on `freeports_engine` (see `Cargo.toml`), but that dependency
//! is safe to use directly *only* for `cli::*` — `DocumentSpec`/`PartialConfig`/`FreeportsConfig`
//! are plain Rust structs, not pyclasses, and touch nothing format-author code shares. Everything
//! that touches a pyclass shared with format-author code (`Algorithm`, `Pipeline`,
//! `DocumentResults`, `TransformedTables`, ...) must go through `py.import("freeports_engine")`
//! instead, never a direct Rust call into that code — Cargo-depending on it and constructing those
//! types natively would produce a *second*, incompatible copy of each pyclass (PyO3 registers a
//! type per *compiled module*, and this binary is a different compiled module from
//! `freeports_engine.cpython-*.so`, the one format-author code's own `import freeports_engine`
//! resolves to), breaking `isinstance`/cast checks the moment this binary's objects meet
//! format-author code's objects. This is the exact trap already documented and fixed in Fase D's
//! `companies_db.rs` — see that module's doc comment for the original `TypeError` it caused.
//! `cli::job`/`cli::batch`/`cli::output` all follow this rule; keep it that way when extending them.

use clap::Parser;
use pyo3::prelude::*;

use freeports_engine::cli::cmd_config::CliArgs;

fn main() {
    let cli_args = CliArgs::parse();

    // Ensures `freeports_engine.cpython-*.so` (and everything else `freeports` pulls in) is
    // loaded through the normal import system before anything below reaches for it via
    // `py.import` — the same module identity every format-author `import freeports_engine`
    // resolves to (see this module's own doc comment). Self-attaches just for this one import
    // rather than wrapping the whole function: `execute` below needs no `py: Python<'_>` handed to
    // it at all (see `cli::run::execute`'s own doc comment for where it attaches its own instead),
    // so there's nothing left here that has to share a token with it.
    let imported = Python::attach(|py| match py.import("freeports") {
        Ok(_) => true,
        Err(err) => {
            err.print(py);
            false
        }
    });
    if !imported {
        std::process::exit(1);
    }

    if let Err(err) = freeports_engine::cli::run::execute(cli_args) {
        // Not `err.print(py)`: this is a Rust-native error (`ExecuteError`), not a `PyErr` — any
        // Python-side failure inside `execute` has already been printed at its own source (see
        // this module's doc comment). `to_string()` is empty exactly when that already happened
        // (see `RunJobsError::Step`'s `Display`), so there's nothing left to print.
        let message = err.to_string();
        if !message.is_empty() {
            eprintln!("{message}");
        }
        std::process::exit(1);
    }
}
