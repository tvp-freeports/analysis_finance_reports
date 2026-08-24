//! Installazione dell'unico sottosistema di logging del crate: solo `tracing`, mai `logging`
//! Python (`PLAN.md` §2 principio 5, §8). Tre destinazioni, ciascuna un `tracing_subscriber`
//! layer indipendente componibile su un `tracing_subscriber::Registry`:
//!
//! 1. stderr, verbosità pilotata da un parametro `Verbosity` (il parsing di `-v`/`-vv`/`-vvv`
//!    è compito di `cli`, milestone M9 — qui si accetta il conteggio già calcolato);
//! 2. `freeports.log`, sempre a livello `debug` (non configurabile dalla verbosità di stderr);
//! 3. `.log.csv`, un `Layer` custom che intercetta gli eventi che portano (direttamente o per
//!    eredità da uno `span` attivo) almeno uno dei campi `page`/`company`/`field`/`row`/`column`
//!    e li scrive come riga CSV.
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
//!     OpenLogFile { path: PathBuf, source: std::io::Error },   // freeports.log non apribile
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
//! pub struct CsvLogLayer { /* privato */ }
//! impl CsvLogLayer {
//!     pub fn create(path: &Path) -> Result<Self, TracingSetupError>;
//!     // Apre/crea (troncando) il file, scrive **subito** (flush incluso, prima del ritorno) la
//!     // riga di intestazione `CSV_HEADER`. Errore d'apertura -> OpenCsvFile.
//! }
//! impl<S> tracing_subscriber::Layer<S> for CsvLogLayer
//! where S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>;
//!
//! pub const CSV_HEADER: &str = "Page,Matched Company,Company,Field name,Row,Column,Message\n";
//!
//! pub fn init(verbosity: Verbosity, log_dir: &Path) -> Result<(), TracingSetupError>;
//! // Compone i tre layer (stderr_layer(verbosity), file_layer(log_dir/"freeports.log"),
//! // CsvLogLayer::create(log_dir/".log.csv")) su un tracing_subscriber::registry() e lo installa
//! // con tracing::subscriber::set_global_default. NON usa `Once`: a differenza del vecchio ponte
//! // PyO3 (che doveva tollerare re-inizializzazioni innescate dall'import Python), il binario
//! // Rust chiama `init` esattamente una volta da `main`; una seconda chiamata nello stesso
//! // processo è un errore di programmazione del chiamante, riportato come
//! // `AlreadyInitialized` invece che ignorato in silenzio (mai panico sul percorso utente).
//! // Ordine vincolante: entrambi i file (`freeports.log`, `.log.csv`) devono essere aperti con
//! // successo *prima* di tentare `set_global_default` — un `log_dir` invalido deve fallire con
//! // `OpenLogFile`/`OpenCsvFile` senza mai installare un subscriber globale, altrimenti una
//! // singola chiamata fallita per un percorso sbagliato brucerebbe comunque l'unica
//! // inizializzazione possibile del processo.
//! ```
//!
//! ## Regola di selezione delle righe di `.log.csv`
//!
//! Per ogni evento, il layer unisce i campi dell'evento con quelli di **tutti** gli span attivi
//! nello stack (dal più esterno al più interno; a parità di nome campo vince lo span più
//! interno; i campi dell'evento vincono su qualunque span). Se l'insieme unito non contiene
//! **nessuno** dei cinque campi taggati (`page`, `company`, `field`, `row`, `column`), l'evento
//! non produce alcuna riga in `.log.csv` (può comunque raggiungere stderr/`freeports.log`, che
//! non hanno questo filtro). Se contiene almeno uno di questi campi, viene scritta una riga:
//! le colonne il cui campo non è presente restano cella vuota (non la stringa `"None"` o simili).
//!
//! Mappatura campo -> colonna:
//!
//! | campo tracing | colonna CSV |
//! |---|---|
//! | `page` | `Page` |
//! | *(nessuno, vedi nota sotto)* | `Matched Company` |
//! | `company` | `Company` |
//! | `field` | `Field name` |
//! | `row` | `Row` |
//! | `column` | `Column` |
//! | messaggio dell'evento (`message`) | `Message` |
//!
//! **Domanda aperta, segnalata dal test-writer**: il vecchio `.log.csv` (Python,
//! `_internals/core/logging.py::CsvFormatter`) popolava sia `Matched Company` (il nome della
//! società così come appare nel PDF) sia `Company` (la società riconosciuta dall'algoritmo) da
//! un'unica stringa `vertical_ref` incastrata a runtime. Le istruzioni di questa milestone
//! elencano solo cinque campi tracing (`page`/`company`/`field`/`row`/`column`), senza un campo
//! distinto per la società "come scritta nel PDF". Decisione presa qui, da confermare: la
//! colonna `Matched Company` resta **sempre vuota** in M0 (nessun campo tracing la alimenta); la
//! colonna va popolata in una milestone successiva quando/se verrà introdotto un campo tracing
//! dedicato (es. `company_match`). I test sotto (`tests::csv_layer::field_capture::
//! matched_company_column_is_always_blank_in_m0`) fissano questo comportamento come atteso *per
//! ora*: se la decisione viene ribaltata, quel test va aggiornato insieme all'implementazione.
//!
//! Escaping CSV: nessuna logica scritta a mano, si delega interamente alle regole di default del
//! crate `csv` (delimitatore `,`, quoting `Necessary`, terminatore di riga `\n`) — i test in
//! `tests::csv_layer::csv_escaping` fissano i casi concreti (virgola, virgolette, newline nel
//! valore).
//!
//! Ogni riga (intestazione compresa) viene scritta **e resa visibile su disco** prima che la
//! chiamata che l'ha prodotta (`CsvLogLayer::create`, oppure l'evento `tracing` che attraversa
//! `on_event`) ritorni: nessun buffering che sopravviva a una singola riga. I test leggono il
//! file subito dopo la chiusura dello scope che ha installato il subscriber (fine di
//! `tracing::subscriber::with_default`), senza flush espliciti aggiuntivi.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing::field::{Field, Visit};
use tracing::span;
use tracing::{Event, Subscriber};
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
}

/// Shared builder behind `stderr_layer`, parameterized over the writer so tests can inject an
/// in-memory buffer instead of real stderr and assert on the layer's actual formatted output —
/// see `tests::stderr_layer_observable_filtering`. `stderr_layer` itself is the only production
/// caller, with the real stderr writer and ANSI colors on; this seam changes no observable
/// behavior of `stderr_layer`.
fn fmt_layer_with_writer<S, W>(writer: W, filter: LevelFilter, ansi: bool) -> impl Layer<S> + std::fmt::Debug
where
    S: Subscriber + for<'span> LookupSpan<'span> + std::fmt::Debug,
    W: for<'writer> tracing_subscriber::fmt::MakeWriter<'writer> + 'static + std::fmt::Debug,
{
    tracing_subscriber::fmt::layer().with_writer(writer).with_ansi(ansi).with_filter(filter)
}

pub fn stderr_layer<S>(verbosity: Verbosity) -> impl Layer<S> + std::fmt::Debug
where
    S: Subscriber + for<'span> LookupSpan<'span> + std::fmt::Debug,
{
    fmt_layer_with_writer(std::io::stderr as fn() -> std::io::Stderr, verbosity.level_filter(), true)
}

pub fn file_layer<S>(path: &Path) -> Result<impl Layer<S> + std::fmt::Debug, TracingSetupError>
where
    S: Subscriber + for<'span> LookupSpan<'span> + std::fmt::Debug,
{
    let file = File::create(path)
        .map_err(|source| TracingSetupError::OpenLogFile { path: path.to_path_buf(), source })?;
    Ok(tracing_subscriber::fmt::layer()
        .with_writer(file)
        .with_ansi(false)
        .with_filter(LevelFilter::DEBUG))
}

pub const CSV_HEADER: &str = "Page,Matched Company,Company,Field name,Row,Column,Message\n";

/// Tracing field names that select a `.log.csv` row when at least one of them is present
/// (directly on the event, or inherited from an enclosing span) — see the module doc's "Regola
/// di selezione delle righe di `.log.csv`". Kept separate from `message`, which always feeds the
/// `Message` column but never by itself triggers a row.
const TAGGED_FIELDS: [&str; 5] = ["page", "company", "field", "row", "column"];

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

    fn record(&mut self, field: &Field, value: String) {
        let name = field.name();
        if name == "message" || TAGGED_FIELDS.contains(&name) {
            self.0.0.insert(name, value);
        }
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record(field, format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record(field, value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record(field, value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record(field, value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record(field, value.to_string());
    }
}

#[derive(Debug)]
pub struct CsvLogLayer {
    writer: Mutex<csv::Writer<File>>,
}

impl CsvLogLayer {
    pub fn create(path: &Path) -> Result<Self, TracingSetupError> {
        let mut file = File::create(path)
            .map_err(|source| TracingSetupError::OpenCsvFile { path: path.to_path_buf(), source })?;
        file.write_all(CSV_HEADER.as_bytes())
            .and_then(|()| file.flush())
            .map_err(|source| TracingSetupError::CsvWrite { source: source.into() })?;
        Ok(Self { writer: Mutex::new(csv::Writer::from_writer(file)) })
    }
}

impl<S> Layer<S> for CsvLogLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::new();
        attrs.record(&mut visitor);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(visitor.0);
        }
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::new();
        values.record(&mut visitor);
        if let Some(span) = ctx.span(id) {
            let mut extensions = span.extensions_mut();
            if let Some(existing) = extensions.get_mut::<CapturedFields>() {
                existing.merge_from(&visitor.0);
            } else {
                extensions.insert(visitor.0);
            }
        }
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

        if !merged.has_any_tagged_field() {
            return;
        }

        let row = [
            merged.get("page"),
            "", // Matched Company: always blank in M0, see the module doc's "domanda aperta".
            merged.get("company"),
            merged.get("field"),
            merged.get("row"),
            merged.get("column"),
            merged.get("message"),
        ];

        // `Layer::on_event` returns `()`, so a write/flush failure here has no channel back to
        // the caller; it is swallowed rather than panicking on the tracing hot path. `init`'s
        // `CsvWrite` variant exists for callers that build/use a `CsvLogLayer` directly.
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_record(row).and_then(|()| writer.flush().map_err(csv::Error::from));
        }
    }
}

pub fn init(verbosity: Verbosity, log_dir: &Path) -> Result<(), TracingSetupError> {
    use tracing_subscriber::layer::SubscriberExt;

    let file = file_layer(&log_dir.join("freeports.log"))?;
    let csv = CsvLogLayer::create(&log_dir.join(".log.csv"))?;

    let subscriber = tracing_subscriber::registry().with(stderr_layer(verbosity)).with(file).with(csv);
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|source| TracingSetupError::AlreadyInitialized { source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tracing_subscriber::prelude::*;

    /// Joins seven already-escaped cell values with commas and a trailing `\n`, matching the
    /// `.log.csv` column order (`Page,Matched Company,Company,Field name,Row,Column,Message`).
    /// Used everywhere below instead of hand-typed comma counts, which are error prone to read.
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

        /// Serializza i quattro test di questo sottomodulo: sono le uniche chiamate nel processo
        /// che condividono l'esatto callsite di `emit_one_event_per_level_at` (stessa riga di
        /// codice sorgente), quindi sono le uniche a rischio di corsa sulla cache di interesse
        /// globale di `tracing` se eseguite in parallelo (`cargo test` di default usa più thread).
        static SERIAL: Mutex<()> = Mutex::new(());

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
                let path = dir.path().join("freeports.log");
                assert!(file_layer::<tracing_subscriber::Registry>(&path).is_ok());
                assert!(path.exists());
            }

            #[test]
            fn errors_when_the_parent_directory_does_not_exist() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join("missing_subdir").join("freeports.log");
                let err = file_layer::<tracing_subscriber::Registry>(&path)
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

            #[test]
            fn captures_debug_and_above_but_not_trace() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join("freeports.log");
                let layer = file_layer(&path).expect("file layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::error!("error-marker");
                    tracing::warn!("warn-marker");
                    tracing::info!("info-marker");
                    tracing::debug!("debug-marker");
                    tracing::trace!("trace-marker");
                });
                let content = std::fs::read_to_string(&path).expect("read freeports.log");
                assert!(content.contains("error-marker"));
                assert!(content.contains("warn-marker"));
                assert!(content.contains("info-marker"));
                assert!(content.contains("debug-marker"));
                assert!(
                    !content.contains("trace-marker"),
                    "freeports.log is filtered to `debug`, trace events must not reach it, got:\n{content}"
                );
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
                assert_eq!(CSV_HEADER, "Page,Matched Company,Company,Field name,Row,Column,Message\n");
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

            #[test]
            fn event_with_no_tagged_field_and_no_enclosing_span_produces_no_row() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!("an entirely untagged message");
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                assert_eq!(content, CSV_HEADER, "an untagged event must not add a data row");
            }

            #[test]
            fn event_with_its_own_tagged_field_produces_a_row() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 3u64, "page-scoped message");
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["3", "", "", "", "", "", "page-scoped message"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn event_with_no_own_tags_inside_a_tagged_span_still_produces_a_row() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("page_processing", page = 7u64);
                    span.in_scope(|| {
                        tracing::warn!("no tags on the event itself, page comes from the span");
                    });
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row([
                        "7",
                        "",
                        "",
                        "",
                        "",
                        "",
                        "\"no tags on the event itself, page comes from the span\""
                    ])
                );
                assert_eq!(content, expected);
            }
        }

        mod field_capture {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn captures_all_five_tagged_fields_from_a_single_event() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::warn!(
                        page = 12u64,
                        company = "Acme Corp",
                        field = "NAV",
                        row = 3u64,
                        column = 2u64,
                        "value out of expected range"
                    );
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["12", "", "Acme Corp", "NAV", "3", "2", "value out of expected range"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn matched_company_column_is_always_blank_in_m0() {
                // See the module doc's "domanda aperta" note: no tracing field feeds this
                // column yet. Fixed here as the current expected behavior, not as an
                // endorsement -- flagged to the user in the test-writer's report.
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, company = "Whatever SA", "tagged message");
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let header_and_row: Vec<&str> = content.lines().collect();
                assert_eq!(header_and_row.len(), 2, "expected exactly one data row");
                let cells: Vec<&str> = header_and_row[1].split(',').collect();
                assert_eq!(cells[1], "", "the \"Matched Company\" column must be blank in M0");
            }

            #[test]
            fn event_field_overrides_a_same_named_span_field() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    let span = tracing::info_span!("page_processing", page = 7u64);
                    span.in_scope(|| {
                        tracing::warn!(page = 9u64, "explicit page wins over the span's");
                    });
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["9", "", "", "", "", "", "explicit page wins over the span's"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn innermost_span_wins_over_an_outer_span_for_the_same_field() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    let outer = tracing::info_span!("document_ingest", page = 1u64);
                    outer.in_scope(|| {
                        let inner = tracing::info_span!("page_classification", page = 2u64);
                        inner.in_scope(|| {
                            tracing::info!("nested inside two page-tagged spans");
                        });
                    });
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["2", "", "", "", "", "", "nested inside two page-tagged spans"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn distinct_span_fields_at_different_nesting_levels_all_merge_into_one_row() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    let outer = tracing::info_span!("document_ingest", page = 4u64);
                    outer.in_scope(|| {
                        let inner = tracing::info_span!("field_extraction", field = "ISIN");
                        inner.in_scope(|| {
                            tracing::warn!(row = 6u64, "merged from event and two spans");
                        });
                    });
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}",
                    row(["4", "", "", "ISIN", "6", "", "merged from event and two spans"])
                );
                assert_eq!(content, expected);
            }
        }

        mod csv_escaping {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn message_containing_a_comma_is_quoted() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "value, with a comma inside");
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}1,,,,,,\"value, with a comma inside\"\n"
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn message_containing_a_double_quote_is_escaped_by_doubling() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "say \"hi\" to the user");
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}1,,,,,,\"say \"\"hi\"\" to the user\"\n"
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn message_containing_a_newline_is_quoted_and_the_newline_is_preserved_verbatim() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "first line\nsecond line");
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!("{CSV_HEADER}1,,,,,,\"first line\nsecond line\"\n");
                assert_eq!(content, expected);
            }

            #[test]
            fn a_tagged_field_value_containing_a_comma_is_quoted() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, company = "Acme, Inc.", "ok");
                });
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
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "first");
                    tracing::info!(page = 2u64, "second");
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}{}",
                    row(["1", "", "", "", "", "", "first"]),
                    row(["2", "", "", "", "", "", "second"])
                );
                assert_eq!(content, expected);
            }

            #[test]
            fn an_untagged_event_between_two_tagged_ones_does_not_add_a_row() {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join(".log.csv");
                let layer = CsvLogLayer::create(&path).expect("csv layer construction");
                let subscriber = tracing_subscriber::registry().with(layer);
                tracing::subscriber::with_default(subscriber, || {
                    tracing::info!(page = 1u64, "first");
                    tracing::info!("skipped, no tags and no enclosing span");
                    tracing::info!(page = 2u64, "second");
                });
                let content = std::fs::read_to_string(&path).expect("read .log.csv");
                let expected = format!(
                    "{CSV_HEADER}{}{}",
                    row(["1", "", "", "", "", "", "first"]),
                    row(["2", "", "", "", "", "", "second"])
                );
                assert_eq!(content, expected);
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
            // This is the *only* test in this module allowed to call `init` successfully: a
            // process only ever accepts one `tracing::subscriber::set_global_default` call, ever
            // (not resettable), so this whole scenario -- success, then the resulting
            // `AlreadyInitialized` error's shape, `Display`, and source chain -- is deliberately
            // kept in one sequential test instead of split across several, to avoid racing other
            // tests for who "wins" the one-time global install (`cargo test` runs tests in
            // parallel threads by default).
            let dir = tempfile::tempdir().expect("tempdir");

            let first = init(Verbosity::from_verbose_and_quiet_counts(1, 0), dir.path());
            assert!(first.is_ok(), "first init in this process must succeed, got {first:?}");
            assert!(dir.path().join("freeports.log").exists());
            assert!(dir.path().join(".log.csv").exists());

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
