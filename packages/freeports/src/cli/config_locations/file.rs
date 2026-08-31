//! Configuration from a YAML file, and finding that file in the standard locations.
//!
//! # Where the file is looked for
//!
//! Three tiers, in decreasing precedence:
//!
//! 1. **the working directory** — a file whose name matches, case-insensitively, either `config-freeports.yaml` or `freeports-config.yaml`, in their several punctuation and extension variants;
//! 2. **the user tier** — the OS's local configuration directory, holding `freeports.yaml` or `freeports.yml`. Local rather than roaming, deliberately: a configuration naming machine-local paths should not follow a user to another machine;
//! 3. **the system tier** — on POSIX, the XDG configuration directories then `/etc`; on Windows, the program-data directory then the system root.
//!
//! Both platform branches are **always compiled**, not behind a target guard, so the tests exercise
//! both whatever system runs them.
//!
//! No file in any tier yields nothing, never an error.
//!
//! # Recognised keys
//!
//! | key | field | note |
//! |---|---|---|
//! | `verbosity` | verbosity | one of the variant names, case-insensitively |
//! | `out_path` | output path | |
//! | `n_workers` | the global parallelism default | a positive integer or `auto` |
//! | `parallelism` | the two per-level overrides | a map with only the `jobs` and `pages` sub-keys |
//! | `batch_file` | batch file | |
//! | `save_pdf` | save PDF | a native YAML boolean |
//! | `url`, `pdf` | contribute to the singular document spec | |
//! | `reports` | the reports | a list, each in the document-spec grammar |
//! | `format` | format | |
//! | `target_lists` | target lists | a list |
//! | `formats_repo` | formats repository path | |
//! | `db_path` | input database path | |
//!
//! An unknown key is an **explicit error**. A misspelled key that is silently ignored configures
//! nothing and says nothing, which is exactly the failure this refuses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::de::Error as _;

use crate::cli::conf_parse::{DocumentSpec, DocumentSpecError};
use crate::cli::parallelism_config::Workers;
use crate::cli::partial_config::{PartialConfig, SourceReportsConflict, resolve_singular_and_plural_reports};
use crate::core::tracing_setup::Verbosity;
use crate::core::tracing_setup::log_error;

#[derive(Debug, thiserror::Error)]
pub enum FileConfigError {
    #[error("cannot read {}: {source}", path.display())]
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error("cannot parse YAML {}: {source}", path.display())]
    Yaml { path: PathBuf, #[source] source: serde_yaml::Error },
    #[error("{}: unknown configuration key {key:?}", path.display())]
    UnknownKey { path: PathBuf, key: String },
    #[error("{}: invalid document specifier {value:?}: {source}", path.display())]
    InvalidReportSpecifier { path: PathBuf, value: String, source: DocumentSpecError },
    #[error("{}: {source}", path.display())]
    ReportsConflict { path: PathBuf, source: SourceReportsConflict },
    #[error("{}: invalid verbosity {value:?}, expected one of: silent, erroronly, warn, info, debug, trace", path.display())]
    InvalidVerbosity { path: PathBuf, value: String },
    #[error("{}: invalid value for '{key}': {value:?}", path.display())]
    InvalidValue { path: PathBuf, key: &'static str, value: String },
}

const CONFIG_FILE_NAMES: [&str; 2] = ["freeports.yaml", "freeports.yml"];

/// The working-directory tier: a file whose name matches either pattern, case-insensitively.
///
/// Takes an explicit directory rather than reading the process's own, so it stays testable without
/// mutating a working directory shared by every test running in parallel.
pub(crate) fn local_config_in(dir: &Path) -> Option<PathBuf> {
    let patterns = [
        onig::Regex::new(r"(?i)^\.?(config|conf)[-._]?freeports\.ya?ml$")
            .expect("fixed pattern, valid by construction -- verified at compile time"),
        onig::Regex::new(r"(?i)^\.?freeports[-._]?(config|conf)\.ya?ml$")
            .expect("fixed pattern, valid by construction -- verified at compile time"),
    ];
    let entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| match e {
                Ok(entry) => Some(entry),
                Err(e) => {
                    tracing::warn!(error = log_error(&e), dir = %dir.display(), "cannot read a directory entry while searching for a configuration file, skipping it: {e}");
                    None
                }
            })
            .map(|e| e.path())
            .collect(),
        Err(e) => {
            tracing::warn!(error = log_error(&e), dir = %dir.display(), "cannot list directory while searching for a configuration file: {e}");
            return None;
        }
    };
    for pattern in &patterns {
        for path in &entries {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if pattern.is_match(name) && path.is_file() {
                return Some(path.clone());
            }
        }
    }
    None
}

/// The user tier, taking the already-resolved configuration directory for the same testability
/// reason.
pub(crate) fn user_config_in(config_local_dir: Option<&Path>) -> Option<PathBuf> {
    let dir = config_local_dir?;
    for name in CONFIG_FILE_NAMES {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The system tier: the XDG configuration directories then `/etc` on POSIX, the program-data
/// directory then the system root on Windows. Both branches are always compiled; see the module
/// documentation.
pub(crate) fn system_config() -> Option<PathBuf> {
    let xdg_dirs: Vec<PathBuf> = match std::env::var("XDG_CONFIG_DIRS") {
        Ok(value) if !value.is_empty() => value.split(':').map(PathBuf::from).collect(),
        _ => vec![PathBuf::from("/etc/xdg")],
    };
    for dir in xdg_dirs.iter().chain(std::iter::once(&PathBuf::from("/etc"))) {
        for name in CONFIG_FILE_NAMES {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    if let Ok(program_data) = std::env::var("PROGRAMDATA") {
        for name in CONFIG_FILE_NAMES {
            let candidate = PathBuf::from(&program_data).join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let Ok(system_root) = std::env::var("SystemRoot") {
        for name in CONFIG_FILE_NAMES {
            let candidate = PathBuf::from(&system_root).join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

pub fn find_config() -> Option<PathBuf> {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            tracing::warn!(error = log_error(&e), "cannot read the current directory, skipping the cwd configuration-file tier: {e}");
            return None;
        }
    };
    let found = local_config_in(&cwd).or_else(|| user_config_in(dirs::config_local_dir().as_deref())).or_else(system_config);
    match &found {
        Some(path) => tracing::debug!(path = %path.display(), "found a configuration file"),
        None => tracing::debug!("no configuration file found in the cwd/user/system tiers"),
    }
    found
}

fn parse_verbosity(path: &Path, value: &str) -> Result<Verbosity, FileConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "silent" => Ok(Verbosity::Silent),
        "error" => Ok(Verbosity::ErrorOnly),
        "warn" => Ok(Verbosity::Warn),
        "info" => Ok(Verbosity::Info),
        "debug" => Ok(Verbosity::Debug),
        "trace" => Ok(Verbosity::Trace),
        _ => Err(FileConfigError::InvalidVerbosity { path: path.to_path_buf(), value: value.to_string() }),
    }
}

fn value_as_string(path: &Path, key: &'static str, value: &serde_yaml::Value) -> Result<String, FileConfigError> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| FileConfigError::InvalidValue { path: path.to_path_buf(), key, value: format!("{value:?}") })
}

/// One parallelism level, written as a positive integer or as the word `auto`.
///
/// YAML tells the two apart on its own, so there is no need to go through the text when the value
/// is already an integer — and a quoted number is still accepted, refusing it being a subtlety with
/// no gain.
fn value_as_workers(path: &Path, key: &'static str, value: &serde_yaml::Value) -> Result<Workers, FileConfigError> {
    let invalid =
        || FileConfigError::InvalidValue { path: path.to_path_buf(), key, value: format!("{value:?}") };
    match value {
        serde_yaml::Value::String(text) => Workers::parse(text).map_err(|_| invalid()),
        _ => match value.as_u64() {
            Some(n) if n > 0 => Ok(Workers::Fixed(n as usize)),
            _ => Err(invalid()),
        },
    }
}

/// The `parallelism` section, with its two sub-keys.
///
/// An unknown sub-key is an error, as at the top level: a level that was considered and never
/// implemented would, if silently accepted, look active.
fn parallelism_section(
    path: &Path,
    value: &serde_yaml::Value,
) -> Result<(Option<Workers>, Option<Workers>), FileConfigError> {
    let mapping = value.as_mapping().ok_or_else(|| FileConfigError::InvalidValue {
        path: path.to_path_buf(),
        key: "parallelism",
        value: format!("{value:?}"),
    })?;
    let mut jobs = None;
    let mut pages = None;
    for (key, entry) in mapping {
        match key.as_str().unwrap_or_default() {
            "jobs" => jobs = Some(value_as_workers(path, "parallelism.jobs", entry)?),
            "pages" => pages = Some(value_as_workers(path, "parallelism.pages", entry)?),
            other => {
                return Err(FileConfigError::UnknownKey {
                    path: path.to_path_buf(),
                    key: format!("parallelism.{other}"),
                });
            }
        }
    }
    Ok((jobs, pages))
}

fn value_as_bool(path: &Path, key: &'static str, value: &serde_yaml::Value) -> Result<bool, FileConfigError> {
    value.as_bool().ok_or_else(|| FileConfigError::InvalidValue { path: path.to_path_buf(), key, value: format!("{value:?}") })
}

fn value_as_string_list(path: &Path, key: &'static str, value: &serde_yaml::Value) -> Result<Vec<String>, FileConfigError> {
    let items = value
        .as_sequence()
        .ok_or_else(|| FileConfigError::InvalidValue { path: path.to_path_buf(), key, value: format!("{value:?}") })?;
    items.iter().map(|item| value_as_string(path, key, item)).collect()
}

/// Wraps `load_impl` to log any failure exactly once -- this is the only place every
/// `FileConfigError` variant is actually constructed (directly or via the small `value_as_*`/
/// `parse_verbosity` helpers below).
pub fn load(path: Option<&Path>) -> Result<PartialConfig, FileConfigError> {
    let result = load_impl(path);
    if let Err(e) = &result {
        tracing::error!(error = log_error(e), "{e}");
    }
    result
}

/// No path yields the empty configuration, with no error; a path is read and validated, an unknown
/// key being an error.
fn load_impl(path: Option<&Path>) -> Result<PartialConfig, FileConfigError> {
    let Some(path) = path else {
        return Ok(PartialConfig::default());
    };

    let content = std::fs::read_to_string(path).map_err(|source| FileConfigError::Io { path: path.to_path_buf(), source })?;
    let value: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|source| FileConfigError::Yaml { path: path.to_path_buf(), source })?;

    let mapping = match value {
        serde_yaml::Value::Null => return Ok(PartialConfig::default()),
        serde_yaml::Value::Mapping(m) => m,
        _ => {
            return Err(FileConfigError::Yaml {
                path: path.to_path_buf(),
                source: serde_yaml::Error::custom("top-level YAML document must be a mapping"),
            });
        }
    };

    const KNOWN_KEYS: [&str; 13] = [
        "verbosity", "out_path", "n_workers", "parallelism", "batch_file", "save_pdf", "url", "pdf", "reports",
        "format", "target_lists", "formats_repo", "db_path",
    ];

    let mut fields: HashMap<&'static str, serde_yaml::Value> = HashMap::new();
    for (k, v) in mapping {
        let key_str = k.as_str().unwrap_or_default().to_string();
        match KNOWN_KEYS.iter().find(|&&known| known == key_str) {
            Some(&known) => {
                fields.insert(known, v);
            }
            None => return Err(FileConfigError::UnknownKey { path: path.to_path_buf(), key: key_str }),
        }
    }

    let verbosity = fields.get("verbosity").map(|v| value_as_string(path, "verbosity", v)).transpose()?;
    let verbosity = verbosity.map(|v| parse_verbosity(path, &v)).transpose()?;

    let out_path = fields.get("out_path").map(|v| value_as_string(path, "out_path", v)).transpose()?.map(PathBuf::from);
    let n_workers = fields.get("n_workers").map(|v| value_as_workers(path, "n_workers", v)).transpose()?;
    let (parallelism_jobs, parallelism_pages) =
        fields.get("parallelism").map(|v| parallelism_section(path, v)).transpose()?.unwrap_or((None, None));
    let batch_file =
        fields.get("batch_file").map(|v| value_as_string(path, "batch_file", v)).transpose()?.map(PathBuf::from);
    let save_pdf = fields.get("save_pdf").map(|v| value_as_bool(path, "save_pdf", v)).transpose()?;
    let format = fields.get("format").map(|v| value_as_string(path, "format", v)).transpose()?;
    let target_lists = fields.get("target_lists").map(|v| value_as_string_list(path, "target_lists", v)).transpose()?;
    let formats_repo_path =
        fields.get("formats_repo").map(|v| value_as_string(path, "formats_repo", v)).transpose()?.map(PathBuf::from);
    let input_db_path = fields.get("db_path").map(|v| value_as_string(path, "db_path", v)).transpose()?.map(PathBuf::from);

    let url = fields.get("url").map(|v| value_as_string(path, "url", v)).transpose()?;
    let pdf = fields.get("pdf").map(|v| value_as_string(path, "pdf", v)).transpose()?;
    let singular =
        if url.is_some() || pdf.is_some() { Some(DocumentSpec { url, path: pdf.map(PathBuf::from), name: None }) } else { None };

    let plural = fields
        .get("reports")
        .map(|v| value_as_string_list(path, "reports", v))
        .transpose()?
        .map(|specs| {
            specs
                .iter()
                .map(|s| {
                    DocumentSpec::parse(s).map_err(|source| FileConfigError::InvalidReportSpecifier {
                        path: path.to_path_buf(),
                        value: s.clone(),
                        source,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;

    let reports = resolve_singular_and_plural_reports(singular, plural)
        .map_err(|source| FileConfigError::ReportsConflict { path: path.to_path_buf(), source })?;

    tracing::info!(path = %path.display(), "loaded configuration from file");
    Ok(PartialConfig {
        verbosity,
        reports,
        target_lists,
        format,
        out_path,
        out_profile: None,
        out_flags: None,
        n_workers,
        parallelism_jobs,
        parallelism_pages,
        batch_file,
        save_pdf,
        formats_repo_path,
        input_db_path,
        config_file: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tracing_setup::Verbosity;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn write(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    mod local_config_in {
        use super::*;

        #[test]
        fn finds_a_file_matching_the_config_freeports_pattern() {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "config-freeports.yaml", "{}");
            assert_eq!(local_config_in(dir.path()), Some(dir.path().join("config-freeports.yaml")));
        }

        #[test]
        fn finds_a_file_matching_the_freeports_config_pattern() {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "freeports-config.yml", "{}");
            assert_eq!(local_config_in(dir.path()), Some(dir.path().join("freeports-config.yml")));
        }

        #[test]
        fn matching_is_case_insensitive() {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "FREEPORTS-CONFIG.YAML", "{}");
            assert!(local_config_in(dir.path()).is_some());
        }

        #[test]
        fn a_leading_dot_is_accepted() {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), ".config-freeports.yaml", "{}");
            assert!(local_config_in(dir.path()).is_some());
        }

        #[test]
        fn an_unrelated_file_name_is_not_matched() {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "settings.yaml", "{}");
            assert_eq!(local_config_in(dir.path()), None);
        }

        #[test]
        fn an_empty_directory_returns_none() {
            let dir = tempfile::tempdir().unwrap();
            assert_eq!(local_config_in(dir.path()), None);
        }
    }

    mod user_config_in {
        use super::*;

        #[test]
        fn finds_freeports_yaml() {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "freeports.yaml", "{}");
            assert_eq!(user_config_in(Some(dir.path())), Some(dir.path().join("freeports.yaml")));
        }

        #[test]
        fn finds_freeports_yml_when_yaml_is_absent() {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "freeports.yml", "{}");
            assert_eq!(user_config_in(Some(dir.path())), Some(dir.path().join("freeports.yml")));
        }

        #[test]
        fn no_config_local_dir_at_all_returns_none() {
            assert_eq!(user_config_in(None), None);
        }

        #[test]
        fn a_config_local_dir_with_no_freeports_file_returns_none() {
            let dir = tempfile::tempdir().unwrap();
            assert_eq!(user_config_in(Some(dir.path())), None);
        }
    }

    mod system_config {
        use super::*;

        const SYSTEM_VARS: &[&str] = &["XDG_CONFIG_DIRS", "PROGRAMDATA", "SystemRoot"];
        static SYSTEM_ENV_LOCK: Mutex<()> = Mutex::new(());

        struct SystemEnvScope {
            _lock: std::sync::MutexGuard<'static, ()>,
            originals: Vec<(&'static str, Option<String>)>,
        }

        impl SystemEnvScope {
            fn new() -> Self {
                let lock = SYSTEM_ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
                let originals = SYSTEM_VARS.iter().map(|&k| (k, std::env::var(k).ok())).collect();
                for &k in SYSTEM_VARS {
                    unsafe { std::env::remove_var(k) };
                }
                Self { _lock: lock, originals }
            }

            fn set(&self, key: &str, value: &str) {
                unsafe { std::env::set_var(key, value) };
            }
        }

        impl Drop for SystemEnvScope {
            fn drop(&mut self) {
                for (k, v) in &self.originals {
                    match v {
                        Some(val) => unsafe { std::env::set_var(k, val) },
                        None => unsafe { std::env::remove_var(k) },
                    }
                }
            }
        }

        // The POSIX and Windows branches are both always compiled (design note in the module
        // doc: no `#[cfg(target_os)]`), so both are exercised on every CI platform regardless of
        // which OS actually runs `cargo test`.

        #[test]
        fn posix_xdg_config_dirs_is_searched_when_set() {
            let scope = SystemEnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "freeports.yaml", "{}");
            scope.set("XDG_CONFIG_DIRS", dir.path().to_str().unwrap());
            assert_eq!(system_config(), Some(dir.path().join("freeports.yaml")));
        }

        #[test]
        fn posix_xdg_config_dirs_with_multiple_colon_separated_entries_searches_each_in_order() {
            let scope = SystemEnvScope::new();
            let empty_dir = tempfile::tempdir().unwrap();
            let real_dir = tempfile::tempdir().unwrap();
            write(real_dir.path(), "freeports.yaml", "{}");
            let joined = format!("{}:{}", empty_dir.path().display(), real_dir.path().display());
            scope.set("XDG_CONFIG_DIRS", &joined);
            assert_eq!(system_config(), Some(real_dir.path().join("freeports.yaml")));
        }

        #[test]
        fn windows_programdata_is_searched_when_set() {
            let scope = SystemEnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "freeports.yaml", "{}");
            scope.set("PROGRAMDATA", dir.path().to_str().unwrap());
            assert_eq!(system_config(), Some(dir.path().join("freeports.yaml")));
        }

        #[test]
        fn windows_systemroot_is_used_when_programdata_has_no_file() {
            let scope = SystemEnvScope::new();
            let program_data = tempfile::tempdir().unwrap(); // present but empty
            let system_root = tempfile::tempdir().unwrap();
            write(system_root.path(), "freeports.yaml", "{}");
            scope.set("PROGRAMDATA", program_data.path().to_str().unwrap());
            scope.set("SystemRoot", system_root.path().to_str().unwrap());
            assert_eq!(system_config(), Some(system_root.path().join("freeports.yaml")));
        }

        #[test]
        fn nothing_set_anywhere_returns_none() {
            let _scope = SystemEnvScope::new();
            assert_eq!(system_config(), None);
        }
    }

    mod load_without_a_path {
        use super::*;

        #[test]
        fn returns_an_entirely_empty_partial_config() {
            let config = load(None).unwrap();
            assert_eq!(config, crate::cli::partial_config::PartialConfig::default());
        }
    }

    mod load_yaml_field_mapping {
        use super::*;

        #[test]
        fn out_path_is_mapped() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "out_path: /tmp/out\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.out_path, Some(PathBuf::from("/tmp/out")));
        }

        #[test]
        fn n_workers_is_mapped() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "n_workers: 4\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.n_workers, Some(Workers::Fixed(4)));
        }

        /// The dedicated section, with the two levels that are really consumed.
        #[test]
        fn the_parallelism_section_maps_both_levels() {
            let dir = tempfile::tempdir().unwrap();
            let path =
                write(dir.path(), "cfg.yaml", "parallelism:\n  jobs: 2\n  pages: auto\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.parallelism_jobs, Some(Workers::Fixed(2)));
            assert_eq!(config.parallelism_pages, Some(Workers::Auto));
        }

        #[test]
        fn a_parallelism_section_may_name_a_single_level() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "parallelism:\n  pages: 4\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.parallelism_jobs, None);
            assert_eq!(config.parallelism_pages, Some(Workers::Fixed(4)));
        }

        /// `auto` written as a bare YAML word, without quotes: the form it will take in every
        /// hand-written configuration file.
        #[test]
        fn auto_is_accepted_unquoted() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "n_workers: auto\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.n_workers, Some(Workers::Auto));
        }

        /// Accepting a level that was never implemented would make it look active, which is worse
        /// than refusing it.
        #[test]
        fn an_unknown_sub_key_of_parallelism_is_an_error_that_names_its_path() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "parallelism:\n  pipelines: 2\n");
            let error = load(Some(&path)).unwrap_err().to_string();
            assert!(error.contains("parallelism.pipelines"), "{error}");
        }

        #[test]
        fn a_parallelism_section_that_is_not_a_mapping_is_an_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "parallelism: 4\n");
            assert!(load(Some(&path)).is_err());
        }

        #[test]
        fn zero_is_rejected_inside_the_parallelism_section_too() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "parallelism:\n  jobs: 0\n");
            assert!(load(Some(&path)).is_err());
        }

        #[test]
        fn save_pdf_is_mapped_as_a_native_yaml_boolean() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "save_pdf: false\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.save_pdf, Some(false));
        }

        #[test]
        fn format_is_mapped() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "format: ACME-EN24\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.format, Some("ACME-EN24".to_string()));
        }

        #[test]
        fn target_lists_is_mapped_as_a_list() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "target_lists:\n  - TEST\n  - OTHER\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.target_lists, Some(vec!["TEST".to_string(), "OTHER".to_string()]));
        }

        #[test]
        fn formats_repo_key_maps_to_formats_repo_path_field() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "formats_repo: /opt/formats\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.formats_repo_path, Some(PathBuf::from("/opt/formats")));
        }

        #[test]
        fn db_path_key_maps_to_input_db_path_field() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "db_path: /opt/db\n");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.input_db_path, Some(PathBuf::from("/opt/db")));
        }

        #[test]
        fn an_empty_yaml_document_is_a_fully_empty_partial_config() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "");
            let config = load(Some(&path)).unwrap();
            assert_eq!(config, crate::cli::partial_config::PartialConfig::default());
        }
    }

    mod load_errors {
        use super::*;

        #[test]
        fn a_missing_file_is_a_typed_io_error_not_a_panic() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("does-not-exist.yaml");
            let result = std::panic::catch_unwind(|| load(Some(&path)));
            assert!(result.is_ok(), "must not panic");
            assert!(matches!(result.unwrap(), Err(FileConfigError::Io { .. })));
        }

        #[test]
        fn malformed_yaml_syntax_is_a_typed_error_not_a_panic() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "out_path: [unterminated\n");
            let result = std::panic::catch_unwind(|| load(Some(&path)));
            assert!(result.is_ok(), "must not panic");
            assert!(result.unwrap().is_err());
        }

        #[test]
        fn an_unknown_key_is_an_explicit_typed_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "not_a_real_key: 1\n");
            let result = load(Some(&path));
            assert!(matches!(result, Err(FileConfigError::UnknownKey { .. })), "got {result:?}");
        }

        #[test]
        fn n_workers_zero_is_rejected() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "n_workers: 0\n");
            assert!(load(Some(&path)).is_err());
        }
    }

    mod verbosity_key {
        use super::*;

        #[test_case::test_case("silent", Verbosity::Silent)]
        #[test_case::test_case("warn", Verbosity::Warn)]
        #[test_case::test_case("Trace", Verbosity::Trace)]
        #[test_case::test_case("DEBUG", Verbosity::Debug)]
        fn every_variant_name_case_insensitive_is_accepted(value: &str, expected: Verbosity) {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", &format!("verbosity: {value}\n"));
            let config = load(Some(&path)).unwrap();
            assert_eq!(config.verbosity, Some(expected));
        }

        #[test]
        fn an_unrecognized_verbosity_value_is_a_typed_error_not_a_panic() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "verbosity: not-a-level\n");
            let result = std::panic::catch_unwind(|| load(Some(&path)));
            assert!(result.is_ok(), "must not panic");
            assert!(matches!(result.unwrap(), Err(FileConfigError::InvalidVerbosity { .. })));
        }
    }

    mod reports_key {
        use super::*;

        #[test]
        fn a_list_of_specifiers_becomes_reports_in_order() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(
                dir.path(),
                "cfg.yaml",
                "reports:\n  - https://example.com/a.pdf\n  - https://example.com/b.pdf\n",
            );
            let config = load(Some(&path)).unwrap();
            let reports = config.reports.unwrap();
            assert_eq!(reports.len(), 2);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/a.pdf"));
            assert_eq!(reports[1].url.as_deref(), Some("https://example.com/b.pdf"));
        }

        #[test]
        fn an_invalid_specifier_in_the_list_is_a_typed_error_not_a_panic() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "reports:\n  - \"a:b:c:d\"\n");
            let result = std::panic::catch_unwind(|| load(Some(&path)));
            assert!(result.is_ok(), "must not panic");
            assert!(result.unwrap().is_err());
        }
    }

    mod url_and_pdf_single_keys {
        use super::*;

        #[test]
        fn url_alone_becomes_a_single_element_reports_list() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "url: https://example.com/report.pdf\n");
            let config = load(Some(&path)).unwrap();
            let reports = config.reports.unwrap();
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/report.pdf"));
        }

        #[test]
        fn pdf_alone_becomes_a_single_element_reports_list() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(dir.path(), "cfg.yaml", "pdf: /tmp/report.pdf\n");
            let config = load(Some(&path)).unwrap();
            let reports = config.reports.unwrap();
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].path, Some(PathBuf::from("/tmp/report.pdf")));
        }

        #[test]
        fn url_and_pdf_together_combine_into_one_spec() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(
                dir.path(),
                "cfg.yaml",
                "url: https://example.com/report.pdf\npdf: /tmp/report.pdf\n",
            );
            let config = load(Some(&path)).unwrap();
            let reports = config.reports.unwrap();
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].url.as_deref(), Some("https://example.com/report.pdf"));
            assert_eq!(reports[0].path, Some(PathBuf::from("/tmp/report.pdf")));
        }
    }

    mod reports_singular_and_plural_conflict {
        use super::*;

        #[test]
        fn reports_and_url_together_is_an_explicit_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(
                dir.path(),
                "cfg.yaml",
                "reports:\n  - https://example.com/a.pdf\nurl: https://example.com/b.pdf\n",
            );
            let result = load(Some(&path));
            assert!(matches!(result, Err(FileConfigError::ReportsConflict { .. })), "got {result:?}");
        }

        #[test]
        fn reports_and_pdf_together_is_an_explicit_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = write(
                dir.path(),
                "cfg.yaml",
                "reports:\n  - https://example.com/a.pdf\npdf: /tmp/x.pdf\n",
            );
            assert!(load(Some(&path)).is_err());
        }
    }
}
