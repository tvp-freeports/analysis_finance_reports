//! Writing the accumulated tables to disk: one CSV per table, plus the YAML side file of bond
//! details.
//!
//! # Three structure profiles
//!
//! **`Regular`** — the default. Creates the output directory, including intermediate ones, and
//! writes one CSV per table plus `investments_add_infos.yaml`.
//!
//! **`SingleFile`** — the output path is treated as a **file**, not a directory: one CSV with the
//! investments columns plus `Maturity` and `Interest rate`, read from the side table by investment
//! id. Only `investments` is written in this profile; no other table.
//!
//! **`Structured`** — a directory holding `investments/table.csv` and `investments/dicts.yaml`.
//! Only `investments`, the same limitation as `SingleFile`.
//!
//! # Rules that hold across profiles
//!
//! - **every CSV always has its header**, even with zero rows: an empty table produces a header-only file, not an empty one. A consumer must be able to tell "no rows" from "no file";
//! - the headers are exact, in text and in order;
//! - an absent optional field is an **empty cell**, never the string `"None"` or `"null"`, and a floating-point number always carries at least one decimal.
//!
//! # Compression
//!
//! For the directory profiles the output is written and then archived into a `.tar.gz` **sibling**
//! of the directory, not inside it; for `SingleFile` into a `.gz` sibling, there being no directory
//! to archive. In both cases, whether the uncompressed output is removed afterwards depends on
//! whether it existed on disk **before** the call — checked before writing anything, so a directory
//! the user already had is never deleted.
//!
//! # Splitting the output per report
//!
//! With `separate_out`, the two tables carrying a report per row — `investments` and
//! `funds_assets` — are split by distinct report, one CSV per report, named
//! `{table}__{report}.csv`. The other tables are unaffected and stay merged. The default is off, so
//! the ordinary behaviour is one file per table.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::Write as _;
use std::path::Path;

use serde::Serialize;

use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
use crate::commons::date::Date;
use crate::output::files_schema::{
    AssetsManagerRow, BondAdditionalInfoRow, ChangeNameEventType, FundAssetsRow, FundChangeNameRow,
    FundEsgIndicatorRow, FundRow, FundSfdrClassificationRow, InvestmentRow, InvestmentsManagerRow,
};

use super::accumulate::TransformedTables;

/// The structure profile of the output files. See the module documentation for what each writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutStructureMode {
    Regular,
    SingleFile,
    Structured,
}

/// Additional flags on writing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OutFlags {
    pub compressed: bool,
    /// One CSV per report instead of one merged file, for the tables that carry one.
    /// See the module documentation.
    pub separate_out: bool,
}

/// Failures of writing the output files.
#[derive(Debug, thiserror::Error)]
pub enum WriteFilesError {
    #[error("cannot {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot write CSV {path}: {source}")]
    Csv {
        path: String,
        #[source]
        source: csv::Error,
    },
    #[error("cannot write YAML {path}: {source}")]
    Yaml {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
}

fn io_err(action: &'static str, path: &Path, source: std::io::Error) -> WriteFilesError {
    WriteFilesError::Io { action, path: path.display().to_string(), source }
}

fn csv_err(path: &Path, source: csv::Error) -> WriteFilesError {
    WriteFilesError::Csv { path: path.display().to_string(), source }
}

/// Writes `rows` as CSV, with the header **always present** — even for zero rows, unlike the
/// default behaviour, which writes the header on the first row and therefore never for an empty
/// table.
///
/// Opens the write span: this is the common primitive for every output CSV, and so the right place
/// to nest it under the span the caller already opened.
fn write_csv_table<T: Serialize>(path: &Path, header: &[&str], rows: &[T]) -> Result<(), WriteFilesError> {
    let span = tracing::info_span!("write", file = %path.display());
    let _guard = span.enter();

    let mut wtr =
        csv::WriterBuilder::new().has_headers(false).from_path(path).map_err(|e| csv_err(path, e))?;
    wtr.write_record(header).map_err(|e| csv_err(path, e))?;
    for row in rows {
        wtr.serialize(row).map_err(|e| csv_err(path, e))?;
    }
    wtr.flush().map_err(|e| io_err("flush", path, e))?;
    tracing::info!(rows = rows.len(), "file written");
    Ok(())
}

fn sfdr_label(article: SfdrArticle) -> &'static str {
    match article {
        SfdrArticle::Art6 => "Art. 6",
        SfdrArticle::Art8 => "Art. 8",
        SfdrArticle::Art9 => "Art. 9",
    }
}

fn event_type_label(event_type: ChangeNameEventType) -> &'static str {
    match event_type {
        ChangeNameEventType::Renaming => "RENAMING",
        ChangeNameEventType::Merging => "MERGING",
    }
}

#[derive(Serialize)]
struct InvestmentCsvRow {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Report")]
    report: String,
    #[serde(rename = "Report page")]
    report_page: u16,
    #[serde(rename = "Triggering text")]
    triggering_text: String,
    #[serde(rename = "Investee")]
    investee: String,
    #[serde(rename = "Financial instrument")]
    financial_instrument: FinancialInstrument,
    #[serde(rename = "Nominal/Quantity")]
    nominal_quantity: Option<f32>,
    #[serde(rename = "Market value")]
    market_value: f32,
    #[serde(rename = "Currency")]
    currency: Currency,
    #[serde(rename = "% net assets")]
    perc_net_assets: Option<f32>,
    #[serde(rename = "Fund ID")]
    fund_id: u32,
    #[serde(rename = "Acquisition cost")]
    acquisition_cost: Option<f32>,
    #[serde(rename = "Acquisition currency")]
    acquisition_currency: Option<Currency>,
}

impl From<&InvestmentRow> for InvestmentCsvRow {
    fn from(r: &InvestmentRow) -> Self {
        Self {
            id: r.id,
            report: r.report.clone(),
            report_page: r.report_page,
            triggering_text: r.triggering_text.clone(),
            investee: r.investee.clone(),
            financial_instrument: r.financial_instrument,
            nominal_quantity: r.nominal_quantity,
            market_value: r.market_value,
            currency: r.currency,
            perc_net_assets: r.perc_net_assets,
            fund_id: r.fund_id,
            acquisition_cost: r.acquisition_cost,
            acquisition_currency: r.acquisition_currency,
        }
    }
}

#[derive(Serialize)]
struct FundAssetsCsvRow {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Report")]
    report: String,
    #[serde(rename = "Report page")]
    report_page: u16,
    #[serde(rename = "Fund ID")]
    fund_id: u32,
    #[serde(rename = "Date")]
    date: Option<Date>,
    #[serde(rename = "Total assets")]
    total_assets: f32,
    #[serde(rename = "Total liabilities")]
    total_liabilities: f32,
    #[serde(rename = "Total net assets")]
    total_net_assets: f32,
    #[serde(rename = "Currency")]
    currency: Currency,
}

impl From<&FundAssetsRow> for FundAssetsCsvRow {
    fn from(r: &FundAssetsRow) -> Self {
        Self {
            id: r.id,
            report: r.report.clone(),
            report_page: r.report_page,
            fund_id: r.fund_id,
            date: r.date,
            total_assets: r.total_assets,
            total_liabilities: r.total_liabilities,
            total_net_assets: r.total_net_assets,
            currency: r.currency,
        }
    }
}

#[derive(Serialize)]
struct FundCsvRow {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Report")]
    report: Option<String>,
    #[serde(rename = "Report page")]
    report_page: Option<u16>,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Managment company ID")]
    management_company_id: Option<u32>,
}

impl From<&FundRow> for FundCsvRow {
    fn from(r: &FundRow) -> Self {
        Self {
            id: r.id,
            report: r.report.clone(),
            report_page: r.report_page,
            name: r.name.clone(),
            management_company_id: r.management_company_id,
        }
    }
}

#[derive(Serialize)]
struct FundSfdrClassificationCsvRow {
    #[serde(rename = "Fund ID")]
    fund_id: u32,
    #[serde(rename = "SFDR classification")]
    sfdr_classification: &'static str,
    #[serde(rename = "Report page")]
    report_page: u16,
    #[serde(rename = "Report")]
    report: String,
}

impl From<&FundSfdrClassificationRow> for FundSfdrClassificationCsvRow {
    fn from(r: &FundSfdrClassificationRow) -> Self {
        Self {
            fund_id: r.fund_id,
            sfdr_classification: sfdr_label(r.sfdr_classification),
            report_page: r.report_page,
            report: r.report.clone(),
        }
    }
}

#[derive(Serialize)]
struct FundEsgIndicatorCsvRow {
    #[serde(rename = "Fund ID")]
    fund_id: u32,
    #[serde(rename = "Indicator")]
    indicator: String,
    #[serde(rename = "Value")]
    value: String,
    #[serde(rename = "Report page")]
    report_page: u16,
    #[serde(rename = "Report")]
    report: String,
}

impl From<&FundEsgIndicatorRow> for FundEsgIndicatorCsvRow {
    fn from(r: &FundEsgIndicatorRow) -> Self {
        Self {
            fund_id: r.fund_id,
            indicator: r.indicator.clone(),
            value: r.value.clone(),
            report_page: r.report_page,
            report: r.report.clone(),
        }
    }
}

#[derive(Serialize)]
struct AssetsManagerCsvRow {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Report")]
    report: String,
    #[serde(rename = "Report page")]
    report_page: u16,
    #[serde(rename = "Name")]
    name: String,
}

impl From<&AssetsManagerRow> for AssetsManagerCsvRow {
    fn from(r: &AssetsManagerRow) -> Self {
        Self { id: r.id, report: r.report.clone(), report_page: r.report_page, name: r.name.clone() }
    }
}

#[derive(Serialize)]
struct InvestmentsManagerCsvRow {
    #[serde(rename = "Investment manager ID")]
    investment_manager_id: u32,
    #[serde(rename = "Fund ID")]
    fund_id: u32,
}

impl From<&InvestmentsManagerRow> for InvestmentsManagerCsvRow {
    fn from(r: &InvestmentsManagerRow) -> Self {
        Self { investment_manager_id: r.investment_manager_id, fund_id: r.fund_id }
    }
}

#[derive(Serialize)]
struct FundChangeNameCsvRow {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Report")]
    report: String,
    #[serde(rename = "Report page")]
    report_page: u16,
    #[serde(rename = "Fund ID")]
    fund_id: u32,
    #[serde(rename = "From")]
    from: Date,
    #[serde(rename = "Type of event")]
    event_type: &'static str,
    #[serde(rename = "Old name")]
    old_name: String,
}

impl From<&FundChangeNameRow> for FundChangeNameCsvRow {
    fn from(r: &FundChangeNameRow) -> Self {
        Self {
            id: r.id,
            report: r.report.clone(),
            report_page: r.report_page,
            fund_id: r.fund_id,
            from: r.from_date,
            event_type: event_type_label(r.event_type),
            old_name: r.old_name.clone(),
        }
    }
}

const INVESTMENTS_HEADER: [&str; 13] = [
    "ID", "Report", "Report page", "Triggering text", "Investee", "Financial instrument",
    "Nominal/Quantity", "Market value", "Currency", "% net assets", "Fund ID", "Acquisition cost",
    "Acquisition currency",
];

const FUNDS_ASSETS_HEADER: [&str; 9] = [
    "ID", "Report", "Report page", "Fund ID", "Date", "Total assets", "Total liabilities",
    "Total net assets", "Currency",
];

fn write_investments_csv(path: &Path, rows: &[InvestmentRow]) -> Result<(), WriteFilesError> {
    let rows: Vec<InvestmentCsvRow> = rows.iter().map(InvestmentCsvRow::from).collect();
    write_csv_table(path, &INVESTMENTS_HEADER, &rows)
}

fn write_funds_assets_csv(path: &Path, rows: &[FundAssetsRow]) -> Result<(), WriteFilesError> {
    let rows: Vec<FundAssetsCsvRow> = rows.iter().map(FundAssetsCsvRow::from).collect();
    write_csv_table(path, &FUNDS_ASSETS_HEADER, &rows)
}

/// Groups rows by report, preserving the order in which each report is first met.
fn split_by_report<'a, T>(rows: &'a [T], report: impl Fn(&T) -> &str) -> Vec<(String, Vec<&'a T>)> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<&'a T>> = HashMap::new();
    for row in rows {
        let key = report(row).to_string();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(row);
    }
    order
        .into_iter()
        .map(|key| {
            let rows = groups.remove(&key).expect("key was just inserted into `groups` above, in the same loop");
            (key, rows)
        })
        .collect()
}

fn write_investments_csv_separate(out_dir: &Path, rows: &[InvestmentRow]) -> Result<(), WriteFilesError> {
    let groups = split_by_report(rows, |r| r.report.as_str());
    tracing::debug!(groups = groups.len(), "splitting investments by report for separate_out");
    for (report, group) in groups {
        let path = out_dir.join(format!("investments__{report}.csv"));
        let csv_rows: Vec<InvestmentCsvRow> = group.into_iter().map(InvestmentCsvRow::from).collect();
        write_csv_table(&path, &INVESTMENTS_HEADER, &csv_rows)?;
    }
    Ok(())
}

fn write_funds_assets_csv_separate(out_dir: &Path, rows: &[FundAssetsRow]) -> Result<(), WriteFilesError> {
    let groups = split_by_report(rows, |r| r.report.as_str());
    tracing::debug!(groups = groups.len(), "splitting funds_assets by report for separate_out");
    for (report, group) in groups {
        let path = out_dir.join(format!("funds_assets__{report}.csv"));
        let csv_rows: Vec<FundAssetsCsvRow> = group.into_iter().map(FundAssetsCsvRow::from).collect();
        write_csv_table(&path, &FUNDS_ASSETS_HEADER, &csv_rows)?;
    }
    Ok(())
}

fn write_funds_csv(path: &Path, rows: &[FundRow]) -> Result<(), WriteFilesError> {
    let header = ["ID", "Report", "Report page", "Name", "Managment company ID"];
    let rows: Vec<FundCsvRow> = rows.iter().map(FundCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_funds_sfdr_classification_csv(path: &Path, rows: &[FundSfdrClassificationRow]) -> Result<(), WriteFilesError> {
    let header = ["Fund ID", "SFDR classification", "Report page", "Report"];
    let rows: Vec<FundSfdrClassificationCsvRow> = rows.iter().map(FundSfdrClassificationCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_funds_esg_indicators_csv(path: &Path, rows: &[FundEsgIndicatorRow]) -> Result<(), WriteFilesError> {
    let header = ["Fund ID", "Indicator", "Value", "Report page", "Report"];
    let rows: Vec<FundEsgIndicatorCsvRow> = rows.iter().map(FundEsgIndicatorCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_assets_managers_csv(path: &Path, rows: &[AssetsManagerRow]) -> Result<(), WriteFilesError> {
    let header = ["ID", "Report", "Report page", "Name"];
    let rows: Vec<AssetsManagerCsvRow> = rows.iter().map(AssetsManagerCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_investments_managers_csv(path: &Path, rows: &[InvestmentsManagerRow]) -> Result<(), WriteFilesError> {
    let header = ["Investment manager ID", "Fund ID"];
    let rows: Vec<InvestmentsManagerCsvRow> = rows.iter().map(InvestmentsManagerCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_funds_change_name_csv(path: &Path, rows: &[FundChangeNameRow]) -> Result<(), WriteFilesError> {
    let header = ["ID", "Report", "Report page", "Fund ID", "From", "Type of event", "Old name"];
    let rows: Vec<FundChangeNameCsvRow> = rows.iter().map(FundChangeNameCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

/// Writes the bond details YAML.
///
/// The YAML is built by hand rather than serialized, for one reason: a maturity date must come out
/// **quoted**. An unquoted `YYYY-MM-DD` scalar is a *timestamp* under the YAML version most readers
/// implement, so on re-reading, that field would come back as a date object instead of a string.
/// Consumers of this file expect a string — a difference invisible to the eye but not to whoever
/// reads it back.
///
/// The rest of the document is a map of maps of two scalars, so writing it by hand gives up
/// nothing: no field can contain a character needing quoting or a nested structure.
fn write_additional_infos_yaml(path: &Path, infos: &BTreeMap<u32, BondAdditionalInfoRow>) -> Result<(), WriteFilesError> {
    let span = tracing::info_span!("write", file = %path.display());
    let _guard = span.enter();

    if infos.is_empty() {
        std::fs::write(path, "{}\n").map_err(|e| io_err("write", path, e))?;
        tracing::info!(entries = 0, "file written");
        return Ok(());
    }
    let mut yaml = String::new();
    for (id, row) in infos {
        yaml.push_str(&format!("{id}:\n"));
        match &row.maturity {
            Some(date) => yaml.push_str(&format!("  maturity: '{date}'\n")),
            None => yaml.push_str("  maturity: null\n"),
        }
        match row.interest_rate {
            Some(rate) => yaml.push_str(&format!("  interest_rate: {rate}\n")),
            None => yaml.push_str("  interest_rate: null\n"),
        }
    }
    std::fs::write(path, yaml).map_err(|e| io_err("write", path, e))?;
    tracing::info!(entries = infos.len(), "file written");
    Ok(())
}

fn write_regular(tables: &TransformedTables, out_dir: &Path, separate_out: bool) -> Result<(), WriteFilesError> {
    std::fs::create_dir_all(out_dir).map_err(|e| io_err("create directory", out_dir, e))?;

    if separate_out {
        write_investments_csv_separate(out_dir, &tables.investments)?;
        write_funds_assets_csv_separate(out_dir, &tables.funds_assets)?;
    } else {
        write_investments_csv(&out_dir.join("investments.csv"), &tables.investments)?;
        write_funds_assets_csv(&out_dir.join("funds_assets.csv"), &tables.funds_assets)?;
    }
    write_funds_csv(&out_dir.join("funds.csv"), &tables.funds)?;
    write_funds_sfdr_classification_csv(
        &out_dir.join("funds_sfdr_classification.csv"),
        &tables.funds_sfdr_classification,
    )?;
    write_funds_esg_indicators_csv(&out_dir.join("funds_esg_indicators.csv"), &tables.funds_esg_indicators)?;
    write_assets_managers_csv(&out_dir.join("assets_managers.csv"), &tables.assets_managers)?;
    write_investments_managers_csv(
        &out_dir.join("investments_managers_to_funds.csv"),
        &tables.investments_managers,
    )?;
    write_funds_change_name_csv(&out_dir.join("funds_change_name.csv"), &tables.funds_change_name)?;
    write_additional_infos_yaml(&out_dir.join("investments_add_infos.yaml"), &tables.additional_infos)?;

    Ok(())
}

#[derive(Serialize)]
struct InvestmentSingleFileCsvRow {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Report")]
    report: String,
    #[serde(rename = "Report page")]
    report_page: u16,
    #[serde(rename = "Triggering text")]
    triggering_text: String,
    #[serde(rename = "Investee")]
    investee: String,
    #[serde(rename = "Financial instrument")]
    financial_instrument: FinancialInstrument,
    #[serde(rename = "Nominal/Quantity")]
    nominal_quantity: Option<f32>,
    #[serde(rename = "Market value")]
    market_value: f32,
    #[serde(rename = "Currency")]
    currency: Currency,
    #[serde(rename = "% net assets")]
    perc_net_assets: Option<f32>,
    #[serde(rename = "Fund ID")]
    fund_id: u32,
    #[serde(rename = "Acquisition cost")]
    acquisition_cost: Option<f32>,
    #[serde(rename = "Acquisition currency")]
    acquisition_currency: Option<Currency>,
    #[serde(rename = "Maturity")]
    maturity: Option<Date>,
    #[serde(rename = "Interest rate")]
    interest_rate: Option<f64>,
}

/// The single-file profile: the investments table enriched with the bond columns from the side
/// table, in one CSV. Only `investments` is written.
fn write_single_file(tables: &TransformedTables, out_path: &Path) -> Result<(), WriteFilesError> {
    let header = [
        "ID", "Report", "Report page", "Triggering text", "Investee", "Financial instrument",
        "Nominal/Quantity", "Market value", "Currency", "% net assets", "Fund ID", "Acquisition cost",
        "Acquisition currency", "Maturity", "Interest rate",
    ];
    let rows: Vec<InvestmentSingleFileCsvRow> = tables
        .investments
        .iter()
        .map(|r| {
            let additional = tables.additional_infos.get(&r.id);
            InvestmentSingleFileCsvRow {
                id: r.id,
                report: r.report.clone(),
                report_page: r.report_page,
                triggering_text: r.triggering_text.clone(),
                investee: r.investee.clone(),
                financial_instrument: r.financial_instrument,
                nominal_quantity: r.nominal_quantity,
                market_value: r.market_value,
                currency: r.currency,
                perc_net_assets: r.perc_net_assets,
                fund_id: r.fund_id,
                acquisition_cost: r.acquisition_cost,
                acquisition_currency: r.acquisition_currency,
                maturity: additional.and_then(|a| a.maturity),
                interest_rate: additional.and_then(|a| a.interest_rate),
            }
        })
        .collect();
    write_csv_table(out_path, &header, &rows)
}

/// The structured profile: an `investments` directory holding a table and its side file, with
/// the same columns as the regular profile. **Only `investments`**, the same limitation as the
/// single-file profile.
fn write_structured(tables: &TransformedTables, out_dir: &Path) -> Result<(), WriteFilesError> {
    std::fs::create_dir_all(out_dir).map_err(|e| io_err("create directory", out_dir, e))?;
    let sub = out_dir.join("investments");
    std::fs::create_dir_all(&sub).map_err(|e| io_err("create directory", &sub, e))?;
    write_investments_csv(&sub.join("table.csv"), &tables.investments)?;
    write_additional_infos_yaml(&sub.join("dicts.yaml"), &tables.additional_infos)
}

/// The file name of `path`, or an empty string when it has none or is not valid UTF-8 — an edge
/// case absorbed silently, so it is logged here before proceeding with a degraded archive name.
fn file_name_or_warn(path: &Path) -> &str {
    match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => {
            tracing::warn!(path = %path.display(), "path has no valid UTF-8 file name; using an empty archive name");
            ""
        }
    }
}

/// Compresses a single file into a `.gz` sibling — not a `.tar.gz`, there being no directory to
/// archive.
fn compress_single_file(path: &Path) -> Result<(), WriteFilesError> {
    let archive_name = format!("{}.gz", file_name_or_warn(path));
    let archive_path = path.with_file_name(archive_name);

    let span = tracing::info_span!("write", file = %archive_path.display());
    let _guard = span.enter();

    let mut input = File::open(path).map_err(|e| io_err("open", path, e))?;
    let output = File::create(&archive_path).map_err(|e| io_err("create", &archive_path, e))?;
    let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
    std::io::copy(&mut input, &mut encoder).map_err(|e| io_err("gzip", path, e))?;
    encoder.finish().map_err(|e| io_err("finish gzip", path, e))?;
    tracing::info!("file written");
    Ok(())
}

/// Compresses a directory into a `.tar.gz` **sibling** of it, not inside it.
fn compress_directory(dir: &Path) -> Result<(), WriteFilesError> {
    let dir_name = file_name_or_warn(dir);
    let archive_path = dir.with_file_name(format!("{dir_name}.tar.gz"));

    let span = tracing::info_span!("write", file = %archive_path.display());
    let _guard = span.enter();

    let output = File::create(&archive_path).map_err(|e| io_err("create", &archive_path, e))?;
    let encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.append_dir_all(dir_name, dir).map_err(|e| io_err("tar", dir, e))?;
    builder
        .into_inner()
        .and_then(|mut e| e.flush())
        .map_err(|e| io_err("finish tar.gz", &archive_path, e))?;
    tracing::info!("file written");
    Ok(())
}

/// Writes `tables` to disk according to the profile and flags. See the module documentation for
/// what each combination produces.
pub fn write_files(
    tables: &TransformedTables,
    out_dir: &Path,
    profile: OutStructureMode,
    flags: OutFlags,
) -> Result<(), WriteFilesError> {
    tracing::debug!(profile = ?profile, flags = ?flags, "resolved output profile and flags");
    let remove_uncompressed = !out_dir.exists();

    match profile {
        OutStructureMode::Regular => write_regular(tables, out_dir, flags.separate_out)?,
        OutStructureMode::SingleFile => write_single_file(tables, out_dir)?,
        OutStructureMode::Structured => write_structured(tables, out_dir)?,
    }

    if flags.compressed {
        if profile == OutStructureMode::SingleFile {
            compress_single_file(out_dir)?;
            if remove_uncompressed {
                std::fs::remove_file(out_dir).map_err(|e| io_err("remove", out_dir, e))?;
                tracing::info!(path = %out_dir.display(), "removed uncompressed output after compression");
            }
        } else {
            compress_directory(out_dir)?;
            if remove_uncompressed {
                std::fs::remove_dir_all(out_dir).map_err(|e| io_err("remove", out_dir, e))?;
                tracing::info!(path = %out_dir.display(), "removed uncompressed output after compression");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_tables() -> TransformedTables {
        TransformedTables {
            investments: vec![],
            funds: vec![],
            funds_change_name: vec![],
            funds_assets: vec![],
            funds_sfdr_classification: vec![],
            funds_esg_indicators: vec![],
            assets_managers: vec![],
            investments_managers: vec![],
            additional_infos: BTreeMap::new(),
        }
    }

    fn read(dir: &Path, name: &str) -> String {
        std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("cannot read {name}: {e}"))
    }

    mod regular_profile_basics {
        use super::*;

        #[test]
        fn creates_the_output_directory_and_every_expected_file() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&empty_tables(), &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            for name in [
                "investments.csv",
                "funds_assets.csv",
                "funds.csv",
                "funds_sfdr_classification.csv",
                "funds_esg_indicators.csv",
                "assets_managers.csv",
                "investments_managers_to_funds.csv",
                "funds_change_name.csv",
                "investments_add_infos.yaml",
            ] {
                assert!(out.join(name).is_file(), "missing {name}");
            }
        }

        #[test]
        fn an_empty_table_still_produces_a_header_only_csv() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&empty_tables(), &out, OutStructureMode::Regular, OutFlags::default()).unwrap();
            assert_eq!(
                read(&out, "investments.csv"),
                "ID,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency\n"
            );
        }

        #[test]
        fn out_flags_default_has_separate_out_false() {
            // An additive field, defaulting to off so that existing behaviour — one merged CSV per
            // table — is unchanged.
            assert!(!OutFlags::default().separate_out);
        }
    }

    mod investments_csv {
        use super::*;

        #[test]
        fn writes_two_rows_byte_for_byte() {
            let mut tables = empty_tables();
            tables.investments = vec![
                InvestmentRow::new(
                    1, 3, "Report A".into(), "Acme".into(), "Acme Corp".into(), FinancialInstrument::EQUITY,
                    None, 1000.0, Currency::EUR, None, 1, None, None,
                )
                .unwrap(),
                InvestmentRow::new(
                    2, 4, "Report A".into(), "Bond Co".into(), "Bond Corp".into(), FinancialInstrument::BOND,
                    Some(10.0), 2000.5, Currency::USD, Some(0.25), 2, Some(50.0), Some(Currency::GBP),
                )
                .unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency\n\
                 1,Report A,3,Acme,Acme Corp,EQUITY,,1000.0,EUR,,1,,\n\
                 2,Report A,4,Bond Co,Bond Corp,BOND,10.0,2000.5,USD,0.25,2,50.0,GBP\n";
            assert_eq!(read(&out, "investments.csv"), expected);
        }
    }

    mod funds_csv {
        use super::*;

        #[test]
        fn writes_both_a_directly_seen_fund_and_an_indirectly_seen_one() {
            let mut tables = empty_tables();
            tables.funds = vec![
                FundRow::new(1, "ALPHA FUND".into(), Some(2), Some(3), Some("Report A".into())).unwrap(),
                FundRow::new(2, "BETA FUND".into(), None, None, None).unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Report,Report page,Name,Managment company ID\n\
                 1,Report A,3,ALPHA FUND,2\n\
                 2,,,BETA FUND,\n";
            assert_eq!(read(&out, "funds.csv"), expected);
        }
    }

    mod funds_sfdr_classification_csv {
        use super::*;

        #[test]
        fn every_article_is_rendered_with_its_reference_style_label() {
            let mut tables = empty_tables();
            tables.funds_sfdr_classification = vec![
                FundSfdrClassificationRow::new(1, SfdrArticle::Art6, 1, "R".into()).unwrap(),
                FundSfdrClassificationRow::new(2, SfdrArticle::Art8, 2, "R".into()).unwrap(),
                FundSfdrClassificationRow::new(3, SfdrArticle::Art9, 3, "R".into()).unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "Fund ID,SFDR classification,Report page,Report\n\
                 1,Art. 6,1,R\n\
                 2,Art. 8,2,R\n\
                 3,Art. 9,3,R\n";
            assert_eq!(read(&out, "funds_sfdr_classification.csv"), expected);
        }
    }

    mod funds_esg_indicators_csv {
        use super::*;

        #[test]
        fn writes_indicator_rows_byte_for_byte() {
            let mut tables = empty_tables();
            tables.funds_esg_indicators =
                vec![FundEsgIndicatorRow::new(1, "GHG intensity".into(), "12.3".into(), 5, "R".into()).unwrap()];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "Fund ID,Indicator,Value,Report page,Report\n1,GHG intensity,12.3,5,R\n";
            assert_eq!(read(&out, "funds_esg_indicators.csv"), expected);
        }
    }

    mod assets_managers_csv {
        use super::*;

        #[test]
        fn writes_manager_rows_byte_for_byte() {
            let mut tables = empty_tables();
            tables.assets_managers = vec![AssetsManagerRow::new(1, 2, "R".into(), "Acme AM".into()).unwrap()];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Report,Report page,Name\n1,R,2,Acme AM\n";
            assert_eq!(read(&out, "assets_managers.csv"), expected);
        }
    }

    mod investments_managers_csv {
        use super::*;

        #[test]
        fn writes_the_two_fk_columns_only() {
            let mut tables = empty_tables();
            tables.investments_managers = vec![InvestmentsManagerRow::new(1, 2).unwrap(), InvestmentsManagerRow::new(1, 3).unwrap()];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "Investment manager ID,Fund ID\n1,2\n1,3\n";
            assert_eq!(read(&out, "investments_managers_to_funds.csv"), expected);
        }
    }

    mod funds_change_name_csv {
        use super::*;

        #[test]
        fn renders_both_event_types_with_their_reference_style_label() {
            let mut tables = empty_tables();
            tables.funds_change_name = vec![
                FundChangeNameRow::new(
                    1, 1, "R".into(), 1, Date::new(2024, 1, 1).unwrap(), ChangeNameEventType::Renaming,
                    "Old Name".into(),
                )
                .unwrap(),
                FundChangeNameRow::new(
                    2, 2, "R".into(), 2, Date::new(2024, 2, 2).unwrap(), ChangeNameEventType::Merging,
                    "Other Old Name".into(),
                )
                .unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Report,Report page,Fund ID,From,Type of event,Old name\n\
                 1,R,1,1,2024-01-01,RENAMING,Old Name\n\
                 2,R,2,2,2024-02-02,MERGING,Other Old Name\n";
            assert_eq!(read(&out, "funds_change_name.csv"), expected);
        }
    }

    mod funds_assets_csv {
        use super::*;

        #[test]
        fn writes_a_row_with_and_a_row_without_a_date() {
            let mut tables = empty_tables();
            tables.funds_assets = vec![
                FundAssetsRow::new(
                    1, 1, "R".into(), 1, Some(Date::new(2024, 12, 31).unwrap()), 100.0, 40.0, 60.0, Currency::EUR,
                )
                .unwrap(),
                FundAssetsRow::new(2, 2, "R".into(), 2, None, 200.0, 80.0, 120.0, Currency::USD).unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Report,Report page,Fund ID,Date,Total assets,Total liabilities,Total net assets,Currency\n\
                 1,R,1,1,2024-12-31,100.0,40.0,60.0,EUR\n\
                 2,R,2,2,,200.0,80.0,120.0,USD\n";
            assert_eq!(read(&out, "funds_assets.csv"), expected);
        }
    }

    mod investments_add_infos_yaml {
        use super::*;

        #[test]
        fn an_empty_map_is_the_empty_yaml_mapping() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&empty_tables(), &out, OutStructureMode::Regular, OutFlags::default()).unwrap();
            assert_eq!(read(&out, "investments_add_infos.yaml"), "{}\n");
        }

        #[test]
        fn a_single_entry_with_both_fields_present() {
            let mut tables = empty_tables();
            tables.additional_infos.insert(
                1,
                BondAdditionalInfoRow::new(Some(Date::new(2028, 3, 30).unwrap()), Some(0.035)).unwrap(),
            );
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();
            assert_eq!(read(&out, "investments_add_infos.yaml"), "1:\n  maturity: '2028-03-30'\n  interest_rate: 0.035\n");
        }

        #[test]
        fn several_entries_keep_ascending_id_order_with_absent_fields_as_null() {
            let mut tables = empty_tables();
            tables.additional_infos.insert(2, BondAdditionalInfoRow::new(None, None).unwrap());
            tables
                .additional_infos
                .insert(1, BondAdditionalInfoRow::new(Some(Date::new(2030, 6, 15).unwrap()), None).unwrap());
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();
            assert_eq!(
                read(&out, "investments_add_infos.yaml"),
                "1:\n  maturity: '2030-06-15'\n  interest_rate: null\n2:\n  maturity: null\n  interest_rate: null\n"
            );
        }
    }

    /// A minimal investments row with a parameterisable report, so the tests do not repeat
    /// thirteen positional arguments per row.
    fn investment(id: i64, report: &str) -> InvestmentRow {
        InvestmentRow::new(
            id, 1, report.to_string(), "Trigger".into(), "Investee".into(), FinancialInstrument::EQUITY,
            None, 1000.0, Currency::EUR, None, 1, None, None,
        )
        .unwrap()
    }

    /// The single-file profile.
    mod single_file_profile {
        use super::*;

        #[test]
        fn writes_a_single_csv_file_not_a_directory() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&tables, &out, OutStructureMode::SingleFile, OutFlags::default()).unwrap();
            assert!(out.is_file());
        }

        #[test]
        fn appends_maturity_and_interest_rate_columns_from_additional_infos() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R")];
            tables
                .additional_infos
                .insert(1, BondAdditionalInfoRow::new(Some(Date::new(2028, 3, 30).unwrap()), Some(0.035)).unwrap());

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&tables, &out, OutStructureMode::SingleFile, OutFlags::default()).unwrap();

            let content = std::fs::read_to_string(&out).unwrap();
            let header = content.lines().next().unwrap();
            assert_eq!(
                header,
                "ID,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency,Maturity,Interest rate"
            );
            let row = content.lines().nth(1).unwrap();
            assert!(row.ends_with("2028-03-30,0.035"), "expected Maturity/Interest rate at the end, got: {row}");
        }

        #[test]
        fn an_investment_with_no_matching_additional_info_gets_empty_maturity_and_interest_rate() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&tables, &out, OutStructureMode::SingleFile, OutFlags::default()).unwrap();
            let content = std::fs::read_to_string(&out).unwrap();
            let row = content.lines().nth(1).unwrap();
            assert!(row.ends_with(",,"), "expected two trailing empty cells, got: {row}");
        }

        #[test]
        fn only_the_investments_table_is_written_no_other_table_files_appear_alongside_it() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R")];
            tables.funds = vec![FundRow::new(1, "ALPHA FUND".into(), None, None, None).unwrap()];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&tables, &out, OutStructureMode::SingleFile, OutFlags::default()).unwrap();
            assert!(!dir.path().join("funds.csv").exists());
            assert!(!dir.path().join("investments_add_infos.yaml").exists());
        }

        #[test]
        fn an_empty_investments_table_still_writes_a_header_only_file() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&empty_tables(), &out, OutStructureMode::SingleFile, OutFlags::default()).unwrap();
            let content = std::fs::read_to_string(&out).unwrap();
            assert_eq!(content.lines().count(), 1, "header only, no data rows");
        }
    }

    /// The structured profile.
    mod structured_profile {
        use super::*;

        #[test]
        fn creates_an_investments_subdirectory_with_table_csv_and_dicts_yaml() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R")];
            tables.additional_infos.insert(1, BondAdditionalInfoRow::new(None, Some(0.05)).unwrap());

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Structured, OutFlags::default()).unwrap();

            assert!(out.join("investments").join("table.csv").is_file());
            assert!(out.join("investments").join("dicts.yaml").is_file());
        }

        #[test]
        fn table_csv_has_the_same_columns_as_the_regular_investments_csv_no_extra_columns() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Structured, OutFlags::default()).unwrap();
            let content = std::fs::read_to_string(out.join("investments").join("table.csv")).unwrap();
            let header = content.lines().next().unwrap();
            assert_eq!(
                header,
                "ID,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency"
            );
        }

        #[test]
        fn dicts_yaml_uses_the_same_format_as_investments_add_infos_yaml() {
            let mut tables = empty_tables();
            tables.additional_infos.insert(1, BondAdditionalInfoRow::new(Some(Date::new(2028, 3, 30).unwrap()), Some(0.035)).unwrap());
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Structured, OutFlags::default()).unwrap();
            let content = std::fs::read_to_string(out.join("investments").join("dicts.yaml")).unwrap();
            assert_eq!(content, "1:\n  maturity: '2028-03-30'\n  interest_rate: 0.035\n");
        }

        #[test]
        fn only_the_investments_table_is_written_no_funds_csv_at_the_top_level() {
            let mut tables = empty_tables();
            tables.funds = vec![FundRow::new(1, "ALPHA FUND".into(), None, None, None).unwrap()];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Structured, OutFlags::default()).unwrap();
            assert!(!out.join("funds.csv").exists());
        }
    }

    /// Compression, of a single file and of a directory.
    mod compression {
        use super::*;
        use std::io::Read;

        fn gunzip(path: &Path) -> Vec<u8> {
            let file = std::fs::File::open(path).unwrap();
            let mut decoder = flate2::read::GzDecoder::new(file);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out).unwrap();
            out
        }

        #[test]
        fn regular_profile_compressed_produces_a_sibling_tar_gz() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&empty_tables(), &out, OutStructureMode::Regular, OutFlags { compressed: true, ..OutFlags::default() })
                .unwrap();
            assert!(dir.path().join("out.tar.gz").is_file());
        }

        #[test]
        fn regular_profile_compressed_removes_the_uncompressed_directory_when_it_did_not_preexist() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out"); // does not exist yet
            write_files(&empty_tables(), &out, OutStructureMode::Regular, OutFlags { compressed: true, ..OutFlags::default() })
                .unwrap();
            assert!(!out.exists(), "the uncompressed directory must be removed since it did not preexist");
            assert!(dir.path().join("out.tar.gz").is_file());
        }

        #[test]
        fn regular_profile_compressed_keeps_the_uncompressed_directory_when_it_preexisted() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            std::fs::create_dir_all(&out).unwrap(); // preexists before write_files runs
            write_files(&empty_tables(), &out, OutStructureMode::Regular, OutFlags { compressed: true, ..OutFlags::default() })
                .unwrap();
            assert!(out.exists(), "a directory that preexisted must be kept");
            assert!(dir.path().join("out.tar.gz").is_file());
        }

        #[test]
        fn structured_profile_compressed_also_produces_a_sibling_tar_gz() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&empty_tables(), &out, OutStructureMode::Structured, OutFlags { compressed: true, ..OutFlags::default() })
                .unwrap();
            assert!(dir.path().join("out.tar.gz").is_file());
        }

        #[test]
        fn single_file_profile_compressed_produces_a_sibling_gz_not_tar_gz() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&empty_tables(), &out, OutStructureMode::SingleFile, OutFlags { compressed: true, ..OutFlags::default() })
                .unwrap();
            assert!(dir.path().join("out.csv.gz").is_file());
            assert!(!dir.path().join("out.csv.tar.gz").exists());
        }

        #[test]
        fn single_file_profile_compressed_removes_the_uncompressed_file_when_it_did_not_preexist() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&empty_tables(), &out, OutStructureMode::SingleFile, OutFlags { compressed: true, ..OutFlags::default() })
                .unwrap();
            assert!(!out.exists());
        }

        #[test]
        fn single_file_profile_compressed_content_gunzips_back_to_the_original_csv() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&tables, &out, OutStructureMode::SingleFile, OutFlags { compressed: true, ..OutFlags::default() })
                .unwrap();
            let content = String::from_utf8(gunzip(&dir.path().join("out.csv.gz"))).unwrap();
            assert!(content.starts_with("ID,Report,"));
            assert!(content.contains(",R,"));
        }

        #[test]
        fn regular_profile_tar_gz_content_extracts_back_to_the_same_csv_files() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags { compressed: true, ..OutFlags::default() })
                .unwrap();

            let archive_bytes = gunzip(&dir.path().join("out.tar.gz"));
            let mut archive = tar::Archive::new(&archive_bytes[..]);
            let extract_to = tempfile::tempdir().unwrap();
            archive.unpack(extract_to.path()).unwrap();
            let content = std::fs::read_to_string(extract_to.path().join("out").join("investments.csv")).unwrap();
            assert!(content.contains(",R,"));
        }
    }

    /// The per-report split.
    ///
    /// The file-name format and the subset of tables involved are a decision made here rather than
    /// a requirement read off elsewhere: the tables split are `investments` and `funds_assets`, and
    /// the name is `{table}__{report}.csv`.
    mod separate_out_flag {
        use super::*;

        #[test]
        fn default_out_flags_keeps_the_single_merged_investments_csv() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "Report A"), investment(2, "Report B")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();
            assert!(out.join("investments.csv").is_file());
            let content = std::fs::read_to_string(out.join("investments.csv")).unwrap();
            assert_eq!(content.lines().count(), 3, "header + two merged rows");
        }

        #[test]
        fn separate_out_splits_investments_by_report_instead_of_merging() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "Report A"), investment(2, "Report B")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let flags = OutFlags { separate_out: true, ..OutFlags::default() };
            write_files(&tables, &out, OutStructureMode::Regular, flags).unwrap();

            assert!(!out.join("investments.csv").exists(), "the merged file must not be produced");
            assert!(out.join("investments__Report A.csv").is_file());
            assert!(out.join("investments__Report B.csv").is_file());
        }

        #[test]
        fn each_split_file_contains_only_its_own_report_rows_with_the_full_header() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "Report A"), investment(2, "Report B")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let flags = OutFlags { separate_out: true, ..OutFlags::default() };
            write_files(&tables, &out, OutStructureMode::Regular, flags).unwrap();

            let content_a = std::fs::read_to_string(out.join("investments__Report A.csv")).unwrap();
            assert!(content_a.starts_with("ID,Report,Report page"));
            assert_eq!(content_a.lines().count(), 2, "header + exactly one row for Report A");
            assert!(content_a.contains(",Report A,"));
            assert!(!content_a.contains(",Report B,"));
        }

        #[test]
        fn separate_out_also_splits_funds_assets() {
            let mut tables = empty_tables();
            tables.funds_assets = vec![
                FundAssetsRow::new(1, 1, "Report A".into(), 1, None, 100.0, 40.0, 60.0, Currency::EUR).unwrap(),
                FundAssetsRow::new(2, 2, "Report B".into(), 2, None, 100.0, 40.0, 60.0, Currency::EUR).unwrap(),
            ];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let flags = OutFlags { separate_out: true, ..OutFlags::default() };
            write_files(&tables, &out, OutStructureMode::Regular, flags).unwrap();

            assert!(!out.join("funds_assets.csv").exists());
            assert!(out.join("funds_assets__Report A.csv").is_file());
            assert!(out.join("funds_assets__Report B.csv").is_file());
        }

        #[test]
        fn separate_out_does_not_affect_tables_outside_its_documented_scope() {
            // `funds`/`funds_sfdr_classification`/`funds_esg_indicators`/`assets_managers`/
            // `investments_managers_to_funds`/`funds_change_name` and the yaml file stay merged
            // as usual -- only `investments`/`funds_assets` are split (see the module doc's
            // judgment-call note).
            let mut tables = empty_tables();
            tables.funds = vec![
                FundRow::new(1, "ALPHA FUND".into(), None, Some(1), Some("Report A".into())).unwrap(),
                FundRow::new(2, "BETA FUND".into(), None, Some(2), Some("Report B".into())).unwrap(),
            ];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let flags = OutFlags { separate_out: true, ..OutFlags::default() };
            write_files(&tables, &out, OutStructureMode::Regular, flags).unwrap();

            assert!(out.join("funds.csv").is_file(), "funds.csv must remain a single merged file");
            let content = std::fs::read_to_string(out.join("funds.csv")).unwrap();
            assert_eq!(content.lines().count(), 3, "header + both funds merged together");
        }

        #[test]
        fn an_empty_investments_table_with_separate_out_produces_no_split_files_at_all() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let flags = OutFlags { separate_out: true, ..OutFlags::default() };
            write_files(&empty_tables(), &out, OutStructureMode::Regular, flags).unwrap();
            assert!(!out.join("investments.csv").exists());
            let split_files: Vec<_> = std::fs::read_dir(&out)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with("investments__"))
                .collect();
            assert!(split_files.is_empty(), "no reports at all -> nothing to split");
        }
    }
}
