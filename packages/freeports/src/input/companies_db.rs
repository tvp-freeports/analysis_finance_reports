//! Lettura del database di input (`input_db/`) e compilazione delle target companies.
//!
//! `PLAN.md` §9 (`input::{load_target_companies, compile_target_companies}`,
//! `input::companies_db`). `M9-implementation-plan.md` §0 Q1: porting di
//! `packages/freeports_core/src/input/companies_db.rs`, tolto tutto ciò che è confine PyO3
//! (`#[pyfunction]`/`PyErr`) e `polars` (letto invece con il crate `csv`, come ogni altro CSV di
//! repo/config in questo crate — `PLAN.md` §2 principio 7), incluse le correzioni di bug già
//! trovate e confermate in quel porting (non riprodotte qui):
//!
//! - matching `bud`/`regex` **non ancorato** (il riferimento Python usa `re.match`, ancorato in
//!   posizione 0, mascherato da un secondo bug che rendeva quella validazione un no-op — vedi il
//!   doc-comment di `require_regex_matches_name` nel riferimento Rust);
//! - **ordine di `companies.csv` preservato**, non alfabetico (`match_company`/`match_fast` è un
//!   algoritmo "primo prefisso che matcha vince": un nome più corto che è prefisso di uno più
//!   lungo deve restare *dopo* quello più specifico nell'ordine del file).
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, thiserror::Error)]
//! pub enum CompaniesDbError {
//!     ReadCsv { path: PathBuf, source: csv::Error },
//!     MissingColumn { path: PathBuf, column: &'static str },
//!     EmptyValue { path: PathBuf, column: &'static str, row: usize },
//!     NotNormalized { context: String, field: &'static str, value: String },
//!     BudNotContained { bud: String, name: String, normalized: String },
//!     InvalidRegex { pattern: String, name: String, message: String },
//!     RegexNotMatching { pattern: String, name: String, normalized: String },
//!     InvalidDate { context: String, value: String },
//!     Duplicate { path: PathBuf, kind: &'static str, value: String },
//!     UnknownReference { path: PathBuf, kind: &'static str, value: String },
//!     InvalidTickerSymbol { path: PathBuf, symbol: String },
//! }
//!
//! /// `get_target_companies` del riferimento, senza il confine Python: restituisce gli input
//! /// grezzi (nome/bud/regex/symbol per azienda, già filtrati per lista bersaglio e aggregati),
//! /// non ancora compilati in `CompanyMatchInfos`.
//! pub fn load_target_companies(
//!     input_db_directory: &Path,
//!     target_lists: &[String],
//! ) -> Result<Vec<crate::formats_utils::text_filter::matcher::TargetCompanyInput>, CompaniesDbError>;
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum CompileTargetCompaniesError {
//!     Load(#[from] CompaniesDbError),
//!     Compile(#[from] crate::formats_utils::text_filter::matcher::PatternCompileError),
//! }
//!
//! /// `PLAN.md` §9: thin wrapper su `CompanyMatchInfos::compile_from_target_companies`
//! /// (già in questo crate da M4, `formats_utils::text_filter::matcher` — nessun porting nuovo
//! /// lì, `M9-implementation-plan.md` §0 Q1).
//! pub fn compile_target_companies(
//!     input_db_directory: &Path,
//!     target_lists: &[String],
//! ) -> Result<Vec<crate::formats_utils::text_filter::matcher::CompanyMatchInfos>, CompileTargetCompaniesError>;
//! ```
//!
//! # Struttura di `input_db/` attesa (identica al riferimento)
//!
//! ```text
//! companies/companies.csv                  Name,Bud,Regex
//! companies/companies_additional_buds.csv  Company name,Bud
//! companies/companies_additional_regexs.csv Company name,Regex
//! companies/markets.csv                    Name
//! companies/tickers.csv                    Market name,Company name,Symbol
//! lists/lists.csv                          Name,Institution,Date
//! lists/company_to_list.csv                List name,Company name
//! ```
//!
//! Validazioni portate identiche al riferimento: nome azienda univoco, `Bud` già normalizzato
//! (`deep_normalize_string(bud) == bud`) e contenuto nel nome normalizzato dell'azienda, `Regex`
//! che matcha (non ancorato) il nome normalizzato, `Date` in forma `YYYY-MM-DD`, simbolo ticker
//! `[A-Z]{2,6}`, ogni riferimento incrociato (bud/regex aggiuntivi, `company_to_list`, `tickers`)
//! deve puntare a un'entità nota.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::core::normalization;
use crate::formats_utils::text_filter::matcher::{CompanyMatchInfos, PatternCompileError, TargetCompanyInput};

/// La cartella `companies_additional_*`/`markets.csv`/`tickers.csv` vive sotto questa
/// sottodirectory di `input_db_directory` — stessa costante del riferimento.
const COMPANIES_DIR: &str = "companies";
/// `lists.csv`/`company_to_list.csv` vivono sotto questa sottodirectory.
const LISTS_DIR: &str = "lists";

#[derive(Debug, thiserror::Error)]
pub enum CompaniesDbError {
    #[error("cannot read {}: {source}", path.display())]
    ReadCsv { path: PathBuf, source: csv::Error },
    #[error("{}: missing required column '{column}'", path.display())]
    MissingColumn { path: PathBuf, column: &'static str },
    #[error("{}: row {row} has an empty '{column}'", path.display())]
    EmptyValue { path: PathBuf, column: &'static str, row: usize },
    #[error("{context}: {field} '{value}' is not already normalized")]
    NotNormalized { context: String, field: &'static str, value: String },
    #[error("Bud '{bud}' is not contained in company name '{name}' (normalized: '{normalized}')")]
    BudNotContained { bud: String, name: String, normalized: String },
    #[error("invalid regex '{pattern}' for company '{name}': {message}")]
    InvalidRegex { pattern: String, name: String, message: String },
    #[error("regex '{pattern}' does not match company name '{name}' (normalized: '{normalized}')")]
    RegexNotMatching { pattern: String, name: String, normalized: String },
    #[error("{context}: '{value}' is not a valid YYYY-MM-DD date")]
    InvalidDate { context: String, value: String },
    #[error("{}: duplicate {kind} '{value}'", path.display())]
    Duplicate { path: PathBuf, kind: &'static str, value: String },
    #[error("{}: '{value}' is not a known {kind}", path.display())]
    UnknownReference { path: PathBuf, kind: &'static str, value: String },
    #[error("{}: '{symbol}' is not a valid ticker symbol (expected 2-6 uppercase letters)", path.display())]
    InvalidTickerSymbol { path: PathBuf, symbol: String },
}

#[derive(Debug, thiserror::Error)]
pub enum CompileTargetCompaniesError {
    #[error(transparent)]
    Load(#[from] CompaniesDbError),
    #[error(transparent)]
    Compile(#[from] PatternCompileError),
}

/// Una tabella CSV già letta interamente in memoria (nessun `DataFrame`, `PLAN.md` §2 principio
/// 7): gli header servono a risolvere il nome di colonna a un indice una volta sola, i record
/// grezzi restano `csv::StringRecord` fino a quando `required_str_column`/`optional_str_column`
/// non li convalidano.
struct Table {
    path: PathBuf,
    headers: csv::StringRecord,
    records: Vec<csv::StringRecord>,
}

fn read_table(path: &Path) -> Result<Table, CompaniesDbError> {
    let map_err = |source: csv::Error| CompaniesDbError::ReadCsv { path: path.to_path_buf(), source };
    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_path(path).map_err(map_err)?;
    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(e) => return Err(CompaniesDbError::ReadCsv { path: path.to_path_buf(), source: e }),
    };
    let mut records = Vec::new();
    for result in reader.records() {
        records.push(result.map_err(map_err)?);
    }
    Ok(Table { path: path.to_path_buf(), headers, records })
}

impl Table {
    fn column_index(&self, column: &'static str) -> Option<usize> {
        self.headers.iter().position(|h| h == column)
    }

    fn required_str_column(&self, column: &'static str) -> Result<Vec<String>, CompaniesDbError> {
        let idx = self
            .column_index(column)
            .ok_or_else(|| CompaniesDbError::MissingColumn { path: self.path.clone(), column })?;
        self.records
            .iter()
            .enumerate()
            .map(|(i, record)| {
                let value = record.get(idx).unwrap_or("");
                if value.is_empty() {
                    Err(CompaniesDbError::EmptyValue { path: self.path.clone(), column, row: i + 1 })
                } else {
                    Ok(value.to_string())
                }
            })
            .collect()
    }

    fn optional_str_column(&self, column: &'static str) -> Result<Vec<Option<String>>, CompaniesDbError> {
        match self.column_index(column) {
            None => Ok(vec![None; self.records.len()]),
            Some(idx) => Ok(self
                .records
                .iter()
                .map(|record| {
                    let value = record.get(idx).unwrap_or("");
                    if value.is_empty() { None } else { Some(value.to_string()) }
                })
                .collect()),
        }
    }
}

fn require_already_normalized(field: &'static str, value: &str, context: &str) -> Result<(), CompaniesDbError> {
    if normalization::deep_normalize_string(value) != value {
        return Err(CompaniesDbError::NotNormalized { context: context.to_string(), field, value: value.to_string() });
    }
    Ok(())
}

fn require_bud_contained_in_name(bud: &str, name: &str) -> Result<(), CompaniesDbError> {
    let n_name = normalization::deep_normalize_string(name);
    if !n_name.contains(bud) {
        return Err(CompaniesDbError::BudNotContained { bud: bud.to_string(), name: name.to_string(), normalized: n_name });
    }
    Ok(())
}

/// Matching non ancorato (bugfix già confermato in `freeports_core`, non riprodotto qui): ogni
/// consumatore reale di questo stesso pattern (`CompanyMatchInfos::compile_from_target_companies`)
/// lo cerca ovunque nella stringa, non solo in posizione 0 — vedi il doc-comment del modulo.
fn require_regex_matches_name(pattern: &str, name: &str) -> Result<(), CompaniesDbError> {
    let n_name = normalization::deep_normalize_string(name);
    let re = onig::Regex::new(pattern)
        .map_err(|e| CompaniesDbError::InvalidRegex { pattern: pattern.to_string(), name: name.to_string(), message: e.to_string() })?;
    if re.find(&n_name).is_none() {
        return Err(CompaniesDbError::RegexNotMatching { pattern: pattern.to_string(), name: name.to_string(), normalized: n_name });
    }
    Ok(())
}

fn require_valid_date(value: &str, context: &str) -> Result<(), CompaniesDbError> {
    let malformed = || CompaniesDbError::InvalidDate { context: context.to_string(), value: value.to_string() };
    let parts: Vec<&str> = value.split('-').collect();
    let [y, m, d] = parts[..].try_into().map_err(|_| malformed())?;
    let (y, m, d): (i32, u32, u32) =
        (y.parse().map_err(|_| malformed())?, m.parse().map_err(|_| malformed())?, d.parse().map_err(|_| malformed())?);
    if y < 1 || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(malformed());
    }
    Ok(())
}

struct CompanyRow {
    name: String,
    bud: Option<String>,
    regex: Option<String>,
}

fn load_companies(input_db_directory: &Path) -> Result<Vec<CompanyRow>, CompaniesDbError> {
    let path = input_db_directory.join(COMPANIES_DIR).join("companies.csv");
    let table = read_table(&path)?;
    let names = table.required_str_column("Name")?;
    let buds = table.optional_str_column("Bud")?;
    let regexs = table.optional_str_column("Regex")?;

    let mut seen = HashSet::new();
    let mut rows = Vec::with_capacity(names.len());
    for i in 0..names.len() {
        let name = names[i].clone();
        if !seen.insert(name.clone()) {
            return Err(CompaniesDbError::Duplicate { path: path.clone(), kind: "company name", value: name });
        }
        if let Some(bud) = &buds[i] {
            require_already_normalized("Bud", bud, &format!("{}, company '{name}'", path.display()))?;
            require_bud_contained_in_name(bud, &name)?;
        }
        if let Some(regex) = &regexs[i] {
            require_regex_matches_name(regex, &name)?;
        }
        rows.push(CompanyRow { name, bud: buds[i].clone(), regex: regexs[i].clone() });
    }
    Ok(rows)
}

/// Condivisa da `companies_additional_buds.csv` e `companies_additional_regexs.csv` — stessa
/// forma (colonna indice `Company name` + una colonna valore), stesso controllo incrociato.
fn load_additional(
    input_db_directory: &Path,
    subdir: &str,
    file_name: &str,
    value_column: &'static str,
    company_names: &HashSet<String>,
) -> Result<Vec<(String, String)>, CompaniesDbError> {
    let path = input_db_directory.join(subdir).join(file_name);
    let table = read_table(&path)?;
    let companies = table.required_str_column("Company name")?;
    let values = table.required_str_column(value_column)?;
    companies
        .into_iter()
        .zip(values)
        .map(|(company_name, value)| {
            if !company_names.contains(&company_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "company", value: company_name });
            }
            Ok((company_name, value))
        })
        .collect()
}

fn load_lists(input_db_directory: &Path) -> Result<HashSet<String>, CompaniesDbError> {
    let path = input_db_directory.join(LISTS_DIR).join("lists.csv");
    let table = read_table(&path)?;
    let names = table.required_str_column("Name")?;
    let institutions = table.required_str_column("Institution")?;
    let dates = table.required_str_column("Date")?;

    let mut seen = HashSet::new();
    for (i, name) in names.iter().enumerate() {
        if !seen.insert(name.clone()) {
            return Err(CompaniesDbError::Duplicate { path: path.clone(), kind: "list name", value: name.clone() });
        }
        require_valid_date(&dates[i], &format!("{}, list '{name}'", path.display()))?;
        let _ = &institutions[i]; // presenza/tipo già garantiti da `required_str_column`
    }
    Ok(seen)
}

fn load_company_to_list(
    input_db_directory: &Path,
    list_names: &HashSet<String>,
    company_names: &HashSet<String>,
) -> Result<Vec<(String, String)>, CompaniesDbError> {
    let path = input_db_directory.join(LISTS_DIR).join("company_to_list.csv");
    let table = read_table(&path)?;
    let lists = table.required_str_column("List name")?;
    let companies = table.required_str_column("Company name")?;
    lists
        .into_iter()
        .zip(companies)
        .map(|(list_name, company_name)| {
            if !list_names.contains(&list_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "list", value: list_name });
            }
            if !company_names.contains(&company_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "company", value: company_name });
            }
            Ok((list_name, company_name))
        })
        .collect()
}

fn load_markets(input_db_directory: &Path) -> Result<HashSet<String>, CompaniesDbError> {
    let path = input_db_directory.join(COMPANIES_DIR).join("markets.csv");
    let table = read_table(&path)?;
    let names = table.required_str_column("Name")?;
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name.clone()) {
            return Err(CompaniesDbError::Duplicate { path: path.clone(), kind: "market name", value: name });
        }
    }
    Ok(seen)
}

fn load_tickers(
    input_db_directory: &Path,
    market_names: &HashSet<String>,
    company_names: &HashSet<String>,
) -> Result<Vec<(String, String, String)>, CompaniesDbError> {
    let path = input_db_directory.join(COMPANIES_DIR).join("tickers.csv");
    let table = read_table(&path)?;
    let markets = table.required_str_column("Market name")?;
    let companies = table.required_str_column("Company name")?;
    let symbols = table.required_str_column("Symbol")?;

    // `\A`/`\z` (non `^`/`$`): ancorati all'intera stringa, non a un'eventuale riga interna.
    let symbol_re = onig::Regex::new(r"\A[A-Z]{2,6}\z")
        .expect("pattern letterale fisso, valido per costruzione -- verificato a compile time");
    (0..markets.len())
        .map(|i| {
            let (market_name, company_name, symbol) = (markets[i].clone(), companies[i].clone(), symbols[i].clone());
            if !market_names.contains(&market_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "market", value: market_name });
            }
            if !company_names.contains(&company_name) {
                return Err(CompaniesDbError::UnknownReference { path: path.clone(), kind: "company", value: company_name });
            }
            if symbol_re.find(&symbol).is_none() {
                return Err(CompaniesDbError::InvalidTickerSymbol { path: path.clone(), symbol });
            }
            Ok((market_name, company_name, symbol))
        })
        .collect()
}

#[derive(Default)]
struct CompanyAggregate {
    buds: Vec<String>,
    regexs: Vec<String>,
    list_names: Vec<String>,
    symbols: Vec<String>,
}

/// `get_target_companies` del riferimento, senza il confine Python: restituisce gli input grezzi
/// (nome/bud/regex/symbol per azienda, già filtrati per lista bersaglio e aggregati), non ancora
/// compilati in `CompanyMatchInfos`.
pub fn load_target_companies(
    input_db_directory: &Path,
    target_lists: &[String],
) -> Result<Vec<TargetCompanyInput>, CompaniesDbError> {
    let companies = load_companies(input_db_directory)?;
    let company_names: HashSet<String> = companies.iter().map(|c| c.name.clone()).collect();

    let additional_buds = load_additional(input_db_directory, COMPANIES_DIR, "companies_additional_buds.csv", "Bud", &company_names)?;
    let additional_regexs =
        load_additional(input_db_directory, COMPANIES_DIR, "companies_additional_regexs.csv", "Regex", &company_names)?;
    let list_names = load_lists(input_db_directory)?;
    let company_to_list = load_company_to_list(input_db_directory, &list_names, &company_names)?;
    let market_names = load_markets(input_db_directory)?;
    let tickers = load_tickers(input_db_directory, &market_names, &company_names)?;

    let mut aggregates: HashMap<String, CompanyAggregate> =
        company_names.iter().map(|n| (n.clone(), CompanyAggregate::default())).collect();
    for company in &companies {
        // Invariante: `agg` esiste sempre per `company.name`, perché `aggregates` è stato
        // popolato da `company_names`, che è derivato dalle stesse `companies`.
        let agg = aggregates.get_mut(&company.name).expect("company name comes from the same set that seeded `aggregates`");
        if let Some(bud) = &company.bud {
            agg.buds.push(bud.clone());
        }
        if let Some(regex) = &company.regex {
            agg.regexs.push(regex.clone());
        }
    }
    for (company_name, bud) in additional_buds {
        // Invariante: `load_additional` ha già rifiutato ogni riga il cui nome non è in
        // `company_names` (`UnknownReference`), quindi la chiave è sempre presente qui.
        aggregates.get_mut(&company_name).expect("checked against company_names by load_additional").buds.push(bud);
    }
    for (company_name, regex) in additional_regexs {
        aggregates.get_mut(&company_name).expect("checked against company_names by load_additional").regexs.push(regex);
    }
    for (list_name, company_name) in company_to_list {
        aggregates.get_mut(&company_name).expect("checked against company_names by load_company_to_list").list_names.push(list_name);
    }
    for (_market_name, company_name, symbol) in tickers {
        aggregates.get_mut(&company_name).expect("checked against company_names by load_tickers").symbols.push(symbol);
    }

    // Ordine di `companies.csv` preservato, non alfabetico -- vedi il doc-comment del modulo:
    // `match_company`/`match_fast` e' un algoritmo "primo prefisso che matcha vince".
    let target_lists: HashSet<&str> = target_lists.iter().map(String::as_str).collect();
    let result: Vec<TargetCompanyInput> = companies
        .into_iter()
        .filter_map(|company| {
            let agg = aggregates.remove(&company.name).expect("removed at most once per unique company name");
            if !agg.list_names.iter().any(|l| target_lists.contains(l.as_str())) {
                return None;
            }
            Some(TargetCompanyInput { name: company.name, regexs: agg.regexs, symbols: agg.symbols, buds: agg.buds })
        })
        .collect();
    Ok(result)
}

/// `PLAN.md` §9: thin wrapper su `CompanyMatchInfos::compile_from_target_companies` (già in
/// questo crate da M4, `formats_utils::text_filter::matcher` — nessun porting nuovo lì,
/// `M9-implementation-plan.md` §0 Q1).
pub fn compile_target_companies(
    input_db_directory: &Path,
    target_lists: &[String],
) -> Result<Vec<CompanyMatchInfos>, CompileTargetCompaniesError> {
    let companies = load_target_companies(input_db_directory, target_lists)?;
    Ok(CompanyMatchInfos::compile_from_target_companies(companies)?)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::formats_utils::text_filter::matcher::TargetCompanyInput;

    /// Stessa fixture del riferimento Rust (`freeports_core/src/input/companies_db.rs`): due
    /// aziende, ("Coca Cola" in lista TEST, "BlackRock" in lista OTHER), ciascuna con un bud/regex
    /// proprio più uno aggiuntivo, e un ticker ciascuna.
    struct Fixture {
        dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(dir.path().join(COMPANIES_DIR)).unwrap();
            std::fs::create_dir_all(dir.path().join(LISTS_DIR)).unwrap();
            let f = Fixture { dir };
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,,\nBlackRock,black,^black ?rock\n");
            f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\nBlackRock,rock\n");
            f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\nBlackRock,rock\n");
            f.write(COMPANIES_DIR, "markets.csv", "Name\nprimary\n");
            f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\nprimary,Coca Cola,COC\nprimary,BlackRock,BLK\n");
            f.write(LISTS_DIR, "lists.csv", "Name,Institution,Date\nTEST,FREEPORTS,2025-01-01\nOTHER,FREEPORTS,2025-01-01\n");
            f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,Coca Cola\nOTHER,BlackRock\n");
            f
        }

        fn write(&self, subdir: &str, file: &str, content: &str) {
            std::fs::write(self.dir.path().join(subdir).join(file), content).unwrap();
        }

        fn root(&self) -> PathBuf {
            self.dir.path().to_path_buf()
        }
    }

    fn names(companies: &[TargetCompanyInput]) -> Vec<&str> {
        companies.iter().map(|c| c.name.as_str()).collect()
    }

    fn tl(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    mod happy_path {
        use super::*;

        #[test]
        fn filters_to_only_companies_in_the_target_list() {
            let f = Fixture::new();
            let result = load_target_companies(&f.root(), &tl(&["TEST"])).unwrap();
            assert_eq!(names(&result), vec!["Coca Cola"]);
        }

        #[test]
        fn own_and_additional_buds_and_regexs_are_both_included() {
            let f = Fixture::new();
            let result = load_target_companies(&f.root(), &tl(&["OTHER"])).unwrap();
            let blackrock = &result[0];
            assert_eq!(blackrock.buds, vec!["black".to_string(), "rock".to_string()]);
            assert_eq!(blackrock.regexs, vec!["^black ?rock".to_string(), "rock".to_string()]);
        }

        #[test]
        fn ticker_symbols_are_aggregated() {
            let f = Fixture::new();
            let result = load_target_companies(&f.root(), &tl(&["TEST"])).unwrap();
            assert_eq!(result[0].symbols, vec!["COC".to_string()]);
        }

        #[test]
        fn multiple_target_lists_include_companies_from_either_without_duplicates() {
            let f = Fixture::new();
            let result = load_target_companies(&f.root(), &tl(&["TEST", "OTHER"])).unwrap();
            // Ordine di `companies.csv`, non alfabetico -- vedi il doc-comment del modulo.
            assert_eq!(names(&result), vec!["Coca Cola", "BlackRock"]);
        }

        #[test]
        fn companies_are_returned_in_companies_csv_file_order_not_alphabetical() {
            // Un nome più corto che è prefisso di uno più lungo ("Israel Government" dentro
            // "Israel Government International") deve restare nell'ordine del file, non essere
            // riordinato alfabeticamente -- altrimenti `match_company` (primo prefisso che
            // matcha vince) sceglierebbe l'azienda sbagliata.
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nIsrael Government International,,\nIsrael Government,,\n");
            f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\n");
            f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\n");
            f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\n");
            f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,Israel Government International\nTEST,Israel Government\n");
            let result = load_target_companies(&f.root(), &tl(&["TEST"])).unwrap();
            assert_eq!(names(&result), vec!["Israel Government International", "Israel Government"]);
        }

        #[test]
        fn a_target_list_with_no_member_companies_returns_empty() {
            let f = Fixture::new();
            f.write(LISTS_DIR, "lists.csv", "Name,Institution,Date\nEMPTY,FREEPORTS,2025-01-01\n");
            f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\n");
            let result = load_target_companies(&f.root(), &tl(&["EMPTY"])).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn no_target_lists_at_all_returns_empty_not_an_error() {
            let f = Fixture::new();
            let result = load_target_companies(&f.root(), &[]).unwrap();
            assert!(result.is_empty());
        }

        #[test]
        fn a_company_never_referenced_in_company_to_list_is_simply_never_selected() {
            let f = Fixture::new();
            let result = load_target_companies(&f.root(), &tl(&["TEST"])).unwrap();
            assert!(!names(&result).contains(&"BlackRock"));
        }

        #[test]
        fn accented_mixed_case_names_pass_bud_and_regex_validation() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nAP Møller Mærsk,maersk,\\bmaersk\n");
            f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\n");
            f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\n");
            f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\n");
            f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,AP Møller Mærsk\n");
            let result = load_target_companies(&f.root(), &tl(&["TEST"])).unwrap();
            assert_eq!(names(&result), vec!["AP Møller Mærsk"]);
        }
    }

    mod companies_csv_validation {
        use super::*;

        #[test]
        fn duplicate_company_name_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,,\nCoca Cola,,\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn bud_not_already_normalized_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,COCA,\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn bud_not_contained_in_name_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,pepsi,\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn regex_not_matching_name_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,,\\bpepsi\\b\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn invalid_regex_syntax_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,,(unclosed\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn regex_matching_only_a_later_word_in_the_name_is_accepted_unanchored() {
            // Pinna il fix dell'ancoraggio: `\bmaersk` deve essere accettato per un'azienda il
            // cui nome normalizzato e' "ap moller maersk" -- "maersk" e' l'*ultima* parola, quindi
            // un match ancorato in posizione 0 (comportamento del riferimento Python, di fatto
            // rotto da un secondo bug indipendente) lo rifiuterebbe.
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nAP Moller Maersk,,\\bmaersk\n");
            f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\n");
            f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\n");
            f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\n");
            f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,AP Moller Maersk\n");
            let result = load_target_companies(&f.root(), &tl(&["TEST"])).unwrap();
            assert_eq!(names(&result), vec!["AP Moller Maersk"]);
        }

        #[test]
        fn missing_name_column_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Bud,Regex\n,\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }
    }

    mod cross_reference_checks {
        use super::*;

        #[test]
        fn additional_bud_referencing_an_unknown_company_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\nNonexistent Co,foo\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn additional_regex_referencing_an_unknown_company_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\nNonexistent Co,foo\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn company_to_list_referencing_an_unknown_list_is_rejected() {
            let f = Fixture::new();
            f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nNOPE,Coca Cola\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn company_to_list_referencing_an_unknown_company_is_rejected() {
            let f = Fixture::new();
            f.write(LISTS_DIR, "company_to_list.csv", "List name,Company name\nTEST,Nonexistent Co\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn ticker_referencing_an_unknown_market_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\nnowhere,Coca Cola,COC\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn ticker_referencing_an_unknown_company_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\nprimary,Nonexistent Co,COC\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }
    }

    mod lists_and_markets_structural_validation {
        use super::*;

        #[test]
        fn duplicate_list_name_is_rejected() {
            let f = Fixture::new();
            f.write(LISTS_DIR, "lists.csv", "Name,Institution,Date\nTEST,A,2025-01-01\nTEST,B,2025-01-01\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn duplicate_market_name_is_rejected() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "markets.csv", "Name\nprimary\nprimary\n");
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test_case::test_case("2025-13-01"; "month out of range")]
        #[test_case::test_case("2025-01-32"; "day out of range")]
        #[test_case::test_case("not-a-date"; "not numeric")]
        #[test_case::test_case("2025-01"; "missing day")]
        fn invalid_dates_are_rejected(bad_date: &str) {
            let f = Fixture::new();
            f.write(LISTS_DIR, "lists.csv", &format!("Name,Institution,Date\nTEST,FREEPORTS,{bad_date}\nOTHER,FREEPORTS,2025-01-01\n"));
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }
    }

    mod ticker_symbol_shape {
        use super::*;

        #[test_case::test_case("COC"; "valid uppercase")]
        #[test_case::test_case("AB"; "valid at minimum length")]
        #[test_case::test_case("ABCDEF"; "valid at maximum length")]
        fn valid_ticker_symbols_are_accepted(symbol: &str) {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "tickers.csv", &format!("Market name,Company name,Symbol\nprimary,Coca Cola,{symbol}\n"));
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_ok());
        }

        #[test_case::test_case("coc"; "lowercase")]
        #[test_case::test_case("A"; "too short")]
        #[test_case::test_case("ABCDEFG"; "too long")]
        #[test_case::test_case("CO1"; "contains a digit")]
        fn invalid_ticker_symbols_are_rejected(symbol: &str) {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "tickers.csv", &format!("Market name,Company name,Symbol\nprimary,Coca Cola,{symbol}\n"));
            assert!(load_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }
    }

    mod compile_target_companies_wrapper {
        use super::*;

        #[test]
        fn compiles_the_same_companies_load_target_companies_would_return() {
            let f = Fixture::new();
            let compiled = compile_target_companies(&f.root(), &tl(&["TEST", "OTHER"])).unwrap();
            assert_eq!(compiled.len(), 2);
        }

        #[test]
        fn a_load_failure_propagates_through_the_wrapper() {
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,,\nCoca Cola,,\n");
            assert!(compile_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }

        #[test]
        fn an_invalid_regex_that_passes_loading_but_fails_pattern_compilation_propagates() {
            // `require_regex_matches_name` uses `onig::Regex::new` too, so a syntactically
            // invalid pattern is normally caught at `load_target_companies` time already; this
            // documents that whichever stage catches it, the wrapper surfaces a single error
            // type covering both failure modes (`CompileTargetCompaniesError`).
            let f = Fixture::new();
            f.write(COMPANIES_DIR, "companies.csv", "Name,Bud,Regex\nCoca Cola,,(unclosed\n");
            assert!(compile_target_companies(&f.root(), &tl(&["TEST"])).is_err());
        }
    }

    mod stress {
        use super::*;

        #[test]
        fn stress_500_companies_across_10_lists_filters_and_aggregates_correctly() {
            let f = Fixture::new();
            let mut companies_csv = String::from("Name,Bud,Regex\n");
            let mut company_to_list_csv = String::from("List name,Company name\n");
            let mut lists_csv = String::from("Name,Institution,Date\n");
            for l in 0..10 {
                lists_csv.push_str(&format!("List{l},FREEPORTS,2025-01-01\n"));
            }
            for i in 0..500 {
                let name = format!("Company{i}");
                companies_csv.push_str(&format!("{name},,\n"));
                company_to_list_csv.push_str(&format!("List{},{name}\n", i % 10));
            }
            f.write(COMPANIES_DIR, "companies.csv", &companies_csv);
            f.write(LISTS_DIR, "lists.csv", &lists_csv);
            f.write(LISTS_DIR, "company_to_list.csv", &company_to_list_csv);
            f.write(COMPANIES_DIR, "companies_additional_buds.csv", "Company name,Bud\n");
            f.write(COMPANIES_DIR, "companies_additional_regexs.csv", "Company name,Regex\n");
            f.write(COMPANIES_DIR, "tickers.csv", "Market name,Company name,Symbol\n");

            let result = load_target_companies(&f.root(), &tl(&["List3"])).unwrap();
            assert_eq!(result.len(), 50);
            assert!(result.iter().all(|c| c.name.starts_with("Company")));
            assert!(result.iter().any(|c| c.name == "Company3"));
            assert!(!result.iter().any(|c| c.name == "Company4"));
        }
    }

    /// Copertura di errore "per varietà di causa": ogni file mancante del tutto (non solo
    /// malformato) deve fallire con un errore, mai un panic (`PLAN.md` §2 principio 4).
    mod missing_files {
        use super::*;

        #[test_case::test_case("companies/companies.csv"; "companies csv")]
        #[test_case::test_case("companies/markets.csv"; "markets csv")]
        #[test_case::test_case("companies/tickers.csv"; "tickers csv")]
        #[test_case::test_case("lists/lists.csv"; "lists csv")]
        #[test_case::test_case("lists/company_to_list.csv"; "company to list csv")]
        fn a_missing_required_csv_file_is_a_typed_error_not_a_panic(relative: &str) {
            let f = Fixture::new();
            std::fs::remove_file(f.root().join(relative)).unwrap();
            let result = std::panic::catch_unwind(|| load_target_companies(&f.root(), &tl(&["TEST"])));
            assert!(result.is_ok(), "must not panic for a missing {relative}");
            assert!(result.unwrap().is_err());
        }
    }
}
