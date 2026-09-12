//! I job di un batch eseguiti in processi figli (P1), contro il binario **vero**.
//!
//! `agent-memory/P1-implementation-plan.md` §7. Perche' un test d'integrazione e non unitario:
//! `std::env::current_exe()` sotto `cargo test` restituisce il binario della suite, non
//! `freeports`. Un test unitario che innescasse il pool lancerebbe copie di se' stesso. Qui il
//! percorso dell'eseguibile vero arriva da `CARGO_BIN_EXE_freeports`, che cargo definisce per i
//! soli test d'integrazione, e il binario viene invocato come lo invocherebbe un utente.
//!
//! Il test centrale e' **`-j 1` contro `-j N`**: e' cio' che trasforma "l'ordine e' preservato" da
//! speranza in garanzia. La risposta a Q-P2 concede l'equivalenza semantica, ma P1 raccoglie i
//! risultati in slot indicizzati, quindi l'identita' byte per byte deve valere lo stesso -- e se un
//! giorno non valesse piu', questo test lo dice.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Un banco di prova completo: due PDF, un repo formati minimo ma funzionante, un file di batch, e
/// una cartella di lavoro separata da cui lanciare il binario (cosi' nessun artefatto della corsa
/// finisce nella cartella della suite).
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new(jobs: &[(&str, &str)]) -> Self {
        let fixture = Self { dir: tempfile::TempDir::new().expect("temp dir") };
        fixture.write_repo();
        std::fs::create_dir_all(fixture.dir.path().join("cwd")).unwrap();

        let mut batch = String::from("pdf,format\n");
        for (name, format) in jobs {
            let pdf = fixture.write_pdf(name);
            batch.push_str(&format!("{},{}\n", pdf.to_str().unwrap(), format));
        }
        std::fs::write(fixture.dir.path().join("jobs.csv"), batch).unwrap();
        // Un file di configurazione vuoto ma esplicito: senza, `find_config` cercherebbe nella cwd
        // reale e la corsa dipenderebbe da cosa c'e' sul disco dello sviluppatore.
        std::fs::write(fixture.dir.path().join("config.yaml"), "").unwrap();
        fixture
    }

    /// Un PDF di una pagina con una riga di testo, generato con lo stesso PyMuPDF che il motore
    /// usera' per rileggerlo.
    fn write_pdf(&self, name: &str) -> PathBuf {
        use pyo3::prelude::*;

        let path = self.dir.path().join(format!("{name}.pdf"));
        Python::attach(|py| {
            let fitz = PyModule::import(py, "fitz")
                .expect("PyMuPDF (fitz) must be importable: activate venv/freeports-dev, see AGENTS.md");
            let doc = fitz.call_method0("open").unwrap();
            let page = doc.call_method1("new_page", (-1i64, 200.0f64, 300.0f64)).unwrap();
            page.call_method1("insert_text", ((20.0f64, 50.0f64), "Holdings")).unwrap();
            doc.call_method1("save", (path.to_str().unwrap(),)).unwrap();
            doc.call_method0("close").unwrap();
        });
        path
    }

    /// Lo stesso repo formati minimo dell'end-to-end di `cli::run`, un solo formato `A-EN24`.
    fn write_repo(&self) {
        let repo = self.dir.path().join("formats_repo");
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
                "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous,Interpret dash as zero\n",
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
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn cwd(&self) -> PathBuf {
        self.dir.path().join("cwd")
    }

    /// Esegue il binario vero in modalita' batch con `workers` job contemporanei, scrivendo in una
    /// cartella di output tutta sua.
    fn run(&self, workers: usize, out_name: &str) -> (Output, PathBuf) {
        self.run_with(workers, out_name, &[])
    }

    /// Come [`Fixture::run`], con argomenti in piu' -- serve ai test che hanno bisogno di alzare la
    /// verbosita' per osservare cio' che a livello di default non viene registrato.
    fn run_with(&self, workers: usize, out_name: &str, extra: &[&str]) -> (Output, PathBuf) {
        let workers = workers.to_string();
        let mut args = vec!["--workers", &workers];
        args.extend_from_slice(extra);
        self.run_args(out_name, &args)
    }

    /// Il livello sotto a [`Fixture::run_with`]: nessuna opzione di parallelismo imposta d'ufficio.
    /// Serve a P5, dove *quali* opzioni di parallelismo compaiono sulla riga di comando -- `--jobs`,
    /// `--pages`, o nessuna delle due -- e' proprio cio' che il test vuole variare.
    fn run_args(&self, out_name: &str, extra: &[&str]) -> (Output, PathBuf) {
        let out_dir = self.path().join(out_name);
        std::fs::create_dir_all(&out_dir).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_freeports"))
            // La cwd e' una cartella dedicata: `freeports.log.jsonl` ci finisce dentro, e cosi' si
            // puo' anche verificare che nient'altro ci finisca.
            .current_dir(self.cwd())
            .args(["--batch", self.path().join("jobs.csv").to_str().unwrap()])
            .args(["--formats-directory", self.path().join("formats_repo").to_str().unwrap()])
            .args(["--target-list", "TEST"])
            .args(["--out", out_dir.to_str().unwrap()])
            .args(["--config", self.path().join("config.yaml").to_str().unwrap()])
            .args(extra)
            .output()
            .expect("the freeports binary must be runnable");
        (output, out_dir)
    }
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

mod one_worker_against_many {
    use super::*;

    /// Il test che rende P1 una garanzia invece di una speranza: gli stessi job, eseguiti in
    /// sequenza e in quattro processi, devono produrre gli **stessi byte**.
    #[test]
    fn the_output_of_four_workers_is_byte_identical_to_the_output_of_one() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24"), ("third", "A-EN24"), ("fourth", "A-EN24")]);

        let (sequential, sequential_out) = fixture.run(1, "out-sequential");
        assert!(sequential.status.success(), "the sequential run failed: {}", String::from_utf8_lossy(&sequential.stderr));
        let (parallel, parallel_out) = fixture.run(4, "out-parallel");
        assert!(parallel.status.success(), "the parallel run failed: {}", String::from_utf8_lossy(&parallel.stderr));

        for name in ["investments.csv", "funds.csv"] {
            assert_eq!(
                read(&sequential_out.join(name)),
                read(&parallel_out.join(name)),
                "{name} differs between one worker and four"
            );
        }
    }

    /// Piu' worker che job: il pool non deve avviare figli che non hanno nulla da fare, e il
    /// risultato resta quello di sempre.
    #[test]
    fn asking_for_more_workers_than_jobs_still_produces_the_same_output() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);

        let (_, sequential_out) = fixture.run(1, "out-sequential");
        let (parallel, parallel_out) = fixture.run(16, "out-parallel");
        assert!(parallel.status.success(), "the parallel run failed: {}", String::from_utf8_lossy(&parallel.stderr));

        assert_eq!(read(&sequential_out.join("investments.csv")), read(&parallel_out.join("investments.csv")));
    }
}

/// P5: la sezione `parallelism` vista dal binario vero.
///
/// I test di precedenza fra sorgenti stanno in `tests/cli_config.rs`, che si ferma alla
/// configurazione risolta. Qui interessa l'altra meta': che le opzioni **facciano** cio' che
/// dicono, e che nessuna combinazione cambi i byte prodotti.
mod parallelism_options {
    use super::*;

    fn four_jobs() -> Fixture {
        Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24"), ("third", "A-EN24"), ("fourth", "A-EN24")])
    }

    /// I due livelli si scelgono separatamente, e nessuna delle quattro combinazioni sposta un
    /// byte: e' l'invariante di determinismo di `PLAN.md` §6, esteso alla superficie di P5.
    #[test]
    fn every_combination_of_the_two_levels_produces_the_same_bytes() {
        let fixture = four_jobs();
        let (sequential, reference) = fixture.run_args("out-sequential", &["--jobs", "1", "--pages", "1"]);
        assert!(sequential.status.success(), "{}", String::from_utf8_lossy(&sequential.stderr));

        for (name, args) in [
            ("out-jobs-only", vec!["--jobs", "4", "--pages", "1"]),
            ("out-pages-only", vec!["--jobs", "1", "--pages", "4"]),
            ("out-both", vec!["--jobs", "4", "--pages", "4"]),
            ("out-auto", vec!["--jobs", "auto", "--pages", "auto"]),
        ] {
            let (run, out_dir) = fixture.run_args(name, &args);
            assert!(run.status.success(), "{args:?} failed: {}", String::from_utf8_lossy(&run.stderr));
            for file in ["investments.csv", "funds.csv"] {
                assert_eq!(read(&reference.join(file)), read(&out_dir.join(file)), "{file} differs with {args:?}");
            }
        }
    }

    /// Il default di P5: nessuna opzione di parallelismo, e la corsa usa comunque la macchina --
    /// senza che l'output ne risenta (`agent-memory/P5-implementation-plan.md` D-P5-4).
    #[test]
    fn a_run_with_no_parallelism_option_at_all_matches_the_sequential_one() {
        let fixture = four_jobs();
        let (_, reference) = fixture.run_args("out-sequential", &["--jobs", "1", "--pages", "1"]);
        let (default_run, default_out) = fixture.run_args("out-default", &[]);
        assert!(default_run.status.success(), "{}", String::from_utf8_lossy(&default_run.stderr));
        assert_eq!(read(&reference.join("investments.csv")), read(&default_out.join("investments.csv")));
    }

    /// `--workers` resta il default globale di **entrambi** i livelli: `--workers 1` e'
    /// l'abbreviazione di `--jobs 1 --pages 1`, cioe' il modo con cui si verifica il determinismo.
    #[test]
    fn one_global_worker_is_the_same_as_one_at_each_level() {
        let fixture = four_jobs();
        let (_, by_level) = fixture.run_args("out-by-level", &["--jobs", "1", "--pages", "1"]);
        let (global, global_out) = fixture.run_args("out-global", &["--workers", "1"]);
        assert!(global.status.success(), "{}", String::from_utf8_lossy(&global.stderr));
        assert_eq!(read(&by_level.join("investments.csv")), read(&global_out.join("investments.csv")));
    }

    /// Un override per livello batte il default globale anche sulla stessa riga di comando.
    #[test]
    fn a_per_level_option_overrides_the_global_default_on_the_same_command_line() {
        let fixture = four_jobs();
        let (_, reference) = fixture.run_args("out-sequential", &["--jobs", "1", "--pages", "1"]);
        let (run, out_dir) = fixture.run_args("out-mixed", &["--workers", "4", "--jobs", "1"]);
        assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(read(&reference.join("investments.csv")), read(&out_dir.join("investments.csv")));
    }

    /// Un valore malformato si ferma alla riga di comando, nominando l'opzione sbagliata: nessun
    /// job parte, e l'utente non deve indovinare quale delle tre opzioni ha scritto male.
    #[test]
    fn a_malformed_value_stops_the_run_and_names_the_option() {
        let fixture = four_jobs();
        let (run, _) = fixture.run_args("out-invalid", &["--pages", "0"]);
        assert!(!run.status.success(), "a malformed --pages must not run any job");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(stderr.contains("--pages"), "{stderr}");
    }
}

mod artifacts_stay_where_they_belong {
    use super::*;
    use test_case::test_case;

    /// `.log.csv` sta accanto agli output, mai nella cartella di lavoro -- una regola che con N
    /// figli ha N modi in piu' di essere violata.
    #[test]
    fn the_log_csv_lands_next_to_the_output_and_never_in_the_working_directory() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        let (output, out_dir) = fixture.run(2, "out");
        assert!(output.status.success(), "the run failed: {}", String::from_utf8_lossy(&output.stderr));

        assert!(out_dir.join(".log.csv").is_file(), "the run log is not next to the output");
        assert!(!fixture.cwd().join(".log.csv").exists(), "a .log.csv was left in the working directory");
    }

    /// Il `.log.csv` del padre assorbe le righe dei figli: deve restare **un solo** header, e ogni
    /// riga deve avere il numero di colonne dichiarato da quell'header. E' qui che si vedrebbe uno
    /// scarto fra il padre e i figli dopo un cambio di colonne — un figlio che scrive otto celle
    /// dove il padre ne dichiara nove passerebbe inosservato in ogni test a processo singolo.
    #[test]
    fn the_absorbed_child_rows_have_the_same_columns_as_the_header() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        let (output, out_dir) = fixture.run(2, "out");
        assert!(output.status.success(), "the run failed: {}", String::from_utf8_lossy(&output.stderr));

        let content = std::fs::read_to_string(out_dir.join(".log.csv")).expect("read .log.csv");
        let mut reader = csv::Reader::from_reader(content.as_bytes());
        let columns = reader.headers().expect("the header row").len();
        assert_eq!(
            reader.headers().expect("the header row").iter().collect::<Vec<_>>().last(),
            Some(&"Message"),
            "the header is not the one the engine writes: {content}"
        );
        for (i, record) in reader.records().enumerate() {
            let record = record.unwrap_or_else(|e| panic!("row {} is malformed: {e}", i + 1));
            assert_eq!(record.len(), columns, "row {} has the wrong number of cells", i + 1);
        }
        assert_eq!(
            content.matches("Report,Page,Activity").count(),
            1,
            "a child's header leaked into the middle of the file: {content}"
        );
    }

    /// I file privati dei figli — richieste, referti, log — vivono in un'area temporanea che
    /// sparisce da sola: nessuno di loro deve comparire fra i risultati della corsa.
    #[test]
    fn no_worker_file_is_left_among_the_results() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        let (_, out_dir) = fixture.run(2, "out");

        let leftovers: Vec<String> = std::fs::read_dir(&out_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("job-") || name == "report.json" || name == "request.json")
            .collect();
        assert!(leftovers.is_empty(), "worker files were left among the results: {leftovers:?}");
    }

    /// Le righe dei figli devono arrivare nel registro della corsa: e' l'unico posto in cui
    /// l'utente le vedra', visto che le cartelle private sono gia' sparite.
    ///
    /// A `-vvv`, perche' e' l'unica verbosita' alla quale il registro strutturato esiste. Il
    /// livello serve a rendere osservabile l'unione, non a cambiarla.
    #[test]
    fn the_run_log_absorbs_what_the_workers_logged() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        let (output, _) = fixture.run_with(2, "out", &["-vvv"]);
        assert!(output.status.success(), "the run failed: {}", String::from_utf8_lossy(&output.stderr));

        let jsonl = String::from_utf8(read(&fixture.cwd().join("freeports.log.jsonl"))).unwrap();
        // Lo span `job` lo apre solo chi esegue un job, e in questa corsa i job li eseguono i figli.
        assert!(jsonl.contains("job finished"), "no worker record reached the run log:\n{jsonl}");
    }

    /// Il registro strutturato e' **l'unico** registro diagnostico, e alla verbosita' massima
    /// esiste: ne' il padre ne' i figli devono lasciare sul disco nient'altro accanto ad esso.
    #[test]
    fn at_trace_verbosity_the_jsonl_log_is_written_and_is_the_only_one() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        let (output, out_dir) = fixture.run_with(2, "out", &["-vvv"]);
        assert!(output.status.success(), "the run failed: {}", String::from_utf8_lossy(&output.stderr));

        assert!(fixture.cwd().join("freeports.log.jsonl").is_file(), "the structured log is missing");
        for dir in [fixture.cwd(), out_dir] {
            let other_logs: Vec<String> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| name.contains("log.yaml") || name.contains("log.yml") || name == "freeports.log")
                .collect();
            assert!(other_logs.is_empty(), "a second diagnostic log was written in {}: {other_logs:?}", dir.display());
        }
    }

    /// La verbosita' risolta, non quella della riga di comando, decide se il registro strutturato
    /// esiste: `verbosity: trace` nel file di configurazione basta, senza alcun `-v`.
    ///
    /// E' il difetto che il padre aveva e i figli no — il padre avviava la registrazione dagli
    /// argomenti prima di risolvere la configurazione e non ci tornava piu' sopra, cosi' i figli
    /// scrivevano un registro che il padre non aveva. Adesso i due filtri di livello si alzano a
    /// configurazione risolta, ed e' l'unico momento in cui la risposta esiste per entrambi.
    #[test]
    fn the_resolved_verbosity_decides_not_the_command_line_one() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        std::fs::write(fixture.path().join("config.yaml"), "verbosity: trace\n").unwrap();
        let (output, _) = fixture.run(2, "out");
        assert!(output.status.success(), "the run failed: {}", String::from_utf8_lossy(&output.stderr));

        let jsonl = fixture.cwd().join("freeports.log.jsonl");
        assert!(jsonl.is_file(), "`verbosity: trace` in the configuration file must produce the structured log");
        let content = String::from_utf8(read(&jsonl)).unwrap();
        // Lo span `job` lo apre solo chi esegue un job: se c'e', anche cio' che hanno scritto i
        // figli e' arrivato nel registro del padre, che e' la meta' che prima si perdeva.
        assert!(content.contains("job finished"), "no worker record reached the run log:\n{content}");
        // E cio' che il padre ha scritto dopo aver risolto la configurazione c'e' al livello nuovo:
        // senza il ricaricamento del filtro, i `debug!` sarebbero gia' stati spenti al callsite.
        assert!(content.contains("\"DEBUG\""), "the reloaded filter did not raise the level:\n{content}");
    }

    /// Sotto alla verbosita' massima il registro strutturato **non esiste**, e non esiste come
    /// assenza di file, non come file vuoto: chi guarda una corsa su stderr non deve trovarsi
    /// niente nella cartella da cui l'ha lanciata.
    #[test_case(&[]; "alla verbosita' predefinita")]
    #[test_case(&["-q"]; "-q: solo errori")]
    #[test_case(&["-v"]; "-v: info")]
    #[test_case(&["-vv"]; "-vv: debug")]
    fn below_trace_verbosity_the_working_directory_stays_empty(extra: &[&str]) {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        let (output, _) = fixture.run_with(2, "out", extra);
        assert!(output.status.success(), "the run failed: {}", String::from_utf8_lossy(&output.stderr));

        let leftovers: Vec<String> = std::fs::read_dir(fixture.cwd())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(leftovers.is_empty(), "the run left files in the working directory: {leftovers:?}");
    }
}

mod a_failing_job {
    use super::*;

    fn message_of(output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| line.starts_with("freeports: "))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Il testo che stderr riporta per un job fallito deve essere lo stesso comunque il job sia
    /// stato eseguito. E' cio' che rende il confine di processo invisibile a chi legge stderr.
    #[test]
    fn is_reported_with_the_same_message_whether_it_ran_sequentially_or_in_a_worker() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-NOPE24")]);

        let (sequential, _) = fixture.run_with(1, "out-sequential", &["-v"]);
        let (parallel, _) = fixture.run_with(2, "out-parallel", &["-v"]);

        let failure_line = |output: &Output| {
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .find(|line| line.contains("job 1 failed"))
                .map(|line| line.to_string())
                .unwrap_or_else(|| panic!("no failure line in:\n{}", String::from_utf8_lossy(&output.stderr)))
        };
        assert_eq!(failure_line(&sequential), failure_line(&parallel), "the same failure is reported differently");
    }

    /// Il punto del piano, misurato da fuori: **un job che fallisce costa i propri risultati e non
    /// quelli degli altri**. Prima, il primo fallimento usciva prima di `write_results` e buttava
    /// via anche i job gia' girati -- su un batch vero, 903 job scartati per 1.
    ///
    /// L'uscita e' `3`: i risultati sono stati scritti, e non e' stato letto tutto.
    #[test]
    fn costs_its_own_results_and_not_those_of_the_other_jobs() {
        let fixture = Fixture::new(&[("first", "A-NOPE24"), ("second", "A-EN24")]);
        let (output, out_dir) = fixture.run(2, "out");

        assert_eq!(
            output.status.code(),
            Some(3),
            "written, but not everything was read:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(out_dir.join("investments.csv").is_file(), "the sound job's results must be on disk");
        assert!(out_dir.join("funds.csv").is_file());
    }

    /// Lo stesso esito dai due rami: `--workers 1` e `--workers 2` sono la stessa corsa, ed e' la
    /// proprieta' che `docs/source/reference/design/determinism.md` difende.
    #[test]
    fn the_exit_status_is_the_same_sequentially_and_in_workers() {
        let fixture = Fixture::new(&[("first", "A-NOPE24"), ("second", "A-EN24")]);

        let (sequential, _) = fixture.run(1, "out-sequential");
        let (parallel, _) = fixture.run(2, "out-parallel");

        assert_eq!(sequential.status.code(), Some(3));
        assert_eq!(parallel.status.code(), Some(3));
    }

    /// Il pavimento: se **nessun** job ha prodotto niente non c'e' nessun risultato parziale da
    /// salvare, e sei tabelle vuote sono peggio di nessuna tabella. Uscita `1`, col messaggio del
    /// primo fallimento in ordine di job.
    #[test]
    fn writes_nothing_and_exits_one_when_no_job_produced_anything() {
        let fixture = Fixture::new(&[("first", "A-NOPE24"), ("second", "A-ALSO-NOPE24")]);
        let (output, out_dir) = fixture.run(2, "out");

        assert_eq!(output.status.code(), Some(1), "a run that produced nothing must fail");
        assert!(!out_dir.join("investments.csv").exists(), "six empty tables are worse than none");
        assert!(message_of(&output).contains("A-NOPE24"), "got: {}", message_of(&output));
    }

    /// E una corsa in cui tutto e' andato bene esce con `0`: il `3` deve distinguere, non allarmare.
    #[test]
    fn a_run_in_which_every_job_worked_still_exits_zero() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        let (output, out_dir) = fixture.run(2, "out");

        assert_eq!(output.status.code(), Some(0), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(out_dir.join("investments.csv").is_file());
    }
}
