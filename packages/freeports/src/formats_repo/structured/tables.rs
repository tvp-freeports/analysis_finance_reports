//! Le tabelle CSV del livello structured: lettura, validazione e unione.
//!
//! Cinque file sotto `content/algorithms/structured/`, due gruppi:
//!
//! - **`investments/`** — `args.csv` (la tabella principale, una riga per pipe),
//!   `additional_args.csv` e `partial_pipes.csv` (zero o una riga per pipe),
//!   `deselection_lists.csv` (quante righe si vuole per pipe).
//! - **`page_classify/`** — `args.csv`, con più righe per pipe (una per `Header set`).
//!
//! Nel riferimento tutto questo è pandas: `read_csv`, `set_index` su una `MultiIndex`,
//! `join(validate="one_to_one")`, `groupby().agg()`, e uno schema pandera per tabella.
//! `PLAN.md` §2 principio 7 e §12 D8 vietano quelle dipendenze: qui ogni tabella è una struct
//! `Deserialize` letta col crate `csv`, la `MultiIndex` è un [`ComputedId`], la join è un
//! `HashMap`, e ogni `pa.Check` è una funzione che riporta **il numero di riga** — che è ciò che
//! il piano chiede espressamente come focus dei test di M7.
//!
//! **Le celle vuote.** Pandas legge una cella vuota come `NaN` e pandera la accetta dove la
//! colonna è `nullable`. Qui ogni colonna facoltativa è un `Option<T>` e la cella vuota è `None`:
//! serve una famiglia di deserializzatori dedicati, perché serde su `Option<i16>` fallirebbe sulla
//! stringa vuota invece di leggerla come "assente", e su `bool` non accetterebbe il `TRUE`
//! maiuscolo che qualunque foglio di calcolo scrive.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Deserializer};

use crate::input::document::selection::is_pdfline_selection;

use super::super::id_format::{ComputedId, FkRelation, computed_ids, id_matches};

/// La cartella dei CSV structured dentro il repo formati.
pub const STRUCTURED_DIR: &str = "content/algorithms/structured";

/// Il nome della pipeline a cui appartengono le righe di `investments/` che non ne dichiarano uno.
pub const INVESTMENTS_PIPELINE: &str = "investments";

/// Fallimenti nella lettura o validazione di una tabella structured.
///
/// Ogni variante riporta il file e, dove ha senso, la riga: è il requisito esplicito di `PLAN.md`
/// §11 per M7 ("ogni CSV malformato dà l'errore giusto con la riga giusta").
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    #[error("missing formats-repository CSV file: {0}")]
    MissingCsv(PathBuf),
    #[error("{path}: malformed row at line {line}: {reason}")]
    MalformedRow { path: PathBuf, line: usize, reason: String },
    #[error("{path}: missing required column '{column}'")]
    MissingColumn { path: PathBuf, column: String },
    #[error("{path}, line {line}: ID '{id}' does not match the expected ID pattern")]
    InvalidId { path: PathBuf, line: usize, id: String },
    #[error("{path}, line {line}: column '{column}' is not a valid line selection: '{value}'")]
    InvalidLineSelection { path: PathBuf, line: usize, column: String, value: String },
    /// Una riga di una tabella secondaria non ha corrispondenza nella tabella principale: nel
    /// riferimento è il `pa.Check.isin(valid_algorithm_ids)` sull'indice.
    #[error("{path}, line {line}: '{id}' has no matching row in the principal table")]
    UnmatchedRow { path: PathBuf, line: usize, id: String },
    /// Due righe di una tabella "al più una per pipe" configurano lo stesso pipe: nel riferimento
    /// è il `validate="one_to_one"` della join.
    #[error("{path}, line {line}: '{id}' is configured twice, but this table allows at most one row per pipe")]
    DuplicateRow { path: PathBuf, line: usize, id: String },
    /// Un segmento disabilitato porta comunque la sua configurazione: nel riferimento è il
    /// `validate_partial_pipes` di `structured_formats_schema`.
    #[error("{path}, line {line}: segment '{segment}' is disabled for '{id}' but column '{column}' is not empty")]
    DisabledSegmentConfigured { path: PathBuf, line: usize, id: String, segment: &'static str, column: &'static str },
    /// Due righe di `page_classify/args.csv` con lo stesso pipe dichiarano classi diverse.
    #[error("{path}, line {line}: '{id}' is declared as class '{found}' but also as '{first}'")]
    ConflictingClass { path: PathBuf, line: usize, id: String, first: String, found: String },
}

// ---------------------------------------------------------------------------------------------
// Deserializzatori per le celle facoltative
// ---------------------------------------------------------------------------------------------

/// Una cella di testo: vuota significa assente.
fn optional_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    Ok(if raw.trim().is_empty() { None } else { Some(raw) })
}

/// Una cella numerica: vuota significa assente, il resto deve essere un numero.
fn optional_number<'de, D: Deserializer<'de>, T: FromStr>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T::Err: std::fmt::Display,
{
    let raw = String::deserialize(deserializer)?;
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    raw.parse::<T>().map(Some).map_err(serde::de::Error::custom)
}

/// Una cella booleana: vuota significa assente, `TRUE`/`FALSE` in qualunque combinazione di
/// maiuscole e minuscole (è così che i fogli di calcolo scrivono i booleani).
fn optional_bool<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => Ok(None),
        "true" => Ok(Some(true)),
        "false" => Ok(Some(false)),
        other => Err(serde::de::Error::custom(format!("expected TRUE, FALSE or an empty cell, found '{other}'"))),
    }
}

// ---------------------------------------------------------------------------------------------
// Le cinque tabelle, così come stanno su disco
// ---------------------------------------------------------------------------------------------

/// Una riga di `investments/args.csv`: la configurazione principale di un pipe investments.
///
/// Le cinque colonne numeriche sono **posizioni di colonna** nella tabella del documento, non
/// valori: possono essere negative (`-1` = l'ultima colonna), da cui `i16` e non un unsigned.
#[derive(Debug, Clone, Deserialize)]
pub struct InvestmentsArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Subfund set", deserialize_with = "optional_text")]
    pub subfund_set: Option<String>,
    #[serde(rename = "Currency set", deserialize_with = "optional_text")]
    pub currency_set: Option<String>,
    #[serde(rename = "Body set", deserialize_with = "optional_text")]
    pub body_set: Option<String>,
    #[serde(rename = "Market value", deserialize_with = "optional_number")]
    pub market_value: Option<i16>,
    #[serde(rename = "Quantity", deserialize_with = "optional_number")]
    pub quantity: Option<i16>,
    #[serde(rename = "% net assets", deserialize_with = "optional_number")]
    pub perc_net_assets: Option<i16>,
    #[serde(rename = "Acquisition cost", deserialize_with = "optional_number")]
    pub acquisition_cost: Option<i16>,
    #[serde(rename = "Acquisition currency", deserialize_with = "optional_number")]
    pub acquisition_currency: Option<i16>,
}

/// Una riga di `investments/additional_args.csv`.
#[derive(Debug, Clone, Deserialize)]
pub struct AdditionalArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Algorithm flags", deserialize_with = "optional_text")]
    pub algorithm_flags: Option<String>,
    #[serde(rename = "Tolerance", deserialize_with = "optional_number")]
    pub tolerance: Option<f32>,
    #[serde(rename = "Interpret quantity as float", deserialize_with = "optional_bool")]
    pub interpret_quantity_as_float: Option<bool>,
    #[serde(rename = "Interpret cost and value as int", deserialize_with = "optional_bool")]
    pub interpret_cost_and_value_as_int: Option<bool>,
    #[serde(rename = "Geometrical indexing", deserialize_with = "optional_bool")]
    pub geometrical_indexing: Option<bool>,
    #[serde(rename = "Merge previous", deserialize_with = "optional_bool")]
    pub merge_previous: Option<bool>,
}

/// Una riga di `investments/deselection_lists.csv`.
#[derive(Debug, Clone, Deserialize)]
pub struct DeselectionRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Deselection set", deserialize_with = "optional_text")]
    pub deselection_set: Option<String>,
}

/// Una riga di `investments/partial_pipes.csv`: quali segmenti della pipeline sono attivi.
///
/// Una colonna vuota significa "attivo": è la `fillna(True)` del riferimento.
#[derive(Debug, Clone, Deserialize)]
pub struct PartialPipesRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "pdf_extract", deserialize_with = "optional_bool")]
    pub pdf_extract: Option<bool>,
    #[serde(rename = "text_filter", deserialize_with = "optional_bool")]
    pub text_filter: Option<bool>,
    #[serde(rename = "deserialize", deserialize_with = "optional_bool")]
    pub deserialize: Option<bool>,
}

/// Una riga di `page_classify/args.csv`.
#[derive(Debug, Clone, Deserialize)]
pub struct PageClassifyArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Header set", deserialize_with = "optional_text")]
    pub header_set: Option<String>,
    #[serde(rename = "Class")]
    pub class: String,
}

// ---------------------------------------------------------------------------------------------
// Lettura
// ---------------------------------------------------------------------------------------------

fn row_error(path: &Path, line: usize, error: &csv::Error) -> TableError {
    let message = error.to_string();
    if let Some(rest) = message.split("missing field `").nth(1)
        && let Some(column) = rest.split('`').next()
    {
        return TableError::MissingColumn { path: path.to_path_buf(), column: column.to_string() };
    }
    TableError::MalformedRow { path: path.to_path_buf(), line, reason: message }
}

/// Legge una tabella structured in righe tipizzate, insieme al proprio percorso (che finisce in
/// ogni messaggio d'errore).
fn read_table<T: serde::de::DeserializeOwned>(
    formats_repo_dir: &Path,
    relative_path: &str,
) -> Result<(PathBuf, Vec<T>), TableError> {
    let path = formats_repo_dir.join(STRUCTURED_DIR).join(relative_path);
    if !path.is_file() {
        return Err(TableError::MissingCsv(path));
    }
    let mut reader = csv::Reader::from_path(&path)
        .map_err(|e| TableError::MalformedRow { path: path.clone(), line: 0, reason: e.to_string() })?;
    let mut rows = Vec::new();
    for (i, record) in reader.deserialize::<T>().enumerate() {
        rows.push(record.map_err(|e| row_error(&path, i + 1, &e))?);
    }
    Ok((path, rows))
}

/// Verifica la forma di ogni `ID` e ne deriva l'identità completa.
fn identify(
    path: &Path,
    ids: &[&str],
    pipeline_default: &str,
    relation: FkRelation,
) -> Result<Vec<ComputedId>, TableError> {
    for (i, id) in ids.iter().enumerate() {
        if !id_matches(id, relation.id_format()) {
            return Err(TableError::InvalidId { path: path.to_path_buf(), line: i + 1, id: (*id).to_string() });
        }
    }
    Ok(computed_ids(ids, Some(pipeline_default), relation))
}

/// Verifica che una cella, se valorizzata, sia una selezione di righe ben formata.
fn check_line_selection(
    path: &Path,
    line: usize,
    column: &str,
    value: Option<&String>,
) -> Result<(), TableError> {
    match value {
        Some(text) if !is_pdfline_selection(text) => Err(TableError::InvalidLineSelection {
            path: path.to_path_buf(),
            line,
            column: column.to_string(),
            value: text.clone(),
        }),
        _ => Ok(()),
    }
}

/// Indicizza una tabella secondaria "al più una riga per pipe", rifiutando i duplicati e le righe
/// che non corrispondono a nulla nella tabella principale.
fn index_unique<T>(
    path: &Path,
    ids: Vec<ComputedId>,
    rows: Vec<T>,
    known: &HashSet<&ComputedId>,
) -> Result<HashMap<ComputedId, T>, TableError> {
    let mut map = HashMap::new();
    for (i, (id, row)) in ids.into_iter().zip(rows).enumerate() {
        let line = i + 1;
        if !known.contains(&id) {
            return Err(TableError::UnmatchedRow { path: path.to_path_buf(), line, id: id.to_string() });
        }
        if map.insert(id.clone(), row).is_some() {
            return Err(TableError::DuplicateRow { path: path.to_path_buf(), line, id: id.to_string() });
        }
    }
    Ok(map)
}

// ---------------------------------------------------------------------------------------------
// Le configurazioni unite, una per pipe
// ---------------------------------------------------------------------------------------------

/// Tutto ciò che il repo dichiara su un pipe investments, con le quattro tabelle già unite.
#[derive(Debug, Clone)]
pub struct InvestmentsConfig {
    pub id: ComputedId,
    pub args: InvestmentsArgsRow,
    pub additional: Option<AdditionalArgsRow>,
    /// Le selezioni da sottrarre al `Body set`, nell'ordine del file.
    pub deselection_sets: Vec<String>,
    pub partial_pipes: Option<PartialPipesRow>,
}

impl InvestmentsConfig {
    /// Se il segmento `pdf_extract` va costruito. Una configurazione assente significa "sì": è la
    /// `fillna(True)` del riferimento.
    pub fn wants_pdf_extract(&self) -> bool {
        self.partial_pipes.as_ref().and_then(|p| p.pdf_extract).unwrap_or(true)
    }

    /// Se il segmento `text_filter` va costruito. Vedi [`Self::wants_pdf_extract`].
    pub fn wants_text_filter(&self) -> bool {
        self.partial_pipes.as_ref().and_then(|p| p.text_filter).unwrap_or(true)
    }

    /// Se il segmento `deserialize` va costruito. Vedi [`Self::wants_pdf_extract`].
    pub fn wants_deserialize(&self) -> bool {
        self.partial_pipes.as_ref().and_then(|p| p.deserialize).unwrap_or(true)
    }
}

/// Tutto ciò che il repo dichiara su un pipe di classificazione pagina.
#[derive(Debug, Clone)]
pub struct PageClassifyConfig {
    pub id: ComputedId,
    /// La page class che il pipe assegna alla pagina quando **tutti** gli header combaciano.
    pub class: String,
    /// Gli header da cercare, uno per riga del CSV, nell'ordine del file.
    pub header_sets: Vec<String>,
}

/// Le colonne che un segmento disattivato non può portare, per ciascuno dei tre segmenti.
///
/// Porting di `validate_partial_pipes`: dichiarare `pdf_extract = FALSE` e poi configurare un
/// `Body set` è una contraddizione, non una configurazione che il caricamento debba indovinare.
const DISABLED_SEGMENT_COLUMNS: [(&str, &[&str]); 3] = [
    ("pdf_extract", &["Subfund set", "Currency set", "Body set", "Deselection set", "Algorithm flags", "Tolerance"]),
    (
        "text_filter",
        &["Market value", "Quantity", "% net assets", "Acquisition cost", "Acquisition currency", "Geometrical indexing", "Merge previous"],
    ),
    ("deserialize", &["Interpret quantity as float", "Interpret cost and value as int"]),
];

impl InvestmentsConfig {
    /// Il valore "è configurata" di ogni colonna citata da [`DISABLED_SEGMENT_COLUMNS`].
    fn column_is_set(&self, column: &str) -> bool {
        let additional = self.additional.as_ref();
        match column {
            "Subfund set" => self.args.subfund_set.is_some(),
            "Currency set" => self.args.currency_set.is_some(),
            "Body set" => self.args.body_set.is_some(),
            "Deselection set" => !self.deselection_sets.is_empty(),
            "Algorithm flags" => additional.is_some_and(|a| a.algorithm_flags.is_some()),
            "Tolerance" => additional.is_some_and(|a| a.tolerance.is_some()),
            "Market value" => self.args.market_value.is_some(),
            "Quantity" => self.args.quantity.is_some(),
            "% net assets" => self.args.perc_net_assets.is_some(),
            "Acquisition cost" => self.args.acquisition_cost.is_some(),
            "Acquisition currency" => self.args.acquisition_currency.is_some(),
            "Geometrical indexing" => additional.is_some_and(|a| a.geometrical_indexing.is_some()),
            "Merge previous" => additional.is_some_and(|a| a.merge_previous.is_some()),
            "Interpret quantity as float" => additional.is_some_and(|a| a.interpret_quantity_as_float.is_some()),
            "Interpret cost and value as int" => additional.is_some_and(|a| a.interpret_cost_and_value_as_int.is_some()),
            other => unreachable!("DISABLED_SEGMENT_COLUMNS names an unknown column {other:?}"),
        }
    }

    /// Nessun segmento disattivato porta configurazione.
    fn check_disabled_segments(&self, path: &Path, line: usize) -> Result<(), TableError> {
        let enabled = [self.wants_pdf_extract(), self.wants_text_filter(), self.wants_deserialize()];
        for ((segment, columns), enabled) in DISABLED_SEGMENT_COLUMNS.iter().zip(enabled) {
            if enabled {
                continue;
            }
            for column in *columns {
                if self.column_is_set(column) {
                    return Err(TableError::DisabledSegmentConfigured {
                        path: path.to_path_buf(),
                        line,
                        id: self.id.to_string(),
                        segment,
                        column,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Le configurazioni dei pipe investments dichiarate dal repo, nell'ordine di `args.csv`.
///
/// È il porting di `structured/pipelines/investments.py::get_structured_formats`: legge le quattro
/// tabelle, verifica ogni riga, e le unisce sulla chiave `(formato, pipeline, indice)`.
pub fn get_investments_configs(formats_repo_dir: &Path) -> Result<Vec<InvestmentsConfig>, TableError> {
    let (args_path, args): (_, Vec<InvestmentsArgsRow>) = read_table(formats_repo_dir, "investments/args.csv")?;
    let args_ids = identify(
        &args_path,
        &args.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        INVESTMENTS_PIPELINE,
        FkRelation::OneToOne,
    )?;
    for (i, row) in args.iter().enumerate() {
        let line = i + 1;
        check_line_selection(&args_path, line, "Subfund set", row.subfund_set.as_ref())?;
        check_line_selection(&args_path, line, "Currency set", row.currency_set.as_ref())?;
        check_line_selection(&args_path, line, "Body set", row.body_set.as_ref())?;
    }
    let known: HashSet<&ComputedId> = args_ids.iter().collect();

    let (add_path, add_rows): (_, Vec<AdditionalArgsRow>) =
        read_table(formats_repo_dir, "investments/additional_args.csv")?;
    let add_ids = identify(
        &add_path,
        &add_rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        INVESTMENTS_PIPELINE,
        FkRelation::OneToMaybe,
    )?;
    let mut additional = index_unique(&add_path, add_ids, add_rows, &known)?;

    let (partial_path, partial_rows): (_, Vec<PartialPipesRow>) =
        read_table(formats_repo_dir, "investments/partial_pipes.csv")?;
    let partial_ids = identify(
        &partial_path,
        &partial_rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        INVESTMENTS_PIPELINE,
        FkRelation::OneToMaybe,
    )?;
    let mut partial = index_unique(&partial_path, partial_ids, partial_rows, &known)?;

    let (desel_path, desel_rows): (_, Vec<DeselectionRow>) =
        read_table(formats_repo_dir, "investments/deselection_lists.csv")?;
    let desel_ids = identify(
        &desel_path,
        &desel_rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        INVESTMENTS_PIPELINE,
        FkRelation::OneToMany,
    )?;
    let mut deselections: HashMap<ComputedId, Vec<String>> = HashMap::new();
    for (i, (id, row)) in desel_ids.into_iter().zip(desel_rows).enumerate() {
        let line = i + 1;
        if !known.contains(&id) {
            return Err(TableError::UnmatchedRow { path: desel_path.clone(), line, id: id.to_string() });
        }
        check_line_selection(&desel_path, line, "Deselection set", row.deselection_set.as_ref())?;
        if let Some(set) = row.deselection_set {
            deselections.entry(id).or_default().push(set);
        }
    }

    let mut configs = Vec::with_capacity(args.len());
    for (i, (id, row)) in args_ids.into_iter().zip(args).enumerate() {
        let config = InvestmentsConfig {
            additional: additional.remove(&id),
            deselection_sets: deselections.remove(&id).unwrap_or_default(),
            partial_pipes: partial.remove(&id),
            args: row,
            id,
        };
        config.check_disabled_segments(&args_path, i + 1)?;
        configs.push(config);
    }
    Ok(configs)
}

/// Le configurazioni dei pipe di classificazione pagina, nell'ordine di prima comparsa in
/// `page_classify/args.csv`.
///
/// Più righe con lo stesso `ID` descrivono lo **stesso** pipe con più header da cercare, e devono
/// quindi dichiarare la stessa `Class`: è l'`aggregate_classes` del riferimento, che solleva se il
/// gruppo non è omogeneo.
pub fn get_page_classify_configs(formats_repo_dir: &Path) -> Result<Vec<PageClassifyConfig>, TableError> {
    let (path, rows): (_, Vec<PageClassifyArgsRow>) = read_table(formats_repo_dir, "page_classify/args.csv")?;
    // La pipeline di default è quella **senza nome**, non `investments`: un classificatore di
    // pagina appartiene alla pipeline di default del formato.
    let ids = identify(&path, &rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), "", FkRelation::OneToMany)?;

    let mut order: Vec<ComputedId> = Vec::new();
    let mut classes: HashMap<ComputedId, String> = HashMap::new();
    let mut headers: HashMap<ComputedId, Vec<String>> = HashMap::new();
    for (i, (id, row)) in ids.into_iter().zip(rows).enumerate() {
        let line = i + 1;
        check_line_selection(&path, line, "Header set", row.header_set.as_ref())?;
        match classes.get(&id) {
            None => {
                order.push(id.clone());
                classes.insert(id.clone(), row.class.clone());
            }
            Some(first) if *first != row.class => {
                return Err(TableError::ConflictingClass {
                    path: path.clone(),
                    line,
                    id: id.to_string(),
                    first: first.clone(),
                    found: row.class.clone(),
                });
            }
            Some(_) => {}
        }
        if let Some(set) = row.header_set {
            headers.entry(id).or_default().push(set);
        }
    }

    Ok(order
        .into_iter()
        .map(|id| PageClassifyConfig {
            class: classes.remove(&id).expect("ogni id in `order` è stato inserito in `classes`"),
            header_sets: headers.remove(&id).unwrap_or_default(),
            id,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const ARGS_HEADER: &str =
        "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n";
    const ADD_HEADER: &str = "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous\n";
    const PARTIAL_HEADER: &str = "ID,pdf_extract,text_filter,deserialize\n";
    const DESEL_HEADER: &str = "ID,Deselection set\n";
    const PAGE_CLASSIFY_HEADER: &str = "ID,Header set,Class\n";

    /// Un repo formati con i cinque CSV structured. Ogni tabella parte dalla sola intestazione e
    /// i test aggiungono le righe che servono loro.
    struct Repo {
        dir: TempDir,
    }

    impl Repo {
        fn new() -> Self {
            let dir = TempDir::new().expect("temp dir");
            let base = dir.path().join(STRUCTURED_DIR);
            fs::create_dir_all(base.join("investments")).expect("investments dir");
            fs::create_dir_all(base.join("page_classify")).expect("page_classify dir");
            let repo = Self { dir };
            repo.write("investments/args.csv", ARGS_HEADER);
            repo.write("investments/additional_args.csv", ADD_HEADER);
            repo.write("investments/partial_pipes.csv", PARTIAL_HEADER);
            repo.write("investments/deselection_lists.csv", DESEL_HEADER);
            repo.write("page_classify/args.csv", PAGE_CLASSIFY_HEADER);
            repo
        }

        fn write(&self, relative: &str, content: &str) -> &Self {
            fs::write(self.dir.path().join(STRUCTURED_DIR).join(relative), content).expect("write csv");
            self
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }
    }

    mod investments_args {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn reads_one_config_per_row_in_file_order() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nB-EN24,,,,2,,,,\n"),
            );
            let configs = get_investments_configs(repo.path()).unwrap();
            assert_eq!(
                configs.iter().map(|c| c.id.to_string()).collect::<Vec<_>>(),
                vec!["A-EN24(investments)/0".to_string(), "B-EN24(investments)/0".to_string()]
            );
        }

        #[test]
        fn the_default_pipeline_of_this_table_is_investments() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            assert_eq!(get_investments_configs(repo.path()).unwrap()[0].id.pipeline, "investments");
        }

        #[test]
        fn two_rows_of_the_same_format_are_two_pipes_of_the_same_pipeline() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nA-EN24,,,,2,,,,\n"));
            let configs = get_investments_configs(repo.path()).unwrap();
            assert_eq!(configs[0].id.index, 0);
            assert_eq!(configs[1].id.index, 1);
        }

        #[test]
        fn an_empty_numeric_cell_becomes_none() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert_eq!(config.args.market_value, Some(1));
            assert_eq!(config.args.quantity, None);
        }

        #[test]
        fn a_negative_column_position_is_accepted() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,-1,2,,\n"));
            assert_eq!(get_investments_configs(repo.path()).unwrap()[0].args.quantity, Some(-1));
        }

        #[test]
        fn a_non_numeric_cell_reports_its_line() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nB-EN24,,,,x,,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(matches!(err, TableError::MalformedRow { line: 2, .. }), "{err}");
        }

        #[test]
        fn an_id_carrying_an_index_is_rejected_by_this_table() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24(investments)/0,,,,1,,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(matches!(err, TableError::InvalidId { line: 1, .. }), "{err}");
        }

        #[test]
        fn a_malformed_line_selection_names_the_column_and_the_line() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,ArialMT ???,1,,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            let TableError::InvalidLineSelection { line, column, .. } = err else {
                panic!("expected InvalidLineSelection, got {err}")
            };
            assert_eq!((line, column.as_str()), (1, "Body set"));
        }

        #[test]
        fn a_well_formed_line_selection_passes() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,ArialMT(:27),ArialNarrow(:208),ArialNarrow(:768),1,-1,2,,\n"),
            );
            assert!(get_investments_configs(repo.path()).is_ok());
        }

        #[test]
        fn a_missing_column_names_it() {
            let repo = Repo::new();
            repo.write("investments/args.csv", "ID,Subfund set\nA-EN24,\n");
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(matches!(err, TableError::MissingColumn { .. }), "{err}");
        }

        #[test]
        fn a_missing_file_is_reported_with_its_path() {
            let repo = Repo::new();
            fs::remove_file(repo.path().join(STRUCTURED_DIR).join("investments/args.csv")).unwrap();
            assert!(matches!(get_investments_configs(repo.path()), Err(TableError::MissingCsv(_))));
        }

        #[test]
        fn an_empty_table_declares_no_pipe() {
            assert!(get_investments_configs(Repo::new().path()).unwrap().is_empty());
        }
    }

    mod secondary_tables {
        use super::*;
        use pretty_assertions::assert_eq;

        fn with_one_pipe() -> Repo {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo
        }

        #[test]
        fn additional_args_join_onto_the_matching_pipe() {
            let repo = with_one_pipe();
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,,,TRUE,FALSE,,TRUE\n"));
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            let additional = config.additional.as_ref().unwrap();
            assert_eq!(additional.interpret_quantity_as_float, Some(true));
            assert_eq!(additional.interpret_cost_and_value_as_int, Some(false));
            assert_eq!(additional.merge_previous, Some(true));
            assert_eq!(additional.geometrical_indexing, None);
        }

        #[test]
        fn a_pipe_without_additional_args_simply_has_none() {
            let repo = with_one_pipe();
            assert!(get_investments_configs(repo.path()).unwrap()[0].additional.is_none());
        }

        #[test]
        fn a_full_id_in_a_secondary_table_matches_the_derived_id_of_the_principal_one() {
            let repo = with_one_pipe();
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}A-EN24(investments)/0,,,TRUE,,,\n"),
            );
            assert!(get_investments_configs(repo.path()).unwrap()[0].additional.is_some());
        }

        #[test]
        fn a_secondary_row_matching_no_pipe_is_rejected_with_its_line() {
            let repo = with_one_pipe();
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}GHOST-EN24,,,TRUE,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            let TableError::UnmatchedRow { line, id, .. } = err else { panic!("expected UnmatchedRow, got {err}") };
            assert_eq!((line, id.as_str()), (1, "GHOST-EN24(investments)/0"));
        }

        #[test]
        fn two_additional_args_rows_for_one_pipe_are_rejected() {
            let repo = with_one_pipe();
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}A-EN24(investments)/0,,,TRUE,,,\nA-EN24(investments)/0,,,FALSE,,,\n"),
            );
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(matches!(err, TableError::DuplicateRow { line: 2, .. }), "{err}");
        }

        #[test]
        fn deselection_rows_accumulate_in_file_order() {
            let repo = with_one_pipe();
            repo.write(
                "investments/deselection_lists.csv",
                &format!("{DESEL_HEADER}A-EN24,\"Arial \"\"FIRST\"\"\"\nA-EN24,\"Arial \"\"SECOND\"\"\"\n"),
            );
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert_eq!(config.deselection_sets, vec!["Arial \"FIRST\"".to_string(), "Arial \"SECOND\"".to_string()]);
        }

        #[test]
        fn several_deselection_rows_for_one_pipe_are_not_a_duplicate() {
            // La relazione è "one to many": più righe per pipe sono la norma, non un errore.
            let repo = with_one_pipe();
            repo.write("investments/deselection_lists.csv", &format!("{DESEL_HEADER}A-EN24,\nA-EN24,\n"));
            assert!(get_investments_configs(repo.path()).is_ok());
        }

        #[test]
        fn an_unmatched_deselection_row_is_rejected() {
            let repo = with_one_pipe();
            repo.write("investments/deselection_lists.csv", &format!("{DESEL_HEADER}GHOST-EN24,\n"));
            assert!(matches!(get_investments_configs(repo.path()), Err(TableError::UnmatchedRow { .. })));
        }

        #[test]
        fn a_malformed_deselection_selection_is_rejected() {
            let repo = with_one_pipe();
            repo.write("investments/deselection_lists.csv", &format!("{DESEL_HEADER}A-EN24,???\n"));
            assert!(matches!(get_investments_configs(repo.path()), Err(TableError::InvalidLineSelection { .. })));
        }
    }

    mod partial_pipes {
        use super::*;

        fn repo_with(args_row: &str, partial_row: &str) -> Repo {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}{args_row}"));
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}{partial_row}"));
            repo
        }

        #[test]
        fn every_segment_is_wanted_when_no_row_declares_otherwise() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "");
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert!(config.wants_pdf_extract() && config.wants_text_filter() && config.wants_deserialize());
        }

        #[test]
        fn an_empty_cell_still_means_wanted() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,,,\n");
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert!(config.wants_pdf_extract() && config.wants_text_filter() && config.wants_deserialize());
        }

        #[test]
        fn a_false_cell_disables_its_segment() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,FALSE,TRUE,TRUE\n");
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert!(!config.wants_pdf_extract());
            assert!(config.wants_text_filter() && config.wants_deserialize());
        }

        #[test]
        fn a_disabled_segment_carrying_its_configuration_is_a_contradiction() {
            let repo = repo_with("A-EN24,,,ArialMT,1,,,,\n", "A-EN24,FALSE,TRUE,TRUE\n");
            let err = get_investments_configs(repo.path()).unwrap_err();
            let TableError::DisabledSegmentConfigured { segment, column, .. } = err else {
                panic!("expected DisabledSegmentConfigured, got {err}")
            };
            assert_eq!((segment, column), ("pdf_extract", "Body set"));
        }

        #[test]
        fn a_disabled_text_filter_may_not_carry_a_column_position() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,TRUE,FALSE,TRUE\n");
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(err, TableError::DisabledSegmentConfigured { segment: "text_filter", column: "Market value", .. }),
                "{err}"
            );
        }

        #[test]
        fn a_disabled_deserialize_may_not_carry_its_interpretation_flags() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,,,,,\n"));
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}A-EN24,TRUE,TRUE,FALSE\n"));
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,,,TRUE,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(err, TableError::DisabledSegmentConfigured { segment: "deserialize", .. }),
                "{err}"
            );
        }

        #[test]
        fn a_disabled_segment_with_nothing_configured_is_fine() {
            let repo = repo_with("A-EN24,,,,,,,,\n", "A-EN24,FALSE,TRUE,TRUE\n");
            assert!(get_investments_configs(repo.path()).is_ok());
        }

        #[test]
        fn a_deselection_row_counts_as_pdf_extract_configuration() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,FALSE,TRUE,TRUE\n");
            repo.write("investments/deselection_lists.csv", &format!("{DESEL_HEADER}A-EN24,ArialMT\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(err, TableError::DisabledSegmentConfigured { column: "Deselection set", .. }),
                "{err}"
            );
        }

        #[test]
        fn an_unparsable_boolean_cell_is_rejected() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,maybe,,\n");
            assert!(matches!(get_investments_configs(repo.path()), Err(TableError::MalformedRow { line: 1, .. })));
        }

        #[test]
        fn a_lowercase_boolean_cell_is_accepted() {
            let repo = repo_with("A-EN24,,,,,,,,\n", "A-EN24,false,true,true\n");
            assert!(!get_investments_configs(repo.path()).unwrap()[0].wants_pdf_extract());
        }
    }

    mod page_classify {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn groups_the_header_sets_of_one_pipe_together() {
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!(
                    "{PAGE_CLASSIFY_HEADER}\
                     CARNE-EN23/0,\"Arial-BoldMT \"\"Description\"\"\",investments\n\
                     CARNE-EN23/0,\"Arial-BoldMT \"\"Currency\"\"\",investments\n\
                     CARNE-EN23/1,\"arial-boldmt \"\"Management Company\"\"\",manco\n"
                ),
            );
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert_eq!(configs.len(), 2);
            assert_eq!(configs[0].class, "investments");
            assert_eq!(configs[0].header_sets.len(), 2);
            assert_eq!(configs[1].class, "manco");
            assert_eq!(configs[1].header_sets.len(), 1);
        }

        #[test]
        fn the_default_pipeline_of_this_table_is_the_unnamed_one() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,investments\n"));
            let config = &get_page_classify_configs(repo.path()).unwrap()[0];
            assert_eq!(config.id.to_string(), "A-EN24()/0");
        }

        #[test]
        fn rows_without_an_index_all_belong_to_pipe_zero() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24,,inv\nA-EN24,,inv\n"));
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert_eq!(configs.len(), 1);
            assert_eq!(configs[0].id.index, 0);
        }

        #[test]
        fn two_rows_of_one_pipe_declaring_different_classes_are_rejected() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\nA-EN24/0,,manco\n"));
            let err = get_page_classify_configs(repo.path()).unwrap_err();
            let TableError::ConflictingClass { line, first, found, .. } = err else {
                panic!("expected ConflictingClass, got {err}")
            };
            assert_eq!((line, first.as_str(), found.as_str()), (2, "inv", "manco"));
        }

        #[test]
        fn different_pipes_may_declare_different_classes() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\nA-EN24/1,,manco\n"));
            assert_eq!(get_page_classify_configs(repo.path()).unwrap().len(), 2);
        }

        #[test]
        fn a_pipe_with_no_header_set_at_all_is_kept_with_an_empty_list() {
            // Un classificatore senza header combacia sempre: è raro ma legale, e va costruito.
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\n"));
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert!(configs[0].header_sets.is_empty());
        }

        #[test]
        fn a_malformed_header_set_names_the_column_and_the_line() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\nA-EN24/0,???,inv\n"));
            let err = get_page_classify_configs(repo.path()).unwrap_err();
            let TableError::InvalidLineSelection { line, column, .. } = err else {
                panic!("expected InvalidLineSelection, got {err}")
            };
            assert_eq!((line, column.as_str()), (2, "Header set"));
        }

        #[test]
        fn the_order_of_the_result_follows_first_appearance_in_the_file() {
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!("{PAGE_CLASSIFY_HEADER}B-EN24/0,,b\nA-EN24/0,,a\nB-EN24/0,,b\n"),
            );
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert_eq!(
                configs.iter().map(|c| c.id.format.as_str()).collect::<Vec<_>>(),
                vec!["B-EN24", "A-EN24"]
            );
        }

        #[test]
        fn an_empty_table_declares_no_pipe() {
            assert!(get_page_classify_configs(Repo::new().path()).unwrap().is_empty());
        }

        #[test]
        fn a_missing_file_is_reported_with_its_path() {
            let repo = Repo::new();
            fs::remove_file(repo.path().join(STRUCTURED_DIR).join("page_classify/args.csv")).unwrap();
            assert!(matches!(get_page_classify_configs(repo.path()), Err(TableError::MissingCsv(_))));
        }
    }
}
