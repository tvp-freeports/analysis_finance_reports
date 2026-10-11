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
        let builder = Self {
            dir: tempfile::TempDir::new().expect("temp dir"),
        };
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
                "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero\n",
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
            PdfLine::new(
                "ArialItalic",
                10.0,
                "Amounts in EUR",
                (0.0, 24.0, 60.0, 32.0),
            ),
            PdfLine::new("Arial", 10.0, "Acme Corp", (0.0, 40.0, 40.0, 50.0)),
            PdfLine::new("Arial", 10.0, "1.000", (50.0, 40.0, 90.0, 50.0)),
        ],
        Vec::new(),
    )
}

/// Una pagina che il classificatore non riconosce.
fn other_page(number: u32) -> Page {
    Page::new(
        number,
        (300.0, 300.0),
        vec![PdfLine::new(
            "Arial",
            10.0,
            "nothing here",
            (0.0, 0.0, 60.0, 10.0),
        )],
        Vec::new(),
    )
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
        let pipelines =
            freeports::formats_repo::load_pipelines(repo.path(), "A-EN24", false).unwrap();
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
            "ID,pdf_extract,text_filter,deserialize\nA-EN24(investments),investments,,\n",
        );
        repo.write(
            "content/algorithms/semistructured/args/pdf_extract.yaml",
            "A-EN24(investments):\n  body_set:\n    font: Arial\n  subfund_set:\n    font: ArialBold\n  currency: EUR\n",
        );
        let pipelines =
            freeports::formats_repo::load_pipelines(repo.path(), "A-EN24", false).unwrap();
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
        assert_eq!(
            (pipeline.as_str(), missing.as_str()),
            ("investments", "deserialize")
        );
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
            "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero\n\
             A-EN24,,,,,,,MARKET_VALUE | PERC_NET_ASSETS\n",
        );
        assert!(
            repo.load().is_ok(),
            "a repository declaring the column must load"
        );
    }

    #[test]
    fn a_misspelled_dash_as_zero_flag_is_rejected_naming_the_flag() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/additional_args.csv",
            "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero\n\
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
            "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero\n\
             A-EN24,,,,,,,MARKET_VALUE\n",
        );
        repo.write(
            "content/algorithms/structured/investments/partial_pipes.csv",
            "ID,pdf_extract,text_filter,deserialize\n\
             A-EN24,,FALSE,\n",
        );
        assert!(
            repo.load().is_err(),
            "a disabled text_filter may not carry the column"
        );
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
        fs::remove_file(
            repo.path()
                .join("content/algorithms/semistructured/args/deserialize.yaml"),
        )
        .unwrap();
        let err = repo.load().unwrap_err();
        assert!(matches!(err, LoadError::Semistructured(_)), "{err}");
    }

    #[test]
    fn an_author_module_that_does_not_import_is_reported_with_its_path() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/unstructured/a_en24.py",
            "raise RuntimeError('boom')\n",
        );
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
        let pipelines =
            freeports::formats_repo::load_pipelines(repo.path(), "A-EN24", true).unwrap();
        assert!(
            !pipelines[&freeports::core::pipeline::PipelineName::new("investments")].is_complete()
        );
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
        repo.write(
            "metadata/url_mapping.csv",
            "Format name,Url\nA-EN24,https://a.example/\n",
        );
        let known = freeports::formats_repo::metadata::get_formats(repo.path()).unwrap();
        let found = freeports::formats_repo::metadata::url_to_format(
            repo.path(),
            &known,
            "https://a.example/r.pdf",
        )
        .unwrap();
        assert_eq!(
            found.map(|pattern| pattern.to_string()).as_deref(),
            Some("A-EN24")
        );
    }

    #[test]
    fn an_unknown_url_matches_no_format() {
        let repo = RepoBuilder::minimal();
        let known = freeports::formats_repo::metadata::get_formats(repo.path()).unwrap();
        assert_eq!(
            freeports::formats_repo::metadata::url_to_format(repo.path(), &known, "https://x/")
                .unwrap(),
            None
        );
    }
}

/// `A8`: ciò che un algoritmo d'autore scrive nel registro del motore.
///
/// I test unitari di `python::log` verificano il livello, i campi e la riga del `.log.csv` con
/// span costruiti a mano. Questi verificano l'unica cosa che quelli non possono: che il giro
/// completo — modulo Python caricato dal repo, chiamato dal motore, dentro gli span veri di
/// `Algorithm::apply` — arrivi davvero nel file, **senza che il chiamante passi niente**.
mod the_author_s_log {
    use super::*;

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{Layer, Registry};

    use freeports::core::tracing_setup::{CsvLogLayer, csv_event_filter};

    /// Un `text_filter` d'autore che scrive nel registro mentre interpreta la pagina.
    ///
    /// È il `text_filter` e non il `pdf_extract` perché un `pdf_extract` d'autore riceve il
    /// dizionario PyMuPDF originale, che una pagina costruita a mano non ha: il segmento giusto per
    /// questa prova è quello che una pagina sintetica attraversa davvero.
    const LOGGING_MODULE: &str = r#"
from freeports.log import warn, warn_imposed, debug

class _Pipeline:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def filter_blocks(blocks, companies):
    warn_imposed(field="currency", value="EUR", reason="constant of the format")
    warn("la terza colonna non ha intestazione", coord_1="col 3")
    debug("questo non deve arrivare nel file")
    return []

pipelines = {"investments": _Pipeline(None, filter_blocks, None)}
"#;

    /// Le righe di dati del `.log.csv` scritto mentre l'algoritmo attraversa un documento.
    fn rows_of_a_run() -> Vec<Vec<String>> {
        let repo = RepoBuilder::minimal();
        repo.write("content/algorithms/unstructured/a_en24.py", LOGGING_MODULE);
        let algorithm = repo.load().expect("the repository loads");
        let document = Document::new("A fund report", "A-EN24", vec![holdings_page(7)]);

        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join(".log.csv");
        let csv = CsvLogLayer::create(&path).expect("csv layer construction");
        let subscriber = Registry::default().with(csv.clone().with_filter(csv_event_filter()));
        tracing::subscriber::with_default(subscriber, || {
            algorithm.apply(&document, &[]).expect("the algorithm runs");
        });
        csv.close().expect("the csv layer closes");

        let content = fs::read_to_string(&path).expect("read .log.csv");
        content
            .lines()
            .skip(1)
            .map(|line| {
                csv::ReaderBuilder::new()
                    .has_headers(false)
                    .from_reader(line.as_bytes())
                    .records()
                    .next()
                    .expect("one record per line")
                    .expect("a well-formed record")
                    .iter()
                    .map(str::to_string)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn what_an_algorithm_writes_reaches_the_log_file() {
        let messages: Vec<String> = rows_of_a_run()
            .into_iter()
            .map(|row| row[8].clone())
            .collect();
        assert!(
            messages
                .iter()
                .any(|m| m == "la terza colonna non ha intestazione"),
            "the free-text warning is missing, found {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|m| m == "currency was not read: imposed as \"EUR\" - constant of the format"),
            "the imposed-value warning is missing, found {messages:?}"
        );
    }

    #[test]
    fn the_report_and_the_page_are_filled_in_by_the_engine() {
        let rows = rows_of_a_run();
        let row = rows
            .iter()
            .find(|row| row[8] == "la terza colonna non ha intestazione")
            .expect("the warning the author wrote");
        assert_eq!(row[0], "A fund report", "Report");
        assert_eq!(row[1], "7", "Page");
        assert!(
            row[2].contains("page[7]"),
            "Activity says where it happened, found {:?}",
            row[2]
        );
        assert_eq!(row[5], "col 3", "First coord");
    }

    #[test]
    fn the_activity_names_the_pipe_the_author_wrote() {
        let rows = rows_of_a_run();
        let activity = rows
            .iter()
            .find(|row| row[8].starts_with("currency was not read"))
            .map(|row| row[2].clone())
            .expect("the imposed-value warning");
        // Non una stringa esatta: l'annidamento degli span è del motore, e questo test riguarda il
        // fatto che il tubo d'autore ci compaia, non la forma del percorso.
        assert!(activity.contains("text_filter"), "found {activity:?}");
        assert!(
            activity.contains("pipeline[investments]"),
            "found {activity:?}"
        );
    }

    #[test]
    fn a_debug_written_by_an_algorithm_does_not_reach_the_log_file() {
        let messages: Vec<String> = rows_of_a_run()
            .into_iter()
            .map(|row| row[8].clone())
            .collect();
        assert!(
            !messages
                .iter()
                .any(|m| m.contains("questo non deve arrivare")),
            "the file's threshold is warn, found {messages:?}"
        );
    }
}

/// `A1` attraverso la superficie Python: che un formato possa nominare il tubo nuovo e che il
/// blocco che ne esce arrivi al segmento seguente.
///
/// I test di unità di `labelled_value.rs` provano la geometria; questo prova la strada — le
/// parole chiave, le due enumerazioni e il passaggio del blocco — che un test di unità in Rust
/// non attraversa.
mod the_labelled_value_pipe {
    use super::*;

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{Layer, Registry};

    use freeports::core::tracing_setup::{CsvLogLayer, csv_event_filter};

    /// Due tubi `A1` sulla stessa pagina: uno legge il valore **sotto** l'etichetta, l'altro quello
    /// **a destra**. Il `text_filter` d'autore riscrive nel registro ciò che ha ricevuto, che è il
    /// modo in cui questo test legge l'uscita del primo segmento.
    const LABELLED_VALUE_MODULE: &str = r#"
from freeports.log import warn
from freeports.standard_funcs.pdf_extract import PdfExtractLabelledValueStandard
from freeports.utils.pdf_extract import PdfLineSelection, LabelledValueWhere

class _Pipeline:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

below = PdfExtractLabelledValueStandard(
    PdfLineSelection.text("^Alpha Fund$"),
    "RELEVANT_BLOCK",
    "the value below",
    where=LabelledValueWhere.BELOW,
)

right = PdfExtractLabelledValueStandard(
    PdfLineSelection.text("^Acme Corp$"),
    "RELEVANT_BLOCK",
    "the value to the right",
    where=LabelledValueWhere.RIGHT,
)

def filter_blocks(blocks, companies):
    for blk in blocks:
        warn("A1 read " + str(blk.content), coord_1="a1")
    return []

def deserialize_nothing(txt_blk):
    return None

pipelines = {"investments": _Pipeline((below, right), filter_blocks, deserialize_nothing)}
"#;

    /// I messaggi che l'algoritmo d'autore ha scritto attraversando un documento.
    fn messages_of_a_run(module: &str) -> Vec<String> {
        let repo = RepoBuilder::minimal();
        // Il livello strutturato esce di scena: la `investments` di questo repo è **solo** quella
        // d'autore, altrimenti il `text_filter` standard riceverebbe blocchi che non sono righe di
        // tabella e farebbe fallire la pagina prima che il tubo in prova dica la sua.
        repo.write(
            "content/algorithms/structured/investments/args.csv",
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n",
        );
        repo.write("content/algorithms/unstructured/a_en24.py", module);
        let algorithm = repo.load().expect("the repository loads");
        let document = Document::new("A fund report", "A-EN24", vec![holdings_page(7)]);

        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join(".log.csv");
        let csv = CsvLogLayer::create(&path).expect("csv layer construction");
        let subscriber = Registry::default().with(csv.clone().with_filter(csv_event_filter()));
        tracing::subscriber::with_default(subscriber, || {
            algorithm.apply(&document, &[]).expect("the algorithm runs");
        });
        csv.close().expect("the csv layer closes");

        let content = fs::read_to_string(&path).expect("read .log.csv");
        content
            .lines()
            .skip(1)
            .filter_map(|line| {
                csv::ReaderBuilder::new()
                    .has_headers(false)
                    .from_reader(line.as_bytes())
                    .records()
                    .next()?
                    .ok()
                    .map(|record| record.iter().map(str::to_string).collect::<Vec<_>>()[8].clone())
            })
            .collect()
    }

    #[test]
    fn a_format_can_name_it_and_the_block_reaches_the_next_segment() {
        let messages = messages_of_a_run(LABELLED_VALUE_MODULE);
        assert!(
            messages.iter().any(|m| m == "A1 read Amounts in EUR"),
            "the value below its label did not come through, found {messages:?}"
        );
    }

    #[test]
    fn the_placement_to_the_right_works_through_the_same_surface() {
        let messages = messages_of_a_run(LABELLED_VALUE_MODULE);
        assert!(
            messages.iter().any(|m| m == "A1 read 1.000"),
            "the value beside its label did not come through, found {messages:?}"
        );
    }

    /// Il difetto è il comportamento di sempre: la riga dell'etichetta stessa, intera.
    #[test]
    fn the_default_placement_emits_the_label_s_own_line() {
        const ON_LINE: &str = r#"
from freeports.log import warn
from freeports.standard_funcs.pdf_extract import PdfExtractLabelledValueStandard
from freeports.utils.pdf_extract import PdfLineSelection

class _Pipeline:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def filter_blocks(blocks, companies):
    for blk in blocks:
        warn("A1 read " + str(blk.content), coord_1="a1")
    return []

def deserialize_nothing(txt_blk):
    return None

pipelines = {"investments": _Pipeline(
    PdfExtractLabelledValueStandard(PdfLineSelection.text("Amounts in"), "RELEVANT_BLOCK", "on line"),
    filter_blocks,
    deserialize_nothing,
)}
"#;
        let messages = messages_of_a_run(ON_LINE);
        assert!(
            messages.iter().any(|m| m == "A1 read Amounts in EUR"),
            "found {messages:?}"
        );
    }

    /// Le due maniere di dire dove finisce la sezione sono alternative, e chiederle insieme è un
    /// errore di cablaggio: il repo non deve caricarsi in silenzio.
    #[test]
    fn stop_and_stop_at_label_family_are_refused_together() {
        const BOTH: &str = r#"
from freeports.standard_funcs.pdf_extract import PdfExtractLabelledValueStandard
from freeports.utils.pdf_extract import PdfLineSelection

pipelines = {"investments": PdfExtractLabelledValueStandard(
    PdfLineSelection.text("^Alpha Fund$"),
    "RELEVANT_BLOCK",
    "both",
    stop=PdfLineSelection.text("^Acme"),
    stop_at_label_family=True,
)}
"#;
        let repo = RepoBuilder::minimal();
        repo.write("content/algorithms/unstructured/a_en24.py", BOTH);
        assert!(
            repo.load().is_err(),
            "a pipe asked for two stops should not load"
        );
    }
}

/// `A2` visto dal repo dei formati: le parole chiave, il pavimento e le coordinate di tabella.
///
/// I test di unità di `section.rs` provano la geometria su pagine misurate; questo prova la strada —
/// come un modulo d'autore costruisce il tubo e che cosa arriva al segmento seguente.
mod the_section_pipe {
    use super::*;

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{Layer, Registry};

    use freeports::core::tracing_setup::{CsvLogLayer, csv_event_filter};

    /// Un modulo d'autore con un solo tubo `A2`, costruito con gli argomenti dati.
    fn module_with(arguments: &str) -> String {
        format!(
            r#"
from freeports.log import warn
from freeports.standard_funcs.pdf_extract import PdfExtractSectionStandard
from freeports.utils.pdf_extract import PdfLineSelection, TableConfig

class _Pipeline:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def filter_blocks(blocks, companies):
    for blk in blocks:
        warn("A2 read " + str(blk.content) + str(sorted(blk.metadata.items())), coord_1="a2")
    return []

def deserialize_nothing(txt_blk):
    return None

pipelines = {{"investments": _Pipeline(
    PdfExtractSectionStandard({arguments}),
    filter_blocks,
    deserialize_nothing,
)}}
"#
        )
    }

    fn messages_of(arguments: &str) -> Vec<String> {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/args.csv",
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n",
        );
        repo.write(
            "content/algorithms/unstructured/a_en24.py",
            &module_with(arguments),
        );
        let algorithm = repo.load().expect("the repository loads");
        let document = Document::new("A fund report", "A-EN24", vec![holdings_page(7)]);

        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join(".log.csv");
        let csv = CsvLogLayer::create(&path).expect("csv layer construction");
        let subscriber = Registry::default().with(csv.clone().with_filter(csv_event_filter()));
        tracing::subscriber::with_default(subscriber, || {
            algorithm.apply(&document, &[]).expect("the algorithm runs");
        });
        csv.close().expect("the csv layer closes");

        let content = fs::read_to_string(&path).expect("read .log.csv");
        content
            .lines()
            .skip(1)
            .filter_map(|line| {
                csv::ReaderBuilder::new()
                    .has_headers(false)
                    .from_reader(line.as_bytes())
                    .records()
                    .next()?
                    .ok()
                    .map(|record| record.iter().map(str::to_string).collect::<Vec<_>>()[8].clone())
            })
            .collect()
    }

    const HEADING: &str = r#"PdfLineSelection.text("^Alpha Fund$"), "RELEVANT_BLOCK", "a section""#;

    #[test]
    fn a_format_can_name_it_and_the_section_reaches_the_next_segment() {
        let messages = messages_of(HEADING);
        assert_eq!(
            messages,
            vec![
                "A2 read Amounts in EUR[]".to_string(),
                "A2 read Acme Corp[]".to_string(),
                "A2 read 1.000[]".to_string(),
            ]
        );
    }

    #[test]
    fn a_family_given_as_a_selection_closes_the_section() {
        let messages = messages_of(&format!(
            r#"{HEADING}, family=PdfLineSelection.text("^Acme")"#
        ));
        assert_eq!(messages, vec!["A2 read Amounts in EUR[]".to_string()]);
    }

    /// `fineco_en23_ir/inv_managers.py` chiude la sua tabella a `y1=520.0`: un'ordinata nuda, senza
    /// nessuna intestazione sotto.
    #[test]
    fn a_bare_ordinate_is_accepted_as_the_bottom() {
        let messages = messages_of(&format!("{HEADING}, bottom=30.0"));
        assert_eq!(messages, vec!["A2 read Amounts in EUR[]".to_string()]);
    }

    #[test]
    fn the_heading_comes_along_when_asked_for() {
        let messages = messages_of(&format!("{HEADING}, include_heading=True"));
        assert_eq!(
            messages.first().map(String::as_str),
            Some("A2 read Alpha Fund[]")
        );
    }

    #[test]
    fn the_body_narrows_the_section_to_the_lines_that_count() {
        let messages = messages_of(&format!(
            r#"{HEADING}, body=PdfLineSelection.font("arial")"#
        ));
        assert_eq!(
            messages,
            vec![
                "A2 read Acme Corp[]".to_string(),
                "A2 read 1.000[]".to_string()
            ]
        );
    }

    #[test]
    fn rows_puts_the_table_coordinates_in_the_metadata() {
        let messages = messages_of(&format!("{HEADING}, rows=True"));
        assert!(
            messages
                .iter()
                .all(|m| m.contains("table-row") && m.contains("table-col")),
            "found {messages:?}"
        );
    }

    #[test]
    fn rows_refuses_something_that_is_neither_true_nor_a_table_config() {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/unstructured/a_en24.py",
            &module_with(&format!(r#"{HEADING}, rows="yes""#)),
        );
        assert!(
            repo.load().is_err(),
            "a pipe given a nonsense `rows` should not load"
        );
    }
}

/// Una pagina di prospetto: il fondo, la valuta e i tre totali con il numero alla loro destra.
///
/// `Holdings` c'è perché è la riga che il classificatore del repo di prova cerca; il resto è la
/// forma che `A4` legge — tre etichette e un numero per ciascuna, più il netto che sta **una riga
/// sotto** la sua etichetta, che è il caso per cui la finestra di una colonna si sposta.
fn assets_page(number: u32) -> Page {
    Page::new(
        number,
        (300.0, 300.0),
        vec![
            PdfLine::new("ArialBold", 10.0, "Holdings", (0.0, 0.0, 60.0, 10.0)),
            PdfLine::new("ArialBold", 10.0, "Alpha Fund", (0.0, 12.0, 60.0, 22.0)),
            PdfLine::new("ArialItalic", 10.0, "in EUR", (0.0, 24.0, 40.0, 32.0)),
            PdfLine::new("ArialBold", 10.0, "Total assets", (0.0, 40.0, 60.0, 50.0)),
            PdfLine::new("Arial", 10.0, "1.500", (100.0, 40.0, 140.0, 50.0)),
            PdfLine::new(
                "ArialBold",
                10.0,
                "Total liabilities",
                (0.0, 60.0, 60.0, 70.0),
            ),
            PdfLine::new("Arial", 10.0, "500", (100.0, 60.0, 140.0, 70.0)),
            PdfLine::new(
                "ArialBold",
                10.0,
                "Total net assets",
                (0.0, 80.0, 60.0, 90.0),
            ),
            PdfLine::new("Arial", 10.0, "1.000", (100.0, 95.0, 140.0, 105.0)),
        ],
        Vec::new(),
    )
}

/// `A4`, la pipe degli attivi, dal lato in cui un formato la nomina.
///
/// La superficie Python prende **una colonna per argomento** invece di tre parole chiave sciolte
/// per colonna: un'ancora senza la sua finestra non vuol dire niente, e tenerle separate lasciava
/// passare un formato che dava il vettore a una colonna e il moltiplicatore a un'altra.
mod the_assets_pipe {
    use super::*;

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{Layer, Registry};

    use freeports::core::tracing_setup::{CsvLogLayer, csv_event_filter};

    /// Un modulo d'autore con un solo tubo `A4`, costruito con gli argomenti dati.
    fn module_with(arguments: &str) -> String {
        format!(
            r#"
from freeports.log import warn
from freeports.standard_funcs.pdf_extract import AssetsColumn, PdfExtractAssetsStandard
from freeports.utils.pdf_extract import PdfLineSelection

class _Pipeline:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def filter_blocks(blocks, companies):
    for blk in blocks:
        warn("A4 read " + str(sorted(blk.metadata.items())), coord_1="a4")
    return []

def deserialize_nothing(txt_blk):
    return None

pipelines = {{"investments": _Pipeline(
    PdfExtractAssetsStandard({arguments}),
    filter_blocks,
    deserialize_nothing,
)}}
"#
        )
    }

    fn repo_with(arguments: &str) -> RepoBuilder {
        let repo = RepoBuilder::minimal();
        repo.write(
            "content/algorithms/structured/investments/args.csv",
            "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n",
        );
        repo.write(
            "content/algorithms/unstructured/a_en24.py",
            &module_with(arguments),
        );
        repo
    }

    fn messages_of(arguments: &str) -> Vec<String> {
        let repo = repo_with(arguments);
        let algorithm = repo.load().expect("the repository loads");
        let document = Document::new("A fund report", "A-EN24", vec![assets_page(7)]);

        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join(".log.csv");
        let csv = CsvLogLayer::create(&path).expect("csv layer construction");
        let subscriber = Registry::default().with(csv.clone().with_filter(csv_event_filter()));
        tracing::subscriber::with_default(subscriber, || {
            algorithm.apply(&document, &[]).expect("the algorithm runs");
        });
        csv.close().expect("the csv layer closes");

        let content = fs::read_to_string(&path).expect("read .log.csv");
        content
            .lines()
            .skip(1)
            .filter_map(|line| {
                csv::ReaderBuilder::new()
                    .has_headers(false)
                    .from_reader(line.as_bytes())
                    .records()
                    .next()?
                    .ok()
                    .map(|record| record.iter().map(str::to_string).collect::<Vec<_>>()[8].clone())
            })
            .collect()
    }

    const HEADS: &str = r#"
        fund_set=PdfLineSelection.text("^Alpha Fund$"),
        currency_set=PdfLineSelection.text("^in EUR$"),
        tot_assets=AssetsColumn(PdfLineSelection.text("^Total assets$")),
        liabilities=AssetsColumn(PdfLineSelection.text("^Total liabilities$")),
    "#;

    #[test]
    fn a_format_can_name_it_and_the_three_totals_reach_the_next_segment() {
        let messages = messages_of(&format!(
            r#"{HEADS}
            net_assets=AssetsColumn(
                PdfLineSelection.text("^Total net assets$"), vec=(1.2, 1.5), mult=(100.0, 1.3)
            ),"#
        ));

        assert_eq!(
            messages,
            vec![
                "A4 read [('currency', 'in EUR'), ('date', None), ('fund', 'Alpha Fund'), \
                 ('liabilities', '500'), ('net_assets', '1.000'), ('tot_assets', '1.500')]"
                    .to_string()
            ]
        );
    }

    /// Il difetto della finestra è «sulla riga dell'etichetta, alla sua destra»: il netto sta una
    /// riga sotto, e senza spostare la finestra la sua colonna non prende niente — e la pipe lo
    /// **dice**, nominando la colonna, invece di lasciare la riga sparire.
    #[test]
    fn a_column_whose_window_reaches_nothing_says_which_one_it_is() {
        let messages = messages_of(&format!(
            r#"{HEADS} net_assets=AssetsColumn(PdfLineSelection.text("^Total net assets$")),"#
        ));

        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(
            messages[0].contains(r#"assets column "net_assets" has 0 entries"#),
            "{messages:?}"
        );
    }

    /// Una colonna è un tipo, non tre parole chiave: passarle una selezione nuda è un errore di
    /// cablaggio, e si vede al caricamento invece che a metà corsa.
    #[test]
    fn a_bare_selection_where_a_column_belongs_is_refused() {
        let repo = repo_with(&format!(
            r#"{HEADS} net_assets=PdfLineSelection.text("^Total net assets$"),"#
        ));
        let error = repo.load().expect_err("a bare selection is not a column");

        assert!(
            format!("{error}").contains("net_assets must be an AssetsColumn"),
            "{error}"
        );
    }

    /// I candidati della data passano il confine come lista, e l'ordine è quello della lista —
    /// non quello della pagina, che è il punto per cui la lista esiste (`R5`).
    #[test]
    fn the_date_candidates_are_tried_in_the_order_the_format_writes_them() {
        let messages = messages_of(&format!(
            r#"{HEADS}
            net_assets=AssetsColumn(PdfLineSelection.text("^Total net assets$"), vec=(1.2, 1.5)),
            date=[PdfLineSelection.text("^nowhere on this page$"), PdfLineSelection.text("^Holdings$")],"#
        ));

        assert_eq!(messages.len(), 1, "{messages:?}");
        assert!(messages[0].contains("('date', 'Holdings')"), "{messages:?}");
    }

    /// Un solo candidato si scrive senza lista: è la forma che tre formati usano già.
    #[test]
    fn a_single_candidate_needs_no_list() {
        let messages = messages_of(&format!(
            r#"{HEADS}
            net_assets=AssetsColumn(PdfLineSelection.text("^Total net assets$"), vec=(1.2, 1.5)),
            date=PdfLineSelection.text("^Holdings$"),"#
        ));

        assert!(messages[0].contains("('date', 'Holdings')"), "{messages:?}");
    }

    /// `R25` e la risposta **B**: candidati che non prendono niente lasciano un avviso e la riga,
    /// non la pagina.
    #[test]
    fn candidates_that_take_nothing_leave_a_warning_and_the_row() {
        let messages = messages_of(&format!(
            r#"{HEADS}
            net_assets=AssetsColumn(PdfLineSelection.text("^Total net assets$"), vec=(1.2, 1.5)),
            date=PdfLineSelection.text("^nowhere on this page$"),"#
        ));

        assert!(
            messages
                .iter()
                .any(|m| m.contains("the date of this statement is declared")),
            "{messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("('date', None)")),
            "{messages:?}"
        );
    }

    /// Le due parole chiave dell'appaiamento si nominano dal formato, e su una pagina a un comparto
    /// solo danno lo stesso esito dell'indice.
    #[test]
    fn a_format_can_ask_for_abscissa_pairing_and_name_a_column_to_drop() {
        let messages = messages_of(&format!(
            r#"{HEADS}
            net_assets=AssetsColumn(PdfLineSelection.text("^Total net assets$"), vec=(1.2, 1.5)),
            pair_by_abscissa=True,
            drop=PdfLineSelection.text("^30/06/2023$"),"#
        ));

        assert_eq!(
            messages,
            vec![
                "A4 read [('currency', 'in EUR'), ('date', None), ('fund', 'Alpha Fund'), \
                 ('liabilities', '500'), ('net_assets', '1.000'), ('tot_assets', '1.500')]"
                    .to_string()
            ]
        );
    }

    /// La valuta è obbligatoria fuori dal caso tabellare, e il costruttore lo dice caricando.
    #[test]
    fn a_missing_currency_selection_is_refused_when_the_page_is_not_a_table() {
        let repo = repo_with(
            r#"
            fund_set=PdfLineSelection.text("^Alpha Fund$"),
            currency_set=None,
            tot_assets=AssetsColumn(PdfLineSelection.text("^Total assets$")),
            liabilities=AssetsColumn(PdfLineSelection.text("^Total liabilities$")),
            net_assets=AssetsColumn(PdfLineSelection.text("^Total net assets$")),
            "#,
        );
        let error = repo.load().expect_err("no currency, no non-tabular branch");

        assert!(format!("{error}").contains("currency"), "{error}");
    }
}
