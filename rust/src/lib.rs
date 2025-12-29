use pyo3::prelude::*;

mod text_extract;
pub mod pdf_filter {
    pub mod tabularizer;
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
pub mod freeports_lib {
    pub use super::pdf_filter;
    // use super::text_extract;
}

