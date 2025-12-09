use pyo3::prelude::*;

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
pub mod freeports_lib {
    use pyo3::prelude::*;
    use std::env;

    /// Formats the sum of two numbers as string.
    #[pyfunction]
    pub fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
        Ok((a + b + 10).to_string())
    }

    /// Calls Python (see https://pyo3.rs for details)
    #[pyfunction]
    fn demo(py: Python) -> PyResult<()> {
        let argv = env::args().collect::<Vec<_>>();
        println!("argv = {:?}", argv);
        // argv[0]: Python path, argv[1]: program name, argv[2..]: given args

        let numbers: Vec<i32> = argv[2..].iter().map(|s| s.parse().unwrap()).collect();

        let python_sum = PyModule::import(py, "builtins")?.getattr("sum")?;
        let total: i32 = python_sum.call1((numbers,))?.extract()?;
        println!("sum({}) = {:?}", argv[2..].join(", "), total);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::freeports_lib::sum_as_string;
    
    #[test]
    fn test_sum_as_string() {
        assert_eq!(sum_as_string(1, 2).unwrap(), "13");
    }
}

    
