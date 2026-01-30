use crate::commons::sets::{Container,Set,AstNode};
use crate::commons::geometry::Limits;

type FontSizeAstLeaf = Limits;

impl Container for Limits {
    type Elem = f32;
    fn contains(&self,x: &f32) -> bool {
        let (a,b) = self.as_tuple();
        a <= *x && *x <= b
    }
}


impl FontSizeSet {
    pub fn new(a: f32, b: f32) -> Self {
        Self(AstNode::Leaf(Limits::new(a,b)))
    }
}


pub type FontSizeSet = Set<Limits,f32>;
pub type FontSizeInterval = FontSizeSet;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;
    #[test]
    fn new_fontsizeset() {
        let (a,b) = (0.2,0.4);
        let res = (0.2,0.4);
        let Set(AstNode::Leaf(interval)) = FontSizeSet::new(a,b) else {
            panic!("Expected have to be a FontSizeSet with just one leaf")
        };
        assert_eq!(interval.as_tuple(),res);
    }

    #[test]
    fn element_in_leafset() {
        let interval=Limits::build(20.0,50.0).unwrap();
        let x=30.5;
        assert!(interval.contains(&x));
    }
    #[test_case(10.5;"too little")]
    #[test_case(55.5;"too big")]
    fn element_not_in_leafset(x: f32) {
        let interval=Limits::build(20.0,50.0).unwrap();
        assert!(!interval.contains(&x));
    }
}