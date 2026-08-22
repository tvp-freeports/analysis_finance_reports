use std::path::{Path, PathBuf};

use onig::Regex;

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

/// `InvalidValue` covers a YAML value with the wrong basic shape (e.g. a mapping where a string
/// was expected) — meaningful only here, since env vars and CLI args are always strings already.
/// `InvalidField` covers everything that goes through the same [`ConfigError`] validators
/// `cli::cmd_config`, `cli::env_config`, and `cli::job_config` use, so a bad `verbosity`/
/// `out_profile`/`n_workers`/... key produces exactly the same message as the equivalent
/// `--flag`, env var, or batch-file column would.
#[derive(Debug, Clone, PartialEq)]
pub enum FileConfigError {
    Io { path: PathBuf, message: String },
    Yaml(String),
    NotAMapping,
    UnknownKey(String),
    InvalidValue { key: &'static str, message: String },
    InvalidField { key: &'static str, source: ConfigError },
    PathDoesNotExist { key: &'static str, path: PathBuf },
    NotAFile { key: &'static str, path: PathBuf },
    NotADirectory { key: &'static str, path: PathBuf },
}

impl std::fmt::Display for FileConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileConfigError::Io { path, message } => {
                write!(f, "could not read config file `{}`: {message}", path.display())
            }
            FileConfigError::Yaml(message) => write!(f, "invalid YAML: {message}"),
            FileConfigError::NotAMapping => write!(f, "config file must contain a YAML mapping at the top level"),
            FileConfigError::UnknownKey(key) => write!(f, "unknown config key `{key}`"),
            FileConfigError::InvalidValue { key, message } => write!(f, "invalid value for `{key}`: {message}"),
            FileConfigError::InvalidField { key, source } => write!(f, "invalid value for `{key}`: {source}"),
            FileConfigError::PathDoesNotExist { key, path } => {
                write!(f, "`{key}`: path `{}` does not exist", path.display())
            }
            FileConfigError::NotAFile { key, path } => write!(f, "`{key}`: `{}` is not a file", path.display()),
            FileConfigError::NotADirectory { key, path } => write!(f, "`{key}`: `{}` is not a directory", path.display()),
        }
    }
}

impl std::error::Error for FileConfigError {}

fn matches_local_config_name(file_name: &str) -> bool {
    const PATTERNS: [&str; 2] = [
        r"(?i)^\.?(config|conf)[-._]?freeports\.ya?ml$",
        r"(?i)^\.?freeports[-._]?(config|conf)\.ya?ml$",
    ];
    PATTERNS.iter().any(|p| Regex::new(p).expect("static regex is valid").is_match(file_name))
}

/// Mirrors `FreeportsFileConfig._local_config`: the first file in the current directory (in
/// `std::fs::read_dir`'s OS-given order, same as Python's `os.listdir`) matching either accepted
/// naming pattern.
pub fn local_config() -> Option<PathBuf> {
    let entries = std::fs::read_dir(".").ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else { continue };
        if !matches_local_config_name(file_name) {
            continue;
        }
        let path = entry.path();
        if path.is_file() {
            return std::fs::canonicalize(&path).ok().or(Some(path));
        }
    }
    None
}

#[cfg(unix)]
fn xdg_config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(home) if !home.is_empty() => dirs.push(PathBuf::from(home)),
        _ => {
            if let Ok(home) = std::env::var("HOME") {
                dirs.push(PathBuf::from(home).join(".config"));
            }
        }
    }
    let config_dirs = std::env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".to_string());
    for dir in config_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(PathBuf::from(dir));
    }
    dirs
}

#[cfg(unix)]
pub fn standard_config() -> Option<PathBuf> {
    for dir in xdg_config_dirs() {
        for file_name in ["freeports.yaml", "freeports.yml"] {
            let candidate = dir.join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(windows)]
pub fn standard_config() -> Option<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local_appdata));
    }
    if let Ok(program_data) = std::env::var("PROGRAMDATA") {
        dirs.push(PathBuf::from(program_data));
    } else {
        dirs.push(PathBuf::from(r"C:\ProgramData"));
    }
    for dir in dirs {
        for file_name in ["freeports.yaml", "freeports.yml"] {
            let candidate = dir.join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Mirrors `FreeportsFileConfig._system_config`.
#[cfg(unix)]
pub fn system_config() -> Option<PathBuf> {
    for path in ["/etc/freeports.yaml", "/etc/freeports.yml"] {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

#[cfg(windows)]
pub fn system_config() -> Option<PathBuf> {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    for file_name in ["freeports.yaml", "freeports.yml"] {
        let candidate = PathBuf::from(&system_root).join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Mirrors `FreeportsFileConfig.find_config`: local, then standard (XDG/AppData), then system
/// (`/etc`/Windows system dir), first hit wins.
pub fn find_config() -> Option<PathBuf> {
    local_config().or_else(standard_config).or_else(system_config)
}

fn as_path(value: &serde_yaml::Value, key: &'static str) -> Result<PathBuf, FileConfigError> {
    value
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| FileConfigError::InvalidValue { key, message: "expected a string path".to_string() })
}

fn as_string(value: &serde_yaml::Value, key: &'static str) -> Result<String, FileConfigError> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| FileConfigError::InvalidValue { key, message: "expected a string".to_string() })
}

fn as_bool(value: &serde_yaml::Value, key: &'static str) -> Result<bool, FileConfigError> {
    value.as_bool().ok_or_else(|| FileConfigError::InvalidValue { key, message: "expected true or false".to_string() })
}

fn as_positive_u32(value: &serde_yaml::Value, key: &'static str) -> Result<u32, FileConfigError> {
    let n = value.as_i64().ok_or_else(|| FileConfigError::InvalidValue { key, message: "expected an integer".to_string() })?;
    validate_workers(n).map_err(|source| FileConfigError::InvalidField { key, source })
}

/// Mirrors `Lists`'s `BeforeValidator`: a single string wraps into a 1-element list; a YAML
/// sequence is taken as-is (each entry must be a string).
fn as_string_list(value: &serde_yaml::Value, key: &'static str) -> Result<Vec<String>, FileConfigError> {
    if let Some(s) = value.as_str() {
        return Ok(vec![s.to_string()]);
    }
    if let Some(seq) = value.as_sequence() {
        return seq.iter().map(|v| as_string(v, key)).collect();
    }
    Err(FileConfigError::InvalidValue { key, message: "expected a string or a list of strings".to_string() })
}

/// Mirrors `flag_from_string`'s `Union[str, list]` input: a YAML sequence of flag names joins
/// into a `|`-expression before evaluating, matching a plain string expression.
fn as_out_flags(value: &serde_yaml::Value, key: &'static str) -> Result<OutFlags, FileConfigError> {
    let expression = if let Some(s) = value.as_str() {
        s.to_string()
    } else if let Some(seq) = value.as_sequence() {
        seq.iter().map(|v| as_string(v, key)).collect::<Result<Vec<_>, _>>()?.join(" | ")
    } else {
        return Err(FileConfigError::InvalidValue { key, message: "expected a string or a list of flag names".to_string() });
    };
    OutFlags::parse(&expression).map_err(|source| FileConfigError::InvalidField { key, source })
}

fn existing_file(path: PathBuf, key: &'static str) -> Result<PathBuf, FileConfigError> {
    if !path.exists() {
        return Err(FileConfigError::PathDoesNotExist { key, path });
    }
    if !path.is_file() {
        return Err(FileConfigError::NotAFile { key, path });
    }
    Ok(path)
}

fn existing_dir(path: PathBuf, key: &'static str) -> Result<PathBuf, FileConfigError> {
    if !path.exists() {
        return Err(FileConfigError::PathDoesNotExist { key, path });
    }
    if !path.is_dir() {
        return Err(FileConfigError::NotADirectory { key, path });
    }
    Ok(path)
}

/// Parses YAML content (already read from a file) into a [`PartialConfig`]. Split out from
/// [`load`] so tests don't need real files on disk for every case.
pub fn parse(yaml_source: &str) -> Result<PartialConfig, FileConfigError> {
    let value: serde_yaml::Value = serde_yaml::from_str(yaml_source).map_err(|e| FileConfigError::Yaml(e.to_string()))?;
    let mapping = value.as_mapping().ok_or(FileConfigError::NotAMapping)?;

    let mut config = PartialConfig::default();
    for (raw_key, raw_value) in mapping {
        let key = raw_key.as_str().ok_or(FileConfigError::NotAMapping)?;
        match key {
            "verbosity" => {
                let n = raw_value.as_i64().ok_or_else(|| FileConfigError::InvalidValue { key: "verbosity", message: "expected an integer".to_string() })?;
                let verbosity =
                    Verbosity::new(n).map_err(|source| FileConfigError::InvalidField { key: "verbosity", source })?;
                config.verbosity = Some(verbosity);
            }
            "out_path" => config.out_path = Some(as_path(raw_value, "out_path")?),
            "out_profile" => {
                let s = as_string(raw_value, "out_profile")?;
                let mode = s
                    .parse::<OutStructureMode>()
                    .map_err(|source| FileConfigError::InvalidField { key: "out_profile", source })?;
                config.out_profile = Some(mode);
            }
            "out_flags" => config.out_flags = Some(as_out_flags(raw_value, "out_flags")?),
            "n_workers" => config.n_workers = Some(as_positive_u32(raw_value, "n_workers")?),
            "batch_file" => {
                let path = existing_file(as_path(raw_value, "batch_file")?, "batch_file")?;
                config.batch_file = Some(path);
            }
            "save_pdf" => config.save_pdf = Some(as_bool(raw_value, "save_pdf")?),
            "input_report" => {
                let s = as_string(raw_value, "input_report")?;
                let spec = s
                    .parse::<DocumentSpec>()
                    .map_err(|source: DocumentSpecError| FileConfigError::InvalidField { key: "input_report", source: source.into() })?;
                config.input_reports = Some(vec![spec]);
            }
            "format" => config.format = Some(as_string(raw_value, "format")?),
            "target_lists" => config.target_lists = Some(as_string_list(raw_value, "target_lists")?),
            "formats_repo" => {
                let path = existing_dir(as_path(raw_value, "formats_repo")?, "formats_repo")?;
                config.formats_repo_path = Some(path);
            }
            "db_path" => {
                let path = existing_dir(as_path(raw_value, "db_path")?, "db_path")?;
                config.input_db_path = Some(path);
            }
            other => return Err(FileConfigError::UnknownKey(other.to_string())),
        }
    }
    Ok(config)
}

/// Mirrors `FreeportsFileConfig.__init__`: if `path` is `None`, [`find_config`] is used; if
/// neither finds anything, an empty [`PartialConfig`] is returned (no error — no config file is a
/// normal, supported state).
pub fn load(path: Option<&Path>) -> Result<PartialConfig, FileConfigError> {
    let path = match path {
        Some(p) => Some(p.to_path_buf()),
        None => find_config(),
    };
    let Some(path) = path else {
        return Ok(PartialConfig::default());
    };
    let content = std::fs::read_to_string(&path).map_err(|e| FileConfigError::Io { path: path.clone(), message: e.to_string() })?;
    parse(&content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    #[test_case("config-freeports.yaml"; "config dash freeports yaml")]
    #[test_case("config-freeports.yml"; "config dash freeports yml")]
    #[test_case(".config-freeports.yaml"; "dotfile")]
    #[test_case("conf.freeports.yaml"; "conf dot freeports")]
    #[test_case("CONFIG-FREEPORTS.YAML"; "uppercase")]
    #[test_case("freeports-config.yaml"; "freeports dash config")]
    #[test_case("freeports_conf.yml"; "freeports underscore conf")]
    #[test_case(".freeportsconfig.yaml"; "dotfile no separator")]
    fn recognizes_every_accepted_local_config_name(name: &str) {
        assert!(matches_local_config_name(name));
    }

    #[test_case("readme.md"; "unrelated file")]
    #[test_case("freeports.yaml"; "bare freeports yaml matches neither local pattern")]
    #[test_case("config-freeports.json"; "wrong extension")]
    #[test_case("config-freeportsx.yaml"; "extra suffix breaks the anchor")]
    fn rejects_names_that_do_not_match(name: &str) {
        assert!(!matches_local_config_name(name));
    }

    #[test]
    fn empty_yaml_mapping_yields_an_empty_partial_config() {
        let config = parse("{}").unwrap();
        assert_eq!(config, PartialConfig::default());
    }

    #[test]
    fn parses_every_recognized_scalar_key() {
        let yaml = "
verbosity: 3
out_path: /tmp/out
out_profile: SINGLE_FILE
out_flags: COMPRESSED
n_workers: 4
save_pdf: false
input_report: http://example.com/report.pdf
format: my-format
target_lists: TEST
";
        let config = parse(yaml).unwrap();
        assert_eq!(config.verbosity, Some(Verbosity::new(3).unwrap()));
        assert_eq!(config.out_path, Some(PathBuf::from("/tmp/out")));
        assert_eq!(config.out_profile, Some(OutStructureMode::SingleFile));
        assert!(config.out_flags.unwrap().contains(OutFlags::COMPRESSED));
        assert_eq!(config.n_workers, Some(4));
        assert_eq!(config.save_pdf, Some(false));
        assert_eq!(config.input_reports.as_ref().unwrap().len(), 1);
        assert_eq!(config.input_reports.unwrap()[0].url.as_ref().unwrap().to_string(), "http://example.com/report.pdf");
        assert_eq!(config.format, Some("my-format".to_string()));
        assert_eq!(config.target_lists, Some(vec!["TEST".to_string()]));
    }

    #[test]
    fn target_lists_accepts_a_yaml_sequence_too() {
        let config = parse("target_lists:\n  - TEST\n  - OTHER\n").unwrap();
        assert_eq!(config.target_lists, Some(vec!["TEST".to_string(), "OTHER".to_string()]));
    }

    #[test]
    fn out_flags_accepts_a_yaml_sequence_of_names() {
        let config = parse("out_flags:\n  - COMPRESSED\n  - SEPARATE_OUT_FILES\n").unwrap();
        let flags = config.out_flags.unwrap();
        assert!(flags.contains(OutFlags::COMPRESSED));
        assert!(flags.contains(OutFlags::SEPARATE_OUT_FILES));
    }

    #[test]
    fn unknown_key_is_a_clean_error_not_a_crash() {
        assert_eq!(parse("pdf: /tmp/report.pdf"), Err(FileConfigError::UnknownKey("pdf".to_string())));
        assert_eq!(parse("totally_unknown: 1"), Err(FileConfigError::UnknownKey("totally_unknown".to_string())));
    }

    #[test]
    fn verbosity_out_of_range_is_reported() {
        assert_eq!(
            parse("verbosity: 9"),
            Err(FileConfigError::InvalidField { key: "verbosity", source: ConfigError::VerbosityOutOfRange(9) })
        );
    }

    #[test]
    fn n_workers_zero_carries_the_shared_config_error() {
        assert_eq!(
            parse("n_workers: 0"),
            Err(FileConfigError::InvalidField { key: "n_workers", source: ConfigError::InvalidWorkers("0".to_string()) })
        );
    }

    #[test]
    fn n_workers_non_integer_is_a_shape_error() {
        assert_eq!(
            parse("n_workers: not-a-number"),
            Err(FileConfigError::InvalidValue { key: "n_workers", message: "expected an integer".to_string() })
        );
    }

    #[test]
    fn out_profile_unknown_value_carries_the_shared_config_error() {
        assert_eq!(
            parse("out_profile: NOPE"),
            Err(FileConfigError::InvalidField { key: "out_profile", source: ConfigError::InvalidOutStructureMode("NOPE".to_string()) })
        );
    }

    #[test]
    fn non_mapping_top_level_is_rejected() {
        assert_eq!(parse("- 1\n- 2\n"), Err(FileConfigError::NotAMapping));
    }

    #[test]
    fn batch_file_must_exist_and_be_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.csv");
        let yaml = format!("batch_file: {}\n", missing.display());
        assert_eq!(
            parse(&yaml),
            Err(FileConfigError::PathDoesNotExist { key: "batch_file", path: missing })
        );

        let as_dir = dir.path();
        let yaml = format!("batch_file: {}\n", as_dir.display());
        assert_eq!(parse(&yaml), Err(FileConfigError::NotAFile { key: "batch_file", path: as_dir.to_path_buf() }));
    }

    #[test]
    fn formats_repo_and_db_path_must_be_existing_directories() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("not_a_dir.txt");
        std::fs::write(&file_path, "x").unwrap();
        let yaml = format!("formats_repo: {}\n", file_path.display());
        assert_eq!(
            parse(&yaml),
            Err(FileConfigError::NotADirectory { key: "formats_repo", path: file_path.clone() })
        );

        let yaml = format!("db_path: {}\n", dir.path().display());
        let config = parse(&yaml).unwrap();
        assert_eq!(config.input_db_path, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn load_with_no_config_file_and_none_found_returns_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.yaml");
        assert!(matches!(load(Some(&missing)), Err(FileConfigError::Io { .. })));
    }

    #[test]
    fn load_reads_and_parses_a_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("freeports.yaml");
        std::fs::write(&config_path, "verbosity: 4\n").unwrap();
        let config = load(Some(&config_path)).unwrap();
        assert_eq!(config.verbosity, Some(Verbosity::new(4).unwrap()));
    }
}
