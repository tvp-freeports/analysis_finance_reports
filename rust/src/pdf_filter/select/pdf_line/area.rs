use crate::commons::sets::{DisjointAtomsSet,Container,Overlappable,AtomOperations,SetRelation,CompoundAtomOperationRes};
use crate::commons::geometry::{Rectangle};

impl Container for Rectangle {
    type Elem = (f32,f32);
    fn contains(&self, point: &(f32,f32)) -> bool {
        let (x0,y0,x1,y1) = self.as_tuple();
        ( x0 <= point.0 && point.0 <= x1 ) && ( y0 <= point.1 && point.1 <= y1 )
    }
}
impl Overlappable<Self> for Rectangle {
    fn set_relation(&self,other: &Self) -> SetRelation {
        use SetRelation::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = self.as_tuple();
        if (
            (x0>a1 || x1<a0) || (a0>x1 || a1<x0)
        ) || (
            (y0>b1 || y1<b0) || (b0>y1 || b1<y0)
        ){
            Disjoint
        } else if (x0,y0,x1,y1) == (a0,b0,a1,b1) {
            Equal
        } else if (a0<=x0 && x1<=a1 && b0<=y0 && y1<=b1) {
            Subset
        } else if (x0<=a0 && a1<=x1 && y0<=b0 && b1<=y1) {
            Superset
        } else {
            Overlapping
        }
    }
}

enum RectOverlapping {
    SmallerLeft,
    SmallerRight,
    SmallerTop,
    SmallerBottom,
    BiggerLeft,
    BiggerRight,
    BiggerTop,
    BiggerBottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight
}

impl Rectangle {
    fn type_overlap(&self,other: &Self) -> RectOverlapping {
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        if x0<=a0 {
            // BiggerRight SmallerRight TopRight BottomRight SmallerTop SmallerBottom
            if x1 <= b1 {
                // BiggerRight SmallerRight TopRight BottomRight
                if y0<=b0 {
                    // SmallerRight BottomRight
                    if b1 <= y1 {
                        SmallerRight
                    } else {
                        BottomRight
                    }
                } else {
                    // BiggerRight TopRight
                    if b0 <= y0 {
                        TopRight
                    } else {
                        BiggerRight
                    }
                }
            } else {
                // SmallerTop SmallerBottom
                if y0 <= b1 {
                    SmallerTop
                } else {
                    SmallerBottom
                }
            }
        } else {
            // BiggerLeft SmallerLeft TopLeft BottomLeft BiggerTop BiggerBottom
            if x1<=a1 {
                // BiggerLeft SmallerLeft TopLeft BottomLeft
                if y0<=b0 {
                    // SmallerLeft BottomLeft
                    if b1 <= y1 {
                        SmallerLeft
                    } else {
                        BottomLeft
                    }
                } else {
                    // BiggerLeft TopLeft
                    if b0 <= y0 {
                        TopLeft
                    } else {
                        BiggerLeft
                    }
                }
            } else {
                // BiggerTop BiggerBottom
                if y0 <= b1 {
                    BiggerTop
                } else {
                    BiggerBottom
                }
            }
        }
    }
}


pub enum SubtractOverlappingRectanglesRes {
    One(Rectangle),
    Two(Rectangle,Rectangle),
    Three(Rectangle,Rectangle,Rectangle)
}
pub enum SubtractSubsetRectanglesRes {
    Four(Rectangle,Rectangle,Rectangle,Rectangle)
}
pub enum IntersectOverlappingRectanglesRes {
    One(Rectangle)
}

impl Into<CompoundAtomOperationRes<Rectangle>> for SubtractSubsetRectanglesRes {
    fn into(self) -> CompoundAtomOperationRes<Rectangle> {
        use SubtractSubsetRectanglesRes::*;
        match self {
            Four(a,b,c,d) => CompoundAtomOperationRes::Four(a,b,c,d)
        }
    }
}
impl Into<CompoundAtomOperationRes<Rectangle>> for SubtractOverlappingRectanglesRes {
    fn into(self) -> CompoundAtomOperationRes<Rectangle> {
        use SubtractOverlappingRectanglesRes::*;
        match self {
            One(a) => CompoundAtomOperationRes::One(a),
            Two(a,b) => CompoundAtomOperationRes::Two(a,b),
            Three(a,b,c) => CompoundAtomOperationRes::Three(a,b,c)
        }
    }
}
impl Into<CompoundAtomOperationRes<Rectangle>> for IntersectOverlappingRectanglesRes {
    fn into(self) -> CompoundAtomOperationRes<Rectangle> {
        use IntersectOverlappingRectanglesRes::*;
        match self {
            One(a) => CompoundAtomOperationRes::One(a)
        }
    }
}


impl AtomOperations for  Rectangle {
    type SubtractOverlappingRes = SubtractOverlappingRectanglesRes;
    type SubtractSubsetRes = SubtractSubsetRectanglesRes;
    type IntersectOverlappingRes = IntersectOverlappingRectanglesRes;
    
    fn subtract_subset(&self,other: &Self) -> SubtractSubsetRectanglesRes {
        use SubtractSubsetRectanglesRes::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        let h: (f32,f32,f32,f32);
        let v: (f32,f32,f32,f32);
        if a0<x0 || b0<y0 || a1<x1 || b1<y1 {
            h=(a0,x0,x1,a1);
            v=(b0,y0,y1,b1);
        } else {
            h=(x0,a0,a1,x1);
            v=(y0,b0,b1,y1);
        }
        Four(
            Self::new(h.0,v.0,h.2,v.1),
            Self::new(h.0,v.1,h.1,v.3),
            Self::new(h.1,v.2,h.3,v.3),
            Self::new(h.2,v.0,h.3,v.2)
        )
    }
    fn subtract_overlapping(&self,other: &Self) -> SubtractOverlappingRectanglesRes {
        use SubtractOverlappingRectanglesRes::*;
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        match self.type_overlap(other) {
            SmallerLeft => Three(
                Self::new(x0,y0,x1,b0),
                Self::new(a1,b0,x1,y1),
                Self::new(x0,b1,a1,y1)
            ),
            SmallerRight => Three(
                Self::new(x0,b1,x1,y1),
                Self::new(x0,y0,a0,b1),
                Self::new(a0,y0,x1,b0)
            ),
            SmallerTop => Three(
                Self::new(a1,y0,x1,y1),
                Self::new(x0,b1,a1,y1),
                Self::new(x0,y0,a0,b1)
            ),
            SmallerBottom => Three(
                Self::new(x0,y0,a0,y1),
                Self::new(a0,y0,x1,b0),
                Self::new(a1,b0,x1,y1)
            ),
            BiggerLeft => One(Self::new(a1,y0,x1,y1)),
            BiggerRight => One(Self::new(x0,y0,a0,y1)),
            BiggerTop => One(Self::new(x0,b1,x1,y1)),
            BiggerBottom => One(Self::new(x0,y0,x1,b0)),
            TopLeft => Two(
                Self::new(a1,y0,x1,y1),
                Self::new(x0,b1,a1,y1)
            ),
            TopRight => Two(
                Self::new(x0,y0,x1,b0),
                Self::new(a1,b0,x1,y1)
            ),
            BottomRight => Two(
                Self::new(x0,y0,a0,y1),
                Self::new(a0,y0,x1,b0)
            ),
            BottomLeft => Two(
                Self::new(x0,b1,x1,y1),
                Self::new(x0,y0,a0,b1)
            )
        }
    }

    fn intersect_overlapping(&self,other: &Self) -> IntersectOverlappingRectanglesRes {
        use IntersectOverlappingRectanglesRes::*;
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        match self.type_overlap(other) {
            SmallerLeft => One(Self::new(x0,b0,a1,b1)),
            SmallerRight => One(Self::new(a0,b0,x1,b1)),
            SmallerTop => One(Self::new(a0,y0,a1,b1)),
            SmallerBottom => One(Self::new(a0,b0,a1,y1)),
            BiggerLeft => One(Self::new(x0,y0,a1,y1)),
            BiggerRight => One(Self::new(a0,y0,x1,y1)),
            BiggerTop => One(Self::new(x0,y0,x1,b1)),
            BiggerBottom => One(Self::new(x0,b0,x1,y1)),
            TopLeft => One(Self::new(x0,y0,a1,b1)),
            TopRight => One(Self::new(x0,b0,a1,y1)),
            BottomRight => One(Self::new(a0,b0,x1,y1)),
            BottomLeft => One(Self::new(a0,y0,x1,b1))
        }
    }
}









// use crate::commons::sets::{Container,Set,AstNode,SetOps,Overlappable,SetRelation};
// use crate::commons::geometry::Rectangle;

// type AreaAstLeaf = Rectangle;






// impl Area {
//     pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
//         Self(AstNode::Leaf(Rectangle::new(x0,y0,x1,y1)))
//     }
// }


// // pub type Area = Set<Rectangle,Rectangle>;
// pub type Area = Set<Rectangle,(f32,f32)>;
// pub type AreaSet = Area;

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use pretty_assertions::assert_eq;
//     use test_case::test_case;

//     #[test]
//     fn new_area() {
//         let (x0,y0,x1,y1) = (0.2,9.0,0.4,10.0);
//         let res = (0.2,9.0,0.4,10.0);
//         let Set(AstNode::Leaf(rec)) = Area::new(x0,y0,x1,y1) else {
//             panic!("Expected have to be a AreaSet with just one leaf")
//         };
//         assert_eq!(rec.as_tuple(),res);
//     }

//     #[test]
//     fn element_in_leafset() {
//         let rec=Rectangle::new(0.0,20.0,50.0,80.0);
//         let point = (3.0,29.89);
//         assert!(rec.contains(&point));
//     }
//     #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(100.0,81.63);"outside")]
//     #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),(0.99,50.0);"more left")]
//     #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(55.0,50.0);"more right")]
//     #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(30.4,11.11);"higher")]
//     #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(30.4,88.88);"lower")]
//     fn element_not_in_leafset(rec: Rectangle, point: (f32,f32)) {
//         assert!(!rec.contains(&point))
//     }

//     // #[test]
//     // fn element_in_leafset() {
//     //     let area=Rectangle::new(0.0,20.0,50.0,80.0);
//     //     let rec = Rectangle::new(3.0,29.89,49.0,79.0);
//     //     assert!(area.contains(&rec));
//     // }
//     // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(300.0,290.89,490.0,790.0);"outside")]
//     // #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,19.89,55.0,88.0);"around")]
//     // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(3.0,13.11,49.0,81.0);"cross")]
//     // #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.5,29.89,49.0,79.0);"left side")]
//     // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(30.0,29.89,49.0,793.0);"right side")]
//     // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(30.0,9.89,49.0,79.0);"top")]
//     // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(30.0,29.89,50.0002,79.0);"bottom")]
//     // fn element_not_in_leafset(area: Rectangle, rec: Rectangle) {
//     //     assert!(!area.contains(&rec))
//     // }
// }