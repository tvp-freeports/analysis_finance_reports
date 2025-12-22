use super::{TableConfig,ColumnConfig};

#[derive(Clone,Copy,PartialEq)]
enum SplittingDirection {
    Up,
    Down
}

#[derive(Clone,Copy)]
pub enum SplittingState {
    Allow(SplittingDirection),
    Disallow
}


fn collapse_table_rows_by_geometry(
    indexes: Vec<(usize, usize)>,
    table_config: &TableConfig,
) -> Vec<(usize, usize)> {
    // Early return if no indexes
    if indexes.is_empty() {
        return indexes;
    }

    // Build column configurations
    let column_configs = build_column_configs(table_config);
    
    // Determine table dimensions and build existence matrix
    let (row_count, col_count, cell_exists) = build_existence_matrix(&indexes);
    
    // Analyze which rows can collapse
    let row_collapse_info = analyze_collapsible_rows(
        row_count, 
        col_count, 
        &cell_exists, 
        &column_configs
    );
    
    // Calculate target rows for collapsing
    let target_rows = calculate_target_rows(&row_collapse_info);
    
    // Apply row collapsing to indexes
    apply_row_collapsing(indexes, &target_rows)
}

// Helper function: Extract column configurations from table config
fn build_column_configs(table_config: &TableConfig) -> Vec<(SplittingState, bool)> {
    let tmp_table_cfg = table_config.clone();
    
    tmp_table_cfg
        .cols
        .unwrap()
        .into_iter()
        .map(|col_config| {
            let splitting = col_config
                .splitting
                .unwrap_or(SplittingState::Allow(SplittingDirection::Down));
            let nullable = col_config.nullable.unwrap_or(false);
            (splitting, nullable)
        })
        .collect()
}

// Helper function: Determine table size and which cells exist
fn build_existence_matrix(
    indexes: &[(usize, usize)]
) -> (usize, usize, Vec<Vec<bool>>) {
    // Find table boundaries
    let max_row = indexes.iter().map(|&(row, _)| row).max().unwrap_or(0);
    let max_col = indexes.iter().map(|&(_, col)| col).max().unwrap_or(0);
    
    let row_count = max_row + 1;
    let col_count = max_col + 1;

    // Build existence matrix
    let mut cell_exists = vec![vec![false; col_count]; row_count];
    for &(row, col) in indexes {
        cell_exists[row][col] = true;
    }

    (row_count, col_count, cell_exists)
}

// Helper function: Analyze which rows can be collapsed
fn analyze_collapsible_rows(
    row_count: usize,
    col_count: usize,
    cell_exists: &[Vec<bool>],
    column_configs: &[(SplittingState, bool)],
) -> Vec<Option<SplittingDirection>> {
    let mut row_can_collapse = vec![None; row_count];

    for row in 0..row_count {
        // Check if row can collapse
        if let Some(direction) = can_row_collapse(
            row, 
            col_count, 
            cell_exists, 
            column_configs
        ) {
            row_can_collapse[row] = Some(direction);
        }
    }

    row_can_collapse
}

// Helper function: Check if a specific row can collapse
fn can_row_collapse(
    row: usize,
    col_count: usize,
    cell_exists: &[Vec<bool>],
    column_configs: &[(SplittingState, bool)],
) -> Option<SplittingDirection> {
    // Full rows don't collapse
    let is_full_row = (0..col_count).all(|col| cell_exists[row][col]);
    if is_full_row {
        return None;
    }

    let mut collapse_direction: Option<SplittingDirection> = None;

    for col in 0..col_count {
        let (splitting, nullable) = column_configs
            .get(col)
            .copied()
            .unwrap_or((SplittingState::Allow(SplittingDirection::Down), false));

        if cell_exists[row][col] {
            // Existing cell - must be splittable
            match splitting {
                SplittingState::Disallow => return None,
                SplittingState::Allow(direction) => {
                    // Check for consistent direction
                    match collapse_direction {
                        Some(existing) if existing != direction => return None,
                        _ => collapse_direction = Some(direction),
                    }
                }
            }
        } else {
            // Missing cell - must be nullable
            if !nullable {
                return None;
            }
        }
    }

    collapse_direction
}

// Helper function: Calculate where each row should move to
fn calculate_target_rows(
    row_collapse_info: &[Option<SplittingDirection>]
) -> Vec<usize> {
    let row_count = row_collapse_info.len();
    let mut target_rows = vec![0; row_count];

    for row in 0..row_count {
        target_rows[row] = match row_collapse_info[row] {
            Some(SplittingDirection::Up) => find_non_collapsible_row_above(row, row_collapse_info),
            Some(SplittingDirection::Down) => find_non_collapsible_row_below(row, row_collapse_info),
            None => row, // Non-collapsible rows stay in place
        };
    }

    target_rows
}

// Helper function: Find nearest non-collapsible row above
fn find_non_collapsible_row_above(
    start_row: usize,
    row_collapse_info: &[Option<SplittingDirection>]
) -> usize {
    (0..=start_row)
        .rev()
        .find(|&row| row_collapse_info[row].is_none())
        .unwrap_or(0)
}

// Helper function: Find nearest non-collapsible row below
fn find_non_collapsible_row_below(
    start_row: usize,
    row_collapse_info: &[Option<SplittingDirection>]
) -> usize {
    (start_row..row_collapse_info.len())
        .find(|&row| row_collapse_info[row].is_none())
        .unwrap_or(start_row)
}

// Helper function: Apply row collapsing to all indexes
fn apply_row_collapsing(
    indexes: Vec<(usize, usize)>,
    target_rows: &[usize],
) -> Vec<(usize, usize)> {
    let mut result = Vec::with_capacity(indexes.len());
    
    for (original_row, col) in indexes {
        let new_row = target_rows[original_row];
        result.push((new_row, col));
    }
    
    result
}





fn collapse_table_rows_by_pattern(
    mut indexes: Vec<(usize, usize)>,
    table_config: &TableConfig
) -> Vec<(usize, usize)> {
    let tmp_table_cfg=table_config.clone();
    let cols_cfg: Vec<SplittingState>=tmp_table_cfg.cols
        .unwrap()
        .into_iter()
        .map(|x| x.splitting.unwrap_or(
            SplittingState::Allow(SplittingDirection::Down)
        ))
        .collect();

    let mut i = 0;
    while i < indexes.len() {
        let current_col = indexes[i].1;
        
        // Skip if not splittable
        let split_direction: SplittingDirection;
        match cols_cfg[current_col] {
            SplittingState::Disallow => {
                i += 1;
                continue
            },
            SplittingState::Allow(dir) => {
                split_direction=dir;
            }
        }
        let mut sequence_end = i + 1;
        while sequence_end < indexes.len() && indexes[sequence_end].1 == current_col {
            sequence_end += 1;
        }
        if sequence_end - i > 1 {
            // Collapse the sequence
            let mut sequence=&mut indexes[i..sequence_end];
            let target_row = match split_direction {
                SplittingDirection::Up => sequence.iter().map(|&(row, _)| row).max().unwrap(),
                SplittingDirection::Down => sequence.iter().map(|&(row, _)| row).min().unwrap()
            };
            sequence.iter_mut().for_each(|(row,_)| *row=target_row);
            i = sequence_end;
        } else {
            i += 1;
        }
    }
    
    indexes
}



fn collapse_table_rows(
    mut indexes: Vec<(usize,usize)>,
    table_config: &TableConfig,
    geometrical_strategy: bool,
    pattern_strategy: bool
) -> Vec<(usize,usize)> {
    let mut tmp_conf=table_config.clone();
    if tmp_conf.cols.is_none() {
        let _tmp =indexes.iter();
        let n_cols=_tmp.max_by_key(|x| x.1).unwrap().1+1;
        tmp_conf.cols=Some(
            vec![ColumnConfig{
                limits: None,
                splitting: None,
                nullable: None
            };n_cols]
        )
    }

    if geometrical_strategy {
        indexes=collapse_table_rows_by_geometry(indexes,&tmp_conf);
    }
    if pattern_strategy {
        indexes=collapse_table_rows_by_pattern(indexes,&tmp_conf);
    }
    indexes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_geometrical_strategy(){
        let cells: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (1,1),
            (1,2),
            (3,0),
            (3,1),
            (3,2),
            // collapsable lines:
            (2,1),
            (4,0),
            (5,0),
        ];
        let both_collapsed_up: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (1,1),
            (1,2),
            (3,0),
            (3,1),
            (3,2),
            // collapsed:
            (1,1),
            (3,0),
            (3,0),
        ];
        let both_collapsed_down: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (2,1),  // <-- collapsed
            (1,2),
            (5,0),  // <-- collapsed
            (3,1),
            (3,2),
            // ----
            (2,1),
            (5,0),  // <-- collapsed
            (5,0),
        ];
        let second_collapsed_up: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (1,1),
            (1,2),
            (3,0),
            (3,1),
            (3,2),
            // -----
            (2,1), 
            (3,0), // <-- colapsed
            (3,0), // <-- colapsed
        ];
        let first_collapsed_down: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (2,1),  // <-- collapsed
            (1,2),
            (3,0),
            (3,1),
            (3,2),
            // ----
            (2,1),
            (4,0),
            (5,0),
        ];
        let unknow_splittable_col = ColumnConfig{
            limits: None,
            splitting: None,
            nullable: None
        };
        let disallow_splittable_col = ColumnConfig{
            splitting: Some(SplittingState::Disallow),
            ..unknow_splittable_col.clone()
        };
        let down_splittable_col = ColumnConfig{
            splitting: Some(SplittingState::Allow(SplittingDirection::Down)),
            ..unknow_splittable_col.clone()
        };
        let up_splittable_col = ColumnConfig{
            splitting: Some(SplittingState::Allow(SplittingDirection::Up)),
            ..unknow_splittable_col.clone()
        };
        let cfg_all_collapsable_unknown=TableConfig{
            rows: None,
            cols: Some(vec![unknow_splittable_col.clone();3])
        };
        let cfg_all_collapsable_up=TableConfig{
            cols: Some(vec![down_splittable_col.clone();3]),
            ..cfg_all_collapsable_unknown.clone()
        };
        let cfg_all_collapsable_down=TableConfig{
            cols: Some(vec![up_splittable_col.clone();3]),
            ..cfg_all_collapsable_unknown.clone()
        };
        let cfg_first_collapsable_down=TableConfig{
            cols: Some(vec![
                disallow_splittable_col.clone(),
                up_splittable_col.clone(),
                disallow_splittable_col.clone(),
            ]),
            ..cfg_all_collapsable_unknown.clone()
        };
        let cfg_second_collapsable_up=TableConfig{
            cols: Some(vec![
                down_splittable_col.clone(),
                disallow_splittable_col.clone(),
                disallow_splittable_col.clone()
            ]),
            ..cfg_all_collapsable_unknown.clone()
        };
        assert_eq!(
            both_collapsed_down,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_all_collapsable_down)
        );
        assert_eq!(
            both_collapsed_up,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_all_collapsable_unknown)
        );
        assert_eq!(
            both_collapsed_up,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_all_collapsable_up)
        );
        assert_eq!(
            first_collapsed_down,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_first_collapsable_down)
        );
        assert_eq!(
            second_collapsed_up,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_second_collapsable_up)
        );
    }
    #[test]
    fn test_pattern_strategy(){
        let cells: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (1,1),
            (2,1),
            (1,2),
            (3,0),
            (4,0),
            (5,0),
            (3,1),
            (3,2)
        ];
        let both_collapsed_up: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (1,1),
            (1,1),
            (1,2),
            (3,0),
            (3,0),
            (3,0),
            (3,1),
            (3,2)
        ];
        let both_collapsed_down: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (2,1),
            (2,1),
            (1,2),
            (5,0),
            (5,0),
            (5,0),
            (3,1),
            (3,2)               
        ];
        let second_collapsed_up: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (1,1),
            (2,1),
            (1,2),
            (3,0),
            (3,0),
            (3,0),
            (3,1),
            (3,2)                
        ];
        let first_collapsed_down: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (2,1),
            (2,1),
            (1,2),
            (3,0),
            (4,0),
            (5,0),
            (3,1),
            (3,2)                
        ];
        let unknow_splittable_col = ColumnConfig{
            limits: None,
            splitting: None,
            nullable: None
        };
        let disallow_splittable_col = ColumnConfig{
            splitting: Some(SplittingState::Disallow),
            ..unknow_splittable_col.clone()
        };
        let down_splittable_col = ColumnConfig{
            splitting: Some(SplittingState::Allow(SplittingDirection::Down)),
            ..unknow_splittable_col.clone()
        };
        let up_splittable_col = ColumnConfig{
            splitting: Some(SplittingState::Allow(SplittingDirection::Up)),
            ..unknow_splittable_col.clone()
        };
        let cfg_all_collapsable_unknown=TableConfig{
            rows: None,
            cols: Some(vec![unknow_splittable_col.clone();3])
        };
        let cfg_all_collapsable_up=TableConfig{
            cols: Some(vec![down_splittable_col.clone();3]),
            ..cfg_all_collapsable_unknown.clone()
        };
        let cfg_all_collapsable_down=TableConfig{
            cols: Some(vec![up_splittable_col.clone();3]),
            ..cfg_all_collapsable_unknown.clone()
        };
        let cfg_first_collapsable_down=TableConfig{
            cols: Some(vec![
                disallow_splittable_col.clone(),
                up_splittable_col.clone(),
                disallow_splittable_col.clone(),
            ]),
            ..cfg_all_collapsable_unknown.clone()
        };
        let cfg_second_collapsable_up=TableConfig{
            cols: Some(vec![
                down_splittable_col.clone(),
                disallow_splittable_col.clone(),
                disallow_splittable_col.clone()
            ]),
            ..cfg_all_collapsable_unknown.clone()
        };
        assert_eq!(
            both_collapsed_down,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_all_collapsable_down)
        );
        assert_eq!(
            both_collapsed_up,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_all_collapsable_unknown)
        );
        assert_eq!(
            both_collapsed_up,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_all_collapsable_up)
        );
        assert_eq!(
            first_collapsed_down,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_first_collapsable_down)
        );
        assert_eq!(
            second_collapsed_up,
            collapse_table_rows_by_pattern(cells.clone(),&cfg_second_collapsable_up)
        );
    }
    #[test]
    fn test_both_strategies(){
        todo!();
    }
}




