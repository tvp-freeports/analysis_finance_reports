//! `MatchFund`: l'identita' di un fondo, definita dal suo nome normalizzato.
//!
//! Lo stesso fondo compare, dentro un report e fra report diversi, con scritture che differiscono
//! per accenti, maiuscole, punteggiatura o spaziatura ("Café Fund", "CAFE  FUND", "Cafe' Fund").
//! `MatchFund` tiene il nome **come e' scritto** — quello che finisce nell'output — accanto alla
//! sua forma profondamente normalizzata ([`crate::core::normalization::deep_normalize_string`]),
//! che e' l'unica cosa su cui si basano uguaglianza, ordine e hash.
//!
//! Porting da `freeports_core` (`src/core/match_fund.rs`) senza il confine PyO3: qui non serve
//! ne' il `#[pyclass]` ne' il ponte Python che gli faceva da mixin per Pydantic (`PLAN.md` §3).
//! La forma normalizzata resta **derivata**: i campi sono privati e non c'e' modo di costruire un
//! `MatchFund` in cui `n_name` non corrisponda a `name`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::hash::{Hash, Hasher};

use super::normalization::deep_normalize_string;

/// Nome di un fondo, confrontabile a meno di accenti, punteggiatura, maiuscole e spaziatura.
#[derive(Debug, Clone)]
pub struct MatchFund {
    name: String,
    n_name: String,
}

impl MatchFund {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let n_name = deep_normalize_string(&name);
        MatchFund { name, n_name }
    }

    /// Il nome come e' scritto nel documento: e' questo che va nell'output.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// La forma normalizzata, su cui si basa l'identita'.
    pub fn normalized(&self) -> &str {
        &self.n_name
    }

    /// Se questo nome e quello dato indicano lo stesso fondo, senza dover costruire un secondo
    /// `MatchFund`.
    pub fn matches(&self, other: &str) -> bool {
        self.n_name == deep_normalize_string(other)
    }
}

impl PartialEq for MatchFund {
    fn eq(&self, other: &Self) -> bool {
        self.n_name == other.n_name
    }
}

impl Eq for MatchFund {}

impl PartialOrd for MatchFund {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Ordina per forma normalizzata, coerentemente con [`PartialEq`]: due nomi equivalenti sono
/// `Ordering::Equal`, non ordinati per la scrittura originale.
impl Ord for MatchFund {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.n_name.cmp(&other.n_name)
    }
}

impl Hash for MatchFund {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.n_name.hash(state);
    }
}

/// Mostra la forma normalizzata: e' cio' che conta per il confronto, e nei log e' l'informazione
/// che spiega perche' due nomi diversi sono stati considerati lo stesso fondo. Il nome originale
/// resta disponibile con [`MatchFund::name`].
impl fmt::Display for MatchFund {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.n_name)
    }
}

/// Serializzato come il **nome originale**: la forma normalizzata e' derivabile e ricalcolarla in
/// lettura costa meno che portarsela dietro, oltre a rendere impossibile un JSON incoerente.
impl Serialize for MatchFund {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name)
    }
}

impl<'de> Deserialize<'de> for MatchFund {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(MatchFund::new(String::deserialize(deserializer)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod construction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn keeps_the_original_name() {
            assert_eq!(MatchFund::new("Café  Fund").name(), "Café  Fund");
        }

        #[test]
        fn computes_the_normalized_form() {
            assert_eq!(MatchFund::new("Café  Fund").normalized(), "cafe fund");
        }

        #[test]
        fn accepts_both_str_and_string() {
            assert_eq!(MatchFund::new("A").normalized(), MatchFund::new(String::from("A")).normalized());
        }

        #[test]
        fn an_empty_name_is_legitimate() {
            let empty = MatchFund::new("");
            assert_eq!(empty.name(), "");
            assert_eq!(empty.normalized(), "");
        }

        /// La normalizzazione e' idempotente, quindi normalizzare un nome gia' normalizzato da'
        /// la stessa identita': un `MatchFund` costruito dalla forma normalizzata di un altro e'
        /// uguale all'originale.
        #[test]
        fn the_normalized_form_has_the_same_identity_as_the_original() {
            let original = MatchFund::new("Café, S.p.A. – Fondo");
            assert_eq!(MatchFund::new(original.normalized()), original);
        }
    }

    mod identity {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::collections::hash_map::DefaultHasher;
        use test_case::test_case;

        fn hash_of(m: &MatchFund) -> u64 {
            let mut h = DefaultHasher::new();
            m.hash(&mut h);
            h.finish()
        }

        #[test_case("Café Fund", "CAFE   FUND"; "accents and case")]
        #[test_case("Acme S.p.A.", "Acme SpA"; "punctuation")]
        #[test_case("Rock & Roll", "Rock and Roll"; "ampersand")]
        #[test_case("Fondo-Alpha", "Fondo Alpha"; "hyphen as separator")]
        #[test_case("  Alpha  ", "Alpha"; "edge spaces")]
        fn equivalent_names_are_the_same_fund(a: &str, b: &str) {
            let (a, b) = (MatchFund::new(a), MatchFund::new(b));
            assert_eq!(a, b);
            assert_eq!(hash_of(&a), hash_of(&b), "hash inconsistent with equality");
            assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
        }

        #[test_case("Fund A", "Fund B"; "different names")]
        #[test_case("Alpha Fund", "Alphafund"; "the space matters")]
        fn different_names_are_different_funds(a: &str, b: &str) {
            assert_ne!(MatchFund::new(a), MatchFund::new(b));
        }

        #[test]
        fn matches_avoids_constructing_a_second_match_fund() {
            let fund = MatchFund::new("Café Fund");
            assert!(fund.matches("CAFE  FUND"));
            assert!(!fund.matches("Altro Fondo"));
        }

        #[test]
        fn matches_and_equality_always_agree() {
            let names = ["Café Fund", "CAFE FUND", "Altro", "", "  ", "Rock & Roll", "Rock and Roll"];
            for a in names {
                let fund = MatchFund::new(a);
                for b in names {
                    assert_eq!(fund.matches(b), fund == MatchFund::new(b), "{a:?} vs {b:?}");
                }
            }
        }

        #[test]
        fn is_usable_as_a_set_key() {
            use std::collections::HashSet;
            let mut set = HashSet::new();
            set.insert(MatchFund::new("Café Fund"));
            set.insert(MatchFund::new("CAFE   FUND"));
            set.insert(MatchFund::new("Altro"));
            assert_eq!(set.len(), 2);
        }

        #[test]
        fn sorts_by_normalized_form_not_by_original_name() {
            let mut funds = [MatchFund::new("beta"), MatchFund::new("Alpha"), MatchFund::new("Ómega")];
            funds.sort();
            let normalized: Vec<&str> = funds.iter().map(MatchFund::normalized).collect();
            assert_eq!(normalized, vec!["alpha", "beta", "omega"]);
        }
    }

    mod representation {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn display_shows_the_normalized_form() {
            assert_eq!(MatchFund::new("Café  Fund").to_string(), "cafe fund");
        }

        #[test]
        fn debug_shows_both_forms() {
            let debug = format!("{:?}", MatchFund::new("Café Fund"));
            assert!(debug.contains("Café Fund"), "{debug}");
            assert!(debug.contains("cafe fund"), "{debug}");
        }
    }

    mod serde_roundtrip {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn serializes_the_original_name() {
            assert_eq!(serde_json::to_string(&MatchFund::new("Café Fund")).unwrap(), "\"Café Fund\"");
        }

        #[test]
        fn rereads_name_and_normalized_form() {
            let original = MatchFund::new("Café  Fund");
            let reread: MatchFund = serde_json::from_str(&serde_json::to_string(&original).unwrap()).unwrap();
            assert_eq!(reread.name(), original.name());
            assert_eq!(reread.normalized(), original.normalized());
        }

        #[test]
        fn deserializes_only_from_a_string() {
            assert!(serde_json::from_str::<MatchFund>("42").is_err());
        }
    }
}
