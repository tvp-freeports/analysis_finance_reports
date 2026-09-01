//! The shims of the entry points: the algorithm, running a job, listing the formats, reading the
//! configuration file.
//!
//! This is the part of the API the **development tooling** needs rather than format authors: load a
//! format's algorithm, apply one segment to a single page for the per-page tests, and run a whole
//! job writing its output files for the integration tests.

use std::path::{Path, PathBuf};

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use crate::cli::conf_parse::DocumentSpec;
use crate::cli::config_locations::file;
use crate::cli::partial_config::{ConfigSource, PartialConfig, defaults, overwrite};
use crate::cli::{freeports_config, job, output};
use crate::core::algorithm::Algorithm;
use crate::core::parallelism::Parallelism;
use crate::core::tracing_setup::{self, CsvLogLayer};
use crate::core::page::FormatName;
use crate::core::schedule::PageClass;
use crate::formats_repo::metadata;
use crate::output::routines::write::{OutFlags, OutStructureMode};

use super::core::{PyPdfBlock, PyTextBlock};
use super::pipes::{extracted_to_py, filter_data_of, page_from_py, previous_results_from_py, target_companies_from_py};
use crate::core::tracing_setup::log_error;

/// A native error as a Python `ValueError`.
fn value_error<E: std::fmt::Display>(error: E) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// The Python shim of an algorithm.
///
/// The three per-segment methods are the API the development tooling drives for its single-page
/// tests. What differs from a naive chaining is *how* they compose, not what they return: the
/// native per-segment API starts from text blocks, so this shim redoes the extraction and filtering
/// chain before deserializing.
#[pyclass(name = "Algorithm", module = "freeports.core", frozen)]
pub struct PyAlgorithm(Algorithm);

#[pymethods]
impl PyAlgorithm {
    /// **A divergence absorbed here:** the third argument is accepted and ignored. It used to be
    /// the list of known formats, which the caller had to fetch and pass; the native loader reads
    /// it from the repository itself, so passing it adds nothing — but existing callers still do.
    #[staticmethod]
    #[pyo3(signature = (formats_repo_dir, format_name, format_names=None))]
    fn load(
        formats_repo_dir: PathBuf,
        format_name: String,
        format_names: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyAlgorithm> {
        let _ = format_names;
        tracing::debug!(
            format = format_name,
            formats_repo_dir = %formats_repo_dir.display(),
            "Algorithm.load called from Python"
        );
        Algorithm::load(&formats_repo_dir, &FormatName::new(format_name.clone())).map(PyAlgorithm).map_err(|e| {
            // `Algorithm::load` itself never logs its own failure (only the success path does,
            // `formats_repo.rs`'s "format algorithm loaded"): this shim is the only place that
            // ever sees this particular call fail, since `freeports-dev`'s single-page runner
            // does not go through `cli::job::run` (which logs job-level failures on its own).
            tracing::error!(error = log_error(&e), format = format_name, "Algorithm.load failed: {e}");
            value_error(e)
        })
    }

    #[getter]
    fn format(&self) -> &str {
        self.0.format().as_str()
    }

    /// The page classes the schedule visits, in schedule order.
    #[getter]
    fn page_classes(&self) -> Vec<String> {
        self.0.schedule().page_classes().iter().map(|class| class.as_str().to_string()).collect()
    }

    fn apply_pdf_extract(&self, page: &Bound<'_, PyAny>, page_class: &str) -> PyResult<Vec<PyPdfBlock>> {
        let py = page.py();
        let page = page_from_py(page)?;
        tracing::debug!(page_class, "Algorithm.apply_pdf_extract called from Python");
        let blocks = self.0.apply_pdf_extract(&page, &PageClass::new(page_class)).map_err(|e| {
            tracing::error!(error = log_error(&e), page_class, "apply_pdf_extract failed: {e}");
            value_error(e)
        })?;
        blocks.iter().map(|block| PyPdfBlock::from_native(py, block)).collect()
    }

    #[pyo3(signature = (page, filter_data, page_class))]
    fn apply_text_filter(
        &self,
        page: &Bound<'_, PyAny>,
        filter_data: &Bound<'_, PyAny>,
        page_class: &str,
    ) -> PyResult<Vec<PyTextBlock>> {
        let py = page.py();
        let page = page_from_py(page)?;
        let companies = target_companies_from_py(filter_data)?;
        let previous = previous_results_from_py(filter_data)?;
        let data = filter_data_of(&companies, &previous);
        tracing::debug!(
            page_class,
            company_count = companies.len(),
            previous_count = previous.len(),
            "Algorithm.apply_text_filter called from Python"
        );
        let blocks = self.0.apply_text_filter(&page, &PageClass::new(page_class), &data).map_err(|e| {
            tracing::error!(error = log_error(&e), page_class, "apply_text_filter failed: {e}");
            value_error(e)
        })?;
        blocks.iter().map(|block| PyTextBlock::from_native(py, block)).collect()
    }

    #[pyo3(signature = (page, filter_data, page_class))]
    fn apply_deserialize<'py>(
        &self,
        py: Python<'py>,
        page: &Bound<'py, PyAny>,
        filter_data: &Bound<'py, PyAny>,
        page_class: &str,
    ) -> PyResult<Vec<Bound<'py, PyAny>>> {
        let page = page_from_py(page)?;
        let companies = target_companies_from_py(filter_data)?;
        let previous = previous_results_from_py(filter_data)?;
        let data = filter_data_of(&companies, &previous);
        tracing::debug!(
            page_class,
            company_count = companies.len(),
            previous_count = previous.len(),
            "Algorithm.apply_deserialize called from Python"
        );
        // The full per-page chain, not the hand-made composition of two segments: the two differ
        // when a page class maps several pipelines.
        let extracted = self.0.apply_deserialize(&page, &PageClass::new(page_class), &data).map_err(|e| {
            tracing::error!(error = log_error(&e), page_class, "apply_deserialize failed: {e}");
            value_error(e)
        })?;
        extracted.iter().map(|item| extracted_to_py(py, item)).collect()
    }

    fn __repr__(&self) -> String {
        format!("Algorithm({:?})", self.0.format().as_str())
    }
}

/// The names of the formats the repository declares.
///
/// **A divergence:** this used to return a whole table indexed by name, of which callers only ever
/// took the index. It returns the names directly: the rest of the table was never read by anyone.
#[pyfunction]
#[pyo3(name = "get_formats", signature = (formats_repo_dir))]
pub fn py_get_formats(formats_repo_dir: PathBuf) -> PyResult<Vec<String>> {
    tracing::debug!(formats_repo_dir = %formats_repo_dir.display(), "get_formats called from Python");
    metadata::get_formats(&formats_repo_dir).map_err(|e| {
        // `metadata::get_formats` never logs its own failure, only the success count: this shim
        // is the only place this call is ever wrapped outside `Algorithm::load` (which opens its
        // own `formats_repo`/`format` spans, not relevant here).
        tracing::error!(error = log_error(&e), formats_repo_dir = %formats_repo_dir.display(), "get_formats failed: {e}");
        value_error(e)
    })
}

/// The format matching a report URL, if the repository declares one.
#[pyfunction]
#[pyo3(name = "url_to_format", signature = (formats_repo_dir, url))]
pub fn py_url_to_format(formats_repo_dir: PathBuf, url: &str) -> PyResult<Option<String>> {
    tracing::debug!(formats_repo_dir = %formats_repo_dir.display(), url, "url_to_format called from Python");
    let format_names = metadata::get_formats(&formats_repo_dir).map_err(|e| {
        tracing::error!(error = log_error(&e), formats_repo_dir = %formats_repo_dir.display(), "url_to_format: get_formats failed: {e}");
        value_error(e)
    })?;
    metadata::url_to_format(&formats_repo_dir, &format_names, url).map_err(|e| {
        tracing::error!(error = log_error(&e), url, "url_to_format failed: {e}");
        value_error(e)
    })
}

/// The write profile from a string, with the same names the command line accepts.
fn out_profile_of(value: &str) -> PyResult<OutStructureMode> {
    match value.to_ascii_lowercase().as_str() {
        "regular" => Ok(OutStructureMode::Regular),
        "single_file" => Ok(OutStructureMode::SingleFile),
        "structured" => Ok(OutStructureMode::Structured),
        other => {
            tracing::error!(value = other, "invalid output profile");
            Err(PyValueError::new_err(format!(
                "invalid output profile {other:?}, expected one of: regular, single_file, structured"
            )))
        }
    }
}

/// The write flags from a string: comma-separated names. A name that is absent from the list means
/// the flag is off — this argument describes one whole run, with no other source to defer to.
fn out_flags_of(value: &str) -> PyResult<OutFlags> {
    let mut flags = OutFlags::default();
    for name in value.split(',').map(str::trim).filter(|name| !name.is_empty()) {
        match name.to_ascii_lowercase().as_str() {
            "compressed" | "archive" => flags.compressed = true,
            "separate_out" => flags.separate_out = true,
            other => {
                tracing::error!(value = other, "invalid output flag");
                return Err(PyValueError::new_err(format!(
                    "invalid output flag {other:?}, expected one of: compressed, separate_out"
                )));
            }
        }
    }
    Ok(flags)
}

/// A `(url, path, name)` triple as a document spec.
fn document_spec_of(spec: (Option<String>, Option<PathBuf>, Option<String>)) -> PyResult<DocumentSpec> {
    let (url, path, name) = spec;
    if url.is_none() && path.is_none() {
        tracing::error!("document spec has neither a url nor a pdf file path");
        return Err(PyValueError::new_err(
            "you have to specify at least one of: the url, the pdf file path, or both",
        ));
    }
    Ok(DocumentSpec { url, path, name })
}

/// Runs **one** job and writes its results, like a single command-line invocation.
///
/// The entry point of the development tooling's integration tests: it takes primitive arguments
/// instead of parsed command-line ones, skips the merge of the configuration sources — there are
/// none here — and calls the same job run and result write the command line does.
#[pyfunction]
#[pyo3(name = "run_job", signature = (
    input_reports, format, target_lists, formats_repo_path, input_db_path, out_path,
    out_profile=None, out_flags=None, save_pdf=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_run_job(
    input_reports: Vec<(Option<String>, Option<PathBuf>, Option<String>)>,
    format: String,
    target_lists: Vec<String>,
    formats_repo_path: PathBuf,
    input_db_path: PathBuf,
    out_path: PathBuf,
    out_profile: Option<String>,
    out_flags: Option<String>,
    save_pdf: Option<bool>,
) -> PyResult<()> {
    tracing::debug!(
        report_count = input_reports.len(),
        format,
        target_list_count = target_lists.len(),
        "run_job called from Python"
    );
    let reports = input_reports.into_iter().map(document_spec_of).collect::<PyResult<Vec<_>>>()?;
    let out_profile = out_profile.as_deref().map(out_profile_of).transpose()?;
    let out_flags = out_flags.as_deref().map(out_flags_of).transpose()?;

    let overlay = PartialConfig {
        reports: Some(reports),
        format: Some(format),
        target_lists: Some(target_lists),
        formats_repo_path: Some(formats_repo_path),
        input_db_path: Some(input_db_path),
        out_path: Some(out_path),
        out_profile,
        separate_out: out_flags.map(|flags| flags.separate_out),
        compressed: out_flags.map(|flags| flags.compressed),
        save_pdf,
        ..Default::default()
    };

    // `freeports_config::validate` already logs its own failure (`cli/freeports_config.rs`,
    // "cannot validate configuration"), but at this exact call site that log is a no-op: no
    // subscriber exists yet in the Python-embedded process (unlike the `freeports` binary, whose
    // `main.rs` installs one from the very first line, before config resolution even starts).
    // `log_dir` below is only known *after* validation succeeds, so the subscriber genuinely
    // cannot be installed any earlier here — this call therefore has no working structured-log
    // sink for its own validation failures. Not fixed here (would mean moving subscriber
    // installation, a behavioral change beyond additive instrumentation); logged anyway so this
    // stops being invisible the day that gap is closed.
    let config = freeports_config::validate(overwrite(defaults(), overlay, ConfigSource::Cmd)).map_err(|e| {
        tracing::error!(error = log_error(&e), "run_job: configuration validation failed: {e}");
        value_error(e)
    })?;

    // The log goes beside the other output files, in the output directory rather than in the
    // working directory. It is a file the integration tests compare, so it must exist even with no
    // rows to write: creating the layer emits its header at once.
    //
    // The layer is installed **per call**, with a scoped subscriber rather than a global one: a
    // test process calls this once per format, and a global subscriber can be installed only once
    // per process. The scoped one is thread-local and takes precedence over any global anyway.
    if config.out_profile != OutStructureMode::SingleFile {
        std::fs::create_dir_all(&config.out_path).map_err(|e| {
            tracing::error!(error = log_error(&e), out_path = %config.out_path.display(), "run_job: cannot create the output directory: {e}");
            PyRuntimeError::new_err(e.to_string())
        })?;
    }
    let log_dir = if config.out_profile == OutStructureMode::SingleFile {
        config.out_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        config.out_path.clone()
    };
    // Same no-subscriber-yet caveat as the `validate` call above: this is the last fallible step
    // before the per-call subscriber exists, so a failure here still has nowhere to land other
    // than this event (a no-op today, ready for the day a sink exists this early).
    let csv_layer = CsvLogLayer::create(&log_dir.join(".log.csv")).map_err(|e| {
        tracing::error!(error = log_error(&e), log_dir = %log_dir.display(), "run_job: cannot create .log.csv: {e}");
        value_error(e)
    })?;
    let subscriber = {
        use tracing_subscriber::layer::SubscriberExt;
        // Binding, same as `tracing_setup::init`: a layer without a level filter leaves the
        // registry's global max level at `TRACE`, so every `trace!` in the crate is built and
        // dispatched on this path too — which is what made `pytest tests/formats` crawl. The
        // Python entry point has no `-v`/`-q` of its own, so it takes the CSV ceiling directly.
        use tracing_subscriber::Layer;
        tracing_subscriber::registry()
            .with(csv_layer.clone().with_filter(tracing_setup::csv_event_filter()))
    };

    // **Sequential, and not an oversight.** Two independent reasons, each sufficient:
    //
    // 1. the subscriber above is installed with **thread-local** scope: the threads of a worker pool would not see it, and every event produced by distributed pages would vanish from the very log this entry point exists to produce;
    // 2. this function runs with the GIL already held by its caller. An author's pipe on a pool thread would wait for a GIL the calling thread does not release until it has finished waiting for those threads: a deadlock, not a slowdown. The automatic degradation for GIL-bound pipes already avoids it, but this is not the kind of thing to leave hanging on a single defence.
    //
    // The parallel gain belongs to the command line, which installs a **global** subscriber and
    // holds no GIL.
    let result = tracing::subscriber::with_default(subscriber, || {
        let outcomes = job::run(&config, Parallelism::SEQUENTIAL)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        output::write_results(&config, &outcomes).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    });
    // Always attempted, whatever the outcome. The pipeline's real outcome takes precedence over a
    // close failure if both fail: an early return on the close would discard a genuine pipeline
    // failure behind a bookkeeping one.
    let close_result = csv_layer.close();
    result.and(close_result.map_err(|e| {
        // Same no-subscriber caveat as above, worse here: `with_default`'s thread-local scope has
        // already ended by this point, so this event has no sink at all today, not even the
        // pre-validation one.
        tracing::error!(error = log_error(&e), "run_job: cannot flush .log.csv: {e}");
        value_error(e)
    }))
}

/// The Python shim of the configuration file.
///
/// It exposes only what callers really read from it: where to find it, and which input database it
/// declares. It is not the complete resolved configuration, which is the result of merging every
/// source and makes no sense to build from Python.
#[pyclass(name = "FreeportsFileConfig", module = "freeports.cli", frozen)]
pub struct PyFreeportsFileConfig(PartialConfig);

#[pymethods]
impl PyFreeportsFileConfig {
    /// The configuration file to read: working directory, then user tier, then system tier.
    #[staticmethod]
    fn find_config() -> Option<PathBuf> {
        file::find_config()
    }

    #[new]
    fn new(path: PathBuf) -> PyResult<Self> {
        file::load(Some(Path::new(&path))).map(PyFreeportsFileConfig).map_err(value_error)
    }

    /// The name is upper-cased, as configuration keys were exposed as upper-case attributes.
    #[getter]
    #[pyo3(name = "INPUT_DB_PATH")]
    fn input_db_path(&self) -> Option<PathBuf> {
        self.0.input_db_path.clone()
    }

    #[getter]
    #[pyo3(name = "FORMATS_REPO_PATH")]
    fn formats_repo_path(&self) -> Option<PathBuf> {
        self.0.formats_repo_path.clone()
    }

    #[getter]
    #[pyo3(name = "OUT_PATH")]
    fn out_path(&self) -> Option<PathBuf> {
        self.0.out_path.clone()
    }
}
