//! Sets over disjoint atoms: [`DisjointAtomsSet`] and [`AtomAlgebra`].
//!
//! The third representation of the same algebra: instead of a tree of operations, the set holds
//! directly the disjoint atoms composing it, always already simplified. [`AtomAlgebra`] and
//! [`AtomOperations`] are the contract a concrete atom type must meet — comparison through
//! [`Overlappable`], plus the operations producing one or more atoms when two overlap without one
//! containing the other.
//!
//! Unlike the two tree representations, this one implements [`ComparableSet`]: two of these sets
//! can always be compared, because equal sets have equal canonical forms. That is what it buys, and
//! the cost is that every operation does the simplification work eagerly.

use super::{ComparableSet, Container, Overlappable, SetAlgebra, SetRelation};
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{BitAnd, BitOr, Div};

/// The shape an atom-level operation can produce: it either stays a single
/// atom, or splits into up to four. Splitting further than four is never
/// needed by any operation this module defines.
#[derive(Debug)]
pub enum CompoundAtomOperationRes<T> {
    One(T),
    Two(T, T),
    Three(T, T, T),
    Four(T, T, T, T),
}

/// The shape a whole-atom operation (`union`/`intersect`/`subtract`) can
/// produce, folding in the four `SetRelation` cases that need no further
/// computation (`EmptySet`, `Lhs`, `Rhs`, `Both`) alongside the one that does
/// (`Compound`, delegating to `AtomOperations`).
#[derive(Debug)]
pub enum AtomOperationRes<T> {
    EmptySet,
    Lhs,
    Rhs,
    Both,
    Compound(CompoundAtomOperationRes<T>),
}

/// The operations a concrete atom type must provide for the `Overlapping`
/// case of each of `union`/`intersect`/`subtract` — the only case that needs
/// real computation, since every other `SetRelation` fully determines the
/// result without inspecting the atoms' contents.
///
/// `union_overlapping`'s default implementation only accepts a
/// `subtract_overlapping` that stays single-atom (`Compound(One(_))`):
/// anything wider is `unreachable!()`. This mirrors `DisjointAtomsSet::
/// atom_union`'s own precondition (see its doc comment) — an `AtomOperations`
/// impl meant to be combined with `|` must keep its subtraction operations
/// single-atom, even though `intersect_overlapping` is free to split into
/// more than one atom.
pub trait AtomOperations: Sized + Clone {
    type SubtractSubsetRes: Into<CompoundAtomOperationRes<Self>>;
    type SubtractOverlappingRes: Into<CompoundAtomOperationRes<Self>>;
    type IntersectOverlappingRes: Into<CompoundAtomOperationRes<Self>>;
    fn subtract_subset(&self, other: &Self) -> Self::SubtractSubsetRes;
    fn subtract_overlapping(&self, other: &Self) -> Self::SubtractOverlappingRes;
    fn intersect_overlapping(&self, other: &Self) -> Self::IntersectOverlappingRes;
    fn union_overlapping(&self, other: &Self) -> CompoundAtomOperationRes<Self> {
        use CompoundAtomOperationRes::*;
        match self.subtract_overlapping(other).into() {
            One(a) => Two(a, (*other).clone()),
            Two(a, b) => Three(a, b, (*other).clone()),
            Three(a, b, c) => Four(a, b, c, (*other).clone()),
            _ => unreachable!("Default implementation of union doesn't support that set subtraction"),
        }
    }
}

/// Combines `Overlappable` (to classify the relation) with `AtomOperations`
/// (to compute the `Overlapping` case) into the three whole-atom operations
/// `DisjointAtomsSet` builds on.
pub trait AtomAlgebra: Overlappable<Self> + AtomOperations {
    fn union(&self, other: &Self) -> AtomOperationRes<Self> {
        use AtomOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Superset => Lhs,
            Subset => Rhs,
            Overlapping => Compound(self.union_overlapping(other)),
            Disjoint => Both,
        }
    }
    fn intersect(&self, other: &Self) -> AtomOperationRes<Self> {
        use AtomOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Subset => Lhs,
            Superset => Rhs,
            Overlapping => Compound(self.intersect_overlapping(other).into()),
            Disjoint => EmptySet,
        }
    }
    fn subtract(&self, other: &Self) -> AtomOperationRes<Self> {
        use AtomOperationRes::*;
        use SetRelation::*;
        match self.set_relation(other) {
            Equal | Subset => EmptySet,
            Superset => Compound(self.subtract_subset(other).into()),
            Overlapping => Compound(self.subtract_overlapping(other).into()),
            Disjoint => Lhs,
        }
    }
}

/// A set represented directly as its (always already simplified) partition
/// into pairwise-disjoint atoms.
#[derive(Clone, Debug)]
pub struct DisjointAtomsSet<A, E>(HashSet<A>)
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized;

impl<A, E> DisjointAtomsSet<A, E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized,
{
    pub fn atoms_ref(&self) -> HashSet<&A> {
        let mut atoms = HashSet::new();
        for a in self.0.iter() {
            atoms.insert(a);
        }
        atoms
    }
    pub fn atoms(&self) -> &HashSet<A> {
        &self.0
    }
    pub fn from_atom(atom: A) -> Self {
        let mut atoms = HashSet::new();
        atoms.insert(atom);
        Self(atoms)
    }
    pub fn empty() -> Self {
        Self(HashSet::new())
    }

    /// Folds a single incoming atom into the partition via union: every
    /// existing atom is shrunk (or dropped) by what it shares with `other`,
    /// then `other` itself is inserted whole. Only `Compound(One(_))` is a
    /// valid non-`Lhs`/`EmptySet` subtraction result here — see
    /// `AtomOperations`'s doc comment for why a wider split is
    /// `unreachable!()`.
    fn atom_union(&self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut new_set = HashSet::new();
        for atm in &self.0 {
            match atm.subtract(&other) {
                EmptySet => (),
                Lhs => {
                    new_set.insert(atm.clone());
                }
                Compound(One(a)) => {
                    new_set.insert(a);
                }
                _ => unreachable!("Invalid operation result in DisjointAtomSet atom_union"),
            };
        }
        new_set.insert(other);
        Self(new_set)
    }

    fn atom_intersection(&self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut atoms: Vec<A> = self.0.iter().cloned().collect();
        let mut i = 0;
        while i < atoms.len() {
            match atoms[i].intersect(&other) {
                EmptySet => {
                    atoms.remove(i);
                }
                Lhs => i += 1,
                Rhs => {
                    atoms[i] = other.clone();
                    i += 1;
                }
                Compound(One(a)) => {
                    atoms[i] = a;
                    i += 1;
                }
                Compound(Two(a, b)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    i += 2;
                }
                Compound(Three(a, b, c)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    atoms.insert(i + 2, c);
                    i += 3;
                }
                Compound(Four(a, b, c, d)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    atoms.insert(i + 2, c);
                    atoms.insert(i + 3, d);
                    i += 4;
                }
                _ => unreachable!("Invalid operation result in DisjointAtomSet atom_intersection"),
            }
        }
        let mut s = HashSet::with_capacity(atoms.len());
        for a in atoms {
            s.insert(a);
        }
        Self(s)
    }

    fn atom_subtraction(&self, other: A) -> Self {
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;
        let mut atoms: Vec<A> = self.0.iter().cloned().collect();
        let mut i = 0;
        while i < atoms.len() {
            match atoms[i].subtract(&other) {
                EmptySet => {
                    atoms.remove(i);
                }
                Lhs => i += 1,
                Compound(One(a)) => {
                    atoms[i] = a;
                    i += 1;
                }
                Compound(Two(a, b)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    i += 2;
                }
                Compound(Three(a, b, c)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    atoms.insert(i + 2, c);
                    i += 3;
                }
                Compound(Four(a, b, c, d)) => {
                    atoms[i] = a;
                    atoms.insert(i + 1, b);
                    atoms.insert(i + 2, c);
                    atoms.insert(i + 3, d);
                    i += 4;
                }
                _ => unreachable!("Invalid operation result in DisjointAtomSet atom_subtraction"),
            }
        }
        let mut s = HashSet::with_capacity(atoms.len());
        for a in atoms {
            s.insert(a);
        }
        Self(s)
    }

    fn union(&self, other: &Self) -> Self {
        let mut res = Self(self.0.clone());
        for o in &other.0 {
            res = res.atom_union(o.clone());
        }
        res
    }
    fn intersect(&self, other: &Self) -> Self {
        let mut res = Self(HashSet::new());
        for o in &other.0 {
            res = res | self.atom_intersection(o.clone());
        }
        res
    }
    fn subtract(&self, other: &Self) -> Self {
        let mut res = Self(self.0.clone());
        for o in &other.0 {
            res = res.atom_subtraction(o.clone());
        }
        res
    }
}

impl<A, E> BitOr<Self> for DisjointAtomsSet<A, E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized,
{
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        self.union(&other)
    }
}

impl<A, E> BitAnd<Self> for DisjointAtomsSet<A, E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized,
{
    type Output = Self;
    fn bitand(self, other: Self) -> Self {
        self.intersect(&other)
    }
}

impl<A, E> Div<Self> for DisjointAtomsSet<A, E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized,
{
    type Output = Self;
    fn div(self, other: Self) -> Self {
        self.subtract(&other)
    }
}

impl<A, E> Container for DisjointAtomsSet<A, E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self, e: &Self::Elem) -> bool {
        for a in &self.0 {
            if a.contains(e) {
                return true;
            }
        }
        false
    }
}

impl<A, E> SetAlgebra for DisjointAtomsSet<A, E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized,
{
}

impl<A, E> Overlappable<Self> for DisjointAtomsSet<A, E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized,
{
    fn set_relation(&self, other: &Self) -> SetRelation {
        use SetRelation::*;
        let mut disjoint = true;
        'outer: for set_atom in &self.0 {
            for o in &other.0 {
                if set_atom.set_relation(o) != Disjoint {
                    disjoint = false;
                    break 'outer;
                }
            }
        }
        if disjoint {
            Disjoint
        } else {
            let self_is_contained = self.subtract(other).0 == HashSet::new();
            let other_is_contained = other.subtract(self).0 == HashSet::new();
            if other_is_contained && self_is_contained {
                Equal
            } else if other_is_contained {
                Superset
            } else if self_is_contained {
                Subset
            } else {
                Overlapping
            }
        }
    }
}

impl<A, E> ComparableSet<E> for DisjointAtomsSet<A, E>
where
    A: AtomAlgebra + Container<Elem = E> + Clone + Debug + Eq + Hash,
    E: ?Sized,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// A `BTreeSet<u32>`-backed atom used throughout this module's tests.
    ///
    /// `intersect_overlapping` deliberately splits its result into multiple atoms
    /// once it reaches 2, 3 or 4 elements (falling back to a single atom
    /// otherwise) purely so that every arm of `CompoundAtomOperationRes` is
    /// reachable from a test; it has no bearing on the *set* the atom represents.
    /// `subtract_overlapping`/`subtract_subset` stay single-atom on purpose: the
    /// private `atom_union` bookkeeping below only ever handles `Compound(One)`
    /// for a subtraction result (anything else is `unreachable!()` in the
    /// reference), so an `AtomOperations` impl meant to be used with `union()`
    /// must keep subtraction single-atom. See the report for this as a
    /// documented, not-fixed, implicit precondition of the reference algebra.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct TestAtom(BTreeSet<u32>);

    impl TestAtom {
        fn new<const N: usize>(items: [u32; N]) -> Self {
            Self(BTreeSet::from(items))
        }
    }

    impl Container for TestAtom {
        type Elem = u32;
        fn contains(&self, n: &u32) -> bool {
            self.0.contains(n)
        }
    }

    impl Overlappable<Self> for TestAtom {
        fn set_relation(&self, other: &Self) -> SetRelation {
            use SetRelation::*;
            if self == other {
                Equal
            } else if self.0.is_subset(&other.0) {
                Subset
            } else if self.0.is_superset(&other.0) {
                Superset
            } else if self.0.is_disjoint(&other.0) {
                Disjoint
            } else {
                Overlapping
            }
        }
    }

    enum TestAtomOpsRes {
        One(BTreeSet<u32>),
        Two(BTreeSet<u32>, BTreeSet<u32>),
        Three(BTreeSet<u32>, BTreeSet<u32>, BTreeSet<u32>),
        Four(BTreeSet<u32>, BTreeSet<u32>, BTreeSet<u32>, BTreeSet<u32>),
    }
    impl From<TestAtomOpsRes> for CompoundAtomOperationRes<TestAtom> {
        fn from(value: TestAtomOpsRes) -> Self {
            use TestAtomOpsRes::*;
            match value {
                One(a) => Self::One(TestAtom(a)),
                Two(a, b) => Self::Two(TestAtom(a), TestAtom(b)),
                Three(a, b, c) => Self::Three(TestAtom(a), TestAtom(b), TestAtom(c)),
                Four(a, b, c, d) => Self::Four(TestAtom(a), TestAtom(b), TestAtom(c), TestAtom(d)),
            }
        }
    }

    impl AtomOperations for TestAtom {
        type SubtractSubsetRes = TestAtomOpsRes;
        type SubtractOverlappingRes = TestAtomOpsRes;
        type IntersectOverlappingRes = TestAtomOpsRes;
        fn subtract_subset(&self, other: &Self) -> Self::SubtractSubsetRes {
            TestAtomOpsRes::One(self.0.difference(&other.0).copied().collect())
        }
        fn subtract_overlapping(&self, other: &Self) -> Self::SubtractOverlappingRes {
            TestAtomOpsRes::One(self.0.difference(&other.0).copied().collect())
        }
        fn intersect_overlapping(&self, other: &Self) -> Self::IntersectOverlappingRes {
            use TestAtomOpsRes::*;
            let res: BTreeSet<u32> = self.0.intersection(&other.0).copied().collect();
            match res.len() {
                2 => {
                    let mut it = res.into_iter();
                    Two(BTreeSet::from([it.next().unwrap()]), BTreeSet::from([it.next().unwrap()]))
                }
                3 => {
                    let mut it = res.into_iter();
                    Three(
                        BTreeSet::from([it.next().unwrap()]),
                        BTreeSet::from([it.next().unwrap()]),
                        BTreeSet::from([it.next().unwrap()]),
                    )
                }
                4 => {
                    let mut it = res.into_iter();
                    Four(
                        BTreeSet::from([it.next().unwrap()]),
                        BTreeSet::from([it.next().unwrap()]),
                        BTreeSet::from([it.next().unwrap()]),
                        BTreeSet::from([it.next().unwrap()]),
                    )
                }
                _ => One(res),
            }
        }
    }
    impl AtomAlgebra for TestAtom {}

    type TestSet = DisjointAtomsSet<TestAtom, u32>;

    impl TestSet {
        fn new<const N: usize>(items: [u32; N]) -> Self {
            Self::from_atom(TestAtom::new(items))
        }
    }

    mod construction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn from_atom_wraps_a_single_atom() {
            let a = TestSet::from_atom(TestAtom::new([20, 30, 40]));
            assert_eq!(a.atoms(), &HashSet::from([TestAtom::new([20, 30, 40])]));
        }

        #[test]
        fn empty_has_no_atoms() {
            let a: TestSet = TestSet::empty();
            assert_eq!(a.atoms(), &HashSet::new());
        }

        #[test]
        fn atoms_returns_an_owned_reference_view() {
            let set = HashSet::from([TestAtom::new([20, 30, 40]), TestAtom::new([80, 60, 20])]);
            let res = DisjointAtomsSet(set.clone());
            assert_eq!(res.atoms(), &set);
        }

        #[test]
        fn atoms_ref_returns_references_to_every_atom() {
            let a = TestAtom::new([20, 30, 40]);
            let b = TestAtom::new([80, 60, 20]);
            let set = HashSet::from([a.clone(), b.clone()]);
            let res = DisjointAtomsSet(set);
            assert_eq!(res.atoms_ref(), HashSet::from([&a, &b]));
        }
    }

    /// `AtomAlgebra`'s default `union`/`intersect`/`subtract`, exhaustive over every
    /// `SetRelation` variant. `intersect` is additionally exhaustive over every
    /// `CompoundAtomOperationRes` arity, since `TestAtom::intersect_overlapping` is
    /// the one operation in this module allowed to split into more than one atom.
    mod atom_operations {
        use super::*;
        use pretty_assertions::assert_eq;
        use AtomOperationRes::*;
        use CompoundAtomOperationRes::*;

        fn assert_same_shape(res: AtomOperationRes<TestAtom>, expected: AtomOperationRes<TestAtom>) {
            match (res, expected) {
                (EmptySet, EmptySet) | (Lhs, Lhs) | (Rhs, Rhs) | (Both, Both) => (),
                (Compound(One(ra)), Compound(One(ea))) => assert_eq!(ra, ea),
                (Compound(Two(ra, rb)), Compound(Two(ea, eb))) => {
                    assert_eq!(ra, ea);
                    assert_eq!(rb, eb);
                }
                (Compound(Three(ra, rb, rc)), Compound(Three(ea, eb, ec))) => {
                    assert_eq!(ra, ea);
                    assert_eq!(rb, eb);
                    assert_eq!(rc, ec);
                }
                (Compound(Four(ra, rb, rc, rd)), Compound(Four(ea, eb, ec, ed))) => {
                    assert_eq!(ra, ea);
                    assert_eq!(rb, eb);
                    assert_eq!(rc, ec);
                    assert_eq!(rd, ed);
                }
                _ => panic!("result and expectation have a different shape"),
            }
        }

        mod union {
            use super::*;
            use test_case::test_case;

            #[test_case(TestAtom::new([1,2,3]), TestAtom::new([1,2,3]), Lhs; "equal keeps lhs")]
            #[test_case(TestAtom::new([1,2,3,4,5]), TestAtom::new([1,2,3]), Lhs; "superset keeps lhs")]
            #[test_case(TestAtom::new([1,2,3]), TestAtom::new([1,2,3,50,10]), Rhs; "subset keeps rhs")]
            #[test_case(
                TestAtom::new([1,2,3,99,999,9,10]), TestAtom::new([2,3,4,5,6,99,999,9]),
                Compound(Two(TestAtom::new([1,10]), TestAtom::new([4,5,6,2,3,9,99,999])));
                "overlapping produces the lhs remainder plus the whole rhs"
            )]
            #[test_case(TestAtom::new([1,2,3]), TestAtom::new([5,6]), Both; "disjoint keeps both")]
            fn matches_set_relation(a: TestAtom, b: TestAtom, expected: AtomOperationRes<TestAtom>) {
                assert_same_shape(a.union(&b), expected);
            }
        }

        mod intersect {
            use super::*;
            use test_case::test_case;

            #[test_case(TestAtom::new([1,2,3]), TestAtom::new([1,2,3]), Lhs; "equal keeps lhs")]
            #[test_case(TestAtom::new([1,2,3,4,5]), TestAtom::new([1,2,3]), Rhs; "superset keeps rhs")]
            #[test_case(TestAtom::new([1,2,3]), TestAtom::new([1,2,3,50,10]), Lhs; "subset keeps lhs")]
            #[test_case(TestAtom::new([1,2,3]), TestAtom::new([5,6]), EmptySet; "disjoint is empty")]
            #[test_case(
                TestAtom::new([1,2,3]), TestAtom::new([3,4,5]),
                Compound(One(TestAtom::new([3])));
                "overlapping with a 1-element intersection stays one atom"
            )]
            #[test_case(
                TestAtom::new([1,2,3]), TestAtom::new([2,3,4]),
                Compound(Two(TestAtom::new([2]), TestAtom::new([3])));
                "overlapping with a 2-element intersection splits in two"
            )]
            #[test_case(
                TestAtom::new([1,2,3,4]), TestAtom::new([2,3,4,5]),
                Compound(Three(TestAtom::new([2]), TestAtom::new([3]), TestAtom::new([4])));
                "overlapping with a 3-element intersection splits in three"
            )]
            #[test_case(
                TestAtom::new([1,2,3,4,5]), TestAtom::new([2,3,4,5,6]),
                Compound(Four(TestAtom::new([2]), TestAtom::new([3]), TestAtom::new([4]), TestAtom::new([5])));
                "overlapping with a 4-element intersection splits in four"
            )]
            #[test_case(
                TestAtom::new([1,2,3,4,5,6]), TestAtom::new([2,3,4,5,6,7]),
                Compound(One(TestAtom::new([2,3,4,5,6])));
                "overlapping with a 5-element intersection falls back to one atom"
            )]
            fn matches_set_relation(a: TestAtom, b: TestAtom, expected: AtomOperationRes<TestAtom>) {
                assert_same_shape(a.intersect(&b), expected);
            }
        }

        mod subtract {
            use super::*;
            use test_case::test_case;

            #[test_case(TestAtom::new([1,2,3]), TestAtom::new([1,2,3]), EmptySet; "equal is empty")]
            #[test_case(
                TestAtom::new([1,2,3,4,5]), TestAtom::new([1,2,3]),
                Compound(One(TestAtom::new([4,5])));
                "superset keeps the remainder"
            )]
            #[test_case(TestAtom::new([1,2,3]), TestAtom::new([1,2,3,50,10]), EmptySet; "subset is empty")]
            #[test_case(
                TestAtom::new([1,2,3,10,20,30]), TestAtom::new([2,4,5,10,20,30]),
                Compound(One(TestAtom::new([1,3])));
                "overlapping keeps the remainder"
            )]
            #[test_case(TestAtom::new([1,2,3]), TestAtom::new([5,6]), Lhs; "disjoint keeps lhs unchanged")]
            fn matches_set_relation(a: TestAtom, b: TestAtom, expected: AtomOperationRes<TestAtom>) {
                assert_same_shape(a.subtract(&b), expected);
            }
        }
    }

    /// The private per-atom bookkeeping (`atom_union`/`atom_intersection`/
    /// `atom_subtraction`) that folds a single incoming atom into an existing
    /// disjoint partition. These are internal to `DisjointAtomsSet` but directly
    /// reachable from tests in this same file.
    mod atom_set_merge {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([1,2,3,4]), TestAtom::new([40,50,60])])),
            TestAtom::new([2,3,4,5,40]),
            DisjointAtomsSet(HashSet::from([TestAtom::new([50,60]), TestAtom::new([1]), TestAtom::new([40,5,4,3,2])]));
            "folds the incoming atom in, shrinking whatever it overlaps"
        )]
        fn atom_union_folds_a_single_atom_in(set: TestSet, atm: TestAtom, expected: TestSet) {
            assert_eq!(set.atom_union(atm).0, expected.0);
        }

        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([1,3,4,5,6,7,8]), TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20]), TestAtom::new([30,50])])),
            TestAtom::new([3,4,5,6,7,8,9,10,11,12,13,14,30,50]),
            DisjointAtomsSet(HashSet::from([
                TestAtom::new([3,4,5,6,7,8]), TestAtom::new([9]), TestAtom::new([10]), TestAtom::new([11]),
                TestAtom::new([12]), TestAtom::new([13]), TestAtom::new([14]), TestAtom::new([30,50]),
            ]));
            "keeps only the overlap with each existing atom (Compound(One) per atom)"
        )]
        fn atom_intersection_keeps_only_the_overlap(set: TestSet, atm: TestAtom, expected: TestSet) {
            assert_eq!(set.atom_intersection(atm).0, expected.0);
        }

        #[test]
        fn atom_intersection_handles_every_compound_arity_from_a_single_atom() {
            // A single existing atom, split into 2/3/4 pieces by `intersect_overlapping`
            // (see `TestAtom`'s doc comment) depending on how large the overlap is.
            let two = DisjointAtomsSet(HashSet::from([TestAtom::new([1, 2, 3])]));
            assert_eq!(
                two.atom_intersection(TestAtom::new([2, 3, 4])).0,
                HashSet::from([TestAtom::new([2]), TestAtom::new([3])])
            );

            let three = DisjointAtomsSet(HashSet::from([TestAtom::new([1, 2, 3, 4])]));
            assert_eq!(
                three.atom_intersection(TestAtom::new([2, 3, 4, 5])).0,
                HashSet::from([TestAtom::new([2]), TestAtom::new([3]), TestAtom::new([4])])
            );

            let four = DisjointAtomsSet(HashSet::from([TestAtom::new([1, 2, 3, 4, 5])]));
            assert_eq!(
                four.atom_intersection(TestAtom::new([2, 3, 4, 5, 6])).0,
                HashSet::from([TestAtom::new([2]), TestAtom::new([3]), TestAtom::new([4]), TestAtom::new([5])])
            );
        }

        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([1,3,4,5,6,7,8]), TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20]), TestAtom::new([30,50])])),
            TestAtom::new([1,2,9,10,11,12,13]),
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,4,5,6,7,8]), TestAtom::new([14,20]), TestAtom::new([30,50])]));
            "removes the incoming atom from whatever it overlaps"
        )]
        fn atom_subtraction_removes_the_incoming_atom(set: TestSet, atm: TestAtom, expected: TestSet) {
            assert_eq!(set.atom_subtraction(atm).0, expected.0);
        }

        /// `atom_subtraction`'s match has arms for every `CompoundAtomOperationRes`
        /// arity (unlike `atom_union`, which only accepts `Compound(One)` and treats
        /// anything wider as `unreachable!()`). A `subtract_overlapping` that itself
        /// splits into multiple pieces is therefore *only* safe to combine with
        /// subtraction-only usage of `DisjointAtomsSet`, never with `union()` — this
        /// type exists solely to exercise that path; it is not meant to model a
        /// realistic atom.
        mod atom_subtraction_arity_coverage {
            use super::*;
            use pretty_assertions::assert_eq;

            #[derive(Clone, Debug, PartialEq, Eq, Hash)]
            struct SplittingAtom(BTreeSet<u32>);

            impl SplittingAtom {
                fn new<const N: usize>(items: [u32; N]) -> Self {
                    Self(BTreeSet::from(items))
                }
            }
            impl Container for SplittingAtom {
                type Elem = u32;
                fn contains(&self, n: &u32) -> bool {
                    self.0.contains(n)
                }
            }
            impl Overlappable<Self> for SplittingAtom {
                fn set_relation(&self, other: &Self) -> SetRelation {
                    use SetRelation::*;
                    if self == other {
                        Equal
                    } else if self.0.is_subset(&other.0) {
                        Subset
                    } else if self.0.is_superset(&other.0) {
                        Superset
                    } else if self.0.is_disjoint(&other.0) {
                        Disjoint
                    } else {
                        Overlapping
                    }
                }
            }
            enum Res {
                One(BTreeSet<u32>),
                Two(BTreeSet<u32>, BTreeSet<u32>),
                Three(BTreeSet<u32>, BTreeSet<u32>, BTreeSet<u32>),
                Four(BTreeSet<u32>, BTreeSet<u32>, BTreeSet<u32>, BTreeSet<u32>),
            }
            impl From<Res> for CompoundAtomOperationRes<SplittingAtom> {
                fn from(value: Res) -> Self {
                    use Res::*;
                    match value {
                        One(a) => Self::One(SplittingAtom(a)),
                        Two(a, b) => Self::Two(SplittingAtom(a), SplittingAtom(b)),
                        Three(a, b, c) => Self::Three(SplittingAtom(a), SplittingAtom(b), SplittingAtom(c)),
                        Four(a, b, c, d) => {
                            Self::Four(SplittingAtom(a), SplittingAtom(b), SplittingAtom(c), SplittingAtom(d))
                        }
                    }
                }
            }
            impl AtomOperations for SplittingAtom {
                type SubtractSubsetRes = Res;
                type SubtractOverlappingRes = Res;
                type IntersectOverlappingRes = Res;
                fn subtract_subset(&self, other: &Self) -> Self::SubtractSubsetRes {
                    Res::One(self.0.difference(&other.0).copied().collect())
                }
                fn subtract_overlapping(&self, other: &Self) -> Self::SubtractOverlappingRes {
                    let res: BTreeSet<u32> = self.0.difference(&other.0).copied().collect();
                    let mut it = res.into_iter();
                    match it.len() {
                        2 => Res::Two(BTreeSet::from([it.next().unwrap()]), BTreeSet::from([it.next().unwrap()])),
                        3 => Res::Three(
                            BTreeSet::from([it.next().unwrap()]),
                            BTreeSet::from([it.next().unwrap()]),
                            BTreeSet::from([it.next().unwrap()]),
                        ),
                        4 => Res::Four(
                            BTreeSet::from([it.next().unwrap()]),
                            BTreeSet::from([it.next().unwrap()]),
                            BTreeSet::from([it.next().unwrap()]),
                            BTreeSet::from([it.next().unwrap()]),
                        ),
                        _ => Res::One(it.collect()),
                    }
                }
                fn intersect_overlapping(&self, other: &Self) -> Self::IntersectOverlappingRes {
                    Res::One(self.0.intersection(&other.0).copied().collect())
                }
            }
            impl AtomAlgebra for SplittingAtom {}

            #[test]
            fn splits_into_two_three_and_four_pieces_as_subtract_overlapping_dictates() {
                // Each `other` here shares *some* elements with `self` but also has an
                // element `self` doesn't (so the relation is `Overlapping`, which is
                // what routes through `subtract_overlapping` rather than
                // `subtract_subset` — a plain "other ⊆ self" would hit `subtract_subset`
                // instead, which stays single-atom in this mock.

                let set = DisjointAtomsSet(HashSet::from([SplittingAtom::new([1, 2, 3, 4, 5, 6, 7, 8])]));
                // self \ other = {1,2,3,4,5,6,7,8} \ {5,6,7,8,9,10} = {1,2,3,4}: 4 elements ->
                // Four.
                let res = set.atom_subtraction(SplittingAtom::new([5, 6, 7, 8, 9, 10]));
                assert_eq!(
                    res.0,
                    HashSet::from([
                        SplittingAtom::new([1]),
                        SplittingAtom::new([2]),
                        SplittingAtom::new([3]),
                        SplittingAtom::new([4]),
                    ])
                );

                let set = DisjointAtomsSet(HashSet::from([SplittingAtom::new([1, 2, 3, 4, 5])]));
                // self \ other = {1,2,3,4,5} \ {4,5,6} = {1,2,3}: 3 elements -> Three.
                let res = set.atom_subtraction(SplittingAtom::new([4, 5, 6]));
                assert_eq!(
                    res.0,
                    HashSet::from([SplittingAtom::new([1]), SplittingAtom::new([2]), SplittingAtom::new([3])])
                );

                let set = DisjointAtomsSet(HashSet::from([SplittingAtom::new([1, 2, 3, 4])]));
                // self \ other = {1,2,3,4} \ {3,4,5} = {1,2}: 2 elements -> Two.
                let res = set.atom_subtraction(SplittingAtom::new([3, 4, 5]));
                assert_eq!(res.0, HashSet::from([SplittingAtom::new([1]), SplittingAtom::new([2])]));
            }
        }
    }

    /// `DisjointAtomsSet`'s public `union`/`intersect`/`subtract` (via `|`, `&`,
    /// `/`), exhaustive over every whole-set `SetRelation`.
    mod set_algebra {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10]), TestAtom::new([13,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([10,2]), TestAtom::new([9]), TestAtom::new([13,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,14]), TestAtom::new([9]), TestAtom::new([2,10])]));
            "equal"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([1,3,4,5,6,7,8]), TestAtom::new([30,50])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([1,3,4,5,6,7,8]), TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20]), TestAtom::new([30,50])]));
            "disjoint"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,2,20]), TestAtom::new([11,12,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,13,20]), TestAtom::new([11,12,14]), TestAtom::new([9,10])]));
            "superset"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,2,20]), TestAtom::new([9]), TestAtom::new([11,12,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,13,20]), TestAtom::new([11,12,14]), TestAtom::new([9])]));
            "subset"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,99]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,2,20]), TestAtom::new([9,34]), TestAtom::new([11,12,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,13,20]), TestAtom::new([11,12,14]), TestAtom::new([9,34]), TestAtom::new([99])]));
            "overlapping"
        )]
        fn union(a: TestSet, b: TestSet, expected: TestSet) {
            assert_eq!((a | b).0, expected.0);
        }

        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10]), TestAtom::new([13,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([10,2]), TestAtom::new([9]), TestAtom::new([13,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,14]), TestAtom::new([9]), TestAtom::new([2,10])]));
            "equal"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([1,3,4,5,6,7,8]), TestAtom::new([30,50])])),
            DisjointAtomsSet(HashSet::new());
            "disjoint"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,2,20]), TestAtom::new([11,12,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([2]), TestAtom::new([13]), TestAtom::new([11]), TestAtom::new([12]), TestAtom::new([20]), TestAtom::new([14])]));
            "superset"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,2,20]), TestAtom::new([9]), TestAtom::new([11,12,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([2]), TestAtom::new([13]), TestAtom::new([20]), TestAtom::new([14]), TestAtom::new([9])]));
            "subset"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,99]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,2,20]), TestAtom::new([9,34]), TestAtom::new([11,12,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([2]), TestAtom::new([9]), TestAtom::new([20]), TestAtom::new([13]), TestAtom::new([14])]));
            "overlapping"
        )]
        fn intersect(a: TestSet, b: TestSet, expected: TestSet) {
            assert_eq!((a & b).0, expected.0);
        }

        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10]), TestAtom::new([13,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([10,2]), TestAtom::new([9]), TestAtom::new([13,14])])),
            DisjointAtomsSet(HashSet::new());
            "equal"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([1,3,4,5,6,7,8]), TestAtom::new([30,50])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20])]));
            "disjoint"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,10,11,12]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,2,20]), TestAtom::new([11,12,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([9,10])]));
            "superset"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([13,2,20]), TestAtom::new([9]), TestAtom::new([11,12,14])])),
            DisjointAtomsSet(HashSet::new());
            "subset"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,9,99]), TestAtom::new([13,14,20])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([2,20]), TestAtom::new([9,34]), TestAtom::new([11,12,14])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([99]), TestAtom::new([13])]));
            "overlapping"
        )]
        fn subtract(a: TestSet, b: TestSet, expected: TestSet) {
            assert_eq!((a / b).0, expected.0);
        }

        #[test]
        fn a_mixed_expression_matches_hand_computed_atoms() {
            let a = TestSet::new([1, 2, 3, 4]);
            let b = TestSet::new([0, 2, 3, 4]);
            let c = TestSet::new([0, 5, 3, 40]);
            let d = TestSet::new([1, 2]);
            let e = TestSet::new([1, 20]);
            let f = a & (b / c) | e;
            assert_eq!(f.0, HashSet::from([TestAtom::new([1, 20]), TestAtom::new([2, 4])]));
            let _ = d; // kept only to mirror the reference's variable naming; unused on purpose
        }
    }

    mod containment {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(TestSet::new([4,5,70]), 70, true; "leaf hit")]
        #[test_case(TestSet::new([4,5,70]), 71, false; "leaf miss")]
        #[test_case(TestSet::new([4,6,7]) | TestSet::new([10,60,8,7]), 60, true; "union hit")]
        #[test_case(TestSet::new([4,6,7]) | TestSet::new([10,60,8,7]), 603, false; "union miss")]
        #[test_case(TestSet::new([30,50,60,70]) & TestSet::new([60,70,80]), 60, true; "intersect hit")]
        #[test_case(TestSet::new([30,50,60,70]) & TestSet::new([60,70,80]), 80, false; "intersect miss")]
        #[test_case(TestSet::new([2,4,6,8]) / TestSet::new([4,6]), 8, true; "subtraction hit")]
        #[test_case(TestSet::new([2,4,6,8]) / TestSet::new([4,6]), 4, false; "subtraction miss")]
        #[test_case(TestSet::empty(), 1, false; "empty set never contains anything")]
        fn contains_matches_expectation(s: TestSet, n: u32, expected: bool) {
            assert_eq!(s.contains(&n), expected);
        }
    }

    mod set_relation {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,4,5]), TestAtom::new([7,8,9])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,8]), TestAtom::new([5]), TestAtom::new([7,4,9])])),
            SetRelation::Equal; "equal"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,4,5,6,0]), TestAtom::new([7,8,9])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,8]), TestAtom::new([5]), TestAtom::new([7,4,9])])),
            SetRelation::Superset; "superset"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,4,5]), TestAtom::new([7,8,9])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,8]), TestAtom::new([5]), TestAtom::new([500]), TestAtom::new([7,4,9])])),
            SetRelation::Subset; "subset"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,4,5]), TestAtom::new([7,8])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([55,66,77]), TestAtom::new([9])])),
            SetRelation::Disjoint; "disjoint"
        )]
        #[test_case(
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,4,5]), TestAtom::new([7,8,9,10])])),
            DisjointAtomsSet(HashSet::from([TestAtom::new([3,8,5,7,4,9,41,44])])),
            SetRelation::Overlapping; "overlapping"
        )]
        fn matches_expectation(a: TestSet, b: TestSet, expected: SetRelation) {
            assert_eq!(a.set_relation(&b), expected);
        }
    }

    mod algebra_invariants {
        use super::*;
        use pretty_assertions::assert_eq;

        const DOMAIN: [u32; 4] = [0, 1, 2, 3];

        fn all_subsets() -> Vec<TestSet> {
            let mut out = Vec::new();
            // mask == 0 is the empty subset: it must be represented as `empty()`
            // (zero atoms), not as `from_atom` of an atom with zero elements — an
            // atom that represents nothing isn't a valid member of a disjoint
            // partition (see `resulting_atoms_are_always_pairwise_disjoint` below,
            // which is exactly what would catch that degenerate case).
            out.push(TestSet::empty());
            for mask in 1..(1u32 << DOMAIN.len()) {
                let mut items = BTreeSet::new();
                for (bit, item) in DOMAIN.iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        items.insert(*item);
                    }
                }
                out.push(DisjointAtomsSet::from_atom(TestAtom(items)));
            }
            out
        }

        fn contains_profile(s: &TestSet) -> Vec<bool> {
            DOMAIN.iter().map(|n| s.contains(n)).collect()
        }

        #[test]
        fn union_and_intersection_are_idempotent_for_every_atom_in_the_domain() {
            for a in all_subsets() {
                let expected = contains_profile(&a);
                assert_eq!(contains_profile(&(a.clone() | a.clone())), expected);
                assert_eq!(contains_profile(&(a.clone() & a.clone())), expected);
            }
        }

        #[test]
        fn union_and_intersection_are_commutative_for_every_pair_in_the_domain() {
            for a in all_subsets() {
                for b in all_subsets() {
                    assert_eq!(
                        contains_profile(&(a.clone() | b.clone())),
                        contains_profile(&(b.clone() | a.clone()))
                    );
                    assert_eq!(
                        contains_profile(&(a.clone() & b.clone())),
                        contains_profile(&(b.clone() & a.clone()))
                    );
                }
            }
        }

        #[test]
        fn union_and_intersection_are_associative_for_a_sample_of_triples() {
            let subsets = all_subsets();
            for a in subsets.iter().step_by(3) {
                for b in subsets.iter().step_by(5) {
                    for c in subsets.iter().step_by(7) {
                        assert_eq!(
                            contains_profile(&((a.clone() | b.clone()) | c.clone())),
                            contains_profile(&(a.clone() | (b.clone() | c.clone())))
                        );
                        assert_eq!(
                            contains_profile(&((a.clone() & b.clone()) & c.clone())),
                            contains_profile(&(a.clone() & (b.clone() & c.clone())))
                        );
                    }
                }
            }
        }

        #[test]
        fn de_morgan_holds_relative_to_a_full_universe_atom() {
            let universe = || DisjointAtomsSet::from_atom(TestAtom(BTreeSet::from(DOMAIN)));
            let complement = |s: TestSet| universe() / s;

            for a in all_subsets() {
                for b in all_subsets() {
                    let lhs = complement(a.clone() | b.clone());
                    let rhs = complement(a.clone()) & complement(b.clone());
                    assert_eq!(contains_profile(&lhs), contains_profile(&rhs));

                    let lhs = complement(a.clone() & b.clone());
                    let rhs = complement(a.clone()) | complement(b.clone());
                    assert_eq!(contains_profile(&lhs), contains_profile(&rhs));
                }
            }
        }

        #[test]
        fn subtracting_a_set_from_itself_has_no_atoms() {
            for a in all_subsets() {
                let diff = a.clone() / a;
                assert_eq!(diff.0, HashSet::new());
            }
        }

        /// The whole point of this representation: no matter which operation
        /// produced it, the resulting atoms must be pairwise disjoint (otherwise
        /// membership/relation queries built on top of the invariant would be
        /// unsound).
        #[test]
        fn resulting_atoms_are_always_pairwise_disjoint() {
            fn assert_pairwise_disjoint(s: &TestSet) {
                let atoms: Vec<&TestAtom> = s.atoms().iter().collect();
                for i in 0..atoms.len() {
                    for j in (i + 1)..atoms.len() {
                        assert_eq!(
                            atoms[i].set_relation(atoms[j]),
                            SetRelation::Disjoint,
                            "atoms {:?} and {:?} are not disjoint",
                            atoms[i],
                            atoms[j]
                        );
                    }
                }
            }
            for a in all_subsets() {
                for b in all_subsets() {
                    assert_pairwise_disjoint(&(a.clone() | b.clone()));
                    assert_pairwise_disjoint(&(a.clone() & b.clone()));
                    assert_pairwise_disjoint(&(a.clone() / b.clone()));
                }
            }
        }
    }

    /// Unlike `ast_simple`/`ast_smart`, atoms in a `DisjointAtomsSet` are pairwise
    /// disjoint by construction, so any two sets can always be compared: this is
    /// the one representation that is a `ComparableSet`, not just `UncomparableSet`.
    mod trait_conformance {
        use super::*;

        fn assert_comparable_set<S: ComparableSet<E>, E: ?Sized>() {}

        #[test]
        fn disjoint_atoms_set_is_a_comparable_set() {
            assert_comparable_set::<TestSet, u32>();
        }
    }
}
