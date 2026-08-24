//! `FundAssets`: il patrimonio di un fondo a una certa data (totale attivo, passivo, netto).
//!
//! M8, passo 4 (`agent-memory/M8-implementation-plan.md` §3/§1). Vedi
//! `packages/freeports_core/src/output/classes/fund_assets.rs` per il riferimento: i tre campi
//! numerici (`tot_assets`, `liabilities`, `net_assets`) **non sono mai promettibili** — il
//! riferimento può quindi verificare l'equazione contabile una volta sola, a costruzione, e così
//! fa anche questo porting. Solo `date` (opzionale) e `currency` sono `Promised`.
//!
//! **Il vincolo contabile** (`liabilities + net_assets == tot_assets`, tolleranza `1e-4`) non è un
//! `FloatConstraint` (non è un vincolo su un singolo campo): è una variante nuova di
//! `OutputClassError`, `UnbalancedFundAssets { tot_assets, liabilities, net_assets }`
//! (`agent-memory/M8-implementation-plan.md` §1) — l'implementer deve aggiungerla a
//! `output::classes::OutputClassError` (`src/output/classes.rs`), che il test-writer non tocca.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub struct FundAssets {
//!     pub fund: String,
//!     pub date: Option<Promised<Date>>,
//!     pub tot_assets: OrderedFloat<f64>,
//!     pub liabilities: OrderedFloat<f64>,
//!     pub net_assets: OrderedFloat<f64>,
//!     pub currency: Promised<Currency>,
//! }
//! impl FundAssets {
//!     pub fn build(
//!         fund: impl Into<String>,
//!         tot_assets: f64,
//!         liabilities: f64,
//!         net_assets: f64,
//!         currency: &BlockValue,
//!         date: Option<&BlockValue>,
//!     ) -> Result<Self, OutputClassError>;
//! }
//! impl PromisableFields for FundAssets { /* pending() -> ["date"?, "currency"?] in quest'ordine */ }
//! ```
//!
//! Deriva `Debug, Clone, PartialEq, Serialize, Deserialize` (niente `Eq`: nessuno dei chiamanti
//! attuali ne ha bisogno per `FundAssets` in particolare, ma non è nemmeno vietato — a differenza
//! di `Equity`/`Bond` non è imposto da `Extracted::eq`, che qui usa comunque `PartialEq` derivato
//! su tutta l'enum: se `Extracted` richiede `Eq` anche per questa variante, l'implementer lo
//! deriva, i test qui sotto non dipendono dalla presenza di `Eq`).

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use crate::commons::consts::Currency;
use crate::commons::date::Date;
use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::promisable::{PromisableFields, Promised};
use crate::core::promise::Promise;

use super::{FloatConstraint, OutputClassError, optional_promised_from_value, pending_of, promised_from_value};

/// La tolleranza dell'equazione contabile `liabilities + net_assets == tot_assets`, come nel
/// riferimento.
const BALANCE_TOLERANCE: f64 = 1e-4;

/// Il patrimonio di un fondo a una certa data. I tre importi non sono mai promettibili — il
/// vincolo contabile va verificato una volta sola, a costruzione — solo `date` e `currency` lo
/// sono.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundAssets {
    pub fund: String,
    #[serde(with = "super::serde_optional_promised")]
    pub date: Option<Promised<Date>>,
    pub tot_assets: OrderedFloat<f64>,
    pub liabilities: OrderedFloat<f64>,
    pub net_assets: OrderedFloat<f64>,
    #[serde(with = "super::serde_promised")]
    pub currency: Promised<Currency>,
}

impl FundAssets {
    pub fn build(
        fund: impl Into<String>,
        tot_assets: f64,
        liabilities: f64,
        net_assets: f64,
        currency: &BlockValue,
        date: Option<&BlockValue>,
    ) -> Result<Self, OutputClassError> {
        let tot_assets = FloatConstraint::NonNegative.validate("tot_assets", tot_assets)?;
        let liabilities = FloatConstraint::NonNegative.validate("liabilities", liabilities)?;
        let net_assets = FloatConstraint::NonNegative.validate("net_assets", net_assets)?;

        if (liabilities + net_assets - tot_assets).abs() > BALANCE_TOLERANCE {
            return Err(OutputClassError::UnbalancedFundAssets {
                tot_assets: OrderedFloat(tot_assets),
                liabilities: OrderedFloat(liabilities),
                net_assets: OrderedFloat(net_assets),
            });
        }

        let currency = promised_from_value("currency", currency, |v| v.currency_or_fail("currency"))?;
        let date = optional_promised_from_value("date", date, |v| v.date_or_fail("date"))?;

        Ok(Self {
            fund: fund.into(),
            date,
            tot_assets: OrderedFloat(tot_assets),
            liabilities: OrderedFloat(liabilities),
            net_assets: OrderedFloat(net_assets),
            currency,
        })
    }
}

impl PromisableFields for FundAssets {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        let mut out = Vec::new();
        if let Some(date) = &self.date {
            out.extend(pending_of("date", date));
        }
        out.extend(pending_of("currency", &self.currency));
        out
    }

    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        match field {
            "date" => {
                self.date = Some(Promised::Resolved(value.date_or_fail("date")?));
                Ok(())
            }
            "currency" => {
                self.currency = Promised::Resolved(value.currency_or_fail("currency")?);
                Ok(())
            }
            other => unreachable!("FundAssets has no promisable field {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promise::Promise as P;
    use crate::core::promise_resolution::FlatPromiseMap;
    use crate::core::promisable::{Fulfilled, fulfill_promises};

    fn eur() -> BlockValue {
        BlockValue::from(Currency::EUR)
    }

    mod construction {
        use super::*;

        #[test]
        fn builds_from_a_balanced_equation() {
            let assets = FundAssets::build("Alpha Fund", 100.0, 40.0, 60.0, &eur(), None).unwrap();
            assert_eq!(assets.fund, "Alpha Fund");
            assert_eq!(assets.tot_assets.into_inner(), 100.0);
            assert_eq!(assets.liabilities.into_inner(), 40.0);
            assert_eq!(assets.net_assets.into_inner(), 60.0);
            assert_eq!(assets.currency.resolved(), Some(&Currency::EUR));
        }

        #[test]
        fn date_defaults_to_none_when_not_given() {
            let assets = FundAssets::build("X", 100.0, 40.0, 60.0, &eur(), None).unwrap();
            assert!(assets.date.is_none());
        }

        #[test]
        fn a_null_date_is_the_same_as_no_date() {
            let assets = FundAssets::build("X", 100.0, 40.0, 60.0, &eur(), Some(&BlockValue::Null)).unwrap();
            assert!(assets.date.is_none());
        }

        #[test]
        fn a_resolved_date_is_carried_through() {
            let date = Date::new(2024, 12, 31).unwrap();
            let assets =
                FundAssets::build("X", 100.0, 40.0, 60.0, &eur(), Some(&BlockValue::from(date))).unwrap();
            assert_eq!(assets.date.and_then(|d| d.resolved().copied()), Some(date));
        }

        #[test]
        fn rejects_unbalanced_equation() {
            let err = FundAssets::build("X", 100.0, 40.0, 61.0, &eur(), None).unwrap_err();
            assert!(matches!(err, OutputClassError::UnbalancedFundAssets { .. }), "{err:?}");
        }

        #[test]
        fn tolerates_a_small_float_error_within_the_1e_minus_4_tolerance() {
            assert!(FundAssets::build("X", 100.0, 40.00005, 60.0, &eur(), None).is_ok());
        }

        #[test]
        fn rejects_a_difference_clearly_outside_the_tolerance() {
            let err = FundAssets::build("X", 100.0, 40.001, 60.0, &eur(), None).unwrap_err();
            assert!(matches!(err, OutputClassError::UnbalancedFundAssets { .. }));
        }

        #[test]
        fn all_three_amounts_zero_is_a_balanced_equation() {
            assert!(FundAssets::build("X", 0.0, 0.0, 0.0, &eur(), None).is_ok());
        }

        #[test]
        fn each_amount_accepts_zero_individually() {
            assert!(FundAssets::build("X", 0.0, 0.0, 0.0, &eur(), None).is_ok());
        }

        #[test]
        fn a_negative_tot_assets_is_rejected_before_the_equation_check() {
            let err = FundAssets::build("X", -1.0, 40.0, 60.0, &eur(), None).unwrap_err();
            assert!(matches!(
                err,
                OutputClassError::OutOfRange { field: "tot_assets", constraint: FloatConstraint::NonNegative, .. }
            ));
        }

        #[test]
        fn a_negative_liabilities_is_rejected() {
            let err = FundAssets::build("X", 100.0, -40.0, 60.0, &eur(), None).unwrap_err();
            assert!(matches!(
                err,
                OutputClassError::OutOfRange { field: "liabilities", constraint: FloatConstraint::NonNegative, .. }
            ));
        }

        #[test]
        fn a_negative_net_assets_is_rejected() {
            let err = FundAssets::build("X", 100.0, 40.0, -60.0, &eur(), None).unwrap_err();
            assert!(matches!(
                err,
                OutputClassError::OutOfRange { field: "net_assets", constraint: FloatConstraint::NonNegative, .. }
            ));
        }

        #[test]
        fn a_wrongly_typed_currency_is_a_field_error_naming_the_field() {
            let err = FundAssets::build("X", 100.0, 40.0, 60.0, &BlockValue::from("EUR"), None).unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "currency", .. }));
        }

        #[test]
        fn a_wrongly_typed_date_is_a_field_error_naming_the_field() {
            let err =
                FundAssets::build("X", 100.0, 40.0, 60.0, &eur(), Some(&BlockValue::from(1i64))).unwrap_err();
            assert!(matches!(err, OutputClassError::Field { field: "date", .. }));
        }
    }

    mod promises {
        use super::*;

        fn promised_assets() -> FundAssets {
            FundAssets::build(
                "X",
                100.0,
                40.0,
                60.0,
                &BlockValue::Promise(P::new("cur-id")),
                Some(&BlockValue::Promise(P::new("date-id"))),
            )
            .unwrap()
        }

        #[test]
        fn reports_every_pending_field_in_declaration_order() {
            let pending: Vec<_> = promised_assets().pending().into_iter().map(|(f, _)| f).collect();
            assert_eq!(pending, vec!["date", "currency"]);
        }

        #[test]
        fn a_fully_resolved_fund_assets_reports_nothing_pending() {
            assert!(FundAssets::build("X", 100.0, 40.0, 60.0, &eur(), None).unwrap().pending().is_empty());
        }

        #[test]
        fn resolving_currency_and_date_works_in_place() {
            let mut assets = promised_assets();
            let map = FlatPromiseMap::from_iter([
                ("cur-id".to_string(), BlockValue::from(Currency::USD)),
                ("date-id".to_string(), BlockValue::from(Date::new(2024, 1, 1).unwrap())),
            ]);
            assert_eq!(fulfill_promises(&mut assets, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(assets.currency.resolved(), Some(&Currency::USD));
            assert_eq!(assets.date.and_then(|d| d.resolved().copied()), Some(Date::new(2024, 1, 1).unwrap()));
        }

        #[test]
        fn resolving_with_a_wrongly_typed_value_reports_the_field() {
            let mut assets = promised_assets();
            let err = assets.resolve_field("currency", BlockValue::from(1i64)).unwrap_err();
            assert!(err.to_string().contains("currency"), "{err}");
        }

        #[test]
        fn a_non_strict_unresolvable_promise_drops_the_entity() {
            let mut assets = promised_assets();
            assert_eq!(fulfill_promises(&mut assets, &FlatPromiseMap::new()).unwrap(), Fulfilled::Dropped);
        }
    }

    mod serde_roundtrip {
        use super::*;

        #[test]
        fn a_resolved_fund_assets_survives_a_json_roundtrip() {
            let assets = FundAssets::build(
                "X",
                100.0,
                40.0,
                60.0,
                &eur(),
                Some(&BlockValue::from(Date::new(2024, 1, 1).unwrap())),
            )
            .unwrap();
            let json = serde_json::to_string(&assets).unwrap();
            assert_eq!(serde_json::from_str::<FundAssets>(&json).unwrap(), assets);
        }

        /// Un campo opzionale promettibile assente deve sopravvivere come `None`, non come
        /// `Some(Promised::Resolved(_))` di un valore fittizio.
        #[test]
        fn an_absent_optional_date_survives_as_none_not_as_a_fake_resolved_value() {
            let assets = FundAssets::build("X", 100.0, 40.0, 60.0, &eur(), None).unwrap();
            let json = serde_json::to_string(&assets).unwrap();
            let back: FundAssets = serde_json::from_str(&json).unwrap();
            assert!(back.date.is_none());
        }
    }
}
