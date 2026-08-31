//! [`Rectangle`], [`Limits`], [`PositiveLimits`]: the basic geometric primitives.
//!
//! Validated construction, `as_tuple`, and `Display` — nothing else. The set algebra over these
//! types ([`Container`](crate::commons::sets::Container) and friends) deliberately lives with the
//! selections that use it, in [`crate::formats_utils::pdf_extract`], so that the data does not
//! depend on the selections.
//!
//! # Two constructors, on purpose
//!
//! `build` returns a `Result` and `new` panics. These are internal geometric invariants, not user
//! input: a rectangle whose left edge is right of its right edge is a bug in the caller, and a
//! panic is the honest report. Where the values *do* come from outside — a page dict, a format's
//! configuration — the caller uses `build` and handles the error, which is exactly what
//! [`crate::input::document::page_dict`] does.
//!
//! A degenerate interval (`a == b`) is **not** valid: bounds are strict.
//!
//! All three types are `Eq + Hash`, which is what lets rectangles be deduplicated in a set.

use ordered_float::OrderedFloat;
use std::fmt;

// `f32` implements `PartialEq` but not `Eq` (NaN), so `Eq` for these error enums is added with a
// manual marker impl instead of `#[derive(Eq)]` — sound because `Eq` has no methods of its own,
// it only promises that the existing `PartialEq` is reflexive, which the doc contract requires.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum LimitsBuildError {
    #[error(
        "left limit bound can't be bigger than right one, found left '{0}' and right '{1}'"
    )]
    NegativeInterval(f32, f32),
}
impl Eq for LimitsBuildError {}

#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum PositiveLimitsBuildError {
    #[error("left bound of positive limit can't be negative, found '{0}'")]
    LeftNegative(f32),
    #[error("right bound of positive limit can't be negative, found '{0}'")]
    RightNegative(f32),
    #[error("{0}")]
    InvalidLimit(LimitsBuildError),
}
impl Eq for PositiveLimitsBuildError {}

#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum RectangleBuildError {
    #[error(
        "left side of a rectangle can't be bigger than right one, found left '{a}' and right '{b}'",
        a = .0.left(),
        b = .0.right()
    )]
    Horizontal(LimitsBuildError),
    #[error(
        "top side of a rectangle can't be bigger than bottom one, found top '{a}' and bottom '{b}'",
        a = .0.left(),
        b = .0.right()
    )]
    Vertical(LimitsBuildError),
}
impl Eq for RectangleBuildError {}

impl LimitsBuildError {
    fn left(&self) -> f32 {
        let Self::NegativeInterval(a, _) = self;
        *a
    }
    fn right(&self) -> f32 {
        let Self::NegativeInterval(_, b) = self;
        *b
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct Limits(OrderedFloat<f32>, OrderedFloat<f32>);

impl Limits {
    pub fn build(a: f32, b: f32) -> Result<Self, LimitsBuildError> {
        use LimitsBuildError::*;
        if a >= b {
            Err(NegativeInterval(a, b))
        } else {
            Ok(Self(OrderedFloat(a), OrderedFloat(b)))
        }
    }

    pub fn new(a: f32, b: f32) -> Self {
        Self::build(a, b).unwrap_or_else(|err| panic!("{err}"))
    }

    pub fn as_tuple(&self) -> (f32, f32) {
        (self.0.into_inner(), self.1.into_inner())
    }
}

impl fmt::Display for Limits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Limits(a, b) = self;
        write!(f, "[{a}:{b}]")
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct PositiveLimits(Limits);

impl PositiveLimits {
    pub fn build(a: f32, b: f32) -> Result<Self, PositiveLimitsBuildError> {
        use PositiveLimitsBuildError::*;
        if a < 0.0 {
            Err(LeftNegative(a))
        } else if b < 0.0 {
            Err(RightNegative(b))
        } else {
            Limits::build(a, b).map(Self).map_err(InvalidLimit)
        }
    }

    pub fn new(a: f32, b: f32) -> Self {
        Self::build(a, b).unwrap_or_else(|err| panic!("{err}"))
    }

    pub fn as_tuple(&self) -> (f32, f32) {
        self.0.as_tuple()
    }
}

impl fmt::Display for PositiveLimits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct Rectangle {
    x0: OrderedFloat<f32>,
    y0: OrderedFloat<f32>,
    x1: OrderedFloat<f32>,
    y1: OrderedFloat<f32>,
}

impl Rectangle {
    pub fn build(x0: f32, y0: f32, x1: f32, y1: f32) -> Result<Self, RectangleBuildError> {
        Limits::build(x0, x1).map_err(RectangleBuildError::Horizontal)?;
        Limits::build(y0, y1).map_err(RectangleBuildError::Vertical)?;
        Ok(Self {
            x0: OrderedFloat(x0),
            y0: OrderedFloat(y0),
            x1: OrderedFloat(x1),
            y1: OrderedFloat(y1),
        })
    }

    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::build(x0, y0, x1, y1).unwrap_or_else(|err| panic!("{err}"))
    }

    pub fn as_tuple(&self) -> (f32, f32, f32, f32) {
        (
            self.x0.into_inner(),
            self.y0.into_inner(),
            self.x1.into_inner(),
            self.y1.into_inner(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod limits {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;
        use LimitsBuildError::*;

        #[test]
        fn build_accepts_a_strictly_increasing_pair() {
            match Limits::build(20.3, 30.7) {
                Ok(Limits(a, b)) => {
                    assert_eq!(a, 20.3);
                    assert_eq!(b, 30.7);
                }
                Err(err) => panic!("expected a valid limit, found error {err}"),
            }
        }

        #[test_case(30.1, 20.0, NegativeInterval(30.1, 20.0); "left strictly greater than right")]
        #[test_case(20.0, 20.0, NegativeInterval(20.0, 20.0); "degenerate interval, left equal to right")]
        fn build_rejects_a_non_increasing_pair(a: f32, b: f32, expected: LimitsBuildError) {
            assert_eq!(Limits::build(a, b), Err(expected));
        }

        #[test_case(
            Ok(Limits(OrderedFloat(-10.1), OrderedFloat(20.1))),
            "[-10.1:20.1]";
            "valid limit"
        )]
        #[test_case(
            Err(NegativeInterval(30.1, 20.0)),
            "left limit bound can't be bigger than right one, found left '30.1' and right '20'";
            "invalid interval error message"
        )]
        fn formats_as_expected(x: Result<Limits, LimitsBuildError>, expected: &str) {
            match x {
                Ok(value) => assert_eq!(format!("{value}"), expected),
                Err(err) => assert_eq!(format!("{err}"), expected),
            }
        }

        #[test]
        fn new_returns_the_built_value_on_success() {
            let Limits(a, b) = Limits::new(-20.3, 30.7);
            assert_eq!(a, -20.3);
            assert_eq!(b, 30.7);
        }

        #[test]
        #[should_panic = "left limit bound can't be bigger than right one, found left '22.2' and right '11.1'"]
        fn new_panics_on_a_non_increasing_pair() {
            Limits::new(22.2, 11.1);
        }

        #[test]
        fn as_tuple_round_trips_the_constructor_arguments() {
            let limits = Limits::new(9.0, 99.0);
            assert_eq!(limits.as_tuple(), (9.0, 99.0));
        }

        #[test]
        fn implements_std_error() {
            fn assert_error<T: std::error::Error>() {}
            assert_error::<LimitsBuildError>();
        }

        #[test]
        fn equal_limits_built_separately_collapse_in_a_hashset() {
            use std::collections::HashSet;
            let mut set = HashSet::new();
            set.insert(Limits::new(1.0, 2.0));
            set.insert(Limits::new(1.0, 2.0));
            assert_eq!(set.len(), 1);
        }
    }

    mod positive_limits {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;
        use PositiveLimitsBuildError::*;

        #[test]
        fn build_accepts_a_non_negative_strictly_increasing_pair() {
            match PositiveLimits::build(20.3, 30.7) {
                Ok(PositiveLimits(Limits(a, b))) => {
                    assert_eq!(a, 20.3);
                    assert_eq!(b, 30.7);
                }
                Err(err) => panic!("expected a valid positive limit, found error {err}"),
            }
        }

        #[test]
        fn build_accepts_zero_as_the_left_bound() {
            assert!(PositiveLimits::build(0.0, 1.0).is_ok());
        }

        #[test_case(-20.0, 30.1, LeftNegative(-20.0); "left bound negative")]
        #[test_case(20.0, -30.1, RightNegative(-30.1); "right bound negative")]
        #[test_case(
            30.1, 20.0, InvalidLimit(LimitsBuildError::NegativeInterval(30.1, 20.0));
            "both bounds non-negative but left greater than right"
        )]
        fn build_rejects_invalid_combinations(a: f32, b: f32, expected: PositiveLimitsBuildError) {
            assert_eq!(PositiveLimits::build(a, b), Err(expected));
        }

        #[test]
        fn build_reports_left_negative_even_when_right_is_also_negative() {
            // left bound is checked first, matching the reference implementation.
            assert_eq!(PositiveLimits::build(-5.0, -1.0), Err(LeftNegative(-5.0)));
        }

        #[test_case(
            Ok(PositiveLimits(Limits(OrderedFloat(10.1), OrderedFloat(20.1)))),
            "[10.1:20.1]";
            "valid limit"
        )]
        #[test_case(
            Err(LeftNegative(-20.1)),
            "left bound of positive limit can't be negative, found '-20.1'";
            "left negative error message"
        )]
        #[test_case(
            Err(RightNegative(-30.1)),
            "right bound of positive limit can't be negative, found '-30.1'";
            "right negative error message"
        )]
        #[test_case(
            Err(InvalidLimit(LimitsBuildError::NegativeInterval(30.1, 20.0))),
            "left limit bound can't be bigger than right one, found left '30.1' and right '20'";
            "invalid limit error message delegates to the inner error"
        )]
        fn formats_as_expected(x: Result<PositiveLimits, PositiveLimitsBuildError>, expected: &str) {
            match x {
                Ok(value) => assert_eq!(format!("{value}"), expected),
                Err(err) => assert_eq!(format!("{err}"), expected),
            }
        }

        #[test]
        fn new_returns_the_built_value_on_success() {
            let PositiveLimits(Limits(a, b)) = PositiveLimits::new(20.3, 30.7);
            assert_eq!(a, 20.3);
            assert_eq!(b, 30.7);
        }

        #[test]
        #[should_panic = "left bound of positive limit can't be negative, found '-20.3'"]
        fn new_panics_on_left_negative() {
            PositiveLimits::new(-20.3, 31.1);
        }

        #[test]
        #[should_panic = "right bound of positive limit can't be negative, found '-35.1'"]
        fn new_panics_on_right_negative() {
            PositiveLimits::new(25.67, -35.1);
        }

        #[test]
        #[should_panic = "left limit bound can't be bigger than right one, found left '22.2' and right '11.1'"]
        fn new_panics_on_non_increasing_pair() {
            PositiveLimits::new(22.2, 11.1);
        }

        #[test]
        fn as_tuple_round_trips_the_constructor_arguments() {
            let limits = PositiveLimits::new(9.0, 99.0);
            assert_eq!(limits.as_tuple(), (9.0, 99.0));
        }

        #[test]
        fn implements_std_error() {
            fn assert_error<T: std::error::Error>() {}
            assert_error::<PositiveLimitsBuildError>();
        }
    }

    mod rectangle {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;
        use LimitsBuildError::*;
        use RectangleBuildError::*;

        #[test]
        fn build_accepts_strictly_increasing_bounds_on_both_axes() {
            match Rectangle::build(20.3, 1.0, 200.3, 5.0) {
                Ok(Rectangle { x0, y0, x1, y1 }) => {
                    assert_eq!(x0, 20.3);
                    assert_eq!(y0, 1.0);
                    assert_eq!(x1, 200.3);
                    assert_eq!(y1, 5.0);
                }
                Err(err) => panic!("expected a valid rectangle, found error {err:?}"),
            }
        }

        #[test_case(30.1, 0.1, 20.0, 0.2, Horizontal(NegativeInterval(30.1, 20.0)); "invalid width")]
        #[test_case(20.0, 0.2, 30.1, 0.1, Vertical(NegativeInterval(0.2, 0.1)); "invalid height")]
        #[test_case(20.0, 1.0, 20.0, 40.0, Horizontal(NegativeInterval(20.0, 20.0)); "degenerate width")]
        #[test_case(20.0, 20.0, 30.1, 20.0, Vertical(NegativeInterval(20.0, 20.0)); "degenerate height, checked after a valid width")]
        fn build_rejects_a_non_increasing_axis(x0: f32, y0: f32, x1: f32, y1: f32, expected: RectangleBuildError) {
            assert_eq!(Rectangle::build(x0, y0, x1, y1), Err(expected));
        }

        #[test_case(
            Horizontal(NegativeInterval(302.1, 20.0)),
            "left side of a rectangle can't be bigger than right one, found left '302.1' and right '20'";
            "invalid width error message"
        )]
        #[test_case(
            Vertical(NegativeInterval(300.1, 202.0)),
            "top side of a rectangle can't be bigger than bottom one, found top '300.1' and bottom '202'";
            "invalid height error message"
        )]
        fn formats_as_expected(err: RectangleBuildError, expected: &str) {
            assert_eq!(format!("{err}"), expected);
        }

        #[test]
        fn new_returns_the_built_value_on_success() {
            let Rectangle { x0, y0, x1, y1 } = Rectangle::new(20.3, 8.0, 30.7, 78.3);
            assert_eq!(x0, 20.3);
            assert_eq!(x1, 30.7);
            assert_eq!(y0, 8.0);
            assert_eq!(y1, 78.3);
        }

        #[test]
        #[should_panic = "left side of a rectangle can't be bigger than right one, found left '22.2' and right '11.1'"]
        fn new_panics_on_invalid_width() {
            Rectangle::new(22.2, 400.0, 11.1, 444.0);
        }

        #[test]
        #[should_panic = "top side of a rectangle can't be bigger than bottom one, found top '480' and bottom '444'"]
        fn new_panics_on_invalid_height() {
            Rectangle::new(2.54, 480.0, 11.1, 444.0);
        }

        #[test]
        fn as_tuple_round_trips_the_constructor_arguments() {
            let rectangle = Rectangle::new(9.0, 6.2, 99.0, 22.3);
            assert_eq!(rectangle.as_tuple(), (9.0, 6.2, 99.0, 22.3));
        }

        #[test]
        fn implements_std_error() {
            fn assert_error<T: std::error::Error>() {}
            assert_error::<RectangleBuildError>();
        }

        #[test]
        fn equal_rectangles_built_separately_collapse_in_a_hashset() {
            // Required by the area selections, which deduplicate rectangles through a set; this is
            // the load-bearing invariant that relies on.
            use std::collections::HashSet;
            let mut set = HashSet::new();
            set.insert(Rectangle::new(3.4, 4.5, 4.5, 56.0));
            set.insert(Rectangle::new(3.4, 4.5, 4.5, 56.0));
            assert_eq!(set.len(), 1);
        }

        #[test]
        fn distinct_rectangles_both_survive_in_a_hashset() {
            use std::collections::HashSet;
            let mut set = HashSet::new();
            set.insert(Rectangle::new(0.0, 0.0, 1.0, 1.0));
            set.insert(Rectangle::new(0.0, 0.0, 2.0, 2.0));
            assert_eq!(set.len(), 2);
        }
    }
}
