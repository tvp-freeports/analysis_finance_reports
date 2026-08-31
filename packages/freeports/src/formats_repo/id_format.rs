//! The grammar of a formats repository's ids, and how `(format, pipeline, index)` is derived from
//! one.
//!
//! Every structured CSV table is indexed by an `ID` column saying *which pipe of which pipeline of
//! which format* the row configures. The full form is `<format>(<pipeline>)/<index>`, but the
//! pipeline and the index are almost always left out and have to be **derived**.
//!
//! Deriving the index is not a per-row operation: within one `(format, pipeline)` group a row's
//! index depends on all the other rows of that group. [`computed_ids`] is the function that does
//! it, and the one the structured tables actually use.
//!
//! The patterns use Oniguruma rather than the `regex` crate, so that they can be written in the
//! same syntax the repositories already use.

use once_cell::sync::Lazy;
use onig::Regex;
use std::collections::HashMap;
use std::fmt;

/// A format name is any prefix, then a hyphen, two capitals and two digits, with an optional `@XX`
/// market and an optional dotted suffix.
const FORMAT_NAME_PATTERN: &str = r".+\-[A-Z]{2}\d{2}(@[A-Z]{2,3})?(\.[^\.\/]+)?";

/// A pipeline name: lowercase letters, digits and underscores. Note the `*`: the **empty** name is
/// legitimate, and it is the default pipeline's.
const PIPELINE_NAME_PATTERN: &str = r"[0-9a-z_]*";

/// An index: a slash followed by digits.
const INDEX_PATTERN: &str = r"/([0-9]+)";

fn pipeline_pattern() -> String {
    format!(r"\(({PIPELINE_NAME_PATTERN})\)")
}

/// The `(pipeline)` group, wherever it occurs in the string.
static PIPELINE_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(&pipeline_pattern()).expect("pattern fisso e valido"));

/// The optional tail to strip in order to obtain the format name. Anchored only on the right,
/// relying on an unanchored search to find the leftmost position the tail matches from.
static SUFFIX_STRIP_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"({})?({INDEX_PATTERN})?$", pipeline_pattern())).expect("pattern fisso e valido"));

/// An index at the very end of the string.
static INDEX_AT_END_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"{INDEX_PATTERN}$")).expect("pattern fisso e valido"));

static EXPANDABLE_NO_INDEX_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"^{FORMAT_NAME_PATTERN}({})?$", pipeline_pattern())).expect("pattern fisso e valido")
});

static EXPANDABLE_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"^{FORMAT_NAME_PATTERN}({})?({INDEX_PATTERN})?$", pipeline_pattern()))
        .expect("pattern fisso e valido")
});

static COMPLETE_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"^{FORMAT_NAME_PATTERN}{}{INDEX_PATTERN}$", pipeline_pattern()))
        .expect("pattern fisso e valido")
});

/// How strict an id's form must be, depending on which table holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdFormat {
    /// A format name with an optional `(pipeline)` and **no** index.
    ExpandableNoIndex,
    /// A format name with `(pipeline)` and `/index` both optional.
    Expandable,
    /// A format name with `(pipeline)` and `/index` both **required**: the form of a
    /// [`ComputedId`], that is, of an id already derived.
    Complete,
}

/// The kind of relation between a secondary table and the main table of its group.
///
/// It decides both how strict the accepted id form is and how a missing index is derived. The two
/// questions are one enum rather than two because no combination of them occurs that this does not
/// already determine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FkRelation {
    /// One row per pipe: the index is **not** read from the id, it is counted.
    OneToOne,
    /// Zero or one row per pipe: the index is read from the id; when absent, it is counted among
    /// the rows that omit it.
    OneToMaybe,
    /// Several rows per pipe: the index is read from the id; when absent, it is zero.
    OneToMany,
}

impl FkRelation {
    /// The id form this column accepts.
    pub fn id_format(self) -> IdFormat {
        match self {
            FkRelation::OneToOne => IdFormat::ExpandableNoIndex,
            FkRelation::OneToMaybe | FkRelation::OneToMany => IdFormat::Expandable,
        }
    }
}

/// A pipe's complete identity: format, pipeline, index.
///
/// It is the key the structured tables join on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComputedId {
    pub format: String,
    pub pipeline: String,
    pub index: u32,
}

impl fmt::Display for ComputedId {
    /// The canonical string form of a computed id.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})/{}", self.format, self.pipeline, self.index)
    }
}

/// The format name: the id minus its `(pipeline)` and `/index` tail.
pub fn derive_format_name(id: &str) -> String {
    match SUFFIX_STRIP_REGEXP.find(id) {
        Some((start, _end)) => id[..start].to_string(),
        None => id.to_string(),
    }
}

/// The pipeline name declared in the id, or `default` if the id declares none.
///
/// An explicit `()` is an empty name that was **found**, not an absent one, so `default` does not
/// replace it — the difference between a missing value and a present-but-empty one.
pub fn derive_pipeline_name(id: &str, default: Option<&str>) -> Option<String> {
    match PIPELINE_REGEXP.captures(id) {
        Some(caps) => Some(caps.at(1).unwrap_or_default().to_string()),
        None => default.map(str::to_string),
    }
}

/// The index declared at the end of the id, if there is one.
pub fn derive_pipe_index(id: &str) -> Option<u32> {
    INDEX_AT_END_REGEXP.captures(id).and_then(|caps| caps.at(1)).and_then(|digits| digits.parse().ok())
}

/// Whether `id` conforms to the required form.
pub fn id_matches(id: &str, format: IdFormat) -> bool {
    let regexp = match format {
        IdFormat::ExpandableNoIndex => &EXPANDABLE_NO_INDEX_REGEXP,
        IdFormat::Expandable => &EXPANDABLE_REGEXP,
        IdFormat::Complete => &COMPLETE_REGEXP,
    };
    regexp.is_match(id)
}

/// Derives the complete identity of every row of a table from its `ID` column.
///
/// This includes the part no per-row function can express:
///
/// - [`FkRelation::OneToOne`] — the index is the **row's position** within its `(format, pipeline)` group, counting from the top. The id must not carry one.
/// - [`FkRelation::OneToMany`] — the index is read from the id; rows omitting it are all zero, so several rows can share the same [`ComputedId`], which is exactly what "one to many" means.
/// - [`FkRelation::OneToMaybe`] — the index is read from the id; rows omitting it are numbered among themselves, **ignoring** those that have one. A quirk worth knowing, because it means an explicit index and a derived one can collide.
///
/// The result is in input row order, one to one.
pub fn computed_ids(ids: &[&str], pipeline_default: Option<&str>, relation: FkRelation) -> Vec<ComputedId> {
    let bases: Vec<(String, String)> = ids
        .iter()
        .map(|id| {
            (derive_format_name(id), derive_pipeline_name(id, pipeline_default).unwrap_or_default())
        })
        .collect();

    let explicit: Vec<Option<u32>> = match relation {
        FkRelation::OneToOne => vec![None; ids.len()],
        FkRelation::OneToMaybe | FkRelation::OneToMany => ids.iter().map(|id| derive_pipe_index(id)).collect(),
    };

    let mut counters: HashMap<(String, String), u32> = HashMap::new();
    let mut out = Vec::with_capacity(ids.len());
    for (base, explicit) in bases.into_iter().zip(explicit) {
        let index = match (explicit, relation) {
            (Some(index), _) => index,
            (None, FkRelation::OneToMany) => 0,
            // One-to-one counts every row, one-to-maybe only those without an explicit index: in
            // both cases the counter is incremented **only** when it is used.
            (None, _) => {
                let counter = counters.entry(base.clone()).or_insert(0);
                let index = *counter;
                *counter += 1;
                index
            }
        };
        out.push(ComputedId { format: base.0, pipeline: base.1, index });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    mod format_name {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("AMUNDI-EN24", "AMUNDI-EN24"; "bare format name")]
        #[test_case("AMUNDI-EN24(investments)", "AMUNDI-EN24"; "with pipeline")]
        #[test_case("AMUNDI-EN24/3", "AMUNDI-EN24"; "with index")]
        #[test_case("AMUNDI-EN24(investments)/3", "AMUNDI-EN24"; "with both")]
        #[test_case("AMUNDI-EN24()", "AMUNDI-EN24"; "with an empty pipeline group")]
        #[test_case("MEDIOLANUM-IT24@ES", "MEDIOLANUM-IT24@ES"; "with a country suffix")]
        #[test_case("MEDIOLANUM-IT24.b", "MEDIOLANUM-IT24.b"; "with a variant suffix")]
        #[test_case("MEDIOLANUM-IT24@ES.b(x)/1", "MEDIOLANUM-IT24@ES.b"; "with everything at once")]
        fn strips_the_optional_tail(id: &str, expected: &str) {
            assert_eq!(derive_format_name(id), expected);
        }

        #[test]
        fn leaves_a_string_without_any_tail_untouched() {
            assert_eq!(derive_format_name("whatever"), "whatever");
        }
    }

    mod pipeline_name {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn reads_the_declared_pipeline() {
            assert_eq!(derive_pipeline_name("X-EN24(investments)", None), Some("investments".to_string()));
        }

        #[test]
        fn falls_back_to_the_default_when_no_group_is_declared() {
            assert_eq!(derive_pipeline_name("X-EN24", Some("investments")), Some("investments".to_string()));
        }

        #[test]
        fn without_a_group_and_without_a_default_there_is_no_name() {
            assert_eq!(derive_pipeline_name("X-EN24", None), None);
        }

        #[test]
        fn an_explicit_empty_group_wins_over_the_default() {
            // The difference between a missing value and an empty one: `()` is a name that was
            // found, and it means the *format's* default pipeline, not the table's.
            assert_eq!(derive_pipeline_name("X-EN24()", Some("investments")), Some(String::new()));
        }

        #[test]
        fn reads_the_group_even_when_an_index_follows_it() {
            assert_eq!(derive_pipeline_name("X-EN24(manco)/2", None), Some("manco".to_string()));
        }

        #[test]
        fn accepts_digits_and_underscores_in_the_name() {
            assert_eq!(derive_pipeline_name("X-EN24(fund_assets_2)", None), Some("fund_assets_2".to_string()));
        }
    }

    mod pipe_index {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("X-EN24/0", Some(0); "zero")]
        #[test_case("X-EN24/7", Some(7); "single digit")]
        #[test_case("X-EN24/42", Some(42); "several digits")]
        #[test_case("X-EN24(inv)/2", Some(2); "after a pipeline group")]
        #[test_case("X-EN24", None; "absent")]
        #[test_case("X-EN24(inv)", None; "absent with a pipeline group")]
        #[test_case("X-EN24/2/3", Some(3); "only the last one counts")]
        fn reads_the_trailing_index(id: &str, expected: Option<u32>) {
            assert_eq!(derive_pipe_index(id), expected);
        }
    }

    mod shape_validation {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("AMUNDI-EN24", true; "bare name")]
        #[test_case("AMUNDI-EN24(inv)", true; "with pipeline")]
        #[test_case("AMUNDI-EN24()", true; "with an empty pipeline")]
        #[test_case("AMUNDI-EN24/0", false; "an index makes it invalid")]
        #[test_case("AMUNDI-EN24(inv)/0", false; "pipeline plus index is invalid")]
        #[test_case("nonsense", false; "not a format name at all")]
        #[test_case("AMUNDI-EN24(INV)", false; "an uppercase pipeline name is invalid")]
        fn expandable_no_index(id: &str, expected: bool) {
            assert_eq!(id_matches(id, IdFormat::ExpandableNoIndex), expected);
        }

        #[test_case("AMUNDI-EN24", true; "bare name")]
        #[test_case("AMUNDI-EN24(inv)", true; "with pipeline")]
        #[test_case("AMUNDI-EN24/0", true; "with index")]
        #[test_case("AMUNDI-EN24(inv)/0", true; "with both")]
        #[test_case("AMUNDI-EN24/x", false; "a non numeric index is invalid")]
        #[test_case("nonsense", false; "not a format name at all")]
        fn expandable(id: &str, expected: bool) {
            assert_eq!(id_matches(id, IdFormat::Expandable), expected);
        }

        #[test_case("AMUNDI-EN24(inv)/0", true; "both parts present")]
        #[test_case("AMUNDI-EN24()/0", true; "empty pipeline still counts as present")]
        #[test_case("AMUNDI-EN24(inv)", false; "index missing")]
        #[test_case("AMUNDI-EN24/0", false; "pipeline missing")]
        #[test_case("AMUNDI-EN24", false; "both missing")]
        fn complete(id: &str, expected: bool) {
            assert_eq!(id_matches(id, IdFormat::Complete), expected);
        }

        #[test]
        fn a_format_name_needs_the_two_letter_two_digit_country_year_suffix() {
            assert!(!id_matches("AMUNDI", IdFormat::Expandable));
            assert!(!id_matches("AMUNDI-EN2", IdFormat::Expandable));
            assert!(id_matches("AMUNDI-EN24", IdFormat::Expandable));
        }

        #[test]
        fn the_computed_id_of_any_row_always_has_the_complete_shape() {
            let ids = computed_ids(&["AMUNDI-EN24", "AMUNDI-EN24(manco)"], Some("investments"), FkRelation::OneToOne);
            for id in ids {
                assert!(id_matches(&id.to_string(), IdFormat::Complete), "{id} is not complete");
            }
        }
    }

    mod fk_relation {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn one_to_one_forbids_an_explicit_index_in_the_id() {
            assert_eq!(FkRelation::OneToOne.id_format(), IdFormat::ExpandableNoIndex);
        }

        #[test]
        fn the_other_two_relations_allow_one() {
            assert_eq!(FkRelation::OneToMaybe.id_format(), IdFormat::Expandable);
            assert_eq!(FkRelation::OneToMany.id_format(), IdFormat::Expandable);
        }
    }

    mod computed_id_display {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn renders_as_the_reference_computed_id_column() {
            let id = ComputedId { format: "AMUNDI-EN24".to_string(), pipeline: "investments".to_string(), index: 3 };
            assert_eq!(id.to_string(), "AMUNDI-EN24(investments)/3");
        }

        #[test]
        fn an_empty_pipeline_name_renders_as_empty_parentheses() {
            let id = ComputedId { format: "X-EN24".to_string(), pipeline: String::new(), index: 0 };
            assert_eq!(id.to_string(), "X-EN24()/0");
        }
    }

    mod derivation_one_to_one {
        use super::*;
        use pretty_assertions::assert_eq;

        fn ids(rows: &[&str]) -> Vec<String> {
            computed_ids(rows, Some("investments"), FkRelation::OneToOne).iter().map(ComputedId::to_string).collect()
        }

        #[test]
        fn numbers_the_rows_of_each_group_from_zero() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/1", "A-EN24(investments)/2"]
            );
        }

        #[test]
        fn each_format_has_its_own_counter() {
            assert_eq!(
                ids(&["A-EN24", "B-EN24", "A-EN24"]),
                vec!["A-EN24(investments)/0", "B-EN24(investments)/0", "A-EN24(investments)/1"]
            );
        }

        #[test]
        fn each_pipeline_of_a_format_has_its_own_counter() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24(manco)", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(manco)/0", "A-EN24(investments)/1"]
            );
        }

        #[test]
        fn an_index_written_in_the_id_is_ignored_in_this_mode() {
            // The form forbids this upstream; this pins that, were it to arrive, the derivation
            // would still count rather than read it.
            assert_eq!(ids(&["A-EN24/9"]), vec!["A-EN24(investments)/0"]);
        }

        #[test]
        fn an_empty_table_produces_no_identity() {
            assert!(ids(&[]).is_empty());
        }

        #[test]
        fn without_a_default_the_pipeline_name_is_empty() {
            let out = computed_ids(&["A-EN24"], None, FkRelation::OneToOne);
            assert_eq!(out[0].to_string(), "A-EN24()/0");
        }
    }

    mod derivation_one_to_many {
        use super::*;
        use pretty_assertions::assert_eq;

        fn ids(rows: &[&str]) -> Vec<String> {
            computed_ids(rows, Some("investments"), FkRelation::OneToMany).iter().map(ComputedId::to_string).collect()
        }

        #[test]
        fn reads_the_index_written_in_the_id() {
            assert_eq!(ids(&["A-EN24/2"]), vec!["A-EN24(investments)/2"]);
        }

        #[test]
        fn every_row_without_an_index_lands_on_zero() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/0", "A-EN24(investments)/0"]
            );
        }

        #[test]
        fn several_rows_may_legitimately_share_one_identity() {
            // The very meaning of "one to many": several rows configure the same pipe.
            let out = ids(&["A-EN24/1", "A-EN24/1"]);
            assert_eq!(out[0], out[1]);
        }

        #[test]
        fn explicit_and_implicit_indexes_coexist_in_the_same_table() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24/1", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/1", "A-EN24(investments)/0"]
            );
        }
    }

    mod derivation_one_to_maybe {
        use super::*;
        use pretty_assertions::assert_eq;

        fn ids(rows: &[&str]) -> Vec<String> {
            computed_ids(rows, Some("investments"), FkRelation::OneToMaybe).iter().map(ComputedId::to_string).collect()
        }

        #[test]
        fn reads_the_index_written_in_the_id() {
            assert_eq!(ids(&["A-EN24/2"]), vec!["A-EN24(investments)/2"]);
        }

        #[test]
        fn rows_without_an_index_are_numbered_among_themselves() {
            assert_eq!(
                ids(&["A-EN24", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/1"]
            );
        }

        #[test]
        fn a_row_carrying_an_index_does_not_advance_the_counter_of_the_others() {
            // The quirk pinned on purpose: counting happens only among the rows that omit an index,
            // so an explicit index and a derived one can collide — here the third row lands on
            // `/1`, which the second already declared.
            assert_eq!(
                ids(&["A-EN24", "A-EN24/1", "A-EN24"]),
                vec!["A-EN24(investments)/0", "A-EN24(investments)/1", "A-EN24(investments)/1"]
            );
        }

        #[test]
        fn the_counter_is_per_format_and_pipeline() {
            assert_eq!(
                ids(&["A-EN24", "B-EN24", "A-EN24(manco)", "A-EN24"]),
                vec![
                    "A-EN24(investments)/0",
                    "B-EN24(investments)/0",
                    "A-EN24(manco)/0",
                    "A-EN24(investments)/1"
                ]
            );
        }
    }

    mod real_repository_shapes {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_page_classify_table_numbers_its_header_rows_by_explicit_index() {
            // Real rows from a formats repository's page-classify arguments table.
            let out = computed_ids(&["CARNE-EN23/0", "CARNE-EN23/0", "CARNE-EN23/1"], Some(""), FkRelation::OneToMany);
            assert_eq!(
                out.iter().map(ComputedId::to_string).collect::<Vec<_>>(),
                vec!["CARNE-EN23()/0", "CARNE-EN23()/0", "CARNE-EN23()/1"]
            );
        }

        #[test]
        fn the_investments_args_table_numbers_bare_ids_by_position() {
            let out = computed_ids(&["AMUNDI-EN24", "AMUNDI-IT24", "ANIMA-EN23"], Some("investments"), FkRelation::OneToOne);
            assert_eq!(
                out.iter().map(ComputedId::to_string).collect::<Vec<_>>(),
                vec!["AMUNDI-EN24(investments)/0", "AMUNDI-IT24(investments)/0", "ANIMA-EN23(investments)/0"]
            );
        }

        #[test]
        fn an_additional_args_row_with_a_full_id_matches_the_args_row_it_refers_to() {
            let principal = computed_ids(&["AMUNDI-IT24"], Some("investments"), FkRelation::OneToOne);
            let secondary =
                computed_ids(&["AMUNDI-IT24(investments)/0"], Some("investments"), FkRelation::OneToMaybe);
            assert_eq!(principal, secondary);
        }
    }
}
