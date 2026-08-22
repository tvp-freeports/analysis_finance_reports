//! Rust port of the data/config classes and `get_groups` from
//! `formats/utils/pdf_extract/position.py`.
//!
//! **Deliberately NOT ported** (user confirmed, 2026-08-19): `SplittingState`, `CollapseAlgorithm`,
//! `TablePosAlgorithm`, `TablePosMeasureUnit` stay Python `Enum`/`Flag` classes.
//! `freeports_lib`'s Rust code (a *different* crate) reads them via generic duck-typing —
//! `getattr("name")` for the plain enums, and `try_iter()` + per-member `getattr("name")` for
//! `TablePosAlgorithm` (a `Flag`, relying on Python's native `Flag.__iter__` decomposition into
//! member flags — not something a from-scratch Rust pyclass gets for free). `TablePosAlgorithm`
//! also carries real Flag-parsing machinery (`commons.enum_utils.flag_from_string`/
//! `input_flags`) with no counterpart here. Porting these four would buy nothing (nothing in this
//! crate ever inspects their values) while risking a subtle break in that duck-typed interop.
//! `get_table_coordinates` itself is left in Python for the same reason: it's mostly thin glue
//! already calling straight into already-Rust `freeports_lib`, and depends on those Enums.
//!
//! **Three real bugs found and fixed at the root in `position.py` (user confirmed, 2026-08-19)**,
//! all in `get_table_coordinates`, all previously dormant (no format in the sibling
//! `analysis_finance_reports_formats` repo currently exercises any of the three paths):
//! 1. The `PERC`-tolerance branch read `l.bounds` — `PdfLine` only has `.bbox`; any format using
//!    `tolerance_mu=PERC` would have hit `AttributeError`. Fixed: `l.bounds` → `l.bbox`.
//! 2. `table_cfg.cols = [ColumnConfig()] * n_cols` aliased every column to the *same* object, so
//!    `cols[company_col].splitting = None` (meant to allow only the company-name column to wrap
//!    across lines) silently disabled splitting for every column instead. Fixed: build `n_cols`
//!    distinct instances.
//! 3. `table_cfg: TableConfig = TableConfig()` was a mutable default argument, and the function
//!    mutates `table_cfg.cols` in place — so any call passing `company_col` without an explicit
//!    `table_cfg` permanently polluted the shared default for every later call that also omitted
//!    `table_cfg`, corrupting unrelated pages/formats. Verified empirically (a second, unrelated
//!    call after a `company_col` call raised `ValueError: Expected 1 columns, found 2`). Fixed
//!    with the standard `Optional[TableConfig] = None` + fresh-instance-per-call idiom.
//!
//! Not ported here — stays in `pdf_extract/position.py` alongside the Enums.

use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};
use pyo3::PyClass;

/// Plain numeric-range check (no `Python<'_>`/PyO3 type touched) — returns the bare violation
/// message rather than a `PyErr` directly, since its only caller, [`InputArea::new`], is the
/// genuine PyO3 boundary (a `#[new]` constructor invoked from Python) that turns it into one.
fn validate_positive(name: &str, v: Option<f64>) -> Result<(), String> {
    if let Some(v) = v
        && v <= 0.0 {
            return Err(format!("{name} must be positive"));
        }
    Ok(())
}

/// Mirrors Pydantic's `PositiveFloat` fields plus the `model_validator(mode="after")` bounds
/// check from the Python original.
#[pyclass(module = "freeports._native")]
#[derive(Clone, Debug)]
pub struct InputArea {
    #[pyo3(get, set)]
    x_min: Option<f64>,
    #[pyo3(get, set)]
    x_max: Option<f64>,
    #[pyo3(get, set)]
    y_min: Option<f64>,
    #[pyo3(get, set)]
    y_max: Option<f64>,
}

#[pymethods]
impl InputArea {
    #[new]
    #[pyo3(signature = (x_min=None, x_max=None, y_min=None, y_max=None))]
    fn new(x_min: Option<f64>, x_max: Option<f64>, y_min: Option<f64>, y_max: Option<f64>) -> PyResult<Self> {
        validate_positive("x_min", x_min).map_err(PyValueError::new_err)?;
        validate_positive("x_max", x_max).map_err(PyValueError::new_err)?;
        validate_positive("y_min", y_min).map_err(PyValueError::new_err)?;
        validate_positive("y_max", y_max).map_err(PyValueError::new_err)?;
        if let (Some(mn), Some(mx)) = (x_min, x_max)
            && mx <= mn {
                return Err(PyValueError::new_err("x_max must be greater than x_min"));
            }
        if let (Some(mn), Some(mx)) = (y_min, y_max)
            && mx <= mn {
                return Err(PyValueError::new_err("y_max must be greater than y_min"));
            }
        Ok(Self { x_min, x_max, y_min, y_max })
    }

    /// Matches Pydantic `BaseModel.model_dump()` for this shape — called directly on an
    /// `InputArea` instance by `pdf_blks_acquire.py::pdfline_selection_from_dict`.
    fn model_dump(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("x_min", self.x_min)?;
        dict.set_item("x_max", self.x_max)?;
        dict.set_item("y_min", self.y_min)?;
        dict.set_item("y_max", self.y_max)?;
        Ok(dict.unbind())
    }

    /// Backs `__get_pydantic_core_schema__` below: `InputArea` is a real field type on
    /// `pdf_blks_acquire.py::InputPdfLineSet`, populated from raw YAML-parsed dicts (see
    /// `content/algorithms/semistructured/args/pdf_extract.yaml`'s `area:` keys) — needs
    /// dict-coercion, not just identity `is_instance_schema` like the enum-ish types in
    /// `commons/consts.rs`.
    #[classmethod]
    fn _pydantic_validate(cls: &Bound<'_, PyType>, value: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        if value.is_instance_of::<InputArea>() {
            return Ok(value.clone().unbind());
        }
        if let Ok(dict) = value.cast::<PyDict>() {
            return Ok(cls.call((), Some(dict))?.unbind());
        }
        Err(PyValueError::new_err("InputArea must be an InputArea instance or a mapping"))
    }

    #[classmethod]
    fn __get_pydantic_core_schema__(
        cls: &Bound<'_, PyType>,
        _source: &Bound<'_, PyAny>,
        _handler: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let py = cls.py();
        let core_schema = py.import("pydantic_core")?.getattr("core_schema")?;
        let validator = cls.getattr("_pydantic_validate")?;
        let schema = core_schema.call_method1("no_info_plain_validator_function", (validator,))?;
        Ok(schema.into())
    }
}

/// `bounds`/`tolerance` field names match exactly what `freeports_lib`'s own (crate-private)
/// `CellGeometry` extracts via `#[derive(FromPyObject)]` (by-name `getattr`) — this class is the
/// Python-facing value that crosses into that other crate, so the names are load-bearing, not
/// cosmetic.
#[pyclass(module = "freeports._native")]
#[derive(Clone)]
pub struct CellGeometry {
    #[pyo3(get, set)]
    bounds: (f64, f64, f64, f64),
    #[pyo3(get, set)]
    tolerance: f64,
}

#[pymethods]
impl CellGeometry {
    #[new]
    fn new(bounds: (f64, f64, f64, f64), tolerance: f64) -> Self {
        Self { bounds, tolerance }
    }
}

#[pyclass(module = "freeports._native")]
#[derive(Clone)]
pub struct RowConfig {
    #[pyo3(get, set)]
    limits: Option<(f64, f64)>,
}

#[pymethods]
impl RowConfig {
    #[new]
    #[pyo3(signature = (limits=None))]
    fn new(limits: Option<(f64, f64)>) -> Self {
        Self { limits }
    }
}

/// `splitting` stays a generic `Py<PyAny>` (a Python `SplittingState` enum member, or `None`) —
/// see the module doc for why `SplittingState` itself isn't ported. Unset defaults to the
/// concrete `SplittingState.DISALLOW` member (matching the Python original's
/// `splitting: ... = SplittingState.DISALLOW` default) — distinct from an explicit `None`
/// (`freeports_lib` treats a `None` `splitting` as "allow splitting downward", see
/// `tabularizer/collapse.rs`'s `unwrap_or(SplittingState::Allow(Down))`). No real caller ever
/// passes `splitting=` to the constructor (verified: the only `None` assignment anywhere is a
/// post-construction attribute set, `table_cfg.cols[company_col].splitting = None`, handled by
/// the plain setter below), so the constructor only needs to resolve the omitted case.
#[pyclass(module = "freeports._native")]
pub struct ColumnConfig {
    #[pyo3(get, set)]
    limits: Option<(f64, f64)>,
    #[pyo3(get, set)]
    nullable: Option<bool>,
    #[pyo3(get, set)]
    splitting: Py<PyAny>,
}

fn splitting_disallow(py: Python<'_>) -> PyResult<Py<PyAny>> {
    py.import("freeports._internals.formats.utils.pdf_extract.position")?
        .getattr("SplittingState")?
        .getattr("DISALLOW")
        .map(Bound::unbind)
}

#[pymethods]
impl ColumnConfig {
    #[new]
    #[pyo3(signature = (limits=None, nullable=None, splitting=None))]
    fn new(py: Python<'_>, limits: Option<(f64, f64)>, nullable: Option<bool>, splitting: Option<Py<PyAny>>) -> PyResult<Self> {
        let splitting = match splitting {
            Some(v) => v,
            None => splitting_disallow(py)?,
        };
        Ok(Self { limits, nullable, splitting })
    }
}

fn coerce_configs<T: PyClass>(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Vec<Py<T>>>> {
    let Some(value) = value else { return Ok(None) };
    if let Ok(single) = value.cast::<T>() {
        return Ok(Some(vec![single.clone().unbind()]));
    }
    let items = value
        .try_iter()?
        .map(|item| -> PyResult<Py<T>> {
            let item = item?;
            Ok(item.cast::<T>().map_err(PyErr::from)?.clone().unbind())
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(Some(items))
}

#[pyclass(module = "freeports._native")]
pub struct TableConfig {
    cols: Option<Vec<Py<ColumnConfig>>>,
    rows: Option<Vec<Py<RowConfig>>>,
}

#[pymethods]
impl TableConfig {
    #[new]
    #[pyo3(signature = (cols=None, rows=None))]
    fn new(cols: Option<&Bound<'_, PyAny>>, rows: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self { cols: coerce_configs(cols)?, rows: coerce_configs(rows)? })
    }

    #[getter]
    fn cols(&self, py: Python<'_>) -> Option<Vec<Py<ColumnConfig>>> {
        self.cols.as_ref().map(|v| v.iter().map(|c| c.clone_ref(py)).collect())
    }

    #[setter]
    fn set_cols(&mut self, cols: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.cols = coerce_configs(cols)?;
        Ok(())
    }

    #[getter]
    fn rows(&self, py: Python<'_>) -> Option<Vec<Py<RowConfig>>> {
        self.rows.as_ref().map(|v| v.iter().map(|r| r.clone_ref(py)).collect())
    }

    #[setter]
    fn set_rows(&mut self, rows: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.rows = coerce_configs(rows)?;
        Ok(())
    }
}

/// Groups `lines` by proximity along one axis. Only each line's `bbox[geoindex]` coordinate is
/// ever read (duck-typed, matching the Python original — works for a real `PdfLine` or anything
/// bbox-shaped), and the return value's order follows the *sorted* order, not the input order,
/// exactly like the Python original (`groups.append` happens inside the loop over
/// `sorted_lines`). Raises `IndexError` on an empty `lines`, matching `sorted_lines[0]` in the
/// original — not "fixed", since nothing here ever calls this with an empty list.
#[pyfunction]
#[pyo3(name = "get_groups")]
#[pyo3(signature = (lines, treshold, vertical=true))]
pub fn py_get_groups(lines: &Bound<'_, PyAny>, treshold: f64, vertical: bool) -> PyResult<Vec<i64>> {
    let geoindex = if vertical { 1usize } else { 0usize };
    let mut keys: Vec<f64> = lines
        .try_iter()?
        .map(|item| -> PyResult<f64> { item?.getattr("bbox")?.get_item(geoindex)?.extract() })
        .collect::<PyResult<_>>()?;
    if keys.is_empty() {
        return Err(PyIndexError::new_err("list index out of range"));
    }
    keys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut groups = Vec::with_capacity(keys.len());
    let mut group_id: i64 = 0;
    let mut a = keys[0];
    for b in keys {
        if (b - a).abs() >= treshold {
            group_id += 1;
        }
        a = b;
        groups.push(group_id);
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyList;

    #[test]
    fn input_area_rejects_non_positive_bound() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let err = InputArea::new(Some(-1.0), None, None, None).unwrap_err();
            assert!(err.is_instance_of::<pyo3::exceptions::PyValueError>(py));
        });
    }

    #[test]
    fn input_area_rejects_x_max_not_greater_than_x_min() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let err = InputArea::new(Some(10.0), Some(5.0), None, None).unwrap_err();
            assert!(err.is_instance_of::<pyo3::exceptions::PyValueError>(py));
        });
    }

    #[test]
    fn input_area_model_dump_round_trips_fields() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let area = InputArea::new(Some(1.0), Some(10.0), None, None).unwrap();
            let dump = area.model_dump(py).unwrap();
            let dump = dump.bind(py);
            let x_min: Option<f64> = dump.get_item("x_min").unwrap().unwrap().extract().unwrap();
            let y_min: Option<f64> = dump.get_item("y_min").unwrap().unwrap().extract().unwrap();
            assert_eq!(x_min, Some(1.0));
            assert_eq!(y_min, None);
        });
    }

    #[test]
    fn column_config_defaults_splitting_to_disallow() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let col = Py::new(py, ColumnConfig::new(py, None, None, None).unwrap()).unwrap();
            let splitting = col.bind(py).borrow().splitting.clone_ref(py);
            let name: String = splitting.bind(py).getattr("name").unwrap().extract().unwrap();
            assert_eq!(name, "DISALLOW");
        });
    }

    #[test]
    fn column_config_explicit_splitting_is_kept() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let none_obj = py.None();
            let col = Py::new(py, ColumnConfig::new(py, None, None, Some(none_obj)).unwrap()).unwrap();
            assert!(col.bind(py).borrow().splitting.is_none(py));
        });
    }

    #[test]
    fn table_config_wraps_single_column_in_a_list() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let col = Py::new(py, ColumnConfig::new(py, None, None, None).unwrap()).unwrap();
            let table_cfg = TableConfig::new(Some(col.bind(py).as_any()), None).unwrap();
            assert_eq!(table_cfg.cols(py).unwrap().len(), 1);
        });
    }

    #[test]
    fn table_config_accepts_a_list_of_columns() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let col1 = Py::new(py, ColumnConfig::new(py, None, None, None).unwrap()).unwrap();
            let col2 = Py::new(py, ColumnConfig::new(py, None, None, None).unwrap()).unwrap();
            let list = PyList::new(py, [col1, col2]).unwrap();
            let table_cfg = TableConfig::new(Some(list.as_any()), None).unwrap();
            assert_eq!(table_cfg.cols(py).unwrap().len(), 2);
        });
    }

    fn make_line(py: Python<'_>, x: f64, y: f64) -> Py<PyAny> {
        let kwargs = pyo3::types::PyDict::new(py);
        kwargs.set_item("bbox", (x, y, x + 1.0, y + 1.0)).unwrap();
        py.import("types")
            .unwrap()
            .getattr("SimpleNamespace")
            .unwrap()
            .call((), Some(&kwargs))
            .unwrap()
            .unbind()
    }

    #[test]
    fn get_groups_splits_on_threshold_along_vertical_axis() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let lines = PyList::new(
                py,
                [make_line(py, 0.0, 0.0), make_line(py, 0.0, 1.0), make_line(py, 0.0, 10.0)],
            )
            .unwrap();
            let groups = py_get_groups(lines.as_any(), 5.0, true).unwrap();
            assert_eq!(groups, vec![0, 0, 1]);
        });
    }

    #[test]
    fn get_groups_uses_horizontal_axis_when_not_vertical() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let lines = PyList::new(py, [make_line(py, 0.0, 0.0), make_line(py, 10.0, 0.0)]).unwrap();
            let groups = py_get_groups(lines.as_any(), 5.0, false).unwrap();
            assert_eq!(groups, vec![0, 1]);
        });
    }

    #[test]
    fn get_groups_raises_index_error_on_empty_input() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let lines = PyList::empty(py);
            let err = py_get_groups(lines.as_any(), 1.0, true).unwrap_err();
            assert!(err.is_instance_of::<PyIndexError>(py));
        });
    }
}
