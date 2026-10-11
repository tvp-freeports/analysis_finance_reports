//! The registry of natively implemented semistructured algorithms.
//!
//! A table of names per segment and a hand-written match in [`super::build_native`]: no common
//! signature can unify algorithms with different arguments and different results, and inventing one
//! would buy nothing that the match does not already give.
//!
//! # Algorithms that take no arguments
//!
//! Most of the `deserialize` segment is in this shape — [`DeserializerFundStandard`] and its
//! siblings are unit structs, and a repository that names one has nothing left to say about it. For
//! those, the mapping cell *is* the whole configuration, so [`ARGLESS_NAMES`] lets the YAML entry be
//! absent rather than demanding a `{}` that carries no information. An algorithm not listed there
//! still requires its entry: a missing one is then a configuration mistake, not a default.

pub mod assets_by_position;
pub mod carryover;
pub mod esg;
pub mod grouped_lines;
pub mod inv_managers;
pub mod labelled_value;
pub mod page_number;
pub mod renaming;
pub mod section;
pub mod selected_text;
pub mod selection;
pub mod sfdr_annex;

use std::sync::Arc;

use serde::Deserialize;

use crate::commons::consts::Currency;
use crate::core::pipeline::{DeserializePipe, PdfExtractPipe};
use crate::formats_utils::deserialize::standard_funcs::{
    DeserializeSfdrArticleStandard, DeserializerFundStandard, DeserializerInvestmentStandard,
    DeserializerInvestmentsManagerFromManco, DeserializerInvestmentsManagerStandard,
    DeserializerManagmentCompanyStandard, DeserializerPageClassifyStandard,
};
use crate::formats_utils::pdf_extract::position::SplittingDirection;
use crate::formats_utils::pdf_extract::standard_funcs::{
    InvestmentsStandardArgs, PdfExtractCurrencyConstant, PdfExtractInvestmentsStandard,
    PdfExtractStandardFuncsError, TableSettings, pdf_extract_currency_standard,
    pdf_extract_fund_standard,
};
use crate::formats_utils::pdf_extract::tabularizer::collapse::CollapseAlgorithm;
use crate::formats_utils::pdf_extract::tabularizer::coordinates::TablePosAlgorithm;
use crate::formats_utils::text_filter::managment_company::AmongLines;
use crate::input::document::selection::{
    InputPdfLineSet, LineSelectionError, pdfline_selection_from_dict,
};

use super::SegmentKind;

/// The native names of each segment.
const NATIVE_NAMES: [(SegmentKind, &[&str]); 3] = [
    (
        SegmentKind::PdfExtract,
        &[
            "investments",
            "currency_constant",
            "sfdr_annex",
            "labelled_value",
            "fund",
            "currency",
            "section",
            "page_number",
            "grouped_lines",
            "assets_by_position",
        ],
    ),
    (
        SegmentKind::TextFilter,
        &[
            "sfdr_annex",
            "manco",
            "manco_imposed",
            "inv_managers",
            "labelled_assets",
            "esg",
            "renaming",
            "assets",
        ],
    ),
    (
        SegmentKind::Deserialize,
        &[
            "fund",
            "managment_company",
            "investments_manager",
            "investments_manager_from_manco",
            "investment",
            "page_classify",
            "sfdr_article",
            "assets",
            "carryover_deposit",
            "esg_indicator",
            "renaming",
        ],
    ),
];

/// The native names that need no YAML entry at all. See the module documentation.
///
/// The spelling `managment_company` keeps the engine's own misspelling, as the pipe and every
/// existing repository's configuration do: correcting it here alone would make two names for one
/// algorithm.
const ARGLESS_NAMES: [(SegmentKind, &[&str]); 3] = [
    (SegmentKind::PdfExtract, &[]),
    (SegmentKind::TextFilter, &["inv_managers", "assets"]),
    (
        SegmentKind::Deserialize,
        &[
            "fund",
            "managment_company",
            "investments_manager",
            "investments_manager_from_manco",
            "investment",
            "page_classify",
            "sfdr_article",
            // Both of its cells have the engine's own default — `int` and the numeric date — so a
            // statement written the ordinary way names it and says nothing else.
            "assets",
            "esg_indicator",
            // Its one cell is the language of the months, and `en_month` is the default the
            // template had: three of the four measured sections are in English.
            "renaming",
        ],
    ),
];

/// The native names registered for a segment.
pub fn names(segment: SegmentKind) -> &'static [&'static str] {
    NATIVE_NAMES
        .iter()
        .find(|(kind, _)| *kind == segment)
        .map(|(_, names)| *names)
        .unwrap_or(&[])
}

/// Whether `name` is a native algorithm of `segment`.
pub fn contains(segment: SegmentKind, name: &str) -> bool {
    names(segment).contains(&name)
}

/// Whether `name` is a native algorithm of `segment` that takes no arguments, and whose YAML entry
/// may therefore be missing.
pub fn is_argless(segment: SegmentKind, name: &str) -> bool {
    ARGLESS_NAMES
        .iter()
        .find(|(kind, _)| *kind == segment)
        .is_some_and(|(_, names)| names.contains(&name))
}

/// The YAML configuration of the management-company reader.
///
/// `after` and `before` are **fragments of a regular expression**, not literal text: a parenthesis
/// in them has to be protected. Leaving `after` out means *from the start of the line*, leaving
/// `before` out *to its end*; leaving out both would take the whole line, which is what the
/// standard filter already does, so a configuration that states neither is accepted and simply
/// reads everything.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InputManco {
    pub after: String,
    pub before: String,
    /// `first` (the default) or `most_recent`, which is `U6`.
    pub among: Option<String>,
}

/// The YAML configuration of the imposed management company: the name the format declares.
#[derive(Debug, Clone, Deserialize)]
pub struct InputMancoImposed {
    pub name: String,
}

/// Which candidate line a configuration asks for.
pub fn among_lines(name: Option<&str>) -> Option<AmongLines> {
    match name.unwrap_or("first") {
        "first" => Some(AmongLines::First),
        "most_recent" => Some(AmongLines::MostRecent),
        _ => None,
    }
}

/// The YAML configuration of the constant-currency pipe: the one thing it has to be told.
#[derive(Debug, Clone, Deserialize)]
pub struct InputCurrencyConstant {
    pub currency: Currency,
}

/// The YAML configuration of [`investments`]: the three pipes a holdings table needs.
///
/// Two cells are required — the body of the table and the column of sub-fund names — and the
/// currency, which is either **declared** by the format or **read** off the page.
///
/// The flag field is a **string** rather than a parsed flag value: in YAML a flag is written by
/// name, and resolving that name is [`TablePosAlgorithm::from_expression`]'s job.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputInvestments {
    #[serde(default)]
    pub deselection_list: Vec<InputPdfLineSet>,
    pub body_set: InputPdfLineSet,
    pub subfund_set: InputPdfLineSet,
    /// The separator that joins **every** selected line into the sub-fund's name, where the page
    /// splits it into several text objects. Absent means the first line alone (`R24`).
    #[serde(default)]
    pub subfund_join: Option<String>,
    /// The currency the format **declares**, where the page does not state it. Exactly one of this
    /// and `currency_set`.
    #[serde(default)]
    pub currency: Option<Currency>,
    /// Where the page **states** the currency. Exactly one of this and `currency`.
    #[serde(default)]
    pub currency_set: Option<InputPdfLineSet>,
    /// Which column of the table holds the company, where it is not the first one.
    #[serde(default)]
    pub company_index: Option<usize>,
    #[serde(default)]
    pub algorithm_flags: Option<String>,
    /// How far apart two cells may sit and still share a **column**.
    #[serde(default)]
    pub col_tolerance: f32,
    /// The same for a **row**. Independent: leaving it out means zero, not `col_tolerance`.
    #[serde(default)]
    pub row_tolerance: f32,
    /// The columns whose cells may break across several lines — the columns of names, since a long
    /// name wraps and a number does not. Naming them asks the pipe for one block per **cell**.
    ///
    /// The same cell the structured level calls `Collapse`, as a list because a page carrying two
    /// tables side by side has two columns of names. Empty is no collapse, which is what every
    /// configuration written before this said.
    #[serde(default)]
    pub collapse_cols: Vec<usize>,
    /// `down` (the default) puts a broken cell on the **first** row of its run, `up` on the last.
    #[serde(default)]
    pub collapse_direction: Option<String>,
}

/// Failures of building a native algorithm.
#[derive(Debug, thiserror::Error)]
pub enum NativeError {
    #[error("field '{field}': {source}")]
    LineSelection {
        field: &'static str,
        #[source]
        source: LineSelectionError,
    },
    #[error("field '{field}': {source}")]
    AlgorithmFlags {
        field: &'static str,
        #[source]
        source: crate::commons::flag_expr::FlagExprError,
    },
    #[error(transparent)]
    Pipe(#[from] PdfExtractStandardFuncsError),
    #[error("field 'collapse_direction': must be 'down' or 'up', found '{found}'")]
    CollapseDirection { found: String },
    /// The currency is either declared or read, and a configuration saying both says one of them
    /// for nothing — while one saying neither leaves the holdings without a currency, which the
    /// output schema requires.
    #[error(
        "exactly one of 'currency' and 'currency_set': the currency is either declared by the format or read off the page"
    )]
    Currency,
}

/// The three extraction pipes a holdings table needs: the table, the sub-fund, the currency.
///
/// They are one name and not three because they always travel together — a table of holdings
/// belongs to a sub-fund and is expressed in a currency, and a page that gives one gives all three.
/// The currency is the one that differs: some formats print it on the page and others do not print
/// it at all, in which case the format declares it and the pipe warns (`R3`).
pub fn investments(input: &InputInvestments) -> Result<Vec<Arc<dyn PdfExtractPipe>>, NativeError> {
    let selection = |field: &'static str, spec: &InputPdfLineSet| {
        pdfline_selection_from_dict(spec)
            .map_err(|source| NativeError::LineSelection { field, source })
    };
    let flags = |field: &'static str, expression: &Option<String>| match expression {
        None => Ok(TablePosAlgorithm::Default),
        Some(expression) => TablePosAlgorithm::from_expression(expression)
            .map_err(|source| NativeError::AlgorithmFlags { field, source }),
    };

    let mut deselection_list = Vec::with_capacity(input.deselection_list.len());
    for spec in &input.deselection_list {
        deselection_list.push(selection("deselection_list", spec)?);
    }

    let collapse_direction = match input.collapse_direction.as_deref().unwrap_or("down") {
        "down" => SplittingDirection::Down,
        "up" => SplittingDirection::Up,
        other => {
            return Err(NativeError::CollapseDirection {
                found: other.to_string(),
            });
        }
    };

    let args = InvestmentsStandardArgs {
        deselection_list,
        company_index: input.company_index,
        table: TableSettings {
            algorithm_flags: flags("algorithm_flags", &input.algorithm_flags)?,
            col_tolerance: input.col_tolerance,
            row_tolerance: input.row_tolerance,
            collapse: !input.collapse_cols.is_empty(),
            collapse_algorithm: CollapseAlgorithm::Pattern,
            collapse_direction,
            collapse_cols: input.collapse_cols.clone(),
            ..TableSettings::default()
        },
        ..InvestmentsStandardArgs::new(selection("body_set", &input.body_set)?)
    };

    let fund = {
        let pipe = pdf_extract_fund_standard(selection("subfund_set", &input.subfund_set)?);
        match &input.subfund_join {
            Some(separator) => pipe.joining(separator.clone()),
            None => pipe,
        }
    };

    // Exactly one of the two, because the currency is either the page's or the format's and a
    // configuration that says both says one of them for nothing.
    let currency: Arc<dyn PdfExtractPipe> = match (input.currency, &input.currency_set) {
        (Some(currency), None) => Arc::new(PdfExtractCurrencyConstant::new(currency)),
        (None, Some(spec)) => Arc::new(pdf_extract_currency_standard(selection(
            "currency_set",
            spec,
        )?)),
        _ => return Err(NativeError::Currency),
    };

    Ok(vec![
        Arc::new(PdfExtractInvestmentsStandard::new(args)),
        Arc::new(fund),
        currency,
    ])
}

/// The `deserialize` pipe of an argument-less native name.
///
/// `None` for a name this segment does not register, which [`super::resolve`] has already ruled
/// out: the caller treats it as unreachable rather than as a configuration error.
///
/// `investment` is **not** here although every call site constructs it bare: it carries two flags
/// with defaults, so a repository may want to say something about it. It is argless in the sense
/// that matters — its entry may be missing — but it is built from the entry when there is one.
pub fn argless_deserialize(name: &str) -> Option<Arc<dyn DeserializePipe>> {
    Some(match name {
        "fund" => Arc::new(DeserializerFundStandard) as Arc<dyn DeserializePipe>,
        "managment_company" => Arc::new(DeserializerManagmentCompanyStandard),
        "investments_manager" => Arc::new(DeserializerInvestmentsManagerStandard),
        "investments_manager_from_manco" => Arc::new(DeserializerInvestmentsManagerFromManco),
        "page_classify" => Arc::new(DeserializerPageClassifyStandard),
        "sfdr_article" => Arc::new(DeserializeSfdrArticleStandard),
        "esg_indicator" => Arc::new(esg::deserialize()),
        _ => return None,
    })
}

/// The YAML configuration of the standard investment deserializer: the two casts it can be told to
/// read differently. Both default to what [`DeserializerInvestmentStandard::default`] chooses, so
/// an absent entry and an empty one mean the same thing.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InputInvestment {
    pub cost_and_value_interpret_int: Option<bool>,
    pub quantity_interpret_float: Option<bool>,
}

impl InputInvestment {
    /// The pipe this configuration describes, each unstated flag taking the standard default.
    pub fn build(&self) -> DeserializerInvestmentStandard {
        let standard = DeserializerInvestmentStandard::default();
        DeserializerInvestmentStandard::new(
            self.cost_and_value_interpret_int
                .unwrap_or(standard.cost_and_value_interpret_int()),
            self.quantity_interpret_float
                .unwrap_or(standard.quantity_interpret_float()),
        )
    }
}
