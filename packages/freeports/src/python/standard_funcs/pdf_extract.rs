//! The shims of the eight standard pipes of the extraction segment.

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
use crate::formats_utils::pdf_extract::labelled_value::{
    LabelledValueArgs, LabelledValueWindow, PdfExtractLabelledValueStandard,
};
use crate::formats_utils::pdf_extract::section::{
    PdfExtractSectionStandard, SectionArgs, SectionEmit, SectionEnd,
};
use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;

use crate::python::consts::PyCurrency;
use crate::python::pipes::PyPdfExtractPipe;
use crate::formats_utils::pdf_extract::standard_funcs::TableSettings;
use crate::formats_utils::pdf_extract::tabularizer::TableCoordinatesConfig;
use crate::formats_utils::pdf_extract::tabularizer::collapse::CollapseAlgorithm;
use crate::python::utils::pdf_extract::{
    PyCollapseAlgorithm, PyLabelledValueTake, PyLabelledValueWhere, PyPdfLineSelection, PyTableConfig,
    PyTablePosAlgorithm, PyTablePosMeasureUnit,
};
use crate::core::tracing_setup::log_error;

/// A native selection from a Python object, with a message naming the argument.
fn selection_of(name: &str, object: &Bound<'_, PyAny>) -> PyResult<PdfLineSelection> {
    object.extract::<PyRef<'_, PyPdfLineSelection>>().map(|selection| selection.selection()).map_err(|_| {
        // A format wiring mistake, not a per-value failure: logged before the error only lives
        // as a Python exception (rule 1 of L2), same reasoning as `python::input`.
        tracing::error!(argument = name, "expected a PdfLineSelection, got something else");
        PyValueError::new_err(format!("{name} must be a PdfLineSelection"))
    })
}

/// As [`selection_of`], for an optional argument.
fn optional_selection_of(name: &str, object: Option<&Bound<'_, PyAny>>) -> PyResult<Option<PdfLineSelection>> {
    match object {
        None => Ok(None),
        Some(object) if object.is_none() => Ok(None),
        Some(object) => selection_of(name, object).map(Some),
    }
}

/// One or several selections: a single selection and an iterable of them are both accepted, and
/// author modules really do use both forms.
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

/// The tabularisation flags, where nothing given means no flags at all.
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

/// Builds the pipe that extracts an expected text and fails the page if it is not there.
///
/// The only pipe of this module also exposed directly among the utilities, because author modules
/// use it for blocks that are none of the three standard cases below.
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

/// The pipe extracting the fund name.
#[pyfunction]
#[pyo3(name = "PdfExtractFundStandard")]
pub fn py_pdf_extract_fund_standard(selection: &Bound<'_, PyAny>) -> PyResult<PyPdfExtractPipe> {
    Ok(PyPdfExtractPipe::new(Arc::new(pdf_extract_fund_standard(selection_of("selection", selection)?))))
}

/// The pipe extracting the currency statement.
#[pyfunction]
#[pyo3(name = "PdfExtractCurrencyStandard")]
pub fn py_pdf_extract_currency_standard(selection: &Bound<'_, PyAny>) -> PyResult<PyPdfExtractPipe> {
    Ok(PyPdfExtractPipe::new(Arc::new(pdf_extract_currency_standard(selection_of("selection", selection)?))))
}

/// `PdfExtractLabelledValueStandard(label, block_type, name, *, …)` — *un'etichetta dichiara un
/// valore lì accanto*.
///
/// Three positional arguments, and every keyword optional: the defaults are the label's own line,
/// the first match and no further constraint, which is the shape twenty-one formats of the
/// reference collection write the management company in. See
/// [`crate::formats_utils::pdf_extract::labelled_value`] for what each placement means and for what
/// `reach` and `across` are measured in.
///
/// `reach` and `across` are separate keywords rather than one `window` tuple so that a format can
/// set the one it needs and leave the other at the default its placement carries — the two have
/// different defaults for `BELOW` and for `RIGHT`, and a tuple would force both to be written.
#[pyfunction]
#[pyo3(name = "PdfExtractLabelledValueStandard")]
#[pyo3(signature = (
    label,
    block_type,
    name,
    *,
    r#where=None,
    reach=None,
    across=None,
    value=None,
    stop=None,
    stop_at_label_family=false,
    take=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_pdf_extract_labelled_value_standard(
    label: &Bound<'_, PyAny>,
    block_type: String,
    name: String,
    r#where: Option<PyRef<'_, PyLabelledValueWhere>>,
    reach: Option<f32>,
    across: Option<f32>,
    value: Option<&Bound<'_, PyAny>>,
    stop: Option<&Bound<'_, PyAny>>,
    stop_at_label_family: bool,
    take: Option<PyRef<'_, PyLabelledValueTake>>,
) -> PyResult<PyPdfExtractPipe> {
    let placement = r#where.map(|w| w.native()).unwrap_or_default();
    let stop = optional_selection_of("stop", stop)?;
    if stop.is_some() && stop_at_label_family {
        // Two ways of saying where the section ends, and no way to honour both: a wiring mistake
        // worth refusing loudly rather than resolving by precedence.
        tracing::error!("stop and stop_at_label_family were both given");
        return Err(PyValueError::new_err("stop and stop_at_label_family are alternatives, not a pair"));
    }

    let args = LabelledValueArgs {
        label: selection_of("label", label)?,
        block_type: BlockType::from(block_type),
        name,
        r#where: placement,
        // A window only where a format asked for one, so that the unset side keeps the default its
        // placement carries rather than an arbitrary zero.
        window: (reach.is_some() || across.is_some()).then(|| {
            let default = LabelledValueWindow::default_for(placement);
            LabelledValueWindow {
                reach: reach.or(default.reach),
                across: across.unwrap_or(default.across),
            }
        }),
        value: optional_selection_of("value", value)?,
        stop: match (stop, stop_at_label_family) {
            (Some(selection), _) => Some(SectionEnd::Selection(selection)),
            (None, true) => Some(SectionEnd::SameFamilyAsAnchor),
            (None, false) => None,
        },
        take: take.map(|t| t.native()).unwrap_or_default(),
    };
    Ok(PyPdfExtractPipe::new(Arc::new(PdfExtractLabelledValueStandard::new(args))))
}

/// `PdfExtractSectionStandard(heading, block_type, name, *, family=None, bottom=None, body=None,
/// include_heading=False, rows=None)` — un'intestazione apre una sezione.
///
/// `family` omesso significa *la prossima riga con la stessa coppia (font, corpo)
/// dell'intestazione*, che è il difetto misurato; passarne una selezione lo sostituisce, ed è il
/// caso di AMUNDI-EN24 p510, dove intestazioni e righe hanno la stessa tipografia. `bottom` accetta
/// una selezione o un'ordinata. `rows`, se dato, chiede le coordinate di tabella nei metadati:
/// `True` scopre le colonne, una `TableConfig` le dichiara.
#[pyfunction]
#[pyo3(name = "PdfExtractSectionStandard")]
#[pyo3(signature = (
    heading,
    block_type,
    name,
    *,
    family=None,
    bottom=None,
    body=None,
    include_heading=false,
    rows=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_pdf_extract_section_standard(
    heading: &Bound<'_, PyAny>,
    block_type: String,
    name: String,
    family: Option<&Bound<'_, PyAny>>,
    bottom: Option<&Bound<'_, PyAny>>,
    body: Option<&Bound<'_, PyAny>>,
    include_heading: bool,
    rows: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyPdfExtractPipe> {
    let args = SectionArgs {
        heading: selection_of("heading", heading)?,
        block_type: BlockType::from(block_type),
        name,
        // One keyword and no companion flag: the default is a value of the same type, so there is
        // nothing to say with a second argument. The lesson `stop` / `stop_at_label_family` taught.
        family: match optional_selection_of("family", family)? {
            Some(selection) => SectionEnd::Selection(selection),
            None => SectionEnd::SameFamilyAsAnchor,
        },
        bottom: section_bottom_of(bottom)?,
        body: optional_selection_of("body", body)?,
        include_heading,
        emit: section_emit_of(rows)?,
    };
    Ok(PyPdfExtractPipe::new(Arc::new(PdfExtractSectionStandard::new(args))))
}

/// The extra floor: a selection, or a bare ordinate as `fineco_en23_ir/inv_managers.py` writes it.
fn section_bottom_of(bottom: Option<&Bound<'_, PyAny>>) -> PyResult<Option<SectionEnd>> {
    let Some(object) = bottom else { return Ok(None) };
    if object.is_none() {
        return Ok(None);
    }
    if let Ok(ordinate) = object.extract::<f32>() {
        return Ok(Some(SectionEnd::Ordinate(ordinate)));
    }
    Ok(Some(SectionEnd::Selection(selection_of("bottom", object)?)))
}

/// `rows`: absent or false for plain lines, `True` to discover the columns, a `TableConfig` to
/// declare them.
fn section_emit_of(rows: Option<&Bound<'_, PyAny>>) -> PyResult<SectionEmit> {
    let Some(object) = rows else { return Ok(SectionEmit::Lines) };
    if object.is_none() {
        return Ok(SectionEmit::Lines);
    }
    if let Ok(wanted) = object.extract::<bool>() {
        return Ok(if wanted {
            SectionEmit::Rows(Box::default())
        } else {
            SectionEmit::Lines
        });
    }
    match object.extract::<PyRef<'_, PyTableConfig>>() {
        Ok(config) => Ok(SectionEmit::Rows(Box::new(TableCoordinatesConfig {
            table_config: Some(config.native().clone()),
            ..TableCoordinatesConfig::default()
        }))),
        Err(_) => {
            tracing::error!(argument = "rows", "expected True or a TableConfig");
            Err(PyValueError::new_err("rows must be True or a TableConfig"))
        }
    }
}

/// `PdfExtractManagmentCompanyStandard(selection)` — la società di gestione.
#[pyfunction]
#[pyo3(name = "PdfExtractManagmentCompanyStandard")]
pub fn py_pdf_extract_managment_company_standard(selection: &Bound<'_, PyAny>) -> PyResult<PyPdfExtractPipe> {
    Ok(PyPdfExtractPipe::new(Arc::new(pdf_extract_managment_company_standard(selection_of(
        "selection", selection,
    )?))))
}

/// The page classifier: classifies the page if **every** header selection finds something.
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

/// A currency known in advance, without looking at the page at all.
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

/// The pipe for the investments table.
///
/// **A divergence absorbed here:** two of the selections are accepted and ignored. That is not an
/// oversight — they were never used on this side either, the currency and the management company
/// arriving from sibling pipes. The native type, having no compatibility constraint, does not have
/// them at all, but author modules still pass them, so the Python signature must still accept them.
#[pyfunction]
#[pyo3(name = "PdfExtractInvestmentsStandard")]
#[pyo3(signature = (
    body_set,
    manco_set=None,
    currency_set=None,
    deselection_list=None,
    company_index=None,
    table_cfg=None,
    algorithm_flags=None,
    collapse_alg=None,
    col_tolerance=0.0,
    row_tolerance=0.0,
    tolerance_mu=None,
    collapse=false,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_pdf_extract_investments_standard(
    body_set: &Bound<'_, PyAny>,
    manco_set: Option<&Bound<'_, PyAny>>,
    currency_set: Option<&Bound<'_, PyAny>>,
    deselection_list: Option<&Bound<'_, PyAny>>,
    company_index: Option<i64>,
    table_cfg: Option<PyRef<'_, PyTableConfig>>,
    algorithm_flags: Option<&Bound<'_, PyAny>>,
    collapse_alg: Option<PyRef<'_, PyCollapseAlgorithm>>,
    col_tolerance: f64,
    row_tolerance: f64,
    tolerance_mu: Option<PyRef<'_, PyTablePosMeasureUnit>>,
    collapse: bool,
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
        table: table_settings(table_cfg, algorithm_flags, collapse_alg, col_tolerance, row_tolerance, tolerance_mu, collapse)?,
        company_index: company_index.map(|index| index as usize),
    };
    Ok(PyPdfExtractPipe::new(Arc::new(PdfExtractInvestmentsStandard::new(args))))
}

/// The table settings of a standard pipe, from the same six keywords `get_table_coordinates`
/// takes. `algorithm_flags` keeps its own converter because it is the one argument the pipes
/// accepted before this, and it accepts a bare flag as well as a combination.
#[allow(clippy::too_many_arguments)]
fn table_settings(
    table_cfg: Option<PyRef<'_, PyTableConfig>>,
    algorithm_flags: Option<&Bound<'_, PyAny>>,
    collapse_alg: Option<PyRef<'_, PyCollapseAlgorithm>>,
    col_tolerance: f64,
    row_tolerance: f64,
    tolerance_mu: Option<PyRef<'_, PyTablePosMeasureUnit>>,
    collapse: bool,
) -> PyResult<TableSettings> {
    Ok(TableSettings {
        table_config: table_cfg.map(|c| c.native().clone()),
        algorithm_flags: flags_of(algorithm_flags)?,
        collapse_algorithm: collapse_alg.map(|c| c.native()).unwrap_or(CollapseAlgorithm::Geometry),
        col_tolerance: col_tolerance as f32,
        row_tolerance: row_tolerance as f32,
        tolerance_unit: tolerance_mu.map(|u| u.native()).unwrap_or_default(),
        collapse,
    })
}

/// One column of the assets pipe, from the flat keyword arguments.
fn assets_column(anchor: PdfLineSelection, vector: (f64, f64), mult: (f64, f64)) -> AssetsColumn {
    AssetsColumn {
        anchor,
        vector: (vector.0 as f32, vector.1 as f32),
        width: mult.0 as f32,
        height: mult.1 as f32,
    }
}

/// The pipe for the fund-assets blocks.
///
/// **A divergence absorbed here:** the Python signature takes fourteen flat keywords, three per
/// numeric column, while the native type groups each triple into one column. The flat signature is
/// kept, and the grouping happens in `assets_column`.
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
    table_cfg=None,
    algorithm_flags=None,
    collapse_alg=None,
    col_tolerance=0.0,
    row_tolerance=0.0,
    tolerance_mu=None,
    collapse=false,
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
    table_cfg: Option<PyRef<'_, PyTableConfig>>,
    algorithm_flags: Option<&Bound<'_, PyAny>>,
    collapse_alg: Option<PyRef<'_, PyCollapseAlgorithm>>,
    col_tolerance: f64,
    row_tolerance: f64,
    tolerance_mu: Option<PyRef<'_, PyTablePosMeasureUnit>>,
    collapse: bool,
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
        // `algorithm_flags` left out keeps this pipe's own two flags, not `TableSettings`'
        // default: a format that names none of these keywords must read its fund columns exactly
        // as it did before.
        table: TableSettings {
            algorithm_flags: match algorithm_flags {
                None => TableSettings::assets_funds().algorithm_flags,
                Some(_) => flags_of(algorithm_flags)?,
            },
            ..table_settings(table_cfg, None, collapse_alg, col_tolerance, row_tolerance, tolerance_mu, collapse)?
        },
    };
    let pipe = PdfExtractAssetsStandard::build(args).map_err(|e| {
        tracing::error!(error = log_error(&e), "PdfExtractAssetsStandard construction failed: {e}");
        PyValueError::new_err(e.to_string())
    })?;
    Ok(PyPdfExtractPipe::new(Arc::new(pipe)))
}
