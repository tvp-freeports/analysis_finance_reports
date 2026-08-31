//! Table configuration and line grouping: [`InputArea`], [`RowConfig`], [`ColumnConfig`],
//! [`TableConfig`], [`get_groups`].
//!
//! These are the pure configuration data a format writes about a table — which limits a row or
//! column has, how a column collapses, whether it may be null — plus the primitive that recovers
//! rows and columns out of a page's geometry by grouping lines that sit close together along one
//! axis.
//!
//! The configuration types live here rather than in [`super::tabularizer`] to keep the dependency
//! between the two modules one-way: `tabularizer` needs the configuration, the configuration does
//! not need `tabularizer`.
//!
//! [`InputArea`] validates through a `Result` and never panics, unlike the geometric primitives of
//! [`crate::commons::geometry`]. The difference is where the value comes from: a rectangle is an
//! internal invariant, while an input area is written by a format author in a configuration file,
//! and bad configuration deserves a message rather than a crash.

use crate::commons::geometry::Limits;

use super::pdf_line::PdfLine;

// `SplittingDirection`, `SplittingState` and `NullableState` are defined here rather than in
// `tabularizer::collapse` to avoid a cycle: [`ColumnConfig`] needs them for its fields, and
// `collapse` needs [`ColumnConfig`]. One direction suffices, and `collapse` re-exports them from
// their original path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplittingDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplittingState {
    Allow(SplittingDirection),
    Disallow,
}

pub type NullableState = bool;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowConfig {
    pub limits: Option<Limits>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnConfig {
    pub limits: Option<Limits>,
    pub splitting: Option<SplittingState>,
    pub nullable: Option<NullableState>,
}

#[derive(Debug, Clone)]
pub struct TableConfig {
    pub cols: Option<Vec<ColumnConfig>>,
    pub rows: Option<Vec<RowConfig>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PositionError {
    #[error("x_min must be positive, found {0}")]
    XMinNotPositive(f32),
    #[error("x_max must be positive, found {0}")]
    XMaxNotPositive(f32),
    #[error("y_min must be positive, found {0}")]
    YMinNotPositive(f32),
    #[error("y_max must be positive, found {0}")]
    YMaxNotPositive(f32),
    #[error("x_max ({x_max}) must be greater than x_min ({x_min})")]
    XBoundsInverted { x_min: f32, x_max: f32 },
    #[error("y_max ({y_max}) must be greater than y_min ({y_min})")]
    YBoundsInverted { y_min: f32, y_max: f32 },
    #[error("get_groups called with an empty list of lines")]
    EmptyLines,
}

/// An optional rectangular input area, validated from external configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputArea {
    x_min: Option<f32>,
    x_max: Option<f32>,
    y_min: Option<f32>,
    y_max: Option<f32>,
}

fn validate_positive(v: Option<f32>, err: impl Fn(f32) -> PositionError) -> Result<(), PositionError> {
    match v {
        Some(v) if v <= 0.0 => Err(err(v)),
        _ => Ok(()),
    }
}

impl InputArea {
    pub fn build(x_min: Option<f32>, x_max: Option<f32>, y_min: Option<f32>, y_max: Option<f32>) -> Result<Self, PositionError> {
        validate_positive(x_min, PositionError::XMinNotPositive)?;
        validate_positive(x_max, PositionError::XMaxNotPositive)?;
        validate_positive(y_min, PositionError::YMinNotPositive)?;
        validate_positive(y_max, PositionError::YMaxNotPositive)?;
        if let (Some(mn), Some(mx)) = (x_min, x_max)
            && mx <= mn
        {
            return Err(PositionError::XBoundsInverted { x_min: mn, x_max: mx });
        }
        if let (Some(mn), Some(mx)) = (y_min, y_max)
            && mx <= mn
        {
            return Err(PositionError::YBoundsInverted { y_min: mn, y_max: mx });
        }
        Ok(Self { x_min, x_max, y_min, y_max })
    }

    pub fn x_min(&self) -> Option<f32> {
        self.x_min
    }
    pub fn x_max(&self) -> Option<f32> {
        self.x_max
    }
    pub fn y_min(&self) -> Option<f32> {
        self.y_min
    }
    pub fn y_max(&self) -> Option<f32> {
        self.y_max
    }
}

/// Groups `lines` by proximity along one axis.
///
/// Takes each line's coordinate along the chosen axis, sorts them, and starts a new group id every
/// time two consecutive values are at least `threshold` apart. This is how the rows and columns of
/// a table are recovered from a page that never declared it had a table.
///
/// The result follows the order of the **sorted** keys, not the order of `lines` as given.
///
/// # Errors
///
/// [`PositionError::EmptyLines`] if `lines` is empty: there is no grouping of nothing, and
/// returning an empty vector would let the caller mistake it for one group.
pub fn get_groups(lines: &[PdfLine], threshold: f32, vertical: bool) -> Result<Vec<i64>, PositionError> {
    if lines.is_empty() {
        return Err(PositionError::EmptyLines);
    }
    let geoindex = if vertical { 1 } else { 0 };
    let mut keys: Vec<f32> = lines
        .iter()
        .map(|l| {
            let bbox = l.bbox().as_tuple();
            [bbox.0, bbox.1, bbox.2, bbox.3][geoindex]
        })
        .collect();
    keys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut groups = Vec::with_capacity(keys.len());
    let mut group_id: i64 = 0;
    let mut a = keys[0];
    for b in keys {
        if (b - a).abs() >= threshold {
            group_id += 1;
        }
        a = b;
        groups.push(group_id);
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::geometry::Limits;
    use crate::formats_utils::pdf_extract::tabularizer::collapse::{SplittingDirection, SplittingState};

    mod row_column_table_config {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn row_config_just_wraps_optional_limits() {
            let none = RowConfig { limits: None };
            let some = RowConfig { limits: Some(Limits::new(1.0, 2.0)) };
            assert_eq!(none.limits, None);
            assert_eq!(some.limits.unwrap().as_tuple(), (1.0, 2.0));
        }

        #[test]
        fn column_config_holds_limits_splitting_and_nullable_independently() {
            let col = ColumnConfig { limits: Some(Limits::new(0.0, 10.0)), splitting: Some(SplittingState::Allow(SplittingDirection::Up)), nullable: Some(true) };
            assert_eq!(col.limits.unwrap().as_tuple(), (0.0, 10.0));
            assert_eq!(col.splitting, Some(SplittingState::Allow(SplittingDirection::Up)));
            assert_eq!(col.nullable, Some(true));
        }

        #[test]
        fn table_config_holds_independent_column_configs_not_aliased_clones() {
            let base = ColumnConfig { limits: None, splitting: None, nullable: None };
            let mut cols = vec![base; 3];
            cols[1].splitting = Some(SplittingState::Disallow);
            let table = TableConfig { cols: Some(cols), rows: None };
            let cols = table.cols.unwrap();
            assert_eq!(cols[0].splitting, None);
            assert_eq!(cols[1].splitting, Some(SplittingState::Disallow));
            assert_eq!(cols[2].splitting, None);
        }
    }

    mod input_area_construction {
        use super::*;

        #[test]
        fn accepts_all_bounds_present_and_correctly_ordered() {
            let area = InputArea::build(Some(1.0), Some(10.0), Some(2.0), Some(20.0)).unwrap();
            assert_eq!(area.x_min(), Some(1.0));
            assert_eq!(area.x_max(), Some(10.0));
            assert_eq!(area.y_min(), Some(2.0));
            assert_eq!(area.y_max(), Some(20.0));
        }

        #[test]
        fn accepts_all_bounds_missing() {
            let area = InputArea::build(None, None, None, None).unwrap();
            assert_eq!((area.x_min(), area.x_max(), area.y_min(), area.y_max()), (None, None, None, None));
        }

        #[test]
        fn accepts_a_single_bound_with_no_partner_to_compare_against() {
            let area = InputArea::build(Some(5.0), None, None, None).unwrap();
            assert_eq!(area.x_min(), Some(5.0));
        }

        // The comparisons below use `if let` plus `assert_eq!` on the fields rather than `matches!`
        // against a literal: a floating-point literal pattern is rejected by the compiler.

        #[test]
        fn rejects_non_positive_x_min() {
            let err = InputArea::build(Some(0.0), None, None, None).unwrap_err();
            let PositionError::XMinNotPositive(v) = err else { panic!("expected XMinNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn rejects_negative_x_min() {
            let err = InputArea::build(Some(-1.0), None, None, None).unwrap_err();
            let PositionError::XMinNotPositive(v) = err else { panic!("expected XMinNotPositive, got {err:?}") };
            assert_eq!(v, -1.0);
        }

        #[test]
        fn rejects_non_positive_x_max() {
            let err = InputArea::build(None, Some(0.0), None, None).unwrap_err();
            let PositionError::XMaxNotPositive(v) = err else { panic!("expected XMaxNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn rejects_non_positive_y_min() {
            let err = InputArea::build(None, None, Some(0.0), None).unwrap_err();
            let PositionError::YMinNotPositive(v) = err else { panic!("expected YMinNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn rejects_non_positive_y_max() {
            let err = InputArea::build(None, None, None, Some(0.0)).unwrap_err();
            let PositionError::YMaxNotPositive(v) = err else { panic!("expected YMaxNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn rejects_x_max_not_greater_than_x_min() {
            let err = InputArea::build(Some(10.0), Some(5.0), None, None).unwrap_err();
            let PositionError::XBoundsInverted { x_min, x_max } = err else { panic!("expected XBoundsInverted, got {err:?}") };
            assert_eq!((x_min, x_max), (10.0, 5.0));
        }

        #[test]
        fn rejects_x_max_equal_to_x_min() {
            let err = InputArea::build(Some(10.0), Some(10.0), None, None).unwrap_err();
            let PositionError::XBoundsInverted { x_min, x_max } = err else { panic!("expected XBoundsInverted, got {err:?}") };
            assert_eq!((x_min, x_max), (10.0, 10.0));
        }

        #[test]
        fn rejects_y_max_not_greater_than_y_min() {
            let err = InputArea::build(None, None, Some(10.0), Some(5.0)).unwrap_err();
            let PositionError::YBoundsInverted { y_min, y_max } = err else { panic!("expected YBoundsInverted, got {err:?}") };
            assert_eq!((y_min, y_max), (10.0, 5.0));
        }
    }

    mod get_groups_behavior {
        use super::*;

        fn line_at(x: f32, y: f32) -> PdfLine {
            PdfLine::new("Arial", 10.0, "x", (x, y, x + 1.0, y + 1.0))
        }

        #[test]
        fn splits_on_threshold_along_the_vertical_axis_by_default() {
            let lines = vec![line_at(0.0, 0.0), line_at(0.0, 1.0), line_at(0.0, 10.0)];
            assert_eq!(get_groups(&lines, 5.0, true).unwrap(), vec![0, 0, 1]);
        }

        #[test]
        fn uses_the_horizontal_axis_when_not_vertical() {
            let lines = vec![line_at(0.0, 0.0), line_at(10.0, 0.0)];
            assert_eq!(get_groups(&lines, 5.0, false).unwrap(), vec![0, 1]);
        }

        #[test]
        fn returns_group_ids_in_sorted_key_order_not_in_input_order() {
            // Input order is y = 10, 0, 5, and the ids come back in the order of the *sorted* keys
            // (0, 1, 2), not in the order of the input lines.
            let lines = vec![line_at(0.0, 10.0), line_at(0.0, 0.0), line_at(0.0, 5.0)];
            assert_eq!(get_groups(&lines, 3.0, true).unwrap(), vec![0, 1, 2]);
        }

        #[test]
        fn keeps_close_values_in_the_same_group() {
            let lines = vec![line_at(0.0, 0.0), line_at(0.0, 1.0), line_at(0.0, 1.5)];
            assert_eq!(get_groups(&lines, 5.0, true).unwrap(), vec![0, 0, 0]);
        }

        #[test]
        fn errors_on_an_empty_list_of_lines() {
            let lines: Vec<PdfLine> = vec![];
            assert!(matches!(get_groups(&lines, 1.0, true), Err(PositionError::EmptyLines)));
        }
    }
}
