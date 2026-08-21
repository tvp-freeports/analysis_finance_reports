//! Rust port of `output/files_schema.py` — the `pandera.DataFrameSchema` definitions that
//! `output/routines.py::transform_to_files_schema` validates its accumulated rows against before
//! writing CSVs.
//!
//! **Design decision (user confirmed, 2026-08-19)**: this is deliberately *not* a literal port.
//! The Python original validates loosely-typed `dict`s at a DataFrame boundary because that's the
//! only place Python's type system gives it any grip — `pa.Check.isin(...)` exists because a
//! plain `str` field could hold anything. Every output class this data comes from
//! (`Fund`/`Equity`/`Bond`/... in `output/{fund,investment,...}.rs`) is now a Rust pyclass, and
//! `FinancialInstrument`/`SfdrArticle`/`Currency` are already typed enums (`commons/consts.rs`).
//! So instead of hand-porting each pandera `Check` as a runtime validator on a dict, each table
//! is a plain Rust struct whose fields use those enums and newtyped numeric bounds directly —
//! `pa.Check.isin([...])` becomes "the field's type is the enum", not a runtime check at all.
//! What's left as real runtime validation is exactly what *can't* be structural: numeric bounds
//! (`greater_than`/`in_range`, checked in each row constructor) and cross-row uniqueness (checked
//! by [`UniqueTable`] as rows are accumulated, mirroring pandera's `unique=[...]`).
//!
//! Column-presence quirks that exist in the Python schemas because a `dict`-per-row can simply
//! omit a key are preserved only where the accumulator (`transform_to_files_schema` in
//! `routines.py`) actually exercises that quirk: every row type below has non-optional
//! `report`/`format`/`report_page` fields *except* [`FundRow`], because
//! `transform_to_files_schema`'s `funds_change_name`/`funds_assets`/`funds_sfdr_classification`/
//! `funds_esg_indicators`/`investments` loops can each create a bare `Fund` entry (keyed only by
//! `ID`/`Name`) for a fund that's never independently classified as a `page_results.funds` block
//! — such a fund's row never receives `add_debug_infos`, so those three columns end up absent
//! from the Python DataFrame too (`pa.Column(..., required=False)` on `common_columns` exists
//! specifically for this).

use std::collections::HashSet;
use std::fmt;

use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
use crate::core::py_date::SimpleDate;

/// Replaces pandera raising `SchemaError` on a failed `Check` — one variant per kind of bound
/// this migration's tables actually use.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaError {
    NotGreaterThan { field: &'static str, value: f64, bound: f64 },
    NotGreaterOrEqual { field: &'static str, value: f64, bound: f64 },
    OutOfRange { field: &'static str, value: f64, min: f64, max: f64 },
    Duplicate { table: &'static str, field: &'static str, value: String },
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaError::NotGreaterThan { field, value, bound } => {
                write!(f, "{field} must be greater than {bound}, got {value}")
            }
            SchemaError::NotGreaterOrEqual { field, value, bound } => {
                write!(f, "{field} must be greater than or equal to {bound}, got {value}")
            }
            SchemaError::OutOfRange { field, value, min, max } => {
                write!(f, "{field} must be in range [{min}, {max}], got {value}")
            }
            SchemaError::Duplicate { table, field, value } => {
                write!(f, "{table}: duplicate {field} value `{value}`")
            }
        }
    }
}

impl std::error::Error for SchemaError {}

fn positive_u32(field: &'static str, value: i64) -> Result<u32, SchemaError> {
    if value > 0 && value <= u32::MAX as i64 {
        Ok(value as u32)
    } else {
        Err(SchemaError::NotGreaterThan { field, value: value as f64, bound: 0.0 })
    }
}

fn positive_u16(field: &'static str, value: i32) -> Result<u16, SchemaError> {
    if value > 0 && value <= u16::MAX as i32 {
        Ok(value as u16)
    } else {
        Err(SchemaError::NotGreaterThan { field, value: value as f64, bound: 0.0 })
    }
}

fn positive_f32(field: &'static str, value: f32) -> Result<f32, SchemaError> {
    if value > 0.0 {
        Ok(value)
    } else {
        Err(SchemaError::NotGreaterThan { field, value: value as f64, bound: 0.0 })
    }
}

fn non_negative_f32(field: &'static str, value: f32) -> Result<f32, SchemaError> {
    if value >= 0.0 {
        Ok(value)
    } else {
        Err(SchemaError::NotGreaterOrEqual { field, value: value as f64, bound: 0.0 })
    }
}

/// `pa.Check.in_range(min, max)` — inclusive on both ends (pandera's default).
fn in_range_inclusive_f32(field: &'static str, value: f32, min: f32, max: f32) -> Result<f32, SchemaError> {
    if value >= min && value <= max {
        Ok(value)
    } else {
        Err(SchemaError::OutOfRange { field, value: value as f64, min: min as f64, max: max as f64 })
    }
}

/// `pydantic.confloat(ge=min, lt=max)` — used only by `BondAdditionalInfos.interest_rate`,
/// unlike `% net assets`'s pandera `in_range` this excludes the upper bound. Stays `f64`
/// (unlike every other numeric field in this module) because `BondAdditionalInfos` is a bare
/// Pydantic model dumped straight to YAML — it never goes through pandas' `Float32Dtype`
/// coercion the way `investments_schema`'s DataFrame columns do, so narrowing to `f32` here would
/// introduce real precision noise on the `f32` -> `f64` upcast when serializing
/// (`investments_add_infos.yaml` mismatch, caught by the full `analysis_finance_reports_formats`
/// suite — not just a style choice).
fn in_range_half_open_f64(field: &'static str, value: f64, min: f64, max: f64) -> Result<f64, SchemaError> {
    if value >= min && value < max {
        Ok(value)
    } else {
        Err(SchemaError::OutOfRange { field, value, min, max })
    }
}

/// A growable table that rejects a row whose uniqueness key (formatted by the caller — a single
/// column's value, or several joined together for combo-uniqueness) has already been seen.
/// Mirrors pandera's per-schema `unique=...`, which is enforced by `DataFrameSchema.validate`
/// across the whole accumulated table, not per-row — same semantics, but caught the moment a
/// duplicate is pushed instead of after building the whole table.
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

    pub fn push(&mut self, key: impl Into<String>, row: Row) -> Result<(), SchemaError> {
        let key = key.into();
        if !self.seen.insert(key.clone()) {
            return Err(SchemaError::Duplicate { table: self.table, field: self.field, value: key });
        }
        self.rows.push(row);
        Ok(())
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

/// `investments_schema`, minus the bond-only fields (split out into [`BondAdditionalInfoRow`],
/// matching `BondAdditionalInfos`/the `d = {k: v for k, v in d.items() if k not in infos}` split
/// in `transform_to_files_schema`).
#[derive(Debug, Clone, PartialEq)]
pub struct InvestmentRow {
    pub id: u32,
    pub report_page: u16,
    pub report: String,
    pub format: String,
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
        format: String,
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
            format,
            triggering_text,
            investee,
            financial_instrument,
            nominal_quantity: nominal_quantity.map(|v| positive_f32("Nominal/Quantity", v)).transpose()?,
            market_value: positive_f32("Market value", market_value)?,
            currency,
            perc_net_assets: perc_net_assets.map(|v| in_range_inclusive_f32("% net assets", v, 0.0, 1.0)).transpose()?,
            fund_id: positive_u32("Fund ID", fund_id)?,
            acquisition_cost: acquisition_cost.map(|v| non_negative_f32("Acquisition cost", v)).transpose()?,
            acquisition_currency,
        })
    }
}

/// `BondAdditionalInfos` (a bare Pydantic model in Python, not part of a `DataFrameSchema`) — the
/// side table keyed by investment ID (`ResultsAccumulator.add_infos`), written to
/// `investments_add_infos.yaml` rather than a CSV column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondAdditionalInfoRow {
    pub maturity: Option<SimpleDate>,
    pub interest_rate: Option<f64>,
}

impl BondAdditionalInfoRow {
    pub fn new(maturity: Option<SimpleDate>, interest_rate: Option<f64>) -> Result<Self, SchemaError> {
        Ok(Self {
            maturity,
            interest_rate: interest_rate.map(|v| in_range_half_open_f64("interest_rate", v, 0.0, 1.0)).transpose()?,
        })
    }
}

/// `funds_schema`. Unlike every other row type here, `report_page`/`report`/`format` are
/// genuinely optional — see the module doc for why.
#[derive(Debug, Clone, PartialEq)]
pub struct FundRow {
    pub id: u32,
    pub name: String,
    pub management_company_id: Option<u32>,
    pub report_page: Option<u16>,
    pub report: Option<String>,
    pub format: Option<String>,
}

impl FundRow {
    pub fn new(
        id: i64,
        name: String,
        management_company_id: Option<i64>,
        report_page: Option<i32>,
        report: Option<String>,
        format: Option<String>,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id: positive_u32("ID", id)?,
            name,
            management_company_id: management_company_id.map(|v| positive_u32("Managment company ID", v)).transpose()?,
            report_page: report_page.map(|v| positive_u16("Report page", v)).transpose()?,
            report,
            format,
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
    pub format: String,
    pub fund_id: u32,
    pub from_date: SimpleDate,
    pub event_type: ChangeNameEventType,
    pub old_name: String,
}

impl FundChangeNameRow {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: i64,
        report_page: i32,
        report: String,
        format: String,
        fund_id: i64,
        from_date: SimpleDate,
        event_type: ChangeNameEventType,
        old_name: String,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id: positive_u32("ID", id)?,
            report_page: positive_u16("Report page", report_page)?,
            report,
            format,
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
    pub format: String,
    pub fund_id: u32,
    pub date: Option<SimpleDate>,
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
        format: String,
        fund_id: i64,
        date: Option<SimpleDate>,
        total_assets: f32,
        total_liabilities: f32,
        total_net_assets: f32,
        currency: Currency,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id: positive_u32("ID", id)?,
            report_page: positive_u16("Report page", report_page)?,
            report,
            format,
            fund_id: positive_u32("Fund ID", fund_id)?,
            date,
            total_assets: positive_f32("Total assets", total_assets)?,
            total_liabilities: positive_f32("Total liabilities", total_liabilities)?,
            total_net_assets: positive_f32("Total net assets", total_net_assets)?,
            currency,
        })
    }
}

/// `funds_sfdr_classification_schema` — note there's no `ID` column at all; `Fund ID` itself is
/// the (unique) key, one classification per fund.
#[derive(Debug, Clone, PartialEq)]
pub struct FundSfdrClassificationRow {
    pub fund_id: u32,
    pub sfdr_classification: SfdrArticle,
    pub report_page: u16,
    pub report: String,
    pub format: String,
}

impl FundSfdrClassificationRow {
    pub fn new(
        fund_id: i64,
        sfdr_classification: SfdrArticle,
        report_page: i32,
        report: String,
        format: String,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            fund_id: positive_u32("Fund ID", fund_id)?,
            sfdr_classification,
            report_page: positive_u16("Report page", report_page)?,
            report,
            format,
        })
    }
}

/// `funds_esg_indicators_schema` — `Fund ID` is *not* unique here (a fund can have several
/// indicators).
#[derive(Debug, Clone, PartialEq)]
pub struct FundEsgIndicatorRow {
    pub fund_id: u32,
    pub indicator: String,
    pub value: String,
    pub report_page: u16,
    pub report: String,
    pub format: String,
}

impl FundEsgIndicatorRow {
    pub fn new(
        fund_id: i64,
        indicator: String,
        value: String,
        report_page: i32,
        report: String,
        format: String,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            fund_id: positive_u32("Fund ID", fund_id)?,
            indicator,
            value,
            report_page: positive_u16("Report page", report_page)?,
            report,
            format,
        })
    }
}

/// `assets_managers_schema`.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetsManagerRow {
    pub id: u32,
    pub report_page: u16,
    pub report: String,
    pub format: String,
    pub name: String,
}

impl AssetsManagerRow {
    pub fn new(id: i64, report_page: i32, report: String, format: String, name: String) -> Result<Self, SchemaError> {
        Ok(Self {
            id: positive_u32("ID", id)?,
            report_page: positive_u16("Report page", report_page)?,
            report,
            format,
            name,
        })
    }
}

/// `investments_managers_schema` — no `common_columns` at all, just the two FK ids.
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
    use test_case::test_case;

    fn date(year: i32, month: u8, day: u8) -> SimpleDate {
        SimpleDate { year, month, day }
    }

    // --- InvestmentRow: every bound, both sides of every branch ---

    fn valid_investment(overrides: impl FnOnce(&mut InvestmentRowArgs)) -> Result<InvestmentRow, SchemaError> {
        let mut args = InvestmentRowArgs::default();
        overrides(&mut args);
        args.build()
    }

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
            Self { id: 1, report_page: 1, nominal_quantity: Some(10.0), market_value: 100.0, perc_net_assets: Some(0.5), fund_id: 1, acquisition_cost: Some(0.0) }
        }
    }

    impl InvestmentRowArgs {
        fn build(&self) -> Result<InvestmentRow, SchemaError> {
            InvestmentRow::new(
                self.id,
                self.report_page,
                "R".into(),
                "F".into(),
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

    #[test]
    fn investment_row_accepts_fully_populated_valid_values() {
        assert!(valid_investment(|_| {}).is_ok());
    }

    #[test]
    fn investment_row_accepts_all_nullable_fields_absent() {
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
    fn investment_row_rejects_non_positive_id(bad_id: i64) {
        let err = valid_investment(|a| a.id = bad_id).unwrap_err();
        assert_eq!(err, SchemaError::NotGreaterThan { field: "ID", value: bad_id as f64, bound: 0.0 });
    }

    #[test_case(0; "zero")]
    #[test_case(-1; "negative")]
    fn investment_row_rejects_non_positive_report_page(bad: i32) {
        let err = valid_investment(|a| a.report_page = bad).unwrap_err();
        assert_eq!(err, SchemaError::NotGreaterThan { field: "Report page", value: bad as f64, bound: 0.0 });
    }

    #[test_case(0.0; "zero")]
    #[test_case(-5.0; "negative")]
    fn investment_row_rejects_non_positive_nominal_quantity_when_present(bad: f32) {
        let err = valid_investment(|a| a.nominal_quantity = Some(bad)).unwrap_err();
        assert_eq!(err, SchemaError::NotGreaterThan { field: "Nominal/Quantity", value: bad as f64, bound: 0.0 });
    }

    #[test_case(0.0; "zero")]
    #[test_case(-1.0; "negative")]
    fn investment_row_rejects_non_positive_market_value(bad: f32) {
        let err = valid_investment(|a| a.market_value = bad).unwrap_err();
        assert_eq!(err, SchemaError::NotGreaterThan { field: "Market value", value: bad as f64, bound: 0.0 });
    }

    #[test_case(0.0; "lower bound inclusive is fine")]
    #[test_case(1.0; "upper bound inclusive is fine")]
    fn investment_row_accepts_perc_net_assets_at_inclusive_bounds(ok: f32) {
        assert!(valid_investment(|a| a.perc_net_assets = Some(ok)).is_ok());
    }

    #[test_case(-0.0001; "just below zero")]
    #[test_case(1.0001; "just above one")]
    fn investment_row_rejects_perc_net_assets_outside_unit_range(bad: f32) {
        let err = valid_investment(|a| a.perc_net_assets = Some(bad)).unwrap_err();
        assert_eq!(err, SchemaError::OutOfRange { field: "% net assets", value: bad as f64, min: 0.0, max: 1.0 });
    }

    #[test_case(0; "zero")]
    #[test_case(-1; "negative")]
    fn investment_row_rejects_non_positive_fund_id(bad: i64) {
        let err = valid_investment(|a| a.fund_id = bad).unwrap_err();
        assert_eq!(err, SchemaError::NotGreaterThan { field: "Fund ID", value: bad as f64, bound: 0.0 });
    }

    #[test]
    fn investment_row_accepts_zero_acquisition_cost() {
        assert!(valid_investment(|a| a.acquisition_cost = Some(0.0)).is_ok());
    }

    #[test]
    fn investment_row_rejects_negative_acquisition_cost() {
        let err = valid_investment(|a| a.acquisition_cost = Some(-0.01)).unwrap_err();
        assert_eq!(err, SchemaError::NotGreaterOrEqual { field: "Acquisition cost", value: -0.01_f32 as f64, bound: 0.0 });
    }

    // --- BondAdditionalInfoRow ---

    #[test]
    fn bond_additional_info_accepts_no_maturity_and_no_interest_rate() {
        assert!(BondAdditionalInfoRow::new(None, None).is_ok());
    }

    #[test]
    fn bond_additional_info_accepts_a_maturity_date() {
        let row = BondAdditionalInfoRow::new(Some(date(2030, 6, 15)), None).unwrap();
        assert_eq!(row.maturity, Some(date(2030, 6, 15)));
    }

    #[test_case(0.0; "lower bound inclusive")]
    #[test_case(0.999; "just under upper bound")]
    fn bond_additional_info_accepts_interest_rate_in_half_open_range(ok: f64) {
        assert!(BondAdditionalInfoRow::new(None, Some(ok)).is_ok());
    }

    #[test]
    fn bond_additional_info_rejects_interest_rate_equal_to_one() {
        // Half-open [0,1): unlike `% net assets`, the upper bound itself is invalid.
        let err = BondAdditionalInfoRow::new(None, Some(1.0)).unwrap_err();
        assert_eq!(err, SchemaError::OutOfRange { field: "interest_rate", value: 1.0, min: 0.0, max: 1.0 });
    }

    #[test]
    fn bond_additional_info_rejects_negative_interest_rate() {
        assert!(BondAdditionalInfoRow::new(None, Some(-0.1)).is_err());
    }

    // --- FundRow: the one row type with genuinely optional report metadata ---

    #[test]
    fn fund_row_accepts_full_debug_info() {
        let row = FundRow::new(1, "Fund A".into(), Some(2), Some(3), Some("R".into()), Some("F".into())).unwrap();
        assert_eq!(row.report_page, Some(3));
    }

    #[test]
    fn fund_row_accepts_no_debug_info_at_all() {
        // Mirrors a Fund created only via e.g. the funds_change_name loop, never independently
        // seen as a page_results.funds entry.
        let row = FundRow::new(1, "Fund A".into(), None, None, None, None).unwrap();
        assert_eq!(row.report_page, None);
        assert_eq!(row.report, None);
        assert_eq!(row.format, None);
    }

    #[test]
    fn fund_row_rejects_non_positive_management_company_id_when_present() {
        assert!(FundRow::new(1, "F".into(), Some(0), None, None, None).is_err());
    }

    #[test]
    fn fund_row_rejects_non_positive_report_page_when_present() {
        assert!(FundRow::new(1, "F".into(), None, Some(0), None, None).is_err());
    }

    // --- FundChangeNameRow ---

    #[test_case(ChangeNameEventType::Renaming; "renaming")]
    #[test_case(ChangeNameEventType::Merging; "merging")]
    fn fund_change_name_row_accepts_both_event_types(event_type: ChangeNameEventType) {
        let row = FundChangeNameRow::new(1, 1, "R".into(), "F".into(), 1, date(2024, 1, 1), event_type, "Old".into()).unwrap();
        assert_eq!(row.event_type, event_type);
    }

    #[test]
    fn fund_change_name_row_rejects_non_positive_fund_id() {
        assert!(FundChangeNameRow::new(1, 1, "R".into(), "F".into(), 0, date(2024, 1, 1), ChangeNameEventType::Renaming, "Old".into()).is_err());
    }

    // --- FundAssetsRow ---

    #[test]
    fn fund_assets_row_accepts_null_date() {
        let row = FundAssetsRow::new(1, 1, "R".into(), "F".into(), 1, None, 100.0, 50.0, 50.0, Currency::EUR).unwrap();
        assert_eq!(row.date, None);
    }

    #[test_case("total_assets", 0.0; "assets zero")]
    #[test_case("total_liabilities", 0.0; "liabilities zero")]
    #[test_case("total_net_assets", 0.0; "net assets zero")]
    fn fund_assets_row_rejects_non_positive_amounts(field: &str, bad: f32) {
        let (assets, liabilities, net) = match field {
            "total_assets" => (bad, 50.0, 50.0),
            "total_liabilities" => (100.0, bad, 50.0),
            _ => (100.0, 50.0, bad),
        };
        assert!(FundAssetsRow::new(1, 1, "R".into(), "F".into(), 1, None, assets, liabilities, net, Currency::EUR).is_err());
    }

    // --- FundSfdrClassificationRow ---

    #[test_case(SfdrArticle::ART_6; "article 6")]
    #[test_case(SfdrArticle::ART_8; "article 8")]
    #[test_case(SfdrArticle::ART_9; "article 9")]
    fn fund_sfdr_classification_row_accepts_every_article(article: SfdrArticle) {
        let row = FundSfdrClassificationRow::new(1, article, 1, "R".into(), "F".into()).unwrap();
        assert_eq!(row.sfdr_classification, article);
    }

    #[test]
    fn fund_sfdr_classification_row_rejects_non_positive_report_page() {
        assert!(FundSfdrClassificationRow::new(1, SfdrArticle::ART_6, 0, "R".into(), "F".into()).is_err());
    }

    // --- FundEsgIndicatorRow ---

    #[test]
    fn fund_esg_indicator_row_allows_arbitrary_indicator_and_value_strings() {
        let row = FundEsgIndicatorRow::new(1, "GHG intensity".into(), "12.3".into(), 1, "R".into(), "F".into()).unwrap();
        assert_eq!(row.indicator, "GHG intensity");
    }

    #[test]
    fn fund_esg_indicator_row_rejects_non_positive_fund_id() {
        assert!(FundEsgIndicatorRow::new(0, "I".into(), "V".into(), 1, "R".into(), "F".into()).is_err());
    }

    // --- AssetsManagerRow ---

    #[test]
    fn assets_manager_row_accepts_valid_values() {
        assert!(AssetsManagerRow::new(1, 1, "R".into(), "F".into(), "BlackRock".into()).is_ok());
    }

    #[test]
    fn assets_manager_row_rejects_non_positive_id() {
        assert!(AssetsManagerRow::new(0, 1, "R".into(), "F".into(), "BlackRock".into()).is_err());
    }

    // --- InvestmentsManagerRow ---

    #[test]
    fn investments_manager_row_accepts_valid_ids() {
        assert!(InvestmentsManagerRow::new(1, 2).is_ok());
    }

    #[test_case(0, 1; "manager id zero")]
    #[test_case(1, 0; "fund id zero")]
    fn investments_manager_row_rejects_either_id_non_positive(manager_id: i64, fund_id: i64) {
        assert!(InvestmentsManagerRow::new(manager_id, fund_id).is_err());
    }

    // --- UniqueTable: dedup semantics + a reasonable stress test ---

    #[test]
    fn unique_table_accepts_first_occurrence_of_each_key() {
        let mut t: UniqueTable<u32> = UniqueTable::new("t", "k");
        t.push("a", 1).unwrap();
        t.push("b", 2).unwrap();
        assert_eq!(t.rows(), &[1, 2]);
    }

    #[test]
    fn unique_table_rejects_a_repeated_key_and_does_not_append_it() {
        let mut t: UniqueTable<u32> = UniqueTable::new("investments", "ID");
        t.push("1", 1).unwrap();
        let err = t.push("1", 99).unwrap_err();
        assert_eq!(err, SchemaError::Duplicate { table: "investments", field: "ID", value: "1".into() });
        assert_eq!(t.len(), 1);
        assert_eq!(t.rows(), &[1]);
    }

    #[test]
    fn unique_table_starts_empty() {
        let t: UniqueTable<u32> = UniqueTable::new("t", "k");
        assert!(t.is_empty());
    }

    #[test]
    fn unique_table_combo_key_treats_differing_components_as_distinct() {
        // Mirrors funds_assets_schema's unique=["Fund ID", "Date"]: same fund, different date,
        // is not a duplicate; identical (fund, date) pair is.
        let mut t: UniqueTable<(u32, &str)> = UniqueTable::new("funds_assets", "Fund ID|Date");
        t.push("1|2024-01-01", (1, "2024-01-01")).unwrap();
        t.push("1|2024-02-01", (1, "2024-02-01")).unwrap();
        t.push("2|2024-01-01", (2, "2024-01-01")).unwrap();
        assert_eq!(t.len(), 3);
        assert!(t.push("1|2024-01-01", (1, "2024-01-01")).is_err());
    }

    #[test]
    fn unique_table_stress_10k_unique_keys_all_accepted_then_every_key_rejected_on_replay() {
        let mut t: UniqueTable<u32> = UniqueTable::new("stress", "ID");
        for i in 0..10_000u32 {
            t.push(i.to_string(), i).unwrap();
        }
        assert_eq!(t.len(), 10_000);
        // Replaying the exact same 10k keys must reject every single one, not just the first.
        let mut rejected = 0;
        for i in 0..10_000u32 {
            if t.push(i.to_string(), i).is_err() {
                rejected += 1;
            }
        }
        assert_eq!(rejected, 10_000);
        assert_eq!(t.len(), 10_000);
    }

    #[test]
    fn schema_error_display_messages_are_human_readable() {
        assert_eq!(
            SchemaError::NotGreaterThan { field: "ID", value: 0.0, bound: 0.0 }.to_string(),
            "ID must be greater than 0, got 0"
        );
        assert_eq!(
            SchemaError::OutOfRange { field: "% net assets", value: 1.5, min: 0.0, max: 1.0 }.to_string(),
            "% net assets must be in range [0, 1], got 1.5"
        );
        assert_eq!(
            SchemaError::Duplicate { table: "investments", field: "ID", value: "1".into() }.to_string(),
            "investments: duplicate ID value `1`"
        );
    }
}
