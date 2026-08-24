//! `execute()`: orchestrazione completa di un'invocazione della CLI (`PLAN.md` §9
//! `cli::{CliArgs, execute}`).
//!
//! `M9-implementation-plan.md` §1 (sequenza di merge esatta) §3 passo 14. Compone tutti gli altri
//! moduli di `cli` nella sequenza descritta in §1:
//!
//! 1. `cmd_partial = config_locations::cmd::load(args)`.
//! 2. `env_partial = config_locations::env::load()`.
//! 3. Prima passata (solo per scoprire `CONFIG_FILE`):
//!    `overwrite(overwrite(defaults(), env_partial, Env), cmd_partial, Cmd)`.
//! 4. `config_file_path = tmp.values.config_file.or_else(config_locations::file::find_config)`.
//! 5. `file_partial = config_locations::file::load(config_file_path)`.
//! 6. Merge reale: `defaults() <- File <- Env <- Cmd`.
//! 7. Se `merged.values.batch_file` è `Some`: una riga di batch per job, ciascuna
//!    `overwrite(merged.clone(), row_partial, Batch)`, poi validata; altrimenti valida `merged`
//!    una sola volta.
//! 8. `cli::job::run` per ciascun `FreeportsConfig`, risultati concatenati in ordine.
//! 9. `cli::output::write_results` sul totale concatenato.
//!
//! **Aggiunta del test-writer al contratto, non nella lettera del piano**: la sequenza 1-7 sopra
//! (risoluzione/merge/validazione) è esposta come funzione propria, `resolve_configs`, invece di
//! restare un dettaglio privato di `execute`. Senza un seam così, l'unico modo di osservare "cmd
//! sovrascrive env sovrascrive file sovrascrive default su ogni campo" (`PLAN.md` §11, il focus di
//! test esplicito di questa milestone) sarebbe dedurlo dagli effetti collaterali su disco di
//! `execute` -- impraticabile per la maggior parte dei tredici campi di `PartialConfig` (es.
//! `n_workers`/`out_profile` non lasciano tracce ispezionabili in un CSV). `execute` diventa un
//! sottile `resolve_configs(args)?` seguito da `job::run` per ciascuna configurazione risolta e
//! `output::write_results` sul totale concatenato -- **nessuna logica nuova**, solo un punto di
//! osservazione in più. `tests/cli_config.rs` (`M9-implementation-plan.md` §3 passo 17) usa
//! `resolve_configs` per la matrice di precedenza; questo modulo la esercita solo per la
//! propagazione degli errori (vedi sotto).
//!
//! **Nota sull'isolamento dei test in questo modulo**: sia `resolve_configs` sia `execute`
//! chiamano internamente `config_locations::env::load()` (variabili d'ambiente reali di processo)
//! e, se `--config`/`config:`/`FREEPORTS_CONFIG_FILE` non risolvono un percorso esplicito,
//! `config_locations::file::find_config()` (cerca nella cwd reale del processo di test) --
//! entrambe fonti di stato globale condiviso con l'intero processo `cargo test`. I test qui sotto
//! passano sempre un `--config` esplicito (un file YAML reale, anche vuoto) per non dipendere da
//! `find_config()`, e puliscono le variabili `FREEPORTS_*` prima di girare, stesso meccanismo di
//! `config_locations::env::tests::EnvScope`.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! #[derive(Debug, thiserror::Error)]
//! pub enum CliError {
//!     Cmd(#[from] crate::cli::config_locations::cmd::CmdConfigError),
//!     Env(#[from] crate::cli::config_locations::env::EnvConfigError),
//!     File(#[from] crate::cli::config_locations::file::FileConfigError),
//!     Batch(#[from] crate::cli::batch::BatchError),
//!     Validate(#[from] crate::cli::freeports_config::FreeportsConfigError),
//!     Job(#[from] crate::cli::job::JobError),
//!     Output(#[from] crate::cli::output::OutputError),
//! }
//!
//! /// Passi 1-7: cmd/env/prima-passata/file/merge reale/(batch -> N righe | non-batch -> 1),
//! /// **senza** eseguire alcun job né scrivere alcun output. Un solo elemento per invocazioni
//! /// non-batch; N elementi (uno per riga CSV, nell'ordine del file) per invocazioni batch.
//! pub fn resolve_configs(
//!     args: crate::cli::config_locations::cmd::CliArgs,
//! ) -> Result<Vec<crate::cli::freeports_config::FreeportsConfig>, CliError>;
//!
//! /// `resolve_configs(args)?`, poi `cli::job::run` per ciascuna configurazione risolta
//! /// (risultati concatenati in ordine), poi `cli::output::write_results` sul totale.
//! pub fn execute(args: crate::cli::config_locations::cmd::CliArgs) -> Result<(), CliError>;
//! ```

use crate::cli::batch::{self, BatchError};
use crate::cli::config_locations::cmd::{CliArgs, CmdConfigError};
use crate::cli::config_locations::env::{self, EnvConfigError};
use crate::cli::config_locations::file::{self, FileConfigError};
use crate::cli::freeports_config::{self, FreeportsConfig, FreeportsConfigError};
use crate::cli::job::{self, JobError};
use crate::cli::output::{self, OutputError};
use crate::cli::partial_config::{ConfigSource, defaults, overwrite};

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Cmd(#[from] CmdConfigError),
    #[error(transparent)]
    Env(#[from] EnvConfigError),
    #[error(transparent)]
    File(#[from] FileConfigError),
    #[error(transparent)]
    Batch(#[from] BatchError),
    #[error(transparent)]
    Validate(#[from] FreeportsConfigError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error(transparent)]
    Output(#[from] OutputError),
}

/// Passi 1-7 della sequenza di merge (`M9-implementation-plan.md` §1): cmd/env/prima-passata
/// (per scoprire `CONFIG_FILE`)/file/merge reale/(batch -> N righe | non-batch -> 1), **senza**
/// eseguire alcun job né scrivere alcun output.
pub fn resolve_configs(args: CliArgs) -> Result<Vec<FreeportsConfig>, CliError> {
    let cmd_partial = args.to_partial_config()?;
    let env_partial = env::load()?;

    // Prima passata: solo per scoprire `CONFIG_FILE`.
    let first_pass = overwrite(overwrite(defaults(), env_partial.clone(), ConfigSource::Env), cmd_partial.clone(), ConfigSource::Cmd);
    let config_file_path = first_pass.values.config_file.clone().or_else(file::find_config);
    let file_partial = file::load(config_file_path.as_deref())?;

    // Merge reale: default <- file <- env <- cmd.
    let merged = overwrite(
        overwrite(overwrite(defaults(), file_partial, ConfigSource::File), env_partial, ConfigSource::Env),
        cmd_partial,
        ConfigSource::Cmd,
    );

    match merged.values.batch_file.clone() {
        Some(batch_file) => batch::load_jobs(&batch_file)?
            .into_iter()
            .map(|row| {
                let row_merged = overwrite(merged.clone(), row, ConfigSource::Batch);
                freeports_config::validate(row_merged).map_err(CliError::from)
            })
            .collect(),
        None => Ok(vec![freeports_config::validate(merged)?]),
    }
}

/// `resolve_configs(args)?`, poi `cli::job::run` per ciascuna configurazione risolta (risultati
/// concatenati in ordine), poi `cli::output::write_results` sul totale. **Judgment call**: quando
/// più configurazioni risolvono (modalità batch), i parametri di scrittura (`out_path`/
/// `out_profile`/`out_flags`) vengono dalla **prima** configurazione risolta -- il piano non
/// specifica quale usare quando le righe di batch potessero, in linea di principio, differire
/// anche su quei campi.
pub fn execute(args: CliArgs) -> Result<(), CliError> {
    let configs = resolve_configs(args)?;
    let mut outcomes = Vec::new();
    for config in &configs {
        outcomes.extend(job::run(config)?);
    }
    if let Some(first) = configs.first() {
        output::write_results(first, &outcomes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::config_locations::cmd::CliArgs;
    use clap::Parser;
    use std::sync::Mutex;

    const ALL_FREEPORTS_VARS: &[&str] = &[
        "FREEPORTS_URL",
        "FREEPORTS_PDF",
        "FREEPORTS_REPORTS",
        "FREEPORTS_VERBOSITY",
        "FREEPORTS_N_WORKERS",
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

    /// Stesso meccanismo di `config_locations::env::tests::EnvScope`: pulisce e restaura tutte le
    /// `FREEPORTS_*` per la durata di un test, così una variabile lasciata dalla shell reale dello
    /// sviluppatore non influenza `execute`'s `env::load()` interno.
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

    fn parse(args: &[&str]) -> CliArgs {
        let mut full = vec!["freeports"];
        full.extend_from_slice(args);
        CliArgs::try_parse_from(full).expect("argv must parse")
    }

    /// Un file YAML vuoto reale: passato esplicitamente via `--config` in ogni test qui sotto per
    /// evitare che `execute` cada sul tier cwd/utente/sistema di `file::find_config()` reale (non
    /// isolato da `EnvScope`, che pulisce solo le `FREEPORTS_*` -- vedi il doc-comment del modulo).
    /// Con `--config` esplicito, `config_file_path` è sempre `Some(questo path)`, e
    /// `find_config()` non viene mai chiamato.
    fn empty_config_file(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("empty.yaml");
        std::fs::write(&path, "").unwrap();
        path
    }

    mod resolve_configs_batch_dispatch {
        use super::*;

        #[test]
        fn a_two_row_batch_file_resolves_to_two_freeports_configs_in_file_order() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf = dir.path().join("report.pdf");
            std::fs::write(&pdf, b"%PDF-1.4").unwrap();
            std::fs::create_dir_all(dir.path().join("metadata")).unwrap();
            std::fs::write(
                dir.path().join("metadata/formats.csv"),
                "Name,Locale,Year,Country,Version\nA,EN,24,,\nB,EN,24,,\n",
            )
            .unwrap();
            std::fs::write(dir.path().join("metadata/url_mapping.csv"), "Format name,Url\n").unwrap();
            let config_path = empty_config_file(dir.path());

            let batch_csv = dir.path().join("jobs.csv");
            std::fs::write(
                &batch_csv,
                format!("format,pdf\nA-EN24,{path}\nB-EN24,{path}\n", path = pdf.to_str().unwrap()),
            )
            .unwrap();

            let args = parse(&[
                "--batch",
                batch_csv.to_str().unwrap(),
                "--formats-directory",
                dir.path().to_str().unwrap(),
                "--target-list",
                "TEST",
                "--config",
                config_path.to_str().unwrap(),
            ]);
            let configs = resolve_configs(args).unwrap();
            assert_eq!(configs.len(), 2);
            assert_eq!(configs[0].format, "A-EN24");
            assert_eq!(configs[1].format, "B-EN24");
        }

        #[test]
        fn a_non_batch_invocation_resolves_to_exactly_one_config() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf = dir.path().join("report.pdf");
            std::fs::write(&pdf, b"%PDF-1.4").unwrap();
            let config_path = empty_config_file(dir.path());
            let args = parse(&[
                "--input",
                pdf.to_str().unwrap(),
                "--format",
                "F",
                "--target-list",
                "TEST",
                "--config",
                config_path.to_str().unwrap(),
            ]);
            let configs = resolve_configs(args).unwrap();
            assert_eq!(configs.len(), 1);
        }
    }

    mod error_propagation {
        use super::*;

        #[test]
        fn an_invalid_cmd_document_specifier_surfaces_as_cli_error_cmd() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let config_path = empty_config_file(dir.path());
            let args = parse(&[
                "--input",
                "a:b:c:d",
                "--target-list",
                "TEST",
                "--format",
                "F",
                "--config",
                config_path.to_str().unwrap(),
            ]);
            let result = std::panic::catch_unwind(|| execute(args));
            assert!(result.is_ok(), "must not panic");
            assert!(matches!(result.unwrap(), Err(CliError::Cmd(_))));
        }

        #[test]
        fn a_missing_target_list_surfaces_as_cli_error_validate() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf = dir.path().join("report.pdf");
            std::fs::write(&pdf, b"%PDF-1.4").unwrap();
            let config_path = empty_config_file(dir.path());
            let args = parse(&[
                "--input",
                pdf.to_str().unwrap(),
                "--format",
                "F",
                "--formats-directory",
                dir.path().to_str().unwrap(),
                "--config",
                config_path.to_str().unwrap(),
                // Deliberately no --target-list: `require_target_lists` must reject this.
            ]);
            let result = execute(args);
            assert!(matches!(result, Err(CliError::Validate(_))), "got {result:?}");
        }

        #[test]
        fn an_unknown_format_surfaces_as_cli_error_job() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();
            let pdf = dir.path().join("report.pdf");
            std::fs::write(&pdf, b"%PDF-1.4").unwrap();
            std::fs::create_dir_all(dir.path().join("metadata")).unwrap();
            std::fs::write(dir.path().join("metadata/formats.csv"), "Name,Locale,Year,Country,Version\n").unwrap();
            std::fs::write(dir.path().join("metadata/url_mapping.csv"), "Format name,Url\n").unwrap();
            let config_path = empty_config_file(dir.path());

            let args = parse(&[
                "--input",
                pdf.to_str().unwrap(),
                "--format",
                "DOES-NOT-EXIST",
                "--formats-directory",
                dir.path().to_str().unwrap(),
                "--target-list",
                "TEST",
                "--config",
                config_path.to_str().unwrap(),
            ]);
            let result = execute(args);
            assert!(matches!(result, Err(CliError::Job(_))), "got {result:?}");
        }
    }

    /// Un solo test end-to-end reale (`execute` completo: risoluzione, job, scrittura su disco).
    /// `M9-implementation-plan.md` §3 passo 17 marca `tests/cli_run_end_to_end.rs` come
    /// opzionale ("se il tempo lo consente"); questo test copre lo stesso terreno senza un file a
    /// parte, dato il tempo limitato di questa sessione -- segnalato nel resoconto del
    /// test-writer. Tocca Python (PyMuPDF), stessa nota di `cli::job::tests::python_boundary`.
    mod python_boundary {
        use super::*;
        use pyo3::prelude::*;

        #[test]
        fn a_full_non_batch_invocation_writes_the_regular_profile_csvs_to_disk() {
            let _scope = EnvScope::new();
            let dir = tempfile::tempdir().unwrap();

            let pdf_path = dir.path().join("report.pdf");
            Python::attach(|py| {
                let fitz = PyModule::import(py, "fitz")
                    .expect("PyMuPDF (fitz) must be importable: activate venv/freeports-dev, see AGENTS.md");
                let doc = fitz.call_method0("open").unwrap();
                let page = doc.call_method1("new_page", (-1i64, 200.0f64, 300.0f64)).unwrap();
                page.call_method1("insert_text", ((20.0f64, 50.0f64), "Holdings")).unwrap();
                doc.call_method1("save", (pdf_path.to_str().unwrap(),)).unwrap();
                doc.call_method0("close").unwrap();
            });

            let repo = dir.path().join("formats_repo");
            for (relative, content) in [
                ("metadata/formats.csv", "Name,Locale,Year,Country,Version\nA,EN,24,,\n"),
                ("metadata/url_mapping.csv", "Format name,Url\n"),
                (
                    "content/orchestration/algorithms_schedule.csv",
                    "Format name,Page type,Filter next iteration\nA-EN24,investments,\n",
                ),
                ("content/orchestration/mapping.csv", "ID,Page type\nA-EN24(investments),investments\n"),
                ("content/orchestration/pageclassify_overwrite.csv", "ID\n"),
                (
                    "content/algorithms/structured/page_classify/args.csv",
                    "ID,Header set,Class\nA-EN24/0,\"Arial \"\"^.*$\"\"\",investments\n",
                ),
                (
                    "content/algorithms/structured/investments/args.csv",
                    "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n\
                     A-EN24,Arial,Arial,Arial,1,,,,\n",
                ),
                (
                    "content/algorithms/structured/investments/additional_args.csv",
                    "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous\n",
                ),
                ("content/algorithms/structured/investments/partial_pipes.csv", "ID,pdf_extract,text_filter,deserialize\n"),
                ("content/algorithms/structured/investments/deselection_lists.csv", "ID,Deselection set\n"),
                ("content/algorithms/semistructured/formats_mapping.csv", "ID,pdf_extract,text_filter,deserialize\n"),
                ("content/algorithms/semistructured/args/pdf_extract.yaml", "{}"),
                ("content/algorithms/semistructured/args/text_filter.yaml", "{}"),
                ("content/algorithms/semistructured/args/deserialize.yaml", "{}"),
            ] {
                let path = repo.join(relative);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, content).unwrap();
            }

            let out_dir = dir.path().join("out");
            std::fs::create_dir_all(&out_dir).unwrap();
            let config_path = empty_config_file(dir.path());

            let args = parse(&[
                "--input",
                pdf_path.to_str().unwrap(),
                "--format",
                "A-EN24",
                "--formats-directory",
                repo.to_str().unwrap(),
                "--target-list",
                "TEST",
                "--out",
                out_dir.to_str().unwrap(),
                "--config",
                config_path.to_str().unwrap(),
            ]);

            execute(args).expect("a fully valid, self-contained invocation must succeed end to end");
            assert!(out_dir.join("investments.csv").is_file());
            assert!(out_dir.join("funds.csv").is_file());
        }
    }
}
