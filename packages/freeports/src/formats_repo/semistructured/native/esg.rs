//! The YAML surface of `A6`: the ESG indicators of the European periodic template.
//!
//! Two names come from here — `esg` in the `text_filter` and `esg_indicator` in the `deserialize`
//! — and the second takes no arguments at all, because building the entity is the whole of its job.
//!
//! The `text_filter` has exactly two cells: **which reader**, and **whose page this is**. Every
//! other parameter of a reader has the value the measured pages share, so the four promoted
//! pipelines write between one and five lines each.

use serde::Deserialize;

use crate::formats_utils::deserialize::esg::DeserializerEsgIndicator;
use crate::formats_utils::text_filter::esg::{EsgError, FundOf, MARKER, NUMBER, Reader, TextFilterEsg};

/// What an ESG configuration can be refused for.
#[derive(Debug, thiserror::Error)]
pub enum EsgConfigError {
    #[error(transparent)]
    Reader(#[from] EsgError),
}

fn number() -> String {
    NUMBER.to_string()
}

fn marker() -> Option<String> {
    Some(MARKER.to_string())
}

fn group_key() -> String {
    "group".to_string()
}

fn page_key() -> String {
    "page".to_string()
}

fn page_separator() -> String {
    "-page-".to_string()
}

fn one() -> i64 {
    1
}

fn colon() -> String {
    ":".to_string()
}

/// Which of the three shapes the page writes its indicators in.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
// The three shapes differ in size by design: each carries the parameters its own reading needs, and
// boxing the largest would cost the configuration its flat, readable shape for bytes nobody counts.
#[allow(clippy::large_enum_variant)]
pub enum InputReader {
    /// The sentence begins with the number. FINECO-EN23@IR and ANIMA_SGR-IT24.
    LeadingNumber {
        /// Absent keeps the standard `1.` / `2.`; `null` says the page writes no marker and the
        /// entries separate by group alone.
        #[serde(default = "marker")]
        marker: Option<String>,
        #[serde(default = "number")]
        number: String,
        #[serde(default = "group_key")]
        group_key: String,
        #[serde(default)]
        max_name: usize,
    },
    /// The indicator is a table. AMUNDI-EN24 and EURIZON-IT24.
    DatedRows {
        #[serde(default = "indicator_key")]
        indicator_key: String,
        #[serde(default = "series_key")]
        series_key: String,
        #[serde(default = "column_key")]
        column_key: String,
        #[serde(default)]
        series: i64,
        #[serde(default)]
        column: i64,
        #[serde(default = "number")]
        number: String,
    },
    /// `label: value`, one entry per line. UBS-EN23.
    LabelledPairs {
        #[serde(default = "colon")]
        separator: String,
        #[serde(default)]
        drop: Vec<String>,
        #[serde(default)]
        strip: Vec<String>,
        #[serde(default = "number")]
        number: String,
        #[serde(default = "group_key")]
        group_key: String,
    },
}

fn indicator_key() -> String {
    "indicator".to_string()
}

fn series_key() -> String {
    "series".to_string()
}

fn column_key() -> String {
    "column".to_string()
}

/// Whose page this is. Absent means nobody, which is a page answering in prose.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputFundOf {
    /// One promise for the whole document: one document, one sub-fund.
    Promise { id: String },
    /// The promise the page before left, keyed by the printed page number.
    PagePromise {
        field: String,
        #[serde(default = "page_key")]
        page_key: String,
        #[serde(default = "one")]
        offset: i64,
        #[serde(default = "page_separator")]
        separator: String,
    },
}

/// The `text_filter` of an ESG indicators pipeline.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputEsg {
    pub read: InputReader,
    pub fund: Option<InputFundOf>,
}

/// Builds the `esg` pipe.
pub fn text_filter(input: &InputEsg) -> Result<TextFilterEsg, EsgConfigError> {
    let reader = match &input.read {
        InputReader::LeadingNumber { marker, number, group_key, max_name } => {
            Reader::leading_number(marker.as_deref(), number, group_key.clone(), *max_name)?
        }
        InputReader::DatedRows { indicator_key, series_key, column_key, series, column, number } => {
            Reader::dated_rows(
                indicator_key.clone(),
                series_key.clone(),
                column_key.clone(),
                *series,
                *column,
                number,
            )?
        }
        InputReader::LabelledPairs { separator, drop, strip, number, group_key } => {
            Reader::labelled_pairs(separator.clone(), drop, strip, number, group_key.clone())?
        }
    };
    let fund = match &input.fund {
        None => FundOf::Nobody,
        Some(InputFundOf::Promise { id }) => FundOf::Promised(id.clone()),
        Some(InputFundOf::PagePromise { field, page_key, offset, separator }) => FundOf::PagePromise {
            field: field.clone(),
            page_key: page_key.clone(),
            offset: *offset,
            separator: separator.clone(),
        },
    };
    Ok(TextFilterEsg::new(reader, fund))
}

/// The argless deserializer of the same algorithm.
pub fn deserialize() -> DeserializerEsgIndicator {
    DeserializerEsgIndicator
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(yaml: &str) -> InputEsg {
        serde_yaml::from_str(yaml).expect("a configuration these tests spell right")
    }

    mod the_reader {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The shortest configuration that works, and it is two of the five promoted pipelines:
        /// name the reader and nothing else.
        #[test]
        fn naming_the_reader_is_the_whole_of_a_minimal_configuration() {
            let input = parsed("read:\n  kind: leading_number\n");
            assert!(input.fund.is_none());
            assert!(text_filter(&input).is_ok());
        }

        #[test]
        fn the_marker_defaults_to_the_numbered_list_and_null_says_the_page_writes_none() {
            match parsed("read:\n  kind: leading_number\n").read {
                InputReader::LeadingNumber { marker, .. } => assert_eq!(marker.as_deref(), Some(MARKER)),
                other => panic!("{other:?}"),
            }
            match parsed("read:\n  kind: leading_number\n  marker: null\n").read {
                InputReader::LeadingNumber { marker, .. } => assert_eq!(marker, None),
                other => panic!("{other:?}"),
            }
        }

        /// `R5` and the benchmark row, as defaults: series zero is the product's and column zero is
        /// the most recent year, so a table that wants both says nothing.
        #[test]
        fn dated_rows_defaults_to_the_product_and_the_year_in_course() {
            match parsed("read:\n  kind: dated_rows\n").read {
                InputReader::DatedRows { series, column, indicator_key, .. } => {
                    assert_eq!((series, column), (0, 0));
                    assert_eq!(indicator_key, "indicator");
                }
                other => panic!("{other:?}"),
            }
        }

        #[test]
        fn labelled_pairs_takes_the_two_lists_that_tell_the_product_from_its_benchmark() {
            let input = parsed(
                "read:\n  kind: labelled_pairs\n  drop:\n    - 'of the benchmark'\n\
                 \x20 strip:\n    - '^o\\s+'\n",
            );
            match &input.read {
                InputReader::LabelledPairs { separator, drop, strip, .. } => {
                    assert_eq!(separator, ":");
                    assert_eq!(drop.len(), 1);
                    assert_eq!(strip.len(), 1);
                }
                other => panic!("{other:?}"),
            }
            assert!(text_filter(&input).is_ok());
        }

        #[test]
        fn a_reader_nobody_knows_is_refused_when_the_repository_loads() {
            let refused: Result<InputEsg, _> = serde_yaml::from_str("read:\n  kind: whatever\n");
            assert!(refused.is_err());
        }

        #[test]
        fn a_pattern_that_does_not_compile_is_refused_by_name() {
            let input = parsed("read:\n  kind: labelled_pairs\n  drop:\n    - '('\n");
            assert!(matches!(text_filter(&input), Err(EsgConfigError::Reader(_))));
        }
    }

    mod whose_page_this_is {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn absent_means_nobody_which_is_a_page_answering_in_prose() {
            assert!(parsed("read:\n  kind: leading_number\n").fund.is_none());
        }

        #[test]
        fn one_promise_for_the_whole_document() {
            let input = parsed("read:\n  kind: dated_rows\nfund:\n  kind: promise\n  id: esg-indicator-fund\n");
            match input.fund.as_ref().unwrap() {
                InputFundOf::Promise { id } => assert_eq!(id, "esg-indicator-fund"),
                other => panic!("{other:?}"),
            }
            assert!(text_filter(&input).is_ok());
        }

        /// The three defaults that make the common case one line: the metadata key the page number
        /// sits under, a distance of one page, and the separator the deposit uses.
        #[test]
        fn a_page_promise_needs_only_its_field() {
            let input = parsed("read:\n  kind: leading_number\nfund:\n  kind: page_promise\n  field: esg-fund\n");
            match input.fund.as_ref().unwrap() {
                InputFundOf::PagePromise { field, page_key, offset, separator } => {
                    assert_eq!(field, "esg-fund");
                    assert_eq!(page_key, "page");
                    assert_eq!(*offset, 1);
                    assert_eq!(separator, "-page-");
                }
                other => panic!("{other:?}"),
            }
            assert!(text_filter(&input).is_ok());
        }
    }
}
