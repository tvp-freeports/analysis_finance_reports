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

/// Which end of a closed domain a value landed on. See [`FloatConstraint`] for why the two are
/// reported at different levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    /// Zero, for every domain that admits it.
    Lower,
    /// The whole, for the domains bounded above.
    Upper,
}

/// The numeric domains a field can be constrained to.
///
/// # The edges belong to the domain, and the two of them are not worth the same
///
/// A domain here is closed wherever a real report can land on its edge: a holding frozen and
/// written off is worth exactly zero, a fund can hold a single position worth its whole net
/// assets. Rejecting those would throw away the very positions this engine exists to surface.
/// What stays rejected is what falls *outside*: a negative amount, a share above the whole.
///
/// Accepting an edge is not the same as passing it over in silence, but the **upper** edge and the
/// **lower** edge say different things, so [`Self::validate`] reports them at different levels.
///
/// The lower edge is zero, and zero is ordinary: a zero-coupon bond has an `interest_rate` of
/// exactly zero, a position that rounds to 0,00% of net assets has a `perc_net_assets` of exactly
/// zero, a security received at no cost has an `acquisition_cost` of exactly zero. Measured over a
/// 903-report run it was **every single one** of the 121 edge events, so it goes to `debug`.
///
/// The upper edge is the whole: a `perc_net_assets` of exactly `1.0` is a fund with its entire net
/// assets in one position. That happens, which is why it is admissible — and it is also the shape a
/// column read one cell off produces, so it is worth finding again in the report. It stays a
/// `warn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatConstraint {
    /// Strictly greater than zero.
    Positive,
    /// Greater than or equal to zero. Zero is admissible, and reported at `debug`.
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
    /// A value on an admissible edge of the domain passes **and** is logged — at `warn` on the
    /// upper edge and at `debug` on the lower one. See the type's documentation for why the two
    /// edges do not deserve the same level.
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
        // `coord_ref_2` rather than a free field: it is the `.log.csv` column that says *which*
        // field a row is about, and the enclosing span already carries the page and the company,
        // which is what makes the value findable in the report.
        match self.edge_of(value) {
            Some(Edge::Upper) => {
                tracing::warn!(coord_ref_2 = field, "{value} sits on the upper edge of the admissible range - kept")
            }
            Some(Edge::Lower) => {
                tracing::debug!(coord_ref_2 = field, "{value} sits on the lower edge of the admissible range - kept")
            }
            None => {}
        }
        Ok(value)
    }

    /// Which edge `value` sits exactly on, if any. An open bound has no such edge: nothing that
    /// passes is next to it.
    fn edge_of(self, value: f64) -> Option<Edge> {
        match self {
            FloatConstraint::Positive => None,
            FloatConstraint::NonNegative | FloatConstraint::UnitIntervalHalfOpen if value == 0.0 => Some(Edge::Lower),
            FloatConstraint::NonNegative | FloatConstraint::UnitIntervalHalfOpen => None,
            FloatConstraint::UnitIntervalClosed if value == 0.0 => Some(Edge::Lower),
            FloatConstraint::UnitIntervalClosed if value == 1.0 => Some(Edge::Upper),
            FloatConstraint::UnitIntervalClosed => None,
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
