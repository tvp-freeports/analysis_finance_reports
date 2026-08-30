//! Le entità `Equity` e `Bond`: una posizione di un fondo su una società bersaglio.
//!
//! Anticipate da M8 a M7 per decisione D-M7-2 dell'utente — vedi il doc-comment di
//! [`crate::output::classes`].
//!
//! `Investment` è astratta nel riferimento e non è mai istanziata; `Equity` e `Bond` sono gli
//! unici tipi concreti e non sono mai sottoclassati. Qui le due struct condividono
//! [`InvestmentData`] — i nove campi comuni e tutta la logica di promessa — e `Bond` aggiunge i
//! propri due (`maturity`, `interest_rate`). Non sono due varianti di un enum perché il codice a
//! valle le distingue sempre per tipo (finiscono in due CSV diversi), mai per `match`.
//!
//! **Non portato**: `Investment.__str__`/`Bond.__str__`, un dump multi-riga tradotto. Nel
//! riferimento è già verificato che nessuno lo chiami; `Debug` copre la stessa esigenza.

use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

use crate::commons::consts::Currency;
use crate::commons::date::Date;
use crate::core::classes::{BlockValue, BlockValueError};
use crate::core::promisable::{PromisableFields, Promised};
use crate::core::promise::Promise;

use super::{
    FloatConstraint, OutputClassError, optional_promised_from_value, pending_of, promised_from_value,
    serde_optional_promised, serde_promised,
};

/// I campi che `Equity` e `Bond` hanno in comune.
///
/// Ogni campo tranne `company`/`company_match`/`nominal_quantity` può arrivare come promessa: è
/// il meccanismo con cui un valore scoperto in una pagina diversa (tipicamente il nome del fondo
/// o la valuta) viene riempito a posteriori.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestmentData {
    pub company: String,
    pub company_match: String,
    #[serde(with = "serde_promised")]
    pub fund: Promised<String>,
    pub nominal_quantity: Option<OrderedFloat<f64>>,
    #[serde(with = "serde_promised")]
    pub market_value: Promised<OrderedFloat<f64>>,
    #[serde(with = "serde_promised")]
    pub currency: Promised<Currency>,
    #[serde(with = "serde_optional_promised")]
    pub perc_net_assets: Option<Promised<OrderedFloat<f64>>>,
    #[serde(with = "serde_optional_promised")]
    pub acquisition_cost: Option<Promised<OrderedFloat<f64>>>,
    #[serde(with = "serde_optional_promised")]
    pub acquisition_currency: Option<Promised<Currency>>,
}

/// I valori grezzi da cui si costruisce un investimento, così come arrivano dai metadati di un
/// `TextBlock`. Ogni campo è un [`BlockValue`], quindi ognuno può essere una promessa.
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
    /// I quattro campi che il riferimento pretende sempre presenti (`md["company"]`,
    /// `md["company match"]`, `md["fund"]`, `md["market value"]`, `md["currency"]`); gli altri
    /// sono opzionali e partono assenti.
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

/// Estrae un numero da un [`BlockValue`], accettando indifferentemente `Int` e `Float` (i cast di
/// `deserialize::cast` producono l'uno o l'altro a seconda del formato del documento).
///
/// Qui si controlla solo il **tipo**: il dominio (`> 0`, `[0, 1)`) lo verifica
/// [`InvestmentData::validate_ranges`], perché un valore fuori dominio non è un errore di tipo e
/// perché un campo ancora promesso non è validabile finché la promessa non si risolve.
fn resolved_float(field: &'static str, value: &BlockValue) -> Result<OrderedFloat<f64>, BlockValueError> {
    match value {
        BlockValue::Int(i) => Ok(OrderedFloat(*i as f64)),
        other => other.float_or_fail(field).map(OrderedFloat),
    }
}

impl InvestmentData {
    /// Costruisce i campi comuni, validando i domini numerici dei valori già risolti (una
    /// promessa non è validabile ora: lo sarà quando `fulfill_promises` la risolverà).
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
                .map(|v| FloatConstraint::Positive.validate("nominal_quantity", v).map(OrderedFloat))
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

    /// Verifica i domini numerici dei soli campi già risolti.
    fn validate_ranges(&self) -> Result<(), OutputClassError> {
        if let Some(v) = self.market_value.resolved() {
            FloatConstraint::Positive.validate("market_value", v.into_inner())?;
        }
        if let Some(Some(v)) = self.perc_net_assets.as_ref().map(Promised::resolved) {
            FloatConstraint::UnitIntervalHalfOpen.validate("perc_net_assets", v.into_inner())?;
        }
        if let Some(Some(v)) = self.acquisition_cost.as_ref().map(Promised::resolved) {
            FloatConstraint::Positive.validate("acquisition_cost", v.into_inner())?;
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

/// Una partecipazione azionaria.
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

/// Un'obbligazione: come [`Equity`], più scadenza e tasso d'interesse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bond {
    #[serde(flatten)]
    pub data: InvestmentData,
    pub maturity: Option<Date>,
    /// Frazione, non percentuale: `0.05` è il 5%.
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
        fn a_zero_market_value_is_rejected_because_the_field_is_strictly_positive() {
            let f = InvestmentFields { market_value: BlockValue::from(0.0), ..fields() };
            assert!(matches!(
                Equity::build(f),
                Err(OutputClassError::OutOfRange { field: "market_value", constraint: FloatConstraint::Positive, .. })
            ));
        }

        #[test]
        fn a_negative_market_value_is_rejected() {
            let f = InvestmentFields { market_value: BlockValue::from(-1.0), ..fields() };
            assert!(Equity::build(f).is_err());
        }

        #[test]
        fn a_zero_nominal_quantity_is_rejected() {
            let f = InvestmentFields { nominal_quantity: Some(0.0), ..fields() };
            assert!(matches!(Equity::build(f), Err(OutputClassError::OutOfRange { field: "nominal_quantity", .. })));
        }

        #[test]
        fn perc_net_assets_accepts_zero_but_not_one() {
            let ok = InvestmentFields { perc_net_assets: Some(BlockValue::from(0.0)), ..fields() };
            assert!(Equity::build(ok).is_ok());
            let ko = InvestmentFields { perc_net_assets: Some(BlockValue::from(1.0)), ..fields() };
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
        fn a_zero_acquisition_cost_is_rejected() {
            let f = InvestmentFields { acquisition_cost: Some(BlockValue::from(0.0)), ..fields() };
            assert!(matches!(Equity::build(f), Err(OutputClassError::OutOfRange { field: "acquisition_cost", .. })));
        }

        #[test]
        fn a_pending_field_is_not_validated_at_construction_time() {
            // Non si può validare un valore che non c'è ancora: la promessa passa, e il dominio
            // sarà verificato solo se e quando il campo verrà risolto e ricostruito.
            let f = InvestmentFields { market_value: BlockValue::Promise(Promise::new("mv")), ..fields() };
            assert!(Equity::build(f).is_ok());
        }

        #[test]
        fn the_error_message_names_both_the_field_and_the_constraint() {
            let f = InvestmentFields { market_value: BlockValue::from(0.0), ..fields() };
            let message = Equity::build(f).unwrap_err().to_string();
            assert!(message.contains("market_value"), "{message}");
            assert!(message.contains("greater than 0"), "{message}");
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
