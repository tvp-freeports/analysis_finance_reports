//! The CSV tables of the structured level: reading, validating and joining.
//!
//! Six files in three groups:
//!
//! - **investments** — a main arguments table with one row per pipe, two tables with at most one row per pipe, and a deselection table with as many rows per pipe as wanted;
//! - **page_classify** — one arguments table with several rows per pipe, one per header set;
//! - **fund_assets** — one arguments table with **one** row per pipe, carrying the parameters of
//!   all three segments.
//!
//! Each of the five algorithms also has a `partial_pipes.csv`, saying which of the three segments a
//! pipe builds: see [`Segments`].
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

use crate::formats_utils::pdf_extract::position::SplittingDirection;
use crate::formats_utils::pdf_extract::tabularizer::collapse::CollapseAlgorithm;
use crate::input::document::selection::is_pdfline_selection;

use super::super::csv_columns;
use super::super::id_format::{ComputedId, FkRelation, computed_ids, id_matches};

/// The directory of the structured CSV files inside a formats repository.
pub const STRUCTURED_DIR: &str = "content/algorithms/structured";

/// The pipeline the management company's rows belong to when they declare none.
pub const MANCO_PIPELINE: &str = "manco";

/// The pipeline that investments rows belong to when they declare none.
pub const INVESTMENTS_PIPELINE: &str = "investments";

/// The same for the SFDR annex. A format with two annex layouts names the second one itself —
/// KAIROS-EN23 is the measured case, `sfdr_classification_2` (`R27`).
pub const SFDR_PIPELINE: &str = "sfdr_classification";

/// Failures of reading or validating a structured table.
///
/// Every variant names the file and, where it makes sense, the row.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    #[error("missing formats-repository CSV file: {0}")]
    MissingCsv(PathBuf),
    #[error("{path}: malformed row at line {line}: {reason}")]
    MalformedRow {
        path: PathBuf,
        line: usize,
        reason: String,
    },
    #[error("{path}: missing required column '{column}'")]
    MissingColumn { path: PathBuf, column: String },
    #[error("{path}, line {line}: ID '{id}' does not match the expected ID pattern")]
    InvalidId {
        path: PathBuf,
        line: usize,
        id: String,
    },
    #[error("{path}, line {line}: column '{column}' is not a valid line selection: '{value}'")]
    InvalidLineSelection {
        path: PathBuf,
        line: usize,
        column: String,
        value: String,
    },
    /// A row of a secondary table has no counterpart in the main table.
    #[error("{path}, line {line}: '{id}' has no matching row in the principal table")]
    UnmatchedRow {
        path: PathBuf,
        line: usize,
        id: String,
    },
    /// Two rows of an at-most-one-per-pipe table configure the same pipe.
    #[error(
        "{path}, line {line}: '{id}' is configured twice, but this table allows at most one row per pipe"
    )]
    DuplicateRow {
        path: PathBuf,
        line: usize,
        id: String,
    },
    /// A disabled segment carries configuration anyway.
    #[error(
        "{path}, line {line}: segment '{segment}' is disabled for '{id}' but column '{column}' is not empty"
    )]
    DisabledSegmentConfigured {
        path: PathBuf,
        line: usize,
        id: String,
        segment: &'static str,
        column: &'static str,
    },
    /// A column a built segment cannot do without is empty.
    #[error(
        "{path}, line {line}: column '{column}' is empty, but '{id}' builds segment '{segment}', which needs it"
    )]
    RequiredCellEmpty {
        path: PathBuf,
        line: usize,
        id: String,
        segment: &'static str,
        column: &'static str,
    },
    /// Two pipes of one pipeline disagree on a segment the pipeline builds once for all of them.
    #[error(
        "{path}, line {line}: '{id}' and '{other}' are pipes of the same pipeline but disagree on segment '{segment}', which the pipeline builds once for all of its pipes"
    )]
    ConflictingSegments {
        path: PathBuf,
        line: usize,
        id: String,
        other: String,
        segment: &'static str,
    },
    /// Two rows configuring the same classification pipe declare different classes.
    #[error("{path}, line {line}: '{id}' is declared as class '{found}' but also as '{first}'")]
    ConflictingClass {
        path: PathBuf,
        line: usize,
        id: String,
        first: String,
        found: String,
    },
}

// ----------------------------------------------------------------------------------------------
// Deserializers for the optional cells
// ----------------------------------------------------------------------------------------------
/// A text cell: empty means absent.
fn optional_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    Ok(if raw.trim().is_empty() {
        None
    } else {
        Some(raw)
    })
}

/// A numeric cell: empty means absent, anything else must be a number.
fn optional_number<'de, D: Deserializer<'de>, T: FromStr>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
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
        other => Err(serde::de::Error::custom(format!(
            "expected TRUE, FALSE or an empty cell, found '{other}'"
        ))),
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
fn optional_window<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ValueWindow>, D::Error> {
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
        text.parse::<f32>()
            .map_err(|e| serde::de::Error::custom(format!("'{text}' is not a number: {e}")))
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
    #[serde(
        rename = "Interpret quantity as float",
        deserialize_with = "optional_bool"
    )]
    pub interpret_quantity_as_float: Option<bool>,
    #[serde(
        rename = "Interpret cost and value as int",
        deserialize_with = "optional_bool"
    )]
    pub interpret_cost_and_value_as_int: Option<bool>,
    #[serde(rename = "Geometrical indexing", deserialize_with = "optional_bool")]
    pub geometrical_indexing: Option<bool>,
    /// The index of the column whose cells may break across several lines — the column of names,
    /// since a long name wraps and a number does not.
    ///
    /// Naming it **is** asking for the collapse: with the cell filled the `pdf_extract` emits one
    /// block per cell instead of one per line, joining the pieces of a broken one. Which lines are
    /// one cell is decided by `tabularizer::collapse`, and every other column is left alone.
    ///
    /// `default`, like `Remove from fund` and for the same reason: it arrived after repositories
    /// existed, and one that does not need it must not grow an empty column to keep loading.
    /// Several columns, because a page often carries **two tables side by side** and then the
    /// column of names is two: `1 | 5` on AMUNDI-EN24, whose grid is eight columns wide.
    #[serde(rename = "Collapse", default, deserialize_with = "optional_text")]
    pub collapse: Option<String>,
    /// Which way a broken cell collapses: `down` (the default) puts the pieces on the **first** row
    /// of the run, `up` on the last. It is what `Merge previous` used to choose on the other side of
    /// the pipeline.
    #[serde(
        rename = "Collapse direction",
        default,
        deserialize_with = "optional_named"
    )]
    pub collapse_direction: Option<CollapseDirectionCell>,
    /// Which of the two collapse algorithms reads the run: `pattern` (the default) takes every run
    /// of consecutive cells of the column, `geometry` only the runs whose rows carry **nothing but
    /// the name**.
    ///
    /// The difference is measured, and it is the difference between a format that needs the
    /// collapse and one that would be harmed by it: on MEDIOLANUM-IT24.A a country heading
    /// (`IRLANDA`) sits in the name's column with a full row of values beside it, and `pattern`
    /// glues it to the investment above. `geometry` leaves it alone, because its row is not empty.
    #[serde(
        rename = "Collapse algorithm",
        default,
        deserialize_with = "optional_named"
    )]
    pub collapse_algorithm: Option<CollapseAlgorithmCell>,
    /// A flag expression naming the numeric fields that read the report's dash as zero — text
    /// rather than a bool, like `Algorithm flags` and for the same reason: the cell names a set.
    #[serde(rename = "Interpret dash as zero", deserialize_with = "optional_text")]
    pub interpret_dash_as_zero: Option<String>,
    /// A regex whose matches are cut out of the fund's name before it becomes the sub-fund's
    /// identity — the `(continued)` a continuation page writes above the rest of a table, or a
    /// footnote mark at its end (`R16`), `\s*[*]+$`.
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
    #[serde(
        rename = "Remove from fund",
        default,
        deserialize_with = "optional_text"
    )]
    pub remove_from_fund: Option<String>,
    /// The pattern that reads the **quantity out of the name cell**, for a report that prints the
    /// two in one cell: `800,000.00 ENEL FIN INTL NV 3.375% …`.
    ///
    /// One pattern and not a list, and `Remove from fund` is the same: a regular expression says
    /// *either of these* with `|` of its own, so a second separator here would collide with the
    /// language the cell is written in.
    #[serde(
        rename = "Quantity in name",
        default,
        deserialize_with = "optional_text"
    )]
    pub quantity_in_name: Option<String>,
}

/// The `Collapse direction` cell: which way a broken cell collapses.
///
/// A named cell rather than a bool, because `up`/`down` is what the collapse mechanism itself calls
/// the two directions, and a repository that writes the engine's own word needs no translation
/// table in its head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseDirectionCell {
    Down,
    Up,
}

impl FromStr for CollapseDirectionCell {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "down" => Ok(CollapseDirectionCell::Down),
            "up" => Ok(CollapseDirectionCell::Up),
            other => Err(format!("'{other}' is not a collapse direction: down or up")),
        }
    }
}

impl CollapseDirectionCell {
    /// The engine's own direction.
    pub fn native(self) -> SplittingDirection {
        match self {
            CollapseDirectionCell::Down => SplittingDirection::Down,
            CollapseDirectionCell::Up => SplittingDirection::Up,
        }
    }
}

/// The `Collapse algorithm` cell: which of the two readings of a run.
///
/// Only the two the measured formats need. The engine has four — the two and their two
/// combinations — and a cell naming a combination can be added the day a page asks for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseAlgorithmCell {
    Pattern,
    Geometry,
}

impl FromStr for CollapseAlgorithmCell {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "pattern" => Ok(CollapseAlgorithmCell::Pattern),
            "geometry" => Ok(CollapseAlgorithmCell::Geometry),
            other => Err(format!(
                "'{other}' is not a collapse algorithm: pattern or geometry"
            )),
        }
    }
}

impl CollapseAlgorithmCell {
    /// The engine's own algorithm.
    pub fn native(self) -> CollapseAlgorithm {
        match self {
            CollapseAlgorithmCell::Pattern => CollapseAlgorithm::Pattern,
            CollapseAlgorithmCell::Geometry => CollapseAlgorithm::Geometry,
        }
    }
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
/// An empty column means active. Every algorithm has one such table, with the same columns.
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

/// One of the three segments of a pipeline, by the name the partial-pipes table gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    PdfExtract,
    TextFilter,
    Deserialize,
}

impl Segment {
    /// The segment's column in the partial-pipes table, which is also how errors name it.
    pub const fn name(self) -> &'static str {
        match self {
            Segment::PdfExtract => "pdf_extract",
            Segment::TextFilter => "text_filter",
            Segment::Deserialize => "deserialize",
        }
    }
}

/// Which segments a pipe builds: all three, unless its partial-pipes row turns some off.
///
/// A segment turned off is left for another level to fill — a semistructured block or an author's
/// Python — and the structured row keeps the others. That is how a format takes the default reading
/// of a datum and replaces one step of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segments {
    pub pdf_extract: bool,
    pub text_filter: bool,
    pub deserialize: bool,
}

impl Segments {
    pub const ALL: Segments = Segments {
        pdf_extract: true,
        text_filter: true,
        deserialize: true,
    };

    /// The segments a pipe's row asks for. No row, or an empty cell, means built.
    pub fn from_row(row: Option<&PartialPipesRow>) -> Self {
        let wanted = |cell: Option<bool>| cell.unwrap_or(true);
        match row {
            None => Self::ALL,
            Some(row) => Segments {
                pdf_extract: wanted(row.pdf_extract),
                text_filter: wanted(row.text_filter),
                deserialize: wanted(row.deserialize),
            },
        }
    }

    pub fn wants(self, segment: Segment) -> bool {
        match segment {
            Segment::PdfExtract => self.pdf_extract,
            Segment::TextFilter => self.text_filter,
            Segment::Deserialize => self.deserialize,
        }
    }
}

/// A column of an algorithm's tables, and the segments that read it.
///
/// A column read by two segments goes only when both do. One that is `required` must be filled as
/// long as any of its segments is built, and is free to be empty otherwise: the four anchors of a
/// statement of net assets mean nothing to a row whose extraction is somebody else's.
struct SegmentColumn {
    column: &'static str,
    segments: &'static [Segment],
    required: bool,
}

const fn optional(column: &'static str, segments: &'static [Segment]) -> SegmentColumn {
    SegmentColumn {
        column,
        segments,
        required: false,
    }
}

const fn required(column: &'static str, segments: &'static [Segment]) -> SegmentColumn {
    SegmentColumn {
        column,
        segments,
        required: true,
    }
}

const PDF_EXTRACT: &[Segment] = &[Segment::PdfExtract];
const TEXT_FILTER: &[Segment] = &[Segment::TextFilter];
const DESERIALIZE: &[Segment] = &[Segment::Deserialize];

/// Checks a pipe's columns against the segments it builds.
///
/// Declaring a segment off and then configuring it is a contradiction, not something loading should
/// have to guess its way through; leaving empty what a built segment needs is the same mistake the
/// other way round. Both are refused with the row, in the order of `columns`.
fn check_segment_columns(
    path: &Path,
    line: usize,
    id: &ComputedId,
    segments: Segments,
    columns: &[SegmentColumn],
    is_set: impl Fn(&str) -> bool,
) -> Result<(), TableError> {
    for spec in columns {
        let built = spec.segments.iter().find(|s| segments.wants(**s));
        match (built, is_set(spec.column)) {
            (None, true) => {
                return Err(TableError::DisabledSegmentConfigured {
                    path: path.to_path_buf(),
                    line,
                    id: id.to_string(),
                    segment: spec.segments[0].name(),
                    column: spec.column,
                });
            }
            (Some(segment), false) if spec.required => {
                return Err(TableError::RequiredCellEmpty {
                    path: path.to_path_buf(),
                    line,
                    id: id.to_string(),
                    segment: segment.name(),
                    column: spec.column,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

/// A partial-pipes table indexed by pipe, each row with its line number.
type PartialPipes = HashMap<ComputedId, (usize, PartialPipesRow)>;

/// An algorithm's partial-pipes table, at most one row per pipe, each row with its line.
fn read_partial_pipes(
    formats_repo_dir: &Path,
    relative_path: &str,
    pipeline_default: &str,
    known: &HashSet<&ComputedId>,
) -> Result<(PathBuf, PartialPipes), TableError> {
    let (path, rows): (_, Vec<PartialPipesRow>) = read_table(formats_repo_dir, relative_path)?;
    let ids = identify(
        &path,
        &rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        pipeline_default,
        FkRelation::OneToMaybe,
    )?;
    let numbered: Vec<(usize, PartialPipesRow)> = rows
        .into_iter()
        .enumerate()
        .map(|(i, row)| (i + 1, row))
        .collect();
    let rows = index_unique(&path, ids, numbered, known)?;
    Ok((path, rows))
}

/// A row of the page-classify arguments table.
#[derive(Debug, Clone, Deserialize)]
pub struct PageClassifyArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Header set", deserialize_with = "optional_text")]
    pub header_set: Option<String>,
    /// Required while the pipe builds its extraction, which is what reads it.
    #[serde(rename = "Class", deserialize_with = "optional_text")]
    pub class: Option<String>,
}

fn row_error(path: &Path, line: usize, error: &csv::Error) -> TableError {
    let message = error.to_string();
    if let Some(rest) = message.split("missing field `").nth(1)
        && let Some(column) = rest.split('`').next()
    {
        return TableError::MissingColumn {
            path: path.to_path_buf(),
            column: column.to_string(),
        };
    }
    TableError::MalformedRow {
        path: path.to_path_buf(),
        line,
        reason: message,
    }
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
    let mut reader = csv::Reader::from_path(&path).map_err(|e| TableError::MalformedRow {
        path: path.clone(),
        line: 0,
        reason: e.to_string(),
    })?;
    let headers = reader.headers().map_err(|e| TableError::MalformedRow {
        path: path.clone(),
        line: 0,
        reason: e.to_string(),
    })?;
    let known = csv_columns::declared::<T>();
    if let Some(column) = csv_columns::first_unknown(headers.iter(), known) {
        return Err(TableError::MalformedRow {
            path: path.clone(),
            line: 0,
            reason: csv_columns::unknown_column_message(column, known),
        });
    }
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
            return Err(TableError::InvalidId {
                path: path.to_path_buf(),
                line: i + 1,
                id: (*id).to_string(),
            });
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

/// Refuses a cell that has an effect only when another cell says so.
///
/// A dependent column filled without the one it refines is read by nobody — `Collapse direction`
/// with no column to collapse, `Product prepend` on a name that is not joined — and a cell read by
/// nobody is a configuration that silently does nothing. Refused when the repository loads.
fn requires(
    path: &Path,
    line: usize,
    column: &str,
    filled: bool,
    condition: &str,
    satisfied: bool,
) -> Result<(), TableError> {
    if filled && !satisfied {
        return Err(TableError::MalformedRow {
            path: path.to_path_buf(),
            line,
            reason: format!("'{column}' has an effect only {condition}: here it would be ignored"),
        });
    }
    Ok(())
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
            return Err(TableError::UnmatchedRow {
                path: path.to_path_buf(),
                line,
                id: id.to_string(),
            });
        }
        if map.insert(id.clone(), row).is_some() {
            return Err(TableError::DuplicateRow {
                path: path.to_path_buf(),
                line,
                id: id.to_string(),
            });
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
    /// The segments this pipe builds.
    pub fn segments(&self) -> Segments {
        Segments::from_row(self.partial_pipes.as_ref())
    }

    /// Whether the extraction segment is to be built. An absent configuration means yes.
    pub fn wants_pdf_extract(&self) -> bool {
        self.segments().pdf_extract
    }

    /// Whether the filtering segment is to be built. See [`Self::wants_pdf_extract`].
    pub fn wants_text_filter(&self) -> bool {
        self.segments().text_filter
    }

    /// Whether the deserialization segment is to be built. See [`Self::wants_pdf_extract`].
    pub fn wants_deserialize(&self) -> bool {
        self.segments().deserialize
    }
}

/// Everything the repository declares about one page-classification pipe.
#[derive(Debug, Clone)]
pub struct PageClassifyConfig {
    pub id: ComputedId,
    /// The page class the pipe assigns when **every** header matches. Absent only where the pipe
    /// does not build its extraction.
    pub class: Option<String>,
    /// The headers to look for, one per CSV row, in file order.
    pub header_sets: Vec<String>,
    /// Its own extraction, and the filter and deserializer of its pipeline, which every pipe of
    /// the pipeline agrees on.
    pub segments: Segments,
}

/// Which segment reads each column of the page-classification table. The other two segments of a
/// classification pipeline take no parameter at all.
const PAGE_CLASSIFY_COLUMNS: &[SegmentColumn] = &[
    required("Class", PDF_EXTRACT),
    optional("Header set", PDF_EXTRACT),
];

impl PageClassifyConfig {
    /// Whether each column named by [`PAGE_CLASSIFY_COLUMNS`] is filled, by any row of the pipe.
    fn column_is_set(&self, column: &str) -> bool {
        match column {
            "Class" => self.class.is_some(),
            "Header set" => !self.header_sets.is_empty(),
            other => unreachable!("PAGE_CLASSIFY_COLUMNS names an unknown column {other:?}"),
        }
    }
}

/// A row of the management-company table: one whole `manco` pipeline.
///
/// One cell is required, the label. Everything else is what a page adds to the common shape — and
/// the common shape is *a label, the value next to it, and a crop* — so a format laid out the usual
/// way is one cell and one or two more.
#[derive(Debug, Clone, Deserialize)]
pub struct MancoArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    /// How the label is recognised, in the compact grammar. Required while the pipe builds its
    /// extraction.
    #[serde(rename = "Label set", deserialize_with = "optional_text")]
    pub label_set: Option<String>,
    /// `on_line` (the default), `below` or `right`.
    #[serde(rename = "Where", default, deserialize_with = "optional_text")]
    pub placement: Option<String>,
    /// `first` (the default) or `all`.
    #[serde(rename = "Take", default, deserialize_with = "optional_text")]
    pub take: Option<String>,
    /// How far past the label the value may be looked for.
    #[serde(rename = "Reach", default, deserialize_with = "optional_number")]
    pub reach: Option<f32>,
    /// What **closes** the band the value is looked for in.
    #[serde(rename = "Stop set", default, deserialize_with = "optional_text")]
    pub stop_set: Option<String>,
    /// The text the name begins after, in the line the label picked (`R23`).
    #[serde(rename = "After", default, deserialize_with = "optional_text")]
    pub after: Option<String>,
    /// The text the name ends before.
    #[serde(rename = "Before", default, deserialize_with = "optional_text")]
    pub before: Option<String>,
    /// `first` (the default) or `most_recent`, which is `U6`: a page naming two companies with
    /// their periods keeps the one in charge at closing.
    #[serde(rename = "Among", default, deserialize_with = "optional_text")]
    pub among: Option<String>,
    /// Whether the same company is also the investment manager of every sub-fund of the document
    /// (`R15`), which is the second deserializer of seven of the measured pipelines.
    #[serde(
        rename = "Also investments manager",
        default,
        deserialize_with = "optional_bool"
    )]
    pub also_investments_manager: Option<bool>,
}

/// A row of the SFDR arguments table: one periodic annex, in one language.
///
/// One cell is required, the language, and the two others describe how the format's own pages break
/// the product name. The anchor overrides of the semistructured form are deliberately **not** here:
/// see [`super::sfdr`].
#[derive(Debug, Clone, Deserialize)]
pub struct SfdrArgsRow {
    #[serde(rename = "ID")]
    pub id: String,
    /// `ITALIAN`, `ENGLISH`, `SPANISH` or `FRENCH`. Written **once**, where the semistructured form
    /// writes it in two segments with nothing checking that the two agree. Required while either
    /// of the two is built.
    #[serde(rename = "Language", deserialize_with = "optional_text")]
    pub language: Option<String>,
    /// Reads the product name in the label's **column**, joined, for a page that wraps it or breaks
    /// it into fragments. Seven of the sixteen measured annexes.
    #[serde(rename = "Product joined", default, deserialize_with = "optional_bool")]
    pub product_joined: Option<bool>,
    /// The word the annex does not write and the holdings do (`R24`). Only with `Product joined`.
    #[serde(
        rename = "Product prepend",
        default,
        deserialize_with = "optional_text"
    )]
    pub product_prepend: Option<String>,
}

/// A row of the SFDR product-prefix table: one pattern to take off the product name.
///
/// A pattern and not a choice between a pattern and a literal: a literal is a pattern without
/// metacharacters, and two cells of which a row may fill only one are two forms of one thing.
#[derive(Debug, Clone, Deserialize)]
pub struct SfdrPrefixRow {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Pattern")]
    pub pattern: String,
}

/// One SFDR annex pipeline, its prefixes joined in file order.
#[derive(Debug, Clone)]
pub struct SfdrConfig {
    pub id: ComputedId,
    pub args: SfdrArgsRow,
    /// The patterns to strip, in file order. Empty means the language's own prefix, which is what
    /// eleven of the sixteen do not override.
    pub prefixes: Vec<String>,
    pub segments: Segments,
}

/// Which segment reads each column of the two SFDR tables. `Pattern` is the prefix table's.
const SFDR_COLUMNS: &[SegmentColumn] = &[
    required("Language", &[Segment::PdfExtract, Segment::TextFilter]),
    optional("Product joined", PDF_EXTRACT),
    optional("Product prepend", PDF_EXTRACT),
    optional("Pattern", TEXT_FILTER),
];

impl SfdrConfig {
    /// Whether each column named by [`SFDR_COLUMNS`] is filled.
    fn column_is_set(&self, column: &str) -> bool {
        match column {
            "Language" => self.args.language.is_some(),
            "Product joined" => self.args.product_joined.is_some(),
            "Product prepend" => self.args.product_prepend.is_some(),
            "Pattern" => !self.prefixes.is_empty(),
            other => unreachable!("SFDR_COLUMNS names an unknown column {other:?}"),
        }
    }
}

/// Which segment reads each column of the four investments tables.
const INVESTMENTS_COLUMNS: &[SegmentColumn] = &[
    optional("Subfund set", PDF_EXTRACT),
    optional("Currency set", PDF_EXTRACT),
    optional("Body set", PDF_EXTRACT),
    optional("Deselection set", PDF_EXTRACT),
    optional("Algorithm flags", PDF_EXTRACT),
    optional("Column tolerance", PDF_EXTRACT),
    optional("Row tolerance", PDF_EXTRACT),
    optional("Collapse", PDF_EXTRACT),
    optional("Collapse direction", PDF_EXTRACT),
    optional("Collapse algorithm", PDF_EXTRACT),
    optional("Market value", TEXT_FILTER),
    optional("Quantity", TEXT_FILTER),
    optional("% net assets", TEXT_FILTER),
    optional("Acquisition cost", TEXT_FILTER),
    optional("Acquisition currency", TEXT_FILTER),
    optional("Geometrical indexing", TEXT_FILTER),
    optional("Interpret dash as zero", TEXT_FILTER),
    optional("Remove from fund", TEXT_FILTER),
    optional("Interpret quantity as float", DESERIALIZE),
    optional("Interpret cost and value as int", DESERIALIZE),
];

impl InvestmentsConfig {
    /// Whether each column named by [`INVESTMENTS_COLUMNS`] is configured.
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
            "Collapse" => additional.is_some_and(|a| a.collapse.is_some()),
            "Collapse direction" => additional.is_some_and(|a| a.collapse_direction.is_some()),
            "Collapse algorithm" => additional.is_some_and(|a| a.collapse_algorithm.is_some()),
            "Interpret quantity as float" => {
                additional.is_some_and(|a| a.interpret_quantity_as_float.is_some())
            }
            "Interpret cost and value as int" => {
                additional.is_some_and(|a| a.interpret_cost_and_value_as_int.is_some())
            }
            "Interpret dash as zero" => {
                additional.is_some_and(|a| a.interpret_dash_as_zero.is_some())
            }
            "Remove from fund" => additional.is_some_and(|a| a.remove_from_fund.is_some()),
            other => unreachable!("INVESTMENTS_COLUMNS names an unknown column {other:?}"),
        }
    }
}

/// The investments pipe configurations the repository declares, in the order of the arguments
/// table.
///
/// Reads the four tables, validates every row, and joins them on the `(format, pipeline, index)`
/// key.
pub fn get_investments_configs(
    formats_repo_dir: &Path,
) -> Result<Vec<InvestmentsConfig>, TableError> {
    let (args_path, args): (_, Vec<InvestmentsArgsRow>) =
        read_table(formats_repo_dir, "investments/args.csv")?;
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
    for (i, row) in add_rows.iter().enumerate() {
        let collapse = row.collapse.is_some();
        let condition = "where 'Collapse' names a column";
        requires(
            &add_path,
            i + 1,
            "Collapse direction",
            row.collapse_direction.is_some(),
            condition,
            collapse,
        )?;
        requires(
            &add_path,
            i + 1,
            "Collapse algorithm",
            row.collapse_algorithm.is_some(),
            condition,
            collapse,
        )?;
    }
    let add_ids = identify(
        &add_path,
        &add_rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        INVESTMENTS_PIPELINE,
        FkRelation::OneToMaybe,
    )?;
    let mut additional = index_unique(&add_path, add_ids, add_rows, &known)?;

    let (_, mut partial) = read_partial_pipes(
        formats_repo_dir,
        "investments/partial_pipes.csv",
        INVESTMENTS_PIPELINE,
        &known,
    )?;

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
            return Err(TableError::UnmatchedRow {
                path: desel_path.clone(),
                line,
                id: id.to_string(),
            });
        }
        check_line_selection(
            &desel_path,
            line,
            "Deselection set",
            row.deselection_set.as_ref(),
        )?;
        if let Some(set) = row.deselection_set {
            deselections.entry(id).or_default().push(set);
        }
    }

    let mut configs = Vec::with_capacity(args.len());
    for (i, (id, row)) in args_ids.into_iter().zip(args).enumerate() {
        let config = InvestmentsConfig {
            additional: additional.remove(&id),
            deselection_sets: deselections.remove(&id).unwrap_or_default(),
            partial_pipes: partial.remove(&id).map(|(_, row)| row),
            args: row,
            id,
        };
        check_segment_columns(
            &args_path,
            i + 1,
            &config.id,
            config.segments(),
            INVESTMENTS_COLUMNS,
            |column| config.column_is_set(column),
        )?;
        configs.push(config);
    }
    tracing::debug!(
        config_count = configs.len(),
        "read investments configuration rows"
    );
    Ok(configs)
}

/// One management-company pipeline, with the segments it builds.
#[derive(Debug, Clone)]
pub struct MancoConfig {
    pub id: ComputedId,
    pub row: MancoArgsRow,
    pub segments: Segments,
}

/// Which segment reads each column of the management-company table.
const MANCO_COLUMNS: &[SegmentColumn] = &[
    required("Label set", PDF_EXTRACT),
    optional("Where", PDF_EXTRACT),
    optional("Take", PDF_EXTRACT),
    optional("Reach", PDF_EXTRACT),
    optional("Stop set", PDF_EXTRACT),
    optional("After", TEXT_FILTER),
    optional("Before", TEXT_FILTER),
    optional("Among", TEXT_FILTER),
    optional("Also investments manager", DESERIALIZE),
];

impl MancoArgsRow {
    /// Whether each column named by [`MANCO_COLUMNS`] is filled.
    fn column_is_set(&self, column: &str) -> bool {
        match column {
            "Label set" => self.label_set.is_some(),
            "Where" => self.placement.is_some(),
            "Take" => self.take.is_some(),
            "Reach" => self.reach.is_some(),
            "Stop set" => self.stop_set.is_some(),
            "After" => self.after.is_some(),
            "Before" => self.before.is_some(),
            "Among" => self.among.is_some(),
            "Also investments manager" => self.also_investments_manager.is_some(),
            other => unreachable!("MANCO_COLUMNS names an unknown column {other:?}"),
        }
    }
}

/// The management-company configurations, in file order.
///
/// One row per pipeline, and the table may be absent: see [`get_sfdr_configs`] for the reasoning,
/// which is the same. Where it is present its partial-pipes table is too.
pub fn get_manco_configs(formats_repo_dir: &Path) -> Result<Vec<MancoConfig>, TableError> {
    if !formats_repo_dir
        .join(STRUCTURED_DIR)
        .join("manco/args.csv")
        .is_file()
    {
        tracing::debug!("no manco table in this repository: no structured management company");
        return Ok(Vec::new());
    }
    let (path, rows): (_, Vec<MancoArgsRow>) = read_table(formats_repo_dir, "manco/args.csv")?;
    let ids = identify(
        &path,
        &rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        MANCO_PIPELINE,
        FkRelation::OneToOne,
    )?;
    let known: HashSet<&ComputedId> = ids.iter().collect();
    let (_, mut partial) = read_partial_pipes(
        formats_repo_dir,
        "manco/partial_pipes.csv",
        MANCO_PIPELINE,
        &known,
    )?;

    let mut configs = Vec::with_capacity(rows.len());
    for (i, (id, row)) in ids.iter().cloned().zip(rows).enumerate() {
        let line = i + 1;
        let segments = Segments::from_row(partial.remove(&id).map(|(_, row)| row).as_ref());
        check_segment_columns(&path, line, &id, segments, MANCO_COLUMNS, |column| {
            row.column_is_set(column)
        })?;
        check_line_selection(&path, line, "Label set", row.label_set.as_ref())?;
        check_line_selection(&path, line, "Stop set", row.stop_set.as_ref())?;
        let placement = row.placement.as_deref().map(str::trim);
        requires(
            &path,
            line,
            "Reach",
            row.reach.is_some(),
            "where the value is 'below' or 'right' of the label",
            matches!(placement, Some("below") | Some("right")),
        )?;
        // `Take` belongs to the extraction and `Among` to the filter: with the extraction built
        // elsewhere, how many lines it hands over is not this row's to say.
        if segments.pdf_extract {
            requires(
                &path,
                line,
                "Among",
                row.among.is_some(),
                "with 'Take: all', since it chooses among several lines",
                row.take.as_deref().map(str::trim) == Some("all"),
            )?;
        }
        configs.push(MancoConfig { id, row, segments });
    }
    tracing::debug!(
        config_count = configs.len(),
        "read manco configuration rows"
    );
    Ok(configs)
}

/// The SFDR annex configurations, in file order.
///
/// Two tables joined on the id: the arguments, one row per pipeline, and the product prefixes,
/// several rows per pipeline. A prefix row whose id no argument row declares is an error, as it is
/// in the investments tables: a prefix belonging to nothing is a mistake in the repository, not a
/// row to pass over in silence.
pub fn get_sfdr_configs(formats_repo_dir: &Path) -> Result<Vec<SfdrConfig>, TableError> {
    // **An absent file means no rows**, for the reason `get_fund_assets_configs` gives at length:
    // this table arrived after repositories already existed, and one whose annexes are all written
    // in YAML or in Python must not have to grow an empty file to keep loading. The prefix table
    // follows the arguments: a repository that has the one and not the other is declaring prefixes
    // for nothing, which the join below reports. The partial-pipes table, like the arguments it
    // qualifies, is required where they are present.
    if !formats_repo_dir
        .join(STRUCTURED_DIR)
        .join("sfdr/args.csv")
        .is_file()
    {
        tracing::debug!("no sfdr table in this repository: no structured annex");
        return Ok(Vec::new());
    }
    let (args_path, args): (_, Vec<SfdrArgsRow>) = read_table(formats_repo_dir, "sfdr/args.csv")?;
    let args_ids = identify(
        &args_path,
        &args.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        SFDR_PIPELINE,
        FkRelation::OneToOne,
    )?;
    for (i, row) in args.iter().enumerate() {
        requires(
            &args_path,
            i + 1,
            "Product prepend",
            row.product_prepend.is_some(),
            "with 'Product joined'",
            row.product_joined == Some(true),
        )?;
    }
    let known: HashSet<ComputedId> = args_ids.iter().cloned().collect();
    let (_, mut partial) = read_partial_pipes(
        formats_repo_dir,
        "sfdr/partial_pipes.csv",
        SFDR_PIPELINE,
        &known.iter().collect(),
    )?;

    let has_prefixes = formats_repo_dir
        .join(STRUCTURED_DIR)
        .join("sfdr/product_prefixes.csv")
        .is_file();
    let (prefix_path, prefix_rows): (_, Vec<SfdrPrefixRow>) = match has_prefixes {
        true => read_table(formats_repo_dir, "sfdr/product_prefixes.csv")?,
        false => (PathBuf::new(), Vec::new()),
    };
    let prefix_ids = identify(
        &prefix_path,
        &prefix_rows
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        SFDR_PIPELINE,
        FkRelation::OneToMany,
    )?;
    let mut prefixes: HashMap<ComputedId, Vec<String>> = HashMap::new();
    for (i, (id, row)) in prefix_ids.into_iter().zip(prefix_rows).enumerate() {
        let line = i + 1;
        if !known.contains(&id) {
            return Err(TableError::UnmatchedRow {
                path: prefix_path.clone(),
                line,
                id: id.to_string(),
            });
        }
        // An empty pattern is a row that strips nothing, which is a cell left empty by mistake.
        if row.pattern.trim().is_empty() {
            return Err(TableError::MalformedRow {
                path: prefix_path.clone(),
                line,
                reason: "a prefix row needs a 'Pattern'".to_string(),
            });
        }
        prefixes.entry(id).or_default().push(row.pattern);
    }

    let mut configs = Vec::with_capacity(args.len());
    for (i, (id, args)) in args_ids.into_iter().zip(args).enumerate() {
        let config = SfdrConfig {
            prefixes: prefixes.remove(&id).unwrap_or_default(),
            segments: Segments::from_row(partial.remove(&id).map(|(_, row)| row).as_ref()),
            id,
            args,
        };
        check_segment_columns(
            &args_path,
            i + 1,
            &config.id,
            config.segments,
            SFDR_COLUMNS,
            |column| config.column_is_set(column),
        )?;
        configs.push(config);
    }
    tracing::debug!(config_count = configs.len(), "read sfdr configuration rows");
    Ok(configs)
}

/// The page-classification pipe configurations, in order of first appearance.
///
/// Several rows sharing an id describe the **same** pipe with several headers to look for, and must
/// therefore declare the same class; a group that disagrees is an error.
///
/// The partial-pipes table turns segments off per pipe, but only the extraction is a pipe's own: a
/// classification pipeline has **one** filter and **one** deserializer for all its pipes, so the
/// pipes of a pipeline must agree on those two. Building them when any pipe asks would be a rule
/// nobody wrote down, hiding a contradiction in the repository; it is refused instead.
pub fn get_page_classify_configs(
    formats_repo_dir: &Path,
) -> Result<Vec<PageClassifyConfig>, TableError> {
    let (path, rows): (_, Vec<PageClassifyArgsRow>) =
        read_table(formats_repo_dir, "page_classify/args.csv")?;
    // The default pipeline here is the **unnamed** one, not `investments`: a page classifier
    // belongs to the format's default pipeline.
    let ids = identify(
        &path,
        &rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        "",
        FkRelation::OneToMany,
    )?;

    let mut order: Vec<ComputedId> = Vec::new();
    let mut first_lines: HashMap<ComputedId, usize> = HashMap::new();
    let mut classes: HashMap<ComputedId, Option<String>> = HashMap::new();
    let mut headers: HashMap<ComputedId, Vec<String>> = HashMap::new();
    for (i, (id, row)) in ids.into_iter().zip(rows).enumerate() {
        let line = i + 1;
        check_line_selection(&path, line, "Header set", row.header_set.as_ref())?;
        match classes.get(&id) {
            None => {
                order.push(id.clone());
                first_lines.insert(id.clone(), line);
                classes.insert(id.clone(), row.class.clone());
            }
            Some(first) if *first != row.class => {
                return Err(TableError::ConflictingClass {
                    path: path.clone(),
                    line,
                    id: id.to_string(),
                    first: first.clone().unwrap_or_default(),
                    found: row.class.clone().unwrap_or_default(),
                });
            }
            Some(_) => {}
        }
        if let Some(set) = row.header_set {
            headers.entry(id).or_default().push(set);
        }
    }

    let known: HashSet<&ComputedId> = order.iter().collect();
    let (partial_path, mut partial) = read_partial_pipes(
        formats_repo_dir,
        "page_classify/partial_pipes.csv",
        "",
        &known,
    )?;

    let mut configs: Vec<PageClassifyConfig> = Vec::with_capacity(order.len());
    // The first pipe of each pipeline, with its partial-pipes line, against which the others are
    // compared.
    let mut pipelines: HashMap<(String, String), (ComputedId, Option<usize>, Segments)> =
        HashMap::new();
    for id in order {
        let row = partial.remove(&id);
        let partial_line = row.as_ref().map(|(line, _)| *line);
        let config = PageClassifyConfig {
            class: classes
                .remove(&id)
                .expect("ogni id in `order` è stato inserito in `classes`"),
            header_sets: headers.remove(&id).unwrap_or_default(),
            segments: Segments::from_row(row.as_ref().map(|(_, row)| row)),
            id,
        };
        check_segment_columns(
            &path,
            first_lines[&config.id],
            &config.id,
            config.segments,
            PAGE_CLASSIFY_COLUMNS,
            |column| config.column_is_set(column),
        )?;

        let key = (config.id.format.clone(), config.id.pipeline.clone());
        match pipelines.get(&key) {
            None => {
                pipelines.insert(key, (config.id.clone(), partial_line, config.segments));
            }
            Some((first, first_line, agreed)) => {
                for segment in [Segment::TextFilter, Segment::Deserialize] {
                    if config.segments.wants(segment) != agreed.wants(segment) {
                        return Err(TableError::ConflictingSegments {
                            path: partial_path,
                            // At least one of the two has a row, or both would build everything.
                            line: partial_line
                                .or(*first_line)
                                .expect("two pipes that disagree cannot both lack a row"),
                            id: config.id.to_string(),
                            other: first.to_string(),
                            segment: segment.name(),
                        });
                    }
                }
            }
        }
        configs.push(config);
    }
    tracing::debug!(
        config_count = configs.len(),
        "read page classify configuration rows"
    );
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
    ///
    /// This and the three amounts are required while the pipe builds its extraction, the only
    /// segment that reads them.
    #[serde(rename = "Fund set", deserialize_with = "optional_text")]
    pub fund_set: Option<String>,
    /// The line declaring the currency. Absent means the page does not state one.
    #[serde(rename = "Currency set", default, deserialize_with = "optional_text")]
    pub currency_set: Option<String>,
    /// The line carrying the statement's date. Absent means the date comes from elsewhere.
    #[serde(rename = "Date set", default, deserialize_with = "optional_text")]
    pub date_set: Option<String>,
    #[serde(rename = "Total assets", deserialize_with = "optional_text")]
    pub tot_assets: Option<String>,
    #[serde(rename = "Total liabilities", deserialize_with = "optional_text")]
    pub liabilities: Option<String>,
    #[serde(rename = "Net assets", deserialize_with = "optional_text")]
    pub net_assets: Option<String>,
    /// The window of each amount, `dx, dy, width, height`; absent means the engine's default.
    #[serde(
        rename = "Total assets window",
        default,
        deserialize_with = "optional_window"
    )]
    pub tot_assets_window: Option<ValueWindow>,
    #[serde(
        rename = "Total liabilities window",
        default,
        deserialize_with = "optional_window"
    )]
    pub liabilities_window: Option<ValueWindow>,
    #[serde(
        rename = "Net assets window",
        default,
        deserialize_with = "optional_window"
    )]
    pub net_assets_window: Option<ValueWindow>,
    /// The heading of the **number** columns to leave out of the statement.
    #[serde(rename = "Drop set", default, deserialize_with = "optional_text")]
    pub drop_set: Option<String>,
    /// A **fund** column that is not a fund, named by one of its own name lines: it goes whole,
    /// name and numbers. See `AssetsStandardArgs::drop_funds`.
    #[serde(rename = "Drop fund set", default, deserialize_with = "optional_text")]
    pub drop_fund_set: Option<String>,
    /// Whether the fund columns are grouped by the tabularizer rather than taken one by one.
    #[serde(
        rename = "Table condition",
        default,
        deserialize_with = "optional_bool"
    )]
    pub table_condition: Option<bool>,
    /// How the tabularizer finds the fund columns: the flag expression of the investments table's
    /// column of the same name. Absent means the engine's `BIG_CELL_RULE | USE_RULER_AREA`, which
    /// keeps a wrapped name's last word in its column (MEDIOLANUM-IT24.A); FINECO-EN23@LUX p34 is
    /// set so tight that the big-cell rule fuses three columns, and it writes `USE_RULER_AREA`.
    #[serde(
        rename = "Algorithm flags",
        default,
        deserialize_with = "optional_text"
    )]
    pub algorithm_flags: Option<String>,
    /// The expression pulling the date out of the line, with exactly one capturing group.
    #[serde(rename = "Date regex", default, deserialize_with = "optional_text")]
    pub date_regex: Option<String>,
    /// One expression to cut out of the fund's name before it becomes its identity. Several cuts go
    /// in one expression, as an alternation, exactly as in the investments table — a footnote mark
    /// at the end of the name (`R16`) among them, `\s*[*]+$`.
    #[serde(
        rename = "Remove from fund",
        default,
        deserialize_with = "optional_text"
    )]
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
    pub segments: Segments,
}

/// Which segment reads each column of the fund-assets table.
const FUND_ASSETS_COLUMNS: &[SegmentColumn] = &[
    required("Fund set", PDF_EXTRACT),
    optional("Currency set", PDF_EXTRACT),
    optional("Date set", PDF_EXTRACT),
    required("Total assets", PDF_EXTRACT),
    required("Total liabilities", PDF_EXTRACT),
    required("Net assets", PDF_EXTRACT),
    optional("Total assets window", PDF_EXTRACT),
    optional("Total liabilities window", PDF_EXTRACT),
    optional("Net assets window", PDF_EXTRACT),
    optional("Drop set", PDF_EXTRACT),
    optional("Drop fund set", PDF_EXTRACT),
    optional("Table condition", PDF_EXTRACT),
    optional("Algorithm flags", PDF_EXTRACT),
    optional("Date regex", TEXT_FILTER),
    optional("Remove from fund", TEXT_FILTER),
    optional("Amount cast", DESERIALIZE),
    optional("Date cast", DESERIALIZE),
];

impl FundAssetsArgsRow {
    /// Whether each column named by [`FUND_ASSETS_COLUMNS`] is filled.
    fn column_is_set(&self, column: &str) -> bool {
        match column {
            "Fund set" => self.fund_set.is_some(),
            "Currency set" => self.currency_set.is_some(),
            "Date set" => self.date_set.is_some(),
            "Total assets" => self.tot_assets.is_some(),
            "Total liabilities" => self.liabilities.is_some(),
            "Net assets" => self.net_assets.is_some(),
            "Total assets window" => self.tot_assets_window.is_some(),
            "Total liabilities window" => self.liabilities_window.is_some(),
            "Net assets window" => self.net_assets_window.is_some(),
            "Drop set" => self.drop_set.is_some(),
            "Drop fund set" => self.drop_fund_set.is_some(),
            "Table condition" => self.table_condition.is_some(),
            "Algorithm flags" => self.algorithm_flags.is_some(),
            "Date regex" => self.date_regex.is_some(),
            "Remove from fund" => self.remove_from_fund.is_some(),
            "Amount cast" => self.amount_cast.is_some(),
            "Date cast" => self.date_cast.is_some(),
            other => unreachable!("FUND_ASSETS_COLUMNS names an unknown column {other:?}"),
        }
    }
}

/// The fund-assets pipe configurations, in the order they appear.
///
/// **One row per pipe**, unlike the page-classification table: a statement of net assets is one
/// thing, and a format that reads two different statements declares two pipelines rather than two
/// rows of one.
pub fn get_fund_assets_configs(
    formats_repo_dir: &Path,
) -> Result<Vec<FundAssetsConfig>, TableError> {
    // **An absent file means no rows**, unlike the five tables that came before. Those are part of
    // what a formats repository is, and a missing one is a defect in it; this table arrived after
    // repositories already existed, and a repository whose statements of net assets are all written
    // in Python must not have to grow an empty file to keep loading. It is the same reasoning as the
    // `Remove from fund` column's `serde(default)`, applied to a whole table. Its partial-pipes
    // table is required where it is present.
    let path = formats_repo_dir
        .join(STRUCTURED_DIR)
        .join("fund_assets/args.csv");
    if !path.is_file() {
        tracing::debug!(
            ?path,
            "no fund assets table in this repository: no structured statement"
        );
        return Ok(Vec::new());
    }
    let (path, rows): (_, Vec<FundAssetsArgsRow>) =
        read_table(formats_repo_dir, "fund_assets/args.csv")?;
    let ids = identify(
        &path,
        &rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        super::fund_assets::FUND_ASSETS_PIPELINE,
        FkRelation::OneToOne,
    )?;
    let known: HashSet<&ComputedId> = ids.iter().collect();
    let (_, mut partial) = read_partial_pipes(
        formats_repo_dir,
        "fund_assets/partial_pipes.csv",
        super::fund_assets::FUND_ASSETS_PIPELINE,
        &known,
    )?;

    // The set is keyed on `(format, pipeline)` and **not** on the whole `ComputedId`, which carries
    // a pipe index: two rows with the same id would otherwise be two pipes of one pipeline, which is
    // what the investments table means by it and is meaningless here — a second extraction pipe
    // would read the same statement twice. Refused rather than silently resolved either way.
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut configs = Vec::with_capacity(rows.len());
    for (i, (id, row)) in ids.iter().cloned().zip(rows).enumerate() {
        let line = i + 1;
        let segments = Segments::from_row(partial.remove(&id).map(|(_, row)| row).as_ref());
        check_segment_columns(&path, line, &id, segments, FUND_ASSETS_COLUMNS, |column| {
            row.column_is_set(column)
        })?;
        check_line_selection(&path, line, "Fund set", row.fund_set.as_ref())?;
        check_line_selection(&path, line, "Currency set", row.currency_set.as_ref())?;
        check_line_selection(&path, line, "Date set", row.date_set.as_ref())?;
        check_line_selection(&path, line, "Total assets", row.tot_assets.as_ref())?;
        check_line_selection(&path, line, "Total liabilities", row.liabilities.as_ref())?;
        check_line_selection(&path, line, "Net assets", row.net_assets.as_ref())?;
        check_line_selection(&path, line, "Drop set", row.drop_set.as_ref())?;
        check_line_selection(&path, line, "Drop fund set", row.drop_fund_set.as_ref())?;
        let tabular = row.table_condition == Some(true);
        requires(
            &path,
            line,
            "Drop fund set",
            row.drop_fund_set.is_some(),
            "with 'Table condition'",
            tabular,
        )?;
        requires(
            &path,
            line,
            "Algorithm flags",
            row.algorithm_flags.is_some(),
            "with 'Table condition'",
            tabular,
        )?;
        // `Date set` belongs to the extraction, the regex and the cast to the two segments after it:
        // with the extraction built elsewhere, whether a date line is read is not this row's to say.
        let dated = row.date_set.is_some() || !segments.pdf_extract;
        requires(
            &path,
            line,
            "Date regex",
            row.date_regex.is_some(),
            "on a date the 'Date set' reads",
            dated,
        )?;
        requires(
            &path,
            line,
            "Date cast",
            row.date_cast.is_some(),
            "on a date the 'Date set' reads",
            dated,
        )?;
        if !seen.insert((id.format.clone(), id.pipeline.clone())) {
            return Err(TableError::DuplicateRow {
                path: path.clone(),
                line,
                id: id.to_string(),
            });
        }
        configs.push(FundAssetsConfig { id, row, segments });
    }
    tracing::debug!(
        config_count = configs.len(),
        "read fund assets configuration rows"
    );
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const ARGS_HEADER: &str = "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n";
    const ADD_HEADER: &str = "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero\n";
    const PARTIAL_HEADER: &str = "ID,pdf_extract,text_filter,deserialize\n";
    const DESEL_HEADER: &str = "ID,Deselection set\n";
    const PAGE_CLASSIFY_HEADER: &str = "ID,Header set,Class\n";

    /// A formats repository with the six structured CSV files. Each table starts as a header row
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
            repo.write("page_classify/partial_pipes.csv", PARTIAL_HEADER);
            repo
        }

        fn write(&self, relative: &str, content: &str) -> &Self {
            let path = self.dir.path().join(STRUCTURED_DIR).join(relative);
            fs::create_dir_all(path.parent().expect("a file inside a table directory"))
                .expect("table dir");
            fs::write(path, content).expect("write csv");
            self
        }

        /// Writes the fund-assets table, with a partial-pipes table of its header alone. Not
        /// written by `new`, because an absent table is legitimate and several tests are about
        /// exactly that.
        fn write_fund_assets(&self, content: &str) -> &Self {
            self.write("fund_assets/args.csv", content);
            self.write("fund_assets/partial_pipes.csv", PARTIAL_HEADER)
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }
    }

    /// The header of the fund-assets table.
    const ASSETS_HEADER: &str = "ID,Fund set,Currency set,Date set,Total assets,Total liabilities,Net assets,Total assets window,Total liabilities window,Net assets window,Drop set,Drop fund set,Table condition,Date regex,Remove from fund,Amount cast,Date cast\n";

    /// The SFDR annex table, which is two tables joined on the id.
    mod sfdr_args {
        use super::*;
        use pretty_assertions::assert_eq;

        const SFDR_HEADER: &str = "ID,Language,Product joined,Product prepend\n";
        const PREFIX_HEADER: &str = "ID,Pattern\n";

        fn repo_with(args: &str, prefixes: &str) -> Repo {
            let repo = Repo::new();
            let base = repo.path().join(STRUCTURED_DIR).join("sfdr");
            fs::create_dir_all(&base).expect("sfdr dir");
            fs::write(base.join("args.csv"), format!("{SFDR_HEADER}{args}")).expect("write args");
            fs::write(base.join("partial_pipes.csv"), PARTIAL_HEADER).expect("write partial");
            if !prefixes.is_empty() {
                fs::write(
                    base.join("product_prefixes.csv"),
                    format!("{PREFIX_HEADER}{prefixes}"),
                )
                .expect("write prefixes");
            }
            repo
        }

        /// A repository that has no annex table at all still loads: the table arrived after
        /// repositories existed.
        #[test]
        fn an_absent_table_is_no_rows_and_not_a_failure() {
            assert!(get_sfdr_configs(Repo::new().path()).unwrap().is_empty());
        }

        #[test]
        fn one_cell_is_a_whole_row() {
            let configs = get_sfdr_configs(repo_with("ANIMA-EN23,ENGLISH,,\n", "").path()).unwrap();
            assert_eq!(configs.len(), 1);
            assert_eq!(
                configs[0].id.to_string(),
                "ANIMA-EN23(sfdr_classification)/0"
            );
            assert_eq!(configs[0].args.language.as_deref(), Some("ENGLISH"));
            assert_eq!(configs[0].args.product_joined, None);
            assert!(configs[0].prefixes.is_empty());
        }

        /// The pipeline is named only where it is not the conventional one — KAIROS-EN23's second
        /// layout, `R27`.
        #[test]
        fn a_second_layout_names_its_own_pipeline() {
            let configs = get_sfdr_configs(
                repo_with(
                    "KAIROS-EN23,ENGLISH,,\nKAIROS-EN23(sfdr_classification_2),ENGLISH,,\n",
                    "",
                )
                .path(),
            )
            .unwrap();
            let names: Vec<String> = configs.iter().map(|c| c.id.pipeline.clone()).collect();
            assert_eq!(names, ["sfdr_classification", "sfdr_classification_2"]);
        }

        #[test]
        fn the_prefixes_keep_their_order() {
            let repo = repo_with(
                "MEDIOLANUM-IT24.A,ITALIAN,,\n",
                "MEDIOLANUM-IT24.A,Nome del prodotto: \nMEDIOLANUM-IT24.A, \\(.*\\)\n",
            );
            let configs = get_sfdr_configs(repo.path()).unwrap();
            assert_eq!(
                configs[0].prefixes,
                ["Nome del prodotto: ", " \\(.*\\)"],
                "in file order"
            );
        }

        /// A row with an empty pattern strips nothing, which is a cell left empty by mistake.
        #[test]
        fn a_prefix_row_needs_a_pattern() {
            let repo = repo_with("AMUNDI-EN24,ENGLISH,,\n", "AMUNDI-EN24,\n");
            let error = get_sfdr_configs(repo.path())
                .err()
                .expect("an empty pattern");
            assert!(error.to_string().contains("needs a 'Pattern'"), "{error}");
        }

        #[test]
        fn a_prefix_for_an_id_no_argument_row_declares_is_refused() {
            let repo = repo_with("AMUNDI-EN24,ENGLISH,,\n", "AMUNDI-IT24,Nome del prodotto\n");
            assert!(matches!(
                get_sfdr_configs(repo.path()),
                Err(TableError::UnmatchedRow { .. })
            ));
        }

        #[test]
        fn a_prepend_without_a_joined_name_is_refused() {
            let error =
                get_sfdr_configs(repo_with("CARNE-EN23,ENGLISH,,SIDERA FUNDS SICAV\n", "").path())
                    .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("'Product prepend' has an effect only"),
                "{error}"
            );
        }

        #[test]
        fn the_two_optional_cells_are_read() {
            let configs = get_sfdr_configs(
                repo_with("CARNE-EN23,ENGLISH,TRUE,SIDERA FUNDS SICAV\n", "").path(),
            )
            .unwrap();
            assert_eq!(configs[0].args.product_joined, Some(true));
            assert_eq!(
                configs[0].args.product_prepend.as_deref(),
                Some("SIDERA FUNDS SICAV")
            );
        }

        #[test]
        fn a_language_is_required() {
            let repo = Repo::new();
            let base = repo.path().join(STRUCTURED_DIR).join("sfdr");
            fs::create_dir_all(&base).expect("sfdr dir");
            fs::write(base.join("args.csv"), "ID,Product joined\nANIMA-EN23,\n").expect("write");
            fs::write(base.join("partial_pipes.csv"), PARTIAL_HEADER).expect("write partial");
            assert!(get_sfdr_configs(repo.path()).is_err());
        }
    }

    mod manco_args {
        use super::*;

        const MANCO_HEADER: &str =
            "ID,Label set,Where,Take,Reach,Stop set,After,Before,Among,Also investments manager\n";

        fn load(row: &str) -> Result<Vec<MancoConfig>, TableError> {
            let repo = Repo::new();
            repo.write("manco/args.csv", &format!("{MANCO_HEADER}{row}"));
            repo.write("manco/partial_pipes.csv", PARTIAL_HEADER);
            get_manco_configs(repo.path())
        }

        #[test]
        fn reach_and_among_are_read_where_they_have_an_effect() {
            assert!(load("A-EN24,arialmt,below,all,12,,,,most_recent,\n").is_ok());
        }

        /// The value on the label's own line has no distance to reach; one line has nothing to
        /// choose among.
        #[test]
        fn reach_on_the_line_and_among_one_line_are_refused() {
            for (row, column) in [
                ("A-EN24,arialmt,,,12,,,,,\n", "Reach"),
                ("A-EN24,arialmt,below,,,,,,most_recent,\n", "Among"),
            ] {
                let error = load(row).unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains(&format!("'{column}' has an effect only")),
                    "{column}: {error}"
                );
            }
        }
    }

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
            tot_assets_window: &'a str,
            liabilities_window: &'a str,
            net_assets_window: &'a str,
            drop_set: &'a str,
            drop_fund_set: &'a str,
            table_condition: &'a str,
            date_regex: &'a str,
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
                    self.tot_assets_window,
                    self.liabilities_window,
                    self.net_assets_window,
                    self.drop_set,
                    self.drop_fund_set,
                    self.table_condition,
                    self.date_regex,
                    self.remove_from_fund,
                    self.amount_cast,
                    self.date_cast,
                ];
                let quoted: Vec<String> = cells
                    .iter()
                    .map(|cell| format!("\"{}\"", cell.replace('"', "\"\"")))
                    .collect();
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
            assert!(
                get_fund_assets_configs(Repo::new().path())
                    .unwrap()
                    .is_empty()
            );
        }

        #[test]
        fn four_cells_are_enough_for_a_row() {
            let configs = get_fund_assets_configs(repo_with(&[Row::minimal()]).path()).unwrap();
            assert_eq!(configs.len(), 1);
            let row = &configs[0].row;
            assert_eq!(row.fund_set.as_deref(), Some("ArialMT"));
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
            let row = Row {
                id: "A-EN24(fund_assets_begin)",
                ..Row::minimal()
            };
            let configs = get_fund_assets_configs(repo_with(&[row]).path()).unwrap();
            assert_eq!(configs[0].id.pipeline, "fund_assets_begin");
        }

        #[test]
        fn two_rows_of_two_formats_are_two_configs() {
            let other = Row {
                id: "B-EN24",
                ..Row::minimal()
            };
            let configs =
                get_fund_assets_configs(repo_with(&[Row::minimal(), other]).path()).unwrap();
            assert_eq!(configs.len(), 2);
        }

        #[test]
        fn one_row_per_pipe_and_not_two() {
            // A statement of net assets is one thing: a format reading two of them declares two
            // pipelines, not two rows of one.
            let err = get_fund_assets_configs(repo_with(&[Row::minimal(), Row::minimal()]).path())
                .unwrap_err();
            assert!(matches!(err, TableError::DuplicateRow { .. }), "{err}");
        }

        #[test]
        fn a_window_cell_is_four_numbers() {
            let row = Row {
                tot_assets_window: "1.2, -0.1, 100, 1.2",
                ..Row::minimal()
            };
            let window = get_fund_assets_configs(repo_with(&[row]).path()).unwrap()[0]
                .row
                .tot_assets_window
                .expect("the window cell");
            assert_eq!(
                (window.dx, window.dy, window.width, window.height),
                (1.2, -0.1, 100.0, 1.2)
            );
        }

        #[test]
        fn a_window_cell_of_three_numbers_says_how_many_it_wanted() {
            let row = Row {
                liabilities_window: "1.2, -0.1, 100",
                ..Row::minimal()
            };
            let err = get_fund_assets_configs(repo_with(&[row]).path()).unwrap_err();
            assert!(err.to_string().contains("four numbers"), "{err}");
        }

        #[test]
        fn a_window_cell_that_is_not_numbers_says_which_one() {
            let row = Row {
                net_assets_window: "1.2, -0.1, wide, 1.2",
                ..Row::minimal()
            };
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
        }

        #[test]
        fn a_cast_is_named_from_a_vocabulary_and_a_typo_lists_it() {
            let row = Row {
                amount_cast: "flaot",
                ..Row::minimal()
            };
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
                let row = Row {
                    amount_cast: name,
                    ..Row::minimal()
                };
                assert!(
                    get_fund_assets_configs(repo_with(&[row]).path()).is_ok(),
                    "{name}"
                );
            }
        }

        #[test]
        fn each_date_cast_name_is_read() {
            for name in ["numeric", "en_month", "it_month", "en_month_first"] {
                let row = Row {
                    date_cast: name,
                    date_set: "ArialMT",
                    ..Row::minimal()
                };
                assert!(
                    get_fund_assets_configs(repo_with(&[row]).path()).is_ok(),
                    "{name}"
                );
            }
        }

        /// A cell that refines another is refused without it: read by nobody, it would be a
        /// configuration that silently does nothing.
        #[test]
        fn a_dependent_cell_without_the_cell_it_refines_is_refused() {
            for (column, row) in [
                (
                    "Drop fund set",
                    Row {
                        drop_fund_set: "ArialMT",
                        ..Row::minimal()
                    },
                ),
                (
                    "Date regex",
                    Row {
                        date_regex: "([0-9]+)",
                        ..Row::minimal()
                    },
                ),
                (
                    "Date cast",
                    Row {
                        date_cast: "en_month",
                        ..Row::minimal()
                    },
                ),
            ] {
                let error = get_fund_assets_configs(repo_with(&[row]).path()).unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains(&format!("'{column}' has an effect only")),
                    "{column}: {error}"
                );
            }
            let with_both = Row {
                drop_fund_set: "ArialMT",
                table_condition: "TRUE",
                ..Row::minimal()
            };
            assert!(get_fund_assets_configs(repo_with(&[with_both]).path()).is_ok());
        }

        /// `Algorithm flags`: read with the same vocabulary as the investments table, refused when
        /// it names a flag that does not exist, and refused without `Table condition`, which is the
        /// only branch that groups fund columns.
        #[test]
        fn the_algorithm_flags_are_read_checked_and_need_a_table() {
            let header = "ID,Fund set,Currency set,Total assets,Total liabilities,Net assets,Table condition,Algorithm flags\n";
            let load = |row: &str| {
                let repo = Repo::new();
                repo.write_fund_assets(&format!("{header}{row}"));
                (
                    get_fund_assets_configs(repo.path()),
                    crate::formats_repo::structured::fund_assets::get_pipelines(
                        repo.path(),
                        "A-EN24",
                    )
                    .map(|p| p.len()),
                )
            };
            let (configs, pipelines) = load(
                "A-EN24,ArialMT,Arial-Bold,Arial-Bold,Arial-Bold,Arial-Bold,TRUE,USE_RULER_AREA\n",
            );
            assert_eq!(
                configs.unwrap()[0].row.algorithm_flags.as_deref(),
                Some("USE_RULER_AREA")
            );
            assert_eq!(pipelines.unwrap(), 1);

            let (_, pipelines) = load(
                "A-EN24,ArialMT,Arial-Bold,Arial-Bold,Arial-Bold,Arial-Bold,TRUE,USE_RULER_ARE\n",
            );
            let error = pipelines.unwrap_err();
            assert!(error.to_string().contains("USE_RULER_ARE"), "{error}");

            let (configs, _) = load(
                "A-EN24,ArialMT,Arial-Bold,Arial-Bold,Arial-Bold,Arial-Bold,,USE_RULER_AREA\n",
            );
            let error = configs.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("'Algorithm flags' has an effect only"),
                "{error}"
            );
        }

        /// The three columns that belonged to pairing by position are gone: the table pairs by
        /// abscissa, and a repository still writing them is told so rather than ignored.
        #[test]
        fn the_columns_of_pairing_by_position_are_refused() {
            for column in ["Pair by abscissa", "Value step", "Values in one line"] {
                let repo = Repo::new();
                repo.write_fund_assets(&format!(
                    "ID,Fund set,Total assets,Total liabilities,Net assets,{column}\n"
                ));
                let error = get_fund_assets_configs(repo.path()).unwrap_err();
                assert!(error.to_string().contains(column), "{column}: {error}");
            }
        }

        #[test]
        fn every_selection_cell_is_validated_and_the_error_names_the_column() {
            let broken = "???";
            for (column, row) in [
                (
                    "Fund set",
                    Row {
                        fund_set: broken,
                        ..Row::minimal()
                    },
                ),
                (
                    "Currency set",
                    Row {
                        currency_set: broken,
                        ..Row::minimal()
                    },
                ),
                (
                    "Date set",
                    Row {
                        date_set: broken,
                        ..Row::minimal()
                    },
                ),
                (
                    "Total assets",
                    Row {
                        tot_assets: broken,
                        ..Row::minimal()
                    },
                ),
                (
                    "Total liabilities",
                    Row {
                        liabilities: broken,
                        ..Row::minimal()
                    },
                ),
                (
                    "Net assets",
                    Row {
                        net_assets: broken,
                        ..Row::minimal()
                    },
                ),
                (
                    "Drop set",
                    Row {
                        drop_set: broken,
                        ..Row::minimal()
                    },
                ),
                (
                    "Drop fund set",
                    Row {
                        drop_fund_set: broken,
                        ..Row::minimal()
                    },
                ),
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
            assert_eq!(
                get_fund_assets_configs(repo_with(&[row]).path())
                    .unwrap()
                    .len(),
                1
            );
        }

        #[test]
        fn the_boolean_cells_accept_the_spreadsheet_spelling() {
            for (cell, expected) in [("TRUE", Some(true)), ("false", Some(false)), ("", None)] {
                let row = Row {
                    table_condition: cell,
                    ..Row::minimal()
                };
                let read = &get_fund_assets_configs(repo_with(&[row]).path()).unwrap()[0].row;
                assert_eq!(read.table_condition, expected, "{cell:?}");
            }
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
                vec![
                    "A-EN24(investments)/0".to_string(),
                    "B-EN24(investments)/0".to_string()
                ]
            );
        }

        #[test]
        fn the_default_pipeline_of_this_table_is_investments() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"),
            );
            assert_eq!(
                get_investments_configs(repo.path()).unwrap()[0].id.pipeline,
                "investments"
            );
        }

        #[test]
        fn two_rows_of_the_same_format_are_two_pipes_of_the_same_pipeline() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nA-EN24,,,,2,,,,\n"),
            );
            let configs = get_investments_configs(repo.path()).unwrap();
            assert_eq!(configs[0].id.index, 0);
            assert_eq!(configs[1].id.index, 1);
        }

        #[test]
        fn an_empty_numeric_cell_becomes_none() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"),
            );
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert_eq!(config.args.market_value, Some(1));
            assert_eq!(config.args.quantity, None);
        }

        #[test]
        fn a_negative_column_position_is_accepted() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,,1,-1,2,,\n"),
            );
            assert_eq!(
                get_investments_configs(repo.path()).unwrap()[0]
                    .args
                    .quantity,
                Some(-1)
            );
        }

        #[test]
        fn a_non_numeric_cell_reports_its_line() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nB-EN24,,,,x,,,,\n"),
            );
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(err, TableError::MalformedRow { line: 2, .. }),
                "{err}"
            );
        }

        #[test]
        fn an_id_carrying_an_index_is_rejected_by_this_table() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24(investments)/0,,,,1,,,,\n"),
            );
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(err, TableError::InvalidId { line: 1, .. }),
                "{err}"
            );
        }

        #[test]
        fn a_malformed_line_selection_names_the_column_and_the_line() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,ArialMT ???,1,,,,\n"),
            );
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
            fs::remove_file(
                repo.path()
                    .join(STRUCTURED_DIR)
                    .join("investments/args.csv"),
            )
            .unwrap();
            assert!(matches!(
                get_investments_configs(repo.path()),
                Err(TableError::MissingCsv(_))
            ));
        }

        #[test]
        fn an_empty_table_declares_no_pipe() {
            assert!(
                get_investments_configs(Repo::new().path())
                    .unwrap()
                    .is_empty()
            );
        }
    }

    mod secondary_tables {
        use super::*;
        use pretty_assertions::assert_eq;

        fn with_one_pipe() -> Repo {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"),
            );
            repo
        }

        #[test]
        fn additional_args_join_onto_the_matching_pipe() {
            let repo = with_one_pipe();
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}A-EN24,,,,TRUE,FALSE,,\n"),
            );
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            let additional = config.additional.as_ref().unwrap();
            assert_eq!(additional.interpret_quantity_as_float, Some(true));
            assert_eq!(additional.interpret_cost_and_value_as_int, Some(false));
            assert_eq!(additional.geometrical_indexing, None);
        }

        #[test]
        fn a_pipe_without_additional_args_simply_has_none() {
            let repo = with_one_pipe();
            assert!(
                get_investments_configs(repo.path()).unwrap()[0]
                    .additional
                    .is_none()
            );
        }

        #[test]
        fn a_full_id_in_a_secondary_table_matches_the_derived_id_of_the_principal_one() {
            let repo = with_one_pipe();
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}A-EN24(investments)/0,,,,TRUE,,,\n"),
            );
            assert!(
                get_investments_configs(repo.path()).unwrap()[0]
                    .additional
                    .is_some()
            );
        }

        #[test]
        fn a_secondary_row_matching_no_pipe_is_rejected_with_its_line() {
            let repo = with_one_pipe();
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}GHOST-EN24,,,,TRUE,,,\n"),
            );
            let err = get_investments_configs(repo.path()).unwrap_err();
            let TableError::UnmatchedRow { line, id, .. } = err else {
                panic!("expected UnmatchedRow, got {err}")
            };
            assert_eq!((line, id.as_str()), (1, "GHOST-EN24(investments)/0"));
        }

        #[test]
        fn two_additional_args_rows_for_one_pipe_are_rejected() {
            let repo = with_one_pipe();
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}A-EN24(investments)/0,,,,TRUE,,,\nA-EN24(investments)/0,,,,FALSE,,,\n"),
            );
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(err, TableError::DuplicateRow { line: 2, .. }),
                "{err}"
            );
        }

        #[test]
        fn deselection_rows_accumulate_in_file_order() {
            let repo = with_one_pipe();
            repo.write(
                "investments/deselection_lists.csv",
                &format!("{DESEL_HEADER}A-EN24,\"Arial \"\"FIRST\"\"\"\nA-EN24,\"Arial \"\"SECOND\"\"\"\n"),
            );
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert_eq!(
                config.deselection_sets,
                vec![
                    "Arial \"FIRST\"".to_string(),
                    "Arial \"SECOND\"".to_string()
                ]
            );
        }

        #[test]
        fn several_deselection_rows_for_one_pipe_are_not_a_duplicate() {
            // The relation is one-to-many: several rows per pipe are the norm, not an error.
            let repo = with_one_pipe();
            repo.write(
                "investments/deselection_lists.csv",
                &format!("{DESEL_HEADER}A-EN24,\nA-EN24,\n"),
            );
            assert!(get_investments_configs(repo.path()).is_ok());
        }

        #[test]
        fn an_unmatched_deselection_row_is_rejected() {
            let repo = with_one_pipe();
            repo.write(
                "investments/deselection_lists.csv",
                &format!("{DESEL_HEADER}GHOST-EN24,\n"),
            );
            assert!(matches!(
                get_investments_configs(repo.path()),
                Err(TableError::UnmatchedRow { .. })
            ));
        }

        #[test]
        fn a_malformed_deselection_selection_is_rejected() {
            let repo = with_one_pipe();
            repo.write(
                "investments/deselection_lists.csv",
                &format!("{DESEL_HEADER}A-EN24,???\n"),
            );
            assert!(matches!(
                get_investments_configs(repo.path()),
                Err(TableError::InvalidLineSelection { .. })
            ));
        }
    }

    /// A column no row type declares is refused at load, **header-only file included**: a stale
    /// `Note marks` would otherwise be a configuration read by nobody.
    #[test]
    fn an_unknown_column_is_refused_even_in_a_file_with_no_rows() {
        let repo = Repo::new();
        repo.write(
            "investments/args.csv",
            &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"),
        );
        repo.write("investments/additional_args.csv", "ID,Note marks\n");
        let error = get_investments_configs(repo.path()).unwrap_err();
        assert!(
            error.to_string().contains("unknown column 'Note marks'"),
            "{error}"
        );
    }

    mod partial_pipes {
        use super::*;

        fn repo_with(args_row: &str, partial_row: &str) -> Repo {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}{args_row}"));
            repo.write(
                "investments/partial_pipes.csv",
                &format!("{PARTIAL_HEADER}{partial_row}"),
            );
            repo
        }

        #[test]
        fn every_segment_is_wanted_when_no_row_declares_otherwise() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "");
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert!(
                config.wants_pdf_extract()
                    && config.wants_text_filter()
                    && config.wants_deserialize()
            );
        }

        #[test]
        fn an_empty_cell_still_means_wanted() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,,,\n");
            let config = &get_investments_configs(repo.path()).unwrap()[0];
            assert!(
                config.wants_pdf_extract()
                    && config.wants_text_filter()
                    && config.wants_deserialize()
            );
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
            let TableError::DisabledSegmentConfigured {
                segment, column, ..
            } = err
            else {
                panic!("expected DisabledSegmentConfigured, got {err}")
            };
            assert_eq!((segment, column), ("pdf_extract", "Body set"));
        }

        #[test]
        fn a_disabled_text_filter_may_not_carry_a_column_position() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,TRUE,FALSE,TRUE\n");
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(
                    err,
                    TableError::DisabledSegmentConfigured {
                        segment: "text_filter",
                        column: "Market value",
                        ..
                    }
                ),
                "{err}"
            );
        }

        #[test]
        fn a_disabled_deserialize_may_not_carry_its_interpretation_flags() {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,,,,,,,,\n"),
            );
            repo.write(
                "investments/partial_pipes.csv",
                &format!("{PARTIAL_HEADER}A-EN24,TRUE,TRUE,FALSE\n"),
            );
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}A-EN24,,,,TRUE,,,\n"),
            );
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(
                    err,
                    TableError::DisabledSegmentConfigured {
                        segment: "deserialize",
                        ..
                    }
                ),
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
            repo.write(
                "investments/deselection_lists.csv",
                &format!("{DESEL_HEADER}A-EN24,ArialMT\n"),
            );
            let err = get_investments_configs(repo.path()).unwrap_err();
            assert!(
                matches!(
                    err,
                    TableError::DisabledSegmentConfigured {
                        column: "Deselection set",
                        ..
                    }
                ),
                "{err}"
            );
        }

        #[test]
        fn an_unparsable_boolean_cell_is_rejected() {
            let repo = repo_with("A-EN24,,,,1,,,,\n", "A-EN24,maybe,,\n");
            assert!(matches!(
                get_investments_configs(repo.path()),
                Err(TableError::MalformedRow { line: 1, .. })
            ));
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
            assert_eq!(configs[0].class.as_deref(), Some("investments"));
            assert_eq!(configs[0].header_sets.len(), 2);
            assert_eq!(configs[1].class.as_deref(), Some("manco"));
            assert_eq!(configs[1].header_sets.len(), 1);
        }

        #[test]
        fn the_default_pipeline_of_this_table_is_the_unnamed_one() {
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,investments\n"),
            );
            let config = &get_page_classify_configs(repo.path()).unwrap()[0];
            assert_eq!(config.id.to_string(), "A-EN24()/0");
        }

        #[test]
        fn rows_without_an_index_all_belong_to_pipe_zero() {
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!("{PAGE_CLASSIFY_HEADER}A-EN24,,inv\nA-EN24,,inv\n"),
            );
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert_eq!(configs.len(), 1);
            assert_eq!(configs[0].id.index, 0);
        }

        #[test]
        fn two_rows_of_one_pipe_declaring_different_classes_are_rejected() {
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\nA-EN24/0,,manco\n"),
            );
            let err = get_page_classify_configs(repo.path()).unwrap_err();
            let TableError::ConflictingClass {
                line, first, found, ..
            } = err
            else {
                panic!("expected ConflictingClass, got {err}")
            };
            assert_eq!((line, first.as_str(), found.as_str()), (2, "inv", "manco"));
        }

        #[test]
        fn different_pipes_may_declare_different_classes() {
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\nA-EN24/1,,manco\n"),
            );
            assert_eq!(get_page_classify_configs(repo.path()).unwrap().len(), 2);
        }

        #[test]
        fn a_pipe_with_no_header_set_at_all_is_kept_with_an_empty_list() {
            // A classifier with no headers matches every page: rare but legal, and it must be
            // built.
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\n"),
            );
            let configs = get_page_classify_configs(repo.path()).unwrap();
            assert!(configs[0].header_sets.is_empty());
        }

        #[test]
        fn a_malformed_header_set_names_the_column_and_the_line() {
            let repo = Repo::new();
            repo.write(
                "page_classify/args.csv",
                &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,,inv\nA-EN24/0,???,inv\n"),
            );
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
                configs
                    .iter()
                    .map(|c| c.id.format.as_str())
                    .collect::<Vec<_>>(),
                vec!["B-EN24", "A-EN24"]
            );
        }

        #[test]
        fn an_empty_table_declares_no_pipe() {
            assert!(
                get_page_classify_configs(Repo::new().path())
                    .unwrap()
                    .is_empty()
            );
        }

        #[test]
        fn a_missing_file_is_reported_with_its_path() {
            let repo = Repo::new();
            fs::remove_file(
                repo.path()
                    .join(STRUCTURED_DIR)
                    .join("page_classify/args.csv"),
            )
            .unwrap();
            assert!(matches!(
                get_page_classify_configs(repo.path()),
                Err(TableError::MissingCsv(_))
            ));
        }
    }

    /// The partial-pipes table of the four tables that came after `investments`: the same file, the
    /// same meaning, checked against each table's own map of columns to segments.
    mod partial_pipes_of_every_table {
        use super::*;

        /// The segment and the column of a refusal, or a panic naming what came instead.
        fn disabled(err: TableError) -> (&'static str, &'static str) {
            match err {
                TableError::DisabledSegmentConfigured {
                    segment, column, ..
                } => (segment, column),
                other => panic!("expected DisabledSegmentConfigured, got {other}"),
            }
        }

        fn required(err: TableError) -> (&'static str, &'static str) {
            match err {
                TableError::RequiredCellEmpty {
                    segment, column, ..
                } => (segment, column),
                other => panic!("expected RequiredCellEmpty, got {other}"),
            }
        }

        mod segments {
            use super::*;
            use pretty_assertions::assert_eq;

            fn row(pdf_extract: Option<bool>, text_filter: Option<bool>) -> PartialPipesRow {
                PartialPipesRow {
                    id: "A-EN24".to_string(),
                    pdf_extract,
                    text_filter,
                    deserialize: None,
                }
            }

            #[test]
            fn no_row_builds_every_segment() {
                assert_eq!(Segments::from_row(None), Segments::ALL);
            }

            #[test]
            fn an_empty_cell_builds_its_segment_and_false_does_not() {
                let segments = Segments::from_row(Some(&row(Some(false), None)));
                assert!(!segments.wants(Segment::PdfExtract));
                assert!(segments.wants(Segment::TextFilter));
                assert!(segments.wants(Segment::Deserialize));
            }

            #[test]
            fn true_is_the_same_as_empty() {
                assert_eq!(
                    Segments::from_row(Some(&row(Some(true), Some(true)))),
                    Segments::ALL
                );
            }
        }

        mod fund_assets {
            use super::*;
            use pretty_assertions::assert_eq;

            const MINIMAL: &str =
                "A-EN24,ArialMT,Arial-Bold,,Arial-Bold,Arial-Bold,Arial-Bold,,,,,,,,,,\n";

            fn load(args: &str, partial: &str) -> Result<Vec<FundAssetsConfig>, TableError> {
                let repo = Repo::new();
                repo.write("fund_assets/args.csv", &format!("{ASSETS_HEADER}{args}"));
                repo.write(
                    "fund_assets/partial_pipes.csv",
                    &format!("{PARTIAL_HEADER}{partial}"),
                );
                get_fund_assets_configs(repo.path())
            }

            #[test]
            fn every_segment_is_built_when_no_row_says_otherwise() {
                assert_eq!(load(MINIMAL, "").unwrap()[0].segments, Segments::ALL);
            }

            #[test]
            fn a_false_cell_turns_its_segment_off() {
                let configs = load(MINIMAL, "A-EN24,,FALSE,\n").unwrap();
                assert!(!configs[0].segments.wants(Segment::TextFilter));
            }

            #[test]
            fn a_disabled_pdf_extract_may_not_carry_its_anchors() {
                let err = load(MINIMAL, "A-EN24,FALSE,,\n").unwrap_err();
                assert_eq!(disabled(err), ("pdf_extract", "Fund set"));
            }

            /// The four anchors are required by the extraction and by nothing else: with the
            /// extraction off, a row is its id and whatever the other two segments read.
            #[test]
            fn a_disabled_pdf_extract_needs_none_of_its_four_anchors() {
                let configs =
                    load("A-EN24,,,,,,,,,,,,,,\\s*[*]+$,float,\n", "A-EN24,FALSE,,\n").unwrap();
                assert!(!configs[0].segments.wants(Segment::PdfExtract));
                assert_eq!(configs[0].row.fund_set, None);
            }

            #[test]
            fn an_anchor_left_empty_with_the_extraction_on_is_refused() {
                let err = load(
                    "A-EN24,ArialMT,Arial-Bold,,Arial-Bold,Arial-Bold,,,,,,,,,,,\n",
                    "",
                )
                .unwrap_err();
                assert_eq!(required(err), ("pdf_extract", "Net assets"));
            }

            #[test]
            fn a_disabled_text_filter_may_not_carry_its_cut() {
                let row = "A-EN24,ArialMT,Arial-Bold,,Arial-Bold,Arial-Bold,Arial-Bold,,,,,,,,\\s*[*]+$,,\n";
                let err = load(row, "A-EN24,,FALSE,\n").unwrap_err();
                assert_eq!(disabled(err), ("text_filter", "Remove from fund"));
            }

            #[test]
            fn a_disabled_deserialize_may_not_carry_its_cast() {
                let row =
                    "A-EN24,ArialMT,Arial-Bold,,Arial-Bold,Arial-Bold,Arial-Bold,,,,,,,,,float,\n";
                let err = load(row, "A-EN24,,,FALSE\n").unwrap_err();
                assert_eq!(disabled(err), ("deserialize", "Amount cast"));
            }

            /// `Date regex` refines `Date set`, which belongs to the extraction. With the extraction
            /// off the date line is read by somebody else, and the regex is not orphaned.
            #[test]
            fn a_cell_refining_another_segments_cell_is_free_when_that_segment_is_off() {
                let configs = load("A-EN24,,,,,,,,,,,,,(\\d+),,,\n", "A-EN24,FALSE,,\n").unwrap();
                assert_eq!(configs[0].row.date_regex.as_deref(), Some("(\\d+)"));
            }

            #[test]
            fn and_still_refused_when_that_segment_is_on() {
                let row =
                    "A-EN24,ArialMT,Arial-Bold,,Arial-Bold,Arial-Bold,Arial-Bold,,,,,,,(\\d+),,,\n";
                let err = load(row, "").unwrap_err();
                assert!(
                    err.to_string().contains("'Date regex' has an effect only"),
                    "{err}"
                );
            }

            #[test]
            fn the_table_is_required_beside_the_arguments() {
                let repo = Repo::new();
                repo.write("fund_assets/args.csv", &format!("{ASSETS_HEADER}{MINIMAL}"));
                assert!(matches!(
                    get_fund_assets_configs(repo.path()),
                    Err(TableError::MissingCsv(_))
                ));
            }

            #[test]
            fn a_row_for_a_pipe_the_arguments_do_not_declare_is_refused() {
                assert!(matches!(
                    load(MINIMAL, "B-EN24,FALSE,,\n"),
                    Err(TableError::UnmatchedRow { line: 1, .. })
                ));
            }
        }

        mod manco {
            use super::*;
            use pretty_assertions::assert_eq;

            const MANCO_HEADER: &str = "ID,Label set,Where,Take,Reach,Stop set,After,Before,Among,Also investments manager\n";

            fn load(args: &str, partial: &str) -> Result<Vec<MancoConfig>, TableError> {
                let repo = Repo::new();
                repo.write("manco/args.csv", &format!("{MANCO_HEADER}{args}"));
                repo.write(
                    "manco/partial_pipes.csv",
                    &format!("{PARTIAL_HEADER}{partial}"),
                );
                get_manco_configs(repo.path())
            }

            #[test]
            fn every_segment_is_built_when_no_row_says_otherwise() {
                let configs = load("A-EN24,arialmt,,,,,,,,\n", "").unwrap();
                assert_eq!(configs[0].segments, Segments::ALL);
                assert_eq!(configs[0].id.to_string(), "A-EN24(manco)/0");
            }

            #[test]
            fn a_disabled_pdf_extract_may_not_carry_its_label() {
                let err = load("A-EN24,arialmt,,,,,,,,\n", "A-EN24,FALSE,,\n").unwrap_err();
                assert_eq!(disabled(err), ("pdf_extract", "Label set"));
            }

            #[test]
            fn a_disabled_pdf_extract_needs_no_label() {
                let configs = load("A-EN24,,,,,,gestione,,,TRUE\n", "A-EN24,FALSE,,\n").unwrap();
                assert_eq!(configs[0].row.label_set, None);
            }

            #[test]
            fn a_label_left_empty_with_the_extraction_on_is_refused() {
                let err = load("A-EN24,,,,,,gestione,,,\n", "").unwrap_err();
                assert_eq!(required(err), ("pdf_extract", "Label set"));
            }

            #[test]
            fn a_disabled_text_filter_may_not_carry_its_crop() {
                let err = load("A-EN24,arialmt,,,,,gestione,,,\n", "A-EN24,,FALSE,\n").unwrap_err();
                assert_eq!(disabled(err), ("text_filter", "After"));
            }

            #[test]
            fn a_disabled_deserialize_may_not_declare_the_investment_manager() {
                let err = load("A-EN24,arialmt,,,,,,,,TRUE\n", "A-EN24,,,FALSE\n").unwrap_err();
                assert_eq!(disabled(err), ("deserialize", "Also investments manager"));
            }

            /// `Among` chooses among the lines `Take: all` hands over, and `Take` belongs to the
            /// extraction: with the extraction elsewhere, what it hands over is not this row's to say.
            #[test]
            fn among_is_free_when_the_extraction_is_off() {
                assert!(load("A-EN24,,,,,,,,most_recent,\n", "A-EN24,FALSE,,\n").is_ok());
            }

            #[test]
            fn the_table_is_required_beside_the_arguments() {
                let repo = Repo::new();
                repo.write(
                    "manco/args.csv",
                    &format!("{MANCO_HEADER}A-EN24,arialmt,,,,,,,,\n"),
                );
                assert!(matches!(
                    get_manco_configs(repo.path()),
                    Err(TableError::MissingCsv(_))
                ));
            }
        }

        mod sfdr {
            use super::*;
            use pretty_assertions::assert_eq;

            const SFDR_HEADER: &str = "ID,Language,Product joined,Product prepend\n";

            fn load(
                args: &str,
                prefixes: &str,
                partial: &str,
            ) -> Result<Vec<SfdrConfig>, TableError> {
                let repo = Repo::new();
                repo.write("sfdr/args.csv", &format!("{SFDR_HEADER}{args}"));
                repo.write(
                    "sfdr/product_prefixes.csv",
                    &format!("ID,Pattern\n{prefixes}"),
                );
                repo.write(
                    "sfdr/partial_pipes.csv",
                    &format!("{PARTIAL_HEADER}{partial}"),
                );
                get_sfdr_configs(repo.path())
            }

            #[test]
            fn every_segment_is_built_when_no_row_says_otherwise() {
                assert_eq!(
                    load("A-EN24,ENGLISH,,\n", "", "").unwrap()[0].segments,
                    Segments::ALL
                );
            }

            /// The language is read by both the extraction and the filter, so turning one of them
            /// off leaves it required by the other.
            #[test]
            fn the_language_stays_with_either_of_its_two_segments() {
                assert!(load("A-EN24,ENGLISH,,\n", "", "A-EN24,FALSE,,\n").is_ok());
                assert!(load("A-EN24,ENGLISH,,\n", "", "A-EN24,,FALSE,\n").is_ok());
                let err = load("A-EN24,,,\n", "", "A-EN24,FALSE,,\n").unwrap_err();
                assert_eq!(required(err), ("text_filter", "Language"));
            }

            #[test]
            fn the_language_goes_only_when_both_its_segments_do() {
                assert!(load("A-EN24,,,\n", "", "A-EN24,FALSE,FALSE,\n").is_ok());
                let err = load("A-EN24,ENGLISH,,\n", "", "A-EN24,FALSE,FALSE,\n").unwrap_err();
                assert_eq!(disabled(err).1, "Language");
            }

            #[test]
            fn a_disabled_pdf_extract_may_not_join_the_product_name() {
                let err = load("A-EN24,ENGLISH,TRUE,\n", "", "A-EN24,FALSE,,\n").unwrap_err();
                assert_eq!(disabled(err), ("pdf_extract", "Product joined"));
            }

            #[test]
            fn a_disabled_text_filter_may_not_carry_prefixes() {
                let err = load(
                    "A-EN24,ENGLISH,,\n",
                    "A-EN24,Product name: \n",
                    "A-EN24,,FALSE,\n",
                )
                .unwrap_err();
                assert_eq!(disabled(err), ("text_filter", "Pattern"));
            }

            #[test]
            fn the_table_is_required_beside_the_arguments() {
                let repo = Repo::new();
                repo.write("sfdr/args.csv", &format!("{SFDR_HEADER}A-EN24,ENGLISH,,\n"));
                assert!(matches!(
                    get_sfdr_configs(repo.path()),
                    Err(TableError::MissingCsv(_))
                ));
            }
        }

        mod page_classify {
            use super::*;
            use pretty_assertions::assert_eq;

            fn load(args: &str, partial: &str) -> Result<Vec<PageClassifyConfig>, TableError> {
                let repo = Repo::new();
                repo.write(
                    "page_classify/args.csv",
                    &format!("{PAGE_CLASSIFY_HEADER}{args}"),
                );
                repo.write(
                    "page_classify/partial_pipes.csv",
                    &format!("{PARTIAL_HEADER}{partial}"),
                );
                get_page_classify_configs(repo.path())
            }

            #[test]
            fn every_segment_is_built_when_no_row_says_otherwise() {
                assert_eq!(
                    load("A-EN24/0,,inv\n", "").unwrap()[0].segments,
                    Segments::ALL
                );
            }

            #[test]
            fn a_disabled_pdf_extract_may_not_declare_a_class() {
                let err = load("A-EN24/0,,inv\n", "A-EN24/0,FALSE,,\n").unwrap_err();
                assert_eq!(disabled(err), ("pdf_extract", "Class"));
            }

            #[test]
            fn a_disabled_pdf_extract_may_not_carry_a_header() {
                let err = load("A-EN24/0,arialmt,\n", "A-EN24/0,FALSE,,\n").unwrap_err();
                assert_eq!(disabled(err), ("pdf_extract", "Header set"));
            }

            #[test]
            fn a_disabled_pdf_extract_needs_no_class() {
                let configs = load("A-EN24/0,,\n", "A-EN24/0,FALSE,,\n").unwrap();
                assert_eq!(configs[0].class, None);
            }

            #[test]
            fn a_class_left_empty_with_the_extraction_on_is_refused() {
                let err = load("A-EN24/0,,\n", "").unwrap_err();
                assert_eq!(required(err), ("pdf_extract", "Class"));
            }

            /// One pipeline, one filter and one deserializer for all its pipes: two pipes cannot
            /// want one each way.
            #[test]
            fn two_pipes_of_one_pipeline_disagreeing_on_the_filter_are_refused() {
                let err =
                    load("A-EN24/0,,inv\nA-EN24/1,,manco\n", "A-EN24/1,,FALSE,\n").unwrap_err();
                let TableError::ConflictingSegments {
                    line,
                    id,
                    other,
                    segment,
                    ..
                } = err
                else {
                    panic!("expected ConflictingSegments, got {err}")
                };
                assert_eq!(
                    (line, id.as_str(), other.as_str(), segment),
                    (1, "A-EN24()/1", "A-EN24()/0", "text_filter")
                );
            }

            #[test]
            fn and_the_same_for_the_deserializer() {
                let err =
                    load("A-EN24/0,,inv\nA-EN24/1,,manco\n", "A-EN24/0,,,FALSE\n").unwrap_err();
                assert!(
                    matches!(
                        err,
                        TableError::ConflictingSegments {
                            segment: "deserialize",
                            ..
                        }
                    ),
                    "{err}"
                );
            }

            #[test]
            fn pipes_agreeing_on_both_may_each_turn_their_own_extraction_off() {
                let configs = load(
                    "A-EN24/0,,inv\nA-EN24/1,,\n",
                    "A-EN24/0,,FALSE,FALSE\nA-EN24/1,FALSE,FALSE,FALSE\n",
                )
                .unwrap();
                assert!(configs[0].segments.wants(Segment::PdfExtract));
                assert!(!configs[1].segments.wants(Segment::PdfExtract));
            }

            #[test]
            fn pipes_of_two_pipelines_need_not_agree() {
                assert!(
                    load(
                        "A-EN24/0,,inv\nA-EN24(other)/0,,manco\n",
                        "A-EN24(other)/0,,FALSE,\n"
                    )
                    .is_ok()
                );
            }

            #[test]
            fn the_table_is_required() {
                let repo = Repo::new();
                fs::remove_file(
                    repo.path()
                        .join(STRUCTURED_DIR)
                        .join("page_classify/partial_pipes.csv"),
                )
                .unwrap();
                assert!(matches!(
                    get_page_classify_configs(repo.path()),
                    Err(TableError::MissingCsv(_))
                ));
            }
        }
    }
}
