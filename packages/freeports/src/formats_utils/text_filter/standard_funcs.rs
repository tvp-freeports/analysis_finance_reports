//! The standard `text_filter` pipes: deciding which of a page's blocks concern the funds and
//! companies being looked for.
//!
//! The most substantial of them is [`TextFilterInvestmentsStandard`], which walks the investments
//! table one row at a time, asks whether any target company is named in the row, and if so reads
//! the row's fields at the offsets the format configured. [`TextFilterPageClassifyStandard`]
//! reduces a page's classification contributions to one, and the remaining three pick out the SFDR
//! article, the management company and the fund's assets.
//!
//! # Reading a currency out of free text
//!
//! [`extract_currency_from_text`] makes two passes. First, every three-uppercase-letter word in the
//! text **as written**, in the order it appears, and the first that is a valid code wins. Only if
//! none is does it fall back to scanning the upper-cased text for each currency in
//! [`Currency::prose_candidates`] in turn, then for the `EURO` alias.
//!
//! The second pass deliberately searches a **smaller** set than the first accepts. Upper-casing the
//! text turns every three-letter word into a candidate code, and most of ISO 4217 is also an
//! ordinary word — `ALL`, `TOP`, `CUP`, `SOS`. A field the report declares to be a currency may
//! hold any of the 159; a currency merely mentioned in a sentence is only guessed for the majors.
//!
//! Order matters, and the first pass exists to get it right: scanning for known codes finds
//! whichever code happens to come first in the *list of currencies*, not the one the document
//! actually declared. Neither pass ever matches a fragment that is not delimited by a word
//! boundary, so `"EURUSD"` and `"100EUR"` are not currencies.
//!
//! Every pipe here implements [`TextFilterPipe`]; the
//! inherent `call` stays as a direct API typed on its own errors, and the trait is the form the
//! engine uses.

use once_cell::sync::Lazy;
use onig::Regex;
use std::collections::{BTreeMap, BTreeSet};

use crate::commons::consts::Currency;
use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock, TextBlock};
use crate::core::match_fund::MatchFund;
use crate::core::page::PageError;
use crate::core::pipeline::{Extracted, FilterData, PipeError, TextFilterPipe};
use crate::formats_utils::text_filter::dash_as_zero::{DashAsZero, substituted};
use crate::formats_utils::text_filter::matcher::{CompanyMatchInfos, match_company};
use crate::formats_utils::text_filter::standard_txt_blk_builders::{
    standard_fund_txt_blk, standard_management_company_txt_blk,
};
use crate::output::classes::fund::Fund;

#[derive(Debug, thiserror::Error)]
pub enum StandardFuncsError {
    #[error("expected at least one pdf block to classify")]
    NoPdfBlocks,
    #[error("page classified both as {first:?} and as {second:?}")]
    ConflictingPageClass { first: String, second: String },
    #[error(transparent)]
    Value(#[from] crate::core::classes::value::BlockValueError),
    #[error("no currency found in text")]
    NoCurrencyFound,
    /// The block the pipe expected to find is not there. **Not fatal**: the row is skipped and the
    /// loop moves on.
    ///
    /// It carries *which* field could not be read and *where* the pipe looked, because those two
    /// answers are what separate the two ways a row goes missing: a page whose grid came out the
    /// wrong width (every offset lands beyond the data), and a row whose description wrapped onto a
    /// second line (the grid is normal, the neighbouring cell is empty). Without them the warning
    /// says a row was skipped and leaves the PDF as the only way to find out why.
    #[error("the {field} is missing: {probe}")]
    ExpectedTextBlockNotFound { field: &'static str, probe: FieldProbe },
    /// A pipe that searches a **flat list** of blocks for one of a given type found none. It has
    /// nothing to do with the variant above — there is no table, no anchor and no offset here — and
    /// sharing one error between the two only meant neither could say what it was looking for.
    #[error("no {block_type} block on the page")]
    ExpectedBlockTypeMissing { block_type: &'static str },
    /// The page cannot be interpreted: becomes a page failure, which the algorithm absorbs by
    /// skipping the page.
    #[error("{message}")]
    PageParseFail { message: String },
    #[error("two subfunds in the same page")]
    TwoFundsInSamePage,
    #[error("two currencies in the same page")]
    TwoCurrenciesInSamePage,
    #[error("all positions should be different")]
    PositionsMustDiffer,
    #[error("company matching failed: {message}")]
    Match { message: String },
    #[error("inconsistent investments table: {message}")]
    InconsistentTable { message: String },
    /// A regex pattern is invalid, or does not have the required number of capture groups — checked
    /// at **construction**, not at call time, so a misconfigured format fails when it is loaded.
    #[error("invalid pattern '{pattern}': {message}")]
    InvalidPattern { pattern: String, message: String },
    /// The pattern is valid but does not match the value given at call time.
    #[error("value '{text}' does not match the configured date pattern")]
    DateRegexMismatch { text: String },
}

impl StandardFuncsError {
    /// Translates into the engine's error type. The pipe's name cannot be recovered from the error,
    /// so the caller supplies it.
    ///
    /// Only [`StandardFuncsError::PageParseFail`] becomes a **non-fatal** page failure; everything
    /// else stops the run.
    pub fn into_pipe_error(self, pipe: &str) -> PipeError {
        match self {
            StandardFuncsError::Value(source) => PipeError::value(pipe, source),
            StandardFuncsError::PageParseFail { message } => {
                PipeError::page_parse(pipe, PageError::ParseFail { message })
            }
            other => PipeError::extraction(pipe, other.to_string()),
        }
    }
}

pub struct TextFilterPageClassifyStandard;

impl TextFilterPageClassifyStandard {
    pub fn call(&self, pdf_blks: &[PdfBlock]) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let last = pdf_blks.last().ok_or(StandardFuncsError::NoPdfBlocks)?;
        let mut found: Option<&BlockValue> = None;
        for blk in pdf_blks {
            let page_type = blk.metadata_or_fail("page_type")?;
            if !page_type.is_null() {
                if let Some(existing) = found {
                    return Err(StandardFuncsError::ConflictingPageClass {
                        first: format!("{existing:?}"),
                        second: format!("{page_type:?}"),
                    });
                }
                found = Some(page_type);
            }
        }
        let page_type = found.cloned().unwrap_or(BlockValue::Null);
        // As for the page classifier: not being classified is the normal case on nearly every page,
        // and logging it would fill the file without saying anything.
        if !page_type.is_null() {
            tracing::debug!(coord_ref_2 = ?page_type, "page class assigned");
        }
        let metadata = BTreeMap::from([("page_type".to_string(), page_type)]);
        Ok(vec![TextBlock::new(BlockType::PAGE_CLASS, metadata, last.clone())])
    }
}

static ISO_CODE_CANDIDATE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b[A-Z]{3}\b").expect("fixed, hand-written pattern, valid onig regex"));

fn word_boundary_pattern(word: &str) -> Regex {
    Regex::new(&format!(r"\b{word}\b")).expect("currency code/alias is a fixed, valid pattern")
}

/// Extracts a [`Currency`] from free text; see the module documentation for the two passes.
///
/// Every check below uses a search rather than a whole-string match, because the pattern has to be
/// able to match anywhere in the text.
pub fn extract_currency_from_text(text: &str) -> Result<Currency, StandardFuncsError> {
    for (start, end) in ISO_CODE_CANDIDATE.find_iter(text) {
        if let Some(currency) = Currency::from_name(&text[start..end]) {
            return Ok(currency);
        }
    }

    // `prose_candidates`, not `variants`: this pass upper-cases the text, so over the full ISO
    // 4217 list it would read "at all" as Albanian lek and "top holdings" as Tongan paʻanga. See
    // `Currency::prose_candidates` for why guessing and being told are two different questions.
    let upper = text.to_uppercase();
    for currency in Currency::prose_candidates() {
        if word_boundary_pattern(currency.code()).find(&upper).is_some() {
            return Ok(*currency);
        }
    }
    if word_boundary_pattern("EURO").find(&upper).is_some() {
        return Ok(Currency::EUR);
    }

    Err(StandardFuncsError::NoCurrencyFound)
}

impl TextFilterPipe for TextFilterPageClassifyStandard {
    fn name(&self) -> &str {
        "TextFilterPageClassifyStandard"
    }

    fn filter(
        &self,
        blocks: &[PdfBlock],
        _data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks).map_err(|e| e.into_pipe_error(self.name()))
    }
}


// ----------------------------------------------------------------------------------------------
// TextFilterInvestmentsStandard, with PdfBlocksTable inlined (its only real caller)
// ----------------------------------------------------------------------------------------------
/// A page's investments table, rebuilt from the `table-row` and `table-col` metadata of its PDF
/// blocks.
///
/// The grid holds **indices** into the flat list of blocks, which is the sole owner, rather than a
/// second set of references to the same blocks. One owner means the two views cannot drift apart,
/// which is the failure mode of keeping both.
///
/// Assumes the `table-row` values are contiguous from zero; violating that is
/// [`StandardFuncsError::InconsistentTable`] rather than an out-of-range access.
struct PdfBlocksTable {
    blks: Vec<PdfBlock>,
    /// row → column → indices into `blks` occupying that cell; normally zero or one, but more is
    /// possible and handled.
    indexes: Vec<Vec<Vec<usize>>>,
}

/// What a cell holds.
///
/// [`Cell::Many`] deliberately carries no blocks: its only two consumers either ask whether the
/// cell is occupied, or read a single content that a multi-block cell cannot provide. The variant
/// expresses exactly those two answers — occupied yes, readable no — without holding values nobody
/// reads.
enum Cell<'a> {
    Empty,
    One(&'a PdfBlock),
    Many,
}

/// Reads a required integer metadata field of a table block.
fn table_meta_int(block: &PdfBlock, field: &str) -> Result<i64, StandardFuncsError> {
    Ok(block.metadata_or_fail(field)?.int_or_fail(field)?)
}

/// Reads a required boolean metadata field of a table block.
fn table_meta_bool(block: &PdfBlock, field: &str) -> Result<bool, StandardFuncsError> {
    Ok(block.metadata_or_fail(field)?.bool_or_fail(field)?)
}

/// The text content of a table block.
fn table_content(block: &PdfBlock) -> Result<String, StandardFuncsError> {
    Ok(block.content.str_or_fail("content")?.to_string())
}

impl PdfBlocksTable {
    fn new(pdf_blocks: &[PdfBlock]) -> Result<Self, StandardFuncsError> {
        let blks = pdf_blocks.to_vec();

        let mut grouped: BTreeMap<i64, BTreeMap<i64, Vec<usize>>> = BTreeMap::new();
        let mut col_max: i64 = 0;
        for (i, blk) in blks.iter().enumerate() {
            let row = table_meta_int(blk, "table-row")?;
            let col = table_meta_int(blk, "table-col")?;
            col_max = col_max.max(col);
            grouped.entry(row).or_default().entry(col).or_default().push(i);
        }

        let indexes = grouped
            .values()
            .map(|cols| {
                (0..=col_max).map(|col| cols.get(&col).cloned().unwrap_or_default()).collect()
            })
            .collect();

        Ok(PdfBlocksTable { blks, indexes })
    }

    fn len(&self) -> usize {
        self.blks.len()
    }

    fn n_cols(&self) -> usize {
        self.indexes.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// Indexes the flat list, negative indices included, where `-1` is the last.
    fn get_flat(&self, i: i64) -> Option<&PdfBlock> {
        let len = self.blks.len() as i64;
        let idx = if i < 0 { i + len } else { i };
        (0..len).contains(&idx).then(|| &self.blks[idx as usize])
    }

    /// Indexes the grid, negative indices included; out of range is an empty cell rather than an
    /// error, since a table with a ragged edge is normal.
    fn get_cell(&self, row: i64, col: i64) -> Cell<'_> {
        let rows = self.indexes.len() as i64;
        let r = if row < 0 { row + rows } else { row };
        if !(0..rows).contains(&r) {
            return Cell::Empty;
        }
        let row_vec = &self.indexes[r as usize];
        let cols = row_vec.len() as i64;
        let c = if col < 0 { col + cols } else { col };
        if !(0..cols).contains(&c) {
            return Cell::Empty;
        }
        match row_vec[c as usize].as_slice() {
            [] => Cell::Empty,
            [only] => Cell::One(&self.blks[*only]),
            _ => Cell::Many,
        }
    }

    /// Removes the block at `j` from both the flat list and the grid, recompacting the indices. If
    /// the row is left empty it disappears and the following rows shift up by one.
    fn pop(&mut self, j: usize) -> Result<(), StandardFuncsError> {
        if j >= self.blks.len() {
            return Err(StandardFuncsError::InconsistentTable {
                message: format!("cannot pop block {j}: the table has {} blocks", self.blks.len()),
            });
        }
        let blk = self.blks.remove(j);
        let row_del = table_meta_int(&blk, "table-row")?;
        let col_del = table_meta_int(&blk, "table-col")?;

        let (row_idx, col_idx) = (row_del as usize, col_del as usize);
        let cell = self
            .indexes
            .get_mut(row_idx)
            .and_then(|row| row.get_mut(col_idx))
            .ok_or_else(|| StandardFuncsError::InconsistentTable {
                message: format!("block {j} claims cell ({row_del}, {col_del}), which does not exist"),
            })?;

        if let Some(position) = cell.iter().position(|&idx| idx == j) {
            cell.remove(position);
            for row in &mut self.indexes {
                for col in row.iter_mut() {
                    for idx in col.iter_mut() {
                        if *idx > j {
                            *idx -= 1;
                        }
                    }
                }
            }
        }

        if self.indexes[row_idx].iter().all(Vec::is_empty) {
            self.indexes.remove(row_idx);
            for blk in &mut self.blks {
                let row = table_meta_int(blk, "table-row")?;
                if row > row_del {
                    blk.metadata.insert("table-row".to_string(), BlockValue::Int(row - 1));
                }
            }
        }
        Ok(())
    }

    /// Merges the contents of blocks `j` and `i` — in the order they appear in the list, not the
    /// order of the arguments — writing the result into `i` and removing `j`.
    fn merge(&mut self, j: usize, i: usize) -> Result<(), StandardFuncsError> {
        let (first, last) = if i < j { (i, j) } else { (j, i) };
        if first >= self.blks.len() || last >= self.blks.len() {
            return Err(StandardFuncsError::InconsistentTable {
                message: format!(
                    "cannot merge blocks {j} and {i}: the table has {} blocks",
                    self.blks.len()
                ),
            });
        }
        let combined =
            format!("{}{}", table_content(&self.blks[first])?, table_content(&self.blks[last])?);
        self.blks[i].content = BlockValue::Str(combined);
        self.pop(j)
    }
}

/// Where the row being extracted is: its position in the flat list, its anchor cell in the grid,
/// and the table's width.
///
/// The three always travel together, so they live in a struct rather than as three repeated
/// parameters.
/// Where [`TextFilterInvestmentsStandard::extract_field`] looked for one field's cell.
///
/// The offset alone does not locate anything — it is a distance from an anchor the reader of a log
/// cannot see — and the cell alone does not say why that cell was chosen. Both together do, which
/// is why they travel as one value, computed once and used both to read the cell and to describe it
/// afterwards. Computing them twice is exactly how a diagnostic starts lying.
///
/// Rendered **one-based**, like every other coordinate that reaches a log: see
/// [`crate::core::tracing_setup::table_coords`] for why the grid's own indices stay zero-based and
/// only the rendering shifts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldProbe {
    /// Geometric mode: the offset from the anchor, the cell it resolved to, and the grid's width —
    /// the width being the one number that tells a page whose columns came out wrong from a page
    /// whose columns are fine.
    Grid { offset: i64, row: i64, col: i64, n_cols: i64 },
    /// Flat mode: the offset from the anchor and the position it resolved to in the block list.
    Flat { offset: i64, index: i64 },
}

impl std::fmt::Display for FieldProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Offset zero is the anchor itself, and "0 cells past the anchor" is not a sentence anyone
        // wants to read: the clause is dropped rather than printed as a degenerate case.
        match *self {
            FieldProbe::Grid { offset: 0, row, col, n_cols } => {
                let (row, col) = crate::core::tracing_setup::table_coords(row, col);
                write!(f, "{row} {col} of a {n_cols}-column grid")
            }
            FieldProbe::Grid { offset, row, col, n_cols } => {
                let (row, col) = crate::core::tracing_setup::table_coords(row, col);
                write!(f, "{offset} cells past the anchor, {row} {col} of a {n_cols}-column grid")
            }
            FieldProbe::Flat { offset: 0, index } => write!(f, "block {}", index + 1),
            FieldProbe::Flat { offset, index } => {
                write!(f, "{offset} blocks past the anchor, block {}", index + 1)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RowAnchor {
    /// The row's position in the flat list of blocks.
    flat_index: i64,
    /// The `(row, column)` cell the geometric offsets start from.
    base: (i64, i64),
    /// The table's column count, for wrapping the offsets.
    n_cols: i64,
}

/// Extracts the investment rows of a table, one per recognised target company.
///
/// The `*_pos` values are offsets from the anchor cell. In geometric mode they are linear distances
/// that wrap into the next row when they exceed the table's width; otherwise they are positions in
/// the flat list of blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFilterInvestmentsStandard {
    pub market_value_pos: i64,
    pub nominal_quantity_pos: Option<i64>,
    pub perc_net_assets_pos: Option<i64>,
    pub acquisition_currency_pos: Option<i64>,
    pub acquisition_cost_pos: Option<i64>,
    /// Whether the offsets are geometric — row and column with wrapping — or positions in the flat
    /// list.
    pub geometrical_indexes: bool,
    /// Whether a cell split across two blocks merges into the **preceding** block or the following
    /// one.
    pub merge_prev: bool,
    /// Which numeric fields read a dash the report prints as the zero it means. Empty by default,
    /// which is no change at all: see [`crate::formats_utils::text_filter::dash_as_zero`].
    pub dash_as_zero: DashAsZero,
}

impl TextFilterInvestmentsStandard {
    /// The eight parameters come from a formats repository's configuration columns, which is what
    /// builds this pipe.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        market_value_pos: i64,
        nominal_quantity_pos: Option<i64>,
        perc_net_assets_pos: Option<i64>,
        acquisition_currency_pos: Option<i64>,
        acquisition_cost_pos: Option<i64>,
        geometrical_indexes: bool,
        merge_prev: bool,
        dash_as_zero: DashAsZero,
    ) -> Result<Self, StandardFuncsError> {
        // The check fires **only** when both optional positions are present: with just one of them,
        // a value colliding with `market_value_pos` is not rejected.
        if let (Some(nq), Some(pna)) = (nominal_quantity_pos, perc_net_assets_pos)
            && (nq == market_value_pos || nq == pna || market_value_pos == pna)
        {
            return Err(StandardFuncsError::PositionsMustDiffer);
        }
        Ok(TextFilterInvestmentsStandard {
            market_value_pos,
            nominal_quantity_pos,
            perc_net_assets_pos,
            acquisition_currency_pos,
            acquisition_cost_pos,
            geometrical_indexes,
            merge_prev,
            dash_as_zero,
        })
    }

    /// Separates the fund name and the currency from the table blocks, then extracts the rows.
    ///
    /// A quirk worth knowing: if the loop over the table produces **no** rows, the result is empty
    /// — the fund-name block, already built, is discarded along with it.
    pub fn call(
        &self,
        pdf_blks: &[PdfBlock],
        target_companies: &[CompanyMatchInfos],
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let mut fund_found: Option<BlockValue> = None;
        let mut currency_found: Option<Currency> = None;
        let mut results: Vec<TextBlock> = Vec::new();
        let mut investments_blks: Vec<PdfBlock> = Vec::new();

        for blk in pdf_blks {
            if blk.type_block == BlockType::FUND_NAME {
                if fund_found.is_some() {
                    return Err(StandardFuncsError::TwoFundsInSamePage);
                }
                fund_found = Some(blk.content.clone());
                results.push(standard_fund_txt_blk(blk.clone()));
            } else if blk.type_block == BlockType::CURRENCY_STATEMENT {
                if currency_found.is_some() {
                    return Err(StandardFuncsError::TwoCurrenciesInSamePage);
                }
                let text = blk.content.str_or_fail("content")?;
                // Here, and only here, an unrecognised currency fails the **page** rather than the
                // document.
                currency_found = Some(extract_currency_from_text(text).map_err(|e| {
                    StandardFuncsError::PageParseFail { message: e.to_string() }
                })?);
            } else {
                investments_blks.push(blk.clone());
            }
        }

        let mut inv = self.run_loop(&investments_blks, target_companies)?;
        if inv.is_empty() {
            if !results.is_empty() {
                // The quirk described above: the fund and currency blocks already built are
                // discarded along with the empty investments table.
                tracing::debug!(
                    "no investment rows extracted - discarding the fund/currency blocks already built for this page"
                );
            }
            return Ok(Vec::new());
        }
        let fund = fund_found.unwrap_or(BlockValue::Null);
        let currency = currency_found.map_or(BlockValue::Null, BlockValue::from);
        for txt_blk in &mut inv {
            txt_blk.metadata.insert("fund".to_string(), fund.clone());
            txt_blk.metadata.insert("currency".to_string(), currency.clone());
        }
        tracing::debug!(coord_ref_1 = ?fund, rows = inv.len(), "investment rows extracted");
        results.extend(inv);
        Ok(results)
    }

    /// The loop over the table's rows: for each block, decide whether it is split onto the next
    /// row, look for a target company in its text, and if one is found extract the row's fields.
    fn run_loop(
        &self,
        pdf_blocks: &[PdfBlock],
        target_companies: &[CompanyMatchInfos],
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let mut out = Vec::new();
        if pdf_blocks.is_empty() {
            return Ok(out);
        }
        let mut table = PdfBlocksTable::new(pdf_blocks)?;
        let n_cols = table.n_cols() as i64;

        let mut i: i64 = 0;
        // Deliberately declared **outside** the loop: the tail below reuses it with whatever value
        // the loop left — zero if the loop never ran — not with the column of the last block.
        let mut col: i64 = 0;

        while i < table.len() as i64 - 1 {
            let mut split = false;
            let (row, cell_width, mut content) = {
                let current = table.get_flat(i).ok_or(StandardFuncsError::InconsistentTable {
                    message: format!("block {i} disappeared from the table"),
                })?;
                col = table_meta_int(current, "table-col")?;
                (
                    table_meta_int(current, "table-row")?,
                    table_meta_bool(current, "is-max-width")?,
                    table_content(current)?,
                )
            };
            let (next_row, next_col, next_content) = {
                let next = table.get_flat(i + 1).ok_or(StandardFuncsError::InconsistentTable {
                    message: format!("block {} disappeared from the table", i + 1),
                })?;
                (
                    table_meta_int(next, "table-row")?,
                    table_meta_int(next, "table-col")?,
                    table_content(next)?,
                )
            };

            if col == next_col {
                let probe_row = if self.merge_prev { row } else { next_row };
                let mut n_full_cols = 0;
                let mut empty_adj = 0;
                for c in 0..n_cols {
                    if matches!(table.get_cell(probe_row, c), Cell::Empty) {
                        if c == col - 1 || c == col + 1 {
                            empty_adj += 1;
                        }
                    } else {
                        n_full_cols += 1;
                    }
                }
                if n_full_cols == 1 || empty_adj == 2 {
                    split = true;
                    if cell_width || content.ends_with(' ') || content.ends_with('\n') {
                        content.push_str(&next_content);
                    }
                }
            }

            if let Some(company) = self.matched_company(&content, target_companies)? {
                if split {
                    let (current_idx, next_idx) = (i as usize, (i + 1) as usize);
                    if self.merge_prev {
                        table.merge(current_idx, next_idx)?;
                    } else {
                        table.merge(next_idx, current_idx)?;
                    }
                }
                let anchor = RowAnchor { flat_index: i, base: (row, col), n_cols };
                self.push_extracted_field(&mut out, &table, anchor, &content, &company)?;
            }

            i += 1;
            if i >= table.len() as i64 - 1 {
                break;
            }
        }

        if i == table.len() as i64 - 1 {
            let (row, content) = {
                let last = table.get_flat(-1).ok_or(StandardFuncsError::InconsistentTable {
                    message: "the table is empty".to_string(),
                })?;
                (table_meta_int(last, "table-row")?, table_content(last)?)
            };
            if let Some(company) = self.matched_company(&content, target_companies)? {
                let anchor = RowAnchor { flat_index: i, base: (row, col), n_cols };
                self.push_extracted_field(&mut out, &table, anchor, &content, &company)?;
            }
        }
        Ok(out)
    }

    /// The target company recognised in the text, if there is one.
    fn matched_company(
        &self,
        content: &str,
        target_companies: &[CompanyMatchInfos],
    ) -> Result<Option<String>, StandardFuncsError> {
        match_company(content, target_companies)
            .map(|found| found.map(str::to_string))
            .map_err(|e| StandardFuncsError::Match { message: e.to_string() })
    }

    /// Extracts the row's fields and, if the expected block was there, appends it to `out`.
    ///
    /// A [`StandardFuncsError::ExpectedTextBlockNotFound`] is **absorbed**: the row is skipped and
    /// the loop carries on.
    fn push_extracted_field(
        &self,
        out: &mut Vec<TextBlock>,
        table: &PdfBlocksTable,
        anchor: RowAnchor,
        content: &str,
        company: &str,
    ) -> Result<(), StandardFuncsError> {
        match self.extract_field(table, anchor) {
            Ok(mut txt_blk) => {
                // Where the row hooked itself, in the grid's own coordinates. Every field of the
                // row is read at a fixed offset from this cell, so an anchor that landed on a
                // header, a total or a currency code shifts them all — and shows up here as a
                // column different from every other row of the page.
                //
                // `debug`, because it is one line per position found: it lives in the JSONL and on
                // stderr at `-vv`, never in the `.log.csv`, whose ceiling is `warn`.
                let (row, col) = crate::core::tracing_setup::table_coords(anchor.base.0, anchor.base.1);
                tracing::debug!(
                    coord_ref_1 = %content,
                    coord_ref_2 = company,
                    coord_1 = %row,
                    coord_2 = %col,
                    "investment row anchored on the matched company"
                );
                txt_blk.metadata.insert("company match".to_string(), BlockValue::from(content));
                txt_blk.metadata.insert("company".to_string(), BlockValue::from(company));
                // The position travels with the row, because the deserializer cannot recover it:
                // by the time it sees this block the table it came from no longer exists, and the
                // events it emits are the ones that end up in the `.log.csv` needing a coordinate.
                txt_blk.metadata.insert("table row".to_string(), BlockValue::from(anchor.base.0));
                txt_blk.metadata.insert("table col".to_string(), BlockValue::from(anchor.base.1));
                out.push(txt_blk);
                Ok(())
            }
            Err(StandardFuncsError::ExpectedTextBlockNotFound { field, probe }) => {
                // `coord_ref_1`, rather than an arbitrary field name: the first anchor column of
                // the `.log.csv` exists precisely for this, a piece of text with which the row can
                // be found again inside the PDF. As a free field it fell outside the columns and
                // was readable only on stderr.
                //
                // The triggering text and not the company, because only the former is written in
                // the report: the company is the name the input database gives the issuer.
                //
                // The coordinates stay the **anchor**: they are what locates the row on the page,
                // which is what those two columns are for. The cell that was actually probed goes
                // in the message, where it reads as a sentence and where it is not a coordinate
                // competing with the one the reader needs. It is not repeated as a structured
                // field: one statement of a fact per event.
                let (row, col) = crate::core::tracing_setup::table_coords(anchor.base.0, anchor.base.1);
                tracing::warn!(
                    coord_ref_1 = %content,
                    coord_ref_2 = company,
                    coord_1 = %row,
                    coord_2 = %col,
                    "the {field} is missing: {probe} - row skipped"
                );
                Ok(())
            }
            Err(other) => Err(other),
        }
    }

    /// One field's text, with the report's dash turned into `"0"` where the format asked for it.
    ///
    /// The substitution puts back the **text** `"0"`, not a typed zero, so everything downstream —
    /// the integer-or-float choice the deserializer makes, the percentage normalisation, the
    /// domain validation — runs exactly as it would on a report that had printed `0` itself. There
    /// is no second path through the deserializer to keep in step with the first.
    ///
    /// `debug`, not `warn`: this is a reading the format declared, not an anomaly, and
    /// `.log.csv`'s ceiling is `warn`. What does reach the audit trail is the consequence — a
    /// market value or a percentage of exactly zero sits on the edge of its domain, and the output
    /// entity says so on its own.
    fn read_dash_as_zero(&self, field: Option<DashAsZero>, name: &str, text: String) -> String {
        let read = substituted(self.dash_as_zero, field, &text);
        if read != text {
            tracing::debug!(
                coord_ref_2 = name,
                "the report writes {text:?} for the {name} - read as 0, as the format declares"
            );
            return read.to_string();
        }
        text
    }

    /// The fields of one investment row, read at the configured offsets from the anchor cell.
    ///
    /// In geometric mode an offset is a **linear distance** that wraps into the next row when it
    /// exceeds the table's width, not a pair of coordinates added component-wise.
    fn extract_field(
        &self,
        table: &PdfBlocksTable,
        RowAnchor { flat_index, base, n_cols }: RowAnchor,
    ) -> Result<TextBlock, StandardFuncsError> {
        // Where an offset lands, without reading anything. Split out from the reading below so the
        // cell a failure reports is, by construction, the cell the reading tried: the two cannot
        // drift apart because there is only one of them.
        let probe_of = |offset: i64| -> FieldProbe {
            if self.geometrical_indexes {
                let (r, c) = base;
                // The column count is never zero: the loop exits earlier if the table is empty, and
                // every row of the grid has at least one column.
                FieldProbe::Grid {
                    offset,
                    row: r + (c + offset).div_euclid(n_cols),
                    col: (c + offset).rem_euclid(n_cols),
                    n_cols,
                }
            } else {
                FieldProbe::Flat { offset, index: flat_index + offset }
            }
        };
        // `Err` carries the probe rather than nothing, so both callers below — the required field
        // that fails the row and the optional one that only logs — can say where they looked.
        let resolve = |offset: i64| -> Result<String, FieldProbe> {
            let probe = probe_of(offset);
            let found = match probe {
                FieldProbe::Grid { row, col, .. } => match table.get_cell(row, col) {
                    Cell::One(b) => b.content.as_str().map(str::to_string),
                    _ => None,
                },
                FieldProbe::Flat { index, .. } => {
                    table.get_flat(index).and_then(|b| b.content.as_str().map(str::to_string))
                }
            };
            found.ok_or(probe)
        };

        // The anchor is the field at offset zero: naming it that way rather than special-casing it
        // means it reports its own absence in the same words as every other field.
        let anchor_probe = probe_of(0);
        let missing_anchor =
            || StandardFuncsError::ExpectedTextBlockNotFound { field: "anchor cell", probe: anchor_probe };
        let anchor = match anchor_probe {
            FieldProbe::Grid { row, col, .. } => match table.get_cell(row, col) {
                Cell::One(block) => block,
                _ => return Err(missing_anchor()),
            },
            FieldProbe::Flat { index, .. } => table.get_flat(index).ok_or_else(missing_anchor)?,
        };

        let mut metadata = BTreeMap::new();
        // An absent key is `None` rather than an error.
        metadata.insert(
            "manco".to_string(),
            anchor.metadata.get("manco").cloned().unwrap_or(BlockValue::Null),
        );

        let market_value = resolve(self.market_value_pos)
            .map_err(|probe| StandardFuncsError::ExpectedTextBlockNotFound { field: "market value", probe })?;
        metadata.insert(
            "market value".to_string(),
            BlockValue::from(self.read_dash_as_zero(Some(DashAsZero::MarketValue), "market value", market_value)),
        );

        // `acquisition currency` is in the loop with **no** flag: a dash there says "no currency",
        // and zero is not a currency. `None` and not `DashAsZero::empty()` — see `substituted`,
        // where the difference between the two is the difference between never and always.
        for (pos, name, field) in [
            (self.perc_net_assets_pos, "% net assets", Some(DashAsZero::PercNetAssets)),
            (self.nominal_quantity_pos, "quantity", Some(DashAsZero::Quantity)),
            (self.acquisition_currency_pos, "acquisition currency", None),
            (self.acquisition_cost_pos, "acquisition cost", Some(DashAsZero::AcquisitionCost)),
        ] {
            if let Some(pos) = pos {
                // Unlike the market value, an optional field that is not found does not fail the
                // extraction: it stays `Null`.
                let value = match resolve(pos) {
                    Ok(text) => BlockValue::from(self.read_dash_as_zero(field, name, text)),
                    Err(probe) => {
                        // The one event that makes a silent loss visible. A `Null` here has two
                        // causes that look identical downstream — the report left the cell blank,
                        // which is normal, or the offset landed on a cell that is not this row's,
                        // which is a page read wrongly — and only the probe tells them apart: a
                        // grid wider than the format expects is the second.
                        //
                        // `debug` and not `warn`: the first cause is ordinary, and `.log.csv` stops
                        // at `warn`. At `-vv` it is there for whoever is looking.
                        tracing::debug!("the {name} is missing: {probe} - field left empty");
                        BlockValue::Null
                    }
                };
                metadata.insert(name.to_string(), value);
            }
        }

        let content = anchor.content.str_or_fail("content")?.replace('\n', "");
        let mut instrument = BlockType::EQUITY_TARGET;
        for pattern in PERC_REGEXES.iter() {
            if let Some(captures) = pattern.captures(&content)
                && let Some(matched) = captures.at(1)
            {
                instrument = BlockType::BOND_TARGET;
                metadata.insert("interest rate".to_string(), BlockValue::from(matched));
                break;
            }
        }
        for pattern in DATE_REGEXES.iter() {
            if let Some(captures) = pattern.captures(&content)
                && let Some(matched) = captures.at(1)
            {
                instrument = BlockType::BOND_TARGET;
                metadata.insert("maturity".to_string(), BlockValue::from(matched));
                break;
            }
        }

        Ok(TextBlock::new(instrument, metadata, anchor.clone()))
    }
}

impl TextFilterPipe for TextFilterInvestmentsStandard {
    fn name(&self) -> &str {
        "TextFilterInvestmentsStandard"
    }

    fn filter(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks, data.target_companies()).map_err(|e| e.into_pipe_error(self.name()))
    }
}

// The `\A` prefix on each pattern is not decorative: matching must begin at position 0, while a
// free search would match anywhere.
//
// On a content such as `"1,300,000.00 ITALY BTPS 3.4% …"`, which starts with a digit, the first
// pattern — which requires a leading letter — must not match at all. A free search would start
// matching at `"ITALY"` and invent an interest rate. This is a real regression found on genuine
// fixtures, not a hypothetical one.
//
// The first pattern's `.*?` is lazy where every other one here is greedy, and that is the whole
// point: a greedy prefix eats as much as it can while still leaving a match, so on
// `"PEMEX 10.00%"` it swallowed the `1` and captured `0.00%`. Every coupon with two digits before
// the point lost all but the last one — `10.25%` became `0.25%`. Lazy takes the leftmost match,
// which is the coupon as written.
//
// The decimal group is optional for a second reason: an integer coupon such as `2%` matched no
// pattern here, fell through to the second one, and came back with a fragment of the maturity
// date read as an interest rate.
static PERC_REGEXES: Lazy<Vec<Regex>> = Lazy::new(|| {
    [r"\A[a-zA-Z].*?((\d+(?:[.,]\d+)?)\s*%).*", r"\A[a-zA-Z].*((\d+[.,]\d+)\s*).*"]
        .into_iter()
        .map(|p| Regex::new(p).expect("fixed, hand-written pattern, valid onig regex"))
        .collect()
});

static DATE_REGEXES: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"\A.*(\d{2}[/\-.]\d{2}[/\-.]\d{4}).*",
        r"\A.*(\d{4}[/\-.]\d{2}[/\-.]\d{2}).*",
        r"\A.*(\d{2}[/\-.]\d{2}[/\-.]\d{2}).*",
        r"\A.*\s(\d{2}[/\-]\d{2})\s.*",
    ]
    .into_iter()
    .map(|p| Regex::new(p).expect("fixed, hand-written pattern, valid onig regex"))
    .collect()
});

/// Compiles a regex pattern supplied by a formats repository, turning an invalid pattern into a
/// typed error rather than a panic. Unlike the fixed library patterns above, these come from
/// external configuration.
fn compile_pattern(pattern: &str) -> Result<Regex, StandardFuncsError> {
    Regex::new(pattern).map_err(|e| StandardFuncsError::InvalidPattern {
        pattern: pattern.to_string(),
        message: e.description().to_string(),
    })
}

/// The funds seen as **resolved** `Fund`s in the preceding steps of the schedule, as a set of
/// [`MatchFund`].
fn resolved_funds(data: &FilterData<'_>) -> BTreeSet<MatchFund> {
    data.previous().iter().filter_map(Extracted::as_fund).filter_map(Fund::name).map(MatchFund::new).collect()
}

/// The `text_filter` pipe for a fund's SFDR classification (article 6, 8 or 9).
///
/// Takes the **first** PDF block, strips the literal prefixes and then the pattern ones, in that
/// order, and optionally checks that the resulting fund is among the investment funds seen in
/// earlier steps.
pub struct TextFilterSfdrArticleStandard {
    prefix_strings: Vec<String>,
    prefix_patterns: Vec<Regex>,
    demand_investment_funds_match: bool,
}

impl TextFilterSfdrArticleStandard {
    pub fn new(
        prefix_strings: Vec<String>,
        prefix_patterns: Vec<String>,
        demand_investment_funds_match: bool,
    ) -> Result<Self, StandardFuncsError> {
        let prefix_patterns =
            prefix_patterns.iter().map(|p| compile_pattern(p)).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { prefix_strings, prefix_patterns, demand_investment_funds_match })
    }

    /// The resolved names of the investment funds (equities and bonds) seen in earlier steps.
    fn resolved_investment_funds(data: &FilterData<'_>) -> BTreeSet<MatchFund> {
        data.previous()
            .iter()
            .filter_map(|e| e.as_equity().map(|eq| &eq.data).or_else(|| e.as_bond().map(|b| &b.data)))
            .filter_map(|inv| inv.fund.resolved())
            .map(MatchFund::new)
            .collect()
    }

    pub fn call(
        &self,
        pdf_blks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let first = pdf_blks.first().ok_or(StandardFuncsError::NoPdfBlocks)?;
        let mut fund_name = first.content.str_or_fail("content")?.to_string();

        // Literal prefixes are removed as substrings wherever they occur, applied in order and
        // before the pattern prefixes.
        for prefix in &self.prefix_strings {
            fund_name = fund_name.replace(prefix.as_str(), "");
        }
        for pattern in &self.prefix_patterns {
            fund_name = pattern.replace_all(&fund_name, "");
        }

        if self.demand_investment_funds_match {
            let known = Self::resolved_investment_funds(data);
            if !known.contains(&MatchFund::new(&fund_name)) {
                tracing::debug!(fund = fund_name, "SFDR article discarded: fund not an investment fund seen so far");
                return Ok(Vec::new());
            }
        }

        Ok(vec![TextBlock::from_content(BlockType::SFDR_ARTICLE, first.metadata.clone(), fund_name)])
    }
}

impl TextFilterPipe for TextFilterSfdrArticleStandard {
    fn name(&self) -> &str {
        "TextFilterSfdrArticleStandard"
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks, data).map_err(|e| e.into_pipe_error(self.name()))
    }
}

/// The `text_filter` pipe for the management company: finds the **first** `MANAGEMENT_COMPANY`
/// block and delegates to [`standard_management_company_txt_blk`].
pub struct TextFilterManagmentCompanyStandard;

impl TextFilterManagmentCompanyStandard {
    pub fn call(
        &self,
        pdf_blks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let block = pdf_blks
            .iter()
            .find(|b| b.type_block == BlockType::MANAGEMENT_COMPANY)
            .ok_or(StandardFuncsError::ExpectedBlockTypeMissing { block_type: "management company" })?;
        let funds = resolved_funds(data);
        Ok(vec![standard_management_company_txt_blk(block.clone(), &funds)])
    }
}

impl TextFilterPipe for TextFilterManagmentCompanyStandard {
    fn name(&self) -> &str {
        "TextFilterManagmentCompanyStandard"
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks, data).map_err(|e| e.into_pipe_error(self.name()))
    }
}

/// The `text_filter` pipe for a fund's assets. Iterates **every** block without filtering by type,
/// on the assumption that the segment hands it only the relevant blocks the assets extractor
/// produced.
pub struct TextFilterAssetsStandard {
    date_regex: Option<Regex>,
    remove_from_fund_regexes: Vec<Regex>,
}

impl TextFilterAssetsStandard {
    pub fn new(
        date_regex: Option<&str>,
        remove_from_fund_regexes: Vec<String>,
    ) -> Result<Self, StandardFuncsError> {
        let date_regex = date_regex
            .map(|p| {
                let compiled = compile_pattern(p)?;
                if compiled.captures_len() != 1 {
                    return Err(StandardFuncsError::InvalidPattern {
                        pattern: p.to_string(),
                        message: format!(
                            "expected exactly one capturing group, found {}",
                            compiled.captures_len()
                        ),
                    });
                }
                Ok(compiled)
            })
            .transpose()?;
        let remove_from_fund_regexes =
            remove_from_fund_regexes.iter().map(|p| compile_pattern(p)).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { date_regex, remove_from_fund_regexes })
    }

    pub fn call(
        &self,
        pdf_blks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let known_funds = resolved_funds(data);
        let mut out = Vec::new();

        for blk in pdf_blks {
            let raw_fund = blk.metadata_or_fail("fund")?.str_or_fail("fund")?;
            let mut fund_name = raw_fund.to_string();
            for pattern in &self.remove_from_fund_regexes {
                fund_name = pattern.replace_all(&fund_name, "");
            }

            if !known_funds.contains(&MatchFund::new(&fund_name)) {
                tracing::trace!(fund = fund_name, "assets block skipped: fund not among the known funds");
                continue;
            }

            let mut metadata = blk.metadata.clone();
            metadata.insert("fund".to_string(), BlockValue::from(fund_name));

            if let Some(date_regex) = &self.date_regex
                && let Some(date_value) = metadata.get("date").cloned()
            {
                let text = date_value.str_or_fail("date")?;
                let captured = date_regex
                    .captures(text)
                    .and_then(|caps| caps.at(1))
                    .ok_or_else(|| StandardFuncsError::DateRegexMismatch { text: text.to_string() })?;
                metadata.insert("date".to_string(), BlockValue::from(captured));
            }

            let currency_text = metadata.get("currency").ok_or_else(|| {
                StandardFuncsError::Value(crate::core::classes::value::BlockValueError::MissingField {
                    field: "currency".to_string(),
                })
            })?;
            let currency = extract_currency_from_text(currency_text.str_or_fail("currency")?)?;
            metadata.insert("currency".to_string(), BlockValue::from(currency));

            out.push(TextBlock::from_content(BlockType::RELEVANT_BLOCK, metadata, ""));
        }

        tracing::debug!(blocks = out.len(), "assets blocks matched against known funds");
        Ok(out)
    }
}

impl TextFilterPipe for TextFilterAssetsStandard {
    fn name(&self) -> &str {
        "TextFilterAssetsStandard"
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks, data).map_err(|e| e.into_pipe_error(self.name()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::consts::Currency;
    use crate::core::classes::value::BlockValue;
    use crate::core::classes::{BlockType, PdfBlock};
    use std::collections::BTreeMap;

    fn page_class_block(page_type: Option<&str>) -> PdfBlock {
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "page_type".to_string(),
            page_type.map(BlockValue::from).unwrap_or(BlockValue::Null),
        );
        PdfBlock::new(BlockType::new("SOME_PDF_TYPE"), metadata, "")
    }

    mod text_filter_page_classify {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn no_classified_block_yields_a_null_page_type() {
            let blks = vec![page_class_block(None), page_class_block(None)];
            let result = TextFilterPageClassifyStandard.call(&blks).unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].type_block, BlockType::PAGE_CLASS);
            assert_eq!(result[0].metadata.get("page_type"), Some(&BlockValue::Null));
        }

        #[test]
        fn a_single_classified_block_is_reported() {
            let blks = vec![page_class_block(None), page_class_block(Some("investments"))];
            let result = TextFilterPageClassifyStandard.call(&blks).unwrap();
            assert_eq!(
                result[0].metadata.get("page_type"),
                Some(&BlockValue::Str("investments".to_string()))
            );
        }

        #[test]
        fn two_conflicting_classifications_is_an_error() {
            let blks =
                vec![page_class_block(Some("investments")), page_class_block(Some("other"))];
            assert!(TextFilterPageClassifyStandard.call(&blks).is_err());
        }

        #[test]
        fn an_empty_list_of_pdf_blocks_is_an_error() {
            assert!(TextFilterPageClassifyStandard.call(&[]).is_err());
        }

        #[test]
        fn the_resulting_pdf_block_is_the_last_one_in_the_list_not_the_first() {
            let first = page_class_block(None);
            let last = page_class_block(None);
            let blks = vec![first, last.clone()];
            let result = TextFilterPageClassifyStandard.call(&blks).unwrap();
            assert_eq!(result[0].pdf_block.as_deref(), Some(&last));
        }
    }

    mod extract_currency_from_text {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn finds_an_iso_code_isolated_in_noise() {
            assert_eq!(
                extract_currency_from_text("Value: 100 EUR total").unwrap(),
                Currency::EUR
            );
        }

        #[test]
        fn finds_a_currency_by_full_name_case_insensitively() {
            assert_eq!(extract_currency_from_text("amount in euro today").unwrap(), Currency::EUR);
        }

        #[test]
        fn finds_the_euro_alias() {
            assert_eq!(extract_currency_from_text("priced in EURO").unwrap(), Currency::EUR);
        }

        #[test]
        fn prefers_the_first_currency_mentioned_in_the_text() {
            // The result must depend on which code actually appears first in the text, not on the
            // declaration order of the currency list.
            assert_eq!(
                extract_currency_from_text("Converted from USD to EUR").unwrap(),
                Currency::USD
            );
            assert_eq!(
                extract_currency_from_text("Converted from EUR to USD").unwrap(),
                Currency::EUR
            );
        }

        #[test]
        fn errors_when_no_currency_is_present() {
            assert!(extract_currency_from_text("no currency here").is_err());
        }

        #[test]
        fn does_not_match_a_code_glued_to_other_letters() {
            // `"EURUSD"` is a single six-letter word: neither `"EUR"` nor `"USD"` is a standalone,
            // word-boundary-delimited match inside it.
            assert!(extract_currency_from_text("Ticker: EURUSD").is_err());
        }

        #[test]
        fn does_not_match_a_code_glued_to_a_digit() {
            assert!(extract_currency_from_text("100EUR").is_err());
        }

        /// The upper-casing pass would read these as Albanian lek, Tongan paʻanga and Cuban peso
        /// if it searched the whole ISO 4217 list. Reading nothing is the right answer, and the
        /// reason the pass searches `Currency::prose_candidates` instead.
        #[test]
        fn an_ordinary_word_that_happens_to_be_an_iso_code_is_not_a_currency() {
            for text in ["nothing at all here", "the top ten holdings", "a cup of coffee"] {
                assert!(extract_currency_from_text(text).is_err(), "{text:?} is not a currency");
            }
        }

        /// The same three letters written **as a code** still are one: the first pass reads the
        /// text as written, so an upper-case standalone token is taken at face value.
        #[test]
        fn the_same_letters_written_as_an_upper_case_code_still_are_one() {
            assert_eq!(extract_currency_from_text("expressed in ALL").unwrap(), Currency::ALL);
        }
    }


    use crate::core::pipeline::{FilterData, PipeError, TextFilterPipe};
    // ------------------------------------------------------------------------------------------
    // The pipes seen through the engine's traits, and TextFilterInvestmentsStandard
    // ------------------------------------------------------------------------------------------
    use crate::formats_utils::text_filter::matcher::{CompanyMatchInfos, TargetCompanyInput};

    /// A table-row block, with the three metadata fields the table requires.
    fn table_row(row: i64, col: i64, text: &str, is_max_width: bool) -> PdfBlock {
        let metadata = BTreeMap::from([
            ("table-row".to_string(), BlockValue::Int(row)),
            ("table-col".to_string(), BlockValue::Int(col)),
            ("is-max-width".to_string(), BlockValue::Bool(is_max_width)),
        ]);
        PdfBlock::new(BlockType::TABLE_BODY, metadata, text)
    }

    /// Target companies built from the name alone — matching already works on the normalised name,
    /// with no need for regexes or symbols.
    fn targets(names: &[&str]) -> Vec<CompanyMatchInfos> {
        CompanyMatchInfos::compile_from_target_companies(
            names
                .iter()
                .map(|name| TargetCompanyInput {
                    name: (*name).to_string(),
                    regexs: vec![],
                    symbols: vec![],
                    buds: vec![],
                })
                .collect(),
        )
        .expect("names without patterns always compile")
    }

    /// The filter in its simplest configuration: only `market_value_pos`, geometric indices.
    fn simple_investments(market_value_pos: i64) -> TextFilterInvestmentsStandard {
        TextFilterInvestmentsStandard::new(market_value_pos, None, None, None, None, true, false, DashAsZero::empty())
            .expect("positions are consistent")
    }

    mod page_classify_as_a_text_filter_pipe {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_pipe_name_identifies_it_in_errors() {
            assert_eq!(TextFilterPageClassifyStandard.name(), "TextFilterPageClassifyStandard");
        }

        #[test]
        fn filtering_produces_the_same_block_as_the_direct_call() {
            let blks = vec![page_class_block(Some("investments"))];
            let direct = TextFilterPageClassifyStandard.call(&blks).unwrap();
            let through_trait =
                TextFilterPageClassifyStandard.filter(&blks, &FilterData::EMPTY).unwrap();
            assert_eq!(direct, through_trait);
        }

        #[test]
        fn the_filter_data_is_ignored() {
            let blks = vec![page_class_block(Some("investments"))];
            let companies = targets(&["Acme"]);
            assert_eq!(
                TextFilterPageClassifyStandard
                    .filter(&blks, &FilterData::TargetCompanies(&companies))
                    .unwrap(),
                TextFilterPageClassifyStandard.filter(&blks, &FilterData::EMPTY).unwrap()
            );
        }

        #[test]
        fn an_empty_page_is_a_fatal_error_not_a_page_failure() {
            let err = TextFilterPageClassifyStandard.filter(&[], &FilterData::EMPTY).unwrap_err();
            assert_eq!(err.pipe(), "TextFilterPageClassifyStandard");
            assert!(!err.is_page_failure());
        }
    }

    mod investments_construction {
        use super::*;

        #[test]
        fn distinct_positions_are_accepted() {
            assert!(
                TextFilterInvestmentsStandard::new(0, Some(1), Some(2), None, None, true, false, DashAsZero::empty())
                    .is_ok()
            );
        }

        #[test]
        fn the_three_main_positions_must_differ_from_each_other() {
            for (mv, nq, pna) in [(0, 0, 1), (0, 1, 1), (0, 1, 0)] {
                assert!(
                    TextFilterInvestmentsStandard::new(
                        mv,
                        Some(nq),
                        Some(pna),
                        None,
                        None,
                        true,
                        false,
                        DashAsZero::empty()
                    )
                    .is_err(),
                    "({mv}, {nq}, {pna}) should be rejected"
                );
            }
        }

        #[test]
        fn a_single_optional_position_is_never_checked_against_market_value() {
            // The check fires only if *both* optional positions are present, so the collision with
            // `market_value_pos` goes unnoticed here.
            assert!(
                TextFilterInvestmentsStandard::new(0, Some(0), None, None, None, true, false, DashAsZero::empty())
                    .is_ok()
            );
            assert!(
                TextFilterInvestmentsStandard::new(0, None, Some(0), None, None, true, false, DashAsZero::empty())
                    .is_ok()
            );
        }

        #[test]
        fn the_optional_acquisition_positions_are_never_checked() {
            assert!(
                TextFilterInvestmentsStandard::new(0, None, None, Some(0), Some(0), true, false, DashAsZero::empty())
                    .is_ok()
            );
        }
    }

    /// Reading the report's dash as the zero it means, one field at a time.
    ///
    /// The unit of the recogniser and the parser is
    /// [`crate::formats_utils::text_filter::dash_as_zero`]; what is tested here is the wiring —
    /// that each flag reaches its own field and no other, and that the substitution happens where
    /// the block is built rather than somewhere downstream.
    mod dash_read_as_zero {
        use super::*;
        use pretty_assertions::assert_eq;

        /// A four-column row: company, market value, quantity, percentage of net assets.
        fn row_of(company: &str, market_value: &str, quantity: &str, perc: &str) -> Vec<PdfBlock> {
            vec![
                table_row(0, 0, company, false),
                table_row(0, 1, market_value, false),
                table_row(0, 2, quantity, false),
                table_row(0, 3, perc, false),
            ]
        }

        fn pipe(flags: DashAsZero) -> TextFilterInvestmentsStandard {
            TextFilterInvestmentsStandard::new(1, Some(2), Some(3), None, None, true, false, flags)
                .expect("positions are consistent")
        }

        fn field(flags: DashAsZero, blks: &[PdfBlock], name: &str) -> BlockValue {
            let out = pipe(flags).call(blks, &targets(&["Acme Corp"])).expect("the page parses");
            assert_eq!(out.len(), 1, "expected exactly one row");
            out[0].metadata.get(name).expect("the field is configured").clone()
        }

        #[test]
        fn a_flagged_market_value_reads_its_dash_as_zero() {
            let blks = row_of("Acme Corp", "-", "10", "0.5");
            assert_eq!(field(DashAsZero::MarketValue, &blks, "market value"), BlockValue::from("0"));
        }

        /// Without the flag the dash travels on untouched, and the deserializer drops the holding
        /// exactly as it did before this feature existed.
        #[test]
        fn an_unflagged_market_value_keeps_its_dash() {
            let blks = row_of("Acme Corp", "-", "10", "0.5");
            assert_eq!(field(DashAsZero::empty(), &blks, "market value"), BlockValue::from("-"));
        }

        #[test]
        fn each_flag_reaches_its_own_field_and_no_other() {
            let blks = row_of("Acme Corp", "-", "-", "-");
            for (flag, substituted, untouched) in [
                (DashAsZero::MarketValue, "market value", ["quantity", "% net assets"]),
                (DashAsZero::Quantity, "quantity", ["market value", "% net assets"]),
                (DashAsZero::PercNetAssets, "% net assets", ["market value", "quantity"]),
            ] {
                assert_eq!(field(flag, &blks, substituted), BlockValue::from("0"), "{substituted}");
                for name in untouched {
                    assert_eq!(field(flag, &blks, name), BlockValue::from("-"), "{name}");
                }
            }
        }

        #[test]
        fn all_substitutes_every_numeric_field_at_once() {
            let blks = row_of("Acme Corp", "-", "-", "-");
            for name in ["market value", "quantity", "% net assets"] {
                assert_eq!(field(DashAsZero::all(), &blks, name), BlockValue::from("0"), "{name}");
            }
        }

        /// The misalignment case: a flagged field whose cell holds a currency code is a format bug,
        /// and must reach the deserializer unchanged so that it still fails loudly.
        #[test]
        fn a_flagged_field_holding_something_other_than_a_dash_is_left_alone() {
            for text in ["USD", "Assets", "", "n/a"] {
                let blks = row_of("Acme Corp", text, "10", "0.5");
                assert_eq!(
                    field(DashAsZero::all(), &blks, "market value"),
                    BlockValue::from(text),
                    "{text:?} must not become a zero"
                );
            }
        }

        /// An en dash is what several reports actually print, and it is recognised like the ASCII
        /// one.
        #[test]
        fn the_other_dashes_of_the_family_are_read_the_same_way() {
            for text in ["\u{2013}", "--", " - "] {
                let blks = row_of("Acme Corp", text, "10", "0.5");
                assert_eq!(
                    field(DashAsZero::MarketValue, &blks, "market value"),
                    BlockValue::from("0"),
                    "{text:?}"
                );
            }
        }

        /// `acquisition currency` has no flag of its own: zero is not a currency, and no
        /// combination of flags — `ALL` included — may turn its dash into one.
        #[test]
        fn the_acquisition_currency_is_never_substituted() {
            let blks = vec![
                table_row(0, 0, "Acme Corp", false),
                table_row(0, 1, "1.000", false),
                table_row(0, 2, "-", false),
            ];
            let pipe =
                TextFilterInvestmentsStandard::new(1, None, None, Some(2), None, true, false, DashAsZero::all())
                    .expect("positions are consistent");
            let out = pipe.call(&blks, &targets(&["Acme Corp"])).expect("the page parses");
            assert_eq!(
                out[0].metadata.get("acquisition currency"),
                Some(&BlockValue::from("-")),
                "a dash where a currency belongs is not a zero"
            );
        }
    }

    mod investments_field_extraction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_matched_row_becomes_one_text_block_carrying_the_company() {
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();

            assert_eq!(out.len(), 1);
            assert_eq!(out[0].metadata.get("company"), Some(&BlockValue::from("Acme Corp")));
            assert_eq!(out[0].metadata.get("company match"), Some(&BlockValue::from("Acme Corp")));
            assert_eq!(out[0].metadata.get("market value"), Some(&BlockValue::from("1.000")));
        }

        #[test]
        fn a_row_with_no_target_company_produces_nothing() {
            let blks = vec![table_row(0, 0, "Nothing here", false), table_row(0, 1, "1.000", false)];
            assert!(simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap().is_empty());
        }

        #[test]
        fn the_optional_fields_are_read_at_their_offsets() {
            let blks = vec![
                table_row(0, 0, "Acme Corp", false),
                table_row(0, 1, "1.000", false),
                table_row(0, 2, "12,5", false),
                table_row(0, 3, "42", false),
            ];
            let filter =
                TextFilterInvestmentsStandard::new(1, Some(3), Some(2), None, None, true, false, DashAsZero::empty())
                    .unwrap();
            let out = filter.call(&blks, &targets(&["Acme Corp"])).unwrap();

            assert_eq!(out[0].metadata.get("% net assets"), Some(&BlockValue::from("12,5")));
            assert_eq!(out[0].metadata.get("quantity"), Some(&BlockValue::from("42")));
        }

        #[test]
        fn an_optional_field_that_is_not_there_stays_null_instead_of_failing() {
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            // `% net assets` points at column 5, which does not exist.
            let filter =
                TextFilterInvestmentsStandard::new(1, None, Some(5), None, None, true, false, DashAsZero::empty())
                    .unwrap();
            let out = filter.call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("% net assets"), Some(&BlockValue::Null));
        }

        #[test]
        fn a_market_value_that_is_not_there_drops_the_whole_row() {
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            // Unlike the optional fields, a missing market value makes the row be skipped.
            assert!(simple_investments(9).call(&blks, &targets(&["Acme Corp"])).unwrap().is_empty());
        }

        #[test]
        fn the_manco_metadata_of_the_anchor_is_carried_over() {
            let mut anchor = table_row(0, 0, "Acme Corp", false);
            anchor.metadata.insert("manco".to_string(), BlockValue::from("Acme SGR"));
            let blks = vec![anchor, table_row(0, 1, "1.000", false)];

            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("manco"), Some(&BlockValue::from("Acme SGR")));
        }

        #[test]
        fn an_anchor_without_manco_gets_a_null_one() {
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("manco"), Some(&BlockValue::Null));
        }

        #[test]
        fn a_geometric_offset_wraps_into_the_next_row() {
            // A 2x2 table: from cell (0,0) an offset of 2 wraps into the next row, column 0.
            let blks = vec![
                table_row(0, 0, "Acme Corp", false),
                table_row(0, 1, "ignored", false),
                table_row(1, 0, "wrapped", false),
                table_row(1, 1, "ignored too", false),
            ];
            let out = simple_investments(2).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("market value"), Some(&BlockValue::from("wrapped")));
        }

        #[test]
        fn flat_offsets_walk_the_block_list_instead_of_the_grid() {
            let blks = vec![
                table_row(0, 0, "Acme Corp", false),
                table_row(0, 1, "1.000", false),
                table_row(1, 0, "next row", false),
            ];
            let filter =
                TextFilterInvestmentsStandard::new(2, None, None, None, None, false, false, DashAsZero::empty())
                    .unwrap();
            let out = filter.call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("market value"), Some(&BlockValue::from("next row")));
        }
    }

    /// Where a row hooked itself is the one thing a misread page never says on its own. Every
    /// field is read at a fixed offset from the anchor cell, so an anchor on the wrong column
    /// shifts them all — and the only way to see it without re-running the job is to have the
    /// position in the log.
    /// The events this pipe emits are half its contract — the anchor it chose, the field it could
    /// not read — so several submodules below need to read them back. The subscriber that captures
    /// them lives here, once, rather than once per submodule that asserts on a log line.
    mod tracing_capture {
        use std::sync::{Arc, Mutex};
        use tracing::field::{Field, Visit};
        use tracing_subscriber::Registry;
        use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

        #[derive(Default, Clone, Debug)]
        pub(super) struct Record {
            pub(super) level: String,
            pub(super) message: String,
            pub(super) fields: Vec<(String, String)>,
        }

        impl Visit for Record {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.message = format!("{value:?}");
                } else {
                    self.fields.push((field.name().to_string(), format!("{value:?}")));
                }
            }

            fn record_str(&mut self, field: &Field, value: &str) {
                self.fields.push((field.name().to_string(), value.to_string()));
            }
        }

        #[derive(Clone, Default)]
        struct CapturingLayer {
            records: Arc<Mutex<Vec<Record>>>,
        }

        impl<S: tracing::Subscriber> Layer<S> for CapturingLayer {
            fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
                let mut record = Record { level: event.metadata().level().to_string(), ..Record::default() };
                event.record(&mut record);
                self.records.lock().unwrap().push(record);
            }
        }

        /// Runs `f` with a capturing subscriber installed and keeps the events whose message
        /// contains `needle`.
        pub(super) fn records_matching(f: impl FnOnce(), needle: &str) -> Vec<Record> {
            let layer = CapturingLayer::default();
            let subscriber = Registry::default().with(layer.clone());
            tracing::subscriber::with_default(subscriber, f);
            let records = layer.records.lock().unwrap();
            records.iter().filter(|r| r.message.contains(needle)).cloned().collect()
        }

        pub(super) fn field_of<'a>(record: &'a Record, name: &str) -> Option<&'a str> {
            record.fields.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
        }
    }

    mod investments_anchor_logging {
        use super::*;
        use super::tracing_capture::{Record, field_of, records_matching};
        use pretty_assertions::assert_eq;

        fn anchor_records(f: impl FnOnce()) -> Vec<Record> {
            records_matching(f, "anchored")
        }

        #[test]
        fn a_matched_row_records_the_cell_it_hooked_itself_to() {
            let blks = vec![
                table_row(0, 0, "Country", false),
                table_row(0, 1, "Market value", false),
                table_row(1, 0, "Acme Corp", false),
                table_row(1, 1, "1.000", false),
            ];
            let records = anchor_records(|| {
                let _ = simple_investments(1).call(&blks, &targets(&["Acme Corp"]));
            });

            assert_eq!(records.len(), 1, "{records:?}");
            assert_eq!(field_of(&records[0], "coord_ref_1"), Some("Acme Corp"));
            assert_eq!(field_of(&records[0], "coord_ref_2"), Some("Acme Corp"));
            // One-based: the matched cell is the grid's `(1, 0)`, and a person counting rows down
            // the page arrives at the second row, not the first.
            assert_eq!(field_of(&records[0], "coord_1"), Some("row 2"));
            assert_eq!(field_of(&records[0], "coord_2"), Some("col 1"));
        }

        /// The two anchors are **not** interchangeable, and this is the case that tells them apart:
        /// the cell says more than the company's name. `coord_ref_1` is the text as the report
        /// writes it, which is what a search inside the PDF can find; `coord_ref_2` is the name the
        /// input database gives the company, which appears nowhere in the document.
        #[test]
        fn the_first_anchor_is_the_triggering_text_and_the_second_the_company() {
            let blks = vec![
                table_row(0, 0, "Acme Corp Reg Shs", false),
                table_row(0, 1, "1.000", false),
                table_row(1, 0, "filler", false),
                table_row(1, 1, "2.000", false),
            ];
            let records = anchor_records(|| {
                let _ = simple_investments(1).call(&blks, &targets(&["Acme Corp"]));
            });

            assert_eq!(records.len(), 1, "{records:?}");
            assert_eq!(field_of(&records[0], "coord_ref_1"), Some("Acme Corp Reg Shs"));
            assert_eq!(field_of(&records[0], "coord_ref_2"), Some("Acme Corp"));
        }

        /// The position has to survive the segment boundary: by the time the deserializer sees the
        /// block, the table it was read from no longer exists, and the events that end up in the
        /// `.log.csv` are all born there.
        #[test]
        fn the_row_carries_its_table_position_in_its_metadata() {
            let blks = vec![
                table_row(0, 0, "Country", false),
                table_row(0, 1, "Market value", false),
                table_row(1, 0, "Acme Corp", false),
                table_row(1, 1, "1.000", false),
            ];
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();

            assert_eq!(out.len(), 1, "{out:?}");
            assert_eq!(out[0].metadata.get("table row"), Some(&BlockValue::from(1i64)));
            assert_eq!(out[0].metadata.get("table col"), Some(&BlockValue::from(0i64)));
        }

        /// The skipped-row warning is the one event of this function that *does* reach the
        /// `.log.csv`, so it is the one that most needs the position: the reader has nothing else
        /// to go on, the row having produced no output at all.
        #[test]
        fn a_skipped_row_says_where_it_was_going_to_read_from() {
            // The matched cell has no cell to its right, so the market value cannot be read and
            // the row is skipped.
            let blks = vec![
                table_row(0, 0, "Country", false),
                table_row(0, 1, "Market value", false),
                table_row(1, 0, "Acme Corp", false),
                table_row(2, 0, "zzz", false),
                table_row(2, 1, "5", false),
            ];
            let records = records_matching(
                || {
                    let _ = simple_investments(1).call(&blks, &targets(&["Acme Corp"]));
                },
                "row skipped",
            );

            assert_eq!(records.len(), 1, "{records:?}");
            assert_eq!(records[0].level, "WARN");
            assert_eq!(field_of(&records[0], "coord_ref_1"), Some("Acme Corp"));
            // One-based: the matched cell is the grid's `(1, 0)`, and a person counting rows down
            // the page arrives at the second row, not the first.
            assert_eq!(field_of(&records[0], "coord_1"), Some("row 2"));
            assert_eq!(field_of(&records[0], "coord_2"), Some("col 1"));
        }

        #[test]
        fn it_stays_at_debug_so_it_never_reaches_the_audit_trail() {
            // `CSV_MAX_LEVEL` is a fixed ceiling at `warn`: one line per position found belongs in
            // the JSONL and on stderr at `-vv`, not in the extraction's audit trail.
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            let records = anchor_records(|| {
                let _ = simple_investments(1).call(&blks, &targets(&["Acme Corp"]));
            });
            assert_eq!(records[0].level, "DEBUG");
        }

        #[test]
        fn a_row_that_matched_nothing_records_no_anchor() {
            let blks = vec![table_row(0, 0, "Nothing here", false), table_row(0, 1, "1.000", false)];
            let records = anchor_records(|| {
                let _ = simple_investments(1).call(&blks, &targets(&["Acme Corp"]));
            });
            assert!(records.is_empty(), "{records:?}");
        }

        #[test]
        fn a_row_whose_fields_could_not_be_read_records_no_anchor_either() {
            // The event says a row *was extracted* at this position; a row that was skipped has
            // its own line, at `warn`, and two lines for one row is what the log must not do.
            let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
            let records = anchor_records(|| {
                let _ = simple_investments(9).call(&blks, &targets(&["Acme Corp"]));
            });
            assert!(records.is_empty(), "{records:?}");
        }
    }

    /// A skipped row used to say only that it had been skipped. What it now has to say is *which*
    /// field could not be read and *where* the pipe looked for it, because those two answers are
    /// the whole diagnosis: an offset that lands beyond the data on a grid wider than the format
    /// expects is a page tabularised wrongly, while the same offset landing on an empty cell of a
    /// normal grid is a description that wrapped onto a second line. The two need opposite fixes.
    mod investments_missing_field_diagnostics {
        use super::*;
        use pretty_assertions::assert_eq;

        use super::tracing_capture::{Record, field_of, records_matching};

        mod field_probe_rendering {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn a_grid_probe_names_the_cell_one_based_and_the_grid_width() {
                let probe = FieldProbe::Grid { offset: 4, row: 16, col: 5, n_cols: 6 };
                assert_eq!(probe.to_string(), "4 cells past the anchor, row 17 col 6 of a 6-column grid");
            }

            /// Offset zero *is* the anchor, and "0 cells past the anchor" is not a sentence.
            #[test]
            fn the_anchor_probe_drops_the_distance_clause() {
                let probe = FieldProbe::Grid { offset: 0, row: 16, col: 0, n_cols: 6 };
                assert_eq!(probe.to_string(), "row 17 col 1 of a 6-column grid");
            }

            #[test]
            fn a_flat_probe_names_the_block_position_one_based() {
                let probe = FieldProbe::Flat { offset: 2, index: 41 };
                assert_eq!(probe.to_string(), "2 blocks past the anchor, block 42");
            }

            #[test]
            fn the_flat_anchor_probe_also_drops_the_distance_clause() {
                assert_eq!(FieldProbe::Flat { offset: 0, index: 0 }.to_string(), "block 1");
            }

            /// A negative offset is how `quantity: -1` and `acquisition currency: -2` are written in
            /// a format's configuration, so it has to read as well as a positive one.
            #[test]
            fn a_negative_offset_is_rendered_as_written() {
                let probe = FieldProbe::Grid { offset: -1, row: 3, col: 0, n_cols: 4 };
                assert_eq!(probe.to_string(), "-1 cells past the anchor, row 4 col 1 of a 4-column grid");
            }
        }

        mod market_value_missing {
            use super::*;
            use pretty_assertions::assert_eq;

            fn skip_records(f: impl FnOnce()) -> Vec<Record> {
                records_matching(f, "row skipped")
            }

            #[test]
            fn the_warning_names_the_field_the_offset_and_the_cell() {
                let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
                // Offset 9 on a 2-column grid wraps: (0 + 9) / 2 = row 4, (0 + 9) % 2 = column 1.
                let records = skip_records(|| {
                    let _ = simple_investments(9).call(&blks, &targets(&["Acme Corp"]));
                });

                assert_eq!(records.len(), 1, "{records:?}");
                assert_eq!(
                    records[0].message.trim_matches('"'),
                    "the market value is missing: 9 cells past the anchor, row 5 col 2 of a 2-column grid - row skipped"
                );
            }

            /// The coordinates keep pointing at the anchor even though the message talks about
            /// another cell: they are what locates the row on the page, and the audit trail has two
            /// coordinate columns, not four.
            #[test]
            fn the_coordinates_still_name_the_anchor_not_the_probe() {
                let blks = vec![
                    table_row(0, 0, "filler", false),
                    table_row(0, 1, "0", false),
                    table_row(1, 0, "Acme Corp", false),
                    table_row(1, 1, "1.000", false),
                ];
                let records = skip_records(|| {
                    let _ = simple_investments(9).call(&blks, &targets(&["Acme Corp"]));
                });

                assert_eq!(records.len(), 1, "{records:?}");
                assert_eq!(field_of(&records[0], "coord_1"), Some("row 2"));
                assert_eq!(field_of(&records[0], "coord_2"), Some("col 1"));
                assert_eq!(field_of(&records[0], "coord_ref_1"), Some("Acme Corp"));
            }

            /// An empty cell inside the grid, not one beyond its edge: this is the shape a
            /// description wrapped onto two lines produces, and it must be told apart from the one
            /// above by the grid width in the message.
            #[test]
            fn an_empty_cell_within_the_grid_is_reported_at_its_own_coordinates() {
                let blks = vec![
                    table_row(0, 0, "Acme Corp", false),
                    table_row(1, 0, "15/02/2030", false),
                    table_row(1, 1, "1.000", false),
                ];
                let records = skip_records(|| {
                    let _ = simple_investments(1).call(&blks, &targets(&["Acme Corp"]));
                });

                assert_eq!(records.len(), 1, "{records:?}");
                assert!(
                    records[0].message.contains("1 cells past the anchor, row 1 col 2 of a 2-column grid"),
                    "{:?}",
                    records[0].message
                );
            }

            #[test]
            fn a_flat_mode_miss_reports_a_block_position_instead_of_a_cell() {
                let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
                let filter =
                    TextFilterInvestmentsStandard::new(9, None, None, None, None, false, false, DashAsZero::empty())
                        .unwrap();
                let records = skip_records(|| {
                    let _ = filter.call(&blks, &targets(&["Acme Corp"]));
                });

                assert_eq!(records.len(), 1, "{records:?}");
                assert!(
                    records[0].message.contains("9 blocks past the anchor, block 10"),
                    "{:?}",
                    records[0].message
                );
            }
        }

        mod optional_field_missing {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn the_field_stays_null_and_the_row_survives() {
                let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
                let filter =
                    TextFilterInvestmentsStandard::new(1, None, Some(5), None, None, true, false, DashAsZero::empty())
                        .unwrap();
                let out = filter.call(&blks, &targets(&["Acme Corp"])).unwrap();
                assert_eq!(out.len(), 1);
                assert_eq!(out[0].metadata.get("% net assets"), Some(&BlockValue::Null));
            }

            /// The event that makes a silent loss visible. It stays at `debug`, because a report
            /// that simply left the cell blank produces the same `Null` and is not an anomaly.
            #[test]
            fn a_debug_event_says_which_field_and_where_it_looked() {
                let blks = vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)];
                let filter =
                    TextFilterInvestmentsStandard::new(1, None, Some(5), None, None, true, false, DashAsZero::empty())
                        .unwrap();
                let records = records_matching(
                    || {
                        let _ = filter.call(&blks, &targets(&["Acme Corp"]));
                    },
                    "field left empty",
                );

                assert_eq!(records.len(), 1, "{records:?}");
                assert_eq!(records[0].level, "DEBUG");
                assert_eq!(
                    records[0].message.trim_matches('"'),
                    "the % net assets is missing: 5 cells past the anchor, row 3 col 2 of a 2-column grid - field left empty"
                );
            }

            #[test]
            fn a_field_that_is_present_logs_nothing() {
                let blks = vec![
                    table_row(0, 0, "Acme Corp", false),
                    table_row(0, 1, "1.000", false),
                    table_row(0, 2, "12,5", false),
                ];
                let filter =
                    TextFilterInvestmentsStandard::new(1, None, Some(2), None, None, true, false, DashAsZero::empty())
                        .unwrap();
                let records = records_matching(
                    || {
                        let _ = filter.call(&blks, &targets(&["Acme Corp"]));
                    },
                    "field left empty",
                );
                assert!(records.is_empty(), "{records:?}");
            }
        }

        /// The one property that keeps the diagnostic honest: the cell a probe names is the cell the
        /// value was read from. Two separate computations of the same offset is precisely how a log
        /// starts pointing at the wrong place.
        mod probe_matches_read {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn every_offset_reads_the_cell_its_probe_names() {
                // A 3-wide grid whose every cell says where it is, so a value identifies its own
                // origin without ambiguity.
                let mut blks = Vec::new();
                for row in 0..3 {
                    for col in 0..3 {
                        let content = if (row, col) == (0, 0) { "Acme Corp".to_string() } else { format!("r{row}c{col}") };
                        blks.push(table_row(row, col, &content, false));
                    }
                }

                for offset in 1..9 {
                    let out = simple_investments(offset).call(&blks, &targets(&["Acme Corp"])).unwrap();
                    // The anchor is (0, 0), so the probe is a plain division of the offset.
                    let (row, col) = (offset / 3, offset % 3);
                    assert_eq!(
                        out[0].metadata.get("market value"),
                        Some(&BlockValue::from(format!("r{row}c{col}").as_str())),
                        "offset {offset} should read the cell its probe names, ({row}, {col})"
                    );
                }
            }
        }
    }

    mod investments_instrument_detection {
        use super::*;
        use pretty_assertions::assert_eq;

        fn instrument_of(text: &str) -> TextBlock {
            let blks = vec![table_row(0, 0, text, false), table_row(0, 1, "1.000", false)];
            simple_investments(1)
                .call(&blks, &targets(&[text]))
                .unwrap()
                .into_iter()
                .next()
                .expect("the row matches its own text")
        }

        #[test]
        fn a_plain_name_is_an_equity() {
            let blk = instrument_of("Acme Corp");
            assert_eq!(blk.type_block, BlockType::EQUITY_TARGET);
            assert!(!blk.metadata.contains_key("interest rate"));
            assert!(!blk.metadata.contains_key("maturity"));
        }

        #[test]
        fn a_percentage_after_a_leading_letter_makes_it_a_bond() {
            let blk = instrument_of("Acme Corp 3,5 % 2030");
            assert_eq!(blk.type_block, BlockType::BOND_TARGET);
            assert_eq!(blk.metadata.get("interest rate"), Some(&BlockValue::from("3,5 %")));
        }

        #[test]
        fn a_date_makes_it_a_bond_even_without_an_interest_rate() {
            let blk = instrument_of("Acme Corp mat 28/03/2025");
            assert_eq!(blk.type_block, BlockType::BOND_TARGET);
            assert_eq!(blk.metadata.get("maturity"), Some(&BlockValue::from("28/03/2025")));
        }

        #[test]
        fn content_starting_with_a_digit_gets_no_spurious_interest_rate() {
            // The percentage patterns are anchored and require a leading letter, so on a content
            // starting with a digit they must not match — even though `"3.4%"` appears later in the
            // text. An unanchored search would match from `"ITALY"` and invent a field.
            let blk = instrument_of("1,300,000.00 ITALY BTPS 3.4% 23-28/03/2025");
            assert_eq!(blk.type_block, BlockType::BOND_TARGET);
            assert!(
                !blk.metadata.contains_key("interest rate"),
                "no spurious 'interest rate' must be produced"
            );
            assert_eq!(blk.metadata.get("maturity"), Some(&BlockValue::from("28/03/2025")));
        }

        #[test]
        fn newlines_are_stripped_before_the_patterns_are_tried() {
            let blk = instrument_of("Acme\nCorp 3,5 % 2030");
            assert_eq!(blk.type_block, BlockType::BOND_TARGET);
        }

        mod coupon_reading {
            use super::*;
            use pretty_assertions::assert_eq;

            fn rate_of(text: &str) -> String {
                instrument_of(text)
                    .metadata
                    .get("interest rate")
                    .expect("the description carries a coupon")
                    .str_or_fail("interest rate")
                    .expect("the interest rate is text")
                    .to_string()
            }

            #[test]
            fn a_coupon_keeps_every_digit_before_the_point() {
                // A greedy prefix used to swallow the leading digits and read `"0.25%"` here.
                assert_eq!(rate_of("TULLOW OIL PLC 10.25% REGS 15/05/2026"), "10.25%");
            }

            #[test]
            fn a_two_digit_coupon_ending_in_zero_is_not_read_as_zero() {
                assert_eq!(rate_of("PETROLEOS MEXICANOS PEMEX 10.00% 07/02/2033"), "10.00%");
            }

            #[test]
            fn a_three_digit_coupon_survives_too() {
                assert_eq!(rate_of("Acme Corp 100.5% 2030"), "100.5%");
            }

            #[test]
            fn an_integer_coupon_is_read_rather_than_falling_through() {
                // Without the optional decimal group this reached the second pattern, which
                // returned a fragment of the maturity date as the interest rate.
                assert_eq!(rate_of("Enel Fin 1% 17-16.09.24"), "1%");
            }

            #[test]
            fn a_date_before_the_coupon_does_not_steal_the_match() {
                assert_eq!(rate_of("Acme Corp 15.05.2026 mat 3.4%"), "3.4%");
            }

            #[test]
            fn a_single_digit_coupon_is_unchanged_by_the_lazy_prefix() {
                assert_eq!(rate_of("VEOLIA ENVIRONNEMENT SA 'EMTN' 5.625% 09.06.26"), "5.625%");
            }

            #[test]
            fn a_zero_coupon_is_still_read_as_zero() {
                assert_eq!(rate_of("SNAM SPA 'EMTN' 0.00000% 07.12.28"), "0.00000%");
            }

            #[test]
            fn a_two_digit_coupon_after_a_leading_digit_is_still_refused() {
                // The leading-letter anchor outranks the coupon: a content starting with a digit
                // must produce no interest rate at all, however plain the coupon looks.
                let blk = instrument_of("1,300,000.00 TULLOW OIL PLC 10.25% 15/05/2026");
                assert!(!blk.metadata.contains_key("interest rate"));
            }
        }
    }

    mod investments_split_cells {
        use super::*;
        use pretty_assertions::assert_eq;

        /// A company name split across two consecutive blocks of the **same column**, with the
        /// second half alone on row 1.
        ///
        /// "Alone" is what triggers the merge: a cell counts as split only if the probing row has a
        /// **single** occupied column, or if both adjacent columns are empty. The market value
        /// therefore sits on row 0, not row 1, which has to stay occupied by one block only.
        fn split_table(first: &str, second: &str, is_max_width: bool) -> Vec<PdfBlock> {
            vec![
                table_row(0, 0, first, is_max_width),
                table_row(1, 0, second, false),
                table_row(0, 1, "1.000", false),
            ]
        }

        #[test]
        fn a_cell_flagged_max_width_is_joined_with_the_next_one_before_matching() {
            let blks = split_table("Acme ", "Corp", true);
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(
                out[0].metadata.get("company match"),
                Some(&BlockValue::from("Acme Corp"))
            );
        }

        #[test]
        fn a_cell_ending_in_a_space_is_joined_even_without_the_max_width_flag() {
            let blks = split_table("Acme ", "Corp", false);
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out.len(), 1);
        }

        #[test]
        fn a_cell_ending_in_a_newline_is_joined_too() {
            let blks = split_table("Acme\n", "Corp", false);
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out.len(), 1);
        }

        #[test]
        fn a_cell_that_neither_is_max_width_nor_ends_in_whitespace_is_not_joined() {
            let blks = split_table("Acme", "Corp", false);
            assert!(simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap().is_empty());
        }

        #[test]
        fn blocks_in_different_columns_are_never_joined() {
            let blks = vec![
                table_row(0, 0, "Acme ", true),
                table_row(0, 1, "Corp", false),
                table_row(0, 2, "1.000", false),
            ];
            assert!(simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap().is_empty());
        }
    }

    mod investments_page_level {
        use super::*;
        use pretty_assertions::assert_eq;

        fn fund_block(name: &str) -> PdfBlock {
            PdfBlock::bare(BlockType::FUND_NAME, name)
        }

        fn currency_block(text: &str) -> PdfBlock {
            PdfBlock::bare(BlockType::CURRENCY_STATEMENT, text)
        }

        fn rows() -> Vec<PdfBlock> {
            vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)]
        }

        #[test]
        fn the_fund_name_becomes_its_own_text_block_before_the_rows() {
            let mut blks = vec![fund_block("Alpha Fund")];
            blks.extend(rows());
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();

            assert_eq!(out.len(), 2);
            assert_eq!(out[0].type_block, BlockType::FUND);
            assert_eq!(out[1].metadata.get("fund"), Some(&BlockValue::from("Alpha Fund")));
        }

        #[test]
        fn the_declared_currency_is_stamped_on_every_row() {
            let mut blks = vec![currency_block("amounts in EUR")];
            blks.extend(rows());
            let out = simple_investments(1).call(&blks, &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("currency"), Some(&BlockValue::from(Currency::EUR)));
        }

        #[test]
        fn a_page_without_fund_or_currency_leaves_both_null() {
            let out = simple_investments(1).call(&rows(), &targets(&["Acme Corp"])).unwrap();
            assert_eq!(out[0].metadata.get("fund"), Some(&BlockValue::Null));
            assert_eq!(out[0].metadata.get("currency"), Some(&BlockValue::Null));
        }

        #[test]
        fn two_fund_names_in_the_same_page_is_an_error() {
            let blks = vec![fund_block("Alpha"), fund_block("Beta")];
            assert!(matches!(
                simple_investments(1).call(&blks, &targets(&["Acme"])),
                Err(StandardFuncsError::TwoFundsInSamePage)
            ));
        }

        #[test]
        fn two_currency_statements_in_the_same_page_is_an_error() {
            let blks = vec![currency_block("in EUR"), currency_block("in USD")];
            assert!(matches!(
                simple_investments(1).call(&blks, &targets(&["Acme"])),
                Err(StandardFuncsError::TwoCurrenciesInSamePage)
            ));
        }

        #[test]
        fn an_unreadable_currency_fails_the_page_rather_than_the_document() {
            let blks = vec![currency_block("no currency at all")];
            let err = simple_investments(1).call(&blks, &targets(&["Acme"])).unwrap_err();
            assert!(matches!(err, StandardFuncsError::PageParseFail { .. }));
            assert!(err.into_pipe_error("p").is_page_failure());
        }

        #[test]
        fn no_matched_rows_discards_the_fund_block_too() {
            // If the loop produces no rows the result is empty — the fund block, already built, is
            // thrown away with it.
            let mut blks = vec![fund_block("Alpha Fund")];
            blks.extend(rows());
            assert!(
                simple_investments(1).call(&blks, &targets(&["Nobody"])).unwrap().is_empty()
            );
        }

        #[test]
        fn a_page_with_no_blocks_at_all_produces_nothing() {
            assert!(simple_investments(1).call(&[], &targets(&["Acme"])).unwrap().is_empty());
        }

        #[test]
        fn a_page_with_only_a_fund_name_produces_nothing() {
            let blks = vec![fund_block("Alpha Fund")];
            assert!(simple_investments(1).call(&blks, &targets(&["Acme"])).unwrap().is_empty());
        }
    }

    mod investments_malformed_input {
        use super::*;

        #[test]
        fn a_row_without_table_coordinates_is_a_value_error() {
            let blks = vec![PdfBlock::bare(BlockType::TABLE_BODY, "Acme Corp")];
            assert!(matches!(
                simple_investments(0).call(&blks, &targets(&["Acme Corp"])),
                Err(StandardFuncsError::Value(_))
            ));
        }

        #[test]
        fn a_row_whose_content_is_not_text_is_a_value_error() {
            let metadata = BTreeMap::from([
                ("table-row".to_string(), BlockValue::Int(0)),
                ("table-col".to_string(), BlockValue::Int(0)),
                ("is-max-width".to_string(), BlockValue::Bool(false)),
            ]);
            let blks = vec![PdfBlock::new(BlockType::TABLE_BODY, metadata, 42i64)];
            assert!(matches!(
                simple_investments(0).call(&blks, &targets(&["Acme"])),
                Err(StandardFuncsError::Value(_))
            ));
        }
    }

    mod investments_as_a_text_filter_pipe {
        use super::*;
        use pretty_assertions::assert_eq;

        fn rows() -> Vec<PdfBlock> {
            vec![table_row(0, 0, "Acme Corp", false), table_row(0, 1, "1.000", false)]
        }

        #[test]
        fn the_pipe_name_identifies_it_in_errors() {
            assert_eq!(simple_investments(1).name(), "TextFilterInvestmentsStandard");
        }

        #[test]
        fn the_target_companies_come_from_the_filter_data() {
            let companies = targets(&["Acme Corp"]);
            let out = simple_investments(1)
                .filter(&rows(), &FilterData::TargetCompanies(&companies))
                .unwrap();
            assert_eq!(out.len(), 1);
        }

        #[test]
        fn a_later_schedule_step_sees_no_target_companies_and_matches_nothing() {
            // A direct consequence of the `FilterData` semantics: outside the first step this pipe
            // has no companies to match against.
            let previous = Vec::new();
            let out =
                simple_investments(1).filter(&rows(), &FilterData::Previous(&previous)).unwrap();
            assert!(out.is_empty());
        }

        #[test]
        fn an_unreadable_currency_becomes_a_non_fatal_page_failure() {
            let blks = vec![PdfBlock::bare(BlockType::CURRENCY_STATEMENT, "nothing here")];
            let err = simple_investments(1).filter(&blks, &FilterData::EMPTY).unwrap_err();
            assert!(err.is_page_failure());
            assert_eq!(err.pipe(), "TextFilterInvestmentsStandard");
        }

        #[test]
        fn a_malformed_row_becomes_a_fatal_value_error() {
            let blks = vec![PdfBlock::bare(BlockType::TABLE_BODY, "Acme Corp")];
            let companies = targets(&["Acme Corp"]);
            let err = simple_investments(0)
                .filter(&blks, &FilterData::TargetCompanies(&companies))
                .unwrap_err();
            assert!(matches!(err, PipeError::Value { .. }));
            assert!(!err.is_page_failure());
        }
    }


    mod text_filter_sfdr_article {
        use super::*;
        use crate::commons::consts::Currency as Cur;
        use crate::output::classes::investment::{Equity, InvestmentFields};

        fn sfdr_pdf_block(content: &str) -> PdfBlock {
            let metadata = BTreeMap::from([("article".to_string(), BlockValue::from("Art. 8"))]);
            PdfBlock::new(BlockType::SFDR_ARTICLE, metadata, content)
        }

        fn investment_fund(fund: &str) -> Extracted {
            Extracted::Equity(
                Equity::build(InvestmentFields::new(
                    "Acme Corp",
                    "Acme",
                    BlockValue::from(fund),
                    BlockValue::from(1.0),
                    BlockValue::from(Cur::EUR),
                ))
                .unwrap(),
            )
        }

        mod construction {
            use super::*;

            #[test]
            fn empty_prefixes_are_accepted() {
                assert!(TextFilterSfdrArticleStandard::new(vec![], vec![], true).is_ok());
            }

            #[test]
            fn an_invalid_regex_prefix_is_rejected_at_construction() {
                assert!(TextFilterSfdrArticleStandard::new(vec![], vec!["(unterminated".to_string()], true).is_err());
            }
        }

        mod prefix_stripping {
            use super::*;

            #[test]
            fn a_literal_prefix_is_stripped() {
                let filter = TextFilterSfdrArticleStandard::new(vec!["Prefix: ".to_string()], vec![], true).unwrap();
                let previous = vec![investment_fund("Acme Fund")];
                let blks = vec![sfdr_pdf_block("Prefix: Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].content.as_str(), Some("Acme Fund"));
            }

            #[test]
            fn a_regex_prefix_is_stripped_too() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec!["^Prefix \\d+: ".to_string()], true).unwrap();
                let previous = vec![investment_fund("Acme Fund")];
                let blks = vec![sfdr_pdf_block("Prefix 42: Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].content.as_str(), Some("Acme Fund"));
            }

            #[test]
            fn literal_prefixes_are_applied_before_regex_prefixes() {
                // The literal prefix is an **unanchored** substring removal, so it strips `"Foo "`
                // wherever it occurs, not only at the head. With this input the order genuinely
                // matters: applying the literal first removes `"Foo "` from the middle, leaving
                // `"Extra Bar"`, on which the anchored pattern `"^Extra Foo "` no longer finds
                // anything. Applying the pattern first would instead match the whole `"Extra Foo "`
                // prefix of the original and leave `"Bar"`. The two orders give different results,
                // which is what shows the literal really is applied first.
                let filter = TextFilterSfdrArticleStandard::new(
                    vec!["Foo ".to_string()],
                    vec!["^Extra Foo ".to_string()],
                    true,
                )
                .unwrap();
                let previous = vec![investment_fund("Extra Bar")];
                let blks = vec![sfdr_pdf_block("Extra Foo Bar")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].content.as_str(), Some("Extra Bar"));
            }
        }

        mod investment_fund_match {
            use super::*;

            #[test]
            fn a_fund_present_among_resolved_investments_matches() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], true).unwrap();
                let previous = vec![investment_fund("Acme Fund")];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out.len(), 1);
            }

            #[test]
            fn a_fund_not_present_produces_nothing_when_match_is_demanded() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], true).unwrap();
                let previous = vec![investment_fund("Someone Else Fund")];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert!(out.is_empty());
            }

            #[test]
            fn no_match_is_demanded_when_the_flag_is_false() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out.len(), 1);
            }

            #[test]
            fn an_unresolved_investment_fund_does_not_count_as_a_match() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], true).unwrap();
                let pending_equity = Extracted::Equity(
                    Equity::build(InvestmentFields::new(
                        "Acme Corp",
                        "Acme",
                        crate::core::promise::Promise::new("fund-id").into(),
                        BlockValue::from(1.0),
                        BlockValue::from(Cur::EUR),
                    ))
                    .unwrap(),
                );
                let previous = vec![pending_equity];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert!(out.is_empty());
            }
        }

        mod pdf_blocks_and_metadata {
            use super::*;

            #[test]
            fn an_empty_list_of_pdf_blocks_is_an_error() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                assert!(matches!(
                    filter.call(&[], &FilterData::Previous(&previous)),
                    Err(StandardFuncsError::NoPdfBlocks)
                ));
            }

            #[test]
            fn only_the_first_pdf_block_is_used() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                let blks = vec![sfdr_pdf_block("First Fund"), sfdr_pdf_block("Second Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].content.as_str(), Some("First Fund"));
            }

            #[test]
            fn the_metadata_of_the_first_block_is_carried_over() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].metadata.get("article"), Some(&BlockValue::from("Art. 8")));
            }

            #[test]
            fn the_result_is_typed_as_sfdr_article() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let previous: Vec<Extracted> = vec![];
                let blks = vec![sfdr_pdf_block("Acme Fund")];
                let out = filter.call(&blks, &FilterData::Previous(&previous)).unwrap();
                assert_eq!(out[0].type_block, BlockType::SFDR_ARTICLE);
            }
        }

        mod as_a_text_filter_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_errors() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                assert_eq!(filter.name(), "TextFilterSfdrArticleStandard");
            }

            #[test]
            fn an_empty_pdf_block_list_is_a_fatal_error_not_a_page_failure() {
                let filter = TextFilterSfdrArticleStandard::new(vec![], vec![], false).unwrap();
                let err = filter.filter(&[], &FilterData::EMPTY).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }

    mod text_filter_managment_company {
        use super::*;
        use crate::output::classes::fund::Fund;

        fn manco_block(content: &str) -> PdfBlock {
            PdfBlock::bare(BlockType::MANAGEMENT_COMPANY, content)
        }

        #[test]
        fn builds_the_same_block_as_the_standard_txt_blk_helper() {
            let block = manco_block("Acme AM");
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund")), Extracted::Fund(Fund::new("Beta Fund"))];
            let via_filter = TextFilterManagmentCompanyStandard
                .call(std::slice::from_ref(&block), &FilterData::Previous(&previous))
                .unwrap();

            // The expected set has to be derived from the same funds as `previous`, not from
            // independent literals, or it diverges from what the pipe actually writes into
            // `managed_funds` — which uses the name as written.
            let funds: BTreeSet<MatchFund> = previous
                .iter()
                .map(|extracted| MatchFund::new(extracted.as_fund().unwrap().name().unwrap()))
                .collect();
            let expected = standard_management_company_txt_blk(block, &funds);
            assert_eq!(via_filter, vec![expected]);
        }

        #[test]
        fn no_management_company_block_is_a_missing_block_type_error() {
            let previous: Vec<Extracted> = vec![];
            let blks = vec![PdfBlock::bare(BlockType::TABLE_BODY, "irrelevant")];
            assert!(matches!(
                TextFilterManagmentCompanyStandard.call(&blks, &FilterData::Previous(&previous)),
                Err(StandardFuncsError::ExpectedBlockTypeMissing { block_type: "management company" })
            ));
        }

        #[test]
        fn an_empty_list_of_pdf_blocks_is_also_a_missing_block_type_error() {
            let previous: Vec<Extracted> = vec![];
            assert!(matches!(
                TextFilterManagmentCompanyStandard.call(&[], &FilterData::Previous(&previous)),
                Err(StandardFuncsError::ExpectedBlockTypeMissing { block_type: "management company" })
            ));
        }

        #[test]
        fn the_first_management_company_block_is_used_when_several_are_present() {
            let previous: Vec<Extracted> = vec![];
            let blks = vec![manco_block("First"), manco_block("Second")];
            let out =
                TextFilterManagmentCompanyStandard.call(&blks, &FilterData::Previous(&previous)).unwrap();
            assert_eq!(out[0].content.as_str(), Some("First"));
        }

        #[test]
        fn an_unresolved_fund_does_not_contribute_to_managed_funds() {
            let previous = vec![Extracted::Fund(
                Fund::from_value(&BlockValue::Promise(crate::core::promise::Promise::new("id"))).unwrap(),
            )];
            let block = manco_block("Acme AM");
            let out =
                TextFilterManagmentCompanyStandard.call(&[block], &FilterData::Previous(&previous)).unwrap();
            let managed_funds = out[0].metadata.get("managed_funds").unwrap().as_set().unwrap();
            assert!(managed_funds.is_empty());
        }

        mod as_a_text_filter_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_errors() {
                assert_eq!(TextFilterManagmentCompanyStandard.name(), "TextFilterManagmentCompanyStandard");
            }

            #[test]
            fn a_missing_management_company_block_is_a_fatal_error_not_a_page_failure() {
                let err = TextFilterManagmentCompanyStandard.filter(&[], &FilterData::EMPTY).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }

    mod text_filter_assets {
        use super::*;
        use crate::output::classes::fund::Fund;

        fn asset_block(fund: &str, currency: &str, date: Option<&str>) -> PdfBlock {
            let mut metadata = BTreeMap::from([
                ("fund".to_string(), BlockValue::from(fund)),
                ("currency".to_string(), BlockValue::from(currency)),
            ]);
            if let Some(d) = date {
                metadata.insert("date".to_string(), BlockValue::from(d));
            }
            PdfBlock::new(BlockType::RELEVANT_BLOCK, metadata, "")
        }

        fn known_funds() -> Vec<Extracted> {
            vec![Extracted::Fund(Fund::new("Alpha Fund"))]
        }

        mod construction {
            use super::*;

            #[test]
            fn no_date_regex_and_no_removal_patterns_are_accepted() {
                assert!(TextFilterAssetsStandard::new(None, vec![]).is_ok());
            }

            #[test]
            fn a_date_regex_with_exactly_one_capturing_group_is_accepted() {
                assert!(TextFilterAssetsStandard::new(Some(r"(\d{2}/\d{2}/\d{4})"), vec![]).is_ok());
            }

            #[test]
            fn a_date_regex_with_zero_capturing_groups_is_rejected() {
                assert!(TextFilterAssetsStandard::new(Some(r"\d{2}/\d{2}/\d{4}"), vec![]).is_err());
            }

            #[test]
            fn a_date_regex_with_more_than_one_capturing_group_is_rejected() {
                assert!(TextFilterAssetsStandard::new(Some(r"(\d{2})/(\d{2})/\d{4}"), vec![]).is_err());
            }

            #[test]
            fn an_invalid_removal_pattern_is_rejected() {
                assert!(TextFilterAssetsStandard::new(None, vec!["(unterminated".to_string()]).is_err());
            }
        }

        mod fund_filtering {
            use super::*;

            #[test]
            fn a_fund_present_among_resolved_funds_produces_a_block() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out.len(), 1);
            }

            #[test]
            fn a_fund_not_present_produces_nothing_for_that_block_not_an_error() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Nobody's Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert!(out.is_empty());
            }

            #[test]
            fn other_blocks_still_produce_output_when_one_fund_does_not_match() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks =
                    vec![asset_block("Nobody's Fund", "EUR", None), asset_block("Alpha Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out.len(), 1);
            }

            #[test]
            fn remove_from_fund_regexes_are_applied_before_the_match() {
                let filter = TextFilterAssetsStandard::new(None, vec!["^Prefix ".to_string()]).unwrap();
                let blks = vec![asset_block("Prefix Alpha Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out.len(), 1);
                assert_eq!(out[0].metadata.get("fund"), Some(&BlockValue::from("Alpha Fund")));
            }
        }

        mod date_and_currency {
            use super::*;

            #[test]
            fn the_date_regex_extracts_its_single_capturing_group() {
                let filter = TextFilterAssetsStandard::new(Some(r"(\d{2}/\d{2}/\d{4})"), vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", Some("As of 31/12/2024"))];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out[0].metadata.get("date"), Some(&BlockValue::from("31/12/2024")));
            }

            #[test]
            fn a_date_that_does_not_match_the_configured_pattern_is_an_error() {
                let filter = TextFilterAssetsStandard::new(Some(r"(\d{2}/\d{2}/\d{4})"), vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", Some("no date here"))];
                let err = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap_err();
                assert!(matches!(err, StandardFuncsError::DateRegexMismatch { .. }));
            }

            #[test]
            fn without_a_configured_date_regex_the_date_field_is_left_untouched() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", Some("As of 31/12/2024"))];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out[0].metadata.get("date"), Some(&BlockValue::from("As of 31/12/2024")));
            }

            #[test]
            fn the_currency_is_extracted_from_free_text_and_typed() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "Reported in EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out[0].metadata.get("currency"), Some(&BlockValue::from(Currency::EUR)));
            }

            #[test]
            fn an_unreadable_currency_is_an_error() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "no currency here", None)];
                assert!(filter.call(&blks, &FilterData::Previous(&known_funds())).is_err());
            }
        }

        mod result_shape {
            use super::*;

            #[test]
            fn the_result_is_a_relevant_block_with_empty_content() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "EUR", None)];
                let out = filter.call(&blks, &FilterData::Previous(&known_funds())).unwrap();
                assert_eq!(out[0].type_block, BlockType::RELEVANT_BLOCK);
                assert_eq!(out[0].content.as_str(), Some(""));
            }

            #[test]
            fn a_missing_fund_key_is_an_error() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let metadata = BTreeMap::from([("currency".to_string(), BlockValue::from("EUR"))]);
                let blk = PdfBlock::new(BlockType::RELEVANT_BLOCK, metadata, "");
                assert!(filter.call(&[blk], &FilterData::Previous(&known_funds())).is_err());
            }
        }

        mod as_a_text_filter_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_errors() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                assert_eq!(filter.name(), "TextFilterAssetsStandard");
            }

            #[test]
            fn an_unreadable_currency_is_a_fatal_error_through_the_trait_too() {
                let filter = TextFilterAssetsStandard::new(None, vec![]).unwrap();
                let blks = vec![asset_block("Alpha Fund", "no currency here", None)];
                let err = filter.filter(&blks, &FilterData::Previous(&known_funds())).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }
}
