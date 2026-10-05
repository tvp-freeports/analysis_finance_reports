//! The shims of the eight standard pipes of the extraction segment.

use std::collections::BTreeMap;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;

use crate::core::classes::BlockType;
use crate::formats_utils::pdf_extract::standard_funcs::{
    AssetsColumn, AssetsPairing, AssetsStandardArgs, ExtractTextPdfBlockOrFailPage, InvestmentsStandardArgs,
    PdfExtractAssetsStandard, PdfExtractCurrencyConstant, PdfExtractInvestmentsStandard,
    PdfExtractPageClassifyStandard, pdf_extract_currency_standard,
    pdf_extract_fund_standard, pdf_extract_managment_company_standard,
};
use crate::formats_utils::pdf_extract::labelled_value::{
    LabelledValueArgs, LabelledValueWindow, PdfExtractLabelledValueStandard,
};
use crate::formats_utils::pdf_extract::pdf_line::Font;
use crate::formats_utils::pdf_extract::section::{
    PdfExtractSectionStandard, SectionArgs, SectionEmit, SectionEnd,
};
use crate::formats_utils::pdf_extract::sfdr_sign::{
    PdfExtractSfdrSignStandard, SfdrMarks, SfdrSignArgs, SfdrSignKind,
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
///
/// `join` is the separator of [`SelectedText::EveryLine`]: absent, the block is the **first** line
/// the selection takes, which is what this pipe has always done. See
/// [`py_pdf_extract_fund_standard`], where it is the parameter that matters.
#[pyfunction]
#[pyo3(name = "ExtractTextPdfBlockOrFailPage")]
#[pyo3(signature = (selection, name, type_block, *, join=None))]
pub fn py_extract_text_pdf_block_or_fail_page(
    selection: &Bound<'_, PyAny>,
    name: String,
    type_block: String,
    join: Option<String>,
) -> PyResult<PyPdfExtractPipe> {
    let selection = selection_of("selection", selection)?;
    let pipe = ExtractTextPdfBlockOrFailPage::new(selection, name, BlockType::from(type_block));
    Ok(PyPdfExtractPipe::new(Arc::new(joining(pipe, join))))
}

/// The pipe gathering every selected line when `join` says so, and untouched otherwise.
fn joining(pipe: ExtractTextPdfBlockOrFailPage, join: Option<String>) -> ExtractTextPdfBlockOrFailPage {
    match join {
        None => pipe,
        Some(separator) => pipe.joining(separator),
    }
}

/// The pipe extracting the fund name.
///
/// `join` makes the block **every** selected line joined by that separator, in reading order,
/// instead of the first one — `join=" "` for a title split where a long dash falls, `join=""` for
/// one split inside a word. Absent, the pipe takes the first line, which is what every format
/// writing the name on one line wants and what this pipe has always done.
///
/// It is a keyword and not a second function because the two are the same algorithm with a
/// parameter: a formats repository had written the joining variant as a Python shim of its own —
/// nine lines of geometry plus the reading order and the squeezing — and the only thing it changed
/// was this.
#[pyfunction]
#[pyo3(name = "PdfExtractFundStandard")]
#[pyo3(signature = (selection, *, join=None))]
pub fn py_pdf_extract_fund_standard(
    selection: &Bound<'_, PyAny>,
    join: Option<String>,
) -> PyResult<PyPdfExtractPipe> {
    let pipe = pdf_extract_fund_standard(selection_of("selection", selection)?);
    Ok(PyPdfExtractPipe::new(Arc::new(joining(pipe, join))))
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
/// include_heading=False, rows=None, entry_gap=None)` — un'intestazione apre una sezione.
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
    entry_gap=None,
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
    entry_gap: Option<f32>,
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
        emit: section_emit_of(rows, entry_gap)?,
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
/// declare them. `entry_gap`: the vertical distance that separates two entries, for the sections
/// whose typography says nothing. The two are alternatives, because a block carries one shape of
/// metadata or the other.
fn section_emit_of(rows: Option<&Bound<'_, PyAny>>, entry_gap: Option<f32>) -> PyResult<SectionEmit> {
    if let Some(gap) = entry_gap {
        if rows.is_some_and(|object| !object.is_none()) {
            tracing::error!(arguments = "rows, entry_gap", "a section emits table coordinates or entry numbers, not both");
            return Err(PyValueError::new_err("rows and entry_gap are alternatives"));
        }
        // Written this way and not `!(gap > 0.0)`: a NaN compares false against everything, so the
        // negation of a comparison is the one form that silently lets it through.
        if gap <= 0.0 || gap.is_nan() {
            tracing::error!(argument = "entry_gap", gap, "the gap between two entries is a positive distance");
            return Err(PyValueError::new_err("entry_gap must be positive"));
        }
        return Ok(SectionEmit::Entries { gap });
    }
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

/// A pair of selections out of a two-element Python sequence.
fn label_pair(name: &str, object: &Bound<'_, PyAny>) -> PyResult<(PdfLineSelection, PdfLineSelection)> {
    let pair = selections_of(name, object)?;
    let [affirmative, negative] = <[PdfLineSelection; 2]>::try_from(pair).map_err(|got| {
        tracing::error!("{name} must be a pair of selections, affirmative first; got {}", got.len());
        PyValueError::new_err(format!("{name} must be a pair of selections, affirmative first"))
    })?;
    Ok((affirmative, negative))
}

/// The characters of a `str`, or of any iterable of one-character strings.
fn chars_of(name: &str, object: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<char>> {
    let Some(object) = object.filter(|object| !object.is_none()) else {
        return Ok(Vec::new());
    };
    if let Ok(text) = object.extract::<String>() {
        return Ok(text.chars().collect());
    }
    let mut characters = Vec::new();
    for item in object.try_iter()? {
        let text: String = item?.extract()?;
        let mut found = text.chars();
        match (found.next(), found.next()) {
            (Some(character), None) => characters.push(character),
            _ => {
                tracing::error!("{name} takes single characters, not {text:?}");
                return Err(PyValueError::new_err(format!("{name} takes single characters, not {text:?}")));
            }
        }
    }
    Ok(characters)
}

/// `PdfExtractSfdrSignStandard(question, labels, *, fallback_labels=None, title=None, signs=None,
/// ticked=None, blank=None, marks_by_font=None)` — *il segno nella casella, assegnato per
/// posizione*.
///
/// Two positional arguments and every keyword optional: the question's anchor and the pair of
/// answer labels, affirmative first. See
/// [`crate::formats_utils::pdf_extract::sfdr_sign`] for what each recognizer looks for, and why the
/// tick characters have to be named rather than guessed.
///
/// `ticked`, `blank` and `marks_by_font` **add to** the measured table rather than replacing it, so
/// a format that meets a new typography declares only what is new.
#[pyfunction]
#[pyo3(name = "PdfExtractSfdrSignStandard")]
#[pyo3(signature = (
    question,
    labels,
    *,
    fallback_labels=None,
    title=None,
    signs=None,
    ticked=None,
    blank=None,
    marks_by_font=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_pdf_extract_sfdr_sign_standard(
    question: &Bound<'_, PyAny>,
    labels: &Bound<'_, PyAny>,
    fallback_labels: Option<&Bound<'_, PyAny>>,
    title: Option<&Bound<'_, PyAny>>,
    signs: Option<Vec<String>>,
    ticked: Option<&Bound<'_, PyAny>>,
    blank: Option<&Bound<'_, PyAny>>,
    marks_by_font: Option<BTreeMap<String, String>>,
) -> PyResult<PyPdfExtractPipe> {
    let signs = match signs {
        None => SfdrSignKind::ALL.to_vec(),
        Some(names) => names
            .iter()
            .map(|name| {
                SfdrSignKind::ALL
                    .into_iter()
                    .find(|kind| kind.as_str() == name)
                    .ok_or_else(|| {
                        let known =
                            SfdrSignKind::ALL.map(SfdrSignKind::as_str).join(", ");
                        tracing::error!("unknown sign recognizer {name:?}; the five are {known}");
                        PyValueError::new_err(format!("unknown sign recognizer {name:?}; the five are {known}"))
                    })
            })
            .collect::<PyResult<Vec<_>>>()?,
    };
    let by_font = marks_by_font
        .unwrap_or_default()
        .into_iter()
        .map(|(font, characters)| (Font::new(&font).inner().to_string(), characters.chars().collect()))
        .collect::<Vec<_>>();

    let args = SfdrSignArgs {
        question: selection_of("question", question)?,
        labels: label_pair("labels", labels)?,
        fallback_labels: match fallback_labels.filter(|object| !object.is_none()) {
            Some(object) => Some(label_pair("fallback_labels", object)?),
            None => None,
        },
        title: optional_selection_of("title", title)?,
        marks: SfdrMarks::default().extended(
            chars_of("ticked", ticked)?,
            chars_of("blank", blank)?,
            by_font,
        ),
        signs,
    };
    Ok(PyPdfExtractPipe::new(Arc::new(PdfExtractSfdrSignStandard::new(args))))
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

/// The Python shim of one numeric column of the assets pipe.
///
/// A column is an anchor plus the search window hung off it, and the three belong together: the
/// window is measured in multiples of *that* anchor's bounding box, so a vector without its anchor
/// means nothing. Grouping them is what lets the assets pipe take three column arguments instead of
/// nine loose ones.
///
/// The defaults are the native constructor's, and they are the shape of the common case: the
/// number sits on the label's own row, to its right.
///
/// The Python selection is kept beside the native one so that the `anchor` getter can hand back
/// what was passed in: the native form is an either-or type the shim cannot rebuild from.
#[pyclass(name = "AssetsColumn", module = "freeports.standard_funcs.pdf_extract")]
#[derive(Clone)]
pub struct PyAssetsColumn {
    column: AssetsColumn,
    anchor: PyPdfLineSelection,
}

impl PyAssetsColumn {
    /// The wrapped native column, as the standard pipe constructor requires it.
    pub fn native(&self) -> AssetsColumn {
        self.column.clone()
    }
}

#[pymethods]
impl PyAssetsColumn {
    #[new]
    #[pyo3(signature = (anchor, vec=(1.2, 0.0), mult=(100.0, 1.3)))]
    fn new(anchor: &Bound<'_, PyAny>, vec: (f64, f64), mult: (f64, f64)) -> PyResult<PyAssetsColumn> {
        let selection = anchor.extract::<PyRef<'_, PyPdfLineSelection>>().map_err(|_| {
            tracing::error!(argument = "anchor", "expected a PdfLineSelection, got something else");
            PyValueError::new_err("anchor must be a PdfLineSelection")
        })?;
        Ok(PyAssetsColumn {
            column: AssetsColumn {
                anchor: selection.selection(),
                vector: (vec.0 as f32, vec.1 as f32),
                width: mult.0 as f32,
                height: mult.1 as f32,
            },
            anchor: selection.clone(),
        })
    }

    #[getter]
    fn anchor(&self) -> PyPdfLineSelection {
        self.anchor.clone()
    }

    #[getter]
    fn vec(&self) -> (f32, f32) {
        self.column.vector
    }

    #[setter]
    fn set_vec(&mut self, vec: (f64, f64)) {
        self.column.vector = (vec.0 as f32, vec.1 as f32);
    }

    #[getter]
    fn mult(&self) -> (f32, f32) {
        (self.column.width, self.column.height)
    }

    #[setter]
    fn set_mult(&mut self, mult: (f64, f64)) {
        self.column.width = mult.0 as f32;
        self.column.height = mult.1 as f32;
    }

    fn __repr__(&self) -> String {
        format!(
            "AssetsColumn(vec=({}, {}), mult=({}, {}))",
            self.column.vector.0, self.column.vector.1, self.column.width, self.column.height
        )
    }
}

/// A native assets column from a Python object, with a message naming the argument.
fn column_of(name: &str, object: &Bound<'_, PyAny>) -> PyResult<AssetsColumn> {
    object.extract::<PyRef<'_, PyAssetsColumn>>().map(|column| column.native()).map_err(|_| {
        tracing::error!(argument = name, "expected an AssetsColumn, got something else");
        PyValueError::new_err(format!("{name} must be an AssetsColumn"))
    })
}

/// The pipe for the fund-assets blocks.
///
/// Each of the three numeric columns is one [`PyAssetsColumn`], which is what the native type has
/// always been: an anchor and the window hung off it. The signature used to spell the three apart
/// into nine flat keywords and regroup them here, which meant a format could pass a vector to one
/// column and a multiplier to another without anything noticing.
#[pyfunction]
#[pyo3(name = "PdfExtractAssetsStandard")]
#[pyo3(signature = (
    fund_set,
    currency_set,
    net_assets,
    liabilities,
    tot_assets,
    date=None,
    table_condition=false,
    skip_column=1,
    pair_by_abscissa=false,
    drop=None,
    drop_funds=None,
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
    net_assets: &Bound<'_, PyAny>,
    liabilities: &Bound<'_, PyAny>,
    tot_assets: &Bound<'_, PyAny>,
    date: Option<&Bound<'_, PyAny>>,
    table_condition: bool,
    skip_column: i64,
    pair_by_abscissa: bool,
    drop: Option<&Bound<'_, PyAny>>,
    drop_funds: Option<&Bound<'_, PyAny>>,
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
        net_assets: column_of("net_assets", net_assets)?,
        liabilities: column_of("liabilities", liabilities)?,
        tot_assets: column_of("tot_assets", tot_assets)?,
        // One selection or a list of them: the list is a preference order, and the common case
        // of a single candidate should not have to be written as a list of one.
        date: match date.filter(|object| !object.is_none()) {
            None => Vec::new(),
            Some(object) => selections_of("date", object)?,
        },
        table_condition,
        skip_column,
        pairing: if pair_by_abscissa { AssetsPairing::ByAbscissa } else { AssetsPairing::ByIndex },
        drop: optional_selection_of("drop", drop)?,
        drop_funds: optional_selection_of("drop_funds", drop_funds)?,
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
