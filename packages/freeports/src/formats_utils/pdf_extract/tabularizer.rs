//! Rebuilding tables out of the lines of a PDF page.
//!
//! A PDF does not say it has a table; it has text at coordinates. [`coordinates`] works out which
//! cell each piece of text belongs to from its geometry, [`collapse`] merges the cells of a row
//! that the layout broke across several lines, and this root module holds the high-level entry
//! point that starts from a page's lines rather than from cells already built.
//!
//! It lives here rather than in [`super::position`] because `coordinates` and `collapse` both
//! import their configuration from `position`: a function calling both would recreate a cycle. The
//! root module, which sees both children, is the natural place for it.

pub mod collapse;
pub mod coordinates;

use super::pdf_line::PdfLine;
use super::position::{ColumnConfig, SplittingState, TableConfig};
use collapse::CollapseAlgorithm;
use coordinates::{CellGeometry, CoordinateExtractionError, TablePosAlgorithm};

/// The unit in which the `tolerance` of [`get_table_coordinates_from_lines`] is expressed.
///
/// Decides how one scalar tolerance, written once in a format's configuration, becomes the
/// tolerance in points of each individual cell — which is what makes a single number work across a
/// document whose type sizes vary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TablePosMeasureUnit {
    /// A multiple of the line's font size (`tolerance * font_size`). The default.
    #[default]
    Em,
    /// A fraction of the line's width (`tolerance * (x1 - x0)`).
    Perc,
    /// Typographic points, used as they are.
    Pt,
}

impl TablePosMeasureUnit {
    /// The tolerance in points for one specific line.
    fn resolve(self, tolerance: f32, line: &PdfLine) -> f32 {
        let (x0, _, x1, _) = line.bbox().as_tuple();
        match self {
            TablePosMeasureUnit::Pt => tolerance,
            TablePosMeasureUnit::Perc => tolerance * (x1 - x0),
            TablePosMeasureUnit::Em => tolerance * *line.font_size(),
        }
    }
}

/// The parameters of [`get_table_coordinates_from_lines`], grouped into a struct.
///
/// There are eight of them, all with defaults, and a struct with a `Default` impl keeps the call
/// sites readable where eight positional arguments would not be.
#[derive(Debug, Clone)]
pub struct TableCoordinatesConfig {
    pub table_config: Option<TableConfig>,
    pub algorithm_flags: TablePosAlgorithm,
    pub collapse_algorithm: CollapseAlgorithm,
    pub tolerance: f32,
    pub tolerance_unit: TablePosMeasureUnit,
    /// Index of the column holding the company name: the only one allowed to break across several
    /// lines, since long names wrap. See [`get_table_coordinates_from_lines`] for the caveat about
    /// when this actually takes effect.
    pub company_col: Option<usize>,
    pub collapse: bool,
}

// `Default` is written out rather than derived, because `TablePosAlgorithm` and `CollapseAlgorithm`
// do not implement it and adding it there would mean touching two modules that are deliberately
// kept as they are.
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

/// The `(row, column)` coordinates of every PDF line of a table.
///
/// Builds one [`CellGeometry`] per line with the tolerance resolved according to
/// [`TablePosMeasureUnit`], delegates to [`coordinates::get_table_coordinates`], and collapses
/// multi-line rows if asked to.
///
/// # A caveat worth knowing
///
/// The `company_col` branch builds its column configuration *after* the coordinates have already
/// been computed, so it takes effect only when `collapse` is `true`. With `collapse: false` — the
/// only case the standard pipes actually use — it is a configuration that is built and never read.
/// This is long-standing behaviour and is left as it is rather than quietly changed.
///
/// The same branch does size its column vector by the highest index actually observed, plus one,
/// computed over an iterator so that a single-cell table is not a special case. A vector one
/// column short would make the collapse step panic downstream, and a panic on the user's path is
/// not an acceptable way to report a malformed table.
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
        // Every column gets its own [`ColumnConfig`]. Sharing one value between columns would make
        // setting `splitting` on a single column change them all; here the values are copies, so
        // that cannot happen.
        let mut cols = vec![
            ColumnConfig { limits: None, splitting: Some(SplittingState::Disallow), nullable: None };
            max_col + 1
        ];
        if let Some(col) = cols.get_mut(company_col) {
            // `None` means "no constraint", which `collapse` reads as allowing a downward split.
            col.splitting = None;
        } else {
            tracing::warn!(
                company_col,
                columns = cols.len(),
                "company_col is out of range for this table - ignored, every column stays non-splittable"
            );
        }
        table_config.cols = Some(cols);
    }

    let coords = if config.collapse {
        collapse::collapse_table_rows(coords, &table_config, config.collapse_algorithm)
    } else {
        coords
    };
    let rows = coords.iter().map(|(row, _)| *row).max().map_or(0, |m| m + 1);
    let cols = coords.iter().map(|(_, col)| *col).max().map_or(0, |m| m + 1);
    tracing::debug!(rows, cols, cells = coords.len(), "table tabularized");
    Ok(coords)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::position::{RowConfig, SplittingDirection};

    /// A line with explicit bounding box and size: geometry and `font_size` are the only attributes
    /// the wrapper reads.
    fn line(text: &str, bbox: (f32, f32, f32, f32), font_size: f32) -> PdfLine {
        PdfLine::new("Arial", font_size, text, bbox)
    }

    /// A regular 2x2 table: two rows, two columns, cells well apart.
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
            // A single column must not be a failure case.
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
            // Three columns declared against two actually present: the error comes from
            // `coordinates`.
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
            // `BigCellRule` changes which cell acts as the ruler: on a regular table the result is
            // unchanged, but the call must not fail.
            let cfg = TableCoordinatesConfig { algorithm_flags: TablePosAlgorithm::BigCellRule, ..Default::default() };
            let coords = get_table_coordinates_from_lines(&two_by_two(), &cfg).unwrap();
            assert_eq!(coords.len(), 4);
        }
    }

    mod tolerance_effect {
        use super::*;

        /// Two nearly aligned cells: without tolerance they are two columns, with a large enough
        /// tolerance they become one.
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
            // 2 em at size 10 is 20 pt: the same effect as the test in points above.
            let cfg = TableCoordinatesConfig { tolerance: 2.0, tolerance_unit: TablePosMeasureUnit::Em, ..Default::default() };
            let coords = get_table_coordinates_from_lines(&nearly_aligned(), &cfg).unwrap();
            assert_eq!(coords[0].1, coords[1].1);
        }

        #[test]
        fn the_same_tolerance_expressed_in_perc_depends_on_the_line_width() {
            // Twice the width (10 pt) is 20 pt.
            let cfg = TableCoordinatesConfig { tolerance: 2.0, tolerance_unit: TablePosMeasureUnit::Perc, ..Default::default() };
            let coords = get_table_coordinates_from_lines(&nearly_aligned(), &cfg).unwrap();
            assert_eq!(coords[0].1, coords[1].1);
        }
    }

    mod company_col_branch {
        use super::*;

        /// A table with a cell missing in the first column: the incomplete row is collapsible for
        /// the geometric algorithm.
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

        /// A table with the missing cell in column 0: it is column 1 that carries the extra line,
        /// so it is the one that has to collapse — or not, depending on its `splitting`.
        fn ragged_on_the_second_column() -> Vec<PdfLine> {
            vec![
                line("r0c0", (0.0, 0.0, 20.0, 10.0), 10.0),
                line("r0c1", (30.0, 0.0, 50.0, 10.0), 10.0),
                line("r1c1", (30.0, 20.0, 50.0, 30.0), 10.0),
            ]
        }

        #[test]
        fn company_col_makes_that_column_the_only_splittable_one_when_collapsing() {
            // Without `company_col` every column may split and the extra line collapses; with
            // `company_col: Some(0)` column 1 becomes `Disallow` and stays where it is.
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
            // `table_config.cols` already set: it is not overwritten.
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
