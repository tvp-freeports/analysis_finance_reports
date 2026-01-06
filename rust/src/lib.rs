
use pyo3::Bound;
use pyo3::prelude::*;

pub mod text_extract;

mod pdf_filter {
    pub mod tabularizer;
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pyo3::pymodule]
mod freeports_lib {
    #[pyo3::pymodule]
    mod pdf_filter {
        #[pyo3::pymodule]
        mod tabularizer {
            #[pymodule_export]
            use crate::pdf_filter::tabularizer::{
                py_get_table_coordinates,
                py_collapse_table_rows
            };
        }
    }
}

