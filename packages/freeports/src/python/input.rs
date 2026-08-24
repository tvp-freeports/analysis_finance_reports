//! Shim di `freeports.input`: il caricamento delle società bersaglio da un input database.
//!
//! È la parte di API che serve a `freeports_dev` (`input_db.py`), non agli autori di formato: il
//! `filter_data` che i pipe `text_filter` ricevono al primo step dello schedule è esattamente la
//! lista di [`CompanyMatchInfos`] che [`py_get_target_companies`] restituisce.

use std::path::PathBuf;

use pyo3::prelude::*;

use crate::formats_utils::text_filter::matcher::CompanyMatchInfos;
use crate::input::companies_db;

/// Shim Python di [`CompanyMatchInfos`], la forma compilata di una società bersaglio.
///
/// È opaco di proposito: contiene regex già compilate, e il codice d'autore non ha nulla da
/// leggerci dentro se non il nome — che è l'unica cosa che il riferimento esponeva.
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

/// Le società delle liste indicate, già compilate e pronte da passare come `filter_data`.
///
/// Nel riferimento questa funzione restituiva un `pd.DataFrame` che il chiamante doveva ancora
/// compilare; qui — come già nella versione Rust precedente — la compilazione è inclusa, perché
/// è l'unica cosa che i chiamanti ne facessero.
#[pyfunction]
#[pyo3(name = "get_target_companies", signature = (input_db_directory, target_lists))]
pub fn py_get_target_companies(
    input_db_directory: PathBuf,
    target_lists: Vec<String>,
) -> PyResult<Vec<PyCompanyMatchInfos>> {
    companies_db::compile_target_companies(&input_db_directory, &target_lists)
        .map(|companies| companies.into_iter().map(PyCompanyMatchInfos::from).collect())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

/// Gli input grezzi, non compilati — l'altra metà di `PLAN.md` §9 per `input`.
#[pyfunction]
#[pyo3(name = "load_target_companies", signature = (input_db_directory, target_lists))]
pub fn py_load_target_companies(
    input_db_directory: PathBuf,
    target_lists: Vec<String>,
) -> PyResult<Vec<String>> {
    companies_db::load_target_companies(&input_db_directory, &target_lists)
        .map(|companies| companies.into_iter().map(|c| c.name).collect())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}
