//! Configurazione da file (YAML) e ricerca delle posizioni standard (cwd, XDG/`dirs`, sistema).
//!
//! `M9-implementation-plan.md` §2/§3 passo 7, §0 Q2/Q3/Q5.
//!
//! # Ricerca del file di configurazione (`find_config`)
//!
//! Tre livelli, in ordine di precedenza decrescente (identico al riferimento
//! `FreeportsFileConfig.find_config`, con la sola dipendenza `dirs` al posto di `xdg` per il
//! tier utente, §0 Q2):
//!
//! 1. **cwd** -- un file nella directory corrente che combacia (case-insensitive) con uno dei due
//!    pattern del riferimento: `^\.?(config|conf)[-._]?freeports\.ya?ml$` oppure
//!    `^\.?freeports[-._]?(config|conf)\.ya?ml$`.
//! 2. **Tier utente** -- `dirs::config_local_dir()` (non `config_dir()`, §0 Q2: su Windows
//!    risolve a `%LOCALAPPDATA%`, la prima voce del riferimento Python, non `%APPDATA%`
//!    roaming), cercando `freeports.yaml`/`freeports.yml`.
//! 3. **Tier di sistema** -- resta a mano, nessuno dei crate candidati lo espone (§0 Q2):
//!    - POSIX: `XDG_CONFIG_DIRS` (spezzata su `:`, fallback `/etc/xdg` se assente/vuota), poi
//!      `/etc` come ultimo tier separato;
//!    - Windows: `%PROGRAMDATA%`, poi `%SystemRoot%` come ultimo tier.
//!
//!    Entrambi i rami sono **sempre compilati** (non dietro `#[cfg(target_os)]`), così i test li
//!    esercitano entrambi indipendentemente dal sistema operativo che esegue `cargo test`.
//!
//! Nessun file trovato in nessun tier -> `None`, mai un errore.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
//! pub enum FileConfigError {
//!     Io { path: PathBuf, source: ... },
//!     Yaml { path: PathBuf, source: ... },
//!     UnknownKey { path: PathBuf, key: String },
//!     InvalidReportSpecifier { path: PathBuf, value: String, source: DocumentSpecError },
//!     ReportsConflict { path: PathBuf, source: SourceReportsConflict },
//!     InvalidVerbosity { path: PathBuf, value: String },
//!     InvalidValue { path: PathBuf, key: &'static str, value: String },
//! }
//!
//! /// Tier 1 (cwd), fattorizzata per accettare una directory esplicita: la funzione pubblica
//! /// `find_config()` la chiama con `std::env::current_dir()`, i test la chiamano direttamente
//! /// con una `tempfile::TempDir` -- niente `std::env::set_current_dir` nei test (che
//! /// muterebbe la cwd dell'intero processo, condivisa da tutti i test del crate eseguiti in
//! /// parallelo: una fonte di flakiness inter-modulo, non solo interna a questo file).
//! pub(crate) fn local_config_in(dir: &std::path::Path) -> Option<std::path::PathBuf>;
//!
//! /// Tier 2 (utente), fattorizzata sul valore già risolto di `dirs::config_local_dir()` per lo
//! /// stesso motivo di testabilità.
//! pub(crate) fn user_config_in(config_local_dir: Option<&std::path::Path>) -> Option<std::path::PathBuf>;
//!
//! /// Tier 3 (sistema): legge le variabili d'ambiente reali (`XDG_CONFIG_DIRS`, `PROGRAMDATA`,
//! /// `SystemRoot`) e il filesystem -- testabile con lo stesso meccanismo di `EnvScope` già usato
//! /// da `config_locations::env`.
//! pub(crate) fn system_config() -> Option<std::path::PathBuf>;
//!
//! pub fn find_config() -> Option<std::path::PathBuf>;
//!
//! /// `path: None` -> `Ok(PartialConfig::default())` (nessun file, nessun errore). `path:
//! /// Some` -> legge e valida lo YAML a quel percorso.
//! pub fn load(path: Option<&std::path::Path>) -> Result<PartialConfig, FileConfigError>;
//! ```
//!
//! # Chiavi YAML riconosciute
//!
//! | chiave | campo | note |
//! |---|---|---|
//! | `verbosity` | `verbosity` | stringa, uno dei sei nomi di variante, case-insensitive (§0 Q5 -- **non** più l'intero `0..5` del riferimento) |
//! | `out_path` | `out_path` | |
//! | `n_workers` | `n_workers` | intero positivo |
//! | `batch_file` | `batch_file` | |
//! | `save_pdf` | `save_pdf` | booleano YAML nativo |
//! | `url` | contribuisce allo spec singolare (con `pdf`), poi risolto in `reports` | |
//! | `pdf` | idem | il riferimento non aveva questa chiave nella propria mappa (probabile omissione: `PDF` era un campo del modello ma `_map_names` non lo citava) -- inclusa qui perché `targets/2_multireport_support.md`/§0 Q3 la richiedono esplicitamente come zucchero sintattico |
//! | `reports` | `reports` | lista di stringhe, ciascuna nella grammatica `DocumentSpec::parse` (§0 Q3) |
//! | `format` | `format` | |
//! | `target_lists` | `target_lists` | lista di stringhe |
//! | `formats_repo` | `formats_repo_path` | |
//! | `db_path` | `input_db_path` | |
//!
//! Una chiave sconosciuta è un **errore esplicito** (`FileConfigError::UnknownKey`) -- scelta del
//! test-writer, non decisa dal piano (`M9-implementation-plan.md` §4 lo lascia esplicitamente
//! aperto: "da scegliere e documentare in fase di implementazione, il riferimento solleva
//! `KeyError` implicito"). Coerente con `PLAN.md` §2 principio 4 (mai un comportamento ambiguo
//! risolto in silenzio) e con il comportamento *di fatto* del riferimento (un `KeyError` non
//! catturato). **Segnalato esplicitamente nel resoconto del test-writer come judgment call**,
//! non come lettura univoca del piano.
//!
//! `reports:` **e** (`url:` o `pdf:`) insieme sulla stessa sorgente -> errore esplicito, stesso
//! meccanismo di `config_locations::env` (`resolve_singular_and_plural_reports`, §0 Q3).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::de::Error as _;

use crate::cli::conf_parse::{DocumentSpec, DocumentSpecError};
use crate::cli::partial_config::{PartialConfig, SourceReportsConflict, resolve_singular_and_plural_reports};
use crate::core::tracing_setup::Verbosity;

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

/// Tier 1 (cwd): un file la cui *nome* combacia (case-insensitive) con uno dei due pattern del
/// riferimento. Fattorizzata su una directory esplicita, non su `std::env::current_dir()`, per
/// restare testabile senza mutare la cwd condivisa del processo -- vedi il doc-comment del modulo.
pub(crate) fn local_config_in(dir: &Path) -> Option<PathBuf> {
    let patterns = [
        onig::Regex::new(r"(?i)^\.?(config|conf)[-._]?freeports\.ya?ml$")
            .expect("fixed pattern, valid by construction -- verified at compile time"),
        onig::Regex::new(r"(?i)^\.?freeports[-._]?(config|conf)\.ya?ml$")
            .expect("fixed pattern, valid by construction -- verified at compile time"),
    ];
    let entries: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
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

/// Tier 2 (utente): `freeports.yaml`/`freeports.yml` dentro `config_local_dir` (già risolto dal
/// chiamante via `dirs::config_local_dir()`, §0 Q2 -- fattorizzata sul valore per testabilità).
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

/// Tier 3 (sistema): `XDG_CONFIG_DIRS` (spezzata su `:`, fallback `/etc/xdg`) poi `/etc` su POSIX;
/// `%PROGRAMDATA%` poi `%SystemRoot%` su Windows. Entrambi i rami sono sempre compilati (nessun
/// `#[cfg(target_os)]`), così i test li esercitano indipendentemente dal sistema operativo che
/// esegue `cargo test` -- vedi il doc-comment del modulo.
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
    let cwd = std::env::current_dir().ok()?;
    local_config_in(&cwd).or_else(|| user_config_in(dirs::config_local_dir().as_deref())).or_else(system_config)
}

fn parse_verbosity(path: &Path, value: &str) -> Result<Verbosity, FileConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "silent" => Ok(Verbosity::Silent),
        "erroronly" => Ok(Verbosity::ErrorOnly),
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

fn value_as_positive_usize(path: &Path, key: &'static str, value: &serde_yaml::Value) -> Result<usize, FileConfigError> {
    match value.as_u64() {
        Some(n) if n > 0 => Ok(n as usize),
        _ => Err(FileConfigError::InvalidValue { path: path.to_path_buf(), key, value: format!("{value:?}") }),
    }
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

/// `path: None` -> `Ok(PartialConfig::default())` (nessun file, nessun errore). `path: Some` ->
/// legge e valida lo YAML a quel percorso -- chiave sconosciuta -> `UnknownKey` (scelta del
/// test-writer, vedi il doc-comment del modulo).
pub fn load(path: Option<&Path>) -> Result<PartialConfig, FileConfigError> {
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

    const KNOWN_KEYS: [&str; 12] = [
        "verbosity", "out_path", "n_workers", "batch_file", "save_pdf", "url", "pdf", "reports", "format",
        "target_lists", "formats_repo", "db_path",
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
    let n_workers = fields.get("n_workers").map(|v| value_as_positive_usize(path, "n_workers", v)).transpose()?;
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

    Ok(PartialConfig {
        verbosity,
        reports,
        target_lists,
        format,
        out_path,
        out_profile: None,
        out_flags: None,
        n_workers,
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
            assert_eq!(config.n_workers, Some(4));
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
