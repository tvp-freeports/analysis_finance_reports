use super::{Container,Set,AstNode};

use crate::commons::geometric::Rectangle;

type AreaAstLeaf = Rectangle;

impl Container for Rectangle {
    type Elem = Self;
    fn contains(&self, other: &Self) -> bool {
        let (x0,y0,x1,y1) = self.as_tuple();
        let (z0,w0,z1,w1) = other.as_tuple();
        (x0<=z0  && z1<=x1) && (y0<=w0 && w1<=y1)
    }
}


impl Area {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self(AstNode::Leaf(Rectangle::new(x0,y0,x1,y1)))
    }
}


pub type Area = Set<Rectangle,Rectangle>;
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
        let area=Rectangle::new(0.0,20.0,50.0,80.0);
        let rec = Rectangle::new(3.0,29.89,49.0,79.0);
        assert!(area.contains(&rec));
    }
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(300.0,290.89,490.0,790.0);"outside")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.2,19.89,55.0,88.0);"around")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(3.0,13.11,49.0,81.0);"cross")]
    #[test_case(Rectangle::new(1.0,20.0,50.0,80.0),Rectangle::new(0.5,29.89,49.0,79.0);"left side")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(30.0,29.89,49.0,793.0);"right side")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(30.0,9.89,49.0,79.0);"top")]
    #[test_case(Rectangle::new(0.0,20.0,50.0,80.0),Rectangle::new(30.0,29.89,50.0002,79.0);"bottom")]
    fn element_not_in_leafset(area: Rectangle, rec: Rectangle) {
        assert!(!area.contains(&rec))
    }
}