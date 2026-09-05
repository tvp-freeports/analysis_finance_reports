//! The entities the `deserialize` pipes produce: what ends up in the output files.
//!
//! # One error type for the whole module
//!
//! The field validations are the same for every entity, and duplicating the error per submodule
//! would force conversions back and forth between twin types that say the same things.
//!
//! # Numeric fields are ordered floats, not bare ones
//!
//! The variant that carries these entities through the engine derives equality, which a bare `f64`
//! makes impossible. The constructors take and the accessors return plain `f64`, so the internal
//! type is not visible from outside.
//!
//! # Every field can arrive as a promise
//!
//! A value a page cannot resolve on its own becomes a [`crate::core::promise::Promise`], and
//! deciding whether to resolve or drop it belongs to promise fulfilment, not to the constructor.
//! That is why nearly every field is a `Promised<T>` rather than a `T`.

pub mod assets_manager;
pub mod fund;
pub mod fund_assets;
pub mod fund_change_name;
pub mod fund_esg_indicator;
pub mod fund_sfdr_classification;
pub mod investment;

use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::promisable::Promised;
use crate::core::promise::Promise;

/// Failures of building an output entity.
///
/// Field validation is done by fallible constructors: an entity that exists is an entity whose
/// invariants hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutputClassError {
    /// A field had a type other than the expected one.
    #[error("field '{field}': {source}")]
    Field {
        field: &'static str,
        #[source]
        source: BlockValueError,
    },
    /// A numeric field is outside its admissible domain.
    #[error("field '{field}': {constraint}, got {value}")]
    OutOfRange { field: &'static str, constraint: FloatConstraint, value: String },
    /// The accounting equation of [`fund_assets::FundAssets`] does not balance, beyond a small
    /// tolerance.
    ///
    /// Not a [`FloatConstraint`], because it is not a constraint on one field but across three.
    #[error(
        "unbalanced fund assets: liabilities ({liabilities}) + net_assets ({net_assets}) != tot_assets ({tot_assets})"
    )]
    UnbalancedFundAssets {
        tot_assets: ordered_float::OrderedFloat<f64>,
        liabilities: ordered_float::OrderedFloat<f64>,
        net_assets: ordered_float::OrderedFloat<f64>,
    },
}

/// The numeric domains a field can be constrained to.
///
/// # The edges belong to the domain, and are worth saying out loud
///
/// A domain here is closed wherever a real report can land on its edge: a holding frozen and
/// written off is worth exactly zero, a fund can hold a single position worth its whole net
/// assets. Rejecting those would throw away the very positions this engine exists to surface.
///
/// Accepting them is not the same as passing them over in silence. A value sitting **exactly** on
/// an edge is rare enough to be worth finding again in the report, so [`Self::validate`] lets it
/// through and emits one `warn`. What stays rejected is what falls *outside*: a negative amount, a
/// share above the whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatConstraint {
    /// Strictly greater than zero.
    Positive,
    /// Greater than or equal to zero. Zero is admissible, and warned about.
    NonNegative,
    /// A **fraction**, not a percentage: `0.05` means five per cent. Both edges are admissible.
    UnitIntervalClosed,
    /// Like [`Self::UnitIntervalClosed`], but the whole is not admissible.
    UnitIntervalHalfOpen,
}

impl std::fmt::Display for FloatConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            FloatConstraint::Positive => "input should be greater than 0",
            FloatConstraint::NonNegative => "input should be greater than or equal to 0",
            FloatConstraint::UnitIntervalClosed => "input should be in the range [0.0, 1.0]",
            FloatConstraint::UnitIntervalHalfOpen => "input should be in the range [0.0, 1.0)",
        };
        f.write_str(text)
    }
}

impl FloatConstraint {
    /// Checks `value`, naming `field` in the error.
    ///
    /// A value on an admissible edge of the domain passes **and** is logged: see the type's
    /// documentation for why the two are not in tension.
    pub fn validate(self, field: &'static str, value: f64) -> Result<f64, OutputClassError> {
        let ok = match self {
            FloatConstraint::Positive => value > 0.0,
            FloatConstraint::NonNegative => value >= 0.0,
            FloatConstraint::UnitIntervalClosed => (0.0..=1.0).contains(&value),
            FloatConstraint::UnitIntervalHalfOpen => (0.0..1.0).contains(&value),
        };
        if !ok {
            return Err(OutputClassError::OutOfRange { field, constraint: self, value: value.to_string() });
        }
        if self.is_on_an_edge(value) {
            // `coord_ref_2` rather than a free field: it is the `.log.csv` column that says *which*
            // field a row is about, and the enclosing span already carries the page and the
            // company, which is what makes the value findable in the report.
            tracing::warn!(coord_ref_2 = field, "{value} sits on the edge of the admissible range - kept");
        }
        Ok(value)
    }

    /// Whether `value` is exactly on an edge the domain **includes**. An open bound has no such
    /// edge: nothing that passes is next to it.
    fn is_on_an_edge(self, value: f64) -> bool {
        match self {
            FloatConstraint::Positive => false,
            FloatConstraint::NonNegative => value == 0.0,
            FloatConstraint::UnitIntervalClosed => value == 0.0 || value == 1.0,
            FloatConstraint::UnitIntervalHalfOpen => value == 0.0,
        }
    }
}

/// Turns a [`BlockValue`] into a `Promised<T>` field: a promise stays pending, any other value is
/// converted at once.
///
/// This is where the general rule of the output entities is concentrated: **every** field may
/// arrive as a promise, and whether to resolve or drop it is decided later.
pub(crate) fn promised_from_value<T>(
    field: &'static str,
    value: &BlockValue,
    extract: impl FnOnce(&BlockValue) -> Result<T, BlockValueError>,
) -> Result<Promised<T>, OutputClassError> {
    match value {
        BlockValue::Promise(promise) => Ok(Promised::Pending(promise.clone())),
        other => extract(other).map(Promised::Resolved).map_err(|source| OutputClassError::Field { field, source }),
    }
}

/// Like [`promised_from_value`], but for an optional field: an absent or null value becomes `None`.
pub(crate) fn optional_promised_from_value<T>(
    field: &'static str,
    value: Option<&BlockValue>,
    extract: impl FnOnce(&BlockValue) -> Result<T, BlockValueError>,
) -> Result<Option<Promised<T>>, OutputClassError> {
    match value {
        None | Some(BlockValue::Null) => Ok(None),
        Some(value) => promised_from_value(field, value, extract).map(Some),
    }
}

/// The pending promise of a field, if there is one: a helper for the promisable implementations.
pub(crate) fn pending_of<T>(field: &'static str, value: &Promised<T>) -> Option<(&'static str, Promise)> {
    value.pending().map(|promise| (field, promise.clone()))
}
