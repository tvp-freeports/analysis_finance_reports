//! The grammar of **partial** format names, and the relations between them.
//!
//! A format name has five slots, in hierarchical order: entity, language, year, location, variant
//! (`ENTITY-LLYY[@LOC][.V]`). A [`FormatPattern`] is a format name some of whose slots may still
//! be open: truncated on the right (`MEDIOLANUM`, `MEDIOLANUM-IT`), or explicitly required but not
//! yet decided (`FIDEURAM-IT23@*`, `MEDIOLANUM-IT24.*`). A complete name — everything the
//! format-name grammar of [`super::id_format`] accepts that contains no `*` — is a pattern with
//! every slot decided.
//!
//! A [`FormatTemplate`] is the `*` as entity (`*-EN23`): not a pattern but a model that becomes one
//! once the entity is known.
//!
//! Every relation between patterns — refinement, meet, compatibility — derives from one rule, the
//! meet of two slots ([`Slot::meet`]): `Q ⊑ P` exactly when `Q ⊓ P = Q`. Writing the refinement
//! rules separately would give two definitions that could drift apart.
//!
//! The normative specification is §1 of `agent-memory/SPEC-document-classification.md`.

use std::fmt;
use std::str::FromStr;

/// The state of one slot of a [`FormatPattern`].
///
/// Not generic over the value: every slot value is a piece of the name's text (the year included,
/// so that `05` keeps its leading zero), and a single string type keeps the slot-by-slot relations
/// one function rather than five.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Slot {
    /// Decided: the value as written in the name.
    Fixed(String),
    /// An optional slot left unwritten in a name that reaches the year: the format does **not**
    /// have it.
    Absent,
    /// `@*` or `.*`: the format **has** the slot, its value is not decided yet.
    Required,
    /// Truncated on the right: nothing is known; an optional slot may end `Fixed` **or** `Absent`.
    Unknown,
}

impl Slot {
    /// The most general slot state refining both, if any: the one rule every relation between
    /// patterns is built on.
    fn meet(&self, other: &Slot) -> Option<Slot> {
        match (self, other) {
            (Slot::Unknown, slot) | (slot, Slot::Unknown) => Some(slot.clone()),
            (Slot::Required, Slot::Fixed(value)) | (Slot::Fixed(value), Slot::Required) => {
                Some(Slot::Fixed(value.clone()))
            }
            (a, b) if a == b => Some(a.clone()),
            _ => None,
        }
    }

    fn is_decided(&self) -> bool {
        matches!(self, Slot::Fixed(_) | Slot::Absent)
    }

    /// The slot as written after its `prefix`; an `Absent` or `Unknown` slot writes nothing.
    fn write(&self, f: &mut fmt::Formatter<'_>, prefix: &str) -> fmt::Result {
        match self {
            Slot::Fixed(value) => write!(f, "{prefix}{value}"),
            Slot::Required => write!(f, "{prefix}*"),
            Slot::Absent | Slot::Unknown => Ok(()),
        }
    }
}

/// The five segments of a format name, as named by a [`FormatPatternError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Segment {
    Entity,
    Language,
    Year,
    Location,
    Variant,
}

impl fmt::Display for Segment {
    /// The lowercase English name of the segment: `entity`, `language`, `year`, `location`,
    /// `variant`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Segment::Entity => "entity",
            Segment::Language => "language",
            Segment::Year => "year",
            Segment::Location => "location",
            Segment::Variant => "variant",
        })
    }
}

/// A string that is not a format pattern (or not a format template): which segment could not be
/// read, and why.
///
/// The message names the input, the segment and the reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{input}': cannot read the {segment} segment: {reason}")]
pub struct FormatPatternError {
    pub input: String,
    pub segment: Segment,
    pub reason: String,
}

impl FormatPatternError {
    fn new(input: &str, segment: Segment, reason: &str) -> Self {
        FormatPatternError { input: input.to_string(), segment, reason: reason.to_string() }
    }
}

/// A possibly partial format name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormatPattern {
    entity: String,
    language: Slot,
    year: Slot,
    location: Slot,
    variant: Slot,
}

impl FormatPattern {
    /// The entity: always decided in a pattern.
    pub fn entity(&self) -> &str {
        &self.entity
    }

    pub fn language(&self) -> &Slot {
        &self.language
    }

    pub fn year(&self) -> &Slot {
        &self.year
    }

    pub fn location(&self) -> &Slot {
        &self.location
    }

    pub fn variant(&self) -> &Slot {
        &self.variant
    }

    fn slots(&self) -> [&Slot; 4] {
        [&self.language, &self.year, &self.location, &self.variant]
    }

    /// No slot is `Required` or `Unknown`.
    pub fn is_complete(&self) -> bool {
        self.slots().iter().all(|slot| slot.is_decided())
    }

    /// `self ⊑ other`: every name refining `self` also refines `other`.
    pub fn refines(&self, other: &FormatPattern) -> bool {
        self.meet(other).as_ref() == Some(self)
    }

    /// `self ⊑ other` and `self != other`.
    pub fn strictly_refines(&self, other: &FormatPattern) -> bool {
        self != other && self.refines(other)
    }

    /// Some complete name refines both (the meet exists).
    pub fn is_compatible_with(&self, other: &FormatPattern) -> bool {
        self.meet(other).is_some()
    }

    /// `self ⊓ other`: the most general pattern refining both, if any.
    pub fn meet(&self, other: &FormatPattern) -> Option<FormatPattern> {
        if self.entity != other.entity {
            return None;
        }
        Some(FormatPattern {
            entity: self.entity.clone(),
            language: self.language.meet(&other.language)?,
            year: self.year.meet(&other.year)?,
            location: self.location.meet(&other.location)?,
            variant: self.variant.meet(&other.variant)?,
        })
    }

    /// The number of slots neither `Unknown` nor `Required` (the entity included).
    pub fn specificity(&self) -> usize {
        1 + self.slots().iter().filter(|slot| slot.is_decided()).count()
    }

    /// The formats among `formats` that refine this pattern, in the given order.
    ///
    /// A string that is not a format name refines nothing.
    pub fn expand(&self, formats: &[String]) -> Vec<String> {
        formats
            .iter()
            .filter(|format| format.parse::<FormatPattern>().is_ok_and(|name| name.refines(self)))
            .cloned()
            .collect()
    }
}

impl FromStr for FormatPattern {
    type Err = FormatPatternError;

    /// The partial grammar first; a name it rejects may still be a complete name whose entity
    /// holds characters a partial cannot (`FOO-BAR-IT24`, `FOO BAR-IT24`), which the format-name
    /// grammar of [`super::id_format`] accepts and so must stay readable. When neither reading
    /// succeeds, the partial grammar's diagnosis is the one reported.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_partial(s).or_else(|error| parse_complete_with_any_entity(s).ok_or(error))
    }
}

impl fmt::Display for FormatPattern {
    /// The canonical form, the one [`FromStr`] reads back.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.entity)?;
        self.language.write(f, "-")?;
        self.year.write(f, "")?;
        self.location.write(f, "@")?;
        self.variant.write(f, ".")
    }
}

/// A pattern crosses a process boundary inside an
/// [`Extracted`](crate::core::pipeline::Extracted): serialised as its canonical string, the one
/// [`FromStr`] reads back.
impl serde::Serialize for FormatPattern {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for FormatPattern {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <String as serde::Deserialize>::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// `*` as entity: a model instantiated with the entity of the partial accumulated so far.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormatTemplate {
    language: String,
    year: Slot,
    location: Slot,
    variant: Slot,
}

impl FormatTemplate {
    /// The pattern this template stands for once `entity` is known.
    pub fn instantiate(&self, entity: &str) -> FormatPattern {
        FormatPattern {
            entity: entity.to_string(),
            language: Slot::Fixed(self.language.clone()),
            year: self.year.clone(),
            location: self.location.clone(),
            variant: self.variant.clone(),
        }
    }
}

impl FromStr for FormatTemplate {
    type Err = FormatPatternError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = match s.strip_prefix("*-") {
            Some(rest) => rest,
            // §1.4: `*` alone would instantiate into the entity it is given, never refining it.
            None if s == "*" => {
                return Err(FormatPatternError::new(s, Segment::Language, "a template writes at least the language"));
            }
            None => return Err(FormatPatternError::new(s, Segment::Entity, "a template's entity is exactly '*'")),
        };
        let Suffix { language, year, location, variant } = parse_suffix(s, rest)?;
        Ok(FormatTemplate { language, year, location, variant })
    }
}

impl fmt::Display for FormatTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.instantiate("*").fmt(f)
    }
}

/// Everything after the entity's hyphen: the language is always written, the rest may be
/// truncated.
struct Suffix {
    language: String,
    year: Slot,
    location: Slot,
    variant: Slot,
}

impl Suffix {
    fn with_entity(self, entity: &str) -> FormatPattern {
        FormatPattern {
            entity: entity.to_string(),
            language: Slot::Fixed(self.language),
            year: self.year,
            location: self.location,
            variant: self.variant,
        }
    }
}

fn parse_partial(s: &str) -> Result<FormatPattern, FormatPatternError> {
    let (entity, suffix) = split_entity(s);
    check_partial_entity(s, entity)?;
    match suffix {
        Some(suffix) => Ok(parse_suffix(s, suffix)?.with_entity(entity)),
        None => Ok(FormatPattern {
            entity: entity.to_string(),
            language: Slot::Unknown,
            year: Slot::Unknown,
            location: Slot::Unknown,
            variant: Slot::Unknown,
        }),
    }
}

/// The entity and what follows its hyphen, if any.
///
/// The entity ends at the **last** hyphen before the first `@` or `.` that follows a hyphen, not
/// at the first one: a hyphen inside the entity is then diagnosed on the entity (`FOO-BAR-IT`)
/// rather than as a malformed language, while a hyphen inside the variant (`X-IT24@*.A-B`) stays
/// the variant's.
fn split_entity(s: &str) -> (&str, Option<&str>) {
    let Some(first_hyphen) = s.find('-') else {
        return (s, None);
    };
    let cut = s[first_hyphen..].find(['@', '.']).map_or(s.len(), |offset| first_hyphen + offset);
    let hyphen = s[..cut].rfind('-').unwrap_or(first_hyphen);
    (&s[..hyphen], Some(&s[hyphen + 1..]))
}

fn check_partial_entity(input: &str, entity: &str) -> Result<(), FormatPatternError> {
    let reason = if entity.is_empty() {
        "it is missing: a pattern is truncated only on the right"
    } else if entity == "*" {
        "'*' as entity is a format template, not a pattern"
    } else if entity.contains('*') {
        "'*' stands only for a whole location or variant"
    } else if entity.contains('-') {
        "an entity containing '-' is readable only in a complete name"
    } else if entity.contains(['@', '.']) || entity.contains(char::is_whitespace) {
        "an entity containing '@', '.' or spaces is readable only in a complete name"
    } else {
        return Ok(());
    };
    Err(FormatPatternError::new(input, Segment::Entity, reason))
}

/// A complete name read the way the format-name grammar reads it: any non-empty entity, split at
/// the rightmost hyphen the rest of the name can follow — the greedy `.+` of
/// `FORMAT_NAME_PATTERN`.
fn parse_complete_with_any_entity(s: &str) -> Option<FormatPattern> {
    if s.contains('*') {
        return None;
    }
    s.rmatch_indices('-').filter(|&(hyphen, _)| hyphen > 0).find_map(|(hyphen, _)| {
        let suffix = parse_suffix(s, &s[hyphen + 1..]).ok()?;
        matches!(suffix.year, Slot::Fixed(_)).then(|| suffix.with_entity(&s[..hyphen]))
    })
}

/// `LL`, or `LLYY` followed by an optional location and an optional variant.
///
/// The year runs up to the first `@` or `.`, the location up to the next `.`, the variant to the
/// end: each segment is cut at its delimiter before being checked, so the error names the segment
/// the stray character landed in.
fn parse_suffix(input: &str, suffix: &str) -> Result<Suffix, FormatPatternError> {
    let error = |segment, reason| FormatPatternError::new(input, segment, reason);
    let (head, tail) = suffix.split_at(suffix.find(['@', '.']).unwrap_or(suffix.len()));
    let language = head
        .get(..2)
        .filter(|language| language.bytes().all(|b| b.is_ascii_uppercase()))
        .ok_or_else(|| error(Segment::Language, "expected two capital letters"))?;
    let year = &head[2..];
    if year.is_empty() {
        if !tail.is_empty() {
            return Err(error(Segment::Year, "a location or a variant needs the year before it"));
        }
        return Ok(Suffix {
            language: language.to_string(),
            year: Slot::Unknown,
            location: Slot::Unknown,
            variant: Slot::Unknown,
        });
    }
    if !(year.len() == 2 && year.bytes().all(|b| b.is_ascii_digit())) {
        return Err(error(Segment::Year, "expected two digits"));
    }
    let (location, tail) = match tail.strip_prefix('@') {
        Some(rest) => {
            let (location, tail) = rest.split_at(rest.find('.').unwrap_or(rest.len()));
            let is_location = (2..=3).contains(&location.len()) && location.bytes().all(|b| b.is_ascii_uppercase());
            let location = open_slot(location, is_location)
                .ok_or_else(|| error(Segment::Location, "expected '*' or two or three capital letters"))?;
            (location, tail)
        }
        None => (Slot::Absent, tail),
    };
    let variant = match tail.strip_prefix('.') {
        Some(variant) => {
            let is_variant = !variant.is_empty() && !variant.contains(['.', '/', '*']);
            open_slot(variant, is_variant)
                .ok_or_else(|| error(Segment::Variant, "expected '*' or a non-empty text without '.', '/' or '*'"))?
        }
        None => Slot::Absent,
    };
    Ok(Suffix { language: language.to_string(), year: Slot::Fixed(year.to_string()), location, variant })
}

/// An optional slot that was written: `*` is `Required`, a well-formed value is `Fixed`.
fn open_slot(text: &str, is_well_formed: bool) -> Option<Slot> {
    match text {
        "*" => Some(Slot::Required),
        _ if is_well_formed => Some(Slot::Fixed(text.to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_repo::id_format::{IdFormat, id_matches};

    /// The 25 formats `analysis_finance_reports_formats/metadata/formats.csv` declares, as
    /// `get_formats` synthesises them, in file order (copied 2026-10-10).
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

    /// A spread of patterns over every slot state and every truncation, for the order properties.
    const REPRESENTATIVE: [&str; 27] = [
        "MEDIOLANUM",
        "MEDIOLANUM-IT",
        "MEDIOLANUM-EN",
        "MEDIOLANUM-IT24",
        "MEDIOLANUM-IT24.*",
        "MEDIOLANUM-IT24.A",
        "MEDIOLANUM-IT24.B",
        "MEDIOLANUM-IT24@*",
        "MEDIOLANUM-IT24@*.*",
        "MEDIOLANUM-IT24@ES",
        "MEDIOLANUM-IT24@ES.*",
        "MEDIOLANUM-ES24.*",
        "FINECO",
        "FINECO-EN",
        "FINECO-EN23",
        "FINECO-EN23@*",
        "FINECO-EN23@LUX",
        "FINECO-EN23@*.*",
        "FINECO-EN23@LUX.*",
        "FINECO-EN23@*.B",
        "FINECO-EN23@LUX.B",
        "ANIMA",
        "ANIMA-EN23",
        "ANIMA_SGR",
        "X-IT23@LUX.*",
        "X-IT23@*.B",
        "X-IT23@LUX.B",
    ];

    fn p(s: &str) -> FormatPattern {
        s.parse().unwrap_or_else(|e| panic!("{s} should parse: {e}"))
    }

    fn t(s: &str) -> FormatTemplate {
        s.parse().unwrap_or_else(|e| panic!("{s} should parse as a template: {e}"))
    }

    fn err(s: &str) -> FormatPatternError {
        match s.parse::<FormatPattern>() {
            Ok(parsed) => panic!("{s:?} should be rejected, got {parsed:?}"),
            Err(e) => e,
        }
    }

    fn fixed(v: &str) -> Slot {
        Slot::Fixed(v.to_string())
    }

    fn real_formats() -> Vec<String> {
        REAL_FORMATS.iter().map(|s| s.to_string()).collect()
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn representative() -> Vec<FormatPattern> {
        REPRESENTATIVE.iter().map(|s| p(s)).collect()
    }

    /// The five slots at once, for compact assertions.
    fn slots(pattern: &FormatPattern) -> (String, Slot, Slot, Slot, Slot) {
        (
            pattern.entity().to_string(),
            pattern.language().clone(),
            pattern.year().clone(),
            pattern.location().clone(),
            pattern.variant().clone(),
        )
    }

    fn expected(entity: &str, language: Slot, year: Slot, location: Slot, variant: Slot) -> (String, Slot, Slot, Slot, Slot) {
        (entity.to_string(), language, year, location, variant)
    }

    /// §1.2: the slot states each truncation of the hierarchy leaves.
    mod truncations {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_entity_alone_leaves_every_other_slot_unknown() {
            assert_eq!(
                slots(&p("MEDIOLANUM")),
                expected("MEDIOLANUM", Slot::Unknown, Slot::Unknown, Slot::Unknown, Slot::Unknown)
            );
        }

        #[test]
        fn truncating_after_the_language_leaves_year_location_and_variant_unknown() {
            assert_eq!(
                slots(&p("MEDIOLANUM-IT")),
                expected("MEDIOLANUM", fixed("IT"), Slot::Unknown, Slot::Unknown, Slot::Unknown)
            );
        }

        #[test]
        fn a_name_reaching_the_year_leaves_location_and_variant_absent_not_unknown() {
            assert_eq!(
                slots(&p("MEDIOLANUM-IT24")),
                expected("MEDIOLANUM", fixed("IT"), fixed("24"), Slot::Absent, Slot::Absent)
            );
        }

        #[test]
        fn a_location_without_variant_leaves_the_variant_absent() {
            assert_eq!(
                slots(&p("FINECO-EN23@LUX")),
                expected("FINECO", fixed("EN"), fixed("23"), fixed("LUX"), Slot::Absent)
            );
        }

        #[test]
        fn a_variant_without_location_leaves_the_location_absent() {
            assert_eq!(
                slots(&p("MEDIOLANUM-IT24.A")),
                expected("MEDIOLANUM", fixed("IT"), fixed("24"), Slot::Absent, fixed("A"))
            );
        }

        #[test]
        fn location_and_variant_together_fix_every_slot() {
            assert_eq!(
                slots(&p("X-IT23@LUX.B")),
                expected("X", fixed("IT"), fixed("23"), fixed("LUX"), fixed("B"))
            );
        }

        #[test]
        fn a_two_letter_location_is_accepted() {
            assert_eq!(slots(&p("FINECO-EN23@IR")).3, fixed("IR"));
        }

        #[test]
        fn a_year_keeps_its_leading_zero() {
            assert_eq!(slots(&p("X-IT05")).2, fixed("05"));
        }

        #[test]
        fn the_language_fixed_after_truncation_is_the_one_written() {
            assert_eq!(slots(&p("MEDIOLANUM-ES")).1, fixed("ES"));
        }

        #[test]
        fn an_entity_may_contain_underscores_and_digits() {
            assert_eq!(slots(&p("ANIMA_SGR2")).0, "ANIMA_SGR2");
        }
    }

    /// §1.2: `@*` and `.*` make a slot `Required`; an unwritten one after the year stays `Absent`.
    mod wildcards {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn at_star_requires_the_location_and_leaves_the_variant_absent() {
            assert_eq!(
                slots(&p("FIDEURAM-IT23@*")),
                expected("FIDEURAM", fixed("IT"), fixed("23"), Slot::Required, Slot::Absent)
            );
        }

        #[test]
        fn dot_star_requires_the_variant_and_leaves_the_location_absent() {
            assert_eq!(
                slots(&p("MEDIOLANUM-IT24.*")),
                expected("MEDIOLANUM", fixed("IT"), fixed("24"), Slot::Absent, Slot::Required)
            );
        }

        #[test]
        fn at_star_dot_star_requires_both() {
            assert_eq!(
                slots(&p("FIDEURAM-IT23@*.*")),
                expected("FIDEURAM", fixed("IT"), fixed("23"), Slot::Required, Slot::Required)
            );
        }

        #[test]
        fn a_fixed_location_with_dot_star() {
            assert_eq!(
                slots(&p("X-IT23@LUX.*")),
                expected("X", fixed("IT"), fixed("23"), fixed("LUX"), Slot::Required)
            );
        }

        #[test]
        fn at_star_with_a_fixed_variant() {
            assert_eq!(
                slots(&p("X-IT23@*.B")),
                expected("X", fixed("IT"), fixed("23"), Slot::Required, fixed("B"))
            );
        }

        #[test]
        fn a_variant_may_contain_a_hyphen_even_in_a_partial() {
            // The variant's form is `[^./]+`: a hyphen in it is not an entity hyphen.
            assert_eq!(
                slots(&p("X-IT24@*.A-B")),
                expected("X", fixed("IT"), fixed("24"), Slot::Required, fixed("A-B"))
            );
        }
    }

    /// §1.2, last two bullets: every complete name of today stays complete and unchanged.
    mod complete_names {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_real_format_parses_as_a_complete_pattern() {
            for name in REAL_FORMATS {
                assert!(p(name).is_complete(), "{name} should be complete");
            }
        }

        #[test]
        fn every_real_format_renders_back_unchanged() {
            for name in REAL_FORMATS {
                assert_eq!(p(name).to_string(), name);
            }
        }

        #[test]
        fn every_real_format_still_matches_the_id_grammar() {
            // Regression guard: `id_format.rs` does not change behaviour.
            for name in REAL_FORMATS {
                assert!(id_matches(name, IdFormat::ExpandableNoIndex), "{name}");
            }
        }

        #[test]
        fn a_complete_name_with_a_hyphen_in_the_entity_is_readable() {
            let pattern = p("FOO-BAR-IT24");
            assert!(pattern.is_complete());
            assert_eq!(
                slots(&pattern),
                expected("FOO-BAR", fixed("IT"), fixed("24"), Slot::Absent, Slot::Absent)
            );
            assert_eq!(pattern.to_string(), "FOO-BAR-IT24");
        }

        #[test]
        fn a_name_with_two_readings_takes_the_strict_one() {
            // `A-IT24.B-EN23` reads two ways: entity `A` with variant `B-EN23` (the partial
            // grammar's entity, split from the right), or entity `A-IT24.B` with `EN23` (what the
            // greedy `.+` of `FORMAT_NAME_PATTERN` would pick). The strict reading wins, so that a
            // name never parses differently from the partials written beside it.
            assert_eq!(
                slots(&p("A-IT24.B-EN23")),
                expected("A", fixed("IT"), fixed("24"), Slot::Absent, fixed("B-EN23"))
            );
        }

        #[test]
        fn a_complete_name_with_a_hyphenated_entity_and_both_suffixes_is_readable() {
            assert_eq!(
                slots(&p("FOO-BAR-IT24@LUX.A")),
                expected("FOO-BAR", fixed("IT"), fixed("24"), fixed("LUX"), fixed("A"))
            );
        }

        #[test]
        fn a_complete_name_with_space_dot_or_at_in_the_entity_is_readable() {
            // FORMAT_NAME_PATTERN opens with `.+`: everything it accepts without `*` is complete.
            for (name, entity) in [("FOO BAR-IT24", "FOO BAR"), ("FOO.BAR-IT24", "FOO.BAR"), ("FOO@BAR-IT24", "FOO@BAR")] {
                let pattern = p(name);
                assert!(pattern.is_complete(), "{name}");
                assert_eq!(pattern.entity(), entity);
                assert_eq!(pattern.to_string(), name);
            }
        }

        #[test]
        fn a_lowercase_variant_is_accepted_as_today() {
            assert_eq!(slots(&p("MEDIOLANUM-IT24@ES.b")).4, fixed("b"));
        }

        #[test]
        fn every_name_the_id_grammar_accepts_without_a_star_is_a_complete_pattern() {
            let names = [
                "AMUNDI-EN24",
                "MEDIOLANUM-IT24@ES.b",
                "MEDIOLANUM-IT24.b",
                "FOO-BAR-IT24",
                "FOO-BAR-IT24@LUX.A",
                "X-IT24.A-B",
                "X-IT05",
                "FOO BAR-IT24",
            ];
            for name in names {
                assert!(id_matches(name, IdFormat::ExpandableNoIndex), "precondition: {name}");
                let pattern = p(name);
                assert!(pattern.is_complete(), "{name}");
                assert_eq!(pattern.to_string(), name);
            }
        }

        #[test]
        fn every_complete_pattern_matches_the_id_grammar() {
            let candidates = [
                "MEDIOLANUM",
                "MEDIOLANUM-IT",
                "MEDIOLANUM-IT24",
                "MEDIOLANUM-IT24.*",
                "FIDEURAM-IT23@*",
                "FIDEURAM-IT23@*.*",
                "X-IT23@LUX.*",
                "X-IT23@*.B",
                "X-IT23@LUX.B",
                "FOO-BAR-IT24",
                "X-IT24@*.A-B",
            ];
            for name in candidates {
                let pattern = p(name);
                if pattern.is_complete() {
                    assert!(id_matches(name, IdFormat::ExpandableNoIndex), "{name}");
                }
            }
        }
    }

    /// §1.2: the forms that are not patterns, and that the error names the segment it could not read.
    mod rejected_forms {
        use super::*;
        use pretty_assertions::assert_eq;

        fn assert_segment(input: &str, segment: Segment) {
            let e = err(input);
            assert_eq!(e.segment, segment, "{input}: {e}");
        }

        #[test]
        fn the_empty_string_is_rejected_on_the_entity() {
            assert_segment("", Segment::Entity);
        }

        #[test]
        fn a_missing_entity_is_rejected_on_the_entity() {
            // Truncation is only from the right.
            assert_segment("-IT24", Segment::Entity);
        }

        #[test]
        fn a_year_without_language_is_rejected_on_the_language() {
            assert_segment("MEDIOLANUM-24", Segment::Language);
        }

        #[test]
        fn a_lowercase_language_is_rejected_on_the_language() {
            assert_segment("X-it24", Segment::Language);
            assert_segment("X-it", Segment::Language);
        }

        #[test]
        fn a_one_letter_language_is_rejected_on_the_language() {
            assert_segment("X-I", Segment::Language);
        }

        #[test]
        fn a_one_digit_year_is_rejected_on_the_year() {
            assert_segment("X-IT2", Segment::Year);
        }

        #[test]
        fn an_empty_location_is_rejected_on_the_location() {
            assert_segment("X-IT24@", Segment::Location);
        }

        #[test]
        fn a_one_letter_location_is_rejected_on_the_location() {
            assert_segment("X-IT24@L", Segment::Location);
        }

        #[test]
        fn a_four_letter_location_is_rejected_on_the_location() {
            assert_segment("X-IT24@LUXX", Segment::Location);
        }

        #[test]
        fn a_lowercase_location_is_rejected_on_the_location() {
            assert_segment("X-IT24@lux", Segment::Location);
        }

        #[test]
        fn an_empty_variant_is_rejected_on_the_variant() {
            assert_segment("X-IT24.", Segment::Variant);
            assert_segment("X-IT24@*.", Segment::Variant);
        }

        #[test]
        fn a_variant_with_a_slash_is_rejected_on_the_variant() {
            assert_segment("X-IT24.A/B", Segment::Variant);
        }

        #[test]
        fn a_hyphen_in_the_entity_of_a_partial_is_rejected_on_the_entity() {
            // §1.2, accepted limit: in a partial `FOO-BA…` would be ambiguous.
            assert_segment("FOO-BAR-IT24@*", Segment::Entity);
            assert_segment("FOO-BAR-IT24.*", Segment::Entity);
            assert_segment("FOO-BAR-IT", Segment::Entity);
        }

        #[test]
        fn a_space_at_or_dot_in_the_entity_of_a_partial_is_rejected_on_the_entity() {
            assert_segment("FOO BAR-IT", Segment::Entity);
            assert_segment("FOO@BAR-IT", Segment::Entity);
            assert_segment("FOO.BAR-IT", Segment::Entity);
            assert_segment("FOO BAR", Segment::Entity);
            assert_segment("FOO.BAR", Segment::Entity);
            assert_segment("FOO@BAR", Segment::Entity);
        }

        #[test]
        fn a_star_inside_the_entity_is_rejected_on_the_entity() {
            assert_segment("F*O-IT24", Segment::Entity);
            assert_segment("FOO*", Segment::Entity);
        }

        #[test]
        fn a_star_as_entity_is_a_template_not_a_pattern() {
            for input in ["*-EN23", "*-EN", "*-EN23@*", "*"] {
                let e = err(input);
                assert_eq!(e.segment, Segment::Entity, "{input}: {e}");
            }
            let message = err("*-EN23").to_string().to_lowercase();
            assert!(message.contains("template"), "the error should point to the template: {message}");
        }

        #[test]
        fn a_star_anywhere_else_than_a_whole_location_or_variant_is_rejected() {
            assert_segment("X-*", Segment::Language);
            assert_segment("X-I*", Segment::Language);
            assert_segment("X-IT*4", Segment::Year);
            assert_segment("X-IT24@L*", Segment::Location);
            assert_segment("X-IT24@*X", Segment::Location);
            assert_segment("X-IT24.A*", Segment::Variant);
            assert_segment("X-IT24.**", Segment::Variant);
        }

        #[test]
        fn other_malformed_forms_are_rejected() {
            for input in [
                "X-",
                "X-IT245",
                "X-ITA24",
                "X-IT24@LUX@IR",
                "X-IT24@*@*",
                "X-IT24.*.*",
                "X-IT24.A.B",
                "X-IT24 ",
                "X-IT24(investments)",
                "X-IT24/0",
                "X-IT@LUX",
                "X-IT.A",
                "X@LUX",
                "X.A",
            ] {
                assert!(input.parse::<FormatPattern>().is_err(), "{input:?} should be rejected");
            }
        }
    }

    /// The error's message: what was not understood, where, and why.
    mod error_messages {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_message_names_the_segment() {
            for (input, name) in [
                ("-IT24", "entity"),
                ("X-it24", "language"),
                ("X-IT2", "year"),
                ("X-IT24@LUXX", "location"),
                ("X-IT24.", "variant"),
            ] {
                let message = err(input).to_string();
                assert!(message.contains(name), "{input}: {message}");
            }
        }

        #[test]
        fn the_message_quotes_the_input() {
            let message = err("X-IT24@LUXX").to_string();
            assert!(message.contains("X-IT24@LUXX"), "{message}");
        }

        #[test]
        fn the_message_carries_a_reason() {
            let e = err("X-IT2");
            assert!(!e.reason.trim().is_empty(), "{e:?}");
        }

        #[test]
        fn the_message_for_a_hyphenated_entity_mentions_the_hyphen() {
            let message = err("FOO-BAR-IT24@*").to_string();
            assert!(message.contains('-'), "{message}");
            assert!(message.contains("entity"), "{message}");
        }

        #[test]
        fn segments_render_as_their_lowercase_names() {
            assert_eq!(
                [Segment::Entity, Segment::Language, Segment::Year, Segment::Location, Segment::Variant]
                    .map(|s| s.to_string()),
                ["entity", "language", "year", "location", "variant"].map(String::from)
            );
        }
    }

    /// `Display` renders the canonical form, and parsing it gives the pattern back.
    mod round_trip {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn rendering_a_parsed_pattern_gives_the_input_back() {
            for name in REPRESENTATIVE.iter().chain(REAL_FORMATS.iter()) {
                assert_eq!(p(name).to_string(), *name);
            }
        }

        #[test]
        fn parsing_a_rendered_pattern_gives_the_pattern_back() {
            for pattern in representative() {
                assert_eq!(p(&pattern.to_string()), pattern);
            }
        }

        #[test]
        fn a_meet_renders_into_a_pattern_that_parses_back() {
            // §1.5: the meet is always expressible in the grammar of §1.2.
            let all = representative();
            for a in &all {
                for b in &all {
                    if let Some(m) = a.meet(b) {
                        assert_eq!(p(&m.to_string()), m, "{a} ⊓ {b}");
                    }
                }
            }
        }
    }

    mod completeness {
        use super::*;

        #[test]
        fn truncated_patterns_are_not_complete() {
            assert!(!p("MEDIOLANUM").is_complete());
            assert!(!p("MEDIOLANUM-IT").is_complete());
        }

        #[test]
        fn a_pattern_with_a_required_slot_is_not_complete() {
            assert!(!p("FIDEURAM-IT23@*").is_complete());
            assert!(!p("MEDIOLANUM-IT24.*").is_complete());
            assert!(!p("X-IT23@LUX.*").is_complete());
            assert!(!p("X-IT23@*.B").is_complete());
        }

        #[test]
        fn a_name_reaching_the_year_is_complete_even_if_no_format_has_it() {
            // §1.3: `MEDIOLANUM-IT24` is complete; whether it exists is the loader's business.
            assert!(p("MEDIOLANUM-IT24").is_complete());
            assert!(p("FINECO-EN23").is_complete());
        }

        #[test]
        fn complete_names_with_suffixes_are_complete() {
            assert!(p("FINECO-EN23@LUX").is_complete());
            assert!(p("MEDIOLANUM-IT24.A").is_complete());
            assert!(p("X-IT23@LUX.B").is_complete());
        }
    }

    /// §1.5, `refines`: the slot-by-slot rules one by one.
    mod refines_slot_rules {
        use super::*;

        fn r(q: &str, other: &str) -> bool {
            p(q).refines(&p(other))
        }

        #[test]
        fn a_fixed_slot_is_refined_only_by_the_same_value() {
            assert!(r("X-IT", "X-IT"));
            assert!(!r("X-EN", "X-IT"));
            assert!(r("X-IT24", "X-IT24") && !r("X-IT23", "X-IT24"));
            assert!(r("X-IT24@LUX", "X-IT24@LUX") && !r("X-IT24@IR", "X-IT24@LUX"));
            assert!(r("X-IT24.A", "X-IT24.A") && !r("X-IT24.B", "X-IT24.A"));
        }

        #[test]
        fn a_fixed_slot_is_not_refined_by_an_open_one() {
            assert!(!r("X", "X-IT"));
            assert!(!r("X-IT", "X-IT24"));
            assert!(!r("X-IT24@*", "X-IT24@LUX"));
            assert!(!r("X-IT24.*", "X-IT24.A"));
        }

        #[test]
        fn a_fixed_slot_is_not_refined_by_an_absent_one() {
            assert!(!r("X-IT24", "X-IT24@LUX"));
            assert!(!r("X-IT24", "X-IT24.A"));
        }

        #[test]
        fn the_entity_is_compared_whole_not_as_a_prefix() {
            assert!(!r("ANIMA_SGR-IT24", "ANIMA"));
            assert!(!r("Y-IT24", "X"));
            assert!(r("ANIMA-EN23", "ANIMA"));
        }

        #[test]
        fn an_absent_slot_is_refined_only_by_absent() {
            assert!(r("FINECO-EN23", "FINECO-EN23"));
            assert!(!r("FINECO-EN23@LUX", "FINECO-EN23"));
            assert!(!r("FINECO-EN23@*", "FINECO-EN23"));
            assert!(!r("MEDIOLANUM-IT24.A", "MEDIOLANUM-IT24"));
            assert!(!r("MEDIOLANUM-IT24.*", "MEDIOLANUM-IT24"));
        }

        #[test]
        fn an_absent_slot_is_not_refined_by_unknown() {
            // An open-ended truncation is more general than a decided absence.
            assert!(!r("X-IT", "X-IT24"));
        }

        #[test]
        fn a_required_slot_is_refined_by_required_or_fixed() {
            assert!(r("FINECO-EN23@*", "FINECO-EN23@*"));
            assert!(r("FINECO-EN23@LUX", "FINECO-EN23@*"));
            assert!(r("MEDIOLANUM-IT24.*", "MEDIOLANUM-IT24.*"));
            assert!(r("MEDIOLANUM-IT24.A", "MEDIOLANUM-IT24.*"));
        }

        #[test]
        fn a_required_slot_is_not_refined_by_absent() {
            assert!(!r("FINECO-EN23", "FINECO-EN23@*"));
            assert!(!r("MEDIOLANUM-IT24", "MEDIOLANUM-IT24.*"));
        }

        #[test]
        fn a_required_slot_is_not_refined_by_unknown() {
            assert!(!r("FINECO-EN", "FINECO-EN23@*"));
        }

        #[test]
        fn an_absent_variant_under_at_star_is_not_refined_by_a_variant() {
            // R2: under `FIDEURAM-IT23@*` (variant absent) `@LUX.A` does not refine.
            assert!(!r("FIDEURAM-IT23@LUX.A", "FIDEURAM-IT23@*"));
            assert!(r("FIDEURAM-IT23@LUX.A", "FIDEURAM-IT23@*.*"));
        }

        #[test]
        fn an_unknown_slot_is_refined_by_anything() {
            for q in ["X", "X-IT", "X-EN", "X-IT24", "X-IT24@*", "X-IT24.*", "X-IT24@LUX.A", "X-IT24@*.*"] {
                assert!(r(q, "X"), "{q} ⊑ X");
            }
            for q in ["X-IT", "X-IT24", "X-IT23@*", "X-IT24.A", "X-IT24@LUX.*"] {
                assert!(r(q, "X-IT"), "{q} ⊑ X-IT");
            }
        }

        #[test]
        fn a_refinement_needs_every_slot_to_agree() {
            assert!(r("X-IT23@LUX.B", "X-IT23@LUX.*"));
            assert!(r("X-IT23@LUX.B", "X-IT23@*.B"));
            assert!(!r("X-IT23@LUX.*", "X-IT23@*.B"));
            assert!(!r("X-IT23@*.B", "X-IT23@LUX.*"));
        }
    }

    /// §1.5: `refines` is a partial order, checked on every pair and triple of a representative set.
    mod refines_order_properties {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn is_reflexive() {
            for a in representative() {
                assert!(a.refines(&a), "{a}");
            }
        }

        #[test]
        fn is_antisymmetric() {
            let all = representative();
            for a in &all {
                for b in &all {
                    if a.refines(b) && b.refines(a) {
                        assert_eq!(a, b);
                    }
                }
            }
        }

        #[test]
        fn is_transitive() {
            let all = representative();
            for a in &all {
                for b in &all {
                    if !a.refines(b) {
                        continue;
                    }
                    for c in &all {
                        if b.refines(c) {
                            assert!(a.refines(c), "{a} ⊑ {b} ⊑ {c}");
                        }
                    }
                }
            }
        }

        #[test]
        fn a_complete_pattern_is_refined_only_by_itself() {
            let all = representative();
            for a in all.iter().filter(|a| a.is_complete()) {
                for b in &all {
                    if b.refines(a) {
                        assert_eq!(b, a);
                    }
                }
            }
        }
    }

    mod strictly_refines {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_pattern_does_not_strictly_refine_itself() {
            for a in representative() {
                assert!(!a.strictly_refines(&a), "{a}");
            }
        }

        #[test]
        fn a_proper_refinement_is_strict() {
            assert!(p("MEDIOLANUM-IT").strictly_refines(&p("MEDIOLANUM")));
            assert!(p("MEDIOLANUM-IT24.A").strictly_refines(&p("MEDIOLANUM-IT24.*")));
            assert!(p("FINECO-EN23@LUX").strictly_refines(&p("FINECO-EN23@*")));
        }

        #[test]
        fn the_reverse_of_a_refinement_is_not_strict() {
            assert!(!p("MEDIOLANUM").strictly_refines(&p("MEDIOLANUM-IT")));
        }

        #[test]
        fn unrelated_patterns_do_not_strictly_refine_each_other() {
            assert!(!p("X-IT23@LUX.*").strictly_refines(&p("X-IT23@*.B")));
        }

        #[test]
        fn is_refines_minus_equality_on_every_pair() {
            let all = representative();
            for a in &all {
                for b in &all {
                    assert_eq!(a.strictly_refines(b), a.refines(b) && a != b, "{a} vs {b}");
                }
            }
        }
    }

    /// §1.5, the meet, slot by slot.
    mod meet_slot_rules {
        use super::*;
        use pretty_assertions::assert_eq;

        fn m(a: &str, b: &str) -> Option<String> {
            p(a).meet(&p(b)).map(|x| x.to_string())
        }

        #[test]
        fn unknown_meets_anything_as_the_other_slot() {
            assert_eq!(m("X", "X-IT24@*"), Some("X-IT24@*".to_string()));
            assert_eq!(m("X-IT", "X-IT24.A"), Some("X-IT24.A".to_string()));
            assert_eq!(m("X", "X"), Some("X".to_string()));
        }

        #[test]
        fn required_meets_fixed_as_fixed() {
            assert_eq!(m("X-IT24@*", "X-IT24@LUX"), Some("X-IT24@LUX".to_string()));
            assert_eq!(m("X-IT24.*", "X-IT24.A"), Some("X-IT24.A".to_string()));
        }

        #[test]
        fn required_meets_required_as_required() {
            assert_eq!(m("X-IT24@*.*", "X-IT24@*.*"), Some("X-IT24@*.*".to_string()));
        }

        #[test]
        fn absent_meets_unknown_as_absent() {
            assert_eq!(m("X-IT", "X-IT24"), Some("X-IT24".to_string()));
            assert_eq!(m("X", "X-IT24@LUX"), Some("X-IT24@LUX".to_string()));
        }

        #[test]
        fn absent_meets_absent_as_absent() {
            assert_eq!(m("X-IT24", "X-IT24"), Some("X-IT24".to_string()));
        }

        #[test]
        fn absent_and_required_have_no_meet() {
            assert_eq!(m("X-IT24", "X-IT24@*"), None);
            assert_eq!(m("X-IT24.*", "X-IT24"), None);
            assert_eq!(m("FIDEURAM-IT23@*", "FIDEURAM-IT23@*.*"), None);
        }

        #[test]
        fn absent_and_fixed_have_no_meet() {
            assert_eq!(m("X-IT24", "X-IT24@LUX"), None);
            assert_eq!(m("X-IT24.A", "X-IT24"), None);
        }

        #[test]
        fn two_different_fixed_values_have_no_meet() {
            assert_eq!(m("X", "Y"), None);
            assert_eq!(m("X-IT", "X-EN"), None);
            assert_eq!(m("X-IT23", "X-IT24"), None);
            assert_eq!(m("X-IT24@LUX", "X-IT24@IR"), None);
            assert_eq!(m("X-IT24.A", "X-IT24.B"), None);
        }

        #[test]
        fn equal_fixed_values_meet_as_themselves() {
            assert_eq!(m("X-IT23@LUX.B", "X-IT23@LUX.B"), Some("X-IT23@LUX.B".to_string()));
        }

        #[test]
        fn two_incomparable_patterns_meet_in_their_common_refinement() {
            // R10.
            assert_eq!(m("X-IT23@LUX.*", "X-IT23@*.B"), Some("X-IT23@LUX.B".to_string()));
        }

        #[test]
        fn a_truncation_and_a_wildcard_meet_slot_by_slot() {
            assert_eq!(m("MEDIOLANUM-IT", "MEDIOLANUM-IT24@*.*"), Some("MEDIOLANUM-IT24@*.*".to_string()));
            assert_eq!(m("MEDIOLANUM-IT", "MEDIOLANUM-EN24"), None);
        }
    }

    /// §1.5: the meet's lattice properties, and compatibility, on every pair of a representative set.
    mod meet_properties {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn is_commutative() {
            let all = representative();
            for a in &all {
                for b in &all {
                    assert_eq!(a.meet(b), b.meet(a), "{a} ⊓ {b}");
                }
            }
        }

        #[test]
        fn is_idempotent() {
            for a in representative() {
                assert_eq!(a.meet(&a), Some(a.clone()));
            }
        }

        #[test]
        fn of_a_refinement_and_its_ancestor_is_the_refinement() {
            let all = representative();
            for a in &all {
                for b in &all {
                    if a.refines(b) {
                        assert_eq!(a.meet(b), Some(a.clone()), "{a} ⊑ {b}");
                    }
                }
            }
        }

        #[test]
        fn refines_both_operands() {
            let all = representative();
            for a in &all {
                for b in &all {
                    if let Some(m) = a.meet(b) {
                        assert!(m.refines(a) && m.refines(b), "{a} ⊓ {b} = {m}");
                    }
                }
            }
        }

        #[test]
        fn is_the_most_general_common_refinement() {
            let all = representative();
            for a in &all {
                for b in &all {
                    for c in &all {
                        if c.refines(a) && c.refines(b) {
                            let m = a.meet(b).unwrap_or_else(|| panic!("{c} refines {a} and {b}, the meet must exist"));
                            assert!(c.refines(&m), "{c} should refine {a} ⊓ {b} = {m}");
                        }
                    }
                }
            }
        }

        #[test]
        fn compatibility_is_the_existence_of_the_meet() {
            let all = representative();
            for a in &all {
                for b in &all {
                    assert_eq!(a.is_compatible_with(b), a.meet(b).is_some(), "{a} vs {b}");
                }
            }
        }

        #[test]
        fn compatibility_examples() {
            assert!(p("X-IT23@LUX.*").is_compatible_with(&p("X-IT23@*.B")));
            assert!(p("MEDIOLANUM").is_compatible_with(&p("MEDIOLANUM-IT24.A")));
            assert!(!p("MEDIOLANUM-IT24").is_compatible_with(&p("MEDIOLANUM-IT24.*")));
            assert!(!p("MEDIOLANUM").is_compatible_with(&p("EURIZON-IT24")));
        }

        #[test]
        fn the_meet_of_two_patterns_expands_to_the_intersection_of_their_expansions() {
            // §1.5: expand(P ⊓ Q) = expand(P) ∩ expand(Q), on the 25 real formats.
            let formats = real_formats();
            let all = representative();
            for a in &all {
                for b in &all {
                    let ea = a.expand(&formats);
                    let eb = b.expand(&formats);
                    let intersection: Vec<String> = formats.iter().filter(|f| ea.contains(f) && eb.contains(f)).cloned().collect();
                    let em = a.meet(b).map(|m| m.expand(&formats)).unwrap_or_default();
                    assert_eq!(em, intersection, "{a} ⊓ {b}");
                }
            }
        }
    }

    /// §1.5, `expand`: the formats refining the pattern, in the order given.
    mod expand {
        use super::*;
        use pretty_assertions::assert_eq;

        fn e(pattern: &str) -> Vec<String> {
            p(pattern).expand(&real_formats())
        }

        #[test]
        fn an_entity_expands_to_all_its_formats_in_file_order() {
            assert_eq!(
                e("MEDIOLANUM"),
                strings(&[
                    "MEDIOLANUM-EN23",
                    "MEDIOLANUM-ES24.A",
                    "MEDIOLANUM-ES24.B",
                    "MEDIOLANUM-IT24.A",
                    "MEDIOLANUM-EN24",
                    "MEDIOLANUM-IT24.B",
                ])
            );
        }

        #[test]
        fn a_required_variant_expands_to_every_variant_in_file_order() {
            assert_eq!(
                e("MEDIOLANUM-IT24.*"),
                strings(&["MEDIOLANUM-IT24.A", "MEDIOLANUM-IT24.B"])
            );
            assert_eq!(e("MEDIOLANUM-ES24.*"), strings(&["MEDIOLANUM-ES24.A", "MEDIOLANUM-ES24.B"]));
        }

        #[test]
        fn a_language_truncation_expands_to_every_year_and_suffix() {
            assert_eq!(
                e("MEDIOLANUM-IT"),
                strings(&["MEDIOLANUM-IT24.A", "MEDIOLANUM-IT24.B"])
            );
            assert_eq!(e("MEDIOLANUM-EN"), strings(&["MEDIOLANUM-EN23", "MEDIOLANUM-EN24"]));
            assert_eq!(e("EURIZON-EN"), strings(&["EURIZON-EN23", "EURIZON-EN21"]));
        }

        #[test]
        fn a_required_location_expands_to_every_location() {
            assert_eq!(e("FINECO-EN23@*"), strings(&["FINECO-EN23@LUX", "FINECO-EN23@IR"]));
        }

        #[test]
        fn a_complete_name_expands_to_itself_when_declared() {
            assert_eq!(e("AMUNDI-EN24"), strings(&["AMUNDI-EN24"]));
            assert_eq!(e("FINECO-EN23@LUX"), strings(&["FINECO-EN23@LUX"]));
            assert_eq!(e("MEDIOLANUM-IT24.A"), strings(&["MEDIOLANUM-IT24.A"]));
        }

        #[test]
        fn a_complete_name_not_declared_expands_to_nothing_even_if_formats_refine_its_prefix() {
            // §1.3: `FINECO-EN23` has the location absent, so neither `@LUX` nor `@IR` refines it.
            assert!(e("FINECO-EN23").is_empty());
            assert!(e("MEDIOLANUM-IT24").is_empty());
        }

        #[test]
        fn an_absent_slot_excludes_formats_that_have_it() {
            // `@*` leaves the variant absent; `@*.*` would need a format with both.
            assert!(e("FINECO-EN23@*.*").is_empty());
            assert!(e("MEDIOLANUM-IT24@*").is_empty());
        }

        #[test]
        fn the_entity_is_matched_whole() {
            assert_eq!(e("ANIMA"), strings(&["ANIMA-EN23"]));
            assert_eq!(e("ANIMA_SGR"), strings(&["ANIMA_SGR-IT24"]));
        }

        #[test]
        fn an_unknown_entity_expands_to_nothing() {
            assert!(e("GHOST").is_empty());
        }

        #[test]
        fn nothing_expands_against_an_empty_repository() {
            assert!(p("MEDIOLANUM").expand(&[]).is_empty());
        }

        #[test]
        fn the_order_is_the_one_given_not_a_sorted_one() {
            let formats = strings(&["Z-IT24.B", "Z-IT24.A"]);
            assert_eq!(p("Z-IT24.*").expand(&formats), formats);
        }

        #[test]
        fn a_format_with_a_hyphenated_entity_is_matched_on_its_whole_entity() {
            let formats = strings(&["FOO-BAR-IT24", "FOO-IT24"]);
            assert_eq!(p("FOO").expand(&formats), strings(&["FOO-IT24"]));
            assert_eq!(p("FOO-BAR-IT24").expand(&formats), strings(&["FOO-BAR-IT24"]));
        }

        #[test]
        fn every_real_format_expands_to_exactly_itself() {
            let formats = real_formats();
            for name in REAL_FORMATS {
                assert_eq!(p(name).expand(&formats), strings(&[name]));
            }
        }
    }

    /// §1.5, specificity: the slots neither `Unknown` nor `Required`, the entity included.
    mod specificity {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn counts_the_decided_slots_of_each_truncation() {
            assert_eq!(p("MEDIOLANUM").specificity(), 1);
            assert_eq!(p("MEDIOLANUM-IT").specificity(), 2);
        }

        #[test]
        fn absent_slots_count_as_decided() {
            assert_eq!(p("MEDIOLANUM-IT24").specificity(), 5);
            assert_eq!(p("FINECO-EN23@LUX").specificity(), 5);
            assert_eq!(p("MEDIOLANUM-IT24.A").specificity(), 5);
        }

        #[test]
        fn required_slots_do_not_count() {
            assert_eq!(p("FIDEURAM-IT23@*").specificity(), 4);
            assert_eq!(p("MEDIOLANUM-IT24.*").specificity(), 4);
            assert_eq!(p("FIDEURAM-IT23@*.*").specificity(), 3);
        }

        #[test]
        fn two_incomparable_patterns_can_have_the_same_specificity() {
            // R10: the reason the combination of sources uses the meet, not specificity.
            assert_eq!(p("X-IT23@LUX.*").specificity(), 4);
            assert_eq!(p("X-IT23@*.B").specificity(), 4);
        }

        #[test]
        fn a_pattern_is_complete_exactly_when_its_specificity_is_five() {
            for a in representative() {
                assert_eq!(a.is_complete(), a.specificity() == 5, "{a}");
            }
        }

        #[test]
        fn a_refinement_is_never_less_specific() {
            let all = representative();
            for a in &all {
                for b in &all {
                    if a.refines(b) {
                        assert!(a.specificity() >= b.specificity(), "{a} ⊑ {b}");
                    }
                }
            }
        }
    }

    /// §1.4: `*` as entity is a template, instantiated with an entity.
    mod templates {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_template_with_language_and_year_instantiates_into_a_complete_name() {
            assert_eq!(t("*-EN23").instantiate("FINECO"), p("FINECO-EN23"));
        }

        #[test]
        fn a_template_with_only_the_language_instantiates_into_a_truncation() {
            assert_eq!(t("*-EN").instantiate("MEDIOLANUM"), p("MEDIOLANUM-EN"));
        }

        #[test]
        fn a_template_keeps_its_wildcards() {
            assert_eq!(t("*-EN23@*").instantiate("FINECO"), p("FINECO-EN23@*"));
            assert_eq!(t("*-IT24.*").instantiate("MEDIOLANUM"), p("MEDIOLANUM-IT24.*"));
            assert_eq!(t("*-IT23@*.*").instantiate("FIDEURAM"), p("FIDEURAM-IT23@*.*"));
        }

        #[test]
        fn a_template_keeps_its_fixed_suffixes() {
            assert_eq!(t("*-IT23@LUX.B").instantiate("X"), p("X-IT23@LUX.B"));
        }

        #[test]
        fn the_same_template_instantiates_differently_for_different_entities() {
            let template = t("*-EN23");
            assert_ne!(template.instantiate("FINECO"), template.instantiate("UBS"));
            assert_eq!(template.instantiate("UBS"), p("UBS-EN23"));
        }

        #[test]
        fn a_template_renders_back_unchanged() {
            for s in ["*-EN23", "*-EN", "*-EN23@*", "*-IT24.*", "*-IT23@*.*", "*-IT23@LUX.B"] {
                assert_eq!(t(s).to_string(), s);
                assert_eq!(t(&t(s).to_string()), t(s));
            }
        }

        #[test]
        fn a_star_alone_is_rejected_because_it_writes_no_language() {
            // §1.4, R8: it would instantiate into the entity of `A`, never strictly refining it.
            let e = "*".parse::<FormatTemplate>().unwrap_err();
            assert_eq!(e.segment, Segment::Language, "{e}");
        }

        #[test]
        fn a_pattern_is_not_a_template() {
            for s in ["MEDIOLANUM", "MEDIOLANUM-IT", "FINECO-EN23@*"] {
                let e = s.parse::<FormatTemplate>().unwrap_err();
                assert_eq!(e.segment, Segment::Entity, "{s}: {e}");
            }
        }

        #[test]
        fn a_star_inside_a_longer_entity_is_rejected() {
            for s in ["*X-EN23", "X*-EN23", "**-EN23"] {
                let e = s.parse::<FormatTemplate>().unwrap_err();
                assert_eq!(e.segment, Segment::Entity, "{s}: {e}");
            }
        }

        #[test]
        fn the_rest_of_a_template_follows_the_pattern_grammar() {
            for (s, segment) in [
                ("*-", Segment::Language),
                ("*-en23", Segment::Language),
                ("*-EN2", Segment::Year),
                ("*-EN23@LUXX", Segment::Location),
                ("*-EN23.", Segment::Variant),
                ("*-EN23.A*", Segment::Variant),
            ] {
                let e = s.parse::<FormatTemplate>().unwrap_err();
                assert_eq!(e.segment, segment, "{s}: {e}");
            }
        }
    }
    /// A pattern crosses a worker-process boundary inside an `Extracted::DocClass`: it is
    /// serialised as its canonical string, so that the JSON is readable and the string is the same
    /// one an author writes and `FromStr` reads.
    mod serde_form {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_pattern_serializes_as_its_canonical_string() {
            for name in ["MEDIOLANUM", "MEDIOLANUM-IT", "MEDIOLANUM-IT24.*", "FIDEURAM-IT23@*", "X-EN23@*.*", "AMUNDI-EN24"] {
                assert_eq!(serde_json::to_string(&p(name)).unwrap(), format!("\"{name}\""));
            }
        }

        #[test]
        fn every_representative_pattern_survives_a_json_round_trip() {
            for pattern in representative() {
                let json = serde_json::to_string(&pattern).unwrap();
                let back: FormatPattern = serde_json::from_str(&json).unwrap();
                assert_eq!(back, pattern, "{json}");
            }
        }

        #[test]
        fn a_string_that_is_not_a_pattern_does_not_deserialize() {
            for json in ["\"-IT24\"", "\"*-EN23\"", "\"\""] {
                assert!(serde_json::from_str::<FormatPattern>(json).is_err(), "{json}");
            }
        }

        #[test]
        fn a_non_string_does_not_deserialize() {
            for json in ["3", "null", "{}", "[\"MEDIOLANUM\"]"] {
                assert!(serde_json::from_str::<FormatPattern>(json).is_err(), "{json}");
            }
        }

        /// The deserialisation error says why, with the grammar's own diagnosis.
        #[test]
        fn the_deserialization_error_carries_the_parse_failure() {
            let reason = "-IT24".parse::<FormatPattern>().unwrap_err().to_string();
            let error = serde_json::from_str::<FormatPattern>("\"-IT24\"").unwrap_err().to_string();
            assert!(error.contains(&reason), "{error:?} should contain {reason:?}");
        }
    }
}
