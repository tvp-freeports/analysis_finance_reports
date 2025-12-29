mod coordinates;
mod collapse;


use coordinates::Limits;
use collapse::{SplittingState,NullableState};


pub use coordinates::{get_table_coordinates,TablePosAlgorithm,CellGeometry};
pub use collapse::{collapse_table_rows,CollapseAlgorithm};

#[derive(Clone,Debug)]
struct ColumnConfig {
    limits: Option<Limits>,
    splitting: Option<SplittingState>,
    nullable: Option<NullableState>
}

#[derive(Clone,Debug)]
struct RowConfig {
    limits: Option<Limits>
}

#[derive(Clone,Debug)]
pub struct TableConfig {
    cols: Option<Vec<ColumnConfig>>,
    rows: Option<Vec<RowConfig>>
}




