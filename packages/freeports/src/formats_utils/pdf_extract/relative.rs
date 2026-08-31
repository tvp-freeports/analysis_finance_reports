//! [`OptionallyRelative`] and the generic machinery behind relative selections.
//!
//! Some things a format looks for are stated absolutely — a font size, a rectangle in page
//! coordinates — and others only make sense against the rest of the page: *the line below the one
//! saying "Total"*, *the column to the right of the ISIN*. [`RelativeInfo`] is what turns the
//! second kind into the first, given the page's other lines.
//!
//! The context is always and only "the other lines of the same page", so [`RelativeInfo`] is not
//! generic over a context type: `contextualize` takes a `&[PdfLine]` and nothing else. Making it
//! generic would buy an abstraction no caller in this crate can use.
//!
//! It lives one level above `select` so that it can be imported both by the relative selections
//! themselves and by anything else in `pdf_extract` that needs it, without going through `select`.

use super::pdf_line::PdfLine;

/// A value that can be *contextualised*: given the page's other lines, it produces a `V`.
pub trait RelativeInfo<V> {
    fn contextualize(self, lines: &[PdfLine]) -> V;
}

/// Either an absolute value, or one relative to the page's context.
pub enum OptionallyRelative<V, R> {
    Absolute(V),
    Relative(R),
}

/// Written by hand rather than derived: `#[derive(Clone)]` would add a `V: Clone, R: Clone` bound
/// to the *type* rather than to this impl, and the same applies to any other derive added later —
/// each would force both parameters to implement it at once.
impl<V, R> Clone for OptionallyRelative<V, R>
where
    V: Clone,
    R: Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Absolute(a) => Self::Absolute(a.clone()),
            Self::Relative(a) => Self::Relative(a.clone()),
        }
    }
}

impl<V, R> RelativeInfo<V> for OptionallyRelative<V, R>
where
    R: RelativeInfo<V>,
{
    fn contextualize(self, lines: &[PdfLine]) -> V {
        match self {
            Self::Absolute(a) => a,
            Self::Relative(ra) => ra.contextualize(lines),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::pdf_line::PdfLine;

    /// A throwaway PDF line for the tests: the content does not matter, only the length of the
    /// slice handed to `contextualize`, which is how a relative implementation is shown to really
    /// receive its context.
    fn some_lines(n: usize) -> Vec<PdfLine> {
        (0..n)
            .map(|i| PdfLine::new("Arial", 10.0, &format!("line {i}"), (0.0, i as f32, 1.0, i as f32 + 1.0)))
            .collect()
    }

    /// A minimal `RelativeInfo<usize>` used only in tests: it returns the number of lines in the
    /// context, which tells "the context was read" apart from "the context was ignored" — the
    /// behaviour of `Absolute`.
    #[derive(Clone, PartialEq, Debug)]
    struct CountLines;

    impl RelativeInfo<usize> for CountLines {
        fn contextualize(self, lines: &[PdfLine]) -> usize {
            lines.len()
        }
    }

    mod optionally_relative {
        use super::*;

        #[test]
        fn absolute_variant_ignores_the_context_entirely() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Absolute(42);
            assert_eq!(value.contextualize(&some_lines(7)), 42);
        }

        #[test]
        fn absolute_variant_ignores_an_empty_context_too() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Absolute(42);
            assert_eq!(value.contextualize(&[]), 42);
        }

        #[test]
        fn relative_variant_delegates_contextualize_to_the_inner_r() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Relative(CountLines);
            assert_eq!(value.contextualize(&some_lines(3)), 3);
        }

        #[test]
        fn clone_of_absolute_variant_contextualizes_to_the_same_value() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Absolute(9);
            let cloned = value.clone();
            let lines = some_lines(2);
            assert_eq!(value.contextualize(&lines), cloned.contextualize(&lines));
        }

        #[test]
        fn clone_of_relative_variant_contextualizes_to_the_same_value() {
            let value: OptionallyRelative<usize, CountLines> = OptionallyRelative::Relative(CountLines);
            let cloned = value.clone();
            let lines = some_lines(5);
            assert_eq!(value.contextualize(&lines), cloned.contextualize(&lines));
        }
    }
}
