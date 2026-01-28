mod coordinates;
mod collapse;

use pyo3::prelude::*;
use crate::commons::geometric::Limits;

pub use coordinates::{
    CellGeometry,
    TablePosAlgorithm,
    get_table_coordinates,    
    py_get_table_coordinates
};
pub use collapse::{
    NullableState,
    SplittingState,
    SplittingDirection,
    CollapseAlgorithm,
    collapse_table_rows,
    py_collapse_table_rows
};

#[derive(Clone,Debug,FromPyObject,Copy)]
pub struct ColumnConfig {
    limits: Option<Limits>,
    splitting: Option<SplittingState>,
    nullable: Option<NullableState>
}

#[derive(Clone,Debug,FromPyObject,Copy)]
pub struct RowConfig {
    limits: Option<Limits>
}


#[derive(Clone,Debug,FromPyObject)]
pub struct TableConfig {
    cols: Option<Vec<ColumnConfig>>,
    rows: Option<Vec<RowConfig>>
}


