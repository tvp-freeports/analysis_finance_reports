//! [`Promise`]: a reference to a value not yet known when the block carrying it is built.
//!
//! A page often produces a value that depends on something written on a *different* page of the
//! same document — the fund name printed once on a cover page, the currency declared in a header,
//! a management company named in a footnote. Rather than ordering the pages or making two passes,
//! the deserializer leaves a [`Promise`] where the value should go; once the document is finished
//! the collected promises are flattened ([`crate::core::promise_resolution`]) and the entities are
//! resolved against them ([`crate::core::promisable`]).
//!
//! This module holds only the *vocabulary*: the identifier, the two flags, and the suffix syntax.
//! Resolution itself lives in `promise_resolution`, which keeps the dependency between the two
//! one-way.
//!
//! # Suffix syntax
//!
//! A trailing `!` means *strict*, a trailing `[]` means *multiple*. The order in which they are
//! stripped is **not** symmetric — `!` first, then `[]` — which makes `"x![]"` and `"x[]!"` two
//! different promises. `tests::suffixes::strip_order_is_not_symmetric` pins that down, and
//! [`Promise`]'s [`fmt::Display`] is written to be the inverse of it.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A deferred reference to a value, resolved later against a
/// [`crate::core::promise_resolution::FlatPromiseMap`].
///
/// Immutable by construction: the fields are private and there are no setters, so the invariant
/// "the id no longer carries the suffixes that turned the flags on" cannot be broken after the
/// fact.
///
/// # Examples
///
/// ```
/// use freeports::core::promise::Promise;
///
/// let plain = Promise::new("fund");
/// assert_eq!((plain.id(), plain.strict(), plain.multiple()), ("fund", false, false));
///
/// let both = Promise::new("isin[]!");
/// assert_eq!((both.id(), both.strict(), both.multiple()), ("isin", true, true));
///
/// // the canonical form always re-parses to the same promise
/// assert_eq!(Promise::new(&both.to_string()), both);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Promise {
    id: String,
    strict: bool,
    multiple: bool,
}

/// Failures of resolving a promise.
///
/// Lives here rather than in `promise_resolution` because it is promise vocabulary, shared by both
/// sides: `promise_resolution` produces [`PromiseError::Circular`], `promisable` runs into
/// [`PromiseError::Unresolved`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PromiseError {
    /// The id has no usable value in the flattened map: missing, `Null`, or itself a promise that
    /// cannot be resolved either.
    #[error("promise '{id}' has no value in the resolution map")]
    Unresolved { id: String },
    /// A chain of references that comes back to itself. `chain` is the whole path, from the first
    /// id visited up to and including the repetition, so the message can show the entire cycle.
    #[error("circular promise chain: {}", .chain.join(" -> "))]
    Circular { chain: Vec<String> },
}

impl Promise {
    /// Builds a promise by reading the suffixes of `raw`: `"fund"`, `"fund!"`, `"fund[]"`,
    /// `"fund[]!"`.
    pub fn new(raw: &str) -> Self {
        Self::with_flags(raw, false, false)
    }

    /// Like [`Promise::new`], but with the two flags already decided by the caller.
    ///
    /// A flag that is already `true` **disables** stripping of the matching suffix:
    /// `with_flags("x!", true, false)` keeps the literal id `"x!"`. That is not an accident — it is
    /// the only way to build an id that genuinely ends in `!` or `[]`.
    pub fn with_flags(raw: &str, mut strict: bool, mut multiple: bool) -> Self {
        let mut id = raw.to_string();
        if !strict && id.ends_with('!') {
            id.pop();
            strict = true;
        }
        if !multiple && id.ends_with("[]") {
            id.truncate(id.len() - 2);
            multiple = true;
        }
        Promise { id, strict, multiple }
    }

    /// The identifier, without the suffixes that turned the flags on.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Whether the promise is *strict*: an unresolvable reference is then an error, instead of
    /// quietly dropping the entity that contains it (see [`crate::core::promisable::Fulfilled`]).
    pub fn strict(&self) -> bool {
        self.strict
    }

    /// Whether the promise is *multiple*: it then always resolves to a list, and the entity
    /// containing it is duplicated once per value.
    pub fn multiple(&self) -> bool {
        self.multiple
    }

    /// An [`PromiseError::Unresolved`] for this promise, built here so the id clone is not repeated
    /// at every resolution site.
    pub(crate) fn unresolved(&self) -> PromiseError {
        PromiseError::Unresolved { id: self.id.clone() }
    }
}

/// The canonical form: the id, then `[]` if *multiple*, then `!` if *strict*.
///
/// `[]` before `!` is not arbitrary. Because stripping removes `!` before `[]`, this is the only
/// order for which the canonical form re-parses through [`Promise::new`] into the same promise for
/// **every** promise that can be built.
impl fmt::Display for Promise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id)?;
        if self.multiple {
            f.write_str("[]")?;
        }
        if self.strict {
            f.write_str("!")?;
        }
        Ok(())
    }
}

/// Serialised as its canonical form (`"fund[]!"`) rather than as a three-field struct: that is the
/// same string format authors write in their CSV configuration, and it round-trips without loss.
impl Serialize for Promise {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Promise {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Promise::new(&raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod suffixes {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("ref", "ref", false, false; "bare id")]
        #[test_case("ref!", "ref", true, false; "trailing bang turns on strict")]
        #[test_case("ref[]", "ref", false, true; "trailing brackets turn on multiple")]
        #[test_case("ref[]!", "ref", true, true; "brackets then bang turn on both")]
        #[test_case("ref![]", "ref!", false, true; "bang then brackets: only multiple")]
        #[test_case("!", "", true, false; "bang only")]
        #[test_case("[]", "", false, true; "brackets only")]
        #[test_case("", "", false, false; "empty string")]
        #[test_case("a!b", "a!b", false, false; "non-trailing bang doesn't count")]
        #[test_case("a[]b", "a[]b", false, false; "non-trailing brackets don't count")]
        #[test_case("ref[", "ref[", false, false; "unpaired bracket")]
        #[test_case("ref]", "ref]", false, false; "lone closing bracket")]
        #[test_case("ref!!", "ref!", true, false; "only one bang is stripped")]
        #[test_case("ref[][]", "ref[]", false, true; "only one pair is stripped")]
        fn new_interprets_the_suffixes(raw: &str, id: &str, strict: bool, multiple: bool) {
            let p = Promise::new(raw);
            assert_eq!(p.id(), id);
            assert_eq!(p.strict(), strict, "strict per {raw:?}");
            assert_eq!(p.multiple(), multiple, "multiple per {raw:?}");
        }

        /// The delicate point of the whole syntax, pinned down on purpose: stripping looks at `!`
        /// **before** `[]`, so `"ref![]"` does not end in `!` and stays non-strict, while
        /// `"ref[]!"` turns both flags on.
        #[test]
        fn strip_order_is_not_symmetric() {
            let bang_then_brackets = Promise::new("ref![]");
            assert_eq!(bang_then_brackets.id(), "ref!");
            assert!(!bang_then_brackets.strict());
            assert!(bang_then_brackets.multiple());

            let brackets_then_bang = Promise::new("ref[]!");
            assert_eq!(brackets_then_bang.id(), "ref");
            assert!(brackets_then_bang.strict());
            assert!(brackets_then_bang.multiple());

            assert_ne!(bang_then_brackets, brackets_then_bang);
        }

        #[test]
        fn flag_already_on_leaves_suffix_in_id() {
            let strict = Promise::with_flags("weird!", true, false);
            assert_eq!(strict.id(), "weird!");
            assert!(strict.strict());
            assert!(!strict.multiple());

            let multiple = Promise::with_flags("weird[]", false, true);
            assert_eq!(multiple.id(), "weird[]");
            assert!(!multiple.strict());
            assert!(multiple.multiple());

            let both = Promise::with_flags("weird[]!", true, true);
            assert_eq!(both.id(), "weird[]!");
        }

        #[test]
        fn with_flags_with_flags_off_matches_new() {
            for raw in ["ref", "ref!", "ref[]", "ref[]!", "ref![]", "", "!", "[]"] {
                assert_eq!(Promise::with_flags(raw, false, false), Promise::new(raw), "raw {raw:?}");
            }
        }
    }

    mod canonical_form {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("ref", "ref"; "no flags")]
        #[test_case("ref!", "ref!"; "strict only")]
        #[test_case("ref[]", "ref[]"; "multiple only")]
        #[test_case("ref[]!", "ref[]!"; "both in canonical form")]
        #[test_case("ref![]", "ref![]"; "both in non-canonical form stay distinguishable")]
        fn display_reformats_to_canonical_form(raw: &str, expected: &str) {
            assert_eq!(Promise::new(raw).to_string(), expected);
        }

        /// The invariant that justifies `[]` before `!` in [`fmt::Display`]: for every promise that
        /// can be built, re-parsing the canonical form gives the same promise back. Checked
        /// exhaustively over all combinations of "hard" ids — ones that contain the suffixes
        /// themselves — and initial flags.
        #[test]
        fn canonical_form_reparses_identically() {
            let ids = ["", "a", "a!", "a[]", "a![]", "a[]!", "!", "[]", "][", "a!!", "a[][]"];
            for id in ids {
                for strict in [false, true] {
                    for multiple in [false, true] {
                        let p = Promise::with_flags(id, strict, multiple);
                        let reparsed = Promise::new(&p.to_string());
                        assert_eq!(reparsed, p, "id {id:?} strict {strict} multiple {multiple}");
                    }
                }
            }
        }
    }

    mod identity {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash_of(p: &Promise) -> u64 {
            let mut h = DefaultHasher::new();
            p.hash(&mut h);
            h.finish()
        }

        #[test]
        fn equal_only_when_id_and_flags_match() {
            let base = Promise::new("ref");
            assert_eq!(base, Promise::new("ref"));
            assert_ne!(base, Promise::new("ref!"));
            assert_ne!(base, Promise::new("ref[]"));
            assert_ne!(base, Promise::new("other"));
        }

        #[test]
        fn equal_promises_have_the_same_hash() {
            assert_eq!(hash_of(&Promise::new("ref[]!")), hash_of(&Promise::with_flags("ref", true, true)));
        }

        #[test]
        fn ordering_follows_id_then_strict_then_multiple() {
            let mut v = [
                Promise::with_flags("b", false, false),
                Promise::with_flags("a", true, false),
                Promise::with_flags("a", false, true),
                Promise::with_flags("a", false, false),
            ];
            v.sort();
            let canonical: Vec<String> = v.iter().map(Promise::to_string).collect();
            assert_eq!(canonical, vec!["a", "a[]", "a!", "b"]);
        }
    }

    mod serde_roundtrip {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn serializes_as_canonical_string() {
            let p = Promise::new("fund[]!");
            assert_eq!(serde_json::to_string(&p).unwrap(), "\"fund[]!\"");
        }

        #[test]
        fn every_constructible_promise_survives_json() {
            for raw in ["ref", "ref!", "ref[]", "ref[]!", "ref![]", "", "a b/c"] {
                let p = Promise::new(raw);
                let json = serde_json::to_string(&p).unwrap();
                let back: Promise = serde_json::from_str(&json).unwrap();
                assert_eq!(back, p, "raw {raw:?}");
            }
        }

        #[test]
        fn deserializes_only_from_string() {
            assert!(serde_json::from_str::<Promise>("42").is_err());
            assert!(serde_json::from_str::<Promise>("{\"id\":\"x\"}").is_err());
        }
    }

    mod errors {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn unresolved_reports_id_without_suffixes() {
            let err = Promise::new("fund[]!").unresolved();
            assert_eq!(err, PromiseError::Unresolved { id: "fund".into() });
            assert_eq!(err.to_string(), "promise 'fund' has no value in the resolution map");
        }

        #[test]
        fn circular_shows_the_full_chain() {
            let err = PromiseError::Circular { chain: vec!["a".into(), "b".into(), "a".into()] };
            assert_eq!(err.to_string(), "circular promise chain: a -> b -> a");
        }
    }
}
