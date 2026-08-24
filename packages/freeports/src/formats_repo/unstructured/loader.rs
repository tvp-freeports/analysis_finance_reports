//! Il caricamento dinamico del modulo Python di un formato unstructured.
//!
//! "Unstructured" significa che l'algoritmo di un segmento è **nel repo formati**, scritto
//! dall'autore: un solo formato, un solo file Python. Questo modulo lo trova, lo importa e ne
//! legge le due cose che il caricamento si aspetta — `pipelines` e, facoltativamente,
//! `compute_page_class`.
//!
//! Porting di `unstructured/acquisition.py`, con la stessa risoluzione file-o-package e lo stesso
//! `templates/` aggiunto a `sys.path`.
//!
//! **Il contratto è per forma, non per tipo** (deciso come D-M7-3 il 2026-08-23, quando l'API
//! Python non esisteva ancora; confermato in M10, quando è arrivata). Il valore `pipelines` deve
//! essere una mappa da nome a un oggetto con gli attributi `pdf_extract`/`text_filter`/
//! `deserialize`, ciascuno un callable o un iterabile di callable. `freeports.core.Pipeline`
//! ([`crate::python::core::PyPipeline`]) soddisfa esattamente quel protocollo, e infatti il
//! caricamento di un repo formati reale non ha richiesto una seconda passata qui.
//!
//! **Pipe nativi e pipe d'autore.** Un segmento può contenere entrambi: i moduli d'autore
//! mescolano liberamente `PdfExtractFundStandard(...)` e le proprie `lambda`. I primi arrivano
//! qui come involucri di [`crate::python::pipes`], e vengono **spacchettati** con
//! `unwrap_pdf_extract` e sorelle: senza quel passaggio un pipe nativo farebbe un giro
//! Rust -> Python -> Rust a ogni blocco, e la conversione duck-typed di [`super::py_pipe`]
//! perderebbe per strada le varianti tipizzate di `BlockValue`. I secondi restano avvolti
//! nell'adattatore, che è la ragione per cui esiste.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::core::pipeline::{Pipeline, PipelineName};

use crate::python::pipes::{unwrap_deserialize, unwrap_pdf_extract, unwrap_text_filter};

use super::py_pipe::{PyDeserializePipe, PyPdfExtractPipe, PyTextFilterPipe};

/// La cartella dei moduli d'autore dentro il repo formati.
pub const UNSTRUCTURED_DIR: &str = "content/algorithms/unstructured";
/// La cartella dei moduli condivisi fra più formati, aggiunta a `sys.path` prima dell'import.
pub const TEMPLATES_DIR: &str = "content/templates";

/// I tre segmenti, nell'ordine in cui una pipeline li applica.
const SEGMENTS: [&str; 3] = ["pdf_extract", "text_filter", "deserialize"];

/// Fallimenti nel caricamento del livello unstructured.
#[derive(Debug, thiserror::Error)]
pub enum UnstructuredError {
    #[error("failed to load the author module for format '{format}' at {path}: {message}")]
    ModuleLoad { format: String, path: PathBuf, message: String },
    #[error("format '{format}': 'pipelines' must be a mapping from pipeline name to pipeline")]
    PipelinesNotAMapping { format: String },
    #[error("format '{format}', pipeline '{pipeline}': the value is not a pipeline (it has no '{segment}')")]
    NotAPipeline { format: String, pipeline: String, segment: &'static str },
    #[error("format '{format}', pipeline '{pipeline}', segment '{segment}': {message}")]
    InvalidSegment { format: String, pipeline: String, segment: &'static str, message: String },
    #[error("format '{format}': 'compute_page_class' is not callable")]
    ComputePageClassNotCallable { format: String },
    #[error("format '{format}': {message}")]
    Python { format: String, message: String },
}

/// Il nome di modulo che corrisponde a un nome di formato: minuscolo, con `-`, `.` e `@`
/// sostituiti da underscore. Verbatim dal riferimento.
pub fn module_name(format_name: &str) -> String {
    format_name.to_lowercase().replace(['-', '.', '@'], "_")
}

/// Il file da importare per `format_name`: `<nome>.py`, oppure `<nome>/__init__.py` se il formato
/// è un package. `None` se il formato non ha un modulo d'autore — che è del tutto legittimo.
pub fn module_path(formats_repo_dir: &Path, format_name: &str) -> Option<(PathBuf, bool)> {
    let base = formats_repo_dir.join(UNSTRUCTURED_DIR);
    let name = module_name(format_name);
    let file = base.join(format!("{name}.py"));
    if file.is_file() {
        return Some((file, false));
    }
    let package = base.join(&name).join("__init__.py");
    if package.is_file() {
        return Some((package, true));
    }
    None
}

/// Importa il modulo d'autore di `format_name`, se esiste.
///
/// `templates/` è aggiunto a `sys.path` prima dell'import, come nel riferimento: è così che più
/// formati condividono codice.
fn import_module<'py>(
    py: Python<'py>,
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<Option<Bound<'py, PyAny>>, UnstructuredError> {
    let Some((path, is_package)) = module_path(formats_repo_dir, format_name) else {
        return Ok(None);
    };

    let load = || -> PyResult<Bound<'py, PyAny>> {
        // Prima di ogni altra cosa: il modulo d'autore fara' `from freeports.core import PdfBlock`,
        // e quel `freeports` deve essere **questo** artefatto compilato, non un altro. Vedi
        // `crate::python::install`, che spiega perche' e che non fa nulla quando siamo gia' dentro
        // il `.so`.
        crate::python::install(py)?;

        let sys = py.import("sys")?;
        let sys_path = sys.getattr("path")?.cast_into::<PyList>().map_err(PyErr::from)?;
        let templates = formats_repo_dir.join(TEMPLATES_DIR);
        let templates = templates.to_string_lossy().to_string();
        if !sys_path.contains(&templates)? {
            sys_path.insert(0, &templates)?;
        }

        let util = py.import("importlib.util")?;
        let runtime_name = format!("_plugin_{}", module_name(format_name));
        let kwargs = PyDict::new(py);
        if is_package {
            let parent = path.parent().unwrap_or(&path).to_string_lossy().to_string();
            kwargs.set_item("submodule_search_locations", vec![parent])?;
        }
        let spec = util.call_method(
            "spec_from_file_location",
            (&runtime_name, path.to_string_lossy().as_ref()),
            Some(&kwargs),
        )?;
        let module = util.call_method1("module_from_spec", (&spec,))?;
        py.import("sys")?.getattr("modules")?.set_item(&runtime_name, &module)?;
        spec.getattr("loader")?.call_method1("exec_module", (&module,))?;
        Ok(module)
    };

    match load() {
        Ok(module) => Ok(Some(module)),
        Err(error) => {
            let message = error.to_string();
            tracing::error!(format = format_name, "failed to load author module: {message}");
            error.print(py);
            Err(UnstructuredError::ModuleLoad { format: format_name.to_string(), path, message })
        }
    }
}

/// Legge un attributo o una chiave, come fa il confine dei pipe: il modulo d'autore può esporre
/// `pipelines` come oggetto o come dizionario.
fn field<'py>(object: &Bound<'py, PyAny>, name: &str) -> Option<Bound<'py, PyAny>> {
    if let Ok(value) = object.getattr(name) {
        return Some(value);
    }
    object.get_item(name).ok()
}

/// I callable di un segmento: uno solo, oppure un iterabile.
fn segment_callables<'py>(
    value: Bound<'py, PyAny>,
    format: &str,
    pipeline: &str,
    segment: &'static str,
) -> Result<Vec<Bound<'py, PyAny>>, UnstructuredError> {
    let invalid = |message: String| UnstructuredError::InvalidSegment {
        format: format.to_string(),
        pipeline: pipeline.to_string(),
        segment,
        message,
    };
    if value.is_none() {
        return Ok(Vec::new());
    }
    if value.is_callable() {
        return Ok(vec![value]);
    }
    let iterator = value.try_iter().map_err(|e| invalid(e.to_string()))?;
    let mut out = Vec::new();
    for item in iterator {
        let item = item.map_err(|e| invalid(e.to_string()))?;
        if !item.is_callable() {
            return Err(invalid("every pipe of a segment must be callable".to_string()));
        }
        out.push(item);
    }
    Ok(out)
}

/// Le pipeline che il modulo d'autore di `format_name` definisce.
///
/// Un formato senza modulo d'autore, o con un modulo che non espone `pipelines`, semplicemente non
/// ne definisce nessuna: non è un errore.
pub fn get_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<HashMap<PipelineName, Pipeline>, UnstructuredError> {
    Python::attach(|py| {
        let Some(module) = import_module(py, formats_repo_dir, format_name)? else {
            return Ok(HashMap::new());
        };
        let Some(pipelines) = field(&module, "pipelines") else {
            return Ok(HashMap::new());
        };
        let mapping = pipelines
            .cast::<PyDict>()
            .map_err(|_| UnstructuredError::PipelinesNotAMapping { format: format_name.to_string() })?;

        let mut out = HashMap::new();
        for (key, value) in mapping.iter() {
            let pipeline_name: String = key.extract().map_err(|e: PyErr| UnstructuredError::Python {
                format: format_name.to_string(),
                message: e.to_string(),
            })?;
            let mut pipeline = Pipeline::new(PipelineName::new(&pipeline_name));

            for segment in SEGMENTS {
                let raw = field(&value, segment).ok_or_else(|| UnstructuredError::NotAPipeline {
                    format: format_name.to_string(),
                    pipeline: pipeline_name.clone(),
                    segment,
                })?;
                for (i, func) in segment_callables(raw, format_name, &pipeline_name, segment)?
                    .into_iter()
                    .enumerate()
                {
                    let name = format!("{pipeline_name}::{segment}[{i}]");
                    match segment {
                        "pdf_extract" => {
                            let pipe = match unwrap_pdf_extract(&func) {
                                Some(native) => native,
                                None => Arc::new(PyPdfExtractPipe::new(&pipeline_name, name, func.unbind())),
                            };
                            pipeline.pdf_extract.push(pipe);
                        }
                        "text_filter" => {
                            let pipe = match unwrap_text_filter(&func) {
                                Some(native) => native,
                                None => Arc::new(PyTextFilterPipe::new(&pipeline_name, name, func.unbind())),
                            };
                            pipeline.text_filter.push(pipe);
                        }
                        _ => {
                            let pipe = match unwrap_deserialize(&func) {
                                Some(native) => native,
                                None => Arc::new(PyDeserializePipe::new(&pipeline_name, name, func.unbind())),
                            };
                            pipeline.deserialize.push(pipe);
                        }
                    }
                }
            }
            out.insert(PipelineName::new(pipeline_name), pipeline);
        }
        Ok(out)
    })
}

/// Il finalizzatore di page class dell'autore, se ne definisce uno.
///
/// `standard_compute_page_class` del riferimento è l'identità, che qui non ha bisogno di esistere:
/// l'assenza è rappresentata da `None`, e il motore ha già
/// [`PageClassFinalizer::Identity`](crate::core::algorithm::PageClassFinalizer).
pub fn get_compute_page_class(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<Option<Py<PyAny>>, UnstructuredError> {
    Python::attach(|py| {
        let Some(module) = import_module(py, formats_repo_dir, format_name)? else {
            return Ok(None);
        };
        let Some(value) = field(&module, "compute_page_class") else {
            return Ok(None);
        };
        if !value.is_callable() {
            return Err(UnstructuredError::ComputePageClassNotCallable { format: format_name.to_string() });
        }
        Ok(Some(value.unbind()))
    })
}

/// Il `compute_page_class` dell'autore, adattato al trait del motore.
///
/// Le page class attraversano il confine come lista di stringhe (o `None` per "non classificata"),
/// che è ciò che il riferimento passa già oggi.
pub struct PyPageClassFinalizer {
    format: String,
    func: Py<PyAny>,
}

impl PyPageClassFinalizer {
    pub fn new(format: impl Into<String>, func: Py<PyAny>) -> Self {
        Self { format: format.into(), func }
    }
}

impl crate::core::algorithm::PageClassFinalize for PyPageClassFinalizer {
    fn finalize(
        &self,
        classes: Vec<Option<crate::core::schedule::PageClass>>,
    ) -> Result<Vec<Option<crate::core::schedule::PageClass>>, crate::core::pipeline::PipeError> {
        use crate::core::pipeline::PipeError;
        use crate::core::schedule::PageClass;

        Python::attach(|py| {
            let call = || -> PyResult<Vec<Option<String>>> {
                let input = PyList::empty(py);
                for class in &classes {
                    match class {
                        Some(class) => input.append(class.as_str())?,
                        None => input.append(py.None())?,
                    }
                }
                self.func.bind(py).call1((input,))?.extract()
            };
            match call() {
                Ok(result) => Ok(result.into_iter().map(|name| name.map(PageClass::new)).collect()),
                Err(error) => {
                    let message = error.to_string();
                    tracing::error!(format = self.format, "compute_page_class raised: {message}");
                    error.print(py);
                    Err(PipeError::author(&self.format, "compute_page_class", message))
                }
            }
        })
    }
}

/// Il finalizzatore di page class del formato, pronto per il motore.
pub fn get_page_class_finalizer(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<crate::core::algorithm::PageClassFinalizer, UnstructuredError> {
    use crate::core::algorithm::PageClassFinalizer;
    Ok(match get_compute_page_class(formats_repo_dir, format_name)? {
        Some(func) => PageClassFinalizer::Custom(Arc::new(PyPageClassFinalizer::new(format_name, func))),
        None => PageClassFinalizer::Identity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    mod module_naming {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("AMUNDI-EN24", "amundi_en24"; "plain")]
        #[test_case("ANIMA_SGR-IT24", "anima_sgr_it24"; "with an underscore already")]
        #[test_case("MEDIOLANUM-IT24@ES", "mediolanum_it24_es"; "with a country suffix")]
        #[test_case("MEDIOLANUM-ES24.B", "mediolanum_es24_b"; "with a version suffix")]
        #[test_case("MEDIOLANUM-IT24@ES.b", "mediolanum_it24_es_b"; "with both")]
        fn maps_a_format_name_onto_a_module_name(format: &str, expected: &str) {
            assert_eq!(module_name(format), expected);
        }
    }

    mod module_resolution {
        use super::*;

        fn repo() -> TempDir {
            let dir = TempDir::new().expect("temp dir");
            fs::create_dir_all(dir.path().join(UNSTRUCTURED_DIR)).expect("unstructured dir");
            dir
        }

        #[test]
        fn a_format_without_a_module_resolves_to_nothing() {
            let dir = repo();
            assert!(module_path(dir.path(), "A-EN24").is_none());
        }

        #[test]
        fn a_single_file_module_is_found() {
            let dir = repo();
            fs::write(dir.path().join(UNSTRUCTURED_DIR).join("a_en24.py"), "").unwrap();
            let (path, is_package) = module_path(dir.path(), "A-EN24").unwrap();
            assert!(path.ends_with("a_en24.py"));
            assert!(!is_package);
        }

        #[test]
        fn a_package_module_is_found_through_its_init() {
            let dir = repo();
            fs::create_dir_all(dir.path().join(UNSTRUCTURED_DIR).join("a_en24")).unwrap();
            fs::write(dir.path().join(UNSTRUCTURED_DIR).join("a_en24").join("__init__.py"), "").unwrap();
            let (path, is_package) = module_path(dir.path(), "A-EN24").unwrap();
            assert!(path.ends_with("__init__.py"));
            assert!(is_package);
        }

        #[test]
        fn a_single_file_wins_over_a_package_of_the_same_name() {
            let dir = repo();
            fs::write(dir.path().join(UNSTRUCTURED_DIR).join("a_en24.py"), "").unwrap();
            fs::create_dir_all(dir.path().join(UNSTRUCTURED_DIR).join("a_en24")).unwrap();
            fs::write(dir.path().join(UNSTRUCTURED_DIR).join("a_en24").join("__init__.py"), "").unwrap();
            let (path, is_package) = module_path(dir.path(), "A-EN24").unwrap();
            assert!(path.ends_with("a_en24.py"));
            assert!(!is_package);
        }
    }

    /// I soli test di questo file che attaccano davvero l'interprete Python (`PLAN.md` §10/§13
    /// D13). I moduli sintetici che scrivono **non importano `freeports`**: è la ragione per cui
    /// funzionano in questa fase, ed è la stessa scelta che rende il livello unstructured testabile
    /// prima che i binding esistano (D-M7-3).
    mod python_boundary {
        use super::*;
        use crate::core::classes::{BlockType, BlockValue, PdfBlock, TextBlock};
        use crate::core::page::Page;
        use crate::core::pipeline::FilterData;
        use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
        use pretty_assertions::assert_eq;
        use std::collections::BTreeMap;

        /// Un repo formati con un solo modulo d'autore, scritto dal test.
        fn repo_with_module(format: &str, source: &str) -> TempDir {
            let dir = TempDir::new().expect("temp dir");
            fs::create_dir_all(dir.path().join(UNSTRUCTURED_DIR)).expect("unstructured dir");
            fs::create_dir_all(dir.path().join(TEMPLATES_DIR)).expect("templates dir");
            fs::write(dir.path().join(UNSTRUCTURED_DIR).join(format!("{}.py", module_name(format))), source)
                .expect("write module");
            dir
        }

        /// Il modulo d'autore minimo che definisce una pipeline completa.
        const COMPLETE_MODULE: &str = r#"
class _Pipeline:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def extract(page):
    return [{"type_block": "RELEVANT_BLOCK", "metadata": {"n": len(page["blocks"])}, "content": "hello"}]

def filter_blocks(blocks, companies):
    # `blocks[0].content` e non `blocks[0]["content"]`: dopo M10 un pipe d'autore riceve i
    # `PdfBlock` veri, come i moduli d'autore di un repo formati reale gia' li scrivevano.
    return [{"type_block": "FUND", "metadata": {"seen": len(blocks)}, "content": blocks[0].content}]

def deserialize_block(block):
    return [{"fund-id": block.content}]

pipelines = {"authored": _Pipeline(extract, filter_blocks, deserialize_block)}
"#;

        fn page_with_raw(py: Python<'_>) -> Page {
            let raw = py
                .eval(
                    pyo3::ffi::c_str!("{'width': 10.0, 'height': 10.0, 'blocks': [{'type': 0, 'lines': []}]}"),
                    None,
                    None,
                )
                .expect("page dict");
            Page::new(1, (10.0, 10.0), vec![PdfLine::new("Arial", 10.0, "x", (0.0, 0.0, 5.0, 5.0))], Vec::new())
                .with_raw(raw.unbind())
        }

        #[test]
        fn a_format_without_a_module_defines_no_pipeline() {
            let dir = TempDir::new().unwrap();
            assert!(get_pipelines(dir.path(), "A-EN24").unwrap().is_empty());
        }

        #[test]
        fn a_module_without_pipelines_defines_no_pipeline() {
            let dir = repo_with_module("A-EN24", "x = 1\n");
            assert!(get_pipelines(dir.path(), "A-EN24").unwrap().is_empty());
        }

        #[test]
        fn a_complete_module_defines_one_complete_pipeline() {
            let dir = repo_with_module("A-EN24", COMPLETE_MODULE);
            let pipelines = get_pipelines(dir.path(), "A-EN24").unwrap();
            assert_eq!(pipelines.len(), 1);
            let pipeline = &pipelines[&PipelineName::new("authored")];
            assert!(pipeline.is_complete());
            assert_eq!(pipeline.pdf_extract.len(), 1);
        }

        #[test]
        fn a_segment_may_hold_several_callables() {
            let source = format!("{COMPLETE_MODULE}\npipelines['authored'].pdf_extract = [extract, extract]\n");
            let dir = repo_with_module("A-EN24", &source);
            let pipelines = get_pipelines(dir.path(), "A-EN24").unwrap();
            assert_eq!(pipelines[&PipelineName::new("authored")].pdf_extract.len(), 2);
        }

        #[test]
        fn an_empty_segment_leaves_the_pipeline_incomplete_instead_of_failing() {
            let source = format!("{COMPLETE_MODULE}\npipelines['authored'].deserialize = None\n");
            let dir = repo_with_module("A-EN24", &source);
            let pipelines = get_pipelines(dir.path(), "A-EN24").unwrap();
            assert!(!pipelines[&PipelineName::new("authored")].is_complete());
        }

        #[test]
        fn a_value_that_is_not_a_pipeline_is_rejected_naming_the_missing_segment() {
            let dir = repo_with_module("A-EN24", "pipelines = {'authored': object()}\n");
            let err = get_pipelines(dir.path(), "A-EN24").unwrap_err();
            assert!(matches!(err, UnstructuredError::NotAPipeline { segment: "pdf_extract", .. }), "{err}");
        }

        #[test]
        fn a_non_callable_pipe_inside_a_segment_is_rejected() {
            let source = format!("{COMPLETE_MODULE}\npipelines['authored'].pdf_extract = [1, 2]\n");
            let dir = repo_with_module("A-EN24", &source);
            let err = get_pipelines(dir.path(), "A-EN24").unwrap_err();
            assert!(matches!(err, UnstructuredError::InvalidSegment { segment: "pdf_extract", .. }), "{err}");
        }

        #[test]
        fn a_pipelines_value_that_is_not_a_mapping_is_rejected() {
            let dir = repo_with_module("A-EN24", "pipelines = [1, 2]\n");
            assert!(matches!(
                get_pipelines(dir.path(), "A-EN24"),
                Err(UnstructuredError::PipelinesNotAMapping { .. })
            ));
        }

        #[test]
        fn a_module_that_fails_to_import_reports_its_path() {
            let dir = repo_with_module("A-EN24", "raise RuntimeError('boom')\n");
            let err = get_pipelines(dir.path(), "A-EN24").unwrap_err();
            assert!(matches!(err, UnstructuredError::ModuleLoad { .. }), "{err}");
            assert!(err.to_string().contains("boom"), "{err}");
        }

        #[test]
        fn a_package_module_is_importable_too() {
            let dir = TempDir::new().unwrap();
            let package = dir.path().join(UNSTRUCTURED_DIR).join("a_en24");
            fs::create_dir_all(&package).unwrap();
            fs::create_dir_all(dir.path().join(TEMPLATES_DIR)).unwrap();
            fs::write(package.join("__init__.py"), COMPLETE_MODULE).unwrap();
            assert_eq!(get_pipelines(dir.path(), "A-EN24").unwrap().len(), 1);
        }

        #[test]
        fn an_authored_pdf_extract_pipe_returns_real_pdf_blocks() {
            let dir = repo_with_module("A-EN24", COMPLETE_MODULE);
            let pipelines = get_pipelines(dir.path(), "A-EN24").unwrap();
            let pipeline = &pipelines[&PipelineName::new("authored")];
            Python::attach(|py| {
                let page = page_with_raw(py);
                let blocks = pipeline.pdf_extract.iter().next().unwrap().extract(&page).unwrap();
                assert_eq!(blocks.len(), 1);
                assert_eq!(blocks[0].type_block, BlockType::RELEVANT_BLOCK);
                assert_eq!(blocks[0].content, BlockValue::from("hello"));
                assert_eq!(blocks[0].metadata.get("n"), Some(&BlockValue::from(1i64)));
            });
        }

        /// Regressione: il binario `freeports` linka il crate come `rlib` e ha quindi le **sue**
        /// classi Python, diverse da quelle del `.so` installato. Se il modulo d'autore importa
        /// `freeports` prima che il caricamento semini `sys.modules`, i due lati non si
        /// riconoscono piu' e il primo pipe che riceve un blocco muore con
        /// `'PdfBlock' object cannot be cast as 'PdfBlock'`. Vedi `crate::python::install`.
        #[test]
        fn loading_a_format_seeds_the_in_process_freeports_module() {
            let source = "
from freeports.core import PdfBlock

class _P:
    def __init__(self):
        self.pdf_extract = None
        self.text_filter = None
        self.deserialize = None

pipelines = {'authored': _P()}
";
            let dir = repo_with_module("A-EN24", source);
            get_pipelines(dir.path(), "A-EN24").unwrap();
            Python::attach(|py| {
                let module = py.import("sys").unwrap().getattr("modules").unwrap();
                let seeded = module.get_item("freeports").unwrap();
                let seeded_type = seeded.getattr("core").unwrap().getattr("PdfBlock").unwrap();
                // L'identita' che conta: la classe raggiungibile da Python e quella che questo
                // artefatto registra devono essere **lo stesso oggetto**.
                let ours = py.get_type::<crate::python::core::PyPdfBlock>();
                assert!(seeded_type.is(&ours), "the seeded PdfBlock is not this artifact's");
            });
        }

        #[test]
        fn an_authored_pdf_extract_pipe_fails_cleanly_on_a_page_without_a_pymupdf_dictionary() {
            let dir = repo_with_module("A-EN24", COMPLETE_MODULE);
            let pipelines = get_pipelines(dir.path(), "A-EN24").unwrap();
            let pipeline = &pipelines[&PipelineName::new("authored")];
            let page = Page::new(1, (10.0, 10.0), Vec::new(), Vec::new());
            let err = pipeline.pdf_extract.iter().next().unwrap().extract(&page).unwrap_err();
            assert!(!err.is_page_failure());
        }

        #[test]
        fn an_authored_text_filter_pipe_receives_the_blocks_and_returns_text_blocks() {
            let dir = repo_with_module("A-EN24", COMPLETE_MODULE);
            let pipelines = get_pipelines(dir.path(), "A-EN24").unwrap();
            let pipeline = &pipelines[&PipelineName::new("authored")];
            let blocks = vec![PdfBlock::bare(BlockType::RELEVANT_BLOCK, "hello")];
            let out = pipeline.text_filter.iter().next().unwrap().filter(&blocks, &FilterData::TargetCompanies(&[])).unwrap();
            assert_eq!(out.len(), 1);
            assert_eq!(out[0].type_block, BlockType::FUND);
            assert_eq!(out[0].content, BlockValue::from("hello"));
            assert_eq!(out[0].metadata.get("seen"), Some(&BlockValue::from(1i64)));
        }

        #[test]
        fn an_authored_deserialize_pipe_returns_promises() {
            let dir = repo_with_module("A-EN24", COMPLETE_MODULE);
            let pipelines = get_pipelines(dir.path(), "A-EN24").unwrap();
            let pipeline = &pipelines[&PipelineName::new("authored")];
            let block = TextBlock::from_content(BlockType::FUND, BTreeMap::new(), "Alpha Fund");
            let out = pipeline.deserialize.iter().next().unwrap().deserialize(&block).unwrap();
            assert_eq!(out.len(), 1);
            let promises = out[0].as_promises().expect("a promise entry");
            assert_eq!(promises.iter().collect::<Vec<_>>(), vec![("fund-id", &BlockValue::from("Alpha Fund"))]);
        }

        #[test]
        fn an_authored_pipe_that_raises_becomes_a_typed_author_error() {
            let source = "
class _P:
    def __init__(self):
        self.pdf_extract = self.boom
        self.text_filter = None
        self.deserialize = None
    def boom(self, page):
        raise ValueError('author bug')
pipelines = {'authored': _P()}
";
            let dir = repo_with_module("A-EN24", source);
            let pipelines = get_pipelines(dir.path(), "A-EN24").unwrap();
            let pipeline = &pipelines[&PipelineName::new("authored")];
            Python::attach(|py| {
                let page = page_with_raw(py);
                let err = pipeline.pdf_extract.iter().next().unwrap().extract(&page).unwrap_err();
                assert!(err.to_string().contains("author bug"), "{err}");
            });
        }

        #[test]
        fn a_format_without_compute_page_class_gets_the_identity_finalizer() {
            let dir = repo_with_module("A-EN24", COMPLETE_MODULE);
            let finalizer = get_page_class_finalizer(dir.path(), "A-EN24").unwrap();
            assert!(matches!(finalizer, crate::core::algorithm::PageClassFinalizer::Identity));
        }

        #[test]
        fn an_authored_compute_page_class_is_applied_to_the_classes() {
            let source = format!(
                "{COMPLETE_MODULE}\ndef compute_page_class(classes):\n    return [c or 'filled' for c in classes]\n"
            );
            let dir = repo_with_module("A-EN24", &source);
            let finalizer = get_page_class_finalizer(dir.path(), "A-EN24").unwrap();
            let classes = vec![Some(crate::core::schedule::PageClass::new("inv")), None];
            let out = finalizer.finalize(classes).unwrap();
            assert_eq!(
                out.iter().map(|c| c.as_ref().map(|c| c.as_str().to_string())).collect::<Vec<_>>(),
                vec![Some("inv".to_string()), Some("filled".to_string())]
            );
        }

        #[test]
        fn a_non_callable_compute_page_class_is_rejected() {
            let source = format!("{COMPLETE_MODULE}\ncompute_page_class = 1\n");
            let dir = repo_with_module("A-EN24", &source);
            assert!(matches!(
                get_page_class_finalizer(dir.path(), "A-EN24"),
                Err(UnstructuredError::ComputePageClassNotCallable { .. })
            ));
        }
    }
}
