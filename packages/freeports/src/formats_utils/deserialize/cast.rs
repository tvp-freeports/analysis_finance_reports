//! Cast da testo a tipi: `to_int`, `to_float`, `to_date`, `to_currency`, `perc_to_float`, ...
//!
//! Port pressoche' diretto di `freeports_core/src/formats_utils/deserialize/cast.rs` (a sua
//! volta port di `_internals/formats/utils/deserialize/cast.py`), critico per i dati finanziari
//! (disambiguazione separatore migliaia/decimali) — vedi `PLAN.md` §2 di
//! `agent-memory/M4-implementation-plan.md` per la motivazione di ogni scelta qui sotto.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione,
//! stesso trattamento di `commons::date`/`formats_utils::pdf_extract::commons`):
//!
//! ```text
//! pub fn to_float(data: &str, keep_sign: bool) -> Result<f64, CastError>;
//! pub fn to_int(data: &str, keep_sign: bool) -> Result<i64, CastError>;
//! pub fn perc_to_float(perc: &str, norm: bool, keep_sign: bool) -> Result<f64, CastError>;
//! pub fn to_str(data: &str) -> String;                          // infallibile
//! pub fn to_currency(data: &str) -> Result<Currency, CastError>;
//! pub fn to_date(data: &str) -> Result<Date, CastError>;
//! pub fn to_int_en_month(text: &str) -> Result<u8, CastError>;
//! pub fn to_int_it_month(text: &str) -> Result<u8, CastError>;
//! pub fn to_date_with_en_month(text: &str) -> Result<Date, CastError>;
//! pub fn to_date_with_it_month(text: &str) -> Result<Date, CastError>;
//! pub fn is_numeric_shape(data: &str) -> bool;                  // infallibile
//! ```
//!
//! - `to_date`/`to_date_with_en_month`/`to_date_with_it_month` restituiscono
//!   [`crate::commons::date::Date`] (M1), **non** una tupla `(i32, u8, u8)` come il riferimento
//!   Rust: `Date::new` valida gia' anno/mese/giorno (bisestili inclusi), quindi una data ben
//!   formata ma calendarialmente impossibile (es. `"31.02.2025"`) e' un `Err`, cosa che il
//!   riferimento (tupla grezza) non garantiva.
//! - `to_int_en_month`/`to_int_it_month` restituiscono `u8`, non `u32` (si compongono
//!   direttamente col `month: u8` di `Date::new`).
//! - `CastError` e' un solo enum `thiserror` per il modulo (D10); la forma esatta delle sue
//!   varianti e' un dettaglio implementativo lasciato all'implementer/`critic` — i test qui sotto
//!   verificano solo `is_ok()`/`is_err()` e i valori `Ok`, mai una variante specifica.
//! - **Warning di forced-cast via `tracing`**: `to_float`/`to_int`/`perc_to_float` emettono
//!   `tracing::warn!(...)` quando il dato non e' gia' in forma numerica pulita (rispettivamente,
//!   per `perc_to_float`, quando un `%` letterale forza la normalizzazione nonostante
//!   `norm=false`) — senza impostare `page`/`company`/`field` (arriveranno da uno span aperto in
//!   M5+, `agent-memory/M4-implementation-plan.md` §2). `mod forced_cast_warnings` verifica solo
//!   che l'evento venga emesso (via un `tracing_subscriber::Layer` di test, non tramite
//!   `.log.csv`/`CsvLogLayer`: quel layer scrive una riga solo se l'evento porta almeno uno dei
//!   campi taggati, che qui non ci sono ancora — vedi `core::tracing_setup`).
//! - **`keep_sign`**: un `-` conta come segno genuino solo se e' il primo carattere non-whitespace
//!   dell'input (trimmato), immediatamente prima del contenuto numerico (es. `"-3.5"`, `"- 3.5"`);
//!   un `-` altrove (finale, incollato ad altro rumore, es. `"3.0 -"`, `"$100-"`) e' rumore,
//!   rimosso esattamente come oggi, e non contribuisce mai al segno. Con `keep_sign=true` un segno
//!   genuino nega il risultato; con `keep_sign=false` (comportamento di oggi) ogni `-` viene
//!   ignorato/rimosso, il risultato e' sempre non negativo quando il parsing riesce. `"-"` da solo
//!   (nessuna cifra) resta un errore in entrambi i casi. `perc_to_float` inoltra `keep_sign` alla
//!   chiamata interna a `to_float` dopo l'eventuale normalizzazione forzata dal `%` letterale.

use once_cell::sync::Lazy;
use onig::Regex;

use crate::commons::consts::Currency;
use crate::commons::date::{Date, DateError};
use crate::core::normalization;

// Oniguruma (via il crate `onig`), non il crate `regex` — stessa scelta gia' fatta in
// `text_filter::matcher` (`PLAN.md`/§1 di `agent-memory/M4-implementation-plan.md`). Un solo
// dettaglio comportamentale: la sintassi di default di `onig::Regex::new` tratta `^`/`$` come
// ancore di riga, non di stringa — irrilevante qui perche' ogni pattern sotto matcha solo
// stringhe gia' normalizzate a parola singola (mai `\n` incorporato).
// Ognuna delle quattro pattern qui sotto e' fissa e scritta a mano: nessuna e' costruita da
// input esterno, quindi un errore di compilazione sarebbe un bug di questo file, non una
// condizione runtime da gestire.
static NUMERIC_SHAPE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\d+([.,]\d+)*$").expect("fixed, hand-written pattern, valid onig regex"));
static NON_NUMERIC_CHARS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[^a-zA-Z.,0-9]+").expect("fixed, hand-written pattern, valid onig regex"));
static FLOAT_THOUSANDS_GROUPED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[1-9]\d{0,2}\.\d{3}(\.\d{3})+$").expect("fixed, hand-written pattern, valid onig regex")
});
static INT_THOUSANDS_GROUPED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[1-9]\d{0,2}(\.\d{3})+$").expect("fixed, hand-written pattern, valid onig regex")
});

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CastError {
    #[error("could not convert {data:?} to a number")]
    NotANumber { data: String },
    #[error("number {data:?} has a mantissa different from 0")]
    NonZeroMantissa { data: String },
    #[error("{data:?} is not a valid Currency")]
    UnknownCurrency { data: String },
    #[error("date string {data:?} is not in a recognized format")]
    UnrecognizedDateFormat { data: String },
    #[error("{text:?} is not a valid {locale} month name")]
    UnknownMonthName { text: String, locale: &'static str },
    #[error("{text:?} is not in \"DD MONTH YYYY\" format")]
    MalformedDayMonthYear { text: String },
    #[error(transparent)]
    InvalidDate(#[from] DateError),
}

/// True quando `data` (gia' word-normalizzato) ha gia' la forma di un numero semplice
/// (`123`, `1.234`, `1,234.567`, ...) — cioe' `to_float`/`to_int` lo useranno cosi' com'e' invece
/// di ripulirlo da rumore. E' anche il predicato su cui scatta il warning di forced-cast.
pub fn is_numeric_shape(data: &str) -> bool {
    NUMERIC_SHAPE.is_match(data)
}

/// Ripulisce il rumore non numerico da una stringa gia' word-normalizzata, a meno che non abbia
/// gia' la forma di un numero semplice. Logga un warning quando e' costretta a farlo.
fn force_numeric(data: &str) -> String {
    if is_numeric_shape(data) {
        data.to_string()
    } else {
        tracing::warn!(data, "trying to cast to number but found a non-numeric shape - forcing cast");
        NON_NUMERIC_CHARS.replace_all(data, "")
    }
}

/// Disambigua separatore migliaia/decimali quando sono presenti sia `.` sia `,`: il carattere
/// che compare per primo e' trattato come separatore delle migliaia e ogni sua occorrenza viene
/// rimossa; il separatore restante (se presente) diventa il punto decimale.
fn resolve_separators(data: &str) -> String {
    let mut data = data.to_string();
    let pos_dot = data.find('.');
    let pos_com = data.find(',');
    if let (Some(pd), Some(pc)) = (pos_dot, pos_com) {
        let first_char = if pd < pc { '.' } else { ',' };
        data = data.chars().filter(|&c| c != first_char).collect();
    }
    data.replace(',', ".")
}

/// Cast a `f64`, gestendo separatori delle migliaia e convenzioni miste `.`/`,`. Quando
/// `keep_sign` e' vero, un `-` genuino (il primo carattere non-whitespace, subito prima del
/// contenuto numerico) nega il risultato — vedi il doc-comment del modulo per il contratto
/// completo.
pub fn to_float(data: &str, keep_sign: bool) -> Result<f64, CastError> {
    let data = normalization::normalize_word(data, false);
    let negate = keep_sign && data.starts_with('-');
    let data = force_numeric(&data);
    let mut data = resolve_separators(&data);
    if FLOAT_THOUSANDS_GROUPED.is_match(&data) {
        data = data.replace('.', "");
    }
    let value = data.parse::<f64>().map_err(|_| CastError::NotANumber { data })?;
    Ok(if negate { -value } else { value })
}

/// Cast a `i64`, gestendo separatori delle migliaia e rifiutando una mantissa non nulla. Vedi
/// [`to_float`] per il contratto di `keep_sign`.
pub fn to_int(data: &str, keep_sign: bool) -> Result<i64, CastError> {
    let data = normalization::normalize_word(data, false);
    let negate = keep_sign && data.starts_with('-');
    let data = force_numeric(&data);
    let mut data = resolve_separators(&data);
    if INT_THOUSANDS_GROUPED.is_match(&data) {
        data = data.replace('.', "");
    }
    if let Some(pos_dot) = data.find('.') {
        let mantissa: i64 = data[pos_dot + 1..]
            .parse()
            .map_err(|_| CastError::NotANumber { data: data.clone() })?;
        if mantissa != 0 {
            return Err(CastError::NonZeroMantissa { data });
        }
        data.truncate(pos_dot);
    }
    let value = data.parse::<i64>().map_err(|_| CastError::NotANumber { data })?;
    Ok(if negate { -value } else { value })
}

/// Cast di una stringa percentuale (opzionalmente con `%` finale) a float. Quando `norm` e'
/// vero il risultato viene diviso per 100 — la divisione per 100 e' forzata a prescindere da
/// `norm` ogni volta che era presente un `%` letterale. `keep_sign` e' inoltrato cosi' com'e'
/// alla chiamata interna a [`to_float`], dopo l'eventuale rimozione del `%` letterale.
pub fn perc_to_float(perc: &str, norm: bool, keep_sign: bool) -> Result<f64, CastError> {
    let mut perc = normalization::normalize_word(perc, false);
    let mut norm = norm;
    if perc.contains('%') {
        if !norm {
            tracing::warn!(
                data = perc.as_str(),
                "percent sign forces normalization despite norm=false"
            );
        }
        perc = normalization::normalize_word(&perc.replace('%', ""), false);
        norm = true;
    }
    let f = to_float(&perc, keep_sign)?;
    Ok(if norm { f / 100.0 } else { f })
}

/// Normalizza una stringa rimuovendo whitespace iniziale/finale (case preservato).
pub fn to_str(data: &str) -> String {
    normalization::normalize_string(data, false)
}

/// Converte una stringa in [`Currency`] (accetta anche l'alias `EURO`) dopo normalizzazione e
/// maiuscolizzazione.
pub fn to_currency(data: &str) -> Result<Currency, CastError> {
    let normalized = normalization::normalize_word(data, false).to_uppercase();
    Currency::from_name(&normalized).ok_or(CastError::UnknownCurrency { data: normalized })
}

const DATE_FORMATS: &[&[&str]] = &[
    &["Y", "-", "m", "-", "d"], // %Y-%m-%d
    &["Y", "/", "m", "/", "d"], // %Y/%m/%d
    &["d", "/", "m", "/", "Y"], // %d/%m/%Y
    &["d", ".", "m", ".", "Y"], // %d.%m.%Y
    &["d", ".", "m", ".", "y"], // %d.%m.%y
    &["d", "/", "m", "/", "y"], // %d/%m/%y
    &["m", "-", "d", "-", "Y"], // %m-%d-%Y
    &["d", "-", "m", "-", "y"], // %d-%m-%y
    &["m", "/", "y"],           // %m/%y (giorno di default 1, come strptime)
];

/// Prova un insieme fisso di formati data (ISO, europeo, US, anno corto) in ordine. `%y` (anno
/// a due cifre) si espande con lo stesso pivot usato da Python `strptime`: 69-99 -> 1969-1999,
/// 00-68 -> 2000-2068.
pub fn to_date(data: &str) -> Result<Date, CastError> {
    let normalized = normalization::normalize_word(data, false);
    for fmt in DATE_FORMATS {
        if let Some((y, m, d)) = try_parse_date(&normalized, fmt) {
            return Ok(Date::new(y, m, d)?);
        }
    }
    Err(CastError::UnrecognizedDateFormat { data: normalized })
}

fn expand_2digit_year(y: i32) -> i32 {
    if y >= 69 { 1900 + y } else { 2000 + y }
}

fn try_parse_date(data: &str, fmt: &[&str]) -> Option<(i32, u8, u8)> {
    // Divide `data` sugli stessi separatori letterali usati dal formato, in ordine, e legge
    // ogni campo numerico secondo la sequenza di nomi di campo del formato (Y/y/m/d).
    let mut rest = data;
    let mut year: Option<i32> = None;
    let mut month: Option<u8> = None;
    let mut day: Option<u8> = None;
    let mut i = 0;
    while i < fmt.len() {
        let field = fmt[i];
        let is_last = i + 1 >= fmt.len();
        let sep = if is_last { None } else { Some(fmt[i + 1]) };
        let (value_str, remainder) = match sep {
            Some(s) => rest.split_once(s)?,
            None => (rest, ""),
        };
        if value_str.is_empty() || !value_str.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        // Rispecchia le larghezze di campo di CPython `_strptime`: `%Y` sono sempre 4 cifre,
        // `%y` sempre 2, `%m`/`%d` sono flessibili (1-2 cifre) — senza questo controllo formati
        // che condividono lo stesso separatore (`%Y/%m/%d` vs `%d/%m/%y`) non si distinguono.
        let width_ok = match field {
            "Y" => value_str.len() == 4,
            "y" => value_str.len() == 2,
            "m" | "d" => (1..=2).contains(&value_str.len()),
            _ => unreachable!("i nomi di campo sono limitati a Y/y/m/d in DATE_FORMATS"),
        };
        if !width_ok {
            return None;
        }
        let value: i32 = value_str.parse().ok()?;
        match field {
            "Y" => year = Some(value),
            "y" => year = Some(expand_2digit_year(value)),
            "m" => month = Some(u8::try_from(value).ok()?),
            "d" => day = Some(u8::try_from(value).ok()?),
            _ => unreachable!("i nomi di campo sono limitati a Y/y/m/d in DATE_FORMATS"),
        }
        rest = remainder;
        i += if sep.is_some() { 2 } else { 1 };
    }
    if !rest.is_empty() {
        return None;
    }
    // `%m/%y` non ha un campo giorno; strptime di Python di default lo imposta a 1.
    let day = day.unwrap_or(1);
    let year = year?;
    let month = month?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

const EN_MONTHS: &[&str] = &[
    "january", "february", "march", "april", "may", "june", "july", "august", "september",
    "october", "november", "december",
];
const IT_MONTHS: &[&str] = &[
    "gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre",
    "ottobre", "novembre", "dicembre",
];

fn month_index(text: &str, months: &[&str], locale: &'static str) -> Result<u8, CastError> {
    let needle = text.to_lowercase();
    let needle = needle.trim();
    months
        .iter()
        .position(|m| *m == needle)
        .map(|i| i as u8 + 1)
        .ok_or_else(|| CastError::UnknownMonthName { text: text.to_string(), locale })
}

/// Converte un nome di mese inglese (case-insensitive) nel suo indice 1-12.
pub fn to_int_en_month(text: &str) -> Result<u8, CastError> {
    month_index(text, EN_MONTHS, "en")
}

/// Converte un nome di mese italiano (case-insensitive) nel suo indice 1-12.
pub fn to_int_it_month(text: &str) -> Result<u8, CastError> {
    month_index(text, IT_MONTHS, "it")
}

fn parse_day_month_name_year(text: &str, months: &[&str], locale: &'static str) -> Result<Date, CastError> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let [day_s, month_s, year_s] = parts.as_slice() else {
        return Err(CastError::MalformedDayMonthYear { text: text.to_string() });
    };
    let day: u8 = day_s
        .parse()
        .map_err(|_| CastError::MalformedDayMonthYear { text: text.to_string() })?;
    let month = month_index(month_s, months, locale)?;
    let year: i32 = year_s
        .parse()
        .map_err(|_| CastError::MalformedDayMonthYear { text: text.to_string() })?;
    Ok(Date::new(year, month, day)?)
}

/// Analizza una stringa data `"DD MONTH YYYY"` con un nome di mese inglese.
pub fn to_date_with_en_month(text: &str) -> Result<Date, CastError> {
    parse_day_month_name_year(text, EN_MONTHS, "en")
}

/// Analizza una stringa data `"DD MONTH YYYY"` con un nome di mese italiano.
pub fn to_date_with_it_month(text: &str) -> Result<Date, CastError> {
    parse_day_month_name_year(text, IT_MONTHS, "it")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::consts::Currency;
    use crate::commons::date::Date;

    mod to_float {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("200", 200.0; "plain integer")]
        #[test_case("309.00", 309.0; "trailing zero decimal")]
        #[test_case("  090.070,00 ", 90070.0; "dot thousands comma decimal with whitespace")]
        #[test_case("4,500", 4.5; "single grouped triple is ambiguous, treated as a decimal")]
        fn matches_the_expected_value(input: &str, expected: f64) {
            assert_eq!(to_float(input, false).unwrap(), expected);
        }

        #[test]
        fn single_grouped_triple_is_treated_as_decimal_not_thousands() {
            // Only one ".XXX" group is ambiguous for floats specifically (could be a genuine
            // decimal) -- left as a decimal point rather than stripped.
            assert_eq!(to_float("1.234", false).unwrap(), 1.234);
        }

        #[test]
        fn two_grouped_triples_are_treated_as_thousands() {
            assert_eq!(to_float("1.234.567", false).unwrap(), 1_234_567.0);
        }

        #[test]
        fn strips_non_numeric_noise_but_keeps_letters() {
            assert_eq!(to_float("€1.234", false).unwrap(), 1.234);
            // Letters survive stripping (only [^a-zA-Z.,0-9]+ is dropped), so a unit suffix
            // still breaks the subsequent float parse.
            assert!(to_float("EUR 1.234 approx", false).is_err());
        }

        #[test]
        fn rejects_a_string_with_no_digits_at_all() {
            assert!(to_float("not a number", false).is_err());
        }
    }

    mod to_int {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("200", 200; "plain integer")]
        #[test_case("309.00", 309; "trailing zero decimal")]
        #[test_case("  090.070,00 ", 90070; "dot thousands comma decimal with whitespace")]
        #[test_case("4,500", 4500; "comma as thousands separator")]
        fn matches_the_expected_value(input: &str, expected: i64) {
            assert_eq!(to_int(input, false).unwrap(), expected);
        }

        #[test]
        fn rejects_nonzero_mantissa() {
            assert!(to_int("100.5", false).is_err());
        }

        #[test]
        fn accepts_zero_mantissa() {
            assert_eq!(to_int("100.0", false).unwrap(), 100);
        }

        #[test]
        fn single_grouped_triple_is_treated_as_thousands() {
            // Unlike to_float, to_int treats even a single ".XXX" group as a thousands
            // separator: this is the intentional asymmetry pinned by `4,500` above (4.5 for
            // to_float, 4500 for to_int).
            assert_eq!(to_int("1.234", false).unwrap(), 1234);
        }

        #[test]
        fn strips_non_numeric_noise_but_keeps_letters() {
            assert_eq!(to_int("€1.234", false).unwrap(), 1234);
            assert_eq!(to_int(" [1.234] ", false).unwrap(), 1234);
            assert!(to_int("EUR 1.234 approx", false).is_err());
        }
    }

    mod is_numeric_shape {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("123", true; "plain digits")]
        #[test_case("1.234", true; "one dot group")]
        #[test_case("1,234.567", true; "comma and dot groups")]
        #[test_case("-365,138.81", false; "leading minus sign is not plain numeric shape")]
        #[test_case("EUR 1.234", false; "letters are not plain numeric shape")]
        #[test_case("", false; "empty string")]
        fn matches_the_expected_shape(input: &str, expected: bool) {
            assert_eq!(is_numeric_shape(input), expected);
        }
    }

    mod perc_to_float {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("5.5%", true, 0.055; "percent sign forces normalization")]
        #[test_case("25,5", false, 25.5; "no percent sign, norm false keeps raw value")]
        #[test_case("10 %", true, 0.1; "percent sign with space before it")]
        #[test_case("25,5", true, 0.255; "no percent sign, norm true divides by 100")]
        fn matches_the_expected_value(input: &str, norm: bool, expected: f64) {
            assert!((perc_to_float(input, norm, false).unwrap() - expected).abs() < 1e-9);
        }

        #[test]
        fn percent_sign_forces_normalization_even_when_norm_is_false() {
            assert_eq!(perc_to_float("10%", false, false).unwrap(), 0.1);
        }
    }

    mod to_str {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn strips_whitespace_and_preserves_case() {
            assert_eq!(to_str("  Hello World  "), "Hello World");
        }
    }

    mod to_currency {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn accepts_an_iso_code() {
            assert_eq!(to_currency("usd").unwrap(), Currency::USD);
        }

        #[test]
        fn accepts_the_euro_alias() {
            assert_eq!(to_currency("euro").unwrap(), Currency::EUR);
        }

        #[test]
        fn is_case_insensitive() {
            assert_eq!(to_currency("EUR").unwrap(), to_currency("eur").unwrap());
        }

        #[test]
        fn rejects_an_unknown_currency() {
            assert!(to_currency("XXX").is_err());
        }
    }

    mod to_date {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        fn date(year: i32, month: u8, day: u8) -> Date {
            Date::new(year, month, day).unwrap()
        }

        #[test_case("2025-07-02", date(2025, 7, 2); "iso format")]
        #[test_case("2025/07/02", date(2025, 7, 2); "iso slash format")]
        #[test_case("02/07/2025", date(2025, 7, 2); "european slash format")]
        #[test_case("02.07.2025", date(2025, 7, 2); "european dot format")]
        #[test_case("02.07.25", date(2025, 7, 2); "european dot short year")]
        #[test_case("02/07/25", date(2025, 7, 2); "european slash short year")]
        #[test_case("07-02-2025", date(2025, 7, 2); "us dash format")]
        #[test_case("01-05-25", date(2025, 5, 1); "day dash month dash short year")]
        #[test_case("05/25", date(2025, 5, 1); "month slash short year, day defaults to 1")]
        fn accepts_every_recognized_format(input: &str, expected: Date) {
            assert_eq!(to_date(input).unwrap(), expected);
        }

        #[test]
        fn two_digit_year_pivots_at_sixty_nine() {
            // 69-99 -> 19xx, 00-68 -> 20xx (same pivot as Python's strptime %y).
            assert_eq!(to_date("01.01.69").unwrap(), date(1969, 1, 1));
            assert_eq!(to_date("01.01.68").unwrap(), date(2068, 1, 1));
        }

        #[test]
        fn rejects_an_unrecognized_format() {
            assert!(to_date("not a date").is_err());
        }

        #[test]
        fn the_first_matching_format_in_the_table_wins() {
            // "2025-07-02" only matches the ISO format in the table, in order.
            assert_eq!(to_date("2025-07-02").unwrap(), date(2025, 7, 2));
        }

        #[test]
        fn rejects_a_well_formed_but_calendarially_impossible_date() {
            // New case vs. the freeports_core reference (which returned a raw, unvalidated
            // tuple): here `Date::new` validates, so "31 February" must be rejected.
            assert!(to_date("31.02.2025").is_err());
        }
    }

    mod month_names {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("January", 1; "january")]
        #[test_case("February", 2; "february")]
        #[test_case("March", 3; "march")]
        #[test_case("April", 4; "april")]
        #[test_case("May", 5; "may")]
        #[test_case("June", 6; "june")]
        #[test_case("July", 7; "july")]
        #[test_case("August", 8; "august")]
        #[test_case("September", 9; "september")]
        #[test_case("October", 10; "october")]
        #[test_case("November", 11; "november")]
        #[test_case("December", 12; "december")]
        fn to_int_en_month_maps_every_month(name: &str, expected: u8) {
            assert_eq!(to_int_en_month(name).unwrap(), expected);
            assert_eq!(to_int_en_month(&name.to_uppercase()).unwrap(), expected, "case insensitive");
            assert_eq!(to_int_en_month(&name.to_lowercase()).unwrap(), expected, "case insensitive");
        }

        #[test_case("gennaio", 1; "gennaio")]
        #[test_case("febbraio", 2; "febbraio")]
        #[test_case("marzo", 3; "marzo")]
        #[test_case("aprile", 4; "aprile")]
        #[test_case("maggio", 5; "maggio")]
        #[test_case("giugno", 6; "giugno")]
        #[test_case("luglio", 7; "luglio")]
        #[test_case("agosto", 8; "agosto")]
        #[test_case("settembre", 9; "settembre")]
        #[test_case("ottobre", 10; "ottobre")]
        #[test_case("novembre", 11; "novembre")]
        #[test_case("dicembre", 12; "dicembre")]
        fn to_int_it_month_maps_every_month(name: &str, expected: u8) {
            assert_eq!(to_int_it_month(name).unwrap(), expected);
            assert_eq!(to_int_it_month(&name.to_uppercase()).unwrap(), expected, "case insensitive");
            assert_eq!(to_int_it_month(&name.to_lowercase()).unwrap(), expected, "case insensitive");
        }

        #[test]
        fn to_int_en_month_rejects_an_unknown_name() {
            assert!(to_int_en_month("Gennaio").is_err());
            assert!(to_int_en_month("Not A Month").is_err());
        }

        #[test]
        fn to_int_it_month_rejects_an_unknown_name() {
            assert!(to_int_it_month("January").is_err());
            assert!(to_int_it_month("Non Un Mese").is_err());
        }
    }

    mod date_with_month_name {
        use super::*;
        use pretty_assertions::assert_eq;

        fn date(year: i32, month: u8, day: u8) -> Date {
            Date::new(year, month, day).unwrap()
        }

        #[test]
        fn to_date_with_en_month_parses_day_month_year() {
            assert_eq!(to_date_with_en_month("1 January 2025").unwrap(), date(2025, 1, 1));
            assert_eq!(to_date_with_en_month("15 December 2024").unwrap(), date(2024, 12, 15));
        }

        #[test]
        fn to_date_with_it_month_parses_day_month_year() {
            assert_eq!(to_date_with_it_month("1 Gennaio 2025").unwrap(), date(2025, 1, 1));
            assert_eq!(to_date_with_it_month("15 Dicembre 2024").unwrap(), date(2024, 12, 15));
        }

        #[test]
        fn rejects_a_string_missing_parts() {
            assert!(to_date_with_en_month("January 2025").is_err());
            assert!(to_date_with_it_month("Gennaio 2025").is_err());
        }

        #[test]
        fn rejects_a_string_with_too_many_parts() {
            assert!(to_date_with_en_month("1 January 2025 extra").is_err());
            assert!(to_date_with_it_month("1 Gennaio 2025 extra").is_err());
        }

        #[test]
        fn rejects_an_unknown_month_name_in_either_language() {
            assert!(to_date_with_en_month("1 Gennaio 2025").is_err());
            assert!(to_date_with_it_month("1 January 2025").is_err());
        }
    }

    /// Cattura gli eventi `tracing` emessi durante `f`, a prescindere da `page`/`company`/
    /// `field`: a differenza di `CsvLogLayer` (`core::tracing_setup`), che scrive una riga solo
    /// se l'evento porta uno di quei campi (assenti qui per costruzione, vedi il doc-comment del
    /// modulo), questo layer di test registra il messaggio di ogni evento di livello WARN senza
    /// alcun filtro sui campi.
    mod forced_cast_warnings {
        use super::*;
        use std::sync::{Arc, Mutex};
        use tracing::field::{Field, Visit};
        use tracing_subscriber::Registry;
        use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

        #[derive(Default)]
        struct MessageVisitor(String);

        impl Visit for MessageVisitor {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }

        #[derive(Clone, Default)]
        struct CapturingLayer {
            warnings: Arc<Mutex<Vec<String>>>,
        }

        impl<S: tracing::Subscriber> Layer<S> for CapturingLayer {
            fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
                if *event.metadata().level() != tracing::Level::WARN {
                    return;
                }
                let mut visitor = MessageVisitor::default();
                event.record(&mut visitor);
                self.warnings.lock().unwrap().push(visitor.0);
            }
        }

        /// Esegue `f` sotto un subscriber dedicato e restituisce i messaggi di ogni evento WARN
        /// emesso durante l'esecuzione.
        fn warnings_emitted_by(f: impl FnOnce()) -> Vec<String> {
            let layer = CapturingLayer::default();
            let subscriber = Registry::default().with(layer.clone());
            tracing::subscriber::with_default(subscriber, f);
            let warnings = layer.warnings.lock().unwrap();
            warnings.clone()
        }

        #[test]
        fn a_clean_cast_does_not_warn() {
            let warnings = warnings_emitted_by(|| {
                let _ = to_int("200", false);
            });
            assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        }

        #[test]
        fn to_int_warns_when_forced_to_strip_noise() {
            let warnings = warnings_emitted_by(|| {
                let _ = to_int("EUR 1.234", false);
            });
            assert!(!warnings.is_empty(), "expected a forced-cast warning, got none");
        }

        #[test]
        fn to_float_warns_when_forced_to_strip_noise() {
            let warnings = warnings_emitted_by(|| {
                let _ = to_float("EUR 1.234", false);
            });
            assert!(!warnings.is_empty(), "expected a forced-cast warning, got none");
        }

        #[test]
        fn to_float_does_not_warn_for_an_already_numeric_shape() {
            let warnings = warnings_emitted_by(|| {
                let _ = to_float("1,234.567", false);
            });
            assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        }

        #[test]
        fn perc_to_float_warns_when_a_percent_sign_forces_normalization_despite_norm_false() {
            let warnings = warnings_emitted_by(|| {
                let _ = perc_to_float("10%", false, false);
            });
            assert!(!warnings.is_empty(), "expected a forced-normalization warning, got none");
        }

        #[test]
        fn perc_to_float_does_not_warn_without_a_percent_sign_and_a_clean_value() {
            let warnings = warnings_emitted_by(|| {
                let _ = perc_to_float("25.5", false, false);
            });
            assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        }
    }

    /// `keep_sign` (new trailing parameter on `to_float`/`to_int`/`perc_to_float`, see
    /// `agent-memory/M4-implementation-plan.md`): a `-` counts as a genuine sign only when it is
    /// the first non-whitespace character of the (trimmed) input, directly preceding the numeric
    /// content — anywhere else (trailing, standalone, glued to other noise) it is noise, stripped
    /// exactly like today regardless of `keep_sign`. When `keep_sign` is `false` the sign (genuine
    /// or stray) is always ignored — this is the back-compat default asserted throughout the
    /// `to_float`/`to_int`/`perc_to_float` submodules above.
    mod sign_handling {
        use super::*;

        mod to_float {
            use super::*;
            use pretty_assertions::assert_eq;
            use test_case::test_case;

            #[test_case("-200", -200.0; "plain integer")]
            #[test_case("-309.00", -309.0; "decimal")]
            #[test_case("-1.234.567", -1_234_567.0; "dot thousands grouped")]
            #[test_case("- 3.5", -3.5; "leading minus separated by whitespace")]
            fn genuine_leading_minus_negates_the_result_when_keep_sign_is_true(
                input: &str,
                expected: f64,
            ) {
                assert_eq!(to_float(input, true).unwrap(), expected);
            }

            #[test_case("-200", 200.0; "plain integer")]
            #[test_case("-309.00", 309.0; "decimal")]
            #[test_case("-1.234.567", 1_234_567.0; "dot thousands grouped")]
            fn genuine_leading_minus_is_stripped_like_noise_when_keep_sign_is_false(
                input: &str,
                expected: f64,
            ) {
                assert_eq!(to_float(input, false).unwrap(), expected);
            }

            #[test_case("3.0 -", 3.0; "trailing minus")]
            #[test_case("$100-", 100.0; "minus glued directly to noise, no leading sign")]
            fn a_stray_minus_is_ignored_when_keep_sign_is_true(input: &str, expected: f64) {
                assert_eq!(to_float(input, true).unwrap(), expected);
            }

            #[test_case("3.0 -", 3.0; "trailing minus")]
            #[test_case("$100-", 100.0; "minus glued directly to noise, no leading sign")]
            fn a_stray_minus_is_ignored_when_keep_sign_is_false(input: &str, expected: f64) {
                assert_eq!(to_float(input, false).unwrap(), expected);
            }

            #[test]
            fn a_lone_minus_with_no_digits_still_errors_when_keep_sign_is_true() {
                assert!(to_float("-", true).is_err());
            }

            #[test]
            fn a_lone_minus_with_no_digits_still_errors_when_keep_sign_is_false() {
                assert!(to_float("-", false).is_err());
            }
        }

        mod to_int {
            use super::*;
            use pretty_assertions::assert_eq;
            use test_case::test_case;

            #[test_case("-200", -200; "plain integer")]
            #[test_case("-1.234", -1234; "dot thousands grouped")]
            #[test_case("- 200", -200; "leading minus separated by whitespace")]
            fn genuine_leading_minus_negates_the_result_when_keep_sign_is_true(
                input: &str,
                expected: i64,
            ) {
                assert_eq!(to_int(input, true).unwrap(), expected);
            }

            #[test_case("-200", 200; "plain integer")]
            #[test_case("-1.234", 1234; "dot thousands grouped")]
            fn genuine_leading_minus_is_stripped_like_noise_when_keep_sign_is_false(
                input: &str,
                expected: i64,
            ) {
                assert_eq!(to_int(input, false).unwrap(), expected);
            }

            #[test_case("200 -", 200; "trailing minus")]
            #[test_case("$100-", 100; "minus glued directly to noise, no leading sign")]
            fn a_stray_minus_is_ignored_when_keep_sign_is_true(input: &str, expected: i64) {
                assert_eq!(to_int(input, true).unwrap(), expected);
            }

            #[test_case("200 -", 200; "trailing minus")]
            #[test_case("$100-", 100; "minus glued directly to noise, no leading sign")]
            fn a_stray_minus_is_ignored_when_keep_sign_is_false(input: &str, expected: i64) {
                assert_eq!(to_int(input, false).unwrap(), expected);
            }

            #[test]
            fn a_lone_minus_with_no_digits_still_errors_when_keep_sign_is_true() {
                assert!(to_int("-", true).is_err());
            }

            #[test]
            fn a_lone_minus_with_no_digits_still_errors_when_keep_sign_is_false() {
                assert!(to_int("-", false).is_err());
            }
        }

        mod perc_to_float {
            use super::*;

            #[test]
            fn genuine_leading_minus_with_norm_true_negates_and_normalizes() {
                // "-5%" -> sign kept (genuine leading '-'), '%' forces norm regardless of the
                // `norm` argument's own value -- so this must hold with norm:true...
                assert!((to_float_eq(perc_to_float("-5%", true, true).unwrap(), -0.05)));
            }

            #[test]
            fn genuine_leading_minus_with_norm_false_still_normalizes_because_of_percent_sign() {
                // ...and with norm:false too, since a literal '%' always forces normalization
                // (see `perc_to_float_warns_when_a_percent_sign_forces_normalization_despite_norm_false`
                // above) -- the new sign handling composes with that pre-existing rule rather than
                // overriding it.
                assert!((to_float_eq(perc_to_float("-5%", false, true).unwrap(), -0.05)));
            }

            #[test]
            fn genuine_leading_minus_without_a_percent_sign_and_norm_false_keeps_the_raw_value() {
                assert!((to_float_eq(perc_to_float("-25.5", false, true).unwrap(), -25.5)));
            }

            #[test]
            fn genuine_leading_minus_without_a_percent_sign_and_norm_true_divides_by_100() {
                assert!((to_float_eq(perc_to_float("-25.5", true, true).unwrap(), -0.255)));
            }

            #[test]
            fn keep_sign_false_ignores_a_genuine_leading_minus_just_like_to_float() {
                assert!((to_float_eq(perc_to_float("-5%", true, false).unwrap(), 0.05)));
            }

            #[test]
            fn a_stray_minus_is_ignored_regardless_of_keep_sign() {
                assert!((to_float_eq(perc_to_float("5% -", true, true).unwrap(), 0.05)));
                assert!((to_float_eq(perc_to_float("5% -", true, false).unwrap(), 0.05)));
            }

            #[test]
            fn a_lone_minus_with_no_digits_still_errors() {
                assert!(perc_to_float("-", true, true).is_err());
                assert!(perc_to_float("-", true, false).is_err());
            }

            fn to_float_eq(actual: f64, expected: f64) -> bool {
                (actual - expected).abs() < 1e-9
            }
        }
    }
}
