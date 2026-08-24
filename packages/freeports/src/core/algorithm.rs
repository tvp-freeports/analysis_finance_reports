//! [`Algorithm`]: classificazione delle pagine, schedule, dispatch ai bundle per page class.
//!
//! `PLAN.md` §5.5. È il livello che tiene insieme tutto il resto del motore: sa quali pipeline
//! classificano le pagine, in che ordine le page class vanno elaborate, e quale bundle spetta a
//! ciascuna.
//!
//! **Multi-documento nativo dal primo giorno** (`PLAN.md` D7, `targets/2_multireport_support.md`).
//! [`Algorithm::apply`] è il caso particolare di [`Algorithm::apply_multidocument`] con un solo
//! documento, non una seconda implementazione: la classificazione avviene **per documento** (il
//! finalizer gira una volta per documento, non una volta per tutte le pagine messe insieme),
//! mentre lo schedule lavora sull'**unione** delle pagine di tutti i documenti.
//!
//! **Cosa non è M5.**
//!
//! - `Algorithm::load` (`PLAN.md` §5.5) legge il repo formati, cioè
//!   `formats_repo::{structured,semistructured,unstructured}`: è M7. M5 fornisce
//!   [`Algorithm::new`] con le validazioni che il riferimento fa in `Algorithm.__new__`.
//! - La verifica che ogni pipeline sia **completa** (`PLAN.md` §6.4) sta in `load`, non qui: il
//!   riferimento la fa a monte, quando acquisisce le pipeline. [`PipelinesBundle::incomplete`]
//!   esiste perché M7 possa farla con un messaggio utile.
//! - [`PageClassFinalizer::Python`] del piano diventa qui [`PageClassFinalizer::Custom`], che
//!   prende un [`PageClassFinalize`] qualunque: il callable dell'autore arriva con M7 e vi si
//!   innesta senza che `core` conosca PyO3.
//!
//! **Risultati per pagina — divergenza voluta dal riferimento** (decisione dell'utente
//! 2026-08-23, `agent-memory/M5-implementation-plan.md` D-M5-3). Il riferimento accumula i
//! risultati in un dict `res[(doc, page)] = risultati_dello_step`, con **assegnazione**: se una
//! page class compare in due step, la stessa pagina viene elaborata due volte e i risultati del
//! primo step spariscono dall'output (pur avendo alimentato il `filter_data` del secondo). Qui i
//! risultati si **accumulano** in [`PageOutcome::results`]. Nei casi normali — una page class in
//! un solo step — il comportamento è identico; differisce solo nel caso che oggi perde dati.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::core::classes::{PdfBlock, TextBlock};
use crate::core::page::{Document, DocumentId, FormatName, Page};
use crate::core::pipeline::bundle::PipelinesBundle;
use crate::core::pipeline::{Extracted, FilterData, Pipeline, PipeError, PipelineName};
use crate::core::schedule::{PageClass, Schedule, ScheduleError};
use crate::formats_utils::text_filter::matcher::CompanyMatchInfos;

/// Chi ha l'ultima parola sulla classificazione delle pagine di un documento.
///
/// Riceve i contributi grezzi prodotti dalle pipeline di classificazione — che possono essere in
/// numero diverso dalle pagine — e deve restituire **esattamente una** class per pagina.
pub trait PageClassFinalize: Send + Sync {
    fn finalize(
        &self,
        classes: Vec<Option<PageClass>>,
    ) -> Result<Vec<Option<PageClass>>, PipeError>;
}

/// Il finalizer di un formato: quello banale, o quello scritto dall'autore.
#[derive(Clone)]
pub enum PageClassFinalizer {
    /// Nessuna finalizzazione: i contributi delle pipeline di classificazione sono già la
    /// classificazione finale, una per pagina.
    Identity,
    /// Un finalizer fornito dal formato. M7 ci innesta il `compute_page_class` dell'autore.
    Custom(Arc<dyn PageClassFinalize>),
}

impl PageClassFinalizer {
    pub fn finalize(
        &self,
        classes: Vec<Option<PageClass>>,
    ) -> Result<Vec<Option<PageClass>>, PipeError> {
        match self {
            PageClassFinalizer::Identity => Ok(classes),
            PageClassFinalizer::Custom(finalizer) => finalizer.finalize(classes),
        }
    }
}

impl std::fmt::Debug for PageClassFinalizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageClassFinalizer::Identity => f.write_str("PageClassFinalizer::Identity"),
            PageClassFinalizer::Custom(_) => f.write_str("PageClassFinalizer::Custom(..)"),
        }
    }
}

/// I risultati di **una** pagina.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageOutcome {
    /// Numero di pagina 1-based.
    pub page: u32,
    /// La class con cui la pagina è stata schedulata.
    pub class: PageClass,
    /// I risultati prodotti. Vuoto se la pagina è stata saltata per un fallimento non fatale, o
    /// se i pipe non avevano nulla da dire.
    pub results: Vec<Extracted>,
}

/// I risultati di **un** documento: la forma tipizzata del dict `{(doc_name, page_n): [...]}`
/// del riferimento.
///
/// `output::routines` (M8) lo convertirà in `DocumentResults`, che è il tipo destinato alla
/// scrittura dei CSV; qui non esiste ancora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentOutcome {
    pub id: DocumentId,
    pub format: FormatName,
    /// Solo le pagine effettivamente **schedulate**, in ordine di numero di pagina. Una pagina
    /// non classificata (class `None`) non entra in nessuno step e non compare qui.
    pub pages: Vec<PageOutcome>,
}

/// L'algoritmo di estrazione di un formato.
#[derive(Debug, Clone)]
pub struct Algorithm {
    format: FormatName,
    page_classify: PipelinesBundle,
    page_class_finalizer: PageClassFinalizer,
    schedule: Schedule,
    bundles: BTreeMap<PageClass, PipelinesBundle>,
}

impl Algorithm {
    /// Costruisce l'algoritmo a partire dalle pipeline già risolte, replicando le tre validazioni
    /// del riferimento:
    ///
    /// 1. ogni pipeline di classificazione ha un'implementazione;
    /// 2. le page class dello schedule e quelle del mapping coincidono **esattamente**;
    /// 3. non esistono pipeline senza implementazione né implementazioni mai usate.
    pub fn new(
        format: impl Into<FormatName>,
        pipelines: BTreeMap<PipelineName, Pipeline>,
        page_classify_pipelines: &[PipelineName],
        page_class_finalizer: PageClassFinalizer,
        schedule: Schedule,
        mapping: BTreeMap<PageClass, Vec<PipelineName>>,
    ) -> Result<Self, AlgorithmError> {
        let known: BTreeSet<PipelineName> = pipelines.keys().cloned().collect();
        let classify_names: BTreeSet<PipelineName> =
            page_classify_pipelines.iter().cloned().collect();

        let unknown: Vec<PipelineName> = classify_names.difference(&known).cloned().collect();
        if !unknown.is_empty() {
            return Err(AlgorithmError::UnknownPageClassifyPipelines { unknown });
        }

        let scheduled_classes = schedule.page_classes();
        let mapped_classes: BTreeSet<PageClass> = mapping.keys().cloned().collect();
        if scheduled_classes != mapped_classes {
            let difference: Vec<PageClass> =
                scheduled_classes.symmetric_difference(&mapped_classes).cloned().collect();
            return Err(AlgorithmError::ScheduleMappingMismatch { difference });
        }

        let mut used: BTreeSet<PipelineName> = classify_names.clone();
        for names in mapping.values() {
            used.extend(names.iter().cloned());
        }
        if used != known {
            return Err(AlgorithmError::PipelineNamesMismatch {
                unmapped: used.difference(&known).cloned().collect(),
                unused: known.difference(&used).cloned().collect(),
            });
        }

        let resolve = |names: &[PipelineName]| -> PipelinesBundle {
            names
                .iter()
                .map(|n| pipelines.get(n).expect("membership just validated above").clone())
                .collect()
        };

        let page_classify = resolve(page_classify_pipelines);
        let bundles = mapping
            .iter()
            .map(|(class, names)| (class.clone(), resolve(names)))
            .collect();

        Ok(Algorithm {
            format: format.into(),
            page_classify,
            page_class_finalizer,
            schedule,
            bundles,
        })
    }

    pub fn format(&self) -> &FormatName {
        &self.format
    }

    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// Il bundle di una page class, o l'errore che lo dice.
    fn bundle(&self, class: &PageClass) -> Result<&PipelinesBundle, AlgorithmError> {
        self.bundles
            .get(class)
            .ok_or_else(|| AlgorithmError::UnmappedPageClass { class: class.clone() })
    }

    /// Classifica le pagine di **un** documento e applica il finalizer.
    pub fn classify_pages(
        &self,
        doc: &Document,
    ) -> Result<Vec<Option<PageClass>>, AlgorithmError> {
        let mut raw = Vec::with_capacity(doc.pages.len());
        for page in &doc.pages {
            for result in self.page_classify.apply(page, &FilterData::EMPTY)? {
                match result {
                    Extracted::PageClass(class) => raw.push(class),
                    other => {
                        return Err(AlgorithmError::NotAPageClassification {
                            document: doc.id.to_string(),
                            page: page.number,
                            found: format!("{other:?}"),
                        });
                    }
                }
            }
        }

        let classified = self.page_class_finalizer.finalize(raw)?;
        if classified.len() != doc.pages.len() {
            return Err(AlgorithmError::ClassificationCountMismatch {
                document: doc.id.to_string(),
                pages: doc.pages.len(),
                classifications: classified.len(),
            });
        }
        Ok(classified)
    }

    /// Classifica più documenti. Il finalizer gira **per documento**, non sull'unione delle
    /// pagine: è ciò che chiede `targets/2_multireport_support.md`.
    pub fn classify_pages_multidocument(
        &self,
        docs: &[Document],
    ) -> Result<Vec<Vec<Option<PageClass>>>, AlgorithmError> {
        docs.iter().map(|doc| self.classify_pages(doc)).collect()
    }

    /// La pipeline completa su un solo documento — caso particolare di
    /// [`Algorithm::apply_multidocument`].
    pub fn apply(
        &self,
        doc: &Document,
        companies: &[CompanyMatchInfos],
    ) -> Result<DocumentOutcome, AlgorithmError> {
        let mut outcomes = self.apply_multidocument(std::slice::from_ref(doc), companies)?;
        Ok(outcomes.remove(0))
    }

    /// Classificazione **per documento**, schedule sull'**unione** delle pagine.
    pub fn apply_multidocument(
        &self,
        docs: &[Document],
        companies: &[CompanyMatchInfos],
    ) -> Result<Vec<DocumentOutcome>, AlgorithmError> {
        let classifications = self.classify_pages_multidocument(docs)?;
        let scheduled = self.schedule.assign(docs, &classifications)?;

        // `(indice documento, numero pagina) -> (class, risultati accumulati)`. La chiave è
        // l'indice e non l'id perché due documenti possono legittimamente avere lo stesso id.
        let mut per_page: BTreeMap<(usize, u32), (PageClass, Vec<Extracted>)> = BTreeMap::new();
        // Il `filter_data` degli step successivi al primo: l'accumulo di *tutti* gli step
        // precedenti, non solo dell'ultimo (D-M5-1).
        let mut previous: Vec<Extracted> = Vec::new();

        for (step_index, step_pages) in scheduled.iter().enumerate() {
            let mut produced_in_this_step: Vec<Extracted> = Vec::new();
            for scheduled_page in step_pages {
                let bundle = self.bundle(&scheduled_page.class)?;
                let data = if step_index == 0 {
                    FilterData::TargetCompanies(companies)
                } else {
                    FilterData::Previous(&previous)
                };

                // Lo span dà a ogni evento prodotto dai pipe di questa pagina il numero di
                // pagina, che è la colonna `Page` del `.log.csv`: nessun pipe lo conosce da sé,
                // e passarlo a mano fino in fondo vorrebbe dire aggiungerlo a ogni firma.
                let page_span = tracing::info_span!("page", page = scheduled_page.page.number);
                let _page_guard = page_span.enter();

                let results = match bundle.apply(scheduled_page.page, &data) {
                    Ok(results) => results,
                    Err(error) if error.is_page_failure() => {
                        // Non fatale: si logga e si prosegue. A differenza del riferimento — dove
                        // la gerarchia di logger Python era scollegata e il messaggio non
                        // raggiungeva `.log.csv` — qui il warning arriva davvero (`PLAN.md` §5.5).
                        tracing::warn!(
                            document = %scheduled_page.doc.id,
                            page = scheduled_page.page.number,
                            error = %error,
                            "pagina saltata"
                        );
                        Vec::new()
                    }
                    Err(error) => return Err(error.into()),
                };

                produced_in_this_step.extend(results.iter().cloned());
                let entry = per_page
                    .entry((scheduled_page.doc_index, scheduled_page.page.number))
                    .or_insert_with(|| (scheduled_page.class.clone(), Vec::new()));
                entry.1.extend(results);
            }
            previous.extend(produced_in_this_step);
        }

        let mut outcomes: Vec<DocumentOutcome> = docs
            .iter()
            .map(|doc| DocumentOutcome {
                id: doc.id.clone(),
                format: doc.format.clone(),
                pages: Vec::new(),
            })
            .collect();
        // `per_page` è una `BTreeMap`: iterandola le pagine escono già ordinate per
        // `(documento, numero di pagina)`, che è l'ordine in cui vanno scritte.
        for ((doc_index, page), (class, results)) in per_page {
            outcomes[doc_index].pages.push(PageOutcome { page, class, results });
        }
        Ok(outcomes)
    }

    /// API di test per segmento: solo `pdf_extract`, per la page class data.
    pub fn apply_pdf_extract(
        &self,
        page: &Page,
        class: &PageClass,
    ) -> Result<Vec<PdfBlock>, AlgorithmError> {
        Ok(self.bundle(class)?.apply_pdf_extract(page)?)
    }

    /// API di test per segmento: `pdf_extract` + `text_filter`, per la page class data.
    pub fn apply_text_filter(
        &self,
        page: &Page,
        class: &PageClass,
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, AlgorithmError> {
        Ok(self.bundle(class)?.apply_text_filter(page, data)?)
    }

    /// API di test per segmento: la catena **completa** di ogni pipeline della page class data.
    ///
    /// Non è la stessa cosa di incatenare a mano [`Self::apply_text_filter`] e
    /// [`Self::apply_deserializer`], e la differenza conta quando una page class mappa **più di
    /// una** pipeline — come `merges` di KAIROS-EN23, che ne mappa due (`renames` e `merges`).
    /// Incatenandoli a mano, i blocchi di testo di *tutte* le pipeline finiscono in un mucchio
    /// solo e ogni pipe `deserialize` li vede tutti, compresi quelli che non sono suoi: due
    /// eventi diventano quattro entità. Qui invece ogni pipeline resta una catena chiusa, come
    /// nella pipeline vera ([`Self::apply`]) e come nel riferimento.
    pub fn apply_deserialize(
        &self,
        page: &Page,
        class: &PageClass,
        data: &FilterData<'_>,
    ) -> Result<Vec<Extracted>, AlgorithmError> {
        Ok(self.bundle(class)?.apply_deserialize(page, data)?)
    }

    /// API di test per segmento: solo `deserialize`, a partire da blocchi di testo già pronti.
    ///
    /// La firma è quella di `PLAN.md` §5.5 (blocchi in ingresso), non quella del riferimento
    /// (che riparte dalla pagina): così i tre metodi decompongono davvero la catena in tre pezzi
    /// concatenabili, invece di ripetere due volte i segmenti a monte.
    pub fn apply_deserializer(
        &self,
        blocks: &[TextBlock],
        class: &PageClass,
    ) -> Result<Vec<Extracted>, AlgorithmError> {
        let mut out = Vec::new();
        for pipeline in self.bundle(class)?.iter() {
            out.extend(pipeline.deserialize.apply(blocks)?);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AlgorithmError {
    #[error(transparent)]
    Pipe(#[from] PipeError),
    #[error(transparent)]
    Schedule(#[from] ScheduleError),
    #[error("some page classify pipelines have no mapping to a pipeline implementation: {unknown:?}")]
    UnknownPageClassifyPipelines { unknown: Vec<PipelineName> },
    #[error("page classes in the schedule have to be mapped to pipeline names; the difference is {difference:?}")]
    ScheduleMappingMismatch { difference: Vec<PageClass> },
    #[error(
        "there are pipeline names not mapped to an implementation or mapped and never used. Unmapped: {unmapped:?} Not used: {unused:?}"
    )]
    PipelineNamesMismatch { unmapped: Vec<PipelineName>, unused: Vec<PipelineName> },
    #[error(
        "document `{document}` has {pages} pages but the finalizer returned {classifications} classifications"
    )]
    ClassificationCountMismatch { document: String, pages: usize, classifications: usize },
    #[error("page {page} of document `{document}`: a page classify pipeline returned {found}, not a page class")]
    NotAPageClassification { document: String, page: u32, found: String },
    #[error("page class `{class}` has no pipelines bundle")]
    UnmappedPageClass { class: PageClass },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::BlockType;
    use crate::core::pipeline::TextFilterPipe;
    use crate::core::pipeline::segment::test_pipes::*;
    use crate::core::schedule::ScheduleStep;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::text_filter::matcher::TargetCompanyInput;

    fn page(number: u32, texts: &[&str]) -> Page {
        let lines = texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let y = i as f32 * 10.0;
                PdfLine::new("Arial", 10.0, t, (0.0, y, 10.0, y + 10.0))
            })
            .collect();
        Page::new(number, (100.0, 100.0), lines, vec![])
    }

    fn doc(id: &str, pages: Vec<Page>) -> Document {
        Document::new(id, "FMT", pages)
    }

    fn companies() -> Vec<CompanyMatchInfos> {
        CompanyMatchInfos::compile_from_target_companies(vec![TargetCompanyInput {
            name: "Acme".to_string(),
            regexs: vec![],
            symbols: vec![],
            buds: vec![],
        }])
        .expect("fixed, valid input")
    }

    /// Una pipeline completa che classifica ogni blocco con `class`.
    fn classifying_pipeline(name: &str, class: Option<&str>) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(ConstantClassifier::pipe("classify", class));
        pipeline
    }

    /// Una pipeline completa che deposita una promessa per blocco.
    fn promising_pipeline(name: &str, id: &str) -> Pipeline {
        let mut pipeline = Pipeline::new(name);
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(PromiseDepositor::pipe("promise", id));
        pipeline
    }

    fn step(classes: &[&str]) -> ScheduleStep {
        classes.iter().copied().collect()
    }

    /// L'algoritmo minimo usato dalla maggior parte dei test: la pipeline `classify` classifica
    /// tutto come `"a"`, la pipeline `work` elabora le pagine di class `"a"`.
    fn simple_algorithm() -> Algorithm {
        Algorithm::new(
            "FMT",
            BTreeMap::from([
                (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                (PipelineName::new("work"), promising_pipeline("work", "id")),
            ]),
            &[PipelineName::new("classify")],
            PageClassFinalizer::Identity,
            Schedule::new(vec![step(&["a"])]),
            BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
        )
        .expect("fixture is consistent")
    }

    mod construction_validation {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_consistent_configuration_is_accepted() {
            assert_eq!(simple_algorithm().format(), &FormatName::new("FMT"));
        }

        #[test]
        fn a_page_classify_pipeline_without_implementation_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([(PipelineName::new("work"), promising_pipeline("work", "id"))]),
                &[PipelineName::new("ghost")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::UnknownPageClassifyPipelines {
                    unknown: vec![PipelineName::new("ghost")]
                }
            );
        }

        #[test]
        fn a_page_class_in_the_schedule_but_not_in_the_mapping_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a", "b"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::ScheduleMappingMismatch {
                    difference: vec![PageClass::new("b")]
                }
            );
        }

        #[test]
        fn a_page_class_in_the_mapping_but_not_in_the_schedule_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([
                    (PageClass::new("a"), vec![PipelineName::new("work")]),
                    (PageClass::new("b"), vec![PipelineName::new("work")]),
                ]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::ScheduleMappingMismatch {
                    difference: vec![PageClass::new("b")]
                }
            );
        }

        #[test]
        fn a_pipeline_implementation_nobody_uses_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                    (PipelineName::new("idle"), promising_pipeline("idle", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::PipelineNamesMismatch {
                    unmapped: vec![],
                    unused: vec![PipelineName::new("idle")],
                }
            );
        }

        #[test]
        fn a_mapped_pipeline_without_implementation_is_rejected() {
            let err = Algorithm::new(
                "FMT",
                BTreeMap::from([(
                    PipelineName::new("classify"),
                    classifying_pipeline("classify", Some("a")),
                )]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("ghost")])]),
            )
            .unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::PipelineNamesMismatch {
                    unmapped: vec![PipelineName::new("ghost")],
                    unused: vec![],
                }
            );
        }

        #[test]
        fn a_pipeline_may_serve_both_classification_and_a_page_class() {
            // Nessuna delle tre validazioni lo vieta, e il riferimento nemmeno: la pipeline
            // compare sia fra quelle di classificazione sia nel mapping.
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([(
                    PipelineName::new("both"),
                    classifying_pipeline("both", Some("a")),
                )]),
                &[PipelineName::new("both")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("both")])]),
            );
            assert!(algorithm.is_ok());
        }
    }

    mod classification {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_page_gets_the_class_its_pipelines_produced() {
            let algorithm = simple_algorithm();
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"])]);
            assert_eq!(
                algorithm.classify_pages(&document).unwrap(),
                vec![Some(PageClass::new("a")), Some(PageClass::new("a"))]
            );
        }

        #[test]
        fn a_page_the_pipelines_could_not_classify_stays_unclassified() {
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", None)),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();
            assert_eq!(algorithm.classify_pages(&doc("d", vec![page(1, &["x"])])).unwrap(), vec![None]);
        }

        #[test]
        fn a_document_with_no_pages_classifies_to_nothing() {
            assert!(simple_algorithm().classify_pages(&doc("d", vec![])).unwrap().is_empty());
        }

        #[test]
        fn one_classification_per_page_is_required_after_the_finalizer() {
            // Due righe per pagina -> due contributi per pagina: con il finalizer identita' il
            // conto non torna, ed e' un errore.
            let algorithm = simple_algorithm();
            let err = algorithm.classify_pages(&doc("d", vec![page(1, &["x", "y"])])).unwrap_err();
            assert_eq!(
                err,
                AlgorithmError::ClassificationCountMismatch {
                    document: "d".to_string(),
                    pages: 1,
                    classifications: 2,
                }
            );
        }

        #[test]
        fn a_custom_finalizer_can_reconcile_the_count() {
            struct KeepFirstPerPage;
            impl PageClassFinalize for KeepFirstPerPage {
                fn finalize(
                    &self,
                    classes: Vec<Option<PageClass>>,
                ) -> Result<Vec<Option<PageClass>>, PipeError> {
                    Ok(classes.into_iter().step_by(2).collect())
                }
            }

            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Custom(Arc::new(KeepFirstPerPage)),
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            assert_eq!(
                algorithm.classify_pages(&doc("d", vec![page(1, &["x", "y"])])).unwrap(),
                vec![Some(PageClass::new("a"))]
            );
        }

        #[test]
        fn a_failing_finalizer_surfaces_its_error() {
            struct AlwaysFails;
            impl PageClassFinalize for AlwaysFails {
                fn finalize(
                    &self,
                    _classes: Vec<Option<PageClass>>,
                ) -> Result<Vec<Option<PageClass>>, PipeError> {
                    Err(PipeError::author("classify", "compute_page_class", "KeyError"))
                }
            }

            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Custom(Arc::new(AlwaysFails)),
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let err = algorithm.classify_pages(&doc("d", vec![page(1, &["x"])])).unwrap_err();
            assert!(matches!(err, AlgorithmError::Pipe(PipeError::Author { .. })));
        }

        #[test]
        fn a_non_classification_result_from_a_classify_pipeline_is_an_error() {
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    // La pipeline di classificazione deposita promesse invece di classificare.
                    (PipelineName::new("classify"), promising_pipeline("classify", "id")),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let err = algorithm.classify_pages(&doc("d", vec![page(1, &["x"])])).unwrap_err();
            assert!(matches!(err, AlgorithmError::NotAPageClassification { page: 1, .. }));
        }

        #[test]
        fn each_document_is_finalized_on_its_own() {
            // Il finalizer registra quante classificazioni riceve per chiamata: due documenti da
            // una pagina devono produrre due chiamate da una, non una da due.
            struct RecordingFinalizer(std::sync::Mutex<Vec<usize>>);
            impl PageClassFinalize for RecordingFinalizer {
                fn finalize(
                    &self,
                    classes: Vec<Option<PageClass>>,
                ) -> Result<Vec<Option<PageClass>>, PipeError> {
                    self.0.lock().expect("test-only mutex").push(classes.len());
                    Ok(classes)
                }
            }

            let finalizer = Arc::new(RecordingFinalizer(std::sync::Mutex::new(Vec::new())));
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Custom(Arc::clone(&finalizer) as Arc<dyn PageClassFinalize>),
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let docs = vec![doc("one", vec![page(1, &["x"])]), doc("two", vec![page(1, &["y"])])];
            algorithm.classify_pages_multidocument(&docs).unwrap();
            assert_eq!(*finalizer.0.lock().expect("test-only mutex"), vec![1, 1]);
        }
    }

    mod single_document_application {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn every_scheduled_page_produces_an_outcome() {
            let algorithm = simple_algorithm();
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"])]);
            let outcome = algorithm.apply(&document, &companies()).unwrap();

            assert_eq!(outcome.id, DocumentId::new("d"));
            assert_eq!(outcome.format, FormatName::new("FMT"));
            let numbers: Vec<u32> = outcome.pages.iter().map(|p| p.page).collect();
            assert_eq!(numbers, vec![1, 2]);
        }

        #[test]
        fn the_outcome_carries_the_class_the_page_was_scheduled_with() {
            let outcome =
                simple_algorithm().apply(&doc("d", vec![page(1, &["x"])]), &companies()).unwrap();
            assert_eq!(outcome.pages[0].class, PageClass::new("a"));
        }

        #[test]
        fn an_unclassified_page_produces_no_outcome_at_all() {
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", None)),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let outcome =
                algorithm.apply(&doc("d", vec![page(1, &["x"])]), &companies()).unwrap();
            assert!(outcome.pages.is_empty());
        }

        #[test]
        fn a_document_with_no_pages_yields_an_empty_outcome() {
            let outcome = simple_algorithm().apply(&doc("d", vec![]), &companies()).unwrap();
            assert!(outcome.pages.is_empty());
        }

        #[test]
        fn the_pages_of_an_outcome_are_ordered_by_page_number() {
            // Lo schedule le visita per class, non per numero: l'ordine finale deve comunque
            // essere quello di pagina.
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), alternating_classifier()),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                // `b` prima di `a`: la pagina 2 (class `b`) viene schedulata per prima.
                Schedule::new(vec![step(&["b", "a"])]),
                BTreeMap::from([
                    (PageClass::new("a"), vec![PipelineName::new("work")]),
                    (PageClass::new("b"), vec![PipelineName::new("work")]),
                ]),
            )
            .unwrap();

            let document = doc("d", vec![page(1, &["a"]), page(2, &["b"])]);
            let outcome = algorithm.apply(&document, &companies()).unwrap();
            let numbers: Vec<u32> = outcome.pages.iter().map(|p| p.page).collect();
            assert_eq!(numbers, vec![1, 2]);
        }
    }

    /// Classifica una pagina in base al testo della sua prima riga: `"a"` -> class `a`,
    /// qualunque altra cosa -> class `b`.
    fn alternating_classifier() -> Pipeline {
        struct ByFirstLine;
        impl crate::core::pipeline::DeserializePipe for ByFirstLine {
            fn name(&self) -> &str {
                "by-first-line"
            }
            fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
                let class = match block.content.as_str() {
                    Some("a") => PageClass::new("a"),
                    _ => PageClass::new("b"),
                };
                Ok(vec![Extracted::PageClass(Some(class))])
            }
        }

        let mut pipeline = Pipeline::new("classify");
        pipeline.pdf_extract.push(LinesToBlocks::pipe("extract"));
        pipeline.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
        pipeline.deserialize.push(Arc::new(ByFirstLine));
        pipeline
    }

    mod filter_data_across_steps {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Algoritmo a due step: la pagina 1 (class `a`) al primo, la pagina 2 (class `b`) al
        /// secondo. Il filtro registrante è condiviso, così si può leggere che cosa ha visto.
        fn two_step_algorithm(filter: Arc<RecordingFilter>) -> Algorithm {
            let mut work_a = Pipeline::new("work_a");
            work_a.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work_a.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            work_a.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            let mut work_b = Pipeline::new("work_b");
            work_b.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work_b.text_filter.push(filter as Arc<dyn TextFilterPipe>);
            work_b.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), alternating_classifier()),
                    (PipelineName::new("work_a"), work_a),
                    (PipelineName::new("work_b"), work_b),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"]), step(&["b"])]),
                BTreeMap::from([
                    (PageClass::new("a"), vec![PipelineName::new("work_a")]),
                    (PageClass::new("b"), vec![PipelineName::new("work_b")]),
                ]),
            )
            .unwrap()
        }

        #[test]
        fn the_first_step_sees_the_target_companies_and_the_next_one_sees_the_results() {
            let filter = RecordingFilter::new("filter");
            let algorithm = two_step_algorithm(Arc::clone(&filter));
            let document = doc("d", vec![page(1, &["a"]), page(2, &["b"])]);

            algorithm.apply(&document, &companies()).unwrap();
            // Prima chiamata (step 0): una target company, zero risultati precedenti.
            // Seconda chiamata (step 1): zero target companies, il risultato dello step 0.
            assert_eq!(filter.seen(), vec![(1, 0), (0, 1)]);
        }

        #[test]
        fn the_results_of_all_earlier_steps_accumulate_not_just_the_last_one() {
            let filter = RecordingFilter::new("filter");
            let mut work_c = Pipeline::new("work_c");
            work_c.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work_c.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            work_c.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            let mut work_ab = Pipeline::new("work_ab");
            work_ab.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work_ab.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            work_ab.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), alternating_classifier()),
                    (PipelineName::new("work_ab"), work_ab),
                    (PipelineName::new("work_c"), work_c),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"]), step(&["b"])]),
                BTreeMap::from([
                    (PageClass::new("a"), vec![PipelineName::new("work_ab")]),
                    (PageClass::new("b"), vec![PipelineName::new("work_c")]),
                ]),
            )
            .unwrap();

            // Due pagine di class `a` al primo step, una di class `b` al secondo: la pagina `b`
            // deve vedere entrambi i risultati precedenti.
            let document = doc("d", vec![page(1, &["a"]), page(2, &["a"]), page(3, &["b"])]);
            algorithm.apply(&document, &companies()).unwrap();
            assert_eq!(filter.seen(), vec![(1, 0), (1, 0), (0, 2)]);
        }

        #[test]
        fn pages_of_the_same_step_do_not_see_each_others_results() {
            // I risultati di uno step entrano nel `filter_data` solo allo step successivo.
            let filter = RecordingFilter::new("filter");
            let algorithm = two_step_algorithm(Arc::clone(&filter));
            let document = doc("d", vec![page(1, &["a"]), page(2, &["a"])]);

            algorithm.apply(&document, &companies()).unwrap();
            assert_eq!(filter.seen(), vec![(1, 0), (1, 0)]);
        }
    }

    mod multidocument {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn each_document_gets_its_own_outcome_in_input_order() {
            let algorithm = simple_algorithm();
            let docs = vec![
                doc("second", vec![page(1, &["x"])]),
                doc("first", vec![page(1, &["y"])]),
            ];
            let outcomes = algorithm.apply_multidocument(&docs, &companies()).unwrap();
            let ids: Vec<String> = outcomes.iter().map(|o| o.id.to_string()).collect();
            assert_eq!(ids, vec!["second", "first"]);
        }

        #[test]
        fn no_documents_yields_no_outcomes() {
            assert!(simple_algorithm().apply_multidocument(&[], &companies()).unwrap().is_empty());
        }

        #[test]
        fn two_documents_with_the_same_id_stay_separate() {
            let algorithm = simple_algorithm();
            let docs = vec![doc("same", vec![page(1, &["x"])]), doc("same", vec![page(1, &["y"])])];
            let outcomes = algorithm.apply_multidocument(&docs, &companies()).unwrap();
            assert_eq!(outcomes.len(), 2);
            assert_eq!(outcomes[0].pages.len(), 1);
            assert_eq!(outcomes[1].pages.len(), 1);
        }

        #[test]
        fn the_schedule_spans_the_union_of_the_pages_of_every_document() {
            // Il filtro vede una chiamata per pagina, di *tutti* i documenti, dentro lo stesso
            // step: lo schedule non riparte da capo per ogni documento.
            let filter = RecordingFilter::new("filter");
            let mut work = Pipeline::new("work");
            work.pdf_extract.push(LinesToBlocks::pipe("extract"));
            work.text_filter.push(Arc::clone(&filter) as Arc<dyn TextFilterPipe>);
            work.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), work),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let docs = vec![doc("one", vec![page(1, &["x"])]), doc("two", vec![page(1, &["y"])])];
            algorithm.apply_multidocument(&docs, &companies()).unwrap();
            assert_eq!(filter.seen(), vec![(1, 0), (1, 0)]);
        }
    }

    mod page_failures {
        use super::*;
        use pretty_assertions::assert_eq;

        fn algorithm_whose_work_pipeline_fails(page_failure: bool) -> Algorithm {
            let mut work = Pipeline::new("work");
            work.pdf_extract.push(if page_failure {
                FailingExtract::page_parse("skipper")
            } else {
                FailingExtract::fatal("boom")
            });
            work.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            work.deserialize.push(PromiseDepositor::pipe("promise", "id"));

            Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), work),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap()
        }

        #[test]
        fn a_page_failure_is_absorbed_and_the_page_keeps_an_empty_outcome() {
            let algorithm = algorithm_whose_work_pipeline_fails(true);
            let document = doc("d", vec![page(1, &["x"]), page(2, &["y"])]);
            let outcome = algorithm.apply(&document, &companies()).unwrap();

            assert_eq!(outcome.pages.len(), 2);
            assert!(outcome.pages.iter().all(|p| p.results.is_empty()));
        }

        #[test]
        fn any_other_failure_stops_the_run() {
            let algorithm = algorithm_whose_work_pipeline_fails(false);
            let err = algorithm.apply(&doc("d", vec![page(1, &["x"])]), &companies()).unwrap_err();
            assert!(matches!(err, AlgorithmError::Pipe(PipeError::Extraction { .. })));
        }

        #[test]
        fn a_page_failure_during_classification_is_not_absorbed() {
            // Il riferimento assorbe `PageParseFail` solo nel ciclo dello schedule, non nella
            // classificazione: la stessa asimmetria vale qui.
            let mut classify = Pipeline::new("classify");
            classify.pdf_extract.push(FailingExtract::page_parse("skipper"));
            classify.text_filter.push(RecordingFilter::new("filter") as Arc<dyn TextFilterPipe>);
            classify.deserialize.push(ConstantClassifier::pipe("classify", Some("a")));

            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classify),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let err =
                algorithm.classify_pages(&doc("d", vec![page(1, &["x"])])).unwrap_err();
            assert!(matches!(err, AlgorithmError::Pipe(PipeError::PageParse { .. })));
        }
    }

    mod results_accumulate_across_steps {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_page_class_named_by_two_steps_keeps_the_results_of_both() {
            // Divergenza voluta dal riferimento (D-M5-3): la' il secondo step sovrascrive i
            // risultati del primo, qui si accumulano.
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("work"), promising_pipeline("work", "id")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"]), step(&["a"])]),
                BTreeMap::from([(PageClass::new("a"), vec![PipelineName::new("work")])]),
            )
            .unwrap();

            let outcome =
                algorithm.apply(&doc("d", vec![page(1, &["x"])]), &companies()).unwrap();
            assert_eq!(outcome.pages.len(), 1);
            assert_eq!(outcome.pages[0].results.len(), 2);
        }
    }

    mod per_segment_api {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn apply_pdf_extract_runs_only_the_first_segment_of_the_class_bundle() {
            let algorithm = simple_algorithm();
            let blocks =
                algorithm.apply_pdf_extract(&page(1, &["x", "y"]), &PageClass::new("a")).unwrap();
            let contents: Vec<&str> =
                blocks.iter().map(|b| b.content.as_str().unwrap()).collect();
            assert_eq!(contents, vec!["x", "y"]);
        }

        #[test]
        fn apply_text_filter_runs_the_first_two_segments() {
            let algorithm = simple_algorithm();
            let blocks = algorithm
                .apply_text_filter(&page(1, &["x"]), &PageClass::new("a"), &FilterData::EMPTY)
                .unwrap();
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].type_block, BlockType::PAGE_CLASS);
        }

        #[test]
        fn apply_deserializer_starts_from_ready_made_text_blocks() {
            let algorithm = simple_algorithm();
            let blocks = vec![TextBlock::from_content(
                BlockType::PAGE_CLASS,
                std::collections::BTreeMap::new(),
                "content",
            )];
            let out = algorithm.apply_deserializer(&blocks, &PageClass::new("a")).unwrap();
            assert_eq!(out.len(), 1);
            assert!(out[0].as_promises().is_some());
        }

        /// Regressione: quando una page class mappa **due** pipeline, la catena completa non e'
        /// la composizione a mano di `apply_text_filter` e `apply_deserializer`. Il caso reale e'
        /// la class `merges` di KAIROS-EN23, che mappa `renames` e `merges`: incatenando a mano,
        /// ogni pipe `deserialize` vede anche i blocchi dell'altra pipeline e due eventi
        /// diventano quattro entita'.
        #[test]
        fn apply_deserialize_keeps_each_pipeline_a_closed_chain() {
            let algorithm = Algorithm::new(
                "FMT",
                BTreeMap::from([
                    (PipelineName::new("classify"), classifying_pipeline("classify", Some("a"))),
                    (PipelineName::new("one"), promising_pipeline("one", "id-one")),
                    (PipelineName::new("two"), promising_pipeline("two", "id-two")),
                ]),
                &[PipelineName::new("classify")],
                PageClassFinalizer::Identity,
                Schedule::new(vec![step(&["a"])]),
                BTreeMap::from([(
                    PageClass::new("a"),
                    vec![PipelineName::new("one"), PipelineName::new("two")],
                )]),
            )
            .expect("fixture is consistent");

            let page = page(1, &["x"]);
            let class = PageClass::new("a");

            let chained = algorithm.apply_deserialize(&page, &class, &FilterData::EMPTY).unwrap();
            assert_eq!(chained.len(), 2, "una entita' per pipeline, non il prodotto incrociato");

            // La composizione a mano e' proprio cio' che non va fatto: la si esercita qui per
            // fissare la differenza, non perche' sia un'alternativa accettabile.
            let blocks =
                algorithm.apply_text_filter(&page, &class, &FilterData::EMPTY).unwrap();
            let crossed = algorithm.apply_deserializer(&blocks, &class).unwrap();
            assert_eq!(crossed.len(), 4, "la composizione a mano incrocia le pipeline");
        }

        #[test]
        fn an_unmapped_page_class_is_an_error_in_all_three() {
            let algorithm = simple_algorithm();
            let ghost = PageClass::new("ghost");
            let expected = AlgorithmError::UnmappedPageClass { class: ghost.clone() };

            assert_eq!(algorithm.apply_pdf_extract(&page(1, &["x"]), &ghost).unwrap_err(), expected);
            assert_eq!(
                algorithm
                    .apply_text_filter(&page(1, &["x"]), &ghost, &FilterData::EMPTY)
                    .unwrap_err(),
                expected
            );
            assert_eq!(algorithm.apply_deserializer(&[], &ghost).unwrap_err(), expected);
        }
    }

    mod error_messages {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_unmapped_page_class_names_the_class() {
            let err = AlgorithmError::UnmappedPageClass { class: PageClass::new("ghost") };
            assert_eq!(err.to_string(), "page class `ghost` has no pipelines bundle");
        }

        #[test]
        fn a_classification_count_mismatch_reports_both_counts() {
            let err = AlgorithmError::ClassificationCountMismatch {
                document: "d".to_string(),
                pages: 3,
                classifications: 5,
            };
            assert_eq!(
                err.to_string(),
                "document `d` has 3 pages but the finalizer returned 5 classifications"
            );
        }

        #[test]
        fn a_pipe_error_is_forwarded_verbatim() {
            let pipe_error = PipeError::extraction("p", "boom");
            let err: AlgorithmError = pipe_error.clone().into();
            assert_eq!(err.to_string(), pipe_error.to_string());
        }

        #[test]
        fn a_schedule_error_is_forwarded_verbatim() {
            let schedule_error = ScheduleError::UnknownPageClass {
                document: "d".to_string(),
                class: PageClass::new("ghost"),
            };
            let err: AlgorithmError = schedule_error.clone().into();
            assert_eq!(err.to_string(), schedule_error.to_string());
        }
    }
}
