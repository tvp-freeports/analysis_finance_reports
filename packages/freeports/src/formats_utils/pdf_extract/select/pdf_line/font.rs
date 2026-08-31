//! Selecting lines by font.
//!
//! The [`Font`] type itself — the data and its normalisation — lives in [`super::super::pdf_line`];
//! this module imports it and implements the selection algebra on top. Rust allows the impl to sit
//! outside the file defining the type, and the split is deliberate: the data must not depend on the
//! selections.
//!
//! Fonts are *atoms* in that algebra: two normalised fonts are either equal or disjoint, never
//! partially overlapping. That is what makes a set of fonts a plain disjoint set, with no interval
//! arithmetic behind it.

use crate::commons::sets::indipendent_atoms::{AtomAlgebra, AtomOperations, CompoundAtomOperationRes, DisjointAtomsSet};
use crate::commons::sets::{Container, Overlappable, SetRelation};
use crate::formats_utils::pdf_extract::pdf_line::Font;

impl Container for Font {
    type Elem = Font;
    fn contains(&self, other: &Self) -> bool {
        self.inner() == other.inner()
    }
}

impl Overlappable<Self> for Font {
    fn set_relation(&self, other: &Self) -> SetRelation {
        use SetRelation::*;
        if self.inner() == other.inner() { Equal } else { Disjoint }
    }
}

impl AtomOperations for Font {
    type SubtractOverlappingRes = CompoundAtomOperationRes<Font>;
    type SubtractSubsetRes = CompoundAtomOperationRes<Font>;
    type IntersectOverlappingRes = CompoundAtomOperationRes<Font>;
    fn subtract_subset(&self, _other: &Self) -> CompoundAtomOperationRes<Font> {
        unreachable!("Font cannot be a subset of another")
    }
    fn subtract_overlapping(&self, _other: &Self) -> CompoundAtomOperationRes<Font> {
        unreachable!("Font cannot be overlapping another")
    }
    fn intersect_overlapping(&self, _other: &Self) -> CompoundAtomOperationRes<Font> {
        unreachable!("Font cannot be overlapping another")
    }
}

impl AtomAlgebra for Font {}

pub type FontSet = DisjointAtomsSet<Font, Font>;

impl FontSet {
    pub fn new(font: &str) -> Self {
        Self::from_atom(Font::new(font))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::sets::{Container, Overlappable, SetRelation};
    use crate::commons::sets::indipendent_atoms::AtomOperations;
    use crate::formats_utils::pdf_extract::pdf_line::Font;
    use std::collections::HashSet;

    mod containment {
        use super::*;

        #[test]
        fn contains_an_equivalently_normalized_font() {
            let font_set = Font::new("casa Sapaforica/L");
            let font = Font::new("CASA-SAPAFORICA-l");
            assert!(font_set.contains(&font));
        }

        #[test]
        fn does_not_contain_a_different_font() {
            let font_set = Font::new("Liquor& ca/io ");
            let font = Font::new("CASA,Semaforica");
            assert!(!font_set.contains(&font));
        }
    }

    mod set_relation {
        use super::*;
        use test_case::test_case;

        #[test_case("\tcalimo", SetRelation::Equal, " CalImo "; "equal after normalization")]
        #[test_case("\tcalimo", SetRelation::Disjoint, " Calo "; "disjoint when normalized forms differ")]
        fn matches_expected_relation(a: &str, rel: SetRelation, b: &str) {
            assert_eq!(Font::new(a).set_relation(&Font::new(b)), rel);
        }
    }

    mod atom_operations_panic_on_impossible_cases {
        use super::*;

        #[test]
        #[should_panic(expected = "internal error: entered unreachable code: Font cannot be overlapping another")]
        fn subtract_overlapping_is_unreachable() {
            let a = Font::new("A");
            let b = Font::new("B");
            a.subtract_overlapping(&b);
        }

        #[test]
        #[should_panic(expected = "internal error: entered unreachable code: Font cannot be overlapping another")]
        fn intersect_overlapping_is_unreachable() {
            let a = Font::new("A");
            let b = Font::new("B");
            a.intersect_overlapping(&b);
        }

        #[test]
        #[should_panic(expected = "internal error: entered unreachable code: Font cannot be a subset of another")]
        fn subtract_subset_is_unreachable() {
            let a = Font::new("A");
            let b = Font::new("B");
            a.subtract_subset(&b);
        }
    }

    mod font_set_construction {
        use super::*;

        #[test]
        fn new_wraps_a_single_normalized_atom() {
            let set = FontSet::new("NicaRAguA");
            let mut expected = HashSet::new();
            let normalized = Font::new("nicaragua");
            expected.insert(&normalized);
            assert_eq!(set.atoms_ref(), expected);
        }
    }
}
