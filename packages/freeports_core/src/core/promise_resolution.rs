//! Rust port of the free functions in
//! `packages/freeports_core/src/freeports/_internals/core/promises.py`:
//! `build_promise_multimap`, `merge_into_multimap`, `flatten_promise_map` — the
//! promise-resolution-map bookkeeping used by `cli/main.py`'s pipeline orchestration to collect
//! every page's raw deserialized values, keyed by promise id, before resolving the whole
//! document's promises in one pass (`dr.fulfill_promises(...)`, see `core/promisable.rs`).
//!
//! `Promise` itself lives in `promise.rs`. `PromisableDict`, the `Promised*` Pydantic type
//! aliases, and `try_convert_to_currency` are deliberately NOT ported here: a full-codebase grep
//! confirmed they are used *only* by the now fully-dead `_Legacy*` classes in
//! `output/classes_schema.py` (every concrete output class is Rust-backed as of this same
//! session) — there is no live Python code left that needs them, so porting them would just be
//! Rust code in service of dead code. See `agent-memory/rust-rewrite-plan.md`.
//!
//! Operates on plain Python `dict`/`list` throughout, not typed Rust collections, because the
//! values flowing through this map are genuinely heterogeneous (numbers, strings, dates,
//! `Currency`, nested `Promise`s, ...) — same category as `PromisableDict.fulfill_promises`
//! (`core/promisable.rs`), where that dynamism is inherent to the problem, not incidental.

use std::collections::HashMap;

use pyo3::create_exception;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::promise::Promise;

create_exception!(
    freeports._native,
    CircularPromisesChain,
    pyo3::exceptions::PyException,
    "Raised when a promise chain references itself, directly or indirectly, during \
     `flatten_promise_map`."
);

#[pyfunction]
#[pyo3(name = "build_promise_multimap")]
pub fn py_build_promise_multimap(py: Python<'_>) -> Bound<'_, PyDict> {
    PyDict::new(py)
}

/// Appends every `(key, value)` pair of `d` onto `mmap[key]` (creating the list on first use) —
/// the Rust equivalent of the Python original's `defaultdict(list)`-backed
/// `mmap[k].append(v)`, without needing a real `collections.defaultdict` from Rust: nothing else
/// in this module (or its caller) relies on `mmap` being an actual `defaultdict` rather than a
/// plain `dict` built up this way.
#[pyfunction]
#[pyo3(name = "merge_into_multimap")]
pub fn py_merge_into_multimap(mmap: &Bound<'_, PyDict>, d: &Bound<'_, PyDict>) -> PyResult<()> {
    let py = mmap.py();
    for (k, v) in d.iter() {
        match mmap.get_item(&k)? {
            Some(existing) => {
                existing.cast_into::<PyList>()?.append(v)?;
            }
            None => {
                mmap.set_item(k, PyList::new(py, [v])?)?;
            }
        }
    }
    Ok(())
}

/// Port of `flatten_promise_map`. Ported as close to line-for-line as Rust allows — including
/// quirks that look redundant (e.g. a key with N `Promise`-valued entries gets queued N times in
/// the worklist, costing extra no-op passes once already resolved) — because promise resolution
/// is an explicit "architectural fixed point" of this migration
/// (`agent-memory/rust-rewrite-plan.md`): behavior must match exactly, not just "morally".
#[pyfunction]
#[pyo3(name = "flatten_promise_map")]
pub fn py_flatten_promise_map<'py>(mapping: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyDict>> {
    let py = mapping.py();
    let flattened = PyDict::new(py);
    let mut resolve_history: HashMap<String, Vec<String>> = HashMap::new();
    let mut promises: Vec<String> = Vec::new();

    for (key_any, values_any) in mapping.iter() {
        let key: String = key_any.extract()?;
        let values = values_any.cast::<PyList>()?;
        let mut flat_values: Vec<Bound<'py, PyAny>> = Vec::new();
        for value in values.iter() {
            if value.extract::<Promise>().is_ok() {
                promises.push(key.clone());
                resolve_history.insert(key.clone(), Vec::new());
            } else {
                flat_values.push(value);
            }
        }
        if !flat_values.is_empty() {
            if flat_values.len() == 1 && !resolve_history.contains_key(&key) {
                flattened.set_item(&key, &flat_values[0])?;
            } else {
                flattened.set_item(&key, PyList::new(py, &flat_values)?)?;
            }
        }
    }

    if promises.is_empty() {
        return Ok(flattened);
    }

    loop {
        let mut i = 0usize;
        loop {
            let p = promises[i].clone();
            let values = mapping
                .get_item(&p)?
                .expect("key was taken from mapping.items(), must still be present")
                .cast_into::<PyList>()?;
            let mut all_resolved = true;
            let len = values.len();
            for j in 0..len {
                let value = values.get_item(j)?;
                if let Ok(promise) = value.extract::<Promise>() {
                    let id = promise.get_id().to_string();
                    let history = resolve_history.get_mut(&p).expect("queued key must have a history entry");
                    if history.contains(&id) {
                        let debug_str = format!("{history:?} -> {id}");
                        return Err(CircularPromisesChain::new_err(format!(
                            "Circular reference detected in promise resolution chain: {debug_str}"
                        )));
                    }
                    history.push(id.clone());
                    let resolved = match mapping.get_item(&id)? {
                        Some(v) => v,
                        None => PyList::new(py, [value.clone()])?.into_any(),
                    };
                    let resolved = match resolved.cast::<PyList>() {
                        Ok(list) if list.len() == 1 => list.get_item(0)?,
                        _ => resolved,
                    };
                    values.set_item(j, resolved)?;
                    all_resolved = false;
                }
            }
            if all_resolved {
                let len = values.len();
                if len == 1 {
                    flattened.set_item(&p, values.get_item(0)?)?;
                } else if len > 1 {
                    let mut filtered: Vec<Bound<'py, PyAny>> = Vec::with_capacity(len);
                    for v in values.iter() {
                        if v.extract::<Promise>().is_err() {
                            filtered.push(v);
                        }
                    }
                    flattened.set_item(&p, PyList::new(py, &filtered)?)?;
                }
                promises.remove(i);
            } else {
                i += 1;
            }
            if i >= promises.len() {
                break;
            }
        }
        if promises.is_empty() {
            break;
        }
    }

    Ok(flattened)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(_py: Python<'_>, dict: &Bound<'_, PyDict>, key: &str) -> Py<PyAny> {
        dict.get_item(key).unwrap().unwrap().unbind()
    }

    #[test]
    fn multimap_appends_across_calls() {
        Python::attach(|py| {
            let mmap = py_build_promise_multimap(py);
            let d1 = PyDict::new(py);
            d1.set_item("a", 1).unwrap();
            py_merge_into_multimap(&mmap, &d1).unwrap();
            let d2 = PyDict::new(py);
            d2.set_item("a", 2).unwrap();
            py_merge_into_multimap(&mmap, &d2).unwrap();
            let list = get(py, &mmap, "a").extract::<Vec<i64>>(py).unwrap();
            assert_eq!(list, vec![1, 2]);
        });
    }

    #[test]
    fn flatten_single_value_becomes_scalar() {
        Python::attach(|py| {
            let mapping = PyDict::new(py);
            let values = PyList::new(py, [42]).unwrap();
            mapping.set_item("x", values).unwrap();
            let flattened = py_flatten_promise_map(&mapping).unwrap();
            assert_eq!(get(py, &flattened, "x").extract::<i64>(py).unwrap(), 42);
        });
    }

    #[test]
    fn flatten_multiple_values_stays_a_list() {
        Python::attach(|py| {
            let mapping = PyDict::new(py);
            let values = PyList::new(py, [1, 2, 3]).unwrap();
            mapping.set_item("x", values).unwrap();
            let flattened = py_flatten_promise_map(&mapping).unwrap();
            let list = get(py, &flattened, "x").extract::<Vec<i64>>(py).unwrap();
            assert_eq!(list, vec![1, 2, 3]);
        });
    }

    #[test]
    fn flatten_resolves_a_promise_reference() {
        Python::attach(|py| {
            let mapping = PyDict::new(py);
            let ref_promise = Promise::from_parts("target", false, false).into_pyobject(py).unwrap();
            mapping.set_item("source", PyList::new(py, [ref_promise]).unwrap()).unwrap();
            mapping.set_item("target", PyList::new(py, [99]).unwrap()).unwrap();
            let flattened = py_flatten_promise_map(&mapping).unwrap();
            assert_eq!(get(py, &flattened, "source").extract::<i64>(py).unwrap(), 99);
            assert_eq!(get(py, &flattened, "target").extract::<i64>(py).unwrap(), 99);
        });
    }

    /// A key with two `Promise`-valued entries gets queued twice in the worklist (once per
    /// promise found) — verified against real Python that this doesn't break resolution, just
    /// costs one redundant extra pass. `x: [Promise("a"), Promise("b")]` should resolve to
    /// `[1, 2]`, matching `merge_into_multimap`/`flatten_promise_map` called the same way in
    /// real Python (checked directly, not assumed).
    #[test]
    fn flatten_handles_a_key_with_multiple_promise_entries() {
        Python::attach(|py| {
            let mmap = py_build_promise_multimap(py);
            let d1 = PyDict::new(py);
            d1.set_item("x", Promise::from_parts("a", false, false).into_pyobject(py).unwrap()).unwrap();
            py_merge_into_multimap(&mmap, &d1).unwrap();
            let d2 = PyDict::new(py);
            d2.set_item("x", Promise::from_parts("b", false, false).into_pyobject(py).unwrap()).unwrap();
            py_merge_into_multimap(&mmap, &d2).unwrap();
            let d3 = PyDict::new(py);
            d3.set_item("a", 1).unwrap();
            py_merge_into_multimap(&mmap, &d3).unwrap();
            let d4 = PyDict::new(py);
            d4.set_item("b", 2).unwrap();
            py_merge_into_multimap(&mmap, &d4).unwrap();

            let flattened = py_flatten_promise_map(&mmap).unwrap();
            assert_eq!(get(py, &flattened, "a").extract::<i64>(py).unwrap(), 1);
            assert_eq!(get(py, &flattened, "b").extract::<i64>(py).unwrap(), 2);
            let x = get(py, &flattened, "x").extract::<Vec<i64>>(py).unwrap();
            assert_eq!(x, vec![1, 2]);
        });
    }

    #[test]
    fn flatten_resolves_a_chained_promise_reference() {
        Python::attach(|py| {
            let mapping = PyDict::new(py);
            let ref_a = Promise::from_parts("b", false, false).into_pyobject(py).unwrap();
            let ref_b = Promise::from_parts("c", false, false).into_pyobject(py).unwrap();
            mapping.set_item("a", PyList::new(py, [ref_a]).unwrap()).unwrap();
            mapping.set_item("b", PyList::new(py, [ref_b]).unwrap()).unwrap();
            mapping.set_item("c", PyList::new(py, [7]).unwrap()).unwrap();
            let flattened = py_flatten_promise_map(&mapping).unwrap();
            assert_eq!(get(py, &flattened, "a").extract::<i64>(py).unwrap(), 7);
        });
    }

    /// Verified against the real Python `flatten_promise_map` before writing this: a dangling
    /// reference (no entry for the referenced id anywhere in `mapping`) does NOT fall back to
    /// leaving the promise in place — the fallback default (`mapping.get(id, [value])`) hands
    /// the *same* unresolved `Promise` right back, so `all_resolved` never becomes `True` for
    /// this key and the second pass's cycle check (`value._id in resolve_history[p]`) fires,
    /// because the same id was already visited on the first pass. So an unresolvable reference
    /// surfaces as `CircularPromisesChain`, not a silent pass-through — a real (if surprising)
    /// behavior of the original that this port must reproduce exactly, not "fix".
    #[test]
    fn flatten_missing_reference_raises_circular_promises_chain() {
        Python::attach(|py| {
            let mapping = PyDict::new(py);
            let dangling = Promise::from_parts("nowhere", false, false).into_pyobject(py).unwrap();
            mapping.set_item("source", PyList::new(py, [dangling]).unwrap()).unwrap();
            let err = py_flatten_promise_map(&mapping).unwrap_err();
            assert!(err.is_instance_of::<CircularPromisesChain>(py));
        });
    }

    #[test]
    fn flatten_detects_circular_reference() {
        Python::attach(|py| {
            let mapping = PyDict::new(py);
            let ref_a = Promise::from_parts("b", false, false).into_pyobject(py).unwrap();
            let ref_b = Promise::from_parts("a", false, false).into_pyobject(py).unwrap();
            mapping.set_item("a", PyList::new(py, [ref_a]).unwrap()).unwrap();
            mapping.set_item("b", PyList::new(py, [ref_b]).unwrap()).unwrap();
            let err = py_flatten_promise_map(&mapping).unwrap_err();
            assert!(err.is_instance_of::<CircularPromisesChain>(py));
        });
    }

    #[test]
    fn flatten_empty_mapping_returns_empty() {
        Python::attach(|py| {
            let mapping = PyDict::new(py);
            let flattened = py_flatten_promise_map(&mapping).unwrap();
            assert_eq!(flattened.len(), 0);
        });
    }
}
