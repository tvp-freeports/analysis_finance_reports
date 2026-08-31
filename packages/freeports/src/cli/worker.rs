//! Il protocollo fra il processo padre e un processo figlio che esegue **un** job (P1).
//!
//! `agent-memory/P1-implementation-plan.md` §2. In modalita' batch `cli::run::execute` puo'
//! eseguire i job in processi figli invece che in un `for` sequenziale. Questo modulo definisce
//! *cosa* si scambiano, e nient'altro: non avvia processi (lo fa `cli::run`) e non esegue job (lo
//! fa `cli::job`).
//!
//! # Perche' due file e non una pipe
//!
//! Lo stdout di un figlio non e' un canale pulito: PyMuPDF e i pipe Python d'autore possono
//! scriverci quando vogliono. Un file dedicato per direzione non ha questo problema, non ha il
//! limite di dimensione di una pipe, e sopravvive al figlio abbastanza da poter essere letto anche
//! quando il figlio e' gia' uscito.
//!
//! # Le due direzioni
//!
//! - **andata** ([`WorkerRequest`]): la configurazione **gia' risolta e validata** del job, piu' i
//!   due percorsi che il figlio deve usare. Il figlio non rifa' la risoluzione: se la rifacesse,
//!   leggerebbe di nuovo l'ambiente e i file di configurazione, e potrebbe risolvere qualcosa di
//!   diverso da cio' che il padre ha deciso.
//! - **ritorno** ([`WorkerReport`]): i risultati del job, oppure la sua diagnosi d'errore.
//!
//! # Un job fallito non e' un figlio fallito
//!
//! Sono due piani distinti, e confonderli renderebbe indistinguibile "il PDF non esiste" da "il
//! processo figlio e' morto di segnale". Un job che fallisce per un motivo di dominio produce un
//! [`WorkerReport::Failed`] e il figlio esce **con codice 0**: l'errore e' *nel payload*. Il codice
//! d'uscita non-zero resta riservato ai fallimenti di protocollo, che il padre riconosce
//! dall'assenza o dall'illeggibilita' del file di ritorno.
//!
//! # Cosa si perde attraversando il confine
//!
//! L'errore di dominio arriva al padre come [`ErrorRecord`] — forma `Debug`, forma `Display` e
//! catena di `source()` — non come `JobError` tipizzato: un enum d'errore non si ricostruisce da
//! una stringa. E' abbastanza perche' il messaggio su stderr sia **identico** a quello del caso
//! sequenziale, che usa la sola forma `Display`. La diagnosi completa non e' comunque perduta: il
//! figlio l'ha gia' registrata nei propri file di log, che il padre unisce ai suoi.

use std::path::{Path, PathBuf};

use crate::cli::freeports_config::FreeportsConfig;
use crate::core::algorithm::DocumentOutcome;
use crate::core::tracing_setup::ErrorRecord;

/// Cosa il padre chiede a un figlio: un job, e i due posti dove metterne gli esiti.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorkerRequest {
    /// La configurazione gia' risolta e validata dal padre.
    pub config: FreeportsConfig,
    /// Dove il figlio scrive il proprio [`WorkerReport`].
    pub report_path: PathBuf,
    /// La cartella **privata** in cui il figlio scrive i propri log. Mai la cartella di output del
    /// padre: i file di un figlio non devono comparire accanto ai risultati della corsa.
    pub log_dir: PathBuf,
    /// Quante pagine alla volta il figlio puo' elaborare (P2).
    ///
    /// Lo decide il padre e non il figlio: e' l'unico dei due che sa quanti job stanno girando
    /// insieme, e quindi l'unico che puo' evitare che N figli aprano N x n_cpu thread. Viaggia
    /// nella richiesta e non in una variabile d'ambiente per la stessa ragione per cui ci viaggia
    /// la configurazione risolta -- il figlio non deve ri-derivare nulla di cio' che il padre ha
    /// gia' deciso.
    pub page_workers: usize,
}

/// Cosa il figlio rimanda al padre.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum WorkerReport {
    /// Il job e' andato a buon fine. Il `Vec` puo' essere vuoto: un job che non estrae nulla e'
    /// un esito legittimo, non un errore.
    Succeeded { documents: Vec<DocumentOutcome> },
    /// Il job e' fallito per un motivo di dominio.
    Failed { error: ErrorRecord },
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("cannot write the worker request to {}: {source}", path.display())]
    WriteRequest {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot read the worker request at {}: {source}", path.display())]
    ReadRequest {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the worker request at {} is malformed: {source}", path.display())]
    ParseRequest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("cannot write the worker report to {}: {source}", path.display())]
    WriteReport {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot read the worker report at {}: {source}", path.display())]
    ReadReport {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the worker report at {} is malformed: {source}", path.display())]
    ParseReport {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("cannot create the worker work area at {}: {source}", path.display())]
    WorkArea {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot start a worker process for job {index}: {source}")]
    Spawn {
        index: usize,
        #[source]
        source: std::io::Error,
    },
    /// Il figlio e' uscito con un codice non-zero, o e' morto di segnale. Un job fallito per un
    /// motivo di dominio **non** passa di qui: esce con 0 e mette l'errore nel referto.
    #[error("the worker process for job {index} {status} without leaving a report")]
    Died { index: usize, status: String },
    /// Il figlio non e' riuscito ad aprire i propri file di log nella cartella privata che il padre
    /// gli ha assegnato. E' un fallimento di protocollo, non di dominio: senza log il figlio
    /// eseguirebbe il job in silenzio, e le sue diagnostiche non arriverebbero mai al registro
    /// unito della corsa.
    #[error(transparent)]
    Logging(#[from] crate::core::tracing_setup::TracingSetupError),
}

/// Serializza `request` in `path`. Il JSON e' compatto: nessuno lo legge a mano, e su un batch
/// grande sono N file in piu' da scrivere.
pub fn write_request(path: &Path, request: &WorkerRequest) -> Result<(), WorkerError> {
    let json = serde_json::to_vec(request).map_err(|e| WorkerError::WriteRequest {
        path: path.to_path_buf(),
        source: std::io::Error::other(e),
    })?;
    std::fs::write(path, json).map_err(|e| WorkerError::WriteRequest { path: path.to_path_buf(), source: e })
}

/// Rilegge una [`WorkerRequest`]. I due modi di fallire sono tenuti distinti — file assente contro
/// file illeggibile come JSON — perche' indicano bug diversi: il primo un problema di percorsi o di
/// pulizia anticipata della cartella temporanea, il secondo una versione del binario diversa fra
/// padre e figlio.
pub fn read_request(path: &Path) -> Result<WorkerRequest, WorkerError> {
    let bytes = std::fs::read(path).map_err(|e| WorkerError::ReadRequest { path: path.to_path_buf(), source: e })?;
    serde_json::from_slice(&bytes).map_err(|e| WorkerError::ParseRequest { path: path.to_path_buf(), source: e })
}

/// Serializza `report` in `path`.
pub fn write_report(path: &Path, report: &WorkerReport) -> Result<(), WorkerError> {
    let json = serde_json::to_vec(report).map_err(|e| WorkerError::WriteReport {
        path: path.to_path_buf(),
        source: std::io::Error::other(e),
    })?;
    std::fs::write(path, json).map_err(|e| WorkerError::WriteReport { path: path.to_path_buf(), source: e })
}

/// Rilegge un [`WorkerReport`]. Un file assente qui significa che il figlio non e' arrivato a
/// scriverlo — morto di segnale, o uscito prima: e' il fallimento di protocollo che il padre
/// distingue da un job fallito.
pub fn read_report(path: &Path) -> Result<WorkerReport, WorkerError> {
    let bytes = std::fs::read(path).map_err(|e| WorkerError::ReadReport { path: path.to_path_buf(), source: e })?;
    serde_json::from_slice(&bytes).map_err(|e| WorkerError::ParseReport { path: path.to_path_buf(), source: e })
}

/// Come un job puo' non produrre risultati. Le due forme sono tenute distinte fino in fondo perche'
/// hanno cause diverse e lettori diversi: la prima e' un problema dei dati o della configurazione,
/// e il suo messaggio e' **identico** a quello che il caso sequenziale avrebbe stampato; la seconda
/// e' un guasto dell'infrastruttura di P1, e riguarda chi sviluppa il motore.
#[derive(Debug, thiserror::Error)]
pub enum JobFailure {
    /// Il job e' fallito dentro il figlio, per un motivo di dominio. Il messaggio e' la forma
    /// `Display` dell'errore originale, verbatim: chi legge stderr non deve accorgersi che il job
    /// e' passato per un altro processo.
    #[error("{}", error.display)]
    Job { index: usize, error: ErrorRecord },
    /// Il protocollo padre-figlio si e' rotto: nessun referto leggibile e' tornato indietro. Il
    /// messaggio nomina il job, perche' a differenza di un errore di dominio qui l'utente non ha
    /// altro contesto da cui capire quale riga del batch e' andata storta.
    #[error("job {index} could not be run in a worker process: {source}")]
    Protocol {
        index: usize,
        #[source]
        source: WorkerError,
    },
}

impl JobFailure {
    /// La posizione del job nel batch, in entrambe le forme: e' cio' su cui si ordina per scegliere
    /// quale fallimento riportare.
    pub fn index(&self) -> usize {
        match self {
            JobFailure::Job { index, .. } | JobFailure::Protocol { index, .. } => *index,
        }
    }
}

/// L'area di lavoro privata di una corsa in processi figli: richieste, referti e log dei figli.
///
/// Sotto la temp di sistema, non nella cwd e non nella cartella di output — vale la stessa regola di
/// L5, e a maggior ragione con N figli. Lo schema del nome (`freeports-jobs-<pid>`) e' quello gia'
/// usato dai PDF temporanei di `cli::job`, cosi' un file rimasto indietro si riconduce comunque a
/// una corsa di questo programma.
///
/// La cancellazione e' in `Drop` e non a fine funzione: l'area deve sparire anche quando la corsa
/// esce per un errore, che e' esattamente il caso in cui e' piu' facile dimenticarsene.
#[derive(Debug)]
pub struct WorkArea {
    path: PathBuf,
}

impl WorkArea {
    pub fn create() -> Result<Self, WorkerError> {
        let path = std::env::temp_dir().join(format!("freeports-jobs-{}", std::process::id()));
        std::fs::create_dir_all(&path).map_err(|source| WorkerError::WorkArea { path: path.clone(), source })?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkArea {
    fn drop(&mut self) {
        // Best-effort: se la cancellazione fallisce restano dei file nella temp di sistema, che e'
        // spiacevole ma non e' un motivo per far fallire una corsa che ha gia' prodotto i risultati.
        if let Err(e) = std::fs::remove_dir_all(&self.path) {
            tracing::debug!(path = %self.path.display(), "could not remove the worker work area: {e}");
        }
    }
}

/// Prepara la cartella privata del job `index` e la richiesta che la descrive.
///
/// Un livello per job (`job-0`, `job-1`, ...) invece di file mescolati in una sola cartella: cosi'
/// i `.log.csv` dei figli, che hanno tutti lo stesso nome fisso, non si sovrascrivono a vicenda.
pub fn prepare_request(
    work_dir: &Path,
    index: usize,
    config: &FreeportsConfig,
    page_workers: usize,
) -> Result<WorkerRequest, WorkerError> {
    let job_dir = work_dir.join(format!("job-{index}"));
    let log_dir = job_dir.join("logs");
    std::fs::create_dir_all(&log_dir).map_err(|source| WorkerError::WriteRequest { path: log_dir.clone(), source })?;
    Ok(WorkerRequest {
        config: config.clone(),
        report_path: job_dir.join("report.json"),
        log_dir,
        page_workers,
    })
}

/// Dove vive il file di richiesta di un job: accanto al suo referto, nella cartella privata del
/// job. Non e' un campo di [`WorkerRequest`] perche' sarebbe l'unico campo che descrive il
/// contenitore invece del contenuto -- il figlio riceve gia' il percorso come argomento.
fn request_path_for(request: &WorkerRequest) -> PathBuf {
    request.report_path.with_file_name("request.json")
}

/// Esegue un job in un processo figlio e ne riporta il referto.
///
/// stderr e' **ereditato**: le righe dei job in corso arrivano all'utente mentre succedono, invece
/// che tutte insieme alla fine. Si interlacciano fra job diversi, ma ogni riga resta intera e porta
/// gia' il proprio percorso di span -- e' il margine che la risposta a Q-P2 concede. stdout invece
/// non e' un canale del protocollo: nulla di cio' che il figlio ci scrive viene letto.
fn run_one(executable: &Path, index: usize, request: &WorkerRequest) -> Result<WorkerReport, WorkerError> {
    let request_path = request_path_for(request);
    write_request(&request_path, request)?;

    let status = std::process::Command::new(executable)
        .arg("--internal-worker")
        .arg(&request_path)
        .status()
        .map_err(|source| WorkerError::Spawn { index, source })?;

    if !status.success() {
        return Err(WorkerError::Died { index, status: status.to_string() });
    }
    read_report(&request.report_path)
}

/// Esegue `requests` in processi figli, al piu' `parallelism` contemporaneamente, e restituisce i
/// referti **in ordine di job**.
///
/// Il pool e' a scorrimento, non a ondate: `parallelism` thread pescano il prossimo indice da un
/// contatore condiviso e depositano il proprio referto nello slot corrispondente. Ogni thread non
/// fa che avviare un processo e aspettarlo — nessun lavoro di dominio gira qui, quindi il GIL non
/// c'entra e i thread non si contendono nulla.
///
/// Gli slot indicizzati sono la ragione per cui l'output aggregato resta identico a quello
/// sequenziale anche con N figli: chi finisce prima non scavalca nessuno.
pub fn run_in_processes(
    executable: &Path,
    requests: &[WorkerRequest],
    parallelism: usize,
) -> Vec<Result<WorkerReport, WorkerError>> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<Result<WorkerReport, WorkerError>>>> = requests.iter().map(|_| Mutex::new(None)).collect();
    let workers = parallelism.clamp(1, requests.len().max(1));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(request) = requests.get(index) else { break };
                    let report = run_one(executable, index, request);
                    *slots[index].lock().unwrap_or_else(|p| p.into_inner()) = Some(report);
                }
            });
        }
    });

    slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            slot.into_inner()
                .unwrap_or_else(|p| p.into_inner())
                .unwrap_or_else(|| panic!("job {index} was never assigned to a worker: the pool left a hole"))
        })
        .collect()
}

/// Concatena i risultati dei job riusciti, o riporta il **primo** fallimento in ordine di job.
///
/// "Il primo in ordine di job", non "il primo arrivato": e' cio' che rende l'errore riportato lo
/// stesso che il `for` sequenziale avrebbe propagato, indipendentemente da quale figlio e' morto
/// per primo nel tempo.
pub fn collect(reports: Vec<Result<WorkerReport, WorkerError>>) -> Result<Vec<DocumentOutcome>, JobFailure> {
    let mut documents = Vec::new();
    for (index, report) in reports.into_iter().enumerate() {
        match report {
            Ok(WorkerReport::Succeeded { documents: mut d }) => documents.append(&mut d),
            Ok(WorkerReport::Failed { error }) => return Err(JobFailure::Job { index, error }),
            Err(source) => return Err(JobFailure::Protocol { index, source }),
        }
    }
    Ok(documents)
}

/// Il codice d'uscita di un figlio il cui **protocollo** si e' rotto: richiesta illeggibile, log non
/// apribili, referto non scrivibile. Un job fallito per un motivo di dominio non passa di qui —
/// esce con 0 e mette l'errore nel referto (vedi il doc-comment del modulo).
pub const PROTOCOL_FAILURE_EXIT_CODE: i32 = 2;

/// Il corpo del modo worker: esegue il job descritto da `request_path` e ne deposita il referto.
///
/// L'ordine dei passi non e' negoziabile. La richiesta si legge **prima** di inizializzare il
/// logging, perche' e' la richiesta a dire dove i log vanno; e i log si inizializzano **prima** di
/// eseguire il job, perche' altrimenti la strumentazione del job scriverebbe nel vuoto.
///
/// `Ok(())` significa "il protocollo ha funzionato", non "il job e' riuscito": un job fallito e' un
/// [`WorkerReport::Failed`] depositato con successo, ed e' esattamente cio' che il padre si aspetta
/// di trovare.
pub fn execute(request_path: &Path) -> Result<(), WorkerError> {
    let request = read_request(request_path)?;

    // Nella cartella privata del figlio, mai in quella di output del padre: `.log.csv` compreso.
    // Il padre li unira' ai propri a fine corsa.
    let log_handle = crate::core::tracing_setup::init(request.config.verbosity, &request.log_dir)?;
    log_handle.set_csv_dir(&request.log_dir)?;

    let parallelism = crate::core::parallelism::Parallelism::pages(request.page_workers);
    let report = match crate::cli::job::run(&request.config, parallelism) {
        Ok(documents) => WorkerReport::Succeeded { documents },
        // Gia' registrato da `job::run` con la sua catena completa: qui l'errore viene solo
        // impacchettato per il viaggio di ritorno, non ri-registrato.
        Err(e) => WorkerReport::Failed { error: ErrorRecord::from_error(&e) },
    };

    let write_result = write_report(&request.report_path, &report);
    // Tentata comunque, come in `main`: le righe diagnostiche di un job fallito sono le piu' utili
    // da avere su disco, e senza questa chiusura il padre unirebbe file mai svuotati.
    let close_result = log_handle.close();
    write_result?;
    close_result.map_err(WorkerError::from)
}

#[cfg(test)]
mod tests {
    use crate::cli::parallelism_config::{ParallelismConfig, Workers};
    use super::*;
    use crate::cli::conf_parse::DocumentSpec;
    use crate::core::algorithm::PageOutcome;
    use crate::core::page::{DocumentId, FormatName};
    use crate::core::pipeline::Extracted;
    use crate::core::schedule::PageClass;
    use crate::core::tracing_setup::Verbosity;
    use crate::output::classes::fund::Fund;
    use crate::output::routines::write::{OutFlags, OutStructureMode};

    fn config() -> FreeportsConfig {
        FreeportsConfig {
            verbosity: Verbosity::Warn,
            reports: vec![DocumentSpec {
                url: Some("https://example.invalid/a.pdf".to_string()),
                path: Some(PathBuf::from("/tmp/a.pdf")),
                name: Some("a".to_string()),
            }],
            target_lists: vec!["TEST".to_string()],
            format: "FMT".to_string(),
            out_path: PathBuf::from("/tmp/out"),
            out_profile: OutStructureMode::Regular,
            out_flags: OutFlags::default(),
            parallelism: ParallelismConfig { jobs: Workers::Fixed(4), pages: Workers::Auto },
            batch_file: Some(PathBuf::from("/tmp/jobs.csv")),
            save_pdf: true,
            formats_repo_path: Some(PathBuf::from("/repo")),
            input_db_path: Some(PathBuf::from("/db")),
            config_file: None,
        }
    }

    fn request() -> WorkerRequest {
        WorkerRequest {
            page_workers: 1,
            config: config(),
            report_path: PathBuf::from("/tmp/w0/report.json"),
            log_dir: PathBuf::from("/tmp/w0/logs"),
        }
    }

    fn documents() -> Vec<DocumentOutcome> {
        vec![DocumentOutcome {
            id: DocumentId::new("a"),
            format: FormatName::new("FMT"),
            pages: vec![PageOutcome {
                page: 12,
                class: PageClass::new("investments"),
                results: vec![Extracted::Fund(Fund::new("Alpha Fund"))],
            }],
        }]
    }

    /// Un errore vero con una `source()`, non una stringa inventata: e' l'unico modo di provare che
    /// la catena delle cause attraversa il confine.
    fn an_error_with_a_source() -> WorkerError {
        WorkerError::ReadReport {
            path: PathBuf::from("/tmp/missing.json"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        }
    }

    mod request_round_trip {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_request_written_to_a_file_reads_back_identical() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("request.json");
            let original = request();
            write_request(&path, &original).expect("writing a request to a fresh temp dir must work");
            assert_eq!(read_request(&path).expect("the request just written must read back"), original);
        }

        /// Il figlio esegue esattamente il job che il padre ha risolto: se un solo campo si
        /// perdesse, farebbe un lavoro diverso senza che nulla fallisca.
        #[test]
        fn every_field_of_the_configuration_crosses_the_boundary() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("request.json");
            write_request(&path, &request()).unwrap();
            let restored = read_request(&path).unwrap().config;
            assert_eq!(restored, config());
        }
    }

    mod report_round_trip {
        use super::*;
        use pretty_assertions::assert_eq;

        fn round_trip(report: &WorkerReport) -> WorkerReport {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("report.json");
            write_report(&path, report).expect("writing a report to a fresh temp dir must work");
            read_report(&path).expect("the report just written must read back")
        }

        #[test]
        fn a_successful_report_reads_back_identical() {
            let report = WorkerReport::Succeeded { documents: documents() };
            assert_eq!(round_trip(&report), report);
        }

        /// Un job che non estrae nulla non e' un errore, ed e' il caso che una serializzazione
        /// distratta confonderebbe con `Failed`.
        #[test]
        fn a_successful_report_with_no_documents_stays_successful() {
            let report = WorkerReport::Succeeded { documents: vec![] };
            assert_eq!(round_trip(&report), report);
        }

        /// **Bit per bit, non "abbastanza vicino".** Il referto e' JSON, e la lettura dei numeri
        /// in virgola mobile di `serde_json` e' esatta solo con la feature `float_roundtrip`:
        /// senza, un `f64` la cui rappresentazione decimale piu' corta non e' quella scritta torna
        /// indietro spostato di un ULP. E' un errore che non fa fallire nulla -- il figlio riesce,
        /// il padre scrive -- e si vede solo confrontando l'output di una corsa in processi con
        /// quello di una sequenziale, dove un `interest_rate` diventa `0.029249999999999998`
        /// invece di `0.02925`. Da P5 in poi il percorso in processi e' il **default**, quindi la
        /// differenza non sarebbe piu' un caso limite di chi passa `-j`.
        ///
        /// I valori qui sotto sono scelti perche' falliscono davvero senza la feature: ciascuno
        /// dista un ULP dal decimale piu' corto che lo rappresenta.
        #[test]
        fn a_float_survives_the_report_bit_for_bit() {
            use crate::commons::consts::Currency;
            use crate::core::classes::BlockValue;
            use crate::output::classes::investment::{Bond, InvestmentFields};

            for rate in [0.029_249_999_999_999_998_f64, 0.057_999_999_999_999_996_f64] {
                let fields = InvestmentFields::new(
                    "Acme Corp",
                    "Acme",
                    BlockValue::from("Alpha Fund"),
                    BlockValue::from(1000.0),
                    BlockValue::from(Currency::EUR),
                );
                let bond = Bond::build(fields, None, Some(rate)).expect("a valid bond");
                let report = WorkerReport::Succeeded {
                    documents: vec![DocumentOutcome {
                        id: DocumentId::new("a"),
                        format: FormatName::new("FMT"),
                        pages: vec![PageOutcome {
                            page: 1,
                            class: PageClass::new("investments"),
                            results: vec![Extracted::Bond(bond)],
                        }],
                    }],
                };
                let back = round_trip(&report);
                let WorkerReport::Succeeded { documents } = &back else { panic!("expected a success") };
                let Extracted::Bond(bond) = &documents[0].pages[0].results[0] else {
                    panic!("expected a bond")
                };
                let value = bond.interest_rate.expect("the rate must survive").into_inner();
                assert_eq!(
                    value.to_bits(),
                    rate.to_bits(),
                    "{value} came back from JSON as a different double than {rate}"
                );
            }
        }

        #[test]
        fn a_failed_report_reads_back_identical() {
            let report = WorkerReport::Failed { error: ErrorRecord::from_error(&an_error_with_a_source()) };
            assert_eq!(round_trip(&report), report);
        }

        /// La forma `Display` e' cio' che il padre stampa su stderr: deve essere **la stessa** che
        /// il caso sequenziale avrebbe stampato, altrimenti lo stesso errore si presenta all'utente
        /// in due modi a seconda di quanti worker ha chiesto.
        #[test]
        fn the_display_form_of_the_error_is_preserved_verbatim() {
            let error = an_error_with_a_source();
            let report = WorkerReport::Failed { error: ErrorRecord::from_error(&error) };
            match round_trip(&report) {
                WorkerReport::Failed { error: record } => assert_eq!(record.display, error.to_string()),
                other => panic!("expected a failed report, got {other:?}"),
            }
        }

        #[test]
        fn the_source_chain_of_the_error_is_preserved() {
            let report = WorkerReport::Failed { error: ErrorRecord::from_error(&an_error_with_a_source()) };
            match round_trip(&report) {
                WorkerReport::Failed { error: record } => assert_eq!(record.source, ["no such file"]),
                other => panic!("expected a failed report, got {other:?}"),
            }
        }
    }

    /// I due modi di non ricevere nulla di leggibile sono tenuti distinti perche' indicano bug
    /// diversi, e in nessuno dei due casi si va in panico: e' il padre che deve poterli riferire.
    mod protocol_failures {
        use super::*;

        #[test]
        fn a_missing_request_file_is_a_read_error_naming_the_path() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("absent.json");
            match read_request(&path) {
                Err(WorkerError::ReadRequest { path: reported, .. }) => assert_eq!(reported, path),
                other => panic!("expected a read error, got {other:?}"),
            }
        }

        #[test]
        fn a_malformed_request_file_is_a_parse_error_naming_the_path() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("garbage.json");
            std::fs::write(&path, b"{ this is not json").unwrap();
            match read_request(&path) {
                Err(WorkerError::ParseRequest { path: reported, .. }) => assert_eq!(reported, path),
                other => panic!("expected a parse error, got {other:?}"),
            }
        }

        /// Il caso insidioso: JSON valido, forma sbagliata. Succede quando padre e figlio sono due
        /// versioni diverse del binario, ed e' l'errore che un `unwrap()` trasformerebbe in un
        /// panico dentro un processo figlio, cioe' in un messaggio che nessuno vede.
        #[test]
        fn well_formed_json_of_the_wrong_shape_is_a_parse_error_not_a_panic() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("wrong-shape.json");
            std::fs::write(&path, br#"{"config": 42}"#).unwrap();
            assert!(matches!(read_request(&path), Err(WorkerError::ParseRequest { .. })));
        }

        #[test]
        fn a_missing_report_file_is_a_read_error_naming_the_path() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("absent.json");
            match read_report(&path) {
                Err(WorkerError::ReadReport { path: reported, .. }) => assert_eq!(reported, path),
                other => panic!("expected a read error, got {other:?}"),
            }
        }

        #[test]
        fn a_report_with_an_unknown_outcome_tag_is_a_parse_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("unknown.json");
            std::fs::write(&path, br#"{"outcome": "exploded"}"#).unwrap();
            assert!(matches!(read_report(&path), Err(WorkerError::ParseReport { .. })));
        }

        #[test]
        fn writing_into_a_directory_that_does_not_exist_is_a_write_error() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("no-such-subdir").join("request.json");
            match write_request(&path, &request()) {
                Err(WorkerError::WriteRequest { path: reported, .. }) => assert_eq!(reported, path),
                other => panic!("expected a write error, got {other:?}"),
            }
        }
    }

    /// Quale fallimento arriva all'utente quando piu' job vanno storti. La regola e' "il primo in
    /// ordine di job", non "il primo arrivato": e' l'unica che rende l'errore riportato lo stesso
    /// che il `for` sequenziale avrebbe propagato, comunque siano andate le corse dei figli.
    mod collecting_reports {
        use super::*;
        use pretty_assertions::assert_eq;

        fn succeeded(name: &str) -> Result<WorkerReport, WorkerError> {
            Ok(WorkerReport::Succeeded {
                documents: vec![DocumentOutcome { id: DocumentId::new(name), format: FormatName::new("FMT"), pages: vec![] }],
            })
        }

        fn failed(message: &str) -> Result<WorkerReport, WorkerError> {
            Ok(WorkerReport::Failed {
                error: ErrorRecord { debug: format!("{message:?}"), display: message.to_string(), source: vec![] },
            })
        }

        fn broken(index: usize) -> Result<WorkerReport, WorkerError> {
            Err(WorkerError::Died { index, status: "exit status: 9".to_string() })
        }

        #[test]
        fn all_successful_jobs_concatenate_in_job_order() {
            let documents = collect(vec![succeeded("a"), succeeded("b"), succeeded("c")]).expect("no job failed");
            let ids: Vec<&str> = documents.iter().map(|d| d.id.as_str()).collect();
            assert_eq!(ids, ["a", "b", "c"]);
        }

        #[test]
        fn an_empty_batch_collects_to_no_documents() {
            assert_eq!(collect(vec![]).expect("no job failed"), vec![]);
        }

        /// Un job che non estrae nulla non interrompe la concatenazione e non lascia buchi.
        #[test]
        fn a_job_with_no_documents_does_not_break_the_concatenation() {
            let empty = Ok(WorkerReport::Succeeded { documents: vec![] });
            let documents = collect(vec![succeeded("a"), empty, succeeded("c")]).expect("no job failed");
            let ids: Vec<&str> = documents.iter().map(|d| d.id.as_str()).collect();
            assert_eq!(ids, ["a", "c"]);
        }

        #[test]
        fn the_first_failing_job_in_order_is_the_one_reported() {
            let failure = collect(vec![succeeded("a"), failed("second broke"), failed("third broke")])
                .expect_err("a failing job must be reported");
            assert_eq!(failure.index(), 1);
            assert_eq!(failure.to_string(), "second broke");
        }

        /// Il caso che distingue "primo in ordine" da "primo arrivato": il job 2 e' morto di
        /// segnale, il job 1 e' fallito per un motivo di dominio. Deve vincere il job 1.
        #[test]
        fn an_earlier_domain_failure_wins_over_a_later_protocol_failure() {
            let failure = collect(vec![succeeded("a"), failed("second broke"), broken(2)])
                .expect_err("a failing job must be reported");
            assert!(matches!(failure, JobFailure::Job { index: 1, .. }), "got {failure:?}");
        }

        #[test]
        fn an_earlier_protocol_failure_wins_over_a_later_domain_failure() {
            let failure = collect(vec![broken(0), failed("second broke")]).expect_err("a failing job must be reported");
            assert!(matches!(failure, JobFailure::Protocol { index: 0, .. }), "got {failure:?}");
        }

        /// Il messaggio di un errore di dominio deve arrivare all'utente **verbatim**: lo stesso
        /// job, eseguito in sequenziale, stampa esattamente questa riga.
        #[test]
        fn a_domain_failure_is_reported_with_the_original_message_and_nothing_else() {
            let original = "the specified path /tmp/nope.pdf does not exist";
            let failure = collect(vec![failed(original)]).expect_err("a failing job must be reported");
            assert_eq!(failure.to_string(), original);
        }

        /// Un fallimento di protocollo invece **deve** nominare il job: non essendo un errore di
        /// dominio, l'utente non ha altro modo di sapere quale riga del batch e' andata storta.
        #[test]
        fn a_protocol_failure_names_the_job_it_belongs_to() {
            let failure = collect(vec![broken(0)]).expect_err("a broken worker must be reported");
            assert!(failure.to_string().contains("job 0"), "message does not name the job: {failure}");
        }
    }

    /// Il pool vero gira contro il binario reale nei test d'integrazione. Qui si prova cio' che non
    /// ha bisogno di un processo: la preparazione delle cartelle private, e il fatto che ogni job
    /// ne abbia una sua.
    mod preparing_requests {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_private_log_directory_is_created_on_disk() {
            let dir = tempfile::tempdir().unwrap();
            let request = prepare_request(dir.path(), 0, &config(), 1).expect("preparing a request must work");
            assert!(request.log_dir.is_dir(), "the log directory was not created: {}", request.log_dir.display());
        }

        /// I `.log.csv` dei figli hanno tutti lo stesso nome fisso: senza una cartella per job si
        /// sovrascriverebbero a vicenda, e il registro unito perderebbe tutte le righe tranne le
        /// ultime.
        #[test]
        fn two_jobs_never_share_a_directory() {
            let dir = tempfile::tempdir().unwrap();
            let first = prepare_request(dir.path(), 0, &config(), 1).unwrap();
            let second = prepare_request(dir.path(), 1, &config(), 1).unwrap();
            assert_ne!(first.log_dir, second.log_dir);
            assert_ne!(first.report_path, second.report_path);
        }

        /// Nessun file del figlio finisce accanto ai risultati della corsa: e' la regola gia'
        /// fissata in L5, e vale a maggior ragione per N figli.
        #[test]
        fn nothing_is_prepared_inside_the_configured_output_directory() {
            let dir = tempfile::tempdir().unwrap();
            let request = prepare_request(dir.path(), 0, &config(), 1).unwrap();
            assert!(request.log_dir.starts_with(dir.path()));
            assert!(!request.log_dir.starts_with(&config().out_path));
            assert!(!request.report_path.starts_with(&config().out_path));
        }

        #[test]
        fn the_configuration_travels_unchanged_into_the_request() {
            let dir = tempfile::tempdir().unwrap();
            assert_eq!(prepare_request(dir.path(), 3, &config(), 1).unwrap().config, config());
        }
    }

    mod error_messages {
        use super::*;

        #[test]
        fn every_message_names_the_file_it_is_about() {
            let path = PathBuf::from("/tmp/x/report.json");
            let io = || std::io::Error::new(std::io::ErrorKind::NotFound, "boom");
            let messages = [
                WorkerError::WriteRequest { path: path.clone(), source: io() }.to_string(),
                WorkerError::ReadRequest { path: path.clone(), source: io() }.to_string(),
                WorkerError::WriteReport { path: path.clone(), source: io() }.to_string(),
                WorkerError::ReadReport { path: path.clone(), source: io() }.to_string(),
            ];
            for message in messages {
                assert!(message.contains("/tmp/x/report.json"), "message does not name the file: {message}");
            }
        }
    }
}
