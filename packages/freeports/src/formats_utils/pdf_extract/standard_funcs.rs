//! The standard `pdf_extract` pipes: the first segment of a structured or semistructured pipeline.
//!
//! These are what a format's configuration names instead of implementing: extract the fund name,
//! the currency statement, the SFDR article, classify the page, rebuild the investments table, read
//! the assets block. Each takes selections written by the format author and produces [`PdfBlock`]s.
//!
//! # How a format configures the tabularizer
//!
//! The two pipes that build a table — [`PdfExtractInvestmentsStandard`] and, for its fund columns,
//! [`PdfExtractAssetsStandard`] — take a [`TableSettings`] and hand it to the tabularizer
//! unchanged. It is the same set of knobs the free `get_table_coordinates` exposes, under the same
//! names, so a format author who has tuned one has tuned the other.
//!
//! # Where the deselection list is subtracted
//!
//! [`PdfExtractInvestmentsStandard`]'s deselection list is subtracted at selection time rather than
//! folded into the body set when the pipe is built. The two are equivalent, since contextualising
//! distributes over the expression tree, and doing it this way keeps the set algebra out of the
//! selection types.

use std::collections::BTreeMap;

use crate::commons::consts::Currency;
use crate::commons::sets::Container;
use crate::core::classes::{BlockType, BlockValue, PdfBlock};
use crate::core::page::Page;
use crate::core::pipeline::{PdfExtractPipe, PipeError};

use super::commons::{CommonsError, SelectedText, extract_text_pdf_block_or_fail_page, select_expected_text};
use super::labelled_value::{LabelledValueArgs, PdfExtractLabelledValueStandard};
use super::pdf_line::PdfLine;
use super::relative::{OptionallyRelative, RelativeInfo};
use super::select::pdf_line::PdfLineSet;
use super::select::relative::{PdfLineSelection, RelativePdfLineSet, RelativeSelectPdfLineSet};
use super::position::{SplittingDirection, TableConfig};
use super::tabularizer::collapse::CollapseAlgorithm;
use super::tabularizer::coordinates::{CoordinateExtractionError, TablePosAlgorithm};
use super::tabularizer::{TableCoordinatesConfig, TablePosMeasureUnit, get_table_coordinates_from_lines};

/// Failures of the standard `pdf_extract` pipes.
///
/// [`PdfExtractStandardFuncsError::Commons`] is the only variant that can carry a **non-fatal**
/// page failure, being the only one built from [`CommonsError::PageParseFail`]. Everything else
/// stops the run: a page that cannot be parsed is one page, while a malformed configuration is
/// every page.
#[derive(Debug, thiserror::Error)]
pub enum PdfExtractStandardFuncsError {
    /// No line matches a selection that *must* produce at least one result.
    #[error("Pdf block during extraction of \"{name}\" not found")]
    ExpectedPdfBlockNotFound { name: String },
    #[error("{0}")]
    Commons(#[from] CommonsError),
    #[error("{0}")]
    Coordinates(#[from] CoordinateExtractionError),
    /// A zero step for a column range, which has no meaningful interpretation.
    #[error("skip_column must not be zero")]
    ZeroSkipColumn,
    /// The three columns of an assets block have different lengths.
    #[error("assets column \"{column}\" has {found} entries, expected at least {expected}")]
    MismatchedAssetsColumn { column: String, found: usize, expected: usize },
    /// There is no currency token to split off the tail of the fund name.
    #[error("fund column \"{column}\" carries no currency token to split off")]
    MissingCurrencyToken { column: String },
}

impl PdfExtractStandardFuncsError {
    /// Translates into the engine's error type. The pipe's name cannot be recovered from the error,
    /// so the caller supplies it.
    pub fn into_pipe_error(self, pipe: &str) -> PipeError {
        match self {
            PdfExtractStandardFuncsError::Commons(source) => PipeError::from_commons(pipe, source),
            other => PipeError::extraction(pipe, other.to_string()),
        }
    }
}

/// The lines of `lines` that `selection` selects, in page order.
///
/// Contextualises the relative part of the selection, if any, and then filters.
fn select<'a>(selection: &PdfLineSelection, lines: &'a [PdfLine]) -> Vec<&'a PdfLine> {
    let set = selection.clone().contextualize(lines);
    lines.iter().filter(|line| set.contains(line)).collect()
}

/// Like [`select`], but returns the already-contextualised set, for where the selection has to be
/// combined with others before being applied.
fn contextualized(selection: &PdfLineSelection, lines: &[PdfLine]) -> PdfLineSet {
    selection.clone().contextualize(lines)
}


// ----------------------------------------------------------------------------------------------
// ExtractTextPdfBlockOrFailPage and the three factories over it
// ----------------------------------------------------------------------------------------------
/// Extracts the text of the selected line — or lines — into a single [`PdfBlock`]; if it finds
/// nothing, fails the whole page, non-fatally, so the schedule skips it and carries on.
///
/// The pipe behind the three fund/currency/management-company factories, which differ only in the
/// name and block type they configure.
///
/// # A name written in pieces
///
/// [`SelectedText::EveryLine`] exists because a PDF generator splits one centred title into two
/// text objects **on the same ordinate** when a long dash falls in the middle — on CARNE-EN23 the
/// fund is `Sidera Funds SICAV – ` and `Euro ESG Credit` at `y=68.6` on pages 65 and 69, and one
/// object on every other page of the same fund. Taking the first line there is half a name, and
/// half a name does not lose one row: it loses **every** row of that fund, which end up attached to
/// a fund that does not exist (`R24`).
///
/// It is the same algorithm with a parameter, not a second pipe: a format repository had written
/// the joining variant as a Python shim of its own, and the two differed in exactly this.
pub struct ExtractTextPdfBlockOrFailPage {
    selection: PdfLineSelection,
    name: String,
    type_block: BlockType,
    text: SelectedText,
}

impl ExtractTextPdfBlockOrFailPage {
    pub fn new(selection: PdfLineSelection, name: impl Into<String>, type_block: BlockType) -> Self {
        Self { selection, name: name.into(), type_block, text: SelectedText::FirstLine }
    }

    /// The same pipe reading **every** selected line, joined by `separator`.
    ///
    /// A space is the separator that fits a split title, because the generator may or may not leave
    /// one at the end of the first piece and the squeezing makes the two layouts agree; a name split
    /// **inside a word** passes `""`.
    pub fn joining(mut self, separator: impl Into<String>) -> Self {
        self.text = SelectedText::EveryLine { separator: separator.into() };
        self
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let set = contextualized(&self.selection, &page.lines);
        Ok(extract_text_pdf_block_or_fail_page(
            &set,
            &page.lines,
            &self.name,
            self.type_block.clone(),
            &self.text,
        )?)
    }
}

impl PdfExtractPipe for ExtractTextPdfBlockOrFailPage {
    fn name(&self) -> &str {
        &self.name
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}

/// The pipe extracting the fund name.
pub fn pdf_extract_fund_standard(selection: PdfLineSelection) -> ExtractTextPdfBlockOrFailPage {
    ExtractTextPdfBlockOrFailPage::new(selection, "fund", BlockType::FUND_NAME)
}

/// The pipe extracting the currency statement. See [`pdf_extract_fund_standard`].
pub fn pdf_extract_currency_standard(selection: PdfLineSelection) -> ExtractTextPdfBlockOrFailPage {
    ExtractTextPdfBlockOrFailPage::new(selection, "currency", BlockType::CURRENCY_STATEMENT)
}

/// The pipe extracting the management company: [`PdfExtractLabelledValueStandard`] with its
/// default placement, which is the one this pipe always had.
///
/// It is not an alias kept for compatibility — this workspace does not keep those. It is a
/// **signpost**: twenty-one of the twenty-six formats of the reference collection write the
/// management company as a label and a value, so whoever has to extract it should reach the
/// standard algorithm before writing geometry of their own. Hence the name stays while the thing
/// behind it becomes the general one.
///
/// Its parameter keeps the name `selection` and is not called `label`, because the ten formats
/// calling this today do not hand it a label: they hand it a window they built by hand around the
/// value. Calling it a label would be a lie, and the opposite of what the new signature teaches.
///
/// The fund name and the currency statement deliberately do **not** follow: on their pages there
/// is no label beside the value, only a crop of the page, and the general name would point them at
/// the wrong idea.
///
/// The spelling `managment` is kept: it is the string that appears in logs and error messages, and
/// in the configuration of every existing formats repository, so correcting it would be an
/// observable change.
pub fn pdf_extract_managment_company_standard(selection: PdfLineSelection) -> PdfExtractLabelledValueStandard {
    PdfExtractLabelledValueStandard::new(LabelledValueArgs::new(
        selection,
        BlockType::MANAGEMENT_COMPANY,
        "managment company",
    ))
}


// ----------------------------------------------------------------------------------------------
// PdfExtractPageClassifyStandard
// ----------------------------------------------------------------------------------------------
/// Page classifier: the page is of the declared type only if **every** header set finds at least
/// one line.
///
/// Always emits exactly one `PAGE_CLASS` block, with `metadata["page_type"]` either the declared
/// type or `Null`. The downstream segment expects one block per pipe, not one only for the pages
/// that were recognised.
pub struct PdfExtractPageClassifyStandard {
    header_sets: Vec<PdfLineSelection>,
    page_type: String,
}

impl PdfExtractPageClassifyStandard {
    pub fn new(header_sets: Vec<PdfLineSelection>, page_type: impl Into<String>) -> Self {
        Self { header_sets, page_type: page_type.into() }
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        // Only a successful recognition is logged, with the first header line that produced it. Not
        // matching is the normal case — every page class is tried against every page — and says
        // nothing, while the text of a recognised header is exactly what one searches for inside
        // the PDF.
        let matched = self.header_sets.iter().all(|hs| !select(hs, &page.lines).is_empty());
        if matched {
            let header = self
                .header_sets
                .first()
                .and_then(|hs| select(hs, &page.lines).first().map(|line| line.text().clone()))
                .unwrap_or_default();
            tracing::debug!(coord_ref_2 = %self.page_type, found = %header, "page class recognized by its header");
        }
        let page_type =
            if matched { BlockValue::from(self.page_type.as_str()) } else { BlockValue::Null };
        let metadata = BTreeMap::from([("page_type".to_string(), page_type)]);
        Ok(vec![PdfBlock::new(BlockType::PAGE_CLASS, metadata, BlockValue::from(""))])
    }
}

impl PdfExtractPipe for PdfExtractPageClassifyStandard {
    fn name(&self) -> &str {
        "PdfExtractPageClassifyStandard"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}


// ----------------------------------------------------------------------------------------------
// PdfExtractCurrencyConstant
// ----------------------------------------------------------------------------------------------
/// A pipe declaring a constant currency, ignoring the page entirely: for formats where the currency
/// is written nowhere but is known in advance.
///
/// # It says so, once
///
/// Nothing here is read: the currency comes from the format's configuration, and every holding of
/// every page inherits it. That is allowed, and it must not be silent — so the pipe warns into the
/// `.log.csv`.
///
/// **Once per pipe, not once per page.** The fact is about the format, not about the page: a
/// document of forty holdings pages would otherwise leave forty identical rows, which is the shape
/// of noise rather than of an audit trail. The row lands on the first page the pipe runs on, and
/// the wording says it holds for the rest.
///
/// A pipe lives for one job, and a job is one report — a batch builds one per row (`cli/run.rs`),
/// and the several files of a multidocument format are one report between them. So "once per pipe"
/// is "once per report", which is the granularity the audit trail wants: whoever reads one
/// document's data finds the note in that document's rows.
pub struct PdfExtractCurrencyConstant {
    currency: Currency,
    block: PdfBlock,
    warned: std::sync::atomic::AtomicBool,
}

impl PdfExtractCurrencyConstant {
    pub fn new(currency: Currency) -> Self {
        let block = PdfBlock::bare(BlockType::CURRENCY_STATEMENT, currency.code());
        Self { currency, block, warned: std::sync::atomic::AtomicBool::new(false) }
    }

    pub fn currency(&self) -> Currency {
        self.currency
    }

    pub fn call(&self, _page: &Page) -> Vec<PdfBlock> {
        // `swap` and not `load`+`store`: pages of one document run in parallel, and two threads
        // arriving together must still produce one row, not two.
        if !self.warned.swap(true, std::sync::atomic::Ordering::Relaxed) {
            crate::core::tracing_setup::warn_imposed(
                "currency",
                self.currency.code(),
                "declared by the format, not written in the report - holds for every page of it",
            );
        }
        vec![self.block.clone()]
    }
}

impl PdfExtractPipe for PdfExtractCurrencyConstant {
    fn name(&self) -> &str {
        "PdfExtractCurrencyConstant"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        Ok(self.call(page))
    }
}


// ----------------------------------------------------------------------------------------------
// TableSettings
// ----------------------------------------------------------------------------------------------
/// How a standard pipe wants its lines grouped into rows and columns.
///
/// Every field is a field of [`TableCoordinatesConfig`], which is what this becomes — the type
/// exists so that a pipe can carry the settings without also carrying the column of the company
/// name, which is the pipe's own business and not the tabularizer's.
///
/// The one knob that is not here is `company_col`: [`Self::config`] takes it, because the
/// investments pipe already has it as a separate parameter and the assets pipe has no use for it.
#[derive(Debug, Clone)]
pub struct TableSettings {
    /// Explicit rows and columns, when the format knows the grid in advance.
    pub table_config: Option<TableConfig>,
    pub algorithm_flags: TablePosAlgorithm,
    pub collapse_algorithm: CollapseAlgorithm,
    /// How far apart two cells may sit and still count as the same **column**.
    pub col_tolerance: f32,
    /// How far apart two cells may sit and still count as the same **row**. Independent of
    /// [`Self::col_tolerance`] and never derived from it: a table whose column of names breaks up
    /// by name length wants a wide column tolerance and a row tolerance of zero.
    pub row_tolerance: f32,
    /// What the two tolerances are measured in: points, or ems of the line's own font size.
    pub tolerance_unit: TablePosMeasureUnit,
    /// Whether rows broken across several lines are collapsed back into one.
    ///
    /// With it on, the pipe emits **one block per cell** rather than one per line: the lines that
    /// `collapse` put in the same `(row, column)` are joined by [`join_in_reading_order`]. Which
    /// lines those are is decided entirely by
    /// [`super::tabularizer::collapse::collapse_table_rows`] — this pipe adds no rule of its own.
    pub collapse: bool,
    /// Which way a wrapped cell collapses. Only read together with the columns allowed to break.
    pub collapse_direction: SplittingDirection,
    /// The columns allowed to break across several lines. Several, because a page often carries
    /// two tables side by side and then the column of names is two columns.
    pub collapse_cols: Vec<usize>,
}

// Written out rather than derived, for the same reason as `TableCoordinatesConfig`'s: neither
// `TablePosAlgorithm` nor `CollapseAlgorithm` implements `Default`.
impl Default for TableSettings {
    fn default() -> Self {
        Self {
            table_config: None,
            algorithm_flags: TablePosAlgorithm::Default,
            collapse_algorithm: CollapseAlgorithm::Geometry,
            col_tolerance: 0.0,
            row_tolerance: 0.0,
            tolerance_unit: TablePosMeasureUnit::Em,
            collapse: false,
            collapse_direction: SplittingDirection::Down,
            collapse_cols: Vec::new(),
        }
    }
}

impl TableSettings {
    /// The settings [`PdfExtractAssetsStandard`] groups its **fund columns** with unless a format
    /// says otherwise.
    ///
    /// These two flags are not a taste: the pipe has always used them, and a statement of net
    /// assets that carries several funds side by side is read correctly today because of them.
    /// They are the default rather than the only possibility now, and nothing else changes.
    pub fn assets_funds() -> Self {
        Self {
            algorithm_flags: TablePosAlgorithm::BigCellRule | TablePosAlgorithm::UseRulerArea,
            ..Self::default()
        }
    }

    /// The tabularizer's own configuration, with the company column the pipe supplies.
    pub fn config(&self, company_col: Option<usize>) -> TableCoordinatesConfig {
        TableCoordinatesConfig {
            table_config: self.table_config.clone(),
            algorithm_flags: self.algorithm_flags,
            collapse_algorithm: self.collapse_algorithm,
            col_tolerance: self.col_tolerance,
            row_tolerance: self.row_tolerance,
            tolerance_unit: self.tolerance_unit,
            company_col,
            collapse: self.collapse,
            collapse_direction: self.collapse_direction,
            collapse_cols: self.collapse_cols.clone(),
        }
    }
}

// ----------------------------------------------------------------------------------------------
// PdfExtractInvestmentsStandard
// ----------------------------------------------------------------------------------------------
/// The pipe that rebuilds the investments table: one PDF line per cell, with the `(row, column)`
/// coordinates put into the metadata for the next segment to read back.
///
/// How the lines are grouped into rows and columns is [`TableSettings`], which the format writes
/// and which reaches the tabularizer unchanged.
pub struct PdfExtractInvestmentsStandard {
    body_set: PdfLineSelection,
    deselection_list: Vec<PdfLineSelection>,
    table: TableSettings,
    company_index: Option<usize>,
}

/// The construction parameters of [`PdfExtractInvestmentsStandard`], grouped into a struct with a
/// [`Default`] so that call sites name what they set.
pub struct InvestmentsStandardArgs {
    pub body_set: PdfLineSelection,
    pub deselection_list: Vec<PdfLineSelection>,
    pub table: TableSettings,
    pub company_index: Option<usize>,
}

impl InvestmentsStandardArgs {
    /// The usual defaults, with only `body_set` required.
    pub fn new(body_set: PdfLineSelection) -> Self {
        Self {
            body_set,
            deselection_list: Vec::new(),
            table: TableSettings::default(),
            company_index: None,
        }
    }
}

impl PdfExtractInvestmentsStandard {
    pub fn new(args: InvestmentsStandardArgs) -> Self {
        let InvestmentsStandardArgs { body_set, deselection_list, table, company_index } = args;
        Self { body_set, deselection_list, table, company_index }
    }

    /// The table settings this pipe was built with.
    pub fn table(&self) -> &TableSettings {
        &self.table
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let mut set = contextualized(&self.body_set, &page.lines);
        for deselection in &self.deselection_list {
            set = set / contextualized(deselection, &page.lines);
        }
        let rows: Vec<PdfLine> = page.lines.iter().filter(|line| set.contains(line)).cloned().collect();
        if rows.is_empty() {
            tracing::debug!("investments body set selected no line, nothing to tabularize");
            return Ok(Vec::new());
        }

        let coords = get_table_coordinates_from_lines(&rows, &self.table.config(self.company_index))?;
        tracing::debug!(
            found = %rows.first().map(|line| line.text().clone()).unwrap_or_default(),
            lines = rows.len(),
            "investments table body selected"
        );

        // `is-max-width` used to be a third key here — whether the line was as wide as the widest
        // of the table — and **nothing ever read it**: the one consumer went away with the `split`
        // loop that `P8` removed, and the field stayed because removing it rewrites eighty page
        // fixtures. It is gone, and so is the width computation that fed it.
        let block = |(table_row, table_col): (usize, usize), lines: &[&PdfLine]| {
            let metadata = BTreeMap::from([
                ("table-row".to_string(), BlockValue::from(table_row as i64)),
                ("table-col".to_string(), BlockValue::from(table_col as i64)),
            ]);
            PdfBlock::new(
                BlockType::TABLE_BODY,
                metadata,
                BlockValue::from(concat_in_reading_order(lines).as_str()),
            )
        };

        if !self.collapse_into_cells() {
            return Ok(rows.iter().enumerate().map(|(i, row)| block(coords[i], &[row])).collect());
        }

        // One block per **cell**. Which lines share a cell is what `collapse` decided; the order
        // inside the cell is the page's own, which is what `join_in_reading_order` reads.
        //
        // The cells come out in the order their **first** line appears, so a table that was
        // already one line per cell comes out exactly as it did before.
        let mut cells: Vec<((usize, usize), Vec<&PdfLine>)> = Vec::new();
        let mut seen: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for (i, row) in rows.iter().enumerate() {
            match seen.get(&coords[i]) {
                Some(&at) => cells[at].1.push(row),
                None => {
                    seen.insert(coords[i], cells.len());
                    cells.push((coords[i], vec![row]));
                }
            }
        }
        // The rows are **renumbered** to be contiguous from zero, because a run that collapsed
        // onto its last row leaves its first row with no cell at all and the consumer of this
        // metadata assumes contiguity: `PdfBlocksTable` builds its grid from the row values it
        // finds and then indexes it by the number a block carries, so a gap would make the two
        // disagree by one and every field of that row would be read from its neighbour.
        //
        // It is the same bookkeeping the mechanism this replaces did when it removed a block and
        // left a row empty — kept here, where the rows are decided.
        let renumbered: BTreeMap<usize, usize> = cells
            .iter()
            .map(|((row, _), _)| *row)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .enumerate()
            .map(|(new, old)| (old, new))
            .collect();
        tracing::debug!(
            cells = cells.len(),
            lines = rows.len(),
            table_rows = renumbered.len(),
            "investments table collapsed into cells"
        );
        Ok(cells
            .into_iter()
            .map(|((row, col), lines)| block((renumbered[&row], col), &lines))
            .collect())
    }

    /// Whether this pipe emits one block per cell instead of one per line.
    ///
    /// It is `collapse` and nothing else: the flag means *rows broken across several lines are put
    /// back together*, and a cell whose coordinates were merged while its text stayed in two blocks
    /// would be half-collapsed — occupied yes, readable no, which is the state the old mechanism in
    /// the `text_filter` existed to repair.
    fn collapse_into_cells(&self) -> bool {
        self.table.collapse
    }
}

impl PdfExtractPipe for PdfExtractInvestmentsStandard {
    fn name(&self) -> &str {
        "PdfExtractInvestmentsStandard"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}


// ----------------------------------------------------------------------------------------------
// PdfExtractAssetsStandard
// ----------------------------------------------------------------------------------------------
/// A range from zero to `len` with the given step, restricted to what is needed here: a zero step
/// is an error, and a negative step yields an empty sequence.
fn range_0_to_len_step(len: usize, step: i64) -> Result<Vec<usize>, PdfExtractStandardFuncsError> {
    if step == 0 {
        return Err(PdfExtractStandardFuncsError::ZeroSkipColumn);
    }
    if step < 0 {
        return Ok(Vec::new());
    }
    Ok((0..len).step_by(step as usize).collect())
}

/// The text of several lines joined in reading order: rows top to bottom, and left to right inside
/// a row. A single line is returned as it stands.
///
/// Two lines belong to the same row when their vertical extents overlap, which is what makes this
/// right for both shapes a wrapped value takes: FINECO-EN23@LUX p31 spells its date as `D` ·
/// `ECEMBER` · ` 31, 2024`, three fragments side by side whose boxes overlap vertically because
/// they are two different sizes of the same word; a value wrapped onto the next line does not
/// overlap, and stays in the order it is read.
fn join_in_reading_order(lines: &[&PdfLine]) -> String {
    // One line is not a join: its text goes through untouched, trailing spaces included. Trimming
    // it would change what every format that already reads a date gets, for no reason — the
    // trimming below exists so that two fragments do not meet with three spaces between them.
    if let [only] = lines {
        return only.text().clone();
    }
    in_reading_order(lines).iter().map(|line| line.text().trim()).collect::<Vec<_>>().join(" ").trim().to_string()
}

/// The same lines, sorted: rows top to bottom, and left to right inside a row.
///
/// Two lines belong to the same row when their vertical extents overlap. Written once because the
/// two joins below differ only in what they put between the pieces.
fn in_reading_order<'a>(lines: &[&'a PdfLine]) -> Vec<&'a PdfLine> {
    let mut ordered: Vec<&'a PdfLine> = lines.to_vec();
    ordered.sort_by(|a, b| {
        let (a_box, b_box) = (a.bbox().as_tuple(), b.bbox().as_tuple());
        let same_row = a_box.1 < b_box.3 && b_box.1 < a_box.3;
        if same_row {
            a_box.0.partial_cmp(&b_box.0).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            a_box.1.partial_cmp(&b_box.1).unwrap_or(std::cmp::Ordering::Equal)
        }
    });
    ordered
}

/// The text of the lines of one collapsed cell, concatenated with **nothing** between them.
///
/// Not a space, and that is the measured choice. A PDF generator breaking a line at a word boundary
/// leaves the space at the **end of the first piece**, and breaking it inside a word leaves none:
/// `Acme Asset Managemen` + `t Company` is one word split in two, and anything inserted between
/// them invents a space the page does not have. The page already says which case it is, so the
/// faithful join is no join at all — which is also, exactly, what the mechanism this replaces did.
fn concat_in_reading_order(lines: &[&PdfLine]) -> String {
    if let [only] = lines {
        return only.text().clone();
    }
    in_reading_order(lines).iter().map(|line| line.text().as_str()).collect::<Vec<_>>().concat()
}

/// The band a line belongs to: the one it overlaps most, or the nearest when it overlaps none.
///
/// The fallback is not a concession. A number is set right-aligned under a heading that is often
/// narrower than the column it titles, so a cell can sit entirely beside its own band — which is
/// why «the nearest» and not «none».
fn band_of(line: &PdfLine, bands: &[(f32, f32)]) -> Option<usize> {
    let (x0, _, x1, _) = line.bbox().as_tuple();
    let overlap = |band: &(f32, f32)| (x1.min(band.1) - x0.max(band.0)).max(0.0);
    let widest = bands
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| overlap(a).total_cmp(&overlap(b)))
        .filter(|(_, band)| overlap(band) > 0.0);
    if let Some((index, _)) = widest {
        return Some(index);
    }
    let distance = |band: &(f32, f32)| if x1 < band.0 { band.0 - x1 } else { x0 - band.1 };
    bands.iter().enumerate().min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b))).map(|(i, _)| i)
}

/// The values a single line of a value window carries, told apart by the run of spaces between
/// them.
///
/// **Two** spaces and not one, because a number never holds two in a row while the gap between two
/// columns always does: the narrowest gap measured on MEDIOLANUM-EN24 p6 is five spaces, and the
/// widest run inside a number is none. Empty pieces go, so the leading and trailing padding of the
/// object — which is what makes the object as wide as the four columns it spans — costs nothing.
///
/// A line carrying one value comes back as that one value, so a window of ordinary cells reads the
/// same with the flag on as with it off. Only the length of the list changes, which is what pairing
/// by index counts.
fn values_in_a_line(text: &str) -> Vec<String> {
    text.split("  ").map(str::trim).filter(|piece| !piece.is_empty()).map(str::to_string).collect()
}

/// Whether a line stands under one of the columns a format asked to leave out.
fn falls_in(line: &PdfLine, bands: &[(f32, f32)]) -> bool {
    let (x0, _, x1, _) = line.bbox().as_tuple();
    let centre = (x0 + x1) / 2.0;
    bands.iter().any(|(left, right)| centre >= *left && centre <= *right)
}

/// How a number is matched to the fund it belongs to.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum AssetsPairing {
    /// By position in the list: the *n*-th number of each label goes with the *n*-th fund.
    ///
    /// The historical behaviour, and the default, because changing it would move the output of
    /// seven formats at once. It holds only while every column has a cell on every one of the
    /// three rows: a single missing cell shifts everything after it, and the lengths match again
    /// once the extra entries are dropped, so nothing says a word.
    #[default]
    ByIndex,
    /// By abscissa: a number goes with the fund whose column it sits under.
    ///
    /// A missing cell then stays a missing cell. It is the rule `CLUSTERS` §4.1 sets for the whole
    /// family — *columns are discovered, and label ↔ value pairing is done by comparing abscissae,
    /// never by column index*.
    ByAbscissa,
}

/// One of the three numeric columns of an assets block, with the multiplier of the search window
/// that locates it from its own label.
#[derive(Clone)]
pub struct AssetsColumn {
    /// The selection of the label the search window starts from.
    pub anchor: PdfLineSelection,
    /// The window's `(dx, dy)` offset from the anchor, in multiples of the anchor's bounding box.
    pub vector: (f32, f32),
    /// The window's width multiplier.
    pub width: f32,
    /// The window's height multiplier.
    pub height: f32,
}

impl AssetsColumn {
    /// The usual defaults: an offset of `(1.2, 0.0)` and multipliers of `(100.0, 1.3)`.
    pub fn new(anchor: PdfLineSelection) -> Self {
        Self { anchor, vector: (1.2, 0.0), width: 100.0, height: 1.3 }
    }
}

/// Extracts the fund-assets blocks: total assets, liabilities, net assets, plus the fund, its
/// currency and possibly a date.
///
/// Two modes:
///
/// - `table_condition: false` — one fund/currency pair per page, extracted with a selection that fails the page if it is missing;
/// - `table_condition: true` — the page holds a table with one fund per column: the names are recomposed column by column, and the currency is either a single one for all of them, where a dedicated selection exists, or the last word of each fund's name.
pub struct PdfExtractAssetsStandard {
    fund_set: PdfLineSelection,
    currency_set: Option<PdfLineSelection>,
    date: Vec<PdfLineSelection>,
    tot_assets: AssetsColumn,
    liabilities: AssetsColumn,
    net_assets: AssetsColumn,
    table_condition: bool,
    skip_column: i64,
    pairing: AssetsPairing,
    drop: Option<PdfLineSelection>,
    drop_funds: Option<PdfLineSelection>,
    table: TableSettings,
    values_in_one_line: bool,
}

/// The construction parameters of [`PdfExtractAssetsStandard`], grouped into a struct so the call
/// site names what it sets rather than passing fourteen positional arguments.
pub struct AssetsStandardArgs {
    pub fund_set: PdfLineSelection,
    pub currency_set: Option<PdfLineSelection>,
    pub net_assets: AssetsColumn,
    pub liabilities: AssetsColumn,
    pub tot_assets: AssetsColumn,
    /// The date, as **candidates tried in order**: the first one that takes any line wins.
    ///
    /// An order of preference is not expressible with the selection algebra, because a union is
    /// unordered and resolves to whichever line comes first **on the page** — on CARNE-EN23 p19
    /// that is the page header, while `R5` wants the one above the column, which is lower down.
    /// A list says which is preferred without saying where it sits.
    ///
    /// Each candidate **joins** every line it takes, in reading order: FINECO-EN23@LUX p31 writes
    /// its date as three fragments in two different sizes.
    pub date: Vec<PdfLineSelection>,
    pub table_condition: bool,
    pub skip_column: i64,
    /// How a number is matched to its fund. See [`AssetsPairing`].
    pub pairing: AssetsPairing,
    /// The header of the **number** columns to leave out, if any. See
    /// [`PdfExtractAssetsStandard::dropped_bands`].
    pub drop: Option<PdfLineSelection>,
    /// A **fund** column to leave out whole, named by any of its own name lines.
    ///
    /// Separate from [`Self::drop`] and not a widening of it, because the two select different
    /// things and a format needs one or the other. `drop` names the header of a column of numbers —
    /// on AMUNDI-EN24 a **date**, `30/06/2023`, over the previous financial year's figures — and the
    /// fund it belongs to is a fund that must stay. This one names a column that is not a fund at
    /// all: FIDEURAM-EN23 p10 carries a `COMBINED STATEMENT OF NET ASSETS` column, which is the
    /// umbrella's total.
    ///
    /// Dropping it from the **numbers** alone was not enough, and that is what made this parameter
    /// necessary: the column stayed in the list of names, so the statement reported a fund with none
    /// of its three totals — true to the letter, and a warning on every run. The whole column goes:
    /// its name, and the numbers standing in its band.
    ///
    /// Subtracting its area in the fund selection instead **cannot** work, and was measured: on the
    /// pages that do not carry that line the anchor does not resolve, a bound that cannot resolve
    /// widens its side, and the subtraction takes the entire header with it — seven sub-funds down
    /// to one.
    pub drop_funds: Option<PdfLineSelection>,
    /// How the **fund columns** are grouped, in the `table_condition: true` mode. The numeric
    /// columns do not pass through the tabularizer at all: each is a window hung off its own
    /// anchor, so nothing here touches them.
    pub table: TableSettings,
    /// Whether **one** line of a value window may carry **several** values, told apart by the run
    /// of spaces between them.
    ///
    /// Off by default, and only [`AssetsPairing::ByIndex`] reads it: pairing by abscissa matches a
    /// line's box against a fund's band, and a box holding four numbers spans all four bands at
    /// once, so there is nothing for it to decide.
    ///
    /// MEDIOLANUM-EN24 is why it exists. Its `CONDENSED STATEMENT OF FINANCIAL POSITION` prints
    /// the ordinary rows as one text object per cell, but the three **total** rows are typeset
    /// differently: `TOTAL ASSETS` puts all four of its numbers in a single object ninety-five
    /// characters wide, and the net-assets row splits the same four across three objects, one plus
    /// one plus two. Counting the objects therefore says nothing, and a window that finds one
    /// value where the columns expect four leaves three sub-funds out of four without their totals.
    ///
    /// The pieces are matched to the columns **by index** and not by abscissa, which is a measured
    /// choice and not a convenience: the characters of Times 8.04 are not of one width — a digit is
    /// 4.02 points and a space 2.01 — so cutting the box up in proportion to the characters is
    /// wrong by as much as sixteen points on a column seventy-one wide. The index is not wrong.
    pub values_in_one_line: bool,
}

impl AssetsStandardArgs {
    /// The five required selections, with the usual defaults for the rest.
    pub fn new(
        fund_set: PdfLineSelection,
        currency_set: Option<PdfLineSelection>,
        net_assets: AssetsColumn,
        liabilities: AssetsColumn,
        tot_assets: AssetsColumn,
    ) -> Self {
        Self {
            fund_set,
            currency_set,
            net_assets,
            liabilities,
            tot_assets,
            date: Vec::new(),
            table_condition: false,
            skip_column: 1,
            pairing: AssetsPairing::default(),
            drop: None,
            drop_funds: None,
            table: TableSettings::assets_funds(),
            values_in_one_line: false,
        }
    }
}

/// One fund column of `A4`'s tabular branch. See [`PdfExtractAssetsStandard::fund_columns`].
struct FundColumn {
    /// The column's own lines, joined — the name the rows under it are attributed to.
    text: String,
    /// The abscissae its name lines span, which is what a number is matched against.
    band: (f32, f32),
    /// Whether `drop_funds` asks for it to go. Carried rather than filtered out at once, because
    /// its band is still needed: the numbers standing under it have to go with it.
    dropped: bool,
}

impl PdfExtractAssetsStandard {
    /// Builds the pipe.
    ///
    /// # Errors
    ///
    /// If `table_condition` is `false` and the currency selection is missing: without it the
    /// non-tabular branch would have nothing to derive a currency from. Catching it here means a
    /// misconfigured format fails when it is loaded rather than in the middle of a run.
    pub fn build(args: AssetsStandardArgs) -> Result<Self, PdfExtractStandardFuncsError> {
        let AssetsStandardArgs {
            fund_set,
            currency_set,
            net_assets,
            liabilities,
            tot_assets,
            date,
            table_condition,
            skip_column,
            pairing,
            drop,
            drop_funds,
            table,
            values_in_one_line,
        } = args;
        if !table_condition && currency_set.is_none() {
            return Err(PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { name: "currency".to_string() });
        }
        Ok(Self {
            fund_set,
            currency_set,
            date,
            tot_assets,
            liabilities,
            net_assets,
            table_condition,
            skip_column,
            pairing,
            drop,
            drop_funds,
            table,
            values_in_one_line,
        })
    }

    /// The page's lines minus the whitespace-only ones.
    fn meaningful_lines(page: &Page) -> Vec<PdfLine> {
        let set = PdfLineSet::select_text("") / PdfLineSet::select_text("^ $");
        page.lines.iter().filter(|line| set.contains(line)).cloned().collect()
    }

    /// The lines falling inside the moving window anchored to `column.anchor`.
    fn select_column<'a>(column: &AssetsColumn, lines: &'a [PdfLine]) -> Vec<&'a PdfLine> {
        let leaf = RelativeSelectPdfLineSet::area_from_movewindow(
            column.anchor.clone(),
            column.vector,
            column.width,
            column.height,
        );
        let selection: PdfLineSelection =
            OptionallyRelative::Relative(RelativePdfLineSet::from_leaf(OptionallyRelative::Relative(leaf)));
        select(&selection, lines)
    }

    /// The fund columns of the tabular branch: what each one's lines spell, the band it occupies,
    /// and whether `drop_funds` asks for it to be left out.
    ///
    /// One reader and not two, because a column's **name** and its **band** are the same grouping
    /// seen twice: the texts are what the rows are attributed to, the bands are what pairing by
    /// abscissa matches numbers against, and a column dropped from one and kept in the other is a
    /// statement whose numbers belong to the wrong funds. They were two functions doing the same
    /// grouping until a format needed to drop a column from both.
    fn fund_columns(&self, lines: &[PdfLine]) -> Result<Vec<FundColumn>, PdfExtractStandardFuncsError> {
        let funds: Vec<PdfLine> = select(&self.fund_set, lines).into_iter().cloned().collect();
        if funds.is_empty() {
            return Ok(Vec::new());
        }
        let coords = get_table_coordinates_from_lines(&funds, &self.table.config(None))?;
        let cols: Vec<usize> = coords.iter().map(|(_, col)| *col).collect();
        let n_cols = cols.iter().copied().max().map(|m| m + 1).unwrap_or(0);
        let unwanted = self.drop_funds.as_ref().map(|selection| contextualized(selection, lines));
        Ok((0..n_cols)
            .map(|col| {
                let members: Vec<&PdfLine> =
                    funds.iter().zip(cols.iter()).filter(|(_, c)| **c == col).map(|(line, _)| line).collect();
                let text = members.iter().map(|line| line.text().trim()).collect::<Vec<_>>().join(" ");
                let band = members.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(left, right), line| {
                    let (x0, _, x1, _) = line.bbox().as_tuple();
                    (left.min(x0), right.max(x1))
                });
                let dropped = unwanted
                    .as_ref()
                    .is_some_and(|set| members.iter().any(|line| set.contains(line)));
                if dropped {
                    // Named, because a format that drops the wrong column loses a real fund and the
                    // only way to tell is to read which one went.
                    tracing::debug!(column = %text, "fund column dropped: drop_funds names it");
                }
                FundColumn { text, band, dropped }
            })
            .collect())
    }

    /// The bands of the columns a format asks to leave out (`drop`).
    ///
    /// AMUNDI-EN24 p360 prints each fund twice, this financial year and the previous one, and only
    /// one of the two is the datum. The selection names the **header** of the columns to leave out
    /// — `30/06/2023`, say — and every number standing under one of them is dropped with it.
    fn dropped_bands(&self, lines: &[PdfLine]) -> Vec<(f32, f32)> {
        let Some(selection) = &self.drop else { return Vec::new() };
        let set = contextualized(selection, lines);
        lines
            .iter()
            .filter(|line| set.contains(line))
            .map(|line| {
                let (x0, _, x1, _) = line.bbox().as_tuple();
                (x0, x1)
            })
            .collect()
    }

    /// Splits the last word off each fund name and takes it as the currency: the fallback of the
    /// tabular branch when there is no dedicated currency selection.
    fn split_trailing_currencies(
        fund_texts: Vec<String>,
    ) -> Result<(Vec<String>, Vec<String>), PdfExtractStandardFuncsError> {
        let mut funds = Vec::with_capacity(fund_texts.len());
        let mut currencies = Vec::with_capacity(fund_texts.len());
        for text in fund_texts {
            let parts: Vec<&str> = text.split_whitespace().collect();
            let Some((currency, name)) = parts.split_last() else {
                return Err(PdfExtractStandardFuncsError::MissingCurrencyToken { column: text });
            };
            funds.push(name.join(" "));
            currencies.push((*currency).to_string());
        }
        Ok((funds, currencies))
    }

    /// A column's values, at only the indices `skip_column` asks for.
    ///
    /// Takes the values already read off the lines rather than the lines themselves, because how
    /// many values a line carries is [`Self::column_values`]'s business and not this one's.
    fn column_texts(
        items: &[String],
        indices: &[usize],
        column: &str,
    ) -> Result<Vec<String>, PdfExtractStandardFuncsError> {
        indices
            .iter()
            .map(|&i| {
                items
                    .get(i)
                    .cloned()
                    .ok_or_else(|| PdfExtractStandardFuncsError::MismatchedAssetsColumn {
                        column: column.to_string(),
                        found: items.len(),
                        expected: i + 1,
                    })
            })
            .collect()
    }

    /// The values a window's lines carry, in the order the lines come in.
    ///
    /// One value per line is the rule, and the line's text goes through untouched — trailing
    /// spaces included, because that is what every format reading this pipe has always been given
    /// and the casts trim for themselves. With `values_in_one_line` a line may carry several, and
    /// then each piece **is** trimmed: a piece is not a line, nothing has seen it before, and
    /// leaving the gap it was cut from attached to it would serve no one. See
    /// [`AssetsStandardArgs::values_in_one_line`].
    fn column_values(&self, items: &[&PdfLine]) -> Vec<String> {
        if !self.values_in_one_line {
            return items.iter().map(|line| line.text().clone()).collect();
        }
        items.iter().flat_map(|line| values_in_a_line(line.text())).collect()
    }

    /// One value per fund column, matched by abscissa instead of by position.
    ///
    /// A column that takes nothing under a given fund leaves a hole rather than shifting every
    /// value after it, which is the whole point: the shift is silent, the hole is not.
    fn by_abscissa(items: &[&PdfLine], bands: &[(f32, f32)], column: &str) -> Vec<Option<String>> {
        let mut out: Vec<Option<String>> = vec![None; bands.len()];
        for item in items {
            let Some(index) = band_of(item, bands) else { continue };
            if out[index].is_some() {
                tracing::debug!(
                    column,
                    fund_column = index,
                    "a second value falls under the same fund column - the first one is kept"
                );
                continue;
            }
            out[index] = Some(item.text().clone());
        }
        out
    }

    /// The currency of **each** fund column of a statement.
    ///
    /// A statement may price its sub-funds in different currencies on one page — FIDEURAM-EN23
    /// prints the first column in CHF and the other three in EUR, AMUNDI-EN24 two columns in USD
    /// and two in EUR — so the currency is a datum of the column, not of the page. It used to be
    /// read as a datum of the page, with the **first** matching line copied onto every fund, and
    /// those four FIDEURAM sub-funds all came out in CHF.
    ///
    /// One matching line still means one currency for the whole statement, which is what the
    /// formats reading a single `(valori espressi in euro)` line say and must keep saying. Only a
    /// selection that matches several lines is paired, and it is paired the way the three amounts
    /// are: by the fund column the line sits under.
    fn currency_texts_by_column(
        &self,
        currency_selection: &PdfLineSelection,
        lines: &[PdfLine],
        bands: &[(f32, f32)],
        gone: &[(f32, f32)],
        funds: usize,
    ) -> Result<Vec<String>, PdfExtractStandardFuncsError> {
        let currency_set = contextualized(currency_selection, lines);
        // A currency standing over a column `drop_funds` took away is **not** a currency of this
        // statement, and it must not be left in: `band_of` attributes a line with no overlap to the
        // **nearest** band — the rule that lets a number sit outside the column it belongs to — so a
        // leftover would be claimed by whichever surviving column is closest, and, coming first in
        // the page, would keep the place of the real one.
        let taken: Vec<&PdfLine> =
            lines.iter().filter(|line| currency_set.contains(line) && !falls_in(line, gone)).collect();
        let [first, rest @ ..] = taken.as_slice() else {
            // The same failure as before: a statement whose currency the format says is on the
            // page, and is not.
            return Ok(vec![select_expected_text(&currency_set, lines, "currency")?; funds]);
        };
        if rest.is_empty() {
            return Ok(vec![first.text().clone(); funds]);
        }

        let by_column = Self::by_abscissa(&taken, bands, "currency");
        let fallback = first.text().clone();
        Ok((0..funds)
            .map(|index| match by_column.get(index).and_then(Option::as_ref) {
                Some(text) => text.clone(),
                None => {
                    // `R3`: a value nobody read on this column is still a value, and it says so.
                    tracing::warn!(
                        fund_column = index,
                        currencies = taken.len(),
                        imposed = %fallback,
                        "this statement prices its columns in different currencies and this column \
                         has none of its own: the first one is imposed"
                    );
                    fallback.clone()
                }
            })
            .collect())
    }

    /// The date of the statement, from the first candidate that takes anything.
    ///
    /// No candidate at all means the format does not read a date, and the value is simply null.
    /// Candidates that all come up empty is a different thing — the format says the date is on the
    /// page and it was not found — so it says so and still yields the row: `R25`, what is seen but
    /// not read leaves a warning, not a lost page. Before, a selection matching nothing cost the
    /// **whole page**, every other fund of the statement included.
    fn date_value(&self, lines: &[PdfLine]) -> BlockValue {
        if self.date.is_empty() {
            return BlockValue::Null;
        }
        for candidate in &self.date {
            let set = contextualized(candidate, lines);
            let taken: Vec<&PdfLine> = lines.iter().filter(|line| set.contains(line)).collect();
            if taken.is_empty() {
                continue;
            }
            return BlockValue::from(join_in_reading_order(&taken));
        }
        tracing::warn!(
            candidates = self.date.len(),
            "the date of this statement is declared but no candidate matches - the row keeps no date"
        );
        BlockValue::Null
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let lines = Self::meaningful_lines(page);

        // The fund columns first, because a column `drop_funds` takes away also takes away the
        // numbers standing in its band — otherwise pairing by index would shift every fund after it
        // onto its neighbour's figures.
        let columns = if self.table_condition { self.fund_columns(&lines)? } else { Vec::new() };
        let kept: Vec<&FundColumn> = columns.iter().filter(|column| !column.dropped).collect();
        // The bands of the columns that are not funds at all. Everything standing in one of them is
        // gone, the currency included — which is where the two drops differ, and why they are two
        // parameters: `drop` says *these numbers are not data* and leaves the column a fund, so a
        // currency printed over it still describes a fund that stays.
        let gone: Vec<(f32, f32)> =
            columns.iter().filter(|column| column.dropped).map(|column| column.band).collect();
        let dropped: Vec<(f32, f32)> =
            self.dropped_bands(&lines).into_iter().chain(gone.iter().copied()).collect();
        let take = |column: &AssetsColumn| -> Vec<&PdfLine> {
            Self::select_column(column, &lines).into_iter().filter(|line| !falls_in(line, &dropped)).collect()
        };
        let tot_assets_items = take(&self.tot_assets);
        let liabilities_items = take(&self.liabilities);
        let net_assets_items = take(&self.net_assets);

        let (fund_texts, currency_texts) = if !self.table_condition {
            let fund_set = contextualized(&self.fund_set, &lines);
            let fund = select_expected_text(&fund_set, &lines, "fund")?;
            // The constructor guarantees the currency selection is present in this branch.
            let currency_selection =
                self.currency_set.as_ref().expect("build rejects a missing currency_set when table_condition is false");
            let currency_set = contextualized(currency_selection, &lines);
            let currency = select_expected_text(&currency_set, &lines, "currency")?;
            (vec![fund], vec![currency])
        } else {
            let fund_texts: Vec<String> = kept.iter().map(|column| column.text.clone()).collect();
            match &self.currency_set {
                Some(currency_selection) => {
                    // The **kept** columns' bands, which is why they are not recomputed here: a
                    // currency matched against a column the names no longer have would land one
                    // fund over.
                    let bands: Vec<(f32, f32)> = kept.iter().map(|column| column.band).collect();
                    let currencies = self.currency_texts_by_column(
                        currency_selection,
                        &lines,
                        &bands,
                        &gone,
                        fund_texts.len(),
                    )?;
                    (fund_texts, currencies)
                }
                None => Self::split_trailing_currencies(fund_texts)?,
            }
        };

        let date = self.date_value(&lines);

        let amounts = match self.pairing {
            AssetsPairing::ByIndex => {
                let tot_assets_values = self.column_values(&tot_assets_items);
                let liabilities_values = self.column_values(&liabilities_items);
                let net_assets_values = self.column_values(&net_assets_items);
                let indices = range_0_to_len_step(tot_assets_values.len(), self.skip_column)?;
                let tot_assets_text = Self::column_texts(&tot_assets_values, &indices, "tot_assets")?;
                let liabilities_text = Self::column_texts(&liabilities_values, &indices, "liabilities")?;
                let net_assets_text = Self::column_texts(&net_assets_values, &indices, "net_assets")?;
                let n_out = [
                    fund_texts.len(),
                    currency_texts.len(),
                    tot_assets_text.len(),
                    liabilities_text.len(),
                    net_assets_text.len(),
                ]
                .into_iter()
                .min()
                .unwrap_or(0);
                if fund_texts.len() != n_out
                    || currency_texts.len() != n_out
                    || tot_assets_text.len() != n_out
                    || liabilities_text.len() != n_out
                    || net_assets_text.len() != n_out
                {
                    tracing::warn!(
                        funds = fund_texts.len(),
                        currencies = currency_texts.len(),
                        tot_assets = tot_assets_text.len(),
                        liabilities = liabilities_text.len(),
                        net_assets = net_assets_text.len(),
                        kept = n_out,
                        "assets columns have mismatched lengths - extra entries dropped"
                    );
                }
                (0..n_out)
                    .map(|i| {
                        (
                            i,
                            Some(tot_assets_text[i].clone()),
                            Some(liabilities_text[i].clone()),
                            Some(net_assets_text[i].clone()),
                        )
                    })
                    .collect::<Vec<_>>()
            }
            AssetsPairing::ByAbscissa => {
                let bands: Vec<(f32, f32)> = if self.table_condition {
                    kept.iter().map(|column| column.band).collect()
                } else {
                    // One fund, one band: everything the three windows take is its own.
                    vec![(f32::NEG_INFINITY, f32::INFINITY)]
                };
                let tot = Self::by_abscissa(&tot_assets_items, &bands, "tot_assets");
                let lia = Self::by_abscissa(&liabilities_items, &bands, "liabilities");
                let net = Self::by_abscissa(&net_assets_items, &bands, "net_assets");
                (0..fund_texts.len().min(bands.len()))
                    .map(|i| (i, tot[i].clone(), lia[i].clone(), net[i].clone()))
                    .collect::<Vec<_>>()
            }
        };

        let mut blocks = Vec::with_capacity(amounts.len());
        for (i, tot_assets, liabilities, net_assets) in amounts {
            let (Some(tot_assets), Some(liabilities), Some(net_assets)) = (tot_assets, liabilities, net_assets)
            else {
                // A hole rather than a shift: the fund is named, one of its three numbers is not
                // under its column, and saying so is the whole reason to pair by abscissa.
                tracing::warn!(
                    fund = fund_texts.get(i).map(String::as_str).unwrap_or(""),
                    "one of the three totals is missing under this fund's column - the row is left out"
                );
                continue;
            };
            let metadata = BTreeMap::from([
                ("fund".to_string(), BlockValue::from(fund_texts[i].as_str())),
                ("currency".to_string(), BlockValue::from(currency_texts[i].as_str())),
                ("tot_assets".to_string(), BlockValue::from(tot_assets.as_str())),
                ("liabilities".to_string(), BlockValue::from(liabilities.as_str())),
                ("net_assets".to_string(), BlockValue::from(net_assets.as_str())),
                ("date".to_string(), date.clone()),
            ]);
            blocks.push(PdfBlock::new(BlockType::RELEVANT_BLOCK, metadata, BlockValue::from("")));
        }
        tracing::debug!(entries = blocks.len(), table_condition = self.table_condition, "assets extracted");

        Ok(blocks)
    }
}

impl PdfExtractPipe for PdfExtractAssetsStandard {
    fn name(&self) -> &str {
        "PdfExtractAssetsStandard"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, bbox: (f32, f32, f32, f32)) -> PdfLine {
        PdfLine::new("Arial", 10.0, text, bbox)
    }

    fn page(lines: Vec<PdfLine>) -> Page {
        Page::new(1, (300.0, 300.0), lines, Vec::new())
    }

    /// An absolute text selection, by far the most common form in a formats repository.
    /// An absolute font selection, for the cases where several lines share a family.
    fn font_sel(font: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_font(font))
    }

    fn text_sel(pattern: &str) -> PdfLineSelection {
        OptionallyRelative::Absolute(PdfLineSet::select_text(pattern))
    }

    fn metadata_of(block: &PdfBlock, key: &str) -> BlockValue {
        block.metadata.get(key).cloned().unwrap_or(BlockValue::Null)
    }

    mod extract_text_pdf_block_or_fail_page {
        use super::*;

        #[test]
        fn builds_one_block_with_the_text_of_the_first_matching_line() {
            let pipe = ExtractTextPdfBlockOrFailPage::new(text_sel("^Alpha$"), "thing", BlockType::FUND_NAME);
            let blocks = pipe.call(&page(vec![line("Alpha", (0.0, 0.0, 10.0, 10.0))])).unwrap();
            assert_eq!(blocks, vec![PdfBlock::bare(BlockType::FUND_NAME, "Alpha")]);
        }

        #[test]
        fn fails_the_page_when_nothing_matches() {
            let pipe = ExtractTextPdfBlockOrFailPage::new(text_sel("^Nope$"), "thing", BlockType::FUND_NAME);
            let err = pipe.call(&page(vec![line("Alpha", (0.0, 0.0, 10.0, 10.0))])).unwrap_err();
            assert!(matches!(err, PdfExtractStandardFuncsError::Commons(CommonsError::PageParseFail { .. })));
        }

        #[test]
        fn a_page_failure_stays_non_fatal_once_translated_for_the_engine() {
            let pipe = ExtractTextPdfBlockOrFailPage::new(text_sel("^Nope$"), "thing", BlockType::FUND_NAME);
            let err = pipe.extract(&page(vec![line("Alpha", (0.0, 0.0, 10.0, 10.0))])).unwrap_err();
            assert!(err.is_page_failure());
        }

        #[test]
        fn the_pipe_name_is_the_field_name_it_extracts() {
            let pipe = ExtractTextPdfBlockOrFailPage::new(text_sel("^Alpha$"), "thing", BlockType::FUND_NAME);
            assert_eq!(PdfExtractPipe::name(&pipe), "thing");
        }

        mod factories {
            use super::*;

            #[test]
            fn fund_uses_the_fund_name_block_type_and_name() {
                let pipe = pdf_extract_fund_standard(text_sel("^X$"));
                assert_eq!(PdfExtractPipe::name(&pipe), "fund");
                let blocks = pipe.call(&page(vec![line("X", (0.0, 0.0, 10.0, 10.0))])).unwrap();
                assert_eq!(blocks[0].type_block, BlockType::FUND_NAME);
            }

            #[test]
            fn currency_uses_the_currency_statement_block_type_and_name() {
                let pipe = pdf_extract_currency_standard(text_sel("^X$"));
                assert_eq!(PdfExtractPipe::name(&pipe), "currency");
                let blocks = pipe.call(&page(vec![line("X", (0.0, 0.0, 10.0, 10.0))])).unwrap();
                assert_eq!(blocks[0].type_block, BlockType::CURRENCY_STATEMENT);
            }

            // La società di gestione non è più qui: dal 2026-09-19 il suo costruttore fabbrica
            // `PdfExtractLabelledValueStandard`, e i suoi test stanno in `labelled_value.rs`,
            // sottomodulo `the_managment_company_signpost`.
        }
    }

    mod page_classify {
        use super::*;

        fn sample() -> Page {
            page(vec![line("Header one", (0.0, 0.0, 60.0, 10.0)), line("Header two", (0.0, 20.0, 60.0, 30.0))])
        }

        #[test]
        fn declares_the_page_type_when_every_header_set_matches() {
            let pipe = PdfExtractPageClassifyStandard::new(vec![text_sel("Header one"), text_sel("Header two")], "investments");
            let blocks = pipe.call(&sample()).unwrap();
            assert_eq!(metadata_of(&blocks[0], "page_type"), BlockValue::from("investments"));
        }

        #[test]
        fn declares_nothing_when_one_header_set_is_missing() {
            let pipe = PdfExtractPageClassifyStandard::new(vec![text_sel("Header one"), text_sel("Header three")], "investments");
            let blocks = pipe.call(&sample()).unwrap();
            assert_eq!(metadata_of(&blocks[0], "page_type"), BlockValue::Null);
        }

        #[test]
        fn an_empty_list_of_header_sets_always_matches() {
            let pipe = PdfExtractPageClassifyStandard::new(Vec::new(), "anything");
            let blocks = pipe.call(&page(Vec::new())).unwrap();
            assert_eq!(metadata_of(&blocks[0], "page_type"), BlockValue::from("anything"));
        }

        #[test]
        fn always_emits_exactly_one_block_whatever_the_outcome() {
            for sets in [vec![text_sel("Header one")], vec![text_sel("absent")]] {
                let pipe = PdfExtractPageClassifyStandard::new(sets, "investments");
                assert_eq!(pipe.call(&sample()).unwrap().len(), 1);
            }
        }

        #[test]
        fn the_block_is_a_page_class_block_with_empty_content() {
            let pipe = PdfExtractPageClassifyStandard::new(vec![text_sel("Header one")], "investments");
            let blocks = pipe.call(&sample()).unwrap();
            assert_eq!(blocks[0].type_block, BlockType::PAGE_CLASS);
            assert_eq!(blocks[0].content, BlockValue::from(""));
        }

        #[test]
        fn an_empty_page_never_matches_a_non_empty_header_set() {
            let pipe = PdfExtractPageClassifyStandard::new(vec![text_sel("Header one")], "investments");
            let blocks = pipe.call(&page(Vec::new())).unwrap();
            assert_eq!(metadata_of(&blocks[0], "page_type"), BlockValue::Null);
        }
    }

    mod currency_constant {
        use super::*;

        #[test]
        fn emits_the_currency_code_regardless_of_the_page() {
            let pipe = PdfExtractCurrencyConstant::new(Currency::USD);
            for p in [page(Vec::new()), page(vec![line("noise", (0.0, 0.0, 10.0, 10.0))])] {
                let blocks = pipe.call(&p);
                assert_eq!(blocks, vec![PdfBlock::bare(BlockType::CURRENCY_STATEMENT, "USD")]);
            }
        }

        #[test]
        fn exposes_the_currency_it_was_built_with() {
            assert_eq!(PdfExtractCurrencyConstant::new(Currency::EUR).currency(), Currency::EUR);
        }

        #[test]
        fn never_fails() {
            assert!(PdfExtractCurrencyConstant::new(Currency::EUR).extract(&page(Vec::new())).is_ok());
        }

        /// R3: a value the engine decided instead of reading must not reach the output in silence.
        mod it_says_the_currency_was_not_read {
            use super::*;
            use crate::core::tracing_setup::capture::{events, one_event};
            use pretty_assertions::assert_eq;

            #[test]
            fn the_first_page_carries_the_warning() {
                let record = one_event(|| {
                    PdfExtractCurrencyConstant::new(Currency::EUR).call(&page(Vec::new()));
                });
                assert_eq!(record.level, Some(tracing::Level::WARN));
                assert!(
                    record.message.starts_with("currency was not read: imposed as \"EUR\""),
                    "found {:?}",
                    record.message
                );
            }

            /// The column that says which field a row is about — not a free field of its own.
            #[test]
            fn the_warning_names_the_field_in_its_own_column() {
                let record = one_event(|| {
                    PdfExtractCurrencyConstant::new(Currency::EUR).call(&page(Vec::new()));
                });
                assert_eq!(record.field("coord_ref_2"), Some("currency"));
            }

            /// The fact is about the format, not about the page: forty holdings pages must not
            /// leave forty identical rows.
            #[test]
            fn it_warns_once_however_many_pages_the_pipe_runs_on() {
                let pipe = PdfExtractCurrencyConstant::new(Currency::USD);
                let records = events(|| {
                    for _ in 0..40 {
                        pipe.call(&page(Vec::new()));
                    }
                });
                assert_eq!(records.len(), 1);
            }

            /// Each document of a job builds its own pipes, so each says it again.
            #[test]
            fn a_second_pipe_warns_again() {
                let records = events(|| {
                    PdfExtractCurrencyConstant::new(Currency::EUR).call(&page(Vec::new()));
                    PdfExtractCurrencyConstant::new(Currency::USD).call(&page(Vec::new()));
                });
                assert_eq!(records.len(), 2);
            }

            #[test]
            fn the_blocks_are_the_same_before_and_after_the_warning() {
                let pipe = PdfExtractCurrencyConstant::new(Currency::GBP);
                let first = pipe.call(&page(Vec::new()));
                let second = pipe.call(&page(Vec::new()));
                assert_eq!(first, second);
            }
        }
    }

    mod investments {
        use super::*;

        fn table_page() -> Page {
            page(vec![
                line("r0c0", (0.0, 0.0, 20.0, 10.0)),
                line("r0c1", (30.0, 0.0, 50.0, 10.0)),
                line("r1c0", (0.0, 20.0, 20.0, 30.0)),
                line("r1c1", (30.0, 20.0, 50.0, 30.0)),
            ])
        }

        fn body_pipe() -> PdfExtractInvestmentsStandard {
            PdfExtractInvestmentsStandard::new(InvestmentsStandardArgs::new(text_sel("")))
        }

        #[test]
        fn emits_one_table_body_block_per_selected_line() {
            let blocks = body_pipe().call(&table_page()).unwrap();
            assert_eq!(blocks.len(), 4);
            assert!(blocks.iter().all(|b| b.type_block == BlockType::TABLE_BODY));
        }

        #[test]
        fn the_content_of_each_block_is_the_text_of_its_line() {
            let blocks = body_pipe().call(&table_page()).unwrap();
            assert_eq!(blocks[0].content, BlockValue::from("r0c0"));
            assert_eq!(blocks[3].content, BlockValue::from("r1c1"));
        }

        #[test]
        fn each_block_carries_its_table_coordinates() {
            let blocks = body_pipe().call(&table_page()).unwrap();
            let coords: Vec<_> = blocks
                .iter()
                .map(|b| (metadata_of(b, "table-row"), metadata_of(b, "table-col")))
                .collect();
            assert_eq!(
                coords,
                vec![
                    (BlockValue::from(0i64), BlockValue::from(0i64)),
                    (BlockValue::from(0i64), BlockValue::from(1i64)),
                    (BlockValue::from(1i64), BlockValue::from(0i64)),
                    (BlockValue::from(1i64), BlockValue::from(1i64)),
                ]
            );
        }

        /// A block of the table carries its **two coordinates and nothing else**.
        ///
        /// It used to carry a third key, `is-max-width` — whether the line was as wide as the
        /// widest of the table — which nothing ever read: its one consumer went away with the
        /// `split` loop `P8` removed. This test is what keeps a fourth key from arriving without
        /// a reader.
        #[test]
        fn a_block_carries_its_two_coordinates_and_nothing_else() {
            let p = page(vec![line("narrow", (0.0, 0.0, 10.0, 10.0)), line("wide", (0.0, 20.0, 90.0, 30.0))]);
            for block in body_pipe().call(&p).unwrap() {
                assert_eq!(
                    block.metadata.keys().collect::<Vec<_>>(),
                    ["table-col", "table-row"],
                    "the metadata of a table block"
                );
            }
        }

        #[test]
        fn returns_nothing_when_the_body_set_selects_no_line() {
            let pipe = PdfExtractInvestmentsStandard::new(InvestmentsStandardArgs::new(text_sel("^absent$")));
            assert!(pipe.call(&table_page()).unwrap().is_empty());
        }

        /// `P8`: with `collapse` on, the pipe emits one block per **cell**, not per line.
        ///
        /// Which lines are one cell is `collapse`'s decision and nothing is added here — the pipe
        /// groups by the coordinates it comes back with and joins the text in reading order. The
        /// column that may collapse is the one `company_index` names, which is the configuration
        /// `get_table_coordinates_from_lines` already built for it.
        mod collapse_into_cells {
            use super::*;
            use pretty_assertions::assert_eq;

            /// A name wrapped onto a second line, with the values beside the first.
            ///
            /// The wrapped remainder is a line of its own in column zero: two consecutive entries
            /// of the same column, which is the run `collapse_table_rows_by_pattern` closes.
            ///
            /// The break falls **inside a word**, which is the case that says how the pieces meet:
            /// with nothing between them.
            fn wrapped_name_page() -> Page {
                page(vec![
                    line("Acme Asset Managemen", (0.0, 0.0, 90.0, 10.0)),
                    line("t Company", (0.0, 11.0, 40.0, 21.0)),
                    line("100", (120.0, 11.0, 140.0, 21.0)),
                    line("Beta Fund", (0.0, 40.0, 50.0, 50.0)),
                    line("200", (120.0, 40.0, 140.0, 50.0)),
                ])
            }

            fn pipe(collapse: bool, direction: SplittingDirection) -> PdfExtractInvestmentsStandard {
                PdfExtractInvestmentsStandard::new(InvestmentsStandardArgs {
                    company_index: Some(0),
                    table: TableSettings {
                        collapse,
                        collapse_direction: direction,
                        collapse_algorithm: CollapseAlgorithm::Pattern,
                        ..TableSettings::default()
                    },
                    ..InvestmentsStandardArgs::new(text_sel(""))
                })
            }

            fn contents(blocks: &[PdfBlock]) -> Vec<String> {
                blocks.iter().map(|b| b.content.as_str().unwrap_or_default().to_string()).collect()
            }

            #[test]
            fn without_it_every_line_is_its_own_block() {
                let blocks = pipe(false, SplittingDirection::Down).call(&wrapped_name_page()).unwrap();
                assert_eq!(blocks.len(), 5);
                assert_eq!(contents(&blocks)[0], "Acme Asset Managemen");
                assert_eq!(contents(&blocks)[1], "t Company");
            }

            #[test]
            fn with_it_the_two_halves_are_one_block() {
                let blocks = pipe(true, SplittingDirection::Down).call(&wrapped_name_page()).unwrap();
                assert_eq!(blocks.len(), 4, "five lines, four cells");
                assert_eq!(
                    contents(&blocks)[0],
                    "Acme Asset Management Company",
                    "the page broke the word, so nothing goes between the pieces"
                );
            }

            /// The cells come out in the order their **first** line appears, so a table that was
            /// already one line per cell is untouched.
            #[test]
            fn a_table_with_no_wrapped_cell_comes_out_exactly_as_before() {
                let without = pipe(false, SplittingDirection::Down).call(&table_page()).unwrap();
                let with = pipe(true, SplittingDirection::Down).call(&table_page()).unwrap();
                assert_eq!(without, with);
            }

            /// The direction decides **which row of the run** the joined cell lands on, and that is
            /// what puts it beside the values that belong to it.
            ///
            /// On this page the values sit beside the name's **second** line, so `up` is the
            /// direction that makes one row of it: name and amount come out sharing a `table-row`,
            /// and with `down` they do not. It is the choice the old `merge_prev` made.
            #[test]
            fn the_direction_puts_the_cell_beside_its_own_values() {
                let row_of = |blocks: &[PdfBlock], at: usize| metadata_of(&blocks[at], "table-row");

                let up = pipe(true, SplittingDirection::Up).call(&wrapped_name_page()).unwrap();
                assert_eq!(contents(&up)[0], "Acme Asset Management Company");
                assert_eq!(contents(&up)[1], "100");
                assert_eq!(row_of(&up, 0), row_of(&up, 1), "the name and its amount are one row");

                let down = pipe(true, SplittingDirection::Down).call(&wrapped_name_page()).unwrap();
                assert_eq!(contents(&down)[0], contents(&up)[0], "the same text either way");
                assert_ne!(row_of(&down, 0), row_of(&down, 1));
            }

            /// The rows come out **contiguous from zero**, whichever way the run collapsed.
            ///
            /// A run that collapses onto its last row leaves its first row with no cell, and the
            /// consumer of this metadata assumes contiguity: it builds its grid from the rows it
            /// finds and then indexes it by the number a block carries. A gap would make the two
            /// disagree by one, and every field of that row would be read from its neighbour — the
            /// defect that cost CARNE-EN23 twenty-two rows before this was here.
            #[test]
            fn the_rows_are_renumbered_contiguously() {
                for direction in [SplittingDirection::Down, SplittingDirection::Up] {
                    let blocks = pipe(true, direction).call(&wrapped_name_page()).unwrap();
                    let mut rows: Vec<i64> = blocks
                        .iter()
                        .map(|b| match metadata_of(b, "table-row") {
                            BlockValue::Int(row) => row,
                            other => panic!("table-row should be an integer, found {other:?}"),
                        })
                        .collect();
                    rows.sort_unstable();
                    rows.dedup();
                    assert_eq!(rows, (0..rows.len() as i64).collect::<Vec<_>>(), "{direction:?}");
                }
            }

            /// A joined cell carries the same two keys as any other block.
            #[test]
            fn a_joined_cell_carries_its_two_coordinates() {
                let blocks = pipe(true, SplittingDirection::Down).call(&wrapped_name_page()).unwrap();
                assert_eq!(blocks[0].metadata.keys().collect::<Vec<_>>(), ["table-col", "table-row"]);
            }
        }

        #[test]
        fn returns_nothing_on_an_empty_page() {
            assert!(body_pipe().call(&page(Vec::new())).unwrap().is_empty());
        }

        #[test]
        fn the_deselection_list_removes_lines_from_the_body_set() {
            let args = InvestmentsStandardArgs {
                deselection_list: vec![text_sel("^r0c0$")],
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            let blocks = PdfExtractInvestmentsStandard::new(args).call(&table_page()).unwrap();
            assert_eq!(blocks.len(), 3);
            assert!(blocks.iter().all(|b| b.content != BlockValue::from("r0c0")));
        }

        #[test]
        fn several_deselections_are_applied_in_sequence() {
            let args = InvestmentsStandardArgs {
                deselection_list: vec![text_sel("^r0c0$"), text_sel("^r1c1$")],
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            let blocks = PdfExtractInvestmentsStandard::new(args).call(&table_page()).unwrap();
            assert_eq!(blocks.len(), 2);
        }

        #[test]
        fn deselecting_everything_yields_no_block_instead_of_failing() {
            let args = InvestmentsStandardArgs {
                deselection_list: vec![text_sel("")],
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            assert!(PdfExtractInvestmentsStandard::new(args).call(&table_page()).unwrap().is_empty());
        }

        /// The columns of `table_page` are centred 30 points apart, so a tolerance either side of
        /// 30 says whether the value reaches the tabularizer at all. It used to be stored and
        /// never read.
        fn columns_with_tolerance(col_tolerance: f32) -> Vec<BlockValue> {
            let args = InvestmentsStandardArgs {
                table: TableSettings {
                    col_tolerance,
                    tolerance_unit: TablePosMeasureUnit::Pt,
                    ..TableSettings::default()
                },
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            PdfExtractInvestmentsStandard::new(args)
                .call(&table_page())
                .unwrap()
                .iter()
                .map(|b| metadata_of(b, "table-col"))
                .collect()
        }

        #[test]
        fn a_tolerance_below_the_column_gap_leaves_the_columns_apart() {
            assert_eq!(
                columns_with_tolerance(5.0),
                vec![BlockValue::from(0i64), BlockValue::from(1i64), BlockValue::from(0i64), BlockValue::from(1i64)]
            );
        }

        #[test]
        fn a_tolerance_above_the_column_gap_merges_them_into_one() {
            assert!(
                columns_with_tolerance(40.0).iter().all(|col| *col == BlockValue::from(0i64)),
                "a tolerance wider than the gap must reach the tabularizer and collapse the columns"
            );
        }

        /// The rows of `table_page` are centred 20 points apart and its columns 30, so a column
        /// tolerance of 40 leaves the rows alone, because the two axes are separate values and
        /// neither stands in for the other.
        #[test]
        fn a_column_tolerance_joins_the_columns_and_leaves_the_rows_alone() {
            let args = InvestmentsStandardArgs {
                table: TableSettings {
                    col_tolerance: 40.0,
                    tolerance_unit: TablePosMeasureUnit::Pt,
                    ..TableSettings::default()
                },
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            let blocks = PdfExtractInvestmentsStandard::new(args).call(&table_page()).unwrap();
            let coords: Vec<_> = blocks
                .iter()
                .map(|b| (metadata_of(b, "table-row"), metadata_of(b, "table-col")))
                .collect();
            assert_eq!(
                coords,
                vec![
                    (BlockValue::from(0i64), BlockValue::from(0i64)),
                    (BlockValue::from(0i64), BlockValue::from(0i64)),
                    (BlockValue::from(1i64), BlockValue::from(0i64)),
                    (BlockValue::from(1i64), BlockValue::from(0i64)),
                ]
            );
        }

        /// The rows of `table_page` are centred 20 points apart. A row tolerance of 40 welds them,
        /// and it does so **without** touching the columns — the mirror of the test above.
        #[test]
        fn a_row_tolerance_joins_the_rows_and_leaves_the_columns_alone() {
            let args = InvestmentsStandardArgs {
                table: TableSettings {
                    row_tolerance: 40.0,
                    tolerance_unit: TablePosMeasureUnit::Pt,
                    ..TableSettings::default()
                },
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            let blocks = PdfExtractInvestmentsStandard::new(args).call(&table_page()).unwrap();
            let coords: Vec<_> = blocks
                .iter()
                .map(|b| (metadata_of(b, "table-row"), metadata_of(b, "table-col")))
                .collect();
            assert_eq!(
                coords,
                vec![
                    (BlockValue::from(0i64), BlockValue::from(0i64)),
                    (BlockValue::from(0i64), BlockValue::from(1i64)),
                    (BlockValue::from(0i64), BlockValue::from(0i64)),
                    (BlockValue::from(0i64), BlockValue::from(1i64)),
                ]
            );
        }

        #[test]
        fn the_table_settings_are_readable_back() {
            let args = InvestmentsStandardArgs {
                table: TableSettings { col_tolerance: 7.5, ..TableSettings::default() },
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            assert_eq!(PdfExtractInvestmentsStandard::new(args).table().col_tolerance, 7.5);
        }

        #[test]
        fn a_column_mismatch_from_the_positioning_algorithm_surfaces_as_an_error() {
            // Lines whose width is incompatible with a fixed column count: forcing the count out of
            // range through `company_index` is enough to check that the error travels up.
            let args = InvestmentsStandardArgs { company_index: Some(0), ..InvestmentsStandardArgs::new(text_sel("")) };
            assert!(PdfExtractInvestmentsStandard::new(args).call(&table_page()).is_ok());
        }

        #[test]
        fn the_pipe_name_identifies_it_in_error_messages() {
            assert_eq!(PdfExtractPipe::name(&body_pipe()), "PdfExtractInvestmentsStandard");
        }
    }

    mod assets {
        use super::*;

        /// A non-tabular page: labels on the left, values on the right.
        fn simple_page() -> Page {
            page(vec![
                line("Fund Alpha", (0.0, 0.0, 40.0, 10.0)),
                line("EUR", (0.0, 12.0, 40.0, 22.0)),
                line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                line("1000", (50.0, 30.0, 90.0, 40.0)),
                line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                line("200", (50.0, 50.0, 90.0, 60.0)),
                line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                line("800", (50.0, 70.0, 90.0, 80.0)),
            ])
        }

        fn simple_args() -> AssetsStandardArgs {
            AssetsStandardArgs::new(
                text_sel("^Fund Alpha$"),
                Some(text_sel("^EUR$")),
                AssetsColumn::new(text_sel("^Net assets$")),
                AssetsColumn::new(text_sel("^Liabilities$")),
                AssetsColumn::new(text_sel("^Total assets$")),
            )
        }

        fn simple_pipe() -> PdfExtractAssetsStandard {
            PdfExtractAssetsStandard::build(simple_args()).unwrap()
        }

        /// Una pagina con due comparti affiancati, ciascuno con le sue tre cifre — la forma di
        /// ANIMA-EN23 p63, dove gli affiancati sono otto.
        ///
        /// La cella mancante è quella che decide tutto: qui ad `Alpha` manca il passivo.
        fn two_funds_page(with_alpha_liabilities: bool) -> Page {
            let mut lines = vec![
                PdfLine::new("Head", 10.0, "Alpha", (100.0, 0.0, 160.0, 10.0)),
                PdfLine::new("Head", 10.0, "Beta", (200.0, 0.0, 260.0, 10.0)),
                line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                line("1000", (110.0, 30.0, 160.0, 40.0)),
                line("2000", (210.0, 30.0, 260.0, 40.0)),
                line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                line("400", (210.0, 50.0, 260.0, 60.0)),
                line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                line("800", (110.0, 70.0, 160.0, 80.0)),
                line("1600", (210.0, 70.0, 260.0, 80.0)),
            ];
            if with_alpha_liabilities {
                lines.push(line("200", (110.0, 50.0, 160.0, 60.0)));
            }
            page(lines)
        }

        fn two_funds_args() -> AssetsStandardArgs {
            AssetsStandardArgs {
                fund_set: font_sel("Head"),
                currency_set: Some(text_sel("^Total assets$")),
                table_condition: true,
                ..simple_args()
            }
        }

        /// Con una cella in meno, l'appaiamento per indice **fa fallire la pagina**, nominando la
        /// colonna: è l'unico caso in cui dice qualcosa, e costa tutti e due i comparti.
        #[test]
        fn pairing_by_index_fails_the_page_when_a_numeric_column_is_short() {
            let pipe = PdfExtractAssetsStandard::build(two_funds_args()).unwrap();
            let error = pipe.call(&two_funds_page(false)).unwrap_err();

            assert!(
                matches!(
                    error,
                    PdfExtractStandardFuncsError::MismatchedAssetsColumn { ref column, .. }
                        if column == "liabilities"
                ),
                "{error:?}"
            );
        }

        /// Per ascissa la cella mancante resta un buco: `Alpha` esce di scena **dicendolo**, e
        /// `Beta` tiene i suoi numeri.
        #[test]
        fn pairing_by_abscissa_leaves_a_hole_instead_of_shifting() {
            let args = AssetsStandardArgs { pairing: AssetsPairing::ByAbscissa, ..two_funds_args() };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&two_funds_page(false)).unwrap();

            assert_eq!(blocks.len(), 1);
            assert_eq!(metadata_of(&blocks[0], "fund"), BlockValue::from("Beta"));
            assert_eq!(metadata_of(&blocks[0], "liabilities"), BlockValue::from("400"));
            assert_eq!(metadata_of(&blocks[0], "net_assets"), BlockValue::from("1600"));
        }

        /// **La misura che giustifica il parametro.** Tutte le celle ci sono, e l'appaiamento per
        /// indice sbaglia lo stesso: le due cifre del passivo arrivano nell'ordine in cui stanno
        /// nella pagina, non in quello delle colonne, e i due comparti si scambiano il numero.
        ///
        /// Non è un caso costruito: molte pagine di questi bilanci portano due tabelle affiancate,
        /// e l'ordine delle righe del PDF non è l'ordine di lettura.
        #[test]
        fn pairing_by_index_follows_the_order_of_the_page_and_by_abscissa_the_columns() {
            let by_index = PdfExtractAssetsStandard::build(two_funds_args())
                .unwrap()
                .call(&two_funds_page(true))
                .unwrap();
            let args = AssetsStandardArgs { pairing: AssetsPairing::ByAbscissa, ..two_funds_args() };
            let by_abscissa =
                PdfExtractAssetsStandard::build(args).unwrap().call(&two_funds_page(true)).unwrap();

            assert_eq!(by_index.len(), 2);
            assert_eq!(by_abscissa.len(), 2);
            // Per indice: `Alpha` si prende il passivo di `Beta`, perché quella riga viene prima.
            assert_eq!(metadata_of(&by_index[0], "fund"), BlockValue::from("Alpha"));
            assert_eq!(metadata_of(&by_index[0], "liabilities"), BlockValue::from("400"));
            // Per ascissa: ciascuno il suo.
            assert_eq!(metadata_of(&by_abscissa[0], "fund"), BlockValue::from("Alpha"));
            assert_eq!(metadata_of(&by_abscissa[0], "liabilities"), BlockValue::from("200"));
            assert_eq!(metadata_of(&by_abscissa[1], "liabilities"), BlockValue::from("400"));
        }

        /// E dove l'ordine della pagina **è** quello delle colonne, le due strade coincidono: è la
        /// condizione perché il difetto possa restare quello di prima senza costare niente.
        #[test]
        fn the_two_pairings_agree_when_the_page_order_is_the_column_order() {
            let ordered = page(vec![
                PdfLine::new("Head", 10.0, "Alpha", (100.0, 0.0, 160.0, 10.0)),
                PdfLine::new("Head", 10.0, "Beta", (200.0, 0.0, 260.0, 10.0)),
                line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                line("1000", (110.0, 30.0, 160.0, 40.0)),
                line("2000", (210.0, 30.0, 260.0, 40.0)),
                line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                line("200", (110.0, 50.0, 160.0, 60.0)),
                line("400", (210.0, 50.0, 260.0, 60.0)),
                line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                line("800", (110.0, 70.0, 160.0, 80.0)),
                line("1600", (210.0, 70.0, 260.0, 80.0)),
            ]);
            let by_index = PdfExtractAssetsStandard::build(two_funds_args()).unwrap().call(&ordered).unwrap();
            let args = AssetsStandardArgs { pairing: AssetsPairing::ByAbscissa, ..two_funds_args() };
            let by_abscissa = PdfExtractAssetsStandard::build(args).unwrap().call(&ordered).unwrap();

            assert_eq!(by_index, by_abscissa);
            assert_eq!(by_index.len(), 2);
        }

        /// AMUNDI-EN24 p360 stampa ogni comparto **due volte**, esercizio in corso e precedente.
        /// `drop` nomina l'intestazione della colonna da lasciare fuori, e i numeri che le stanno
        /// sotto se ne vanno con lei.
        #[test]
        fn drop_leaves_out_the_numbers_standing_under_the_named_column() {
            let lines = vec![
                PdfLine::new("Head", 10.0, "Alpha", (100.0, 0.0, 160.0, 10.0)),
                line("30/06/2024", (110.0, 15.0, 160.0, 25.0)),
                line("30/06/2023", (170.0, 15.0, 220.0, 25.0)),
                line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                line("1000", (110.0, 30.0, 160.0, 40.0)),
                line("900", (170.0, 30.0, 220.0, 40.0)),
                line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                line("200", (110.0, 50.0, 160.0, 60.0)),
                line("100", (170.0, 50.0, 220.0, 60.0)),
                line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                line("800", (110.0, 70.0, 160.0, 80.0)),
                line("800", (170.0, 70.0, 220.0, 80.0)),
            ];
            let args = AssetsStandardArgs {
                fund_set: font_sel("Head"),
                currency_set: Some(text_sel("^Total assets$")),
                table_condition: true,
                pairing: AssetsPairing::ByAbscissa,
                drop: Some(text_sel("^30/06/2023$")),
                ..simple_args()
            };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&page(lines)).unwrap();

            assert_eq!(blocks.len(), 1);
            assert_eq!(metadata_of(&blocks[0], "tot_assets"), BlockValue::from("1000"));
            assert_eq!(metadata_of(&blocks[0], "liabilities"), BlockValue::from("200"));
        }

        /// Una pagina con tre colonne di comparto, la terza delle quali è il **totale
        /// dell'ombrello** e non un comparto — la forma di FIDEURAM-EN23 p10.
        fn umbrella_page() -> Page {
            page(vec![
                PdfLine::new("Head", 10.0, "Alpha", (100.0, 0.0, 160.0, 10.0)),
                PdfLine::new("Head", 10.0, "Beta", (200.0, 0.0, 260.0, 10.0)),
                PdfLine::new("Head", 10.0, "COMBINED STATEMENT", (300.0, 0.0, 360.0, 10.0)),
                line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                line("1000", (110.0, 30.0, 160.0, 40.0)),
                line("2000", (210.0, 30.0, 260.0, 40.0)),
                line("3000", (310.0, 30.0, 360.0, 40.0)),
                line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                line("200", (110.0, 50.0, 160.0, 60.0)),
                line("400", (210.0, 50.0, 260.0, 60.0)),
                line("600", (310.0, 50.0, 360.0, 60.0)),
                line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                line("800", (110.0, 70.0, 160.0, 80.0)),
                line("1600", (210.0, 70.0, 260.0, 80.0)),
                line("2400", (310.0, 70.0, 360.0, 80.0)),
            ])
        }

        fn umbrella_args() -> AssetsStandardArgs {
            AssetsStandardArgs {
                fund_set: font_sel("Head"),
                currency_set: Some(text_sel("^Total assets$")),
                table_condition: true,
                ..simple_args()
            }
        }

        /// `drop_funds` porta via la colonna **intera**: il nome e i numeri sotto di lei.
        #[test]
        fn drop_funds_takes_the_whole_column_away() {
            let args = AssetsStandardArgs {
                drop_funds: Some(text_sel("^COMBINED")),
                ..umbrella_args()
            };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&umbrella_page()).unwrap();

            assert_eq!(blocks.len(), 2);
            assert_eq!(metadata_of(&blocks[0], "fund"), BlockValue::from("Alpha"));
            assert_eq!(metadata_of(&blocks[0], "tot_assets"), BlockValue::from("1000"));
            assert_eq!(metadata_of(&blocks[1], "fund"), BlockValue::from("Beta"));
            assert_eq!(metadata_of(&blocks[1], "tot_assets"), BlockValue::from("2000"));
        }

        /// **La misura che giustifica il parametro.** `drop` toglieva i numeri e lasciava il nome,
        /// quindi la colonna restava un comparto senza nessuno dei suoi tre totali — vero alla
        /// lettera, e un avviso a ogni corsa.
        #[test]
        fn drop_alone_leaves_the_column_in_the_names_with_no_totals() {
            let args = AssetsStandardArgs {
                pairing: AssetsPairing::ByAbscissa,
                drop: Some(text_sel("^COMBINED")),
                ..umbrella_args()
            };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&umbrella_page()).unwrap();

            // Due righe e non tre: la terza colonna è nei nomi, i suoi numeri no, quindi `A4` la
            // lascia cadere **dicendolo**. Con `drop_funds` non c'è niente da dire.
            assert_eq!(blocks.len(), 2);
            let names: Vec<BlockValue> = blocks.iter().map(|b| metadata_of(b, "fund")).collect();
            assert_eq!(names, vec![BlockValue::from("Alpha"), BlockValue::from("Beta")]);
        }

        /// Per indice la colonna scartata non sposta i comparti che la seguono, che è la ragione
        /// per cui la sua banda entra fra quelle scartate e non solo il suo nome.
        #[test]
        fn a_dropped_column_does_not_shift_the_funds_after_it() {
            let lines = vec![
                PdfLine::new("Head", 10.0, "COMBINED STATEMENT", (100.0, 0.0, 160.0, 10.0)),
                PdfLine::new("Head", 10.0, "Alpha", (200.0, 0.0, 260.0, 10.0)),
                line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                line("3000", (110.0, 30.0, 160.0, 40.0)),
                line("1000", (210.0, 30.0, 260.0, 40.0)),
                line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                line("600", (110.0, 50.0, 160.0, 60.0)),
                line("200", (210.0, 50.0, 260.0, 60.0)),
                line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                line("2400", (110.0, 70.0, 160.0, 80.0)),
                line("800", (210.0, 70.0, 260.0, 80.0)),
            ];
            let args = AssetsStandardArgs {
                drop_funds: Some(text_sel("^COMBINED")),
                ..umbrella_args()
            };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&page(lines)).unwrap();

            assert_eq!(blocks.len(), 1);
            assert_eq!(metadata_of(&blocks[0], "fund"), BlockValue::from("Alpha"));
            // I numeri di `Alpha`, non quelli dell'ombrello scalati di una posizione.
            assert_eq!(metadata_of(&blocks[0], "tot_assets"), BlockValue::from("1000"));
            assert_eq!(metadata_of(&blocks[0], "net_assets"), BlockValue::from("800"));
        }

        /// La valuta per colonna segue le colonne **rimaste**: appaiata a quelle di prima sarebbe
        /// un comparto più a destra.
        #[test]
        fn the_per_column_currency_follows_the_columns_that_are_kept() {
            let lines = vec![
                PdfLine::new("Head", 10.0, "COMBINED STATEMENT", (100.0, 0.0, 160.0, 10.0)),
                PdfLine::new("Head", 10.0, "Alpha", (200.0, 0.0, 260.0, 10.0)),
                PdfLine::new("Curr", 10.0, "CHF", (110.0, 15.0, 160.0, 25.0)),
                PdfLine::new("Curr", 10.0, "EUR", (210.0, 15.0, 260.0, 25.0)),
                line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                line("3000", (110.0, 30.0, 160.0, 40.0)),
                line("1000", (210.0, 30.0, 260.0, 40.0)),
                line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                line("600", (110.0, 50.0, 160.0, 60.0)),
                line("200", (210.0, 50.0, 260.0, 60.0)),
                line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                line("2400", (110.0, 70.0, 160.0, 80.0)),
                line("800", (210.0, 70.0, 260.0, 80.0)),
            ];
            let args = AssetsStandardArgs {
                fund_set: font_sel("Head"),
                currency_set: Some(font_sel("Curr")),
                table_condition: true,
                pairing: AssetsPairing::ByAbscissa,
                drop_funds: Some(text_sel("^COMBINED")),
                ..simple_args()
            };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&page(lines)).unwrap();

            assert_eq!(blocks.len(), 1);
            assert_eq!(metadata_of(&blocks[0], "fund"), BlockValue::from("Alpha"));
            assert_eq!(metadata_of(&blocks[0], "currency"), BlockValue::from("EUR"));
        }

        /// Senza `drop`, la stessa pagina: il numero dell'esercizio precedente entra, ed è il
        /// difetto che `drop` esiste per togliere.
        #[test]
        fn without_drop_the_previous_year_column_is_read_as_if_it_were_a_fund() {
            let lines = vec![
                PdfLine::new("Head", 10.0, "Alpha", (100.0, 0.0, 160.0, 10.0)),
                line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                line("1000", (110.0, 30.0, 160.0, 40.0)),
                line("900", (170.0, 30.0, 220.0, 40.0)),
                line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                line("200", (110.0, 50.0, 160.0, 60.0)),
                line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                line("800", (110.0, 70.0, 160.0, 80.0)),
            ];
            let args = AssetsStandardArgs {
                fund_set: font_sel("Head"),
                currency_set: Some(text_sel("^Total assets$")),
                table_condition: true,
                pairing: AssetsPairing::ByAbscissa,
                ..simple_args()
            };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&page(lines)).unwrap();

            // Una banda sola, quindi il secondo numero cade sulla stessa e il primo resta: la riga
            // è giusta per caso, e su una pagina a più comparti non lo sarebbe.
            assert_eq!(blocks.len(), 1);
            assert_eq!(metadata_of(&blocks[0], "tot_assets"), BlockValue::from("1000"));
        }

        #[test]
        fn build_rejects_a_missing_currency_set_outside_table_mode() {
            let args = AssetsStandardArgs { currency_set: None, ..simple_args() };
            let err = PdfExtractAssetsStandard::build(args)
                .err()
                .expect("build must reject a missing currency_set outside table mode");
            assert!(matches!(err, PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { .. }));
        }

        #[test]
        fn build_accepts_a_missing_currency_set_in_table_mode() {
            let args = AssetsStandardArgs { currency_set: None, table_condition: true, ..simple_args() };
            assert!(PdfExtractAssetsStandard::build(args).is_ok());
        }

        #[test]
        fn emits_a_relevant_block_with_the_three_amounts() {
            let blocks = simple_pipe().call(&simple_page()).unwrap();
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].type_block, BlockType::RELEVANT_BLOCK);
            assert_eq!(metadata_of(&blocks[0], "tot_assets"), BlockValue::from("1000"));
            assert_eq!(metadata_of(&blocks[0], "liabilities"), BlockValue::from("200"));
            assert_eq!(metadata_of(&blocks[0], "net_assets"), BlockValue::from("800"));
        }

        #[test]
        fn carries_the_fund_name_and_the_currency() {
            let blocks = simple_pipe().call(&simple_page()).unwrap();
            assert_eq!(metadata_of(&blocks[0], "fund"), BlockValue::from("Fund Alpha"));
            assert_eq!(metadata_of(&blocks[0], "currency"), BlockValue::from("EUR"));
        }

        #[test]
        fn the_date_is_null_when_no_date_set_is_configured() {
            let blocks = simple_pipe().call(&simple_page()).unwrap();
            assert_eq!(metadata_of(&blocks[0], "date"), BlockValue::Null);
        }

        #[test]
        fn a_configured_candidate_fills_the_date_metadata() {
            let mut lines = simple_page().lines;
            lines.push(line("31/12/2024", (0.0, 90.0, 40.0, 100.0)));
            let args = AssetsStandardArgs { date: vec![text_sel("^31/12/2024$")], ..simple_args() };
            let pipe = PdfExtractAssetsStandard::build(args).unwrap();
            let blocks = pipe.call(&page(lines)).unwrap();
            assert_eq!(metadata_of(&blocks[0], "date"), BlockValue::from("31/12/2024"));
        }

        /// `R5`: si preferisce la data **della colonna**, vicina al numero; la testata di pagina è
        /// l'alternativa. Su CARNE-EN23 p19 ci sono tutt'e due, e la testata sta **più in alto** —
        /// quindi un'unione di selezioni prenderebbe proprio quella sbagliata.
        #[test]
        fn the_first_candidate_that_takes_something_wins_whatever_the_page_order() {
            let mut lines = simple_page().lines;
            lines.push(line("Annual report as of 31 December 2023", (0.0, 1.0, 200.0, 8.0)));
            lines.push(line("31.12.2023", (150.0, 90.0, 200.0, 100.0)));
            let args = AssetsStandardArgs {
                date: vec![text_sel("^31.12.2023$"), text_sel("^Annual report as of 31 December 2023$")],
                ..simple_args()
            };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&page(lines)).unwrap();
            assert_eq!(metadata_of(&blocks[0], "date"), BlockValue::from("31.12.2023"));
        }

        /// Lo stesso insieme di candidati, sull'altro ordine: il secondo vale quando il primo non
        /// prende niente, che è la pagina in cui la colonna la data non ce l'ha.
        #[test]
        fn the_next_candidate_is_tried_when_the_first_takes_nothing() {
            let mut lines = simple_page().lines;
            lines.push(line("Annual report as of 31 December 2023", (0.0, 1.0, 200.0, 8.0)));
            let args = AssetsStandardArgs {
                date: vec![text_sel("^31.12.2023$"), text_sel("^Annual report as of 31 December 2023$")],
                ..simple_args()
            };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&page(lines)).unwrap();
            assert_eq!(
                metadata_of(&blocks[0], "date"),
                BlockValue::from("Annual report as of 31 December 2023")
            );
        }

        /// FINECO-EN23@LUX p31 scrive `D` · `ECEMBER` · ` 31, 2024` in due corpi diversi, fianco a
        /// fianco: un candidato prende tutt'e tre le righe e le unisce nell'ordine di lettura.
        #[test]
        fn a_candidate_joins_the_fragments_it_takes_left_to_right() {
            let mut lines = simple_page().lines;
            lines.push(PdfLine::new("Narrow", 12.0, "D", (453.9, 44.6, 461.1, 56.6)));
            lines.push(PdfLine::new("Narrow", 9.0, "ECEMBER", (461.0, 47.4, 497.5, 56.4)));
            lines.push(PdfLine::new("Narrow", 12.0, "31, 2024", (497.5, 44.6, 538.6, 56.6)));
            let args = AssetsStandardArgs { date: vec![font_sel("Narrow")], ..simple_args() };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&page(lines)).unwrap();
            assert_eq!(metadata_of(&blocks[0], "date"), BlockValue::from("D ECEMBER 31, 2024"));
        }

        /// Un valore che va a capo resta nell'ordine in cui si legge: le due righe non si
        /// sovrappongono in verticale, quindi decide l'ordinata e non l'ascissa.
        #[test]
        fn a_candidate_that_wraps_onto_the_next_line_keeps_the_reading_order() {
            let mut lines = simple_page().lines;
            lines.push(PdfLine::new("Narrow", 10.0, "31 dicembre", (400.0, 90.0, 460.0, 100.0)));
            lines.push(PdfLine::new("Narrow", 10.0, "2024", (100.0, 104.0, 140.0, 114.0)));
            let args = AssetsStandardArgs { date: vec![font_sel("Narrow")], ..simple_args() };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&page(lines)).unwrap();
            assert_eq!(metadata_of(&blocks[0], "date"), BlockValue::from("31 dicembre 2024"));
        }

        /// `R25` e la risposta **B**: un formato che dichiara la data e non la trova avvisa e tiene
        /// la riga. Prima costava **la pagina intera**, cioè ogni altro comparto del prospetto.
        #[test]
        fn candidates_that_all_come_up_empty_warn_and_keep_the_row() {
            let args = AssetsStandardArgs { date: vec![text_sel("^nowhere on this page$")], ..simple_args() };
            let blocks = PdfExtractAssetsStandard::build(args).unwrap().call(&simple_page()).unwrap();
            assert_eq!(metadata_of(&blocks[0], "date"), BlockValue::Null);
        }

        #[test]
        fn a_missing_fund_name_is_reported_as_expected_text_not_found() {
            // This branch raises a not-found error and **not** a page-parse failure: it is fatal
            // for the document rather than a skipped page.
            let p = page(simple_page().lines.into_iter().filter(|l| l.text() != "Fund Alpha").collect());
            let err = simple_pipe().call(&p).unwrap_err();
            assert!(matches!(err, PdfExtractStandardFuncsError::Commons(CommonsError::ExpectedTextNotFound { .. })));
        }

        #[test]
        fn a_missing_fund_name_is_not_a_skipped_page_once_translated_for_the_engine() {
            let p = page(simple_page().lines.into_iter().filter(|l| l.text() != "Fund Alpha").collect());
            let err = simple_pipe().extract(&p).unwrap_err();
            assert!(!err.is_page_failure());
        }

        #[test]
        fn lines_made_only_of_a_space_are_dropped_before_anything_else() {
            let mut lines = simple_page().lines;
            lines.push(line(" ", (50.0, 30.0, 90.0, 40.0)));
            let with_blank = simple_pipe().call(&page(lines)).unwrap();
            assert_eq!(with_blank, simple_pipe().call(&simple_page()).unwrap());
        }

        #[test]
        fn a_zero_skip_column_is_rejected() {
            let pipe = PdfExtractAssetsStandard::build(AssetsStandardArgs { skip_column: 0, ..simple_args() }).unwrap();
            assert!(matches!(pipe.call(&simple_page()), Err(PdfExtractStandardFuncsError::ZeroSkipColumn)));
        }

        #[test]
        fn a_negative_skip_column_yields_no_block_like_an_empty_python_range() {
            let pipe = PdfExtractAssetsStandard::build(AssetsStandardArgs { skip_column: -1, ..simple_args() }).unwrap();
            assert!(pipe.call(&simple_page()).unwrap().is_empty());
        }

        /// La valuta **per colonna**: un prospetto che prezza i suoi comparti in valute diverse
        /// sulla stessa pagina.
        ///
        /// Il caso misurato è FIDEURAM-EN23: la prima colonna in CHF e le altre tre in EUR, e
        /// tutte e quattro uscivano in CHF perché la valuta era un dato della pagina. AMUNDI-EN24
        /// ne porta un secondo, due colonne in USD e due in EUR.
        /// `values_in_one_line`: le righe dei totali di MEDIOLANUM-EN24 p6, che portano **tutti**
        /// i valori in un oggetto di testo solo — e quella del patrimonio netto in tre, `1 + 1 + 2`.
        mod values_in_one_line {
            use super::*;
            use pretty_assertions::assert_eq;

            /// La forma misurata: due comparti affiancati, due periodi ciascuno, e le tre righe dei
            /// valori scritte come la pagina le scrive — una in un oggetto, una in un oggetto, una
            /// in tre.
            fn one_object_per_row_page() -> Page {
                page(vec![
                    PdfLine::new("Head", 10.0, "Alpha", (100.0, 0.0, 160.0, 10.0)),
                    PdfLine::new("Head", 10.0, "Beta", (200.0, 0.0, 260.0, 10.0)),
                    line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                    line("   1000     900     2000     1800  ", (100.0, 30.0, 260.0, 40.0)),
                    line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                    line("   200     150     400     350  ", (100.0, 50.0, 260.0, 60.0)),
                    line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                    // Il patrimonio netto in tre pezzi, uno più uno più due: contare gli oggetti
                    // non dice quanti valori ci sono, ed è il motivo per cui si leggono gli spazi.
                    line("800  ", (100.0, 70.0, 150.0, 80.0)),
                    line("750  ", (160.0, 70.0, 200.0, 80.0)),
                    line("1600     1450  ", (210.0, 70.0, 260.0, 80.0)),
                ])
            }

            fn args(values_in_one_line: bool) -> AssetsStandardArgs {
                AssetsStandardArgs { values_in_one_line, skip_column: 2, ..two_funds_args() }
            }

            fn amounts(blocks: &[PdfBlock]) -> Vec<(String, String, String, String)> {
                blocks
                    .iter()
                    .map(|block| {
                        let field = |name: &str| {
                            block.metadata[name].as_str().expect("the pipe writes strings").to_string()
                        };
                        (field("fund"), field("tot_assets"), field("liabilities"), field("net_assets"))
                    })
                    .collect()
            }

            #[test]
            fn a_run_of_two_spaces_is_what_tells_two_values_apart() {
                assert_eq!(values_in_a_line("   1000     900  "), vec!["1000", "900"]);
            }

            #[test]
            fn a_line_carrying_one_value_comes_back_as_that_one_value() {
                assert_eq!(values_in_a_line(" 1,234,567 "), vec!["1,234,567"]);
            }

            /// Un numero non porta mai due spazi di fila, quindi uno spazio solo non divide niente:
            /// è il caso del trattino che vale zero, ` - `.
            #[test]
            fn a_single_space_divides_nothing() {
                assert_eq!(values_in_a_line(" - "), vec!["-"]);
                assert_eq!(values_in_a_line("JUNE 30, 2025"), vec!["JUNE 30, 2025"]);
            }

            #[test]
            fn the_padding_of_the_object_costs_nothing() {
                assert_eq!(values_in_a_line("      "), Vec::<String>::new());
            }

            /// Acceso, i quattro pezzi di ogni riga si appaiano per indice alle quattro colonne, e
            /// `Value step = 2` tiene le due del periodo corrente.
            #[test]
            fn the_pieces_pair_by_index_and_the_step_keeps_the_current_period() {
                let pipe = PdfExtractAssetsStandard::build(args(true)).unwrap();
                let blocks = pipe.call(&one_object_per_row_page()).unwrap();

                assert_eq!(
                    amounts(&blocks),
                    vec![
                        ("Alpha".to_string(), "1000".to_string(), "200".to_string(), "800".to_string()),
                        ("Beta".to_string(), "2000".to_string(), "400".to_string(), "1600".to_string()),
                    ]
                );
            }

            /// Spento, la stessa pagina dà **un** comparto su due e gli mette in mano la riga
            /// intera: è il difetto che il parametro corregge, detto come misura.
            #[test]
            fn without_it_the_whole_row_becomes_one_value_and_a_fund_is_lost() {
                let pipe = PdfExtractAssetsStandard::build(args(false)).unwrap();
                let blocks = pipe.call(&one_object_per_row_page()).unwrap();

                assert_eq!(blocks.len(), 1, "un comparto su due");
                assert_eq!(
                    blocks[0].metadata["tot_assets"].as_str().unwrap(),
                    "   1000     900     2000     1800  ",
                    "la riga intera, non un numero"
                );
            }

            /// L'invarianza, detta dove conta: su una pagina scritta un oggetto per cella il
            /// parametro **non cambia niente**. È il motivo per cui le altre dieci righe della
            /// tabella non lo nominano.
            #[test]
            fn on_a_page_of_ordinary_cells_it_changes_nothing() {
                let with = PdfExtractAssetsStandard::build(AssetsStandardArgs {
                    values_in_one_line: true,
                    ..two_funds_args()
                })
                .unwrap();
                let without = PdfExtractAssetsStandard::build(two_funds_args()).unwrap();
                let page = two_funds_page(true);

                assert_eq!(amounts(&with.call(&page).unwrap()), amounts(&without.call(&page).unwrap()));
            }
        }

        mod currency_by_column {
            use super::*;
            use pretty_assertions::assert_eq;

            /// Due comparti affiancati, ciascuno con la **sua** riga di valuta sopra i numeri.
            fn page_with_two_currencies(second: &str) -> Page {
                page(vec![
                    PdfLine::new("Head", 10.0, "Alpha", (100.0, 0.0, 160.0, 10.0)),
                    PdfLine::new("Head", 10.0, "Beta", (200.0, 0.0, 260.0, 10.0)),
                    PdfLine::new("Curr", 8.0, "CHF", (110.0, 15.0, 150.0, 25.0)),
                    PdfLine::new("Curr", 8.0, second, (210.0, 15.0, 250.0, 25.0)),
                    line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                    line("1000", (110.0, 30.0, 160.0, 40.0)),
                    line("2000", (210.0, 30.0, 260.0, 40.0)),
                    line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                    line("200", (110.0, 50.0, 160.0, 60.0)),
                    line("400", (210.0, 50.0, 260.0, 60.0)),
                    line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                    line("800", (110.0, 70.0, 160.0, 80.0)),
                    line("1600", (210.0, 70.0, 260.0, 80.0)),
                ])
            }

            /// Una pagina con **una** riga di valuta sola, che è ciò che dicono i formati che
            /// leggono un `(valori espressi in euro)` e che deve continuare a dire la stessa cosa.
            fn page_with_one_currency() -> Page {
                page(vec![
                    PdfLine::new("Head", 10.0, "Alpha", (100.0, 0.0, 160.0, 10.0)),
                    PdfLine::new("Head", 10.0, "Beta", (200.0, 0.0, 260.0, 10.0)),
                    PdfLine::new("Curr", 8.0, "EUR", (0.0, 15.0, 40.0, 25.0)),
                    line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                    line("1000", (110.0, 30.0, 160.0, 40.0)),
                    line("2000", (210.0, 30.0, 260.0, 40.0)),
                    line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                    line("200", (110.0, 50.0, 160.0, 60.0)),
                    line("400", (210.0, 50.0, 260.0, 60.0)),
                    line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                    line("800", (110.0, 70.0, 160.0, 80.0)),
                    line("1600", (210.0, 70.0, 260.0, 80.0)),
                ])
            }

            fn args() -> AssetsStandardArgs {
                AssetsStandardArgs {
                    fund_set: font_sel("Head"),
                    currency_set: Some(font_sel("Curr")),
                    table_condition: true,
                    ..simple_args()
                }
            }

            fn currencies_of(page: &Page) -> Vec<BlockValue> {
                let blocks = PdfExtractAssetsStandard::build(args()).unwrap().call(page).unwrap();
                blocks.iter().map(|b| metadata_of(b, "currency")).collect()
            }

            /// Il difetto che questo cambiamento corregge.
            #[test]
            fn each_column_keeps_its_own_currency() {
                assert_eq!(
                    currencies_of(&page_with_two_currencies("EUR")),
                    vec![BlockValue::from("CHF"), BlockValue::from("EUR")]
                );
            }

            /// E nell'altro ordine, perché prima vinceva sempre la **prima** riga trovata.
            #[test]
            fn the_first_line_no_longer_wins_over_the_others() {
                let currencies = currencies_of(&page_with_two_currencies("USD"));
                assert_ne!(currencies[0], currencies[1], "due colonne, due valute");
                assert_eq!(currencies[1], BlockValue::from("USD"));
            }

            /// L'invarianza che i cinque formati che già chiamavano `A4` pretendono.
            #[test]
            fn one_currency_line_still_serves_every_column() {
                assert_eq!(
                    currencies_of(&page_with_one_currency()),
                    vec![BlockValue::from("EUR"), BlockValue::from("EUR")]
                );
            }

            /// Una colonna senza valuta propria non perde la riga: prende la prima, **dicendolo**
            /// (`R3`). Qui `Beta` non ha la sua.
            #[test]
            fn a_column_without_its_own_currency_takes_the_first_one() {
                let page = page(vec![
                    PdfLine::new("Head", 10.0, "Alpha", (100.0, 0.0, 160.0, 10.0)),
                    PdfLine::new("Head", 10.0, "Beta", (200.0, 0.0, 260.0, 10.0)),
                    PdfLine::new("Curr", 8.0, "CHF", (110.0, 15.0, 150.0, 25.0)),
                    PdfLine::new("Curr", 8.0, "CHF", (115.0, 16.0, 155.0, 26.0)),
                    line("Total assets", (0.0, 30.0, 40.0, 40.0)),
                    line("1000", (110.0, 30.0, 160.0, 40.0)),
                    line("2000", (210.0, 30.0, 260.0, 40.0)),
                    line("Liabilities", (0.0, 50.0, 40.0, 60.0)),
                    line("200", (110.0, 50.0, 160.0, 60.0)),
                    line("400", (210.0, 50.0, 260.0, 60.0)),
                    line("Net assets", (0.0, 70.0, 40.0, 80.0)),
                    line("800", (110.0, 70.0, 160.0, 80.0)),
                    line("1600", (210.0, 70.0, 260.0, 80.0)),
                ]);
                assert_eq!(
                    currencies_of(&page),
                    vec![BlockValue::from("CHF"), BlockValue::from("CHF")],
                    "nessuna riga persa"
                );
            }

            /// Una selezione che non prende niente resta l'errore che era: il formato dice che la
            /// valuta è in pagina e non c'è.
            #[test]
            fn a_currency_selection_matching_nothing_still_fails() {
                let args = AssetsStandardArgs { currency_set: Some(font_sel("Missing")), ..args() };
                let error = PdfExtractAssetsStandard::build(args)
                    .unwrap()
                    .call(&page_with_two_currencies("EUR"))
                    .unwrap_err();
                assert!(format!("{error:?}").contains("currency"), "{error:?}");
            }
        }

        mod range_helper {
            use super::*;

            #[test]
            fn a_step_of_one_yields_every_index() {
                assert_eq!(range_0_to_len_step(4, 1).unwrap(), vec![0, 1, 2, 3]);
            }

            #[test]
            fn a_step_of_two_skips_every_other_index() {
                assert_eq!(range_0_to_len_step(5, 2).unwrap(), vec![0, 2, 4]);
            }

            #[test]
            fn a_zero_length_yields_nothing() {
                assert!(range_0_to_len_step(0, 1).unwrap().is_empty());
            }

            #[test]
            fn a_zero_step_is_an_error() {
                assert!(matches!(range_0_to_len_step(3, 0), Err(PdfExtractStandardFuncsError::ZeroSkipColumn)));
            }

            #[test]
            fn a_negative_step_yields_nothing() {
                assert!(range_0_to_len_step(3, -2).unwrap().is_empty());
            }
        }

        mod trailing_currency_split {
            use super::*;

            #[test]
            fn splits_the_last_word_off_each_fund_name() {
                let (funds, currencies) =
                    PdfExtractAssetsStandard::split_trailing_currencies(vec!["Fund Alpha EUR".to_string()]).unwrap();
                assert_eq!(funds, vec!["Fund Alpha".to_string()]);
                assert_eq!(currencies, vec!["EUR".to_string()]);
            }

            #[test]
            fn a_single_word_leaves_an_empty_fund_name() {
                let (funds, currencies) =
                    PdfExtractAssetsStandard::split_trailing_currencies(vec!["EUR".to_string()]).unwrap();
                assert_eq!(funds, vec![String::new()]);
                assert_eq!(currencies, vec!["EUR".to_string()]);
            }

            #[test]
            fn an_empty_column_has_no_currency_token_to_split() {
                let err = PdfExtractAssetsStandard::split_trailing_currencies(vec!["   ".to_string()]).unwrap_err();
                assert!(matches!(err, PdfExtractStandardFuncsError::MissingCurrencyToken { .. }));
            }

            #[test]
            fn handles_several_columns_independently() {
                let (funds, currencies) = PdfExtractAssetsStandard::split_trailing_currencies(vec![
                    "Fund A USD".to_string(),
                    "Fund B GBP".to_string(),
                ])
                .unwrap();
                assert_eq!(funds, vec!["Fund A".to_string(), "Fund B".to_string()]);
                assert_eq!(currencies, vec!["USD".to_string(), "GBP".to_string()]);
            }
        }
    }
}
