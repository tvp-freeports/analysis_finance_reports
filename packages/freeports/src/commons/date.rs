//! A minimal validated date: year, month, day, with parsing, formatting and serde.
//!
//! No calendar arithmetic at all — only validated construction, conversion to and from the
//! canonical ISO-8601 `YYYY-MM-DD` form, comparison, hashing and serde. Nothing in this crate needs
//! to add days to a date, and a date type that cannot do arithmetic cannot do it wrongly.
//!
//! Validation is real: month in range, and day in range **for that month and year**, leap years
//! included with the century rule, so 1900 has no 29 February and 2000 does. Years are limited to
//! four digits, which is what the canonical form can represent.
//!
//! Ordering is chronological, following the declaration order of the fields. Serde represents a
//! date as the canonical **string**, not as a three-field object, so a serialized date is readable
//! and round-trips through its `Display` and `FromStr` implementations.

use serde::{Deserialize, Serialize, de};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DateError {
    #[error("year must be between 0 and 9999, found {0}")]
    YearOutOfRange(i32),
    #[error("month must be between 1 and 12, found {0}")]
    InvalidMonth(u8),
    #[error("day must be between 1 and {max} for {year:04}-{month:02}, found {day}")]
    InvalidDay { year: i32, month: u8, day: u8, max: u8 },
    #[error("invalid date '{0}', expected format YYYY-MM-DD")]
    InvalidFormat(String),
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => unreachable!("month is validated to be in 1..=12 before calling days_in_month"),
    }
}

impl Date {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, DateError> {
        if !(0..=9999).contains(&year) {
            return Err(DateError::YearOutOfRange(year));
        }
        if !(1..=12).contains(&month) {
            return Err(DateError::InvalidMonth(month));
        }
        let max = days_in_month(year, month);
        if !(1..=max).contains(&day) {
            return Err(DateError::InvalidDay { year, month, day, max });
        }
        Ok(Self { year, month, day })
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl FromStr for Date {
    type Err = DateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || DateError::InvalidFormat(s.to_string());
        let parts: Vec<&str> = s.split('-').collect();
        let [year_str, month_str, day_str] = parts[..] else {
            return Err(invalid());
        };
        let is_all_ascii_digits =
            |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
        if !is_all_ascii_digits(year_str, 4)
            || !is_all_ascii_digits(month_str, 2)
            || !is_all_ascii_digits(day_str, 2)
        {
            return Err(invalid());
        }
        let year: i32 = year_str.parse().map_err(|_| invalid())?;
        let month: u8 = month_str.parse().map_err(|_| invalid())?;
        let day: u8 = day_str.parse().map_err(|_| invalid())?;
        Date::new(year, month, day)
    }
}

impl Serialize for Date {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod construction {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;
        use DateError::*;

        #[test]
        fn accepts_an_ordinary_date() {
            match Date::new(2024, 6, 15) {
                Ok(Date { year, month, day }) => {
                    assert_eq!(year, 2024);
                    assert_eq!(month, 6);
                    assert_eq!(day, 15);
                }
                Err(err) => panic!("expected a valid date, found error {err}"),
            }
        }

        #[test_case(2024, 2, 29; "leap year, divisible by 4 and not by 100")]
        #[test_case(2000, 2, 29; "leap year, divisible by 400")]
        #[test_case(1900, 2, 28; "century divisible by 100 but not 400 is not leap: last valid day")]
        #[test_case(2023, 2, 28; "ordinary non-leap year: last valid day")]
        #[test_case(2024, 4, 30; "30-day month: last valid day")]
        #[test_case(2024, 1, 31; "31-day month: last valid day")]
        #[test_case(2024, 12, 31; "december last day")]
        #[test_case(2024, 1, 1; "first day of the year")]
        #[test_case(0, 1, 1; "lowest valid year")]
        #[test_case(9999, 12, 31; "highest valid year")]
        fn accepts_every_real_calendar_date(year: i32, month: u8, day: u8) {
            match Date::new(year, month, day) {
                Ok(d) => assert_eq!(d, Date { year, month, day }),
                Err(err) => panic!("expected {year}-{month}-{day} to be valid, found error {err}"),
            }
        }

        #[test_case(-1, 1, 1, YearOutOfRange(-1); "negative year")]
        #[test_case(10000, 1, 1, YearOutOfRange(10000); "year above four digits")]
        fn rejects_year_outside_four_digit_range(year: i32, month: u8, day: u8, expected: DateError) {
            assert_eq!(Date::new(year, month, day), Err(expected));
        }

        #[test_case(2024, 0, 1, InvalidMonth(0); "month zero")]
        #[test_case(2024, 13, 1, InvalidMonth(13); "month thirteen")]
        #[test_case(2024, 255, 1, InvalidMonth(255); "month at the u8 upper bound")]
        fn rejects_month_outside_one_to_twelve(year: i32, month: u8, day: u8, expected: DateError) {
            assert_eq!(Date::new(year, month, day), Err(expected));
        }

        #[test_case(2024, 1, 0, InvalidDay{year: 2024, month: 1, day: 0, max: 31}; "day zero")]
        #[test_case(2023, 2, 29, InvalidDay{year: 2023, month: 2, day: 29, max: 28}; "february 29th on a non-leap year")]
        #[test_case(1900, 2, 29, InvalidDay{year: 1900, month: 2, day: 29, max: 28}; "february 29th on a century non-leap year")]
        #[test_case(2000, 2, 30, InvalidDay{year: 2000, month: 2, day: 30, max: 29}; "february 30th on a century leap year")]
        #[test_case(2024, 4, 31, InvalidDay{year: 2024, month: 4, day: 31, max: 30}; "31st of a 30-day month")]
        #[test_case(2024, 1, 32, InvalidDay{year: 2024, month: 1, day: 32, max: 31}; "32nd of a 31-day month")]
        fn rejects_day_out_of_range_for_month(year: i32, month: u8, day: u8, expected: DateError) {
            assert_eq!(Date::new(year, month, day), Err(expected));
        }

        #[test]
        fn checks_month_before_day_when_both_are_invalid() {
            // month 13 does not exist, so the day is never even evaluated against it.
            assert_eq!(Date::new(2024, 13, 99), Err(InvalidMonth(13)));
        }

        #[test]
        fn checks_year_before_month_when_both_are_invalid() {
            assert_eq!(Date::new(-1, 13, 1), Err(YearOutOfRange(-1)));
        }
    }

    mod parsing {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;
        use DateError::*;

        #[test_case("2024-06-15", Date{year: 2024, month: 6, day: 15}; "ordinary date")]
        #[test_case("2024-02-29", Date{year: 2024, month: 2, day: 29}; "leap day")]
        #[test_case("2000-02-29", Date{year: 2000, month: 2, day: 29}; "leap day on a century leap year")]
        #[test_case("0000-01-01", Date{year: 0, month: 1, day: 1}; "lowest valid year, zero padded")]
        #[test_case("9999-12-31", Date{year: 9999, month: 12, day: 31}; "highest valid year")]
        fn parses_canonical_form(input: &str, expected: Date) {
            assert_eq!(input.parse::<Date>(), Ok(expected));
        }

        #[test_case(""; "empty string")]
        #[test_case("2024/01/01"; "slash separators")]
        #[test_case("2024.01.01"; "dot separators")]
        #[test_case("20240101"; "no separators")]
        #[test_case("2024-1-01"; "single digit month")]
        #[test_case("2024-01-1"; "single digit day")]
        #[test_case("202-01-01"; "three digit year")]
        #[test_case("20245-01-01"; "five digit year")]
        #[test_case("abcd-01-01"; "non numeric year")]
        #[test_case("2024-ab-01"; "non numeric month")]
        #[test_case("2024-01-ab"; "non numeric day")]
        #[test_case(" 2024-01-01"; "leading whitespace")]
        #[test_case("2024-01-01 "; "trailing whitespace")]
        #[test_case("2024-01-01T00:00:00"; "trailing time component")]
        #[test_case("-024-01-01"; "leading dash before year")]
        #[test_case("2024-01-01-01"; "extra trailing segment")]
        fn rejects_strings_with_the_wrong_shape(input: &str) {
            assert_eq!(input.parse::<Date>(), Err(InvalidFormat(input.to_string())));
        }

        #[test_case("2024-13-01", InvalidMonth(13); "well formed but invalid month")]
        #[test_case("2024-00-01", InvalidMonth(0); "well formed but month zero")]
        #[test_case("2024-02-30", InvalidDay{year: 2024, month: 2, day: 30, max: 29}; "well formed but invalid day, leap year")]
        #[test_case("1900-02-29", InvalidDay{year: 1900, month: 2, day: 29, max: 28}; "well formed but invalid day, century non-leap year")]
        #[test_case("2024-04-31", InvalidDay{year: 2024, month: 4, day: 31, max: 30}; "well formed but invalid day, 30-day month")]
        fn rejects_well_formed_strings_with_impossible_calendar_values(input: &str, expected: DateError) {
            assert_eq!(input.parse::<Date>(), Err(expected));
        }
    }

    mod formatting {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case(Date{year: 2024, month: 6, day: 15}, "2024-06-15"; "ordinary date")]
        #[test_case(Date{year: 2024, month: 1, day: 1}, "2024-01-01"; "pads single digit month and day with a leading zero")]
        #[test_case(Date{year: 7, month: 3, day: 4}, "0007-03-04"; "pads a short year with leading zeros")]
        #[test_case(Date{year: 9999, month: 12, day: 31}, "9999-12-31"; "highest representable year")]
        fn formats_as_canonical_iso8601(date: Date, expected: &str) {
            assert_eq!(date.to_string(), expected);
        }

        #[test_case(2024, 6, 15; "ordinary date")]
        #[test_case(2024, 2, 29; "leap day")]
        #[test_case(0, 1, 1; "lowest valid year")]
        #[test_case(9999, 12, 31; "highest valid year")]
        fn round_trips_through_format_and_parse(year: i32, month: u8, day: u8) {
            let original = Date::new(year, month, day).expect("test table only has valid dates");
            let formatted = original.to_string();
            let reparsed: Date = formatted.parse().expect("format() must always produce a parseable string");
            assert_eq!(reparsed, original);
        }
    }

    mod error_display {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;
        use DateError::*;

        #[test_case(
            YearOutOfRange(-3),
            "year must be between 0 and 9999, found -3";
            "year out of range"
        )]
        #[test_case(
            InvalidMonth(13),
            "month must be between 1 and 12, found 13";
            "invalid month"
        )]
        #[test_case(
            InvalidDay{year: 2023, month: 2, day: 29, max: 28},
            "day must be between 1 and 28 for 2023-02, found 29";
            "invalid day"
        )]
        #[test_case(
            InvalidFormat("not-a-date".to_string()),
            "invalid date 'not-a-date', expected format YYYY-MM-DD";
            "invalid format"
        )]
        fn formats_the_expected_message(err: DateError, expected: &str) {
            assert_eq!(format!("{err}"), expected);
        }

        fn assert_implements_std_error<T: std::error::Error>() {}

        #[test]
        fn is_a_real_std_error_not_just_a_string() {
            assert_implements_std_error::<DateError>();
        }
    }

    mod ordering {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn orders_by_year_first() {
            let earlier = Date::new(2023, 12, 31).unwrap();
            let later = Date::new(2024, 1, 1).unwrap();
            assert!(earlier < later);
        }

        #[test]
        fn orders_by_month_when_years_are_equal() {
            let earlier = Date::new(2024, 1, 31).unwrap();
            let later = Date::new(2024, 2, 1).unwrap();
            assert!(earlier < later);
        }

        #[test]
        fn orders_by_day_when_year_and_month_are_equal() {
            let earlier = Date::new(2024, 6, 1).unwrap();
            let later = Date::new(2024, 6, 2).unwrap();
            assert!(earlier < later);
        }

        #[test]
        #[allow(clippy::nonminimal_bool)] // asserting `<`/`>` directly, not their negated complements.
        fn equal_dates_are_neither_less_nor_greater() {
            let a = Date::new(2024, 6, 15).unwrap();
            let b = Date::new(2024, 6, 15).unwrap();
            assert_eq!(a, b);
            assert!(!(a < b));
            assert!(!(a > b));
        }

        #[test]
        fn sorts_a_shuffled_vec_into_chronological_order() {
            let mut dates = vec![
                Date::new(2024, 1, 1).unwrap(),
                Date::new(2023, 12, 31).unwrap(),
                Date::new(2024, 6, 15).unwrap(),
                Date::new(2000, 2, 29).unwrap(),
                Date::new(2024, 1, 2).unwrap(),
            ];
            dates.sort();
            let expected = vec![
                Date::new(2000, 2, 29).unwrap(),
                Date::new(2023, 12, 31).unwrap(),
                Date::new(2024, 1, 1).unwrap(),
                Date::new(2024, 1, 2).unwrap(),
                Date::new(2024, 6, 15).unwrap(),
            ];
            assert_eq!(dates, expected);
        }
    }

    mod hashing {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::collections::HashSet;

        #[test]
        fn equal_dates_built_separately_collapse_in_a_hashset() {
            let mut set = HashSet::new();
            set.insert(Date::new(2024, 6, 15).unwrap());
            set.insert(Date::new(2024, 6, 15).unwrap());
            assert_eq!(set.len(), 1);
        }

        #[test]
        fn distinct_dates_both_survive_in_a_hashset() {
            let mut set = HashSet::new();
            set.insert(Date::new(2024, 6, 15).unwrap());
            set.insert(Date::new(2024, 6, 16).unwrap());
            assert_eq!(set.len(), 2);
        }
    }

    mod serde_roundtrip {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test]
        fn serializes_as_the_canonical_string_not_as_a_struct() {
            let date = Date::new(2024, 6, 15).unwrap();
            assert_eq!(serde_json::to_string(&date).unwrap(), "\"2024-06-15\"");
        }

        #[test]
        fn deserializes_from_the_canonical_string() {
            let date: Date = serde_json::from_str("\"2024-06-15\"").unwrap();
            assert_eq!(date, Date::new(2024, 6, 15).unwrap());
        }

        #[test_case(2024, 6, 15; "ordinary date")]
        #[test_case(2024, 2, 29; "leap day")]
        #[test_case(0, 1, 1; "lowest valid year")]
        #[test_case(9999, 12, 31; "highest valid year")]
        fn round_trips_through_serde_json(year: i32, month: u8, day: u8) {
            let original = Date::new(year, month, day).unwrap();
            let json = serde_json::to_string(&original).unwrap();
            let back: Date = serde_json::from_str(&json).unwrap();
            assert_eq!(back, original);
        }

        #[test]
        fn rejects_a_json_string_with_an_impossible_calendar_date() {
            let result: Result<Date, _> = serde_json::from_str("\"2024-02-30\"");
            assert!(result.is_err());
        }

        #[test]
        fn rejects_a_json_string_with_the_wrong_shape() {
            let result: Result<Date, _> = serde_json::from_str("\"15-06-2024\"");
            assert!(result.is_err());
        }

        #[test]
        fn rejects_a_non_string_json_value() {
            let result: Result<Date, _> = serde_json::from_str("20240615");
            assert!(result.is_err());
        }
    }
}
