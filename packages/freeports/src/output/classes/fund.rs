//! The [`Fund`] entity: a fund's name, normalised.
//!
//! The name that goes into the output is not the constructor's argument but its **deeply
//! normalised, upper-cased** form, while comparison and hashing use the normalised lower-case form.
//! The struct therefore stores the normalised form and [`Fund::name`] upper-cases it on read, which
//! gives the same observable behaviour without keeping two copies in sync.
//!
//! [`Fund::resolve_field`] normalises too. That matters: a name arriving through a resolved promise
//! takes a different route into the struct than one passed to the constructor, and skipping
//! normalisation there would leave a `Fund` that compares and hashes unlike every other.

use serde::{Deserialize, Serialize};

use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::normalization::deep_normalize_string;
use crate::core::promisable::{PromisableFields, Promised};
use crate::core::promise::Promise;

use super::{OutputClassError, pending_of, promised_from_value, serde_promised};

/// A fund, identified by its name alone.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fund {
    /// The deeply normalised, lower-case form of the name, or the promise that will produce it.
    ///
    /// Private, because the name one reads and writes is [`Fund::name`], which upper-cases it.
    ///
    /// **Renamed to `name` for serde** rather than letting the internal name leak: test fixtures
    /// derive their keys from the serde form, on the assumption — true of every other entity — that
    /// those match the constructor's arguments. Without the rename this was the one exception, and
    /// a regenerated fixture could not be read back. Re-normalising is idempotent, so rebuilding
    /// from an already normalised value yields the same fund.
    #[serde(with = "serde_promised", rename = "name")]
    n_name: Promised<String>,
}

impl Fund {
    /// Builds a fund from a name already known.
    pub fn new(name: &str) -> Self {
        Self { n_name: Promised::Resolved(deep_normalize_string(name)) }
    }

    /// Builds a fund from a [`BlockValue`], which may be a string or a promise.
    pub fn from_value(value: &BlockValue) -> Result<Self, OutputClassError> {
        let n_name = promised_from_value("name", value, |v| v.str_or_fail("name").map(deep_normalize_string))?;
        Ok(Self { n_name })
    }

    /// The fund's name: normalised and upper-cased. `None` while the name is an unresolved promise.
    pub fn name(&self) -> Option<String> {
        self.n_name.resolved().map(|n| n.to_uppercase())
    }

    /// The normalised lower-case form, the one funds are compared on.
    pub fn normalized_name(&self) -> Option<&str> {
        self.n_name.resolved().map(String::as_str)
    }

    /// The promise still to be resolved, if the name is pending.
    pub fn pending_name(&self) -> Option<&Promise> {
        self.n_name.pending()
    }
}

impl PromisableFields for Fund {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        pending_of("name", &self.n_name).into_iter().collect()
    }

    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        match field {
            // Always normalised, here too; see the module documentation.
            "name" => {
                self.n_name = Promised::Resolved(deep_normalize_string(value.str_or_fail("name")?));
                Ok(())
            }
            other => unreachable!("Fund has no promisable field {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promise::Promise;
    use crate::core::promise_resolution::FlatPromiseMap;
    use crate::core::promisable::{Fulfilled, fulfill_promises};

    mod construction {
        use super::*;

        #[test]
        fn normalizes_and_uppercases_the_name_it_exposes() {
            assert_eq!(Fund::new("  Alpha   Fund  ").name(), Some("ALPHA FUND".to_string()));
        }

        #[test]
        fn keeps_the_lowercase_normalized_form_for_comparisons() {
            assert_eq!(Fund::new("Alpha Fund").normalized_name(), Some("alpha fund"));
        }

        #[test]
        fn two_names_differing_only_in_case_and_spacing_are_the_same_fund() {
            assert_eq!(Fund::new("Alpha  Fund"), Fund::new("alpha fund"));
        }

        #[test]
        fn builds_from_a_string_block_value() {
            let fund = Fund::from_value(&BlockValue::from("Alpha Fund")).unwrap();
            assert_eq!(fund.name(), Some("ALPHA FUND".to_string()));
        }

        #[test]
        fn a_non_string_block_value_is_a_typed_field_error() {
            let err = Fund::from_value(&BlockValue::from(42i64)).unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "name", .. }));
        }

        #[test]
        fn a_null_block_value_is_rejected_rather_than_silently_accepted() {
            assert!(Fund::from_value(&BlockValue::Null).is_err());
        }
    }

    mod promises {
        use super::*;

        fn promised_fund() -> Fund {
            Fund::from_value(&BlockValue::Promise(Promise::new("fund-id"))).unwrap()
        }

        #[test]
        fn a_promise_stays_pending_instead_of_becoming_a_name() {
            let fund = promised_fund();
            assert_eq!(fund.name(), None);
            assert!(fund.pending_name().is_some());
        }

        #[test]
        fn a_pending_fund_reports_its_name_field_as_pending() {
            let pending = promised_fund().pending();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].0, "name");
        }

        #[test]
        fn a_resolved_fund_reports_nothing_pending() {
            assert!(Fund::new("Alpha").pending().is_empty());
        }

        #[test]
        fn resolving_a_promise_normalizes_the_name_exactly_like_construction() {
            // Resolving through a promise must normalise, exactly as the constructor does.
            let mut fund = promised_fund();
            fund.resolve_field("name", BlockValue::from("  Alpha   Fund ")).unwrap();
            assert_eq!(fund, Fund::new("Alpha Fund"));
            assert_eq!(fund.name(), Some("ALPHA FUND".to_string()));
        }

        #[test]
        fn resolving_with_a_non_string_value_is_an_error() {
            let mut fund = promised_fund();
            assert!(fund.resolve_field("name", BlockValue::from(1i64)).is_err());
        }

        #[test]
        fn fulfilling_against_a_map_produces_the_same_fund_as_direct_construction() {
            let mut fund = promised_fund();
            let map = FlatPromiseMap::from_pairs([("fund-id".to_string(), BlockValue::from("Alpha Fund"))]);
            assert_eq!(fulfill_promises(&mut fund, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(fund, Fund::new("Alpha Fund"));
        }
    }

    mod serde_roundtrip {
        use super::*;

        #[test]
        fn a_resolved_fund_survives_a_json_roundtrip() {
            let fund = Fund::new("Alpha Fund");
            let json = serde_json::to_string(&fund).unwrap();
            assert_eq!(serde_json::from_str::<Fund>(&json).unwrap(), fund);
        }

        #[test]
        fn the_serialized_form_is_the_normalized_name_not_the_promise_wrapper() {
            let json = serde_json::to_string(&Fund::new("Alpha Fund")).unwrap();
            assert!(json.contains("alpha fund"), "unexpected serialization: {json}");
        }
    }
}
