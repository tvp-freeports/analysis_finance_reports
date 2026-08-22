//! Rust port of `FundAssets` in `output/classes_schema.py`.
//!
//! `tot_assets`/`liabilities`/`net_assets` are plain `NonNegativeFloat` — never `Promise`-typed
//! in the Python original — so the accounting-equation check (`liabilities + net_assets ==
//! tot_assets`, within a `1e-4` tolerance) can run once at construction time, exactly like the
//! Python `@model_validator(mode="after")` does (it only ever runs right after all fields are
//! set, and none of the three float fields can change afterward — `fulfill_promises` never
//! touches them, only `date`/`currency` are promise-bearing).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::commons::consts::Currency;
use crate::core::promisable::{self, FloatConstraint, PromisableFields, Promised};
use crate::core::promise::Promise;
use crate::core::py_date::SimpleDate;

#[pyclass(module = "freeports._native")]
#[derive(Debug, Clone, PartialEq)]
pub struct FundAssets {
    fund: String,
    date: Option<Promised<SimpleDate>>,
    tot_assets: f64,
    liabilities: f64,
    net_assets: f64,
    currency: Promised<Currency>,
}

impl PromisableFields for FundAssets {
    fn pending_promises(&self) -> Vec<(&'static str, Promise)> {
        let mut out = Vec::new();
        if let Some(Promised::Pending(p)) = &self.date {
            out.push(("date", p.clone()));
        }
        if let Promised::Pending(p) = &self.currency {
            out.push(("currency", p.clone()));
        }
        out
    }

    fn resolve_field(&mut self, py: Python<'_>, field: &'static str, value: Py<PyAny>) -> PyResult<()> {
        match field {
            "date" => {
                self.date = Some(Promised::Resolved(value.extract::<SimpleDate>(py)?));
                Ok(())
            }
            "currency" => {
                self.currency = Promised::Resolved(Currency::new(value.bind(py))?);
                Ok(())
            }
            _ => unreachable!("FundAssets has no promisable field {field:?}"),
        }
    }
}

#[pymethods]
impl FundAssets {
    #[new]
    #[pyo3(signature = (*, fund, tot_assets, liabilities, net_assets, currency, date = None))]
    fn new(
        fund: String,
        tot_assets: &Bound<'_, PyAny>,
        liabilities: &Bound<'_, PyAny>,
        net_assets: &Bound<'_, PyAny>,
        currency: &Bound<'_, PyAny>,
        date: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let tot_assets = promisable::extract_f64(tot_assets, FloatConstraint::NonNegative)?;
        let liabilities = promisable::extract_f64(liabilities, FloatConstraint::NonNegative)?;
        let net_assets = promisable::extract_f64(net_assets, FloatConstraint::NonNegative)?;
        let currency = promisable::extract_promised_currency(currency)?;
        let date = date
            .filter(|d| !d.is_none())
            .map(promisable::extract_promised::<SimpleDate>)
            .transpose()?;
        if (liabilities + net_assets - tot_assets).abs() > 1e-4 {
            return Err(PyValueError::new_err(format!(
                "liabilities ({liabilities}) + net_assets ({net_assets}) must equal tot_assets ({tot_assets})"
            )));
        }
        Ok(Self { fund, date, tot_assets, liabilities, net_assets, currency })
    }

    #[getter]
    fn fund(&self) -> &str {
        &self.fund
    }

    #[getter]
    fn date<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        match &self.date {
            None => Ok(py.None().into_bound(py)),
            Some(d) => d.clone().into_pyobject(py),
        }
    }

    #[getter]
    fn tot_assets(&self) -> f64 {
        self.tot_assets
    }

    #[getter]
    fn liabilities(&self) -> f64 {
        self.liabilities
    }

    #[getter]
    fn net_assets(&self) -> f64 {
        self.net_assets
    }

    #[getter]
    fn currency(&self) -> Promised<Currency> {
        self.currency.clone()
    }

    #[classattr]
    fn __rust_model_fields__() -> (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str) {
        ("fund", "date", "tot_assets", "liabilities", "net_assets", "currency")
    }

    #[pyo3(signature = (*, mode = "python", by_alias = false))]
    fn model_dump<'py>(&self, py: Python<'py>, mode: &str, by_alias: bool) -> PyResult<Bound<'py, PyDict>> {
        if mode != "json" || !by_alias {
            return Err(PyValueError::new_err(
                "only model_dump(mode=\"json\", by_alias=True) is supported by this Rust port",
            ));
        }
        let dict = PyDict::new(py);
        match &self.date {
            None => dict.set_item("Date", py.None())?,
            Some(Promised::Resolved(d)) => {
                dict.set_item("Date", format!("{:04}-{:02}-{:02}", d.year, d.month, d.day))?;
            }
            Some(Promised::Pending(_)) => {
                return Err(PyValueError::new_err(
                    "cannot model_dump a FundAssets with an unresolved 'date' promise",
                ));
            }
        }
        dict.set_item("Total assets", self.tot_assets)?;
        dict.set_item("Total liabilities", self.liabilities)?;
        dict.set_item("Total net assets", self.net_assets)?;
        match &self.currency {
            Promised::Resolved(c) => dict.set_item("Currency", c.code())?,
            Promised::Pending(_) => {
                return Err(PyValueError::new_err(
                    "cannot model_dump a FundAssets with an unresolved 'currency' promise",
                ));
            }
        }
        Ok(dict)
    }

    fn fulfill_promises(
        &mut self,
        py: Python<'_>,
        mapping: &Bound<'_, PyDict>,
    ) -> PyResult<Option<Vec<Py<Self>>>> {
        let expansions = promisable::fulfill_promises(self, py, mapping)?;
        match expansions {
            None => Ok(None),
            Some(clones) => clones
                .into_iter()
                .map(|f| Py::new(py, f))
                .collect::<PyResult<Vec<_>>>()
                .map(Some),
        }
    }

    /// `date` is deliberately excluded, matching the Python original's
    /// `hash((tot_assets, liabilities, net_assets, currency, fund))`.
    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.tot_assets.to_bits().hash(&mut hasher);
        self.liabilities.to_bits().hash(&mut hasher);
        self.net_assets.to_bits().hash(&mut hasher);
        self.currency.hash(&mut hasher);
        self.fund.hash(&mut hasher);
        hasher.finish()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<PyRef<'_, Self>>().map(|o| *self == *o).unwrap_or(false)
    }

    fn __repr__(&self) -> String {
        format!(
            "FundAssets(fund={:?},tot_assets={},liabilities={},net_assets={},currency={:?})",
            self.fund, self.tot_assets, self.liabilities, self.net_assets, self.currency
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(py: Python<'_>, tot: f64, liab: f64, net: f64) -> PyResult<FundAssets> {
        let tot = tot.into_pyobject(py).unwrap().into_any();
        let liab = liab.into_pyobject(py).unwrap().into_any();
        let net = net.into_pyobject(py).unwrap().into_any();
        let currency = Currency::EUR.into_pyobject(py).unwrap().into_any();
        FundAssets::new("Fund X".into(), &tot, &liab, &net, &currency, None)
    }

    #[test]
    fn accepts_balanced_equation() {
        Python::attach(|py| {
            let f = make(py, 100.0, 40.0, 60.0).unwrap();
            assert_eq!(f.tot_assets, 100.0);
        });
    }

    #[test]
    fn rejects_unbalanced_equation() {
        Python::attach(|py| {
            assert!(make(py, 100.0, 40.0, 61.0).is_err());
        });
    }

    #[test]
    fn tolerates_small_float_error() {
        Python::attach(|py| {
            assert!(make(py, 100.0, 40.00005, 60.0).is_ok());
        });
    }

    #[test]
    fn date_defaults_to_none() {
        Python::attach(|py| {
            let f = make(py, 100.0, 40.0, 60.0).unwrap();
            assert!(f.date(py).unwrap().is_none());
        });
    }

    #[test]
    fn model_dump_includes_aliases_and_null_date() {
        Python::attach(|py| {
            let f = make(py, 100.0, 40.0, 60.0).unwrap();
            let dumped = f.model_dump(py, "json", true).unwrap();
            assert!(dumped.get_item("Date").unwrap().unwrap().is_none());
            assert_eq!(dumped.get_item("Total assets").unwrap().unwrap().extract::<f64>().unwrap(), 100.0);
            assert!(dumped.get_item("Currency").unwrap().is_some());
        });
    }

    #[test]
    fn hash_ignores_date_field() {
        Python::attach(|py| {
            let a = make(py, 100.0, 40.0, 60.0).unwrap();
            let date = SimpleDate { year: 2024, month: 1, day: 1 }.into_pyobject(py).unwrap().into_any();
            let tot = 100.0f64.into_pyobject(py).unwrap().into_any();
            let liab = 40.0f64.into_pyobject(py).unwrap().into_any();
            let net = 60.0f64.into_pyobject(py).unwrap().into_any();
            let currency = Currency::EUR.into_pyobject(py).unwrap().into_any();
            let b = FundAssets::new("Fund X".into(), &tot, &liab, &net, &currency, Some(&date)).unwrap();
            assert_eq!(a.__hash__(), b.__hash__());
            assert_ne!(a, b);
        });
    }

    #[test]
    fn currency_accepts_raw_string_like_python_try_convert() {
        Python::attach(|py| {
            let tot = 100.0f64.into_pyobject(py).unwrap().into_any();
            let liab = 40.0f64.into_pyobject(py).unwrap().into_any();
            let net = 60.0f64.into_pyobject(py).unwrap().into_any();
            let currency = "EUR".into_pyobject(py).unwrap().into_any();
            let f = FundAssets::new("Fund X".into(), &tot, &liab, &net, &currency, None).unwrap();
            assert_eq!(f.currency, Promised::Resolved(Currency::EUR));
        });
    }
}
