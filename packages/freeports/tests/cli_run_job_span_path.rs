//! Phase 11: the span path of the job in two stages, against the **real** binary
//! (`agent-memory/DESIGN-phase11-run-integration.md` §3, §8.E4).
//!
//! The label of a span is built when the span is created, so the job's span can name the format
//! only once it is known. With a configured format both stages open `job[<format>]` and every path
//! is the one of today — the property that protects the `Activity` column of the reference
//! `.log.csv` files of the formats repository. Without one, what happens before the format is
//! settled (stage 1: opening and classifying the documents) is under a bare `job` span.
//!
//! **Observed on stderr, not in the `.log.csv`** (a deviation from §8.E4 of the design): the
//! `document skipped` event carries none of the five tagged fields (`page`, `coord_*`), so it never
//! writes a `.log.csv` row at all. stderr renders the same span path in the same shape as the
//! `Activity` column (`SpanPathFormat` in `core::tracing_setup`), and the `document` span has a
//! single field, so the two read alike here.
//!
//! The binary runs in a working directory of its own, with every `FREEPORTS_*` variable removed and
//! an explicit empty configuration file; the repository and the PDFs are temporary, built here.

use std::path::PathBuf;
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

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    /// The minimal one-format repository of the other binary tests (`A-EN24`), plus a YAML rule
    /// classifying a document whose line is `Holdings` as `A-EN24`.
    fn new() -> Self {
        let fixture = Self {
            dir: tempfile::TempDir::new().expect("temp dir"),
        };
        let repo = fixture.repo();
        for (relative, content) in [
            (
                "metadata/formats.csv",
                "Name,Locale,Year,Country,Version\nA,EN,24,,\n",
            ),
            ("metadata/url_mapping.csv", "Format name,Url\n"),
            (
                "metadata/document_classify.yaml",
                "1:\n  A-EN24: {text: '^Holdings$'}\n",
            ),
            (
                "content/orchestration/algorithms_schedule.csv",
                "Format name,Page type,Filter next iteration\nA-EN24,investments,\n",
            ),
            (
                "content/orchestration/mapping.csv",
                "ID,Page type\nA-EN24(investments),investments\n",
            ),
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
                "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero\n",
            ),
            (
                "content/algorithms/structured/investments/partial_pipes.csv",
                "ID,pdf_extract,text_filter,deserialize\n",
            ),
            (
                "content/algorithms/structured/page_classify/partial_pipes.csv",
                "ID,pdf_extract,text_filter,deserialize\n",
            ),
            (
                "content/algorithms/structured/investments/deselection_lists.csv",
                "ID,Deselection set\n",
            ),
            (
                "content/algorithms/semistructured/formats_mapping.csv",
                "ID,pdf_extract,text_filter,deserialize\n",
            ),
            (
                "content/algorithms/semistructured/args/pdf_extract.yaml",
                "{}",
            ),
            (
                "content/algorithms/semistructured/args/text_filter.yaml",
                "{}",
            ),
            (
                "content/algorithms/semistructured/args/deserialize.yaml",
                "{}",
            ),
        ] {
            let path = repo.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        std::fs::create_dir_all(fixture.cwd()).unwrap();
        std::fs::write(fixture.config(), "").unwrap();
        fixture
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join("formats_repo")
    }

    fn cwd(&self) -> PathBuf {
        self.dir.path().join("cwd")
    }

    fn config(&self) -> PathBuf {
        self.dir.path().join("config.yaml")
    }

    /// A one-page PDF with the line `Holdings`, and its `-i` spec named `name`.
    fn classified(&self, name: &str) -> String {
        use pyo3::prelude::*;

        let path = self.dir.path().join(format!("{name}.pdf"));
        Python::attach(|py| {
            let fitz = PyModule::import(py, "fitz").expect(
                "PyMuPDF (fitz) must be importable: activate venv/freeports-dev, see AGENTS.md",
            );
            let doc = fitz.call_method0("open").unwrap();
            let page = doc
                .call_method1("new_page", (-1i64, 200.0f64, 300.0f64))
                .unwrap();
            page.call_method1("insert_text", ((20.0f64, 50.0f64), "Holdings"))
                .unwrap();
            doc.call_method1("save", (path.to_str().unwrap(),)).unwrap();
            doc.call_method0("close").unwrap();
        });
        format!("{}:{name}", path.to_str().unwrap())
    }

    /// A file that is not a PDF, and its `-i` spec named `name`.
    fn unreadable(&self, name: &str) -> String {
        let path = self.dir.path().join(format!("{name}.pdf"));
        std::fs::write(&path, b"this is not a PDF at all").unwrap();
        format!("{}:{name}", path.to_str().unwrap())
    }

    /// `freeports -i <specs> <extra> -F <repo> -T TEST -o <out> --config <empty>`; the output
    /// directory, where the `.log.csv` lands.
    fn run(&self, specs: &[String], extra: &[&str]) -> (Output, PathBuf) {
        let out = self.dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_freeports"));
        command.current_dir(self.cwd());
        for var in ALL_FREEPORTS_VARS {
            command.env_remove(var);
        }
        let output = command
            .arg("-i")
            .args(specs)
            .args(extra)
            .args(["--formats-directory", self.repo().to_str().unwrap()])
            .args(["--target-list", "TEST"])
            .args(["--out", out.to_str().unwrap()])
            .args(["--config", self.config().to_str().unwrap()])
            .output()
            .expect("the freeports binary must be runnable");
        (output, out)
    }
}

/// stderr without its ANSI colors.
fn plain_stderr(output: &Output) -> String {
    let raw = String::from_utf8_lossy(&output.stderr);
    let mut plain = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    plain
}

/// The span path of every stderr line whose message starts with `prefix`: the text between the
/// level and the `: ` that precedes the message.
fn span_paths_of(output: &Output, prefix: &str) -> Vec<String> {
    plain_stderr(output)
        .lines()
        .filter_map(|line| {
            let (head, _) = line.split_once(&format!(": {prefix}"))?;
            let path = head.trim_start().split_once(' ')?.1;
            Some(path.trim().to_string())
        })
        .collect()
}

fn assert_written_but_not_everything(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(3),
        "written, but not everything was read:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// With `-f A-EN24` the skipped document is under `job[A-EN24]`, as today.
#[test]
fn with_a_configured_format_the_skipped_document_is_under_the_job_of_that_format() {
    let fixture = Fixture::new();
    let specs = vec![fixture.classified("Sound"), fixture.unreadable("Broken")];
    let (output, _) = fixture.run(&specs, &["-f", "A-EN24"]);
    assert_written_but_not_everything(&output);

    let paths = span_paths_of(&output, "document skipped");
    assert_eq!(paths.len(), 1, "{}", plain_stderr(&output));
    assert!(
        paths[0].ends_with("job[A-EN24]/document[Broken]"),
        "{paths:?}"
    );
}

/// Without `-f`, the document is opened (and found unreadable) in stage 1, before the format is
/// known: it is under the bare `job` span.
#[test]
fn without_a_format_the_skipped_document_is_under_the_bare_job_span() {
    let fixture = Fixture::new();
    let specs = vec![fixture.classified("Sound"), fixture.unreadable("Broken")];
    let (output, _) = fixture.run(&specs, &[]);
    assert_written_but_not_everything(&output);

    let paths = span_paths_of(&output, "document skipped");
    assert_eq!(paths.len(), 1, "{}", plain_stderr(&output));
    assert!(paths[0].ends_with("job/document[Broken]"), "{paths:?}");
    assert!(!paths[0].contains("job["), "{paths:?}");
}

/// And what follows the format, in stage 2, is under `job[<detected>]` again, as with `-f`: the
/// job's own `job finished` line names the format the documents gave.
#[test]
fn without_a_format_the_second_stage_is_under_the_job_of_the_detected_format() {
    let fixture = Fixture::new();
    let specs = vec![fixture.classified("Sound")];
    let (output, _) = fixture.run(&specs, &["-v"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let paths = span_paths_of(&output, "job finished");
    assert_eq!(paths.len(), 1, "{}", plain_stderr(&output));
    assert!(paths[0].ends_with("job[A-EN24]"), "{paths:?}");
}

/// The path **starts** at the job: the command line opens no span above it. `freeports.cli.run_job`
/// — what the format repositories' integration tests call — has nothing above its job either, so a
/// reference `.log.csv` written by the command line has the same `Activity` column as the one the
/// test computes, and can be regenerated with the command line.
#[test]
fn the_span_path_starts_at_the_job_as_it_does_under_run_job() {
    let fixture = Fixture::new();
    let specs = vec![fixture.classified("Sound"), fixture.unreadable("Broken")];
    let (output, _) = fixture.run(&specs, &["-f", "A-EN24"]);
    assert_written_but_not_everything(&output);

    let paths = span_paths_of(&output, "document skipped");
    assert_eq!(
        paths,
        ["job[A-EN24]/document[Broken]"],
        "{}",
        plain_stderr(&output)
    );
}
