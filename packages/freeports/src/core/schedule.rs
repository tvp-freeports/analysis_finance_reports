//! Schedule: gruppi ordinati di [`PageClass`] e assegnazione pagine → step.
//!
//! `PLAN.md` §5.5. Lo schedule è la sequenza di "passate" che l'algoritmo fa sulle pagine di uno
//! o più documenti: ogni step nomina un insieme di page class, e le pagine che portano quelle
//! class vengono elaborate in quello step. I risultati di uno step alimentano il `filter_data`
//! degli step successivi (`PLAN.md` §5.4).
//!
//! **Ordine, differenza voluta dal riferimento** (`PLAN.md` §5.2, D5). In `freeports_core` uno
//! step è un `set` Python, quindi l'ordine di iterazione delle page class è quello di hash. Qui
//! [`ScheduleStep`] conserva l'ordine di inserimento e deduplica: l'ordine in cui le pagine
//! vengono elaborate dentro uno step è deterministico e i test sono riproducibili.
//!
//! **Pagina non classificata.** Una pagina la cui class è `None` è legittima e semplicemente non
//! entra in nessuno step — nel riferimento questo è espresso aggiungendo `None` all'insieme
//! `page_classes` usato per la validazione. Una pagina classificata con una class **non** nominata
//! da nessuno step è invece un errore ([`ScheduleError::UnknownPageClass`]), come nel riferimento.

use std::collections::BTreeSet;

use super::page::{Document, Page};

/// La classe di una pagina (es. `"investments"`, `"fund_info"`).
///
/// Newtype su `String` e non enum chiuso, per la stessa ragione di
/// [`BlockType`](crate::core::classes::BlockType): le page class le definiscono i repo formati,
/// quindi un enum in libreria non può elencarle.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageClass(String);

impl PageClass {
    pub fn new(name: impl Into<String>) -> Self {
        PageClass(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PageClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for PageClass {
    fn from(value: &str) -> Self {
        PageClass(value.to_string())
    }
}

impl From<String> for PageClass {
    fn from(value: String) -> Self {
        PageClass(value)
    }
}

/// Un gruppo di page class elaborate nella stessa passata.
///
/// Deduplicato, con ordine di inserimento. Uno step **vuoto** è legale: il riferimento lo produce
/// quando l'ultima riga di `algorithms_schedule.csv` ha `Filter next iteration` vero.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScheduleStep(Vec<PageClass>);

impl ScheduleStep {
    pub fn new() -> Self {
        ScheduleStep::default()
    }

    /// Aggiunge una page class in coda, se non già presente. Restituisce `true` se è stata
    /// davvero aggiunta.
    pub fn push(&mut self, class: impl Into<PageClass>) -> bool {
        let class = class.into();
        if self.0.contains(&class) {
            return false;
        }
        self.0.push(class);
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = &PageClass> {
        self.0.iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, class: &PageClass) -> bool {
        self.0.contains(class)
    }
}

impl<C: Into<PageClass>> FromIterator<C> for ScheduleStep {
    fn from_iter<I: IntoIterator<Item = C>>(iter: I) -> Self {
        let mut step = ScheduleStep::new();
        for class in iter {
            step.push(class);
        }
        step
    }
}

/// La sequenza ordinata degli step.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schedule(Vec<ScheduleStep>);

impl Schedule {
    pub fn new(steps: Vec<ScheduleStep>) -> Self {
        Schedule(steps)
    }

    pub fn steps(&self) -> &[ScheduleStep] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Tutte le page class nominate da almeno uno step, in ordine deterministico.
    ///
    /// Un `BTreeSet` e non un `Vec`: il solo consumatore è la validazione di
    /// [`Algorithm::new`](crate::core::algorithm::Algorithm::new), che lo confronta con le chiavi
    /// del mapping page class → pipeline.
    pub fn page_classes(&self) -> BTreeSet<PageClass> {
        self.0.iter().flat_map(|s| s.iter().cloned()).collect()
    }

    /// Vero se almeno uno step nomina questa page class.
    pub fn contains(&self, class: &PageClass) -> bool {
        self.0.iter().any(|s| s.contains(class))
    }

    /// Distribuisce le pagine dei documenti fra gli step.
    ///
    /// `classifications[d][p]` è la class assegnata alla pagina in posizione `p` del documento
    /// `docs[d]`; `None` significa "non classificata", che non è un errore ma esclude la pagina
    /// da ogni step.
    ///
    /// **Ordine del risultato**, identico al riferimento: per ogni step, le page class nell'ordine
    /// dello step; per ciascuna, i documenti nell'ordine dato; per ciascuno, le pagine nell'ordine
    /// del documento.
    pub fn assign<'a>(
        &self,
        docs: &'a [Document],
        classifications: &[Vec<Option<PageClass>>],
    ) -> Result<Vec<Vec<ScheduledPage<'a>>>, ScheduleError> {
        if classifications.len() != docs.len() {
            return Err(ScheduleError::ClassificationCountMismatch {
                documents: docs.len(),
                classifications: classifications.len(),
            });
        }
        for (doc, classes) in docs.iter().zip(classifications) {
            if classes.len() != doc.pages.len() {
                return Err(ScheduleError::PageCountMismatch {
                    document: doc.id.to_string(),
                    pages: doc.pages.len(),
                    classifications: classes.len(),
                });
            }
            for class in classes.iter().flatten() {
                if !self.contains(class) {
                    return Err(ScheduleError::UnknownPageClass {
                        document: doc.id.to_string(),
                        class: class.clone(),
                    });
                }
            }
        }

        let mut scheduled = Vec::with_capacity(self.0.len());
        for step in &self.0 {
            let mut step_pages = Vec::new();
            for class in step.iter() {
                for (doc_index, (doc, classes)) in docs.iter().zip(classifications).enumerate() {
                    for (page, page_class) in doc.pages.iter().zip(classes) {
                        if page_class.as_ref() == Some(class) {
                            step_pages.push(ScheduledPage {
                                doc_index,
                                doc,
                                page,
                                class: class.clone(),
                            });
                        }
                    }
                }
            }
            scheduled.push(step_pages);
        }
        tracing::debug!(
            steps = scheduled.len(),
            pages = scheduled.iter().map(Vec::len).sum::<usize>(),
            "pages assigned to schedule"
        );
        Ok(scheduled)
    }
}

/// Una pagina assegnata a uno step, con il documento da cui viene e la class che l'ha assegnata.
///
/// Rimpiazza la tupla non tipizzata `(doc_name, page_n, page)` del riferimento (`PLAN.md` §5.5).
#[derive(Debug)]
pub struct ScheduledPage<'a> {
    /// Posizione del documento nella slice passata a [`Schedule::assign`]. Serve a
    /// [`Algorithm`](crate::core::algorithm::Algorithm) per ricomporre i risultati per documento
    /// senza dover confrontare gli id — che non sono garantiti unici.
    pub doc_index: usize,
    pub doc: &'a Document,
    pub page: &'a Page,
    pub class: PageClass,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScheduleError {
    #[error("expected one classification list per document: {documents} documents, {classifications} lists")]
    ClassificationCountMismatch { documents: usize, classifications: usize },
    #[error(
        "document `{document}` has {pages} pages but {classifications} classifications: every page has to be classified"
    )]
    PageCountMismatch { document: String, pages: usize, classifications: usize },
    #[error("all pages have to enter the schedule at some point: `{class}` (document `{document}`) is not part of it")]
    UnknownPageClass { document: String, class: PageClass },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(number: u32) -> Page {
        Page::new(number, (1.0, 1.0), vec![], vec![])
    }

    fn doc(id: &str, n_pages: u32) -> Document {
        Document::new(id, "FMT", (1..=n_pages).map(page).collect())
    }

    fn step(classes: &[&str]) -> ScheduleStep {
        classes.iter().copied().collect()
    }

    /// `[a] -> [b]`: due step, una class ciascuno.
    fn two_step_schedule() -> Schedule {
        Schedule::new(vec![step(&["a"]), step(&["b"])])
    }

    mod page_class {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn round_trips_through_its_accessor() {
            assert_eq!(PageClass::new("investments").as_str(), "investments");
        }

        #[test]
        fn displays_as_its_bare_string() {
            assert_eq!(PageClass::new("investments").to_string(), "investments");
        }

        #[test]
        fn is_built_from_both_str_and_string() {
            assert_eq!(PageClass::from("x"), PageClass::from("x".to_string()));
        }
    }

    mod step_construction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn keeps_insertion_order_rather_than_sorting() {
            let s = step(&["zulu", "alpha"]);
            let names: Vec<&str> = s.iter().map(PageClass::as_str).collect();
            assert_eq!(names, vec!["zulu", "alpha"]);
        }

        #[test]
        fn deduplicates_and_reports_that_it_did() {
            let mut s = ScheduleStep::new();
            assert!(s.push("a"));
            assert!(!s.push("a"));
            assert_eq!(s.len(), 1);
        }

        #[test]
        fn a_duplicate_does_not_move_the_original_to_the_back() {
            let mut s = step(&["a", "b"]);
            s.push("a");
            let names: Vec<&str> = s.iter().map(PageClass::as_str).collect();
            assert_eq!(names, vec!["a", "b"]);
        }

        #[test]
        fn an_empty_step_is_legal() {
            // Il riferimento lo produce quando l'ultima riga dello schedule CSV chiede
            // `Filter next iteration`.
            let s = ScheduleStep::new();
            assert!(s.is_empty());
            assert_eq!(s.len(), 0);
        }

        #[test]
        fn reports_whether_it_contains_a_class() {
            let s = step(&["a"]);
            assert!(s.contains(&PageClass::new("a")));
            assert!(!s.contains(&PageClass::new("b")));
        }
    }

    mod schedule_construction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn collects_the_page_classes_of_every_step() {
            let s = Schedule::new(vec![step(&["b", "a"]), step(&["c"])]);
            let page_classes = s.page_classes();
            let classes: Vec<&str> = page_classes.iter().map(PageClass::as_str).collect();
            assert_eq!(classes, vec!["a", "b", "c"]);
        }

        #[test]
        fn a_class_repeated_across_steps_is_collected_once() {
            let s = Schedule::new(vec![step(&["a"]), step(&["a"])]);
            assert_eq!(s.page_classes().len(), 1);
        }

        #[test]
        fn an_empty_schedule_has_no_page_classes() {
            let s = Schedule::default();
            assert!(s.is_empty());
            assert!(s.page_classes().is_empty());
        }

        #[test]
        fn reports_whether_any_step_names_a_class() {
            let s = two_step_schedule();
            assert!(s.contains(&PageClass::new("b")));
            assert!(!s.contains(&PageClass::new("z")));
        }
    }

    mod assignment {
        use super::*;
        use pretty_assertions::assert_eq;

        /// `(id documento, numero pagina)` per ogni pagina schedulata, step per step — la forma
        /// compatta su cui sono scritte le asserzioni d'ordine.
        fn shape(scheduled: &[Vec<ScheduledPage<'_>>]) -> Vec<Vec<(String, u32)>> {
            scheduled
                .iter()
                .map(|step| step.iter().map(|s| (s.doc.id.to_string(), s.page.number)).collect())
                .collect()
        }

        #[test]
        fn each_page_lands_in_the_step_that_names_its_class() {
            let docs = vec![doc("d", 2)];
            let classes = vec![vec![Some(PageClass::new("a")), Some(PageClass::new("b"))]];
            let scheduled = two_step_schedule().assign(&docs, &classes).unwrap();
            assert_eq!(shape(&scheduled), vec![vec![("d".into(), 1)], vec![("d".into(), 2)]]);
        }

        #[test]
        fn an_unclassified_page_enters_no_step_and_is_not_an_error() {
            let docs = vec![doc("d", 2)];
            let classes = vec![vec![None, Some(PageClass::new("a"))]];
            let scheduled = two_step_schedule().assign(&docs, &classes).unwrap();
            assert_eq!(shape(&scheduled), vec![vec![("d".into(), 2)], vec![]]);
        }

        #[test]
        fn a_class_no_step_names_is_an_error() {
            let docs = vec![doc("d", 1)];
            let classes = vec![vec![Some(PageClass::new("zzz"))]];
            let err = two_step_schedule().assign(&docs, &classes).unwrap_err();
            assert_eq!(
                err,
                ScheduleError::UnknownPageClass {
                    document: "d".to_string(),
                    class: PageClass::new("zzz"),
                }
            );
        }

        #[test]
        fn documents_are_visited_in_order_within_one_page_class() {
            let docs = vec![doc("first", 1), doc("second", 1)];
            let classes = vec![vec![Some(PageClass::new("a"))], vec![Some(PageClass::new("a"))]];
            let scheduled = Schedule::new(vec![step(&["a"])]).assign(&docs, &classes).unwrap();
            assert_eq!(
                shape(&scheduled),
                vec![vec![("first".into(), 1), ("second".into(), 1)]]
            );
        }

        #[test]
        fn page_classes_are_visited_in_step_order_not_alphabetically() {
            // Uno step `[zulu, alpha]`: le pagine `zulu` devono precedere quelle `alpha`.
            let docs = vec![doc("d", 2)];
            let classes = vec![vec![Some(PageClass::new("alpha")), Some(PageClass::new("zulu"))]];
            let schedule = Schedule::new(vec![step(&["zulu", "alpha"])]);
            let scheduled = schedule.assign(&docs, &classes).unwrap();
            assert_eq!(shape(&scheduled), vec![vec![("d".into(), 2), ("d".into(), 1)]]);
        }

        #[test]
        fn a_class_named_by_two_steps_schedules_the_page_twice() {
            let docs = vec![doc("d", 1)];
            let classes = vec![vec![Some(PageClass::new("a"))]];
            let schedule = Schedule::new(vec![step(&["a"]), step(&["a"])]);
            let scheduled = schedule.assign(&docs, &classes).unwrap();
            assert_eq!(shape(&scheduled), vec![vec![("d".into(), 1)], vec![("d".into(), 1)]]);
        }

        #[test]
        fn an_empty_step_schedules_nothing_without_disturbing_the_others() {
            let docs = vec![doc("d", 1)];
            let classes = vec![vec![Some(PageClass::new("a"))]];
            let schedule = Schedule::new(vec![ScheduleStep::new(), step(&["a"])]);
            let scheduled = schedule.assign(&docs, &classes).unwrap();
            assert_eq!(shape(&scheduled), vec![vec![], vec![("d".into(), 1)]]);
        }

        #[test]
        fn the_scheduled_page_carries_the_class_that_selected_it() {
            let docs = vec![doc("d", 1)];
            let classes = vec![vec![Some(PageClass::new("a"))]];
            let scheduled = two_step_schedule().assign(&docs, &classes).unwrap();
            assert_eq!(scheduled[0][0].class, PageClass::new("a"));
        }

        #[test]
        fn the_scheduled_page_carries_the_position_of_its_document() {
            // Due documenti con lo *stesso* id: solo la posizione li distingue.
            let docs = vec![doc("same", 1), doc("same", 1)];
            let classes =
                vec![vec![Some(PageClass::new("a"))], vec![Some(PageClass::new("a"))]];
            let scheduled = Schedule::new(vec![step(&["a"])]).assign(&docs, &classes).unwrap();
            let indexes: Vec<usize> = scheduled[0].iter().map(|s| s.doc_index).collect();
            assert_eq!(indexes, vec![0, 1]);
        }

        #[test]
        fn one_classification_list_per_document_is_required() {
            let docs = vec![doc("a", 1), doc("b", 1)];
            let classes = vec![vec![Some(PageClass::new("a"))]];
            let err = two_step_schedule().assign(&docs, &classes).unwrap_err();
            assert_eq!(
                err,
                ScheduleError::ClassificationCountMismatch { documents: 2, classifications: 1 }
            );
        }

        #[test]
        fn one_classification_per_page_is_required() {
            let docs = vec![doc("d", 3)];
            let classes = vec![vec![Some(PageClass::new("a"))]];
            let err = two_step_schedule().assign(&docs, &classes).unwrap_err();
            assert_eq!(
                err,
                ScheduleError::PageCountMismatch {
                    document: "d".to_string(),
                    pages: 3,
                    classifications: 1,
                }
            );
        }

        #[test]
        fn no_documents_yields_one_empty_bucket_per_step() {
            let scheduled = two_step_schedule().assign(&[], &[]).unwrap();
            assert_eq!(shape(&scheduled), vec![Vec::new(), Vec::new()]);
        }
    }

    mod error_messages {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn an_unknown_page_class_names_both_class_and_document() {
            let err = ScheduleError::UnknownPageClass {
                document: "report".to_string(),
                class: PageClass::new("ghost"),
            };
            assert_eq!(
                err.to_string(),
                "all pages have to enter the schedule at some point: `ghost` (document `report`) is not part of it"
            );
        }

        #[test]
        fn a_page_count_mismatch_reports_both_counts() {
            let err = ScheduleError::PageCountMismatch {
                document: "report".to_string(),
                pages: 3,
                classifications: 2,
            };
            assert_eq!(
                err.to_string(),
                "document `report` has 3 pages but 2 classifications: every page has to be classified"
            );
        }

        #[test]
        fn a_classification_count_mismatch_reports_both_counts() {
            let err =
                ScheduleError::ClassificationCountMismatch { documents: 2, classifications: 1 };
            assert_eq!(
                err.to_string(),
                "expected one classification list per document: 2 documents, 1 lists"
            );
        }
    }
}
