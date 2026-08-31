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
//! `parallelism`/`out_profile` non lasciano tracce ispezionabili in un CSV). `execute` diventa un
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
use crate::cli::parallelism_config::ParallelismConfig;
use crate::cli::partial_config::{ConfigSource, defaults, overwrite};
use crate::cli::worker::{self, JobFailure, WorkerError};
use crate::core::algorithm::DocumentOutcome;
use crate::core::parallelism::{self, Parallelism};
use crate::core::tracing_setup::{LogHandle, TracingSetupError};

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
    /// Il registro `.log.csv` non ha potuto prendere posto accanto agli output (cartella non
    /// creabile, file non apribile). Non e' un errore del job, ma non va nemmeno ingoiato: senza
    /// registro l'utente perde le diagnostiche localizzate proprio della corsa che sta lanciando.
    #[error(transparent)]
    Logging(#[from] TracingSetupError),
    /// Un job eseguito in un processo figlio non ha prodotto risultati (P1). Trasparente di
    /// proposito: per un fallimento di dominio il messaggio deve essere **identico** a quello che
    /// il percorso sequenziale avrebbe stampato.
    #[error(transparent)]
    Worker(#[from] JobFailure),
    /// L'infrastruttura dei processi figli non e' partita: area di lavoro non creabile, eseguibile
    /// non identificabile. Non e' un job fallito -- nessun job e' mai partito.
    #[error("cannot set up the worker processes: {source}")]
    WorkerSetup {
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    WorkerRequest(#[from] WorkerError),
}

/// Config resolution entry point (`M9-implementation-plan.md` §1): passi 1-7 della sequenza di
/// merge -- cmd/env/prima-passata (per scoprire `CONFIG_FILE`)/file/merge reale/(batch -> N righe
/// | non-batch -> 1), **senza** eseguire alcun job né scrivere alcun output. Opens its own span:
/// each of `to_partial_config`/`env::load`/`file::load`/`batch::load_jobs`/`freeports_config::
/// validate` already logs its own failure at the point the specific typed error is constructed, so
/// this function does not re-log a propagated error, only the resolution steps genuinely local to
/// it (which config file ends up in effect, whether batch mode applies, how many jobs came out).
pub fn resolve_configs(args: CliArgs) -> Result<Vec<FreeportsConfig>, CliError> {
    let span = tracing::info_span!("resolve_config");
    let _guard = span.enter();

    let cmd_partial = args.to_partial_config()?;
    let env_partial = env::load()?;

    // Prima passata: solo per scoprire `CONFIG_FILE`.
    let first_pass = overwrite(overwrite(defaults(), env_partial.clone(), ConfigSource::Env), cmd_partial.clone(), ConfigSource::Cmd);
    // No log in the common (`None`) branch: `file::find_config` already logs, more specifically,
    // whether/where it found a configuration file -- logging again here would just repeat it.
    // Only the override branch below adds information `find_config` never gets a chance to log
    // (it isn't even called).
    let config_file_path = match first_pass.values.config_file.clone() {
        Some(path) => {
            tracing::debug!(config_file = %path.display(), "configuration file location set via cmd/env, skipping the search tiers");
            Some(path)
        }
        None => file::find_config(),
    };
    let file_partial = file::load(config_file_path.as_deref())?;

    // Merge reale: default <- file <- env <- cmd.
    let merged = overwrite(
        overwrite(overwrite(defaults(), file_partial, ConfigSource::File), env_partial, ConfigSource::Env),
        cmd_partial,
        ConfigSource::Cmd,
    );

    let configs = match merged.values.batch_file.clone() {
        Some(batch_file) => {
            tracing::info!(batch_file = %batch_file.display(), "resolving batch configuration");
            batch::load_jobs(&batch_file)?
                .into_iter()
                .map(|row| {
                    let row_merged = overwrite(merged.clone(), row, ConfigSource::Batch);
                    freeports_config::validate(row_merged).map_err(CliError::from)
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        None => vec![freeports_config::validate(merged)?],
    };
    tracing::debug!(job_count = configs.len(), "resolved job configuration(s)");
    Ok(configs)
}

/// `resolve_configs(args)?`, poi `cli::job::run` per ciascuna configurazione risolta (risultati
/// concatenati in ordine), poi `cli::output::write_results` sul totale. **Judgment call**: quando
/// più configurazioni risolvono (modalità batch), i parametri di scrittura (`out_path`/
/// `out_profile`/`out_flags`) vengono dalla **prima** configurazione risolta -- il piano non
/// specifica quale usare quando le righe di batch potessero, in linea di principio, differire
/// anche su quei campi.
///
/// Opens the outermost `run` span (`PLAN.md` §3's `Activity` root) so that every nested span
/// opened downstream (`resolve_config`, `job`, `document`, ...) -- and any error each of them logs
/// at its own boundary -- carries `run/...` context. No error is re-logged here: each of
/// `resolve_configs`/`job::run` already logs its own failure once, closest to where it happens.
pub fn execute(args: CliArgs, log_handle: &LogHandle) -> Result<(), CliError> {
    let span = tracing::info_span!("run");
    let _guard = span.enter();

    let configs = resolve_configs(args)?;
    // Il primo momento in cui si sa dove vanno gli output, e quindi dove va `.log.csv`: prima di
    // questa riga il registro non ha ancora una destinazione (`CsvLogLayer::deferred`), e le righe
    // gia' prodotte dalla risoluzione della configurazione sono in memoria. Stessa scelta della
    // prima configurazione risolta che governa gia' i parametri di scrittura in modalita' batch.
    if let Some(first) = configs.first() {
        log_handle.set_csv_dir(&output::log_csv_dir(first)).map_err(CliError::from)?;
    }
    let outcomes = run_jobs(&configs, log_handle)?;
    if let Some(first) = configs.first() {
        output::write_results(first, &outcomes)?;
    }
    Ok(())
}

/// La sezione `parallelism` che governa questa corsa (P5).
///
/// E' quella della **prima** configurazione risolta, la stessa che governa gia' i parametri di
/// scrittura in modalita' batch: i due livelli sono proprieta' della corsa intera, non di un job,
/// e le colonne di un file di batch non possono comunque impostarli.
fn run_parallelism(configs: &[FreeportsConfig]) -> ParallelismConfig {
    configs.first().map_or_else(ParallelismConfig::default, |first| first.parallelism)
}

/// Quanti job alla volta (P1) e quante pagine alla volta dentro ciascuno (P2), risolti insieme.
///
/// **Insieme e in quest'ordine** perche' il secondo dipende dal primo: in `auto` il budget di core
/// si divide fra i job concorrenti, cosi' che un batch con quattro job su venti thread hardware ne
/// usi cinque per job invece di venti. Con un job solo -- il caso non-batch, che e' anche quello in
/// cui P1 non fa nulla -- restano tutti disponibili, ed e' li' che P2 conta davvero.
///
/// Un `pages` **esplicito** non si divide: chi lo scrive lo ha chiesto. In quel caso il prodotto
/// `jobs x pages` puo' superare i core della macchina, e allora si onora la richiesta e la si
/// segnala -- `PLAN.md` §2 principio 4 vieta gli override silenziosi, non le configurazioni
/// scomode.
fn resolve_parallelism(configs: &[FreeportsConfig]) -> (usize, Parallelism) {
    let requested = run_parallelism(configs);
    let jobs = requested.resolve_jobs(configs.len());
    let pages = requested.resolve_pages(jobs);
    if let Some(total) = ParallelismConfig::oversubscription(jobs, pages) {
        tracing::warn!(
            jobs,
            pages = pages.pages,
            total,
            available = parallelism::available_threads(),
            "the requested parallelism opens {total} workers on a machine with {} cores",
            parallelism::available_threads()
        );
    }
    tracing::debug!(
        requested_jobs = %requested.jobs,
        requested_pages = %requested.pages,
        jobs,
        pages = pages.pages,
        "parallelism resolved"
    );
    (jobs, pages)
}

/// Esegue i job risolti e ne concatena i risultati in ordine.
///
/// Un job solo, o `n_workers` a 1, restano **esattamente** sul `for` sequenziale di sempre: nessun
/// processo, nessuna area di lavoro temporanea, nessuna differenza osservabile. E' il default, ed e'
/// la ragione per cui chi non chiede nulla non vede cambiare niente.
fn run_jobs(configs: &[FreeportsConfig], log_handle: &LogHandle) -> Result<Vec<DocumentOutcome>, CliError> {
    let (jobs, pages) = resolve_parallelism(configs);
    if jobs <= 1 {
        let mut outcomes = Vec::new();
        for config in configs {
            outcomes.extend(job::run(config, pages)?);
        }
        return Ok(outcomes);
    }
    run_jobs_in_processes(configs, jobs, pages, log_handle)
}

/// I job in processi figli (P1, `agent-memory/P1-implementation-plan.md` §2).
///
/// E' l'unico livello di parallelismo che scavalca il GIL, ed e' l'unica ragione per cui vale la
/// pena pagare un confine di processo: P0 ha misurato che il caricamento PyMuPDF, che nessun thread
/// puo' accelerare, e' il 35-75% del tempo di un job.
///
/// **Attenzione a `current_exe` sotto `cargo test`**: li' restituisce il binario della suite, non
/// `freeports`. Un test che innescasse questo ramo lancerebbe copie del binario di test, che non
/// conoscono `--internal-worker` e uscirebbero con un codice non-zero -- un `WorkerError::Died`
/// pulito, non un ciclo infinito, ma comunque un test che non prova cio' che crede. Il pool vero si
/// esercita dai test d'integrazione, con `env!("CARGO_BIN_EXE_freeports")`.
fn run_jobs_in_processes(
    configs: &[FreeportsConfig],
    parallelism: usize,
    page_workers: Parallelism,
    log_handle: &LogHandle,
) -> Result<Vec<DocumentOutcome>, CliError> {
    let executable = std::env::current_exe().map_err(|source| CliError::WorkerSetup { source })?;
    // Un'area per corsa, che sparisce da sola: i file privati dei figli non sopravvivono al padre
    // e non compaiono mai accanto ai risultati.
    let work_area = worker::WorkArea::create()?;
    let requests = configs
        .iter()
        .enumerate()
        .map(|(index, config)| {
            worker::prepare_request(work_area.path(), index, config, page_workers.pages)
        })
        .collect::<Result<Vec<_>, _>>()?;

    tracing::info!(
        job_count = requests.len(),
        parallelism,
        pages = page_workers.pages,
        "running jobs in worker processes"
    );
    let reports = worker::run_in_processes(&executable, &requests, parallelism);

    // Prima di leggere gli esiti, e comunque siano andati: l'area di lavoro sparisce all'uscita da
    // questa funzione, e i log di un job **fallito** sono i piu' utili di tutti da conservare. Il
    // padre li riversera' nei propri file alla chiusura, in ordine di job.
    for request in &requests {
        log_handle.absorb_worker_logs(&request.log_dir)?;
    }

    Ok(worker::collect(reports)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un `LogHandle` usa e getta per i test di `execute`, con entrambe le destinazioni in una
    /// tempdir che sparisce a fine test. `execute` chiama `set_csv_dir` sulla cartella di output
    /// risolta: senza un handle vero non lo si potrebbe esercitare, e con uno vero non si sporca
    /// mai la cwd della suite.
    fn test_log_handle() -> crate::core::tracing_setup::LogHandle {
        let dir = tempfile::tempdir().expect("tempdir");
        let handle = crate::core::tracing_setup::log_handle_for_tests(dir.path())
            .expect("test log handle");
        // La tempdir viene lasciata in vita per tutta la durata del processo di test: `execute`
        // riapre comunque il csv nella cartella di output vera, e tenerla viva evita che la
        // destinazione di fallback sparisca sotto i piedi di `close()`.
        std::mem::forget(dir);
        handle
    }
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

    /// P1/P5: quale dei due percorsi -- il `for` sequenziale di sempre o il pool di processi figli
    /// -- prende una corsa, e perche'. La decisione e' tutta in `resolve_parallelism`, quindi si
    /// prova li' dove non serve avviare nulla; il pool vero gira nei test d'integrazione.
    mod parallelism_decides_the_path {
        use super::*;
        use crate::cli::conf_parse::DocumentSpec;
        use crate::cli::parallelism_config::Workers;
        use crate::core::tracing_setup::Verbosity;
        use crate::output::routines::write::{OutFlags, OutStructureMode};
        use std::path::PathBuf;

        fn config_with(parallelism: ParallelismConfig) -> FreeportsConfig {
            FreeportsConfig {
                verbosity: Verbosity::Warn,
                reports: vec![DocumentSpec { url: None, path: Some(PathBuf::from("/tmp/a.pdf")), name: Some("a".to_string()) }],
                target_lists: vec!["TEST".to_string()],
                format: "FMT".to_string(),
                out_path: PathBuf::from("/tmp/out"),
                out_profile: OutStructureMode::Regular,
                out_flags: OutFlags::default(),
                parallelism,
                batch_file: None,
                save_pdf: false,
                formats_repo_path: None,
                input_db_path: None,
                config_file: None,
            }
        }

        fn batch_of(count: usize, jobs: usize) -> Vec<FreeportsConfig> {
            let parallelism =
                ParallelismConfig { jobs: Workers::Fixed(jobs), pages: Workers::Fixed(1) };
            (0..count).map(|_| config_with(parallelism)).collect()
        }

        fn jobs_of(configs: &[FreeportsConfig]) -> usize {
            resolve_parallelism(configs).0
        }

        /// `jobs: 1` -- che dopo P5 si ottiene con un `-j 1`, o con `parallelism.jobs: 1` --
        /// continua a percorrere esattamente il codice di prima di P1.
        #[test]
        fn one_job_worker_keeps_a_batch_sequential() {
            assert_eq!(jobs_of(&batch_of(8, 1)), 1);
        }

        #[test]
        fn a_single_job_is_sequential_however_many_workers_were_asked_for() {
            assert_eq!(jobs_of(&batch_of(1, 16)), 1);
        }

        /// P5: il default e' cambiato. Prima di P5 `n_workers` valeva `1` e un batch girava
        /// sequenziale se nessuno chiedeva altro; ora entrambi i livelli valgono `auto`, quindi un
        /// batch usa i core della macchina senza che l'utente debba saperlo
        /// (`agent-memory/P5-implementation-plan.md` D-P5-4).
        #[test]
        fn the_default_now_runs_a_batch_in_parallel() {
            let configs: Vec<FreeportsConfig> =
                (0..8).map(|_| config_with(ParallelismConfig::default())).collect();
            let expected = parallelism::available_threads().min(8);
            assert_eq!(jobs_of(&configs), expected);
        }

        /// L'altra meta' dello stesso default: le pagine di un job. Con un job solo il budget non
        /// si divide con nessuno, ed e' li' che P2 conta davvero.
        #[test]
        fn the_default_gives_a_lone_job_every_core_for_its_pages() {
            let configs = vec![config_with(ParallelismConfig::default())];
            let (jobs, pages) = resolve_parallelism(&configs);
            assert_eq!(jobs, 1);
            assert_eq!(pages.pages, parallelism::available_threads());
        }

        /// L'invariante di `PLAN.md` §6: `1` ovunque percorre il codice sequenziale a entrambi i
        /// livelli, ed e' il modo con cui si verifica il determinismo.
        #[test]
        fn one_everywhere_is_sequential_at_both_levels() {
            let configs: Vec<FreeportsConfig> =
                (0..8).map(|_| config_with(ParallelismConfig::SEQUENTIAL)).collect();
            assert_eq!(resolve_parallelism(&configs), (1, Parallelism::SEQUENTIAL));
        }

        /// P5 D-P5-3: un `pages` esplicito non si divide fra i job concorrenti. Il prodotto puo'
        /// superare i core -- la richiesta si onora, e `resolve_parallelism` la segnala.
        #[test]
        fn an_explicit_page_count_is_not_divided_among_the_jobs() {
            let parallelism =
                ParallelismConfig { jobs: Workers::Fixed(2), pages: Workers::Fixed(7) };
            let configs: Vec<FreeportsConfig> =
                (0..4).map(|_| config_with(parallelism)).collect();
            assert_eq!(resolve_parallelism(&configs), (2, Parallelism::pages(7)));
        }

        /// Non ha senso avviare piu' processi che job: sarebbero figli che nascono per non fare
        /// nulla, ciascuno con il costo di un interprete Python da inizializzare.
        #[test]
        fn more_workers_than_jobs_are_capped_at_the_number_of_jobs() {
            assert_eq!(jobs_of(&batch_of(3, 16)), 3);
        }

        #[test]
        fn fewer_workers_than_jobs_are_taken_as_they_are() {
            assert_eq!(jobs_of(&batch_of(16, 4)), 4);
        }

        /// Un batch vuoto e' un file di batch con la sola intestazione: legittimo, e non deve
        /// diventare uno `0` che si propaga in un `min` o in un ciclo di thread. Un solo
        /// lavoratore per zero job non ne avvia comunque nessuno -- `run_jobs` prende il ramo
        /// sequenziale, che itera su niente.
        #[test]
        fn an_empty_batch_never_asks_for_more_than_one_worker() {
            assert_eq!(jobs_of(&[]), 1);
        }

        /// Le colonne di un file di batch non possono impostare il parallelismo: il valore e'
        /// quello della prima configurazione risolta, la stessa che governa gia' i parametri di
        /// scrittura.
        #[test]
        fn the_value_comes_from_the_first_resolved_configuration() {
            let mut configs = batch_of(4, 3);
            configs[1].parallelism.jobs = Workers::Fixed(99);
            assert_eq!(jobs_of(&configs), 3);
        }
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
            let handle = test_log_handle();
            let result = std::panic::catch_unwind(|| execute(args, &handle));
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
            let result = execute(args, &test_log_handle());
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
                // `--out` non e' decorativo: e' l'unico test di questo modulo che arriva *oltre*
                // la risoluzione della configurazione, quindi l'unico in cui `execute` chiama
                // `set_csv_dir`. Senza, `out_path` prende il suo default -- la cwd, che per un
                // binario di test e' la radice del package -- e la suite lascia un `.log.csv`
                // di sola intestazione dentro `packages/freeports/` a ogni `cargo test`.
                "--out",
                dir.path().join("out").to_str().unwrap(),
            ]);
            let result = execute(args, &test_log_handle());
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

            execute(args, &test_log_handle())
                .expect("a fully valid, self-contained invocation must succeed end to end");
            assert!(out_dir.join("investments.csv").is_file());
            assert!(out_dir.join("funds.csv").is_file());
        }
    }
}
