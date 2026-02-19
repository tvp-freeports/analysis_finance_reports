use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use std::fmt;
use ordered_float::OrderedFloat;

#[derive(Debug,Clone,Copy,Hash,Eq,PartialEq)]
pub struct Limits(OrderedFloat<f32>,OrderedFloat<f32>);

#[derive(Debug,Clone,Copy,Hash,Eq,PartialEq)]
pub struct PositiveLimits(Limits);

impl Limits {
    pub fn as_tuple(&self) -> (f32,f32) {
        (self.0.into_inner(),self.1.into_inner())
    }
    pub fn new(a: f32, b: f32) -> Self {
        Self::build(a,b).unwrap_or_else(
            |err| panic!("{err}")
        )
    }
    pub fn build(a: f32, b: f32) -> Result<Self,LimitsBuildError> {
        use LimitsBuildError::*;
        if a >= b {
            Err(NegativeInterval(a,b))
        } else {
            Ok(Self(OrderedFloat(a),OrderedFloat(b)))
        }
    }
}


impl PositiveLimits {
    pub fn as_tuple(&self) -> (f32,f32) {
        self.0.as_tuple()
    }
    pub fn new(a: f32, b: f32) -> Self {
        Self::build(a,b).unwrap_or_else(
            |err| panic!("{err}")
        )
    }
    pub fn build(a: f32, b: f32) -> Result<Self,PositiveLimitsBuildError> {
        use PositiveLimitsBuildError::*;
        if a < 0.0 {
            Err(LeftNegative(a))
        } else if b < 0.0 {
            Err(RightNegative(b))
        } else {
            match Limits::build(a,b) {
                Ok(l) => Ok(Self(l)),
                Err(err) => Err(InvalidLimit(err))
            }
        }
    }
}


impl fmt::Display for Limits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Limits(a,b) = self;
        write!(f,"[{a}:{b}]")
    }
}

impl fmt::Display for PositiveLimits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f,"{}",self.0)
    }
}


impl FromPyObject<'_, '_> for Limits {
    type Error = PyErr;
    fn extract(tuple: Borrowed<'_, '_,PyAny>) -> Result<Self, Self::Error> {
        let a: f32 = tuple.get_item(0)?.extract()?;
        let b: f32 = tuple.get_item(1)?.extract()?;
        Ok(Limits::build(a,b)?)
    }
}

#[derive(Debug,PartialEq)]
pub enum LimitsBuildError {
    NegativeInterval(f32,f32)
}

#[derive(Debug,PartialEq)]
pub enum PositiveLimitsBuildError {
    LeftNegative(f32),
    RightNegative(f32),
    InvalidLimit(LimitsBuildError)
}


impl fmt::Display for LimitsBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NegativeInterval(a,b) => write!(f,"left limit bound can't be bigger than right one, found left '{a}' and right '{b}'")
        }
    }
}


impl fmt::Display for PositiveLimitsBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LeftNegative(a) => write!(f,"left bound of positive limit can't be negative, found '{a}'"),
            Self::RightNegative(b) => write!(f,"right bound of positive limit can't be negative, found '{b}'"),
            Self::InvalidLimit(le) => write!(f,"{le}")
        }
    }
}


impl From<LimitsBuildError> for PyErr {
    fn from(err: LimitsBuildError) -> PyErr {
        PyValueError::new_err(format!("{err}"))
    }
}

impl From<PositiveLimitsBuildError> for PyErr {
    fn from(err: PositiveLimitsBuildError) -> PyErr {
        PyValueError::new_err(format!("{err}"))
    }
}



#[derive(Debug,Clone,Copy,Hash,Eq,PartialEq)]
pub struct Rectangle{
    x0: OrderedFloat<f32>,
    y0: OrderedFloat<f32>,
    x1: OrderedFloat<f32>,
    y1: OrderedFloat<f32>
}

#[derive(PartialEq,Debug)]
pub enum RectangleBuildError {
    Horizontal(LimitsBuildError),
    Vertical(LimitsBuildError)
}
impl fmt::Display for RectangleBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use LimitsBuildError::*;
        match self {
            Self::Horizontal(NegativeInterval(a,b)) => write!(f,"left side of a rectangle can't be bigger than right one, found left '{a}' and right '{b}'"),
            Self::Vertical(NegativeInterval(a,b)) => write!(f,"top side of a rectangle can't be bigger than bottom one, found top '{a}' and bottom '{b}'")
        }
    }
}


impl Rectangle {
    pub fn build(x0: f32, y0: f32, x1: f32, y1: f32) -> Result<Self,RectangleBuildError> {
        Limits::build(x0,x1).map_err(|err| RectangleBuildError::Horizontal(err))?;
        Limits::build(y0,y1).map_err(|err| RectangleBuildError::Vertical(err))?;
        Ok(Self {
            x0: OrderedFloat(x0),
            y0: OrderedFloat(y0),
            x1: OrderedFloat(x1),
            y1: OrderedFloat(y1)
        })
    }
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::build(x0,y0,x1,y1).unwrap_or_else(
            |err| panic!("{err}")
        )
    }
    pub fn as_tuple(&self) -> (f32,f32,f32,f32) {
        (
            self.x0.into_inner(),
            self.y0.into_inner(),
            self.x1.into_inner(),
            self.y1.into_inner()
        )
    }
}





#[cfg(test)]
mod tests {
    use super::*;
    mod limits {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;
        use LimitsBuildError::*;
        #[test]
        fn build_ok() {
            match Limits::build(20.3,30.7) {
                Ok(Limits(a,b)) => {
                    assert_eq!(a,20.3);
                    assert_eq!(b,30.7);
                },
                Err(err) => panic!("Found err variant: {err}")
            }
        }
        #[test_case(30.1,20.0,NegativeInterval(30.1, 20.0);"invalid interval")]
        fn build_err(a: f32,b: f32, err: LimitsBuildError) {
            match Limits::build(a,b) {
                Err(e) => assert_eq!(e,err),
                Ok(l) => panic!("Expected error '{err}', found limit {l}")
            }
        }
        #[test_case(
            Ok(Limits(OrderedFloat(-10.1),OrderedFloat(20.1))),
            "[-10.1:20.1]";
            "limit"
        )]
        #[test_case(
            Err(NegativeInterval(30.1, 20.0)),
            "left limit bound can't be bigger than right one, found left '30.1' and right '20'";
            "invalid interval"
        )]
        fn format(x: Result<Limits,LimitsBuildError>,expected: &str) {
            match x {
                Ok(value) => assert_eq!(format!("{value}"),expected),
                Err(err) => assert_eq!(format!("{err}"),expected)
            }
        }
        #[test]
        fn new_success() {
            let Limits(a,b) = Limits::new(-20.3,30.7);
            assert_eq!(a,-20.3);
            assert_eq!(b,30.7);   
        }
        #[test]
        #[should_panic = "left limit bound can't be bigger than right one, found left '22.2' and right '11.1'"]
        fn new_panic_invalid_interval() {
            Limits::new(22.2,11.1);
        }
        #[test]
        fn as_tuple() {
            let limits = Limits::new(9.0,99.0);
            let tuple = (9.0,99.0);
            assert_eq!(limits.as_tuple(),tuple);
        }

    }

    mod positive_limits {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;
        use PositiveLimitsBuildError::*;
        #[test]
        fn build_ok() {
            match PositiveLimits::build(20.3,30.7) {
                Ok(PositiveLimits(Limits(a,b))) => {
                    assert_eq!(a,20.3);
                    assert_eq!(b,30.7);
                },
                Err(err) => panic!("Found err variant: {err}")
            }
        }
        #[test_case(-20.0,30.1,LeftNegative(-20.0);"left negative")]
        #[test_case(20.0,-30.1,RightNegative(-30.1);"right negative")]
        #[test_case(30.1,20.0,InvalidLimit(LimitsBuildError::NegativeInterval(30.1, 20.0));"invalid interval")]
        fn build_err(a: f32,b: f32, err: PositiveLimitsBuildError) {
            match PositiveLimits::build(a,b) {
                Err(e) => assert_eq!(e,err),
                Ok(l) => panic!("Expected error '{err}', found limit {l}")
            }
        }
        #[test_case(
            Ok(PositiveLimits(Limits(OrderedFloat(10.1),OrderedFloat(20.1)))),
            "[10.1:20.1]";
            "limit"
        )]
        #[test_case(
            Err(LeftNegative(-20.1)),
            "left bound of positive limit can't be negative, found '-20.1'";
            "left negative"
        )]
        #[test_case(
            Err(RightNegative(-30.1)),
            "right bound of positive limit can't be negative, found '-30.1'";
            "right negative"
        )]
        #[test_case(
            Err(InvalidLimit(LimitsBuildError::NegativeInterval(30.1, 20.0))),
            "left limit bound can't be bigger than right one, found left '30.1' and right '20'";
            "invalid interval"
        )]
        fn format(x: Result<PositiveLimits,PositiveLimitsBuildError>,expected: &str) {
            match x {
                Ok(value) => assert_eq!(format!("{value}"),expected),
                Err(err) => assert_eq!(format!("{err}"),expected)
            }
        }
        #[test]
        fn new_success() {
            let PositiveLimits(Limits(a,b)) = PositiveLimits::new(20.3,30.7);
            assert_eq!(a,20.3);
            assert_eq!(b,30.7);
        }
        #[test]
        #[should_panic = "left bound of positive limit can't be negative, found '-20.3'"]
        fn new_panic_left_negative() {
            PositiveLimits::new(-20.3,31.1);
        }
        #[test]
        #[should_panic = "right bound of positive limit can't be negative, found '-35.1'"]
        fn new_panic_right_negative() {
            PositiveLimits::new(25.67,-35.1);
        }
        #[test]
        #[should_panic = "left limit bound can't be bigger than right one, found left '22.2' and right '11.1'"]
        fn new_panic_invalid_interval() {
            PositiveLimits::new(22.2,11.1);
        }
        #[test]
        fn as_tuple() {
            let limits = PositiveLimits::new(9.0,99.0);
            let tuple = (9.0,99.0);
            assert_eq!(limits.as_tuple(),tuple);
        }

    }
    mod rectangle {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;
        use LimitsBuildError::*;
        use RectangleBuildError::*;
        #[test]
        fn build_ok() {
            match Rectangle::build(20.3,1.0,200.3,5.0) {
                Ok(Rectangle{x0,y0,x1,y1}) => {
                    assert_eq!(x0,20.3);
                    assert_eq!(y0,1.0);
                    assert_eq!(x1,200.3);
                    assert_eq!(y1,5.0);
                },
                Err(err) => panic!("Found err variant: {err}")
            }
        }
        #[test_case(30.1,0.1,20.0,0.2,Horizontal(NegativeInterval(30.1, 20.0));"invalid width")]
        #[test_case(20.0,0.2,30.1,0.1,Vertical(NegativeInterval(0.2, 0.1));"invalid height")]
        fn build_err(x0: f32, y0: f32, x1: f32, y1: f32, err: RectangleBuildError) {
            match Rectangle::build(x0,y0,x1,y1) {
                Err(e) => assert_eq!(e,err),
                Ok(r) => panic!("Expected error '{err}', found rectangle {r:?}")
            }
        }
        #[test_case(
            Horizontal(NegativeInterval(302.1, 20.0)),
            "left side of a rectangle can't be bigger than right one, found left '302.1' and right '20'";
            "invalid width"
        )]
        #[test_case(
            Vertical(NegativeInterval(300.1, 202.0)),
            "top side of a rectangle can't be bigger than bottom one, found top '300.1' and bottom '202'";
            "invalid height"
        )]
        fn format(err: RectangleBuildError,expected: &str) {
            assert_eq!(format!("{err}"),expected);
        }
        #[test]
        fn new_success() {
            let Rectangle{x0,y0,x1,y1} = Rectangle::new(20.3,8.0,30.7,78.3);
            assert_eq!(x0,20.3);
            assert_eq!(x1,30.7);
            assert_eq!(y0,8.0);
            assert_eq!(y1,78.3);
        }
        #[test]
        #[should_panic = "left side of a rectangle can't be bigger than right one, found left '22.2' and right '11.1'"]
        fn new_panic_invalid_width() {
            Rectangle::new(22.2,400.0,11.1,444.0);
        }
        #[test]
        #[should_panic = "top side of a rectangle can't be bigger than bottom one, found top '480' and bottom '444'"]
        fn new_panic_invalid_height() {
            Rectangle::new(2.54,480.0,11.1,444.0);
        }
        
        #[test]
        fn as_tuple() {
            let rectangle = Rectangle::new(9.0,6.2,99.0,22.3);
            let tuple = (9.0,6.2,99.0,22.3);
            assert_eq!(rectangle.as_tuple(),tuple);
        }
    }
}