//! Pipeline: i tre segmenti `pdf_extract` → `text_filter` → `deserialize`.
//!
//! `PLAN.md` §5.3. Rispetto al riferimento la [`Pipeline`] ha un **nome**: là il nome vive solo
//! come chiave della mappa che la contiene, e questo rende i messaggi d'errore inutilizzabili
//! (`PLAN.md` D6).
//!
//! Il modulo è anche il punto d'ingresso del vocabolario del motore: ri-esporta
//! [`FilterData`]/[`Extracted`]/[`PipeError`] da [`data`] e i tre trait dei pipe con i loro
//! segmenti da [`segment`], così chi usa il motore importa da `core::pipeline` e non deve
//! conoscere la suddivisione interna in file.

pub mod bundle;
pub mod data;
pub mod segment;

pub use data::{Extracted, FilterData, PipeError, PromiseEntries};
pub use segment::{
    DeserializePipe, DeserializeSegment, PdfExtractPipe, PdfExtractSegment, Segment,
    TextFilterPipe, TextFilterSegment,
};

use crate::core::classes::{PdfBlock, TextBlock};
use crate::core::page::Page;

/// Nome di una pipeline all'interno di un formato (la parte fra parentesi di
/// `<formato>(<pipeline>)/<indice>`).
///
/// La stringa vuota è un nome legittimo: nel repo formati identifica la pipeline "senza gruppo".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PipelineName(String);

impl PipelineName {
    pub fn new(name: impl Into<String>) -> Self {
        PipelineName(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PipelineName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for PipelineName {
    fn from(value: &str) -> Self {
        PipelineName(value.to_string())
    }
}

impl From<String> for PipelineName {
    fn from(value: String) -> Self {
        PipelineName(value)
    }
}

/// I tre segmenti applicati in catena a una pagina.
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub name: PipelineName,
    pub pdf_extract: PdfExtractSegment,
    pub text_filter: TextFilterSegment,
    pub deserialize: DeserializeSegment,
}

impl Pipeline {
    /// Una pipeline con il nome dato e i tre segmenti vuoti.
    pub fn new(name: impl Into<PipelineName>) -> Self {
        Pipeline {
            name: name.into(),
            pdf_extract: PdfExtractSegment::new(),
            text_filter: TextFilterSegment::new(),
            deserialize: DeserializeSegment::new(),
        }
    }

    /// Una pipeline è completa quando **tutti e tre** i segmenti hanno almeno un pipe: solo
    /// allora può produrre risultati. `Algorithm::load` (M7) rifiuta quelle incomplete.
    pub fn is_complete(&self) -> bool {
        !self.pdf_extract.is_empty() && !self.text_filter.is_empty() && !self.deserialize.is_empty()
    }

    /// Estrae, filtra, deserializza: l'intera catena su una pagina.
    pub fn apply(
        &self,
        page: &Page,
        data: &FilterData<'_>,
    ) -> Result<Vec<Extracted>, PipeError> {
        // Punto di orchestrazione del vocabolario `Activity` (`PLAN.md` §3 L1/L2): i tre segmenti
        // (aperti dentro `Segment::apply`) e i `pipe[<nome>]` di ciascuno si annidano qui sotto.
        let pipeline_span = tracing::info_span!("pipeline", pipeline = %self.name);
        let _pipeline_guard = pipeline_span.enter();

        let pdf_blocks = self.pdf_extract.apply(page)?;
        let text_blocks = self.text_filter.apply(&pdf_blocks, data)?;
        self.deserialize.apply(&text_blocks)
    }

    /// Solo il primo segmento — API di test per segmento (`freeports-dev`, `PLAN.md` §5.3).
    pub fn apply_pdf_extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        let pipeline_span = tracing::info_span!("pipeline", pipeline = %self.name);
        let _pipeline_guard = pipeline_span.enter();
        self.pdf_extract.apply(page)
    }

    /// I primi due segmenti — API di test per segmento.
    pub fn apply_text_filter(
        &self,
        page: &Page,
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        let pipeline_span = tracing::info_span!("pipeline", pipeline = %self.name);
        let _pipeline_guard = pipeline_span.enter();
        let pdf_blocks = self.pdf_extract.apply(page)?;
        self.text_filter.apply(&pdf_blocks, data)
    }
}

impl std::ops::Add for Pipeline {
    type Output = Pipeline;

    /// Fonde i tre segmenti, uno per uno — è così che structured + semistructured + unstructured
    /// si combinano (`PLAN.md` §6.4).
    ///
    /// Il nome conservato è quello dell'operando **sinistro**: `Algorithm::load` (M7) somma solo
    /// pipeline che ha già raggruppato per nome, quindi i due nomi coincidono sempre nell'uso
    /// reale. Sommare pipeline di nome diverso non è un errore rilevabile qui (`Add` non può
    /// restituire un `Result`) ed è un errore del chiamante.
    fn add(self, rhs: Self) -> Self::Output {
        Pipeline {
            name: self.name,
            pdf_extract: self.pdf_extract + rhs.pdf_extract,
            text_filter: self.text_filter + rhs.text_filter,
            deserialize: self.deserialize + rhs.deserialize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::segment::test_pipes::*;
    use super::*;
    use crate::core::classes::BlockType;
    use crate::core::schedule::PageClass;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use std::sync::Arc;

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

    /// Una pipeline completa che classifica ogni pagina con `class`.
    fn classifying_pipeline(name: &str, class: Option<&str>) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(ConstantClassifier::pipe("classify", class));
        pipeline
    }

    mod pipeline_name {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn round_trips_and_displays() {
            assert_eq!(PipelineName::new("investments").as_str(), "investments");
            assert_eq!(PipelineName::from("investments").to_string(), "investments");
        }

        #[test]
        fn the_empty_name_is_legal_and_is_the_default() {
            // Nel repo formati identifica la pipeline "senza gruppo".
            assert_eq!(PipelineName::default(), PipelineName::new(""));
            assert_eq!(PipelineName::new("").to_string(), "");
        }

        #[test]
        fn is_built_from_both_str_and_string() {
            assert_eq!(PipelineName::from("p"), PipelineName::from("p".to_string()));
        }
    }

    mod completeness {
        use super::*;

        #[test]
        fn a_fresh_pipeline_is_incomplete() {
            assert!(!Pipeline::new("p").is_complete());
        }

        #[test]
        fn all_three_segments_are_required() {
            let full = classifying_pipeline("p", Some("x"));
            assert!(full.is_complete());

            let mut no_extract = full.clone();
            no_extract.pdf_extract = PdfExtractSegment::new();
            assert!(!no_extract.is_complete());

            let mut no_filter = full.clone();
            no_filter.text_filter = TextFilterSegment::new();
            assert!(!no_filter.is_complete());

            let mut no_deserialize = full;
            no_deserialize.deserialize = DeserializeSegment::new();
            assert!(!no_deserialize.is_complete());
        }
    }

    mod chaining {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_three_segments_run_in_order_and_feed_each_other() {
            let pipeline = classifying_pipeline("p", Some("investments"));
            let out = pipeline.apply(&page_with(&["a", "b"]), &FilterData::EMPTY).unwrap();
            // Due righe -> due blocchi pdf -> due blocchi di testo -> due classificazioni.
            assert_eq!(
                out,
                vec![
                    Extracted::PageClass(Some(PageClass::new("investments"))),
                    Extracted::PageClass(Some(PageClass::new("investments"))),
                ]
            );
        }

        #[test]
        fn an_incomplete_pipeline_simply_yields_nothing() {
            // Nessun pipe di deserializzazione: la catena arriva in fondo e produce zero
            // risultati, non un errore. E' `Algorithm::load` (M7) a rifiutare le incomplete.
            let mut pipeline = Pipeline::new("p");
            pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
            pipeline
                .text_filter
                .push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            assert!(pipeline.apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap().is_empty());
        }

        #[test]
        fn a_failure_in_the_first_segment_stops_the_chain() {
            let mut pipeline = classifying_pipeline("p", Some("x"));
            pipeline.pdf_extract = PdfExtractSegment::new();
            pipeline.pdf_extract.push(FailingExtract::fatal("boom"));

            let err = pipeline.apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap_err();
            assert_eq!(err.pipe(), "boom");
        }

        #[test]
        fn the_filter_data_reaches_the_text_filter_segment() {
            let filter = RecordingFilter::new("filter");
            let mut pipeline = Pipeline::new("p");
            pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
            pipeline.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            pipeline.deserialize.push(ConstantClassifier::pipe("classify", None));

            let previous = vec![Extracted::PageClass(None), Extracted::PageClass(None)];
            pipeline.apply(&page_with(&["a"]), &FilterData::Previous(&previous)).unwrap();
            assert_eq!(filter.seen(), vec![(0, 2)]);
        }
    }

    mod per_segment_api {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn apply_pdf_extract_stops_after_the_first_segment() {
            let pipeline = classifying_pipeline("p", Some("x"));
            let blocks = pipeline.apply_pdf_extract(&page_with(&["a", "b"])).unwrap();
            let contents: Vec<&str> =
                blocks.iter().map(|b| b.content.as_str().unwrap()).collect();
            assert_eq!(contents, vec!["a", "b"]);
        }

        #[test]
        fn apply_text_filter_stops_after_the_second_segment() {
            let pipeline = classifying_pipeline("p", Some("x"));
            let blocks =
                pipeline.apply_text_filter(&page_with(&["a", "b"]), &FilterData::EMPTY).unwrap();
            assert_eq!(blocks.len(), 2);
            assert_eq!(blocks[0].type_block, BlockType::PAGE_CLASS);
        }
    }

    mod merging {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn merging_unions_all_three_segments() {
            let left = classifying_pipeline("p", Some("x"));
            let right = classifying_pipeline("p", Some("y"));

            let merged = left + right;
            assert_eq!(merged.pdf_extract.len(), 2);
            assert_eq!(merged.text_filter.len(), 2);
            assert_eq!(merged.deserialize.len(), 2);
        }

        #[test]
        fn merging_keeps_the_left_hand_name() {
            let merged = Pipeline::new("left") + Pipeline::new("right");
            assert_eq!(merged.name, PipelineName::new("left"));
        }

        #[test]
        fn a_pipe_shared_by_both_pipelines_is_kept_once() {
            let shared = LinesToBlocks::pipe("shared");
            let mut left = Pipeline::new("p");
            left.pdf_extract.push(Arc::clone(&shared));
            let mut right = Pipeline::new("p");
            right.pdf_extract.push(shared);

            assert_eq!((left + right).pdf_extract.len(), 1);
        }

        #[test]
        fn merging_an_incomplete_pipeline_can_complete_it() {
            // E' esattamente il caso di `PLAN.md` §6.4: structured porta un segmento,
            // semistructured un altro, e solo la somma e' completa.
            let mut structured = Pipeline::new("p");
            structured.pdf_extract.push(LinesToBlocks::pipe("extract"));
            let mut semistructured = Pipeline::new("p");
            semistructured
                .text_filter
                .push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            semistructured.deserialize.push(ConstantClassifier::pipe("classify", None));

            assert!(!structured.is_complete());
            assert!(!semistructured.is_complete());
            assert!((structured + semistructured).is_complete());
        }

        #[test]
        fn merging_two_empty_pipelines_yields_an_empty_one() {
            let merged = Pipeline::new("p") + Pipeline::new("p");
            assert!(!merged.is_complete());
            assert_eq!(merged.pdf_extract.len(), 0);
        }
    }
}
