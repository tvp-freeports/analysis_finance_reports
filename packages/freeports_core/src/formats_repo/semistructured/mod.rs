//! Rust port of `packages/freeports_core/src/freeports/_internals/formats/repo/algorithms/
//! semistructured/acquisition.py`'s `get_pipelines` (plus the `getattr`-by-name dispatch
//! `_get_segment` performs today) — the hybrid native-Rust-first/author-Python-fallback dispatch
//! layer for the 3 semistructured segment types (`pdf_extract`, `text_filter`, `deserialize`).
//!
//! See `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`, Milestone 2's
//! Decisions 1-6 and "Sequencing" item 3, for the full design context — this module is that
//! sequencing item. It sits directly on top of two already-landed sibling modules: [`formats_mapping`]
//! (sequencing item 1 — `formats_mapping.csv` row reading/ID derivation) and [`native`] (sequencing
//! item 2's `standard_cost_curr` port, plus this step's own small name registry).
//!
//! # Ground truth read directly off `acquisition.py`/`unstructured/acquisition.py` (not guessed)
//!
//! - **The *only* dispatch mechanism that exists in Python today is native-equivalent**:
//!   `_get_segment`'s `getattr(p, mapping[segment_name])` always resolves against the single,
//!   fixed, imported `pdf_extract`/`text_filter`/`deserialize` module (`p`/`t`/`d` — always the
//!   *built-in* `semistructured/{pdf_extract,text_filter,deserialize}.py`, never a per-format or
//!   author-provided module) — there is no format-author extensibility for this family at all
//!   today. `resolve`'s author-fallback branch (this task) is a **new capability**, not a port of
//!   existing Python behavior; only the native-registry-match branch has a direct Python
//!   equivalent (`formats_mapping_schema`'s `pa.Check(lambda x: x.isin(pdf_extract_funcs))`,
//!   itself derived from `inspect.getmembers(p, inspect.isfunction)` — i.e. exactly the same
//!   "every top-level function actually defined in the built-in module" set this module's own
//!   [`native::names`] hand-codes for `pdf_extract`, and correctly comes out empty for
//!   `text_filter`/`deserialize`, since both of those built-in `.py` files are empty today).
//! - **The author-module-loading idiom (`spec_from_file_location`/`module_from_spec`/
//!   `sys.modules[...]`/`exec_module`) is reused from `unstructured/acquisition.py`'s `get_module`
//!   conceptually, but that function itself is never called** (Decision 3 — `unstructured/
//!   acquisition.py` stays entirely untouched, permanently, per the requirements note). This
//!   module's own author-module loader is an independent implementation of the same idiom, called
//!   via `py.import("importlib.util")` from Rust, not a new Python glue file.
//! - **`_get_segment`'s exact per-row algorithm**, read directly off `acquisition.py` (not the
//!   file's own docstrings, which are stale about the return shape — see below): args are loaded
//!   once per segment (`yaml.safe_load((formats_repo_dir / ARGS_DIR / f"{segment_name}.yaml")
//!   .open("r"))`), **unconditionally, for all 3 segments, every `get_pipelines` call** — this
//!   happens *before* the per-row loop that skips null cells, so a missing
//!   `args/{segment}.yaml` file is a hard error even for a segment the requested format never uses
//!   at all (confirmed by reading the function body: the `open()` call is not inside, or guarded
//!   by, the `for pipeline, mapping in pipes_mapping:` loop). For each row with a non-null cell,
//!   `algorithm_id = f"{format_name}({pipeline})"`; the args dict is looked up by `algorithm_id`
//!   first, falling back to the bare `format_name` key **only when `pipeline == ""`** (an empty
//!   pipeline name renders `algorithm_id` as the literal string `f"{format_name}()"`, which will
//!   almost never be an actual YAML key — the fallback is what actually gets used in that case,
//!   confirmed against the real, checked-in `args/pdf_extract.yaml`, whose `AMUNDI-IT24
//!   (investments)`/`FIDEURAM-IT24(investments)` keys are both non-empty-pipeline forms, so this
//!   fallback path isn't exercised by the real fixture at all, but the code path is real and
//!   reachable); for a non-empty pipeline, a missing key propagates as an uncaught `KeyError` with
//!   no fallback attempted, even if the bare `format_name` key happens to exist. If the resolved
//!   args value is a `list`, the entry actually used for this particular row is
//!   `list[len(segment[pipeline])]` — the count of pipe callables *already appended* to this
//!   `(pipeline, segment)`'s own output list so far, **not** the CSV row's own `Pipe index`
//!   (`formats_mapping.rs`'s own per-`(format,pipeline)` cumcount) — those two counters coincide
//!   only when every row for a given `(format, pipeline)` pair contributes exactly one callable to
//!   this exact segment; they diverge the moment any earlier row for the same pipeline+segment
//!   resolves to something that expands into more than one callable (exactly `standard_cost_curr`'s
//!   own case — it always produces 3 callables from a single row/single args entry). Finally,
//!   `func = selected_func(selected_input(**arg))`; if `callable(func)` it's appended as one pipe,
//!   otherwise every item of `func` (assumed iterable) is appended as its own pipe — this is how
//!   `standard_cost_curr`'s 3-pipe return tuple becomes 3 separate entries in one pipeline's
//!   `pdf_extract` segment list, not one bundled callable.
//! - **The return shape is a plain `dict[str, Pipeline]`** (pipeline name → `Pipeline`), *not* the
//!   3-tuple-of-dicts the module's own `get_pipelines` docstring describes — confirmed by reading
//!   the actual `return pipelines` statement at the bottom of the function, which is a single dict
//!   built by iterating the union (`|`) of the 3 segments' own key-sets. [`get_pipelines`] mirrors
//!   this real return shape, not the stale docstring.
//!
//! # Implementation notes (`implementer` phase)
//!
//! `resolve`/`get_pipelines`'s per-row args lookup mirrors `_get_segment` exactly (see the ground
//! truth above): args are loaded once per segment via `serde_yaml` (a blank/empty YAML file parses
//! to `Value::Null`, mirroring `yaml.safe_load` on an empty file returning `None` — never touched
//! by a segment no row actually uses), a small `yaml_value_to_py` helper converts an arbitrary
//! `serde_yaml::Value` into the obvious PyO3 equivalent for the author-dispatch branch and for
//! `standard_cost_curr`'s own opaque dict fields (`body_set`/`subfund_set`/`deselection_list`
//! entries/`algorithm_flags`/`row_algorithm_flags`), and the native `standard_cost_curr` branch is
//! hand-dispatched (not routed through a generic function-pointer table — see [`native`]'s own doc
//! comment for why). `resolve`'s author-module loader reimplements the `spec_from_file_location`/
//! `module_from_spec`/`sys.modules`/`exec_module` idiom directly via `py.import("importlib.util")`
//! calls (Decision 3 — independent of, and never calling, `unstructured/acquisition.py`'s
//! `get_module`). `Pipeline` instances are constructed by calling `py.get_type::<crate::pipeline::
//! Pipeline>()` as a Python callable (its own `#[new]` is private to `pipeline::mod`, by design —
//! not part of that module's public Rust surface) rather than a direct in-crate
//! `crate::pipeline::Pipeline::new(...)` call; this stays within one compiled artifact (the type
//! object is looked up on this same crate's own registered `#[pyclass]`, not via `py.import(...)`
//! of the separately-compiled installed `.so`), so it does not hit the PyO3 cross-compiled-artifact
//! identity trap Step 1.6 documented — no test here needs to round-trip a `Pipeline` through
//! `py.import`.
//!
//! `From<PyErr>` is the same 2-line boilerplate idiom already established verbatim by
//! [`crate::formats_repo::orchestration::OrchestrationError`]'s own `From<PyErr>`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

const CONTENT_DIR: &str = "content";
const ALGORITHMS_DIR: &str = "algorithms";
const SEMISTRUCTURED_DIR: &str = "semistructured";
const ARGS_DIR: &str = "args";
const LOCAL_EXTENSIONS_DIR: &str = "local_extensions";

pub mod formats_mapping;
// `standard_cost_curr`'s native port (sequencing item 2, already landed) plus this step's own
// small native-name registry (sequencing item 3's own addition) — declared out-of-line the same
// way this file itself is (see this module's own doc comment / `lib.rs`'s doc comment on why),
// resolving to `semistructured/native/mod.rs`.
pub mod native;

/// The 3 fixed semistructured segment types — mirrors `pdf_extract`/`text_filter`/`deserialize`,
/// the 3 literal `formats_mapping.csv` columns (`formats_mapping.rs`'s own [`formats_mapping::
/// MappingRow`] fields) and the 3 built-in `algorithms/semistructured/{pdf_extract,text_filter,
/// deserialize}.py` files / `local_extensions/{same}.py` author files / `args/{same}.yaml` files —
/// one Rust value per segment-type-keyed filename family, everywhere in this module and its
/// siblings ([`native`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegmentKind {
    PdfExtract,
    TextFilter,
    Deserialize,
}

/// Where a resolved algorithm name's actual callable/input-class pair comes from — the direct
/// replacement for `formats_mapping_schema`'s per-column `isin(whitelist)` check (Decision 5).
/// `Native` carries no payload: the caller already knows which segment+name resolved natively, and
/// the actual native function to call ([`native::pdf_extract::standard_cost_curr`], today's only
/// entry) is looked up by [`get_pipelines`] itself via [`native::contains`]/[`native::names`], not
/// returned here — there is no single uniform native-function-pointer shape that would fit every
/// (hypothetical future) native algorithm's own distinct argument/return types.
pub enum AlgorithmSource {
    Native,
    Author { func: Py<PyAny>, input_class: Py<PyAny> },
}

/// Mirrors the union of failure modes `_get_segment`/`get_pipelines`'s `getattr`/`KeyError`/
/// dynamic-module-loading logic (plus this task's *new* native/author collision check, Decision 2)
/// can produce, as plain Rust variants rather than an attempt at 1:1 Python-exception-shape
/// fidelity (same confirmed policy as [`crate::formats_repo::metadata::MetadataError`]/
/// [`crate::formats_repo::orchestration::OrchestrationError`]/[`formats_mapping::
/// FormatsMappingError`]). Exact shape fixed by the implementation plan's Decision 6 — not
/// re-litigated here.
#[derive(Debug, Clone, PartialEq)]
pub enum SemistructuredError {
    /// `name` is neither a native algorithm for `segment` nor (when an author module exists for
    /// `segment`) a top-level attribute of it at all.
    UnknownAlgorithm { segment: SegmentKind, name: String },
    /// `name` is defined **both** as a native algorithm for `segment` and as a top-level attribute
    /// of `segment`'s `local_extensions/{segment}.py` author module — a hard configuration error
    /// (Decision 2), independent of whether any `formats_mapping.csv` row for the current format
    /// actually requests `name`; this fires whenever `segment`'s author module gets loaded at all
    /// (i.e. whenever *any* row for the current format touches `segment`), scanning the whole
    /// loaded module against [`native::names`] for `segment`.
    AmbiguousAlgorithm { segment: SegmentKind, name: String },
    /// `name` resolved to a callable top-level attribute of `segment`'s author module, but that
    /// module has no matching `Input{Titlecase(name)}` class (mirrors `_input_from_func`'s naming
    /// rule: split `name` on `_`, capitalize each part's first character, join, prefix `Input`).
    /// `expected_class` is that computed name, for a useful error message.
    AuthorAlgorithmMissingInputClass { segment: SegmentKind, name: String, expected_class: String },
    /// `name` is a top-level attribute of `segment`'s author module, but it isn't callable (e.g. a
    /// plain module-level value, not a function/class).
    AuthorAlgorithmNotCallable { segment: SegmentKind, name: String },
    /// `segment`'s `local_extensions/{segment}.py` file exists on disk but failed to load (a
    /// syntax/import error inside it) — the underlying `PyErr` is printed (`err.print(py)`) at the
    /// point of failure, same convention as [`crate::formats_repo::orchestration::
    /// OrchestrationError::Python`]/the retired `FreeportsConfigError::Python`; `message` is a
    /// short, human-readable recap, `path` is the file that failed to load.
    AuthorModuleLoad { segment: SegmentKind, path: PathBuf, message: String },
    /// [`get_pipelines`]'s per-row args lookup (`args/{segment}.yaml`, keyed by `algorithm_id =
    /// "{format_name}({pipeline_name})"`, falling back to the bare `format_name` key only when
    /// `pipeline_name` is empty) found no matching key at all. `algorithm_id` is always the
    /// original `"{format_name}({pipeline_name})"` form, even when the fallback bare-`format_name`
    /// lookup is the one that actually failed (mirrors this task's own confirmed policy: a clear
    /// message is enough, not fidelity to which exact key `KeyError` names in the Python original).
    MissingArgs { algorithm_id: String },
    /// The args value resolved for `algorithm_id` exists but couldn't be used as this row's actual
    /// argument — e.g. it's a YAML list too short for this row's positional index (see this
    /// module's own doc comment on the list-vs-mapping positional-indexing rule), or its shape
    /// can't be turned into the resolved algorithm's `Input*` instance. `message` is a short,
    /// human-readable explanation.
    MalformedArgs { algorithm_id: String, message: String },
    /// A required on-disk file is missing — reused generically for `formats_mapping.csv` (via
    /// [`formats_mapping::FormatsMappingError::MissingCsv`]) **and** for a missing
    /// `args/{segment}.yaml` (see this module's own doc comment: all 3 are read unconditionally by
    /// every [`get_pipelines`] call, regardless of whether the requested format uses that
    /// segment) — [`SemistructuredError::MissingCsv`] variant is not renamed
    /// `MissingFile`/`MissingYaml` per Decision 6's own fixed enum shape.
    MissingCsv(PathBuf),
    /// Mirrors [`formats_mapping::FormatsMappingError::MalformedRow`] (propagated through
    /// [`formats_mapping::rows_for_format`]).
    MalformedRow { line: usize, reason: String },
    /// Mirrors [`formats_mapping::FormatsMappingError::InvalidId`] (propagated through
    /// [`formats_mapping::rows_for_format`]).
    InvalidId(String),
    /// Any other `PyErr` surfacing anywhere in this module (e.g. constructing an author's
    /// `Input*(**dict)` instance, calling the resolved `func`/`selected_func`, or building a
    /// [`crate::pipeline::Pipeline`]) — printed (`err.print(py)`) at the point of failure via
    /// [`From<PyErr>`] below, same convention as [`crate::formats_repo::orchestration::
    /// OrchestrationError::Python`].
    Python(String),
}

impl std::fmt::Display for SemistructuredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SemistructuredError::UnknownAlgorithm { segment, name } => {
                write!(f, "no native or author-provided algorithm named '{name}' is registered for segment {segment:?}")
            }
            SemistructuredError::AmbiguousAlgorithm { segment, name } => {
                write!(f, "'{name}' is defined both natively and by the author-provided module for segment {segment:?}")
            }
            SemistructuredError::AuthorAlgorithmMissingInputClass { segment, name, expected_class } => {
                write!(
                    f,
                    "author-provided algorithm '{name}' for segment {segment:?} has no matching '{expected_class}' class"
                )
            }
            SemistructuredError::AuthorAlgorithmNotCallable { segment, name } => {
                write!(f, "author-provided attribute '{name}' for segment {segment:?} is not callable")
            }
            SemistructuredError::AuthorModuleLoad { segment, path, message } => {
                write!(f, "failed to load author-provided module for segment {segment:?} at {}: {message}", path.display())
            }
            SemistructuredError::MissingArgs { algorithm_id } => {
                write!(f, "no args entry found for algorithm id '{algorithm_id}'")
            }
            SemistructuredError::MalformedArgs { algorithm_id, message } => {
                write!(f, "malformed args for algorithm id '{algorithm_id}': {message}")
            }
            SemistructuredError::MissingCsv(path) => {
                write!(f, "missing formats-repository file: {}", path.display())
            }
            SemistructuredError::MalformedRow { line, reason } => {
                write!(f, "malformed row at line {line}: {reason}")
            }
            SemistructuredError::InvalidId(id) => {
                write!(f, "ID '{id}' does not match the expected ID pattern")
            }
            // The full traceback is already printed (`err.print(py)`) right where this was
            // generated — see `From<PyErr>`'s doc comment above — so this is a short, deliberately
            // redundant recap, matching `OrchestrationError::Python`'s own convention.
            SemistructuredError::Python(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SemistructuredError {}

/// Mirrors [`formats_mapping::FormatsMappingError`]'s variants 1:1 — [`formats_mapping::
/// rows_for_format`]'s own error surface is a strict subset of this module's own enum (see each
/// matched variant's own doc comment above).
impl From<formats_mapping::FormatsMappingError> for SemistructuredError {
    fn from(e: formats_mapping::FormatsMappingError) -> Self {
        match e {
            formats_mapping::FormatsMappingError::MissingCsv(path) => SemistructuredError::MissingCsv(path),
            formats_mapping::FormatsMappingError::MalformedRow { line, reason } => {
                SemistructuredError::MalformedRow { line, reason }
            }
            formats_mapping::FormatsMappingError::InvalidId(id) => SemistructuredError::InvalidId(id),
        }
    }
}

/// Same 2-line idiom already established verbatim by
/// [`crate::formats_repo::orchestration::OrchestrationError`]'s own `From<PyErr>` (itself mirroring
/// the retired `FreeportsConfigError::Python`) — not new business logic.
impl From<PyErr> for SemistructuredError {
    fn from(e: PyErr) -> Self {
        Python::attach(|py| e.print(py));
        SemistructuredError::Python(e.to_string())
    }
}

/// Resolves `name` (one `formats_mapping.csv` cell's requested algorithm, for `segment`) to either
/// the native Rust registry ([`native::contains`]/[`native::names`], only `"standard_cost_curr"`
/// under [`SegmentKind::PdfExtract`] exists today) or `segment`'s author-provided
/// `local_extensions/{segment}.py` module (loaded fresh, lazily, on every call — no cross-call
/// cache, per Decision 4), with a hard [`SemistructuredError::AmbiguousAlgorithm`] whenever the
/// author module (if it exists at all for `segment`) defines *any* name also present in the native
/// registry for `segment`, independent of whether `name` itself is the colliding one (Decision 2 —
/// see this module's own doc comment and [`SemistructuredError::AmbiguousAlgorithm`]'s own doc
/// comment). See this module's own doc comment for the full native-vs-author/`Input*`-class-lookup
/// semantics.
pub fn resolve(
    py: Python<'_>,
    formats_repo_dir: &Path,
    segment: SegmentKind,
    name: &str,
) -> Result<AlgorithmSource, SemistructuredError> {
    let ext_path = local_extension_path(formats_repo_dir, segment);
    if !ext_path.is_file() {
        return resolve_native_or_unknown(segment, name);
    }

    let module = load_author_module(py, segment, &ext_path)?;

    // Decision 2/4: the collision check runs whenever the author module is loaded at all, for
    // *every* native name registered for this segment - independent of whether `name` itself is
    // the one that collides.
    for native_name in native::names(segment) {
        if module_attr(&module, native_name)?.is_some() {
            return Err(SemistructuredError::AmbiguousAlgorithm { segment, name: (*native_name).to_string() });
        }
    }

    match module_attr(&module, name)? {
        Some(attr) => {
            if !attr.is_callable() {
                return Err(SemistructuredError::AuthorAlgorithmNotCallable { segment, name: name.to_string() });
            }
            let expected_class = input_class_name(name);
            let input_class = module_attr(&module, &expected_class)?.ok_or_else(|| {
                SemistructuredError::AuthorAlgorithmMissingInputClass {
                    segment,
                    name: name.to_string(),
                    expected_class: expected_class.clone(),
                }
            })?;
            Ok(AlgorithmSource::Author { func: attr.unbind(), input_class: input_class.unbind() })
        }
        None => resolve_native_or_unknown(segment, name),
    }
}

fn resolve_native_or_unknown(segment: SegmentKind, name: &str) -> Result<AlgorithmSource, SemistructuredError> {
    if native::contains(segment, name) {
        Ok(AlgorithmSource::Native)
    } else {
        Err(SemistructuredError::UnknownAlgorithm { segment, name: name.to_string() })
    }
}

/// The literal lowercase file stem shared by `args/{segment}.yaml`/`local_extensions/{segment}.py`.
fn segment_file_name(segment: SegmentKind) -> &'static str {
    match segment {
        SegmentKind::PdfExtract => "pdf_extract",
        SegmentKind::TextFilter => "text_filter",
        SegmentKind::Deserialize => "deserialize",
    }
}

fn local_extension_path(formats_repo_dir: &Path, segment: SegmentKind) -> PathBuf {
    formats_repo_dir
        .join(CONTENT_DIR)
        .join(ALGORITHMS_DIR)
        .join(SEMISTRUCTURED_DIR)
        .join(LOCAL_EXTENSIONS_DIR)
        .join(format!("{}.py", segment_file_name(segment)))
}

fn args_path(formats_repo_dir: &Path, segment: SegmentKind) -> PathBuf {
    formats_repo_dir
        .join(CONTENT_DIR)
        .join(ALGORITHMS_DIR)
        .join(SEMISTRUCTURED_DIR)
        .join(ARGS_DIR)
        .join(format!("{}.yaml", segment_file_name(segment)))
}

/// Loads `path` as a fresh Python module, reusing the `spec_from_file_location`/
/// `module_from_spec`/`sys.modules[...]`/`exec_module` idiom `unstructured/acquisition.py`'s
/// `get_module` already uses for a different purpose (Decision 3 - an independent
/// reimplementation, `get_module` itself is never called). Registered under a synthetic
/// `sys.modules` name derived from `segment` alone (this module's own filenames are fixed,
/// segment-keyed, unlike `get_module`'s per-format name) - re-executed fresh on every call, no
/// caching, per Decision 4.
fn load_author_module<'py>(py: Python<'py>, segment: SegmentKind, path: &Path) -> Result<Bound<'py, PyAny>, SemistructuredError> {
    let runtime_name = format!("_freeports_native_semistructured_local_extension_{}", segment_file_name(segment));
    let importlib_util = py.import("importlib.util")?;
    let path_str = path.to_string_lossy().into_owned();
    let spec = importlib_util.call_method1("spec_from_file_location", (runtime_name.as_str(), path_str))?;
    let module = importlib_util.call_method1("module_from_spec", (&spec,))?;
    py.import("sys")?.getattr("modules")?.set_item(runtime_name.as_str(), &module)?;
    let loader = spec.getattr("loader")?;
    loader.call_method1("exec_module", (&module,)).map_err(|e| {
        e.print(py);
        SemistructuredError::AuthorModuleLoad { segment, path: path.to_path_buf(), message: e.to_string() }
    })?;
    Ok(module)
}

/// `module.name`, or `None` if `module` has no such top-level attribute at all.
fn module_attr<'py>(module: &Bound<'py, PyAny>, name: &str) -> Result<Option<Bound<'py, PyAny>>, SemistructuredError> {
    if module.hasattr(name)? {
        Ok(Some(module.getattr(name)?))
    } else {
        Ok(None)
    }
}

/// Mirrors `_input_from_func`'s naming rule (`"Input" + name.title().replace("_", "")`): split
/// `name` on `_`, capitalize each part's first character, join, prefix `Input`.
fn input_class_name(name: &str) -> String {
    let mut result = String::from("Input");
    for part in name.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            result.extend(first.to_uppercase());
            result.push_str(chars.as_str());
        }
    }
    result
}

/// Rust port of `acquisition.py`'s `get_pipelines`, end to end — see this module's own doc comment
/// for the full ground-truth algorithm (args-YAML lookup/fallback, list-vs-mapping positional
/// indexing, native-vs-author dispatch via [`resolve`], `callable(func) else iterate` pipe
/// flattening, final pipeline-name-keyed assembly via [`crate::pipeline::Pipeline`]). A
/// `format_name` with zero `formats_mapping.csv` rows (mirrors [`formats_mapping::rows_for_format`]'s
/// own no-error contract) returns an empty dict, not an error.
pub fn get_pipelines(
    py: Python<'_>,
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<Py<PyDict>, SemistructuredError> {
    let rows = formats_mapping::rows_for_format(formats_repo_dir, format_name)?;

    let pdf_extract_segment = build_segment(py, formats_repo_dir, format_name, SegmentKind::PdfExtract, &rows)?;
    let text_filter_segment = build_segment(py, formats_repo_dir, format_name, SegmentKind::TextFilter, &rows)?;
    let deserialize_segment = build_segment(py, formats_repo_dir, format_name, SegmentKind::Deserialize, &rows)?;

    let mut pipeline_names: HashSet<&str> = HashSet::new();
    pipeline_names.extend(pdf_extract_segment.keys().map(String::as_str));
    pipeline_names.extend(text_filter_segment.keys().map(String::as_str));
    pipeline_names.extend(deserialize_segment.keys().map(String::as_str));

    let pipeline_type = py.get_type::<crate::pipeline::Pipeline>();
    let result = PyDict::new(py);
    for pipeline_name in pipeline_names {
        let pdf_extract_arg = segment_value(py, pdf_extract_segment.get(pipeline_name))?;
        let text_filter_arg = segment_value(py, text_filter_segment.get(pipeline_name))?;
        let deserialize_arg = segment_value(py, deserialize_segment.get(pipeline_name))?;
        let pipeline = pipeline_type.call1((pdf_extract_arg, text_filter_arg, deserialize_arg))?;
        result.set_item(pipeline_name, pipeline)?;
    }
    Ok(result.unbind())
}

/// PyO3-facing entry point for [`get_pipelines`], exported as
/// `freeports._native.core.get_semistructured_pipelines`.
#[pyfunction]
#[pyo3(name = "get_semistructured_pipelines")]
pub fn py_get_semistructured_pipelines(py: Python<'_>, formats_repo_dir: PathBuf, format_name: String) -> PyResult<Py<PyDict>> {
    get_pipelines(py, &formats_repo_dir, &format_name).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// One segment's worth of `_get_segment`'s output: pipeline name -> the ordered list of pipe
/// callables assembled for it so far.
type SegmentPipes = HashMap<String, Vec<Py<PyAny>>>;

/// Rust port of `_get_segment`, for one [`SegmentKind`] - see this module's own doc comment for the
/// full per-row algorithm (args-YAML unconditional load, list-vs-mapping positional indexing by
/// already-appended-callable count, native-vs-author dispatch via [`resolve`], `callable(func) else
/// iterate` flattening).
fn build_segment(
    py: Python<'_>,
    formats_repo_dir: &Path,
    format_name: &str,
    segment: SegmentKind,
    rows: &[formats_mapping::MappingRow],
) -> Result<SegmentPipes, SemistructuredError> {
    let args = load_args_yaml(formats_repo_dir, segment)?;
    let mut pipes: SegmentPipes = HashMap::new();

    for row in rows {
        let Some(name) = segment_cell(row, segment) else { continue };
        let pipeline = &row.pipeline_name;
        let algorithm_id = format!("{format_name}({pipeline})");
        let selected_args = lookup_args(&args, format_name, pipeline, &algorithm_id)?;

        let entry = pipes.entry(pipeline.clone()).or_default();
        let arg = positional_arg(selected_args, entry.len(), &algorithm_id)?;

        let new_pipes = match resolve(py, formats_repo_dir, segment, name)? {
            AlgorithmSource::Native => dispatch_native(py, segment, name, arg, &algorithm_id)?,
            AlgorithmSource::Author { func, input_class } => dispatch_author(py, &func, &input_class, arg, &algorithm_id)?,
        };
        entry.extend(new_pipes);
    }
    Ok(pipes)
}

fn segment_cell(row: &formats_mapping::MappingRow, segment: SegmentKind) -> Option<&str> {
    match segment {
        SegmentKind::PdfExtract => row.pdf_extract.as_deref(),
        SegmentKind::TextFilter => row.text_filter.as_deref(),
        SegmentKind::Deserialize => row.deserialize.as_deref(),
    }
}

/// `pipes`, as the `Option<&Bound<'_, PyAny>>`-shaped argument [`crate::pipeline::Pipeline`]'s
/// constructor expects: a Python list of pipe callables, or Python `None` when this segment has no
/// entry at all for this pipeline (mirrors `segment.get(pn)`, which is `None` for a pipeline this
/// segment never touched).
fn segment_value<'py>(py: Python<'py>, pipes: Option<&Vec<Py<PyAny>>>) -> PyResult<Bound<'py, PyAny>> {
    match pipes {
        Some(pipes) => {
            let bound: Vec<Bound<'py, PyAny>> = pipes.iter().map(|p| p.bind(py).clone()).collect();
            Ok(PyList::new(py, bound)?.into_any())
        }
        None => Ok(py.None().into_bound(py)),
    }
}

/// All 3 `args/{segment}.yaml` files are read unconditionally by every [`get_pipelines`] call (see
/// this module's own doc comment) - a missing file is a hard [`SemistructuredError::MissingCsv`]
/// even for a segment the requested format never uses. An empty file mirrors `yaml.safe_load`
/// returning `None` for empty input (`serde_yaml::Value::Null`) - not an error by itself, only a
/// problem if some row later tries to look an algorithm id up in it.
fn load_args_yaml(formats_repo_dir: &Path, segment: SegmentKind) -> Result<serde_yaml::Value, SemistructuredError> {
    let path = args_path(formats_repo_dir, segment);
    if !path.exists() {
        return Err(SemistructuredError::MissingCsv(path));
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| SemistructuredError::MalformedRow { line: 0, reason: format!("failed to read {}: {e}", path.display()) })?;
    serde_yaml::from_str(&content)
        .map_err(|e| SemistructuredError::MalformedRow { line: 0, reason: format!("failed to parse {}: {e}", path.display()) })
}

/// `args[algorithm_id]`, falling back to `args[format_name]` only when `pipeline` is empty (mirrors
/// `_get_segment`'s own `try/except KeyError` fallback exactly - see this module's own doc
/// comment). `algorithm_id` is always the original `"{format_name}({pipeline})"` form, even when
/// it's the fallback lookup that actually fails.
fn lookup_args<'a>(
    args: &'a serde_yaml::Value,
    format_name: &str,
    pipeline: &str,
    algorithm_id: &str,
) -> Result<&'a serde_yaml::Value, SemistructuredError> {
    if let Some(v) = args.get(algorithm_id) {
        return Ok(v);
    }
    if pipeline.is_empty()
        && let Some(v) = args.get(format_name)
    {
        return Ok(v);
    }
    Err(SemistructuredError::MissingArgs { algorithm_id: algorithm_id.to_string() })
}

/// The actual argument value for this particular row: `selected[already_appended]` when `selected`
/// is a YAML list (the stacked-algorithm positional-indexing rule - see this module's own doc
/// comment on why `already_appended` is a running per-appended-callable counter, not the CSV row's
/// own `Pipe index`), or `selected` itself unchanged otherwise.
fn positional_arg<'a>(
    selected: &'a serde_yaml::Value,
    already_appended: usize,
    algorithm_id: &str,
) -> Result<&'a serde_yaml::Value, SemistructuredError> {
    match selected.as_sequence() {
        Some(seq) => seq.get(already_appended).ok_or_else(|| SemistructuredError::MalformedArgs {
            algorithm_id: algorithm_id.to_string(),
            message: format!("stacked args list has only {} entrie(s), needed index {already_appended}", seq.len()),
        }),
        None => Ok(selected),
    }
}

/// Dispatches to the one native algorithm that exists today. [`native::contains`] is the only
/// producer of [`AlgorithmSource::Native`] (via [`resolve`]), and it registers exactly this
/// `(segment, name)` pair - see [`native`]'s own doc comment for why this is a hand-written match
/// rather than a generic function-pointer table.
fn dispatch_native(
    py: Python<'_>,
    segment: SegmentKind,
    name: &str,
    arg: &serde_yaml::Value,
    algorithm_id: &str,
) -> Result<Vec<Py<PyAny>>, SemistructuredError> {
    match (segment, name) {
        (SegmentKind::PdfExtract, "standard_cost_curr") => {
            let input = input_standard_cost_curr_from_yaml(py, arg, algorithm_id)?;
            let (investments, fund, currency) = native::pdf_extract::standard_cost_curr(py, input)?;
            // Mirrors the Python original's own `callable(func) else iterate` branch: the 3-tuple
            // `standard_cost_curr` returns is not itself callable, so all 3 items are appended
            // individually, exactly like iterating a plain Python tuple would.
            Ok(vec![Py::new(py, investments)?.into_any(), fund, Py::new(py, currency)?.into_any()])
        }
        _ => unreachable!("native::contains/resolve only ever resolve (PdfExtract, \"standard_cost_curr\") to AlgorithmSource::Native today"),
    }
}

fn yaml_get<'a>(value: &'a serde_yaml::Value, key: &str) -> Option<&'a serde_yaml::Value> {
    value.get(key)
}

/// Builds a [`native::pdf_extract::InputStandardCostCurr`] from the YAML value resolved for one
/// row - see that module's own doc comment for the exact field shapes this mirrors.
fn input_standard_cost_curr_from_yaml(
    py: Python<'_>,
    value: &serde_yaml::Value,
    algorithm_id: &str,
) -> Result<native::pdf_extract::InputStandardCostCurr, SemistructuredError> {
    let malformed = |message: String| SemistructuredError::MalformedArgs { algorithm_id: algorithm_id.to_string(), message };

    let deselection_list = match yaml_get(value, "deselection_list") {
        None | Some(serde_yaml::Value::Null) => Vec::new(),
        Some(v) => match v.as_sequence() {
            Some(items) => items.iter().map(|item| yaml_value_to_py(py, item)).collect::<PyResult<Vec<_>>>()?,
            None => return Err(malformed("'deselection_list' must be a list".to_string())),
        },
    };

    let body_set = yaml_get(value, "body_set")
        .ok_or_else(|| malformed("missing required 'body_set' key".to_string()))
        .and_then(|v| yaml_value_to_py(py, v).map_err(SemistructuredError::from))?;
    let subfund_set = yaml_get(value, "subfund_set")
        .ok_or_else(|| malformed("missing required 'subfund_set' key".to_string()))
        .and_then(|v| yaml_value_to_py(py, v).map_err(SemistructuredError::from))?;

    let currency_str = yaml_get(value, "currency")
        .and_then(serde_yaml::Value::as_str)
        .ok_or_else(|| malformed("missing or non-string required 'currency' key".to_string()))?;
    let currency = crate::formats_utils::deserialize::cast::to_currency(currency_str).map_err(malformed)?;

    let algorithm_flags = match yaml_get(value, "algorithm_flags") {
        None | Some(serde_yaml::Value::Null) => None,
        Some(v) => Some(yaml_value_to_py(py, v)?),
    };
    let row_algorithm_flags = match yaml_get(value, "row_algorithm_flags") {
        None | Some(serde_yaml::Value::Null) => None,
        Some(v) => Some(yaml_value_to_py(py, v)?),
    };

    let tolerance = yaml_get(value, "tolerance").and_then(serde_yaml::Value::as_f64).unwrap_or(0.0);
    let row_tolerance = yaml_get(value, "row_tolerance").and_then(serde_yaml::Value::as_f64).unwrap_or(0.0);

    Ok(native::pdf_extract::InputStandardCostCurr {
        deselection_list,
        body_set,
        subfund_set,
        currency,
        algorithm_flags,
        tolerance,
        row_algorithm_flags,
        row_tolerance,
    })
}

/// Dispatches to an author-provided algorithm: builds `input_class(**arg)`, calls `func(instance)`,
/// then flattens the result the same way [`dispatch_native`]'s own callers do.
fn dispatch_author(
    py: Python<'_>,
    func: &Py<PyAny>,
    input_class: &Py<PyAny>,
    arg: &serde_yaml::Value,
    algorithm_id: &str,
) -> Result<Vec<Py<PyAny>>, SemistructuredError> {
    let py_arg = yaml_value_to_py(py, arg)?;
    let kwargs = py_arg.bind(py).cast::<PyDict>().map_err(|_| SemistructuredError::MalformedArgs {
        algorithm_id: algorithm_id.to_string(),
        message: "author algorithm args must be a mapping".to_string(),
    })?;
    let instance = input_class.bind(py).call((), Some(kwargs))?;
    let pipe = func.bind(py).call1((instance,))?;
    flatten_pipe(pipe)
}

/// Mirrors `_get_segment`'s own `if callable(func): segment[pipeline].append(func) else: for f in
/// func: segment[pipeline].append(f)`.
fn flatten_pipe(pipe: Bound<'_, PyAny>) -> Result<Vec<Py<PyAny>>, SemistructuredError> {
    if pipe.is_callable() {
        Ok(vec![pipe.unbind()])
    } else {
        let mut result = Vec::new();
        for item in pipe.try_iter()? {
            result.push(item?.unbind());
        }
        Ok(result)
    }
}

/// Converts an arbitrary `serde_yaml::Value` into the obvious PyO3 equivalent (`Null` -> `None`,
/// `Bool`/`String` -> the matching Python primitive, `Number` -> `int` when it fits losslessly,
/// `float` otherwise, `Sequence`/`Mapping` -> `list`/`dict`, recursively). Used both for
/// author-dispatch's `input_class(**arg)` kwargs and for `standard_cost_curr`'s own opaque dict
/// fields.
fn yaml_value_to_py(py: Python<'_>, value: &serde_yaml::Value) -> PyResult<Py<PyAny>> {
    match value {
        serde_yaml::Value::Null => Ok(py.None()),
        serde_yaml::Value::Bool(b) => Ok(b.into_pyobject(py)?.to_owned().into_any().unbind()),
        serde_yaml::Value::Number(n) => match n.as_i64() {
            Some(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
            None => Ok(n.as_f64().unwrap_or(0.0).into_pyobject(py)?.into_any().unbind()),
        },
        serde_yaml::Value::String(s) => Ok(s.into_pyobject(py)?.into_any().unbind()),
        serde_yaml::Value::Sequence(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(yaml_value_to_py(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        serde_yaml::Value::Mapping(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(yaml_value_to_py(py, k)?, yaml_value_to_py(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
        serde_yaml::Value::Tagged(tagged) => yaml_value_to_py(py, &tagged.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    // ============================================================
    // Fixture helpers
    // ============================================================

    /// Writes `<dir>/content/algorithms/semistructured/formats_mapping.csv` with the given raw CSV
    /// text. Same shape as `formats_mapping.rs`'s own `write_formats_mapping_csv` (this module
    /// can't import that private test helper across files, so it gets its own copy — same
    /// discipline as every other file in this crate's test modules).
    fn write_formats_mapping_csv(dir: &Path, csv_text: &str) {
        let semistructured_dir = dir.join("content").join("algorithms").join("semistructured");
        std::fs::create_dir_all(&semistructured_dir).unwrap();
        std::fs::write(semistructured_dir.join("formats_mapping.csv"), csv_text).unwrap();
    }

    /// Writes `<dir>/content/algorithms/semistructured/args/{segment_name}.yaml` (`segment_name`
    /// is the literal lowercase file stem: `"pdf_extract"`/`"text_filter"`/`"deserialize"`).
    fn write_args_yaml(dir: &Path, segment_name: &str, yaml_text: &str) {
        let args_dir = dir.join("content").join("algorithms").join("semistructured").join("args");
        std::fs::create_dir_all(&args_dir).unwrap();
        std::fs::write(args_dir.join(format!("{segment_name}.yaml")), yaml_text).unwrap();
    }

    /// Writes `<dir>/content/algorithms/semistructured/args/{pdf_extract,text_filter,
    /// deserialize}.yaml` as 3 empty files — the minimum needed for [`get_pipelines`] to run to
    /// completion for a format that uses none of the 3 segments, mirroring this module's own doc
    /// comment on why all 3 are read unconditionally by every call.
    fn write_empty_args_yaml(dir: &Path) {
        write_args_yaml(dir, "pdf_extract", "");
        write_args_yaml(dir, "text_filter", "");
        write_args_yaml(dir, "deserialize", "");
    }

    /// Writes `<dir>/content/algorithms/semistructured/local_extensions/{segment_name}.py`
    /// (Decision 1's exact new subfolder/naming), creating the subfolder as needed.
    fn write_local_extension(dir: &Path, segment_name: &str, python_source: &str) {
        let ext_dir = dir.join("content").join("algorithms").join("semistructured").join("local_extensions");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join(format!("{segment_name}.py")), python_source).unwrap();
    }

    /// Evaluates `src` as a Python literal and returns it unbound - same `py.eval` idiom
    /// `native/pdf_extract.rs`'s own tests already use for building sample Python values inline.
    fn py_eval(py: Python<'_>, src: &str) -> Py<PyAny> {
        let code = CString::new(src).unwrap();
        py.eval(&code, None, None).unwrap().unbind()
    }

    /// One PDF line's raw dict shape (see `native/pdf_extract.rs`'s own `line_src`/`page_with_lines`
    /// helpers, duplicated here in trimmed form - test modules in this crate don't share private
    /// helpers across files).
    fn line_src(font: &str, size: f64, text: &str, bbox: (f64, f64, f64, f64)) -> String {
        format!(
            "{{'dir': (1.0, 0.0), 'bbox': {bbox:?}, 'spans': [\
             {{'font': '{font}', 'size': {size:?}, 'text': '{text}', 'bbox': {bbox:?}}}]}}"
        )
    }

    fn page_with_lines<'py>(py: Python<'py>, lines_src: &str) -> Bound<'py, PyAny> {
        let src = format!("{{'width': 300.0, 'height': 300.0, 'blocks': [{{'type': 0, 'lines': [{lines_src}]}}]}}");
        py.eval(&CString::new(src).unwrap(), None, None).unwrap()
    }

    /// Fails the test unless `resolve(...)` returns `Err(...)`, returning the error for
    /// variant-specific assertions - `AlgorithmSource` has no `PartialEq` (its `Author` variant
    /// holds opaque `Py<PyAny>` fields with no meaningful structural equality), so `assert_eq!`
    /// against a whole `Result<AlgorithmSource, SemistructuredError>` can't be used here.
    fn expect_resolve_err(py: Python<'_>, dir: &Path, segment: SegmentKind, name: &str) -> SemistructuredError {
        match resolve(py, dir, segment, name) {
            Ok(_) => panic!("expected resolve({segment:?}, {name:?}) to fail, got Ok"),
            Err(e) => e,
        }
    }

    /// Fails the test unless `get_pipelines(...)` returns `Err(...)`, returning the error for
    /// variant-specific assertions (`Py<PyDict>` has no useful `PartialEq` either).
    fn expect_pipelines_err(py: Python<'_>, dir: &Path, format_name: &str) -> SemistructuredError {
        match get_pipelines(py, dir, format_name) {
            Ok(_) => panic!("expected get_pipelines({format_name:?}) to fail, got Ok"),
            Err(e) => e,
        }
    }

    // ============================================================
    // resolve() - case 1: native name found, no local_extensions file on disk at all.
    // ============================================================

    #[test]
    fn resolve_finds_a_native_name_with_no_local_extensions_directory_on_disk_at_all() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            // Not even `content/algorithms/semistructured/` exists yet - resolve() must not
            // require the directory tree to pre-exist just to check for an (absent) author file.
            let result = resolve(py, dir.path(), SegmentKind::PdfExtract, "standard_cost_curr");
            assert!(matches!(result, Ok(AlgorithmSource::Native)));
        });
    }

    #[test]
    fn resolve_finds_a_native_name_even_when_the_semistructured_directory_exists_for_other_reasons() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_formats_mapping_csv(dir.path(), "ID,pdf_extract,text_filter,deserialize\n");
            let result = resolve(py, dir.path(), SegmentKind::PdfExtract, "standard_cost_curr");
            assert!(matches!(result, Ok(AlgorithmSource::Native)));
        });
    }

    // ============================================================
    // resolve() - case 2: author name found, function actually resolvable + its Input class.
    // ============================================================

    #[test]
    fn resolve_finds_an_author_provided_name_with_its_matching_input_class() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_local_extension(
                dir.path(),
                "text_filter",
                "class InputMyCustomFilter:\n    def __init__(self, factor):\n        self.factor = factor\n\n\ndef my_custom_filter(arg):\n    def _pipe(blocks, filter_data):\n        return list(blocks)[: arg.factor]\n\n    return _pipe\n",
            );
            match resolve(py, dir.path(), SegmentKind::TextFilter, "my_custom_filter").unwrap() {
                AlgorithmSource::Author { func, input_class } => {
                    assert!(func.bind(py).is_callable());
                    let class_name: String = input_class.bind(py).getattr("__name__").unwrap().extract().unwrap();
                    assert_eq!(class_name, "InputMyCustomFilter");
                }
                AlgorithmSource::Native => panic!("expected Author, got Native"),
            }
        });
    }

    // ============================================================
    // resolve() - case 3: name found in neither native nor (if present) author file.
    // ============================================================

    #[test]
    fn resolve_errors_with_unknown_algorithm_when_no_local_extensions_file_exists_at_all() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            let err = expect_resolve_err(py, dir.path(), SegmentKind::PdfExtract, "totally_unknown");
            assert_eq!(
                err,
                SemistructuredError::UnknownAlgorithm { segment: SegmentKind::PdfExtract, name: "totally_unknown".to_string() }
            );
        });
    }

    #[test]
    fn resolve_errors_with_unknown_algorithm_when_the_author_file_exists_but_lacks_the_name() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_local_extension(
                dir.path(),
                "pdf_extract",
                "def some_other_thing(arg):\n    return lambda page: []\n\n\nclass InputSomeOtherThing:\n    def __init__(self):\n        pass\n",
            );
            let err = expect_resolve_err(py, dir.path(), SegmentKind::PdfExtract, "totally_unknown");
            assert_eq!(
                err,
                SemistructuredError::UnknownAlgorithm { segment: SegmentKind::PdfExtract, name: "totally_unknown".to_string() }
            );
        });
    }

    // ============================================================
    // resolve() - case 4: same name in both native and author file -> AmbiguousAlgorithm, even
    // when resolving a *different*, otherwise-valid name, and scoped per-segment.
    // ============================================================

    #[test]
    fn resolve_errors_with_ambiguous_algorithm_when_the_author_file_redefines_a_native_name() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_local_extension(
                dir.path(),
                "pdf_extract",
                "def standard_cost_curr(arg):\n    return lambda page: []\n",
            );
            let err = expect_resolve_err(py, dir.path(), SegmentKind::PdfExtract, "standard_cost_curr");
            assert_eq!(
                err,
                SemistructuredError::AmbiguousAlgorithm {
                    segment: SegmentKind::PdfExtract,
                    name: "standard_cost_curr".to_string()
                }
            );
        });
    }

    #[test]
    fn resolve_errors_with_ambiguous_algorithm_even_when_resolving_a_different_unrelated_name() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_local_extension(
                dir.path(),
                "pdf_extract",
                "def standard_cost_curr(arg):\n    return lambda page: []\n\n\nclass InputMyOtherAlgo:\n    def __init__(self, value):\n        self.value = value\n\n\ndef my_other_algo(arg):\n    return lambda page: []\n",
            );
            // "my_other_algo" itself has no collision at all - but resolving it still fails,
            // because loading pdf_extract's author module at all (which resolving *any* name for
            // this segment requires) discovers the unrelated "standard_cost_curr" collision
            // (Decision 2/4 - the check is not scoped to the specific name being resolved).
            let err = expect_resolve_err(py, dir.path(), SegmentKind::PdfExtract, "my_other_algo");
            assert_eq!(
                err,
                SemistructuredError::AmbiguousAlgorithm {
                    segment: SegmentKind::PdfExtract,
                    name: "standard_cost_curr".to_string()
                }
            );
        });
    }

    #[test]
    fn resolve_ambiguity_in_one_segment_does_not_affect_a_different_clean_segment() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_local_extension(
                dir.path(),
                "pdf_extract",
                "def standard_cost_curr(arg):\n    return lambda page: []\n",
            );
            write_local_extension(
                dir.path(),
                "text_filter",
                "class InputCleanFilter:\n    def __init__(self, x):\n        self.x = x\n\n\ndef clean_filter(arg):\n    return lambda blocks, filter_data: blocks\n",
            );
            // pdf_extract is ambiguous...
            assert!(matches!(
                resolve(py, dir.path(), SegmentKind::PdfExtract, "standard_cost_curr"),
                Err(SemistructuredError::AmbiguousAlgorithm { .. })
            ));
            // ...but text_filter, a different segment, resolves cleanly.
            assert!(matches!(
                resolve(py, dir.path(), SegmentKind::TextFilter, "clean_filter"),
                Ok(AlgorithmSource::Author { .. })
            ));
        });
    }

    // ============================================================
    // resolve() - case 5: malformed author-provided Python file -> AuthorModuleLoad.
    // ============================================================

    #[test]
    fn resolve_errors_with_author_module_load_on_a_syntax_error_in_the_local_extensions_file() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_local_extension(dir.path(), "deserialize", "def broken(\n");
            let err = expect_resolve_err(py, dir.path(), SegmentKind::Deserialize, "whatever");
            match err {
                SemistructuredError::AuthorModuleLoad { segment, path, message } => {
                    assert_eq!(segment, SegmentKind::Deserialize);
                    assert!(path.ends_with("deserialize.py"), "unexpected path: {}", path.display());
                    assert!(!message.is_empty());
                }
                other => panic!("expected AuthorModuleLoad, got {other:?}"),
            }
        });
    }

    // ============================================================
    // resolve() - case 6: author function present but missing its Input* class.
    // ============================================================

    #[test]
    fn resolve_errors_with_missing_input_class_when_the_author_function_has_no_matching_input_class() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_local_extension(dir.path(), "pdf_extract", "def some_function(arg):\n    return lambda page: []\n");
            let err = expect_resolve_err(py, dir.path(), SegmentKind::PdfExtract, "some_function");
            assert_eq!(
                err,
                SemistructuredError::AuthorAlgorithmMissingInputClass {
                    segment: SegmentKind::PdfExtract,
                    name: "some_function".to_string(),
                    expected_class: "InputSomeFunction".to_string(),
                }
            );
        });
    }

    // ============================================================
    // resolve() - case 7: author-defined name exists but isn't callable.
    // ============================================================

    #[test]
    fn resolve_errors_with_not_callable_when_the_author_defined_name_is_a_plain_value() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            // InputSomeValue is defined too, so this can't be mistaken for the "missing Input
            // class" case above - the only thing wrong here is callability.
            write_local_extension(
                dir.path(),
                "pdf_extract",
                "class InputSomeValue:\n    def __init__(self, x):\n        self.x = x\n\n\nsome_value = 42\n",
            );
            let err = expect_resolve_err(py, dir.path(), SegmentKind::PdfExtract, "some_value");
            assert_eq!(
                err,
                SemistructuredError::AuthorAlgorithmNotCallable { segment: SegmentKind::PdfExtract, name: "some_value".to_string() }
            );
        });
    }

    // ============================================================
    // get_pipelines() - case: native dispatch end to end (AMUNDI-IT24-shaped row/args).
    // ============================================================

    #[test]
    fn get_pipelines_resolves_a_native_pdf_extract_algorithm_end_to_end() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let dir = tempfile::tempdir().unwrap();
            write_formats_mapping_csv(
                dir.path(),
                "ID,pdf_extract,text_filter,deserialize\nAMUNDI-IT24(investments),standard_cost_curr,,\n",
            );
            write_args_yaml(
                dir.path(),
                "pdf_extract",
                "AMUNDI-IT24(investments):\n  deselection_list:\n    - font: TrebuchetMS\n      text: \"^ \"\n  body_set:\n    font: TrebuchetMS\n  subfund_set:\n    font: Arial-BoldItalicMT\n    area:\n      y_max: 60\n  currency: EUR\n  tolerance: 1.0\n  row_tolerance: 0.5\n",
            );
            write_args_yaml(dir.path(), "text_filter", "");
            write_args_yaml(dir.path(), "deserialize", "");
            // No local_extensions directory at all - purely native dispatch.

            let pipelines = get_pipelines(py, dir.path(), "AMUNDI-IT24").unwrap();
            let pipelines = pipelines.bind(py);
            assert_eq!(pipelines.len(), 1);
            let pipeline = pipelines.get_item("investments").unwrap().unwrap();

            let page = page_with_lines(
                py,
                &format!(
                    "{},{}",
                    two_row_table_page_lines("TrebuchetMS"),
                    line_src("Arial-BoldItalicMT", 10.0, "Subfund X", (0.0, 10.0, 40.0, 20.0))
                ),
            );
            let blocks = pipeline.getattr("pdf_extract").unwrap().call_method1("__call__", (page,)).unwrap();
            let blocks: Vec<Bound<'_, PyAny>> = blocks.try_iter().unwrap().collect::<PyResult<_>>().unwrap();
            // 4 TABLE_BODY (two_row_table_page) + 1 FUND_NAME (subfund line) + 1
            // CURRENCY_STATEMENT (always emitted) = 6.
            assert_eq!(blocks.len(), 6);
            let type_blocks: std::collections::HashSet<String> =
                blocks.iter().map(|b| b.getattr("type_block").unwrap().extract().unwrap()).collect();
            assert_eq!(
                type_blocks,
                std::collections::HashSet::from([
                    "TABLE_BODY".to_string(),
                    "FUND_NAME".to_string(),
                    "CURRENCY_STATEMENT".to_string()
                ])
            );
        });
    }

    /// Builds a classic 2-row/2-col table's 4 line sources (same shape as `native/pdf_extract.rs`'s
    /// own `two_row_table_page`), as a fragment to embed inside a larger page alongside an extra
    /// (e.g. subfund) line - `page_with_lines` above only builds a whole page from scratch, it
    /// doesn't compose with other lines.
    fn two_row_table_page_lines(font: &str) -> String {
        [
            line_src(font, 10.0, "Row1Col1", (0.0, 0.0, 20.0, 10.0)),
            line_src(font, 10.0, "Row1Col2", (30.0, 0.0, 50.0, 10.0)),
            line_src(font, 10.0, "Row2Col1", (0.0, 20.0, 20.0, 30.0)),
            line_src(font, 10.0, "Row2Col2", (30.0, 20.0, 50.0, 30.0)),
        ]
        .join(",")
    }

    // ============================================================
    // get_pipelines() - case: author dispatch end to end - the function actually gets called
    // with an instance built from the YAML args.
    // ============================================================

    #[test]
    fn get_pipelines_resolves_an_author_provided_text_filter_algorithm_and_calls_it_with_the_yaml_args() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_formats_mapping_csv(dir.path(), "ID,pdf_extract,text_filter,deserialize\nCARNE-EN23(renaming),,my_custom_filter,\n");
            write_args_yaml(dir.path(), "text_filter", "CARNE-EN23(renaming):\n  factor: 2\n");
            write_args_yaml(dir.path(), "pdf_extract", "");
            write_args_yaml(dir.path(), "deserialize", "");
            write_local_extension(
                dir.path(),
                "text_filter",
                "class InputMyCustomFilter:\n    def __init__(self, factor):\n        self.factor = factor\n\n\ndef my_custom_filter(arg):\n    def _pipe(blocks, filter_data):\n        return list(blocks)[: arg.factor]\n\n    return _pipe\n",
            );

            let pipelines = get_pipelines(py, dir.path(), "CARNE-EN23").unwrap();
            let pipelines = pipelines.bind(py);
            let pipeline = pipelines.get_item("renaming").unwrap().unwrap();
            let blocks = py_eval(py, "[1, 2, 3]");
            let result = pipeline
                .getattr("text_filter")
                .unwrap()
                .call_method1("__call__", (blocks.bind(py), py.None()))
                .unwrap();
            let result: Vec<i64> = result.extract().unwrap();
            // InputMyCustomFilter(factor=2) truncates [1, 2, 3] down to its first 2 elements -
            // proves the YAML-derived `factor` value actually reached the constructed instance.
            assert_eq!(result, vec![1, 2]);
        });
    }

    // ============================================================
    // get_pipelines() - case: stacked-algorithm positional-list-indexing (2 rows, same
    // pipeline+segment, args value is a 2-element YAML list, each row consumes its own entry).
    // ============================================================

    #[test]
    fn get_pipelines_gives_each_stacked_row_its_own_positional_args_list_entry() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_formats_mapping_csv(
                dir.path(),
                "ID,pdf_extract,text_filter,deserialize\nCARNE-EN23(combo),,stack_filter,\nCARNE-EN23(combo),,stack_filter,\n",
            );
            write_args_yaml(dir.path(), "text_filter", "CARNE-EN23(combo):\n  - factor: 2\n  - factor: 5\n");
            write_args_yaml(dir.path(), "pdf_extract", "");
            write_args_yaml(dir.path(), "deserialize", "");
            write_local_extension(
                dir.path(),
                "text_filter",
                "class InputStackFilter:\n    def __init__(self, factor):\n        self.factor = factor\n\n\ndef stack_filter(arg):\n    def _pipe(blocks, filter_data):\n        return [arg.factor]\n\n    return _pipe\n",
            );

            let pipelines = get_pipelines(py, dir.path(), "CARNE-EN23").unwrap();
            let pipelines = pipelines.bind(py);
            let pipeline = pipelines.get_item("combo").unwrap().unwrap();
            let empty_list = py_eval(py, "[]");
            let result = pipeline
                .getattr("text_filter")
                .unwrap()
                .call_method1("__call__", (empty_list.bind(py), py.None()))
                .unwrap();
            let result: Vec<i64> = result.extract().unwrap();
            // Row 0 -> args list[0] (factor=2), row 1 -> args list[1] (factor=5), in row order.
            assert_eq!(result, vec![2, 5]);
        });
    }

    // ============================================================
    // get_pipelines() - case: MissingArgs, both for a non-empty pipeline (no fallback attempted
    // even though a bare-format-name key exists) and for an empty pipeline (fallback attempted
    // but the bare-format-name key is also absent).
    // ============================================================

    #[test]
    fn get_pipelines_errors_with_missing_args_for_a_non_empty_pipeline_with_no_matching_key() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_formats_mapping_csv(
                dir.path(),
                "ID,pdf_extract,text_filter,deserialize\nCARNE-EN23(investments),standard_cost_curr,,\n",
            );
            // Only the *bare* format name key exists - must not be used as a fallback, since the
            // pipeline name here ("investments") is non-empty.
            write_args_yaml(dir.path(), "pdf_extract", "CARNE-EN23:\n  currency: EUR\n");
            write_args_yaml(dir.path(), "text_filter", "");
            write_args_yaml(dir.path(), "deserialize", "");

            let err = expect_pipelines_err(py, dir.path(), "CARNE-EN23");
            assert_eq!(err, SemistructuredError::MissingArgs { algorithm_id: "CARNE-EN23(investments)".to_string() });
        });
    }

    #[test]
    fn get_pipelines_errors_with_missing_args_for_an_empty_pipeline_when_the_fallback_key_is_also_absent() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            // A bare ID (no "(pipeline)" group) - pipeline name defaults to "", so
            // algorithm_id is the literal "CARNE-EN23()".
            write_formats_mapping_csv(dir.path(), "ID,pdf_extract,text_filter,deserialize\nCARNE-EN23,standard_cost_curr,,\n");
            write_args_yaml(dir.path(), "pdf_extract", "OTHER-EN24(investments):\n  currency: EUR\n");
            write_args_yaml(dir.path(), "text_filter", "");
            write_args_yaml(dir.path(), "deserialize", "");

            let err = expect_pipelines_err(py, dir.path(), "CARNE-EN23");
            assert_eq!(err, SemistructuredError::MissingArgs { algorithm_id: "CARNE-EN23()".to_string() });
        });
    }

    // ============================================================
    // get_pipelines() - case: MalformedArgs when a stacked args list runs out of entries.
    // ============================================================

    #[test]
    fn get_pipelines_errors_with_malformed_args_when_the_stacked_args_list_is_too_short() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_formats_mapping_csv(
                dir.path(),
                "ID,pdf_extract,text_filter,deserialize\nCARNE-EN23(combo2),,stack_filter,\nCARNE-EN23(combo2),,stack_filter,\nCARNE-EN23(combo2),,stack_filter,\n",
            );
            // Only 2 entries for 3 rows sharing the same pipeline+segment.
            write_args_yaml(dir.path(), "text_filter", "CARNE-EN23(combo2):\n  - factor: 2\n  - factor: 5\n");
            write_args_yaml(dir.path(), "pdf_extract", "");
            write_args_yaml(dir.path(), "deserialize", "");
            write_local_extension(
                dir.path(),
                "text_filter",
                "class InputStackFilter:\n    def __init__(self, factor):\n        self.factor = factor\n\n\ndef stack_filter(arg):\n    def _pipe(blocks, filter_data):\n        return [arg.factor]\n\n    return _pipe\n",
            );

            let err = expect_pipelines_err(py, dir.path(), "CARNE-EN23");
            match err {
                SemistructuredError::MalformedArgs { algorithm_id, message } => {
                    assert_eq!(algorithm_id, "CARNE-EN23(combo2)");
                    assert!(!message.is_empty());
                }
                other => panic!("expected MalformedArgs, got {other:?}"),
            }
        });
    }

    // ============================================================
    // get_pipelines() - case: a format with zero formats_mapping.csv rows -> empty dict, not an
    // error (mirrors formats_mapping::rows_for_format's own no-error contract).
    // ============================================================

    #[test]
    fn get_pipelines_returns_an_empty_dict_for_a_format_with_no_rows_at_all() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            write_formats_mapping_csv(
                dir.path(),
                "ID,pdf_extract,text_filter,deserialize\nOTHER-EN24(investments),standard_cost_curr,,\n",
            );
            write_empty_args_yaml(dir.path());

            let pipelines = get_pipelines(py, dir.path(), "GHOST-EN24").unwrap();
            assert_eq!(pipelines.bind(py).len(), 0);
        });
    }

    // ============================================================
    // get_pipelines() - real Python-confirmed behavior: all 3 args YAML files are read
    // unconditionally, even for a segment the requested format never uses at all.
    // ============================================================

    #[test]
    fn get_pipelines_errors_when_an_unused_segments_args_yaml_is_missing_entirely() {
        Python::attach(|py| {
            let dir = tempfile::tempdir().unwrap();
            // This format only ever populates pdf_extract - text_filter/deserialize.yaml are
            // still required to exist on disk, per this module's own doc comment (confirmed by
            // reading acquisition.py's _get_segment directly: the yaml.safe_load call is not
            // inside, or guarded by, the per-row loop).
            write_formats_mapping_csv(
                dir.path(),
                "ID,pdf_extract,text_filter,deserialize\nAMUNDI-IT24(investments),standard_cost_curr,,\n",
            );
            write_args_yaml(
                dir.path(),
                "pdf_extract",
                "AMUNDI-IT24(investments):\n  body_set:\n    font: TrebuchetMS\n  subfund_set:\n    font: Arial\n  currency: EUR\n",
            );
            // text_filter.yaml / deserialize.yaml deliberately not written at all.
            let result = get_pipelines(py, dir.path(), "AMUNDI-IT24");
            assert!(result.is_err(), "expected an error when an unused segment's args yaml is missing entirely");
        });
    }

    // ============================================================
    // SemistructuredError Display - loose, content-only checks (no pandera-shape/KeyError-text
    // fidelity required, same policy as every sibling error enum's own Display tests).
    // ============================================================

    #[test]
    fn semistructured_error_unknown_algorithm_display_mentions_the_segment_and_name() {
        let message = SemistructuredError::UnknownAlgorithm { segment: SegmentKind::PdfExtract, name: "foo".to_string() }.to_string();
        assert!(message.contains("foo"));
    }

    #[test]
    fn semistructured_error_ambiguous_algorithm_display_mentions_the_name() {
        let message =
            SemistructuredError::AmbiguousAlgorithm { segment: SegmentKind::TextFilter, name: "bar".to_string() }.to_string();
        assert!(message.contains("bar"));
    }

    #[test]
    fn semistructured_error_missing_input_class_display_mentions_the_expected_class() {
        let message = SemistructuredError::AuthorAlgorithmMissingInputClass {
            segment: SegmentKind::Deserialize,
            name: "baz".to_string(),
            expected_class: "InputBaz".to_string(),
        }
        .to_string();
        assert!(message.contains("InputBaz"));
    }

    #[test]
    fn semistructured_error_not_callable_display_mentions_the_name() {
        let message =
            SemistructuredError::AuthorAlgorithmNotCallable { segment: SegmentKind::PdfExtract, name: "qux".to_string() }.to_string();
        assert!(message.contains("qux"));
    }

    #[test]
    fn semistructured_error_author_module_load_display_mentions_the_message() {
        let message = SemistructuredError::AuthorModuleLoad {
            segment: SegmentKind::PdfExtract,
            path: PathBuf::from("/some/repo/local_extensions/pdf_extract.py"),
            message: "boom".to_string(),
        }
        .to_string();
        assert!(message.contains("boom"));
    }

    #[test]
    fn semistructured_error_missing_args_display_mentions_the_algorithm_id() {
        let message = SemistructuredError::MissingArgs { algorithm_id: "FMT(pipe)".to_string() }.to_string();
        assert!(message.contains("FMT(pipe)"));
    }

    #[test]
    fn semistructured_error_malformed_args_display_mentions_the_algorithm_id_and_message() {
        let message =
            SemistructuredError::MalformedArgs { algorithm_id: "FMT(pipe)".to_string(), message: "too short".to_string() }
                .to_string();
        assert!(message.contains("FMT(pipe)"));
        assert!(message.contains("too short"));
    }

    #[test]
    fn semistructured_error_missing_csv_display_mentions_the_path() {
        let path = PathBuf::from("/some/repo/content/algorithms/semistructured/args/pdf_extract.yaml");
        let message = SemistructuredError::MissingCsv(path.clone()).to_string();
        assert!(message.contains(&path.display().to_string()));
    }

    #[test]
    fn semistructured_error_malformed_row_display_mentions_the_line_and_reason() {
        let message = SemistructuredError::MalformedRow { line: 4, reason: "bad row".to_string() }.to_string();
        assert!(message.contains('4'));
        assert!(message.contains("bad row"));
    }

    #[test]
    fn semistructured_error_invalid_id_display_mentions_the_id() {
        let message = SemistructuredError::InvalidId("GHOST-EN24(pipe)/0".to_string()).to_string();
        assert!(message.contains("GHOST-EN24(pipe)/0"));
    }

    #[test]
    fn semistructured_error_python_display_mentions_the_underlying_message() {
        let message = SemistructuredError::Python("boom".to_string()).to_string();
        assert!(message.contains("boom"));
    }
}
