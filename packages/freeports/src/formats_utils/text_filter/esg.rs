//! `A6` — the ESG indicators of the European periodic template, read once for every format.
//!
//! The European periodic template asks *"How did the sustainability indicators perform?"* and the
//! reports answer in three different ways: a **table** of pairs, a **sentence beginning with the
//! number**, or **prose only**. The third is not a gap: `R11` says that where there is no number
//! there is no indicator, and extracting nothing is the right reading — ANIMA_SGR-IT24 p1726
//! answers with four lines of discourse and no measurements.
//!
//! From that comes the rule holding the three cases together: **an indicator with no number does
//! not exist.** The readers below silently drop everything without one, which is why the prose-only
//! page needs no branch of its own and reads with the same configuration as FINECO-EN23@IR p894.
//!
//! | how the page writes the indicator | reader | measured on |
//! |---|---|---|
//! | a numbered sentence that **begins with the value** | [`Reader::LeadingNumber`] | FINECO-EN23@IR D1 p894 |
//! | **prose only**, no number | the same, finding nothing | ANIMA_SGR-IT24 D1 p1726 |
//! | a **table**: the name above, one row per series, one column per year | [`Reader::DatedRows`] | AMUNDI-EN24 p778 |
//! | **`label: value`**, one entry per line | [`Reader::LabelledPairs`] | UBS-EN23 p140 |
//!
//! Two things recur in all three, and they are choices rather than details:
//!
//! - **the previous year is not extracted** (`R5`). Three measured pages out of five repeat last
//!   year's figures, and writing them would mean two indistinguishable rows for the same indicator
//!   of the same sub-fund. Where the repetition is a section it is closed earlier; where it is a
//!   column, the column of the most recent date is the one kept;
//! - **the benchmark's row is not a datum of the sub-fund.** `DatedRows` leaves it out with
//!   `series`, `LabelledPairs` with `drop`.
//!
//! # Whose page is this
//!
//! The question this pipe does **not** answer by itself, because the answer changes per format: on
//! KAIROS-EN23 the name is on the indicators' own page, on FINECO-EN23@IR it is on the page
//! **before**, on EURIZON-IT24 it comes from a promise the product-name pipe deposited. So
//! [`FundOf`] says where to look, and the value may be a promise — which is what
//! `FundEsgIndicator` accepts for that field.
//!
//! # The trap common to every template page
//!
//! The glossary column of the European model sits at `x≈4.8` and, in reading order by ordinate
//! alone, **interleaves** with the body. The section therefore has to be cut by abscissa in the
//! `pdf_extract`, which is where `R23` puts geometry; what arrives here is the body alone.

use onig::Regex;

use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock, TextBlock};
use crate::core::pipeline::{FilterData, PipeError, TextFilterPipe};
use crate::core::promise::Promise;

/// A value of an indicator: a percentage, or a plain number.
///
/// The percentage is tried **first**, or `93.07% of the Fund…` would give `93.07` and leave the per
/// cent sign in the name — which is the same thing written in two different units.
pub const NUMBER: &str = r"[-+]?\d{1,3}(?:[.,]\d+)?\s*%|[-+]?\d+(?:[.,]\d+)?";

/// The marker opening an entry of the list: `1.`, `2.`, in a block of its own.
pub const MARKER: &str = r"^\d+[.)]$";

/// An indicator as the page writes it: a name, a value, and the block it came from.
///
/// The block is carried because the text block that comes out **is built from it**: a `TextBlock`
/// holds the `PdfBlock` it came from, that field takes part in equality, and the reference runs
/// were frozen with it. It is the entry's **first** block, which is where the indicator's sentence
/// begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indicator {
    pub name: String,
    pub value: String,
    pub block: PdfBlock,
}

/// What a reader refuses at build time.
#[derive(Debug, thiserror::Error)]
pub enum EsgError {
    #[error("'{pattern}' is not a valid pattern: {message}")]
    InvalidPattern { pattern: String, message: String },
    #[error("a separator that is empty would never close a label")]
    EmptySeparator,
}

fn compiled(pattern: &str) -> Result<Regex, EsgError> {
    Regex::new(pattern).map_err(|e| EsgError::InvalidPattern {
        pattern: pattern.to_string(),
        message: e.description().to_string(),
    })
}

/// The three shapes a measured page writes its indicators in.
///
/// Three variants and not three pipes, because what differs is **the reading**, and everything
/// around it — whose page this is, what block comes out, what the deserializer builds — is the
/// same. `onig::Regex` is not `Clone`, so neither is this; nothing clones a reader.
pub enum Reader {
    /// The sentence **begins with the number**, and the name is the rest of it.
    ///
    /// FINECO-EN23@IR p894 lists six numbered entries, each saying how much before saying of what:
    /// *"93.07% of the Fund was invested in securities of issuers with favourable ESG
    /// characteristics…"*.
    LeadingNumber {
        /// The marker opening an entry where the page writes it in a block of its own (`1.`).
        /// Where there is none the entries separate by group, and where there is no group either
        /// the section is a single entry.
        marker: Option<Regex>,
        /// The value anchored at the start, with the name as everything after it — the two groups
        /// of `\A\s*(number)\s*(.+)`, composed **once** here rather than per entry, because
        /// `onig::Regex` cannot hand its own pattern back and recompiling in the loop would be
        /// paying for the same string on every line of every page.
        leading: Regex,
        group_key: String,
        /// How many characters of the name to keep; `0` keeps all of it. Some sentences continue
        /// for two lines of qualifications that are not part of the name.
        max_name: usize,
    },
    /// The indicator is a **table**: the name above, one row per series, one column per year.
    ///
    /// The reader does not look at the page: it reads the metadata the `pdf_extract` wrote — which
    /// indicator, which series, which column — because that is geometry and lives there (`R23`).
    /// What lives here are the two decisions that are interpretation: **which series** and **which
    /// column**.
    DatedRows {
        indicator_key: String,
        series_key: String,
        column_key: String,
        /// The index of the series to keep. `0` is the product's, which is the first the table
        /// writes; the others are benchmarks.
        series: i64,
        /// The index of the column. The `pdf_extract` numbers them **from the most recent**, which
        /// is what `R5` asks to keep, hence `0`.
        column: i64,
        /// The value as the **whole** cell, `\A\s*(?:number)\s*\z`: a cell that is not entirely a
        /// number is not an indicator.
        whole: Regex,
    },
    /// `<label>: <value>`, one entry per line, the value being the number after the colon.
    ///
    /// UBS-EN23 p140 answers with a two-level list, and the entries carrying a datum are the second
    /// level's. Each measurement is written **twice**, once for the sub-fund and once for its
    /// benchmark, and the benchmark's is not a datum of the sub-fund: a phrase tells them apart,
    /// and `drop` is that phrase. The same phrase, the other way round, dirties the label — `… of
    /// the financial product` is not part of the indicator's name — and `strip` takes it off.
    LabelledPairs {
        /// What divides the label from the value. The split is on the **first** one, which is the
        /// one closing the label.
        separator: String,
        /// The expressions qualifying an entry as not the product's.
        drop: Vec<Regex>,
        /// The tails to take off the label.
        strip: Vec<Regex>,
        /// Of the text after the separator, the **first** number is taken, because the unit follows
        /// it (`26.81 tons CO₂ per million dollars revenues`).
        number: Regex,
        group_key: String,
    },
}

impl Reader {
    pub fn leading_number(
        marker: Option<&str>,
        number: &str,
        group_key: String,
        max_name: usize,
    ) -> Result<Self, EsgError> {
        Ok(Reader::LeadingNumber {
            marker: marker.map(compiled).transpose()?,
            leading: compiled(&format!(r"\A\s*({number})\s*(.+)"))?,
            group_key,
            max_name,
        })
    }

    pub fn dated_rows(
        indicator_key: String,
        series_key: String,
        column_key: String,
        series: i64,
        column: i64,
        number: &str,
    ) -> Result<Self, EsgError> {
        Ok(Reader::DatedRows {
            indicator_key,
            series_key,
            column_key,
            series,
            column,
            whole: compiled(&format!(r"\A\s*(?:{number})\s*\z"))?,
        })
    }

    pub fn labelled_pairs(
        separator: String,
        drop: &[String],
        strip: &[String],
        number: &str,
        group_key: String,
    ) -> Result<Self, EsgError> {
        if separator.is_empty() {
            return Err(EsgError::EmptySeparator);
        }
        Ok(Reader::LabelledPairs {
            separator,
            drop: drop.iter().map(|p| compiled(p)).collect::<Result<_, _>>()?,
            strip: strip.iter().map(|p| compiled(p)).collect::<Result<_, _>>()?,
            number: compiled(number)?,
            group_key,
        })
    }

    /// The indicators the page carries, in the order it writes them.
    pub fn call(&self, blocks: &[PdfBlock]) -> Vec<Indicator> {
        match self {
            Reader::LeadingNumber { marker, leading, group_key, max_name } => {
                let mut out = Vec::new();
                for entry in entries(blocks, marker.as_ref(), group_key) {
                    let text = joined(&entry);
                    // `R11` drops an entry that does not begin with a number, and that is what
                    // makes the prose-only page a case already covered rather than an exception.
                    let Some(found) = leading.captures(&text) else { continue };
                    let (value, name) =
                        (found.at(1).unwrap_or("").to_string(), found.at(2).unwrap_or("").to_string());
                    let mut name = name.trim().to_string();
                    if *max_name > 0 && name.chars().count() > *max_name {
                        name = name.chars().take(*max_name).collect::<String>().trim_end().to_string();
                    }
                    let Some(first) = entry.first() else { continue };
                    out.push(Indicator {
                        name: name.trim_end_matches('.').trim().to_string(),
                        value: value.trim().to_string(),
                        block: (*first).clone(),
                    });
                }
                out
            }
            Reader::DatedRows { indicator_key, series_key, column_key, series, column, whole } => {
                blocks
                    .iter()
                    .filter(|block| index_of(block, series_key) == Some(*series))
                    .filter(|block| index_of(block, column_key) == Some(*column))
                    .filter_map(|block| {
                        let value = text_of(block);
                        // A cell that is not a number is not an indicator (`R11`), and on this
                        // table it happens: the last row of each block is a source note.
                        whole.find(&value)?;
                        let name = block
                            .metadata
                            .get(indicator_key)
                            .map(string_of)
                            .unwrap_or_default()
                            .trim()
                            .trim_end_matches(':')
                            .trim()
                            .to_string();
                        if name.is_empty() {
                            return None;
                        }
                        Some(Indicator { name, value, block: block.clone() })
                    })
                    .collect()
            }
            Reader::LabelledPairs { separator, drop, strip, number, group_key } => {
                let mut out = Vec::new();
                for entry in entries(blocks, None, group_key) {
                    let text = joined(&entry);
                    let Some((name, tail)) = text.split_once(separator.as_str()) else { continue };
                    if drop.iter().any(|pattern| pattern.find(&text).is_some()) {
                        continue;
                    }
                    let Some(value) = number.find(tail).map(|(start, end)| tail[start..end].to_string())
                    else {
                        continue;
                    };
                    let mut name = name.to_string();
                    for pattern in strip {
                        name = pattern.replace_all(&name, "");
                    }
                    let name = name.trim().to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let Some(first) = entry.first() else { continue };
                    out.push(Indicator {
                        name,
                        value: value.trim().to_string(),
                        block: (*first).clone(),
                    });
                }
                out
            }
        }
    }
}

/// The blocks split into entries: on the marker where there is one, on the group otherwise, all
/// together where there is neither.
fn entries<'a>(blocks: &'a [PdfBlock], marker: Option<&Regex>, group_key: &str) -> Vec<Vec<&'a PdfBlock>> {
    let mut out: Vec<Vec<&PdfBlock>> = Vec::new();
    let mut previous: Option<Option<String>> = None;
    for block in blocks {
        let text = text_of(block);
        if marker.is_some_and(|pattern| pattern.find(&text).is_some()) {
            out.push(Vec::new());
            continue;
        }
        let group = block.metadata.get(group_key).map(string_of);
        if out.is_empty() || (group.is_some() && Some(group.clone()) != previous) {
            out.push(Vec::new());
        }
        out.last_mut().expect("a group was just pushed if there was none").push(block);
        previous = Some(group);
    }
    out.retain(|entry| !entry.is_empty());
    out
}

/// The texts of an entry's blocks, joined, with the empty ones left out.
fn joined(entry: &[&PdfBlock]) -> String {
    entry.iter().map(|block| text_of(block)).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ")
}

fn text_of(block: &PdfBlock) -> String {
    string_of(&block.content).trim().to_string()
}

fn string_of(value: &BlockValue) -> String {
    match value {
        BlockValue::Str(found) => found.clone(),
        BlockValue::Int(found) => found.to_string(),
        BlockValue::Float(found) => found.to_string(),
        _ => String::new(),
    }
}

/// The number written under `key`, or `None` if there is none or it is not a number.
fn index_of(block: &PdfBlock, key: &str) -> Option<i64> {
    match block.metadata.get(key)? {
        BlockValue::Int(found) => Some(*found),
        BlockValue::Str(found) => found.trim().parse().ok(),
        _ => None,
    }
}

/// Whose page this is.
#[derive(Debug, Clone, Default)]
pub enum FundOf {
    /// Nobody: the format reads the section and attributes it to no one, so no row comes out.
    ///
    /// ANIMA_SGR-IT24 is this case, and it is not a mistake: its page answers the question in prose,
    /// the reader finds no number, and the question of whose page it is never arises. The variant
    /// exists so that the configuration can say so instead of inventing an owner.
    #[default]
    Nobody,
    /// One promise for the whole document: one document, one sub-fund.
    Promised(String),
    /// The promise the page `n - offset` left, keyed by the **printed** page number this page
    /// carries in its metadata. See [`super::carryover::page`].
    PagePromise { field: String, page_key: String, offset: i64, separator: String },
}

impl FundOf {
    /// The value to write in the block's `fund`, or `None` when nobody can be named.
    fn resolve(&self, blocks: &[PdfBlock]) -> Option<BlockValue> {
        match self {
            FundOf::Nobody => None,
            FundOf::Promised(id) => Some(BlockValue::Promise(Promise::new(id))),
            FundOf::PagePromise { field, page_key, offset, separator } => {
                let page = blocks.iter().find_map(|block| index_of(block, page_key));
                let Some(page) = page else {
                    tracing::error!(
                        "this page carries no '{page_key}': the indicators cannot be tied to a fund \
                         and the rows are dropped"
                    );
                    return None;
                };
                Some(BlockValue::Promise(Promise::new(&format!(
                    "{field}{separator}{}",
                    page - offset
                ))))
            }
        }
    }
}

/// The `text_filter` of an indicators pipeline: read, and attach the sub-fund to each one.
pub struct TextFilterEsg {
    reader: Reader,
    fund: FundOf,
}

impl TextFilterEsg {
    pub fn new(reader: Reader, fund: FundOf) -> Self {
        Self { reader, fund }
    }

    pub fn call(&self, blocks: &[PdfBlock], _data: &FilterData<'_>) -> Vec<TextBlock> {
        let indicators = self.reader.call(blocks);
        if indicators.is_empty() {
            // `R11`: a section with no numbers is not a section missed, it is a section saying
            // there are no measurements. No warning, because there is nothing anomalous to report.
            return Vec::new();
        }
        let Some(owner) = self.fund.resolve(blocks) else { return Vec::new() };
        indicators
            .into_iter()
            .map(|indicator| {
                let metadata = [
                    ("fund".to_string(), owner.clone()),
                    ("name".to_string(), BlockValue::from(indicator.name.as_str())),
                    ("value".to_string(), BlockValue::from(indicator.value.as_str())),
                ]
                .into_iter()
                .collect();
                TextBlock::new(BlockType::RELEVANT_BLOCK, metadata, indicator.block)
            })
            .collect()
    }
}

impl TextFilterPipe for TextFilterEsg {
    fn name(&self) -> &str {
        "TextFilterEsg"
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        Ok(self.call(blocks, data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A section's blocks: one line per block, as `A2` hands them over.
    fn lines(texts: &[&str]) -> Vec<PdfBlock> {
        texts
            .iter()
            .map(|text| PdfBlock::new(BlockType::RELEVANT_BLOCK, BTreeMap::new(), BlockValue::from(*text)))
            .collect()
    }

    fn with_meta(pairs: &[(&str, BlockValue)], text: &str) -> PdfBlock {
        let metadata = pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        PdfBlock::new(BlockType::RELEVANT_BLOCK, metadata, BlockValue::from(text))
    }

    /// The indicators in a shape easy to compare: value, then name.
    fn read(reader: &Reader, blocks: &[PdfBlock]) -> Vec<(String, String)> {
        reader.call(blocks).into_iter().map(|i| (i.value, i.name)).collect()
    }

    fn leading(marker: Option<&str>, max_name: usize) -> Reader {
        Reader::leading_number(marker, NUMBER, "group".to_string(), max_name).unwrap()
    }

    fn dated(series: i64) -> Reader {
        Reader::dated_rows(
            "indicator".to_string(),
            "series".to_string(),
            "column".to_string(),
            series,
            0,
            NUMBER,
        )
        .unwrap()
    }

    /// FINECO-EN23@IR D1 p894, measured: six numbered sentences, each saying how much before
    /// saying of what — and the second beginning with a qualification instead of the number.
    fn fineco() -> Vec<PdfBlock> {
        lines(&[
            "1.",
            "93.07% of the Fund was invested in securities of issuers with favourable ESG characteristics ",
            "in accordance with Fidelity\u{2019}s Sustainable Investing Framework.",
            "2.",
            "in respect of its direct investments in corporate issuers, 0% of the Fund invested in ",
            "securities of issuers with exposure to the Exclusions.",
            "3.",
            "38.42% of the Fund was invested in sustainable investments.",
        ])
    }

    mod leading_number {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn each_numbered_item_is_one_indicator() {
            assert_eq!(
                read(&leading(Some(MARKER), 0), &fineco()),
                vec![
                    (
                        "93.07%".to_string(),
                        "of the Fund was invested in securities of issuers with favourable ESG \
                         characteristics in accordance with Fidelity\u{2019}s Sustainable Investing \
                         Framework"
                            .to_string()
                    ),
                    ("38.42%".to_string(), "of the Fund was invested in sustainable investments".to_string()),
                ]
            );
        }

        /// p894's second entry begins with a premise: the number is there but does not open the
        /// sentence. `R11` is a rule about the **value**, not about the text — keeping that entry
        /// would mean writing the premise as the indicator's name and a number from mid-sentence as
        /// its value.
        #[test]
        fn an_item_that_does_not_begin_with_a_number_is_not_an_indicator() {
            let second = fineco()[4..6].to_vec();
            assert!(read(&leading(Some(MARKER), 0), &second).is_empty());
        }

        /// ANIMA_SGR-IT24 p1726: four lines of discourse and no measurement (`R11`). The same
        /// configuration reads this page and FINECO's, which is the whole point of the family.
        #[test]
        fn a_section_of_prose_alone_produces_nothing() {
            let prose = lines(&[
                "Gli indicatori di sostenibilità usati da questo prodotto finanziario sono rappresentati ",
                "dall\u{2019}esclusione dei Paesi sanzionati dalle Nazioni Unite a livello di governo centrale per ",
                "sistematiche violazioni dei diritti umani. ",
            ]);
            assert!(read(&leading(Some(MARKER), 0), &prose).is_empty());
        }

        /// KAIROS-EN23 p114 measures `1.42` and `127.38`, which are not percentages.
        #[test]
        fn a_plain_number_is_a_value_as_much_as_a_percentage() {
            assert_eq!(
                read(&leading(None, 0), &lines(&["1.42 tonnes of GHG emissions"])),
                vec![("1.42".to_string(), "tonnes of GHG emissions".to_string())]
            );
        }

        /// The same measurement written in two different units: the sign belongs to the value.
        #[test]
        fn the_percentage_sign_stays_with_the_value_and_not_with_the_name() {
            let got = read(&leading(None, 0), &lines(&["93.07% of the Fund"]));
            assert_eq!(got[0].0, "93.07%");
            assert!(!got[0].1.starts_with('%'));
        }

        #[test]
        fn without_the_marker_the_group_is_what_separates_two_items() {
            let blocks = vec![
                with_meta(&[("group", BlockValue::from(0_i64))], "10% of the first thing"),
                with_meta(&[("group", BlockValue::from(1_i64))], "20% of the second thing"),
            ];
            assert_eq!(
                read(&leading(None, 0), &blocks),
                vec![
                    ("10%".to_string(), "of the first thing".to_string()),
                    ("20%".to_string(), "of the second thing".to_string()),
                ]
            );
        }

        #[test]
        fn a_name_can_be_cut_where_the_precisions_begin() {
            let got = read(&leading(Some(MARKER), 30), &fineco());
            assert!(got[0].1.chars().count() <= 30, "{:?}", got[0].1);
        }

        /// The block the text block will be built from is the entry's **first**, which is where the
        /// sentence begins — not the marker's, which is not part of the indicator.
        #[test]
        fn the_indicator_carries_the_block_its_sentence_begins_on() {
            let got = leading(Some(MARKER), 0).call(&fineco());
            assert!(
                string_of(&got[0].block.content).starts_with("93.07%"),
                "{:?}",
                got[0].block.content
            );
        }
    }

    mod dated_rows {
        use super::*;
        use pretty_assertions::assert_eq;

        /// AMUNDI-EN24 p778: `Weighted Average Carbon Intensity` is 71 for the product this year,
        /// 72 last year, and 83 / 101 for the `Stoxx 600` benchmark.
        fn table() -> Vec<PdfBlock> {
            [
                ("Weighted Average Carbon Intensity", 0, 0, "71"),
                ("Weighted Average Carbon Intensity", 0, 1, "72"),
                ("Weighted Average Carbon Intensity", 1, 0, "83"),
                ("Weighted Average Carbon Intensity", 1, 1, "101"),
                ("Carbon Footprint", 0, 0, "12"),
                ("Carbon Footprint", 0, 1, "16"),
                ("Coverage (% invested)", 0, 0, "96.51%"),
                ("Coverage (% invested)", 0, 1, "100.00%"),
            ]
            .into_iter()
            .map(|(name, series, column, value)| {
                with_meta(
                    &[
                        ("indicator", BlockValue::from(name)),
                        ("series", BlockValue::from(series as i64)),
                        ("column", BlockValue::from(column as i64)),
                    ],
                    value,
                )
            })
            .collect()
        }

        #[test]
        fn only_the_product_and_only_the_year_in_course() {
            assert_eq!(
                read(&dated(0), &table()),
                vec![
                    ("71".to_string(), "Weighted Average Carbon Intensity".to_string()),
                    ("12".to_string(), "Carbon Footprint".to_string()),
                    ("96.51%".to_string(), "Coverage (% invested)".to_string()),
                ]
            );
        }

        /// Not extracted, but the parameter is there: it is the format's choice, not the reader's
        /// rule.
        #[test]
        fn the_benchmark_row_is_reachable_by_asking_for_it() {
            assert_eq!(
                read(&dated(1), &table()),
                vec![("83".to_string(), "Weighted Average Carbon Intensity".to_string())]
            );
        }

        /// `R11`: the last row of each block of the page is a source note, not a measurement.
        #[test]
        fn a_cell_that_is_not_a_number_is_not_an_indicator() {
            let blocks = vec![with_meta(
                &[
                    ("indicator", BlockValue::from("Carbon Footprint")),
                    ("series", BlockValue::from(0_i64)),
                    ("column", BlockValue::from(0_i64)),
                ],
                "Source Montpensier Finance / MSCI",
            )];
            assert!(read(&dated(0), &blocks).is_empty());
        }

        #[test]
        fn a_cell_without_its_indicator_is_dropped() {
            let blocks = vec![with_meta(
                &[("series", BlockValue::from(0_i64)), ("column", BlockValue::from(0_i64))],
                "71",
            )];
            assert!(read(&dated(0), &blocks).is_empty());
        }
    }

    /// UBS-EN23 p140, the shape the Python template never had a test for.
    mod labelled_pairs {
        use super::*;
        use pretty_assertions::assert_eq;

        fn reader() -> Reader {
            Reader::labelled_pairs(
                ":".to_string(),
                &["of the benchmark".to_string()],
                &[r"^o\s+".to_string(), r"\s+of the financial product$".to_string()],
                NUMBER,
                "group".to_string(),
            )
            .unwrap()
        }

        fn entry(group: i64, text: &str) -> PdfBlock {
            with_meta(&[("group", BlockValue::from(group))], text)
        }

        /// The measured lines: each measurement written twice, once for the sub-fund and once for
        /// its benchmark.
        fn page() -> Vec<PdfBlock> {
            vec![
                entry(0, "o  Weighted Average Carbon Intensity (WACI) of the financial product: 26.81 tons CO\u{2082} per million dollars revenues"),
                entry(1, "o  Weighted Average Carbon Intensity (WACI) of the benchmark: 38.2 tons CO\u{2082} per million dollars revenues"),
                entry(2, "o  UBS consensus score of the financial product: 6.4"),
            ]
        }

        #[test]
        fn the_label_is_the_name_and_the_first_number_after_the_colon_is_the_value() {
            assert_eq!(
                read(&reader(), &page()),
                vec![
                    ("26.81".to_string(), "Weighted Average Carbon Intensity (WACI)".to_string()),
                    ("6.4".to_string(), "UBS consensus score".to_string()),
                ]
            );
        }

        /// The benchmark's measurement is not a datum of the sub-fund, and one phrase is all that
        /// tells the two apart.
        #[test]
        fn the_benchmarks_entry_is_dropped_by_the_phrase_that_names_it() {
            let kept = read(&reader(), &page());
            assert!(!kept.iter().any(|(value, _)| value == "38.2"), "{kept:?}");
        }

        /// The unit follows the number, so the **first** number after the separator is the value —
        /// `26.81 tons CO₂ per million dollars revenues` must not yield `2` from `CO₂`.
        #[test]
        fn the_unit_after_the_value_is_not_part_of_it() {
            assert_eq!(read(&reader(), &page())[0].0, "26.81");
        }

        #[test]
        fn an_entry_with_no_separator_is_not_an_entry() {
            let blocks = vec![entry(0, "o  For the characteristic 1")];
            assert!(read(&reader(), &blocks).is_empty());
        }

        /// A bullet with no label at all: `strip` empties it, and an indicator with no name is not
        /// an indicator.
        #[test]
        fn an_entry_whose_label_strips_to_nothing_is_dropped() {
            let blocks = vec![entry(0, "o  : 12")];
            assert!(read(&reader(), &blocks).is_empty());
        }

        /// What `strip` does **not** do, measured because it is easy to expect the opposite: the
        /// two patterns are applied in order and each one anchors on the text as it stands, so
        /// `^o\s+` removing the bullet leaves `of the financial product` with no leading space and
        /// `\s+of the financial product$` then no longer matches. The entry survives with that as
        /// its name. It is the template's behaviour, and no measured page writes such a line.
        #[test]
        fn stripping_is_applied_in_order_and_each_pattern_sees_the_text_as_it_stands() {
            let blocks = vec![entry(0, "o  of the financial product: 12")];
            assert_eq!(
                read(&reader(), &blocks),
                vec![("12".to_string(), "of the financial product".to_string())]
            );
        }

        #[test]
        fn an_empty_separator_is_refused_when_the_reader_is_built() {
            assert!(
                Reader::labelled_pairs(String::new(), &[], &[], NUMBER, "group".to_string()).is_err()
            );
        }
    }

    mod whose_page_this_is {
        use super::*;
        use crate::core::pipeline::FilterData;
        use pretty_assertions::assert_eq;

        fn rows(fund: FundOf, blocks: &[PdfBlock]) -> Vec<TextBlock> {
            TextFilterEsg::new(leading(Some(MARKER), 0), fund).call(blocks, &FilterData::EMPTY)
        }

        fn fund_of(block: &TextBlock) -> BlockValue {
            block.metadata["fund"].clone()
        }

        #[test]
        fn one_promise_for_the_whole_document() {
            let got = rows(FundOf::Promised("esg-indicator-fund".to_string()), &fineco());
            assert_eq!(got.len(), 2);
            assert_eq!(fund_of(&got[0]), BlockValue::Promise(Promise::new("esg-indicator-fund")));
        }

        /// The key is computed from the **printed** page number this page carries, and the engine
        /// resolves it at the end of the job.
        #[test]
        fn the_page_before_is_the_one_asked() {
            let mut blocks = fineco();
            blocks.push(with_meta(&[("page", BlockValue::from(894_i64))], ""));
            let got = rows(
                FundOf::PagePromise {
                    field: "esg-fund".to_string(),
                    page_key: "page".to_string(),
                    offset: 1,
                    separator: "-page-".to_string(),
                },
                &blocks,
            );
            assert_eq!(fund_of(&got[0]), BlockValue::Promise(Promise::new("esg-fund-page-893")));
        }

        /// UBS-EN23: the title at p14 and the statement at p16, two pages apart.
        #[test]
        fn a_format_whose_pages_are_two_apart_says_so() {
            let mut blocks = fineco();
            blocks.push(with_meta(&[("page", BlockValue::from(16_i64))], ""));
            let got = rows(
                FundOf::PagePromise {
                    field: "esg-fund".to_string(),
                    page_key: "page".to_string(),
                    offset: 2,
                    separator: "-page-".to_string(),
                },
                &blocks,
            );
            assert_eq!(fund_of(&got[0]), BlockValue::Promise(Promise::new("esg-fund-page-14")));
        }

        /// No printed number is no key, and a row tied to the wrong sub-fund is worse than one
        /// that is missing.
        #[test]
        fn a_page_without_its_number_yields_no_row() {
            let got = rows(
                FundOf::PagePromise {
                    field: "esg-fund".to_string(),
                    page_key: "page".to_string(),
                    offset: 1,
                    separator: "-page-".to_string(),
                },
                &fineco(),
            );
            assert!(got.is_empty());
        }

        /// ANIMA_SGR-IT24: the section answers in prose, so no row comes out and the question of
        /// whose page it is never arises. Saying so is what `FundOf::Nobody` is for.
        #[test]
        fn nobody_is_a_configuration_and_not_an_oversight() {
            let prose = lines(&["solo prosa, nessuna misura"]);
            assert!(rows(FundOf::Nobody, &prose).is_empty());
            // And with indicators on the page it still yields nothing, because there is nobody to
            // attribute them to: the same answer, for the stated reason.
            assert!(rows(FundOf::Nobody, &fineco()).is_empty());
        }

        /// The question costs a lookup, and on a prose-only page it has neither an answer nor a
        /// use: the reader runs first.
        #[test]
        fn a_page_without_indicators_never_asks_who_it_belongs_to() {
            let prose = lines(&["solo prosa"]);
            let got = rows(
                FundOf::PagePromise {
                    field: "esg-fund".to_string(),
                    page_key: "page".to_string(),
                    offset: 1,
                    separator: "-page-".to_string(),
                },
                &prose,
            );
            assert!(got.is_empty(), "nessuna riga, e nessun errore sul numero mancante");
        }
    }
}
