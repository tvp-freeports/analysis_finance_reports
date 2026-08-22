//! Rust port of `Fund` in `output/classes_schema.py`.
//!
//! The Python original has a genuinely surprising twist, verified empirically before porting:
//! `Fund.__init__` (a custom override, not a plain Pydantic field) does
//! `MatchFund.__init__(self, name)` (computing the deep-normalized form) and then
//! **overwrites** `self.name` with `self._n_name.upper()` — so the Pydantic-visible `.name`
//! field (and therefore CSV/`model_dump` output) is the *uppercased, deep-normalized* name, not
//! the raw constructor argument. Hashing/equality are based on the (non-uppercased) normalized
//! form via `MatchFund.__hash__`/`__eq__`.
//!
//! **Bug found in the Python original while porting, fixed here (not just replicated)**:
//! `PromisableDict.fulfill_promises` resolves a pending `name` via plain `setattr(self, "name",
//! value)`, which — unlike construction — does **not** go through `Fund.__init__`'s custom
//! normalization logic (Pydantic's `validate_assignment=True` re-validates the field's *type*,
//! it does not re-run a custom `__init__`). The result: a `Fund` built from a `Promise` name and
//! then resolved via `fulfill_promises` ends up with an un-normalized `.name` **and** a missing
//! `_core`/`_n_name`, which makes `hash()`/`__eq__` on it raise `AttributeError` — verified
//! against the real Python class before deciding this was worth fixing rather than replicating
//! (small/non-architectural difference: `PromisableDict.fulfill_promises` itself, the
//! architectural fixed point, is unaffected — this is `Fund`-specific glue around it). This Rust
//! port always computes the normalized form on resolution, so a promise-resolved `Fund` behaves
//! identically to a directly-constructed one.
//!
//! Reconstruction via `core/serialization.py`'s fixture round-trip (`cls(**resolved)`, see
//! `__rust_model_fields__`) is unaffected by any of this: it goes through `#[new]` (this port's
//! equivalent of the Python original going through `model_validate` -> `__init__`, verified to
//! behave identically — an already-normalized-and-uppercased name re-normalizes to itself,
//! idempotently).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::core::normalization;
use crate::core::promisable::{self, PromisableFields, Promised};
use crate::core::promise::Promise;

#[pyclass(module = "freeports._native")]
#[derive(Debug, Clone)]
pub struct Fund {
    /// `Resolved`: the deep-normalized (lowercase) name — `MatchFund.n_name`'s equivalent. The
    /// Python-visible `.name` getter uppercases it on read. `Pending`: the raw promise, exactly
    /// like every other `Promised` field.
    n_name: Promised<String>,
}

impl PromisableFields for Fund {
    fn pending_promises(&self) -> Vec<(&'static str, Promise)> {
        match &self.n_name {
            Promised::Pending(p) => vec![("name", p.clone())],
            Promised::Resolved(_) => vec![],
        }
    }

    fn resolve_field(&mut self, py: Python<'_>, field: &'static str, value: Py<PyAny>) -> PyResult<()> {
        match field {
            "name" => {
                let raw: String = value.extract(py)?;
                self.n_name = Promised::Resolved(normalization::deep_normalize_string(&raw));
                Ok(())
            }
            _ => unreachable!("Fund has no promisable field {field:?}"),
        }
    }
}

#[pymethods]
impl Fund {
    #[new]
    pub fn new(name: &Bound<'_, PyAny>) -> PyResult<Self> {
        let n_name = match promisable::extract_promised::<String>(name)? {
            Promised::Resolved(raw) => Promised::Resolved(normalization::deep_normalize_string(&raw)),
            Promised::Pending(p) => Promised::Pending(p),
        };
        Ok(Self { n_name })
    }

    #[getter]
    pub fn name<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        match &self.n_name {
            Promised::Resolved(n) => Ok(n.to_uppercase().into_pyobject(py)?.into_any()),
            Promised::Pending(p) => Ok(p.clone().into_pyobject(py)?.into_any()),
        }
    }

    #[classattr]
    fn __rust_model_fields__() -> (&'static str,) {
        ("name",)
    }

    #[pyo3(signature = (*, mode = "python", by_alias = false))]
    fn model_dump<'py>(&self, py: Python<'py>, mode: &str, by_alias: bool) -> PyResult<Bound<'py, PyDict>> {
        if mode != "json" || !by_alias {
            return Err(PyValueError::new_err(
                "only model_dump(mode=\"json\", by_alias=True) is supported by this Rust port",
            ));
        }
        let dict = PyDict::new(py);
        match &self.n_name {
            Promised::Resolved(n) => dict.set_item("Name", n.to_uppercase())?,
            Promised::Pending(_) => {
                return Err(PyValueError::new_err(
                    "cannot model_dump a Fund with an unresolved 'name' promise",
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

    /// Hashes the promise itself when pending (matches `hash(self.name)` in the Python
    /// original), or the normalized name when resolved. Uses direct `n_name` equality rather
    /// than replicating `MatchFund.__eq__`'s literal `hash(self) == hash(other)` comparison —
    /// value equality is unambiguously the intent (hash-equality is only an intentional shortcut
    /// when it's *consistent* with value equality, and a hash collision would make the Python
    /// original wrongly report equality too; treated as an acceptable non-architectural
    /// difference, not a behavior this migration needs to preserve bit-for-bit).
    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        match &self.n_name {
            Promised::Resolved(n) => {
                let mut hasher = DefaultHasher::new();
                n.hash(&mut hasher);
                Ok(hasher.finish() as isize)
            }
            Promised::Pending(p) => p.clone().into_pyobject(py)?.hash(),
        }
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .extract::<PyRef<'_, Self>>()
            .map(|o| self.n_name == o.n_name)
            .unwrap_or(false)
    }

    fn __repr__<'py>(&self, py: Python<'py>) -> PyResult<String> {
        let name = self.name(py)?;
        Ok(format!("Fund(name={})", name.repr()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fund(name: &str) -> Fund {
        Python::attach(|py| {
            let name = name.into_pyobject(py).unwrap().into_any();
            Fund::new(&name).unwrap()
        })
    }

    #[test]
    fn normalizes_and_uppercases_name() {
        Python::attach(|py| {
            let f = make_fund("Café   Fund");
            let name: String = f.name(py).unwrap().extract().unwrap();
            assert_eq!(name, "CAFE FUND");
        });
    }

    #[test]
    fn equal_across_case_and_accent_variants() {
        let a = make_fund("Café   Fund");
        let b = make_fund("CAFE FUND");
        Python::attach(|py| {
            let bound_b = Py::new(py, b).unwrap();
            assert!(a.__eq__(bound_b.bind(py)));
            assert_eq!(a.__hash__(py).unwrap(), bound_b.bind(py).borrow().__hash__(py).unwrap());
        });
    }

    #[test]
    fn constructor_recognizes_a_pending_promise() {
        Python::attach(|py| {
            let promise = Promise::from_parts("f", false, false).into_pyobject(py).unwrap().into_any();
            let f = Fund::new(&promise).unwrap();
            assert!(matches!(f.n_name, Promised::Pending(_)));
        });
    }

    #[test]
    fn fulfill_promises_normalizes_the_resolved_name_fixing_the_python_bug() {
        Python::attach(|py| {
            let promise = Promise::from_parts("f", false, false).into_pyobject(py).unwrap().into_any();
            let mut f = Fund::new(&promise).unwrap();
            let mapping = PyDict::new(py);
            mapping.set_item("f", "Café   Fund").unwrap();
            let result = f.fulfill_promises(py, &mapping).unwrap();
            assert!(result.is_none());
            let name: String = f.name(py).unwrap().extract().unwrap();
            assert_eq!(name, "CAFE FUND");
            // Must not panic (this is exactly what's broken in the Python original).
            assert!(f.__hash__(py).is_ok());
        });
    }

    #[test]
    fn model_dump_uses_uppercased_name() {
        Python::attach(|py| {
            let f = make_fund("Café Fund");
            let dumped = f.model_dump(py, "json", true).unwrap();
            assert_eq!(dumped.get_item("Name").unwrap().unwrap().extract::<String>().unwrap(), "CAFE FUND");
        });
    }
}
