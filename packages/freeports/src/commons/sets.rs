//! A generic set algebra: [`Container`], [`Overlappable`], [`Set`], [`SetOps`], [`SetRelation`].
//!
//! The vocabulary shared by the three submodules — [`ast_simple`], [`ast_smart`],
//! [`indipendent_atoms`] — each of which represents the same algebra of union, intersection and
//! difference over a [`Container`] with a different internal model. [`Set`] adds the degenerate
//! `Empty` and `Universe` cases on top of any of them.
//!
//! Three representations rather than one because they trade differently: an unsimplified tree is
//! cheapest to build, a simplifying tree keeps expressions small, and a set of disjoint atoms is
//! the only one where two sets can always be compared. The tests here check the vocabulary itself
//! and the agreement of all three on the same expression.

pub mod ast_simple;
pub mod ast_smart;
pub mod indipendent_atoms;

use std::ops::{BitAnd, BitOr, Div};

/// The three boolean operations an internal AST branch can carry, and their
/// truth-table evaluation once both sides have been reduced to `contains`
/// booleans.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SetOps {
    Union,
    Inter,
    Sub,
}

impl SetOps {
    fn call(&self, a: bool, b: bool) -> bool {
        match self {
            Self::Union => a || b,
            Self::Inter => a && b,
            Self::Sub => a && !b,
        }
    }
}

/// How two sets relate to one another, as decided by `Overlappable`.
#[derive(Debug, PartialEq)]
pub enum SetRelation {
    Overlapping,
    Subset,
    Superset,
    Disjoint,
    Equal,
}

/// Membership test: the one operation every representation in this module
/// (and `Set<S,E>` itself) must support.
pub trait Container {
    type Elem: ?Sized;
    fn contains(&self, e: &Self::Elem) -> bool;
}

/// Types that can classify their relationship to another instance of `Rhs`
/// (equal/subset/superset/disjoint/overlapping) without necessarily being
/// able to enumerate their elements.
pub trait Overlappable<Rhs> {
    fn set_relation(&self, other: &Rhs) -> SetRelation;
}

/// Adds the two degenerate cases (`Empty`, `Universe`) on top of any concrete
/// representation `S`, so no concrete representation needs to model them
/// itself.
pub enum Set<S, E>
where
    S: Container<Elem = E> + SetAlgebra,
    E: ?Sized,
{
    Empty,
    Universe,
    Set(S),
}

impl<S, E> Container for Set<S, E>
where
    S: Container<Elem = E> + SetAlgebra,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self, ele: &Self::Elem) -> bool {
        match self {
            Self::Empty => false,
            Self::Universe => true,
            Self::Set(set) => set.contains(ele),
        }
    }
}

impl<S, E> BitOr<Self> for Set<S, E>
where
    S: Container<Elem = E> + SetAlgebra,
    E: ?Sized,
{
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::Universe, _) => Self::Universe,
            (_, Self::Universe) => Self::Universe,
            (a, Self::Empty) => a,
            (Self::Empty, b) => b,
            (Self::Set(a), Self::Set(b)) => Self::Set(a | b),
        }
    }
}

impl<S, E> BitAnd<Self> for Set<S, E>
where
    S: Container<Elem = E> + SetAlgebra,
    E: ?Sized,
{
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        match (self, rhs) {
            (a, Self::Universe) => a,
            (Self::Universe, b) => b,
            (Self::Empty, _) => Self::Empty,
            (_, Self::Empty) => Self::Empty,
            (Self::Set(a), Self::Set(b)) => Self::Set(a & b),
        }
    }
}

impl<S, E> Div<Self> for Set<S, E>
where
    S: Container<Elem = E> + SetAlgebra,
    E: ?Sized,
{
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        match (self, rhs) {
            // There is no generic way to enumerate "everything not in a
            // `Container`", so `Universe` has no complement in this
            // vocabulary: subtracting anything from it is deliberately
            // unimplemented rather than given invented behavior. See
            // `tests::set_algebra_difference::universe_as_minuend_is_unimplemented`.
            (Self::Universe, _) => unimplemented!("Set::Universe has no generic complement"),
            (_, Self::Universe) => Self::Empty,
            (a, Self::Empty) => a,
            (Self::Empty, _) => Self::Empty,
            (Self::Set(a), Self::Set(b)) => Self::Set(a / b),
        }
    }
}

/// Marker trait for a concrete set representation that supports the three
/// binary operations: implemented by `AstSet`, `SmartAstSet`, `DisjointAtomsSet`
/// and by `Set<S,E>` itself (so `Set<Set<S,E>,E>` would still type-check, even
/// if nothing in this crate needs that).
pub trait SetAlgebra: BitOr<Self, Output = Self> + BitAnd<Self, Output = Self> + Div<Self, Output = Self> + Sized {}

/// A `SetAlgebra` whose two instances cannot in general be compared
/// (`ast_simple`, `ast_smart`): membership can still be tested per-element,
/// but no `Overlappable` relation is available.
pub trait UncomparableSet<E>: Container<Elem = E> + SetAlgebra
where
    E: ?Sized,
{
}

/// A `SetAlgebra` whose two instances can always be compared
/// (`indipendent_atoms`, thanks to its pairwise-disjoint-atoms invariant).
pub trait ComparableSet<E>: Container<Elem = E> + SetAlgebra + Overlappable<Self>
where
    E: ?Sized,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::{BitOr, BitAnd, Div};
    use std::collections::BTreeSet;

    /// A minimal `Container` + `SetAlgebra` leaf used only to exercise the generic
    /// `Set<S,E>` wrapper (Empty/Universe/Set) independently of any concrete
    /// representation (AST-based or atom-based).
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestLeaf(BTreeSet<i32>);

    impl TestLeaf {
        fn new<const N: usize>(items: [i32; N]) -> Self {
            Self(BTreeSet::from(items))
        }
    }

    impl Container for TestLeaf {
        type Elem = i32;
        fn contains(&self, e: &i32) -> bool {
            self.0.contains(e)
        }
    }
    impl BitOr<Self> for TestLeaf {
        type Output = Self;
        fn bitor(self, rhs: Self) -> Self {
            Self(self.0.union(&rhs.0).copied().collect())
        }
    }
    impl BitAnd<Self> for TestLeaf {
        type Output = Self;
        fn bitand(self, rhs: Self) -> Self {
            Self(self.0.intersection(&rhs.0).copied().collect())
        }
    }
    impl Div<Self> for TestLeaf {
        type Output = Self;
        fn div(self, rhs: Self) -> Self {
            Self(self.0.difference(&rhs.0).copied().collect())
        }
    }
    impl SetAlgebra for TestLeaf {}

    /// Flattened view of a `Set<TestLeaf,i32>` result, used because `Set` itself
    /// derives neither `Debug` nor `PartialEq` (it can't, in general: `S` needn't).
    #[derive(Debug, PartialEq)]
    enum Kind {
        Empty,
        Universe,
        Set(Vec<i32>),
    }

    fn classify(s: Set<TestLeaf, i32>) -> Kind {
        match s {
            Set::Empty => Kind::Empty,
            Set::Universe => Kind::Universe,
            Set::Set(TestLeaf(inner)) => Kind::Set(inner.into_iter().collect()),
        }
    }

    mod set_ops_truth_table {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(SetOps::Union, true, true, true; "union of true true is true")]
        #[test_case(SetOps::Union, true, false, true; "union of true false is true")]
        #[test_case(SetOps::Union, false, true, true; "union of false true is true")]
        #[test_case(SetOps::Union, false, false, false; "union of false false is false")]
        #[test_case(SetOps::Inter, true, true, true; "inter of true true is true")]
        #[test_case(SetOps::Inter, true, false, false; "inter of true false is false")]
        #[test_case(SetOps::Inter, false, true, false; "inter of false true is false")]
        #[test_case(SetOps::Inter, false, false, false; "inter of false false is false")]
        #[test_case(SetOps::Sub, true, true, false; "sub of true true is false")]
        #[test_case(SetOps::Sub, true, false, true; "sub of true false is true")]
        #[test_case(SetOps::Sub, false, true, false; "sub of false true is false")]
        #[test_case(SetOps::Sub, false, false, false; "sub of false false is false")]
        fn evaluates_as_expected(op: SetOps, a: bool, b: bool, expected: bool) {
            assert_eq!(op.call(a, b), expected);
        }
    }

    mod set_container {
        use super::*;

        #[test]
        fn empty_never_contains_anything() {
            let s: Set<TestLeaf, i32> = Set::Empty;
            assert!(!s.contains(&0));
            assert!(!s.contains(&42));
        }

        #[test]
        fn universe_always_contains_everything() {
            let s: Set<TestLeaf, i32> = Set::Universe;
            assert!(s.contains(&0));
            assert!(s.contains(&-999));
        }

        #[test]
        fn concrete_set_delegates_to_inner_container() {
            let s: Set<TestLeaf, i32> = Set::Set(TestLeaf::new([1, 2, 3]));
            assert!(s.contains(&2));
            assert!(!s.contains(&4));
        }
    }

    mod set_algebra_union {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(Set::Universe, Set::Empty, Kind::Universe; "universe or empty is universe")]
        #[test_case(Set::Universe, Set::Universe, Kind::Universe; "universe or universe is universe")]
        #[test_case(Set::Universe, Set::Set(TestLeaf::new([1,2])), Kind::Universe; "universe or set is universe")]
        #[test_case(Set::Empty, Set::Universe, Kind::Universe; "empty or universe is universe")]
        #[test_case(Set::Set(TestLeaf::new([1,2])), Set::Universe, Kind::Universe; "set or universe is universe")]
        #[test_case(Set::Empty, Set::Empty, Kind::Empty; "empty or empty is empty")]
        #[test_case(Set::Set(TestLeaf::new([1,2])), Set::Empty, Kind::Set(vec![1,2]); "set or empty is unchanged set")]
        #[test_case(Set::Empty, Set::Set(TestLeaf::new([3,4])), Kind::Set(vec![3,4]); "empty or set is unchanged set")]
        #[test_case(Set::Set(TestLeaf::new([1,2])), Set::Set(TestLeaf::new([2,3])), Kind::Set(vec![1,2,3]); "set or set is the real union")]
        fn matches_absorbing_element_rules(lhs: Set<TestLeaf, i32>, rhs: Set<TestLeaf, i32>, expected: Kind) {
            assert_eq!(classify(lhs | rhs), expected);
        }
    }

    mod set_algebra_intersection {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(Set::Empty, Set::Universe, Kind::Empty; "empty and universe keeps lhs")]
        #[test_case(Set::Universe, Set::Universe, Kind::Universe; "universe and universe is universe")]
        #[test_case(Set::Set(TestLeaf::new([1,2])), Set::Universe, Kind::Set(vec![1,2]); "set and universe keeps lhs set")]
        #[test_case(Set::Universe, Set::Empty, Kind::Empty; "universe and empty keeps rhs")]
        #[test_case(Set::Universe, Set::Set(TestLeaf::new([3,4])), Kind::Set(vec![3,4]); "universe and set keeps rhs set")]
        #[test_case(Set::Empty, Set::Empty, Kind::Empty; "empty and empty is empty")]
        #[test_case(Set::Empty, Set::Set(TestLeaf::new([1,2])), Kind::Empty; "empty and set is empty")]
        #[test_case(Set::Set(TestLeaf::new([1,2])), Set::Empty, Kind::Empty; "set and empty is empty")]
        #[test_case(Set::Set(TestLeaf::new([1,2,3])), Set::Set(TestLeaf::new([2,3,4])), Kind::Set(vec![2,3]); "set and set is the real intersection")]
        fn matches_absorbing_element_rules(lhs: Set<TestLeaf, i32>, rhs: Set<TestLeaf, i32>, expected: Kind) {
            assert_eq!(classify(lhs & rhs), expected);
        }
    }

    mod set_algebra_difference {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(Set::Empty, Set::Universe, Kind::Empty; "empty minus universe is empty")]
        #[test_case(Set::Set(TestLeaf::new([1,2])), Set::Universe, Kind::Empty; "set minus universe is empty")]
        #[test_case(Set::Empty, Set::Empty, Kind::Empty; "empty minus empty is empty")]
        #[test_case(Set::Set(TestLeaf::new([1,2])), Set::Empty, Kind::Set(vec![1,2]); "set minus empty is unchanged")]
        #[test_case(Set::Empty, Set::Set(TestLeaf::new([3,4])), Kind::Empty; "empty minus set is empty")]
        #[test_case(Set::Set(TestLeaf::new([1,2,3])), Set::Set(TestLeaf::new([2,3,4])), Kind::Set(vec![1]); "set minus set is the real difference")]
        fn matches_absorbing_element_rules(lhs: Set<TestLeaf, i32>, rhs: Set<TestLeaf, i32>, expected: Kind) {
            assert_eq!(classify(lhs / rhs), expected);
        }

        /// `Universe` has no generic complement in this vocabulary (there's no way to
        /// enumerate "everything not in a `Container`"), so subtracting anything from
        /// it is unimplemented today. This test documents (and pins) that gap rather
        /// than inventing behavior for it; see the open question in the report.
        #[test_case(Set::Empty; "universe minus empty panics")]
        #[test_case(Set::Universe; "universe minus universe panics")]
        #[test_case(Set::Set(TestLeaf::new([1])); "universe minus set panics")]
        #[should_panic]
        fn universe_as_minuend_is_unimplemented(rhs: Set<TestLeaf, i32>) {
            let _ = Set::<TestLeaf, i32>::Universe / rhs;
        }
    }

    /// Ties the three concrete representations (`ast_simple`, `ast_smart`,
    /// `indipendent_atoms`) back to the shared vocabulary defined in this module:
    /// they are meant to be different internal encodings of the *same* algebra, so
    /// for any expression built the same way, membership must agree regardless of
    /// which representation computed it.
    mod cross_representation_consistency {
        use super::*;
        use pretty_assertions::assert_eq;
        use crate::commons::sets::ast_simple::AstSet;
        use crate::commons::sets::ast_smart::SmartAstSet;
        use crate::commons::sets::indipendent_atoms::{AtomOperations, AtomAlgebra, CompoundAtomOperationRes, DisjointAtomsSet};

        /// A leaf over a small `u32` domain, reused as-is across the three
        /// representations: it satisfies `Container` (needed by all three),
        /// `Overlappable`/`Clone` (needed by `ast_smart`) and `AtomAlgebra` (needed
        /// by `indipendent_atoms`), so the "same" leaves can build the "same"
        /// expression in each representation.
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        struct DomainLeaf(BTreeSet<u32>);

        impl DomainLeaf {
            fn new<const N: usize>(items: [u32; N]) -> Self {
                Self(BTreeSet::from(items))
            }
        }

        impl Container for DomainLeaf {
            type Elem = u32;
            fn contains(&self, e: &u32) -> bool {
                self.0.contains(e)
            }
        }

        impl Overlappable<Self> for DomainLeaf {
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

        impl AtomOperations for DomainLeaf {
            type SubtractSubsetRes = CompoundAtomOperationRes<DomainLeaf>;
            type SubtractOverlappingRes = CompoundAtomOperationRes<DomainLeaf>;
            type IntersectOverlappingRes = CompoundAtomOperationRes<DomainLeaf>;
            fn subtract_subset(&self, other: &Self) -> Self::SubtractSubsetRes {
                CompoundAtomOperationRes::One(DomainLeaf(self.0.difference(&other.0).copied().collect()))
            }
            fn subtract_overlapping(&self, other: &Self) -> Self::SubtractOverlappingRes {
                CompoundAtomOperationRes::One(DomainLeaf(self.0.difference(&other.0).copied().collect()))
            }
            fn intersect_overlapping(&self, other: &Self) -> Self::IntersectOverlappingRes {
                CompoundAtomOperationRes::One(DomainLeaf(self.0.intersection(&other.0).copied().collect()))
            }
        }
        impl AtomAlgebra for DomainLeaf {}

        /// Every four-element subset of a small universe is enumerated below, so the seemingly
        /// random leaves are really an exhaustive sweep.
        const UNIVERSE_SIZE: u32 = 4;

        fn all_subsets() -> Vec<DomainLeaf> {
            let mut out = Vec::new();
            for mask in 0..(1u32 << UNIVERSE_SIZE) {
                let mut set = BTreeSet::new();
                for bit in 0..UNIVERSE_SIZE {
                    if mask & (1 << bit) != 0 {
                        set.insert(bit);
                    }
                }
                out.push(DomainLeaf(set));
            }
            out
        }

        fn domain() -> impl Iterator<Item = u32> {
            0..UNIVERSE_SIZE
        }

        #[test]
        fn union_agrees_with_plain_set_union_across_all_three_representations() {
            for a in all_subsets() {
                for b in all_subsets() {
                    let simple = AstSet::from_leaf(a.clone()) | AstSet::from_leaf(b.clone());
                    let smart = SmartAstSet::from_leaf(a.clone()) | SmartAstSet::from_leaf(b.clone());
                    let atoms = DisjointAtomsSet::from_atom(a.clone()) | DisjointAtomsSet::from_atom(b.clone());
                    for x in domain() {
                        let expected = a.contains(&x) || b.contains(&x);
                        assert_eq!(simple.contains(&x), expected, "ast_simple union mismatch at {x} for {a:?} | {b:?}");
                        assert_eq!(smart.contains(&x), expected, "ast_smart union mismatch at {x} for {a:?} | {b:?}");
                        assert_eq!(atoms.contains(&x), expected, "indipendent_atoms union mismatch at {x} for {a:?} | {b:?}");
                    }
                }
            }
        }

        #[test]
        fn intersection_agrees_with_plain_set_intersection_across_all_three_representations() {
            for a in all_subsets() {
                for b in all_subsets() {
                    let simple = AstSet::from_leaf(a.clone()) & AstSet::from_leaf(b.clone());
                    let smart = SmartAstSet::from_leaf(a.clone()) & SmartAstSet::from_leaf(b.clone());
                    let atoms = DisjointAtomsSet::from_atom(a.clone()) & DisjointAtomsSet::from_atom(b.clone());
                    for x in domain() {
                        let expected = a.contains(&x) && b.contains(&x);
                        assert_eq!(simple.contains(&x), expected, "ast_simple intersection mismatch at {x} for {a:?} & {b:?}");
                        assert_eq!(smart.contains(&x), expected, "ast_smart intersection mismatch at {x} for {a:?} & {b:?}");
                        assert_eq!(atoms.contains(&x), expected, "indipendent_atoms intersection mismatch at {x} for {a:?} & {b:?}");
                    }
                }
            }
        }

        #[test]
        fn difference_agrees_with_plain_set_difference_across_all_three_representations() {
            for a in all_subsets() {
                for b in all_subsets() {
                    let simple = AstSet::from_leaf(a.clone()) / AstSet::from_leaf(b.clone());
                    let smart = SmartAstSet::from_leaf(a.clone()) / SmartAstSet::from_leaf(b.clone());
                    let atoms = DisjointAtomsSet::from_atom(a.clone()) / DisjointAtomsSet::from_atom(b.clone());
                    for x in domain() {
                        let expected = a.contains(&x) && !b.contains(&x);
                        assert_eq!(simple.contains(&x), expected, "ast_simple difference mismatch at {x} for {a:?} / {b:?}");
                        assert_eq!(smart.contains(&x), expected, "ast_smart difference mismatch at {x} for {a:?} / {b:?}");
                        assert_eq!(atoms.contains(&x), expected, "indipendent_atoms difference mismatch at {x} for {a:?} / {b:?}");
                    }
                }
            }
        }

        #[test]
        fn compound_expression_agrees_across_all_three_representations() {
            let a = DomainLeaf::new([0, 1, 2]);
            let b = DomainLeaf::new([2, 3]);
            let c = DomainLeaf::new([0, 1, 2, 3]);
            let d = DomainLeaf::new([1]);

            // (a | b) & (c / d) -- an arbitrary but nontrivial expression mixing all
            // three operators, built independently in each representation.
            let simple = (AstSet::from_leaf(a.clone()) | AstSet::from_leaf(b.clone()))
                & (AstSet::from_leaf(c.clone()) / AstSet::from_leaf(d.clone()));
            let smart = (SmartAstSet::from_leaf(a.clone()) | SmartAstSet::from_leaf(b.clone()))
                & (SmartAstSet::from_leaf(c.clone()) / SmartAstSet::from_leaf(d.clone()));
            let atoms = (DisjointAtomsSet::from_atom(a.clone()) | DisjointAtomsSet::from_atom(b.clone()))
                & (DisjointAtomsSet::from_atom(c.clone()) / DisjointAtomsSet::from_atom(d.clone()));

            for x in domain() {
                let expected = (a.contains(&x) || b.contains(&x)) && (c.contains(&x) && !d.contains(&x));
                assert_eq!(simple.contains(&x), expected, "ast_simple compound expression mismatch at {x}");
                assert_eq!(smart.contains(&x), expected, "ast_smart compound expression mismatch at {x}");
                assert_eq!(atoms.contains(&x), expected, "indipendent_atoms compound expression mismatch at {x}");
            }
        }
    }
}
