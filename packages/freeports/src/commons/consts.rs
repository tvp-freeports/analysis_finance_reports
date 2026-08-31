//! The closed domain vocabularies: [`Currency`], [`SfdrArticle`], [`FinancialInstrument`].
//!
//! # Lookup by code and by name are deliberately different
//!
//! [`Currency::from_code`] is an exact ISO-code match and accepts no aliases;
//! [`Currency::from_name`] accepts the canonical names **and** the `"EURO"` alias for `EUR`. The
//! asymmetry is intentional and must not be collapsed into one lookup: a code column in a data file
//! has to mean exactly what it says, while free text naming a currency does not.
//!
//! Serialisation follows the code semantics — a bare string such as `"EUR"` — so `"EURO"` fails to
//! deserialize even though it resolves as a name.
//!
//! # Declaration order is significant
//!
//! All three enums derive `Ord` from declaration order, not alphabetically, and
//! [`Currency::variants`] yields them in that order. Consumers scanning for the first currency
//! mentioned in a text depend on it, and the enums have to be `Ord` to sit inside an ordered
//! [`BlockValue`](crate::core::classes::value::BlockValue).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FinancialInstrument {
    EQUITY,
    BOND,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SfdrArticle {
    Art6,
    Art8,
    Art9,
}

/// ISO 3-letter currency codes, in the same order as the reference Python `Currency` enum
/// (order matters: it's the iteration/`__members__` order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Currency {
    USD,
    EUR,
    GBP,
    JPY,
    CNY,
    AUD,
    CAD,
    CHF,
    CNH,
    SEK,
    NOK,
    DKK,
    SGD,
    HKD,
    KRW,
    INR,
    BRL,
    MXN,
    RUB,
    ZAR,
    TRY,
    PLN,
    THB,
    IDR,
    MYR,
    PHP,
    ILS,
    AED,
    SAR,
    QAR,
    KWD,
    CLP,
    COP,
    PEN,
    ARS,
    VND,
    UAH,
    CZK,
    HUF,
    RON,
    HRK,
    BGN,
    ISK,
    NZD,
    EGP,
    TWD,
}

impl Currency {
    /// All 46 canonical members, in declaration order.
    pub fn variants() -> &'static [Currency] {
        use Currency::*;
        &[
            USD, EUR, GBP, JPY, CNY, AUD, CAD, CHF, CNH, SEK, NOK, DKK, SGD, HKD, KRW, INR, BRL,
            MXN, RUB, ZAR, TRY, PLN, THB, IDR, MYR, PHP, ILS, AED, SAR, QAR, KWD, CLP, COP, PEN,
            ARS, VND, UAH, CZK, HUF, RON, HRK, BGN, ISK, NZD, EGP, TWD,
        ]
    }

    pub fn code(&self) -> &'static str {
        match self {
            Currency::USD => "USD",
            Currency::EUR => "EUR",
            Currency::GBP => "GBP",
            Currency::JPY => "JPY",
            Currency::CNY => "CNY",
            Currency::AUD => "AUD",
            Currency::CAD => "CAD",
            Currency::CHF => "CHF",
            Currency::CNH => "CNH",
            Currency::SEK => "SEK",
            Currency::NOK => "NOK",
            Currency::DKK => "DKK",
            Currency::SGD => "SGD",
            Currency::HKD => "HKD",
            Currency::KRW => "KRW",
            Currency::INR => "INR",
            Currency::BRL => "BRL",
            Currency::MXN => "MXN",
            Currency::RUB => "RUB",
            Currency::ZAR => "ZAR",
            Currency::TRY => "TRY",
            Currency::PLN => "PLN",
            Currency::THB => "THB",
            Currency::IDR => "IDR",
            Currency::MYR => "MYR",
            Currency::PHP => "PHP",
            Currency::ILS => "ILS",
            Currency::AED => "AED",
            Currency::SAR => "SAR",
            Currency::QAR => "QAR",
            Currency::KWD => "KWD",
            Currency::CLP => "CLP",
            Currency::COP => "COP",
            Currency::PEN => "PEN",
            Currency::ARS => "ARS",
            Currency::VND => "VND",
            Currency::UAH => "UAH",
            Currency::CZK => "CZK",
            Currency::HUF => "HUF",
            Currency::RON => "RON",
            Currency::HRK => "HRK",
            Currency::BGN => "BGN",
            Currency::ISK => "ISK",
            Currency::NZD => "NZD",
            Currency::EGP => "EGP",
            Currency::TWD => "TWD",
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Currency::USD => "$",
            Currency::EUR => "€",
            Currency::GBP => "£",
            Currency::JPY => "¥",
            Currency::CNY => "¥",
            Currency::AUD => "$",
            Currency::CAD => "$",
            Currency::CHF => "CHF",
            Currency::CNH => "¥",
            Currency::SEK => "kr",
            Currency::NOK => "kr",
            Currency::DKK => "kr",
            Currency::SGD => "$",
            Currency::HKD => "$",
            Currency::KRW => "₩",
            Currency::INR => "₹",
            Currency::BRL => "R$",
            Currency::MXN => "$",
            Currency::RUB => "₽",
            Currency::ZAR => "R",
            Currency::TRY => "₺",
            Currency::PLN => "zł",
            Currency::THB => "฿",
            Currency::IDR => "Rp",
            Currency::MYR => "RM",
            Currency::PHP => "₱",
            Currency::ILS => "₪",
            Currency::AED => "د.إ",
            Currency::SAR => "﷼",
            Currency::QAR => "ر.ق",
            Currency::KWD => "د.ك",
            Currency::CLP => "$",
            Currency::COP => "$",
            Currency::PEN => "S/.",
            Currency::ARS => "$",
            Currency::VND => "₫",
            Currency::UAH => "₴",
            Currency::CZK => "Kč",
            Currency::HUF => "Ft",
            Currency::RON => "lei",
            Currency::HRK => "kn",
            Currency::BGN => "лв",
            Currency::ISK => "kr",
            Currency::NZD => "$",
            Currency::EGP => "ج.م",
            Currency::TWD => "$",
        }
    }

    /// Value-based lookup: exact ISO code match only, no aliases. Mirrors the reference's
    /// `Currency(value)` (`_value2member_map_` — aliases don't get their own entry).
    pub fn from_code(code: &str) -> Option<Currency> {
        Currency::variants()
            .iter()
            .copied()
            .find(|c| c.code() == code)
    }

    /// Name-based lookup: accepts both canonical member names and the `EURO` alias for `EUR`.
    /// Mirrors the reference's `Currency[name]` (`_member_map_`, which does include aliases).
    pub fn from_name(name: &str) -> Option<Currency> {
        if name == "EURO" {
            return Some(Currency::EUR);
        }
        Currency::variants()
            .iter()
            .copied()
            .find(|c| c.code() == name)
    }
}

/// Serializes as a bare ISO-code JSON string (`"EUR"`), not `{"code":"EUR"}`.
impl Serialize for Currency {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.code())
    }
}

/// Deserializes using exact-code-match semantics (`from_code`), deliberately not accepting the
/// `"EURO"` alias that `from_name` accepts as a lookup convenience — see the module doc.
impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let code = String::deserialize(deserializer)?;
        Currency::from_code(&code)
            .ok_or_else(|| serde::de::Error::custom(format!("{code:?} is not a valid Currency")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod financial_instrument {
        use super::*;

        #[test]
        fn equity_round_trips_through_json() {
            let json = serde_json::to_string(&FinancialInstrument::EQUITY).unwrap();
            let back: FinancialInstrument = serde_json::from_str(&json).unwrap();
            assert_eq!(back, FinancialInstrument::EQUITY);
        }

        #[test]
        fn bond_round_trips_through_json() {
            let json = serde_json::to_string(&FinancialInstrument::BOND).unwrap();
            let back: FinancialInstrument = serde_json::from_str(&json).unwrap();
            assert_eq!(back, FinancialInstrument::BOND);
        }

        #[test]
        fn equity_and_bond_have_distinct_json_representations() {
            let equity = serde_json::to_string(&FinancialInstrument::EQUITY).unwrap();
            let bond = serde_json::to_string(&FinancialInstrument::BOND).unwrap();
            assert_ne!(equity, bond);
        }

        #[test]
        fn variants_compare_equal_to_themselves_and_not_to_each_other() {
            assert_eq!(FinancialInstrument::EQUITY, FinancialInstrument::EQUITY);
            assert_ne!(FinancialInstrument::EQUITY, FinancialInstrument::BOND);
        }

        #[test]
        fn dedups_via_hashset_when_equal() {
            use std::collections::HashSet;
            let mut set = HashSet::new();
            set.insert(FinancialInstrument::EQUITY);
            set.insert(FinancialInstrument::EQUITY);
            set.insert(FinancialInstrument::BOND);
            assert_eq!(set.len(), 2);
        }
    }

    mod sfdr_article {
        use super::*;

        #[test]
        fn art_6_round_trips_through_json() {
            let json = serde_json::to_string(&SfdrArticle::Art6).unwrap();
            let back: SfdrArticle = serde_json::from_str(&json).unwrap();
            assert_eq!(back, SfdrArticle::Art6);
        }

        #[test]
        fn art_8_round_trips_through_json() {
            let json = serde_json::to_string(&SfdrArticle::Art8).unwrap();
            let back: SfdrArticle = serde_json::from_str(&json).unwrap();
            assert_eq!(back, SfdrArticle::Art8);
        }

        #[test]
        fn art_9_round_trips_through_json() {
            let json = serde_json::to_string(&SfdrArticle::Art9).unwrap();
            let back: SfdrArticle = serde_json::from_str(&json).unwrap();
            assert_eq!(back, SfdrArticle::Art9);
        }

        #[test]
        fn all_three_variants_have_pairwise_distinct_json_representations() {
            use std::collections::HashSet;
            let reprs: HashSet<String> = [SfdrArticle::Art6, SfdrArticle::Art8, SfdrArticle::Art9]
                .iter()
                .map(|a| serde_json::to_string(a).unwrap())
                .collect();
            assert_eq!(reprs.len(), 3, "expected 3 distinct JSON representations");
        }

        #[test]
        fn dedups_via_hashset_when_equal() {
            use std::collections::HashSet;
            let mut set = HashSet::new();
            set.insert(SfdrArticle::Art6);
            set.insert(SfdrArticle::Art6);
            set.insert(SfdrArticle::Art8);
            set.insert(SfdrArticle::Art9);
            assert_eq!(set.len(), 3);
        }
    }

    mod currency {
        use super::*;

        mod lookup {
            use super::*;

            #[test]
            fn value_lookup_finds_exact_code() {
                assert_eq!(Currency::from_code("EUR"), Some(Currency::EUR));
            }

            #[test]
            fn value_lookup_rejects_alias() {
                assert_eq!(Currency::from_code("EURO"), None);
            }

            #[test]
            fn name_lookup_accepts_alias() {
                assert_eq!(Currency::from_name("EURO"), Some(Currency::EUR));
            }

            #[test]
            fn name_lookup_finds_canonical_name() {
                assert_eq!(Currency::from_name("USD"), Some(Currency::USD));
            }

            #[test]
            fn unknown_code_is_none() {
                assert_eq!(Currency::from_code("XXX"), None);
            }

            #[test]
            fn unknown_name_is_none() {
                assert_eq!(Currency::from_name("XXX"), None);
            }

            #[test]
            fn value_lookup_is_case_sensitive() {
                // Mirrors the old `Currency("eur")` raising `ValueError`: no case folding.
                assert_eq!(Currency::from_code("eur"), None);
            }

            #[test]
            fn empty_string_is_not_a_currency() {
                assert_eq!(Currency::from_code(""), None);
                assert_eq!(Currency::from_name(""), None);
            }
        }

        mod membership {
            use super::*;

            #[test]
            fn has_46_canonical_members() {
                assert_eq!(Currency::variants().len(), 46);
            }

            #[test]
            fn every_variant_has_a_distinct_code() {
                let mut codes: Vec<&str> = Currency::variants().iter().map(|c| c.code()).collect();
                let n = codes.len();
                codes.sort_unstable();
                codes.dedup();
                assert_eq!(codes.len(), n, "duplicate currency code found");
            }

            #[test]
            fn every_variant_round_trips_through_from_code() {
                for c in Currency::variants() {
                    assert_eq!(Currency::from_code(c.code()), Some(*c));
                }
            }

            #[test]
            fn declaration_order_matches_reference_enum() {
                // Order is significant (see module doc + the reference file's own doc comment):
                // it mirrors `freeports_core`'s `Currency.__members__` iteration order.
                use Currency::*;
                let expected = [
                    USD, EUR, GBP, JPY, CNY, AUD, CAD, CHF, CNH, SEK, NOK, DKK, SGD, HKD, KRW,
                    INR, BRL, MXN, RUB, ZAR, TRY, PLN, THB, IDR, MYR, PHP, ILS, AED, SAR, QAR,
                    KWD, CLP, COP, PEN, ARS, VND, UAH, CZK, HUF, RON, HRK, BGN, ISK, NZD, EGP,
                    TWD,
                ];
                assert_eq!(Currency::variants(), &expected[..]);
            }
        }

        mod symbols {
            use super::*;

            #[test]
            fn symbol_matches_known_values() {
                assert_eq!(Currency::EUR.symbol(), "€");
                assert_eq!(Currency::USD.symbol(), "$");
                assert_eq!(Currency::CHF.symbol(), "CHF");
                assert_eq!(Currency::JPY.symbol(), "¥");
            }

            #[test]
            fn every_variant_has_a_non_empty_symbol() {
                for c in Currency::variants() {
                    assert!(!c.symbol().is_empty(), "{} has an empty symbol", c.code());
                }
            }
        }

        mod serde_roundtrip {
            use super::*;

            #[test]
            fn serializes_as_bare_iso_code_string() {
                assert_eq!(
                    serde_json::to_value(Currency::EUR).unwrap(),
                    serde_json::json!("EUR")
                );
            }

            #[test]
            fn every_variant_round_trips_through_json() {
                for c in Currency::variants() {
                    let json = serde_json::to_string(c).unwrap();
                    let back: Currency = serde_json::from_str(&json).unwrap();
                    assert_eq!(back, *c);
                }
            }

            #[test]
            fn deserialize_uses_exact_code_match_not_alias() {
                // Deserialization mirrors `from_code`, not `from_name`: the "EURO" alias is a
                // lookup convenience, not a wire format, so it must NOT deserialize.
                let result: Result<Currency, _> =
                    serde_json::from_value(serde_json::json!("EURO"));
                assert!(result.is_err());
            }

            #[test]
            fn deserialize_rejects_unknown_code() {
                let result: Result<Currency, _> = serde_json::from_value(serde_json::json!("XXX"));
                assert!(result.is_err());
            }
        }

        mod traits {
            use super::*;

            #[test]
            fn dedups_via_hashset_when_equal() {
                use std::collections::HashSet;
                let mut set = HashSet::new();
                set.insert(Currency::EUR);
                set.insert(Currency::EUR);
                set.insert(Currency::USD);
                assert_eq!(set.len(), 2);
            }

            #[test]
            #[allow(clippy::clone_on_copy)] // `.clone()` here is the thing under test.
            fn is_copy_and_clone() {
                let a = Currency::EUR;
                let b = a; // Copy, not a move: `a` must still be usable below.
                let c = a.clone();
                assert_eq!(a, b);
                assert_eq!(a, c);
            }
        }
    }
}
