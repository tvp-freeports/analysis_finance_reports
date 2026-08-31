//! Quanto parallelismo, livello per livello: la sezione `parallelism` della configurazione.
//!
//! `PLAN.md` §4 P5, `agent-memory/P5-implementation-plan.md`. Due livelli hanno un consumatore
//! vero e sono gli unici che compaiono qui: `jobs` (P1, processi figli in modalita' batch) e
//! `pages` (P2, thread rayon dentro un job). Gli altri due che il piano immaginava non ci sono —
//! `pipelines` (P3) e `deserialize_blocks_threshold` (P4) sono stati chiusi senza
//! implementazione dalla misura di P0, e un'opzione che non governa niente e' peggio di
//! un'opzione assente.
//!
//! # Perche' in `cli` e non in `core::parallelism`
//!
//! `jobs` conta i **processi** di una corsa in batch: e' un concetto della riga di comando, non
//! del motore. `core::parallelism` resta il livello che obbedisce — riceve una
//! [`Parallelism`](crate::core::parallelism::Parallelism) gia' risolta e non sa da dove venga.
//! Qui vive la parte che *decide*, che e' anche l'unica che ha bisogno di sapere quanti job
//! stanno per girare insieme.
//!
//! # `n_workers` e i due override
//!
//! `n_workers` esisteva da prima dei livelli, e diventa il **default globale**: il valore che ogni
//! livello prende se `parallelism.<livello>` non dice altro (`agent-memory/P5-implementation-
//! plan.md` D-P5-1). E' anche cio' che rida' a `-j 1` il suo significato universale — un job alla
//! volta *e* una pagina alla volta, cioe' esattamente il comportamento sequenziale che `PLAN.md`
//! §6 usa per verificare il determinismo.

use crate::core::parallelism::{self, Parallelism};

/// Quanti lavoratori a un livello: un numero deciso da chi configura, o `auto`.
///
/// `Fixed` non e' mai `0`: i parser di tutte e tre le sorgenti rifiutano lo zero con un errore
/// tipizzato invece di normalizzarlo, perche' "zero processi" e' quasi sempre un refuso e mai una
/// richiesta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Workers {
    /// Risolto a runtime dal numero di core e dal lavoro da distribuire.
    #[default]
    Auto,
    Fixed(usize),
}

/// Il testo di `auto`, riconosciuto senza distinzione di maiuscole da tutte le sorgenti.
pub const AUTO: &str = "auto";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("expected a positive number of workers or {AUTO:?}, got {value:?}")]
pub struct WorkersParseError {
    pub value: String,
}

impl Workers {
    /// `auto` (in qualunque combinazione di maiuscole) o un intero positivo.
    pub fn parse(value: &str) -> Result<Workers, WorkersParseError> {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case(AUTO) {
            return Ok(Workers::Auto);
        }
        match trimmed.parse::<usize>() {
            Ok(n) if n > 0 => Ok(Workers::Fixed(n)),
            _ => Err(WorkersParseError { value: value.to_string() }),
        }
    }

    /// Il numero richiesto, o quello dei thread hardware se la richiesta e' `auto`.
    fn requested(&self) -> usize {
        match self {
            Workers::Auto => parallelism::available_threads(),
            Workers::Fixed(n) => (*n).max(1),
        }
    }

    fn is_auto(&self) -> bool {
        matches!(self, Workers::Auto)
    }
}

impl std::fmt::Display for Workers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Workers::Auto => f.write_str(AUTO),
            Workers::Fixed(n) => write!(f, "{n}"),
        }
    }
}

/// La sezione `parallelism` risolta a due valori, uno per livello.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ParallelismConfig {
    /// P1: quanti job di un batch girano insieme, in processi figli.
    pub jobs: Workers,
    /// P2: quante pagine dello stesso step un job elabora insieme, in thread.
    pub pages: Workers,
}

impl ParallelismConfig {
    /// Il comportamento di sempre: un job alla volta, una pagina alla volta.
    pub const SEQUENTIAL: ParallelismConfig =
        ParallelismConfig { jobs: Workers::Fixed(1), pages: Workers::Fixed(1) };

    /// Quanti job alla volta, dato quanti ce ne sono da eseguire.
    ///
    /// Il numero si limita comunque ai job disponibili: un processo figlio senza job da eseguire
    /// non ha nulla da fare, e chiederne piu' di quanti ne esistono e' un modo legittimo di dire
    /// "tutti".
    pub fn resolve_jobs(&self, job_count: usize) -> usize {
        self.jobs.requested().min(job_count.max(1)).max(1)
    }

    /// Quante pagine alla volta dentro **un** job, dato quanti job girano insieme.
    ///
    /// In `auto` il budget di core si **divide** fra i job concorrenti, cosi' che un batch con
    /// quattro job su venti thread hardware ne usi cinque per job invece di venti: e' l'invariante
    /// che P2 ha introdotto e che P5 non cambia. Un valore **esplicito** non si divide — chi lo
    /// scrive lo ha chiesto, e [`ParallelismConfig::oversubscription`] si occupa di dirlo.
    pub fn resolve_pages(&self, resolved_jobs: usize) -> Parallelism {
        if self.pages.is_auto() {
            return Parallelism::pages(parallelism::available_threads() / resolved_jobs.max(1));
        }
        Parallelism::pages(self.pages.requested())
    }

    /// Quanti thread la configurazione risolta apre in tutto, se sono piu' dei core disponibili.
    ///
    /// `None` quando la richiesta ci sta: e' il caso normale, e non merita una riga di log. Serve
    /// a `cli::run` per avvertire senza rifiutare — `PLAN.md` §2 principio 4 vieta gli override
    /// silenziosi, non le configurazioni scomode.
    pub fn oversubscription(resolved_jobs: usize, resolved_pages: Parallelism) -> Option<usize> {
        let total = resolved_jobs.max(1) * resolved_pages.pages.max(1);
        (total > parallelism::available_threads()).then_some(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod parsing {
        use super::*;

        #[test]
        fn auto_is_recognised_whatever_the_case() {
            for spelling in ["auto", "AUTO", "Auto", " auto "] {
                assert_eq!(Workers::parse(spelling).unwrap(), Workers::Auto, "{spelling:?}");
            }
        }

        #[test]
        fn a_positive_integer_is_kept_as_it_is() {
            assert_eq!(Workers::parse("7").unwrap(), Workers::Fixed(7));
        }

        #[test]
        fn zero_is_a_typed_error_not_a_silent_one() {
            assert_eq!(Workers::parse("0").unwrap_err().value, "0");
        }

        #[test]
        fn a_negative_number_is_a_typed_error() {
            assert!(Workers::parse("-1").is_err());
        }

        #[test]
        fn a_word_that_is_not_auto_is_a_typed_error() {
            assert!(Workers::parse("many").is_err());
            assert!(Workers::parse("").is_err());
        }

        #[test]
        fn the_error_quotes_the_value_it_refused() {
            let message = Workers::parse("many").unwrap_err().to_string();
            assert!(message.contains("\"many\""), "{message}");
            assert!(message.contains("auto"), "{message}");
        }

        #[test]
        fn display_round_trips_through_parse() {
            for workers in [Workers::Auto, Workers::Fixed(3)] {
                assert_eq!(Workers::parse(&workers.to_string()).unwrap(), workers);
            }
        }
    }

    mod resolving_jobs {
        use super::*;

        fn with_jobs(jobs: Workers) -> ParallelismConfig {
            ParallelismConfig { jobs, pages: Workers::Auto }
        }

        #[test]
        fn an_explicit_request_is_honoured_when_there_are_enough_jobs() {
            assert_eq!(with_jobs(Workers::Fixed(3)).resolve_jobs(10), 3);
        }

        #[test]
        fn more_workers_than_jobs_are_capped_at_the_number_of_jobs() {
            assert_eq!(with_jobs(Workers::Fixed(99)).resolve_jobs(2), 2);
        }

        #[test]
        fn one_worker_stays_one_however_many_jobs_there_are() {
            assert_eq!(with_jobs(Workers::Fixed(1)).resolve_jobs(1_000), 1);
        }

        #[test]
        fn auto_never_exceeds_either_the_cores_or_the_jobs() {
            let resolved = with_jobs(Workers::Auto).resolve_jobs(3);
            assert!(resolved >= 1);
            assert!(resolved <= 3);
            assert!(resolved <= parallelism::available_threads());
        }

        #[test]
        fn no_jobs_at_all_still_resolves_to_one_worker() {
            assert_eq!(with_jobs(Workers::Auto).resolve_jobs(0), 1);
        }
    }

    mod resolving_pages {
        use super::*;

        fn with_pages(pages: Workers) -> ParallelismConfig {
            ParallelismConfig { jobs: Workers::Auto, pages }
        }

        #[test]
        fn an_explicit_request_is_not_divided_among_the_concurrent_jobs() {
            assert_eq!(with_pages(Workers::Fixed(6)).resolve_pages(4).pages, 6);
        }

        #[test]
        fn auto_divides_the_core_budget_among_the_concurrent_jobs() {
            let cores = parallelism::available_threads();
            assert_eq!(with_pages(Workers::Auto).resolve_pages(1).pages, cores.max(1));
            assert_eq!(with_pages(Workers::Auto).resolve_pages(2).pages, (cores / 2).max(1));
        }

        #[test]
        fn auto_divided_below_one_is_sequential_not_zero() {
            assert_eq!(with_pages(Workers::Auto).resolve_pages(10_000), Parallelism::SEQUENTIAL);
        }

        #[test]
        fn one_page_is_the_sequential_engine() {
            assert_eq!(with_pages(Workers::Fixed(1)).resolve_pages(1), Parallelism::SEQUENTIAL);
        }
    }

    mod sequential_preset {
        use super::*;

        #[test]
        fn one_everywhere_is_one_job_and_one_page() {
            let config = ParallelismConfig::SEQUENTIAL;
            assert_eq!(config.resolve_jobs(50), 1);
            assert_eq!(config.resolve_pages(1), Parallelism::SEQUENTIAL);
        }

        #[test]
        fn the_default_is_auto_at_both_levels_not_sequential() {
            assert_eq!(
                ParallelismConfig::default(),
                ParallelismConfig { jobs: Workers::Auto, pages: Workers::Auto }
            );
        }
    }

    mod oversubscription {
        use super::*;

        #[test]
        fn a_request_that_fits_the_machine_is_not_reported() {
            assert_eq!(ParallelismConfig::oversubscription(1, Parallelism::SEQUENTIAL), None);
        }

        #[test]
        fn asking_for_more_threads_than_cores_reports_the_total() {
            let cores = parallelism::available_threads();
            let total = ParallelismConfig::oversubscription(cores + 1, Parallelism::pages(2));
            assert_eq!(total, Some((cores + 1) * 2));
        }
    }

    mod serialization {
        use super::*;

        /// La configurazione risolta attraversa il confine di processo verso un job worker (P1):
        /// se questi tipi non sopravvivessero al giro in JSON, un figlio girerebbe con un
        /// parallelismo diverso da quello che il padre ha deciso.
        #[test]
        fn a_configuration_survives_a_json_round_trip() {
            let config = ParallelismConfig { jobs: Workers::Fixed(4), pages: Workers::Auto };
            let json = serde_json::to_string(&config).expect("serializable");
            assert_eq!(serde_json::from_str::<ParallelismConfig>(&json).unwrap(), config);
        }
    }
}
