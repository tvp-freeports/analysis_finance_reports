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
//! # Estensione M9 (`M9-implementation-plan.md` §0 Q6, §3 passo 12) — **RIAPRE M8**
//!
//! Su autorizzazione esplicita dell'utente, stesso trattamento di `core::tracing_setup::Verbosity`
//! (§0 Q5): `SingleFile`/`Structured`/`OutFlags::compressed`, tutti e tre finora rifiutati con un
//! `WriteFilesError` tipizzato (`UnsupportedProfile`/`CompressionNotSupported`, M8 Q1.2), sono ora
//! **implementati per davvero** — porting diretto di `packages/freeports_core/src/output/
//! routines.rs::{write_single_file, write_structured, compress_single_file, compress_directory}`,
//! l'unico riferimento pulito per la forma esatta (letto per intero prima di scrivere questi
//! test, non inventato). `OutFlags` guadagna anche `separate_out: bool` (default `false`), nuovo
//! di questa milestone (non nel riferimento, che lo modellava diversamente — vedi sotto).
//! `WriteFilesError::UnsupportedProfile`/`CompressionNotSupported` **spariscono**: ogni
//! combinazione di `profile`/`flags` è ora gestita, quindi quei due rami sono diventati
//! irraggiungibili.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub enum OutStructureMode { Regular, SingleFile, Structured }
//!
//! #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
//! pub struct OutFlags { pub compressed: bool, pub separate_out: bool }
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum WriteFilesError {
//!     Io { .. },   // fallimento di I/O (creare dir, aprire file, comprimere, ...)
//!     Csv { .. },  // fallimento della serializzazione CSV
//!     Yaml { .. }, // fallimento della serializzazione YAML
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
//! **Regole pinnate dai test** (`agent-memory/M8-implementation-plan.md` §4, "Scrittura CSV" —
//! per il profilo `Regular` e i default, invariate a M9; nuove sotto per `SingleFile`/
//! `Structured`/`compressed`/`separate_out`):
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
//!
//! ## `SingleFile` (porting diretto di `write_single_file`)
//!
//! `out_dir` è trattato come **percorso di un file** (non una directory): un solo CSV con le
//! colonne di `investments.csv` più due colonne aggiuntive, `Maturity`/`Interest rate`, lette da
//! `TransformedTables::additional_infos` per `id` di ciascun investimento (assenti -> cella
//! vuota). **Solo `investments` è scritto** in questo profilo -- nessun'altra tabella (`funds`,
//! `funds_assets`, ...): comportamento verbatim del riferimento, non un'omissione di questo
//! porting.
//!
//! ## `Structured` (porting diretto di `write_structured`)
//!
//! Crea `out_dir/investments/table.csv` (stesse colonne di `investments.csv`) e
//! `out_dir/investments/dicts.yaml` (`additional_infos`, stesso formato di
//! `investments_add_infos.yaml`). **Solo `investments`**, stessa limitazione di `SingleFile` --
//! verbatim dal riferimento.
//!
//! ## `OutFlags::compressed` (porting diretto di `compress_single_file`/`compress_directory`)
//!
//! - `Regular`/`Structured` (una directory): l'output normale viene scritto, poi comprimo in un
//!   `.tar.gz` **sibling** (`out_dir.with_file_name("{nome}.tar.gz")`, non dentro `out_dir`
//!   stessa) con `tar`+`flate2`. Se `out_dir` **non esisteva già sul disco prima della chiamata**
//!   (`!out_dir.exists()`, controllato **prima** di scrivere qualunque file), la directory non
//!   compressa viene rimossa dopo la compressione; se esisteva già, viene lasciata intatta.
//! - `SingleFile` (un file): comprimo con `flate2::write::GzEncoder` in un `.gz` sibling (non
//!   `.tar.gz`, non c'è una directory da archiviare). Stessa regola sulla preesistenza per
//!   decidere se rimuovere il file non compresso.
//! - `set_compress_flag` (`cli::freeports_config`, validazione a monte) ha già strippato un
//!   eventuale suffisso `.tar.gz` da `OUT_PATH` prima che `write_files` lo veda: qui il suffisso
//!   viene sempre **aggiunto**, mai atteso già presente nell'`out_dir` ricevuto.
//!
//! ## `OutFlags::separate_out` (nuovo di M9, non nel riferimento in questa forma)
//!
//! `M9-implementation-plan.md` §0 Q6: "un CSV per `Report` (chiave `DocumentOutcome::id`/
//! `format`), non più `prefix_out`". Il riferimento (`reference_legacy/_internals/cli/main.py`)
//! separava per **formato** su un unico dataframe concatenato con `Report identifier`/`Format`
//! come colonne aggiunte a posteriori -- non traducibile direttamente in questa architettura
//! (`TransformedTables` è già multi-tabella tipizzata, ogni riga porta già `Report`/`Format`).
//! **Scelta del test-writer, segnalata come judgment call nel resoconto finale** (non pinnata da
//! nessuna sezione del piano con un formato di nome file esplicito): per il profilo `Regular` con
//! `separate_out: true`, ciascuna tabella che porta `Report`/`Format` per riga (qui: `investments`
//! e `funds_assets`, sottoinsieme scelto per contenere l'ambito della modifica -- non le altre sei)
//! viene **spezzata** per coppia `(Report, Format)` distinta, un CSV per coppia, nome
//! `{tabella}__{report}__{format}.csv` (es. `investments__Report A__FMT.csv`) al posto del singolo
//! `investments.csv`/`funds_assets.csv` merged. Le altre tabelle (`funds`, `funds_sfdr_classification`,
//! `funds_esg_indicators`, `assets_managers`, `investments_managers_to_funds`,
//! `funds_change_name`) e `investments_add_infos.yaml` **non sono affette**: restano scritte come
//! nel profilo `Regular` di default, non spezzate. `separate_out` con `SingleFile`/`Structured`
//! **non è testato qui** (interazione non specificata dal piano) -- vedi il resoconto.
//! `OutFlags::default()` (`separate_out: false`) **non cambia comportamento**: i test M8 esistenti
//! (tutti con `OutFlags::default()`) restano verdi senza modifiche.

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

/// Il profilo di struttura dei file di output. **Solo `Regular` è implementato** in questa
/// milestone (`PLAN.md` §13, decisione Q1.2): `SingleFile`/`Structured` restano a M9, quando
/// esiste davvero un flag da riga di comando che li seleziona.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutStructureMode {
    Regular,
    SingleFile,
    Structured,
}

/// Flag aggiuntivi sulla scrittura.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutFlags {
    pub compressed: bool,
    /// M9 (`M9-implementation-plan.md` §0 Q6, riapre M8): un CSV per `Report`/`Format` invece di
    /// uno unico, limitato a `investments`/`funds_assets` -- vedi il doc-comment del modulo.
    pub separate_out: bool,
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

const INVESTMENTS_HEADER: [&str; 14] = [
    "ID", "Format", "Report", "Report page", "Triggering text", "Investee", "Financial instrument",
    "Nominal/Quantity", "Market value", "Currency", "% net assets", "Fund ID", "Acquisition cost",
    "Acquisition currency",
];

const FUNDS_ASSETS_HEADER: [&str; 10] = [
    "ID", "Format", "Report", "Report page", "Fund ID", "Date", "Total assets", "Total liabilities",
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

/// Raggruppa `rows` per `(Report, Format)`, preservando l'ordine del primo incontro di ciascuna
/// coppia -- usato da `OutFlags::separate_out` (`M9-implementation-plan.md` §0 Q6) per scrivere un
/// CSV per coppia invece di una tabella unica.
fn split_by_report_and_format<'a, T>(
    rows: &'a [T],
    report: impl Fn(&T) -> &str,
    format: impl Fn(&T) -> &str,
) -> Vec<(String, String, Vec<&'a T>)> {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut groups: HashMap<(String, String), Vec<&'a T>> = HashMap::new();
    for row in rows {
        let key = (report(row).to_string(), format(row).to_string());
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(row);
    }
    order
        .into_iter()
        .map(|key| {
            let rows = groups.remove(&key).expect("key was just inserted into `groups` above, in the same loop");
            (key.0, key.1, rows)
        })
        .collect()
}

fn write_investments_csv_separate(out_dir: &Path, rows: &[InvestmentRow]) -> Result<(), WriteFilesError> {
    for (report, format, group) in split_by_report_and_format(rows, |r| r.report.as_str(), |r| r.format.as_str()) {
        let path = out_dir.join(format!("investments__{report}__{format}.csv"));
        let csv_rows: Vec<InvestmentCsvRow> = group.into_iter().map(InvestmentCsvRow::from).collect();
        write_csv_table(&path, &INVESTMENTS_HEADER, &csv_rows)?;
    }
    Ok(())
}

fn write_funds_assets_csv_separate(out_dir: &Path, rows: &[FundAssetsRow]) -> Result<(), WriteFilesError> {
    for (report, format, group) in split_by_report_and_format(rows, |r| r.report.as_str(), |r| r.format.as_str()) {
        let path = out_dir.join(format!("funds_assets__{report}__{format}.csv"));
        let csv_rows: Vec<FundAssetsCsvRow> = group.into_iter().map(FundAssetsCsvRow::from).collect();
        write_csv_table(&path, &FUNDS_ASSETS_HEADER, &csv_rows)?;
    }
    Ok(())
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

/// Scrive `investments_add_infos.yaml`.
///
/// Il YAML e' costruito a mano, e non con `serde_yaml::to_string`, per una ragione sola: la data
/// di scadenza deve uscire **fra apici** (`maturity: '2025-09-22'`). `serde_yaml` la emette come
/// scalare nudo, e uno scalare della forma `AAAA-MM-GG` e' un *timestamp* per la YAML 1.1 che
/// `yaml.safe_load` di PyYAML implementa: rileggendo il file, quel campo tornerebbe come
/// `datetime.date` invece che come stringa. Il riferimento (PyYAML in scrittura) lo quotava, e i
/// consumatori del file si aspettano una stringa -- una differenza invisibile a occhio ma non a
/// chi rilegge.
///
/// Il resto del documento e' banale (una mappa di mappe di due scalari), quindi la scrittura
/// manuale non rinuncia a niente: nessun campo puo' contenere caratteri da quotare o strutture
/// annidate.
fn write_additional_infos_yaml(path: &Path, infos: &BTreeMap<u32, BondAdditionalInfoRow>) -> Result<(), WriteFilesError> {
    if infos.is_empty() {
        return std::fs::write(path, "{}\n").map_err(|e| io_err("write", path, e));
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
    std::fs::write(path, yaml).map_err(|e| io_err("write", path, e))
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
    #[serde(rename = "Maturity")]
    maturity: Option<Date>,
    #[serde(rename = "Interest rate")]
    interest_rate: Option<f64>,
}

/// Porting diretto di `write_single_file` (`freeports_core/src/output/routines.rs`): investments
/// arricchito con `Maturity`/`Interest rate` da `additional_infos` per `id`, un solo CSV. **Solo
/// `investments` è scritto**, verbatim dal riferimento -- vedi il doc-comment del modulo.
fn write_single_file(tables: &TransformedTables, out_path: &Path) -> Result<(), WriteFilesError> {
    let header = [
        "ID", "Format", "Report", "Report page", "Triggering text", "Investee", "Financial instrument",
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
                maturity: additional.and_then(|a| a.maturity),
                interest_rate: additional.and_then(|a| a.interest_rate),
            }
        })
        .collect();
    write_csv_table(out_path, &header, &rows)
}

/// Porting diretto di `write_structured`: `out_dir/investments/table.csv` (stesse colonne del
/// profilo `Regular`) + `out_dir/investments/dicts.yaml`. **Solo `investments`**, stessa
/// limitazione di `SingleFile` -- verbatim dal riferimento.
fn write_structured(tables: &TransformedTables, out_dir: &Path) -> Result<(), WriteFilesError> {
    std::fs::create_dir_all(out_dir).map_err(|e| io_err("create directory", out_dir, e))?;
    let sub = out_dir.join("investments");
    std::fs::create_dir_all(&sub).map_err(|e| io_err("create directory", &sub, e))?;
    write_investments_csv(&sub.join("table.csv"), &tables.investments)?;
    write_additional_infos_yaml(&sub.join("dicts.yaml"), &tables.additional_infos)
}

/// Porting diretto di `compress_single_file`: `.gz` sibling di `path` (non `.tar.gz`, non c'è una
/// directory da archiviare).
fn compress_single_file(path: &Path) -> Result<(), WriteFilesError> {
    let archive_name = format!("{}.gz", path.file_name().and_then(|n| n.to_str()).unwrap_or_default());
    let archive_path = path.with_file_name(archive_name);
    let mut input = File::open(path).map_err(|e| io_err("open", path, e))?;
    let output = File::create(&archive_path).map_err(|e| io_err("create", &archive_path, e))?;
    let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
    std::io::copy(&mut input, &mut encoder).map_err(|e| io_err("gzip", path, e))?;
    encoder.finish().map_err(|e| io_err("finish gzip", path, e))?;
    Ok(())
}

/// Porting diretto di `compress_directory`: `.tar.gz` **sibling** di `dir` (non dentro `dir`
/// stessa).
fn compress_directory(dir: &Path) -> Result<(), WriteFilesError> {
    let archive_name = format!("{}.tar.gz", dir.file_name().and_then(|n| n.to_str()).unwrap_or_default());
    let archive_path = dir.with_file_name(archive_name);
    let output = File::create(&archive_path).map_err(|e| io_err("create", &archive_path, e))?;
    let encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let arcname = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    builder.append_dir_all(arcname, dir).map_err(|e| io_err("tar", dir, e))?;
    builder
        .into_inner()
        .and_then(|mut e| e.flush())
        .map_err(|e| io_err("finish tar.gz", &archive_path, e))?;
    Ok(())
}

/// Scrive `tables` su disco secondo `profile`/`flags`. Estensione M9 (`M9-implementation-plan.md`
/// §0 Q6): tutte e tre le combinazioni di `profile`, più `OutFlags::compressed`/`separate_out`,
/// sono ora implementate -- vedi il doc-comment del modulo.
pub fn write_files(
    tables: &TransformedTables,
    out_dir: &Path,
    profile: OutStructureMode,
    flags: OutFlags,
) -> Result<(), WriteFilesError> {
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
            }
        } else {
            compress_directory(out_dir)?;
            if remove_uncompressed {
                std::fs::remove_dir_all(out_dir).map_err(|e| io_err("remove", out_dir, e))?;
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
                "ID,Format,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency\n"
            );
        }

        #[test]
        fn out_flags_default_has_separate_out_false() {
            // M9 additive field (`M9-implementation-plan.md` §0 Q6): must default to `false` so
            // every pre-existing M8 test built with `OutFlags::default()` keeps its Regular-
            // profile, single-merged-CSV-per-table behavior unchanged.
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

    /// Costruisce un `InvestmentRow` minimo, con `report`/`format` parametrizzabili -- usato dai
    /// nuovi test M9 (`single_file_profile`, `structured_profile`, `separate_out_flag`) per
    /// evitare di ripetere i 14 argomenti posizionali di `InvestmentRow::new` ad ogni riga.
    fn investment(id: i64, report: &str, format: &str) -> InvestmentRow {
        InvestmentRow::new(
            id, 1, report.to_string(), format.to_string(), "Trigger".into(), "Investee".into(),
            FinancialInstrument::EQUITY, None, 1000.0, Currency::EUR, None, 1, None, None,
        )
        .unwrap()
    }

    /// M9 (`M9-implementation-plan.md` §0 Q6, riapre M8): `OutStructureMode::SingleFile`, porting
    /// diretto di `write_single_file` (`freeports_core/src/output/routines.rs`).
    mod single_file_profile {
        use super::*;

        #[test]
        fn writes_a_single_csv_file_not_a_directory() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R", "F")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&tables, &out, OutStructureMode::SingleFile, OutFlags::default()).unwrap();
            assert!(out.is_file());
        }

        #[test]
        fn appends_maturity_and_interest_rate_columns_from_additional_infos() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R", "F")];
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
                "ID,Format,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency,Maturity,Interest rate"
            );
            let row = content.lines().nth(1).unwrap();
            assert!(row.ends_with("2028-03-30,0.035"), "expected Maturity/Interest rate at the end, got: {row}");
        }

        #[test]
        fn an_investment_with_no_matching_additional_info_gets_empty_maturity_and_interest_rate() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R", "F")];
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
            tables.investments = vec![investment(1, "R", "F")];
            tables.funds = vec![FundRow::new(1, "ALPHA FUND".into(), None, None, None, None).unwrap()];
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

    /// M9: `OutStructureMode::Structured`, porting diretto di `write_structured`.
    mod structured_profile {
        use super::*;

        #[test]
        fn creates_an_investments_subdirectory_with_table_csv_and_dicts_yaml() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R", "F")];
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
            tables.investments = vec![investment(1, "R", "F")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Structured, OutFlags::default()).unwrap();
            let content = std::fs::read_to_string(out.join("investments").join("table.csv")).unwrap();
            let header = content.lines().next().unwrap();
            assert_eq!(
                header,
                "ID,Format,Report,Report page,Triggering text,Investee,Financial instrument,Nominal/Quantity,Market value,Currency,% net assets,Fund ID,Acquisition cost,Acquisition currency"
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
            tables.funds = vec![FundRow::new(1, "ALPHA FUND".into(), None, None, None, None).unwrap()];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Structured, OutFlags::default()).unwrap();
            assert!(!out.join("funds.csv").exists());
        }
    }

    /// M9: `OutFlags::compressed`, porting diretto di `compress_single_file`/`compress_directory`.
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
            tables.investments = vec![investment(1, "R", "F")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out.csv");
            write_files(&tables, &out, OutStructureMode::SingleFile, OutFlags { compressed: true, ..OutFlags::default() })
                .unwrap();
            let content = String::from_utf8(gunzip(&dir.path().join("out.csv.gz"))).unwrap();
            assert!(content.starts_with("ID,Format,Report"));
            assert!(content.contains(",R,"));
        }

        #[test]
        fn regular_profile_tar_gz_content_extracts_back_to_the_same_csv_files() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "R", "F")];
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

    /// M9 (`M9-implementation-plan.md` §0 Q6): `OutFlags::separate_out`. **Judgment call
    /// segnalato nel resoconto del test-writer**: il piano descrive solo "un CSV per Report" senza
    /// pinnare un formato di nome file o l'elenco esatto di tabelle coinvolte -- questi test
    /// fissano una proposta concreta (`investments`/`funds_assets`, nome
    /// `{tabella}__{report}__{format}.csv`), non una lettura univoca del piano.
    mod separate_out_flag {
        use super::*;

        #[test]
        fn default_out_flags_keeps_the_single_merged_investments_csv() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "Report A", "F"), investment(2, "Report B", "F")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            write_files(&tables, &out, OutStructureMode::Regular, OutFlags::default()).unwrap();
            assert!(out.join("investments.csv").is_file());
            let content = std::fs::read_to_string(out.join("investments.csv")).unwrap();
            assert_eq!(content.lines().count(), 3, "header + two merged rows");
        }

        #[test]
        fn separate_out_splits_investments_by_report_and_format_instead_of_merging() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "Report A", "F"), investment(2, "Report B", "F")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let flags = OutFlags { separate_out: true, ..OutFlags::default() };
            write_files(&tables, &out, OutStructureMode::Regular, flags).unwrap();

            assert!(!out.join("investments.csv").exists(), "the merged file must not be produced");
            assert!(out.join("investments__Report A__F.csv").is_file());
            assert!(out.join("investments__Report B__F.csv").is_file());
        }

        #[test]
        fn each_split_file_contains_only_its_own_report_rows_with_the_full_header() {
            let mut tables = empty_tables();
            tables.investments = vec![investment(1, "Report A", "F"), investment(2, "Report B", "F")];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let flags = OutFlags { separate_out: true, ..OutFlags::default() };
            write_files(&tables, &out, OutStructureMode::Regular, flags).unwrap();

            let content_a = std::fs::read_to_string(out.join("investments__Report A__F.csv")).unwrap();
            assert!(content_a.starts_with("ID,Format,Report,Report page"));
            assert_eq!(content_a.lines().count(), 2, "header + exactly one row for Report A");
            assert!(content_a.contains(",Report A,"));
            assert!(!content_a.contains(",Report B,"));
        }

        #[test]
        fn separate_out_also_splits_funds_assets() {
            let mut tables = empty_tables();
            tables.funds_assets = vec![
                FundAssetsRow::new(1, 1, "Report A".into(), "F".into(), 1, None, 100.0, 40.0, 60.0, Currency::EUR).unwrap(),
                FundAssetsRow::new(2, 2, "Report B".into(), "F".into(), 2, None, 100.0, 40.0, 60.0, Currency::EUR).unwrap(),
            ];
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            let flags = OutFlags { separate_out: true, ..OutFlags::default() };
            write_files(&tables, &out, OutStructureMode::Regular, flags).unwrap();

            assert!(!out.join("funds_assets.csv").exists());
            assert!(out.join("funds_assets__Report A__F.csv").is_file());
            assert!(out.join("funds_assets__Report B__F.csv").is_file());
        }

        #[test]
        fn separate_out_does_not_affect_tables_outside_its_documented_scope() {
            // `funds`/`funds_sfdr_classification`/`funds_esg_indicators`/`assets_managers`/
            // `investments_managers_to_funds`/`funds_change_name` and the yaml file stay merged
            // as usual -- only `investments`/`funds_assets` are split (see the module doc's
            // judgment-call note).
            let mut tables = empty_tables();
            tables.funds = vec![
                FundRow::new(1, "ALPHA FUND".into(), None, Some(1), Some("Report A".into()), Some("F".into())).unwrap(),
                FundRow::new(2, "BETA FUND".into(), None, Some(2), Some("Report B".into()), Some("F".into())).unwrap(),
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
