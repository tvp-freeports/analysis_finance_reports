//! Loading the declarative document classification, `metadata/document_classify.yaml`.
//!
//! The loading side only: the steps built here are run by [`crate::core::doc_classify`]. The
//! normative text is §2 (the grammar), §4 (the simplifications and their one normalization) and
//! §1.4 (the constraints of `*`) of `agent-memory/SPEC-document-classification.md`, with the
//! corrections R2, R4, R5, R6 and R15 of its §10.
//!
//! Three stages, each a function of its own:
//!
//! - [`normalize`] reads the text and brings every simplified form to the verbose one (§2.1),
//!   composing each child's suffix with its parent's pattern, so that what comes out is a tree of
//!   [`VerboseStep`]s whose keys are **full** patterns and whose criteria are still the structured
//!   selection form [`InputPdfLineSet`] — comparable, which is what lets two spellings of the same
//!   classification be proved identical;
//! - [`parse`] is [`normalize`] followed by building each criterion with
//!   [`pdfline_selection_from_dict`](crate::input::document::selection::pdfline_selection_from_dict),
//!   into the [`YamlEntry`]s the engine runs;
//! - [`load`] reads `<repo>/metadata/document_classify.yaml` and parses it. A missing file means
//!   *no declarative classification*, never an error.
//!
//! Only the defects of **form** are found here — those decidable from the file alone. Whatever needs
//! `formats.csv` (a complete name that does not exist, a partial expanding to nothing, a `*` no
//! entity instantiates, reachability) is checked at load time by phase 9.

use std::fmt;
use std::num::NonZeroI32;
use std::path::{Path, PathBuf};

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

use crate::core::doc_classify::{Step, StepKey, YamlEntry};
use crate::formats_repo::csv_columns;
use crate::formats_repo::format_pattern::{
    FormatPattern, FormatPatternError, FormatTemplate, Slot,
};
use crate::input::document::selection::{InputPdfLineSet, pdfline_selection_from_dict};

/// The file name of the declarative document classification, inside the repository's
/// [`METADATA_DIR`](super::metadata::METADATA_DIR).
pub const DOCUMENT_CLASSIFY_YAML: &str = "document_classify.yaml";

/// One entry in the verbose form (§2.1): a page, and the steps tried on it, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct VerboseEntry {
    pub page: NonZeroI32,
    pub steps: Vec<VerboseStep>,
}

/// One step in the verbose form: its **full** pattern (or, at the top level only, a `*` template),
/// its criteria — a set, gathered from wherever they were written in the body — and its children,
/// in the order written.
#[derive(Debug, Clone, PartialEq)]
pub struct VerboseStep {
    pub key: StepKey,
    pub criteria: Vec<InputPdfLineSet>,
    pub children: Vec<VerboseStep>,
}

/// Where in the file a defect is.
///
/// - `page`: the page of the entry the defect is in; `None` for a defect in a page key itself, or
///   above the entries;
/// - `steps`: the steps enclosing the defect, outermost first, each by its **full** pattern (or its
///   template, `*-EN23`) — the names the engine reports too;
/// - `key`: the offending key **as written** in the YAML (`21`, `"@LUX"`, `A`), when the defect is
///   a key; `None` when it is a criterion or a body.
///
/// Displayed as the page, the path of the step and the key, in the style of
/// [`super::metadata`]'s row-and-reason errors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct YamlLocation {
    pub page: Option<i64>,
    pub steps: Vec<String>,
    pub key: Option<String>,
}

impl YamlLocation {
    fn on_page(page: NonZeroI32) -> Self {
        YamlLocation {
            page: Some(page.get().into()),
            ..Default::default()
        }
    }

    /// The location of what is read inside `step`.
    fn within(&self, step: &StepKey) -> Self {
        let mut steps = self.steps.clone();
        steps.push(step.to_string());
        YamlLocation {
            page: self.page,
            steps,
            key: None,
        }
    }

    fn broken(&self, kind: RuleError) -> DocClassifyYamlError {
        DocClassifyYamlError::Rule {
            at: self.clone(),
            kind,
        }
    }

    fn broken_at(&self, key: String, kind: RuleError) -> DocClassifyYamlError {
        DocClassifyYamlError::Rule {
            at: YamlLocation {
                key: Some(key),
                ..self.clone()
            },
            kind,
        }
    }
}

impl fmt::Display for YamlLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(page) = self.page {
            parts.push(format!("page {page}"));
        }
        if !self.steps.is_empty() {
            parts.push(format!("step {}", self.steps.join(" > ")));
        }
        if let Some(key) = &self.key {
            parts.push(format!("key '{key}'"));
        }
        match parts.is_empty() {
            true => f.write_str("top level"),
            false => f.write_str(&parts.join(", ")),
        }
    }
}

/// A rule of the grammar (§2, §4, §1.4) the file breaks, decidable from the file alone.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    /// A value of the wrong YAML kind where the grammar wants a list or a mapping: the top level,
    /// the steps of a page, a body, an element of a body.
    #[error("{reason}")]
    Shape { reason: String },
    /// A page key that is not an integer (`1.5`, `uno`).
    #[error("a page is an integer, 1-based, negative counted from the end")]
    PageNotAnInteger,
    /// Page `0`: pages are 1-based, negative from the end, and `0` is neither.
    #[error("there is no page 0: pages are 1-based, negative counted from the end")]
    PageZero,
    /// Two keys of one mapping that are the same step — written the same (what `serde_yaml`
    /// rejects) or normalizing to the same pattern (`21`/`"21"`, `LUX`/`"@LUX"`, `A`/`.A`, §2.3.3).
    /// The message suggests the list form, where repeating a key is an ordered OR.
    #[error(
        "duplicate key: two keys of this mapping name the same step; write the steps as a list to repeat one"
    )]
    DuplicateKey,
    /// An element of the top-level list holding more than one page: one element is one entry
    /// (§2.4).
    #[error(
        "an element of the list of entries is one page: write several pages as several elements"
    )]
    SeveralPages,
    /// A page with no steps (`1: ~`, `1: []`, `1: {}`): an entry that cannot classify anything is
    /// a slip (§2.4).
    #[error("this page has no steps, so it cannot classify anything")]
    PageWithoutSteps,
    /// A child key written with the hyphen of a whole name (`-ES` below `MEDIOLANUM`): child keys
    /// are suffixes without it (§2.4). `suggestion` is the key to write (`ES`).
    #[error("a child key is a suffix without the hyphen: write '{suggestion}'")]
    HyphenatedChildKey { suggestion: String },
    /// A float, boolean or null key where a child key is expected (`24.10:` is the float `24.1`):
    /// the spelling is lost, so the message suggests quoting it (`"24.10":`) rather than guessing.
    #[error("this key is not read as text by YAML (a float, a boolean or a null): quote it")]
    NonStringKey,
    /// A top-level key that is neither a format pattern nor a `*` template.
    #[error("not a format pattern: {reason}")]
    InvalidKey { reason: String },
    /// A child key that does not compose with its parent's pattern, or composes into one that does
    /// not **strictly** refine it (§2.2, R2): it fills an `Absent` slot, repeats the parent, or is
    /// not a pattern suffix at all.
    #[error("this key does not strictly refine its parent '{parent}': {reason}")]
    KeyDoesNotRefineParent { parent: String, reason: String },
    /// A step whose pattern is complete has children: after the year an unwritten slot is
    /// `Absent`, and an `Absent` slot does not refine (§2.2, R2). `suggestion` is the partial to
    /// write instead (`FINECO-EN23@*` for a `"@LUX"` child of `FINECO-EN23`).
    #[error("'{parent}' is a complete name and cannot have children: write '{suggestion}'")]
    CompleteParentWithChildren { parent: String, suggestion: String },
    /// A bare key below `@*.*`, where it could be a location or a variant (`X-EN23@LUX.*` against
    /// `X-EN23@*.LUX`): the separator is mandatory (§2.2, R3b).
    #[error("below '@*.*' a key needs its separator: '@' for a location, '.' for a variant")]
    SeparatorRequired,
    /// A `*` template below the top level, where the entity is already fixed by the root (§1.4).
    #[error("a '*' template is allowed only as a top-level step")]
    TemplateNotAtTopLevel,
    /// A `*` template with children: the root of a block must be a pattern (§1.4, R8).
    #[error("a '*' template cannot have children")]
    TemplateWithChildren,
    /// An element of a body list holding both selection keys and a step key (§2.1).
    #[error("an element of a list is either a criterion or one step, not both")]
    MixedElement,
    /// An element of a list holding more than one step key: one element is one step (§2.1).
    #[error("an element of a list is one step: write several steps as several elements")]
    SeveralStepKeys,
    /// A criterion that states nothing (`{}`, `{any_of: []}`, `{without: […]}` alone), which would
    /// select every line (§2.1, R4).
    #[error("this criterion states nothing, so it would select every line of the page")]
    EmptyCriterion,
    /// A criterion that is not a valid line selection.
    #[error("invalid line selection: {reason}")]
    InvalidSelection { reason: String },
}

fn shape(reason: &str) -> RuleError {
    RuleError::Shape {
        reason: reason.to_string(),
    }
}

/// Why the declarative document classification could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocClassifyYamlError {
    /// The file is there and could not be read.
    #[error("{path}: {reason}")]
    Unreadable { path: PathBuf, reason: String },
    /// A defect of the file's content, with the file it is in.
    #[error("{path}: {source}")]
    InFile {
        path: PathBuf,
        #[source]
        source: Box<DocClassifyYamlError>,
    },
    /// `serde_yaml` refused the text before any rule of the grammar could be checked. `hint` is
    /// set when the refusal is one of the traps of §2.3 — an unquoted `*` (an alias: "unknown
    /// anchor") or `@` (a reserved indicator) opening a key — and suggests the quotes.
    #[error("{reason}{}", in_parentheses(.hint))]
    Syntax {
        reason: String,
        hint: Option<String>,
    },
    /// A rule of the grammar broken at `at`.
    #[error("{at}: {kind}{}", both_readings(.kind, .at))]
    Rule { at: YamlLocation, kind: RuleError },
}

fn in_parentheses(hint: &Option<String>) -> String {
    hint.as_ref()
        .map(|hint| format!(" ({hint})"))
        .unwrap_or_default()
}

/// A bare key below `@*.*` is refused because it has two readings (§2.2): the message spells both,
/// so that the fix is a copy away.
fn both_readings(kind: &RuleError, at: &YamlLocation) -> String {
    match (kind, &at.key) {
        (RuleError::SeparatorRequired, Some(key)) => format!(": write '@{key}' or '.{key}'"),
        _ => String::new(),
    }
}

/// `<formats_repo_dir>/metadata/document_classify.yaml`.
pub fn yaml_path(formats_repo_dir: &Path) -> PathBuf {
    formats_repo_dir
        .join(super::metadata::METADATA_DIR)
        .join(DOCUMENT_CLASSIFY_YAML)
}

/// Reads a declarative document classification and brings it to the verbose form (§4): every
/// simplification undone, every child key composed with its parent's pattern, the order of every
/// mapping kept. An empty text, or one of comments only, has no entries.
pub fn normalize(text: &str) -> Result<Vec<VerboseEntry>, DocClassifyYamlError> {
    let document: Node = serde_yaml::from_str(text).map_err(|error| syntax_error(text, &error))?;
    let top = YamlLocation::default();
    match document {
        Node::Null => Ok(vec![]),
        Node::Seq(elements) => elements
            .into_iter()
            .map(|element| match element {
                Node::Map(pairs) => match <[(Node, Node); 1]>::try_from(pairs) {
                    Ok([(page, steps)]) => entry(&page, steps),
                    Err(pairs) if pairs.is_empty() => {
                        Err(top.broken(shape("an entry maps a page to its steps")))
                    }
                    Err(_) => Err(top.broken(RuleError::SeveralPages)),
                },
                _ => Err(top.broken(shape(
                    "an element of the list of entries maps a page to its steps",
                ))),
            })
            .collect(),
        Node::Map(pairs) => {
            let mut entries: Vec<VerboseEntry> = Vec::with_capacity(pairs.len());
            for (page, steps) in pairs {
                let number = page_number(&page)?;
                if entries.iter().any(|entry| entry.page == number) {
                    return Err(top.broken_at(page.written(), RuleError::DuplicateKey));
                }
                entries.push(VerboseEntry {
                    page: number,
                    steps: page_steps(number, steps)?,
                });
            }
            Ok(entries)
        }
        _ => Err(top.broken(shape(
            "the file is a list of entries, or a mapping of pages to their steps",
        ))),
    }
}

/// [`normalize`], then every criterion built into a line selection: the entries the engine runs.
pub fn parse(text: &str) -> Result<Vec<YamlEntry>, DocClassifyYamlError> {
    normalize(text)?
        .into_iter()
        .map(|entry| {
            let on_page = YamlLocation::on_page(entry.page);
            let steps = entry
                .steps
                .into_iter()
                .map(|step| build(&on_page, step))
                .collect::<Result<_, _>>()?;
            Ok(YamlEntry {
                page: entry.page,
                steps,
            })
        })
        .collect()
}

/// Loads `<formats_repo_dir>/metadata/document_classify.yaml`. A missing file — or a missing
/// `metadata/` — is no declarative classification: no entries, not an error.
pub fn load(formats_repo_dir: &Path) -> Result<Vec<YamlEntry>, DocClassifyYamlError> {
    let path = yaml_path(formats_repo_dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => {
            return Err(DocClassifyYamlError::Unreadable {
                path,
                reason: error.to_string(),
            });
        }
    };
    parse(&text).map_err(|source| DocClassifyYamlError::InFile {
        path,
        source: Box::new(source),
    })
}

/// The hint for the two traps of §2.3.1: `*` opens an alias and `@` is reserved, so neither can
/// open an unquoted key, and `serde_yaml`'s own words do not say that quoting is the fix.
fn syntax_error(text: &str, error: &serde_yaml::Error) -> DocClassifyYamlError {
    let reason = error.to_string();
    let at = error
        .location()
        .and_then(|location| text.get(location.index()..))
        .and_then(|rest| rest.chars().next());
    let hint = if reason.contains("unknown anchor") {
        Some("an unquoted '*' opens a YAML alias: quote a template key, as in \"*-EN23\":")
    } else if at == Some('@') {
        Some("'@' cannot open an unquoted YAML value: quote a location key, as in \"@LUX\":")
    } else {
        None
    };
    DocClassifyYamlError::Syntax {
        reason,
        hint: hint.map(str::to_string),
    }
}

fn page_number(key: &Node) -> Result<NonZeroI32, DocClassifyYamlError> {
    let top = YamlLocation::default();
    match key {
        Node::Int(0) => Err(top.broken_at(key.written(), RuleError::PageZero)),
        Node::Int(page) => i32::try_from(*page)
            .ok()
            .and_then(NonZeroI32::new)
            .ok_or_else(|| top.broken_at(key.written(), RuleError::PageNotAnInteger)),
        _ => Err(top.broken_at(key.written(), RuleError::PageNotAnInteger)),
    }
}

/// One element of the list of entries: `{page: steps}`.
fn entry(page: &Node, steps: Node) -> Result<VerboseEntry, DocClassifyYamlError> {
    let number = page_number(page)?;
    Ok(VerboseEntry {
        page: number,
        steps: page_steps(number, steps)?,
    })
}

/// The steps of a page, as a list or as a mapping (§4); never none (§2.4).
fn page_steps(page: NonZeroI32, steps: Node) -> Result<Vec<VerboseStep>, DocClassifyYamlError> {
    let on_page = YamlLocation::on_page(page);
    let steps = match steps {
        Node::Null => vec![],
        Node::Seq(elements) => elements
            .into_iter()
            .map(|element| match element {
                Node::Map(pairs) => match <[(Node, Node); 1]>::try_from(pairs) {
                    Ok([(key, body)]) => step(&on_page, top_key(&on_page, &key)?, body),
                    Err(pairs) if pairs.is_empty() => {
                        Err(on_page.broken(shape("a step maps its key to its body")))
                    }
                    Err(_) => Err(on_page.broken(RuleError::SeveralStepKeys)),
                },
                _ => Err(on_page.broken(shape(
                    "an element of a page's steps maps a step's key to its body",
                ))),
            })
            .collect::<Result<_, _>>()?,
        Node::Map(pairs) => steps_of_mapping(&on_page, pairs, |key| top_key(&on_page, key))?,
        _ => return Err(on_page.broken(shape("the steps of a page are a list or a mapping"))),
    };
    match steps.is_empty() {
        true => Err(on_page.broken(RuleError::PageWithoutSteps)),
        false => Ok(steps),
    }
}

/// The steps of a mapping, in the order written. Two keys naming the same step are refused on the
/// key **read** (§2.3.3): `serde_yaml` compares YAML values, and `21`/`"21"` or `LUX`/`"@LUX"` are
/// two values but one step.
fn steps_of_mapping(
    scope: &YamlLocation,
    pairs: Vec<(Node, Node)>,
    key_of: impl Fn(&Node) -> Result<StepKey, DocClassifyYamlError>,
) -> Result<Vec<VerboseStep>, DocClassifyYamlError> {
    let mut steps: Vec<VerboseStep> = Vec::with_capacity(pairs.len());
    for (key, body) in pairs {
        let read = key_of(&key)?;
        if steps.iter().any(|seen| seen.key == read) {
            return Err(scope.broken_at(key.written(), RuleError::DuplicateKey));
        }
        steps.push(step(scope, read, body)?);
    }
    Ok(steps)
}

/// A top-level key: a pattern, or a `*` template (§1.4).
fn top_key(scope: &YamlLocation, key: &Node) -> Result<StepKey, DocClassifyYamlError> {
    let text = key_text(key).map_err(|kind| scope.broken_at(key.written(), kind))?;
    let read: Result<StepKey, FormatPatternError> = match text.starts_with('*') {
        true => text.parse::<FormatTemplate>().map(StepKey::Template),
        false => text.parse::<FormatPattern>().map(StepKey::Pattern),
    };
    read.map_err(|error| {
        scope.broken_at(
            key.written(),
            RuleError::InvalidKey {
                reason: error.to_string(),
            },
        )
    })
}

/// A step's body in any of its forms (§2.1, §4), into its criteria and its children. `scope` is
/// where the step itself is.
fn step(
    scope: &YamlLocation,
    key: StepKey,
    body: Node,
) -> Result<VerboseStep, DocClassifyYamlError> {
    let inside = scope.within(&key);
    let mut criteria = Vec::new();
    let mut children = Vec::new();
    match body {
        Node::Null => {}
        Node::Seq(elements) => {
            for element in elements {
                let Node::Map(pairs) = element else {
                    return Err(inside.broken(shape(
                        "an element of a body is a criterion or a step, both mappings",
                    )));
                };
                match <[(Node, Node); 1]>::try_from(pairs) {
                    Ok([(child, body)]) if !is_selection_key(&child) => {
                        children.push(step(&inside, child_key(&inside, &key, &child)?, body)?);
                    }
                    Ok(pair) => criteria.push(criterion(&inside, Node::Map(pair.into()))?),
                    Err(pairs) => match pairs
                        .iter()
                        .filter(|(key, _)| !is_selection_key(key))
                        .count()
                    {
                        0 => criteria.push(criterion(&inside, Node::Map(pairs))?),
                        step_keys if step_keys < pairs.len() => {
                            return Err(inside.broken(RuleError::MixedElement));
                        }
                        _ => return Err(inside.broken(RuleError::SeveralStepKeys)),
                    },
                }
            }
        }
        Node::Map(pairs) => {
            let (selection, steps): (Vec<_>, Vec<_>) = pairs
                .into_iter()
                .partition(|(key, _)| is_selection_key(key));
            if !selection.is_empty() {
                criteria.push(criterion(&inside, Node::Map(selection))?);
            }
            children = steps_of_mapping(&inside, steps, |child| child_key(&inside, &key, child))?;
        }
        _ => {
            return Err(inside.broken(shape(
                "a body is a list of criteria and steps, a mapping, or empty",
            )));
        }
    }
    Ok(VerboseStep {
        key,
        criteria,
        children,
    })
}

/// A key of `InputPdfLineSet` — the criteria are exactly its keys, reused (§2.1). Anything else is
/// a step.
fn is_selection_key(key: &Node) -> bool {
    matches!(key, Node::Str(key) if csv_columns::declared::<InputPdfLineSet>().contains(&key.as_str()))
}

fn criterion(scope: &YamlLocation, written: Node) -> Result<InputPdfLineSet, DocClassifyYamlError> {
    let value = written
        .into_value()
        .map_err(|duplicate| scope.broken_at(duplicate, RuleError::DuplicateKey))?;
    let criterion: InputPdfLineSet = serde_yaml::from_value(value).map_err(|error| {
        scope.broken(RuleError::InvalidSelection {
            reason: error.to_string(),
        })
    })?;
    match criterion.states_a_criterion() {
        true => Ok(criterion),
        false => Err(scope.broken(RuleError::EmptyCriterion)),
    }
}

/// A child key, composed with its parent's pattern into the child's full one. `scope` is where the
/// parent is.
fn child_key(
    scope: &YamlLocation,
    parent: &StepKey,
    key: &Node,
) -> Result<StepKey, DocClassifyYamlError> {
    let broken = |kind| scope.broken_at(key.written(), kind);
    let parent = match parent {
        StepKey::Template(_) => return Err(broken(RuleError::TemplateWithChildren)),
        StepKey::Pattern(parent) => parent,
    };
    if matches!(key, Node::Str(text) if text.starts_with("*-")) {
        return Err(broken(RuleError::TemplateNotAtTopLevel));
    }
    compose(parent, key).map(StepKey::Pattern).map_err(broken)
}

/// The child's full pattern: the key continues the parent from its first slot neither `Fixed` nor
/// `Absent` (§2.2), and must strictly refine it (R2).
fn compose(parent: &FormatPattern, key: &Node) -> Result<FormatPattern, RuleError> {
    if parent.is_complete() {
        return Err(RuleError::CompleteParentWithChildren {
            parent: parent.to_string(),
            suggestion: opened_for(parent, &key.written()),
        });
    }
    let does_not_refine = |reason: String| RuleError::KeyDoesNotRefineParent {
        parent: parent.to_string(),
        reason,
    };
    let name = match (parent.language(), parent.year()) {
        (Slot::Unknown, _) => {
            let text = key_text(key)?;
            if let Some(suggestion) = text.strip_prefix('-') {
                return Err(RuleError::HyphenatedChildKey {
                    suggestion: suggestion.to_string(),
                });
            }
            format!("{}-{text}", parent.entity())
        }
        (_, Slot::Unknown) => {
            let year = match key {
                // §2.3.2: YAML reads `05` as a string but `5` as an integer, which in the year slot
                // can only mean `05`.
                Node::Int(year) if (0..=99).contains(year) => format!("{year:02}"),
                Node::Int(year) => {
                    return Err(does_not_refine(format!(
                        "a year is two digits, and {year} is not"
                    )));
                }
                _ => key_text(key)?,
            };
            format!("{parent}{year}")
        }
        _ => after_the_year(parent, &key_text(key)?)?,
    };
    let child: FormatPattern = name
        .parse()
        .map_err(|error: FormatPatternError| does_not_refine(error.to_string()))?;
    match child.strictly_refines(parent) {
        true => Ok(child),
        false if child == *parent => Err(does_not_refine("it names the parent again".to_string())),
        false => Err(does_not_refine(format!(
            "it composes into '{child}', which does not refine the parent"
        ))),
    }
}

/// The name a key below a parent that has its year composes into: the slots the key writes, the
/// parent's for the others. A bare key goes in the one open slot; with two open, it would have two
/// readings, and needs its separator (§2.2, R3b).
fn after_the_year(parent: &FormatPattern, key: &str) -> Result<String, RuleError> {
    let separated = match (
        key.starts_with(['@', '.']),
        parent.location(),
        parent.variant(),
    ) {
        (true, _, _) => key.to_string(),
        (false, Slot::Required, Slot::Required) => return Err(RuleError::SeparatorRequired),
        (false, Slot::Required, _) => format!("@{key}"),
        (false, _, _) => format!(".{key}"),
    };
    let (location, variant) = separated.split_at(separated.find('.').unwrap_or(separated.len()));
    let or_the_parents = |written: &str, prefix: &str, slot: &Slot| match written.is_empty() {
        true => slot_text(prefix, slot),
        false => written.to_string(),
    };
    Ok(format!(
        "{}{}{}",
        up_to_the_year(parent),
        or_the_parents(location, "@", parent.location()),
        or_the_parents(variant, ".", parent.variant())
    ))
}

/// The partial a complete parent should have been written as for `key` to be its child: the
/// `Absent` slots the key writes, opened.
fn opened_for(parent: &FormatPattern, key: &str) -> String {
    let opened = |prefix: &str, slot: &Slot, written: bool| match slot {
        Slot::Absent if written => format!("{prefix}*"),
        slot => slot_text(prefix, slot),
    };
    format!(
        "{}{}{}",
        up_to_the_year(parent),
        opened("@", parent.location(), !key.starts_with('.')),
        opened(".", parent.variant(), key.contains('.'))
    )
}

fn up_to_the_year(pattern: &FormatPattern) -> String {
    format!(
        "{}{}{}",
        pattern.entity(),
        slot_text("-", pattern.language()),
        slot_text("", pattern.year())
    )
}

/// A slot as a name writes it after its `prefix`.
fn slot_text(prefix: &str, slot: &Slot) -> String {
    match slot {
        Slot::Fixed(value) => format!("{prefix}{value}"),
        Slot::Required => format!("{prefix}*"),
        Slot::Absent | Slot::Unknown => String::new(),
    }
}

/// The text of a key. An integer is its decimal text — the year slot pads it before getting here
/// (§2.3.2); a float, a boolean or a null has lost its spelling, and is refused rather than guessed.
fn key_text(key: &Node) -> Result<String, RuleError> {
    match key {
        Node::Str(text) => Ok(text.clone()),
        Node::Int(number) => Ok(number.to_string()),
        Node::Float(_) | Node::Bool(_) | Node::Null => Err(RuleError::NonStringKey),
        Node::Seq(_) | Node::Map(_) => {
            Err(shape("a key is a single word, not a list or a mapping"))
        }
    }
}

/// A verbose step with its criteria built into line selections; `scope` is where the step is.
fn build(scope: &YamlLocation, step: VerboseStep) -> Result<Step, DocClassifyYamlError> {
    let inside = scope.within(&step.key);
    let criteria = step
        .criteria
        .iter()
        .map(|criterion| {
            pdfline_selection_from_dict(criterion).map_err(|error| {
                inside.broken(RuleError::InvalidSelection {
                    reason: error.to_string(),
                })
            })
        })
        .collect::<Result<_, _>>()?;
    let children = step
        .children
        .into_iter()
        .map(|child| build(&inside, child))
        .collect::<Result<_, _>>()?;
    Ok(Step {
        key: step.key,
        criteria,
        children,
    })
}

/// A YAML document as written, every mapping a list of pairs.
///
/// Not `serde_yaml::Value`: its `Mapping` refuses a duplicate key while it is being read, before
/// the loader can say on which page and in which step it is (§2.3.3); and the duplicates the loader
/// has to find anyway — two keys naming the same step — are judged on the keys it reads, not on
/// YAML values. Keeping every pair lets both be one check, made where the location is known.
#[derive(Debug)]
enum Node {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

impl Node {
    /// The node as a key is shown in an error.
    fn written(&self) -> String {
        match self {
            Node::Null => "~".to_string(),
            Node::Bool(value) => value.to_string(),
            Node::Int(value) => value.to_string(),
            Node::Float(value) => value.to_string(),
            Node::Str(value) => value.clone(),
            Node::Seq(_) => "[…]".to_string(),
            Node::Map(_) => "{…}".to_string(),
        }
    }

    /// The node as the `serde_yaml::Value` a criterion deserializes from, or the first key written
    /// twice in one of its mappings.
    fn into_value(self) -> Result<serde_yaml::Value, String> {
        Ok(match self {
            Node::Null => serde_yaml::Value::Null,
            Node::Bool(value) => value.into(),
            Node::Int(value) => value.into(),
            Node::Float(value) => value.into(),
            Node::Str(value) => value.into(),
            Node::Seq(items) => serde_yaml::Value::Sequence(
                items
                    .into_iter()
                    .map(Node::into_value)
                    .collect::<Result<_, _>>()?,
            ),
            Node::Map(pairs) => {
                let mut mapping = serde_yaml::Mapping::with_capacity(pairs.len());
                for (key, value) in pairs {
                    let written = key.written();
                    if mapping
                        .insert(key.into_value()?, value.into_value()?)
                        .is_some()
                    {
                        return Err(written);
                    }
                }
                serde_yaml::Value::Mapping(mapping)
            }
        })
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NodeVisitor)
    }
}

struct NodeVisitor;

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML value")
    }

    fn visit_unit<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }

    /// An empty document.
    fn visit_none<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Node, D::Error> {
        Node::deserialize(deserializer)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Node, E> {
        Ok(Node::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Node, E> {
        Ok(Node::Int(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Node, E> {
        i64::try_from(value)
            .map(Node::Int)
            .map_err(|_| E::custom(format!("integer {value} is too large")))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Node, E> {
        Ok(Node::Float(value))
    }

    fn visit_str<E>(self, value: &str) -> Result<Node, E> {
        Ok(Node::Str(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Node, E> {
        Ok(Node::Str(value))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Node, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = access.next_element()? {
            items.push(item);
        }
        Ok(Node::Seq(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Node, A::Error> {
        let mut pairs = Vec::new();
        while let Some(pair) = access.next_entry()? {
            pairs.push(pair);
        }
        Ok(Node::Map(pairs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    use crate::core::doc_classify::{DocClassification, DocumentClassifier, PageSource, Step};
    use crate::core::page::Page;
    use crate::formats_repo::format_pattern::{FormatPattern, FormatTemplate};
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::pdf_extract::standard_funcs::first_line_if_every_selection_matches;
    use crate::input::document::DocumentError;
    use crate::input::document::lazy::resolve_page_number;

    // ------------------------------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------------------------------

    fn n(number: i32) -> NonZeroI32 {
        NonZeroI32::new(number).expect("test page numbers are never zero")
    }

    fn p(text: &str) -> FormatPattern {
        text.parse().expect("fixture pattern must be valid")
    }

    fn t(text: &str) -> FormatTemplate {
        text.parse().expect("fixture template must be valid")
    }

    /// A criterion in the structured form, written as a YAML flow mapping.
    fn sel(yaml: &str) -> InputPdfLineSet {
        serde_yaml::from_str(yaml)
            .unwrap_or_else(|e| panic!("fixture selection {yaml:?} must read: {e}"))
    }

    /// A verbose step keyed by a full pattern.
    fn step(key: &str, criteria: &[&str], children: Vec<VerboseStep>) -> VerboseStep {
        VerboseStep {
            key: StepKey::Pattern(p(key)),
            criteria: criteria.iter().map(|c| sel(c)).collect(),
            children,
        }
    }

    fn leaf(key: &str, criteria: &[&str]) -> VerboseStep {
        step(key, criteria, vec![])
    }

    /// No criteria, no children: the "otherwise" of a block (§3.2).
    fn unconditional(key: &str) -> VerboseStep {
        step(key, &[], vec![])
    }

    fn template(key: &str, criteria: &[&str]) -> VerboseStep {
        VerboseStep {
            key: StepKey::Template(t(key)),
            criteria: criteria.iter().map(|c| sel(c)).collect(),
            children: vec![],
        }
    }

    fn entry(page: i32, steps: Vec<VerboseStep>) -> VerboseEntry {
        VerboseEntry {
            page: n(page),
            steps,
        }
    }

    fn normalized(text: &str) -> Vec<VerboseEntry> {
        normalize(text)
            .unwrap_or_else(|e| panic!("expected the YAML to normalize, got: {e}\n---\n{text}"))
    }

    /// `Vec<YamlEntry>` is not `Debug` (a `PdfLineSelection` is not), so `unwrap_err` is not
    /// available: both directions go through an explicit match.
    fn parsed(text: &str) -> Vec<YamlEntry> {
        match parse(text) {
            Ok(entries) => entries,
            Err(e) => panic!("expected the YAML to parse, got: {e}\n---\n{text}"),
        }
    }

    fn parse_err(text: &str) -> DocClassifyYamlError {
        match parse(text) {
            Ok(_) => panic!("expected the YAML to be refused\n---\n{text}"),
            Err(e) => e,
        }
    }

    /// The location and the rule of a refusal that must be a broken rule of the grammar.
    fn rule(text: &str) -> (YamlLocation, RuleError) {
        match parse_err(text) {
            DocClassifyYamlError::Rule { at, kind } => (at, kind),
            other => panic!("expected a broken rule, got {other:?}\n---\n{text}"),
        }
    }

    fn at(page: Option<i64>, steps: &[&str], key: Option<&str>) -> YamlLocation {
        YamlLocation {
            page,
            steps: steps.iter().map(|s| s.to_string()).collect(),
            key: key.map(str::to_string),
        }
    }

    /// The only step of the only entry.
    fn only_step(text: &str) -> VerboseStep {
        let entries = normalized(text);
        assert_eq!(entries.len(), 1, "expected one entry");
        assert_eq!(entries[0].steps.len(), 1, "expected one step");
        entries[0].steps[0].clone()
    }

    /// The YAML of a one-step file: `parent` (quoted) on page 1, with one child whose key is
    /// written exactly as `child` (raw YAML: the caller quotes it when it needs quotes).
    fn with_child(parent: &str, child: &str) -> String {
        format!("1:\n  \"{parent}\":\n    {child}: {{text: x}}\n")
    }

    /// The full key the loader composes for `child` written under `parent`.
    fn compose(parent: &str, child: &str) -> StepKey {
        let parent_step = only_step(&with_child(parent, child));
        assert_eq!(
            parent_step.children.len(),
            1,
            "expected one child under {parent}"
        );
        parent_step.children[0].key.clone()
    }

    fn pattern_key(text: &str) -> StepKey {
        StepKey::Pattern(p(text))
    }

    /// A comparable picture of the built entries: page, key, number of criteria, children.
    #[derive(Debug, PartialEq)]
    struct Shape {
        key: String,
        template: bool,
        criteria: usize,
        children: Vec<Shape>,
    }

    fn step_shape(step: &Step) -> Shape {
        Shape {
            key: step.key.to_string(),
            template: matches!(step.key, StepKey::Template(_)),
            criteria: step.criteria.len(),
            children: step.children.iter().map(step_shape).collect(),
        }
    }

    fn verbose_shape(step: &VerboseStep) -> Shape {
        Shape {
            key: step.key.to_string(),
            template: matches!(step.key, StepKey::Template(_)),
            criteria: step.criteria.len(),
            children: step.children.iter().map(verbose_shape).collect(),
        }
    }

    fn shape(entries: &[YamlEntry]) -> Vec<(i32, Vec<Shape>)> {
        entries
            .iter()
            .map(|e| (e.page.get(), e.steps.iter().map(step_shape).collect()))
            .collect()
    }

    fn shape_of_verbose(entries: &[VerboseEntry]) -> Vec<(i32, Vec<Shape>)> {
        entries
            .iter()
            .map(|e| (e.page.get(), e.steps.iter().map(verbose_shape).collect()))
            .collect()
    }

    /// A line of a fake page. Every line has the same box: no criterion run on a fake page here
    /// selects by position.
    fn line(font: &str, size: f32, text: &str) -> PdfLine {
        PdfLine::new(font, size, text, (0.0, 0.0, 200.0, 10.0))
    }

    fn arial(text: &str) -> PdfLine {
        line("Arial", 10.0, text)
    }

    /// A document held in memory, for running the built entries through the engine.
    struct FakeDocument {
        pages: Vec<Vec<PdfLine>>,
    }

    impl FakeDocument {
        fn new(pages: Vec<Vec<PdfLine>>) -> Self {
            FakeDocument { pages }
        }
    }

    impl PageSource for FakeDocument {
        fn page_count(&self) -> usize {
            self.pages.len()
        }

        fn page(&self, number: NonZeroI32) -> Result<Option<Page>, DocumentError> {
            Ok(resolve_page_number(number, self.pages.len()).map(|index| {
                Page::new(
                    index as u32 + 1,
                    (595.0, 842.0),
                    self.pages[index].clone(),
                    vec![],
                )
            }))
        }
    }

    fn classify(
        formats: &[&str],
        entries: Vec<YamlEntry>,
        document: &FakeDocument,
    ) -> DocClassification {
        let formats = formats.iter().map(|f| f.to_string()).collect();
        DocumentClassifier::new(formats, vec![], entries)
            .classify(document)
            .expect("classification must not fail")
    }

    fn complete_format(outcome: &DocClassification) -> String {
        match outcome {
            DocClassification::Complete(complete) => complete.format.to_string(),
            other => panic!("expected a complete outcome, got {other:?}"),
        }
    }

    /// Whether a built criterion finds at least one of `lines`.
    fn finds(
        criterion: &crate::formats_utils::pdf_extract::select::relative::PdfLineSelection,
        lines: &[PdfLine],
    ) -> bool {
        first_line_if_every_selection_matches(std::slice::from_ref(criterion), lines).is_some()
    }

    // ------------------------------------------------------------------------------------------
    // What `serde_yaml` 0.9 does with this file: the facts the grammar is built on (§2.3)
    // ------------------------------------------------------------------------------------------

    mod what_serde_yaml_reads {
        use pretty_assertions::assert_eq;

        fn keys(text: &str) -> Vec<serde_yaml::Value> {
            let value: serde_yaml::Value = serde_yaml::from_str(text).unwrap();
            value
                .as_mapping()
                .expect("a mapping")
                .keys()
                .cloned()
                .collect()
        }

        #[test]
        fn a_mapping_keeps_the_order_it_was_written_in() {
            let read = keys("zeta: 1\nalpha: 2\nmid: 3\n5: 4\n1: 5\nBETA: 6\n");
            let expected: Vec<serde_yaml::Value> = vec![
                "zeta".into(),
                "alpha".into(),
                "mid".into(),
                5.into(),
                1.into(),
                "BETA".into(),
            ];
            assert_eq!(read, expected);
        }

        #[test]
        fn a_leading_zero_makes_a_string_not_an_integer() {
            assert_eq!(keys("08: x"), vec![serde_yaml::Value::from("08")]);
            assert_eq!(keys("05: x"), vec![serde_yaml::Value::from("05")]);
        }

        #[test]
        fn digits_without_a_leading_zero_are_an_integer() {
            assert_eq!(keys("21: x"), vec![serde_yaml::Value::from(21)]);
        }

        #[test]
        fn a_year_and_a_numeric_variant_unquoted_are_a_float() {
            assert!(keys("24.10: x")[0].is_f64());
            assert!(keys(".10: x")[0].is_f64());
        }

        #[test]
        fn no_is_a_string_in_yaml_1_2() {
            assert_eq!(keys("NO: x"), vec![serde_yaml::Value::from("NO")]);
        }

        #[test]
        fn a_duplicate_key_in_a_mapping_is_refused() {
            assert!(serde_yaml::from_str::<serde_yaml::Value>("a: 1\na: 2\n").is_err());
        }

        #[test]
        fn an_integer_and_its_string_are_two_different_keys() {
            assert_eq!(keys("21: a\n\"21\": b").len(), 2);
        }

        #[test]
        fn an_unquoted_star_or_at_cannot_open_a_key() {
            assert!(serde_yaml::from_str::<serde_yaml::Value>("*-EN23: x").is_err());
            assert!(serde_yaml::from_str::<serde_yaml::Value>("@LUX: x").is_err());
        }
    }

    // ------------------------------------------------------------------------------------------
    // No rules: missing, empty, comments only
    // ------------------------------------------------------------------------------------------

    mod no_rules {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_empty_text_has_no_entries() {
            assert_eq!(normalized(""), vec![]);
            assert!(parsed("").is_empty());
        }

        #[test]
        fn a_text_of_comments_only_has_no_entries() {
            let text = "# the declarative document classification\n# nothing yet\n";
            assert_eq!(normalized(text), vec![]);
            assert!(parsed(text).is_empty());
        }

        #[test]
        fn blank_lines_only_have_no_entries() {
            assert_eq!(normalized("\n\n   \n"), vec![]);
        }

        #[test]
        fn an_empty_list_of_entries_has_no_entries() {
            assert_eq!(normalized("[]"), vec![]);
        }

        #[test]
        fn an_empty_mapping_of_pages_has_no_entries() {
            assert_eq!(normalized("{}"), vec![]);
        }
    }

    mod loading_the_file {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::fs;
        use tempfile::TempDir;

        fn repo_with(content: Option<&str>) -> TempDir {
            let dir = TempDir::new().unwrap();
            fs::create_dir_all(dir.path().join("metadata")).unwrap();
            if let Some(content) = content {
                fs::write(
                    dir.path().join("metadata").join("document_classify.yaml"),
                    content,
                )
                .unwrap();
            }
            dir
        }

        fn loaded(dir: &Path) -> Vec<YamlEntry> {
            match load(dir) {
                Ok(entries) => entries,
                Err(e) => panic!("expected the file to load, got: {e}"),
            }
        }

        fn load_err(dir: &Path) -> DocClassifyYamlError {
            match load(dir) {
                Ok(_) => panic!("expected the load to fail"),
                Err(e) => e,
            }
        }

        #[test]
        fn the_file_is_in_metadata() {
            let repo = Path::new("/some/repo");
            assert_eq!(
                yaml_path(repo),
                PathBuf::from("/some/repo/metadata/document_classify.yaml")
            );
        }

        #[test]
        fn the_file_name_is_document_classify_yaml() {
            assert_eq!(DOCUMENT_CLASSIFY_YAML, "document_classify.yaml");
        }

        #[test]
        fn a_missing_file_is_no_declarative_classification_not_an_error() {
            let repo = repo_with(None);
            assert!(loaded(repo.path()).is_empty());
        }

        #[test]
        fn a_missing_metadata_directory_is_no_declarative_classification_either() {
            let repo = TempDir::new().unwrap();
            assert!(loaded(repo.path()).is_empty());
        }

        #[test]
        fn an_empty_file_has_no_rules() {
            let repo = repo_with(Some(""));
            assert!(loaded(repo.path()).is_empty());
        }

        #[test]
        fn a_file_of_comments_only_has_no_rules() {
            let repo = repo_with(Some("# written by freeports-dev\n# add the rules here\n"));
            assert!(loaded(repo.path()).is_empty());
        }

        #[test]
        fn a_file_with_rules_loads_them() {
            let repo = repo_with(Some("1:\n  AMUNDI-EN24: {text: '^AMUNDI FUNDS$'}\n"));
            let entries = loaded(repo.path());
            assert_eq!(
                shape(&entries),
                vec![(
                    1,
                    vec![Shape {
                        key: "AMUNDI-EN24".into(),
                        template: false,
                        criteria: 1,
                        children: vec![]
                    }]
                )]
            );
        }

        #[test]
        fn a_defect_in_the_file_names_the_file_and_the_defect() {
            let repo = repo_with(Some("0:\n  AMUNDI-EN24: {text: x}\n"));
            let error = load_err(repo.path());
            let path = yaml_path(repo.path());
            match &error {
                DocClassifyYamlError::InFile {
                    path: in_file,
                    source,
                } => {
                    assert_eq!(in_file, &path);
                    assert!(
                        matches!(
                            **source,
                            DocClassifyYamlError::Rule {
                                kind: RuleError::PageZero,
                                ..
                            }
                        ),
                        "{source:?}"
                    );
                }
                other => panic!("expected InFile, got {other:?}"),
            }
            let message = error.to_string();
            assert!(message.contains(&path.display().to_string()), "{message}");
            assert!(
                message.contains("page 0"),
                "the inner defect must show: {message}"
            );
        }

        #[test]
        fn a_file_that_cannot_be_read_is_unreadable() {
            use std::os::unix::fs::PermissionsExt;
            let repo = repo_with(Some("1:\n  AMUNDI-EN24: {text: x}\n"));
            let path = yaml_path(repo.path());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            if fs::read_to_string(&path).is_ok() {
                // Running with privileges that ignore file modes: nothing to observe.
                return;
            }
            match load_err(repo.path()) {
                DocClassifyYamlError::Unreadable { path, .. } => {
                    assert_eq!(path, yaml_path(repo.path()))
                }
                other => panic!("expected Unreadable, got {other:?}"),
            }
        }
    }

    // ------------------------------------------------------------------------------------------
    // The verbose form (§2.1)
    // ------------------------------------------------------------------------------------------

    mod the_verbose_form {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The example of §2.1, with its `…` filled in.
        const SECTION_2_1: &str = r#"
- 1:
    - EURIZON-EN23:
        - {font: arialmt, text: '^ciao$'}
        - {text: PROVA, font_size: 8.9}
    - MEDIOLANUM:
        - {text: '^Mediolanum'}
        - EN: [{text: 'Annual Report'}]
        - IT23:
            - {text: 'Relazione'}
- 3:
    - AMUNDI-EN24:
        - {text: '^AMUNDI FUNDS$'}
"#;

        #[test]
        fn the_example_of_the_specification_reads_as_written() {
            assert_eq!(
                normalized(SECTION_2_1),
                vec![
                    entry(
                        1,
                        vec![
                            leaf(
                                "EURIZON-EN23",
                                &[
                                    "{font: arialmt, text: '^ciao$'}",
                                    "{text: PROVA, font_size: 8.9}"
                                ]
                            ),
                            step(
                                "MEDIOLANUM",
                                &["{text: '^Mediolanum'}"],
                                vec![
                                    leaf("MEDIOLANUM-EN", &["{text: 'Annual Report'}"]),
                                    leaf("MEDIOLANUM-IT23", &["{text: 'Relazione'}"])
                                ],
                            ),
                        ]
                    ),
                    entry(3, vec![leaf("AMUNDI-EN24", &["{text: '^AMUNDI FUNDS$'}"])]),
                ]
            );
        }

        #[test]
        fn the_same_page_may_appear_more_than_once() {
            let text = "- 1:\n    - AMUNDI-EN24: [{text: a}]\n- 2:\n    - KAIROS-EN23: [{text: b}]\n- 1:\n    - UBS-EN23: [{text: c}]\n";
            let pages: Vec<i32> = normalized(text).iter().map(|e| e.page.get()).collect();
            assert_eq!(pages, vec![1, 2, 1]);
        }

        #[test]
        fn negative_pages_count_from_the_end() {
            let text =
                "- -1:\n    - AMUNDI-EN24: [{text: a}]\n- -2:\n    - KAIROS-EN23: [{text: b}]\n";
            let pages: Vec<i32> = normalized(text).iter().map(|e| e.page.get()).collect();
            assert_eq!(pages, vec![-1, -2]);
        }

        #[test]
        fn a_repeated_step_in_a_list_is_an_ordered_or() {
            let text = "- 1:\n    - EURIZON-EN23: [{text: a}]\n    - FIDEURAM-IT21: [{text: b}]\n    - EURIZON-EN23: [{text: c}]\n";
            assert_eq!(
                normalized(text),
                vec![entry(
                    1,
                    vec![
                        leaf("EURIZON-EN23", &["{text: a}"]),
                        leaf("FIDEURAM-IT21", &["{text: b}"]),
                        leaf("EURIZON-EN23", &["{text: c}"])
                    ]
                )]
            );
        }

        #[test]
        fn a_repeated_child_in_a_list_is_an_ordered_or_too() {
            let text = "1:\n  MEDIOLANUM-ES:\n    - 21: [{text: a}]\n    - 23: [{text: b}]\n    - 21: [{text: c}]\n";
            let children: Vec<StepKey> = only_step(text)
                .children
                .into_iter()
                .map(|c| c.key)
                .collect();
            assert_eq!(
                children,
                vec![
                    pattern_key("MEDIOLANUM-ES21"),
                    pattern_key("MEDIOLANUM-ES23"),
                    pattern_key("MEDIOLANUM-ES21")
                ]
            );
        }

        #[test]
        fn criteria_written_after_the_children_are_still_the_steps_criteria() {
            let after = "1:\n  MEDIOLANUM:\n    - EN23: [{text: e}]\n    - {text: m}\n";
            let before = "1:\n  MEDIOLANUM:\n    - {text: m}\n    - EN23: [{text: e}]\n";
            let expected = step(
                "MEDIOLANUM",
                &["{text: m}"],
                vec![leaf("MEDIOLANUM-EN23", &["{text: e}"])],
            );
            assert_eq!(only_step(after), expected);
            assert_eq!(only_step(before), expected);
        }

        #[test]
        fn criteria_interleaved_with_children_are_gathered_in_order_and_children_keep_theirs() {
            let text = "1:\n  MEDIOLANUM:\n    - {text: a}\n    - EN23: [{text: e}]\n    - {text: b}\n    - IT23: [{text: i}]\n";
            assert_eq!(
                only_step(text),
                step(
                    "MEDIOLANUM",
                    &["{text: a}", "{text: b}"],
                    vec![
                        leaf("MEDIOLANUM-EN23", &["{text: e}"]),
                        leaf("MEDIOLANUM-IT23", &["{text: i}"])
                    ]
                )
            );
        }
    }

    // ------------------------------------------------------------------------------------------
    // The simplifications of §4, one by one
    // ------------------------------------------------------------------------------------------

    mod each_simplification {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_top_level_as_a_mapping_is_the_list_of_entries_in_the_same_order() {
            let simplified = "5:\n  - EURIZON-IT24: [{text: a}]\n1:\n  - AMUNDI-EN24: [{text: b}]\n-1:\n  - UBS-EN23: [{text: c}]\n";
            let verbose = "- 5:\n    - EURIZON-IT24: [{text: a}]\n- 1:\n    - AMUNDI-EN24: [{text: b}]\n- -1:\n    - UBS-EN23: [{text: c}]\n";
            assert_eq!(normalized(simplified), normalized(verbose));
            let pages: Vec<i32> = normalized(simplified)
                .iter()
                .map(|e| e.page.get())
                .collect();
            assert_eq!(pages, vec![5, 1, -1]);
        }

        #[test]
        fn the_steps_as_a_mapping_are_the_list_of_steps_in_the_same_order() {
            let simplified = "- 1:\n    UBS-EN23: [{text: a}]\n    AMUNDI-EN24: [{text: b}]\n    KAIROS-EN23: [{text: c}]\n";
            let verbose = "- 1:\n    - UBS-EN23: [{text: a}]\n    - AMUNDI-EN24: [{text: b}]\n    - KAIROS-EN23: [{text: c}]\n";
            assert_eq!(normalized(simplified), normalized(verbose));
        }

        #[test]
        fn a_body_mapping_mixing_selection_keys_and_children_is_one_criterion_then_the_children() {
            let simplified = "1:\n  MEDIOLANUM:\n    text: '^Mediolanum'\n    font: arialmt\n    EN23: [{text: e}]\n    IT23: [{text: i}]\n";
            let verbose = "1:\n  - MEDIOLANUM:\n      - {text: '^Mediolanum', font: arialmt}\n      - EN23: [{text: e}]\n      - IT23: [{text: i}]\n";
            assert_eq!(normalized(simplified), normalized(verbose));
            assert_eq!(
                only_step(simplified),
                step(
                    "MEDIOLANUM",
                    &["{text: '^Mediolanum', font: arialmt}"],
                    vec![
                        leaf("MEDIOLANUM-EN23", &["{text: e}"]),
                        leaf("MEDIOLANUM-IT23", &["{text: i}"])
                    ]
                )
            );
        }

        #[test]
        fn in_a_mixed_body_mapping_the_selection_keys_may_come_anywhere() {
            let scattered = "1:\n  MEDIOLANUM:\n    EN23: [{text: e}]\n    text: '^Mediolanum'\n    IT23: [{text: i}]\n    font: arialmt\n";
            let gathered = "1:\n  MEDIOLANUM:\n    text: '^Mediolanum'\n    font: arialmt\n    EN23: [{text: e}]\n    IT23: [{text: i}]\n";
            assert_eq!(normalized(scattered), normalized(gathered));
        }

        #[test]
        fn a_body_of_selection_keys_only_is_a_single_criterion_and_no_children() {
            let simplified =
                "1:\n  EURIZON-IT24: {text: '^Relazione di gestione$', font_size: 26}\n";
            let verbose =
                "1:\n  - EURIZON-IT24:\n      - {text: '^Relazione di gestione$', font_size: 26}\n";
            assert_eq!(normalized(simplified), normalized(verbose));
            assert_eq!(
                only_step(simplified),
                leaf(
                    "EURIZON-IT24",
                    &["{text: '^Relazione di gestione$', font_size: 26}"]
                )
            );
        }

        #[test]
        fn a_null_body_is_an_unconditional_step() {
            assert_eq!(
                only_step("1:\n  ASTERIA-EN23: ~\n"),
                unconditional("ASTERIA-EN23")
            );
        }

        #[test]
        fn an_empty_body_written_null_is_an_unconditional_step() {
            assert_eq!(
                only_step("1:\n  ASTERIA-EN23: null\n"),
                unconditional("ASTERIA-EN23")
            );
        }

        #[test]
        fn an_empty_body_written_as_nothing_is_an_unconditional_step() {
            assert_eq!(
                only_step("1:\n  ASTERIA-EN23:\n"),
                unconditional("ASTERIA-EN23")
            );
        }

        #[test]
        fn an_empty_list_body_is_an_unconditional_step() {
            assert_eq!(
                only_step("1:\n  ASTERIA-EN23: []\n"),
                unconditional("ASTERIA-EN23")
            );
        }

        #[test]
        fn an_empty_mapping_body_is_an_unconditional_step_not_an_empty_criterion() {
            assert_eq!(
                only_step("1:\n  ASTERIA-EN23: {}\n"),
                unconditional("ASTERIA-EN23")
            );
        }

        #[test]
        fn the_three_unconditional_bodies_are_the_same_thing() {
            let null = normalized("1:\n  ASTERIA:\n    text: a\n    EN23: ~\n");
            let list = normalized("1:\n  ASTERIA:\n    text: a\n    EN23: []\n");
            let mapping = normalized("1:\n  ASTERIA:\n    text: a\n    EN23: {}\n");
            assert_eq!(null, list);
            assert_eq!(null, mapping);
        }

        #[test]
        fn two_criteria_of_one_step_in_mapping_form_go_through_all_of() {
            let step = only_step("1:\n  ARCA-IT24:\n    all_of: [{text: a}, {text: b}]\n");
            assert_eq!(
                step,
                leaf("ARCA-IT24", &["{all_of: [{text: a}, {text: b}]}"])
            );
        }

        #[test]
        fn each_level_simplifies_on_its_own_a_mapping_of_pages_with_a_list_of_steps() {
            let text = "1:\n  - AMUNDI-EN24: {text: a}\n  - AMUNDI-EN24: {text: b}\n";
            assert_eq!(
                normalized(text),
                vec![entry(
                    1,
                    vec![
                        leaf("AMUNDI-EN24", &["{text: a}"]),
                        leaf("AMUNDI-EN24", &["{text: b}"])
                    ]
                )]
            );
        }

        #[test]
        fn each_level_simplifies_on_its_own_a_list_of_pages_with_a_mapping_of_steps() {
            let text = "- 1:\n    AMUNDI-EN24: {text: a}\n    KAIROS-EN23: [{text: b}]\n";
            assert_eq!(
                normalized(text),
                vec![entry(
                    1,
                    vec![
                        leaf("AMUNDI-EN24", &["{text: a}"]),
                        leaf("KAIROS-EN23", &["{text: b}"])
                    ]
                )]
            );
        }

        #[test]
        fn a_body_mapping_with_children_only_is_a_prefix() {
            let text = "1:\n  MEDIOLANUM-IT24.*:\n    A: {text: a}\n    B: {text: b}\n";
            assert_eq!(
                only_step(text),
                step(
                    "MEDIOLANUM-IT24.*",
                    &[],
                    vec![
                        leaf("MEDIOLANUM-IT24.A", &["{text: a}"]),
                        leaf("MEDIOLANUM-IT24.B", &["{text: b}"])
                    ]
                )
            );
        }

        #[test]
        fn every_selection_key_is_read_as_a_criterion_never_as_a_child() {
            let bodies = [
                ("text", "{text: a}"),
                ("font", "{font: arialmt}"),
                ("font_size", "{font_size: 9}"),
                ("area", "{area: {x_min: 10}}"),
                (
                    "window",
                    "{window: {anchor: {text: a}, dx: 0, dy: 1, width: 1, height: 1}}",
                ),
                ("font_of", "{font_of: {text: a}}"),
                ("any_of", "{any_of: [{text: a}, {text: b}]}"),
                ("all_of", "{all_of: [{text: a}, {text: b}]}"),
                ("without", "{text: a, without: [{text: b}]}"),
            ];
            for (key, body) in bodies {
                let text = format!("1:\n  MEDIOLANUM-EN23: {body}\n");
                assert_eq!(
                    only_step(&text),
                    leaf("MEDIOLANUM-EN23", &[body]),
                    "`{key}` must be a selection key"
                );
            }
        }
    }

    // ------------------------------------------------------------------------------------------
    // The test that matters most: the verbose and the simplified spelling are the same thing
    // ------------------------------------------------------------------------------------------

    mod the_verbose_and_the_simplified_example_are_the_same {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The example of the source (`classify_formats.txt`), as §3.5 and the critique corrected it:
        /// `MEDIOALNUM` is `MEDIOLANUM`, the root's criteria and its children are one step (the form
        /// §3.5 recommends, which the mapping form needs anyway), `*-EN23` is quoted, the `…` are
        /// filled in. The repeated `EURIZON-EN23` and `ES: 21` are left to the list-form tests, as
        /// the source's own simplified example left them out.
        const SOURCE_VERBOSE: &str = r#"
- 1:
    - EURIZON-EN23:
        - {font: arial, text: '^ciao$'}
        - {font: arial, font_size: 8.9, text: PROVA}
    - FIDEURAM-IT21:
        - {area: {x_min: 10, x_max: 300}, text: '^Fideuram'}
    - MEDIOLANUM:
        - {font: frigigtur, text: sabbia}
        - EN:
            - {text: '^Annual Report'}
        - IT23:
            - {text: '^Relazione'}
        - ES:
            - 21:
                - {text: '^Informe anual'}
            - 23:
                - {text: '^Informe 2023'}
    - AMUNDI:
        - {text: 'amundi$'}
    - "*-EN23":
        - {font: arial, text: 'Annual Report 2023'}
    - FIDEURAM-IT23@*:
        - LUX:
            - {text: Luxembourg}
        - IR:
            - {text: Ireland}
- 3:
    - EURIZON-IT24:
        - {text: '^Relazione di gestione$'}
"#;

        const SOURCE_SIMPLIFIED: &str = r#"
1:
  EURIZON-EN23:
    - {font: arial, text: '^ciao$'}
    - {font: arial, font_size: 8.9, text: PROVA}
  FIDEURAM-IT21:
    area:
      x_min: 10
      x_max: 300
    text: '^Fideuram'
  MEDIOLANUM:
    font: frigigtur
    text: sabbia
    EN:
      text: '^Annual Report'
    IT23:
      text: '^Relazione'
    ES:
      21:
        text: '^Informe anual'
      23:
        text: '^Informe 2023'
  AMUNDI:
    text: 'amundi$'
  "*-EN23":
    font: arial
    text: 'Annual Report 2023'
  FIDEURAM-IT23@*:
    LUX:
      text: Luxembourg
    IR:
      text: Ireland
3:
  EURIZON-IT24:
    text: '^Relazione di gestione$'
"#;

        const SOURCE_FORMATS: &[&str] = &[
            "AMUNDI-EN24",
            "EURIZON-EN23",
            "EURIZON-IT24",
            "FIDEURAM-IT21",
            "FIDEURAM-IT23@IR",
            "FIDEURAM-IT23@LUX",
            "MEDIOLANUM-EN23",
            "MEDIOLANUM-ES21",
            "MEDIOLANUM-ES23",
            "MEDIOLANUM-IT23",
        ];

        fn source_expected() -> Vec<VerboseEntry> {
            vec![
                entry(
                    1,
                    vec![
                        leaf(
                            "EURIZON-EN23",
                            &[
                                "{font: arial, text: '^ciao$'}",
                                "{font: arial, font_size: 8.9, text: PROVA}",
                            ],
                        ),
                        leaf(
                            "FIDEURAM-IT21",
                            &["{area: {x_min: 10, x_max: 300}, text: '^Fideuram'}"],
                        ),
                        step(
                            "MEDIOLANUM",
                            &["{font: frigigtur, text: sabbia}"],
                            vec![
                                leaf("MEDIOLANUM-EN", &["{text: '^Annual Report'}"]),
                                leaf("MEDIOLANUM-IT23", &["{text: '^Relazione'}"]),
                                step(
                                    "MEDIOLANUM-ES",
                                    &[],
                                    vec![
                                        leaf("MEDIOLANUM-ES21", &["{text: '^Informe anual'}"]),
                                        leaf("MEDIOLANUM-ES23", &["{text: '^Informe 2023'}"]),
                                    ],
                                ),
                            ],
                        ),
                        leaf("AMUNDI", &["{text: 'amundi$'}"]),
                        template("*-EN23", &["{font: arial, text: 'Annual Report 2023'}"]),
                        step(
                            "FIDEURAM-IT23@*",
                            &[],
                            vec![
                                leaf("FIDEURAM-IT23@LUX", &["{text: Luxembourg}"]),
                                leaf("FIDEURAM-IT23@IR", &["{text: Ireland}"]),
                            ],
                        ),
                    ],
                ),
                entry(
                    3,
                    vec![leaf("EURIZON-IT24", &["{text: '^Relazione di gestione$'}"])],
                ),
            ]
        }

        #[test]
        fn the_verbose_source_example_normalizes_to_the_expected_tree() {
            assert_eq!(normalized(SOURCE_VERBOSE), source_expected());
        }

        #[test]
        fn the_simplified_source_example_normalizes_to_the_same_tree() {
            assert_eq!(normalized(SOURCE_SIMPLIFIED), source_expected());
        }

        #[test]
        fn the_two_source_examples_build_the_same_steps() {
            assert_eq!(
                shape(&parsed(SOURCE_SIMPLIFIED)),
                shape(&parsed(SOURCE_VERBOSE))
            );
            assert_eq!(
                shape(&parsed(SOURCE_VERBOSE)),
                shape_of_verbose(&source_expected())
            );
        }

        /// The two spellings, run by the engine over the same documents, decide the same.
        #[test]
        fn the_two_source_examples_classify_every_document_the_same() {
            let documents = [
                FakeDocument::new(vec![vec![arial("ciao"), line("Arial", 8.9, "PROVA")]]),
                FakeDocument::new(vec![vec![
                    line("Frigigtur", 10.0, "sabbia"),
                    arial("Annual Report 2023"),
                ]]),
                FakeDocument::new(vec![vec![
                    line("Frigigtur", 10.0, "sabbia"),
                    arial("Informe anual"),
                ]]),
                FakeDocument::new(vec![vec![], vec![], vec![arial("Relazione di gestione")]]),
                FakeDocument::new(vec![vec![arial("fondi amundi")]]),
                FakeDocument::new(vec![vec![arial("nothing to see")]]),
            ];
            let expected = [
                Some("EURIZON-EN23"),
                Some("MEDIOLANUM-EN23"),
                Some("MEDIOLANUM-ES21"),
                Some("EURIZON-IT24"),
                None, // the partial AMUNDI
                None, // unclassified
            ];
            for (index, (document, expected)) in documents.iter().zip(expected).enumerate() {
                let verbose = classify(SOURCE_FORMATS, parsed(SOURCE_VERBOSE), document);
                let simplified = classify(SOURCE_FORMATS, parsed(SOURCE_SIMPLIFIED), document);
                assert_eq!(verbose, simplified, "document {index}");
                if let Some(format) = expected {
                    assert_eq!(complete_format(&verbose), format, "document {index}");
                }
            }
            assert!(matches!(
                classify(SOURCE_FORMATS, parsed(SOURCE_SIMPLIFIED), &documents[4]),
                DocClassification::Partial { ref pattern, .. } if *pattern == p("AMUNDI")
            ));
            assert_eq!(
                classify(SOURCE_FORMATS, parsed(SOURCE_SIMPLIFIED), &documents[5]),
                DocClassification::Unclassified
            );
        }

        /// The draft of §8 written out in the verbose form.
        const DRAFT_VERBOSE: &str = r#"
- 1:
    - AMUNDI-EN24:       [{text: '^AMUNDI FUNDS$'}]
    - AMUNDI-IT24:       [{text: '^FONDI AMUNDI '}]
    - ANIMA-EN23:        [{text: '^ANIMA Funds Plc$'}]
    - ANIMA_SICAV-EN24:  [{text: '^ANIMA INVESTMENT SICAV$'}]
    - ARCA-IT24:         [{text: '^RELAZIONE '}, {text: '^ARCA '}]
    - ASTERIA:
        - {text: '^Asteria Funds$'}
        - EN24: [{text: '^Investment Managers$'}]
        - EN23: []
    - CARNE-EN23:        [{text: '^SIDERA FUNDS$'}]
    - DANSKEINVEST-EN24: [{text: '^DANSKE INVEST SICAV$'}]
    - EURIZON-EN21:      [{text: '^Eurizon AM Sicav$'}]
    - EURIZON-EN23:      [{text: '^Eurizon Fund$'}]
    - FIDEURAM-EN23:     [{text: '^WILLERFUNDS$'}]
    - FIDEURAM-IT24:     [{text: '^Fideuram ISPB Asset Management SGR'}]
    - FINECO-EN23@IR:    [{text: '^FAM Series UCITS ICAV$'}]
    - FINECO-EN23@LUX:   [{text: '^CoRe Series$'}]
    - KAIROS-EN23:       [{text: '^KAIROS INTERNATIONAL SICAV$'}]
    - UBS-EN23:          [{text: '^Annual Report', font: ubsheadline}]
    - MEDIOLANUM-EN24:   [{text: '^CHALLENGE FUNDS$'}]
    - MEDIOLANUM-ES24.A: [{text: '^GAMAX FUNDS FCP$'}]
    - MEDIOLANUM:
        - {any_of: [{text: '^Mediolanum'}, {text: '^MEDIOLANUM'}]}
        - EN23:   [{text: '^Mediolanum Specialities SICAV-SIF$'}]
        - ES24.B: [{text: 'Nº Registro CNMV'}]
        - IT24.*:
            - A: [{text: '^Mediolanum Best Brands$'}]
            - B: [{text: '^Relazione di gestione annuale'}]
- 2:
    - ANIMA_SGR-IT24:    [{text: '^ANIMA SGR S.p.A.$'}]
- 5:
    - EURIZON-IT24:      [{text: '^Relazione di gestione$', font_size: 26}]
"#;

        #[test]
        fn the_draft_of_the_specification_is_the_same_in_both_spellings() {
            assert_eq!(
                normalized(super::the_draft_of_the_specification::DRAFT),
                normalized(DRAFT_VERBOSE)
            );
        }

        #[test]
        fn the_draft_of_the_specification_builds_the_same_steps_in_both_spellings() {
            assert_eq!(
                shape(&parsed(super::the_draft_of_the_specification::DRAFT)),
                shape(&parsed(DRAFT_VERBOSE))
            );
        }
    }

    // ------------------------------------------------------------------------------------------
    // The draft of §8: the 25 real formats
    // ------------------------------------------------------------------------------------------

    mod the_draft_of_the_specification {
        use super::*;
        use pretty_assertions::assert_eq;

        /// §8 of the specification, verbatim.
        pub(super) const DRAFT: &str = r#"
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

        /// The 25 formats of `analysis_finance_reports_formats/metadata/formats.csv` (2026-10-11).
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

        fn draft() -> Vec<VerboseEntry> {
            normalized(DRAFT)
        }

        fn top(key: &str) -> VerboseStep {
            draft()[0]
                .steps
                .iter()
                .find(|s| s.key == pattern_key(key))
                .unwrap_or_else(|| panic!("no step {key} on page 1"))
                .clone()
        }

        fn complete_leaves(steps: &[VerboseStep], into: &mut Vec<String>) {
            for step in steps {
                if let StepKey::Pattern(pattern) = &step.key
                    && pattern.is_complete()
                {
                    into.push(pattern.to_string());
                }
                complete_leaves(&step.children, into);
            }
        }

        #[test]
        fn it_loads_without_a_defect_of_form() {
            assert_eq!(parsed(DRAFT).len(), 3);
        }

        #[test]
        fn its_pages_are_1_2_and_5_in_that_order() {
            let pages: Vec<i32> = draft().iter().map(|e| e.page.get()).collect();
            assert_eq!(pages, vec![1, 2, 5]);
        }

        #[test]
        fn page_1_has_its_19_steps_in_the_order_written() {
            let keys: Vec<String> = draft()[0].steps.iter().map(|s| s.key.to_string()).collect();
            assert_eq!(
                keys,
                vec![
                    "AMUNDI-EN24",
                    "AMUNDI-IT24",
                    "ANIMA-EN23",
                    "ANIMA_SICAV-EN24",
                    "ARCA-IT24",
                    "ASTERIA",
                    "CARNE-EN23",
                    "DANSKEINVEST-EN24",
                    "EURIZON-EN21",
                    "EURIZON-EN23",
                    "FIDEURAM-EN23",
                    "FIDEURAM-IT24",
                    "FINECO-EN23@IR",
                    "FINECO-EN23@LUX",
                    "KAIROS-EN23",
                    "UBS-EN23",
                    "MEDIOLANUM-EN24",
                    "MEDIOLANUM-ES24.A",
                    "MEDIOLANUM",
                ]
            );
        }

        #[test]
        fn its_complete_steps_are_exactly_the_25_formats() {
            let mut leaves = Vec::new();
            for entry in draft() {
                complete_leaves(&entry.steps, &mut leaves);
            }
            leaves.sort();
            let mut expected: Vec<String> = REAL_FORMATS.iter().map(|f| f.to_string()).collect();
            expected.sort();
            assert_eq!(leaves, expected);
        }

        #[test]
        fn mediolanum_has_its_criterion_and_three_children_with_the_prefix_last() {
            assert_eq!(
                top("MEDIOLANUM"),
                step(
                    "MEDIOLANUM",
                    &["{any_of: [{text: '^Mediolanum'}, {text: '^MEDIOLANUM'}]}"],
                    vec![
                        leaf(
                            "MEDIOLANUM-EN23",
                            &["{text: '^Mediolanum Specialities SICAV-SIF$'}"]
                        ),
                        leaf("MEDIOLANUM-ES24.B", &["{text: 'Nº Registro CNMV'}"]),
                        step(
                            "MEDIOLANUM-IT24.*",
                            &[],
                            vec![
                                leaf("MEDIOLANUM-IT24.A", &["{text: '^Mediolanum Best Brands$'}"]),
                                leaf(
                                    "MEDIOLANUM-IT24.B",
                                    &["{text: '^Relazione di gestione annuale'}"]
                                ),
                            ],
                        ),
                    ],
                )
            );
        }

        #[test]
        fn asteria_ends_with_the_otherwise_of_its_block() {
            assert_eq!(
                top("ASTERIA"),
                step(
                    "ASTERIA",
                    &["{text: '^Asteria Funds$'}"],
                    vec![
                        leaf("ASTERIA-EN24", &["{text: '^Investment Managers$'}"]),
                        unconditional("ASTERIA-EN23")
                    ],
                )
            );
        }

        #[test]
        fn arca_has_two_criteria() {
            assert_eq!(top("ARCA-IT24").criteria.len(), 2);
        }

        #[test]
        fn ubs_has_one_criterion_of_text_and_font() {
            assert_eq!(
                top("UBS-EN23"),
                leaf("UBS-EN23", &["{text: '^Annual Report', font: ubsheadline}"])
            );
        }

        #[test]
        fn fineco_is_two_leaves_with_their_location() {
            assert_eq!(
                top("FINECO-EN23@IR"),
                leaf("FINECO-EN23@IR", &["{text: '^FAM Series UCITS ICAV$'}"])
            );
            assert_eq!(
                top("FINECO-EN23@LUX"),
                leaf("FINECO-EN23@LUX", &["{text: '^CoRe Series$'}"])
            );
        }

        #[test]
        fn pages_2_and_5_hold_anima_sgr_and_eurizon_it24() {
            let draft = draft();
            assert_eq!(
                draft[1],
                entry(
                    2,
                    vec![leaf("ANIMA_SGR-IT24", &["{text: '^ANIMA SGR S.p.A.$'}"])]
                )
            );
            assert_eq!(
                draft[2],
                entry(
                    5,
                    vec![leaf(
                        "EURIZON-IT24",
                        &["{text: '^Relazione di gestione$', font_size: 26}"]
                    )]
                )
            );
        }

        fn real_formats() -> Vec<&'static str> {
            REAL_FORMATS.to_vec()
        }

        #[test]
        fn run_by_the_engine_it_finds_mediolanum_best_brands_through_the_prefix() {
            let document = FakeDocument::new(vec![vec![
                arial("MEDIOLANUM"),
                arial("Mediolanum Best Brands"),
            ]]);
            assert_eq!(
                complete_format(&classify(&real_formats(), parsed(DRAFT), &document)),
                "MEDIOLANUM-IT24.A"
            );
        }

        #[test]
        fn run_by_the_engine_it_decides_asteria_en23_by_exclusion() {
            let document = FakeDocument::new(vec![vec![arial("Asteria Funds")]]);
            match classify(&real_formats(), parsed(DRAFT), &document) {
                DocClassification::Complete(complete) => {
                    assert_eq!(complete.format, p("ASTERIA-EN23"));
                    assert!(complete.by_exclusion);
                }
                other => panic!("expected ASTERIA-EN23, got {other:?}"),
            }
        }

        #[test]
        fn run_by_the_engine_it_finds_eurizon_it24_on_page_5_by_its_font_size() {
            let mut pages = vec![vec![]; 4];
            pages.push(vec![line("TrebuchetMS", 26.0, "Relazione di gestione")]);
            let document = FakeDocument::new(pages);
            assert_eq!(
                complete_format(&classify(&real_formats(), parsed(DRAFT), &document)),
                "EURIZON-IT24"
            );

            let mut pages = vec![vec![]; 4];
            pages.push(vec![line("TrebuchetMS", 10.0, "Relazione di gestione")]);
            let document = FakeDocument::new(pages);
            assert_eq!(
                classify(&real_formats(), parsed(DRAFT), &document),
                DocClassification::Unclassified
            );
        }
    }

    // ------------------------------------------------------------------------------------------
    // The steps built for the engine
    // ------------------------------------------------------------------------------------------

    mod the_built_steps {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_child_is_keyed_by_its_full_pattern() {
            let entries = parsed("1:\n  MEDIOLANUM:\n    text: m\n    ES:\n      21: {text: e}\n");
            let mediolanum = &entries[0].steps[0];
            assert_eq!(mediolanum.key, pattern_key("MEDIOLANUM"));
            assert_eq!(mediolanum.children[0].key, pattern_key("MEDIOLANUM-ES"));
            assert_eq!(
                mediolanum.children[0].children[0].key,
                pattern_key("MEDIOLANUM-ES21")
            );
        }

        #[test]
        fn a_template_is_kept_as_a_template() {
            let entries = parsed("1:\n  \"*-EN23\": {text: x}\n");
            assert_eq!(entries[0].steps[0].key, StepKey::Template(t("*-EN23")));
        }

        #[test]
        fn the_page_is_kept() {
            let entries = parsed("-2:\n  UBS-EN23: {text: x}\n");
            assert_eq!(entries[0].page, n(-2));
        }

        #[test]
        fn a_text_criterion_selects_the_lines_it_names() {
            let entries = parsed("1:\n  AMUNDI-EN24: {text: '^AMUNDI FUNDS$'}\n");
            let criterion = &entries[0].steps[0].criteria[0];
            assert!(finds(criterion, &[arial("AMUNDI FUNDS")]));
            assert!(!finds(criterion, &[arial("FONDI AMUNDI")]));
        }

        #[test]
        fn a_criterion_intersects_its_keys() {
            let entries =
                parsed("1:\n  EURIZON-IT24: {text: '^Relazione di gestione$', font_size: 26}\n");
            let criterion = &entries[0].steps[0].criteria[0];
            assert!(finds(
                criterion,
                &[line("Arial", 26.0, "Relazione di gestione")]
            ));
            assert!(!finds(
                criterion,
                &[line("Arial", 10.0, "Relazione di gestione")]
            ));
        }

        #[test]
        fn criteria_keep_the_order_they_were_gathered_in() {
            let entries = parsed("1:\n  ARCA-IT24: [{text: '^first$'}, {text: '^second$'}]\n");
            let criteria = &entries[0].steps[0].criteria;
            assert!(finds(&criteria[0], &[arial("first")]));
            assert!(finds(&criteria[1], &[arial("second")]));
            assert!(!finds(&criteria[0], &[arial("second")]));
        }

        #[test]
        fn a_relative_criterion_builds_too() {
            let entries =
                parsed("1:\n  MEDIOLANUM-EN23: {font_of: {text: '^Heading$'}, text: '^Body$'}\n");
            let criterion = &entries[0].steps[0].criteria[0];
            assert!(finds(criterion, &[arial("Heading"), arial("Body")]));
            assert!(!finds(
                criterion,
                &[arial("Heading"), line("Times", 10.0, "Body")]
            ));
        }
    }

    // ------------------------------------------------------------------------------------------
    // The keys of children (§2.2): which slot they fill
    // ------------------------------------------------------------------------------------------

    mod child_keys_under_an_entity {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_language() {
            assert_eq!(compose("MEDIOLANUM", "ES"), pattern_key("MEDIOLANUM-ES"));
        }

        #[test]
        fn a_language_and_a_year() {
            assert_eq!(
                compose("MEDIOLANUM", "IT23"),
                pattern_key("MEDIOLANUM-IT23")
            );
        }

        #[test]
        fn a_language_a_year_and_a_variant() {
            assert_eq!(
                compose("MEDIOLANUM", "IT24.A"),
                pattern_key("MEDIOLANUM-IT24.A")
            );
        }

        #[test]
        fn a_language_a_year_and_a_location() {
            assert_eq!(
                compose("FINECO", "EN23@LUX"),
                pattern_key("FINECO-EN23@LUX")
            );
        }

        #[test]
        fn a_language_a_year_and_an_open_location() {
            assert_eq!(
                compose("FIDEURAM", "IT23@*"),
                pattern_key("FIDEURAM-IT23@*")
            );
        }

        #[test]
        fn a_language_a_year_and_an_open_variant() {
            assert_eq!(
                compose("MEDIOLANUM", "IT24.*"),
                pattern_key("MEDIOLANUM-IT24.*")
            );
        }

        #[test]
        fn a_language_a_year_and_both_slots_open() {
            assert_eq!(compose("X", "EN23@*.*"), pattern_key("X-EN23@*.*"));
        }

        #[test]
        fn no_unquoted_is_norwegian_not_false() {
            assert_eq!(compose("X", "NO"), pattern_key("X-NO"));
        }

        #[test]
        fn a_key_that_is_not_a_suffix_does_not_refine_the_parent() {
            let (location, kind) = rule(&with_child("MEDIOLANUM", "\"I T\""));
            assert_eq!(location, at(Some(1), &["MEDIOLANUM"], Some("I T")));
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { ref parent, .. } if parent == "MEDIOLANUM"),
                "{kind:?}"
            );
        }

        #[test]
        fn a_language_written_with_its_hyphen_is_an_error_that_suggests_the_bare_key() {
            let (location, kind) = rule(&with_child("MEDIOLANUM", "-ES"));
            assert_eq!(location, at(Some(1), &["MEDIOLANUM"], Some("-ES")));
            assert_eq!(
                kind,
                RuleError::HyphenatedChildKey {
                    suggestion: "ES".into()
                }
            );
        }

        #[test]
        fn a_language_and_year_written_with_the_hyphen_suggests_them_without_it() {
            let (_, kind) = rule(&with_child("MEDIOLANUM", "-IT24.*"));
            assert_eq!(
                kind,
                RuleError::HyphenatedChildKey {
                    suggestion: "IT24.*".into()
                }
            );
        }

        #[test]
        fn the_hyphen_message_names_the_key_to_write() {
            let message = parse_err(&with_child("MEDIOLANUM", "-ES")).to_string();
            assert!(message.contains("'ES'"), "{message}");
        }

        #[test]
        fn a_whole_name_nested_is_not_a_suffix() {
            let (location, kind) = rule(&with_child("MEDIOLANUM", "MEDIOLANUM-IT23"));
            assert_eq!(
                location,
                at(Some(1), &["MEDIOLANUM"], Some("MEDIOLANUM-IT23"))
            );
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { .. }),
                "{kind:?}"
            );
        }

        #[test]
        fn a_lowercase_key_that_is_not_a_selection_key_is_a_child_that_does_not_compose() {
            let (location, kind) =
                rule("1:\n  MEDIOLANUM:\n    text: m\n    colour: {text: red}\n");
            assert_eq!(location, at(Some(1), &["MEDIOLANUM"], Some("colour")));
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { .. }),
                "{kind:?}"
            );
        }
    }

    mod child_keys_under_a_language {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_integer_year() {
            assert_eq!(
                compose("MEDIOLANUM-ES", "21"),
                pattern_key("MEDIOLANUM-ES21")
            );
        }

        #[test]
        fn a_quoted_year() {
            assert_eq!(
                compose("MEDIOLANUM-ES", "\"21\""),
                pattern_key("MEDIOLANUM-ES21")
            );
        }

        #[test]
        fn a_one_digit_integer_year_gets_its_leading_zero_back() {
            assert_eq!(
                compose("MEDIOLANUM-ES", "5"),
                pattern_key("MEDIOLANUM-ES05")
            );
        }

        #[test]
        fn year_zero_is_00() {
            assert_eq!(
                compose("MEDIOLANUM-ES", "0"),
                pattern_key("MEDIOLANUM-ES00")
            );
        }

        #[test]
        fn year_99_is_the_last_one() {
            assert_eq!(
                compose("MEDIOLANUM-ES", "99"),
                pattern_key("MEDIOLANUM-ES99")
            );
        }

        #[test]
        fn a_year_with_a_leading_zero_unquoted_is_read_as_written() {
            assert_eq!(
                compose("MEDIOLANUM-ES", "08"),
                pattern_key("MEDIOLANUM-ES08")
            );
        }

        #[test]
        fn an_integer_year_above_99_is_an_error() {
            let (location, _) = rule(&with_child("MEDIOLANUM-ES", "100"));
            assert_eq!(location, at(Some(1), &["MEDIOLANUM-ES"], Some("100")));
        }

        #[test]
        fn a_negative_integer_year_is_an_error() {
            let (location, _) = rule(&with_child("MEDIOLANUM-ES", "-1"));
            assert_eq!(location, at(Some(1), &["MEDIOLANUM-ES"], Some("-1")));
        }

        #[test]
        fn a_year_and_an_open_variant() {
            assert_eq!(
                compose("MEDIOLANUM-ES", "24.*"),
                pattern_key("MEDIOLANUM-ES24.*")
            );
        }

        #[test]
        fn a_year_and_a_numeric_variant_quoted() {
            assert_eq!(
                compose("MEDIOLANUM-IT", "\"24.10\""),
                pattern_key("MEDIOLANUM-IT24.10")
            );
        }

        #[test]
        fn a_year_and_a_location() {
            assert_eq!(
                compose("FINECO-EN", "23@LUX"),
                pattern_key("FINECO-EN23@LUX")
            );
        }

        #[test]
        fn a_language_below_a_language_does_not_refine_the_parent() {
            let (location, kind) = rule(&with_child("MEDIOLANUM-ES", "IT"));
            assert_eq!(location, at(Some(1), &["MEDIOLANUM-ES"], Some("IT")));
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { .. }),
                "{kind:?}"
            );
        }
    }

    mod child_keys_under_an_open_location {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_bare_location() {
            assert_eq!(
                compose("FIDEURAM-IT23@*", "LUX"),
                pattern_key("FIDEURAM-IT23@LUX")
            );
        }

        #[test]
        fn a_location_with_its_separator_is_the_same() {
            assert_eq!(
                compose("FIDEURAM-IT23@*", "\"@LUX\""),
                pattern_key("FIDEURAM-IT23@LUX")
            );
        }

        #[test]
        fn a_three_letter_location() {
            assert_eq!(
                compose("FIDEURAM-IT23@*", "IRL"),
                pattern_key("FIDEURAM-IT23@IRL")
            );
        }

        #[test]
        fn a_location_and_a_variant_fill_the_absent_variant_and_do_not_refine() {
            let (location, kind) = rule(&with_child("FIDEURAM-IT23@*", "LUX.A"));
            assert_eq!(location, at(Some(1), &["FIDEURAM-IT23@*"], Some("LUX.A")));
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { ref parent, .. } if parent == "FIDEURAM-IT23@*"),
                "{kind:?}"
            );
        }

        #[test]
        fn an_integer_is_not_a_location() {
            let (location, kind) = rule(&with_child("FIDEURAM-IT23@*", "5"));
            assert_eq!(location, at(Some(1), &["FIDEURAM-IT23@*"], Some("5")));
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { .. }),
                "{kind:?}"
            );
        }

        #[test]
        fn the_open_location_again_does_not_strictly_refine() {
            let (location, kind) = rule(&with_child("FIDEURAM-IT23@*", "\"@*\""));
            assert_eq!(location, at(Some(1), &["FIDEURAM-IT23@*"], Some("@*")));
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { .. }),
                "{kind:?}"
            );
        }

        #[test]
        fn a_variant_fills_the_absent_variant_and_does_not_refine() {
            let (location, kind) = rule(&with_child("FIDEURAM-IT23@*", ".A"));
            assert_eq!(location, at(Some(1), &["FIDEURAM-IT23@*"], Some(".A")));
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { .. }),
                "{kind:?}"
            );
        }
    }

    mod child_keys_under_an_open_variant {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_bare_variant() {
            assert_eq!(
                compose("MEDIOLANUM-IT24.*", "A"),
                pattern_key("MEDIOLANUM-IT24.A")
            );
        }

        #[test]
        fn a_variant_with_its_separator_is_the_same() {
            assert_eq!(
                compose("MEDIOLANUM-IT24.*", ".A"),
                pattern_key("MEDIOLANUM-IT24.A")
            );
        }

        #[test]
        fn an_integer_variant_is_written_in_decimal() {
            assert_eq!(
                compose("MEDIOLANUM-IT24.*", "1"),
                pattern_key("MEDIOLANUM-IT24.1")
            );
        }

        #[test]
        fn an_integer_variant_gets_no_padding() {
            assert_eq!(
                compose("MEDIOLANUM-IT24.*", "10"),
                pattern_key("MEDIOLANUM-IT24.10")
            );
        }

        #[test]
        fn a_negative_integer_variant_is_its_decimal_text() {
            assert_eq!(
                compose("MEDIOLANUM-IT24.*", "-3"),
                pattern_key("MEDIOLANUM-IT24.-3")
            );
        }

        #[test]
        fn integer_zero_is_variant_0() {
            assert_eq!(
                compose("MEDIOLANUM-IT24.*", "0"),
                pattern_key("MEDIOLANUM-IT24.0")
            );
        }

        #[test]
        fn a_quoted_variant_with_a_leading_zero_keeps_it() {
            assert_eq!(
                compose("MEDIOLANUM-IT24.*", "\"01\""),
                pattern_key("MEDIOLANUM-IT24.01")
            );
        }

        #[test]
        fn a_numeric_variant_with_its_separator_quoted() {
            assert_eq!(
                compose("MEDIOLANUM-IT24.*", "\".10\""),
                pattern_key("MEDIOLANUM-IT24.10")
            );
        }

        #[test]
        fn a_location_fills_the_absent_location_and_does_not_refine() {
            let (location, kind) = rule(&with_child("MEDIOLANUM-IT24.*", "\"@LUX\""));
            assert_eq!(location, at(Some(1), &["MEDIOLANUM-IT24.*"], Some("@LUX")));
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { .. }),
                "{kind:?}"
            );
        }

        #[test]
        fn a_variant_named_like_a_selection_key_is_a_criterion() {
            let step = only_step("1:\n  MEDIOLANUM-IT24.*:\n    text: '^Best Brands$'\n");
            assert_eq!(
                step,
                leaf("MEDIOLANUM-IT24.*", &["{text: '^Best Brands$'}"])
            );
        }

        #[test]
        fn a_variant_named_like_a_selection_key_is_a_child_with_its_separator() {
            let step = only_step("1:\n  MEDIOLANUM-IT24.*:\n    .text: {text: '^Best Brands$'}\n");
            assert_eq!(step, step_with_text_child());
        }

        fn step_with_text_child() -> VerboseStep {
            step(
                "MEDIOLANUM-IT24.*",
                &[],
                vec![leaf("MEDIOLANUM-IT24.text", &["{text: '^Best Brands$'}"])],
            )
        }
    }

    mod child_keys_under_two_open_slots {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_location_with_its_separator_leaves_the_variant_open() {
            assert_eq!(
                compose("X-EN23@*.*", "\"@LUX\""),
                pattern_key("X-EN23@LUX.*")
            );
        }

        #[test]
        fn a_variant_with_its_separator_leaves_the_location_open() {
            assert_eq!(compose("X-EN23@*.*", ".A"), pattern_key("X-EN23@*.A"));
        }

        #[test]
        fn a_location_and_a_variant() {
            assert_eq!(
                compose("X-EN23@*.*", "\"@LUX.A\""),
                pattern_key("X-EN23@LUX.A")
            );
        }

        #[test]
        fn a_location_and_an_open_variant() {
            assert_eq!(
                compose("X-EN23@*.*", "\"@LUX.*\""),
                pattern_key("X-EN23@LUX.*")
            );
        }

        #[test]
        fn an_open_location_and_a_variant() {
            assert_eq!(compose("X-EN23@*.*", "\"@*.A\""), pattern_key("X-EN23@*.A"));
        }

        #[test]
        fn a_bare_key_needs_its_separator() {
            let (location, kind) = rule(&with_child("X-EN23@*.*", "LUX"));
            assert_eq!(location, at(Some(1), &["X-EN23@*.*"], Some("LUX")));
            assert_eq!(kind, RuleError::SeparatorRequired);
        }

        #[test]
        fn a_bare_variant_needs_its_separator_too() {
            let (_, kind) = rule(&with_child("X-EN23@*.*", "A"));
            assert_eq!(kind, RuleError::SeparatorRequired);
        }

        #[test]
        fn the_error_names_both_readings_of_the_bare_key() {
            let message = parse_err(&with_child("X-EN23@*.*", "LUX")).to_string();
            assert!(message.contains("@LUX"), "{message}");
            assert!(message.contains(".LUX"), "{message}");
        }

        #[test]
        fn a_bare_integer_is_an_error_too() {
            let (location, _) = rule(&with_child("X-EN23@*.*", "1"));
            assert_eq!(location, at(Some(1), &["X-EN23@*.*"], Some("1")));
        }

        #[test]
        fn the_two_open_slots_again_do_not_strictly_refine() {
            let (_, kind) = rule(&with_child("X-EN23@*.*", "\"@*.*\""));
            assert!(
                matches!(kind, RuleError::KeyDoesNotRefineParent { .. }),
                "{kind:?}"
            );
        }
    }

    mod a_complete_parent_has_no_children {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_complete_top_level_step_with_a_child_is_an_error_that_suggests_the_open_location() {
            let (location, kind) = rule("1:\n  FINECO-EN23:\n    \"@LUX\": {text: x}\n");
            assert_eq!(location.page, Some(1));
            assert_eq!(
                kind,
                RuleError::CompleteParentWithChildren {
                    parent: "FINECO-EN23".into(),
                    suggestion: "FINECO-EN23@*".into()
                }
            );
        }

        #[test]
        fn the_message_names_the_suggestion() {
            let message = parse_err("1:\n  FINECO-EN23:\n    \"@LUX\": {text: x}\n").to_string();
            assert!(message.contains("FINECO-EN23@*"), "{message}");
        }

        #[test]
        fn a_complete_child_with_a_child_is_an_error_too() {
            let (location, kind) = rule(
                "1:\n  MEDIOLANUM:\n    text: m\n    IT23:\n      text: i\n      .A: {text: a}\n",
            );
            assert_eq!(
                location,
                at(Some(1), &["MEDIOLANUM", "MEDIOLANUM-IT23"], Some(".A"))
            );
            assert!(
                matches!(kind, RuleError::CompleteParentWithChildren { ref parent, .. } if parent == "MEDIOLANUM-IT23"),
                "{kind:?}"
            );
        }

        #[test]
        fn a_complete_step_with_criteria_and_no_children_is_fine() {
            assert_eq!(
                only_step("1:\n  FINECO-EN23@LUX: {text: x}\n"),
                leaf("FINECO-EN23@LUX", &["{text: x}"])
            );
        }

        #[test]
        fn a_complete_unconditional_child_is_fine() {
            let step = only_step("1:\n  ASTERIA:\n    text: a\n    EN23: ~\n");
            assert_eq!(step.children, vec![unconditional("ASTERIA-EN23")]);
        }
    }

    // ------------------------------------------------------------------------------------------
    // Numeric keys that are not integers (§2.3.2, R5)
    // ------------------------------------------------------------------------------------------

    mod keys_that_yaml_does_not_read_as_text {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_year_and_variant_unquoted_is_a_float_and_an_error() {
            let (location, kind) = rule("1:\n  MEDIOLANUM-IT:\n    24.10: {text: x}\n");
            assert_eq!(kind, RuleError::NonStringKey);
            assert_eq!(location.page, Some(1));
            assert_eq!(location.steps, vec!["MEDIOLANUM-IT".to_string()]);
        }

        #[test]
        fn a_numeric_variant_with_its_separator_unquoted_is_a_float_and_an_error() {
            let (location, kind) = rule("1:\n  MEDIOLANUM-IT24.*:\n    .10: {text: x}\n");
            assert_eq!(kind, RuleError::NonStringKey);
            assert_eq!(location.steps, vec!["MEDIOLANUM-IT24.*".to_string()]);
        }

        #[test]
        fn a_boolean_key_is_an_error() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-IT24.*:\n    true: {text: x}\n");
            assert_eq!(kind, RuleError::NonStringKey);
        }

        #[test]
        fn a_null_key_is_an_error() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-IT24.*:\n    ~: {text: x}\n");
            assert_eq!(kind, RuleError::NonStringKey);
        }

        #[test]
        fn the_message_suggests_the_quotes() {
            let message = parse_err("1:\n  MEDIOLANUM-IT:\n    24.10: {text: x}\n").to_string();
            assert!(message.contains("quote"), "{message}");
        }

        #[test]
        fn the_same_key_quoted_is_read() {
            assert_eq!(
                compose("MEDIOLANUM-IT", "\"24.10\""),
                pattern_key("MEDIOLANUM-IT24.10")
            );
        }
    }

    // ------------------------------------------------------------------------------------------
    // Duplicate keys (§2.3.3, R6)
    // ------------------------------------------------------------------------------------------

    mod duplicate_keys {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_same_step_twice_in_a_mapping_is_an_error_naming_page_and_key() {
            let (location, kind) =
                rule("1:\n  MEDIOLANUM: {text: a}\n  AMUNDI: {text: b}\n  MEDIOLANUM: {text: c}\n");
            assert_eq!(kind, RuleError::DuplicateKey);
            assert_eq!(location, at(Some(1), &[], Some("MEDIOLANUM")));
        }

        #[test]
        fn the_message_suggests_the_list_form() {
            let message =
                parse_err("1:\n  MEDIOLANUM: {text: a}\n  MEDIOLANUM: {text: c}\n").to_string();
            assert!(message.contains("list"), "{message}");
            assert!(message.contains("page 1"), "{message}");
            assert!(message.contains("MEDIOLANUM"), "{message}");
        }

        #[test]
        fn the_same_page_twice_in_a_mapping_is_an_error() {
            let (location, kind) =
                rule("1:\n  AMUNDI-EN24: {text: a}\n1:\n  KAIROS-EN23: {text: b}\n");
            assert_eq!(kind, RuleError::DuplicateKey);
            assert_eq!(location, at(None, &[], Some("1")));
        }

        #[test]
        fn the_same_child_twice_in_a_mapping_is_an_error() {
            let (location, kind) =
                rule("1:\n  MEDIOLANUM:\n    text: m\n    EN23: {text: a}\n    EN23: {text: b}\n");
            assert_eq!(kind, RuleError::DuplicateKey);
            assert_eq!(location, at(Some(1), &["MEDIOLANUM"], Some("EN23")));
        }

        #[test]
        fn the_same_selection_key_twice_in_a_body_is_an_error() {
            let (location, kind) = rule("1:\n  MEDIOLANUM-EN23:\n    text: a\n    text: b\n");
            assert_eq!(kind, RuleError::DuplicateKey);
            assert_eq!(location, at(Some(1), &["MEDIOLANUM-EN23"], Some("text")));
        }

        #[test]
        fn an_integer_year_and_its_string_are_the_same_child() {
            let (location, kind) =
                rule("1:\n  MEDIOLANUM-ES:\n    21: {text: a}\n    \"21\": {text: b}\n");
            assert_eq!(kind, RuleError::DuplicateKey);
            assert_eq!(location.page, Some(1));
            assert_eq!(location.steps, vec!["MEDIOLANUM-ES".to_string()]);
            assert_eq!(location.key.as_deref(), Some("21"));
        }

        #[test]
        fn a_one_digit_integer_year_and_its_padded_string_are_the_same_child() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-ES:\n    8: {text: a}\n    08: {text: b}\n");
            assert_eq!(kind, RuleError::DuplicateKey);
        }

        #[test]
        fn a_bare_location_and_the_same_with_its_separator_are_the_same_child() {
            let (location, kind) =
                rule("1:\n  FIDEURAM-IT23@*:\n    LUX: {text: a}\n    \"@LUX\": {text: b}\n");
            assert_eq!(kind, RuleError::DuplicateKey);
            assert_eq!(location, at(Some(1), &["FIDEURAM-IT23@*"], Some("@LUX")));
        }

        #[test]
        fn a_bare_variant_and_the_same_with_its_separator_are_the_same_child() {
            let (location, kind) =
                rule("1:\n  MEDIOLANUM-IT24.*:\n    A: {text: a}\n    .A: {text: b}\n");
            assert_eq!(kind, RuleError::DuplicateKey);
            assert_eq!(location, at(Some(1), &["MEDIOLANUM-IT24.*"], Some(".A")));
        }

        #[test]
        fn an_integer_variant_and_its_string_are_the_same_child() {
            let (_, kind) =
                rule("1:\n  MEDIOLANUM-IT24.*:\n    1: {text: a}\n    \"1\": {text: b}\n");
            assert_eq!(kind, RuleError::DuplicateKey);
        }

        #[test]
        fn an_integer_variant_and_a_padded_string_are_two_children() {
            let step =
                only_step("1:\n  MEDIOLANUM-IT24.*:\n    1: {text: a}\n    \"01\": {text: b}\n");
            let keys: Vec<StepKey> = step.children.into_iter().map(|c| c.key).collect();
            assert_eq!(
                keys,
                vec![
                    pattern_key("MEDIOLANUM-IT24.1"),
                    pattern_key("MEDIOLANUM-IT24.01")
                ]
            );
        }

        #[test]
        fn duplicates_are_judged_per_mapping_not_across_mappings() {
            let text = "1:\n  MEDIOLANUM-ES:\n    21: {text: a}\n2:\n  MEDIOLANUM-ES:\n    21: {text: b}\n";
            assert_eq!(normalized(text).len(), 2);
        }
    }

    // ------------------------------------------------------------------------------------------
    // `*` as entity (§1.4, R8) and the YAML traps of `*` and `@` (§2.3.1)
    // ------------------------------------------------------------------------------------------

    mod wildcards {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_quoted_template_with_language_and_year_is_a_top_level_template() {
            assert_eq!(
                only_step("1:\n  \"*-EN23\": {text: x}\n"),
                template("*-EN23", &["{text: x}"])
            );
        }

        #[test]
        fn a_template_with_the_language_only() {
            assert_eq!(
                only_step("1:\n  \"*-EN\": {text: x}\n"),
                template("*-EN", &["{text: x}"])
            );
        }

        #[test]
        fn a_template_with_an_open_location() {
            assert_eq!(
                only_step("1:\n  \"*-EN23@*\": {text: x}\n"),
                template("*-EN23@*", &["{text: x}"])
            );
        }

        #[test]
        fn a_template_in_the_verbose_form() {
            assert_eq!(
                only_step("- 1:\n    - \"*-EN23\": [{text: x}]\n"),
                template("*-EN23", &["{text: x}"])
            );
        }

        #[test]
        fn an_unconditional_template_is_allowed_at_the_top_level() {
            assert_eq!(only_step("1:\n  \"*-EN23\": ~\n"), template("*-EN23", &[]));
        }

        #[test]
        fn an_unconditional_template_with_an_empty_body_is_the_same() {
            assert_eq!(only_step("1:\n  \"*-EN23\": []\n"), template("*-EN23", &[]));
            assert_eq!(only_step("1:\n  \"*-EN23\": {}\n"), template("*-EN23", &[]));
        }

        #[test]
        fn an_unconditional_template_decides_by_exclusion_for_the_entity_already_found() {
            let text = "1:\n  MEDIOLANUM: {text: '^Mediolanum'}\n  \"*-EN23\": ~\n";
            let document = FakeDocument::new(vec![vec![arial("Mediolanum")]]);
            match classify(
                &["MEDIOLANUM-EN23", "MEDIOLANUM-IT23"],
                parsed(text),
                &document,
            ) {
                DocClassification::Complete(complete) => {
                    assert_eq!(complete.format, p("MEDIOLANUM-EN23"));
                    assert!(complete.by_exclusion);
                }
                other => panic!("expected MEDIOLANUM-EN23 by exclusion, got {other:?}"),
            }
        }

        #[test]
        fn a_star_alone_writes_no_language_and_is_an_error() {
            let (location, _) = rule("1:\n  \"*\": {text: x}\n");
            assert_eq!(location, at(Some(1), &[], Some("*")));
        }

        #[test]
        fn a_template_below_the_top_level_is_an_error() {
            let (location, kind) =
                rule("1:\n  MEDIOLANUM:\n    text: m\n    \"*-EN23\": {text: x}\n");
            assert_eq!(kind, RuleError::TemplateNotAtTopLevel);
            assert_eq!(location, at(Some(1), &["MEDIOLANUM"], Some("*-EN23")));
        }

        #[test]
        fn a_template_with_children_is_an_error() {
            let (location, kind) = rule("1:\n  \"*-EN\":\n    text: x\n    23: {text: y}\n");
            assert_eq!(kind, RuleError::TemplateWithChildren);
            assert_eq!(location.page, Some(1));
        }

        #[test]
        fn a_template_with_children_in_the_list_form_is_an_error_too() {
            let (_, kind) = rule("1:\n  \"*-EN\":\n    - {text: x}\n    - 23: [{text: y}]\n");
            assert_eq!(kind, RuleError::TemplateWithChildren);
        }

        #[test]
        fn a_star_inside_the_entity_is_an_error() {
            let (location, _) = rule("1:\n  \"MEDIO*-EN23\": {text: x}\n");
            assert_eq!(location, at(Some(1), &[], Some("MEDIO*-EN23")));
        }

        #[test]
        fn a_star_in_the_year_is_an_error() {
            let (location, _) = rule("1:\n  \"MEDIOLANUM-IT2*\": {text: x}\n");
            assert_eq!(location, at(Some(1), &[], Some("MEDIOLANUM-IT2*")));
        }

        #[test]
        fn a_star_inside_a_variant_is_an_error() {
            let (location, _) = rule("1:\n  \"MEDIOLANUM-IT24.A*\": {text: x}\n");
            assert_eq!(location, at(Some(1), &[], Some("MEDIOLANUM-IT24.A*")));
        }

        #[test]
        fn a_star_in_a_child_year_is_an_error() {
            let (location, _) = rule(&with_child("MEDIOLANUM-IT", "\"2*\""));
            assert_eq!(location, at(Some(1), &["MEDIOLANUM-IT"], Some("2*")));
        }

        #[test]
        fn an_open_location_at_the_top_level_is_a_pattern() {
            assert_eq!(
                only_step("1:\n  FIDEURAM-IT23@*: {text: x}\n").key,
                pattern_key("FIDEURAM-IT23@*")
            );
        }

        #[test]
        fn an_open_variant_at_the_top_level_is_a_pattern() {
            assert_eq!(
                only_step("1:\n  MEDIOLANUM-IT24.*: {text: x}\n").key,
                pattern_key("MEDIOLANUM-IT24.*")
            );
        }

        #[test]
        fn both_slots_open_at_the_top_level_is_a_pattern() {
            assert_eq!(
                only_step("1:\n  X-EN23@*.*: {text: x}\n").key,
                pattern_key("X-EN23@*.*")
            );
        }

        #[test]
        fn an_unquoted_template_is_a_yaml_error_with_the_hint_to_quote_it() {
            match parse_err("1:\n  *-EN23: {text: x}\n") {
                DocClassifyYamlError::Syntax { reason, hint } => {
                    assert!(
                        reason.contains("unknown anchor"),
                        "the YAML reader's own words must stay: {reason}"
                    );
                    let hint = hint.expect("an unquoted `*` must come with a hint");
                    assert!(hint.contains("quote"), "{hint}");
                }
                other => panic!("expected a syntax error, got {other:?}"),
            }
        }

        #[test]
        fn an_unquoted_template_in_a_list_gets_the_hint_too() {
            match parse_err("- 1:\n    - *-EN23: [{text: x}]\n") {
                DocClassifyYamlError::Syntax { hint, .. } => assert!(hint.is_some()),
                other => panic!("expected a syntax error, got {other:?}"),
            }
        }

        #[test]
        fn an_unquoted_location_is_a_yaml_error_with_the_hint_to_quote_it() {
            match parse_err("1:\n  FIDEURAM-IT23@*:\n    @LUX: {text: x}\n") {
                DocClassifyYamlError::Syntax { hint, .. } => {
                    let hint = hint.expect("an unquoted `@` must come with a hint");
                    assert!(hint.contains("quote"), "{hint}");
                }
                other => panic!("expected a syntax error, got {other:?}"),
            }
        }

        #[test]
        fn the_hint_shows_in_the_message() {
            let message = parse_err("1:\n  *-EN23: {text: x}\n").to_string();
            assert!(message.contains("quote"), "{message}");
        }
    }

    // ------------------------------------------------------------------------------------------
    // Pages
    // ------------------------------------------------------------------------------------------

    mod pages {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn page_zero_is_an_error_in_the_mapping_form() {
            let (location, kind) = rule("0:\n  AMUNDI-EN24: {text: x}\n");
            assert_eq!(kind, RuleError::PageZero);
            assert_eq!(location, at(None, &[], Some("0")));
        }

        #[test]
        fn page_zero_is_an_error_in_the_list_form() {
            let (_, kind) = rule("- 0:\n    - AMUNDI-EN24: [{text: x}]\n");
            assert_eq!(kind, RuleError::PageZero);
        }

        #[test]
        fn a_float_page_is_an_error() {
            let (location, kind) = rule("1.5:\n  AMUNDI-EN24: {text: x}\n");
            assert_eq!(kind, RuleError::PageNotAnInteger);
            assert_eq!(location.page, None);
        }

        #[test]
        fn a_word_page_is_an_error() {
            let (location, kind) = rule("uno:\n  AMUNDI-EN24: {text: x}\n");
            assert_eq!(kind, RuleError::PageNotAnInteger);
            assert_eq!(location, at(None, &[], Some("uno")));
        }

        #[test]
        fn a_word_page_in_the_list_form_is_an_error() {
            let (_, kind) = rule("- uno:\n    - AMUNDI-EN24: [{text: x}]\n");
            assert_eq!(kind, RuleError::PageNotAnInteger);
        }

        #[test]
        fn a_quoted_page_is_not_an_integer() {
            let (location, kind) = rule("\"1\":\n  AMUNDI-EN24: {text: x}\n");
            assert_eq!(kind, RuleError::PageNotAnInteger);
            assert_eq!(location, at(None, &[], Some("1")));
        }

        #[test]
        fn a_quoted_page_in_the_list_form_is_not_an_integer_either() {
            let (_, kind) = rule("- \"1\":\n    - AMUNDI-EN24: [{text: x}]\n");
            assert_eq!(kind, RuleError::PageNotAnInteger);
        }

        #[test]
        fn an_element_of_the_entries_with_two_pages_is_an_error() {
            let (_, kind) =
                rule("- {1: [{AMUNDI-EN24: [{text: a}]}], 3: [{KAIROS-EN23: [{text: b}]}]}\n");
            assert_eq!(kind, RuleError::SeveralPages);
        }

        #[test]
        fn a_page_with_null_steps_is_an_error() {
            let (location, kind) = rule("1: ~\n");
            assert_eq!(kind, RuleError::PageWithoutSteps);
            assert_eq!(location.page, Some(1));
        }

        #[test]
        fn a_page_with_an_empty_list_of_steps_is_an_error() {
            let (_, kind) = rule("1: []\n");
            assert_eq!(kind, RuleError::PageWithoutSteps);
        }

        #[test]
        fn a_page_with_an_empty_mapping_of_steps_is_an_error() {
            let (_, kind) = rule("1: {}\n");
            assert_eq!(kind, RuleError::PageWithoutSteps);
        }

        #[test]
        fn a_page_with_no_steps_in_the_list_form_is_an_error() {
            let (_, kind) = rule("- 1: []\n");
            assert_eq!(kind, RuleError::PageWithoutSteps);
        }

        #[test]
        fn a_page_without_steps_among_valid_ones_is_still_an_error() {
            let (location, kind) = rule("1:\n  AMUNDI-EN24: {text: x}\n3:\n");
            assert_eq!(kind, RuleError::PageWithoutSteps);
            assert_eq!(location.page, Some(3));
        }

        #[test]
        fn the_last_page_is_minus_one() {
            assert_eq!(normalized("-1:\n  AMUNDI-EN24: {text: x}\n")[0].page, n(-1));
        }
    }

    // ------------------------------------------------------------------------------------------
    // Elements, criteria and bodies (§2.1, R4)
    // ------------------------------------------------------------------------------------------

    mod elements_and_criteria {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_element_mixing_a_criterion_and_a_step_is_an_error() {
            let (location, kind) = rule("1:\n  MEDIOLANUM:\n    - {text: m, EN23: ~}\n");
            assert_eq!(kind, RuleError::MixedElement);
            assert_eq!(location.page, Some(1));
            assert_eq!(location.steps, vec!["MEDIOLANUM".to_string()]);
        }

        #[test]
        fn an_element_with_a_selection_key_and_an_unknown_key_mixes_them() {
            let (_, kind) = rule("1:\n  MEDIOLANUM:\n    - {text: m, colour: {text: red}}\n");
            assert_eq!(kind, RuleError::MixedElement);
        }

        #[test]
        fn an_element_of_a_body_with_two_step_keys_is_an_error() {
            let (location, kind) =
                rule("1:\n  MEDIOLANUM:\n    - {text: m}\n    - {EN23: ~, IT23: ~}\n");
            assert_eq!(kind, RuleError::SeveralStepKeys);
            assert_eq!(location.steps, vec!["MEDIOLANUM".to_string()]);
        }

        #[test]
        fn an_element_of_a_pages_steps_with_two_step_keys_is_an_error() {
            let (location, kind) =
                rule("- 1:\n    - {AMUNDI-EN24: [{text: a}], KAIROS-EN23: [{text: b}]}\n");
            assert_eq!(kind, RuleError::SeveralStepKeys);
            assert_eq!(location.page, Some(1));
        }

        #[test]
        fn an_empty_criterion_in_a_list_is_an_error() {
            let (location, kind) = rule("1:\n  MEDIOLANUM-EN23: [{}]\n");
            assert_eq!(kind, RuleError::EmptyCriterion);
            assert_eq!(location, at(Some(1), &["MEDIOLANUM-EN23"], None));
        }

        #[test]
        fn an_empty_union_states_nothing() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-EN23: [{any_of: []}]\n");
            assert_eq!(kind, RuleError::EmptyCriterion);
        }

        #[test]
        fn an_empty_union_as_the_body_states_nothing_either() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-EN23: {any_of: []}\n");
            assert_eq!(kind, RuleError::EmptyCriterion);
        }

        #[test]
        fn an_empty_intersection_states_nothing() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-EN23: [{all_of: []}]\n");
            assert_eq!(kind, RuleError::EmptyCriterion);
        }

        #[test]
        fn a_difference_alone_states_nothing() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-EN23: [{without: [{text: x}]}]\n");
            assert_eq!(kind, RuleError::EmptyCriterion);
        }

        #[test]
        fn a_difference_alone_as_the_body_states_nothing_either() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-EN23: {without: [{text: x}]}\n");
            assert_eq!(kind, RuleError::EmptyCriterion);
        }

        #[test]
        fn a_criterion_that_is_not_a_valid_selection_is_an_error() {
            let (location, kind) = rule("1:\n  MEDIOLANUM-EN23: {text: x, font_size: -1}\n");
            assert!(
                matches!(kind, RuleError::InvalidSelection { .. }),
                "{kind:?}"
            );
            assert_eq!(location, at(Some(1), &["MEDIOLANUM-EN23"], None));
        }

        #[test]
        fn a_criterion_of_the_wrong_type_is_an_invalid_selection() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-EN23: {text: [a, b]}\n");
            assert!(
                matches!(kind, RuleError::InvalidSelection { .. }),
                "{kind:?}"
            );
        }

        #[test]
        fn an_unknown_key_inside_a_criterion_is_an_invalid_selection() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-EN23: {text: x, area: {x_minimum: 3}}\n");
            assert!(
                matches!(kind, RuleError::InvalidSelection { .. }),
                "{kind:?}"
            );
        }

        #[test]
        fn an_invalid_selection_deep_in_the_tree_names_the_path() {
            let (location, _) = rule(
                "1:\n  MEDIOLANUM:\n    text: m\n    IT24.*:\n      A: {text: a, font_size: [9, 8]}\n",
            );
            assert_eq!(
                location,
                at(
                    Some(1),
                    &["MEDIOLANUM", "MEDIOLANUM-IT24.*", "MEDIOLANUM-IT24.A"],
                    None
                )
            );
        }

        #[test]
        fn the_compact_grammar_is_not_a_criterion() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-EN23: ['ArialMT & text(\"x\")']\n");
            assert!(matches!(kind, RuleError::Shape { .. }), "{kind:?}");
        }

        #[test]
        fn a_scalar_body_is_an_error() {
            let (location, kind) = rule("1:\n  MEDIOLANUM-EN23: '^Mediolanum'\n");
            assert!(matches!(kind, RuleError::Shape { .. }), "{kind:?}");
            assert_eq!(location.page, Some(1));
        }

        #[test]
        fn a_scalar_element_of_a_pages_steps_is_an_error() {
            let (_, kind) = rule("- 1:\n    - MEDIOLANUM-EN23\n");
            assert!(matches!(kind, RuleError::Shape { .. }), "{kind:?}");
        }

        #[test]
        fn a_scalar_page_body_is_an_error() {
            let (_, kind) = rule("1: MEDIOLANUM-EN23\n");
            assert!(matches!(kind, RuleError::Shape { .. }), "{kind:?}");
        }

        #[test]
        fn a_scalar_top_level_is_an_error() {
            let (_, kind) = rule("hello\n");
            assert!(matches!(kind, RuleError::Shape { .. }), "{kind:?}");
        }

        #[test]
        fn a_scalar_element_of_the_entries_is_an_error() {
            let (_, kind) = rule("- 1\n");
            assert!(matches!(kind, RuleError::Shape { .. }), "{kind:?}");
        }

        #[test]
        fn a_top_level_key_that_is_not_a_pattern_is_an_error() {
            let (location, kind) = rule("1:\n  -IT24: {text: x}\n");
            assert!(matches!(kind, RuleError::InvalidKey { .. }), "{kind:?}");
            assert_eq!(location, at(Some(1), &[], Some("-IT24")));
        }

        #[test]
        fn a_top_level_key_truncated_on_the_left_is_an_error() {
            let (_, kind) = rule("1:\n  MEDIOLANUM-24: {text: x}\n");
            assert!(matches!(kind, RuleError::InvalidKey { .. }), "{kind:?}");
        }
    }

    // ------------------------------------------------------------------------------------------
    // Deep nesting
    // ------------------------------------------------------------------------------------------

    mod deep_nesting {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn five_levels_compose_slot_by_slot() {
            let text = r#"
1:
  MEDIOLANUM:
    text: m
    IT:
      24@*.*:
        "@LUX":
          text: l
          A: {text: a}
          B: ~
"#;
            assert_eq!(
                only_step(text),
                step(
                    "MEDIOLANUM",
                    &["{text: m}"],
                    vec![step(
                        "MEDIOLANUM-IT",
                        &[],
                        vec![step(
                            "MEDIOLANUM-IT24@*.*",
                            &[],
                            vec![step(
                                "MEDIOLANUM-IT24@LUX.*",
                                &["{text: l}"],
                                vec![
                                    leaf("MEDIOLANUM-IT24@LUX.A", &["{text: a}"]),
                                    unconditional("MEDIOLANUM-IT24@LUX.B")
                                ],
                            )],
                        )],
                    )],
                )
            );
        }

        #[test]
        fn criteria_at_every_level_stay_at_their_level() {
            let text = "1:\n  MEDIOLANUM:\n    text: m\n    ES:\n      text: e\n      24.*:\n        text: s\n        A: {text: a}\n";
            assert_eq!(
                only_step(text),
                step(
                    "MEDIOLANUM",
                    &["{text: m}"],
                    vec![step(
                        "MEDIOLANUM-ES",
                        &["{text: e}"],
                        vec![step(
                            "MEDIOLANUM-ES24.*",
                            &["{text: s}"],
                            vec![leaf("MEDIOLANUM-ES24.A", &["{text: a}"])]
                        )],
                    )],
                )
            );
        }

        #[test]
        fn verbose_and_simplified_agree_at_depth() {
            let simplified = "1:\n  MEDIOLANUM:\n    text: m\n    ES:\n      text: e\n      24.*:\n        A: {text: a}\n        B: ~\n";
            let verbose = r#"
- 1:
    - MEDIOLANUM:
        - {text: m}
        - ES:
            - {text: e}
            - 24.*:
                - A:
                    - {text: a}
                - B: []
"#;
            assert_eq!(normalized(simplified), normalized(verbose));
        }

        #[test]
        fn an_error_at_depth_names_every_enclosing_step() {
            let (location, _) = rule(
                "1:\n  MEDIOLANUM:\n    text: m\n    IT:\n      24.*:\n        \"@LUX\": {text: x}\n",
            );
            assert_eq!(
                location,
                at(
                    Some(1),
                    &["MEDIOLANUM", "MEDIOLANUM-IT", "MEDIOLANUM-IT24.*"],
                    Some("@LUX")
                )
            );
        }

        #[test]
        fn the_message_shows_the_page_the_path_and_the_key() {
            let message = parse_err(
                "3:\n  MEDIOLANUM:\n    text: m\n    IT24.*:\n      \"@LUX\": {text: x}\n",
            )
            .to_string();
            assert!(message.contains("page 3"), "{message}");
            assert!(message.contains("MEDIOLANUM-IT24.*"), "{message}");
            assert!(message.contains("@LUX"), "{message}");
        }
    }

    // ------------------------------------------------------------------------------------------
    // Order (the order of mappings is semantics, §2.3)
    // ------------------------------------------------------------------------------------------

    mod order {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_order_of_steps_in_a_mapping_is_kept_and_not_sorted() {
            let text = "1:\n  ZETA-EN23: {text: z}\n  ALPHA-EN23: {text: a}\n  MID-EN23: {text: m}\n  BETA-EN23: {text: b}\n  OMEGA-EN23: {text: o}\n";
            let keys: Vec<String> = normalized(text)[0]
                .steps
                .iter()
                .map(|s| s.key.to_string())
                .collect();
            assert_eq!(
                keys,
                vec![
                    "ZETA-EN23",
                    "ALPHA-EN23",
                    "MID-EN23",
                    "BETA-EN23",
                    "OMEGA-EN23"
                ]
            );
        }

        #[test]
        fn the_order_of_children_in_a_mixed_mapping_is_kept() {
            let text =
                "1:\n  X:\n    ZZ: {text: z}\n    text: x\n    AA: {text: a}\n    MM: {text: m}\n";
            let keys: Vec<String> = only_step(text)
                .children
                .iter()
                .map(|s| s.key.to_string())
                .collect();
            assert_eq!(keys, vec!["X-ZZ", "X-AA", "X-MM"]);
        }

        #[test]
        fn the_order_of_pages_in_a_mapping_is_kept() {
            let text = "5:\n  A-EN23: {text: a}\n-2:\n  B-EN23: {text: b}\n1:\n  C-EN23: {text: c}\n3:\n  D-EN23: {text: d}\n";
            let pages: Vec<i32> = normalized(text).iter().map(|e| e.page.get()).collect();
            assert_eq!(pages, vec![5, -2, 1, 3]);
        }

        #[test]
        fn the_order_of_numeric_children_is_kept() {
            let text = "1:\n  MEDIOLANUM-ES:\n    23: {text: b}\n    21: {text: a}\n    \"05\": {text: c}\n";
            let keys: Vec<String> = only_step(text)
                .children
                .iter()
                .map(|s| s.key.to_string())
                .collect();
            assert_eq!(
                keys,
                vec!["MEDIOLANUM-ES23", "MEDIOLANUM-ES21", "MEDIOLANUM-ES05"]
            );
        }

        #[test]
        fn the_built_entries_keep_the_order_too() {
            let text = "1:\n  ZETA-EN23: {text: z}\n  ALPHA-EN23: {text: a}\n";
            let keys: Vec<String> = parsed(text)[0]
                .steps
                .iter()
                .map(|s| s.key.to_string())
                .collect();
            assert_eq!(keys, vec!["ZETA-EN23", "ALPHA-EN23"]);
        }
    }
}
