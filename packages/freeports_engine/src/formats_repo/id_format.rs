//! ID-string regexes and per-row derivation helpers shared across `formats_repo`'s submodules.
//!
//! Rust port of two things that live together in Python but not in the same file:
//! - `FORMAT_NAME_REGEXP`, from
//!   `packages/freeports_core/src/freeports/_internals/formats/repo/metadata.py`.
//! - `pipeline_name_regexp`/`pipeline_regexp`/`index_regexp` and the `add_format_name`/
//!   `add_pipeline_name`/`add_pipe_index`/`check_id_column` family, from
//!   `packages/freeports_core/src/freeports/_internals/formats/repo/algorithms/
//!   pipelines_definition.py`'s "Regular expressions" section.
//!
//! These are **independent Rust copies**, not calls into Python — both Python originals stay
//! exactly as they are (live, still imported by real format-authoring code), per
//! `agent-memory/detect-format-metadata-rust-port-requirements.md`'s explicit constraint. See
//! `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`, Milestone 1 Step 1.1,
//! for the full design context.
//!
//! `onig` (Oniguruma), not the `regex` crate, per this workspace's established convention (see
//! `input/companies_db.rs`, `formats_utils/deserialize/cast.rs`).
//!
//! **Pre-implementation scaffolding note (test-writer phase)**: every item below is a `todo!()`
//! stub — this file's job at this stage is only to give the test suite below a real type/
//! signature surface to compile against (`cargo test --lib` must compile cleanly even though
//! every test currently panics/fails). `implementer` fills these in; per this workspace's TDD
//! discipline, tests are the contract and must not be edited to make them pass.
//!
//! **One implementation-detail choice made here that wasn't fully pinned by the plan**:
//! `derive_pipe_index` mirrors only `add_pipe_index`'s `PipeIndexMode.EXPLICIT` *extraction* step
//! (`df["ID"].str.extract(rf"{index_regexp}$")`) — a single ID string cannot, by itself, produce
//! the `MissingIndexPolicy.INFER` group-`cumcount()` fallback the Python original also supports,
//! since that requires every other row in the same `(Format name, Pipeline name)` group, which a
//! per-row `&str -> Option<u32>` function structurally cannot see. The plan explicitly scopes this
//! function to "per-row derivation... operating on a single `&str` ID", so this file only exposes
//! the explicit-suffix extraction; any group-level `INFER`/`ZERO` fallback policy is
//! `formats_mapping.rs`'s (Milestone 2) responsibility to apply across rows, not this function's.
//! Flagged for confirmation before Milestone 2 relies on it.

use once_cell::sync::Lazy;
use onig::Regex;

/// Raw pattern text backing [`FORMAT_NAME_REGEXP`], kept as a plain `&str` so the anchored
/// composites below (`EXPANDABLE_NO_INDEX_REGEXP` & co.) can splice it into a larger pattern
/// string the same way `pipelines_definition.py`'s f-strings splice `FORMAT_NAME_REGEXP` in.
const FORMAT_NAME_PATTERN: &str = r".+\-[A-Z]{2}\d{2}(@[A-Z]{2,3})?(\.[^\.\/]+)?";

/// Raw pattern text backing [`PIPELINE_NAME_REGEXP`].
const PIPELINE_NAME_PATTERN: &str = r"[0-9a-z_]*";

/// Raw pattern text backing [`INDEX_REGEXP`].
const INDEX_PATTERN: &str = r"/([0-9]+)";

/// Raw pattern text backing [`PIPELINE_REGEXP`], built from [`PIPELINE_NAME_PATTERN`] exactly like
/// `pipelines_definition.py`'s `pipeline_regexp: str = rf"\(({pipeline_name_regexp})\)"`.
fn pipeline_pattern() -> String {
    format!(r"\(({PIPELINE_NAME_PATTERN})\)")
}

/// Mirrors `metadata.py`'s `FORMAT_NAME_REGEXP` constant, same pattern string
/// (`r".+\-[A-Z]{2}\d{2}(@[A-Z]{2,3})?(\.[^\.\/]+)?"`). Matches a *fragment* — callers that need a
/// whole-string check (like `metadata.py`'s own `formats_schema` index check, or this module's own
/// `id_matches_*` helpers) must anchor it themselves, exactly as every Python consumer already
/// does (`f"^{FORMAT_NAME_REGEXP}$"`, `check_id_column`'s `f"^{reg}$"`) — the constant itself is
/// never anchored, in either language.
pub static FORMAT_NAME_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(FORMAT_NAME_PATTERN).unwrap());

/// Mirrors `pipelines_definition.py`'s `pipeline_name_regexp: str = r"[0-9a-z_]*"`. Note the `*`
/// (not `+`): an empty pipeline name is valid input to this pattern (though not necessarily to a
/// full ID — see `id_matches_*`).
pub static PIPELINE_NAME_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(PIPELINE_NAME_PATTERN).unwrap());

/// Mirrors `pipelines_definition.py`'s `pipeline_regexp: str = rf"\(({pipeline_name_regexp})\)"`.
pub static PIPELINE_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(&pipeline_pattern()).unwrap());

/// Mirrors `pipelines_definition.py`'s `index_regexp: str = r"/([0-9]+)"`.
pub static INDEX_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(INDEX_PATTERN).unwrap());

/// Mirrors `add_format_name`'s `rf"({pipeline_regexp})?({index_regexp})?$"` replacement pattern:
/// an optional trailing `(pipeline)` group followed by an optional trailing `/index` group,
/// anchored to the end of the string (but not the start — same as the Python original, which
/// relies on `str.replace`'s unanchored search finding the leftmost position from which this
/// end-anchored suffix matches).
static SUFFIX_STRIP_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"({})?({INDEX_PATTERN})?$", pipeline_pattern())).unwrap());

/// Mirrors `add_pipe_index`'s `PipeIndexMode.EXPLICIT` extraction pattern, `rf"{index_regexp}$"`.
static INDEX_AT_END_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"{INDEX_PATTERN}$")).unwrap());

/// Mirrors `check_id_column(IDFormat.EXPANDIBLE_NO_INDEX)`'s `f"^{reg}$"`, where
/// `reg = rf"{FORMAT_NAME_REGEXP}({pipeline_regexp})?"`.
static EXPANDABLE_NO_INDEX_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(r"^{FORMAT_NAME_PATTERN}({})?$", pipeline_pattern())).unwrap()
});

/// Mirrors `check_id_column(IDFormat.EXPANDIBLE)`'s `f"^{reg}$"`, where
/// `reg = rf"{FORMAT_NAME_REGEXP}({pipeline_regexp})?({index_regexp})?"`.
static EXPANDABLE_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"^{FORMAT_NAME_PATTERN}({})?({INDEX_PATTERN})?$",
        pipeline_pattern()
    ))
    .unwrap()
});

/// Mirrors `check_id_column(IDFormat.COMPLETE)`'s `f"^{reg}$"`, where
/// `reg = rf"{FORMAT_NAME_REGEXP}{pipeline_regexp}{index_regexp}"` (both parts mandatory).
static COMPLETE_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"^{FORMAT_NAME_PATTERN}{}{INDEX_PATTERN}$",
        pipeline_pattern()
    ))
    .unwrap()
});

/// Mirrors `add_format_name`: strips a trailing `(pipeline)` and/or `/index` suffix off an `ID`
/// string, returning the bare format name. Equivalent to Python's
/// `id.replace(rf"({pipeline_regexp})?({index_regexp})?$", "")` applied to a single string (the
/// Rust port operates row-by-row rather than on a whole `pd.Series`, per the plan).
pub fn derive_format_name(id: &str) -> String {
    match SUFFIX_STRIP_REGEXP.find(id) {
        Some((start, _end)) => id[..start].to_string(),
        None => id.to_string(),
    }
}

/// Mirrors `add_pipeline_name`: extracts the pipeline name from the first `(...)` group found
/// anywhere in `id` (unanchored search, exactly like `pd.Series.str.extract` — the pipeline group
/// is not required to be at the end of the string). Returns `None` when no `(...)` group matching
/// `PIPELINE_NAME_REGEXP` is found *and* `default` is `None` (mirrors a `NaN` cell with no
/// `fillna`); returns `Some(default)` when no group is found but a `default` is given; returns
/// `Some(extracted)` — **including `Some(String::new())` for an explicit empty `()`** — whenever a
/// group is actually found, regardless of `default` (mirrors `fillna` only ever touching genuine
/// `NaN`, never a real empty-string match).
pub fn derive_pipeline_name(id: &str, default: Option<&str>) -> Option<String> {
    match PIPELINE_REGEXP.captures(id) {
        Some(caps) => Some(caps.at(1).unwrap_or_default().to_string()),
        None => default.map(str::to_string),
    }
}

/// Per-row explicit index extraction (see this file's module doc for why this is *not* a full
/// port of `add_pipe_index`). Mirrors the `PipeIndexMode.EXPLICIT` extraction step only: returns
/// `Some(n)` when `id` ends in a literal `/<digits>` suffix, `None` otherwise.
pub fn derive_pipe_index(id: &str) -> Option<u32> {
    INDEX_AT_END_REGEXP
        .captures(id)
        .and_then(|caps| caps.at(1))
        .and_then(|digits| digits.parse().ok())
}

/// Mirrors `check_id_column(IDFormat.EXPANDIBLE_NO_INDEX)`: `id` is a format name with an
/// *optional* `(pipeline)` group and **no** index suffix at all (a trailing `/<digits>` makes this
/// `false`, even though the pipeline part is optional).
pub fn id_matches_expandable_no_index(id: &str) -> bool {
    EXPANDABLE_NO_INDEX_REGEXP.is_match(id)
}

/// Mirrors `check_id_column(IDFormat.EXPANDIBLE)`: `id` is a format name with an optional
/// `(pipeline)` group and an optional `/index` suffix (any combination of present/absent is
/// valid; if a pipeline group is present at all, its content must fully conform to
/// `PIPELINE_NAME_REGEXP`).
pub fn id_matches_expandable(id: &str) -> bool {
    EXPANDABLE_REGEXP.is_match(id)
}

/// Mirrors `check_id_column(IDFormat.COMPLETE)`: `id` is a format name with a **required**
/// `(pipeline)` group (possibly empty, i.e. `()`) and a **required** `/index` suffix.
pub fn id_matches_complete(id: &str) -> bool {
    COMPLETE_REGEXP.is_match(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    /// `onig::Regex::find`/`::is_match` search unanchored (leftmost match anywhere), matching
    /// `require_regex_matches_name`'s documented behavior elsewhere in this crate — a "does this
    /// whole string have this shape" check needs the match span to cover the entire input, which
    /// none of the bare pattern constants above guarantee on their own (they're fragments meant to
    /// be composed/anchored by callers, exactly like their Python counterparts).
    fn matches_whole(re: &Regex, s: &str) -> bool {
        re.find(s).is_some_and(|(start, end)| start == 0 && end == s.len())
    }

    // ============================================================
    // FORMAT_NAME_REGEXP
    // ============================================================

    #[test]
    fn format_name_regexp_matches_a_plain_name_without_country_or_version() {
        assert!(matches_whole(&FORMAT_NAME_REGEXP, "AMUNDI-EN24"));
    }

    #[test]
    fn format_name_regexp_matches_a_name_with_country() {
        // Real format name from analysis_finance_reports_formats/metadata/formats.csv
        // (FINECO, EN, 23, Country=IR).
        assert!(matches_whole(&FORMAT_NAME_REGEXP, "FINECO-EN23@IR"));
    }

    #[test]
    fn format_name_regexp_matches_a_name_with_version() {
        // Real format name (MEDIOLANUM, ES, 24, Version=B).
        assert!(matches_whole(&FORMAT_NAME_REGEXP, "MEDIOLANUM-ES24.B"));
    }

    #[test]
    fn format_name_regexp_matches_a_name_with_both_country_and_version() {
        // Taken verbatim from metadata.py's own get_formats() docstring example.
        assert!(matches_whole(&FORMAT_NAME_REGEXP, "Eurizon-IT24@IT.v2"));
    }

    #[test]
    fn format_name_regexp_rejects_a_name_missing_the_locale_year_suffix() {
        assert!(!matches_whole(&FORMAT_NAME_REGEXP, "AMUNDI"));
    }

    #[test]
    fn format_name_regexp_rejects_a_lowercase_locale() {
        assert!(!matches_whole(&FORMAT_NAME_REGEXP, "AMUNDI-en24"));
    }

    #[test]
    fn format_name_regexp_rejects_a_single_digit_year() {
        assert!(!matches_whole(&FORMAT_NAME_REGEXP, "AMUNDI-EN2"));
    }

    #[test]
    fn format_name_regexp_rejects_a_country_code_longer_than_three_letters() {
        // The country group is capped at {2,3} letters; "ITALY" (5 letters) can't fit, so the
        // greedy match stops after 3 ("ITA"), leaving "LY" unconsumed - not a whole-string match.
        assert!(!matches_whole(&FORMAT_NAME_REGEXP, "AMUNDI-EN24@ITALY"));
    }

    #[test]
    fn format_name_regexp_rejects_a_version_containing_a_second_dot() {
        // The version group excludes '.' from its own character class, so it can only ever
        // capture one dot-free segment - a second dot can't be absorbed into the same version.
        assert!(!matches_whole(&FORMAT_NAME_REGEXP, "AMUNDI-EN24.v1.v2"));
    }

    #[test]
    fn format_name_regexp_rejects_the_empty_string() {
        assert!(!matches_whole(&FORMAT_NAME_REGEXP, ""));
    }

    // ============================================================
    // PIPELINE_NAME_REGEXP
    // ============================================================

    #[test]
    fn pipeline_name_regexp_matches_the_empty_string() {
        // `[0-9a-z_]*` (a star, not a plus) - zero-length is a valid match.
        assert!(matches_whole(&PIPELINE_NAME_REGEXP, ""));
    }

    #[test]
    fn pipeline_name_regexp_matches_a_lowercase_snake_case_name() {
        assert!(matches_whole(&PIPELINE_NAME_REGEXP, "sfdr_classification_2"));
    }

    #[test]
    fn pipeline_name_regexp_rejects_uppercase_letters() {
        assert!(!matches_whole(&PIPELINE_NAME_REGEXP, "Investments"));
    }

    #[test]
    fn pipeline_name_regexp_rejects_a_hyphen() {
        assert!(!matches_whole(&PIPELINE_NAME_REGEXP, "sfdr-classification"));
    }

    // ============================================================
    // PIPELINE_REGEXP
    // ============================================================

    #[test]
    fn pipeline_regexp_matches_a_named_pipeline_in_parens() {
        assert!(matches_whole(&PIPELINE_REGEXP, "(renaming)"));
    }

    #[test]
    fn pipeline_regexp_matches_empty_parens() {
        assert!(matches_whole(&PIPELINE_REGEXP, "()"));
    }

    #[test]
    fn pipeline_regexp_captures_the_pipeline_name() {
        let caps = PIPELINE_REGEXP.captures("(fund_assets)").expect("should match");
        assert_eq!(caps.at(1), Some("fund_assets"));
    }

    #[test]
    fn pipeline_regexp_captures_an_empty_name_from_empty_parens() {
        let caps = PIPELINE_REGEXP.captures("()").expect("should match");
        assert_eq!(caps.at(1), Some(""));
    }

    #[test]
    fn pipeline_regexp_does_not_match_without_parens() {
        assert!(PIPELINE_REGEXP.find("renaming").is_none());
    }

    #[test]
    fn pipeline_regexp_does_not_match_uppercase_content() {
        assert!(PIPELINE_REGEXP.find("(Renaming)").is_none());
    }

    // ============================================================
    // INDEX_REGEXP
    // ============================================================

    #[test]
    fn index_regexp_matches_a_single_digit_index() {
        assert!(matches_whole(&INDEX_REGEXP, "/0"));
    }

    #[test]
    fn index_regexp_matches_a_multi_digit_index() {
        assert!(matches_whole(&INDEX_REGEXP, "/12"));
    }

    #[test]
    fn index_regexp_captures_the_index_value() {
        let caps = INDEX_REGEXP.captures("/12").expect("should match");
        assert_eq!(caps.at(1), Some("12"));
    }

    #[test]
    fn index_regexp_does_not_match_without_a_leading_slash() {
        assert!(INDEX_REGEXP.find("0").is_none());
    }

    #[test]
    fn index_regexp_does_not_match_a_non_digit_suffix() {
        assert!(INDEX_REGEXP.find("/abc").is_none());
    }

    #[test]
    fn index_regexp_finds_the_suffix_embedded_in_a_full_id() {
        // "CARNE-EN23" is 10 bytes, so the "/0" suffix starts at offset 10.
        let found = INDEX_REGEXP.find("CARNE-EN23/0");
        assert_eq!(found, Some((10, 12)));
    }

    // ============================================================
    // derive_format_name
    // ============================================================

    #[test]
    fn derive_format_name_strips_a_trailing_pipeline_group() {
        // Real ID from analysis_finance_reports_formats/content/orchestration/mapping.csv.
        assert_eq!(derive_format_name("EURIZON-EN23(renaming)"), "EURIZON-EN23");
    }

    #[test]
    fn derive_format_name_strips_a_trailing_pipeline_and_index() {
        // Real ID from .../content/algorithms/structured/investments/additional_args.csv.
        assert_eq!(derive_format_name("AMUNDI-IT24(investments)/0"), "AMUNDI-IT24");
    }

    #[test]
    fn derive_format_name_strips_a_trailing_index_only() {
        // Real ID from the same additional_args.csv (no pipeline group at all).
        assert_eq!(derive_format_name("ASTERIA-EN24/0"), "ASTERIA-EN24");
    }

    #[test]
    fn derive_format_name_leaves_a_bare_format_name_untouched() {
        assert_eq!(derive_format_name("CARNE-EN23"), "CARNE-EN23");
    }

    #[test]
    fn derive_format_name_handles_a_format_name_with_country_and_index() {
        // Real ID from .../content/algorithms/structured/page_classify/args.csv.
        assert_eq!(derive_format_name("FINECO-EN23@IR/0"), "FINECO-EN23@IR");
    }

    #[test]
    fn derive_format_name_handles_a_format_name_with_version_and_pipeline() {
        // Real ID from .../content/orchestration/mapping.csv.
        assert_eq!(derive_format_name("MEDIOLANUM-ES24.B(subfund)"), "MEDIOLANUM-ES24.B");
    }

    // ============================================================
    // derive_pipeline_name
    // ============================================================

    #[test]
    fn derive_pipeline_name_extracts_the_name_from_parens() {
        assert_eq!(derive_pipeline_name("EURIZON-EN23(renaming)", None), Some("renaming".to_string()));
    }

    #[test]
    fn derive_pipeline_name_extracts_the_name_when_an_index_follows() {
        assert_eq!(
            derive_pipeline_name("AMUNDI-IT24(investments)/0", None),
            Some("investments".to_string())
        );
    }

    #[test]
    fn derive_pipeline_name_returns_none_when_absent_and_no_default_given() {
        assert_eq!(derive_pipeline_name("ASTERIA-EN24/0", None), None);
    }

    #[test]
    fn derive_pipeline_name_falls_back_to_the_default_when_absent() {
        assert_eq!(
            derive_pipeline_name("ASTERIA-EN24/0", Some("default_pipe")),
            Some("default_pipe".to_string())
        );
    }

    #[test]
    fn derive_pipeline_name_falls_back_to_the_default_for_a_bare_format_name() {
        assert_eq!(derive_pipeline_name("CARNE-EN23", Some("main")), Some("main".to_string()));
    }

    #[test]
    fn derive_pipeline_name_returns_an_empty_string_for_empty_parens_even_with_a_default() {
        // Pandas' `fillna` only ever replaces a genuine NaN cell - an explicit empty match (`()`)
        // is a real (empty) value, so a `default` must NOT override it.
        assert_eq!(
            derive_pipeline_name("FOO-EN24()", Some("fallback")),
            Some(String::new())
        );
    }

    #[test]
    fn derive_pipeline_name_returns_none_for_uppercase_content_in_parens() {
        // PIPELINE_NAME_REGEXP is lowercase-only ([0-9a-z_]*): "(Investments)" starts with an
        // uppercase letter, so `[0-9a-z_]*` matches zero characters right after "(" and then
        // fails to find the required ")" immediately after - no match anywhere in the string,
        // exactly mirroring pandas' `str.extract` returning NaN here.
        assert_eq!(derive_pipeline_name("FOO-EN24(Investments)", None), None);
    }

    // ============================================================
    // derive_pipe_index
    // ============================================================

    #[test]
    fn derive_pipe_index_extracts_a_single_digit_index() {
        assert_eq!(derive_pipe_index("AMUNDI-IT24(investments)/0"), Some(0));
    }

    #[test]
    fn derive_pipe_index_extracts_a_multi_digit_index() {
        assert_eq!(derive_pipe_index("CARNE-EN23/12"), Some(12));
    }

    #[test]
    fn derive_pipe_index_returns_none_when_no_index_suffix_is_present() {
        assert_eq!(derive_pipe_index("EURIZON-EN23(renaming)"), None);
    }

    #[test]
    fn derive_pipe_index_returns_none_for_a_bare_format_name() {
        assert_eq!(derive_pipe_index("CARNE-EN23"), None);
    }

    // ============================================================
    // id_matches_expandable_no_index
    // ============================================================

    #[test_case("CARNE-EN23", true; "bare format name")]
    #[test_case("EURIZON-EN23(renaming)", true; "format name with a pipeline")]
    #[test_case("AMUNDI-IT24(investments)/0", false; "index present is rejected")]
    #[test_case("ASTERIA-EN24/0", false; "index-only is rejected")]
    #[test_case("not-a-format", false; "not a valid format name at all")]
    fn id_matches_expandable_no_index_cases(id: &str, expected: bool) {
        assert_eq!(id_matches_expandable_no_index(id), expected);
    }

    // ============================================================
    // id_matches_expandable
    // ============================================================

    #[test_case("CARNE-EN23", true; "bare format name")]
    #[test_case("EURIZON-EN23(renaming)", true; "pipeline only")]
    #[test_case("ASTERIA-EN24/0", true; "index only")]
    #[test_case("AMUNDI-IT24(investments)/0", true; "pipeline and index")]
    #[test_case("CARNE-EN23(Investments)", false; "malformed uppercase pipeline content")]
    #[test_case("not-a-format", false; "not a valid format name at all")]
    fn id_matches_expandable_cases(id: &str, expected: bool) {
        assert_eq!(id_matches_expandable(id), expected);
    }

    // ============================================================
    // id_matches_complete
    // ============================================================

    #[test_case("AMUNDI-IT24(investments)/0", true; "pipeline and index both present")]
    #[test_case("FOO-EN24()/5", true; "an explicitly empty pipeline name still counts as present")]
    #[test_case("EURIZON-EN23(renaming)", false; "missing the required index")]
    #[test_case("ASTERIA-EN24/0", false; "missing the required pipeline")]
    #[test_case("CARNE-EN23", false; "missing both pipeline and index")]
    fn id_matches_complete_cases(id: &str, expected: bool) {
        assert_eq!(id_matches_complete(id), expected);
    }
}
