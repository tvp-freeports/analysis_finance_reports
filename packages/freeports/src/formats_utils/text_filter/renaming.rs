//! `A7` — the renamings and mergers of a sub-fund, read once for every format.
//!
//! The datum is always the same — **old name, new name, date** — and what changes from one format
//! to the next is where the date is written and how the pair of names is held together:
//!
//! | how the page writes it | reader | measured on |
//! |---|---|---|
//! | a **dated heading**, and under it a list `<old> into <new>` | [`Reader::Listed`] | AMUNDI-EN24 p510 |
//! | a **sentence per event**, with the verb and the date inside it | [`Reader::Sentence`] | DANSKEINVEST-EN24 p351 · KAIROS-EN23 D2 p104 |
//! | the **section is there and empty**: it declares the absence | neither, and that is the point | ARCA-IT24 D1 p6 |
//!
//! The third row has no reader, and that is deliberate: a section saying *"Non si segnalano eventi
//! rilevanti"* is read with the same two readers, which find nothing and produce nothing. It is a
//! positive test — the page was found, the section opened, and the empty result **is** the datum —
//! which is why that page's test is written all the same.
//!
//! # The sibling sections, which are the real trap
//!
//! AMUNDI-EN24 p510 puts three sections with identical typography one under the other: the
//! sub-funds **launched**, those **renamed**, those **liquidated**. Only the second carries a
//! datum. [`Reader::Listed`] tells them apart without looking at section names one by one: a
//! heading matching one of the declared patterns **opens** a scope with its date, and any other
//! line not carrying the connector **closes** it. So a new section nobody foresaw does not get in
//! by accident, which is the behaviour wanted when the document changes from one year to the next.
//!
//! # Which of the two names has to be a sub-fund of the job
//!
//! The `text_filter` does one thing beyond calling the reader: it **keeps the changes concerning a
//! sub-fund of the job**. That is `R24` applied here — the name reaching the output must be the one
//! the holdings write — and at the same time the filter against noise, because a section of prose
//! also names sub-funds of other documents and companies that have nothing to do with it.

use std::collections::BTreeSet;

use onig::Regex;

use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock, TextBlock};
use crate::core::match_fund::MatchFund;
use crate::core::pipeline::{Extracted, FilterData, PipeError, TextFilterPipe};
use crate::output::classes::fund::Fund;

/// The note marks taken off a sub-fund's name (`R16`, `U3`).
pub const NOTE_MARKS: &str = "*†‡¹²³⁴⁵";

/// The three shapes the collection's documents write a date in, inside a sentence.
///
/// Not a converter: here the date is **found**, and reading it is the `deserialize`'s, which is the
/// only segment `R23` leaves the casts to.
pub const DATE: &str = r"\d{1,2}\s+\w+\s+\d{4}|\w+\s+\d{1,2},?\s+\d{4}|\d{1,2}[/.]\d{1,2}[/.]\d{4}";

/// Where a heading's date is introduced, and which is cut off a line that carries its own.
pub const DATE_INTRO: &str = r"\s+(?:on|il|dal|in data|a far data dal)$";

/// A change as the page declares it: two names, a date, and which kind of change it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub old_name: String,
    pub current_name: String,
    pub when: String,
    pub merge: bool,
    /// The block the text block will be built from — it takes part in equality, and the reference
    /// runs were frozen with it.
    pub block: PdfBlock,
}

/// What a reader refuses at build time.
#[derive(Debug, thiserror::Error)]
pub enum RenamingError {
    #[error("'{pattern}' is not a valid pattern: {message}")]
    InvalidPattern { pattern: String, message: String },
    #[error("a connector that is empty would split every line of the list")]
    EmptyConnector,
    #[error("'{0}' needs the three named groups 'old', 'current' and 'when'")]
    MissingGroups(String),
}

fn compiled(pattern: &str) -> Result<Regex, RenamingError> {
    Regex::new(pattern).map_err(|e| RenamingError::InvalidPattern {
        pattern: pattern.to_string(),
        // The engine's regexes are Oniguruma's, where a named group is `(?<name>…)`. Python's
        // `(?P<name>…)` — which is what the templates these patterns come from were written in —
        // fails with `undefined group option`, a message that says nothing about the cause. Since
        // this is the one mistake a pattern moved from a template will make, it is named here.
        message: if pattern.contains("(?P<") {
            format!(
                "{} - a named group is written `(?<name>…)` here and not `(?P<name>…)`, which is \
                 Python's spelling",
                e.description()
            )
        } else {
            e.description().to_string()
        },
    })
}

/// A sentence pattern with the indexes of its three named groups, resolved **once**.
///
/// `onig` cannot look a group up by name on a `Captures`, only by index, and it will not hand back
/// its own pattern string either. So the three indexes are read at build time with `foreach_name`
/// — which is also where a pattern missing one of them is refused, instead of silently reading
/// nothing on the page that needed it.
pub struct Named {
    pattern: Regex,
    old: usize,
    current: usize,
    when: usize,
}

impl Named {
    pub fn new(pattern: Regex) -> Result<Self, RenamingError> {
        let mut found: [Option<usize>; 3] = [None; 3];
        pattern.foreach_name(|name, indexes| {
            let slot = match name {
                "old" => 0,
                "current" => 1,
                "when" => 2,
                _ => return true,
            };
            found[slot] = indexes.first().map(|index| *index as usize);
            true
        });
        let [Some(old), Some(current), Some(when)] = found else {
            return Err(RenamingError::MissingGroups(
                "a sentence pattern".to_string(),
            ));
        };
        Ok(Self { pattern, old, current, when })
    }

    /// The group's text, or the empty string: an index resolved at build time cannot be absent, and
    /// an optional group that did not participate is an empty name, which `clean` leaves empty and
    /// the filter then drops.
    fn group<'t>(&self, caps: &onig::Captures<'t>, index: usize) -> &'t str {
        caps.at(index).unwrap_or("")
    }
}

/// A set of patterns and the kind of change they declare.
pub struct Patterns {
    renamed: Vec<Regex>,
    merged: Vec<Regex>,
}

impl Patterns {
    fn new(renamed: &[String], merged: &[String]) -> Result<Self, RenamingError> {
        Ok(Self {
            renamed: renamed.iter().map(|p| compiled(p)).collect::<Result<_, _>>()?,
            merged: merged.iter().map(|p| compiled(p)).collect::<Result<_, _>>()?,
        })
    }

    /// The two groups in the order they are tried: renamings first, mergers after, which is the
    /// order the output rows come out in and therefore an output value.
    fn groups(&self) -> [(&[Regex], bool); 2] {
        [(&self.renamed, false), (&self.merged, true)]
    }
}

/// How a page ties the two names together.
pub enum Reader {
    /// A dated heading opens a list, and every line of the list is a change.
    Listed {
        patterns: Patterns,
        /// What divides the old name from the new one on the line. A line not carrying it
        /// **closes** the open scope, and that is how the sibling sections stay out.
        connector: String,
        date: Regex,
        date_intro: Regex,
        /// What has to come off both names so they coincide with the holdings' (`R24`): an umbrella
        /// prefix, usually.
        strip: Vec<Regex>,
        note_marks: String,
    },
    /// A sentence per event, with both names and the date inside it.
    ///
    /// The patterns carry three named groups — `old`, `current`, `when` — and are tried against
    /// **all** of the text, not only at its start: one sentence may declare more than one change
    /// (EURIZON-IT24 D4 p14 declares two), and looking for one would lose half of them.
    Sentence {
        /// The renaming patterns and then the merger ones, each with its group indexes, and the
        /// flag saying which of the two it is. One list and not two, because the order the two
        /// groups are tried in is the order the output rows come out in.
        patterns: Vec<(Named, bool)>,
        /// Which metadata key separates one paragraph from the next; without it the blocks join
        /// into one, because a sentence wraps.
        group_key: String,
        strip: Vec<Regex>,
        note_marks: String,
    },
}

impl Reader {
    pub fn listed(
        renamed: &[String],
        merged: &[String],
        connector: String,
        date: &str,
        date_intro: &str,
        strip: &[String],
        note_marks: String,
    ) -> Result<Self, RenamingError> {
        if connector.is_empty() {
            return Err(RenamingError::EmptyConnector);
        }
        Ok(Reader::Listed {
            patterns: Patterns::new(renamed, merged)?,
            connector,
            date: compiled(date)?,
            date_intro: compiled(date_intro)?,
            strip: strip.iter().map(|p| compiled(p)).collect::<Result<_, _>>()?,
            note_marks,
        })
    }

    pub fn sentence(
        renamed: &[String],
        merged: &[String],
        group_key: String,
        strip: &[String],
        note_marks: String,
    ) -> Result<Self, RenamingError> {
        let mut patterns = Vec::new();
        for (set, merge) in [(renamed, false), (merged, true)] {
            for pattern in set {
                patterns.push((Named::new(compiled(pattern)?)?, merge));
            }
        }
        Ok(Reader::Sentence {
            patterns,
            group_key,
            strip: strip.iter().map(|p| compiled(p)).collect::<Result<_, _>>()?,
            note_marks,
        })
    }

    /// The changes the page declares, in the order it writes them.
    pub fn call(&self, blocks: &[PdfBlock]) -> Vec<Change> {
        match self {
            Reader::Listed { patterns, connector, date, date_intro, strip, note_marks } => {
                let mut out = Vec::new();
                let mut scope: Option<(Option<String>, bool)> = None;
                for block in blocks {
                    let text = text_of(block);
                    if text.is_empty() {
                        continue;
                    }
                    if text.contains(connector.as_str()) && scope.is_some() {
                        let (when, merge) = scope.clone().expect("just checked");
                        if let Some(change) = listed_change(
                            &text, when, merge, block, connector, date, date_intro, strip, note_marks,
                        ) {
                            out.push(change);
                        }
                        continue;
                    }
                    scope = opens(&text, patterns, date);
                }
                out
            }
            Reader::Sentence { patterns, group_key, strip, note_marks } => {
                let mut out = Vec::new();
                for entry in entries(blocks, group_key) {
                    let text = entry
                        .iter()
                        .map(|block| text_of(block))
                        .filter(|line| !line.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let Some(first) = entry.first() else { continue };
                    for (named, merge) in patterns {
                        for caps in named.pattern.captures_iter(&text) {
                            out.push(Change {
                                old_name: clean(named.group(&caps, named.old), note_marks, strip),
                                current_name: clean(named.group(&caps, named.current), note_marks, strip),
                                when: named.group(&caps, named.when).trim().to_string(),
                                merge: *merge,
                                block: (*first).clone(),
                            });
                        }
                    }
                }
                out
            }
        }
    }
}

/// The date and the kind of change if this line opens a list, `None` otherwise.
///
/// A heading **without** a date opens all the same: it means the date is on every line, which is
/// the shape AMUNDI-EN24 writes its mergers in, under the list of renamings that carries its date
/// only once.
fn opens(text: &str, patterns: &Patterns, date: &Regex) -> Option<(Option<String>, bool)> {
    for (set, merge) in patterns.groups() {
        for pattern in set {
            if pattern.find(text).is_none() {
                continue;
            }
            let when = date.find(text).map(|(start, end)| text[start..end].to_string());
            return Some((when, merge));
        }
    }
    None
}

/// The change a line of the list declares, with its own date or the heading's.
#[allow(clippy::too_many_arguments)]
fn listed_change(
    text: &str,
    heading_date: Option<String>,
    merge: bool,
    block: &PdfBlock,
    connector: &str,
    date: &Regex,
    date_intro: &Regex,
    strip: &[Regex],
    note_marks: &str,
) -> Option<Change> {
    let mut when = heading_date;
    let mut text = text.to_string();
    if let Some((start, end)) = date.find(&text) {
        // The date on the line wins over the heading's, and has to come off the name: on
        // AMUNDI-EN24 p510 the list of mergers carries it line by line, and leaving it there would
        // call the sub-fund `… ESG Improvers on 23 February 2024`.
        when = Some(text[start..end].to_string());
        text = date_intro.replace_all(text[..start].trim_end(), "");
    }
    let Some(when) = when else {
        tracing::warn!(
            coord_ref_1 = text.as_str(),
            "'{text}' declares a fund name change with no date, neither on the line nor on the \
             heading above it: the line is not read"
        );
        return None;
    };
    let (old, current) = text.split_once(connector)?;
    Some(Change {
        old_name: clean(old, note_marks, strip),
        current_name: clean(current, note_marks, strip),
        when,
        merge,
        block: block.clone(),
    })
}

/// The blocks split into paragraphs: by group where the metadata says so, all in one otherwise.
fn entries<'a>(blocks: &'a [PdfBlock], group_key: &str) -> Vec<Vec<&'a PdfBlock>> {
    let mut out: Vec<Vec<&PdfBlock>> = Vec::new();
    let mut previous: Option<Option<String>> = None;
    for block in blocks {
        let group = block.metadata.get(group_key).map(string_of);
        if out.is_empty() || (group.is_some() && Some(group.clone()) != previous) {
            out.push(Vec::new());
        }
        out.last_mut().expect("a paragraph was just pushed if there was none").push(block);
        previous = Some(group);
    }
    out
}

/// The name as the holdings write it: no full stop, no note mark, no umbrella.
///
/// `strip` removes what the section writes and the holdings do not — on AMUNDI-EN24 the prefix
/// `Amundi Funds `, which the same format's SFDR module already removes for the same reason. It is
/// `R24`: two ways of writing one sub-fund are the same sub-fund only if the two writings arrive at
/// the same form.
fn clean(text: &str, note_marks: &str, strip: &[Regex]) -> String {
    let mut name = text.trim().trim_end_matches('.').trim().to_string();
    for pattern in strip {
        name = pattern.replace_all(&name, "").trim().to_string();
    }
    if note_marks.is_empty() {
        return name;
    }
    name.trim_end_matches(|c| note_marks.contains(c)).trim().to_string()
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

/// Which of the two names has to be a sub-fund the job has seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OfInterest {
    /// The **new** name, which is the common case: the document is the one of the year after the
    /// change, and the holdings speak with the new name.
    #[default]
    Current,
    /// The **old** one, where it is the old name still in the holdings.
    Old,
    /// Either of the two.
    Any,
}

/// The `text_filter` of a renamings pipeline: read, filter, and write the blocks.
pub struct TextFilterRenaming {
    reader: Reader,
    of_interest: OfInterest,
}

impl TextFilterRenaming {
    pub fn new(reader: Reader, of_interest: OfInterest) -> Self {
        Self { reader, of_interest }
    }

    pub fn call(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Vec<TextBlock> {
        let funds: BTreeSet<MatchFund> = data
            .previous()
            .iter()
            .filter_map(Extracted::as_fund)
            .filter_map(Fund::name)
            .map(MatchFund::new)
            .collect();
        self.reader
            .call(blocks)
            .into_iter()
            .filter(|change| self.is_of_interest(change, &funds))
            .map(|change| {
                let metadata = [
                    ("old_name".to_string(), BlockValue::from(change.old_name.as_str())),
                    ("current_name".to_string(), BlockValue::from(change.current_name.as_str())),
                    ("when".to_string(), BlockValue::from(change.when.as_str())),
                    ("merge".to_string(), BlockValue::Bool(change.merge)),
                ]
                .into_iter()
                .collect();
                TextBlock::new(BlockType::RELEVANT_BLOCK, metadata, change.block)
            })
            .collect()
    }

    fn is_of_interest(&self, change: &Change, funds: &BTreeSet<MatchFund>) -> bool {
        let old = funds.contains(&MatchFund::new(&change.old_name));
        let current = funds.contains(&MatchFund::new(&change.current_name));
        match self.of_interest {
            OfInterest::Old => old,
            OfInterest::Any => old || current,
            OfInterest::Current => current,
        }
    }
}

impl TextFilterPipe for TextFilterRenaming {
    fn name(&self) -> &str {
        "TextFilterRenaming"
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

    /// The changes in a shape easy to compare.
    fn read(reader: &Reader, blocks: &[PdfBlock]) -> Vec<(String, String, String, bool)> {
        reader
            .call(blocks)
            .into_iter()
            .map(|c| (c.old_name, c.current_name, c.when, c.merge))
            .collect()
    }

    /// AMUNDI-EN24 p510's configuration.
    fn amundi() -> Reader {
        Reader::listed(
            &[r"have been renamed".to_string()],
            &[r"have been merged".to_string()],
            " into ".to_string(),
            DATE,
            DATE_INTRO,
            &[r"^Amundi Funds\s+".to_string()],
            NOTE_MARKS.to_string(),
        )
        .unwrap()
    }

    /// DANSKEINVEST-EN24 p351's configuration, in the engine's own spelling of a named group.
    fn danske() -> Reader {
        Reader::sentence(
            &[r"(?<old>Danske Invest SICAV\s*[-–]\s*[^.]+?) was renamed (?<current>[^.]+?) effective (?<when>\d{1,2} \w+ \d{4})".to_string()],
            &[r"(?<old>Danske Invest SICAV\s*[-–]\s*[^.]+?) merged into (?<current>[^.]+?) on (?<when>\d{1,2} \w+ \d{4})".to_string()],
            "group".to_string(),
            &[r"^Danske Invest SICAV\s*[-–]\s*".to_string()],
            NOTE_MARKS.to_string(),
        )
        .unwrap()
    }

    mod listed {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_heading_gives_its_date_to_every_line_below_it() {
            let blocks = lines(&[
                "The following Sub-Funds have been renamed on 15 March 2024: ",
                "Amundi Funds Pioneer US Bond into Amundi Funds US Bond ",
                "Amundi Funds Pioneer US Corporate Bond into Amundi Funds US Corporate Bond ",
            ]);
            assert_eq!(
                read(&amundi(), &blocks),
                vec![
                    ("Pioneer US Bond".into(), "US Bond".into(), "15 March 2024".into(), false),
                    (
                        "Pioneer US Corporate Bond".into(),
                        "US Corporate Bond".into(),
                        "15 March 2024".into(),
                        false
                    ),
                ]
            );
        }

        /// p510's three sections have identical typography, and only one carries a datum.
        #[test]
        fn a_sibling_section_closes_the_list_instead_of_continuing_it() {
            let blocks = lines(&[
                "The following Sub-Funds have been renamed on 15 March 2024: ",
                "Amundi Funds Pioneer US Bond into Amundi Funds US Bond ",
                "The following Sub-Funds have been liquidated during the year: ",
                "Amundi Funds Global Macro Bonds & Currencies on 12 March 2024 ",
            ]);
            assert_eq!(
                read(&amundi(), &blocks),
                vec![("Pioneer US Bond".into(), "US Bond".into(), "15 March 2024".into(), false)]
            );
        }

        /// p510's list of mergers carries its date line by line, not in the heading.
        #[test]
        fn the_date_on_the_line_wins_over_the_one_on_the_heading() {
            let blocks = lines(&[
                "The following Sub-Funds have been merged during the period: ",
                "Amundi S.F. European Equity Optimal Volatility into Amundi Funds Global Equity ESG Improvers on 13 October 2023 ",
            ]);
            assert_eq!(
                read(&amundi(), &blocks),
                vec![(
                    "Amundi S.F. European Equity Optimal Volatility".into(),
                    "Global Equity ESG Improvers".into(),
                    "13 October 2023".into(),
                    true
                )]
            );
        }

        #[test]
        fn a_line_with_no_date_anywhere_is_reported_and_dropped() {
            let blocks = lines(&[
                "The following Sub-Funds have been merged during the period: ",
                "Amundi Funds Alpha into Amundi Funds Beta ",
            ]);
            assert!(read(&amundi(), &blocks).is_empty());
        }

        /// The note's introduction names old names of the SICAV, and those are not renamings.
        #[test]
        fn a_line_before_any_heading_is_not_a_change() {
            let blocks = lines(&[
                "The name of the Fund was then changed to GIF SICAV into something else ",
                "The following Sub-Funds have been renamed on 15 March 2024: ",
                "Amundi Funds Pioneer US Bond into Amundi Funds US Bond ",
            ]);
            assert_eq!(
                read(&amundi(), &blocks),
                vec![("Pioneer US Bond".into(), "US Bond".into(), "15 March 2024".into(), false)]
            );
        }

        #[test]
        fn an_empty_connector_is_refused_when_the_reader_is_built() {
            assert!(
                Reader::listed(&[], &[], String::new(), DATE, DATE_INTRO, &[], String::new()).is_err()
            );
        }
    }

    mod sentence {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_sentence_that_wraps_is_read_whole() {
            let blocks = lines(&[
                "Danske Invest SICAV - Global Corporate Sustainable Bond was renamed European Corporate Green Bond effective 22 November ",
                "2024.",
            ]);
            assert_eq!(
                read(&danske(), &blocks),
                vec![(
                    "Global Corporate Sustainable Bond".into(),
                    "European Corporate Green Bond".into(),
                    "22 November 2024".into(),
                    false
                )]
            );
        }

        /// The same section carries launches and liquidations, which are not name changes.
        #[test]
        fn the_sentences_that_declare_no_change_are_left_out() {
            let blocks = lines(&[
                "Danske Invest SICAV - Global Fixed Income Solution fund launched on 7 November 2024.",
                "Danske Invest SICAV - Multi Asset Inflation Strategy was liquidated on 7 October 2024.",
                "Danske Invest SICAV – India merged into Danske Invest SICAV – Global Emerging Markets on 21 February 2025.",
            ]);
            assert_eq!(
                read(&danske(), &blocks),
                vec![("India".into(), "Global Emerging Markets".into(), "21 February 2025".into(), true)]
            );
        }

        /// The defect the joined text brings out: without the full stop the name takes them all.
        #[test]
        fn a_name_never_crosses_the_full_stop_that_ends_the_previous_event() {
            let blocks = lines(&[
                "Danske Invest SICAV - Something Else fund launched on 7 November 2024.",
                "Danske Invest SICAV – India merged into Danske Invest SICAV – Global Emerging Markets on 21 February 2025.",
            ]);
            assert_eq!(read(&danske(), &blocks)[0].0, "India");
        }

        /// ARCA-IT24 p6: `Non si segnalano eventi rilevanti.` **is** the datum, not a gap.
        #[test]
        fn a_section_that_declares_the_absence_produces_nothing() {
            assert!(read(&danske(), &lines(&["Non si segnalano eventi rilevanti. "])).is_empty());
        }

        /// Python's spelling of a named group is the one mistake a pattern moved from a template
        /// will make, and the engine's regexes are Oniguruma's: it is refused, by name.
        #[test]
        fn pythons_spelling_of_a_named_group_is_refused_and_the_message_says_why() {
            let refused = Reader::sentence(
                &[r"(?P<old>A) to (?P<current>B) on (?P<when>C)".to_string()],
                &[],
                "group".to_string(),
                &[],
                String::new(),
            );
            // `Reader` is not `Debug` — `onig::Regex` is not — so only the error is matched.
            match refused.err() {
                Some(RenamingError::InvalidPattern { message, .. }) => {
                    assert!(message.contains("(?<name>"), "{message}");
                }
                other => panic!("Python's spelling must be refused by name, got {other:?}"),
            }
        }

        #[test]
        fn a_pattern_without_the_three_groups_is_refused_when_the_reader_is_built() {
            let refused = Reader::sentence(
                &[r"(?<old>A) to (?<current>B)".to_string()],
                &[],
                "group".to_string(),
                &[],
                String::new(),
            );
            assert!(matches!(refused.err(), Some(RenamingError::MissingGroups(_))));
        }
    }

    mod which_of_the_two_names {
        use super::*;
        use crate::core::pipeline::FilterData;
        use pretty_assertions::assert_eq;

        fn blocks() -> Vec<PdfBlock> {
            lines(&[
                "The following Sub-Funds have been renamed on 15 March 2024: ",
                "Amundi Funds Pioneer US Bond into Amundi Funds US Bond ",
            ])
        }

        fn kept(of_interest: OfInterest, funds: &[&str]) -> Vec<TextBlock> {
            let previous: Vec<Extracted> =
                funds.iter().map(|name| Extracted::Fund(Fund::new(name))).collect();
            TextFilterRenaming::new(amundi(), of_interest)
                .call(&blocks(), &FilterData::Previous(&previous))
        }

        #[test]
        fn by_default_the_new_name_is_the_one_that_has_to_match() {
            let got = kept(OfInterest::Current, &["US Bond"]);
            assert_eq!(got.len(), 1);
            assert_eq!(got[0].metadata["old_name"].as_str(), Some("Pioneer US Bond"));
            assert_eq!(got[0].metadata["merge"], BlockValue::Bool(false));
        }

        /// `R24`: the name reaching the output must be the one the holdings write.
        #[test]
        fn a_change_no_fund_of_the_job_carries_is_left_out() {
            assert!(kept(OfInterest::Current, &["Another Fund"]).is_empty());
        }

        #[test]
        fn the_old_name_can_be_the_one_that_has_to_match() {
            assert_eq!(kept(OfInterest::Old, &["Pioneer US Bond"]).len(), 1);
            assert!(kept(OfInterest::Current, &["Pioneer US Bond"]).is_empty());
        }

        #[test]
        fn either_of_the_two_is_a_third_answer() {
            assert_eq!(kept(OfInterest::Any, &["Pioneer US Bond"]).len(), 1);
            assert_eq!(kept(OfInterest::Any, &["US Bond"]).len(), 1);
            assert!(kept(OfInterest::Any, &["Neither"]).is_empty());
        }

        /// The block the text block is built from is the line of the list, not the heading: the
        /// reference runs were frozen with it, and it is what a reader of the `.log.csv` looks for
        /// in the PDF.
        #[test]
        fn the_block_the_change_was_read_on_travels_with_it() {
            let got = kept(OfInterest::Current, &["US Bond"]);
            assert_eq!(
                got[0].content.as_str(),
                Some("Amundi Funds Pioneer US Bond into Amundi Funds US Bond ")
            );
        }
    }
}
