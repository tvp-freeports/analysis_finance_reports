//! Loading the author's Python document classification: `metadata/document_classify.py`, or the
//! package `metadata/document_classify/`.
//!
//! The loading side only: the entries built here are run by [`crate::core::doc_classify`], Python
//! first and YAML after. The normative text is §6 of `agent-memory/SPEC-document-classification.md`.
//!
//! # The layout
//!
//! Like every author-written Python entry point of a formats repository, the module is **either** a
//! single file, `metadata/document_classify.py`, **or** a package, `metadata/document_classify/`
//! with an `__init__.py` (and as many other files as the author wants). Both present is an error:
//! which one is meant cannot be guessed. Neither present is no custom classification — never an
//! error. The shared-templates directory `content/templates` is on the import path while the
//! module is imported, as for the format modules (`unstructured::loader`).
//!
//! # The contract is by shape
//!
//! `document_classifier` is a list of `(page, pipeline)` pairs: the page a non-zero integer
//! (1-based, negative counted from the end), the pipeline any object — or mapping — with
//! `pdf_extract`, `text_filter` and `deserialize`, each a callable or an iterable of callables,
//! exactly as the `pipelines` of a format module.
//!
//! # Native pipes and author pipes
//!
//! A native pipe arriving from Python (the three standard bricks of phase 5, or any other) is
//! **unwrapped** back to its Rust form, as `unstructured::loader` does. An author's `pdf_extract` or
//! `text_filter` callable is wrapped in the ordinary adapters of `unstructured::py_pipe`. An author's
//! **deserializer** is not: in a document classification it returns `str` or `None` — the form
//! `DeserializerDocClassifyStandard` has in Python — which the generic adapter rightly refuses in an
//! extraction pipeline. [`PyDocClassDeserializePipe`] is the adapter dedicated to it.

use std::num::NonZeroI32;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyInt, PyList, PyString, PyTuple};

use crate::core::classes::TextBlock;
use crate::core::doc_classify::PythonEntry;
use crate::core::pipeline::{DeserializePipe, Extracted, PipeError, Pipeline};
use crate::python::pipes::{unwrap_deserialize, unwrap_pdf_extract, unwrap_text_filter};

use super::format_pattern::FormatPattern;
use super::metadata::METADATA_DIR;
use super::unstructured::loader::{
    BothLayouts, UnstructuredError, field, find_module, import_module_at, runtime_name,
    segment_callables,
};
use super::unstructured::py_pipe::{
    PyPdfExtractPipe, PyTextFilterPipe, author_error, text_block_to_py,
};

/// The name of the module (file `document_classify.py` or package `document_classify/`) inside the
/// repository's [`METADATA_DIR`](super::metadata::METADATA_DIR).
pub const DOCUMENT_CLASSIFY_MODULE: &str = "document_classify";

/// The variable the module must define.
pub const DOCUMENT_CLASSIFIER: &str = "document_classifier";

/// Why the Python document classification could not be loaded. Every variant names the module's
/// file; every variant about one entry names it as `document_classifier[<entry>]`.
#[derive(Debug, thiserror::Error)]
pub enum DocClassifyPyError {
    /// `metadata/document_classify.py` and `metadata/document_classify/__init__.py` both exist.
    #[error("{} and {} both exist: keep one of the two layouts", .file.display(), .package.display())]
    TwoLayouts { file: PathBuf, package: PathBuf },
    /// Importing the module raised; `message` is the Python exception.
    #[error("{}: failed to import the document classification module: {message}", .path.display())]
    ModuleLoad { path: PathBuf, message: String },
    /// The module exists and does not define `document_classifier`.
    #[error("{}: the module defines no 'document_classifier'", .path.display())]
    MissingClassifier { path: PathBuf },
    /// `document_classifier` is not a list (or tuple) of pairs.
    #[error("{}: 'document_classifier' must be a list of (page, pipeline) pairs, found a {found}", .path.display())]
    ClassifierNotAList { path: PathBuf, found: String },
    /// An element is not a two-element tuple or list.
    #[error("{}: document_classifier[{entry}] must be a (page, pipeline) pair, found {found}", .path.display())]
    NotAPair {
        path: PathBuf,
        entry: usize,
        found: String,
    },
    /// The page is not an integer (a `bool` is not a page either).
    #[error("{}: document_classifier[{entry}]: the page must be an integer, found {found}", .path.display())]
    PageNotAnInteger {
        path: PathBuf,
        entry: usize,
        found: String,
    },
    /// The page is an integer outside the range of a page number.
    #[error("{}: document_classifier[{entry}]: page {page} is out of range", .path.display())]
    PageOutOfRange {
        path: PathBuf,
        entry: usize,
        page: String,
    },
    /// Page `0`: pages are 1-based, and negative ones count from the end.
    #[error(
        "{}: document_classifier[{entry}]: page 0 does not exist (pages are 1-based, negative ones count from the end)",
        .path.display()
    )]
    PageZero { path: PathBuf, entry: usize },
    /// The pipeline lacks one of the three attributes.
    #[error("{}: document_classifier[{entry}]: the value is not a pipeline (it has no '{segment}')", .path.display())]
    NotAPipeline {
        path: PathBuf,
        entry: usize,
        segment: &'static str,
    },
    /// A segment is neither a callable nor an iterable of callables.
    #[error("{}: document_classifier[{entry}], segment '{segment}': {message}", .path.display())]
    InvalidSegment {
        path: PathBuf,
        entry: usize,
        segment: &'static str,
        message: String,
    },
    /// A segment holds no pipe: the pipeline could never produce anything.
    #[error("{}: document_classifier[{entry}]: the pipeline has no pipe in {}", .path.display(), .missing.join(", "))]
    IncompletePipeline {
        path: PathBuf,
        entry: usize,
        missing: Vec<&'static str>,
    },
}

/// The file to import, and whether it is a package's `__init__.py`. `None` when the repository has
/// no Python document classification (no file, no package, or no `metadata/` at all).
pub fn module_path(formats_repo_dir: &Path) -> Result<Option<(PathBuf, bool)>, DocClassifyPyError> {
    find_module(
        &formats_repo_dir.join(METADATA_DIR),
        DOCUMENT_CLASSIFY_MODULE,
    )
    .map_err(|BothLayouts { file, package }| DocClassifyPyError::TwoLayouts { file, package })
}

/// The entries of `document_classifier`, in the order written. No module: no entries.
///
/// Each entry's pipeline is named `document_classifier[<index>]`; an author pipe in it is named
/// `document_classifier[<index>]::<segment>[<position>]`, as in `unstructured::loader`.
pub fn load(formats_repo_dir: &Path) -> Result<Vec<PythonEntry>, DocClassifyPyError> {
    let Some((path, is_package)) = module_path(formats_repo_dir)? else {
        tracing::trace!("no Python document classification module in the repository");
        return Ok(Vec::new());
    };
    Python::attach(|py| {
        let module = import_module_at(py, formats_repo_dir, &path, is_package, &runtime_name(formats_repo_dir, DOCUMENT_CLASSIFY_MODULE))
            .map_err(|error| {
                let message = error.to_string();
                tracing::error!(path = %path.display(), "failed to import the document classification module: {message}");
                error.print(py);
                DocClassifyPyError::ModuleLoad { path: path.clone(), message }
            })?;
        let classifier = module
            .getattr(DOCUMENT_CLASSIFIER)
            .map_err(|_| DocClassifyPyError::MissingClassifier { path: path.clone() })?;
        // A set or a dict has no order to read the entries in, and a generator would be consumed
        // by the first load.
        let pairs = list_or_tuple_items(&classifier).ok_or_else(|| {
            DocClassifyPyError::ClassifierNotAList {
                path: path.clone(),
                found: type_name(&classifier),
            }
        })?;
        let entries = pairs
            .iter()
            .enumerate()
            .map(|(entry, pair)| read_entry(&path, entry, pair))
            .collect::<Result<Vec<_>, _>>()?;
        tracing::debug!(path = %path.display(), entry_count = entries.len(), "loaded the Python document classification");
        Ok(entries)
    })
}

/// The Python type name of `object`, for the errors.
fn type_name(object: &Bound<'_, PyAny>) -> String {
    object
        .get_type()
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "<unknown type>".to_string())
}

/// The items of a list or a tuple; `None` for anything else, iterable or not.
fn list_or_tuple_items<'py>(object: &Bound<'py, PyAny>) -> Option<Vec<Bound<'py, PyAny>>> {
    if !(object.is_instance_of::<PyList>() || object.is_instance_of::<PyTuple>()) {
        return None;
    }
    object.extract().ok()
}

/// One `(page, pipeline)` element of `document_classifier`, the `entry`-th.
fn read_entry(
    path: &Path,
    entry: usize,
    pair: &Bound<'_, PyAny>,
) -> Result<PythonEntry, DocClassifyPyError> {
    let not_a_pair = |found: String| DocClassifyPyError::NotAPair {
        path: path.to_path_buf(),
        entry,
        found,
    };
    // Only a list or a tuple: a two-character string or a two-key dict is iterable in two, too.
    let items = list_or_tuple_items(pair)
        .ok_or_else(|| not_a_pair(format!("an object of type {}", type_name(pair))))?;
    let [page, pipeline] = items.as_slice() else {
        return Err(not_a_pair(format!(
            "a {} of length {}",
            type_name(pair),
            items.len()
        )));
    };
    Ok(PythonEntry {
        page: read_page(path, entry, page)?,
        pipeline: read_pipeline(path, entry, pipeline)?,
    })
}

/// The page of an entry: a non-zero integer that fits a page number.
fn read_page(
    path: &Path,
    entry: usize,
    page: &Bound<'_, PyAny>,
) -> Result<NonZeroI32, DocClassifyPyError> {
    // `bool` first: it is a subclass of `int`, and `(True, pipeline)` would read as page 1.
    if page.is_instance_of::<PyBool>() || !page.is_instance_of::<PyInt>() {
        return Err(DocClassifyPyError::PageNotAnInteger {
            path: path.to_path_buf(),
            entry,
            found: type_name(page),
        });
    }
    let number: i32 = page
        .extract()
        .map_err(|_| DocClassifyPyError::PageOutOfRange {
            path: path.to_path_buf(),
            entry,
            page: page.str().map(|text| text.to_string()).unwrap_or_default(),
        })?;
    NonZeroI32::new(number).ok_or(DocClassifyPyError::PageZero {
        path: path.to_path_buf(),
        entry,
    })
}

/// The pipeline of an entry, recognised by shape. Native pipes are unwrapped to their Rust form;
/// an author's callables are adapted, the deserializer by [`PyDocClassDeserializePipe`].
fn read_pipeline(
    path: &Path,
    entry: usize,
    object: &Bound<'_, PyAny>,
) -> Result<Pipeline, DocClassifyPyError> {
    let pipeline_name = format!("{DOCUMENT_CLASSIFIER}[{entry}]");
    let callables = |segment: &'static str| {
        let raw = field(object, segment).ok_or(DocClassifyPyError::NotAPipeline {
            path: path.to_path_buf(),
            entry,
            segment,
        })?;
        segment_callables(raw, "", &pipeline_name, segment).map_err(|error| {
            let UnstructuredError::InvalidSegment { message, .. } = error else {
                unreachable!("segment_callables fails only with InvalidSegment")
            };
            DocClassifyPyError::InvalidSegment {
                path: path.to_path_buf(),
                entry,
                segment,
                message,
            }
        })
    };
    let pdf_extract = callables("pdf_extract")?;
    let text_filter = callables("text_filter")?;
    let deserialize = callables("deserialize")?;

    let missing: Vec<&'static str> = [
        ("pdf_extract", &pdf_extract),
        ("text_filter", &text_filter),
        ("deserialize", &deserialize),
    ]
    .into_iter()
    .filter(|(_, pipes)| pipes.is_empty())
    .map(|(segment, _)| segment)
    .collect();
    if !missing.is_empty() {
        return Err(DocClassifyPyError::IncompletePipeline {
            path: path.to_path_buf(),
            entry,
            missing,
        });
    }

    let pipe_name =
        |segment: &str, position: usize| format!("{pipeline_name}::{segment}[{position}]");
    let mut pipeline = Pipeline::new(pipeline_name.as_str());
    for (position, func) in pdf_extract.into_iter().enumerate() {
        pipeline
            .pdf_extract
            .push(unwrap_pdf_extract(&func).unwrap_or_else(|| {
                Arc::new(PyPdfExtractPipe::new(
                    &pipeline_name,
                    pipe_name("pdf_extract", position),
                    func.unbind(),
                ))
            }));
    }
    for (position, func) in text_filter.into_iter().enumerate() {
        pipeline
            .text_filter
            .push(unwrap_text_filter(&func).unwrap_or_else(|| {
                Arc::new(PyTextFilterPipe::new(
                    &pipeline_name,
                    pipe_name("text_filter", position),
                    func.unbind(),
                ))
            }));
    }
    for (position, func) in deserialize.into_iter().enumerate() {
        pipeline
            .deserialize
            .push(unwrap_deserialize(&func).unwrap_or_else(|| {
                Arc::new(PyDocClassDeserializePipe::new(
                    &pipeline_name,
                    pipe_name("deserialize", position),
                    func.unbind(),
                ))
            }));
    }
    Ok(pipeline)
}

/// An author's deserializer in a document-classification pipeline: it returns `str` or `None`, read
/// as one [`Extracted::DocClass`]. A string that is not a format pattern, or a value of another
/// type, is an error saying so.
pub struct PyDocClassDeserializePipe {
    pipeline: String,
    name: String,
    func: Py<PyAny>,
}

impl PyDocClassDeserializePipe {
    pub fn new(pipeline: impl Into<String>, name: impl Into<String>, func: Py<PyAny>) -> Self {
        Self {
            pipeline: pipeline.into(),
            name: name.into(),
            func,
        }
    }
}

impl DeserializePipe for PyDocClassDeserializePipe {
    fn name(&self) -> &str {
        &self.name
    }

    /// Every call takes the GIL back, as for the other author adapters.
    fn scales_with_threads(&self) -> bool {
        false
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        Python::attach(|py| {
            let call = || -> PyResult<Option<FormatPattern>> {
                let result = self.func.bind(py).call1((text_block_to_py(py, block)?,))?;
                if result.is_none() {
                    return Ok(None);
                }
                if !result.is_instance_of::<PyString>() {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "a document-classification deserializer must return a format pattern (str) or None, found a {}",
                        type_name(&result)
                    )));
                }
                let text: String = result.extract()?;
                text.parse().map(Some).map_err(|why| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "'{text}' is not a format pattern: {why}"
                    ))
                })
            };
            let pattern =
                call().map_err(|error| author_error(py, &self.pipeline, &self.name, error))?;
            // `trace!`: runs once per block.
            tracing::trace!(pipeline = %self.pipeline, pipe = %self.name, ?pattern, "author document-classification deserializer");
            Ok(vec![Extracted::DocClass(pattern)])
        })
    }
}

/// Test support shared with [`super::doc_classify`]'s tests: a document whose pages carry their
/// PyMuPDF dict, as the lazy path gives them, so that author pipes can read them.
#[cfg(test)]
pub(crate) mod test_support {
    use std::ffi::CString;
    use std::num::NonZeroI32;

    use pyo3::prelude::*;

    // ------------------------------------------------------------------------------------------
    // The pages: a fake document whose pages carry their PyMuPDF dict, as the lazy path gives them
    // ------------------------------------------------------------------------------------------

    /// Builds a PyMuPDF-like page dict from its lines of text.
    const PAGE_BUILDER: &str = r#"
def build(texts):
    lines = []
    for i, t in enumerate(texts):
        y = 12.0 * i
        bbox = (0.0, y, 200.0, y + 10.0)
        lines.append({'dir': (1.0, 0.0), 'bbox': bbox,
                      'spans': [{'font': 'Arial', 'size': 10.0, 'text': t, 'bbox': bbox}]})
    return {'width': 595.0, 'height': 842.0, 'blocks': [{'type': 0, 'lines': lines}] if lines else []}
"#;

    /// One page, with its native lines **and** its PyMuPDF dict: native pipes read the former,
    /// author pipes the latter.
    pub(crate) fn page_of(lines: &[&str]) -> crate::core::page::Page {
        Python::attach(|py| {
            let builder = pyo3::types::PyModule::from_code(
                py,
                &CString::new(PAGE_BUILDER).unwrap(),
                c"page_builder.py",
                c"page_builder",
            )
            .expect("page builder");
            let texts: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            let dict = builder
                .getattr("build")
                .unwrap()
                .call1((texts,))
                .expect("page dict");
            crate::python::pipes::page_from_py(&dict).expect("page from dict")
        })
    }

    /// A document held in memory, recording every page number asked for.
    pub(crate) struct FakeDocument {
        pages: Vec<Vec<String>>,
        requests: std::sync::Mutex<Vec<i32>>,
    }

    impl FakeDocument {
        pub(crate) fn new(pages: &[&[&str]]) -> Self {
            FakeDocument {
                pages: pages
                    .iter()
                    .map(|lines| lines.iter().map(|l| l.to_string()).collect())
                    .collect(),
                requests: std::sync::Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn requests(&self) -> Vec<i32> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl crate::core::doc_classify::PageSource for FakeDocument {
        fn page_count(&self) -> usize {
            self.pages.len()
        }

        fn page(
            &self,
            number: NonZeroI32,
        ) -> Result<Option<crate::core::page::Page>, crate::input::document::DocumentError>
        {
            self.requests.lock().unwrap().push(number.get());
            Ok(
                crate::input::document::lazy::resolve_page_number(number, self.pages.len()).map(
                    |index| {
                        let lines: Vec<&str> =
                            self.pages[index].iter().map(String::as_str).collect();
                        page_of(&lines)
                    },
                ),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::CString;
    use std::fs;
    use std::num::NonZeroI32;
    use std::sync::Arc;

    use tempfile::TempDir;

    use crate::core::classes::BlockType;
    use crate::formats_repo::format_pattern::FormatPattern;
    use crate::formats_repo::metadata::METADATA_DIR;
    use crate::formats_repo::unstructured::loader::TEMPLATES_DIR;

    use super::test_support::FakeDocument;

    // ------------------------------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------------------------------

    fn n(number: i32) -> NonZeroI32 {
        NonZeroI32::new(number).expect("test page numbers are never zero")
    }

    fn p(text: &str) -> FormatPattern {
        text.parse().expect("fixture pattern must be valid")
    }

    fn metadata(dir: &TempDir) -> PathBuf {
        dir.path().join(METADATA_DIR)
    }

    /// A repository with `metadata/` and `content/templates/`, and nothing in them.
    fn empty_repo() -> TempDir {
        let dir = TempDir::new().expect("temp dir");
        fs::create_dir_all(metadata(&dir)).expect("metadata dir");
        fs::create_dir_all(dir.path().join(TEMPLATES_DIR)).expect("templates dir");
        dir
    }

    /// A repository with the single-file layout.
    fn repo_with_file(source: &str) -> TempDir {
        let dir = empty_repo();
        fs::write(metadata(&dir).join("document_classify.py"), source).expect("write module");
        dir
    }

    /// A repository with the package layout: `__init__.py` plus the given other files.
    fn repo_with_package(init: &str, others: &[(&str, &str)]) -> TempDir {
        let dir = empty_repo();
        let package = metadata(&dir).join("document_classify");
        fs::create_dir_all(&package).expect("package dir");
        fs::write(package.join("__init__.py"), init).expect("write __init__");
        for (name, source) in others {
            fs::write(package.join(name), source).expect("write submodule");
        }
        dir
    }

    fn file_path(dir: &TempDir) -> PathBuf {
        metadata(dir).join("document_classify.py")
    }

    fn loaded(dir: &TempDir) -> Vec<PythonEntry> {
        match load(dir.path()) {
            Ok(entries) => entries,
            Err(e) => panic!("expected the module to load, got: {e}"),
        }
    }

    fn load_err(dir: &TempDir) -> DocClassifyPyError {
        match load(dir.path()) {
            Ok(entries) => panic!("expected the load to fail, got {} entries", entries.len()),
            Err(e) => e,
        }
    }

    /// A pure-author prelude: a pipeline class that is **not** the engine's, a reader of the
    /// PyMuPDF page dict, and a pipeline factory recognising one line. Imports nothing from the
    /// engine, so that the layout and shape tests do not depend on the Python package.
    const PRELUDE: &str = r#"
class P:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def texts(page):
    return [s['text'] for b in page['blocks'] for l in b.get('lines', []) for s in l['spans']]

def recognises(line, pattern):
    def extract(page):
        return [{'type_block': 'DOC_CLASS', 'metadata': {}, 'content': t} for t in texts(page) if t == line]
    def keep(blocks, data):
        return [{'type_block': 'DOC_CLASS', 'metadata': {}, 'content': b.content} for b in blocks]
    def deserialize(block):
        return pattern
    return P(extract, keep, deserialize)
"#;

    fn module(body: &str) -> String {
        format!("{PRELUDE}\n{body}\n")
    }

    /// Runs `entries` alone (no YAML) over `document`.
    fn classify_python(
        entries: Vec<PythonEntry>,
        formats: &[&str],
        document: &FakeDocument,
    ) -> Result<
        crate::core::doc_classify::DocClassification,
        crate::core::doc_classify::DocClassifyError,
    > {
        let formats = formats.iter().map(|f| f.to_string()).collect();
        // Under the engine tests' lock: see `SERIAL` in `core::doc_classify`'s tests for the
        // process-wide `tracing` race it closes. Never taken while attached to Python.
        let _guard = crate::core::doc_classify::tests::SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::core::doc_classify::DocumentClassifier::new(formats, entries, vec![])
            .classify(document)
    }

    const FORMATS: &[&str] = &[
        "AMUNDI-EN24",
        "EURIZON-EN23",
        "MEDIOLANUM-EN23",
        "MEDIOLANUM-IT24.A",
        "MEDIOLANUM-IT24.B",
    ];

    fn assert_complete(outcome: &crate::core::doc_classify::DocClassification, format: &str) {
        match outcome {
            crate::core::doc_classify::DocClassification::Complete(complete) => {
                assert_eq!(complete.format, p(format))
            }
            other => panic!("expected the complete format '{format}', got {other:?}"),
        }
    }

    // ==========================================================================================
    // The tests
    // ==========================================================================================

    /// The names the specification fixes.
    mod names {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_module_is_document_classify() {
            assert_eq!(DOCUMENT_CLASSIFY_MODULE, "document_classify");
        }

        #[test]
        fn the_variable_is_document_classifier() {
            assert_eq!(DOCUMENT_CLASSIFIER, "document_classifier");
        }
    }

    /// Where the module is looked for (§6): a file **or** a package, never both.
    mod layout {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_repository_without_the_module_has_none() {
            let dir = empty_repo();
            assert!(module_path(dir.path()).unwrap().is_none());
        }

        #[test]
        fn a_repository_without_metadata_has_none() {
            let dir = TempDir::new().unwrap();
            assert!(module_path(dir.path()).unwrap().is_none());
        }

        #[test]
        fn the_single_file_is_found() {
            let dir = repo_with_file("");
            let (path, is_package) = module_path(dir.path()).unwrap().unwrap();
            assert_eq!(path, file_path(&dir));
            assert!(!is_package);
        }

        #[test]
        fn the_package_is_found_through_its_init() {
            let dir = repo_with_package("", &[]);
            let (path, is_package) = module_path(dir.path()).unwrap().unwrap();
            assert_eq!(
                path,
                metadata(&dir).join("document_classify").join("__init__.py")
            );
            assert!(is_package);
        }

        /// Not the layout: a folder is a package only with its `__init__.py`.
        #[test]
        fn a_folder_without_init_is_not_a_module() {
            let dir = empty_repo();
            fs::create_dir_all(metadata(&dir).join("document_classify")).unwrap();
            fs::write(
                metadata(&dir).join("document_classify").join("rules.py"),
                "x = 1\n",
            )
            .unwrap();
            assert!(module_path(dir.path()).unwrap().is_none());
        }

        /// §6: two sources for one module are an error — as they now are for a format's algorithm
        /// too (`unstructured::loader`, `TwoLayouts`).
        #[test]
        fn both_layouts_present_is_an_error_naming_both() {
            let dir = repo_with_package("", &[]);
            fs::write(file_path(&dir), "").unwrap();
            match module_path(dir.path()) {
                Err(DocClassifyPyError::TwoLayouts { file, package }) => {
                    assert_eq!(file, file_path(&dir));
                    assert_eq!(
                        package,
                        metadata(&dir).join("document_classify").join("__init__.py")
                    );
                }
                other => panic!("expected TwoLayouts, got {other:?}"),
            }
        }

        #[test]
        fn both_layouts_present_fails_the_load_too() {
            let dir = repo_with_package("document_classifier = []\n", &[]);
            fs::write(file_path(&dir), "document_classifier = []\n").unwrap();
            assert!(matches!(
                load_err(&dir),
                DocClassifyPyError::TwoLayouts { .. }
            ));
        }

        #[test]
        fn the_two_layouts_error_names_both_paths_in_its_message() {
            let dir = repo_with_package("", &[]);
            fs::write(file_path(&dir), "").unwrap();
            let message = module_path(dir.path()).unwrap_err().to_string();
            assert!(
                message.contains(&file_path(&dir).display().to_string()),
                "{message}"
            );
            assert!(
                message.contains(
                    &metadata(&dir)
                        .join("document_classify")
                        .display()
                        .to_string()
                ),
                "{message}"
            );
        }
    }

    /// Absent module: no custom classification, never an error (§6).
    mod no_module {
        use super::*;

        #[test]
        fn no_module_means_no_entries() {
            assert!(loaded(&empty_repo()).is_empty());
        }

        #[test]
        fn no_metadata_directory_means_no_entries_either() {
            let dir = TempDir::new().unwrap();
            assert!(loaded(&dir).is_empty());
        }

        /// Other files in `metadata/` are not the module.
        #[test]
        fn a_module_of_another_name_is_ignored() {
            let dir = empty_repo();
            fs::write(
                metadata(&dir).join("document_classifier.py"),
                "raise RuntimeError('not me')\n",
            )
            .unwrap();
            assert!(loaded(&dir).is_empty());
        }
    }

    /// Both layouts load, and the package may be several files.
    mod importing {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_single_file_layout_loads() {
            let dir = repo_with_file(&module(
                "document_classifier = [(1, recognises('Amundi Funds', 'AMUNDI-EN24'))]",
            ));
            let entries = loaded(&dir);
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].page, n(1));
        }

        #[test]
        fn the_package_layout_loads_with_a_submodule_it_imports() {
            let dir = repo_with_package(
                "from .rules import document_classifier\n",
                &[(
                    "rules.py",
                    &module(
                        "document_classifier = [(2, recognises('Eurizon Fund', 'EURIZON-EN23'))]",
                    ),
                )],
            );
            let entries = loaded(&dir);
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].page, n(2));
        }

        #[test]
        fn the_package_layout_loads_with_the_list_in_init_itself() {
            let dir = repo_with_package(
                &module("document_classifier = [(1, recognises('x', 'AMUNDI-EN24'))]"),
                &[],
            );
            assert_eq!(loaded(&dir).len(), 1);
        }

        /// The shared templates are importable, as for the format modules.
        #[test]
        fn the_templates_directory_is_on_the_import_path() {
            let dir = repo_with_file("from doc_classify_helpers_7c1 import document_classifier\n");
            fs::write(
                dir.path()
                    .join(TEMPLATES_DIR)
                    .join("doc_classify_helpers_7c1.py"),
                module("document_classifier = [(3, recognises('x', 'AMUNDI-EN24'))]"),
            )
            .unwrap();
            assert_eq!(loaded(&dir)[0].page, n(3));
        }

        /// Two repositories are two modules: loading the second must not see the first one's code,
        /// not even a submodule of the same name.
        #[test]
        fn two_repositories_with_the_same_submodule_name_load_their_own_code() {
            let first = repo_with_package(
                "from .rules import document_classifier\n",
                &[(
                    "rules.py",
                    &module("document_classifier = [(1, recognises('x', 'AMUNDI-EN24'))]"),
                )],
            );
            let second = repo_with_package(
                "from .rules import document_classifier\n",
                &[(
                    "rules.py",
                    &module(
                        "document_classifier = [(-1, recognises('x', 'AMUNDI-EN24')), (2, recognises('y', 'EURIZON-EN23'))]",
                    ),
                )],
            );
            assert_eq!(loaded(&first).len(), 1);
            let entries = loaded(&second);
            assert_eq!(
                entries.iter().map(|e| e.page).collect::<Vec<_>>(),
                vec![n(-1), n(2)]
            );
            // And back: the first is still itself.
            assert_eq!(loaded(&first).len(), 1);
        }

        #[test]
        fn two_repositories_with_the_single_file_layout_load_their_own_code() {
            let first = repo_with_file(&module(
                "document_classifier = [(1, recognises('x', 'AMUNDI-EN24'))]",
            ));
            let second = repo_with_file(&module("document_classifier = []"));
            assert_eq!(loaded(&first).len(), 1);
            assert!(loaded(&second).is_empty());
        }

        #[test]
        fn loading_the_same_repository_twice_gives_the_same_entries() {
            let dir = repo_with_file(&module(
                "document_classifier = [(1, recognises('x', 'AMUNDI-EN24')), (-2, recognises('y', 'EURIZON-EN23'))]",
            ));
            let pages =
                |entries: Vec<PythonEntry>| entries.iter().map(|e| e.page).collect::<Vec<_>>();
            assert_eq!(pages(loaded(&dir)), pages(loaded(&dir)));
        }
    }

    /// A module that raises while being imported: the error carries the Python exception.
    mod import_errors {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_exception_at_import_is_a_module_load_error_with_the_exception() {
            let dir = repo_with_file("raise RuntimeError('boom at import')\n");
            match load_err(&dir) {
                DocClassifyPyError::ModuleLoad { path, message } => {
                    assert_eq!(path, file_path(&dir));
                    assert!(message.contains("boom at import"), "{message}");
                    assert!(message.contains("RuntimeError"), "{message}");
                }
                other => panic!("expected ModuleLoad, got {other:?}"),
            }
        }

        #[test]
        fn the_module_load_message_names_the_file_and_the_exception() {
            let dir = repo_with_file("raise RuntimeError('boom at import')\n");
            let message = load_err(&dir).to_string();
            assert!(
                message.contains(&file_path(&dir).display().to_string()),
                "{message}"
            );
            assert!(message.contains("boom at import"), "{message}");
        }

        #[test]
        fn a_syntax_error_is_a_module_load_error() {
            let dir = repo_with_file("document_classifier = [(1, \n");
            assert!(matches!(
                load_err(&dir),
                DocClassifyPyError::ModuleLoad { .. }
            ));
        }

        #[test]
        fn an_exception_in_a_submodule_of_the_package_is_a_module_load_error() {
            let dir = repo_with_package(
                "from .rules import document_classifier\n",
                &[("rules.py", "raise ValueError('broken rules')\n")],
            );
            match load_err(&dir) {
                DocClassifyPyError::ModuleLoad { path, message } => {
                    assert_eq!(
                        path,
                        metadata(&dir).join("document_classify").join("__init__.py")
                    );
                    assert!(message.contains("broken rules"), "{message}");
                }
                other => panic!("expected ModuleLoad, got {other:?}"),
            }
        }

        #[test]
        fn a_missing_import_is_a_module_load_error() {
            let dir = repo_with_file("import a_module_that_does_not_exist_9f2\n");
            let message = load_err(&dir).to_string();
            assert!(
                message.contains("a_module_that_does_not_exist_9f2"),
                "{message}"
            );
        }
    }

    /// `document_classifier` itself: present, and a list.
    mod the_variable {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Choice (the specification is silent): a module that exists **is** the author saying
        /// "classify with this", so a module without the variable is a mistake — most likely a
        /// typo in its name — not an absent classification. Silently classifying nothing would
        /// hide it until a document fails to classify.
        #[test]
        fn a_module_without_the_variable_is_an_error() {
            let dir = repo_with_file("document_classifiers = []\n");
            match load_err(&dir) {
                DocClassifyPyError::MissingClassifier { path } => assert_eq!(path, file_path(&dir)),
                other => panic!("expected MissingClassifier, got {other:?}"),
            }
        }

        #[test]
        fn the_missing_variable_error_names_the_file_and_the_variable() {
            let dir = repo_with_file("x = 1\n");
            let message = load_err(&dir).to_string();
            assert!(
                message.contains(&file_path(&dir).display().to_string()),
                "{message}"
            );
            assert!(message.contains("document_classifier"), "{message}");
        }

        #[test]
        fn a_package_without_the_variable_is_an_error_too() {
            let dir = repo_with_package("", &[("rules.py", "document_classifier = []\n")]);
            assert!(matches!(
                load_err(&dir),
                DocClassifyPyError::MissingClassifier { .. }
            ));
        }

        #[test]
        fn an_empty_list_is_no_entries_and_not_an_error() {
            assert!(loaded(&repo_with_file("document_classifier = []\n")).is_empty());
        }

        #[test]
        fn a_tuple_of_pairs_is_accepted_as_a_list() {
            let dir = repo_with_file(&module(
                "document_classifier = ((1, recognises('x', 'AMUNDI-EN24')),)",
            ));
            assert_eq!(loaded(&dir).len(), 1);
        }

        fn wrong_type(value: &str, expected_found: &str) {
            let dir = repo_with_file(&format!("document_classifier = {value}\n"));
            match load_err(&dir) {
                DocClassifyPyError::ClassifierNotAList { path, found } => {
                    assert_eq!(path, file_path(&dir));
                    assert!(found.contains(expected_found), "found: {found}");
                }
                other => panic!("expected ClassifierNotAList for {value}, got {other:?}"),
            }
        }

        #[test]
        fn none_is_the_wrong_type() {
            wrong_type("None", "NoneType");
        }

        #[test]
        fn a_dict_is_the_wrong_type() {
            wrong_type("{1: None}", "dict");
        }

        /// Unordered: the order of the entries is semantics (§3).
        #[test]
        fn a_set_is_the_wrong_type() {
            wrong_type("{1, 2}", "set");
        }

        #[test]
        fn a_string_is_the_wrong_type() {
            wrong_type("'1, x'", "str");
        }

        #[test]
        fn an_integer_is_the_wrong_type() {
            wrong_type("1", "int");
        }

        /// A generator is iterable, but has no order the author can read back, and would be
        /// consumed by the first load.
        #[test]
        fn a_generator_is_the_wrong_type() {
            wrong_type("(x for x in [])", "generator");
        }

        #[test]
        fn the_wrong_type_error_names_the_file_and_the_type() {
            let dir = repo_with_file("document_classifier = {1: None}\n");
            let message = load_err(&dir).to_string();
            assert!(
                message.contains(&file_path(&dir).display().to_string()),
                "{message}"
            );
            assert!(message.contains("dict"), "{message}");
        }
    }

    /// Each element: a `(page, pipeline)` pair.
    mod pairs {
        use super::*;
        use pretty_assertions::assert_eq;

        fn not_a_pair(element: &str, entry: usize) {
            let body =
                format!("document_classifier = [(1, recognises('x', 'AMUNDI-EN24')), {element}]");
            let dir = repo_with_file(&module(&body));
            match load_err(&dir) {
                DocClassifyPyError::NotAPair {
                    path,
                    entry: found_entry,
                    ..
                } => {
                    assert_eq!(path, file_path(&dir));
                    assert_eq!(found_entry, entry);
                }
                other => panic!("expected NotAPair for {element}, got {other:?}"),
            }
        }

        #[test]
        fn a_list_of_two_is_a_pair_too() {
            let dir = repo_with_file(&module(
                "document_classifier = [[1, recognises('x', 'AMUNDI-EN24')]]",
            ));
            assert_eq!(loaded(&dir)[0].page, n(1));
        }

        #[test]
        fn a_triple_is_not_a_pair() {
            not_a_pair("(2, recognises('x', 'AMUNDI-EN24'), 'extra')", 1);
        }

        #[test]
        fn a_single_is_not_a_pair() {
            not_a_pair("(2,)", 1);
        }

        #[test]
        fn an_empty_tuple_is_not_a_pair() {
            not_a_pair("()", 1);
        }

        #[test]
        fn a_bare_pipeline_is_not_a_pair() {
            not_a_pair("recognises('x', 'AMUNDI-EN24')", 1);
        }

        #[test]
        fn a_bare_page_is_not_a_pair() {
            not_a_pair("2", 1);
        }

        /// Two characters, iterable, of length two: still not a pair.
        #[test]
        fn a_two_character_string_is_not_a_pair() {
            not_a_pair("'12'", 1);
        }

        #[test]
        fn a_dict_is_not_a_pair() {
            not_a_pair("{'page': 2, 'pipeline': None}", 1);
        }

        #[test]
        fn the_reversed_pair_is_caught_on_the_page() {
            let dir = repo_with_file(&module(
                "document_classifier = [(recognises('x', 'AMUNDI-EN24'), 1)]",
            ));
            assert!(matches!(
                load_err(&dir),
                DocClassifyPyError::PageNotAnInteger { entry: 0, .. }
            ));
        }

        #[test]
        fn the_not_a_pair_error_names_the_file_and_the_entry() {
            let dir = repo_with_file(&module(
                "document_classifier = [(1, recognises('x', 'AMUNDI-EN24')), (2,)]",
            ));
            let message = load_err(&dir).to_string();
            assert!(
                message.contains(&file_path(&dir).display().to_string()),
                "{message}"
            );
            assert!(message.contains("document_classifier[1]"), "{message}");
        }
    }

    /// The page of an entry: a non-zero integer, negative from the end.
    mod pages {
        use super::*;
        use pretty_assertions::assert_eq;

        fn page_of_entry(page: &str) -> Result<Vec<PythonEntry>, DocClassifyPyError> {
            let dir = repo_with_file(&module(&format!(
                "document_classifier = [({page}, recognises('x', 'AMUNDI-EN24'))]"
            )));
            load(dir.path())
        }

        #[test]
        fn a_positive_page_is_kept() {
            assert_eq!(page_of_entry("4").unwrap()[0].page, n(4));
        }

        #[test]
        fn a_negative_page_is_kept_as_written() {
            assert_eq!(page_of_entry("-2").unwrap()[0].page, n(-2));
        }

        #[test]
        fn page_zero_is_an_error_naming_the_entry() {
            let dir = repo_with_file(&module(
                "document_classifier = [(1, recognises('x', 'AMUNDI-EN24')), (0, recognises('y', 'AMUNDI-EN24'))]",
            ));
            match load_err(&dir) {
                DocClassifyPyError::PageZero { path, entry } => {
                    assert_eq!(path, file_path(&dir));
                    assert_eq!(entry, 1);
                }
                other => panic!("expected PageZero, got {other:?}"),
            }
        }

        #[test]
        fn the_page_zero_message_names_the_file_and_the_entry() {
            let dir = repo_with_file(&module(
                "document_classifier = [(0, recognises('y', 'AMUNDI-EN24'))]",
            ));
            let message = load_err(&dir).to_string();
            assert!(
                message.contains(&file_path(&dir).display().to_string()),
                "{message}"
            );
            assert!(message.contains("document_classifier[0]"), "{message}");
        }

        fn not_an_integer(page: &str, found: &str) {
            match page_of_entry(page) {
                Err(DocClassifyPyError::PageNotAnInteger {
                    entry: 0, found: f, ..
                }) => {
                    assert!(f.contains(found), "found: {f}");
                }
                other => panic!("expected PageNotAnInteger for {page}, got {other:?}"),
            }
        }

        #[test]
        fn a_float_page_is_not_an_integer_even_when_whole() {
            not_an_integer("1.0", "float");
        }

        #[test]
        fn a_string_page_is_not_an_integer() {
            not_an_integer("'1'", "str");
        }

        #[test]
        fn a_none_page_is_not_an_integer() {
            not_an_integer("None", "NoneType");
        }

        /// In Python `True == 1`, which would make `(True, pipeline)` read as page 1.
        #[test]
        fn a_boolean_page_is_not_an_integer() {
            not_an_integer("True", "bool");
        }

        #[test]
        fn the_not_an_integer_message_names_the_file_and_the_entry() {
            let dir = repo_with_file(&module(
                "document_classifier = [('1', recognises('x', 'AMUNDI-EN24'))]",
            ));
            let message = load_err(&dir).to_string();
            assert!(
                message.contains(&file_path(&dir).display().to_string()),
                "{message}"
            );
            assert!(message.contains("document_classifier[0]"), "{message}");
        }

        #[test]
        fn a_page_beyond_the_range_of_a_page_number_is_an_error() {
            match page_of_entry("2 ** 40") {
                Err(DocClassifyPyError::PageOutOfRange { entry: 0, page, .. }) => {
                    assert_eq!(page, "1099511627776")
                }
                other => panic!("expected PageOutOfRange, got {other:?}"),
            }
        }

        #[test]
        fn a_very_negative_page_is_out_of_range_too() {
            assert!(matches!(
                page_of_entry("-(2 ** 40)"),
                Err(DocClassifyPyError::PageOutOfRange { .. })
            ));
        }

        #[test]
        fn the_largest_page_number_is_accepted() {
            assert_eq!(page_of_entry("2 ** 31 - 1").unwrap()[0].page, n(i32::MAX));
        }
    }

    /// The pipeline, recognised by shape (§6), as the `pipelines` of a format module.
    mod pipeline_shape {
        use super::*;
        use pretty_assertions::assert_eq;

        fn not_a_pipeline(pipeline: &str, segment: &'static str) {
            let dir = repo_with_file(&module(&format!(
                "document_classifier = [(1, recognises('x', 'AMUNDI-EN24')), (2, {pipeline})]"
            )));
            match load_err(&dir) {
                DocClassifyPyError::NotAPipeline {
                    path,
                    entry,
                    segment: missing,
                } => {
                    assert_eq!(path, file_path(&dir));
                    assert_eq!(entry, 1);
                    assert_eq!(missing, segment);
                }
                other => panic!("expected NotAPipeline({segment}), got {other:?}"),
            }
        }

        #[test]
        fn an_object_without_pdf_extract_is_not_a_pipeline() {
            not_a_pipeline(
                "type('Q', (), {'text_filter': None, 'deserialize': None})()",
                "pdf_extract",
            );
        }

        #[test]
        fn an_object_without_text_filter_is_not_a_pipeline() {
            not_a_pipeline(
                "type('Q', (), {'pdf_extract': len, 'deserialize': len})()",
                "text_filter",
            );
        }

        #[test]
        fn an_object_without_deserialize_is_not_a_pipeline() {
            not_a_pipeline(
                "type('Q', (), {'pdf_extract': len, 'text_filter': len})()",
                "deserialize",
            );
        }

        #[test]
        fn a_bare_object_is_not_a_pipeline_and_the_first_missing_segment_is_named() {
            not_a_pipeline("object()", "pdf_extract");
        }

        #[test]
        fn the_not_a_pipeline_message_names_the_file_the_entry_and_the_segment() {
            let dir = repo_with_file(&module("document_classifier = [(1, object())]"));
            let message = load_err(&dir).to_string();
            assert!(
                message.contains(&file_path(&dir).display().to_string()),
                "{message}"
            );
            assert!(message.contains("document_classifier[0]"), "{message}");
            assert!(message.contains("pdf_extract"), "{message}");
        }

        /// As for `pipelines`: a mapping with the three keys is a pipeline too.
        #[test]
        fn a_mapping_with_the_three_keys_is_a_pipeline() {
            let dir = repo_with_file(&module(
                "r = recognises('x', 'AMUNDI-EN24')\n\
                 document_classifier = [(1, {'pdf_extract': r.pdf_extract, 'text_filter': r.text_filter, 'deserialize': r.deserialize})]",
            ));
            assert_eq!(loaded(&dir).len(), 1);
        }

        #[test]
        fn a_segment_may_be_one_callable_or_several() {
            let dir = repo_with_file(&module(
                "r = recognises('x', 'AMUNDI-EN24')\n\
                 document_classifier = [(1, P([r.pdf_extract, r.pdf_extract], (r.text_filter,), r.deserialize))]",
            ));
            let entries = loaded(&dir);
            assert_eq!(entries[0].pipeline.pdf_extract.len(), 2);
            assert_eq!(entries[0].pipeline.text_filter.len(), 1);
            assert_eq!(entries[0].pipeline.deserialize.len(), 1);
        }

        #[test]
        fn a_non_callable_pipe_in_a_segment_is_an_invalid_segment() {
            let dir = repo_with_file(&module(
                "r = recognises('x', 'AMUNDI-EN24')\n\
                 document_classifier = [(1, P([r.pdf_extract, 3], r.text_filter, r.deserialize))]",
            ));
            match load_err(&dir) {
                DocClassifyPyError::InvalidSegment {
                    path,
                    entry,
                    segment,
                    ..
                } => {
                    assert_eq!(path, file_path(&dir));
                    assert_eq!(entry, 0);
                    assert_eq!(segment, "pdf_extract");
                }
                other => panic!("expected InvalidSegment, got {other:?}"),
            }
        }

        #[test]
        fn a_segment_that_is_neither_callable_nor_iterable_is_an_invalid_segment() {
            let dir = repo_with_file(&module(
                "r = recognises('x', 'AMUNDI-EN24')\n\
                 document_classifier = [(1, P(r.pdf_extract, r.text_filter, 3))]",
            ));
            assert!(matches!(
                load_err(&dir),
                DocClassifyPyError::InvalidSegment {
                    segment: "deserialize",
                    ..
                }
            ));
        }

        /// Choice (the specification is silent): a format's pipeline may be completed by another
        /// level, but nothing completes an entry of `document_classifier`. An empty segment would
        /// fail on every document; it is found when loading instead.
        #[test]
        fn an_empty_segment_is_an_incomplete_pipeline_naming_it() {
            let dir = repo_with_file(&module(
                "r = recognises('x', 'AMUNDI-EN24')\n\
                 document_classifier = [(1, P(r.pdf_extract, [], r.deserialize))]",
            ));
            match load_err(&dir) {
                DocClassifyPyError::IncompletePipeline {
                    path,
                    entry,
                    missing,
                } => {
                    assert_eq!(path, file_path(&dir));
                    assert_eq!(entry, 0);
                    assert_eq!(missing, vec!["text_filter"]);
                }
                other => panic!("expected IncompletePipeline, got {other:?}"),
            }
        }

        #[test]
        fn none_segments_are_empty_and_all_of_them_are_named_in_order() {
            let dir = repo_with_file(&module("document_classifier = [(1, P(None, None, None))]"));
            match load_err(&dir) {
                DocClassifyPyError::IncompletePipeline { missing, .. } => {
                    assert_eq!(missing, vec!["pdf_extract", "text_filter", "deserialize"]);
                }
                other => panic!("expected IncompletePipeline, got {other:?}"),
            }
        }

        #[test]
        fn the_entries_keep_the_order_written() {
            let dir = repo_with_file(&module(
                "document_classifier = [(3, recognises('a', 'AMUNDI-EN24')), (1, recognises('b', 'AMUNDI-EN24')), (-1, recognises('c', 'AMUNDI-EN24'))]",
            ));
            assert_eq!(
                loaded(&dir).iter().map(|e| e.page).collect::<Vec<_>>(),
                vec![n(3), n(1), n(-1)]
            );
        }

        /// The same page may appear twice: two pipelines on one page, tried in order (§6).
        #[test]
        fn the_same_page_may_appear_in_several_entries() {
            let dir = repo_with_file(&module(
                "document_classifier = [(1, recognises('a', 'AMUNDI-EN24')), (1, recognises('b', 'EURIZON-EN23'))]",
            ));
            assert_eq!(loaded(&dir).len(), 2);
        }

        /// A pipeline has no name in a list: its position names it, in errors and in the outcome.
        #[test]
        fn each_pipeline_is_named_by_its_position() {
            let dir = repo_with_file(&module(
                "document_classifier = [(1, recognises('a', 'AMUNDI-EN24')), (2, recognises('b', 'EURIZON-EN23'))]",
            ));
            let names: Vec<String> = loaded(&dir)
                .iter()
                .map(|e| e.pipeline.name.to_string())
                .collect();
            assert_eq!(
                names,
                vec!["document_classifier[0]", "document_classifier[1]"]
            );
        }

        /// The same pipeline object used twice yields two entries, each its own name.
        #[test]
        fn one_pipeline_object_may_serve_two_pages() {
            let dir = repo_with_file(&module(
                "r = recognises('a', 'AMUNDI-EN24')\ndocument_classifier = [(1, r), (-1, r)]",
            ));
            let entries = loaded(&dir);
            assert_eq!(entries.len(), 2);
            assert_eq!(
                entries[1].pipeline.name.to_string(),
                "document_classifier[1]"
            );
        }
    }

    /// Native pipes arriving from Python are unwrapped; author pipes are adapted (§6, phase 5).
    mod native_and_author_pipes {
        use super::*;
        use pretty_assertions::assert_eq;

        /// An author module using the engine's own `Pipeline` and the three standard bricks,
        /// mixed with author callables.
        const MIXED: &str = r#"
from freeports.core import Pipeline
from freeports.standard_funcs.pdf_extract import PdfExtractDocClassifyStandard
from freeports.standard_funcs.text_filter import TextFilterDocClassifyStandard
from freeports.standard_funcs.deserialize import DeserializerDocClassifyStandard
from freeports.utils.pdf_extract import PdfLineSelection

def texts(page):
    return [s['text'] for b in page['blocks'] for l in b.get('lines', []) for s in l['spans']]

def eurizon(page):
    # The shape the standard filter reduces: one block, the pattern or None in its metadata.
    found = 'EURIZON-EN23' if 'Eurizon Fund' in texts(page) else None
    return [{'type_block': 'DOC_CLASS', 'metadata': {'format_pattern': found}, 'content': ''}]

def read_pattern(block):
    return block.metadata.get('format_pattern')

document_classifier = [
    (1, Pipeline(
        pdf_extract=[PdfExtractDocClassifyStandard([PdfLineSelection(text='^Amundi Funds$')], 'AMUNDI-EN24'), eurizon],
        text_filter=TextFilterDocClassifyStandard(),
        deserialize=read_pattern,
    )),
    (2, Pipeline(
        pdf_extract=PdfExtractDocClassifyStandard([PdfLineSelection(text='^Mediolanum$')], 'MEDIOLANUM'),
        text_filter=TextFilterDocClassifyStandard(),
        deserialize=DeserializerDocClassifyStandard(),
    )),
]
"#;

        fn names<P: ?Sized + PipeName>(pipes: impl Iterator<Item = Arc<P>>) -> Vec<String> {
            pipes.map(|pipe| pipe.pipe_name()).collect()
        }

        /// The three pipe traits each have their own `name`; one helper for the three.
        trait PipeName {
            fn pipe_name(&self) -> String;
        }
        impl PipeName for dyn crate::core::pipeline::PdfExtractPipe {
            fn pipe_name(&self) -> String {
                self.name().to_string()
            }
        }
        impl PipeName for dyn crate::core::pipeline::TextFilterPipe {
            fn pipe_name(&self) -> String {
                self.name().to_string()
            }
        }
        impl PipeName for dyn DeserializePipe {
            fn pipe_name(&self) -> String {
                self.name().to_string()
            }
        }

        #[test]
        fn native_pipes_are_unwrapped_and_author_pipes_adapted() {
            let entries = loaded(&repo_with_file(MIXED));
            let first = &entries[0].pipeline;
            assert_eq!(
                names(first.pdf_extract.iter().cloned()),
                vec![
                    "PdfExtractDocClassifyStandard",
                    "document_classifier[0]::pdf_extract[1]"
                ]
            );
            assert_eq!(
                names(first.text_filter.iter().cloned()),
                vec!["TextFilterDocClassifyStandard"]
            );
            assert_eq!(
                names(first.deserialize.iter().cloned()),
                vec!["document_classifier[0]::deserialize[0]"]
            );
        }

        #[test]
        fn the_standard_deserializer_from_python_is_unwrapped_to_the_native_one() {
            let entries = loaded(&repo_with_file(MIXED));
            assert_eq!(
                names(entries[1].pipeline.deserialize.iter().cloned()),
                vec!["DeserializerDocClassifyStandard"]
            );
        }

        /// A pipeline of native pipes only stays native end to end: it scales with threads, which
        /// a Python round trip would not.
        #[test]
        fn an_all_native_pipeline_scales_with_threads() {
            let entries = loaded(&repo_with_file(MIXED));
            assert!(entries[1].pipeline.scales_with_threads());
            assert!(!entries[0].pipeline.scales_with_threads());
        }

        #[test]
        fn the_native_pipe_found_by_the_mixed_pipeline_classifies() {
            let entries = loaded(&repo_with_file(MIXED));
            let document = FakeDocument::new(&[&["Amundi Funds"]]);
            assert_complete(
                &classify_python(entries, FORMATS, &document).unwrap(),
                "AMUNDI-EN24",
            );
        }

        #[test]
        fn the_author_pipe_found_by_the_mixed_pipeline_classifies() {
            let entries = loaded(&repo_with_file(MIXED));
            let document = FakeDocument::new(&[&["Eurizon Fund"]]);
            assert_complete(
                &classify_python(entries, FORMATS, &document).unwrap(),
                "EURIZON-EN23",
            );
        }

        #[test]
        fn the_all_native_pipeline_classifies_a_partial() {
            let entries = loaded(&repo_with_file(MIXED));
            let document = FakeDocument::new(&[&["nothing"], &["Mediolanum"]]);
            match classify_python(entries, FORMATS, &document).unwrap() {
                crate::core::doc_classify::DocClassification::Partial {
                    pattern,
                    candidates,
                } => {
                    assert_eq!(pattern, p("MEDIOLANUM"));
                    assert_eq!(
                        candidates,
                        vec!["MEDIOLANUM-EN23", "MEDIOLANUM-IT24.A", "MEDIOLANUM-IT24.B"]
                    );
                }
                other => panic!("expected a partial, got {other:?}"),
            }
        }

        #[test]
        fn a_pure_author_pipeline_classifies() {
            let dir = repo_with_file(&module(
                "document_classifier = [(1, recognises('Amundi Funds', 'AMUNDI-EN24'))]",
            ));
            let document = FakeDocument::new(&[&["Annual Report", "Amundi Funds"]]);
            assert_complete(
                &classify_python(loaded(&dir), FORMATS, &document).unwrap(),
                "AMUNDI-EN24",
            );
        }

        #[test]
        fn a_pure_author_pipeline_that_finds_nothing_leaves_the_document_unclassified() {
            let dir = repo_with_file(&module(
                "document_classifier = [(1, recognises('Amundi Funds', 'AMUNDI-EN24'))]",
            ));
            let document = FakeDocument::new(&[&["Eurizon Fund"]]);
            assert_eq!(
                classify_python(loaded(&dir), FORMATS, &document).unwrap(),
                crate::core::doc_classify::DocClassification::Unclassified
            );
        }

        /// The loader does not judge which native pipe sits in which segment; a page-class
        /// deserializer in a document classification is the engine's runtime error.
        #[test]
        fn a_native_pipe_of_the_wrong_classification_loads_and_fails_at_run_time() {
            let source = r#"
from freeports.core import Pipeline
from freeports.standard_funcs.pdf_extract import PdfExtractDocClassifyStandard
from freeports.standard_funcs.text_filter import TextFilterDocClassifyStandard
from freeports.standard_funcs.deserialize import DeserializerPageClassifyStandard
document_classifier = [(1, Pipeline(
    pdf_extract=PdfExtractDocClassifyStandard([], 'AMUNDI-EN24'),
    text_filter=TextFilterDocClassifyStandard(),
    deserialize=DeserializerPageClassifyStandard(),
))]
"#;
            let entries = loaded(&repo_with_file(source));
            assert_eq!(
                names(entries[0].pipeline.deserialize.iter().cloned()),
                vec!["DeserializerPageClassifyStandard"]
            );
            let document = FakeDocument::new(&[&["x"]]);
            assert!(classify_python(entries, FORMATS, &document).is_err());
        }
    }

    /// The adapter dedicated to an author's deserializer in a document classification: `str` or
    /// `None` in, one `Extracted::DocClass` out.
    mod doc_class_deserializer {
        use super::*;
        use pretty_assertions::assert_eq;

        fn pipe(lambda: &str) -> PyDocClassDeserializePipe {
            Python::attach(|py| {
                let func = py
                    .eval(&CString::new(lambda).unwrap(), None, None)
                    .expect("the fixture lambda");
                PyDocClassDeserializePipe::new(
                    "document_classifier[0]",
                    "document_classifier[0]::deserialize[0]",
                    func.unbind(),
                )
            })
        }

        fn block(content: &str) -> TextBlock {
            TextBlock::from_content(BlockType::DOC_CLASS, BTreeMap::new(), content)
        }

        #[test]
        fn a_complete_name_is_one_doc_class() {
            let out = pipe("lambda block: 'AMUNDI-EN24'")
                .deserialize(&block(""))
                .unwrap();
            assert_eq!(out, vec![Extracted::DocClass(Some(p("AMUNDI-EN24")))]);
        }

        #[test]
        fn a_partial_name_is_one_doc_class() {
            let out = pipe("lambda block: 'MEDIOLANUM-IT24.*'")
                .deserialize(&block(""))
                .unwrap();
            assert_eq!(out, vec![Extracted::DocClass(Some(p("MEDIOLANUM-IT24.*")))]);
        }

        /// As the standard deserializer: one explicit "nothing", not an empty result.
        #[test]
        fn none_is_one_empty_doc_class() {
            let out = pipe("lambda block: None").deserialize(&block("")).unwrap();
            assert_eq!(out, vec![Extracted::DocClass(None)]);
        }

        /// The pattern is read in its canonical form, whatever the spelling (§1).
        #[test]
        fn the_string_is_read_as_a_pattern() {
            let out = pipe("lambda block: 'MEDIOLANUM-IT24@*.*'")
                .deserialize(&block(""))
                .unwrap();
            assert_eq!(
                out,
                vec![Extracted::DocClass(Some(p("MEDIOLANUM-IT24@*.*")))]
            );
        }

        #[test]
        fn the_author_receives_the_text_block() {
            let out =
                pipe("lambda block: block.content if block.type_block == 'DOC_CLASS' else None")
                    .deserialize(&block("EURIZON-EN23"))
                    .unwrap();
            assert_eq!(out, vec![Extracted::DocClass(Some(p("EURIZON-EN23")))]);
        }

        #[test]
        fn the_author_reads_the_metadata_of_the_block() {
            let block = TextBlock::from_content(
                BlockType::DOC_CLASS,
                BTreeMap::from([(
                    "format_pattern".to_string(),
                    crate::core::classes::BlockValue::from("AMUNDI"),
                )]),
                "",
            );
            let out = pipe("lambda block: block.metadata['format_pattern']")
                .deserialize(&block)
                .unwrap();
            assert_eq!(out, vec![Extracted::DocClass(Some(p("AMUNDI")))]);
        }

        /// The error says the string is not a pattern, which one, and why (the grammar's words).
        #[test]
        fn a_string_that_is_not_a_pattern_is_an_error_saying_so() {
            let why = "-IT24".parse::<FormatPattern>().unwrap_err().to_string();
            let err = pipe("lambda block: '-IT24'")
                .deserialize(&block(""))
                .unwrap_err();
            let message = err.to_string();
            assert!(message.contains("-IT24"), "{message}");
            assert!(message.contains(&why), "{message}");
            assert!(message.contains("pattern"), "{message}");
            assert_eq!(err.pipe(), "document_classifier[0]::deserialize[0]");
            assert!(!err.is_page_failure());
        }

        /// `*` as entity is the YAML's template (§1.4); from Python arrive values.
        #[test]
        fn a_template_is_not_a_pattern() {
            assert!(
                pipe("lambda block: '*-EN23'")
                    .deserialize(&block(""))
                    .is_err()
            );
        }

        #[test]
        fn an_empty_string_is_not_a_pattern() {
            assert!(pipe("lambda block: ''").deserialize(&block("")).is_err());
        }

        fn wrong_type(lambda: &str, type_name: &str) {
            let err = pipe(lambda).deserialize(&block("")).unwrap_err();
            let message = err.to_string();
            assert!(message.contains(type_name), "{message}");
            assert_eq!(err.pipe(), "document_classifier[0]::deserialize[0]");
        }

        #[test]
        fn an_integer_is_the_wrong_type() {
            wrong_type("lambda block: 3", "int");
        }

        #[test]
        fn bytes_are_the_wrong_type() {
            wrong_type("lambda block: b'AMUNDI-EN24'", "bytes");
        }

        /// One value per block: a list is not `str | None`, even a list of one string.
        #[test]
        fn a_list_is_the_wrong_type() {
            wrong_type("lambda block: ['AMUNDI-EN24']", "list");
        }

        /// The map-of-promises shape the generic adapter accepts means nothing here.
        #[test]
        fn a_mapping_is_the_wrong_type() {
            wrong_type("lambda block: {'fund-id': 'x'}", "dict");
        }

        #[test]
        fn an_exception_is_an_author_error_carrying_it() {
            let err = pipe("lambda block: 1 / 0")
                .deserialize(&block(""))
                .unwrap_err();
            assert!(matches!(err, PipeError::Author { .. }), "{err:?}");
            assert!(err.to_string().contains("division by zero"), "{err}");
            assert_eq!(err.pipe(), "document_classifier[0]::deserialize[0]");
        }

        #[test]
        fn it_keeps_its_name() {
            assert_eq!(
                pipe("lambda block: None").name(),
                "document_classifier[0]::deserialize[0]"
            );
        }

        /// Every call takes the GIL back, like the other author adapters.
        #[test]
        fn it_does_not_scale_with_threads() {
            assert!(!pipe("lambda block: None").scales_with_threads());
        }
    }

    /// The deserializer of a loaded entry is the dedicated adapter, end to end through the engine.
    mod author_deserializer_through_the_engine {
        use super::*;
        use pretty_assertions::assert_eq;

        fn entries_returning(value: &str) -> Vec<PythonEntry> {
            let dir = repo_with_file(&format!(
                "{PRELUDE}\n\
                 r = recognises('x', None)\n\
                 document_classifier = [(1, P(r.pdf_extract, r.text_filter, lambda block: {value}))]\n"
            ));
            loaded(&dir)
        }

        fn document() -> FakeDocument {
            FakeDocument::new(&[&["x"]])
        }

        #[test]
        fn a_string_classifies() {
            let outcome =
                classify_python(entries_returning("'AMUNDI-EN24'"), FORMATS, &document()).unwrap();
            assert_complete(&outcome, "AMUNDI-EN24");
        }

        #[test]
        fn none_leaves_the_document_unclassified() {
            assert_eq!(
                classify_python(entries_returning("None"), FORMATS, &document()).unwrap(),
                crate::core::doc_classify::DocClassification::Unclassified
            );
        }

        #[test]
        fn an_invalid_string_fails_the_classification_naming_the_entry() {
            let err =
                classify_python(entries_returning("'-IT24'"), FORMATS, &document()).unwrap_err();
            match &err {
                crate::core::doc_classify::DocClassifyError::Pipe {
                    entry,
                    pipeline,
                    source,
                    ..
                } => {
                    assert_eq!(*entry, 0);
                    assert_eq!(pipeline.to_string(), "document_classifier[0]");
                    assert!(source.to_string().contains("-IT24"), "{source}");
                }
                other => panic!("expected a pipe error, got {other:?}"),
            }
        }

        #[test]
        fn a_value_of_the_wrong_type_fails_the_classification() {
            let err = classify_python(entries_returning("42"), FORMATS, &document()).unwrap_err();
            assert!(
                matches!(
                    err,
                    crate::core::doc_classify::DocClassifyError::Pipe { entry: 0, .. }
                ),
                "{err:?}"
            );
        }

        /// The string is a pattern of the grammar but not a format of the repository: the
        /// engine's runtime check (§6), which needs the adapter to have read it as a pattern.
        #[test]
        fn an_unknown_complete_name_is_the_engines_unknown_format() {
            let err = classify_python(entries_returning("'AMUNDI-EN99'"), FORMATS, &document())
                .unwrap_err();
            assert!(
                matches!(
                    err,
                    crate::core::doc_classify::DocClassifyError::UnknownFormat { entry: 0, .. }
                ),
                "{err:?}"
            );
        }
    }

    /// Non-regression: the generic adapter of the extraction pipelines still refuses a string. The
    /// dedicated adapter exists so that this one need not be widened.
    mod generic_adapter_unchanged {
        use super::*;
        use crate::formats_repo::unstructured::py_pipe::PyDeserializePipe;
        use pretty_assertions::assert_eq;

        fn generic(lambda: &str) -> PyDeserializePipe {
            Python::attach(|py| {
                let func = py
                    .eval(&CString::new(lambda).unwrap(), None, None)
                    .expect("the fixture lambda");
                PyDeserializePipe::new("authored", "authored::deserialize[0]", func.unbind())
            })
        }

        fn block() -> TextBlock {
            TextBlock::from_content(BlockType::FUND, BTreeMap::new(), "Alpha Fund")
        }

        #[test]
        fn a_string_is_still_refused_by_the_generic_adapter() {
            let err = generic("lambda block: 'AMUNDI-EN24'")
                .deserialize(&block())
                .unwrap_err();
            assert!(matches!(err, PipeError::Author { .. }), "{err:?}");
        }

        #[test]
        fn a_string_from_a_format_modules_pipeline_is_still_refused() {
            use crate::core::pipeline::PipelineName;
            use crate::formats_repo::unstructured::loader::{
                UNSTRUCTURED_DIR, get_pipelines, module_name,
            };
            let dir = empty_repo();
            fs::create_dir_all(dir.path().join(UNSTRUCTURED_DIR)).unwrap();
            fs::write(
                dir.path()
                    .join(UNSTRUCTURED_DIR)
                    .join(format!("{}.py", module_name("A-EN24"))),
                format!(
                    "{PRELUDE}\nr = recognises('x', 'AMUNDI-EN24')\npipelines = {{'authored': r}}\n"
                ),
            )
            .unwrap();
            let pipelines = get_pipelines(dir.path(), "A-EN24").unwrap();
            let pipe = pipelines[&PipelineName::new("authored")]
                .deserialize
                .iter()
                .next()
                .unwrap()
                .clone();
            assert!(pipe.deserialize(&block()).is_err());
        }

        /// What it accepts is unchanged too: a map of promises, and `None` as nothing.
        #[test]
        fn a_promise_mapping_and_none_are_still_accepted() {
            assert_eq!(
                generic("lambda block: {'fund-id': block.content}")
                    .deserialize(&block())
                    .unwrap()
                    .len(),
                1
            );
            assert!(
                generic("lambda block: None")
                    .deserialize(&block())
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
