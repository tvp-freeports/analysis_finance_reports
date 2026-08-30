//! Installazione dell'unico sottosistema di logging del crate: solo `tracing`, mai `logging`
//! Python (`PLAN.md` §2 principio 5, §8). Tre destinazioni, ciascuna un `tracing_subscriber`
//! layer indipendente componibile su un `tracing_subscriber::Registry`:
//!
//! 1. stderr, verbosità pilotata da un parametro `Verbosity` (il parsing di `-v`/`-vv`/`-vvv`
//!    è compito di `cli`, milestone M9 — qui si accetta il conteggio già calcolato);
//! 2. `freeports.log.jsonl`, il log diagnostico **strutturato** (L5): un oggetto JSON per riga,
//!    allo stesso livello di stderr (L4 -- non piu' il `debug` fisso di M0);
//! 3. `.log.csv`, un `Layer` custom che intercetta gli eventi che portano (direttamente o per
//!    eredità da uno `span` attivo) almeno uno dei campi `page`/`coord_ref_1`/`coord_ref_2`/
//!    `coord_1`/`coord_2` e li accumula per scriverli come righe CSV, ordinate, alla chiusura
//!    esplicita del layer (`CsvLogLayer::close`, L1 -- vedi più sotto).
//!
//! # Contratto per l'implementazione (i test sotto sono il contratto vincolante; questo modulo
//! # doc è solo una mappa di lettura, in caso di conflitto vincono i test)
//!
//! ## `Verbosity` — **RIAPERTO a M9** (`M9-implementation-plan.md` §0 Q5, su autorizzazione
//! ## esplicita dell'utente: M0 era chiusa, questa è un'estensione di comportamento su codice
//! ## chiuso, non un'iniziativa autonoma — vedi la nota di chiusura M9 in `STATUS.md`).
//!
//! L'enum a 4 varianti (`Warn, Info, Debug, Trace`) e la coppia `from_flag_count`/`level` di M0
//! **spariscono** (non restano deprecate: `Silent` non ha un `tracing::Level` corrispondente,
//! quindi `level()` non può restare com'era), sostituiti da una scala a 6 livelli con `-v`/`-q`
//! come manopole indipendenti:
//!
//! ```text
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub enum Verbosity { Silent, ErrorOnly, Warn, Info, Debug, Trace }
//!
//! impl Verbosity {
//!     /// Ordine crescente di verbosità.
//!     pub const ORDER: [Verbosity; 6] =
//!         [Verbosity::Silent, Verbosity::ErrorOnly, Verbosity::Warn,
//!          Verbosity::Info, Verbosity::Debug, Verbosity::Trace];
//!     /// Indice di `ORDER` quando né `-v` né `-q` compaiono (0 e 0) -- `Warn`.
//!     pub const DEFAULT_INDEX: usize = 2;
//!
//!     /// Sostituisce `from_flag_count`. `-v` e `-q` sono manopole indipendenti sommate con
//!     /// segno rispetto a `DEFAULT_INDEX`, **non** mutuamente esclusive (divergenza deliberata
//!     /// dal riferimento Python, che le tratta come tali -- voluta dall'utente, "independent
//!     /// dials"). L'offset netto è clampato a `[0, ORDER.len()-1]`, mai un panic/indice fuori
//!     /// bound qualunque siano `verbose`/`quiet` (anche ai limiti di `u8`).
//!     pub fn from_verbose_and_quiet_counts(verbose: u8, quiet: u8) -> Verbosity;
//!
//!     /// Sostituisce `level(self) -> tracing::Level`. Usato da `stderr_layer` al posto di
//!     /// `LevelFilter::from_level(verbosity.level())`.
//!     pub fn level_filter(self) -> tracing_subscriber::filter::LevelFilter;
//! }
//! ```
//!
//! Semantica esatta (`M9-implementation-plan.md` §0 Q5, tabella completa):
//!
//! | flag | risultato |
//! |---|---|
//! | nessuno (`0,0`) | `Warn` (mostra `Warn` **e** `Error`, verificato sulla semantica reale di `tracing_subscriber::filter::LevelFilter`, non solo dedotto dal nome) |
//! | `-q` (`0,1`) | `ErrorOnly` |
//! | `-qq` o più (`0,2+`) | `Silent`, clampato |
//! | `-v` (`1,0`) | `Info` |
//! | `-vv` (`2,0`) | `Debug` |
//! | `-vvv` o più (`3+,0`) | `Trace`, clampato |
//! | combinazioni (es. `2,1`) | offset netto `verbose - quiet` sommato a `DEFAULT_INDEX`, clampato -- mai un errore |
//!
//! **Equivalenza con la vecchia `from_flag_count` quando `quiet == 0`** (nessuna perdita di
//! comportamento sul solo `-v`): `(0,0)->Warn`, `(1,0)->Info`, `(2,0)->Debug`, `(3+,0)->Trace`
//! saturato -- identico al vecchio `from_flag_count`.
//!
//! `level_filter()` mappa esattamente:
//! `Silent -> OFF`, `ErrorOnly -> ERROR`, `Warn -> WARN`, `Info -> INFO`, `Debug -> DEBUG`,
//! `Trace -> TRACE`.
//!
//! `stderr_layer(verbosity)` cambia una riga rispetto a M0:
//! `.with_filter(verbosity.level_filter())` al posto di
//! `.with_filter(LevelFilter::from_level(verbosity.level()))` -- unico cambiamento di
//! `stderr_layer` stesso, il resto del modulo (`file_layer`/`CsvLogLayer`/`init`) è indipendente
//! dalla verbosità e non cambia.
//!
//! ## `TracingSetupError`
//!
//! Un solo enum d'errore per il modulo (`PLAN.md` §2 principio 4), con `thiserror`:
//!
//! ```text
//! pub enum TracingSetupError {
//!     OpenLogFile { path: PathBuf, source: std::io::Error },   // il log su file non apribile
//!     OpenCsvFile { path: PathBuf, source: std::io::Error },   // .log.csv non apribile
//!     CsvWrite { source: csv::Error },                          // scrittura riga CSV fallita
//!     AlreadyInitialized { source: tracing::subscriber::SetGlobalDefaultError },
//! }
//! ```
//!
//! Messaggi (`Display`) esatti, verificati dai test in `tests::errors::display`:
//! - `OpenLogFile`: `"cannot open log file at {path}: {source}"`
//! - `OpenCsvFile`: `"cannot open csv log file at {path}: {source}"`
//! - `CsvWrite`: `"failed to write a row to the .log.csv file: {source}"`
//! - `AlreadyInitialized`: `"cannot install the global tracing subscriber: {source}"`
//!
//! ## Layer builder e funzione di init
//!
//! ```text
//! pub fn stderr_layer<S>(verbosity: Verbosity) -> impl tracing_subscriber::Layer<S>
//! where S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>;
//!
//! pub fn file_layer<S>(path: &Path) -> Result<impl tracing_subscriber::Layer<S>, TracingSetupError>
//! where S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>;
//! // Filtra sempre a `debug` (include debug/info/warn/error, esclude trace), indipendentemente
//! // dalla `Verbosity` passata a `stderr_layer` — sono due destinazioni indipendenti.
//! // Apre/crea (troncando) il file al percorso dato; errore -> OpenLogFile.
//!
//! pub struct CsvLogLayer { /* privato, Clone (Arc interno) */ }
//! impl CsvLogLayer {
//!     pub fn create(path: &Path) -> Result<Self, TracingSetupError>;
//!     // Apre/crea (troncando) il file, scrive **subito** (flush incluso, prima del ritorno) la
//!     // riga di intestazione `CSV_HEADER`. Errore d'apertura -> OpenCsvFile. Le righe dati non
//!     // scrivono più in streaming: si accumulano, vedi `close` sotto e "Ciclo di vita".
//!
//!     pub fn close(&self) -> Result<(), TracingSetupError>;
//!     // Ordina tutte le righe accumulate finora per `RowOrderKey` e le scrive, poi fa flush.
//!     // Idempotente (una seconda chiamata, o un `Drop` successivo, non duplica nulla).
//! }
//! impl<S> tracing_subscriber::Layer<S> for CsvLogLayer
//! where S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>;
//!
//! pub const CSV_HEADER: &str =
//!     "Page,Activity,First coord ref,Second coord ref,First coord,Second coord,Message\n";
//!
//! pub fn init(verbosity: Verbosity, log_dir: &Path) -> Result<CsvLogLayer, TracingSetupError>;
//! // Compone i tre layer (stderr_layer(verbosity), file_layer(log_dir/LOG_FILE_NAME),
//! // CsvLogLayer::create(log_dir/".log.csv")) su un tracing_subscriber::registry() e lo installa
//! // con tracing::subscriber::set_global_default. NON usa `Once`: a differenza del vecchio ponte
//! // PyO3 (che doveva tollerare re-inizializzazioni innescate dall'import Python), il binario
//! // Rust chiama `init` esattamente una volta da `main`; una seconda chiamata nello stesso
//! // processo è un errore di programmazione del chiamante, riportato come
//! // `AlreadyInitialized` invece che ignorato in silenzio (mai panico sul percorso utente).
//! // Ordine vincolante: entrambi i file (`freeports.log.jsonl`, `.log.csv`) devono essere aperti con
//! // successo *prima* di tentare `set_global_default` — un `log_dir` invalido deve fallire con
//! // `OpenLogFile`/`OpenCsvFile` senza mai installare un subscriber globale, altrimenti una
//! // singola chiamata fallita per un percorso sbagliato brucerebbe comunque l'unica
//! // inizializzazione possibile del processo. **Firma cambiata da L1**: ritorna l'handle del
//! // layer CSV, che il chiamante deve chiudere esplicitamente con `.close()` prima che il
//! // processo termini -- il subscriber globale installato qui non viene mai droppato a fine
//! // processo, vedi "Ciclo di vita" sotto.
//! ```
//!
//! ## Ciclo di vita di `CsvLogLayer` (L1)
//!
//! `close()` esplicito è l'unico meccanismo supportato per rendere visibili su disco le righe
//! dati accumulate: il subscriber globale installato da `init` non viene mai droppato a fine
//! processo (`tracing::subscriber::set_global_default` installa un `Dispatch` `'static`), quindi
//! non c'è altro modo di svuotare il buffer sul percorso CLI. `Drop` (su `CsvLogLayerInner`, non
//! su ogni singolo clone di `CsvLogLayer`) resta una rete di sicurezza best-effort per gli usi
//! via `tracing::subscriber::with_default` (dove la subscriber viene comunque droppata a fine
//! scope, il percorso Python), ma non è il contratto: i chiamanti devono chiamare `close()`
//! esplicitamente.
//!
//! ## Regola di selezione delle righe di `.log.csv`
//!
//! Per ogni evento, il layer unisce i campi dell'evento con quelli di **tutti** gli span attivi
//! nello stack (dal più esterno al più interno; a parità di nome campo vince lo span più
//! interno; i campi dell'evento vincono su qualunque span). Se l'insieme unito non contiene
//! **nessuno** dei cinque campi taggati (`page`, `coord_ref_1`, `coord_ref_2`, `coord_1`,
//! `coord_2`), l'evento non produce alcuna riga in `.log.csv` (può comunque raggiungere
//! stderr/`freeports.log.jsonl`, che non hanno questo filtro). Se contiene almeno uno di questi campi,
//! viene scritta una riga: le colonne il cui campo non è presente restano cella vuota (non la
//! stringa `"None"` o simili). `Activity` (vedi sotto) non è mai un campo taggato, quindi non
//! basta da sola a far scattare una riga (Q-L2, `L1-implementation-plan.md`).
//!
//! Mappatura campo -> colonna:
//!
//! | campo tracing | colonna CSV |
//! |---|---|
//! | `page` | `Page` |
//! | *(nessuno: calcolata dagli span attivi)* | `Activity` |
//! | `coord_ref_1` | `First coord ref` |
//! | `coord_ref_2` | `Second coord ref` |
//! | `coord_1` | `First coord` |
//! | `coord_2` | `Second coord` |
//! | messaggio dell'evento (`message`) | `Message` |
//!
//! **`Activity`** è il percorso `/`-separato dei nomi degli span attivi al momento dell'evento,
//! dal più esterno al più interno (`activity_path`, calcolato **solo dopo** aver confermato che
//! l'evento produce comunque una riga — vedi il commento nel corpo di `on_event`, e
//! `L1-implementation-plan.md` §2.2/§2.3).
//!
//! I due campi `coord_ref_1`/`coord_ref_2` sono ancoraggi testuali alla stessa posizione (es. una
//! società riconosciuta, il nome di un campo) e non vengono messi sugli eventi uno per uno ma su
//! uno **span** che avvolge la deserializzazione di una riga di investimento, che è il modo
//! idiomatico in `tracing` di dare un contesto a tutti gli eventi che ne discendono (e il layer
//! li eredita già, vedi `on_event`).
//!
//! # Una riga per evento, non tre per fallimento
//!
//! Il riferimento scriveva **tre** righe per ogni campo perso: un `ERROR` con ciò che era andato
//! storto, e due `WARN` con la mitigazione e la conseguenza. Qui un fallimento di cast è una riga
//! sola, che dice entrambe le cose (`"Error casting, skipping field: ..."`) — scelta concordata
//! con l'utente (2026-08-24): il criterio originale resta valido, ma una riga per evento è la
//! forma idiomatica di `tracing`, dove il livello dice già la gravità e il messaggio la
//! conseguenza. Resta invece una riga a sé la mitigazione **riuscita** (il `forcing cast` di
//! `deserialize::cast`), che è un'informazione diversa: lì non si è perso niente.
//!
//! Escaping CSV: nessuna logica scritta a mano, si delega interamente alle regole di default del
//! crate `csv` (delimitatore `,`, quoting `Necessary`, terminatore di riga `\n`) — i test in
//! `tests::csv_layer::csv_escaping` fissano i casi concreti (virgola, virgolette, newline nel
//! valore).
//!
//! La riga di intestazione viene scritta **e resa visibile su disco** prima che
//! `CsvLogLayer::create` ritorni, come sempre. Le righe **dati** non lo sono più (L1): `on_event`
//! le accumula soltanto, ordinate e scritte solo da una chiamata esplicita a `CsvLogLayer::close`
//! (`Drop` resta una rete di sicurezza best-effort, non il contratto — vedi "Ciclo di vita" più
//! sopra). I test chiamano `close()` esplicitamente prima di leggere il file, non si affidano più
//! al solo momento in cui lo scope che ha installato il subscriber finisce.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Silent,
    ErrorOnly,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Verbosity {
    /// Ordine crescente, usato sia dal clamping di `from_verbose_and_quiet_counts` sia dai test
    /// che iterano tutti i livelli.
    pub const ORDER: [Verbosity; 6] = [
        Verbosity::Silent,
        Verbosity::ErrorOnly,
        Verbosity::Warn,
        Verbosity::Info,
        Verbosity::Debug,
        Verbosity::Trace,
    ];
    /// Indice di `ORDER` usato quando `-v`/`-q` non compaiono affatto (0 e 0).
    pub const DEFAULT_INDEX: usize = 2; // Warn

    /// Sostituisce `from_flag_count` (rimossa, non deprecata: equivalente esatto quando
    /// `quiet == 0`). `-v` e `-q` sono manopole indipendenti sommate con segno rispetto a
    /// `DEFAULT_INDEX`, non mutuamente esclusive -- divergenza deliberata dal riferimento
    /// Python (che le tratta come mutuamente esclusive), voluta dall'utente ("independent
    /// dials", `M9-implementation-plan.md` §0 Q5).
    pub fn from_verbose_and_quiet_counts(verbose: u8, quiet: u8) -> Verbosity {
        let offset = i16::from(verbose) - i16::from(quiet);
        let last = (Self::ORDER.len() - 1) as i16;
        let index = (Self::DEFAULT_INDEX as i16 + offset).clamp(0, last);
        Self::ORDER[index as usize]
    }

    /// Sostituisce `level(self) -> tracing::Level` (rimosso: `Silent` non ha un
    /// `tracing::Level` corrispondente, l'assenza di un livello non è un livello).
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
    #[error("cannot write the yaml error log at {}: {source}", path.display())]
    OpenYamlFile { path: PathBuf, source: std::io::Error },
    #[error("cannot serialize the yaml error log for {}: {source}", path.display())]
    YamlWrite { path: PathBuf, source: serde_yaml::Error },
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

/// Renders a span's fields as **values only**, comma-separated, with no `key=` prefix and no
/// `Debug` quoting — the half of `SpanPathFormat` that turns `class{class=investments}` into the
/// `class[investments]` the user asked for (`PLAN.md` §3 L4). Only ever used to build the
/// `FormattedFields` of a *span*: `SpanPathFormat` formats an event's own fields itself, where
/// `key=value` is still the useful form.
///
/// A span with no fields yields an empty string, which `SpanPathFormat` renders as the bare span
/// name (no empty `[]`).
#[derive(Debug, Default, Clone, Copy)]
pub struct SpanValueFields;

/// Collects the values of every field it visits, in declaration order. `record_str` keeps the
/// string raw (no surrounding quotes, unlike the default `Debug`-based field formatter, which is
/// what produced the noisy `pipe{pipe="PdfExtractInvestmentsStandard"}`).
struct ValueListVisitor<'a> {
    writer: Writer<'a>,
    written: bool,
    result: fmt::Result,
}

impl ValueListVisitor<'_> {
    fn write(&mut self, value: std::fmt::Arguments<'_>) {
        if self.result.is_err() {
            return;
        }
        if self.written {
            self.result = write!(self.writer, ",");
        }
        self.written = true;
        if self.result.is_ok() {
            self.result = write!(self.writer, "{value}");
        }
    }
}

impl Visit for ValueListVisitor<'_> {
    fn record_debug(&mut self, _field: &Field, value: &dyn fmt::Debug) {
        self.write(format_args!("{value:?}"));
    }

    fn record_str(&mut self, _field: &Field, value: &str) {
        self.write(format_args!("{value}"));
    }
}

impl<'writer> FormatFields<'writer> for SpanValueFields {
    fn format_fields<R: tracing_subscriber::field::RecordFields>(
        &self,
        writer: Writer<'writer>,
        fields: R,
    ) -> fmt::Result {
        let mut visitor = ValueListVisitor { writer, written: false, result: Ok(()) };
        fields.record(&mut visitor);
        visitor.result
    }
}

/// Event format of **stderr only**. One line:
///
/// ```text
/// DEBUG run/job[EURIZON-EN23]/page[353]: message key=value
/// ```
///
/// Three deliberate differences from `tracing_subscriber`'s default `Format<Full>`, all asked for
/// by the user after the L2 sweep (`PLAN.md` §3 L4/L5):
///
/// 1. spans are joined with `/` and carry their identifying value in brackets
///    (`page[353]`), instead of `:`-joined `name{field=value}` pairs that repeat the span name
///    inside its own braces (`page{page=353}`);
/// 2. the resulting path is **the same string** the `.log.csv` `Activity` column carries (see
///    `activity_path`), so a line on stderr and a row in the CSV name the same place identically;
/// 3. **no timestamp and no `target`** (L5). Both were dropped when `freeports.log` became
///    structured: the module path (`freeports::core::algorithm`) is the longest token on the line
///    and is almost never what a human reading a live run is looking for, while the wall clock is
///    only useful afterwards. Neither is lost — `freeports.log.jsonl` carries both, on every
///    record, in a form a machine can filter on.
///
/// Four colors, not one, so the eye can take the path apart without reading it (L5): the
/// structure recedes, the names carry the shape, the values stand out.
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

/// Writes an event's own fields as `key=value`, skipping `message` (which the caller has already
/// written as the line's text). The mirror image of `ValueListVisitor`, which drops the keys.
struct EventFieldVisitor<'a> {
    writer: Writer<'a>,
    result: fmt::Result,
}

impl EventFieldVisitor<'_> {
    fn write(&mut self, field: &Field, value: std::fmt::Arguments<'_>) {
        // `message` is already the line's text. `error` is deliberately skipped too: by
        // convention every site that attaches one with `log_error` also interpolates it into its
        // message, so printing the field as well would say the same thing twice on the same line
        // — precisely the repetitiveness this format set out to remove. The structured copy is
        // not lost, it is what `.freeports.log.yaml` serializes.
        if self.result.is_err() || field.name() == "message" || field.name() == "error" {
            return;
        }
        self.result = write!(self.writer, " {}={}", field.name(), value);
    }
}

impl Visit for EventFieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.write(field, format_args!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.write(field, format_args!("{value}"));
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
/// `JsonLogLayer` writes its lines into and the thing `LogHandle::close` flushes.
///
/// Buffering is not a micro-optimization here: before L4 the layer wrote straight into a bare
/// `File`, one `write(2)` per event, and it did so at a hardcoded `DEBUG` regardless of
/// verbosity — 33.086 unbuffered syscalls for a single 1140-page job. It is one of the five
/// causes measured in `agent-memory/L4-logging-tuning-plan.md` §0.
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


pub const CSV_HEADER: &str =
    "Page,Activity,First coord ref,Second coord ref,First coord,Second coord,Message\n";

/// Tracing field names that select a `.log.csv` row when at least one of them is present
/// (directly on the event, or inherited from an enclosing span) — see the module doc's "Regola
/// di selezione delle righe di `.log.csv`". Kept separate from `message`, which always feeds the
/// `Message` column but never by itself triggers a row. `Activity` deliberately never appears
/// here: it is never a field recorded into `CapturedFields`, it is computed separately in
/// `on_event` from the active span names themselves — see `activity_path`.
const TAGGED_FIELDS: [&str; 5] = ["page", "coord_ref_1", "coord_ref_2", "coord_1", "coord_2"];

/// Field values collected from a single event or span, restricted to the columns `CsvLogLayer`
/// actually cares about (the five tagged fields above, plus the event's own `message`) — other
/// fields are deliberately not stored, they never reach a CSV column.
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

    /// The five tagged fields plus `message` are the only ones that ever reach a CSV column.
    fn keeps(field: &Field) -> bool {
        let name = field.name();
        name == "message" || TAGGED_FIELDS.contains(&name)
    }

    /// **Only ever called after `keeps` returned true.** Every `record_*` below tests the field
    /// name *before* rendering the value: the pre-L4 version formatted first and discarded
    /// after, so every field of every event in the crate paid a `format!` allocation just to be
    /// dropped (cause 4 of `agent-memory/L4-logging-tuning-plan.md` §0).
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

/// Collects **every** field value of a span, in declaration order, to build `SpanLabel`.
/// Deliberately distinct from `FieldVisitor` (which keeps only the tagged five) and from
/// `ValueListVisitor` (which writes straight into a `fmt::Writer` for the stderr/file layers).
struct SpanLabelVisitor(Vec<String>);

impl Visit for SpanLabelVisitor {
    fn record_debug(&mut self, _field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push(format!("{value:?}"));
    }

    fn record_str(&mut self, _field: &Field, value: &str) {
        self.0.push(value.to_string());
    }
}

impl SpanLabel {
    /// `name` when the span has no fields, `name[v1,v2]` when it has some — the vocabulary of
    /// `PLAN.md` §3 L1 (`page[353]`, `class[investments]`, `format[EURIZON-EN23]`).
    fn build(name: &str, attrs: &span::Attributes<'_>) -> Self {
        let mut visitor = SpanLabelVisitor(Vec::new());
        attrs.record(&mut visitor);
        // Un valore vuoto non produce parentesi: la pipeline senza nome del page-classify si
        // rende `pipeline`, non `pipeline[]`.
        visitor.0.retain(|value| !value.is_empty());
        if visitor.0.is_empty() {
            Self(name.to_string())
        } else {
            Self(format!("{name}[{}]", visitor.0.join(",")))
        }
    }
}

/// `/`-separated path of the currently active spans, outermost to innermost, each rendered as
/// `name[value]` via its `SpanLabel`. Empty string if no span is active.
///
/// Since L4 this is the vocabulary `PLAN.md` §3 L1 actually specified
/// (`run/job[EURIZON-EN23]/step[0]/page[353]/pipeline[investments]/deserialize`) rather than the
/// bare names it produced before (`run/job/step/page/pipeline/deserialize`), and it is the same
/// string the stderr line carries (see `SpanPathFormat`) and the `activity` key of the two
/// structured logs (see `build_record`).
///
/// **Calling this has a real cost** (it walks the whole span stack and allocates a `Vec`/
/// `String` on every call): callers must only invoke it once a row is already known to be
/// emitted (`CapturedFields::has_any_tagged_field()` is true), never unconditionally at the top
/// of `on_event` — see `L1-implementation-plan.md` §2.2 (critic 2026-08-29, point 3).
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
/// **Extension point for L2/P1 — two pitfalls, not just "add more fields"**
/// (`L1-implementation-plan.md` §2.3, critic 2026-08-29, point 4):
/// 1. With `#[derive(Ord)]` on a plain struct, fields compare **in declaration order**. A future
///    "document, page, step, sequence" key must declare `document`/`step` **before** `page`, not
///    append them after `sequence` — appending at the end compiles fine but sorts silently
///    wrong (by page first, by document second — the opposite of the intended hierarchy).
/// 2. A future `document` field must be a job **index** (`u64`, assigned in execution order), not
///    a document id/name `String`: sorting by a `String` id would produce alphabetical order,
///    while today's batch behavior is arrival order of jobs (the sequential `for` in
///    `cli::run::execute`).
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
    cells: [String; 7],
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

/// Best-effort safety net, **not** the supported contract (`L1-implementation-plan.md` §0
/// Q-L1d): the CLI path installs a `'static` global `Dispatch` that the process never drops, so
/// `close()` is the only way to guarantee the buffer reaches disk there. Implemented on
/// `CsvLogLayerInner`, not on `CsvLogLayer`, so it only fires when the **last** `Arc` disappears,
/// not on every `.clone()` dropped. I/O errors are swallowed here (`Drop` has no channel back to
/// a caller); a no-op if the buffer is already empty.
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
/// Shared as a free function, and called by **both** `CsvLogLayer` and `YamlLogLayer`, so that
/// neither depends on the other being installed. That dependency was real and silent: with only
/// the YAML layer in a registry, every record came out with a bare `activity: page` and no
/// coordinates at all, because the labels and fields were only ever written by the CSV layer.
/// Idempotent — whichever layer gets there first does the work.
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

        // Binding: check selection *before* computing `activity_path` (`L1-implementation-plan.md`
        // §2.2/§2.3, critic 2026-08-29 point 3) -- `activity_path` walks the whole span stack and
        // must never run on an event that ends up producing no row.
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
            merged.get("page").to_string(),
            activity,
            merged.get("coord_ref_1").to_string(),
            merged.get("coord_ref_2").to_string(),
            merged.get("coord_1").to_string(),
            merged.get("coord_2").to_string(),
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

/// What `init` hands back: the destinations that hold data in memory and must be settled before
/// the process exits — the accumulated `.log.csv` rows and the `freeports.log.jsonl` buffer. The global
/// `Dispatch` installed by `init` is `'static` and never dropped, so neither one reaches disk
/// without this.
#[derive(Debug, Clone)]
pub struct LogHandle {
    csv: CsvLogLayer,
    file: SharedFileWriter,
    /// `Some` only at maximum verbosity — see `YamlLogLayer` and `init`.
    yaml: Option<YamlLogLayer>,
}

impl LogHandle {
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

    /// Flushes every destination. Attempts the `freeports.log.jsonl` flush even if the CSV one failed,
    /// so a failure in one never costs the diagnostics held by the other; the CSV error wins as
    /// the reported one, being the artifact the integration tests compare.
    pub fn close(&self) -> Result<(), TracingSetupError> {
        // Nessuna destinazione = nessun file (L5). Ci si arriva solo quando la corsa muore
        // *prima* che la configurazione risolva, cioe' prima che `cli::run::execute` sappia dove
        // vanno gli output: fino a L5 in quel caso il registro ripiegava sulla cartella di
        // lavoro, lasciando un `.log.csv` di sola intestazione a ogni corsa fallita. Le righe in
        // sospeso sono tutte eventi che hanno gia' raggiunto stderr e `freeports.log.jsonl`.
        if !self.csv.has_destination() {
            self.csv.discard();
        }
        // Ogni destinazione viene tentata comunque, anche se una precedente ha fallito: un
        // errore su una non deve costare le diagnostiche tenute dalle altre. Il CSV vince come
        // errore riportato, essendo l'artefatto che i test d'integrazione confrontano.
        let csv_result = self.csv.close();
        let yaml_result = self.yaml.as_ref().map_or(Ok(()), YamlLogLayer::close);
        let file_result = self.file.flush();
        csv_result?;
        yaml_result?;
        file_result.map_err(|source| TracingSetupError::OpenLogFile {
            path: PathBuf::from(LOG_FILE_NAME),
            source,
        })
    }
}

/// Un `LogHandle` completo **senza** installare alcun subscriber globale: serve ai test di
/// `cli::run::execute`, che devono poter chiamare `set_csv_dir` senza bruciare l'unica
/// `set_global_default` del processo di test.
#[cfg(test)]
pub fn log_handle_for_tests(log_dir: &Path) -> Result<LogHandle, TracingSetupError> {
    let (_, file_writer) = file_layer::<tracing_subscriber::Registry>(
        &log_dir.join(LOG_FILE_NAME),
        Verbosity::Warn,
    )?;
    Ok(LogHandle {
        csv: CsvLogLayer::deferred(),
        file: file_writer,
        yaml: None,
    })
}

/// Coerces a concrete error into the `&dyn Error` that `tracing` records **structurally**
/// (`Visit::record_error`) rather than as a flat string.
///
/// It exists so a log site can write `error = log_error(&e)` instead of
/// `error = &e as &(dyn std::error::Error + 'static)`, and so the coercion is impossible to get
/// subtly wrong. It is what fills the `error:` key of `.freeports.log.yaml` with a `Debug` form,
/// a `Display` form and the full `source()` chain — see `ErrorRecord`.
///
/// The message of such a site keeps interpolating the error as before: stderr and `.log.csv` stay
/// readable by a human, while the YAML gets the machine-readable version of the same failure.
pub fn log_error<E>(error: &E) -> &(dyn std::error::Error + 'static)
where
    E: std::error::Error + 'static,
{
    error
}

/// File name of the structured error log written only at maximum verbosity — L3 of `PLAN.md` §3.
pub const YAML_FILE_NAME: &str = ".freeports.log.yaml";

/// Whether this run generates `.freeports.log.yaml` at all: **only at maximum verbosity**
/// (`-vvv`), by the user's decision. Extracted from `init` so the rule can be tested without
/// burning the process's one and only `set_global_default`.
pub fn wants_yaml_log(verbosity: Verbosity) -> bool {
    verbosity == Verbosity::Trace
}

/// Level recorded into `.freeports.log.yaml`: warnings and errors. It is the *error* log — the
/// plan calls it "la serializzazione degli errori" — so it takes what went wrong, not a second
/// copy of the trace that `-vvv` already writes to `freeports.log`.
pub const YAML_LEVEL: LevelFilter = LevelFilter::WARN;

/// The error attached to one record, in the **structural** form of Q-L3 option (b): no
/// `Serialize` derived on the crate's ~25 `thiserror` enums, nothing about their shape frozen
/// into a serialization contract, and third-party errors work too.
///
/// `debug` stands in for the `type` field the plan sketched. A `&dyn Error` cannot report its own
/// concrete type name (`type_name_of_val` on a trait object answers `dyn core::error::Error`, and
/// `Error::type_id` is unstable), but `{:?}` on a `thiserror` enum already prints the variant and
/// its fields — `CastError::NotANumber { value: "n/a" }` — which is strictly more than the type
/// name would have been.
#[derive(Debug, Clone, serde::Serialize)]
struct ErrorRecord {
    debug: String,
    display: String,
    /// The `source()` chain, outermost cause first. Empty for an error with no source.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    source: Vec<String>,
}

impl ErrorRecord {
    fn from_error(error: &(dyn std::error::Error + 'static)) -> Self {
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

/// The page coordinates of a record, the same five tagged fields `.log.csv` puts in columns.
/// Omitted entirely when the event carries none of them.
#[derive(Debug, Clone, Default, serde::Serialize)]
struct CoordsRecord {
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
        self.page.is_none()
            && self.first_ref.is_none()
            && self.second_ref.is_none()
            && self.first.is_none()
            && self.second.is_none()
    }
}

/// One entry of the two structured logs: a line of `freeports.log.jsonl`, an item of
/// `.freeports.log.yaml`. Same shape on both, deliberately — they differ in *which* events they
/// take and in how they are serialized, never in what a record says.
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
/// one, and every remaining field as a string. The five coordinates are *not* collected here —
/// they can also be inherited from an enclosing span, which a visitor over the event alone cannot
/// see, so `build_record` resolves them separately.
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
            name if TAGGED_FIELDS.contains(&name) => {}
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
/// (`2026-08-30T08:12:25.626426Z`). Goes through `tracing_subscriber`'s own `SystemTime`
/// formatter rather than a new date dependency: same bytes as before, nothing added to `Cargo.toml`.
fn now_timestamp() -> String {
    let mut buffer = String::new();
    let _ = SystemTime.format_time(&mut Writer::new(&mut buffer));
    buffer
}

/// Turns one event plus its span context into a [`LogRecord`]. Shared by `JsonLogLayer` and
/// `YamlLogLayer` so that the two files can never drift apart on what a record contains.
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
/// `.freeports.log.yaml` (L3) keeps its own job: a small, human-readable digest of the failures
/// only, at maximum verbosity. Both files are built from the same [`LogRecord`].
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

/// `.freeports.log.yaml` — the structured error log of L3, written **only at maximum verbosity**
/// and **in the working directory** (the user's decision, 2026-08-30: it is a diagnostic
/// artifact, unlike `.log.csv`, which travels with the output).
///
/// Same lifecycle as `CsvLogLayer`: records accumulate in memory and reach disk only on `close()`,
/// which `LogHandle::close` calls. Unlike the CSV there is no ordering key — the records are
/// written in the order the events happened, which is the order you want when reading a chain of
/// failures.
#[derive(Debug, Clone)]
pub struct YamlLogLayer {
    inner: Arc<YamlLogLayerInner>,
}

#[derive(Debug)]
struct YamlLogLayerInner {
    path: PathBuf,
    records: Mutex<Vec<LogRecord>>,
}

impl YamlLogLayer {
    /// Does **not** touch the filesystem: an empty run must not leave an empty YAML file behind,
    /// unlike `.log.csv`, whose mere existence is part of the integration-test contract.
    pub fn create(path: &Path) -> Self {
        Self {
            inner: Arc::new(YamlLogLayerInner {
                path: path.to_path_buf(),
                records: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Serializes everything accumulated so far and writes it, then empties the buffer so a
    /// second call is a no-op. Writes nothing at all when no record was collected.
    pub fn close(&self) -> Result<(), TracingSetupError> {
        let mut guard = self.inner.records.lock().unwrap_or_else(PoisonError::into_inner);
        let records = std::mem::take(&mut *guard);
        drop(guard);
        if records.is_empty() {
            return Ok(());
        }
        let yaml = serde_yaml::to_string(&records).map_err(|source| {
            TracingSetupError::YamlWrite { path: self.inner.path.clone(), source }
        })?;
        std::fs::write(&self.inner.path, yaml).map_err(|source| {
            TracingSetupError::OpenYamlFile { path: self.inner.path.clone(), source }
        })
    }
}

impl<S> Layer<S> for YamlLogLayer
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
        let record = build_record(event, &ctx);
        self.inner.records.lock().unwrap_or_else(PoisonError::into_inner).push(record);
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
    // dispatched even at `-q` — cause 1 of the ~100x slowdown measured in
    // `agent-memory/L4-logging-tuning-plan.md` §0. Do not remove `.with_filter` here.
    // L3: il log YAML degli errori esiste **solo** alla verbosita' massima, e resta nella
    // cartella di lavoro (`log_dir`) invece di seguire gli output come fa `.log.csv` — e' un
    // artefatto diagnostico, non un prodotto della corsa. `EventLevelFilter` a `WARN`: e' il log
    // *degli errori*, non un secondo trace.
    let yaml = wants_yaml_log(verbosity)
        .then(|| YamlLogLayer::create(&log_dir.join(YAML_FILE_NAME)));

    let subscriber = tracing_subscriber::registry()
        .with(stderr_layer(verbosity))
        .with(file_layer)
        .with(csv.clone().with_filter(EventLevelFilter::new(csv_level_filter(verbosity))))
        .with(
            yaml.clone()
                .map(|layer| layer.with_filter(EventLevelFilter::new(YAML_LEVEL))),
        );
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|source| TracingSetupError::AlreadyInitialized { source })?;
    Ok(LogHandle { csv, file: file_writer, yaml })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::PoisonError;
    use tracing_subscriber::prelude::*;

    /// Serializes **every** test in this whole `mod tests` block that installs a `tracing`
    /// dispatcher, via `tracing::subscriber::with_default` or `tracing::subscriber::
    /// set_global_default` (`init`). This is a process-wide race, not a per-callsite one: a
    /// `tracing_core` callsite's `Interest` is cached the first time that callsite is ever hit in
    /// the process, as the AND of every *currently live* `Dispatch`'s interest -- on any thread,
    /// not just the one installing the dispatcher that triggers the recomputation
    /// (`tracing_core::callsite::rebuild_interest`). A dispatcher built from a static
    /// `LevelFilter` (as `stderr_layer`/`file_layer`/`init` all use) can therefore permanently
    /// cache `Interest::never()` for a brand-new callsite belonging to a *completely unrelated*
    /// test on another thread, the very first time that callsite fires while the restrictive
    /// dispatcher happens to be alive -- observed concretely: `csv_layer::row_ordering::
    /// dropping_the_last_clone_without_calling_close_still_flushes_accumulated_rows` flaked
    /// (~1 in 15-20 runs of this module, only alongside the rest of the suite, never in
    /// isolation) because its own, otherwise never-touched-elsewhere callsite got poisoned by a
    /// `stderr_layer_observable_filtering` dispatcher alive on another thread at that instant --
    /// `CsvLogLayer::on_event` was then never invoked for that event, so `Drop`'s flush correctly
    /// wrote nothing.
    ///
    /// This extends the narrower precedent already fixed once in this module (see the original,
    /// submodule-local `SERIAL` this replaces, on `stderr_layer_observable_filtering` below): that
    /// fix only serialized tests sharing the exact same callsite. The hazard is broader --
    /// *any* live dispatcher with a static level filter can poison *any* brand-new callsite
    /// elsewhere -- so the fix has to be broader too: one shared lock, held for the whole body of
    /// every test in this file that ever installs a dispatcher, whether or not it looks related to
    /// any other. Same philosophy as before ("eliminare la corsa, non limitarsi a nasconderla"):
    /// this removes the race outright rather than only hiding its one first-observed symptom.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// Joins seven already-escaped cell values with commas and a trailing `\n`, matching the
    /// `.log.csv` column order (`Page,Activity,First coord ref,Second coord ref,First coord,
    /// Second coord,Message` -- L1, `L1-implementation-plan.md` §2.1). Used everywhere below
    /// instead of hand-typed comma counts, which are error prone to read.
    fn row(cells: [&str; 7]) -> String {
        format!("{}\n", cells.join(","))
    }

    /// M9 (`M9-implementation-plan.md` §0 Q5, §4): riscrive completamente `mod verbosity`
    /// (M0) sopra `Verbosity::from_verbose_and_quiet_counts`/`level_filter`, che sostituiscono
    /// `from_flag_count`/`level` -- rimossi, non deprecati (vedi il doc-comment del modulo).
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

        /// `-v`/`-q` sono manopole indipendenti, non mutuamente esclusive (divergenza voluta dal
        /// riferimento Python, `M9-implementation-plan.md` §0 Q5): nessuna combinazione è un
        /// errore.
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

        /// Nessuna perdita di comportamento sul solo `-v` rispetto alla vecchia
        /// `from_flag_count` (rimossa): stessa tabella, `quiet` fissato a 0.
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

    /// Comportamento osservabile di `stderr_layer` (nuovo a M9): prima non serviva distinguere
    /// "nessun evento passa" da "livello più permissivo", perché non esisteva `Silent`.
    ///
    /// **Riscritto rispetto alla prima versione di questi test** (test-writer aveva usato un
    /// layer "spia" fratello aggiunto sullo stesso `Registry` per contare gli eventi che
    /// "superano" il filtro di `stderr_layer`). Quel design è strutturalmente sbagliato per due
    /// motivi indipendenti, verificati empiricamente (non solo per ispezione) prima di riscrivere
    /// — vedi la decisione registrata in `STATUS.md` alla chiusura di M9:
    /// 1. Il filtro applicato con `.with_filter(...)` (`Filtered<L, F, S>`) governa **solo**
    ///    l'`on_event` del layer a cui è attaccato. Un layer fratello aggiunto con un secondo
    ///    `.with(...)` sullo stesso `Registry` non è "dietro" quel filtro: riceve ogni evento
    ///    indipendentemente da cosa `stderr_layer` decida di scrivere.
    /// 2. `tracing` mette in cache l'interesse per singolo *callsite* (riga di codice sorgente) a
    ///    livello di processo, non per singola chiamata. I quattro test originali chiamavano le
    ///    stesse macro (`tracing::error!("e")` ecc., stessa riga) da `#[test]` diversi eseguiti in
    ///    parallelo su thread diversi con dispatcher diversi: una corsa fra `with_default` di test
    ///    concorrenti sulla cache globale del callsite produceva risultati non deterministici
    ///    (osservato: `silent_shows_nothing_at_all` riceveva comunque tutti e 5 i livelli).
    ///
    /// Fix: si cattura l'output **reale** scritto dal layer iniettando un writer di test
    /// (`SharedBuffer`, tramite il seam `fmt_layer_with_writer` usato anche da `stderr_layer`
    /// stesso — si esercita quindi la vera logica di produzione, non una sua reimplementazione),
    /// e i quattro test condividono un `Mutex` che serializza l'unico callsite che hanno in comune
    /// (`emit_one_event_per_level_at`), eliminando la corsa sulla cache di `tracing` invece di
    /// limitarsi a nasconderla.
    mod stderr_layer_observable_filtering {
        use super::*;
        use std::sync::{Arc, Mutex};

        /// Writer di test: un buffer in memoria condiviso, letto dopo la chiusura dello scope di
        /// dispatch. `MakeWriter::make_writer` clona l'`Arc` interno, così ogni scrittura del
        /// layer finisce nello stesso buffer osservato dal test.
        #[derive(Clone, Default, Debug)]
        struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

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

    /// L5: `freeports.log` e' diventato `freeports.log.jsonl` — una riga JSON per evento, con il
    /// `target` che stderr non stampa piu' e l'errore serializzato che stderr non ha mai portato.
    mod json_layer {
        use super::*;

        /// Un errore a due livelli, per avere una `source()` chain vera da serializzare.
        #[derive(Debug, thiserror::Error)]
        #[error("the inner thing broke")]
        struct InnerError;

        #[derive(Debug, thiserror::Error)]
        #[error("the outer thing broke")]
        struct OuterError {
            #[source]
            source: InnerError,
        }

        /// Esegue `body` con un `file_layer` a `Trace`, poi restituisce le righe del file gia'
        /// deserializzate. Ogni riga deve essere JSON valido di per se': e' l'intero senso del
        /// formato a righe, quindi il parsing e' parte dell'asserzione, non un dettaglio del
        /// supporto di test.
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

            /// Il `target` e' precisamente cio' che L5 toglie da stderr: se sparisse anche di
            /// qui, l'informazione sarebbe persa del tutto.
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

            /// Uno span aperto e chiuso non produce righe: il file registra eventi, non span.
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

            /// La richiesta dell'utente: quando l'evento e' legato a un `Err`, il file ne
            /// contiene la deserializzazione — forma `Debug`, forma `Display` e catena di
            /// `source()`, non una sola stringa appiattita.
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

            /// Un evento che non ha niente a che vedere con un errore non deve inventarsi la
            /// chiave.
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

            /// Le stesse coordinate del `.log.csv`, risolte con la stessa regola condivisa
            /// (`build_record`): lo span piu' interno batte quello esterno, l'evento batte ogni
            /// span.
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

        /// Il motivo per cui il file e' a righe e non un unico documento JSON o YAML: i record
        /// raggiungono il disco mentre la corsa procede, quindi il log di un processo morto a
        /// meta' resta leggibile.
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
                // Abbastanza eventi da superare il buffer di `BufWriter` (8 KiB): e' la soglia
                // oltre la quale il contenuto e' gia' sul disco senza che nessuno abbia chiuso
                // niente -- esattamente cio' che si vuole leggere dopo un crash.
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
                    "Page,Activity,First coord ref,Second coord ref,First coord,Second coord,Message\n"
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
                    row(["3", "", "", "", "", "", "page-scoped message"])
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
                        "7",
                        "page_processing[7]",
                        "",
                        "",
                        "",
                        "",
                        "\"no tags on the event itself, page comes from the span\""
                    ])
                );
                assert_eq!(content, expected);
            }

            /// Esaustivo su `TAGGED_FIELDS` (§2.1/§5.4 di `L1-implementation-plan.md`): ciascuno
            /// dei cinque campi taggati, da solo, seleziona una riga -- sostituisce i vecchi test
            /// ad hoc `the_two_company_columns_come_from_their_own_fields`/
            /// `company_match_alone_selects_a_row` con una copertura completa e uniforme.
            ///
            /// Il piano illustra questo test come `#[test_case("page", "1")]` parametrico sul
            /// *nome* del campo, ma le macro di `tracing` richiedono un identificatore di campo
            /// letterale a tempo di compilazione: il nome del campo non può essere una variabile
            /// runtime. Si parametrizza quindi su una funzione di emissione (`fn()`, senza
            /// cattura, coercibile da una closure letterale) invece che su una coppia
            /// nome/valore -- stessa esaustività, adattata al vincolo del linguaggio.
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
                // No active span: `Activity` (index 1) is empty. `coord_1`/`coord_2` carry the
                // unit in the value itself (`"row 3"`, `"col 2"`), per the L1 convention -- no
                // real producer exercises this yet (`L1-implementation-plan.md` §1.2), but the
                // layer must still pass the value through verbatim.
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row([
                        "12",
                        "",
                        "Acme Corp",
                        "NAV",
                        "row 3",
                        "col 2",
                        "value out of expected range"
                    ])
                );
                assert_eq!(content, expected);
            }

            /// Sostituisce `the_two_company_columns_come_from_their_own_fields` (colonna
            /// `Company` eliminata, §1.3 di `L1-implementation-plan.md`): stesso schema, sui nomi
            /// superstiti `coord_ref_1`/`coord_ref_2`.
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
                assert_eq!(cells[2], "Acme Corp", "\"First coord ref\" comes from `coord_ref_1`");
                assert_eq!(cells[3], "NAV", "\"Second coord ref\" comes from `coord_ref_2`");
            }

            // `company_match_alone_selects_a_row` (a hand-written single-field test) is gone: its
            // job -- "each tagged field alone selects a row" -- is now covered exhaustively for
            // all five surviving tagged fields by
            // `selectivity::each_tagged_field_alone_selects_a_row` (§5.4 of
            // `L1-implementation-plan.md`), rather than by one ad hoc test per old field.

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
                        "9",
                        "page_processing[7]",
                        "",
                        "",
                        "",
                        "",
                        "explicit page wins over the span's"
                    ])
                );
                assert_eq!(content, expected);
            }

            /// Esaustività: la sovrascrittura evento-su-span non è provata solo su `page` (test
            /// sopra), ma anche su un campo `coord_*` (§5.1 di `L1-implementation-plan.md`).
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
                    row(["", "field[SPAN_VALUE]", "", "EVENT_VALUE", "", "", "event field wins over the span's"])
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
                        "2",
                        "document_ingest[1]/page_classification[2]",
                        "",
                        "",
                        "",
                        "",
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
                        "4",
                        "document_ingest[4]/field_extraction[ISIN]",
                        "",
                        "ISIN",
                        "6",
                        "",
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
                    "{CSV_HEADER}1,,,,,,\"value, with a comma inside\"\n"
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
                    "{CSV_HEADER}1,,,,,,\"say \"\"hi\"\" to the user\"\n"
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
                let expected = format!("{CSV_HEADER}1,,,,,,\"first line\nsecond line\"\n");
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
                let expected = format!("{CSV_HEADER}1,,\"Acme, Inc.\",,,,ok\n");
                assert_eq!(content, expected);
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
                    row(["1", "", "", "", "", "", "first"]),
                    row(["2", "", "", "", "", "", "second"])
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
                    row(["1", "", "", "", "", "", "first"]),
                    row(["2", "", "", "", "", "", "second"])
                );
                assert_eq!(content, expected);
            }
        }

        /// Colonna `Activity` (§2.2/§5.2 di `L1-implementation-plan.md`): percorso `/`-separato
        /// dei nomi degli span attivi, dal più esterno al più interno. `Activity` non è mai un
        /// campo taggato (§1.4 -- non entra mai in `CapturedFields`/`TAGGED_FIELDS`), quindi non
        /// basta da sola a selezionare una riga: gli ultimi due test di questo modulo pinnano
        /// esattamente questa invariante (Q-L2).
        /// `EventLevelFilter` — il filtro che rende sostenibile il costo del logging senza
        /// perdere il contesto degli eventi che restano. Ogni test qui difende una delle due
        /// meta' del contratto: gli span passano sempre, gli eventi no.
        /// La destinazione differita del `.log.csv` — cio' che gli permette di finire accanto
        /// agli output invece che nella cwd, pur essendo il layer installato molto prima che la
        /// configurazione dica dove sono gli output.
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
                    row(["3", "", "", "", "", "", "logged before anyone knew where to put it"])
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

            /// Chiudere senza aver mai fissato una destinazione non deve perdere le righe in
            /// silenzio: e' un errore esplicito. `LogHandle::close` non ci arriva mai — da L5
            /// chiama `discard()` invece di ripiegare sulla cartella di lavoro — ma il layer da
            /// solo deve dirlo, invece di ingoiare.
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

            /// Nessuna riga e nessuna destinazione: non c'e' niente da scrivere, quindi non c'e'
            /// niente da segnalare.
            #[test]
            fn closing_an_empty_layer_without_a_destination_is_a_no_op() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                CsvLogLayer::deferred().close().expect("nothing accumulated, nothing to write");
            }
        }

        mod event_level_filter {
            use super::*;
            use pretty_assertions::assert_eq;
            use tracing_subscriber::Layer;

            /// La regressione che ha motivato l'esistenza di questo tipo: con un
            /// `LevelFilter::WARN` semplice al posto suo, un `info_span!` non viene mai aperto,
            /// il `warn!` interno perde il campo `page` ereditato e la riga non viene neppure
            /// selezionata. Su un job reale erano 391 righe che diventavano 0.
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
                        "42",
                        "page[42]",
                        "",
                        "",
                        "",
                        "",
                        "something went wrong on this page"
                    ])
                );
                assert_eq!(content, expected);
            }

            /// L'altra meta': gli eventi sotto il livello richiesto non arrivano, anche quando
            /// sono dentro uno span che il filtro lascia passare.
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

            /// `max_level_hint` e' cio' che spegne i `debug!`/`trace!` al callsite (il grosso del
            /// guadagno): deve restare `INFO` finche' gli span sono `info_span!`, e non scendere
            /// al livello degli eventi.
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

            /// Sopra il livello degli span il suggerimento segue la verbosita' richiesta,
            /// altrimenti `-vv`/`-vvv` non mostrerebbero nulla di piu'.
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

            /// `-qq` deve essere silenzio totale: nemmeno la contabilita' degli span.
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
                    format!("{CSV_HEADER}{}", row(["1", "", "", "", "", "", "top-level no span"]));
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
                    row(["7", "page_processing[7]", "", "", "", "", "no fields of its own"])
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
                    row(["3", "run/job/document", "", "", "", "", "three levels deep"])
                );
                assert_eq!(content, expected);
            }

            /// Pinna Q-L2/§1.4 di `L1-implementation-plan.md`: uno span attivo interamente privo
            /// di campi taggati non produce una riga, anche se `Activity` avrebbe comunque un
            /// valore non vuoto -- `Activity` da sola non è mai un motivo per scrivere una riga.
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

            /// Un span esterno privo di campi taggati non impedisce a un campo più interno di
            /// selezionare la riga, e il suo nome compare comunque in `Activity` accanto a quello
            /// dello span che ha davvero portato il campo.
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
                        "5",
                        "untagged_outer/tagged_inner[5]",
                        "",
                        "",
                        "",
                        "",
                        "no fields of its own -- page comes from the inner span"
                    ])
                );
                assert_eq!(content, expected);
            }
        }

        /// Determinismo (§2.3/§5.3 di `L1-implementation-plan.md`): le righe non sono più scritte
        /// in streaming, si accumulano in un buffer e vengono ordinate per `RowOrderKey`
        /// (`(pagina, sequenza di arrivo)`) solo a `close()`. Questo modulo è la sola prova che il
        /// meccanismo di ordinamento funzioni -- nessun test di integrazione lo esercita (§6.1 del
        /// piano: il confronto pytest del repo formati ordina già entrambi i lati, e i soli 4 file
        /// di riferimento con righe dati sono già in ordine di pagina crescente).
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
                    row(["2", "", "", "", "", "", "at page two"]),
                    row(["5", "", "", "", "", "", "at page five"]),
                    row(["8", "", "", "", "", "", "at page eight"])
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
                    row(["4", "", "", "", "", "", "first at page four"]),
                    row(["4", "", "", "", "", "", "second at page four"])
                );
                assert_eq!(
                    content, expected,
                    "the arrival sequence must break ties between rows sharing the same page"
                );
            }

            /// Pinna la scelta di §0 Q-L1c di `L1-implementation-plan.md`: nessuna fixture reale
            /// la esercita oggi, è una scelta di principio.
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
                    row(["3", "", "", "", "", "", "numbered"]),
                    row(["", "", "no page here", "", "", "", "unnumbered"])
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

            /// Rete di sicurezza `Drop` (§0 Q-L1d): senza mai chiamare `close()`, quando l'unico
            /// `Arc` restante esce di scope il buffer viene comunque svuotato su disco.
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
                    row(["9", "", "", "", "", "", "flushed only via Drop -- close() never called"])
                );
                assert_eq!(content, expected);
            }

            /// `Drop` va implementato su `CsvLogLayerInner`, non su `CsvLogLayer`: deve scattare
            /// solo quando l'ultimo `Arc` sparisce, non a ogni singolo clone droppato.
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
                    format!("{CSV_HEADER}{}", row(["1", "", "", "", "", "", "not yet flushed"]));
                assert_eq!(after, expected, "dropping the last remaining clone must flush");
            }

            /// Test di stress in stile "long list" (`F2-implementation-plan.md` §5.5): molti
            /// eventi in un ordine di pagina non crescente e fisso (non casuale, per essere
            /// riproducibile), tutti ordinati dopo `close()`.
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
                    .map(|line| line.split(',').next().expect("page cell").parse().expect("numeric page"))
                    .collect();
                let expected_pages: Vec<u64> = (0..COUNT).collect();
                assert_eq!(observed_pages, expected_pages, "all rows must be sorted by page after close()");
            }

            /// Critic 2026-08-29, punto 1 (`L1-implementation-plan.md` §2.5/§5.3): la prova, senza
            /// Python, che `close()` riporta un vero errore di I/O invece di panicare o di
            /// restituire silenziosamente `Ok` -- è il pezzo verificabile a livello di
            /// `CsvLogLayer` della precedenza d'errore corretta in `main.rs`/`py_run_job`.
            ///
            /// Sostituisce il `File` scrivibile del layer con un handle indipendente, fresco,
            /// aperto in sola lettura sullo stesso percorso: la scrittura successiva fallisce con
            /// un vero errore di I/O (il descrittore non è aperto in scrittura) invece di riuscire
            /// o di panicare. Bianco-scatola deliberato: accede al campo privato
            /// `CsvLogLayerInner::file` (§2.3 del piano), che vive nello stesso file e lo espone ai
            /// suoi discendenti per costruzione delle regole di visibilità di Rust.
            ///
            /// **Non** si sabota il file descriptor originale chiudendolo da sotto (una prima
            /// versione di questo test lo faceva aprendo un secondo `File` sullo stesso numero di
            /// fd via `File::from_raw_fd` e droppandolo subito): quel pattern crea due `OwnedFd`
            /// indipendenti proprietari dello stesso fd, e quando il `File` originale del layer
            /// viene droppato più avanti (fine scope del test, o `Drop` su un altro clone),
            /// l'hardening I/O-safety della libreria standard rileva il "double close" e
            /// **abortisce l'intero processo** (`fatal runtime error: IO Safety violation`) invece
            /// di limitarsi a restituire un errore -- osservato concretamente su rustc 1.94.0,
            /// riproducibile anche a singolo thread, e in esecuzione parallela capace di
            /// corrompere il fd di un altro test in corso. Rimpiazzare l'intero `File` (invece di
            /// duplicarne solo il numero di fd) evita del tutto la doppia proprietà.
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

    /// `YamlLogLayer` — il log strutturato degli errori di L3. I test sono raggruppati per
    /// argomento: forma del record, l'errore serializzato (la ragione d'essere del file), le
    /// coordinate ereditate dagli span, il ciclo di vita del file.
    /// L5: `.log.csv` esiste **solo** accanto agli output. Fino a L4 `LogHandle::close`
    /// ripiegava sulla cartella passata a `init` — la cartella di lavoro, per la CLI — cosi' che
    /// ogni corsa fallita prima della risoluzione della configurazione vi lasciava un `.log.csv`
    /// di sola intestazione. Riprodotto dall'utente e da
    /// `agent-memory/L5-structured-log-plan.md`.
    mod csv_never_in_the_working_directory {
        use super::*;

        /// Un `LogHandle` la cui destinazione CSV non viene mai fissata, con righe in sospeso:
        /// e' la corsa che muore prima di sapere dove vanno gli output.
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

        /// L'altra meta' del contratto: fissata la destinazione, non si perde niente. Le righe
        /// accumulate *prima* della chiamata finiscono nel file giusto.
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

    mod yaml_layer {
        use super::*;

        /// Un errore a due livelli, per avere una `source()` chain vera da serializzare.
        #[derive(Debug, thiserror::Error)]
        #[error("the inner thing broke")]
        struct InnerError;

        #[derive(Debug, thiserror::Error)]
        #[error("the outer thing broke")]
        struct OuterError {
            #[source]
            source: InnerError,
        }

        /// Esegue `body` con un `YamlLogLayer` filtrato come in produzione e restituisce il
        /// contenuto del file, oppure `None` se il layer non ne ha scritto nessuno.
        fn yaml_after(body: impl FnOnce()) -> Option<String> {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join(YAML_FILE_NAME);
            let layer = YamlLogLayer::create(&path);
            let subscriber = tracing_subscriber::registry()
                .with(layer.clone().with_filter(EventLevelFilter::new(YAML_LEVEL)));
            tracing::subscriber::with_default(subscriber, body);
            layer.close().expect("close must succeed");
            path.exists().then(|| std::fs::read_to_string(&path).expect("read the yaml log"))
        }

        mod record_shape {
            use super::*;

            #[test]
            fn a_warning_records_activity_level_target_and_message() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let yaml = yaml_after(|| {
                    let span = tracing::info_span!("page", page = 12u64);
                    span.in_scope(|| tracing::warn!("something is off"));
                })
                .expect("a warning must produce a file");
                assert!(yaml.contains("activity: page[12]"), "got:\n{yaml}");
                assert!(yaml.contains("level: WARN"), "got:\n{yaml}");
                assert!(yaml.contains("message: something is off"), "got:\n{yaml}");
                assert!(
                    yaml.contains("target: freeports::core::tracing_setup"),
                    "got:\n{yaml}"
                );
            }

            /// I campi che non sono ne' messaggio ne' coordinate non si perdono.
            #[test]
            fn other_event_fields_are_kept_under_fields() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let yaml = yaml_after(|| {
                    tracing::warn!(format = "EURIZON-EN23", "format is unhappy");
                })
                .expect("a warning must produce a file");
                assert!(yaml.contains("format: EURIZON-EN23"), "got:\n{yaml}");
            }
        }

        mod serialized_error {
            use super::*;

            /// La ragione d'essere del file: un sito che registra l'errore con `log_error`
            /// ottiene forma `Debug`, forma `Display` e l'intera catena di `source()`, invece di
            /// una sola stringa appiattita.
            #[test]
            fn log_error_serializes_display_debug_and_the_whole_source_chain() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let yaml = yaml_after(|| {
                    let e = OuterError { source: InnerError };
                    tracing::error!(error = log_error(&e), "it failed: {e}");
                })
                .expect("an error must produce a file");
                assert!(yaml.contains("display: the outer thing broke"), "got:\n{yaml}");
                assert!(yaml.contains("OuterError { source: InnerError }"), "got:\n{yaml}");
                assert!(yaml.contains("the inner thing broke"), "got:\n{yaml}");
            }

            /// Un errore senza cause non deve produrre una chiave `source` vuota.
            #[test]
            fn an_error_without_a_source_omits_the_source_key() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let yaml = yaml_after(|| {
                    let e = InnerError;
                    tracing::error!(error = log_error(&e), "it failed: {e}");
                })
                .expect("an error must produce a file");
                assert!(!yaml.contains("source:"), "got:\n{yaml}");
            }

            /// Un sito che interpola l'errore nel messaggio senza `log_error` resta valido: il
            /// record esiste, semplicemente senza la parte strutturata.
            #[test]
            fn a_site_without_log_error_still_records_the_message() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let yaml = yaml_after(|| tracing::error!("it failed: something"))
                    .expect("an error must produce a file");
                assert!(yaml.contains("message: 'it failed: something'"), "got:\n{yaml}");
                assert!(!yaml.contains("error:"), "got:\n{yaml}");
            }
        }

        mod inherited_coordinates {
            use super::*;

            /// Le stesse coordinate del `.log.csv`, risolte con la stessa regola: lo span piu'
            /// interno batte quello esterno, l'evento batte ogni span.
            #[test]
            fn coordinates_come_from_the_enclosing_spans_too() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let yaml = yaml_after(|| {
                    let outer = tracing::info_span!("page", page = 44u64);
                    outer.in_scope(|| {
                        let inner = tracing::info_span!("field", coord_ref_2 = "market value");
                        inner.in_scope(|| tracing::warn!(coord_1 = "row 12", "cast failed"));
                    });
                })
                .expect("a warning must produce a file");
                assert!(yaml.contains("page: '44'"), "got:\n{yaml}");
                assert!(yaml.contains("second_ref: market value"), "got:\n{yaml}");
                assert!(yaml.contains("first: row 12"), "got:\n{yaml}");
            }

            #[test]
            fn an_event_with_no_coordinates_omits_the_coords_key() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let yaml = yaml_after(|| tracing::warn!("nowhere in particular"))
                    .expect("a warning must produce a file");
                assert!(!yaml.contains("coords:"), "got:\n{yaml}");
            }
        }

        mod file_lifecycle {
            use super::*;

            /// A differenza del `.log.csv`, la cui sola esistenza fa parte del contratto dei test
            /// d'integrazione, una corsa senza errori non deve lasciare in giro un file vuoto.
            #[test]
            fn a_run_with_no_warnings_leaves_no_file_at_all() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                assert!(yaml_after(|| tracing::info!("all good")).is_none());
            }

            /// E' il log **degli errori**: `info!`/`debug!`/`trace!` non vi finiscono, nemmeno
            /// alla verbosita' che lo genera.
            #[test]
            fn only_warnings_and_errors_reach_it() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let yaml = yaml_after(|| {
                    tracing::info!("info-marker");
                    tracing::debug!("debug-marker");
                    tracing::trace!("trace-marker");
                    tracing::warn!("warn-marker");
                    tracing::error!("error-marker");
                })
                .expect("two events must produce a file");
                assert!(yaml.contains("warn-marker"), "got:\n{yaml}");
                assert!(yaml.contains("error-marker"), "got:\n{yaml}");
                for absent in ["info-marker", "debug-marker", "trace-marker"] {
                    assert!(!yaml.contains(absent), "{absent} must not be there, got:\n{yaml}");
                }
            }

            #[test]
            fn close_is_idempotent() {
                let _guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(YAML_FILE_NAME);
                let layer = YamlLogLayer::create(&path);
                let subscriber = tracing_subscriber::registry().with(layer.clone());
                tracing::subscriber::with_default(subscriber, || tracing::warn!("once"));
                layer.close().expect("first close");
                let first = std::fs::read_to_string(&path).expect("read the yaml log");
                layer.close().expect("second close must be a no-op");
                let second = std::fs::read_to_string(&path).expect("read the yaml log again");
                assert_eq!(first, second, "a second close must not duplicate or truncate");
            }
        }

        /// La regola dell'utente: il file esiste **solo** a verbosita' massima. `init` non e'
        /// chiamabile qui (una sola `set_global_default` per processo), quindi si esercita la
        /// funzione che `init` interroga, esaustivamente su tutti e sei i livelli.
        mod generated_only_at_max_verbosity {
            use super::*;
            use test_case::test_case;

            #[test_case(Verbosity::Silent, false)]
            #[test_case(Verbosity::ErrorOnly, false)]
            #[test_case(Verbosity::Warn, false)]
            #[test_case(Verbosity::Info, false)]
            #[test_case(Verbosity::Debug, false)]
            #[test_case(Verbosity::Trace, true)]
            fn only_trace_verbosity_asks_for_the_yaml_layer(verbosity: Verbosity, expected: bool) {
                assert_eq!(wants_yaml_log(verbosity), expected);
            }
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
            // calling `set_global_default` twice in the same process; doing that from more than
            // one test would race other tests for who "wins" the one-time global install
            // (`cargo test` runs tests in parallel by default), so all such coverage lives in
            // that single sequential test instead.
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
