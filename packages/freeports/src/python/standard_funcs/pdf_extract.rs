//! Shim di `freeports.standard_funcs.pdf_extract`: gli otto pipe standard del primo segmento.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;

use crate::core::classes::BlockType;
use crate::formats_utils::pdf_extract::standard_funcs::{
    AssetsColumn, AssetsStandardArgs, ExtractTextPdfBlockOrFailPage, InvestmentsStandardArgs,
    PdfExtractAssetsStandard, PdfExtractCurrencyConstant, PdfExtractInvestmentsStandard,
    PdfExtractPageClassifyStandard, PdfExtractSfdrArticleStandard, pdf_extract_currency_standard,
    pdf_extract_fund_standard, pdf_extract_managment_company_standard,
};
use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;

use crate::python::consts::PyCurrency;
use crate::python::pipes::PyPdfExtractPipe;
use crate::python::utils::pdf_extract::{PyPdfLineSelection, PyTablePosAlgorithm};
use crate::core::tracing_setup::log_error;

/// Una selezione nativa da un oggetto Python, con un messaggio che nomina l'argomento.
fn selection_of(name: &str, object: &Bound<'_, PyAny>) -> PyResult<PdfLineSelection> {
    object.extract::<PyRef<'_, PyPdfLineSelection>>().map(|selection| selection.selection()).map_err(|_| {
        // A format wiring mistake, not a per-value failure: logged before the error only lives
        // as a Python exception (rule 1 of L2), same reasoning as `python::input`.
        tracing::error!(argument = name, "expected a PdfLineSelection, got something else");
        PyValueError::new_err(format!("{name} must be a PdfLineSelection"))
    })
}

/// Come [`selection_of`], per un argomento opzionale.
fn optional_selection_of(name: &str, object: Option<&Bound<'_, PyAny>>) -> PyResult<Option<PdfLineSelection>> {
    match object {
        None => Ok(None),
        Some(object) if object.is_none() => Ok(None),
        Some(object) => selection_of(name, object).map(Some),
    }
}

/// Una o più selezioni: il riferimento accetta indifferentemente una selezione sola o un iterabile
/// di selezioni, e i moduli d'autore usano davvero entrambe le forme.
fn selections_of(name: &str, object: &Bound<'_, PyAny>) -> PyResult<Vec<PdfLineSelection>> {
    if let Ok(one) = object.extract::<PyRef<'_, PyPdfLineSelection>>() {
        return Ok(vec![one.selection()]);
    }
    object
        .try_iter()
        .map_err(|_| {
            tracing::error!(argument = name, "expected a PdfLineSelection or an iterable of them");
            PyValueError::new_err(format!("{name} must be a PdfLineSelection or an iterable of them"))
        })?
        .map(|item| selection_of(name, &item?))
        .collect()
}

/// I flag di tabularizzazione, con lo stesso default del riferimento: `None` significa "nessun
/// flag", cioè `TablePosAlgorithm::Default`, che vale zero.
fn flags_of(
    object: Option<&Bound<'_, PyAny>>,
) -> PyResult<crate::formats_utils::pdf_extract::tabularizer::coordinates::TablePosAlgorithm> {
    use crate::formats_utils::pdf_extract::tabularizer::coordinates::TablePosAlgorithm;
    match object {
        None => Ok(TablePosAlgorithm::Default),
        Some(object) if object.is_none() => Ok(TablePosAlgorithm::Default),
        Some(object) => Ok(object
            .extract::<PyRef<'_, PyTablePosAlgorithm>>()
            .map_err(|_| {
                tracing::error!("expected a TablePosAlgorithm for the algorithm flags argument");
                PyValueError::new_err("algorithm flags must be a TablePosAlgorithm")
            })?
            .native()),
    }
}

/// Costruisce il pipe che estrae un testo atteso e fa fallire la pagina se non lo trova.
///
/// È l'unico dei pipe di questo modulo che il riferimento espone anche direttamente (da
/// `freeports.utils.pdf_extract`, non da `standard_funcs`), perché i moduli d'autore lo usano per
/// blocchi che non sono nessuno dei tre casi standard qui sotto.
#[pyfunction]
#[pyo3(name = "ExtractTextPdfBlockOrFailPage")]
pub fn py_extract_text_pdf_block_or_fail_page(
    selection: &Bound<'_, PyAny>,
    name: String,
    type_block: String,
) -> PyResult<PyPdfExtractPipe> {
    let selection = selection_of("selection", selection)?;
    Ok(PyPdfExtractPipe::new(Arc::new(ExtractTextPdfBlockOrFailPage::new(
        selection,
        name,
        BlockType::from(type_block),
    ))))
}

/// `PdfExtractFundStandard(selection)` — il nome del fondo.
#[pyfunction]
#[pyo3(name = "PdfExtractFundStandard")]
pub fn py_pdf_extract_fund_standard(selection: &Bound<'_, PyAny>) -> PyResult<PyPdfExtractPipe> {
    Ok(PyPdfExtractPipe::new(Arc::new(pdf_extract_fund_standard(selection_of("selection", selection)?))))
}

/// `PdfExtractCurrencyStandard(selection)` — la riga che dichiara la valuta.
#[pyfunction]
#[pyo3(name = "PdfExtractCurrencyStandard")]
pub fn py_pdf_extract_currency_standard(selection: &Bound<'_, PyAny>) -> PyResult<PyPdfExtractPipe> {
    Ok(PyPdfExtractPipe::new(Arc::new(pdf_extract_currency_standard(selection_of("selection", selection)?))))
}

/// `PdfExtractManagmentCompanyStandard(selection)` — la società di gestione.
#[pyfunction]
#[pyo3(name = "PdfExtractManagmentCompanyStandard")]
pub fn py_pdf_extract_managment_company_standard(selection: &Bound<'_, PyAny>) -> PyResult<PyPdfExtractPipe> {
    Ok(PyPdfExtractPipe::new(Arc::new(pdf_extract_managment_company_standard(selection_of(
        "selection", selection,
    )?))))
}

/// `PdfExtractPageClassifyStandard(header_sets, page_type)` — classifica la pagina se **tutte** le
/// selezioni d'intestazione trovano qualcosa.
#[pyfunction]
#[pyo3(name = "PdfExtractPageClassifyStandard")]
#[pyo3(signature = (header_sets, page_type))]
pub fn py_pdf_extract_page_classify_standard(
    header_sets: &Bound<'_, PyAny>,
    page_type: String,
) -> PyResult<PyPdfExtractPipe> {
    let header_sets = selections_of("header_sets", header_sets)?;
    Ok(PyPdfExtractPipe::new(Arc::new(PdfExtractPageClassifyStandard::new(header_sets, page_type))))
}

/// `PdfExtractCurrencyConstant(currency)` — una valuta nota a priori, senza guardare la pagina.
#[pyfunction]
#[pyo3(name = "PdfExtractCurrencyConstant")]
pub fn py_pdf_extract_currency_constant(currency: PyRef<'_, PyCurrency>) -> PyPdfExtractPipe {
    PyPdfExtractPipe::new(Arc::new(PdfExtractCurrencyConstant::new(currency.inner())))
}

/// `PdfExtractSfdrArticleStandard(art9_selection, art8_selection, fund_selection)`.
#[pyfunction]
#[pyo3(name = "PdfExtractSfdrArticleStandard")]
pub fn py_pdf_extract_sfdr_article_standard(
    art9_selection: &Bound<'_, PyAny>,
    art8_selection: &Bound<'_, PyAny>,
    fund_selection: &Bound<'_, PyAny>,
) -> PyResult<PyPdfExtractPipe> {
    Ok(PyPdfExtractPipe::new(Arc::new(PdfExtractSfdrArticleStandard::new(
        selection_of("art9_selection", art9_selection)?,
        selection_of("art8_selection", art8_selection)?,
        selection_of("fund_selection", fund_selection)?,
    ))))
}

/// `PdfExtractInvestmentsStandard(...)` — la tabella degli investimenti.
///
/// **Divergenza assorbita qui:** `manco_set` e `currency_set` sono accettati e ignorati. Non è una
/// svista: il riferimento fa esattamente lo stesso (li prende come parametri e non li assegna mai
/// a `self`; la valuta e la società di gestione arrivano da pipe fratelli come
/// `PdfExtractCurrencyStandard`). Il tipo nativo, che non ha vincoli di compatibilità, non li ha
/// proprio — ma i moduli d'autore li passano ancora, quindi la firma Python li deve accettare.
#[pyfunction]
#[pyo3(name = "PdfExtractInvestmentsStandard")]
#[pyo3(signature = (
    body_set,
    manco_set=None,
    currency_set=None,
    deselection_list=None,
    algorithm_flags=None,
    tolerance=0.0,
    row_algorithm_flags=None,
    row_tolerance=0.0,
    company_index=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_pdf_extract_investments_standard(
    body_set: &Bound<'_, PyAny>,
    manco_set: Option<&Bound<'_, PyAny>>,
    currency_set: Option<&Bound<'_, PyAny>>,
    deselection_list: Option<&Bound<'_, PyAny>>,
    algorithm_flags: Option<&Bound<'_, PyAny>>,
    tolerance: f64,
    row_algorithm_flags: Option<&Bound<'_, PyAny>>,
    row_tolerance: f64,
    company_index: Option<i64>,
) -> PyResult<PyPdfExtractPipe> {
    let _ = (manco_set, currency_set);

    let deselection_list = match deselection_list {
        None => Vec::new(),
        Some(list) if list.is_none() => Vec::new(),
        Some(list) => selections_of("deselection_list", list)?,
    };

    let args = InvestmentsStandardArgs {
        body_set: selection_of("body_set", body_set)?,
        deselection_list,
        algorithm_flags: flags_of(algorithm_flags)?,
        tolerance: tolerance as f32,
        row_algorithm_flags: flags_of(row_algorithm_flags)?,
        row_tolerance: row_tolerance as f32,
        company_index: company_index.map(|index| index as usize),
    };
    Ok(PyPdfExtractPipe::new(Arc::new(PdfExtractInvestmentsStandard::new(args))))
}

/// Una colonna di [`PdfExtractAssetsStandard`] dalle due keyword piatte del riferimento.
fn assets_column(anchor: PdfLineSelection, vector: (f64, f64), mult: (f64, f64)) -> AssetsColumn {
    AssetsColumn {
        anchor,
        vector: (vector.0 as f32, vector.1 as f32),
        width: mult.0 as f32,
        height: mult.1 as f32,
    }
}

/// `PdfExtractAssetsStandard(...)` — i blocchi "patrimonio del fondo".
///
/// **Divergenza assorbita qui:** il riferimento prende quattordici keyword piatte, tre per ogni
/// colonna numerica (`*_set`, `*_vec`, `*_mult`); il tipo nativo raggruppa ogni terzina in un
/// [`AssetsColumn`]. La firma Python resta quella piatta, e il raggruppamento lo fa
/// [`assets_column`].
#[pyfunction]
#[pyo3(name = "PdfExtractAssetsStandard")]
#[pyo3(signature = (
    fund_set,
    currency_set,
    net_assets_set,
    liabilities_set,
    tot_assets_set,
    net_assets_vec=(1.2, 0.0),
    liabilities_vec=(1.2, 0.0),
    tot_assets_vec=(1.2, 0.0),
    net_assets_mult=(100.0, 1.3),
    liabilities_mult=(100.0, 1.3),
    tot_assets_mult=(100.0, 1.3),
    date_set=None,
    table_condition=false,
    skip_column=1,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_pdf_extract_assets_standard(
    fund_set: &Bound<'_, PyAny>,
    currency_set: Option<&Bound<'_, PyAny>>,
    net_assets_set: &Bound<'_, PyAny>,
    liabilities_set: &Bound<'_, PyAny>,
    tot_assets_set: &Bound<'_, PyAny>,
    net_assets_vec: (f64, f64),
    liabilities_vec: (f64, f64),
    tot_assets_vec: (f64, f64),
    net_assets_mult: (f64, f64),
    liabilities_mult: (f64, f64),
    tot_assets_mult: (f64, f64),
    date_set: Option<&Bound<'_, PyAny>>,
    table_condition: bool,
    skip_column: i64,
) -> PyResult<PyPdfExtractPipe> {
    let args = AssetsStandardArgs {
        fund_set: selection_of("fund_set", fund_set)?,
        currency_set: optional_selection_of("currency_set", currency_set)?,
        net_assets: assets_column(selection_of("net_assets_set", net_assets_set)?, net_assets_vec, net_assets_mult),
        liabilities: assets_column(
            selection_of("liabilities_set", liabilities_set)?,
            liabilities_vec,
            liabilities_mult,
        ),
        tot_assets: assets_column(selection_of("tot_assets_set", tot_assets_set)?, tot_assets_vec, tot_assets_mult),
        date_set: optional_selection_of("date_set", date_set)?,
        table_condition,
        skip_column,
    };
    let pipe = PdfExtractAssetsStandard::build(args).map_err(|e| {
        tracing::error!(error = log_error(&e), "PdfExtractAssetsStandard construction failed: {e}");
        PyValueError::new_err(e.to_string())
    })?;
    Ok(PyPdfExtractPipe::new(Arc::new(pipe)))
}
