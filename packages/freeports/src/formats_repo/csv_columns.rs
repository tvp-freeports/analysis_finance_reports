//! The columns a formats-repository CSV may carry, and the check that it carries no other.
//!
//! A column the engine does not know is not harmless: its cells are read by nobody, so a repository
//! written for an older engine — one that still has `Note marks`, or `Merge previous` — loses that
//! configuration **in silence**, and the first sign of it is a row missing from the output. Every
//! table of the repository is therefore checked against the columns its row type declares, and an
//! unknown one is refused when the repository loads.
//!
//! The check is on the **header**, not on the rows: serde's `deny_unknown_fields` would only fire on
//! a file that has at least one row, and a header-only table with a stale column would pass.

use serde::de::{self, DeserializeOwned, Deserializer, Visitor};

/// The column names a row type declares, as serde sees them — after `rename`.
///
/// Read by asking the type to deserialize itself from a probe that records the field list serde
/// hands to `deserialize_struct` and then stops. Nothing is built.
pub fn declared<T: DeserializeOwned>() -> &'static [&'static str] {
    let mut fields: &'static [&'static str] = &[];
    // The probe always answers with an error once it has the fields: that is how it stops.
    let _ = T::deserialize(Probe(&mut fields));
    fields
}

/// The first header of `headers` that is not among `known`, if any.
pub fn first_unknown<'h>(
    headers: impl IntoIterator<Item = &'h str>,
    known: &[&str],
) -> Option<&'h str> {
    headers.into_iter().find(|header| !known.contains(header))
}

/// The message a reader puts in its own error type: which column, and which ones it could have been.
pub fn unknown_column_message(column: &str, known: &[&str]) -> String {
    format!(
        "unknown column '{column}': the columns this table takes are {}",
        known.join(", ")
    )
}

#[derive(Debug)]
struct Stop;

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("field list read")
    }
}

impl std::error::Error for Stop {}

impl de::Error for Stop {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Stop
    }
}

struct Probe<'a>(&'a mut &'static [&'static str]);

impl<'de> Deserializer<'de> for Probe<'_> {
    type Error = Stop;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Stop> {
        Err(Stop)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Stop> {
        *self.0 = fields;
        Err(Stop)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option
        unit unit_struct newtype_struct seq tuple tuple_struct map enum identifier ignored_any
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct Row {
        #[serde(rename = "ID")]
        id: String,
        #[serde(rename = "Remove from fund", default)]
        cut: Option<String>,
    }

    #[test]
    fn the_declared_columns_are_the_renamed_ones() {
        assert_eq!(declared::<Row>(), ["ID", "Remove from fund"]);
    }

    #[test]
    fn an_unknown_header_is_named() {
        assert_eq!(
            first_unknown(["ID", "Note marks"], declared::<Row>()),
            Some("Note marks")
        );
    }

    #[test]
    fn known_headers_in_any_order_and_subset_pass() {
        assert_eq!(
            first_unknown(["Remove from fund", "ID"], declared::<Row>()),
            None
        );
        assert_eq!(first_unknown(["ID"], declared::<Row>()), None);
    }

    #[test]
    fn the_message_lists_the_known_columns() {
        let message = unknown_column_message("Note marks", declared::<Row>());
        assert!(
            message.contains("Note marks") && message.contains("ID, Remove from fund"),
            "{message}"
        );
    }
}
