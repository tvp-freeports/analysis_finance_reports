//! Test d'integrazione di P2: gli span di `tracing` attraversano il confine di thread.
//!
//! `PLAN.md` §4 P2, punto 3. È l'unico requisito del passo che non si può verificare con un test
//! unitario: serve un subscriber **globale**, perché quello installato con `with_default` ha uno
//! scope thread-local e i thread di un pool rayon non lo vedrebbero — che è esattamente il difetto
//! che questo test deve saper riconoscere. Un file d'integrazione è un processo a sé, quindi
//! `set_global_default` (una volta sola per processo) qui si può usare.
//!
//! Se le closure del ciclo parallelo non riagganciassero lo span del chiamante, gli eventi
//! prodotti dai pipe uscirebbero con un percorso di span monco — e la colonna `Activity` del
//! `.log.csv` si svuoterebbe proprio dove serve, senza che alcun test unitario se ne accorgesse.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use freeports::api::core::{
    Algorithm, BlockType, Document, Extracted, FilterData, Page, PageClass, PageClassFinalizer,
    Parallelism, PdfBlock, PdfExtractPipe, PipeError, Pipeline, PipelineName, Schedule,
    ScheduleStep, TextBlock, TextFilterPipe,
};
use freeports::api::core::{DeserializePipe, PromiseEntries};
use freeports::formats_utils::pdf_extract::pdf_line::PdfLine;

use tracing::subscriber::set_global_default;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

// ---------------------------------------------------------------------------------------------
// Il layer che osserva: per ogni evento, il percorso `/`-separato degli span attivi
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct SpanPathCollector {
    paths: Arc<Mutex<Vec<String>>>,
}

impl SpanPathCollector {
    fn paths(&self) -> Vec<String> {
        self.paths.lock().expect("test-only mutex is never poisoned").clone()
    }
}

impl<S> Layer<S> for SpanPathCollector
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let path = ctx
            .event_scope(event)
            .map(|scope| {
                scope.from_root().map(|span| span.name().to_string()).collect::<Vec<_>>().join("/")
            })
            .unwrap_or_default();
        self.paths.lock().expect("test-only mutex is never poisoned").push(path);
    }
}

// ---------------------------------------------------------------------------------------------
// Pipe minimi, definiti qui con la sola superficie `api` — come in `algorithm_end_to_end.rs`
// ---------------------------------------------------------------------------------------------

struct OneBlockPerLine;

impl PdfExtractPipe for OneBlockPerLine {
    fn name(&self) -> &str {
        "one-block-per-line"
    }

    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        Ok(page
            .lines
            .iter()
            .map(|line| PdfBlock::bare(BlockType::RELEVANT_BLOCK, line.text().clone()))
            .collect())
    }
}

/// Emette **un evento per pagina**: è la traccia che il test va a leggere.
struct TalkativeFilter;

impl TextFilterPipe for TalkativeFilter {
    fn name(&self) -> &str {
        "talkative"
    }

    fn filter(&self, blocks: &[PdfBlock], _data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        tracing::warn!(blocks = blocks.len(), "the talkative pipe has something to say");
        Ok(blocks
            .iter()
            .map(|b| TextBlock::new(BlockType::PAGE_CLASS, b.metadata.clone(), b.clone()))
            .collect())
    }
}

struct ClassifyAs(Option<PageClass>);

impl DeserializePipe for ClassifyAs {
    fn name(&self) -> &str {
        "classify-as"
    }

    fn deserialize(&self, _block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        Ok(vec![Extracted::PageClass(self.0.clone())])
    }
}

struct EmitNothing;

impl DeserializePipe for EmitNothing {
    fn name(&self) -> &str {
        "emit-nothing"
    }

    fn deserialize(&self, _block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        Ok(vec![Extracted::Promises(PromiseEntries::default())])
    }
}

fn pipeline(name: &str, last: Arc<dyn DeserializePipe>) -> Pipeline {
    let mut pipeline = Pipeline::new(name);
    pipeline.pdf_extract.push(Arc::new(OneBlockPerLine) as Arc<dyn PdfExtractPipe>);
    pipeline.text_filter.push(Arc::new(TalkativeFilter) as Arc<dyn TextFilterPipe>);
    pipeline.deserialize.push(last);
    pipeline
}

fn algorithm() -> Algorithm {
    Algorithm::new(
        "FMT",
        BTreeMap::from([
            (
                PipelineName::new("classify"),
                pipeline("classify", Arc::new(ClassifyAs(Some(PageClass::new("a"))))),
            ),
            (PipelineName::new("work"), pipeline("work", Arc::new(EmitNothing))),
        ]),
        &[PipelineName::new("classify")],
        PageClassFinalizer::Identity,
        Schedule::new(vec![ScheduleStep::from_iter([PageClass::new("a")])]),
        BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
    )
    .expect("fixture is consistent")
}

fn document(pages: u32) -> Document {
    let pages = (1..=pages)
        .map(|n| {
            let line = PdfLine::new("Arial", 10.0, &format!("page {n}"), (0.0, 0.0, 10.0, 10.0));
            Page::new(n, (100.0, 100.0), vec![line], vec![])
        })
        .collect();
    Document::new("doc", "FMT", pages)
}

// ---------------------------------------------------------------------------------------------

/// Un solo `#[test]`: `set_global_default` si può chiamare una volta sola per processo, e i test
/// di uno stesso binario condividono il processo. Le tre asserzioni sono altrettante verifiche
/// distinte, tenute insieme dal subscriber che possono installare una volta sola.
#[test]
fn the_span_path_survives_the_thread_boundary() {
    let collector = SpanPathCollector::default();
    set_global_default(tracing_subscriber::registry().with(collector.clone()))
        .expect("this test binary installs the only global subscriber of its process");

    let outer = tracing::info_span!("run");
    let _guard = outer.enter();
    let outcome = algorithm()
        .apply_with(&document(32), &[], Parallelism::pages(4))
        .expect("the fixture cannot fail");
    assert_eq!(outcome.pages.len(), 32);

    let paths = collector.paths();
    assert!(!paths.is_empty(), "the talkative pipe must have produced events");

    // Classificazione: `run/classify/page/pipeline/text_filter/pipe`, con il prefisso `run` che
    // esiste solo sul thread chiamante — se non fosse riagganciato, il percorso comincerebbe da
    // `page`.
    assert!(
        paths.iter().any(|p| p.starts_with("run/classify/page/")),
        "no event kept the caller's span path through classification: {paths:?}"
    );
    // Esecuzione: `run/step/class/page/...`, il ciclo che P2 parallelizza per primo.
    assert!(
        paths.iter().any(|p| p.starts_with("run/step/class/page/")),
        "no event kept the caller's span path through the step loop: {paths:?}"
    );
    // Nessun evento orfano: **ogni** evento deve stare sotto lo span del chiamante, non solo
    // qualcuno. `"run"` esatto è legittimo (un evento emesso a quel livello, sul thread
    // chiamante); un percorso vuoto o che comincia da `page` sarebbe uno span perduto.
    assert!(
        paths.iter().all(|p| p == "run" || p.starts_with("run/")),
        "some events lost the caller's span entirely: {:?}",
        paths.iter().filter(|p| *p != "run" && !p.starts_with("run/")).collect::<Vec<_>>()
    );
}
