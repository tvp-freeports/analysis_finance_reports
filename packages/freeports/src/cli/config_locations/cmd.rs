//! [`CliArgs`]: the command line, and its conversion into a partial configuration.
//!
//! # Flag to field
//!
//! | flag | field | note |
//! |---|---|---|
//! | `--input`/`--report`/`-i` (repeatable) | reports | each value parsed as a document spec; none given leaves the field unset rather than an empty list |
//! | `--batch`/`-b` | batch file | |
//! | `--workers`/`-j` | the global parallelism default | a positive integer **or** `auto`; zero and negatives are a typed error |
//! | `--jobs` | the job-level override | same grammar |
//! | `--pages` | the page-level override | same grammar |
//! | `--format`/`-f` | format | |
//! | `--no-download` | save PDF | present means false; absent leaves it unset, never true — the real default lives in the defaults tier |
//! | `--config` | configuration file | |
//! | `--out`/`-o` | output path | |
//! | `-v`/`-q` (counted) | verbosity | only when at least one is given; otherwise unset |
//! | `--target-list`/`-T` (repeatable) | target lists | none given leaves the field unset |
//! | `--separate-out`, `--archive` | output flags | combined into one value; neither given leaves it unset |
//! | `--out-profile`/`-P` | output profile | one of the profile names, case-insensitively |
//! | `--db-directory`/`-I` | input database path | |
//! | `--formats-directory`/`-F`/`--repo`/`-r` | formats repository path | |
//!
//! Repeatedly: a flag that is absent leaves its field **unset**, never set to a default. Defaults
//! belong to the defaults tier, and a command line that sets everything would make every other
//! source unreachable.
//!
//! `-v` and `-q` are **independent dials** and using them together is never an error.

use std::path::PathBuf;

use crate::cli::conf_parse::{DocumentSpec, DocumentSpecError};
use crate::cli::parallelism_config::{Workers, WorkersParseError};
use crate::cli::partial_config::PartialConfig;
use crate::core::tracing_setup::Verbosity;
use crate::output::routines::write::{OutFlags, OutStructureMode};
use crate::core::tracing_setup::log_error;

#[derive(Debug, Clone, clap::Parser)]
#[command(about = "Estrae dati strutturati da report finanziari in formato PDF")]
pub struct CliArgs {
    #[arg(long = "input", visible_alias = "report", short = 'i', num_args = 1..)]
    pub input: Vec<String>,
    #[arg(long = "batch", short = 'b')]
    pub batch: Option<String>,
    // Hyphen values must be allowed: without that, a negative value is taken for an unknown option
    // rather than for this option's argument, and the typed error for a non-positive count could
    // never even be reached.
    //
    // This is the global default of *both* parallelism levels, not merely a process count. It is
    // what gives `-j 1` a universal meaning — one job at a time *and* one page at a time.
    #[arg(long = "workers", short = 'j', allow_hyphen_values = true, help = "Workers to use at every parallelism level: a positive number, or 'auto'")]
    pub workers: Option<String>,
    // The job-level override. Long form only: the short form is already taken by the global
    // default.
    #[arg(long = "jobs", allow_hyphen_values = true, help = "Documents processed at once, in separate processes [default: --workers]")]
    pub jobs: Option<String>,
    // The page-level override.
    #[arg(long = "pages", allow_hyphen_values = true, help = "Pages of one document processed at once, in threads [default: --workers]")]
    pub pages: Option<String>,
    #[arg(long = "format", short = 'f')]
    pub format: Option<String>,
    #[arg(long = "no-download")]
    pub no_download: bool,
    #[arg(long = "separate-out")]
    pub separate_out: bool,
    #[arg(long = "config")]
    pub config: Option<String>,
    #[arg(long = "out", short = 'o')]
    pub out: Option<String>,
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    pub verbose: u8,
    #[arg(short = 'q', action = clap::ArgAction::Count)]
    pub quiet: u8,
    #[arg(long = "target-list", short = 'T', num_args = 1..)]
    pub target_list: Vec<String>,
    #[arg(long = "archive", short = 'z')]
    pub archive: bool,
    #[arg(long = "out-profile", short = 'P')]
    pub out_profile: Option<String>,
    #[arg(long = "db-directory", short = 'I')]
    pub db_directory: Option<String>,
    #[arg(long = "formats-directory", visible_alias = "repo", short = 'F', visible_short_alias = 'r')]
    pub formats_directory: Option<String>,
    /// Runs **one** worker job, read from the file named, instead of resolving a configuration. A
    /// parent passes it to itself when running the jobs of a batch in child processes.
    ///
    /// Hidden from the help on purpose: it is not a user interface but the internal channel between
    /// two copies of the same binary. Appearing in `--help` would invite using it by hand, where it
    /// makes no sense — the request file is written by the parent, with paths inside a temporary
    /// directory that disappears when the run ends.
    ///
    /// It is deliberately **not** mapped into the partial configuration: it is a mode of the
    /// process, not a configuration value.
    #[arg(long = "internal-worker", hide = true)]
    pub internal_worker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CmdConfigError {
    #[error("invalid document specifier {value:?}: {source}")]
    InvalidReportSpecifier { value: String, source: DocumentSpecError },
    #[error("invalid output profile {value:?}, expected one of: regular, single_file, structured")]
    InvalidOutProfile { value: String },
    #[error("{flag} {source}")]
    InvalidWorkers { flag: &'static str, #[source] source: WorkersParseError },
}

/// The three parallelism options share one grammar and differ only in the name appearing in the
/// error: someone who mistyped `--pages` must read `--pages`, not a generic message about a
/// concept.
fn parse_workers(flag: &'static str, value: Option<&str>) -> Result<Option<Workers>, CmdConfigError> {
    value
        .map(|v| Workers::parse(v).map_err(|source| CmdConfigError::InvalidWorkers { flag, source }))
        .transpose()
}

fn parse_out_profile(value: &str) -> Result<OutStructureMode, CmdConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "regular" => Ok(OutStructureMode::Regular),
        "single_file" => Ok(OutStructureMode::SingleFile),
        "structured" => Ok(OutStructureMode::Structured),
        _ => Err(CmdConfigError::InvalidOutProfile { value: value.to_string() }),
    }
}

impl CliArgs {
    /// Wraps `Self::to_partial_config_impl` to log any conversion failure exactly once, at the
    /// point where the command-line arguments are turned into a `PartialConfig` -- this is the
    /// only place all three `CmdConfigError` variants are actually constructed.
    pub fn to_partial_config(&self) -> Result<PartialConfig, CmdConfigError> {
        let result = self.to_partial_config_impl();
        if let Err(e) = &result {
            tracing::error!(error = log_error(e), "invalid command-line arguments: {e}");
        }
        result
    }

    fn to_partial_config_impl(&self) -> Result<PartialConfig, CmdConfigError> {
        let reports = if self.input.is_empty() {
            None
        } else {
            let specs: Result<Vec<DocumentSpec>, CmdConfigError> = self
                .input
                .iter()
                .map(|s| {
                    DocumentSpec::parse(s)
                        .map_err(|source| CmdConfigError::InvalidReportSpecifier { value: s.clone(), source })
                })
                .collect();
            Some(specs?)
        };

        let n_workers = parse_workers("--workers", self.workers.as_deref())?;
        let parallelism_jobs = parse_workers("--jobs", self.jobs.as_deref())?;
        let parallelism_pages = parse_workers("--pages", self.pages.as_deref())?;

        let verbosity = if self.verbose > 0 || self.quiet > 0 {
            Some(Verbosity::from_verbose_and_quiet_counts(self.verbose, self.quiet))
        } else {
            None
        };

        let mut out_flags = OutFlags::default();
        let mut any_out_flag = false;
        if self.separate_out {
            out_flags.separate_out = true;
            any_out_flag = true;
        }
        if self.archive {
            out_flags.compressed = true;
            any_out_flag = true;
        }
        let out_flags = if any_out_flag { Some(out_flags) } else { None };

        let out_profile = self.out_profile.as_deref().map(parse_out_profile).transpose()?;

        let target_lists = if self.target_list.is_empty() { None } else { Some(self.target_list.clone()) };

        Ok(PartialConfig {
            verbosity,
            reports,
            target_lists,
            format: self.format.clone(),
            out_path: self.out.as_ref().map(PathBuf::from),
            out_profile,
            out_flags,
            n_workers,
            parallelism_jobs,
            parallelism_pages,
            batch_file: self.batch.as_ref().map(PathBuf::from),
            save_pdf: if self.no_download { Some(false) } else { None },
            formats_repo_path: self.formats_directory.as_ref().map(PathBuf::from),
            input_db_path: self.db_directory.as_ref().map(PathBuf::from),
            config_file: self.config.as_ref().map(PathBuf::from),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tracing_setup::Verbosity;
    use crate::output::routines::write::{OutFlags, OutStructureMode};
    use clap::Parser;
    use std::path::PathBuf;

    fn parse(args: &[&str]) -> CliArgs {
        let mut full = vec!["freeports"];
        full.extend_from_slice(args);
        CliArgs::try_parse_from(full).expect("argv must parse")
    }

    /// The hidden worker flag is not a configuration option but the way one copy of the binary
    /// tells another to run a job. Its whole contract is here: it parses, it does not appear in the
    /// help, and it contributes nothing to the resolved configuration.
    mod internal_worker_flag {
        use super::*;

        #[test]
        fn it_parses_and_carries_the_request_path() {
            let args = parse(&["--internal-worker", "/tmp/w3/request.json"]);
            assert_eq!(args.internal_worker.as_deref(), Some("/tmp/w3/request.json"));
        }

        #[test]
        fn it_is_absent_when_not_given() {
            assert_eq!(parse(&[]).internal_worker, None);
        }

        /// Hiding it is not cosmetic: a user who found it in the help would try to use it, and the
        /// paths it expects are written only by the parent, in a temporary directory.
        #[test]
        fn it_does_not_appear_in_the_help_text() {
            use clap::CommandFactory;
            let help = CliArgs::command().render_help().to_string();
            assert!(!help.contains("internal-worker"), "the internal channel must not be advertised in --help:\n{help}");
        }

        /// Were it to reach the partial configuration, the request path would become a
        /// configuration value merging with file and environment — and a child could inherit worker
        /// mode from a configuration file, recursively.
        #[test]
        fn it_contributes_nothing_to_the_resolved_configuration() {
            let with = parse(&["--internal-worker", "/tmp/w3/request.json"]).to_partial_config().unwrap();
            let without = parse(&[]).to_partial_config().unwrap();
            assert_eq!(with, without);
        }
    }

    mod clap_parsing_shape {
        use super::*;

        #[test]
        fn no_arguments_at_all_parses_successfully_with_empty_defaults() {
            let args = parse(&[]);
            assert!(args.input.is_empty());
            assert_eq!(args.batch, None);
            assert_eq!(args.verbose, 0);
            assert_eq!(args.quiet, 0);
        }

        #[test]
        fn an_unknown_flag_is_rejected_by_clap() {
            let result = CliArgs::try_parse_from(["freeports", "--this-flag-does-not-exist"]);
            assert!(result.is_err());
        }

        #[test]
        fn input_accepts_multiple_values_in_a_single_flag_occurrence() {
            let args = parse(&["--input", "a.pdf", "b.pdf", "c.pdf"]);
            assert_eq!(args.input, vec!["a.pdf", "b.pdf", "c.pdf"]);
        }

        #[test]
        fn short_flag_i_is_an_alias_for_input() {
            let args = parse(&["-i", "a.pdf"]);
            assert_eq!(args.input, vec!["a.pdf"]);
        }

        #[test]
        fn report_is_an_alias_for_input() {
            let args = parse(&["--report", "a.pdf"]);
            assert_eq!(args.input, vec!["a.pdf"]);
        }

        #[test]
        fn v_count_accumulates_across_repetitions() {
            let args = parse(&["-vvv"]);
            assert_eq!(args.verbose, 3);
        }

        #[test]
        fn q_count_accumulates_across_repetitions() {
            let args = parse(&["-qq"]);
            assert_eq!(args.quiet, 2);
        }

        #[test]
        fn v_and_q_together_is_accepted_by_clap_itself() {
            // Independent dials: parsing must not reject the combination, and nothing downstream
            // rejects it either.
            assert!(CliArgs::try_parse_from(["freeports", "-v", "-q"]).is_ok());
        }

        #[test]
        fn repo_is_an_alias_for_formats_directory() {
            let args = parse(&["--repo", "/opt/formats"]);
            assert_eq!(args.formats_directory.as_deref(), Some("/opt/formats"));
        }

        #[test]
        fn short_r_is_an_alias_for_formats_directory() {
            let args = parse(&["-r", "/opt/formats"]);
            assert_eq!(args.formats_directory.as_deref(), Some("/opt/formats"));
        }
    }

    mod to_partial_config_simple_fields {
        use super::*;

        #[test]
        fn batch_file_is_mapped() {
            let config = parse(&["--batch", "jobs.csv"]).to_partial_config().unwrap();
            assert_eq!(config.batch_file, Some(PathBuf::from("jobs.csv")));
        }

        #[test]
        fn format_is_mapped() {
            let config = parse(&["--format", "ACME-EN24"]).to_partial_config().unwrap();
            assert_eq!(config.format, Some("ACME-EN24".to_string()));
        }

        #[test]
        fn config_file_is_mapped() {
            let config = parse(&["--config", "/etc/freeports.yaml"]).to_partial_config().unwrap();
            assert_eq!(config.config_file, Some(PathBuf::from("/etc/freeports.yaml")));
        }

        #[test]
        fn out_path_is_mapped() {
            let config = parse(&["--out", "/tmp/out"]).to_partial_config().unwrap();
            assert_eq!(config.out_path, Some(PathBuf::from("/tmp/out")));
        }

        #[test]
        fn workers_is_mapped() {
            let config = parse(&["--workers", "4"]).to_partial_config().unwrap();
            assert_eq!(config.n_workers, Some(Workers::Fixed(4)));
        }

        /// `auto` is a word all three options accept, not merely an implicit default: it is what
        /// brings a level back to automatic when a lower-priority source has pinned it to a number.
        #[test]
        fn auto_is_accepted_by_all_three_parallelism_options() {
            let config = parse(&["--workers", "auto", "--jobs", "AUTO", "--pages", "auto"])
                .to_partial_config()
                .unwrap();
            assert_eq!(config.n_workers, Some(Workers::Auto));
            assert_eq!(config.parallelism_jobs, Some(Workers::Auto));
            assert_eq!(config.parallelism_pages, Some(Workers::Auto));
        }

        #[test]
        fn the_two_per_level_options_are_mapped_separately() {
            let config = parse(&["--jobs", "2", "--pages", "8"]).to_partial_config().unwrap();
            assert_eq!(config.n_workers, None);
            assert_eq!(config.parallelism_jobs, Some(Workers::Fixed(2)));
            assert_eq!(config.parallelism_pages, Some(Workers::Fixed(8)));
        }

        #[test]
        fn none_of_the_three_options_is_set_when_none_is_given() {
            let config = parse(&[]).to_partial_config().unwrap();
            assert_eq!(config.n_workers, None);
            assert_eq!(config.parallelism_jobs, None);
            assert_eq!(config.parallelism_pages, None);
        }

        /// The message must name **the option that was wrong**: with three options sharing a
        /// grammar, a generic error would leave the reader to guess which of the three.
        #[test]
        fn the_error_names_the_option_that_was_wrong() {
            let error = parse(&["--pages", "nope"]).to_partial_config().unwrap_err().to_string();
            assert!(error.contains("--pages"), "{error}");
            assert!(error.contains("\"nope\""), "{error}");
        }

        #[test]
        fn zero_or_negative_is_a_typed_error_on_the_per_level_options_too() {
            assert!(parse(&["--jobs", "0"]).to_partial_config().is_err());
            assert!(parse(&["--pages", "-2"]).to_partial_config().is_err());
        }

        #[test]
        fn zero_or_negative_workers_is_a_typed_error() {
            assert!(parse(&["--workers", "0"]).to_partial_config().is_err());
            assert!(parse(&["--workers", "-1"]).to_partial_config().is_err());
        }

        #[test]
        fn db_directory_maps_to_input_db_path() {
            let config = parse(&["--db-directory", "/opt/db"]).to_partial_config().unwrap();
            assert_eq!(config.input_db_path, Some(PathBuf::from("/opt/db")));
        }

        #[test]
        fn formats_directory_maps_to_formats_repo_path() {
            let config = parse(&["--formats-directory", "/opt/formats"]).to_partial_config().unwrap();
            assert_eq!(config.formats_repo_path, Some(PathBuf::from("/opt/formats")));
        }

        #[test]
        fn target_list_is_mapped_in_order() {
            let config = parse(&["--target-list", "TEST", "OTHER"]).to_partial_config().unwrap();
            assert_eq!(config.target_lists, Some(vec!["TEST".to_string(), "OTHER".to_string()]));
        }

        #[test]
        fn no_target_list_flag_leaves_the_field_none_not_an_empty_list() {
            let config = parse(&[]).to_partial_config().unwrap();
            assert_eq!(config.target_lists, None);
        }
    }

    mod save_pdf {
        use super::*;

        #[test]
        fn no_download_flag_sets_save_pdf_to_false() {
            let config = parse(&["--no-download"]).to_partial_config().unwrap();
            assert_eq!(config.save_pdf, Some(false));
        }

        #[test]
        fn absent_flag_leaves_save_pdf_none() {
            let config = parse(&[]).to_partial_config().unwrap();
            assert_eq!(config.save_pdf, None, "cmd must never inject a Some(true) default -- that belongs to defaults()");
        }
    }

    mod verbosity_wiring {
        use super::*;

        #[test]
        fn neither_v_nor_q_leaves_verbosity_none() {
            let config = parse(&[]).to_partial_config().unwrap();
            assert_eq!(config.verbosity, None, "cmd must not inject Some(Warn) when unused -- that belongs to defaults()");
        }

        #[test]
        fn v_alone_sets_verbosity_from_the_shared_formula() {
            let config = parse(&["-vv"]).to_partial_config().unwrap();
            assert_eq!(config.verbosity, Some(Verbosity::from_verbose_and_quiet_counts(2, 0)));
        }

        #[test]
        fn q_alone_sets_verbosity_from_the_shared_formula() {
            let config = parse(&["-q"]).to_partial_config().unwrap();
            assert_eq!(config.verbosity, Some(Verbosity::from_verbose_and_quiet_counts(0, 1)));
        }

        #[test]
        fn v_and_q_together_is_not_an_error_and_uses_the_net_offset() {
            let config = parse(&["-vv", "-q"]).to_partial_config().unwrap();
            assert_eq!(config.verbosity, Some(Verbosity::from_verbose_and_quiet_counts(2, 1)));
        }
    }

    mod out_flags_wiring {
        use super::*;

        #[test]
        fn neither_flag_leaves_out_flags_none() {
            let config = parse(&[]).to_partial_config().unwrap();
            assert_eq!(config.out_flags, None);
        }

        #[test]
        fn separate_out_alone() {
            let config = parse(&["--separate-out"]).to_partial_config().unwrap();
            assert_eq!(config.out_flags, Some(OutFlags { compressed: false, separate_out: true }));
        }

        #[test]
        fn archive_alone_sets_compressed() {
            let config = parse(&["--archive"]).to_partial_config().unwrap();
            assert_eq!(config.out_flags, Some(OutFlags { compressed: true, separate_out: false }));
        }

        #[test]
        fn both_together() {
            let config = parse(&["--separate-out", "--archive"]).to_partial_config().unwrap();
            assert_eq!(config.out_flags, Some(OutFlags { compressed: true, separate_out: true }));
        }
    }

    mod out_profile_wiring {
        use super::*;

        #[test_case::test_case("regular", OutStructureMode::Regular)]
        #[test_case::test_case("single_file", OutStructureMode::SingleFile)]
        #[test_case::test_case("structured", OutStructureMode::Structured)]
        #[test_case::test_case("REGULAR", OutStructureMode::Regular; "case insensitive")]
        fn recognized_values_map_to_the_expected_mode(value: &str, expected: OutStructureMode) {
            let config = parse(&["--out-profile", value]).to_partial_config().unwrap();
            assert_eq!(config.out_profile, Some(expected));
        }

        #[test]
        fn an_unrecognized_value_is_a_typed_error_not_a_panic() {
            let result = std::panic::catch_unwind(|| parse(&["--out-profile", "not-a-mode"]).to_partial_config());
            assert!(result.is_ok(), "must not panic");
            assert!(result.unwrap().is_err());
        }

        #[test]
        fn absent_flag_leaves_it_none() {
            let config = parse(&[]).to_partial_config().unwrap();
            assert_eq!(config.out_profile, None);
        }
    }

    mod input_report_specs {
        use super::*;

        #[test]
        fn no_input_flag_leaves_reports_none_not_an_empty_list() {
            let config = parse(&[]).to_partial_config().unwrap();
            assert_eq!(config.reports, None);
        }

        #[test]
        fn each_value_is_parsed_with_the_full_document_spec_grammar_in_order() {
            let config = parse(&["--input", "https://example.com/a.pdf", "report-b.pdf:Report B"])
                .to_partial_config()
                .unwrap();
            let reports = config.reports.unwrap();
            assert_eq!(reports.len(), 2);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/a.pdf"));
            assert_eq!(reports[1].name.as_deref(), Some("Report B"));
        }

        #[test]
        fn an_invalid_specifier_is_a_typed_error_not_a_panic() {
            let result = std::panic::catch_unwind(|| parse(&["--input", "a:b:c:d"]).to_partial_config());
            assert!(result.is_ok(), "must not panic");
            assert!(result.unwrap().is_err());
        }

        #[test]
        fn cmd_has_no_singular_form_to_reconcile_multiple_input_occurrences_just_accumulate() {
            // Unlike the environment and the file, the command line has no singular/plural sugar to
            // conflict with: repeating the flag already collects into a list, so there is no
            // ambiguity to detect here.
            let config = parse(&["--input", "a.pdf", "b.pdf"]).to_partial_config().unwrap();
            assert_eq!(config.reports.unwrap().len(), 2);
        }
    }
}
