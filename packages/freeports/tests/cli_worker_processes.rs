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

    /// Regola di L5, che con N figli avrebbe N modi in piu' di essere violata: `.log.csv` sta
    /// accanto agli output, mai nella cartella di lavoro.
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
    /// A `-v`, non alla verbosita' di default: dopo L4 il registro su file si ferma a `warn`, e una
    /// corsa che riesce non ne produce nessuno. Il livello serve a rendere osservabile l'unione, non
    /// a cambiarla.
    #[test]
    fn the_run_log_absorbs_what_the_workers_logged() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        let (output, _) = fixture.run_with(2, "out", &["-v"]);
        assert!(output.status.success(), "the run failed: {}", String::from_utf8_lossy(&output.stderr));

        let jsonl = String::from_utf8(read(&fixture.cwd().join("freeports.log.jsonl"))).unwrap();
        // Lo span `job` lo apre solo chi esegue un job, e in questa corsa i job li eseguono i figli.
        assert!(jsonl.contains("job finished"), "no worker record reached the run log:\n{jsonl}");
    }

    /// Alla verbosita' massima il registro strutturato si genera, ed e' **l'unico**: il vecchio
    /// `.freeports.log.yaml` e' stato ritirato (richiesta dell'utente, 2026-08-31), e ne' il padre
    /// ne' i figli devono lasciarne piu' traccia sul disco.
    #[test]
    fn at_trace_verbosity_only_the_jsonl_log_is_written_never_a_yaml_one() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-EN24")]);
        let (output, out_dir) = fixture.run_with(2, "out", &["-vvv"]);
        assert!(output.status.success(), "the run failed: {}", String::from_utf8_lossy(&output.stderr));

        assert!(fixture.cwd().join("freeports.log.jsonl").is_file(), "the structured log is missing");
        for dir in [fixture.cwd(), out_dir] {
            let yaml_logs: Vec<String> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| name.contains("log.yaml") || name.contains("log.yml"))
                .collect();
            assert!(yaml_logs.is_empty(), "a yaml log was written in {}: {yaml_logs:?}", dir.display());
        }
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

    /// Un job che fallisce deve fallire allo stesso modo comunque sia stato eseguito: stesso esito,
    /// stesso messaggio. E' cio' che rende il confine di processo invisibile a chi legge stderr.
    #[test]
    fn fails_with_the_same_message_whether_it_ran_sequentially_or_in_a_worker() {
        let fixture = Fixture::new(&[("first", "A-EN24"), ("second", "A-NOPE24")]);

        let (sequential, _) = fixture.run(1, "out-sequential");
        let (parallel, _) = fixture.run(2, "out-parallel");

        assert!(!sequential.status.success(), "a job with an unknown format must fail");
        assert!(!parallel.status.success(), "a job with an unknown format must fail in a worker too");
        assert_eq!(message_of(&sequential), message_of(&parallel), "the same failure is reported differently");
    }

    /// Il `for` sequenziale non scrive nulla quando un job fallisce, perche' `?` propaga prima di
    /// `write_results`. Il pool deve comportarsi allo stesso modo, anche se i job successivi hanno
    /// gia' girato.
    #[test]
    fn writes_no_output_at_all_even_when_later_jobs_succeeded() {
        let fixture = Fixture::new(&[("first", "A-NOPE24"), ("second", "A-EN24")]);
        let (output, out_dir) = fixture.run(2, "out");

        assert!(!output.status.success());
        assert!(!out_dir.join("investments.csv").exists(), "results were written despite a failing job");
    }
}
