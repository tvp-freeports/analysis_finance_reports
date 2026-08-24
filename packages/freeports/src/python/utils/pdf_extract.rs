//! Shim di `freeports.utils.pdf_extract`: righe di pagina, selezioni, geometria, tabelle.
//!
//! È il modulo che il codice d'autore di un repo formati usa di più: `PdfLineSelection` da sola
//! compare centinaia di volte. Il contratto Python è quello del riferimento — stessi nomi, stessi
//! metodi statici, stessi operatori — verificato contro i `#[pyclass]` che il vecchio
//! `freeports_core` esponeva, così che l'aggiornamento del repo formati resti limitato agli
//! import.

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

/// Un errore nativo come `ValueError` Python.
fn value_error<E: std::fmt::Display>(error: E) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(error.to_string())
}

// =================================================================================================
// PdfLine
// =================================================================================================

/// Shim Python di [`PdfLine`]: una riga di testo con font, corpo e riquadro.
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

    /// I campi che definiscono una riga, in una forma confrontabile e hashabile.
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

    /// `PdfLine` non deriva `PartialEq`/`Hash` nel crate, e aggiungerli sarebbe una modifica al
    /// codice esistente che questo layer si è imposto di non fare: uguaglianza e hash dello shim
    /// passano quindi dai quattro campi osservabili, che sono esattamente ciò che definisce una
    /// riga. Il corpo del carattere entra nell'hash per i suoi bit, perché `f32` non è `Hash`.
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

/// Una lista Python di `PdfLine` come slice nativo. È la conversione più frequente di tutto lo
/// shim: ogni selezione la fa almeno una volta.
fn lines_from_py(lines: &Bound<'_, PyAny>) -> PyResult<Vec<PdfLine>> {
    lines
        .try_iter()?
        .map(|item| Ok(item?.extract::<PyRef<'_, PyPdfLine>>()?.inner().clone()))
        .collect()
}

// =================================================================================================
// PdfLineSelection
// =================================================================================================

/// Shim Python di [`PdfLineSelection`].
///
/// Avvolge un [`RelativePdfLineSet`] e non un `PdfLineSelection` (che è
/// `OptionallyRelative<PdfLineSet, RelativePdfLineSet>`) per la stessa ragione del riferimento: il
/// ramo relativo è un superinsieme di quello assoluto — una selezione assoluta è una foglia
/// `Absolute` dentro un albero relativo — e tenerne uno solo rende gli operatori (`|`, `&`, `/`)
/// definibili una volta invece che su quattro combinazioni.
#[pyclass(name = "PdfLineSelection", module = "freeports.utils.pdf_extract", frozen)]
#[derive(Clone)]
pub struct PyPdfLineSelection(RelativePdfLineSet);

impl PyPdfLineSelection {
    /// La selezione nella forma che le funzioni native pretendono.
    pub fn selection(&self) -> PdfLineSelection {
        OptionallyRelative::Relative(self.0.clone())
    }
}

/// Una selezione relativa da una foglia assoluta.
fn absolute(leaf: crate::formats_utils::pdf_extract::select::pdf_line::SelectPdfLineSet) -> PyPdfLineSelection {
    PyPdfLineSelection(RelativePdfLineSet::from_leaf(OptionallyRelative::Absolute(leaf)))
}

/// Una selezione relativa da una foglia relativa.
fn relative(leaf: RelativeSelectPdfLineSet) -> PyPdfLineSelection {
    PyPdfLineSelection(RelativePdfLineSet::from_leaf(OptionallyRelative::Relative(leaf)))
}

#[pymethods]
impl PyPdfLineSelection {
    /// Le quattro componenti in intersezione; nessuna componente equivale a "qualunque corpo
    /// carattere", verbatim dal riferimento.
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

    /// Ogni bordo è un numero (coordinata assoluta) **oppure** un'altra selezione, di cui si
    /// prende il bordo corrispondente. È la forma che il repo formati usa di più dopo `text`.
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

    /// Alias di `|`, come nel riferimento.
    fn __add__(&self, other: PyPdfLineSelection) -> PyPdfLineSelection {
        self.__or__(other)
    }

    /// Alias di `/`, come nel riferimento.
    fn __sub__(&self, other: PyPdfLineSelection) -> PyPdfLineSelection {
        self.__truediv__(other)
    }

    // --- applicazione ---------------------------------------------------------------------

    /// Le righe che soddisfano la selezione, nell'ordine in cui compaiono in `lines`.
    ///
    /// La selezione va prima *contestualizzata*: le sue parti relative (`text_of`, `area_of`,
    /// ...) hanno bisogno delle righe della pagina per sapere a cosa si riferiscono.
    fn select<'py>(&self, py: Python<'py>, lines: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyList>> {
        let lines = lines_from_py(lines)?;
        let set = self.0.clone().contextualize(&lines);
        let selected: Vec<PyPdfLine> =
            lines.into_iter().filter(|line| set.contains(line)).map(PyPdfLine).collect();
        PyList::new(py, selected)
    }

    /// La selezione risolta contro un insieme di righe, senza applicarla.
    fn contextualize(&self, lines: &Bound<'_, PyAny>) -> PyResult<PyPdfLineSet> {
        Ok(PyPdfLineSet(self.0.clone().contextualize(&lines_from_py(lines)?)))
    }
}

/// Un bordo di `area_from_bounds`: un numero, oppure una selezione da cui ricavarlo.
fn bound_from_py(value: &Bound<'_, PyAny>) -> PyResult<OptionallyRelative<f32, PdfLineSelection>> {
    if let Ok(absolute) = value.extract::<f32>() {
        return Ok(OptionallyRelative::Absolute(absolute));
    }
    let selection = value.extract::<PyPdfLineSelection>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err(
            "an area bound must be a number or a PdfLineSelection",
        )
    })?;
    Ok(OptionallyRelative::Relative(selection.selection()))
}

/// Shim Python di [`PdfLineSet`]: una selezione già risolta contro una pagina.
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

/// Le righe di testo di una pagina, dal dict che PyMuPDF restituisce (`page.get_text("dict")`).
#[pyfunction]
#[pyo3(name = "pdflines_from_pagedict", signature = (page, auto_rotate=true))]
pub fn py_pdflines_from_pagedict(page: &Bound<'_, PyDict>, auto_rotate: bool) -> PyResult<Vec<PyPdfLine>> {
    let page = PageDict::from_py(page).map_err(value_error)?;
    Ok(page_dict::pdflines_from_pagedict(&page, auto_rotate).into_iter().map(PyPdfLine).collect())
}

/// Le immagini raster di una pagina, dallo stesso dict.
#[pyfunction]
#[pyo3(name = "pdfimages_from_pagedict", signature = (page))]
pub fn py_pdfimages_from_pagedict(page: &Bound<'_, PyDict>) -> PyResult<Vec<PyPageImage>> {
    let page = PageDict::from_py(page).map_err(value_error)?;
    Ok(page_dict::pdfimages_from_pagedict(&page).into_iter().map(PyPageImage).collect())
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

/// Una selezione scritta nella sintassi testuale che i repo formati usano nei loro CSV.
#[pyfunction]
#[pyo3(name = "pdfline_selection_from_str", signature = (input))]
pub fn py_pdfline_selection_from_str(input: &str) -> PyResult<PyPdfLineSelection> {
    selection::pdfline_selection_from_str(input).map(into_shim).map_err(value_error)
}

/// Una selezione scritta come dizionario (la forma "estesa" della stessa configurazione).
#[pyfunction]
#[pyo3(name = "pdfline_selection_from_dict", signature = (data))]
pub fn py_pdfline_selection_from_dict(data: &Bound<'_, PyAny>) -> PyResult<PyPdfLineSelection> {
    let spec: selection::InputPdfLineSet = serde_json::from_str(&py_to_json(data)?)
        .map_err(|e| value_error(format!("not a valid line-selection mapping: {e}")))?;
    selection::pdfline_selection_from_dict(&spec).map(into_shim).map_err(value_error)
}

/// Una `PdfLineSelection` nativa come shim.
///
/// Il ramo `Absolute` viene riavvolto in un albero relativo di una sola foglia: lo shim tiene
/// sempre la forma relativa (vedi il doc di [`PyPdfLineSelection`]), e un insieme assoluto è
/// esattamente una foglia `Absolute`.
fn into_shim(selection: PdfLineSelection) -> PyPdfLineSelection {
    match selection {
        OptionallyRelative::Relative(relative) => PyPdfLineSelection(relative),
        OptionallyRelative::Absolute(set) => PyPdfLineSelection(lift_absolute(set.ast())),
    }
}

/// Solleva un albero di selezione **assoluto** nel corrispondente albero relativo, foglia per
/// foglia.
///
/// Non è una conversione con perdita né una scorciatoia: `RelativePdfLineSet` è un albero le cui
/// foglie sono `OptionallyRelative`, quindi ogni foglia assoluta ha già il suo posto esatto lì
/// dentro. Serve perché lo shim tiene una sola delle due forme (vedi il doc di
/// [`PyPdfLineSelection`]) mentre le funzioni native di configurazione possono restituire l'una o
/// l'altra.
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

/// Un oggetto Python come JSON, per riusare i `Deserialize` già scritti invece di reimplementare
/// a mano la lettura di ogni campo di configurazione.
fn py_to_json(value: &Bound<'_, PyAny>) -> PyResult<String> {
    let json = value.py().import("json")?;
    json.call_method1("dumps", (value,))?.extract()
}

// =================================================================================================
// Geometria e tabelle
// =================================================================================================

/// Raggruppa le righe per vicinanza lungo un asse.
#[pyfunction]
#[pyo3(name = "get_groups", signature = (lines, treshold, vertical=true))]
pub fn py_get_groups(lines: &Bound<'_, PyAny>, treshold: f32, vertical: bool) -> PyResult<Vec<i64>> {
    position::get_groups(&lines_from_py(lines)?, treshold, vertical).map_err(value_error)
}

/// Le coordinate `(riga, colonna)` di ogni riga PDF di una tabella.
///
/// Firma identica a quella del riferimento (`position.get_table_coordinates`), argomenti per nome
/// compresi: è così che il repo formati la chiama.
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
    get_table_coordinates_from_lines(&lines_from_py(lines)?, &config).map_err(value_error)
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

/// Gli estremi di una riga o colonna, come tupla o come `Limits` già costruiti.
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

/// Shim Python di [`ColumnConfig`].
///
/// A differenza di `RowConfig` non è congelato: il repo formati imposta `splitting` **dopo** la
/// costruzione (`table_cfg.cols[i].splitting = None`), e senza un setter quella riga smetterebbe
/// di funzionare.
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

/// Shim Python di [`TableConfig`].
///
/// `cols`/`rows` accettano una configurazione singola o un iterabile: `TableConfig(ColumnConfig(...))`
/// è la forma che il repo formati usa più spesso, ed è quella del riferimento.
#[pyclass(name = "TableConfig", module = "freeports.utils.pdf_extract")]
#[derive(Clone)]
pub struct PyTableConfig(TableConfig);

/// Una configurazione singola o un iterabile di configurazioni, come vettore.
///
/// Accettare anche il caso singolo non è una comodità gratuita: `TableConfig(ColumnConfig(...))`
/// è la forma che il repo formati scrive più spesso, ed è quella del riferimento.
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

/// Shim Python di [`TablePosAlgorithm`], i flag che governano il riconoscimento delle tabelle.
///
/// Non è un catalogo di nomi come quelli di `interfaces`: i flag si **combinano** con `|`, ed è
/// così che il repo formati li scrive (`USE_RULER_AREA | BIG_CELL_RULE`).
#[pyclass(name = "TablePosAlgorithm", module = "freeports.utils.pdf_extract", frozen)]
#[derive(Debug, Clone, Copy)]
pub struct PyTablePosAlgorithm(TablePosAlgorithm);

impl PyTablePosAlgorithm {
    /// I flag nativi avvolti — come li pretendono i costruttori dei pipe standard.
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

    /// `bitflags` non deriva `PartialEq`/`Hash` su `TablePosAlgorithm`, quindi uguaglianza e hash
    /// dello shim passano dai bit — che è comunque l'identità vera di un insieme di flag.
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

    /// L'espressione testuale che i repo formati scrivono nei loro file di configurazione.
    #[classmethod]
    fn from_expression(_cls: &Bound<'_, PyType>, expression: &str) -> PyResult<PyTablePosAlgorithm> {
        TablePosAlgorithm::from_expression(expression).map(PyTablePosAlgorithm).map_err(value_error)
    }
}

/// Genera lo shim di un enum nativo senza campi, con i membri attaccati a runtime.
///
/// Lo shim tiene l'**indice** del membro in [`MEMBERS`](Self::MEMBERS), non il valore nativo:
/// `SplittingState` e `CollapseAlgorithm` non derivano `PartialEq`/`Eq`/`Hash` nel crate, e
/// aggiungerli sarebbe una modifica al codice esistente che questo layer si è imposto di non
/// fare. Un enum senza campi è comunque identificato esattamente dal suo membro, quindi
/// confrontare gli indici è confrontare i valori.
macro_rules! plain_enum_shim {
    ($shim:ident, $native:ty, $py_name:literal, [$(($member:ident, $value:expr)),+ $(,)?]) => {
        #[doc = concat!("Shim Python di [`", stringify!($native), "`].")]
        #[pyclass(name = $py_name, module = "freeports.utils.pdf_extract", frozen, eq, hash)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $shim(usize);

        impl $shim {
            /// I membri, nell'ordine di dichiarazione del riferimento.
            pub const MEMBERS: &'static [(&'static str, $native)] = &[$((stringify!($member), $value)),+];

            /// Il valore nativo del membro.
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

/// Attacca al modulo i membri degli enum e i flag di `TablePosAlgorithm`.
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

    // `NullableState` è un alias di `bool` nel crate: in Python resta `bool`, così
    // `NullableState(True)` continua a funzionare come nel riferimento.
    module.setattr("NullableState", py.get_type::<pyo3::types::PyBool>())?;
    Ok(())
}

impl PySplittingState {
    /// Lo shim del membro corrispondente a un valore nativo.
    ///
    /// `SplittingState` non deriva `PartialEq`, quindi il confronto passa dal discriminante e
    /// dalla direzione — le uniche due cose che distinguono le sue tre forme.
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
