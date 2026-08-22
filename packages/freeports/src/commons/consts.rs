//! Pure-Rust domain enums: `Currency`, `SfdrArticle`, `FinancialInstrument`.
//!
//! Port of the *logic* half of `packages/freeports_core/src/commons/consts.rs` (see that
//! reference file's module doc for the value-lookup vs. name-lookup asymmetry this preserves).
//! All PyO3 / pydantic-schema machinery from the reference (`#[pyclass]`, `__class_getitem__`,
//! `__get_pydantic_core_schema__`, the `__members__` `PyDict` classattr, singleton `is`
//! semantics, the custom-metaclass `__iter__` workaround) is dropped: `commons` is not one of
//! the two Python-boundary modules (`PLAN.md` §3), and this crate has no Python shim layer yet
//! (`PLAN.md` §0).
//!
//! ## Deviations from the reference worth calling out
//!
//! - **`SfdrArticle` variant names.** The Python-era names `ART_6`/`ART_8`/`ART_9` trip rustc's
//!   `non_camel_case_types` lint (verified empirically: `rustc -W non-camel-case-types` flags
//!   them and suggests exactly `Art6`/`Art8`/`Art9`) because of the `_<digit>` suffix — unlike
//!   `Currency`'s or `FinancialInstrument`'s all-caps variants (`USD`, `EQUITY`, ...), which
//!   rustc's heuristic accepts as acronym-shaped and does NOT flag. Renamed here to
//!   `Art6`/`Art8`/`Art9` to stay lint-clean without an `#[allow(...)]` escape hatch.
//!   Declaration order (6, 8, 9) is unchanged from the reference.
//! - **`int_value()` is dropped, not ported.** The reference's `int_value()` tables
//!   (`EQUITY=1,BOND=2`, `ART_6=1,ART_8=2,ART_9=3`) existed only for Python `enum.value` /
//!   pydantic-dump compatibility (`__repr__`, the `.value` getter, `pydantic_value_ser_schema`).
//!   Nothing in this crate's design (`PLAN.md` §4.1, §9) calls for a numeric identity for these
//!   two enums, and the tests below deliberately do not assert one.
//!   **Open question, flagged rather than guessed**: confirm with the user that dropping
//!   `int_value()` permanently (vs. re-adding it later for, say, an output CSV column) is
//!   correct — see the task report for this milestone.
//!
//! ## Expected API surface these tests assume (the contract the implementer must meet)
//!
//! - `Currency::variants() -> &'static [Currency]` — all 46 members, in the reference's
//!   declaration order (order is significant, see the reference module doc).
//! - `Currency::code(&self) -> &'static str` — canonical ISO code.
//! - `Currency::symbol(&self) -> &'static str` — currency symbol.
//! - `Currency::from_code(code: &str) -> Option<Currency>` — exact ISO-code match only, no
//!   aliases: mirrors the old `Currency(value)`.
//! - `Currency::from_name(name: &str) -> Option<Currency>` — canonical names *and* the `"EURO"`
//!   alias for `EUR`: mirrors the old `Currency[name]`. Deliberately asymmetric with
//!   `from_code` — do not collapse the two into one lookup.
//! - `Currency` (de)serializes via its ISO code as a bare JSON string (`"EUR"`, not
//!   `{"code":"EUR"}`), using `from_code`/`code` semantics — i.e. deserialization is
//!   exact-match, *not* alias-accepting (`"EURO"` must fail to deserialize even though
//!   `from_name("EURO")` succeeds as a lookup).
//! - `FinancialInstrument`, `SfdrArticle` derive `Serialize`/`Deserialize` with serde's default
//!   (externally tagged) enum representation — no custom wire format needed.
//! - All three enums derive `Clone, Copy, Debug, PartialEq, Eq, Hash` (required so they can sit
//!   inside `core::classes::value::BlockValue`, see `PLAN.md` §4.1).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FinancialInstrument {
    EQUITY,
    BOND,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SfdrArticle {
    Art6,
    Art8,
    Art9,
}

/// ISO 3-letter currency codes, in the same order as the reference Python `Currency` enum
/// (order matters: it's the iteration/`__members__` order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
