use super::{Container,SetRelation,SetOps,Set,SetAlgebra,Overlappable};
use std::ops::{BitOr, BitAnd, Div};

#[derive(Clone)]
enum SmartAstNode<L,E>
where
    L: Container<Elem = E> + Clone + Overlappable<L>,
    E: ?Sized,
{
    Leaf(L),
    EmptySet,
    Branch(Box<SmartAstNode<L,E>>, SetOps, Box<SmartAstNode<L,E>>)
}


#[derive(Clone)]
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

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use test_case::test_case;
//     use pretty_assertions::assert_eq;
//     use std::collections::HashSet;
//     impl Container for HashSet<String> {
//         type Elem = str;
//         fn contains(&self,txt: &str) -> bool {
//             HashSet::contains(self,txt)
//         }
//     }
//     type TestSet = SmartAstSet<HashSet<String>,str>;
//     type TestNode = SmartAstNode<HashSet<String>,str>;
//     impl TestSet {
//         fn new<const N: usize>(vec: [&str; N]) -> Self {
//             Self(SmartAstNode::Leaf(
//                 HashSet::from(vec.map(|s|s.to_string()))
//             ))
//         }
//     }
//     impl Clone for TestSet {
//         fn clone(&self) -> Self {
//             Self(self.0.clone())
//         }
//     }
//     impl Clone for TestNode {
//         fn clone(&self) -> Self {
//             match self {
//                 SmartAstNode::Leaf(a) => Self::Leaf(a.clone()),
//                 SmartAstNode::Branch(box_a,op,box_b) => Self::Branch(
//                     box_a.clone(),
//                     *op,
//                     box_b.clone()
//                 )
//             }
//         }  
//     }
//     mod ast_creation {
//         use super::*;
//         use pretty_assertions::assert_eq;
//         #[test]
//         fn union() {
//             let a = TestSet::new(["cave","ghino"]);
//             let SmartAstSet(SmartAstNode::Leaf(a_leaf)) = a.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let b = TestSet::new(["canem","sunnia","sonnino"]);
//             let SmartAstSet(SmartAstNode::Leaf(b_leaf)) = b.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let c = a | b;
//             match c {
//                 SmartAstSet(SmartAstNode::Branch(
//                     box_x,
//                     op,
//                     box_y
//                 )) => {
//                     let SmartAstNode::Leaf(x) = *box_x else {
//                         panic!("unexpected node structure")
//                     };
//                     let SmartAstNode::Leaf(y) = *box_y else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op,SetOps::Union);
//                     assert_eq!(x,a_leaf);
//                     assert_eq!(y,b_leaf);
//                 },
//                 _ => panic!("Ast structured different from the one expected")

//             }
//         }
//         #[test]
//         fn intersection() {
//             let a = TestSet::new(["cave","ghino"]);
//             let SmartAstSet(SmartAstNode::Leaf(a_leaf)) = a.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let b = TestSet::new(["canem","sunnia","sonnino"]);
//             let SmartAstSet(SmartAstNode::Leaf(b_leaf)) = b.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let c = a & b;
//             match c {
//                 SmartAstSet(SmartAstNode::Branch(
//                     box_x,
//                     op,
//                     box_y
//                 )) => {
//                     let SmartAstNode::Leaf(x) = *box_x else {
//                         panic!("unexpected node structure")
//                     };
//                     let SmartAstNode::Leaf(y) = *box_y else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op,SetOps::Inter);
//                     assert_eq!(x,a_leaf);
//                     assert_eq!(y,b_leaf);
//                 },
//                 _ => panic!("Ast structured different from the one expected")

//             }
//         }
//         #[test]
//         fn subtraction() {
//             let a = TestSet::new(["cave","ghino"]);
//             let SmartAstSet(SmartAstNode::Leaf(a_leaf)) = a.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let b = TestSet::new(["canem","sunnia","sonnino"]);
//             let SmartAstSet(SmartAstNode::Leaf(b_leaf)) = b.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let c = a / b;
//             match c {
//                 SmartAstSet(SmartAstNode::Branch(
//                     box_x,
//                     op,
//                     box_y
//                 )) => {
//                     let SmartAstNode::Leaf(x) = *box_x else {
//                         panic!("unexpected node structure")
//                     };
//                     let SmartAstNode::Leaf(y) = *box_y else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op,SetOps::Sub);
//                     assert_eq!(x,a_leaf);
//                     assert_eq!(y,b_leaf);
//                 },
//                 _ => panic!("Ast structured different from the one expected")
//             } 
//         }
//         #[test]
//         fn expression() {
//             let a = TestSet::new(["A"]);
//             let SmartAstSet(SmartAstNode::Leaf(a_leaf)) = a.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let b = TestSet::new(["B"]);
//             let SmartAstSet(SmartAstNode::Leaf(b_leaf)) = b.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let c = TestSet::new(["C"]);
//             let SmartAstSet(SmartAstNode::Leaf(c_leaf)) = c.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let d = TestSet::new(["D"]);
//             let SmartAstSet(SmartAstNode::Leaf(d_leaf)) = d.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let e = TestSet::new(["E"]);
//             let SmartAstSet(SmartAstNode::Leaf(e_leaf)) = e.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let f = TestSet::new(["F"]);
//             let SmartAstSet(SmartAstNode::Leaf(f_leaf)) = f.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let g = a | (b / (c | d)) & (e / f);
//             match g {
//                 SmartAstSet(SmartAstNode::Branch(
//                     box_x0,
//                     op0,
//                     box_y0
//                 )) => {
//                     assert_eq!(op0,SetOps::Union);
//                     let SmartAstNode::Leaf(should_a) = *box_x0 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_a,a_leaf);
//                     let SmartAstNode::Branch(box_x1,op1,box_y1) = *box_y0 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op1,SetOps::Inter);

//                     let SmartAstNode::Branch(box_x2,op2,box_y2) = *box_x1 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op2,SetOps::Sub);
//                     let SmartAstNode::Branch(box_x3,op3,box_y3) = *box_y1 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op3,SetOps::Sub);

//                     let SmartAstNode::Leaf(should_e) = *box_x3 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_e,e_leaf);
//                     let SmartAstNode::Leaf(should_f) = *box_y3 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_f,f_leaf);

//                     let SmartAstNode::Leaf(should_b) = *box_x2 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_b,b_leaf);
//                     let SmartAstNode::Branch(box_x4,op4,box_y4) = *box_y2 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op4,SetOps::Union);

//                     let SmartAstNode::Leaf(should_c) = *box_x4 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_c,c_leaf);
//                     let SmartAstNode::Leaf(should_d) = *box_y4 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_d,d_leaf);
//                 },
//                 _ => panic!("Ast structured different from the one expected")

//             }
//         }
//     }

//     #[test_case(
//         TestSet::new(["liquore","text","kkk"]),
//         "text";
//         "simple"
//     )]
//     #[test_case(
//         TestSet::new(["niluk"]) | TestSet::new(["jukonne si"]),
//         "jukonne si";
//         "union"
//     )]
//     #[test_case(
//         TestSet::new(["grum","jukonne","litro"]) & TestSet::new(["nespo","jukonne"]),
//         "jukonne";
//         "intersect"
//     )]
//     #[test_case(
//         TestSet::new(["text that has to be jukonne"]) / TestSet::new(["jukone"]),
//         "text that has to be jukonne";
//         "subtraction"
//     )]
//     #[test_case(
//         TestSet::new(["tra"]) | (TestSet::new(["golib","be jukonne"]) & TestSet::new(["be jukonne","ggg"]) / TestSet::new(["pulvilio"])),
//         "be jukonne";
//         "expression"
//     )]
//     fn element_in_set(txt_set: TestSet, txt: &str){
//         assert!(txt_set.contains(txt));
//     }


//     #[test_case(
//         TestSet::new(["liquore","text","kkk"]),
//         "gulm";
//         "simple"
//     )]
//     #[test_case(
//         TestSet::new(["niluk"]) | TestSet::new(["jukonne si"]),
//         "jukonne no";
//         "union"
//     )]
//     #[test_case(
//         TestSet::new(["grum","jukonne","litro"]) & TestSet::new(["nespo","jukonne"]),
//         "grum";
//         "intersect"
//     )]
//     #[test_case(
//         TestSet::new(["jukone","grummo"]) / TestSet::new(["piffo","jukone"]),
//         "jukone";
//         "subtraction"
//     )]
//     #[test_case(
//         TestSet::new(["tra"]) | (
//             TestSet::new(["golib","be jukonne"]) & (
//                 TestSet::new(["be jukonne","ggg"]) / TestSet::new(["pulvilio","be jukonne"])
//             )
//         ),
//         "be jukonne";
//         "expression"
//     )]
//     fn element_not_in_set(txt_set: TestSet, txt: &str){
//         assert!(!txt_set.contains(txt));
//     }
// }


