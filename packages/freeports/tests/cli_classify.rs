//! `freeports classify` against the **real** binary (phase 10,
//! `agent-memory/DESIGN-phase10-classify.md` §5, §6, §7 group F).
//!
//! The binary comes from `CARGO_BIN_EXE_freeports`, runs in a working directory of its own (so that
//! nothing it leaves behind can hide), with every `FREEPORTS_*` variable removed and an explicit
//! empty configuration file. The repository, the PDFs and — for the documents given by URL — a
//! local HTTP server are temporary, built here.
//!
//! stdout is the outcome, at every verbosity; stderr is the log. Exit codes: `0` every document
//! read (also "not classified", a partial, or algorithm and URL disagreeing); `1` an illegal
//! configuration, a classifier that will not load, documents disagreeing with each other, no
//! document readable; `3` some documents read and some not.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
    "FREEPORTS_OUT_PROFILE",
    "FREEPORTS_SEPARATE_OUT",
    "FREEPORTS_ARCHIVE",
    "FREEPORTS_SAVE_PDF",
    "FREEPORTS_FORMAT",
    "FREEPORTS_CONFIG_FILE",
    "FREEPORTS_TARGET_LIST",
    "FREEPORTS_FORMATS_REPO_PATH",
    "FREEPORTS_INPUT_DB_PATH",
];

const FORMATS_CSV: &str = "Name,Locale,Year,Country,Version\n\
                           AMUNDI,EN,24,,\n\
                           EURIZON,EN,23,,\n\
                           MEDIOLANUM,EN,23,,\n\
                           MEDIOLANUM,IT,24,,A\n\
                           MEDIOLANUM,IT,24,,B\n";

/// One entry on page 1: two complete formats; a partial block whose `B` is the "otherwise" (by
/// exclusion); and an entity alone, which leaves a partial.
const YAML: &str = "\
- 1:
    - AMUNDI-EN24:
        - {text: '^Amundi Funds$'}
    - EURIZON-EN23:
        - {text: '^Eurizon Fund$'}
    - MEDIOLANUM-IT24.*:
        - {text: '^Mediolanum Italia$'}
        - A: [{text: '^Classe A$'}]
        - B: ~
    - MEDIOLANUM:
        - {text: '^Mediolanum Gruppo$'}
";

/// One pipeline giving `AMUNDI-EN24` for a `Conflict A` line and `EURIZON-EN23` for a
/// `Conflict B` line: a page with both is a runtime classification error (D5). On any other page
/// it says nothing, and the YAML decides.
const PYTHON: &str = r#"
class P:
    def __init__(self, pdf_extract, text_filter, deserialize):
        self.pdf_extract = pdf_extract
        self.text_filter = text_filter
        self.deserialize = deserialize

def texts(page):
    return [s['text'] for b in page['blocks'] for l in b.get('lines', []) for s in l['spans']]

def extract(page):
    return [{'type_block': 'DOC_CLASS', 'metadata': {}, 'content': t} for t in texts(page) if t.startswith('Conflict')]

def keep(blocks, data):
    return [{'type_block': 'DOC_CLASS', 'metadata': {}, 'content': b.content} for b in blocks]

def deserialize(block):
    return {'Conflict A': 'AMUNDI-EN24', 'Conflict B': 'EURIZON-EN23'}.get(block.content)

document_classifier = [(1, P(extract, keep, deserialize))]
"#;

/// The text of each kind of document the tests use.
const AMUNDI: &[&str] = &["Amundi Funds"];
const EURIZON: &[&str] = &["Eurizon Fund"];
const MEDIOLANUM_A: &[&str] = &["Mediolanum Italia", "Classe A"];
const MEDIOLANUM_BY_EXCLUSION: &[&str] = &["Mediolanum Italia"];
const MEDIOLANUM_GROUP: &[&str] = &["Mediolanum Gruppo"];
const NOTHING: &[&str] = &["Nothing to recognise here"];
const CONFLICT: &[&str] = &["Conflict A", "Conflict B"];

/// A one-page PDF with one line per entry of `lines`, written with the same PyMuPDF the engine
/// reads it back with.
fn write_pdf(path: &Path, lines: &[&str]) {
    use pyo3::prelude::*;

    Python::attach(|py| {
        let fitz = PyModule::import(py, "fitz").expect(
            "PyMuPDF (fitz) must be importable: activate venv/freeports-dev, see AGENTS.md",
        );
        let doc = fitz.call_method0("open").unwrap();
        let page = doc
            .call_method1("new_page", (-1i64, 300.0f64, 400.0f64))
            .unwrap();
        for (i, line) in lines.iter().enumerate() {
            let y = 50.0f64 + 20.0 * i as f64;
            page.call_method1("insert_text", ((20.0f64, y), *line))
                .unwrap();
        }
        doc.call_method1("save", (path.to_str().unwrap(),)).unwrap();
        doc.call_method0("close").unwrap();
    });
}

fn pdf_bytes(lines: &[&str]) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("served.pdf");
    write_pdf(&path, lines);
    std::fs::read(path).unwrap()
}

/// A local HTTP server answering each path in `routes` with its bytes, and anything else with a
/// 404. Lives as long as the test process.
struct Server {
    base: String,
}

impl Server {
    fn start(routes: Vec<(&str, Vec<u8>)>) -> Self {
        let routes: HashMap<String, Vec<u8>> = routes
            .into_iter()
            .map(|(path, bytes)| (path.to_string(), bytes))
            .collect();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 8192];
                let read = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..read]).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                match routes.get(&path) {
                    Some(body) => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/pdf\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(body);
                    }
                    None => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                }
            }
        });
        Server {
            base: format!("http://{addr}"),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

/// A document spec `"<url>":<name>`: the URL quoted for its port's colon.
fn url_spec(url: &str, name: &str) -> String {
    format!("\"{url}\":{name}")
}

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    /// The repository (formats, YAML, Python, an empty URL mapping), an empty configuration file,
    /// and an empty working directory.
    fn new() -> Self {
        let fixture = Fixture {
            dir: tempfile::tempdir().unwrap(),
        };
        let metadata = fixture.repo().join("metadata");
        std::fs::create_dir_all(&metadata).unwrap();
        std::fs::create_dir_all(fixture.repo().join("content/templates")).unwrap();
        std::fs::write(metadata.join("formats.csv"), FORMATS_CSV).unwrap();
        std::fs::write(metadata.join("document_classify.yaml"), YAML).unwrap();
        std::fs::write(metadata.join("document_classify.py"), PYTHON).unwrap();
        fixture.url_mapping(&[]);
        std::fs::create_dir_all(fixture.cwd()).unwrap();
        std::fs::create_dir_all(fixture.docs()).unwrap();
        std::fs::write(fixture.config(), "").unwrap();
        fixture
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join("formats_repo")
    }

    fn cwd(&self) -> PathBuf {
        self.dir.path().join("cwd")
    }

    fn docs(&self) -> PathBuf {
        self.dir.path().join("docs")
    }

    fn config(&self) -> PathBuf {
        self.dir.path().join("config.yaml")
    }

    /// `url_mapping.csv` with these rows.
    fn url_mapping(&self, rows: &[(&str, String)]) {
        let mut csv = String::from("Format name,Url\n");
        for (format, url) in rows {
            csv.push_str(&format!("{format},{url}\n"));
        }
        std::fs::write(self.repo().join("metadata/url_mapping.csv"), csv).unwrap();
    }

    /// A local PDF, and its `-i` spec named `name`.
    fn local(&self, name: &str, lines: &[&str]) -> String {
        let path = self.docs().join(format!("{name}.pdf"));
        write_pdf(&path, lines);
        format!("{}:{name}", path.to_str().unwrap())
    }

    /// A local file that is not a PDF, and its `-i` spec named `name`.
    fn not_a_pdf(&self, name: &str) -> String {
        let path = self.docs().join(format!("{name}.pdf"));
        std::fs::write(&path, b"this is not a PDF at all").unwrap();
        format!("{}:{name}", path.to_str().unwrap())
    }

    /// `freeports <args>`, from the dedicated working directory, with no `FREEPORTS_*` variable.
    fn freeports(&self, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_freeports"));
        command.current_dir(self.cwd());
        for var in ALL_FREEPORTS_VARS {
            command.env_remove(var);
        }
        command
            .args(args)
            .output()
            .expect("the freeports binary must be runnable")
    }

    /// `freeports classify <args> --config <empty> -F <repo>`.
    fn classify(&self, args: &[&str]) -> Output {
        let config = self.config();
        let repo = self.repo();
        let mut all = vec!["classify"];
        all.extend_from_slice(args);
        all.extend_from_slice(&["--config", config.to_str().unwrap()]);
        all.extend_from_slice(&["-F", repo.to_str().unwrap()]);
        self.freeports(&all)
    }

    /// `freeports classify -i <specs…> <extra>`.
    fn classify_inputs(&self, specs: &[String], extra: &[&str]) -> Output {
        let mut args: Vec<&str> = vec!["-i"];
        args.extend(specs.iter().map(String::as_str));
        args.extend_from_slice(extra);
        self.classify(&args)
    }

    fn cwd_entries(&self) -> Vec<String> {
        std::fs::read_dir(self.cwd())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout_lines(output: &Output) -> Vec<String> {
    stdout(output).lines().map(str::to_string).collect()
}

/// The exit code, with both streams in the message when it is not the one expected.
fn assert_code(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

/// The fixture itself, checked once outside the binary: a mistake in it would otherwise read as
/// a failure of `classify`.
mod the_fixture {
    use super::*;

    #[test]
    fn the_repository_classification_loads() {
        let fixture = Fixture::new();
        freeports::formats_repo::doc_classify::load(&fixture.repo())
            .unwrap_or_else(|e| panic!("the fixture repository must load: {e}"));
    }
}

/// A local document: one line, exit `0`, whatever the outcome.
mod local_documents {
    use super::*;

    #[test]
    fn a_complete_format() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(&[fixture.local("Amundi", AMUNDI)], &[]);
        assert_code(&output, 0);
        assert_eq!(stdout_lines(&output), vec!["Amundi: AMUNDI-EN24"]);
    }

    #[test]
    fn a_complete_format_through_a_child_step() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(&[fixture.local("Med", MEDIOLANUM_A)], &[]);
        assert_code(&output, 0);
        assert_eq!(stdout_lines(&output), vec!["Med: MEDIOLANUM-IT24.A"]);
    }

    #[test]
    fn a_complete_format_by_exclusion() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(&[fixture.local("Med", MEDIOLANUM_BY_EXCLUSION)], &[]);
        assert_code(&output, 0);
        assert_eq!(
            stdout_lines(&output),
            vec!["Med: MEDIOLANUM-IT24.B (by exclusion)"]
        );
    }

    /// "Not classified" is an outcome, not a malfunction: `0` (decision of 2026-10-11).
    #[test]
    fn not_classified_exits_zero() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(&[fixture.local("Blank", NOTHING)], &[]);
        assert_code(&output, 0);
        assert_eq!(stdout_lines(&output), vec!["Blank: not classified"]);
    }

    #[test]
    fn a_partial_prints_its_candidates_and_exits_zero() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(&[fixture.local("Group", MEDIOLANUM_GROUP)], &[]);
        assert_code(&output, 0);
        assert_eq!(
            stdout_lines(&output),
            vec![
                "Group: partial MEDIOLANUM — candidates: MEDIOLANUM-EN23, MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.B"
            ]
        );
    }

    /// From `-v` on, the detail says which way and from which page.
    #[test]
    fn at_info_the_detail_names_the_algorithm_and_its_page() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(&[fixture.local("Amundi", AMUNDI)], &["-v"]);
        assert_code(&output, 0);
        let lines = stdout_lines(&output);
        assert_eq!(lines[0], "Amundi: AMUNDI-EN24");
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("  algorithm: AMUNDI-EN24 — page 1, YAML entry 0")),
            "{lines:#?}"
        );
        assert!(
            lines.iter().any(|l| l == "  sources: algorithm only"),
            "{lines:#?}"
        );
    }

    /// stdout is the product of the command: `-qq` silences the log, not the outcome.
    #[test]
    fn the_outcome_is_printed_even_at_qq() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(&[fixture.local("Amundi", AMUNDI)], &["-qq"]);
        assert_code(&output, 0);
        assert_eq!(stdout_lines(&output), vec!["Amundi: AMUNDI-EN24"]);
    }
}

/// Documents given by URL: the four combinations of algorithm and URL.
mod documents_by_url {
    use super::*;

    /// A server with an Amundi document under `/amundi/` (mapped to `AMUNDI-EN24`) and under
    /// `/other/` (not mapped), a blank one and an Amundi one under `/eurizon/` (mapped to
    /// `EURIZON-EN23`).
    fn serving(fixture: &Fixture) -> Server {
        let server = Server::start(vec![
            ("/amundi/report.pdf", pdf_bytes(AMUNDI)),
            ("/other/report.pdf", pdf_bytes(AMUNDI)),
            ("/eurizon/blank.pdf", pdf_bytes(NOTHING)),
            ("/eurizon/amundi.pdf", pdf_bytes(AMUNDI)),
        ]);
        fixture.url_mapping(&[
            ("AMUNDI-EN24", server.url("/amundi/")),
            ("EURIZON-EN23", server.url("/eurizon/")),
        ]);
        server
    }

    #[test]
    fn algorithm_and_url_agreeing() {
        let fixture = Fixture::new();
        let server = serving(&fixture);
        let output = fixture.classify_inputs(
            &[url_spec(&server.url("/amundi/report.pdf"), "Doc")],
            &["--no-download", "-v"],
        );
        assert_code(&output, 0);
        let lines = stdout_lines(&output);
        assert_eq!(lines[0], "Doc: AMUNDI-EN24");
        assert!(lines.iter().any(|l| l == "  sources: agree"), "{lines:#?}");
    }

    #[test]
    fn only_the_algorithm_when_the_url_is_not_mapped() {
        let fixture = Fixture::new();
        let server = serving(&fixture);
        let url = server.url("/other/report.pdf");
        let output = fixture.classify_inputs(&[url_spec(&url, "Doc")], &["--no-download", "-v"]);
        assert_code(&output, 0);
        let lines = stdout_lines(&output);
        assert_eq!(lines[0], "Doc: AMUNDI-EN24");
        assert!(
            lines
                .iter()
                .any(|l| *l == format!("  url: not mapped — {url}")),
            "{lines:#?}"
        );
        assert!(
            lines.iter().any(|l| l == "  sources: algorithm only"),
            "{lines:#?}"
        );
    }

    #[test]
    fn only_the_url_when_the_algorithm_does_not_classify() {
        let fixture = Fixture::new();
        let server = serving(&fixture);
        let output = fixture.classify_inputs(
            &[url_spec(&server.url("/eurizon/blank.pdf"), "Doc")],
            &["--no-download", "-v"],
        );
        assert_code(&output, 0);
        let lines = stdout_lines(&output);
        assert_eq!(lines[0], "Doc: EURIZON-EN23");
        assert!(
            lines.iter().any(|l| l == "  sources: url only"),
            "{lines:#?}"
        );
    }

    /// The algorithm wins on stdout, the disagreement is an `ERROR` on stderr, and the exit is
    /// still `0`: the document was read.
    #[test]
    fn algorithm_and_url_disagreeing_print_the_algorithm_log_an_error_and_exit_zero() {
        let fixture = Fixture::new();
        let server = serving(&fixture);
        let output = fixture.classify_inputs(
            &[url_spec(&server.url("/eurizon/amundi.pdf"), "Doc")],
            &["--no-download"],
        );
        assert_code(&output, 0);
        assert_eq!(stdout_lines(&output), vec!["Doc: AMUNDI-EN24"]);
        let err = stderr(&output);
        assert!(err.contains("AMUNDI-EN24"), "{err}");
        assert!(err.contains("EURIZON-EN23"), "{err}");
    }

    /// Decision of 2026-10-11: a document given only by URL is saved as in the run.
    #[test]
    fn a_url_only_document_is_saved_in_the_working_directory_as_in_the_run() {
        let fixture = Fixture::new();
        let server = serving(&fixture);
        let output =
            fixture.classify_inputs(&[url_spec(&server.url("/amundi/report.pdf"), "Doc")], &[]);
        assert_code(&output, 0);
        assert!(
            fixture.cwd().join("report.pdf").is_file(),
            "{:?}",
            fixture.cwd_entries()
        );
    }

    #[test]
    fn with_no_download_nothing_is_left_in_the_working_directory() {
        let fixture = Fixture::new();
        let server = serving(&fixture);
        let output = fixture.classify_inputs(
            &[url_spec(&server.url("/amundi/report.pdf"), "Doc")],
            &["--no-download"],
        );
        assert_code(&output, 0);
        assert!(
            fixture.cwd_entries().is_empty(),
            "{:?}",
            fixture.cwd_entries()
        );
    }
}

/// More than one document: a line each, then the format of the set.
mod several_documents {
    use super::*;

    #[test]
    fn agreeing_documents_are_followed_by_the_format_line() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(
            &[
                fixture.local("Group", MEDIOLANUM_GROUP),
                fixture.local("Med", MEDIOLANUM_A),
            ],
            &[],
        );
        assert_code(&output, 0);
        assert_eq!(
            stdout_lines(&output),
            vec![
                "Group: partial MEDIOLANUM — candidates: MEDIOLANUM-EN23, MEDIOLANUM-IT24.A, MEDIOLANUM-IT24.B",
                "Med: MEDIOLANUM-IT24.A",
                "format: MEDIOLANUM-IT24.A",
            ]
        );
    }

    /// The lines of the documents, no `format:` line, the error on stderr, exit `1`.
    #[test]
    fn disagreeing_documents_print_their_lines_and_fail() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(
            &[
                fixture.local("Amundi", AMUNDI),
                fixture.local("Eurizon", EURIZON),
            ],
            &[],
        );
        assert_code(&output, 1);
        assert_eq!(
            stdout_lines(&output),
            vec!["Amundi: AMUNDI-EN24", "Eurizon: EURIZON-EN23"]
        );
        let err = stderr(&output);
        assert!(
            err.contains("documents disagree on the algorithm format"),
            "{err}"
        );
    }

    /// User decision of 2026-10-17 (phase 11): the error names the documents by the label their
    /// line on stdout carries, not by their position.
    #[test]
    fn the_disagreement_names_the_documents_by_their_label() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(
            &[
                fixture.local("Amundi", AMUNDI),
                fixture.local("Eurizon", EURIZON),
            ],
            &[],
        );
        assert_code(&output, 1);
        let err = stderr(&output);
        assert!(err.contains("Amundi is AMUNDI-EN24"), "{err}");
        assert!(err.contains("Eurizon is EURIZON-EN23"), "{err}");
        assert!(!err.contains("document 0"), "{err}");
        assert!(!err.contains("document 1"), "{err}");
    }

    /// An unreadable document in between shifts nothing: the labels are the documents' own.
    #[test]
    fn an_unreadable_document_in_between_does_not_change_the_labels() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(
            &[
                fixture.local("Amundi", AMUNDI),
                fixture.not_a_pdf("Broken"),
                fixture.local("Eurizon", EURIZON),
            ],
            &[],
        );
        assert_code(&output, 1);
        let err = stderr(&output);
        let message = err
            .lines()
            .find(|line| line.contains("documents disagree"))
            .unwrap_or_else(|| panic!("no disagreement in:\n{err}"));
        assert!(message.contains("Amundi is AMUNDI-EN24"), "{message}");
        assert!(message.contains("Eurizon is EURIZON-EN23"), "{message}");
        assert!(!message.contains("Broken"), "{message}");
    }

    /// A document given by its path alone is named by its path, as on its stdout line.
    #[test]
    fn a_document_given_by_path_alone_is_named_by_its_path() {
        let fixture = Fixture::new();
        let amundi = fixture.local("amundi", AMUNDI);
        let amundi_path = amundi.rsplit_once(':').unwrap().0.to_string();
        let eurizon = fixture.local("eurizon", EURIZON);
        let eurizon_path = eurizon.rsplit_once(':').unwrap().0.to_string();
        let output = fixture.classify_inputs(&[amundi_path.clone(), eurizon_path.clone()], &[]);
        assert_code(&output, 1);
        let lines = stdout_lines(&output);
        assert!(
            lines.contains(&format!("{amundi_path}: AMUNDI-EN24")),
            "{lines:#?}"
        );
        let err = stderr(&output);
        assert!(
            err.contains(&format!("{amundi_path} is AMUNDI-EN24")),
            "{err}"
        );
        assert!(
            err.contains(&format!("{eurizon_path} is EURIZON-EN23")),
            "{err}"
        );
    }
}

/// What a document that cannot be read costs.
mod unreadable_documents {
    use super::*;

    #[test]
    fn one_unreadable_among_readable_ones_exits_three() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(
            &[fixture.not_a_pdf("Broken"), fixture.local("Amundi", AMUNDI)],
            &[],
        );
        assert_code(&output, 3);
        let lines = stdout_lines(&output);
        assert!(
            lines.iter().any(|l| l == "Amundi: AMUNDI-EN24"),
            "{lines:#?}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("Broken: error: ")),
            "{lines:#?}"
        );
    }

    #[test]
    fn every_document_unreadable_exits_one() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(
            &[fixture.not_a_pdf("First"), fixture.not_a_pdf("Second")],
            &[],
        );
        assert_code(&output, 1);
        let err = stderr(&output);
        assert!(
            err.contains("none of the 2 documents could be read"),
            "{err}"
        );
    }

    /// D5 in the subcommand: the error is the outcome of that document, printed, and counts as a
    /// document not read.
    #[test]
    fn a_runtime_classification_error_among_others_is_an_error_line_and_exits_three() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(
            &[
                fixture.local("Conflict", CONFLICT),
                fixture.local("Amundi", AMUNDI),
            ],
            &[],
        );
        assert_code(&output, 3);
        let lines = stdout_lines(&output);
        let conflict = lines
            .iter()
            .find(|l| l.starts_with("Conflict: error: "))
            .unwrap_or_else(|| panic!("no error line in {lines:#?}"));
        assert!(conflict.contains("two different formats"), "{conflict}");
        assert!(
            lines.iter().any(|l| l == "Amundi: AMUNDI-EN24"),
            "{lines:#?}"
        );
    }

    /// A download that fails is a document not read, like one that will not open.
    #[test]
    fn a_failed_download_among_readable_documents_exits_three() {
        let fixture = Fixture::new();
        let server = Server::start(vec![]);
        let output = fixture.classify_inputs(
            &[
                url_spec(&server.url("/missing.pdf"), "Missing"),
                fixture.local("Amundi", AMUNDI),
            ],
            &["--no-download"],
        );
        assert_code(&output, 3);
        assert!(
            stdout_lines(&output)
                .iter()
                .any(|l| l == "Amundi: AMUNDI-EN24"),
            "{}",
            stdout(&output)
        );
    }
}

/// Configuration and repository failures: exit `1`, nothing on stdout.
mod configuration_failures {
    use super::*;

    #[test]
    fn no_repository_exits_one_with_nothing_on_stdout() {
        let fixture = Fixture::new();
        let spec = fixture.local("Amundi", AMUNDI);
        let config = fixture.config();
        let output = fixture.freeports(&[
            "classify",
            "-i",
            &spec,
            "--config",
            config.to_str().unwrap(),
        ]);
        assert_code(&output, 1);
        assert!(stdout(&output).is_empty(), "{}", stdout(&output));
        let err = stderr(&output);
        assert!(err.contains("no formats_repo_path was configured"), "{err}");
    }

    #[test]
    fn no_document_exits_one_with_nothing_on_stdout() {
        let fixture = Fixture::new();
        let output = fixture.classify(&[]);
        assert_code(&output, 1);
        assert!(stdout(&output).is_empty(), "{}", stdout(&output));
        let err = stderr(&output);
        assert!(err.contains("no document to classify"), "{err}");
    }

    #[test]
    fn a_classification_that_will_not_load_exits_one_with_nothing_on_stdout() {
        let fixture = Fixture::new();
        std::fs::write(
            fixture.repo().join("metadata/document_classify.yaml"),
            "- 1:\n    - NOT-A-FORMAT-EN99:\n        - {text: 'x'}\n",
        )
        .unwrap();
        let output = fixture.classify_inputs(&[fixture.local("Amundi", AMUNDI)], &[]);
        assert_code(&output, 1);
        assert!(stdout(&output).is_empty(), "{}", stdout(&output));
        let err = stderr(&output);
        assert!(err.contains("document_classify.yaml"), "{err}");
    }

    /// clap's own refusal: a run flag with `classify`.
    #[test]
    fn a_run_flag_with_classify_is_a_usage_error() {
        let fixture = Fixture::new();
        let output = fixture.freeports(&["classify", "-T", "TEST"]);
        assert_ne!(output.status.code(), Some(0));
        assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    }
}

/// What `classify` leaves behind: nothing, below `trace`.
mod artifacts {
    use super::*;

    #[test]
    fn no_log_csv_nor_any_other_file_is_left_in_the_working_directory() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(
            &[
                fixture.local("Amundi", AMUNDI),
                fixture.local("Blank", NOTHING),
            ],
            &["-v"],
        );
        assert_code(&output, 0);
        assert!(
            fixture.cwd_entries().is_empty(),
            "{:?}",
            fixture.cwd_entries()
        );
    }

    #[test]
    fn nor_is_one_left_beside_the_documents() {
        let fixture = Fixture::new();
        let output = fixture.classify_inputs(&[fixture.local("Amundi", AMUNDI)], &[]);
        assert_code(&output, 0);
        let mut entries: Vec<String> = std::fs::read_dir(fixture.docs())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        assert_eq!(entries, vec!["Amundi.pdf"]);
    }
}

/// The run without a subcommand is untouched.
mod non_regression {
    use super::*;

    #[test]
    fn the_internal_worker_flag_still_fails_with_the_protocol_code_on_a_missing_request() {
        let fixture = Fixture::new();
        let output = fixture.freeports(&["--internal-worker", "/nonexistent/request.json"]);
        assert_eq!(
            output.status.code(),
            Some(freeports::cli::worker::PROTOCOL_FAILURE_EXIT_CODE),
            "{}",
            stderr(&output)
        );
    }

    /// Without a subcommand `classify` after `-i` is a document name, as it always was: the run
    /// fails on it as a missing document, not as a subcommand.
    #[test]
    fn classify_after_input_values_is_still_a_document_of_the_run() {
        let fixture = Fixture::new();
        let spec = fixture.local("Amundi", AMUNDI);
        let config = fixture.config();
        let output = fixture.freeports(&[
            "-i",
            &spec,
            "classify",
            "--config",
            config.to_str().unwrap(),
            "-T",
            "TEST",
            "-f",
            "AMUNDI-EN24",
        ]);
        assert_code(&output, 1);
        assert!(stdout(&output).is_empty(), "{}", stdout(&output));
        let err = stderr(&output);
        assert!(err.contains("does not exist"), "{err}");
    }
}
