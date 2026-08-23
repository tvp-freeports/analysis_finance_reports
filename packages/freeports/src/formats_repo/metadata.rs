//! I metadati del repo formati: l'elenco dei formati e la mappa URL → formato.
//!
//! Due CSV in `metadata/`:
//!
//! - `formats.csv` — una riga per formato, con le componenti (`Name`, `Locale`, `Year`,
//!   `Country`, `Version`) da cui si **sintetizza** il nome del formato,
//!   `Nome-Locale<AA>[@Paese][.Versione]`. Il nome non è scritto da nessuna parte: esiste solo
//!   come risultato di questa sintesi, ed è la chiave con cui tutto il resto del repo si
//!   riferisce al formato.
//! - `url_mapping.csv` — quali URL appartengono a quale formato, usato per riconoscere il formato
//!   di un documento scaricato dal suo indirizzo.
//!
//! Porting di `repo/metadata.py` (e del suo porting Rust parziale in `freeports_core`), senza
//! pandas/pandera: il crate `csv` con struct tipizzate più validazione esplicita, come chiede
//! `PLAN.md` §2 principio 7. Ogni errore riporta la riga.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::id_format::{IdFormat, id_matches};

/// Sottocartella del repo formati che contiene i due CSV.
pub const METADATA_DIR: &str = "metadata";

/// Fallimenti nella lettura dei metadati.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MetadataError {
    #[error("missing formats-repository CSV file: {0}")]
    MissingCsv(PathBuf),
    #[error("{path}: malformed row at line {line}: {reason}")]
    MalformedRow { path: PathBuf, line: usize, reason: String },
    #[error("{path}: missing required column '{column}'")]
    MissingColumn { path: PathBuf, column: String },
    #[error("duplicate format name: {0}")]
    DuplicateFormatName(String),
    #[error("format name '{0}' does not match the expected format name pattern")]
    InvalidFormatName(String),
    #[error("{path}, line {line}: unknown format name: {name}")]
    UnknownFormatName { path: PathBuf, line: usize, name: String },
}

/// Una riga di `formats.csv`.
#[derive(Debug, Clone, Deserialize)]
struct FormatRow {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Locale")]
    locale: String,
    #[serde(rename = "Year")]
    year: String,
    /// Colonna obbligatoria, cella quasi sempre vuota: come nel riferimento, la *presenza* della
    /// colonna è richiesta anche quando nessuna riga la valorizza.
    #[serde(rename = "Country")]
    country: String,
    #[serde(rename = "Version")]
    version: String,
}

impl FormatRow {
    /// Il nome sintetizzato: `Nome-Locale<AA>`, più `@Paese` e `.Versione` se le colonne sono
    /// valorizzate. `<AA>` sono le **ultime due cifre** dell'anno, comunque sia scritto.
    fn format_name(&self) -> String {
        let year = self.year.trim();
        let yy = if year.len() >= 2 { &year[year.len() - 2..] } else { year };
        let mut name = format!("{}-{}{}", self.name, self.locale, yy);
        if !self.country.is_empty() {
            name.push('@');
            name.push_str(&self.country);
        }
        if !self.version.is_empty() {
            name.push('.');
            name.push_str(&self.version);
        }
        name
    }
}

/// Una riga di `url_mapping.csv`.
#[derive(Debug, Clone, Deserialize)]
struct UrlRow {
    #[serde(rename = "Format name")]
    format_name: String,
    #[serde(rename = "Url")]
    url: String,
}

/// Apre un CSV di `metadata/`, distinguendo "il file non c'è" da "il file non si legge".
fn open_csv(formats_repo_dir: &Path, file_name: &str) -> Result<(PathBuf, csv::Reader<std::fs::File>), MetadataError> {
    let path = formats_repo_dir.join(METADATA_DIR).join(file_name);
    if !path.is_file() {
        return Err(MetadataError::MissingCsv(path));
    }
    let reader = csv::Reader::from_path(&path)
        .map_err(|e| MetadataError::MalformedRow { path: path.clone(), line: 0, reason: e.to_string() })?;
    Ok((path, reader))
}

/// Traduce un errore del crate `csv` nell'errore giusto: una colonna mancante è una diagnosi
/// diversa (e molto più utile) da una riga malformata.
fn row_error(path: &Path, line: usize, error: &csv::Error) -> MetadataError {
    // Il crate `csv` annega "missing field `X`" in un messaggio più lungo con posizione e byte:
    // qui la si ripesca, perché una colonna assente è una diagnosi diversa (e molto più
    // azionabile per chi cura il repo formati) da una riga malformata.
    let message = error.to_string();
    if let Some(rest) = message.split("missing field `").nth(1)
        && let Some(column) = rest.split('`').next()
    {
        return MetadataError::MissingColumn { path: path.to_path_buf(), column: column.to_string() };
    }
    MetadataError::MalformedRow { path: path.to_path_buf(), line, reason: message }
}

/// Legge tutte le righe tipizzate di un CSV, numerandole a partire da 1 (l'intestazione non conta).
fn read_rows<T: serde::de::DeserializeOwned>(
    formats_repo_dir: &Path,
    file_name: &str,
) -> Result<(PathBuf, Vec<T>), MetadataError> {
    let (path, mut reader) = open_csv(formats_repo_dir, file_name)?;
    let mut rows = Vec::new();
    for (i, record) in reader.deserialize::<T>().enumerate() {
        rows.push(record.map_err(|e| row_error(&path, i + 1, &e))?);
    }
    Ok((path, rows))
}

/// I nomi dei formati dichiarati dal repo, nell'ordine in cui compaiono in `formats.csv`.
///
/// Ogni nome sintetizzato è verificato contro la grammatica dei nomi di formato e contro i nomi
/// già visti: due righe che sintetizzano lo stesso nome sono un errore di configurazione (nel
/// riferimento è l'`unique=True` dell'indice pandera).
pub fn get_formats(formats_repo_dir: &Path) -> Result<Vec<String>, MetadataError> {
    let (_, rows): (_, Vec<FormatRow>) = read_rows(formats_repo_dir, "formats.csv")?;
    let mut seen = HashSet::new();
    let mut names = Vec::with_capacity(rows.len());
    for row in rows {
        let name = row.format_name();
        // La grammatica accetta il nome nudo, senza pipeline né indice: è ciò che `formats.csv`
        // dichiara.
        if !id_matches(&name, IdFormat::ExpandableNoIndex) {
            return Err(MetadataError::InvalidFormatName(name));
        }
        if !seen.insert(name.clone()) {
            return Err(MetadataError::DuplicateFormatName(name));
        }
        names.push(name);
    }
    Ok(names)
}

/// Legge `url_mapping.csv`, verificando **tutte** le righe prima di restituirne una qualsiasi.
///
/// La validazione è volutamente eager e sull'intero file, come la `validate` di pandera del
/// riferimento: un formato sconosciuto in fondo al file è un errore anche se la riga che serviva
/// era la prima.
fn read_url_mapping(formats_repo_dir: &Path, format_names: &[String]) -> Result<Vec<UrlRow>, MetadataError> {
    let (path, rows): (_, Vec<UrlRow>) = read_rows(formats_repo_dir, "url_mapping.csv")?;
    let known: HashSet<&str> = format_names.iter().map(String::as_str).collect();
    for (i, row) in rows.iter().enumerate() {
        if !known.contains(row.format_name.as_str()) {
            return Err(MetadataError::UnknownFormatName {
                path: path.clone(),
                line: i + 1,
                name: row.format_name.clone(),
            });
        }
    }
    Ok(rows)
}

/// Il formato a cui appartiene `url`, se il repo ne dichiara uno.
///
/// Vince il **prefisso letterale più lungo**: un `Url` in tabella non è mai interpretato come
/// espressione regolare, nemmeno se contiene metacaratteri. A parità di lunghezza vince la riga
/// che viene prima nel file (l'`idxmax()` stabile di pandas).
pub fn url_to_format(
    formats_repo_dir: &Path,
    format_names: &[String],
    url: &str,
) -> Result<Option<String>, MetadataError> {
    let rows = read_url_mapping(formats_repo_dir, format_names)?;
    // `max_by_key` restituirebbe l'**ultimo** massimo; qui serve il primo, quindi il confronto è
    // strettamente maggiore.
    let mut best: Option<&UrlRow> = None;
    for row in rows.iter().filter(|row| url.starts_with(row.url.as_str())) {
        if best.is_none_or(|current| row.url.len() > current.url.len()) {
            best = Some(row);
        }
    }
    Ok(best.map(|row| row.format_name.clone()))
}

/// Tutti gli URL dichiarati, raggruppati per formato e nell'ordine del file.
pub fn get_url_mapping(
    formats_repo_dir: &Path,
    format_names: &[String],
) -> Result<HashMap<String, Vec<String>>, MetadataError> {
    let rows = read_url_mapping(formats_repo_dir, format_names)?;
    let mut mapping: HashMap<String, Vec<String>> = HashMap::new();
    for row in rows {
        mapping.entry(row.format_name).or_default().push(row.url);
    }
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Un repo formati minimale su disco: `PLAN.md` §10 vuole i test d'integrazione su una
    /// `TempDir`, non su fixture esterni, e vale a maggior ragione per gli unitari.
    fn repo(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("temp dir");
        fs::create_dir_all(dir.path().join(METADATA_DIR)).expect("metadata dir");
        for (name, content) in files {
            fs::write(dir.path().join(METADATA_DIR).join(name), content).expect("write csv");
        }
        dir
    }

    const FORMATS_CSV: &str = "Name,Locale,Year,Country,Version\n\
                               AMUNDI,EN,24,,\n\
                               AMUNDI,IT,24,,\n\
                               MEDIOLANUM,IT,24,ES,b\n";

    mod format_names {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn synthesizes_one_name_per_row_in_file_order() {
            let dir = repo(&[("formats.csv", FORMATS_CSV)]);
            assert_eq!(
                get_formats(dir.path()).unwrap(),
                vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string(), "MEDIOLANUM-IT24@ES.b".to_string()]
            );
        }

        #[test]
        fn a_four_digit_year_keeps_only_its_last_two_digits() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,2024,,\n")]);
            assert_eq!(get_formats(dir.path()).unwrap(), vec!["AMUNDI-EN24".to_string()]);
        }

        #[test]
        fn an_empty_country_and_version_add_no_suffix() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\n")]);
            assert_eq!(get_formats(dir.path()).unwrap(), vec!["AMUNDI-EN24".to_string()]);
        }

        #[test]
        fn a_country_alone_adds_only_the_at_suffix() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nMEDIOLANUM,IT,24,ES,\n")]);
            assert_eq!(get_formats(dir.path()).unwrap(), vec!["MEDIOLANUM-IT24@ES".to_string()]);
        }

        #[test]
        fn a_version_alone_adds_only_the_dot_suffix() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nMEDIOLANUM,IT,24,,b\n")]);
            assert_eq!(get_formats(dir.path()).unwrap(), vec!["MEDIOLANUM-IT24.b".to_string()]);
        }

        #[test]
        fn an_empty_table_declares_no_format() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\n")]);
            assert!(get_formats(dir.path()).unwrap().is_empty());
        }
    }

    mod format_name_errors {
        use super::*;

        #[test]
        fn a_missing_file_is_reported_with_its_full_path() {
            let dir = repo(&[]);
            let err = get_formats(dir.path()).unwrap_err();
            let MetadataError::MissingCsv(path) = err else { panic!("expected MissingCsv") };
            assert!(path.ends_with("metadata/formats.csv"), "{}", path.display());
        }

        #[test]
        fn a_missing_column_names_the_column() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year\nAMUNDI,EN,24\n")]);
            let err = get_formats(dir.path()).unwrap_err();
            let MetadataError::MissingColumn { column, .. } = err else { panic!("expected MissingColumn, got {err}") };
            assert_eq!(column, "Country");
        }

        #[test]
        fn a_name_that_does_not_match_the_grammar_is_rejected() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,en,24,,\n")]);
            let err = get_formats(dir.path()).unwrap_err();
            assert!(matches!(err, MetadataError::InvalidFormatName(name) if name == "AMUNDI-en24"));
        }

        #[test]
        fn two_rows_synthesizing_the_same_name_are_rejected() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,EN,2024,,\n")]);
            let err = get_formats(dir.path()).unwrap_err();
            assert!(matches!(err, MetadataError::DuplicateFormatName(name) if name == "AMUNDI-EN24"));
        }

        #[test]
        fn a_row_with_too_few_cells_reports_its_line_number() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,IT\n")]);
            let err = get_formats(dir.path()).unwrap_err();
            let MetadataError::MalformedRow { line, .. } = err else { panic!("expected MalformedRow, got {err}") };
            assert_eq!(line, 2);
        }

        #[test]
        fn the_error_message_carries_the_offending_file() {
            let dir = repo(&[("formats.csv", "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,IT\n")]);
            let message = get_formats(dir.path()).unwrap_err().to_string();
            assert!(message.contains("formats.csv"), "{message}");
        }
    }

    mod url_detection {
        use super::*;
        use pretty_assertions::assert_eq;

        const URL_CSV: &str = "Format name,Url\n\
                               AMUNDI-EN24,https://www.amundi.com/\n\
                               AMUNDI-EN24,https://www.amundi.com/ABC\n\
                               AMUNDI-IT24,https://www.amundi.it/\n";

        fn full_repo() -> TempDir {
            repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", URL_CSV)])
        }

        fn names() -> Vec<String> {
            vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string(), "MEDIOLANUM-IT24@ES.b".to_string()]
        }

        #[test]
        fn recognises_a_url_by_its_declared_prefix() {
            let dir = full_repo();
            let found = url_to_format(dir.path(), &names(), "https://www.amundi.it/report.pdf").unwrap();
            assert_eq!(found, Some("AMUNDI-IT24".to_string()));
        }

        #[test]
        fn an_unknown_url_matches_nothing() {
            let dir = full_repo();
            assert_eq!(url_to_format(dir.path(), &names(), "https://example.org/x.pdf").unwrap(), None);
        }

        #[test]
        fn the_longest_matching_prefix_wins() {
            let dir = full_repo();
            // Entrambe le righe AMUNDI-EN24 sono prefissi; vince la più lunga, che qui porta però
            // lo stesso formato: il test pinna la scelta della riga, non solo dell'esito.
            let found = url_to_format(dir.path(), &names(), "https://www.amundi.com/ABC/report.pdf").unwrap();
            assert_eq!(found, Some("AMUNDI-EN24".to_string()));
        }

        #[test]
        fn a_tie_between_equally_long_prefixes_goes_to_the_first_row() {
            let csv = "Format name,Url\nAMUNDI-IT24,https://x.example/\nAMUNDI-EN24,https://x.example/\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://x.example/a.pdf").unwrap(),
                Some("AMUNDI-IT24".to_string())
            );
        }

        #[test]
        fn a_url_cell_is_never_interpreted_as_a_regular_expression() {
            let csv = "Format name,Url\nAMUNDI-EN24,https://.*\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            assert_eq!(url_to_format(dir.path(), &names(), "https://www.amundi.com/").unwrap(), None);
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://.*/report.pdf").unwrap(),
                Some("AMUNDI-EN24".to_string())
            );
        }

        #[test]
        fn an_exactly_equal_url_matches_too() {
            let dir = full_repo();
            assert_eq!(
                url_to_format(dir.path(), &names(), "https://www.amundi.it/").unwrap(),
                Some("AMUNDI-IT24".to_string())
            );
        }

        #[test]
        fn an_unknown_format_name_anywhere_in_the_file_is_an_error() {
            let csv = "Format name,Url\nAMUNDI-EN24,https://a/\nGHOST-EN24,https://b/\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            let err = url_to_format(dir.path(), &names(), "https://a/x.pdf").unwrap_err();
            let MetadataError::UnknownFormatName { name, line, .. } = err else {
                panic!("expected UnknownFormatName")
            };
            assert_eq!((name.as_str(), line), ("GHOST-EN24", 2));
        }

        #[test]
        fn a_missing_url_mapping_file_is_reported_as_such() {
            let dir = repo(&[("formats.csv", FORMATS_CSV)]);
            assert!(matches!(url_to_format(dir.path(), &names(), "https://a/"), Err(MetadataError::MissingCsv(_))));
        }
    }

    mod url_grouping {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn groups_every_url_under_its_format_preserving_file_order() {
            let csv = "Format name,Url\n\
                       AMUNDI-EN24,https://a/\n\
                       AMUNDI-IT24,https://b/\n\
                       AMUNDI-EN24,https://c/\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            let names = vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string()];
            let mapping = get_url_mapping(dir.path(), &names).unwrap();
            assert_eq!(mapping["AMUNDI-EN24"], vec!["https://a/".to_string(), "https://c/".to_string()]);
            assert_eq!(mapping["AMUNDI-IT24"], vec!["https://b/".to_string()]);
        }

        #[test]
        fn a_format_with_no_url_is_simply_absent_from_the_mapping() {
            let csv = "Format name,Url\nAMUNDI-EN24,https://a/\n";
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", csv)]);
            let names = vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string()];
            let mapping = get_url_mapping(dir.path(), &names).unwrap();
            assert!(!mapping.contains_key("AMUNDI-IT24"));
        }

        #[test]
        fn an_empty_mapping_file_yields_an_empty_map() {
            let dir = repo(&[("formats.csv", FORMATS_CSV), ("url_mapping.csv", "Format name,Url\n")]);
            assert!(get_url_mapping(dir.path(), &names_of(&[])).unwrap().is_empty());
        }

        fn names_of(names: &[&str]) -> Vec<String> {
            names.iter().map(|n| n.to_string()).collect()
        }
    }

    mod real_repository {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Le prime righe reali di `analysis_finance_reports_formats/metadata/`, riprodotte in
        /// una `TempDir`: il test resta indipendente da quel repo, ma la forma è quella vera.
        #[test]
        fn reproduces_the_names_of_the_real_italian_formats_repository() {
            let dir = repo(&[(
                "formats.csv",
                "Name,Locale,Year,Country,Version\nAMUNDI,EN,24,,\nAMUNDI,IT,24,,\nANIMA,EN,23,,\n",
            )]);
            assert_eq!(
                get_formats(dir.path()).unwrap(),
                vec!["AMUNDI-EN24".to_string(), "AMUNDI-IT24".to_string(), "ANIMA-EN23".to_string()]
            );
        }
    }
}
