//! The standard `deserialize` pipes: turning surviving text blocks into typed entities.
//!
//! Each pipe reads one kind of block and builds one kind of entity from [`crate::output::classes`].
//! A block of the wrong type is not an error: the pipe has nothing to say and returns nothing,
//! which is how several deserializers coexist in one segment.
//!
//! # Two error policies, deliberately different
//!
//! A **required** field that will not convert loses the whole row: an investment position without a
//! market value is not a position. An **optional** field that will not convert leaves the field
//! empty, logs a warning, and the row survives. One unreadable cell should not cost the holding it
//! belongs to.
//!
//! # A lost holding costs a holding
//!
//! Losing the entity is where it stops. [`DeserializerInvestmentStandard`] absorbs its own failures
//! and returns "nothing to say" rather than propagating them, because one holding is the smallest
//! thing that contains such a failure and a document of a thousand pages must not be thrown away
//! over one cell. Two severities are kept apart: a report writing a dash where a number belongs is
//! expected (`warn`), a value that is there and will not convert is not (`error`). What still
//! propagates is a block of the **wrong type**, which is a mistake in the format rather than in the
//! report.
//!
//! The events say *holding*, not *row*: by the time a block reaches a deserializer the table it may
//! have come from is gone, and what is lost is a position in a portfolio. A row is a fact of the
//! layout, and the word belongs upstream — in `text_filter`, where a `(row, column)` really exists.
//!
//! # Values may arrive already typed
//!
//! Currencies and amounts are accepted either as an already-typed [`BlockValue`] or as a string to
//! be converted. The typed path is the one the real pipeline exercises, since the filtering segment
//! already produces a typed currency; the string path serves hand-built fixtures and formats that
//! do not.
//!
//! # A quirk worth knowing
//!
//! Parentheses are stripped from the `liabilities` text *before* conversion, so `"(200)"` becomes
//! `200.0` and not `-200.0`, despite the accounting convention that parentheses mean a negative.
//! This is long-standing behaviour that the reference outputs depend on, and it is preserved rather
//! than quietly corrected.

use crate::core::classes::value::{BlockValue, BlockValueError};
use crate::core::classes::{BlockType, TextBlock};
use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
use crate::core::schedule::PageClass;
use crate::formats_utils::deserialize::cast::{self, CastError};
use crate::output::classes::OutputClassError;
use crate::output::classes::assets_manager::{InvestmentsManager, ManagementCompany};
use crate::output::classes::fund::Fund;
use crate::output::classes::fund_assets::FundAssets;
use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
use crate::output::classes::investment::{Bond, Equity, InvestmentFields};
use crate::core::tracing_setup::log_error;

#[derive(Debug, thiserror::Error)]
pub enum DeserializeStandardFuncsError {
    #[error(transparent)]
    Value(#[from] BlockValueError),
    #[error("page_type is a {found}, not a string naming a page class")]
    PageTypeNotAString { found: &'static str },
    /// A required metadata field is missing or has an unusable type.
    #[error("required field '{field}' is missing")]
    MissingField { field: &'static str },
    /// A required field will not convert: the row is lost.
    #[error("field '{field}': {source}")]
    LineParseFail {
        field: &'static str,
        #[source]
        source: CastError,
    },
    /// A domain validation of an output entity rejected the value.
    #[error(transparent)]
    OutputClass(#[from] OutputClassError),
}

impl DeserializeStandardFuncsError {
    /// Translates into the engine's error type. The pipe's name cannot be recovered from the error,
    /// so the caller supplies it.
    pub fn into_pipe_error(self, pipe: &str) -> PipeError {
        match self {
            DeserializeStandardFuncsError::Value(source) => PipeError::value(pipe, source),
            other => PipeError::extraction(pipe, other.to_string()),
        }
    }
}

pub struct DeserializerPageClassifyStandard;

impl DeserializerPageClassifyStandard {
    pub fn call(&self, txt_blk: &TextBlock) -> Result<BlockValue, DeserializeStandardFuncsError> {
        Ok(txt_blk.metadata_or_fail("page_type")?.clone())
    }

    /// The `page_type` read by [`DeserializerPageClassifyStandard::call`], translated into the
    /// typed page class the engine expects.
    ///
    /// A null value is the "no class" classification — an `Ok(None)`, not an error: the filtering
    /// pipe always writes that key, null when no block of the page was classified. Any other type
    /// is a configuration error in the formats repository.
    pub fn call_page_class(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<Option<PageClass>, DeserializeStandardFuncsError> {
        match self.call(txt_blk)? {
            BlockValue::Null => Ok(None),
            BlockValue::Str(name) => Ok(Some(PageClass::new(name))),
            other => {
                Err(DeserializeStandardFuncsError::PageTypeNotAString { found: other.kind() })
            }
        }
    }
}

impl DeserializePipe for DeserializerPageClassifyStandard {
    fn name(&self) -> &str {
        "DeserializerPageClassifyStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let class = self.call_page_class(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(vec![Extracted::PageClass(class)])
    }
}


// ----------------------------------------------------------------------------------------------
// DeserializerFundStandard / DeserializerInvestmentStandard
// ----------------------------------------------------------------------------------------------
/// Builds a [`Fund`] from the content of a `FUND` block.
///
/// A block of another type is not an error: the pipe has nothing to say and returns an empty list.
pub struct DeserializerFundStandard;

impl DeserializerFundStandard {
    pub fn call(&self, txt_blk: &TextBlock) -> Result<Option<Fund>, DeserializeStandardFuncsError> {
        if txt_blk.type_block != BlockType::FUND {
            return Ok(None);
        }
        Ok(Some(Fund::from_value(&txt_blk.content)?))
    }
}

impl DeserializePipe for DeserializerFundStandard {
    fn name(&self) -> &str {
        "DeserializerFundStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let fund = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(fund.map(Extracted::Fund).into_iter().collect())
    }
}

/// Where in its table the row sat, if it came from one.
///
/// Written by the text filter, which is the only segment that ever sees the table; absent for a
/// filter that is not table-based, which is why this is an `Option` and not an error. It becomes
/// the `First coord` and `Second coord` columns of the `.log.csv`, the position a reader needs once
/// the triggering text has taken them to the right page.
fn table_position(metadata: &std::collections::BTreeMap<String, BlockValue>) -> Option<(i64, i64)> {
    let row = metadata.get("table row")?.as_int()?;
    let col = metadata.get("table col")?.as_int()?;
    Some((row, col))
}

/// Builds an [`Equity`] or a [`Bond`] from the metadata of an `EQUITY_TARGET` or `BOND_TARGET`
/// block.
///
/// The two error policies described in the module documentation meet here. Required — company,
/// company match, fund, market value, currency — fails the holding, which is skipped with one event
/// naming the company and the field. Optional — quantity, percentage of net assets, acquisition
/// cost and currency — leaves the field empty with a warning, so a single unreadable cell does not
/// cost the whole position.
pub struct DeserializerInvestmentStandard {
    cost_and_value_interpret_int: bool,
    quantity_interpret_float: bool,
}

impl Default for DeserializerInvestmentStandard {
    /// The usual defaults: integer amounts, integer quantities.
    fn default() -> Self {
        Self { cost_and_value_interpret_int: true, quantity_interpret_float: false }
    }
}

impl DeserializerInvestmentStandard {
    pub fn new(cost_and_value_interpret_int: bool, quantity_interpret_float: bool) -> Self {
        Self { cost_and_value_interpret_int, quantity_interpret_float }
    }

    /// Amounts and costs: integers or floats, depending on the format's configuration.
    fn cast_amount(&self, data: &str) -> Result<f64, CastError> {
        if self.cost_and_value_interpret_int { cast::to_int(data).map(|v| v as f64) } else { cast::to_float(data) }
    }

    /// Nominal quantity: float or integer, depending on the format's configuration.
    fn cast_quantity(&self, data: &str) -> Result<f64, CastError> {
        if self.quantity_interpret_float { cast::to_float(data) } else { cast::to_int(data).map(|v| v as f64) }
    }

    /// Applies `cast` to a required value, letting a promise through untouched and accepting an
    /// already-typed value.
    fn required<T>(
        field: &'static str,
        value: Option<&BlockValue>,
        already_typed: impl FnOnce(&BlockValue) -> Option<T>,
        cast: impl FnOnce(&str) -> Result<T, CastError>,
    ) -> Result<BlockValue, DeserializeStandardFuncsError>
    where
        BlockValue: From<T>,
    {
        let value = value.ok_or(DeserializeStandardFuncsError::MissingField { field })?;
        match value {
            BlockValue::Promise(_) => Ok(value.clone()),
            BlockValue::Str(text) => cast(text)
                .map(BlockValue::from)
                .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field, source }),
            other => already_typed(other)
                .map(BlockValue::from)
                .ok_or(DeserializeStandardFuncsError::MissingField { field }),
        }
    }

    /// Like [`Self::required`], but a failure leaves the field empty instead of failing the row.
    fn optional<T>(
        field: &'static str,
        value: Option<&BlockValue>,
        already_typed: impl FnOnce(&BlockValue) -> Option<T>,
        cast: impl FnOnce(&str) -> Result<T, CastError>,
    ) -> Option<BlockValue>
    where
        BlockValue: From<T>,
    {
        match value {
            None | Some(BlockValue::Null) => None,
            Some(promise @ BlockValue::Promise(_)) => Some(promise.clone()),
            Some(BlockValue::Str(text)) => {
                // The span carries the field name into the `Second coord ref` column of the
                // `.log.csv` even for events born **inside** the conversion, which do not know
                // which field they are converting.
                let field_span = tracing::info_span!("field", coord_ref_2 = field);
                let _field_guard = field_span.enter();
                match cast(text) {
                    Ok(v) => Some(BlockValue::from(v)),
                    Err(err) => {
                        // One line saying all three things: the value that will not convert, why,
                        // and that the field is lost. `warn!` rather than `error!`, because the
                        // record survives without this field — it is the "cast failed, field
                        // dropped" case, not one where the requested work was not produced.
                        let data = text.replace('\n', "\\n");
                        tracing::warn!(error = log_error(&err), "could not cast {data:?}: {err} - field skipped");
                        None
                    }
                }
            }
            Some(other) => already_typed(other).map(BlockValue::from),
        }
    }

    pub fn call(&self, txt_blk: &TextBlock) -> Result<Option<Extracted>, DeserializeStandardFuncsError> {
        let is_equity = txt_blk.type_block == BlockType::EQUITY_TARGET;
        let is_bond = txt_blk.type_block == BlockType::BOND_TARGET;
        if !is_equity && !is_bond {
            return Ok(None);
        }
        let md = &txt_blk.metadata;

        let company = cast::to_str(md.get("company").and_then(BlockValue::as_str).unwrap_or_default());
        let company_match = cast::to_str(md.get("company match").and_then(BlockValue::as_str).unwrap_or_default());

        // The `First coord ref` column of the `.log.csv` comes from here. It is set on a span
        // rather than on individual events because it has to hold for *everything* this row's
        // deserialization produces, including events born inside the conversion functions, which
        // have no way of knowing it — and including the two events below, which is why the guard
        // must outlive them and the span is opened here rather than inside `build_row`.
        //
        // The **triggering text**, not the company: the point of the column is to be pasted into a
        // PDF viewer's search box, and only the triggering text is written in the report. The
        // company is the input database's own name for the issuer, and lands in `Investee`.
        //
        // Two spellings of the same span rather than one with empty coordinates: a field that is
        // present but blank still counts as present, and the fields of a span are what decide
        // whether an event under it writes a `.log.csv` row at all.
        let row_span = match table_position(md) {
            Some((row, col)) => {
                let (row, col) = crate::core::tracing_setup::table_coords(row, col);
                tracing::info_span!(
                    "investment",
                    coord_ref_1 = %company_match,
                    coord_1 = %row,
                    coord_2 = %col,
                )
            }
            None => tracing::info_span!("investment", coord_ref_1 = %company_match),
        };
        let _row_guard = row_span.enter();

        match self.build_row(txt_blk, is_equity, company, company_match) {
            Ok(extracted) => Ok(extracted),
            // A block whose *type* is not what this pipe expects is a broken format, not dirty
            // data: nothing about the page would be salvaged by carrying on, so it keeps
            // travelling. Page-level containment stops it one level up.
            Err(err @ DeserializeStandardFuncsError::Value(_)) => Err(err),
            // Expected: the report itself writes "no value here". The holding is lost, but
            // nothing went wrong, hence `warn!` and the marker quoted as it was printed.
            Err(DeserializeStandardFuncsError::LineParseFail {
                field,
                source: CastError::NoDigits { data },
            }) => {
                tracing::warn!(
                    coord_ref_2 = field,
                    "the report writes {data:?} where a number belongs - holding skipped"
                );
                Ok(None)
            }
            // Anomalous: there *is* a value and it cannot be made sense of. One holding is lost;
            // the page and the run are not.
            Err(err) => {
                tracing::error!(error = log_error(&err), "could not build the investment: {err} - holding skipped");
                Ok(None)
            }
        }
    }

    /// The body of [`Self::call`]: everything that can fail on account of what a row *contains*.
    ///
    /// Split out so that the failure is handled where the `investment` span is open — that span is
    /// what carries the triggering text into the `.log.csv`, and without it a skipped holding would
    /// be impossible to find again in the PDF.
    fn build_row(
        &self,
        txt_blk: &TextBlock,
        is_equity: bool,
        company: String,
        company_match: String,
    ) -> Result<Option<Extracted>, DeserializeStandardFuncsError> {
        let md = &txt_blk.metadata;
        let fields = InvestmentFields {
            company,
            company_match,
            fund: md.get("fund").cloned().ok_or(DeserializeStandardFuncsError::MissingField { field: "fund" })?,
            // The nominal quantity is the entity's only non-promisable field: a promise here has
            // nowhere to be kept, so the field is left empty rather than failing the row.
            nominal_quantity: Self::optional("quantity", md.get("quantity"), BlockValue::as_float, |t| {
                self.cast_quantity(t)
            })
            .and_then(|v| v.as_float()),
            market_value: Self::required("market value", md.get("market value"), BlockValue::as_float, |t| {
                self.cast_amount(t)
            })?,
            currency: Self::required("currency", md.get("currency"), BlockValue::as_currency, cast::to_currency)?,
            perc_net_assets: Self::optional("% net assets", md.get("% net assets"), BlockValue::as_float, |t| {
                cast::perc_to_float(t, true)
            }),
            acquisition_cost: Self::optional("acquisition cost", md.get("acquisition cost"), BlockValue::as_float, |t| {
                self.cast_amount(t)
            }),
            acquisition_currency: Self::optional(
                "acquisition currency",
                md.get("acquisition currency"),
                BlockValue::as_currency,
                cast::to_currency,
            ),
        };

        if is_equity {
            return Ok(Some(Extracted::Equity(Equity::build(fields)?)));
        }
        // Maturity and interest rate convert **only** if the key is present, and a conversion
        // failure fails the row: they are not tried-and-dropped like the optional fields.
        let maturity = match md.get("maturity") {
            None | Some(BlockValue::Null) => None,
            Some(BlockValue::Date(date)) => Some(*date),
            Some(value) => Some(
                cast::to_date(value.str_or_fail("maturity")?)
                    .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field: "maturity", source })?,
            ),
        };
        let interest_rate = match md.get("interest rate") {
            None | Some(BlockValue::Null) => None,
            Some(BlockValue::Float(rate)) => Some(rate.into_inner()),
            Some(value) => Some(cast::perc_to_float(value.str_or_fail("interest rate")?, true).map_err(
                |source| DeserializeStandardFuncsError::LineParseFail { field: "interest rate", source },
            )?),
        };
        Ok(Some(Extracted::Bond(Bond::build(fields, maturity, interest_rate)?)))
    }
}

impl DeserializePipe for DeserializerInvestmentStandard {
    fn name(&self) -> &str {
        "DeserializerInvestmentStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let extracted = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(extracted.into_iter().collect())
    }
}


// ----------------------------------------------------------------------------------------------
// The SFDR, manager and assets deserializers
// ----------------------------------------------------------------------------------------------
/// Builds a [`FundSfdrClassification`] from an `SFDR_ARTICLE` block. Does not filter by block type:
/// it always builds, from the content (the fund name) and a required `article` metadata field.
pub struct DeserializeSfdrArticleStandard;

impl DeserializeSfdrArticleStandard {
    pub fn call(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<FundSfdrClassification, DeserializeStandardFuncsError> {
        let fund = txt_blk.content.str_or_fail("content")?;
        let article = txt_blk.metadata_or_fail("article")?;
        Ok(FundSfdrClassification::build(fund, article)?)
    }
}

impl DeserializePipe for DeserializeSfdrArticleStandard {
    fn name(&self) -> &str {
        "DeserializeSfdrArticleStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let classification = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(vec![Extracted::FundSfdrClassification(classification)])
    }
}

/// The body shared by the three manager deserializers: if the block type is not the expected one,
/// nothing is produced; otherwise the content is collapsed — whitespace runs joined into single
/// spaces, not merely trimmed — and passed with the managed funds to the entity's constructor.
fn build_manager<T>(
    txt_blk: &TextBlock,
    expected_type: &crate::core::classes::BlockType,
    ctor: impl Fn(&BlockValue, &BlockValue) -> Result<T, OutputClassError>,
) -> Result<Option<T>, DeserializeStandardFuncsError> {
    if &txt_blk.type_block != expected_type {
        return Ok(None);
    }
    let name = txt_blk.content.str_or_fail("content")?;
    let normalized_name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let managed_funds = txt_blk.metadata_or_fail("managed_funds")?;
    Ok(Some(ctor(&BlockValue::from(normalized_name), managed_funds)?))
}

/// Reads a `MANAGEMENT_COMPANY` block and builds a [`ManagementCompany`].
pub struct DeserializerManagmentCompanyStandard;

impl DeserializerManagmentCompanyStandard {
    pub fn call(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<Option<ManagementCompany>, DeserializeStandardFuncsError> {
        build_manager(txt_blk, &BlockType::MANAGEMENT_COMPANY, ManagementCompany::build)
    }
}

impl DeserializePipe for DeserializerManagmentCompanyStandard {
    fn name(&self) -> &str {
        "DeserializerManagmentCompanyStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let manager = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(manager.map(Extracted::ManagementCompany).into_iter().collect())
    }
}

/// Reads the **same** `MANAGEMENT_COMPANY` block but builds an [`InvestmentsManager`]: different
/// formats use one or the other over the same block type, never both in one pipeline.
pub struct DeserializerInvestmentsManagerFromManco;

impl DeserializerInvestmentsManagerFromManco {
    pub fn call(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<Option<InvestmentsManager>, DeserializeStandardFuncsError> {
        build_manager(txt_blk, &BlockType::MANAGEMENT_COMPANY, InvestmentsManager::build)
    }
}

impl DeserializePipe for DeserializerInvestmentsManagerFromManco {
    fn name(&self) -> &str {
        "DeserializerInvestmentsManagerFromManco"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let manager = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(manager.map(Extracted::InvestmentsManager).into_iter().collect())
    }
}

/// Like [`DeserializerInvestmentsManagerFromManco`], but reads an `INVESTMENTS_MANAGER` block.
pub struct DeserializerInvestmentsManagerStandard;

impl DeserializerInvestmentsManagerStandard {
    pub fn call(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<Option<InvestmentsManager>, DeserializeStandardFuncsError> {
        build_manager(txt_blk, &BlockType::INVESTMENTS_MANAGER, InvestmentsManager::build)
    }
}

impl DeserializePipe for DeserializerInvestmentsManagerStandard {
    fn name(&self) -> &str {
        "DeserializerInvestmentsManagerStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let manager = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(manager.map(Extracted::InvestmentsManager).into_iter().collect())
    }
}

/// The amount converter of [`DeserializerAssetsStandard`]: a shareable function rather than a flag,
/// because a format author may supply an arbitrary one.
pub type NumConverter = std::sync::Arc<dyn Fn(&str) -> Result<f64, CastError> + Send + Sync>;

/// The date converter of [`DeserializerAssetsStandard`], in the same shareable form as
/// [`NumConverter`].
pub type DateConverter =
    std::sync::Arc<dyn Fn(&str) -> Result<crate::commons::date::Date, CastError> + Send + Sync>;

/// Builds a [`FundAssets`] from the relevant block the assets filter produced.
///
/// Both converters are configurable. [`DeserializerAssetsStandard::new`] picks between the built-in
/// integer and float converters, while [`DeserializerAssetsStandard::with_converters`] takes
/// arbitrary ones.
pub struct DeserializerAssetsStandard {
    num_converter: NumConverter,
    date_converter: DateConverter,
}

impl Default for DeserializerAssetsStandard {
    /// **Not fully verified**: interpreting amounts as integers by default is extrapolated from the
    /// investment deserializer's default, not checked against a real format.
    fn default() -> Self {
        Self::new(true, cast::to_date)
    }
}

impl DeserializerAssetsStandard {
    pub fn new(
        interpret_int: bool,
        date_converter: fn(&str) -> Result<crate::commons::date::Date, CastError>,
    ) -> Self {
        Self::with_converters(Self::builtin_num_converter(interpret_int), std::sync::Arc::new(date_converter))
    }

    /// The built-in amount converter that `interpret_int` selects.
    pub fn builtin_num_converter(interpret_int: bool) -> NumConverter {
        if interpret_int {
            std::sync::Arc::new(|text: &str| cast::to_int(text).map(|v| v as f64))
        } else {
            std::sync::Arc::new(|text: &str| cast::to_float(text))
        }
    }

    /// Builds the pipe with arbitrary converters rather than only the built-in ones.
    ///
    /// It exists because a format author's module really does supply its own — something along the
    /// lines of "treat a dash as zero, otherwise parse an integer" — and a boolean cannot represent
    /// that. [`Self::new`] stays the convenient signature for the two built-in cases.
    pub fn with_converters(num_converter: NumConverter, date_converter: DateConverter) -> Self {
        Self { num_converter, date_converter }
    }

    /// Converts amounts with the configured converter: an already-typed value is accepted directly,
    /// a string goes through the converter.
    fn cast_amount(&self, field: &'static str, value: &BlockValue) -> Result<f64, DeserializeStandardFuncsError> {
        match value {
            BlockValue::Str(text) => (self.num_converter)(text)
                .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field, source }),
            other => other
                .as_float()
                .or_else(|| other.as_int().map(|v| v as f64))
                .ok_or(DeserializeStandardFuncsError::MissingField { field }),
        }
    }

    /// Reads a required metadata field, reporting `MissingField` if it is absent or null.
    fn required_field<'a>(
        md: &'a std::collections::BTreeMap<String, BlockValue>,
        field: &'static str,
    ) -> Result<&'a BlockValue, DeserializeStandardFuncsError> {
        match md.get(field) {
            None | Some(BlockValue::Null) => Err(DeserializeStandardFuncsError::MissingField { field }),
            Some(value) => Ok(value),
        }
    }

    pub fn call(&self, txt_blk: &TextBlock) -> Result<FundAssets, DeserializeStandardFuncsError> {
        let md = &txt_blk.metadata;

        let fund = Self::required_field(md, "fund")?.str_or_fail("fund")?.to_string();
        let currency = match Self::required_field(md, "currency")? {
            BlockValue::Currency(c) => BlockValue::Currency(*c),
            BlockValue::Str(text) => BlockValue::from(
                cast::to_currency(text)
                    .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field: "currency", source })?,
            ),
            _ => return Err(DeserializeStandardFuncsError::MissingField { field: "currency" }),
        };

        let tot_assets = self.cast_amount("tot_assets", Self::required_field(md, "tot_assets")?)?;
        let net_assets = self.cast_amount("net_assets", Self::required_field(md, "net_assets")?)?;

        // The parentheses quirk: they are stripped *before* conversion, so `"(200)"` becomes
        // `200.0` rather than `-200.0`. See the module documentation.
        let liabilities_raw = Self::required_field(md, "liabilities")?;
        let liabilities = match liabilities_raw {
            BlockValue::Str(text) => {
                let cleaned = text.replace(['(', ')'], "");
                self.cast_amount("liabilities", &BlockValue::Str(cleaned))?
            }
            other => self.cast_amount("liabilities", other)?,
        };

        let date = match md.get("date") {
            None | Some(BlockValue::Null) => None,
            Some(BlockValue::Date(d)) => Some(BlockValue::Date(*d)),
            Some(value) => {
                let text = value.str_or_fail("date")?;
                Some(BlockValue::from(
                    (self.date_converter)(text)
                        .map_err(|source| DeserializeStandardFuncsError::LineParseFail { field: "date", source })?,
                ))
            }
        };

        Ok(FundAssets::build(fund, tot_assets, liabilities, net_assets, &currency, date.as_ref())?)
    }
}

impl DeserializePipe for DeserializerAssetsStandard {
    fn name(&self) -> &str {
        "DeserializerAssetsStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let assets = self.call(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(vec![Extracted::FundAssets(assets)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::value::BlockValue;
    use crate::core::classes::{BlockType, TextBlock};
    use std::collections::BTreeMap;

    mod deserializer_fund {
        use super::*;
        use crate::core::promise::Promise;

        fn fund_block(content: BlockValue) -> TextBlock {
            TextBlock::from_content(BlockType::FUND, BTreeMap::new(), content)
        }

        #[test]
        fn builds_a_fund_from_the_block_content() {
            let fund = DeserializerFundStandard.call(&fund_block(BlockValue::from("Alpha Fund"))).unwrap();
            assert_eq!(fund.unwrap().name(), Some("ALPHA FUND".to_string()));
        }

        #[test]
        fn a_block_of_another_type_is_skipped_rather_than_rejected() {
            let other = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "whatever");
            assert!(DeserializerFundStandard.call(&other).unwrap().is_none());
        }

        #[test]
        fn a_skipped_block_produces_no_extracted_result() {
            let other = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "whatever");
            assert!(DeserializerFundStandard.deserialize(&other).unwrap().is_empty());
        }

        #[test]
        fn a_matching_block_produces_exactly_one_fund_result() {
            let out = DeserializerFundStandard.deserialize(&fund_block(BlockValue::from("Alpha"))).unwrap();
            assert_eq!(out.len(), 1);
            assert!(out[0].as_fund().is_some());
        }

        #[test]
        fn a_promised_name_is_carried_through_unresolved() {
            let block = fund_block(BlockValue::Promise(Promise::new("fund-id")));
            let fund = DeserializerFundStandard.call(&block).unwrap().unwrap();
            assert!(fund.pending_name().is_some());
        }

        #[test]
        fn a_non_string_content_fails_the_pipe() {
            assert!(DeserializerFundStandard.call(&fund_block(BlockValue::from(1i64))).is_err());
        }
    }

    mod deserializer_investment {
        use super::*;
        use crate::commons::consts::Currency;
        use crate::commons::date::Date;
        use crate::core::promise::Promise;

        fn base_metadata() -> BTreeMap<String, BlockValue> {
            BTreeMap::from([
                ("company".to_string(), BlockValue::from("Acme Corp")),
                ("company match".to_string(), BlockValue::from("Acme")),
                ("fund".to_string(), BlockValue::from("Alpha Fund")),
                ("market value".to_string(), BlockValue::from("1.000")),
                ("currency".to_string(), BlockValue::from(Currency::EUR)),
            ])
        }

        fn block(type_block: BlockType, metadata: BTreeMap<String, BlockValue>) -> TextBlock {
            TextBlock::from_content(type_block, metadata, "")
        }

        fn equity_block(metadata: BTreeMap<String, BlockValue>) -> TextBlock {
            block(BlockType::EQUITY_TARGET, metadata)
        }

        mod dispatch {
            use super::*;

            #[test]
            fn an_equity_target_block_becomes_an_equity() {
                let out = DeserializerInvestmentStandard::default().call(&equity_block(base_metadata())).unwrap();
                assert!(out.unwrap().as_equity().is_some());
            }

            #[test]
            fn a_bond_target_block_becomes_a_bond() {
                let out = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::BOND_TARGET, base_metadata()))
                    .unwrap();
                assert!(out.unwrap().as_bond().is_some());
            }

            #[test]
            fn any_other_block_type_is_skipped() {
                let out = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::FUND, base_metadata()))
                    .unwrap();
                assert!(out.is_none());
            }

            #[test]
            fn a_skipped_block_produces_no_extracted_result() {
                let out = DeserializerInvestmentStandard::default()
                    .deserialize(&block(BlockType::FUND, base_metadata()))
                    .unwrap();
                assert!(out.is_empty());
            }
        }

        mod required_fields {
            use super::*;

            #[test]
            fn the_company_and_its_match_are_normalized_strings() {
                let mut md = base_metadata();
                md.insert("company".to_string(), BlockValue::from("  Acme   Corp  "));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert_eq!(extracted.as_equity().unwrap().data.company, "Acme Corp");
            }

            #[test]
            fn the_market_value_is_cast_as_an_integer_by_default() {
                let extracted =
                    DeserializerInvestmentStandard::default().call(&equity_block(base_metadata())).unwrap().unwrap();
                let value = extracted.as_equity().unwrap().data.market_value.resolved().map(|v| v.into_inner());
                assert_eq!(value, Some(1000.0));
            }

            #[test]
            fn the_market_value_is_cast_as_a_float_when_configured_so() {
                let mut md = base_metadata();
                md.insert("market value".to_string(), BlockValue::from("1.000,50"));
                let deserializer = DeserializerInvestmentStandard::new(false, false);
                let extracted = deserializer.call(&equity_block(md)).unwrap().unwrap();
                let value = extracted.as_equity().unwrap().data.market_value.resolved().map(|v| v.into_inner());
                assert_eq!(value, Some(1000.5));
            }

            #[test]
            fn an_unreadable_market_value_loses_the_whole_line() {
                let mut md = base_metadata();
                md.insert("market value".to_string(), BlockValue::from("not a number"));
                let out = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap();
                assert!(out.is_none());
            }

            #[test]
            fn a_missing_market_value_key_loses_the_whole_line() {
                let mut md = base_metadata();
                md.remove("market value");
                let out = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap();
                assert!(out.is_none());
            }

            #[test]
            fn a_currency_written_as_text_is_cast() {
                let mut md = base_metadata();
                md.insert("currency".to_string(), BlockValue::from("usd"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert_eq!(extracted.as_equity().unwrap().data.currency.resolved(), Some(&Currency::USD));
            }

            #[test]
            fn an_unknown_currency_loses_the_whole_line() {
                let mut md = base_metadata();
                md.insert("currency".to_string(), BlockValue::from("XYZ"));
                assert!(DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().is_none());
            }

            #[test]
            fn a_promised_fund_stays_pending() {
                let mut md = base_metadata();
                md.insert("fund".to_string(), BlockValue::Promise(Promise::new("fund-id")));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.fund.is_pending());
            }

            #[test]
            fn a_null_fund_loses_the_whole_line_like_in_the_reference() {
                let mut md = base_metadata();
                md.insert("fund".to_string(), BlockValue::Null);
                assert!(DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().is_none());
            }
        }

        mod optional_fields {
            use super::*;

            #[test]
            fn an_absent_optional_field_simply_stays_empty() {
                let extracted =
                    DeserializerInvestmentStandard::default().call(&equity_block(base_metadata())).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.perc_net_assets.is_none());
            }

            #[test]
            fn a_null_optional_field_stays_empty_too() {
                let mut md = base_metadata();
                md.insert("% net assets".to_string(), BlockValue::Null);
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.perc_net_assets.is_none());
            }

            #[test]
            fn a_percentage_is_normalized_to_a_fraction() {
                let mut md = base_metadata();
                md.insert("% net assets".to_string(), BlockValue::from("12,5 %"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                let value = extracted.as_equity().unwrap().data.perc_net_assets.as_ref().and_then(|p| p.resolved());
                assert_eq!(value.map(|v| v.into_inner()), Some(0.125));
            }

            #[test]
            fn an_unreadable_optional_field_is_skipped_and_the_line_survives() {
                let mut md = base_metadata();
                md.insert("acquisition cost".to_string(), BlockValue::from("garbage"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.acquisition_cost.is_none());
            }

            #[test]
            fn the_quantity_is_an_integer_by_default() {
                let mut md = base_metadata();
                md.insert("quantity".to_string(), BlockValue::from("1.042"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert_eq!(
                    extracted.as_equity().unwrap().data.nominal_quantity.map(|v| v.into_inner()),
                    Some(1042.0)
                );
            }

            #[test]
            fn a_fractional_quantity_is_skipped_when_the_format_declares_it_integer() {
                // An integer conversion rejects a non-zero fractional part; being an optional
                // field, the row survives and the quantity is left empty.
                let mut md = base_metadata();
                md.insert("quantity".to_string(), BlockValue::from("42,7"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                assert!(extracted.as_equity().unwrap().data.nominal_quantity.is_none());
            }

            #[test]
            fn the_quantity_keeps_its_decimals_when_configured_so() {
                let mut md = base_metadata();
                md.insert("quantity".to_string(), BlockValue::from("42,7"));
                let extracted =
                    DeserializerInvestmentStandard::new(true, true).call(&equity_block(md)).unwrap().unwrap();
                assert_eq!(
                    extracted.as_equity().unwrap().data.nominal_quantity.map(|v| v.into_inner()),
                    Some(42.7)
                );
            }

            #[test]
            fn an_acquisition_currency_written_as_text_is_cast() {
                let mut md = base_metadata();
                md.insert("acquisition currency".to_string(), BlockValue::from("GBP"));
                let extracted = DeserializerInvestmentStandard::default().call(&equity_block(md)).unwrap().unwrap();
                let value = extracted.as_equity().unwrap().data.acquisition_currency.as_ref().and_then(|p| p.resolved());
                assert_eq!(value, Some(&Currency::GBP));
            }
        }

        mod bond_specific_fields {
            use super::*;

            #[test]
            fn the_maturity_is_cast_from_text() {
                let mut md = base_metadata();
                md.insert("maturity".to_string(), BlockValue::from("28/03/2025"));
                let extracted = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::BOND_TARGET, md))
                    .unwrap()
                    .unwrap();
                assert_eq!(extracted.as_bond().unwrap().maturity, Some(Date::new(2025, 3, 28).unwrap()));
            }

            #[test]
            fn the_interest_rate_is_normalized_to_a_fraction() {
                let mut md = base_metadata();
                md.insert("interest rate".to_string(), BlockValue::from("3,5 %"));
                let extracted = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::BOND_TARGET, md))
                    .unwrap()
                    .unwrap();
                assert_eq!(extracted.as_bond().unwrap().interest_rate.map(|v| v.into_inner()), Some(0.035));
            }

            #[test]
            fn a_bond_without_those_keys_gets_none_for_both() {
                let extracted = DeserializerInvestmentStandard::default()
                    .call(&block(BlockType::BOND_TARGET, base_metadata()))
                    .unwrap()
                    .unwrap();
                let bond = extracted.as_bond().unwrap();
                assert!(bond.maturity.is_none() && bond.interest_rate.is_none());
            }

            #[test]
            fn an_unreadable_maturity_loses_the_whole_line_unlike_an_optional_field() {
                let mut md = base_metadata();
                md.insert("maturity".to_string(), BlockValue::from("not a date"));
                let out =
                    DeserializerInvestmentStandard::default().call(&block(BlockType::BOND_TARGET, md)).unwrap();
                assert!(out.is_none());
            }

            #[test]
            fn an_out_of_range_interest_rate_is_rejected_by_the_entity() {
                let mut md = base_metadata();
                md.insert("interest rate".to_string(), BlockValue::from("150 %"));
                let out =
                    DeserializerInvestmentStandard::default().call(&block(BlockType::BOND_TARGET, md)).unwrap();
                assert!(out.is_none());
            }

            #[test]
            fn a_maturity_of_the_wrong_type_still_fails_the_pipe() {
                // Not dirty data but a block that is not shaped as this pipe's contract says: that
                // is a bug in the format, and it keeps travelling.
                let mut md = base_metadata();
                md.insert("maturity".to_string(), BlockValue::from(1i64));
                let err =
                    DeserializerInvestmentStandard::default().call(&block(BlockType::BOND_TARGET, md)).unwrap_err();
                assert!(matches!(err, DeserializeStandardFuncsError::Value(_)));
            }
        }

        /// A holding that cannot be read costs that holding and nothing more. The severities
        /// stay apart — a report writing "no value" is expected, a value that will not convert is
        /// not — and both leave the page and the run alive. The word is *holding* and not *row*
        /// because a table is not what a deserializer sees: it sees a block, and what it fails to
        /// build from it is a position in a portfolio.
        mod holding_containment {
            use super::*;
            use std::sync::{Arc, Mutex};
            use test_case::test_case;
            use tracing::Level;
            use tracing::field::{Field, Visit};
            use tracing_subscriber::Registry;
            use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

            #[derive(Default)]
            struct MessageVisitor(String);

            impl Visit for MessageVisitor {
                fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                    if field.name() == "message" {
                        self.0 = format!("{value:?}");
                    }
                }
            }

            #[derive(Clone, Default)]
            struct CapturingLayer {
                events: Arc<Mutex<Vec<(Level, String)>>>,
            }

            impl<S: tracing::Subscriber> Layer<S> for CapturingLayer {
                fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
                    let mut visitor = MessageVisitor::default();
                    event.record(&mut visitor);
                    self.events.lock().unwrap().push((*event.metadata().level(), visitor.0));
                }
            }

            /// The level and message of every event emitted while `f` ran.
            fn events_of(f: impl FnOnce()) -> Vec<(Level, String)> {
                let layer = CapturingLayer::default();
                let subscriber = Registry::default().with(layer.clone());
                tracing::subscriber::with_default(subscriber, f);
                let events = layer.events.lock().unwrap();
                events.clone()
            }

            fn with_market_value(value: &str) -> TextBlock {
                let mut md = base_metadata();
                md.insert("market value".to_string(), BlockValue::from(value));
                equity_block(md)
            }

            #[test]
            fn a_dash_instead_of_a_market_value_costs_the_holding_not_the_job() {
                let out = DeserializerInvestmentStandard::default().deserialize(&with_market_value("-")).unwrap();
                assert!(out.is_empty());
            }

            #[test]
            fn a_dash_is_expected_enough_to_be_a_warning_naming_what_the_report_wrote() {
                let events = events_of(|| {
                    let _ = DeserializerInvestmentStandard::default().deserialize(&with_market_value("-"));
                });
                let skipped: Vec<_> =
                    events.iter().filter(|(_, message)| message.contains("holding skipped")).collect();
                assert_eq!(skipped.len(), 1, "one holding, one event: {events:?}");
                assert_eq!(skipped[0].0, Level::WARN, "{events:?}");
                assert!(skipped[0].1.contains(r#""-""#), "the marker itself is quoted: {events:?}");
            }

            #[test_case("\u{2013}"; "en dash")]
            #[test_case("n/a"; "not available")]
            #[test_case(""; "empty cell")]
            fn every_no_value_marker_behaves_like_the_dash(marker: &str) {
                let out =
                    DeserializerInvestmentStandard::default().deserialize(&with_market_value(marker)).unwrap();
                assert!(out.is_empty(), "{marker:?}");
            }

            #[test]
            fn garbage_carrying_digits_costs_the_holding_too() {
                // A false positive of the company matcher anchors the row on the wrong column and
                // every offset shifts: this is what a market value then looks like.
                let out = DeserializerInvestmentStandard::default()
                    .deserialize(&with_market_value("CALLEUROBUNDWEEKLY10012025135"))
                    .unwrap();
                assert!(out.is_empty());
            }

            #[test]
            fn garbage_is_anomalous_enough_to_be_an_error() {
                let events = events_of(|| {
                    let _ = DeserializerInvestmentStandard::default()
                        .deserialize(&with_market_value("CALLEUROBUNDWEEKLY10012025135"));
                });
                let skipped: Vec<_> =
                    events.iter().filter(|(_, message)| message.contains("holding skipped")).collect();
                assert_eq!(skipped.len(), 1, "one holding, one event: {events:?}");
                assert_eq!(skipped[0].0, Level::ERROR, "{events:?}");
            }

            #[test]
            fn a_market_value_of_zero_is_a_position_and_is_kept() {
                let out =
                    DeserializerInvestmentStandard::default().deserialize(&with_market_value("0,00")).unwrap();
                assert_eq!(out.len(), 1);
                let value = out[0].as_equity().unwrap().data.market_value.resolved().map(|v| v.into_inner());
                assert_eq!(value, Some(0.0));
            }

            #[test]
            fn a_percentage_outside_its_domain_costs_the_holding_not_the_job() {
                let mut md = base_metadata();
                md.insert("% net assets".to_string(), BlockValue::from(456622.96));
                let out = DeserializerInvestmentStandard::default().deserialize(&equity_block(md)).unwrap();
                assert!(out.is_empty());
            }

            #[test]
            fn a_skipped_holding_does_not_stop_the_blocks_after_it() {
                let deserializer = DeserializerInvestmentStandard::default();
                let good = deserializer.deserialize(&with_market_value("1.000")).unwrap();
                let bad = deserializer.deserialize(&with_market_value("-")).unwrap();
                let good_again = deserializer.deserialize(&with_market_value("2.000")).unwrap();
                assert_eq!((good.len(), bad.len(), good_again.len()), (1, 0, 1));
            }
        }

        /// The anchor the whole row's deserialization hangs from: the `investment` span, whose
        /// `coord_ref_1` becomes the `First coord ref` column of the `.log.csv` for every event
        /// born beneath it.
        mod row_anchor_span {
            use super::*;
            use std::sync::{Arc, Mutex};
            use tracing::field::{Field, Visit};
            use tracing::span;
            use tracing_subscriber::Registry;
            use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

            #[derive(Default)]
            struct AnchorVisitor(BTreeMap<String, String>);

            impl Visit for AnchorVisitor {
                fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                    self.0.insert(field.name().to_string(), format!("{value:?}"));
                }

                fn record_str(&mut self, field: &Field, value: &str) {
                    self.0.insert(field.name().to_string(), value.to_string());
                }
            }

            #[derive(Clone, Default)]
            struct SpanCapturingLayer {
                spans: Arc<Mutex<Vec<BTreeMap<String, String>>>>,
            }

            impl<S: tracing::Subscriber> Layer<S> for SpanCapturingLayer {
                fn on_new_span(&self, attrs: &span::Attributes<'_>, _id: &span::Id, _ctx: Context<'_, S>) {
                    if attrs.metadata().name() != "investment" {
                        return;
                    }
                    let mut visitor = AnchorVisitor::default();
                    attrs.record(&mut visitor);
                    self.spans.lock().unwrap().push(visitor.0);
                }
            }

            /// The fields of every `investment` span opened while `f` ran.
            fn spans_of(f: impl FnOnce()) -> Vec<BTreeMap<String, String>> {
                let layer = SpanCapturingLayer::default();
                let subscriber = Registry::default().with(layer.clone());
                tracing::subscriber::with_default(subscriber, f);
                let spans = layer.spans.lock().unwrap();
                spans.clone()
            }

            /// The `coord_ref_1` of every `investment` span opened while `f` ran.
            fn anchors_of(f: impl FnOnce()) -> Vec<String> {
                spans_of(f).into_iter().filter_map(|mut s| s.remove("coord_ref_1")).collect()
            }

            fn positioned_metadata() -> BTreeMap<String, BlockValue> {
                let mut md = base_metadata();
                md.insert("table row".to_string(), BlockValue::from(42i64));
                md.insert("table col".to_string(), BlockValue::from(3i64));
                md
            }

            /// `base_metadata` deliberately gives the two different values — the report writes
            /// `Acme`, the input database calls the company `Acme Corp` — which is the whole point
            /// of the column: only the former can be searched for inside the PDF.
            #[test]
            fn the_span_is_anchored_on_the_triggering_text_not_on_the_company() {
                let anchors = anchors_of(|| {
                    let _ = DeserializerInvestmentStandard::default().call(&equity_block(base_metadata()));
                });
                assert_eq!(anchors, vec!["Acme".to_string()]);
            }

            /// Where the row sat is written by the text filter into the block's metadata, because
            /// nothing downstream can recover it: this is the span that turns it back into the
            /// `First coord` and `Second coord` columns for every event of the row.
            #[test]
            fn the_table_position_becomes_the_two_coordinate_fields() {
                let spans = spans_of(|| {
                    let _ = DeserializerInvestmentStandard::default()
                        .call(&equity_block(positioned_metadata()));
                });
                assert_eq!(spans.len(), 1, "{spans:?}");
                // The metadata holds the grid's zero-based index; the column holds what a person
                // counts.
                assert_eq!(spans[0].get("coord_1").map(String::as_str), Some("row 43"));
                assert_eq!(spans[0].get("coord_2").map(String::as_str), Some("col 4"));
            }

            /// A deserializer is reachable from a text filter that is not table-based, and a field
            /// that is present but blank still counts as present when the `.log.csv` layer decides
            /// whether to write a row — so the absent case must leave the fields off entirely.
            #[test]
            fn a_block_that_came_from_no_table_carries_no_coordinates_at_all() {
                let spans = spans_of(|| {
                    let _ = DeserializerInvestmentStandard::default().call(&equity_block(base_metadata()));
                });
                assert_eq!(spans.len(), 1, "{spans:?}");
                assert!(!spans[0].contains_key("coord_1"), "{spans:?}");
                assert!(!spans[0].contains_key("coord_2"), "{spans:?}");
            }

            #[test]
            fn a_holding_skipped_mid_row_is_still_anchored_on_the_triggering_text() {
                let mut md = base_metadata();
                md.insert("market value".to_string(), BlockValue::from("-"));
                let anchors = anchors_of(|| {
                    let _ = DeserializerInvestmentStandard::default().call(&equity_block(md));
                });
                assert_eq!(anchors, vec!["Acme".to_string()]);
            }
        }

        mod as_a_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializerInvestmentStandard::default().name(), "DeserializerInvestmentStandard");
            }

            #[test]
            fn a_line_parse_failure_costs_the_holding_and_nothing_else() {
                let mut md = base_metadata();
                md.insert("market value".to_string(), BlockValue::from("nope"));
                let out = DeserializerInvestmentStandard::default().deserialize(&equity_block(md)).unwrap();
                assert!(out.is_empty());
            }

            #[test]
            fn a_wrongly_typed_block_is_still_a_value_error_of_the_pipe() {
                let mut md = base_metadata();
                md.insert("interest rate".to_string(), BlockValue::from(BTreeMap::new()));
                let err = DeserializerInvestmentStandard::default()
                    .deserialize(&block(BlockType::BOND_TARGET, md))
                    .unwrap_err();
                assert!(matches!(err, PipeError::Value { .. }), "{err:?}");
            }
        }
    }


    mod deserialize_sfdr_article {
        use super::*;
        use crate::commons::consts::SfdrArticle;

        fn sfdr_block(content: BlockValue, article: Option<BlockValue>) -> TextBlock {
            let metadata = article.map(|a| BTreeMap::from([("article".to_string(), a)])).unwrap_or_default();
            TextBlock::from_content(BlockType::SFDR_ARTICLE, metadata, content)
        }

        #[test]
        fn builds_a_classification_from_content_and_the_article_metadata() {
            let blk = sfdr_block(BlockValue::from("Alpha Fund"), Some(BlockValue::from(SfdrArticle::Art8)));
            let classification = DeserializeSfdrArticleStandard.call(&blk).unwrap();
            assert_eq!(classification.fund, "Alpha Fund");
            assert_eq!(classification.article.resolved(), Some(&SfdrArticle::Art8));
        }

        #[test]
        fn a_missing_article_key_is_an_error() {
            let blk = sfdr_block(BlockValue::from("Alpha Fund"), None);
            assert!(DeserializeSfdrArticleStandard.call(&blk).is_err());
        }

        #[test]
        fn a_non_string_content_is_an_error() {
            let blk = sfdr_block(BlockValue::from(1i64), Some(BlockValue::from(SfdrArticle::Art6)));
            assert!(DeserializeSfdrArticleStandard.call(&blk).is_err());
        }

        #[test]
        fn a_wrongly_typed_article_is_an_error() {
            let blk = sfdr_block(BlockValue::from("Alpha Fund"), Some(BlockValue::from("Art. 8")));
            assert!(DeserializeSfdrArticleStandard.call(&blk).is_err());
        }

        mod as_a_deserialize_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializeSfdrArticleStandard.name(), "DeserializeSfdrArticleStandard");
            }

            #[test]
            fn a_matching_block_produces_exactly_one_classification_result() {
                let blk = sfdr_block(BlockValue::from("Alpha Fund"), Some(BlockValue::from(SfdrArticle::Art9)));
                let out = DeserializeSfdrArticleStandard.deserialize(&blk).unwrap();
                assert_eq!(out.len(), 1);
                assert!(out[0].as_fund_sfdr_classification().is_some());
            }

            #[test]
            fn a_missing_article_is_a_value_error_not_a_page_failure() {
                let blk = sfdr_block(BlockValue::from("Alpha Fund"), None);
                let err = DeserializeSfdrArticleStandard.deserialize(&blk).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }

    /// The three manager deserializers share one body, differing only in the block type expected
    /// and the entity built.
    mod manager_deserializers {
        use super::*;
        use std::collections::BTreeSet;

        fn managed_funds_value(names: &[&str]) -> BlockValue {
            BlockValue::Set(names.iter().map(|n| BlockValue::from(*n)).collect())
        }

        fn manager_block(
            type_block: BlockType,
            content: &str,
            managed_funds: Option<BlockValue>,
        ) -> TextBlock {
            let metadata = managed_funds
                .map(|f| BTreeMap::from([("managed_funds".to_string(), f)]))
                .unwrap_or_default();
            TextBlock::from_content(type_block, metadata, content)
        }

        mod management_company {
            use super::*;

            #[test]
            fn builds_a_management_company_from_a_matching_block() {
                let blk = manager_block(
                    BlockType::MANAGEMENT_COMPANY,
                    "  Acme   Manager  \n",
                    Some(managed_funds_value(&["Fund A", "Fund B"])),
                );
                let manager = DeserializerManagmentCompanyStandard.call(&blk).unwrap().unwrap();
                assert_eq!(manager.data.name, "Acme Manager");
                assert_eq!(
                    manager.data.managed_funds,
                    BTreeSet::from(["Fund A".to_string(), "Fund B".to_string()])
                );
            }

            #[test]
            fn a_block_of_another_type_is_skipped_rather_than_rejected() {
                let blk = manager_block(BlockType::INVESTMENTS_MANAGER, "Acme", Some(managed_funds_value(&[])));
                assert!(DeserializerManagmentCompanyStandard.call(&blk).unwrap().is_none());
            }

            #[test]
            fn a_skipped_block_produces_no_extracted_result() {
                let blk = manager_block(BlockType::INVESTMENTS_MANAGER, "Acme", Some(managed_funds_value(&[])));
                assert!(DeserializerManagmentCompanyStandard.deserialize(&blk).unwrap().is_empty());
            }

            #[test]
            fn a_missing_managed_funds_key_is_an_error() {
                let blk = manager_block(BlockType::MANAGEMENT_COMPANY, "Acme", None);
                assert!(DeserializerManagmentCompanyStandard.call(&blk).is_err());
            }

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializerManagmentCompanyStandard.name(), "DeserializerManagmentCompanyStandard");
            }
        }

        /// The same block type as the management-company deserializer, but building an
        /// [`InvestmentsManager`].
        mod investments_manager_from_manco {
            use super::*;

            #[test]
            fn builds_an_investments_manager_from_a_management_company_block() {
                let blk = manager_block(
                    BlockType::MANAGEMENT_COMPANY,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                let manager = DeserializerInvestmentsManagerFromManco.call(&blk).unwrap().unwrap();
                assert_eq!(manager.data.name, "Acme Manager");
            }

            #[test]
            fn an_investments_manager_typed_block_is_skipped() {
                let blk = manager_block(
                    BlockType::INVESTMENTS_MANAGER,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                assert!(DeserializerInvestmentsManagerFromManco.call(&blk).unwrap().is_none());
            }

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(
                    DeserializerInvestmentsManagerFromManco.name(),
                    "DeserializerInvestmentsManagerFromManco"
                );
            }
        }

        /// As above, but reading an `INVESTMENTS_MANAGER` block.
        mod investments_manager_standard {
            use super::*;

            #[test]
            fn builds_an_investments_manager_from_an_investments_manager_block() {
                let blk = manager_block(
                    BlockType::INVESTMENTS_MANAGER,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                let manager = DeserializerInvestmentsManagerStandard.call(&blk).unwrap().unwrap();
                assert_eq!(manager.data.name, "Acme Manager");
            }

            #[test]
            fn a_management_company_typed_block_is_skipped() {
                let blk = manager_block(
                    BlockType::MANAGEMENT_COMPANY,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                assert!(DeserializerInvestmentsManagerStandard.call(&blk).unwrap().is_none());
            }

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializerInvestmentsManagerStandard.name(), "DeserializerInvestmentsManagerStandard");
            }

            #[test]
            fn a_matching_block_produces_exactly_one_extracted_result() {
                let blk = manager_block(
                    BlockType::INVESTMENTS_MANAGER,
                    "Acme Manager",
                    Some(managed_funds_value(&["Fund A"])),
                );
                let out = DeserializerInvestmentsManagerStandard.deserialize(&blk).unwrap();
                assert_eq!(out.len(), 1);
                assert!(out[0].as_investments_manager().is_some());
            }
        }
    }

    mod deserializer_assets {
        use super::*;
        use crate::commons::consts::Currency;
        use crate::commons::date::Date;

        fn assets_metadata(overrides: &[(&str, BlockValue)]) -> BTreeMap<String, BlockValue> {
            let mut md = BTreeMap::from([
                ("fund".to_string(), BlockValue::from("Alpha Fund")),
                ("currency".to_string(), BlockValue::from("EUR")),
                ("tot_assets".to_string(), BlockValue::from("1000")),
                ("net_assets".to_string(), BlockValue::from("800")),
                ("liabilities".to_string(), BlockValue::from("200")),
            ]);
            for (k, v) in overrides {
                md.insert((*k).to_string(), v.clone());
            }
            md
        }

        fn assets_block(metadata: BTreeMap<String, BlockValue>) -> TextBlock {
            TextBlock::from_content(BlockType::RELEVANT_BLOCK, metadata, "")
        }

        mod required_fields {
            use super::*;

            #[test]
            fn builds_fund_assets_from_a_fully_populated_block() {
                let assets =
                    DeserializerAssetsStandard::default().call(&assets_block(assets_metadata(&[]))).unwrap();
                assert_eq!(assets.fund, "Alpha Fund");
                assert_eq!(assets.currency.resolved(), Some(&Currency::EUR));
                assert_eq!(assets.tot_assets.into_inner(), 1000.0);
                assert_eq!(assets.net_assets.into_inner(), 800.0);
                assert_eq!(assets.liabilities.into_inner(), 200.0);
            }

            #[test]
            fn a_missing_fund_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("fund");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn a_missing_currency_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("currency");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn a_missing_tot_assets_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("tot_assets");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn a_missing_net_assets_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("net_assets");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn a_missing_liabilities_key_is_an_error() {
                let mut md = assets_metadata(&[]);
                md.remove("liabilities");
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }

            #[test]
            fn an_already_typed_currency_is_accepted_directly() {
                let md = assets_metadata(&[("currency", BlockValue::from(Currency::USD))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(assets.currency.resolved(), Some(&Currency::USD));
            }

            #[test]
            fn an_unknown_currency_text_is_an_error() {
                let md = assets_metadata(&[("currency", BlockValue::from("XYZ"))]);
                assert!(DeserializerAssetsStandard::default().call(&assets_block(md)).is_err());
            }
        }

        mod num_converter {
            use super::*;

            #[test]
            fn interprets_amounts_as_integers_by_default() {
                let md = assets_metadata(&[("tot_assets", BlockValue::from("1.000"))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(assets.tot_assets.into_inner(), 1000.0);
            }

            #[test]
            fn interprets_amounts_as_floats_when_configured_so() {
                let md = assets_metadata(&[
                    ("tot_assets", BlockValue::from("1.000,5")),
                    ("net_assets", BlockValue::from("800,5")),
                ]);
                let deserializer = DeserializerAssetsStandard::new(false, cast::to_date);
                let assets = deserializer.call(&assets_block(md)).unwrap();
                assert_eq!(assets.tot_assets.into_inner(), 1000.5);
            }

            #[test]
            fn an_unreadable_amount_loses_the_whole_line() {
                let md = assets_metadata(&[("tot_assets", BlockValue::from("not a number"))]);
                let err = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap_err();
                assert!(matches!(err, DeserializeStandardFuncsError::LineParseFail { field: "tot_assets", .. }));
            }
        }

        /// The quirk preserved on purpose: parentheses are stripped from `liabilities` **before**
        /// conversion, so `"(200)"` becomes `200.0`, not `-200.0`.
        mod liabilities_parentheses_quirk {
            use super::*;

            #[test]
            fn parenthesized_liabilities_become_positive_not_negative() {
                let md = assets_metadata(&[("liabilities", BlockValue::from("(200)"))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(assets.liabilities.into_inner(), 200.0);
            }

            #[test]
            fn liabilities_without_parentheses_are_unaffected() {
                let md = assets_metadata(&[("liabilities", BlockValue::from("200"))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(assets.liabilities.into_inner(), 200.0);
            }
        }

        mod date_field {
            use super::*;

            #[test]
            fn an_absent_date_key_becomes_none_not_an_error() {
                let md = assets_metadata(&[]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert!(assets.date.is_none());
            }

            #[test]
            fn a_null_date_becomes_none_too() {
                let md = assets_metadata(&[("date", BlockValue::Null)]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert!(assets.date.is_none());
            }

            #[test]
            fn a_textual_date_is_converted_with_the_configured_converter() {
                let md = assets_metadata(&[("date", BlockValue::from("2024-12-31"))]);
                let assets = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap();
                assert_eq!(
                    assets.date.and_then(|d| d.resolved().copied()),
                    Some(Date::new(2024, 12, 31).unwrap())
                );
            }

            #[test]
            fn an_unreadable_date_loses_the_whole_line() {
                let md = assets_metadata(&[("date", BlockValue::from("not a date"))]);
                let err = DeserializerAssetsStandard::default().call(&assets_block(md)).unwrap_err();
                assert!(matches!(err, DeserializeStandardFuncsError::LineParseFail { field: "date", .. }));
            }
        }

        mod as_a_deserialize_pipe {
            use super::*;

            #[test]
            fn the_pipe_name_identifies_it_in_error_messages() {
                assert_eq!(DeserializerAssetsStandard::default().name(), "DeserializerAssetsStandard");
            }

            #[test]
            fn a_matching_block_produces_exactly_one_extracted_result() {
                let out = DeserializerAssetsStandard::default()
                    .deserialize(&assets_block(assets_metadata(&[])))
                    .unwrap();
                assert_eq!(out.len(), 1);
                assert!(out[0].as_fund_assets().is_some());
            }

            #[test]
            fn a_missing_required_field_is_a_value_error_not_a_page_failure() {
                let mut md = assets_metadata(&[]);
                md.remove("fund");
                let err = DeserializerAssetsStandard::default().deserialize(&assets_block(md)).unwrap_err();
                assert!(!err.is_page_failure());
            }
        }
    }

    mod deserializer_page_classify {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn reads_a_present_page_type() {
            let mut metadata = BTreeMap::new();
            metadata.insert("page_type".to_string(), BlockValue::Str("investments".to_string()));
            let txt = TextBlock::from_content(BlockType::PAGE_CLASS, metadata, "");

            let result = DeserializerPageClassifyStandard.call(&txt).unwrap();
            assert_eq!(result, BlockValue::Str("investments".to_string()));
        }

        #[test]
        fn a_present_but_null_page_type_is_ok_not_an_error() {
            let mut metadata = BTreeMap::new();
            metadata.insert("page_type".to_string(), BlockValue::Null);
            let txt = TextBlock::from_content(BlockType::PAGE_CLASS, metadata, "");

            let result = DeserializerPageClassifyStandard.call(&txt).unwrap();
            assert_eq!(result, BlockValue::Null);
        }

        #[test]
        fn a_missing_page_type_key_is_an_error() {
            let txt = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "");
            assert!(DeserializerPageClassifyStandard.call(&txt).is_err());
        }
    }

    /// The same pipe seen as a [`DeserializePipe`], which is how the engine uses it.
    mod as_a_deserialize_pipe {
        use super::*;
        use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
        use crate::core::schedule::PageClass;
        use pretty_assertions::assert_eq;

        fn block_with(page_type: BlockValue) -> TextBlock {
            let metadata = BTreeMap::from([("page_type".to_string(), page_type)]);
            TextBlock::from_content(BlockType::PAGE_CLASS, metadata, "")
        }

        #[test]
        fn a_string_page_type_becomes_a_page_class() {
            let out = DeserializerPageClassifyStandard
                .deserialize(&block_with(BlockValue::from("investments")))
                .unwrap();
            assert_eq!(out, vec![Extracted::PageClass(Some(PageClass::new("investments")))]);
        }

        #[test]
        fn a_null_page_type_becomes_an_explicitly_unclassified_page() {
            let out =
                DeserializerPageClassifyStandard.deserialize(&block_with(BlockValue::Null)).unwrap();
            assert_eq!(out, vec![Extracted::PageClass(None)]);
        }

        #[test]
        fn a_page_type_of_the_wrong_type_is_a_pipe_error_naming_the_pipe() {
            let err = DeserializerPageClassifyStandard
                .deserialize(&block_with(BlockValue::from(3i64)))
                .unwrap_err();
            assert_eq!(err.pipe(), "DeserializerPageClassifyStandard");
            assert!(matches!(err, PipeError::Extraction { .. }));
        }

        #[test]
        fn a_missing_page_type_key_is_a_value_error_not_a_page_failure() {
            let block = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "");
            let err = DeserializerPageClassifyStandard.deserialize(&block).unwrap_err();
            assert!(matches!(err, PipeError::Value { .. }));
            assert!(!err.is_page_failure());
        }
    }
}
