//! `CliArgs` (clap derive): parsing della riga di comando e conversione in `PartialConfig`.
//!
//! `M9-implementation-plan.md` §2/§3 passo 8, §0 Q5/Q6. Porta `create_parser()`/
//! `FreeportsCmdConfig.__init__` (`conf_parse.py`), con due divergenze deliberate dal riferimento:
//!
//! - **`-v`/`-q` sono manopole indipendenti**, mai un errore se usate insieme (§0 Q5: il
//!   riferimento le tratta come mutuamente esclusive, `argparse.ArgumentTypeError` se entrambe
//!   presenti -- divergenza voluta dall'utente, "independent dials").
//! - **`--separate-out`/`--archive` confluiscono in `OutFlags`** (`output::routines::write`, §0
//!   Q6), non in campi booleani separati come `SEPARATE_OUT_FILES` del riferimento.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, Clone, clap::Parser)]
//! #[command(about = ...)]
//! pub struct CliArgs {
//!     #[arg(long = "input", visible_alias = "report", short = 'i', num_args = 1..)]
//!     pub input: Vec<String>,
//!     #[arg(long = "batch", short = 'b')]
//!     pub batch: Option<String>,
//!     #[arg(long = "workers", short = 'j')]
//!     pub workers: Option<i64>,
//!     #[arg(long = "format", short = 'f')]
//!     pub format: Option<String>,
//!     #[arg(long = "no-download")]
//!     pub no_download: bool,
//!     #[arg(long = "separate-out")]
//!     pub separate_out: bool,
//!     #[arg(long = "config")]
//!     pub config: Option<String>,
//!     #[arg(long = "out", short = 'o')]
//!     pub out: Option<String>,
//!     #[arg(short = 'v', action = clap::ArgAction::Count)]
//!     pub verbose: u8,
//!     #[arg(short = 'q', action = clap::ArgAction::Count)]
//!     pub quiet: u8,
//!     #[arg(long = "target-list", short = 'T', num_args = 1..)]
//!     pub target_list: Vec<String>,
//!     #[arg(long = "archive", short = 'z')]
//!     pub archive: bool,
//!     #[arg(long = "out-profile", short = 'P')]
//!     pub out_profile: Option<String>,
//!     #[arg(long = "db-directory", short = 'I')]
//!     pub db_directory: Option<String>,
//!     #[arg(long = "formats-directory", visible_alias = "repo", short = 'F', visible_short_alias = 'r')]
//!     pub formats_directory: Option<String>,
//! }
//!
//! #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
//! pub enum CmdConfigError {
//!     InvalidReportSpecifier { value: String, source: DocumentSpecError },
//!     InvalidOutProfile { value: String },
//!     InvalidWorkers { value: i64 },  // <= 0
//! }
//!
//! impl CliArgs {
//!     pub fn to_partial_config(&self) -> Result<PartialConfig, CmdConfigError>;
//! }
//! ```
//!
//! # Mappatura flag -> campo
//!
//! | flag | campo | note |
//! |---|---|---|
//! | `--input`/`--report`/`-i` (multiplo) | `reports` | ciascun valore via `DocumentSpec::parse`; nessun valore -> `None`, non `Some(vec![])` |
//! | `--batch`/`-b` | `batch_file` | |
//! | `--workers`/`-j` | `n_workers` | positivo; `<= 0` -> `CmdConfigError::InvalidWorkers` (semplificazione: il riferimento tratta `<= 0` come "auto-rileva i cpu disponibili", non riprodotto qui -- vedi il resoconto del test-writer) |
//! | `--format`/`-f` | `format` | |
//! | `--no-download` | `save_pdf` | presente -> `Some(false)`; assente -> `None` (mai `Some(true)`: il default vero vive in `defaults()`) |
//! | `--config` | `config_file` | |
//! | `--out`/`-o` | `out_path` | |
//! | `-v`/`-q` (conteggio) | `verbosity` | solo se `v > 0 \|\| q > 0`; altrimenti `None` (§0 Q5) |
//! | `--target-list`/`-T` (multiplo) | `target_lists` | nessun valore -> `None`, non `Some(vec![])` |
//! | `--separate-out`, `--archive` | `out_flags` | combinati in un solo `OutFlags`; nessuno dei due -> `None` |
//! | `--out-profile`/`-P` | `out_profile` | stringa fra `regular`/`single_file`/`structured`, case-insensitive |
//! | `--db-directory`/`-I` | `input_db_path` | |
//! | `--formats-directory`/`-F`/`--repo`/`-r` | `formats_repo_path` | |

use std::path::PathBuf;

use crate::cli::conf_parse::{DocumentSpec, DocumentSpecError};
use crate::cli::partial_config::PartialConfig;
use crate::core::tracing_setup::Verbosity;
use crate::output::routines::write::{OutFlags, OutStructureMode};

#[derive(Debug, Clone, clap::Parser)]
#[command(about = "Estrae dati strutturati da report finanziari in formato PDF")]
pub struct CliArgs {
    #[arg(long = "input", visible_alias = "report", short = 'i', num_args = 1..)]
    pub input: Vec<String>,
    #[arg(long = "batch", short = 'b')]
    pub batch: Option<String>,
    // `allow_hyphen_values`: senza, clap tratta un valore negativo (es. `-1`) come un'opzione
    // sconosciuta invece che come il valore di `--workers`, impedendo persino di raggiungere
    // `CmdConfigError::InvalidWorkers` per un `<= 0` (`tests::to_partial_config_simple_fields::
    // zero_or_negative_workers_is_a_typed_error`).
    #[arg(long = "workers", short = 'j', allow_hyphen_values = true)]
    pub workers: Option<i64>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CmdConfigError {
    #[error("invalid document specifier {value:?}: {source}")]
    InvalidReportSpecifier { value: String, source: DocumentSpecError },
    #[error("invalid output profile {value:?}, expected one of: regular, single_file, structured")]
    InvalidOutProfile { value: String },
    #[error("--workers must be a positive number, got {value}")]
    InvalidWorkers { value: i64 },
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
    pub fn to_partial_config(&self) -> Result<PartialConfig, CmdConfigError> {
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

        let n_workers = match self.workers {
            None => None,
            Some(w) if w > 0 => Some(w as usize),
            Some(w) => return Err(CmdConfigError::InvalidWorkers { value: w }),
        };

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
            // §0 Q5: independent dials -- clap-level parsing must not reject the combination
            // (any rejection, if desired at all, would be `to_partial_config`'s business, but the
            // plan says there is none: this combination is never an error).
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
            assert_eq!(config.n_workers, Some(4));
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
            // Unlike env/file, cmd has no `url`/`pdf` single-value sugar to conflict with --
            // repeating `--input` (clap's `num_args = 1..` already collects a single occurrence
            // into a list; this documents there's no separate "singular vs plural" ambiguity to
            // detect here, per §0 Q3).
            let config = parse(&["--input", "a.pdf", "b.pdf"]).to_partial_config().unwrap();
            assert_eq!(config.reports.unwrap().len(), 2);
        }
    }
}
