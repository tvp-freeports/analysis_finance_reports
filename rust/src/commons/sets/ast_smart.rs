use super::{Container,SetRelation,SetOps,Set,SetAlgebra,Overlappable};
use std::ops::{BitOr, BitAnd, Div};

#[derive(Debug,PartialEq)]
enum SmartAstNode<L,E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    Leaf(L),
    EmptySet,
    Branch(Box<SmartAstNode<L,E>>, SetOps, Box<SmartAstNode<L,E>>)
}

#[derive(Debug)]
pub struct SmartAstSet<L,E>(SmartAstNode<L,E>)
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized
;

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

impl<L,E> Set<E> for SmartAstSet<L,E>
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
    impl Container for HashSet<u32> {
        type Elem = u32;
        fn contains(&self,n: &u32) -> bool {
            HashSet::contains(self,n)
        }
    }
    type TestSet = SmartAstSet<HashSet<u32>,u32>;
    type TestNode = SmartAstNode<HashSet<u32>,u32>;
    impl TestSet {
        fn new<const N: usize>(vec: [u32; N]) -> Self {
            Self(SmartAstNode::Leaf(
                HashSet::from(vec)
            ))
        }
    }
    impl Clone for TestSet {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl Clone for TestNode {
        fn clone(&self) -> Self {
            match self {
                Self::EmptySet => Self::EmptySet,
                Self::Leaf(a) => Self::Leaf(a.clone()),
                Self::Branch(box_a,op,box_b) => Self::Branch(
                    box_a.clone(),
                    *op,
                    box_b.clone()
                )
            }
        }  
    }
    impl Overlappable<Self> for HashSet<u32> {
        fn set_relation(&self, other: &Self) -> SetRelation {
            use SetRelation::*;
            if self==other {
                Equal
            } else if self.is_subset(other) {
                Subset
            } else if self.is_superset(other) {
                Superset
            } else if self.is_disjoint(other) {
                Disjoint
            } else {
                Overlapping
            }
        }
    }
    use SetOps::*;
    #[test_case(
        TestSet::new([1,2,3,4]),Union,TestSet::new([1,2,3,4]),
        TestNode::Leaf(HashSet::from([1,2,3,4]));"union equal"
    )]
    #[test_case(
        TestSet::new([2,3]),Union,TestSet::new([1,2,3,4]),
        TestNode::Leaf(HashSet::from([1,2,3,4]));"union subset"
    )]
    #[test_case(
        TestSet::new([1,2,3,4,5]),Union,TestSet::new([1,2,3,4]),
        TestNode::Leaf(HashSet::from([1,2,3,4,5]));"union superset"
    )]
    #[test_case(
        TestSet::new([1,2]),Union,TestSet::new([4,5,6]),
        TestNode::Branch(
            Box::new(TestNode::Leaf(HashSet::from([1,2]))),
            Union,
            Box::new(TestNode::Leaf(HashSet::from([4,5,6])))
        );"union disjoint"
    )]
    #[test_case(
        TestSet::new([1,2,3,4,5]),Union,TestSet::new([4,5,6]),
        TestNode::Branch(
            Box::new(TestNode::Leaf(HashSet::from([1,2,3,4,5]))),
            Union,
            Box::new(TestNode::Leaf(HashSet::from([4,5,6])))
        );"union overlapping"
    )]
    #[test_case(
        TestSet::new([3,4,50]),Inter,TestSet::new([3,4,50]),
        TestNode::Leaf(HashSet::from([3,4,50]));"intersect equal"
    )]
    #[test_case(
        TestSet::new([2,3]),Inter,TestSet::new([1,2,3,4]),
        TestNode::Leaf(HashSet::from([2,3]));"intersect subset"
    )]
    #[test_case(
        TestSet::new([1,2,3,4,5]),Inter,TestSet::new([2,3,4]),
        TestNode::Leaf(HashSet::from([2,3,4]));"intersect superset"
    )]
    #[test_case(
        TestSet::new([1,2]),Inter,TestSet::new([4,5,6]),
        TestNode::EmptySet;"intersect disjoint"
    )]
    #[test_case(
        TestSet::new([3,4,5]),Inter,TestSet::new([4,5,6]),
        TestNode::Branch(
            Box::new(TestNode::Leaf(HashSet::from([3,4,5]))),
            Inter,
            Box::new(TestNode::Leaf(HashSet::from([4,5,6])))
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
            Box::new(TestNode::Leaf(HashSet::from([1,2,3,4,5]))),
            Sub,
            Box::new(TestNode::Leaf(HashSet::from([2,3,4])))
        );"subtract superset"
    )]
    #[test_case(
        TestSet::new([1,2]),Sub,TestSet::new([4,5,6]),
        TestNode::Leaf(HashSet::from([1,2]));"subtract disjoint"
    )]
    #[test_case(
        TestSet::new([3,4,5,70]),Sub,TestSet::new([4,5,60]),
        TestNode::Branch(
            Box::new(TestNode::Leaf(HashSet::from([3,4,5,70]))),
            Sub,
            Box::new(TestNode::Leaf(HashSet::from([4,5,60])))
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
        let SmartAstSet(SmartAstNode::Leaf(c_leaf)) = c.clone() else {
            panic!("unexpected set structure")
        };
        let d = TestSet::new([4]);
        let SmartAstSet(SmartAstNode::Leaf(d_leaf)) = d.clone() else {
            panic!("unexpected set structure")
        };
        let e = TestSet::new([2,5]);
        let SmartAstSet(SmartAstNode::Leaf(e_leaf)) = e.clone() else {
            panic!("unexpected set structure")
        };
        let f = TestSet::new([6]);
        let SmartAstSet(SmartAstNode::Leaf(f_leaf)) = f.clone() else {
            panic!("unexpected set structure")
        };
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
    

    // #[test_case(
    //     TestSet::new(["liquore","text","kkk"]),
    //     "text";
    //     "simple"
    // )]
    // #[test_case(
    //     TestSet::new(["niluk"]) | TestSet::new(["jukonne si"]),
    //     "jukonne si";
    //     "union"
    // )]
    // #[test_case(
    //     TestSet::new(["grum","jukonne","litro"]) & TestSet::new(["nespo","jukonne"]),
    //     "jukonne";
    //     "intersect"
    // )]
    // #[test_case(
    //     TestSet::new(["text that has to be jukonne"]) / TestSet::new(["jukone"]),
    //     "text that has to be jukonne";
    //     "subtraction"
    // )]
    // #[test_case(
    //     TestSet::new(["tra"]) | (TestSet::new(["golib","be jukonne"]) & TestSet::new(["be jukonne","ggg"]) / TestSet::new(["pulvilio"])),
    //     "be jukonne";
    //     "expression"
    // )]
    // fn element_in_set(txt_set: TestSet, txt: &str){
    //     assert!(txt_set.contains(txt));
    // }


    // #[test_case(
    //     TestSet::new(["liquore","text","kkk"]),
    //     "gulm";
    //     "simple"
    // )]
    // #[test_case(
    //     TestSet::new(["niluk"]) | TestSet::new(["jukonne si"]),
    //     "jukonne no";
    //     "union"
    // )]
    // #[test_case(
    //     TestSet::new(["grum","jukonne","litro"]) & TestSet::new(["nespo","jukonne"]),
    //     "grum";
    //     "intersect"
    // )]
    // #[test_case(
    //     TestSet::new(["jukone","grummo"]) / TestSet::new(["piffo","jukone"]),
    //     "jukone";
    //     "subtraction"
    // )]
    // #[test_case(
    //     TestSet::new(["tra"]) | (
    //         TestSet::new(["golib","be jukonne"]) & (
    //             TestSet::new(["be jukonne","ggg"]) / TestSet::new(["pulvilio","be jukonne"])
    //         )
    //     ),
    //     "be jukonne";
    //     "expression"
    // )]
    // fn element_not_in_set(txt_set: TestSet, txt: &str){
    //     assert!(!txt_set.contains(txt));
    // }
}
