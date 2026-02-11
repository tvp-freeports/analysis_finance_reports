use crate::commons::sets::{
    DisjointAtomsSet,
    Container,
    Overlappable,
    AtomOperations,
    SetRelation,
    CompoundAtomOperationRes,
    AtomAlgebra
};
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
        let (a0,b0,a1,b1) = other.as_tuple();
        if (
            (x0>=a1 || x1<=a0) || (a0>=x1 || a1<=x0)
        ) || (
            (y0>=b1 || y1<=b0) || (b0>=y1 || b1<=y0)
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

#[derive(PartialEq,Debug)]
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
    BottomRight,
    Vertical,
    Horizontal
}

impl Rectangle {
    fn type_overlap(&self,other: &Self) -> RectOverlapping {
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        if x0<a0 {
            // BiggerRight SmallerRight TopRight BottomRight SmallerTop SmallerBottom Vertical
            if x1 <= a1 {
                // BiggerRight SmallerRight TopRight BottomRight
                if y0<b0 {
                    // SmallerRight BottomRight
                    if b1 < y1 {
                        SmallerRight
                    } else {
                        BottomRight
                    }
                } else {
                    // BiggerRight TopRight
                    if b1 < y1 {
                        TopRight
                    } else {
                        BiggerRight
                    }
                }
            } else {
                // SmallerTop SmallerBottom Vertical
                if b0 <= y0 {
                    // SmallerTop Vertical
                    if b1 < y1 {
                        SmallerTop
                    } else {
                        Vertical
                    } 
                } else {
                    SmallerBottom
                }
            }
        } else {
            // BiggerLeft SmallerLeft TopLeft BottomLeft BiggerTop BiggerBottom Horizontal
            if a1<x1 {
                // BiggerLeft SmallerLeft TopLeft BottomLeft
                if y0<b0 {
                    // SmallerLeft BottomLeft
                    if b1 < y1 {
                        SmallerLeft
                    } else {
                        BottomLeft
                    }
                } else {
                    // BiggerLeft TopLeft
                    if b1 < y1 {
                        TopLeft
                    } else {
                        BiggerLeft
                    }
                }
            } else {
                // BiggerTop BiggerBottom Horizontal
                if b1 < y1 {
                    // BiggerTop Horizontal
                    if y0 < b0 {
                        Horizontal
                    } else {
                        BiggerTop
                    }
                } else {
                    BiggerBottom
                }
            }
        }
    }
    fn subtract_as_overlap_type(&self,other: &Self, ovrlt: RectOverlapping) -> SubtractOverlappingRectanglesRes {
        use SubtractOverlappingRectanglesRes::*;
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        println!("galego");
        match dbg!(ovrlt) {
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
                Self::new(x0,b1,x1,y1),
                Self::new(x0,y0,a0,b1)
            ),
            BottomRight => Two(
                Self::new(x0,y0,a0,y1),
                Self::new(a0,y0,x1,b0)
            ),
            BottomLeft => Two(
                Self::new(x0,y0,x1,b0),
                Self::new(a1,b0,x1,y1)
            ),
            Vertical => Two(
                Self::new(x0,y0,a0,y1),
                Self::new(a1,y0,x1,y1)
            ),
            Horizontal => Two(
                Self::new(x0,y0,x1,b0),
                Self::new(x0,b1,x1,y1)
            )
        }
    }
}


pub enum SubtractOverlappingRectanglesRes {
    One(Rectangle),
    Two(Rectangle,Rectangle),
    Three(Rectangle,Rectangle,Rectangle)
}
pub enum SubtractSubsetRectanglesRes {
    Three(Rectangle,Rectangle,Rectangle),
    Four(Rectangle,Rectangle,Rectangle,Rectangle)
}
pub enum IntersectOverlappingRectanglesRes {
    One(Rectangle)
}

impl Into<CompoundAtomOperationRes<Rectangle>> for SubtractOverlappingRectanglesRes {
    fn into(self) -> CompoundAtomOperationRes<Rectangle> {
        use CompoundAtomOperationRes::*;
        match self {
            Self::One(a) => One(a),
            Self::Two(a,b) => Two(a,b),
            Self::Three(a,b,c) => Three(a,b,c)
        }
    }
}
impl Into<CompoundAtomOperationRes<Rectangle>> for IntersectOverlappingRectanglesRes {
    fn into(self) -> CompoundAtomOperationRes<Rectangle> {
        use CompoundAtomOperationRes::*;
        match self {
            Self::One(a) => One(a)
        }
    }
}


impl AtomOperations for  Rectangle {
    type SubtractOverlappingRes = SubtractOverlappingRectanglesRes;
    type SubtractSubsetRes = CompoundAtomOperationRes<Rectangle>;
    type IntersectOverlappingRes = IntersectOverlappingRectanglesRes;
    
    fn subtract_subset(&self,other: &Self) -> CompoundAtomOperationRes<Rectangle> {
        use CompoundAtomOperationRes::*;
        use RectOverlapping::*;
        let (x0,y0,x1,y1) = self.as_tuple();
        let (a0,b0,a1,b1) = other.as_tuple();
        let h = (x0,a0,a1,x1);
        let v = (y0,b0,b1,y1);
        match (x0==a0,y0==b0,x1==a1,y1==b1) {
            (true,true,true,true) => unreachable!("if all side are equal rectangle is not subset"),
            (false,true,true,true) => self.subtract_as_overlap_type(other,BiggerRight).into(),
            (true,false,true,true) => self.subtract_as_overlap_type(other,BiggerBottom).into(),
            (true,true,false,true) => self.subtract_as_overlap_type(other,BiggerLeft).into(),
            (true,true,true,false) => self.subtract_as_overlap_type(other,BiggerTop).into(),
            (true,true,false,false) => self.subtract_as_overlap_type(other,TopLeft).into(),
            (false,true,true,false) => self.subtract_as_overlap_type(other,TopRight).into(),
            (false,false,true,true) => self.subtract_as_overlap_type(other,BottomRight).into(),
            (true,false,false,true) => self.subtract_as_overlap_type(other,BottomLeft).into(),
            (true,false,true,false) => self.subtract_as_overlap_type(other,Horizontal).into(),
            (false,true,false,true) => self.subtract_as_overlap_type(other,Vertical).into(),
            (true,false,false,false) => self.subtract_as_overlap_type(other,SmallerLeft).into(),
            (false,true,false,false) => self.subtract_as_overlap_type(other,SmallerTop).into(),
            (false,false,true,false) => self.subtract_as_overlap_type(other,SmallerRight).into(),
            (false,false,false,true) => self.subtract_as_overlap_type(other,SmallerBottom).into(),
            (false,false,false,false) => Four(
                Self::new(h.0,v.0,h.2,v.1),
                Self::new(h.0,v.1,h.1,v.3),
                Self::new(h.1,v.2,h.3,v.3),
                Self::new(h.2,v.0,h.3,v.2)
            )
        }
    }

    fn subtract_overlapping(&self,other: &Self) -> SubtractOverlappingRectanglesRes {
        self.subtract_as_overlap_type(other,self.type_overlap(other))
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
            TopRight => One(Self::new(a0,y0,x1,b1)),
            BottomRight => One(Self::new(a0,b0,x1,y1)),
            BottomLeft => One(Self::new(x0,b0,a1,y1)),
            Vertical => One(Self::new(a0,y0,a1,y1)),
            Horizontal => One(Self::new(x0,b0,x1,b1))
        }
    }
}

impl AtomAlgebra for Rectangle {}

pub type Area = DisjointAtomsSet<Rectangle,(f32,f32)>;

impl Area {
    pub fn new(x0:f32,y0:f32,x1:f32,y1:f32) -> Self {
        Self::from_atom(Rectangle::new(x0,y0,x1,y1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;
    use std::collections::HashSet;
    #[test]
    fn new_area() {
        let (x0,y0,x1,y1) = (3.4,4.5,4.5,56.0);
        let mut set = HashSet::new();
        set.insert(Rectangle::new(x0,y0,x1,y1));
        assert_eq!(Area::new(x0,y0,x1,y1).atoms(),&set);
    }

    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(3.0,29.89);"common")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(3.0,20.0);"touch up")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(3.0,80.0);"touch down")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(50.0,78.9);"touch right")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(0.0,60.4);"touch left")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(50.0,20.0);"top right corner")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(0.0,20.0);"top left corner")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(50.0,80.0);"bottom right corner")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(0.0,80.0);"bottom left corner")]
    fn element_in_leafset(rec: Rectangle, point: (f32,f32)) {
        assert!(rec.contains(&point));
    }
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(100.0,81.63);"outside")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),(0.99,50.0);"more left")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(55.0,50.0);"more right")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(30.4,11.11);"higher")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),(30.4,88.88);"lower")]
    fn element_not_in_leafset(rec: Rectangle, point: (f32,f32)) {
        assert!(!rec.contains(&point))
    }

    use SetRelation::*;
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Equal,Rectangle::new(0.0,20.0,50.0,80.0);"equal")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(3.0,29.89,49.0,79.0);"superset")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(3.0,20.0,49.0,79.0);"superset touch up")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(3.0,29.89,49.0,80.0);"superset touch down")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(3.0,29.89,50.0,79.0);"superset touch right")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(0.0,29.89,49.0,79.0);"superset touch left")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(0.0,20.0,49.0,79.0);"superset same top left corner")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(3.0,20.0,50.0,79.0);"superset same top right corner")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(3.0,29.89,50.0,80.0);"superset same bottom right corner")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(0.0,29.89,49.0,80.0);"superset same bottom left corner")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(0.0,20.0,50.0,79.0);"superset same top corners")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(0.0,29.89,50.0,80.0);"superset same bottom corners")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(0.0,20.0,49.0,80.0);"superset same left corners")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Superset,Rectangle::new(3.0,20.0,50.0,80.0);"superset same right corners")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(0.0,19.89,59.9,799.0);"subset")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(0.0,20.0,59.9,799.0);"subset touch up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(0.0,19.89,59.9,80.0);"subset touch down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(0.0,19.89,50.0,799.0);"subset touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(1.0,19.89,59.9,799.0);"subset touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(1.0,20.0,59.9,799.0);"subset same top left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(0.0,20.0,50.0,799.0);"subset same top right corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(0.0,10.0,50.0,80.0);"subset same bottom right corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(1.0,10.0,59.9,80.0);"subset same bottom left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(1.0,20.0,50.0,799.0);"subset same top corners")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(1.0,10.0,50.0,80.0);"subset same bottom corners")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(1.0,20.0,500.0,80.0);"subset same left corners")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Subset,Rectangle::new(0.4,20.0,50.0,80.0);"subset same right corners")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Disjoint,Rectangle::new(300.0,290.89,490.0,790.0);"disjoint")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Disjoint,Rectangle::new(0.0,19.0,490.0,20.0);"disjoint touch top")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Disjoint,Rectangle::new(0.0,80.0,490.0,790.0);"disjoint touch bottom")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Disjoint,Rectangle::new(0.0,19.89,1.0,790.0);"disjoint touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Disjoint,Rectangle::new(50.0,19.89,490.0,790.0);"disjoint touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Disjoint,Rectangle::new(0.0,19.89,1.0,20.0);"disjoint touch top left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Disjoint,Rectangle::new(0.0,80.0,1.0,790.0);"disjoint touch bottom left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Disjoint,Rectangle::new(50.0,19.89,490.0,20.0);"disjoint touch top right corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Disjoint,Rectangle::new(50.0,80.0,490.0,790.0);"disjoint touch bottom right corner")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Overlapping,Rectangle::new(3.0,13.11,49.0,81.0);"overlapping")]
    fn set_relation(a: Rectangle, rel: SetRelation, b: Rectangle) {
        assert_eq!(a.set_relation(&b),rel);
    }
    use RectOverlapping::*;
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),SmallerLeft,Rectangle::new(0.0,23.11,2.0,67.0);"smaller left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),SmallerRight,Rectangle::new(16.0,23.11,200.0,67.0);"smaller right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),SmallerTop,Rectangle::new(1.1,13.11,2.0,67.0);"smaller top")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),SmallerBottom,Rectangle::new(1.1,67.0,2.0,670.0);"smaller bottom")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerLeft,Rectangle::new(0.0,13.11,2.0,670.0);"bigger left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerLeft,Rectangle::new(1.0,13.11,2.0,670.0);"bigger left touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerLeft,Rectangle::new(0.0,13.11,2.0,80.0);"bigger left touch down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerLeft,Rectangle::new(0.0,20.0,2.0,670.0);"bigger left touch up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerLeft,Rectangle::new(0.0,20.0,2.0,80.0);"bigger left touch up and down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerLeft,Rectangle::new(1.0,13.11,2.0,80.0);"bigger left touch down and left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerLeft,Rectangle::new(0.0,20.0,2.0,670.0);"bigger left touch up and left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerRight,Rectangle::new(16.0,13.11,200.0,670.0);"bigger right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerRight,Rectangle::new(16.0,13.11,50.0,670.0);"bigger right touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerRight,Rectangle::new(16.0,13.11,200.0,80.0);"bigger right touch down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerRight,Rectangle::new(16.0,20.0,200.0,670.0);"bigger right touch up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerRight,Rectangle::new(16.0,20.0,200.0,80.0);"bigger right touch up and down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerRight,Rectangle::new(16.0,13.11,50.0,80.0);"bigger right touch down and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerRight,Rectangle::new(16.0,20.0,50.0,670.0);"bigger right touch up and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerTop,Rectangle::new(0.1,13.11,200.0,67.0);"bigger top")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerTop,Rectangle::new(0.1,20.0,200.0,67.0);"bigger top touch top")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerTop,Rectangle::new(1.0,13.11,200.0,67.0);"bigger top touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerTop,Rectangle::new(0.1,13.11,50.0,67.0);"bigger top touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerTop,Rectangle::new(1.0,13.11,50.0,67.0);"bigger top touch left and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerTop,Rectangle::new(1.0,20.0,200.0,67.0);"bigger top touch top and left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerTop,Rectangle::new(0.1,20.0,50.0,67.0);"bigger top touch top and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerBottom,Rectangle::new(0.1,67.0,200.0,670.0);"bigger bottom")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerBottom,Rectangle::new(0.1,67.0,200.0,80.0);"bigger bottom touch bottom")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerBottom,Rectangle::new(1.0,67.0,200.0,670.0);"bigger bottom touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerBottom,Rectangle::new(0.1,67.0,50.0,670.0);"bigger bottom touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerBottom,Rectangle::new(1.0,67.0,50.0,670.0);"bigger bottom touch left and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerBottom,Rectangle::new(1.0,67.0,200.0,80.0);"bigger bottom touch bottom and left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BiggerBottom,Rectangle::new(0.1,67.0,50.0,80.0);"bigger bottom touch bottom and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),TopLeft,Rectangle::new(0.0,13.11,2.0,67.0);"top left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),TopLeft,Rectangle::new(0.0,20.0,2.0,67.0);"top left up touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),TopLeft,Rectangle::new(0.0,20.0,2.0,67.0);"top left left touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),TopRight,Rectangle::new(41.41,13.11,200.0,67.0);"top right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),TopRight,Rectangle::new(41.41,20.0,200.0,67.0);"top right up touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),TopRight,Rectangle::new(41.41,13.11,50.0,67.0);"top right right touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BottomLeft,Rectangle::new(0.1,25.11,26.0,670.0);"bottom left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BottomLeft,Rectangle::new(0.1,25.11,26.0,80.0);"bottom left down touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BottomLeft,Rectangle::new(1.0,25.11,26.0,670.0);"bottom left left touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BottomRight,Rectangle::new(5.1,67.0,200.0,670.0);"bottom right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BottomRight,Rectangle::new(5.1,67.0,200.0,80.0);"bottom right down touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),BottomRight,Rectangle::new(5.1,67.0,50.0,670.0);"bottom right right touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Vertical,Rectangle::new(5.1,17.0,23.0,670.0);"vertical")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Vertical,Rectangle::new(5.1,20.0,23.0,670.0);"vertical touch up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Vertical,Rectangle::new(5.1,17.0,23.0,80.0);"vertical touch down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Horizontal,Rectangle::new(0.1,22.0,200.0,67.0);"horizontal")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Horizontal,Rectangle::new(1.0,22.0,200.0,67.0);"horizontal touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Horizontal,Rectangle::new(0.1,22.0,50.0,67.0);"horizontal touch right")]
    fn type_overlap(a: Rectangle, ovrt: RectOverlapping, b: Rectangle) {
        assert_eq!(a.type_overlap(&b),ovrt);
    }

    use CompoundAtomOperationRes::*;
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(3.0,29.89,49.0,79.0),
    Four(
        Rectangle::new(1.0,20.0,49.0,29.89),
        Rectangle::new(1.0,29.89,3.0,80.0),
        Rectangle::new(3.0,79.0,50.0,80.0),
        Rectangle::new(49.0,20.0,50.0,79.0)
    );"common")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(3.0,20.0,49.0,79.0),
    Three(
        Rectangle::new(49.0,20.0,50.0,80.0),
        Rectangle::new(1.0,79.0,49.0,80.0),
        Rectangle::new(1.0,20.0,3.0,79.0)
    );"touch up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(3.0,22.22,49.0,80.0),
    Three(
        Rectangle::new(1.0,20.0,3.0,80.0),
        Rectangle::new(3.0,20.0,50.0,22.22),
        Rectangle::new(49.0,22.22,50.0,80.0)
    );"touch down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(2.0,22.0,50.0,70.0),
    Three(
        Rectangle::new(1.0,70.0,50.0,80.0),
        Rectangle::new(1.0,20.0,2.0,70.0),
        Rectangle::new(2.0,20.0,50.0,22.0)
    );"touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,22.0,2.0,70.0),
    Three(
        Rectangle::new(1.0,20.0,50.0,22.0),
        Rectangle::new(2.0,22.0,50.0,80.0),
        Rectangle::new(1.0,70.0,2.0,80.0)
    );"touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,20.0,40.0,70.0),
    Two(
        Rectangle::new(40.0,20.0,50.0,80.0),
        Rectangle::new(1.0,70.0,40.0,80.0)
    );"same top left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,20.0,50.0,60.0),
    Two(
        Rectangle::new(1.0,60.0,50.0,80.0),
        Rectangle::new(1.0,20.0,10.0,60.0)
    );"same top right corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,25.5,50.0,80.0),
    Two(
        Rectangle::new(1.0,20.0,10.0,80.0),
        Rectangle::new(10.0,20.0,50.0,25.5)
    );"same bottom right corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,25.2,40.0,80.0),
    Two(
        Rectangle::new(1.0,20.0,50.0,25.2),
        Rectangle::new(40.0,25.2,50.0,80.0)
    );"same bottom left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,20.0,30.0,80.0),
    Two(
        Rectangle::new(1.0,20.0,11.0,80.0),
        Rectangle::new(30.0,20.0,50.0,80.0)
    );"vertical crossing")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,25.0,50.0,77.0),
    Two(
        Rectangle::new(1.0,20.0,50.0,25.0),
        Rectangle::new(1.0,77.0,50.0,80.0)
    );"horizontal crossing")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,20.0,50.0,40.0),
    One(Rectangle::new(1.0,40.0,50.0,80.0));"same top corners")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,23.0,50.0,80.0),
    One(Rectangle::new(1.0,20.0,50.0,23.0));"same bottom corners")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,20.0,33.0,80.0),
    One(Rectangle::new(33.0,20.0,50.0,80.0));"same left corners")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,20.0,50.0,80.0),
    One(Rectangle::new(1.0,20.0,11.0,80.0));"same right corners")]
    fn subtract_subset(a: Rectangle, b: Rectangle, exp: CompoundAtomOperationRes<Rectangle>){
        match (a.subtract_subset(&b).into(),exp) {
            (Four(ra,rb,rc,rd),Four(ea,eb,ec,ed)) => {
                assert_eq!(ra.as_tuple(),ea.as_tuple());
                assert_eq!(rb.as_tuple(),eb.as_tuple());
                assert_eq!(rc.as_tuple(),ec.as_tuple());
                assert_eq!(rd.as_tuple(),ed.as_tuple());
            },
            (Three(ra,rb,rc),Three(ea,eb,ec)) => {
                assert_eq!(ra.as_tuple(),ea.as_tuple());
                assert_eq!(rb.as_tuple(),eb.as_tuple());
                assert_eq!(rc.as_tuple(),ec.as_tuple());
            },
            (Two(ra,rb),Two(ea,eb)) => {
                assert_eq!(ra.as_tuple(),ea.as_tuple());
                assert_eq!(rb.as_tuple(),eb.as_tuple());
            },
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            _ => panic!("Result doesn't have the expected shape")
        }
    }




    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(3.0,19.0,49.0,79.0),
    Three(
        Rectangle::new(49.0,20.0,50.0,80.0),
        Rectangle::new(1.0,79.0,49.0,80.0),
        Rectangle::new(1.0,20.0,3.0,79.0)
    );"small up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(3.0,22.22,49.0,88.0),
    Three(
        Rectangle::new(1.0,20.0,3.0,80.0),
        Rectangle::new(3.0,20.0,50.0,22.22),
        Rectangle::new(49.0,22.22,50.0,80.0)
    );"small down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(2.0,22.0,55.0,70.0),
    Three(
        Rectangle::new(1.0,70.0,50.0,80.0),
        Rectangle::new(1.0,20.0,2.0,70.0),
        Rectangle::new(2.0,20.0,50.0,22.0)
    );"small right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.0,22.0,2.0,70.0),
    Three(
        Rectangle::new(1.0,20.0,50.0,22.0),
        Rectangle::new(2.0,22.0,50.0,80.0),
        Rectangle::new(1.0,70.0,2.0,80.0)
    );"small left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.0,2.0,40.0,70.0),
    Two(
        Rectangle::new(40.0,20.0,50.0,80.0),
        Rectangle::new(1.0,70.0,40.0,80.0)
    );"top left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.0,20.0,40.0,70.0),
    Two(
        Rectangle::new(40.0,20.0,50.0,80.0),
        Rectangle::new(1.0,70.0,40.0,80.0)
    );"top left corner up touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,2.0,40.0,70.0),
    Two(
        Rectangle::new(40.0,20.0,50.0,80.0),
        Rectangle::new(1.0,70.0,40.0,80.0)
    );"top left corner left touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,2.0,58.0,60.0),
    Two(
        Rectangle::new(1.0,60.0,50.0,80.0),
        Rectangle::new(1.0,20.0,10.0,60.0)
    );"top right corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,20.0,58.0,60.0),
    Two(
        Rectangle::new(1.0,60.0,50.0,80.0),
        Rectangle::new(1.0,20.0,10.0,60.0)
    );"top right corner up touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,2.0,50.0,60.0),
    Two(
        Rectangle::new(1.0,60.0,50.0,80.0),
        Rectangle::new(1.0,20.0,10.0,60.0)
    );"top right corner right touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,25.5,58.0,800.0),
    Two(
        Rectangle::new(1.0,20.0,10.0,80.0),
        Rectangle::new(10.0,20.0,50.0,25.5)
    );"bottom right corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,25.5,58.0,80.0),
    Two(
        Rectangle::new(1.0,20.0,10.0,80.0),
        Rectangle::new(10.0,20.0,50.0,25.5)
    );"bottom right corner down touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,25.5,50.0,800.0),
    Two(
        Rectangle::new(1.0,20.0,10.0,80.0),
        Rectangle::new(10.0,20.0,50.0,25.5)
    );"bottom right corner right touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,25.2,40.0,89.0),
    Two(
        Rectangle::new(1.0,20.0,50.0,25.2),
        Rectangle::new(40.0,25.2,50.0,80.0)
    );"bottom left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,25.2,40.0,80.0),
    Two(
        Rectangle::new(1.0,20.0,50.0,25.2),
        Rectangle::new(40.0,25.2,50.0,80.0)
    );"bottom left corner down touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,25.2,40.0,89.0),
    Two(
        Rectangle::new(1.0,20.0,50.0,25.2),
        Rectangle::new(40.0,25.2,50.0,80.0)
    );"bottom left corner left touch")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,12.0,30.0,90.0),
    Two(
        Rectangle::new(1.0,20.0,11.0,80.0),
        Rectangle::new(30.0,20.0,50.0,80.0)
    );"vertical crossing")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,20.0,30.0,90.0),
    Two(
        Rectangle::new(1.0,20.0,11.0,80.0),
        Rectangle::new(30.0,20.0,50.0,80.0)
    );"vertical crossing touch up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,12.0,30.0,80.0),
    Two(
        Rectangle::new(1.0,20.0,11.0,80.0),
        Rectangle::new(30.0,20.0,50.0,80.0)
    );"vertical crossing touch down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.05,25.0,500.0,77.0),
    Two(
        Rectangle::new(1.0,20.0,50.0,25.0),
        Rectangle::new(1.0,77.0,50.0,80.0)
    );"horizontal crossing")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,25.0,500.0,77.0),
    Two(
        Rectangle::new(1.0,20.0,50.0,25.0),
        Rectangle::new(1.0,77.0,50.0,80.0)
    );"horizontal crossing touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.05,25.0,50.0,77.0),
    Two(
        Rectangle::new(1.0,20.0,50.0,25.0),
        Rectangle::new(1.0,77.0,50.0,80.0)
    );"horizontal crossing touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,2.0,500.0,40.0),
    One(Rectangle::new(1.0,40.0,50.0,80.0));"bigger top")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,20.0,500.0,40.0),
    One(Rectangle::new(1.0,40.0,50.0,80.0));"bigger top touch top")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,2.0,500.0,40.0),
    One(Rectangle::new(1.0,40.0,50.0,80.0));"bigger top touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,2.0,50.0,40.0),
    One(Rectangle::new(1.0,40.0,50.0,80.0));"bigger top touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,2.0,50.0,40.0),
    One(Rectangle::new(1.0,40.0,50.0,80.0));"bigger top touch left and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,20.0,500.0,40.0),
    One(Rectangle::new(1.0,40.0,50.0,80.0));"bigger top touch top and left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,20.0,50.0,40.0),
    One(Rectangle::new(1.0,40.0,50.0,80.0));"bigger top touch top and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,23.0,500.0,86.0),
    One(Rectangle::new(1.0,20.0,50.0,23.0));"bigger bottom")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,23.0,500.0,80.0),
    One(Rectangle::new(1.0,20.0,50.0,23.0));"bigger bottom touch bottom")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,23.0,500.0,86.0),
    One(Rectangle::new(1.0,20.0,50.0,23.0));"bigger bottom touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,23.0,50.0,86.0),
    One(Rectangle::new(1.0,20.0,50.0,23.0));"bigger bottom touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,23.0,50.0,86.0),
    One(Rectangle::new(1.0,20.0,50.0,23.0));"bigger bottom touch left and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,23.0,500.0,80.0),
    One(Rectangle::new(1.0,20.0,50.0,23.0));"bigger bottom touch bottom and left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,23.0,50.0,80.0),
    One(Rectangle::new(1.0,20.0,50.0,23.0));"bigger bottom touch bottom and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,2.0,33.0,80.2),
    One(Rectangle::new(33.0,20.0,50.0,80.0));"bigger left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,2.0,33.0,80.2),
    One(Rectangle::new(33.0,20.0,50.0,80.0));"bigger left touch left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,2.0,33.0,80.0),
    One(Rectangle::new(33.0,20.0,50.0,80.0));"bigger left touch down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,20.0,33.0,80.2),
    One(Rectangle::new(33.0,20.0,50.0,80.0));"bigger left touch up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,20.0,33.0,80.0),
    One(Rectangle::new(33.0,20.0,50.0,80.0));"bigger left touch up and down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,2.0,33.0,80.0),
    One(Rectangle::new(33.0,20.0,50.0,80.0));"bigger left touch down and left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(1.0,20.0,33.0,80.2),
    One(Rectangle::new(33.0,20.0,50.0,80.0));"bigger left touch up and left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,2.2,51.0,80.2),
    One(Rectangle::new(1.0,20.0,11.0,80.0));"bigger right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,2.2,50.0,80.2),
    One(Rectangle::new(1.0,20.0,11.0,80.0));"bigger right touch right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,2.2,51.0,80.0),
    One(Rectangle::new(1.0,20.0,11.0,80.0));"bigger right touch down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,20.0,51.0,80.2),
    One(Rectangle::new(1.0,20.0,11.0,80.0));"bigger right touch up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,20.0,51.0,80.0),
    One(Rectangle::new(1.0,20.0,11.0,80.0));"bigger right touch up and down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,2.2,50.0,80.0),
    One(Rectangle::new(1.0,20.0,11.0,80.0));"bigger right touch down and right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,20.0,50.0,80.2),
    One(Rectangle::new(1.0,20.0,11.0,80.0));"bigger right touch up and right")]
    fn subtract_overlapping(a: Rectangle, b: Rectangle, exp: CompoundAtomOperationRes<Rectangle>){
        match (a.subtract_overlapping(&b).into(),exp) {
            (Four(ra,rb,rc,rd),Four(ea,eb,ec,ed)) => {
                assert_eq!(ra.as_tuple(),ea.as_tuple());
                assert_eq!(rb.as_tuple(),eb.as_tuple());
                assert_eq!(rc.as_tuple(),ec.as_tuple());
                assert_eq!(rd.as_tuple(),ed.as_tuple());
            },
            (Three(ra,rb,rc),Three(ea,eb,ec)) => {
                assert_eq!(ra.as_tuple(),ea.as_tuple());
                assert_eq!(rb.as_tuple(),eb.as_tuple());
                assert_eq!(rc.as_tuple(),ec.as_tuple());
            },
            (Two(ra,rb),Two(ea,eb)) => {
                assert_eq!(ra.as_tuple(),ea.as_tuple());
                assert_eq!(rb.as_tuple(),eb.as_tuple());
            },
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            _ => panic!("Result doesn't have the expected shape")
        }
    }

    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(3.0,19.0,49.0,79.0),
    One(Rectangle::new(3.0,20.0,49.0,79.0));"small up")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(3.0,22.22,49.0,88.0),
    One(Rectangle::new(3.0,22.22,49.0,80.0));"small down")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(2.0,22.0,55.0,70.0),
    One(Rectangle::new(2.0,22.0,50.0,70.0));"small right")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.0,22.0,2.0,70.0),
    One(Rectangle::new(1.0,22.0,2.0,70.0));"small left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.0,2.0,40.0,70.0),
    One(Rectangle::new(1.0,20.0,40.0,70.0));"top left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,2.0,58.0,60.0),
    One(Rectangle::new(10.0,20.0,50.0,60.0));"top right corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(10.0,25.5,58.0,800.0),
    One(Rectangle::new(10.0,25.5,50.0,80.0));"bottom right corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,25.2,40.0,89.0),
    One(Rectangle::new(1.0,25.2,40.0,80.0));"bottom left corner")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,12.0,30.0,90.0),
    One(Rectangle::new(11.0,20.0,30.0,80.0));"vertical crossing")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.05,25.0,500.0,77.0),
    One(Rectangle::new(1.0,25.0,50.0,77.0));"horizontal crossing")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,2.0,500.0,40.0),
    One(Rectangle::new(1.0,20.0,50.0,40.0));"bigger top")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,23.0,500.0,86.0),
    One(Rectangle::new(1.0,23.0,50.0,80.0));"bigger bottom")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,2.0,33.0,80.2),
    One(Rectangle::new(1.0,20.0,33.0,80.0));"bigger left")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(11.0,2.2,51.0,80.2),
    One(Rectangle::new(11.0,20.0,50.0,80.0));"bigger right")]
    fn intersect_overlapping(a: Rectangle, b: Rectangle, exp: CompoundAtomOperationRes<Rectangle>){
        match (a.intersect_overlapping(&b).into(),exp) {
            (One(r),One(e)) => assert_eq!(r.as_tuple(),e.as_tuple()),
            _ => panic!("Result doesn't have the expected shape")
        }
    }






}
