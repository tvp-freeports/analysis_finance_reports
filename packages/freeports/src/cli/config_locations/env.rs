//! Configuration from the `FREEPORTS_*` environment variables.
//!
//! | variable | field |
//! |---|---|
//! | `FREEPORTS_URL`, `FREEPORTS_PDF` | contribute to the singular document spec, later resolved into the reports |
//! | `FREEPORTS_REPORTS` | the reports, several separated by the document separator |
//! | `FREEPORTS_VERBOSITY` | verbosity, by variant name, case-insensitively |
//! | `FREEPORTS_N_WORKERS` | the global parallelism default: a positive integer or `auto` |
//! | `FREEPORTS_PARALLELISM_JOBS` | the job-level override, same grammar |
//! | `FREEPORTS_PARALLELISM_PAGES` | the page-level override, same grammar |
//! | `FREEPORTS_BATCH_FILE` | batch file |
//! | `FREEPORTS_OUT_PATH` | output path |
//! | `FREEPORTS_OUT_PROFILE` | output profile, by name, case-insensitively |
//! | `FREEPORTS_SEPARATE_OUT` | the separate-output flag, `true` or `false` |
//! | `FREEPORTS_ARCHIVE` | the archive flag, `true` or `false` |
//! | `FREEPORTS_SAVE_PDF` | save PDF, `true` or `false` case-insensitively |
//! | `FREEPORTS_FORMAT` | format |
//! | `FREEPORTS_CONFIG_FILE` | configuration file |
//! | `FREEPORTS_TARGET_LIST` | the target lists — **one element**, the whole raw value, never split |
//! | `FREEPORTS_FORMATS_REPO_PATH` | formats repository path |
//! | `FREEPORTS_INPUT_DB_PATH` | input database path |
//!
//! An **absent** variable leaves its field unset, never an error. Giving both `FREEPORTS_REPORTS`
//! and one of the singular variables is a conflict, as in the file source: an override nobody can
//! see is worse than a refusal.

use std::path::PathBuf;

use crate::cli::conf_parse::{DOC_SPEC_SEPARATOR, DocumentSpec, DocumentSpecError};
use crate::cli::parallelism_config::Workers;
use crate::cli::partial_config::{PartialConfig, SourceReportsConflict, resolve_singular_and_plural_reports};
use crate::core::tracing_setup::Verbosity;
use crate::core::tracing_setup::log_error;
use crate::output::routines::write::OutStructureMode;



macro_rules! freeports_env {
    ($var:literal) => (concat!("FREEPORTS_",$var))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvConfigError {
    #[error("invalid document specifier {value:?} in {variable}: {source}")]
    InvalidReportSpecifier { variable: &'static str, value: String, source: DocumentSpecError },
    #[error("{source}")]
    ReportsConflict { source: SourceReportsConflict },
    #[error("invalid FREEPORTS_VERBOSITY {value:?}, expected one of: silent, erroronly, warn, info, debug, trace")]
    InvalidVerbosity { value: String },
    #[error("invalid FREEPORTS_OUT_PROFILE {value:?}, expected one of: regular, single_file, structured")]
    InvalidOutProfile { value: String },
    #[error("invalid value for {variable}: {value:?}")]
    InvalidValue { variable: &'static str, value: String },
}

fn env_var(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(e @ std::env::VarError::NotUnicode(_)) => {
            tracing::warn!(error = log_error(&e), name, "ignoring non-unicode environment variable: {e}");
            None
        }
    }
}

fn parse_verbosity(value: &str) -> Result<Verbosity, EnvConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "silent" => Ok(Verbosity::Silent),
        "error" => Ok(Verbosity::ErrorOnly),
        "warn" => Ok(Verbosity::Warn),
        "info" => Ok(Verbosity::Info),
        "debug" => Ok(Verbosity::Debug),
        "trace" => Ok(Verbosity::Trace),
        _ => Err(EnvConfigError::InvalidVerbosity { value: value.to_string() }),
    }
}

/// The output profile, by the same names the command line accepts, so that a run does not change
/// meaning when its profile moves from a flag into the environment.
fn parse_out_profile(value: &str) -> Result<OutStructureMode, EnvConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "regular" => Ok(OutStructureMode::Regular),
        "single_file" => Ok(OutStructureMode::SingleFile),
        "structured" => Ok(OutStructureMode::Structured),
        _ => Err(EnvConfigError::InvalidOutProfile { value: value.to_string() }),
    }
}

fn parse_bool(variable: &'static str, value: &str) -> Result<bool, EnvConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" | "y" | "t" => Ok(true),
        "false" | "no" | "0" | "n" | "f" => Ok(false),
        _ => Err(EnvConfigError::InvalidValue { variable, value: value.to_string() }),
    }
}

/// The three parallelism variables share the same grammar — `auto` or a positive integer — and
/// differ only in the name appearing in the error.
fn parse_workers(variable: &'static str, value: &str) -> Result<Workers, EnvConfigError> {
    Workers::parse(value).map_err(|_| EnvConfigError::InvalidValue { variable, value: value.to_string() })
}

/// Wraps `load_impl` to log any failure exactly once -- this is the only place all four
/// `EnvConfigError` variants are actually constructed (directly or via the small `parse_*`
/// helpers below).
pub fn load() -> Result<PartialConfig, EnvConfigError> {
    let result = load_impl();
    if let Err(e) = &result {
        tracing::error!(error = log_error(e), "invalid configuration from environment variables: {e}");
    }
    result
}

fn load_impl() -> Result<PartialConfig, EnvConfigError> {
    let url = env_var(freeports_env!("URL"));
    let pdf = env_var(freeports_env!("PDF"));
    let singular =
        if url.is_some() || pdf.is_some() { Some(DocumentSpec { url, path: pdf.map(PathBuf::from), name: None }) } else { None };

    let plural = env_var(freeports_env!("REPORTS"))
        .map(|value| {
            value
                .split(DOC_SPEC_SEPARATOR)
                .map(|s| {
                    DocumentSpec::parse(s).map_err(|source| EnvConfigError::InvalidReportSpecifier {
                        variable: freeports_env!("REPORTS"),
                        value: s.to_string(),
                        source,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;

    let reports = resolve_singular_and_plural_reports(singular, plural).map_err(|source| EnvConfigError::ReportsConflict { source })?;

    let verbosity = env_var(freeports_env!("VERBOSITY")).map(|v| parse_verbosity(&v)).transpose()?;
    let n_workers =
        env_var(freeports_env!("N_WORKERS")).map(|v| parse_workers(freeports_env!("N_WORKERS"), &v)).transpose()?;
    let parallelism_jobs = env_var(freeports_env!("PARALLELISM_JOBS"))
        .map(|v| parse_workers(freeports_env!("PARALLELISM_JOBS"), &v))
        .transpose()?;
    let parallelism_pages = env_var(freeports_env!("PARALLELISM_PAGES"))
        .map(|v| parse_workers(freeports_env!("PARALLELISM_PAGES"), &v))
        .transpose()?;
    let save_pdf = env_var(freeports_env!("SAVE_PDF")).map(|v| parse_bool(freeports_env!("SAVE_PDF"), &v)).transpose()?;
    let out_profile = env_var(freeports_env!("OUT_PROFILE")).map(|v| parse_out_profile(&v)).transpose()?;
    // The two output flags are read independently, each leaving its field unset when its variable
    // is absent: naming one of them must not switch the other off.
    let separate_out = env_var(freeports_env!("SEPARATE_OUT"))
        .map(|v| parse_bool(freeports_env!("SEPARATE_OUT"), &v))
        .transpose()?;
    let compressed =
        env_var(freeports_env!("ARCHIVE")).map(|v| parse_bool(freeports_env!("ARCHIVE"), &v)).transpose()?;

    Ok(PartialConfig {
        verbosity,
        reports,
        target_lists: env_var(freeports_env!("TARGET_LIST")).map(|v| vec![v]),
        format: env_var(freeports_env!("FORMAT")),
        out_path: env_var(freeports_env!("OUT_PATH")).map(PathBuf::from),
        out_profile,
        separate_out,
        compressed,
        n_workers,
        parallelism_jobs,
        parallelism_pages,
        batch_file: env_var(freeports_env!("BATCH_FILE")).map(PathBuf::from),
        save_pdf,
        formats_repo_path: env_var(freeports_env!("FORMATS_REPO_PATH")).map(PathBuf::from),
        input_db_path: env_var(freeports_env!("INPUT_DB_PATH")).map(PathBuf::from),
        config_file: env_var(freeports_env!("CONFIG_FILE")).map(PathBuf::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parallelism_config::Workers;
    use crate::core::tracing_setup::Verbosity;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// Every variable this module reads, used both to clear the environment before each test — so
    /// the developer's own shell cannot influence a result — and to restore it exactly afterwards.
    const ALL_VARS: &[&str] = &[
        freeports_env!("URL"),
        freeports_env!("PDF"),
        freeports_env!("REPORTS"),
        freeports_env!("VERBOSITY"),
        freeports_env!("N_WORKERS"),
        freeports_env!("PARALLELISM_JOBS"),
        freeports_env!("PARALLELISM_PAGES"),
        freeports_env!("BATCH_FILE"),
        freeports_env!("OUT_PATH"),
        freeports_env!("OUT_PROFILE"),
        freeports_env!("SEPARATE_OUT"),
        freeports_env!("ARCHIVE"),
        freeports_env!("SAVE_PDF"),
        freeports_env!("FORMAT"),
        freeports_env!("CONFIG_FILE"),
        freeports_env!("TARGET_LIST"),
        freeports_env!("FORMATS_REPO_PATH"),
        freeports_env!("INPUT_DB_PATH"),
    ];

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Serialises the tests of this module, environment variables being global to the process while
    /// tests run in parallel threads of one process, and restores the environment exactly as it
    /// was, variable by variable, at the end of the scope.
    struct EnvScope {
        _lock: std::sync::MutexGuard<'static, ()>,
        originals: Vec<(&'static str, Option<String>)>,
    }

    impl EnvScope {
        fn new() -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
            let originals: Vec<(&'static str, Option<String>)> =
                ALL_VARS.iter().map(|&k| (k, std::env::var(k).ok())).collect();
            for &k in ALL_VARS {
                // SAFETY: serialized by `ENV_LOCK`, no other thread in this test binary reads
                // `FREEPORTS_*` concurrently while this guard is alive.
                unsafe { std::env::remove_var(k) };
            }
            Self { _lock: lock, originals }
        }

        fn set(&self, key: &str, value: &str) {
            unsafe { std::env::set_var(key, value) };
        }
    }

    impl Drop for EnvScope {
        fn drop(&mut self) {
            for (k, v) in &self.originals {
                match v {
                    Some(val) => unsafe { std::env::set_var(k, val) },
                    None => unsafe { std::env::remove_var(k) },
                }
            }
        }
    }

    mod absence {
        use super::*;

        #[test]
        fn no_freeports_variables_set_yields_an_entirely_empty_partial_config() {
            let _scope = EnvScope::new();
            let config = load().unwrap();
            assert_eq!(config, crate::cli::partial_config::PartialConfig::default());
        }
    }

    mod simple_field_mapping {
        use super::*;

        #[test]
        fn out_path_is_mapped() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("OUT_PATH"), "/tmp/out");
            let config = load().unwrap();
            assert_eq!(config.out_path, Some(PathBuf::from("/tmp/out")));
        }

        #[test]
        fn batch_file_is_mapped() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("BATCH_FILE"), "/tmp/batch.csv");
            let config = load().unwrap();
            assert_eq!(config.batch_file, Some(PathBuf::from("/tmp/batch.csv")));
        }

        #[test]
        fn format_is_mapped_as_a_raw_passthrough_string() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("FORMAT"), "ACME-EN24");
            let config = load().unwrap();
            assert_eq!(config.format, Some("ACME-EN24".to_string()));
        }

        #[test]
        fn config_file_is_mapped() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("CONFIG_FILE"), "/etc/freeports.yaml");
            let config = load().unwrap();
            assert_eq!(config.config_file, Some(PathBuf::from("/etc/freeports.yaml")));
        }

        #[test]
        fn formats_repo_path_is_mapped() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("FORMATS_REPO_PATH"), "/opt/formats");
            let config = load().unwrap();
            assert_eq!(config.formats_repo_path, Some(PathBuf::from("/opt/formats")));
        }

        #[test]
        fn input_db_path_is_mapped() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("INPUT_DB_PATH"), "/opt/db");
            let config = load().unwrap();
            assert_eq!(config.input_db_path, Some(PathBuf::from("/opt/db")));
        }

        #[test]
        fn n_workers_is_mapped_as_a_positive_integer() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("N_WORKERS"), "4");
            let config = load().unwrap();
            assert_eq!(config.n_workers, Some(Workers::Fixed(4)));
        }

        /// The three variables accept the word `auto`, which is how a level is brought back to
        /// automatic after a configuration file has pinned it.
        #[test]
        fn the_three_parallelism_variables_accept_auto() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("N_WORKERS"), "auto");
            scope.set(freeports_env!("PARALLELISM_JOBS"), "AUTO");
            scope.set(freeports_env!("PARALLELISM_PAGES"), "auto");
            let config = load().unwrap();
            assert_eq!(config.n_workers, Some(Workers::Auto));
            assert_eq!(config.parallelism_jobs, Some(Workers::Auto));
            assert_eq!(config.parallelism_pages, Some(Workers::Auto));
        }

        #[test]
        fn the_two_per_level_variables_are_mapped_separately() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("PARALLELISM_JOBS"), "2");
            scope.set(freeports_env!("PARALLELISM_PAGES"), "8");
            let config = load().unwrap();
            assert_eq!(config.n_workers, None);
            assert_eq!(config.parallelism_jobs, Some(Workers::Fixed(2)));
            assert_eq!(config.parallelism_pages, Some(Workers::Fixed(8)));
        }

        #[test]
        fn a_malformed_per_level_variable_names_itself_in_the_error() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("PARALLELISM_PAGES"), "lots");
            let error = load().unwrap_err().to_string();
            assert!(error.contains(freeports_env!("PARALLELISM_PAGES")), "{error}");
        }

        #[test]
        fn zero_is_rejected_on_the_per_level_variables_too() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("PARALLELISM_JOBS"), "0");
            assert!(load().is_err());
        }

        #[test]
        fn n_workers_zero_is_rejected_not_positive() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("N_WORKERS"), "0");
            assert!(load().is_err());
        }

        #[test]
        fn n_workers_non_numeric_is_a_typed_error_not_a_panic() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("N_WORKERS"), "not-a-number");
            let result = std::panic::catch_unwind(|| load());
            assert!(result.is_ok(), "must not panic");
            assert!(result.unwrap().is_err());
        }

        #[test_case::test_case("true", true; "lowercase true")]
        #[test_case::test_case("TRUE", true; "uppercase true")]
        #[test_case::test_case("false", false; "lowercase false")]
        #[test_case::test_case("FALSE", false; "uppercase false")]
        fn save_pdf_accepts_case_insensitive_booleans(value: &str, expected: bool) {
            let scope = EnvScope::new();
            scope.set(freeports_env!("SAVE_PDF"), value);
            let config = load().unwrap();
            assert_eq!(config.save_pdf, Some(expected));
        }

        #[test]
        fn save_pdf_with_an_unrecognized_value_is_a_typed_error() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("SAVE_PDF"), "maybe");
            assert!(load().is_err());
        }

        #[test]
        fn target_list_becomes_a_single_element_list_never_split() {
            // Matches the reference's `Lists` `BeforeValidator`: a single env string becomes a
            // one-element list, not split on any separator (even if it happens to contain a
            // comma or the batch/report separator).
            let scope = EnvScope::new();
            scope.set(freeports_env!("TARGET_LIST"), "TEST,OTHER");
            let config = load().unwrap();
            assert_eq!(config.target_lists, Some(vec!["TEST,OTHER".to_string()]));
        }
    }

    /// The output profile and the two output flags, added to this source so that a run's shape can
    /// be fixed by the environment rather than repeated on every command line.
    mod output_shape {
        use super::*;
        use crate::output::routines::write::OutStructureMode;

        #[test_case::test_case("regular", OutStructureMode::Regular)]
        #[test_case::test_case("single_file", OutStructureMode::SingleFile)]
        #[test_case::test_case("structured", OutStructureMode::Structured)]
        #[test_case::test_case("STRUCTURED", OutStructureMode::Structured; "case insensitive")]
        #[test_case::test_case("Single_File", OutStructureMode::SingleFile; "mixed case")]
        fn every_profile_name_is_accepted_as_the_command_line_spells_it(value: &str, expected: OutStructureMode) {
            let scope = EnvScope::new();
            scope.set(freeports_env!("OUT_PROFILE"), value);
            assert_eq!(load().unwrap().out_profile, Some(expected));
        }

        #[test]
        fn an_unrecognized_profile_name_is_a_typed_error_naming_the_alternatives() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("OUT_PROFILE"), "flat");
            let error = load().unwrap_err();
            assert_eq!(error, EnvConfigError::InvalidOutProfile { value: "flat".to_string() });
            assert!(error.to_string().contains("single_file"), "{error}");
        }

        #[test]
        fn an_unrecognized_profile_name_does_not_panic() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("OUT_PROFILE"), "flat");
            let result = std::panic::catch_unwind(load);
            assert!(result.is_ok(), "must not panic");
            assert!(result.unwrap().is_err());
        }

        #[test]
        fn an_absent_profile_stays_none_not_the_regular_default() {
            let _scope = EnvScope::new();
            assert_eq!(load().unwrap().out_profile, None);
        }

        #[test_case::test_case("true", true; "lowercase true")]
        #[test_case::test_case("YES", true; "uppercase yes")]
        #[test_case::test_case("1", true; "one")]
        #[test_case::test_case("false", false; "lowercase false")]
        #[test_case::test_case("NO", false; "uppercase no")]
        #[test_case::test_case("0", false; "zero")]
        fn the_separate_out_flag_accepts_the_same_booleans_as_save_pdf(value: &str, expected: bool) {
            let scope = EnvScope::new();
            scope.set(freeports_env!("SEPARATE_OUT"), value);
            assert_eq!(load().unwrap().separate_out, Some(expected));
        }

        #[test_case::test_case("true", true; "lowercase true")]
        #[test_case::test_case("t", true; "t")]
        #[test_case::test_case("false", false; "lowercase false")]
        #[test_case::test_case("f", false; "f")]
        fn the_archive_flag_accepts_the_same_booleans(value: &str, expected: bool) {
            let scope = EnvScope::new();
            scope.set(freeports_env!("ARCHIVE"), value);
            assert_eq!(load().unwrap().compressed, Some(expected));
        }

        /// The point of keeping the two flags apart: naming one in the environment must leave the
        /// other free to come from the configuration file.
        #[test]
        fn setting_one_flag_leaves_the_other_unset() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("ARCHIVE"), "true");
            let config = load().unwrap();
            assert_eq!(config.compressed, Some(true));
            assert_eq!(config.separate_out, None);
        }

        #[test]
        fn both_flags_can_be_set_at_once() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("SEPARATE_OUT"), "true");
            scope.set(freeports_env!("ARCHIVE"), "true");
            let config = load().unwrap();
            assert_eq!(config.separate_out, Some(true));
            assert_eq!(config.compressed, Some(true));
        }

        /// `false` is a real statement, not the absence of one: it is how a configuration file's
        /// flag is switched back off from the environment.
        #[test]
        fn an_explicit_false_is_recorded_as_some_false_not_as_absence() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("SEPARATE_OUT"), "false");
            assert_eq!(load().unwrap().separate_out, Some(false));
        }

        #[test_case::test_case(freeports_env!("SEPARATE_OUT"))]
        #[test_case::test_case(freeports_env!("ARCHIVE"))]
        fn an_unrecognized_boolean_is_a_typed_error_naming_its_variable(variable: &str) {
            let scope = EnvScope::new();
            scope.set(variable, "sometimes");
            let error = load().unwrap_err().to_string();
            assert!(error.contains(variable), "{error}");
        }

        #[test]
        fn absent_flags_stay_none_so_that_a_lower_tier_still_decides() {
            let _scope = EnvScope::new();
            let config = load().unwrap();
            assert_eq!(config.separate_out, None);
            assert_eq!(config.compressed, None);
        }
    }

    mod verbosity {
        use super::*;

        #[test_case::test_case("silent", Verbosity::Silent)]
        #[test_case::test_case("error", Verbosity::ErrorOnly)]
        #[test_case::test_case("warn", Verbosity::Warn)]
        #[test_case::test_case("info", Verbosity::Info)]
        #[test_case::test_case("debug", Verbosity::Debug)]
        #[test_case::test_case("trace", Verbosity::Trace)]
        fn every_variant_name_is_accepted_lowercase(value: &str, expected: Verbosity) {
            let scope = EnvScope::new();
            scope.set(freeports_env!("VERBOSITY"), value);
            let config = load().unwrap();
            assert_eq!(config.verbosity, Some(expected));
        }

        #[test_case::test_case("SILENT")]
        #[test_case::test_case("Warn")]
        #[test_case::test_case("TRACE")]
        fn variant_names_are_case_insensitive(value: &str) {
            let scope = EnvScope::new();
            scope.set(freeports_env!("VERBOSITY"), value);
            assert!(load().is_ok());
        }

        #[test]
        fn an_unrecognized_verbosity_string_is_a_typed_error_not_a_panic() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("VERBOSITY"), "abc");
            let result = std::panic::catch_unwind(|| load());
            assert!(result.is_ok(), "must not panic");
            assert!(result.unwrap().is_err());
        }

        #[test]
        fn absent_verbosity_stays_none_not_a_default() {
            // The `Some(Warn)` default lives in `partial_config::defaults()`, not in this
            // source's own output -- `env::load` reports absence honestly as `None`.
            let _scope = EnvScope::new();
            let config = load().unwrap();
            assert_eq!(config.verbosity, None);
        }
    }

    mod reports_singular {
        use super::*;

        #[test]
        fn url_alone_becomes_a_one_element_reports_list_with_only_a_url() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("URL"), "https://example.com/report.pdf");
            let config = load().unwrap();
            let reports = config.reports.expect("reports must be set");
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/report.pdf"));
            assert_eq!(reports[0].path, None);
        }

        #[test]
        fn pdf_alone_becomes_a_one_element_reports_list_with_only_a_path() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("PDF"), "/tmp/report.pdf");
            let config = load().unwrap();
            let reports = config.reports.expect("reports must be set");
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].path, Some(PathBuf::from("/tmp/report.pdf")));
            assert_eq!(reports[0].url, None);
        }

        #[test]
        fn url_and_pdf_together_combine_into_a_single_spec() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("URL"), "https://example.com/report.pdf");
            scope.set(freeports_env!("PDF"), "/tmp/report.pdf");
            let config = load().unwrap();
            let reports = config.reports.expect("reports must be set");
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/report.pdf"));
            assert_eq!(reports[0].path, Some(PathBuf::from("/tmp/report.pdf")));
        }

        #[test]
        fn neither_url_nor_pdf_leaves_reports_none() {
            let _scope = EnvScope::new();
            let config = load().unwrap();
            assert_eq!(config.reports, None);
        }
    }

    mod reports_plural {
        use super::*;

        #[test]
        fn a_single_element_reports_list_parses_one_specifier() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("REPORTS"), "https://example.com/a.pdf");
            let config = load().unwrap();
            let reports = config.reports.unwrap();
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/a.pdf"));
        }

        #[test]
        fn multiple_elements_are_split_on_the_shared_doc_spec_separator_in_order() {
            let scope = EnvScope::new();
            let value = format!(
                "https://example.com/a.pdf{sep}https://example.com/b.pdf{sep}https://example.com/c.pdf",
                sep = crate::cli::conf_parse::DOC_SPEC_SEPARATOR
            );
            scope.set(freeports_env!("REPORTS"), &value);
            let config = load().unwrap();
            let reports = config.reports.unwrap();
            assert_eq!(reports.len(), 3);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/a.pdf"));
            assert_eq!(reports[1].url.as_deref(), Some("https://example.com/b.pdf"));
            assert_eq!(reports[2].url.as_deref(), Some("https://example.com/c.pdf"));
        }

        #[test]
        fn each_element_uses_the_full_document_spec_grammar() {
            let scope = EnvScope::new();
            let value = format!(
                "https://example.com/a.pdf:report-a.pdf:Report A{sep}report-b.pdf:Report B",
                sep = crate::cli::conf_parse::DOC_SPEC_SEPARATOR
            );
            scope.set(freeports_env!("REPORTS"), &value);
            let config = load().unwrap();
            let reports = config.reports.unwrap();
            assert_eq!(reports.len(), 2);
            assert_eq!(reports[0].name.as_deref(), Some("Report A"));
            assert_eq!(reports[1].name.as_deref(), Some("Report B"));
        }

        #[test]
        fn an_invalid_element_is_a_typed_error_not_a_panic() {
            let scope = EnvScope::new();
            let value = format!("a:b:c:d{sep}ok.pdf", sep = crate::cli::conf_parse::DOC_SPEC_SEPARATOR);
            scope.set(freeports_env!("REPORTS"), &value);
            let result = std::panic::catch_unwind(|| load());
            assert!(result.is_ok(), "must not panic");
            assert!(result.unwrap().is_err());
        }
    }

    mod reports_singular_and_plural_conflict {
        use super::*;

        #[test]
        fn reports_and_url_together_is_an_explicit_error() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("REPORTS"), "https://example.com/a.pdf");
            scope.set(freeports_env!("URL"), "https://example.com/b.pdf");
            assert!(load().is_err(), "FREEPORTS_REPORTS + FREEPORTS_URL together must be rejected, not silently merged");
        }

        #[test]
        fn reports_and_pdf_together_is_an_explicit_error() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("REPORTS"), "https://example.com/a.pdf");
            scope.set(freeports_env!("PDF"), "/tmp/report.pdf");
            assert!(load().is_err());
        }

        #[test]
        fn reports_alone_without_url_or_pdf_is_never_a_conflict() {
            let scope = EnvScope::new();
            scope.set(freeports_env!("REPORTS"), "https://example.com/a.pdf");
            assert!(load().is_ok());
        }
    }

    mod not_a_panic_regardless_of_input {
        use super::*;

        #[test]
        fn every_freeports_variable_set_to_garbage_at_once_never_panics() {
            let scope = EnvScope::new();
            for &var in ALL_VARS {
                // No interior NUL byte: `std::env::set_var` itself panics on one (an OS-level
                // constraint on environment variable values, not something `load()` could ever
                // see) -- the point here is exercising `load()` with garbage it *can* receive.
                scope.set(var, "not\u{1}valid???€");
            }
            let result = std::panic::catch_unwind(|| load());
            assert!(result.is_ok(), "env::load must never panic, regardless of how malformed the environment is");
        }
    }
}
