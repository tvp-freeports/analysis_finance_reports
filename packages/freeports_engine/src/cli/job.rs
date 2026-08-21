//! Per-job document resolution and `Algorithm` execution — the parts of `cli/main.py`'s
//! `_resolve_documents`/`_main_job` not already covered by `freeports_engine::pipeline::Algorithm::run_documents`
//! (the bridge method added in Fase E, punto 3d-iii — see its doc comment). Besides the native
//! `formats_repo::metadata::get_formats` call used to build `format_names` (Milestone 1 Step 1.4 of
//! `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`), everything else here
//! goes through `py.import("freeports_engine")`/`py.import("pymupdf")`, never a direct Rust call
//! into `freeports_engine`'s `Algorithm`/`Pipeline` machinery — see `main.rs`'s module doc for why.
//!
//! **`.log.csv` is deliberately not wired up here.** `core/logging.py` (the module that owns it)
//! isn't ported yet, and — per Fase B's own finding, preserved intentionally — the page-skip
//! warnings it would record don't actually reach that file in the current Python either (a
//! disconnected logger hierarchy). Skipping it here doesn't regress anything that currently works;
//! it's a diagnostic gap to close once `core/logging.py` itself is ported, not before.

use std::path::PathBuf;

use pyo3::exceptions::{PyFileNotFoundError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::freeports_config::FreeportsConfig;
use crate::formats_repo::metadata;

/// Mirrors `_resolve_documents`. **Fixes a bug found while porting**: the original always opens
/// `ds["path"]` directly whenever it's set, even when `validate_document_specs` computed it as a
/// *not-yet-downloaded* target (the "URL + existing directory + SAVE_PDF" case, which sets
/// `path = dir/"report.pdf"` without downloading anything) — `pypdf.Document(path)` on a file that
/// was never fetched crashes with a pymupdf file error instead of downloading it first. Fixed:
/// download whenever a URL is present and the path either isn't set or doesn't exist yet; open
/// the local file directly only when it's actually already there.
fn resolve_documents<'py>(py: Python<'py>, config: &FreeportsConfig) -> PyResult<Vec<(String, Bound<'py, PyAny>)>> {
    let pypdf = py.import("pymupdf")?;
    let core = py.import("freeports_engine")?.getattr("core")?;

    let mut result = Vec::with_capacity(config.input_reports.len());
    for ds in &config.input_reports {
        let name = ds.name.clone().expect("DocumentSpec::new always sets name when url or path is set");
        let already_local = ds.path.as_ref().is_some_and(|p| p.exists());
        let doc = if already_local {
            let path = ds.path.as_ref().unwrap();
            pypdf.call_method1("Document", (path.to_str().expect("path must be valid UTF-8"),))?
        } else if let Some(url) = &ds.url {
            let save_path: Option<PathBuf> = if config.save_pdf { ds.path.clone() } else { None };
            let stream = core.call_method1("download_pdf", (url.to_string(), save_path))?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("stream", stream)?;
            pypdf.call_method("Document", (), Some(&kwargs))?
        } else if let Some(path) = &ds.path {
            // `FreeportsConfig::build`'s `validate_document_specs` should already have rejected
            // this (no URL, non-existent path) — a clear error here is a safety net, not the
            // expected path.
            return Err(PyFileNotFoundError::new_err(format!(
                "document `{name}`: path `{}` does not exist and no URL was given to download it from",
                path.display()
            )));
        } else {
            unreachable!("DocumentSpec::new requires url or path");
        };
        result.push((name, doc));
    }
    Ok(result)
}

/// A Python-side failure that's already been printed (`err.print(py)`) right where it surfaced —
/// carries nothing else: propagating it further only signals "this failed", never re-inspects the
/// original `PyErr`. Shared with [`super::output::write_results`], the other function downstream
/// of [`super::run::execute`] that does real Python work and can fail the exact same way.
#[derive(Debug)]
pub struct PyStepFailed;

/// Mirrors the document-processing core of `_main_job`: resolves every document (opening or
/// downloading its PDF), loads the `Algorithm` for this job's format, and runs it — returning the
/// resulting `DocumentResults` list (as opaque Python objects; the caller accumulates these across
/// jobs before calling `transform_to_files_schema`/`write_files`, matching `main()`'s own
/// `results_documents.extend(...)` accumulation).
///
/// Any `PyErr` raised anywhere in here (including in [`resolve_documents`]) is caught and printed
/// once, right at this function's own boundary — as close to where it's generated as this crate's
/// error-reporting granularity goes without a `match` at every single `?` site (see `main.rs`'s
/// module doc for the overall policy). Only [`PyStepFailed`] — never the underlying `PyErr` —
/// propagates from here.
///
/// Self-attaches rather than taking `py: Python<'_>` from the caller — same reasoning as
/// `freeports_config::detect_format`/`run::run_jobs`: this function's own body is the only part
/// that needs a token, so it holds one just for its own call to `run_job_attached` and unbinds the
/// results (`Bound<'py, PyAny>` -> `Py<PyAny>`) before returning, letting the caller (`run::run_jobs`)
/// accumulate results across every job without holding a `Python<'_>` open across the whole loop.
pub fn run_job(config: &FreeportsConfig) -> Result<Vec<Py<PyAny>>, PyStepFailed> {
    Python::attach(|py| match run_job_attached(py, config) {
        Ok(results) => Ok(results.into_iter().map(Bound::unbind).collect()),
        Err(err) => {
            err.print(py);
            Err(PyStepFailed)
        }
    })
}

fn run_job_attached<'py>(py: Python<'py>, config: &FreeportsConfig) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let documents = resolve_documents(py, config)?;
    tracing::info!(count = documents.len(), format = config.format.as_deref(), "processing document(s)");

    let core = py.import("freeports_engine")?.getattr("core")?;
    let format = config.format.as_deref().expect("FreeportsConfig::build always resolves FORMAT");

    let format_names = metadata::get_formats(&config.formats_repo_path).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let format_names = PyList::new(py, &format_names)?;

    let algorithm =
        core.getattr("Algorithm")?.call_method1("load", (&config.formats_repo_path, format, &format_names))?;
    let targets = core.call_method1("get_target_companies", (&config.input_db_path, config.target_lists.clone()))?;

    let docs_list = PyList::empty(py);
    for (name, page_obj) in &documents {
        let page_dicts = PyList::empty(py);
        for page in page_obj.try_iter()? {
            let page = page?;
            page_dicts.append(page.call_method1("get_text", ("dict",))?)?;
        }
        let doc_tuple = PyTuple::new(py, [name.into_pyobject(py)?.into_any(), page_dicts.into_any()])?;
        docs_list.append(doc_tuple)?;
    }

    let doc_results = algorithm.call_method1("run_documents", (docs_list, targets, format))?;
    doc_results.try_iter()?.collect()
}
