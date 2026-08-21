//! Rust port of `AssetsManager`/`ManagementCompany`/`InvestmentsManager` in
//! `packages/freeports_core/src/freeports/_internals/output/classes_schema.py`. Like
//! `FundChangeName`/`FundRename`/`FundMerge` (`core/fund_change_name.rs`), the abstract base
//! (`AssetsManager`) is never directly instantiated and the two concrete variants are never
//! subclassed — so they're two independent pyclasses generated from one macro, sharing field
//! layout and logic, rather than a PyO3 `#[pyclass(subclass)]` hierarchy.
//!
//! Neither field is ever `Promise`-typed in the Python original (`name: str`,
//! `managed_funds: Set[str]`), so `fulfill_promises` is a direct no-op here rather than going
//! through `core::promisable`'s generic machinery — there is nothing for that machinery to do.

use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet};

macro_rules! assets_manager_variant {
    ($name:ident) => {
        #[pyclass(module = "freeports_engine")]
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            name: String,
            managed_funds: BTreeSet<String>,
        }

        #[pymethods]
        impl $name {
            // `managed_funds` accepts any iterable, not just a real Python `set`/`frozenset`:
            // verified that Pydantic's `Set[str]`-typed field coerces from a plain `list` too
            // (`ManagementCompany(managed_funds=[...])` works in the original), and a real
            // deserializer (`deserialize/standard_funcs.py`) is only guaranteed to hand this a
            // generic iterable, not necessarily a set — extracting straight to `BTreeSet<String>`
            // would reject anything that isn't already a Python set/frozenset, which is stricter
            // than the original. Collecting via `try_iter` instead matches Pydantic's coercion.
            #[new]
            pub fn new(name: String, managed_funds: &Bound<'_, PyAny>) -> PyResult<Self> {
                let managed_funds: BTreeSet<String> = managed_funds
                    .try_iter()?
                    .map(|item| item?.extract::<String>())
                    .collect::<PyResult<_>>()?;
                Ok(Self { name, managed_funds })
            }

            #[getter]
            fn name(&self) -> &str {
                &self.name
            }

            #[getter]
            fn managed_funds<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PySet>> {
                PySet::new(py, &self.managed_funds)
            }

            /// Field names, in declaration order — see `core/fund_change_name.rs`'s
            /// `__rust_model_fields__` for why this exists (generic fixture round-trip in
            /// `core/serialization.py`).
            #[classattr]
            fn __rust_model_fields__() -> (&'static str, &'static str) {
                ("name", "managed_funds")
            }

            #[pyo3(signature = (*, mode = "python", by_alias = false))]
            fn model_dump<'py>(
                &self,
                py: Python<'py>,
                mode: &str,
                by_alias: bool,
            ) -> PyResult<Bound<'py, PyDict>> {
                if mode != "json" || !by_alias {
                    return Err(PyValueError::new_err(
                        "only model_dump(mode=\"json\", by_alias=True) is supported by this Rust port",
                    ));
                }
                let dict = PyDict::new(py);
                dict.set_item("Name", &self.name)?;
                Ok(dict)
            }

            /// No field of this class is ever `Promise`-typed in the Python original, so
            /// resolution is always a no-op — matches `PromisableDict.fulfill_promises` on an
            /// instance with no pending promises, which always returns `None`.
            fn fulfill_promises(
                &mut self,
                _py: Python<'_>,
                _mapping: &Bound<'_, PyDict>,
            ) -> PyResult<Option<Vec<Py<Self>>>> {
                Ok(None)
            }

            fn __hash__(&self) -> u64 {
                let mut hasher = DefaultHasher::new();
                self.name.hash(&mut hasher);
                self.managed_funds.hash(&mut hasher);
                hasher.finish()
            }

            fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
                other
                    .extract::<PyRef<'_, Self>>()
                    .map(|o| *self == *o)
                    .unwrap_or(false)
            }

            fn __repr__(&self) -> String {
                format!("{}(\"{}\")", stringify!($name), self.name)
            }
        }
    };
}

assets_manager_variant!(ManagementCompany);
assets_manager_variant!(InvestmentsManager);

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyList;

    fn make_manager(py: Python<'_>, name: &str, funds: &[&str]) -> ManagementCompany {
        let list = PyList::new(py, funds).unwrap();
        let set = PySet::new(py, list.iter()).unwrap();
        ManagementCompany::new(name.into(), set.as_any()).unwrap()
    }

    #[test]
    fn constructs_and_reads_fields() {
        Python::attach(|py| {
            let m = make_manager(py, "Acme", &["Fund A", "Fund B"]);
            assert_eq!(m.name(), "Acme");
            let funds = m.managed_funds(py).unwrap();
            assert_eq!(funds.len(), 2);
            assert!(funds.contains("Fund A").unwrap());
        });
    }

    /// Verified against real Pydantic before writing this: `ManagementCompany(managed_funds=[...])`
    /// (a plain list, not a set) works in the original, because `Set[str]` coerces from any
    /// iterable — found via a real deserializer (`deserialize/standard_funcs.py`) failing this
    /// exact way when it passed a `list` straight through without wrapping it in `set(...)`.
    #[test]
    fn accepts_a_plain_list_not_just_a_real_set() {
        Python::attach(|py| {
            let list = PyList::new(py, ["Fund A", "Fund B"]).unwrap();
            let m = ManagementCompany::new("Acme".into(), list.as_any()).unwrap();
            assert_eq!(m.managed_funds.len(), 2);
        });
    }

    #[test]
    fn model_dump_excludes_managed_funds() {
        Python::attach(|py| {
            let m = make_manager(py, "Acme", &["Fund A"]);
            let dumped = m.model_dump(py, "json", true).unwrap();
            assert_eq!(dumped.len(), 1);
            assert_eq!(dumped.get_item("Name").unwrap().unwrap().extract::<String>().unwrap(), "Acme");
        });
    }

    #[test]
    fn hash_and_eq_ignore_managed_funds_insertion_order() {
        Python::attach(|py| {
            let a = make_manager(py, "Acme", &["Fund A", "Fund B"]);
            let b = make_manager(py, "Acme", &["Fund B", "Fund A"]);
            assert_eq!(a, b);
            assert_eq!(a.__hash__(), b.__hash__());
        });
    }

    #[test]
    fn fulfill_promises_is_always_a_no_op() {
        Python::attach(|py| {
            let mut m = make_manager(py, "Acme", &["Fund A"]);
            let mapping = PyDict::new(py);
            assert!(m.fulfill_promises(py, &mapping).unwrap().is_none());
        });
    }

    #[test]
    fn cross_type_eq_is_false() {
        Python::attach(|py| {
            let a = make_manager(py, "Acme", &["Fund A"]);
            let im = InvestmentsManager::new("Acme".into(), PySet::new(py, ["Fund A"]).unwrap().as_any()).unwrap();
            let bound_im = Py::new(py, im).unwrap();
            assert!(!a.__eq__(bound_im.bind(py)));
        });
    }
}
