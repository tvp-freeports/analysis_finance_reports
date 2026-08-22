//! Fund-name matching primitive: stores a name alongside its normalized form so that
//! funds referenced under slightly different spellings/casing/accents can still be
//! recognized as the same fund.
//!
//! Rust port of `packages/freeports_core/src/freeports/_internals/core/match.py::MatchFund`.
//! Unlike the normalization functions, this is exposed to Python as a real pyclass — see
//! `_internals/core/match.py` for the thin Python bridge that wraps it. That bridge (not
//! this pyclass directly) is what `output/classes_schema.py::Fund` mixes in, because a
//! PyO3 pyclass cannot serve as a mixin base alongside Pydantic's `BaseModel` — the bridge
//! is a deliberately temporary interop shim, to be deleted once `Fund` itself no longer
//! needs a Python-side mixin (see `analysis_finance_reports/agent-memory/rust-rewrite-plan.md`).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pyo3::prelude::*;
use pyo3::pyclass::CompareOp;

use super::normalization;

/// A fund name paired with its deep-normalized form, used for consistent matching.
#[pyclass]
pub struct MatchFund {
    name: String,
    n_name: String,
}

impl MatchFund {
    pub fn new(name: String) -> Self {
        let n_name = normalization::deep_normalize_string(&name);
        MatchFund { name, n_name }
    }
}

#[pymethods]
impl MatchFund {
    #[new]
    fn py_new(name: String) -> Self {
        MatchFund::new(name)
    }

    /// The original, un-normalized fund name.
    ///
    /// `pub` (unlike the other `#[pymethods]` here) so `formats_utils/text_filter/
    /// standard_txt_blks.rs` can read it directly off a `Py<MatchFund>` without a Python
    /// `getattr` round-trip — see that module's own doc comment ("zero Python touched inside
    /// these functions beyond the `Py<PdfBlock>`/`&MatchFund` arguments they receive").
    #[getter]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The deep-normalized fund name used for hashing and equality.
    #[getter]
    fn n_name(&self) -> &str {
        &self.n_name
    }

    fn __str__(&self) -> &str {
        &self.n_name
    }

    fn __repr__(&self) -> String {
        format!("MatchFund(\"{}\")", self.name)
    }

    fn __hash__(&self) -> isize {
        let mut hasher = DefaultHasher::new();
        self.n_name.hash(&mut hasher);
        hasher.finish() as isize
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
        match op {
            CompareOp::Eq => Ok(self.n_name == other.n_name),
            CompareOp::Ne => Ok(self.n_name != other.n_name),
            _ => Err(pyo3::exceptions::PyTypeError::new_err(
                "MatchFund only supports equality comparisons",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_original_name_unchanged() {
        let m = MatchFund::new("Café Fund".to_string());
        assert_eq!(m.name, "Café Fund");
    }

    #[test]
    fn normalizes_name_for_matching() {
        let m = MatchFund::new("Café Fund".to_string());
        assert_eq!(m.n_name, "cafe fund");
    }

    #[test]
    fn equal_when_normalized_names_match() {
        let a = MatchFund::new("Café Fund".to_string());
        let b = MatchFund::new("CAFE   FUND".to_string());
        assert_eq!(a.n_name, b.n_name);
    }

    #[test]
    fn different_when_normalized_names_differ() {
        let a = MatchFund::new("Fund A".to_string());
        let b = MatchFund::new("Fund B".to_string());
        assert_ne!(a.n_name, b.n_name);
    }

    #[test]
    fn hash_is_consistent_for_equal_normalized_names() {
        let a = MatchFund::new("Café Fund".to_string());
        let b = MatchFund::new("CAFE   FUND".to_string());
        let hash_of = |m: &MatchFund| {
            let mut hasher = DefaultHasher::new();
            m.n_name.hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash_of(&a), hash_of(&b));
    }
}
