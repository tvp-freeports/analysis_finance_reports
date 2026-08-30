//! Selezioni relative ad altre selezioni (RelativePdfLineSet).
//!
//! Porting verbatim (`PLAN.md` §0/§12 D14) della meta' "bottom half" del vecchio
//! `freeports_core::formats_utils::pdf_extract::select::relative` — tutto cio' che menziona
//! `SelectPdfLineSet`/`PdfLineSet` (la parte generica `OptionallyRelative`/`RelativeInfo` e'
//! rilocata in `pdf_extract::relative`, R3 di `PLAN.md`, e importata qui).
//!
//! Contratto atteso dai test qui sotto (il test-writer non scrive codice di produzione):
//!
//! - `pub enum RelativeSelectPdfLineSet { Font(RelativeFontSet), FontSize(RelativeFontSizeInterval),
//!   Text(RelativeTextSet), Area(RelativeArea) }`, con costruttori
//!   `select_font_of/select_fontsize_of/select_text_of/select_area_of(target: PdfLineSelection)`,
//!   `area_from_movewindow(...)`, `area_from_bounds(...)`, e
//!   `impl RelativeInfo<SelectPdfLineSet> for RelativeSelectPdfLineSet` che dispaccia ogni
//!   variante alla `contextualize` della sua componente.
//! - `type LeafType = OptionallyRelative<SelectPdfLineSet, RelativeSelectPdfLineSet>;`
//! - `pub enum NodeRelativePdfLineSet { Leaf(LeafType), Branch(Box<Self>, SetOps, Box<Self>) }`
//!   con `impl RelativeInfo<PdfLineSet> for NodeRelativePdfLineSet`: `Leaf` si contestualizza in
//!   un `PdfLineSet::from_leaf`; `Branch` contestualizza entrambi i lati e applica `|`/`&`/`/` a
//!   seconda di `SetOps`.
//! - `pub struct RelativePdfLineSet(NodeRelativePdfLineSet)` con `BitOr`/`BitAnd`/`Div` che
//!   costruiscono un `Branch`, `from_leaf`, `ast()`, e costruttori
//!   `from_font/from_fontsize/from_text/from_area` che avvolgono un
//!   `OptionallyRelative<Set,RelativeSet>` nel `SelectPdfLineSet`/`RelativeSelectPdfLineSet`
//!   corrispondente.
//! - `pub type PdfLineSelection = OptionallyRelative<PdfLineSet, RelativePdfLineSet>;` con
//!   `from_font/from_fontsize/from_text/from_area` analoghi (producono `Absolute(PdfLineSet::..)`
//!   o `Relative(RelativePdfLineSet::from_..)` a seconda della variante in ingresso).
//! - `RelativeArea`: tre varianti, `Select(Box<PdfLineSelection>)`, `MoveWindow{ target, vec,
//!   width_mult, height_mult }`, `Bounds{ x0,y0,x1,y1: OptionallyRelative<f32,Box<PdfLineSelection>> }`.
//!   `contextualize`:
//!   - `Select`: unione delle aree (bbox) di tutte le righe selezionate, `Area::empty()` se
//!     nessuna riga corrisponde.
//!   - `MoveWindow`: prende la bbox della *prima* riga selezionata (l'ordine e' quello di
//!     `lines`, non ordinato), trasla/scala secondo `vec`/`width_mult`/`height_mult`;
//!     `Area::empty()` se nessuna riga corrisponde.
//!   - `Bounds`: ciascun lato assoluto e' preso cosi' com'e'; ciascun lato relativo prende un
//!     lato della bbox della *prima* riga selezionata dalla sua sotto-selezione (`x0`/`x1` la
//!     coordinata opposta lungo l'asse orizzontale, `y0`/`y1` lungo il verticale — vedi il
//!     riferimento per il dettaglio esatto), con fallback `0.0`/`10e6` se nessuna riga corrisponde.
//! - `RelativeFontSet`/`RelativeFontSizeInterval`/`RelativeTextSet`: un'unica variante
//!   `Select(Box<PdfLineSelection>)`; `contextualize` filtra le righe che soddisfano la
//!   selezione contestualizzata e unisce (`|`) il `FontSet`/`FontSizeInterval`(con
//!   `from_precision(fs, 1e-4)`)/`TextSet` (`^{testo}$`) derivato da ciascuna, con
//!   `empty()` se nessuna riga corrisponde.

use std::cmp::max;
use std::ops::{BitAnd, BitOr, Div};

use ordered_float::OrderedFloat;

use crate::commons::sets::{Container, SetOps};
use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
use crate::formats_utils::pdf_extract::relative::{OptionallyRelative, RelativeInfo};

use super::pdf_line::area::Area;
use super::pdf_line::font::FontSet;
use super::pdf_line::font_size::FontSizeInterval;
use super::pdf_line::text::TextSet;
use super::pdf_line::{PdfLineSet, SelectPdfLineSet};

type OptRel<V, R> = OptionallyRelative<V, R>;

#[derive(Clone)]
pub enum RelativeSelectPdfLineSet {
    Font(RelativeFontSet),
    FontSize(RelativeFontSizeInterval),
    Text(RelativeTextSet),
    Area(RelativeArea),
}

impl RelativeSelectPdfLineSet {
    pub fn select_font_of(target: PdfLineSelection) -> Self {
        Self::Font(RelativeFontSet::from_selection(target))
    }
    pub fn select_fontsize_of(target: PdfLineSelection) -> Self {
        Self::FontSize(RelativeFontSizeInterval::from_selection(target))
    }
    pub fn select_text_of(target: PdfLineSelection) -> Self {
        Self::Text(RelativeTextSet::from_selection(target))
    }
    pub fn select_area_of(target: PdfLineSelection) -> Self {
        Self::Area(RelativeArea::from_selection(target))
    }
    pub fn area_from_movewindow(target: PdfLineSelection, vec: (f32, f32), width_mult: f32, height_mult: f32) -> Self {
        Self::Area(RelativeArea::from_movewindow(target, vec, width_mult, height_mult))
    }
    pub fn area_from_bounds(
        x0: OptRel<f32, PdfLineSelection>,
        y0: OptRel<f32, PdfLineSelection>,
        x1: OptRel<f32, PdfLineSelection>,
        y1: OptRel<f32, PdfLineSelection>,
    ) -> Self {
        Self::Area(RelativeArea::from_bounds(x0, y0, x1, y1))
    }
}

impl RelativeInfo<SelectPdfLineSet> for RelativeSelectPdfLineSet {
    fn contextualize(self, lines: &[PdfLine]) -> SelectPdfLineSet {
        use SelectPdfLineSet::*;
        match self {
            Self::Font(rf) => Font(rf.contextualize(lines)),
            Self::FontSize(rfs) => FontSize(rfs.contextualize(lines)),
            Self::Text(rt) => Text(rt.contextualize(lines)),
            Self::Area(ra) => Area(ra.contextualize(lines)),
        }
    }
}

type LeafType = OptRel<SelectPdfLineSet, RelativeSelectPdfLineSet>;

pub enum NodeRelativePdfLineSet {
    Leaf(LeafType),
    Branch(Box<NodeRelativePdfLineSet>, SetOps, Box<NodeRelativePdfLineSet>),
}

impl Clone for NodeRelativePdfLineSet {
    fn clone(&self) -> Self {
        match self {
            Self::Leaf(a) => Self::Leaf(a.clone()),
            Self::Branch(a, ops, b) => Self::Branch(a.clone(), *ops, b.clone()),
        }
    }
}

impl RelativeInfo<PdfLineSet> for NodeRelativePdfLineSet {
    fn contextualize(self, lines: &[PdfLine]) -> PdfLineSet {
        use SetOps::*;
        match self {
            Self::Leaf(leaf) => PdfLineSet::from_leaf(leaf.contextualize(lines)),
            Self::Branch(box_x, op, box_y) => {
                let a = box_x.contextualize(lines);
                let b = box_y.contextualize(lines);
                match op {
                    Union => a | b,
                    Inter => a & b,
                    Sub => a / b,
                }
            }
        }
    }
}

pub struct RelativePdfLineSet(NodeRelativePdfLineSet);

impl Clone for RelativePdfLineSet {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl RelativeInfo<PdfLineSet> for RelativePdfLineSet {
    fn contextualize(self, lines: &[PdfLine]) -> PdfLineSet {
        self.0.contextualize(lines)
    }
}

impl BitOr<Self> for RelativePdfLineSet {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(NodeRelativePdfLineSet::Branch(Box::new(self.0), SetOps::Union, Box::new(rhs.0)))
    }
}
impl BitAnd<Self> for RelativePdfLineSet {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(NodeRelativePdfLineSet::Branch(Box::new(self.0), SetOps::Inter, Box::new(rhs.0)))
    }
}
impl Div<Self> for RelativePdfLineSet {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self(NodeRelativePdfLineSet::Branch(Box::new(self.0), SetOps::Sub, Box::new(rhs.0)))
    }
}

impl RelativePdfLineSet {
    pub fn from_font(f: OptRel<FontSet, RelativeFontSet>) -> Self {
        use OptionallyRelative::*;
        use RelativeSelectPdfLineSet as R;
        use SelectPdfLineSet::*;
        match f {
            Absolute(af) => Self::from_leaf(Absolute(Font(af))),
            Relative(rf) => Self::from_leaf(Relative(R::Font(rf))),
        }
    }
    pub fn from_fontsize(fs: OptRel<FontSizeInterval, RelativeFontSizeInterval>) -> Self {
        use OptionallyRelative::*;
        use RelativeSelectPdfLineSet as R;
        use SelectPdfLineSet::*;
        match fs {
            Absolute(afs) => Self::from_leaf(Absolute(FontSize(afs))),
            Relative(rfs) => Self::from_leaf(Relative(R::FontSize(rfs))),
        }
    }
    pub fn from_text(t: OptRel<TextSet, RelativeTextSet>) -> Self {
        use OptionallyRelative::*;
        use RelativeSelectPdfLineSet as R;
        use SelectPdfLineSet::*;
        match t {
            Absolute(at) => Self::from_leaf(Absolute(Text(at))),
            Relative(rt) => Self::from_leaf(Relative(R::Text(rt))),
        }
    }
    pub fn from_area(a: OptRel<Area, RelativeArea>) -> Self {
        use OptionallyRelative::*;
        use RelativeSelectPdfLineSet as R;
        use SelectPdfLineSet::*;
        match a {
            Absolute(aa) => Self::from_leaf(Absolute(Area(aa))),
            Relative(ra) => Self::from_leaf(Relative(R::Area(ra))),
        }
    }

    pub fn from_leaf(leaf: LeafType) -> Self {
        Self(NodeRelativePdfLineSet::Leaf(leaf))
    }
    pub fn ast(&self) -> &NodeRelativePdfLineSet {
        &self.0
    }
}

pub type PdfLineSelection = OptRel<PdfLineSet, RelativePdfLineSet>;

impl PdfLineSelection {
    pub fn from_font(f: OptRel<FontSet, RelativeFontSet>) -> Self {
        use OptionallyRelative::*;
        match f {
            Absolute(af) => Absolute(PdfLineSet::font(af)),
            Relative(rf) => Relative(RelativePdfLineSet::from_font(Relative(rf))),
        }
    }
    pub fn from_fontsize(fs: OptRel<FontSizeInterval, RelativeFontSizeInterval>) -> Self {
        use OptionallyRelative::*;
        match fs {
            Absolute(afs) => Absolute(PdfLineSet::fontsize(afs)),
            Relative(rfs) => Relative(RelativePdfLineSet::from_fontsize(Relative(rfs))),
        }
    }
    pub fn from_text(t: OptRel<TextSet, RelativeTextSet>) -> Self {
        use OptionallyRelative::*;
        match t {
            Absolute(at) => Absolute(PdfLineSet::text(at)),
            Relative(rt) => Relative(RelativePdfLineSet::from_text(Relative(rt))),
        }
    }
    pub fn from_area(a: OptRel<Area, RelativeArea>) -> Self {
        use OptionallyRelative::*;
        match a {
            Absolute(aa) => Absolute(PdfLineSet::area(aa)),
            Relative(ra) => Relative(RelativePdfLineSet::from_area(Relative(ra))),
        }
    }
}

#[derive(Clone)]
pub enum RelativeArea {
    Select(Box<PdfLineSelection>),
    MoveWindow { target: Box<PdfLineSelection>, vec: (f32, f32), width_mult: f32, height_mult: f32 },
    Bounds {
        x0: OptRel<f32, Box<PdfLineSelection>>,
        y0: OptRel<f32, Box<PdfLineSelection>>,
        x1: OptRel<f32, Box<PdfLineSelection>>,
        y1: OptRel<f32, Box<PdfLineSelection>>,
    },
}
#[derive(Clone)]
pub enum RelativeFontSet {
    Select(Box<PdfLineSelection>),
}
impl RelativeFontSet {
    fn from_selection(select: PdfLineSelection) -> Self {
        Self::Select(Box::new(select))
    }
}

#[derive(Clone)]
pub enum RelativeFontSizeInterval {
    Select(Box<PdfLineSelection>),
}
impl RelativeFontSizeInterval {
    fn from_selection(select: PdfLineSelection) -> Self {
        Self::Select(Box::new(select))
    }
}
#[derive(Clone)]
pub enum RelativeTextSet {
    Select(Box<PdfLineSelection>),
}
impl RelativeTextSet {
    fn from_selection(select: PdfLineSelection) -> Self {
        Self::Select(Box::new(select))
    }
}

impl RelativeInfo<FontSet> for RelativeFontSet {
    fn contextualize(self, lines: &[PdfLine]) -> FontSet {
        let Self::Select(r) = self;
        let line_set = r.contextualize(lines);
        let matched: Vec<&PdfLine> = lines.iter().filter(|l| line_set.contains(l)).collect();
        if matched.is_empty() {
            tracing::trace!("RelativeFontSet: target selection matched no line, resolved to an empty set");
        }
        matched
            .into_iter()
            .map(|l| FontSet::from_atom(l.font().clone()))
            .reduce(|a, b| a | b)
            .unwrap_or(FontSet::empty())
    }
}

impl RelativeInfo<TextSet> for RelativeTextSet {
    fn contextualize(self, lines: &[PdfLine]) -> TextSet {
        let Self::Select(r) = self;
        let line_set = r.contextualize(lines);
        let matched: Vec<&PdfLine> = lines.iter().filter(|l| line_set.contains(l)).collect();
        if matched.is_empty() {
            tracing::trace!("RelativeTextSet: target selection matched no line, resolved to an empty set");
        }
        matched
            .into_iter()
            .map(|l| TextSet::new(&format!("^{}$", l.text())))
            .reduce(|a, b| a | b)
            .unwrap_or(TextSet::empty())
    }
}

impl RelativeInfo<FontSizeInterval> for RelativeFontSizeInterval {
    fn contextualize(self, lines: &[PdfLine]) -> FontSizeInterval {
        let Self::Select(r) = self;
        let line_set = r.contextualize(lines);
        let matched: Vec<&PdfLine> = lines.iter().filter(|l| line_set.contains(l)).collect();
        if matched.is_empty() {
            tracing::trace!("RelativeFontSizeInterval: target selection matched no line, resolved to an empty set");
        }
        matched
            .into_iter()
            .map(|l| {
                let fs = *l.font_size();
                let a = max(OrderedFloat(0.0), OrderedFloat(fs - 1e-4)).into_inner();
                FontSizeInterval::new(a, fs + 1e-4)
            })
            .reduce(|a, b| a | b)
            .unwrap_or(FontSizeInterval::empty())
    }
}

impl RelativeArea {
    fn from_selection(select: PdfLineSelection) -> Self {
        Self::Select(Box::new(select))
    }
    fn from_movewindow(target: PdfLineSelection, vec: (f32, f32), width_mult: f32, height_mult: f32) -> Self {
        Self::MoveWindow { target: Box::new(target), vec, width_mult, height_mult }
    }
    fn from_bounds(
        x0: OptRel<f32, PdfLineSelection>,
        y0: OptRel<f32, PdfLineSelection>,
        x1: OptRel<f32, PdfLineSelection>,
        y1: OptRel<f32, PdfLineSelection>,
    ) -> Self {
        fn map_bound(b: OptRel<f32, PdfLineSelection>) -> OptRel<f32, Box<PdfLineSelection>> {
            use OptionallyRelative::*;
            match b {
                Absolute(x) => Absolute(x),
                Relative(select) => Relative(Box::new(select)),
            }
        }
        Self::Bounds { x0: map_bound(x0), y0: map_bound(y0), x1: map_bound(x1), y1: map_bound(y1) }
    }
    fn contextualize_movewindow(lines: &[PdfLine], target: PdfLineSelection, vec: (f32, f32), width_mult: f32, height_mult: f32) -> Area {
        let line_set = target.contextualize(lines);
        let (x, y) = vec;
        let anchor = lines.iter().filter(|l| line_set.contains(l)).map(|l| l.bbox().as_tuple()).next();
        if anchor.is_none() {
            tracing::trace!("RelativeArea::MoveWindow: target selection matched no line, resolved to an empty area");
        }
        anchor
            .map(|(x0, y0, x1, y1)| {
                let w = x1 - x0;
                let h = y1 - y0;
                Area::new(x0 + x * w, y0 + y * h, x0 + (width_mult + x) * w, y0 + (height_mult + y) * h)
            })
            .unwrap_or(Area::empty())
    }
    fn contextualize_bounds(
        lines: &[PdfLine],
        x0: OptRel<f32, Box<PdfLineSelection>>,
        y0: OptRel<f32, Box<PdfLineSelection>>,
        x1: OptRel<f32, Box<PdfLineSelection>>,
        y1: OptRel<f32, Box<PdfLineSelection>>,
    ) -> Area {
        let left = match x0 {
            OptionallyRelative::Absolute(x) => x,
            OptionallyRelative::Relative(rls) => {
                let line_set = rls.contextualize(lines);
                match lines.iter().filter(|l| line_set.contains(l)).map(|l| l.bbox().as_tuple()).next() {
                    Some((_, _, x1, _)) => x1,
                    None => {
                        tracing::trace!(side = "x0", "RelativeArea::Bounds: side selection matched no line, using fallback bound");
                        0.0
                    }
                }
            }
        };
        let right = match x1 {
            OptionallyRelative::Absolute(x) => x,
            OptionallyRelative::Relative(rls) => {
                let line_set = rls.contextualize(lines);
                match lines.iter().filter(|l| line_set.contains(l)).map(|l| l.bbox().as_tuple()).next() {
                    Some((x0, _, _, _)) => x0,
                    None => {
                        tracing::trace!(side = "x1", "RelativeArea::Bounds: side selection matched no line, using fallback bound");
                        10e+6
                    }
                }
            }
        };
        let up = match y0 {
            OptionallyRelative::Absolute(y) => y,
            OptionallyRelative::Relative(rls) => {
                let line_set = rls.contextualize(lines);
                match lines.iter().filter(|l| line_set.contains(l)).map(|l| l.bbox().as_tuple()).next() {
                    Some((_, _, _, y1)) => y1,
                    None => {
                        tracing::trace!(side = "y0", "RelativeArea::Bounds: side selection matched no line, using fallback bound");
                        0.0
                    }
                }
            }
        };
        let bottom = match y1 {
            OptionallyRelative::Absolute(y) => y,
            OptionallyRelative::Relative(rls) => {
                let line_set = rls.contextualize(lines);
                match lines.iter().filter(|l| line_set.contains(l)).map(|l| l.bbox().as_tuple()).next() {
                    Some((_, y0, _, _)) => y0,
                    None => {
                        tracing::trace!(side = "y1", "RelativeArea::Bounds: side selection matched no line, using fallback bound");
                        10e+6
                    }
                }
            }
        };
        Area::new(left, up, right, bottom)
    }
    fn contextualize_selection(lines: &[PdfLine], set: PdfLineSelection) -> Area {
        let line_set = set.contextualize(lines);
        let matched: Vec<&PdfLine> = lines.iter().filter(|l| line_set.contains(l)).collect();
        if matched.is_empty() {
            tracing::trace!("RelativeArea::Select: target selection matched no line, resolved to an empty area");
        }
        matched.into_iter().map(|l| Area::from_atom(*l.bbox())).reduce(|a, b| a | b).unwrap_or(Area::empty())
    }
}

impl RelativeInfo<Area> for RelativeArea {
    fn contextualize(self, lines: &[PdfLine]) -> Area {
        match self {
            Self::Select(r) => Self::contextualize_selection(lines, *r),
            Self::MoveWindow { target, vec, width_mult, height_mult } => Self::contextualize_movewindow(lines, *target, vec, width_mult, height_mult),
            Self::Bounds { x0, y0, x1, y1 } => Self::contextualize_bounds(lines, x0, y0, x1, y1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::pdf_extract::relative::OptionallyRelative::*;
    use crate::formats_utils::pdf_extract::select::pdf_line::area::Area;
    use crate::formats_utils::pdf_extract::select::pdf_line::font::FontSet;
    use crate::formats_utils::pdf_extract::select::pdf_line::font_size::FontSizeInterval;
    use crate::formats_utils::pdf_extract::select::pdf_line::text::TextSet;
    use std::sync::LazyLock;

    /// Stessa pagina di prova del riferimento: un titolo, cinque righe "simili" a font/corpo
    /// diversi, un'intestazione di sezione, e un'altra manciata di righe "simili" sotto — usata
    /// per esercitare selezioni relative combinatorie (`PLAN.md` §10).
    static LINES: LazyLock<Vec<PdfLine>> = LazyLock::new(|| {
        vec![
            PdfLine::new("Arial", 45.0, "TITLE OF THE PAGE", (35.0, 1.0, 65.0, 5.0)),
            PdfLine::new("A", 1.5, "text", (10.0, 10.0, 15.0, 11.0)),
            PdfLine::new("A", 1.7, "with", (10.0, 11.0, 15.0, 12.0)),
            PdfLine::new("C", 1.3, "similar", (10.0, 12.0, 15.0, 13.0)),
            PdfLine::new("D", 1.1, "font", (10.0, 13.0, 15.0, 14.0)),
            PdfLine::new("E", 1.13, "size", (10.0, 14.0, 15.0, 15.0)),
            PdfLine::new("Fracktur", 40.0, "SECTION 2", (35.0, 21.0, 65.0, 25.0)),
            PdfLine::new("A", 14.5, "same", (10.0, 30.0, 15.0, 31.0)),
            PdfLine::new("A", 188.7, "font", (10.0, 31.0, 15.0, 32.0)),
            PdfLine::new("B", 0.3, "-----", (10.0, 32.0, 15.0, 33.0)),
            PdfLine::new("DDD", 14.1, "font", (10.0, 33.0, 15.0, 34.0)),
            PdfLine::new("EEE", 14.13, "size", (10.0, 34.0, 15.0, 35.0)),
        ]
    });

    mod contextualize_relative_font_set {
        use super::*;
        use test_case::test_case;

        #[test_case(
            RelativeFontSet::from_selection(PdfLineSelection::from_font(Absolute(FontSet::new("A")))),
            FontSet::new("A");
            "selecting by font gathers the fonts of the matching lines"
        )]
        #[test_case(
            RelativeFontSet::from_selection(PdfLineSelection::from_fontsize(Absolute(FontSizeInterval::new(1.09, 1.8)))),
            FontSet::new("A") | FontSet::new("C") | FontSet::new("D") | FontSet::new("E");
            "selecting by font size gathers the fonts of the matching lines"
        )]
        #[test_case(
            RelativeFontSet::from_selection(PdfLineSelection::from_text(Absolute(TextSet::new("fon")))),
            FontSet::new("A") | FontSet::new("D") | FontSet::new("DDD");
            "selecting by text gathers the fonts of the matching lines"
        )]
        #[test_case(
            RelativeFontSet::from_selection(PdfLineSelection::from_area(Absolute(Area::new(0.0, 30.0, 20.0, 34.0)))),
            FontSet::new("A") | FontSet::new("B") | FontSet::new("DDD");
            "selecting by area gathers the fonts of the matching lines"
        )]
        #[test_case(
            RelativeFontSet::from_selection(PdfLineSelection::from_area(Absolute(Area::new(800.0, 30.0, 2000.0, 340.0)))),
            FontSet::empty();
            "no matching line yields an empty font set"
        )]
        fn matches_expected_fontset(rf: RelativeFontSet, expected: FontSet) {
            let f = rf.contextualize(&LINES);
            assert_eq!(f.atoms(), expected.atoms());
        }
    }

    mod contextualize_relative_font_size_interval {
        use super::*;
        use test_case::test_case;

        #[test_case(
            RelativeFontSizeInterval::from_selection(PdfLineSelection::from_font(Absolute(FontSet::new("A")))),
            FontSizeInterval::from_precision(1.5, 1e-4)
                | FontSizeInterval::from_precision(1.7, 1e-4)
                | FontSizeInterval::from_precision(14.5, 1e-4)
                | FontSizeInterval::from_precision(188.7, 1e-4);
            "selecting by font gathers the font sizes of the matching lines"
        )]
        #[test_case(
            RelativeFontSizeInterval::from_selection(PdfLineSelection::from_fontsize(Absolute(FontSizeInterval::new(1.14, 1.8)))),
            FontSizeInterval::from_precision(1.3, 1e-4)
                | FontSizeInterval::from_precision(1.5, 1e-4)
                | FontSizeInterval::from_precision(1.7, 1e-4);
            "selecting by font size gathers the font sizes of the matching lines"
        )]
        #[test_case(
            RelativeFontSizeInterval::from_selection(PdfLineSelection::from_text(Absolute(TextSet::new("size")))),
            FontSizeInterval::from_precision(1.13, 1e-4) | FontSizeInterval::from_precision(14.13, 1e-4);
            "selecting by text gathers the font sizes of the matching lines"
        )]
        #[test_case(
            RelativeFontSizeInterval::from_selection(PdfLineSelection::from_area(Absolute(Area::new(0.0, 30.0, 20.0, 33.0)))),
            FontSizeInterval::from_precision(14.5, 1e-4)
                | FontSizeInterval::from_precision(188.7, 1e-4)
                | FontSizeInterval::from_precision(0.3, 1e-4);
            "selecting by area gathers the font sizes of the matching lines"
        )]
        #[test_case(
            RelativeFontSizeInterval::from_selection(PdfLineSelection::from_area(Absolute(Area::new(800.0, 30.0, 2000.0, 340.0)))),
            FontSizeInterval::empty();
            "no matching line yields an empty font size interval"
        )]
        fn matches_expected_fontsizeinterval(rfs: RelativeFontSizeInterval, expected: FontSizeInterval) {
            let fs = rfs.contextualize(&LINES);
            assert_eq!(fs.atoms(), expected.atoms());
        }
    }

    mod contextualize_relative_text_set {
        use super::*;
        use test_case::test_case;

        #[test_case(
            RelativeTextSet::from_selection(PdfLineSelection::from_font(Absolute(FontSet::new("A")))),
            TextSet::new("^text$") | TextSet::new("^with$") | TextSet::new("^same$") | TextSet::new("^font$");
            "selecting by font gathers the exact text of the matching lines"
        )]
        #[test_case(
            RelativeTextSet::from_selection(PdfLineSelection::from_fontsize(Absolute(FontSizeInterval::new(1.14, 1.8)))),
            TextSet::new("^text$") | TextSet::new("^with$") | TextSet::new("^similar$");
            "selecting by font size gathers the exact text of the matching lines"
        )]
        #[test_case(
            RelativeTextSet::from_selection(PdfLineSelection::from_text(Absolute(TextSet::new("i")))),
            // Nota: "size" compare come testo esatto di *due* righe distinte (indice 5 e 11 in
            // `LINES`), non e' un refuso: la struttura del risultato (fold via `|` sulle righe
            // filtrate, nell'ordine di `LINES`) dipende da quante volte compare, non solo da
            // quali fonts/testi compaiono — vedi il commento sul riferimento originale, che
            // scrive lo stesso duplicato in questo identico test.
            TextSet::new("^with$") | TextSet::new("^similar$") | TextSet::new("^size$") | TextSet::new("^size$");
            "selecting by text gathers the exact text of the matching lines"
        )]
        #[test_case(
            RelativeTextSet::from_selection(PdfLineSelection::from_area(Absolute(Area::new(0.0, 30.0, 20.0, 33.0)))),
            TextSet::new("^same$") | TextSet::new("^font$") | TextSet::new("^-----$");
            "selecting by area gathers the exact text of the matching lines"
        )]
        #[test_case(
            RelativeTextSet::from_selection(PdfLineSelection::from_area(Absolute(Area::new(800.0, 30.0, 2000.0, 340.0)))),
            TextSet::empty();
            "no matching line yields an empty text set"
        )]
        fn matches_expected_textset(rt: RelativeTextSet, expected: TextSet) {
            let t = rt.contextualize(&LINES);
            assert_eq!(t.ast(), expected.ast());
        }
    }

    mod contextualize_relative_area {
        use super::*;
        use test_case::test_case;

        #[test_case(
            RelativeArea::from_selection(PdfLineSelection::from_font(Absolute(FontSet::new("A")))),
            Area::new(10.0, 10.0, 15.0, 11.0)
                | Area::new(10.0, 11.0, 15.0, 12.0)
                | Area::new(10.0, 30.0, 15.0, 31.0)
                | Area::new(10.0, 31.0, 15.0, 32.0);
            "select: union of the bboxes of the matching lines"
        )]
        #[test_case(
            RelativeArea::from_selection(PdfLineSelection::from_fontsize(Absolute(FontSizeInterval::new(1.14, 1.8)))),
            Area::new(10.0, 12.0, 15.0, 13.0) | Area::new(10.0, 10.0, 15.0, 11.0) | Area::new(10.0, 11.0, 15.0, 12.0);
            "select: union of bboxes selected by font size"
        )]
        #[test_case(
            RelativeArea::from_selection(PdfLineSelection::from_text(Absolute(TextSet::new("size")))),
            Area::new(10.0, 14.0, 15.0, 15.0) | Area::new(10.0, 34.0, 15.0, 35.0);
            "select: union of bboxes selected by text"
        )]
        #[test_case(
            RelativeArea::from_selection(PdfLineSelection::from_area(Absolute(Area::new(0.0, 30.0, 20.0, 33.0)))),
            Area::new(10.0, 30.0, 15.0, 31.0) | Area::new(10.0, 31.0, 15.0, 32.0) | Area::new(10.0, 32.0, 15.0, 33.0);
            "select: union of bboxes selected by area"
        )]
        #[test_case(
            RelativeArea::MoveWindow {
                target: Box::new(PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2")))),
                vec: (1.0, -0.6),
                width_mult: 1.5,
                height_mult: 0.5,
            },
            Area::new(65.0, 18.6, 110.0, 20.6);
            "move window: translates and scales the matched line's bbox"
        )]
        #[test_case(
            RelativeArea::Bounds {
                x0: Relative(Box::new(PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2"))))),
                y0: Absolute(60.0),
                x1: Absolute(70.0),
                y1: Absolute(300.0),
            },
            Area::new(65.0, 60.0, 70.0, 300.0);
            "bounds: relative left edge taken from the matched line's right edge"
        )]
        #[test_case(
            RelativeArea::Bounds {
                x0: Absolute(10.0),
                y0: Absolute(60.0),
                x1: Relative(Box::new(PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2"))))),
                y1: Absolute(300.0),
            },
            Area::new(10.0, 60.0, 35.0, 300.0);
            "bounds: relative right edge taken from the matched line's left edge"
        )]
        #[test_case(
            RelativeArea::Bounds {
                x0: Absolute(10.0),
                y0: Relative(Box::new(PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2"))))),
                x1: Absolute(60.0),
                y1: Absolute(300.0),
            },
            Area::new(10.0, 25.0, 60.0, 300.0);
            "bounds: relative top edge taken from the matched line's bottom edge"
        )]
        #[test_case(
            RelativeArea::Bounds {
                x0: Absolute(10.0),
                y0: Absolute(3.0),
                x1: Absolute(60.0),
                y1: Relative(Box::new(PdfLineSelection::from_text(Absolute(TextSet::new("SECTION 2"))))),
            },
            Area::new(10.0, 3.0, 60.0, 21.0);
            "bounds: relative bottom edge taken from the matched line's top edge"
        )]
        #[test_case(
            RelativeArea::from_selection(PdfLineSelection::from_area(Absolute(Area::new(800.0, 30.0, 2000.0, 340.0)))),
            Area::empty();
            "no matching line yields an empty area"
        )]
        fn matches_expected_area(ra: RelativeArea, expected: Area) {
            let a = ra.contextualize(&LINES);
            assert_eq!(a.atoms(), expected.atoms());
        }
    }

    mod relative_pdf_line_set_algebra {
        use super::*;
        use crate::commons::sets::Container;

        #[test]
        fn bitor_of_two_absolute_leaves_matches_either() {
            let combined = RelativePdfLineSet::from_font(Absolute(FontSet::new("A")))
                | RelativePdfLineSet::from_font(Absolute(FontSet::new("B")));
            let set = combined.contextualize(&LINES);
            assert!(set.contains(&PdfLine::new("A", 1.5, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(set.contains(&PdfLine::new("B", 1.5, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!set.contains(&PdfLine::new("C", 1.5, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn bitand_of_two_absolute_leaves_matches_both() {
            let combined = RelativePdfLineSet::from_font(Absolute(FontSet::new("A")))
                & RelativePdfLineSet::from_fontsize(Absolute(FontSizeInterval::new(0.0, 5.0)));
            let set = combined.contextualize(&LINES);
            assert!(set.contains(&PdfLine::new("A", 1.5, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!set.contains(&PdfLine::new("A", 50.0, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn div_of_two_absolute_leaves_matches_difference() {
            let combined = RelativePdfLineSet::from_font(Absolute(FontSet::new("A")))
                / RelativePdfLineSet::from_fontsize(Absolute(FontSizeInterval::new(0.0, 5.0)));
            let set = combined.contextualize(&LINES);
            assert!(set.contains(&PdfLine::new("A", 50.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!set.contains(&PdfLine::new("A", 1.5, "x", (0.0, 0.0, 1.0, 1.0))));
        }
    }
}
