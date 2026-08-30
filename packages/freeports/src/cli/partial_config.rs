//! Configurazione parziale e merge fra le sorgenti (cmd > env > file > default > batch).
//!
//! `PLAN.md` §9 (implicito: `cli::{CliArgs, execute}`), `M9-implementation-plan.md` §1/§3 passo 5.
//! **Un solo `PartialConfig`**, non tre struct pydantic quasi identiche come nel riferimento
//! (`FreeportsFileConfig`/`FreeportsEnvConfig`/`FreeportsCmdConfig`): un solo tipo con tutti i
//! campi `Option<T>`, prodotto da `config_locations::{cmd,env,file}::load(...)` e da `cli::batch`
//! (una riga di CSV = un `PartialConfig`).
//!
//! Possiede anche il meccanismo condiviso di risoluzione singolare/plurale di
//! `M9-implementation-plan.md` §0 Q3: `file::load`/`env::load` chiamano entrambi
//! `resolve_singular_and_plural_reports` per combinare `url:`/`pdf:`
//! (`FREEPORTS_URL`/`FREEPORTS_PDF`) con `reports:`/`FREEPORTS_REPORTS` -- specificare **sia** la
//! forma singolare **sia** quella plurale sulla stessa sorgente è un errore di configurazione
//! esplicito (`PLAN.md` §2 principio 4: mai un override silenzioso).
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, Clone, Default, PartialEq)]
//! pub struct PartialConfig {
//!     pub verbosity: Option<crate::core::tracing_setup::Verbosity>,
//!     pub reports: Option<Vec<crate::cli::conf_parse::DocumentSpec>>,
//!     pub target_lists: Option<Vec<String>>,
//!     pub format: Option<String>,
//!     pub out_path: Option<std::path::PathBuf>,
//!     pub out_profile: Option<crate::output::routines::write::OutStructureMode>,
//!     pub out_flags: Option<crate::output::routines::write::OutFlags>,
//!     pub n_workers: Option<usize>,
//!     pub batch_file: Option<std::path::PathBuf>,
//!     pub save_pdf: Option<bool>,
//!     pub formats_repo_path: Option<std::path::PathBuf>,
//!     pub input_db_path: Option<std::path::PathBuf>,
//!     pub config_file: Option<std::path::PathBuf>,
//! }
//!
//! #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
//! pub enum ConfigSource { Default, File, Env, Cmd, Batch }
//!
//! #[derive(Debug, Clone, Default)]
//! pub struct MergedConfig {
//!     pub values: PartialConfig,
//!     pub sources: std::collections::BTreeMap<&'static str, ConfigSource>,
//! }
//!
//! **Judgment call del test-writer, segnalato nel resoconto finale**: `M9-implementation-plan.md`
//! §1/§0 Q5/Q8 fissa esplicitamente solo due valori di `defaults()` (`verbosity`/`target_lists`).
//! Perché `cli::freeports_config::validate` possa mai produrre una `FreeportsConfig` completa
//! (i cui campi non-`target_lists`/`format`/`reports` **non** sono `Option`) quando nessuna
//! sorgente reale tocca un campo, `defaults()` deve fornire un valore anche per gli altri campi
//! "strutturali" -- altrimenti `validate` non avrebbe altra scelta che un ulteriore errore
//! "campo mai impostato" non previsto da nessuna delle sei regole del piano. Scelta qui, non
//! decisa esplicitamente dal piano: `defaults()` fissa anche `out_path` alla cwd **assoluta**
//! risolta a runtime (`std::env::current_dir()`, con fallback a `PathBuf::from(".")` nel raro
//! caso in cui la cwd non sia leggibile) -- **non** il letterale `Path(".")` del riferimento
//! (`DEFAULT_CONFIG["OUT_PATH"]`): verificato con `rustc` prima di scrivere questo test che
//! `Path::new(".").parent()` in Rust è `Some("")` (percorso vuoto, `.exists() == false`), a
//! differenza di `pathlib.Path(".").parent` in Python (che resta `Path(".")`, sempre esistente) --
//! un letterale `"."` avrebbe fatto fallire `out_path_exists` per **ogni** configurazione che non
//! specifica esplicitamente `--out`, l'esatto opposto di un default utile. `out_profile: Some(Regular)`,
//! `out_flags: Some(OutFlags::default())`, `n_workers: Some(1)` (non il rilevamento automatico
//! dei core CPU del riferimento -- stessa semplificazione già segnalata in `config_locations::cmd`),
//! `save_pdf: Some(true)` (come `DEFAULT_CONFIG["SAVE_PDF"]`), `reports: Some(Vec::new())` (nessuna
//! regola del piano richiede che `reports` provenga da una sorgente esplicita, a differenza di
//! `target_lists` -- §0 Q8 è specifico di quel campo).
//!
//! /// `defaults()` è il tier più basso del merge (`M9-implementation-plan.md` §1, passo 6 della
//! /// sequenza): imposta esplicitamente `verbosity: Some(Verbosity::Warn)` (produce il
//! /// "Warn+Error di default" richiesto dall'utente quando nessun'altra sorgente tocca il campo,
//! /// §0 Q5) ma **non** imposta `target_lists` (resta `None` se nessuna sorgente lo tocca, §0
//! /// Q8 dipende da questo -- niente `Vec::new()` silenzioso).
//! pub fn defaults() -> MergedConfig;
//!
//! /// Un campo `Some` in `overlay` sovrascrive il corrispondente in `base.values` e registra
//! /// `source` in `base.sources` sotto il nome del campo; un campo `None` in `overlay` lascia
//! /// `base` (valore e provenienza) del tutto invariato.
//! pub fn overwrite(base: MergedConfig, overlay: PartialConfig, source: ConfigSource) -> MergedConfig;
//!
//! #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
//! #[error("this source sets both a singular report (url/pdf) and the plural `reports` key")]
//! pub struct SourceReportsConflict;
//!
//! /// Usato sia da `config_locations::file::load` sia da `config_locations::env::load`
//! /// (`M9-implementation-plan.md` §0 Q3): due call site reali, non un'astrazione ipotetica.
//! pub fn resolve_singular_and_plural_reports(
//!     singular: Option<crate::cli::conf_parse::DocumentSpec>,
//!     plural: Option<Vec<crate::cli::conf_parse::DocumentSpec>>,
//! ) -> Result<Option<Vec<crate::cli::conf_parse::DocumentSpec>>, SourceReportsConflict>;
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::cli::conf_parse::DocumentSpec;
use crate::core::tracing_setup::Verbosity;
use crate::output::routines::write::{OutFlags, OutStructureMode};
use crate::core::tracing_setup::log_error;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartialConfig {
    pub verbosity: Option<Verbosity>,
    pub reports: Option<Vec<DocumentSpec>>,
    pub target_lists: Option<Vec<String>>,
    pub format: Option<String>,
    pub out_path: Option<PathBuf>,
    pub out_profile: Option<OutStructureMode>,
    pub out_flags: Option<OutFlags>,
    pub n_workers: Option<usize>,
    pub batch_file: Option<PathBuf>,
    pub save_pdf: Option<bool>,
    pub formats_repo_path: Option<PathBuf>,
    pub input_db_path: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigSource {
    Default,
    File,
    Env,
    Cmd,
    Batch,
}

#[derive(Debug, Clone, Default)]
pub struct MergedConfig {
    pub values: PartialConfig,
    pub sources: BTreeMap<&'static str, ConfigSource>,
}

/// Tier più basso del merge (`M9-implementation-plan.md` §1, passo 6 della sequenza). Fissa
/// esplicitamente `verbosity: Some(Verbosity::Warn)` (produce il "Warn+Error di default" richiesto
/// dall'utente quando nessun'altra sorgente tocca il campo, §0 Q5) e **non** imposta
/// `target_lists` (resta `None` se nessuna sorgente lo tocca, §0 Q8 dipende da questo -- niente
/// `Vec::new()` silenzioso).
///
/// **Judgment call, non pinnata esplicitamente dal piano** (vedi il doc-comment del modulo): ogni
/// altro campo "strutturale" (`out_path`/`out_profile`/`out_flags`/`n_workers`/`save_pdf`/
/// `reports`) riceve comunque un default, altrimenti `cli::freeports_config::validate` non
/// avrebbe altra scelta che un errore "campo mai impostato" non previsto da nessuna delle sei
/// regole del piano. `out_path` è la cwd **assoluta** risolta a runtime, non il letterale
/// `Path(".")` del riferimento: `Path::new(".").parent()` è `Some("")` in Rust (percorso vuoto,
/// mai esistente), a differenza di `pathlib.Path(".").parent` in Python.
pub fn defaults() -> MergedConfig {
    let out_path = std::env::current_dir().unwrap_or_else(|e| {
        tracing::warn!(error = log_error(&e), "cannot read the current directory, defaulting out_path to \".\": {e}");
        PathBuf::from(".")
    });
    MergedConfig {
        values: PartialConfig {
            verbosity: Some(Verbosity::Warn),
            reports: Some(Vec::new()),
            target_lists: None,
            format: None,
            out_path: Some(out_path),
            out_profile: Some(OutStructureMode::Regular),
            out_flags: Some(OutFlags::default()),
            n_workers: Some(1),
            batch_file: None,
            save_pdf: Some(true),
            formats_repo_path: None,
            input_db_path: None,
            config_file: None,
        },
        sources: BTreeMap::new(),
    }
}

/// Un campo `Some` in `overlay` sovrascrive il corrispondente in `base.values` e registra `source`
/// in `base.sources` sotto il nome del campo; un campo `None` in `overlay` lascia `base` (valore e
/// provenienza) del tutto invariato.
pub fn overwrite(mut base: MergedConfig, overlay: PartialConfig, source: ConfigSource) -> MergedConfig {
    macro_rules! apply {
        ($field:ident) => {
            if overlay.$field.is_some() {
                base.values.$field = overlay.$field;
                base.sources.insert(stringify!($field), source);
            }
        };
    }
    apply!(verbosity);
    apply!(reports);
    apply!(target_lists);
    apply!(format);
    apply!(out_path);
    apply!(out_profile);
    apply!(out_flags);
    apply!(n_workers);
    apply!(batch_file);
    apply!(save_pdf);
    apply!(formats_repo_path);
    apply!(input_db_path);
    apply!(config_file);
    base
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("this source sets both a singular report (url/pdf) and the plural `reports` key")]
pub struct SourceReportsConflict;

/// Usato sia da `config_locations::file::load` sia da `config_locations::env::load`
/// (`M9-implementation-plan.md` §0 Q3): due call site reali, non un'astrazione ipotetica.
pub fn resolve_singular_and_plural_reports(
    singular: Option<DocumentSpec>,
    plural: Option<Vec<DocumentSpec>>,
) -> Result<Option<Vec<DocumentSpec>>, SourceReportsConflict> {
    match (singular, plural) {
        (None, None) => Ok(None),
        (Some(s), None) => Ok(Some(vec![s])),
        (None, Some(list)) => Ok(Some(list)),
        (Some(_), Some(_)) => Err(SourceReportsConflict),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::conf_parse::DocumentSpec;
    use crate::core::tracing_setup::Verbosity;
    use std::path::PathBuf;

    fn empty_partial() -> PartialConfig {
        PartialConfig::default()
    }

    fn spec(name: &str) -> DocumentSpec {
        DocumentSpec { url: None, path: None, name: Some(name.to_string()) }
    }

    mod overwrite_basics {
        use super::*;

        #[test]
        fn a_some_field_in_the_overlay_replaces_the_base_value() {
            let base = MergedConfig::default();
            let overlay = PartialConfig { out_path: Some(PathBuf::from("/tmp/out")), ..empty_partial() };
            let merged = overwrite(base, overlay, ConfigSource::File);
            assert_eq!(merged.values.out_path, Some(PathBuf::from("/tmp/out")));
        }

        #[test]
        fn a_some_field_in_the_overlay_registers_its_source() {
            let base = MergedConfig::default();
            let overlay = PartialConfig { out_path: Some(PathBuf::from("/tmp/out")), ..empty_partial() };
            let merged = overwrite(base, overlay, ConfigSource::File);
            assert_eq!(merged.sources.get("out_path"), Some(&ConfigSource::File));
        }

        #[test]
        fn a_none_field_in_the_overlay_leaves_the_base_value_untouched() {
            let mut base = MergedConfig::default();
            base.values.out_path = Some(PathBuf::from("/original"));
            base.sources.insert("out_path", ConfigSource::Default);
            let overlay = empty_partial();
            let merged = overwrite(base, overlay, ConfigSource::Cmd);
            assert_eq!(merged.values.out_path, Some(PathBuf::from("/original")));
        }

        #[test]
        fn a_none_field_in_the_overlay_does_not_change_the_recorded_source() {
            let mut base = MergedConfig::default();
            base.values.out_path = Some(PathBuf::from("/original"));
            base.sources.insert("out_path", ConfigSource::Default);
            let merged = overwrite(base, empty_partial(), ConfigSource::Cmd);
            assert_eq!(merged.sources.get("out_path"), Some(&ConfigSource::Default));
        }

        #[test]
        fn unrelated_fields_are_unaffected_by_an_overlay_touching_a_different_field() {
            let mut base = MergedConfig::default();
            base.values.save_pdf = Some(true);
            base.sources.insert("save_pdf", ConfigSource::Default);
            let overlay = PartialConfig { out_path: Some(PathBuf::from("/tmp/out")), ..empty_partial() };
            let merged = overwrite(base, overlay, ConfigSource::File);
            assert_eq!(merged.values.save_pdf, Some(true));
            assert_eq!(merged.sources.get("save_pdf"), Some(&ConfigSource::Default));
        }
    }

    mod overwrite_chain {
        use super::*;

        #[test]
        fn the_last_source_to_touch_a_field_wins_the_value() {
            let base = MergedConfig::default();
            let file = PartialConfig { verbosity: Some(Verbosity::Trace), ..empty_partial() };
            let env = PartialConfig { verbosity: Some(Verbosity::Debug), ..empty_partial() };
            let cmd = PartialConfig { verbosity: Some(Verbosity::Info), ..empty_partial() };

            let merged = overwrite(overwrite(overwrite(base, file, ConfigSource::File), env, ConfigSource::Env), cmd, ConfigSource::Cmd);
            assert_eq!(merged.values.verbosity, Some(Verbosity::Info));
        }

        #[test]
        fn the_last_source_to_touch_a_field_wins_the_recorded_source() {
            let base = MergedConfig::default();
            let file = PartialConfig { verbosity: Some(Verbosity::Trace), ..empty_partial() };
            let env = PartialConfig { verbosity: Some(Verbosity::Debug), ..empty_partial() };
            let cmd = PartialConfig { verbosity: Some(Verbosity::Info), ..empty_partial() };

            let merged = overwrite(overwrite(overwrite(base, file, ConfigSource::File), env, ConfigSource::Env), cmd, ConfigSource::Cmd);
            assert_eq!(merged.sources.get("verbosity"), Some(&ConfigSource::Cmd));
        }

        #[test]
        fn a_middle_source_leaving_a_field_untouched_does_not_break_the_chain() {
            // file sets verbosity, env leaves it alone, cmd overrides again: env's `None` must
            // not erase file's contribution before cmd is applied.
            let base = MergedConfig::default();
            let file = PartialConfig { verbosity: Some(Verbosity::Trace), ..empty_partial() };
            let env = empty_partial(); // does not touch verbosity
            let cmd = PartialConfig { verbosity: Some(Verbosity::Info), ..empty_partial() };

            let merged = overwrite(overwrite(overwrite(base, file, ConfigSource::File), env, ConfigSource::Env), cmd, ConfigSource::Cmd);
            assert_eq!(merged.values.verbosity, Some(Verbosity::Info));
        }

        #[test]
        fn a_field_never_touched_by_any_source_keeps_the_starting_default() {
            let base = defaults();
            let file = empty_partial();
            let env = empty_partial();
            let cmd = empty_partial();
            let merged = overwrite(overwrite(overwrite(base, file, ConfigSource::File), env, ConfigSource::Env), cmd, ConfigSource::Cmd);
            assert_eq!(merged.values.verbosity, Some(Verbosity::Warn), "must keep defaults()'s Warn, untouched by any source");
        }
    }

    mod defaults {
        use super::*;

        #[test]
        fn verbosity_defaults_to_warn_not_none() {
            assert_eq!(defaults().values.verbosity, Some(Verbosity::Warn));
        }

        #[test]
        fn target_lists_has_no_default_stays_none() {
            // §0 Q8: no silent `Vec::new()` default -- absence must be observable so
            // `freeports_config::validate`'s `require_target_lists` rule can fire.
            assert_eq!(defaults().values.target_lists, None);
        }

        #[test]
        fn out_path_defaults_to_the_absolute_current_directory_not_the_literal_dot() {
            // See the module doc's judgment-call note: a literal "." would make
            // `out_path_exists` reject every default configuration (`Path::new(".").parent()` is
            // `Some("")` in Rust, and `"".exists()` is `false`).
            let expected = std::env::current_dir().expect("test process must have a readable cwd");
            assert_eq!(defaults().values.out_path, Some(expected));
        }

        #[test]
        fn out_profile_defaults_to_regular() {
            assert_eq!(
                defaults().values.out_profile,
                Some(crate::output::routines::write::OutStructureMode::Regular)
            );
        }

        #[test]
        fn out_flags_defaults_to_the_all_false_default() {
            assert_eq!(
                defaults().values.out_flags,
                Some(crate::output::routines::write::OutFlags::default())
            );
        }

        #[test]
        fn n_workers_defaults_to_one() {
            assert_eq!(defaults().values.n_workers, Some(1));
        }

        #[test]
        fn save_pdf_defaults_to_true() {
            assert_eq!(defaults().values.save_pdf, Some(true));
        }

        #[test]
        fn reports_defaults_to_an_empty_list_not_none() {
            // Unlike `target_lists` (§0 Q8), no rule requires `reports` to come from an explicit
            // source -- a config with zero reports is a degenerate but valid starting point.
            assert_eq!(defaults().values.reports, Some(vec![]));
        }

        #[test]
        fn defaults_records_no_sources_for_any_field() {
            // `defaults()` is the starting tier, before any real source has been applied --
            // nothing should appear in `sources` yet (a field being `Some` in `defaults()`, like
            // `verbosity`, is not the same as a *source* having set it).
            assert!(defaults().sources.is_empty());
        }
    }

    mod resolve_singular_and_plural_reports {
        use super::*;

        #[test]
        fn neither_present_is_none() {
            assert_eq!(super::resolve_singular_and_plural_reports(None, None), Ok(None));
        }

        #[test]
        fn only_singular_becomes_a_one_element_list() {
            let s = spec("a");
            assert_eq!(
                super::resolve_singular_and_plural_reports(Some(s.clone()), None),
                Ok(Some(vec![s]))
            );
        }

        #[test]
        fn only_plural_is_passed_through_unchanged() {
            let list = vec![spec("a"), spec("b")];
            assert_eq!(
                super::resolve_singular_and_plural_reports(None, Some(list.clone())),
                Ok(Some(list))
            );
        }

        #[test]
        fn both_present_is_a_conflict_error() {
            let result = super::resolve_singular_and_plural_reports(Some(spec("a")), Some(vec![spec("b")]));
            assert_eq!(result, Err(SourceReportsConflict));
        }

        #[test]
        fn both_present_with_an_empty_plural_list_is_still_a_conflict() {
            // An empty `reports: []` alongside a singular `url:`/`pdf:` is still "both forms
            // used on the same source" -- the conflict is about which *keys* were set, not
            // whether the plural list ended up non-empty.
            let result = super::resolve_singular_and_plural_reports(Some(spec("a")), Some(vec![]));
            assert_eq!(result, Err(SourceReportsConflict));
        }
    }

    mod merged_config_default {
        use super::*;

        #[test]
        fn a_fresh_default_merged_config_has_no_sources_recorded() {
            assert!(MergedConfig::default().sources.is_empty());
        }

        #[test]
        fn a_fresh_default_merged_config_has_no_values_set() {
            assert_eq!(MergedConfig::default().values, empty_partial());
        }
    }
}
