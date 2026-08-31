//! [`MatchFund`]: a fund's identity, defined by its normalised name.
//!
//! The same fund appears — within one report and across reports — spelled in ways that differ by
//! accents, case, punctuation or spacing: `"Café Fund"`, `"CAFE  FUND"`, `"Cafe' Fund"`. A
//! [`MatchFund`] keeps the name **as written**, which is what ends up in the output, next to its
//! deeply normalised form ([`crate::core::normalization::deep_normalize_string`]), which is the
//! only thing equality, ordering and hashing look at.
//!
//! The normalised form is **derived and kept private**: there is no way to build a [`MatchFund`]
//! whose normalised field disagrees with its name, so the two can never drift apart.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::hash::{Hash, Hasher};

use super::normalization::deep_normalize_string;

/// A fund name, comparable up to accents, punctuation, case and spacing.
///
/// # Examples
///
/// ```
/// use freeports::core::match_fund::MatchFund;
///
/// let written = MatchFund::new("Café Fund (EUR)");
/// let shouted = MatchFund::new("CAFE  FUND EUR");
///
/// assert_eq!(written, shouted);              // same fund
/// assert_eq!(written.name(), "Café Fund (EUR)");  // but the original spelling survives
/// ```
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

    /// The name as written in the document: this is what goes into the output.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The normalised form, on which identity is based.
    pub fn normalized(&self) -> &str {
        &self.n_name
    }

    /// Whether this name and the given one denote the same fund, without building a second
    /// [`MatchFund`] to compare against.
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

/// Orders by normalised form, consistently with [`PartialEq`]: two equivalent names compare
/// `Ordering::Equal` rather than being ordered by their original spelling.
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

/// Shows the normalised form: it is what comparison acts on, and in a log it is the piece of
/// information that explains why two differently written names were treated as one fund. The
/// original name stays available through [`MatchFund::name`].
impl fmt::Display for MatchFund {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.n_name)
    }
}

/// Serialised as the **original name**: the normalised form is derivable, recomputing it on read
/// costs less than carrying it around, and leaving it out makes an inconsistent JSON impossible to
/// write in the first place.
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

        /// Normalisation is idempotent, so normalising an already normalised name yields the same
        /// identity: a `MatchFund` built from another's normalised form equals the original.
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
