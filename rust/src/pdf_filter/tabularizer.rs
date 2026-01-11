use pyo3::prelude::*;

mod coordinates;
mod collapse;

pub use coordinates::{
    Limits,
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

#[derive(Clone,Debug,FromPyObject)]
pub struct ColumnConfig {
    limits: Option<Limits>,
    splitting: Option<SplittingState>,
    nullable: Option<NullableState>
}

#[derive(Clone,Debug,FromPyObject)]
pub struct RowConfig {
    limits: Option<Limits>
}


#[derive(Clone,Debug,FromPyObject)]
pub struct TableConfig {
    cols: Option<Vec<ColumnConfig>>,
    rows: Option<Vec<RowConfig>>
}


