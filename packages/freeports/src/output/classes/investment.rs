//! The [`Equity`] and [`Bond`] entities: a fund's position in a target company.
//!
//! The two structs share [`InvestmentData`] — nine common fields and all the promise handling — and
//! a bond adds its own two, a maturity and an interest rate.
//!
//! They are two structs rather than two variants of one enum because everything downstream tells
//! them apart by type, never by matching: they end up in two different output files.

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use crate::commons::consts::Currency;
use crate::commons::date::Date;
use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::promisable::{PromisableFields, Promised};
use crate::core::promise::Promise;

use super::{
    FloatConstraint, OutputClassError, optional_promised_from_value, pending_of, promised_from_value,
};

/// The fields an equity and a bond have in common.
///
/// Nearly every field can arrive as a promise: it is the mechanism by which a value discovered on a
/// different page — typically the fund name or the currency — is filled in afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestmentData {
    pub company: String,
    pub company_match: String,
    pub fund: Promised<String>,
    pub nominal_quantity: Option<OrderedFloat<f64>>,
    pub market_value: Promised<OrderedFloat<f64>>,
    pub currency: Promised<Currency>,
    pub perc_net_assets: Option<Promised<OrderedFloat<f64>>>,
    pub acquisition_cost: Option<Promised<OrderedFloat<f64>>>,
    pub acquisition_currency: Option<Promised<Currency>>,
}

/// The raw values an investment is built from, as they arrive in a text block's metadata. Every
/// field is a [`BlockValue`], so every one of them may be a promise.
#[derive(Debug, Clone)]
pub struct InvestmentFields {
    pub company: String,
    pub company_match: String,
    pub fund: BlockValue,
    pub nominal_quantity: Option<f64>,
    pub market_value: BlockValue,
    pub currency: BlockValue,
    pub perc_net_assets: Option<BlockValue>,
    pub acquisition_cost: Option<BlockValue>,
    pub acquisition_currency: Option<BlockValue>,
}

impl InvestmentFields {
    /// The fields that must always be present; the rest are optional and start out absent.
    pub fn new(
        company: impl Into<String>,
        company_match: impl Into<String>,
        fund: BlockValue,
        market_value: BlockValue,
        currency: BlockValue,
    ) -> Self {
        Self {
            company: company.into(),
            company_match: company_match.into(),
            fund,
            nominal_quantity: None,
            market_value,
            currency,
            perc_net_assets: None,
            acquisition_cost: None,
            acquisition_currency: None,
        }
    }
}

/// Extracts a number from a [`BlockValue`], accepting an integer or a float indifferently, since
/// the casts produce one or the other depending on how the document writes it.
///
/// Only the **type** is checked here. The domain is checked by [`InvestmentData::validate_ranges`],
/// because a value outside its domain is not a type error, and because a field still promised
/// cannot be validated until the promise resolves.
fn resolved_float(field: &'static str, value: &BlockValue) -> Result<OrderedFloat<f64>, BlockValueError> {
    match value {
        BlockValue::Int(i) => Ok(OrderedFloat(*i as f64)),
        other => other.float_or_fail(field).map(OrderedFloat),
    }
}

impl InvestmentData {
    /// Builds the common fields, validating the numeric domains of the values already resolved.
    pub fn build(fields: InvestmentFields) -> Result<Self, OutputClassError> {
        let InvestmentFields {
            company,
            company_match,
            fund,
            nominal_quantity,
            market_value,
            currency,
            perc_net_assets,
            acquisition_cost,
            acquisition_currency,
        } = fields;

        let data = Self {
            company,
            company_match,
            fund: promised_from_value("fund", &fund, |v| v.str_or_fail("fund").map(str::to_string))?,
            nominal_quantity: nominal_quantity
                .map(|v| FloatConstraint::NonNegative.validate("nominal_quantity", v).map(OrderedFloat))
                .transpose()?,
            market_value: promised_from_value(
                "market_value",
                &market_value,
                |v| resolved_float("market_value", v),
            )?,
            currency: promised_from_value("currency", &currency, |v| v.currency_or_fail("currency"))?,
            perc_net_assets: optional_promised_from_value(
                "perc_net_assets",
                perc_net_assets.as_ref(),
                |v| resolved_float("perc_net_assets", v),
            )?,
            acquisition_cost: optional_promised_from_value(
                "acquisition_cost",
                acquisition_cost.as_ref(),
                |v| resolved_float("acquisition_cost", v),
            )?,
            acquisition_currency: optional_promised_from_value("acquisition_currency", acquisition_currency.as_ref(), |v| {
                v.currency_or_fail("acquisition_currency")
            })?,
        };
        data.validate_ranges()?;
        Ok(data)
    }

    /// Checks the numeric domains of the already-resolved fields only.
    ///
    /// Every amount here admits its edges, and says so with a warning rather than a refusal: a
    /// holding frozen and written off is printed at `0,00`, a bond can be carried at no
    /// acquisition cost, and a fund can hold one position worth its entire net assets. Those are
    /// the positions worth finding, not the ones worth dropping. Outside the edges — a negative
    /// amount, a share above the whole — it is still an error.
    fn validate_ranges(&self) -> Result<(), OutputClassError> {
        if let Some(v) = self.market_value.resolved() {
            FloatConstraint::NonNegative.validate("market_value", v.into_inner())?;
        }
        if let Some(Some(v)) = self.perc_net_assets.as_ref().map(Promised::resolved) {
            FloatConstraint::UnitIntervalClosed.validate("perc_net_assets", v.into_inner())?;
        }
        if let Some(Some(v)) = self.acquisition_cost.as_ref().map(Promised::resolved) {
            FloatConstraint::NonNegative.validate("acquisition_cost", v.into_inner())?;
        }
        Ok(())
    }

    fn pending_fields(&self) -> Vec<(&'static str, Promise)> {
        let mut out = Vec::new();
        out.extend(pending_of("fund", &self.fund));
        out.extend(pending_of("market_value", &self.market_value));
        out.extend(pending_of("currency", &self.currency));
        for (name, field) in [
            ("perc_net_assets", &self.perc_net_assets),
            ("acquisition_cost", &self.acquisition_cost),
        ] {
            if let Some(promised) = field {
                out.extend(pending_of(name, promised));
            }
        }
        if let Some(promised) = &self.acquisition_currency {
            out.extend(pending_of("acquisition_currency", promised));
        }
        out
    }

    fn resolve(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        match field {
            "fund" => self.fund = Promised::Resolved(value.str_or_fail("fund")?.to_string()),
            "market_value" => self.market_value = Promised::Resolved(resolved_float("market_value", &value)?),
            "currency" => self.currency = Promised::Resolved(value.currency_or_fail("currency")?),
            "perc_net_assets" => {
                self.perc_net_assets = Some(Promised::Resolved(resolved_float("perc_net_assets", &value)?))
            }
            "acquisition_cost" => {
                self.acquisition_cost = Some(Promised::Resolved(resolved_float("acquisition_cost", &value)?))
            }
            "acquisition_currency" => {
                self.acquisition_currency = Some(Promised::Resolved(value.currency_or_fail("acquisition_currency")?))
            }
            other => unreachable!("InvestmentData has no promisable field {other:?}"),
        }
        Ok(())
    }
}

/// An equity holding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Equity {
    #[serde(flatten)]
    pub data: InvestmentData,
}

impl Equity {
    pub fn build(fields: InvestmentFields) -> Result<Self, OutputClassError> {
        Ok(Self { data: InvestmentData::build(fields)? })
    }
}

impl PromisableFields for Equity {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        self.data.pending_fields()
    }

    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        self.data.resolve(field, value)
    }
}

/// A bond: like an [`Equity`], plus a maturity and an interest rate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bond {
    #[serde(flatten)]
    pub data: InvestmentData,
    pub maturity: Option<Date>,
    /// A fraction, not a percentage: `0.05` is five per cent.
    pub interest_rate: Option<OrderedFloat<f64>>,
}

impl Bond {
    pub fn build(
        fields: InvestmentFields,
        maturity: Option<Date>,
        interest_rate: Option<f64>,
    ) -> Result<Self, OutputClassError> {
        let interest_rate = interest_rate
            .map(|v| FloatConstraint::UnitIntervalHalfOpen.validate("interest_rate", v).map(OrderedFloat))
            .transpose()?;
        Ok(Self { data: InvestmentData::build(fields)?, maturity, interest_rate })
    }
}

impl PromisableFields for Bond {
    fn pending(&self) -> Vec<(&'static str, Promise)> {
        self.data.pending_fields()
    }

    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError> {
        self.data.resolve(field, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promisable::{Fulfilled, fulfill_promises};
    use crate::core::promise::Promise;
    use crate::core::promise_resolution::FlatPromiseMap;

    fn fields() -> InvestmentFields {
        InvestmentFields::new(
            "Acme Corp",
            "Acme",
            BlockValue::from("Alpha Fund"),
            BlockValue::from(1000.0),
            BlockValue::from(Currency::EUR),
        )
    }

    mod construction {
        use super::*;

        #[test]
        fn builds_an_equity_from_the_five_required_fields() {
            let equity = Equity::build(fields()).unwrap();
            assert_eq!(equity.data.company, "Acme Corp");
            assert_eq!(equity.data.company_match, "Acme");
            assert_eq!(equity.data.fund.resolved().map(String::as_str), Some("Alpha Fund"));
            assert_eq!(equity.data.market_value.resolved().map(|v| v.into_inner()), Some(1000.0));
            assert_eq!(equity.data.currency.resolved(), Some(&Currency::EUR));
        }

        #[test]
        fn the_optional_fields_start_empty() {
            let equity = Equity::build(fields()).unwrap();
            assert!(equity.data.nominal_quantity.is_none());
            assert!(equity.data.perc_net_assets.is_none());
            assert!(equity.data.acquisition_cost.is_none());
            assert!(equity.data.acquisition_currency.is_none());
        }

        #[test]
        fn accepts_an_integer_market_value_as_well_as_a_float() {
            let f = InvestmentFields { market_value: BlockValue::from(1000i64), ..fields() };
            assert_eq!(Equity::build(f).unwrap().data.market_value.resolved().map(|v| v.into_inner()), Some(1000.0));
        }

        #[test]
        fn a_bond_carries_its_maturity_and_interest_rate() {
            let bond = Bond::build(fields(), Some(Date::new(2025, 3, 28).unwrap()), Some(0.035)).unwrap();
            assert_eq!(bond.maturity, Some(Date::new(2025, 3, 28).unwrap()));
            assert_eq!(bond.interest_rate.map(|v| v.into_inner()), Some(0.035));
        }

        #[test]
        fn a_bond_without_maturity_or_rate_is_valid() {
            let bond = Bond::build(fields(), None, None).unwrap();
            assert!(bond.maturity.is_none() && bond.interest_rate.is_none());
        }

        #[test]
        fn a_wrongly_typed_fund_is_a_field_error_naming_the_field() {
            let f = InvestmentFields { fund: BlockValue::from(1i64), ..fields() };
            assert!(matches!(Equity::build(f), Err(OutputClassError::Field { field: "fund", .. })));
        }

        #[test]
        fn a_wrongly_typed_currency_is_a_field_error_naming_the_field() {
            let f = InvestmentFields { currency: BlockValue::from("EUR"), ..fields() };
            assert!(matches!(Equity::build(f), Err(OutputClassError::Field { field: "currency", .. })));
        }
    }

    mod range_validation {
        use super::*;

        #[test]
        fn a_negative_nominal_quantity_is_rejected() {
            let f = InvestmentFields { nominal_quantity: Some(-1.0), ..fields() };
            assert!(matches!(Equity::build(f), Err(OutputClassError::OutOfRange { field: "nominal_quantity", .. })));
        }

        #[test]
        fn perc_net_assets_accepts_the_whole_but_not_more() {
            let ok = InvestmentFields { perc_net_assets: Some(BlockValue::from(1.0)), ..fields() };
            assert!(Equity::build(ok).is_ok(), "a fund may hold a single position worth all of it");
            let ko = InvestmentFields { perc_net_assets: Some(BlockValue::from(1.0001)), ..fields() };
            assert!(matches!(Equity::build(ko), Err(OutputClassError::OutOfRange { field: "perc_net_assets", .. })));
        }

        #[test]
        fn an_interest_rate_of_one_or_more_is_rejected() {
            assert!(matches!(
                Bond::build(fields(), None, Some(1.0)),
                Err(OutputClassError::OutOfRange { field: "interest_rate", .. })
            ));
        }

        #[test]
        fn a_negative_acquisition_cost_is_rejected() {
            let f = InvestmentFields { acquisition_cost: Some(BlockValue::from(-0.01)), ..fields() };
            assert!(matches!(Equity::build(f), Err(OutputClassError::OutOfRange { field: "acquisition_cost", .. })));
        }

        #[test]
        fn a_pending_field_is_not_validated_at_construction_time() {
            // A value that is not there yet cannot be validated: the promise passes, and its domain
            // will be checked if and when the field is resolved and rebuilt.
            let f = InvestmentFields { market_value: BlockValue::Promise(Promise::new("mv")), ..fields() };
            assert!(Equity::build(f).is_ok());
        }

        #[test]
        fn the_error_message_names_both_the_field_and_the_constraint() {
            let f = InvestmentFields { nominal_quantity: Some(-1.0), ..fields() };
            let message = Equity::build(f).unwrap_err().to_string();
            assert!(message.contains("nominal_quantity"), "{message}");
            assert!(message.contains("greater than or equal to 0"), "{message}");
        }
    }

    /// Every amount of a holding admits its edge, and none of them passes it over in silence.
    /// This is the policy the engine applies to real reports: a value on the boundary is data, and
    /// data is kept — but it is rare enough to deserve one line naming it.
    mod domain_edges {
        use super::*;
        use std::sync::{Arc, Mutex};
        use tracing::Level;
        use tracing::field::{Field, Visit};
        use tracing_subscriber::Registry;
        use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

        #[derive(Clone, Debug)]
        struct Record {
            level: Level,
            message: String,
            field: String,
        }

        impl Visit for Record {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.message = format!("{value:?}");
                }
            }

            fn record_str(&mut self, field: &Field, value: &str) {
                if field.name() == "coord_ref_2" {
                    self.field = value.to_string();
                }
            }
        }

        #[derive(Clone, Default)]
        struct CapturingLayer {
            records: Arc<Mutex<Vec<Record>>>,
        }

        impl<S: tracing::Subscriber> Layer<S> for CapturingLayer {
            fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
                let mut record = Record {
                    level: *event.metadata().level(),
                    message: String::new(),
                    field: String::new(),
                };
                event.record(&mut record);
                self.records.lock().unwrap().push(record);
            }
        }

        /// The events emitted while building an entity from `fields`, and the entity itself.
        fn build_and_capture(f: InvestmentFields) -> (Result<Equity, OutputClassError>, Vec<Record>) {
            let layer = CapturingLayer::default();
            let subscriber = Registry::default().with(layer.clone());
            let built = tracing::subscriber::with_default(subscriber, || Equity::build(f));
            let records = layer.records.lock().unwrap().clone();
            (built, records)
        }

        fn edge_events(records: &[Record]) -> Vec<&Record> {
            records.iter().filter(|r| r.message.contains("edge of the admissible range")).collect()
        }

        #[test]
        fn a_zero_acquisition_cost_is_kept() {
            let f = InvestmentFields { acquisition_cost: Some(BlockValue::from(0.0)), ..fields() };
            let value = Equity::build(f).unwrap().data.acquisition_cost.and_then(|p| p.resolved().copied());
            assert_eq!(value.map(|v| v.into_inner()), Some(0.0));
        }

        #[test]
        fn a_zero_nominal_quantity_is_kept() {
            let f = InvestmentFields { nominal_quantity: Some(0.0), ..fields() };
            assert_eq!(Equity::build(f).unwrap().data.nominal_quantity.map(|v| v.into_inner()), Some(0.0));
        }

        #[test]
        fn a_position_worth_the_whole_fund_is_kept() {
            let f = InvestmentFields { perc_net_assets: Some(BlockValue::from(1.0)), ..fields() };
            let value = Equity::build(f).unwrap().data.perc_net_assets.and_then(|p| p.resolved().copied());
            assert_eq!(value.map(|v| v.into_inner()), Some(1.0));
        }

        #[test]
        fn a_value_on_the_edge_is_warned_about_once_and_names_its_field() {
            let f = InvestmentFields { market_value: BlockValue::from(0.0), ..fields() };
            let (built, records) = build_and_capture(f);
            assert!(built.is_ok());

            let edges = edge_events(&records);
            assert_eq!(edges.len(), 1, "one value on an edge, one event: {records:?}");
            assert_eq!(edges[0].level, Level::WARN);
            assert_eq!(edges[0].field, "market_value");
            assert!(edges[0].message.contains('0'), "{:?}", edges[0]);
        }

        #[test]
        fn each_field_on_an_edge_gets_its_own_event() {
            let f = InvestmentFields {
                market_value: BlockValue::from(0.0),
                nominal_quantity: Some(0.0),
                perc_net_assets: Some(BlockValue::from(1.0)),
                acquisition_cost: Some(BlockValue::from(0.0)),
                ..fields()
            };
            let (built, records) = build_and_capture(f);
            assert!(built.is_ok());

            let mut named: Vec<&str> = edge_events(&records).iter().map(|r| r.field.as_str()).collect();
            named.sort_unstable();
            assert_eq!(named, ["acquisition_cost", "market_value", "nominal_quantity", "perc_net_assets"]);
        }

        #[test]
        fn a_value_comfortably_inside_its_domain_says_nothing() {
            let (built, records) = build_and_capture(fields());
            assert!(built.is_ok());
            assert!(edge_events(&records).is_empty(), "{records:?}");
        }

        #[test]
        fn a_value_outside_the_domain_is_an_error_and_not_an_edge() {
            let f = InvestmentFields { market_value: BlockValue::from(-1.0), ..fields() };
            let (built, records) = build_and_capture(f);
            assert!(built.is_err());
            assert!(edge_events(&records).is_empty(), "{records:?}");
        }

        #[test]
        fn an_interest_rate_of_one_stays_out_of_range_because_its_bound_is_open() {
            // The coupon keeps the half-open interval: unlike a share of net assets, a rate of one
            // whole is not something a report legitimately prints.
            assert!(matches!(
                Bond::build(fields(), None, Some(1.0)),
                Err(OutputClassError::OutOfRange { field: "interest_rate", .. })
            ));
        }
    }

    mod market_value_domain {
        //! The market value is the one amount that admits zero: a holding frozen and written off
        //! is printed at `0,00` in a real report, and dropping it would hide exactly the kind of
        //! position this tool exists to find.
        use super::*;

        #[test]
        fn a_zero_market_value_is_accepted() {
            let f = InvestmentFields { market_value: BlockValue::from(0.0), ..fields() };
            let equity = Equity::build(f).unwrap();
            assert_eq!(equity.data.market_value.resolved().map(|v| v.into_inner()), Some(0.0));
        }

        #[test]
        fn a_zero_market_value_written_as_an_integer_is_accepted_too() {
            let f = InvestmentFields { market_value: BlockValue::from(0i64), ..fields() };
            assert!(Equity::build(f).is_ok());
        }

        #[test]
        fn a_negative_market_value_is_still_rejected() {
            let f = InvestmentFields { market_value: BlockValue::from(-1.0), ..fields() };
            assert!(matches!(
                Equity::build(f),
                Err(OutputClassError::OutOfRange {
                    field: "market_value",
                    constraint: FloatConstraint::NonNegative,
                    ..
                })
            ));
        }

        #[test]
        fn the_rejection_message_names_the_relaxed_constraint() {
            let f = InvestmentFields { market_value: BlockValue::from(-1.0), ..fields() };
            let message = Equity::build(f).unwrap_err().to_string();
            assert!(message.contains("market_value"), "{message}");
            assert!(message.contains("greater than or equal to 0"), "{message}");
        }

        #[test]
        fn a_zero_market_value_resolved_from_a_promise_is_accepted() {
            let f = InvestmentFields { market_value: BlockValue::Promise(Promise::new("mv")), ..fields() };
            let mut equity = Equity::build(f).unwrap();
            equity.resolve_field("market_value", BlockValue::from(0.0)).unwrap();
            assert_eq!(equity.data.market_value.resolved().map(|v| v.into_inner()), Some(0.0));
        }
    }

    mod promises {
        use super::*;

        fn promised_equity() -> Equity {
            let f = InvestmentFields {
                fund: BlockValue::Promise(Promise::new("fund-id")),
                currency: BlockValue::Promise(Promise::new("cur-id")),
                ..fields()
            };
            Equity::build(f).unwrap()
        }

        #[test]
        fn reports_every_pending_field_in_declaration_order() {
            let pending: Vec<_> = promised_equity().pending().into_iter().map(|(f, _)| f).collect();
            assert_eq!(pending, vec!["fund", "currency"]);
        }

        #[test]
        fn a_fully_resolved_investment_reports_nothing_pending() {
            assert!(Equity::build(fields()).unwrap().pending().is_empty());
        }

        #[test]
        fn every_promisable_field_can_actually_be_pending() {
            let f = InvestmentFields {
                fund: BlockValue::Promise(Promise::new("a")),
                market_value: BlockValue::Promise(Promise::new("b")),
                currency: BlockValue::Promise(Promise::new("c")),
                perc_net_assets: Some(BlockValue::Promise(Promise::new("d"))),
                acquisition_cost: Some(BlockValue::Promise(Promise::new("e"))),
                acquisition_currency: Some(BlockValue::Promise(Promise::new("f"))),
                ..fields()
            };
            let pending: Vec<_> = Equity::build(f).unwrap().pending().into_iter().map(|(n, _)| n).collect();
            assert_eq!(
                pending,
                vec!["fund", "market_value", "currency", "perc_net_assets", "acquisition_cost", "acquisition_currency"]
            );
        }

        #[test]
        fn resolving_fills_the_field_in_place() {
            let mut equity = promised_equity();
            let map = FlatPromiseMap::from_pairs([
                ("fund-id".to_string(), BlockValue::from("Alpha Fund")),
                ("cur-id".to_string(), BlockValue::from(Currency::USD)),
            ]);
            assert_eq!(fulfill_promises(&mut equity, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(equity.data.fund.resolved().map(String::as_str), Some("Alpha Fund"));
            assert_eq!(equity.data.currency.resolved(), Some(&Currency::USD));
        }

        #[test]
        fn resolving_an_integer_into_a_float_field_works() {
            let f = InvestmentFields { market_value: BlockValue::Promise(Promise::new("mv")), ..fields() };
            let mut equity = Equity::build(f).unwrap();
            let map = FlatPromiseMap::from_pairs([("mv".to_string(), BlockValue::from(1500i64))]);
            fulfill_promises(&mut equity, &map).unwrap();
            assert_eq!(equity.data.market_value.resolved().map(|v| v.into_inner()), Some(1500.0));
        }

        #[test]
        fn a_bond_resolves_the_same_shared_fields_as_an_equity() {
            let f = InvestmentFields { fund: BlockValue::Promise(Promise::new("fund-id")), ..fields() };
            let mut bond = Bond::build(f, None, None).unwrap();
            let map = FlatPromiseMap::from_pairs([("fund-id".to_string(), BlockValue::from("Alpha Fund"))]);
            fulfill_promises(&mut bond, &map).unwrap();
            assert_eq!(bond.data.fund.resolved().map(String::as_str), Some("Alpha Fund"));
        }

        #[test]
        fn resolving_with_a_wrongly_typed_value_reports_the_field() {
            let mut equity = promised_equity();
            let err = equity.resolve_field("currency", BlockValue::from(1i64)).unwrap_err();
            assert!(err.to_string().contains("currency"), "{err}");
        }
    }

    mod serde_roundtrip {
        use super::*;

        #[test]
        fn a_resolved_equity_survives_a_json_roundtrip() {
            let equity = Equity::build(fields()).unwrap();
            let json = serde_json::to_string(&equity).unwrap();
            assert_eq!(serde_json::from_str::<Equity>(&json).unwrap(), equity);
        }

        #[test]
        fn a_resolved_bond_survives_a_json_roundtrip_with_all_its_fields() {
            let f = InvestmentFields {
                nominal_quantity: Some(10.0),
                perc_net_assets: Some(BlockValue::from(0.05)),
                acquisition_cost: Some(BlockValue::from(900.0)),
                acquisition_currency: Some(BlockValue::from(Currency::USD)),
                ..fields()
            };
            let bond = Bond::build(f, Some(Date::new(2030, 1, 1).unwrap()), Some(0.02)).unwrap();
            let json = serde_json::to_string(&bond).unwrap();
            assert_eq!(serde_json::from_str::<Bond>(&json).unwrap(), bond);
        }

        #[test]
        fn the_shared_fields_are_flattened_not_nested_under_a_data_key() {
            let json = serde_json::to_string(&Equity::build(fields()).unwrap()).unwrap();
            assert!(json.contains("\"company\""), "{json}");
            assert!(!json.contains("\"data\""), "{json}");
        }
    }
}
