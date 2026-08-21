//! Rust port of `FundEsgIndicator` in `output/classes_schema.py`.
//!
//! The Python original defines neither `__hash__` nor `__eq__` of its own, so Pydantic's default
//! `BaseModel.__eq__` (type + every field equal) applies, and — Python's normal rule for a class
//! with `__eq__` but no `__hash__` — instances are **unhashable** (verified: `hash(FundEsgIndicator(...))`
//! raises `TypeError`). This port implements `__eq__` but deliberately does not implement
//! `__hash__`, matching that.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::core::promisable::{self, PromisableFields, Promised};
use crate::core::promise::Promise;

#[pyclass(module = "freeports_engine")]
#[derive(Debug, Clone, PartialEq)]
pub struct FundEsgIndicator {
    fund: Promised<String>,
    name: String,
    value: String,
}

impl PromisableFields for FundEsgIndicator {
    fn pending_promises(&self) -> Vec<(&'static str, Promise)> {
        match &self.fund {
            Promised::Pending(p) => vec![("fund", p.clone())],
            Promised::Resolved(_) => vec![],
        }
    }

    fn resolve_field(&mut self, py: Python<'_>, field: &'static str, value: Py<PyAny>) -> PyResult<()> {
        match field {
            "fund" => {
                self.fund = Promised::Resolved(value.extract::<String>(py)?);
                Ok(())
            }
            _ => unreachable!("FundEsgIndicator has no promisable field {field:?}"),
        }
    }
}

#[pymethods]
impl FundEsgIndicator {
    #[new]
    fn new(fund: &Bound<'_, PyAny>, name: String, value: String) -> PyResult<Self> {
        let fund = promisable::extract_promised::<String>(fund)?;
        Ok(Self { fund, name, value })
    }

    #[getter]
    fn fund(&self) -> Promised<String> {
        self.fund.clone()
    }

    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[getter]
    fn value(&self) -> &str {
        &self.value
    }

    #[classattr]
    fn __rust_model_fields__() -> (&'static str, &'static str, &'static str) {
        ("fund", "name", "value")
    }

    #[pyo3(signature = (*, mode = "python", by_alias = false))]
    fn model_dump<'py>(&self, py: Python<'py>, mode: &str, by_alias: bool) -> PyResult<Bound<'py, PyDict>> {
        if mode != "json" || !by_alias {
            return Err(PyValueError::new_err(
                "only model_dump(mode=\"json\", by_alias=True) is supported by this Rust port",
            ));
        }
        let dict = PyDict::new(py);
        dict.set_item("Indicator", &self.name)?;
        dict.set_item("Value", &self.value)?;
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

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<PyRef<'_, Self>>().map(|o| *self == *o).unwrap_or(false)
    }

    fn __repr__(&self) -> String {
        format!(
            "FundEsgIndicator(fund={:?}, name={:?}, value={:?})",
            self.fund, self.name, self.value
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(py: Python<'_>, fund: &str, name: &str, value: &str) -> FundEsgIndicator {
        let fund = fund.into_pyobject(py).unwrap().into_any();
        FundEsgIndicator::new(&fund, name.into(), value.into()).unwrap()
    }

    #[test]
    fn model_dump_uses_aliases_and_excludes_fund() {
        Python::attach(|py| {
            let e = make(py, "X", "Some Indicator", "Yes");
            let dumped = e.model_dump(py, "json", true).unwrap();
            assert_eq!(dumped.len(), 2);
            assert_eq!(dumped.get_item("Indicator").unwrap().unwrap().extract::<String>().unwrap(), "Some Indicator");
            assert_eq!(dumped.get_item("Value").unwrap().unwrap().extract::<String>().unwrap(), "Yes");
        });
    }

    #[test]
    fn eq_compares_all_fields() {
        Python::attach(|py| {
            let a = make(py, "X", "n", "v");
            let b = make(py, "X", "n", "v");
            let c = make(py, "X", "n", "different");
            let bound_b = Py::new(py, b).unwrap();
            let bound_c = Py::new(py, c).unwrap();
            assert!(a.__eq__(bound_b.bind(py)));
            assert!(!a.__eq__(bound_c.bind(py)));
        });
    }

    #[test]
    fn fulfill_promises_resolves_fund() {
        Python::attach(|py| {
            let promise = Promise::from_parts("f", false, false).into_pyobject(py).unwrap().into_any();
            let mut e = FundEsgIndicator::new(&promise, "n".into(), "v".into()).unwrap();
            let mapping = PyDict::new(py);
            mapping.set_item("f", "Resolved Fund").unwrap();
            let result = e.fulfill_promises(py, &mapping).unwrap();
            assert!(result.is_none());
            assert_eq!(e.fund, Promised::Resolved("Resolved Fund".to_string()));
        });
    }
}
