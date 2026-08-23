//! Selezione per corpo del font (intervalli).
//!
//! Porting verbatim (`PLAN.md` §0/§12 D14) di
//! `freeports_core::formats_utils::pdf_extract::select::pdf_line::font_size`. Il tipo atomo
//! (`PositiveLimits`) e' gia' definito in `commons::geometry` (M1): questo modulo vi implementa
//! sopra `Container`/`Overlappable`/`AtomOperations`/`AtomAlgebra` (non ce li ha di suo, vedi il
//! doc-comment di `commons::geometry` — quel modulo espone solo costruzione validata e
//! `as_tuple`, l'algebra di selezione vive qui per lo stesso motivo di R4: i dati non dipendono
//! dalle selezioni).
//!
//! Contratto atteso dai test qui sotto (il test-writer non scrive codice di produzione):
//!
//! - `impl Container for PositiveLimits { type Elem = f32; ... }`: `a <= x && x <= b`
//!   (estremi inclusi).
//! - `impl Overlappable<Self> for PositiveLimits`: le cinque relazioni standard, con gli
//!   estremi che si toccano trattati come *non* disgiunti (`Subset`/`Superset`/`Overlapping` a
//!   seconda dei casi), esattamente come nel riferimento.
//! - `impl AtomOperations for PositiveLimits`: `subtract_subset` produce uno o due intervalli
//!   a seconda che uno dei due estremi coincida; `subtract_overlapping`/`intersect_overlapping`
//!   producono sempre un solo intervallo.
//! - `impl AtomAlgebra for PositiveLimits {}`.
//! - `type Interval = DisjointAtomsSet<PositiveLimits,f32>; pub type FontSizeSet = Interval; pub
//!   type FontSizeInterval = FontSizeSet;` con:
//!   - `FontSizeInterval::new(a: f32, b: f32) -> Self` (= `Self::from_atom(PositiveLimits::new(a,b))`).
//!   - `FontSizeInterval::from_precision(c: f32, prec: f32) -> Self`: intervallo
//!     `[max(0.0, c-prec), c+prec]` (il minimo con `0.0` evita bound negativi, dato che
//!     `PositiveLimits` li rifiuta).

use ordered_float::OrderedFloat;
use std::cmp::max;

use crate::commons::geometry::PositiveLimits;
use crate::commons::sets::indipendent_atoms::{AtomAlgebra, AtomOperations, CompoundAtomOperationRes, DisjointAtomsSet};
use crate::commons::sets::{Container, Overlappable, SetRelation};

impl Container for PositiveLimits {
    type Elem = f32;
    fn contains(&self, x: &f32) -> bool {
        let (a, b) = self.as_tuple();
        a <= *x && *x <= b
    }
}

impl Overlappable<Self> for PositiveLimits {
    fn set_relation(&self, other: &Self) -> SetRelation {
        use SetRelation::*;
        let (a0, a1) = self.as_tuple();
        let (b0, b1) = other.as_tuple();
        if a0 >= b1 || b0 >= a1 {
            Disjoint
        } else if a0 == b0 && a1 == b1 {
            Equal
        } else if b0 <= a0 && a1 <= b1 {
            Subset
        } else if a0 <= b0 && b1 <= a1 {
            Superset
        } else {
            Overlapping
        }
    }
}

pub enum SubtractOverlappingPositiveLimitsRes {
    One(PositiveLimits),
}
pub enum SubtractSubsetPositiveLimitsRes {
    One(PositiveLimits),
    Two(PositiveLimits, PositiveLimits),
}

pub enum IntersectOverlappingPositiveLimitsRes {
    One(PositiveLimits),
}

impl From<SubtractOverlappingPositiveLimitsRes> for CompoundAtomOperationRes<PositiveLimits> {
    fn from(val: SubtractOverlappingPositiveLimitsRes) -> Self {
        use CompoundAtomOperationRes::*;
        match val {
            SubtractOverlappingPositiveLimitsRes::One(a) => One(a),
        }
    }
}

impl From<SubtractSubsetPositiveLimitsRes> for CompoundAtomOperationRes<PositiveLimits> {
    fn from(val: SubtractSubsetPositiveLimitsRes) -> Self {
        use CompoundAtomOperationRes::*;
        match val {
            SubtractSubsetPositiveLimitsRes::One(a) => One(a),
            SubtractSubsetPositiveLimitsRes::Two(a, b) => Two(a, b),
        }
    }
}

impl From<IntersectOverlappingPositiveLimitsRes> for CompoundAtomOperationRes<PositiveLimits> {
    fn from(val: IntersectOverlappingPositiveLimitsRes) -> Self {
        use CompoundAtomOperationRes::*;
        match val {
            IntersectOverlappingPositiveLimitsRes::One(a) => One(a),
        }
    }
}

impl AtomOperations for PositiveLimits {
    type SubtractSubsetRes = SubtractSubsetPositiveLimitsRes;
    type SubtractOverlappingRes = SubtractOverlappingPositiveLimitsRes;
    type IntersectOverlappingRes = IntersectOverlappingPositiveLimitsRes;
    fn subtract_subset(&self, other: &Self) -> SubtractSubsetPositiveLimitsRes {
        use SubtractSubsetPositiveLimitsRes::*;
        let (a0, a1) = self.as_tuple();
        let (b0, b1) = other.as_tuple();
        if b0 == a0 {
            One(PositiveLimits::new(b1, a1))
        } else if a1 == b1 {
            One(PositiveLimits::new(a0, b0))
        } else {
            Two(PositiveLimits::new(a0, b0), PositiveLimits::new(b1, a1))
        }
    }
    fn subtract_overlapping(&self, other: &Self) -> SubtractOverlappingPositiveLimitsRes {
        use SubtractOverlappingPositiveLimitsRes::*;
        let (a0, a1) = self.as_tuple();
        let (b0, b1) = other.as_tuple();
        if b1 >= a1 { One(PositiveLimits::new(a0, b0)) } else { One(PositiveLimits::new(b1, a1)) }
    }
    fn intersect_overlapping(&self, other: &Self) -> IntersectOverlappingPositiveLimitsRes {
        use IntersectOverlappingPositiveLimitsRes::*;
        let (a0, a1) = self.as_tuple();
        let (b0, b1) = other.as_tuple();
        if b1 >= a1 { One(PositiveLimits::new(b0, a1)) } else { One(PositiveLimits::new(a0, b1)) }
    }
    fn union_overlapping(&self, other: &Self) -> CompoundAtomOperationRes<Self> {
        use CompoundAtomOperationRes::*;
        let (a0, a1) = self.as_tuple();
        let (b0, b1) = other.as_tuple();
        if a0 < b0 { One(PositiveLimits::new(a0, b1)) } else { One(PositiveLimits::new(b0, a1)) }
    }
}

impl AtomAlgebra for PositiveLimits {}

type Interval = DisjointAtomsSet<PositiveLimits, f32>;
pub type FontSizeSet = Interval;
pub type FontSizeInterval = FontSizeSet;

impl FontSizeInterval {
    pub fn new(a: f32, b: f32) -> Self {
        Self::from_atom(PositiveLimits::new(a, b))
    }
    pub fn from_precision(c: f32, prec: f32) -> Self {
        let a = max(OrderedFloat(0.0), OrderedFloat(c - prec)).into_inner();
        Self::from_atom(PositiveLimits::new(a, c + prec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::geometry::PositiveLimits;
    use crate::commons::sets::{Container, Overlappable, SetRelation};
    use crate::commons::sets::indipendent_atoms::{AtomOperations, CompoundAtomOperationRes};
    use std::collections::HashSet;

    mod font_size_interval_construction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn new_wraps_a_single_atom_with_the_given_bounds() {
            let mut expected = HashSet::new();
            expected.insert(PositiveLimits::new(6.0, 70.0));
            assert_eq!(FontSizeInterval::new(6.0, 70.0).atoms(), &expected);
        }

        #[test]
        fn from_precision_centers_a_symmetric_window_around_c() {
            let mut expected = HashSet::new();
            expected.insert(PositiveLimits::new(55.0, 65.0));
            assert_eq!(FontSizeInterval::from_precision(60.0, 5.0).atoms(), &expected);
        }

        #[test]
        fn from_precision_clamps_the_lower_bound_at_zero() {
            let mut expected = HashSet::new();
            expected.insert(PositiveLimits::new(0.0, 6.0));
            assert_eq!(FontSizeInterval::from_precision(1.0, 5.0).atoms(), &expected);
        }
    }

    mod containment {
        use super::*;
        use test_case::test_case;

        #[test_case(PositiveLimits::new(20.0, 50.0), 30.5; "a value strictly inside the interval")]
        #[test_case(PositiveLimits::new(20.0, 50.0), 50.0; "the right bound itself")]
        #[test_case(PositiveLimits::new(20.0, 50.0), 20.0; "the left bound itself")]
        fn contains_values_within_inclusive_bounds(interval: PositiveLimits, x: f32) {
            assert!(interval.contains(&x));
        }

        #[test_case(10.5; "below the left bound")]
        #[test_case(55.5; "above the right bound")]
        fn does_not_contain_values_outside_bounds(x: f32) {
            let interval = PositiveLimits::build(20.0, 50.0).unwrap();
            assert!(!interval.contains(&x));
        }
    }

    mod set_relation {
        use super::*;
        use SetRelation::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(PositiveLimits::new(2.0, 5.5), Equal, PositiveLimits::new(2.0, 5.5); "identical bounds are equal")]
        #[test_case(PositiveLimits::new(1.9, 5.8), Superset, PositiveLimits::new(2.0, 5.5); "strictly wider interval is a superset")]
        #[test_case(PositiveLimits::new(2.0, 5.8), Superset, PositiveLimits::new(2.0, 5.5); "superset touching on the left bound")]
        #[test_case(PositiveLimits::new(1.9, 5.5), Superset, PositiveLimits::new(2.0, 5.5); "superset touching on the right bound")]
        #[test_case(PositiveLimits::new(3.0, 3.5), Subset, PositiveLimits::new(2.0, 5.5); "strictly narrower interval is a subset")]
        #[test_case(PositiveLimits::new(2.0, 3.5), Subset, PositiveLimits::new(2.0, 5.5); "subset touching on the left bound")]
        #[test_case(PositiveLimits::new(3.0, 5.5), Subset, PositiveLimits::new(2.0, 5.5); "subset touching on the right bound")]
        #[test_case(PositiveLimits::new(2.0, 5.5), Overlapping, PositiveLimits::new(5.0, 50.5); "overlapping intervals")]
        #[test_case(PositiveLimits::new(2.0, 5.5), Disjoint, PositiveLimits::new(20.0, 50.5); "far apart intervals are disjoint")]
        #[test_case(PositiveLimits::new(2.0, 5.5), Disjoint, PositiveLimits::new(5.5, 50.5); "touching endpoints are still disjoint")]
        fn matches_expected_relation(a: PositiveLimits, rel: SetRelation, b: PositiveLimits) {
            assert_eq!(a.set_relation(&b), rel);
        }
    }

    mod atom_operations {
        use super::*;
        use CompoundAtomOperationRes::*;
        use test_case::test_case;
        use pretty_assertions::assert_eq;

        #[test_case(PositiveLimits::new(2.0, 5.5), PositiveLimits::new(5.0, 50.5), One(PositiveLimits::new(2.0, 5.0)); "keeps the part to the right of the overlap")]
        #[test_case(PositiveLimits::new(5.0, 53.5), PositiveLimits::new(2.0, 5.5), One(PositiveLimits::new(5.5, 53.5)); "keeps the part to the left of the overlap")]
        fn subtract_overlapping_keeps_the_non_overlapping_side(a: PositiveLimits, b: PositiveLimits, expected: CompoundAtomOperationRes<PositiveLimits>) {
            match (a.subtract_overlapping(&b).into(), expected) {
                (One(r), One(e)) => assert_eq!(r.as_tuple(), e.as_tuple()),
                _ => panic!("expected a single-atom result"),
            }
        }

        #[test_case(PositiveLimits::new(2.0, 5.5), PositiveLimits::new(5.0, 50.5), One(PositiveLimits::new(5.0, 5.5)); "right overlap")]
        #[test_case(PositiveLimits::new(5.1, 53.5), PositiveLimits::new(2.0, 5.6), One(PositiveLimits::new(5.1, 5.6)); "left overlap")]
        fn intersect_overlapping_keeps_the_shared_part(a: PositiveLimits, b: PositiveLimits, expected: CompoundAtomOperationRes<PositiveLimits>) {
            match (a.intersect_overlapping(&b).into(), expected) {
                (One(r), One(e)) => assert_eq!(r.as_tuple(), e.as_tuple()),
                _ => panic!("expected a single-atom result"),
            }
        }

        #[test_case(PositiveLimits::new(2.0, 5.5), PositiveLimits::new(5.0, 50.5), One(PositiveLimits::new(2.0, 50.5)); "right overlap")]
        #[test_case(PositiveLimits::new(5.1, 53.5), PositiveLimits::new(2.2, 5.6), One(PositiveLimits::new(2.2, 53.5)); "left overlap")]
        fn union_overlapping_spans_both(a: PositiveLimits, b: PositiveLimits, expected: CompoundAtomOperationRes<PositiveLimits>) {
            match (a.union_overlapping(&b), expected) {
                (One(r), One(e)) => assert_eq!(r.as_tuple(), e.as_tuple()),
                _ => panic!("expected a single-atom result"),
            }
        }

        #[test_case(PositiveLimits::new(30.6, 40.2), PositiveLimits::new(33.6, 36.1), Two(PositiveLimits::new(30.6, 33.6), PositiveLimits::new(36.1, 40.2)); "subset strictly inside splits into two")]
        #[test_case(PositiveLimits::new(30.6, 40.2), PositiveLimits::new(30.6, 36.1), One(PositiveLimits::new(36.1, 40.2)); "subset touching the left bound keeps one piece")]
        #[test_case(PositiveLimits::new(30.6, 40.2), PositiveLimits::new(33.6, 40.2), One(PositiveLimits::new(30.6, 33.6)); "subset touching the right bound keeps one piece")]
        fn subtract_subset_splits_around_the_hole(a: PositiveLimits, b: PositiveLimits, expected: CompoundAtomOperationRes<PositiveLimits>) {
            match (a.subtract_subset(&b).into(), expected) {
                (One(r), One(e)) => assert_eq!(r.as_tuple(), e.as_tuple()),
                (Two(ra, rb), Two(ea, eb)) => {
                    assert_eq!(ra.as_tuple(), ea.as_tuple());
                    assert_eq!(rb.as_tuple(), eb.as_tuple());
                }
                _ => panic!("result doesn't have the expected shape"),
            }
        }
    }
}
