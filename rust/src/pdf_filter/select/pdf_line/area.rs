use crate::commons::sets::{Container,Set,AstNode,SetOps,Overlappable,SetRelation};
use crate::commons::geometry::Rectangle;

type AreaAstLeaf = Rectangle;

impl Container for Rectangle {
    type Elem = (f32,f32);
    fn contains(&self, point: &(f32,f32)) -> bool {
        let (x0,y0,x1,y1) = self.as_tuple();
        ( x0 <= point.0 && point.0 <= x1 ) && ( y0 <= point.1 && point.1 <= y1 )
    }
}

impl Overlappable for Rectangle {
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


impl Area {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self(AstNode::Leaf(Rectangle::new(x0,y0,x1,y1)))
    }
}


// pub type Area = Set<Rectangle,Rectangle>;
pub type Area = Set<Rectangle,(f32,f32)>;
pub type AreaSet = Area;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    #[test]
    fn new_area() {
        let (x0,y0,x1,y1) = (0.2,9.0,0.4,10.0);
        let res = (0.2,9.0,0.4,10.0);
        let Set(AstNode::Leaf(rec)) = Area::new(x0,y0,x1,y1) else {
            panic!("Expected have to be a AreaSet with just one leaf")
        };
        assert_eq!(rec.as_tuple(),res);
    }

    #[test]
    fn element_in_leafset() {
        let rec=Rectangle::new(0.0,20.0,50.0,80.0);
        let point = (3.0,29.89);
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

    // #[test]
    // fn element_in_leafset() {
    //     let area=Rectangle::new(0.0,20.0,50.0,80.0);
    //     let rec = Rectangle::new(3.0,29.89,49.0,79.0);
    //     assert!(area.contains(&rec));
    // }
    // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(300.0,290.89,490.0,790.0);"outside")]
    // #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,19.89,55.0,88.0);"around")]
    // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(3.0,13.11,49.0,81.0);"cross")]
    // #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.5,29.89,49.0,79.0);"left side")]
    // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(30.0,29.89,49.0,793.0);"right side")]
    // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(30.0,9.89,49.0,79.0);"top")]
    // #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(30.0,29.89,50.0002,79.0);"bottom")]
    // fn element_not_in_leafset(area: Rectangle, rec: Rectangle) {
    //     assert!(!area.contains(&rec))
    // }
}