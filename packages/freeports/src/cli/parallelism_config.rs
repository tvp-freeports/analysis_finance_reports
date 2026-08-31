//! How much parallelism, level by level: the `parallelism` section of the configuration.
//!
//! Two levels have a real consumer and are the only ones here: `jobs`, the child processes of a
//! batch run, and `pages`, the threads inside one job. Levels that would govern nothing are
//! deliberately absent — an option that changes no behaviour is worse than no option.
//!
//! # Why this lives in `cli` and not with the engine's parallelism
//!
//! `jobs` counts the **processes** of a batch run: a command-line concept, not an engine one.
//! [`crate::core::parallelism`] is the level that obeys — it receives an already-resolved value and
//! does not know where it came from. What *decides* lives here, which is also the only place that
//! knows how many jobs are about to run together.
//!
//! # One global default and two overrides
//!
//! `n_workers` is the **global default**: the value each level takes when its own key says nothing.
//! It is also what gives `-j 1` a universal meaning — one job at a time *and* one page at a time,
//! which is exactly the sequential behaviour the determinism checks rely on.

use crate::core::parallelism::{self, Parallelism};

/// How many workers at one level: a number chosen by whoever configures, or automatic.
///
/// A fixed value is never zero: every source rejects zero with a typed error rather than
/// normalising it, because "zero processes" is nearly always a typo and never a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Workers {
    /// Risolto a runtime dal numero di core e dal lavoro da distribuire.
    #[default]
    Auto,
    Fixed(usize),
}

/// The text of the automatic setting, recognised case-insensitively by every source.
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

    /// The number requested, or the hardware thread count when the request is automatic.
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

/// The `parallelism` section resolved to one value per level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ParallelismConfig {
    /// P1: quanti job di un batch girano insieme, in processi figli.
    pub jobs: Workers,
    /// How many pages of the same step one job processes together, in threads.
    pub pages: Workers,
}

impl ParallelismConfig {
    /// The behaviour that has always been: one job at a time, one page at a time.
    pub const SEQUENTIAL: ParallelismConfig =
        ParallelismConfig { jobs: Workers::Fixed(1), pages: Workers::Fixed(1) };

    /// How many jobs at a time, given how many there are to run.
    ///
    /// Capped at the number of jobs available: a child process with no job to run has nothing to
    /// do, and asking for more than exist is a legitimate way of saying "all of them".
    pub fn resolve_jobs(&self, job_count: usize) -> usize {
        self.jobs.requested().min(job_count.max(1)).max(1)
    }

    /// How many pages at a time inside **one** job, given how many jobs run together.
    ///
    /// Automatically, the budget of cores is **divided** among the concurrent jobs, so that a batch
    /// of four jobs on twenty hardware threads uses five per job rather than twenty. An
    /// **explicit** value is not divided: whoever wrote it asked for it, and
    /// [`ParallelismConfig::oversubscription`] is what says so.
    pub fn resolve_pages(&self, resolved_jobs: usize) -> Parallelism {
        if self.pages.is_auto() {
            return Parallelism::pages(parallelism::available_threads() / resolved_jobs.max(1));
        }
        Parallelism::pages(self.pages.requested())
    }

    /// How many threads the resolved configuration opens in total, when that exceeds the cores
    /// available.
    ///
    /// `None` when the request fits, which is the normal case and does not deserve a log line. It
    /// lets the caller warn without refusing: silently overriding what someone configured is worse
    /// than an awkward configuration.
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

        /// The resolved configuration crosses a process boundary to a worker job: were these types
        /// not to survive the round trip, a child would run with a different parallelism from the
        /// one the parent decided.
        #[test]
        fn a_configuration_survives_a_json_round_trip() {
            let config = ParallelismConfig { jobs: Workers::Fixed(4), pages: Workers::Auto };
            let json = serde_json::to_string(&config).expect("serializable");
            assert_eq!(serde_json::from_str::<ParallelismConfig>(&json).unwrap(), config);
        }
    }
}
