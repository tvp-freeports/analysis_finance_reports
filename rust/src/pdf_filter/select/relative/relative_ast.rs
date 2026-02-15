use crate::commons::sets::{SetOps};
use std::ops::{BitOr, BitAnd, Div};
use super::{RelativeInfo};
use super::super::pdf_line::{
    font::FontSet,
    PdfLine
};

pub enum AstNode<L>
{
    Leaf(L),
    Branch(Box<AstNode<L>>, SetOps, Box<AstNode<L>>)
}

impl<L> PartialEq<Self> for AstNode<L>
where
    L: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self,other) {
            (Self::Leaf(a),Self::Leaf(b)) => a == b,
            (
                Self::Branch(box_a0,op_a,box_a1),
                Self::Branch(box_b0,op_b,box_b1)
            ) => op_a == op_b && box_a0 == box_b0 && box_a1 == box_b1,
            _ => false
        }
    }
}


// #[derive(Clone)]
pub struct AstReltaiveInfo<L>(AstNode<L>);

impl<L> AstReltaiveInfo<L>
{
    pub fn from_leaf(leaf: L) -> Self {
        Self(AstNode::Leaf(leaf))
    }
    pub fn ast(&self) -> &AstNode<L> {
        &self.0
    }
}

impl<L,V> RelativeInfo<V> for AstNode<L>
where
    L: RelativeInfo<V>,
    V: BitOr<V,Output=V> + BitAnd<V,Output=V> + Div<V,Output=V>
{
    fn contextualize(self,lines: &[PdfLine]) -> V {
        use SetOps::*;
        match self {
            Self::Leaf(leaf) => leaf.contextualize(lines),
            Self::Branch(box_x,op,box_y) => {
                let a = box_x.contextualize(lines);
                let b = box_y.contextualize(lines);
                match op {
                    Union => a | b,
                    Inter => a & b,
                    Sub => a / b
                }
            }
        }
    }
}

impl<L> BitOr<Self> for AstReltaiveInfo<L>
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
impl<L> BitAnd<Self> for AstReltaiveInfo<L>
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
impl<L> Div<Self> for AstReltaiveInfo<L>
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





#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    use std::sync::LazyLock;
    static LINES: LazyLock<Vec<PdfLine>> = LazyLock::new(|| vec![
        PdfLine::new("Arial",45.0,"TITLE OF THE PAGE",(35.0,1.0,65.0,5.0)),
        PdfLine::new("A",1.5,"text",(10.0,10.0,15.0,11.0)),
        PdfLine::new("A",1.7,"with",(10.0,11.0,15.0,12.0)),
        PdfLine::new("C",1.3,"similar",(10.0,12.0,15.0,13.0)),
        PdfLine::new("D",1.1,"font",(10.0,13.0,15.0,14.0)),
        PdfLine::new("E",1.13,"size",(10.0,14.0,15.0,15.0)),
        PdfLine::new("Fracktur",40.0,"SECTION 2",(35.0,21.0,65.0,25.0)),
        PdfLine::new("A",14.5,"same",(10.0,30.0,15.0,31.0)),
        PdfLine::new("A",188.7,"font",(10.0,31.0,15.0,32.0)),
        PdfLine::new("B",0.3,"-----",(10.0,32.0,15.0,33.0)),
        PdfLine::new("DDD",14.1,"font",(10.0,33.0,15.0,34.0)),
        PdfLine::new("EEE",14.13,"size",(10.0,34.0,15.0,35.0))
    ]);
    struct TestAstLeaf(String);
    impl RelativeInfo<Option<FontSet>> for TestAstLeaf {
        fn contextualize(self,lines: &[PdfLine]) -> Option<FontSet> {
            lines.iter()
            .filter(|l| l.text() == &self.0)
            .map(|l| FontSet::from_atom(l.font().clone()))
            .reduce(|a,b| a | b )
        }
    }

}
//     use std::collections::HashSet;
//     impl Container for HashSet<String> {
//         type Elem = str;
//         fn contains(&self,txt: &str) -> bool {
//             HashSet::contains(self,txt)
//         }
//     }
//     type TestSet = AstReltaiveInfo<HashSet<String>,str>;
//     type TestNode = AstNode<HashSet<String>,str>;
//     impl TestSet {
//         fn new<const N: usize>(vec: [&str; N]) -> Self {
//             Self(AstNode::Leaf(
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
//                 AstNode::Leaf(a) => Self::Leaf(a.clone()),
//                 AstNode::Branch(box_a,op,box_b) => Self::Branch(
//                     box_a.clone(),
//                     *op,
//                     box_b.clone()
//                 )
//             }
//         }  
//     }
//     #[test]
//     fn new() {
//         let l = HashSet::from(["nilpo".to_string(),"grummo".to_string(),"sabbo".to_string()]);
//         let s = TestSet::from_leaf(l.clone());
//         match s {
//             AstReltaiveInfo(AstNode::Leaf(lf)) => assert_eq!(l,lf),
//             _ => panic!("AstReltaiveInfo doesn't have expected shape")
//         }
//     }
//     mod ast_creation {
//         use super::*;
//         use pretty_assertions::assert_eq;
//         #[test]
//         fn union() {
//             let a = TestSet::new(["cave","ghino"]);
//             let AstReltaiveInfo(AstNode::Leaf(a_leaf)) = a.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let b = TestSet::new(["canem","sunnia","sonnino"]);
//             let AstReltaiveInfo(AstNode::Leaf(b_leaf)) = b.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let c = a | b;
//             match c {
//                 AstReltaiveInfo(AstNode::Branch(
//                     box_x,
//                     op,
//                     box_y
//                 )) => {
//                     let AstNode::Leaf(x) = *box_x else {
//                         panic!("unexpected node structure")
//                     };
//                     let AstNode::Leaf(y) = *box_y else {
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
//             let AstReltaiveInfo(AstNode::Leaf(a_leaf)) = a.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let b = TestSet::new(["canem","sunnia","sonnino"]);
//             let AstReltaiveInfo(AstNode::Leaf(b_leaf)) = b.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let c = a & b;
//             match c {
//                 AstReltaiveInfo(AstNode::Branch(
//                     box_x,
//                     op,
//                     box_y
//                 )) => {
//                     let AstNode::Leaf(x) = *box_x else {
//                         panic!("unexpected node structure")
//                     };
//                     let AstNode::Leaf(y) = *box_y else {
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
//             let AstReltaiveInfo(AstNode::Leaf(a_leaf)) = a.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let b = TestSet::new(["canem","sunnia","sonnino"]);
//             let AstReltaiveInfo(AstNode::Leaf(b_leaf)) = b.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let c = a / b;
//             match c {
//                 AstReltaiveInfo(AstNode::Branch(
//                     box_x,
//                     op,
//                     box_y
//                 )) => {
//                     let AstNode::Leaf(x) = *box_x else {
//                         panic!("unexpected node structure")
//                     };
//                     let AstNode::Leaf(y) = *box_y else {
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
//             let AstReltaiveInfo(AstNode::Leaf(a_leaf)) = a.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let b = TestSet::new(["B"]);
//             let AstReltaiveInfo(AstNode::Leaf(b_leaf)) = b.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let c = TestSet::new(["C"]);
//             let AstReltaiveInfo(AstNode::Leaf(c_leaf)) = c.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let d = TestSet::new(["D"]);
//             let AstReltaiveInfo(AstNode::Leaf(d_leaf)) = d.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let e = TestSet::new(["V"]);
//             let AstReltaiveInfo(AstNode::Leaf(e_leaf)) = e.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let f = TestSet::new(["F"]);
//             let AstReltaiveInfo(AstNode::Leaf(f_leaf)) = f.clone() else {
//                 panic!("unexpected set structure")
//             };
//             let g = a | (b / (c | d)) & (e / f);
//             match g {
//                 AstReltaiveInfo(AstNode::Branch(
//                     box_x0,
//                     op0,
//                     box_y0
//                 )) => {
//                     assert_eq!(op0,SetOps::Union);
//                     let AstNode::Leaf(should_a) = *box_x0 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_a,a_leaf);
//                     let AstNode::Branch(box_x1,op1,box_y1) = *box_y0 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op1,SetOps::Inter);

//                     let AstNode::Branch(box_x2,op2,box_y2) = *box_x1 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op2,SetOps::Sub);
//                     let AstNode::Branch(box_x3,op3,box_y3) = *box_y1 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op3,SetOps::Sub);

//                     let AstNode::Leaf(should_e) = *box_x3 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_e,e_leaf);
//                     let AstNode::Leaf(should_f) = *box_y3 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_f,f_leaf);

//                     let AstNode::Leaf(should_b) = *box_x2 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_b,b_leaf);
//                     let AstNode::Branch(box_x4,op4,box_y4) = *box_y2 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(op4,SetOps::Union);

//                     let AstNode::Leaf(should_c) = *box_x4 else {
//                         panic!("unexpected node structure")
//                     };
//                     assert_eq!(should_c,c_leaf);
//                     let AstNode::Leaf(should_d) = *box_y4 else {
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


