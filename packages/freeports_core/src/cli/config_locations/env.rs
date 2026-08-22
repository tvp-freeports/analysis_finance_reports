use std::path::PathBuf;

use super::super::conf_parse::{self, ConfigError, DocumentSpec, OutFlags, OutStructureMode};
use super::super::partial_config::PartialConfig;

/// Builds a `FREEPORTS_`-prefixed env var name as a `&'static str` literal, entirely at compile
/// time (via `concat!`) — the prefix lives in this one macro, so renaming it means editing one
/// line instead of hunting down every `"FREEPORTS_..."` literal in this file and its tests.
macro_rules! env_var {
    ($suffix:literal) => {
        concat!("FREEPORTS_", $suffix)
    };
}

/// Every validation failure is a [`ConfigError`] — the same enum `cli::cmd_config`,
/// `cli::file_config`, and `cli::job_config` wrap under their own `InvalidField` variant, so an
/// invalid `VERBOSITY`/`N_WORKERS`/`OUT_PROFILE`/... env var produces exactly the same message as
/// the equivalent `--flag`, YAML key, or batch-file column would.
#[derive(Debug, Clone, PartialEq)]
pub enum EnvConfigError {
    InvalidField { var: &'static str, source: ConfigError },
}

impl std::fmt::Display for EnvConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnvConfigError::InvalidField { var, source } => write!(f, "invalid value for `{var}`: {source}"),
        }
    }
}

impl std::error::Error for EnvConfigError {}

fn get(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn parse_var<T, F>(name: &'static str, parse: F) -> Result<Option<T>, EnvConfigError>
where
    F: FnOnce(&str) -> Result<T, ConfigError>,
{
    match get(name) {
        None => Ok(None),
        Some(raw) => parse(&raw)
            .map(Some)
            .map_err(|source| EnvConfigError::InvalidField { var: name, source }),
    }
}

/// Mirrors `FreeportsEnvConfig.__init__`: reads every `FREEPORTS_*` variable that's set, ignores
/// the ones that aren't (matching `os.environ.get`, which is `None` rather than an error for a
/// missing variable).
pub fn load() -> Result<PartialConfig, EnvConfigError> {
    let verbosity = parse_var(env_var!("VERBOSITY"), conf_parse::parse_verbosity)?;
    let n_workers = parse_var(env_var!("N_WORKERS"), conf_parse::parse_workers)?;
    let batch_file = parse_var(env_var!("BATCH_FILE"), |s| Ok(PathBuf::from(s)))?;
    let out_path = parse_var(env_var!("OUT_PATH"), |s| Ok(PathBuf::from(s)))?;
    let out_profile = parse_var(env_var!("OUT_PROFILE"), |s| s.parse::<OutStructureMode>())?;
    let out_flags = parse_var(env_var!("OUT_FLAGS"), OutFlags::parse)?;
    let save_pdf = parse_var(env_var!("SAVE_PDF"), conf_parse::parse_bool_alias)?;
    let format = parse_var(env_var!("FORMAT"), |s| Ok(s.to_string()))?;
    let input_report = parse_var(env_var!("INPUT_REPORT"), |s| {
        s.parse::<DocumentSpec>().map(|spec| vec![spec]).map_err(ConfigError::from)
    })?;
    let config_file = parse_var(env_var!("CONFIG_FILE"), |s| Ok(PathBuf::from(s)))?;
    let target_lists = parse_var(env_var!("TARGET_LIST"), |s| Ok(vec![s.to_string()]))?;
    let formats_repo_path = parse_var(env_var!("FORMATS_REPO_PATH"), |s| Ok(PathBuf::from(s)))?;
    let input_db_path = parse_var(env_var!("INPUT_DB_PATH"), |s| Ok(PathBuf::from(s)))?;

    Ok(PartialConfig {
        verbosity,
        input_reports: input_report,
        out_profile,
        out_flags,
        out_path,
        n_workers,
        batch_file,
        save_pdf,
        format,
        target_lists,
        formats_repo_path,
        input_db_path,
        config_file,
        prefix_out: None,
    })
}

/// Environment variables are process-global state; every test anywhere in the crate that reads
/// or mutates `FREEPORTS_*` env vars (this module's own tests, plus `cli::cmd`'s, which calls
/// through to [`load`]) locks this first, so `cargo test`'s parallel test threads can't race each
/// other's `std::env::set_var`/`remove_var`/`load()`.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use super::conf_parse::Verbosity;
    use pretty_assertions::assert_eq;

    struct EnvVarGuard {
        name: &'static str,
    }

    impl EnvVarGuard {
        fn set(name: &'static str, value: &str) -> Self {
            unsafe { std::env::set_var(name, value) };
            EnvVarGuard { name }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            unsafe { std::env::remove_var(self.name) };
        }
    }

    #[test]
    fn no_variables_set_yields_an_empty_partial_config() {
        let _lock = ENV_LOCK.lock().unwrap();
        for var in [
            env_var!("VERBOSITY"),
            env_var!("N_WORKERS"),
            env_var!("BATCH_FILE"),
            env_var!("OUT_PATH"),
            env_var!("OUT_PROFILE"),
            env_var!("OUT_FLAGS"),
            env_var!("SAVE_PDF"),
            env_var!("FORMAT"),
            env_var!("INPUT_REPORT"),
            env_var!("CONFIG_FILE"),
            env_var!("TARGET_LIST"),
            env_var!("FORMATS_REPO_PATH"),
            env_var!("INPUT_DB_PATH"),
        ] {
            unsafe { std::env::remove_var(var) };
        }
        assert_eq!(load().unwrap(), PartialConfig::default());
    }

    #[test]
    fn reads_verbosity_and_save_pdf() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _v = EnvVarGuard::set(env_var!("VERBOSITY"), "4");
        let _s = EnvVarGuard::set(env_var!("SAVE_PDF"), "no");
        let config = load().unwrap();
        assert_eq!(config.verbosity, Some(Verbosity::new(4).unwrap()));
        assert_eq!(config.save_pdf, Some(false));
    }

    #[test]
    fn reads_input_report_via_document_spec() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _g = EnvVarGuard::set(env_var!("INPUT_REPORT"), "http://example.com/report.pdf");
        let config = load().unwrap();
        assert_eq!(config.input_reports.unwrap()[0].url.as_ref().unwrap().to_string(), "http://example.com/report.pdf");
    }

    #[test]
    fn reads_target_list_singular_var_into_target_lists_field() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _g = EnvVarGuard::set(env_var!("TARGET_LIST"), "TEST");
        let config = load().unwrap();
        assert_eq!(config.target_lists, Some(vec!["TEST".to_string()]));
    }

    #[test]
    fn invalid_verbosity_is_a_clean_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _g = EnvVarGuard::set(env_var!("VERBOSITY"), "not-a-number");
        assert!(matches!(load(), Err(EnvConfigError::InvalidField { var: env_var!("VERBOSITY"), .. })));
    }

    #[test]
    fn invalid_save_pdf_is_a_clean_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _g = EnvVarGuard::set(env_var!("SAVE_PDF"), "maybe");
        assert!(matches!(load(), Err(EnvConfigError::InvalidField { var: env_var!("SAVE_PDF"), .. })));
    }

    #[test]
    fn invalid_n_workers_carries_the_shared_config_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _g = EnvVarGuard::set(env_var!("N_WORKERS"), "0");
        assert_eq!(
            load(),
            Err(EnvConfigError::InvalidField { var: env_var!("N_WORKERS"), source: ConfigError::InvalidWorkers("0".to_string()) })
        );
    }

    #[test]
    fn invalid_out_profile_carries_the_shared_config_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _g = EnvVarGuard::set(env_var!("OUT_PROFILE"), "NOPE");
        assert_eq!(
            load(),
            Err(EnvConfigError::InvalidField {
                var: env_var!("OUT_PROFILE"),
                source: ConfigError::InvalidOutStructureMode("NOPE".to_string())
            })
        );
    }

    #[test]
    fn out_flags_expression_from_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _g = EnvVarGuard::set(env_var!("OUT_FLAGS"), "COMPRESSED | SEPARATE_OUT_FILES");
        let config = load().unwrap();
        let flags = config.out_flags.unwrap();
        assert!(flags.contains(OutFlags::COMPRESSED));
        assert!(flags.contains(OutFlags::SEPARATE_OUT_FILES));
    }
}
