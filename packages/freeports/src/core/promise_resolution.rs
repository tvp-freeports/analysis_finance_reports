//! The two maps through which promises are collected and then resolved.
//!
//! 1. as the pages of a document are deserialized, every pipe deposits the `(id, value)` pairs it
//!    produced into a [`PromiseMap`] — a *multimap*, because different pages can contribute to the
//!    same id: the fund name printed several times, the total repeated at the foot of every table;
//! 2. once the document is finished the multimap is **flattened** ([`PromiseMap::flatten`]):
//!    references between promises are followed and replaced by the contributions of the id they
//!    point at, while every id keeps its own **sequence** of contributions;
//! 3. the entities produced by the deserializers are resolved against the resulting
//!    [`FlatPromiseMap`] (see [`crate::core::promisable`]).
//!
//! # A container is not a contribution
//!
//! Flattening synthesises no list. `[("x", 1), ("x", 2)]` leaves two contributions;
//! `[("x", List([1, 2]))]` leaves **one** that happens to be a list; and the two flattened maps
//! stay distinguishable. This is the distinction the whole module is built to preserve, because
//! once it is lost no later stage can recover it.
//!
//! It has a consequence for pipe authors: to deposit several contributions for one id, a pipe
//! returns a list of separate dicts (`[{"id": a}, {"id": b}]`), not one dict with a list value
//! (`{"id": [a, b]}`, which is a single list-valued contribution). Each dict becomes its own
//! `Extracted::Promises`, and they all flow into the same multimap, which accumulates per key.
//!
//! # A [`BlockValue::Null`] is a non-contribution
//!
//! It is discarded during flattening, as if it had never been deposited. An id whose contributions
//! were all `Null` therefore vanishes from the flattened map, exactly like an id that never had
//! any.
//!
//! # Pending references are not an error here
//!
//! A promise pointing at an id the map knows nothing about stays in the flattened map as a
//! [`BlockValue::Promise`], and the policy is decided downstream by
//! [`crate::core::promisable::fulfill_promises`] — non-strict means the entity disappears, strict
//! means an error. [`PromiseError::Circular`] is reserved for genuine cycles, so that a message
//! about a cycle always means there is one.
//!
//! # Determinism
//!
//! Both maps are [`BTreeMap`]s rather than hash maps. Flattening visits ids in order, so for equal
//! content the chain reported by a cycle is always the same one and error messages are reproducible
//! in tests.

use std::collections::BTreeMap;

use super::classes::value::BlockValue;
use super::promise::{Promise, PromiseError};

/// A multimap of `id -> contributions`, filled one page at a time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromiseMap {
    entries: BTreeMap<String, Vec<BlockValue>>,
}

/// A map of `id -> flattened contributions`, produced by [`PromiseMap::flatten`] and used to
/// resolve.
///
/// Invariant: no id appears in it with an empty vector — an id that left no contribution after
/// flattening simply does not enter the map. That is what lets a reference tell "resolved id" from
/// "id with nothing to give" by the presence of the key alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlatPromiseMap {
    entries: BTreeMap<String, Vec<BlockValue>>,
}

impl PromiseMap {
    pub fn new() -> Self {
        PromiseMap::default()
    }

    /// Appends a contribution for `id`, after the ones already there.
    ///
    /// Insertion order is significant: it is page order, and the later one wins when the promise is
    /// not *multiple* (see [`FlatPromiseMap::fulfill`]).
    pub fn push(&mut self, id: impl Into<String>, value: impl Into<BlockValue>) {
        self.entries.entry(id.into()).or_default().push(value.into());
    }

    /// Pours every pair produced by a pipe into the multimap.
    pub fn merge<I, K, V>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<BlockValue>,
    {
        for (k, v) in entries {
            self.push(k, v);
        }
    }

    /// The contributions recorded for `id`, in insertion order.
    pub fn get(&self, id: &str) -> Option<&[BlockValue]> {
        self.entries.get(id).map(Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &[BlockValue])> {
        self.entries.iter().map(|(k, v)| (k, v.as_slice()))
    }

    /// Follows the references between promises, leaving every id its own sequence of contributions.
    ///
    /// A contribution that is not a promise is kept as it is, containers included: the number of
    /// contributions is not lost, and one [`BlockValue::List`] contribution stays distinguishable
    /// from several scalar ones. [`BlockValue::Null`]s are the exception — they are
    /// non-contributions and are discarded here; an id left with no contributions, whether it had
    /// none or they were all `Null`, vanishes from the flattened map.
    ///
    /// A `Promise` contribution pointing at a resolved id is replaced by **all** of that id's
    /// contributions, spliced in its place and in their order: a reference *inherits* its target's
    /// contributions. Packing them into a single list would reintroduce the container/contribution
    /// ambiguity at every reference hop. Two references to the same target therefore splice its
    /// contributions twice.
    ///
    /// Pending references stay `Promise` (see the module documentation); a cycle is
    /// [`PromiseError::Circular`], carrying the whole chain from the first id visited to the
    /// repetition.
    ///
    /// The recursion does **not** descend into lists, sets and maps: a `Promise` nested inside a
    /// [`BlockValue::List`] is not resolved. Pipes deposit promises at the top level, never buried
    /// in a container.
    pub fn flatten(&self) -> Result<FlatPromiseMap, PromiseError> {
        let mut resolved = BTreeMap::new();
        let mut in_progress = Vec::new();
        for id in self.entries.keys() {
            self.resolve_id(id, &mut in_progress, &mut resolved)?;
        }
        tracing::debug!(ids = resolved.len(), "promise map flattened");
        Ok(FlatPromiseMap { entries: resolved })
    }

    /// Depth-first visit of a single id. `in_progress` is the current path, which detects cycles;
    /// `resolved` is the memoisation, so each id is flattened once however many others reference
    /// it.
    ///
    /// The "no empty vector in `resolved`" invariant holds by induction on the depth of the visit:
    /// an entry is inserted only when the accumulated vector is non-empty, and the contributions
    /// inherited from a reference come from an id already inserted, hence already non-empty.
    fn resolve_id(
        &self,
        id: &str,
        in_progress: &mut Vec<String>,
        resolved: &mut BTreeMap<String, Vec<BlockValue>>,
    ) -> Result<(), PromiseError> {
        if resolved.contains_key(id) {
            return Ok(());
        }
        if let Some(start) = in_progress.iter().position(|visited| visited == id) {
            let mut chain: Vec<String> = in_progress[start..].to_vec();
            chain.push(id.to_string());
            return Err(PromiseError::Circular { chain });
        }
        // An id we know nothing about is not an error: whoever references it keeps the promise.
        let Some(contributions) = self.entries.get(id) else {
            return Ok(());
        };

        in_progress.push(id.to_string());
        let mut flattened = Vec::with_capacity(contributions.len());
        for contribution in contributions {
            match contribution {
                // A `Null` is not a null value: it is a contribution that is not there.
                BlockValue::Null => {}
                BlockValue::Promise(promise) => {
                    self.resolve_id(promise.id(), in_progress, resolved)?;
                    match resolved.get(promise.id()) {
                        // The reference inherits its target's contributions, spliced in its place:
                        // packing them into a list would make them a single contribution.
                        Some(values) => flattened.extend(values.iter().cloned()),
                        // The referenced id does not exist, or left no contributions: the promise
                        // stays pending. Not an error, but a detail worth having when debugging.
                        None => {
                            tracing::trace!(id, target = promise.id(), "reference kept pending: target has no contributions");
                            flattened.push(contribution.clone());
                        }
                    }
                }
                other => flattened.push(other.clone()),
            }
        }
        in_progress.pop();

        if !flattened.is_empty() {
            resolved.insert(id.to_string(), flattened);
        }
        Ok(())
    }
}

impl FlatPromiseMap {
    pub fn new() -> Self {
        FlatPromiseMap::default()
    }

    /// The flattened contributions recorded for `id`, in order. Never empty: an id with no
    /// contributions does not enter the map.
    ///
    /// The contributions are unfiltered — they may still hold pending references, which only
    /// [`FlatPromiseMap::fulfill`] discards. [`BlockValue::Null`]s, in contrast, are never there:
    /// flattening has already dropped them.
    pub fn get(&self, id: &str) -> Option<&[BlockValue]> {
        self.entries.get(id).map(Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The ids with their contributions, in key order. As with [`FlatPromiseMap::get`], the
    /// contributions are unfiltered: pending references are still present.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &[BlockValue])> {
        self.entries.iter().map(|(k, v)| (k, v.as_slice()))
    }

    /// Resolves one promise against this map.
    ///
    /// - id absent from the map: [`PromiseError::Unresolved`];
    /// - contributions still `Promise` (pending references) are discarded; if no candidate remains, the
    ///   promise is `Unresolved`. `Null`s never reach here, flattening having dropped them;
    /// - *multiple* promise: a [`BlockValue::List`] of the candidates, always non-empty. A candidate
    ///   that is itself a list goes in as one element, without being unwrapped;
    /// - ordinary promise: **the last** candidate wins — the contribution of the latest page — returned
    ///   as it is, so if it is a list, that list is what comes out.
    pub fn fulfill(&self, promise: &Promise) -> Result<BlockValue, PromiseError> {
        let contributions = self.entries.get(promise.id()).ok_or_else(|| promise.unresolved())?;
        let candidates: Vec<&BlockValue> =
            contributions.iter().filter(|v| !v.is_promise()).collect();
        if candidates.len() != contributions.len() {
            tracing::trace!(
                id = promise.id(),
                pending = contributions.len() - candidates.len(),
                "pending contributions ignored while fulfilling"
            );
        }
        if promise.multiple() {
            if candidates.is_empty() {
                return Err(promise.unresolved());
            }
            return Ok(BlockValue::List(candidates.into_iter().cloned().collect()));
        }
        candidates.last().map(|v| (*v).clone()).ok_or_else(|| promise.unresolved())
    }
}

impl<K: Into<String>, V: Into<BlockValue>> FromIterator<(K, V)> for PromiseMap {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = PromiseMap::new();
        map.merge(iter);
        map
    }
}

/// Outside tests the only legitimate producer of a [`FlatPromiseMap`] is [`PromiseMap::flatten`].
///
/// There is deliberately no `FromIterator`: an `Into<BlockValue>` bound would silently let a
/// `vec![a, b]` slip in as a **single** list-valued contribution, which is exactly the ambiguity
/// this module exists to remove.
#[cfg(test)]
impl FlatPromiseMap {
    /// Builds an already-flattened map from `(id, contribution)` pairs, **accumulating** per key:
    /// repeated keys are several contributions of the same id, not overwrites.
    pub(crate) fn from_pairs<K, V, I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<BlockValue>,
    {
        let mut entries: BTreeMap<String, Vec<BlockValue>> = BTreeMap::new();
        for (id, contribution) in pairs {
            entries.entry(id.into()).or_default().push(contribution.into());
        }
        FlatPromiseMap { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn promise(raw: &str) -> BlockValue {
        BlockValue::Promise(Promise::new(raw))
    }

    mod multimap {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn accumulates_contributions_in_order() {
            let mut map = PromiseMap::new();
            map.push("fund", 1_i64);
            map.push("fund", 2_i64);
            assert_eq!(map.get("fund"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
        }

        #[test]
        fn merge_pours_multiple_pairs_at_once() {
            let mut map = PromiseMap::new();
            map.merge([("a", 1_i64), ("b", 2_i64)]);
            map.merge([("a", 3_i64)]);
            assert_eq!(map.get("a"), Some(&[BlockValue::Int(1), BlockValue::Int(3)][..]));
            assert_eq!(map.get("b"), Some(&[BlockValue::Int(2)][..]));
            assert_eq!(map.len(), 2);
        }

        #[test]
        fn a_new_map_is_empty() {
            let map = PromiseMap::new();
            assert!(map.is_empty());
            assert_eq!(map.len(), 0);
            assert_eq!(map.get("assente"), None);
        }

        #[test]
        fn is_built_from_an_iterator() {
            let map: PromiseMap = [("a", 1_i64), ("a", 2_i64), ("b", 3_i64)].into_iter().collect();
            assert_eq!(map.get("a"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
            assert_eq!(map.len(), 2);
        }

        #[test]
        fn iterates_in_key_order() {
            let map: PromiseMap = [("z", 1_i64), ("a", 2_i64), ("m", 3_i64)].into_iter().collect();
            let keys: Vec<&str> = map.iter().map(|(k, _)| k.as_str()).collect();
            assert_eq!(keys, vec!["a", "m", "z"]);
        }
    }

    mod flattening {
        use super::*;
        use pretty_assertions::assert_eq;

        /// There is no scalarisation: one contribution stays one contribution, in a vector of
        /// length one.
        #[test]
        fn a_single_contribution_stays_alone() {
            let map: PromiseMap = [("x", 42_i64)].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("x"), Some(&[BlockValue::Int(42)][..]));
        }

        /// Several contributions stay several: they are not packed into a [`BlockValue::List`],
        /// which would be indistinguishable from a single list-valued contribution.
        #[test]
        fn multiple_contributions_stay_separate() {
            let map: PromiseMap = [("x", 1_i64), ("x", 2_i64), ("x", 3_i64)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.get("x"),
                Some(&[BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(3)][..])
            );
        }

        #[test]
        fn an_empty_map_flattens_to_an_empty_map() {
            assert!(PromiseMap::new().flatten().unwrap().is_empty());
        }

        #[test]
        fn an_id_without_contributions_disappears() {
            let mut map = PromiseMap::new();
            map.entries.insert("vuoto".into(), Vec::new());
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("vuoto"), None);
            assert!(flat.is_empty());
        }

        /// A [`BlockValue::Null`] contribution is a *non*-contribution, discarded during
        /// flattening. An id of nothing but `Null`s therefore yields an empty vector, and an empty
        /// vector does not enter the flattened map: the id vanishes exactly as if it had never had
        /// contributions.
        #[test]
        fn a_null_only_id_disappears_from_the_flat_map() {
            let map: PromiseMap =
                [("solo-null", BlockValue::Null), ("solo-null", BlockValue::Null)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("solo-null"), None);
            assert!(flat.is_empty());
        }

        /// A `Null` among other contributions does not survive and so cannot become the winning
        /// value: it disappears, the others do not.
        #[test]
        fn a_null_contribution_disappears_from_among_the_others() {
            let map: PromiseMap =
                [("x", BlockValue::Int(1)), ("x", BlockValue::Null), ("x", BlockValue::Int(2))]
                    .into_iter()
                    .collect();
            assert_eq!(
                map.flatten().unwrap().get("x"),
                Some(&[BlockValue::Int(1), BlockValue::Int(2)][..])
            );
        }

        #[test]
        fn resolves_a_simple_reference() {
            let map: PromiseMap =
                [("source", promise("target")), ("target", BlockValue::Int(99))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("source"), Some(&[BlockValue::Int(99)][..]));
            assert_eq!(flat.get("target"), Some(&[BlockValue::Int(99)][..]));
        }

        #[test]
        fn resolves_a_chain_of_references() {
            let map: PromiseMap =
                [("a", promise("b")), ("b", promise("c")), ("c", BlockValue::Int(7))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("a"), Some(&[BlockValue::Int(7)][..]));
            assert_eq!(flat.get("b"), Some(&[BlockValue::Int(7)][..]));
        }

        /// A reference **inherits its target's contributions**, in order, rather than packing them
        /// into a list — otherwise the container/contribution ambiguity would come back in through
        /// the reference: a target with two scalar contributions would look, to whoever references
        /// it, exactly like a target with one list-valued contribution.
        #[test]
        fn a_reference_receives_the_contributions_of_its_target() {
            let map: PromiseMap =
                [("src", promise("t")), ("t", BlockValue::Int(1)), ("t", BlockValue::Int(2))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            let expected = [BlockValue::Int(1), BlockValue::Int(2)];
            assert_eq!(flat.get("src"), Some(&expected[..]));
            assert_eq!(flat.get("t"), Some(&expected[..]));
        }

        /// An id with two contributions that are both promises. One visit, and the result is the
        /// two referenced values in insertion order.
        #[test]
        fn an_id_with_two_promised_contributions_becomes_the_values_of_both() {
            let map: PromiseMap = [
                ("x", promise("a")),
                ("x", promise("b")),
                ("a", BlockValue::Int(1)),
                ("b", BlockValue::Int(2)),
            ]
            .into_iter()
            .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
        }

        #[test]
        fn unpromised_contributions_stay_alongside_resolved_ones() {
            let map: PromiseMap =
                [("x", BlockValue::from("fisso")), ("x", promise("a")), ("a", BlockValue::Int(5))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[BlockValue::from("fisso"), BlockValue::Int(5)][..]));
        }

        /// Contributions inherited from a reference are spliced **in the promise's place**, not
        /// appended: the insertion order of the id's own contributions is preserved around them.
        #[test]
        fn a_reference_splices_the_contributions_of_its_target_among_its_own() {
            let map: PromiseMap = [
                ("x", promise("a")),
                ("x", BlockValue::Int(5)),
                ("a", BlockValue::Int(1)),
                ("a", BlockValue::Int(2)),
            ]
            .into_iter()
            .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.get("x"),
                Some(&[BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(5)][..])
            );
        }

        /// Two references to the same multi-contribution target splice its contributions twice: the
        /// cardinality of `x` is the sum, not the number of references. It is the widest observable
        /// consequence of splicing, and it shows on the *multiple* promise too, which expands into
        /// four copies here.
        #[test]
        fn two_references_to_the_same_multi_contribution_target_splice_twice() {
            let map: PromiseMap = [
                ("x", promise("a")),
                ("x", promise("a")),
                ("a", BlockValue::Int(1)),
                ("a", BlockValue::Int(2)),
            ]
            .into_iter()
            .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.get("x"),
                Some(
                    &[BlockValue::Int(1), BlockValue::Int(2), BlockValue::Int(1), BlockValue::Int(2)][..]
                )
            );
            assert_eq!(
                flat.fulfill(&Promise::new("x[]")).unwrap(),
                BlockValue::List(vec![
                    BlockValue::Int(1),
                    BlockValue::Int(2),
                    BlockValue::Int(1),
                    BlockValue::Int(2),
                ])
            );
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), BlockValue::Int(2));
        }

        #[test]
        fn the_promise_flags_do_not_affect_flattening() {
            // Strict and multiple matter when an entity is resolved, not here: `flatten` replaces
            // the promise with the referenced id's contributions either way.
            let map: PromiseMap =
                [("src", promise("t[]!")), ("t", BlockValue::Int(1))].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("src"), Some(&[BlockValue::Int(1)][..]));
        }

        #[test]
        fn does_not_descend_into_containers() {
            let nested = BlockValue::List(vec![promise("t")]);
            let map: PromiseMap =
                [("src", nested.clone()), ("t", BlockValue::Int(1))].into_iter().collect();
            assert_eq!(map.flatten().unwrap().get("src"), Some(&[nested][..]));
        }

        /// The invariant everything else rests on: in the flattened map no id has an empty
        /// contribution vector. That is what lets a reference tell "resolved id" from "id with
        /// nothing to give" by the presence of the key alone.
        #[test]
        fn the_flat_map_never_holds_an_empty_contribution_list() {
            let mut map = PromiseMap::new();
            map.push("scalare", 1_i64);
            map.push("multi", 1_i64);
            map.push("multi", 2_i64);
            map.push("riferimento", Promise::new("multi"));
            map.push("pendente", Promise::new("nowhere"));
            map.push("contenitore-vuoto", BlockValue::List(Vec::new()));
            map.push("solo-null", BlockValue::Null);
            map.entries.insert("senza-contributi".into(), Vec::new());

            let flat = map.flatten().unwrap();
            for (id, contributions) in flat.iter() {
                assert!(!contributions.is_empty(), "l'id `{id}` ha un vettore di contributi vuoto");
            }
            assert_eq!(flat.get("solo-null"), None);
            assert_eq!(flat.get("senza-contributi"), None);
            // An empty container, in contrast, is a real value and stays.
            assert_eq!(flat.get("contenitore-vuoto"), Some(&[BlockValue::List(Vec::new())][..]));
        }
    }

    /// A contribution that *is* a container is not the same thing as N contributions.
    ///
    /// Exercised across both flattening and resolution, because the two are the places where the
    /// distinction can be lost.
    mod container_valued_contributions {
        use super::*;
        use pretty_assertions::{assert_eq, assert_ne};
        use std::collections::BTreeSet;

        fn one_list_contribution() -> FlatPromiseMap {
            let map: PromiseMap =
                [("x", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]))]
                    .into_iter()
                    .collect();
            map.flatten().unwrap()
        }

        fn two_scalar_contributions() -> FlatPromiseMap {
            let map: PromiseMap = [("x", BlockValue::Int(1)), ("x", BlockValue::Int(2))].into_iter().collect();
            map.flatten().unwrap()
        }

        /// The two maps must stay distinguishable after flattening: once they are not, no later
        /// stage can separate them again.
        #[test]
        fn one_list_contribution_is_not_two_scalar_contributions() {
            let container = one_list_contribution();
            let scalars = two_scalar_contributions();
            assert_eq!(
                container.get("x"),
                Some(&[BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])][..])
            );
            assert_eq!(scalars.get("x"), Some(&[BlockValue::Int(1), BlockValue::Int(2)][..]));
            assert_ne!(container, scalars);
        }

        #[test]
        fn a_normal_promise_on_a_single_list_contribution_returns_the_whole_list() {
            assert_eq!(
                one_list_contribution().fulfill(&Promise::new("x")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        #[test]
        fn a_normal_promise_on_two_scalar_contributions_still_returns_the_last() {
            assert_eq!(
                two_scalar_contributions().fulfill(&Promise::new("x")).unwrap(),
                BlockValue::Int(2)
            );
        }

        #[test]
        fn a_multiple_promise_on_a_single_list_contribution_wraps_it() {
            assert_eq!(
                one_list_contribution().fulfill(&Promise::new("x[]")).unwrap(),
                BlockValue::List(vec![BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])])
            );
        }

        #[test]
        fn a_multiple_promise_on_two_scalar_contributions_returns_both() {
            assert_eq!(
                two_scalar_contributions().fulfill(&Promise::new("x[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        /// To be read against `resolution::an_id_with_zero_contributions_is_unresolvable`: "a
        /// contribution that is an empty list" and "no contribution" are two different things.
        #[test]
        fn an_empty_list_is_a_legitimate_value() {
            let map: PromiseMap = [("x", BlockValue::List(Vec::new()))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[BlockValue::List(Vec::new())][..]));
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), BlockValue::List(Vec::new()));
            assert_eq!(
                flat.fulfill(&Promise::new("x[]")).unwrap(),
                BlockValue::List(vec![BlockValue::List(Vec::new())])
            );
        }

        #[test]
        fn a_nested_list_contribution_survives_intact() {
            let nested = BlockValue::List(vec![
                BlockValue::List(vec![BlockValue::Int(1)]),
                BlockValue::List(vec![BlockValue::Int(2)]),
            ]);
            let map: PromiseMap = [("x", nested.clone())].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[nested.clone()][..]));
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), nested);
        }

        /// The behaviour must not be written for the `List` variant alone: a `Set` is a container
        /// like the others, and is pinned here so it does not start being unwrapped.
        #[test]
        fn a_set_contribution_survives_intact() {
            let set = BlockValue::Set(BTreeSet::from([BlockValue::Int(1), BlockValue::Int(2)]));
            let map: PromiseMap = [("x", set.clone())].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[set.clone()][..]));
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), set.clone());
            assert_eq!(flat.fulfill(&Promise::new("x[]")).unwrap(), BlockValue::List(vec![set]));
        }

        #[test]
        fn a_map_contribution_survives_intact() {
            let inner = BlockValue::Map(BTreeMap::from([
                ("a".to_string(), BlockValue::Int(1)),
                ("b".to_string(), BlockValue::Int(2)),
            ]));
            let map: PromiseMap = [("x", inner.clone())].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("x"), Some(&[inner.clone()][..]));
            assert_eq!(flat.fulfill(&Promise::new("x")).unwrap(), inner.clone());
            assert_eq!(flat.fulfill(&Promise::new("x[]")).unwrap(), BlockValue::List(vec![inner]));
        }

        /// The same distinction seen through a reference: `src` inherits **one** contribution,
        /// which is a list, not two.
        #[test]
        fn a_reference_to_an_id_whose_only_contribution_is_a_list_yields_the_list() {
            let list = BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)]);
            let map: PromiseMap = [("src", promise("t")), ("t", list.clone())].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("src"), Some(&[list.clone()][..]));
            assert_eq!(flat.get("t"), Some(&[list.clone()][..]));
            assert_eq!(flat.fulfill(&Promise::new("src")).unwrap(), list);
        }

        /// Round trip: flattening, re-inserting into a multimap and flattening again does not
        /// unwrap the container.
        #[test]
        fn flattening_a_list_contribution_twice_does_not_unwrap_it() {
            let map: PromiseMap = [
                ("contenitore", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])),
                ("separati", BlockValue::Int(1)),
                ("separati", BlockValue::Int(2)),
            ]
            .into_iter()
            .collect();
            let once = map.flatten().unwrap();
            let reinserted: PromiseMap = once
                .iter()
                .flat_map(|(id, contributions)| {
                    contributions.iter().map(move |v| (id.clone(), v.clone()))
                })
                .collect();
            assert_eq!(reinserted.flatten().unwrap(), once);
            assert_ne!(once.get("contenitore"), once.get("separati"));
        }
    }

    /// A reference that leads nowhere is not a flattening error.
    mod pending_references {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_unknown_id_leaves_the_promise_in_place() {
            let map: PromiseMap = [("source", promise("nowhere"))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("source"), Some(&[promise("nowhere")][..]));
        }

        #[test]
        fn a_reference_to_an_id_without_contributions_leaves_the_promise() {
            let mut map = PromiseMap::new();
            map.push("source", Promise::new("vuoto"));
            map.entries.insert("vuoto".into(), Vec::new());
            assert_eq!(map.flatten().unwrap().get("source"), Some(&[promise("vuoto")][..]));
        }

        /// `Null`s are already gone when splicing looks at the target, so an id of nothing but
        /// `Null`s is indistinguishable from an id with no contributions: the promise stays pending
        /// and inherits no `Null`.
        #[test]
        fn a_reference_to_a_null_only_target_stays_pending() {
            let map: PromiseMap =
                [("source", promise("t")), ("t", BlockValue::Null)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("source"), Some(&[promise("t")][..]));
            assert_eq!(flat.get("t"), None);
        }

        #[test]
        fn a_chain_that_ends_in_nothing_stops_on_the_pending_promise() {
            let map: PromiseMap = [("a", promise("b")), ("b", promise("nowhere"))].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("b"), Some(&[promise("nowhere")][..]));
            assert_eq!(flat.get("a"), Some(&[promise("nowhere")][..]));
        }

        #[test]
        fn a_pending_reference_does_not_prevent_others_from_resolving() {
            let map: PromiseMap =
                [("a", promise("nowhere")), ("b", promise("c")), ("c", BlockValue::Int(3))]
                    .into_iter()
                    .collect();
            let flat = map.flatten().unwrap();
            assert_eq!(flat.get("a"), Some(&[promise("nowhere")][..]));
            assert_eq!(flat.get("b"), Some(&[BlockValue::Int(3)][..]));
        }
    }

    mod cycles {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_self_reference_is_a_cycle() {
            let map: PromiseMap = [("a", promise("a"))].into_iter().collect();
            assert_eq!(
                map.flatten().unwrap_err(),
                PromiseError::Circular { chain: vec!["a".into(), "a".into()] }
            );
        }

        #[test]
        fn two_ids_that_reference_each_other_are_a_cycle() {
            let map: PromiseMap = [("a", promise("b")), ("b", promise("a"))].into_iter().collect();
            assert_eq!(
                map.flatten().unwrap_err(),
                PromiseError::Circular { chain: vec!["a".into(), "b".into(), "a".into()] }
            );
        }

        #[test]
        fn the_reported_chain_covers_the_whole_cycle() {
            let map: PromiseMap =
                [("a", promise("b")), ("b", promise("c")), ("c", promise("a"))].into_iter().collect();
            let err = map.flatten().unwrap_err();
            assert_eq!(
                err,
                PromiseError::Circular { chain: vec!["a".into(), "b".into(), "c".into(), "a".into()] }
            );
            assert_eq!(err.to_string(), "circular promise chain: a -> b -> c -> a");
        }

        /// A path that *enters* a cycle without being part of it: the reported chain starts at the
        /// first id visited, not at the first id of the cycle, so the message also shows how it got
        /// there.
        #[test]
        fn a_path_that_enters_a_cycle_also_reports_the_entry_point() {
            let map: PromiseMap =
                [("ingresso", promise("a")), ("a", promise("b")), ("b", promise("a"))]
                    .into_iter()
                    .collect();
            assert_eq!(
                map.flatten().unwrap_err(),
                PromiseError::Circular { chain: vec!["a".into(), "b".into(), "a".into()] }
            );
        }

        /// Flattening visits ids in order, so the reported chain does not depend on the order the
        /// contributions were inserted in: error messages are reproducible.
        #[test]
        fn the_reported_chain_is_deterministic() {
            let forward: PromiseMap =
                [("a", promise("b")), ("b", promise("c")), ("c", promise("a"))].into_iter().collect();
            let reversed: PromiseMap =
                [("c", promise("a")), ("b", promise("c")), ("a", promise("b"))].into_iter().collect();
            assert_eq!(forward.flatten().unwrap_err(), reversed.flatten().unwrap_err());
        }

        #[test]
        fn a_cycle_fails_the_entire_flattening() {
            let map: PromiseMap =
                [("sano", BlockValue::Int(1)), ("a", promise("b")), ("b", promise("a"))]
                    .into_iter()
                    .collect();
            assert!(map.flatten().is_err());
        }
    }

    mod resolution {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Repeated keys mean several contributions for the same id: `from_pairs` accumulates
        /// rather than overwriting.
        fn flat(pairs: Vec<(&str, BlockValue)>) -> FlatPromiseMap {
            FlatPromiseMap::from_pairs(pairs)
        }

        #[test]
        fn a_scalar_value_resolves_to_itself() {
            let map = flat(vec![("fund", BlockValue::from("Acme"))]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::from("Acme"));
        }

        /// The later one wins: that is page order.
        #[test]
        fn on_several_contributions_the_last_one_wins() {
            let map = flat(vec![
                ("fund", BlockValue::Int(1)),
                ("fund", BlockValue::Int(2)),
                ("fund", BlockValue::Int(3)),
            ]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::Int(3));
        }

        #[test]
        fn a_multiple_promise_always_gets_a_list() {
            let single = flat(vec![("fund", BlockValue::Int(1))]);
            assert_eq!(
                single.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1)])
            );

            let several = flat(vec![("fund", BlockValue::Int(1)), ("fund", BlockValue::Int(2))]);
            assert_eq!(
                several.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        #[test]
        fn a_missing_id_is_unresolvable() {
            let map = flat(vec![("altro", BlockValue::Int(1))]);
            assert_eq!(
                map.fulfill(&Promise::new("fund")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
        }

        /// A recorded `Null` counts as an *absent* value rather than a null one.
        ///
        /// The internal route is that `flatten` drops it, so the id does not appear in the
        /// flattened map at all. The test therefore goes through `flatten` rather than a hand-built
        /// [`FlatPromiseMap`], which is the only way a `Null` can really turn up.
        #[test]
        fn a_null_value_is_unresolvable() {
            let map: PromiseMap = [("fund", BlockValue::Null)].into_iter().collect();
            let flat = map.flatten().unwrap();
            assert_eq!(
                flat.fulfill(&Promise::new("fund")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
            assert!(flat.fulfill(&Promise::new("fund[]")).is_err());
        }

        /// The other half of the pending-reference policy: a promise that survived flattening
        /// resolves nothing, and this is where it becomes an error.
        #[test]
        fn a_value_still_a_promise_is_unresolvable() {
            let map = flat(vec![("fund", promise("nowhere"))]);
            assert_eq!(
                map.fulfill(&Promise::new("fund")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
        }

        #[test]
        fn pending_promise_contributions_are_discarded() {
            let map = flat(vec![
                ("fund", BlockValue::Int(1)),
                ("fund", promise("nowhere")),
                ("fund", BlockValue::Int(2)),
            ]);
            assert_eq!(map.fulfill(&Promise::new("fund")).unwrap(), BlockValue::Int(2));
            assert_eq!(
                map.fulfill(&Promise::new("fund[]")).unwrap(),
                BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])
            );
        }

        #[test]
        fn an_id_with_only_pending_contributions_is_unresolvable() {
            let map = flat(vec![("fund", promise("a")), ("fund", promise("b"))]);
            assert!(map.fulfill(&Promise::new("fund")).is_err());
            assert!(map.fulfill(&Promise::new("fund[]")).is_err());
        }

        /// An id present with zero contributions cannot exist in a map produced by `flatten` (see
        /// `flattening::the_flat_map_never_holds_an_empty_contribution_list`); it is built by hand,
        /// from inside the module, only to pin down what `fulfill` would do if it ever arrived. Not
        /// to be confused with
        /// `container_valued_contributions::an_empty_list_is_a_legitimate_value`.
        #[test]
        fn an_id_with_zero_contributions_is_unresolvable() {
            let mut map = FlatPromiseMap::new();
            map.entries.insert("fund".into(), Vec::new());
            assert!(map.fulfill(&Promise::new("fund")).is_err());
            assert!(map.fulfill(&Promise::new("fund[]")).is_err());
        }

        /// `strict` does not change *whether* a promise resolves, only what happens to whoever
        /// contains it when it does not — a decision that belongs to `promisable`.
        #[test]
        fn strict_does_not_change_the_resolution_outcome() {
            let map = flat(vec![("fund", BlockValue::Int(1))]);
            assert_eq!(map.fulfill(&Promise::new("fund!")).unwrap(), BlockValue::Int(1));
            let empty = FlatPromiseMap::new();
            assert_eq!(
                empty.fulfill(&Promise::new("fund!")).unwrap_err(),
                empty.fulfill(&Promise::new("fund")).unwrap_err()
            );
        }

        #[test]
        fn the_error_names_the_id_without_suffixes() {
            let empty = FlatPromiseMap::new();
            assert_eq!(
                empty.fulfill(&Promise::new("fund[]!")).unwrap_err(),
                PromiseError::Unresolved { id: "fund".into() }
            );
        }
    }

    /// Properties that must hold over generated input, not only over hand-written cases.
    mod invariants {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Flattening twice changes nothing: the flattened map, re-inserted into a multimap one
        /// contribution at a time, flattens into itself.
        ///
        /// Idempotence holds with splicing too: a contribution left as `Promise` points by
        /// construction at an id **absent** from the flattened map, so on the second pass it stays
        /// pending instead of splicing something new. The fixture includes a contribution that is
        /// itself a `List`, so the property covers the container case as well.
        #[test]
        fn flattening_is_idempotent() {
            let map: PromiseMap = [
                ("a", promise("b")),
                ("b", BlockValue::Int(1)),
                ("b", BlockValue::Int(2)),
                ("c", BlockValue::from("x")),
                ("d", promise("nowhere")),
                ("e", BlockValue::List(vec![BlockValue::Int(1), BlockValue::Int(2)])),
            ]
            .into_iter()
            .collect();
            let once = map.flatten().unwrap();
            let reinserted: PromiseMap = once
                .iter()
                .flat_map(|(id, contributions)| {
                    contributions.iter().map(move |v| (id.clone(), v.clone()))
                })
                .collect();
            assert_eq!(reinserted.flatten().unwrap(), once);
        }

        /// A long linear chain resolves entirely to the same final value, and every id is left with
        /// **one** contribution: each link references a target that has one, so splicing lengthens
        /// nothing. (With repeated references the length can instead double at every link — see
        /// `flattening::two_references_to_the_same_multi_contribution_target_splice_twice`.) The
        /// visit stays linear thanks to memoisation.
        #[test]
        fn a_long_chain_resolves_entirely_to_the_final_value() {
            const LENGTH: usize = 500;
            let mut map = PromiseMap::new();
            for i in 0..LENGTH {
                map.push(format!("id{i}"), Promise::new(&format!("id{}", i + 1)));
            }
            map.push(format!("id{LENGTH}"), 42_i64);
            let flat = map.flatten().unwrap();
            for i in 0..=LENGTH {
                assert_eq!(flat.get(&format!("id{i}")), Some(&[BlockValue::Int(42)][..]), "id{i}");
            }
        }

        /// Many ids all pointing at the same target: no cycle, all resolved.
        #[test]
        fn many_references_to_the_same_id_all_resolve() {
            let mut map = PromiseMap::new();
            map.push("target", 7_i64);
            for i in 0..200 {
                map.push(format!("src{i}"), Promise::new("target"));
            }
            let flat = map.flatten().unwrap();
            for i in 0..200 {
                assert_eq!(flat.get(&format!("src{i}")), Some(&[BlockValue::Int(7)][..]));
            }
        }

        /// A long cycle is still detected, and the reported chain is exactly the length of the
        /// cycle plus one, the closing repetition.
        #[test]
        fn a_long_cycle_is_detected() {
            const LENGTH: usize = 300;
            let mut map = PromiseMap::new();
            for i in 0..LENGTH {
                map.push(format!("id{i:03}"), Promise::new(&format!("id{:03}", (i + 1) % LENGTH)));
            }
            match map.flatten().unwrap_err() {
                PromiseError::Circular { chain } => assert_eq!(chain.len(), LENGTH + 1),
                other => panic!("atteso un ciclo, trovato {other:?}"),
            }
        }

        /// If no contribution is a promise, flattening is not a reduction but the identity: every
        /// id keeps **all** its contributions, in order, element by element, containers included.
        #[test]
        fn without_promises_flattening_preserves_every_contribution() {
            for n_contributions in 1..8_usize {
                let mut map = PromiseMap::new();
                for i in 0..n_contributions {
                    map.push("scalari", i as i64);
                    map.push("contenitori", BlockValue::List(vec![BlockValue::Int(i as i64)]));
                    map.push("misti", i as i64);
                    map.push("misti", BlockValue::List(vec![BlockValue::Int(i as i64)]));
                }
                let flat = map.flatten().unwrap();
                assert_eq!(flat.len(), map.len(), "con {n_contributions} contributi");
                for (id, contributions) in map.iter() {
                    assert_eq!(
                        flat.get(id),
                        Some(contributions),
                        "id `{id}` con {n_contributions} contributi"
                    );
                }
            }
        }

        /// At volume: a list-valued contribution at a known position — not the last — is neither
        /// unwrapped nor moved. Covers order, non-unwrapping and the absence of accidental
        /// flattening together.
        #[test]
        fn a_long_list_of_contributions_keeps_a_list_valued_one_in_place() {
            const TOTAL: usize = 200;
            const CONTAINER_AT: usize = 137;
            let container = BlockValue::List(vec![BlockValue::Int(-1), BlockValue::Int(-2)]);

            let mut map = PromiseMap::new();
            for i in 0..TOTAL {
                if i == CONTAINER_AT {
                    map.push("x", container.clone());
                } else {
                    map.push("x", i as i64);
                }
            }

            let flat = map.flatten().unwrap();
            let contributions = flat.get("x").expect("l'id ha contributi");
            assert_eq!(contributions.len(), TOTAL);
            assert_eq!(contributions[CONTAINER_AT], container);
            assert_eq!(contributions[0], BlockValue::Int(0));

            let expanded = flat.fulfill(&Promise::new("x[]")).unwrap();
            match expanded {
                BlockValue::List(values) => {
                    assert_eq!(values.len(), TOTAL);
                    assert_eq!(values[CONTAINER_AT], container);
                    assert_eq!(values[TOTAL - 1], BlockValue::Int((TOTAL - 1) as i64));
                }
                other => panic!("attesa una lista, trovato {other:?}"),
            }

            assert_eq!(
                flat.fulfill(&Promise::new("x")).unwrap(),
                BlockValue::Int((TOTAL - 1) as i64)
            );
        }
    }
}
