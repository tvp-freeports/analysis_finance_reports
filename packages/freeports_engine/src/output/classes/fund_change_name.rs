//! Rust port of `FundChangeName`/`FundRename`/`FundMerge` in
//! `packages/freeports_core/src/freeports/_internals/output/classes_schema.py`. `FundRename`
//! and `FundMerge` are never subclassed anywhere (verified across
//! `analysis_finance_reports_formats`) and share every field/behavior — they differ *only* in
//! Python type identity, used purely for `isinstance()` dispatch in `output/routines.py` (rename
//! vs. merge CSV rows). Rather than reach for PyO3's `#[pyclass(subclass)]`/`extends` machinery
//! (which would need extra work to keep the *most derived* type through `fulfill_promises`'s
//! cloning), they're implemented as two independent pyclasses wrapping the same inner data +
//! shared logic — a small amount of boilerplate duplication (via `fund_change_name_variant!`)
//! beats a generic mechanism for a two-variant, zero-extra-field case.
//!
//! Placed at the TOP level of the `freeports_engine` pymodule (not nested under `.core`), same
//! convention as `Currency`/`SfdrArticle`/`FinancialInstrument`: `core/serialization.py`'s
//! `_enum_to_tag`-style tag round-trip (extended here to any class exposing
//! `__rust_model_fields__`, see that module) needs `importlib.import_module(type(v).__module__)`
//! to succeed, and nested PyO3 submodules aren't real `sys.modules` entries.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::core::promisable::{self, PromisableFields, Promised};
use crate::core::promise::Promise;
use crate::core::py_date::SimpleDate;

#[derive(Debug, Clone)]
struct FundChangeNameData {
    old_name: String,
    current_name: String,
    date: Promised<SimpleDate>,
}

impl FundChangeNameData {
    fn hash_value(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.old_name.hash(&mut hasher);
        self.current_name.hash(&mut hasher);
        self.date.hash(&mut hasher);
        hasher.finish()
    }

    /// Only `model_dump(mode="json", by_alias=True)` is ever called on these classes (verified
    /// by grep across both `freeports_core` and `analysis_finance_reports_formats`), so that's
    /// the only combination implemented — matches Pydantic's aliasing (`old_name` -> "Old name",
    /// `date` -> "From") and its `Field(exclude=True)` on `current_name` (dropped from output).
    fn model_dump_json_by_alias<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("Old name", &self.old_name)?;
        match &self.date {
            Promised::Resolved(d) => {
                dict.set_item("From", format!("{:04}-{:02}-{:02}", d.year, d.month, d.day))?;
            }
            Promised::Pending(_) => {
                return Err(PyValueError::new_err(
                    "cannot model_dump a FundChangeName with an unresolved 'date' promise",
                ));
            }
        }
        Ok(dict)
    }
}

impl PromisableFields for FundChangeNameData {
    fn pending_promises(&self) -> Vec<(&'static str, Promise)> {
        match &self.date {
            Promised::Pending(p) => vec![("date", p.clone())],
            Promised::Resolved(_) => vec![],
        }
    }

    fn resolve_field(&mut self, py: Python<'_>, field: &'static str, value: Py<PyAny>) -> PyResult<()> {
        match field {
            "date" => {
                self.date = Promised::Resolved(value.extract::<SimpleDate>(py)?);
                Ok(())
            }
            _ => unreachable!("FundChangeNameData has no promisable field {field:?}"),
        }
    }
}

macro_rules! fund_change_name_variant {
    ($name:ident) => {
        #[pyclass(module = "freeports_engine")]
        #[derive(Clone)]
        pub struct $name {
            inner: FundChangeNameData,
        }

        #[pymethods]
        impl $name {
            #[new]
            fn new(old_name: String, current_name: String, date: &Bound<'_, PyAny>) -> PyResult<Self> {
                let date = promisable::extract_promised::<SimpleDate>(date)?;
                Ok(Self {
                    inner: FundChangeNameData { old_name, current_name, date },
                })
            }

            #[getter]
            fn old_name(&self) -> &str {
                &self.inner.old_name
            }

            #[getter]
            fn current_name(&self) -> &str {
                &self.inner.current_name
            }

            #[getter]
            fn date(&self) -> Promised<SimpleDate> {
                self.inner.date.clone()
            }

            /// Field names, in declaration order — read generically by
            /// `core/serialization.py`'s fixture (de)serialization (the `__pydantic__` tag
            /// scheme, extended to recognize any class with this classattr, not just
            /// `pydantic.BaseModel` subclasses). Deliberately *not* the CSV-export alias names.
            #[classattr]
            fn __rust_model_fields__() -> (&'static str, &'static str, &'static str) {
                ("old_name", "current_name", "date")
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
                self.inner.model_dump_json_by_alias(py)
            }

            fn fulfill_promises(
                &mut self,
                py: Python<'_>,
                mapping: &Bound<'_, PyDict>,
            ) -> PyResult<Option<Vec<Py<Self>>>> {
                let expansions = promisable::fulfill_promises(&mut self.inner, py, mapping)?;
                match expansions {
                    None => Ok(None),
                    Some(clones) => clones
                        .into_iter()
                        .map(|inner| Py::new(py, Self { inner }))
                        .collect::<PyResult<Vec<_>>>()
                        .map(Some),
                }
            }

            fn __hash__(&self) -> u64 {
                self.inner.hash_value()
            }

            fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
                other
                    .extract::<PyRef<'_, Self>>()
                    .map(|o| {
                        self.inner.old_name == o.inner.old_name
                            && self.inner.current_name == o.inner.current_name
                            && self.inner.date == o.inner.date
                    })
                    .unwrap_or(false)
            }

            fn __repr__(&self) -> String {
                format!(
                    "{}(old_name={:?}, current_name={:?}, date={:?})",
                    stringify!($name),
                    self.inner.old_name,
                    self.inner.current_name,
                    self.inner.date,
                )
            }
        }
    };
}

fund_change_name_variant!(FundRename);
fund_change_name_variant!(FundMerge);

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyList;

    fn make_date(py: Python<'_>, y: i32, m: u8, d: u8) -> Bound<'_, PyAny> {
        SimpleDate { year: y, month: m, day: d }
            .into_pyobject(py)
            .unwrap()
            .into_any()
    }

    fn make_promise<'py>(py: Python<'py>, id: &str, strict: bool, multiple: bool) -> Bound<'py, PyAny> {
        Promise::from_parts(id, strict, multiple)
            .into_pyobject(py)
            .unwrap()
            .into_any()
    }

    fn rename(old_name: &str, current_name: &str, date: &Bound<'_, PyAny>) -> FundRename {
        FundRename::new(old_name.into(), current_name.into(), date).unwrap()
    }

    #[test]
    fn constructs_and_reads_fields() {
        Python::attach(|py| {
            let date = make_date(py, 2025, 7, 2);
            let obj = rename("Old Fund", "New Fund", &date);
            assert_eq!(obj.old_name(), "Old Fund");
            assert_eq!(obj.current_name(), "New Fund");
            assert_eq!(obj.inner.date, Promised::Resolved(SimpleDate { year: 2025, month: 7, day: 2 }));
        });
    }

    #[test]
    fn constructor_recognizes_a_pending_promise() {
        Python::attach(|py| {
            let promise = make_promise(py, "d!", true, false);
            let obj = rename("A", "B", &promise);
            match obj.inner.date {
                Promised::Pending(p) => assert!(p.strict()),
                Promised::Resolved(_) => panic!("expected a pending promise"),
            }
        });
    }

    #[test]
    fn model_dump_excludes_current_name_and_uses_aliases() {
        Python::attach(|py| {
            let date = make_date(py, 2025, 7, 2);
            let obj = FundMerge::new("Old Fund".into(), "New Fund".into(), &date).unwrap();
            let dumped = obj.model_dump(py, "json", true).unwrap();
            assert_eq!(dumped.len(), 2);
            assert_eq!(dumped.get_item("Old name").unwrap().unwrap().extract::<String>().unwrap(), "Old Fund");
            assert_eq!(dumped.get_item("From").unwrap().unwrap().extract::<String>().unwrap(), "2025-07-02");
            assert!(dumped.get_item("current_name").unwrap().is_none());
        });
    }

    #[test]
    fn model_dump_rejects_unsupported_kwargs() {
        Python::attach(|py| {
            let date = make_date(py, 2020, 1, 1);
            let obj = rename("A", "B", &date);
            assert!(obj.model_dump(py, "python", false).is_err());
        });
    }

    #[test]
    fn fulfill_promises_resolves_single_valued_date_in_place() {
        Python::attach(|py| {
            let promise = make_promise(py, "d", false, false);
            let mut obj = rename("A", "B", &promise);
            let mapping = PyDict::new(py);
            mapping.set_item("d", make_date(py, 2024, 12, 31)).unwrap();
            let result = obj.fulfill_promises(py, &mapping).unwrap();
            assert!(result.is_none());
            match obj.inner.date {
                Promised::Resolved(SimpleDate { year, month, day }) => {
                    assert_eq!((year, month, day), (2024, 12, 31));
                }
                Promised::Pending(_) => panic!("expected resolved date"),
            }
        });
    }

    #[test]
    fn fulfill_promises_drops_non_strict_unresolved() {
        Python::attach(|py| {
            let promise = make_promise(py, "missing", false, false);
            let mut obj = rename("A", "B", &promise);
            let mapping = PyDict::new(py);
            let result = obj.fulfill_promises(py, &mapping).unwrap();
            assert!(matches!(result, Some(v) if v.is_empty()));
        });
    }

    #[test]
    fn fulfill_promises_raises_for_strict_unresolved() {
        Python::attach(|py| {
            let promise = make_promise(py, "missing!", true, false);
            let mut obj = rename("A", "B", &promise);
            let mapping = PyDict::new(py);
            assert!(obj.fulfill_promises(py, &mapping).is_err());
        });
    }

    #[test]
    fn fulfill_promises_expands_multiple_dates() {
        Python::attach(|py| {
            let promise = make_promise(py, "dates[]", false, false);
            let mut obj = rename("A", "B", &promise);
            let mapping = PyDict::new(py);
            let values = PyList::new(py, [make_date(py, 2020, 1, 1), make_date(py, 2021, 2, 2)]).unwrap();
            mapping.set_item("dates", values).unwrap();
            let result = obj.fulfill_promises(py, &mapping).unwrap().unwrap();
            assert_eq!(result.len(), 2);
        });
    }
}
