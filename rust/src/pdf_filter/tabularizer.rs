mod coordinates;
mod collapse;


use coordinates::Limits;
use collapse::{SplittingState,NullableState};








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
struct TableConfig {
    cols: Option<Vec<ColumnConfig>>,
    rows: Option<Vec<RowConfig>>
}




