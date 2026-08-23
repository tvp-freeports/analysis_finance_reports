//! Il registro degli algoritmi semistructured implementati nativamente.
//!
//! "Semistructured" sta a metà fra structured e unstructured: l'algoritmo è nella libreria, come
//! nel primo, ma è **nominato** e riceve una configurazione ricca (YAML, non colonne di CSV), come
//! nel secondo. Il repo formati lo richiama per nome da `formats_mapping.csv`.
//!
//! Oggi esiste un solo algoritmo nativo, [`standard_cost_curr`], ed è nel segmento `pdf_extract`.
//! Il registro è quindi una tabella minuscola e un `match` scritto a mano: non c'è una firma
//! comune che possa unificare algoritmi con argomenti e risultati diversi, e inventarne una per un
//! solo elemento sarebbe astrazione a vuoto — è la stessa scelta del riferimento.

use serde::Deserialize;

use crate::commons::consts::Currency;
use crate::formats_utils::pdf_extract::standard_funcs::{
    ExtractTextPdfBlockOrFailPage, InvestmentsStandardArgs, PdfExtractCurrencyConstant, PdfExtractInvestmentsStandard,
    PdfExtractStandardFuncsError, pdf_extract_fund_standard,
};
use crate::formats_utils::pdf_extract::tabularizer::coordinates::TablePosAlgorithm;
use crate::input::document::selection::{InputPdfLineSet, LineSelectionError, pdfline_selection_from_dict};

use super::SegmentKind;

/// I nomi nativi di ciascun segmento.
const NATIVE_NAMES: [(SegmentKind, &[&str]); 3] = [
    (SegmentKind::PdfExtract, &["standard_cost_curr"]),
    (SegmentKind::TextFilter, &[]),
    (SegmentKind::Deserialize, &[]),
];

/// I nomi nativi registrati per un segmento.
pub fn names(segment: SegmentKind) -> &'static [&'static str] {
    NATIVE_NAMES.iter().find(|(kind, _)| *kind == segment).map(|(_, names)| *names).unwrap_or(&[])
}

/// `true` se `name` è un algoritmo nativo di `segment`.
pub fn contains(segment: SegmentKind, name: &str) -> bool {
    names(segment).contains(&name)
}

/// La configurazione YAML di [`standard_cost_curr`].
///
/// Traduzione diretta di `InputStandardCostCurr` (Pydantic, riferimento), con i due flag
/// espressi come **stringhe** invece che come oggetti `TablePosAlgorithm`: in YAML un flag è
/// scritto per nome, e [`TablePosAlgorithm::from_expression`] è ciò che lo risolve.
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

/// Fallimenti nella costruzione di un algoritmo nativo.
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

/// I tre pipe `pdf_extract` che un formato "costo e valuta standard" richiede.
///
/// La valuta non è scritta nel documento ma dichiarata nella configurazione: da qui il
/// [`PdfExtractCurrencyConstant`] al posto del solito pipe che la legge dalla pagina.
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
