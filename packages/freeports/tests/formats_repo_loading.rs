//! Caricamento di un repo formati, end to end.
//!
//! Ogni test costruisce un repo formati **minimo ma completo** in una `TempDir` (`PLAN.md` §10:
//! niente fixture esterni) e verifica che `Algorithm::load` ne ricavi un algoritmo funzionante, o
//! che fallisca con l'errore giusto. È il focus di test che `PLAN.md` §11 assegna a M7: la fusione
//! dei tre livelli e la diagnosi dei CSV malformati.

use std::fs;
use std::path::Path;

use freeports::core::algorithm::Algorithm;
use freeports::core::page::{Document, FormatName, Page};
use freeports::formats_repo::LoadError;
use freeports::formats_utils::pdf_extract::pdf_line::PdfLine;

/// Un repo formati costruito su disco file per file.
///
/// Parte da una configurazione valida e minima, e ogni test sovrascrive solo ciò che gli
/// interessa: così un test che rompe `mapping.csv` dice, per costruzione, che è *quello* a
/// rompersi.
struct RepoBuilder {
    dir: tempfile::TempDir,
}

impl RepoBuilder {
    /// Un repo con un solo formato, `A-EN24`, e due pipeline: quella senza nome, che classifica le
    /// pagine, e `investments`, che ne estrae la tabella.
    fn minimal() -> Self {
        let builder = Self { dir: tempfile::TempDir::new().expect("temp dir") };
        builder
            .write("metadata/formats.csv", "Name,Locale,Year,Country,Version\nA,EN,24,,\n")
            .write("metadata/url_mapping.csv", "Format name,Url\n")
            .write(
                "content/orchestration/algorithms_schedule.csv",
                "Format name,Page type,Filter next iteration\nA-EN24,investments,\n",
            )
            .write("content/orchestration/mapping.csv", "ID,Page type\nA-EN24(investments),investments\n")
            .write("content/orchestration/pageclassify_overwrite.csv", "ID\n")
            .write(
                "content/algorithms/structured/page_classify/args.csv",
                "ID,Header set,Class\nA-EN24/0,\"ArialBold \"\"^Holdings$\"\"\",investments\n",
            )
            .write(
                "content/algorithms/structured/investments/args.csv",
                "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n\
                 A-EN24,ArialBold,ArialItalic,Arial,1,,,,\n",
            )
            .write(
                "content/algorithms/structured/investments/additional_args.csv",
                "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous,Interpret dash as zero\n",
            )
            .write("content/algorithms/structured/investments/partial_pipes.csv", "ID,pdf_extract,text_filter,deserialize\n")
            .write("content/algorithms/structured/investments/deselection_lists.csv", "ID,Deselection set\n")
            .write("content/algorithms/semistructured/formats_mapping.csv", "ID,pdf_extract,text_filter,deserialize\n")
            .write("content/algorithms/semistructured/args/pdf_extract.yaml", "{}")
            .write("content/algorithms/semistructured/args/text_filter.yaml", "{}")
            .write("content/algorithms/semistructured/args/deserialize.yaml", "{}");
        builder
    }

    fn write(&self, relative: &str, content: &str) -> &Self {
        let path = self.path().join(relative);
        fs::create_dir_all(path.parent().expect("a parent directory")).expect("create dirs");
        fs::write(path, content).expect("write file");
        self
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn load(&self) -> Result<Algorithm, LoadError> {
        Algorithm::load(self.path(), &FormatName::new("A-EN24"))
    }
}

/// Una pagina che il classificatore riconosce come `investments`.
fn holdings_page(number: u32) -> Page {
    Page::new(
        number,
        (300.0, 300.0),
        vec![
            PdfLine::new("ArialBold", 10.0, "Holdings", (0.0, 0.0, 60.0, 10.0)),
            PdfLine::new("ArialBold", 10.0, "Alpha Fund", (0.0, 12.0, 60.0, 22.0)),
            PdfLine::new("ArialItalic", 10.0, "Amounts in EUR", (0.0, 24.0, 60.0, 32.0)),
            PdfLine::new("Arial", 10.0, "Acme Corp", (0.0, 40.0, 40.0, 50.0)),
            PdfLine::new("Arial", 10.0, "1.000", (50.0, 40.0, 90.0, 50.0)),
        ],
        Vec::new(),
    )
}

/// Una pagina che il classificatore non riconosce.
fn other_page(number: u32) -> Page {
    Page::new(number, (300.0, 300.0), vec![PdfLine::new("Arial", 10.0, "nothing here", (0.0, 0.0, 60.0, 10.0))], Vec::new())
}

mod a_minimal_repository {
    use super::*;

    #[test]
    fn loads_into_an_algorithm() {
        assert!(RepoBuilder::minimal().load().is_ok());
    }

    #[test]
    fn the_algorithm_knows_the_format_it_was_loaded_for() {
        let algorithm = RepoBuilder::minimal().load().unwrap();
        assert_eq!(algorithm.format().as_str(), "A-EN24");
    }

    #[test]
    fn it_classifies_the_page_its_headers_describe() {
        let algorithm = RepoBuilder::minimal().load().unwrap();
        let document = Document::new("doc", "A-EN24", vec![holdings_page(1), other_page(2)]);
        let classes = algorithm.classify_pages(&document).unwrap();
        assert_eq!(classes[0].as_ref().map(|c| c.as_str()), Some("investments"));
        assert_eq!(classes[1], None);
    }

    #[test]
    fn it_runs_the_whole_chain_on_a_document() {
        let algorithm = RepoBuilder::minimal().load().unwrap();
        let document = Document::new("doc", "A-EN24", vec![holdings_page(1)]);
        assert!(algorithm.apply(&document, &[]).is_ok());
    }
}

mod the_three_levels_merge {
    use super::*;

    /// Un modulo d'autore che aggiunge un pipe `deserialize` alla pipeline `investments`.
    const AUTHOR_MODULE: &str = r#"
class _Pipeline:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def extra_deserialize(block):
    return [{"authored": block["content"]}]

pipelines = {"investments": _Pipeline(None, None, extra_deserialize)}
"#;

    #[test]
    fn an_authored_segment_is_added_to_the_structured_pipeline_of_the_same_name() {
        let repo = RepoBuilder::minimal();
        repo.write("content/algorithms/unstructured/a_en24.py", AUTHOR_MODULE);
        let pipelines = freeports::formats_repo::load_pipelines(repo.path(), "A-EN24", false).unwrap();
        let investments = &pipelines[&freeports::core::pipeline::PipelineName::new("investments")];
        // Due deserializer structured + uno d'autore.
        assert_eq!(investments.deserialize.len(), 3);
    }

    #[test]
    fn the_merged_pipeline_still_loads_into_an_algorithm() {
        let repo = RepoBuilder::minimal();
        repo.write("content/algorithms/unstructured/a_en24.py", AUTHOR_MODULE);
        assert!(repo.load().is_ok());
    }

    #[test]
    fn a_semistructured_row_adds_its_pipes_too() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/semistructured/formats_mapping.csv",
            "ID,pdf_extract,text_filter,deserialize\nA-EN24(investments),standard_cost_curr,,\n",
        );
        repo.write(
            "content/algorithms/semistructured/args/pdf_extract.yaml",
            "A-EN24(investments):\n  body_set:\n    font: Arial\n  subfund_set:\n    font: ArialBold\n  currency: EUR\n",
        );
        let pipelines = freeports::formats_repo::load_pipelines(repo.path(), "A-EN24", false).unwrap();
        let investments = &pipelines[&freeports::core::pipeline::PipelineName::new("investments")];
        // Tre pipe structured + tre semistructured.
        assert_eq!(investments.pdf_extract.len(), 6);
    }

    #[test]
    fn a_format_using_only_the_unstructured_level_still_loads() {
        let repo = RepoBuilder::minimal();
        // Si toglie la pipeline structured `investments` e la si rimpiazza con una d'autore.
        repo.write(
            "content/algorithms/structured/investments/args.csv",
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n",
        );
        repo.write(
            "content/algorithms/unstructured/a_en24.py",
            r#"
class _Pipeline:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def extract(page):
    return [{"type_block": "RELEVANT_BLOCK", "metadata": {}, "content": "x"}]

def filter_blocks(blocks, companies):
    return [{"type_block": "FUND", "metadata": {}, "content": "x"}]

def deserialize_block(block):
    return [{"authored": block["content"]}]

pipelines = {"investments": _Pipeline(extract, filter_blocks, deserialize_block)}
"#,
        );
        assert!(repo.load().is_ok());
    }
}

mod configuration_errors {
    use super::*;

    #[test]
    fn an_unknown_format_is_reported_as_such() {
        let repo = RepoBuilder::minimal();
        let err = Algorithm::load(repo.path(), &FormatName::new("GHOST-EN24")).unwrap_err();
        assert!(matches!(err, LoadError::UnknownFormat { .. }), "{err}");
    }

    #[test]
    fn an_incomplete_pipeline_names_the_missing_segments() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/partial_pipes.csv",
            "ID,pdf_extract,text_filter,deserialize\nA-EN24,TRUE,TRUE,FALSE\n",
        );
        let err = repo.load().unwrap_err();
        let LoadError::IncompletePipeline { pipeline, missing } = err else {
            panic!("expected IncompletePipeline, got {err}")
        };
        assert_eq!((pipeline.as_str(), missing.as_str()), ("investments", "deserialize"));
    }

    #[test]
    fn a_page_class_in_the_schedule_but_not_in_the_mapping_is_rejected() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/orchestration/algorithms_schedule.csv",
            "Format name,Page type,Filter next iteration\nA-EN24,investments,\nA-EN24,ghosts,\n",
        );
        let err = repo.load().unwrap_err();
        assert!(matches!(err, LoadError::Algorithm(_)), "{err}");
    }

    #[test]
    fn a_pipeline_that_nothing_maps_is_rejected() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/args.csv",
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n\
             A-EN24,ArialBold,ArialItalic,Arial,1,,,,\n\
             A-EN24(orphan),ArialBold,ArialItalic,Arial,1,,,,\n",
        );
        let err = repo.load().unwrap_err();
        assert!(matches!(err, LoadError::Algorithm(_)), "{err}");
    }

    #[test]
    fn a_malformed_structured_csv_reports_the_offending_line() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/args.csv",
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n\
             A-EN24,ArialBold,ArialItalic,Arial,1,,,,\n\
             B-EN24,,,,not-a-number,,,,\n",
        );
        let err = repo.load().unwrap_err();
        assert!(err.to_string().contains("line 2"), "{err}");
    }

    #[test]
    fn a_malformed_line_selection_names_its_column() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/args.csv",
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n\
             A-EN24,ArialBold,ArialItalic,\"Arial ???\",1,,,,\n",
        );
        let err = repo.load().unwrap_err();
        assert!(err.to_string().contains("Body set"), "{err}");
    }

    /// The `Interpret dash as zero` column, from a whole repository rather than a hand-built
    /// config: a repository that declares it loads, and one that misspells a flag does not.
    #[test]
    fn a_repository_declaring_dash_as_zero_loads() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/additional_args.csv",
            "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous,Interpret dash as zero\n\
             A-EN24,,,,,,,MARKET_VALUE | PERC_NET_ASSETS\n",
        );
        assert!(repo.load().is_ok(), "a repository declaring the column must load");
    }

    #[test]
    fn a_misspelled_dash_as_zero_flag_is_rejected_naming_the_flag() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/additional_args.csv",
            "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous,Interpret dash as zero\n\
             A-EN24,,,,,,,MARKET_VALEU\n",
        );
        let err = repo.load().unwrap_err();
        assert!(err.to_string().contains("MARKET_VALEU"), "{err}");
    }

    /// The column configures `text_filter`, so a repository that switches that segment off and
    /// still fills the column in is contradicting itself.
    #[test]
    fn a_disabled_text_filter_carrying_dash_as_zero_is_rejected() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/additional_args.csv",
            "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous,Interpret dash as zero\n\
             A-EN24,,,,,,,MARKET_VALUE\n",
        );
        repo.write(
            "content/algorithms/structured/investments/partial_pipes.csv",
            "ID,pdf_extract,text_filter,deserialize\n\
             A-EN24,,FALSE,\n",
        );
        assert!(repo.load().is_err(), "a disabled text_filter may not carry the column");
    }

    #[test]
    fn an_orchestration_row_naming_an_unknown_format_is_rejected() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/orchestration/mapping.csv",
            "ID,Page type\nA-EN24(investments),investments\nGHOST-EN24(x),ghosts\n",
        );
        let err = repo.load().unwrap_err();
        assert!(err.to_string().contains("GHOST-EN24"), "{err}");
    }

    #[test]
    fn a_missing_metadata_file_is_reported_with_its_path() {
        let repo = RepoBuilder::minimal();
        fs::remove_file(repo.path().join("metadata/formats.csv")).unwrap();
        let err = repo.load().unwrap_err();
        assert!(matches!(err, LoadError::Metadata(_)), "{err}");
        assert!(err.to_string().contains("formats.csv"), "{err}");
    }

    #[test]
    fn a_missing_semistructured_args_file_is_reported_even_if_unused() {
        let repo = RepoBuilder::minimal();
        fs::remove_file(repo.path().join("content/algorithms/semistructured/args/deserialize.yaml")).unwrap();
        let err = repo.load().unwrap_err();
        assert!(matches!(err, LoadError::Semistructured(_)), "{err}");
    }

    #[test]
    fn an_author_module_that_does_not_import_is_reported_with_its_path() {
        let repo = RepoBuilder::minimal();
        repo.write("content/algorithms/unstructured/a_en24.py", "raise RuntimeError('boom')\n");
        let err = repo.load().unwrap_err();
        assert!(matches!(err, LoadError::Unstructured(_)), "{err}");
        assert!(err.to_string().contains("boom"), "{err}");
    }
}

mod partial_loading {
    use super::*;

    #[test]
    fn an_incomplete_pipeline_is_allowed_when_partial_loading_is_requested() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/partial_pipes.csv",
            "ID,pdf_extract,text_filter,deserialize\nA-EN24,TRUE,TRUE,FALSE\n",
        );
        let pipelines = freeports::formats_repo::load_pipelines(repo.path(), "A-EN24", true).unwrap();
        assert!(!pipelines[&freeports::core::pipeline::PipelineName::new("investments")].is_complete());
    }

    #[test]
    fn the_same_repository_fails_when_partial_loading_is_not_requested() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/partial_pipes.csv",
            "ID,pdf_extract,text_filter,deserialize\nA-EN24,TRUE,TRUE,FALSE\n",
        );
        assert!(freeports::formats_repo::load_pipelines(repo.path(), "A-EN24", false).is_err());
    }
}

mod format_detection {
    use super::*;

    #[test]
    fn a_url_is_matched_against_the_declared_formats() {
        let repo = RepoBuilder::minimal();
        repo.write("metadata/url_mapping.csv", "Format name,Url\nA-EN24,https://a.example/\n");
        let known = freeports::formats_repo::metadata::get_formats(repo.path()).unwrap();
        let found = freeports::formats_repo::metadata::url_to_format(repo.path(), &known, "https://a.example/r.pdf").unwrap();
        assert_eq!(found.as_deref(), Some("A-EN24"));
    }

    #[test]
    fn an_unknown_url_matches_no_format() {
        let repo = RepoBuilder::minimal();
        let known = freeports::formats_repo::metadata::get_formats(repo.path()).unwrap();
        assert_eq!(freeports::formats_repo::metadata::url_to_format(repo.path(), &known, "https://x/").unwrap(), None);
    }
}
