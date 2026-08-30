//! Shim di `freeports.utils.text_filter`.

use pyo3::prelude::*;

use crate::core::match_fund::MatchFund;
use crate::core::normalization;
use crate::formats_utils::text_filter::standard_funcs::extract_currency_from_text;

use crate::python::consts::PyCurrency;
use crate::core::tracing_setup::log_error;

/// Shim Python di [`MatchFund`]: un nome di fondo che sa confrontarsi con un altro dopo la
/// normalizzazione profonda.
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
    /// `name` è accettato sia posizionale sia per parola chiave: il repo formati usa entrambe le
    /// forme (`MatchFund(x.name)` e `MatchFund(name=s)`).
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

/// La normalizzazione "leggera" di una stringa (spazi collassati, opzionalmente minuscola).
#[pyfunction]
#[pyo3(name = "normalize_string", signature = (input, lower=false))]
pub fn py_normalize_string(input: &str, lower: bool) -> String {
    normalization::normalize_string(input, lower)
}

/// La normalizzazione profonda, quella su cui si confrontano i nomi di fondo.
#[pyfunction]
#[pyo3(name = "deep_normalize_string", signature = (input))]
pub fn py_deep_normalize_string(input: &str) -> String {
    normalization::deep_normalize_string(input)
}

/// La normalizzazione di una singola parola.
#[pyfunction]
#[pyo3(name = "normalize_word", signature = (input, lower=false))]
pub fn py_normalize_word(input: &str, lower: bool) -> String {
    normalization::normalize_word(input, lower)
}

/// La prima valuta nominata da un testo.
#[pyfunction]
#[pyo3(name = "extract_currency_from_text", signature = (text))]
pub fn py_extract_currency_from_text(text: &str) -> PyResult<PyCurrency> {
    extract_currency_from_text(text).map(PyCurrency::from).map_err(|e| {
        // Past this point the error only lives as a Python exception (rule 1 of L2).
        tracing::error!(error = log_error(&e), "extract_currency_from_text failed: {e}");
        pyo3::exceptions::PyValueError::new_err(e.to_string())
    })
}

/// I due decoratori che sostituiscono l'argomento `filter_data` di un pipe `text_filter` con
/// l'insieme dei `MatchFund` dei fondi già noti, prima di chiamare la funzione decorata.
///
/// Sono classi usate come decoratori (`@investment_fund_filter_data`), non funzioni: è la forma
/// che il riferimento aveva, e per questo il nome Python è minuscolo come quello di una funzione.
/// La logica vive qui nello shim e non nel crate perché opera su un callable Python arbitrario e
/// su una lista di risultati Python — concetti che esistono solo da questo lato del confine.
///
/// La firma di `__call__` è `(pdf_blks, filter_data)`, esattamente quella del riferimento: i pipe
/// `text_filter` ricevono sempre quei due argomenti in quell'ordine.
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

/// Da dove si prendono i nomi dei fondi con cui rimpiazzare `filter_data`.
#[derive(Clone, Copy)]
enum FundSource {
    /// Il nome dei risultati `Fund` — il decoratore `fund_filter_data`.
    Funds,
    /// Il campo `fund` dei risultati `Equity`/`Bond` — `investment_fund_filter_data`.
    Investments,
}

/// L'insieme dei `MatchFund` ricavati da un `filter_data`.
///
/// I risultati che non sono del tipo cercato — e quelli il cui nome è ancora una promessa, quindi
/// non una stringa — vengono semplicemente saltati, come nel riferimento: un fondo non ancora
/// risolto non ha un nome con cui confrontarsi.
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
