//! Minimal bridge between `datetime.date` and a plain Rust value, used by any [`Promised`]
//! (`core::promisable`) field that carries a date (e.g. `FundChangeName.date`). Reads
//! year/month/day via plain attribute access (`.year`/`.month`/`.day`) rather than PyO3's
//! `PyDateAccess` trait, because that trait is gated `#[cfg(not(Py_LIMITED_API))]` and this
//! crate builds against the stable ABI (`abi3-py38`), where it isn't available.
//!
//! [`Promised`]: super::promisable::Promised

use pyo3::prelude::*;
use pyo3::types::PyDate;
use pyo3::Borrowed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimpleDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl<'a, 'py> FromPyObject<'a, 'py> for SimpleDate {
    type Error = PyErr;

    fn extract(ob: Borrowed<'a, 'py, PyAny>) -> Result<Self, Self::Error> {
        Ok(SimpleDate {
            year: ob.getattr("year")?.extract()?,
            month: ob.getattr("month")?.extract()?,
            day: ob.getattr("day")?.extract()?,
        })
    }
}

impl<'py> IntoPyObject<'py> for SimpleDate {
    type Target = PyDate;
    type Output = Bound<'py, PyDate>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        PyDate::new(py, self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_python_date() {
        Python::attach(|py| {
            let original = SimpleDate { year: 2025, month: 7, day: 2 };
            let py_date = original.into_pyobject(py).unwrap();
            let back = py_date.extract::<SimpleDate>().unwrap();
            assert_eq!(original, back);
        });
    }
}
