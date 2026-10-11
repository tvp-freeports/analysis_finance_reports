//! `U7` — a statement written as labels, read even when it sits on two pages.
//!
//! Eight formats of the collection write their statement of net assets as **labels with values**
//! rather than as a table: `TOTALE ATTIVITA'` on one line with its number beside it, and the same
//! for the liabilities and the net assets. Six of them put the assets on one page and the
//! liabilities with the net assets on the **next**, and
//! [`FundAssets`](crate::output::classes::fund_assets::FundAssets) carries the three amounts
//! together and checks them together (`R32`), so neither page alone can build the row.
//!
//! This module is the one pipe that reads that shape, and it is four independent things:
//!
//! - [`record`] gathers the bare blocks several `A1` pipes leave on **one** page into one block
//!   with the metadata the standard `text_filter`s expect. It serves the formats with no second
//!   page too;
//! - [`promise`] carries a value **from the first page to the second**, depositing it as a promise
//!   and finding it again in `data.previous()`, pairing the two pages **by the sub-fund's name**;
//! - [`page`] does the same where the sub-fund's name is not the handhold and the two pages are
//!   paired by **printed page number**;
//! - [`document`] answers *which sub-fund is this page about* when the page does not say and the
//!   document carries one only.
//!
//! # Why `A1` and not `A4`
//!
//! §5.3 of the plan foresaw `A4` on both pages. Measured, it does not hold, and the reason is
//! structural: `A4` builds a row out of **three** numeric columns and drops the row that is missing
//! one. A page declaring `TOTALE ATTIVITA'` alone is not an assets table missing something — it is
//! **a label with its value**, which is exactly the shape `A1` reads. The two pages therefore have
//! two, three or four `A1` pipes each, and [`record`] puts them back together.
//!
//! # What stays with the format author
//!
//! The `pdf_extract` does. `A1` is not a semistructured name yet, so a promoted pipeline keeps its
//! author's extraction and takes only `text_filter` and `deserialize` from here — which the three
//! levels allow, because they **sum** for one pipeline name. That is the same division
//! `inv_managers` settled into.

pub mod document;
pub mod page;
pub mod promise;
pub mod record;

use std::collections::BTreeMap;

use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock, TextBlock};
use crate::core::pipeline::{FilterData, PipeError, TextFilterPipe};

use super::standard_funcs::{StandardFuncsError, TextFilterAssetsStandard};
use page::PageCarryover;
use promise::Carryover;
use record::Record;

/// Where the sub-fund's name comes from, when the page does not simply write it.
#[derive(Debug, Clone, Default)]
pub enum FundFrom {
    /// The page names its own sub-fund, which is the ordinary case.
    #[default]
    Page,
    /// Ask the document, but **only** when the page carries no name of its own.
    ///
    /// `C23`'s correction, and the order matters: asked before the page, `sole_fund` would report a
    /// sub-fund as missing that had in fact been read. Two formats need it, and in both the
    /// condition is the same — the page's own fund block is absent or empty.
    SoleIfAbsent,
    /// The nearest page before this one left it. See [`page`].
    NearestPage {
        carry: PageCarryover,
        /// The type of the block whose content is this page's printed number.
        page_block_type: BlockType,
    },
}

/// The `text_filter` of a statement written as labels: gather, filter, and resume what crossed.
///
/// The three steps are one pipe and not three, because a segment's pipes are **concatenated** and
/// not chained: each one receives the same PDF blocks and their outputs are added together, so a
/// chain cannot be expressed by three rows of configuration. The Python call sites wrote the chain
/// by hand inside one function, and this is that function.
pub struct TextFilterLabelledAssets {
    record: Record,
    fund: FundFrom,
    /// The metadata key the sub-fund's name lives under — `fund` everywhere measured, a parameter
    /// because [`record`]'s keys are the format's own.
    fund_key: String,
    assets: TextFilterAssetsStandard,
    /// The promise to resume, on the **second** of two pages. `None` on a one-page statement and on
    /// the first page of two, which deposits instead of resuming.
    resume: Option<Carryover>,
}

impl TextFilterLabelledAssets {
    pub fn new(
        record: Record,
        fund: FundFrom,
        fund_key: String,
        assets: TextFilterAssetsStandard,
        resume: Option<Carryover>,
    ) -> Self {
        Self {
            record,
            fund,
            fund_key,
            assets,
            resume,
        }
    }

    /// Whether the page writes its own sub-fund's name, in a block that is there and not empty.
    ///
    /// The emptiness matters: a block of the right type carrying nothing is a page whose footer was
    /// not read, and treating it as an answer costs the row. It is EURIZON-IT24's reading of `C23`,
    /// taken as the rule.
    fn page_names_its_fund(&self, blocks: &[PdfBlock]) -> bool {
        let Some(block_type) = self.record.fields.get(&self.fund_key) else {
            return false;
        };
        blocks.iter().any(|blk| {
            &blk.type_block == block_type && !record::as_text(&blk.content).trim().is_empty()
        })
    }

    /// What the page does not write and the job already knows, ready for [`Record::call`].
    ///
    /// An empty map is the ordinary case. `None` means *this page cannot be attributed to anybody*,
    /// and then no block comes out at all — the diagnosis has already been said by whoever could
    /// not answer.
    fn also(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Option<BTreeMap<String, BlockValue>> {
        let mut also = BTreeMap::new();
        match &self.fund {
            FundFrom::Page => {}
            FundFrom::SoleIfAbsent => {
                if !self.page_names_its_fund(blocks) {
                    // A document that cannot answer is not a page that yields nothing: the Python
                    // template returned no fallback and let the gathering go on without a fund, so
                    // the row fell at the filter on the known funds. Same outcome, said once.
                    if let Some(name) = document::sole_fund(data) {
                        also.insert(self.fund_key.clone(), BlockValue::from(name.as_str()));
                    }
                }
            }
            FundFrom::NearestPage {
                carry,
                page_block_type,
            } => {
                let page = blocks.iter().find(|blk| &blk.type_block == page_block_type);
                let number =
                    page.and_then(|blk| record::as_text(&blk.content).trim().parse::<i64>().ok());
                let Some(number) = number else {
                    tracing::debug!(
                        "this page carries no printed number: nothing can be carried over to it"
                    );
                    return None;
                };
                also.insert(self.fund_key.clone(), carry.nearest(data, number)?);
            }
        }
        Some(also)
    }

    pub fn call(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let Some(also) = self.also(blocks, data) else {
            return Ok(Vec::new());
        };
        let gathered = self.record.call(blocks, &also);
        let filtered = self.assets.call(&gathered, data)?;
        Ok(match &self.resume {
            Some(carry) => carry.resume(filtered, data),
            None => filtered,
        })
    }
}

impl TextFilterPipe for TextFilterLabelledAssets {
    fn name(&self) -> &str {
        "TextFilterLabelledAssets"
    }

    fn filter(
        &self,
        blocks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        self.call(blocks, data)
            .map_err(|e| e.into_pipe_error(TextFilterPipe::name(self)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::pipeline::{Extracted, PromiseEntries};
    use crate::output::classes::fund::Fund;

    /// The sub-fund the measured pages belong to: ARCA-IT24 D1 p8 → p9.
    ///
    /// Those two pages are where every number in this module's tests comes from, read with
    /// `inspect-page --mode lines --order y`: `TOTALE ATTIVITA' 21.970.695` on the first,
    /// `TOTALE PASSIVITA' 274.045` and `VALORE COMPLESSIVO NETTO DEL FONDO 21.696.650` on the
    /// second — and the equation adds up, which is the whole reason the two pages have to speak to
    /// each other.
    const FUND: &str = "ARCA OPPORTUNITÀ GREEN 2027 VIII";
    const TOT_ASSETS: &str = "21.970.695";
    const LIABILITIES: &str = "274.045";
    const NET_ASSETS: &str = "21.696.650";

    /// The bare blocks the `A1` pipes of one page leave: a type and a text.
    fn bare(pairs: &[(&str, &str)]) -> Vec<PdfBlock> {
        pairs
            .iter()
            .map(|(type_block, text)| {
                PdfBlock::new(
                    BlockType::from(type_block.to_string()),
                    BTreeMap::new(),
                    BlockValue::from(*text),
                )
            })
            .collect()
    }

    /// The statement block as the standard assets `text_filter` hands it on.
    fn statement(pairs: &[(&str, &str)]) -> TextBlock {
        let metadata = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), BlockValue::from(*v)))
            .collect();
        TextBlock::from_content(BlockType::RELEVANT_BLOCK, metadata, "")
    }

    fn promises(pairs: &[(&str, &str)]) -> Vec<Extracted> {
        let entries: PromiseEntries = pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        vec![Extracted::Promises(entries)]
    }

    fn carry(fields: &[&str]) -> promise::Carryover {
        promise::Carryover::new(
            fields.iter().map(|f| f.to_string()).collect(),
            "fund".to_string(),
            "@".to_string(),
        )
        .expect("a carryover these tests spell right")
    }

    fn record(pairs: &[(&str, &str)]) -> Record {
        Record::new(
            BlockType::RELEVANT_BLOCK,
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), BlockType::from(v.to_string())))
                .collect(),
        )
        .expect("a record these tests spell right")
    }

    /// The events a piece of work emits, kept by the text of their message.
    mod capture {
        use std::sync::{Arc, Mutex};
        use tracing::field::{Field, Visit};
        use tracing_subscriber::Registry;
        use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

        #[derive(Default, Clone, Debug)]
        pub(super) struct Event {
            pub(super) level: String,
            pub(super) message: String,
        }

        impl Visit for Event {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.message = format!("{value:?}");
                }
            }
        }

        #[derive(Clone, Default)]
        struct Capturing(Arc<Mutex<Vec<Event>>>);

        impl<S: tracing::Subscriber> Layer<S> for Capturing {
            fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
                let mut kept = Event {
                    level: event.metadata().level().to_string(),
                    ..Event::default()
                };
                event.record(&mut kept);
                self.0.lock().unwrap().push(kept);
            }
        }

        pub(super) fn events<T>(f: impl FnOnce() -> T) -> (T, Vec<Event>) {
            let layer = Capturing::default();
            let subscriber = Registry::default().with(layer.clone());
            let out = tracing::subscriber::with_default(subscriber, f);
            let kept = layer.0.lock().unwrap().clone();
            (out, kept)
        }

        pub(super) fn said(events: &[Event], level: &str, needle: &str) -> bool {
            events
                .iter()
                .any(|e| e.level == level && e.message.contains(needle))
        }
    }

    mod the_record {
        use super::*;
        use pretty_assertions::assert_eq;

        fn metadata_of(blocks: &[PdfBlock]) -> Vec<(String, String)> {
            blocks[0]
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), record::as_text(v)))
                .collect()
        }

        #[test]
        fn each_block_type_fills_its_own_key() {
            let gathered = record(&[("fund", "FUND_NAME"), ("tot_assets", "TOT_ASSETS")]).call(
                &bare(&[("FUND_NAME", FUND), ("TOT_ASSETS", TOT_ASSETS)]),
                &BTreeMap::new(),
            );

            assert_eq!(gathered.len(), 1);
            assert_eq!(
                metadata_of(&gathered),
                vec![
                    ("fund".to_string(), FUND.to_string()),
                    ("tot_assets".to_string(), TOT_ASSETS.to_string())
                ]
            );
        }

        #[test]
        fn the_block_that_comes_out_is_of_the_declared_type() {
            let declared = Record::new(
                BlockType::from("STATEMENT".to_string()),
                BTreeMap::from([("fund".to_string(), BlockType::FUND_NAME)]),
            )
            .unwrap();
            let gathered = declared.call(&bare(&[("FUND_NAME", FUND)]), &BTreeMap::new());
            assert_eq!(
                gathered[0].type_block,
                BlockType::from("STATEMENT".to_string())
            );
        }

        #[test]
        fn by_default_the_block_is_the_one_the_standard_filters_expect() {
            let gathered = record(&[("fund", "FUND_NAME")])
                .call(&bare(&[("FUND_NAME", FUND)]), &BTreeMap::new());
            assert_eq!(gathered[0].type_block, BlockType::RELEVANT_BLOCK);
        }

        #[test]
        fn blocks_of_no_declared_type_are_left_where_they_are() {
            let gathered = record(&[("fund", "FUND_NAME")]).call(
                &bare(&[("FUND_NAME", FUND), ("PAGE_CLASS", "fund_assets_begin")]),
                &BTreeMap::new(),
            );
            assert_eq!(
                metadata_of(&gathered),
                vec![("fund".to_string(), FUND.to_string())]
            );
        }

        #[test]
        fn a_missing_block_type_says_so_and_leaves_the_key_out() {
            let (gathered, events) = capture::events(|| {
                record(&[("fund", "FUND_NAME"), ("currency", "CURRENCY_STATEMENT")])
                    .call(&bare(&[("FUND_NAME", FUND)]), &BTreeMap::new())
            });
            assert_eq!(
                metadata_of(&gathered),
                vec![("fund".to_string(), FUND.to_string())]
            );
            assert!(capture::said(&events, "WARN", "currency"), "{events:?}");
        }

        #[test]
        fn two_blocks_of_one_type_take_the_first_and_say_so() {
            let (gathered, events) = capture::events(|| {
                record(&[("net_assets", "NET_ASSETS")]).call(
                    &bare(&[("NET_ASSETS", NET_ASSETS), ("NET_ASSETS", "1.702.020")]),
                    &BTreeMap::new(),
                )
            });
            assert_eq!(
                metadata_of(&gathered),
                vec![("net_assets".to_string(), NET_ASSETS.to_string())]
            );
            assert!(
                capture::said(&events, "WARN", "one statement"),
                "{events:?}"
            );
        }

        #[test]
        fn a_page_with_nothing_expected_on_it_yields_no_statement() {
            let (gathered, events) = capture::events(|| {
                record(&[("fund", "FUND_NAME")])
                    .call(&bare(&[("SOMETHING_ELSE", "x")]), &BTreeMap::new())
            });
            assert!(gathered.is_empty());
            assert!(
                capture::said(&events, "WARN", "no statement is read"),
                "{events:?}"
            );
        }

        #[test]
        fn a_record_with_no_field_is_refused_when_it_is_built() {
            assert!(Record::new(BlockType::RELEVANT_BLOCK, BTreeMap::new()).is_err());
        }
    }

    mod the_deposit {
        use super::*;
        use pretty_assertions::assert_eq;

        fn deposited(carry: &promise::Carryover, block: &TextBlock) -> Vec<(String, String)> {
            carry
                .deposit(&block.metadata)
                .iter()
                .map(|(k, v)| (k.to_string(), record::as_text(v)))
                .collect()
        }

        #[test]
        fn the_identifier_joins_the_field_and_the_fund() {
            assert_eq!(
                carry(&["tot_assets"]).promise_id("tot_assets", FUND),
                format!("tot_assets@{FUND}")
            );
        }

        #[test]
        fn the_separator_is_a_parameter() {
            let other = promise::Carryover::new(
                vec!["tot_assets".to_string()],
                "fund".to_string(),
                "::".to_string(),
            )
            .unwrap();
            assert_eq!(
                other.promise_id("tot_assets", FUND),
                format!("tot_assets::{FUND}")
            );
        }

        #[test]
        fn one_entry_per_carried_field() {
            let block = statement(&[
                ("fund", FUND),
                ("tot_assets", TOT_ASSETS),
                ("currency", "EUR"),
                ("extra", "ignored"),
            ]);
            assert_eq!(
                deposited(&carry(&["tot_assets", "currency"]), &block),
                vec![
                    (format!("tot_assets@{FUND}"), TOT_ASSETS.to_string()),
                    (format!("currency@{FUND}"), "EUR".to_string()),
                ]
            );
        }

        #[test]
        fn a_field_the_page_does_not_carry_is_left_out_with_a_warning() {
            let block = statement(&[("fund", FUND), ("tot_assets", TOT_ASSETS)]);
            let (entries, events) =
                capture::events(|| deposited(&carry(&["tot_assets", "liabilities"]), &block));
            assert_eq!(
                entries,
                vec![(format!("tot_assets@{FUND}"), TOT_ASSETS.to_string())]
            );
            assert!(capture::said(&events, "WARN", "liabilities"), "{events:?}");
        }

        #[test]
        fn without_a_fund_there_is_nobody_to_promise_to() {
            let block = statement(&[("tot_assets", TOT_ASSETS)]);
            let (entries, events) = capture::events(|| deposited(&carry(&["tot_assets"]), &block));
            assert!(entries.is_empty());
            assert!(
                capture::said(&events, "ERROR", "cannot leave anything"),
                "{events:?}"
            );
        }

        #[test]
        fn the_fund_key_is_a_parameter() {
            let other = promise::Carryover::new(
                vec!["tot_assets".to_string()],
                "comparto".to_string(),
                "@".to_string(),
            )
            .unwrap();
            let block = statement(&[("comparto", FUND), ("tot_assets", TOT_ASSETS)]);
            assert_eq!(
                deposited(&other, &block),
                vec![(format!("tot_assets@{FUND}"), TOT_ASSETS.to_string())]
            );
        }

        #[test]
        fn a_carryover_with_no_field_or_no_separator_is_refused_when_it_is_built() {
            assert!(promise::Carryover::new(vec![], "fund".to_string(), "@".to_string()).is_err());
            assert!(
                promise::Carryover::new(
                    vec!["tot_assets".to_string()],
                    "fund".to_string(),
                    String::new()
                )
                .is_err()
            );
        }
    }

    mod what_was_promised {
        use super::*;
        use crate::core::match_fund::MatchFund;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_promise_is_found_by_the_fund_it_names() {
            let left = promises(&[(&format!("tot_assets@{FUND}"), TOT_ASSETS)]);
            let available = carry(&["tot_assets"]).promised(&FilterData::Previous(&left));
            assert_eq!(available.len(), 1);
            assert_eq!(
                record::as_text(&available[&MatchFund::new(FUND)]["tot_assets"]),
                TOT_ASSETS
            );
        }

        /// The comparison is by fund identity, not by string: `MatchFund` normalises deeply.
        #[test]
        fn the_two_pages_may_write_the_name_differently() {
            let left = promises(&[("tot_assets@ARCA  opportunità green 2027 VIII", "1")]);
            let available = carry(&["tot_assets"]).promised(&FilterData::Previous(&left));
            assert!(
                available.contains_key(&MatchFund::new(FUND)),
                "{available:?}"
            );
        }

        /// Split by known prefix and not on the separator, because a sub-fund's name may contain it.
        #[test]
        fn a_fund_name_containing_the_separator_survives() {
            let left = promises(&[("tot_assets@Alpha @ Beta", "1")]);
            let available = carry(&["tot_assets"]).promised(&FilterData::Previous(&left));
            assert!(
                available.contains_key(&MatchFund::new("Alpha @ Beta")),
                "{available:?}"
            );
        }

        #[test]
        fn promises_of_another_datum_are_left_alone() {
            let left = promises(&[("esg-fund-page-3", "Whatever")]);
            assert!(
                carry(&["tot_assets"])
                    .promised(&FilterData::Previous(&left))
                    .is_empty()
            );
        }

        #[test]
        fn results_that_are_not_promises_are_skipped() {
            let left = vec![Extracted::Fund(Fund::new("Alpha"))];
            assert!(
                carry(&["tot_assets"])
                    .promised(&FilterData::Previous(&left))
                    .is_empty()
            );
        }

        #[test]
        fn several_fields_of_one_fund_gather_under_one_key() {
            let left = promises(&[
                (&format!("tot_assets@{FUND}"), TOT_ASSETS),
                (&format!("liabilities@{FUND}"), LIABILITIES),
            ]);
            let available =
                carry(&["tot_assets", "liabilities"]).promised(&FilterData::Previous(&left));
            assert_eq!(available[&MatchFund::new(FUND)].len(), 2);
        }
    }

    mod resuming {
        use super::*;
        use pretty_assertions::assert_eq;

        fn resumed(
            fields: &[&str],
            blocks: Vec<TextBlock>,
            left: &[(&str, &str)],
        ) -> Vec<TextBlock> {
            let left = promises(left);
            carry(fields).resume(blocks, &FilterData::Previous(&left))
        }

        fn field(block: &TextBlock, key: &str) -> String {
            block
                .metadata
                .get(key)
                .map(record::as_text)
                .unwrap_or_default()
        }

        #[test]
        fn the_carried_value_joins_the_metadata_of_its_own_fund() {
            let got = resumed(
                &["tot_assets"],
                vec![statement(&[
                    ("fund", FUND),
                    ("liabilities", LIABILITIES),
                    ("net_assets", NET_ASSETS),
                ])],
                &[(&format!("tot_assets@{FUND}"), TOT_ASSETS)],
            );
            assert_eq!(got.len(), 1);
            assert_eq!(field(&got[0], "tot_assets"), TOT_ASSETS);
            assert_eq!(field(&got[0], "liabilities"), LIABILITIES);
            assert_eq!(field(&got[0], "net_assets"), NET_ASSETS);
        }

        /// Not a test of the code: the measurement that says the two pages are one row.
        #[test]
        fn the_equation_of_the_two_pages_adds_up() {
            let number = |text: &str| text.replace('.', "").parse::<i64>().unwrap();
            assert_eq!(number(TOT_ASSETS) - number(LIABILITIES), number(NET_ASSETS));
        }

        #[test]
        fn the_type_and_the_content_of_the_block_are_kept() {
            let block = TextBlock::from_content(
                BlockType::from("STATEMENT".to_string()),
                BTreeMap::from([("fund".to_string(), BlockValue::from(FUND))]),
                "some content",
            );
            let got = resumed(
                &["tot_assets"],
                vec![block],
                &[(&format!("tot_assets@{FUND}"), "1")],
            );
            assert_eq!(got[0].type_block, BlockType::from("STATEMENT".to_string()));
            assert_eq!(got[0].content.as_str(), Some("some content"));
        }

        #[test]
        fn each_fund_gets_its_own_and_nobody_elses() {
            let got = resumed(
                &["tot_assets"],
                vec![
                    statement(&[("fund", "Alpha")]),
                    statement(&[("fund", "Beta")]),
                ],
                &[("tot_assets@Alpha", "10"), ("tot_assets@Beta", "20")],
            );
            assert_eq!(
                got.iter()
                    .map(|b| field(b, "tot_assets"))
                    .collect::<Vec<_>>(),
                vec!["10".to_string(), "20".to_string()]
            );
        }

        #[test]
        fn a_statement_whose_other_page_never_arrived_is_dropped_loudly() {
            let (got, events) = capture::events(|| {
                resumed(&["tot_assets"], vec![statement(&[("fund", FUND)])], &[])
            });
            assert!(got.is_empty());
            assert!(capture::said(&events, "ERROR", FUND), "{events:?}");
        }

        /// One field out of two does not build the row: `FundAssets` wants all three together.
        #[test]
        fn a_half_kept_promise_is_as_bad_as_none() {
            let (got, events) = capture::events(|| {
                resumed(
                    &["tot_assets", "liabilities"],
                    vec![statement(&[("fund", FUND)])],
                    &[(&format!("tot_assets@{FUND}"), "1")],
                )
            });
            assert!(got.is_empty());
            assert!(capture::said(&events, "ERROR", "liabilities"), "{events:?}");
        }

        #[test]
        fn a_block_with_no_fund_cannot_be_paired() {
            let (got, events) = capture::events(|| {
                resumed(
                    &["tot_assets"],
                    vec![statement(&[("liabilities", LIABILITIES)])],
                    &[(&format!("tot_assets@{FUND}"), "1")],
                )
            });
            assert!(got.is_empty());
            assert!(
                capture::said(&events, "ERROR", "nothing was carried over"),
                "{events:?}"
            );
        }

        #[test]
        fn what_was_already_in_the_metadata_is_not_lost() {
            let got = resumed(
                &["tot_assets"],
                vec![statement(&[
                    ("fund", FUND),
                    ("currency", "EUR"),
                    ("date", "30/12/2024"),
                ])],
                &[(&format!("tot_assets@{FUND}"), TOT_ASSETS)],
            );
            assert_eq!(field(&got[0], "currency"), "EUR");
            assert_eq!(field(&got[0], "date"), "30/12/2024");
        }
    }

    /// The pipe itself: the three steps one after the other, and the three answers to *which
    /// sub-fund is this page about*.
    mod the_pipe {
        use super::*;
        use pretty_assertions::assert_eq;

        fn pipe(
            fund: FundFrom,
            fields: &[(&str, &str)],
            resume: Option<&[&str]>,
        ) -> TextFilterLabelledAssets {
            TextFilterLabelledAssets::new(
                record(fields),
                fund,
                "fund".to_string(),
                crate::formats_utils::text_filter::standard_funcs::TextFilterAssetsStandard::new(
                    None,
                    vec![],
                )
                .unwrap(),
                resume.map(carry),
            )
        }

        fn declared(names: &[&str]) -> Vec<Extracted> {
            names
                .iter()
                .map(|name| Extracted::Fund(Fund::new(name)))
                .collect()
        }

        fn field(block: &TextBlock, key: &str) -> String {
            block
                .metadata
                .get(key)
                .map(record::as_text)
                .unwrap_or_default()
        }

        const FIELDS: [(&str, &str); 3] = [
            ("fund", "FUND_NAME"),
            ("currency", "CURRENCY_STATEMENT"),
            ("tot_assets", "TOT_ASSETS"),
        ];

        /// The ordinary case, which is six of the nine measured pipelines: the page names its own
        /// sub-fund, and the row comes out with the three amounts in its metadata.
        #[test]
        fn a_page_that_names_its_fund_needs_nothing_else() {
            let known = declared(&[FUND]);
            let got = pipe(FundFrom::Page, &FIELDS, None)
                .call(
                    &bare(&[
                        ("FUND_NAME", FUND),
                        ("CURRENCY_STATEMENT", "EUR"),
                        ("TOT_ASSETS", TOT_ASSETS),
                    ]),
                    &FilterData::Previous(&known),
                )
                .unwrap();

            assert_eq!(got.len(), 1);
            assert_eq!(field(&got[0], "tot_assets"), TOT_ASSETS);
        }

        /// A sub-fund nobody asked about leaves no row, which is why `resume` has to run **after**
        /// this filter: here the absence is ordinary, not a defect.
        #[test]
        fn a_fund_of_no_interest_leaves_no_row() {
            let known = declared(&["Some other fund"]);
            let got = pipe(FundFrom::Page, &FIELDS, None)
                .call(
                    &bare(&[
                        ("FUND_NAME", FUND),
                        ("CURRENCY_STATEMENT", "EUR"),
                        ("TOT_ASSETS", TOT_ASSETS),
                    ]),
                    &FilterData::Previous(&known),
                )
                .unwrap();
            assert!(got.is_empty());
        }

        /// `sole_if_absent` is **only** asked when the page is silent: asked first, it would report
        /// a sub-fund as missing that had in fact been read. That is `C23`'s correction.
        #[test]
        fn sole_if_absent_is_not_asked_when_the_page_names_its_fund() {
            let known = declared(&[FUND]);
            let (got, events) = capture::events(|| {
                pipe(FundFrom::SoleIfAbsent, &FIELDS, None)
                    .call(
                        &bare(&[
                            ("FUND_NAME", FUND),
                            ("CURRENCY_STATEMENT", "EUR"),
                            ("TOT_ASSETS", TOT_ASSETS),
                        ]),
                        &FilterData::Previous(&known),
                    )
                    .unwrap()
            });
            assert_eq!(got.len(), 1);
            assert!(
                !capture::said(&events, "WARN", "declares one fund only"),
                "{events:?}"
            );
        }

        #[test]
        fn sole_if_absent_answers_for_the_page_that_is_silent_and_declares_it_imposed() {
            let known = declared(&[FUND]);
            let (got, events) = capture::events(|| {
                pipe(FundFrom::SoleIfAbsent, &FIELDS, None)
                    .call(
                        &bare(&[("CURRENCY_STATEMENT", "EUR"), ("TOT_ASSETS", TOT_ASSETS)]),
                        &FilterData::Previous(&known),
                    )
                    .unwrap()
            });
            assert_eq!(got.len(), 1);
            assert_eq!(field(&got[0], "fund"), Fund::new(FUND).name().unwrap());
            assert!(
                capture::said(&events, "WARN", "declares one fund only"),
                "{events:?}"
            );
        }

        /// An empty block of the right type: the fallback **is** asked, and then the page's own
        /// empty value overwrites what it answered, so the row still falls.
        ///
        /// This is the Python template's behaviour, kept on purpose: `Record` reads the first block
        /// of a declared type whenever one exists, and `also` is a fallback under it, never over it.
        /// It is arguably a hole — a footer that was not read loses the row quietly, which is what
        /// `R12` is about — but no measured page has an empty fund block, so changing it here would
        /// be a behaviour change with no document behind it. Written down so that the day one
        /// appears, this test says where to look: it is `Record::call`'s `if !taken.is_empty()`, and
        /// the fix is to treat empty content as absent.
        #[test]
        fn an_empty_fund_block_still_wins_over_the_fallback() {
            let known = declared(&[FUND]);
            let (got, events) = capture::events(|| {
                pipe(FundFrom::SoleIfAbsent, &FIELDS, None)
                    .call(
                        &bare(&[
                            ("FUND_NAME", "  "),
                            ("CURRENCY_STATEMENT", "EUR"),
                            ("TOT_ASSETS", TOT_ASSETS),
                        ]),
                        &FilterData::Previous(&known),
                    )
                    .unwrap()
            });
            assert!(got.is_empty(), "la pagina vince col suo blocco vuoto");
            // The fallback did run, which is the half of the behaviour that is right: the page was
            // read as silent, and the document answered.
            assert!(
                capture::said(&events, "WARN", "declares one fund only"),
                "{events:?}"
            );
        }

        /// UBS-EN23's shape: the statement does not name its sub-fund at all, and the title is on
        /// the nearest page before it that left one.
        #[test]
        fn nearest_page_takes_the_fund_from_the_title_a_few_pages_back() {
            let carry_page =
                page::PageCarryover::new("ubs-fund".to_string(), "-page-".to_string()).unwrap();
            let entries: crate::core::pipeline::PromiseEntries =
                [("ubs-fund-page-14".to_string(), FUND)]
                    .into_iter()
                    .collect();
            let known: Vec<Extracted> = vec![
                Extracted::Promises(entries),
                Extracted::Fund(Fund::new(FUND)),
            ];

            let fields = [
                ("currency", "CURRENCY_STATEMENT"),
                ("tot_assets", "TOT_ASSETS"),
                ("page", "ASSETS_PAGE"),
            ];
            let got = pipe(
                FundFrom::NearestPage {
                    carry: carry_page,
                    page_block_type: BlockType::from("ASSETS_PAGE".to_string()),
                },
                &fields,
                None,
            )
            .call(
                &bare(&[
                    ("ASSETS_PAGE", "16"),
                    ("CURRENCY_STATEMENT", "EUR"),
                    ("TOT_ASSETS", TOT_ASSETS),
                ]),
                &FilterData::Previous(&known),
            )
            .unwrap();

            assert_eq!(got.len(), 1);
            assert_eq!(field(&got[0], "fund"), FUND);
        }

        /// No printed number is no key, and a row tied to the wrong sub-fund is worse than a row
        /// that is missing.
        #[test]
        fn nearest_page_without_a_printed_number_yields_nothing() {
            let carry_page =
                page::PageCarryover::new("ubs-fund".to_string(), "-page-".to_string()).unwrap();
            let got = pipe(
                FundFrom::NearestPage {
                    carry: carry_page,
                    page_block_type: BlockType::from("ASSETS_PAGE".to_string()),
                },
                &[
                    ("currency", "CURRENCY_STATEMENT"),
                    ("tot_assets", "TOT_ASSETS"),
                ],
                None,
            )
            .call(&bare(&[("CURRENCY_STATEMENT", "EUR")]), &FilterData::EMPTY)
            .unwrap();
            assert!(got.is_empty());
        }

        /// The whole chain of the second page: gather, filter, resume — and the order is what makes
        /// the missing promise a defect rather than an ordinary absence.
        #[test]
        fn the_second_page_gathers_filters_and_then_resumes() {
            let promised: crate::core::pipeline::PromiseEntries =
                [(format!("tot_assets@{FUND}"), TOT_ASSETS)]
                    .into_iter()
                    .collect();
            let known: Vec<Extracted> = vec![
                Extracted::Promises(promised),
                Extracted::Fund(Fund::new(FUND)),
            ];

            let fields = [
                ("fund", "FUND_NAME"),
                ("currency", "CURRENCY_STATEMENT"),
                ("liabilities", "LIABILITIES"),
                ("net_assets", "NET_ASSETS"),
            ];
            let got = pipe(FundFrom::Page, &fields, Some(&["tot_assets"]))
                .call(
                    &bare(&[
                        ("FUND_NAME", FUND),
                        ("CURRENCY_STATEMENT", "EUR"),
                        ("LIABILITIES", LIABILITIES),
                        ("NET_ASSETS", NET_ASSETS),
                    ]),
                    &FilterData::Previous(&known),
                )
                .unwrap();

            assert_eq!(got.len(), 1);
            assert_eq!(field(&got[0], "tot_assets"), TOT_ASSETS);
            assert_eq!(field(&got[0], "liabilities"), LIABILITIES);
            assert_eq!(field(&got[0], "net_assets"), NET_ASSETS);
        }
    }

    mod the_two_pages_together {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The two halves one after the other, as the two pipelines put them.
        #[test]
        fn what_the_first_page_deposits_is_what_the_second_finds() {
            let carried = carry(&["tot_assets"]);

            let first = record(&[("fund", "FUND_NAME"), ("tot_assets", "TOT_ASSETS")]).call(
                &bare(&[("FUND_NAME", FUND), ("TOT_ASSETS", TOT_ASSETS)]),
                &BTreeMap::new(),
            );
            let deposited = carried.deposit(&first[0].metadata);

            let second = record(&[
                ("fund", "FUND_NAME"),
                ("liabilities", "LIABILITIES"),
                ("net_assets", "NET_ASSETS"),
            ])
            .call(
                &bare(&[
                    ("FUND_NAME", FUND),
                    ("LIABILITIES", LIABILITIES),
                    ("NET_ASSETS", NET_ASSETS),
                ]),
                &BTreeMap::new(),
            );
            let as_text =
                TextBlock::from_content(BlockType::RELEVANT_BLOCK, second[0].metadata.clone(), "");

            let left = vec![Extracted::Promises(deposited)];
            let got = carried.resume(vec![as_text], &FilterData::Previous(&left));

            assert_eq!(got.len(), 1);
            assert_eq!(record::as_text(&got[0].metadata["tot_assets"]), TOT_ASSETS);
            assert_eq!(
                record::as_text(&got[0].metadata["liabilities"]),
                LIABILITIES
            );
            assert_eq!(record::as_text(&got[0].metadata["net_assets"]), NET_ASSETS);
        }
    }

    mod what_the_other_page_already_said {
        use super::*;
        use pretty_assertions::assert_eq;

        fn funds(names: &[&str]) -> Vec<Extracted> {
            names
                .iter()
                .map(|name| Extracted::Fund(Fund::new(name)))
                .collect()
        }

        /// The name comes back as `Fund` writes it — normalised — which is how `funds.csv` writes it.
        #[test]
        fn the_only_fund_of_the_report_is_the_one() {
            let declared = funds(&[FUND]);
            let (got, _) =
                capture::events(|| document::sole_fund(&FilterData::Previous(&declared)));
            assert_eq!(got, Fund::new(FUND).name());
        }

        #[test]
        fn attributing_it_is_an_imposed_value_and_says_so() {
            let declared = funds(&[FUND]);
            let (_, events) =
                capture::events(|| document::sole_fund(&FilterData::Previous(&declared)));
            assert!(
                capture::said(&events, "WARN", "declares one fund only"),
                "{events:?}"
            );
        }

        #[test]
        fn two_funds_are_not_a_choice_to_make() {
            let declared = funds(&["Alpha", "Beta"]);
            let (got, events) =
                capture::events(|| document::sole_fund(&FilterData::Previous(&declared)));
            assert_eq!(got, None);
            assert!(
                capture::said(&events, "ERROR", "would be a guess"),
                "{events:?}"
            );
        }

        #[test]
        fn no_fund_at_all_is_an_error_too() {
            let (got, events) = capture::events(|| document::sole_fund(&FilterData::Previous(&[])));
            assert_eq!(got, None);
            assert!(
                capture::said(&events, "ERROR", "nobody to attribute"),
                "{events:?}"
            );
        }

        #[test]
        fn also_fills_what_the_page_does_not_declare() {
            let also = BTreeMap::from([("fund".to_string(), BlockValue::from(FUND))]);
            let gathered = record(&[("tot_assets", "TOT_ASSETS")])
                .call(&bare(&[("TOT_ASSETS", "31.363.698")]), &also);
            assert_eq!(record::as_text(&gathered[0].metadata["fund"]), FUND);
            assert_eq!(
                record::as_text(&gathered[0].metadata["tot_assets"]),
                "31.363.698"
            );
        }

        /// `also` is a fallback, never an override: what the page declares always wins.
        #[test]
        fn what_the_page_declares_wins_over_what_it_is_told() {
            let also = BTreeMap::from([("fund".to_string(), BlockValue::from("Un altro fondo"))]);
            let gathered =
                record(&[("fund", "FUND_NAME")]).call(&bare(&[("FUND_NAME", FUND)]), &also);
            assert_eq!(record::as_text(&gathered[0].metadata["fund"]), FUND);
        }

        #[test]
        fn a_field_that_also_supplies_is_not_reported_as_missing() {
            let also = BTreeMap::from([("fund".to_string(), BlockValue::from(FUND))]);
            let (gathered, events) = capture::events(|| {
                record(&[("fund", "FUND_NAME"), ("tot_assets", "TOT_ASSETS")])
                    .call(&bare(&[("TOT_ASSETS", "1")]), &also)
            });
            assert_eq!(record::as_text(&gathered[0].metadata["fund"]), FUND);
            assert!(
                !capture::said(&events, "WARN", "no 'FUND_NAME' block"),
                "{events:?}"
            );
        }
    }
}
