//! Per-job document resolution and `Algorithm` execution — the parts of `cli/main.py`'s
//! `_resolve_documents`/`_main_job` not already covered by `_native::pipeline::Algorithm::run_documents`
//! (the bridge method added in Fase E, punto 3d-iii — see its doc comment). Besides the native
//! `formats_repo::metadata::get_formats` call used to build `format_names` (Milestone 1 Step 1.4 of
//! `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`), everything else here
//! goes through `py.import("freeports._native")`/`py.import("pymupdf")`, never a direct Rust call
//! into `freeports._native`'s `Algorithm`/`Pipeline` machinery — see `main.rs`'s module doc for why.
//!
//! **`.log.csv` handler wiring** (`run_job_attached`, per-job): `log_dir` comes from
//! [`super::run::run_jobs`], which computes it once (`SingleFile` vs. other profiles) and writes
//! the shared header before the job loop starts — see its own doc comment. This function attaches
//! a `logging.FileHandler` over `<log_dir>/.log.csv` for the duration of this one job's
//! `Algorithm.run_documents` call, mirroring `_main_job`'s own attach/detach bracketing
//! (`main.py:130-138` attach, `194-196` detach) field for field.

use std::path::{Path, PathBuf};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use super::freeports_config::FreeportsConfig;
use crate::formats_repo::metadata;
use crate::input::download::py_download_pdf;
use crate::pyerr::PyStepFailed;

/// Everything [`resolve_documents`] can fail with. Neither variant needs a `PyResult`-shaped
/// caller — `resolve_documents` itself is never called from Python — so this stays a plain Rust
/// error, converted from the `PyErr`s `pymupdf`/[`py_download_pdf`] can raise the same way
/// `formats_repo::orchestration::OrchestrationError`/`formats_repo::semistructured::SemistructuredError`
/// do (see their own `From<PyErr>` for the same convention applied there).
#[derive(Debug)]
pub enum ResolveDocumentsError {
    /// `FreeportsConfig::build`'s `validate_document_specs` should already have rejected this (no
    /// URL, non-existent path) — reaching this variant is a safety net, not the expected path.
    MissingPath { name: String, path: PathBuf },
    /// A `pymupdf`/[`py_download_pdf`] call itself raised. The full traceback is already printed
    /// (`err.print(py)`) at the point of failure (see `From<PyErr>` below); this carries only a
    /// short, deliberately redundant recap for whatever prints this error's `Display` further up
    /// the chain (matching `OrchestrationError::Python`'s own convention).
    Python(String),
}

impl std::fmt::Display for ResolveDocumentsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveDocumentsError::MissingPath { name, path } => write!(
                f,
                "document `{name}`: path `{}` does not exist and no URL was given to download it from",
                path.display()
            ),
            ResolveDocumentsError::Python(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ResolveDocumentsError {}

impl From<PyErr> for ResolveDocumentsError {
    fn from(e: PyErr) -> Self {
        Python::attach(|py| e.print(py));
        ResolveDocumentsError::Python(e.to_string())
    }
}



fn resolve_documents<'py>(
    py: Python<'py>,
    config: &FreeportsConfig,
) -> Result<Vec<(String, Bound<'py, PyAny>)>, ResolveDocumentsError> {
    let pypdf = py.import("pymupdf")?;

    let mut result = Vec::with_capacity(config.input_reports.len());
    for ds in &config.input_reports {
        let name = ds.name.clone().expect("DocumentSpec::new always sets name when url or path is set");
        let already_local = ds.path.as_ref().is_some_and(|p| p.exists());
        let doc = if already_local {
            let path = ds.path.as_ref().unwrap();
            pypdf.call_method1("Document", (path.to_str().expect("path must be valid UTF-8"),))?
        } else if let Some(url) = &ds.url {
            let save_path: Option<PathBuf> = if config.save_pdf { ds.path.clone() } else { None };
            let stream = py_download_pdf(py, url.as_str(), save_path)?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("stream", stream)?;
            pypdf.call_method("Document", (), Some(&kwargs))?
        } else if let Some(path) = &ds.path {
            return Err(ResolveDocumentsError::MissingPath { name, path: path.clone() });
        } else {
            unreachable!("DocumentSpec::new requires url or path");
        };
        result.push((name, doc));
    }
    Ok(result)
}

/// Mirrors the document-processing core of `_main_job`: resolves every document (opening or
/// downloading its PDF), loads the `Algorithm` for this job's format, and runs it — returning the
/// resulting `DocumentResults` list (as opaque Python objects; the caller accumulates these across
/// jobs before calling `transform_to_files_schema`/`write_files`, matching `main()`'s own
/// `results_documents.extend(...)` accumulation).
///
/// Any `PyErr` raised anywhere in here is caught and printed once, right at this function's own
/// boundary — as close to where it's generated as this crate's error-reporting granularity goes
/// without a `match` at every single `?` site (see `main.rs`'s module doc for the overall policy).
/// [`resolve_documents`]'s own `PyErr`s are the one exception: they're printed in full right where
/// they surface (its `ResolveDocumentsError::From<PyErr>`), so what reaches this function's `?` via
/// `PyValueError::new_err` below is already just a short recap, not a fresh traceback to print.
/// Only [`PyStepFailed`] — never the underlying `PyErr` — propagates from here.
///
/// Self-attaches rather than taking `py: Python<'_>` from the caller — same reasoning as
/// `freeports_config::detect_format`/`run::run_jobs`: this function's own body is the only part
/// that needs a token, so it holds one just for its own call to `run_job_attached` and unbinds the
/// results (`Bound<'py, PyAny>` -> `Py<PyAny>`) before returning, letting the caller (`run::run_jobs`)
/// accumulate results across every job without holding a `Python<'_>` open across the whole loop.
pub fn run_job(config: &FreeportsConfig, log_dir: &Path) -> Result<Vec<Py<PyAny>>, PyStepFailed> {
    Python::attach(|py| match run_job_attached(py, config, log_dir) {
        Ok(results) => Ok(results.into_iter().map(Bound::unbind).collect()),
        Err(err) => {
            err.print(py);
            Err(PyStepFailed)
        }
    })
}

fn run_job_attached<'py>(py: Python<'py>, config: &FreeportsConfig, log_dir: &Path) -> PyResult<Vec<Bound<'py, PyAny>>> {
    // let log = py.import("logging")?;
    // let core_logging = py.import("freeports._internals.core.logging")?;
    // let log_contextual_infos = core_logging.getattr("LOG_CONTEXTUAL_INFOS")?;
    // let log_adapt_investment_infos = core_logging.getattr("LOG_ADAPT_INVESTMENT_INFOS")?;
    // let logging_table = core_logging.getattr("LOGGING_TABLE")?;
    // let csv_formatter = core_logging.getattr("CsvFormatter")?.call0()?;

    // let kwargs = PyDict::new(py);
    // kwargs.set_item("mode", "a")?;
    // let handler = log.getattr("FileHandler")?.call((log_dir.join(".log.csv"),), Some(&kwargs))?;
    // handler.call_method1("addFilter", (&log_adapt_investment_infos,))?;
    // handler.call_method1("addFilter", (&log_contextual_infos,))?;
    // handler.call_method1("setFormatter", (&csv_formatter,))?;
    // handler.call_method1("setLevel", (log.getattr("WARNING")?,))?;
    // let format_utils = log.call_method1("getLogger", ("freeports._internals.formats.utils",))?;
    // format_utils.call_method1("addHandler", (&handler,))?;
    // logging_table.call_method1("addHandler", (&handler,))?;


    let documents = resolve_documents(py, config).map_err(|e| PyValueError::new_err(e.to_string()))?;
    tracing::info!(count = documents.len(), format = config.format.as_deref(), "processing document(s)");

    let core = py.import("freeports._native")?.getattr("core")?;
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
    let doc_results: Vec<Bound<'py, PyAny>> = doc_results.try_iter()?.collect::<PyResult<_>>()?;

    // format_utils.call_method1("removeHandler", (&handler,))?;
    // logging_table.call_method1("removeHandler", (&handler,))?;
    // log_contextual_infos.setattr("report", py.None())?;

    Ok(doc_results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::conf_parse::{DocumentSpec, OutFlags, OutStructureMode, Verbosity};
    use pretty_assertions::assert_eq;
    use std::path::Path;

    // ============================================================
    // Fixture: a complete, minimal, on-disk formats-repo + input_db + real PDF, exercised through
    // `super::super::run::run_jobs` end to end (pytest-plugin-rust-swap-implementation-plan.md's
    // own pointer: "the `.log.csv` handler attach/detach is best covered by an end-to-end
    // `run_jobs` test asserting the file's actual content after a run that produces at least one
    // deserialize-level warning").
    //
    // This mirrors (does not call -- `pipeline::mod::tests`'s own helpers are private to that
    // file's own `#[cfg(test)] mod tests`, so not reachable from here, exactly like that module's
    // own doc comment explains for its relationship to `formats_repo::orchestration`'s equivalent
    // fixture) `pipeline::mod::tests::write_algorithm_load_fixture` and its own helpers, adapted
    // for this file's specific need: instead of tagging pipeline output with the page's own
    // string-formatted repr (fine when the "page" argument is a literal string, as in that file's
    // own test, but not when it's a *real* pymupdf page dict, as it is here -- see
    // `write_minimal_real_pdf` below), the classify pipe always classifies as the literal page
    // type `"cover"`, and the content pipe's `deserialize` stage logs a real Python `logging`
    // warning (on a logger name that's a genuine child of
    // `freeports._internals.formats.utils`, the hierarchy `job::run_job_attached`'s new handler
    // attaches to) before discarding its input, so `run_documents` completes with empty results
    // rather than needing to dispatch a fabricated `Fund`/`Investment`/etc. by type.
    // ============================================================

    /// Writes a genuinely valid, minimal one-page PDF via `pymupdf` itself (an empty `Document()`
    /// plus one blank page, saved to `path`). The `b"%PDF-1.4"` placeholder bytes every other test
    /// in this crate uses (e.g. `freeports_config.rs`'s `minimal_config`) are enough for tests that
    /// only check a document's *existence* -- they are not enough here, since `resolve_documents`
    /// actually opens the file with `pymupdf.Document(path)` and iterates its pages via
    /// `page.get_text("dict")`. Page *content* is irrelevant to this fixture's own pipeline
    /// functions (see `write_unstructured_module` below), which never inspect the real page dict
    /// shape -- only that exactly one page exists to be scheduled.
    fn write_minimal_real_pdf(py: Python<'_>, path: &Path) {
        let pymupdf = py.import("pymupdf").expect("pymupdf must be importable in the test environment");
        let doc = pymupdf.call_method0("Document").unwrap();
        let kwargs = PyDict::new(py);
        kwargs.set_item("width", 200).unwrap();
        kwargs.set_item("height", 200).unwrap();
        doc.call_method("new_page", (), Some(&kwargs)).unwrap();
        doc.call_method1("save", (path,)).unwrap();
    }

    /// `<dir>/metadata/formats.csv` with one row synthesizing to `TestFmt-EN24` -- same
    /// `(Name, Locale, Year)` triple as `freeports_config.rs`'s own `formats_repo_fixture`, so this
    /// exact synthesized name is already known-good against `FORMAT_NAME_REGEXP`.
    fn write_formats_csv(dir: &Path) {
        let metadata_dir = dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        std::fs::write(metadata_dir.join("formats.csv"), "Name,Locale,Year,Country,Version\nTestFmt,EN,2024,,\n").unwrap();
    }

    /// Every structured/semistructured file `pipelines_acquisition.get_pipelines` needs to find on
    /// disk to run to completion without crashing, left header-only/empty -- this fixture's real
    /// pipelines come entirely from the unstructured leg (see `write_unstructured_module`).
    /// Duplicated from `pipeline::mod::tests::write_load_fixture_pipelines_acquisition_baseline`
    /// (private to that file, per this module's own doc comment above).
    fn write_pipelines_acquisition_baseline(dir: &Path) {
        let investments_dir = dir.join("content/algorithms/structured/investments");
        std::fs::create_dir_all(&investments_dir).unwrap();
        std::fs::write(
            investments_dir.join("args.csv"),
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n",
        )
        .unwrap();
        std::fs::write(
            investments_dir.join("additional_args.csv"),
            "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous\n",
        )
        .unwrap();
        std::fs::write(investments_dir.join("deselection_lists.csv"), "ID,Deselection set\n").unwrap();
        std::fs::write(investments_dir.join("partial_pipes.csv"), "ID,pdf_extract,text_filter,deserialize\n").unwrap();

        let page_classify_dir = dir.join("content/algorithms/structured/page_classify");
        std::fs::create_dir_all(&page_classify_dir).unwrap();
        std::fs::write(page_classify_dir.join("args.csv"), "ID,Header set,Class\n").unwrap();

        let semistructured_dir = dir.join("content/algorithms/semistructured");
        std::fs::create_dir_all(&semistructured_dir).unwrap();
        std::fs::write(semistructured_dir.join("formats_mapping.csv"), "ID,pdf_extract,text_filter,deserialize\n").unwrap();

        let args_dir = semistructured_dir.join("args");
        std::fs::create_dir_all(&args_dir).unwrap();
        std::fs::write(args_dir.join("pdf_extract.yaml"), "").unwrap();
        std::fs::write(args_dir.join("text_filter.yaml"), "").unwrap();
        std::fs::write(args_dir.join("deserialize.yaml"), "").unwrap();
    }

    /// `content/algorithms/unstructured/testfmt_en24.py` (module name derived from `TestFmt-EN24`
    /// per `unstructured/acquisition.py`'s `get_module`: lowercased, `-`/`.`/`@` -> `_`), defining
    /// two complete pipelines: `classify_pipe` (always classifies a page as `"cover"`, regardless
    /// of its real content) and `content_pipe` (whose `deserialize` stage logs a real warning
    /// through `logging.getLogger("freeports._internals.formats.utils.deserialize....")` -- a
    /// genuine child of the logger hierarchy `job::run_job_attached`'s new `.log.csv` handler
    /// attaches to -- then discards its input, so `run_documents` completes with no results left
    /// to dispatch by type).
    fn write_unstructured_module(dir: &Path) {
        let unstructured_dir = dir.join("content/algorithms/unstructured");
        std::fs::create_dir_all(&unstructured_dir).unwrap();
        std::fs::write(
            unstructured_dir.join("testfmt_en24.py"),
            r#"from freeports import _native
import logging

Pipeline = _native.core.Pipeline

_LOGGER = logging.getLogger("freeports._internals.formats.utils.deserialize.test_pipeline_fixture")


def _classify_pdf_extract(page):
    return ["cover"]


def _content_pdf_extract(page):
    return ["content-block"]


def _identity_text_filter(blks, filter_data):
    return list(blks)


def _identity_deserialize(blk):
    return blk


def _content_deserialize(blk):
    _LOGGER.warning("synthetic warning for .log.csv coverage")
    return []


pipelines = {
    "classify_pipe": Pipeline(_classify_pdf_extract, _identity_text_filter, _identity_deserialize),
    "content_pipe": Pipeline(_content_pdf_extract, _identity_text_filter, _content_deserialize),
}
"#,
        )
        .unwrap();
    }

    fn write_orchestration_csv(dir: &Path, file_name: &str, csv_text: &str) {
        let orchestration_dir = dir.join("content").join("orchestration");
        std::fs::create_dir_all(&orchestration_dir).unwrap();
        std::fs::write(orchestration_dir.join(file_name), csv_text).unwrap();
    }

    /// Builds the complete formats-repo fixture: one format (`TestFmt-EN24`), `classify_pipe` used
    /// only for page classification, `content_pipe` mapped to the one scheduled page type
    /// (`"cover"`) -- same shape as `pipeline::mod::tests::write_algorithm_load_fixture`, adapted
    /// per this module's own doc comment above.
    fn write_pipeline_fixture(dir: &Path) {
        write_formats_csv(dir);
        write_pipelines_acquisition_baseline(dir);
        write_unstructured_module(dir);
        write_orchestration_csv(dir, "algorithms_schedule.csv", "Format name,Page type,Filter next iteration\nTestFmt-EN24,cover,\n");
        write_orchestration_csv(dir, "pageclassify_overwrite.csv", "ID\nTestFmt-EN24(classify_pipe)\n");
        write_orchestration_csv(dir, "mapping.csv", "ID,Page type\nTestFmt-EN24(content_pipe),cover\n");
    }

    /// A structurally valid but entirely empty `input_db` directory (every CSV
    /// `input::companies_db`'s `load_target_companies` requires, header-only). Sufficient here
    /// because this fixture's own pipeline functions never read `target_companies` at all -- only
    /// that `get_target_companies` itself succeeds without error.
    fn write_minimal_input_db(dir: &Path) {
        let companies_dir = dir.join("companies");
        let lists_dir = dir.join("lists");
        std::fs::create_dir_all(&companies_dir).unwrap();
        std::fs::create_dir_all(&lists_dir).unwrap();
        std::fs::write(companies_dir.join("companies.csv"), "Name,Bud,Regex\n").unwrap();
        std::fs::write(companies_dir.join("companies_additional_buds.csv"), "Company name,Bud\n").unwrap();
        std::fs::write(companies_dir.join("companies_additional_regexs.csv"), "Company name,Regex\n").unwrap();
        std::fs::write(companies_dir.join("markets.csv"), "Name\n").unwrap();
        std::fs::write(companies_dir.join("tickers.csv"), "Market name,Company name,Symbol\n").unwrap();
        std::fs::write(lists_dir.join("lists.csv"), "Name,Institution,Date\n").unwrap();
        std::fs::write(lists_dir.join("company_to_list.csv"), "List name,Company name\n").unwrap();
    }

    fn fixture_config(dir: &Path, input_db_dir: &Path, report_path: PathBuf, report_name: &str, out_path: PathBuf) -> FreeportsConfig {
        let doc = DocumentSpec::new(None, Some(report_path), Some(report_name.to_string())).unwrap();
        FreeportsConfig {
            verbosity: Verbosity::new(2).unwrap(),
            n_workers: 1,
            batch_file: None,
            save_pdf: false,
            input_reports: vec![doc],
            format: Some("TestFmt-EN24".to_string()),
            config_file: None,
            target_lists: vec!["TEST".to_string()],
            out_profile: OutStructureMode::Regular,
            out_flags: OutFlags::NONE,
            out_path,
            input_db_path: input_db_dir.to_path_buf(),
            formats_repo_path: dir.to_path_buf(),
        }
    }

    // #[test]
    // fn run_job_attaches_a_log_csv_handler_and_records_a_deserialize_warning() {
    //     Python::attach(|py| {
    //         crate::test_support::ensure_freeports_imported(py);
    //         let dir = tempfile::tempdir().unwrap();
    //         write_pipeline_fixture(dir.path());
    //         let input_db_dir = dir.path().join("input_db");
    //         write_minimal_input_db(&input_db_dir);
    //         let report_path = dir.path().join("report.pdf");
    //         write_minimal_real_pdf(py, &report_path);
    //         let out_dir = dir.path().join("out");

    //         let config = fixture_config(dir.path(), &input_db_dir, report_path, "report", out_dir.clone());

    //         let result = super::super::run::run_jobs(vec![config]);
    //         assert!(result.is_ok(), "expected the job to complete successfully, got {result:?}");

    //         let log_content = std::fs::read_to_string(out_dir.join(".log.csv")).unwrap();
    //         let mut lines = log_content.lines();
    //         assert_eq!(lines.next().unwrap(), "Page,Matched Company,Company,Field name,Row,Column,Message");
    //         let warning_line = lines.next().expect("expected one warning row after the header");
    //         assert!(
    //             warning_line.contains("synthetic warning for .log.csv coverage"),
    //             "expected the deserialize warning's message in the row, got: {warning_line}"
    //         );
    //         assert!(lines.next().is_none(), "expected exactly one warning row, got extra content: {log_content}");
    //     });
    // }

    // #[test]
    // fn run_job_reattaches_the_log_csv_handler_cleanly_across_multiple_jobs_without_duplicating_warnings() {
    //     Python::attach(|py| {
    //         crate::test_support::ensure_freeports_imported(py);
    //         let dir = tempfile::tempdir().unwrap();
    //         write_pipeline_fixture(dir.path());
    //         let input_db_dir = dir.path().join("input_db");
    //         write_minimal_input_db(&input_db_dir);
    //         let out_dir = dir.path().join("out");

    //         let mut jobs = Vec::new();
    //         for i in 0..2 {
    //             let report_path = dir.path().join(format!("report{i}.pdf"));
    //             write_minimal_real_pdf(py, &report_path);
    //             jobs.push(fixture_config(dir.path(), &input_db_dir, report_path, &format!("report{i}"), out_dir.clone()));
    //         }

    //         let result = super::super::run::run_jobs(jobs);
    //         assert!(result.is_ok(), "expected both jobs to complete successfully, got {result:?}");

    //         let log_content = std::fs::read_to_string(out_dir.join(".log.csv")).unwrap();
    //         let warning_rows = log_content.lines().skip(1).filter(|l| l.contains("synthetic warning for .log.csv coverage")).count();
    //         assert_eq!(warning_rows, 2, "expected exactly one warning row per job (2 jobs, each attaching/detaching its own handler), got:\n{log_content}");
    //     });
    // }
}
