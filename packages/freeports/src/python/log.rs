//! The bridge that lets a format's algorithm write into the engine's log.
//!
//! # Why it exists
//!
//! A format's algorithm is the place that *knows* — that a value was imposed instead of read, that
//! a residual was assigned to a manager the report never named, that something on the page was
//! seen and could not be modelled. Until this module existed, none of that could be said: the
//! `.log.csv` is built from `tracing` inside the crate, and a `logging.warning` written in an
//! author's module reached nothing at all.
//!
//! # An author's event is an engine event
//!
//! Not a separate channel. It goes through the same three destinations, with the same thresholds,
//! and it inherits the same context — which is the whole reason nothing has to be passed:
//!
//! | destination | threshold |
//! |---|---|
//! | `.log.csv` | `warn` and `error`, and nothing else |
//! | stderr | the run's `Verbosity`, `warn` by default |
//! | `freeports.log.jsonl` | `Verbosity::Trace` only — below it the file does not exist |
//!
//! So a `debug` written in an algorithm goes nowhere in an ordinary run, appears on stderr when
//! someone raises the verbosity, and lands in the structured log when someone is diagnosing in
//! earnest. That is deliberate: a log holding the crate's events but not the algorithms' would
//! tell half the story.
//!
//! # Report, page and activity are never passed
//!
//! The call into an author's pipe happens *inside* the engine's spans and *on the same thread*
//! (`formats_repo::unstructured::py_pipe` calls `Python::attach` inline), and `tracing` spans are
//! thread-local. An event emitted from inside a Python pipe therefore already carries its report,
//! its page and the activity that produced it. The only thing a caller may add is a position, and
//! that is what the four coordinate arguments are.
//!
//! # Two properties this module leans on
//!
//! 1. **`Option<T>` is a `Value`, and a `None` field is never visited** (`tracing-core`). That is
//!    what makes one call site per level enough: an omitted coordinate is *absent*, not *empty*,
//!    so it leaves its CSV cell blank and never, by itself, creates a row.
//! 2. **A `.log.csv` row exists only where one of the five tagged fields is present** — `page`,
//!    `coord_ref_1`, `coord_ref_2`, `coord_1`, `coord_2`. Inside a page the `page` span supplies
//!    the first, so an author's `warn` makes a row with or without coordinates; outside one it
//!    does not, which is the rule that keeps that file a record of the extraction rather than a
//!    transcript of the program.
//!
//! # The trap not to reintroduce
//!
//! The engine's spans are opened with `info_span!`, and a plain level filter switches off span
//! *creation* as well as events — which is why the crate filters with `EventLevelFilter` instead.
//! Nothing here opens a span, so nothing here is exposed to it. A future helper tempted to open
//! one just to carry its coordinates would bring the problem back, and would take the `Report` and
//! `Page` columns down with it at the default level.

use pyo3::prelude::*;

/// The `tracing` target of every event emitted through this module.
///
/// It replaces the Rust module path, which would say `freeports::python::log` for an event whose
/// real author is a format's algorithm. It appears only in `freeports.log.jsonl` — stderr does not
/// print targets and the `.log.csv` has no column for one — and it is there so that a tool reading
/// that file can separate what an algorithm said from what the engine said, with a filter rather
/// than a guess.
const FORMAT_TARGET: &str = "format";

/// The four optional anchors a caller may attach to an event.
///
/// Grouped into one type so the five level functions differ in exactly one token. `None` means the
/// column stays empty: see the module documentation for why that is absence and not emptiness.
struct Coords<'a> {
    coord_ref_1: Option<&'a str>,
    coord_ref_2: Option<&'a str>,
    coord_1: Option<&'a str>,
    coord_2: Option<&'a str>,
}

/// Emits one event at a fixed level with the four coordinates and the caller's text.
///
/// A macro rather than a function because `tracing` requires the level to be constant at the call
/// site: there is no `event!(runtime_level, …)`. Five expansions, one per level, and the dynamic
/// part is the text.
macro_rules! emit {
    ($level:ident, $message:expr, $coords:expr) => {{
        let Coords { coord_ref_1, coord_ref_2, coord_1, coord_2 } = $coords;
        tracing::$level!(
            target: FORMAT_TARGET,
            coord_ref_1 = coord_ref_1,
            coord_ref_2 = coord_ref_2,
            coord_1 = coord_1,
            coord_2 = coord_2,
            "{}",
            $message
        );
    }};
}

/// Builds the five `#[pyfunction]`s, which are identical but for the level they emit at.
macro_rules! level_function {
    ($name:ident, $py_name:literal, $level:ident, $doc:literal) => {
        #[doc = $doc]
        ///
        /// The four coordinates are keyword-only and optional; report, page and activity come from
        /// the engine's spans and must not be passed.
        #[pyfunction]
        #[pyo3(name = $py_name, signature = (message, *, coord_ref_1=None, coord_ref_2=None, coord_1=None, coord_2=None))]
        pub fn $name(
            message: &str,
            coord_ref_1: Option<&str>,
            coord_ref_2: Option<&str>,
            coord_1: Option<&str>,
            coord_2: Option<&str>,
        ) {
            emit!($level, message, Coords { coord_ref_1, coord_ref_2, coord_1, coord_2 });
        }
    };
}

level_function!(
    py_error,
    "error",
    error,
    "Something the extraction got wrong and did not silently absorb — an accounting equation that \
     does not balance, a row that had to be dropped. Reaches `.log.csv`.\n\nIt does **not** fail \
     the pipe: raising an exception is how a pipe fails, and keeping the two apart is what lets an \
     algorithm report a bad row without losing the page."
);

level_function!(
    py_warn,
    "warn",
    warn,
    "Something a reader of the data must know: a value imposed rather than read, an assignment \
     deduced from an ambiguity, something seen on the page and not modelled. Reaches `.log.csv`."
);

level_function!(
    py_info,
    "info",
    info,
    "The course of the extraction, for someone watching the run. Never reaches `.log.csv`."
);

level_function!(
    py_debug,
    "debug",
    debug,
    "What the author of the format needs while writing it. Visible with `-vv`, and in \
     `freeports.log.jsonl` at `-vvv`."
);

level_function!(
    py_trace,
    "trace",
    trace,
    "The finest detail, for a page that has stopped adding up. Visible only at `-vvv`, where it \
     also lands in `freeports.log.jsonl`."
);

/// A value the engine decided instead of reading, said in one fixed wording.
///
/// The free-text functions above are the point of this module, and this one is the counterweight:
/// a recurring warning written by hand each time becomes ten phrasings of one fact, and then
/// nobody can find it in the `.log.csv`. This is the only such helper that exists, because it is
/// the only wording the rules have already settled. The next one is added when a phrasing has
/// actually repeated itself, not in anticipation of it.
///
/// `field` fills `Second coord ref`, the column that says which field a row is about.
#[pyfunction]
#[pyo3(name = "warn_imposed", signature = (*, field, value, reason))]
pub fn py_warn_imposed(field: &str, value: &Bound<'_, PyAny>, reason: &str) -> PyResult<()> {
    let value = value.str()?.extract::<String>()?;
    emit!(
        warn,
        format_args!("{field} was not read: imposed as {value:?} - {reason}"),
        Coords { coord_ref_1: None, coord_ref_2: Some(field), coord_1: None, coord_2: None }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::{Level, Subscriber};
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
    use tracing_subscriber::Registry;

    use crate::core::tracing_setup::{csv_event_filter, CsvLogLayer};

    /// One captured event: its level, its message, and every field it actually recorded.
    ///
    /// The field map is the point of most of these tests — a coordinate left out must be *missing*
    /// from it, not present and empty.
    #[derive(Debug, Clone, Default)]
    struct Record {
        level: Option<Level>,
        target: String,
        message: String,
        fields: std::collections::HashMap<String, String>,
    }

    impl Visit for Record {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            let rendered = format!("{value:?}");
            if field.name() == "message" {
                self.message = rendered;
            } else {
                self.fields.insert(field.name().to_string(), rendered);
            }
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            if field.name() == "message" {
                self.message = value.to_string();
            } else {
                self.fields.insert(field.name().to_string(), value.to_string());
            }
        }
    }

    #[derive(Clone, Default)]
    struct CapturingLayer {
        records: Arc<Mutex<Vec<Record>>>,
    }

    impl<S: Subscriber> Layer<S> for CapturingLayer {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut record = Record {
                level: Some(*event.metadata().level()),
                target: event.metadata().target().to_string(),
                ..Record::default()
            };
            event.record(&mut record);
            self.records.lock().expect("the capture mutex is not poisoned").push(record);
        }
    }

    /// The events emitted by `f`, captured with no level filter at all.
    fn captured(f: impl FnOnce()) -> Vec<Record> {
        let layer = CapturingLayer::default();
        let subscriber = Registry::default().with(layer.clone());
        tracing::subscriber::with_default(subscriber, f);
        layer.records.lock().expect("the capture mutex is not poisoned").clone()
    }

    /// The single event emitted by `f`.
    fn one(f: impl FnOnce()) -> Record {
        let mut records = captured(f);
        assert_eq!(records.len(), 1, "exactly one event was expected, found {}", records.len());
        records.pop().expect("one record")
    }

    mod levels {
        use super::*;
        use pretty_assertions::assert_eq;
        use test_case::test_case;

        #[test_case(Level::ERROR; "error")]
        #[test_case(Level::WARN; "warn")]
        #[test_case(Level::INFO; "info")]
        #[test_case(Level::DEBUG; "debug")]
        #[test_case(Level::TRACE; "trace")]
        fn each_function_emits_at_its_own_level(level: Level) {
            let record = one(|| match level {
                Level::ERROR => py_error("said", None, None, None, None),
                Level::WARN => py_warn("said", None, None, None, None),
                Level::INFO => py_info("said", None, None, None, None),
                Level::DEBUG => py_debug("said", None, None, None, None),
                _ => py_trace("said", None, None, None, None),
            });
            assert_eq!(record.level, Some(level));
        }

        #[test]
        fn the_message_is_the_caller_s_text_verbatim() {
            // Free text is the point of this module: braces and percent signs are characters, not
            // formatting directives, and an algorithm that reports what it read must be able to
            // quote it.
            let text = "the report writes {2,5%} where a number belongs";
            let record = one(|| py_warn(text, None, None, None, None));
            assert_eq!(record.message, text);
        }

        #[test]
        fn every_level_carries_the_format_target() {
            for record in captured(|| {
                py_error("a", None, None, None, None);
                py_warn("b", None, None, None, None);
                py_info("c", None, None, None, None);
                py_debug("d", None, None, None, None);
                py_trace("e", None, None, None, None);
            }) {
                assert_eq!(record.target, FORMAT_TARGET);
            }
        }
    }

    mod coordinates {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_four_given_coordinates_are_recorded() {
            let record = one(|| py_warn("said", Some("anchor"), Some("field"), Some("row 3"), Some("col 5")));
            assert_eq!(record.fields.get("coord_ref_1").map(String::as_str), Some("anchor"));
            assert_eq!(record.fields.get("coord_ref_2").map(String::as_str), Some("field"));
            assert_eq!(record.fields.get("coord_1").map(String::as_str), Some("row 3"));
            assert_eq!(record.fields.get("coord_2").map(String::as_str), Some("col 5"));
        }

        /// The property the whole design rests on: a `None` field is never visited, so it is
        /// missing rather than empty. Were it recorded as `""`, every call would carry one of the
        /// five tagged fields and would write a `.log.csv` row from anywhere in the program.
        #[test]
        fn an_omitted_coordinate_is_absent_not_empty() {
            let record = one(|| py_warn("said", None, None, None, None));
            assert_eq!(record.fields, std::collections::HashMap::new());
        }

        #[test]
        fn coordinates_are_independent_of_each_other() {
            let record = one(|| py_warn("said", None, Some("market_value"), None, None));
            assert_eq!(record.fields.len(), 1);
            assert_eq!(record.fields.get("coord_ref_2").map(String::as_str), Some("market_value"));
        }
    }

    mod imposed_values {
        use super::*;
        use pretty_assertions::assert_eq;

        fn imposed(value: impl for<'py> FnOnce(Python<'py>) -> Bound<'py, PyAny>) -> Record {
            Python::attach(|py| {
                let value = value(py);
                one(|| {
                    py_warn_imposed("management_company", &value, "constant of the format")
                        .expect("a printable value");
                })
            })
        }

        #[test]
        fn it_warns_with_the_one_settled_wording() {
            let record = imposed(|py| pyo3::types::PyString::new(py, "Amundi SGR S.p.A.").into_any());
            assert_eq!(record.level, Some(Level::WARN));
            assert_eq!(
                record.message,
                "management_company was not read: imposed as \"Amundi SGR S.p.A.\" - constant of the format"
            );
        }

        #[test]
        fn the_field_fills_the_column_that_says_which_field() {
            let record = imposed(|py| pyo3::types::PyString::new(py, "EUR").into_any());
            assert_eq!(record.fields.get("coord_ref_2").map(String::as_str), Some("management_company"));
        }

        /// An imposed value is not always a string — a currency, a number, an enum of the engine's
        /// own. It is printed the way Python prints it rather than rejected.
        #[test]
        fn a_value_that_is_not_a_string_is_printed_as_python_prints_it() {
            let record = imposed(|py| 42i64.into_pyobject(py).expect("an int").into_any());
            assert!(record.message.contains("imposed as \"42\""), "found {:?}", record.message);
        }
    }

    /// The half that cannot be checked by capturing events: what the engine's own layers make of
    /// them. Report, page and activity are never passed by the caller, and these tests are what
    /// says so.
    mod through_the_real_csv_layer {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The `.log.csv` written while `f` runs, as its data rows.
        fn csv_rows(f: impl FnOnce()) -> Vec<Vec<String>> {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join(".log.csv");
            let csv = CsvLogLayer::create(&path).expect("csv layer construction");
            let subscriber = Registry::default().with(csv.clone().with_filter(csv_event_filter()));
            tracing::subscriber::with_default(subscriber, f);
            csv.close().expect("the csv layer closes");
            let content = std::fs::read_to_string(&path).expect("read .log.csv");
            content
                .lines()
                .skip(1)
                .map(|line| {
                    csv::ReaderBuilder::new()
                        .has_headers(false)
                        .from_reader(line.as_bytes())
                        .records()
                        .next()
                        .expect("one record per line")
                        .expect("a well-formed record")
                        .iter()
                        .map(str::to_string)
                        .collect()
                })
                .collect()
        }

        /// Report, page and activity as the engine supplies them around an author's pipe.
        fn inside_the_engine_spans(f: impl FnOnce()) {
            let document = tracing::info_span!("document", report = "EURIZON 2023");
            let _document = document.enter();
            let page = tracing::info_span!("page", page = 54u64);
            let _page = page.enter();
            let pipeline = tracing::info_span!("pipeline", pipeline = "investments");
            let _pipeline = pipeline.enter();
            f();
        }

        #[test]
        fn an_event_inherits_report_page_and_activity_without_being_given_them() {
            let rows = csv_rows(|| {
                inside_the_engine_spans(|| py_warn("the residual went to the manco", None, None, None, None))
            });
            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(row[0], "EURIZON 2023", "Report");
            assert_eq!(row[1], "54", "Page");
            assert_eq!(row[2], "document[EURIZON 2023]/page[54]/pipeline[investments]", "Activity");
            assert_eq!(row[7], "WARN", "Level");
            assert_eq!(row[8], "the residual went to the manco", "Message");
        }

        #[test]
        fn the_coordinate_columns_of_an_event_without_coordinates_stay_empty() {
            let rows = csv_rows(|| inside_the_engine_spans(|| py_warn("said", None, None, None, None)));
            assert_eq!(&rows[0][3..7], ["", "", "", ""]);
        }

        #[test]
        fn the_four_coordinates_fill_their_own_columns() {
            let rows = csv_rows(|| {
                inside_the_engine_spans(|| {
                    py_error("the accounting equation does not balance", Some("Total"), Some("net_assets"), Some("row 12"), Some("col 3"))
                })
            });
            assert_eq!(&rows[0][3..7], ["Total", "net_assets", "row 12", "col 3"]);
            assert_eq!(rows[0][7], "ERROR");
        }

        /// The threshold is the file's, not this module's: three of the five levels are meant for
        /// whoever is watching or diagnosing the run, and must not reach a product of it.
        #[test]
        fn only_warn_and_error_reach_the_file() {
            let rows = csv_rows(|| {
                inside_the_engine_spans(|| {
                    py_error("kept", None, None, None, None);
                    py_warn("kept", None, None, None, None);
                    py_info("dropped", None, None, None, None);
                    py_debug("dropped", None, None, None, None);
                    py_trace("dropped", None, None, None, None);
                })
            });
            assert_eq!(rows.len(), 2);
            assert_eq!(rows.iter().map(|r| r[7].as_str()).collect::<Vec<_>>(), ["ERROR", "WARN"]);
        }

        /// Outside a page, with no coordinate of its own, an event has nothing to anchor it: it
        /// says something happened without saying where, and the audit trail is better off without
        /// it. It is still on stderr and in the structured log.
        #[test]
        fn an_event_with_neither_page_nor_coordinate_writes_no_row() {
            let rows = csv_rows(|| py_warn("nowhere in particular", None, None, None, None));
            assert_eq!(rows.len(), 0);
        }

        #[test]
        fn a_coordinate_alone_is_enough_to_write_a_row() {
            let rows = csv_rows(|| py_warn("outside any page", Some("the table header"), None, None, None));
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0][3], "the table header");
        }
    }
}
