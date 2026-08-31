//! Shim di `freeports.utils.text_filter`.

use pyo3::prelude::*;

use crate::core::match_fund::MatchFund;
use crate::core::normalization;
use crate::formats_utils::text_filter::standard_funcs::extract_currency_from_text;

use crate::python::consts::PyCurrency;
use crate::core::tracing_setup::log_error;

/// The Python shim of a fund identity: a name that knows how to compare itself with another after
/// deep normalisation.
#[pyclass(name = "MatchFund", module = "freeports.utils.text_filter", frozen, eq, hash)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PyMatchFund(MatchFund);

impl PyMatchFund {
    pub fn inner(&self) -> &MatchFund {
        &self.0
    }
}

impl From<MatchFund> for PyMatchFund {
    fn from(value: MatchFund) -> Self {
        PyMatchFund(value)
    }
}

#[pymethods]
impl PyMatchFund {
    /// The name is accepted both positionally and by keyword: formats repositories use both forms.
    #[new]
    #[pyo3(signature = (name))]
    fn new(name: &str) -> PyMatchFund {
        PyMatchFund(MatchFund::new(name))
    }

    #[getter]
    fn name(&self) -> &str {
        self.0.name()
    }

    #[getter]
    fn n_name(&self) -> &str {
        self.0.normalized()
    }

    fn matches(&self, other: &str) -> bool {
        self.0.matches(other)
    }

    fn __repr__(&self) -> String {
        format!("MatchFund({:?})", self.0.name())
    }
}

/// Light normalisation of a string: whitespace runs collapsed, optionally lower-cased.
#[pyfunction]
#[pyo3(name = "normalize_string", signature = (input, lower=false))]
pub fn py_normalize_string(input: &str, lower: bool) -> String {
    normalization::normalize_string(input, lower)
}

/// Deep normalisation, the form fund names are compared on.
#[pyfunction]
#[pyo3(name = "deep_normalize_string", signature = (input))]
pub fn py_deep_normalize_string(input: &str) -> String {
    normalization::deep_normalize_string(input)
}

/// Normalisation of a single word.
#[pyfunction]
#[pyo3(name = "normalize_word", signature = (input, lower=false))]
pub fn py_normalize_word(input: &str, lower: bool) -> String {
    normalization::normalize_word(input, lower)
}

/// The first currency named in a text.
#[pyfunction]
#[pyo3(name = "extract_currency_from_text", signature = (text))]
pub fn py_extract_currency_from_text(text: &str) -> PyResult<PyCurrency> {
    extract_currency_from_text(text).map(PyCurrency::from).map_err(|e| {
        // Past this point the error only lives as a Python exception (rule 1 of L2).
        tracing::error!(error = log_error(&e), "extract_currency_from_text failed: {e}");
        pyo3::exceptions::PyValueError::new_err(e.to_string())
    })
}

/// The two decorators that replace a filtering pipe's filter-data argument with the set of
/// already-known fund identities, before calling the decorated function.
///
/// They are classes used as decorators rather than functions — which is why their Python names are
/// lower-case like a function's. The logic lives in the shim rather than in the crate because it
/// operates on an arbitrary Python callable and a list of Python results, concepts that exist only
/// on this side of the boundary.
macro_rules! filter_data_decorator {
    ($shim:ident, $py_name:literal, $source:expr, $doc:literal) => {
        #[doc = $doc]
        #[pyclass(name = $py_name, module = "freeports.utils.text_filter", frozen)]
        pub struct $shim {
            wrapped: Py<PyAny>,
        }

        #[pymethods]
        impl $shim {
            #[new]
            fn new(wrapped: Py<PyAny>) -> $shim {
                $shim { wrapped }
            }

            fn __call__<'py>(
                &self,
                py: Python<'py>,
                pdf_blks: Py<PyAny>,
                filter_data: &Bound<'py, PyAny>,
            ) -> PyResult<Bound<'py, PyAny>> {
                let funds = match_funds_from(py, filter_data, $source)?;
                self.wrapped.bind(py).call1((pdf_blks, funds))
            }
        }
    };
}

filter_data_decorator!(
    PyFundFilterData,
    "fund_filter_data",
    FundSource::Funds,
    "Decoratore: `filter_data` diventa l'insieme dei `MatchFund` dei risultati `Fund`."
);
filter_data_decorator!(
    PyInvestmentFundFilterData,
    "investment_fund_filter_data",
    FundSource::Investments,
    "Decoratore: `filter_data` diventa l'insieme dei `MatchFund` dei fondi citati dagli investimenti."
);

/// Where the fund names replacing the filter data are taken from.
#[derive(Clone, Copy)]
enum FundSource {
    /// The names of the fund results.
    Funds,
    /// The fund field of the investment results.
    Investments,
}

/// The set of fund identities derived from a filter data.
///
/// Results that are not of the kind sought — and those whose name is still a promise, and therefore
/// not a string — are simply skipped: an unresolved fund has no name to compare with.
fn match_funds_from<'py>(
    py: Python<'py>,
    filter_data: &Bound<'py, PyAny>,
    source: FundSource,
) -> PyResult<Bound<'py, pyo3::types::PySet>> {
    let funds = pyo3::types::PySet::empty(py)?;
    for item in filter_data.try_iter()? {
        let item = item?;
        let name = match source {
            FundSource::Funds if item.is_instance_of::<crate::python::output::PyFund>() => {
                Some(item.getattr("name")?)
            }
            FundSource::Investments
                if item.is_instance_of::<crate::python::output::PyEquity>()
                    || item.is_instance_of::<crate::python::output::PyBond>() =>
            {
                Some(item.getattr("fund")?)
            }
            _ => None,
        };
        if let Some(name) = name {
            match name.extract::<String>() {
                Ok(name) => {
                    funds.add(Bound::new(py, PyMatchFund::new(&name))?)?;
                }
                // Not yet resolved (still a promise): nothing lost, just not comparable yet -
                // same "kept pending" situation as `promise_resolution::reference kept pending`.
                Err(_) => tracing::trace!("filter_data entry skipped: its name is not resolved yet"),
            }
        }
    }
    Ok(funds)
}
