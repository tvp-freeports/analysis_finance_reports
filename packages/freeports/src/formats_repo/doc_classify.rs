//! Assembling a formats repository's document classification: `formats.csv`, the author's Python
//! (`metadata/document_classify.py` or the package) and the declarative YAML
//! (`metadata/document_classify.yaml`) into one ready [`DocumentClassifier`].
//!
//! The one entry point the callers use (the `classify` subcommand and the run, phases 10–11 of
//! `agent-memory/PLAN-document-classification.md`). Each source keeps its own loader —
//! [`super::metadata::get_formats`], [`super::doc_classify_py::load`],
//! [`super::doc_classify_yaml::load`] — and its own error; this module only puts them together. The
//! precedence between the two (Python first; YAML after, from the Python's partial if any; not at
//! all after a complete one, §6 of the specification) belongs to the engine,
//! [`DocumentClassifier::classify`].
//!
//! Neither source is required: a repository with neither has a classifier that classifies nothing.

use std::path::Path;

use crate::core::doc_classify::DocumentClassifier;

use super::doc_classify_py::{self, DocClassifyPyError};
use super::doc_classify_yaml::{self, DocClassifyYamlError};
use super::metadata::{MetadataError, get_formats};

/// Why a repository's document classification could not be loaded: the error of the source that
/// failed, which names its file.
#[derive(Debug, thiserror::Error)]
pub enum DocClassifyLoadError {
    #[error(transparent)]
    Metadata(#[from] MetadataError),
    #[error(transparent)]
    Python(#[from] DocClassifyPyError),
    #[error(transparent)]
    Yaml(#[from] DocClassifyYamlError),
}

/// The document classifier of the repository at `formats_repo_dir`: its formats in `formats.csv`
/// order, its Python entries, its YAML entries.
///
/// Both sources are loaded whatever the other holds: the precedence is an order at run time, and a
/// broken YAML beside a Python that happens to decide every document is still broken.
pub fn load(formats_repo_dir: &Path) -> Result<DocumentClassifier, DocClassifyLoadError> {
    let formats = get_formats(formats_repo_dir)?;
    let python = doc_classify_py::load(formats_repo_dir)?;
    let yaml = doc_classify_yaml::load(formats_repo_dir)?;
    tracing::debug!(
        format_count = formats.len(),
        python_entry_count = python.len(),
        yaml_entry_count = yaml.len(),
        "loaded the document classification"
    );
    Ok(DocumentClassifier::new(formats, python, yaml))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::core::doc_classify::{DecidedBy, DocClassification, DocClassifyError};
    use crate::formats_repo::doc_classify_py::test_support::FakeDocument;
    use crate::formats_repo::format_pattern::FormatPattern;
    use crate::formats_repo::metadata::METADATA_DIR;
    use crate::formats_repo::unstructured::loader::TEMPLATES_DIR;

    // ------------------------------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------------------------------

    /// `formats.csv` of the test repository: two entities, one with a variant pair, so that a
    /// partial has more than one candidate.
    const FORMATS_CSV: &str = "Name,Locale,Year,Country,Version\n\
                               AMUNDI,EN,24,,\n\
                               EURIZON,EN,23,,\n\
                               MEDIOLANUM,EN,23,,\n\
                               MEDIOLANUM,IT,24,,A\n\
                               MEDIOLANUM,IT,24,,B\n";

    /// An author module recognising one line per pipeline, importing nothing from the engine.
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

    struct Repo {
        dir: TempDir,
    }

    impl Repo {
        /// `formats.csv` only.
        fn new() -> Self {
            let dir = TempDir::new().expect("temp dir");
            fs::create_dir_all(dir.path().join(METADATA_DIR)).expect("metadata dir");
            fs::create_dir_all(dir.path().join(TEMPLATES_DIR)).expect("templates dir");
            fs::write(dir.path().join(METADATA_DIR).join("formats.csv"), FORMATS_CSV).expect("formats.csv");
            Repo { dir }
        }

        fn metadata(&self) -> PathBuf {
            self.dir.path().join(METADATA_DIR)
        }

        /// The single-file Python module, with the prelude and `body`.
        fn python(self, body: &str) -> Self {
            fs::write(self.metadata().join("document_classify.py"), format!("{PRELUDE}\n{body}\n")).unwrap();
            self
        }

        /// The package layout, with the classifier in a submodule.
        fn python_package(self, body: &str) -> Self {
            let package = self.metadata().join("document_classify");
            fs::create_dir_all(&package).unwrap();
            fs::write(package.join("__init__.py"), "from .rules import document_classifier\n").unwrap();
            fs::write(package.join("rules.py"), format!("{PRELUDE}\n{body}\n")).unwrap();
            self
        }

        fn raw_python(self, source: &str) -> Self {
            fs::write(self.metadata().join("document_classify.py"), source).unwrap();
            self
        }

        fn yaml(self, text: &str) -> Self {
            fs::write(self.metadata().join("document_classify.yaml"), text).unwrap();
            self
        }

        fn classifier(&self) -> DocumentClassifier {
            match load(self.dir.path()) {
                Ok(classifier) => classifier,
                Err(e) => panic!("expected the classification to load, got: {e}"),
            }
        }

        fn load_err(&self) -> DocClassifyLoadError {
            match load(self.dir.path()) {
                Ok(_) => panic!("expected the load to fail"),
                Err(e) => e,
            }
        }
    }

/// [`DocumentClassifier::classify`] under the engine tests' `SERIAL` lock (see it in
    /// `core::doc_classify`'s tests: the process-wide `tracing` race it closes). Never taken while
    /// attached to Python.
    trait ClassifySerialized {
        fn classify_serialized(
            &self,
            source: &dyn crate::core::doc_classify::PageSource,
        ) -> Result<DocClassification, DocClassifyError>;
    }

    impl ClassifySerialized for DocumentClassifier {
        fn classify_serialized(
            &self,
            source: &dyn crate::core::doc_classify::PageSource,
        ) -> Result<DocClassification, DocClassifyError> {
            let _guard = crate::core::doc_classify::tests::SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            self.classify(source)
        }
    }

    fn p(text: &str) -> FormatPattern {
        text.parse().expect("fixture pattern must be valid")
    }

    fn complete_format(outcome: &DocClassification) -> &FormatPattern {
        match outcome {
            DocClassification::Complete(complete) => &complete.format,
            other => panic!("expected a complete outcome, got {other:?}"),
        }
    }

    fn decided_by(outcome: &DocClassification) -> &DecidedBy {
        match outcome {
            DocClassification::Complete(complete) => &complete.decided_by,
            other => panic!("expected a complete outcome, got {other:?}"),
        }
    }

    fn by_python(outcome: &DocClassification) -> bool {
        matches!(decided_by(outcome), DecidedBy::Python { .. })
    }

    fn by_yaml(outcome: &DocClassification) -> bool {
        matches!(decided_by(outcome), DecidedBy::Yaml { .. })
    }

    // ==========================================================================================
    // The tests
    // ==========================================================================================

    /// What the assembly reads, and what it needs.
    mod sources {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_repository_with_neither_source_classifies_nothing_and_reads_no_page() {
            let repo = Repo::new();
            let document = FakeDocument::new(&[&["Amundi Funds"]]);
            assert_eq!(repo.classifier().classify_serialized(&document).unwrap(), DocClassification::Unclassified);
            assert!(document.requests().is_empty(), "{:?}", document.requests());
        }

        #[test]
        fn the_yaml_alone_classifies() {
            let repo = Repo::new().yaml("1:\n  AMUNDI-EN24: {text: '^Amundi Funds$'}\n");
            let outcome = repo.classifier().classify_serialized(&FakeDocument::new(&[&["Amundi Funds"]])).unwrap();
            assert_eq!(complete_format(&outcome), &p("AMUNDI-EN24"));
            assert!(by_yaml(&outcome));
        }

        #[test]
        fn the_python_alone_classifies() {
            let repo = Repo::new().python("document_classifier = [(1, recognises('Amundi Funds', 'AMUNDI-EN24'))]");
            let outcome = repo.classifier().classify_serialized(&FakeDocument::new(&[&["Amundi Funds"]])).unwrap();
            assert_eq!(complete_format(&outcome), &p("AMUNDI-EN24"));
            assert!(by_python(&outcome));
        }

        #[test]
        fn the_python_package_alone_classifies() {
            let repo = Repo::new().python_package("document_classifier = [(-1, recognises('Eurizon Fund', 'EURIZON-EN23'))]");
            let outcome = repo.classifier().classify_serialized(&FakeDocument::new(&[&["cover"], &["Eurizon Fund"]])).unwrap();
            assert_eq!(complete_format(&outcome), &p("EURIZON-EN23"));
        }

        /// The formats come from `formats.csv`, in file order: they are the candidates of a partial.
        #[test]
        fn the_candidates_of_a_partial_are_the_formats_of_formats_csv_in_file_order() {
            let repo = Repo::new().yaml("1:\n  MEDIOLANUM: {text: '^Mediolanum$'}\n");
            match repo.classifier().classify_serialized(&FakeDocument::new(&[&["Mediolanum"]])).unwrap() {
                DocClassification::Partial { pattern, candidates } => {
                    assert_eq!(pattern, p("MEDIOLANUM"));
                    assert_eq!(candidates, vec!["MEDIOLANUM-EN23", "MEDIOLANUM-IT24.A", "MEDIOLANUM-IT24.B"]);
                }
                other => panic!("expected a partial, got {other:?}"),
            }
        }

        /// The Python's runtime check (§6) runs against the formats of `formats.csv`.
        #[test]
        fn a_python_outcome_is_checked_against_formats_csv() {
            let repo = Repo::new().python("document_classifier = [(1, recognises('x', 'FIDEURAM-IT21'))]");
            let err = repo.classifier().classify_serialized(&FakeDocument::new(&[&["x"]])).unwrap_err();
            assert!(matches!(err, DocClassifyError::UnknownFormat { entry: 0, .. }), "{err:?}");
        }
    }

    /// A broken source fails the assembly with that source's own error.
    mod errors {
        use super::*;

        #[test]
        fn a_missing_formats_csv_is_a_metadata_error() {
            let repo = Repo::new();
            fs::remove_file(repo.metadata().join("formats.csv")).unwrap();
            assert!(matches!(repo.load_err(), DocClassifyLoadError::Metadata(MetadataError::MissingCsv(_))));
        }

        #[test]
        fn a_broken_python_module_is_a_python_error_naming_its_file() {
            let repo = Repo::new().raw_python("raise RuntimeError('broken classifier')\n");
            let err = repo.load_err();
            assert!(matches!(err, DocClassifyLoadError::Python(DocClassifyPyError::ModuleLoad { .. })), "{err:?}");
            let message = err.to_string();
            assert!(message.contains("broken classifier"), "{message}");
            assert!(message.contains("document_classify.py"), "{message}");
        }

        #[test]
        fn both_python_layouts_present_is_a_python_error() {
            let repo = Repo::new().python_package("document_classifier = []").raw_python("document_classifier = []\n");
            assert!(matches!(repo.load_err(), DocClassifyLoadError::Python(DocClassifyPyError::TwoLayouts { .. })));
        }

        #[test]
        fn a_module_without_the_variable_is_a_python_error() {
            let repo = Repo::new().raw_python("x = 1\n");
            assert!(matches!(repo.load_err(), DocClassifyLoadError::Python(DocClassifyPyError::MissingClassifier { .. })));
        }

        #[test]
        fn a_broken_yaml_is_a_yaml_error_naming_its_file() {
            let repo = Repo::new().yaml("0:\n  AMUNDI-EN24: {text: x}\n");
            let err = repo.load_err();
            assert!(matches!(err, DocClassifyLoadError::Yaml(_)), "{err:?}");
            assert!(err.to_string().contains("document_classify.yaml"), "{err}");
        }

        /// A broken YAML is an error even when a valid Python would decide every document: the
        /// precedence is a run-time order, not a reason to skip loading.
        #[test]
        fn a_broken_yaml_fails_even_beside_a_valid_python() {
            let repo = Repo::new()
                .python("document_classifier = [(1, recognises('x', 'AMUNDI-EN24'))]")
                .yaml("1:\n  AMUNDI-EN24: [\n");
            assert!(matches!(repo.load_err(), DocClassifyLoadError::Yaml(_)));
        }
    }

    /// Python first; YAML after, from where the Python left (§6, [U 2026-10-10]).
    mod precedence {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The YAML on page 2 is never reached: page 2 is never converted.
        #[test]
        fn a_complete_python_outcome_means_the_yaml_is_not_touched() {
            let repo = Repo::new()
                .python("document_classifier = [(1, recognises('Amundi Funds', 'AMUNDI-EN24'))]")
                .yaml("2:\n  EURIZON-EN23: {text: '^Eurizon Fund$'}\n");
            let document = FakeDocument::new(&[&["Amundi Funds"], &["Eurizon Fund"]]);
            let outcome = repo.classifier().classify_serialized(&document).unwrap();
            assert_eq!(complete_format(&outcome), &p("AMUNDI-EN24"));
            assert!(by_python(&outcome));
            assert_eq!(document.requests(), vec![1]);
        }

        /// A YAML that would classify the same page differently does not get the chance.
        #[test]
        fn a_complete_python_outcome_wins_over_a_yaml_that_would_say_otherwise() {
            let repo = Repo::new()
                .python("document_classifier = [(1, recognises('Amundi Funds', 'AMUNDI-EN24'))]")
                .yaml("1:\n  EURIZON-EN23: {text: '^Amundi Funds$'}\n");
            let outcome = repo.classifier().classify_serialized(&FakeDocument::new(&[&["Amundi Funds"]])).unwrap();
            assert_eq!(complete_format(&outcome), &p("AMUNDI-EN24"));
            assert!(by_python(&outcome));
        }

        #[test]
        fn no_python_outcome_means_the_yaml_runs_from_nothing() {
            let repo = Repo::new()
                .python("document_classifier = [(1, recognises('Amundi Funds', 'AMUNDI-EN24'))]")
                .yaml("2:\n  EURIZON-EN23: {text: '^Eurizon Fund$'}\n");
            let document = FakeDocument::new(&[&["nothing"], &["Eurizon Fund"]]);
            let outcome = repo.classifier().classify_serialized(&document).unwrap();
            assert_eq!(complete_format(&outcome), &p("EURIZON-EN23"));
            assert!(by_yaml(&outcome));
            assert_eq!(document.requests(), vec![1, 2]);
        }

        /// The Python's partial is the YAML's starting `A`: the template `*-EN23` is instantiated
        /// with the Python's entity, which the YAML itself never found.
        #[test]
        fn a_partial_python_outcome_is_where_the_yaml_starts_from() {
            let repo = Repo::new()
                .python("document_classifier = [(1, recognises('Mediolanum', 'MEDIOLANUM'))]")
                .yaml("2:\n  \"*-EN23\": {text: '^Annual Report 2023$'}\n");
            let document = FakeDocument::new(&[&["Mediolanum"], &["Annual Report 2023"]]);
            let outcome = repo.classifier().classify_serialized(&document).unwrap();
            assert_eq!(complete_format(&outcome), &p("MEDIOLANUM-EN23"));
            assert!(by_yaml(&outcome));
        }

        /// From the Python's partial the YAML can only refine: a step of another entity is not a
        /// candidate (§3.1.1), however well it matches.
        #[test]
        fn from_a_partial_python_outcome_the_yaml_can_only_refine_it() {
            let repo = Repo::new()
                .python("document_classifier = [(1, recognises('Mediolanum', 'MEDIOLANUM'))]")
                .yaml("1:\n  EURIZON-EN23: {text: '^Mediolanum$'}\n");
            match repo.classifier().classify_serialized(&FakeDocument::new(&[&["Mediolanum"]])).unwrap() {
                DocClassification::Partial { pattern, .. } => assert_eq!(pattern, p("MEDIOLANUM")),
                other => panic!("expected the Python's partial to survive, got {other:?}"),
            }
        }

        /// The page the Python already converted is not converted again for the YAML.
        #[test]
        fn a_page_read_by_the_python_is_not_read_again_by_the_yaml() {
            let repo = Repo::new()
                .python("document_classifier = [(1, recognises('Mediolanum', 'MEDIOLANUM'))]")
                .yaml("1:\n  MEDIOLANUM-EN23: {text: '^Annual Report$'}\n");
            let document = FakeDocument::new(&[&["Mediolanum", "Annual Report"]]);
            let outcome = repo.classifier().classify_serialized(&document).unwrap();
            assert_eq!(complete_format(&outcome), &p("MEDIOLANUM-EN23"));
            assert_eq!(document.requests(), vec![1]);
        }

        #[test]
        fn a_runtime_error_of_the_python_stops_before_the_yaml() {
            let repo = Repo::new()
                .python("document_classifier = [(1, recognises('x', '-IT24'))]")
                .yaml("2:\n  EURIZON-EN23: {text: '^Eurizon Fund$'}\n");
            let document = FakeDocument::new(&[&["x"], &["Eurizon Fund"]]);
            let err = repo.classifier().classify_serialized(&document).unwrap_err();
            assert!(matches!(err, DocClassifyError::Pipe { entry: 0, .. }), "{err:?}");
            assert_eq!(document.requests(), vec![1]);
        }
    }
}
