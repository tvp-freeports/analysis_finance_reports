//! The registry of natively implemented semistructured algorithms.
//!
//! There is one today, in the extraction segment. The registry is therefore a tiny table and a
//! hand-written match: no common signature can unify algorithms with different arguments and
//! different results, and inventing one for a single element would be abstraction for its own sake.

use serde::Deserialize;

use crate::commons::consts::Currency;
use crate::formats_utils::pdf_extract::standard_funcs::{
    ExtractTextPdfBlockOrFailPage, InvestmentsStandardArgs, PdfExtractCurrencyConstant, PdfExtractInvestmentsStandard,
    PdfExtractStandardFuncsError, pdf_extract_fund_standard,
};
use crate::formats_utils::pdf_extract::tabularizer::coordinates::TablePosAlgorithm;
use crate::input::document::selection::{InputPdfLineSet, LineSelectionError, pdfline_selection_from_dict};

use super::SegmentKind;

/// The native names of each segment.
const NATIVE_NAMES: [(SegmentKind, &[&str]); 3] = [
    (SegmentKind::PdfExtract, &["standard_cost_curr"]),
    (SegmentKind::TextFilter, &[]),
    (SegmentKind::Deserialize, &[]),
];

/// The native names registered for a segment.
pub fn names(segment: SegmentKind) -> &'static [&'static str] {
    NATIVE_NAMES.iter().find(|(kind, _)| *kind == segment).map(|(_, names)| *names).unwrap_or(&[])
}

/// Whether `name` is a native algorithm of `segment`.
pub fn contains(segment: SegmentKind, name: &str) -> bool {
    names(segment).contains(&name)
}

/// The YAML configuration of [`standard_cost_curr`].
///
/// The two flag fields are **strings** rather than parsed flag values: in YAML a flag is written by
/// name, and resolving that name is [`TablePosAlgorithm::from_expression`]'s job.
#[derive(Debug, Clone, Deserialize)]
pub struct InputStandardCostCurr {
    #[serde(default)]
    pub deselection_list: Vec<InputPdfLineSet>,
    pub body_set: InputPdfLineSet,
    pub subfund_set: InputPdfLineSet,
    pub currency: Currency,
    #[serde(default)]
    pub algorithm_flags: Option<String>,
    #[serde(default)]
    pub tolerance: f32,
    #[serde(default)]
    pub row_algorithm_flags: Option<String>,
    #[serde(default)]
    pub row_tolerance: f32,
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
}

/// The three extraction pipes a "standard cost and currency" format needs.
///
/// The currency is not written in the document but declared in the configuration, hence a
/// constant-currency pipe in place of the usual one that reads it off the page.
pub fn standard_cost_curr(
    input: &InputStandardCostCurr,
) -> Result<(PdfExtractInvestmentsStandard, ExtractTextPdfBlockOrFailPage, PdfExtractCurrencyConstant), NativeError> {
    let selection = |field: &'static str, spec: &InputPdfLineSet| {
        pdfline_selection_from_dict(spec).map_err(|source| NativeError::LineSelection { field, source })
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

    let args = InvestmentsStandardArgs {
        deselection_list,
        algorithm_flags: flags("algorithm_flags", &input.algorithm_flags)?,
        tolerance: input.tolerance,
        row_algorithm_flags: flags("row_algorithm_flags", &input.row_algorithm_flags)?,
        row_tolerance: input.row_tolerance,
        ..InvestmentsStandardArgs::new(selection("body_set", &input.body_set)?)
    };

    Ok((
        PdfExtractInvestmentsStandard::new(args),
        pdf_extract_fund_standard(selection("subfund_set", &input.subfund_set)?),
        PdfExtractCurrencyConstant::new(input.currency),
    ))
}
