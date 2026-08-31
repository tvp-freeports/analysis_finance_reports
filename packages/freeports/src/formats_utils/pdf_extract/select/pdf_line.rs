//! Selecting lines: [`SelectPdfLineSet`] and [`PdfLineSet`].
//!
//! A selection says which lines of a page a pipe is interested in, and it is built as a set
//! expression over four primitive criteria — font, font size, text, area. Because they compose with
//! the usual set operators, a format states what it is after (`the bold lines inside this rectangle
//! whose text matches …`) instead of writing a loop.
//!
//! [`PdfLineSet::new`] intersects whichever of the four are given; with none given it degenerates
//! to "any font size at all", which is the identity for this algebra.

pub mod area;
pub mod font;
pub mod font_size;
pub mod text;

use area::Area;
use font::FontSet;
use font_size::FontSizeInterval;
use text::TextSet;

use crate::commons::sets::ast_simple::AstSet;
use crate::commons::sets::{Container, Overlappable, SetRelation};
use crate::formats_utils::pdf_extract::pdf_line::PdfLine;

#[derive(Debug, Clone)]
pub enum SelectPdfLineSet {
    Font(FontSet),
    FontSize(FontSizeInterval),
    Text(TextSet),
    Area(Area),
}

impl Container for SelectPdfLineSet {
    type Elem = PdfLine;
    /// No logging here, at any level.
    ///
    /// This is the leaf of every selection: it runs once per line per leaf per pipe per page, and
    /// the outcome of a single comparison is not information. What is worth knowing is *which line*
    /// a selection finally chose, and that is logged once, with its text, where the choice is made.
    fn contains(&self, ele: &PdfLine) -> bool {
        match self {
            Self::Font(a) => a.contains(ele.font()),
            Self::FontSize(a) => a.contains(ele.font_size()),
            Self::Text(a) => a.contains(ele.text()),
            Self::Area(a) => {
                let r = a.set_relation(&ele.area());
                r == SetRelation::Equal || r == SetRelation::Superset
            }
        }
    }
}

impl SelectPdfLineSet {
    pub fn select_font(font: &str) -> Self {
        Self::Font(FontSet::new(font))
    }
    pub fn select_fontsize(a: f32, b: f32) -> Self {
        Self::FontSize(FontSizeInterval::new(a, b))
    }
    pub fn select_text(text: &str) -> Self {
        Self::Text(TextSet::new(text))
    }
    pub fn select_area(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::Area(Area::new(x0, y0, x1, y1))
    }
}

pub type PdfLineSet = AstSet<SelectPdfLineSet, PdfLine>;

impl PdfLineSet {
    pub fn select_font(font: &str) -> Self {
        Self::from_leaf(SelectPdfLineSet::select_font(font))
    }
    pub fn select_fontsize(a: f32, b: f32) -> Self {
        Self::from_leaf(SelectPdfLineSet::FontSize(FontSizeInterval::new(a, b)))
    }
    pub fn select_text(text: &str) -> Self {
        Self::from_leaf(SelectPdfLineSet::Text(TextSet::new(text)))
    }
    pub fn select_area(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::from_leaf(SelectPdfLineSet::Area(Area::new(x0, y0, x1, y1)))
    }
    pub fn font(font: FontSet) -> Self {
        Self::from_leaf(SelectPdfLineSet::Font(font))
    }
    pub fn fontsize(font_size: FontSizeInterval) -> Self {
        Self::from_leaf(SelectPdfLineSet::FontSize(font_size))
    }
    pub fn text(text: TextSet) -> Self {
        Self::from_leaf(SelectPdfLineSet::Text(text))
    }
    pub fn area(area: Area) -> Self {
        Self::from_leaf(SelectPdfLineSet::Area(area))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new(font: Option<&str>, font_size: Option<(f32, f32)>, text: Option<&str>, area: Option<(f32, f32, f32, f32)>) -> Self {
        match (font, font_size, text, area) {
            (None, None, None, None) => Self::select_fontsize(0.0, 1e6),
            (Some(f), None, None, None) => Self::select_font(f),
            (None, Some((a, b)), None, None) => Self::select_fontsize(a, b),
            (None, None, Some(t), None) => Self::select_text(t),
            (None, None, None, Some((x0, y0, x1, y1))) => Self::select_area(x0, y0, x1, y1),
            (Some(f), Some((a, b)), None, None) => Self::select_font(f) & Self::select_fontsize(a, b),
            (Some(f), None, Some(t), None) => Self::select_font(f) & Self::select_text(t),
            (Some(f), None, None, Some((x0, y0, x1, y1))) => Self::select_font(f) & Self::select_area(x0, y0, x1, y1),
            (None, Some((a, b)), Some(t), None) => Self::select_fontsize(a, b) & Self::select_text(t),
            (None, Some((a, b)), None, Some((x0, y0, x1, y1))) => Self::select_fontsize(a, b) & Self::select_area(x0, y0, x1, y1),
            (None, None, Some(t), Some((x0, y0, x1, y1))) => Self::select_text(t) & Self::select_area(x0, y0, x1, y1),
            (Some(f), Some((a, b)), Some(t), None) => Self::select_font(f) & Self::select_fontsize(a, b) & Self::select_text(t),
            (Some(f), Some((a, b)), None, Some((x0, y0, x1, y1))) => {
                Self::select_font(f) & Self::select_fontsize(a, b) & Self::select_area(x0, y0, x1, y1)
            }
            (Some(f), None, Some(t), Some((x0, y0, x1, y1))) => Self::select_font(f) & Self::select_text(t) & Self::select_area(x0, y0, x1, y1),
            (None, Some((a, b)), Some(t), Some((x0, y0, x1, y1))) => {
                Self::select_fontsize(a, b) & Self::select_text(t) & Self::select_area(x0, y0, x1, y1)
            }
            (Some(f), Some((a, b)), Some(t), Some((x0, y0, x1, y1))) => {
                Self::select_font(f) & Self::select_fontsize(a, b) & Self::select_text(t) & Self::select_area(x0, y0, x1, y1)
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn from_sets(font: Option<FontSet>, font_size: Option<FontSizeInterval>, text: Option<TextSet>, area: Option<Area>) -> Self {
        match (font, font_size, text, area) {
            (None, None, None, None) => Self::fontsize(FontSizeInterval::new(0.0, 1e6)),
            (Some(f), None, None, None) => Self::font(f),
            (None, Some(fs), None, None) => Self::fontsize(fs),
            (None, None, Some(t), None) => Self::text(t),
            (None, None, None, Some(a)) => Self::area(a),
            (Some(f), Some(fs), None, None) => Self::font(f) & Self::fontsize(fs),
            (Some(f), None, Some(t), None) => Self::font(f) & Self::text(t),
            (Some(f), None, None, Some(a)) => Self::font(f) & Self::area(a),
            (None, Some(fs), Some(t), None) => Self::fontsize(fs) & Self::text(t),
            (None, Some(fs), None, Some(a)) => Self::fontsize(fs) & Self::area(a),
            (None, None, Some(t), Some(a)) => Self::text(t) & Self::area(a),
            (Some(f), Some(fs), Some(t), None) => Self::font(f) & Self::fontsize(fs) & Self::text(t),
            (Some(f), Some(fs), None, Some(a)) => Self::font(f) & Self::fontsize(fs) & Self::area(a),
            (Some(f), None, Some(t), Some(a)) => Self::font(f) & Self::text(t) & Self::area(a),
            (None, Some(fs), Some(t), Some(a)) => Self::fontsize(fs) & Self::text(t) & Self::area(a),
            (Some(f), Some(fs), Some(t), Some(a)) => Self::font(f) & Self::fontsize(fs) & Self::text(t) & Self::area(a),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::sets::Container;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;

    mod select_pdf_line_set_variants {
        use super::*;
        use test_case::test_case;

        #[test_case(SelectPdfLineSet::select_font("ARIAL"), PdfLine::new("Arial\n", 43.2, "rumi", (0.0, 0.0, 1.0, 1.0)); "font")]
        #[test_case(SelectPdfLineSet::select_fontsize(0.0, 100.0), PdfLine::new("Arial\n", 43.2, "rumi", (0.0, 0.0, 1.0, 1.0)); "font size")]
        #[test_case(SelectPdfLineSet::select_text("mi$"), PdfLine::new("Arial\n", 43.2, "rumi", (0.0, 0.0, 1.0, 1.0)); "text")]
        #[test_case(SelectPdfLineSet::select_area(0.0, 0.0, 2.0, 2.0), PdfLine::new("Arial\n", 43.2, "rumi", (0.0, 0.0, 1.0, 1.0)); "area")]
        fn element_in_leaf(set: SelectPdfLineSet, ele: PdfLine) {
            assert!(set.contains(&ele));
        }

        #[test_case(SelectPdfLineSet::select_font("fraktur sans-serif"), PdfLine::new("Arial\n", 43.2, "rumi", (0.0, 0.0, 1.0, 1.0)); "font")]
        #[test_case(SelectPdfLineSet::select_fontsize(30.0, 40.0), PdfLine::new("Arial\n", 43.2, "rumi", (0.0, 0.0, 1.0, 1.0)); "font size")]
        #[test_case(SelectPdfLineSet::select_text("^rum$"), PdfLine::new("Arial\n", 43.2, "rumi", (0.0, 0.0, 1.0, 1.0)); "text")]
        #[test_case(SelectPdfLineSet::select_area(0.1, 0.0, 2.0, 2.0), PdfLine::new("Arial\n", 43.2, "rumi", (0.0, 0.0, 1.0, 1.0)); "area")]
        fn element_not_in_leaf(set: SelectPdfLineSet, ele: PdfLine) {
            assert!(!set.contains(&ele));
        }

        #[test]
        fn area_variant_accepts_a_line_whose_bbox_is_strictly_inside() {
            let set = SelectPdfLineSet::select_area(0.0, 0.0, 100.0, 100.0);
            let line = PdfLine::new("Arial", 10.0, "inside", (10.0, 10.0, 20.0, 20.0));
            assert!(set.contains(&line));
        }

        #[test]
        fn area_variant_rejects_a_line_whose_bbox_only_partially_overlaps() {
            let set = SelectPdfLineSet::select_area(0.0, 0.0, 15.0, 15.0);
            let line = PdfLine::new("Arial", 10.0, "straddling", (10.0, 10.0, 20.0, 20.0));
            assert!(!set.contains(&line));
        }
    }

    /// A combinatorial stress test: for all sixteen combinations of the four criteria being present
    /// or absent, [`PdfLineSet::new`] must behave exactly like the hand-written intersection of the
    /// ones present. That catches both a missing branch and two swapped arguments in the
    /// sixteen-way match, which reading it cannot.
    mod pdf_line_set_new_combines_criteria_with_intersection {
        use super::*;

        fn sample_lines() -> Vec<PdfLine> {
            vec![
                PdfLine::new("Arial", 12.0, "Alpha", (0.0, 0.0, 10.0, 10.0)),
                PdfLine::new("Times", 8.0, "Beta", (20.0, 20.0, 30.0, 30.0)),
                PdfLine::new("Arial", 20.0, "Alpha", (5.0, 5.0, 15.0, 15.0)),
            ]
        }

        fn manual_intersection(
            font: Option<&str>,
            font_size: Option<(f32, f32)>,
            text: Option<&str>,
            area: Option<(f32, f32, f32, f32)>,
        ) -> PdfLineSet {
            let mut parts = Vec::new();
            if let Some(f) = font {
                parts.push(PdfLineSet::select_font(f));
            }
            if let Some((a, b)) = font_size {
                parts.push(PdfLineSet::select_fontsize(a, b));
            }
            if let Some(t) = text {
                parts.push(PdfLineSet::select_text(t));
            }
            if let Some((x0, y0, x1, y1)) = area {
                parts.push(PdfLineSet::select_area(x0, y0, x1, y1));
            }
            match parts.into_iter().reduce(|a, b| a & b) {
                Some(combined) => combined,
                None => PdfLineSet::select_fontsize(0.0, 1e6),
            }
        }

        #[test]
        fn agrees_with_manual_intersection_across_all_sixteen_presence_combinations() {
            let lines = sample_lines();
            let fonts: [Option<&str>; 2] = [None, Some("Arial")];
            let sizes: [Option<(f32, f32)>; 2] = [None, Some((10.0, 25.0))];
            let texts: [Option<&str>; 2] = [None, Some("^Alpha$")];
            let areas: [Option<(f32, f32, f32, f32)>; 2] = [None, Some((0.0, 0.0, 20.0, 20.0))];

            for &font in &fonts {
                for &size in &sizes {
                    for &text in &texts {
                        for &area in &areas {
                            let via_new = PdfLineSet::new(font, size, text, area);
                            let via_manual = manual_intersection(font, size, text, area);
                            for line in &lines {
                                assert_eq!(
                                    via_new.contains(line),
                                    via_manual.contains(line),
                                    "mismatch for font={font:?} size={size:?} text={text:?} area={area:?} line={line:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    mod pdf_line_set_from_sets_matches_new {
        use super::*;
        use crate::formats_utils::pdf_extract::select::pdf_line::area::Area;
        use crate::formats_utils::pdf_extract::select::pdf_line::font::FontSet;
        use crate::formats_utils::pdf_extract::select::pdf_line::font_size::FontSizeInterval;
        use crate::formats_utils::pdf_extract::select::pdf_line::text::TextSet;

        fn sample_lines() -> Vec<PdfLine> {
            vec![
                PdfLine::new("Arial", 12.0, "Alpha", (0.0, 0.0, 10.0, 10.0)),
                PdfLine::new("Times", 30.0, "Beta", (50.0, 50.0, 60.0, 60.0)),
            ]
        }

        #[test]
        fn agrees_with_new_when_only_font_is_present() {
            let lines = sample_lines();
            let a = PdfLineSet::from_sets(Some(FontSet::new("Arial")), None, None, None);
            let b = PdfLineSet::new(Some("Arial"), None, None, None);
            for line in &lines {
                assert_eq!(a.contains(line), b.contains(line));
            }
        }

        #[test]
        fn agrees_with_new_when_every_criterion_is_present() {
            let lines = sample_lines();
            let a = PdfLineSet::from_sets(
                Some(FontSet::new("Arial")),
                Some(FontSizeInterval::new(10.0, 15.0)),
                Some(TextSet::new("^Alpha$")),
                Some(Area::new(0.0, 0.0, 20.0, 20.0)),
            );
            let b = PdfLineSet::new(Some("Arial"), Some((10.0, 15.0)), Some("^Alpha$"), Some((0.0, 0.0, 20.0, 20.0)));
            for line in &lines {
                assert_eq!(a.contains(line), b.contains(line));
            }
        }

        #[test]
        fn agrees_with_new_when_nothing_is_present() {
            let lines = sample_lines();
            let a = PdfLineSet::from_sets(None, None, None, None);
            let b = PdfLineSet::new(None, None, None, None);
            for line in &lines {
                assert_eq!(a.contains(line), b.contains(line));
            }
        }
    }
}
