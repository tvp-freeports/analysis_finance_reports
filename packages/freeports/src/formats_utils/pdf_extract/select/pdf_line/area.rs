//! Selezione per area geometrica.
//!
//! Porting verbatim (`PLAN.md` §0/§12 D14) di
//! `freeports_core::formats_utils::pdf_extract::select::pdf_line::area`. `Rectangle` e' gia'
//! definito in `commons::geometry` (M1): questo modulo vi implementa sopra
//! `Container`/`Overlappable`/`AtomOperations`/`AtomAlgebra`, per lo stesso motivo di R4 spiegato
//! in `font_size.rs`.
//!
//! **Decisione R4 (`PLAN.md`) sul campo `area` di `PdfLine`**: nel riferimento, `PdfLine` cache-a
//! un campo `area: Area` costruito con `Area::from_atom(bbox)` — puro wrapping di `bbox` in un
//! `DisjointAtomsSet` a un solo atomo, nessuna normalizzazione reale (a differenza di
//! `Font::new`, che *fa* lavoro: accenti, spazi, punteggiatura). Il nuovo `PdfLine`
//! (`pdf_extract::pdf_line`) non ha quindi un campo `area`: questo modulo aggiunge invece un
//! metodo `PdfLine::area(&self) -> Area` che lo deriva on demand da `bbox`. E' un `impl PdfLine`
//! scritto qui (non in `pdf_line.rs`) apposta: il tipo `Area` e' un tipo di selezione, e
//! `pdf_line.rs` (dati puri) non deve dipendere da `select` — l'`impl` puo' comunque stare qui
//! perche' in Rust un blocco `impl` non deve stare nel file che definisce il tipo, solo nello
//! stesso crate.
//!
//! Contratto atteso dai test qui sotto (il test-writer non scrive codice di produzione):
//!
//! - `impl Container for Rectangle { type Elem = (f32,f32); ... }`: un punto e' contenuto se
//!   cade dentro i quattro lati, estremi inclusi.
//! - `impl Overlappable<Self> for Rectangle`: le cinque relazioni standard su rettangoli
//!   assi-allineati.
//! - `impl AtomOperations for Rectangle`: `subtract_overlapping`/`subtract_subset` possono
//!   produrre da uno a quattro rettangoli a seconda di quanti lati coincidono/quanto i due
//!   rettangoli si overlappano (vedi `RectOverlapping` nel riferimento, tipo privato di
//!   classificazione); `intersect_overlapping` produce sempre un solo rettangolo.
//! - `impl AtomAlgebra for Rectangle {}`.
//! - `pub type Area = DisjointAtomsSet<Rectangle,(f32,f32)>;` con `Area::new(x0,y0,x1,y1) -> Self`
//!   (= `Self::from_atom(Rectangle::new(x0,y0,x1,y1))`).
//! - `impl PdfLine { pub(crate) fn area(&self) -> Area }` (vedi sopra): equivalente a
//!   `Area::from_atom(*self.bbox())`.

use crate::commons::geometry::Rectangle;
use crate::commons::sets::indipendent_atoms::{AtomAlgebra, AtomOperations, CompoundAtomOperationRes, DisjointAtomsSet};
use crate::commons::sets::{Container, Overlappable, SetRelation};
use crate::formats_utils::pdf_extract::pdf_line::PdfLine;

impl Container for Rectangle {
    type Elem = (f32, f32);
    fn contains(&self, point: &(f32, f32)) -> bool {
        let (x0, y0, x1, y1) = self.as_tuple();
        (x0 <= point.0 && point.0 <= x1) && (y0 <= point.1 && point.1 <= y1)
    }
}

impl Overlappable<Self> for Rectangle {
    fn set_relation(&self, other: &Self) -> SetRelation {
        use SetRelation::*;
        let (x0, y0, x1, y1) = self.as_tuple();
        let (a0, b0, a1, b1) = other.as_tuple();
        if (x0 >= a1 || x1 <= a0) || (y0 >= b1 || y1 <= b0) {
            Disjoint
        } else if (x0, y0, x1, y1) == (a0, b0, a1, b1) {
            Equal
        } else if a0 <= x0 && x1 <= a1 && b0 <= y0 && y1 <= b1 {
            Subset
        } else if x0 <= a0 && a1 <= x1 && y0 <= b0 && b1 <= y1 {
            Superset
        } else {
            Overlapping
        }
    }
}

#[derive(PartialEq, Debug)]
enum RectOverlapping {
    SmallerLeft,
    SmallerRight,
    SmallerTop,
    SmallerBottom,
    BiggerLeft,
    BiggerRight,
    BiggerTop,
    BiggerBottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Vertical,
    Horizontal,
}

impl Rectangle {
    fn type_overlap(&self, other: &Self) -> RectOverlapping {
        use RectOverlapping::*;
        let (x0, y0, x1, y1) = self.as_tuple();
        let (a0, b0, a1, b1) = other.as_tuple();
        if x0 < a0 {
            // BiggerRight SmallerRight TopRight BottomRight SmallerTop SmallerBottom Vertical
            if x1 <= a1 {
                // BiggerRight SmallerRight TopRight BottomRight
                if y0 < b0 {
                    // SmallerRight BottomRight
                    if b1 < y1 { SmallerRight } else { BottomRight }
                } else {
                    // BiggerRight TopRight
                    if b1 < y1 { TopRight } else { BiggerRight }
                }
            } else {
                // SmallerTop SmallerBottom Vertical
                if b0 <= y0 {
                    // SmallerTop Vertical
                    if b1 < y1 { SmallerTop } else { Vertical }
                } else {
                    SmallerBottom
                }
            }
        } else {
            // BiggerLeft SmallerLeft TopLeft BottomLeft BiggerTop BiggerBottom Horizontal
            if a1 < x1 {
                // BiggerLeft SmallerLeft TopLeft BottomLeft
                if y0 < b0 {
                    // SmallerLeft BottomLeft
                    if b1 < y1 { SmallerLeft } else { BottomLeft }
                } else {
                    // BiggerLeft TopLeft
                    if b1 < y1 { TopLeft } else { BiggerLeft }
                }
            } else {
                // BiggerTop BiggerBottom Horizontal
                if b1 < y1 {
                    // BiggerTop Horizontal
                    if y0 < b0 { Horizontal } else { BiggerTop }
                } else {
                    BiggerBottom
                }
            }
        }
    }
    fn subtract_as_overlap_type(&self, other: &Self, ovrlt: RectOverlapping) -> SubtractOverlappingRectanglesRes {
        use RectOverlapping::*;
        use SubtractOverlappingRectanglesRes::*;
        let (x0, y0, x1, y1) = self.as_tuple();
        let (a0, b0, a1, b1) = other.as_tuple();
        match ovrlt {
            SmallerLeft => Three(Self::new(x0, y0, x1, b0), Self::new(a1, b0, x1, y1), Self::new(x0, b1, a1, y1)),
            SmallerRight => Three(Self::new(x0, b1, x1, y1), Self::new(x0, y0, a0, b1), Self::new(a0, y0, x1, b0)),
            SmallerTop => Three(Self::new(a1, y0, x1, y1), Self::new(x0, b1, a1, y1), Self::new(x0, y0, a0, b1)),
            SmallerBottom => Three(Self::new(x0, y0, a0, y1), Self::new(a0, y0, x1, b0), Self::new(a1, b0, x1, y1)),
            BiggerLeft => One(Self::new(a1, y0, x1, y1)),
            BiggerRight => One(Self::new(x0, y0, a0, y1)),
            BiggerTop => One(Self::new(x0, b1, x1, y1)),
            BiggerBottom => One(Self::new(x0, y0, x1, b0)),
            TopLeft => Two(Self::new(a1, y0, x1, y1), Self::new(x0, b1, a1, y1)),
            TopRight => Two(Self::new(x0, b1, x1, y1), Self::new(x0, y0, a0, b1)),
            BottomRight => Two(Self::new(x0, y0, a0, y1), Self::new(a0, y0, x1, b0)),
            BottomLeft => Two(Self::new(x0, y0, x1, b0), Self::new(a1, b0, x1, y1)),
            Vertical => Two(Self::new(x0, y0, a0, y1), Self::new(a1, y0, x1, y1)),
            Horizontal => Two(Self::new(x0, y0, x1, b0), Self::new(x0, b1, x1, y1)),
        }
    }
}

pub enum SubtractOverlappingRectanglesRes {
    One(Rectangle),
    Two(Rectangle, Rectangle),
    Three(Rectangle, Rectangle, Rectangle),
}
pub enum SubtractSubsetRectanglesRes {
    Three(Rectangle, Rectangle, Rectangle),
    Four(Rectangle, Rectangle, Rectangle, Rectangle),
}
pub enum IntersectOverlappingRectanglesRes {
    One(Rectangle),
}

impl From<SubtractOverlappingRectanglesRes> for CompoundAtomOperationRes<Rectangle> {
    fn from(val: SubtractOverlappingRectanglesRes) -> Self {
        use CompoundAtomOperationRes::*;
        match val {
            SubtractOverlappingRectanglesRes::One(a) => One(a),
            SubtractOverlappingRectanglesRes::Two(a, b) => Two(a, b),
            SubtractOverlappingRectanglesRes::Three(a, b, c) => Three(a, b, c),
        }
    }
}
impl From<IntersectOverlappingRectanglesRes> for CompoundAtomOperationRes<Rectangle> {
    fn from(val: IntersectOverlappingRectanglesRes) -> Self {
        use CompoundAtomOperationRes::*;
        match val {
            IntersectOverlappingRectanglesRes::One(a) => One(a),
        }
    }
}

impl AtomOperations for Rectangle {
    type SubtractOverlappingRes = SubtractOverlappingRectanglesRes;
    type SubtractSubsetRes = CompoundAtomOperationRes<Rectangle>;
    type IntersectOverlappingRes = IntersectOverlappingRectanglesRes;

    fn subtract_subset(&self, other: &Self) -> CompoundAtomOperationRes<Rectangle> {
        use CompoundAtomOperationRes::*;
        use RectOverlapping::*;
        let (x0, y0, x1, y1) = self.as_tuple();
        let (a0, b0, a1, b1) = other.as_tuple();
        let h = (x0, a0, a1, x1);
        let v = (y0, b0, b1, y1);
        match (x0 == a0, y0 == b0, x1 == a1, y1 == b1) {
            (true, true, true, true) => unreachable!("if all side are equal rectangle is not subset"),
            (false, true, true, true) => self.subtract_as_overlap_type(other, BiggerRight).into(),
            (true, false, true, true) => self.subtract_as_overlap_type(other, BiggerBottom).into(),
            (true, true, false, true) => self.subtract_as_overlap_type(other, BiggerLeft).into(),
            (true, true, true, false) => self.subtract_as_overlap_type(other, BiggerTop).into(),
            (true, true, false, false) => self.subtract_as_overlap_type(other, TopLeft).into(),
            (false, true, true, false) => self.subtract_as_overlap_type(other, TopRight).into(),
            (false, false, true, true) => self.subtract_as_overlap_type(other, BottomRight).into(),
            (true, false, false, true) => self.subtract_as_overlap_type(other, BottomLeft).into(),
            (true, false, true, false) => self.subtract_as_overlap_type(other, Horizontal).into(),
            (false, true, false, true) => self.subtract_as_overlap_type(other, Vertical).into(),
            (true, false, false, false) => self.subtract_as_overlap_type(other, SmallerLeft).into(),
            (false, true, false, false) => self.subtract_as_overlap_type(other, SmallerTop).into(),
            (false, false, true, false) => self.subtract_as_overlap_type(other, SmallerRight).into(),
            (false, false, false, true) => self.subtract_as_overlap_type(other, SmallerBottom).into(),
            (false, false, false, false) => {
                Four(Self::new(h.0, v.0, h.2, v.1), Self::new(h.0, v.1, h.1, v.3), Self::new(h.1, v.2, h.3, v.3), Self::new(h.2, v.0, h.3, v.2))
            }
        }
    }

    fn subtract_overlapping(&self, other: &Self) -> SubtractOverlappingRectanglesRes {
        self.subtract_as_overlap_type(other, self.type_overlap(other))
    }

    fn intersect_overlapping(&self, other: &Self) -> IntersectOverlappingRectanglesRes {
        use IntersectOverlappingRectanglesRes::*;
        use RectOverlapping::*;
        let (x0, y0, x1, y1) = self.as_tuple();
        let (a0, b0, a1, b1) = other.as_tuple();
        match self.type_overlap(other) {
            SmallerLeft => One(Self::new(x0, b0, a1, b1)),
            SmallerRight => One(Self::new(a0, b0, x1, b1)),
            SmallerTop => One(Self::new(a0, y0, a1, b1)),
            SmallerBottom => One(Self::new(a0, b0, a1, y1)),
            BiggerLeft => One(Self::new(x0, y0, a1, y1)),
            BiggerRight => One(Self::new(a0, y0, x1, y1)),
            BiggerTop => One(Self::new(x0, y0, x1, b1)),
            BiggerBottom => One(Self::new(x0, b0, x1, y1)),
            TopLeft => One(Self::new(x0, y0, a1, b1)),
            TopRight => One(Self::new(a0, y0, x1, b1)),
            BottomRight => One(Self::new(a0, b0, x1, y1)),
            BottomLeft => One(Self::new(x0, b0, a1, y1)),
            Vertical => One(Self::new(a0, y0, a1, y1)),
            Horizontal => One(Self::new(x0, b0, x1, b1)),
        }
    }
}

impl AtomAlgebra for Rectangle {}

pub type Area = DisjointAtomsSet<Rectangle, (f32, f32)>;

impl Area {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::from_atom(Rectangle::new(x0, y0, x1, y1))
    }
}

impl PdfLine {
    /// Deriva l'`Area` dalla `bbox` on demand (nessun campo cache, R4 del `PLAN.md`).
    pub fn area(&self) -> Area {
        Area::from_atom(*self.bbox())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::geometry::Rectangle;
    use crate::commons::sets::{Container, Overlappable, SetRelation};
    use crate::commons::sets::indipendent_atoms::CompoundAtomOperationRes;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use std::collections::HashSet;

    mod area_construction {
        use super::*;

        #[test]
        fn new_wraps_a_single_atom_with_the_given_bounds() {
            let (x0, y0, x1, y1) = (3.4, 4.5, 4.5, 56.0);
            let mut expected = HashSet::new();
            expected.insert(Rectangle::new(x0, y0, x1, y1));
            assert_eq!(Area::new(x0, y0, x1, y1).atoms(), &expected);
        }
    }

    mod pdfline_derived_area {
        use super::*;

        #[test]
        fn matches_bbox_wrapped_as_a_single_atom_with_no_cached_field_needed() {
            let line = PdfLine::new("Arial", 10.0, "txt", (1.0, 2.0, 3.0, 4.0));
            let mut expected = HashSet::new();
            expected.insert(Rectangle::new(1.0, 2.0, 3.0, 4.0));
            assert_eq!(line.area().atoms(), &expected);
        }

        #[test]
        fn stays_consistent_with_bbox_across_repeated_calls() {
            let line = PdfLine::new("Arial", 10.0, "txt", (0.0, 0.0, 10.0, 10.0));
            assert_eq!(line.area().atoms(), line.area().atoms());
        }
    }

    mod containment {
        use super::*;
        use test_case::test_case;

        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (3.0, 29.89); "a point strictly inside")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (3.0, 20.0); "touching the top side")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (3.0, 80.0); "touching the bottom side")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (50.0, 78.9); "touching the right side")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (0.0, 60.4); "touching the left side")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (50.0, 20.0); "the top right corner")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (0.0, 20.0); "the top left corner")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (50.0, 80.0); "the bottom right corner")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (0.0, 80.0); "the bottom left corner")]
        fn contains_points_within_inclusive_bounds(rec: Rectangle, point: (f32, f32)) {
            assert!(rec.contains(&point));
        }

        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (100.0, 81.63); "far outside")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), (0.99, 50.0); "just to the left")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (55.0, 50.0); "just to the right")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (30.4, 11.11); "just above")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), (30.4, 88.88); "just below")]
        fn does_not_contain_points_outside_bounds(rec: Rectangle, point: (f32, f32)) {
            assert!(!rec.contains(&point));
        }
    }

    mod set_relation {
        use super::*;
        use SetRelation::*;
        use test_case::test_case;

        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Equal, Rectangle::new(0.0, 20.0, 50.0, 80.0); "identical rectangles")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(3.0, 29.89, 49.0, 79.0); "strictly wider rectangle")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(3.0, 20.0, 49.0, 79.0); "superset touching the top side")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(3.0, 29.89, 49.0, 80.0); "superset touching the bottom side")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(3.0, 29.89, 50.0, 79.0); "superset touching the right side")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(0.0, 29.89, 49.0, 79.0); "superset touching the left side")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(0.0, 20.0, 49.0, 79.0); "superset sharing the top left corner")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(3.0, 20.0, 50.0, 79.0); "superset sharing the top right corner")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(3.0, 29.89, 50.0, 80.0); "superset sharing the bottom right corner")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(0.0, 29.89, 49.0, 80.0); "superset sharing the bottom left corner")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(0.0, 20.0, 50.0, 79.0); "superset sharing both top corners")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(0.0, 29.89, 50.0, 80.0); "superset sharing both bottom corners")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(0.0, 20.0, 49.0, 80.0); "superset sharing both left corners")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Superset, Rectangle::new(3.0, 20.0, 50.0, 80.0); "superset sharing both right corners")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(0.0, 19.89, 59.9, 799.0); "strictly narrower rectangle")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(0.0, 20.0, 59.9, 799.0); "subset touching the top side")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(0.0, 19.89, 59.9, 80.0); "subset touching the bottom side")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(0.0, 19.89, 50.0, 799.0); "subset touching the right side")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(1.0, 19.89, 59.9, 799.0); "subset touching the left side")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(1.0, 20.0, 59.9, 799.0); "subset sharing the top left corner")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(0.0, 20.0, 50.0, 799.0); "subset sharing the top right corner")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(0.0, 10.0, 50.0, 80.0); "subset sharing the bottom right corner")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(1.0, 10.0, 59.9, 80.0); "subset sharing the bottom left corner")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(1.0, 20.0, 50.0, 799.0); "subset sharing both top corners")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(1.0, 10.0, 50.0, 80.0); "subset sharing both bottom corners")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(1.0, 20.0, 500.0, 80.0); "subset sharing both left corners")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Subset, Rectangle::new(0.4, 20.0, 50.0, 80.0); "subset sharing both right corners")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Disjoint, Rectangle::new(300.0, 290.89, 490.0, 790.0); "far apart rectangles")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Disjoint, Rectangle::new(0.0, 19.0, 490.0, 20.0); "disjoint touching the top edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Disjoint, Rectangle::new(0.0, 80.0, 490.0, 790.0); "disjoint touching the bottom edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Disjoint, Rectangle::new(0.0, 19.89, 1.0, 790.0); "disjoint touching the left edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Disjoint, Rectangle::new(50.0, 19.89, 490.0, 790.0); "disjoint touching the right edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Disjoint, Rectangle::new(0.0, 19.89, 1.0, 20.0); "disjoint touching the top left corner")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Disjoint, Rectangle::new(0.0, 80.0, 1.0, 790.0); "disjoint touching the bottom left corner")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Disjoint, Rectangle::new(50.0, 19.89, 490.0, 20.0); "disjoint touching the top right corner")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Disjoint, Rectangle::new(50.0, 80.0, 490.0, 790.0); "disjoint touching the bottom right corner")]
        #[test_case(Rectangle::new(0.0, 20.0, 50.0, 80.0), Overlapping, Rectangle::new(3.0, 13.11, 49.0, 81.0); "properly overlapping rectangles")]
        fn matches_expected_relation(a: Rectangle, rel: SetRelation, b: Rectangle) {
            assert_eq!(a.set_relation(&b), rel);
        }
    }

    /// `type_overlap` è la classificazione (privata) da cui dipendono sia `subtract_as_overlap_type`
    /// sia, indirettamente, `subtract_subset`/`subtract_overlapping`: usa `<` su alcuni confini e
    /// `<=` su altri, quindi un caso "di lato" per variante non basta — ogni variante è coperta sia
    /// dal caso generico sia da ogni combinazione di lati/angoli condivisi ("touch"), portando
    /// verbatim l'esaustività del riferimento (`PLAN.md` §10: stress test dove la logica è
    /// combinatoria).
    mod type_overlap_classification {
        use super::*;
        use RectOverlapping::*;
        use test_case::test_case;

        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), SmallerLeft, Rectangle::new(0.0, 23.11, 2.0, 67.0); "smaller left")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), SmallerRight, Rectangle::new(16.0, 23.11, 200.0, 67.0); "smaller right")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), SmallerTop, Rectangle::new(1.1, 13.11, 2.0, 67.0); "smaller top")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), SmallerBottom, Rectangle::new(1.1, 67.0, 2.0, 670.0); "smaller bottom")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerLeft, Rectangle::new(0.0, 13.11, 2.0, 670.0); "bigger left")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerLeft, Rectangle::new(1.0, 13.11, 2.0, 670.0); "bigger left touching the left edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerLeft, Rectangle::new(0.0, 13.11, 2.0, 80.0); "bigger left touching the bottom edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerLeft, Rectangle::new(0.0, 20.0, 2.0, 670.0); "bigger left touching the top edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerLeft, Rectangle::new(0.0, 20.0, 2.0, 80.0); "bigger left touching both top and bottom edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerLeft, Rectangle::new(1.0, 13.11, 2.0, 80.0); "bigger left touching bottom and left edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerLeft, Rectangle::new(0.0, 20.0, 2.0, 670.0); "bigger left touching top and left edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerRight, Rectangle::new(16.0, 13.11, 200.0, 670.0); "bigger right")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerRight, Rectangle::new(16.0, 13.11, 50.0, 670.0); "bigger right touching the right edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerRight, Rectangle::new(16.0, 13.11, 200.0, 80.0); "bigger right touching the bottom edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerRight, Rectangle::new(16.0, 20.0, 200.0, 670.0); "bigger right touching the top edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerRight, Rectangle::new(16.0, 20.0, 200.0, 80.0); "bigger right touching both top and bottom edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerRight, Rectangle::new(16.0, 13.11, 50.0, 80.0); "bigger right touching bottom and right edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerRight, Rectangle::new(16.0, 20.0, 50.0, 670.0); "bigger right touching top and right edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerTop, Rectangle::new(0.1, 13.11, 200.0, 67.0); "bigger top")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerTop, Rectangle::new(0.1, 20.0, 200.0, 67.0); "bigger top touching the top edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerTop, Rectangle::new(1.0, 13.11, 200.0, 67.0); "bigger top touching the left edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerTop, Rectangle::new(0.1, 13.11, 50.0, 67.0); "bigger top touching the right edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerTop, Rectangle::new(1.0, 13.11, 50.0, 67.0); "bigger top touching both left and right edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerTop, Rectangle::new(1.0, 20.0, 200.0, 67.0); "bigger top touching top and left edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerTop, Rectangle::new(0.1, 20.0, 50.0, 67.0); "bigger top touching top and right edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerBottom, Rectangle::new(0.1, 67.0, 200.0, 670.0); "bigger bottom")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerBottom, Rectangle::new(0.1, 67.0, 200.0, 80.0); "bigger bottom touching the bottom edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerBottom, Rectangle::new(1.0, 67.0, 200.0, 670.0); "bigger bottom touching the left edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerBottom, Rectangle::new(0.1, 67.0, 50.0, 670.0); "bigger bottom touching the right edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerBottom, Rectangle::new(1.0, 67.0, 50.0, 670.0); "bigger bottom touching both left and right edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerBottom, Rectangle::new(1.0, 67.0, 200.0, 80.0); "bigger bottom touching bottom and left edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BiggerBottom, Rectangle::new(0.1, 67.0, 50.0, 80.0); "bigger bottom touching bottom and right edges")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), TopLeft, Rectangle::new(0.0, 13.11, 2.0, 67.0); "top left")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), TopLeft, Rectangle::new(0.0, 20.0, 2.0, 67.0); "top left touching the top edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), TopRight, Rectangle::new(41.41, 13.11, 200.0, 67.0); "top right")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), TopRight, Rectangle::new(41.41, 20.0, 200.0, 67.0); "top right touching the top edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), TopRight, Rectangle::new(41.41, 13.11, 50.0, 67.0); "top right touching the right edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BottomLeft, Rectangle::new(0.1, 25.11, 26.0, 670.0); "bottom left")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BottomLeft, Rectangle::new(0.1, 25.11, 26.0, 80.0); "bottom left touching the bottom edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BottomLeft, Rectangle::new(1.0, 25.11, 26.0, 670.0); "bottom left touching the left edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BottomRight, Rectangle::new(5.1, 67.0, 200.0, 670.0); "bottom right")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BottomRight, Rectangle::new(5.1, 67.0, 200.0, 80.0); "bottom right touching the bottom edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), BottomRight, Rectangle::new(5.1, 67.0, 50.0, 670.0); "bottom right touching the right edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Vertical, Rectangle::new(5.1, 17.0, 23.0, 670.0); "vertical")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Vertical, Rectangle::new(5.1, 20.0, 23.0, 670.0); "vertical touching the top edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Vertical, Rectangle::new(5.1, 17.0, 23.0, 80.0); "vertical touching the bottom edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Horizontal, Rectangle::new(0.1, 22.0, 200.0, 67.0); "horizontal")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Horizontal, Rectangle::new(1.0, 22.0, 200.0, 67.0); "horizontal touching the left edge")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Horizontal, Rectangle::new(0.1, 22.0, 50.0, 67.0); "horizontal touching the right edge")]
        fn matches_expected_variant(a: Rectangle, ovrt: RectOverlapping, b: Rectangle) {
            assert_eq!(a.type_overlap(&b), ovrt);
        }
    }

    /// `subtract_subset`/`subtract_overlapping` possono produrre da uno a quattro rettangoli
    /// (mai piu' di quattro): questi test coprono tutte le combinazioni di lati/angoli condivisi
    /// che determinano quanti pezzi risultano, non solo il caso generico "quattro pezzi".
    mod atom_operations {
        use super::*;
        use CompoundAtomOperationRes::*;
        use test_case::test_case;

        fn assert_matches(result: CompoundAtomOperationRes<Rectangle>, expected: CompoundAtomOperationRes<Rectangle>) {
            match (result, expected) {
                (Four(ra, rb, rc, rd), Four(ea, eb, ec, ed)) => {
                    assert_eq!(ra.as_tuple(), ea.as_tuple());
                    assert_eq!(rb.as_tuple(), eb.as_tuple());
                    assert_eq!(rc.as_tuple(), ec.as_tuple());
                    assert_eq!(rd.as_tuple(), ed.as_tuple());
                }
                (Three(ra, rb, rc), Three(ea, eb, ec)) => {
                    assert_eq!(ra.as_tuple(), ea.as_tuple());
                    assert_eq!(rb.as_tuple(), eb.as_tuple());
                    assert_eq!(rc.as_tuple(), ec.as_tuple());
                }
                (Two(ra, rb), Two(ea, eb)) => {
                    assert_eq!(ra.as_tuple(), ea.as_tuple());
                    assert_eq!(rb.as_tuple(), eb.as_tuple());
                }
                (One(r), One(e)) => assert_eq!(r.as_tuple(), e.as_tuple()),
                _ => panic!("result doesn't have the expected shape"),
            }
        }

        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(3.0, 29.89, 49.0, 79.0),
            Four(
                Rectangle::new(1.0, 20.0, 49.0, 29.89),
                Rectangle::new(1.0, 29.89, 3.0, 80.0),
                Rectangle::new(3.0, 79.0, 50.0, 80.0),
                Rectangle::new(49.0, 20.0, 50.0, 79.0)
            ); "no shared side splits into four")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(3.0, 20.0, 49.0, 79.0),
            Three(
                Rectangle::new(49.0, 20.0, 50.0, 80.0),
                Rectangle::new(1.0, 79.0, 49.0, 80.0),
                Rectangle::new(1.0, 20.0, 3.0, 79.0)
            ); "sharing the top side splits into three")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(3.0, 22.22, 49.0, 80.0),
            Three(
                Rectangle::new(1.0, 20.0, 3.0, 80.0),
                Rectangle::new(3.0, 20.0, 50.0, 22.22),
                Rectangle::new(49.0, 22.22, 50.0, 80.0)
            ); "sharing the bottom side splits into three")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(2.0, 22.0, 50.0, 70.0),
            Three(
                Rectangle::new(1.0, 70.0, 50.0, 80.0),
                Rectangle::new(1.0, 20.0, 2.0, 70.0),
                Rectangle::new(2.0, 20.0, 50.0, 22.0)
            ); "sharing the right side splits into three")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 22.0, 2.0, 70.0),
            Three(
                Rectangle::new(1.0, 20.0, 50.0, 22.0),
                Rectangle::new(2.0, 22.0, 50.0, 80.0),
                Rectangle::new(1.0, 70.0, 2.0, 80.0)
            ); "sharing the left side splits into three")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 20.0, 40.0, 70.0),
            Two(Rectangle::new(40.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 70.0, 40.0, 80.0)); "sharing the top left corner splits into two")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(10.0, 20.0, 50.0, 60.0),
            Two(Rectangle::new(1.0, 60.0, 50.0, 80.0), Rectangle::new(1.0, 20.0, 10.0, 60.0)); "sharing the top right corner splits into two")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(10.0, 25.5, 50.0, 80.0),
            Two(Rectangle::new(1.0, 20.0, 10.0, 80.0), Rectangle::new(10.0, 20.0, 50.0, 25.5)); "sharing the bottom right corner splits into two")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 25.2, 40.0, 80.0),
            Two(Rectangle::new(1.0, 20.0, 50.0, 25.2), Rectangle::new(40.0, 25.2, 50.0, 80.0)); "sharing the bottom left corner splits into two")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(11.0, 20.0, 30.0, 80.0),
            Two(Rectangle::new(1.0, 20.0, 11.0, 80.0), Rectangle::new(30.0, 20.0, 50.0, 80.0)); "crossing vertically splits into two")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 25.0, 50.0, 77.0),
            Two(Rectangle::new(1.0, 20.0, 50.0, 25.0), Rectangle::new(1.0, 77.0, 50.0, 80.0)); "crossing horizontally splits into two")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 20.0, 50.0, 40.0),
            One(Rectangle::new(1.0, 40.0, 50.0, 80.0)); "sharing both top corners keeps one piece")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 23.0, 50.0, 80.0),
            One(Rectangle::new(1.0, 20.0, 50.0, 23.0)); "sharing both bottom corners keeps one piece")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 20.0, 33.0, 80.0),
            One(Rectangle::new(33.0, 20.0, 50.0, 80.0)); "sharing both left corners keeps one piece")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(11.0, 20.0, 50.0, 80.0),
            One(Rectangle::new(1.0, 20.0, 11.0, 80.0)); "sharing both right corners keeps one piece")]
        fn subtract_subset_splits_the_remaining_border(a: Rectangle, b: Rectangle, expected: CompoundAtomOperationRes<Rectangle>) {
            assert_matches(a.subtract_subset(&b).into(), expected);
        }

        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(3.0, 19.0, 49.0, 79.0),
            Three(Rectangle::new(49.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 79.0, 49.0, 80.0), Rectangle::new(1.0, 20.0, 3.0, 79.0)); "smaller and above")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(3.0, 22.22, 49.0, 88.0),
            Three(Rectangle::new(1.0, 20.0, 3.0, 80.0), Rectangle::new(3.0, 20.0, 50.0, 22.22), Rectangle::new(49.0, 22.22, 50.0, 80.0)); "smaller and below")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(2.0, 22.0, 55.0, 70.0),
            Three(Rectangle::new(1.0, 70.0, 50.0, 80.0), Rectangle::new(1.0, 20.0, 2.0, 70.0), Rectangle::new(2.0, 20.0, 50.0, 22.0)); "smaller and to the right")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(0.0, 22.0, 2.0, 70.0),
            Three(Rectangle::new(1.0, 20.0, 50.0, 22.0), Rectangle::new(2.0, 22.0, 50.0, 80.0), Rectangle::new(1.0, 70.0, 2.0, 80.0)); "smaller and to the left")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(0.2, 2.0, 33.0, 80.2),
            One(Rectangle::new(33.0, 20.0, 50.0, 80.0)); "bigger on the left")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(11.0, 2.2, 51.0, 80.2),
            One(Rectangle::new(1.0, 20.0, 11.0, 80.0)); "bigger on the right")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(0.2, 2.0, 500.0, 40.0),
            One(Rectangle::new(1.0, 40.0, 50.0, 80.0)); "bigger on top")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(0.2, 23.0, 500.0, 86.0),
            One(Rectangle::new(1.0, 20.0, 50.0, 23.0)); "bigger on the bottom")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(0.0, 2.0, 40.0, 70.0),
            Two(Rectangle::new(40.0, 20.0, 50.0, 80.0), Rectangle::new(1.0, 70.0, 40.0, 80.0)); "overlapping the top left corner")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(11.0, 12.0, 30.0, 90.0),
            Two(Rectangle::new(1.0, 20.0, 11.0, 80.0), Rectangle::new(30.0, 20.0, 50.0, 80.0)); "overlapping crosses vertically")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(0.05, 25.0, 500.0, 77.0),
            Two(Rectangle::new(1.0, 20.0, 50.0, 25.0), Rectangle::new(1.0, 77.0, 50.0, 80.0)); "overlapping crosses horizontally")]
        fn subtract_overlapping_keeps_the_non_overlapping_side(a: Rectangle, b: Rectangle, expected: CompoundAtomOperationRes<Rectangle>) {
            assert_matches(a.subtract_overlapping(&b).into(), expected);
        }

        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(3.0, 19.0, 49.0, 79.0),
            One(Rectangle::new(3.0, 20.0, 49.0, 79.0)); "overlap smaller and above")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(2.0, 22.0, 55.0, 70.0),
            One(Rectangle::new(2.0, 22.0, 50.0, 70.0)); "overlap smaller and to the right")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(11.0, 12.0, 30.0, 90.0),
            One(Rectangle::new(11.0, 20.0, 30.0, 80.0)); "overlap crosses vertically")]
        #[test_case(Rectangle::new(1.0, 20.0, 50.0, 80.0), Rectangle::new(0.05, 25.0, 500.0, 77.0),
            One(Rectangle::new(1.0, 25.0, 50.0, 77.0)); "overlap crosses horizontally")]
        fn intersect_overlapping_keeps_the_shared_rectangle(a: Rectangle, b: Rectangle, expected: CompoundAtomOperationRes<Rectangle>) {
            assert_matches(a.intersect_overlapping(&b).into(), expected);
        }
    }
}
