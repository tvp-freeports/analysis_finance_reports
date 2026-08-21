//! Deferred value resolution: a promise pattern where values can be resolved later from a
//! mapping built up while a whole multi-page document is parsed.
//!
//! Rust port of `packages/freeports_core/src/freeports/_internals/core/promises.py::Promise`.
//! Only the `Promise` class itself is ported — `PromisableDict` (a Pydantic mixin, same
//! situation as `MatchFund`: it must stay Python to keep working as a mixin base for
//! `BaseModel` subclasses) and the free functions (`build_promise_multimap`,
//! `merge_into_multimap`, `flatten_promise_map`, `fulfill_promises`) stay Python for now — they
//! operate on plain dicts/lists of arbitrary Python values, not on `Promise` internals, so
//! there's no shared logic here worth duplicating yet.

use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// A deferred reference to a value that will be resolved later from a
/// `{promise_id: value}` mapping accumulated across a document's pages.
#[pyclass(eq, frozen, hash)]
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Promise {
    id: String,
    strict: bool,
    multiple: bool,
}

impl Promise {
    /// Parses the `!`/`[]` suffix syntax into flags, exactly like the Python constructor: a
    /// trailing `!` (when `strict` wasn't already `True`) sets strict mode and is stripped from
    /// the id; a trailing `[]` (when `multiple` wasn't already `True`) sets multiple mode and is
    /// stripped too. Order matters — `!` is checked (and stripped) before `[]`, matching the
    /// original's `if not strict: ...` then `if not multiple: ...` sequence.
    pub fn from_parts(promise_id: &str, mut strict: bool, mut multiple: bool) -> Self {
        let mut id = promise_id.to_string();
        if !strict && id.ends_with('!') {
            id.pop();
            strict = true;
        }
        if !multiple && id.ends_with("[]") {
            id.truncate(id.len() - 2);
            multiple = true;
        }
        Promise { id, strict, multiple }
    }
}

#[pymethods]
impl Promise {
    #[new]
    #[pyo3(signature = (promise_id, strict = false, multiple = false))]
    fn new(promise_id: &str, strict: bool, multiple: bool) -> Self {
        Promise::from_parts(promise_id, strict, multiple)
    }

    #[getter]
    #[pyo3(name = "_id")]
    pub fn get_id(&self) -> &str {
        &self.id
    }

    #[getter]
    pub fn strict(&self) -> bool {
        self.strict
    }

    #[getter]
    pub fn multiple(&self) -> bool {
        self.multiple
    }

    /// Resolves this promise's value from `mapping`. Raises `KeyError` if the id isn't present.
    /// When `multiple`, always returns a list (the raw value wrapped in one if it wasn't already
    /// a list). Otherwise, a list value resolves to its *last* element (later entries win), and
    /// a scalar value resolves to itself.
    ///
    /// `pub` (beyond what `#[pymethods]` already exposes to Python) so other Rust modules that
    /// build on `Promise` — see `core/promisable.rs` — can call it directly.
    pub fn fulfill_with(&self, mapping: &Bound<'_, PyDict>) -> PyResult<Py<PyAny>> {
        let py = mapping.py();
        let value = mapping.get_item(&self.id)?;
        let Some(value) = value else {
            return Err(PyKeyError::new_err(self.id.clone()));
        };
        if value.is_none() {
            return Err(PyKeyError::new_err(self.id.clone()));
        }
        if self.multiple {
            if let Ok(list) = value.cast::<PyList>() {
                return Ok(list.clone().into_any().unbind());
            }
            return Ok(PyList::new(py, [value])?.into_any().unbind());
        }
        if let Ok(list) = value.cast::<PyList>() {
            let last = list.get_item(list.len().saturating_sub(1))?;
            return Ok(last.unbind());
        }
        Ok(value.unbind())
    }

    fn __str__(&self) -> &str {
        &self.id
    }

    fn __repr__(&self) -> String {
        let mut flags = String::new();
        if self.strict {
            flags.push('!');
        }
        if self.multiple {
            flags.push_str("[]");
        }
        format!("Promise(\"{}{}\")", self.id, flags)
    }

    fn __format__(&self, _fmt: &str) -> String {
        self.__repr__()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_id_has_no_flags() {
        let p = Promise::from_parts("ref", false, false);
        assert_eq!(p.id, "ref");
        assert!(!p.strict);
        assert!(!p.multiple);
    }

    #[test]
    fn trailing_bang_sets_strict_and_is_stripped() {
        let p = Promise::from_parts("ref!", false, false);
        assert_eq!(p.id, "ref");
        assert!(p.strict);
        assert!(!p.multiple);
    }

    #[test]
    fn trailing_brackets_set_multiple_and_are_stripped() {
        let p = Promise::from_parts("ref[]", false, false);
        assert_eq!(p.id, "ref");
        assert!(!p.strict);
        assert!(p.multiple);
    }

    #[test]
    fn explicit_strict_flag_leaves_trailing_bang_alone() {
        // Matches the Python `if not strict and raw.endswith("!")` guard: when strict is
        // already True via the keyword argument, a literal "!" in the id is NOT stripped.
        let p = Promise::from_parts("weird!", true, false);
        assert_eq!(p.id, "weird!");
        assert!(p.strict);
    }

    #[test]
    fn explicit_multiple_flag_leaves_trailing_brackets_alone() {
        let p = Promise::from_parts("weird[]", false, true);
        assert_eq!(p.id, "weird[]");
        assert!(p.multiple);
    }

    #[test]
    fn both_suffixes_only_combine_in_bracket_then_bang_order() {
        // Verified against the actual Python `Promise` before writing this: suffix stripping
        // checks "!" first (against the raw, unstripped id), then "[]" against whatever's left.
        // So "ref![]" does NOT end with "!" (it ends with "]"), meaning `strict` stays False and
        // only the "[]" gets stripped, leaving "ref!" as the id. Only "id[]!" (brackets before
        // the bang) makes both flags fire, because after the bang is stripped the remainder
        // ("ref[]") does end with "[]" too.
        let mismatched_order = Promise::from_parts("ref![]", false, false);
        assert_eq!(mismatched_order.id, "ref!");
        assert!(!mismatched_order.strict);
        assert!(mismatched_order.multiple);

        let both = Promise::from_parts("ref[]!", false, false);
        assert_eq!(both.id, "ref");
        assert!(both.strict);
        assert!(both.multiple);
    }

    #[test]
    fn equality_and_hash_depend_on_id_strict_and_multiple() {
        let a = Promise::from_parts("ref", false, false);
        let b = Promise::from_parts("ref", false, false);
        let c = Promise::from_parts("ref", true, false);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn repr_matches_python_format() {
        let p = Promise::from_parts("ref![]", false, false);
        assert_eq!(p.__repr__(), "Promise(\"ref![]\")");
        let plain = Promise::from_parts("ref", false, false);
        assert_eq!(plain.__repr__(), "Promise(\"ref\")");
    }
}
