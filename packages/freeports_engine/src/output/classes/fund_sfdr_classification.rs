//! Rust port of `FundSfdrClassification` in `output/classes_schema.py`. Both fields are
//! `Field(exclude=True)` in the Python original, so `model_dump(mode="json", by_alias=True)`
//! always returns an empty dict — verified against `output/routines.py`, which dumps first and
//! then adds keys manually (`d["SFDR classification"] = ...`) rather than relying on the model
//! for any of its own fields.
//!
//! `article: PromisedSfdrArticle` has no `BeforeValidator` in the Python original (unlike
//! `PromisedCurrency`) — nothing in the codebase ever constructs an `SfdrArticle` from a raw
//! value, only passes existing instances around (see `core/consts.rs`'s
//! `pydantic_is_instance_schema` comment) — so the generic [`promisable::extract_promised`] is
//! enough here, no `Currency`-style dedicated coercion helper needed.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::commons::consts::SfdrArticle;
use crate::core::promisable::{self, PromisableFields, Promised};
use crate::core::promise::Promise;

#[pyclass(module = "freeports_engine")]
#[derive(Debug, Clone, PartialEq)]
pub struct FundSfdrClassification {
    fund: String,
    article: Promised<SfdrArticle>,
}

impl PromisableFields for FundSfdrClassification {
    fn pending_promises(&self) -> Vec<(&'static str, Promise)> {
        match &self.article {
            Promised::Pending(p) => vec![("article", p.clone())],
            Promised::Resolved(_) => vec![],
        }
    }

    fn resolve_field(&mut self, py: Python<'_>, field: &'static str, value: Py<PyAny>) -> PyResult<()> {
        match field {
            "article" => {
                self.article = Promised::Resolved(value.extract::<SfdrArticle>(py)?);
                Ok(())
            }
            _ => unreachable!("FundSfdrClassification has no promisable field {field:?}"),
        }
    }
}

#[pymethods]
impl FundSfdrClassification {
    #[new]
    pub fn new(fund: String, article: &Bound<'_, PyAny>) -> PyResult<Self> {
        let article = promisable::extract_promised::<SfdrArticle>(article)?;
        Ok(Self { fund, article })
    }

    #[getter]
    fn fund(&self) -> &str {
        &self.fund
    }

    #[getter]
    fn article(&self) -> Promised<SfdrArticle> {
        self.article.clone()
    }

    #[classattr]
    fn __rust_model_fields__() -> (&'static str, &'static str) {
        ("fund", "article")
    }

    #[pyo3(signature = (*, mode = "python", by_alias = false))]
    fn model_dump<'py>(&self, py: Python<'py>, mode: &str, by_alias: bool) -> PyResult<Bound<'py, PyDict>> {
        if mode != "json" || !by_alias {
            return Err(PyValueError::new_err(
                "only model_dump(mode=\"json\", by_alias=True) is supported by this Rust port",
            ));
        }
        Ok(PyDict::new(py))
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

    fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.fund.hash(&mut hasher);
        self.article.hash(&mut hasher);
        hasher.finish()
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<PyRef<'_, Self>>().map(|o| *self == *o).unwrap_or(false)
    }

    fn __repr__(&self) -> String {
        format!("FundSfdrClassification(fund={:?}, article={:?})", self.fund, self.article)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_resolved(py: Python<'_>, fund: &str, article: SfdrArticle) -> FundSfdrClassification {
        let article = article.into_pyobject(py).unwrap().into_any();
        FundSfdrClassification::new(fund.into(), &article).unwrap()
    }

    #[test]
    fn model_dump_is_always_empty() {
        Python::attach(|py| {
            let c = make_resolved(py, "X", SfdrArticle::ART_6);
            let dumped = c.model_dump(py, "json", true).unwrap();
            assert_eq!(dumped.len(), 0);
        });
    }

    #[test]
    fn hash_and_eq_consider_both_fields() {
        Python::attach(|py| {
            let a = make_resolved(py, "X", SfdrArticle::ART_6);
            let b = make_resolved(py, "X", SfdrArticle::ART_6);
            let c = make_resolved(py, "X", SfdrArticle::ART_8);
            let bound_b = Py::new(py, b).unwrap();
            let bound_c = Py::new(py, c).unwrap();
            assert!(a.__eq__(bound_b.bind(py)));
            assert!(!a.__eq__(bound_c.bind(py)));
        });
    }

    #[test]
    fn fulfill_promises_resolves_article() {
        Python::attach(|py| {
            let promise = Promise::from_parts("art", false, false).into_pyobject(py).unwrap().into_any();
            let mut c = FundSfdrClassification::new("X".into(), &promise).unwrap();
            let mapping = PyDict::new(py);
            mapping.set_item("art", SfdrArticle::ART_9).unwrap();
            let result = c.fulfill_promises(py, &mapping).unwrap();
            assert!(result.is_none());
            assert_eq!(c.article, Promised::Resolved(SfdrArticle::ART_9));
        });
    }
}
