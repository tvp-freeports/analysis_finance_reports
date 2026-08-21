//! Converts this crate's `OutStructureMode`/`OutFlags` (the Fase E collapsed types — see
//! `freeports_engine::cli::conf_parse`'s doc comment) into the real Python enum/flag instances
//! `write_files` (ported in Fase C, before those Rust types existed) still expects, then calls
//! `transform_to_files_schema`/`write_files` — both already Rust, both reached via `py.import`
//! like everything else that touches a pyclass (see `main.rs`'s module doc).

use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::PyList;

use super::conf_parse::{OutFlags, OutStructureMode};

fn out_profile_to_python<'py>(py: Python<'py>, profile: OutStructureMode, is_batch: bool) -> PyResult<Bound<'py, PyAny>> {
    let conf_parse = py.import("freeports._internals.cli.conf_parse")?;
    let enum_class = conf_parse.getattr(if is_batch { "OutStructureBatchMode" } else { "OutStructureNormalMode" })?;
    let name = match profile {
        OutStructureMode::Regular => "REGULAR",
        OutStructureMode::SingleFile => "SINGLE_FILE",
        OutStructureMode::Structured => "STRUCTURED",
    };
    enum_class.getattr(name)
}

fn out_flags_to_python<'py>(py: Python<'py>, flags: OutFlags, is_batch: bool) -> PyResult<Bound<'py, PyAny>> {
    let conf_parse = py.import("freeports._internals.cli.conf_parse")?;
    let flag_class = conf_parse.getattr(if is_batch { "OutFlagsBatchMode" } else { "OutFlagsNormalMode" })?;
    let mut value = flag_class.call1((0,))?;
    if flags.contains(OutFlags::COMPRESSED) {
        value = value.call_method1("__or__", (flag_class.getattr("COMPRESSED")?,))?;
    }
    if is_batch && flags.contains(OutFlags::SEPARATE_OUT_FILES) {
        value = value.call_method1("__or__", (flag_class.getattr("SEPARATE_OUT_FILES")?,))?;
    }
    Ok(value)
}

/// Mirrors `main()`'s final `transform_to_files_schema` + `write_files` call.
///
/// Same policy as [`super::job::run_job`]: any `PyErr` from within is caught and printed once,
/// right at this function's own boundary, and only [`super::job::PyStepFailed`] propagates out.
pub fn write_results(
    py: Python<'_>,
    all_results: Vec<Bound<'_, PyAny>>,
    out_path: &Path,
    out_profile: OutStructureMode,
    out_flags: OutFlags,
    is_batch: bool,
) -> Result<(), super::job::PyStepFailed> {
    match write_results_attached(py, all_results, out_path, out_profile, out_flags, is_batch) {
        Ok(()) => Ok(()),
        Err(err) => {
            err.print(py);
            Err(super::job::PyStepFailed)
        }
    }
}

fn write_results_attached(
    py: Python<'_>,
    all_results: Vec<Bound<'_, PyAny>>,
    out_path: &Path,
    out_profile: OutStructureMode,
    out_flags: OutFlags,
    is_batch: bool,
) -> PyResult<()> {
    let core = py.import("freeports_engine")?.getattr("core")?;
    let results_list = PyList::new(py, all_results)?;
    let transformed = core.call_method1("transform_to_files_schema", (results_list, is_batch))?;
    let profile_obj = out_profile_to_python(py, out_profile, is_batch)?;
    let flags_obj = out_flags_to_python(py, out_flags, is_batch)?;
    core.call_method1("write_files", (transformed, out_path, profile_obj, flags_obj))?;
    Ok(())
}
