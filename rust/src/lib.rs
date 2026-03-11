
pub mod commons {
    pub mod geometry;
    pub mod sets;
}

pub mod pdf_extract {
    pub mod tabularizer;
    pub mod select;
}

pub mod text_filter {
    pub mod matcher;
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pyo3::pymodule]
mod freeports_lib {
    #[pyo3::pymodule]
    mod pdf_extract {
        #[pyo3::pymodule]
        mod tabularizer {
            #[pymodule_export]
            use crate::pdf_extract::tabularizer::{
                py_get_table_coordinates,
                py_collapse_table_rows
            };
        }
        #[pyo3::pymodule]
        mod select {
            #[pymodule_export]
            use crate::pdf_extract::select::{
                PyPdfLineSelection,
                PyPdfLine
            };
        }

    }
    #[pyo3::pymodule]
    mod text_filter {
        #[pyo3::pymodule]
        mod matcher {
            #[pymodule_export]
            use crate::text_filter::matcher::{
                py_match_company,
                CompanyMatchInfos
            };
        }
    }
}


