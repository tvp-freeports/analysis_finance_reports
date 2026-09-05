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

/// ISO 3-letter currency codes.
///
/// The **first 46** are in the same order as the reference Python `Currency` enum, and must stay
/// that way: order matters, being the iteration/`__members__` order. Everything after them is the
/// rest of the active ISO 4217 list, appended — new members go at the end, where they disturb no
/// existing position.
///
/// The list is deliberately the whole of ISO 4217 and not a curated selection. A curated one was
/// what existed before, and it dropped a real holding priced in Nigerian naira because nobody had
/// foreseen a Nigerian issuer; the next gap would have been found the same way. What stays out is
/// only what is not a currency: the precious metals (`XAU`, `XAG`, `XPT`, `XPD`), the accounting
/// units (`XDR`, `XUA`, `XSU`, `XBA`–`XBD`), the fund-valuation codes (`CLF`, `USN`, `MXV`, …) and
/// the reserved `XTS`/`XXX`. A report cannot quote a holding in those.
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
    AFN,
    ALL,
    AMD,
    ANG,
    AOA,
    AWG,
    AZN,
    BAM,
    BBD,
    BDT,
    BHD,
    BIF,
    BMD,
    BND,
    BOB,
    BSD,
    BTN,
    BWP,
    BYN,
    BZD,
    CDF,
    CRC,
    CUP,
    CVE,
    DJF,
    DOP,
    DZD,
    ERN,
    ETB,
    FJD,
    FKP,
    GEL,
    GHS,
    GIP,
    GMD,
    GNF,
    GTQ,
    GYD,
    HNL,
    HTG,
    IQD,
    IRR,
    JMD,
    JOD,
    KES,
    KGS,
    KHR,
    KMF,
    KPW,
    KYD,
    KZT,
    LAK,
    LBP,
    LKR,
    LRD,
    LSL,
    LYD,
    MAD,
    MDL,
    MGA,
    MKD,
    MMK,
    MNT,
    MOP,
    MRU,
    MUR,
    MVR,
    MWK,
    MZN,
    NAD,
    NGN,
    NIO,
    NPR,
    OMR,
    PAB,
    PGK,
    PKR,
    PYG,
    RSD,
    RWF,
    SBD,
    SCR,
    SDG,
    SHP,
    SLE,
    SOS,
    SRD,
    SSP,
    STN,
    SVC,
    SYP,
    SZL,
    TJS,
    TMT,
    TND,
    TOP,
    TTD,
    TZS,
    UGX,
    UYU,
    UZS,
    VED,
    VES,
    VUV,
    WST,
    XAF,
    XCD,
    XCG,
    XOF,
    XPF,
    YER,
    ZMW,
    ZWG,
}

impl Currency {
    /// All 159 canonical members, in declaration order.
    pub fn variants() -> &'static [Currency] {
        use Currency::*;
        &[
            USD, EUR, GBP, JPY, CNY, AUD, CAD, CHF, CNH, SEK, NOK, DKK, SGD, HKD, KRW, INR, BRL,
            MXN, RUB, ZAR, TRY, PLN, THB, IDR, MYR, PHP, ILS, AED, SAR, QAR, KWD, CLP, COP, PEN,
            ARS, VND, UAH, CZK, HUF, RON, HRK, BGN, ISK, NZD, EGP, TWD,
            AFN, ALL, AMD, ANG, AOA, AWG, AZN, BAM, BBD, BDT, BHD, BIF, BMD, BND, BOB, BSD, BTN, BWP,
            BYN, BZD, CDF, CRC, CUP, CVE, DJF, DOP, DZD, ERN, ETB, FJD, FKP, GEL, GHS, GIP, GMD, GNF,
            GTQ, GYD, HNL, HTG, IQD, IRR, JMD, JOD, KES, KGS, KHR, KMF, KPW, KYD, KZT, LAK, LBP, LKR,
            LRD, LSL, LYD, MAD, MDL, MGA, MKD, MMK, MNT, MOP, MRU, MUR, MVR, MWK, MZN, NAD, NGN, NIO,
            NPR, OMR, PAB, PGK, PKR, PYG, RSD, RWF, SBD, SCR, SDG, SHP, SLE, SOS, SRD, SSP, STN, SVC,
            SYP, SZL, TJS, TMT, TND, TOP, TTD, TZS, UGX, UYU, UZS, VED, VES, VUV, WST, XAF, XCD, XCG,
            XOF, XPF, YER, ZMW, ZWG,
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
            Currency::AFN => "AFN",
            Currency::ALL => "ALL",
            Currency::AMD => "AMD",
            Currency::ANG => "ANG",
            Currency::AOA => "AOA",
            Currency::AWG => "AWG",
            Currency::AZN => "AZN",
            Currency::BAM => "BAM",
            Currency::BBD => "BBD",
            Currency::BDT => "BDT",
            Currency::BHD => "BHD",
            Currency::BIF => "BIF",
            Currency::BMD => "BMD",
            Currency::BND => "BND",
            Currency::BOB => "BOB",
            Currency::BSD => "BSD",
            Currency::BTN => "BTN",
            Currency::BWP => "BWP",
            Currency::BYN => "BYN",
            Currency::BZD => "BZD",
            Currency::CDF => "CDF",
            Currency::CRC => "CRC",
            Currency::CUP => "CUP",
            Currency::CVE => "CVE",
            Currency::DJF => "DJF",
            Currency::DOP => "DOP",
            Currency::DZD => "DZD",
            Currency::ERN => "ERN",
            Currency::ETB => "ETB",
            Currency::FJD => "FJD",
            Currency::FKP => "FKP",
            Currency::GEL => "GEL",
            Currency::GHS => "GHS",
            Currency::GIP => "GIP",
            Currency::GMD => "GMD",
            Currency::GNF => "GNF",
            Currency::GTQ => "GTQ",
            Currency::GYD => "GYD",
            Currency::HNL => "HNL",
            Currency::HTG => "HTG",
            Currency::IQD => "IQD",
            Currency::IRR => "IRR",
            Currency::JMD => "JMD",
            Currency::JOD => "JOD",
            Currency::KES => "KES",
            Currency::KGS => "KGS",
            Currency::KHR => "KHR",
            Currency::KMF => "KMF",
            Currency::KPW => "KPW",
            Currency::KYD => "KYD",
            Currency::KZT => "KZT",
            Currency::LAK => "LAK",
            Currency::LBP => "LBP",
            Currency::LKR => "LKR",
            Currency::LRD => "LRD",
            Currency::LSL => "LSL",
            Currency::LYD => "LYD",
            Currency::MAD => "MAD",
            Currency::MDL => "MDL",
            Currency::MGA => "MGA",
            Currency::MKD => "MKD",
            Currency::MMK => "MMK",
            Currency::MNT => "MNT",
            Currency::MOP => "MOP",
            Currency::MRU => "MRU",
            Currency::MUR => "MUR",
            Currency::MVR => "MVR",
            Currency::MWK => "MWK",
            Currency::MZN => "MZN",
            Currency::NAD => "NAD",
            Currency::NGN => "NGN",
            Currency::NIO => "NIO",
            Currency::NPR => "NPR",
            Currency::OMR => "OMR",
            Currency::PAB => "PAB",
            Currency::PGK => "PGK",
            Currency::PKR => "PKR",
            Currency::PYG => "PYG",
            Currency::RSD => "RSD",
            Currency::RWF => "RWF",
            Currency::SBD => "SBD",
            Currency::SCR => "SCR",
            Currency::SDG => "SDG",
            Currency::SHP => "SHP",
            Currency::SLE => "SLE",
            Currency::SOS => "SOS",
            Currency::SRD => "SRD",
            Currency::SSP => "SSP",
            Currency::STN => "STN",
            Currency::SVC => "SVC",
            Currency::SYP => "SYP",
            Currency::SZL => "SZL",
            Currency::TJS => "TJS",
            Currency::TMT => "TMT",
            Currency::TND => "TND",
            Currency::TOP => "TOP",
            Currency::TTD => "TTD",
            Currency::TZS => "TZS",
            Currency::UGX => "UGX",
            Currency::UYU => "UYU",
            Currency::UZS => "UZS",
            Currency::VED => "VED",
            Currency::VES => "VES",
            Currency::VUV => "VUV",
            Currency::WST => "WST",
            Currency::XAF => "XAF",
            Currency::XCD => "XCD",
            Currency::XCG => "XCG",
            Currency::XOF => "XOF",
            Currency::XPF => "XPF",
            Currency::YER => "YER",
            Currency::ZMW => "ZMW",
            Currency::ZWG => "ZWG",
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
            Currency::AFN => "AFN",
            Currency::ALL => "ALL",
            Currency::AMD => "AMD",
            Currency::ANG => "ANG",
            Currency::AOA => "AOA",
            Currency::AWG => "AWG",
            Currency::AZN => "AZN",
            Currency::BAM => "BAM",
            Currency::BBD => "BBD",
            Currency::BDT => "BDT",
            Currency::BHD => "BHD",
            Currency::BIF => "BIF",
            Currency::BMD => "BMD",
            Currency::BND => "BND",
            Currency::BOB => "BOB",
            Currency::BSD => "BSD",
            Currency::BTN => "BTN",
            Currency::BWP => "BWP",
            Currency::BYN => "BYN",
            Currency::BZD => "BZD",
            Currency::CDF => "CDF",
            Currency::CRC => "CRC",
            Currency::CUP => "CUP",
            Currency::CVE => "CVE",
            Currency::DJF => "DJF",
            Currency::DOP => "DOP",
            Currency::DZD => "DZD",
            Currency::ERN => "ERN",
            Currency::ETB => "ETB",
            Currency::FJD => "FJD",
            Currency::FKP => "FKP",
            Currency::GEL => "GEL",
            Currency::GHS => "GHS",
            Currency::GIP => "GIP",
            Currency::GMD => "GMD",
            Currency::GNF => "GNF",
            Currency::GTQ => "GTQ",
            Currency::GYD => "GYD",
            Currency::HNL => "HNL",
            Currency::HTG => "HTG",
            Currency::IQD => "IQD",
            Currency::IRR => "IRR",
            Currency::JMD => "JMD",
            Currency::JOD => "JOD",
            Currency::KES => "KES",
            Currency::KGS => "KGS",
            Currency::KHR => "KHR",
            Currency::KMF => "KMF",
            Currency::KPW => "KPW",
            Currency::KYD => "KYD",
            Currency::KZT => "KZT",
            Currency::LAK => "LAK",
            Currency::LBP => "LBP",
            Currency::LKR => "LKR",
            Currency::LRD => "LRD",
            Currency::LSL => "LSL",
            Currency::LYD => "LYD",
            Currency::MAD => "MAD",
            Currency::MDL => "MDL",
            Currency::MGA => "MGA",
            Currency::MKD => "MKD",
            Currency::MMK => "MMK",
            Currency::MNT => "MNT",
            Currency::MOP => "MOP",
            Currency::MRU => "MRU",
            Currency::MUR => "MUR",
            Currency::MVR => "MVR",
            Currency::MWK => "MWK",
            Currency::MZN => "MZN",
            Currency::NAD => "NAD",
            Currency::NGN => "NGN",
            Currency::NIO => "NIO",
            Currency::NPR => "NPR",
            Currency::OMR => "OMR",
            Currency::PAB => "PAB",
            Currency::PGK => "PGK",
            Currency::PKR => "PKR",
            Currency::PYG => "PYG",
            Currency::RSD => "RSD",
            Currency::RWF => "RWF",
            Currency::SBD => "SBD",
            Currency::SCR => "SCR",
            Currency::SDG => "SDG",
            Currency::SHP => "SHP",
            Currency::SLE => "SLE",
            Currency::SOS => "SOS",
            Currency::SRD => "SRD",
            Currency::SSP => "SSP",
            Currency::STN => "STN",
            Currency::SVC => "SVC",
            Currency::SYP => "SYP",
            Currency::SZL => "SZL",
            Currency::TJS => "TJS",
            Currency::TMT => "TMT",
            Currency::TND => "TND",
            Currency::TOP => "TOP",
            Currency::TTD => "TTD",
            Currency::TZS => "TZS",
            Currency::UGX => "UGX",
            Currency::UYU => "UYU",
            Currency::UZS => "UZS",
            Currency::VED => "VED",
            Currency::VES => "VES",
            Currency::VUV => "VUV",
            Currency::WST => "WST",
            Currency::XAF => "XAF",
            Currency::XCD => "XCD",
            Currency::XCG => "XCG",
            Currency::XOF => "XOF",
            Currency::XPF => "XPF",
            Currency::YER => "YER",
            Currency::ZMW => "ZMW",
            Currency::ZWG => "ZWG",
        }
    }

    /// The currencies worth **guessing** from running prose, as opposed to reading from a field
    /// that is declared to hold one.
    ///
    /// A three-letter uppercase word is thin evidence. Over the whole ISO 4217 list it is thinner
    /// than it looks, because a great many codes are also ordinary words once the text has been
    /// upper-cased: `ALL` in "at all", `TOP` in "top holdings", `CUP`, `BOB`, `SOS`, `GEL`, `MOP`.
    /// Reading those as Albanian lek or Tongan paʻanga would be worse than reading nothing.
    ///
    /// So the two questions get two answers. *What currency is this field?* — anything in ISO 4217,
    /// via [`Currency::from_code`], because the report says so. *What currency does this sentence
    /// mention?* — only these, because here the engine is inferring rather than being told.
    ///
    /// **Adding a currency:** put it in the enum, always. Put it here only if a report writes that
    /// currency's code inside a sentence, which in practice means the majors already listed.
    pub fn prose_candidates() -> &'static [Currency] {
        use Currency::*;
        &[
            USD, EUR, GBP, JPY, CNY, AUD, CAD, CHF, CNH, SEK, NOK, DKK, SGD, HKD, KRW, INR, BRL,
            MXN, RUB, ZAR, TRY, PLN, THB, IDR, MYR, PHP, ILS, AED, SAR, QAR, KWD, CLP, COP, PEN,
            ARS, VND, UAH, CZK, HUF, RON, HRK, BGN, ISK, NZD, EGP, TWD,
        ]
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
            fn has_every_canonical_member() {
                assert_eq!(Currency::variants().len(), 159);
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

            /// The 46 members that mirror the reference enum keep their order, and keep it as a
            /// **prefix**: order is significant (see the module doc and the reference file's own
            /// doc comment, `freeports_core`'s `Currency.__members__` iteration order), so those
            /// may not be reordered — but the list has to be able to grow, and growth goes at the
            /// end where it disturbs nothing.
            #[test]
            fn the_reference_members_keep_their_order_at_the_head_of_the_list() {
                use Currency::*;
                let reference = [
                    USD, EUR, GBP, JPY, CNY, AUD, CAD, CHF, CNH, SEK, NOK, DKK, SGD, HKD, KRW,
                    INR, BRL, MXN, RUB, ZAR, TRY, PLN, THB, IDR, MYR, PHP, ILS, AED, SAR, QAR,
                    KWD, CLP, COP, PEN, ARS, VND, UAH, CZK, HUF, RON, HRK, BGN, ISK, NZD, EGP,
                    TWD,
                ];
                assert_eq!(&Currency::variants()[..reference.len()], &reference[..]);
            }

            /// Nigeria's naira is the member this list grew for: a report priced a real holding in
            /// it and the engine dropped the field. The assertion is about the case, not the code —
            /// any currency a report can quote has to be in here, and the ISO 4217 codes that are
            /// not currencies at all (`XAU`, `XDR`, `XXX` and the fund-accounting units) still must
            /// not be.
            #[test]
            fn the_list_covers_currencies_reports_actually_quote_and_nothing_else() {
                for code in ["NGN", "KES", "MAD", "PKR", "BHD", "OMR", "JOD", "XOF", "GHS", "TND"] {
                    assert!(Currency::from_code(code).is_some(), "{code} should be a currency");
                }
                for code in ["XAU", "XAG", "XDR", "XXX", "XTS", "USN", "CLF"] {
                    assert!(Currency::from_code(code).is_none(), "{code} is not a currency");
                }
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
