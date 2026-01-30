use std::ops::{BitOr,BitAnd,Div};
use std::cmp::{PartialOrd,Ordering};

pub enum SetRelation {
    Overlapping,
    Subset,
    Superset,
    Disjoint,
    Equal
}

pub trait Container {
    type Elem: ?Sized;
    fn contains(&self,e: &Self::Elem) -> bool;
}

pub trait Overlappable: PartialOrd<Self> {
    fn set_relation(&self, other: &Self) -> SetRelation;
    use SetRelation::*;
    fn is_disjoint(&self, other: &Self) -> bool {
        match self.set_relation() {
            Overlapping => false,
            Subset => false,
            Superset => false,
            Equal => false,
            Disjoint => true,
        }
    }
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use Ordering::*;
        match self.set_relation() {
            Overlapping => None,
            Subset => Some(Less),
            Superset => Some(Greater),
            Equal => Some(Equal),
            Disjoint => None,
        } 
    }
}

pub trait SetAstLeaf<T: Container<Elem = E> + Clone + Overlappable, E: ?Sized>: 
    BitAnd<Self,Output=Self> + 
    BitOr<Self,Output=Self> + 
    Div<Self,Output=Self> +
    Sized {}

trait SetAlgebra<T: SetAstLeaf,E: ?Sized>:
    Container<Elem=E> +
    BitAnd<Self,Output=Self> + 
    BitOr<Self,Output=Self> + 
    Div<Self,Output=Self> +
    Sized {}



pub enum AstNode<T,E>
where
    T: Container<Elem = E> + Clone,
    E: ?Sized,
{
    Leaf(T),
    Branch(Box<AstNode<T,E>>, SetOps, Box<AstNode<T,E>>)
}
impl<T: Container<Elem = E> + Clone, E: ?Sized> Clone for AstNode<T,E> {
    fn clone(&self) -> Self {
        match self {
            AstNode::Leaf(a) => AstNode::Leaf(a.clone()),
            AstNode::Branch(box_x,op,box_y) => AstNode::Branch(box_x.clone(),*op,box_y.clone())
        }
    }
}


#[derive(Clone,Copy,Debug,PartialEq)]
pub enum SetOps{
    Union,
    Inter,
    Sub
}

impl SetOps {
    pub fn call(&self, a: bool, b: bool) -> bool {
        match self {
            Self::Union => a || b,
            Self::Inter => a && b,
            Self::Sub => a && !b,
        }
    }
}

impl<T,E> Container for AstNode<T,E>
where
    T: Container<Elem = E> + Clone,
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


pub struct Set<T: Container<Elem = E> + Clone, E: ?Sized>(pub AstNode<T,E>);
impl<T: Container<Elem = E> + Clone, E: ?Sized> Clone for Set<T,E> {
    fn clone(&self) -> Self {
        Self(
            self.0.clone()
        )
    }
}


impl<T,E> BitOr<Self> for Set<T,E>
where
    T: Container<Elem = E> + Clone,
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
impl<T,E> BitAnd<Self> for Set<T,E> 
where
    T: Container<Elem = E> + Clone,
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
impl<T,E> Div<Self> for Set<T,E> 
where
    T: Container<Elem = E> + Clone,
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


impl<T,E> Container for Set<T,E> 
where
    T: Container<Elem = E> + Clone,
    E: ?Sized,
{
    type Elem = E;
    fn contains(&self,txt: &E) -> bool {
        let Self(root_node) = self;
        root_node.contains(txt)
    }
}

impl<T,E> SetAlgebra<T,E> for Set<T,E> 
where
    T: Container<Elem = E> + Clone,
    E: ?Sized,
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
    type TestSet = Set<HashSet<String>,str>;
    impl TestSet {
        fn new<const N: usize>(vec: [&str; N]) -> Self {
            Self(AstNode::Leaf(
                HashSet::from(vec.map(|s|s.to_string()))
            ))
        }
    }
    
    mod ast {
        use super::*;
        use pretty_assertions::assert_eq;
        #[test]
        fn union() {
            let a = TestSet::new(["cave","ghino"]);
            let Set(AstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TestSet::new(["canem","sunnia","sonnino"]);
            let Set(AstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = a | b;
            match c {
                Set(AstNode::Branch(
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
            let Set(AstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TestSet::new(["canem","sunnia","sonnino"]);
            let Set(AstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = a & b;
            match c {
                Set(AstNode::Branch(
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
            let Set(AstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TestSet::new(["canem","sunnia","sonnino"]);
            let Set(AstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = a / b;
            match c {
                Set(AstNode::Branch(
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
            let Set(AstNode::Leaf(a_leaf)) = a.clone() else {
                panic!("unexpected set structure")
            };
            let b = TestSet::new(["B"]);
            let Set(AstNode::Leaf(b_leaf)) = b.clone() else {
                panic!("unexpected set structure")
            };
            let c = TestSet::new(["C"]);
            let Set(AstNode::Leaf(c_leaf)) = c.clone() else {
                panic!("unexpected set structure")
            };
            let d = TestSet::new(["D"]);
            let Set(AstNode::Leaf(d_leaf)) = d.clone() else {
                panic!("unexpected set structure")
            };
            let e = TestSet::new(["E"]);
            let Set(AstNode::Leaf(e_leaf)) = e.clone() else {
                panic!("unexpected set structure")
            };
            let f = TestSet::new(["F"]);
            let Set(AstNode::Leaf(f_leaf)) = f.clone() else {
                panic!("unexpected set structure")
            };
            let g = a | (b / (c | d)) & (e / f);
            match g {
                Set(AstNode::Branch(
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


    #[test_case(SetOps::Union,true,true,true)]
    #[test_case(SetOps::Union,true,false,true)]
    #[test_case(SetOps::Union,false,true,true)]
    #[test_case(SetOps::Union,false,false,false)]
    #[test_case(SetOps::Inter,true,true,true)]
    #[test_case(SetOps::Inter,true,false,false)]
    #[test_case(SetOps::Inter,false,true,false)]
    #[test_case(SetOps::Inter,false,false,false)]
    #[test_case(SetOps::Sub,true,true,false)]
    #[test_case(SetOps::Sub,true,false,true)]
    #[test_case(SetOps::Sub,false,true,false)]
    #[test_case(SetOps::Sub,false,false,false)]
    fn evaluate_setops(op: SetOps, a: bool, b: bool, res: bool){
        assert_eq!(op.call(a,b),res);
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