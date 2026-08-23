//! Ricostruzione di tabelle da righe PDF.
//!
//! I due sottomoduli sono il porting verbatim (`PLAN.md` §0/§12 D14) dell'algoritmo di
//! posizionamento (`coordinates`) e di quello di collasso (`collapse`). Questo modulo radice
//! aggiunge l'unico pezzo che nel riferimento **non** è Rust: il wrapper di alto livello
//! `get_table_coordinates` di `formats/utils/pdf_extract/position.py`, che parte dalle righe di
//! una pagina invece che da celle già costruite.
//!
//! **Perché vive qui e non in `position`** (dove sta il suo originale Python): `position` è il
//! livello *sotto* `tabularizer` — `tabularizer::{coordinates,collapse}` importano
//! `TableConfig`/`ColumnConfig` da `position`, e in M3 si è deliberatamente spezzata la
//! dipendenza circolare fra i due moduli spostando le definizioni condivise in `position`.
//! Rimettere qui una funzione di `position` che chiama `coordinates` e `collapse` ricreerebbe
//! quel ciclo; il modulo radice di `tabularizer`, che vede entrambi i figli, è il posto naturale.
//!
//! **Risolve `PLAN.md` §13 punto 4 (`TablePosMeasureUnit`)**, aperto da M3. Il tipo *esiste* nel
//! riferimento — non in Rust, ma in `position.py` — ed è esattamente l'unità di misura della
//! `tolerance` di questo wrapper: senza il wrapper non aveva un consumatore, il che spiega
//! perché M3 non ne avesse trovato traccia. `api::utils::pdf_extract` esporta questa funzione
//! (quella che `PLAN.md` §9 elenca accanto a `TablePosMeasureUnit`, e quella che gli autori di
//! formato chiamano davvero) sotto il nome `get_table_coordinates`, e quella per celle di
//! `coordinates` sotto `get_table_coordinates_from_cells`.

pub mod collapse;
pub mod coordinates;

use super::pdf_line::PdfLine;
use super::position::{ColumnConfig, SplittingState, TableConfig};
use collapse::CollapseAlgorithm;
use coordinates::{CellGeometry, CoordinateExtractionError, TablePosAlgorithm};

/// Unità di misura della `tolerance` di [`get_table_coordinates_from_lines`].
///
/// Porting di `TablePosMeasureUnit` (`formats/utils/pdf_extract/position.py`): decide come la
/// tolleranza scalare configurata dal repo formati diventa la tolleranza in punti di ogni cella.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TablePosMeasureUnit {
    /// Multiplo del corpo del carattere della riga (`tolerance * font_size`). Default, come nel
    /// riferimento.
    #[default]
    Em,
    /// Frazione della larghezza della riga (`tolerance * (x1 - x0)`).
    Perc,
    /// Punti tipografici, usata così com'è.
    Pt,
}

impl TablePosMeasureUnit {
    /// La tolleranza in punti per una specifica riga.
    fn resolve(self, tolerance: f32, line: &PdfLine) -> f32 {
        let (x0, _, x1, _) = line.bbox().as_tuple();
        match self {
            TablePosMeasureUnit::Pt => tolerance,
            TablePosMeasureUnit::Perc => tolerance * (x1 - x0),
            TablePosMeasureUnit::Em => tolerance * *line.font_size(),
        }
    }
}

/// Parametri di [`get_table_coordinates_from_lines`], raggruppati perché nel riferimento sono
/// otto argomenti con default (Python li passa per nome; in Rust una struct con `Default` è
/// l'equivalente leggibile, e evita il `clippy::too_many_arguments` che il riferimento silenzia).
#[derive(Debug, Clone)]
pub struct TableCoordinatesConfig {
    pub table_config: Option<TableConfig>,
    pub algorithm_flags: TablePosAlgorithm,
    pub collapse_algorithm: CollapseAlgorithm,
    pub tolerance: f32,
    pub tolerance_unit: TablePosMeasureUnit,
    /// Indice della colonna che contiene il nome della società: è l'unica a cui è concesso di
    /// spezzarsi su più righe (i nomi lunghi vanno a capo). Vedi il doc di
    /// [`get_table_coordinates_from_lines`] per il caveat sul suo effetto reale.
    pub company_col: Option<usize>,
    pub collapse: bool,
}

// `Default` scritto a mano e non derivato: `TablePosAlgorithm` (bitflags, M3 verbatim) e
// `CollapseAlgorithm` non implementano `Default`, e aggiungerlo la' significherebbe toccare due
// moduli che `PLAN.md` §0 vuole portati invariati. I valori sono quelli dei parametri con default
// del riferimento (`TablePosAlgorithm(0)`, `CollapseAlgorithm.GEOMETRY`, `tolerance=0`,
// `tolerance_mu=EM`, `company_col=None`, `collapse=False`).
impl Default for TableCoordinatesConfig {
    fn default() -> Self {
        Self {
            table_config: None,
            algorithm_flags: TablePosAlgorithm::Default,
            collapse_algorithm: CollapseAlgorithm::Geometry,
            tolerance: 0.0,
            tolerance_unit: TablePosMeasureUnit::Em,
            company_col: None,
            collapse: false,
        }
    }
}

/// Coordinate `(riga, colonna)` di ogni riga PDF di una tabella.
///
/// Porting di `position.get_table_coordinates`. Costruisce una [`CellGeometry`] per riga con la
/// tolleranza risolta secondo [`TablePosMeasureUnit`], delega a
/// [`coordinates::get_table_coordinates`] e, se richiesto, collassa le righe multi-linea.
///
/// **Caveat ereditato dal riferimento, conservato di proposito.** Il ramo `company_col` costruisce
/// una configurazione di colonne (tutte `Disallow` tranne quella della società) *dopo* aver già
/// calcolato le coordinate, quindi ha effetto solo se `collapse` è `true`: con `collapse: false`
/// — l'unico caso che i pipe standard usano davvero — è una configurazione costruita e mai letta.
/// È così anche nell'originale Python e non viene "corretto" qui.
///
/// **Una divergenza voluta**, sempre in quel ramo: il riferimento fa `n_cols = max(*cols)`, che
/// solleva `TypeError` quando la tabella ha una sola cella e produce comunque un vettore di
/// colonne lungo `max` invece di `max + 1`. Qui il massimo è calcolato su un iteratore (nessun
/// caso limite) e il vettore ha una colonna per ogni indice realmente osservato, perché
/// `PLAN.md` §2 principio 4 vieta i panici sul percorso utente e un vettore corto farebbe
/// panicare `collapse_table_rows` a valle. Non osservabile dai chiamanti attuali (`collapse:
/// false`).
pub fn get_table_coordinates_from_lines(
    lines: &[PdfLine],
    config: &TableCoordinatesConfig,
) -> Result<Vec<(usize, usize)>, CoordinateExtractionError> {
    let mut table_config = config.table_config.clone().unwrap_or(TableConfig { cols: None, rows: None });

    let cells: Vec<CellGeometry> = lines
        .iter()
        .map(|line| CellGeometry::new(line.bbox().as_tuple(), config.tolerance_unit.resolve(config.tolerance, line)))
        .collect();

    let coords = coordinates::get_table_coordinates(&cells, config.algorithm_flags, &table_config)?;

    if table_config.cols.is_none()
        && let Some(company_col) = config.company_col
        && let Some(max_col) = coords.iter().map(|(_, col)| *col).max()
    {
        // Ogni colonna ha la propria `ColumnConfig`: nel riferimento `[ColumnConfig()] * n` creava
        // n alias dello stesso oggetto, e impostare `splitting` su una sola colonna le mutava
        // tutte — bug già corretto lì, qui strutturalmente impossibile (i valori sono copiati).
        let mut cols = vec![
            ColumnConfig { limits: None, splitting: Some(SplittingState::Disallow), nullable: None };
            max_col + 1
        ];
        if let Some(col) = cols.get_mut(company_col) {
            // `None` = "nessun vincolo", che `collapse` interpreta come `Allow(Down)`: è la
            // traduzione fedele del `cols[company_col].splitting = None` del riferimento, dove il
            // default costruito era invece `SplittingState.DISALLOW`.
            col.splitting = None;
        }
        table_config.cols = Some(cols);
    }

    if config.collapse {
        return Ok(collapse::collapse_table_rows(coords, &table_config, config.collapse_algorithm));
    }
    Ok(coords)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::position::{RowConfig, SplittingDirection};

    /// Riga con bbox e corpo espliciti: geometria e `font_size` sono i soli attributi che il
    /// wrapper legge.
    fn line(text: &str, bbox: (f32, f32, f32, f32), font_size: f32) -> PdfLine {
        PdfLine::new("Arial", font_size, text, bbox)
    }

    /// Tabella 2x2 regolare: due righe, due colonne, celle ben separate.
    fn two_by_two() -> Vec<PdfLine> {
        vec![
            line("r0c0", (0.0, 0.0, 20.0, 10.0), 10.0),
            line("r0c1", (30.0, 0.0, 50.0, 10.0), 10.0),
            line("r1c0", (0.0, 20.0, 20.0, 30.0), 10.0),
            line("r1c1", (30.0, 20.0, 50.0, 30.0), 10.0),
        ]
    }

    mod table_pos_measure_unit {
        use super::*;

        #[test]
        fn pt_uses_the_tolerance_verbatim() {
            let l = line("x", (0.0, 0.0, 40.0, 10.0), 12.0);
            assert_eq!(TablePosMeasureUnit::Pt.resolve(3.0, &l), 3.0);
        }

        #[test]
        fn perc_scales_the_tolerance_by_the_line_width() {
            let l = line("x", (10.0, 0.0, 50.0, 10.0), 12.0);
            assert_eq!(TablePosMeasureUnit::Perc.resolve(0.25, &l), 10.0);
        }

        #[test]
        fn em_scales_the_tolerance_by_the_font_size() {
            let l = line("x", (0.0, 0.0, 40.0, 10.0), 12.0);
            assert_eq!(TablePosMeasureUnit::Em.resolve(0.5, &l), 6.0);
        }

        #[test]
        fn em_is_the_default_unit_like_in_the_reference() {
            assert_eq!(TablePosMeasureUnit::default(), TablePosMeasureUnit::Em);
        }

        #[test]
        fn a_zero_tolerance_resolves_to_zero_in_every_unit() {
            let l = line("x", (0.0, 0.0, 40.0, 10.0), 12.0);
            for unit in [TablePosMeasureUnit::Pt, TablePosMeasureUnit::Perc, TablePosMeasureUnit::Em] {
                assert_eq!(unit.resolve(0.0, &l), 0.0, "unit {unit:?}");
            }
        }
    }

    mod defaults {
        use super::*;

        #[test]
        fn mirror_the_python_signature_defaults() {
            let cfg = TableCoordinatesConfig::default();
            assert!(cfg.table_config.is_none());
            assert!(cfg.algorithm_flags.is_empty());
            assert!(matches!(cfg.collapse_algorithm, CollapseAlgorithm::Geometry));
            assert_eq!(cfg.tolerance, 0.0);
            assert_eq!(cfg.tolerance_unit, TablePosMeasureUnit::Em);
            assert!(cfg.company_col.is_none());
            assert!(!cfg.collapse);
        }
    }

    mod coordinates_from_lines {
        use super::*;

        #[test]
        fn assigns_row_and_column_indexes_to_a_regular_table() {
            let coords = get_table_coordinates_from_lines(&two_by_two(), &TableCoordinatesConfig::default()).unwrap();
            assert_eq!(coords, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
        }

        #[test]
        fn returns_one_coordinate_pair_per_input_line() {
            let lines = two_by_two();
            let coords = get_table_coordinates_from_lines(&lines, &TableCoordinatesConfig::default()).unwrap();
            assert_eq!(coords.len(), lines.len());
        }

        #[test]
        fn accepts_an_empty_list_of_lines() {
            let coords = get_table_coordinates_from_lines(&[], &TableCoordinatesConfig::default()).unwrap();
            assert!(coords.is_empty());
        }

        #[test]
        fn accepts_a_single_line_where_the_reference_would_raise() {
            // `max(*cols)` del riferimento solleva `TypeError` con una sola colonna; qui no.
            let cfg = TableCoordinatesConfig { company_col: Some(0), ..Default::default() };
            let coords = get_table_coordinates_from_lines(&[line("only", (0.0, 0.0, 10.0, 10.0), 10.0)], &cfg).unwrap();
            assert_eq!(coords, vec![(0, 0)]);
        }

        #[test]
        fn an_explicit_table_config_is_forwarded_to_the_positioning_algorithm() {
            let cfg = TableCoordinatesConfig {
                table_config: Some(TableConfig {
                    cols: Some(vec![ColumnConfig { limits: None, splitting: None, nullable: None }; 3]),
                    rows: None,
                }),
                ..Default::default()
            };
            // 3 colonne dichiarate contro 2 realmente presenti: l'errore arriva da `coordinates`.
            let err = get_table_coordinates_from_lines(&two_by_two(), &cfg).unwrap_err();
            assert!(matches!(err, CoordinateExtractionError::MismatchColumnNumber(3, 2)));
        }

        #[test]
        fn a_row_mismatch_in_the_explicit_config_is_reported_as_such() {
            let cfg = TableCoordinatesConfig {
                table_config: Some(TableConfig { cols: None, rows: Some(vec![RowConfig { limits: None }; 5]) }),
                ..Default::default()
            };
            let err = get_table_coordinates_from_lines(&two_by_two(), &cfg).unwrap_err();
            assert!(matches!(err, CoordinateExtractionError::MismatchRowNumber(5, 2)));
        }

        #[test]
        fn the_algorithm_flags_reach_the_positioning_algorithm() {
            // `BigCellRule` cambia quale cella fa da righello: su una tabella regolare il
            // risultato resta lo stesso, ma la chiamata non deve fallire.
            let cfg = TableCoordinatesConfig { algorithm_flags: TablePosAlgorithm::BigCellRule, ..Default::default() };
            let coords = get_table_coordinates_from_lines(&two_by_two(), &cfg).unwrap();
            assert_eq!(coords.len(), 4);
        }
    }

    mod tolerance_effect {
        use super::*;

        /// Due celle quasi allineate: senza tolleranza sono due colonne, con una tolleranza
        /// abbastanza grande diventano la stessa.
        fn nearly_aligned() -> Vec<PdfLine> {
            vec![line("a", (0.0, 0.0, 10.0, 10.0), 10.0), line("b", (11.0, 20.0, 21.0, 30.0), 10.0)]
        }

        #[test]
        fn no_tolerance_keeps_nearly_aligned_cells_in_distinct_columns() {
            let coords = get_table_coordinates_from_lines(&nearly_aligned(), &TableCoordinatesConfig::default()).unwrap();
            assert_ne!(coords[0].1, coords[1].1);
        }

        #[test]
        fn a_large_pt_tolerance_merges_nearly_aligned_cells_into_one_column() {
            let cfg = TableCoordinatesConfig { tolerance: 20.0, tolerance_unit: TablePosMeasureUnit::Pt, ..Default::default() };
            let coords = get_table_coordinates_from_lines(&nearly_aligned(), &cfg).unwrap();
            assert_eq!(coords[0].1, coords[1].1);
        }

        #[test]
        fn the_same_tolerance_expressed_in_em_depends_on_the_font_size() {
            // 2 em con corpo 10 = 20 pt: stesso effetto del test in punti qui sopra.
            let cfg = TableCoordinatesConfig { tolerance: 2.0, tolerance_unit: TablePosMeasureUnit::Em, ..Default::default() };
            let coords = get_table_coordinates_from_lines(&nearly_aligned(), &cfg).unwrap();
            assert_eq!(coords[0].1, coords[1].1);
        }

        #[test]
        fn the_same_tolerance_expressed_in_perc_depends_on_the_line_width() {
            // 2 volte la larghezza (10 pt) = 20 pt.
            let cfg = TableCoordinatesConfig { tolerance: 2.0, tolerance_unit: TablePosMeasureUnit::Perc, ..Default::default() };
            let coords = get_table_coordinates_from_lines(&nearly_aligned(), &cfg).unwrap();
            assert_eq!(coords[0].1, coords[1].1);
        }
    }

    mod company_col_branch {
        use super::*;

        /// Tabella con una cella mancante nella prima colonna: la riga incompleta è
        /// "collassabile" per l'algoritmo geometrico.
        fn ragged() -> Vec<PdfLine> {
            vec![
                line("r0c0", (0.0, 0.0, 20.0, 10.0), 10.0),
                line("r0c1", (30.0, 0.0, 50.0, 10.0), 10.0),
                line("r1c0", (0.0, 20.0, 20.0, 30.0), 10.0),
            ]
        }

        #[test]
        fn without_collapse_the_coordinates_are_untouched_by_company_col() {
            let plain = get_table_coordinates_from_lines(&ragged(), &TableCoordinatesConfig::default()).unwrap();
            let cfg = TableCoordinatesConfig { company_col: Some(0), ..Default::default() };
            assert_eq!(get_table_coordinates_from_lines(&ragged(), &cfg).unwrap(), plain);
        }

        #[test]
        fn an_out_of_range_company_col_is_ignored_instead_of_panicking() {
            let cfg = TableCoordinatesConfig { company_col: Some(99), collapse: true, ..Default::default() };
            assert!(get_table_coordinates_from_lines(&ragged(), &cfg).is_ok());
        }

        /// Tabella con la cella mancante nella colonna 0: è la colonna 1 a portare la riga in
        /// più, quindi è lei a dover collassare (o no, a seconda del suo `splitting`).
        fn ragged_on_the_second_column() -> Vec<PdfLine> {
            vec![
                line("r0c0", (0.0, 0.0, 20.0, 10.0), 10.0),
                line("r0c1", (30.0, 0.0, 50.0, 10.0), 10.0),
                line("r1c1", (30.0, 20.0, 50.0, 30.0), 10.0),
            ]
        }

        #[test]
        fn company_col_makes_that_column_the_only_splittable_one_when_collapsing() {
            // Senza `company_col` ogni colonna è splittabile e la riga in più collassa; con
            // `company_col: Some(0)` la colonna 1 diventa `Disallow` e resta dov'è.
            let lines = ragged_on_the_second_column();
            let without = TableCoordinatesConfig { collapse: true, ..Default::default() };
            let with_company = TableCoordinatesConfig { company_col: Some(0), collapse: true, ..Default::default() };
            let collapsed = get_table_coordinates_from_lines(&lines, &without).unwrap();
            let pinned = get_table_coordinates_from_lines(&lines, &with_company).unwrap();
            assert_eq!(collapsed[2].0, collapsed[0].0, "senza company_col la riga in più collassa");
            assert_ne!(pinned[2].0, pinned[0].0, "con company_col la colonna 1 non può collassare");
        }

        #[test]
        fn an_explicit_cols_config_disables_the_company_col_branch() {
            // `table_config.cols` già impostato: il riferimento non lo sovrascrive, e nemmeno noi.
            let explicit = TableConfig {
                cols: Some(vec![ColumnConfig { limits: None, splitting: Some(SplittingState::Disallow), nullable: None }; 2]),
                rows: None,
            };
            let cfg = TableCoordinatesConfig {
                table_config: Some(explicit.clone()),
                company_col: Some(0),
                collapse: true,
                ..Default::default()
            };
            let cfg_no_company =
                TableCoordinatesConfig { table_config: Some(explicit), collapse: true, ..Default::default() };
            assert_eq!(
                get_table_coordinates_from_lines(&ragged(), &cfg).unwrap(),
                get_table_coordinates_from_lines(&ragged(), &cfg_no_company).unwrap()
            );
        }
    }

    mod collapse_flag {
        use super::*;

        fn ragged() -> Vec<PdfLine> {
            vec![
                line("r0c0", (0.0, 0.0, 20.0, 10.0), 10.0),
                line("r0c1", (30.0, 0.0, 50.0, 10.0), 10.0),
                line("r1c0", (0.0, 20.0, 20.0, 30.0), 10.0),
            ]
        }

        #[test]
        fn collapse_false_returns_the_raw_coordinates() {
            let cfg = TableCoordinatesConfig { collapse: false, ..Default::default() };
            assert_eq!(get_table_coordinates_from_lines(&ragged(), &cfg).unwrap(), vec![(0, 0), (0, 1), (1, 0)]);
        }

        #[test]
        fn collapse_true_merges_the_incomplete_row_into_the_previous_one() {
            let cfg = TableCoordinatesConfig { collapse: true, ..Default::default() };
            let coords = get_table_coordinates_from_lines(&ragged(), &cfg).unwrap();
            assert_eq!(coords[2].0, coords[0].0);
        }

        #[test]
        fn every_collapse_algorithm_is_accepted() {
            for algorithm in [
                CollapseAlgorithm::Geometry,
                CollapseAlgorithm::Pattern,
                CollapseAlgorithm::GeometryThenPattern,
                CollapseAlgorithm::PatternThenGeometry,
            ] {
                let cfg = TableCoordinatesConfig { collapse: true, collapse_algorithm: algorithm, ..Default::default() };
                assert!(get_table_coordinates_from_lines(&ragged(), &cfg).is_ok(), "algorithm {algorithm:?}");
            }
        }

        #[test]
        fn collapsing_never_changes_how_many_coordinates_come_back() {
            let lines = ragged();
            let cfg = TableCoordinatesConfig { collapse: true, ..Default::default() };
            assert_eq!(get_table_coordinates_from_lines(&lines, &cfg).unwrap().len(), lines.len());
        }

        #[test]
        fn a_splitting_direction_up_config_collapses_the_other_way() {
            let cfg = TableCoordinatesConfig {
                table_config: Some(TableConfig {
                    cols: Some(vec![
                        ColumnConfig {
                            limits: None,
                            splitting: Some(SplittingState::Allow(SplittingDirection::Up)),
                            nullable: None
                        };
                        2
                    ]),
                    rows: None,
                }),
                collapse: true,
                ..Default::default()
            };
            assert!(get_table_coordinates_from_lines(&ragged(), &cfg).is_ok());
        }
    }
}
