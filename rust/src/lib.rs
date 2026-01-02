
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
    use pyo3::Bound;
    use pyo3::prelude::*;
    #[pyo3::pymodule]
    mod pdf_filter {
        use pyo3::Bound;
        use pyo3::prelude::*;
        #[pymodule_init]
        fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
            // Arbitrary code to run at the module initialization
            m.setattr("__package__", "freeports_lib.pdf_filter")?;
            m.getattr("tabularizer")?.setattr("__name__","freeports_lib.pdf_filter.tabularizer")

        }
        #[pyo3::pymodule]
        mod tabularizer {
            use pyo3::Bound;
            use pyo3::prelude::*;
            #[pymodule_init]
            fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
                // Arbitrary code to run at the module initialization
                m.setattr("__package__", "freeports_lib.pdf_filter")
            }
            #[pymodule_export]
            use crate::pdf_filter::tabularizer::{
                collapse_table_rows,
                TableConfig,
                ColumnConfig,
                RowConfig,
                CellGeometry,
            };
            #[pymodule_export]
            use crate::pdf_filter::tabularizer::py_get_table_coordinates;

        }
    }
    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        // Arbitrary code to run at the module initialization
        m.setattr("__package__", "freeports_lib")?;
        m.getattr("pdf_filter")?.setattr("__name__","freeports_lib.pdf_filter")
    }
}

