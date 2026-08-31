//! Test d'integrazione della precedenza `cmd > env > file > default` (`PLAN.md` §11, il focus di
//! test esplicito di M9; `M9-implementation-plan.md` §3 passo 17, §4 "Integrazione").
//!
//! Un file per riga di questa matrice: ogni test tocca **un solo campo** alla volta, con le altre
//! sorgenti che lasciano quel campo intatto -- non "funziona in generale", una prova per campo che
//! l'ultima sorgente a toccarlo vince, esattamente come richiesto da `PLAN.md` §11.
//!
//! Scritto **solo contro `freeports::api`** più `cli::config_locations::cmd::CliArgs` e
//! `cli::run::resolve_configs` (non ancora in `api`, ma il solo punto d'osservazione utile per
//! ispezionare `FreeportsConfig` senza eseguire un job reale -- vedi il doc-comment di
//! `cli::run`).
//!
//! **Copertura nota come parziale, segnalata nel resoconto del test-writer**: `out_profile`/
//! `out_flags` sono testati solo su cmd/default -- `config_locations::env`/`::file` non hanno una
//! grammatica testuale definita per questi due campi in questo piano (vedi il doc-comment di
//! `config_locations::env`), quindi non c'è una precedenza `env`/`file` da verificare per loro.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use freeports::cli::config_locations::cmd::CliArgs;
use freeports::cli::run::resolve_configs;
use clap::Parser;

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
    "FREEPORTS_SAVE_PDF",
    "FREEPORTS_FORMAT",
    "FREEPORTS_CONFIG_FILE",
    "FREEPORTS_TARGET_LIST",
    "FREEPORTS_FORMATS_REPO_PATH",
    "FREEPORTS_INPUT_DB_PATH",
];

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Pulisce e restaura le `FREEPORTS_*` per la durata di un test -- ogni test di questo file (un
/// intero processo per file di `tests/`, ma `cargo test` esegue comunque i `#[test]` di **questo**
/// file in parallelo su thread dello stesso processo) usa questo guardiano.
struct EnvScope {
    _lock: std::sync::MutexGuard<'static, ()>,
    originals: Vec<(&'static str, Option<String>)>,
}

impl EnvScope {
    fn new() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let originals = ALL_FREEPORTS_VARS.iter().map(|&k| (k, std::env::var(k).ok())).collect();
        for &k in ALL_FREEPORTS_VARS {
            unsafe { std::env::remove_var(k) };
        }
        Self { _lock: lock, originals }
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

/// Un repo formati minimo con due formati dichiarati (`A-EN24`, `B-EN24`) -- sufficiente per ogni
/// test qui sotto, che non ha mai bisogno di caricare un `Algorithm` reale (solo di risolvere la
/// configurazione, `resolve_configs` non esegue alcun job).
fn write_minimal_formats_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("formats_repo");
    std::fs::create_dir_all(repo.join("metadata")).unwrap();
    std::fs::write(repo.join("metadata/formats.csv"), "Name,Locale,Year,Country,Version\nA,EN,24,,\nB,EN,24,,\n").unwrap();
    std::fs::write(repo.join("metadata/url_mapping.csv"), "Format name,Url\n").unwrap();
    repo
}

fn write_pdf(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"%PDF-1.4 fake").unwrap();
    path
}

fn write_yaml(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn parse(args: &[&str]) -> CliArgs {
    let mut full = vec!["freeports"];
    full.extend_from_slice(args);
    CliArgs::try_parse_from(full).expect("argv must parse")
}

/// Base comune a ogni test: un pdf reale, un repo formati reale, un file di configurazione YAML
/// vuoto passato esplicitamente (evita `file::find_config()` sulla cwd/tier utente/sistema reali,
/// non isolati da `EnvScope` -- stessa cautela di `cli::run::tests`).
struct Fixture {
    dir: tempfile::TempDir,
    pdf: PathBuf,
    formats_repo: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "report.pdf");
        let formats_repo = write_minimal_formats_repo(dir.path());
        Self { dir, pdf, formats_repo }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Argomenti "di base" comuni a ogni scenario che **non** riguarda `reports`/`target_lists`:
    /// formato, repo formati, lista bersaglio (fissa, `"BASE"`), e un file YAML esplicito (di
    /// default vuoto). **Deliberatamente senza `--input`**: `reports` resta vuoto per default
    /// (`cli::partial_config::defaults()`, `Some(Vec::new())`) e resta comunque una
    /// configurazione valida (nessuna regola richiede almeno un report) -- così ogni test che
    /// *non* riguarda `reports` può ignorarlo del tutto, e ogni test che *lo* riguarda
    /// (`mod reports_precedence` sotto) può impostarlo esattamente una volta, alla sorgente sotto
    /// esame, senza rischiare un doppio `--input` che classificherebbe erroneamente la
    /// precedenza (vedi la nota in `mod reports_precedence`).
    fn base_args(&self, config_file: &Path) -> Vec<String> {
        vec![
            "--format".to_string(),
            "A-EN24".to_string(),
            "--formats-directory".to_string(),
            self.formats_repo.to_str().unwrap().to_string(),
            "--target-list".to_string(),
            "BASE".to_string(),
            "--config".to_string(),
            config_file.to_str().unwrap().to_string(),
        ]
    }

    fn resolve(&self, extra_args: &[&str], config_file: &Path) -> freeports::cli::freeports_config::FreeportsConfig {
        let base = self.base_args(config_file);
        let mut all: Vec<&str> = base.iter().map(String::as_str).collect();
        all.extend_from_slice(extra_args);
        let args = parse(&all);
        let mut configs = resolve_configs(args).expect("resolve_configs must succeed for this fixture");
        assert_eq!(configs.len(), 1, "non-batch invocation must resolve to exactly one config");
        configs.remove(0)
    }

    /// Come `base_args`, ma **senza** `--target-list` -- usato solo da `mod
    /// target_lists_precedence`, che deve poter impostare quel campo esattamente una volta, alla
    /// sorgente sotto esame, senza il `"BASE"` di `base_args` a interferire.
    fn base_args_no_target_list(&self, config_file: &Path) -> Vec<String> {
        vec![
            "--input".to_string(),
            self.pdf.to_str().unwrap().to_string(),
            "--format".to_string(),
            "A-EN24".to_string(),
            "--formats-directory".to_string(),
            self.formats_repo.to_str().unwrap().to_string(),
            "--config".to_string(),
            config_file.to_str().unwrap().to_string(),
        ]
    }

    fn resolve_no_target_list(
        &self,
        extra_args: &[&str],
        config_file: &Path,
    ) -> freeports::cli::freeports_config::FreeportsConfig {
        let base = self.base_args_no_target_list(config_file);
        let mut all: Vec<&str> = base.iter().map(String::as_str).collect();
        all.extend_from_slice(extra_args);
        let args = parse(&all);
        let mut configs = resolve_configs(args).expect("resolve_configs must succeed for this fixture");
        assert_eq!(configs.len(), 1);
        configs.remove(0)
    }
}

mod verbosity_precedence {
    use super::*;
    use freeports::core::tracing_setup::Verbosity;

    #[test]
    fn default_is_warn_when_nothing_sets_it() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "");
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.verbosity, Verbosity::Warn);
    }

    #[test]
    fn file_overrides_default() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "verbosity: debug\n");
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.verbosity, Verbosity::Debug);
    }

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        scope.set("FREEPORTS_VERBOSITY", "trace");
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "verbosity: debug\n");
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.verbosity, Verbosity::Trace);
    }

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        scope.set("FREEPORTS_VERBOSITY", "trace");
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "verbosity: debug\n");
        let config = fixture.resolve(&["-q"], &config_file);
        assert_eq!(config.verbosity, Verbosity::from_verbose_and_quiet_counts(0, 1));
    }
}

// `reports`/`target_lists` are the two fields `Fixture::base_args` deliberately leaves unset (see
// its doc comment): both modules below build their own argv per test instead of layering on top
// of `base_args`'s single fixed value, so exactly one occurrence of `--input`/`--target-list`
// exists per test, at whichever tier is under examination -- a second, competing occurrence of
// the same cmd flag would make "which source wins" ambiguous to read off the assertions.

mod reports_precedence {
    use super::*;

    #[test]
    fn file_alone_sets_reports() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let file_pdf = write_pdf(fixture.path(), "from_file.pdf");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", &format!("pdf: {}\n", file_pdf.to_str().unwrap()));
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.reports.len(), 1);
        assert_eq!(config.reports[0].path, Some(file_pdf));
    }

    #[test]
    fn env_reports_overrides_file_reports() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        let env_pdf = write_pdf(fixture.path(), "from_env.pdf");
        scope.set("FREEPORTS_PDF", env_pdf.to_str().unwrap());
        let file_pdf = write_pdf(fixture.path(), "from_file.pdf");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", &format!("pdf: {}\n", file_pdf.to_str().unwrap()));

        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.reports.len(), 1);
        assert_eq!(config.reports[0].path, Some(env_pdf));
    }

    #[test]
    fn cmd_input_overrides_env_and_file_reports() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        let env_pdf = write_pdf(fixture.path(), "from_env.pdf");
        scope.set("FREEPORTS_PDF", env_pdf.to_str().unwrap());
        let file_pdf = write_pdf(fixture.path(), "from_file.pdf");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", &format!("pdf: {}\n", file_pdf.to_str().unwrap()));
        let cmd_pdf = write_pdf(fixture.path(), "from_cmd.pdf");

        // `base_args` sets no `--input` at all (see its doc comment), so this is the *only*
        // occurrence of `--input` in this argv -- no ambiguity about which one "wins" at the
        // clap-parsing level, only real cmd > env > file precedence is exercised here.
        let config = fixture.resolve(&["--input", cmd_pdf.to_str().unwrap()], &config_file);
        assert_eq!(config.reports.len(), 1);
        assert_eq!(config.reports[0].path, Some(cmd_pdf));
    }
}

mod target_lists_precedence {
    use super::*;

    #[test]
    fn file_alone_sets_target_lists() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "target_lists:\n  - FROM-FILE\n");
        let config = fixture.resolve_no_target_list(&[], &config_file);
        assert_eq!(config.target_lists, vec!["FROM-FILE".to_string()]);
    }

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_TARGET_LIST", "FROM-ENV");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "target_lists:\n  - FROM-FILE\n");
        let config = fixture.resolve_no_target_list(&[], &config_file);
        assert_eq!(config.target_lists, vec!["FROM-ENV".to_string()]);
    }

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_TARGET_LIST", "FROM-ENV");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "target_lists:\n  - FROM-FILE\n");
        // `base_args_no_target_list` sets no `--target-list` at all, so this is the only
        // occurrence in this argv.
        let config = fixture.resolve_no_target_list(&["--target-list", "FROM-CMD"], &config_file);
        assert_eq!(config.target_lists, vec!["FROM-CMD".to_string()]);
    }
}

mod format_precedence {
    use super::*;

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_FORMAT", "B-EN24");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "format: A-EN24\n");
        // Base args already set --format via cmd; to test env-over-file we must not let the base
        // cmd flag interfere, so this scenario builds its own minimal argv instead of the shared
        // `base_args` helper.
        let args = parse(&[
            "--input",
            fixture.pdf.to_str().unwrap(),
            "--formats-directory",
            fixture.formats_repo.to_str().unwrap(),
            "--target-list",
            "BASE",
            "--config",
            config_file.to_str().unwrap(),
        ]);
        let mut configs = resolve_configs(args).unwrap();
        assert_eq!(configs.remove(0).format, "B-EN24");
    }

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_FORMAT", "B-EN24");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "format: B-EN24\n");
        let config = fixture.resolve(&[], &config_file); // base_args sets --format A-EN24
        assert_eq!(config.format, "A-EN24");
    }
}

mod out_path_precedence {
    use super::*;

    #[test]
    fn file_overrides_default() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let out_dir = fixture.path().join("from-file-out");
        std::fs::create_dir_all(&out_dir).unwrap();
        let config_file =
            write_yaml(fixture.path(), "cfg.yaml", &format!("out_path: {}\n", out_dir.to_str().unwrap()));
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.out_path, out_dir);
    }

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        let env_out = fixture.path().join("from-env-out");
        std::fs::create_dir_all(&env_out).unwrap();
        scope.set("FREEPORTS_OUT_PATH", env_out.to_str().unwrap());
        let file_out = fixture.path().join("from-file-out");
        std::fs::create_dir_all(&file_out).unwrap();
        let config_file =
            write_yaml(fixture.path(), "cfg.yaml", &format!("out_path: {}\n", file_out.to_str().unwrap()));
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.out_path, env_out);
    }

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        let env_out = fixture.path().join("from-env-out");
        std::fs::create_dir_all(&env_out).unwrap();
        scope.set("FREEPORTS_OUT_PATH", env_out.to_str().unwrap());
        let file_out = fixture.path().join("from-file-out");
        std::fs::create_dir_all(&file_out).unwrap();
        let config_file =
            write_yaml(fixture.path(), "cfg.yaml", &format!("out_path: {}\n", file_out.to_str().unwrap()));
        let cmd_out = fixture.path().join("from-cmd-out");
        std::fs::create_dir_all(&cmd_out).unwrap();
        let config = fixture.resolve(&["--out", cmd_out.to_str().unwrap()], &config_file);
        assert_eq!(config.out_path, cmd_out);
    }
}

/// P5. Le tre sorgenti si sovrappongono sul default globale come su ogni altro campo, e i due
/// override per livello si sovrappongono a loro volta al default globale -- ma **per campo**, cosi'
/// che un file che fissa `pages` e un ambiente che fissa `jobs` non si cancellino a vicenda.
mod parallelism_precedence {
    use super::*;
    use freeports::cli::parallelism_config::{ParallelismConfig, Workers};

    #[test]
    fn file_overrides_default() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "n_workers: 2\n");
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(
            config.parallelism,
            ParallelismConfig { jobs: Workers::Fixed(2), pages: Workers::Fixed(2) }
        );
    }

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_N_WORKERS", "3");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "n_workers: 2\n");
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.parallelism.jobs, Workers::Fixed(3));
    }

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_N_WORKERS", "3");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "n_workers: 2\n");
        let config = fixture.resolve(&["--workers", "4"], &config_file);
        assert_eq!(
            config.parallelism,
            ParallelismConfig { jobs: Workers::Fixed(4), pages: Workers::Fixed(4) }
        );
    }

    /// Il default globale raggiunge i livelli che nessuno tocca, e si ferma davanti a quello che
    /// una qualunque sorgente ha fissato -- anche una **meno** prioritaria di quella che ha
    /// impostato il default globale.
    #[test]
    fn a_level_set_by_the_file_survives_a_global_default_set_on_the_command_line() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file =
            write_yaml(fixture.path(), "cfg.yaml", "parallelism:\n  pages: 8\n");
        let config = fixture.resolve(&["--workers", "2"], &config_file);
        assert_eq!(
            config.parallelism,
            ParallelismConfig { jobs: Workers::Fixed(2), pages: Workers::Fixed(8) }
        );
    }

    /// I due livelli si fondono **per campo**: sorgenti diverse possono fissarne uno ciascuno.
    #[test]
    fn one_level_from_the_file_and_the_other_from_the_environment() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_PARALLELISM_JOBS", "3");
        let config_file =
            write_yaml(fixture.path(), "cfg.yaml", "parallelism:\n  pages: 5\n");
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(
            config.parallelism,
            ParallelismConfig { jobs: Workers::Fixed(3), pages: Workers::Fixed(5) }
        );
    }

    /// `--pages` batte `FREEPORTS_PARALLELISM_PAGES`, che batte `parallelism.pages` del file:
    /// la stessa catena di precedenza di ogni altro campo, sul livello invece che sul globale.
    #[test]
    fn the_per_level_options_follow_the_same_precedence_chain() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_PARALLELISM_PAGES", "5");
        let config_file =
            write_yaml(fixture.path(), "cfg.yaml", "parallelism:\n  pages: 8\n");
        let from_env = fixture.resolve(&[], &config_file);
        assert_eq!(from_env.parallelism.pages, Workers::Fixed(5));
        let from_cmd = fixture.resolve(&["--pages", "6"], &config_file);
        assert_eq!(from_cmd.parallelism.pages, Workers::Fixed(6));
    }

    /// Il default quando nessuna delle tre sorgenti dice niente: `auto` a entrambi i livelli
    /// (`agent-memory/P5-implementation-plan.md` D-P5-4).
    #[test]
    fn nothing_anywhere_is_auto_at_both_levels() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "verbosity: warn\n");
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.parallelism, ParallelismConfig::default());
    }

    /// `auto` esplicito riporta un livello al comportamento automatico dopo che una sorgente meno
    /// prioritaria lo ha fissato a un numero: senza la parola, un numero in un file di sistema
    /// non sarebbe piu' annullabile.
    #[test]
    fn auto_on_the_command_line_undoes_a_number_from_the_file() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "n_workers: 2\n");
        let config = fixture.resolve(&["--workers", "auto"], &config_file);
        assert_eq!(config.parallelism, ParallelismConfig::default());
    }
}

mod save_pdf_precedence {
    use super::*;

    #[test]
    fn default_is_true() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "");
        let config = fixture.resolve(&[], &config_file);
        assert!(config.save_pdf);
    }

    #[test]
    fn file_overrides_default() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "save_pdf: false\n");
        let config = fixture.resolve(&[], &config_file);
        assert!(!config.save_pdf);
    }

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_SAVE_PDF", "true");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "save_pdf: false\n");
        let config = fixture.resolve(&[], &config_file);
        assert!(config.save_pdf);
    }

    #[test]
    fn cmd_no_download_overrides_env_and_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        scope.set("FREEPORTS_SAVE_PDF", "true");
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "save_pdf: true\n");
        let config = fixture.resolve(&["--no-download"], &config_file);
        assert!(!config.save_pdf);
    }
}

mod formats_repo_path_precedence {
    use super::*;

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        let other_repo = write_minimal_formats_repo(&fixture.path().join("other"));
        scope.set("FREEPORTS_FORMATS_REPO_PATH", other_repo.to_str().unwrap());
        let config_file =
            write_yaml(fixture.path(), "cfg.yaml", &format!("formats_repo: {}\n", other_repo.to_str().unwrap()));
        // base_args already passes --formats-directory pointing at fixture.formats_repo.
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.formats_repo_path.as_deref(), Some(fixture.formats_repo.as_path()));
    }
}

mod input_db_path_precedence {
    use super::*;

    #[test]
    fn env_overrides_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        let env_db = fixture.path().join("env_db");
        std::fs::create_dir_all(&env_db).unwrap();
        scope.set("FREEPORTS_INPUT_DB_PATH", env_db.to_str().unwrap());
        let file_db = fixture.path().join("file_db");
        std::fs::create_dir_all(&file_db).unwrap();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", &format!("db_path: {}\n", file_db.to_str().unwrap()));
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.input_db_path.as_deref(), Some(env_db.as_path()));
    }
}

mod batch_file_precedence {
    use super::*;

    #[test]
    fn cmd_overrides_env_and_file() {
        let scope = EnvScope::new();
        let fixture = Fixture::new();
        let env_batch = write_yaml(fixture.path(), "env_batch.csv", "format\nA-EN24\n");
        scope.set("FREEPORTS_BATCH_FILE", env_batch.to_str().unwrap());
        let file_batch = write_yaml(fixture.path(), "file_batch.csv", "format\nA-EN24\n");
        let config_file =
            write_yaml(fixture.path(), "cfg.yaml", &format!("batch_file: {}\n", file_batch.to_str().unwrap()));
        let cmd_batch = write_yaml(fixture.path(), "cmd_batch.csv", "format\nA-EN24\n");

        let args = parse(&[
            "--formats-directory",
            fixture.formats_repo.to_str().unwrap(),
            "--target-list",
            "BASE",
            "--config",
            config_file.to_str().unwrap(),
            "--batch",
            cmd_batch.to_str().unwrap(),
        ]);
        let configs = resolve_configs(args).unwrap();
        assert_eq!(configs.len(), 1, "the cmd batch file has a single row");
        assert_eq!(configs[0].format, "A-EN24");
    }
}

mod out_profile_and_out_flags_cmd_vs_default {
    use super::*;
    use freeports::output::routines::write::{OutFlags, OutStructureMode};

    #[test]
    fn out_profile_default_is_regular() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "");
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.out_profile, OutStructureMode::Regular);
    }

    #[test]
    fn cmd_out_profile_overrides_the_default() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "");
        let config = fixture.resolve(&["--out-profile", "structured"], &config_file);
        assert_eq!(config.out_profile, OutStructureMode::Structured);
    }

    #[test]
    fn out_flags_default_is_all_false() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "");
        let config = fixture.resolve(&[], &config_file);
        assert_eq!(config.out_flags, OutFlags::default());
    }

    #[test]
    fn cmd_archive_and_separate_out_override_the_default() {
        let _scope = EnvScope::new();
        let fixture = Fixture::new();
        let config_file = write_yaml(fixture.path(), "cfg.yaml", "");
        let config = fixture.resolve(&["--archive", "--separate-out"], &config_file);
        assert_eq!(config.out_flags, OutFlags { compressed: true, separate_out: true });
    }
}
