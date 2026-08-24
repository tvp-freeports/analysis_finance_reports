//! Scrittura di [`super::accumulate::TransformedTables`] su disco: un CSV per tabella più il file
//! YAML delle informazioni aggiuntive sulle obbligazioni.
//!
//! M8, passo 13 (`agent-memory/M8-implementation-plan.md` §0 Q1.2/§1/§3). **Solo il profilo
//! `Regular`** è implementato in questa milestone (una directory, un CSV per tabella, crate
//! `csv`) — `SingleFile`/`Structured` e la compressione (`OutFlags`) restano a M9, quando esiste
//! davvero un flag da riga di comando che li seleziona. `OutStructureMode`/`OutFlags` sono
//! definiti **qui**, non in `cli::conf_parse` (ancora uno stub): è `output` che li possiede,
//! `cli` li riuserà quando esisterà (`PLAN.md` §13, decisione Q1.1).
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub enum OutStructureMode { Regular, SingleFile, Structured }
//!
//! #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
//! pub struct OutFlags { pub compressed: bool }
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum WriteFilesError {
//!     Io { .. },                                    // fallimento di I/O (creare dir, aprire file, ...)
//!     Csv { .. },                                    // fallimento della serializzazione CSV
//!     Yaml { .. },                                   // fallimento della serializzazione YAML
//!     UnsupportedProfile { mode: OutStructureMode },  // SingleFile/Structured, deferiti a M9
//!     CompressionNotSupported,                        // OutFlags::compressed, deferito a M9
//! }
//!
//! pub fn write_files(
//!     tables: &TransformedTables,
//!     out_dir: &std::path::Path,
//!     profile: OutStructureMode,
//!     flags: OutFlags,
//! ) -> Result<(), WriteFilesError>;
//! ```
//!
//! **Regole pinnate dai test** (`agent-memory/M8-implementation-plan.md` §4, "Scrittura CSV"):
//!
//! - Profilo `Regular`: crea `out_dir` (comprese le directory intermedie) e scrive, in
//!   quest'ordine di nome file, un CSV per tabella — `investments.csv`, `funds_assets.csv`,
//!   `funds.csv`, `funds_sfdr_classification.csv`, `funds_esg_indicators.csv`,
//!   `assets_managers.csv`, `investments_managers_to_funds.csv`, `funds_change_name.csv` — più
//!   `investments_add_infos.yaml`.
//! - **Ogni CSV ha sempre l'intestazione**, anche con zero righe: una tabella vuota produce un
//!   file di sola intestazione, non un file vuoto — stesso comportamento del riferimento
//!   (DataFrame vuoto ma con colonne definite).
//! - Intestazioni **esatte** (testo e ordine), pinnate dai test sotto — portate dal riferimento
//!   (`packages/freeports_core/src/output/routines.rs`, le funzioni `*_df`), unica differenza
//!   `"SFDR classification"`/`"Type of event"` che qui sono stringhe calcolate (non il
//!   `Serialize` di `SfdrArticle`/`ChangeNameEventType`) — `"Art. 6"`/`"Art. 8"`/`"Art. 9"` e
//!   `"RENAMING"`/`"MERGING"`.
//! - Un campo `Option<_>` assente è una **cella vuota**, mai la stringa `"None"`/`"null"`; un
//!   numero in virgola mobile porta sempre almeno un decimale (`100.0`, non `100`) — è il
//!   comportamento naturale di `csv`+`serde` su `f32`/`f64`, verificato empiricamente prima di
//!   scrivere questi test.
//! - `investments_add_infos.yaml` usa **`serde_yaml`** direttamente su
//!   `TransformedTables::additional_infos` (un `BTreeMap<u32, BondAdditionalInfoRow>`, che deriva
//!   `Serialize` — vedi `files_schema.rs`), non una riproduzione a mano del formato PyYAML: non è
//!   più un requisito di fedeltà in questa fase (`files_schema`/`routines` non sono moduli
//!   verbatim, `PLAN.md` §0). Una mappa vuota produce `"{}\n"`.
//! - `flags.compressed` e i profili diversi da `Regular` sono errori **espliciti**
//!   ([`WriteFilesError::CompressionNotSupported`]/[`WriteFilesError::UnsupportedProfile`]), non
//!   un'implementazione silenziosamente incompleta: nessun file/directory viene toccato quando
//!   `write_files` fallisce così.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
use crate::commons::date::Date;
use crate::output::files_schema::{
    AssetsManagerRow, BondAdditionalInfoRow, ChangeNameEventType, FundAssetsRow, FundChangeNameRow,
    FundEsgIndicatorRow, FundRow, FundSfdrClassificationRow, InvestmentRow, InvestmentsManagerRow,
};

use super::accumulate::TransformedTables;

/// Il profilo di struttura dei file di output. **Solo `Regular` è implementato** in questa
/// milestone (`PLAN.md` §13, decisione Q1.2): `SingleFile`/`Structured` restano a M9, quando
/// esiste davvero un flag da riga di comando che li seleziona.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutStructureMode {
    Regular,
    SingleFile,
    Structured,
}

/// Flag aggiuntivi sulla scrittura. **Solo `compressed: false` è supportato** in questa
/// milestone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutFlags {
    pub compressed: bool,
}

/// Fallimenti della scrittura dei file di output.
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
    /// `SingleFile`/`Structured` restano a M9.
    #[error("output profile {mode:?} is not supported yet")]
    UnsupportedProfile { mode: OutStructureMode },
    /// `OutFlags::compressed` resta a M9.
    #[error("compressed output is not supported yet")]
    CompressionNotSupported,
}

fn io_err(action: &'static str, path: &Path, source: std::io::Error) -> WriteFilesError {
    WriteFilesError::Io { action, path: path.display().to_string(), source }
}

fn csv_err(path: &Path, source: csv::Error) -> WriteFilesError {
    WriteFilesError::Csv { path: path.display().to_string(), source }
}

/// Scrive `rows` come CSV in `path`, con l'intestazione `header` **sempre presente** — anche a
/// zero righe, a differenza del comportamento di default del crate `csv` (che scrive
/// l'intestazione solo alla prima `serialize`, quindi mai se non c'e' nessuna riga).
fn write_csv_table<T: Serialize>(path: &Path, header: &[&str], rows: &[T]) -> Result<(), WriteFilesError> {
    let mut wtr =
        csv::WriterBuilder::new().has_headers(false).from_path(path).map_err(|e| csv_err(path, e))?;
    wtr.write_record(header).map_err(|e| csv_err(path, e))?;
    for row in rows {
        wtr.serialize(row).map_err(|e| csv_err(path, e))?;
    }
    wtr.flush().map_err(|e| io_err("flush", path, e))
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
    #[serde(rename = "Format")]
    format: String,
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
            format: r.format.clone(),
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
    #[serde(rename = "Format")]
    format: String,
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
            format: r.format.clone(),
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
    #[serde(rename = "Format")]
    format: Option<String>,
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
            format: r.format.clone(),
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
    #[serde(rename = "Format")]
    format: String,
    #[serde(rename = "Report")]
    report: String,
}

impl From<&FundSfdrClassificationRow> for FundSfdrClassificationCsvRow {
    fn from(r: &FundSfdrClassificationRow) -> Self {
        Self {
            fund_id: r.fund_id,
            sfdr_classification: sfdr_label(r.sfdr_classification),
            report_page: r.report_page,
            format: r.format.clone(),
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
    #[serde(rename = "Format")]
    format: String,
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
            format: r.format.clone(),
            report: r.report.clone(),
        }
    }
}

#[derive(Serialize)]
struct AssetsManagerCsvRow {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Format")]
    format: String,
    #[serde(rename = "Report")]
    report: String,
    #[serde(rename = "Report page")]
    report_page: u16,
    #[serde(rename = "Name")]
    name: String,
}

impl From<&AssetsManagerRow> for AssetsManagerCsvRow {
    fn from(r: &AssetsManagerRow) -> Self {
        Self { id: r.id, format: r.format.clone(), report: r.report.clone(), report_page: r.report_page, name: r.name.clone() }
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
    #[serde(rename = "Format")]
    format: String,
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
            format: r.format.clone(),
            report: r.report.clone(),
            report_page: r.report_page,
            fund_id: r.fund_id,
            from: r.from_date,
            event_type: event_type_label(r.event_type),
            old_name: r.old_name.clone(),
        }
    }
}

fn write_investments_csv(path: &Path, rows: &[InvestmentRow]) -> Result<(), WriteFilesError> {
    let header = [
        "ID", "Format", "Report", "Report page", "Triggering text", "Investee", "Financial instrument",
        "Nominal/Quantity", "Market value", "Currency", "% net assets", "Fund ID", "Acquisition cost",
        "Acquisition currency",
    ];
    let rows: Vec<InvestmentCsvRow> = rows.iter().map(InvestmentCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_funds_assets_csv(path: &Path, rows: &[FundAssetsRow]) -> Result<(), WriteFilesError> {
    let header = [
        "ID", "Format", "Report", "Report page", "Fund ID", "Date", "Total assets", "Total liabilities",
        "Total net assets", "Currency",
    ];
    let rows: Vec<FundAssetsCsvRow> = rows.iter().map(FundAssetsCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_funds_csv(path: &Path, rows: &[FundRow]) -> Result<(), WriteFilesError> {
    let header = ["ID", "Format", "Report", "Report page", "Name", "Managment company ID"];
    let rows: Vec<FundCsvRow> = rows.iter().map(FundCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_funds_sfdr_classification_csv(path: &Path, rows: &[FundSfdrClassificationRow]) -> Result<(), WriteFilesError> {
    let header = ["Fund ID", "SFDR classification", "Report page", "Format", "Report"];
    let rows: Vec<FundSfdrClassificationCsvRow> = rows.iter().map(FundSfdrClassificationCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_funds_esg_indicators_csv(path: &Path, rows: &[FundEsgIndicatorRow]) -> Result<(), WriteFilesError> {
    let header = ["Fund ID", "Indicator", "Value", "Report page", "Format", "Report"];
    let rows: Vec<FundEsgIndicatorCsvRow> = rows.iter().map(FundEsgIndicatorCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_assets_managers_csv(path: &Path, rows: &[AssetsManagerRow]) -> Result<(), WriteFilesError> {
    let header = ["ID", "Format", "Report", "Report page", "Name"];
    let rows: Vec<AssetsManagerCsvRow> = rows.iter().map(AssetsManagerCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_investments_managers_csv(path: &Path, rows: &[InvestmentsManagerRow]) -> Result<(), WriteFilesError> {
    let header = ["Investment manager ID", "Fund ID"];
    let rows: Vec<InvestmentsManagerCsvRow> = rows.iter().map(InvestmentsManagerCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_funds_change_name_csv(path: &Path, rows: &[FundChangeNameRow]) -> Result<(), WriteFilesError> {
    let header = ["ID", "Format", "Report", "Report page", "Fund ID", "From", "Type of event", "Old name"];
    let rows: Vec<FundChangeNameCsvRow> = rows.iter().map(FundChangeNameCsvRow::from).collect();
    write_csv_table(path, &header, &rows)
}

fn write_additional_infos_yaml(path: &Path, infos: &BTreeMap<u32, BondAdditionalInfoRow>) -> Result<(), WriteFilesError> {
    let yaml = serde_yaml::to_string(infos).map_err(|e| WriteFilesError::Yaml { path: path.display().to_string(), source: e })?;
    std::fs::write(path, yaml).map_err(|e| io_err("write", path, e))
}

/// Scrive `tables` su disco secondo `profile`/`flags`. **Solo `OutStructureMode::Regular` con
/// `OutFlags { compressed: false }`** è implementato in questa milestone (`PLAN.md` §13, Q1.2):
/// ogni altra combinazione è un errore esplicito, senza toccare il filesystem.
pub fn write_files(
    tables: &TransformedTables,
    out_dir: &Path,
    profile: OutStructureMode,
    flags: OutFlags,
) -> Result<(), WriteFilesError> {
    if flags.compressed {
        return Err(WriteFilesError::CompressionNotSupported);
    }
    if profile != OutStructureMode::Regular {
        return Err(WriteFilesError::UnsupportedProfile { mode: profile });
    }

    std::fs::create_dir_all(out_dir).map_err(|e| io_err("create directory", out_dir, e))?;

    write_investments_csv(&out_dir.join("investments.csv"), &tables.investments)?;
    write_funds_assets_csv(&out_dir.join("funds_assets.csv"), &tables.funds_assets)?;
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
                "ID,Format,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency\n"
            );
        }

        #[test]
        fn single_file_profile_is_not_supported_yet() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            let err =
                write_files(&empty_tables(), &out, OutStructureMode::SingleFile, OutFlags::default()).unwrap_err();
            assert!(matches!(err, WriteFilesError::UnsupportedProfile { mode: OutStructureMode::SingleFile }));
            assert!(!out.exists(), "no file should be created for an unsupported profile");
        }

        #[test]
        fn structured_profile_is_not_supported_yet() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let err =
                write_files(&empty_tables(), &out, OutStructureMode::Structured, OutFlags::default()).unwrap_err();
            assert!(matches!(err, WriteFilesError::UnsupportedProfile { mode: OutStructureMode::Structured }));
            assert!(!out.exists());
        }

        #[test]
        fn compression_is_not_supported_yet_regardless_of_profile() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let flags = OutFlags { compressed: true };
            let err = write_files(&empty_tables(), &out, OutStructureMode::Regular, flags).unwrap_err();
            assert!(matches!(err, WriteFilesError::CompressionNotSupported));
            assert!(!out.exists(), "nothing should be written when compression is requested but unsupported");
        }
    }

    mod investments_csv {
        use super::*;

        #[test]
        fn writes_two_rows_byte_for_byte() {
            let mut tables = empty_tables();
            tables.investments = vec![
                InvestmentRow::new(
                    1, 3, "Report A".into(), "FMT".into(), "Acme".into(), "Acme Corp".into(),
                    FinancialInstrument::EQUITY, None, 1000.0, Currency::EUR, None, 1, None, None,
                )
                .unwrap(),
                InvestmentRow::new(
                    2, 4, "Report A".into(), "FMT".into(), "Bond Co".into(), "Bond Corp".into(),
                    FinancialInstrument::BOND, Some(10.0), 2000.5, Currency::USD, Some(0.25), 2, Some(50.0),
                    Some(Currency::GBP),
                )
                .unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Format,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency\n\
                 1,FMT,Report A,3,Acme,Acme Corp,EQUITY,,1000.0,EUR,,1,,\n\
                 2,FMT,Report A,4,Bond Co,Bond Corp,BOND,10.0,2000.5,USD,0.25,2,50.0,GBP\n";
            assert_eq!(read(&out, "investments.csv"), expected);
        }
    }

    mod funds_csv {
        use super::*;

        #[test]
        fn writes_both_a_directly_seen_fund_and_an_indirectly_seen_one() {
            let mut tables = empty_tables();
            tables.funds = vec![
                FundRow::new(1, "ALPHA FUND".into(), Some(2), Some(3), Some("Report A".into()), Some("FMT".into()))
                    .unwrap(),
                FundRow::new(2, "BETA FUND".into(), None, None, None, None).unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Format,Report,Report page,Name,Managment company ID\n\
                 1,FMT,Report A,3,ALPHA FUND,2\n\
                 2,,,,BETA FUND,\n";
            assert_eq!(read(&out, "funds.csv"), expected);
        }
    }

    mod funds_sfdr_classification_csv {
        use super::*;

        #[test]
        fn every_article_is_rendered_with_its_reference_style_label() {
            let mut tables = empty_tables();
            tables.funds_sfdr_classification = vec![
                FundSfdrClassificationRow::new(1, SfdrArticle::Art6, 1, "R".into(), "F".into()).unwrap(),
                FundSfdrClassificationRow::new(2, SfdrArticle::Art8, 2, "R".into(), "F".into()).unwrap(),
                FundSfdrClassificationRow::new(3, SfdrArticle::Art9, 3, "R".into(), "F".into()).unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "Fund ID,SFDR classification,Report page,Format,Report\n\
                 1,Art. 6,1,F,R\n\
                 2,Art. 8,2,F,R\n\
                 3,Art. 9,3,F,R\n";
            assert_eq!(read(&out, "funds_sfdr_classification.csv"), expected);
        }
    }

    mod funds_esg_indicators_csv {
        use super::*;

        #[test]
        fn writes_indicator_rows_byte_for_byte() {
            let mut tables = empty_tables();
            tables.funds_esg_indicators =
                vec![FundEsgIndicatorRow::new(1, "GHG intensity".into(), "12.3".into(), 5, "R".into(), "F".into()).unwrap()];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "Fund ID,Indicator,Value,Report page,Format,Report\n1,GHG intensity,12.3,5,F,R\n";
            assert_eq!(read(&out, "funds_esg_indicators.csv"), expected);
        }
    }

    mod assets_managers_csv {
        use super::*;

        #[test]
        fn writes_manager_rows_byte_for_byte() {
            let mut tables = empty_tables();
            tables.assets_managers = vec![AssetsManagerRow::new(1, 2, "R".into(), "F".into(), "Acme AM".into()).unwrap()];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Format,Report,Report page,Name\n1,F,R,2,Acme AM\n";
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
                    1, 1, "R".into(), "F".into(), 1, Date::new(2024, 1, 1).unwrap(), ChangeNameEventType::Renaming,
                    "Old Name".into(),
                )
                .unwrap(),
                FundChangeNameRow::new(
                    2, 2, "R".into(), "F".into(), 2, Date::new(2024, 2, 2).unwrap(), ChangeNameEventType::Merging,
                    "Other Old Name".into(),
                )
                .unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Format,Report,Report page,Fund ID,From,Type of event,Old name\n\
                 1,F,R,1,1,2024-01-01,RENAMING,Old Name\n\
                 2,F,R,2,2,2024-02-02,MERGING,Other Old Name\n";
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
                    1, 1, "R".into(), "F".into(), 1, Some(Date::new(2024, 12, 31).unwrap()), 100.0, 40.0, 60.0,
                    Currency::EUR,
                )
                .unwrap(),
                FundAssetsRow::new(2, 2, "R".into(), "F".into(), 2, None, 200.0, 80.0, 120.0, Currency::USD).unwrap(),
            ];

            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();

            let expected = "ID,Format,Report,Report page,Fund ID,Date,Total assets,Total liabilities,Total net assets,Currency\n\
                 1,F,R,1,1,2024-12-31,100.0,40.0,60.0,EUR\n\
                 2,F,R,2,2,,200.0,80.0,120.0,USD\n";
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
            assert_eq!(read(&out, "investments_add_infos.yaml"), "1:\n  maturity: 2028-03-30\n  interest_rate: 0.035\n");
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
                "1:\n  maturity: 2030-06-15\n  interest_rate: null\n2:\n  maturity: null\n  interest_rate: null\n"
            );
        }
    }
}
