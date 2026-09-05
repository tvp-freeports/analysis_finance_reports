//! How an entity with promised fields is resolved against a flattened promise map.
//!
//! [`Promised<T>`] is a single field — either already a `T`, or still a
//! [`Promise`]. [`PromisableFields`] is what an entity must be able to do for
//! [`fulfill_promises`] to resolve it: list the fields still pending, and assign one by name. The
//! real entities live in `output::classes`; what is here is only the mechanism.
//!
//! # Why the outcome is an enum
//!
//! Resolution has three outcomes — resolved in place, the entity disappears, the entity multiplies
//! — and encoding them as `None` / `Some([])` / `Some([…])` would give three meanings to one type,
//! distinguishable only by reading the caller. [`Fulfilled`] gives each of them a name.
//!
//! # Two phases, in this order
//!
//! 1. fields with an ordinary promise resolve **in place**, mutating the entity;
//! 2. only then do fields with a *multiple* promise expand the entity into one copy per value —
//!    a cartesian product if more than one field is multiple, in the order the fields appear in
//!    [`PromisableFields::pending`].
//!
//! The order matters for work, not just for semantics: the copies produced by phase 2 already
//! carry the values resolved in phase 1, instead of resolving them once per copy.

use serde::{Deserialize, Serialize};

use super::classes::value::{BlockValue, BlockValueError};
use super::promise::{Promise, PromiseError};
use super::promise_resolution::FlatPromiseMap;
use crate::core::tracing_setup::log_error;

/// A field that is either already resolved, or still a promise.
///
/// # The serde representation is tagged, and has to be
///
/// `{"resolved": …}` or `{"pending": "fund[]!"}`, for the same reason
/// [`BlockValue`](crate::core::classes::value::BlockValue) is tagged: written flat, a pending
/// promise is indistinguishable from a resolved value — literally so for `T = String`, where the
/// promise's id and a legitimate name are both just text.
///
/// This is not a theoretical tidiness. A worker process serialises its results and the parent reads
/// them back **before** the promises are fulfilled, since fulfilment happens once, in the parent,
/// over every job. Every pending field of every entity therefore crosses that boundary. Untagged,
/// the crossing either failed loudly — a promise id landing where an `SfdrArticle` was expected,
/// which aborted whole batches — or, worse, succeeded quietly, giving a fund the name of the
/// promise that should have filled it in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Promised<T> {
    Resolved(T),
    Pending(Promise),
}

impl<T> Promised<T> {
    /// The value, if the field is already resolved.
    pub fn resolved(&self) -> Option<&T> {
        match self {
            Promised::Resolved(v) => Some(v),
            Promised::Pending(_) => None,
        }
    }

    /// The promise, if the field is still pending.
    pub fn pending(&self) -> Option<&Promise> {
        match self {
            Promised::Pending(p) => Some(p),
            Promised::Resolved(_) => None,
        }
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Promised::Pending(_))
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self, Promised::Resolved(_))
    }

    /// Consumes the field and yields the resolved value, if there is one.
    pub fn into_resolved(self) -> Option<T> {
        match self {
            Promised::Resolved(v) => Some(v),
            Promised::Pending(_) => None,
        }
    }

    /// Maps the resolved value, leaving a pending promise untouched.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Promised<U> {
        match self {
            Promised::Resolved(v) => Promised::Resolved(f(v)),
            Promised::Pending(p) => Promised::Pending(p),
        }
    }
}

impl<T> From<Promise> for Promised<T> {
    fn from(p: Promise) -> Self {
        Promised::Pending(p)
    }
}

/// What happened to an entity passed to [`fulfill_promises`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fulfilled<T> {
    /// Every promise resolved in place: the entity handed in is the good one.
    InPlace,
    /// A non-strict promise could not be resolved: the entity is to be dropped.
    Dropped,
    /// At least one field was *multiple*: the entity is replaced by these copies.
    ///
    /// The list may hold a single element — a *multiple* promise with one value — and it is still
    /// `Expanded`, because the caller must replace the entity with the list's contents rather than
    /// keep the one it had.
    Expanded(Vec<T>),
}

/// Failures of resolving an entity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PromisableError {
    /// The resolved value was not of the type the field expects.
    #[error("field '{field}': {source}")]
    Field {
        field: &'static str,
        #[source]
        source: BlockValueError,
    },
    /// A *strict* promise could not be resolved.
    #[error(transparent)]
    Promise(#[from] PromiseError),
}

/// What an entity must be able to do to be resolvable.
///
/// Field names are `&'static str` rather than `String`: they are the struct's own field names,
/// known at compile time, which saves one allocation per pending field per entity.
pub trait PromisableFields: Clone {
    /// The fields still pending, with their promise, in a stable order — the order the fields are
    /// declared in. The order of phase 2's cartesian product follows from it.
    fn pending(&self) -> Vec<(&'static str, Promise)>;

    /// Assigns the resolved value to `field`.
    ///
    /// `field` is always one of the names returned by [`PromisableFields::pending`]. The
    /// implementation converts the [`BlockValue`] into the field's type and reports a
    /// [`BlockValueError`] if it does not convert.
    fn resolve_field(&mut self, field: &'static str, value: BlockValue) -> Result<(), BlockValueError>;
}

/// Resolves every promise of `entity` against `map`.
///
/// See [`Fulfilled`] for the outcome, and the module documentation for the order of the two phases.
pub fn fulfill_promises<T: PromisableFields>(
    entity: &mut T,
    map: &FlatPromiseMap,
) -> Result<Fulfilled<T>, PromisableError> {
    let mut multiples = Vec::new();

    // Phase 1: ordinary promises, resolved in place.
    for (field, promise) in entity.pending() {
        if promise.multiple() {
            multiples.push((field, promise));
            continue;
        }
        match map.fulfill(&promise) {
            Ok(value) => assign(entity, field, value)?,
            Err(err) if promise.strict() => return Err(err.into()),
            Err(err) => {
                // Non-strict, so the entity disappears rather than travelling up as an error.
                // Logged before it is dropped, because a dropped entity is otherwise invisible.
                tracing::warn!(
                    coord_ref_2 = field,
                    promise = %promise,
                    error = log_error(&err),
                    "unresolved promise: entity dropped - {err}"
                );
                return Ok(Fulfilled::Dropped);
            }
        }
    }

    if multiples.is_empty() {
        return Ok(Fulfilled::InPlace);
    }

    // Phase 2: multiple promises, one copy per value.
    let mut expansions = vec![entity.clone()];
    for (field, promise) in multiples {
        let values = match map.fulfill(&promise) {
            Ok(v) => v,
            Err(err) if promise.strict() => return Err(err.into()),
            Err(err) => {
                tracing::warn!(
                    coord_ref_2 = field,
                    promise = %promise,
                    error = log_error(&err),
                    "unresolved promise: entity dropped - {err}"
                );
                return Ok(Fulfilled::Dropped);
            }
        };
        // `FlatPromiseMap::fulfill` on a *multiple* promise always returns a non-empty `List`; the
        // `other` branch only covers that contract changing. Its elements are the contributions one
        // by one, and a contribution may itself be a `List`: the copy then receives that list as
        // the field's value, and it is up to `resolve_field` to accept or refuse it.
        let values = match values {
            BlockValue::List(items) => items,
            other => vec![other],
        };
        let mut next = Vec::with_capacity(expansions.len() * values.len());
        for base in &expansions {
            for value in &values {
                let mut copy = base.clone();
                assign(&mut copy, field, value.clone())?;
                next.push(copy);
            }
        }
        expansions = next;
    }

    Ok(Fulfilled::Expanded(expansions))
}

/// Assigns a field, naming it in the error: without this, a [`BlockValueError`] would travel up
/// without saying *which* entity and which field produced it.
fn assign<T: PromisableFields>(
    entity: &mut T,
    field: &'static str,
    value: BlockValue,
) -> Result<(), PromisableError> {
    entity
        .resolve_field(field, value)
        .map_err(|source| PromisableError::Field { field, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test entity with two promisable fields and one that is never promised — the minimum needed
    /// to exercise both phases and the cartesian product.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Investment {
        fund: Promised<String>,
        quantity: Promised<i64>,
        note: String,
    }

    impl Investment {
        fn new(fund: Promised<String>, quantity: Promised<i64>) -> Self {
            Investment { fund, quantity, note: "fissa".into() }
        }

        fn resolved(fund: &str, quantity: i64) -> Self {
            Investment::new(Promised::Resolved(fund.into()), Promised::Resolved(quantity))
        }
    }

    impl PromisableFields for Investment {
        fn pending(&self) -> Vec<(&'static str, Promise)> {
            let mut out = Vec::new();
            if let Some(p) = self.fund.pending() {
                out.push(("fund", p.clone()));
            }
            if let Some(p) = self.quantity.pending() {
                out.push(("quantity", p.clone()));
            }
            out
        }

        fn resolve_field(
            &mut self,
            field: &'static str,
            value: BlockValue,
        ) -> Result<(), BlockValueError> {
            match field {
                "fund" => self.fund = Promised::Resolved(value.str_or_fail(field)?.to_string()),
                "quantity" => self.quantity = Promised::Resolved(value.int_or_fail(field)?),
                other => return Err(BlockValueError::MissingField { field: other.to_string() }),
            }
            Ok(())
        }
    }

    /// Repeated keys mean several contributions for the same id: `from_pairs` accumulates rather
    /// than overwriting. That is how "two candidates for one id" is expressed — a single
    /// [`BlockValue::List`] would instead be *one* contribution that happens to be a list, which is
    /// a different thing.
    fn flat_map(pairs: Vec<(&str, BlockValue)>) -> FlatPromiseMap {
        FlatPromiseMap::from_pairs(pairs)
    }

    fn pending(raw: &str) -> Promised<String> {
        Promised::Pending(Promise::new(raw))
    }

    fn pending_i64(raw: &str) -> Promised<i64> {
        Promised::Pending(Promise::new(raw))
    }

    mod promised_field {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn distinguishes_resolved_from_pending() {
            let resolved: Promised<i64> = Promised::Resolved(3);
            assert!(resolved.is_resolved());
            assert!(!resolved.is_pending());
            assert_eq!(resolved.resolved(), Some(&3));
            assert_eq!(resolved.pending(), None);

            let pending_field = pending_i64("x!");
            assert!(pending_field.is_pending());
            assert!(!pending_field.is_resolved());
            assert_eq!(pending_field.resolved(), None);
            assert_eq!(pending_field.pending(), Some(&Promise::new("x!")));
        }

        #[test]
        fn into_resolved_consumes_the_field() {
            assert_eq!(Promised::Resolved("x".to_string()).into_resolved(), Some("x".to_string()));
            assert_eq!(pending("x").into_resolved(), None);
        }

        #[test]
        fn map_transforms_only_the_resolved_value() {
            assert_eq!(Promised::Resolved(2_i64).map(|v| v * 2), Promised::Resolved(4));
            assert_eq!(pending_i64("x").map(|v| v * 2), Promised::Pending(Promise::new("x")));
        }

        #[test]
        fn is_built_from_a_promise() {
            let field: Promised<i64> = Promise::new("x[]").into();
            assert_eq!(field, Promised::Pending(Promise::new("x[]")));
        }

        #[test]
        fn serializes_with_the_tag_that_says_which_of_the_two_it_is() {
            assert_eq!(serde_json::to_string(&Promised::Resolved(3_i64)).unwrap(), r#"{"resolved":3}"#);
            assert_eq!(
                serde_json::to_string(&pending_i64("fund[]!")).unwrap(),
                r#"{"pending":"fund[]!"}"#
            );
        }

        #[test]
        fn a_resolved_field_reads_back_as_the_same_value() {
            let field = Promised::Resolved(3_i64);
            let json = serde_json::to_string(&field).unwrap();
            assert_eq!(serde_json::from_str::<Promised<i64>>(&json).unwrap(), field);
        }

        #[test]
        fn a_pending_field_reads_back_as_the_same_promise() {
            // The whole point of the tag. A worker process hands its results to the parent before
            // any promise is fulfilled, so this is the ordinary crossing, not an edge case.
            let field = pending_i64("fund[]!");
            let json = serde_json::to_string(&field).unwrap();
            assert_eq!(serde_json::from_str::<Promised<i64>>(&json).unwrap(), field);
        }

        #[test]
        fn a_promised_string_does_not_read_a_promise_back_as_a_name() {
            // The silent half of the bug the tag exists to prevent: for `T = String` a promise id
            // and a real name are both text, so an untagged form gave a fund the name of the
            // promise that was supposed to fill it in.
            let field: Promised<String> = Promised::Pending(Promise::new("fund_name"));
            let json = serde_json::to_string(&field).unwrap();
            let back: Promised<String> = serde_json::from_str(&json).unwrap();
            assert!(back.is_pending(), "read back as {back:?}");
            assert_eq!(back, field);
        }

        #[test]
        fn an_optional_promised_field_survives_in_all_three_states() {
            for field in [
                Some(Promised::Resolved(3_i64)),
                Some(pending_i64("x")),
                None,
            ] {
                let json = serde_json::to_string(&field).unwrap();
                assert_eq!(serde_json::from_str::<Option<Promised<i64>>>(&json).unwrap(), field);
            }
        }
    }

    mod no_promise {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_already_resolved_entity_stays_in_place_and_intact() {
            let mut entity = Investment::resolved("Acme", 10);
            let before = entity.clone();
            assert_eq!(fulfill_promises(&mut entity, &FlatPromiseMap::new()).unwrap(), Fulfilled::InPlace);
            assert_eq!(entity, before);
        }
    }

    mod in_place_phase {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn resolves_a_promised_field() {
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(10));
            let map = flat_map(vec![("fund", BlockValue::from("Acme"))]);
            assert_eq!(fulfill_promises(&mut entity, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(entity.fund, Promised::Resolved("Acme".into()));
        }

        #[test]
        fn resolves_multiple_fields_in_the_same_pass() {
            let mut entity = Investment::new(pending("fund"), pending_i64("qty"));
            let map = flat_map(vec![("fund", BlockValue::from("Acme")), ("qty", BlockValue::Int(7))]);
            assert_eq!(fulfill_promises(&mut entity, &map).unwrap(), Fulfilled::InPlace);
            assert_eq!(entity, Investment::resolved("Acme", 7));
        }

        #[test]
        fn does_not_touch_unpromised_fields() {
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(10));
            let map = flat_map(vec![("fund", BlockValue::from("Acme"))]);
            fulfill_promises(&mut entity, &map).unwrap();
            assert_eq!(entity.note, "fissa");
            assert_eq!(entity.quantity, Promised::Resolved(10));
        }

        /// Two contributions for the same id — two pages promising the same field — and the last
        /// one wins, that is, the later page.
        #[test]
        fn on_several_contributions_takes_the_last_value() {
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(1));
            let map = flat_map(vec![
                ("fund", BlockValue::from("Vecchio")),
                ("fund", BlockValue::from("Nuovo")),
            ]);
            fulfill_promises(&mut entity, &map).unwrap();
            assert_eq!(entity.fund, Promised::Resolved("Nuovo".into()));
        }
    }

    mod unresolvable_promise {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn non_strict_makes_the_entity_disappear() {
            let mut entity = Investment::new(pending("assente"), Promised::Resolved(1));
            assert_eq!(
                fulfill_promises(&mut entity, &FlatPromiseMap::new()).unwrap(),
                Fulfilled::Dropped
            );
        }

        #[test]
        fn strict_is_an_error() {
            let mut entity = Investment::new(pending("assente!"), Promised::Resolved(1));
            assert_eq!(
                fulfill_promises(&mut entity, &FlatPromiseMap::new()).unwrap_err(),
                PromisableError::Promise(PromiseError::Unresolved { id: "assente".into() })
            );
        }

        /// A promise that survived flattening becomes a drop or an error depending on `strict`, not
        /// a flattening error: that is where the policy on pending references closes.
        #[test]
        fn a_promise_surviving_flattening_behaves_like_a_missing_id() {
            let map = flat_map(vec![("fund", BlockValue::Promise(Promise::new("nowhere")))]);

            let mut non_strict = Investment::new(pending("fund"), Promised::Resolved(1));
            assert_eq!(fulfill_promises(&mut non_strict, &map).unwrap(), Fulfilled::Dropped);

            let mut strict = Investment::new(pending("fund!"), Promised::Resolved(1));
            assert!(fulfill_promises(&mut strict, &map).is_err());
        }

        /// A `Null` contribution counts as an absent value. The map is built here by going through
        /// `flatten`, which is the only way a `Null` can really turn up — flattening already
        /// discards it, so the id never reaches `fulfill` at all.
        #[test]
        fn a_null_value_counts_as_missing() {
            let promises: crate::core::promise_resolution::PromiseMap =
                [("fund", BlockValue::Null)].into_iter().collect();
            let map = promises.flatten().unwrap();
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(1));
            assert_eq!(fulfill_promises(&mut entity, &map).unwrap(), Fulfilled::Dropped);
        }

        #[test]
        fn an_unresolvable_non_strict_multiple_makes_the_entity_disappear() {
            let mut entity = Investment::new(pending("assente[]"), Promised::Resolved(1));
            assert_eq!(
                fulfill_promises(&mut entity, &FlatPromiseMap::new()).unwrap(),
                Fulfilled::Dropped
            );
        }

        #[test]
        fn an_unresolvable_strict_multiple_is_an_error() {
            let mut entity = Investment::new(pending("assente[]!"), Promised::Resolved(1));
            assert!(fulfill_promises(&mut entity, &FlatPromiseMap::new()).is_err());
        }

        /// Dropping beats expanding: if an ordinary field fails to resolve, phase 2 never starts.
        #[test]
        fn an_unresolvable_normal_field_prevents_the_expansion() {
            let mut entity = Investment::new(pending("assente"), pending_i64("qty[]"));
            let map = flat_map(vec![("qty", BlockValue::Int(1)), ("qty", BlockValue::Int(2))]);
            assert_eq!(fulfill_promises(&mut entity, &map).unwrap(), Fulfilled::Dropped);
        }
    }

    mod expansion_phase {
        use super::*;
        use pretty_assertions::assert_eq;

        fn expansions(outcome: Fulfilled<Investment>) -> Vec<Investment> {
            match outcome {
                Fulfilled::Expanded(v) => v,
                other => panic!("attesa un'espansione, trovato {other:?}"),
            }
        }

        #[test]
        fn one_copy_per_value() {
            let mut entity = Investment::new(pending("fund[]"), Promised::Resolved(1));
            let map = flat_map(vec![
                ("fund", BlockValue::from("A")),
                ("fund", BlockValue::from("B")),
                ("fund", BlockValue::from("C")),
            ]);
            let copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            let names: Vec<&str> = copies.iter().filter_map(|c| c.fund.resolved()).map(String::as_str).collect();
            assert_eq!(names, vec!["A", "B", "C"]);
        }

        /// A single value is still an expansion rather than an `InPlace`: in both cases the caller
        /// must replace the entity with the list's contents.
        #[test]
        fn a_single_value_still_produces_an_expansion() {
            let mut entity = Investment::new(pending("fund[]"), Promised::Resolved(1));
            let map = flat_map(vec![("fund", BlockValue::from("A"))]);
            let copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            assert_eq!(copies, vec![Investment::resolved("A", 1)]);
        }

        #[test]
        fn two_multiple_fields_give_the_cartesian_product() {
            let mut entity = Investment::new(pending("fund[]"), pending_i64("qty[]"));
            let map = flat_map(vec![
                ("fund", BlockValue::from("A")),
                ("fund", BlockValue::from("B")),
                ("qty", BlockValue::Int(1)),
                ("qty", BlockValue::Int(2)),
                ("qty", BlockValue::Int(3)),
            ]);
            let copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            assert_eq!(copies.len(), 6);
            let pairs: Vec<(&str, i64)> = copies
                .iter()
                .filter_map(|c| Some((c.fund.resolved()?.as_str(), *c.quantity.resolved()?)))
                .collect();
            // The field appearing first in `pending` varies most slowly.
            assert_eq!(
                pairs,
                vec![("A", 1), ("A", 2), ("A", 3), ("B", 1), ("B", 2), ("B", 3)]
            );
        }

        /// The order of the two phases, made observable: the ordinary field is already resolved in
        /// *every* copy, so it was resolved once, before the expansion.
        #[test]
        fn the_copies_already_carry_the_fields_resolved_in_the_first_phase() {
            let mut entity = Investment::new(pending("fund"), pending_i64("qty[]"));
            let map = flat_map(vec![
                ("fund", BlockValue::from("Acme")),
                ("qty", BlockValue::Int(1)),
                ("qty", BlockValue::Int(2)),
            ]);
            let copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            assert_eq!(copies, vec![Investment::resolved("Acme", 1), Investment::resolved("Acme", 2)]);
        }

        #[test]
        fn a_multiple_on_a_scalar_value_produces_a_single_copy() {
            let mut entity = Investment::new(Promised::Resolved("Acme".into()), pending_i64("qty[]"));
            let map = flat_map(vec![("qty", BlockValue::Int(9))]);
            assert_eq!(
                expansions(fulfill_promises(&mut entity, &map).unwrap()),
                vec![Investment::resolved("Acme", 9)]
            );
        }

        #[test]
        fn the_copies_are_independent_from_each_other() {
            let mut entity = Investment::new(pending("fund[]"), Promised::Resolved(1));
            let map = flat_map(vec![
                ("fund", BlockValue::from("A")),
                ("fund", BlockValue::from("B")),
            ]);
            let mut copies = expansions(fulfill_promises(&mut entity, &map).unwrap());
            copies[0].note = "cambiata".into();
            assert_eq!(copies[1].note, "fissa");
        }
    }

    mod type_errors {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_value_of_the_wrong_type_names_the_field() {
            let mut entity = Investment::new(pending("fund"), Promised::Resolved(1));
            let map = flat_map(vec![("fund", BlockValue::Int(3))]);
            let err = fulfill_promises(&mut entity, &map).unwrap_err();
            assert_eq!(
                err,
                PromisableError::Field {
                    field: "fund",
                    source: BlockValueError::TypeMismatch {
                        field: "fund".into(),
                        expected: "str",
                        found: "int",
                    },
                }
            );
            assert_eq!(err.to_string(), "field 'fund': field 'fund' expected str, found int");
        }

        /// Three contributions for one id, the second of the wrong type: the expansion really
        /// starts (the first value is assigned), then the malformed value stops it and the error
        /// travels up instead of producing partial copies.
        ///
        /// The expected error is checked in full rather than with a `matches!` on the field name
        /// alone: without `found: "str"` the test would stay green even if no expansion happened
        /// and the failure were the assignment of a whole container.
        #[test]
        fn a_type_error_during_expansion_stops_everything() {
            let mut entity = Investment::new(Promised::Resolved("Acme".into()), pending_i64("qty[]"));
            let map = flat_map(vec![
                ("qty", BlockValue::Int(1)),
                ("qty", BlockValue::from("non un numero")),
                ("qty", BlockValue::Int(3)),
            ]);
            let err = fulfill_promises(&mut entity, &map).unwrap_err();
            assert_eq!(
                err,
                PromisableError::Field {
                    field: "quantity",
                    source: BlockValueError::TypeMismatch {
                        field: "quantity".into(),
                        expected: "int",
                        found: "str",
                    },
                }
            );
        }
    }
}
