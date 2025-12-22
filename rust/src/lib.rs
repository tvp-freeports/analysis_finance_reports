use pyo3::prelude::*;

mod text_extract;
mod pdf_filter {
    mod tabularizer;
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
pub mod freeports_lib {
    // use super::pdf_filter;
    // use super::text_extract;
}

