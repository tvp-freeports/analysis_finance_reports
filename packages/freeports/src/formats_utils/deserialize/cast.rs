//! Casting text to typed values: numbers, dates, currencies, percentages.
//!
//! This is where a financial report's ambiguity is resolved. `1.234` is one thousand two hundred
//! and thirty-four in one country and one-point-two-three-four in another; a report may write both
//! conventions on the same page, and neither says which it means.
//!
//! # How the ambiguity is settled
//!
//! When both `.` and `,` occur, the one appearing **first** is the thousands separator and every
//! occurrence of it is dropped; whichever remains becomes the decimal point. When only one occurs,
//! [`to_float`] and [`to_int`] deliberately disagree: a lone `.234` group could be a genuine
//! decimal, so `to_float` keeps it as one, while an integer cannot have a fractional part and
//! `to_int` reads it as a thousands separator. The asymmetry is intentional and pinned by tests.
//!
//! # Forced casts are logged, once, and only when they worked
//!
//! When the input is not already a clean numeric shape, the noise is stripped from it and the
//! number read from what is left. A **successful** forcing emits one warning naming both the text
//! as written and the text it was reduced to: a value that needed forcing is one worth being able
//! to find again in the report, because that is where a silently wrong number would come from.
//!
//! A forcing that then fails emits nothing here. The caller that drops the field or the row already
//! writes one line saying what would not convert and what was done about it, and two rows for one
//! fact is precisely what the logging contract forbids.
//!
//! # Signs
//!
//! A `-` counts as a genuine sign when it is the first character of the trimmed input, immediately
//! before the numeric content. Anywhere else — trailing, glued to other noise — it is noise and is
//! stripped. A lone `"-"` with no digits is a nil marker, not a sign.
//!
//! A leading sign is **data, and is always honoured**: `"-123"` is plainly minus one hundred and
//! twenty-three, and reading it as a positive number would be inventing a value the report does
//! not contain. It is therefore not counted as noise either, so a well-formed negative number
//! produces no forced-cast warning — there was nothing to dig it out of.
//!
//! Most fields of the output schema are magnitudes that admit no negative value, and this module
//! deliberately does **not** protect them by taking a modulus. A negative reaching such a field is
//! rejected by `FloatConstraint` and the field or the row is skipped, which is the honest outcome:
//! silently flipping the sign turned a short position into a holding the fund never had. A format
//! whose report prints a magnitude with a minus — a `Total Liabilities` line laid out so that
//! assets plus liabilities equal net assets — says so itself by wrapping the converter in `abs`,
//! where the reader can see the convention being applied.
//!
//! # Nil markers
//!
//! A text that has no digit left once the noise is stripped — `-`, `–`, `n/a`, an empty cell — is a
//! [`CastError::NoDigits`], not a [`CastError::NotANumber`]. The report is saying there is no value
//! here rather than writing one badly, and the two deserve different treatment: an absent value is
//! expected, a corrupt one is not.
//!
//! # Dates
//!
//! A fixed set of formats is tried in order. Two-digit years use the same pivot as `strptime`:
//! 69-99 map to 1969-1999, 00-68 to 2000-2068. The result is a validated [`Date`], so a well-formed
//! but impossible date such as `"31.02.2025"` is an error rather than a value that goes wrong
//! later.

use once_cell::sync::Lazy;
use onig::Regex;

use crate::commons::consts::Currency;
use crate::commons::date::{Date, DateError};
use crate::core::normalization;

// Oniguruma rather than the `regex` crate, as elsewhere in this crate. Its default syntax treats
// `^` and `$` as line anchors rather than string anchors, which is irrelevant here: every pattern
// below matches strings already normalised to a single word, never one with an embedded newline.
//
// All four patterns are fixed and hand-written, never built from external input, so a compilation
// failure would be a bug in this file rather than a runtime condition to handle.
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
    /// The text carries no digit at all: it is a "no value" marker — `-`, `–`, `n/a`, an empty
    /// cell — not a number written badly. Whoever receives it can tell an absent value from a
    /// corrupt one, which is the difference between an expected event and an anomalous one.
    #[error("{data:?} contains no digits")]
    NoDigits { data: String },
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

/// Whether `data` already has the shape of a plain number (`123`, `1.234`, `1,234.567`, …), meaning
/// the cast functions will use it as it is instead of stripping noise from it. It is also what
/// decides whether a successful cast is worth a forced-cast warning.
pub fn is_numeric_shape(data: &str) -> bool {
    NUMERIC_SHAPE.is_match(data)
}

/// What [`force_numeric`] produced: the unsigned text a number is to be read from, whether the
/// input carried a leading sign, and whether noise had to be stripped to get there.
///
/// The forced flag travels back to the caller instead of being logged here, because whether the
/// forcing is worth an event depends on how it ends — and only the caller knows that.
struct Forced {
    cleaned: String,
    negative: bool,
    was_forced: bool,
}

/// Splits off a leading sign and strips non-numeric noise from what remains, unless that already
/// has the shape of a plain number.
///
/// The sign is taken off first and reported separately rather than being stripped as noise: a
/// number written with a minus is a negative number, and a caller must be able to tell that apart
/// from a number that had to be recovered from a cell full of rubbish. `"-"` on its own leaves
/// nothing behind and stays a nil marker.
fn force_numeric(data: &str) -> Forced {
    let (negative, body) = match data.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, data),
    };
    if is_numeric_shape(body) {
        Forced { cleaned: body.to_string(), negative, was_forced: false }
    } else {
        Forced { cleaned: NON_NUMERIC_CHARS.replace_all(body, ""), negative, was_forced: true }
    }
}

/// Renders what the text was reduced to, sign included, for the forced-cast event.
fn forced_as_read(cleaned: &str, negative: bool) -> String {
    if negative { format!("-{cleaned}") } else { cleaned.to_string() }
}

/// The one event a forced cast is worth, emitted **after** the value has been read.
///
/// A forcing that worked is a mitigation that succeeded, and the documentation is explicit that
/// such a thing deserves its own row: nothing was lost, and a number that had to be dug out of
/// noise is one to be able to find again in the report. A forcing that then failed emits nothing
/// here — the caller that drops the field or the row says so in a single line of its own, which
/// also carries the consequence.
fn log_forced_cast(original: &str, cleaned: &str) {
    tracing::warn!("forced {original:?} to {cleaned:?} to read it as a number");
}

/// Whether there is still a digit to read after the noise has been stripped.
///
/// This is what tells a nil marker from a malformed number: `"-"`, `"n/a"` and an empty cell all
/// reduce to something with no digit in it, and the report meant "no value", not a number it got
/// wrong.
fn has_digits(data: &str) -> bool {
    data.bytes().any(|b| b.is_ascii_digit())
}

/// Disambiguates the thousands and decimal separators when both `.` and `,` are present: the
/// character occurring first is the thousands separator and every occurrence of it is removed;
/// whichever remains becomes the decimal point.
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

/// Casts to `f64`, handling thousands separators and mixed `.`/`,` conventions.
///
/// See the module documentation for how the separators are disambiguated and what counts as a
/// genuine sign.
pub fn to_float(data: &str) -> Result<f64, CastError> {
    let data = normalization::normalize_word(data, false);
    let Forced { cleaned, negative, was_forced } = force_numeric(&data);
    // Decided before the separators are resolved, and reported with the text as it arrived: once
    // stripped, `"-"` and `"."` are both the empty string, and an error naming `""` says nothing
    // about what the cell actually held.
    if !has_digits(&cleaned) {
        return Err(CastError::NoDigits { data });
    }
    let mut number = resolve_separators(&cleaned);
    if FLOAT_THOUSANDS_GROUPED.is_match(&number) {
        number = number.replace('.', "");
    }
    let value = number.parse::<f64>().map_err(|_| CastError::NotANumber { data: number })?;
    if was_forced {
        log_forced_cast(&data, &forced_as_read(&cleaned, negative));
    }
    Ok(if negative { -value } else { value })
}

/// Casts to `i64`, handling thousands separators and rejecting a non-zero fractional part.
///
/// Note that a lone `.234` group is read as a thousands separator here, where [`to_float`] would
/// read it as a decimal point.
pub fn to_int(data: &str) -> Result<i64, CastError> {
    let data = normalization::normalize_word(data, false);
    let Forced { cleaned, negative, was_forced } = force_numeric(&data);
    if !has_digits(&cleaned) {
        return Err(CastError::NoDigits { data });
    }
    let mut number = resolve_separators(&cleaned);
    if INT_THOUSANDS_GROUPED.is_match(&number) {
        number = number.replace('.', "");
    }
    if let Some(pos_dot) = number.find('.') {
        let mantissa: i64 = number[pos_dot + 1..]
            .parse()
            .map_err(|_| CastError::NotANumber { data: number.clone() })?;
        if mantissa != 0 {
            return Err(CastError::NonZeroMantissa { data: number });
        }
        number.truncate(pos_dot);
    }
    let value = number.parse::<i64>().map_err(|_| CastError::NotANumber { data: number })?;
    if was_forced {
        log_forced_cast(&data, &forced_as_read(&cleaned, negative));
    }
    Ok(if negative { -value } else { value })
}

/// Casts a percentage string, with or without a trailing `%`, to a float.
///
/// The result is divided by 100 when `norm` is set — and **always** when a literal `%` was present,
/// whatever `norm` says: the sign is in the data, and honouring it beats honouring the argument.
pub fn perc_to_float(perc: &str, norm: bool) -> Result<f64, CastError> {
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
    let f = to_float(&perc)?;
    Ok(if norm { f / 100.0 } else { f })
}

/// Trims leading and trailing whitespace, preserving case.
pub fn to_str(data: &str) -> String {
    normalization::normalize_string(data, false)
}

/// Converts a string to a [`Currency`], accepting the `EURO` alias, after normalising and
/// upper-casing.
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

/// Tries a fixed set of date formats in order: ISO, European, US, and short-year variants.
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
    // Splits `data` on the format's own literal separators, in order, reading each numeric field
    // according to the format's sequence of field names.
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
        // Field widths are enforced: a four-digit year is always four digits, a two-digit year
        // always two, while month and day accept one or two. Without this, formats sharing a
        // separator — `%Y/%m/%d` against `%d/%m/%y` — could not be told apart.
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
    // A month-and-year format has no day field; it defaults to the first of the month.
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

/// Converts an English month name, case-insensitively, to its index from 1 to 12.
pub fn to_int_en_month(text: &str) -> Result<u8, CastError> {
    month_index(text, EN_MONTHS, "en")
}

/// Converts an Italian month name, case-insensitively, to its index from 1 to 12.
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

/// Parses a `"DD MONTH YYYY"` date with an English month name.
pub fn to_date_with_en_month(text: &str) -> Result<Date, CastError> {
    parse_day_month_name_year(text, EN_MONTHS, "en")
}

/// Parses a `"DD MONTH YYYY"` date with an Italian month name.
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
            assert_eq!(to_float(input).unwrap(), expected);
        }

        #[test]
        fn single_grouped_triple_is_treated_as_decimal_not_thousands() {
            // Only one `.XXX` group is ambiguous for floats specifically, since it could be a
            // genuine decimal, so it is left as a decimal point rather than stripped.
            assert_eq!(to_float("1.234").unwrap(), 1.234);
        }

        #[test]
        fn two_grouped_triples_are_treated_as_thousands() {
            assert_eq!(to_float("1.234.567").unwrap(), 1_234_567.0);
        }

        #[test]
        fn strips_non_numeric_noise_but_keeps_letters() {
            assert_eq!(to_float("€1.234").unwrap(), 1.234);
            // Letters survive stripping, so a unit suffix still breaks the subsequent parse.
            assert!(to_float("EUR 1.234 approx").is_err());
        }

        #[test]
        fn rejects_a_string_with_no_digits_at_all() {
            assert!(to_float("not a number").is_err());
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
            assert_eq!(to_int(input).unwrap(), expected);
        }

        #[test]
        fn rejects_nonzero_mantissa() {
            assert!(to_int("100.5").is_err());
        }

        #[test]
        fn accepts_zero_mantissa() {
            assert_eq!(to_int("100.0").unwrap(), 100);
        }

        #[test]
        fn single_grouped_triple_is_treated_as_thousands() {
            // Unlike `to_float`, `to_int` treats even a single `.XXX` group as a thousands
            // separator: the intentional asymmetry pinned by `4,500` above, which is 4.5 for
            // `to_float` and 4500 for `to_int`.
            assert_eq!(to_int("1.234").unwrap(), 1234);
        }

        #[test]
        fn strips_non_numeric_noise_but_keeps_letters() {
            assert_eq!(to_int("€1.234").unwrap(), 1234);
            assert_eq!(to_int(" [1.234] ").unwrap(), 1234);
            assert!(to_int("EUR 1.234 approx").is_err());
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
            assert!((perc_to_float(input, norm).unwrap() - expected).abs() < 1e-9);
        }

        #[test]
        fn percent_sign_forces_normalization_even_when_norm_is_false() {
            assert_eq!(perc_to_float("10%", false).unwrap(), 0.1);
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
            // 69-99 map to 19xx, 00-68 to 20xx — the same pivot as `strptime`'s two-digit year.
            assert_eq!(to_date("01.01.69").unwrap(), date(1969, 1, 1));
            assert_eq!(to_date("01.01.68").unwrap(), date(2068, 1, 1));
        }

        #[test]
        fn rejects_an_unrecognized_format() {
            assert!(to_date("not a date").is_err());
        }

        #[test]
        fn the_first_matching_format_in_the_table_wins() {
            // `"2025-07-02"` matches only the ISO format in the table, in order.
            assert_eq!(to_date("2025-07-02").unwrap(), date(2025, 7, 2));
        }

        #[test]
        fn rejects_a_well_formed_but_calendarially_impossible_date() {
            // `Date::new` validates, so a 31st of February must be rejected rather than carried
            // forward as a value that goes wrong later.
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

    /// Captures the `tracing` events emitted during `f`, whatever fields they carry: unlike the CSV
    /// layer, which writes a row only for events carrying one of its tagged fields, this test layer
    /// records the message of every warning without filtering.
    /// A cell with no digit in it is a nil marker, not a broken number: the distinction is what
    /// lets a caller treat "the report says there is nothing here" as expected and "the report
    /// wrote something unreadable" as anomalous.
    mod nil_markers {
        use super::*;
        use test_case::test_case;

        #[test_case("-"; "ascii hyphen")]
        #[test_case("\u{2013}"; "en dash")]
        #[test_case("\u{2014}"; "em dash")]
        #[test_case("--"; "double hyphen")]
        #[test_case("n/a"; "lowercase n slash a")]
        #[test_case("N.A."; "uppercase with dots")]
        #[test_case(""; "empty cell")]
        #[test_case("   "; "whitespace only")]
        #[test_case("."; "lone dot")]
        #[test_case(","; "lone comma")]
        fn to_float_reads_a_digitless_text_as_a_nil_marker(text: &str) {
            assert!(matches!(to_float(text), Err(CastError::NoDigits { .. })), "{text:?}");
        }

        #[test_case("-"; "ascii hyphen")]
        #[test_case("\u{2013}"; "en dash")]
        #[test_case("n/a"; "lowercase n slash a")]
        #[test_case(""; "empty cell")]
        #[test_case("."; "lone dot")]
        #[test_case(","; "lone comma")]
        fn to_int_reads_a_digitless_text_as_a_nil_marker(text: &str) {
            assert!(matches!(to_int(text), Err(CastError::NoDigits { .. })), "{text:?}");
        }

        #[test_case("-", "-"; "the dash itself, not the empty string it strips to")]
        #[test_case("  n/a  ", "n/a"; "surrounding whitespace only is dropped")]
        #[test_case(".", "."; "a lone separator is reported as written")]
        fn the_error_carries_the_original_text_rather_than_the_stripped_one(text: &str, expected: &str) {
            let err = to_float(text).unwrap_err();
            assert_eq!(err, CastError::NoDigits { data: expected.to_string() });
        }

        #[test_case("0"; "plain zero")]
        #[test_case("0,00"; "zero with decimals")]
        #[test_case("-5"; "a negative number")]
        #[test_case("1.234abc"; "noise around real digits")]
        fn a_text_with_digits_is_never_a_nil_marker(text: &str) {
            assert!(!matches!(to_float(text), Err(CastError::NoDigits { .. })), "{text:?}");
        }

        #[test]
        fn a_forced_cast_that_still_has_digits_succeeds() {
            assert_eq!(to_float("\u{20ac}1.234").unwrap(), 1.234);
        }

        #[test]
        fn text_glued_to_digits_stays_an_unreadable_number_rather_than_a_nil_marker() {
            // The stripping keeps letters, so this one still has digits and fails to parse: it is
            // an anomaly, not an absence, and the two must not be confused.
            assert!(matches!(to_float("1.234abc"), Err(CastError::NotANumber { .. })));
        }

        #[test]
        fn a_percentage_inherits_the_nil_marker_through_to_float() {
            assert!(matches!(perc_to_float("-", true), Err(CastError::NoDigits { data }) if data == "-"));
        }
    }

    mod forced_cast_warnings {
        /// A sign is data, not noise, so reading one is not a forced cast and must stay silent.
        mod signs {
            use super::*;

            #[test]
            fn a_well_formed_negative_number_emits_no_forced_cast_warning() {
                let warnings = warnings_emitted_by(|| {
                    let _ = to_float("-365,138.81");
                });
                assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
            }

            #[test]
            fn a_negative_number_buried_in_noise_still_emits_one() {
                let warnings = warnings_emitted_by(|| {
                    let _ = to_int("[-1.234]");
                });
                assert_eq!(warnings.len(), 1, "expected exactly one warning: {warnings:?}");
            }

            #[test]
            fn a_minus_glued_to_noise_is_not_a_sign_and_the_value_stays_positive() {
                // `-` counts only as the first character. Here it sits inside the brackets, so it
                // is stripped with them and the row keeps a positive number.
                assert_eq!(to_int("[-1.234]").unwrap(), 1234);
            }

            #[test]
            fn the_forced_cast_event_reports_the_sign_it_read() {
                let warnings = warnings_emitted_by(|| {
                    let _ = to_int("-€1.234");
                });
                assert!(
                    warnings.iter().any(|w: &String| w.contains("\"-1.234\"")),
                    "the event must show the negative value it read: {warnings:?}"
                );
            }
        }

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

        /// Runs `f` under a dedicated subscriber and returns the messages of every warning emitted
        /// while it ran.
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
                let _ = to_int("200");
            });
            assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        }

        #[test]
        fn to_int_warns_when_forced_to_strip_noise() {
            let warnings = warnings_emitted_by(|| {
                let _ = to_int("\u{20ac} 1.234");
            });
            assert_eq!(warnings.len(), 1, "one forced cast, one warning: {warnings:?}");
        }

        #[test]
        fn to_float_warns_when_forced_to_strip_noise() {
            let warnings = warnings_emitted_by(|| {
                let _ = to_float("\u{20ac} 1.234");
            });
            assert_eq!(warnings.len(), 1, "one forced cast, one warning: {warnings:?}");
        }

        #[test]
        fn the_warning_names_the_text_as_written_and_the_text_it_was_reduced_to() {
            let warnings = warnings_emitted_by(|| {
                let _ = to_float("\u{20ac} 1.234");
            });
            assert!(warnings[0].contains("\"\u{20ac}1.234\""), "{warnings:?}");
            assert!(warnings[0].contains(r#""1.234""#), "{warnings:?}");
        }

        #[test]
        fn a_forcing_that_ends_in_a_failure_says_nothing_here() {
            // The caller that drops the field or the row emits the one line that also carries the
            // consequence; a second row about the attempt would say the same fact twice.
            let warnings = warnings_emitted_by(|| {
                let _ = to_int("EUR 1.234");
                let _ = to_float("not a number");
                let _ = to_float("-");
            });
            assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        }

        #[test]
        fn a_nil_marker_is_not_reported_as_a_forced_cast() {
            let warnings = warnings_emitted_by(|| {
                let _ = to_int("-");
            });
            assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        }

        #[test]
        fn to_float_does_not_warn_for_an_already_numeric_shape() {
            let warnings = warnings_emitted_by(|| {
                let _ = to_float("1,234.567");
            });
            assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        }

        #[test]
        fn perc_to_float_warns_when_a_percent_sign_forces_normalization_despite_norm_false() {
            let warnings = warnings_emitted_by(|| {
                let _ = perc_to_float("10%", false);
            });
            assert!(!warnings.is_empty(), "expected a forced-normalization warning, got none");
        }

        #[test]
        fn perc_to_float_does_not_warn_without_a_percent_sign_and_a_clean_value() {
            let warnings = warnings_emitted_by(|| {
                let _ = perc_to_float("25.5", false);
            });
            assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        }
    }

    /// A `-` counts as a genuine sign when it is the first character of the trimmed input,
    /// directly preceding the numeric content. Anywhere else — trailing, standalone, glued to
    /// other noise — it is stripped as noise.
    ///
    /// A genuine sign is **always** honoured, and is never treated as noise: reading `"-200"` as
    /// two hundred would invent a value the report does not contain. Protecting a field that
    /// admits no negative is not this module's job — `FloatConstraint` rejects the value and the
    /// caller skips the field or the row — and a format that really does read a magnitude written
    /// with a minus says so by wrapping the converter in `abs`.
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
            fn a_genuine_leading_minus_negates_the_result(input: &str, expected: f64) {
                assert_eq!(to_float(input).unwrap(), expected);
            }

            #[test_case("3.0 -", 3.0; "trailing minus")]
            #[test_case("$100-", 100.0; "minus glued directly to noise, no leading sign")]
            fn a_stray_minus_is_ignored(input: &str, expected: f64) {
                assert_eq!(to_float(input).unwrap(), expected);
            }

            #[test]
            fn a_lone_minus_with_no_digits_is_still_a_nil_marker() {
                assert!(matches!(to_float("-"), Err(CastError::NoDigits { .. })));
            }
        }

        mod to_int {
            use super::*;
            use pretty_assertions::assert_eq;
            use test_case::test_case;

            #[test_case("-200", -200; "plain integer")]
            #[test_case("-1.234", -1234; "dot thousands grouped")]
            #[test_case("- 200", -200; "leading minus separated by whitespace")]
            fn a_genuine_leading_minus_negates_the_result(input: &str, expected: i64) {
                assert_eq!(to_int(input).unwrap(), expected);
            }

            #[test_case("200 -", 200; "trailing minus")]
            #[test_case("$100-", 100; "minus glued directly to noise, no leading sign")]
            fn a_stray_minus_is_ignored(input: &str, expected: i64) {
                assert_eq!(to_int(input).unwrap(), expected);
            }

            #[test]
            fn a_lone_minus_with_no_digits_is_still_a_nil_marker() {
                assert!(matches!(to_int("-"), Err(CastError::NoDigits { .. })));
            }
        }

        mod perc_to_float {
            use super::*;

            #[test]
            fn a_genuine_leading_minus_with_norm_true_negates_and_normalizes() {
                // `"-5%"`: the leading `-` is genuine and kept, and the `%` forces normalisation
                // whatever `norm` says — so this must hold with `norm: true`…
                assert!(to_float_eq(perc_to_float("-5%", true).unwrap(), -0.05));
            }

            #[test]
            fn a_genuine_leading_minus_with_norm_false_still_normalizes_because_of_percent_sign() {
                // …and with `norm: false` too, since a literal `%` always forces normalisation. The
                // sign handling composes with that rule rather than overriding it.
                assert!(to_float_eq(perc_to_float("-5%", false).unwrap(), -0.05));
            }

            #[test]
            fn a_genuine_leading_minus_without_a_percent_sign_and_norm_false_keeps_the_raw_value() {
                assert!(to_float_eq(perc_to_float("-25.5", false).unwrap(), -25.5));
            }

            #[test]
            fn a_genuine_leading_minus_without_a_percent_sign_and_norm_true_divides_by_100() {
                assert!(to_float_eq(perc_to_float("-25.5", true).unwrap(), -0.255));
            }

            #[test]
            fn a_stray_minus_is_ignored() {
                assert!(to_float_eq(perc_to_float("5% -", true).unwrap(), 0.05));
            }

            #[test]
            fn a_lone_minus_with_no_digits_still_errors() {
                assert!(perc_to_float("-", true).is_err());
            }

            fn to_float_eq(actual: f64, expected: f64) -> bool {
                (actual - expected).abs() < 1e-9
            }
        }

    }
}
