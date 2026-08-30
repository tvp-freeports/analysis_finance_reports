//! `FundEsgIndicator`: un indicatore ESG di un fondo (`fund`, `name`, `value`).
//!
//! M8, passo 2 (`agent-memory/M8-implementation-plan.md` §3). Il più semplice delle cinque
//! entità mancanti: un solo campo promettibile (`fund`), nessun vincolo numerico — `name`/`value`
//! sono stringhe libere (es. `name = "GHG intensity"`, `value = "12.3"`), portate senza
//! interpretazione. Vedi `packages/freeports_core/src/output/classes/fund_esg_indicator.rs` per
//! il riferimento.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub struct FundEsgIndicator { pub fund: Promised<String>, pub name: String, pub value: String }
//! impl FundEsgIndicator {
//!     pub fn build(fund: &BlockValue, name: impl Into<String>, value: impl Into<String>)
//!         -> Result<Self, OutputClassError>;
//! }
//! impl PromisableFields for FundEsgIndicator { /* pending() -> ["fund"] se pendente */ }
//! ```
//!
//! Deriva `Debug, Clone, PartialEq, Eq, Serialize, Deserialize` (nessun campo `f64`, quindi `Eq`
//! non è un problema qui a differenza di `Equity`/`Bond`).

use serde::{Deserialize, Serialize};

use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::promisable::{PromisableFields, Promised};
use crate::core::promise::Promise;

use super::{OutputClassError, pending_of, promised_from_value, serde_promised};

/// Un indicatore ESG di un fondo: `name`/`value` sono stringhe libere, portate senza
/// interpretazione (es. `name = "GHG intensity"`, `value = "12.3"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundEsgIndicator {
    #[serde(with = "serde_promised")]
    pub fund: Promised<String>,
    pub name: String,
    pub value: String,
}

impl FundEsgIndicator {
    pub fn build(
        fund: &BlockValue,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, OutputClassError> {
        let fund = promised_from_value("fund", fund, |v| v.str_or_fail("fund").map(str::to_string))?;
        Ok(Self { fund, name: name.into(), value: value.into() })
    }
}

impl PromisableFields for FundEsgIndicator {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        pending_of("fund", &self.fund).into_iter().collect()
    }

    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        match field {
            "fund" => {
                self.fund = Promised::Resolved(value.str_or_fail("fund")?.to_string());
                Ok(())
            }
            other => unreachable!("FundEsgIndicator has no promisable field {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promise_resolution::FlatPromiseMap;
    use crate::core::promisable::{Fulfilled, fulfill_promises};

    mod construction {
        use super::*;

        #[test]
        fn builds_an_indicator_from_a_resolved_fund_name() {
            let indicator =
                FundEsgIndicator::build(&BlockValue::from("Alpha Fund"), "GHG intensity", "12.3").unwrap();
            assert_eq!(indicator.fund.resolved().map(String::as_str), Some("Alpha Fund"));
            assert_eq!(indicator.name, "GHG intensity");
            assert_eq!(indicator.value, "12.3");
        }

        #[test]
        fn arbitrary_indicator_and_value_strings_are_accepted_verbatim() {
            let indicator = FundEsgIndicator::build(&BlockValue::from("X"), "Any Indicator", "Any Value").unwrap();
            assert_eq!(indicator.name, "Any Indicator");
            assert_eq!(indicator.value, "Any Value");
        }

        #[test]
        fn a_wrongly_typed_fund_is_a_field_error_naming_the_field() {
            let err = FundEsgIndicator::build(&BlockValue::from(1i64), "n", "v").unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "fund", .. }));
        }

        #[test]
        fn a_null_fund_is_rejected_rather_than_silently_accepted() {
            assert!(FundEsgIndicator::build(&BlockValue::Null, "n", "v").is_err());
        }
    }

    mod promises {
        use super::*;
        use crate::core::promise::Promise as P;

        fn promised_indicator() -> FundEsgIndicator {
            FundEsgIndicator::build(&BlockValue::Promise(P::new("fund-id")), "n", "v").unwrap()
        }

        #[test]
        fn a_promise_stays_pending_instead_of_becoming_a_name() {
            let indicator = promised_indicator();
            assert!(indicator.fund.resolved().is_none());
            assert!(indicator.fund.pending().is_some());
        }

        #[test]
        fn a_pending_indicator_reports_its_fund_field_as_pending() {
            let pending = promised_indicator().pending();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].0, "fund");
        }

        #[test]
        fn a_resolved_indicator_reports_nothing_pending() {
            let indicator = FundEsgIndicator::build(&BlockValue::from("X"), "n", "v").unwrap();
            assert!(indicator.pending().is_empty());
        }

        #[test]
        fn resolving_the_fund_field_works_in_place() {
            let mut indicator = promised_indicator();
            indicator.resolve_field("fund", BlockValue::from("Resolved Fund")).unwrap();
            assert_eq!(indicator.fund.resolved().map(String::as_str), Some("Resolved Fund"));
        }

        #[test]
        fn resolving_with_a_non_string_value_is_an_error() {
            let mut indicator = promised_indicator();
            assert!(indicator.resolve_field("fund", BlockValue::from(1i64)).is_err());
        }

        #[test]
        fn fulfilling_against_a_map_resolves_the_fund_field() {
            let mut indicator = promised_indicator();
            let map = FlatPromiseMap::from_pairs([("fund-id".to_string(), BlockValue::from("Resolved Fund"))]);
            assert_eq!(fulfill_promises(&mut indicator, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(indicator.fund.resolved().map(String::as_str), Some("Resolved Fund"));
        }
    }

    mod serde_roundtrip {
        use super::*;

        #[test]
        fn a_resolved_indicator_survives_a_json_roundtrip() {
            let indicator = FundEsgIndicator::build(&BlockValue::from("X"), "n", "v").unwrap();
            let json = serde_json::to_string(&indicator).unwrap();
            assert_eq!(serde_json::from_str::<FundEsgIndicator>(&json).unwrap(), indicator);
        }
    }
}
