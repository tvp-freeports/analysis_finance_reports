//! The datum a page leaves to a later one when the link is the **page**, not the name.
//!
//! [`Carryover`](super::promise::Carryover) pairs two pages by the sub-fund's name, because the
//! liabilities page names the sub-fund it speaks of. Some pages offer no such handhold: on UBS-EN23
//! a sub-fund's title is on one page and its statement of net assets one, two, three or four pages
//! later — in between sit the management commentary and the three-year table, which are as long as
//! they happen to be — and the only thing tying the two together is that one comes before the other.
//!
//! # Why the nearest, and not a computed key
//!
//! Where the distance between the two pages is a **constant of the format**, the key can be
//! computed and the engine resolves the promise at the end of the job. Where it varies, as on
//! UBS-EN23, nothing can be computed: the resolution has to look at what the preceding pages
//! actually left, and pick the **closest one before**. That is an interpretation, and `R23` wants
//! those in this segment — which is also why the pipeline that deposits has to sit in an **earlier
//! step** of `algorithms_schedule.csv`, or `data.previous()` carries nothing yet.
//!
//! # The page number is the **printed** one
//!
//! The `pdf_extract` receives the page's dictionary and not its index, so the number is read off
//! the page. That it is the printed one rather than the PDF's is not a defect: the two pipelines
//! only have to compute the **same** key, and the footer is the one thing both of them see.

use std::collections::BTreeMap;

use crate::core::classes::value::BlockValue;
use crate::core::pipeline::{Extracted, FilterData};

/// The value a page leaves to a page that comes after it, keyed by the printed page number.
///
/// Built **once per format**, like [`Carryover`](super::promise::Carryover), so the identifier is
/// written in one place and the two pages cannot spell it differently.
#[derive(Debug, Clone)]
pub struct PageCarryover {
    field: String,
    separator: String,
}

/// What a [`PageCarryover`] refuses at build time.
#[derive(Debug, thiserror::Error)]
pub enum PageCarryoverError {
    #[error("a page carryover with no field would carry nothing")]
    NoField,
    #[error("an empty separator would make the field and the page number unsplittable")]
    EmptySeparator,
}

impl PageCarryover {
    pub fn new(field: String, separator: String) -> Result<Self, PageCarryoverError> {
        if field.is_empty() {
            return Err(PageCarryoverError::NoField);
        }
        if separator.is_empty() {
            return Err(PageCarryoverError::EmptySeparator);
        }
        Ok(Self { field, separator })
    }

    /// The identifier, written here and nowhere else.
    ///
    /// It holds **inside a job**, like every promise: two documents of one run carrying the same
    /// printed page are two different jobs and do not see each other.
    pub fn promise_id(&self, page: i64) -> String {
        format!("{}{}{page}", self.field, self.separator)
    }

    /// `page → value` of what the preceding pages of this job left.
    ///
    /// Keys belonging to another [`PageCarryover`] of the same run are left alone, which is why the
    /// prefix is checked and not only the tail.
    pub fn deposited(&self, data: &FilterData<'_>) -> BTreeMap<i64, BlockValue> {
        let prefix = format!("{}{}", self.field, self.separator);
        let mut out = BTreeMap::new();
        for entries in data.previous().iter().filter_map(Extracted::as_promises) {
            for (key, value) in entries.iter() {
                let Some(tail) = key.strip_prefix(&prefix) else {
                    continue;
                };
                if let Ok(page) = tail.parse::<i64>() {
                    out.insert(page, value.clone());
                }
            }
        }
        out
    }

    /// The value left by the **nearest page before** `page`, or `None`.
    pub fn nearest(&self, data: &FilterData<'_>, page: i64) -> Option<BlockValue> {
        let left = self.deposited(data);
        let Some((_, value)) = left.range(..page).next_back() else {
            tracing::error!(
                "no '{}' was left by any page before {page}: this page cannot say which fund it is \
                 about, and its row is dropped",
                self.field
            );
            return None;
        };
        Some(value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::pipeline::{Extracted, PromiseEntries};
    use pretty_assertions::assert_eq;

    /// The measured shape: UBS-EN23, where a sub-fund's title is on one page and its statement one
    /// to four pages later — p14 → p16 is the case the distance cannot be computed from.
    fn carry() -> PageCarryover {
        PageCarryover::new("ubs-fund".to_string(), "-page-".to_string()).unwrap()
    }

    fn left(pairs: &[(&str, &str)]) -> Vec<Extracted> {
        let entries: PromiseEntries = pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        vec![Extracted::Promises(entries)]
    }

    fn text(value: &BlockValue) -> String {
        value.as_str().unwrap_or_default().to_string()
    }

    #[test]
    fn the_identifier_joins_the_field_and_the_page() {
        assert_eq!(carry().promise_id(14), "ubs-fund-page-14");
    }

    #[test]
    fn a_field_or_a_separator_that_is_empty_is_refused_when_it_is_built() {
        assert!(PageCarryover::new(String::new(), "-page-".to_string()).is_err());
        assert!(PageCarryover::new("f".to_string(), String::new()).is_err());
    }

    #[test]
    fn what_each_page_left_is_read_back_by_its_number() {
        let promises = left(&[
            ("ubs-fund-page-14", "UBS Alpha"),
            ("ubs-fund-page-20", "UBS Beta"),
        ]);
        let got = carry().deposited(&FilterData::Previous(&promises));
        assert_eq!(got.len(), 2);
        assert_eq!(text(&got[&14]), "UBS Alpha");
    }

    /// The prefix is checked, not only the tail: a second carryover of the same run has its own
    /// field, and UBS-EN23 really has two — one for the statements, one for the SFDR annex.
    #[test]
    fn another_carryover_of_the_same_run_is_left_alone() {
        let promises = left(&[("ubs-esg-fund-page-14", "Something else")]);
        assert!(
            carry()
                .deposited(&FilterData::Previous(&promises))
                .is_empty()
        );
    }

    #[test]
    fn a_tail_that_is_not_a_number_is_not_a_page() {
        let promises = left(&[("ubs-fund-page-fourteen", "Nonsense")]);
        assert!(
            carry()
                .deposited(&FilterData::Previous(&promises))
                .is_empty()
        );
    }

    /// The nearest **before**, which is the whole point: between the title and the statement sit
    /// the management commentary and the three-year table, and they are as long as they happen to be.
    #[test]
    fn the_nearest_page_before_is_the_one_that_answers() {
        let promises = left(&[
            ("ubs-fund-page-10", "UBS Alpha"),
            ("ubs-fund-page-14", "UBS Beta"),
            ("ubs-fund-page-20", "UBS Gamma"),
        ]);
        let got = carry().nearest(&FilterData::Previous(&promises), 16);
        assert_eq!(got.as_ref().map(text), Some("UBS Beta".to_string()));
    }

    /// A page that left something on the page being read is **not** before it. The statement is on
    /// its own page, and taking that page's own deposit would pair a title with itself.
    #[test]
    fn the_page_itself_does_not_count_as_before_it() {
        let promises = left(&[
            ("ubs-fund-page-10", "UBS Alpha"),
            ("ubs-fund-page-16", "UBS Beta"),
        ]);
        let got = carry().nearest(&FilterData::Previous(&promises), 16);
        assert_eq!(got.as_ref().map(text), Some("UBS Alpha".to_string()));
    }

    #[test]
    fn nothing_before_it_is_an_error_and_no_value() {
        let promises = left(&[("ubs-fund-page-20", "UBS Gamma")]);
        assert!(
            carry()
                .nearest(&FilterData::Previous(&promises), 16)
                .is_none()
        );
    }
}
