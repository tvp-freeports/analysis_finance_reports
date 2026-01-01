use pyo3::prelude::*;
use pyo3::pyclass;

mod coordinates;
mod collapse;


use coordinates::Limits;
use collapse::{NullableState};

pub use coordinates::{get_table_coordinates,TablePosAlgorithm,CellGeometry,py_get_table_coordinates};
pub use collapse::{collapse_table_rows,CollapseAlgorithm,SplittingDirection,SplittingState};

#[pyclass]
#[derive(Clone,Debug)]
pub struct ColumnConfig {
    limits: Option<Limits>,
    splitting: Option<SplittingState>,
    nullable: Option<NullableState>
}
#[pymethods]
impl ColumnConfig {
    #[new]
    fn py_new(limits: Option<Limits>, splitting: Option<SplittingState>, nullable: Option<NullableState> ) -> Self {
        ColumnConfig{
            limits,
            splitting,
            nullable
        }
    }
}


#[pyclass]
#[derive(Clone,Debug)]
pub struct RowConfig {
    limits: Option<Limits>
}
#[pymethods]
impl RowConfig {
    #[new]
    fn py_new(limits: Option<(f32,f32)>) -> Self {
        RowConfig{
            limits: match limits {
                Some((a,b)) => Some(Limits::build(a,b).unwrap()),
                None => None
            }
        }
    }
}



#[pyclass]
#[derive(Clone,Debug)]
pub struct TableConfig {
    cols: Option<Vec<ColumnConfig>>,
    rows: Option<Vec<RowConfig>>
}

#[pymethods]
impl TableConfig {
    #[new]
    fn py_new(cols: Option<Vec<ColumnConfig>>, rows: Option<Vec<RowConfig>>) -> Self {
        TableConfig{
            cols,
            rows
        }
    }
}



