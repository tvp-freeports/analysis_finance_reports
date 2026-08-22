//! Casting utilities for deserializing string data extracted from PDFs into typed values.
//!
//! Rust port of
//! `packages/freeports_core/src/freeports/_internals/formats/utils/deserialize/cast.py`. This
//! logic is financial-data-critical (thousand/decimal separator disambiguation) so every
//! function here mirrors the Python original's regex-driven algorithm as closely as possible,
//! rather than a from-scratch reimplementation, to minimize the chance of a subtle behavior
//! drift. Exhaustive tests below pin the same corner cases the original handles (mixed
//! separators, ambiguous single thousands-group, non-zero mantissa rejection, etc.).
//!
//! **Correction, found the hard way**: an earlier version of this module assumed the Python
//! original's `logger.warning(...)` calls on a forced/lossy cast were purely cosmetic
//! (a log-only side effect nothing depends on) and skipped replicating them. That assumption
//! was wrong — a `.log.csv` file in every format-repo test fixture is built *from* those exact
//! warning messages (`"Trying to cast to number but found '...' - forcing cast"`) via a
//! logging handler wired up in `output/routines.py`, and `freeports-dev test`'s `test_pipeline`
//! compares that file — so dropping the warnings silently broke 4 formats' fixtures. Rather than
//! reach back into Python `logging`/gettext from Rust (which would tangle this module with
//! concerns it shouldn't own), [`is_numeric_shape`]/[`py_is_numeric_shape`] exposes the exact
//! predicate the warning fires on, and the *logging itself* stays in `cast.py`'s thin Python
//! wrapper (see that file) — same division of labor as everywhere else in this migration:
//! Rust does the computation, Python does OS/i18n/logging-adjacent side effects.

use once_cell::sync::Lazy;
use onig::Regex;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDate;

use crate::commons::consts::Currency;
use crate::core::normalization;

// Oniguruma (via the `onig` crate), not the `regex` crate, matching the choice already made in
// `freeports_lib` (see `text_filter/matcher.rs`) — faster for this workload. One behavioral
// wrinkle: `onig::Regex::new`'s default syntax is Oniguruma's `ONIG_SYNTAX_DEFAULT` (Ruby-like),
// where `^`/`$` are per-line anchors rather than whole-string anchors like the `regex` crate's
// default. Harmless here: every pattern below only ever matches an already word-normalized,
// single-line string (no embedded `\n`), so line-anchored and string-anchored `^`/`$` coincide.
static NUMERIC_SHAPE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+([.,]\d+)*$").unwrap());
static NON_NUMERIC_CHARS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^a-zA-Z.,0-9]+").unwrap());
static FLOAT_THOUSANDS_GROUPED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[1-9]\d{0,2}\.\d{3}(\.\d{3})+$").unwrap());
static INT_THOUSANDS_GROUPED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[1-9]\d{0,2}(\.\d{3})+$").unwrap());

/// True when `data` (expected to already be word-normalized) already has the shape of a plain
/// number (`123`, `1.234`, `1,234.567`, ...) — i.e. `to_float`/`to_int` will use it as-is rather
/// than stripping noise out of it. Exposed to Python so `cast.py` can log its forced-cast
/// warning under the same condition the original did, without Rust reaching into Python
/// `logging`/gettext itself.
pub fn is_numeric_shape(data: &str) -> bool {
    NUMERIC_SHAPE.is_match(data)
}

/// Strips non-numeric noise from an already word-normalized string, unless it already has the
/// shape of a plain number (`123`, `1.234`, `1,234.567`, ...).
fn force_numeric(data: &str) -> String {
    if is_numeric_shape(data) {
        data.to_string()
    } else {
        NON_NUMERIC_CHARS.replace_all(data, "")
    }
}

/// Disambiguates thousands vs. decimal separators when both `.` and `,` are present: the
/// first-occurring character is treated as the thousands separator and every occurrence of it
/// is dropped; the remaining separator (if any) becomes the decimal point.
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

/// Casts a string to `f64`, handling thousand separators and mixed `.`/`,` decimal conventions.
pub fn to_float(data: &str) -> Result<f64, String> {
    let data = normalization::normalize_word(data, false);
    let data = force_numeric(&data);
    let mut data = resolve_separators(&data);
    if FLOAT_THOUSANDS_GROUPED.is_match(&data) {
        data = data.replace('.', "");
    }
    data.parse::<f64>()
        .map_err(|_| format!("could not convert string to float: {data:?}"))
}

/// Casts a string to `i64`, handling thousand separators and rejecting a non-zero mantissa.
pub fn to_int(data: &str) -> Result<i64, String> {
    let data = normalization::normalize_word(data, false);
    let data = force_numeric(&data);
    let mut data = resolve_separators(&data);
    if INT_THOUSANDS_GROUPED.is_match(&data) {
        data = data.replace('.', "");
    }
    if let Some(pos_dot) = data.find('.') {
        let mantissa: i64 = data[pos_dot + 1..]
            .parse()
            .map_err(|_| format!("could not parse mantissa of {data:?}"))?;
        if mantissa != 0 {
            return Err(format!("Number {data} has a mantissa different form 0"));
        }
        data.truncate(pos_dot);
    }
    data.parse::<i64>()
        .map_err(|_| format!("could not convert string to int: {data:?}"))
}

/// Casts a percentage string (optionally with a trailing `%`) to a float. When `norm` is true
/// (the default), the result is divided by 100 — and division by 100 is forced regardless of
/// `norm` whenever a literal `%` was present, matching the Python original.
pub fn perc_to_float(perc: &str, norm: bool) -> Result<f64, String> {
    let mut perc = normalization::normalize_word(perc, false);
    let mut norm = norm;
    if perc.contains('%') {
        perc = normalization::normalize_word(&perc.replace('%', ""), false);
        norm = true;
    }
    let f = to_float(&perc)
        .map_err(|_| format!("Failed to convert percentage string {perc:?} to float"))?;
    Ok(if norm { f / 100.0 } else { f })
}

/// Normalizes a string by stripping leading/trailing whitespace (case preserved).
pub fn to_str(data: &str) -> String {
    normalization::normalize_string(data, false)
}

/// Converts a string to a [`Currency`], by name (supports the `EURO` alias, unlike
/// value-based `Currency(...)` construction) after normalizing and uppercasing.
pub fn to_currency(data: &str) -> Result<Currency, String> {
    let normalized = normalization::normalize_word(data, false).to_uppercase();
    Currency::from_name(&normalized).ok_or_else(|| format!("{normalized:?} is not a valid Currency"))
}

const DATE_FORMATS: &[&[&str]] = &[
    &["Y", "-", "m", "-", "d"],       // %Y-%m-%d
    &["Y", "/", "m", "/", "d"],       // %Y/%m/%d
    &["d", "/", "m", "/", "Y"],       // %d/%m/%Y
    &["d", ".", "m", ".", "Y"],       // %d.%m.%Y
    &["d", ".", "m", ".", "y"],       // %d.%m.%y
    &["d", "/", "m", "/", "y"],       // %d/%m/%y
    &["m", "-", "d", "-", "Y"],       // %m-%d-%Y
    &["d", "-", "m", "-", "y"],       // %d-%m-%y
    &["m", "/", "y"],                 // %m/%y (day defaults to 1, like strptime does)
];

/// Tries a fixed set of date formats (ISO, European, US, short year) in order, mirroring the
/// Python original's `datetime.strptime` loop. `%y` (2-digit year) resolves via the same
/// pivot Python's `time`/`datetime` module uses: 69-99 -> 1969-1999, 00-68 -> 2000-2068.
pub fn to_date(data: &str) -> Result<(i32, u8, u8), String> {
    let data = normalization::normalize_word(data, false);
    for fmt in DATE_FORMATS {
        if let Some(ymd) = try_parse_date(&data, fmt) {
            return Ok(ymd);
        }
    }
    Err(format!("Date string {data:?} is not in a recognized format."))
}

fn expand_2digit_year(y: i32) -> i32 {
    if y >= 69 { 1900 + y } else { 2000 + y }
}

fn try_parse_date(data: &str, fmt: &[&str]) -> Option<(i32, u8, u8)> {
    // Split `data` on the same literal separators the format uses, in order, and read each
    // numeric field according to the format's field-name sequence (Y/y/m/d).
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
        // Mirrors CPython's `_strptime` field widths: `%Y` is a fixed 4 digits, `%y` a fixed 2
        // digits, `%m`/`%d` are flexible 1-2 digits. This isn't cosmetic — without it, formats
        // that share a separator (`%Y/%m/%d` vs. `%d/%m/%y`, both `/`-separated) can't be told
        // apart: "02/07/25" would otherwise satisfy `%Y/%m/%d` too (year="02"), matching the
        // wrong format before ever trying the right one. Verified against the real Python
        // `to_date` before fixing (it correctly returns 2025-07-02, not year=2).
        let width_ok = match field {
            "Y" => value_str.len() == 4,
            "y" => value_str.len() == 2,
            "m" | "d" => (1..=2).contains(&value_str.len()),
            _ => unreachable!(),
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
            _ => unreachable!(),
        }
        rest = remainder;
        i += if sep.is_some() { 2 } else { 1 };
    }
    if !rest.is_empty() {
        return None;
    }
    // `%m/%y` has no day field; Python's strptime defaults the unset day to 1.
    let day = day.unwrap_or(1);
    let year = year?;
    let month = month?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

/// Python-visible wrapper: builds a real `datetime.date` from [`to_date`]'s result, so Python
/// callers get the exact same type the original returned (day/month validity, e.g. rejecting
/// "31 February", is enforced by `PyDate::new` itself).
#[pyfunction]
#[pyo3(name = "to_date")]
pub fn py_to_date<'py>(py: Python<'py>, data: &str) -> PyResult<Bound<'py, PyDate>> {
    let (y, m, d) = to_date(data).map_err(PyValueError::new_err)?;
    PyDate::new(py, y, m, d)
}

const EN_MONTHS: &[&str] = &[
    "january", "february", "march", "april", "may", "june", "july", "august", "september",
    "october", "november", "december",
];
const IT_MONTHS: &[&str] = &[
    "gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto",
    "settembre", "ottobre", "novembre", "dicembre",
];

fn month_index(text: &str, months: &[&str]) -> Result<u32, String> {
    let needle = text.to_lowercase();
    let needle = needle.trim();
    months
        .iter()
        .position(|m| *m == needle)
        .map(|i| i as u32 + 1)
        .ok_or_else(|| format!("{text:?} is not a valid month name"))
}

/// Converts an English month name (case-insensitive) to its 1-12 index.
pub fn to_int_en_month(text: &str) -> Result<u32, String> {
    month_index(text, EN_MONTHS)
}

/// Converts an Italian month name (case-insensitive) to its 1-12 index.
pub fn to_int_it_month(text: &str) -> Result<u32, String> {
    month_index(text, IT_MONTHS)
}

fn parse_day_month_name_year(text: &str, months: &[&str]) -> Result<(i32, u32, u32), String> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let [day_s, month_s, year_s] = parts.as_slice() else {
        return Err(format!("{text:?} is not in \"DD MONTH YYYY\" format"));
    };
    let day: u32 = day_s
        .parse()
        .map_err(|_| format!("{day_s:?} is not a valid day"))?;
    let month = month_index(month_s, months)?;
    let year: i32 = year_s
        .parse()
        .map_err(|_| format!("{year_s:?} is not a valid year"))?;
    Ok((year, month, day))
}

/// Parses a `"DD MONTH YYYY"` date string with an English month name.
pub fn to_date_with_en_month(text: &str) -> Result<(i32, u32, u32), String> {
    parse_day_month_name_year(text, EN_MONTHS)
}

/// Parses a `"DD MONTH YYYY"` date string with an Italian month name.
pub fn to_date_with_it_month(text: &str) -> Result<(i32, u32, u32), String> {
    parse_day_month_name_year(text, IT_MONTHS)
}

#[pyfunction]
#[pyo3(name = "to_date_with_en_month")]
pub fn py_to_date_with_en_month<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyDate>> {
    let (y, m, d) = to_date_with_en_month(text).map_err(PyValueError::new_err)?;
    PyDate::new(py, y, m as u8, d as u8)
}

#[pyfunction]
#[pyo3(name = "to_date_with_it_month")]
pub fn py_to_date_with_it_month<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyDate>> {
    let (y, m, d) = to_date_with_it_month(text).map_err(PyValueError::new_err)?;
    PyDate::new(py, y, m as u8, d as u8)
}

#[pyfunction]
#[pyo3(name = "to_int_en_month")]
pub fn py_to_int_en_month(text: &str) -> PyResult<u32> {
    to_int_en_month(text).map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(name = "to_int_it_month")]
pub fn py_to_int_it_month(text: &str) -> PyResult<u32> {
    to_int_it_month(text).map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(name = "to_float")]
pub fn py_to_float(data: &str) -> PyResult<f64> {
    to_float(data).map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(name = "to_int")]
pub fn py_to_int(data: &str) -> PyResult<i64> {
    to_int(data).map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(name = "to_str")]
pub fn py_to_str(data: &str) -> String {
    to_str(data)
}

/// Matches the Python original's `if isinstance(data, Currency): return data` fast path —
/// an already-`Currency` argument passes through unchanged instead of being (incorrectly)
/// treated as a string to parse.
#[pyfunction]
#[pyo3(name = "to_currency")]
pub fn py_to_currency(data: &Bound<'_, PyAny>) -> PyResult<Currency> {
    if let Ok(existing) = data.extract::<Currency>() {
        return Ok(existing);
    }
    let s: String = data
        .extract()
        .map_err(|_| PyValueError::new_err(format!("{data:?} is not a valid Currency")))?;
    to_currency(&s).map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(name = "perc_to_float", signature = (perc, norm = true))]
pub fn py_perc_to_float(perc: &str, norm: bool) -> PyResult<f64> {
    perc_to_float(perc, norm).map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(name = "is_numeric_shape")]
pub fn py_is_numeric_shape(data: &str) -> bool {
    is_numeric_shape(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("200", 200.0; "plain integer")]
    #[test_case("309.00", 309.0; "trailing zero decimal")]
    #[test_case("  090.070,00 ", 90070.0; "dot thousands comma decimal with whitespace")]
    #[test_case("4,500", 4.5; "comma as decimal point")]
    fn test_to_float_matches_python_fixture_cases(input: &str, expected: f64) {
        assert_eq!(to_float(input).unwrap(), expected);
    }

    #[test_case("200", 200; "plain integer")]
    #[test_case("309.00", 309; "trailing zero decimal")]
    #[test_case("  090.070,00 ", 90070; "dot thousands comma decimal with whitespace")]
    #[test_case("4,500", 4500; "comma as thousands separator")]
    fn test_to_int_matches_python_fixture_cases(input: &str, expected: i64) {
        assert_eq!(to_int(input).unwrap(), expected);
    }

    #[test]
    fn to_int_rejects_nonzero_mantissa() {
        assert!(to_int("100.5").is_err());
    }

    #[test]
    fn to_int_accepts_zero_mantissa() {
        assert_eq!(to_int("100.0").unwrap(), 100);
    }

    #[test]
    fn to_float_single_grouped_triple_is_treated_as_decimal_not_thousands() {
        // Only one ".XXX" group is ambiguous for floats specifically (could be a genuine
        // decimal) -- the Python original leaves it as a decimal point rather than stripping.
        assert_eq!(to_float("1.234").unwrap(), 1.234);
    }

    #[test]
    fn to_float_two_grouped_triples_are_treated_as_thousands() {
        assert_eq!(to_float("1.234.567").unwrap(), 1234567.0);
    }

    #[test]
    fn to_int_single_grouped_triple_is_treated_as_thousands() {
        // Unlike to_float, to_int treats even a single ".XXX" group as a thousands separator.
        assert_eq!(to_int("1.234").unwrap(), 1234);
    }

    #[test_case("123", true; "plain digits")]
    #[test_case("1.234", true; "one dot group")]
    #[test_case("1,234.567", true; "comma and dot groups")]
    #[test_case("-365,138.81", false; "leading minus sign is not plain numeric shape")]
    #[test_case("EUR 1.234", false; "letters are not plain numeric shape")]
    #[test_case("", false; "empty string")]
    fn test_is_numeric_shape(input: &str, expected: bool) {
        assert_eq!(is_numeric_shape(input), expected);
    }

    #[test]
    fn strips_non_numeric_noise_when_shape_does_not_match() {
        // The noise-stripping regex only drops characters outside [a-zA-Z0-9.,] — letters are
        // deliberately kept (verified against the real Python `to_int`: `to_int("EUR 1.234
        // approx")` also raises, for the same reason: "EUR"/"approx" survive stripping and
        // break the subsequent int parse). Non-letter noise like a currency symbol or brackets
        // does get stripped correctly.
        assert_eq!(to_int("€1.234").unwrap(), 1234);
        assert_eq!(to_int(" [1.234] ").unwrap(), 1234);
        assert!(to_int("EUR 1.234 approx").is_err());
    }

    #[test_case("5.5%", true, 0.055; "percent sign forces normalization")]
    #[test_case("25,5", false, 25.5; "no percent sign, norm false keeps raw value")]
    #[test_case("10 %", true, 0.1; "percent sign with space")]
    #[test_case("25,5", true, 0.255; "no percent sign, norm true divides by 100")]
    fn test_perc_to_float(input: &str, norm: bool, expected: f64) {
        assert!((perc_to_float(input, norm).unwrap() - expected).abs() < 1e-9);
    }

    #[test]
    fn perc_sign_forces_normalization_even_when_norm_false() {
        // Matches the Python docstring/behavior: a literal '%' forces norm=True regardless of
        // the caller's `norm` argument.
        assert_eq!(perc_to_float("10%", false).unwrap(), 0.1);
    }

    #[test]
    fn to_str_strips_whitespace_preserves_case() {
        assert_eq!(to_str("  Hello World  "), "Hello World");
    }

    #[test]
    fn to_currency_accepts_iso_code() {
        assert_eq!(to_currency("usd").unwrap(), Currency::USD);
    }

    #[test]
    fn to_currency_accepts_euro_alias() {
        assert_eq!(to_currency("euro").unwrap(), Currency::EUR);
    }

    #[test]
    fn to_currency_rejects_unknown() {
        assert!(to_currency("XXX").is_err());
    }

    #[test_case("2025-07-02", (2025, 7, 2); "iso format")]
    #[test_case("2025/07/02", (2025, 7, 2); "iso slash format")]
    #[test_case("02/07/2025", (2025, 7, 2); "european slash format")]
    #[test_case("02.07.2025", (2025, 7, 2); "european dot format")]
    #[test_case("02.07.25", (2025, 7, 2); "european dot short year")]
    #[test_case("02/07/25", (2025, 7, 2); "european slash short year")]
    #[test_case("07-02-2025", (2025, 7, 2); "us dash format")]
    #[test_case("01-05-25", (2025, 5, 1); "day dash month dash short year")]
    #[test_case("05/25", (2025, 5, 1); "month slash short year, day defaults to 1")]
    fn test_to_date_formats(input: &str, expected: (i32, u8, u8)) {
        assert_eq!(to_date(input).unwrap(), expected);
    }

    #[test]
    fn to_date_two_digit_year_pivot() {
        // 69-99 -> 19xx, 00-68 -> 20xx (same pivot as Python's strptime %y).
        assert_eq!(to_date("01.01.69").unwrap().0, 1969);
        assert_eq!(to_date("01.01.68").unwrap().0, 2068);
    }

    #[test]
    fn to_date_rejects_unrecognized_format() {
        assert!(to_date("not a date").is_err());
    }

    #[test]
    fn to_date_first_matching_format_wins() {
        // "2025-07-02" only matches the ISO format in the list, in order.
        assert_eq!(to_date("2025-07-02").unwrap(), (2025, 7, 2));
    }

    #[test]
    fn to_int_en_month_case_insensitive() {
        assert_eq!(to_int_en_month("February").unwrap(), 2);
        assert_eq!(to_int_en_month("december").unwrap(), 12);
    }

    #[test]
    fn to_int_it_month_case_insensitive() {
        assert_eq!(to_int_it_month("Gennaio").unwrap(), 1);
        assert_eq!(to_int_it_month("dicembre").unwrap(), 12);
    }

    #[test]
    fn to_date_with_en_month_parses() {
        assert_eq!(to_date_with_en_month("1 January 2025").unwrap(), (2025, 1, 1));
    }

    #[test]
    fn to_date_with_it_month_parses() {
        assert_eq!(to_date_with_it_month("1 Gennaio 2025").unwrap(), (2025, 1, 1));
    }
}
