use super::{Container,SetOps,UncomparableSet,SetAlgebra};
use std::ops::{BitOr, BitAnd, Div};

#[derive(Clone)]
enum AstNode<L,E>
where
    L: Container<Elem = E> + Clone,
    E: ?Sized,
{
    Leaf(L),
    Branch(Box<AstNode<L,E>>, SetOps, Box<AstNode<L,E>>)
}


#[derive(Clone)]
pub struct AstSet<L,E>(AstNode<L,E>)
where
    L: Container<Elem = E> + Clone,
    E: ?Sized
;

impl<L,E> AstSet<L,E> 
where
    L: Container<Elem = E> + Clone,
    E: ?Sized
{
    pub fn from_leaf(leaf: L) -> Self {
        Self(AstNode::Leaf(leaf))
    }
}

impl<L,E> Container for AstNode<L,E>
where
    L: Container<Elem = E> + Clone,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self,ele: &E) -> bool {
        match self {
            Self::Leaf(leaf) => leaf.contains(ele),
            Self::Branch(box_x,op,box_y) => op.call(
                box_x.contains(ele),box_y.contains(ele)
            )
        }
    }
}

impl<L,E> BitOr<Self> for AstSet<L,E>
where
    L: Container<Elem = E> + Clone,
    E: ?Sized,
{
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(
            AstNode::Branch(
                Box::new(self.0),
                SetOps::Union,
                Box::new(rhs.0)
            )
        )
    }
}
impl<L,E> BitAnd<Self> for AstSet<L,E> 
where
    L: Container<Elem = E> + Clone,
    E: ?Sized,
{
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(
            AstNode::Branch(
                Box::new(self.0),
                SetOps::Inter,
                Box::new(rhs.0)
            )
        )
    }
}
impl<L,E> Div<Self> for AstSet<L,E> 
where
    L: Container<Elem = E> + Clone,
    E: ?Sized,
{
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self(
            AstNode::Branch(
                Box::new(self.0),
                SetOps::Sub,
                Box::new(rhs.0)
            )
        )
    }
}


impl<L,E> Container for AstSet<L,E> 
where
    L: Container<Elem = E> + Clone,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self,txt: &E) -> bool {
        let Self(root_node) = self;
        root_node.contains(txt)
    }
}


impl<L,E> SetAlgebra for AstSet<L,E> 
where
    L: Container<Elem = E> + Clone,
    E: ?Sized
{}

impl<L,E> UncomparableSet<E> for AstSet<L,E>
where
    L: Container<Elem = E> + Clone,
    E: ?Sized
{}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    use pretty_assertions::assert_eq;
    use std::collections::HashSet;
    impl Container for HashSet<String> {
        type Elem = str;
        fn contains(&self,txt: &str) -> bool {
            HashSet::contains(self,txt)
        }
    }
    type TestSet = AstSet<HashSet<String>,str>;
    type TestNode = AstNode<HashSet<String>,str>;
    impl TestSet {
        fn new<const N: usize>(vec: [&str; N]) -> Self {
            Self(AstNode::Leaf(
                HashSet::from(vec.map(|s|s.to_string()))
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
                AstNode::Leaf(a) => Self::Leaf(a.clone()),
                AstNode::Branch(box_a,op,box_b) => Self::Branch(
                    box_a.clone(),
                    *op,
                    box_b.clone()
                )
            }
        }  
    }
    #[test]
    fn new() {
        let l = HashSet::from(["nilpo".to_string(),"grummo".to_string(),"sabbo".to_string()]);
        let s = TestSet::from_leaf(l.clone());
        match s {
            AstSet(AstNode::Leaf(lf)) => assert_eq!(l,lf),
            _ => panic!("AstSet doesn't have expected shape")
        }
    }
    mod ast_creation {
        use super::*;
        use pretty_assertions::assert_eq;
        #[test]
        fn union() {
            let a = TestSet::new(["cave","ghino"]);
            let AstSet(AstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TestSet::new(["canem","sunnia","sonnino"]);
            let AstSet(AstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = a | b;
            match c {
                AstSet(AstNode::Branch(
                    box_x,
                    op,
                    box_y
                )) => {
                    let AstNode::Leaf(x) = *box_x else {
                        panic!("unexpected node structure")
                    };
                    let AstNode::Leaf(y) = *box_y else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op,SetOps::Union);
                    assert_eq!(x,a_leaf);
                    assert_eq!(y,b_leaf);
                },
                _ => panic!("Ast structured different from the one expected")

            }
        }
        #[test]
        fn intersection() {
            let a = TestSet::new(["cave","ghino"]);
            let AstSet(AstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TestSet::new(["canem","sunnia","sonnino"]);
            let AstSet(AstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = a & b;
            match c {
                AstSet(AstNode::Branch(
                    box_x,
                    op,
                    box_y
                )) => {
                    let AstNode::Leaf(x) = *box_x else {
                        panic!("unexpected node structure")
                    };
                    let AstNode::Leaf(y) = *box_y else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op,SetOps::Inter);
                    assert_eq!(x,a_leaf);
                    assert_eq!(y,b_leaf);
                },
                _ => panic!("Ast structured different from the one expected")

            }
        }
        #[test]
        fn subtraction() {
            let a = TestSet::new(["cave","ghino"]);
            let AstSet(AstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TestSet::new(["canem","sunnia","sonnino"]);
            let AstSet(AstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = a / b;
            match c {
                AstSet(AstNode::Branch(
                    box_x,
                    op,
                    box_y
                )) => {
                    let AstNode::Leaf(x) = *box_x else {
                        panic!("unexpected node structure")
                    };
                    let AstNode::Leaf(y) = *box_y else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op,SetOps::Sub);
                    assert_eq!(x,a_leaf);
                    assert_eq!(y,b_leaf);
                },
                _ => panic!("Ast structured different from the one expected")
            } 
        }
        #[test]
        fn expression() {
            let a = TestSet::new(["A"]);
            let AstSet(AstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TestSet::new(["B"]);
            let AstSet(AstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = TestSet::new(["C"]);
            let AstSet(AstNode::Leaf(c_leaf)) = c.clone() else {
                panic!("unexpected set structure")
            };
            let d = TestSet::new(["D"]);
            let AstSet(AstNode::Leaf(d_leaf)) = d.clone() else {
                panic!("unexpected set structure")
            };
            let e = TestSet::new(["E"]);
            let AstSet(AstNode::Leaf(e_leaf)) = e.clone() else {
                panic!("unexpected set structure")
            };
            let f = TestSet::new(["F"]);
            let AstSet(AstNode::Leaf(f_leaf)) = f.clone() else {
                panic!("unexpected set structure")
            };
            let g = a | (b / (c | d)) & (e / f);
            match g {
                AstSet(AstNode::Branch(
                    box_x0,
                    op0,
                    box_y0
                )) => {
                    assert_eq!(op0,SetOps::Union);
                    let AstNode::Leaf(should_a) = *box_x0 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_a,a_leaf);
                    let AstNode::Branch(box_x1,op1,box_y1) = *box_y0 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op1,SetOps::Inter);

                    let AstNode::Branch(box_x2,op2,box_y2) = *box_x1 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op2,SetOps::Sub);
                    let AstNode::Branch(box_x3,op3,box_y3) = *box_y1 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op3,SetOps::Sub);

                    let AstNode::Leaf(should_e) = *box_x3 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_e,e_leaf);
                    let AstNode::Leaf(should_f) = *box_y3 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_f,f_leaf);

                    let AstNode::Leaf(should_b) = *box_x2 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_b,b_leaf);
                    let AstNode::Branch(box_x4,op4,box_y4) = *box_y2 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(op4,SetOps::Union);

                    let AstNode::Leaf(should_c) = *box_x4 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_c,c_leaf);
                    let AstNode::Leaf(should_d) = *box_y4 else {
                        panic!("unexpected node structure")
                    };
                    assert_eq!(should_d,d_leaf);
                },
                _ => panic!("Ast structured different from the one expected")

            }
        }
    }

    #[test_case(
        TestSet::new(["liquore","text","kkk"]),
        "text";
        "simple"
    )]
    #[test_case(
        TestSet::new(["niluk"]) | TestSet::new(["jukonne si"]),
        "jukonne si";
        "union"
    )]
    #[test_case(
        TestSet::new(["grum","jukonne","litro"]) & TestSet::new(["nespo","jukonne"]),
        "jukonne";
        "intersect"
    )]
    #[test_case(
        TestSet::new(["text that has to be jukonne"]) / TestSet::new(["jukone"]),
        "text that has to be jukonne";
        "subtraction"
    )]
    #[test_case(
        TestSet::new(["tra"]) | (TestSet::new(["golib","be jukonne"]) & TestSet::new(["be jukonne","ggg"]) / TestSet::new(["pulvilio"])),
        "be jukonne";
        "expression"
    )]
    fn element_in_set(txt_set: TestSet, txt: &str){
        assert!(txt_set.contains(txt));
    }


    #[test_case(
        TestSet::new(["liquore","text","kkk"]),
        "gulm";
        "simple"
    )]
    #[test_case(
        TestSet::new(["niluk"]) | TestSet::new(["jukonne si"]),
        "jukonne no";
        "union"
    )]
    #[test_case(
        TestSet::new(["grum","jukonne","litro"]) & TestSet::new(["nespo","jukonne"]),
        "grum";
        "intersect"
    )]
    #[test_case(
        TestSet::new(["jukone","grummo"]) / TestSet::new(["piffo","jukone"]),
        "jukone";
        "subtraction"
    )]
    #[test_case(
        TestSet::new(["tra"]) | (
            TestSet::new(["golib","be jukonne"]) & (
                TestSet::new(["be jukonne","ggg"]) / TestSet::new(["pulvilio","be jukonne"])
            )
        ),
        "be jukonne";
        "expression"
    )]
    fn element_not_in_set(txt_set: TestSet, txt: &str){
        assert!(!txt_set.contains(txt));
    }
}


