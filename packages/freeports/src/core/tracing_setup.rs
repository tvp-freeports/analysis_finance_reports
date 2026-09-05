//! The crate's one logging subsystem: `tracing`, and nothing else.
//!
//! A run has three independent destinations, each a `tracing_subscriber` layer composed onto one
//! `Registry`. They are independent on purpose: what a person reads while watching a run and what a
//! tool reads afterwards are not the same thing, and trying to serve both from one stream makes
//! each worse.
//!
//! | Destination | What it is for | Level |
//! |---|---|---|
//! | stderr | watching a run happen | [`Verbosity`] |
//! | `freeports.log.jsonl` | one JSON object per line, for tools | [`Verbosity`] |
//! | `.log.csv` | the extraction's own audit trail, anchored to pages and coordinates | `warn` and above |
//!
//! # Verbosity
//!
//! [`Verbosity`] is a six-level scale, and `-v` and `-q` are **independent dials** rather than
//! mutually exclusive flags: the net offset `verbose - quiet` is added to the default and clamped,
//! so no combination of counts can produce an error or an out-of-range level.
//!
//! | flags | level |
//! |---|---|
//! | none | `Warn` |
//! | `-q` | `ErrorOnly` |
//! | `-qq` or more | `Silent` |
//! | `-v` | `Info` |
//! | `-vv` | `Debug` |
//! | `-vvv` or more | `Trace` |
//!
//! # What puts a row in `.log.csv`
//!
//! For each event the layer merges the event's own fields with those of **every** active span, from
//! the outermost inwards; on a name clash the innermost span wins, and the event's own fields win
//! over any span. A row is written only if the merged set carries at least one of the five tagged
//! fields — `page`, `coord_ref_1`, `coord_ref_2`, `coord_1`, `coord_2`. Columns whose field is
//! absent stay empty cells.
//!
//! | tracing field | CSV column |
//! |---|---|
//! | `report` | `Report` |
//! | `page` | `Page` |
//! | *(computed from the active spans)* | `Activity` |
//! | `coord_ref_1` | `First coord ref` |
//! | `coord_ref_2` | `Second coord ref` |
//! | `coord_1` | `First coord` |
//! | `coord_2` | `Second coord` |
//! | *(the event's own level)* | `Level` |
//! | the event's `message` | `Message` |
//!
//! `Level` is the one column that comes from neither a field nor the span stack: it is
//! `event.metadata().level()`, read directly. That is not an implementation shortcut but the
//! requirement — a level must **never** be inheritable, and a `warn!` under an `info_span` is a
//! `warn`. Being metadata it also cannot select a row: every event has a level, so a level that
//! counted as a tagged field would turn this file into a transcript of the program, which is
//! exactly what the page-or-coordinate rule exists to prevent.
//!
//! `report` is **not** one of the tagged five. A document's name is not a position, and a warning
//! that names only a report is not an audit-trail entry — so it fills its column wherever a row
//! exists and never writes one by itself. It comes from the `document` span, which is what lets an
//! event born deep inside a pipe carry it without knowing it.
//!
//! `Activity`, like `Report`, enriches a row but never justifies one on its own — it is derived
//! from the span stack rather than recorded at all. The two `coord_ref_*` fields are textual anchors to a position,
//! and are set on a **span** wrapping the deserialization of a row rather than on each event, which
//! is how `tracing` is meant to give context to everything beneath it. `coord_ref_1` is the
//! **triggering text** — the report's own words, which a search inside the PDF can find again —
//! while `coord_ref_2` is whichever second anchor the event has: the company that text matched, or
//! the field the row is about.
//!
//! # One row per event, not three per failure
//!
//! A lost field is one row saying both what went wrong and what was done about it
//! (`"Error casting, skipping field: …"`), not an error row plus two warnings about mitigation and
//! consequence. The level already carries the severity and the message the consequence. A
//! **successful** mitigation does stay a row of its own, because nothing was lost and that is
//! different information.
//!
//! CSV escaping is delegated entirely to the `csv` crate's defaults; nothing is quoted by hand.
//!
//! # Lifecycle
//!
//! The header row of `.log.csv` is written and flushed before [`CsvLogLayer::create`] returns. Data
//! rows are **accumulated**, then sorted and written by an explicit [`CsvLogLayer::close`].
//!
//! The explicit close is the only supported mechanism, because the global subscriber installed by
//! [`init`] is never dropped: `set_global_default` installs a `'static` dispatcher, so process exit
//! flushes nothing. `Drop` remains a best-effort safety net for uses that install a subscriber for
//! a scope, but it is not the contract.
//!
//! [`init`] likewise opens **every** log file before attempting `set_global_default`. A bad log
//! directory must fail without installing anything: a process gets one global subscriber, and
//! burning it on a call that then fails would leave the run with no logging at all and no way to
//! retry.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tracing::field::{Field, Visit};
use tracing::span;
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, FormattedFields, MakeWriter};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{Layer, filter::LevelFilter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Verbosity {
    Silent,
    ErrorOnly,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Verbosity {
    /// Increasing order of verbosity, used both by the clamping in
    /// [`Verbosity::from_verbose_and_quiet_counts`] and by the tests that iterate every level.
    pub const ORDER: [Verbosity; 6] = [
        Verbosity::Silent,
        Verbosity::ErrorOnly,
        Verbosity::Warn,
        Verbosity::Info,
        Verbosity::Debug,
        Verbosity::Trace,
    ];
    /// The index into [`Verbosity::ORDER`] used when neither `-v` nor `-q` appears.
    pub const DEFAULT_INDEX: usize = 2; // Warn

    /// `-v` and `-q` are **independent dials**, summed with sign against
    /// [`Verbosity::DEFAULT_INDEX`], rather than mutually exclusive flags.
    ///
    /// The net offset is clamped to the range of [`Verbosity::ORDER`], so no pair of counts —
    /// including the extremes of `u8` — can panic or produce an out-of-range level.
    pub fn from_verbose_and_quiet_counts(verbose: u8, quiet: u8) -> Verbosity {
        let offset = i16::from(verbose) - i16::from(quiet);
        let last = (Self::ORDER.len() - 1) as i16;
        let index = (Self::DEFAULT_INDEX as i16 + offset).clamp(0, last);
        Self::ORDER[index as usize]
    }

    /// The level filter for this verbosity.
    ///
    /// A `LevelFilter` rather than a `tracing::Level`, because `Silent` has no corresponding level:
    /// the absence of a level is not a level.
    pub fn level_filter(self) -> LevelFilter {
        match self {
            Verbosity::Silent => LevelFilter::OFF,
            Verbosity::ErrorOnly => LevelFilter::ERROR,
            Verbosity::Warn => LevelFilter::WARN,
            Verbosity::Info => LevelFilter::INFO,
            Verbosity::Debug => LevelFilter::DEBUG,
            Verbosity::Trace => LevelFilter::TRACE,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TracingSetupError {
    #[error("cannot open log file at {}: {source}", path.display())]
    OpenLogFile { path: PathBuf, source: std::io::Error },
    #[error("cannot open csv log file at {}: {source}", path.display())]
    OpenCsvFile { path: PathBuf, source: std::io::Error },
    #[error("failed to write a row to the .log.csv file: {source}")]
    CsvWrite { source: csv::Error },
    #[error("cannot install the global tracing subscriber: {source}")]
    AlreadyInitialized { source: tracing::subscriber::SetGlobalDefaultError },
    #[error("the .log.csv destination was never set")]
    CsvDestinationUnset,
}

/// Level of every orchestration span in the crate (`run`, `job`, `page`, `class`, `pipeline`,
/// `pipe`, the three segments, …). They are opened with `info_span!`, and `EventLevelFilter`
/// below is built around that fact — change one and you must change the other.
const SPAN_LEVEL: LevelFilter = LevelFilter::INFO;

/// Filters **events** by level while letting **every span through unconditionally**.
///
/// A plain `LevelFilter` cannot be used on these layers, and the reason is not obvious: a
/// per-layer filter gates span creation too. With `LevelFilter::WARN` the crate's `info_span!`s
/// are never opened, so a `warn!` fired deep inside a page loses its whole context — no
/// `page[353]` on the stderr line, and no `Page` column in `.log.csv`, because the CSV row is
/// selected precisely by the `page` field it inherits from that span. Measured, not theorized:
/// with a plain `LevelFilter` the 391 real `warn!`s of a EURIZON-EN23 job produced **zero** CSV
/// rows instead of 391.
///
/// So spans always pass and only events are levelled. The price is `max_level_hint` reporting
/// `SPAN_LEVEL` (`INFO`) rather than the event level: at the default `Warn`, `info!` events are
/// still constructed and then dropped by every layer. That is a handful of once-per-step sites,
/// and it costs nothing next to what the hint does buy — `debug!` and `trace!`, which is where
/// the hot loops live, stay switched off at the callsite.
#[derive(Debug, Clone, Copy)]
pub struct EventLevelFilter {
    level: LevelFilter,
}

impl EventLevelFilter {
    fn new(level: LevelFilter) -> Self {
        Self { level }
    }
}

impl<S> tracing_subscriber::layer::Filter<S> for EventLevelFilter {
    fn enabled(&self, meta: &tracing::Metadata<'_>, _cx: &Context<'_, S>) -> bool {
        meta.is_span() || LevelFilter::from_level(*meta.level()) <= self.level
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        // `OFF` means "nothing at all", spans included: `-qq` must stay completely silent rather
        // than keep paying for span bookkeeping nobody will ever read.
        if self.level == LevelFilter::OFF {
            Some(LevelFilter::OFF)
        } else {
            Some(std::cmp::max(self.level, SPAN_LEVEL))
        }
    }
}

/// Renders a span's fields as **values only**, with no `key=` prefix — the half of
/// [`SpanPathFormat`] that turns `class{class=investments}` into `class[investments]`. Only ever
/// used to build the `FormattedFields` of a *span*: [`SpanPathFormat`] formats an event's own
/// fields itself, where `key=value` is still the useful form.
///
/// A span with no fields yields an empty string, which [`SpanPathFormat`] renders as the bare span
/// name, with no empty `[]`.
///
/// # One field bare, several fields quoted
///
/// A span's value is very often **text copied out of the report** — a company as the document
/// spells it, a fund's name. That text contains spaces, dashes and sometimes commas, which are
/// exactly the characters a separator would be made of, so as soon as a second value stands beside
/// it there is no way to see where it ends:
///
/// ```text
/// investment[EXXON MOBIL CORP - 110.00 - 16.08.24 PUT,row 12,col 6]
/// ```
///
/// Hence the rule: **one field renders bare, several render quoted**, comma-separated.
///
/// ```text
/// investment["EXXON MOBIL CORP - 110.00 - 16.08.24 PUT","row 12","col 6"]
/// job[AMUNDI-EN24]
/// page[53]
/// ```
///
/// It is structural rather than a guess about content, and it costs nothing where nothing is
/// ambiguous: every span in the crate but `investment` carries exactly one field, so the noisy
/// `pipe{pipe="PdfExtractInvestmentsStandard"}` this formatter was written to remove does not come
/// back. Quoting is [`quote_value`]'s, not `Debug`'s, for the reason given there.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpanValueFields;

/// Wraps `value` in `"` and doubles any `"` inside it — CSV's convention, and deliberately not
/// `{:?}`.
///
/// `Debug` escapes non-ASCII and backslashes, and the whole point of these values is that they can
/// be pasted into a PDF viewer's search box: a company printed `Sté Générale` must not reach the
/// terminal as `St\u{e9} G\u{e9}n\u{e9}rale`.
fn quote_value(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Collects the values of every field it visits, in declaration order. `record_str` keeps the
/// string raw; the decision to quote is taken afterwards, by [`SpanValueFields::format_fields`],
/// because it depends on **how many** fields there turned out to be and a visitor cannot know that
/// while it is still visiting. That is also why the values are buffered rather than written
/// straight through.
#[derive(Default)]
struct ValueListVisitor {
    values: Vec<String>,
}

impl Visit for ValueListVisitor {
    fn record_debug(&mut self, _field: &Field, value: &dyn fmt::Debug) {
        self.values.push(format!("{value:?}"));
    }

    fn record_str(&mut self, _field: &Field, value: &str) {
        self.values.push(value.to_string());
    }
}

impl<'writer> FormatFields<'writer> for SpanValueFields {
    fn format_fields<R: tracing_subscriber::field::RecordFields>(
        &self,
        mut writer: Writer<'writer>,
        fields: R,
    ) -> fmt::Result {
        let mut visitor = ValueListVisitor::default();
        fields.record(&mut visitor);
        match visitor.values.as_slice() {
            [] => Ok(()),
            [only] => write!(writer, "{only}"),
            several => {
                let quoted: Vec<String> = several.iter().map(|v| quote_value(v)).collect();
                write!(writer, "{}", quoted.join(","))
            }
        }
    }
}

/// Event format of **stderr only**. One line:
///
/// ```text
/// DEBUG run/job[EURIZON-EN23]/page[353]: message key=value
/// ```
///
/// Three deliberate differences from `tracing_subscriber`'s default `Format<Full>`:
///
/// 1. spans are joined with `/` and carry their identifying value in brackets (`page[353]`),
///    instead of `:`-joined `name{field=value}` pairs that repeat the span name inside its own
///    braces (`page{page=353}`);
/// 2. the resulting path has the same **shape** as the `.log.csv` `Activity` column (see
///    `activity_path`), so a line on stderr and a row in the CSV name the same place in the same
///    vocabulary. The two are not the same string, and the difference is deliberate: `SpanLabel`
///    renders a span's **first** field, because `Activity` sits beside columns that already hold
///    the coordinates, while stderr renders **all** of them through [`SpanValueFields`], because
///    there the coordinates have nowhere else to appear. A multi-field span therefore reads
///    `investment["…","row 12","col 6"]` here and `investment[…]` there;
/// 3. **no timestamp and no `target`**. The module path (`freeports::core::algorithm`) is the
///    longest token on the line and almost never what a person watching a live run is looking for,
///    while the wall clock is only useful afterwards. Neither is lost: `freeports.log.jsonl` carries
///    both on every record, in a form a machine can filter on.
///
/// Four colors, not one, so the eye can take the path apart without reading it: the structure
/// recedes, the names carry the shape, the values stand out.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpanPathFormat {
    ansi: bool,
}

/// Reset sequence closing every colored run below.
const ANSI_RESET: &str = "\x1b[0m";
/// Cyan: the **names** of the spans, the skeleton of the path — `run`, `job`, `page`, `pipe`.
const ANSI_ACTIVITY: &str = "\x1b[36m";
/// Bright magenta: the **values** inside the brackets — `EURIZON-EN23`, `353`. Deliberately a
/// different hue from the names rather than a different shade of the same one: the user's request
/// was to tell segments and their parameters apart *at a glance*, and hue is what does that.
const ANSI_PARAM: &str = "\x1b[95m";
/// Bright black, i.e. the terminal's dark grey: the punctuation that holds the path together —
/// the `/` between segments and the `[`/`]` around the values. It is pure structure, so it is the
/// one part that should recede.
const ANSI_SEPARATOR: &str = "\x1b[90m";
/// Dim: the `key=value` tail of an event's own fields.
const ANSI_FIELDS: &str = "\x1b[2m";

impl SpanPathFormat {
    /// `ansi` colors the line. It is the format's only knob left: with `freeports.log` now
    /// structured (`JsonLogLayer`), this formatter has exactly one destination — stderr, read
    /// live while the run is in front of you.
    fn new(ansi: bool) -> Self {
        Self { ansi }
    }

    /// `\x1b[…m` codes written by hand rather than pulled from a color crate: five level names,
    /// four fixed colors for the other segments, nothing a dependency would do better.
    fn level_color(level: &tracing::Level) -> &'static str {
        match *level {
            tracing::Level::ERROR => "\x1b[31m",
            tracing::Level::WARN => "\x1b[33m",
            tracing::Level::INFO => "\x1b[32m",
            tracing::Level::DEBUG => "\x1b[34m",
            tracing::Level::TRACE => "\x1b[35m",
        }
    }

    /// `(prefix, suffix)` for one colored segment — both empty when ANSI is off, so every call
    /// site stays a single `write!` instead of an `if` around each one.
    fn paint(&self, color: &'static str) -> (&'static str, &'static str) {
        if self.ansi { (color, ANSI_RESET) } else { ("", "") }
    }
}

/// Writes an event's own fields as `key="value"`, skipping `message` (which the caller has already
/// written as the line's text). The mirror image of `ValueListVisitor`, which drops the keys.
///
/// The tail carries the same ambiguity a span's brackets do — `coord_ref_1=Leonardo Spa Az Nom
/// coord_ref_2=Leonardo` gives no clue where the first value ends — so a value **containing a
/// space** is quoted. One that does not cannot bleed into the next pair: it ends at the space that
/// precedes the next key, and `page=53` gains nothing from quotes.
///
/// The condition is on the space rather than on the field's type because the tail mixes the two
/// freely: `coord_1=%row` renders `row 12` through `Debug`, and a `rows=17` renders a number
/// through the same method.
struct EventFieldVisitor<'a> {
    writer: Writer<'a>,
    result: fmt::Result,
}

impl EventFieldVisitor<'_> {
    fn write(&mut self, field: &Field, value: &str) {
        // `message` is already the line's text. `error` is deliberately skipped too: by
        // convention every site that attaches one with `log_error` also interpolates it into its
        // message, so printing the field as well would say the same thing twice on the same line
        // — precisely the repetitiveness this format set out to remove. The structured copy is
        // not lost, it is what `freeports.log.jsonl` serializes.
        if self.result.is_err() || field.name() == "message" || field.name() == "error" {
            return;
        }
        let rendered =
            if value.contains(' ') || value.contains('"') { quote_value(value) } else { value.to_string() };
        self.result = write!(self.writer, " {}={}", field.name(), rendered);
    }
}

impl Visit for EventFieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.write(field, &format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.write(field, value);
    }
}

/// Extracts an event's `message` field, so the line can put it before the `key=value` tail
/// instead of in field position.
struct MessageVisitor(Option<String>);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0 = Some(format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0 = Some(value.to_string());
        }
    }
}

impl<S, N> FormatEvent<S, N> for SpanPathFormat
where
    S: Subscriber + for<'span> LookupSpan<'span>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let level = event.metadata().level();
        let (on, off) = self.paint(Self::level_color(level));
        write!(writer, "{on}{level:>5}{off} ")?;

        // The span path, rendered exactly like the `.log.csv` `Activity` column but taken apart
        // into three colors so the shape reads without being read: grey punctuation, cyan names,
        // magenta values.
        let (name_on, name_off) = self.paint(ANSI_ACTIVITY);
        let (param_on, param_off) = self.paint(ANSI_PARAM);
        let (sep_on, sep_off) = self.paint(ANSI_SEPARATOR);
        let mut first = true;
        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                if !first {
                    write!(writer, "{sep_on}/{sep_off}")?;
                }
                first = false;
                write!(writer, "{name_on}{}{name_off}", span.name())?;
                let extensions = span.extensions();
                if let Some(fields) = extensions.get::<FormattedFields<N>>()
                    && !fields.is_empty()
                {
                    write!(
                        writer,
                        "{sep_on}[{sep_off}{param_on}{fields}{param_off}{sep_on}]{sep_off}"
                    )?;
                }
            }
        }
        if !first {
            write!(writer, "{sep_on}:{sep_off} ")?;
        }

        let mut message = MessageVisitor(None);
        event.record(&mut message);
        if let Some(text) = message.0 {
            write!(writer, "{text}")?;
        }

        let (on, off) = self.paint(ANSI_FIELDS);
        write!(writer, "{on}")?;
        let mut fields = EventFieldVisitor { writer: writer.by_ref(), result: Ok(()) };
        event.record(&mut fields);
        fields.result?;
        write!(writer, "{off}")?;

        writeln!(writer)
    }
}

/// Shared builder behind `stderr_layer`, parameterized over the writer so tests can inject an
/// in-memory buffer instead of real stderr and assert on the layer's actual formatted output —
/// see `tests::stderr_layer_observable_filtering`.
fn fmt_layer_with_writer<S, W>(
    writer: W,
    filter: LevelFilter,
    ansi: bool,
) -> impl Layer<S> + std::fmt::Debug
where
    S: Subscriber + for<'span> LookupSpan<'span> + std::fmt::Debug,
    W: for<'writer> MakeWriter<'writer> + 'static + std::fmt::Debug,
{
    tracing_subscriber::fmt::layer()
        .fmt_fields(SpanValueFields)
        .event_format(SpanPathFormat::new(ansi))
        .with_writer(writer)
        .with_filter(EventLevelFilter::new(filter))
}

pub fn stderr_layer<S>(verbosity: Verbosity) -> impl Layer<S> + std::fmt::Debug
where
    S: Subscriber + for<'span> LookupSpan<'span> + std::fmt::Debug,
{
    fmt_layer_with_writer(
        std::io::stderr as fn() -> std::io::Stderr,
        verbosity.level_filter(),
        true,
    )
}

/// A `BufWriter<File>` behind a shared `Mutex`, so the same buffer can be both the destination
/// [`JsonLogLayer`] writes its lines into and the thing [`LogHandle::close`] flushes.
///
/// Buffering is not a micro-optimisation here. Writing straight into a bare `File` meant one
/// `write(2)` per event — 33,086 unbuffered syscalls for a single 1140-page job — and it happened
/// at a hardcoded `DEBUG` regardless of verbosity.
#[derive(Debug, Clone)]
pub struct SharedFileWriter(Arc<Mutex<BufWriter<File>>>);

impl SharedFileWriter {
    /// Appends one already-serialized record and its newline. Called once per event from inside
    /// a `Layer::on_event`, which has no channel to report a failure back through — hence the
    /// `io::Result` returned here and deliberately dropped by its caller (see `JsonLogLayer`).
    fn write_line(&self, line: &str) -> std::io::Result<()> {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        guard.write_all(line.as_bytes())?;
        guard.write_all(b"\n")
    }

    /// Appends bytes verbatim, without adding a newline (P1: the JSON Lines a worker process
    /// already wrote, newlines included). Goes through the same writer as every other line, so
    /// the absorbed records land after the parent's instead of racing its buffer.
    fn append_raw(&self, bytes: &[u8]) -> std::io::Result<()> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).write_all(bytes)
    }

    fn flush(&self) -> std::io::Result<()> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).flush()
    }
}

/// `freeports.log.jsonl`, the diagnostic log — **structured** since L5, at the same level as
/// stderr. See [`JsonLogLayer`] for why JSON Lines and not a single JSON or YAML document.
pub fn file_layer<S>(
    path: &Path,
    verbosity: Verbosity,
) -> Result<(impl Layer<S> + std::fmt::Debug, SharedFileWriter), TracingSetupError>
where
    S: Subscriber + for<'span> LookupSpan<'span> + std::fmt::Debug,
{
    let file = File::create(path)
        .map_err(|source| TracingSetupError::OpenLogFile { path: path.to_path_buf(), source })?;
    let writer = SharedFileWriter(Arc::new(Mutex::new(BufWriter::new(file))));
    let layer = JsonLogLayer::new(writer.clone())
        .with_filter(EventLevelFilter::new(verbosity.level_filter()));
    Ok((layer, writer))
}


/// The `First coord` / `Second coord` pair naming a cell of a page's table, built from the grid's
/// own **zero-based** indices.
///
/// The grid counts from zero, the log counts from one: someone matching a row against the page
/// counts `1, 2, 3`, and a coordinate is written to be read by a person, not fed back into the
/// grid. The `table row` / `table col` metadata a text filter passes to a deserializer stays
/// zero-based, because that one *is* an index — which is exactly why the conversion lives here, in
/// the one place both segments call, rather than being applied twice and drifting apart.
pub fn table_coords(row: i64, col: i64) -> (String, String) {
    (format!("row {}", row + 1), format!("col {}", col + 1))
}

pub const CSV_HEADER: &str =
    "Report,Page,Activity,First coord ref,Second coord ref,First coord,Second coord,Level,Message\n";

/// Tracing field names that select a `.log.csv` row when at least one of them is present, either
/// directly on the event or inherited from an enclosing span. See the module documentation.
///
/// Kept separate from `message`, which always feeds the `Message` column but never on its own
/// triggers a row. `Activity` deliberately never appears here: it is never recorded into
/// `CapturedFields`, it is computed separately in `on_event` from the active span names — see
/// `activity_path`.
const TAGGED_FIELDS: [&str; 5] = ["page", "coord_ref_1", "coord_ref_2", "coord_1", "coord_2"];

/// Fields that fill a `.log.csv` column but never select a row, the way `message` does not either.
///
/// `report` is here rather than in [`TAGGED_FIELDS`] because a document's name is not a position:
/// the rule that keeps this file an audit trail of the *extraction* is that a row exists only where
/// the event names a page or a coordinate, and a warning carrying nothing but a report name would
/// break it.
const COLUMN_ONLY_FIELDS: [&str; 1] = ["report"];

/// Field values collected from a single event or span, restricted to the columns `CsvLogLayer`
/// actually cares about (the five tagged fields, the column-only ones, and the event's own
/// `message`) — other fields are deliberately not stored, they never reach a CSV column.
#[derive(Debug, Default, Clone)]
struct CapturedFields(HashMap<&'static str, String>);

impl CapturedFields {
    fn get(&self, name: &str) -> &str {
        self.0.get(name).map(String::as_str).unwrap_or("")
    }

    /// Overwrites `self` with every field present in `other` — used both to fold an outer span's
    /// fields under an inner one (innermost wins) and to fold an event's own fields on top of
    /// the merged span fields (the event always wins).
    fn merge_from(&mut self, other: &CapturedFields) {
        for (&name, value) in &other.0 {
            self.0.insert(name, value.clone());
        }
    }

    fn has_any_tagged_field(&self) -> bool {
        TAGGED_FIELDS.iter().any(|field| self.0.contains_key(field))
    }
}

struct FieldVisitor(CapturedFields);

impl FieldVisitor {
    fn new() -> Self {
        Self(CapturedFields::default())
    }

    /// The five tagged fields, the column-only ones and `message` are the only fields that ever
    /// reach a CSV column.
    fn keeps(field: &Field) -> bool {
        let name = field.name();
        name == "message" || TAGGED_FIELDS.contains(&name) || COLUMN_ONLY_FIELDS.contains(&name)
    }

    /// **Only ever called after `keeps` returned true.** Every `record_*` below tests the field
    /// name *before* rendering the value: formatting first and discarding after made every field of
    /// every event in the crate pay a `format!` allocation just to be thrown away.
    fn record(&mut self, field: &Field, value: String) {
        self.0.0.insert(field.name(), value);
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if Self::keeps(field) {
            self.record(field, format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if Self::keeps(field) {
            self.record(field, value.to_string());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if Self::keeps(field) {
            self.record(field, value.to_string());
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if Self::keeps(field) {
            self.record(field, value.to_string());
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if Self::keeps(field) {
            self.record(field, value.to_string());
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        if Self::keeps(field) {
            self.record(field, value.to_string());
        }
    }
}

/// The `name[value]` rendering of one span, computed once when the span is created and kept in
/// its extensions. Two reasons it is stored instead of recomputed: an event under a deep stack
/// would otherwise re-render every ancestor, and `CapturedFields` cannot serve here — it keeps
/// only the five tagged fields, while a span's identifying value (`pipeline`, `pipe`, `format`,
/// `class`, …) is none of them.
#[derive(Debug, Clone)]
struct SpanLabel(String);

/// Collects the **first non-empty** field value of a span, in declaration order, to build
/// `SpanLabel`. Deliberately distinct from `FieldVisitor` (which keeps only the fields with a
/// column) and from `ValueListVisitor` (which writes straight into a `fmt::Writer` for the
/// stderr/file layers, and does show every field).
struct SpanLabelVisitor(Option<String>);

impl SpanLabelVisitor {
    fn keep_first(&mut self, value: String) {
        if self.0.is_none() && !value.is_empty() {
            self.0 = Some(value);
        }
    }
}

impl Visit for SpanLabelVisitor {
    fn record_debug(&mut self, _field: &Field, value: &dyn std::fmt::Debug) {
        self.keep_first(format!("{value:?}"));
    }

    fn record_str(&mut self, _field: &Field, value: &str) {
        self.keep_first(value.to_string());
    }
}

impl SpanLabel {
    /// `name` when the span has no fields, `name[value]` when it has one — the vocabulary of
    /// `page[353]`, `class[investments]`, `document[EURIZON 2023]`.
    ///
    /// # Only the first field, on purpose
    ///
    /// A span label names *what the span is about*, and a span is about one thing. The `investment`
    /// span carries three fields — the triggering text and the row's two coordinates — but the
    /// latter two are a position within the row, not its identity, and they have `First coord` and
    /// `Second coord` of their own; repeating them here would only make `Activity` longer and force
    /// the whole cell to be quoted, for no information. Nothing is hidden: stderr renders a span's
    /// fields in full through `FormattedFields`, independently of this.
    ///
    /// An empty value produces no brackets either, so the unnamed page-classify pipeline renders as
    /// `pipeline` and not `pipeline[]`.
    fn build(name: &str, attrs: &span::Attributes<'_>) -> Self {
        let mut visitor = SpanLabelVisitor(None);
        attrs.record(&mut visitor);
        match visitor.0 {
            None => Self(name.to_string()),
            Some(value) => Self(format!("{name}[{value}]")),
        }
    }
}

/// `/`-separated path of the currently active spans, outermost to innermost, each rendered as
/// `name[value]` through its `SpanLabel`. The empty string if no span is active.
///
/// It is the `activity` key of the two structured logs (see `build_record`), and the same path
/// stderr prints — in the same vocabulary but not, for a multi-field span, the same string: see
/// point 2 of [`SpanPathFormat`] for which half shows what, and why.
///
/// **Calling this has a real cost**: it walks the whole span stack and allocates a `Vec` and a
/// `String` every time. Callers must invoke it only once a row is known to be emitted
/// (`CapturedFields::has_any_tagged_field()` is true), never unconditionally at the top of
/// `on_event`.
fn activity_path<S>(ctx: &Context<'_, S>, event: &Event<'_>) -> String
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    ctx.event_scope(event)
        .map(|scope| {
            scope
                .from_root()
                .map(|span| {
                    span.extensions()
                        .get::<SpanLabel>()
                        .map(|label| label.0.clone())
                        .unwrap_or_else(|| span.name().to_string())
                })
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

/// Ordering key for a `.log.csv` row. Limited today to what the crate actually tracks (only
/// `page` has a real producer): `page` itself, with an arrival counter as both tie-breaker and
/// total fallback for rows without a numbered page.
///
/// # Extending this key: two pitfalls, not just "add more fields"
///
/// 1. With `#[derive(Ord)]` on a plain struct, fields compare **in declaration order**. A future
///    "document, page, step, sequence" key must declare `document`/`step` **before** `page`, not
///    append them after `sequence` — appending at the end compiles fine but sorts silently
///    wrong (by page first, by document second — the opposite of the intended hierarchy).
/// 2. A future `document` field must be a job **index** (`u64`, assigned in execution order), not
///    a document id/name `String`: sorting by a `String` id would produce alphabetical order,
///    while the batch behaviour is arrival order of jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RowOrderKey {
    page: PageKey,
    sequence: u64,
}

/// Rows with a numbered page always sort before rows without one (`Numbered` declared before
/// `Unnumbered`: declaration order drives `derive(Ord)` for a field-less enum comparison). No
/// real fixture exercises this today — see `tests::csv_layer::row_ordering`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PageKey {
    Numbered(u64),
    Unnumbered,
}

#[derive(Debug, Clone)]
struct PendingRow {
    order_key: RowOrderKey,
    cells: [String; 9],
}

#[derive(Debug, Clone)]
pub struct CsvLogLayer {
    inner: Arc<CsvLogLayerInner>,
}

#[derive(Debug)]
struct CsvLogLayerInner {
    /// `None` until a destination is known. The CLI cannot know it at `init` time: `.log.csv`
    /// belongs next to the output files, and *where the output goes* is a configuration value
    /// that is only resolved once logging is already running (config resolution logs). Rows are
    /// accumulated in memory and written only at `close()` anyway (L1's determinism requirement),
    /// so the file can be opened late without losing a single row.
    file: Mutex<Option<File>>,
    rows: Mutex<Vec<PendingRow>>,
    sequence: AtomicU64,
}

impl CsvLogLayerInner {
    /// Sorts every row accumulated so far by `RowOrderKey` and writes it, then flushes. Takes
    /// the rows out of the buffer (`mem::take`) before writing, so a second call (or `Drop`
    /// after a `close()` that already ran) is a no-op instead of duplicating anything.
    fn flush_rows(&self) -> Result<(), TracingSetupError> {
        let mut rows_guard = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
        let mut pending = std::mem::take(&mut *rows_guard);
        drop(rows_guard);

        if pending.is_empty() {
            return Ok(());
        }
        pending.sort_by_key(|row| row.order_key);

        let file_guard = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(file) = file_guard.as_ref() else {
            // Unreachable through `LogHandle::close`, which always settles a destination first.
            // Rows are deliberately *not* put back: they were taken above, and re-queuing them
            // would make a second `close()` (or `Drop`) write them to a file that still does not
            // exist. Losing them silently is not an option either, hence the error.
            return Err(TracingSetupError::CsvDestinationUnset);
        };
        let mut writer = csv::WriterBuilder::new().has_headers(false).from_writer(file);
        for row in &pending {
            writer
                .write_record(&row.cells)
                .map_err(|source| TracingSetupError::CsvWrite { source })?;
        }
        writer.flush().map_err(|source| TracingSetupError::CsvWrite { source: source.into() })
    }
}

/// Best-effort safety net, **not** the supported contract: the CLI path installs a `'static` global
/// `Dispatch` that the process never drops, so [`CsvLogLayer::close`] is the only way to guarantee
/// the buffer reaches disk there.
///
/// Implemented on `CsvLogLayerInner` rather than on [`CsvLogLayer`], so it fires only when the
/// **last** `Arc` disappears, not on every dropped clone. I/O errors are swallowed here — `Drop`
/// has no channel back to a caller — and it is a no-op if the buffer is already empty.
impl Drop for CsvLogLayerInner {
    fn drop(&mut self) {
        let _ = self.flush_rows();
    }
}

impl CsvLogLayer {
    /// A layer whose destination is already known — the Python entry point, where `out_path` is
    /// an argument of the call. The header reaches disk before this returns, so the file exists
    /// even for a run that logs nothing (the format repository's integration tests compare it).
    pub fn create(path: &Path) -> Result<Self, TracingSetupError> {
        let layer = Self::deferred();
        layer.set_destination(path)?;
        Ok(layer)
    }

    /// A layer with **no destination yet** — the CLI, which learns where the output goes only
    /// after resolving the configuration. Call `set_destination` (through
    /// `LogHandle::set_csv_dir`) before `close()`.
    pub fn deferred() -> Self {
        Self {
            inner: Arc::new(CsvLogLayerInner {
                file: Mutex::new(None),
                rows: Mutex::new(Vec::new()),
                sequence: AtomicU64::new(0),
            }),
        }
    }

    /// Opens (truncating) the file at `path` and writes `CSV_HEADER` to it immediately, flushed.
    /// Replaces any previous destination; rows accumulated so far are untouched and will land in
    /// the new file.
    pub fn set_destination(&self, path: &Path) -> Result<(), TracingSetupError> {
        let mut file = File::create(path)
            .map_err(|source| TracingSetupError::OpenCsvFile { path: path.to_path_buf(), source })?;
        file.write_all(CSV_HEADER.as_bytes())
            .and_then(|()| file.flush())
            .map_err(|source| TracingSetupError::CsvWrite { source: source.into() })?;
        *self.inner.file.lock().unwrap_or_else(PoisonError::into_inner) = Some(file);
        Ok(())
    }

    /// True once a destination has been set — `LogHandle::close` uses it to decide whether it
    /// must fall back to the directory `init` was given.
    fn has_destination(&self) -> bool {
        self.inner.file.lock().unwrap_or_else(PoisonError::into_inner).is_some()
    }

    /// The only supported way to make accumulated data rows visible on disk — see
    /// `CsvLogLayerInner::flush_rows`. Idempotent: safe to call more than once, and safe to call
    /// even if `Drop` later runs too (or already ran on another clone).
    pub fn close(&self) -> Result<(), TracingSetupError> {
        self.inner.flush_rows()
    }

    /// Appends already-formatted data rows — no header — after everything this layer wrote.
    ///
    /// P1: the `.log.csv` of a worker process, absorbed into the parent's. Deliberately *after*
    /// `close()` has sorted and written the parent's own rows, and deliberately not merged into
    /// `rows`: a row read back from a CSV file has no `RowOrderKey` to be sorted by, and inventing
    /// one would put a worker's rows in an order that has nothing to do with when they happened.
    /// Grouping them per job instead is what Q-P2's answer allows, and it also reads better.
    pub fn append_rows(&self, rows: &[u8]) -> Result<(), TracingSetupError> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut file_guard = self.inner.file.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(file) = file_guard.as_mut() else {
            return Err(TracingSetupError::CsvDestinationUnset);
        };
        file.write_all(rows)
            .and_then(|()| file.flush())
            .map_err(|source| TracingSetupError::CsvWrite { source: source.into() })
    }

    /// Throws away every accumulated row **without creating any file**.
    ///
    /// This is what a run that never got a destination does at the end (L5): `.log.csv` belongs
    /// next to the output, and a run that failed before resolving its configuration has no
    /// output to sit next to. Writing it to the working directory instead — which is what
    /// happened until L5 — left a stray header-only `.log.csv` behind after every failed run,
    /// which the user asked to stop. The rows are not really lost: every one of them is an event
    /// that also reached stderr and `freeports.log.jsonl`.
    pub fn discard(&self) {
        self.inner.rows.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}

/// Stores the two pieces of per-span bookkeeping both file layers read: the tagged fields a
/// nested event inherits (`CapturedFields`) and the `name[value]` rendering of the span
/// (`SpanLabel`).
///
/// A free function rather than a method, so that no layer depends on another being installed.
/// That dependency was real and silent while a second file layer existed: with the CSV layer
/// absent, records came out with a bare `activity: page` and no coordinates at all, because the
/// labels and fields were only ever written there. Idempotent — whichever layer gets there first
/// does the work.
fn record_span_metadata<S>(attrs: &span::Attributes<'_>, id: &span::Id, ctx: &Context<'_, S>)
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    let Some(span) = ctx.span(id) else { return };
    let mut extensions = span.extensions_mut();
    if extensions.get_mut::<SpanLabel>().is_some() {
        return;
    }
    let mut visitor = FieldVisitor::new();
    attrs.record(&mut visitor);
    extensions.insert(visitor.0);
    extensions.insert(SpanLabel::build(span.name(), attrs));
}

/// Folds fields recorded on a span *after* its creation (`Span::record`) into its
/// `CapturedFields`. Companion of `record_span_metadata`, same sharing reason.
fn merge_span_fields<S>(values: &span::Record<'_>, id: &span::Id, ctx: &Context<'_, S>)
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    let mut visitor = FieldVisitor::new();
    values.record(&mut visitor);
    let Some(span) = ctx.span(id) else { return };
    let mut extensions = span.extensions_mut();
    if let Some(existing) = extensions.get_mut::<CapturedFields>() {
        existing.merge_from(&visitor.0);
    } else {
        extensions.insert(visitor.0);
    }
}

impl<S> Layer<S> for CsvLogLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        record_span_metadata(attrs, id, &ctx);
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        merge_span_fields(values, id, &ctx);
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut merged = CapturedFields::default();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if let Some(span_fields) = span.extensions().get::<CapturedFields>() {
                    merged.merge_from(span_fields);
                }
            }
        }

        let mut event_visitor = FieldVisitor::new();
        event.record(&mut event_visitor);
        merged.merge_from(&event_visitor.0);

        // Binding: check selection *before* computing `activity_path`, which walks the whole span
        // stack and must never run for an event that ends up producing no row.
        if !merged.has_any_tagged_field() {
            return;
        }

        let activity = activity_path(&ctx, event);
        let page = match merged.get("page").parse::<u64>() {
            Ok(page) => PageKey::Numbered(page),
            Err(_) => PageKey::Unnumbered,
        };
        let sequence = self.inner.sequence.fetch_add(1, Ordering::Relaxed);

        let cells = [
            merged.get("report").to_string(),
            merged.get("page").to_string(),
            activity,
            merged.get("coord_ref_1").to_string(),
            merged.get("coord_ref_2").to_string(),
            merged.get("coord_1").to_string(),
            merged.get("coord_2").to_string(),
            // Not a captured field: read straight off the event, like `activity`. See the module
            // documentation for why it must not be one.
            event.metadata().level().as_str().to_string(),
            merged.get("message").to_string(),
        ];

        let mut rows = self.inner.rows.lock().unwrap_or_else(PoisonError::into_inner);
        rows.push(PendingRow { order_key: RowOrderKey { page, sequence }, cells });
    }
}

/// Ceiling on the level `.log.csv` ever records, regardless of `-v`: **warnings and errors only**
/// (user's decision, 2026-08-30).
///
/// `.log.csv` is the register of *localized* events a format author acts on, not a full trace.
/// Two measurements stand behind the ceiling: with no filter at all the file reached **2,8 GB** on
/// a single 1140-page job, and even capped at `DEBUG` one format's reference fixture went from 28
/// rows to 3047 — neither is a file anybody opens. Below this ceiling it still follows `-q`, so
/// `-q` narrows it to errors and `-qq` silences it entirely.
pub const CSV_MAX_LEVEL: LevelFilter = LevelFilter::WARN;

fn csv_level_filter(verbosity: Verbosity) -> LevelFilter {
    std::cmp::min(verbosity.level_filter(), CSV_MAX_LEVEL)
}

/// The `.log.csv` level on the Python entry point, which has no `-v`/`-q` to follow.
///
/// `WARN`, not the `CSV_MAX_LEVEL` ceiling: on that path the file is the artifact the format
/// repository's integration tests compare, and its job there is to record what went wrong and
/// where — a cast that failed, a page skipped, a column missing. At `DEBUG` a single format's
/// reference file went from 28 rows to 3047, which is neither reviewable by a human nor a useful
/// thing to diff on every test run. It matches the CLI's own default verbosity, so both entry
/// points agree on what `.log.csv` contains unless someone asks for more with `-v`.
pub const CSV_DEFAULT_LEVEL: LevelFilter = LevelFilter::WARN;

/// The `.log.csv` filter for the Python entry point — see `CSV_DEFAULT_LEVEL` and
/// `python::api::py_run_job`.
pub fn csv_event_filter() -> EventLevelFilter {
    EventLevelFilter::new(CSV_DEFAULT_LEVEL)
}

/// File name of the per-run localized-event register, written next to the output files.
pub const CSV_FILE_NAME: &str = ".log.csv";
/// File name of the structured diagnostic log, written in the working directory. One JSON object
/// per line — see `JsonLogLayer`.
pub const LOG_FILE_NAME: &str = "freeports.log.jsonl";

/// What `init` hands back: the destinations that hold data in memory and must be settled before the
/// process exits — the accumulated `.log.csv` rows and the `freeports.log.jsonl` buffer. The global
/// `Dispatch` installed by `init` is `'static` and never dropped, so neither one reaches disk
/// without this.
#[derive(Debug, Clone)]
pub struct LogHandle {
    csv: CsvLogLayer,
    file: SharedFileWriter,
    /// What the worker processes logged, in job order, waiting to be poured into this run's own
    /// files at `close()`.
    ///
    /// Held in memory rather than merged file by file because the worker area is deleted as soon as
    /// the jobs are done, while this run's files are only written at the very end. Behind an `Arc`
    /// like the other two fields: a cloned [`LogHandle`] must share the same buffer, or the copy
    /// that receives the children's logs is not the one `close()` is called on.
    absorbed: Arc<Mutex<Vec<WorkerLogs>>>,
}

/// The two log files one worker process left behind, read verbatim (P1).
#[derive(Debug, Default)]
struct WorkerLogs {
    /// `.log.csv` **without** its header line: the parent's file already has one, and a second
    /// header in the middle would break every reader.
    csv_rows: Vec<u8>,
    jsonl: Vec<u8>,
}

impl LogHandle {
    /// Takes in the logs a worker process wrote into its private directory (P1).
    ///
    /// Called once per job, in job order, while the worker area still exists; the content is poured
    /// into this run's own files at `close()`, after everything the parent itself logged. A worker
    /// that produced no file at all — it died before writing one — is not an error here: the
    /// missing report is what reports that failure, and losing the log on top of it would only
    /// replace one diagnosis with two.
    pub fn absorb_worker_logs(&self, log_dir: &Path) -> Result<(), TracingSetupError> {
        let read = |name: &str| std::fs::read(log_dir.join(name)).unwrap_or_default();
        let logs = WorkerLogs {
            csv_rows: strip_csv_header(&read(CSV_FILE_NAME)),
            jsonl: read(LOG_FILE_NAME),
        };
        self.absorbed.lock().unwrap_or_else(PoisonError::into_inner).push(logs);
        Ok(())
    }

    /// Pours every absorbed worker log into this run's files, in job order. Each destination is
    /// attempted regardless of the others, same rule as `close()` itself.
    fn pour_absorbed(&self) -> Result<(), TracingSetupError> {
        let absorbed = std::mem::take(&mut *self.absorbed.lock().unwrap_or_else(PoisonError::into_inner));
        let mut first_error = None;
        for logs in &absorbed {
            let mut record = |result: Result<(), TracingSetupError>| {
                if let Err(e) = result {
                    first_error.get_or_insert(e);
                }
            };
            if self.csv.has_destination() {
                record(self.csv.append_rows(&logs.csv_rows));
            }
            record(self.file.append_raw(&logs.jsonl).map_err(|source| TracingSetupError::OpenLogFile {
                path: PathBuf::from(LOG_FILE_NAME),
                source,
            }));
        }
        first_error.map_or(Ok(()), Err)
    }
    /// Points `.log.csv` at `dir`, creating the directory if it does not exist yet, and writes the
    /// header there straight away.
    ///
    /// This is what puts the file **next to the output** rather than in the working directory: the
    /// CLI calls it as soon as the configuration resolves and `out_path` is known (see
    /// `cli::run::execute`). Rows logged before this call are not lost — they are held in memory
    /// until `close()` regardless of when the destination is settled.
    pub fn set_csv_dir(&self, dir: &Path) -> Result<(), TracingSetupError> {
        std::fs::create_dir_all(dir)
            .map_err(|source| TracingSetupError::OpenCsvFile { path: dir.to_path_buf(), source })?;
        self.csv.set_destination(&dir.join(CSV_FILE_NAME))
    }

    /// Flushes every destination. Attempts the `freeports.log.jsonl` flush even if the CSV one
    /// failed, so a failure in one never costs the diagnostics held by the other; the CSV error
    /// wins as the reported one, being the artifact the integration tests compare.
    pub fn close(&self) -> Result<(), TracingSetupError> {
        // No destination means no file. This is reached only when a run dies *before* the
        // configuration resolves — that is, before it is known where the outputs go. Falling back
        // to the working directory instead would leave a header-only `.log.csv` behind after every
        // failed run. The pending rows are all events that have already reached stderr and
        // `freeports.log.jsonl`.
        if !self.csv.has_destination() {
            self.csv.discard();
        }
        // Every destination is attempted even if an earlier one failed: an error on one must not
        // cost the diagnostics held by the others. The CSV wins as the reported error, being the
        // artefact the integration tests compare.
        let csv_result = self.csv.close();
        // After, never before: the parent's rows are sorted and written first, and `.log.csv` is
        // *truncated* by that write — pouring the children in earlier would mean writing them and
        // then erasing them.
        let absorbed_result = self.pour_absorbed();
        let file_result = self.file.flush();
        csv_result?;
        absorbed_result?;
        file_result.map_err(|source| TracingSetupError::OpenLogFile {
            path: PathBuf::from(LOG_FILE_NAME),
            source,
        })
    }
}

/// Drops the header line of a `.log.csv` read back from disk, leaving only data rows.
///
/// Byte-level on purpose: the rows are appended verbatim, never re-encoded, so a message containing
/// a quoted newline stays exactly the CSV record the worker wrote.
fn strip_csv_header(bytes: &[u8]) -> Vec<u8> {
    match bytes.iter().position(|&b| b == b'\n') {
        Some(end) => bytes[end + 1..].to_vec(),
        // No newline at all: a file holding nothing but a truncated header, or nothing at all.
        None => Vec::new(),
    }
}

/// A complete [`LogHandle`] **without** installing any global subscriber.
///
/// For the tests of `cli::run::execute`, which need to call `set_csv_dir` without burning the test
/// process's one `set_global_default`.
#[cfg(test)]
pub fn log_handle_for_tests(log_dir: &Path) -> Result<LogHandle, TracingSetupError> {
    let (_, file_writer) = file_layer::<tracing_subscriber::Registry>(
        &log_dir.join(LOG_FILE_NAME),
        Verbosity::Warn,
    )?;
    Ok(LogHandle { csv: CsvLogLayer::deferred(), file: file_writer, absorbed: Arc::new(Mutex::new(Vec::new())) })
}

/// Coerces a concrete error into the `&dyn Error` that `tracing` records **structurally**
/// (`Visit::record_error`) rather than as a flat string.
///
/// It exists so a log site can write `error = log_error(&e)` instead of
/// `error = &e as &(dyn std::error::Error + 'static)`, and so the coercion is impossible to get
/// subtly wrong. It is what fills the `error:` key of a `freeports.log.jsonl` line with a `Debug`
/// form, a `Display` form and the full `source()` chain — see `ErrorRecord`.
///
/// The message of such a site keeps interpolating the error as before: stderr and `.log.csv` stay
/// readable by a human, while the JSON line gets the machine-readable version of the same
/// failure.
pub fn log_error<E>(error: &E) -> &(dyn std::error::Error + 'static)
where
    E: std::error::Error + 'static,
{
    error
}

/// The error attached to one record, in **structural** form: nothing derives `Serialize` on the
/// crate's error enums, no error's shape is frozen into a serialization contract, and third-party
/// errors work too.
///
/// `debug` stands in for a type name. A `&dyn Error` cannot report its own concrete type
/// (`type_name_of_val` on a trait object answers `dyn core::error::Error`, and `Error::type_id` is
/// unstable), but `{:?}` on a `thiserror` enum already prints the variant and its fields —
/// `CastError::NotANumber { value: "n/a" }` — which is strictly more than the type name would have
/// been.
///
/// It is `pub` and `Deserialize` because the same shape carries a job's failure from a worker
/// process back to the parent, which has to *read* it. The alternative was a second, identical
/// record type in `cli::worker`: the same `source()` walk with the same cycle guard, written twice.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ErrorRecord {
    pub debug: String,
    pub display: String,
    /// The `source()` chain, outermost cause first. Empty for an error with no source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source: Vec<String>,
}

impl ErrorRecord {
    pub fn from_error(error: &(dyn std::error::Error + 'static)) -> Self {
        let mut source = Vec::new();
        let mut current = error.source();
        // Bounded on purpose: a cyclic `source()` chain is a bug in somebody's error type, but it
        // must not hang the logger.
        const MAX_DEPTH: usize = 32;
        while let Some(cause) = current {
            source.push(cause.to_string());
            if source.len() >= MAX_DEPTH {
                break;
            }
            current = cause.source();
        }
        Self { debug: format!("{error:?}"), display: error.to_string(), source }
    }
}

/// Where a record happened: the same fields `.log.csv` puts in columns, so the two structured logs
/// say the same thing. Omitted entirely when the event carries none of them.
#[derive(Debug, Clone, Default, serde::Serialize)]
struct CoordsRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    second_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    second: Option<String>,
}

impl CoordsRecord {
    fn is_empty(&self) -> bool {
        self.report.is_none()
            && self.page.is_none()
            && self.first_ref.is_none()
            && self.second_ref.is_none()
            && self.first.is_none()
            && self.second.is_none()
    }
}

/// One entry of the structured log: a line of `freeports.log.jsonl`.
#[derive(Debug, Clone, serde::Serialize)]
struct LogRecord {
    /// Wall clock, in the same format the pre-L5 text `freeports.log` printed. It moved from the
    /// line's prefix into a field, but it did not disappear: it is what tells you *when* a run
    /// spent its time, which is most of why the file is read at all.
    time: String,
    level: String,
    activity: String,
    /// The module path the event came from — `freeports::core::algorithm`. Kept here precisely
    /// because L5 removed it from stderr: too long to read live, too useful to lose.
    target: String,
    message: String,
    #[serde(skip_serializing_if = "CoordsRecord::is_empty")]
    coords: CoordsRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorRecord>,
    /// Any other field the event carried, as `name: value`. Keeps a site's extra context
    /// (`format`, `path`, `pipe`, …) instead of dropping it on the floor.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    fields: BTreeMap<String, String>,
}

/// Collects everything one event carries: the message, a real `&dyn Error` if the site recorded
/// one, and every remaining field as a string. The fields that have a column of their own are
/// *not* collected here — they can also be inherited from an enclosing span, which a visitor over
/// the event alone cannot see, so `build_record` resolves them separately.
struct RecordVisitor {
    message: String,
    error: Option<ErrorRecord>,
    fields: BTreeMap<String, String>,
}

impl RecordVisitor {
    fn new() -> Self {
        Self { message: String::new(), error: None, fields: BTreeMap::new() }
    }

    fn put(&mut self, name: &str, value: String) {
        match name {
            "message" => self.message = value,
            name if TAGGED_FIELDS.contains(&name) || COLUMN_ONLY_FIELDS.contains(&name) => {}
            other => {
                self.fields.insert(other.to_string(), value);
            }
        }
    }
}

impl Visit for RecordVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.put(field.name(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field.name(), value.to_string());
    }

    /// The whole point of the structured logs: a site that records its error as
    /// `&dyn std::error::Error` (rather than only interpolating it into the message) gets its
    /// `Debug` form, its `Display` form and its full `source()` chain serialized, instead of one
    /// flattened string.
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        if field.name() == "error" {
            self.error = Some(ErrorRecord::from_error(value));
        } else {
            self.put(field.name(), value.to_string());
        }
    }
}

/// The current wall clock, formatted exactly as the pre-L5 text log printed it
/// (`2026-08-30T08:12:25.626426Z`). Goes through `tracing_subscriber`'s own `SystemTime` formatter
/// rather than a new date dependency: same bytes as before, nothing added to `Cargo.toml`.
fn now_timestamp() -> String {
    let mut buffer = String::new();
    let _ = SystemTime.format_time(&mut Writer::new(&mut buffer));
    buffer
}

/// Turns one event plus its span context into a [`LogRecord`].
///
/// Coordinates are resolved exactly as `.log.csv` resolves them, through the same `CapturedFields`
/// the spans already carry: outermost span first, an inner span beating an outer one, the event's
/// own fields beating every span. Without the inherited half, a `warn!` deep inside a pipe would
/// lose the page it never mentions itself.
fn build_record<S>(event: &Event<'_>, ctx: &Context<'_, S>) -> LogRecord
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    let mut visitor = RecordVisitor::new();
    event.record(&mut visitor);

    let mut merged = CapturedFields::default();
    if let Some(scope) = ctx.event_scope(event) {
        for span in scope.from_root() {
            if let Some(span_fields) = span.extensions().get::<CapturedFields>() {
                merged.merge_from(span_fields);
            }
        }
    }
    let mut own = FieldVisitor::new();
    event.record(&mut own);
    merged.merge_from(&own.0);

    let coord = |name: &str| {
        let value = merged.get(name);
        (!value.is_empty()).then(|| value.to_string())
    };
    let coords = CoordsRecord {
        report: coord("report"),
        page: coord("page"),
        first_ref: coord("coord_ref_1"),
        second_ref: coord("coord_ref_2"),
        first: coord("coord_1"),
        second: coord("coord_2"),
    };

    LogRecord {
        time: now_timestamp(),
        level: event.metadata().level().to_string(),
        activity: activity_path(ctx, event),
        target: event.metadata().target().to_string(),
        message: visitor.message,
        coords,
        error: visitor.error,
        fields: visitor.fields,
    }
}

/// `freeports.log.jsonl` — the diagnostic log, **structured** since L5 (the user's request: "in
/// freeports.log fosse strutturato"). One JSON object per line, at the same level as stderr, with
/// the `target` stderr no longer prints and the serialized error stderr never carried.
///
/// JSON Lines rather than a single JSON array or a YAML document, for two reasons that both come
/// from the volume this file sees at `-vvv` (tens of thousands of records for one job):
///
/// 1. it **streams**. Each record is serialized and handed to the buffered writer as it happens,
///    so nothing accumulates in memory. An array or a YAML document has to be closed at the end,
///    and is therefore unreadable if the process dies — which is exactly the run whose log you
///    most want to read;
/// 2. every line stands alone, so `grep` works on it and `jq` reads it as a stream without
///    holding the file in memory.
///
#[derive(Debug, Clone)]
pub struct JsonLogLayer {
    writer: SharedFileWriter,
}

impl JsonLogLayer {
    fn new(writer: SharedFileWriter) -> Self {
        Self { writer }
    }
}

impl<S> Layer<S> for JsonLogLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        record_span_metadata(attrs, id, &ctx);
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        merge_span_fields(values, id, &ctx);
    }

    /// Serialization and I/O failures are swallowed on purpose: this runs inside the tracing
    /// dispatch, so the only way to report a failure would be to log it — which would re-enter
    /// this same method. A record that cannot be written is dropped; the event still reached
    /// stderr and `.log.csv` through their own layers.
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if let Ok(line) = serde_json::to_string(&build_record(event, &ctx)) {
            let _ = self.writer.write_line(&line);
        }
    }
}

pub fn init(verbosity: Verbosity, log_dir: &Path) -> Result<LogHandle, TracingSetupError> {
    use tracing_subscriber::layer::SubscriberExt;

    let (file_layer, file_writer) = file_layer(&log_dir.join(LOG_FILE_NAME), verbosity)?;
    // Deferred on purpose: `.log.csv` belongs in the output directory, which the configuration
    // only reveals later — `log_dir` is merely the fallback. See `LogHandle::set_csv_dir`.
    let csv = CsvLogLayer::deferred();

    // Binding: the CSV layer **must** carry a level filter. A layer without one leaves the
    // registry's global max level at `TRACE`, so every `trace!` in the crate is constructed and
    // dispatched even at `-q`. Do not remove `.with_filter` here.
    let subscriber = tracing_subscriber::registry()
        .with(stderr_layer(verbosity))
        .with(file_layer)
        .with(csv.clone().with_filter(EventLevelFilter::new(csv_level_filter(verbosity))));
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|source| TracingSetupError::AlreadyInitialized { source })?;
    Ok(LogHandle { csv, file: file_writer, absorbed: Arc::new(Mutex::new(Vec::new())) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::PoisonError;
    use tracing_subscriber::prelude::*;

    /// Serializes **every** test in this file that installs a `tracing` dispatcher, whether through
    /// `tracing::subscriber::with_default` or `set_global_default`.
    ///
    /// The race is process-wide, not per-callsite. A `tracing_core` callsite's `Interest` is cached
    /// the first time that callsite is ever hit in the process, as the AND of every *currently
    /// live* dispatcher's interest — on any thread, not only the one that installed the dispatcher
    /// triggering the recomputation. A dispatcher built from a static `LevelFilter`, which every
    /// builder here uses, can therefore permanently cache `Interest::never()` for a brand-new
    /// callsite belonging to a completely unrelated test on another thread, the first time that
    /// callsite fires while the restrictive dispatcher happens to be alive.
    ///
    /// This was observed, not theorised: a `row_ordering` test flaked roughly once in fifteen runs,
    /// only alongside the rest of the suite and never in isolation, because its own callsite was
    /// poisoned by an unrelated dispatcher alive on another thread at that instant.
    ///
    /// Hence one shared lock held for the whole body of every dispatcher-installing test, related
    /// or not. Serializing only tests that share a callsite would hide the first observed symptom
    /// rather than remove the race.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// Joins nine already-escaped cell values with commas and a trailing newline, in `.log.csv`
    /// column order. Used everywhere below instead of hand-typed comma counts, which are hard to
    /// read and easy to miscount.
    ///
    /// The eighth cell is `Level`, which is why the tests below spell out `"INFO"` or `"WARN"`
    /// rather than letting a default stand in: the column exists precisely so that severity is not
    /// inferred from the message, and a helper that inferred it would be testing nothing.
    fn row(cells: [&str; 9]) -> String {
        format!("{}\n", cells.join(","))
    }

    /// [`Verbosity`]: the six-level scale, and `-v`/`-q` as independent dials.
    mod verbosity {
        use super::*;
        use tracing_subscriber::filter::LevelFilter;

        mod no_flags_default {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn zero_and_zero_is_exactly_warn() {
                assert_eq!(Verbosity::from_verbose_and_quiet_counts(0, 0), Verbosity::Warn);
            }
        }

        mod quiet_only {
            use super::*;
            use test_case::test_case;
            use pretty_assertions::assert_eq;

            #[test_case(1, Verbosity::ErrorOnly; "-q once: error only")]
            #[test_case(2, Verbosity::Silent; "-qq: silent")]
            #[test_case(3, Verbosity::Silent; "-qqq: still silent, clamped")]
            #[test_case(u8::MAX, Verbosity::Silent; "u8 upper bound: still silent, clamped")]
            fn decreasing_verbosity(quiet: u8, expected: Verbosity) {
                assert_eq!(Verbosity::from_verbose_and_quiet_counts(0, quiet), expected);
            }
        }

        mod verbose_only {
            use super::*;
            use test_case::test_case;
            use pretty_assertions::assert_eq;

            #[test_case(1, Verbosity::Info; "-v once: info")]
            #[test_case(2, Verbosity::Debug; "-vv: debug")]
            #[test_case(3, Verbosity::Trace; "-vvv: trace")]
            #[test_case(4, Verbosity::Trace; "one flag above the highest defined tier: still trace, clamped")]
            #[test_case(u8::MAX, Verbosity::Trace; "u8 upper bound: still trace, clamped")]
            fn increasing_verbosity(verbose: u8, expected: Verbosity) {
                assert_eq!(Verbosity::from_verbose_and_quiet_counts(verbose, 0), expected);
            }
        }

        mod combined_dials {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn net_positive_offset_moves_up_from_warn() {
                // verbose=2, quiet=1 -> net offset +1 from Warn (index 2) -> Info (index 3).
                assert_eq!(Verbosity::from_verbose_and_quiet_counts(2, 1), Verbosity::Info);
            }

            #[test]
            fn net_negative_offset_clamps_at_silent_instead_of_going_out_of_bounds() {
                // verbose=1, quiet=3 -> net offset -2 from Warn (index 2) -> index 0 -> Silent,
                // not a negative index.
                assert_eq!(Verbosity::from_verbose_and_quiet_counts(1, 3), Verbosity::Silent);
            }

            #[test]
            fn equal_verbose_and_quiet_counts_cancel_out_to_the_default() {
                assert_eq!(Verbosity::from_verbose_and_quiet_counts(5, 5), Verbosity::Warn);
            }

            #[test]
            fn maximal_verbose_and_quiet_together_never_panics_and_clamps() {
                let result = std::panic::catch_unwind(|| {
                    Verbosity::from_verbose_and_quiet_counts(u8::MAX, u8::MAX)
                });
                assert!(result.is_ok(), "must never panic regardless of extreme input");
                assert_eq!(result.unwrap(), Verbosity::Warn);
            }
        }

        /// `-v` and `-q` are independent dials, not mutually exclusive flags: no combination of
        /// them is an error.
        mod independent_dials_never_error {
            use super::*;
            use test_case::test_case;

            #[test_case(1, 1; "both present, net zero")]
            #[test_case(3, 1; "both present, net positive")]
            #[test_case(1, 3; "both present, net negative")]
            fn combining_v_and_q_is_never_an_error(verbose: u8, quiet: u8) {
                let result =
                    std::panic::catch_unwind(|| Verbosity::from_verbose_and_quiet_counts(verbose, quiet));
                assert!(result.is_ok(), "combining -v and -q must never panic or error");
            }
        }

        /// No behaviour is lost on `-v` alone: the same table, with `quiet` fixed at 0.
        mod equivalence_with_old_from_flag_count_formula {
            use super::*;
            use test_case::test_case;
            use pretty_assertions::assert_eq;

            #[test_case(0, Verbosity::Warn)]
            #[test_case(1, Verbosity::Info)]
            #[test_case(2, Verbosity::Debug)]
            #[test_case(3, Verbosity::Trace)]
            #[test_case(10, Verbosity::Trace; "saturates same as the old formula")]
            fn matches_the_old_four_tier_mapping_when_quiet_is_zero(count: u8, expected: Verbosity) {
                assert_eq!(Verbosity::from_verbose_and_quiet_counts(count, 0), expected);
            }
        }

        mod level_filter_mapping {
            use super::*;
            use test_case::test_case;
            use pretty_assertions::assert_eq;

            #[test_case(Verbosity::Silent, LevelFilter::OFF)]
            #[test_case(Verbosity::ErrorOnly, LevelFilter::ERROR)]
            #[test_case(Verbosity::Warn, LevelFilter::WARN)]
            #[test_case(Verbosity::Info, LevelFilter::INFO)]
            #[test_case(Verbosity::Debug, LevelFilter::DEBUG)]
            #[test_case(Verbosity::Trace, LevelFilter::TRACE)]
            fn maps_every_variant_to_the_expected_level_filter(verbosity: Verbosity, expected: LevelFilter) {
                assert_eq!(verbosity.level_filter(), expected);
            }
        }

        mod order_and_default_index {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn order_is_increasing_verbosity() {
                assert_eq!(
                    Verbosity::ORDER,
                    [
                        Verbosity::Silent,
                        Verbosity::ErrorOnly,
                        Verbosity::Warn,
                        Verbosity::Info,
                        Verbosity::Debug,
                        Verbosity::Trace,
                    ]
                );
            }

            #[test]
            fn default_index_points_at_warn() {
                assert_eq!(Verbosity::ORDER[Verbosity::DEFAULT_INDEX], Verbosity::Warn);
            }
        }
    }

    mod stderr_layer_construction {
        use super::*;
        use test_case::test_case;

        fn emit_one_event_per_level() {
            tracing::error!("error level event");
            tracing::warn!("warn level event");
            tracing::info!("info level event");
            tracing::debug!("debug level event");
            tracing::trace!("trace level event");
        }

        #[test_case(Verbosity::Silent; "silent tier")]
        #[test_case(Verbosity::ErrorOnly; "error-only tier")]
        #[test_case(Verbosity::Warn; "warn tier")]
        #[test_case(Verbosity::Info; "info tier")]
        #[test_case(Verbosity::Debug; "debug tier")]
        #[test_case(Verbosity::Trace; "trace tier")]
        fn builds_and_runs_without_panicking(verbosity: Verbosity) {
            let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
            let subscriber = tracing_subscriber::registry().with(stderr_layer(verbosity));
            // The point of this test is the absence of a panic while the layer is exercised at
            // every level, at every verbosity tier -- stderr output itself is not asserted on
            // (see the module doc: only "does not panic" is in scope for this destination).
            // `Silent` (OFF) must also construct and run cleanly, not just the five real levels.
            tracing::subscriber::with_default(subscriber, emit_one_event_per_level);
        }
    }

    /// The **observable** behaviour of `stderr_layer`: what it actually writes.
    ///
    /// Two traps make the obvious test design wrong, both confirmed empirically rather than by
    /// inspection:
    ///
    /// 1. a filter applied with `.with_filter(…)` governs **only** the `on_event` of the layer it is
    ///    attached to. A sibling layer added with a second `.with(…)` on the same `Registry` is not
    ///    "behind" that filter: it receives every event regardless of what `stderr_layer` decides to
    ///    write. Counting events with a spy layer therefore measures nothing;
    /// 2. `tracing` caches interest per *callsite* — per line of source — process-wide, not per call.
    ///    Tests firing the same macro line from different threads with different dispatchers race on
    ///    that global cache and produce non-deterministic results.
    ///
    /// So these tests capture the layer's **real** output by injecting a test writer through the
    /// same seam `stderr_layer` itself uses, exercising the production logic rather than a
    /// reimplementation of it, and share a lock that serializes their one common callsite.
    mod stderr_layer_observable_filtering {
        use super::*;
        use std::sync::{Arc, Mutex};

        /// A test writer: a shared in-memory buffer, read after the dispatch scope closes.
        /// `MakeWriter::make_writer` clones the inner `Arc`, so every write by the layer lands in
        /// the same buffer the test observes.
        #[derive(Clone, Default, Debug)]
        pub(super) struct SharedBuffer(pub(super) Arc<Mutex<Vec<u8>>>);

        impl std::io::Write for SharedBuffer {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }

            fn flush(&mut self) -> std::io::Result<()> {
                self.0.lock().unwrap().flush()
            }
        }

        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedBuffer {
            type Writer = SharedBuffer;

            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        // Serialization: originally a `SERIAL` local to this submodule (the four tests here share
        // the exact same `emit_one_event_per_level_at` callsite, the narrowest instance of the
        // race). Promoted to the module-wide `SERIAL` at the top of `mod tests` once a second,
        // broader instance of the same race was found across *unrelated* callsites -- see that
        // static's doc comment for the full story.

        fn emit_one_event_per_level_at() {
            tracing::error!("error-marker");
            tracing::warn!("warn-marker");
            tracing::info!("info-marker");
            tracing::debug!("debug-marker");
            tracing::trace!("trace-marker");
        }

        fn captured_output(verbosity: Verbosity) -> String {
            let _guard = SERIAL.lock().unwrap();
            let buffer = SharedBuffer::default();
            let layer = fmt_layer_with_writer(buffer.clone(), verbosity.level_filter(), false);
            let subscriber = tracing_subscriber::registry().with(layer);
            tracing::subscriber::with_default(subscriber, emit_one_event_per_level_at);
            String::from_utf8(buffer.0.lock().unwrap().clone()).expect("captured output is utf8")
        }

        #[test]
        fn warn_shows_warn_and_error_but_not_info() {
            let output = captured_output(Verbosity::Warn);
            assert!(output.contains("error-marker"), "missing error-marker in:\n{output}");
            assert!(output.contains("warn-marker"), "missing warn-marker in:\n{output}");
            assert!(!output.contains("info-marker"), "unexpected info-marker in:\n{output}");
            assert!(!output.contains("debug-marker"), "unexpected debug-marker in:\n{output}");
            assert!(!output.contains("trace-marker"), "unexpected trace-marker in:\n{output}");
        }

        #[test]
        fn error_only_shows_only_error() {
            let output = captured_output(Verbosity::ErrorOnly);
            assert!(output.contains("error-marker"), "missing error-marker in:\n{output}");
            assert!(!output.contains("warn-marker"), "unexpected warn-marker in:\n{output}");
            assert!(!output.contains("info-marker"), "unexpected info-marker in:\n{output}");
            assert!(!output.contains("debug-marker"), "unexpected debug-marker in:\n{output}");
            assert!(!output.contains("trace-marker"), "unexpected trace-marker in:\n{output}");
        }

        #[test]
        fn silent_shows_nothing_at_all_not_even_error() {
            let output = captured_output(Verbosity::Silent);
            assert!(output.is_empty(), "expected no output at Silent, got:\n{output}");
        }

        #[test]
        fn trace_shows_every_level() {
            let output = captured_output(Verbosity::Trace);
            assert!(output.contains("error-marker"), "missing error-marker in:\n{output}");
            assert!(output.contains("warn-marker"), "missing warn-marker in:\n{output}");
            assert!(output.contains("info-marker"), "missing info-marker in:\n{output}");
            assert!(output.contains("debug-marker"), "missing debug-marker in:\n{output}");
            assert!(output.contains("trace-marker"), "missing trace-marker in:\n{output}");
        }
    }

    /// Where a value ends, on the one destination that renders several of them side by side.
    ///
    /// The `.log.csv` and the JSONL are unaffected by any of this: both go through `SpanLabel`,
    /// which keeps a span's first field only. These tests are about stderr.
    mod stderr_value_delimiting {
        use super::*;
        use stderr_layer_observable_filtering::SharedBuffer;

        /// Runs `body` under a real `fmt_layer_with_writer`, ANSI off, and returns what it wrote.
        fn rendered(body: impl FnOnce()) -> String {
            let _guard = SERIAL.lock().unwrap();
            let buffer = SharedBuffer::default();
            let layer = fmt_layer_with_writer(buffer.clone(), LevelFilter::TRACE, false);
            let subscriber = tracing_subscriber::registry().with(layer);
            tracing::subscriber::with_default(subscriber, body);
            String::from_utf8(buffer.0.lock().unwrap().clone()).expect("captured output is utf8")
        }

        mod span_brackets {
            use super::*;

            #[test]
            fn a_single_field_span_renders_its_value_bare() {
                let output = rendered(|| {
                    let span = tracing::info_span!("job", format = "AMUNDI-EN24");
                    let _guard = span.enter();
                    tracing::warn!("marker");
                });
                assert!(output.contains("job[AMUNDI-EN24]"), "expected a bare value in:\n{output}");
            }

            /// The case that motivated the rule: the company's text is followed by two more
            /// values, and without quotes there is no way to see where it ends.
            #[test]
            fn a_multi_field_span_quotes_every_value() {
                let output = rendered(|| {
                    let span = tracing::info_span!(
                        "investment",
                        coord_ref_1 = "EXXON MOBIL CORP - 110.00 - 16.08.24 PUT",
                        coord_1 = "row 12",
                        coord_2 = "col 6",
                    );
                    let _guard = span.enter();
                    tracing::warn!("marker");
                });
                assert!(
                    output.contains(
                        "investment[\"EXXON MOBIL CORP - 110.00 - 16.08.24 PUT\",\"row 12\",\"col 6\"]"
                    ),
                    "expected every value quoted in:\n{output}"
                );
            }

            #[test]
            fn a_quote_inside_a_value_is_doubled() {
                let output = rendered(|| {
                    let span = tracing::info_span!(
                        "investment",
                        coord_ref_1 = "SOCIETE \"GENERALE\" SA",
                        coord_1 = "row 3",
                    );
                    let _guard = span.enter();
                    tracing::warn!("marker");
                });
                assert!(
                    output.contains("investment[\"SOCIETE \"\"GENERALE\"\" SA\",\"row 3\"]"),
                    "expected the inner quotes doubled in:\n{output}"
                );
            }

            /// A `%`-recorded value reaches the visitor through `record_debug`, and must come out
            /// as the text a person can search for rather than as an escaped `Debug` rendering.
            #[test]
            fn a_display_recorded_value_keeps_its_accents() {
                let output = rendered(|| {
                    let span = tracing::info_span!("document", report = %"Société Générale");
                    let _guard = span.enter();
                    tracing::warn!("marker");
                });
                assert!(
                    output.contains("document[Société Générale]"),
                    "expected the text unescaped in:\n{output}"
                );
            }

            #[test]
            fn a_span_with_no_fields_renders_no_brackets() {
                let output = rendered(|| {
                    let span = tracing::info_span!("deserialize");
                    let _guard = span.enter();
                    tracing::warn!("marker");
                });
                assert!(output.contains("deserialize:"), "expected a bare span name in:\n{output}");
                assert!(!output.contains("deserialize["), "unexpected brackets in:\n{output}");
            }
        }

        mod event_tail {
            use super::*;
            use pretty_assertions::assert_eq;

            /// Extracts the ` key=value` tail that follows the message.
            fn tail(output: &str) -> String {
                output.split_once("marker").expect("the message is on the line").1.trim_end().to_string()
            }

            #[test]
            fn a_value_containing_spaces_is_quoted() {
                let output = rendered(|| {
                    tracing::warn!(coord_ref_1 = "Leonardo Spa Az Nom", coord_ref_2 = "Leonardo", "marker");
                });
                assert_eq!(tail(&output), " coord_ref_1=\"Leonardo Spa Az Nom\" coord_ref_2=Leonardo");
            }

            /// A value with no space ends at the space before the next key, so quoting it would be
            /// noise — and a number is never quoted.
            #[test]
            fn a_value_without_spaces_stays_bare() {
                let output = rendered(|| tracing::warn!(page = 53, "marker"));
                assert_eq!(tail(&output), " page=53");
            }

            #[test]
            fn message_and_error_never_reach_the_tail() {
                let output = rendered(|| {
                    let err = std::io::Error::other("boom");
                    tracing::warn!(error = log_error(&err), page = 7, "marker");
                });
                assert_eq!(tail(&output), " page=7");
            }
        }
    }

    mod file_destination {
        use super::*;

        mod construction {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn creates_the_file_at_the_given_path() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(LOG_FILE_NAME);
                assert!(
                    file_layer::<tracing_subscriber::Registry>(&path, Verbosity::Debug).is_ok()
                );
                assert!(path.exists());
            }

            #[test]
            fn errors_when_the_parent_directory_does_not_exist() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join("missing_subdir").join(LOG_FILE_NAME);
                let err = file_layer::<tracing_subscriber::Registry>(&path, Verbosity::Debug)
                    .expect_err("parent directory does not exist, this must fail");
                match err {
                    TracingSetupError::OpenLogFile { path: reported, source: _ } => {
                        assert_eq!(reported, path);
                    }
                    other => panic!("expected OpenLogFile, found {other:?}"),
                }
            }
        }

        mod level_filtering {
            use super::*;

            /// Emits one marker per level through a `file_layer` built at `verbosity`, flushes
            /// the buffered writer (buffering is why reading the file without flushing first
            /// would see nothing) and returns the file's content.
            fn file_content_at(verbosity: Verbosity) -> String {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(LOG_FILE_NAME);
                let (layer, writer) =
                    file_layer(&path, verbosity).expect("file layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::error!("error-marker");
                    tracing::warn!("warn-marker");
                    tracing::info!("info-marker");
                    tracing::debug!("debug-marker");
                    tracing::trace!("trace-marker");
                });
                writer.flush().expect("flush the buffered freeports.log.jsonl writer");
                std::fs::read_to_string(&path).expect("read freeports.log.jsonl")
            }

            /// L4: the file log follows `-v`/`-q` instead of the hardcoded `DEBUG` it used
            /// before. That hardcoding is why a default (`Warn`) run still formatted and wrote
            /// every `debug!` in the crate to disk. The markers are matched as substrings, so
            /// this stays true of the JSON lines L5 turned the file into.
            #[test]
            fn follows_the_given_verbosity_instead_of_a_hardcoded_debug() {
                let content = file_content_at(Verbosity::Warn);
                assert!(content.contains("error-marker"));
                assert!(content.contains("warn-marker"));
                assert!(
                    !content.contains("info-marker"),
                    "at Warn the file must not carry info events, got:\n{content}"
                );
                assert!(
                    !content.contains("debug-marker"),
                    "at Warn the file must not carry debug events, got:\n{content}"
                );
            }

            #[test]
            fn captures_every_level_at_trace() {
                let content = file_content_at(Verbosity::Trace);
                assert!(content.contains("error-marker"));
                assert!(content.contains("warn-marker"));
                assert!(content.contains("info-marker"));
                assert!(content.contains("debug-marker"));
                assert!(content.contains("trace-marker"));
            }

            #[test]
            fn captures_debug_and_above_but_not_trace_at_debug() {
                let content = file_content_at(Verbosity::Debug);
                assert!(content.contains("error-marker"));
                assert!(content.contains("warn-marker"));
                assert!(content.contains("info-marker"));
                assert!(content.contains("debug-marker"));
                assert!(
                    !content.contains("trace-marker"),
                    "at Debug the file must not carry trace events, got:\n{content}"
                );
            }
        }
    }

    /// `freeports.log.jsonl`: one JSON line per event, carrying the `target` that stderr no longer
    /// prints and the serialized error that stderr never carried.
    mod json_layer {
        use super::*;

        /// A two-level error, so there is a real `source()` chain to serialize.
        #[derive(Debug, thiserror::Error)]
        #[error("the inner thing broke")]
        struct InnerError;

        #[derive(Debug, thiserror::Error)]
        #[error("the outer thing broke")]
        struct OuterError {
            #[source]
            source: InnerError,
        }

        /// Runs `body` with a `file_layer` at `Trace`, then returns the file's lines already
        /// deserialized. Every line must be valid JSON on its own — that is the whole point of the
        /// line-oriented format — so parsing is part of the assertion, not a detail of the fixture.
        fn records(body: impl FnOnce()) -> Vec<serde_json::Value> {
            let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join(LOG_FILE_NAME);
            let (layer, writer) =
                file_layer(&path, Verbosity::Trace).expect("file layer construction");
            let subscriber = tracing_subscriber::registry().with(layer);
            tracing::subscriber::with_default(subscriber, body);
            writer.flush().expect("flush the buffered writer");
            std::fs::read_to_string(&path)
                .expect("read the jsonl log")
                .lines()
                .map(|line| {
                    serde_json::from_str(line).unwrap_or_else(|e| {
                        panic!("every line must be valid JSON on its own, {line:?} is not: {e}")
                    })
                })
                .collect()
        }

        mod record_shape {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn one_object_per_event_with_time_level_activity_target_and_message() {
                let records = records(|| {
                    let span = tracing::info_span!("page", page = 12u64);
                    span.in_scope(|| tracing::warn!("something is off"));
                });
                let record = records.last().expect("the warning must be there");
                assert_eq!(record["level"], "WARN");
                assert_eq!(record["activity"], "page[12]");
                assert_eq!(record["message"], "something is off");
                assert!(
                    record["target"]
                        .as_str()
                        .is_some_and(|t| t.starts_with("freeports::core::tracing_setup")),
                    "got: {record}"
                );
                assert!(
                    record["time"].as_str().is_some_and(|t| t.ends_with('Z')),
                    "the wall clock moved into a field, it did not disappear: {record}"
                );
            }

            /// The module path is precisely what stderr no longer prints: were it to disappear from
            /// here too, the information would be lost altogether.
            #[test]
            fn the_module_path_removed_from_stderr_is_kept_here() {
                let records = records(|| tracing::error!("boom"));
                assert!(
                    records.iter().all(|r| r["target"].is_string()),
                    "every record carries its module path: {records:?}"
                );
            }

            #[test]
            fn other_event_fields_are_kept_under_fields() {
                let records = records(|| {
                    tracing::warn!(format = "EURIZON-EN23", "format is unhappy");
                });
                let record = records.last().expect("the warning must be there");
                assert_eq!(record["fields"]["format"], "EURIZON-EN23");
            }

            /// Opening and closing a span writes no line: the file records events, not spans.
            #[test]
            fn opening_a_span_alone_writes_nothing() {
                let records = records(|| {
                    let span = tracing::info_span!("page", page = 1u64);
                    let _guard = span.enter();
                });
                assert!(records.is_empty(), "got: {records:?}");
            }
        }

        mod serialized_error {
            use super::*;
            use pretty_assertions::assert_eq;

            /// When the event is tied to an `Err`, the file carries the whole error: `Display`,
            /// `Debug` and the `source()` chain, not one flattened string.
            #[test]
            fn an_event_tied_to_an_error_carries_display_debug_and_the_source_chain() {
                let records = records(|| {
                    let e = OuterError { source: InnerError };
                    tracing::error!(error = log_error(&e), "it failed: {e}");
                });
                let error = &records.last().expect("the error must be there")["error"];
                assert_eq!(error["display"], "the outer thing broke");
                assert_eq!(error["debug"], "OuterError { source: InnerError }");
                assert_eq!(error["source"][0], "the inner thing broke");
            }

            #[test]
            fn an_error_without_a_cause_omits_the_source_key() {
                let records = records(|| {
                    let e = InnerError;
                    tracing::error!(error = log_error(&e), "it failed: {e}");
                });
                let error = &records.last().expect("the error must be there")["error"];
                assert!(error["source"].is_null(), "got: {error}");
            }

            /// An event with nothing to do with an error must not invent an `error` key.
            #[test]
            fn an_event_with_no_error_has_no_error_key() {
                let records = records(|| tracing::info!("all good"));
                let record = records.last().expect("the event must be there");
                assert!(record["error"].is_null(), "got: {record}");
            }
        }

        mod inherited_coordinates {
            use super::*;
            use pretty_assertions::assert_eq;

            /// The same coordinates as `.log.csv`, resolved by the same shared rule: the innermost
            /// span beats the outer one, the event beats every span.
            #[test]
            fn coordinates_come_from_the_enclosing_spans_too() {
                let records = records(|| {
                    let outer = tracing::info_span!("page", page = 44u64);
                    outer.in_scope(|| {
                        let inner = tracing::info_span!("field", coord_ref_2 = "market value");
                        inner.in_scope(|| tracing::warn!(coord_1 = "row 12", "cast failed"));
                    });
                });
                let coords = &records.last().expect("the warning must be there")["coords"];
                assert_eq!(coords["page"], "44");
                assert_eq!(coords["second_ref"], "market value");
                assert_eq!(coords["first"], "row 12");
            }

            #[test]
            fn an_event_with_no_coordinates_omits_the_coords_key() {
                let records = records(|| tracing::warn!("nowhere in particular"));
                let record = records.last().expect("the warning must be there");
                assert!(record["coords"].is_null(), "got: {record}");
            }
        }

        /// Why the file is line-oriented and not one JSON or YAML document: records reach the disk
        /// while the run is still going, so the log of a process that died is still readable.
        mod streaming {
            use super::*;

            #[test]
            fn records_reach_disk_before_anyone_flushes_or_closes() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(LOG_FILE_NAME);
                let (layer, _writer) =
                    file_layer(&path, Verbosity::Trace).expect("file layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                // Enough events to overflow the `BufWriter` (8 KiB): past that threshold the
                // content is on disk without anyone having closed anything — exactly what one wants
                // to read after a crash.
                tracing::subscriber::with_default(subscriber, || {
                    for i in 0..500u64 {
                        tracing::info!(page = i, "still running");
                    }
                });
                let content = std::fs::read_to_string(&path).expect("read the jsonl log");
                let first = content.lines().next().expect("at least one complete line on disk");
                let parsed: serde_json::Value =
                    serde_json::from_str(first).expect("the first line is already valid JSON");
                assert_eq!(parsed["message"], "still running");
            }
        }
    }

    mod csv_layer {
        use super::*;

        mod header {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn create_writes_the_header_row_immediately() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                CsvLogLayer::create(&path).expect("csv layer construction");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(content, CSV_HEADER);
            }

            #[test]
            fn header_matches_the_documented_column_order() {
                assert_eq!(
                    CSV_HEADER,
                    "Report,Page,Activity,First coord ref,Second coord ref,First coord,Second coord,Level,Message\n"
                );
            }
        }

        mod construction_errors {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn errors_when_the_parent_directory_does_not_exist() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join("missing_subdir").join(".log.csv");
                let err = CsvLogLayer::create(&path).expect_err("parent directory is missing");
                match err {
                    TracingSetupError::OpenCsvFile { path: reported, source: _ } => {
                        assert_eq!(reported, path);
                    }
                    other => panic!("expected OpenCsvFile, found {other:?}"),
                }
            }
        }

        mod selectivity {
            use super::*;
            use pretty_assertions::assert_eq;
            use test_case::test_case;

            #[test]
            fn event_with_no_tagged_field_and_no_enclosing_span_produces_no_row() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!("an entirely untagged message");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(content, CSV_HEADER, "an untagged event must not add a data row");
            }

            #[test]
            fn event_with_its_own_tagged_field_produces_a_row() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 3u64, "page-scoped message");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                // No active span: the `Activity` column (index 1) is empty.
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["", "3", "", "", "", "", "", "INFO", "page-scoped message"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn event_with_no_own_tags_inside_a_tagged_span_still_produces_a_row() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("page_processing", page = 7u64);
                    span.in_scope(|| {
                        tracing::warn!("no tags on the event itself, page comes from the span");
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                // `Activity` (index 1) now carries the name of the one active span.
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row([
                        "",
                        "7",
                        "page_processing[7]",
                        "",
                        "",
                        "",
                        "",
                        "WARN",
                        "\"no tags on the event itself, page comes from the span\""
                    ])
                );
                assert_eq!(content, expected);
            }

            /// Exhaustive over `TAGGED_FIELDS`: each of the five tagged fields, on its own, selects
            /// a row.
            ///
            /// Parameterised over an emitting function rather than over a `(name, value)` pair,
            /// because the `tracing` macros need a literal field identifier at compile time: the
            /// field name cannot be a runtime variable. Same exhaustiveness, adapted to the
            /// language's constraint.
            #[test_case(|| { tracing::info!(page = 1u64, "solo"); }; "page alone")]
            #[test_case(|| { tracing::info!(coord_ref_1 = "x", "solo"); }; "coord_ref_1 alone")]
            #[test_case(|| { tracing::info!(coord_ref_2 = "x", "solo"); }; "coord_ref_2 alone")]
            #[test_case(|| { tracing::info!(coord_1 = "row 1", "solo"); }; "coord_1 alone")]
            #[test_case(|| { tracing::info!(coord_2 = "col 1", "solo"); }; "coord_2 alone")]
            fn each_tagged_field_alone_selects_a_row(emit: fn()) {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, emit);
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(
                    content.lines().count(),
                    2,
                    "expected exactly one data row (header + 1), got:\n{content}"
                );
            }
        }

        mod field_capture {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn captures_all_tagged_fields_from_a_single_event() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::warn!(
                        page = 12u64,
                        coord_ref_1 = "Acme Corp",
                        coord_ref_2 = "NAV",
                        coord_1 = "row 3",
                        coord_2 = "col 2",
                        "value out of expected range"
                    );
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                // No active span, so `Activity` is empty. `coord_1` and `coord_2` carry their unit
                // inside the value itself (`"row 3"`, `"col 2"`); no real producer exercises this
                // yet, but the layer must still pass the value through verbatim.
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row([
                        "",
                        "12",
                        "",
                        "Acme Corp",
                        "NAV",
                        "row 3",
                        "col 2",
                        "WARN",
                        "value out of expected range"
                    ])
                );
                assert_eq!(content, expected);
            }

            /// The two ref columns come from their own fields, `coord_ref_1` and `coord_ref_2`.
            #[test]
            fn the_two_ref_columns_come_from_their_own_fields() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(
                        page = 1u64,
                        coord_ref_1 = "Acme Corp",
                        coord_ref_2 = "NAV",
                        "tagged message"
                    );
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let header_and_row: Vec<&str> = content.lines().collect();
                assert_eq!(header_and_row.len(), 2, "expected exactly one data row");
                let cells: Vec<&str> = header_and_row[1].split(',').collect();
                assert_eq!(cells[3], "Acme Corp", "\"First coord ref\" comes from `coord_ref_1`");
                assert_eq!(cells[4], "NAV", "\"Second coord ref\" comes from `coord_ref_2`");
            }


            #[test]
            fn event_field_overrides_a_same_named_span_field() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("page_processing", page = 7u64);
                    span.in_scope(|| {
                        tracing::warn!(page = 9u64, "explicit page wins over the span's");
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row([
                        "",
                        "9",
                        "page_processing[7]",
                        "",
                        "",
                        "",
                        "",
                        "WARN",
                        "explicit page wins over the span's"
                    ])
                );
                assert_eq!(content, expected);
            }

            /// Exhaustiveness: event-over-span overriding is pinned not only on `page` (the test
            /// above) but also on a `coord_*` field.
            #[test]
            fn event_field_overrides_a_same_named_span_coord_field() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("field", coord_ref_2 = "SPAN_VALUE");
                    span.in_scope(|| {
                        tracing::warn!(coord_ref_2 = "EVENT_VALUE", "event field wins over the span's");
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["", "", "field[SPAN_VALUE]", "", "EVENT_VALUE", "", "", "WARN", "event field wins over the span's"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn innermost_span_wins_over_an_outer_span_for_the_same_field() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let outer = tracing::info_span!("document_ingest", page = 1u64);
                    outer.in_scope(|| {
                        let inner = tracing::info_span!("page_classification", page = 2u64);
                        inner.in_scope(|| {
                            tracing::info!("nested inside two page-tagged spans");
                        });
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                // `Activity` is the outermost-to-innermost span path, regardless of which span
                // actually contributed the winning field value.
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row([
                        "",
                        "2",
                        "document_ingest[1]/page_classification[2]",
                        "",
                        "",
                        "",
                        "",
                        "INFO",
                        "nested inside two page-tagged spans"
                    ])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn distinct_span_fields_at_different_nesting_levels_all_merge_into_one_row() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let outer = tracing::info_span!("document_ingest", page = 4u64);
                    outer.in_scope(|| {
                        let inner = tracing::info_span!("field_extraction", coord_ref_2 = "ISIN");
                        inner.in_scope(|| {
                            tracing::warn!(coord_1 = 6u64, "merged from event and two spans");
                        });
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row([
                        "",
                        "4",
                        "document_ingest[4]/field_extraction[ISIN]",
                        "",
                        "ISIN",
                        "6",
                        "",
                        "WARN",
                        "merged from event and two spans"
                    ])
                );
                assert_eq!(content, expected);
            }
        }

        mod csv_escaping {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn message_containing_a_comma_is_quoted() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "value, with a comma inside");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER},1,,,,,,INFO,\"value, with a comma inside\"\n"
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn message_containing_a_double_quote_is_escaped_by_doubling() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "say \"hi\" to the user");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER},1,,,,,,INFO,\"say \"\"hi\"\" to the user\"\n"
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn message_containing_a_newline_is_quoted_and_the_newline_is_preserved_verbatim() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "first line\nsecond line");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!("{CSV_HEADER},1,,,,,,INFO,\"first line\nsecond line\"\n");
                assert_eq!(content, expected);
            }

            #[test]
            fn a_tagged_field_value_containing_a_comma_is_quoted() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, coord_ref_1 = "Acme, Inc.", "ok");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!("{CSV_HEADER},1,,\"Acme, Inc.\",,,,INFO,ok\n");
                assert_eq!(content, expected);
            }
        }

        /// `Report`: a column that is filled wherever a row exists, and is never a reason for one.
        /// The `Level` column: the one value that comes from the event's metadata rather than
        /// from a field, and therefore the one that must never be inherited or select a row.
        mod level_column {
            use super::*;
            use pretty_assertions::assert_eq;

            /// Runs `body` under a `CsvLogLayer` with no level ceiling and returns the file.
            fn written(body: impl FnOnce()) -> String {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, body);
                layer.close().expect("close must succeed");
                std::fs::read_to_string(&path).expect("read .log.csv")
            }

            #[test]
            fn a_warning_writes_warn() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let content = written(|| tracing::warn!(page = 3u64, "dropped a field"));
                assert_eq!(
                    content,
                    format!("{CSV_HEADER}{}", row(["", "3", "", "", "", "", "", "WARN", "dropped a field"]))
                );
            }

            #[test]
            fn an_error_writes_error() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let content = written(|| tracing::error!(page = 3u64, "lost the holding"));
                assert_eq!(
                    content,
                    format!("{CSV_HEADER}{}", row(["", "3", "", "", "", "", "", "ERROR", "lost the holding"]))
                );
            }

            /// The level is metadata, not a field: it does not merge down the span stack the way
            /// `page` and the coordinates do. A `warn` under an `info_span` is a `warn`.
            #[test]
            fn an_event_keeps_its_own_level_inside_a_span_of_another_level() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let content = written(|| {
                    let span = tracing::info_span!("investment", coord_ref_1 = "ALROSA CJSC");
                    span.in_scope(|| tracing::warn!(page = 12u64, "the report writes a dash here"));
                });
                assert_eq!(
                    content,
                    format!(
                        "{CSV_HEADER}{}",
                        row([
                            "",
                            "12",
                            "investment[ALROSA CJSC]",
                            "ALROSA CJSC",
                            "",
                            "",
                            "",
                            "WARN",
                            "the report writes a dash here"
                        ])
                    )
                );
            }

            /// Every event has a level, so a level alone must not write a row — otherwise the
            /// page-or-coordinate rule that keeps this file an audit trail would be void.
            #[test]
            fn a_level_alone_selects_no_row() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let content = written(|| tracing::error!("no page, no coordinate, no row"));
                assert_eq!(content, CSV_HEADER);
            }
        }

        mod report_column {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn a_report_alone_selects_no_row() {
                // The rule that keeps the file an audit trail of the extraction: a document's name
                // is not a position. A `document` span wraps a whole run, so if `report` selected
                // rows, every warning of the program would become one.
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("document", report = "EURIZON 2023");
                    span.in_scope(|| tracing::warn!("something happened, but nowhere in particular"));
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(content, CSV_HEADER);
            }

            #[test]
            fn a_report_from_an_enclosing_span_fills_the_column_of_a_row_a_page_selected() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let outer = tracing::info_span!("document", report = "EURIZON 2023");
                    outer.in_scope(|| {
                        tracing::warn!(page = 16u64, "a row of its own");
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["EURIZON 2023", "16", "document[EURIZON 2023]", "", "", "", "", "WARN", "a row of its own"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn the_innermost_document_wins_as_every_other_field_does() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let outer = tracing::info_span!("document", report = "OUTER");
                    outer.in_scope(|| {
                        let inner = tracing::info_span!("document", report = "INNER");
                        inner.in_scope(|| tracing::warn!(page = 1u64, "nested"));
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let cells: Vec<&str> =
                    content.lines().nth(1).expect("one data row").split(',').collect();
                assert_eq!(cells[0], "INNER");
            }
        }

        mod multiple_rows {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn two_tagged_events_produce_two_rows_in_emission_order() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "first");
                    tracing::info!(page = 2u64, "second");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                // Pages already arrive in increasing order: also a first, minimal proof that
                // `close`'s page ordering (`row_ordering`, below) does not disturb an already
                // sorted sequence.
                let expected = format!(
                    "{CSV_HEADER}{}{}",
                    row(["", "1", "", "", "", "", "", "INFO", "first"]),
                    row(["", "2", "", "", "", "", "", "INFO", "second"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn an_untagged_event_between_two_tagged_ones_does_not_add_a_row() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "first");
                    tracing::info!("skipped, no tags and no enclosing span");
                    tracing::info!(page = 2u64, "second");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}{}",
                    row(["", "1", "", "", "", "", "", "INFO", "first"]),
                    row(["", "2", "", "", "", "", "", "INFO", "second"])
                );
                assert_eq!(content, expected);
            }
        }

        /// The deferred `.log.csv` destination — what lets the file end up beside the outputs
        /// rather than in the working directory, even though the layer is installed long before the
        /// configuration says where the outputs are.
        mod deferred_destination {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn rows_logged_before_the_destination_is_set_still_reach_the_file() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let layer = CsvLogLayer::deferred();
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::warn!(page = 3u64, "logged before anyone knew where to put it");
                });

                let path = dir.path().join(CSV_FILE_NAME);
                layer.set_destination(&path).expect("set_destination must succeed");
                layer.close().expect("close must succeed");

                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["", "3", "", "", "", "", "", "WARN", "logged before anyone knew where to put it"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn set_destination_writes_the_header_immediately() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(CSV_FILE_NAME);
                let layer = CsvLogLayer::deferred();
                assert!(!path.exists());
                layer.set_destination(&path).expect("set_destination must succeed");
                assert_eq!(
                    std::fs::read_to_string(&path).expect("read .log.csv"),
                    CSV_HEADER
                );
            }

            /// Closing without ever having settled a destination must not lose the rows silently: it
            /// is an explicit error. `LogHandle::close` never gets there, since it calls
            /// `discard()` rather than falling back to the working directory, but the layer on its
            /// own has to say so instead of swallowing.
            #[test]
            fn closing_without_a_destination_is_an_error_not_a_silent_loss() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let layer = CsvLogLayer::deferred();
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::warn!(page = 1u64, "nowhere to go");
                });
                match layer.close() {
                    Err(TracingSetupError::CsvDestinationUnset) => {}
                    other => panic!("expected CsvDestinationUnset, found {other:?}"),
                }
            }

            /// No rows and no destination: there is nothing to write, so there is nothing to fail
            /// over.
            #[test]
            fn closing_an_empty_layer_without_a_destination_is_a_no_op() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                CsvLogLayer::deferred().close().expect("nothing accumulated, nothing to write");
            }
        }

        /// `EventLevelFilter` — the filter that makes the cost of logging bearable without losing
        /// the context of the events that remain. Each test here defends one half of the contract:
        /// spans always pass, events do not.
        mod event_level_filter {
            use super::*;
            use pretty_assertions::assert_eq;
            use tracing_subscriber::Layer;

            /// The regression this type exists for: with a plain `LevelFilter::WARN` in its place an
            /// `info_span!` is never opened, the `warn!` inside it loses the inherited `page`
            /// field, and the row is not even selected. On a real job that was 391 rows becoming
            /// zero.
            #[test]
            fn a_warning_inside_an_info_span_keeps_the_span_page_at_warn_level() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry()
                    .with(layer.clone().with_filter(EventLevelFilter::new(LevelFilter::WARN)));
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("page", page = 42u64);
                    span.in_scope(|| {
                        tracing::warn!("something went wrong on this page");
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row([
                        "",
                        "42",
                        "page[42]",
                        "",
                        "",
                        "",
                        "",
                        "WARN",
                        "something went wrong on this page"
                    ])
                );
                assert_eq!(content, expected);
            }

            /// The other half: events below the requested level do not arrive, even when they are
            /// inside a span the filter lets through.
            #[test]
            fn debug_and_trace_events_never_reach_the_layer_at_warn_level() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry()
                    .with(layer.clone().with_filter(EventLevelFilter::new(LevelFilter::WARN)));
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("page", page = 42u64);
                    span.in_scope(|| {
                        tracing::info!("info-marker");
                        tracing::debug!("debug-marker");
                        tracing::trace!("trace-marker");
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(
                    content, CSV_HEADER,
                    "only the header must be present, no event was at warn or above"
                );
            }

            /// `max_level_hint` is what switches `debug!` and `trace!` off at the callsite, which is
            /// where most of the saving comes from: it must stay at `INFO` while the spans are
            /// `info_span!`, and never drop to the event level.
            #[test]
            fn max_level_hint_never_drops_below_the_span_level() {
                use tracing_subscriber::layer::Filter;
                for level in [LevelFilter::ERROR, LevelFilter::WARN, LevelFilter::INFO] {
                    let filter = EventLevelFilter::new(level);
                    assert_eq!(
                        Filter::<tracing_subscriber::Registry>::max_level_hint(&filter),
                        Some(SPAN_LEVEL),
                        "at {level} the hint must still admit the crate's info_span!s"
                    );
                }
            }

            /// Above the span level the hint follows the requested verbosity, or `-vv` and `-vvv`
            /// would show nothing more.
            #[test]
            fn max_level_hint_follows_the_event_level_above_the_span_level() {
                use tracing_subscriber::layer::Filter;
                for level in [LevelFilter::DEBUG, LevelFilter::TRACE] {
                    let filter = EventLevelFilter::new(level);
                    assert_eq!(
                        Filter::<tracing_subscriber::Registry>::max_level_hint(&filter),
                        Some(level)
                    );
                }
            }

            /// `-qq` must be complete silence: not even span bookkeeping.
            #[test]
            fn off_stays_off_spans_included() {
                use tracing_subscriber::layer::Filter;
                let filter = EventLevelFilter::new(LevelFilter::OFF);
                assert_eq!(
                    Filter::<tracing_subscriber::Registry>::max_level_hint(&filter),
                    Some(LevelFilter::OFF)
                );
            }
        }

        /// The `Activity` column: the `/`-separated path of the active span names, outermost to
        /// innermost. `Activity` is never a tagged field, so it enriches a row but never selects
        /// one on its own; the last two tests of this module pin exactly that.
        mod activity_column {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn no_active_span_yields_an_empty_activity() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "top-level no span");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                // No comma in the message: `row()` joins already-escaped cells verbatim, and a
                // comma here would need quoting in the real CSV output (see
                // `csv_escaping::message_containing_a_comma_is_quoted`) -- irrelevant to what this
                // test targets (an empty `Activity`), so it is avoided rather than escaped by hand.
                let expected =
                    format!("{CSV_HEADER}{}", row(["", "1", "", "", "", "", "", "INFO", "top-level no span"]));
                assert_eq!(content, expected);
            }

            #[test]
            fn a_single_active_span_yields_its_name() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("page_processing", page = 7u64);
                    span.in_scope(|| {
                        tracing::info!("no fields of its own");
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["", "7", "page_processing[7]", "", "", "", "", "INFO", "no fields of its own"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn nested_spans_yield_a_slash_joined_path_outermost_first() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let outer = tracing::info_span!("run");
                    outer.in_scope(|| {
                        let middle = tracing::info_span!("job");
                        middle.in_scope(|| {
                            let inner = tracing::info_span!("document");
                            inner.in_scope(|| {
                                tracing::warn!(page = 3u64, "three levels deep");
                            });
                        });
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["", "3", "run/job/document", "", "", "", "", "WARN", "three levels deep"])
                );
                assert_eq!(content, expected);
            }

            /// An active span with no tagged fields at all produces no row, even though `Activity`
            /// would have a non-empty value: `Activity` alone is never a reason to write a row.
            #[test]
            fn an_active_but_entirely_untagged_span_still_appears_in_activity_without_selecting_a_row()
             {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("untagged_span");
                    span.in_scope(|| {
                        tracing::info!("nothing tagged anywhere");
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(
                    content, CSV_HEADER,
                    "Activity alone (from an active but entirely untagged span) must never select a row"
                );
            }

            /// An outer span with no tagged fields does not stop an inner field from selecting the
            /// row, and its name still appears in `Activity` next to that of the span that
            /// actually carried the field.
            #[test]
            fn an_untagged_span_appears_in_activity_alongside_a_row_selected_by_a_deeper_tagged_field()
             {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    let outer = tracing::info_span!("untagged_outer");
                    outer.in_scope(|| {
                        let inner = tracing::info_span!("tagged_inner", page = 5u64);
                        inner.in_scope(|| {
                            tracing::info!("no fields of its own -- page comes from the inner span");
                        });
                    });
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row([
                        "",
                        "5",
                        "untagged_outer/tagged_inner[5]",
                        "",
                        "",
                        "",
                        "",
                        "INFO",
                        "no fields of its own -- page comes from the inner span"
                    ])
                );
                assert_eq!(content, expected);
            }
        }

        /// Determinism: rows are not written as they arrive but accumulated and sorted by
        /// `RowOrderKey` — `(page, arrival sequence)` — at `close()`.
        ///
        /// This module is the only proof that the ordering mechanism works: no integration test
        /// exercises it, since the pytest comparison of the formats repository sorts both sides
        /// already, and the few reference files that have data rows are in increasing page order to
        /// begin with.
        mod row_ordering {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn no_rows_are_written_before_close_is_called() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 5u64, "buffered, not yet on disk");
                    tracing::info!(page = 2u64, "also buffered");
                });
                // Deliberately read the file *before* calling `close()`.
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(
                    content, CSV_HEADER,
                    "data rows must stay buffered until close() is called, got:\n{content}"
                );
            }

            #[test]
            fn close_writes_all_accumulated_rows_sorted_by_page() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 5u64, "at page five");
                    tracing::info!(page = 2u64, "at page two");
                    tracing::info!(page = 8u64, "at page eight");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}{}{}",
                    row(["", "2", "", "", "", "", "", "INFO", "at page two"]),
                    row(["", "5", "", "", "", "", "", "INFO", "at page five"]),
                    row(["", "8", "", "", "", "", "", "INFO", "at page eight"])
                );
                assert_eq!(content, expected, "rows must come out sorted by page, not by arrival order");
            }

            #[test]
            fn same_page_rows_preserve_arrival_order() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 4u64, "first at page four");
                    tracing::info!(page = 4u64, "second at page four");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}{}",
                    row(["", "4", "", "", "", "", "", "INFO", "first at page four"]),
                    row(["", "4", "", "", "", "", "", "INFO", "second at page four"])
                );
                assert_eq!(
                    content, expected,
                    "the arrival sequence must break ties between rows sharing the same page"
                );
            }

            /// No real fixture exercises this today; it is a decision of principle, pinned so it
            /// does not drift.
            #[test]
            fn rows_without_a_page_sort_after_rows_with_a_page() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    // Emitted "no page, then with page" -- close() must invert this order.
                    tracing::info!(coord_ref_1 = "no page here", "unnumbered");
                    tracing::info!(page = 3u64, "numbered");
                });
                layer.close().expect("close must succeed");
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}{}",
                    row(["", "3", "", "", "", "", "", "INFO", "numbered"]),
                    row(["", "", "", "no page here", "", "", "", "INFO", "unnumbered"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn close_is_idempotent_a_second_call_writes_nothing_new() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "only row");
                });
                layer.close().expect("first close must succeed");
                let after_first = std::fs::read_to_string(&path).expect("read .log.csv");
                layer.close().expect("second close must also succeed, and be a no-op");
                let after_second = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(
                    after_first, after_second,
                    "a second close() must not duplicate or otherwise change the file's content"
                );
            }

            /// The `Drop` safety net: with `close()` never called, the buffer still reaches disk
            /// when the last remaining `Arc` goes out of scope.
            #[test]
            fn dropping_the_last_clone_without_calling_close_still_flushes_accumulated_rows() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                {
                    // `layer` is moved into the registry stack here, with no `.clone()` kept
                    // anywhere else: when this scope ends, the subscriber (and with it the last
                    // `Arc<CsvLogLayerInner>`) is dropped.
                    let subscriber = tracing_subscriber::registry().with(layer);
                    tracing::subscriber::with_default(subscriber, || {
                        tracing::info!(page = 9u64, "flushed only via Drop -- close() never called");
                    });
                }
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["", "9", "", "", "", "", "", "INFO", "flushed only via Drop -- close() never called"])
                );
                assert_eq!(content, expected);
            }

            /// `Drop` belongs on `CsvLogLayerInner`, not on `CsvLogLayer`: it must fire only when
            /// the last `Arc` disappears, not on every dropped clone.
            #[test]
            fn dropping_one_clone_while_another_is_still_held_does_not_flush_yet() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let kept_clone = layer.clone();
                {
                    let subscriber = tracing_subscriber::registry().with(layer);
                    tracing::subscriber::with_default(subscriber, || {
                        tracing::info!(page = 1u64, "not yet flushed");
                    });
                    // The subscriber (holding one clone of the Arc) is dropped at the end of this
                    // scope, but `kept_clone` keeps the inner shared state alive.
                }
                let before = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(
                    before, CSV_HEADER,
                    "must not flush while another clone of the layer is still alive, got:\n{before}"
                );

                drop(kept_clone);

                let after = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected =
                    format!("{CSV_HEADER}{}", row(["", "1", "", "", "", "", "", "INFO", "not yet flushed"]));
                assert_eq!(after, expected, "dropping the last remaining clone must flush");
            }

            /// A stress test: many events in a fixed non-increasing page order — fixed rather than
            /// random, so it is reproducible — all sorted after `close()`.
            #[test]
            fn many_events_in_scrambled_page_order_are_all_sorted_after_close() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());

                const COUNT: u64 = 151;
                // A fixed multiplicative permutation of 0..COUNT (COUNT is prime), scrambled but
                // reproducible: emits every page number in 0..COUNT exactly once, in an order
                // that is very far from sorted.
                let pages: Vec<u64> = (0..COUNT).map(|i| (i * 37) % COUNT).collect();
                assert_eq!(
                    { let mut sorted = pages.clone(); sorted.sort_unstable(); sorted.dedup(); sorted.len() },
                    COUNT as usize,
                    "the fixture must be a permutation of 0..COUNT, sanity-checking the test itself"
                );

                tracing::subscriber::with_default(subscriber, || {
                    for page in &pages {
                        tracing::info!(page = *page, "row {}", page);
                    }
                });
                layer.close().expect("close must succeed");

                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let observed_pages: Vec<u64> = content
                    .lines()
                    .skip(1) // header
                    // Column 1: `Report` comes first, and is empty here.
                    .map(|line| line.split(',').nth(1).expect("page cell").parse().expect("numeric page"))
                    .collect();
                let expected_pages: Vec<u64> = (0..COUNT).collect();
                assert_eq!(observed_pages, expected_pages, "all rows must be sorted by page after close()");
            }

            /// Proof, without involving Python, that `close()` reports a real I/O error instead of
            /// panicking or silently returning `Ok`.
            ///
            /// It replaces the layer's writable `File` with an independent, freshly opened
            /// read-only handle on the same path, so the following write fails with a genuine I/O
            /// error rather than succeeding. Deliberately white-box: it reaches the private
            /// `CsvLogLayerInner::file`, which lives in this same file and is visible to its
            /// descendants by Rust's own visibility rules.
            ///
            /// It does **not** sabotage the original file descriptor by closing it from underneath.
            /// Opening a second `File` on the same fd number via `File::from_raw_fd` and dropping
            /// it creates two independent `OwnedFd`s owning one descriptor; when the layer's
            /// original `File` is later dropped, the standard library's I/O-safety hardening
            /// detects the double close and **aborts the whole process** instead of returning an
            /// error. Observed on rustc 1.94.0, reproducible single-threaded, and under parallel
            /// execution able to corrupt another running test's descriptor. Replacing the whole
            /// `File` avoids the double ownership entirely.
            #[test]
            fn close_reports_an_io_error_instead_of_panicking() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "will never reach disk");
                });

                let read_only_handle = std::fs::OpenOptions::new()
                    .read(true)
                    .open(&path)
                    .expect("reopen the csv file read-only");
                {
                    let mut file_guard =
                        layer.inner.file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    // Replacing the whole `File` drops the old, write-capable one normally (its
                    // own, never-shared fd is closed exactly once) and swaps in a read-only
                    // handle that cannot satisfy the write `close()` is about to attempt.
                    *file_guard = Some(read_only_handle);
                }

                let result = layer.close();
                match result {
                    Err(TracingSetupError::CsvWrite { .. }) => {}
                    other => panic!("expected Err(TracingSetupError::CsvWrite), found {other:?}"),
                }
            }
        }
    }

    /// The logs that child processes wrote in their private directories end up in this run's files,
    /// in job order, without spoiling their shape.
    mod absorbing_worker_logs {
        use super::*;

        /// A log directory as a child process would leave it: the three destinations, with the
        /// header every run's `.log.csv` carries by contract.
        fn worker_log_dir(dir: &std::path::Path, name: &str, message: &str) -> std::path::PathBuf {
            let log_dir = dir.join(name);
            std::fs::create_dir_all(&log_dir).unwrap();
            std::fs::write(
                log_dir.join(CSV_FILE_NAME),
                format!("{CSV_HEADER}7,run/job,,,,,{message}\n"),
            )
            .unwrap();
            std::fs::write(log_dir.join(LOG_FILE_NAME), format!("{{\"message\":\"{message}\"}}\n")).unwrap();
            log_dir
        }

        fn handle_writing_into(dir: &std::path::Path) -> LogHandle {
            let handle = log_handle_for_tests(dir).expect("test log handle");
            handle.set_csv_dir(dir).expect("the csv destination must be settable");
            handle
        }

        #[test]
        fn the_rows_of_every_worker_reach_the_run_csv() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            std::fs::create_dir_all(&out).unwrap();
            let handle = handle_writing_into(&out);

            handle.absorb_worker_logs(&worker_log_dir(dir.path(), "job-0", "first job spoke")).unwrap();
            handle.absorb_worker_logs(&worker_log_dir(dir.path(), "job-1", "second job spoke")).unwrap();
            handle.close().unwrap();

            let csv = std::fs::read_to_string(out.join(CSV_FILE_NAME)).unwrap();
            assert!(csv.contains("first job spoke"), "the first worker's row is missing:\n{csv}");
            assert!(csv.contains("second job spoke"), "the second worker's row is missing:\n{csv}");
        }

        /// A second header block in the middle of the file would make it unreadable to any CSV
        /// reader — and the reference `.log.csv` files of the formats repository are read by
        /// pytest.
        #[test]
        fn the_run_csv_keeps_exactly_one_header() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            std::fs::create_dir_all(&out).unwrap();
            let handle = handle_writing_into(&out);

            handle.absorb_worker_logs(&worker_log_dir(dir.path(), "job-0", "a")).unwrap();
            handle.absorb_worker_logs(&worker_log_dir(dir.path(), "job-1", "b")).unwrap();
            handle.close().unwrap();

            let csv = std::fs::read_to_string(out.join(CSV_FILE_NAME)).unwrap();
            assert_eq!(csv.matches(CSV_HEADER.trim_end()).count(), 1, "expected exactly one header in:\n{csv}");
        }

        /// The order is that of the jobs, not that in which the children finished: it is what makes
        /// the log readable as a single run.
        #[test]
        fn workers_are_poured_in_job_order() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            std::fs::create_dir_all(&out).unwrap();
            let handle = handle_writing_into(&out);

            handle.absorb_worker_logs(&worker_log_dir(dir.path(), "job-0", "aaa")).unwrap();
            handle.absorb_worker_logs(&worker_log_dir(dir.path(), "job-1", "bbb")).unwrap();
            handle.close().unwrap();

            let csv = std::fs::read_to_string(out.join(CSV_FILE_NAME)).unwrap();
            assert!(csv.find("aaa") < csv.find("bbb"), "worker rows are out of job order:\n{csv}");
        }

        #[test]
        fn the_json_lines_of_every_worker_reach_the_run_log() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            std::fs::create_dir_all(&out).unwrap();
            let handle = handle_writing_into(&out);

            handle.absorb_worker_logs(&worker_log_dir(dir.path(), "job-0", "first")).unwrap();
            handle.close().unwrap();

            // `log_handle_for_tests` opens `freeports.log.jsonl` in the directory it is given,
            // which here is `out`, not the root of the temporary directory.
            let jsonl = std::fs::read_to_string(out.join(LOG_FILE_NAME)).unwrap();
            assert!(jsonl.contains("\"first\""), "the worker's json line is missing:\n{jsonl}");
        }

        /// A child that died before writing any file must not make closing fail: the missing report
        /// already signals that failure, and losing the parent's log as well would hide it.
        #[test]
        fn a_worker_that_left_no_files_behind_is_not_an_error() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            std::fs::create_dir_all(&out).unwrap();
            let handle = handle_writing_into(&out);

            handle.absorb_worker_logs(&dir.path().join("job-that-never-ran")).expect("an absent log directory is tolerated");
            handle.close().expect("closing must still succeed");
        }

        /// A run with no children must behave exactly as it did before worker processes existed.
        #[test]
        fn absorbing_nothing_leaves_the_run_files_exactly_as_they_were() {
            let dir = tempfile::tempdir().unwrap();
            let out = dir.path().join("out");
            std::fs::create_dir_all(&out).unwrap();
            let handle = handle_writing_into(&out);
            handle.close().unwrap();

            assert_eq!(std::fs::read_to_string(out.join(CSV_FILE_NAME)).unwrap(), CSV_HEADER);
        }
    }

    /// `.log.csv` exists **only** beside the outputs. Falling back to the directory passed to
    /// `init` — the working directory, for the CLI — left a header-only `.log.csv` behind after
    /// every run that failed before the configuration resolved.
    mod csv_never_in_the_working_directory {
        use super::*;

        /// A `LogHandle` whose CSV destination is never settled, with rows pending: the run that
        /// dies before knowing where the outputs go.
        fn handle_with_pending_rows(dir: &std::path::Path) -> LogHandle {
            let handle = log_handle_for_tests(dir).expect("test log handle");
            let subscriber = tracing_subscriber::registry().with(handle.csv.clone());
            tracing::subscriber::with_default(subscriber, || {
                tracing::warn!(page = 7u64, "died before the configuration resolved");
            });
            handle
        }

        #[test]
        fn closing_without_a_destination_leaves_no_csv_behind() {
            let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
            let dir = tempfile::tempdir().expect("tempdir");
            let handle = handle_with_pending_rows(dir.path());
            handle.close().expect("close must succeed even with nowhere to put the csv");
            assert!(
                !dir.path().join(CSV_FILE_NAME).exists(),
                "a run that never settled a destination must not leave a .log.csv behind"
            );
        }

        /// The other half of the contract: once the destination is settled, nothing is lost. Rows
        /// accumulated *before* the call end up in the right file.
        #[test]
        fn a_settled_destination_still_receives_the_rows_logged_before_it_was_known() {
            let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
            let dir = tempfile::tempdir().expect("tempdir");
            let handle = handle_with_pending_rows(dir.path());
            let out_dir = dir.path().join("out");
            handle.set_csv_dir(&out_dir).expect("settling the destination must succeed");
            handle.close().expect("close must succeed");
            let content =
                std::fs::read_to_string(out_dir.join(CSV_FILE_NAME)).expect("read .log.csv");
            assert!(
                content.contains("died before the configuration resolved"),
                "got:\n{content}"
            );
            assert!(
                !dir.path().join(CSV_FILE_NAME).exists(),
                "and still nothing in the working directory, got:\n{content}"
            );
        }
    }

    mod errors {
        use super::*;

        mod display {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn open_log_file_message_includes_path_and_source() {
                let source = std::io::Error::new(std::io::ErrorKind::NotFound, "boom");
                let err = TracingSetupError::OpenLogFile { path: PathBuf::from("/tmp/x/freeports.log"), source };
                assert_eq!(err.to_string(), "cannot open log file at /tmp/x/freeports.log: boom");
            }

            #[test]
            fn open_csv_file_message_includes_path_and_source() {
                let source = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
                let err = TracingSetupError::OpenCsvFile { path: PathBuf::from("/tmp/x/.log.csv"), source };
                assert_eq!(err.to_string(), "cannot open csv log file at /tmp/x/.log.csv: denied");
            }

            #[test]
            fn csv_write_message_includes_source() {
                let source: csv::Error = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe closed").into();
                let err = TracingSetupError::CsvWrite { source };
                assert_eq!(err.to_string(), "failed to write a row to the .log.csv file: pipe closed");
            }

            // `AlreadyInitialized`'s Display and source-chain behavior are covered by
            // `tests::init::first_call_succeeds_and_creates_both_log_files_second_call_reports_already_initialized`
            // rather than here: `tracing::subscriber::SetGlobalDefaultError` has no public
            // constructor, so the only way to obtain a real one is to actually trigger it by
            // calling `set_global_default` twice in the same process; doing that from more than one
            // test would race other tests for who "wins" the one-time global install (`cargo test`
            // runs tests in parallel by default), so all such coverage lives in that single
            // sequential test instead.
        }

        mod source_chain {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn open_log_file_exposes_its_io_error_as_source() {
                let source = std::io::Error::new(std::io::ErrorKind::NotFound, "boom");
                let err = TracingSetupError::OpenLogFile { path: PathBuf::from("x"), source };
                let chained = std::error::Error::source(&err).expect("must expose a source");
                assert_eq!(chained.to_string(), "boom");
            }

            #[test]
            fn open_csv_file_exposes_its_io_error_as_source() {
                let source = std::io::Error::new(std::io::ErrorKind::NotFound, "boom");
                let err = TracingSetupError::OpenCsvFile { path: PathBuf::from("x"), source };
                let chained = std::error::Error::source(&err).expect("must expose a source");
                assert_eq!(chained.to_string(), "boom");
            }

            #[test]
            fn csv_write_exposes_its_csv_error_as_source() {
                let source: csv::Error = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe closed").into();
                let err = TracingSetupError::CsvWrite { source };
                let chained = std::error::Error::source(&err).expect("must expose a source");
                assert_eq!(chained.to_string(), "pipe closed");
            }

            // `AlreadyInitialized`'s source-chain behavior: see the comment in `display` above,
            // covered instead by `tests::init::
            // first_call_succeeds_and_creates_both_log_files_second_call_reports_already_initialized`.

            fn assert_implements_std_error<T: std::error::Error>() {}

            #[test]
            fn tracing_setup_error_is_a_real_std_error_not_just_a_string() {
                assert_implements_std_error::<TracingSetupError>();
            }
        }
    }

    mod global_init {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn first_call_succeeds_and_creates_both_log_files_second_call_reports_already_initialized() {
            let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
            // This is the *only* test in this module allowed to call `init` successfully: a
            // process only ever accepts one `tracing::subscriber::set_global_default` call, ever
            // (not resettable), so this whole scenario -- success, then the resulting
            // `AlreadyInitialized` error's shape, `Display`, and source chain -- is deliberately
            // kept in one sequential test instead of split across several, to avoid racing other
            // tests for who "wins" the one-time global install (`cargo test` runs tests in
            // parallel threads by default).
            let dir = tempfile::tempdir().expect("tempdir");

            let first = init(Verbosity::from_verbose_and_quiet_counts(1, 0), dir.path());
            let handle = match first {
                Ok(handle) => handle,
                other => panic!("first init in this process must succeed, got {other:?}"),
            };
            assert!(dir.path().join(LOG_FILE_NAME).exists());
            // `.log.csv` deliberately does **not** exist yet: since it moved next to the output,
            // its destination is only known once the configuration resolves. `init` leaves it
            // deferred, and `set_csv_dir` is the only thing that ever creates it (L5: there is no
            // longer a fallback to the working directory).
            assert!(
                !dir.path().join(CSV_FILE_NAME).exists(),
                "init must not create .log.csv before a destination is settled"
            );

            let out_dir = dir.path().join("out");
            handle.set_csv_dir(&out_dir).expect("settling the csv destination must succeed");
            assert!(
                out_dir.join(CSV_FILE_NAME).exists(),
                "set_csv_dir must create the directory and write the header straight away"
            );
            assert_eq!(
                std::fs::read_to_string(out_dir.join(CSV_FILE_NAME)).expect("read .log.csv"),
                CSV_HEADER,
                "a run that logs nothing still leaves a header-only file"
            );

            let second = init(Verbosity::from_verbose_and_quiet_counts(1, 0), dir.path());
            let err = match second {
                Err(err @ TracingSetupError::AlreadyInitialized { .. }) => err,
                other => panic!("expected AlreadyInitialized, found {other:?}"),
            };
            assert_eq!(
                err.to_string(),
                "cannot install the global tracing subscriber: a global default trace dispatcher has already been set"
            );
            let source = std::error::Error::source(&err).expect("AlreadyInitialized must expose a source");
            assert_eq!(source.to_string(), "a global default trace dispatcher has already been set");
        }

        #[test]
        fn errors_when_the_log_directory_does_not_exist() {
            let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
            // Independent tempdir, and deliberately never followed by a successful `init` call
            // in this test: `init`'s error path must not depend on whether the process-wide
            // global default is already set by another test (`OpenLogFile`/`OpenCsvFile` must be
            // checked, and fail, before `set_global_default` is ever attempted).
            let dir = tempfile::tempdir().expect("tempdir");
            let missing = dir.path().join("does_not_exist");
            let result = init(Verbosity::from_verbose_and_quiet_counts(0, 0), &missing);
            assert!(
                matches!(
                    result,
                    Err(TracingSetupError::OpenLogFile { .. }) | Err(TracingSetupError::OpenCsvFile { .. })
                ),
                "expected an OpenLogFile or OpenCsvFile error, found {result:?}"
            );
        }
    }
}
