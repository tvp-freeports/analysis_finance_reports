//! `PipelineTest.runtest` -> Rust bridge: the single new PyO3 entry point
//! (`packages/freeports_dev/src/freeports_dev/pytest_plugin.py`'s `PipelineTest.runtest`) takes
//! primitive Python-typed arguments (paths/strings/bools/a list of `(url, path, name)` document
//! tuples) and drives the same job-running machinery `cli::run::execute`/`main.rs` already use --
//! see `agent-memory/pytest-plugin-rust-swap-implementation-plan.md`, File 2, for the full design.
//!
//! **Why this doesn't follow the `config_locations/*.rs` 4-file pattern
//! (`cmd.rs`/`env.rs`/`file.rs`/`job.rs`)**: those four exist to parse *text* (CLI flags, env
//! vars, YAML scalars, CSV cells) into typed values, merged via `PartialConfig::overwrite`'s
//! precedence chain. This bridge has neither need -- PyO3's own per-argument extraction already
//! does the Python -> Rust typing at the function boundary (that's the whole point of taking
//! primitive args instead of a raw dict), and it's the *only* config source for this call (no
//! merging, no precedence chain to fold into). The `PartialConfig` struct literal is built
//! directly in `py_run_job`'s body instead of through a dedicated parsing module.
//!
//! **Error-propagation contract** (decision #3 of the requirements note, concretized in the
//! implementation plan's own summary table): a bad document tuple, a bad `out_profile`/`out_flags`
//! string, or a `FreeportsConfig::build` validation failure each raise a real, informative
//! `PyValueError` -- none of these ever touch Python themselves, so there is no
//! "already-printed-elsewhere, keep it opaque" convention to preserve for them. A [`run_jobs`]
//! failure instead raises a `PyRuntimeError`, using `RunJobsError`'s own `Display` as the message
//! when it's non-empty (currently only true for `RunJobsError::Log`), and a generic fallback
//! string when it's empty (`RunJobsError::Step`/`RunJobsError::Write` are deliberately empty --
//! see `run.rs`'s own doc comment; `RunJobsError::NoJobs`'s `Display` is real but never reachable
//! from here, since this bridge always calls `run_jobs` with exactly one job).
//!
//! Implemented against the test suite below exactly as `test-writer` left it (per this
//! workspace's TDD discipline, the tests are the contract and were not edited to make them pass).

use std::path::PathBuf;
use std::str::FromStr;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use url::Url;

use super::conf_parse::{DocumentSpec, OutFlags, OutStructureMode};
use super::freeports_config::FreeportsConfig;
use super::partial_config::PartialConfig;
use super::run::run_jobs;

/// Converts one raw `(url, path, name)` tuple -- exactly the shape `input_reports` extracts a
/// Python list of 3-tuples into -- to a native [`DocumentSpec`], mapping both possible failure
/// modes (an unparsable `url` string via `url::Url::parse`, or [`DocumentSpec::new`]'s own
/// "neither url nor path given" check) to a `PyValueError` -- the same pattern
/// `formats_repo::metadata::py_get_formats`/`py_url_to_format` already use for their own native
/// errors.
fn document_spec_from_tuple(spec: (Option<String>, Option<PathBuf>, Option<String>)) -> PyResult<DocumentSpec> {
    let (url, path, name) = spec;
    let url = url.map(|u| Url::parse(&u)).transpose().map_err(|e| PyValueError::new_err(e.to_string()))?;
    DocumentSpec::new(url, path, name).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// `#[pyfunction]`, exposed to Python as `run_job` (see the implementation plan's File 1 for the
/// exact nested-pymodule path this ends up reachable at from `pytest_plugin.py`: `lib.rs` puts it
/// in its own `cli` nested pymodule rather than the existing `core` one, so this is reachable as
/// `freeports._native.cli.run_job` -- `core` mirrors this crate's own `core::*`/pipeline-mechanics
/// module tree, while `run_job` is conceptually a `cli::*` item (job resolution/config/
/// orchestration), matching this crate's own internal split. Builds a native
/// `PartialConfig`/`FreeportsConfig` from primitive Python-typed arguments (reusing
/// `FreeportsConfig::build`'s existing 6 validators), then runs it through
/// [`run_jobs`](super::run::run_jobs) -- see this file's own module doc for the full
/// error-propagation contract.
///
/// Deliberately **excludes** `PDF`, top-level `URL`, `PREFIX_OUT`, `VERBOSITY`, `N_WORKERS`,
/// `BATCH_FILE`, `CONFIG_FILE` -- see the implementation plan's File 2 for why each of these is
/// confirmed dead/unused for this single-job, non-batch call site.
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
    let input_reports = input_reports.into_iter().map(document_spec_from_tuple).collect::<PyResult<Vec<_>>>()?;
    let out_profile = out_profile.map(|s| OutStructureMode::from_str(&s)).transpose().map_err(|e| PyValueError::new_err(e.to_string()))?;
    let out_flags = out_flags.map(|s| OutFlags::parse(&s)).transpose().map_err(|e| PyValueError::new_err(e.to_string()))?;

    let partial = PartialConfig {
        input_reports: Some(input_reports),
        format: Some(format),
        target_lists: Some(target_lists),
        formats_repo_path: Some(formats_repo_path),
        input_db_path: Some(input_db_path),
        out_path: Some(out_path),
        out_profile,
        out_flags,
        save_pdf,
        ..Default::default()
    };

    let config = FreeportsConfig::build(partial).map_err(|e| PyValueError::new_err(e.to_string()))?;

    run_jobs(vec![config]).map_err(|e| {
        let msg = e.to_string();
        PyRuntimeError::new_err(if msg.is_empty() { "job execution failed; see the traceback printed above".to_string() } else { msg })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyRuntimeError, PyValueError};
    use pyo3::types::PyDict;
    use std::path::Path;
    use url::Url;

    // ============================================================
    // `document_spec_from_tuple` -- isolated unit tests (no `Python::attach` needed: constructing
    // a `PyErr` doesn't require the GIL in this PyO3 version, only *inspecting* one via
    // `is_instance_of`/`.value(py)` does, see the error-table tests further below).
    // ============================================================

    #[test]
    fn document_spec_from_tuple_accepts_a_valid_url_with_no_path_or_name() {
        let spec = document_spec_from_tuple((Some("http://example.com/report.pdf".to_string()), None, None)).unwrap();
        assert_eq!(spec.url.unwrap(), Url::parse("http://example.com/report.pdf").unwrap());
        assert!(spec.path.is_none());
        // `DocumentSpec::new` defaults `name` to the url's own string form when none is given.
        assert_eq!(spec.name.unwrap(), "http://example.com/report.pdf");
    }

    #[test]
    fn document_spec_from_tuple_accepts_a_valid_path_with_no_url_or_name() {
        let path = PathBuf::from("/tmp/report.pdf");
        let spec = document_spec_from_tuple((None, Some(path.clone()), None)).unwrap();
        assert!(spec.url.is_none());
        assert_eq!(spec.path.unwrap(), path);
        assert_eq!(spec.name.unwrap(), path.display().to_string());
    }

    #[test]
    fn document_spec_from_tuple_keeps_an_explicit_name_over_the_default() {
        let spec = document_spec_from_tuple((
            Some("http://example.com/report.pdf".to_string()),
            None,
            Some("MyReport".to_string()),
        ))
        .unwrap();
        assert_eq!(spec.name.unwrap(), "MyReport");
    }

    #[test]
    fn document_spec_from_tuple_accepts_both_url_and_path_together() {
        let path = PathBuf::from("/tmp/local.pdf");
        let spec = document_spec_from_tuple((Some("http://example.com/report.pdf".to_string()), Some(path.clone()), None)).unwrap();
        assert_eq!(spec.url.unwrap(), Url::parse("http://example.com/report.pdf").unwrap());
        assert_eq!(spec.path.unwrap(), path);
    }

    #[test]
    fn document_spec_from_tuple_rejects_an_invalid_url_string_as_a_value_error() {
        let result = document_spec_from_tuple((Some("not a valid url at all".to_string()), None, None));
        let err = result.unwrap_err();
        Python::attach(|py| {
            assert!(err.is_instance_of::<PyValueError>(py), "expected PyValueError, got {err}");
        });
    }

    #[test]
    fn document_spec_from_tuple_rejects_neither_url_nor_path_as_a_value_error() {
        let result = document_spec_from_tuple((None, None, None));
        let err = result.unwrap_err();
        Python::attach(|py| {
            assert!(err.is_instance_of::<PyValueError>(py), "expected PyValueError, got {err}");
        });
    }

    #[test]
    fn document_spec_from_tuple_rejects_neither_url_nor_path_even_with_a_name_given() {
        // Mirrors `conf_parse::tests::new_rejects_neither_url_nor_path_even_with_a_name` -- a
        // `name` alone never counts as "an input was specified".
        let result = document_spec_from_tuple((None, None, Some("MyReport".to_string())));
        let err = result.unwrap_err();
        Python::attach(|py| {
            assert!(err.is_instance_of::<PyValueError>(py), "expected PyValueError, got {err}");
        });
    }

    // ============================================================
    // `py_run_job` end to end -- one test per row of the error-propagation table in this file's
    // own module doc / the implementation plan's summary table, plus one genuine happy-path run.
    // Called directly as a plain Rust function (not through `freeports._native.*`'s Python
    // attribute lookup) -- same convention `input/companies_db.rs`'s own `py_get_target_companies`
    // tests already use for calling a `#[pyfunction]` natively.
    // ============================================================

    #[test]
    fn py_run_job_a_bad_document_url_surfaces_as_a_value_error() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            let result = py_run_job(
                vec![(Some("not a valid url at all".to_string()), None, None)],
                "some-format".to_string(),
                vec!["TEST".to_string()],
                dir.path().to_path_buf(),
                dir.path().to_path_buf(),
                dir.path().to_path_buf(),
                None,
                None,
                None,
            );
            let err = result.unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py), "expected PyValueError, got {err}");
        });
    }

    #[test]
    fn py_run_job_rejects_an_invalid_out_profile_string_as_a_value_error() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            let doc_path = dir.path().join("report.pdf");
            std::fs::write(&doc_path, b"placeholder").unwrap();
            let result = py_run_job(
                vec![(None, Some(doc_path), Some("doc".to_string()))],
                "some-format".to_string(),
                vec!["TEST".to_string()],
                dir.path().to_path_buf(),
                dir.path().to_path_buf(),
                dir.path().to_path_buf(),
                Some("NOT_A_REAL_PROFILE".to_string()),
                None,
                None,
            );
            let err = result.unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py), "expected PyValueError, got {err}");
        });
    }

    #[test]
    fn py_run_job_rejects_an_invalid_out_flags_string_as_a_value_error() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            let doc_path = dir.path().join("report.pdf");
            std::fs::write(&doc_path, b"placeholder").unwrap();
            let result = py_run_job(
                vec![(None, Some(doc_path), Some("doc".to_string()))],
                "some-format".to_string(),
                vec!["TEST".to_string()],
                dir.path().to_path_buf(),
                dir.path().to_path_buf(),
                dir.path().to_path_buf(),
                None,
                Some("NOT_A_REAL_FLAG".to_string()),
                None,
            );
            let err = result.unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py), "expected PyValueError, got {err}");
        });
    }

    #[test]
    fn py_run_job_rejects_an_empty_input_reports_list_as_a_freeports_config_build_failure() {
        // Stands in for the "FreeportsConfig::build validation failure" row of the
        // error-propagation table. `MissingTargetLists`/`MissingInputDbPath`/
        // `MissingFormatsRepoPath` can never actually fire through `py_run_job`: `target_lists`/
        // `input_db_path`/`formats_repo_path` are required (non-`Option`) Python parameters, always
        // wrapped `Some(...)` before `FreeportsConfig::build` ever sees them (see this file's own
        // `py_run_job` doc comment / the implementation plan's File 2, step 3). `NoInputReports` is
        // the one "missing required thing" validator that *is* reachable from here, via an empty
        // `input_reports` Python list -- so that's what this test exercises.
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            let result = py_run_job(
                vec![],
                "some-format".to_string(),
                vec!["TEST".to_string()],
                dir.path().to_path_buf(),
                dir.path().to_path_buf(),
                dir.path().to_path_buf(),
                None,
                None,
                None,
            );
            let err = result.unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py), "expected PyValueError, got {err}");
        });
    }

    #[test]
    fn py_run_job_a_job_execution_failure_surfaces_as_a_generic_py_runtime_error() {
        // `formats_repo_path`/`input_db_path` are never touched here: `format` is given
        // explicitly (so `FreeportsConfig::build`'s `detect_format` never needs `formats.csv`,
        // since no document has a `url`), and the document's local path exists as a real file (so
        // `FreeportsConfig::build`'s `validate_document_specs` accepts it) but is not a real PDF --
        // `resolve_documents`'s own `pymupdf.Document(path)` call fails to parse it, which is
        // exactly the "job execution failure" (`PyStepFailed`, inside `job::run_job`) row of the
        // error-propagation table: `run_jobs` surfaces this as `RunJobsError::Step`, whose
        // `Display` is deliberately empty, so `py_run_job` falls back to its generic message.
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            let broken_pdf = dir.path().join("not_really_a_pdf.pdf");
            std::fs::write(&broken_pdf, b"this is not a valid pdf file").unwrap();
            let out_dir = dir.path().join("out");
            std::fs::create_dir_all(&out_dir).unwrap();

            let result = py_run_job(
                vec![(None, Some(broken_pdf), Some("doc".to_string()))],
                "some-format".to_string(),
                vec!["TEST".to_string()],
                dir.path().to_path_buf(),
                dir.path().to_path_buf(),
                out_dir,
                None,
                None,
                Some(false),
            );

            let err = result.expect_err("pymupdf should fail to open a non-PDF file, surfacing as a job execution failure");
            assert!(err.is_instance_of::<PyRuntimeError>(py), "expected PyRuntimeError, got {err}");
            assert_eq!(err.value(py).to_string(), "job execution failed; see the traceback printed above");
        });
    }

    // ============================================================
    // Happy path: full `py_run_job` -> `FreeportsConfig::build` -> `run_jobs` -> `job::run_job`
    // wiring, against a minimal but real, complete formats-repo/input_db/PDF fixture. Mirrors
    // `freeports_config.rs`'s own `build_end_to_end_with_a_minimal_valid_config` fixture-building
    // style, and duplicates (does not call -- private to that file's own `#[cfg(test)] mod tests`,
    // same reasoning as that module's own doc comment) `cli::job::tests`'s fixture helpers, since
    // this test needs to prove the *whole* stack wires together starting from `py_run_job`'s own
    // primitive-argument boundary, not just `run_jobs` downward.
    // ============================================================

    fn write_minimal_real_pdf(py: Python<'_>, path: &Path) {
        let pymupdf = py.import("pymupdf").expect("pymupdf must be importable in the test environment");
        let doc = pymupdf.call_method0("Document").unwrap();
        let kwargs = PyDict::new(py);
        kwargs.set_item("width", 200).unwrap();
        kwargs.set_item("height", 200).unwrap();
        doc.call_method("new_page", (), Some(&kwargs)).unwrap();
        doc.call_method1("save", (path,)).unwrap();
    }

    fn write_formats_csv(dir: &Path) {
        let metadata_dir = dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        std::fs::write(metadata_dir.join("formats.csv"), "Name,Locale,Year,Country,Version\nTestFmt,EN,2024,,\n").unwrap();
    }

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

    fn write_pipeline_fixture(dir: &Path) {
        write_formats_csv(dir);
        write_pipelines_acquisition_baseline(dir);
        write_unstructured_module(dir);
        write_orchestration_csv(dir, "algorithms_schedule.csv", "Format name,Page type,Filter next iteration\nTestFmt-EN24,cover,\n");
        write_orchestration_csv(dir, "pageclassify_overwrite.csv", "ID\nTestFmt-EN24(classify_pipe)\n");
        write_orchestration_csv(dir, "mapping.csv", "ID,Page type\nTestFmt-EN24(content_pipe),cover\n");
    }

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

    #[test]
    fn py_run_job_end_to_end_success_writes_output_and_log_files() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let dir = tempfile::tempdir().unwrap();
            write_pipeline_fixture(dir.path());
            let input_db_dir = dir.path().join("input_db");
            write_minimal_input_db(&input_db_dir);
            let report_path = dir.path().join("report.pdf");
            write_minimal_real_pdf(py, &report_path);
            let out_dir = dir.path().join("out");

            let result = py_run_job(
                vec![(None, Some(report_path), Some("report".to_string()))],
                "TestFmt-EN24".to_string(),
                vec!["TEST".to_string()],
                dir.path().to_path_buf(),
                input_db_dir,
                out_dir.clone(),
                None,
                None,
                Some(false),
            );

            assert!(result.is_ok(), "expected py_run_job to complete successfully, got {result:?}");
            assert!(out_dir.join(".log.csv").exists());
            let log_content = std::fs::read_to_string(out_dir.join(".log.csv")).unwrap();
            assert!(log_content.contains("synthetic warning for .log.csv coverage"));
            // `write_files` in Regular mode always produces these, even for a run with no matched
            // results -- see `output/routines.rs`'s own `write_regular_creates_every_expected_file`.
            assert!(out_dir.join("investments.csv").exists());
            assert!(out_dir.join("funds.csv").exists());
        });
    }
}
