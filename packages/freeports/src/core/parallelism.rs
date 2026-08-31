//! Quante pagine alla volta, e su quale pool di thread.
//!
//! `PLAN.md` §4 P2. Il motore non decide da sé quanto parallelizzare: riceve una [`Parallelism`]
//! e la rispetta. È una scelta deliberata (`agent-memory/P2-implementation-plan.md` D-P2-2): le
//! firme storiche di [`Algorithm`](crate::core::algorithm::Algorithm) restano sequenziali, le
//! varianti `*_with` prendono questo parametro, e `pages == 1` percorre esattamente il codice di
//! prima — che è il modo con cui `PLAN.md` §6 vuole si verifichi il determinismo.
//!
//! # Perché un pool proprio e non quello globale di rayon
//!
//! `rayon::ThreadPoolBuilder::build_global` si può chiamare una volta sola per processo, e il pool
//! globale appartiene a chi *incorpora* il crate — l'example `p0_profile`, un consumatore Python,
//! un binario di terze parti. Sequestrarlo sarebbe una decisione presa a nome suo. Qui si tiene
//! un pool dedicato, costruito pigramente alla prima richiesta di parallelismo vero.
//!
//! # Il GIL non entra da questa porta
//!
//! Il pool serve a distribuire lavoro **Rust puro**. I pipe definiti dall'autore di un formato
//! riprendono il GIL a ogni chiamata e su N thread si riserializzano fra loro: per questo non
//! vengono distribuiti affatto, ma rilevati e degradati a sequenziale
//! (`PipelinesBundle::scales_with_threads`).

use std::sync::OnceLock;

/// Quanto parallelismo il motore può usare.
///
/// Un solo campo oggi. È una struct e non un `usize` nudo perché P3 (`pipelines`) e P5
/// (lo schema `parallelism` completo) vi si aggiungono come campi, senza cambiare nessuna firma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parallelism {
    /// Quante pagine dello stesso step (o della stessa classificazione) elaborare insieme.
    /// `1` significa *sequenziale*, non "un thread": non si tocca nemmeno il pool.
    pub pages: usize,
}

impl Parallelism {
    /// Il comportamento di sempre: nessun thread, nessun pool, nessuna differenza osservabile.
    pub const SEQUENTIAL: Parallelism = Parallelism { pages: 1 };

    /// Tante pagine quanti sono i thread hardware disponibili.
    ///
    /// `available_parallelism` fallisce su piattaforme che non sanno rispondere: in quel caso la
    /// risposta prudente è "una", non un numero inventato.
    pub fn auto() -> Parallelism {
        Parallelism { pages: available_threads() }
    }

    /// `pages` pagine alla volta, con `0` normalizzato a `1`.
    ///
    /// Zero arriva da configurazioni scritte a mano e da divisioni di budget fra job
    /// (`cli::run::page_parallelism`): trattarlo come sequenziale è più utile che rifiutarlo.
    pub fn pages(pages: usize) -> Parallelism {
        Parallelism { pages: pages.max(1) }
    }

    /// `true` se vale la pena distribuire `count` unità di lavoro.
    ///
    /// Una sola pagina non si parallelizza: il costo di distribuzione di rayon sarebbe pagato per
    /// intero e il guadagno sarebbe zero.
    pub fn is_worth_it(&self, count: usize) -> bool {
        self.pages > 1 && count > 1
    }
}

impl Default for Parallelism {
    fn default() -> Self {
        Parallelism::SEQUENTIAL
    }
}

/// Quanti thread hardware, o `1` se il sistema non lo sa dire.
pub fn available_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();

/// Il pool dedicato del motore, costruito alla prima richiesta.
///
/// **La prima richiesta fissa la dimensione per tutta la vita del processo.** In una corsa reale
/// il valore è uno solo (lo stesso `Parallelism` attraversa tutto il job), quindi il caso "due
/// dimensioni diverse" esiste solo nei test; là il pool più grande basta comunque, perché la
/// distribuzione effettiva la decide `Parallelism::is_worth_it`, non la taglia del pool.
///
/// Se rayon non riesce a costruire il pool — non ci sono thread da dare — la risposta è `None` e
/// il chiamante torna sul percorso sequenziale invece di fallire: il parallelismo è una
/// ottimizzazione, non un requisito di correttezza.
pub fn pool(parallelism: Parallelism) -> Option<&'static rayon::ThreadPool> {
    if let Some(existing) = POOL.get() {
        return Some(existing);
    }
    let built = rayon::ThreadPoolBuilder::new()
        .num_threads(parallelism.pages)
        .thread_name(|index| format!("freeports-page-{index}"))
        .build();
    match built {
        Ok(pool) => Some(POOL.get_or_init(|| pool)),
        Err(error) => {
            tracing::warn!(
                error = crate::core::tracing_setup::log_error(&error),
                threads = parallelism.pages,
                "no thread pool available, pages will be processed one at a time: {error}"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod construction {
        use super::*;

        #[test]
        fn sequential_is_one_page_at_a_time() {
            assert_eq!(Parallelism::SEQUENTIAL.pages, 1);
            assert_eq!(Parallelism::default(), Parallelism::SEQUENTIAL);
        }

        #[test]
        fn auto_is_never_zero() {
            assert!(Parallelism::auto().pages >= 1);
        }

        #[test]
        fn zero_pages_means_sequential_not_zero_threads() {
            assert_eq!(Parallelism::pages(0), Parallelism::SEQUENTIAL);
        }

        #[test]
        fn a_positive_request_is_kept_as_it_is() {
            assert_eq!(Parallelism::pages(4).pages, 4);
        }
    }

    mod worth_it {
        use super::*;

        #[test]
        fn sequential_never_distributes_however_much_work_there_is() {
            assert!(!Parallelism::SEQUENTIAL.is_worth_it(10_000));
        }

        #[test]
        fn a_single_unit_of_work_is_never_distributed() {
            assert!(!Parallelism::pages(8).is_worth_it(1));
            assert!(!Parallelism::pages(8).is_worth_it(0));
        }

        #[test]
        fn more_than_one_unit_with_more_than_one_thread_is_distributed() {
            assert!(Parallelism::pages(2).is_worth_it(2));
        }
    }

    mod thread_pool {
        use super::*;

        #[test]
        fn the_pool_is_built_once_and_reused() {
            let first = pool(Parallelism::pages(2)).expect("a pool must be available");
            let second = pool(Parallelism::pages(3)).expect("a pool must be available");
            assert!(std::ptr::eq(first, second), "the pool must be built once per process");
        }

        #[test]
        fn work_submitted_to_the_pool_actually_runs() {
            let pool = pool(Parallelism::pages(2)).expect("a pool must be available");
            let sum: usize = pool.install(|| {
                use rayon::prelude::*;
                (1..=100usize).into_par_iter().sum()
            });
            assert_eq!(sum, 5050);
        }
    }
}
