//! Coordinate di tabella (TablePosAlgorithm, CellGeometry, get_table_coordinates).
//!
//! Porting verbatim (`PLAN.md` §0/§12 D14) di
//! `freeports_core::formats_utils::pdf_extract::tabularizer::coordinates`, meno il confine PyO3
//! (`FromPyObject` per `TablePosAlgorithm`, `#[derive(FromPyObject)]` su `CellGeometry`,
//! `#[pyfunction] py_get_table_coordinates`, la conversione `CoordinateExtractionError -> PyErr`).
//!
//! **Decisione R2 (`PLAN.md`)**: `CellGeometry` definito **qui** e' l'unico canonico del crate —
//! quello davvero usato dall'algoritmo, validato tramite `Limits::build` (a differenza di un
//! secondo `CellGeometry` non validato che compariva in un vecchio riferimento di `position.rs`,
//! mai portato). `position::RowConfig`/`ColumnConfig`/`TableConfig` non definiscono un proprio
//! `CellGeometry`: quello *usato per costruire* le celle di una tabella e' sempre questo.
//!
//! `ColumnConfig`/`RowConfig`/`TableConfig` vivono in `pdf_extract::position` (non in
//! `tabularizer`, a differenza del vecchio riferimento): importati da li'.
//!
//! Contratto atteso dai test qui sotto (il test-writer non scrive codice di produzione):
//!
//! - `pub enum CoordinateExtractionError { MismatchColumnNumber(usize,usize),
//!   MismatchRowNumber(usize,usize) }`, `thiserror::Error` con un messaggio che riporta atteso e
//!   trovato.
//! - `bitflags! { pub struct TablePosAlgorithm: u8 { Default=0; ReturnRows=1; BigCellRule=2;
//!   UseRulerArea=4; UseTestPos=8; } }`.
//! - `pub struct CellGeometry { bounds: (f32,f32,f32,f32), tolerance: f32 }` (campi privati
//!   leggibili dai test annidati), con `CellGeometry::new(bounds, tolerance) -> Self` che va in
//!   panico (messaggi `"Invalid horizontal interval: {err:?}"` / `"Invalid vertical interval:
//!   {err:?}"`) se `Limits::build` rifiuta l'intervallo orizzontale/verticale di `bounds`.
//! - `CellGeometryUnindexed` (privato): `pos = (a+b)/2.0`, `area = b-a` lungo l'asse scelto
//!   (orizzontale se `horizontal`, verticale altrimenti), con `from_cell_geometry`/`from_limits`.
//! - `same_position`/`position_in_area`/`areas_intersect` (privati): confronto tolleranza-aware
//!   fra due `CellGeometryUnindexed`.
//! - `get_table_indexes(cells, algorithm_flags, table_config) -> Result<Vec<usize>,
//!   CoordinateExtractionError>`: assegna iterativamente ogni cella non ancora indicizzata al
//!   "ruler" piu' vicino (il piu' piccolo, o il piu' grande con `BigCellRule`, salvo quando la
//!   config fornisce dei limiti espliciti per quella posizione), poi rinumera i ruler per
//!   posizione crescente; errore se il numero di ruler non combacia con la config esplicita
//!   (righe o colonne, a seconda di `ReturnRows`).
//! - `get_table_coordinates(cells, algorithm_flags, table_config) -> Result<Vec<(usize,usize)>,
//!   CoordinateExtractionError>`: `(riga, colonna)` per cella, calcolando entrambi gli assi;
//!   va in panico se `algorithm_flags` include gia' `ReturnRows` (non ha senso chiedere
//!   esplicitamente le righe quando si vogliono entrambi gli assi).

use std::collections::HashMap;
use std::iter::zip;

use bitflags::bitflags;

use crate::commons::geometry::Limits;

// `ColumnConfig`/`RowConfig` sono usati solo dai test (`use super::*` annidato piu' volte): nella
// build non-test di questo modulo restano inutilizzati, da cui l'`allow` mirato.
#[allow(unused_imports)]
use super::super::position::{ColumnConfig, RowConfig, TableConfig};

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum CoordinateExtractionError {
    #[error("Expected {0} columns, found {1}")]
    MismatchColumnNumber(usize, usize),
    #[error("Expected {0} rows, found {1}")]
    MismatchRowNumber(usize, usize),
}

bitflags! {
    // `Debug` aggiunto in M7 (modifica puramente additiva a codice M3 verbatim, stesso precedente
    // dei derive aggiunti a M1 durante M2): `tabularizer::TableCoordinatesConfig` lo richiede.
    #[derive(Debug,Clone,Copy)]
    pub struct TablePosAlgorithm: u8 {
        const Default = 0b000000000;
        const ReturnRows = 0b00000001;
        const BigCellRule = 0b00000010;
        const UseRulerArea = 0b00000100;
        const UseTestPos = 0b00001000;
    }
}

impl TablePosAlgorithm {
    /// I nomi dei flag, per l'espressione che il repo formati scrive nei suoi CSV/YAML.
    fn flag_names() -> std::collections::HashMap<String, u64> {
        std::collections::HashMap::from([
            ("RETURN_ROWS".to_string(), TablePosAlgorithm::ReturnRows.bits() as u64),
            ("BIG_CELL_RULE".to_string(), TablePosAlgorithm::BigCellRule.bits() as u64),
            ("USE_RULER_AREA".to_string(), TablePosAlgorithm::UseRulerArea.bits() as u64),
            ("USE_TEST_POS".to_string(), TablePosAlgorithm::UseTestPos.bits() as u64),
        ])
    }

    /// Analizza l'espressione di flag scritta dal repo formati (`"USE_RULER_AREA"`,
    /// `"BIG_CELL_RULE | USE_RULER_AREA"`, ...).
    ///
    /// Aggiunta in M7 — è il `TablePosAlgorithm.from_dict` del riferimento, che qui non poteva
    /// esistere prima perché nessun modulo leggeva ancora la configurazione del repo. Delega a
    /// `commons::flag_expr` (M1), che accetta il nome singolo del riferimento e in più le
    /// espressioni booleane: è un superinsieme, quindi non si perde nulla.
    pub fn from_expression(expression: &str) -> Result<Self, crate::commons::flag_expr::FlagExprError> {
        let bits = crate::commons::flag_expr::evaluate(expression, &Self::flag_names())?;
        // `evaluate` lavora su `u64`; i quattro flag stanno in un `u8` e `evaluate` non può
        // produrre bit che non gli siano stati dati, quindi il troncamento non perde nulla.
        Ok(TablePosAlgorithm::from_bits_truncate(bits as u8))
    }
}

/// Unico `CellGeometry` canonico del crate (decisione R2, `PLAN.md`): quello usato realmente
/// dall'algoritmo di posizionamento, validato tramite `Limits::build`.
#[derive(Debug, Clone, Copy)]
pub struct CellGeometry {
    bounds: (f32, f32, f32, f32),
    tolerance: f32,
}

impl CellGeometry {
    pub fn new(bounds: (f32, f32, f32, f32), tolerance: f32) -> Self {
        let (x0, y0, x1, y1) = bounds;
        if let Err(err) = Limits::build(x0, x1) {
            panic!("Invalid horizontal interval: {err:?}");
        }
        if let Err(err) = Limits::build(y0, y1) {
            panic!("Invalid vertical interval: {err:?}");
        }
        Self { bounds, tolerance }
    }
}

// Nessun parametro di lifetime (a differenza del riferimento, che porta un `PhantomData<&'a
// CellGeometry>` puramente decorativo): tutti i campi sono valori posseduti, e i test di questa
// milestone costruiscono `CellGeometryUnindexed` a partire da due riferimenti a lifetime
// indipendenti nella stessa espressione, il che non elide con un parametro di lifetime esplicito.
#[derive(Debug, Clone, Copy)]
struct CellGeometryUnindexed {
    index: usize,
    area: f32,
    pos: f32,
    bounds: Limits,
    tolerance: f32,
}
impl CellGeometryUnindexed {
    fn from_cell_geometry(cell: &CellGeometry, index: usize, horizontal: bool) -> Self {
        let CellGeometry { tolerance, bounds: (x0, y0, x1, y1) } = cell;
        let (a, b) = if horizontal { (*x0, *x1) } else { (*y0, *y1) };
        Self { index, tolerance: *tolerance, area: b - a, pos: (a + b) / 2.0, bounds: Limits::build(a, b).unwrap() }
    }
    fn from_limits(limits: Limits, index: usize, tolerance: f32) -> Self {
        let (a, b) = limits.as_tuple();
        Self { index, tolerance, area: b - a, pos: (a + b) / 2.0, bounds: limits }
    }
}

fn same_position(a: &CellGeometryUnindexed, b: &CellGeometryUnindexed) -> bool {
    (a.pos - b.pos).abs() <= (a.tolerance + b.tolerance) / 2.0
}
fn position_in_area(a: &CellGeometryUnindexed, b: &CellGeometryUnindexed) -> bool {
    let (l, r) = b.bounds.as_tuple();
    let t = (a.tolerance + b.tolerance) / 2.0;
    a.pos <= r + t && a.pos >= l - t
}
fn areas_intersect(a: &CellGeometryUnindexed, b: &CellGeometryUnindexed) -> bool {
    let (_al, _ar) = a.bounds.as_tuple();
    let (_bl, _br) = b.bounds.as_tuple();
    let (al, ar, bl, br) = (_al - a.tolerance, _ar + a.tolerance, _bl - b.tolerance, _br + b.tolerance);

    ar >= bl && al <= br
}

fn get_table_indexes(
    cells: &[CellGeometry],
    algorithm_flags: TablePosAlgorithm,
    table_config: &TableConfig,
) -> Result<Vec<usize>, CoordinateExtractionError> {
    let return_col = !algorithm_flags.contains(TablePosAlgorithm::ReturnRows);
    let mut unindexed: Vec<CellGeometryUnindexed> =
        cells.iter().enumerate().map(|(i, c)| CellGeometryUnindexed::from_cell_geometry(c, i, return_col)).collect();
    let mut indexes: Vec<Option<usize>> = vec![None; cells.len()];
    let mut rulers: Vec<CellGeometryUnindexed> = Vec::new();
    let cfg: Option<Vec<Option<Limits>>> = if return_col {
        table_config.cols.as_ref().map(|conf| conf.iter().map(|c| c.limits).collect())
    } else {
        table_config.rows.as_ref().map(|conf| conf.iter().map(|c| c.limits).collect())
    };
    let n_expected_indexes: Option<usize> = cfg.as_ref().map(|l| l.len());
    let mut cfg_iter = cfg.map(|v| v.into_iter());
    while indexes.iter().any(|a| a.is_none()) {
        let limits: Option<Limits> = cfg_iter.as_mut().and_then(|i| i.next()).flatten();
        let current_ruler_idx = rulers.len();

        let selected: CellGeometryUnindexed = match limits {
            Some(l) => CellGeometryUnindexed::from_limits(l, 0, 0.0),
            None => {
                *if !algorithm_flags.contains(TablePosAlgorithm::BigCellRule) {
                    unindexed.iter().min_by(|a, b| a.area.partial_cmp(&b.area).unwrap()).unwrap()
                } else {
                    unindexed.iter().max_by(|a, b| a.area.partial_cmp(&b.area).unwrap()).unwrap()
                }
            }
        };

        rulers.push(selected);
        let ruler = &rulers[current_ruler_idx];
        unindexed.retain(|elem| {
            let it_matches = if algorithm_flags.contains(TablePosAlgorithm::UseRulerArea | TablePosAlgorithm::UseTestPos) {
                position_in_area(elem, ruler)
            } else if algorithm_flags.contains(TablePosAlgorithm::UseRulerArea) {
                areas_intersect(ruler, elem)
            } else if algorithm_flags.contains(TablePosAlgorithm::UseTestPos) {
                same_position(ruler, elem)
            } else {
                position_in_area(ruler, elem)
            };
            if it_matches {
                indexes[elem.index] = Some(current_ruler_idx)
            }
            !it_matches
        });
    }

    if let Some(n_expected_indexes) = n_expected_indexes {
        let n_indexes = rulers.len();
        if n_indexes != n_expected_indexes {
            if return_col {
                return Err(CoordinateExtractionError::MismatchColumnNumber(n_expected_indexes, n_indexes));
            } else {
                return Err(CoordinateExtractionError::MismatchRowNumber(n_expected_indexes, n_indexes));
            }
        }
    }
    let mut unordered_mapping: Vec<(usize, CellGeometryUnindexed)> = rulers.into_iter().enumerate().collect();
    unordered_mapping.sort_by(|a, b| b.1.pos.partial_cmp(&a.1.pos).unwrap());
    unordered_mapping.reverse();
    let mut mapping: HashMap<usize, usize> = HashMap::new();
    for (i, pos) in unordered_mapping.into_iter().map(|(i, _r)| i).enumerate() {
        mapping.insert(pos, i);
    }
    Ok(indexes.iter().map(|x| mapping[&x.unwrap()]).collect())
}

pub fn get_table_coordinates(
    cells: &[CellGeometry],
    algorithm_flags: TablePosAlgorithm,
    table_config: &TableConfig,
) -> Result<Vec<(usize, usize)>, CoordinateExtractionError> {
    if algorithm_flags.contains(TablePosAlgorithm::ReturnRows) {
        panic!("Doesn't make any sense to return Row indexes when interested to (Row,Col)")
    }
    let algorithm_flags_rows = algorithm_flags | TablePosAlgorithm::ReturnRows;
    let cols = get_table_indexes(cells, algorithm_flags, table_config)?;
    let rows = get_table_indexes(cells, algorithm_flags_rows, table_config)?;
    Ok(zip(rows, cols).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::geometry::Limits;
    use std::iter::zip;

    mod cell_geometry_new {
        use super::*;

        #[test]
        fn accepts_valid_bounds() {
            // Nessun `matches!` con letterali in virgola mobile nel pattern (vietato dal
            // compilatore): confronto diretto dei campi dopo il pattern-match sulla struct.
            let CellGeometry { bounds, tolerance } = CellGeometry::new((0.1, 2.0, 40.0, 22.0), 43.0);
            assert_eq!(bounds, (0.1, 2.0, 40.0, 22.0));
            assert_eq!(tolerance, 43.0);
        }

        #[test]
        #[should_panic(expected = "Invalid horizontal interval:")]
        fn panics_on_an_inverted_horizontal_interval() {
            CellGeometry::new((110.1, 2.0, 40.0, 22.0), 43.0);
        }

        #[test]
        #[should_panic(expected = "Invalid vertical interval:")]
        fn panics_on_an_inverted_vertical_interval() {
            CellGeometry::new((0.1, 22.0, 40.0, 2.0), 43.0);
        }
    }

    mod table_coordinate_predicates {
        use super::*;

        fn pairs(cell_a: &CellGeometry, cell_b: &CellGeometry) -> Vec<(CellGeometryUnindexed, CellGeometryUnindexed)> {
            [(cell_a, cell_b), (cell_b, cell_a)]
                .iter()
                .map(|(a, b)| (CellGeometryUnindexed::from_cell_geometry(a, 10, true), CellGeometryUnindexed::from_cell_geometry(b, 12, true)))
                .collect()
        }

        #[test]
        fn same_position_is_true_within_the_combined_tolerance() {
            let cell_a = CellGeometry::new((0.0, 0.0, 1.0, 1.0), 0.0);
            let cell_b = CellGeometry::new((0.5, 0.0, 1.5, 1.5), 1.1);
            for (a, b) in pairs(&cell_a, &cell_b) {
                assert!(same_position(&a, &b));
            }
        }

        #[test]
        fn same_position_is_false_beyond_the_combined_tolerance() {
            let cell_a = CellGeometry::new((0.0, 0.0, 1.0, 1.0), 0.0);
            let cell_b = CellGeometry::new((10.5, 0.0, 11.5, 1.5), 1.1);
            for (a, b) in pairs(&cell_a, &cell_b) {
                assert!(!same_position(&a, &b));
            }
        }

        #[test]
        fn position_in_area_is_true_when_the_center_falls_within_the_other_bounds() {
            let cell_a = CellGeometry::new((0.0, 0.0, 1.0, 1.0), 0.0);
            let cell_b = CellGeometry::new((0.5, 0.0, 1.5, 1.5), 1.1);
            for (a, b) in pairs(&cell_a, &cell_b) {
                assert!(position_in_area(&a, &b));
            }
        }

        #[test]
        fn position_in_area_is_false_when_the_center_falls_outside_the_other_bounds() {
            let cell_a = CellGeometry::new((0.0, 0.0, 1.0, 1.0), 0.0);
            let cell_b = CellGeometry::new((10.5, 0.0, 11.5, 1.5), 1.1);
            for (a, b) in pairs(&cell_a, &cell_b) {
                assert!(!position_in_area(&a, &b));
            }
        }

        #[test]
        fn areas_intersect_is_true_when_the_tolerance_widened_bounds_overlap() {
            let cell_a = CellGeometry::new((0.0, 0.0, 1.0, 1.0), 0.0);
            let cell_b = CellGeometry::new((1.1, 0.0, 10.5, 1.5), 0.2);
            for (a, b) in pairs(&cell_a, &cell_b) {
                assert!(areas_intersect(&a, &b));
            }
        }

        #[test]
        fn areas_intersect_is_false_when_the_tolerance_widened_bounds_stay_apart() {
            let cell_a = CellGeometry::new((0.0, 0.0, 1.0, 1.0), 0.0);
            let cell_b = CellGeometry::new((10.5, 0.0, 11.5, 1.5), 1.1);
            for (a, b) in pairs(&cell_a, &cell_b) {
                assert!(!areas_intersect(&a, &b));
            }
        }
    }

    mod cell_geometry_unindexed {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn from_cell_geometry_horizontal_reads_the_x_axis() {
            let cell = CellGeometry::new((0.1, 2.0, 40.0, 22.0), 43.0);
            let u = CellGeometryUnindexed::from_cell_geometry(&cell, 100, true);
            assert_eq!(u.index, 100);
            assert_eq!(u.pos, 20.05);
            assert_eq!(u.area, 39.9);
            assert_eq!(u.tolerance, 43.0);
            assert_eq!(u.bounds.as_tuple(), (0.1, 40.0));
        }

        #[test]
        fn from_cell_geometry_vertical_reads_the_y_axis() {
            let cell = CellGeometry::new((0.1, 2.0, 40.0, 22.0), 43.0);
            let u = CellGeometryUnindexed::from_cell_geometry(&cell, 11, false);
            assert_eq!(u.index, 11);
            assert_eq!(u.pos, 12.0);
            assert_eq!(u.area, 20.0);
            assert_eq!(u.tolerance, 43.0);
            assert_eq!(u.bounds.as_tuple(), (2.0, 22.0));
        }

        #[test]
        fn from_limits_derives_pos_and_area_from_the_given_limits() {
            let u = CellGeometryUnindexed::from_limits(Limits::new(0.1, 40.5), 99, 5.7);
            assert_eq!(u.index, 99);
            assert_eq!(u.pos, 20.3);
            assert_eq!(u.area, 40.4);
            assert_eq!(u.tolerance, 5.7);
            assert_eq!(u.bounds.as_tuple(), (0.1, 40.5));
        }
    }

    mod get_table_indexes_and_coordinates {
        use super::*;
        use pretty_assertions::assert_eq;

        const X_COL: [((f32, f32), usize); 9] = [
            ((0.0, 2.0), 0),
            ((2.0, 3.0), 1),
            ((3.0, 4.0), 2),
            ((3.0, 4.0), 2),
            ((0.0, 2.0), 0),
            ((2.0, 3.0), 1),
            ((0.0, 2.0), 0),
            ((3.0, 4.0), 2),
            ((2.0, 3.0), 1),
        ];
        const Y_ROW: [((f32, f32), usize); 9] = [
            ((10.0, 20.0), 0),
            ((20.0, 30.0), 1),
            ((10.0, 20.0), 0),
            ((30.0, 40.0), 2),
            ((20.0, 30.0), 1),
            ((10.0, 20.0), 0),
            ((20.0, 30.0), 1),
            ((30.0, 40.0), 2),
            ((30.0, 40.0), 2),
        ];

        fn cells_from(x_col: &[((f32, f32), usize); 9], y_row: &[((f32, f32), usize); 9]) -> Vec<CellGeometry> {
            x_col
                .iter()
                .zip(y_row.iter())
                .map(|(&((x0, x1), _), &((y0, y1), _))| CellGeometry::new((x0, y0, x1, y1), 0.0))
                .collect()
        }

        #[test]
        fn with_no_config_assigns_indexes_purely_from_geometry() {
            let col_indexes: Vec<usize> = X_COL.iter().map(|a| a.1).collect();
            let row_indexes: Vec<usize> = Y_ROW.iter().map(|a| a.1).collect();
            let cells = cells_from(&X_COL, &Y_ROW);
            let table_cfg = TableConfig { cols: None, rows: None };
            assert_eq!(col_indexes, get_table_indexes(&cells, TablePosAlgorithm::Default, &table_cfg).unwrap());
            assert_eq!(row_indexes, get_table_indexes(&cells, TablePosAlgorithm::ReturnRows, &table_cfg).unwrap());
            assert_eq!(
                zip(row_indexes, col_indexes).collect::<Vec<(usize, usize)>>(),
                get_table_coordinates(&cells, TablePosAlgorithm::Default, &table_cfg).unwrap()
            );
        }

        #[test]
        fn column_and_row_counts_only_constrain_how_many_rulers_are_expected() {
            let col_indexes: Vec<usize> = X_COL.iter().map(|a| a.1).collect();
            let row_indexes: Vec<usize> = Y_ROW.iter().map(|a| a.1).collect();
            let cells = cells_from(&X_COL, &Y_ROW);
            let table_cfg = TableConfig { cols: Some(vec![ColumnConfig { limits: None, splitting: None, nullable: None }; 3]), rows: None };
            assert_eq!(col_indexes, get_table_indexes(&cells, TablePosAlgorithm::Default, &table_cfg).unwrap());

            let table_cfg = TableConfig {
                cols: Some(vec![ColumnConfig { limits: None, splitting: None, nullable: None }; 3]),
                rows: Some(vec![RowConfig { limits: None }; 3]),
            };
            assert_eq!(row_indexes, get_table_indexes(&cells, TablePosAlgorithm::ReturnRows, &table_cfg).unwrap());
        }

        #[test]
        fn mismatched_column_count_is_a_typed_error() {
            let cells = cells_from(&X_COL, &Y_ROW);
            let table_cfg = TableConfig { cols: Some(vec![ColumnConfig { limits: None, splitting: None, nullable: None }; 4]), rows: None };
            assert!(matches!(
                get_table_indexes(&cells, TablePosAlgorithm::Default, &table_cfg),
                Err(CoordinateExtractionError::MismatchColumnNumber(4, 3))
            ));
        }

        #[test]
        fn mismatched_row_count_is_a_typed_error() {
            let cells = cells_from(&X_COL, &Y_ROW);
            let table_cfg = TableConfig { cols: None, rows: Some(vec![RowConfig { limits: None }; 2]) };
            assert!(matches!(
                get_table_indexes(&cells, TablePosAlgorithm::ReturnRows, &table_cfg),
                Err(CoordinateExtractionError::MismatchRowNumber(2, 3))
            ));
        }

        const INTERVALS: [(f32, f32); 9] =
            [(0.0, 2.0), (0.5, 1.5), (0.0, 1.8), (0.7, 1.0), (1.9, 3.0), (2.0, 3.0), (10.0, 20.0), (30.0, 40.0), (35.0, 45.0)];
        const BIG_ONE: [usize; 9] = [0; 9];
        const ALL_THREE_LEFT_TOUCH: [usize; 9] = [0, 0, 0, 0, 0, 0, 1, 1, 2];
        const ALL_THREE_RIGHT_TOUCH: [usize; 9] = [0, 0, 0, 0, 1, 1, 1, 2, 2];
        const CENTER_UNSPECIFIED: [usize; 9] = [0, 0, 0, 0, 1, 1, 2, 2, 2];

        #[test]
        fn explicit_column_limits_override_the_smallest_cell_heuristic() {
            let cells: Vec<CellGeometry> = INTERVALS.iter().map(|&(x0, x1)| CellGeometry::new((x0, 0.0, x1, 1.0), 0.0)).collect();
            let table_cfg = TableConfig { cols: Some(vec![ColumnConfig { splitting: None, nullable: None, limits: Some(Limits::build(0.1, 50.0).unwrap()) }]), rows: None };
            assert_eq!(BIG_ONE.to_vec(), get_table_indexes(&cells, TablePosAlgorithm::UseRulerArea, &table_cfg).unwrap());
        }

        #[test]
        fn explicit_row_limits_can_be_touching_on_the_left_or_the_right() {
            let cells: Vec<CellGeometry> = INTERVALS.iter().map(|&(y0, y1)| CellGeometry::new((0.0, y0, 1.0, y1), 0.0)).collect();
            let table_cfg = TableConfig {
                rows: Some(vec![
                    RowConfig { limits: Some(Limits::build(0.0, 3.0).unwrap()) },
                    RowConfig { limits: Some(Limits::build(10.0, 35.0).unwrap()) },
                    RowConfig { limits: Some(Limits::build(35.0, 50.0).unwrap()) },
                ]),
                cols: None,
            };
            assert_eq!(
                ALL_THREE_LEFT_TOUCH.to_vec(),
                get_table_indexes(&cells, TablePosAlgorithm::UseRulerArea | TablePosAlgorithm::ReturnRows | TablePosAlgorithm::UseTestPos, &table_cfg).unwrap()
            );

            let table_cfg = TableConfig {
                rows: Some(vec![
                    RowConfig { limits: Some(Limits::build(0.0, 2.0).unwrap()) },
                    RowConfig { limits: Some(Limits::build(2.0, 20.0).unwrap()) },
                    RowConfig { limits: Some(Limits::build(20.0, 50.0).unwrap()) },
                ]),
                cols: None,
            };
            assert_eq!(
                ALL_THREE_RIGHT_TOUCH.to_vec(),
                get_table_indexes(&cells, TablePosAlgorithm::UseRulerArea | TablePosAlgorithm::ReturnRows | TablePosAlgorithm::UseTestPos, &table_cfg).unwrap()
            );
        }

        #[test]
        fn a_row_with_unspecified_limits_falls_back_to_the_smallest_cell_heuristic() {
            let cells: Vec<CellGeometry> = INTERVALS.iter().map(|&(y0, y1)| CellGeometry::new((0.0, y0, 1.0, y1), 0.0)).collect();
            let table_cfg = TableConfig {
                rows: Some(vec![
                    RowConfig { limits: Some(Limits::build(0.0, 2.0).unwrap()) },
                    RowConfig { limits: None },
                    RowConfig { limits: Some(Limits::build(3.0, 50.0).unwrap()) },
                ]),
                cols: None,
            };
            assert_eq!(
                CENTER_UNSPECIFIED.to_vec(),
                get_table_indexes(&cells, TablePosAlgorithm::UseRulerArea | TablePosAlgorithm::ReturnRows | TablePosAlgorithm::UseTestPos, &table_cfg).unwrap()
            );
        }
    }

    mod get_table_coordinates_guard {
        #[test]
        #[should_panic(expected = "Doesn't make any sense to return Row indexes when interested to (Row,Col)")]
        fn panics_when_return_rows_is_set() {
            use super::*;
            let cells = vec![CellGeometry::new((0.0, 1.0, 1.0, 2.0), 0.0)];
            let _ = get_table_coordinates(&cells, TablePosAlgorithm::ReturnRows, &TableConfig { cols: None, rows: None });
        }
    }
}
