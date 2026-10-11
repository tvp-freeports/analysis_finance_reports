//! `freeports classify`: the precedence `cmd > env > file > default` of the settings it reads, via
//! `cli::classify::resolve_config` (phase 10, `agent-memory/DESIGN-phase10-classify.md` §4.1, §7
//! group B). Same style as `cli_config.rs`: one field at a time, with the other sources leaving it
//! alone.
//!
//! No new setting: `classify` reads `reports`, `formats_repo_path`, `save_pdf`, `verbosity` and the
//! configuration file from the three tiers exactly as the run does, and **ignores** every other one
//! — format, batch file, target lists, output, parallelism — even when the environment or the file
//! sets them: they are ambient, not an error.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use clap::Parser;
use freeports::cli::classify::{ClassifyConfig, ClassifyError, resolve_config};
use freeports::cli::config_locations::cmd::{ClassifyArgs, CliArgs, Command};
use freeports::cli::freeports_config::FreeportsConfigError;
use freeports::cli::run::CliError;
use freeports::core::tracing_setup::Verbosity;

const ALL_FREEPORTS_VARS: &[&str] = &[
    "FREEPORTS_URL",
    "FREEPORTS_PDF",
    "FREEPORTS_REPORTS",
    "FREEPORTS_VERBOSITY",
    "FREEPORTS_N_WORKERS",
    "FREEPORTS_PARALLELISM_JOBS",
    "FREEPORTS_PARALLELISM_PAGES",
    "FREEPORTS_BATCH_FILE",
    "FREEPORTS_OUT_PATH",
    "FREEPORTS_OUT_PROFILE",
    "FREEPORTS_SEPARATE_OUT",
    "FREEPORTS_ARCHIVE",
    "FREEPORTS_SAVE_PDF",
    "FREEPORTS_FORMAT",
    "FREEPORTS_CONFIG_FILE",
    "FREEPORTS_TARGET_LIST",
    "FREEPORTS_FORMATS_REPO_PATH",
    "FREEPORTS_INPUT_DB_PATH",
];

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Clears and restores the `FREEPORTS_*` variables for the duration of a test, and serializes the
/// tests of this file, which share the process environment.
struct EnvScope {
    _lock: std::sync::MutexGuard<'static, ()>,
    originals: Vec<(&'static str, Option<String>)>,
}

impl EnvScope {
    fn new() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let originals = ALL_FREEPORTS_VARS
            .iter()
            .map(|&k| (k, std::env::var(k).ok()))
            .collect();
        for &k in ALL_FREEPORTS_VARS {
            unsafe { std::env::remove_var(k) };
        }
        Self {
            _lock: lock,
            originals,
        }
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

/// The arguments of `freeports classify <args>`.
fn classify_args(args: &[&str]) -> ClassifyArgs {
    let mut full = vec!["freeports", "classify"];
    full.extend_from_slice(args);
    match CliArgs::try_parse_from(full)
        .expect("argv must parse")
        .command
    {
        Some(Command::Classify(classify)) => classify,
        None => panic!("`classify {args:?}` must parse as the classify subcommand"),
    }
}

/// A temporary directory with three PDFs (bytes only: nothing is opened here), two formats
/// repositories, and an empty configuration file passed explicitly — so that resolution never
/// falls back on the real search tiers, which `EnvScope` does not isolate.
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Fixture {
            dir: tempfile::tempdir().unwrap(),
        };
        for name in ["from_cmd.pdf", "from_env.pdf", "from_file.pdf"] {
            fixture.pdf(name);
        }
        for repo in ["repo_cmd", "repo_env", "repo_file"] {
            let metadata = fixture.path().join(repo).join("metadata");
            std::fs::create_dir_all(&metadata).unwrap();
            std::fs::write(
                metadata.join("formats.csv"),
                "Name,Locale,Year,Country,Version\nA,EN,24,,\n",
            )
            .unwrap();
            std::fs::write(metadata.join("url_mapping.csv"), "Format name,Url\n").unwrap();
        }
        fixture
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn pdf(&self, name: &str) -> PathBuf {
        let path = self.path().join(name);
        std::fs::write(&path, b"%PDF-1.4 fake").unwrap();
        path
    }

    fn at(&self, name: &str) -> String {
        self.path().join(name).to_str().unwrap().to_string()
    }

    fn config(&self, content: &str) -> String {
        let path = self.path().join("cfg.yaml");
        std::fs::write(&path, content).unwrap();
        path.to_str().unwrap().to_string()
    }

    /// `classify` with `extra`, the configuration file `config_content`, and — unless the test is
    /// about them — a document and a repository from the command line.
    fn resolve(&self, extra: &[&str], config_content: &str) -> Result<ClassifyConfig, CliError> {
        let config = self.config(config_content);
        let mut args: Vec<&str> = vec!["--config", &config];
        args.extend_from_slice(extra);
        resolve_config(classify_args(&args))
    }

    /// The base the tests not about `reports` or `formats_repo_path` add to: one document and one
    /// repository on the command line.
    fn base(&self) -> Vec<String> {
        vec![
            "-i".to_string(),
            self.at("from_cmd.pdf"),
            "-F".to_string(),
            self.at("repo_cmd"),
        ]
    }

    fn resolve_with_base(&self, extra: &[&str], config_content: &str) -> ClassifyConfig {
        let base = self.base();
        let mut args: Vec<&str> = base.iter().map(String::as_str).collect();
        args.extend_from_slice(extra);
        self.resolve(&args, config_content)
            .expect("this resolution must succeed")
    }
}

fn paths(config: &ClassifyConfig) -> Vec<PathBuf> {
    config
        .reports
        .iter()
        .map(|spec| spec.path.clone().expect("a local document has a path"))
        .collect()
}

mod reports_precedence {
    use super::*;

    #[test]
    fn file_alone_sets_the_reports() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config = fixture
            .resolve(
                &["-F", &fixture.at("repo_cmd")],
                &format!("pdf: {}\n", fixture.at("from_file.pdf")),
            )
            .unwrap();
        assert_eq!(paths(&config), vec![fixture.path().join("from_file.pdf")]);
    }

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_PDF", &fixture.at("from_env.pdf"));
        let config = fixture
            .resolve(
                &["-F", &fixture.at("repo_cmd")],
                &format!("pdf: {}\n", fixture.at("from_file.pdf")),
            )
            .unwrap();
        assert_eq!(paths(&config), vec![fixture.path().join("from_env.pdf")]);
    }

    #[test]
    fn the_plural_environment_variable_is_read_too() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set(
            "FREEPORTS_REPORTS",
            &format!(
                "{}|{}",
                fixture.at("from_env.pdf"),
                fixture.at("from_file.pdf")
            ),
        );
        let config = fixture
            .resolve(&["-F", &fixture.at("repo_cmd")], "")
            .unwrap();
        assert_eq!(
            paths(&config),
            vec![
                fixture.path().join("from_env.pdf"),
                fixture.path().join("from_file.pdf")
            ]
        );
    }

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_PDF", &fixture.at("from_env.pdf"));
        let config = fixture
            .resolve(
                &[
                    "-i",
                    &fixture.at("from_cmd.pdf"),
                    "-F",
                    &fixture.at("repo_cmd"),
                ],
                &format!("pdf: {}\n", fixture.at("from_file.pdf")),
            )
            .unwrap();
        assert_eq!(paths(&config), vec![fixture.path().join("from_cmd.pdf")]);
    }

    #[test]
    fn several_inputs_keep_their_order_and_names() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let second = format!("{}:Second", fixture.at("from_env.pdf"));
        let config = fixture
            .resolve(
                &[
                    "-i",
                    &fixture.at("from_cmd.pdf"),
                    &second,
                    "-F",
                    &fixture.at("repo_cmd"),
                ],
                "",
            )
            .unwrap();
        assert_eq!(
            paths(&config),
            vec![
                fixture.path().join("from_cmd.pdf"),
                fixture.path().join("from_env.pdf")
            ]
        );
        assert_eq!(config.reports[1].name.as_deref(), Some("Second"));
    }

    /// Default: no document, an error of its own (and not a silent success printing nothing).
    #[test]
    fn no_document_from_any_source_is_no_documents() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let result = fixture.resolve(&["-F", &fixture.at("repo_cmd")], "");
        assert!(
            matches!(result, Err(CliError::Classify(ClassifyError::NoDocuments))),
            "{result:?}"
        );
    }
}

mod formats_repo_path_precedence {
    use super::*;

    fn input(fixture: &Fixture) -> Vec<String> {
        vec!["-i".to_string(), fixture.at("from_cmd.pdf")]
    }

    #[test]
    fn file_alone_sets_it() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let input = input(&fixture);
        let args: Vec<&str> = input.iter().map(String::as_str).collect();
        let config = fixture
            .resolve(
                &args,
                &format!("formats_repo: {}\n", fixture.at("repo_file")),
            )
            .unwrap();
        assert_eq!(config.formats_repo_path, fixture.path().join("repo_file"));
    }

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_FORMATS_REPO_PATH", &fixture.at("repo_env"));
        let input = input(&fixture);
        let args: Vec<&str> = input.iter().map(String::as_str).collect();
        let config = fixture
            .resolve(
                &args,
                &format!("formats_repo: {}\n", fixture.at("repo_file")),
            )
            .unwrap();
        assert_eq!(config.formats_repo_path, fixture.path().join("repo_env"));
    }

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_FORMATS_REPO_PATH", &fixture.at("repo_env"));
        let config = fixture
            .resolve(
                &[
                    "-i",
                    &fixture.at("from_cmd.pdf"),
                    "--repo",
                    &fixture.at("repo_cmd"),
                ],
                &format!("formats_repo: {}\n", fixture.at("repo_file")),
            )
            .unwrap();
        assert_eq!(config.formats_repo_path, fixture.path().join("repo_cmd"));
    }

    /// Default: none. Nothing to classify with.
    #[test]
    fn no_repository_from_any_source_is_missing_formats_repo_path() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let result = fixture.resolve(&["-i", &fixture.at("from_cmd.pdf")], "");
        assert!(
            matches!(
                result,
                Err(CliError::Classify(ClassifyError::MissingFormatsRepoPath))
            ),
            "{result:?}"
        );
    }
}

mod save_pdf_precedence {
    use super::*;

    #[test]
    fn default_is_true() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        assert!(fixture.resolve_with_base(&[], "").save_pdf);
    }

    #[test]
    fn file_overrides_default() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        assert!(!fixture.resolve_with_base(&[], "save_pdf: false\n").save_pdf);
    }

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        scope.set("FREEPORTS_SAVE_PDF", "true");
        let fixture = Fixture::new();
        assert!(fixture.resolve_with_base(&[], "save_pdf: false\n").save_pdf);
    }

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        scope.set("FREEPORTS_SAVE_PDF", "true");
        let fixture = Fixture::new();
        assert!(
            !fixture
                .resolve_with_base(&["--no-download"], "save_pdf: true\n")
                .save_pdf
        );
    }
}

mod verbosity_precedence {
    use super::*;

    #[test]
    fn default_is_warn() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        assert_eq!(
            fixture.resolve_with_base(&[], "").verbosity,
            Verbosity::Warn
        );
    }

    #[test]
    fn file_overrides_default() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        assert_eq!(
            fixture
                .resolve_with_base(&[], "verbosity: debug\n")
                .verbosity,
            Verbosity::Debug
        );
    }

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        scope.set("FREEPORTS_VERBOSITY", "trace");
        let fixture = Fixture::new();
        assert_eq!(
            fixture
                .resolve_with_base(&[], "verbosity: debug\n")
                .verbosity,
            Verbosity::Trace
        );
    }

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        scope.set("FREEPORTS_VERBOSITY", "trace");
        let fixture = Fixture::new();
        assert_eq!(
            fixture
                .resolve_with_base(&["-q"], "verbosity: debug\n")
                .verbosity,
            Verbosity::from_verbose_and_quiet_counts(0, 1)
        );
    }
}

/// `--config` chooses the file, over `FREEPORTS_CONFIG_FILE`; without it the environment does.
mod config_file_choice {
    use super::*;

    fn two_files(fixture: &Fixture) -> (String, String) {
        let chosen = fixture.path().join("chosen.yaml");
        std::fs::write(&chosen, "verbosity: debug\n").unwrap();
        let other = fixture.path().join("other.yaml");
        std::fs::write(&other, "verbosity: info\n").unwrap();
        (
            chosen.to_str().unwrap().to_string(),
            other.to_str().unwrap().to_string(),
        )
    }

    #[test]
    fn the_command_line_config_is_the_file_read() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        let (chosen, other) = two_files(&fixture);
        scope.set("FREEPORTS_CONFIG_FILE", &other);
        let base = fixture.base();
        let mut args: Vec<&str> = base.iter().map(String::as_str).collect();
        args.extend_from_slice(&["--config", &chosen]);
        let config = resolve_config(classify_args(&args)).unwrap();
        assert_eq!(config.verbosity, Verbosity::Debug);
    }

    #[test]
    fn without_it_the_environment_chooses_the_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        let (chosen, _) = two_files(&fixture);
        scope.set("FREEPORTS_CONFIG_FILE", &chosen);
        let base = fixture.base();
        let args: Vec<&str> = base.iter().map(String::as_str).collect();
        let config = resolve_config(classify_args(&args)).unwrap();
        assert_eq!(config.verbosity, Verbosity::Debug);
    }
}

/// What the run needs and `classify` does not: set by environment or file, ignored, never an
/// error.
mod settings_of_the_run_are_ignored {
    use super::*;

    #[test]
    fn format_batch_file_and_target_lists_in_the_environment_are_ignored() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_FORMAT", "NOT-A-FORMAT-OF-THE-REPO");
        scope.set(
            "FREEPORTS_BATCH_FILE",
            &fixture.at("no_such_batch_file.csv"),
        );
        scope.set("FREEPORTS_TARGET_LIST", "TEST");
        scope.set("FREEPORTS_N_WORKERS", "3");
        let config = fixture.resolve_with_base(&[], "");
        assert_eq!(paths(&config), vec![fixture.path().join("from_cmd.pdf")]);
    }

    /// An output path whose parent does not exist would stop the run; `classify` writes no
    /// output, so it does not even look.
    #[test]
    fn format_batch_file_target_lists_and_output_in_the_file_are_ignored() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config = fixture.resolve_with_base(
            &[],
            &format!(
                "format: NOT-A-FORMAT-OF-THE-REPO\n\
                 batch_file: {}\n\
                 target_lists:\n  - TEST\n\
                 out_path: {}\n\
                 n_workers: 2\n",
                fixture.at("no_such_batch_file.csv"),
                fixture.at("no/such/dir/out"),
            ),
        );
        assert_eq!(paths(&config), vec![fixture.path().join("from_cmd.pdf")]);
    }
}

/// The documents go through the run's own validation (`validate_document_specs`).
mod document_validation {
    use super::*;

    #[test]
    fn a_path_that_does_not_exist_is_the_runs_error() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let result = fixture.resolve(
            &[
                "-i",
                &fixture.at("missing.pdf"),
                "-F",
                &fixture.at("repo_cmd"),
            ],
            "",
        );
        assert!(
            matches!(
                result,
                Err(CliError::Validate(
                    FreeportsConfigError::DocumentPathDoesNotExist { .. }
                ))
            ),
            "{result:?}"
        );
    }

    #[test]
    fn a_directory_is_expanded_into_its_pdfs_in_order() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let folder = fixture.path().join("folder");
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["b.pdf", "a.pdf", "notes.txt"] {
            std::fs::write(folder.join(name), b"%PDF-1.4 fake").unwrap();
        }
        let config = fixture
            .resolve(
                &[
                    "-i",
                    folder.to_str().unwrap(),
                    "-F",
                    &fixture.at("repo_cmd"),
                ],
                "",
            )
            .unwrap();
        assert_eq!(
            paths(&config),
            vec![folder.join("a.pdf"), folder.join("b.pdf")]
        );
    }

    #[test]
    fn an_invalid_specifier_is_the_command_lines_error() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let result = fixture.resolve(&["-i", "a:b:c:d", "-F", &fixture.at("repo_cmd")], "");
        assert!(matches!(result, Err(CliError::Cmd(_))), "{result:?}");
    }

    /// Decision of 2026-10-11: a document given only by URL is saved as in the run — into
    /// `./report.pdf` when `save_pdf` is on.
    #[test]
    fn a_url_only_document_is_given_the_runs_download_path_when_saving() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config = fixture
            .resolve(
                &[
                    "-i",
                    "https://example.com/report.pdf",
                    "-F",
                    &fixture.at("repo_cmd"),
                ],
                "",
            )
            .unwrap();
        assert_eq!(config.reports.len(), 1);
        assert_eq!(
            config.reports[0].url.as_deref(),
            Some("https://example.com/report.pdf")
        );
        assert_eq!(
            config.reports[0].path,
            Some(std::env::current_dir().unwrap().join("report.pdf"))
        );
    }

    #[test]
    fn a_url_only_document_has_no_path_with_no_download() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config = fixture
            .resolve(
                &[
                    "-i",
                    "https://example.com/report.pdf",
                    "-F",
                    &fixture.at("repo_cmd"),
                    "--no-download",
                ],
                "",
            )
            .unwrap();
        assert_eq!(config.reports[0].path, None);
        assert!(!config.save_pdf);
    }
}
