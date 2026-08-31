//! A simplifying set expression tree: [`SmartAstSet`] and [`SmartAstNode`].
//!
//! The same algebra as [`super::ast_simple`], but every operator between two leaves tries to
//! simplify immediately using their set relation: equal, subset and superset collapse into a single
//! leaf, while disjoint and overlapping stay a branch.
//!
//! It is still [`UncomparableSet`]: simplification is local to pairs of leaves and does not
//! normalise the whole tree, so two trees built differently can still denote the same set without
//! comparing equal.

use super::{Container, Overlappable, SetAlgebra, SetOps, SetRelation, UncomparableSet};
use std::ops::{BitAnd, BitOr, Div};

#[derive(Debug)]
pub enum SmartAstNode<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    Leaf(L),
    EmptySet,
    Branch(Box<SmartAstNode<L, E>>, SetOps, Box<SmartAstNode<L, E>>),
}

impl<L, E> PartialEq<Self> for SmartAstNode<L, E>
where
    L: Container<Elem = E> + PartialEq + Clone + Overlappable<L>,
    E: ?Sized,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::EmptySet, Self::EmptySet) => true,
            (Self::Leaf(a), Self::Leaf(b)) => a == b,
            (Self::Branch(box_a0, op_a, box_a1), Self::Branch(box_b0, op_b, box_b1)) => {
                op_a == op_b && box_a0 == box_b0 && box_a1 == box_b1
            }
            _ => false,
        }
    }
}

impl<L, E> Clone for SmartAstNode<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    fn clone(&self) -> Self {
        match self {
            SmartAstNode::Leaf(l) => SmartAstNode::Leaf(l.clone()),
            SmartAstNode::EmptySet => SmartAstNode::EmptySet,
            SmartAstNode::Branch(left, op, right) => SmartAstNode::Branch(left.clone(), *op, right.clone()),
        }
    }
}

#[derive(Debug)]
pub struct SmartAstSet<L, E>(SmartAstNode<L, E>)
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized;

impl<L, E> Clone for SmartAstSet<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<L, E> SmartAstSet<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    pub fn from_leaf(leaf: L) -> Self {
        Self(SmartAstNode::Leaf(leaf))
    }
    pub fn empty() -> Self {
        Self(SmartAstNode::EmptySet)
    }
    pub fn ast(&self) -> &SmartAstNode<L, E> {
        &self.0
    }
}

impl<L, E> Container for SmartAstNode<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self, ele: &E) -> bool {
        match self {
            Self::EmptySet => false,
            Self::Leaf(leaf) => leaf.contains(ele),
            Self::Branch(box_x, op, box_y) => op.call(box_x.contains(ele), box_y.contains(ele)),
        }
    }
}

impl<L, E> BitOr<Self> for SmartAstSet<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        use SetRelation::*;
        match (self.0, rhs.0) {
            (SmartAstNode::Leaf(a), SmartAstNode::Leaf(b)) => match a.set_relation(&b) {
                Equal | Superset => Self(SmartAstNode::Leaf(a)),
                Subset => Self(SmartAstNode::Leaf(b)),
                Overlapping | Disjoint => Self(SmartAstNode::Branch(
                    Box::new(SmartAstNode::Leaf(a)),
                    SetOps::Union,
                    Box::new(SmartAstNode::Leaf(b)),
                )),
            },
            (left_node, right_node) => {
                Self(SmartAstNode::Branch(Box::new(left_node), SetOps::Union, Box::new(right_node)))
            }
        }
    }
}

impl<L, E> BitAnd<Self> for SmartAstSet<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        use SetRelation::*;
        match (self.0, rhs.0) {
            (SmartAstNode::Leaf(a), SmartAstNode::Leaf(b)) => match a.set_relation(&b) {
                Equal | Subset => Self(SmartAstNode::Leaf(a)),
                Superset => Self(SmartAstNode::Leaf(b)),
                Disjoint => Self(SmartAstNode::EmptySet),
                Overlapping => Self(SmartAstNode::Branch(
                    Box::new(SmartAstNode::Leaf(a)),
                    SetOps::Inter,
                    Box::new(SmartAstNode::Leaf(b)),
                )),
            },
            (left_node, right_node) => {
                Self(SmartAstNode::Branch(Box::new(left_node), SetOps::Inter, Box::new(right_node)))
            }
        }
    }
}

impl<L, E> Div<Self> for SmartAstSet<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        use SetRelation::*;
        match (self.0, rhs.0) {
            (SmartAstNode::Leaf(a), SmartAstNode::Leaf(b)) => match a.set_relation(&b) {
                Equal | Subset => Self(SmartAstNode::EmptySet),
                Disjoint => Self(SmartAstNode::Leaf(a)),
                Superset | Overlapping => Self(SmartAstNode::Branch(
                    Box::new(SmartAstNode::Leaf(a)),
                    SetOps::Sub,
                    Box::new(SmartAstNode::Leaf(b)),
                )),
            },
            (left_node, right_node) => {
                Self(SmartAstNode::Branch(Box::new(left_node), SetOps::Sub, Box::new(right_node)))
            }
        }
    }
}

impl<L, E> Container for SmartAstSet<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self, txt: &E) -> bool {
        self.0.contains(txt)
    }
}

impl<L, E> SetAlgebra for SmartAstSet<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
}

impl<L, E> UncomparableSet<E> for SmartAstSet<L, E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct TestSmartLeaf(HashSet<u32>);

    impl TestSmartLeaf {
        fn new<const N: usize>(items: [u32; N]) -> Self {
            Self(HashSet::from(items))
        }
    }

    impl Container for TestSmartLeaf {
        type Elem = u32;
        fn contains(&self, n: &u32) -> bool {
            self.0.contains(n)
        }
    }

    impl Overlappable<Self> for TestSmartLeaf {
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

    type TestSet = SmartAstSet<TestSmartLeaf, u32>;
    type TestNode = SmartAstNode<TestSmartLeaf, u32>;

    impl TestSet {
        fn new<const N: usize>(items: [u32; N]) -> Self {
            Self(SmartAstNode::Leaf(TestSmartLeaf::new(items)))
        }
    }

    /// The shape a resulting node is expected to take after a leaf/leaf operation:
    /// either it collapsed to a single leaf/empty set (simplified away), or it stayed
    /// a `Branch` (nothing to simplify, e.g. two genuinely overlapping/disjoint sets).
    #[derive(Debug, PartialEq)]
    enum Shape {
        Leaf(Vec<u32>),
        EmptySet,
        Branch(Vec<u32>, SetOps, Vec<u32>),
    }

    fn shape_of(node: &TestNode) -> Shape {
        match node {
            TestNode::EmptySet => Shape::EmptySet,
            TestNode::Leaf(TestSmartLeaf(items)) => {
                let mut v: Vec<u32> = items.iter().copied().collect();
                v.sort();
                Shape::Leaf(v)
            }
            TestNode::Branch(x, op, y) => {
                let TestNode::Leaf(TestSmartLeaf(xs)) = x.as_ref() else {
                    panic!("test only builds leaf/leaf branches")
                };
                let TestNode::Leaf(TestSmartLeaf(ys)) = y.as_ref() else {
                    panic!("test only builds leaf/leaf branches")
                };
                let mut xs: Vec<u32> = xs.iter().copied().collect();
                xs.sort();
                let mut ys: Vec<u32> = ys.iter().copied().collect();
                ys.sort();
                Shape::Branch(xs, *op, ys)
            }
        }
    }

    mod construction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn from_leaf_wraps_the_leaf_unchanged() {
            let leaf = TestSmartLeaf::new([3, 4, 5]);
            let s = TestSet::from_leaf(leaf.clone());
            assert_eq!(shape_of(s.ast()), Shape::Leaf(vec![3, 4, 5]));
        }

        #[test]
        fn empty_builds_the_empty_set_variant() {
            let s: TestSet = TestSet::empty();
            assert_eq!(shape_of(s.ast()), Shape::EmptySet);
        }
    }

    /// Exhaustive over every `SetRelation` variant, for every operator: this is
    /// exactly the extra behavior `ast_smart` adds on top of `ast_simple` (it
    /// simplifies leaf/leaf operations using `Overlappable::set_relation` instead of
    /// always building a `Branch`).
    mod simplification_on_union {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;
        use SetOps::Union;

        #[test_case(TestSet::new([1,2,3,4]), TestSet::new([1,2,3,4]), Shape::Leaf(vec![1,2,3,4]); "equal leaves collapse to one leaf")]
        #[test_case(TestSet::new([2,3]), TestSet::new([1,2,3,4]), Shape::Leaf(vec![1,2,3,4]); "subset collapses to the superset leaf")]
        #[test_case(TestSet::new([1,2,3,4,5]), TestSet::new([1,2,3,4]), Shape::Leaf(vec![1,2,3,4,5]); "superset collapses to itself")]
        #[test_case(TestSet::new([1,2]), TestSet::new([4,5,6]), Shape::Branch(vec![1,2], Union, vec![4,5,6]); "disjoint leaves stay a branch")]
        #[test_case(TestSet::new([1,2,3,4,5]), TestSet::new([4,5,6]), Shape::Branch(vec![1,2,3,4,5], Union, vec![4,5,6]); "overlapping leaves stay a branch")]
        fn simplifies_according_to_set_relation(a: TestSet, b: TestSet, expected: Shape) {
            assert_eq!(shape_of((a | b).ast()), expected);
        }
    }

    mod simplification_on_intersection {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;
        use SetOps::Inter;

        #[test_case(TestSet::new([3,4,50]), TestSet::new([3,4,50]), Shape::Leaf(vec![3,4,50]); "equal leaves collapse to one leaf")]
        #[test_case(TestSet::new([2,3]), TestSet::new([1,2,3,4]), Shape::Leaf(vec![2,3]); "subset collapses to the subset leaf")]
        #[test_case(TestSet::new([1,2,3,4,5]), TestSet::new([2,3,4]), Shape::Leaf(vec![2,3,4]); "superset collapses to the smaller leaf")]
        #[test_case(TestSet::new([1,2]), TestSet::new([4,5,6]), Shape::EmptySet; "disjoint leaves collapse to the empty set")]
        #[test_case(TestSet::new([3,4,5]), TestSet::new([4,5,6]), Shape::Branch(vec![3,4,5], Inter, vec![4,5,6]); "overlapping leaves stay a branch")]
        fn simplifies_according_to_set_relation(a: TestSet, b: TestSet, expected: Shape) {
            assert_eq!(shape_of((a & b).ast()), expected);
        }
    }

    mod simplification_on_subtraction {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;
        use SetOps::Sub;

        #[test_case(TestSet::new([3,4,50]), TestSet::new([3,4,50]), Shape::EmptySet; "equal leaves collapse to the empty set")]
        #[test_case(TestSet::new([2,3]), TestSet::new([1,2,3,4]), Shape::EmptySet; "subset collapses to the empty set")]
        #[test_case(TestSet::new([1,2,3,4,5]), TestSet::new([2,3,4]), Shape::Branch(vec![1,2,3,4,5], Sub, vec![2,3,4]); "superset stays a branch (still needs the actual removal)")]
        #[test_case(TestSet::new([1,2]), TestSet::new([4,5,6]), Shape::Leaf(vec![1,2]); "disjoint collapses to the unchanged minuend")]
        #[test_case(TestSet::new([3,4,5,70]), TestSet::new([4,5,60]), Shape::Branch(vec![3,4,5,70], Sub, vec![4,5,60]); "overlapping leaves stay a branch")]
        fn simplifies_according_to_set_relation(a: TestSet, b: TestSet, expected: Shape) {
            assert_eq!(shape_of((a / b).ast()), expected);
        }
    }

    mod containment {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(TestSet::new([4,5,70]), 70, true; "leaf hit")]
        #[test_case(TestSet::new([4,5,70]), 71, false; "leaf miss")]
        #[test_case(TestSet::new([4,6,7]) | TestSet::new([10,60,8,7]), 60, true; "union hit on either side")]
        #[test_case(TestSet::new([4,6,7]) | TestSet::new([10,60,8,7]), 603, false; "union miss on both sides")]
        #[test_case(TestSet::new([30,50,60,70]) & TestSet::new([60,70,80]), 60, true; "intersection hit on both sides")]
        #[test_case(TestSet::new([30,50,60,70]) & TestSet::new([60,70,80]), 80, false; "intersection miss when only one side has it")]
        #[test_case(TestSet::new([2,4,6,8]) / TestSet::new([4,6]), 8, true; "subtraction hit when not in subtrahend")]
        #[test_case(TestSet::new([2,4,6,8]) / TestSet::new([4,6]), 4, false; "subtraction miss when also in subtrahend")]
        #[test_case(TestSet::empty(), 1, false; "empty set never contains anything")]
        fn contains_matches_expectation(s: TestSet, n: u32, expected: bool) {
            assert_eq!(s.contains(&n), expected);
        }
    }

    /// Same invariants as `ast_simple`'s, but here they must hold *and* the
    /// simplification must not change what the expression means (only how it's
    /// represented) — `contains` is the ground truth, not the tree shape.
    mod algebra_invariants {
        use super::*;
        use pretty_assertions::assert_eq;

        const DOMAIN: [u32; 4] = [0, 1, 2, 3];

        fn all_subsets() -> Vec<TestSet> {
            let mut out = Vec::new();
            for mask in 0..(1u32 << DOMAIN.len()) {
                let mut items = HashSet::new();
                for (bit, item) in DOMAIN.iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        items.insert(*item);
                    }
                }
                out.push(SmartAstSet(SmartAstNode::Leaf(TestSmartLeaf(items))));
            }
            out
        }

        fn contains_profile(s: &TestSet) -> Vec<bool> {
            DOMAIN.iter().map(|n| s.contains(n)).collect()
        }

        #[test]
        fn union_and_intersection_are_idempotent_for_every_leaf_in_the_domain() {
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
        fn de_morgan_holds_relative_to_a_full_universe_leaf() {
            let universe = || SmartAstSet(SmartAstNode::Leaf(TestSmartLeaf(HashSet::from(DOMAIN))));
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
        fn subtracting_a_set_from_itself_is_the_empty_set() {
            for a in all_subsets() {
                let diff = a.clone() / a;
                assert_eq!(shape_of(diff.ast()), Shape::EmptySet);
            }
        }
    }

    /// `ast_smart` stays `UncomparableSet`, same as `ast_simple`: simplification
    /// only collapses individual leaf/leaf operations, it does not give the whole
    /// tree an `Overlappable` implementation of its own.
    mod trait_conformance {
        use super::*;

        fn assert_uncomparable_set<S: UncomparableSet<E>, E: ?Sized>() {}

        #[test]
        fn smart_ast_set_is_an_uncomparable_set() {
            assert_uncomparable_set::<TestSet, u32>();
        }
    }
}
