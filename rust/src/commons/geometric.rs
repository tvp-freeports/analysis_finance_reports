use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use std::fmt;

#[derive(Debug,Clone,Copy)]
pub struct Limits(f32, f32);

impl Limits {
    pub fn as_tuple(&self) -> (f32,f32) {
        (self.0,self.1)
    }
    pub fn new(a: f32, b: f32) -> Self {
        Self::build(a,b).unwrap_or_else(
            |err| panic!("{err}")
        )
    }
    pub fn build(a: f32, b: f32) -> Result<Self,LimitsBuildError> {
        use LimitsBuildError::*;
        if a < 0.0 {
            Err(LeftNegative(a))
        } else if b < 0.0 {
            Err(RightNegative(b))
        } else if a >= b {
            Err(NegativeInterval(a,b))
        } else {
            Ok(Self(a,b))
        }
    }
}
impl fmt::Display for Limits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Limits(a,b) = self;
        write!(f,"[{a}:{b}]")
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
    LeftNegative(f32),
    RightNegative(f32),
    NegativeInterval(f32,f32)
}

impl fmt::Display for LimitsBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LeftNegative(a) => write!(f,"left bound of limit can't be negative, found '{a}'"),
            Self::RightNegative(b) => write!(f,"right bound of limit can't be negative, found '{b}'"),
            Self::NegativeInterval(a,b) => write!(f,"left limit bound can't be bigger than right one, found left '{a}' and right '{b}'")
        }
    }
}

impl From<LimitsBuildError> for PyErr {
    fn from(err: LimitsBuildError) -> PyErr {
        PyValueError::new_err(format!("{err}"))
    }
}




#[derive(Debug,Clone,Copy)]
pub struct Rectangle{
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32
}

#[derive(PartialEq,Debug)]
enum RectangleBuildError {
    Horizontal(LimitsBuildError),
    Vertical(LimitsBuildError)
}
impl fmt::Display for RectangleBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use LimitsBuildError::*;
        match self {
            Self::Horizontal(LeftNegative(a)) => write!(f,"left bound of rectangle can't be negative, found '{a}'"),
            Self::Horizontal(RightNegative(b)) => write!(f,"right bound of rectangle can't be negative, found '{b}'"),
            Self::Horizontal(NegativeInterval(a,b)) => write!(f,"left side of a rectangle can't be bigger than right one, found left '{a}' and right '{b}'"),
            Self::Vertical(LeftNegative(a)) => write!(f,"top bound of rectangle can't be negative, found '{a}'"),
            Self::Vertical(RightNegative(b)) => write!(f,"bottom bound of rectangle can't be negative, found '{b}'"),
            Self::Vertical(NegativeInterval(a,b)) => write!(f,"top side of a rectangle can't be bigger than bottom one, found top '{a}' and bottom '{b}'")
        }
    }
}


impl Rectangle {
    fn build(x0: f32, y0: f32, x1: f32, y1: f32) -> Result<Self,RectangleBuildError> {
        Limits::build(x0,x1).map_err(|err| RectangleBuildError::Horizontal(err))?;
        Limits::build(y0,y1).map_err(|err| RectangleBuildError::Vertical(err))?;
        Ok(Self {x0,y0,x1,y1})
    }
    fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::build(x0,y0,x1,y1).unwrap_or_else(
            |err| panic!("{err}")
        )
    }
    fn as_tuple(&self) -> (f32,f32,f32,f32) {
        (self.x0,self.y0,self.x1,self.y1)
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
        #[test_case(-20.0,30.1,LeftNegative(-20.0);"left negative")]
        #[test_case(20.0,-30.1,RightNegative(-30.1);"right negative")]
        #[test_case(30.1,20.0,NegativeInterval(30.1, 20.0);"invalid interval")]
        fn build_err(a: f32,b: f32, err: LimitsBuildError) {
            match Limits::build(a,b) {
                Err(e) => assert_eq!(e,err),
                Ok(l) => panic!("Expected error '{err}', found limit {l}")
            }
        }
        #[test_case(
            Ok(Limits(10.1,20.1)),
            "[10.1:20.1]";
            "limit"
        )]
        #[test_case(
            Err(LeftNegative(-20.1)),
            "left bound of limit can't be negative, found '-20.1'";
            "left negative"
        )]
        #[test_case(
            Err(RightNegative(-30.1)),
            "right bound of limit can't be negative, found '-30.1'";
            "right negative"
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
            let Limits(a,b) = Limits::new(20.3,30.7);
            assert_eq!(a,20.3);
            assert_eq!(b,30.7);   
        }
        #[test]
        #[should_panic = "left bound of limit can't be negative, found '-20.3'"]
        fn new_panic_left_negative() {
            Limits::new(-20.3,31.1);
        }
        #[test]
        #[should_panic = "right bound of limit can't be negative, found '-35.1'"]
        fn new_panic_right_negative() {
            Limits::new(25.67,-35.1);
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
        #[test_case(-20.0,0.1,30.1,0.2,Horizontal(LeftNegative(-20.0));"left negative")]
        #[test_case(20.0,0.1,-30.1,0.2,Horizontal(RightNegative(-30.1));"right negative")]
        #[test_case(30.1,0.1,20.0,0.2,Horizontal(NegativeInterval(30.1, 20.0));"invalid width")]
        #[test_case(20.0,-0.1,30.1,0.2,Vertical(LeftNegative(-0.1));"top negative")]
        #[test_case(20.0,0.1,30.1,-0.2,Vertical(RightNegative(-0.2));"bottom negative")]
        #[test_case(20.0,0.2,30.1,0.1,Vertical(NegativeInterval(0.2, 0.1));"invalid height")]
        fn build_err(x0: f32, y0: f32, x1: f32, y1: f32, err: RectangleBuildError) {
            match Rectangle::build(x0,y0,x1,y1) {
                Err(e) => assert_eq!(e,err),
                Ok(r) => panic!("Expected error '{err}', found rectangle {r:?}")
            }
        }
        #[test_case(
            Horizontal(LeftNegative(-208.1)),
            "left bound of rectangle can't be negative, found '-208.1'";
            "left negative"
        )]
        #[test_case(
            Horizontal(RightNegative(-302.1)),
            "right bound of rectangle can't be negative, found '-302.1'";
            "right negative"
        )]
        #[test_case(
            Horizontal(NegativeInterval(302.1, 20.0)),
            "left side of a rectangle can't be bigger than right one, found left '302.1' and right '20'";
            "invalid width"
        )]
        #[test_case(
            Vertical(LeftNegative(-208.1)),
            "top bound of rectangle can't be negative, found '-208.1'";
            "top negative"
        )]
        #[test_case(
            Vertical(RightNegative(-30.1)),
            "bottom bound of rectangle can't be negative, found '-30.1'";
            "bottom negative"
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
        #[should_panic = "left bound of limit can't be negative, found '-20.3'"]
        fn new_panic_left_negative() {
            todo!();
            Limits::new(-20.3,31.1);
        }
        #[test]
        #[should_panic = "right bound of limit can't be negative, found '-35.1'"]
        fn new_panic_right_negative() {
            todo!();
            Limits::new(25.67,-35.1);
        }
        #[test]
        #[should_panic = "left limit bound can't be bigger than right one, found left '22.2' and right '11.1'"]
        fn new_panic_invalid_interval() {
            todo!();
            Limits::new(22.2,11.1);
        }
        #[test]
        fn as_tuple() {
            let rectangle = Rectangle::new(9.0,6.2,99.0,22.3);
            let tuple = (9.0,6.2,99.0,22.3);
            assert_eq!(rectangle.as_tuple(),tuple);
        }
    }
}