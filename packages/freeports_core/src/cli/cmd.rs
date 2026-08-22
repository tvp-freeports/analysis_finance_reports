use super::config_locations::cmd::{from_args, CliArgs, CmdConfigError};
use super::config_locations::env::{load as load_env_config, EnvConfigError};
use super::config_locations::file::{load as load_file_config, FileConfigError};
use super::freeports_config::{FreeportsConfig, FreeportsConfigError};
use super::partial_config::{ConfigLocations, ConfigSource, PartialConfig};

pub const DEFAULT_VERBOSITY: u8 = 2;

#[derive(Debug)]
pub enum CmdError {
    Cmd(CmdConfigError),
    Env(EnvConfigError),
    File(FileConfigError),
    Config(FreeportsConfigError),
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CmdError::Cmd(e) => write!(f, "command-line arguments: {e}"),
            CmdError::Env(e) => write!(f, "environment configuration: {e}"),
            CmdError::File(e) => write!(f, "config file: {e}"),
            CmdError::Config(e) => write!(f, "configuration: {e}"),
        }
    }
}

impl std::error::Error for CmdError {}

impl From<CmdConfigError> for CmdError {
    fn from(e: CmdConfigError) -> Self {
        CmdError::Cmd(e)
    }
}
impl From<EnvConfigError> for CmdError {
    fn from(e: EnvConfigError) -> Self {
        CmdError::Env(e)
    }
}
impl From<FileConfigError> for CmdError {
    fn from(e: FileConfigError) -> Self {
        CmdError::File(e)
    }
}
impl From<FreeportsConfigError> for CmdError {
    fn from(e: FreeportsConfigError) -> Self {
        CmdError::Config(e)
    }
}

/// Resolves already-parsed `CliArgs` against env and file config, mirroring `cmd()`'s precedence
/// chain: default → file → env → cmd, with a preliminary env+cmd-only pass first just to find
/// `CONFIG_FILE`'s own path (matching the original's `tmp_config`/`tmp_config_location` two-pass
/// resolution — the config file's path can itself come from `--config` or `FREEPORTS_CONFIG_FILE`,
/// so it has to be resolved before the file it names can be loaded and merged in for real).
/// Stops short of [`freeports_config::build`] — see [`resolve_config`], which does that on top —
/// because batch mode (Fase E, punto 3d-iv) needs this merged-but-unbuilt [`PartialConfig`] as the
/// base each job row overlays onto, not the fully-validated single-job [`FreeportsConfig`].
pub fn resolve_partial_config(cli_args: CliArgs) -> Result<PartialConfig, CmdError> {
    let cmd_config = from_args(cli_args, DEFAULT_VERBOSITY)?;
    let env_config = load_env_config()?;

    let mut scratch_locations = ConfigLocations::default();
    let preliminary = PartialConfig::defaults()
        .overwrite(&env_config, ConfigSource::Env, &mut scratch_locations)
        .overwrite(&cmd_config, ConfigSource::Cmd, &mut scratch_locations);

    let file_config = load_file_config(preliminary.config_file.as_deref())?;

    let mut locations = ConfigLocations::default();
    let merged: PartialConfig = PartialConfig::defaults()
        .overwrite(&file_config, ConfigSource::File, &mut locations)
        .overwrite(&env_config, ConfigSource::Env, &mut locations)
        .overwrite(&cmd_config, ConfigSource::Cmd, &mut locations);

    Ok(merged)
}

/// [`resolve_partial_config`] followed by [`FreeportsConfig::build`] — the single-job entry
/// point `main.rs` uses directly when there's no `--batch` file. No `py: Python<'_>` parameter:
/// `build` attaches its own where it actually needs one (see its doc comment).
pub fn resolve_config(cli_args: CliArgs) -> Result<FreeportsConfig, CmdError> {
    let merged = resolve_partial_config(cli_args)?;
    Ok(FreeportsConfig::build(merged)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use pretty_assertions::assert_eq;
    use pyo3::prelude::*;

    fn parse(argv: &[&str]) -> CliArgs {
        let mut full = vec!["freeports"];
        full.extend_from_slice(argv);
        CliArgs::parse_from(full)
    }

    /// `find_config`'s XDG search (`standard_config`) reads real machine state via
    /// `XDG_CONFIG_HOME` — this dev machine has a real `~/.config/freeports.yaml` from earlier
    /// manual CLI testing (`save_pdf`/`verbosity`/`out_path`/`target_lists` all set). Every test
    /// in this module runs through this helper so none of them depend on what's actually on disk
    /// outside the test, whether or not they pass `--config` themselves.
    fn resolve_isolated(argv: &[&str]) -> Result<FreeportsConfig, CmdError> {
        let _env_lock = super::super::config_locations::env::ENV_LOCK.lock().unwrap();
        let empty_xdg = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("XDG_CONFIG_HOME", empty_xdg.path()) };
        Python::attach(crate::test_support::ensure_freeports_imported);
        let result = resolve_config(parse(argv));
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        result
    }

    #[test]
    fn cmd_line_input_resolves_through_the_full_precedence_chain() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("report.pdf");
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();

        let config = resolve_isolated(&[
            "--input",
            pdf.to_str().unwrap(),
            "--format",
            "my-format",
            "--target-list",
            "TEST",
            "--db-directory",
            dir.path().to_str().unwrap(),
            "--formats-directory",
            dir.path().to_str().unwrap(),
            "--out",
            dir.path().to_str().unwrap(),
        ])
        .unwrap();
        assert_eq!(config.format.as_deref(), Some("my-format"));
        assert_eq!(config.target_lists, vec!["TEST".to_string()]);
        assert_eq!(config.input_reports.len(), 1);
    }

    #[test]
    fn completely_empty_invocation_reports_the_first_missing_required_field() {
        assert!(matches!(
            resolve_isolated(&[]),
            Err(CmdError::Config(FreeportsConfigError::MissingTargetLists))
        ));
    }

    #[test]
    fn no_input_document_surfaces_as_no_input_reports() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_isolated(&[
            "--target-list",
            "TEST",
            "--db-directory",
            dir.path().to_str().unwrap(),
            "--formats-directory",
            dir.path().to_str().unwrap(),
        ]);
        assert!(matches!(result, Err(CmdError::Config(FreeportsConfigError::NoInputReports))));
    }

    #[test]
    fn config_flag_actually_loads_the_named_file_fixing_the_original_no_op_bug() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("report.pdf");
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();
        let config_path = dir.path().join("custom.yaml");
        std::fs::write(&config_path, format!("input_report: {}\nformat: from-file\n", pdf.display())).unwrap();

        let config = resolve_isolated(&[
            "--config",
            config_path.to_str().unwrap(),
            "--target-list",
            "TEST",
            "--db-directory",
            dir.path().to_str().unwrap(),
            "--formats-directory",
            dir.path().to_str().unwrap(),
            "--out",
            dir.path().to_str().unwrap(),
        ])
        .unwrap();
        assert_eq!(config.format.as_deref(), Some("from-file"));
        assert_eq!(config.input_reports.len(), 1);
    }

    #[test]
    fn cmd_line_overrides_file_config() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("report.pdf");
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();
        let config_path = dir.path().join("custom.yaml");
        std::fs::write(&config_path, "format: from-file\n").unwrap();

        let config = resolve_isolated(&[
            "--config",
            config_path.to_str().unwrap(),
            "--input",
            pdf.to_str().unwrap(),
            "--format",
            "from-cmd",
            "--target-list",
            "TEST",
            "--db-directory",
            dir.path().to_str().unwrap(),
            "--formats-directory",
            dir.path().to_str().unwrap(),
            "--out",
            dir.path().to_str().unwrap(),
        ])
        .unwrap();
        assert_eq!(config.format.as_deref(), Some("from-cmd"));
    }
}
