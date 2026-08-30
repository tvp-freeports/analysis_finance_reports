//! I tre trait dei pipe e [`Segment<P>`], la collezione ordinata e deduplicata che li contiene.
//!
//! `PLAN.md` §5.1 e §5.2. Due differenze sostanziali dal riferimento, entrambe volute:
//!
//! 1. **Un pipe è un trait, non un callable.** Nel riferimento un segmento è un `Vec<Py<PyAny>>`
//!    di oggetti chiamabili e il resto del sistema non ha modo di sapere che cosa siano. Qui i
//!    pipe nativi e quelli definiti dall'autore del formato (M7) implementano lo *stesso* trait:
//!    il motore non sa se un pipe è Rust o Python, e un pipe che fallisce è identificabile perché
//!    ha un `name()`.
//! 2. **Un solo `Segment<P>` generico** invece di tre struct con i metodi copiaincollati (nel
//!    riferimento la triplicazione è imposta da PyO3, che ammette un solo blocco `#[pymethods]`
//!    per pyclass). Deduplicazione, unione e iterazione sono scritte una volta sola; aggiungere un
//!    quarto segmento (`targets/3_add_segments.md`) costa un trait, un alias e un campo.
//!
//! **Ordine** (`PLAN.md` §5.2, D5): i pipe girano nell'ordine di inserimento, non nell'ordine di
//! hash di un `set` Python. È deterministico e rende i test riproducibili.
//!
//! **Deduplicazione per identità** ([`Arc::ptr_eq`]), come il `set` di oggetti senza `__hash__`
//! del riferimento: due pipe *configurati allo stesso modo* ma costruiti separatamente sono due
//! pipe distinti e girano entrambi.
//!
//! `Send + Sync` è richiesto sui tre trait fin da ora, non dopo: rende possibile parallelizzare
//! per pagina/documento senza riprogettare (i pipe Python resteranno serializzati dal GIL, il
//! resto scala).

use std::sync::Arc;

use crate::core::classes::{BlockValue, PdfBlock, TextBlock};
use crate::core::page::Page;

use super::data::{Extracted, FilterData, PipeError};

/// Primo segmento: dalla pagina ai blocchi PDF grezzi.
pub trait PdfExtractPipe: Send + Sync {
    /// Nome del pipe, per il logging e i messaggi d'errore.
    fn name(&self) -> &str;
    fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError>;
}

/// Secondo segmento: dai blocchi PDF ai blocchi di testo selezionati.
pub trait TextFilterPipe: Send + Sync {
    fn name(&self) -> &str;
    fn filter(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError>;
}

/// Terzo segmento: da un blocco di testo alle entità estratte.
///
/// Prende **un** blocco per volta, non la lista: è la forma del riferimento, dove un
/// deserializer viene invocato una volta per blocco.
pub trait DeserializePipe: Send + Sync {
    fn name(&self) -> &str;
    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError>;
}

/// Collezione ordinata e deduplicata di pipe dello stesso segmento.
pub struct Segment<P: ?Sized>(Vec<Arc<P>>);

impl<P: ?Sized> Segment<P> {
    pub fn new() -> Self {
        Segment(Vec::new())
    }

    /// Aggiunge un pipe in coda, se non è già presente **per identità**. Restituisce `true` se è
    /// stato davvero aggiunto.
    pub fn push(&mut self, pipe: Arc<P>) -> bool {
        if self.0.iter().any(|existing| Arc::ptr_eq(existing, &pipe)) {
            return false;
        }
        self.0.push(pipe);
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<P>> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Unione preservando l'ordine: prima i pipe di `self`, poi quelli di `other` non già
    /// presenti per identità.
    pub fn union(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for pipe in &other.0 {
            merged.push(Arc::clone(pipe));
        }
        merged
    }
}

// `#[derive(Default)]`/`#[derive(Clone)]` aggiungerebbero un bound `P: Default`/`P: Clone`, che
// per un `dyn Trait` non è soddisfabile: entrambi vanno scritti a mano.
impl<P: ?Sized> Default for Segment<P> {
    fn default() -> Self {
        Segment::new()
    }
}

impl<P: ?Sized> Clone for Segment<P> {
    fn clone(&self) -> Self {
        Segment(self.0.iter().map(Arc::clone).collect())
    }
}

impl<P: ?Sized> std::fmt::Debug for Segment<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Segment({} pipes)", self.0.len())
    }
}

impl<P: ?Sized> std::ops::Add for Segment<P> {
    type Output = Segment<P>;

    /// È così che i tre livelli del repo formati (structured + semistructured + unstructured) si
    /// combinano — `PLAN.md` §6.4.
    fn add(self, rhs: Self) -> Self::Output {
        self.union(&rhs)
    }
}

impl<P: ?Sized> FromIterator<Arc<P>> for Segment<P> {
    fn from_iter<I: IntoIterator<Item = Arc<P>>>(iter: I) -> Self {
        let mut segment = Segment::new();
        for pipe in iter {
            segment.push(pipe);
        }
        segment
    }
}

/// Un estratto breve del contenuto di un blocco, pensato per una riga di log: il **testo** che un
/// autore di formati puo' incollare in Ctrl-F dentro il PDF per ritrovare il punto.
///
/// E' la ragione per cui i log dei tre segmenti non si limitano piu' a contare i blocchi
/// prodotti: un `blocks=12` non e' ancorabile a niente, mentre la prima riga di testo estratta
/// dice subito *dove* e' successo. Il conteggio resta, come campo secondario.
///
/// Un contenitore (`List`/`Set`/`Map`) si riduce al primo elemento, ricorsivamente: un pipe che
/// produce una tabella deve mostrare la prima cella, non `List([...])`.
fn searchable_excerpt(value: &BlockValue) -> String {
    /// Oltre questa soglia il testo viene troncato con un'ellissi: una riga di log deve restare
    /// una riga.
    const MAX_CHARS: usize = 60;

    let raw = match value {
        BlockValue::Null => String::new(),
        BlockValue::Str(text) => text.clone(),
        BlockValue::List(items) => items.first().map(searchable_excerpt).unwrap_or_default(),
        BlockValue::Set(items) => items.iter().next().map(searchable_excerpt).unwrap_or_default(),
        BlockValue::Map(entries) => {
            entries.values().next().map(searchable_excerpt).unwrap_or_default()
        }
        other => format!("{other:?}"),
    };
    if raw.chars().count() <= MAX_CHARS {
        raw
    } else {
        format!("{}…", raw.chars().take(MAX_CHARS).collect::<String>())
    }
}

/// La riga di log di un segmento, emessa **solo se c'e' davvero qualcosa da dire**: almeno un
/// risultato *e* un estratto non vuoto con cui ancorarlo alla pagina.
///
/// La condizione "estratto non vuoto" non e' pignoleria. `PdfExtractPageClassifyStandard`
/// restituisce sempre esattamente un blocco, anche quando la pagina non appartiene alla sua page
/// class, e quel blocco ha contenuto vuoto: contare i blocchi produceva 11.259 righe identiche e
/// prive di contenuto su un solo documento, meta' dell'intero `.log.csv` a `-vv`.
fn log_segment_output(message: &'static str, produced: usize, sample: Option<&BlockValue>) {
    if produced == 0 {
        return;
    }
    let Some(excerpt) = sample.map(searchable_excerpt).filter(|text| !text.is_empty()) else {
        return;
    };
    tracing::debug!(found = %excerpt, produced, "{}", message);
}

pub type PdfExtractSegment = Segment<dyn PdfExtractPipe>;
pub type TextFilterSegment = Segment<dyn TextFilterPipe>;
pub type DeserializeSegment = Segment<dyn DeserializePipe>;

impl PdfExtractSegment {
    /// Concatena i blocchi prodotti da ogni pipe, nell'ordine di inserimento.
    pub fn apply(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        let segment_span = tracing::info_span!("pdf_extract");
        let _segment_guard = segment_span.enter();

        let mut out = Vec::new();
        for pipe in self.iter() {
            // Span innermost del vocabolario `Activity` (`PLAN.md` §3 L1/L2): incapsula la
            // singola chiamata a un pipe, non l'intero segmento.
            let pipe_span = tracing::info_span!("pipe", pipe = pipe.name());
            let _pipe_guard = pipe_span.enter();
            let blocks = pipe.extract(page)?;
            // Solo se il pipe ha davvero prodotto qualcosa. Un pipe che non si applica a questa
            // pagina e' il caso normale — ogni page class viene provata su ogni pagina — e la sua
            // riga vuota era da sola meta' del `.log.csv` a `-vv`.
            log_segment_output("pdf blocks extracted", blocks.len(), blocks.first().map(|b| &b.content));
            out.extend(blocks);
        }
        Ok(out)
    }
}

impl TextFilterSegment {
    /// Concatena i blocchi di testo prodotti da ogni pipe, nell'ordine di inserimento.
    pub fn apply(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        let segment_span = tracing::info_span!("text_filter");
        let _segment_guard = segment_span.enter();

        let mut out = Vec::new();
        for pipe in self.iter() {
            let pipe_span = tracing::info_span!("pipe", pipe = pipe.name());
            let _pipe_guard = pipe_span.enter();
            let filtered = pipe.filter(blocks, data)?;
            log_segment_output("text blocks kept", filtered.len(), filtered.first().map(|b| &b.content));
            out.extend(filtered);
        }
        Ok(out)
    }
}

impl DeserializeSegment {
    /// Itera **pipe × blocchi** (per ogni pipe, tutti i blocchi), come il riferimento.
    ///
    /// Il riferimento conserva nel risultato anche i `None` restituiti dai pipe, per filtrarli
    /// più avanti; qui non serve, perché un pipe che non ha nulla da dire restituisce un vettore
    /// vuoto. La distinzione che quel `None` doveva rappresentare nella classificazione — "una
    /// classificazione c'è stata, ed è *nessuna class*" — non si perde: è
    /// [`Extracted::PageClass(None)`](crate::core::pipeline::Extracted::PageClass), una variante
    /// esplicita.
    pub fn apply(&self, blocks: &[TextBlock]) -> Result<Vec<Extracted>, PipeError> {
        let segment_span = tracing::info_span!("deserialize");
        let _segment_guard = segment_span.enter();

        let mut out = Vec::new();
        for pipe in self.iter() {
            // Il conteggio si logga una volta per pipe, non per blocco: un pipe di
            // deserializzazione gira su tutti i blocchi della pagina (rule L2 "nessun log sopra
            // trace in un ciclo caldo"), mentre lo span `pipe` avvolge comunque ogni singola
            // chiamata, come richiesto dal vocabolario `Activity`.
            let mut produced = 0usize;
            for block in blocks {
                let pipe_span = tracing::info_span!("pipe", pipe = pipe.name());
                let _pipe_guard = pipe_span.enter();
                let results = pipe.deserialize(block)?;
                produced += results.len();
                out.extend(results);
            }
            log_segment_output("entities deserialized", produced, blocks.first().map(|b| &b.content));
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod test_pipes {
    //! Pipe finti condivisi dai test di `segment`, `pipeline`, `bundle` e `algorithm`.
    //!
    //! Nessun pipe reale esiste ancora: `formats_utils::pdf_extract::standard_funcs` non è
    //! assegnato a nessuna milestone (vedi `STATUS.md`) e i pipe `text_filter`/`deserialize`
    //! reali arrivano con `output::classes` (M8). I test del motore verificano l'orchestrazione,
    //! non i pipe: questi doppi rendono esplicito *cosa* il motore garantisce.

    use super::*;
    use crate::core::classes::{BlockType, TextBlock};
    use crate::core::page::PageError;
    use crate::core::schedule::PageClass;
    use std::sync::Mutex;

    /// Estrae un blocco per ogni riga della pagina, con il testo della riga come contenuto.
    pub(crate) struct LinesToBlocks {
        pub(crate) name: String,
        pub(crate) type_block: BlockType,
    }

    impl LinesToBlocks {
        pub(crate) fn pipe(name: &str) -> Arc<dyn PdfExtractPipe> {
            Arc::new(LinesToBlocks {
                name: name.to_string(),
                type_block: BlockType::RELEVANT_BLOCK,
            })
        }
    }

    impl PdfExtractPipe for LinesToBlocks {
        fn name(&self) -> &str {
            &self.name
        }

        fn extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
            Ok(page
                .lines
                .iter()
                .map(|line| PdfBlock::bare(self.type_block.clone(), line.text().clone()))
                .collect())
        }
    }

    /// Fallisce sempre, con l'errore che gli è stato dato — serve a distinguere il fallimento
    /// assorbibile (pagina saltata) da quello fatale.
    pub(crate) struct FailingExtract {
        pub(crate) name: String,
        pub(crate) error: PipeError,
    }

    impl FailingExtract {
        pub(crate) fn page_parse(name: &str) -> Arc<dyn PdfExtractPipe> {
            Arc::new(FailingExtract {
                name: name.to_string(),
                error: PipeError::page_parse(name, PageError::ParseFail {
                    message: "unreadable page".to_string(),
                }),
            })
        }

        pub(crate) fn fatal(name: &str) -> Arc<dyn PdfExtractPipe> {
            Arc::new(FailingExtract {
                name: name.to_string(),
                error: PipeError::extraction(name, "boom"),
            })
        }
    }

    impl PdfExtractPipe for FailingExtract {
        fn name(&self) -> &str {
            &self.name
        }

        fn extract(&self, _page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
            Err(self.error.clone())
        }
    }

    /// Converte ogni blocco PDF in un blocco di testo, e registra quale `FilterData` ha visto.
    pub(crate) struct RecordingFilter {
        pub(crate) name: String,
        pub(crate) seen: Mutex<Vec<(usize, usize)>>,
    }

    impl RecordingFilter {
        pub(crate) fn new(name: &str) -> Arc<RecordingFilter> {
            Arc::new(RecordingFilter { name: name.to_string(), seen: Mutex::new(Vec::new()) })
        }

        /// `(numero di target companies, numero di risultati precedenti)` visti a ogni chiamata.
        pub(crate) fn seen(&self) -> Vec<(usize, usize)> {
            self.seen.lock().expect("test-only mutex is never poisoned").clone()
        }
    }

    impl TextFilterPipe for RecordingFilter {
        fn name(&self) -> &str {
            &self.name
        }

        fn filter(
            &self,
            blocks: &[PdfBlock],
            data: &FilterData<'_>,
        ) -> Result<Vec<TextBlock>, PipeError> {
            self.seen
                .lock()
                .expect("test-only mutex is never poisoned")
                .push((data.target_companies().len(), data.previous().len()));
            Ok(blocks
                .iter()
                .map(|b| TextBlock::new(BlockType::PAGE_CLASS, b.metadata.clone(), b.clone()))
                .collect())
        }
    }

    /// Classifica ogni blocco con la class fissa che gli è stata data.
    pub(crate) struct ConstantClassifier {
        pub(crate) name: String,
        pub(crate) class: Option<PageClass>,
    }

    impl ConstantClassifier {
        pub(crate) fn pipe(name: &str, class: Option<&str>) -> Arc<dyn DeserializePipe> {
            Arc::new(ConstantClassifier {
                name: name.to_string(),
                class: class.map(PageClass::new),
            })
        }
    }

    impl DeserializePipe for ConstantClassifier {
        fn name(&self) -> &str {
            &self.name
        }

        fn deserialize(&self, _block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
            Ok(vec![Extracted::PageClass(self.class.clone())])
        }
    }

    /// Deposita una promessa per ogni blocco ricevuto.
    pub(crate) struct PromiseDepositor {
        pub(crate) name: String,
        pub(crate) id: String,
    }

    impl PromiseDepositor {
        pub(crate) fn pipe(name: &str, id: &str) -> Arc<dyn DeserializePipe> {
            Arc::new(PromiseDepositor { name: name.to_string(), id: id.to_string() })
        }
    }

    impl DeserializePipe for PromiseDepositor {
        fn name(&self) -> &str {
            &self.name
        }

        fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
            let entries = [(self.id.clone(), block.content.clone())].into_iter().collect();
            Ok(vec![Extracted::Promises(entries)])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_pipes::*;
    use super::*;
    use crate::core::classes::BlockType;
    use crate::core::page::Page;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;

    fn page_with(texts: &[&str]) -> Page {
        let lines = texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let y = i as f32 * 10.0;
                PdfLine::new("Arial", 10.0, t, (0.0, y, 10.0, y + 10.0))
            })
            .collect();
        Page::new(1, (100.0, 100.0), lines, vec![])
    }

    mod dedup_and_order {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn pipes_run_in_insertion_order() {
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("first"));
            segment.push(LinesToBlocks::pipe("second"));

            let names: Vec<&str> = segment.iter().map(|p| p.name()).collect();
            assert_eq!(names, vec!["first", "second"]);
        }

        #[test]
        fn the_same_pipe_added_twice_is_kept_once() {
            let pipe = LinesToBlocks::pipe("only");
            let mut segment = PdfExtractSegment::new();
            assert!(segment.push(Arc::clone(&pipe)));
            assert!(!segment.push(pipe));
            assert_eq!(segment.len(), 1);
        }

        #[test]
        fn two_identically_configured_pipes_are_distinct() {
            // Deduplicazione per identita', non per valore: e' la semantica del `set` di oggetti
            // senza `__hash__` del riferimento.
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("same"));
            segment.push(LinesToBlocks::pipe("same"));
            assert_eq!(segment.len(), 2);
        }

        #[test]
        fn a_duplicate_does_not_move_the_original_to_the_back() {
            let first = LinesToBlocks::pipe("first");
            let mut segment = PdfExtractSegment::new();
            segment.push(Arc::clone(&first));
            segment.push(LinesToBlocks::pipe("second"));
            segment.push(first);

            let names: Vec<&str> = segment.iter().map(|p| p.name()).collect();
            assert_eq!(names, vec!["first", "second"]);
        }

        #[test]
        fn a_fresh_segment_is_empty() {
            let segment = PdfExtractSegment::new();
            assert!(segment.is_empty());
            assert_eq!(segment.len(), 0);
            assert_eq!(PdfExtractSegment::default().len(), 0);
        }

        #[test]
        fn collecting_from_an_iterator_deduplicates_too() {
            let pipe = LinesToBlocks::pipe("only");
            let segment: PdfExtractSegment =
                [Arc::clone(&pipe), pipe].into_iter().collect();
            assert_eq!(segment.len(), 1);
        }
    }

    mod union {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn adding_two_segments_concatenates_them_in_order() {
            let mut left = PdfExtractSegment::new();
            left.push(LinesToBlocks::pipe("a"));
            let mut right = PdfExtractSegment::new();
            right.push(LinesToBlocks::pipe("b"));

            let merged = left + right;
            let names: Vec<&str> = merged.iter().map(|p| p.name()).collect();
            assert_eq!(names, vec!["a", "b"]);
        }

        #[test]
        fn a_pipe_present_in_both_segments_appears_once() {
            let shared = LinesToBlocks::pipe("shared");
            let mut left = PdfExtractSegment::new();
            left.push(Arc::clone(&shared));
            let mut right = PdfExtractSegment::new();
            right.push(Arc::clone(&shared));
            right.push(LinesToBlocks::pipe("extra"));

            let merged = left + right;
            let names: Vec<&str> = merged.iter().map(|p| p.name()).collect();
            assert_eq!(names, vec!["shared", "extra"]);
        }

        #[test]
        fn union_leaves_both_operands_untouched() {
            let mut left = PdfExtractSegment::new();
            left.push(LinesToBlocks::pipe("a"));
            let mut right = PdfExtractSegment::new();
            right.push(LinesToBlocks::pipe("b"));

            let merged = left.union(&right);
            assert_eq!(merged.len(), 2);
            assert_eq!(left.len(), 1);
            assert_eq!(right.len(), 1);
        }

        #[test]
        fn adding_an_empty_segment_changes_nothing() {
            let mut left = PdfExtractSegment::new();
            left.push(LinesToBlocks::pipe("a"));
            let merged = left + PdfExtractSegment::new();
            assert_eq!(merged.len(), 1);
        }

        #[test]
        fn cloning_a_segment_shares_the_same_pipes() {
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("a"));
            let copy = segment.clone();
            // Stessi `Arc`: l'unione delle due deduplica a uno solo.
            assert_eq!((segment + copy).len(), 1);
        }
    }

    mod pdf_extract_application {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_blocks_of_every_pipe_are_concatenated_in_order() {
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("first"));
            segment.push(LinesToBlocks::pipe("second"));

            let blocks = segment.apply(&page_with(&["alpha"])).unwrap();
            let contents: Vec<&str> =
                blocks.iter().map(|b| b.content.as_str().unwrap()).collect();
            assert_eq!(contents, vec!["alpha", "alpha"]);
        }

        #[test]
        fn an_empty_segment_extracts_nothing() {
            let blocks = PdfExtractSegment::new().apply(&page_with(&["alpha"])).unwrap();
            assert!(blocks.is_empty());
        }

        #[test]
        fn a_page_with_no_lines_yields_no_blocks() {
            let mut segment = PdfExtractSegment::new();
            segment.push(LinesToBlocks::pipe("only"));
            assert!(segment.apply(&page_with(&[])).unwrap().is_empty());
        }

        #[test]
        fn the_first_failing_pipe_stops_the_segment() {
            let mut segment = PdfExtractSegment::new();
            segment.push(FailingExtract::fatal("boom"));
            segment.push(LinesToBlocks::pipe("never reached"));

            let err = segment.apply(&page_with(&["alpha"])).unwrap_err();
            assert_eq!(err.pipe(), "boom");
        }

        #[test]
        fn a_page_failure_travels_out_of_the_segment_unchanged() {
            let mut segment = PdfExtractSegment::new();
            segment.push(FailingExtract::page_parse("skipper"));
            let err = segment.apply(&page_with(&["alpha"])).unwrap_err();
            assert!(err.is_page_failure());
        }
    }

    mod text_filter_application {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_pipe_sees_the_same_blocks_and_the_same_filter_data() {
            let first = RecordingFilter::new("first");
            let second = RecordingFilter::new("second");
            let mut segment = TextFilterSegment::new();
            segment.push(Arc::clone(&first) as Arc<dyn TextFilterPipe>);
            segment.push(Arc::clone(&second) as Arc<dyn TextFilterPipe>);

            let blocks = vec![PdfBlock::bare(BlockType::RELEVANT_BLOCK, "x")];
            let previous = vec![Extracted::PageClass(None)];
            let out = segment.apply(&blocks, &FilterData::Previous(&previous)).unwrap();

            assert_eq!(out.len(), 2);
            assert_eq!(first.seen(), vec![(0, 1)]);
            assert_eq!(second.seen(), vec![(0, 1)]);
        }

        #[test]
        fn an_empty_segment_filters_nothing() {
            let out = TextFilterSegment::new().apply(&[], &FilterData::EMPTY).unwrap();
            assert!(out.is_empty());
        }
    }

    mod deserialize_application {
        use super::*;
        use pretty_assertions::assert_eq;

        fn text_blocks(n: usize) -> Vec<TextBlock> {
            (0..n)
                .map(|i| {
                    TextBlock::from_content(
                        BlockType::PAGE_CLASS,
                        std::collections::BTreeMap::new(),
                        format!("blk{i}"),
                    )
                })
                .collect()
        }

        #[test]
        fn each_pipe_is_applied_to_every_block() {
            let mut segment = DeserializeSegment::new();
            segment.push(ConstantClassifier::pipe("a", Some("x")));
            segment.push(ConstantClassifier::pipe("b", Some("y")));

            let out = segment.apply(&text_blocks(2)).unwrap();
            assert_eq!(out.len(), 4);
        }

        #[test]
        fn pipes_are_the_outer_loop_and_blocks_the_inner_one() {
            // Ordine del riferimento: tutti i blocchi del primo pipe, poi quelli del secondo.
            let mut segment = DeserializeSegment::new();
            segment.push(ConstantClassifier::pipe("a", Some("x")));
            segment.push(ConstantClassifier::pipe("b", Some("y")));

            let out = segment.apply(&text_blocks(2)).unwrap();
            let classes: Vec<String> = out
                .iter()
                .map(|e| e.as_page_class().unwrap().as_ref().unwrap().to_string())
                .collect();
            assert_eq!(classes, vec!["x", "x", "y", "y"]);
        }

        #[test]
        fn an_unclassified_result_is_an_explicit_variant_not_an_absence() {
            let mut segment = DeserializeSegment::new();
            segment.push(ConstantClassifier::pipe("a", None));

            let out = segment.apply(&text_blocks(1)).unwrap();
            assert_eq!(out, vec![Extracted::PageClass(None)]);
        }

        #[test]
        fn no_blocks_means_no_results_even_with_pipes() {
            let mut segment = DeserializeSegment::new();
            segment.push(ConstantClassifier::pipe("a", Some("x")));
            assert!(segment.apply(&[]).unwrap().is_empty());
        }

        #[test]
        fn an_empty_segment_deserializes_nothing() {
            assert!(DeserializeSegment::new().apply(&text_blocks(2)).unwrap().is_empty());
        }
    }
}
