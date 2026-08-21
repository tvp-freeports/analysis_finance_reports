//! Rust port of `_internals/cli/conf_parse.py`'s `ParitalConfiguration` mixin and the shared
//! field set that `FreeportsFileConfig`/`FreeportsEnvConfig`/`FreeportsCmdConfig`/`DEFAULT_CONFIG`
//! all populate a subset of. See `agent-memory/rust-native-binary-plan.md`, Fase E, punto 3b-ii.
//!
//! The Python original gives each config source its own Pydantic model (different field subsets,
//! different validation) and merges them via `overwrite_config`, which walks `self.model_dump()`
//! generically — every field whose value isn't `None` overwrites the running config dict, and
//! which source last set it is recorded per-field for the startup diagnostic log (`log_config`).
//! Rust has no equivalent to iterating a struct's fields generically, so instead every source
//! (file, env, cmd, default, batch job row) builds the *same* [`PartialConfig`] type directly, and
//! [`PartialConfig::overwrite`] does the merge field-by-field, recording each field's [`ConfigSource`]
//! into a [`ConfigLocations`] as it goes.
//!
//! [`ConfigLocations`] is a *twin* of `PartialConfig`: same field names, but each typed
//! `ConfigSource` instead of `Option<T>` — every field always has a known origin (starting out
//! [`ConfigSource::Default`] before anything overlays it), unlike a map that would simply have no
//! entry for an as-yet-untouched field. Both structs, plus `overwrite`, come from the single
//! [`partial_config!`] declaration below, so a field's name and type are written exactly once —
//! there's no second place (a hand-written `overwrite` body, or a map key kept aligned with the
//! field name by hand) to remember to update when a config field is added.
//!
//! **Design decision (user confirmed, 2026-08-20)**: the Python original's `FreeportsFileConfig`/
//! `FreeportsEnvConfig` have singular `URL`/`PDF` fields that turned out to be completely dead —
//! validated, but never actually reachable by `FreeportsConfig` (which only has `INPUT_REPORTS:
//! List[DocumentSpec]`, populated solely by `--input` on the command line; a `pdf_path_validation`
//! validator that used to bridge singular `URL`/`PDF` into a `DocumentSpec` is present in the
//! source only as a comment, evidently dropped when the schema moved onto `INPUT_REPORTS`). Rather
//! than port that dead end or resurrect the disconnected old bridge, file/env config here get a
//! **single** `DocumentSpec` field (`input_report`, parsed with the same [`super::conf_parse::DocumentSpec`]
//! used by `--input`) that folds into the *same* `input_reports: Option<Vec<DocumentSpec>>` slot
//! cmd-line config's (list-valued) `--input` uses — one document from file/env config actually
//! works now, expressed with the one grammar this crate already has, instead of two disconnected
//! fields.

use std::path::PathBuf;

use super::conf_parse::{DocumentSpec, OutFlags, OutStructureMode, Verbosity};

/// Which config source last set a field, so it can later be reported (e.g. in a startup
/// diagnostic log) where each part of the resolved configuration actually came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Default,
    File,
    Env,
    Cmd,
    Job,
}

impl Default for ConfigSource {
    fn default() -> Self {
        ConfigSource::Default
    }
}

/// Declares `PartialConfig` and its twin `ConfigLocations`, and generates
/// [`PartialConfig::overwrite`], all from the same field list — so each field's name and type
/// live in exactly one place.
macro_rules! partial_config {
    ($($field:ident : $ty:ty),+ $(,)?) => {
        /// Every config-source-settable field, all optional — mirrors the union of
        /// `FreeportsFileConfig`/`FreeportsEnvConfig`/`FreeportsCmdConfig`/`DEFAULT_CONFIG`'s
        /// fields. `None` means "this source doesn't set this field", exactly like the Python
        /// originals' fields defaulting to `None` and `overwrite_config` skipping `None` values.
        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct PartialConfig {
            $(pub $field: Option<$ty>,)+
        }

        /// Tracks which source last set each field of [`PartialConfig`] — Rust equivalent of the
        /// `config_location` dict `overwrite_config` threads through in the Python original.
        /// [`Default`]s to every field being [`ConfigSource::Default`], matching
        /// `DEFAULT_CONFIG_LOCATION` pre-filling every key before any real source overlays it.
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct ConfigLocations {
            $(pub $field: ConfigSource,)+
        }

        impl PartialConfig {
            /// Overlays every `Some` field of `overlay` onto `self`, recording `source` as that
            /// field's origin. Mirrors `ParitalConfiguration.overwrite_config`: only non-`None`
            /// values overwrite, so a lower-precedence source's already-set field survives an
            /// overlay that leaves it unset.
            pub fn overwrite(mut self, overlay: &PartialConfig, source: ConfigSource, locations: &mut ConfigLocations) -> Self {
                $(
                    if let Some(value) = overlay.$field.clone() {
                        self.$field = Some(value);
                        locations.$field = source;
                    }
                )+
                self
            }
        }
    };
}

partial_config! {
    verbosity: Verbosity,
    input_reports: Vec<DocumentSpec>,
    out_profile: OutStructureMode,
    out_flags: OutFlags,
    out_path: PathBuf,
    n_workers: u32,
    batch_file: PathBuf,
    save_pdf: bool,
    format: String,
    target_lists: Vec<String>,
    formats_repo_path: PathBuf,
    input_db_path: PathBuf,
    config_file: PathBuf,
    prefix_out: String,
}

impl PartialConfig {
    /// Mirrors `DEFAULT_CONFIG`: the base every other config source overlays onto. `target_lists`,
    /// `input_db_path`, and `formats_repo_path` are deliberately absent — the Python original sets
    /// them to `None` in `DEFAULT_CONFIG` too, but their `FreeportsConfig` field types were never
    /// `Optional` (confirmed empirically: omitting `--db-directory`/`--formats-directory`/
    /// `--target-list` and every equivalent env/file-config key crashes with a Pydantic
    /// "Input is not a valid path"/"Input should be a valid list" error regardless of what
    /// `DEFAULT_CONFIG` claims). That's accurately modeled here as "genuinely required from some
    /// source", not patched with an invented universal default that wouldn't make sense (there's no
    /// sensible default for "where is your formats repo").
    pub fn defaults() -> PartialConfig {
        PartialConfig {
            verbosity: Some(Verbosity::new(2).expect("2 is in range")),
            n_workers: Some(std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1)),
            out_path: Some(PathBuf::from(".")),
            out_profile: Some(OutStructureMode::Regular),
            out_flags: Some(OutFlags::NONE),
            save_pdf: Some(true),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn locations_default_to_config_source_default_for_every_field() {
        let locations = ConfigLocations::default();
        assert_eq!(locations.verbosity, ConfigSource::Default);
        assert_eq!(locations.save_pdf, ConfigSource::Default);
    }

    #[test]
    fn overwrite_takes_every_set_field_from_the_overlay() {
        let base = PartialConfig::default();
        let overlay = PartialConfig {
            verbosity: Some(Verbosity::new(3).unwrap()),
            save_pdf: Some(false),
            ..Default::default()
        };
        let mut locations = ConfigLocations::default();
        let merged = base.overwrite(&overlay, ConfigSource::Env, &mut locations);
        assert_eq!(merged.verbosity, Some(Verbosity::new(3).unwrap()));
        assert_eq!(merged.save_pdf, Some(false));
        assert_eq!(locations.verbosity, ConfigSource::Env);
        assert_eq!(locations.save_pdf, ConfigSource::Env);
    }

    #[test]
    fn overwrite_leaves_unset_overlay_fields_untouched() {
        let base = PartialConfig { verbosity: Some(Verbosity::new(1).unwrap()), ..Default::default() };
        let overlay = PartialConfig::default();
        let mut locations = ConfigLocations::default();
        let merged = base.overwrite(&overlay, ConfigSource::Cmd, &mut locations);
        assert_eq!(merged.verbosity, Some(Verbosity::new(1).unwrap()));
        assert_eq!(locations.verbosity, ConfigSource::Default);
    }

    #[test]
    fn higher_precedence_overlay_wins_over_a_previous_source() {
        let base = PartialConfig { verbosity: Some(Verbosity::new(1).unwrap()), ..Default::default() };
        let mut locations = ConfigLocations { verbosity: ConfigSource::File, ..Default::default() };
        let overlay = PartialConfig { verbosity: Some(Verbosity::new(4).unwrap()), ..Default::default() };
        let merged = base.overwrite(&overlay, ConfigSource::Cmd, &mut locations);
        assert_eq!(merged.verbosity, Some(Verbosity::new(4).unwrap()));
        assert_eq!(locations.verbosity, ConfigSource::Cmd);
    }

    #[test]
    fn chained_overwrites_match_the_default_then_file_then_env_then_cmd_precedence() {
        let default = PartialConfig { verbosity: Some(Verbosity::new(2).unwrap()), save_pdf: Some(true), ..Default::default() };
        let file = PartialConfig { verbosity: Some(Verbosity::new(1).unwrap()), ..Default::default() };
        let env = PartialConfig::default();
        let cmd = PartialConfig { save_pdf: Some(false), ..Default::default() };

        let mut locations = ConfigLocations::default();
        let merged = default
            .overwrite(&file, ConfigSource::File, &mut locations)
            .overwrite(&env, ConfigSource::Env, &mut locations)
            .overwrite(&cmd, ConfigSource::Cmd, &mut locations);

        assert_eq!(merged.verbosity, Some(Verbosity::new(1).unwrap()));
        assert_eq!(merged.save_pdf, Some(false));
        assert_eq!(locations.verbosity, ConfigSource::File);
        assert_eq!(locations.save_pdf, ConfigSource::Cmd);
    }

    #[test]
    fn defaults_returns_the_default_config_baseline() {
        let defaults = PartialConfig::defaults();
        assert_eq!(defaults.verbosity, Some(Verbosity::new(2).unwrap()));
        assert_eq!(defaults.save_pdf, Some(true));
        assert_eq!(defaults.out_profile, Some(OutStructureMode::Regular));
        assert_eq!(defaults.target_lists, None);
    }
}
