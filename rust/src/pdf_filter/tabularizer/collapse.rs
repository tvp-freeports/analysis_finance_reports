use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use pyo3::pyclass;

use super::{TableConfig,ColumnConfig};

#[derive(Clone)]
pub enum CollapseAlgorithm {
    Pattern,
    Geometry,
    GeometryThenPattern,
    PatternThenGeometry
}
impl FromPyObject<'_, '_> for CollapseAlgorithm {
    type Error = PyErr;
    fn extract(py_enum_variant: Borrowed<'_, '_,PyAny>) -> Result<Self, Self::Error> {
        let name: String = py_enum_variant.getattr("name")?.extract()?;
        match name.as_str() {
            "PATTERN" => Ok(Self::Pattern),
            "GEOMETRY" => Ok(Self::Geometry),
            "GEOMETRY_PATTERN" => Ok(Self::GeometryThenPattern),
            "PATTERN_GEOMETRY" => Ok(Self::PatternThenGeometry),
            _ => Err(PyValueError::new_err(
                "CollapseAlgorithm enum value not recognized",
            )),
        }
    }
}

#[pyfunction]
pub fn collapse_table_rows(
    indexes: Vec<(usize, usize)>,
    table_config: &TableConfig,
    alghoritm: CollapseAlgorithm
) -> Vec<(usize, usize)> {
    use CollapseAlgorithm::*;
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
    match alghoritm {
        Pattern => collapse_table_rows_by_pattern(indexes,&tmp_conf),
        Geometry => collapse_table_rows_by_geometry(indexes,&tmp_conf),
        PatternThenGeometry => {
            let tmp_res = collapse_table_rows_by_pattern(indexes,&tmp_conf);
            collapse_table_rows_by_geometry(tmp_res,&tmp_conf)
        },
        GeometryThenPattern => {
            let tmp_res = collapse_table_rows_by_geometry(indexes,&tmp_conf);
            collapse_table_rows_by_pattern(tmp_res,&tmp_conf)
        }
    }
}


#[pyclass]
#[derive(Clone,Copy,PartialEq,Debug)]
pub enum SplittingDirection {
    Up,
    Down
}

// #[pyclass]
// #[pyo3(name = "SplittingState")]
// #[derive(Clone,Copy)]
// pub enum PySplittingState {
//     Allow(SplittingDirection),
//     Disallow()
// }
// #[pymethods]
// impl PySplittingState {
//     #[new]
//     fn py_new(direction: Option<SplittingDirection>) -> Self {
//         match direction {
//             Some(a) => PySplittingState::Allow(a),
//             None => PySplittingState::Disallow()
//         }
//     }
// }


#[derive(Clone,Copy,Debug,PartialEq)]
pub enum SplittingState {
    Allow(SplittingDirection),
    Disallow
}
impl FromPyObject<'_, '_> for SplittingState {
    type Error = PyErr;
    fn extract(py_enum_variant: Borrowed<'_, '_,PyAny>) -> Result<Self, Self::Error> {
        let name: String = py_enum_variant.getattr("name")?.extract()?;
        match name.as_str() {
            "UP" => Ok(Self::Allow(SplittingDirection::Up)),
            "DOWN" => Ok(Self::Allow(SplittingDirection::Down)),
            "DISALLOW" => Ok(Self::Disallow),
            _ => Err(PyValueError::new_err(
                "SplittingState enum value not recognized",
            )),
        }
    }

}




pub type NullableState = bool;

type Collapsability = bool;

fn collapse_table_rows_by_geometry(
    indexes: Vec<(usize, usize)>,
    table_config: &TableConfig,
) -> Vec<(usize, usize)> {
    // Early return if no indexes
    if indexes.is_empty() {
        return indexes;
    }
    

    // Build column configurations
    let column_info = extract_column_info(table_config);
    let cell_exists = build_existence_matrix(&indexes);
    let cell_configuration = build_configuration_matrix(&cell_exists, &column_info);
    let target_rows = calc_target_rows(&cell_configuration);
    
    // Apply row collapsing to indexes
    let mut result = Vec::with_capacity(indexes.len());
    
    for (original_row, col) in indexes {
        let new_row = target_rows[original_row];
        result.push((new_row, col));
    }
    result
}

#[derive(Clone,Copy,Debug)]
struct GeometryCollapseConfig{
    splitting: SplittingState,
    nullable: NullableState
}

fn extract_column_info(table_config: &TableConfig) -> Vec<GeometryCollapseConfig> {
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
            GeometryCollapseConfig{
                splitting, 
                nullable
            }
        })
        .collect()
}


#[derive(Clone,Copy,Debug,PartialEq)]
struct CellCollapseState(SplittingState,Collapsability);

// Helper function: Determine table size and which cells exist
fn build_existence_matrix(
    indexes: &[(usize, usize)]
) -> Vec<Vec<bool>> {
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

    cell_exists
}

fn is_row_collapsable(row: &[bool],_cfg: &[GeometryCollapseConfig]) -> bool {
    row.iter().any(|full| !full)
}
fn is_row_splittable(row: &[bool],cfg: &[GeometryCollapseConfig])  -> bool {
    // Only unsplittable rows are the ones that contains empty non nullable cells
    !(0..row.len()).filter(|&i| !row[i]).any(|j| !cfg[j].nullable)
}

fn build_configuration_matrix(
    matrix: &[Vec<bool>], 
    cfg: &[GeometryCollapseConfig]
) -> Vec::<Vec<CellCollapseState>> {
    let nrows=matrix.len();
    let ncols=matrix[0].len();
    let mut cfg_matrix = vec![
        vec![
            CellCollapseState(SplittingState::Disallow,false); ncols
        ]; nrows
    ];
    for (i,row) in matrix.iter().enumerate() {
        let collapsable_row=is_row_collapsable(row,cfg);
        let splittable_row=is_row_splittable(row,cfg);
        for j in 0..ncols {
            if row[j] {
                cfg_matrix[i][j]=CellCollapseState(
                    if splittable_row { cfg[j].splitting } else { SplittingState::Disallow },
                    collapsable_row
                )
            }
        }
    }

    cfg_matrix
}



fn calc_target_rows(matrix: &[Vec<CellCollapseState>]) -> Vec<usize> {
    let nrows = matrix.len();
    let ncols = matrix[0].len();
    let mut target_rows: Vec<usize> = (0..nrows).collect();
    let mut collapsing_rows=vec![false; nrows];

    for col in 0..ncols {
        let mut start_collapsing: Option<usize> = None;
        let mut end_collapsing: Option<usize> = None;
        for row in 0..nrows {
            if let (CellCollapseState(SplittingState::Allow(SplittingDirection::Down),_),None) = (matrix[row][col],start_collapsing) {
                start_collapsing=Some(row);
                end_collapsing=Some(row);
            } else if let Some(start) = start_collapsing {
                if !matrix[row][col].1 || row == nrows-1 {
                    if row == nrows-1 && matrix[row][col].1 {
                        end_collapsing=Some(row)
                    }
                    (start..=end_collapsing.unwrap()).for_each(
                        |i| {
                            target_rows[i] = start;
                            collapsing_rows[i] = true;
                        }
                    );
                    (start_collapsing,end_collapsing) = match matrix[row][col].0 {
                        SplittingState::Allow(SplittingDirection::Down) => (Some(row),Some(row)),
                        _ => (None,None)
                    }
                } else {
                    end_collapsing=Some(row);
                }
            }
        }
        let mut start_collapsing: Option<usize> = None;
        let mut end_collapsing: Option<usize> = None;
        for row in (0..nrows).rev() {
            if let (CellCollapseState(SplittingState::Allow(SplittingDirection::Up),_),None) = (matrix[row][col],start_collapsing) {
                start_collapsing=Some(row);
                end_collapsing=Some(row);
            } else if let Some(start) = start_collapsing {
                if !matrix[row][col].1 || row == 0 {
                    if row == 0 && matrix[row][col].1 {
                        end_collapsing=Some(row)
                    }
                    (end_collapsing.unwrap()..start).for_each(
                        |i| {
                            if !collapsing_rows[i] {
                                target_rows[i] = start;
                            } else {
                                target_rows[i] = i;
                            }
                            
                        }
                    );
                    (start_collapsing,end_collapsing) = match matrix[row][col].0 {
                        SplittingState::Allow(SplittingDirection::Up) => (Some(row),Some(row)),
                        _ => (None,None)
                    }
                } else {
                    end_collapsing=Some(row);
                }
            }
        }
    }
    target_rows
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
            // Collapsability the sequence
            let sequence=&mut indexes[i..sequence_end];
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




#[cfg(test)]
mod tests {
    use super::*;
    mod geometrical_strategy {
        use super::*;
        #[test]
        fn test_extract_column_info(){
            let unknow_splittable_col = ColumnConfig{
                limits: None,
                splitting: None,
                nullable: None
            };
            let disallow_splittable_col = ColumnConfig{
                splitting: Some(SplittingState::Disallow),
                ..unknow_splittable_col.clone()
            };
            let up_splittable_col = ColumnConfig{
                splitting: Some(SplittingState::Allow(SplittingDirection::Up)),
                nullable: Some(true),
                ..unknow_splittable_col.clone()
            };
            let cfg=TableConfig{
                rows: None,
                cols: Some(vec![
                    unknow_splittable_col.clone(),
                    disallow_splittable_col.clone(),
                    up_splittable_col.clone()
                ])
            };
            let cfg_geo=extract_column_info(&cfg);
            assert!(matches!(
                cfg_geo[0],
                GeometryCollapseConfig{
                    splitting: SplittingState::Allow(SplittingDirection::Down),
                    nullable: false
                },
            ));
            assert!(matches!(
                cfg_geo[1],
                GeometryCollapseConfig{
                    splitting: SplittingState::Disallow,
                    nullable: false
                }
            ));
            assert!(matches!(
                cfg_geo[2],
                GeometryCollapseConfig{
                    splitting: SplittingState::Allow(SplittingDirection::Up),
                    nullable: true
                },
            ));
        }
        #[test]
        fn test_build_existence_matrix(){
            let cells: Vec<(usize,usize)>=vec![
                (0,0),
                (0,1),
                (0,2),
                (2,1),
                (3,0),
                (3,2),
            ];
            let matrix = build_existence_matrix(&cells);
            assert_eq!(matrix,vec![
                vec![ true, true, true],
                vec![false,false,false],
                vec![false, true,false],
                vec![ true,false, true]
            ]);
        }
        #[test]
        fn test_build_configuration_matrix(){
            let matrix=vec![
                vec![true,true,true],
                vec![false,false,false],
                vec![false,true,false],
                vec![true,false,true]
            ];
            let column_cfg=vec![
                GeometryCollapseConfig{
                    splitting: SplittingState::Allow(SplittingDirection::Down),
                    nullable: false
                },
                GeometryCollapseConfig{
                    splitting: SplittingState::Disallow,
                    nullable: true
                },
                GeometryCollapseConfig{
                    splitting: SplittingState::Allow(SplittingDirection::Up),
                    nullable: false

                }
            ];
            let emp=CellCollapseState(SplittingState::Disallow,false);
            let sd=CellCollapseState(SplittingState::Allow(SplittingDirection::Down),false);
            let su=CellCollapseState(SplittingState::Allow(SplittingDirection::Up),false);
            let col=CellCollapseState(SplittingState::Disallow,true);
            let sdc=CellCollapseState(SplittingState::Allow(SplittingDirection::Down),true);
            let suc=CellCollapseState(SplittingState::Allow(SplittingDirection::Up),true);
            let cfg_matrix=vec![
                vec![ sd.clone(),emp.clone(), su.clone()],
                vec![emp.clone(),emp.clone(),emp.clone()],
                vec![emp.clone(),col.clone(),emp.clone()],
                vec![sdc.clone(),emp.clone(),suc.clone()]
            ];
            assert_eq!(
                cfg_matrix,
                build_configuration_matrix(
                    &matrix,
                    &column_cfg
                )
            )
        }
        #[test]
        fn test_is_row_collapsable(){
            let column_cfg=vec![
                GeometryCollapseConfig{
                    splitting: SplittingState::Allow(SplittingDirection::Down),
                    nullable: true
                },
                GeometryCollapseConfig{
                    splitting: SplittingState::Disallow,
                    nullable: false
                },
                GeometryCollapseConfig{
                    splitting: SplittingState::Allow(SplittingDirection::Up),
                    nullable: true

                }
            ];
            assert!(is_row_collapsable(
                &vec![true,true,false],&column_cfg
            ));
            assert!(is_row_collapsable(
                &vec![false,true,false],&column_cfg
            ));
            assert!(!is_row_collapsable(
                &vec![true,true,true],&column_cfg
            ));

        }
        #[test]
        fn test_is_row_splittable(){
            let column_cfg=vec![
                GeometryCollapseConfig{
                    splitting: SplittingState::Allow(SplittingDirection::Down),
                    nullable: true
                },
                GeometryCollapseConfig{
                    splitting: SplittingState::Disallow,
                    nullable: false
                },
                GeometryCollapseConfig{
                    splitting: SplittingState::Allow(SplittingDirection::Up),
                    nullable: true

                }
            ];
            assert!(is_row_splittable(
                &vec![true,true,true],&column_cfg
            ));
            assert!(is_row_splittable(
                &vec![false,true,false],&column_cfg
            ));
            assert!(!is_row_splittable(
                &vec![true,false,true],&column_cfg
            ));
        }
        mod target_row {
            use super::*;
            const EMP: CellCollapseState = CellCollapseState(SplittingState::Disallow,false);
            const SD: CellCollapseState = CellCollapseState(SplittingState::Allow(SplittingDirection::Down),false);
            const SU: CellCollapseState = CellCollapseState(SplittingState::Allow(SplittingDirection::Up),false);
            const CP: CellCollapseState = CellCollapseState(SplittingState::Disallow,true);
            const SDC: CellCollapseState = CellCollapseState(SplittingState::Allow(SplittingDirection::Down),true);
            const SUC: CellCollapseState = CellCollapseState(SplittingState::Allow(SplittingDirection::Up),true);
            #[test]
            fn normal() {
                let matrix = vec![
                    vec![EMP, SD,EMP,EMP,EMP, SD],
                    vec![EMP, CP,EMP,EMP,EMP,EMP],
                    vec![EMP,EMP,EMP,EMP, CP,EMP],
                    vec![EMP,EMP, CP,EMP, CP,EMP],
                    vec![EMP,EMP, SU,EMP, SU,EMP]
                ];
                let target_rows = vec![0,0,4,4,4];
                assert_eq!(target_rows,calc_target_rows(&matrix));
            }
            #[test]
            fn splittable_is_collapsable () {
                let matrix = vec![
                    vec![EMP, CP,EMP,EMP],
                    vec![EMP,SDC,EMP,EMP],
                    vec![EMP, CP,EMP,EMP],
                    vec![EMP,SDC,EMP,EMP],
                    vec![EMP,EMP,EMP,EMP]
                ];
                let target_rows = vec![0,1,1,1,4];
                assert_eq!(target_rows,calc_target_rows(&matrix));
                let matrix = vec![
                    vec![EMP,EMP,EMP, SU],
                    vec![EMP,EMP,EMP,SUC],
                    vec![EMP,EMP,EMP,SUC],
                    vec![EMP,EMP,EMP, CP],
                    vec![EMP,EMP,EMP, CP],
                    vec![EMP,EMP,EMP, SU]
                ];
                let target_rows = vec![0,5,5,5,5,5];
                assert_eq!(target_rows,calc_target_rows(&matrix));
            }
            #[test]
            fn collapse_concurrency () {
                let matrix = vec![
                    vec![EMP, SD,EMP,EMP,EMP],
                    vec![EMP, CP,EMP,EMP,EMP],
                    vec![EMP, CP,EMP, CP,EMP],
                    vec![EMP,EMP,EMP, SU,EMP],
                    vec![EMP,EMP,EMP,EMP,EMP]
                ];
                let target_rows = vec![0,0,2,3,4];
                assert_eq!(target_rows,calc_target_rows(&matrix));
            }
            #[test]
            fn splittables_adj () {
                let matrix = vec![
                    vec![SD,EMP,EMP],
                    vec![SD,EMP,EMP],
                    vec![CP,EMP,EMP],
                    vec![EMP,EMP,CP],
                    vec![EMP,EMP,SU],
                    vec![EMP,EMP,SU]
                ];
                let target_rows = vec![0,1,1,4,4,5];
                assert_eq!(target_rows,calc_target_rows(&matrix));
            }
        }
    }
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
            (1,1),
            (1,2),
            (3,0),
            (3,1),
            (3,2),
            // ----
            (3,1),  // <-- collapsed
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
            (1,1),
            (1,2),
            (3,0),
            (3,1),
            (3,2),
            // ----
            (3,1),
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
            nullable: Some(true),
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
            collapse_table_rows_by_geometry(cells.clone(),&cfg_all_collapsable_down)
        );
        assert_eq!(
            both_collapsed_up,
            collapse_table_rows_by_geometry(cells.clone(),&cfg_all_collapsable_unknown)
        );
        assert_eq!(
            both_collapsed_up,
            collapse_table_rows_by_geometry(cells.clone(),&cfg_all_collapsable_up)
        );
        assert_eq!(
            first_collapsed_down,
            collapse_table_rows_by_geometry(cells.clone(),&cfg_first_collapsable_down)
        );
        assert_eq!(
            second_collapsed_up,
            collapse_table_rows_by_geometry(cells.clone(),&cfg_second_collapsable_up)
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
    fn test_collapse_algorithm(){
        let indexes: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (2,0),
            (3,0),
            (1,1),
            (4,0),
            (4,1),
            (4,2),
            (5,1)
        ];
        let cfg = TableConfig{
            rows: None,
            cols: None
        };
        let only_pattern: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (1,0),
            (1,0),
            (1,0),
            (1,1),
            (4,0),
            (4,1),
            (4,2),
            (5,1)
        ];
        let only_geometry: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (0,0),
            (0,0),
            (0,0),
            (0,1),
            (4,0),
            (4,1),
            (4,2),
            (4,1)
        ];
        let pattern_geometry: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (0,0),
            (0,0),
            (0,0),
            (0,1),
            (4,0),
            (4,1),
            (4,2),
            (4,1)
        ];
        let geometry_pattern: Vec<(usize,usize)> = vec![
            (0,0),
            (0,1),
            (0,2),
            (0,0),
            (0,0),
            (0,0),
            (0,1),
            (4,0),
            (4,1),
            (4,2),
            (4,1)
        ];
        assert_eq!(only_pattern,collapse_table_rows(indexes.clone(),&cfg,CollapseAlgorithm::Pattern));
        assert_eq!(only_geometry,collapse_table_rows(indexes.clone(),&cfg,CollapseAlgorithm::Geometry));
        assert_eq!(pattern_geometry,collapse_table_rows(indexes.clone(),&cfg,CollapseAlgorithm::PatternThenGeometry));
        assert_eq!(geometry_pattern,collapse_table_rows(indexes.clone(),&cfg,CollapseAlgorithm::GeometryThenPattern));
    }
}




