
pub mod pdf_filter {
    pub mod tabularizer;
    pub mod select;
}

pub mod text_extract {
    pub mod matcher;
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
    #[pyo3::pymodule]
    mod text_extract {
        #[pyo3::pymodule]
        mod matcher {
            #[pymodule_export]
            use crate::text_extract::matcher::{
                py_match_company,
                CompanyMatchInfos
            };
        }
    }
}


