//! The rows of the output files, with their numeric limits and their uniqueness keys.
//!
//! Each struct here is one output table's row: the columns as they are written, the domain each
//! numeric field must fall in, and — through [`UniqueTable`] — the key on which two rows count as
//! the same.
//!
//! The validation lives here rather than at the point of writing because these are the invariants
//! of the *product*, not of the run: an output file that violates them is wrong even if every step
//! that built it was right.

use std::collections::HashSet;

use serde::Serialize;

use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
use crate::commons::date::Date;

/// A validation failure: one variant per kind of constraint the tables here actually use.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaError {
    #[error("{field} must be greater than {bound}, got {value}")]
    NotGreaterThan { field: &'static str, value: String, bound: String },
    #[error("{field} must be greater than or equal to {bound}, got {value}")]
    NotGreaterOrEqual { field: &'static str, value: String, bound: String },
    #[error("{field} must be in range [{min}, {max}], got {value}")]
    OutOfRange { field: &'static str, value: String, min: String, max: String },
}

fn positive_u32(field: &'static str, value: i64) -> Result<u32, SchemaError> {
    u32::try_from(value)
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| SchemaError::NotGreaterThan { field, value: value.to_string(), bound: "0".to_string() })
}

fn positive_u16(field: &'static str, value: i32) -> Result<u16, SchemaError> {
    u16::try_from(value)
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| SchemaError::NotGreaterThan { field, value: value.to_string(), bound: "0".to_string() })
}

fn positive_f32(field: &'static str, value: f32) -> Result<f32, SchemaError> {
    if value > 0.0 {
        Ok(value)
    } else {
        Err(SchemaError::NotGreaterThan { field, value: value.to_string(), bound: "0".to_string() })
    }
}

fn non_negative_f32(field: &'static str, value: f32) -> Result<f32, SchemaError> {
    if value >= 0.0 {
        Ok(value)
    } else {
        Err(SchemaError::NotGreaterOrEqual { field, value: value.to_string(), bound: "0".to_string() })
    }
}

/// `pa.Check.in_range(min, max)` — incluso su entrambi i lati (default di pandera).
fn in_range_inclusive_f32(field: &'static str, value: f32, min: f32, max: f32) -> Result<f32, SchemaError> {
    if value >= min && value <= max {
        Ok(value)
    } else {
        Err(SchemaError::OutOfRange {
            field,
            value: value.to_string(),
            min: min.to_string(),
            max: max.to_string(),
        })
    }
}

/// `pydantic.confloat(ge=min, lt=max)` — a differenza di `in_range_inclusive_f32`, esclude il
/// limite superiore.
fn in_range_half_open_f64(field: &'static str, value: f64, min: f64, max: f64) -> Result<f64, SchemaError> {
    if value >= min && value < max {
        Ok(value)
    } else {
        Err(SchemaError::OutOfRange {
            field,
            value: value.to_string(),
            min: min.to_string(),
            max: max.to_string(),
        })
    }
}

/// A table that keeps **one** row per uniqueness key — formatted by the caller, from one column or
/// several joined for a composite key.
///
/// # A repeated key is the same fact, not a mistake
///
/// Every key here identifies something in the world rather than something in a file: a fund's
/// merger, its assets on a date, its SFDR article, the manager that runs it. A run over several
/// reports of the same house meets those facts more than once — the same merger is printed in
/// every prospectus it concerns — and the second telling adds nothing but its own provenance.
///
/// So a repeated key is **idempotent**: the first row stays, the later ones are dropped, and the
/// run carries on. Refusing them instead used to abort the writing of an entire batch over a
/// merger reported twice, which is a real cost paid for no information.
pub struct UniqueTable<Row> {
    table: &'static str,
    field: &'static str,
    seen: HashSet<String>,
    rows: Vec<Row>,
}

impl<Row> UniqueTable<Row> {
    pub fn new(table: &'static str, field: &'static str) -> Self {
        Self { table, field, seen: HashSet::new(), rows: Vec::new() }
    }

    /// Adds `row` unless its key is already there, and says whether it was kept.
    ///
    /// `debug` rather than `warn`: nothing was lost — the fact is in the table, told by whichever
    /// report reached it first — and a batch of related reports repeats its shared facts by the
    /// hundred.
    pub fn push(&mut self, key: impl Into<String>, row: Row) -> bool {
        let key = key.into();
        if !self.seen.insert(key.clone()) {
            tracing::debug!(
                table = self.table,
                "{} already told by another report ({key}) - counted once",
                self.field
            );
            return false;
        }
        self.rows.push(row);
        true
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn into_rows(self) -> Vec<Row> {
        self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// An investments row, minus the bond-only fields, which live in [`BondAdditionalInfoRow`].
#[derive(Debug, Clone, PartialEq)]
pub struct InvestmentRow {
    pub id: u32,
    pub report_page: u16,
    pub report: String,
    pub triggering_text: String,
    pub investee: String,
    pub financial_instrument: FinancialInstrument,
    pub nominal_quantity: Option<f32>,
    pub market_value: f32,
    pub currency: Currency,
    pub perc_net_assets: Option<f32>,
    pub fund_id: u32,
    pub acquisition_cost: Option<f32>,
    pub acquisition_currency: Option<Currency>,
}

impl InvestmentRow {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: i64,
        report_page: i32,
        report: String,
        triggering_text: String,
        investee: String,
        financial_instrument: FinancialInstrument,
        nominal_quantity: Option<f32>,
        market_value: f32,
        currency: Currency,
        perc_net_assets: Option<f32>,
        fund_id: i64,
        acquisition_cost: Option<f32>,
        acquisition_currency: Option<Currency>,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id: positive_u32("ID", id)?,
            report_page: positive_u16("Report page", report_page)?,
            report,
            triggering_text,
            investee,
            financial_instrument,
            // Non-negative for the same reason as the market value: a report can carry a position
            // whose quantity has been written down to nothing, and the product's invariant is that
            // the number is not negative.
            nominal_quantity: nominal_quantity.map(|v| non_negative_f32("Nominal/Quantity", v)).transpose()?,
            // Zero is admissible here for the same reason as in
            // `InvestmentData::validate_ranges`: a frozen holding is reported at zero, and the
            // product's invariant is that the value is not negative, not that it is not zero.
            market_value: non_negative_f32("Market value", market_value)?,
            currency,
            perc_net_assets: perc_net_assets
                .map(|v| in_range_inclusive_f32("% net assets", v, 0.0, 1.0))
                .transpose()?,
            fund_id: positive_u32("Fund ID", fund_id)?,
            acquisition_cost: acquisition_cost.map(|v| non_negative_f32("Acquisition cost", v)).transpose()?,
            acquisition_currency,
        })
    }
}

/// The bond-only fields, held in a side table indexed by investment id and written as YAML rather
/// than as CSV columns: they apply to a minority of rows, and adding them as always-empty columns
/// would widen every row for nothing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BondAdditionalInfoRow {
    pub maturity: Option<Date>,
    pub interest_rate: Option<f64>,
}

impl BondAdditionalInfoRow {
    pub fn new(maturity: Option<Date>, interest_rate: Option<f64>) -> Result<Self, SchemaError> {
        Ok(Self {
            maturity,
            interest_rate: interest_rate
                .map(|v| in_range_half_open_f64("interest_rate", v, 0.0, 1.0))
                .transpose()?,
        })
    }
}

/// A funds row.
///
/// Unlike every other row here, the three provenance fields are genuinely optional: a fund seen
/// only indirectly — as another entity's fund, never as an entity of its own — never carries them.
#[derive(Debug, Clone, PartialEq)]
pub struct FundRow {
    pub id: u32,
    pub name: String,
    pub management_company_id: Option<u32>,
    pub report_page: Option<u16>,
    pub report: Option<String>,
}

impl FundRow {
    pub fn new(
        id: i64,
        name: String,
        management_company_id: Option<i64>,
        report_page: Option<i32>,
        report: Option<String>,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id: positive_u32("ID", id)?,
            name,
            management_company_id: management_company_id
                .map(|v| positive_u32("Managment company ID", v))
                .transpose()?,
            report_page: report_page.map(|v| positive_u16("Report page", v)).transpose()?,
            report,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeNameEventType {
    Renaming,
    Merging,
}

/// `funds_change_name_schema`.
#[derive(Debug, Clone, PartialEq)]
pub struct FundChangeNameRow {
    pub id: u32,
    pub report_page: u16,
    pub report: String,
    pub fund_id: u32,
    pub from_date: Date,
    pub event_type: ChangeNameEventType,
    pub old_name: String,
}

impl FundChangeNameRow {
    pub fn new(
        id: i64,
        report_page: i32,
        report: String,
        fund_id: i64,
        from_date: Date,
        event_type: ChangeNameEventType,
        old_name: String,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id: positive_u32("ID", id)?,
            report_page: positive_u16("Report page", report_page)?,
            report,
            fund_id: positive_u32("Fund ID", fund_id)?,
            from_date,
            event_type,
            old_name,
        })
    }
}

/// `funds_assets_schema`.
#[derive(Debug, Clone, PartialEq)]
pub struct FundAssetsRow {
    pub id: u32,
    pub report_page: u16,
    pub report: String,
    pub fund_id: u32,
    pub date: Option<Date>,
    pub total_assets: f32,
    pub total_liabilities: f32,
    pub total_net_assets: f32,
    pub currency: Currency,
}

impl FundAssetsRow {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: i64,
        report_page: i32,
        report: String,
        fund_id: i64,
        date: Option<Date>,
        total_assets: f32,
        total_liabilities: f32,
        total_net_assets: f32,
        currency: Currency,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id: positive_u32("ID", id)?,
            report_page: positive_u16("Report page", report_page)?,
            report,
            fund_id: positive_u32("Fund ID", fund_id)?,
            date,
            total_assets: positive_f32("Total assets", total_assets)?,
            total_liabilities: positive_f32("Total liabilities", total_liabilities)?,
            total_net_assets: positive_f32("Total net assets", total_net_assets)?,
            currency,
        })
    }
}

/// A fund SFDR classification row. There is no separate id column: the fund is the key, one
/// classification per fund.
#[derive(Debug, Clone, PartialEq)]
pub struct FundSfdrClassificationRow {
    pub fund_id: u32,
    pub sfdr_classification: SfdrArticle,
    pub report_page: u16,
    pub report: String,
}

impl FundSfdrClassificationRow {
    pub fn new(
        fund_id: i64,
        sfdr_classification: SfdrArticle,
        report_page: i32,
        report: String,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            fund_id: positive_u32("Fund ID", fund_id)?,
            sfdr_classification,
            report_page: positive_u16("Report page", report_page)?,
            report,
        })
    }
}

/// A fund ESG indicator row. The fund is **not** unique here, a fund having several indicators.
#[derive(Debug, Clone, PartialEq)]
pub struct FundEsgIndicatorRow {
    pub fund_id: u32,
    pub indicator: String,
    pub value: String,
    pub report_page: u16,
    pub report: String,
}

impl FundEsgIndicatorRow {
    pub fn new(
        fund_id: i64,
        indicator: String,
        value: String,
        report_page: i32,
        report: String,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            fund_id: positive_u32("Fund ID", fund_id)?,
            indicator,
            value,
            report_page: positive_u16("Report page", report_page)?,
            report,
        })
    }
}

/// `assets_managers_schema`.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetsManagerRow {
    pub id: u32,
    pub report_page: u16,
    pub report: String,
    pub name: String,
}

impl AssetsManagerRow {
    pub fn new(id: i64, report_page: i32, report: String, name: String) -> Result<Self, SchemaError> {
        Ok(Self {
            id: positive_u32("ID", id)?,
            report_page: positive_u16("Report page", report_page)?,
            report,
            name,
        })
    }
}

/// An investments manager row: no common columns, only the two foreign keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvestmentsManagerRow {
    pub investment_manager_id: u32,
    pub fund_id: u32,
}

impl InvestmentsManagerRow {
    pub fn new(investment_manager_id: i64, fund_id: i64) -> Result<Self, SchemaError> {
        Ok(Self {
            investment_manager_id: positive_u32("Investment manager ID", investment_manager_id)?,
            fund_id: positive_u32("Fund ID", fund_id)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u8, day: u8) -> Date {
        Date::new(year, month, day).unwrap()
    }

    // Every limit, on both sides of every interval.

    struct InvestmentRowArgs {
        id: i64,
        report_page: i32,
        nominal_quantity: Option<f32>,
        market_value: f32,
        perc_net_assets: Option<f32>,
        fund_id: i64,
        acquisition_cost: Option<f32>,
    }

    impl Default for InvestmentRowArgs {
        fn default() -> Self {
            Self {
                id: 1,
                report_page: 1,
                nominal_quantity: Some(10.0),
                market_value: 100.0,
                perc_net_assets: Some(0.5),
                fund_id: 1,
                acquisition_cost: Some(0.0),
            }
        }
    }

    impl InvestmentRowArgs {
        fn build(&self) -> Result<InvestmentRow, SchemaError> {
            InvestmentRow::new(
                self.id,
                self.report_page,
                "R".into(),
                "trig".into(),
                "Investee".into(),
                FinancialInstrument::EQUITY,
                self.nominal_quantity,
                self.market_value,
                Currency::EUR,
                self.perc_net_assets,
                self.fund_id,
                self.acquisition_cost,
                None,
            )
        }
    }

    fn valid_investment(overrides: impl FnOnce(&mut InvestmentRowArgs)) -> Result<InvestmentRow, SchemaError> {
        let mut args = InvestmentRowArgs::default();
        overrides(&mut args);
        args.build()
    }

    mod investment_row {
        use super::*;
        use test_case::test_case;

        #[test]
        fn accepts_fully_populated_valid_values() {
            assert!(valid_investment(|_| {}).is_ok());
        }

        #[test]
        fn accepts_all_nullable_fields_absent() {
            let row = valid_investment(|a| {
                a.nominal_quantity = None;
                a.perc_net_assets = None;
                a.acquisition_cost = None;
            })
            .unwrap();
            assert_eq!(row.nominal_quantity, None);
            assert_eq!(row.perc_net_assets, None);
            assert_eq!(row.acquisition_cost, None);
        }

        #[test_case(0; "zero")]
        #[test_case(-1; "negative")]
        fn rejects_non_positive_id(bad_id: i64) {
            assert!(matches!(
                valid_investment(|a| a.id = bad_id),
                Err(SchemaError::NotGreaterThan { field: "ID", .. })
            ));
        }

        #[test_case(0; "zero")]
        #[test_case(-1; "negative")]
        fn rejects_non_positive_report_page(bad: i32) {
            assert!(matches!(
                valid_investment(|a| a.report_page = bad),
                Err(SchemaError::NotGreaterThan { field: "Report page", .. })
            ));
        }

        #[test]
        fn accepts_a_zero_nominal_quantity() {
            assert!(valid_investment(|a| a.nominal_quantity = Some(0.0)).is_ok());
        }

        #[test]
        fn rejects_a_negative_nominal_quantity() {
            assert!(matches!(
                valid_investment(|a| a.nominal_quantity = Some(-5.0)),
                Err(SchemaError::NotGreaterOrEqual { field: "Nominal/Quantity", .. })
            ));
        }

        #[test]
        fn accepts_a_zero_market_value() {
            assert!(valid_investment(|a| a.market_value = 0.0).is_ok());
        }

        #[test]
        fn rejects_a_negative_market_value() {
            assert!(matches!(
                valid_investment(|a| a.market_value = -1.0),
                Err(SchemaError::NotGreaterOrEqual { field: "Market value", .. })
            ));
        }

        #[test_case(0.0; "lower bound inclusive is fine")]
        #[test_case(1.0; "upper bound inclusive is fine")]
        fn accepts_perc_net_assets_at_inclusive_bounds(ok: f32) {
            assert!(valid_investment(|a| a.perc_net_assets = Some(ok)).is_ok());
        }

        #[test_case(-0.0001; "just below zero")]
        #[test_case(1.0001; "just above one")]
        fn rejects_perc_net_assets_outside_unit_range(bad: f32) {
            assert!(matches!(
                valid_investment(|a| a.perc_net_assets = Some(bad)),
                Err(SchemaError::OutOfRange { field: "% net assets", .. })
            ));
        }

        #[test_case(0; "zero")]
        #[test_case(-1; "negative")]
        fn rejects_non_positive_fund_id(bad: i64) {
            assert!(matches!(
                valid_investment(|a| a.fund_id = bad),
                Err(SchemaError::NotGreaterThan { field: "Fund ID", .. })
            ));
        }

        #[test]
        fn accepts_zero_acquisition_cost() {
            assert!(valid_investment(|a| a.acquisition_cost = Some(0.0)).is_ok());
        }

        #[test]
        fn rejects_negative_acquisition_cost() {
            assert!(matches!(
                valid_investment(|a| a.acquisition_cost = Some(-0.01)),
                Err(SchemaError::NotGreaterOrEqual { field: "Acquisition cost", .. })
            ));
        }

        #[test]
        fn every_financial_instrument_variant_is_accepted() {
            for instrument in [FinancialInstrument::EQUITY, FinancialInstrument::BOND] {
                let row = InvestmentRow::new(
                    1, 1, "R".into(), "t".into(), "i".into(), instrument, None, 1.0, Currency::EUR, None, 1,
                    None, None,
                )
                .unwrap();
                assert_eq!(row.financial_instrument, instrument);
            }
        }
    }

    // --- BondAdditionalInfoRow ---

    mod bond_additional_info_row {
        use super::*;
        use test_case::test_case;

        #[test]
        fn accepts_no_maturity_and_no_interest_rate() {
            assert!(BondAdditionalInfoRow::new(None, None).is_ok());
        }

        #[test]
        fn accepts_a_maturity_date() {
            let row = BondAdditionalInfoRow::new(Some(date(2030, 6, 15)), None).unwrap();
            assert_eq!(row.maturity, Some(date(2030, 6, 15)));
        }

        #[test_case(0.0; "lower bound inclusive")]
        #[test_case(0.999; "just under upper bound")]
        fn accepts_interest_rate_in_half_open_range(ok: f64) {
            assert!(BondAdditionalInfoRow::new(None, Some(ok)).is_ok());
        }

        #[test]
        fn rejects_interest_rate_equal_to_one() {
            // Semiaperto [0,1): a differenza di "% net assets", il limite superiore stesso e'
            // rifiutato.
            assert!(matches!(
                BondAdditionalInfoRow::new(None, Some(1.0)),
                Err(SchemaError::OutOfRange { field: "interest_rate", .. })
            ));
        }

        #[test]
        fn rejects_negative_interest_rate() {
            assert!(BondAdditionalInfoRow::new(None, Some(-0.1)).is_err());
        }
    }

    // The only row with genuinely optional provenance fields.

    mod fund_row {
        use super::*;

        #[test]
        fn accepts_full_debug_info() {
            let row = FundRow::new(1, "Fund A".into(), Some(2), Some(3), Some("R".into())).unwrap();
            assert_eq!(row.report_page, Some(3));
        }

        #[test]
        fn accepts_no_debug_info_at_all() {
            // A fund seen only indirectly — as another entity's fund, never as an entity of its own
            // — carries no provenance.
            let row = FundRow::new(1, "Fund A".into(), None, None, None).unwrap();
            assert_eq!(row.report_page, None);
            assert_eq!(row.report, None);
        }

        #[test]
        fn rejects_non_positive_management_company_id_when_present() {
            assert!(FundRow::new(1, "F".into(), Some(0), None, None).is_err());
        }

        #[test]
        fn rejects_non_positive_report_page_when_present() {
            assert!(FundRow::new(1, "F".into(), None, Some(0), None).is_err());
        }

        #[test]
        fn rejects_non_positive_id() {
            assert!(FundRow::new(0, "F".into(), None, None, None).is_err());
        }
    }

    // --- FundChangeNameRow ---

    mod fund_change_name_row {
        use super::*;
        use test_case::test_case;

        #[test_case(ChangeNameEventType::Renaming; "renaming")]
        #[test_case(ChangeNameEventType::Merging; "merging")]
        fn accepts_both_event_types(event_type: ChangeNameEventType) {
            let row =
                FundChangeNameRow::new(1, 1, "R".into(), 1, date(2024, 1, 1), event_type, "Old".into())
                    .unwrap();
            assert_eq!(row.event_type, event_type);
        }

        #[test]
        fn rejects_non_positive_fund_id() {
            assert!(
                FundChangeNameRow::new(
                    1,
                    1,
                    "R".into(),
                    0,
                    date(2024, 1, 1),
                    ChangeNameEventType::Renaming,
                    "Old".into()
                )
                .is_err()
            );
        }

        #[test]
        fn rejects_non_positive_id() {
            assert!(
                FundChangeNameRow::new(
                    0,
                    1,
                    "R".into(),
                    1,
                    date(2024, 1, 1),
                    ChangeNameEventType::Renaming,
                    "Old".into()
                )
                .is_err()
            );
        }

        #[test]
        fn rejects_non_positive_report_page() {
            assert!(
                FundChangeNameRow::new(
                    1,
                    0,
                    "R".into(),
                    1,
                    date(2024, 1, 1),
                    ChangeNameEventType::Renaming,
                    "Old".into()
                )
                .is_err()
            );
        }
    }

    // --- FundAssetsRow ---

    mod fund_assets_row {
        use super::*;
        use test_case::test_case;

        #[test]
        fn accepts_null_date() {
            let row = FundAssetsRow::new(1, 1, "R".into(), 1, None, 100.0, 50.0, 50.0, Currency::EUR)
                .unwrap();
            assert_eq!(row.date, None);
        }

        #[test]
        fn accepts_a_resolved_date() {
            let row =
                FundAssetsRow::new(1, 1, "R".into(), 1, Some(date(2024, 3, 1)), 100.0, 50.0, 50.0, Currency::EUR)
                    .unwrap();
            assert_eq!(row.date, Some(date(2024, 3, 1)));
        }

        #[test_case(0.0; "total assets zero")]
        fn rejects_non_positive_total_assets(bad: f32) {
            assert!(FundAssetsRow::new(1, 1, "R".into(), 1, None, bad, 50.0, 50.0, Currency::EUR).is_err());
        }

        #[test_case(0.0; "total liabilities zero")]
        fn rejects_non_positive_total_liabilities(bad: f32) {
            assert!(FundAssetsRow::new(1, 1, "R".into(), 1, None, 100.0, bad, 50.0, Currency::EUR).is_err());
        }

        #[test_case(0.0; "total net assets zero")]
        fn rejects_non_positive_total_net_assets(bad: f32) {
            assert!(FundAssetsRow::new(1, 1, "R".into(), 1, None, 100.0, 50.0, bad, Currency::EUR).is_err());
        }

        #[test]
        fn rejects_non_positive_fund_id() {
            assert!(FundAssetsRow::new(1, 1, "R".into(), 0, None, 100.0, 50.0, 50.0, Currency::EUR).is_err());
        }
    }

    // --- FundSfdrClassificationRow ---

    mod fund_sfdr_classification_row {
        use super::*;
        use test_case::test_case;

        #[test_case(SfdrArticle::Art6; "article 6")]
        #[test_case(SfdrArticle::Art8; "article 8")]
        #[test_case(SfdrArticle::Art9; "article 9")]
        fn accepts_every_article(article: SfdrArticle) {
            let row = FundSfdrClassificationRow::new(1, article, 1, "R".into()).unwrap();
            assert_eq!(row.sfdr_classification, article);
        }

        #[test]
        fn rejects_non_positive_report_page() {
            assert!(FundSfdrClassificationRow::new(1, SfdrArticle::Art6, 0, "R".into()).is_err());
        }

        #[test]
        fn rejects_non_positive_fund_id() {
            assert!(FundSfdrClassificationRow::new(0, SfdrArticle::Art6, 1, "R".into()).is_err());
        }
    }

    // --- FundEsgIndicatorRow ---

    mod fund_esg_indicator_row {
        use super::*;

        #[test]
        fn allows_arbitrary_indicator_and_value_strings() {
            let row = FundEsgIndicatorRow::new(1, "GHG intensity".into(), "12.3".into(), 1, "R".into())
                .unwrap();
            assert_eq!(row.indicator, "GHG intensity");
            assert_eq!(row.value, "12.3");
        }

        #[test]
        fn rejects_non_positive_fund_id() {
            assert!(FundEsgIndicatorRow::new(0, "I".into(), "V".into(), 1, "R".into()).is_err());
        }

        #[test]
        fn rejects_non_positive_report_page() {
            assert!(FundEsgIndicatorRow::new(1, "I".into(), "V".into(), 0, "R".into()).is_err());
        }
    }

    // --- AssetsManagerRow ---

    mod assets_manager_row {
        use super::*;

        #[test]
        fn accepts_valid_values() {
            assert!(AssetsManagerRow::new(1, 1, "R".into(), "BlackRock".into()).is_ok());
        }

        #[test]
        fn rejects_non_positive_id() {
            assert!(AssetsManagerRow::new(0, 1, "R".into(), "BlackRock".into()).is_err());
        }

        #[test]
        fn rejects_non_positive_report_page() {
            assert!(AssetsManagerRow::new(1, 0, "R".into(), "BlackRock".into()).is_err());
        }
    }

    // --- InvestmentsManagerRow ---

    mod investments_manager_row {
        use super::*;
        use test_case::test_case;

        #[test]
        fn accepts_valid_ids() {
            assert!(InvestmentsManagerRow::new(1, 2).is_ok());
        }

        #[test_case(0, 1; "manager id zero")]
        #[test_case(1, 0; "fund id zero")]
        fn rejects_either_id_non_positive(manager_id: i64, fund_id: i64) {
            assert!(InvestmentsManagerRow::new(manager_id, fund_id).is_err());
        }
    }

    // --- UniqueTable: semantica di dedup + uno stress test ragionevole ---

    mod unique_table {
        use super::*;

        #[test]
        fn accepts_first_occurrence_of_each_key() {
            let mut t: UniqueTable<u32> = UniqueTable::new("t", "k");
            assert!(t.push("a", 1));
            assert!(t.push("b", 2));
            assert_eq!(t.rows(), &[1, 2]);
        }

        #[test]
        fn a_repeated_key_is_counted_once_instead_of_failing() {
            let mut t: UniqueTable<u32> = UniqueTable::new("investments", "ID");
            t.push("1", 1);
            assert!(!t.push("1", 99), "the second telling of the same fact is dropped");
            assert_eq!(t.len(), 1);
            assert_eq!(t.rows(), &[1]);
        }

        #[test]
        fn the_row_kept_is_the_one_that_arrived_first() {
            // Two reports tell the same fact; the later one differs only in provenance, so which
            // of the two is kept is arbitrary — but it must be *decided*, not left to chance.
            let mut t: UniqueTable<&str> = UniqueTable::new("t", "k");
            t.push("k", "from the first report");
            t.push("k", "from the second report");
            assert_eq!(t.rows(), &["from the first report"]);
        }

        #[test]
        fn starts_empty() {
            let t: UniqueTable<u32> = UniqueTable::new("t", "k");
            assert!(t.is_empty());
            assert_eq!(t.len(), 0);
        }

        #[test]
        fn into_rows_consumes_the_table() {
            let mut t: UniqueTable<u32> = UniqueTable::new("t", "k");
            t.push("a", 1);
            t.push("b", 2);
            assert_eq!(t.into_rows(), vec![1, 2]);
        }

        #[test]
        fn combo_key_treats_differing_components_as_distinct() {
            // The same fund on a different date is not a duplicate; the same fund on the same date
            // is.
            let mut t: UniqueTable<(u32, &str)> = UniqueTable::new("funds_assets", "Fund ID|Date");
            t.push("1|2024-01-01", (1, "2024-01-01"));
            t.push("1|2024-02-01", (1, "2024-02-01"));
            t.push("2|2024-01-01", (2, "2024-01-01"));
            assert_eq!(t.len(), 3);
            assert!(!t.push("1|2024-01-01", (1, "2024-01-01")));
        }

        #[test]
        fn stress_10k_unique_keys_all_accepted_then_every_key_deduplicated_on_replay() {
            let mut t: UniqueTable<u32> = UniqueTable::new("stress", "ID");
            for i in 0..10_000u32 {
                assert!(t.push(i.to_string(), i));
            }
            assert_eq!(t.len(), 10_000);
            let deduplicated = (0..10_000u32).filter(|i| !t.push(i.to_string(), *i)).count();
            assert_eq!(deduplicated, 10_000);
            assert_eq!(t.len(), 10_000);
        }
    }

    mod schema_error_display {
        use super::*;

        #[test]
        fn not_greater_than_message() {
            assert_eq!(
                SchemaError::NotGreaterThan { field: "ID", value: "0".into(), bound: "0".into() }.to_string(),
                "ID must be greater than 0, got 0"
            );
        }

        #[test]
        fn not_greater_or_equal_message() {
            assert_eq!(
                SchemaError::NotGreaterOrEqual { field: "Acquisition cost", value: "-0.01".into(), bound: "0".into() }
                    .to_string(),
                "Acquisition cost must be greater than or equal to 0, got -0.01"
            );
        }

        #[test]
        fn out_of_range_message() {
            assert_eq!(
                SchemaError::OutOfRange { field: "% net assets", value: "1.5".into(), min: "0".into(), max: "1".into() }
                    .to_string(),
                "% net assets must be in range [0, 1], got 1.5"
            );
        }
    }
}
