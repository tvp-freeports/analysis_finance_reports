use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use std::fmt;

#[derive(Debug,Clone,Copy)]
pub struct Limits(pub f32, pub f32);

impl Limits {
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



#[cfg(test)]
mod tests {
    use super::*;
    mod limits {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case; 
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
        #[test_case(-20.0,30.1,LimitsBuildError::LeftNegative(-20.0);"left_negative")]
        #[test_case(20.0,-30.1,LimitsBuildError::RightNegative(-30.1);"right_negative")]
        #[test_case(30.1,20.0,LimitsBuildError::NegativeInterval(30.1, 20.0);"invalid_interval")]
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
            Err(LimitsBuildError::LeftNegative(-20.1)),
            "left bound of limit can't be negative, found '-20.1'";
            "left_negative"
        )]
        #[test_case(
            Err(LimitsBuildError::RightNegative(-30.1)),
            "right bound of limit can't be negative, found '-30.1'";
            "right_negative"
        )]
        #[test_case(
            Err(LimitsBuildError::NegativeInterval(30.1, 20.0)),
            "left limit bound can't be bigger than right one, found left '30.1' and right '20'";
            "invalid_interval"
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
    }
}