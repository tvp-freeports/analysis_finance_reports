//! Reading the dash a report prints where a number belongs as the zero it means.
//!
//! Several reports write `-` instead of `0` — a frozen holding, a position too small to round to a
//! visible percentage. The engine cannot decide on its own that a dash is a zero, because in the
//! same column of a different report a dash is a value the layout failed to produce. So the
//! decision belongs to the format, one field at a time, and this module is where that decision is
//! expressed and applied.
//!
//! # Why a dash, and not "no digits"
//!
//! The conversion layer already has a name for a cell with no digit in it —
//! [`CastError::NoDigits`](crate::formats_utils::deserialize::cast::CastError::NoDigits) — and
//! keying the substitution on that would have been one line. It would also have been wrong, and
//! measurably so. On a real batch, the events reporting a digitless cell are dominated not by
//! dashes but by **misalignment**: the configured offset landed a column away and the "number" the
//! engine found is `USD`, `GBP`, `Assets`, `nominale`, or the name of a company. Those are bugs in
//! a format's offsets, they must keep failing loudly, and a rule that turned every digitless cell
//! into zero would bury several hundred of them under a plausible-looking `0.0`.
//!
//! So [`is_dash`] recognises the **dash family and nothing else**: the six Unicode dashes, alone or
//! repeated, with surrounding whitespace. Not the empty cell — in these tables an empty cell means
//! *absent*, which is a different statement and the repository's own convention. Not `n/a`, not a
//! lone `.` or `,`.
//!
//! # Four fields, not five
//!
//! `acquisition currency` has no flag: a dash there says "no currency", and zero is not a currency.
//! `maturity` and `interest rate` have none for the same reason — a dash in a date is not a
//! quantity of nothing.

use bitflags::bitflags;

use crate::commons::flag_expr::{self, FlagExprError};

bitflags! {
    /// Which of an investment row's numeric fields read a dash as zero.
    ///
    /// A bitmask rather than one boolean because the need is genuinely per field: a report that
    /// prints `-` for a percentage too small to show may print nothing of the sort in the quantity
    /// column, and a single switch would invent a zero quantity there.
    ///
    /// No zero-valued member is declared — `bitflags` already gives `empty()`, and a constant named
    /// after the empty set would read as "the default set of fields to substitute", which is the
    /// opposite of what it is.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DashAsZero: u8 {
        const MarketValue     = 0b0001;
        const Quantity        = 0b0010;
        const PercNetAssets   = 0b0100;
        const AcquisitionCost = 0b1000;
    }
}

impl DashAsZero {
    /// The flag names a formats repository writes in its configuration cell.
    ///
    /// `ALL` is a convenience for the four together. It adds no bit of its own, so the universe
    /// `~` complements within is unchanged and `~QUANTITY` is still the other three.
    fn flag_names() -> std::collections::HashMap<String, u64> {
        let all = DashAsZero::all().bits() as u64;
        std::collections::HashMap::from([
            ("MARKET_VALUE".to_string(), DashAsZero::MarketValue.bits() as u64),
            ("QUANTITY".to_string(), DashAsZero::Quantity.bits() as u64),
            ("PERC_NET_ASSETS".to_string(), DashAsZero::PercNetAssets.bits() as u64),
            ("ACQUISITION_COST".to_string(), DashAsZero::AcquisitionCost.bits() as u64),
            ("ALL".to_string(), all),
        ])
    }

    /// Parses the flag expression a formats repository writes (`"MARKET_VALUE"`,
    /// `"MARKET_VALUE | PERC_NET_ASSETS"`, `"ALL & ~QUANTITY"`, …).
    ///
    /// Delegates to [`crate::commons::flag_expr`], the same evaluator the algorithm flags use, so a
    /// format author meets one syntax and not two.
    pub fn from_expression(expression: &str) -> Result<Self, FlagExprError> {
        let bits = flag_expr::evaluate(expression, &Self::flag_names())?;
        // The evaluator works in `u64` while the four flags fit in a `u8`; it cannot produce a bit
        // it was not given, so the truncation loses nothing.
        Ok(DashAsZero::from_bits_truncate(bits as u8))
    }
}

/// The dash characters a report may print where a number belongs.
///
/// Six of them, because a PDF's font and a publisher's house style disagree about which one means
/// "nothing here": the reports in hand use at least the ASCII hyphen and the en dash.
const DASHES: [char; 6] = [
    '\u{002D}', // hyphen-minus, the ASCII one
    '\u{2012}', // figure dash
    '\u{2013}', // en dash
    '\u{2014}', // em dash
    '\u{2015}', // horizontal bar
    '\u{2212}', // minus sign
];

/// Whether `text` is a dash marker: nothing but dashes, once the surrounding whitespace is gone.
///
/// Repetitions count (`--`, `---`): a report that rules a cell out with a run of dashes is saying
/// the same thing as one that uses a single one. An empty string does **not** count — see the
/// module documentation for why that is a deliberate exclusion rather than an oversight.
pub fn is_dash(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| DASHES.contains(&c))
}

/// The text a field should be read as: `"0"` where the format asked for it and the report wrote a
/// dash, the text as written everywhere else.
///
/// `field` is an `Option` and not simply a `DashAsZero`, because a field that has **no** flag —
/// `acquisition currency` — has to be expressible, and the empty set cannot express it:
/// `contains(empty())` is true for every set, so passing `DashAsZero::empty()` as the field would
/// substitute *always* rather than never. `None` says "this field has no flag" and cannot be
/// misread.
///
/// Returns a borrowed `&str` in the common case, so the overwhelming majority of cells — every cell
/// of every format that sets no flag — pay nothing for the feature.
pub fn substituted(flags: DashAsZero, field: Option<DashAsZero>, text: &str) -> &str {
    match field {
        Some(field) if flags.contains(field) && is_dash(text) => "0",
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parsing {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn each_name_yields_its_own_flag() {
            for (expression, expected) in [
                ("MARKET_VALUE", DashAsZero::MarketValue),
                ("QUANTITY", DashAsZero::Quantity),
                ("PERC_NET_ASSETS", DashAsZero::PercNetAssets),
                ("ACQUISITION_COST", DashAsZero::AcquisitionCost),
            ] {
                assert_eq!(DashAsZero::from_expression(expression).expect(expression), expected);
            }
        }

        #[test]
        fn an_or_combines_two_fields() {
            assert_eq!(
                DashAsZero::from_expression("MARKET_VALUE | PERC_NET_ASSETS").expect("valid"),
                DashAsZero::MarketValue | DashAsZero::PercNetAssets
            );
        }

        #[test]
        fn all_is_the_four_fields_together() {
            assert_eq!(DashAsZero::from_expression("ALL").expect("valid"), DashAsZero::all());
        }

        /// `ALL` contributes no bit of its own, so the universe `~` inverts within stays the four
        /// real fields.
        #[test]
        fn a_complement_is_taken_within_the_four_fields() {
            assert_eq!(
                DashAsZero::from_expression("~QUANTITY").expect("valid"),
                DashAsZero::MarketValue | DashAsZero::PercNetAssets | DashAsZero::AcquisitionCost
            );
        }

        #[test]
        fn names_are_matched_case_insensitively() {
            assert_eq!(
                DashAsZero::from_expression("market_value").expect("valid"),
                DashAsZero::MarketValue
            );
        }

        #[test]
        fn an_unknown_name_is_rejected_by_name() {
            match DashAsZero::from_expression("ACQUISITION_CURRENCY") {
                Err(FlagExprError::UnknownFlag { name }) => assert_eq!(name, "ACQUISITION_CURRENCY"),
                other => panic!("expected UnknownFlag, found {other:?}"),
            }
        }

        #[test]
        fn an_empty_expression_is_an_error_rather_than_the_empty_set() {
            assert!(matches!(
                DashAsZero::from_expression(""),
                Err(FlagExprError::EmptyExpression)
            ));
        }

        #[test]
        fn a_syntax_error_is_reported_as_one() {
            assert!(DashAsZero::from_expression("MARKET_VALUE |").is_err());
        }
    }

    mod dash_recognition {
        use super::*;

        #[test]
        fn every_dash_of_the_family_is_recognised() {
            for dash in DASHES {
                assert!(is_dash(&dash.to_string()), "{dash:?} must be a dash marker");
            }
        }

        #[test]
        fn repeated_dashes_and_surrounding_whitespace_are_recognised() {
            for text in ["--", "---", " - ", "\t–\n", "  ——  ", "- "] {
                assert!(is_dash(text), "{text:?} must be a dash marker");
            }
        }

        /// The exclusions that keep the misalignment bugs failing. Each of these was observed in a
        /// real run as a cell an offset landed on by mistake.
        #[test]
        fn nothing_else_is_a_dash_marker() {
            for text in [
                "", "   ", "n/a", "N/A", ".", ",", "0", "-1", "- 5", "1-2", "USD", "GBP", "Assets",
                "nominale", "Valutazione", "ARION BANKI HF", "UNITEDSTATES", "CounterpartyB",
                "EXXON MOBIL CORP - 110.00 - 16.08.24 PUT",
            ] {
                assert!(!is_dash(text), "{text:?} must not be a dash marker");
            }
        }
    }

    mod substitution {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_flagged_field_reads_its_dash_as_zero() {
            assert_eq!(
                substituted(DashAsZero::MarketValue, Some(DashAsZero::MarketValue), "-"),
                "0"
            );
        }

        #[test]
        fn an_unflagged_field_keeps_its_dash() {
            assert_eq!(
                substituted(DashAsZero::MarketValue, Some(DashAsZero::Quantity), "-"),
                "-"
            );
        }

        #[test]
        fn a_flagged_field_that_is_not_a_dash_is_left_alone() {
            assert_eq!(
                substituted(DashAsZero::MarketValue, Some(DashAsZero::MarketValue), "USD"),
                "USD"
            );
        }

        #[test]
        fn the_empty_set_of_flags_substitutes_nothing() {
            assert_eq!(substituted(DashAsZero::empty(), Some(DashAsZero::MarketValue), "-"), "-");
        }

        /// A field with no flag is never substituted, not even by `ALL`. Guarding the trap that
        /// `DashAsZero::empty()` would have sprung: `contains(empty())` is true for every set, so
        /// the empty set as a field would have meant *always*, the exact opposite of never.
        #[test]
        fn a_field_with_no_flag_is_never_substituted() {
            assert_eq!(substituted(DashAsZero::all(), None, "-"), "-");
            assert_eq!(substituted(DashAsZero::empty(), None, "-"), "-");
        }

        #[test]
        fn one_flag_of_several_is_enough_for_its_own_field() {
            let flags = DashAsZero::MarketValue | DashAsZero::PercNetAssets;
            assert_eq!(substituted(flags, Some(DashAsZero::PercNetAssets), "–"), "0");
            assert_eq!(substituted(flags, Some(DashAsZero::AcquisitionCost), "–"), "–");
        }
    }
}
