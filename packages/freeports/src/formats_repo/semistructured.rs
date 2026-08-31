//! The semistructured level: **named** algorithms, native or written by the format's author.
//!
//! It sits between the other two. Like structured, the algorithm can live in the library; like
//! unstructured, it takes a rich configuration instead of a handful of columns, and it may be
//! written by the author. What is distinctive is that the algorithm has a **name**: a mapping table
//! says which name serves which segment of which pipeline, and a YAML file per segment carries its
//! configuration.
//!
//! - [`formats_mapping`] reads the mapping table;
//! - [`args`] reads and resolves the YAML;
//! - [`native`] is the registry of algorithms implemented in Rust;
//! - a non-native name is looked up in the author's module for that segment.
//!
//! # A name defined both natively and by the author is an error
//!
//! Not a precedence to be guessed. The check runs over *every* native name of the segment as soon
//! as the author's module is loaded, not only over the name the row in question asks for. The
//! alternative — letting one of them win — makes invisible a collision that is nearly always a
//! typo.

pub mod args;
pub mod formats_mapping;
pub mod native;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::core::pipeline::{Pipeline, PipelineName};

use super::unstructured::py_pipe::{PyDeserializePipe, PyPdfExtractPipe, PyTextFilterPipe};
use args::ArgsError;
use formats_mapping::{FormatsMappingError, MappingRow};
use native::NativeError;

/// The three segments of a pipeline, which here are also three file names and three columns of the
/// mapping table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegmentKind {
    PdfExtract,
    TextFilter,
    Deserialize,
}

impl SegmentKind {
    /// The three segments, in the order a pipeline applies them.
    pub const ALL: [SegmentKind; 3] = [SegmentKind::PdfExtract, SegmentKind::TextFilter, SegmentKind::Deserialize];

    /// The name the segment goes by in the repository's files.
    pub fn file_stem(self) -> &'static str {
        match self {
            SegmentKind::PdfExtract => "pdf_extract",
            SegmentKind::TextFilter => "text_filter",
            SegmentKind::Deserialize => "deserialize",
        }
    }

    /// The cell of the mapping row concerning this segment.
    fn cell(self, row: &MappingRow) -> Option<&str> {
        match self {
            SegmentKind::PdfExtract => row.pdf_extract.as_deref(),
            SegmentKind::TextFilter => row.text_filter.as_deref(),
            SegmentKind::Deserialize => row.deserialize.as_deref(),
        }
    }
}

/// Failures of loading the semistructured level.
#[derive(Debug, thiserror::Error)]
pub enum SemistructuredError {
    #[error(transparent)]
    Mapping(#[from] FormatsMappingError),
    #[error("segment '{segment}': {source}")]
    Args {
        segment: &'static str,
        #[source]
        source: ArgsError,
    },
    #[error("segment '{segment}', algorithm '{name}': {source}")]
    Native {
        segment: &'static str,
        name: String,
        #[source]
        source: NativeError,
    },
    #[error("segment '{segment}', algorithm '{name}': the args are not a mapping the algorithm can read: {message}")]
    MalformedArgs { segment: &'static str, name: String, message: String },
    #[error("segment '{segment}': no native or author-provided algorithm named '{name}' is registered")]
    UnknownAlgorithm { segment: &'static str, name: String },
    /// A name defined both natively and by the author. See the module documentation.
    #[error("segment '{segment}': '{name}' is defined both natively and by the author's module")]
    AmbiguousAlgorithm { segment: &'static str, name: String },
    #[error("segment '{segment}': author-provided '{name}' is not callable")]
    AuthorAlgorithmNotCallable { segment: &'static str, name: String },
    #[error("segment '{segment}': author-provided '{name}' has no matching '{expected_class}' class")]
    AuthorAlgorithmMissingInputClass { segment: &'static str, name: String, expected_class: String },
    #[error("failed to load the author module for segment '{segment}' at {path}: {message}")]
    AuthorModuleLoad { segment: &'static str, path: PathBuf, message: String },
    #[error("segment '{segment}': {message}")]
    Python { segment: &'static str, message: String },
}

/// The path of a segment's author-written module.
pub fn local_extension_path(formats_repo_dir: &Path, segment: SegmentKind) -> PathBuf {
    formats_repo_dir
        .join(formats_mapping::SEMISTRUCTURED_DIR)
        .join("local_extensions")
        .join(format!("{}.py", segment.file_stem()))
}

/// The name of the input class an author must provide alongside a function: `standard_cost_curr`
/// becomes `InputStandardCostCurr`.
pub fn input_class_name(name: &str) -> String {
    let mut out = String::from("Input");
    for part in name.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// Where the algorithm a mapping cell asks for comes from.
enum AlgorithmSource {
    Native,
    Author { func: Py<PyAny>, input_class: Py<PyAny> },
}

/// Imports a segment's author module, if it exists.
fn load_author_module<'py>(
    py: Python<'py>,
    formats_repo_dir: &Path,
    segment: SegmentKind,
) -> Result<Option<Bound<'py, PyAny>>, SemistructuredError> {
    let path = local_extension_path(formats_repo_dir, segment);
    if !path.is_file() {
        return Ok(None);
    }
    let load = || -> PyResult<Bound<'py, PyAny>> {
        let util = py.import("importlib.util")?;
        let runtime_name = format!("_local_extensions_{}", segment.file_stem());
        let spec = util.call_method1("spec_from_file_location", (&runtime_name, path.to_string_lossy().as_ref()))?;
        let module = util.call_method1("module_from_spec", (&spec,))?;
        py.import("sys")?.getattr("modules")?.set_item(&runtime_name, &module)?;
        spec.getattr("loader")?.call_method1("exec_module", (&module,))?;
        Ok(module)
    };
    match load() {
        Ok(module) => {
            tracing::debug!(segment = segment.file_stem(), "loaded the author's local extension module");
            Ok(Some(module))
        }
        Err(error) => {
            let message = error.to_string();
            tracing::error!(segment = segment.file_stem(), "failed to load local extension: {message}");
            error.print(py);
            Err(SemistructuredError::AuthorModuleLoad { segment: segment.file_stem(), path, message })
        }
    }
}

/// Resolves an algorithm name, checking for a native/author collision.
fn resolve(
    py: Python<'_>,
    formats_repo_dir: &Path,
    segment: SegmentKind,
    name: &str,
) -> Result<AlgorithmSource, SemistructuredError> {
    let Some(module) = load_author_module(py, formats_repo_dir, segment)? else {
        return if native::contains(segment, name) {
            Ok(AlgorithmSource::Native)
        } else {
            Err(SemistructuredError::UnknownAlgorithm { segment: segment.file_stem(), name: name.to_string() })
        };
    };

    // The ambiguity check runs over **every** native name of the segment, not only the one
    // requested: a collision is a configuration error regardless of who uses it.
    for native_name in native::names(segment) {
        if module.hasattr(*native_name).unwrap_or(false) {
            return Err(SemistructuredError::AmbiguousAlgorithm {
                segment: segment.file_stem(),
                name: (*native_name).to_string(),
            });
        }
    }

    if native::contains(segment, name) {
        return Ok(AlgorithmSource::Native);
    }

    let Ok(func) = module.getattr(name) else {
        return Err(SemistructuredError::UnknownAlgorithm { segment: segment.file_stem(), name: name.to_string() });
    };
    if !func.is_callable() {
        return Err(SemistructuredError::AuthorAlgorithmNotCallable {
            segment: segment.file_stem(),
            name: name.to_string(),
        });
    }
    let expected_class = input_class_name(name);
    let input_class = module.getattr(expected_class.as_str()).map_err(|_| {
        SemistructuredError::AuthorAlgorithmMissingInputClass {
            segment: segment.file_stem(),
            name: name.to_string(),
            expected_class,
        }
    })?;
    Ok(AlgorithmSource::Author { func: func.unbind(), input_class: input_class.unbind() })
}

/// The pipes an algorithm produces for a segment, already in the form the engine uses.
enum SegmentPipes {
    PdfExtract(Vec<Arc<dyn crate::core::pipeline::PdfExtractPipe>>),
    TextFilter(Vec<Arc<dyn crate::core::pipeline::TextFilterPipe>>),
    Deserialize(Vec<Arc<dyn crate::core::pipeline::DeserializePipe>>),
}

impl SegmentPipes {
    fn len(&self) -> usize {
        match self {
            SegmentPipes::PdfExtract(pipes) => pipes.len(),
            SegmentPipes::TextFilter(pipes) => pipes.len(),
            SegmentPipes::Deserialize(pipes) => pipes.len(),
        }
    }

    /// Pours the pipes into the right segment of the pipeline.
    fn push_into(self, pipeline: &mut Pipeline) {
        match self {
            SegmentPipes::PdfExtract(pipes) => pipes.into_iter().for_each(|p| {
                pipeline.pdf_extract.push(p);
            }),
            SegmentPipes::TextFilter(pipes) => pipes.into_iter().for_each(|p| {
                pipeline.text_filter.push(p);
            }),
            SegmentPipes::Deserialize(pipes) => pipes.into_iter().for_each(|p| {
                pipeline.deserialize.push(p);
            }),
        }
    }
}

/// Builds the pipes of a **native** algorithm from its YAML arguments.
fn build_native(
    segment: SegmentKind,
    name: &str,
    arg: &serde_yaml::Value,
) -> Result<SegmentPipes, SemistructuredError> {
    match (segment, name) {
        (SegmentKind::PdfExtract, "standard_cost_curr") => {
            let input: native::InputStandardCostCurr =
                serde_yaml::from_value(arg.clone()).map_err(|e| SemistructuredError::MalformedArgs {
                    segment: segment.file_stem(),
                    name: name.to_string(),
                    message: e.to_string(),
                })?;
            let (investments, fund, currency) = native::standard_cost_curr(&input).map_err(|source| {
                SemistructuredError::Native { segment: segment.file_stem(), name: name.to_string(), source }
            })?;
            Ok(SegmentPipes::PdfExtract(vec![Arc::new(investments), Arc::new(fund), Arc::new(currency)]))
        }
        // `resolve` cannot produce a native source for any other pair: the registry is its only
        // source.
        _ => unreachable!("native::contains only registers (PdfExtract, \"standard_cost_curr\")"),
    }
}

/// Converts a YAML value into its Python equivalent, to build an author's input object.
fn yaml_to_py<'py>(py: Python<'py>, value: &serde_yaml::Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match value {
        serde_yaml::Value::Null => py.None().into_bound(py),
        serde_yaml::Value::Bool(v) => v.into_pyobject(py)?.to_owned().into_any(),
        serde_yaml::Value::Number(n) => match n.as_i64() {
            Some(i) => i.into_pyobject(py)?.into_any(),
            None => n.as_f64().unwrap_or(0.0).into_pyobject(py)?.into_any(),
        },
        serde_yaml::Value::String(s) => s.into_pyobject(py)?.into_any(),
        serde_yaml::Value::Sequence(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(yaml_to_py(py, item)?)?;
            }
            list.into_any()
        }
        serde_yaml::Value::Mapping(map) => {
            let dict = PyDict::new(py);
            for (key, item) in map {
                dict.set_item(yaml_to_py(py, key)?, yaml_to_py(py, item)?)?;
            }
            dict.into_any()
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_py(py, &tagged.value)?,
    })
}

/// Builds the pipes of an **author's** algorithm: call the function with its input object, then
/// wrap whatever comes out.
fn build_author(
    py: Python<'_>,
    segment: SegmentKind,
    name: &str,
    pipeline_name: &str,
    func: &Py<PyAny>,
    input_class: &Py<PyAny>,
    arg: &serde_yaml::Value,
) -> Result<SegmentPipes, SemistructuredError> {
    let python_error = |message: String| SemistructuredError::Python { segment: segment.file_stem(), message };

    let call = || -> PyResult<Vec<Py<PyAny>>> {
        let py_arg = yaml_to_py(py, arg)?;
        let kwargs = py_arg.cast_into::<PyDict>().map_err(PyErr::from)?;
        let instance = input_class.bind(py).call((), Some(&kwargs))?;
        let result = func.bind(py).call1((instance,))?;
        if result.is_callable() {
            return Ok(vec![result.unbind()]);
        }
        let mut out = Vec::new();
        for item in result.try_iter()? {
            out.push(item?.unbind());
        }
        Ok(out)
    };
    let callables = call().map_err(|e| {
        let message = e.to_string();
        tracing::error!(segment = segment.file_stem(), algorithm = name, "author algorithm raised: {message}");
        e.print(py);
        python_error(message)
    })?;

    let pipe_name = |i: usize| format!("{pipeline_name}::{name}[{i}]");
    Ok(match segment {
        SegmentKind::PdfExtract => SegmentPipes::PdfExtract(
            callables
                .into_iter()
                .enumerate()
                .map(|(i, f)| {
                    Arc::new(PyPdfExtractPipe::new(pipeline_name, pipe_name(i), f))
                        as Arc<dyn crate::core::pipeline::PdfExtractPipe>
                })
                .collect(),
        ),
        SegmentKind::TextFilter => SegmentPipes::TextFilter(
            callables
                .into_iter()
                .enumerate()
                .map(|(i, f)| {
                    Arc::new(PyTextFilterPipe::new(pipeline_name, pipe_name(i), f))
                        as Arc<dyn crate::core::pipeline::TextFilterPipe>
                })
                .collect(),
        ),
        SegmentKind::Deserialize => SegmentPipes::Deserialize(
            callables
                .into_iter()
                .enumerate()
                .map(|(i, f)| {
                    Arc::new(PyDeserializePipe::new(pipeline_name, pipe_name(i), f))
                        as Arc<dyn crate::core::pipeline::DeserializePipe>
                })
                .collect(),
        ),
    })
}

/// The semistructured pipelines the repository defines for `format_name`.
///
/// A format absent from the mapping table defines none — and the YAML files are read anyway,
/// because a missing one is an error in the repository rather than in the format.
pub fn get_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<HashMap<PipelineName, Pipeline>, SemistructuredError> {
    let rows = formats_mapping::rows_for_format(formats_repo_dir, format_name)?;

    let mut args_by_segment = HashMap::new();
    for segment in SegmentKind::ALL {
        let loaded = args::load(formats_repo_dir, segment)
            .map_err(|source| SemistructuredError::Args { segment: segment.file_stem(), source })?;
        args_by_segment.insert(segment, loaded);
    }

    let mut pipelines: HashMap<PipelineName, Pipeline> = HashMap::new();
    // How many pipes each `(pipeline, segment)` pair has already emitted: the counter that drives
    // the positional choice among list-valued arguments (see [`args`]).
    let mut emitted: HashMap<(String, SegmentKind), usize> = HashMap::new();

    Python::attach(|py| -> Result<(), SemistructuredError> {
        for row in &rows {
            for segment in SegmentKind::ALL {
                let Some(name) = segment.cell(row) else { continue };
                let args = &args_by_segment[&segment];
                let selected = args::lookup(args, format_name, &row.pipeline_name)
                    .map_err(|source| SemistructuredError::Args { segment: segment.file_stem(), source })?;
                let algorithm_id = args::algorithm_id(format_name, &row.pipeline_name);
                let already = *emitted.get(&(row.pipeline_name.clone(), segment)).unwrap_or(&0);
                let arg = args::positional(selected, already, &algorithm_id)
                    .map_err(|source| SemistructuredError::Args { segment: segment.file_stem(), source })?;

                let pipes = match resolve(py, formats_repo_dir, segment, name)? {
                    AlgorithmSource::Native => build_native(segment, name, arg)?,
                    AlgorithmSource::Author { func, input_class } => {
                        build_author(py, segment, name, &row.pipeline_name, &func, &input_class, arg)?
                    }
                };
                tracing::debug!(
                    pipeline = row.pipeline_name.as_str(),
                    segment = segment.file_stem(),
                    algorithm = name,
                    pipe_count = pipes.len(),
                    "resolved a semistructured algorithm"
                );
                *emitted.entry((row.pipeline_name.clone(), segment)).or_insert(0) += pipes.len();

                let pipeline_name = PipelineName::new(&row.pipeline_name);
                let pipeline = pipelines
                    .entry(pipeline_name.clone())
                    .or_insert_with(|| Pipeline::new(pipeline_name));
                pipes.push_into(pipeline);
            }
        }
        Ok(())
    })?;

    tracing::debug!(pipeline_count = pipelines.len(), "built semistructured pipelines");
    Ok(pipelines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const MAPPING_HEADER: &str = "ID,pdf_extract,text_filter,deserialize\n";

    /// The minimal YAML configuration of the one native algorithm.
    const COST_CURR_ARGS: &str = "\
A-EN24(investments):
  body_set:
    font: Tahoma
  subfund_set:
    font: Arial-BoldMT
  currency: EUR
";

    /// A formats repository with the semistructured files, all present and empty by default.
    struct Repo {
        dir: TempDir,
    }

    impl Repo {
        fn new() -> Self {
            let dir = TempDir::new().expect("temp dir");
            let base = dir.path().join(formats_mapping::SEMISTRUCTURED_DIR);
            fs::create_dir_all(base.join("args")).expect("args dir");
            let repo = Self { dir };
            repo.write("formats_mapping.csv", MAPPING_HEADER);
            repo.write("args/pdf_extract.yaml", "{}");
            repo.write("args/text_filter.yaml", "{}");
            repo.write("args/deserialize.yaml", "{}");
            repo
        }

        fn write(&self, relative: &str, content: &str) -> &Self {
            let path = self.dir.path().join(formats_mapping::SEMISTRUCTURED_DIR).join(relative);
            fs::create_dir_all(path.parent().expect("a parent")).expect("parent dir");
            fs::write(path, content).expect("write file");
            self
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }
    }

    mod segment_kind {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn each_segment_names_its_own_file() {
            assert_eq!(SegmentKind::PdfExtract.file_stem(), "pdf_extract");
            assert_eq!(SegmentKind::TextFilter.file_stem(), "text_filter");
            assert_eq!(SegmentKind::Deserialize.file_stem(), "deserialize");
        }

        #[test]
        fn all_lists_the_three_segments_in_pipeline_order() {
            assert_eq!(SegmentKind::ALL, [SegmentKind::PdfExtract, SegmentKind::TextFilter, SegmentKind::Deserialize]);
        }
    }

    mod input_class_naming {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case("standard_cost_curr", "InputStandardCostCurr"; "several parts")]
        #[test_case("simple", "InputSimple"; "one part")]
        #[test_case("a_b_c", "InputABC"; "single letter parts")]
        #[test_case("already_Capital", "InputAlreadyCapital"; "a part already capitalized")]
        fn mirrors_the_reference_naming_rule(name: &str, expected: &str) {
            assert_eq!(input_class_name(name), expected);
        }
    }

    mod args_resolution {
        use super::*;
        use pretty_assertions::assert_eq;

        fn yaml(text: &str) -> serde_yaml::Value {
            serde_yaml::from_str(text).expect("valid yaml")
        }

        #[test]
        fn looks_the_pipe_up_by_format_and_pipeline() {
            let args = yaml("A-EN24(inv): {x: 1}\n");
            assert!(args::lookup(&args, "A-EN24", "inv").is_ok());
        }

        #[test]
        fn the_bare_format_key_is_a_fallback_only_for_the_unnamed_pipeline() {
            let args = yaml("A-EN24: {x: 1}\n");
            assert!(args::lookup(&args, "A-EN24", "").is_ok());
            assert!(matches!(args::lookup(&args, "A-EN24", "inv"), Err(ArgsError::MissingArgs { .. })));
        }

        #[test]
        fn a_missing_key_names_the_algorithm_id_in_its_full_form() {
            let args = yaml("{}");
            let err = args::lookup(&args, "A-EN24", "inv").unwrap_err();
            assert!(err.to_string().contains("A-EN24(inv)"), "{err}");
        }

        #[test]
        fn a_scalar_value_is_the_argument_of_every_pipe_that_uses_it() {
            let value = yaml("{x: 1}");
            assert_eq!(args::positional(&value, 0, "id").unwrap(), &value);
            assert_eq!(args::positional(&value, 7, "id").unwrap(), &value);
        }

        #[test]
        fn a_list_value_is_indexed_by_the_number_of_pipes_already_emitted() {
            let value = yaml("[{x: 1}, {x: 2}]");
            assert_eq!(args::positional(&value, 0, "id").unwrap(), &yaml("{x: 1}"));
            assert_eq!(args::positional(&value, 1, "id").unwrap(), &yaml("{x: 2}"));
        }

        #[test]
        fn a_list_too_short_for_the_position_is_an_error_that_says_so() {
            let value = yaml("[{x: 1}]");
            let err = args::positional(&value, 3, "A-EN24(inv)").unwrap_err();
            assert!(matches!(err, ArgsError::MalformedArgs { .. }), "{err}");
            assert!(err.to_string().contains("needed index 3"), "{err}");
        }

        #[test]
        fn a_missing_yaml_file_is_an_error_even_for_an_unused_segment() {
            let repo = Repo::new();
            fs::remove_file(repo.path().join(formats_mapping::SEMISTRUCTURED_DIR).join("args/deserialize.yaml"))
                .unwrap();
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(matches!(err, SemistructuredError::Args { segment: "deserialize", .. }), "{err}");
        }
    }

    mod native_registry {
        use super::*;

        #[test]
        fn standard_cost_curr_is_the_only_native_algorithm_today() {
            assert!(native::contains(SegmentKind::PdfExtract, "standard_cost_curr"));
            assert_eq!(native::names(SegmentKind::PdfExtract).len(), 1);
            assert!(native::names(SegmentKind::TextFilter).is_empty());
            assert!(native::names(SegmentKind::Deserialize).is_empty());
        }

        #[test]
        fn a_name_of_another_segment_is_not_native_here() {
            assert!(!native::contains(SegmentKind::TextFilter, "standard_cost_curr"));
        }
    }

    mod building_pipelines {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_format_with_no_rows_defines_no_pipeline() {
            assert!(get_pipelines(Repo::new().path(), "A-EN24").unwrap().is_empty());
        }

        #[test]
        fn standard_cost_curr_contributes_three_pdf_extract_pipes() {
            let repo = Repo::new();
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\n"));
            repo.write("args/pdf_extract.yaml", COST_CURR_ARGS);
            let pipelines = get_pipelines(repo.path(), "A-EN24").unwrap();
            let pipeline = &pipelines[&PipelineName::new("investments")];
            assert_eq!(pipeline.pdf_extract.len(), 3);
            assert!(pipeline.text_filter.is_empty());
        }

        #[test]
        fn the_pipeline_is_named_after_the_row_it_comes_from() {
            let repo = Repo::new();
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\n"));
            repo.write("args/pdf_extract.yaml", COST_CURR_ARGS);
            let pipelines = get_pipelines(repo.path(), "A-EN24").unwrap();
            assert_eq!(pipelines.keys().collect::<Vec<_>>(), vec![&PipelineName::new("investments")]);
        }

        #[test]
        fn a_second_row_of_the_same_pipeline_needs_its_own_entry_in_a_stacked_args_list() {
            let repo = Repo::new();
            repo.write(
                "formats_mapping.csv",
                &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\nA-EN24(investments),standard_cost_curr,,\n"),
            );
            // A single scalar argument: both rows share it, so the pipes come to six.
            repo.write("args/pdf_extract.yaml", COST_CURR_ARGS);
            let pipelines = get_pipelines(repo.path(), "A-EN24").unwrap();
            assert_eq!(pipelines[&PipelineName::new("investments")].pdf_extract.len(), 6);
        }

        #[test]
        fn a_stacked_args_list_is_indexed_by_emitted_pipes_not_by_row() {
            // The algorithm emits three pipes, so the second row looks for index 3 rather than 1.
            let repo = Repo::new();
            repo.write(
                "formats_mapping.csv",
                &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\nA-EN24(investments),standard_cost_curr,,\n"),
            );
            repo.write(
                "args/pdf_extract.yaml",
                "A-EN24(investments):\n  - {body_set: {font: A}, subfund_set: {font: B}, currency: EUR}\n",
            );
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(err.to_string().contains("needed index 3"), "{err}");
        }

        #[test]
        fn a_missing_args_entry_for_a_requested_algorithm_is_an_error() {
            let repo = Repo::new();
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\n"));
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(matches!(err, SemistructuredError::Args { segment: "pdf_extract", .. }), "{err}");
        }

        #[test]
        fn malformed_args_for_a_native_algorithm_name_the_algorithm() {
            let repo = Repo::new();
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\n"));
            repo.write("args/pdf_extract.yaml", "A-EN24(investments):\n  body_set: {font: A}\n");
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(matches!(&err, SemistructuredError::MalformedArgs { name, .. } if name == "standard_cost_curr"), "{err}");
        }

        #[test]
        fn an_unknown_algorithm_name_is_rejected() {
            let repo = Repo::new();
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),nope,,\n"));
            repo.write("args/pdf_extract.yaml", "A-EN24(investments): {}\n");
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(matches!(&err, SemistructuredError::UnknownAlgorithm { name, .. } if name == "nope"), "{err}");
        }

        #[test]
        fn an_unknown_currency_in_the_args_is_reported_as_malformed_args() {
            let repo = Repo::new();
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\n"));
            repo.write(
                "args/pdf_extract.yaml",
                "A-EN24(investments):\n  body_set: {font: A}\n  subfund_set: {font: B}\n  currency: XYZ\n",
            );
            assert!(matches!(
                get_pipelines(repo.path(), "A-EN24"),
                Err(SemistructuredError::MalformedArgs { .. })
            ));
        }

        #[test]
        fn the_algorithm_flags_of_the_args_reach_the_pipe() {
            let repo = Repo::new();
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\n"));
            repo.write("args/pdf_extract.yaml", &format!("{COST_CURR_ARGS}  algorithm_flags: USE_RULER_AREA\n"));
            assert!(get_pipelines(repo.path(), "A-EN24").is_ok());
        }

        #[test]
        fn an_unknown_algorithm_flag_is_rejected_naming_the_field() {
            let repo = Repo::new();
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\n"));
            repo.write("args/pdf_extract.yaml", &format!("{COST_CURR_ARGS}  algorithm_flags: NOPE\n"));
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(matches!(err, SemistructuredError::Native { .. }), "{err}");
        }
    }

    /// The tests that attach the interpreter. The synthetic author modules deliberately do not
    /// import the crate's own Python package.
    mod python_boundary {
        use super::*;
        use pretty_assertions::assert_eq;

        const AUTHOR_MODULE: &str = r#"
class InputMyAlgo:
    def __init__(self, threshold=0):
        self.threshold = threshold

def my_algo(arg):
    def pipe(page):
        return [{"type_block": "RELEVANT_BLOCK", "metadata": {"t": arg.threshold}, "content": "x"}]
    return pipe
"#;

        fn repo_with_author(module: &str) -> Repo {
            let repo = Repo::new();
            repo.write("local_extensions/pdf_extract.py", module);
            repo
        }

        #[test]
        fn an_author_algorithm_is_resolved_and_wrapped() {
            let repo = repo_with_author(AUTHOR_MODULE);
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),my_algo,,\n"));
            repo.write("args/pdf_extract.yaml", "A-EN24(investments):\n  threshold: 5\n");
            let pipelines = get_pipelines(repo.path(), "A-EN24").unwrap();
            assert_eq!(pipelines[&PipelineName::new("investments")].pdf_extract.len(), 1);
        }

        #[test]
        fn an_author_algorithm_returning_several_pipes_contributes_all_of_them() {
            let module = format!("{AUTHOR_MODULE}\ndef pair(arg):\n    return [my_algo(arg), my_algo(arg)]\n\nInputPair = InputMyAlgo\n");
            let repo = repo_with_author(&module);
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),pair,,\n"));
            repo.write("args/pdf_extract.yaml", "A-EN24(investments):\n  threshold: 5\n");
            let pipelines = get_pipelines(repo.path(), "A-EN24").unwrap();
            assert_eq!(pipelines[&PipelineName::new("investments")].pdf_extract.len(), 2);
        }

        #[test]
        fn an_author_algorithm_without_its_input_class_is_rejected() {
            let module = "def lonely(arg):\n    return lambda page: []\n";
            let repo = repo_with_author(module);
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),lonely,,\n"));
            repo.write("args/pdf_extract.yaml", "A-EN24(investments): {}\n");
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            let SemistructuredError::AuthorAlgorithmMissingInputClass { expected_class, .. } = err else {
                panic!("expected AuthorAlgorithmMissingInputClass")
            };
            assert_eq!(expected_class, "InputLonely");
        }

        #[test]
        fn a_non_callable_author_attribute_is_rejected() {
            let repo = repo_with_author("not_a_func = 1\nInputNotAFunc = dict\n");
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),not_a_func,,\n"));
            repo.write("args/pdf_extract.yaml", "A-EN24(investments): {}\n");
            assert!(matches!(
                get_pipelines(repo.path(), "A-EN24"),
                Err(SemistructuredError::AuthorAlgorithmNotCallable { .. })
            ));
        }

        #[test]
        fn a_name_defined_both_natively_and_by_the_author_is_a_hard_error() {
            let repo = repo_with_author("def standard_cost_curr(arg):\n    return lambda page: []\n");
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),standard_cost_curr,,\n"));
            repo.write("args/pdf_extract.yaml", COST_CURR_ARGS);
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(matches!(&err, SemistructuredError::AmbiguousAlgorithm { name, .. } if name == "standard_cost_curr"), "{err}");
        }

        #[test]
        fn the_collision_check_fires_even_when_another_algorithm_is_the_one_requested() {
            let module = format!("{AUTHOR_MODULE}\ndef standard_cost_curr(arg):\n    return lambda page: []\n");
            let repo = repo_with_author(&module);
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),my_algo,,\n"));
            repo.write("args/pdf_extract.yaml", "A-EN24(investments):\n  threshold: 5\n");
            assert!(matches!(
                get_pipelines(repo.path(), "A-EN24"),
                Err(SemistructuredError::AmbiguousAlgorithm { .. })
            ));
        }

        #[test]
        fn an_author_module_that_fails_to_import_reports_its_path() {
            let repo = repo_with_author("raise RuntimeError('boom')\n");
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),whatever,,\n"));
            repo.write("args/pdf_extract.yaml", "A-EN24(investments): {}\n");
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(matches!(err, SemistructuredError::AuthorModuleLoad { .. }), "{err}");
        }

        #[test]
        fn an_author_pipe_actually_runs_with_the_configured_arguments() {
            use crate::core::classes::{BlockValue, PdfBlock};
            use crate::core::page::Page;
            use crate::formats_utils::pdf_extract::pdf_line::PdfLine;

            let repo = repo_with_author(AUTHOR_MODULE);
            repo.write("formats_mapping.csv", &format!("{MAPPING_HEADER}A-EN24(investments),my_algo,,\n"));
            repo.write("args/pdf_extract.yaml", "A-EN24(investments):\n  threshold: 5\n");
            let pipelines = get_pipelines(repo.path(), "A-EN24").unwrap();
            let pipeline = &pipelines[&PipelineName::new("investments")];

            Python::attach(|py| {
                let raw = py
                    .eval(pyo3::ffi::c_str!("{'blocks': []}"), None, None)
                    .expect("page dict")
                    .unbind();
                let page = Page::new(1, (10.0, 10.0), vec![PdfLine::new("A", 10.0, "x", (0.0, 0.0, 1.0, 1.0))], Vec::new())
                    .with_raw(raw);
                let blocks: Vec<PdfBlock> = pipeline.pdf_extract.iter().next().unwrap().extract(&page).unwrap();
                assert_eq!(blocks[0].metadata.get("t"), Some(&BlockValue::from(5i64)));
            });
        }
    }
}
