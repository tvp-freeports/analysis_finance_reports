//! The load-time validation of a repository's document classification (§5 of
//! `agent-memory/SPEC-document-classification.md`, phase 9 of the plan).
//!
//! What the loaders cannot see by themselves: [`super::doc_classify_yaml`] finds the defects of
//! **form** of the YAML from the file alone; this module checks the YAML against `formats.csv` and
//! against the presence of an author's Python, which is why it runs where the three meet,
//! [`super::doc_classify::load`].
//!
//! Two kinds of defect, both errors:
//!
//! - **a name that cannot exist** (error 1): a step with criteria, or unconditional, whose pattern
//!   is complete and not declared, or partial and refined by no declared format; a `*` template no
//!   entity of `formats.csv` instantiates into a declared format or a partial with a non-empty
//!   expansion. Prefix-only steps are never an outcome, and need not exist;
//! - **a step no document reaches** (errors 4 and 5): one pass in the visit order of §3, carrying
//!   the set `𝒜` of the values the accumulated partial can have there. Candidability is the
//!   engine's own ([`crate::core::doc_classify::is_candidable`],
//!   [`crate::core::doc_classify::template_instance`]), not a copy. `*` steps are never reported
//!   as unreachable (user's decision, 2026-10-11). With a Python present `𝒜` starts from
//!   nothing accumulated plus every partial pattern that expands to some declared format — all
//!   the Python can return without failing its run-time check — so the error is never false. An
//!   unconditional step removes values from `𝒜` only when its entry is on page `1` or `-1`, the
//!   only pages every document has.
//!
//! Every defect found is reported, in file order — entries as listed, steps in depth-first
//! preorder — never in the order of a hash map.

use std::num::NonZeroI32;

use crate::core::doc_classify::{
    PythonEntry, Step, StepKey, YamlEntry, is_candidable, template_instance,
};
use crate::formats_repo::doc_classify_yaml::YamlLocation;
use crate::formats_repo::format_pattern::{FormatPattern, FormatTemplate, Slot};

/// What a step breaks (§5).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationRule {
    /// Error 1: a complete name not declared in `formats.csv` (§1.3: also when it only exists with
    /// variants — the partial is written `.*`).
    #[error("'{format}' is not a format of formats.csv")]
    UnknownFormat { format: FormatPattern },
    /// Error 1: a partial no format of `formats.csv` refines.
    #[error("no format of formats.csv refines the partial '{pattern}'")]
    PartialMatchesNoFormat { pattern: FormatPattern },
    /// Error 1 for a `*` template (§1.4, R8): no entity of `formats.csv` completes it.
    #[error("no entity of formats.csv completes the template '{template}'")]
    TemplateMatchesNoFormat { template: FormatTemplate },
    /// Error 4: no value the accumulated partial can have at this step makes it candidable.
    #[error("no document can reach this step")]
    Unreachable,
    /// Error 5: a top-level step candidable only with nothing accumulated, after a top-level
    /// unconditional complete step — `step`, on `page` — that decides every document still
    /// unclassified when it is visited.
    #[error(
        "no document can reach this step: the unconditional step '{step}' on page {page} decides every document that gets there first"
    )]
    UnreachableAfterUnconditional {
        step: FormatPattern,
        page: NonZeroI32,
    },
}

/// One defect: where, and what.
///
/// `at.page` is the entry's page as written; `at.steps` the enclosing steps' full patterns,
/// outermost first; `at.key` the step's own **full** pattern (or its template, `*-EN23`) — the
/// written suffix is gone after normalization.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{at}: {kind}")]
pub struct ValidationIssue {
    pub at: YamlLocation,
    pub kind: ValidationRule,
}

/// Every defect of a document classification, in file order.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", list(.issues))]
pub struct DocClassifyValidationError {
    pub issues: Vec<ValidationIssue>,
}

fn list(issues: &[ValidationIssue]) -> String {
    issues
        .iter()
        .map(ValidationIssue::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Validates the YAML entries of a repository against its `formats` (in `formats.csv` order) and
/// its Python entries: only whether there are any matters — with some, `𝒜` starts with every
/// partial that expands to a declared format besides nothing accumulated (§5).
pub fn validate(
    formats: &[String],
    python: &[PythonEntry],
    yaml: &[YamlEntry],
) -> Result<(), DocClassifyValidationError> {
    let mut accumulable = vec![None];
    if !python.is_empty() {
        accumulable.extend(every_partial_refined_by(formats).into_iter().map(Some));
    }
    let mut walk = Walk {
        formats,
        accumulable,
        nothing_decided_by: None,
        issues: Vec::new(),
    };
    for entry in yaml {
        let on_page = YamlLocation {
            page: Some(entry.page.get().into()),
            ..Default::default()
        };
        for step in &entry.steps {
            walk.step(step, entry.page, &on_page, None, &[]);
        }
    }
    match walk.issues.is_empty() {
        true => Ok(()),
        false => Err(DocClassifyValidationError {
            issues: walk.issues,
        }),
    }
}

/// Every partial pattern some format of `formats` refines — its truncations, and its forms with
/// `@*`/`.*` in place of a written location or variant: all the Python can return without failing
/// its run-time check.
fn every_partial_refined_by(formats: &[String]) -> Vec<FormatPattern> {
    let mut partials: Vec<FormatPattern> = Vec::new();
    for format in formats
        .iter()
        .filter_map(|format| format.parse::<FormatPattern>().ok())
    {
        let (Slot::Fixed(language), Slot::Fixed(year)) = (format.language(), format.year()) else {
            continue;
        };
        let entity = format.entity();
        let mut written = vec![entity.to_string(), format!("{entity}-{language}")];
        for location in written_or_open(format.location(), '@') {
            for variant in written_or_open(format.variant(), '.') {
                written.push(format!("{entity}-{language}{year}{location}{variant}"));
            }
        }
        // An entity a partial cannot hold (`FOO-BAR`) would read back as another pattern.
        let readable = written
            .iter()
            .filter_map(|text| text.parse::<FormatPattern>().ok());
        for partial in
            readable.filter(|pattern| !pattern.is_complete() && pattern.entity() == entity)
        {
            if !partials.contains(&partial) {
                partials.push(partial);
            }
        }
    }
    partials
}

/// An optional slot as a partial may write it: a written value, or `*` in its place.
fn written_or_open(slot: &Slot, separator: char) -> Vec<String> {
    match slot {
        Slot::Fixed(value) => vec![format!("{separator}{value}"), format!("{separator}*")],
        _ => vec![String::new()],
    }
}

/// The one pass of §5 in the visit order of §3.
struct Walk<'a> {
    formats: &'a [String],
    /// `𝒜`: every value the accumulated partial can have at the step being visited, `None` for
    /// nothing accumulated.
    accumulable: Vec<Option<FormatPattern>>,
    /// The top-level unconditional complete step that took "nothing accumulated" out of `𝒜`, and
    /// its page: what error 5 names.
    nothing_decided_by: Option<(FormatPattern, NonZeroI32)>,
    issues: Vec<ValidationIssue>,
}

impl Walk<'_> {
    /// One step, then its children. `context` is the root of rule 3.1.2 as the engine computes it,
    /// `guards` the ancestors with criteria of their own, outermost first (rule 3.1.4).
    fn step(
        &mut self,
        step: &Step,
        page: NonZeroI32,
        enclosing: &YamlLocation,
        context: Option<&FormatPattern>,
        guards: &[&FormatPattern],
    ) {
        let at = YamlLocation {
            key: Some(step.key.to_string()),
            ..enclosing.clone()
        };
        let pattern = match &step.key {
            // A template is top level and has no children (§1.4).
            StepKey::Template(template) => return self.template(template, step, page, at),
            StepKey::Pattern(pattern) => pattern,
        };
        if !step.is_prefix() {
            if pattern.expand(self.formats).is_empty() {
                let kind = match pattern.is_complete() {
                    true => ValidationRule::UnknownFormat {
                        format: pattern.clone(),
                    },
                    false => ValidationRule::PartialMatchesNoFormat {
                        pattern: pattern.clone(),
                    },
                };
                self.issues.push(ValidationIssue {
                    at: at.clone(),
                    kind,
                });
            }
            let had_nothing = self.accumulable.contains(&None);
            let reached = self.evaluate(
                |accumulated| is_candidable(pattern, context, accumulated).then(|| pattern.clone()),
                step,
                page,
                guards,
            );
            if !reached {
                let kind = match (&self.nothing_decided_by, context) {
                    (Some((decider, decided_on)), None) => {
                        ValidationRule::UnreachableAfterUnconditional {
                            step: decider.clone(),
                            page: *decided_on,
                        }
                    }
                    _ => ValidationRule::Unreachable,
                };
                self.issues.push(ValidationIssue {
                    at: at.clone(),
                    kind,
                });
            } else if had_nothing && !self.accumulable.contains(&None) && pattern.is_complete() {
                self.nothing_decided_by = Some((pattern.clone(), page));
            }
        }
        let root = if step.criteria.is_empty() {
            context.unwrap_or(pattern)
        } else {
            pattern
        };
        let mut inner_guards = guards.to_vec();
        if !step.criteria.is_empty() {
            inner_guards.push(pattern);
        }
        let mut within = enclosing.clone();
        within.steps.push(step.key.to_string());
        for child in &step.children {
            self.step(child, page, &within, Some(root), &inner_guards);
        }
    }

    /// A `*` step: error 1 if no entity of the repository completes it, never error 4 (user's
    /// decision, 2026-10-11) — for an accumulated partial it cannot complete it simply does not
    /// start.
    fn template(
        &mut self,
        template: &FormatTemplate,
        step: &Step,
        page: NonZeroI32,
        at: YamlLocation,
    ) {
        let formats = self.formats;
        let completed = formats
            .iter()
            .filter_map(|format| format.parse::<FormatPattern>().ok())
            .any(|format| template_instance(template, Some(&format), formats).is_some());
        if !completed {
            self.issues.push(ValidationIssue {
                at,
                kind: ValidationRule::TemplateMatchesNoFormat {
                    template: template.clone(),
                },
            });
        }
        self.evaluate(
            |accumulated| {
                template_instance(template, accumulated, formats)
                    .filter(|instance| is_candidable(instance, None, accumulated))
            },
            step,
            page,
            &[],
        );
    }

    /// Updates `𝒜` for a step that is not a prefix, `candidate` giving, for one value of `𝒜`, the
    /// pattern the step is evaluated with if it is candidable at all. Returns whether some value
    /// makes it candidable.
    ///
    /// A reachable partial adds itself (each of its instances, for a `*`). An unconditional step
    /// takes out the values it is candidable for and no guard can stop — complete, it decides
    /// them; partial, it replaces them with itself — but only on page `1` or `-1`: on any other
    /// page the document may be shorter, and then the entry is nothing.
    fn evaluate(
        &mut self,
        candidate: impl Fn(Option<&FormatPattern>) -> Option<FormatPattern>,
        step: &Step,
        page: NonZeroI32,
        guards: &[&FormatPattern],
    ) -> bool {
        let decides = step.criteria.is_empty() && page.get().abs() == 1;
        let mut reached = false;
        let mut kept = Vec::with_capacity(self.accumulable.len());
        let mut added = Vec::new();
        for accumulated in self.accumulable.drain(..) {
            let Some(pattern) = candidate(accumulated.as_ref()) else {
                kept.push(accumulated);
                continue;
            };
            reached = true;
            let unstoppable = guards.iter().all(|guard| {
                accumulated
                    .as_ref()
                    .is_some_and(|found| found.refines(guard))
            });
            if !(decides && unstoppable) {
                kept.push(accumulated);
            }
            if !pattern.is_complete() {
                added.push(Some(pattern));
            }
        }
        for value in added {
            if !kept.contains(&value) {
                kept.push(value);
            }
        }
        self.accumulable = kept;
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::core::pipeline::Pipeline;
    use crate::formats_repo::doc_classify_yaml::parse;

    // ------------------------------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------------------------------

    /// The engine tests' repository plus FINECO: entities with one format, with several, with
    /// locations and with variants.
    const FORMATS: &[&str] = &[
        "AMUNDI-EN24",
        "ASTERIA-EN23",
        "ASTERIA-EN24",
        "EURIZON-EN23",
        "FIDEURAM-IT21",
        "FIDEURAM-IT23@LUX",
        "FINECO-EN23@IR",
        "FINECO-EN23@LUX",
        "MEDIOLANUM-EN23",
        "MEDIOLANUM-ES21",
        "MEDIOLANUM-ES24.A",
        "MEDIOLANUM-ES24.B",
        "MEDIOLANUM-IT23",
        "MEDIOLANUM-IT24.A",
        "MEDIOLANUM-IT24.B",
    ];

    fn formats() -> Vec<String> {
        FORMATS.iter().map(|f| f.to_string()).collect()
    }

    fn formats_without(excluded: &str) -> Vec<String> {
        FORMATS
            .iter()
            .filter(|f| **f != excluded)
            .map(|f| f.to_string())
            .collect()
    }

    fn n(number: i32) -> NonZeroI32 {
        NonZeroI32::new(number).expect("test page numbers are never zero")
    }

    fn p(text: &str) -> FormatPattern {
        text.parse().expect("fixture pattern must be valid")
    }

    fn t(text: &str) -> FormatTemplate {
        text.parse().expect("fixture template must be valid")
    }

    /// The YAML, which must have no defect of form: what is tested here is what only
    /// `formats.csv` and the Python can tell.
    fn yaml(text: &str) -> Vec<YamlEntry> {
        match parse(text) {
            Ok(entries) => entries,
            Err(e) => panic!("fixture YAML must parse, got: {e}\n---\n{text}"),
        }
    }

    /// One Python entry: its content is never run by the validation, only its presence counts.
    fn python() -> Vec<PythonEntry> {
        vec![PythonEntry {
            page: n(1),
            pipeline: Pipeline::new("author"),
        }]
    }

    fn issues_in(formats: &[String], python: &[PythonEntry], text: &str) -> Vec<ValidationIssue> {
        match validate(formats, python, &yaml(text)) {
            Ok(()) => panic!("expected the validation to fail\n---\n{text}"),
            Err(error) => error.issues,
        }
    }

    /// The issues of `text` against [`FORMATS`], without Python.
    fn issues(text: &str) -> Vec<ValidationIssue> {
        issues_in(&formats(), &[], text)
    }

    fn issues_with_python(text: &str) -> Vec<ValidationIssue> {
        issues_in(&formats(), &python(), text)
    }

    fn assert_valid_in(formats: &[String], python: &[PythonEntry], text: &str) {
        if let Err(error) = validate(formats, python, &yaml(text)) {
            panic!("expected the YAML to be valid, got:\n{error}\n---\n{text}");
        }
    }

    fn assert_valid(text: &str) {
        assert_valid_in(&formats(), &[], text)
    }

    fn assert_valid_with_python(text: &str) {
        assert_valid_in(&formats(), &python(), text)
    }

    fn at(page: i64, steps: &[&str], key: &str) -> YamlLocation {
        YamlLocation {
            page: Some(page),
            steps: steps.iter().map(|s| s.to_string()).collect(),
            key: Some(key.to_string()),
        }
    }

    fn issue(page: i64, steps: &[&str], key: &str, kind: ValidationRule) -> ValidationIssue {
        ValidationIssue {
            at: at(page, steps, key),
            kind,
        }
    }

    fn unknown(format: &str) -> ValidationRule {
        ValidationRule::UnknownFormat { format: p(format) }
    }

    fn matches_nothing(pattern: &str) -> ValidationRule {
        ValidationRule::PartialMatchesNoFormat {
            pattern: p(pattern),
        }
    }

    fn completed_by_no_entity(template: &str) -> ValidationRule {
        ValidationRule::TemplateMatchesNoFormat {
            template: t(template),
        }
    }

    fn after_unconditional(step: &str, page: i32) -> ValidationRule {
        ValidationRule::UnreachableAfterUnconditional {
            step: p(step),
            page: n(page),
        }
    }

    // ==========================================================================================
    // The tests
    // ==========================================================================================

    mod nothing_to_validate {
        use super::*;

        #[test]
        fn no_entries_is_valid() {
            assert_valid("");
        }

        #[test]
        fn no_entries_is_valid_with_a_python_too() {
            assert_valid_with_python("");
        }

        #[test]
        fn an_empty_formats_csv_with_no_entries_is_valid() {
            assert_valid_in(&[], &[], "");
        }
    }

    /// Error 1 on the steps that can be an outcome: with criteria, or unconditional (§5, §1.3).
    mod names_that_do_not_exist {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_declared_complete_name_is_valid() {
            assert_valid(
                "1:\n  AMUNDI-EN24: {text: a}\n  FIDEURAM-IT23@LUX: {text: f}\n  MEDIOLANUM-ES24.B: {text: m}\n",
            );
        }

        #[test]
        fn a_complete_name_not_in_formats_csv_is_an_error() {
            assert_eq!(
                issues("1:\n  GHOST-EN24: {text: g}\n"),
                vec![issue(1, &[], "GHOST-EN24", unknown("GHOST-EN24"))]
            );
        }

        /// §1.3 [U 2026-10-10]: `MEDIOLANUM-IT24` refines `.A` and `.B`, and is still not a
        /// partial: the partial is written `MEDIOLANUM-IT24.*`.
        #[test]
        fn a_complete_name_that_exists_only_with_variants_is_an_error() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM-IT24: {text: m}\n"),
                vec![issue(1, &[], "MEDIOLANUM-IT24", unknown("MEDIOLANUM-IT24"))]
            );
        }

        #[test]
        fn a_complete_name_that_exists_only_with_a_location_is_an_error() {
            assert_eq!(
                issues("1:\n  FIDEURAM-IT23: {text: f}\n"),
                vec![issue(1, &[], "FIDEURAM-IT23", unknown("FIDEURAM-IT23"))]
            );
        }

        #[test]
        fn a_typo_in_the_year_of_a_complete_name_is_an_error() {
            assert_eq!(
                issues("1:\n  EURIZON-EN32: {text: e}\n"),
                vec![issue(1, &[], "EURIZON-EN32", unknown("EURIZON-EN32"))]
            );
        }

        #[test]
        fn a_nested_complete_name_not_in_formats_csv_is_an_error_located_under_its_parent() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM:\n    text: m\n    IT25: {text: x}\n"),
                vec![issue(
                    1,
                    &["MEDIOLANUM"],
                    "MEDIOLANUM-IT25",
                    unknown("MEDIOLANUM-IT25")
                )]
            );
        }

        #[test]
        fn an_unconditional_complete_name_not_in_formats_csv_is_an_error() {
            assert_eq!(
                issues("1:\n  ASTERIA:\n    text: a\n    EN24: {text: i}\n    EN22: ~\n"),
                vec![issue(
                    1,
                    &["ASTERIA"],
                    "ASTERIA-EN22",
                    unknown("ASTERIA-EN22")
                )]
            );
        }

        #[test]
        fn a_top_level_unconditional_complete_name_not_in_formats_csv_is_an_error() {
            assert_eq!(
                issues("1:\n  GHOST-EN24: ~\n"),
                vec![issue(1, &[], "GHOST-EN24", unknown("GHOST-EN24"))]
            );
        }

        #[test]
        fn a_partial_refined_by_some_format_is_valid() {
            assert_valid(
                "1:\n  MEDIOLANUM-IT24.*: {text: m}\n  FIDEURAM-IT23@*: {text: f}\n  MEDIOLANUM-ES: {text: e}\n",
            );
        }

        #[test]
        fn an_entity_with_some_format_is_a_valid_partial() {
            assert_valid("1:\n  MEDIOLANUM: {text: m}\n");
        }

        #[test]
        fn an_entity_no_format_has_is_an_error() {
            assert_eq!(
                issues("1:\n  GHOST: {text: g}\n"),
                vec![issue(1, &[], "GHOST", matches_nothing("GHOST"))]
            );
        }

        #[test]
        fn a_partial_language_no_format_has_is_an_error() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM-FR: {text: m}\n"),
                vec![issue(
                    1,
                    &[],
                    "MEDIOLANUM-FR",
                    matches_nothing("MEDIOLANUM-FR")
                )]
            );
        }

        /// `@*` says the format **has** a location: `MEDIOLANUM-IT24` has none.
        #[test]
        fn an_open_location_on_a_format_without_one_is_an_error() {
            assert_eq!(
                issues("1:\n  \"MEDIOLANUM-IT24@*\": {text: m}\n"),
                vec![issue(
                    1,
                    &[],
                    "MEDIOLANUM-IT24@*",
                    matches_nothing("MEDIOLANUM-IT24@*")
                )]
            );
        }

        #[test]
        fn a_nested_partial_with_criteria_that_expands_to_nothing_is_an_error() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM:\n    text: m\n    FR: {text: f}\n"),
                vec![issue(
                    1,
                    &["MEDIOLANUM"],
                    "MEDIOLANUM-FR",
                    matches_nothing("MEDIOLANUM-FR")
                )]
            );
        }

        #[test]
        fn an_unconditional_partial_that_expands_to_nothing_is_an_error() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM:\n    text: m\n    FR: ~\n"),
                vec![issue(
                    1,
                    &["MEDIOLANUM"],
                    "MEDIOLANUM-FR",
                    matches_nothing("MEDIOLANUM-FR")
                )]
            );
        }

        /// §5: "I passi di solo prefisso non devono esistere nel repo: non sono mai un esito."
        #[test]
        fn a_prefix_step_need_not_be_a_format() {
            assert_valid(
                "1:\n  MEDIOLANUM:\n    text: m\n    IT24.*:\n      A: {text: a}\n      B: {text: b}\n",
            );
        }

        /// The prefix `MEDIOLANUM-IT24@*` refines nothing and is not reported; the outcome below
        /// it, which cannot exist, is.
        #[test]
        fn a_prefix_step_that_expands_to_nothing_is_not_reported_but_its_children_are() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM:\n    text: m\n    \"IT24@*\":\n      LUX: {text: l}\n"),
                vec![issue(
                    1,
                    &["MEDIOLANUM", "MEDIOLANUM-IT24@*"],
                    "MEDIOLANUM-IT24@LUX",
                    unknown("MEDIOLANUM-IT24@LUX")
                )]
            );
        }

        /// A partial root with criteria is itself an exit point: it can be the final partial.
        #[test]
        fn a_root_with_criteria_and_children_is_checked_as_well() {
            assert_eq!(
                issues("1:\n  GHOST:\n    text: g\n    EN24: {text: e}\n"),
                vec![
                    issue(1, &[], "GHOST", matches_nothing("GHOST")),
                    issue(1, &["GHOST"], "GHOST-EN24", unknown("GHOST-EN24")),
                ]
            );
        }

        #[test]
        fn names_are_checked_against_the_given_formats_only() {
            assert_eq!(
                issues_in(
                    &formats_without("AMUNDI-EN24"),
                    &[],
                    "1:\n  AMUNDI-EN24: {text: a}\n"
                ),
                vec![issue(1, &[], "AMUNDI-EN24", unknown("AMUNDI-EN24"))]
            );
        }

        #[test]
        fn on_another_page_the_location_names_that_page() {
            assert_eq!(
                issues("-1:\n  GHOST-EN24: {text: g}\n"),
                vec![issue(-1, &[], "GHOST-EN24", unknown("GHOST-EN24"))]
            );
        }
    }

    /// Error 1 for `*` templates (§1.4, R8), and their exclusion from errors 4–5 (§5, user's
    /// decision of 2026-10-11).
    mod templates {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_template_no_entity_completes_is_an_error() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM: {text: m}\n  \"*-DE23\": {text: d}\n"),
                vec![issue(1, &[], "*-DE23", completed_by_no_entity("*-DE23"))]
            );
        }

        #[test]
        fn a_typo_in_the_year_of_a_template_is_an_error() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM: {text: m}\n  \"*-EN32\": {text: e}\n"),
                vec![issue(1, &[], "*-EN32", completed_by_no_entity("*-EN32"))]
            );
        }

        #[test]
        fn a_partial_template_no_entity_completes_is_an_error() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM: {text: m}\n  \"*-FR\": {text: f}\n"),
                vec![issue(1, &[], "*-FR", completed_by_no_entity("*-FR"))]
            );
        }

        /// `MEDIOLANUM-IT24` exists only with variants: the complete instance is no format, for
        /// any entity.
        #[test]
        fn a_complete_template_only_variants_refine_is_an_error() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM: {text: m}\n  \"*-IT24\": {text: i}\n"),
                vec![issue(1, &[], "*-IT24", completed_by_no_entity("*-IT24"))]
            );
        }

        #[test]
        fn an_unconditional_template_no_entity_completes_is_an_error() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM: {text: m}\n  \"*-DE23\": ~\n"),
                vec![issue(1, &[], "*-DE23", completed_by_no_entity("*-DE23"))]
            );
        }

        /// `*-EN23` is completed by ASTERIA, EURIZON and MEDIOLANUM, not by AMUNDI or FIDEURAM.
        #[test]
        fn a_template_only_some_entities_complete_is_valid() {
            assert_valid("1:\n  MEDIOLANUM: {text: m}\n  \"*-EN23\": {text: e}\n");
        }

        #[test]
        fn a_partial_template_some_entity_completes_is_valid() {
            assert_valid(
                "1:\n  MEDIOLANUM: {text: m}\n  \"*-IT24.*\": {text: i}\n  \"*-IT23@*\": {text: l}\n",
            );
        }

        /// The user's rule (§5, 2026-10-11): with `A = MEDIOLANUM` and no `MEDIOLANUM-EN23`,
        /// `*-EN23` never starts, and that is no error — not even when no partial the YAML can
        /// accumulate makes it instantiable. Another entity (ASTERIA) completes it, so it is no
        /// typo either.
        #[test]
        fn a_template_the_accumulated_partials_cannot_complete_is_not_unreachable() {
            assert_valid_in(
                &formats_without("MEDIOLANUM-EN23"),
                &[],
                "1:\n  MEDIOLANUM: {text: m}\n  \"*-EN23\": {text: e}\n",
            );
        }

        #[test]
        fn a_template_with_nothing_ever_accumulated_is_not_unreachable() {
            assert_valid("1:\n  \"*-EN23\": {text: e}\n");
        }

        #[test]
        fn an_unconditional_template_with_nothing_ever_accumulated_is_not_unreachable() {
            assert_valid("1:\n  \"*-EN23\": ~\n");
        }

        /// Error 5 names the steps that only nothing-accumulated made candidable; a `*` is never
        /// one of them, and is excluded anyway.
        #[test]
        fn a_template_after_a_top_level_unconditional_complete_step_is_not_reported() {
            assert_valid("1:\n  AMUNDI-EN24: ~\n  \"*-EN23\": {text: e}\n");
        }

        /// R7 (1): a reachable partial `*` adds its instances to `𝒜`, so the block below
        /// `MEDIOLANUM-ES` has its root found.
        #[test]
        fn a_partial_template_instance_counts_as_a_root_found() {
            assert_valid(
                "1:\n  MEDIOLANUM: {text: m}\n  \"*-ES\": {text: e}\n  MEDIOLANUM-ES:\n    24.A: {text: g}\n",
            );
        }

        #[test]
        fn without_the_template_the_same_block_is_unreachable() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM: {text: m}\n  MEDIOLANUM-ES:\n    24.A: {text: g}\n"),
                vec![issue(
                    1,
                    &["MEDIOLANUM-ES"],
                    "MEDIOLANUM-ES24.A",
                    ValidationRule::Unreachable
                )]
            );
        }

        /// A `*` instantiable for no value of `𝒜` adds nothing: `*-ES` cannot start for
        /// `AMUNDI`, the only partial found, and the block below `AMUNDI-ES` stays unreachable.
        /// (`AMUNDI-ES` is a prefix and need not exist; its child is checked for both.)
        #[test]
        fn a_template_instance_the_repository_lacks_adds_nothing() {
            assert_eq!(
                issues(
                    "1:\n  AMUNDI: {text: a}\n  \"*-ES\": {text: e}\n  MEDIOLANUM-ES:\n    21: {text: g}\n"
                ),
                vec![issue(
                    1,
                    &["MEDIOLANUM-ES"],
                    "MEDIOLANUM-ES21",
                    ValidationRule::Unreachable
                )]
            );
        }

        /// An unconditional partial `*` replaces the values it is candidable for with its
        /// instance: with `A = MEDIOLANUM`, `"*-ES": ~` always turns it into `MEDIOLANUM-ES`, so
        /// `IT23` below the prefix `MEDIOLANUM` can never refine what is accumulated.
        #[test]
        fn an_unconditional_partial_template_replaces_the_values_it_starts_for() {
            assert_eq!(
                issues(
                    "1:\n  - MEDIOLANUM: {text: m}\n  - \"*-ES\": ~\n  - MEDIOLANUM:\n      IT23: {text: i}\n"
                ),
                vec![issue(
                    1,
                    &["MEDIOLANUM"],
                    "MEDIOLANUM-IT23",
                    ValidationRule::Unreachable
                )]
            );
        }
    }

    /// Error 4 for nested steps: the root never found (rule 3.1.2, D2), `𝒜` per R7.
    mod unreachable_nested_steps {
        use super::*;
        use pretty_assertions::assert_eq;

        /// §8: FINECO cannot be a block without a criterion common to its two covers.
        #[test]
        fn the_children_of_a_top_level_prefix_whose_root_is_never_found_are_unreachable() {
            assert_eq!(
                issues("1:\n  \"FINECO-EN23@*\":\n    IR: {text: i}\n    LUX: {text: l}\n"),
                vec![
                    issue(
                        1,
                        &["FINECO-EN23@*"],
                        "FINECO-EN23@IR",
                        ValidationRule::Unreachable
                    ),
                    issue(
                        1,
                        &["FINECO-EN23@*"],
                        "FINECO-EN23@LUX",
                        ValidationRule::Unreachable
                    ),
                ]
            );
        }

        /// §3.5: "`FIDEURAM-IT23@*` (blocco senza criteri) → radice mai trovata, non valutato".
        #[test]
        fn the_example_of_the_source_with_its_root_never_found() {
            assert_eq!(
                issues("1:\n  \"FIDEURAM-IT23@*\":\n    LUX: {text: l}\n"),
                vec![issue(
                    1,
                    &["FIDEURAM-IT23@*"],
                    "FIDEURAM-IT23@LUX",
                    ValidationRule::Unreachable
                )]
            );
        }

        #[test]
        fn the_children_of_a_root_with_criteria_are_reachable() {
            assert_valid(
                "1:\n  MEDIOLANUM:\n    text: m\n    EN23: {text: e}\n    IT23: {text: i}\n",
            );
        }

        #[test]
        fn a_prefix_root_found_earlier_on_the_same_page_is_found() {
            assert_valid("1:\n  - MEDIOLANUM: {text: m}\n  - MEDIOLANUM:\n      EN23: {text: e}\n");
        }

        /// §3.3: `A` is inherited from one entry to the next.
        #[test]
        fn a_prefix_root_found_on_an_earlier_page_is_found() {
            assert_valid("1:\n  MEDIOLANUM: {text: m}\n2:\n  MEDIOLANUM:\n    EN23: {text: e}\n");
        }

        #[test]
        fn a_prefix_root_found_only_on_a_later_entry_is_not_found() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM:\n    EN23: {text: e}\n2:\n  MEDIOLANUM: {text: m}\n"),
                vec![issue(
                    1,
                    &["MEDIOLANUM"],
                    "MEDIOLANUM-EN23",
                    ValidationRule::Unreachable
                )]
            );
        }

        /// D2, R12: the root of a chain of prefixes is its top-level key, so `MEDIOLANUM` found
        /// is not `MEDIOLANUM-ES` found.
        #[test]
        fn the_root_of_a_chain_of_prefixes_is_its_top_level_key() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM: {text: m}\n2:\n  MEDIOLANUM-ES:\n    21: {text: e}\n"),
                vec![issue(
                    2,
                    &["MEDIOLANUM-ES"],
                    "MEDIOLANUM-ES21",
                    ValidationRule::Unreachable
                )]
            );
        }

        /// D2: the root is the nearest ancestor with criteria; the prefix `24.*` between does not
        /// count.
        #[test]
        fn the_root_is_the_nearest_ancestor_with_criteria() {
            assert_valid(
                "1:\n  MEDIOLANUM:\n    text: m\n    ES:\n      text: e\n      24.*:\n        A: {text: a}\n",
            );
        }

        /// R7 (2): an unreachable partial adds nothing to `𝒜`, so it does not make its own
        /// children reachable.
        #[test]
        fn an_unreachable_partial_does_not_make_its_children_reachable() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM-ES:\n    24.*:\n      text: s\n      A: {text: a}\n"),
                vec![
                    issue(
                        1,
                        &["MEDIOLANUM-ES"],
                        "MEDIOLANUM-ES24.*",
                        ValidationRule::Unreachable
                    ),
                    issue(
                        1,
                        &["MEDIOLANUM-ES", "MEDIOLANUM-ES24.*"],
                        "MEDIOLANUM-ES24.A",
                        ValidationRule::Unreachable
                    ),
                ]
            );
        }

        /// The "otherwise" of a block replaces every value it is candidable for: after `EN23: ~`
        /// no value lets `EN24` be evaluated (nested: error 4, not 5).
        #[test]
        fn a_sibling_after_the_otherwise_of_its_block_is_unreachable() {
            assert_eq!(
                issues("1:\n  ASTERIA:\n    text: a\n    EN23: ~\n    EN24: {text: i}\n"),
                vec![issue(
                    1,
                    &["ASTERIA"],
                    "ASTERIA-EN24",
                    ValidationRule::Unreachable
                )]
            );
        }

        /// The "otherwise" inside a block does not close the top level: the root may fail.
        #[test]
        fn the_otherwise_of_a_block_leaves_the_following_top_level_steps_reachable() {
            assert_valid(
                "1:\n  ASTERIA:\n    text: a\n    EN24: {text: i}\n    EN23: ~\n  AMUNDI-EN24: {text: x}\n",
            );
        }

        /// R7, rule 3.1.4: `24.B: ~` only replaces `MEDIOLANUM-ES`, for which its ancestor `ES`
        /// is not evaluated; with `A = MEDIOLANUM` the criteria of `ES` can fail, `24.B` is not
        /// reached, and `IT23` is.
        #[test]
        fn an_ancestor_with_criteria_can_stop_the_otherwise() {
            assert_valid(
                "1:\n  MEDIOLANUM: {text: m}\n2:\n  MEDIOLANUM:\n    ES:\n      text: x\n      24.B: ~\n    IT23: {text: i}\n",
            );
        }

        /// A sibling found first does not make the next one unreachable: `EN` may fail.
        #[test]
        fn a_partial_sibling_with_criteria_does_not_hide_the_next_one() {
            assert_valid(
                "1:\n  MEDIOLANUM:\n    text: m\n    EN: {text: e}\n    IT23: {text: i}\n",
            );
        }

        #[test]
        fn a_deep_child_under_a_root_never_found_is_unreachable_with_its_path() {
            assert_eq!(
                issues("1:\n  MEDIOLANUM-IT:\n    24.*:\n      A: {text: a}\n"),
                vec![issue(
                    1,
                    &["MEDIOLANUM-IT", "MEDIOLANUM-IT24.*"],
                    "MEDIOLANUM-IT24.A",
                    ValidationRule::Unreachable
                )]
            );
        }

        /// A step whose name does not exist and that no document reaches has both defects.
        #[test]
        fn an_unreachable_step_that_does_not_exist_has_both_issues() {
            let found = issues("1:\n  \"FINECO-EN23@*\":\n    DE: {text: d}\n");
            assert_eq!(found.len(), 2, "{found:?}");
            assert!(
                found.contains(&issue(
                    1,
                    &["FINECO-EN23@*"],
                    "FINECO-EN23@DE",
                    unknown("FINECO-EN23@DE")
                )),
                "{found:?}"
            );
            assert!(
                found.contains(&issue(
                    1,
                    &["FINECO-EN23@*"],
                    "FINECO-EN23@DE",
                    ValidationRule::Unreachable
                )),
                "{found:?}"
            );
        }
    }

    /// Errors 4 and 5 at the top level: only the removal of "nothing accumulated" from `𝒜` makes a
    /// top-level step unreachable.
    mod unreachable_top_level_steps {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_step_after_a_top_level_unconditional_complete_step_is_error_5() {
            assert_eq!(
                issues("1:\n  AMUNDI-EN24: ~\n  EURIZON-EN23: {text: e}\n"),
                vec![issue(
                    1,
                    &[],
                    "EURIZON-EN23",
                    after_unconditional("AMUNDI-EN24", 1)
                )]
            );
        }

        #[test]
        fn every_step_after_it_is_reported() {
            assert_eq!(
                issues("1:\n  AMUNDI-EN24: ~\n  EURIZON-EN23: {text: e}\n  ASTERIA: {text: a}\n"),
                vec![
                    issue(
                        1,
                        &[],
                        "EURIZON-EN23",
                        after_unconditional("AMUNDI-EN24", 1)
                    ),
                    issue(1, &[], "ASTERIA", after_unconditional("AMUNDI-EN24", 1)),
                ]
            );
        }

        #[test]
        fn a_second_unconditional_complete_step_is_itself_unreachable() {
            assert_eq!(
                issues("1:\n  AMUNDI-EN24: ~\n  EURIZON-EN23: ~\n"),
                vec![issue(
                    1,
                    &[],
                    "EURIZON-EN23",
                    after_unconditional("AMUNDI-EN24", 1)
                )]
            );
        }

        /// Page 1 exists in every document that has a page: what follows, on any page, is never
        /// reached with nothing accumulated.
        #[test]
        fn a_step_on_a_later_entry_after_an_unconditional_on_page_1_is_error_5() {
            assert_eq!(
                issues("1:\n  AMUNDI-EN24: ~\n2:\n  EURIZON-EN23: {text: e}\n"),
                vec![issue(
                    2,
                    &[],
                    "EURIZON-EN23",
                    after_unconditional("AMUNDI-EN24", 1)
                )]
            );
        }

        /// The steps before it are reachable, and so is the unconditional step itself.
        #[test]
        fn the_steps_before_it_are_reachable() {
            assert_valid(
                "1:\n  EURIZON-EN23: {text: e}\n  MEDIOLANUM: {text: m}\n  AMUNDI-EN24: ~\n",
            );
        }

        /// With `A = MEDIOLANUM` the unconditional `AMUNDI-EN24` is not candidable, so `A` survives
        /// it and a refinement of it stays reachable.
        #[test]
        fn a_step_refining_a_partial_found_before_it_stays_reachable() {
            assert_valid(
                "1:\n  MEDIOLANUM: {text: m}\n  AMUNDI-EN24: ~\n  MEDIOLANUM-IT23: {text: i}\n",
            );
        }

        /// A partial top-level step candidable only with nothing accumulated is error 5; the
        /// children, never candidable with nothing accumulated, are error 4.
        #[test]
        fn a_block_after_it_is_error_5_at_its_root_and_error_4_below() {
            assert_eq!(
                issues("1:\n  AMUNDI-EN24: ~\n  MEDIOLANUM:\n    text: m\n    EN23: {text: e}\n"),
                vec![
                    issue(1, &[], "MEDIOLANUM", after_unconditional("AMUNDI-EN24", 1)),
                    issue(
                        1,
                        &["MEDIOLANUM"],
                        "MEDIOLANUM-EN23",
                        ValidationRule::Unreachable
                    ),
                ]
            );
        }

        /// Error 5 is for an unconditional **complete** step: an unconditional partial replaces
        /// "nothing accumulated" with itself, and what it makes unreachable is error 4.
        #[test]
        fn a_step_after_a_top_level_unconditional_partial_step_is_error_4() {
            assert_eq!(
                issues(
                    "1:\n  MEDIOLANUM: ~\n  AMUNDI-EN24: {text: a}\n  MEDIOLANUM-IT23: {text: i}\n"
                ),
                vec![issue(1, &[], "AMUNDI-EN24", ValidationRule::Unreachable)]
            );
        }

        /// After an unconditional partial the accumulated partial is that partial, so a block
        /// rooted there has its root found.
        #[test]
        fn a_top_level_unconditional_partial_finds_the_root_of_a_later_block() {
            assert_valid("1:\n  - MEDIOLANUM: ~\n  - MEDIOLANUM:\n      IT23: {text: i}\n");
        }

        #[test]
        fn the_error_5_message_names_the_unconditional_step_and_its_page() {
            let found = issues("-1:\n  AMUNDI-EN24: ~\n  EURIZON-EN23: {text: e}\n");
            let message = found[0].to_string();
            assert!(message.contains("AMUNDI-EN24"), "{message}");
            assert!(message.contains("page -1"), "{message}");
            assert!(message.contains("EURIZON-EN23"), "{message}");
        }
    }

    /// §5, "Pagine che possono non esistere" (2026-10-11): an unconditional step removes values
    /// from `𝒜` only when its entry is on page `1` or `-1`; elsewhere the document may be shorter,
    /// the entry is "nothing", and what follows stays reachable.
    mod pages_that_may_not_exist {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_unconditional_complete_step_on_page_5_does_not_hide_a_later_entry() {
            assert_valid("- 5:\n    - AMUNDI-EN24: ~\n- 2:\n    - EURIZON-EN23: {text: e}\n");
        }

        #[test]
        fn the_same_on_page_1_is_error_5() {
            assert_eq!(
                issues("- 1:\n    - AMUNDI-EN24: ~\n- 2:\n    - EURIZON-EN23: {text: e}\n"),
                vec![issue(
                    2,
                    &[],
                    "EURIZON-EN23",
                    after_unconditional("AMUNDI-EN24", 1)
                )]
            );
        }

        #[test]
        fn the_same_on_page_minus_1_is_error_5() {
            assert_eq!(
                issues("- -1:\n    - AMUNDI-EN24: ~\n- 2:\n    - EURIZON-EN23: {text: e}\n"),
                vec![issue(
                    2,
                    &[],
                    "EURIZON-EN23",
                    after_unconditional("AMUNDI-EN24", -1)
                )]
            );
        }

        /// The rule is on the page of the entry, not on whether the next step shares it.
        #[test]
        fn on_page_5_it_removes_nothing_even_for_the_steps_of_its_own_entry() {
            assert_valid("5:\n  AMUNDI-EN24: ~\n  EURIZON-EN23: {text: e}\n");
        }

        #[test]
        fn on_page_2_the_otherwise_of_a_block_does_not_hide_its_next_sibling() {
            assert_valid("2:\n  ASTERIA:\n    text: a\n    EN23: ~\n    EN24: {text: i}\n");
        }

        #[test]
        fn on_page_minus_1_the_otherwise_of_a_block_hides_its_next_sibling() {
            assert_eq!(
                issues("-1:\n  ASTERIA:\n    text: a\n    EN23: ~\n    EN24: {text: i}\n"),
                vec![issue(
                    -1,
                    &["ASTERIA"],
                    "ASTERIA-EN24",
                    ValidationRule::Unreachable
                )]
            );
        }

        /// Off pages ±1 an unconditional partial still adds itself to `𝒜` (it is reachable), but
        /// takes nothing away: `AMUNDI-EN24` stays reachable from nothing accumulated, and the
        /// block rooted at `MEDIOLANUM` has its root found.
        #[test]
        fn on_page_3_an_unconditional_partial_adds_itself_and_removes_nothing() {
            assert_valid(
                "3:\n  - MEDIOLANUM: ~\n  - AMUNDI-EN24: {text: a}\n  - MEDIOLANUM:\n      IT23: {text: i}\n",
            );
        }

        #[test]
        fn on_page_1_the_same_unconditional_partial_hides_the_other_entity() {
            assert_eq!(
                issues(
                    "1:\n  - MEDIOLANUM: ~\n  - AMUNDI-EN24: {text: a}\n  - MEDIOLANUM:\n      IT23: {text: i}\n"
                ),
                vec![issue(1, &[], "AMUNDI-EN24", ValidationRule::Unreachable)]
            );
        }

        /// The same holds with a Python: page 5 removes nothing.
        #[test]
        fn with_a_python_an_unconditional_on_page_5_removes_nothing() {
            assert_valid_with_python(
                "- 5:\n    - AMUNDI-EN24: ~\n- 2:\n    - MEDIOLANUM: {text: m}\n",
            );
        }
    }

    /// §5, "Con il Python presente" (2026-10-11, replacing the earlier suspension): `𝒜` starts
    /// from nothing accumulated plus every partial that expands to some format of the repository;
    /// checks 4–5 apply to every step with that `𝒜`, `*` steps still excluded; errors 1–3 all stay.
    mod with_a_python {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The Python can accumulate `FINECO-EN23@*` itself.
        #[test]
        fn a_nested_step_whose_root_the_python_can_give_is_reachable() {
            assert_valid_with_python(
                "1:\n  \"FINECO-EN23@*\":\n    IR: {text: i}\n    LUX: {text: l}\n",
            );
        }

        /// No longer suspended: the only partials that make `EN24` candidable below `ASTERIA` are
        /// `ASTERIA` and `ASTERIA-EN`, and the "otherwise" `EN23: ~` on page 1 decides both.
        #[test]
        fn a_sibling_after_the_otherwise_of_its_block_is_unreachable_even_with_a_python() {
            assert_eq!(
                issues_with_python(
                    "1:\n  ASTERIA:\n    text: a\n    EN23: ~\n    EN24: {text: i}\n"
                ),
                vec![issue(
                    1,
                    &["ASTERIA"],
                    "ASTERIA-EN24",
                    ValidationRule::Unreachable
                )]
            );
        }

        /// The coordinator's example: the Python can return `EURIZON`, for which the unconditional
        /// `AMUNDI-EN24` is not candidable, and `EURIZON-EN23` refines it.
        #[test]
        fn a_step_refining_a_partial_the_python_can_give_survives_a_top_level_unconditional() {
            assert_valid_with_python("1:\n  AMUNDI-EN24: ~\n  EURIZON-EN23: {text: e}\n");
        }

        #[test]
        fn a_block_whose_root_the_python_can_give_survives_a_top_level_unconditional() {
            assert_valid_with_python(
                "1:\n  AMUNDI-EN24: ~\n  \"FINECO-EN23@*\":\n    IR: {text: i}\n",
            );
        }

        /// No format has a location under `MEDIOLANUM-IT24`, so no partial the Python can return
        /// is that root: the child is unreachable (and does not exist).
        #[test]
        fn a_nested_step_whose_root_no_format_refines_is_unreachable_even_with_a_python() {
            let found = issues_with_python("1:\n  \"MEDIOLANUM-IT24@*\":\n    LUX: {text: l}\n");
            assert_eq!(found.len(), 2, "{found:?}");
            assert!(
                found.contains(&issue(
                    1,
                    &["MEDIOLANUM-IT24@*"],
                    "MEDIOLANUM-IT24@LUX",
                    unknown("MEDIOLANUM-IT24@LUX")
                )),
                "{found:?}"
            );
            assert!(
                found.contains(&issue(
                    1,
                    &["MEDIOLANUM-IT24@*"],
                    "MEDIOLANUM-IT24@LUX",
                    ValidationRule::Unreachable
                )),
                "{found:?}"
            );
        }

        /// The Python's partials expand to some format: a prefix root that expands to nothing is
        /// never among them, even when its children would exist.
        #[test]
        fn a_prefix_root_refining_no_format_is_never_given_by_the_python() {
            assert_eq!(
                issues_with_python("1:\n  MEDIOLANUM-FR:\n    23: {text: f}\n")
                    .into_iter()
                    .filter(|issue| issue.kind == ValidationRule::Unreachable)
                    .collect::<Vec<_>>(),
                vec![issue(
                    1,
                    &["MEDIOLANUM-FR"],
                    "MEDIOLANUM-FR23",
                    ValidationRule::Unreachable
                )]
            );
        }

        #[test]
        fn a_template_is_still_not_reported_as_unreachable() {
            assert_valid_with_python("1:\n  \"*-EN23\": {text: e}\n");
        }

        #[test]
        fn a_complete_name_not_in_formats_csv_is_still_an_error() {
            assert_eq!(
                issues_with_python("1:\n  GHOST-EN24: {text: g}\n"),
                vec![issue(1, &[], "GHOST-EN24", unknown("GHOST-EN24"))]
            );
        }

        #[test]
        fn a_nested_name_not_in_formats_csv_is_still_an_error() {
            assert_eq!(
                issues_with_python("1:\n  \"FINECO-EN23@*\":\n    DE: {text: d}\n"),
                vec![issue(
                    1,
                    &["FINECO-EN23@*"],
                    "FINECO-EN23@DE",
                    unknown("FINECO-EN23@DE")
                )]
            );
        }

        #[test]
        fn a_partial_expanding_to_nothing_is_still_an_error() {
            assert_eq!(
                issues_with_python("1:\n  MEDIOLANUM-FR: {text: f}\n"),
                vec![issue(
                    1,
                    &[],
                    "MEDIOLANUM-FR",
                    matches_nothing("MEDIOLANUM-FR")
                )]
            );
        }

        #[test]
        fn a_template_no_entity_completes_is_still_an_error() {
            assert_eq!(
                issues_with_python("1:\n  \"*-DE23\": {text: d}\n"),
                vec![issue(1, &[], "*-DE23", completed_by_no_entity("*-DE23"))]
            );
        }

        /// An entity alone is candidable only with nothing accumulated, whatever the Python
        /// found: after `AMUNDI-EN24: ~` it is reached by no document, Python or not.
        #[test]
        fn an_entity_after_a_top_level_unconditional_complete_step_is_still_error_5() {
            assert_eq!(
                issues_with_python("1:\n  AMUNDI-EN24: ~\n  MEDIOLANUM: {text: m}\n"),
                vec![issue(
                    1,
                    &[],
                    "MEDIOLANUM",
                    after_unconditional("AMUNDI-EN24", 1)
                )]
            );
        }

        /// A `document_classifier` with no entries accumulates nothing: it is no Python, and `𝒜`
        /// starts from nothing accumulated alone.
        #[test]
        fn a_python_without_entries_is_no_python() {
            assert_eq!(
                issues_in(
                    &formats(),
                    &[],
                    "1:\n  \"FINECO-EN23@*\":\n    IR: {text: i}\n"
                ),
                vec![issue(
                    1,
                    &["FINECO-EN23@*"],
                    "FINECO-EN23@IR",
                    ValidationRule::Unreachable
                )]
            );
        }
    }

    /// Every issue is reported, with its page, path and key, in file order.
    mod issues_and_their_order {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_issue_is_reported_in_file_order() {
            let text = "1:\n  GHOST-EN24: {text: g}\n  \"FINECO-EN23@*\":\n    IR: {text: i}\n  MEDIOLANUM:\n    text: m\n    FR: {text: f}\n2:\n  \"*-DE23\": {text: d}\n";
            assert_eq!(
                issues(text),
                vec![
                    issue(1, &[], "GHOST-EN24", unknown("GHOST-EN24")),
                    issue(
                        1,
                        &["FINECO-EN23@*"],
                        "FINECO-EN23@IR",
                        ValidationRule::Unreachable
                    ),
                    issue(
                        1,
                        &["MEDIOLANUM"],
                        "MEDIOLANUM-FR",
                        matches_nothing("MEDIOLANUM-FR")
                    ),
                    issue(2, &[], "*-DE23", completed_by_no_entity("*-DE23")),
                ]
            );
        }

        /// File order, not the alphabet nor a hash: twelve names in no particular order.
        #[test]
        fn many_issues_follow_the_file_not_the_alphabet() {
            let names = [
                "ZETA", "ALPHA", "MU", "BETA", "OMEGA", "GAMMA", "KAPPA", "DELTA", "SIGMA",
                "EPSILON", "TAU", "ETA",
            ];
            let text: String = std::iter::once("1:\n".to_string())
                .chain(
                    names
                        .iter()
                        .map(|name| format!("  {name}-EN24: {{text: x}}\n")),
                )
                .collect();
            let keys: Vec<String> = issues(&text)
                .into_iter()
                .map(|issue| issue.at.key.unwrap())
                .collect();
            let expected: Vec<String> = names.iter().map(|name| format!("{name}-EN24")).collect();
            assert_eq!(keys, expected);
        }

        /// Entries in the order listed, not by page number.
        #[test]
        fn entries_are_in_the_order_written_not_by_page_number() {
            let found =
                issues("- 5:\n    - GHOST-EN24: {text: g}\n- 1:\n    - GHOST-EN23: {text: g}\n");
            assert_eq!(
                found,
                vec![
                    issue(5, &[], "GHOST-EN24", unknown("GHOST-EN24")),
                    issue(1, &[], "GHOST-EN23", unknown("GHOST-EN23")),
                ]
            );
        }

        /// Depth-first preorder: a step, its children, then the next step.
        #[test]
        fn nested_issues_come_between_their_parent_and_the_next_step() {
            let text =
                "1:\n  MEDIOLANUM:\n    text: m\n    IT25: {text: x}\n  GHOST-EN24: {text: g}\n";
            assert_eq!(
                issues(text),
                vec![
                    issue(
                        1,
                        &["MEDIOLANUM"],
                        "MEDIOLANUM-IT25",
                        unknown("MEDIOLANUM-IT25")
                    ),
                    issue(1, &[], "GHOST-EN24", unknown("GHOST-EN24")),
                ]
            );
        }

        #[test]
        fn validating_twice_gives_the_same_issues() {
            let text = "1:\n  GHOST-EN24: {text: g}\n  \"FINECO-EN23@*\":\n    IR: {text: i}\n    LUX: {text: l}\n";
            assert_eq!(issues(text), issues(text));
        }

        #[test]
        fn an_issue_displays_its_page_its_path_its_key_and_its_rule() {
            let found = issues("1:\n  MEDIOLANUM:\n    text: m\n    IT25: {text: x}\n");
            let message = found[0].to_string();
            assert!(message.contains("page 1"), "{message}");
            assert!(message.contains("step MEDIOLANUM"), "{message}");
            assert!(message.contains("MEDIOLANUM-IT25"), "{message}");
            assert!(message.contains("formats.csv"), "{message}");
        }

        #[test]
        fn the_error_lists_every_issue_in_order() {
            let text = "1:\n  GHOST-EN24: {text: g}\n  MEDIOLANUM-FR: {text: f}\n  \"*-DE23\": {text: d}\n";
            let error = validate(&formats(), &[], &yaml(text))
                .expect_err("expected the validation to fail");
            let message = error.to_string();
            let positions: Vec<usize> = error
                .issues
                .iter()
                .map(|issue| {
                    message
                        .find(&issue.to_string())
                        .unwrap_or_else(|| panic!("'{issue}' missing from:\n{message}"))
                })
                .collect();
            assert_eq!(error.issues.len(), 3);
            assert!(
                positions.windows(2).all(|pair| pair[0] < pair[1]),
                "out of order:\n{message}"
            );
        }

        #[test]
        fn a_template_issue_is_keyed_by_the_template_as_written() {
            let found = issues("1:\n  \"*-DE23\": {text: d}\n");
            assert_eq!(found[0].at.key.as_deref(), Some("*-DE23"));
        }
    }

    /// §8: the draft for the 25 real formats passes clean (fase 9, "fine fase").
    mod the_draft_of_the_specification {
        use super::*;
        use pretty_assertions::assert_eq;

        /// §8 of the specification, verbatim (as in `doc_classify_yaml`'s tests).
        const DRAFT: &str = r#"
1:
  AMUNDI-EN24:       {text: '^AMUNDI FUNDS$'}
  AMUNDI-IT24:       {text: '^FONDI AMUNDI '}
  ANIMA-EN23:        {text: '^ANIMA Funds Plc$'}
  ANIMA_SICAV-EN24:  {text: '^ANIMA INVESTMENT SICAV$'}
  ARCA-IT24:         [{text: '^RELAZIONE '}, {text: '^ARCA '}]
  ASTERIA:
    text: '^Asteria Funds$'
    EN24: {text: '^Investment Managers$'}
    EN23: ~                                   # l'«altrimenti» del blocco
  CARNE-EN23:        {text: '^SIDERA FUNDS$'}
  DANSKEINVEST-EN24: {text: '^DANSKE INVEST SICAV$'}
  EURIZON-EN21:      {text: '^Eurizon AM Sicav$'}
  EURIZON-EN23:      {text: '^Eurizon Fund$'}
  FIDEURAM-EN23:     {text: '^WILLERFUNDS$'}
  FIDEURAM-IT24:     {text: '^Fideuram ISPB Asset Management SGR'}
  FINECO-EN23@IR:    {text: '^FAM Series UCITS ICAV$'}
  FINECO-EN23@LUX:   {text: '^CoRe Series$'}
  KAIROS-EN23:       {text: '^KAIROS INTERNATIONAL SICAV$'}
  UBS-EN23:          {text: '^Annual Report', font: ubsheadline}
  MEDIOLANUM-EN24:   {text: '^CHALLENGE FUNDS$'}
  MEDIOLANUM-ES24.A: {text: '^GAMAX FUNDS FCP$'}
  MEDIOLANUM:                                 # parziale per ultimo: è «appiccicoso» (§9.2)
    any_of: [{text: '^Mediolanum'}, {text: '^MEDIOLANUM'}]
    EN23:   {text: '^Mediolanum Specialities SICAV-SIF$'}
    ES24.B: {text: 'Nº Registro CNMV'}
    IT24.*:
      A: {text: '^Mediolanum Best Brands$'}
      B: {text: '^Relazione di gestione annuale'}
2:
  ANIMA_SGR-IT24:    {text: '^ANIMA SGR S.p.A.$'}
5:
  EURIZON-IT24:      {text: '^Relazione di gestione$', font_size: 26}
"#;

        /// `analysis_finance_reports_formats/metadata/formats.csv`, in file order (2026-10-11).
        const REAL_FORMATS: [&str; 25] = [
            "AMUNDI-EN24",
            "AMUNDI-IT24",
            "ANIMA-EN23",
            "ARCA-IT24",
            "ASTERIA-EN23",
            "ASTERIA-EN24",
            "DANSKEINVEST-EN24",
            "EURIZON-IT24",
            "EURIZON-EN23",
            "EURIZON-EN21",
            "FIDEURAM-EN23",
            "MEDIOLANUM-EN23",
            "MEDIOLANUM-ES24.A",
            "MEDIOLANUM-ES24.B",
            "MEDIOLANUM-IT24.A",
            "MEDIOLANUM-EN24",
            "MEDIOLANUM-IT24.B",
            "FIDEURAM-IT24",
            "CARNE-EN23",
            "ANIMA_SGR-IT24",
            "ANIMA_SICAV-EN24",
            "FINECO-EN23@LUX",
            "FINECO-EN23@IR",
            "KAIROS-EN23",
            "UBS-EN23",
        ];

        fn real_formats() -> Vec<String> {
            REAL_FORMATS.iter().map(|f| f.to_string()).collect()
        }

        #[test]
        fn it_passes_clean_on_the_25_real_formats() {
            assert_valid_in(&real_formats(), &[], DRAFT);
        }

        #[test]
        fn it_passes_clean_beside_a_python() {
            assert_valid_in(&real_formats(), &python(), DRAFT);
        }

        /// The same draft against a repository that lost one of its formats: the step naming it
        /// is the one issue.
        #[test]
        fn a_format_removed_from_formats_csv_is_the_one_issue() {
            let formats: Vec<String> = real_formats()
                .into_iter()
                .filter(|f| f != "MEDIOLANUM-IT24.B")
                .collect();
            assert_eq!(
                issues_in(&formats, &[], DRAFT),
                vec![issue(
                    1,
                    &["MEDIOLANUM", "MEDIOLANUM-IT24.*"],
                    "MEDIOLANUM-IT24.B",
                    unknown("MEDIOLANUM-IT24.B")
                )]
            );
        }

        /// §8's lesson: FINECO as a block without a common criterion is never reached.
        #[test]
        fn fineco_written_as_a_block_without_criteria_is_unreachable() {
            let text = "1:\n  \"FINECO-EN23@*\":\n    IR: {text: '^FAM Series UCITS ICAV$'}\n    LUX: {text: '^CoRe Series$'}\n";
            assert_eq!(
                issues_in(&real_formats(), &[], text),
                vec![
                    issue(
                        1,
                        &["FINECO-EN23@*"],
                        "FINECO-EN23@IR",
                        ValidationRule::Unreachable
                    ),
                    issue(
                        1,
                        &["FINECO-EN23@*"],
                        "FINECO-EN23@LUX",
                        ValidationRule::Unreachable
                    ),
                ]
            );
        }
    }
}
