//! The native-Rust half of `formats_repo::semistructured`'s hybrid dispatch (Decision 5's "native
//! Rust by name first, fall back to the author's Python file if not found natively") — a small,
//! hand-coded registry of which algorithm names are implemented natively, keyed by
//! [`crate::formats_repo::semistructured::SegmentKind`], plus the one native algorithm that
//! exists today ([`pdf_extract::standard_cost_curr`], sequencing item 2, already landed).
//!
//! # Ground truth (not guessed)
//!
//! Mirrors exactly what `_get_defined_list(p, inspect.isfunction)`/`_get_defined_list(t, ...)`/
//! `_get_defined_list(d, ...)` compute today in `acquisition.py` (the whitelist
//! `formats_mapping_schema`'s `pa.Check(lambda x: x.isin(pdf_extract_funcs))` etc. validate CSV
//! cells against) — i.e. "every top-level function actually defined in the *built-in*
//! `semistructured/{pdf_extract,text_filter,deserialize}.py` file for this segment". Confirmed by
//! reading all 3 files directly: `pdf_extract.py` defines exactly one function,
//! `standard_cost_curr`; `text_filter.py` and `deserialize.py` are both **entirely empty** (no
//! functions or classes defined at all) — so their registries are empty, not merely
//! "unimplemented", matching today's real, checked-in behavior exactly, not a stub carved out of
//! this task's own scope.
//!
//! This is deliberately **not** a literal `HashMap<&str, fn(...)>` (or similar uniform
//! function-pointer table): each (hypothetical, future) native algorithm has its own distinct
//! argument/return shape (`standard_cost_curr`'s own signature, `PyResult<(PdfExtractInvestments
//! Standard, Py<PyAny>, PdfExtractCurrencyConstant)>`, has nothing in common with, say, a
//! single-pipe `text_filter` algorithm's eventual shape), so there is no meaningful common
//! function-pointer type to store generically. [`resolve`](crate::formats_repo::semistructured::
//! resolve)/[`get_pipelines`](crate::formats_repo::semistructured::get_pipelines) use [`contains`]
//! purely for **name membership** (is `name` native for `segment` at all, and — for the collision
//! check, Decision 2 — does the author module for `segment` define any name in [`names`]);
//! actually *invoking* the one native algorithm that exists today is [`get_pipelines`](crate::
//! formats_repo::semistructured::get_pipelines)'s own direct, hand-written call into
//! [`pdf_extract::standard_cost_curr`], not a generic dispatch through this registry.
//!
use crate::formats_repo::semistructured::SegmentKind;

pub mod pdf_extract;

/// Whether `name` is registered as a native algorithm for `segment`. Only
/// `"standard_cost_curr"` under [`SegmentKind::PdfExtract`] is `true` today; every other
/// `(segment, name)` pair is `false`, including any name at all under `TextFilter`/`Deserialize`
/// (both have empty registries, per this module's own doc comment).
pub fn contains(segment: SegmentKind, name: &str) -> bool {
    names(segment).contains(&name)
}

/// All native algorithm names registered for `segment`, for
/// [`resolve`](crate::formats_repo::semistructured::resolve)'s Decision-2 collision check against
/// an author module's own top-level names. `&["standard_cost_curr"]` for
/// [`SegmentKind::PdfExtract`]; `&[]` for `TextFilter`/`Deserialize`. Order is unspecified (a
/// single-name set today) — callers must not depend on any particular order.
pub fn names(segment: SegmentKind) -> &'static [&'static str] {
    match segment {
        SegmentKind::PdfExtract => &["standard_cost_curr"],
        SegmentKind::TextFilter => &[],
        SegmentKind::Deserialize => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // contains
    // ============================================================

    #[test]
    fn contains_is_true_for_standard_cost_curr_under_pdf_extract() {
        assert!(contains(SegmentKind::PdfExtract, "standard_cost_curr"));
    }

    #[test]
    fn contains_is_false_for_an_unknown_name_under_pdf_extract() {
        assert!(!contains(SegmentKind::PdfExtract, "not_a_real_algorithm"));
    }

    #[test]
    fn contains_is_false_for_standard_cost_curr_under_a_different_segment() {
        // Native registration is segment-scoped - a pdf_extract-only name is not native for
        // text_filter/deserialize just because the string matches.
        assert!(!contains(SegmentKind::TextFilter, "standard_cost_curr"));
        assert!(!contains(SegmentKind::Deserialize, "standard_cost_curr"));
    }

    #[test]
    fn contains_is_false_for_every_name_under_text_filter_and_deserialize() {
        // text_filter.py/deserialize.py are both entirely empty today - their registries have no
        // entries at all, for any name.
        assert!(!contains(SegmentKind::TextFilter, ""));
        assert!(!contains(SegmentKind::TextFilter, "anything"));
        assert!(!contains(SegmentKind::Deserialize, ""));
        assert!(!contains(SegmentKind::Deserialize, "anything"));
    }

    // ============================================================
    // names
    // ============================================================

    #[test]
    fn names_lists_exactly_standard_cost_curr_for_pdf_extract() {
        assert_eq!(names(SegmentKind::PdfExtract), &["standard_cost_curr"]);
    }

    #[test]
    fn names_is_empty_for_text_filter() {
        assert!(names(SegmentKind::TextFilter).is_empty());
    }

    #[test]
    fn names_is_empty_for_deserialize() {
        assert!(names(SegmentKind::Deserialize).is_empty());
    }

    #[test]
    fn names_and_contains_agree_with_each_other_for_every_segment() {
        for segment in [SegmentKind::PdfExtract, SegmentKind::TextFilter, SegmentKind::Deserialize] {
            for name in names(segment) {
                assert!(contains(segment, name), "names({segment:?}) contains {name:?} but contains() disagrees");
            }
        }
    }
}
