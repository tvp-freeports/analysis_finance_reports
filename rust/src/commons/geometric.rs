use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;

#[derive(Debug,Clone,Copy)]
pub struct Limits(pub f32, pub f32);

impl Limits {
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
impl FromPyObject<'_, '_> for Limits {
    type Error = PyErr;
    fn extract(tuple: Borrowed<'_, '_,PyAny>) -> Result<Self, Self::Error> {
        let a: f32 = tuple.get_item(0)?.extract()?;
        let b: f32 = tuple.get_item(1)?.extract()?;
        Ok(Limits::build(a,b)?)
    }
}

#[derive(Debug)]
pub enum LimitsBuildError {
    LeftNegative(f32),
    RightNegative(f32),
    NegativeInterval(f32,f32)
}
impl From<LimitsBuildError> for PyErr {
    fn from(err: LimitsBuildError) -> PyErr {
        PyValueError::new_err(format!("{err:?}"))
    }
}



#[cfg(test)]
mod tests {
    use super::*;    
    mod limits_build {
        use super::*;
        #[test]
        fn ok() {
            assert!(matches!(
                Limits::build(20.3,30.7),
                Ok(Limits(20.3,30.7))
            ));
        }
        #[test]
        fn err() {
            use LimitsBuildError::*;
            assert!(matches!(
                Limits::build(-20.0, 30.1),
                Err(LeftNegative(-20.0))
            ));
            assert!(matches!(
                Limits::build(20.0, -30.1),
                Err(RightNegative(-30.1))
            ));
            assert!(matches!(
                Limits::build(30.1, 20.0),
                Err(NegativeInterval(30.1, 20.0))
            ));
        }
    }
}