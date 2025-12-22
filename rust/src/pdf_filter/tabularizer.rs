mod coordinates;
mod collapse;


use coordinates::Limits;
use collapse::SplittingState;








#[derive(Clone)]
struct ColumnConfig {
    limits: Option<Limits>,
    splitting: Option<SplittingState>,
    nullable: Option<bool>
}

#[derive(Clone)]
struct RowConfig {
    limits: Option<Limits>
}

#[derive(Clone)]
struct TableConfig {
    cols: Option<Vec<ColumnConfig>>,
    rows: Option<Vec<RowConfig>>
}




