//! Shared infrastructure for output classes that mix in Python's `PromisableDict`
//! (`packages/freeports_core/src/freeports/_internals/core/promises.py`): a field can hold
//! either a resolved value or a pending [`Promise`], and `fulfill_promises(mapping)` resolves
//! every pending field against a `{promise_id: value}` map — either in place (single-valued
//! promises) or by expanding the entity into one clone per value (fields flagged
//! `multiple=True`).
//!
//! [`Promised<T>`] is the per-field wrapper; [`PromisableFields`] + [`fulfill_promises`]
//! reproduce the two-phase algorithm from `PromisableDict.fulfill_promises` exactly: non-multiple
//! fields resolve in place first; only afterward are any `multiple=True` fields expanded, one
//! clone per resolved value, in encounter order. This is one of the "architectural fixed points"
//! of the migration (see `agent-memory/rust-rewrite-plan.md`) — its return contract must match
//! the Python original exactly: `None` = resolved in place (caller keeps using the same entity),
//! `Some(vec![])` = drop the entity (non-strict promise, nothing to resolve with), `Some(clones)`
//! = multi-valued expansion.

use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFrozenSet, PyList};

use crate::commons::consts::Currency;
use super::promise::Promise;

/// A field that is either already resolved to `T`, or still a pending [`Promise`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Promised<T> {
    Resolved(T),
    Pending(Promise),
}

/// Extracts a `Promised<T>` out of a raw Python argument. Not a `FromPyObject` impl — PyO3
/// 0.27's `FromPyObject` trait takes two lifetime parameters tied to the borrow *and* the GIL
/// token in a way that's awkward to thread through a generic wrapper type, and every call site
/// here already receives a `&Bound<'_, PyAny>` directly (constructor arguments), so a plain
/// function is simpler — matches the established pattern in `cast.rs`'s `py_to_currency`, which
/// takes `&Bound<'_, PyAny>` and dispatches manually rather than implementing a trait.
///
/// A `Promise` instance is recognized first (matching the Python union's declaration order,
/// `Union[Promise, T]`, and `try_convert_to_currency`'s explicit `isinstance(value, Promise)`
/// check) — only if that fails is the value parsed as `T`.
pub fn extract_promised<'a, 'py, T>(ob: &'a Bound<'py, PyAny>) -> PyResult<Promised<T>>
where
    T: FromPyObject<'a, 'py>,
    T::Error: Into<PyErr>,
{
    if let Ok(p) = ob.extract::<Promise>() {
        return Ok(Promised::Pending(p));
    }
    ob.extract::<T>().map(Promised::Resolved).map_err(Into::into)
}

/// `PromisedCurrency`'s extraction (`promises.py::try_convert_to_currency`): a `Promise` is
/// recognized first, otherwise the value is parsed via `Currency(value)`'s own semantics (pass
/// through an existing `Currency`, else parse a string) — `Currency`'s derived `FromPyObject`
/// only *downcasts* (checks the argument already IS a `Currency`), it does not parse a raw
/// string, so the generic [`extract_promised`] isn't enough for this field type; this mirrors
/// `cast.rs::py_to_currency`'s same isinstance-first-else-parse dance.
pub fn extract_promised_currency(ob: &Bound<'_, PyAny>) -> PyResult<Promised<Currency>> {
    if let Ok(p) = ob.extract::<Promise>() {
        return Ok(Promised::Pending(p));
    }
    Currency::new(ob).map(Promised::Resolved)
}

/// A float constraint mirroring the handful of Pydantic constrained-float types used across
/// `output/classes_schema.py`/`core/promises.py` (`PositiveFloat`, `NonNegativeFloat`,
/// `confloat(ge=0.0, lt=1.0)` for fractional percentages/rates). One small enum instead of a
/// newtype-per-constraint: the validation is a one-line range check, not worth a distinct Rust
/// type for each of the three cases.
#[derive(Debug, Clone, Copy)]
pub enum FloatConstraint {
    /// Pydantic `PositiveFloat`: strictly greater than 0.
    Positive,
    /// Pydantic `NonNegativeFloat`: greater than or equal to 0.
    NonNegative,
    /// Pydantic `confloat(ge=0.0, lt=1.0)`: used for `perc_net_assets`/`interest_rate` — a
    /// fraction, not a percentage (`0.05` means 5%).
    UnitIntervalHalfOpen,
}

impl FloatConstraint {
    fn validate(self, v: f64) -> PyResult<f64> {
        let ok = match self {
            FloatConstraint::Positive => v > 0.0,
            FloatConstraint::NonNegative => v >= 0.0,
            FloatConstraint::UnitIntervalHalfOpen => (0.0..1.0).contains(&v),
        };
        if ok {
            Ok(v)
        } else {
            Err(PyValueError::new_err(match self {
                FloatConstraint::Positive => format!("Input should be greater than 0, got {v}"),
                FloatConstraint::NonNegative => {
                    format!("Input should be greater than or equal to 0, got {v}")
                }
                FloatConstraint::UnitIntervalHalfOpen => {
                    format!("Input should be in the range [0.0, 1.0), got {v}")
                }
            }))
        }
    }
}

/// Extracts a plain (non-`Promised`) constrained `f64` — used for fields like
/// `FundAssets.tot_assets` (`NonNegativeFloat`, never a `Promise` in the Python original).
pub fn extract_f64(ob: &Bound<'_, PyAny>, constraint: FloatConstraint) -> PyResult<f64> {
    let v: f64 = ob.extract()?;
    constraint.validate(v)
}

/// Extracts a `Promised<f64>` under `constraint` — used for fields like `Investment.market_value`
/// (`PromisedMarketValue = Union[Promise, PositiveFloat]`). The constraint only applies once a
/// concrete value is present; a pending `Promise` is recognized first, same as every other
/// `Promised<T>` field.
pub fn extract_promised_f64(ob: &Bound<'_, PyAny>, constraint: FloatConstraint) -> PyResult<Promised<f64>> {
    if let Ok(p) = ob.extract::<Promise>() {
        return Ok(Promised::Pending(p));
    }
    extract_f64(ob, constraint).map(Promised::Resolved)
}

/// Reproduces `Investment.__hash__`'s `hash(frozenset(self.model_dump(mode="json",
/// by_alias=True).items()))` exactly — not just an order-independent approximation of it — by
/// building a real Python `frozenset` out of the already-computed `model_dump` dict's items and
/// asking Python to hash it. Cheaper and more obviously correct than reimplementing CPython's
/// frozenset hash algorithm in Rust.
pub fn hash_via_model_dump_items(py: Python<'_>, dumped: &Bound<'_, PyDict>) -> PyResult<isize> {
    let items: Vec<(Bound<'_, PyAny>, Bound<'_, PyAny>)> = dumped.iter().collect();
    PyFrozenSet::new(py, items)?.hash()
}

impl<'py, T: IntoPyObject<'py>> IntoPyObject<'py> for Promised<T> {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        use pyo3::conversion::IntoPyObjectExt;
        match self {
            Promised::Resolved(v) => v.into_bound_py_any(py),
            Promised::Pending(p) => p.into_bound_py_any(py),
        }
    }
}

fn is_key_error(py: Python<'_>, err: &PyErr) -> bool {
    err.is_instance_of::<PyKeyError>(py)
}

/// Implemented by the (non-pyclass) inner data struct of every promise-bearing output class.
/// `pending_promises` must report every field currently holding `Promised::Pending`;
/// `resolve_field` must set a *named* field (one previously reported as pending) to the parsed
/// form of `value` — a raw value straight out of [`Promise::fulfill_with`], not yet the field's
/// resolved type.
pub trait PromisableFields: Clone {
    fn pending_promises(&self) -> Vec<(&'static str, Promise)>;
    fn resolve_field(&mut self, py: Python<'_>, field: &'static str, value: Py<PyAny>) -> PyResult<()>;
}

/// Port of `PromisableDict.fulfill_promises`. `entity` is mutated in place for every
/// non-`multiple` pending field; `Ok(None)` means "resolved in place, keep using `entity`". A
/// `multiple` field instead produces one clone of `entity` per resolved value (cross product
/// across every `multiple` field, in the order they were encountered) — `Ok(Some(vec![...]))`,
/// where an empty vec means "drop this entity" (non-strict promise, unresolved).
pub fn fulfill_promises<T: PromisableFields>(
    entity: &mut T,
    py: Python<'_>,
    mapping: &Bound<'_, PyDict>,
) -> PyResult<Option<Vec<T>>> {
    let mut multi_fields = Vec::new();
    for (name, promise) in entity.pending_promises() {
        if promise.multiple() {
            multi_fields.push((name, promise));
            continue;
        }
        match promise.fulfill_with(mapping) {
            Ok(value) => entity.resolve_field(py, name, value)?,
            Err(e) if is_key_error(py, &e) => {
                if promise.strict() {
                    return Err(e);
                }
                return Ok(Some(Vec::new()));
            }
            Err(e) => return Err(e),
        }
    }

    if multi_fields.is_empty() {
        return Ok(None);
    }

    let mut expansions = vec![entity.clone()];
    for (name, promise) in multi_fields {
        let values = match promise.fulfill_with(mapping) {
            Ok(v) => v,
            Err(e) if is_key_error(py, &e) => {
                if promise.strict() {
                    return Err(e);
                }
                return Ok(Some(Vec::new()));
            }
            Err(e) => return Err(e),
        };
        // `Promise::fulfill_with` always returns a `list` when `multiple` is set.
        let list = values.bind(py).cast::<PyList>()?.clone();
        if list.is_empty() {
            continue;
        }
        let mut next = Vec::with_capacity(expansions.len() * list.len());
        for base in &expansions {
            for item in list.iter() {
                let mut clone = base.clone();
                clone.resolve_field(py, name, item.unbind())?;
                next.push(clone);
            }
        }
        expansions = next;
    }
    Ok(Some(expansions))
}
