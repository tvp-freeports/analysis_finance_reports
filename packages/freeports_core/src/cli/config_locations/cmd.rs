use std::path::PathBuf;

use clap::{ArgAction, Parser};

use super::super::conf_parse::{
    validate_workers,
    ConfigError,
    DocumentSpec,
    DocumentSpecError,
    OutFlags,
    OutStructureMode,
    Verbosity
};
use super::super::partial_config::PartialConfig;

const PROGRAM_DESCRIPTION: &str = "Analyze finance reports searching for investing in companies\n\
allegedly involved interantional law violations by third parties\n";

#[derive(Debug, Parser)]
#[command(about = PROGRAM_DESCRIPTION)]
pub struct CliArgs {
    /// PDF file(s), directory, URL(s) specifier
    #[arg(long = "input", visible_alias = "report", short = 'i', num_args = 1.., value_name = "SPEC")]
    input: Option<Vec<String>>,

    /// Activate `BATCH MODE`, path of the batch file
    #[arg(long, short = 'b')]
    batch: Option<String>,

    /// # parallel workers in `BATCH MODE`, if num <= 0, it set to # cpu available
    #[arg(long = "workers", short = 'j', allow_hyphen_values = true)]
    workers: Option<i64>,

    /// PDF format
    #[arg(long, short = 'f')]
    format: Option<String>,

    /// Don't save file locally
    #[arg(long = "no-download")]
    no_download: bool,

    /// Separate output files
    #[arg(long = "separate-out")]
    separate_out: bool,

    /// Custom configuration file location
    #[arg(long)]
    config: Option<String>,

    /// Output file cvs
    #[arg(long, short = 'o')]
    out: Option<String>,

    /// Increase verbosity
    #[arg(short = 'v', action = ArgAction::Count)]
    v: u8,

    /// Decrease verbosity
    #[arg(short = 'q', action = ArgAction::Count)]
    q: u8,

    /// List to filter the companies of interest
    #[arg(long = "target-list", short = 'T', num_args = 1..)]
    target_list: Option<Vec<String>>,

    /// Create a `.tar.gz` archive of the output
    #[arg(long, short = 'z')]
    archive: bool,

    /// Specify the structure of the output dataset
    #[arg(long = "out-profile", short = 'P')]
    out_profile: Option<String>,

    /// Specify the location of the input database
    #[arg(long = "db-directory", short = 'I')]
    db_directory: Option<String>,

    /// Specify the location of the package containing formats
    #[arg(long = "formats-directory", visible_alias = "repo", short = 'F', short_alias = 'r')]
    formats_directory: Option<String>,
}

/// `ConflictingVerbosity` is the one error genuinely specific to the command line (`-v`/`-q`
/// together makes no sense for any other config source). Every other invalid value is a
/// [`ConfigError`] — the same enum `cli::env_config`, `cli::file_config`, and `cli::job_config`
/// wrap under their own `InvalidField` variant, so `--flag value` produces exactly the same
/// message as the equivalent env var, YAML key, or batch-file column would.
#[derive(Debug, Clone, PartialEq)]
pub enum CmdConfigError {
    ConflictingVerbosity,
    InvalidField { flag: &'static str, source: ConfigError },
}

impl std::fmt::Display for CmdConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CmdConfigError::ConflictingVerbosity => write!(f, "Cannot increase and decrease verbosity!"),
            CmdConfigError::InvalidField { flag, source } => write!(f, "--{flag}: {source}"),
        }
    }
}

impl std::error::Error for CmdConfigError {}

/// Mirrors `FreeportsCmdConfig.__init__`. `default_verbosity` is the baseline the `-v`/`-q` count
/// is applied against — `DEFAULT_CONFIG["VERBOSITY"]` in the original.
pub fn from_args(args: CliArgs, default_verbosity: u8) -> Result<PartialConfig, CmdConfigError> {
    if args.v > 0 && args.q > 0 {
        return Err(CmdConfigError::ConflictingVerbosity);
    }
    let verbosity = if args.v > 0 {
        let raised = default_verbosity as i64 + args.v as i64;
        Some(Verbosity::new(raised.clamp(0, Verbosity::MAX as i64)).expect("clamped into range"))
    } else if args.q > 0 {
        let lowered = default_verbosity as i64 - args.q as i64;
        Some(Verbosity::new(lowered.clamp(0, Verbosity::MAX as i64)).expect("clamped into range"))
    } else {
        None
    };

    let n_workers = match args.workers {
        None => None,
        Some(n) => Some(
            validate_workers(n).map_err(|source| CmdConfigError::InvalidField { flag: "workers", source })?,
        ),
    };

    let mut out_flags_value = OutFlags::NONE;
    let mut out_flags_set = false;
    if args.separate_out {
        out_flags_value = out_flags_value | OutFlags::SEPARATE_OUT_FILES;
        out_flags_set = true;
    }
    if args.archive {
        out_flags_value = out_flags_value | OutFlags::COMPRESSED;
        out_flags_set = true;
    }
    let out_flags = out_flags_set.then_some(out_flags_value);

    let input_reports = match args.input {
        None => None,
        Some(specs) => Some(
            specs
                .into_iter()
                .map(|s| s.parse::<DocumentSpec>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|source: DocumentSpecError| CmdConfigError::InvalidField { flag: "input", source: source.into() })?,
        ),
    };

    let out_profile = match args.out_profile {
        None => None,
        Some(s) => Some(
            s.parse::<OutStructureMode>()
                .map_err(|source| CmdConfigError::InvalidField { flag: "out-profile", source })?,
        ),
    };

    Ok(PartialConfig {
        verbosity,
        input_reports,
        out_profile,
        out_flags,
        out_path: args.out.map(PathBuf::from),
        n_workers,
        batch_file: args.batch.map(PathBuf::from),
        save_pdf: args.no_download.then_some(false),
        format: args.format,
        target_lists: args.target_list,
        formats_repo_path: args.formats_directory.map(PathBuf::from),
        input_db_path: args.db_directory.map(PathBuf::from),
        config_file: args.config.map(PathBuf::from),
        prefix_out: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    fn parse(argv: &[&str]) -> CliArgs {
        let mut full = vec!["freeports"];
        full.extend_from_slice(argv);
        CliArgs::parse_from(full)
    }

    #[test]
    fn no_arguments_yields_an_empty_partial_config() {
        let config = from_args(parse(&[]), 2).unwrap();
        assert_eq!(config, PartialConfig::default());
    }

    #[test]
    fn config_flag_sets_config_file_fixing_the_original_no_op_bug() {
        let config = from_args(parse(&["--config", "/tmp/custom.yaml"]), 2).unwrap();
        assert_eq!(config.config_file, Some(PathBuf::from("/tmp/custom.yaml")));
    }

    /// Regression pin: the Python original raised `VERBOSITY` on `-q` instead of lowering it.
    #[test_case(&["-q"], 2, 1; "single q lowers by one")]
    #[test_case(&["-q", "-q"], 2, 0; "double q clamps at zero not negative")]
    #[test_case(&["-v"], 2, 3; "single v raises by one")]
    #[test_case(&["-v", "-v", "-v", "-v"], 2, 5; "many v clamps at five")]
    fn verbosity_flags_move_in_the_documented_direction(argv: &[&str], default: u8, expected: u8) {
        let config = from_args(parse(argv), default).unwrap();
        assert_eq!(config.verbosity, Some(Verbosity::new(expected as i64).unwrap()));
    }

    #[test]
    fn v_and_q_together_is_a_conflict_error() {
        assert_eq!(from_args(parse(&["-v", "-q"]), 2), Err(CmdConfigError::ConflictingVerbosity));
    }

    #[test]
    fn no_download_sets_save_pdf_false() {
        let config = from_args(parse(&["--no-download"]), 2).unwrap();
        assert_eq!(config.save_pdf, Some(false));
    }

    #[test]
    fn absent_no_download_leaves_save_pdf_unset() {
        let config = from_args(parse(&[]), 2).unwrap();
        assert_eq!(config.save_pdf, None);
    }

    #[test]
    fn separate_out_and_archive_combine_into_out_flags() {
        let config = from_args(parse(&["--separate-out", "--archive"]), 2).unwrap();
        let flags = config.out_flags.unwrap();
        assert!(flags.contains(OutFlags::SEPARATE_OUT_FILES));
        assert!(flags.contains(OutFlags::COMPRESSED));
    }

    #[test]
    fn archive_alone_sets_only_compressed() {
        let config = from_args(parse(&["--archive"]), 2).unwrap();
        let flags = config.out_flags.unwrap();
        assert!(flags.contains(OutFlags::COMPRESSED));
        assert!(!flags.contains(OutFlags::SEPARATE_OUT_FILES));
    }

    #[test]
    fn neither_out_flag_leaves_out_flags_unset() {
        let config = from_args(parse(&[]), 2).unwrap();
        assert_eq!(config.out_flags, None);
    }

    #[test]
    fn input_accepts_multiple_specs_and_parses_each_as_a_document_spec() {
        let config = from_args(parse(&["--input", "a.pdf", "http://example.com/b.pdf"]), 2).unwrap();
        let specs = config.input_reports.unwrap();
        assert_eq!(specs.len(), 2);
        assert!(specs[0].path.is_some());
        assert!(specs[1].url.is_some());
    }

    #[test]
    fn report_and_i_are_aliases_for_input() {
        let a = from_args(parse(&["--report", "a.pdf"]), 2).unwrap();
        let b = from_args(parse(&["-i", "a.pdf"]), 2).unwrap();
        assert_eq!(a.input_reports.unwrap().len(), 1);
        assert_eq!(b.input_reports.unwrap().len(), 1);
    }

    #[test]
    fn invalid_input_spec_is_a_clean_error() {
        assert!(matches!(from_args(parse(&["--input", "a|b|c"]), 2), Err(CmdConfigError::InvalidField { flag: "input", .. })));
    }

    #[test]
    fn formats_directory_repo_and_short_aliases_all_work() {
        for flag in ["--formats-directory", "--repo", "-F", "-r"] {
            let config = from_args(parse(&[flag, "/tmp/formats"]), 2).unwrap();
            assert_eq!(config.formats_repo_path, Some(PathBuf::from("/tmp/formats")));
        }
    }

    #[test]
    fn out_profile_parses_via_out_structure_mode() {
        let config = from_args(parse(&["--out-profile", "single_file"]), 2).unwrap();
        assert_eq!(config.out_profile, Some(OutStructureMode::SingleFile));
    }

    #[test]
    fn invalid_out_profile_is_a_clean_error() {
        assert!(matches!(from_args(parse(&["--out-profile", "NOPE"]), 2), Err(CmdConfigError::InvalidField { flag: "out-profile", .. })));
    }

    #[test]
    fn workers_zero_or_negative_is_rejected() {
        assert_eq!(
            from_args(parse(&["--workers", "0"]), 2),
            Err(CmdConfigError::InvalidField { flag: "workers", source: ConfigError::InvalidWorkers("0".to_string()) })
        );
        assert_eq!(
            from_args(parse(&["--workers", "-1"]), 2),
            Err(CmdConfigError::InvalidField { flag: "workers", source: ConfigError::InvalidWorkers("-1".to_string()) })
        );
    }

    #[test]
    fn target_list_accepts_multiple_values() {
        let config = from_args(parse(&["--target-list", "TEST", "OTHER"]), 2).unwrap();
        assert_eq!(config.target_lists, Some(vec!["TEST".to_string(), "OTHER".to_string()]));
    }

    #[test]
    fn out_batch_and_db_directory_map_to_the_expected_fields() {
        let config = from_args(parse(&["--out", "/tmp/out", "--batch", "/tmp/jobs.csv", "--db-directory", "/tmp/db"]), 2).unwrap();
        assert_eq!(config.out_path, Some(PathBuf::from("/tmp/out")));
        assert_eq!(config.batch_file, Some(PathBuf::from("/tmp/jobs.csv")));
        assert_eq!(config.input_db_path, Some(PathBuf::from("/tmp/db")));
    }
}
