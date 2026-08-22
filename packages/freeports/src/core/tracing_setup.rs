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
//! ## `Verbosity`
//!
//! ```text
//! pub enum Verbosity { Warn, Info, Debug, Trace }
//! impl Verbosity {
//!     pub fn from_flag_count(count: u8) -> Verbosity;
//!     pub fn level(self) -> tracing::Level;
//! }
//! ```
//!
//! Mappatura `from_flag_count` (numero di `-v` sulla riga di comando, 0 se assente):
//! `0 -> Warn`, `1 -> Info`, `2 -> Debug`, `3` e oltre (saturante) `-> Trace`. Non è arbitraria:
//! deriva dalla formula esistente in `freeports_core`
//! (`reference_legacy/_internals/cli/main.py`, `LOG_LEVEL = (5 - VERBOSITY) * 10` su livelli
//! `logging` standard) con `VERBOSITY` di default `2` (`conf_parse.py`, `DEFAULT_CONFIG`), che dà
//! `WARNING` di base e un livello `logging` in meno per ogni `-v`: `WARNING, INFO, DEBUG,
//! NOTSET`. `NOTSET` (mostra tutto) non ha equivalente diretto in `tracing::Level` (il più fine
//! è `TRACE`), da cui la saturazione a `Trace` per 3 o più `-v` invece di un quinto livello.
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
    Warn,
    Info,
    Debug,
    Trace,
}

impl Verbosity {
    pub fn from_flag_count(count: u8) -> Verbosity {
        match count {
            0 => Verbosity::Warn,
            1 => Verbosity::Info,
            2 => Verbosity::Debug,
            _ => Verbosity::Trace,
        }
    }

    pub fn level(self) -> tracing::Level {
        match self {
            Verbosity::Warn => tracing::Level::WARN,
            Verbosity::Info => tracing::Level::INFO,
            Verbosity::Debug => tracing::Level::DEBUG,
            Verbosity::Trace => tracing::Level::TRACE,
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

pub fn stderr_layer<S>(verbosity: Verbosity) -> impl Layer<S> + std::fmt::Debug
where
    S: Subscriber + for<'span> LookupSpan<'span> + std::fmt::Debug,
{
    tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr as fn() -> std::io::Stderr)
        .with_filter(LevelFilter::from_level(verbosity.level()))
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

    mod verbosity {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case(0, tracing::Level::WARN; "no -v flags: warn")]
        #[test_case(1, tracing::Level::INFO; "single -v: info")]
        #[test_case(2, tracing::Level::DEBUG; "double -v: debug")]
        #[test_case(3, tracing::Level::TRACE; "triple -v: trace")]
        fn maps_flag_count_to_the_expected_level(count: u8, expected: tracing::Level) {
            assert_eq!(Verbosity::from_flag_count(count).level(), expected);
        }

        #[test_case(4; "one flag above the highest defined tier")]
        #[test_case(10; "arbitrary high count")]
        #[test_case(u8::MAX; "the u8 upper bound")]
        fn saturates_at_trace_beyond_three_flags(count: u8) {
            assert_eq!(Verbosity::from_flag_count(count).level(), tracing::Level::TRACE);
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

        #[test_case(0; "warn tier")]
        #[test_case(1; "info tier")]
        #[test_case(2; "debug tier")]
        #[test_case(3; "trace tier")]
        #[test_case(255; "saturated tier")]
        fn builds_and_runs_without_panicking(flag_count: u8) {
            let verbosity = Verbosity::from_flag_count(flag_count);
            let subscriber = tracing_subscriber::registry().with(stderr_layer(verbosity));
            // The point of this test is the absence of a panic while the layer is exercised at
            // every level, at every verbosity tier -- stderr output itself is not asserted on
            // (see the module doc: only "does not panic" is in scope for this destination).
            tracing::subscriber::with_default(subscriber, emit_one_event_per_level);
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

            let first = init(Verbosity::from_flag_count(1), dir.path());
            assert!(first.is_ok(), "first init in this process must succeed, got {first:?}");
            assert!(dir.path().join("freeports.log").exists());
            assert!(dir.path().join(".log.csv").exists());

            let second = init(Verbosity::from_flag_count(1), dir.path());
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
            let result = init(Verbosity::from_flag_count(0), &missing);
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
