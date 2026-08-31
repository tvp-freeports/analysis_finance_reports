//! P0 profiling harness (`PLAN.md` §4 P0): where does a real job spend its time?
//!
//! Not production code and not compiled into the `freeports` binary — a cargo example, so the
//! measurement can be repeated after P1..P4 land without any of it living in the crate.
//!
//! It answers the three questions `PLAN.md` §4 P0 asks, and nothing else:
//!
//! 1. how much `input::document::load_document_pages` (PyMuPDF, **under the GIL**) weighs on the
//!    total — measured with a plain `Instant` around the call, since it is one call per document;
//! 2. how much classification weighs against the execution steps;
//! 3. how the three segments weigh against each other, and how much a single pipe costs.
//!
//! Questions 2 and 3 are answered by the spans L2 already installed everywhere (`classify`,
//! `step`, `class`, `pipeline`, `pdf_extract`, `text_filter`, `deserialize`, `pipe`): this file
//! adds a `tracing` layer that accumulates *busy time* per span path instead of formatting the
//! events. Nothing else in the crate changes — the numbers come from the instrumentation that is
//! already there.
//!
//! The filter mirrors production (`core::tracing_setup::EventLevelFilter` at the default `Warn`
//! verbosity): spans always pass, events are levelled at `WARN`. Measuring with `-vvv` would
//! measure the logging, not the engine.
//!
//! ```bash
//! cargo run --release --example p0_profile -- \
//!     <formats_repo> <input_db> <format> <report.pdf> [out_dir]
//! ```
//!
//! The last positional is optional; without it the outputs go to a temporary directory that is
//! removed on exit (run artifacts never land in the working directory).

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Filter, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

use clap::Parser as _;

use freeports::cli::config_locations::cmd::CliArgs;
use freeports::cli::output as cli_output;
use freeports::cli::run;
use freeports::core::algorithm::Algorithm;
use freeports::core::parallelism::Parallelism;
use freeports::core::page::FormatName;
use freeports::input::companies_db::compile_target_companies;
use freeports::input::document::load_document;

// ---------------------------------------------------------------------------------------------
// The timing layer
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default, Clone, Copy)]
struct Stat {
    /// Wall time between `enter` and `exit`, children included.
    inclusive: Duration,
    /// Same, minus the time spent inside nested spans — the time this span burns *itself*.
    exclusive: Duration,
    entries: u64,
}

fn totals() -> &'static Mutex<HashMap<String, Stat>> {
    static TOTALS: OnceLock<Mutex<HashMap<String, Stat>>> = OnceLock::new();
    TOTALS.get_or_init(|| Mutex::new(HashMap::new()))
}

struct Frame {
    label: String,
    enter: Instant,
    /// Inclusive time of the direct children closed while this frame was on top.
    children: Duration,
}

thread_local! {
    static STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// The label a span contributes to the path.
///
/// Span names alone are a tiny vocabulary (`page`, `step`, `pipe`, …), so the *values* are what
/// separate one pipe from another. Only the low-cardinality ones are kept: `page[1824]` would
/// give one row per page and answer nothing, while `pipe[…]` is exactly what question 3 asks for.
struct LabelVisitor {
    label: String,
    wants_field: bool,
    taken: bool,
}

impl Visit for LabelVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_any(field, format_args!("{value:?}"));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_any(field, format_args!("{value}"));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_any(field, format_args!("{value}"));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_any(field, format_args!("{value}"));
    }
}

impl LabelVisitor {
    fn record_any(&mut self, _field: &Field, value: std::fmt::Arguments<'_>) {
        if !self.wants_field || self.taken {
            return;
        }
        self.taken = true;
        let _ = write!(self.label, "[{value}]");
    }
}

/// Span names whose first field is small enough a vocabulary to keep in the path.
fn keeps_field_value(name: &str) -> bool {
    matches!(name, "step" | "class" | "pipeline" | "pipe" | "format")
}

struct TimingLayer;

impl<S> Layer<S> for TimingLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let name = attrs.metadata().name();
        let mut visitor = LabelVisitor {
            label: name.to_string(),
            wants_field: keeps_field_value(name),
            taken: false,
        };
        attrs.record(&mut visitor);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(SpanLabel(visitor.label));
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let label = ctx
            .span(id)
            .and_then(|span| span.extensions().get::<SpanLabel>().map(|l| l.0.clone()))
            .unwrap_or_else(|| "?".to_string());
        STACK.with(|stack| {
            stack.borrow_mut().push(Frame { label, enter: Instant::now(), children: Duration::ZERO })
        });
    }

    fn on_exit(&self, _id: &Id, _ctx: Context<'_, S>) {
        let now = Instant::now();
        STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            let Some(frame) = stack.pop() else { return };
            let inclusive = now.saturating_duration_since(frame.enter);
            let exclusive = inclusive.saturating_sub(frame.children);
            let mut path = String::new();
            for parent in stack.iter() {
                path.push_str(&parent.label);
                path.push('/');
            }
            path.push_str(&frame.label);
            if let Some(parent) = stack.last_mut() {
                parent.children += inclusive;
            }
            let mut totals = totals().lock().expect("the profiling map is never poisoned");
            let entry = totals.entry(path).or_default();
            entry.inclusive += inclusive;
            entry.exclusive += exclusive;
            entry.entries += 1;
        });
    }
}

struct SpanLabel(String);

/// Production's filter, replicated: spans unconditionally, events at the default `Warn`.
/// See `core::tracing_setup::EventLevelFilter` — measuring with events on would measure logging.
struct SpansAlwaysEventsAtWarn;

impl<S> Filter<S> for SpansAlwaysEventsAtWarn {
    fn enabled(&self, meta: &tracing::Metadata<'_>, _cx: &Context<'_, S>) -> bool {
        meta.is_span() || LevelFilter::from_level(*meta.level()) <= LevelFilter::WARN
    }
    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::INFO)
    }
}

// ---------------------------------------------------------------------------------------------
// The job, phase by phase
// ---------------------------------------------------------------------------------------------

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Estrae `--pages N` da `argv`, lasciando i posizionali al loro posto.
///
/// P2 aggiunge un solo grado di liberta' a questo strumento: quante pagine alla volta. Un flag
/// invece di un sesto posizionale, cosi' che il comando documentato in `agent-memory/P0-profile.md`
/// continui a funzionare parola per parola.
fn take_pages_flag(argv: &mut Vec<String>) -> usize {
    let Some(index) = argv.iter().position(|arg| arg == "--pages") else {
        return 1;
    };
    let value = argv
        .get(index + 1)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| {
            eprintln!("--pages wants a positive integer");
            std::process::exit(2);
        });
    argv.drain(index..=index + 1);
    value.max(1)
}

fn main() {
    let mut argv: Vec<String> = std::env::args().collect();
    let pages = take_pages_flag(&mut argv);
    if argv.len() < 5 {
        eprintln!(
            "usage: p0_profile [--pages N] <formats_repo> <input_db> <format> <report.pdf> [out_dir]"
        );
        std::process::exit(2);
    }
    let formats_repo = argv[1].clone();
    let input_db = argv[2].clone();
    let format = argv[3].clone();
    let pdf = argv[4].clone();

    // Every artifact of this run — outputs, `.log.csv` — goes under the output directory, never
    // in the working directory.
    let temp = tempfile::tempdir().expect("cannot create the temporary output directory");
    let out_dir: PathBuf =
        argv.get(5).map(PathBuf::from).unwrap_or_else(|| temp.path().join("out"));
    // An explicit empty config file keeps `find_config` from picking up whatever sits in the
    // working directory, which would silently change what is being measured.
    let empty_config = temp.path().join("empty.yaml");
    std::fs::write(&empty_config, "").expect("cannot write the empty config file");
    for (key, _) in std::env::vars() {
        if key.starts_with("FREEPORTS_") {
            unsafe { std::env::remove_var(&key) };
        }
    }

    tracing_subscriber::registry()
        .with(TimingLayer.with_filter(SpansAlwaysEventsAtWarn))
        .init();

    let args = CliArgs::parse_from([
        "freeports",
        "--config",
        &empty_config.to_string_lossy(),
        "-i",
        &pdf,
        "-f",
        &format,
        "-T",
        "TEST",
        "-I",
        &input_db,
        "-r",
        &formats_repo,
        "-o",
        &out_dir.to_string_lossy(),
    ]);

    let t_total = Instant::now();

    let t = Instant::now();
    let configs = run::resolve_configs(args).expect("the configuration must resolve");
    let config = configs.into_iter().next().expect("exactly one non-batch configuration");
    let d_config = t.elapsed();

    let t = Instant::now();
    let algorithm = Algorithm::load(
        config.formats_repo_path.as_deref().expect("a formats repo path was given"),
        &FormatName::new(config.format.clone()),
    )
    .expect("the algorithm must load");
    let d_algorithm = t.elapsed();

    let t = Instant::now();
    let companies = compile_target_companies(
        config.input_db_path.as_deref().expect("an input db path was given"),
        &config.target_lists,
    )
    .expect("the target companies must compile");
    let d_companies = t.elapsed();

    // Question 1. One call, one `Instant`: PyMuPDF holds the GIL for its whole duration, so this
    // is the number that decides how much P2 (threads) can possibly buy.
    let spec = config.reports.first().expect("one document per invocation here");
    let path = spec.path.clone().expect("a local pdf path was given");
    let t = Instant::now();
    let document = load_document(
        Path::new(&path),
        spec.name.clone().unwrap_or_default(),
        config.format.clone(),
        true,
    )
    .expect("the document must load");
    let d_load = t.elapsed();
    let page_count = document.pages.len();

    // Questions 2 and 3: everything inside is broken down by the span table below.
    let t = Instant::now();
    // `--pages 1` e' il percorso sequenziale di sempre: e' il termine di paragone con cui si
    // misura il guadagno di P2. Con `--pages N` la tabella degli span qui sotto va letta con
    // attenzione -- i tempi si sommano **per thread**, quindi il tempo inclusivo di uno span puo'
    // superare il tempo di parete. Il numero che conta per il confronto e' `apply_multidocument`,
    // che e' misurato con un `Instant` e resta tempo di parete.
    let outcomes = algorithm
        .apply_multidocument_with(&[document], &companies, Parallelism::pages(pages))
        .expect("the algorithm must run");
    let d_apply = t.elapsed();

    let t = Instant::now();
    cli_output::write_results(&config, &outcomes).expect("the results must be written");
    let d_output = t.elapsed();

    let d_total = t_total.elapsed();

    // -----------------------------------------------------------------------------------------
    // Report
    // -----------------------------------------------------------------------------------------

    println!();
    println!("# {format} — {page_count} pages — {pdf} — pages at a time: {pages}");
    println!();
    println!("## Phases (wall clock, profiling layer installed)");
    println!();
    println!("| phase | ms | % of total |");
    println!("|---|---:|---:|");
    let phases = [
        ("config resolution", d_config),
        ("Algorithm::load (formats repo)", d_algorithm),
        ("compile_target_companies (input db)", d_companies),
        ("load_document_pages (PyMuPDF, GIL)", d_load),
        ("apply_multidocument (classify + steps)", d_apply),
        ("write_results (output)", d_output),
    ];
    for (name, d) in phases {
        println!("| {name} | {:.1} | {:.1}% |", ms(d), 100.0 * d.as_secs_f64() / d_total.as_secs_f64());
    }
    println!("| **total** | **{:.1}** | 100% |", ms(d_total));
    println!();
    println!("Per page: load {:.2} ms, apply {:.2} ms.", ms(d_load) / page_count as f64, ms(d_apply) / page_count as f64);

    let totals = totals().lock().expect("the profiling map is never poisoned");
    let mut rows: Vec<(&String, &Stat)> = totals.iter().collect();
    rows.sort_by(|a, b| b.1.exclusive.cmp(&a.1.exclusive));
    let span_entries: u64 = rows.iter().map(|(_, s)| s.entries).sum();

    // Question 2 x question 3 in one table: the rows are the phases (classification, then one row
    // per execution step), the columns are the three segments plus the orchestration overhead the
    // engine spends outside them. Every cell is *own* time, so the whole table sums to the engine's
    // work without double counting.
    let mut matrix: HashMap<(String, &'static str), Duration> = HashMap::new();
    for (path, stat) in rows.iter() {
        let phase = match path.split('/').next() {
            Some("classify") => "classify".to_string(),
            Some(step) if step.starts_with("step[") => step.to_string(),
            Some(other) => other.to_string(),
            None => "?".to_string(),
        };
        let segment = if path.split('/').any(|c| c == "pdf_extract") {
            "pdf_extract"
        } else if path.split('/').any(|c| c == "text_filter") {
            "text_filter"
        } else if path.split('/').any(|c| c == "deserialize") {
            "deserialize"
        } else {
            "orchestration"
        };
        *matrix.entry((phase, segment)).or_default() += stat.exclusive;
    }
    let mut phases_seen: Vec<String> = matrix.keys().map(|(p, _)| p.clone()).collect();
    phases_seen.sort();
    phases_seen.dedup();
    const SEGMENTS: [&str; 4] = ["pdf_extract", "text_filter", "deserialize", "orchestration"];

    println!();
    println!("## Classification vs steps x the three segments (own time, ms)");
    println!();
    println!("| phase | pdf_extract | text_filter | deserialize | orchestration | row total |");
    println!("|---|---:|---:|---:|---:|---:|");
    for phase in &phases_seen {
        let cells: Vec<Duration> = SEGMENTS
            .iter()
            .map(|seg| matrix.get(&(phase.clone(), *seg)).copied().unwrap_or_default())
            .collect();
        let row_total: Duration = cells.iter().sum();
        println!(
            "| `{phase}` | {:.1} | {:.1} | {:.1} | {:.1} | **{:.1}** |",
            ms(cells[0]), ms(cells[1]), ms(cells[2]), ms(cells[3]), ms(row_total)
        );
    }
    let col_totals: Vec<Duration> = SEGMENTS
        .iter()
        .map(|seg| {
            phases_seen
                .iter()
                .map(|p| matrix.get(&(p.clone(), *seg)).copied().unwrap_or_default())
                .sum()
        })
        .collect();
    println!(
        "| **total** | **{:.1}** | **{:.1}** | **{:.1}** | **{:.1}** | **{:.1}** |",
        ms(col_totals[0]), ms(col_totals[1]), ms(col_totals[2]), ms(col_totals[3]),
        ms(col_totals.iter().copied().sum::<Duration>())
    );

    // Question 3, second half: what a single pipe costs. `avg` is the per-call cost, which is what
    // decides whether a parallel split can ever pay for its distribution overhead.
    println!();
    println!("## Pipes, by own time");
    println!();
    println!("| pipe | phase | calls | own ms | avg ms/call |");
    println!("|---|---|---:|---:|---:|");
    for (path, stat) in rows.iter().filter(|(p, _)| p.contains("/pipe[")).take(15) {
        let pipe = path.rsplit('/').find(|c| c.starts_with("pipe[")).unwrap_or(path);
        let phase = path.split('/').next().unwrap_or("?");
        println!(
            "| `{pipe}` | `{phase}` | {} | {:.1} | {:.3} |",
            stat.entries,
            ms(stat.exclusive),
            ms(stat.exclusive) / stat.entries.max(1) as f64
        );
    }

    println!();
    println!("## Span paths, by own (exclusive) time");
    println!();
    println!("({span_entries} span entries recorded; `page` at the root of a path is the per-page \
span PyMuPDF loading opens, not an engine span.)");
    println!();
    println!("| span path | entries | inclusive ms | own ms | own % of total |");
    println!("|---|---:|---:|---:|---:|");
    for (path, stat) in rows.iter().take(45) {
        println!(
            "| `{path}` | {} | {:.1} | {:.1} | {:.1}% |",
            stat.entries,
            ms(stat.inclusive),
            ms(stat.exclusive),
            100.0 * stat.exclusive.as_secs_f64() / d_total.as_secs_f64()
        );
    }
    println!();
}
