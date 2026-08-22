//! Calls `transform_to_files_schema`/`write_files` (both already Rust, in `output::routines`),
//! mirroring `main()`'s final step.
//!
//! Neither call needs a `py.import` round-trip anymore. `transform_to_files_schema` self-attaches
//! (see its own doc comment in `output::routines`) — [`write_results`] runs it first, outside any
//! attach of its own. `write_files` (`output::routines::write_files`) takes this crate's native
//! `OutStructureMode`/`OutFlags` directly rather than the Python `Enum`/`Flag` objects it used to
//! be converted into (`freeports._internals.cli.conf_parse`'s `OutStructureNormalMode`/
//! `OutFlagsNormalMode`/etc.) purely to be read back out generically right afterwards. It also
//! needs no `py: Python<'_>` token and never touches `PyErr` — its failures are the native
//! `output::routines::WriteFilesError`, printed directly with `eprintln!` rather than through a
//! `Python::attach` scope, since there is no genuine Python interpreter involvement left on this
//! path to attach for.

use std::path::Path;

use pyo3::prelude::*;

use super::conf_parse::{OutFlags, OutStructureMode};
use super::super::output::routines::{write_files, transform_to_files_schema};
use crate::pyerr::PyStepFailed;

/// [`write_results`]'s own "already failed, already printed" marker — distinct from
/// [`PyStepFailed`] on purpose. `transform_to_files_schema`'s failure genuinely is a `PyStepFailed`
/// (it walks real Python objects — see its own doc comment), so that case converts via `?` below.
/// But `write_files`'s failure is a plain [`crate::output::routines::WriteFilesError`], printed
/// with `eprintln!` right here, not `PyErr::print` — returning `PyStepFailed` for *that* branch
/// would claim a Python step failed when none was ever touched, the exact naming lie that motivated
/// dropping `write_files`'s old `py_` prefix and `PyResult` return type in the first place. Both
/// branches already have nothing left to say by the time they reach a caller — same convention as
/// `cli::run::RunJobsError::Step`'s deliberately empty `Display` — so this stays a bare marker too.
#[derive(Debug)]
pub struct WriteResultsFailed;

impl std::fmt::Display for WriteResultsFailed {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl std::error::Error for WriteResultsFailed {}

impl From<PyStepFailed> for WriteResultsFailed {
    fn from(_: PyStepFailed) -> Self {
        WriteResultsFailed
    }
}

/// Mirrors `main()`'s final `transform_to_files_schema` + `write_files` call.
///
/// Same policy as [`super::job::run_job`]: any failure from within is reported once, right at
/// this function's own boundary, and only [`WriteResultsFailed`] propagates out — see its own doc
/// comment for why that marker, not [`PyStepFailed`], is what this function returns.
pub fn write_results(
    all_results: Vec<Py<PyAny>>,
    out_path: &Path,
    out_profile: OutStructureMode,
    out_flags: OutFlags,
    is_batch: bool,
) -> Result<(), WriteResultsFailed> {
    let transformed = transform_to_files_schema(all_results, is_batch)?;
    write_files(&transformed, out_path.to_path_buf(), out_profile, out_flags).map_err(|err| {
        eprintln!("{err}");
        WriteResultsFailed
    })
}
