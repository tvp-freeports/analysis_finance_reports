//! AST di insiemi non normalizzato (`AstSet`/`AstNode`).
//!
//! Rappresentazione più semplice dell'algebra condivisa in `commons::sets`: ogni `|`/`&`/`/`
//! costruisce un nuovo nodo `Branch`, senza mai semplificare l'albero (a differenza di
//! `ast_smart`). `AstSet` implementa `UncomparableSet`: due `AstSet` non normalizzati non sono
//! confrontabili in generale (serve `ast_smart` o `indipendent_atoms` per quello).

use super::{Container, SetAlgebra, SetOps, UncomparableSet};
use std::ops::{BitAnd, BitOr, Div};

pub enum AstNode<L, E>
where
    L: Container<Elem = E>,
    E: ?Sized,
{
    Leaf(L),
    Branch(Box<AstNode<L, E>>, SetOps, Box<AstNode<L, E>>),
}

impl<L, E> Clone for AstNode<L, E>
where
    L: Container<Elem = E> + Clone,
    E: ?Sized,
{
    fn clone(&self) -> Self {
        match self {
            Self::Leaf(l) => Self::Leaf(l.clone()),
            Self::Branch(a, ops, b) => Self::Branch(a.clone(), *ops, b.clone()),
        }
    }
}

impl<L, E> PartialEq<Self> for AstNode<L, E>
where
    L: Container<Elem = E> + PartialEq,
    E: ?Sized,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Leaf(a), Self::Leaf(b)) => a == b,
            (Self::Branch(box_a0, op_a, box_a1), Self::Branch(box_b0, op_b, box_b1)) => {
                op_a == op_b && box_a0 == box_b0 && box_a1 == box_b1
            }
            _ => false,
        }
    }
}

pub struct AstSet<L, E>(AstNode<L, E>)
where
    L: Container<Elem = E>,
    E: ?Sized;

impl<L, E> Clone for AstSet<L, E>
where
    L: Container<Elem = E> + Clone,
    E: ?Sized,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<L, E> AstSet<L, E>
where
    L: Container<Elem = E>,
    E: ?Sized,
{
    pub fn from_leaf(leaf: L) -> Self {
        Self(AstNode::Leaf(leaf))
    }
    pub fn ast(&self) -> &AstNode<L, E> {
        &self.0
    }
}

impl<L, E> Container for AstNode<L, E>
where
    L: Container<Elem = E>,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self, ele: &E) -> bool {
        match self {
            Self::Leaf(leaf) => leaf.contains(ele),
            Self::Branch(box_x, op, box_y) => op.call(box_x.contains(ele), box_y.contains(ele)),
        }
    }
}

impl<L, E> BitOr<Self> for AstSet<L, E>
where
    L: Container<Elem = E>,
    E: ?Sized,
{
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(AstNode::Branch(Box::new(self.0), SetOps::Union, Box::new(rhs.0)))
    }
}

impl<L, E> BitAnd<Self> for AstSet<L, E>
where
    L: Container<Elem = E>,
    E: ?Sized,
{
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(AstNode::Branch(Box::new(self.0), SetOps::Inter, Box::new(rhs.0)))
    }
}

impl<L, E> Div<Self> for AstSet<L, E>
where
    L: Container<Elem = E>,
    E: ?Sized,
{
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self(AstNode::Branch(Box::new(self.0), SetOps::Sub, Box::new(rhs.0)))
    }
}

impl<L, E> Container for AstSet<L, E>
where
    L: Container<Elem = E>,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self, txt: &E) -> bool {
        let Self(root_node) = self;
        root_node.contains(txt)
    }
}

impl<L, E> SetAlgebra for AstSet<L, E>
where
    L: Container<Elem = E>,
    E: ?Sized,
{
}

impl<L, E> UncomparableSet<E> for AstSet<L, E>
where
    L: Container<Elem = E>,
    E: ?Sized,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    impl Container for HashSet<String> {
        type Elem = str;
        fn contains(&self, txt: &str) -> bool {
            HashSet::contains(self, txt)
        }
    }

    type TestSet = AstSet<HashSet<String>, str>;
    type TestNode = AstNode<HashSet<String>, str>;

    impl TestSet {
        fn new<const N: usize>(words: [&str; N]) -> Self {
            Self(AstNode::Leaf(HashSet::from(words.map(|s| s.to_string()))))
        }
    }

    /// Extracts the leaf's words for assertions, panicking (test-only) if the node
    /// is not a leaf — the test's own contract, not the module's.
    fn leaf_words(node: &TestNode) -> HashSet<String> {
        match node {
            AstNode::Leaf(l) => l.clone(),
            AstNode::Branch(..) => panic!("expected a leaf node"),
        }
    }

    mod construction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn from_leaf_wraps_the_leaf_unchanged() {
            let leaf = HashSet::from(["nilpo".to_string(), "grummo".to_string()]);
            let s = TestSet::from_leaf(leaf.clone());
            assert_eq!(leaf_words(s.ast()), leaf);
        }

        #[test]
        fn ast_exposes_the_underlying_tree() {
            let s = TestSet::new(["a"]);
            match s.ast() {
                AstNode::Leaf(_) => (),
                AstNode::Branch(..) => panic!("a freshly-built leaf set must be a Leaf node"),
            }
        }
    }

    mod ast_creation {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn union_builds_a_branch_with_union_op_and_both_leaves_preserved() {
            let a = TestSet::new(["cave", "ghino"]);
            let b = TestSet::new(["canem", "sunnia"]);
            let (a_words, b_words) = (leaf_words(a.ast()), leaf_words(b.ast()));

            let AstSet(AstNode::Branch(x, op, y)) = a | b else {
                panic!("union must build a Branch node")
            };
            assert_eq!(op, SetOps::Union);
            assert_eq!(leaf_words(&x), a_words);
            assert_eq!(leaf_words(&y), b_words);
        }

        #[test]
        fn intersection_builds_a_branch_with_inter_op_and_both_leaves_preserved() {
            let a = TestSet::new(["cave", "ghino"]);
            let b = TestSet::new(["canem", "sunnia"]);
            let (a_words, b_words) = (leaf_words(a.ast()), leaf_words(b.ast()));

            let AstSet(AstNode::Branch(x, op, y)) = a & b else {
                panic!("intersection must build a Branch node")
            };
            assert_eq!(op, SetOps::Inter);
            assert_eq!(leaf_words(&x), a_words);
            assert_eq!(leaf_words(&y), b_words);
        }

        #[test]
        fn subtraction_builds_a_branch_with_sub_op_and_both_leaves_preserved() {
            let a = TestSet::new(["cave", "ghino"]);
            let b = TestSet::new(["canem", "sunnia"]);
            let (a_words, b_words) = (leaf_words(a.ast()), leaf_words(b.ast()));

            let AstSet(AstNode::Branch(x, op, y)) = a / b else {
                panic!("subtraction must build a Branch node")
            };
            assert_eq!(op, SetOps::Sub);
            assert_eq!(leaf_words(&x), a_words);
            assert_eq!(leaf_words(&y), b_words);
        }

        #[test]
        fn respects_bitor_bitand_div_precedence_when_mixed_in_one_expression() {
            // `&`/`|` bind looser than `/` in Rust's normal operator precedence, but
            // here all three are user-defined via `BitOr`/`BitAnd`/`Div`, whose
            // *actual* Rust precedence is: `/` (Div, a `MulDiv`-precedence op) binds
            // tighter than `&` (BitAnd), which binds tighter than `|` (BitOr). So
            // `a | (b / (c | d)) & (e / f)` parses as
            // `a | ( (b / (c|d)) & (e / f) )`.
            let a = TestSet::new(["A"]);
            let b = TestSet::new(["B"]);
            let c = TestSet::new(["C"]);
            let d = TestSet::new(["D"]);
            let e = TestSet::new(["E"]);
            let f = TestSet::new(["F"]);

            let g = a | (b.clone() / (c.clone() | d.clone())) & (e.clone() / f.clone());

            let AstSet(AstNode::Branch(x0, op0, y0)) = g else {
                panic!("top level must be a Branch")
            };
            assert_eq!(op0, SetOps::Union);
            assert_eq!(leaf_words(&x0), leaf_words(TestSet::new(["A"]).ast()));

            let AstNode::Branch(x1, op1, y1) = *y0 else {
                panic!("right side of the top union must be a Branch")
            };
            assert_eq!(op1, SetOps::Inter);

            let AstNode::Branch(x2, op2, y2) = *x1 else {
                panic!("left side of the intersection must be a Branch (b / (c|d))")
            };
            assert_eq!(op2, SetOps::Sub);
            assert_eq!(leaf_words(&x2), leaf_words(b.ast()));

            let AstNode::Branch(x3, op3, y3) = *y2 else {
                panic!("right side of the subtraction must be a Branch (c|d)")
            };
            assert_eq!(op3, SetOps::Union);
            assert_eq!(leaf_words(&x3), leaf_words(c.ast()));
            assert_eq!(leaf_words(&y3), leaf_words(d.ast()));

            let AstNode::Branch(x4, op4, y4) = *y1 else {
                panic!("right side of the intersection must be a Branch (e/f)")
            };
            assert_eq!(op4, SetOps::Sub);
            assert_eq!(leaf_words(&x4), leaf_words(e.ast()));
            assert_eq!(leaf_words(&y4), leaf_words(f.ast()));
        }
    }

    mod containment {
        use super::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(TestSet::new(["liquore", "text", "kkk"]), "text", true; "leaf hit")]
        #[test_case(TestSet::new(["liquore", "text", "kkk"]), "gulm", false; "leaf miss")]
        #[test_case(TestSet::new(["niluk"]) | TestSet::new(["jukonne si"]), "jukonne si", true; "union hit on either side")]
        #[test_case(TestSet::new(["niluk"]) | TestSet::new(["jukonne si"]), "jukonne no", false; "union miss on both sides")]
        #[test_case(TestSet::new(["grum", "jukonne", "litro"]) & TestSet::new(["nespo", "jukonne"]), "jukonne", true; "intersection hit on both sides")]
        #[test_case(TestSet::new(["grum", "jukonne", "litro"]) & TestSet::new(["nespo", "jukonne"]), "grum", false; "intersection miss when only one side has it")]
        #[test_case(TestSet::new(["text that has to be jukonne"]) / TestSet::new(["jukone"]), "text that has to be jukonne", true; "subtraction hit when not in subtrahend")]
        #[test_case(TestSet::new(["jukone", "grummo"]) / TestSet::new(["piffo", "jukone"]), "jukone", false; "subtraction miss when also in subtrahend")]
        fn contains_matches_expectation(txt_set: TestSet, txt: &str, expected: bool) {
            assert_eq!(txt_set.contains(txt), expected);
        }
    }

    /// PLAN.md §10 asks for algebraic invariants to be checked over a small,
    /// exhaustively-enumerated universe rather than by randomization.
    mod algebra_invariants {
        use super::*;
        use pretty_assertions::assert_eq;

        const DOMAIN: [&str; 4] = ["a", "b", "c", "d"];

        fn all_subsets() -> Vec<TestSet> {
            let mut out = Vec::new();
            for mask in 0..(1u32 << DOMAIN.len()) {
                let mut words = HashSet::new();
                for (bit, word) in DOMAIN.iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        words.insert(word.to_string());
                    }
                }
                out.push(AstSet(AstNode::Leaf(words)));
            }
            out
        }

        fn contains_profile(s: &TestSet) -> Vec<bool> {
            DOMAIN.iter().map(|w| s.contains(w)).collect()
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
            // There's no explicit "complement" operator in this algebra, but
            // relative complement against a leaf containing the whole domain
            // (`universe / x`) plays that role for the purposes of De Morgan's law.
            let universe = || AstSet(AstNode::Leaf(HashSet::from(DOMAIN.map(|w| w.to_string()))));
            let complement = |s: TestSet| universe() / s;

            for a in all_subsets() {
                for b in all_subsets() {
                    // not(a | b) == not(a) & not(b)
                    let lhs = complement(a.clone() | b.clone());
                    let rhs = complement(a.clone()) & complement(b.clone());
                    assert_eq!(contains_profile(&lhs), contains_profile(&rhs));

                    // not(a & b) == not(a) | not(b)
                    let lhs = complement(a.clone() & b.clone());
                    let rhs = complement(a.clone()) | complement(b.clone());
                    assert_eq!(contains_profile(&lhs), contains_profile(&rhs));
                }
            }
        }

        #[test]
        fn subtracting_a_set_from_itself_contains_nothing_in_the_domain() {
            for a in all_subsets() {
                let empty_ish = a.clone() / a;
                assert!(DOMAIN.iter().all(|w| !empty_ish.contains(w)));
            }
        }
    }

    /// The `AstSet`/`AstNode` pair must keep satisfying the shared vocabulary
    /// (`commons::sets`): it is unnormalized, so it is `UncomparableSet`, never
    /// `ComparableSet` (that upgrade belongs to `ast_smart`/`indipendent_atoms`).
    mod trait_conformance {
        use super::*;

        fn assert_uncomparable_set<S: UncomparableSet<E>, E: ?Sized>() {}

        #[test]
        fn ast_set_is_an_uncomparable_set() {
            assert_uncomparable_set::<TestSet, str>();
        }
    }
}
