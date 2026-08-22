//! Rust port of the runtime execution classes from
//! `formats/repo/algorithms/pipelines_definition.py` (`PipelineSegement` and its 3 subclasses,
//! `Pipeline`) and `formats/repo/algorithms/definitions.py` (`PipelinesBundle`, `Algorithm`) —
//! Phase B of `agent-memory/rust-native-binary-plan.md`.
//!
//! **Scope, deliberately**: this ports the *hot path* — a `Pipeline`/segment gets invoked once
//! per pipe per page, potentially thousands of times per document — not the one-time bookkeeping
//! around it. `Algorithm`'s `schedule`/`bundles_mapping`/`page_classes` (built once at
//! `Algorithm.load()` time from pandas/pandera-validated CSVs in `orchestration.py`, which stay
//! Python pending the pandas-vs-polars decision) are kept as generic Python containers
//! (`Py<PyDict>`/`Py<PyList>`/`Py<PySet>`) rather than typed Rust collections — a dict lookup per
//! page is not the part of this system that benefits from being Rust. `Algorithm.load()` itself
//! (dynamic `importlib` loading of user-authored format modules, CSV-schema validation) is not
//! ported here either, for the same reason.
//!
//! `Pipeline` is not an internal implementation detail — it's part of the public format-authoring
//! API: every format definition in `analysis_finance_reports_formats` constructs
//! `Pipeline(pdf_extract, text_filter, deserialize)` directly. The constructor signature and
//! calling convention are preserved exactly.
//!
//! **Bug found while investigating (user confirmed, 2026-08-19)**: `definitions.py`'s
//! `logger = logging.getLogger("freeports_analysis.formats.utils")` — `freeports_analysis` is an
//! old pre-rename package name — is not an ancestor of `freeports._internals.formats.utils`
//! (verified: they only share the root logger), which is the only logger the `.log.csv`
//! `FileHandler` is attached to. So `logger.warning("Skipping page...")` on a caught
//! `PageParseFail` (and `logger_source.error(e)`, under yet another disconnected hierarchy,
//! `freeports._internals.formats.repo.algorithms.definitions`) never reaches `.log.csv` today.
//! Unlike the `text_filter/standard_funcs.rs` logging fix, here the user asked to *preserve* the
//! current (broken) behavior rather than fix it now — fixing it would add new rows to `.log.csv`
//! for every fixture where a page currently fails `PageParseFail`, requiring a fixture audit
//! that's out of scope for this port. `tracing::warn!`/`tracing::error!` are used here instead,
//! matching that already-current invisibility, verified empirically before assuming it was safe.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyString, PyTuple};
use pyo3::PyClass;

use crate::formats_repo::orchestration;
use crate::core::{PdfBlock,TextBlock};
use crate::output::*;

fn is_callable(obj: &Bound<'_, PyAny>) -> bool {
    obj.is_callable()
}

struct PdfExtractPipe<F> {
    f: F
}

impl <F> PdfExtractPipe<F>
where
    F: Fn(&Bound<'_,PyAny>) -> Vec<PdfBlock>,
{
    fn call(&self, page: &Bound<'_,PyAny>) -> Vec<PdfBlock> {
        (self.f)(page)
    }
}

struct TextFilterPipe<F> {
    f: F
}

impl <F> TextFilterPipe<F>
where
    F: Fn(&[PdfBlock],&Bound<'_,PyAny>) -> Vec<TextBlock>,
{
    fn call(&self, pdf_blks: &[PdfBlock], filter_data: &Bound<'_,PyAny>) -> Vec<TextBlock> {
        (self.f)(pdf_blks,filter_data)
    }
}

// struct DeserializePipe<F> {
//     f: F
// }
// impl <F> Deserialize<F> 
// where
//     F: Fn(&TextBlock,&Bound<'_,PyAny>) -> Vec<TextBlock>,
// {
//     fn call(&self, pdf_blks: &[PdfBlock], filter_data: &Bound<'_,PyAny>) -> Vec<TextBlock> {
//         (self.f)(pdf_blks,filter_data)
//     }
// }




/// Shared by the 3 segment kinds below: a deduplicated (by identity, matching Python's default
/// `set` semantics for objects without custom `__hash__`/`__eq__`), insertion-ordered collection
/// of callable "pipes". Iteration order over a real Python `set` is hash-dependent, not
/// insertion order, so this is not a strictly literal port — but nothing depends on segment pipe
/// execution order any more than it depends on cross-page order (already confirmed acceptable by
/// the user for the same reason: future parallelization doesn't guarantee either).
#[derive(Default)]
struct PipeSet {
    pipes: Vec<Py<PyAny>>,
}

impl PipeSet {
    fn add(&mut self, py: Python<'_>, pipe: Bound<'_, PyAny>) -> PyResult<()> {
        if !is_callable(&pipe) {
            return Err(PyValueError::new_err("Pipe added to segment has to be callable"));
        }
        if !self.pipes.iter().any(|existing| existing.bind(py).is(&pipe)) {
            self.pipes.push(pipe.unbind());
        }
        Ok(())
    }

    fn from_arg(py: Python<'_>, pipes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let mut set = Self::default();
        let Some(pipes) = pipes else { return Ok(set) };
        match pipes.try_iter() {
            Ok(iter) => {
                for item in iter {
                    set.add(py, item?)?;
                }
            }
            Err(_) if is_callable(pipes) => set.add(py, pipes.clone())?,
            Err(_) => return Err(PyValueError::new_err(format!("Specified pipes {pipes} is nor an iterable or a callable"))),
        }
        Ok(set)
    }

    fn union(&self, py: Python<'_>, other: &Self) -> Self {
        let mut set = Self { pipes: self.pipes.iter().map(|p| p.clone_ref(py)).collect() };
        for p in &other.pipes {
            let bound = p.bind(py);
            if !set.pipes.iter().any(|existing| existing.bind(py).is(bound)) {
                set.pipes.push(p.clone_ref(py));
            }
        }
        set
    }

    fn as_pyset(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        Ok(PySet::new(py, &self.pipes)?.unbind())
    }
}

/// Every method here except `call`/`__call__` is identical across the 3 segment types — a
/// `macro_rules!` was tried first but PyO3 only allows one `#[pymethods]` block per pyclass, so
/// splitting "shared methods" and "type-specific `__call__`" into two blocks (as the macro would
/// need to) doesn't compile. Three explicit blocks are more lines but far simpler than fighting
/// the macro into emitting one `#[pymethods]` block per type with an interpolated `__call__`.
macro_rules! pipeline_segment_struct {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[pyclass(module = "freeports._native")]
        pub struct $name {
            pipes: PipeSet,
        }
    };
}

pipeline_segment_struct!(PdfExtractSegment, "PDF-block extraction stage of a `Pipeline`.");
pipeline_segment_struct!(TextFilterSegment, "Text-block filtering stage of a `Pipeline`.");
pipeline_segment_struct!(DeserializeSegment, "Deserialization stage of a `Pipeline`.");

#[pymethods]
impl PdfExtractSegment {
    #[new]
    #[pyo3(signature = (pipes=None))]
    fn new(py: Python<'_>, pipes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self { pipes: PipeSet::from_arg(py, pipes)? })
    }

    fn add_pipe(&mut self, py: Python<'_>, pipe: Bound<'_, PyAny>) -> PyResult<()> {
        self.pipes.add(py, pipe)
    }

    #[getter]
    fn pipes(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        self.pipes.as_pyset(py)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.pipes.as_pyset(py)?.bind(py).try_iter()?.unbind().into_any())
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!("PdfExtractSegment{}", self.pipes.as_pyset(py)?.bind(py).repr()?))
    }

    fn __add__(&self, py: Python<'_>, other: &Self) -> Self {
        Self { pipes: self.pipes.union(py, &other.pipes) }
    }

    /// `[pdf_blk for pipe in self for pdf_blk in pipe(page)]`.
    #[pyo3(name = "__call__")]
    fn call(&self, py: Python<'_>, page: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::new();
        for pipe in &self.pipes.pipes {
            for blk in pipe.bind(py).call1((page,))?.try_iter()? {
                out.push(blk?.unbind());
            }
        }
        Ok(out)
    }
}

#[pymethods]
impl TextFilterSegment {
    #[new]
    #[pyo3(signature = (pipes=None))]
    fn new(py: Python<'_>, pipes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self { pipes: PipeSet::from_arg(py, pipes)? })
    }

    fn add_pipe(&mut self, py: Python<'_>, pipe: Bound<'_, PyAny>) -> PyResult<()> {
        self.pipes.add(py, pipe)
    }

    #[getter]
    fn pipes(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        self.pipes.as_pyset(py)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.pipes.as_pyset(py)?.bind(py).try_iter()?.unbind().into_any())
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!("TextFilterSegment{}", self.pipes.as_pyset(py)?.bind(py).repr()?))
    }

    fn __add__(&self, py: Python<'_>, other: &Self) -> Self {
        Self { pipes: self.pipes.union(py, &other.pipes) }
    }

    /// `[txt_blk for pipe in self for txt_blk in pipe(pdf_blks, filter_data)]`.
    #[pyo3(name = "__call__")]
    fn call(&self, py: Python<'_>, pdf_blks: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::new();
        for pipe in &self.pipes.pipes {
            for blk in pipe.bind(py).call1((pdf_blks, filter_data))?.try_iter()? {
                out.push(blk?.unbind());
            }
        }
        Ok(out)
    }
}

#[pymethods]
impl DeserializeSegment {
    #[new]
    #[pyo3(signature = (pipes=None))]
    fn new(py: Python<'_>, pipes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self { pipes: PipeSet::from_arg(py, pipes)? })
    }

    fn add_pipe(&mut self, py: Python<'_>, pipe: Bound<'_, PyAny>) -> PyResult<()> {
        self.pipes.add(py, pipe)
    }

    #[getter]
    fn pipes(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        self.pipes.as_pyset(py)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.pipes.as_pyset(py)?.bind(py).try_iter()?.unbind().into_any())
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!("DeserializeSegment{}", self.pipes.as_pyset(py)?.bind(py).repr()?))
    }

    fn __add__(&self, py: Python<'_>, other: &Self) -> Self {
        Self { pipes: self.pipes.union(py, &other.pipes) }
    }

    /// Matches the Python original exactly, including its `None`-preserving flatten (unlike
    /// `PdfExtractSegment`/`TextFilterSegment`, a pipe returning a bare list/tuple gets its items
    /// spliced in, but a pipe returning `None` contributes a literal `None` — filtered out later
    /// by `PipelinesBundle.apply_deserialize`/`Algorithm.__call__`, not here).
    #[pyo3(name = "__call__")]
    fn call(&self, py: Python<'_>, txt_blks: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let blks: Vec<Bound<'_, PyAny>> = txt_blks.try_iter()?.collect::<PyResult<_>>()?;
        let mut out = Vec::new();
        for pipe in &self.pipes.pipes {
            for blk in &blks {
                let r = pipe.bind(py).call1((blk,))?;
                if r.is_none() {
                    out.push(r.unbind());
                } else if r.is_instance_of::<PyList>() || r.is_instance_of::<pyo3::types::PyTuple>() {
                    for item in r.try_iter()? {
                        out.push(item?.unbind());
                    }
                } else {
                    out.push(r.unbind());
                }
            }
        }
        Ok(out)
    }
}

/// Public format-authoring API — see module doc. `Pipeline(pdf_extract, text_filter,
/// deserialize)` is called directly by every format definition in
/// `analysis_finance_reports_formats`; the constructor accepts either an already-built segment or
/// anything `PipeSet::from_arg` accepts (a single callable or an iterable of callables), exactly
/// like the Python original.
#[pyclass(module = "freeports._native")]
pub struct Pipeline {
    pdf_extract: Py<PdfExtractSegment>,
    text_filter: Py<TextFilterSegment>,
    deserialize: Py<DeserializeSegment>,
}

fn coerce_segment<T: PyClass<Frozen = pyo3::pyclass::boolean_struct::False> + Into<PyClassInitializer<T>>>(
    py: Python<'_>,
    value: Option<&Bound<'_, PyAny>>,
    build: impl FnOnce(Python<'_>, Option<&Bound<'_, PyAny>>) -> PyResult<T>,
) -> PyResult<Py<T>> {
    if let Some(value) = value
        && let Ok(existing) = value.cast::<T>() {
            return Ok(existing.clone().unbind());
        }
    Py::new(py, build(py, value)?)
}

#[pymethods]
impl Pipeline {
    #[new]
    #[pyo3(signature = (pdf_extract=None, text_filter=None, deserialize=None))]
    fn new(
        py: Python<'_>,
        pdf_extract: Option<&Bound<'_, PyAny>>,
        text_filter: Option<&Bound<'_, PyAny>>,
        deserialize: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            pdf_extract: coerce_segment(py, pdf_extract, PdfExtractSegment::new)?,
            text_filter: coerce_segment(py, text_filter, TextFilterSegment::new)?,
            deserialize: coerce_segment(py, deserialize, DeserializeSegment::new)?,
        })
    }

    #[getter]
    fn pdf_extract(&self, py: Python<'_>) -> Py<PdfExtractSegment> {
        self.pdf_extract.clone_ref(py)
    }
    #[getter]
    fn text_filter(&self, py: Python<'_>) -> Py<TextFilterSegment> {
        self.text_filter.clone_ref(py)
    }
    #[getter]
    fn deserialize(&self, py: Python<'_>) -> Py<DeserializeSegment> {
        self.deserialize.clone_ref(py)
    }

    fn add_pdf_extract(&self, py: Python<'_>, pipe: Bound<'_, PyAny>) -> PyResult<()> {
        self.pdf_extract.bind(py).borrow_mut().add_pipe(py, pipe)
    }
    fn add_text_filter(&self, py: Python<'_>, pipe: Bound<'_, PyAny>) -> PyResult<()> {
        self.text_filter.bind(py).borrow_mut().add_pipe(py, pipe)
    }
    fn add_deserialize(&self, py: Python<'_>, pipe: Bound<'_, PyAny>) -> PyResult<()> {
        self.deserialize.bind(py).borrow_mut().add_pipe(py, pipe)
    }

    fn complete(&self, py: Python<'_>) -> bool {
        !self.pdf_extract.bind(py).borrow().pipes.pipes.is_empty()
            && !self.text_filter.bind(py).borrow().pipes.pipes.is_empty()
            && !self.deserialize.bind(py).borrow().pipes.pipes.is_empty()
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!(
            "Pipeline: =[{}--{}--{}]=>",
            self.pdf_extract.bind(py).borrow().__repr__(py)?,
            self.text_filter.bind(py).borrow().__repr__(py)?,
            self.deserialize.bind(py).borrow().__repr__(py)?,
        ))
    }

    /// `pdf_blks = self.pdf_extract(page); txt_blks = self.text_filter(pdf_blks, filter_data);
    /// return self.deserialize(txt_blks)`.
    #[pyo3(name = "__call__")]
    fn call(&self, py: Python<'_>, page: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let pdf_blks = self.pdf_extract.bind(py).borrow().call(py, page)?;
        let pdf_blks = PyList::new(py, pdf_blks)?;
        let txt_blks = self.text_filter.bind(py).borrow().call(py, pdf_blks.as_any(), filter_data)?;
        let txt_blks = PyList::new(py, txt_blks)?;
        self.deserialize.bind(py).borrow().call(py, txt_blks.as_any())
    }

    fn __add__(&self, py: Python<'_>, other: &Self) -> PyResult<Self> {
        Ok(Self {
            pdf_extract: Py::new(py, self.pdf_extract.bind(py).borrow().__add__(py, &other.pdf_extract.bind(py).borrow()))?,
            text_filter: Py::new(py, self.text_filter.bind(py).borrow().__add__(py, &other.text_filter.bind(py).borrow()))?,
            deserialize: Py::new(py, self.deserialize.bind(py).borrow().__add__(py, &other.deserialize.bind(py).borrow()))?,
        })
    }
}

/// A deduplicated (by identity) collection of `Pipeline`s executed together on the same input —
/// mirrors `PipelinesBundle`'s `Set[Pipeline]` semantics the same way `PipeSet` mirrors
/// `PipelineSegement`'s.
#[pyclass(module = "freeports._native")]
pub struct PipelinesBundle {
    pipelines: Vec<Py<Pipeline>>,
}

#[pymethods]
impl PipelinesBundle {
    #[new]
    #[pyo3(signature = (pipelines=None))]
    fn new(py: Python<'_>, pipelines: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let mut out = Vec::new();
        if let Some(pipelines) = pipelines {
            if let Ok(single) = pipelines.cast::<Pipeline>() {
                out.push(single.clone().unbind());
            } else {
                for item in pipelines.try_iter()? {
                    let item = item?;
                    let p = item.cast::<Pipeline>().map_err(|_| {
                        PyValueError::new_err(format!(
                            "Pipelines bundle can contain only Pipeline, tried to add `{}`",
                            item.get_type().name().unwrap_or_else(|_| PyString::new(py, "?"))
                        ))
                    })?;
                    if !out.iter().any(|existing: &Py<Pipeline>| existing.bind(py).is(p)) {
                        out.push(p.clone().unbind());
                    }
                }
            }
        }
        Ok(Self { pipelines: out })
    }

    fn add_pipeline(&mut self, py: Python<'_>, pipeline: &Bound<'_, PyAny>) -> PyResult<()> {
        let p = pipeline.cast::<Pipeline>().map_err(|_| {
            PyValueError::new_err(format!(
                "Pipelines bundle can contain only Pipeline, tried to add `{}`",
                pipeline.get_type().name().unwrap_or_else(|_| PyString::new(py, "?"))
            ))
        })?;
        if !self.pipelines.iter().any(|existing| existing.bind(py).is(p)) {
            self.pipelines.push(p.clone().unbind());
        }
        Ok(())
    }

    #[getter]
    fn pipelines(&self, py: Python<'_>) -> PyResult<Py<PySet>> {
        Ok(PySet::new(py, &self.pipelines)?.unbind())
    }

    fn __repr__(&self) -> String {
        format!("PipelinesBundle({} pipelines)", self.pipelines.len())
    }

    #[pyo3(name = "__call__")]
    fn call(&self, py: Python<'_>, page: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::new();
        for p in &self.pipelines {
            out.extend(p.bind(py).borrow().call(py, page, filter_data)?);
        }
        Ok(out)
    }

    fn apply_pdf_extract(&self, py: Python<'_>, page: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::new();
        for p in &self.pipelines {
            let seg = p.bind(py).borrow().pdf_extract.clone_ref(py);
            out.extend(seg.bind(py).borrow().call(py, page)?);
        }
        Ok(out)
    }

    fn apply_text_filter(&self, py: Python<'_>, page: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::new();
        for p in &self.pipelines {
            let p = p.bind(py).borrow();
            let pdf_blks = p.pdf_extract.bind(py).borrow().call(py, page)?;
            let pdf_blks = PyList::new(py, pdf_blks)?;
            out.extend(p.text_filter.bind(py).borrow().call(py, pdf_blks.as_any(), filter_data)?);
        }
        Ok(out)
    }

    fn apply_deserialize(&self, py: Python<'_>, page: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let mut out = Vec::new();
        for p in &self.pipelines {
            for r in p.bind(py).borrow().call(py, page, filter_data)? {
                if !r.bind(py).is_none() {
                    out.push(r);
                }
            }
        }
        Ok(out)
    }
}

/// Orchestrates page classification, scheduling, and format-specific pipeline dispatch. See the
/// module doc for why `schedule`/`bundles_mapping`/`page_classes` stay generic Python containers
/// while `page_classify_bundle` (and, transitively, every `PipelinesBundle`/`Pipeline` this
/// dispatches to) is the real Rust type above.
#[pyclass(module = "freeports._native")]
pub struct Algorithm {
    page_classify_bundle: Py<PipelinesBundle>,
    page_classify_finalizer: Py<PyAny>,
    schedule: Py<PyList>,
    bundles_mapping: Py<PyDict>,
    page_classes: Py<PySet>,
}

fn set_from_pyobject(value: &Bound<'_, PyAny>) -> PyResult<HashSet<String>> {
    value.try_iter()?.map(|item| item?.extract::<String>()).collect()
}

#[pymethods]
impl Algorithm {
    /// Matches `Algorithm.load(cls, formats_repo_dir, format_name, format_names)` exactly — 2 of
    /// the 5 acquisition functions (`pipelines_acquisition.get_pipelines`,
    /// `unstructured_acquisition.get_compute_page_class`) stay `py.import`, permanently, by design
    /// (dynamic `importlib` loading of user-authored format modules is out of scope for this port,
    /// see module doc); the other 3 (`orchestration.get_pageclassify_pipelines`/`get_schedule`/
    /// `get_mapping`) are native in-crate calls (Milestone 1 Step 1.6). Native results are
    /// converted into the plain Python containers `Algorithm::new` already expects before being
    /// handed off to it.
    #[staticmethod]
    fn load(py: Python<'_>, formats_repo_dir: &Bound<'_, PyAny>, format_name: &Bound<'_, PyAny>, format_names: &Bound<'_, PyAny>) -> PyResult<Self> {
        let pipelines_acquisition = py.import("freeports._internals.formats.repo.algorithms.pipelines_acquisition")?;
        let unstructured_acquisition = py.import("freeports._internals.formats.repo.algorithms.unstructured.acquisition")?;

        let kwargs = PyDict::new(py);
        kwargs.set_item("allow_partial_pipelines", false)?;
        let pipelines_map = pipelines_acquisition.call_method("get_pipelines", (formats_repo_dir, format_name), Some(&kwargs))?;
        let pipelines_map = pipelines_map.cast::<PyDict>().map_err(PyErr::from)?;

        let page_classify_finalizer = unstructured_acquisition.call_method1("get_compute_page_class", (formats_repo_dir, format_name))?;

        let native_formats_repo_dir: PathBuf = formats_repo_dir.extract()?;
        let native_format_name: String = format_name.extract()?;
        let native_format_names: Vec<String> = format_names.extract()?;

        let page_classify_pipelines = orchestration::get_pageclassify_pipelines(&native_formats_repo_dir, &native_format_name, &native_format_names)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        let page_classify_pipelines = PySet::new(py, &page_classify_pipelines)?;

        let schedule = orchestration::get_schedule(&native_formats_repo_dir, &native_format_name, &native_format_names)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        let schedule = schedule.iter().map(|step| PySet::new(py, step)).collect::<PyResult<Vec<_>>>()?;
        let schedule = PyList::new(py, schedule)?;

        let mapping = orchestration::get_mapping(&native_formats_repo_dir, &native_format_name, &native_format_names)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        let mapping_dict = PyDict::new(py);
        for (page_type, pipeline_names) in mapping {
            mapping_dict.set_item(page_type, PySet::new(py, &pipeline_names)?)?;
        }

        Self::new(py, pipelines_map, page_classify_pipelines.as_any(), page_classify_finalizer.unbind(), &schedule, &mapping_dict)
    }

    #[new]
    fn new(
        py: Python<'_>,
        pipelines_map: &Bound<'_, PyDict>,
        page_classify_pipelines: &Bound<'_, PyAny>,
        page_classify_finalizer: Py<PyAny>,
        schedule: &Bound<'_, PyList>,
        page_type_pipelines_mapping: &Bound<'_, PyDict>,
    ) -> PyResult<Self> {
        let known_pipelines: HashSet<String> = pipelines_map.keys().iter().map(|k| k.extract()).collect::<PyResult<_>>()?;
        let page_classify_pipelines = set_from_pyobject(page_classify_pipelines)?;
        if !page_classify_pipelines.is_subset(&known_pipelines) {
            let unknown: Vec<_> = page_classify_pipelines.difference(&known_pipelines).collect();
            return Err(PyValueError::new_err(format!(
                "Some page classify pipelines names have no mapping to pipeline implementation: {unknown:?}"
            )));
        }
        let page_classify_bundle = {
            let names = PyList::new(py, &page_classify_pipelines)?;
            let pipelines: Vec<Bound<'_, PyAny>> =
                names.try_iter()?.map(|n| pipelines_map.get_item(n?)?.ok_or_else(|| PyValueError::new_err("missing pipeline"))).collect::<PyResult<_>>()?;
            let pipelines_list = PyList::new(py, pipelines)?;
            Py::new(py, PipelinesBundle::new(py, Some(pipelines_list.as_any()))?)?
        };

        let mut page_classes: HashSet<String> = HashSet::new();
        for step in schedule.try_iter()? {
            for pt in step?.try_iter()? {
                page_classes.insert(pt?.extract()?);
            }
        }
        let page_types_mapped: HashSet<String> = page_type_pipelines_mapping.keys().iter().map(|k| k.extract()).collect::<PyResult<_>>()?;
        if page_classes != page_types_mapped {
            let diff: Vec<_> = page_classes.symmetric_difference(&page_types_mapped).collect();
            return Err(PyValueError::new_err(format!(
                "Page classes in schedule have to be mapped to pipelines names. The difference is {diff:?}"
            )));
        }

        let mut pipelines_mapped_to_pagetype: HashSet<String> = HashSet::new();
        for (_, names) in page_type_pipelines_mapping.iter() {
            pipelines_mapped_to_pagetype.extend(set_from_pyobject(&names)?);
        }
        let tot_pipelines_names: HashSet<String> = pipelines_mapped_to_pagetype.union(&page_classify_pipelines).cloned().collect();
        if tot_pipelines_names != known_pipelines {
            let unknown: Vec<_> = tot_pipelines_names.difference(&known_pipelines).collect();
            let useless: Vec<_> = known_pipelines.difference(&tot_pipelines_names).collect();
            return Err(PyValueError::new_err(format!(
                "There are pipeline names not mapped to implementation or mapped and not used. Unmapped: {unknown:?} Not used: {useless:?}"
            )));
        }

        let bundles_mapping = PyDict::new(py);
        for (pt, names) in page_type_pipelines_mapping.iter() {
            let pipelines: Vec<Bound<'_, PyAny>> =
                names.try_iter()?.map(|n| pipelines_map.get_item(n?)?.ok_or_else(|| PyValueError::new_err("missing pipeline"))).collect::<PyResult<_>>()?;
            let pipelines_list = PyList::new(py, pipelines)?;
            let bundle = Py::new(py, PipelinesBundle::new(py, Some(pipelines_list.as_any()))?)?;
            bundles_mapping.set_item(pt, bundle)?;
        }

        let page_classes_set = PySet::new(py, &page_classes)?;
        page_classes_set.add(py.None())?;

        Ok(Self {
            page_classify_bundle,
            page_classify_finalizer,
            schedule: schedule.clone().unbind(),
            bundles_mapping: bundles_mapping.unbind(),
            page_classes: page_classes_set.unbind(),
        })
    }

    fn _transform_multidocs_if_single<'py>(&self, py: Python<'py>, docs: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyList>> {
        let err = "Input should be list of pages or list of docuements or list of tuple with document name";
        if docs.is_instance_of::<PyList>() {
            let docs = docs.cast::<PyList>().unwrap();
            if docs.is_empty() {
                let pair = pyo3::types::PyTuple::new(py, [py.None(), docs.as_any().clone().unbind().into_any()])?;
                return PyList::new(py, [pair]);
            }
            let first = docs.get_item(0)?;
            if first.is_instance_of::<PyDict>() {
                // Python: `docs=[(None,[docs])]` — wraps the *whole original* flat page list
                // (not just its first element) as the sole document.
                let inner = PyList::new(py, [docs.as_any().clone()])?;
                let pair = pyo3::types::PyTuple::new(py, [py.None(), inner.as_any().clone().unbind().into_any()])?;
                return PyList::new(py, [pair]);
            } else if first.is_instance_of::<PyList>() {
                let mut pairs = Vec::with_capacity(docs.len());
                for d in docs.try_iter()? {
                    pairs.push(pyo3::types::PyTuple::new(py, [py.None(), d?.unbind()])?);
                }
                return PyList::new(py, pairs);
            } else if first.is_instance_of::<pyo3::types::PyTuple>() {
                return Ok(docs.clone());
            }
            Err(PyValueError::new_err(err))
        } else if docs.is_instance_of::<pyo3::types::PyTuple>() {
            PyList::new(py, [docs.clone()])
        } else {
            Err(PyValueError::new_err(err))
        }
    }

    fn schedule_pages<'py>(&self, py: Python<'py>, docs: &Bound<'py, PyAny>) -> PyResult<Py<PyList>> {
        let docs = self._transform_multidocs_if_single(py, docs)?;
        let schedule = self.schedule.bind(py);

        let pages_scheduled = PyList::empty(py);
        for step in schedule.try_iter()? {
            let d = PyDict::new(py);
            for pt in step?.try_iter()? {
                d.set_item(pt?, PyList::empty(py))?;
            }
            pages_scheduled.append(d)?;
        }

        for doc in docs.try_iter()? {
            let doc = doc?;
            let doc_name = doc.get_item(0)?;
            let pages = doc.get_item(1)?;
            let pages_list: Vec<Bound<'py, PyAny>> = pages.try_iter()?.collect::<PyResult<_>>()?;

            let mut page_classification = Vec::with_capacity(pages_list.len());
            for p in &pages_list {
                for c in self.page_classify_bundle.bind(py).borrow().call(py, p, &py.None().into_bound(py))? {
                    page_classification.push(c);
                }
            }
            let page_classification_list = PyList::new(py, &page_classification)?;
            let page_classification = self.page_classify_finalizer.bind(py).call1((page_classification_list,))?;
            let page_classification: Vec<Bound<'py, PyAny>> = page_classification.try_iter()?.collect::<PyResult<_>>()?;

            if page_classification.len() != pages_list.len() {
                return Err(PyValueError::new_err("Number of pages unclassified must be equal of number of page classified"));
            }
            let page_classes = self.page_classes.bind(py);
            for pc in &page_classification {
                if !page_classes.contains(pc)? {
                    return Err(PyValueError::new_err(format!(
                        "All pages have to enter in some point in the schedule, {pc} is not part of the schedule"
                    )));
                }
            }

            for (sn, step) in schedule.try_iter()?.enumerate() {
                let step_dict = pages_scheduled.get_item(sn)?;
                for pt in step?.try_iter()? {
                    let pt = pt?;
                    let bucket = step_dict.get_item(&pt)?;
                    let bucket = bucket.cast::<PyList>().map_err(PyErr::from)?;
                    for (i, page) in pages_list.iter().enumerate() {
                        if page_classification[i].eq(&pt)? {
                            let triple = pyo3::types::PyTuple::new(py, [doc_name.clone(), (i as i64 + 1).into_pyobject(py)?.into_any(), page.clone()])?;
                            bucket.append(triple)?;
                        }
                    }
                }
            }
        }

        Ok(pages_scheduled.unbind())
    }

    /// `target_companies` is already a compiled `List[CompanyMatchInfos]` — since Phase D,
    /// `companies_db.get_target_companies` (the only real producer of this argument, via
    /// `cli/main.py`) compiles it itself (via `freeports_lib`'s `CompanyMatchInfos.compile_from_rows`,
    /// called through `py.import` — not a Rust-to-Rust call, see `input/companies_db.rs`'s module
    /// doc for why) instead of returning a DataFrame for this method to compile via
    /// `compile_from_pandas_df`. `freeports-dev test`'s per-stage test API
    /// (`apply_pdf_extract`/`apply_text_filter`/`apply_deserialize`) already passed pre-compiled
    /// `CompanyMatchInfos` lists directly and never went through `__call__`/this method at all
    /// (verified: `freeports_dev`'s test harness never calls `Algorithm.__call__`), so this
    /// method is the only place that needed to change.
    #[pyo3(name = "__call__")]
    fn call(&self, py: Python<'_>, docs: &Bound<'_, PyAny>, target_companies: &Bound<'_, PyAny>) -> PyResult<Py<PyDict>> {
        let docs = self._transform_multidocs_if_single(py, docs)?;
        let compiled_target_companies = target_companies;

        let scheduled = self.schedule_pages(py, docs.as_any())?;
        let scheduled = scheduled.bind(py);

        let res = PyDict::new(py);
        let filter_data = PyList::empty(py);
        let n_steps = self.schedule.bind(py).len();
        for step in 0..n_steps {
            let new_filter_data = PyList::empty(py);
            let step_dict = scheduled.get_item(step)?;
            let step_dict = step_dict.cast::<PyDict>().map_err(PyErr::from)?;
            for (pt, page_triples) in step_dict.iter() {
                let bundle = self.bundles_mapping.bind(py).get_item(&pt)?.ok_or_else(|| PyValueError::new_err("unmapped page type"))?;
                let bundle = bundle.cast::<PipelinesBundle>().map_err(PyErr::from)?;
                for triple in page_triples.try_iter()? {
                    let triple = triple?;
                    let doc_name = triple.get_item(0)?;
                    let page_n = triple.get_item(1)?;
                    let pages = triple.get_item(2)?;

                    // `LOG_CONTEXTUAL_INFOS.page` set/reset around this call used to stamp the
                    // current page number onto whatever `text_filter`'s Python `logging` calls
                    // wrote. `core/logging.py` isn't ported yet, and per the Python-elimination
                    // plan (`agent-memory/python-circumscription-plan.md`, Fase 2) logging is
                    // deliberately commented out here rather than ported now.

                    let this_step_filter_data =
                        if step == 0 { compiled_target_companies.clone() } else { filter_data.as_any().clone() };
                    let list_res = match bundle.borrow().call(py, &pages, &this_step_filter_data) {
                        Ok(results) => {
                            let filtered = PyList::empty(py);
                            for r in results {
                                if !r.bind(py).is_none() {
                                    filtered.append(r)?;
                                }
                            }
                            filtered
                        }
                        Err(err) if err.is_instance_of::<crate::core::classes::PageParseFail>(py) => {
                            tracing::error!(error = %err, "page parse failed");
                            tracing::warn!("Skipping page...");
                            PyList::empty(py)
                        }
                        Err(err) => return Err(err),
                    };
                    for r in list_res.try_iter()? {
                        new_filter_data.append(r?)?;
                    }
                    let key = pyo3::types::PyTuple::new(py, [doc_name, page_n])?;
                    res.set_item(key, list_res)?;
                }
            }
            for r in new_filter_data.try_iter()? {
                filter_data.append(r?)?;
            }
        }
        Ok(res.unbind())
    }

    /// Bridges `__call__`'s raw `{(doc_name, page_n): [results]}` dict into assembled
    /// `DocumentResults`/`PageResults` (dispatching each result by type, matching promises into
    /// the promise-resolution multimap) — the part of `cli/main.py`'s `_main_job` that isn't
    /// already covered by an existing method. Added specifically so the `freeports` binary's
    /// `main.rs` (Fase E, punto 3d) can drive a full run with one call through the Python
    /// boundary, per the user's guidance (2026-08-20): keep `Algorithm`/`Pipeline` as Rust, and
    /// where `main.rs` needs more than the existing public surface, add a purpose-built bridge
    /// method like this one rather than a Cargo dependency on this crate's pyclasses (which would
    /// hit the same cross-module PyO3 identity trap already solved in Fase D — see this module's
    /// own doc comment).
    ///
    /// Every `DocumentResults`/`PageResults` interaction below goes through their *Python*-visible
    /// surface (`py.get_type::<T>().call1(...)`, `.getattr(...)`, `.call_method1(...)`) rather
    /// than direct Rust field/method access — the same reason `main.rs` itself calls *this*
    /// method through `py.import`, not a Cargo dependency: whichever module asks for the type
    /// object gets the one real registered class, not a second incompatible copy.
    ///
    /// Assumes every document has a real name (`Option<String>` extracted from the dict key's
    /// first element) — true for `main.rs`'s real usage, where every `(doc_name, page_n)` key
    /// comes from a `DocumentSpec` whose `name` is never `None` for a valid spec (see
    /// `conf_parse.rs`'s `DocumentSpec::new`). The `None`-document-name case `__call__`'s own
    /// `_transform_multidocs_if_single` can produce is for other callers (e.g. `freeports_dev`'s
    /// per-stage test API, which never reaches this method) — a document with no name here is a
    /// clean error, not a crash.
    #[pyo3(signature = (docs, target_companies, format_name))]
    fn run_documents(&self, py: Python<'_>, docs: &Bound<'_, PyAny>, target_companies: &Bound<'_, PyAny>, format_name: &str) -> PyResult<Py<PyList>> {
        // (doc_name, page_n, doc_name as a Python object, page_n as a Python object) — the last
        // two kept alongside the extracted Rust values so the original key objects can be reused
        // to look the entry back up in `results` without rebuilding an equivalent tuple.
        type ResultKeyEntry<'py> = (Option<String>, i64, Bound<'py, PyAny>, Bound<'py, PyAny>);

        let results = self.call(py, docs, target_companies)?;
        let results = results.bind(py);

        let mut entries: Vec<ResultKeyEntry<'_>> = Vec::new();
        for key in results.keys() {
            let tup = key.cast::<PyTuple>().map_err(PyErr::from)?;
            let doc_name_obj = tup.get_item(0)?;
            let page_n_obj = tup.get_item(1)?;
            let doc_name: Option<String> = doc_name_obj.extract()?;
            let page_n: i64 = page_n_obj.extract()?;
            entries.push((doc_name, page_n, doc_name_obj, page_n_obj));
        }
        entries.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));

        let promises_map = crate::core::promise_resolution::py_build_promise_multimap(py);
        let doc_results_class = py.get_type::<crate::output::routines::DocumentResults>();
        let page_results_class = py.get_type::<crate::output::routines::PageResults>();

        let doc_results_list = PyList::empty(py);
        let mut doc_results_by_name: HashMap<Option<String>, Bound<'_, PyAny>> = HashMap::new();
        let mut seen_pages_by_name: HashMap<Option<String>, HashSet<i64>> = HashMap::new();

        for (doc_name, page_n, doc_name_obj, page_n_obj) in &entries {
            if !doc_results_by_name.contains_key(doc_name) {
                let report_name = doc_name.clone().ok_or_else(|| {
                    PyValueError::new_err(
                        "run_documents requires every document to have a name (DocumentSpec.name is never None for a valid spec)",
                    )
                })?;
                let dr = doc_results_class.call1((report_name, format_name))?;
                doc_results_list.append(&dr)?;
                doc_results_by_name.insert(doc_name.clone(), dr);
                seen_pages_by_name.insert(doc_name.clone(), HashSet::new());
            }
            let dr = doc_results_by_name.get(doc_name).expect("just inserted above if missing");
            let seen_pages = seen_pages_by_name.get_mut(doc_name).expect("just inserted above if missing");
            if !seen_pages.contains(page_n) {
                let pr = page_results_class.call0()?;
                pr.setattr("page_number", page_n_obj)?;
                dr.getattr("results")?.call_method1("append", (&pr,))?;
                seen_pages.insert(*page_n);
            }
            let last_page = dr.getattr("results")?.call_method1("__getitem__", (-1,))?;

            let key = PyTuple::new(py, [doc_name_obj, page_n_obj])?;
            let page_result_items = results.get_item(&key)?.expect("key came from results.keys() itself");
            for r in page_result_items.try_iter()? {
                let r = r?;
                if r.is_instance_of::<PyDict>() {
                    let d = r.cast::<PyDict>().map_err(PyErr::from)?;
                    crate::core::promise_resolution::py_merge_into_multimap(&promises_map, d)?;
                } else if r.is_instance_of::<crate::output::investment::Equity>()
                    || r.is_instance_of::<crate::output::investment::Bond>()
                {
                    last_page.getattr("investments")?.call_method1("append", (&r,))?;
                } else if r.is_instance_of::<crate::output::assets_manager::ManagementCompany>()
                    || r.is_instance_of::<crate::output::assets_manager::InvestmentsManager>()
                {
                    last_page.getattr("assets_managers")?.call_method1("append", (&r,))?;
                } else if r.is_instance_of::<crate::output::fund::Fund>() {
                    last_page.getattr("funds")?.call_method1("append", (&r,))?;
                } else if r.is_instance_of::<crate::output::fund_sfdr_classification::FundSfdrClassification>() {
                    last_page.getattr("funds_sfdr_classification")?.call_method1("append", (&r,))?;
                } else if r.is_instance_of::<crate::output::fund_esg_indicator::FundEsgIndicator>() {
                    last_page.getattr("funds_esg_indicators")?.call_method1("append", (&r,))?;
                } else if r.is_instance_of::<crate::output::fund_assets::FundAssets>() {
                    last_page.getattr("funds_assets")?.call_method1("append", (&r,))?;
                } else if r.is_instance_of::<crate::output::fund_change_name::FundRename>()
                    || r.is_instance_of::<crate::output::fund_change_name::FundMerge>()
                {
                    last_page.getattr("funds_change_name")?.call_method1("append", (&r,))?;
                } else {
                    return Err(PyValueError::new_err(format!("Not recognized type of result {}", r.get_type().name()?)));
                }
            }
        }

        let flattened = crate::core::promise_resolution::py_flatten_promise_map(&promises_map)?;
        for dr in doc_results_list.try_iter()? {
            dr?.call_method1("fulfill_promises", (&flattened,))?;
        }

        Ok(doc_results_list.unbind())
    }

    fn classify_pages(&self, py: Python<'_>, pages: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let mut page_classification = Vec::new();
        for p in pages.try_iter()? {
            for c in self.page_classify_bundle.bind(py).borrow().call(py, &p?, &py.None().into_bound(py))? {
                page_classification.push(c);
            }
        }
        let page_classification_list = PyList::new(py, &page_classification)?;
        Ok(self.page_classify_finalizer.bind(py).call1((page_classification_list,))?.unbind())
    }

    fn apply_to_page(&self, py: Python<'_>, pages: &Bound<'_, PyAny>, page_number: i64, filter_data: &Bound<'_, PyAny>, page_class: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let bundle = self.bundles_mapping.bind(py).get_item(page_class)?.ok_or_else(|| PyValueError::new_err("unmapped page type"))?;
        let bundle = bundle.cast::<PipelinesBundle>().map_err(PyErr::from)?;
        let page = pages.get_item(page_number - 1)?;
        bundle.borrow().call(py, &page, filter_data)
    }

    fn apply_pdf_extract(&self, py: Python<'_>, page: &Bound<'_, PyAny>, page_class: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let bundle = self.bundles_mapping.bind(py).get_item(page_class)?.ok_or_else(|| PyValueError::new_err("unmapped page type"))?;
        let bundle = bundle.cast::<PipelinesBundle>().map_err(PyErr::from)?;
        bundle.borrow().apply_pdf_extract(py, page)
    }

    fn apply_text_filter(&self, py: Python<'_>, page: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>, page_class: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let bundle = self.bundles_mapping.bind(py).get_item(page_class)?.ok_or_else(|| PyValueError::new_err("unmapped page type"))?;
        let bundle = bundle.cast::<PipelinesBundle>().map_err(PyErr::from)?;
        bundle.borrow().apply_text_filter(py, page, filter_data)
    }

    fn apply_deserialize(&self, py: Python<'_>, page: &Bound<'_, PyAny>, filter_data: &Bound<'_, PyAny>, page_class: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
        let bundle = self.bundles_mapping.bind(py).get_item(page_class)?.ok_or_else(|| PyValueError::new_err("unmapped page type"))?;
        let bundle = bundle.cast::<PipelinesBundle>().map_err(PyErr::from)?;
        bundle.borrow().apply_deserialize(py, page, filter_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::{PyDict, PyList, PySet, PyTuple};

    fn eval<'py>(py: Python<'py>, code: &str) -> Bound<'py, PyAny> {
        py.eval(std::ffi::CString::new(code).unwrap().as_c_str(), None, None).unwrap()
    }

    fn make_empty_algorithm(py: Python<'_>) -> Algorithm {
        let pipelines_map = PyDict::new(py);
        let page_classify_pipelines = PySet::empty(py).unwrap();
        let schedule = PyList::empty(py);
        let mapping = PyDict::new(py);
        Algorithm::new(py, &pipelines_map, page_classify_pipelines.as_any(), py.None(), &schedule, &mapping).unwrap()
    }

    #[test]
    fn pipe_set_dedups_identical_pipe_by_identity() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let pipe = eval(py, "lambda page: [page]");
            let mut seg = PdfExtractSegment::new(py, None).unwrap();
            seg.add_pipe(py, pipe.clone()).unwrap();
            seg.add_pipe(py, pipe.clone()).unwrap();
            assert_eq!(seg.pipes.pipes.len(), 1);
        });
    }

    #[test]
    fn pipe_set_rejects_non_callable() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let mut seg = PdfExtractSegment::new(py, None).unwrap();
            let err = seg.add_pipe(py, 42i32.into_pyobject(py).unwrap().into_any()).unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py));
        });
    }

    #[test]
    fn segment_add_unions_and_dedups() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let shared = eval(py, "lambda page: [page]");
            let only_left = eval(py, "lambda page: [page]");
            let mut left = PdfExtractSegment::new(py, None).unwrap();
            left.add_pipe(py, shared.clone()).unwrap();
            left.add_pipe(py, only_left).unwrap();
            let mut right = PdfExtractSegment::new(py, None).unwrap();
            right.add_pipe(py, shared).unwrap();
            let combined = left.__add__(py, &right);
            assert_eq!(combined.pipes.pipes.len(), 2);
        });
    }

    #[test]
    fn pipeline_call_chains_all_three_segments() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let pdf_pipe = eval(py, "lambda page: [page]");
            let text_pipe = eval(py, "lambda blks, filter_data: list(blks)");
            let deserialize_pipe = eval(py, "lambda blk: blk");

            let pipeline = Pipeline::new(py, Some(&pdf_pipe), Some(&text_pipe), Some(&deserialize_pipe)).unwrap();
            assert!(pipeline.complete(py));

            let page = eval(py, "'X'");
            let result = pipeline.call(py, &page, &py.None().into_bound(py)).unwrap();
            let result: Vec<String> = result.into_iter().map(|r| r.extract(py).unwrap()).collect();
            assert_eq!(result, vec!["X".to_string()]);
        });
    }

    #[test]
    fn pipeline_incomplete_without_a_pipe_in_every_segment() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let pdf_pipe = eval(py, "lambda page: [page]");
            let pipeline = Pipeline::new(py, Some(&pdf_pipe), None, None).unwrap();
            assert!(!pipeline.complete(py));
        });
    }

    #[test]
    fn deserialize_segment_preserves_none_but_splices_lists() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let mut seg = DeserializeSegment::new(py, None).unwrap();
            seg.add_pipe(py, eval(py, "lambda blk: None if blk == 'skip' else [blk, blk]")).unwrap();
            let blks = PyList::new(py, ["keep", "skip"]).unwrap();
            let result = seg.call(py, blks.as_any()).unwrap();
            let result: Vec<Option<String>> = result.into_iter().map(|r| r.extract(py).unwrap()).collect();
            assert_eq!(
                result,
                vec![Some("keep".to_string()), Some("keep".to_string()), None]
            );
        });
    }

    #[test]
    fn pipelines_bundle_rejects_non_pipeline() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let mut bundle = PipelinesBundle::new(py, None).unwrap();
            let not_a_pipeline = eval(py, "object()");
            let err = bundle.add_pipeline(py, &not_a_pipeline).unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py));
            assert!(err.value(py).str().unwrap().to_string().contains("Pipelines bundle can contain only Pipeline"));
        });
    }

    #[test]
    fn transform_multidocs_empty_list_wraps_as_single_none_named_doc() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let alg = make_empty_algorithm(py);
            let docs = PyList::empty(py);
            let result = alg._transform_multidocs_if_single(py, docs.as_any()).unwrap();
            assert_eq!(result.len(), 1);
            let pair = result.get_item(0).unwrap();
            let pair = pair.cast::<PyTuple>().unwrap();
            assert!(pair.get_item(0).unwrap().is_none());
            let inner = pair.get_item(1).unwrap();
            assert!(inner.is(&docs));
        });
    }

    #[test]
    fn transform_multidocs_list_of_dict_wraps_whole_list_as_sole_document() {
        // Regression test for the bug found while porting: the Python original's dict-branch
        // wrapped only `docs[0]` instead of the whole `docs` list — this pins the fixed
        // `docs=[(None,[docs])]` semantics (the *entire* flat page list becomes the one
        // document's page list).
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let alg = make_empty_algorithm(py);
            let page_a = eval(py, "{'page': 'a'}");
            let page_b = eval(py, "{'page': 'b'}");
            let docs = PyList::new(py, [page_a, page_b]).unwrap();
            let result = alg._transform_multidocs_if_single(py, docs.as_any()).unwrap();
            assert_eq!(result.len(), 1);
            let item = result.get_item(0).unwrap();
            let pair = item.cast::<PyTuple>().unwrap();
            assert!(pair.get_item(0).unwrap().is_none());
            let wrapped_docs = pair.get_item(1).unwrap();
            let wrapped_docs = wrapped_docs.cast::<PyList>().unwrap();
            assert_eq!(wrapped_docs.len(), 1);
            let sole_document_pages = wrapped_docs.get_item(0).unwrap();
            assert!(sole_document_pages.is(&docs));
        });
    }

    #[test]
    fn transform_multidocs_list_of_list_becomes_one_none_named_doc_per_sublist() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let alg = make_empty_algorithm(py);
            let doc1_pages = PyList::new(py, ["p1"]).unwrap();
            let doc2_pages = PyList::new(py, ["p2"]).unwrap();
            let docs = PyList::new(py, [doc1_pages.clone(), doc2_pages.clone()]).unwrap();
            let result = alg._transform_multidocs_if_single(py, docs.as_any()).unwrap();
            assert_eq!(result.len(), 2);
            for (i, expected) in [doc1_pages, doc2_pages].into_iter().enumerate() {
                let item = result.get_item(i).unwrap();
                let pair = item.cast::<PyTuple>().unwrap();
                assert!(pair.get_item(0).unwrap().is_none());
                assert!(pair.get_item(1).unwrap().is(&expected));
            }
        });
    }

    #[test]
    fn transform_multidocs_list_of_tuple_passes_through_unchanged() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let alg = make_empty_algorithm(py);
            let doc = PyTuple::new(py, [eval(py, "'doc-a'"), PyList::new(py, ["p1"]).unwrap().into_any()]).unwrap();
            let docs = PyList::new(py, [doc]).unwrap();
            let result = alg._transform_multidocs_if_single(py, docs.as_any()).unwrap();
            assert!(result.is(&docs));
        });
    }

    #[test]
    fn transform_multidocs_bare_tuple_gets_wrapped_in_a_list() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let alg = make_empty_algorithm(py);
            let doc = PyTuple::new(py, [eval(py, "'doc-a'"), PyList::new(py, ["p1"]).unwrap().into_any()]).unwrap();
            let result = alg._transform_multidocs_if_single(py, doc.as_any()).unwrap();
            assert_eq!(result.len(), 1);
            assert!(result.get_item(0).unwrap().is(&doc));
        });
    }

    #[test]
    fn transform_multidocs_rejects_unrecognized_shapes() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let alg = make_empty_algorithm(py);

            let not_a_list_or_tuple = 42i32.into_pyobject(py).unwrap().into_any();
            assert!(alg._transform_multidocs_if_single(py, &not_a_list_or_tuple).is_err());

            let list_of_ints = PyList::new(py, [1, 2, 3]).unwrap();
            assert!(alg._transform_multidocs_if_single(py, list_of_ints.as_any()).is_err());
        });
    }

    // --- run_documents ---

    /// A deserialize pipe that always returns `result`, whatever page it's given — built by
    /// injecting `result` into an `eval` namespace rather than reconstructing it *inside* the
    /// Python code string. Constructing an output-class instance via a string like
    /// `"freeports._native.Fund(...)"` would go through `sys.modules['freeports._native']` — the
    /// version of this crate *installed in the venv*, not the one `cargo test` just compiled —
    /// hitting the exact cross-module PyO3 identity trap this whole method exists to avoid (see
    /// its own doc comment). Building `result` in Rust via `py.get_type::<T>()` first and handing
    /// it in as a ready-made object sidesteps that entirely.
    fn fixed_result_pipe<'py>(py: Python<'py>, result: &Bound<'py, PyAny>) -> Bound<'py, PyAny> {
        let globals = PyDict::new(py);
        globals.set_item("__RESULT__", result).unwrap();
        py.eval(std::ffi::CString::new("lambda blk: __RESULT__").unwrap().as_c_str(), Some(&globals), None).unwrap()
    }

    /// A minimal 1-page-type ("BODY"), 1-pipeline Algorithm whose pdf_extract/text_filter pipes
    /// are pass-through and whose deserialize pipe is `deserialize_pipe` — enough to drive
    /// `run_documents` end to end without needing a real formats-repo fixture (`Algorithm::load`'s
    /// acquisition functions aren't exercised at all). The page-classify bundle isn't left empty:
    /// it runs a trivial classify pipeline that emits one `"BODY"` marker per page, so the
    /// finalizer (identity here) sees an input the same length as the page list regardless of how
    /// many pages a test document has — an *actually* empty classify bundle would always feed the
    /// finalizer a 0-length list, breaking on anything but a single-page document.
    fn make_algorithm_with_one_pipe<'py>(py: Python<'py>, deserialize_pipe: Bound<'py, PyAny>) -> Algorithm {
        let pdf_pipe = eval(py, "lambda page: [page]");
        let text_pipe = eval(py, "lambda blks, filter_data: list(blks)");
        let pipeline = Pipeline::new(py, Some(&pdf_pipe), Some(&text_pipe), Some(&deserialize_pipe)).unwrap();
        let pipeline = Py::new(py, pipeline).unwrap();

        let classify_pdf_pipe = eval(py, "lambda page: [page]");
        let classify_text_pipe = eval(py, "lambda blks, filter_data: list(blks)");
        let classify_deserialize_pipe = eval(py, "lambda blk: 'BODY'");
        let classify_pipeline =
            Pipeline::new(py, Some(&classify_pdf_pipe), Some(&classify_text_pipe), Some(&classify_deserialize_pipe)).unwrap();
        let classify_pipeline = Py::new(py, classify_pipeline).unwrap();

        let pipelines_map = PyDict::new(py);
        pipelines_map.set_item("main", pipeline).unwrap();
        pipelines_map.set_item("classify", &classify_pipeline).unwrap();
        let page_classify_pipelines = PySet::new(py, ["classify"]).unwrap();
        let finalizer = eval(py, "lambda classification: list(classification)");
        let schedule = PyList::new(py, [PySet::new(py, ["BODY"]).unwrap()]).unwrap();
        let mapping = PyDict::new(py);
        mapping.set_item("BODY", PyList::new(py, ["main"]).unwrap()).unwrap();
        Algorithm::new(py, &pipelines_map, page_classify_pipelines.as_any(), finalizer.unbind(), &schedule, &mapping).unwrap()
    }

    fn doc_with_pages<'py>(py: Python<'py>, doc_name: &str, n_pages: usize) -> Bound<'py, PyAny> {
        let pages: Vec<&str> = (0..n_pages).map(|_| "page").collect();
        let pages = PyList::new(py, pages).unwrap();
        PyList::new(py, [PyTuple::new(py, [doc_name.into_pyobject(py).unwrap().into_any(), pages.into_any()]).unwrap()])
            .unwrap()
            .into_any()
    }

    fn one_page_one_doc<'py>(py: Python<'py>, doc_name: &str) -> Bound<'py, PyAny> {
        doc_with_pages(py, doc_name, 1)
    }

    fn fund<'py>(py: Python<'py>, name: &str) -> Bound<'py, PyAny> {
        py.get_type::<crate::output::fund::Fund>().call1((name,)).unwrap()
    }

    #[test]
    fn run_documents_dispatches_fund() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let f = fund(py, "TestFund");
            let pipe = fixed_result_pipe(py, &f);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let docs = one_page_one_doc(py, "Doc1");
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, &docs, targets.as_any(), "TestFormat").unwrap();
            let result = result.bind(py);
            assert_eq!(result.len(), 1);
            let doc_results = result.get_item(0).unwrap();
            assert_eq!(doc_results.getattr("report_name").unwrap().extract::<String>().unwrap(), "Doc1");
            let page0 = doc_results.getattr("results").unwrap().call_method1("__getitem__", (0,)).unwrap();
            assert_eq!(page0.getattr("funds").unwrap().len().unwrap(), 1);
        });
    }

    #[test]
    fn run_documents_dispatches_management_company_as_assets_manager() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let managed_funds = PyList::empty(py);
            let mc = py
                .get_type::<crate::output::assets_manager::ManagementCompany>()
                .call1(("MyCo", managed_funds))
                .unwrap();
            let pipe = fixed_result_pipe(py, &mc);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let docs = one_page_one_doc(py, "Doc1");
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, &docs, targets.as_any(), "TestFormat").unwrap();
            let doc_results = result.bind(py).get_item(0).unwrap();
            let page0 = doc_results.getattr("results").unwrap().call_method1("__getitem__", (0,)).unwrap();
            assert_eq!(page0.getattr("assets_managers").unwrap().len().unwrap(), 1);
        });
    }

    #[test]
    fn run_documents_dispatches_fund_rename_as_fund_change_name() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let date = eval(py, "__import__('datetime').date(2024, 1, 1)");
            let rename = py
                .get_type::<crate::output::fund_change_name::FundRename>()
                .call1(("Old", "New", date))
                .unwrap();
            let pipe = fixed_result_pipe(py, &rename);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let docs = one_page_one_doc(py, "Doc1");
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, &docs, targets.as_any(), "TestFormat").unwrap();
            let doc_results = result.bind(py).get_item(0).unwrap();
            let page0 = doc_results.getattr("results").unwrap().call_method1("__getitem__", (0,)).unwrap();
            assert_eq!(page0.getattr("funds_change_name").unwrap().len().unwrap(), 1);
        });
    }

    #[test]
    fn run_documents_dispatches_equity_as_investment() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let currency = crate::commons::consts::Currency::EUR.into_pyobject(py).unwrap();
            let kwargs = PyDict::new(py);
            kwargs.set_item("company", "ACME").unwrap();
            kwargs.set_item("company_match", "ACME").unwrap();
            kwargs.set_item("fund", "F").unwrap();
            kwargs.set_item("market_value", 1.0).unwrap();
            kwargs.set_item("currency", currency).unwrap();
            let equity = py.get_type::<crate::output::investment::Equity>().call((), Some(&kwargs)).unwrap();
            let pipe = fixed_result_pipe(py, &equity);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let docs = one_page_one_doc(py, "Doc1");
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, &docs, targets.as_any(), "TestFormat").unwrap();
            let doc_results = result.bind(py).get_item(0).unwrap();
            let page0 = doc_results.getattr("results").unwrap().call_method1("__getitem__", (0,)).unwrap();
            assert_eq!(page0.getattr("investments").unwrap().len().unwrap(), 1);
        });
    }

    #[test]
    fn run_documents_dispatches_fund_sfdr_classification() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let article = crate::commons::consts::SfdrArticle::ART_6.into_pyobject(py).unwrap();
            let sfdr = py
                .get_type::<crate::output::fund_sfdr_classification::FundSfdrClassification>()
                .call1(("F", article))
                .unwrap();
            let pipe = fixed_result_pipe(py, &sfdr);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let docs = one_page_one_doc(py, "Doc1");
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, &docs, targets.as_any(), "TestFormat").unwrap();
            let doc_results = result.bind(py).get_item(0).unwrap();
            let page0 = doc_results.getattr("results").unwrap().call_method1("__getitem__", (0,)).unwrap();
            assert_eq!(page0.getattr("funds_sfdr_classification").unwrap().len().unwrap(), 1);
        });
    }

    #[test]
    fn run_documents_dispatches_fund_esg_indicator() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let esg = py
                .get_type::<crate::output::fund_esg_indicator::FundEsgIndicator>()
                .call1(("F", "Indicator", "value"))
                .unwrap();
            let pipe = fixed_result_pipe(py, &esg);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let docs = one_page_one_doc(py, "Doc1");
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, &docs, targets.as_any(), "TestFormat").unwrap();
            let doc_results = result.bind(py).get_item(0).unwrap();
            let page0 = doc_results.getattr("results").unwrap().call_method1("__getitem__", (0,)).unwrap();
            assert_eq!(page0.getattr("funds_esg_indicators").unwrap().len().unwrap(), 1);
        });
    }

    #[test]
    fn run_documents_dispatches_fund_assets() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let currency = crate::commons::consts::Currency::EUR.into_pyobject(py).unwrap();
            let kwargs = PyDict::new(py);
            kwargs.set_item("fund", "F").unwrap();
            kwargs.set_item("tot_assets", 100.0).unwrap();
            kwargs.set_item("liabilities", 10.0).unwrap();
            kwargs.set_item("net_assets", 90.0).unwrap();
            kwargs.set_item("currency", currency).unwrap();
            let fund_assets = py.get_type::<crate::output::fund_assets::FundAssets>().call((), Some(&kwargs)).unwrap();
            let pipe = fixed_result_pipe(py, &fund_assets);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let docs = one_page_one_doc(py, "Doc1");
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, &docs, targets.as_any(), "TestFormat").unwrap();
            let doc_results = result.bind(py).get_item(0).unwrap();
            let page0 = doc_results.getattr("results").unwrap().call_method1("__getitem__", (0,)).unwrap();
            assert_eq!(page0.getattr("funds_assets").unwrap().len().unwrap(), 1);
        });
    }

    #[test]
    fn run_documents_rejects_unrecognized_result_type() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let marker = 42i32.into_pyobject(py).unwrap().into_any();
            let pipe = fixed_result_pipe(py, &marker);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let docs = one_page_one_doc(py, "Doc1");
            let targets = PyList::empty(py);
            let err = alg.run_documents(py, &docs, targets.as_any(), "TestFormat").unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py));
        });
    }

    #[test]
    fn run_documents_errors_when_a_document_has_no_name() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let f = fund(py, "TestFund");
            let pipe = fixed_result_pipe(py, &f);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let pages = PyList::new(py, ["page-1"]).unwrap();
            let docs = PyList::new(py, [PyTuple::new(py, [py.None(), pages.into_any().unbind()]).unwrap()]).unwrap();
            let targets = PyList::empty(py);
            let err = alg.run_documents(py, docs.as_any(), targets.as_any(), "TestFormat").unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py));
        });
    }

    #[test]
    fn run_documents_groups_multiple_documents_and_sorts_by_name() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let f = fund(py, "TestFund");
            let pipe = fixed_result_pipe(py, &f);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let pages_b = PyList::new(py, ["page-1"]).unwrap();
            let pages_a = PyList::new(py, ["page-1"]).unwrap();
            let docs = PyList::new(
                py,
                [
                    PyTuple::new(py, [eval(py, "'Bravo'"), pages_b.into_any()]).unwrap(),
                    PyTuple::new(py, [eval(py, "'Alpha'"), pages_a.into_any()]).unwrap(),
                ],
            )
            .unwrap();
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, docs.as_any(), targets.as_any(), "TestFormat").unwrap();
            let result = result.bind(py);
            assert_eq!(result.len(), 2);
            assert_eq!(result.get_item(0).unwrap().getattr("report_name").unwrap().extract::<String>().unwrap(), "Alpha");
            assert_eq!(result.get_item(1).unwrap().getattr("report_name").unwrap().extract::<String>().unwrap(), "Bravo");
        });
    }

    #[test]
    fn run_documents_creates_separate_page_results_per_page() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let f = fund(py, "TestFund");
            let pipe = fixed_result_pipe(py, &f);
            let alg = make_algorithm_with_one_pipe(py, pipe);
            let docs = doc_with_pages(py, "Doc1", 3);
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, &docs, targets.as_any(), "TestFormat").unwrap();
            let doc_results = result.bind(py).get_item(0).unwrap();
            assert_eq!(doc_results.getattr("results").unwrap().len().unwrap(), 3);
            for i in 0..3 {
                let page = doc_results.getattr("results").unwrap().call_method1("__getitem__", (i,)).unwrap();
                assert_eq!(page.getattr("page_number").unwrap().extract::<i64>().unwrap(), i as i64 + 1);
                assert_eq!(page.getattr("funds").unwrap().len().unwrap(), 1);
            }
        });
    }

    #[test]
    fn run_documents_resolves_promises_across_the_whole_document() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            // First page yields a Fund carrying an unresolved name Promise; second page yields the
            // dict that resolves it. `run_documents` must merge promises across the whole document
            // before calling `fulfill_promises`, not per page.
            let promise = crate::core::promise::Promise::from_parts("fund-name", false, false);
            let promise = promise.into_pyobject(py).unwrap().into_any();
            let fund_with_promise = py.get_type::<crate::output::fund::Fund>().call1((promise,)).unwrap();
            let resolving_dict = PyDict::new(py);
            resolving_dict.set_item("fund-name", "Resolved Name").unwrap();

            let globals = PyDict::new(py);
            globals.set_item("__FUND__", &fund_with_promise).unwrap();
            globals.set_item("__RESOLVER__", &resolving_dict).unwrap();
            let pipe = py
                .eval(
                    std::ffi::CString::new("lambda blk: __FUND__ if blk == 'page-1' else __RESOLVER__").unwrap().as_c_str(),
                    Some(&globals),
                    None,
                )
                .unwrap();

            let alg = make_algorithm_with_one_pipe(py, pipe);
            let pages = PyList::new(py, ["page-1", "page-2"]).unwrap();
            let docs = PyList::new(py, [PyTuple::new(py, [eval(py, "'Doc1'"), pages.into_any()]).unwrap()]).unwrap();
            let targets = PyList::empty(py);
            let result = alg.run_documents(py, docs.as_any(), targets.as_any(), "TestFormat").unwrap();
            let doc_results = result.bind(py).get_item(0).unwrap();
            let page0 = doc_results.getattr("results").unwrap().call_method1("__getitem__", (0,)).unwrap();
            let funds = page0.getattr("funds").unwrap();
            assert_eq!(funds.len().unwrap(), 1);
            let fund = funds.get_item(0).unwrap();
            assert_eq!(fund.getattr("name").unwrap().extract::<String>().unwrap(), "RESOLVED NAME");
            // The dict result itself never becomes an investment/fund/etc entry on page 2.
            let page1 = doc_results.getattr("results").unwrap().call_method1("__getitem__", (1,)).unwrap();
            assert_eq!(page1.getattr("funds").unwrap().len().unwrap(), 0);
        });
    }

    // ============================================================
    // Algorithm::load -- end-to-end fixture (Step 1.6 of
    // agent-memory/detect-format-metadata-rust-port-implementation-plan.md). `Algorithm::load` had
    // **zero** dedicated Rust tests before this -- the flagged pre-existing coverage gap this step
    // is meant to close.
    //
    // **Cross-module PyO3 identity constraint, discovered while writing this test (not previously
    // documented for this specific case)**: `Algorithm::load` cannot be called as a plain native
    // Rust function (`Algorithm::load(py, ...)`) from a `cargo test --lib` unit test the way every
    // other test in this file calls `Pipeline::new`/`PdfExtractSegment::new`/etc. directly --
    // `load` internally calls `py.import("...pipelines_acquisition").call_method(...)`, which
    // constructs `Pipeline` objects via the *actually installed* `_native.cpython-*.so`
    // (whatever `sys.path` resolves `import freeports._native` to), a **different compiled
    // artifact** from the one `cargo test --lib` links into its own test binary -- confirmed
    // empirically (not assumed): `py.get_type::<Pipeline>().is(&python_side_pipeline.get_type())`
    // is `false` even though both print as `<class 'freeports._native.Pipeline'>`, and
    // `python_side_pipeline.cast::<Pipeline>()` fails, the moment `PipelinesBundle::new` tries to
    // fold a real Python-constructed `Pipeline` into a native bundle. This is the same class of
    // trap `main.rs`'s and `companies_db.rs`'s own doc comments already describe for *production*
    // code (a second, differently-compiled copy of a `#[pyclass]` can never `isinstance`/cast
    // against the first) -- it just hadn't previously been hit by a *test*, because every earlier
    // test in this file builds its `Pipeline`/`PipelinesBundle` values natively in Rust rather than
    // by round-tripping through a real `pipelines_acquisition.get_pipelines` Python call. Since
    // `get_pageclassify_pipelines` never returns a truly empty set (its own no-rows fallback is the
    // *singleton* `{""}`, per Step 1.5's own findings, and a real aggregated row set is never empty
    // either), any successful `Algorithm::load` run necessarily pulls at least one real `Pipeline`
    // value out of `pipelines_map` and folds it into `page_classify_bundle` -- so this cast failure
    // is unavoidable for *any* fixture, not specific to the one first tried here.
    //
    // **Fix**: call `Algorithm.load` the same way *production* code does (`job.rs`'s
    // `run_job_attached`) -- through `py.import("freeports._native").getattr("core")
    // .getattr("Algorithm").call_method1("load", ...)`, the *installed* module's own classmethod,
    // never a direct Rust call into this crate's own copy. Every subsequent operation (the
    // `Pipeline`/`PipelinesBundle` construction inside `load`, and every method called on the
    // resulting `Algorithm` below) then happens entirely inside that one, single, consistently
    // identified compiled artifact -- no cast ever crosses the two-copies boundary. The returned
    // `alg` is therefore an *opaque* `Bound<'_, PyAny>` (an instance of the installed module's own
    // `Algorithm` class, not castable to this crate's own `Algorithm` struct either, same reason) --
    // its private Rust fields (`schedule`/`bundles_mapping`/`page_classify_bundle`/`page_classes`)
    // are never read directly. Instead, this test asserts on `Algorithm`'s own public,
    // Python-visible methods (`schedule_pages`/`classify_pages`/`apply_to_page`), which is also
    // exactly what the task instructions ask for ("assert only on observable `Algorithm` output").
    //
    // Written as a regression pin against *today's* Python-backed
    // `orchestration.get_pageclassify_pipelines`/`get_schedule`/`get_mapping` calls inside `load`
    // (still `py.import` at the time this test was written) -- it must keep passing unchanged once
    // `implementer` swaps those 3 calls to
    // `crate::formats_repo::orchestration::{get_pageclassify_pipelines, get_schedule, get_mapping}`
    // in the same step, since native behavior must be equivalent, not different.
    // ============================================================

    /// Writes `<dir>/metadata/formats.csv` with one row for `TESTFMT-EN24` (Name=TestFmt,
    /// Locale=EN, Year=2024, no Country/Version). Not actually read by `Algorithm::load` itself
    /// (it takes already-resolved `format_name`/`format_names` arguments, never calls
    /// `metadata::get_formats` itself) -- included anyway so this fixture is a genuinely "complete"
    /// on-disk formats repo, per this step's own fixture requirement, and reusable as-is by any
    /// future test that does need it.
    fn write_load_fixture_formats_csv(dir: &std::path::Path) {
        let metadata_dir = dir.join("metadata");
        std::fs::create_dir_all(&metadata_dir).unwrap();
        std::fs::write(metadata_dir.join("formats.csv"), "Name,Locale,Year,Country,Version\nTestFmt,EN,2024,,\n").unwrap();
    }

    /// Mirrors (does not call -- it's a private `fn` inside that file's own `#[cfg(test)] mod
    /// tests`, not `pub(crate)`, so not reachable from here) `formats_repo::orchestration`'s own
    /// `python_acquisition_fixture` helper
    /// (`packages/freeports_engine/src/formats_repo/orchestration.rs`). See that helper's doc
    /// comment for the ground-truth reasoning (read directly off `pipelines_acquisition
    /// .get_pipelines`'s 3 callees, not guessed) on exactly which files must exist on disk, and in
    /// which shape, for that Python call to run to completion without crashing: all 5
    /// `structured/investments|page_classify` CSVs, plus `semistructured/formats_mapping.csv` and
    /// its 3 `args/*.yaml` files -- `unstructured/` needs no on-disk presence at all (until this
    /// fixture adds its own module below). Every structured/semistructured file here is left
    /// header-only/empty -- this test's real pipelines come entirely from the unstructured leg, see
    /// [`write_load_fixture_unstructured_module`].
    fn write_load_fixture_pipelines_acquisition_baseline(dir: &std::path::Path) {
        let investments_dir = dir.join("content/algorithms/structured/investments");
        std::fs::create_dir_all(&investments_dir).unwrap();
        std::fs::write(
            investments_dir.join("args.csv"),
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n",
        )
        .unwrap();
        std::fs::write(
            investments_dir.join("additional_args.csv"),
            "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous\n",
        )
        .unwrap();
        std::fs::write(investments_dir.join("deselection_lists.csv"), "ID,Deselection set\n").unwrap();
        std::fs::write(investments_dir.join("partial_pipes.csv"), "ID,pdf_extract,text_filter,deserialize\n").unwrap();

        let page_classify_dir = dir.join("content/algorithms/structured/page_classify");
        std::fs::create_dir_all(&page_classify_dir).unwrap();
        std::fs::write(page_classify_dir.join("args.csv"), "ID,Header set,Class\n").unwrap();

        let semistructured_dir = dir.join("content/algorithms/semistructured");
        std::fs::create_dir_all(&semistructured_dir).unwrap();
        std::fs::write(semistructured_dir.join("formats_mapping.csv"), "ID,pdf_extract,text_filter,deserialize\n").unwrap();

        let args_dir = semistructured_dir.join("args");
        std::fs::create_dir_all(&args_dir).unwrap();
        std::fs::write(args_dir.join("pdf_extract.yaml"), "").unwrap();
        std::fs::write(args_dir.join("text_filter.yaml"), "").unwrap();
        std::fs::write(args_dir.join("deserialize.yaml"), "").unwrap();
    }

    /// Writes `content/algorithms/unstructured/testfmt_en24.py` (module name derived from
    /// `TESTFMT-EN24` per `unstructured/acquisition.py`'s `get_module`: lowercased,
    /// `-`/`.`/`@` -> `_`) defining two trivially-complete (all 3 segments present, distinctly
    /// tagged so each one's output is unambiguous in assertions) `Pipeline`s: `"classify_pipe"`
    /// (meant to be used only for page classification) and `"content_pipe"` (meant to be used only
    /// as a real scheduled page type's pipeline) -- picked over the real built-in structured
    /// `"investments"` pipeline (which needs genuine PDF-block-shaped input to do anything
    /// observable) specifically so this test can drive `Algorithm`'s public methods with plain
    /// strings, the same style every other test in this file already uses (see `eval`'s `"lambda
    /// page: [page]"` pipes above).
    fn write_load_fixture_unstructured_module(dir: &std::path::Path) {
        let unstructured_dir = dir.join("content/algorithms/unstructured");
        std::fs::create_dir_all(&unstructured_dir).unwrap();
        std::fs::write(
            unstructured_dir.join("testfmt_en24.py"),
            "from freeports import _native\n\n\nPipeline = _native.core.Pipeline\n\n\ndef _classify_pdf_extract(page):\n    return [f'classify:{page}']\n\n\ndef _content_pdf_extract(page):\n    return [f'content:{page}']\n\n\ndef _text_filter(blks, filter_data):\n    return list(blks)\n\n\ndef _deserialize(blk):\n    return blk\n\n\npipelines = {\n    'classify_pipe': Pipeline(_classify_pdf_extract, _text_filter, _deserialize),\n    'content_pipe': Pipeline(_content_pdf_extract, _text_filter, _deserialize),\n}\n",
        )
        .unwrap();
    }

    fn write_load_fixture_orchestration_csv(dir: &std::path::Path, file_name: &str, csv_text: &str) {
        let orchestration_dir = dir.join("content").join("orchestration");
        std::fs::create_dir_all(&orchestration_dir).unwrap();
        std::fs::write(orchestration_dir.join(file_name), csv_text).unwrap();
    }

    /// Builds a complete, small, on-disk formats-repo fixture for one format (`TESTFMT-EN24`) with
    /// exactly two real, complete pipelines (`"classify_pipe"`/`"content_pipe"`, both unstructured):
    /// one used only for page classification, one used only for a real scheduled page type
    /// (`"cover"`) -- exercising both `page_classify_bundle` and `bundles_mapping` non-trivially in
    /// the same fixture, via a single real (non-fallback) row in each orchestration CSV.
    ///
    /// Hand-computed trace, cross-checked against `orchestration.py`/`pipelines_acquisition.py`
    /// directly (not guessed):
    /// - `pipelines_acquisition.get_pipelines`: structured/semistructured legs both resolve to `{}`
    ///   (every baseline CSV is header-only); the unstructured leg resolves to
    ///   `{"classify_pipe": Pipeline(...), "content_pipe": Pipeline(...)}` (both complete, 3
    ///   segments each) -> `known_pipelines = {"classify_pipe", "content_pipe"}`.
    /// - `pageclassify_overwrite.csv` has one row, `TESTFMT-EN24(classify_pipe)` ->
    ///   `orchestration.get_pageclassify_pipelines` returns `{"classify_pipe"}` (a real row, not
    ///   the no-rows `{""}` fallback).
    /// - `mapping.csv` has one row, `TESTFMT-EN24(content_pipe),cover` ->
    ///   `orchestration.get_mapping` returns `{"cover": {"content_pipe"}}` directly (a real row --
    ///   this fixture doesn't exercise `get_mapping`'s Python-fallback branch at all, unlike some
    ///   of `orchestration.rs`'s own tests).
    /// - `algorithms_schedule.csv` has one row, `TESTFMT-EN24,cover,` (blank `Filter next
    ///   iteration` -> defaults `false`) -> `orchestration.get_schedule` returns `[{"cover"}]` (one
    ///   step, containing just `"cover"`).
    /// - `Algorithm::new`'s cross-checks all hold: `page_classify_pipelines` (`{"classify_pipe"}`)
    ///   is a subset of `known_pipelines`; `page_classes` (union over the schedule, `{"cover"}`)
    ///   equals `page_type_pipelines_mapping`'s keys (`{"cover"}`); `tot_pipelines_names`
    ///   (`{"content_pipe"}` union `{"classify_pipe"}`) equals `known_pipelines`.
    ///
    /// Expected `Algorithm::load` result (asserted below via `Algorithm`'s own public methods, see
    /// this test module's own doc comment on why not via direct field access):
    /// `page_classify_bundle` holds exactly `"classify_pipe"`; `schedule` is `[{"cover"}]`;
    /// `bundles_mapping` is `{"cover": Pipeline("content_pipe")}` only.
    fn write_algorithm_load_fixture(dir: &std::path::Path) {
        write_load_fixture_formats_csv(dir);
        write_load_fixture_pipelines_acquisition_baseline(dir);
        write_load_fixture_unstructured_module(dir);
        write_load_fixture_orchestration_csv(dir, "algorithms_schedule.csv", "Format name,Page type,Filter next iteration\nTESTFMT-EN24,cover,\n");
        write_load_fixture_orchestration_csv(dir, "pageclassify_overwrite.csv", "ID\nTESTFMT-EN24(classify_pipe)\n");
        write_load_fixture_orchestration_csv(dir, "mapping.csv", "ID,Page type\nTESTFMT-EN24(content_pipe),cover\n");
    }

    #[test]
    fn algorithm_load_end_to_end_against_a_complete_small_formats_repo() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let dir = tempfile::tempdir().unwrap();
            write_algorithm_load_fixture(dir.path());

            // Same call shape as `job.rs`'s `run_job_attached` -- see this section's own doc
            // comment on why a direct `Algorithm::load(py, ...)` Rust call cannot work here.
            let core = py.import("freeports._native").unwrap().getattr("core").unwrap();
            let alg = core
                .getattr("Algorithm")
                .unwrap()
                .call_method1("load", (dir.path(), "TESTFMT-EN24", vec!["TESTFMT-EN24"]))
                .unwrap();

            // schedule: one step, exactly the one page type "cover" (an initially-empty bucket).
            let docs = PyList::empty(py);
            let scheduled = alg.call_method1("schedule_pages", (docs.as_any(),)).unwrap();
            let scheduled = scheduled.cast::<PyList>().unwrap();
            assert_eq!(scheduled.len(), 1);
            let step0 = scheduled.get_item(0).unwrap();
            let step0 = step0.cast::<PyDict>().unwrap();
            assert_eq!(step0.len(), 1);
            assert!(step0.contains("cover").unwrap());
            assert_eq!(step0.get_item("cover").unwrap().unwrap().len().unwrap(), 0);

            // page_classify_bundle: exactly "classify_pipe" (tagged output distinguishes it from
            // "content_pipe", which must never run here).
            let pages = PyList::new(py, ["X"]).unwrap();
            let classified = alg.call_method1("classify_pages", (pages.as_any(),)).unwrap();
            let classified: Vec<String> = classified.try_iter().unwrap().map(|r| r.unwrap().extract().unwrap()).collect();
            assert_eq!(classified, vec!["classify:X".to_string()]);

            // bundles_mapping: exactly {"cover": Pipeline("content_pipe")} -- "cover" resolves to
            // the tagged "content_pipe" output, ...
            let filter_data = PyList::empty(py);
            let cover_pages = PyList::new(py, ["Y"]).unwrap();
            let cover_result = alg.call_method1("apply_to_page", (cover_pages.as_any(), 1i64, filter_data.as_any(), "cover")).unwrap();
            let cover_result: Vec<String> = cover_result.try_iter().unwrap().map(|r| r.unwrap().extract().unwrap()).collect();
            assert_eq!(cover_result, vec!["content:Y".to_string()]);

            // ... and "classify_pipe" itself is *not* a mapped page type (it was entirely consumed
            // by page classification, per the disjointness `Algorithm::new` enforces at
            // construction time) -- confirms `bundles_mapping` has no other entries.
            let err = alg
                .call_method1("apply_to_page", (cover_pages.as_any(), 1i64, filter_data.as_any(), "classify_pipe"))
                .unwrap_err();
            assert!(err.is_instance_of::<PyValueError>(py));
        });
    }
}
