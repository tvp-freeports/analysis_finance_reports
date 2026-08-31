//! How many pages at a time, and on which thread pool.
//!
//! The engine never decides on its own how much to parallelise: it is handed a [`Parallelism`] and
//! obeys it. That is deliberate. The historical [`Algorithm`](crate::core::algorithm::Algorithm)
//! signatures stay sequential and the `*_with` variants take this parameter, so `pages == 1` walks
//! exactly the same code as before — which is how the equivalence between a sequential run and a
//! parallel one can be checked at all.
//!
//! # Why a pool of its own, and not rayon's global one
//!
//! `rayon::ThreadPoolBuilder::build_global` can be called only once per process, and the global
//! pool belongs to whoever *embeds* this crate — a Python consumer, a third-party binary, a
//! profiling harness. Seizing it would be a decision taken in their name. Instead a dedicated pool
//! is built lazily, on the first request for real parallelism.
//!
//! # The GIL does not come in through this door
//!
//! This pool distributes **pure Rust** work only. Pipes written by a format author take the GIL
//! back on every call and would re-serialise against each other across N threads, so they are not
//! distributed at all: they are detected and degraded to sequential by
//! [`PipelinesBundle::scales_with_threads`].
//!
//! [`PipelinesBundle::scales_with_threads`]:
//!     crate::core::pipeline::bundle::PipelinesBundle::scales_with_threads

use std::sync::OnceLock;

/// How much parallelism the engine may use.
///
/// One field today. It is a struct rather than a bare `usize` so that further levels can be added
/// as fields without changing a single signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parallelism {
    /// How many pages of the same step — or of the same classification pass — to process together.
    ///
    /// `1` means *sequential*, not "one thread": the pool is not even touched.
    pub pages: usize,
}

impl Parallelism {
    /// The behaviour that has always been: no threads, no pool, nothing observably different.
    pub const SEQUENTIAL: Parallelism = Parallelism { pages: 1 };

    /// As many pages as there are hardware threads available.
    ///
    /// `available_parallelism` fails on platforms that cannot answer; there the careful reply is
    /// one, not an invented number.
    pub fn auto() -> Parallelism {
        Parallelism { pages: available_threads() }
    }

    /// `pages` pages at a time, with `0` normalised to `1`.
    ///
    /// Zero arrives from hand-written configurations and from splitting a worker budget across
    /// jobs; treating it as sequential is more useful than rejecting it.
    pub fn pages(pages: usize) -> Parallelism {
        Parallelism { pages: pages.max(1) }
    }

    /// Whether distributing `count` units of work is worth it at all.
    ///
    /// A single page is never parallelised: rayon's distribution cost would be paid in full for no
    /// gain.
    pub fn is_worth_it(&self, count: usize) -> bool {
        self.pages > 1 && count > 1
    }
}

impl Default for Parallelism {
    fn default() -> Self {
        Parallelism::SEQUENTIAL
    }
}

/// How many hardware threads there are, or `1` if the system cannot say.
pub fn available_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();

/// The engine's dedicated pool, built on first use.
///
/// **The first request fixes the size for the lifetime of the process.** In a real run there is
/// only ever one value — the same [`Parallelism`] travels through the whole job — so the case of
/// two different sizes exists only in tests, and there the larger pool is harmless: how much work
/// is actually spread is decided by [`Parallelism::is_worth_it`], not by the pool's size.
///
/// Returns `None` if rayon cannot build the pool, and the caller then falls back to the sequential
/// path instead of failing: parallelism is an optimisation, not a correctness requirement.
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
