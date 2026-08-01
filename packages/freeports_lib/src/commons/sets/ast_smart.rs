use super::{Container,SetRelation,SetOps,UncomparableSet,SetAlgebra,Overlappable};
use std::ops::{BitOr, BitAnd, Div};

#[derive(Debug)]
pub enum SmartAstNode<L,E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    Leaf(L),
    EmptySet,
    Branch(Box<SmartAstNode<L,E>>, SetOps, Box<SmartAstNode<L,E>>)
}


impl<L,E> PartialEq<Self> for SmartAstNode<L,E>
where
    L: Container<Elem = E> + PartialEq + Clone + Overlappable<L>,
    E: ?Sized,
{
    fn eq(&self, other: &Self) -> bool {
        match (self,other) {
            (Self::EmptySet,Self::EmptySet) => true,
            (Self::Leaf(a),Self::Leaf(b)) => a == b,
            (
                Self::Branch(box_a0,op_a,box_a1),
                Self::Branch(box_b0,op_b,box_b1)
            ) => op_a == op_b && box_a0 == box_b0 && box_a1 == box_b1,
            _ => false
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
            SmartAstNode::Branch(left, op, right) => SmartAstNode::Branch(
                left.clone(),
                op.clone(),
                right.clone(),
            ),
        }
    }
}

#[derive(Debug)]
pub struct SmartAstSet<L,E>(SmartAstNode<L,E>)
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized
;

impl<L,E> Clone for SmartAstSet<L,E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<L,E> SmartAstSet<L,E> 
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized
{
    pub fn from_leaf(leaf: L) -> Self {
        Self(SmartAstNode::Leaf(leaf))
    }
    pub fn empty() -> Self {
        Self(SmartAstNode::EmptySet)
    }
    pub fn ast(&self) -> &SmartAstNode<L,E> {
        &self.0
    }
}


impl<L,E> Container for SmartAstNode<L,E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self,ele: &E) -> bool {
        match self {
            Self::EmptySet => false,
            Self::Leaf(leaf) => leaf.contains(ele),
            Self::Branch(box_x,op,box_y) => op.call(
                box_x.contains(ele),box_y.contains(ele)
            )
        }
    }
}

impl<L,E> BitOr<Self> for SmartAstSet<L,E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        use SetRelation::*;
        match (self.0,rhs.0) {
            (SmartAstNode::Leaf(a),SmartAstNode::Leaf(b)) => {
                match a.set_relation(&b) {
                    Equal | Superset => Self(SmartAstNode::Leaf(a)),
                    Subset => Self(SmartAstNode::Leaf(b)),
                    Overlapping | Disjoint => Self(SmartAstNode::Branch(
                        Box::new(SmartAstNode::Leaf(a)),
                        SetOps::Union,
                        Box::new(SmartAstNode::Leaf(b))
                    ))
                }
            },
            (left_node,right_node) => Self(SmartAstNode::Branch(
                Box::new(left_node),
                SetOps::Union,
                Box::new(right_node)
            ))
        }
    }
}
impl<L,E> BitAnd<Self> for SmartAstSet<L,E> 
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        use SetRelation::*;
        match (self.0,rhs.0) {
            (SmartAstNode::Leaf(a),SmartAstNode::Leaf(b)) => {
                match a.set_relation(&b) {
                    Equal | Subset => Self(SmartAstNode::Leaf(a)),
                    Superset => Self(SmartAstNode::Leaf(b)),
                    Disjoint => Self(SmartAstNode::EmptySet),
                    Overlapping => Self(SmartAstNode::Branch(
                        Box::new(SmartAstNode::Leaf(a)),
                        SetOps::Inter,
                        Box::new(SmartAstNode::Leaf(b))
                    ))
                }
            },
            (left_node,right_node) => Self(SmartAstNode::Branch(
                Box::new(left_node),
                SetOps::Inter,
                Box::new(right_node)
            ))
        }
    }
    
}
impl<L,E> Div<Self> for SmartAstSet<L,E> 
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        use SetRelation::*;
        match (self.0,rhs.0) {
            (SmartAstNode::Leaf(a),SmartAstNode::Leaf(b)) => {
                match a.set_relation(&b) {
                    Equal | Subset => Self(SmartAstNode::EmptySet),
                    Disjoint => Self(SmartAstNode::Leaf(a)),
                    Superset | Overlapping => Self(SmartAstNode::Branch(
                        Box::new(SmartAstNode::Leaf(a)),
                        SetOps::Sub,
                        Box::new(SmartAstNode::Leaf(b))
                    ))
                }
            },
            (left_node,right_node) => Self(SmartAstNode::Branch(
                Box::new(left_node),
                SetOps::Sub,
                Box::new(right_node)
            ))
        }
    }
}


impl<L,E> Container for SmartAstSet<L,E> 
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self,txt: &E) -> bool {
        self.0.contains(txt)
    }
}


impl<L,E> SetAlgebra for SmartAstSet<L,E> 
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized
{}

impl<L,E> UncomparableSet<E> for SmartAstSet<L,E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized
{}


#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    use pretty_assertions::assert_eq;
    use std::collections::HashSet;
    #[derive(Clone,Debug,PartialEq)]
    struct TestSmartLeaf(HashSet<u32>);
    impl Container for TestSmartLeaf {
        type Elem = u32;
        fn contains(&self,n: &u32) -> bool {
            self.0.contains(n)
        }
    }
    type TestSet = SmartAstSet<TestSmartLeaf,u32>;
    type TestNode = SmartAstNode<TestSmartLeaf,u32>;
    impl TestSet {
        fn new<const N: usize>(vec: [u32; N]) -> Self {
            Self(SmartAstNode::Leaf(
                TestSmartLeaf(HashSet::from(vec))
            ))
        }
    }
    impl Overlappable<Self> for TestSmartLeaf {
        fn set_relation(&self, other: &Self) -> SetRelation {
            use SetRelation::*;
            if self==other {
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
    #[test]
    fn new() {
        let l = TestSmartLeaf(HashSet::from([3,4,5]));
        let s = TestSet::from_leaf(l.clone());
        match s {
            SmartAstSet(SmartAstNode::Leaf(lf)) => assert_eq!(l,lf),
            _ => panic!("AstSet doesn't have expected shape")
        }
    }
    use SetOps::*;
    #[test_case(
        TestSet::new([1,2,3,4]),Union,TestSet::new([1,2,3,4]),
        TestNode::Leaf(TestSmartLeaf(HashSet::from([1,2,3,4])));"union equal"
    )]
    #[test_case(
        TestSet::new([2,3]),Union,TestSet::new([1,2,3,4]),
        TestNode::Leaf(TestSmartLeaf(HashSet::from([1,2,3,4])));"union subset"
    )]
    #[test_case(
        TestSet::new([1,2,3,4,5]),Union,TestSet::new([1,2,3,4]),
        TestNode::Leaf(TestSmartLeaf(HashSet::from([1,2,3,4,5])));"union superset"
    )]
    #[test_case(
        TestSet::new([1,2]),Union,TestSet::new([4,5,6]),
        TestNode::Branch(
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([1,2])))),
            Union,
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([4,5,6]))))
        );"union disjoint"
    )]
    #[test_case(
        TestSet::new([1,2,3,4,5]),Union,TestSet::new([4,5,6]),
        TestNode::Branch(
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([1,2,3,4,5])))),
            Union,
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([4,5,6]))))
        );"union overlapping"
    )]
    #[test_case(
        TestSet::new([3,4,50]),Inter,TestSet::new([3,4,50]),
        TestNode::Leaf(TestSmartLeaf(HashSet::from([3,4,50])));"intersect equal"
    )]
    #[test_case(
        TestSet::new([2,3]),Inter,TestSet::new([1,2,3,4]),
        TestNode::Leaf(TestSmartLeaf(HashSet::from([2,3])));"intersect subset"
    )]
    #[test_case(
        TestSet::new([1,2,3,4,5]),Inter,TestSet::new([2,3,4]),
        TestNode::Leaf(TestSmartLeaf(HashSet::from([2,3,4])));"intersect superset"
    )]
    #[test_case(
        TestSet::new([1,2]),Inter,TestSet::new([4,5,6]),
        TestNode::EmptySet;"intersect disjoint"
    )]
    #[test_case(
        TestSet::new([3,4,5]),Inter,TestSet::new([4,5,6]),
        TestNode::Branch(
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([3,4,5])))),
            Inter,
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([4,5,6]))))
        );"intersect overlapping"
    )]
    #[test_case(
        TestSet::new([3,4,50]),Sub,TestSet::new([3,4,50]),
        TestNode::EmptySet;"subtract equal"
    )]
    #[test_case(
        TestSet::new([2,3]),Sub,TestSet::new([1,2,3,4]),
        TestNode::EmptySet;"subtract subset"
    )]
    #[test_case(
        TestSet::new([1,2,3,4,5]),Sub,TestSet::new([2,3,4]),
        TestNode::Branch(
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([1,2,3,4,5])))),
            Sub,
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([2,3,4]))))
        );"subtract superset"
    )]
    #[test_case(
        TestSet::new([1,2]),Sub,TestSet::new([4,5,6]),
        TestNode::Leaf(TestSmartLeaf(HashSet::from([1,2])));"subtract disjoint"
    )]
    #[test_case(
        TestSet::new([3,4,5,70]),Sub,TestSet::new([4,5,60]),
        TestNode::Branch(
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([3,4,5,70])))),
            Sub,
            Box::new(TestNode::Leaf(TestSmartLeaf(HashSet::from([4,5,60]))))
        );"subtract overlapping"
    )]
    fn ast_creation(a: TestSet, op: SetOps, b: TestSet,expected: TestNode) {
        use SetOps::*;
        let c = match op {
            Union => a | b,
            Inter => a & b,
            Sub => a / b
        };
        match (c.0,expected) {
            (
                SmartAstNode::Branch(box_x,op,box_y),
                SmartAstNode::Branch(exp_x,exp_op,exp_y)
            ) => {
                assert_eq!(op,exp_op);
                assert_eq!(box_x,exp_x);
                assert_eq!(box_y,exp_y);      
            },
            (
                SmartAstNode::Leaf(res),
                SmartAstNode::Leaf(exp)
            ) => {
                assert_eq!(res,exp);
            },
            (SmartAstNode::EmptySet,SmartAstNode::EmptySet) => (),
            _ => panic!("unexpected set structure")
        }
    }

    #[test]
    fn ast_creation_expression() {
        let a = TestSet::new([1]);
        let SmartAstSet(SmartAstNode::Leaf(a_leaf)) = a.clone() else {
            panic!("unexpected set structure")
        };
        let b = TestSet::new([2]);
        let SmartAstSet(SmartAstNode::Leaf(b_leaf)) = b.clone() else {
            panic!("unexpected set structure")
        };
        let c = TestSet::new([3,4]);
        let d = TestSet::new([4]);
        let e = TestSet::new([2,5]);
        let f = TestSet::new([6]);
        let g = a | (b / (c | d)) & (e / f);
        match g {
            SmartAstSet(SmartAstNode::Branch(
                box_x0,
                op0,
                box_y0
            )) => {
                assert_eq!(op0,SetOps::Union);
                let SmartAstNode::Leaf(should_a) = *box_x0 else {
                    panic!("unexpected node structure")
                };
                assert_eq!(should_a,a_leaf);
                let SmartAstNode::Leaf(should_b) = *box_y0 else {
                    panic!("unexpected node structure")
                };
                assert_eq!(should_a,a_leaf);
                assert_eq!(should_b,b_leaf);
            },
            _ => panic!("Ast structured different from the one expected")

        }
    }

    #[test_case(
        TestSet::new([4,5,70]),
        70;
        "simple"
    )]
    #[test_case(
        TestSet::new([4,6,7]) | TestSet::new([10,60,8,7]),
        60;
        "union"
    )]
    #[test_case(
        TestSet::new([30,50,60,70]) & TestSet::new([60,70,80]),
        60;
        "intersect"
    )]
    #[test_case(
        TestSet::new([2,4,6,8]) / TestSet::new([4,6]),
        8;
        "subtraction"
    )]
    #[test_case(
        TestSet::new([6]) | (TestSet::new([3,89]) & TestSet::new([56,67,89]) / TestSet::new([67,78])),
        89;
        "expression"
    )]
    fn element_in_set(test_set: TestSet, n: u32){
        assert!(test_set.contains(&n));
    }



    #[test_case(
        TestSet::new([4,5,70]),
        71;
        "simple"
    )]
    #[test_case(
        TestSet::new([4,6,7]) | TestSet::new([10,60,8,7]),
        603;
        "union"
    )]
    #[test_case(
        TestSet::new([30,50,60,70]) & TestSet::new([60,70,80]),
        80;
        "intersect"
    )]
    #[test_case(
        TestSet::new([2,4,6,8]) / TestSet::new([4,6]),
        4;
        "subtraction"
    )]
    #[test_case(
        TestSet::new([6]) | (TestSet::new([3,89,67]) & TestSet::new([56,67,89]) / TestSet::new([67,78])),
        67;
        "expression"
    )]
    fn element_not_in_set(test_set: TestSet, n: u32){
        assert!(!test_set.contains(&n));
    }
}
