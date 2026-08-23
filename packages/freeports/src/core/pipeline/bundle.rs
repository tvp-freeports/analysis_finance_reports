//! [`PipelinesBundle`]: le pipeline che vengono applicate alla stessa pagina.
//!
//! `PLAN.md` §5.3. Un bundle è ciò che una page class "vale": l'insieme delle pipeline che
//! elaborano le pagine di quella class. `Algorithm` ne tiene uno per page class, più quello della
//! classificazione.
//!
//! **Deduplicazione per nome**, non per identità come nel riferimento. Là un bundle contiene
//! riferimenti Python (`Py<Pipeline>`) e l'identità è l'unica nozione disponibile; qui le
//! pipeline arrivano sempre da una mappa `nome → Pipeline` (`Algorithm::new` risolve una lista di
//! nomi), quindi "stessa pipeline" e "stesso nome" sono la stessa cosa — e il nome è anche ciò
//! che rende leggibili i messaggi d'errore (`PLAN.md` D6).

use crate::core::classes::{PdfBlock, TextBlock};
use crate::core::page::Page;

use super::data::{Extracted, FilterData, PipeError};
use super::{Pipeline, PipelineName};

/// Un insieme ordinato e deduplicato di [`Pipeline`] eseguite sulla stessa pagina.
#[derive(Debug, Clone, Default)]
pub struct PipelinesBundle(Vec<Pipeline>);

impl PipelinesBundle {
    pub fn new() -> Self {
        PipelinesBundle::default()
    }

    /// Aggiunge una pipeline in coda, se non ce n'è già una con lo stesso nome. Restituisce
    /// `true` se è stata davvero aggiunta.
    pub fn push(&mut self, pipeline: Pipeline) -> bool {
        if self.0.iter().any(|existing| existing.name == pipeline.name) {
            return false;
        }
        self.0.push(pipeline);
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = &Pipeline> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// La pipeline con questo nome, se il bundle la contiene.
    pub fn get(&self, name: &PipelineName) -> Option<&Pipeline> {
        self.0.iter().find(|p| &p.name == name)
    }

    /// Vero se **ogni** pipeline del bundle è completa. Un bundle vuoto è completo per vacuità:
    /// è `Algorithm::new` a decidere se un bundle vuoto sia accettabile, non questo metodo.
    pub fn is_complete(&self) -> bool {
        self.0.iter().all(Pipeline::is_complete)
    }

    /// I nomi delle pipeline incomplete, per i messaggi d'errore di chi carica il repo formati.
    pub fn incomplete(&self) -> Vec<&PipelineName> {
        self.0.iter().filter(|p| !p.is_complete()).map(|p| &p.name).collect()
    }

    /// La catena completa di ogni pipeline, concatenata nell'ordine di inserimento.
    pub fn apply(&self, page: &Page, data: &FilterData<'_>) -> Result<Vec<Extracted>, PipeError> {
        let mut out = Vec::new();
        for pipeline in &self.0 {
            out.extend(pipeline.apply(page, data)?);
        }
        Ok(out)
    }

    /// Solo il primo segmento di ogni pipeline — API di test per segmento (`freeports-dev`).
    pub fn apply_pdf_extract(&self, page: &Page) -> Result<Vec<PdfBlock>, PipeError> {
        let mut out = Vec::new();
        for pipeline in &self.0 {
            out.extend(pipeline.apply_pdf_extract(page)?);
        }
        Ok(out)
    }

    /// I primi due segmenti di ogni pipeline — API di test per segmento.
    pub fn apply_text_filter(
        &self,
        page: &Page,
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        let mut out = Vec::new();
        for pipeline in &self.0 {
            out.extend(pipeline.apply_text_filter(page, data)?);
        }
        Ok(out)
    }

    /// La catena completa — API di test per segmento.
    ///
    /// Coincide con [`PipelinesBundle::apply`]. Nel riferimento le due funzioni differiscono
    /// perché `__call__` conserva i `None` restituiti dai pipe di deserializzazione e
    /// `apply_deserialize` li filtra; qui quei `None` non esistono (un pipe che non ha nulla da
    /// dire restituisce un vettore vuoto), quindi non c'è niente da filtrare. Il metodo resta
    /// perché fa parte delle tre API parziali che `freeports-dev` usa (`PLAN.md` §5.3).
    pub fn apply_deserialize(
        &self,
        page: &Page,
        data: &FilterData<'_>,
    ) -> Result<Vec<Extracted>, PipeError> {
        self.apply(page, data)
    }
}

impl FromIterator<Pipeline> for PipelinesBundle {
    fn from_iter<I: IntoIterator<Item = Pipeline>>(iter: I) -> Self {
        let mut bundle = PipelinesBundle::new();
        for pipeline in iter {
            bundle.push(pipeline);
        }
        bundle
    }
}

#[cfg(test)]
mod tests {
    use super::super::segment::test_pipes::*;
    use super::*;
    use crate::core::pipeline::TextFilterPipe;
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

    fn classifying_pipeline(name: &str, class: Option<&str>) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(ConstantClassifier::pipe("classify", class));
        pipeline
    }

    mod membership {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn pipelines_keep_their_insertion_order() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("a", None), classifying_pipeline("b", None)]
                    .into_iter()
                    .collect();
            let names: Vec<&str> = bundle.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, vec!["a", "b"]);
        }

        #[test]
        fn a_second_pipeline_with_the_same_name_is_rejected() {
            let mut bundle = PipelinesBundle::new();
            assert!(bundle.push(classifying_pipeline("a", None)));
            assert!(!bundle.push(classifying_pipeline("a", Some("other"))));
            assert_eq!(bundle.len(), 1);
        }

        #[test]
        fn a_pipeline_is_retrievable_by_name() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("a", None)].into_iter().collect();
            assert!(bundle.get(&PipelineName::new("a")).is_some());
            assert!(bundle.get(&PipelineName::new("b")).is_none());
        }

        #[test]
        fn a_fresh_bundle_is_empty() {
            assert!(PipelinesBundle::new().is_empty());
            assert_eq!(PipelinesBundle::default().len(), 0);
        }
    }

    mod completeness {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_bundle_of_complete_pipelines_is_complete() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("a", None)].into_iter().collect();
            assert!(bundle.is_complete());
            assert!(bundle.incomplete().is_empty());
        }

        #[test]
        fn an_incomplete_pipeline_is_reported_by_name() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("good", None), Pipeline::new("bad")].into_iter().collect();
            assert!(!bundle.is_complete());
            assert_eq!(bundle.incomplete(), vec![&PipelineName::new("bad")]);
        }

        #[test]
        fn an_empty_bundle_is_vacuously_complete() {
            assert!(PipelinesBundle::new().is_complete());
        }
    }

    mod application {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_results_of_every_pipeline_are_concatenated_in_order() {
            let bundle: PipelinesBundle =
                [classifying_pipeline("a", Some("x")), classifying_pipeline("b", Some("y"))]
                    .into_iter()
                    .collect();

            let out = bundle.apply(&page_with(&["line"]), &FilterData::EMPTY).unwrap();
            assert_eq!(
                out,
                vec![
                    Extracted::PageClass(Some(PageClass::new("x"))),
                    Extracted::PageClass(Some(PageClass::new("y"))),
                ]
            );
        }

        #[test]
        fn an_empty_bundle_produces_nothing() {
            let out = PipelinesBundle::new().apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap();
            assert!(out.is_empty());
        }

        #[test]
        fn a_failing_pipeline_stops_the_bundle() {
            let mut failing = classifying_pipeline("bad", None);
            failing.pdf_extract = Default::default();
            failing.pdf_extract.push(FailingExtract::fatal("boom"));

            let bundle: PipelinesBundle =
                [failing, classifying_pipeline("never reached", None)].into_iter().collect();
            let err = bundle.apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap_err();
            assert_eq!(err.pipe(), "boom");
        }

        #[test]
        fn a_page_failure_travels_out_of_the_bundle_unchanged() {
            let mut failing = classifying_pipeline("bad", None);
            failing.pdf_extract = Default::default();
            failing.pdf_extract.push(FailingExtract::page_parse("skipper"));

            let bundle: PipelinesBundle = [failing].into_iter().collect();
            let err = bundle.apply(&page_with(&["a"]), &FilterData::EMPTY).unwrap_err();
            assert!(err.is_page_failure());
        }
    }

    mod per_segment_api {
        use super::*;
        use pretty_assertions::assert_eq;

        fn two_pipeline_bundle() -> PipelinesBundle {
            [classifying_pipeline("a", Some("x")), classifying_pipeline("b", Some("y"))]
                .into_iter()
                .collect()
        }

        #[test]
        fn apply_pdf_extract_concatenates_the_first_segment_of_every_pipeline() {
            let blocks = two_pipeline_bundle().apply_pdf_extract(&page_with(&["l"])).unwrap();
            assert_eq!(blocks.len(), 2);
        }

        #[test]
        fn apply_text_filter_concatenates_the_first_two_segments() {
            let blocks = two_pipeline_bundle()
                .apply_text_filter(&page_with(&["l"]), &FilterData::EMPTY)
                .unwrap();
            assert_eq!(blocks.len(), 2);
        }

        #[test]
        fn apply_deserialize_matches_apply_because_there_are_no_nones_to_filter() {
            let bundle = two_pipeline_bundle();
            let page = page_with(&["l"]);
            assert_eq!(
                bundle.apply_deserialize(&page, &FilterData::EMPTY).unwrap(),
                bundle.apply(&page, &FilterData::EMPTY).unwrap()
            );
        }
    }
}
