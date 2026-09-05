//! Test d'integrazione del motore (M5), scritto **solo contro `freeports::api`**.
//!
//! `PLAN.md` §10 chiede un file per flusso in `tests/`. Questo copre il flusso completo del
//! motore: pipe definiti da fuori il crate → pipeline → bundle → schedule a due step su due
//! documenti → risultati per pagina → risoluzione delle promesse.
//!
//! Che i pipe siano definiti **qui**, e non riusati dai test unitari, è il punto: verifica che i
//! tre trait siano davvero implementabili da fuori con la sola superficie `api`, cosa che i test
//! unitari (che vedono l'albero interno) non possono dimostrare. È anche la prova che il confine
//! di `PLAN.md` §5.1 regge — "il resto del sistema non sa se un pipe è Rust o Python" — perché
//! questi pipe non sono nulla di ciò che il crate conosce.

use std::collections::BTreeMap;
use std::sync::Arc;

use freeports::api::core::{
    Algorithm, BlockType, BlockValue, Document, Extracted, FilterData, Page, PageClass,
    PageClassFinalizer, PdfBlock, PdfExtractPipe, PipeError, Pipeline, PipelineName, Promise,
    PromiseMap, Schedule, ScheduleStep, TextBlock, TextFilterPipe,
};
use freeports::api::core::{DeserializePipe, PromiseEntries};

// ---------------------------------------------------------------------------------------------
// Pipe definiti dal "consumatore", con la sola superficie pubblica
// ---------------------------------------------------------------------------------------------

/// Un blocco per riga di pagina; il tipo di blocco dipende dal prefisso del testo, così una
/// pagina può contenere sia l'intestazione (`fund:`) sia le righe di tabella (`row:`).
struct SplitLines;

impl PdfExtractPipe for SplitLines {
    fn name(&self) -> &str {
        "split-lines"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        page.lines
            .iter()
            .map(|line| {
                let text = line.text();
                let (type_block, content) = match text.split_once(':') {
                    Some(("fund", rest)) => (BlockType::FUND_NAME, rest),
                    Some(("row", rest)) => (BlockType::TABLE_BODY, rest),
                    _ => {
                        return Err(PipeError::extraction(
                            "split-lines",
                            format!("unrecognized line `{text}`"),
                        ));
                    }
                };
                Ok(PdfBlock::bare(type_block, content.trim()))
            })
            .collect()
    }
}

/// Tiene solo i blocchi del tipo dato, e li trasforma in blocchi di testo.
struct KeepType {
    name: String,
    wanted: BlockType,
}

impl KeepType {
    fn pipe(name: &str, wanted: BlockType) -> Arc<dyn TextFilterPipe> {
        Arc::new(KeepType { name: name.to_string(), wanted })
    }
}

impl TextFilterPipe for KeepType {
    fn name(&self) -> &str {
        &self.name
    }

    fn filter(
        &self,
        blocks: &[PdfBlock],
        _data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        Ok(blocks
            .iter()
            .filter(|b| b.type_block == self.wanted)
            .map(|b| TextBlock::new(BlockType::FUND, BTreeMap::new(), b.clone()))
            .collect())
    }
}

/// Tiene **esattamente un** blocco di testo per pagina, quello costruito dal primo blocco PDF.
///
/// È ciò che un filtro di classificazione deve fare: il finalizer identità pretende una
/// classificazione per pagina, quindi un filtro che scarta del tutto le pagine che non gli
/// piacciono romperebbe il conto.
struct FirstBlockOnly;

impl TextFilterPipe for FirstBlockOnly {
    fn name(&self) -> &str {
        "first-block-only"
    }

    fn filter(
        &self,
        blocks: &[PdfBlock],
        _data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        let first = blocks
            .first()
            .ok_or_else(|| PipeError::extraction("first-block-only", "page has no blocks"))?;
        Ok(vec![TextBlock::new(BlockType::PAGE_CLASS, BTreeMap::new(), first.clone())])
    }
}

/// Classifica la pagina: `fund_info` se ha visto un nome di fondo, `investments` altrimenti.
struct ClassifyByBlockType;

impl DeserializePipe for ClassifyByBlockType {
    fn name(&self) -> &str {
        "classify-by-block-type"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let class = match block.pdf_block.as_ref().map(|b| b.type_block.clone()) {
            Some(t) if t == BlockType::FUND_NAME => PageClass::new("fund_info"),
            _ => PageClass::new("investments"),
        };
        Ok(vec![Extracted::PageClass(Some(class))])
    }
}

/// Deposita il contenuto del blocco come contributo alla promessa `fund_name`.
struct PromiseFundName;

impl DeserializePipe for PromiseFundName {
    fn name(&self) -> &str {
        "promise-fund-name"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let mut entries = PromiseEntries::new();
        entries.push("fund_name", block.content.clone());
        Ok(vec![Extracted::Promises(entries)])
    }
}

/// Deposita **un solo** contributo, che è però un contenitore: le parole del contenuto del blocco
/// raccolte in una `BlockValue::List`.
///
/// È la prova, attraverso la sola superficie `freeports::api`, che un contributo-contenitore non
/// viene confuso con N contributi scalari (F2): un id con un solo contributo-lista deve restare
/// distinguibile da un id con tante parole quante ne contiene la lista.
struct PromiseWordsAsOneList;

impl DeserializePipe for PromiseWordsAsOneList {
    fn name(&self) -> &str {
        "promise-words-as-one-list"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let content = block.content.as_str().ok_or_else(|| {
            PipeError::extraction("promise-words-as-one-list", "block content is not a string")
        })?;
        let words: Vec<BlockValue> = content.split_whitespace().map(BlockValue::from).collect();
        let mut entries = PromiseEntries::new();
        entries.push("fund_words", BlockValue::List(words));
        Ok(vec![Extracted::Promises(entries)])
    }
}

/// Deserializza come [`PromiseFundName`], ma **esplode** sulle righe marcate `boom`.
///
/// Serve a esercitare il contenimento per pagina dall'esterno: una riga illeggibile in una pagina
/// non deve costare né le altre pagine né la corsa.
struct ExplodeOnBoom;

impl DeserializePipe for ExplodeOnBoom {
    fn name(&self) -> &str {
        "explode-on-boom"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let content = block.content.as_str().unwrap_or_default();
        if content.contains("boom") {
            return Err(PipeError::extraction("explode-on-boom", format!("cannot read `{content}`")));
        }
        let mut entries = PromiseEntries::new();
        entries.push("fund_name", block.content.clone());
        Ok(vec![Extracted::Promises(entries)])
    }
}

/// Registra quante target companies e quanti risultati precedenti ha visto, e non produce nulla:
/// serve a verificare la semantica di `FilterData` attraverso il motore intero.
struct CountingFilter {
    seen: std::sync::Mutex<Vec<(usize, usize)>>,
}

impl TextFilterPipe for CountingFilter {
    fn name(&self) -> &str {
        "counting-filter"
    }

    fn filter(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        self.seen
            .lock()
            .expect("test-only mutex")
            .push((data.target_companies().len(), data.previous().len()));
        Ok(blocks
            .iter()
            .map(|b| TextBlock::new(BlockType::FUND, BTreeMap::new(), b.clone()))
            .collect())
    }
}

// ---------------------------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------------------------

fn page(number: u32, lines: &[&str]) -> Page {
    use freeports::formats_utils::pdf_extract::pdf_line::PdfLine;
    let lines = lines
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let y = i as f32 * 10.0;
            PdfLine::new("Arial", 10.0, text, (0.0, y, 100.0, y + 10.0))
        })
        .collect();
    Page::new(number, (595.0, 842.0), lines, vec![])
}

fn pipeline(
    name: &str,
    extract: Arc<dyn PdfExtractPipe>,
    filter: Arc<dyn TextFilterPipe>,
    deserialize: Arc<dyn DeserializePipe>,
) -> Pipeline {
    let mut pipeline = Pipeline::new(name);
    pipeline.pdf_extract.push(extract);
    pipeline.text_filter.push(filter);
    pipeline.deserialize.push(deserialize);
    pipeline
}

/// L'algoritmo del test: classifica ogni pagina, poi in due step elabora prima le pagine
/// `fund_info` e poi quelle `investments`.
fn algorithm(investments_filter: Arc<dyn TextFilterPipe>) -> Algorithm {
    algorithm_with(investments_filter, Arc::new(PromiseFundName))
}

/// Come [`algorithm`], ma con il deserializzatore che deposita le promesse scelto dal chiamante:
/// serve a esercitare forme diverse di contributo attraverso lo stesso motore.
fn algorithm_with(
    investments_filter: Arc<dyn TextFilterPipe>,
    promising: Arc<dyn DeserializePipe>,
) -> Algorithm {
    let classify = pipeline(
        "classify",
        Arc::new(SplitLines),
        Arc::new(FirstBlockOnly),
        Arc::new(ClassifyByBlockType),
    );
    let fund_info = pipeline(
        "fund_info",
        Arc::new(SplitLines),
        KeepType::pipe("keep-fund", BlockType::FUND_NAME),
        Arc::clone(&promising),
    );
    let investments = pipeline(
        "investments",
        Arc::new(SplitLines),
        investments_filter,
        promising,
    );

    Algorithm::new(
        "TESTFMT-EN24",
        BTreeMap::from([
            (PipelineName::new("classify"), classify),
            (PipelineName::new("fund_info"), fund_info),
            (PipelineName::new("investments"), investments),
        ]),
        &[PipelineName::new("classify")],
        PageClassFinalizer::Identity,
        Schedule::new(vec![
            ["fund_info"].into_iter().collect::<ScheduleStep>(),
            ["investments"].into_iter().collect::<ScheduleStep>(),
        ]),
        BTreeMap::from([
            (PageClass::new("fund_info"), vec![PipelineName::new("fund_info")]),
            (PageClass::new("investments"), vec![PipelineName::new("investments")]),
        ]),
    )
    .expect("fixture is a consistent configuration")
}

/// Un documento con una pagina di intestazione fondo e una di tabella.
fn document(id: &str, fund: &str) -> Document {
    Document::new(
        id,
        "TESTFMT-EN24",
        vec![
            page(1, &[&format!("fund: {fund}")]),
            page(2, &["row: Acme Corp"]),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------------------------

mod single_document {
    use super::*;

    #[test]
    fn the_whole_engine_runs_from_pages_to_per_page_results() {
        let algorithm = algorithm(KeepType::pipe("keep-rows", BlockType::TABLE_BODY));
        let outcome = algorithm.apply(&document("report-2023", "Alpha Fund"), &[]).unwrap();

        assert_eq!(outcome.id.as_str(), "report-2023");
        assert_eq!(outcome.format.as_str(), "TESTFMT-EN24");

        let pages: Vec<(u32, &str, usize)> = outcome
            .pages
            .iter()
            .map(|p| (p.page, p.class.as_str(), p.results.len()))
            .collect();
        assert_eq!(pages, vec![(1, "fund_info", 1), (2, "investments", 1)]);
    }

    #[test]
    fn each_page_is_classified_before_being_scheduled() {
        let algorithm = algorithm(KeepType::pipe("keep-rows", BlockType::TABLE_BODY));
        let classes = algorithm.classify_pages(&document("d", "Alpha Fund")).unwrap();
        assert_eq!(
            classes,
            vec![Some(PageClass::new("fund_info")), Some(PageClass::new("investments"))]
        );
    }

    /// I contributi depositati da pagine diverse restano **due contributi** anche dopo
    /// l'appiattimento: la multimappa non li fonde in un unico valore.
    #[test]
    fn the_promises_deposited_across_pages_resolve_to_two_contributions() {
        let algorithm = algorithm(KeepType::pipe("keep-rows", BlockType::TABLE_BODY));
        let outcome = algorithm.apply(&document("d", "Alpha Fund"), &[]).unwrap();

        let mut promises = PromiseMap::new();
        for page in &outcome.pages {
            for result in &page.results {
                if let Some(entries) = result.as_promises() {
                    entries.merge_into(&mut promises);
                }
            }
        }

        // Due contributi: il nome del fondo (pagina 1) e la riga di tabella (pagina 2).
        assert_eq!(promises.get("fund_name").map(<[BlockValue]>::len), Some(2));

        let flattened = promises.flatten().unwrap();
        assert_eq!(
            flattened.get("fund_name"),
            Some(&[BlockValue::from("Alpha Fund"), BlockValue::from("Acme Corp")][..])
        );
        // Promessa normale: vince l'ultima pagina. Promessa multiple: entrambe.
        assert_eq!(
            flattened.fulfill(&Promise::new("fund_name")).unwrap(),
            BlockValue::from("Acme Corp")
        );
        assert_eq!(
            flattened.fulfill(&Promise::new("fund_name[]")).unwrap(),
            BlockValue::List(vec![BlockValue::from("Alpha Fund"), BlockValue::from("Acme Corp")])
        );
    }

    #[test]
    fn a_pipe_that_cannot_read_a_line_costs_that_page_and_not_the_run() {
        let algorithm = algorithm(KeepType::pipe("keep-rows", BlockType::TABLE_BODY));
        let broken = Document::new("d", "TESTFMT-EN24", vec![page(1, &["no prefix here"])]);
        let outcome = algorithm.apply(&broken, &[]).unwrap();
        // La pagina esplode già in classificazione, quindi resta senza classe e nessuno step la
        // prende in carico: la corsa finisce bene, con niente da dire su quella pagina.
        assert!(outcome.pages.is_empty());
    }
}

/// Un guasto appartiene alla cosa più piccola che lo contiene. Visto da fuori: un documento con
/// una pagina rotta produce lo stesso i risultati di quelle sane.
mod page_containment {
    use super::*;

    /// Il documento del resto dei test, più una terza pagina che fa esplodere il deserializzatore.
    fn document_with_a_doomed_page() -> Document {
        Document::new(
            "d",
            "TESTFMT-EN24",
            vec![
                page(1, &["fund: Alpha Fund"]),
                page(2, &["row: Acme Corp"]),
                page(3, &["row: boom"]),
            ],
        )
    }

    fn exploding_algorithm() -> Algorithm {
        algorithm_with(KeepType::pipe("keep-rows", BlockType::TABLE_BODY), Arc::new(ExplodeOnBoom))
    }

    #[test]
    fn the_pages_that_can_be_read_are_read() {
        let outcome = exploding_algorithm().apply(&document_with_a_doomed_page(), &[]).unwrap();

        let per_page: Vec<(u32, usize)> =
            outcome.pages.iter().map(|p| (p.page, p.results.len())).collect();
        assert_eq!(per_page, vec![(1, 1), (2, 1), (3, 0)]);
    }

    #[test]
    fn what_the_good_pages_promised_still_resolves() {
        let outcome = exploding_algorithm().apply(&document_with_a_doomed_page(), &[]).unwrap();

        let mut promises = PromiseMap::new();
        for page in &outcome.pages {
            for result in &page.results {
                if let Some(entries) = result.as_promises() {
                    entries.merge_into(&mut promises);
                }
            }
        }
        let flattened = promises.flatten().unwrap();
        assert_eq!(
            flattened.get("fund_name"),
            Some(&[BlockValue::from("Alpha Fund"), BlockValue::from("Acme Corp")][..])
        );
    }

    #[test]
    fn a_broken_page_of_one_document_does_not_touch_the_other_documents() {
        let algorithm = exploding_algorithm();
        let documents = [document_with_a_doomed_page(), document("clean", "Beta Fund")];
        let outcomes = algorithm.apply_multidocument(&documents, &[]).unwrap();

        assert_eq!(outcomes.len(), 2);
        assert!(outcomes[1].pages.iter().all(|p| p.results.len() == 1));
    }
}

/// F2, visto da fuori: un contributo che è un contenitore non è N contributi.
mod container_valued_promises {
    use super::*;

    /// Raccoglie in una multimappa tutte le promesse depositate dalle pagine di un `outcome`.
    fn collect(outcome: &freeports::api::core::DocumentOutcome) -> PromiseMap {
        let mut promises = PromiseMap::new();
        for page in &outcome.pages {
            for result in &page.results {
                if let Some(entries) = result.as_promises() {
                    entries.merge_into(&mut promises);
                }
            }
        }
        promises
    }

    /// Ogni pagina deposita **un solo** contributo, che è una lista di parole. Dopo
    /// l'appiattimento devono restare due contributi (uno per pagina), ognuno la sua lista
    /// intatta — non quattro parole sciolte.
    #[test]
    fn a_list_contribution_survives_flattening_and_reaches_fulfill_intact() {
        let algorithm = algorithm_with(
            KeepType::pipe("keep-rows", BlockType::TABLE_BODY),
            Arc::new(PromiseWordsAsOneList),
        );
        let outcome = algorithm.apply(&document("d", "Alpha Fund"), &[]).unwrap();
        let promises = collect(&outcome);

        assert_eq!(promises.get("fund_words").map(<[BlockValue]>::len), Some(2));

        let alpha = BlockValue::List(vec![BlockValue::from("Alpha"), BlockValue::from("Fund")]);
        let acme = BlockValue::List(vec![BlockValue::from("Acme"), BlockValue::from("Corp")]);

        let flattened = promises.flatten().unwrap();
        assert_eq!(flattened.get("fund_words"), Some(&[alpha.clone(), acme.clone()][..]));

        // Promessa normale: vince l'ultimo contributo, ed è **la lista**, non il suo ultimo
        // elemento — è esattamente il punto in cui il contenitore veniva confuso col contributo.
        assert_eq!(
            flattened.fulfill(&Promise::new("fund_words")).unwrap(),
            acme.clone()
        );
        // Promessa multiple: due valori, ognuno la sua lista.
        assert_eq!(
            flattened.fulfill(&Promise::new("fund_words[]")).unwrap(),
            BlockValue::List(vec![alpha, acme])
        );
    }

    /// Lo stesso documento, deserializzato una volta in contributi-lista e una volta in contributi
    /// scalari: le due mappe appiattite devono restare **diverse**.
    #[test]
    fn one_list_contribution_per_page_is_not_the_same_as_one_scalar_per_page() {
        let doc = document("d", "Alpha Fund");
        let filter = || KeepType::pipe("keep-rows", BlockType::TABLE_BODY);

        let containers = algorithm_with(filter(), Arc::new(PromiseWordsAsOneList));
        let scalars = algorithm_with(filter(), Arc::new(PromiseFundName));

        let from_containers = collect(&containers.apply(&doc, &[]).unwrap()).flatten().unwrap();
        let from_scalars = collect(&scalars.apply(&doc, &[]).unwrap()).flatten().unwrap();

        assert_eq!(from_containers.get("fund_words").map(<[BlockValue]>::len), Some(2));
        assert_eq!(from_scalars.get("fund_name").map(<[BlockValue]>::len), Some(2));
        assert_ne!(from_containers.get("fund_words"), from_scalars.get("fund_name"));
    }
}

mod filter_data_across_the_schedule {
    use super::*;

    #[test]
    fn the_second_step_sees_what_the_first_produced() {
        let counting = Arc::new(CountingFilter { seen: std::sync::Mutex::new(Vec::new()) });
        let algorithm = algorithm(Arc::clone(&counting) as Arc<dyn TextFilterPipe>);

        algorithm.apply(&document("d", "Alpha Fund"), &[]).unwrap();

        // Il filtro sta solo nella pipeline `investments`, che gira al secondo step: vede zero
        // target companies e il risultato prodotto dal primo step.
        assert_eq!(*counting.seen.lock().expect("test-only mutex"), vec![(0, 1)]);
    }
}

mod multi_document {
    use super::*;

    #[test]
    fn two_documents_are_classified_separately_and_scheduled_together() {
        let counting = Arc::new(CountingFilter { seen: std::sync::Mutex::new(Vec::new()) });
        let algorithm = algorithm(Arc::clone(&counting) as Arc<dyn TextFilterPipe>);

        let docs = vec![document("first", "Alpha Fund"), document("second", "Beta Fund")];
        let outcomes = algorithm.apply_multidocument(&docs, &[]).unwrap();

        let ids: Vec<&str> = outcomes.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["first", "second"]);
        assert!(outcomes.iter().all(|o| o.pages.len() == 2));

        // Lo schedule lavora sull'unione: al secondo step entrambe le pagine `investments`
        // vedono i due risultati prodotti dal primo step (uno per documento).
        assert_eq!(*counting.seen.lock().expect("test-only mutex"), vec![(0, 2), (0, 2)]);
    }

    #[test]
    fn promises_of_different_documents_land_in_the_same_multimap() {
        let algorithm = algorithm(KeepType::pipe("keep-rows", BlockType::TABLE_BODY));
        let docs = vec![document("first", "Alpha Fund"), document("second", "Beta Fund")];
        let outcomes = algorithm.apply_multidocument(&docs, &[]).unwrap();

        let mut promises = PromiseMap::new();
        for outcome in &outcomes {
            for page in &outcome.pages {
                for result in &page.results {
                    if let Some(entries) = result.as_promises() {
                        entries.merge_into(&mut promises);
                    }
                }
            }
        }
        assert_eq!(promises.get("fund_name").map(<[BlockValue]>::len), Some(4));
    }
}

mod per_segment_api {
    use super::*;

    #[test]
    fn the_three_partial_apis_chain_into_the_full_one() {
        let algorithm = algorithm(KeepType::pipe("keep-rows", BlockType::TABLE_BODY));
        let class = PageClass::new("investments");
        let page = page(2, &["row: Acme Corp"]);

        let blocks = algorithm.apply_pdf_extract(&page, &class).unwrap();
        assert_eq!(blocks.len(), 1);

        let text_blocks = algorithm.apply_text_filter(&page, &class, &FilterData::EMPTY).unwrap();
        assert_eq!(text_blocks.len(), 1);

        let extracted = algorithm.apply_deserializer(&text_blocks, &class).unwrap();
        assert_eq!(extracted.len(), 1);
        assert!(extracted[0].as_promises().is_some());
    }
}

mod configuration_errors {
    use super::*;

    #[test]
    fn a_schedule_naming_a_page_class_nobody_maps_is_rejected_at_construction() {
        let classify = pipeline(
            "classify",
            Arc::new(SplitLines),
            KeepType::pipe("keep", BlockType::FUND_NAME),
            Arc::new(ClassifyByBlockType),
        );
        let result = Algorithm::new(
            "TESTFMT-EN24",
            BTreeMap::from([(PipelineName::new("classify"), classify)]),
            &[PipelineName::new("classify")],
            PageClassFinalizer::Identity,
            Schedule::new(vec![["ghost"].into_iter().collect::<ScheduleStep>()]),
            BTreeMap::new(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn a_page_whose_class_no_step_names_is_rejected_at_run_time() {
        // La pipeline di classificazione produce `investments`, ma lo schedule conosce solo
        // `fund_info`: la pagina 2 non ha dove andare.
        let classify = pipeline(
            "classify",
            Arc::new(SplitLines),
            Arc::new(FirstBlockOnly),
            Arc::new(ClassifyByBlockType),
        );
        let fund_info = pipeline(
            "fund_info",
            Arc::new(SplitLines),
            KeepType::pipe("keep-fund", BlockType::FUND_NAME),
            Arc::new(PromiseFundName),
        );
        let algorithm = Algorithm::new(
            "TESTFMT-EN24",
            BTreeMap::from([
                (PipelineName::new("classify"), classify),
                (PipelineName::new("fund_info"), fund_info),
            ]),
            &[PipelineName::new("classify")],
            PageClassFinalizer::Identity,
            Schedule::new(vec![["fund_info"].into_iter().collect::<ScheduleStep>()]),
            BTreeMap::from([(
                PageClass::new("fund_info"),
                vec![PipelineName::new("fund_info")],
            )]),
        )
        .unwrap();

        assert!(algorithm.apply(&document("d", "Alpha Fund"), &[]).is_err());
    }
}
