//! The shims of the page-geometry utilities: lines, selections, geometry, tables.
//!
//! The module author code uses most: line selections alone appear hundreds of times in a formats
//! repository. The Python contract is the established one — same names, same static methods, same
//! operators — so that updating a repository stays limited to its imports.

use pyo3::PyClass;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyType};

use crate::commons::geometry::Limits;
use crate::core::page::PageImage;
use crate::commons::sets::Container;
use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
use crate::formats_utils::pdf_extract::position::{self, ColumnConfig, RowConfig, TableConfig};
use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;
use crate::formats_utils::pdf_extract::select::relative::{
    PdfLineSelection, RelativePdfLineSet, RelativeSelectPdfLineSet,
};
use crate::formats_utils::pdf_extract::tabularizer::collapse::{CollapseAlgorithm, SplittingState};
use crate::formats_utils::pdf_extract::tabularizer::coordinates::TablePosAlgorithm;
use crate::formats_utils::pdf_extract::tabularizer::{
    TableCoordinatesConfig, TablePosMeasureUnit, get_table_coordinates_from_lines,
};
use crate::formats_utils::pdf_extract::relative::RelativeInfo;
use crate::input::document::page_dict::{self, PageDict};
use crate::input::document::selection;
use crate::core::tracing_setup::log_error;

/// A native error as a Python `ValueError`.
///
/// Logged before the conversion: past this point the error lives only as a Python exception,
/// invisible to this crate's own logging. An error rather than a warning: unlike a per-value cast,
/// the failure here is over a whole page, configuration or selection, not over one value author
/// code tries and discards.
///
/// It also attaches the structured error, so that the file logs carry its debug form, display form
/// and chain of causes rather than the message alone.
fn value_error<E: std::error::Error + 'static>(error: E) -> PyErr {
    tracing::error!(error = log_error(&error), "pdf_extract call failed: {error}");
    pyo3::exceptions::PyValueError::new_err(error.to_string())
}

/// The variant for the one caller that has no error to pass but an already-composed message. The
/// same event, without the structured part: there is no failure to serialize.
fn value_error_msg(message: String) -> PyErr {
    tracing::error!("pdf_extract call failed: {message}");
    pyo3::exceptions::PyValueError::new_err(message)
}

// =================================================================================================
// PdfLine
// =================================================================================================

/// The Python shim of a page line: text with its font, size and bounding box.
#[pyclass(name = "PdfLine", module = "freeports.utils.pdf_extract", frozen)]
#[derive(Debug, Clone)]
pub struct PyPdfLine(PdfLine);

impl From<PdfLine> for PyPdfLine {
    fn from(value: PdfLine) -> Self {
        PyPdfLine(value)
    }
}

impl PyPdfLine {
    pub fn inner(&self) -> &PdfLine {
        &self.0
    }

    /// The fields defining a line, in a comparable and hashable form.
    fn identity(&self) -> (&str, u32, &str, [u32; 4]) {
        let (x0, y0, x1, y1) = self.0.bbox().as_tuple();
        (
            self.0.font().inner(),
            self.0.font_size().to_bits(),
            self.0.text(),
            [x0.to_bits(), y0.to_bits(), x1.to_bits(), y1.to_bits()],
        )
    }
}

#[pymethods]
impl PyPdfLine {
    #[new]
    fn new(font: &str, font_size: f32, text: &str, bbox: (f32, f32, f32, f32)) -> PyPdfLine {
        PyPdfLine(PdfLine::new(font, font_size, text, bbox))
    }

    #[getter]
    fn text(&self) -> &str {
        self.0.text()
    }

    #[getter]
    fn bbox(&self) -> (f32, f32, f32, f32) {
        self.0.bbox().as_tuple()
    }

    #[getter]
    fn font_name(&self) -> &str {
        self.0.font().inner()
    }

    #[getter]
    fn font_size(&self) -> f32 {
        *self.0.font_size()
    }

    fn __repr__(&self) -> String {
        let (x0, y0, x1, y1) = self.0.bbox().as_tuple();
        format!(
            "PdfLine(font={:?}, font_size={}, text={:?}, bbox=({x0}, {y0}, {x1}, {y1}))",
            self.0.font().inner(),
            self.0.font_size(),
            self.0.text()
        )
    }

    /// The native line derives neither equality nor hashing, and adding them would be a change to
    /// existing code this layer has undertaken not to make: the shim's equality and hashing go
    /// through the four observable fields, which are exactly what defines a line. The font size
    /// enters the hash by its bits, floating-point values not being hashable.
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.extract::<PyRef<'_, PyPdfLine>>() {
            Ok(other) => self.identity() == other.identity(),
            Err(_) => false,
        }
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.identity().hash(&mut hasher);
        hasher.finish()
    }
}

/// A Python list of lines as a native slice. The most frequent conversion in the whole shim: every
/// selection does it at least once.
fn lines_from_py(lines: &Bound<'_, PyAny>) -> PyResult<Vec<PdfLine>> {
    lines
        .try_iter()?
        .map(|item| Ok(item?.extract::<PyRef<'_, PyPdfLine>>()?.inner().clone()))
        .collect()
}

// =================================================================================================
// PdfLineSelection
// =================================================================================================

/// The Python shim of a line selection.
///
/// It wraps the *relative* form rather than the either-or one, because the relative branch is a
/// superset of the absolute one — an absolute selection is an absolute leaf inside a relative tree
/// — and keeping only one makes the operators definable once instead of over four combinations.
#[pyclass(name = "PdfLineSelection", module = "freeports.utils.pdf_extract", frozen)]
#[derive(Clone)]
pub struct PyPdfLineSelection(RelativePdfLineSet);

impl PyPdfLineSelection {
    /// The selection in the form the native functions require.
    pub fn selection(&self) -> PdfLineSelection {
        OptionallyRelative::Relative(self.0.clone())
    }
}

/// A relative selection from an absolute leaf.
fn absolute(leaf: crate::formats_utils::pdf_extract::select::pdf_line::SelectPdfLineSet) -> PyPdfLineSelection {
    PyPdfLineSelection(RelativePdfLineSet::from_leaf(OptionallyRelative::Absolute(leaf)))
}

/// A relative selection from a relative leaf.
fn relative(leaf: RelativeSelectPdfLineSet) -> PyPdfLineSelection {
    PyPdfLineSelection(RelativePdfLineSet::from_leaf(OptionallyRelative::Relative(leaf)))
}

#[pymethods]
impl PyPdfLineSelection {
    /// The four criteria, intersected; none of them given means any font size at all.
    #[new]
    #[pyo3(signature = (font=None, font_size=None, text=None, area=None))]
    fn new(
        font: Option<&str>,
        font_size: Option<(f32, f32)>,
        text: Option<&str>,
        area: Option<(f32, f32, f32, f32)>,
    ) -> PyPdfLineSelection {
        let mut parts: Vec<PyPdfLineSelection> = Vec::new();
        if let Some(font) = font {
            parts.push(PyPdfLineSelection::font(font));
        }
        if let Some((a, b)) = font_size {
            parts.push(PyPdfLineSelection::font_size(a, b));
        }
        if let Some(text) = text {
            parts.push(PyPdfLineSelection::text(text));
        }
        if let Some((x0, y0, x1, y1)) = area {
            parts.push(PyPdfLineSelection::area(x0, y0, x1, y1));
        }
        match parts.split_first() {
            None => PyPdfLineSelection::font_size(0.0, 1e6),
            Some((first, rest)) => {
                rest.iter().fold(first.clone(), |acc, part| acc.__and__(part.clone()))
            }
        }
    }

    // --- selezioni assolute ---------------------------------------------------------------

    #[staticmethod]
    fn font(font: &str) -> PyPdfLineSelection {
        absolute(crate::formats_utils::pdf_extract::select::pdf_line::SelectPdfLineSet::select_font(font))
    }

    #[staticmethod]
    fn font_size(a: f32, b: f32) -> PyPdfLineSelection {
        absolute(crate::formats_utils::pdf_extract::select::pdf_line::SelectPdfLineSet::select_fontsize(a, b))
    }

    #[staticmethod]
    fn text(text: &str) -> PyPdfLineSelection {
        absolute(crate::formats_utils::pdf_extract::select::pdf_line::SelectPdfLineSet::select_text(text))
    }

    #[staticmethod]
    fn area(x0: f32, y0: f32, x1: f32, y1: f32) -> PyPdfLineSelection {
        absolute(crate::formats_utils::pdf_extract::select::pdf_line::SelectPdfLineSet::select_area(x0, y0, x1, y1))
    }

    // --- selezioni relative a un'altra selezione ------------------------------------------

    #[staticmethod]
    fn font_of(target: PyPdfLineSelection) -> PyPdfLineSelection {
        relative(RelativeSelectPdfLineSet::select_font_of(target.selection()))
    }

    #[staticmethod]
    fn font_size_of(target: PyPdfLineSelection) -> PyPdfLineSelection {
        relative(RelativeSelectPdfLineSet::select_fontsize_of(target.selection()))
    }

    #[staticmethod]
    fn text_of(target: PyPdfLineSelection) -> PyPdfLineSelection {
        relative(RelativeSelectPdfLineSet::select_text_of(target.selection()))
    }

    #[staticmethod]
    fn area_of(target: PyPdfLineSelection) -> PyPdfLineSelection {
        relative(RelativeSelectPdfLineSet::select_area_of(target.selection()))
    }

    #[staticmethod]
    #[pyo3(signature = (target, vec=(0.0, 0.0), width_mult=1.0, height_mult=1.0))]
    fn area_from_movewindow(
        target: PyPdfLineSelection,
        vec: (f32, f32),
        width_mult: f32,
        height_mult: f32,
    ) -> PyPdfLineSelection {
        relative(RelativeSelectPdfLineSet::area_from_movewindow(
            target.selection(),
            vec,
            width_mult,
            height_mult,
        ))
    }

    /// Each edge is either a number, an absolute coordinate, **or** another selection, whose
    /// corresponding edge is taken. The form a formats repository uses most after matching by text.
    #[staticmethod]
    fn area_from_bounds(
        x0: &Bound<'_, PyAny>,
        y0: &Bound<'_, PyAny>,
        x1: &Bound<'_, PyAny>,
        y1: &Bound<'_, PyAny>,
    ) -> PyResult<PyPdfLineSelection> {
        Ok(relative(RelativeSelectPdfLineSet::area_from_bounds(
            bound_from_py(x0)?,
            bound_from_py(y0)?,
            bound_from_py(x1)?,
            bound_from_py(y1)?,
        )))
    }

    // --- algebra --------------------------------------------------------------------------

    fn __or__(&self, other: PyPdfLineSelection) -> PyPdfLineSelection {
        PyPdfLineSelection(self.0.clone() | other.0)
    }

    fn __and__(&self, other: PyPdfLineSelection) -> PyPdfLineSelection {
        PyPdfLineSelection(self.0.clone() & other.0)
    }

    fn __truediv__(&self, other: PyPdfLineSelection) -> PyPdfLineSelection {
        PyPdfLineSelection(self.0.clone() / other.0)
    }

    /// An alias of the union operator.
    fn __add__(&self, other: PyPdfLineSelection) -> PyPdfLineSelection {
        self.__or__(other)
    }

    /// An alias of the difference operator.
    fn __sub__(&self, other: PyPdfLineSelection) -> PyPdfLineSelection {
        self.__truediv__(other)
    }

    // --- applicazione ---------------------------------------------------------------------

    /// The lines satisfying the selection, in the order they appear.
    ///
    /// The selection is *contextualised* first: its relative parts need the page's lines to know
    /// what they refer to.
    fn select<'py>(&self, py: Python<'py>, lines: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyList>> {
        let lines = lines_from_py(lines)?;
        let set = self.0.clone().contextualize(&lines);
        let selected: Vec<PyPdfLine> =
            lines.into_iter().filter(|line| set.contains(line)).map(PyPdfLine).collect();
        PyList::new(py, selected)
    }

    /// The selection resolved against a set of lines, without applying it.
    fn contextualize(&self, lines: &Bound<'_, PyAny>) -> PyResult<PyPdfLineSet> {
        Ok(PyPdfLineSet(self.0.clone().contextualize(&lines_from_py(lines)?)))
    }
}

/// One edge of a bounds-built area: a number, or a selection to derive it from.
fn bound_from_py(value: &Bound<'_, PyAny>) -> PyResult<OptionallyRelative<f32, PdfLineSelection>> {
    if let Ok(absolute) = value.extract::<f32>() {
        return Ok(OptionallyRelative::Absolute(absolute));
    }
    let selection = value.extract::<PyPdfLineSelection>().map_err(|_| {
        tracing::error!("area_from_bounds: a bound was neither a number nor a PdfLineSelection");
        pyo3::exceptions::PyTypeError::new_err(
            "an area bound must be a number or a PdfLineSelection",
        )
    })?;
    Ok(OptionallyRelative::Relative(selection.selection()))
}

/// The Python shim of a line set: a selection already resolved against a page.
#[pyclass(name = "PdfLineSet", module = "freeports.utils.pdf_extract", frozen)]
pub struct PyPdfLineSet(PdfLineSet);

#[pymethods]
impl PyPdfLineSet {
    fn __contains__(&self, line: PyRef<'_, PyPdfLine>) -> bool {
        self.0.contains(line.inner())
    }
}

// =================================================================================================
// Dal dict di PyMuPDF
// =================================================================================================

/// The text lines of a page, from the dict PyMuPDF returns.
#[pyfunction]
#[pyo3(name = "pdflines_from_pagedict", signature = (page, auto_rotate=true))]
pub fn py_pdflines_from_pagedict(page: &Bound<'_, PyDict>, auto_rotate: bool) -> PyResult<Vec<PyPdfLine>> {
    let page = PageDict::from_py(page).map_err(value_error)?;
    let lines = page_dict::pdflines_from_pagedict(&page, auto_rotate);
    Ok(lines.into_iter().map(PyPdfLine).collect())
}

/// The raster images of a page, from the same dict.
#[pyfunction]
#[pyo3(name = "pdfimages_from_pagedict", signature = (page))]
pub fn py_pdfimages_from_pagedict(page: &Bound<'_, PyDict>) -> PyResult<Vec<PyPageImage>> {
    let page = PageDict::from_py(page).map_err(value_error)?;
    let images = page_dict::pdfimages_from_pagedict(&page);
    Ok(images.into_iter().map(PyPageImage).collect())
}

/// Shim Python di [`PageImage`].
#[pyclass(name = "PageImage", module = "freeports.utils.pdf_extract", frozen)]
pub struct PyPageImage(PageImage);

#[pymethods]
impl PyPageImage {
    #[getter]
    fn bbox(&self) -> (f32, f32, f32, f32) {
        self.0.bbox.as_tuple()
    }

    #[getter]
    fn ext(&self) -> &str {
        &self.0.ext
    }

    #[getter]
    fn data(&self) -> &[u8] {
        &self.0.data
    }
}

/// A selection written in the compact textual syntax formats repositories use in their
/// configuration files.
#[pyfunction]
#[pyo3(name = "pdfline_selection_from_str", signature = (input))]
pub fn py_pdfline_selection_from_str(input: &str) -> PyResult<PyPdfLineSelection> {
    selection::pdfline_selection_from_str(input).map(into_shim).map_err(value_error)
}

/// A selection written as a mapping — the extended form of the same configuration.
#[pyfunction]
#[pyo3(name = "pdfline_selection_from_dict", signature = (data))]
pub fn py_pdfline_selection_from_dict(data: &Bound<'_, PyAny>) -> PyResult<PyPdfLineSelection> {
    let spec: selection::InputPdfLineSet = serde_json::from_str(&py_to_json(data)?)
        .map_err(|e| value_error_msg(format!("not a valid line-selection mapping: {e}")))?;
    selection::pdfline_selection_from_dict(&spec).map(into_shim).map_err(value_error)
}

/// A native selection as a shim.
///
/// An absolute selection is re-wrapped into a relative tree of a single leaf: the shim always keeps
/// the relative form, and an absolute set is exactly an absolute leaf.
fn into_shim(selection: PdfLineSelection) -> PyPdfLineSelection {
    match selection {
        OptionallyRelative::Relative(relative) => PyPdfLineSelection(relative),
        OptionallyRelative::Absolute(set) => PyPdfLineSelection(lift_absolute(set.ast())),
    }
}

/// Lifts an **absolute** selection tree into the corresponding relative one, leaf by leaf.
///
/// Neither a lossy conversion nor a shortcut: the relative tree's leaves are exactly "absolute or
/// relative", so every absolute leaf already has its precise place inside it. It is needed because
/// the shim keeps only one of the two forms.
fn lift_absolute(
    node: &crate::commons::sets::ast_simple::AstNode<
        crate::formats_utils::pdf_extract::select::pdf_line::SelectPdfLineSet,
        PdfLine,
    >,
) -> RelativePdfLineSet {
    use crate::commons::sets::SetOps;
    use crate::commons::sets::ast_simple::AstNode;
    match node {
        AstNode::Leaf(leaf) => {
            RelativePdfLineSet::from_leaf(OptionallyRelative::Absolute(leaf.clone()))
        }
        AstNode::Branch(left, op, right) => {
            let (left, right) = (lift_absolute(left), lift_absolute(right));
            match op {
                SetOps::Union => left | right,
                SetOps::Inter => left & right,
                SetOps::Sub => left / right,
            }
        }
    }
}

/// A Python object as JSON, to reuse the deserializers already written instead of reimplementing
/// the reading of every configuration field by hand.
fn py_to_json(value: &Bound<'_, PyAny>) -> PyResult<String> {
    let json = value.py().import("json")?;
    json.call_method1("dumps", (value,))?.extract()
}

// =================================================================================================
// Geometria e tabelle
// =================================================================================================

/// Groups lines by proximity along one axis.
#[pyfunction]
#[pyo3(name = "get_groups", signature = (lines, treshold, vertical=true))]
pub fn py_get_groups(lines: &Bound<'_, PyAny>, treshold: f32, vertical: bool) -> PyResult<Vec<i64>> {
    position::get_groups(&lines_from_py(lines)?, treshold, vertical).map_err(value_error)
}

/// The `(row, column)` coordinates of every PDF line of a table.
///
/// The signature is the established one, keyword arguments included: that is how a formats
/// repository calls it.
#[pyfunction]
#[pyo3(name = "get_table_coordinates", signature = (
    lines, table_cfg=None, algorithm_flags=None, collapse_alg=None,
    tolerance=0.0, tolerance_mu=None, company_col=None, collapse=false,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_get_table_coordinates(
    lines: &Bound<'_, PyAny>,
    table_cfg: Option<PyRef<'_, PyTableConfig>>,
    algorithm_flags: Option<PyRef<'_, PyTablePosAlgorithm>>,
    collapse_alg: Option<PyRef<'_, PyCollapseAlgorithm>>,
    tolerance: f32,
    tolerance_mu: Option<PyRef<'_, PyTablePosMeasureUnit>>,
    company_col: Option<usize>,
    collapse: bool,
) -> PyResult<Vec<(usize, usize)>> {
    let config = TableCoordinatesConfig {
        table_config: table_cfg.map(|c| c.0.clone()),
        algorithm_flags: algorithm_flags.map(|f| f.0).unwrap_or(TablePosAlgorithm::Default),
        collapse_algorithm: collapse_alg.map(|c| c.native()).unwrap_or(CollapseAlgorithm::Geometry),
        tolerance,
        tolerance_unit: tolerance_mu.map(|u| u.native()).unwrap_or_default(),
        company_col,
        collapse,
    };
    let lines = lines_from_py(lines)?;
    let coords = get_table_coordinates_from_lines(&lines, &config).map_err(value_error)?;
    Ok(coords)
}

/// Shim Python di [`Limits`], l'intervallo `(a, b)` con `a < b`.
#[pyclass(name = "Limits", module = "freeports.utils.pdf_extract", frozen, eq)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PyLimits(Limits);

#[pymethods]
impl PyLimits {
    #[new]
    fn new(a: f32, b: f32) -> PyResult<PyLimits> {
        Limits::build(a, b).map(PyLimits).map_err(value_error)
    }

    fn __repr__(&self) -> String {
        let (a, b) = self.0.as_tuple();
        format!("Limits({a}, {b})")
    }
}

/// The bounds of a row or column, as a tuple or as an already-built value.
fn limits_from_py(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Limits>> {
    let Some(value) = value.filter(|v| !v.is_none()) else { return Ok(None) };
    if let Ok(limits) = value.extract::<PyRef<'_, PyLimits>>() {
        return Ok(Some(limits.0));
    }
    let (a, b) = value.extract::<(f32, f32)>()?;
    Limits::build(a, b).map(Some).map_err(value_error)
}

/// Shim Python di [`RowConfig`].
#[pyclass(name = "RowConfig", module = "freeports.utils.pdf_extract", frozen)]
#[derive(Clone, Copy)]
pub struct PyRowConfig(RowConfig);

#[pymethods]
impl PyRowConfig {
    #[new]
    #[pyo3(signature = (limits=None))]
    fn new(limits: Option<&Bound<'_, PyAny>>) -> PyResult<PyRowConfig> {
        Ok(PyRowConfig(RowConfig { limits: limits_from_py(limits)? }))
    }

    #[getter]
    fn limits(&self) -> Option<PyLimits> {
        self.0.limits.map(PyLimits)
    }
}

/// The Python shim of a column configuration.
///
/// Unlike the row one it is not frozen: a formats repository sets the splitting **after**
/// construction, and without a setter that line would stop working.
#[pyclass(name = "ColumnConfig", module = "freeports.utils.pdf_extract")]
#[derive(Clone, Copy)]
pub struct PyColumnConfig(ColumnConfig);

#[pymethods]
impl PyColumnConfig {
    #[new]
    #[pyo3(signature = (limits=None, nullable=None, splitting=None))]
    fn new(
        limits: Option<&Bound<'_, PyAny>>,
        nullable: Option<bool>,
        splitting: Option<PyRef<'_, PySplittingState>>,
    ) -> PyResult<PyColumnConfig> {
        Ok(PyColumnConfig(ColumnConfig {
            limits: limits_from_py(limits)?,
            nullable,
            splitting: splitting.map(|s| s.native()),
        }))
    }

    #[getter]
    fn limits(&self) -> Option<PyLimits> {
        self.0.limits.map(PyLimits)
    }

    #[getter]
    fn nullable(&self) -> Option<bool> {
        self.0.nullable
    }

    #[getter]
    fn splitting(&self) -> Option<PySplittingState> {
        self.0.splitting.and_then(PySplittingState::of)
    }

    #[setter]
    fn set_splitting(&mut self, splitting: Option<PyRef<'_, PySplittingState>>) {
        self.0.splitting = splitting.map(|s| s.native());
    }

    #[setter]
    fn set_nullable(&mut self, nullable: Option<bool>) {
        self.0.nullable = nullable;
    }

    #[setter]
    fn set_limits(&mut self, limits: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.0.limits = limits_from_py(limits)?;
        Ok(())
    }
}

/// The Python shim of a table configuration.
///
/// The rows and columns accept a single configuration or an iterable: passing one directly is the
/// form a formats repository uses most often.
#[pyclass(name = "TableConfig", module = "freeports.utils.pdf_extract")]
#[derive(Clone)]
pub struct PyTableConfig(TableConfig);

/// A single configuration or an iterable of them, as a vector.
///
/// Accepting the single case is not a free convenience: it is the form a formats repository writes
/// most often.
fn coerce_configs<T, N>(value: Option<&Bound<'_, PyAny>>, native: fn(&T) -> N) -> PyResult<Option<Vec<N>>>
where
    T: PyClass,
{
    let Some(value) = value.filter(|v| !v.is_none()) else { return Ok(None) };
    if let Ok(single) = value.extract::<PyRef<'_, T>>() {
        return Ok(Some(vec![native(&single)]));
    }
    let items = value
        .try_iter()?
        .map(|item| {
            let item = item?;
            let config = item.extract::<PyRef<'_, T>>()?;
            Ok(native(&config))
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(Some(items))
}

#[pymethods]
impl PyTableConfig {
    #[new]
    #[pyo3(signature = (cols=None, rows=None))]
    fn new(cols: Option<&Bound<'_, PyAny>>, rows: Option<&Bound<'_, PyAny>>) -> PyResult<PyTableConfig> {
        Ok(PyTableConfig(TableConfig {
            cols: coerce_configs::<PyColumnConfig, _>(cols, |c| c.0)?,
            rows: coerce_configs::<PyRowConfig, _>(rows, |r| r.0)?,
        }))
    }

    #[getter]
    fn cols(&self) -> Option<Vec<PyColumnConfig>> {
        self.0.cols.as_ref().map(|c| c.iter().copied().map(PyColumnConfig).collect())
    }

    #[setter]
    fn set_cols(&mut self, cols: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.0.cols = coerce_configs::<PyColumnConfig, _>(cols, |c| c.0)?;
        Ok(())
    }

    #[getter]
    fn rows(&self) -> Option<Vec<PyRowConfig>> {
        self.0.rows.as_ref().map(|r| r.iter().copied().map(PyRowConfig).collect())
    }

    #[setter]
    fn set_rows(&mut self, rows: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.0.rows = coerce_configs::<PyRowConfig, _>(rows, |r| r.0)?;
        Ok(())
    }
}

/// The Python shim of the table-recognition flags.
///
/// Not a catalogue of names like the block-type ones: these flags **combine** with `|`, and that is
/// how a formats repository writes them.
#[pyclass(name = "TablePosAlgorithm", module = "freeports.utils.pdf_extract", frozen)]
#[derive(Debug, Clone, Copy)]
pub struct PyTablePosAlgorithm(TablePosAlgorithm);

impl PyTablePosAlgorithm {
    /// The wrapped native flags, as the standard pipe constructors require them.
    pub fn native(&self) -> TablePosAlgorithm {
        self.0
    }
}

#[pymethods]
impl PyTablePosAlgorithm {
    #[new]
    #[pyo3(signature = (bits=0))]
    fn new(bits: u8) -> PyTablePosAlgorithm {
        PyTablePosAlgorithm(TablePosAlgorithm::from_bits_truncate(bits))
    }

    fn __or__(&self, other: PyRef<'_, PyTablePosAlgorithm>) -> PyTablePosAlgorithm {
        PyTablePosAlgorithm(self.0 | other.0)
    }

    fn __and__(&self, other: PyRef<'_, PyTablePosAlgorithm>) -> PyTablePosAlgorithm {
        PyTablePosAlgorithm(self.0 & other.0)
    }

    fn __contains__(&self, other: PyRef<'_, PyTablePosAlgorithm>) -> bool {
        self.0.contains(other.0)
    }

    /// The native flag type derives neither equality nor hashing, so the shim's go through the bits
    /// — which is the true identity of a set of flags anyway.
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.extract::<PyRef<'_, PyTablePosAlgorithm>>() {
            Ok(other) => self.0.bits() == other.0.bits(),
            Err(_) => false,
        }
    }

    fn __hash__(&self) -> u64 {
        u64::from(self.0.bits())
    }

    #[getter]
    fn value(&self) -> u8 {
        self.0.bits()
    }

    fn __repr__(&self) -> String {
        format!("TablePosAlgorithm({:#06b})", self.0.bits())
    }

    /// The textual expression formats repositories write in their configuration files.
    #[classmethod]
    fn from_expression(_cls: &Bound<'_, PyType>, expression: &str) -> PyResult<PyTablePosAlgorithm> {
        TablePosAlgorithm::from_expression(expression).map(PyTablePosAlgorithm).map_err(value_error)
    }
}

/// Generates the shim of a field-less native enum, with its members attached at runtime.
///
/// The shim holds the member's **index** rather than the native value: the native enums derive
/// neither equality nor hashing, and adding them would be a change to existing code this layer has
/// undertaken not to make. A field-less enum is identified exactly by its member, so comparing
/// indices is comparing values.
macro_rules! plain_enum_shim {
    ($shim:ident, $native:ty, $py_name:literal, [$(($member:ident, $value:expr)),+ $(,)?]) => {
        #[doc = concat!("Shim Python di [`", stringify!($native), "`].")]
        #[pyclass(name = $py_name, module = "freeports.utils.pdf_extract", frozen, eq, hash)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $shim(usize);

        impl $shim {
            /// I membri, nell'ordine di dichiarazione del riferimento.
            pub const MEMBERS: &'static [(&'static str, $native)] = &[$((stringify!($member), $value)),+];

            /// The member's native value.
            pub fn native(&self) -> $native {
                Self::MEMBERS[self.0].1
            }
        }

        #[pymethods]
        impl $shim {
            #[getter]
            fn name(&self) -> &'static str {
                Self::MEMBERS[self.0].0
            }

            #[getter]
            fn value(&self) -> &'static str {
                self.name()
            }

            fn __repr__(&self) -> String {
                format!("<{}.{}>", $py_name, self.name())
            }

            #[classattr]
            fn __members__() -> std::collections::BTreeMap<&'static str, $shim> {
                Self::MEMBERS.iter().enumerate().map(|(i, (n, _))| (*n, $shim(i))).collect()
            }
        }
    };
}

plain_enum_shim!(
    PySplittingState,
    SplittingState,
    "SplittingState",
    [
        (DISALLOW, SplittingState::Disallow),
        (ALLOW_UP, SplittingState::Allow(crate::formats_utils::pdf_extract::position::SplittingDirection::Up)),
        (ALLOW_DOWN, SplittingState::Allow(crate::formats_utils::pdf_extract::position::SplittingDirection::Down)),
    ]
);
plain_enum_shim!(
    PyCollapseAlgorithm,
    CollapseAlgorithm,
    "CollapseAlgorithm",
    [
        (PATTERN, CollapseAlgorithm::Pattern),
        (GEOMETRY, CollapseAlgorithm::Geometry),
        (GEOMETRY_THEN_PATTERN, CollapseAlgorithm::GeometryThenPattern),
        (PATTERN_THEN_GEOMETRY, CollapseAlgorithm::PatternThenGeometry),
    ]
);
plain_enum_shim!(
    PyTablePosMeasureUnit,
    TablePosMeasureUnit,
    "TablePosMeasureUnit",
    [
        (EM, TablePosMeasureUnit::Em),
        (PERC, TablePosMeasureUnit::Perc),
        (PT, TablePosMeasureUnit::Pt),
    ]
);

/// Attaches the enumeration members and the table flags to the module.
pub fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let attach = |py_name: &str, members: Vec<(&str, Py<PyAny>)>| -> PyResult<()> {
        let class = module.getattr(py_name)?;
        for (name, value) in members {
            class.setattr(name, value)?;
        }
        Ok(())
    };
    let py = module.py();

    attach(
        "SplittingState",
        PySplittingState::MEMBERS
            .iter()
            .enumerate()
            .map(|(i, (n, _))| Ok((*n, Bound::new(py, PySplittingState(i))?.into_any().unbind())))
            .collect::<PyResult<_>>()?,
    )?;
    attach(
        "CollapseAlgorithm",
        PyCollapseAlgorithm::MEMBERS
            .iter()
            .enumerate()
            .map(|(i, (n, _))| Ok((*n, Bound::new(py, PyCollapseAlgorithm(i))?.into_any().unbind())))
            .collect::<PyResult<_>>()?,
    )?;
    attach(
        "TablePosMeasureUnit",
        PyTablePosMeasureUnit::MEMBERS
            .iter()
            .enumerate()
            .map(|(i, (n, _))| Ok((*n, Bound::new(py, PyTablePosMeasureUnit(i))?.into_any().unbind())))
            .collect::<PyResult<_>>()?,
    )?;
    attach(
        "TablePosAlgorithm",
        [
            ("Default", TablePosAlgorithm::Default),
            ("RETURN_ROWS", TablePosAlgorithm::ReturnRows),
            ("BIG_CELL_RULE", TablePosAlgorithm::BigCellRule),
            ("USE_RULER_AREA", TablePosAlgorithm::UseRulerArea),
            ("USE_TEST_POS", TablePosAlgorithm::UseTestPos),
        ]
        .into_iter()
        .map(|(n, v)| Ok((n, Bound::new(py, PyTablePosAlgorithm(v))?.into_any().unbind())))
        .collect::<PyResult<_>>()?,
    )?;

    // The nullable state is an alias of `bool` in the crate, and stays `bool` in Python, so
    // that calling it like a constructor keeps working.
    module.setattr("NullableState", py.get_type::<pyo3::types::PyBool>())?;
    Ok(())
}

impl PySplittingState {
    /// The shim member corresponding to a native value.
    ///
    /// The native type derives no equality, so the comparison goes through the discriminant and
    /// the direction — the only two things distinguishing its three forms.
    fn of(state: SplittingState) -> Option<PySplittingState> {
        use crate::formats_utils::pdf_extract::position::SplittingDirection;
        let index = match state {
            SplittingState::Disallow => 0,
            SplittingState::Allow(SplittingDirection::Up) => 1,
            SplittingState::Allow(SplittingDirection::Down) => 2,
        };
        Some(PySplittingState(index))
    }
}
