use super::Container;
use crate::commons::geometric::Limits;

type Interval = Limits;

impl Container for Interval {
    type Elem = f32;
    fn contains(&self,x: &f32) -> bool {
        let Self(a,b) = self;
        a <= x && x <= b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    #[test]
    fn element_in_leafset() {
        let interval=Interval::build(20.0,50.0).unwrap();
        let x=30.5;
        assert!(interval.contains(&x));
    }
    #[test_case(10.5;"too little")]
    #[test_case(55.5;"too big")]
    fn element_not_in_leafset(x: f32) {
        let interval=Interval::build(20.0,50.0).unwrap();
        assert!(!interval.contains(&x));
    }
}