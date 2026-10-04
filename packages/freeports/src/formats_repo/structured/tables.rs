//! The CSV tables of the structured level: reading, validating and joining.
//!
//! Six files in three groups:
//!
//! - **investments** — a main arguments table with one row per pipe, two tables with at most one row per pipe, and a deselection table with as many rows per pipe as wanted;
//! - **page_classify** — one arguments table with several rows per pipe, one per header set;
//! - **fund_assets** — one arguments table with **one** row per pipe, carrying the parameters of
//!   all three segments.
//!
//! Every table is a `Deserialize` struct read with the `csv` crate, the composite key is a
//! [`ComputedId`], the join is a hash map, and every validation is a function reporting **the row
//! number** — because a formats repository is edited by hand in a spreadsheet, and an error that
//! cannot say which row is nearly useless.
//!
//! # Empty cells
//!
//! Every optional column is an `Option<T>` and an empty cell is `None`. This needs a small family
//! of dedicated deserializers: serde on an `Option<i16>` would fail on the empty string rather than
//! read it as absent, and on a `bool` it would not accept the upper-case `TRUE` that every
//! spreadsheet writes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Deserializer};

use crate::input::document::selection::is_pdfline_selection;

use super::super::id_format::{ComputedId, FkRelation, computed_ids, id_matches};

/// The directory of the structured CSV files inside a formats repository.
pub const STRUCTURED_DIR: &str = "content/algorithms/structured";

/// The pipeline that investments rows belong to when they declare none.
pub const INVESTMENTS_PIPELINE: &str = "investments";

/// Failures of reading or validating a structured table.
///
/// Every variant names the file and, where it makes sense, the row.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    #[error("missing formats-repository CSV file: {0}")]
    MissingCsv(PathBuf),
    #[error("{path}: malformed row at line {line}: {reason}")]
    MalformedRow { path: PathBuf, line: usize, reason: String },
    #[error("{path}: missing required column '{column}'")]
    MissingColumn { path: PathBuf, column: String },
    #[error("{path}, line {line}: ID '{id}' does not match the expected ID pattern")]
    InvalidId { path: PathBuf, line: usize, id: String },
    #[error("{path}, line {line}: column '{column}' is not a valid line selection: '{value}'")]
    InvalidLineSelection { path: PathBuf, line: usize, column: String, value: String },
    /// A row of a secondary table has no counterpart in the main table.
    #[error("{path}, line {line}: '{id}' has no matching row in the principal table")]
    UnmatchedRow { path: PathBuf, line: usize, id: String },
    /// Two rows of an at-most-one-per-pipe table configure the same pipe.
    #[error("{path}, line {line}: '{id}' is configured twice, but this table allows at most one row per pipe")]
    DuplicateRow { path: PathBuf, line: usize, id: String },
    /// A disabled segment carries configuration anyway.
    #[error("{path}, line {line}: segment '{segment}' is disabled for '{id}' but column '{column}' is not empty")]
    DisabledSegmentConfigured { path: PathBuf, line: usize, id: String, segment: &'static str, column: &'static str },
    /// Two rows configuring the same classification pipe declare different classes.
    #[error("{path}, line {line}: '{id}' is declared as class '{found}' but also as '{first}'")]
    ConflictingClass { path: PathBuf, line: usize, id: String, first: String, found: String },
}


// ----------------------------------------------------------------------------------------------
// Deserializers for the optional cells
// ----------------------------------------------------------------------------------------------
/// A text cell: empty means absent.
fn optional_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    Ok(if raw.trim().is_empty() { None } else { Some(raw) })
}

/// A numeric cell: empty means absent, anything else must be a number.
fn optional_number<'de, D: Deserializer<'de>, T: FromStr>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T::Err: std::fmt::Display,
{
    let raw = String::deserialize(deserializer)?;
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    raw.parse::<T>().map(Some).map_err(serde::de::Error::custom)
}

/// A boolean cell: empty means absent, and `TRUE`/`FALSE` in any mixture of cases — which is how
/// spreadsheets write booleans.
fn optional_bool<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => Ok(None),
        "true" => Ok(Some(true)),
        "false" => Ok(Some(false)),
        other => Err(serde::de::Error::custom(format!("expected TRUE, FALSE or an empty cell, found '{other}'"))),
    }
}

/// The four numbers of a value window, written `dx, dy, width, height` in one cell.
///
/// One cell and not four columns: the four belong together — they are one rectangle derived from
/// one label — and a format that states them states all four. Splitting them would turn the
/// statement of net assets into a table twenty columns wide for no gain in clarity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueWindow {
    pub dx: f32,
    pub dy: f32,
    pub width: f32,
    pub height: f32,
}

/// A window cell: empty means absent, anything else must be four numbers.
fn optional_window<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<ValueWindow>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
    let [dx, dy, width, height] = parts.as_slice() else {
        return Err(serde::de::Error::custom(format!(
            "expected four numbers 'dx, dy, width, height', found {} in '{raw}'",
            parts.len()
        )));
    };
    let number = |text: &str| {
        text.parse::<f32>().map_err(|e| serde::de::Error::custom(format!("'{text}' is not a number: {e}")))
    };
    Ok(Some(ValueWindow {
        dx: number(dx)?,
        dy: number(dy)?,
        width: number(width)?,
        height: number(height)?,
    }))
}

/// A cell naming one of a small vocabulary, such as a cast: empty means absent.
fn optional_named<'de, D: Deserializer<'de>, T: FromStr<Err = String>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    raw.parse::<T>().map(Some).map_err(serde::de::Error::custom)
}


// ----------------------------------------------------------------------------------------------
// The five tables, as they sit on disk
// ----------------------------------------------------------------------------------------------
/// A row of the investments arguments table: a pipe's main configuration.
///
/// The five numeric columns are **column positions** in the document's table, not values, and they
/// may be negative — `-1` meaning the last column — hence a signed integer.
#[derive(Debug, Clone, Deserialize)]
pub struct InvestmentsArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Subfund set", deserialize_with = "optional_text")]
    pub subfund_set: Option<String>,
    #[serde(rename = "Currency set", deserialize_with = "optional_text")]
    pub currency_set: Option<String>,
    #[serde(rename = "Body set", deserialize_with = "optional_text")]
    pub body_set: Option<String>,
    #[serde(rename = "Market value", deserialize_with = "optional_number")]
    pub market_value: Option<i16>,
    #[serde(rename = "Quantity", deserialize_with = "optional_number")]
    pub quantity: Option<i16>,
    #[serde(rename = "% net assets", deserialize_with = "optional_number")]
    pub perc_net_assets: Option<i16>,
    #[serde(rename = "Acquisition cost", deserialize_with = "optional_number")]
    pub acquisition_cost: Option<i16>,
    #[serde(rename = "Acquisition currency", deserialize_with = "optional_number")]
    pub acquisition_currency: Option<i16>,
}

/// A row of the additional arguments table.
#[derive(Debug, Clone, Deserialize)]
pub struct AdditionalArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Algorithm flags", deserialize_with = "optional_text")]
    pub algorithm_flags: Option<String>,
    #[serde(rename = "Column tolerance", deserialize_with = "optional_number")]
    pub col_tolerance: Option<f32>,
    /// The row pass's own tolerance. Independent of the column one: left empty it is zero, not the
    /// column's value.
    #[serde(rename = "Row tolerance", deserialize_with = "optional_number")]
    pub row_tolerance: Option<f32>,
    #[serde(rename = "Interpret quantity as float", deserialize_with = "optional_bool")]
    pub interpret_quantity_as_float: Option<bool>,
    #[serde(rename = "Interpret cost and value as int", deserialize_with = "optional_bool")]
    pub interpret_cost_and_value_as_int: Option<bool>,
    #[serde(rename = "Geometrical indexing", deserialize_with = "optional_bool")]
    pub geometrical_indexing: Option<bool>,
    #[serde(rename = "Merge previous", deserialize_with = "optional_bool")]
    pub merge_previous: Option<bool>,
    /// A flag expression naming the numeric fields that read the report's dash as zero — text
    /// rather than a bool, like `Algorithm flags` and for the same reason: the cell names a set.
    #[serde(rename = "Interpret dash as zero", deserialize_with = "optional_text")]
    pub interpret_dash_as_zero: Option<String>,
    /// The characters that count as a footnote mark on the fund's name (`R16`), written one after
    /// the other — text and not a bool, because the cell names a set of characters and the two
    /// formats that need it both use `*` while others use a superscript digit.
    #[serde(rename = "Note marks", deserialize_with = "optional_text")]
    pub note_marks: Option<String>,
    /// A regex whose matches are cut out of the fund's name before it becomes the sub-fund's
    /// identity — the `(continued)` a continuation page writes above the rest of a table.
    ///
    /// **One** pattern per cell, and that is not a limitation worth a second column: a regex
    /// already expresses several cuts as an alternation, `(?:\(continued\)|\(cont\.\))`, while a
    /// separator between patterns would have to be a character no regex may contain, and there
    /// isn't one.
    ///
    /// `default` and not just `deserialize_with`, unlike every column above: those were born with
    /// the table, this one arrived after formats repositories already existed, and a repository
    /// that does not need it must not have to grow an empty column to keep loading. Where the
    /// column is absent the value is `None`, which is exactly what an empty cell means.
    #[serde(rename = "Remove from fund", default, deserialize_with = "optional_text")]
    pub remove_from_fund: Option<String>,
}

/// A row of the deselection table.
#[derive(Debug, Clone, Deserialize)]
pub struct DeselectionRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Deselection set", deserialize_with = "optional_text")]
    pub deselection_set: Option<String>,
}

/// A row of the partial-pipes table: which segments of the pipeline are active.
///
/// An empty column means active.
#[derive(Debug, Clone, Deserialize)]
pub struct PartialPipesRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "pdf_extract", deserialize_with = "optional_bool")]
    pub pdf_extract: Option<bool>,
    #[serde(rename = "text_filter", deserialize_with = "optional_bool")]
    pub text_filter: Option<bool>,
    #[serde(rename = "deserialize", deserialize_with = "optional_bool")]
    pub deserialize: Option<bool>,
}

/// A row of the page-classify arguments table.
#[derive(Debug, Clone, Deserialize)]
pub struct PageClassifyArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Header set", deserialize_with = "optional_text")]
    pub header_set: Option<String>,
    #[serde(rename = "Class")]
    pub class: String,
}


fn row_error(path: &Path, line: usize, error: &csv::Error) -> TableError {
    let message = error.to_string();
    if let Some(rest) = message.split("missing field `").nth(1)
        && let Some(column) = rest.split('`').next()
    {
        return TableError::MissingColumn { path: path.to_path_buf(), column: column.to_string() };
    }
    TableError::MalformedRow { path: path.to_path_buf(), line, reason: message }
}

// ----------------------------------------------------------------------------------------------
// Reading
// ----------------------------------------------------------------------------------------------
/// Reads a structured table into typed rows, together with its own path, which appears in every
/// error message.
fn read_table<T: serde::de::DeserializeOwned>(
    formats_repo_dir: &Path,
    relative_path: &str,
) -> Result<(PathBuf, Vec<T>), TableError> {
    let path = formats_repo_dir.join(STRUCTURED_DIR).join(relative_path);
    if !path.is_file() {
        return Err(TableError::MissingCsv(path));
    }
    let mut reader = csv::Reader::from_path(&path)
        .map_err(|e| TableError::MalformedRow { path: path.clone(), line: 0, reason: e.to_string() })?;
    let mut rows = Vec::new();
    for (i, record) in reader.deserialize::<T>().enumerate() {
        rows.push(record.map_err(|e| row_error(&path, i + 1, &e))?);
    }
    Ok((path, rows))
}

/// Checks the form of every id and derives the complete identity from it.
fn identify(
    path: &Path,
    ids: &[&str],
    pipeline_default: &str,
    relation: FkRelation,
) -> Result<Vec<ComputedId>, TableError> {
    for (i, id) in ids.iter().enumerate() {
        if !id_matches(id, relation.id_format()) {
            return Err(TableError::InvalidId { path: path.to_path_buf(), line: i + 1, id: (*id).to_string() });
        }
    }
    Ok(computed_ids(ids, Some(pipeline_default), relation))
}

/// Checks that a cell, when non-empty, is a well-formed line selection.
fn check_line_selection(
    path: &Path,
    line: usize,
    column: &str,
    value: Option<&String>,
) -> Result<(), TableError> {
    match value {
        Some(text) if !is_pdfline_selection(text) => Err(TableError::InvalidLineSelection {
            path: path.to_path_buf(),
            line,
            column: column.to_string(),
            value: text.clone(),
        }),
        _ => Ok(()),
    }
}

/// Indexes an at-most-one-row-per-pipe table, rejecting duplicates and rows matching nothing in the
/// main table.
fn index_unique<T>(
    path: &Path,
    ids: Vec<ComputedId>,
    rows: Vec<T>,
    known: &HashSet<&ComputedId>,
) -> Result<HashMap<ComputedId, T>, TableError> {
    let mut map = HashMap::new();
    for (i, (id, row)) in ids.into_iter().zip(rows).enumerate() {
        let line = i + 1;
        if !known.contains(&id) {
            return Err(TableError::UnmatchedRow { path: path.to_path_buf(), line, id: id.to_string() });
        }
        if map.insert(id.clone(), row).is_some() {
            return Err(TableError::DuplicateRow { path: path.to_path_buf(), line, id: id.to_string() });
        }
    }
    Ok(map)
}


// ----------------------------------------------------------------------------------------------
// The joined configurations, one per pipe
// ----------------------------------------------------------------------------------------------
/// Everything the repository declares about one investments pipe, with the four tables already
/// joined.
#[derive(Debug, Clone)]
pub struct InvestmentsConfig {
    pub id: ComputedId,
    pub args: InvestmentsArgsRow,
    pub additional: Option<AdditionalArgsRow>,
    /// The selections to subtract from the body set, in file order.
    pub deselection_sets: Vec<String>,
    pub partial_pipes: Option<PartialPipesRow>,
}

impl InvestmentsConfig {
    /// Whether the extraction segment is to be built. An absent configuration means yes.
    pub fn wants_pdf_extract(&self) -> bool {
        self.partial_pipes.as_ref().and_then(|p| p.pdf_extract).unwrap_or(true)
    }

    /// Whether the filtering segment is to be built. See [`Self::wants_pdf_extract`].
    pub fn wants_text_filter(&self) -> bool {
        self.partial_pipes.as_ref().and_then(|p| p.text_filter).unwrap_or(true)
    }

    /// Whether the deserialization segment is to be built. See [`Self::wants_pdf_extract`].
    pub fn wants_deserialize(&self) -> bool {
        self.partial_pipes.as_ref().and_then(|p| p.deserialize).unwrap_or(true)
    }
}

/// Everything the repository declares about one page-classification pipe.
#[derive(Debug, Clone)]
pub struct PageClassifyConfig {
    pub id: ComputedId,
    /// The page class the pipe assigns when **every** header matches.
    pub class: String,
    /// The headers to look for, one per CSV row, in file order.
    pub header_sets: Vec<String>,
}

/// The columns a disabled segment may not carry, for each of the three segments.
///
/// Declaring a segment off and then configuring it is a contradiction, not something loading should
/// have to guess its way through.
const DISABLED_SEGMENT_COLUMNS: [(&str, &[&str]); 3] = [
    (
        "pdf_extract",
        &["Subfund set", "Currency set", "Body set", "Deselection set", "Algorithm flags", "Column tolerance", "Row tolerance"],
    ),
    (
        "text_filter",
        &[
            "Market value",
            "Quantity",
            "% net assets",
            "Acquisition cost",
            "Acquisition currency",
            "Geometrical indexing",
            "Merge previous",
            "Interpret dash as zero",
            "Note marks",
            "Remove from fund",
        ],
    ),
    ("deserialize", &["Interpret quantity as float", "Interpret cost and value as int"]),
];

impl InvestmentsConfig {
    /// Whether each column named by [`DISABLED_SEGMENT_COLUMNS`] is configured.
    fn column_is_set(&self, column: &str) -> bool {
        let additional = self.additional.as_ref();
        match column {
            "Subfund set" => self.args.subfund_set.is_some(),
            "Currency set" => self.args.currency_set.is_some(),
            "Body set" => self.args.body_set.is_some(),
            "Deselection set" => !self.deselection_sets.is_empty(),
            "Algorithm flags" => additional.is_some_and(|a| a.algorithm_flags.is_some()),
            "Column tolerance" => additional.is_some_and(|a| a.col_tolerance.is_some()),
            "Row tolerance" => additional.is_some_and(|a| a.row_tolerance.is_some()),
            "Market value" => self.args.market_value.is_some(),
            "Quantity" => self.args.quantity.is_some(),
            "% net assets" => self.args.perc_net_assets.is_some(),
            "Acquisition cost" => self.args.acquisition_cost.is_some(),
            "Acquisition currency" => self.args.acquisition_currency.is_some(),
            "Geometrical indexing" => additional.is_some_and(|a| a.geometrical_indexing.is_some()),
            "Merge previous" => additional.is_some_and(|a| a.merge_previous.is_some()),
            "Interpret quantity as float" => additional.is_some_and(|a| a.interpret_quantity_as_float.is_some()),
            "Interpret cost and value as int" => additional.is_some_and(|a| a.interpret_cost_and_value_as_int.is_some()),
            "Interpret dash as zero" => additional.is_some_and(|a| a.interpret_dash_as_zero.is_some()),
            "Note marks" => additional.is_some_and(|a| a.note_marks.is_some()),
            "Remove from fund" => additional.is_some_and(|a| a.remove_from_fund.is_some()),
            other => unreachable!("DISABLED_SEGMENT_COLUMNS names an unknown column {other:?}"),
        }
    }

    /// Checks that no disabled segment carries configuration.
    fn check_disabled_segments(&self, path: &Path, line: usize) -> Result<(), TableError> {
        let enabled = [self.wants_pdf_extract(), self.wants_text_filter(), self.wants_deserialize()];
        for ((segment, columns), enabled) in DISABLED_SEGMENT_COLUMNS.iter().zip(enabled) {
            if enabled {
                continue;
            }
            for column in *columns {
                if self.column_is_set(column) {
                    return Err(TableError::DisabledSegmentConfigured {
                        path: path.to_path_buf(),
                        line,
                        id: self.id.to_string(),
                        segment,
                        column,
                    });
                }
            }
        }
        Ok(())
    }
}

/// The investments pipe configurations the repository declares, in the order of the arguments
/// table.
///
/// Reads the four tables, validates every row, and joins them on the `(format, pipeline, index)`
/// key.
pub fn get_investments_configs(formats_repo_dir: &Path) -> Result<Vec<InvestmentsConfig>, TableError> {
    let (args_path, args): (_, Vec<InvestmentsArgsRow>) = read_table(formats_repo_dir, "investments/args.csv")?;
    let args_ids = identify(
        &args_path,
        &args.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        INVESTMENTS_PIPELINE,
        FkRelation::OneToOne,
    )?;
    for (i, row) in args.iter().enumerate() {
        let line = i + 1;
        check_line_selection(&args_path, line, "Subfund set", row.subfund_set.as_ref())?;
        check_line_selection(&args_path, line, "Currency set", row.currency_set.as_ref())?;
        check_line_selection(&args_path, line, "Body set", row.body_set.as_ref())?;
    }
    let known: HashSet<&ComputedId> = args_ids.iter().collect();

    let (add_path, add_rows): (_, Vec<AdditionalArgsRow>) =
        read_table(formats_repo_dir, "investments/additional_args.csv")?;
    let add_ids = identify(
        &add_path,
        &add_rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        INVESTMENTS_PIPELINE,
        FkRelation::OneToMaybe,
    )?;
    let mut additional = index_unique(&add_path, add_ids, add_rows, &known)?;

    let (partial_path, partial_rows): (_, Vec<PartialPipesRow>) =
        read_table(formats_repo_dir, "investments/partial_pipes.csv")?;
    let partial_ids = identify(
        &partial_path,
        &partial_rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        INVESTMENTS_PIPELINE,
        FkRelation::OneToMaybe,
    )?;
    let mut partial = index_unique(&partial_path, partial_ids, partial_rows, &known)?;

    let (desel_path, desel_rows): (_, Vec<DeselectionRow>) =
        read_table(formats_repo_dir, "investments/deselection_lists.csv")?;
    let desel_ids = identify(
        &desel_path,
        &desel_rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        INVESTMENTS_PIPELINE,
        FkRelation::OneToMany,
    )?;
    let mut deselections: HashMap<ComputedId, Vec<String>> = HashMap::new();
    for (i, (id, row)) in desel_ids.into_iter().zip(desel_rows).enumerate() {
        let line = i + 1;
        if !known.contains(&id) {
            return Err(TableError::UnmatchedRow { path: desel_path.clone(), line, id: id.to_string() });
        }
        check_line_selection(&desel_path, line, "Deselection set", row.deselection_set.as_ref())?;
        if let Some(set) = row.deselection_set {
            deselections.entry(id).or_default().push(set);
        }
    }

    let mut configs = Vec::with_capacity(args.len());
    for (i, (id, row)) in args_ids.into_iter().zip(args).enumerate() {
        let config = InvestmentsConfig {
            additional: additional.remove(&id),
            deselection_sets: deselections.remove(&id).unwrap_or_default(),
            partial_pipes: partial.remove(&id),
            args: row,
            id,
        };
        config.check_disabled_segments(&args_path, i + 1)?;
        configs.push(config);
    }
    tracing::debug!(config_count = configs.len(), "read investments configuration rows");
    Ok(configs)
}

/// The page-classification pipe configurations, in order of first appearance.
///
/// Several rows sharing an id describe the **same** pipe with several headers to look for, and must
/// therefore declare the same class; a group that disagrees is an error.
pub fn get_page_classify_configs(formats_repo_dir: &Path) -> Result<Vec<PageClassifyConfig>, TableError> {
    let (path, rows): (_, Vec<PageClassifyArgsRow>) = read_table(formats_repo_dir, "page_classify/args.csv")?;
    // The default pipeline here is the **unnamed** one, not `investments`: a page classifier
    // belongs to the format's default pipeline.
    let ids = identify(&path, &rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), "", FkRelation::OneToMany)?;

    let mut order: Vec<ComputedId> = Vec::new();
    let mut classes: HashMap<ComputedId, String> = HashMap::new();
    let mut headers: HashMap<ComputedId, Vec<String>> = HashMap::new();
    for (i, (id, row)) in ids.into_iter().zip(rows).enumerate() {
        let line = i + 1;
        check_line_selection(&path, line, "Header set", row.header_set.as_ref())?;
        match classes.get(&id) {
            None => {
                order.push(id.clone());
                classes.insert(id.clone(), row.class.clone());
            }
            Some(first) if *first != row.class => {
                return Err(TableError::ConflictingClass {
                    path: path.clone(),
                    line,
                    id: id.to_string(),
                    first: first.clone(),
                    found: row.class.clone(),
                });
            }
            Some(_) => {}
        }
        if let Some(set) = row.header_set {
            headers.entry(id).or_default().push(set);
        }
    }

    let configs: Vec<PageClassifyConfig> = order
        .into_iter()
        .map(|id| PageClassifyConfig {
            class: classes.remove(&id).expect("ogni id in `order` è stato inserito in `classes`"),
            header_sets: headers.remove(&id).unwrap_or_default(),
            id,
        })
        .collect();
    tracing::debug!(config_count = configs.len(), "read page classify configuration rows");
    Ok(configs)
}

/// A row of the fund-assets arguments table: a whole statement of net assets.
///
/// Four cells are required — the fund column and the three amounts — and every other one has the
/// value the engine itself defaults to, so a statement laid out the common way is four anchors.
/// See [`super::fund_assets`] for what each one does.
#[derive(Debug, Clone, Deserialize)]
pub struct FundAssetsArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    /// The column of sub-fund names, which is what the three amounts are paired against.
    #[serde(rename = "Fund set")]
    pub fund_set: String,
    /// The line declaring the currency. Absent means the page does not state one.
    #[serde(rename = "Currency set", default, deserialize_with = "optional_text")]
    pub currency_set: Option<String>,
    /// The line carrying the statement's date. Absent means the date comes from elsewhere.
    #[serde(rename = "Date set", default, deserialize_with = "optional_text")]
    pub date_set: Option<String>,
    #[serde(rename = "Total assets")]
    pub tot_assets: String,
    #[serde(rename = "Total liabilities")]
    pub liabilities: String,
    #[serde(rename = "Net assets")]
    pub net_assets: String,
    /// The window of all three amounts, `dx, dy, width, height`.
    #[serde(rename = "Value window", default, deserialize_with = "optional_window")]
    pub value_window: Option<ValueWindow>,
    #[serde(rename = "Total assets window", default, deserialize_with = "optional_window")]
    pub tot_assets_window: Option<ValueWindow>,
    #[serde(rename = "Total liabilities window", default, deserialize_with = "optional_window")]
    pub liabilities_window: Option<ValueWindow>,
    #[serde(rename = "Net assets window", default, deserialize_with = "optional_window")]
    pub net_assets_window: Option<ValueWindow>,
    /// The heading of the **number** columns to leave out of the statement.
    #[serde(rename = "Drop set", default, deserialize_with = "optional_text")]
    pub drop_set: Option<String>,
    /// A **fund** column that is not a fund, named by one of its own name lines: it goes whole,
    /// name and numbers. See `AssetsStandardArgs::drop_funds`.
    #[serde(rename = "Drop fund set", default, deserialize_with = "optional_text")]
    pub drop_fund_set: Option<String>,
    /// Whether the fund columns are grouped by the tabularizer rather than taken one by one.
    #[serde(rename = "Table condition", default, deserialize_with = "optional_bool")]
    pub table_condition: Option<bool>,
    /// Whether an amount is matched to its fund by abscissa rather than by position.
    #[serde(rename = "Pair by abscissa", default, deserialize_with = "optional_bool")]
    pub pair_by_abscissa: Option<bool>,
    /// One value in every N a column's window finds. Absent means every one of them.
    ///
    /// A statement that prints the previous financial year beside the current one puts two numbers
    /// in every window, and only the first of each pair is this year's: MEDIOLANUM-IT24.A is that
    /// shape, and its step is two.
    #[serde(rename = "Value step", default, deserialize_with = "optional_number")]
    pub value_step: Option<i64>,
    /// The expression pulling the date out of the line, with exactly one capturing group.
    #[serde(rename = "Date regex", default, deserialize_with = "optional_text")]
    pub date_regex: Option<String>,
    /// The characters to strip from the end of a fund's name (`R16`).
    #[serde(rename = "Note marks", default, deserialize_with = "optional_text")]
    pub note_marks: Option<String>,
    /// One expression to cut out of the fund's name before it becomes its identity. Several cuts go
    /// in one expression, as an alternation, exactly as in the investments table.
    #[serde(rename = "Remove from fund", default, deserialize_with = "optional_text")]
    pub remove_from_fund: Option<String>,
    /// How an amount is read. See [`super::fund_assets::AmountCast`].
    #[serde(rename = "Amount cast", default, deserialize_with = "optional_named")]
    pub amount_cast: Option<super::fund_assets::AmountCast>,
    /// How the date is read. See [`super::fund_assets::DateCast`].
    #[serde(rename = "Date cast", default, deserialize_with = "optional_named")]
    pub date_cast: Option<super::fund_assets::DateCast>,
}

/// One fund-assets pipe, with its identity derived.
#[derive(Debug, Clone)]
pub struct FundAssetsConfig {
    pub id: ComputedId,
    pub row: FundAssetsArgsRow,
}

/// The fund-assets pipe configurations, in the order they appear.
///
/// **One row per pipe**, unlike the page-classification table: a statement of net assets is one
/// thing, and a format that reads two different statements declares two pipelines rather than two
/// rows of one.
pub fn get_fund_assets_configs(formats_repo_dir: &Path) -> Result<Vec<FundAssetsConfig>, TableError> {
    // **An absent file means no rows**, unlike the five tables that came before. Those are part of
    // what a formats repository is, and a missing one is a defect in it; this table arrived after
    // repositories already existed, and a repository whose statements of net assets are all written
    // in Python must not have to grow an empty file to keep loading. It is the same reasoning as the
    // `Remove from fund` column's `serde(default)`, applied to a whole table.
    let path = formats_repo_dir.join(STRUCTURED_DIR).join("fund_assets/args.csv");
    if !path.is_file() {
        tracing::debug!(?path, "no fund assets table in this repository: no structured statement");
        return Ok(Vec::new());
    }
    let (path, rows): (_, Vec<FundAssetsArgsRow>) = read_table(formats_repo_dir, "fund_assets/args.csv")?;
    let ids = identify(
        &path,
        &rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        super::fund_assets::FUND_ASSETS_PIPELINE,
        FkRelation::OneToOne,
    )?;

    // The set is keyed on `(format, pipeline)` and **not** on the whole `ComputedId`, which carries
    // a pipe index: two rows with the same id would otherwise be two pipes of one pipeline, which is
    // what the investments table means by it and is meaningless here — a second extraction pipe
    // would read the same statement twice. Refused rather than silently resolved either way.
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut configs = Vec::with_capacity(rows.len());
    for (i, (id, row)) in ids.into_iter().zip(rows).enumerate() {
        let line = i + 1;
        check_line_selection(&path, line, "Fund set", Some(&row.fund_set))?;
        check_line_selection(&path, line, "Currency set", row.currency_set.as_ref())?;
        check_line_selection(&path, line, "Date set", row.date_set.as_ref())?;
        check_line_selection(&path, line, "Total assets", Some(&row.tot_assets))?;
        check_line_selection(&path, line, "Total liabilities", Some(&row.liabilities))?;
        check_line_selection(&path, line, "Net assets", Some(&row.net_assets))?;
        check_line_selection(&path, line, "Drop set", row.drop_set.as_ref())?;
        check_line_selection(&path, line, "Drop fund set", row.drop_fund_set.as_ref())?;
        if !seen.insert((id.format.clone(), id.pipeline.clone())) {
            return Err(TableError::DuplicateRow { path: path.clone(), line, id: id.to_string() });
        }
        configs.push(FundAssetsConfig { id, row });
    }
    tracing::debug!(config_count = configs.len(), "read fund assets configuration rows");
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const ARGS_HEADER: &str =
        "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n";
    const ADD_HEADER: &str = "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous,Interpret dash as zero,Note marks\n";
    const PARTIAL_HEADER: &str = "ID,pdf_extract,text_filter,deserialize\n";
    const DESEL_HEADER: &str = "ID,Deselection set\n";
    const PAGE_CLASSIFY_HEADER: &str = "ID,Header set,Class\n";

    /// A formats repository with the five structured CSV files. Each table starts as a header row
    /// alone, and the tests add the rows they need.
    struct Repo {
        dir: TempDir,
    }

    impl Repo {
        fn new() -> Self {
            let dir = TempDir::new().expect("temp dir");
            let base = dir.path().join(STRUCTURED_DIR);
            fs::create_dir_all(base.join("investments")).expect("investments dir");
            fs::create_dir_all(base.join("page_classify")).expect("page_classify dir");
            let repo = Self { dir };
            repo.write("investments/args.csv", ARGS_HEADER);
            repo.write("investments/additional_args.csv", ADD_HEADER);
            repo.write("investments/partial_pipes.csv", PARTIAL_HEADER);
            repo.write("investments/deselection_lists.csv", DESEL_HEADER);
            repo.write("page_classify/args.csv", PAGE_CLASSIFY_HEADER);
            repo
        }

        fn write(&self, relative: &str, content: &str) -> &Self {
            fs::write(self.dir.path().join(STRUCTURED_DIR).join(relative), content).expect("write csv");
            self
        }

        /// Writes the fund-assets table, creating its directory. Not written by `new`, because an
        /// absent table is legitimate and several tests are about exactly that.
        fn write_fund_assets(&self, content: &str) -> &Self {
            let base = self.dir.path().join(STRUCTURED_DIR).join("fund_assets");
            fs::create_dir_all(&base).expect("fund_assets dir");
            fs::write(base.join("args.csv"), content).expect("write csv");
            self
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }
    }


    /// The header of the fund-assets table.
    const ASSETS_HEADER: &str = "ID,Fund set,Currency set,Date set,Total assets,Total liabilities,Net assets,Value window,Total assets window,Total liabilities window,Net assets window,Drop set,Drop fund set,Table condition,Pair by abscissa,Value step,Date regex,Note marks,Remove from fund,Amount cast,Date cast\n";

    mod fund_assets_args {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The columns of the table, in order, so a test can name the one it is about.
        #[derive(Default)]
        struct Row<'a> {
            id: &'a str,
            fund_set: &'a str,
            currency_set: &'a str,
            date_set: &'a str,
            tot_assets: &'a str,
            liabilities: &'a str,
            net_assets: &'a str,
            value_window: &'a str,
            tot_assets_window: &'a str,
            liabilities_window: &'a str,
            net_assets_window: &'a str,
            drop_set: &'a str,
            drop_fund_set: &'a str,
            table_condition: &'a str,
            pair_by_abscissa: &'a str,
            value_step: &'a str,
            date_regex: &'a str,
            note_marks: &'a str,
            remove_from_fund: &'a str,
            amount_cast: &'a str,
            date_cast: &'a str,
        }

        impl Row<'_> {
            /// The minimal row that loads: four selections, and a currency because a statement that
            /// is not tabular and states no currency is refused by the pipe itself.
            fn minimal() -> Self {
                Self {
                    id: "A-EN24",
                    fund_set: "ArialMT",
                    currency_set: "Arial-Bold",
                    tot_assets: "Arial-Bold",
                    liabilities: "Arial-Bold",
                    net_assets: "Arial-Bold",
                    ..Default::default()
                }
            }

            /// The row as a CSV line, every cell quoted so a comma inside one is not a separator.
            fn line(&self) -> String {
                let cells = [
                    self.id,
                    self.fund_set,
                    self.currency_set,
                    self.date_set,
                    self.tot_assets,
                    self.liabilities,
                    self.net_assets,
                    self.value_window,
                    self.tot_assets_window,
                    self.liabilities_window,
                    self.net_assets_window,
                    self.drop_set,
                    self.drop_fund_set,
                    self.table_condition,
                    self.pair_by_abscissa,
                    self.value_step,
                    self.date_regex,
                    self.note_marks,
                    self.remove_from_fund,
                    self.amount_cast,
                    self.date_cast,
                ];
                let quoted: Vec<String> =
                    cells.iter().map(|cell| format!("\"{}\"", cell.replace('"', "\"\""))).collect();
                format!("{}\n", quoted.join(","))
            }
        }

        /// A repository whose fund-assets table holds these rows.
        fn repo_with(rows: &[Row<'_>]) -> Repo {
            let repo = Repo::new();
            let body: String = rows.iter().map(Row::line).collect();
            repo.write_fund_assets(&format!("{ASSETS_HEADER}{body}"));
            repo
        }

        #[test]
        fn a_repository_without_the_table_declares_no_statement() {
            // The table arrived after repositories existed: one whose statements are all written in
            // Python must not have to grow an empty file to keep loading.
            assert!(get_fund_assets_configs(Repo::new().path()).unwrap().is_empty());
        }

        #[test]
        fn four_cells_are_enough_for_a_row() {
            let configs = get_fund_assets_configs(repo_with(&[Row::minimal()]).path()).unwrap();
            assert_eq!(configs.len(), 1);
            let row = &configs[0].row;
            assert_eq!(row.fund_set, "ArialMT");
            assert!(row.value_window.is_none());
            assert!(row.amount_cast.is_none());
            assert!(row.date_cast.is_none());
            assert!(row.table_condition.is_none());
            assert!(row.date_set.is_none());
        }

        #[test]
        fn the_pipeline_defaults_to_fund_assets() {
            let configs = get_fund_assets_configs(repo_with(&[Row::minimal()]).path()).unwrap();
            assert_eq!(configs[0].id.pipeline, "fund_assets");
        }

        #[test]
        fn an_explicit_pipeline_in_the_id_is_kept() {
            let row = Row { id: "A-EN24(fund_assets_begin)", ..Row::minimal() };
            let configs = get_fund_assets_configs(repo_with(&[row]).path()).unwrap();
            assert_eq!(configs[0].id.pipeline, "fund_assets_begin");
        }

        #[test]
        fn two_rows_of_two_formats_are_two_configs() {
            let other = Row { id: "B-EN24", ..Row::minimal() };
            let configs = get_fund_assets_configs(repo_with(&[Row::minimal(), other]).path()).unwrap();
            assert_eq!(configs.len(), 2);
        }

        #[test]
        fn one_row_per_pipe_and_not_two() {
            // A statement of net assets is one thing: a format reading two of them declares two
            // pipelines, not two rows of one.
            let err = get_fund_assets_configs(repo_with(&[Row::minimal(), Row::minimal()]).path()).unwrap_err();
            assert!(matches!(err, TableError::DuplicateRow { .. }), "{err}");
        }

        #[test]
        fn a_window_cell_is_four_numbers() {
            let row = Row { value_window: "1.2, -0.1, 100, 1.2", ..Row::minimal() };
            let window = get_fund_assets_configs(repo_with(&[row]).path()).unwrap()[0]
                .row
                .value_window
                .expect("the window cell");
            assert_eq!((window.dx, window.dy, window.width, window.height), (1.2, -0.1, 100.0, 1.2));
        }

        #[test]
        fn a_window_cell_of_three_numbers_says_how_many_it_wanted() {
            let row = Row { value_window: "1.2, -0.1, 100", ..Row::minimal() };
            let err = get_fund_assets_configs(repo_with(&[row]).path()).unwrap_err();
            assert!(err.to_string().contains("four numbers"), "{err}");
        }

        #[test]
        fn a_window_cell_that_is_not_numbers_says_which_one() {
            let row = Row { net_assets_window: "1.2, -0.1, wide, 1.2", ..Row::minimal() };
            let err = get_fund_assets_configs(repo_with(&[row]).path()).unwrap_err();
            assert!(err.to_string().contains("wide"), "{err}");
        }

        #[test]
        fn the_three_per_column_windows_are_read_separately() {
            let row = Row {
                tot_assets_window: "1.2, -3.5, 100, 3",
                liabilities_window: "1.2, -3.5, 100, 3",
                net_assets_window: "1.2, 0, 100, 2.2",
                ..Row::minimal()
            };
            let read = &get_fund_assets_configs(repo_with(&[row]).path()).unwrap()[0].row;
            assert_eq!(read.tot_assets_window.unwrap().dy, -3.5);
            assert_eq!(read.net_assets_window.unwrap().height, 2.2);
            assert!(read.value_window.is_none());
        }

        #[test]
        fn a_cast_is_named_from_a_vocabulary_and_a_typo_lists_it() {
            let row = Row { amount_cast: "flaot", ..Row::minimal() };
            let err = get_fund_assets_configs(repo_with(&[row]).path()).unwrap_err();
            let message = err.to_string();
            for name in ["int", "float", "dash_as_zero", "magnitude"] {
                assert!(message.contains(name), "{message}");
            }
        }

        #[test]
        fn each_amount_cast_name_is_read() {
            for name in [
                "int",
                "float",
                "int + dash_as_zero",
                "float + magnitude",
                "int + dash_as_zero + magnitude",
            ] {
                let row = Row { amount_cast: name, ..Row::minimal() };
                assert!(get_fund_assets_configs(repo_with(&[row]).path()).is_ok(), "{name}");
            }
        }

        #[test]
        fn each_date_cast_name_is_read() {
            for name in ["numeric", "en_month", "it_month", "en_month_first"] {
                let row = Row { date_cast: name, ..Row::minimal() };
                assert!(get_fund_assets_configs(repo_with(&[row]).path()).is_ok(), "{name}");
            }
        }

        #[test]
        fn the_value_step_is_read_and_absent_means_every_value() {
            let row = Row { value_step: "2", ..Row::minimal() };
            let configs = get_fund_assets_configs(repo_with(&[row]).path()).unwrap();
            assert_eq!(configs[0].row.value_step, Some(2));

            let configs = get_fund_assets_configs(repo_with(&[Row::minimal()]).path()).unwrap();
            assert_eq!(configs[0].row.value_step, None);
        }

        #[test]
        fn a_value_step_that_is_not_a_number_is_refused() {
            let row = Row { value_step: "every other", ..Row::minimal() };
            assert!(get_fund_assets_configs(repo_with(&[row]).path()).is_err());
        }

        #[test]
        fn every_selection_cell_is_validated_and_the_error_names_the_column() {
            let broken = "???";
            for (column, row) in [
                ("Fund set", Row { fund_set: broken, ..Row::minimal() }),
                ("Currency set", Row { currency_set: broken, ..Row::minimal() }),
                ("Date set", Row { date_set: broken, ..Row::minimal() }),
                ("Total assets", Row { tot_assets: broken, ..Row::minimal() }),
                ("Total liabilities", Row { liabilities: broken, ..Row::minimal() }),
                ("Net assets", Row { net_assets: broken, ..Row::minimal() }),
                ("Drop set", Row { drop_set: broken, ..Row::minimal() }),
                ("Drop fund set", Row { drop_fund_set: broken, ..Row::minimal() }),
            ] {
                let err = get_fund_assets_configs(repo_with(&[row]).path()).unwrap_err();
                assert!(err.to_string().contains(column), "{column}: {err}");
            }
        }

        #[test]
        fn a_fund_set_bounded_by_an_anchor_passes_validation() {
            // The form the statements actually use, and the reason the compact grammar grew text
            // anchors: it has to survive the table's own validation too.
            let row = Row {
                fund_set: r#"((:1000000)(:<frutiger-black "STATEMENT OF NET ASSETS AS AT">))"#,
                ..Row::minimal()
            };
            assert_eq!(get_fund_assets_configs(repo_with(&[row]).path()).unwrap().len(), 1);
        }

        #[test]
        fn the_boolean_cells_accept_the_spreadsheet_spelling() {
            let row = Row { table_condition: "TRUE", pair_by_abscissa: "FALSE", ..Row::minimal() };
            let read = &get_fund_assets_configs(repo_with(&[row]).path()).unwrap()[0].row;
            assert_eq!(read.table_condition, Some(true));
            assert_eq!(read.pair_by_abscissa, Some(false));
        }
    }

    mod investments_args {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn reads_one_config_per_row_in_file_order() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nB-EN24,,,,2,,,,\n"),
            );
            let configs = get_investments_configs(repo.path()).unwrap();
            assert_eq!(
                configs.iter().map(|c| c.id.to_string()).collect::<Vec<_>>(),
                vec!["A-EN24(investments)/0".to_string(), "B-EN24(investments)/0".to_string()]
            );
        }

        #[test]
        fn the_default_pipeline_of_this_table_is_investments() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            assert_eq!(get_investments_configs(repo.path()).unwrap()[0].id.pipeline, "investments");
        }

        #[test]
        fn two_rows_of_the_same_format_are_two_pipes_of_the_same_pipeline() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nA-EN24,,,,2,,,,\n"));
            let configs = get_investments_configs(repo.path()).unwrap();
            assert_eq!(configs[0].id.index, 0);
            assert_eq!(configs[1].id.index, 1);
        }

        #[test]
        fn an_empty_numeric_cell_becomes_none() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert_eq!(config.args.market_value, Some(1));
            assert_eq!(config.args.quantity, None);
        }

        #[test]
        fn a_negative_column_position_is_accepted() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,-1,2,,\n"));
            assert_eq!(get_investments_configs(repo.path()).unwrap()[0].args.quantity, Some(-1));
        }

        #[test]
        fn a_non_numeric_cell_reports_its_line() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nB-EN24,,,,x,,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(matches!(err, TableError::MalformedRow { line: 2, .. }), "{err}");
        }

        #[test]
        fn an_id_carrying_an_index_is_rejected_by_this_table() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24(investments)/0,,,,1,,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(matches!(err, TableError::InvalidId { line: 1, .. }), "{err}");
        }

        #[test]
        fn a_malformed_line_selection_names_the_column_and_the_line() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,ArialMT ???,1,,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            let TableError::InvalidLineSelection { line, column, .. } = err else {
                panic!("expected InvalidLineSelection, got {err}")
            };
            assert_eq!((line, column.as_str()), (1, "Body set"));
        }

        #[test]
        fn a_well_formed_line_selection_passes() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,ArialMT(:27),ArialNarrow(:208),ArialNarrow(:768),1,-1,2,,\n"),
            );
            assert!(get_investments_configs(repo.path()).is_ok());
        }

        #[test]
        fn a_missing_column_names_it() {
            let repo = Repo::new();
            repo.write("investments/args.csv", "ID,Subfund set\nA-EN24,\n");
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(matches!(err, TableError::MissingColumn { .. }), "{err}");
        }

        #[test]
        fn a_missing_file_is_reported_with_its_path() {
            let repo = Repo::new();
            fs::remove_file(repo.path().join(STRUCTURED_DIR).join("investments/args.csv")).unwrap();
            assert!(matches!(get_investments_configs(repo.path()), Err(TableError::MissingCsv(_))));
        }

        #[test]
        fn an_empty_table_declares_no_pipe() {
            assert!(get_investments_configs(Repo::new().path()).unwrap().is_empty());
        }
    }

    mod secondary_tables {
        use super::*;
        use pretty_assertions::assert_eq;

        fn with_one_pipe() -> Repo {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo
        }

        #[test]
        fn additional_args_join_onto_the_matching_pipe() {
            let repo = with_one_pipe();
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,,,,TRUE,FALSE,,TRUE,,\n"));
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            let additional = config.additional.as_ref().unwrap();
            assert_eq!(additional.interpret_quantity_as_float, Some(true));
            assert_eq!(additional.interpret_cost_and_value_as_int, Some(false));
            assert_eq!(additional.merge_previous, Some(true));
            assert_eq!(additional.geometrical_indexing, None);
        }

        #[test]
        fn a_pipe_without_additional_args_simply_has_none() {
            let repo = with_one_pipe();
            assert!(get_investments_configs(repo.path()).unwrap()[0].additional.is_none());
        }

        #[test]
        fn a_full_id_in_a_secondary_table_matches_the_derived_id_of_the_principal_one() {
            let repo = with_one_pipe();
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}A-EN24(investments)/0,,,,TRUE,,,,,\n"),
            );
            assert!(get_investments_configs(repo.path()).unwrap()[0].additional.is_some());
        }

        #[test]
        fn a_secondary_row_matching_no_pipe_is_rejected_with_its_line() {
            let repo = with_one_pipe();
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}GHOST-EN24,,,,TRUE,,,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            let TableError::UnmatchedRow { line, id, .. } = err else { panic!("expected UnmatchedRow, got {err}") };
            assert_eq!((line, id.as_str()), (1, "GHOST-EN24(investments)/0"));
        }

        #[test]
        fn two_additional_args_rows_for_one_pipe_are_rejected() {
            let repo = with_one_pipe();
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}A-EN24(investments)/0,,,,TRUE,,,,,\nA-EN24(investments)/0,,,,FALSE,,,,,\n"),
            );
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(matches!(err, TableError::DuplicateRow { line: 2, .. }), "{err}");
        }

        #[test]
        fn deselection_rows_accumulate_in_file_order() {
            let repo = with_one_pipe();
            repo.write(
                "investments/deselection_lists.csv",
                &format!("{DESEL_HEADER}A-EN24,\"Arial \"\"FIRST\"\"\"\nA-EN24,\"Arial \"\"SECOND\"\"\"\n"),
            );
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert_eq!(config.deselection_sets, vec!["Arial \"FIRST\"".to_string(), "Arial \"SECOND\"".to_string()]);
        }

        #[test]
        fn several_deselection_rows_for_one_pipe_are_not_a_duplicate() {
            // The relation is one-to-many: several rows per pipe are the norm, not an error.
            let repo = with_one_pipe();
            repo.write("investments/deselection_lists.csv", &format!("{DESEL_HEADER}A-EN24,\nA-EN24,\n"));
            assert!(get_investments_configs(repo.path()).is_ok());
        }

        #[test]
        fn an_unmatched_deselection_row_is_rejected() {
            let repo = with_one_pipe();
            repo.write("investments/deselection_lists.csv", &format!("{DESEL_HEADER}GHOST-EN24,\n"));
            assert!(matches!(get_investments_configs(repo.path()), Err(TableError::UnmatchedRow { .. })));
        }

        #[test]
        fn a_malformed_deselection_selection_is_rejected() {
            let repo = with_one_pipe();
            repo.write("investments/deselection_lists.csv", &format!("{DESEL_HEADER}A-EN24,???\n"));
            assert!(matches!(get_investments_configs(repo.path()), Err(TableError::InvalidLineSelection { .. })));
        }
    }

    mod partial_pipes {
        use super::*;

        fn repo_with(args_row: &str, partial_row: &str) -> Repo {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}{args_row}"));
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}{partial_row}"));
            repo
        }

        #[test]
        fn every_segment_is_wanted_when_no_row_declares_otherwise() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "");
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert!(config.wants_pdf_extract() && config.wants_text_filter() && config.wants_deserialize());
        }

        #[test]
        fn an_empty_cell_still_means_wanted() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,,,\n");
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert!(config.wants_pdf_extract() && config.wants_text_filter() && config.wants_deserialize());
        }

        #[test]
        fn a_false_cell_disables_its_segment() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,FALSE,TRUE,TRUE\n");
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert!(!config.wants_pdf_extract());
            assert!(config.wants_text_filter() && config.wants_deserialize());
        }

        #[test]
        fn a_disabled_segment_carrying_its_configuration_is_a_contradiction() {
            let repo = repo_with("A-EN24,,,ArialMT,1,,,,\n", "A-EN24,FALSE,TRUE,TRUE\n");
            let err = get_investments_configs(repo.path()).unwrap_err();
            let TableError::DisabledSegmentConfigured { segment, column, .. } = err else {
                panic!("expected DisabledSegmentConfigured, got {err}")
            };
            assert_eq!((segment, column), ("pdf_extract", "Body set"));
        }

        #[test]
        fn a_disabled_text_filter_may_not_carry_a_column_position() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,TRUE,FALSE,TRUE\n");
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(err, TableError::DisabledSegmentConfigured { segment: "text_filter", column: "Market value", .. }),
                "{err}"
            );
        }

        #[test]
        fn a_disabled_deserialize_may_not_carry_its_interpretation_flags() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,,,,,\n"));
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}A-EN24,TRUE,TRUE,FALSE\n"));
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,,,,TRUE,,,,,\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(err, TableError::DisabledSegmentConfigured { segment: "deserialize", .. }),
                "{err}"
            );
        }

        #[test]
        fn a_disabled_segment_with_nothing_configured_is_fine() {
            let repo = repo_with("A-EN24,,,,,,,,\n", "A-EN24,FALSE,TRUE,TRUE\n");
            assert!(get_investments_configs(repo.path()).is_ok());
        }

        #[test]
        fn a_deselection_row_counts_as_pdf_extract_configuration() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,FALSE,TRUE,TRUE\n");
            repo.write("investments/deselection_lists.csv", &format!("{DESEL_HEADER}A-EN24,ArialMT\n"));
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(err, TableError::DisabledSegmentConfigured { column: "Deselection set", .. }),
                "{err}"
            );
        }

        #[test]
        fn an_unparsable_boolean_cell_is_rejected() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,maybe,,\n");
            assert!(matches!(get_investments_configs(repo.path()), Err(TableError::MalformedRow { line: 1, .. })));
        }

        #[test]
        fn a_lowercase_boolean_cell_is_accepted() {
            let repo = repo_with("A-EN24,,,,,,,,\n", "A-EN24,false,true,true\n");
            assert!(!get_investments_configs(repo.path()).unwrap()[0].wants_pdf_extract());
        }
    }

    mod page_classify {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn groups_the_header_sets_of_one_pipe_together() {
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!(
                    "{PAGE_CLASSIFY_HEADER}\
                     CARNE-EN23/0,\"Arial-BoldMT \"\"Description\"\"\",investments\n\
                     CARNE-EN23/0,\"Arial-BoldMT \"\"Currency\"\"\",investments\n\
                     CARNE-EN23/1,\"arial-boldmt \"\"Management Company\"\"\",manco\n"
                ),
            );
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert_eq!(configs.len(), 2);
            assert_eq!(configs[0].class, "investments");
            assert_eq!(configs[0].header_sets.len(), 2);
            assert_eq!(configs[1].class, "manco");
            assert_eq!(configs[1].header_sets.len(), 1);
        }

        #[test]
        fn the_default_pipeline_of_this_table_is_the_unnamed_one() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,investments\n"));
            let config = &get_page_classify_configs(repo.path()).unwrap()[0];
            assert_eq!(config.id.to_string(), "A-EN24()/0");
        }

        #[test]
        fn rows_without_an_index_all_belong_to_pipe_zero() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24,,inv\nA-EN24,,inv\n"));
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert_eq!(configs.len(), 1);
            assert_eq!(configs[0].id.index, 0);
        }

        #[test]
        fn two_rows_of_one_pipe_declaring_different_classes_are_rejected() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\nA-EN24/0,,manco\n"));
            let err = get_page_classify_configs(repo.path()).unwrap_err();
            let TableError::ConflictingClass { line, first, found, .. } = err else {
                panic!("expected ConflictingClass, got {err}")
            };
            assert_eq!((line, first.as_str(), found.as_str()), (2, "inv", "manco"));
        }

        #[test]
        fn different_pipes_may_declare_different_classes() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\nA-EN24/1,,manco\n"));
            assert_eq!(get_page_classify_configs(repo.path()).unwrap().len(), 2);
        }

        #[test]
        fn a_pipe_with_no_header_set_at_all_is_kept_with_an_empty_list() {
            // A classifier with no headers matches every page: rare but legal, and it must be
            // built.
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\n"));
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert!(configs[0].header_sets.is_empty());
        }

        #[test]
        fn a_malformed_header_set_names_the_column_and_the_line() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\nA-EN24/0,???,inv\n"));
            let err = get_page_classify_configs(repo.path()).unwrap_err();
            let TableError::InvalidLineSelection { line, column, .. } = err else {
                panic!("expected InvalidLineSelection, got {err}")
            };
            assert_eq!((line, column.as_str()), (2, "Header set"));
        }

        #[test]
        fn the_order_of_the_result_follows_first_appearance_in_the_file() {
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!("{PAGE_CLASSIFY_HEADER}B-EN24/0,,b\nA-EN24/0,,a\nB-EN24/0,,b\n"),
            );
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert_eq!(
                configs.iter().map(|c| c.id.format.as_str()).collect::<Vec<_>>(),
                vec!["B-EN24", "A-EN24"]
            );
        }

        #[test]
        fn an_empty_table_declares_no_pipe() {
            assert!(get_page_classify_configs(Repo::new().path()).unwrap().is_empty());
        }

        #[test]
        fn a_missing_file_is_reported_with_its_path() {
            let repo = Repo::new();
            fs::remove_file(repo.path().join(STRUCTURED_DIR).join("page_classify/args.csv")).unwrap();
            assert!(matches!(get_page_classify_configs(repo.path()), Err(TableError::MissingCsv(_))));
        }
    }
}
