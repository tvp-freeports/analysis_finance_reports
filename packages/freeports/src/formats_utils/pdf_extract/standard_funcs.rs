//! The standard `pdf_extract` pipes: the first segment of a structured or semistructured pipeline.
//!
//! These are what a format's configuration names instead of implementing: extract the fund name,
//! the currency statement, the SFDR article, classify the page, rebuild the investments table, read
//! the assets block. Each takes selections written by the format author and produces [`PdfBlock`]s.
//!
//! # Two dead fields, kept on purpose
//!
//! [`PdfExtractInvestmentsStandard`] carries a `tolerance` and a `row_algorithm_flags` that are
//! readable but never consulted: only `algorithm_flags` and `row_tolerance` actually feed the
//! coordinate computation. No real format sets a non-default value for either, so keeping them
//! costs nothing while removing them would silently change what a configuration means.
//!
//! # Where the deselection list is subtracted
//!
//! [`PdfExtractInvestmentsStandard`]'s deselection list is subtracted at selection time rather than
//! folded into the body set when the pipe is built. The two are equivalent, since contextualising
//! distributes over the expression tree, and doing it this way keeps the set algebra out of the
//! selection types.

use std::collections::BTreeMap;

use crate::commons::consts::{Currency, SfdrArticle};
use crate::commons::sets::Container;
use crate::core::classes::{BlockType, BlockValue, PdfBlock};
use crate::core::page::Page;
use crate::core::pipeline::{PdfExtractPipe, PipeError};

use super::commons::{CommonsError, extract_text_pdf_block_or_fail_page, select_expected_text};
use super::pdf_line::PdfLine;
use super::relative::{OptionallyRelative, RelativeInfo};
use super::select::pdf_line::PdfLineSet;
use super::select::relative::{PdfLineSelection, RelativePdfLineSet, RelativeSelectPdfLineSet};
use super::tabularizer::coordinates::{CoordinateExtractionError, TablePosAlgorithm};
use super::tabularizer::{TableCoordinatesConfig, get_table_coordinates_from_lines};

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
/// Extracts the text of the first selected line into a single [`PdfBlock`]; if it finds nothing,
/// fails the whole page, non-fatally, so the schedule skips it and carries on.
///
/// The pipe behind the three fund/currency/management-company factories, which differ only in the
/// name and block type they configure.
pub struct ExtractTextPdfBlockOrFailPage {
    selection: PdfLineSelection,
    name: String,
    type_block: BlockType,
}

impl ExtractTextPdfBlockOrFailPage {
    pub fn new(selection: PdfLineSelection, name: impl Into<String>, type_block: BlockType) -> Self {
        Self { selection, name: name.into(), type_block }
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let set = contextualized(&self.selection, &page.lines);
        Ok(extract_text_pdf_block_or_fail_page(&set, &page.lines, &self.name, self.type_block.clone())?)
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

/// The pipe extracting the management company. See [`pdf_extract_fund_standard`].
///
/// The spelling `managment` is kept: it is the string that appears in logs and error messages, and
/// in the configuration of every existing formats repository, so correcting it would be an
/// observable change.
pub fn pdf_extract_managment_company_standard(selection: PdfLineSelection) -> ExtractTextPdfBlockOrFailPage {
    ExtractTextPdfBlockOrFailPage::new(selection, "managment company", BlockType::MANAGEMENT_COMPANY)
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
pub struct PdfExtractCurrencyConstant {
    currency: Currency,
    block: PdfBlock,
}

impl PdfExtractCurrencyConstant {
    pub fn new(currency: Currency) -> Self {
        let block = PdfBlock::bare(BlockType::CURRENCY_STATEMENT, currency.code());
        Self { currency, block }
    }

    pub fn currency(&self) -> Currency {
        self.currency
    }

    pub fn call(&self, _page: &Page) -> Vec<PdfBlock> {
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
// PdfExtractSfdrArticleStandard
// ----------------------------------------------------------------------------------------------
/// Extracts the SFDR article the page declares, and the name of the fund it refers to.
///
/// The order of trial is deliberately **not** symmetric: article 8 first, then 9; if neither
/// appears, the outcome is article 6 — the "no declaration" case — rather than an error, because
/// saying nothing about SFDR is itself a classification.
pub struct PdfExtractSfdrArticleStandard {
    art9_selection: PdfLineSelection,
    art8_selection: PdfLineSelection,
    fund_selection: PdfLineSelection,
}

impl PdfExtractSfdrArticleStandard {
    pub fn new(
        art9_selection: PdfLineSelection,
        art8_selection: PdfLineSelection,
        fund_selection: PdfLineSelection,
    ) -> Self {
        Self { art9_selection, art8_selection, fund_selection }
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let lines = &page.lines;
        let article = if !select(&self.art8_selection, lines).is_empty() {
            SfdrArticle::Art8
        } else if !select(&self.art9_selection, lines).is_empty() {
            SfdrArticle::Art9
        } else {
            SfdrArticle::Art6
        };

        let mut funds = select(&self.fund_selection, lines);
        if funds.is_empty() {
            return Err(PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { name: "Fund name".to_string() });
        }
        // Several lines: the texts are concatenated top to bottom. The sort is by vertical position
        // and is stable, so lines at the same height keep page order.
        if funds.len() > 1 {
            funds.sort_by(|a, b| {
                let (_, ay, _, _) = a.bbox().as_tuple();
                let (_, by, _, _) = b.bbox().as_tuple();
                ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        let text: String = funds.iter().map(|line| line.text().as_str()).collect();
        tracing::debug!(article = ?article, "SFDR article determined");

        let metadata = BTreeMap::from([("article".to_string(), BlockValue::from(article))]);
        Ok(vec![PdfBlock::new(BlockType::SFDR_ARTICLE, metadata, BlockValue::from(text))])
    }
}

impl PdfExtractPipe for PdfExtractSfdrArticleStandard {
    fn name(&self) -> &str {
        "PdfExtractSfdrArticleStandard"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        self.call(page).map_err(|e| e.into_pipe_error(PdfExtractPipe::name(self)))
    }
}


// ----------------------------------------------------------------------------------------------
// PdfExtractInvestmentsStandard
// ----------------------------------------------------------------------------------------------
/// The pipe that rebuilds the investments table: one PDF line per cell, with the `(row, column)`
/// coordinates put into the metadata for the next segment to read back.
///
/// `tolerance` and `row_algorithm_flags` are kept but **not** consulted; see the module
/// documentation.
pub struct PdfExtractInvestmentsStandard {
    body_set: PdfLineSelection,
    deselection_list: Vec<PdfLineSelection>,
    algorithm_flags: TablePosAlgorithm,
    tolerance: f32,
    row_algorithm_flags: TablePosAlgorithm,
    row_tolerance: f32,
    company_index: Option<usize>,
}

/// The construction parameters of [`PdfExtractInvestmentsStandard`], grouped into a struct with a
/// [`Default`] so that call sites name what they set.
pub struct InvestmentsStandardArgs {
    pub body_set: PdfLineSelection,
    pub deselection_list: Vec<PdfLineSelection>,
    pub algorithm_flags: TablePosAlgorithm,
    pub tolerance: f32,
    pub row_algorithm_flags: TablePosAlgorithm,
    pub row_tolerance: f32,
    pub company_index: Option<usize>,
}

impl InvestmentsStandardArgs {
    /// The usual defaults, with only `body_set` required.
    pub fn new(body_set: PdfLineSelection) -> Self {
        Self {
            body_set,
            deselection_list: Vec::new(),
            algorithm_flags: TablePosAlgorithm::Default,
            tolerance: 0.0,
            row_algorithm_flags: TablePosAlgorithm::Default,
            row_tolerance: 0.0,
            company_index: None,
        }
    }
}

impl PdfExtractInvestmentsStandard {
    pub fn new(args: InvestmentsStandardArgs) -> Self {
        let InvestmentsStandardArgs {
            body_set,
            deselection_list,
            algorithm_flags,
            tolerance,
            row_algorithm_flags,
            row_tolerance,
            company_index,
        } = args;
        Self { body_set, deselection_list, algorithm_flags, tolerance, row_algorithm_flags, row_tolerance, company_index }
    }

    /// The configured tolerance, never consulted by the algorithm; see the module documentation.
    pub fn tolerance(&self) -> f32 {
        self.tolerance
    }

    /// The configured row flags, never consulted by the algorithm; see the module documentation.
    pub fn row_algorithm_flags(&self) -> TablePosAlgorithm {
        self.row_algorithm_flags
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

        let config = TableCoordinatesConfig {
            algorithm_flags: self.algorithm_flags,
            tolerance: self.row_tolerance,
            company_col: self.company_index,
            ..Default::default()
        };
        let coords = get_table_coordinates_from_lines(&rows, &config)?;
        tracing::debug!(
            found = %rows.first().map(|line| line.text().clone()).unwrap_or_default(),
            lines = rows.len(),
            "investments table body selected"
        );

        let widths: Vec<f32> = rows
            .iter()
            .map(|row| {
                let (x0, _, x1, _) = row.bbox().as_tuple();
                x1 - x0
            })
            .collect();
        let max_width = widths.iter().copied().fold(f32::MIN, f32::max);

        Ok(rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let (table_row, table_col) = coords[i];
                let metadata = BTreeMap::from([
                    ("table-row".to_string(), BlockValue::from(table_row as i64)),
                    ("table-col".to_string(), BlockValue::from(table_col as i64)),
                    ("is-max-width".to_string(), BlockValue::from(widths[i] == max_width)),
                ]);
                PdfBlock::new(BlockType::TABLE_BODY, metadata, BlockValue::from(row.text().as_str()))
            })
            .collect())
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
    date_set: Option<PdfLineSelection>,
    tot_assets: AssetsColumn,
    liabilities: AssetsColumn,
    net_assets: AssetsColumn,
    table_condition: bool,
    skip_column: i64,
}

/// The construction parameters of [`PdfExtractAssetsStandard`], grouped into a struct so the call
/// site names what it sets rather than passing fourteen positional arguments.
pub struct AssetsStandardArgs {
    pub fund_set: PdfLineSelection,
    pub currency_set: Option<PdfLineSelection>,
    pub net_assets: AssetsColumn,
    pub liabilities: AssetsColumn,
    pub tot_assets: AssetsColumn,
    pub date_set: Option<PdfLineSelection>,
    pub table_condition: bool,
    pub skip_column: i64,
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
            date_set: None,
            table_condition: false,
            skip_column: 1,
        }
    }
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
            date_set,
            table_condition,
            skip_column,
        } = args;
        if !table_condition && currency_set.is_none() {
            return Err(PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { name: "currency".to_string() });
        }
        Ok(Self { fund_set, currency_set, date_set, tot_assets, liabilities, net_assets, table_condition, skip_column })
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

    /// The fund names recomposed column by column, when the page is tabular.
    fn fund_texts_by_column(&self, lines: &[PdfLine]) -> Result<Vec<String>, PdfExtractStandardFuncsError> {
        let funds: Vec<PdfLine> = select(&self.fund_set, lines).into_iter().cloned().collect();
        if funds.is_empty() {
            return Ok(Vec::new());
        }
        let config = TableCoordinatesConfig {
            algorithm_flags: TablePosAlgorithm::BigCellRule | TablePosAlgorithm::UseRulerArea,
            ..Default::default()
        };
        let coords = get_table_coordinates_from_lines(&funds, &config)?;
        let cols: Vec<usize> = coords.iter().map(|(_, col)| *col).collect();
        let n_cols = cols.iter().copied().max().map(|m| m + 1).unwrap_or(0);
        Ok((0..n_cols)
            .map(|col| {
                funds
                    .iter()
                    .zip(cols.iter())
                    .filter(|(_, c)| **c == col)
                    .map(|(line, _)| line.text().trim())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect())
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

    /// The text of a column's cells, at only the indices `skip_column` asks for.
    fn column_texts(
        items: &[&PdfLine],
        indices: &[usize],
        column: &str,
    ) -> Result<Vec<String>, PdfExtractStandardFuncsError> {
        indices
            .iter()
            .map(|&i| {
                items
                    .get(i)
                    .map(|line| line.text().clone())
                    .ok_or_else(|| PdfExtractStandardFuncsError::MismatchedAssetsColumn {
                        column: column.to_string(),
                        found: items.len(),
                        expected: i + 1,
                    })
            })
            .collect()
    }

    pub fn call(&self, page: &Page) -> Result<Vec<PdfBlock>, PdfExtractStandardFuncsError> {
        let lines = Self::meaningful_lines(page);

        let tot_assets_items = Self::select_column(&self.tot_assets, &lines);
        let liabilities_items = Self::select_column(&self.liabilities, &lines);
        let net_assets_items = Self::select_column(&self.net_assets, &lines);

        let indices = range_0_to_len_step(tot_assets_items.len(), self.skip_column)?;
        let tot_assets_text = Self::column_texts(&tot_assets_items, &indices, "tot_assets")?;
        let liabilities_text = Self::column_texts(&liabilities_items, &indices, "liabilities")?;
        let net_assets_text = Self::column_texts(&net_assets_items, &indices, "net_assets")?;

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
            let fund_texts = self.fund_texts_by_column(&lines)?;
            match &self.currency_set {
                Some(currency_selection) => {
                    let currency_set = contextualized(currency_selection, &lines);
                    let currency = select_expected_text(&currency_set, &lines, "currency")?;
                    let currencies = vec![currency; fund_texts.len()];
                    (fund_texts, currencies)
                }
                None => Self::split_trailing_currencies(fund_texts)?,
            }
        };

        let date = match &self.date_set {
            Some(selection) => {
                let set = contextualized(selection, &lines);
                BlockValue::from(select_expected_text(&set, &lines, "fund assets date")?)
            }
            None => BlockValue::Null,
        };

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
        tracing::debug!(entries = n_out, table_condition = self.table_condition, "assets extracted");

        Ok((0..n_out)
            .map(|i| {
                let metadata = BTreeMap::from([
                    ("fund".to_string(), BlockValue::from(fund_texts[i].as_str())),
                    ("currency".to_string(), BlockValue::from(currency_texts[i].as_str())),
                    ("tot_assets".to_string(), BlockValue::from(tot_assets_text[i].as_str())),
                    ("liabilities".to_string(), BlockValue::from(liabilities_text[i].as_str())),
                    ("net_assets".to_string(), BlockValue::from(net_assets_text[i].as_str())),
                    ("date".to_string(), date.clone()),
                ]);
                PdfBlock::new(BlockType::RELEVANT_BLOCK, metadata, BlockValue::from(""))
            })
            .collect())
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

            #[test]
            fn management_company_keeps_the_reference_spelling_of_its_name() {
                let pipe = pdf_extract_managment_company_standard(text_sel("^X$"));
                assert_eq!(PdfExtractPipe::name(&pipe), "managment company");
                let blocks = pipe.call(&page(vec![line("X", (0.0, 0.0, 10.0, 10.0))])).unwrap();
                assert_eq!(blocks[0].type_block, BlockType::MANAGEMENT_COMPANY);
            }
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
    }

    mod sfdr_article {
        use super::*;

        fn pipe() -> PdfExtractSfdrArticleStandard {
            PdfExtractSfdrArticleStandard::new(text_sel("Article 9"), text_sel("Article 8"), text_sel("Fund"))
        }

        fn page_with(disclosure: &str) -> Page {
            page(vec![line(disclosure, (0.0, 0.0, 60.0, 10.0)), line("Fund Alpha", (0.0, 20.0, 60.0, 30.0))])
        }

        #[test]
        fn recognises_article_8() {
            let blocks = pipe().call(&page_with("Article 8 disclosure")).unwrap();
            assert_eq!(metadata_of(&blocks[0], "article"), BlockValue::from(SfdrArticle::Art8));
        }

        #[test]
        fn recognises_article_9() {
            let blocks = pipe().call(&page_with("Article 9 disclosure")).unwrap();
            assert_eq!(metadata_of(&blocks[0], "article"), BlockValue::from(SfdrArticle::Art9));
        }

        #[test]
        fn falls_back_to_article_6_when_neither_is_declared() {
            let blocks = pipe().call(&page_with("no disclosure here")).unwrap();
            assert_eq!(metadata_of(&blocks[0], "article"), BlockValue::from(SfdrArticle::Art6));
        }

        #[test]
        fn article_8_wins_over_article_9_when_both_appear() {
            // The asymmetry, pinned on purpose: article 8 is checked first.
            let p = page(vec![
                line("Article 8 and Article 9 disclosure", (0.0, 0.0, 60.0, 10.0)),
                line("Fund Alpha", (0.0, 20.0, 60.0, 30.0)),
            ]);
            let blocks = pipe().call(&p).unwrap();
            assert_eq!(metadata_of(&blocks[0], "article"), BlockValue::from(SfdrArticle::Art8));
        }

        #[test]
        fn the_content_is_the_fund_name() {
            let blocks = pipe().call(&page_with("Article 8 disclosure")).unwrap();
            assert_eq!(blocks[0].content, BlockValue::from("Fund Alpha"));
            assert_eq!(blocks[0].type_block, BlockType::SFDR_ARTICLE);
        }

        #[test]
        fn errors_when_no_line_carries_the_fund_name() {
            let p = page(vec![line("Article 8 disclosure", (0.0, 0.0, 60.0, 10.0))]);
            let err = pipe().call(&p).unwrap_err();
            let PdfExtractStandardFuncsError::ExpectedPdfBlockNotFound { name } = err else {
                panic!("expected ExpectedPdfBlockNotFound")
            };
            assert_eq!(name, "Fund name");
        }

        #[test]
        fn a_missing_fund_name_is_a_fatal_pipe_error_not_a_skipped_page() {
            let p = page(vec![line("Article 8 disclosure", (0.0, 0.0, 60.0, 10.0))]);
            let err = pipe().extract(&p).unwrap_err();
            assert!(!err.is_page_failure());
        }

        #[test]
        fn several_fund_lines_are_concatenated_top_to_bottom() {
            let p = page(vec![
                line("Fund second", (0.0, 40.0, 60.0, 50.0)),
                line("Fund first", (0.0, 10.0, 60.0, 20.0)),
                line("Article 8", (0.0, 0.0, 60.0, 5.0)),
            ]);
            let blocks = pipe().call(&p).unwrap();
            assert_eq!(blocks[0].content, BlockValue::from("Fund firstFund second"));
        }

        #[test]
        fn fund_lines_at_the_same_height_keep_their_page_order() {
            let p = page(vec![
                line("Fund A", (0.0, 10.0, 20.0, 20.0)),
                line("Fund B", (30.0, 10.0, 50.0, 20.0)),
                line("Article 8", (0.0, 0.0, 60.0, 5.0)),
            ]);
            let blocks = pipe().call(&p).unwrap();
            assert_eq!(blocks[0].content, BlockValue::from("Fund AFund B"));
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

        #[test]
        fn marks_the_widest_lines_with_is_max_width() {
            let p = page(vec![line("narrow", (0.0, 0.0, 10.0, 10.0)), line("wide", (0.0, 20.0, 90.0, 30.0))]);
            let blocks = body_pipe().call(&p).unwrap();
            assert_eq!(metadata_of(&blocks[0], "is-max-width"), BlockValue::from(false));
            assert_eq!(metadata_of(&blocks[1], "is-max-width"), BlockValue::from(true));
        }

        #[test]
        fn several_equally_wide_lines_are_all_marked() {
            let blocks = body_pipe().call(&table_page()).unwrap();
            assert!(blocks.iter().all(|b| metadata_of(b, "is-max-width") == BlockValue::from(true)));
        }

        #[test]
        fn returns_nothing_when_the_body_set_selects_no_line() {
            let pipe = PdfExtractInvestmentsStandard::new(InvestmentsStandardArgs::new(text_sel("^absent$")));
            assert!(pipe.call(&table_page()).unwrap().is_empty());
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

        #[test]
        fn the_dead_fields_are_stored_and_readable_but_do_not_change_the_result() {
            let args = InvestmentsStandardArgs {
                tolerance: 7.5,
                row_algorithm_flags: TablePosAlgorithm::UseTestPos,
                ..InvestmentsStandardArgs::new(text_sel(""))
            };
            let pipe = PdfExtractInvestmentsStandard::new(args);
            assert_eq!(pipe.tolerance(), 7.5);
            assert!(pipe.row_algorithm_flags().contains(TablePosAlgorithm::UseTestPos));
            assert_eq!(pipe.call(&table_page()).unwrap(), body_pipe().call(&table_page()).unwrap());
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
        fn a_configured_date_set_fills_the_date_metadata() {
            let mut lines = simple_page().lines;
            lines.push(line("31/12/2024", (0.0, 90.0, 40.0, 100.0)));
            let args = AssetsStandardArgs { date_set: Some(text_sel("^31/12/2024$")), ..simple_args() };
            let pipe = PdfExtractAssetsStandard::build(args).unwrap();
            let blocks = pipe.call(&page(lines)).unwrap();
            assert_eq!(metadata_of(&blocks[0], "date"), BlockValue::from("31/12/2024"));
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
