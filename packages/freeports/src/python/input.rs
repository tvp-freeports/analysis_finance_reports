//! The shim for loading the target companies from an input database.
//!
//! The part of the API the development tooling needs rather than format authors: the filter data a
//! filtering pipe receives at the first step of a schedule is exactly the list
//! [`py_get_target_companies`] returns.

use std::path::PathBuf;

use pyo3::prelude::*;

use crate::formats_utils::text_filter::matcher::CompanyMatchInfos;
use crate::input::companies_db;
use crate::core::tracing_setup::log_error;

/// The Python shim of a compiled target company.
///
/// Opaque on purpose: it holds already-compiled patterns, and author code has nothing to read
/// inside it but the name — which is the only thing ever exposed.
#[pyclass(name = "CompanyMatchInfos", module = "freeports.input", frozen)]
#[derive(Clone)]
pub struct PyCompanyMatchInfos(CompanyMatchInfos);

impl PyCompanyMatchInfos {
    pub fn inner(&self) -> &CompanyMatchInfos {
        &self.0
    }
}

impl From<CompanyMatchInfos> for PyCompanyMatchInfos {
    fn from(value: CompanyMatchInfos) -> Self {
        PyCompanyMatchInfos(value)
    }
}

#[pymethods]
impl PyCompanyMatchInfos {
    #[getter]
    fn name(&self) -> &str {
        self.0.name()
    }

    #[getter]
    fn n_name(&self) -> &str {
        self.0.normalized_name()
    }

    fn __repr__(&self) -> String {
        format!("CompanyMatchInfos({:?})", self.0.name())
    }
}

/// The companies of the named lists, already compiled and ready to pass as filter data.
///
/// The compilation is included, rather than left to the caller, because it is the only thing
/// callers ever did with the result.
#[pyfunction]
#[pyo3(name = "get_target_companies", signature = (input_db_directory, target_lists))]
pub fn py_get_target_companies(
    input_db_directory: PathBuf,
    target_lists: Vec<String>,
) -> PyResult<Vec<PyCompanyMatchInfos>> {
    tracing::debug!(
        directory = %input_db_directory.display(),
        target_list_count = target_lists.len(),
        "get_target_companies called from Python"
    );
    companies_db::compile_target_companies(&input_db_directory, &target_lists)
        .map(|companies| companies.into_iter().map(PyCompanyMatchInfos::from).collect())
        .map_err(|e| {
            // Past this point the error only lives as a Python exception, invisible to this
            // crate's tracing/CSV pipeline (rule 1: log before it is absorbed by PyO3).
            tracing::error!(error = log_error(&e), "get_target_companies failed: {e}");
            pyo3::exceptions::PyValueError::new_err(e.to_string())
        })
}

/// The raw, uncompiled inputs.
#[pyfunction]
#[pyo3(name = "load_target_companies", signature = (input_db_directory, target_lists))]
pub fn py_load_target_companies(
    input_db_directory: PathBuf,
    target_lists: Vec<String>,
) -> PyResult<Vec<String>> {
    tracing::debug!(
        directory = %input_db_directory.display(),
        target_list_count = target_lists.len(),
        "load_target_companies called from Python"
    );
    companies_db::load_target_companies(&input_db_directory, &target_lists)
        .map(|companies| companies.into_iter().map(|c| c.name).collect())
        .map_err(|e| {
            // Same PyO3-boundary rationale as `py_get_target_companies` above.
            tracing::error!(error = log_error(&e), "load_target_companies failed: {e}");
            pyo3::exceptions::PyValueError::new_err(e.to_string())
        })
}
