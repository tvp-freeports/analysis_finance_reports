//! Collasso di righe/celle multi-riga (CollapseAlgorithm, SplittingState).
//!
//! Porting verbatim (`PLAN.md` §0/§12 D14) di
//! `freeports_core::formats_utils::pdf_extract::tabularizer::collapse`, meno il confine PyO3
//! (`FromPyObject` per `CollapseAlgorithm`/`SplittingState`, `#[pyfunction] py_collapse_table_rows`).
//! `ColumnConfig`/`TableConfig` non sono definiti qui (a differenza del riferimento): per
//! decisione esplicita di questa milestone vivono in `pdf_extract::position` (vedi il doc-comment
//! di quel modulo e la decisione R2 di `PLAN.md`), e questo modulo li importa da li'.
//! `SplittingDirection`/`SplittingState`/`NullableState` sono anch'essi definiti in `position`
//! (evita una dipendenza circolare fra i due moduli: vedi il commento li') e qui sono
//! ri-esportati dal loro percorso storico, cosi' che nulla fuori da questi due file debba sapere
//! del trasloco.
//!
//! Contratto atteso dai test qui sotto (il test-writer non scrive codice di produzione):
//!
//! - `pub enum CollapseAlgorithm { Pattern, Geometry, GeometryThenPattern, PatternThenGeometry }`.
//! - `pub enum SplittingDirection { Up, Down }` e
//!   `pub enum SplittingState { Allow(SplittingDirection), Disallow }`; `pub type NullableState = bool`.
//! - `pub fn collapse_table_rows(indexes: Vec<(usize,usize)>, table_config: &TableConfig,
//!   alghoritm: CollapseAlgorithm) -> Vec<(usize,usize)>`: se `table_config.cols` e' `None`, lo
//!   sostituisce con `n_cols` colonne "sconosciute" (`limits: None, splitting: None, nullable:
//!   None`, una per colonna, **non** lo stesso `ColumnConfig` clonato/aliasato — altrimenti
//!   impostare `splitting` su una sola colonna dopo la chiamata ne muterebbe altre per
//!   condivisione, bug gia' noto e corretto altrove nel porting di `position`); dispaccia
//!   all'algoritmo scelto, applicando entrambe le strategie in sequenza per le due varianti
//!   composte.
//! - Strategia "pattern" (`collapse_table_rows_by_pattern`): sequenze consecutive di celle nella
//!   stessa colonna collassano sulla riga minima (`SplittingState::Allow(Down)`, default se
//!   `splitting` e' `None`) o massima (`Allow(Up)`) della sequenza; `Disallow` lascia la colonna
//!   invariata.
//! - Strategia "geometry" (`collapse_table_rows_by_geometry`): righe con almeno una cella vuota
//!   sono "collassabili"; una riga e' "splittabile" solo se ogni cella vuota che contiene e'
//!   `nullable`; le celle presenti in righe splittabili ereditano lo `splitting` di colonna,
//!   altrimenti `Disallow`; sequenze verticali di righe splittabili+collassabili in una colonna
//!   collassano verso l'alto o il basso a seconda della direzione, con gestione delle sequenze
//!   che si toccano (l'ultima riga di ogni run puo' gia' avere un target da un run opposto).

use super::super::position::{ColumnConfig, TableConfig};
pub use super::super::position::{NullableState, SplittingDirection, SplittingState};

#[derive(Clone, Copy)]
pub enum CollapseAlgorithm {
    Pattern,
    Geometry,
    GeometryThenPattern,
    PatternThenGeometry,
}

type Collapsability = bool;

pub fn collapse_table_rows(indexes: Vec<(usize, usize)>, table_config: &TableConfig, alghoritm: CollapseAlgorithm) -> Vec<(usize, usize)> {
    use CollapseAlgorithm::*;
    let mut tmp_conf = table_config.clone();
    if tmp_conf.cols.is_none() {
        let n_cols = indexes.iter().max_by_key(|x| x.1).unwrap().1 + 1;
        tmp_conf.cols = Some(vec![ColumnConfig { limits: None, splitting: None, nullable: None }; n_cols]);
    }
    match alghoritm {
        Pattern => collapse_table_rows_by_pattern(indexes, &tmp_conf),
        Geometry => collapse_table_rows_by_geometry(indexes, &tmp_conf),
        PatternThenGeometry => {
            let tmp_res = collapse_table_rows_by_pattern(indexes, &tmp_conf);
            collapse_table_rows_by_geometry(tmp_res, &tmp_conf)
        }
        GeometryThenPattern => {
            let tmp_res = collapse_table_rows_by_geometry(indexes, &tmp_conf);
            collapse_table_rows_by_pattern(tmp_res, &tmp_conf)
        }
    }
}

fn collapse_table_rows_by_geometry(indexes: Vec<(usize, usize)>, table_config: &TableConfig) -> Vec<(usize, usize)> {
    if indexes.is_empty() {
        return indexes;
    }

    let column_info = extract_column_info(table_config);
    let cell_exists = build_existence_matrix(&indexes);
    let cell_configuration = build_configuration_matrix(&cell_exists, &column_info);
    let target_rows = calc_target_rows(&cell_configuration);

    let mut result = Vec::with_capacity(indexes.len());
    for (original_row, col) in indexes {
        let new_row = target_rows[original_row];
        result.push((new_row, col));
    }
    result
}

#[derive(Clone, Copy, Debug)]
struct GeometryCollapseConfig {
    splitting: SplittingState,
    nullable: NullableState,
}

fn extract_column_info(table_config: &TableConfig) -> Vec<GeometryCollapseConfig> {
    let tmp_table_cfg = table_config.clone();

    tmp_table_cfg
        .cols
        .unwrap()
        .into_iter()
        .map(|col_config| {
            let splitting = col_config.splitting.unwrap_or(SplittingState::Allow(SplittingDirection::Down));
            let nullable = col_config.nullable.unwrap_or(false);
            GeometryCollapseConfig { splitting, nullable }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CellCollapseState(SplittingState, Collapsability);

fn build_existence_matrix(indexes: &[(usize, usize)]) -> Vec<Vec<bool>> {
    let max_row = indexes.iter().map(|&(row, _)| row).max().unwrap_or(0);
    let max_col = indexes.iter().map(|&(_, col)| col).max().unwrap_or(0);

    let row_count = max_row + 1;
    let col_count = max_col + 1;

    let mut cell_exists = vec![vec![false; col_count]; row_count];
    for &(row, col) in indexes {
        cell_exists[row][col] = true;
    }

    cell_exists
}

fn is_row_collapsable(row: &[bool], _cfg: &[GeometryCollapseConfig]) -> bool {
    row.iter().any(|full| !full)
}
fn is_row_splittable(row: &[bool], cfg: &[GeometryCollapseConfig]) -> bool {
    // Only unsplittable rows are the ones that contains empty non nullable cells
    !(0..row.len()).filter(|&i| !row[i]).any(|j| !cfg[j].nullable)
}

fn build_configuration_matrix(matrix: &[Vec<bool>], cfg: &[GeometryCollapseConfig]) -> Vec<Vec<CellCollapseState>> {
    let nrows = matrix.len();
    let ncols = matrix[0].len();
    let mut cfg_matrix = vec![vec![CellCollapseState(SplittingState::Disallow, false); ncols]; nrows];
    for (i, row) in matrix.iter().enumerate() {
        let collapsable_row = is_row_collapsable(row, cfg);
        let splittable_row = is_row_splittable(row, cfg);
        for j in 0..ncols {
            if row[j] {
                cfg_matrix[i][j] = CellCollapseState(if splittable_row { cfg[j].splitting } else { SplittingState::Disallow }, collapsable_row)
            }
        }
    }

    cfg_matrix
}

fn calc_target_rows(matrix: &[Vec<CellCollapseState>]) -> Vec<usize> {
    let nrows = matrix.len();
    let ncols = matrix[0].len();
    let mut target_rows: Vec<usize> = (0..nrows).collect();
    let mut collapsing_rows = vec![false; nrows];

    for i_col in 0..ncols {
        let mut start_collapsing: Option<usize> = None;
        let mut end_collapsing: Option<usize> = None;
        for (i_row, row) in matrix.iter().enumerate() {
            if let (CellCollapseState(SplittingState::Allow(SplittingDirection::Down), _), None) = (row[i_col], start_collapsing) {
                start_collapsing = Some(i_row);
                end_collapsing = Some(i_row);
            } else if let Some(start) = start_collapsing {
                if !row[i_col].1 || i_row == nrows - 1 {
                    if i_row == nrows - 1 && row[i_col].1 {
                        end_collapsing = Some(i_row)
                    }
                    (start..=end_collapsing.unwrap()).for_each(|i| {
                        target_rows[i] = start;
                        collapsing_rows[i] = true;
                    });
                    (start_collapsing, end_collapsing) = match row[i_col].0 {
                        SplittingState::Allow(SplittingDirection::Down) => (Some(i_row), Some(i_row)),
                        _ => (None, None),
                    }
                } else {
                    end_collapsing = Some(i_row);
                }
            }
        }
        let mut start_collapsing: Option<usize> = None;
        let mut end_collapsing: Option<usize> = None;
        for (i_row, row) in matrix.iter().enumerate().rev() {
            if let (CellCollapseState(SplittingState::Allow(SplittingDirection::Up), _), None) = (row[i_col], start_collapsing) {
                start_collapsing = Some(i_row);
                end_collapsing = Some(i_row);
            } else if let Some(start) = start_collapsing {
                if !row[i_col].1 || i_row == 0 {
                    if i_row == 0 && row[i_col].1 {
                        end_collapsing = Some(i_row)
                    }
                    (end_collapsing.unwrap()..start).for_each(|i| {
                        if !collapsing_rows[i] {
                            target_rows[i] = start;
                        } else {
                            target_rows[i] = i;
                        }
                    });
                    (start_collapsing, end_collapsing) = match row[i_col].0 {
                        SplittingState::Allow(SplittingDirection::Up) => (Some(i_row), Some(i_row)),
                        _ => (None, None),
                    }
                } else {
                    end_collapsing = Some(i_row);
                }
            }
        }
    }
    target_rows
}

fn collapse_table_rows_by_pattern(mut indexes: Vec<(usize, usize)>, table_config: &TableConfig) -> Vec<(usize, usize)> {
    let tmp_table_cfg = table_config.clone();
    let cols_cfg: Vec<SplittingState> =
        tmp_table_cfg.cols.unwrap().into_iter().map(|x| x.splitting.unwrap_or(SplittingState::Allow(SplittingDirection::Down))).collect();

    let mut i = 0;
    while i < indexes.len() {
        let current_col = indexes[i].1;

        let split_direction: SplittingDirection = match cols_cfg[current_col] {
            SplittingState::Disallow => {
                i += 1;
                continue;
            }
            SplittingState::Allow(dir) => dir,
        };
        let mut sequence_end = i + 1;
        while sequence_end < indexes.len() && indexes[sequence_end].1 == current_col {
            sequence_end += 1;
        }
        if sequence_end - i > 1 {
            let sequence = &mut indexes[i..sequence_end];
            let target_row = match split_direction {
                SplittingDirection::Up => sequence.iter().map(|&(row, _)| row).max().unwrap(),
                SplittingDirection::Down => sequence.iter().map(|&(row, _)| row).min().unwrap(),
            };
            sequence.iter_mut().for_each(|(row, _)| *row = target_row);
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

    const UNKNOWN_SPLITTABLE_COL: ColumnConfig = ColumnConfig { limits: None, splitting: None, nullable: None };

    mod extract_column_info {
        use super::*;

        #[test]
        fn defaults_missing_splitting_to_allow_down_and_missing_nullable_to_false() {
            let disallow_splittable_col = ColumnConfig { splitting: Some(SplittingState::Disallow), ..UNKNOWN_SPLITTABLE_COL };
            let up_splittable_col =
                ColumnConfig { splitting: Some(SplittingState::Allow(SplittingDirection::Up)), nullable: Some(true), ..UNKNOWN_SPLITTABLE_COL };
            let cfg = TableConfig { rows: None, cols: Some(vec![UNKNOWN_SPLITTABLE_COL, disallow_splittable_col, up_splittable_col]) };
            let cfg_geo = extract_column_info(&cfg);
            assert!(matches!(cfg_geo[0], GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Down), nullable: false }));
            assert!(matches!(cfg_geo[1], GeometryCollapseConfig { splitting: SplittingState::Disallow, nullable: false }));
            assert!(matches!(cfg_geo[2], GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Up), nullable: true }));
        }
    }

    mod build_existence_matrix {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn marks_present_cells_and_leaves_absent_rows_all_false() {
            let cells: Vec<(usize, usize)> = vec![(0, 0), (0, 1), (0, 2), (2, 1), (3, 0), (3, 2)];
            let matrix = build_existence_matrix(&cells);
            assert_eq!(
                matrix,
                vec![vec![true, true, true], vec![false, false, false], vec![false, true, false], vec![true, false, true]]
            );
        }
    }

    mod build_configuration_matrix {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn combines_per_row_collapsability_and_splittability_with_per_column_splitting() {
            let matrix = vec![vec![true, true, true], vec![false, false, false], vec![false, true, false], vec![true, false, true]];
            let column_cfg = vec![
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Down), nullable: false },
                GeometryCollapseConfig { splitting: SplittingState::Disallow, nullable: true },
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Up), nullable: false },
            ];
            let emp = CellCollapseState(SplittingState::Disallow, false);
            let sd = CellCollapseState(SplittingState::Allow(SplittingDirection::Down), false);
            let su = CellCollapseState(SplittingState::Allow(SplittingDirection::Up), false);
            let col = CellCollapseState(SplittingState::Disallow, true);
            let sdc = CellCollapseState(SplittingState::Allow(SplittingDirection::Down), true);
            let suc = CellCollapseState(SplittingState::Allow(SplittingDirection::Up), true);
            let cfg_matrix = vec![vec![sd, emp, su], vec![emp, emp, emp], vec![emp, col, emp], vec![sdc, emp, suc]];
            assert_eq!(cfg_matrix, build_configuration_matrix(&matrix, &column_cfg));
        }
    }

    mod is_row_collapsable {
        use super::*;

        #[test]
        fn any_missing_cell_makes_the_row_collapsable() {
            let column_cfg = vec![
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Down), nullable: true },
                GeometryCollapseConfig { splitting: SplittingState::Disallow, nullable: false },
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Up), nullable: true },
            ];
            assert!(is_row_collapsable(&[true, true, false], &column_cfg));
            assert!(is_row_collapsable(&[false, true, false], &column_cfg));
        }

        #[test]
        fn a_fully_populated_row_is_not_collapsable() {
            let column_cfg = vec![
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Down), nullable: true },
                GeometryCollapseConfig { splitting: SplittingState::Disallow, nullable: false },
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Up), nullable: true },
            ];
            assert!(!is_row_collapsable(&[true, true, true], &column_cfg));
        }
    }

    mod is_row_splittable {
        use super::*;

        #[test]
        fn a_row_whose_missing_cells_are_all_nullable_is_splittable() {
            let column_cfg = vec![
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Down), nullable: true },
                GeometryCollapseConfig { splitting: SplittingState::Disallow, nullable: false },
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Up), nullable: true },
            ];
            assert!(is_row_splittable(&[true, true, true], &column_cfg));
            assert!(is_row_splittable(&[false, true, false], &column_cfg));
        }

        #[test]
        fn a_row_missing_a_non_nullable_cell_is_not_splittable() {
            let column_cfg = vec![
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Down), nullable: true },
                GeometryCollapseConfig { splitting: SplittingState::Disallow, nullable: false },
                GeometryCollapseConfig { splitting: SplittingState::Allow(SplittingDirection::Up), nullable: true },
            ];
            assert!(!is_row_splittable(&[true, false, true], &column_cfg));
        }
    }

    mod calc_target_rows {
        use super::*;
        use pretty_assertions::assert_eq;

        const EMP: CellCollapseState = CellCollapseState(SplittingState::Disallow, false);
        const SD: CellCollapseState = CellCollapseState(SplittingState::Allow(SplittingDirection::Down), false);
        const SU: CellCollapseState = CellCollapseState(SplittingState::Allow(SplittingDirection::Up), false);
        const CP: CellCollapseState = CellCollapseState(SplittingState::Disallow, true);
        const SDC: CellCollapseState = CellCollapseState(SplittingState::Allow(SplittingDirection::Down), true);
        const SUC: CellCollapseState = CellCollapseState(SplittingState::Allow(SplittingDirection::Up), true);

        #[test]
        fn collapses_a_down_run_and_an_up_run_towards_their_anchors() {
            let matrix = vec![
                vec![EMP, SD, EMP, EMP, EMP, SD],
                vec![EMP, CP, EMP, EMP, EMP, EMP],
                vec![EMP, EMP, EMP, EMP, CP, EMP],
                vec![EMP, EMP, CP, EMP, CP, EMP],
                vec![EMP, EMP, SU, EMP, SU, EMP],
            ];
            assert_eq!(vec![0, 0, 4, 4, 4], calc_target_rows(&matrix));
        }

        #[test]
        fn a_splittable_row_that_is_also_collapsable_still_anchors_the_run() {
            let matrix = vec![
                vec![EMP, CP, EMP, EMP],
                vec![EMP, SDC, EMP, EMP],
                vec![EMP, CP, EMP, EMP],
                vec![EMP, SDC, EMP, EMP],
                vec![EMP, EMP, EMP, EMP],
            ];
            assert_eq!(vec![0, 1, 1, 1, 4], calc_target_rows(&matrix));

            let matrix = vec![
                vec![EMP, EMP, EMP, SU],
                vec![EMP, EMP, EMP, SUC],
                vec![EMP, EMP, EMP, SUC],
                vec![EMP, EMP, EMP, CP],
                vec![EMP, EMP, EMP, CP],
                vec![EMP, EMP, EMP, SU],
            ];
            assert_eq!(vec![0, 5, 5, 5, 5, 5], calc_target_rows(&matrix));
        }

        #[test]
        fn two_concurrently_collapsing_columns_do_not_interfere() {
            let matrix = vec![
                vec![EMP, SD, EMP, EMP, EMP],
                vec![EMP, CP, EMP, EMP, EMP],
                vec![EMP, CP, EMP, CP, EMP],
                vec![EMP, EMP, EMP, SU, EMP],
                vec![EMP, EMP, EMP, EMP, EMP],
            ];
            assert_eq!(vec![0, 0, 2, 3, 4], calc_target_rows(&matrix));
        }

        #[test]
        fn adjacent_splittable_columns_collapse_independently() {
            let matrix = vec![
                vec![SD, EMP, EMP],
                vec![SD, EMP, EMP],
                vec![CP, EMP, EMP],
                vec![EMP, EMP, CP],
                vec![EMP, EMP, SU],
                vec![EMP, EMP, SU],
            ];
            assert_eq!(vec![0, 1, 1, 4, 4, 5], calc_target_rows(&matrix));
        }
    }

    mod collapse_table_rows_by_geometry {
        use super::*;
        use pretty_assertions::assert_eq;

        fn cells() -> Vec<(usize, usize)> {
            vec![
                (0, 0),
                (0, 1),
                (0, 2),
                (1, 0),
                (1, 1),
                (1, 2),
                (3, 0),
                (3, 1),
                (3, 2),
                // collapsable lines:
                (2, 1),
                (4, 0),
                (5, 0),
            ]
        }

        #[test]
        fn collapses_downward_when_every_column_allows_it() {
            let up_splittable_col =
                ColumnConfig { splitting: Some(SplittingState::Allow(SplittingDirection::Up)), nullable: Some(true), ..UNKNOWN_SPLITTABLE_COL };
            let cfg = TableConfig { rows: None, cols: Some(vec![up_splittable_col; 3]) };
            let expected: Vec<(usize, usize)> = vec![
                (0, 0),
                (0, 1),
                (0, 2),
                (1, 0),
                (1, 1),
                (1, 2),
                (3, 0),
                (3, 1),
                (3, 2),
                (3, 1),
                (5, 0),
                (5, 0),
            ];
            assert_eq!(expected, collapse_table_rows_by_geometry(cells(), &cfg));
        }

        #[test]
        fn collapses_upward_by_default_when_splitting_is_unspecified() {
            let cfg = TableConfig { rows: None, cols: Some(vec![UNKNOWN_SPLITTABLE_COL; 3]) };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (3, 0), (3, 1), (3, 2), (1, 1), (3, 0), (3, 0)];
            assert_eq!(expected, collapse_table_rows_by_geometry(cells(), &cfg));
        }

        #[test]
        fn collapses_upward_when_every_column_allows_it_explicitly() {
            let down_splittable_col = ColumnConfig { splitting: Some(SplittingState::Allow(SplittingDirection::Down)), ..UNKNOWN_SPLITTABLE_COL };
            let cfg = TableConfig { rows: None, cols: Some(vec![down_splittable_col; 3]) };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (3, 0), (3, 1), (3, 2), (1, 1), (3, 0), (3, 0)];
            assert_eq!(expected, collapse_table_rows_by_geometry(cells(), &cfg));
        }

        #[test]
        fn only_the_column_configured_to_split_collapses() {
            let disallow_splittable_col = ColumnConfig { splitting: Some(SplittingState::Disallow), ..UNKNOWN_SPLITTABLE_COL };
            let up_splittable_col =
                ColumnConfig { splitting: Some(SplittingState::Allow(SplittingDirection::Up)), nullable: Some(true), ..UNKNOWN_SPLITTABLE_COL };
            let cfg =
                TableConfig { rows: None, cols: Some(vec![disallow_splittable_col, up_splittable_col, disallow_splittable_col]) };
            let expected: Vec<(usize, usize)> = vec![
                (0, 0),
                (0, 1),
                (0, 2),
                (1, 0),
                (1, 1),
                (1, 2),
                (3, 0),
                (3, 1),
                (3, 2),
                (3, 1),
                (4, 0),
                (5, 0),
            ];
            assert_eq!(expected, collapse_table_rows_by_geometry(cells(), &cfg));
        }

        #[test]
        fn only_the_first_column_configured_to_split_collapses() {
            let disallow_splittable_col = ColumnConfig { splitting: Some(SplittingState::Disallow), ..UNKNOWN_SPLITTABLE_COL };
            let down_splittable_col = ColumnConfig { splitting: Some(SplittingState::Allow(SplittingDirection::Down)), ..UNKNOWN_SPLITTABLE_COL };
            let cfg =
                TableConfig { rows: None, cols: Some(vec![down_splittable_col, disallow_splittable_col, disallow_splittable_col]) };
            let expected: Vec<(usize, usize)> = vec![
                (0, 0),
                (0, 1),
                (0, 2),
                (1, 0),
                (1, 1),
                (1, 2),
                (3, 0),
                (3, 1),
                (3, 2),
                (2, 1),
                (3, 0),
                (3, 0),
            ];
            assert_eq!(expected, collapse_table_rows_by_geometry(cells(), &cfg));
        }
    }

    mod collapse_table_rows_by_pattern {
        use super::*;
        use pretty_assertions::assert_eq;

        fn cells() -> Vec<(usize, usize)> {
            vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (2, 1), (1, 2), (3, 0), (4, 0), (5, 0), (3, 1), (3, 2)]
        }

        #[test]
        fn collapses_downward_when_every_column_allows_it() {
            let up_splittable_col = ColumnConfig { splitting: Some(SplittingState::Allow(SplittingDirection::Up)), ..UNKNOWN_SPLITTABLE_COL };
            let cfg = TableConfig { rows: None, cols: Some(vec![up_splittable_col; 3]) };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (1, 0), (2, 1), (2, 1), (1, 2), (5, 0), (5, 0), (5, 0), (3, 1), (3, 2)];
            assert_eq!(expected, collapse_table_rows_by_pattern(cells(), &cfg));
        }

        #[test]
        fn collapses_upward_by_default_when_splitting_is_unspecified() {
            let cfg = TableConfig { rows: None, cols: Some(vec![UNKNOWN_SPLITTABLE_COL; 3]) };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 1), (1, 2), (3, 0), (3, 0), (3, 0), (3, 1), (3, 2)];
            assert_eq!(expected, collapse_table_rows_by_pattern(cells(), &cfg));
        }

        #[test]
        fn collapses_upward_when_every_column_allows_it_explicitly() {
            let down_splittable_col = ColumnConfig { splitting: Some(SplittingState::Allow(SplittingDirection::Down)), ..UNKNOWN_SPLITTABLE_COL };
            let cfg = TableConfig { rows: None, cols: Some(vec![down_splittable_col; 3]) };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 1), (1, 2), (3, 0), (3, 0), (3, 0), (3, 1), (3, 2)];
            assert_eq!(expected, collapse_table_rows_by_pattern(cells(), &cfg));
        }

        #[test]
        fn only_the_column_configured_to_split_collapses() {
            let disallow_splittable_col = ColumnConfig { splitting: Some(SplittingState::Disallow), ..UNKNOWN_SPLITTABLE_COL };
            let up_splittable_col = ColumnConfig { splitting: Some(SplittingState::Allow(SplittingDirection::Up)), ..UNKNOWN_SPLITTABLE_COL };
            let cfg = TableConfig { rows: None, cols: Some(vec![disallow_splittable_col, up_splittable_col, disallow_splittable_col]) };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (1, 0), (2, 1), (2, 1), (1, 2), (3, 0), (4, 0), (5, 0), (3, 1), (3, 2)];
            assert_eq!(expected, collapse_table_rows_by_pattern(cells(), &cfg));
        }

        #[test]
        fn only_the_first_column_configured_to_split_collapses() {
            let disallow_splittable_col = ColumnConfig { splitting: Some(SplittingState::Disallow), ..UNKNOWN_SPLITTABLE_COL };
            let down_splittable_col = ColumnConfig { splitting: Some(SplittingState::Allow(SplittingDirection::Down)), ..UNKNOWN_SPLITTABLE_COL };
            let cfg = TableConfig { rows: None, cols: Some(vec![down_splittable_col, disallow_splittable_col, disallow_splittable_col]) };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (2, 1), (1, 2), (3, 0), (3, 0), (3, 0), (3, 1), (3, 2)];
            assert_eq!(expected, collapse_table_rows_by_pattern(cells(), &cfg));
        }
    }

    mod collapse_table_rows_dispatch {
        use super::*;
        use pretty_assertions::assert_eq;

        fn indexes() -> Vec<(usize, usize)> {
            vec![(0, 0), (0, 1), (0, 2), (1, 0), (2, 0), (3, 0), (1, 1), (4, 0), (4, 1), (4, 2), (5, 1)]
        }

        #[test]
        fn pattern_only_collapses_consecutive_same_column_sequences() {
            let cfg = TableConfig { rows: None, cols: None };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 0), (1, 0), (1, 1), (4, 0), (4, 1), (4, 2), (5, 1)];
            assert_eq!(expected, collapse_table_rows(indexes(), &cfg, CollapseAlgorithm::Pattern));
        }

        #[test]
        fn geometry_only_collapses_towards_the_default_anchor() {
            let cfg = TableConfig { rows: None, cols: None };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (0, 0), (0, 0), (0, 0), (0, 1), (4, 0), (4, 1), (4, 2), (4, 1)];
            assert_eq!(expected, collapse_table_rows(indexes(), &cfg, CollapseAlgorithm::Geometry));
        }

        #[test]
        fn pattern_then_geometry_applies_both_in_sequence() {
            let cfg = TableConfig { rows: None, cols: None };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (0, 0), (0, 0), (0, 0), (0, 1), (4, 0), (4, 1), (4, 2), (4, 1)];
            assert_eq!(expected, collapse_table_rows(indexes(), &cfg, CollapseAlgorithm::PatternThenGeometry));
        }

        #[test]
        fn geometry_then_pattern_applies_both_in_sequence() {
            let cfg = TableConfig { rows: None, cols: None };
            let expected: Vec<(usize, usize)> =
                vec![(0, 0), (0, 1), (0, 2), (0, 0), (0, 0), (0, 0), (0, 1), (4, 0), (4, 1), (4, 2), (4, 1)];
            assert_eq!(expected, collapse_table_rows(indexes(), &cfg, CollapseAlgorithm::GeometryThenPattern));
        }

        #[test]
        fn missing_column_config_is_filled_with_independent_unknown_columns_not_aliased_clones() {
            // Se `cols` fosse riempito clonando/aliasando lo stesso `ColumnConfig`, configurare
            // in modo diverso una singola colonna dopo la chiamata (o un bug analogo interno)
            // altererebbe anche le altre: qui verifichiamo solo il comportamento osservabile,
            // cioe' che senza configurazione esplicita ogni colonna si comporta secondo i
            // default (`Allow(Down)` per il pattern), indipendentemente dalle altre.
            let cfg = TableConfig { rows: None, cols: None };
            let cells: Vec<(usize, usize)> = vec![(0, 0), (1, 0), (0, 1), (2, 1)];
            let result = collapse_table_rows(cells, &cfg, CollapseAlgorithm::Pattern);
            assert_eq!(result, vec![(0, 0), (0, 0), (0, 1), (0, 1)]);
        }
    }
}
