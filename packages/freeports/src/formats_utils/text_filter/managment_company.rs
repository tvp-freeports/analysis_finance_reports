//! The management company read **inside** the line that declares it, and `U6`.
//!
//! [`super::standard_funcs::TextFilterManagmentCompanyStandard`] takes the whole line, which is the
//! right answer where the label sits **above** the value and the value is all there is. This module
//! covers the other four measured ways — the label on the same line, the parenthesis that closes the
//! name, the prose sentence, the comma — and the rule on which of two companies is kept when the
//! report names both.
//!
//! The `pdf_extract` stays the engine's `A1`: *where* the value sits relative to its label is
//! geometry (`R23`). What a format adds here is *which piece of the line is the name*, which is text
//! interpretation and cannot be done earlier.
//!
//! # The name sits between two fixed expressions
//!
//! Four formats write the name inside a line wider than itself, and the four shapes are one thing
//! with two parameters:
//!
//! | how the line writes the name | reader | measured on |
//! |---|---|---|
//! | the label opens the line and the name is the rest — in **every** page's header | `after` | ANIMA_SGR-IT24 D1 p3 |
//! | the name opens the line and a parenthesis closes it | `before` | EURIZON-EN21 p7 |
//! | the name sits **in the middle** of a prose sentence | both | FIDEURAM-EN23 p67 |
//! | the name comes after a word and ends at the comma | both | EURIZON-IT24 D1 p7 |
//!
//! `after` and `before` are **fragments of a regular expression**, not text to find literally: the
//! FIDEURAM-EN23 line closes on `(the “Management Company”)`, and that parenthesis has to be
//! protected. Neither is required — no `after` means *from the start of the line*, no `before`
//! *to its end*.
//!
//! # The temporal qualification — `U6`
//!
//! A financial year in which the company changed **names both**, each with its period in brackets,
//! and the row to extract is one: the one in charge **at the closing date**.
//!
//! EURIZON-EN21 p7 writes it like this, under the label `Management Company:`:
//!
//! ```text
//! Pramerica Management Company S.A. (until June 30, 2021)
//! 37/A, Avenue J.F. Kennedy
//! L-1855 Luxembourg
//! Eurizon Capital S.A. (since July 1, 2021)
//! ```
//!
//! [`AmongLines::MostRecent`] keeps the second. The two address lines carry no qualification and are
//! not candidates: it is the qualification itself that filters, and not the geometry, because an
//! address sits under its name exactly as the next name does.

use std::collections::BTreeSet;

use onig::Regex;

use crate::commons::date::Date;
use crate::core::classes::{BlockType, PdfBlock, TextBlock};
use crate::core::match_fund::MatchFund;
use crate::core::page::PageError;
use crate::core::pipeline::{Extracted, FilterData, PipeError, TextFilterPipe};
use crate::output::classes::fund::Fund;

use super::standard_txt_blk_builders::{
    standard_management_company_txt_blk, standard_management_company_txt_blk_from_content,
};

/// Failures of building or running a management-company reader.
#[derive(Debug, thiserror::Error)]
pub enum ManagmentCompanyError {
    #[error("the reader's '{field}' is not a valid expression fragment: {message}")]
    InvalidFragment {
        field: &'static str,
        message: String,
    },
    #[error("no management company line on a page classified for one")]
    NoLine,
    #[error("the management company line has an unexpected shape: {line:?}")]
    UnexpectedShape { line: String },
}

impl ManagmentCompanyError {
    /// A page that cannot be read is one page: the shape failures are **non-fatal**, so the schedule
    /// skips the page and carries on, while a malformed expression stops the run because it is every
    /// page.
    fn into_pipe_error(self, pipe: &str) -> PipeError {
        match self {
            ManagmentCompanyError::NoLine | ManagmentCompanyError::UnexpectedShape { .. } => {
                PipeError::page_parse(
                    pipe,
                    PageError::ParseFail {
                        message: self.to_string(),
                    },
                )
            }
            other => PipeError::extraction(pipe, other.to_string()),
        }
    }
}

/// The name is the piece of line between two fixed expressions.
///
/// Reading yields `None` when the line does not have the expected shape: the caller decides whether
/// that is another line of the same section — an address, say — or the wrong page.
///
/// `Debug` is written out rather than derived: `onig::Regex` has none.
pub struct Between {
    pattern: Regex,
    /// The expression the two fragments were assembled into. Kept because `onig::Regex` does not
    /// give it back, and a reader that reads the wrong piece of a line is diagnosed by this string.
    source: String,
}

impl Between {
    /// Builds the reader from the two fragments, either of which may be empty.
    pub fn new(after: &str, before: &str) -> Result<Self, ManagmentCompanyError> {
        let head = if after.is_empty() {
            String::new()
        } else {
            format!("(?:{after})\\s*")
        };
        let tail = if before.is_empty() {
            "\\s*$".to_string()
        } else {
            format!("(?={before})")
        };
        let source = format!("{head}(.+?){tail}");
        let pattern = Regex::new(&source).map_err(|e| ManagmentCompanyError::InvalidFragment {
            field: if after.is_empty() { "before" } else { "after" },
            message: e.to_string(),
        })?;
        Ok(Self { pattern, source })
    }

    /// The name inside `text`, trimmed, or `None` when the line does not match.
    pub fn read(&self, text: &str) -> Option<String> {
        let captures = self.pattern.captures(text)?;
        captures.at(1).map(|found| found.trim().to_string())
    }
}

impl std::fmt::Debug for Between {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Between")
            .field("source", &self.source)
            .finish()
    }
}

/// Which of the candidate lines carries the company to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AmongLines {
    /// The normal case: the page declares **one** company, and that is the one taken.
    #[default]
    First,
    /// `U6` — of several companies declared with their period, the one in charge at closing.
    MostRecent,
}

/// The verbs that **close** a period instead of opening one.
///
/// `since July 1, 2021` says *from that day on*, so at the closing date that company was still
/// there; `until June 30, 2021` says the opposite. When every line carries an `until` — a document
/// naming only outgoing companies — the **last to leave** is kept anyway, as the nearest to the
/// closing date, the alternative being to extract nothing.
const OPEN_ENDED: [&str; 3] = ["until", "till", "fino al"];

/// The bracketed period next to a company's name.
static QUALIFICATION: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?i)\(\s*(until|till|since|from|fino al|dal)\s+([^)]+?)\s*\)")
        .expect("the qualification pattern is a literal and compiles")
});

/// An English date written month-first, which the engine's cast reads day-first.
static EN_DATE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^([A-Za-z]+)\s+(\d{1,2})\s*,?\s*(\d{4})$")
        .expect("the date pattern is a literal and compiles")
});

/// The qualification's verb and date, or `None` when the line carries none.
fn when(text: &str) -> Option<(String, Date)> {
    let found = QUALIFICATION.captures(text)?;
    let verb = found.at(1)?.to_ascii_lowercase();
    let mut written = found.at(2)?.trim().to_string();
    if let Some(english) = EN_DATE.captures(&written)
        && let (Some(month), Some(day), Some(year)) = (english.at(1), english.at(2), english.at(3))
    {
        written = format!("{day} {month} {year}");
    }
    match crate::formats_utils::deserialize::cast::to_date_with_en_month(&written) {
        Ok(date) => Some((verb, date)),
        // The bracket was there but held no date: not a temporal qualification, and a piece of
        // information we cannot read rather than a line to drop in silence (`R25`).
        Err(_) => {
            tracing::warn!(
                "a parenthesis next to a management company did not carry a date: {written:?}"
            );
            None
        }
    }
}

impl AmongLines {
    /// The chosen line among the candidates, or `None` when there are none.
    pub fn choose<'a>(self, texts: &[&'a str]) -> Option<&'a str> {
        match self {
            AmongLines::First => texts.first().copied(),
            AmongLines::MostRecent => {
                let qualified: Vec<(&'a str, (String, Date))> = texts
                    .iter()
                    .filter_map(|text| when(text).map(|w| (*text, w)))
                    .collect();
                // No line with a bracket means the company is a single one, and `First` is the
                // answer rather than nothing.
                if qualified.is_empty() {
                    return texts.first().copied();
                }
                let chosen = qualified
                    .iter()
                    // An open period beats a closed one at the same date: whoever came in on the
                    // first of July was there at closing, whoever left on the thirtieth of June
                    // was not.
                    .max_by_key(|(_, (verb, date))| {
                        (*date, !OPEN_ENDED.contains(&verb.as_str()))
                    })?;
                if qualified.len() > 1 {
                    let (verb, date) = &chosen.1;
                    tracing::info!(
                        "the report names {} management companies: kept the one in charge at closing ({verb} {date})",
                        qualified.len()
                    );
                }
                Some(chosen.0)
            }
        }
    }
}

/// The funds this job has seen so far, which the block carries by `R26`.
fn resolved_funds(data: &FilterData<'_>) -> BTreeSet<MatchFund> {
    data.previous()
        .iter()
        .filter_map(Extracted::as_fund)
        .filter_map(Fund::name)
        .map(MatchFund::new)
        .collect()
}

/// The `manco` `text_filter`: picks the line, reads the name out of it, declares it over the funds.
///
/// `R26`: the management company is **one to many** over the whole document, so the block carries
/// every fund the job has seen so far — which is why in `algorithms_schedule.csv` the `manco` class
/// always comes **after** the holdings.
///
/// A page classified `manco` that then has no name is a page classified wrongly, and it fails
/// **that page** without stopping the document.
pub struct TextFilterManagmentCompanyInLine {
    reader: Between,
    among: AmongLines,
}

impl TextFilterManagmentCompanyInLine {
    pub fn new(
        after: &str,
        before: &str,
        among: AmongLines,
    ) -> Result<Self, ManagmentCompanyError> {
        Ok(Self {
            reader: Between::new(after, before)?,
            among,
        })
    }

    pub fn call(
        &self,
        pdf_blks: &[PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, ManagmentCompanyError> {
        let candidates: Vec<(&PdfBlock, &str)> = pdf_blks
            .iter()
            .filter(|b| b.type_block == BlockType::MANAGEMENT_COMPANY)
            .filter_map(|b| b.content.as_str().map(|text| (b, text)))
            .collect();
        let texts: Vec<&str> = candidates.iter().map(|(_, text)| *text).collect();
        let chosen = self
            .among
            .choose(&texts)
            .ok_or(ManagmentCompanyError::NoLine)?;
        let name =
            self.reader
                .read(chosen)
                .ok_or_else(|| ManagmentCompanyError::UnexpectedShape {
                    line: chosen.to_string(),
                })?;
        let funds = resolved_funds(data);
        // The originating block travels with the value **when it cannot contradict it**: that is
        // the one rule `TextBlock::new` lays down, and it is why there is no constructor taking a
        // block and a content at once. Where the reader took the line whole, block and value say
        // the same thing and the provenance is kept; where it cut a name out of a sentence, the
        // block would claim a provenance for a string the page does not carry.
        //
        // It is what makes this pipe and `TextFilterManagmentCompanyStandard` the **same** reading
        // instead of two: that pipe always takes the line whole, so it always lands in the first
        // branch.
        let origin = candidates
            .iter()
            .find(|(_, text)| std::ptr::eq(*text, chosen) && *text == name.as_str())
            .map(|(block, _)| *block);
        Ok(vec![match origin {
            Some(block) => standard_management_company_txt_blk(block.clone(), &funds),
            None => standard_management_company_txt_blk_from_content(&name, &funds),
        }])
    }
}

impl TextFilterPipe for TextFilterManagmentCompanyInLine {
    fn name(&self) -> &str {
        "TextFilterManagmentCompanyInLine"
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

/// The `manco` `text_filter` for the company the report does **not** write: the name comes from the
/// format.
///
/// The engine already has a constant-currency pipe, and this is the same thing for the management
/// company: a value declared by the configuration instead of read from the document. The rule is
/// then that the engine must say so, so this pipe **warns** — once per document, because once per
/// document is the grain at which the fact holds.
///
/// It is for where the name is not there *in that role*: AMUNDI-IT24 writes it three times in prose,
/// but in sentences about corporate control and the registered office, not about who manages the
/// funds. Reading it there would rest an output value on a coincidence of words.
pub struct TextFilterManagmentCompanyImposed {
    name: String,
    warned: std::sync::atomic::AtomicBool,
}

impl TextFilterManagmentCompanyImposed {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            warned: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn call(&self, data: &FilterData<'_>) -> Vec<TextBlock> {
        if !self.warned.swap(true, std::sync::atomic::Ordering::Relaxed) {
            tracing::warn!(
                "the management company is not written in this report: imposed as \"{}\" - declared \
                 by the format, which knows whose report this is",
                self.name
            );
        }
        let funds = resolved_funds(data);
        vec![standard_management_company_txt_blk_from_content(
            &self.name, &funds,
        )]
    }
}

impl TextFilterPipe for TextFilterManagmentCompanyImposed {
    fn name(&self) -> &str {
        "TextFilterManagmentCompanyImposed"
    }

    fn filter(&self, _: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        Ok(self.call(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(content: &str) -> PdfBlock {
        PdfBlock::bare(BlockType::MANAGEMENT_COMPANY, content)
    }

    fn name_of(blocks: &[TextBlock]) -> String {
        blocks[0].content.as_str().unwrap_or_default().to_string()
    }

    mod the_reader {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_label_opens_the_line_and_the_name_is_the_rest() {
            // ANIMA_SGR-IT24 D1 p3, in every page's header.
            let reader = Between::new("Società di Gestione del Risparmio", "").unwrap();
            assert_eq!(
                reader
                    .read("Società di Gestione del Risparmio ANIMA SGR S.p.A.")
                    .as_deref(),
                Some("ANIMA SGR S.p.A.")
            );
        }

        #[test]
        fn a_parenthesis_closes_the_name() {
            // EURIZON-EN21 p7.
            let reader = Between::new("", r"\(").unwrap();
            assert_eq!(
                reader
                    .read("Eurizon Capital S.A. (since July 1, 2021)")
                    .as_deref(),
                Some("Eurizon Capital S.A.")
            );
        }

        #[test]
        fn the_name_sits_in_the_middle_of_a_sentence() {
            // FIDEURAM-EN23 p67, whose line closes on a parenthesis that has to be protected.
            let reader = Between::new("managed by", r"\(the").unwrap();
            let line =
                "The Fund is managed by Fideuram Asset Management (the “Management Company”)";
            assert_eq!(
                reader.read(line).as_deref(),
                Some("Fideuram Asset Management")
            );
        }

        #[test]
        fn the_name_ends_at_the_comma() {
            // EURIZON-IT24 D1 p7.
            let reader = Between::new("gestione", ",").unwrap();
            let line = "della società di gestione Eurizon Capital SGR S.p.A., con sede in Milano";
            assert_eq!(
                reader.read(line).as_deref(),
                Some("Eurizon Capital SGR S.p.A.")
            );
        }

        #[test]
        fn stating_neither_fragment_reads_the_whole_line() {
            let reader = Between::new("", "").unwrap();
            assert_eq!(
                reader.read("Acme Asset Management").as_deref(),
                Some("Acme Asset Management")
            );
        }

        #[test]
        fn a_line_of_another_shape_reads_as_nothing_rather_than_as_a_wrong_name() {
            let reader = Between::new("Management Company:", "").unwrap();
            assert!(reader.read("37/A, Avenue J.F. Kennedy").is_none());
        }

        #[test]
        fn the_name_comes_back_trimmed() {
            let reader = Between::new("label", ",").unwrap();
            assert_eq!(
                reader.read("label    Acme AM   , rest").as_deref(),
                Some("Acme AM")
            );
        }

        #[test]
        fn a_malformed_fragment_is_refused_naming_the_field_it_came_from() {
            let error = Between::new("", "(unclosed").unwrap_err();
            assert!(
                matches!(
                    error,
                    ManagmentCompanyError::InvalidFragment {
                        field: "before",
                        ..
                    }
                ),
                "{error}"
            );
            let error = Between::new("(unclosed", "").unwrap_err();
            assert!(
                matches!(
                    error,
                    ManagmentCompanyError::InvalidFragment { field: "after", .. }
                ),
                "{error}"
            );
        }
    }

    mod choosing_among_lines {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The four lines of EURIZON-EN21 p7, address lines included.
        const EURIZON: [&str; 4] = [
            "Pramerica Management Company S.A. (until June 30, 2021)",
            "37/A, Avenue J.F. Kennedy",
            "L-1855 Luxembourg",
            "Eurizon Capital S.A. (since July 1, 2021)",
        ];

        #[test]
        fn first_takes_the_first_line_whatever_it_says() {
            assert_eq!(AmongLines::First.choose(&EURIZON), Some(EURIZON[0]));
        }

        #[test]
        fn most_recent_keeps_the_company_in_charge_at_closing() {
            assert_eq!(AmongLines::MostRecent.choose(&EURIZON), Some(EURIZON[3]));
        }

        #[test]
        fn the_address_lines_do_not_compete_because_they_carry_no_qualification() {
            let only_addresses = ["37/A, Avenue J.F. Kennedy", "L-1855 Luxembourg"];
            // No qualification anywhere means the company is a single one: the first line answers.
            assert_eq!(
                AmongLines::MostRecent.choose(&only_addresses),
                Some(only_addresses[0])
            );
        }

        #[test]
        fn an_open_period_beats_a_closed_one_at_the_same_date() {
            let lines = [
                "Outgoing S.A. (until July 1, 2021)",
                "Incoming S.A. (since July 1, 2021)",
            ];
            assert_eq!(AmongLines::MostRecent.choose(&lines), Some(lines[1]));
        }

        #[test]
        fn of_outgoing_companies_only_the_last_to_leave_is_kept() {
            let lines = [
                "First S.A. (until March 31, 2021)",
                "Second S.A. (until June 30, 2021)",
            ];
            assert_eq!(AmongLines::MostRecent.choose(&lines), Some(lines[1]));
        }

        #[test]
        fn an_italian_qualification_is_read_too() {
            let lines = [
                "Uscente S.p.A. (fino al 30 giugno 2021)",
                "Entrante S.p.A. (dal 1 July 2021)",
            ];
            assert_eq!(AmongLines::MostRecent.choose(&lines), Some(lines[1]));
        }

        #[test]
        fn a_parenthesis_without_a_date_does_not_qualify_anything() {
            let lines = ["Acme AM (the Management Company)", "Other AM"];
            // It warns and falls back: no qualification means the first line.
            assert_eq!(AmongLines::MostRecent.choose(&lines), Some(lines[0]));
        }

        #[test]
        fn no_candidate_at_all_chooses_nothing() {
            assert!(AmongLines::First.choose(&[]).is_none());
            assert!(AmongLines::MostRecent.choose(&[]).is_none());
        }

        #[test]
        fn first_is_the_default() {
            assert_eq!(AmongLines::default(), AmongLines::First);
        }
    }

    mod the_in_line_pipe {
        use super::*;
        use pretty_assertions::assert_eq;

        fn pipe(after: &str, before: &str) -> TextFilterManagmentCompanyInLine {
            TextFilterManagmentCompanyInLine::new(after, before, AmongLines::First).unwrap()
        }

        #[test]
        fn it_emits_one_block_carrying_the_name_read_out_of_the_line() {
            let blocks = [block("Società di Gestione del Risparmio ANIMA SGR S.p.A.")];
            let got = pipe("Società di Gestione del Risparmio", "")
                .call(&blocks, &FilterData::EMPTY)
                .unwrap();
            assert_eq!(got.len(), 1);
            assert_eq!(name_of(&got), "ANIMA SGR S.p.A.");
        }

        #[test]
        fn blocks_of_another_type_are_not_candidates() {
            let blocks = [
                PdfBlock::bare(BlockType::FUND_NAME, "Alpha Fund"),
                block("label Acme AM"),
            ];
            let got = pipe("label", "").call(&blocks, &FilterData::EMPTY).unwrap();
            assert_eq!(name_of(&got), "Acme AM");
        }

        #[test]
        fn a_page_with_no_candidate_fails_that_page_and_says_which_pipe() {
            let error = pipe("label", "").call(&[], &FilterData::EMPTY).unwrap_err();
            assert!(matches!(error, ManagmentCompanyError::NoLine), "{error}");
            let as_pipe_error = error.into_pipe_error("p");
            assert!(
                matches!(as_pipe_error, PipeError::PageParse { .. }),
                "{as_pipe_error}"
            );
        }

        #[test]
        fn a_line_of_the_wrong_shape_fails_the_page_quoting_the_line() {
            let blocks = [block("37/A, Avenue J.F. Kennedy")];
            let error = pipe("Management Company:", "")
                .call(&blocks, &FilterData::EMPTY)
                .unwrap_err();
            assert!(error.to_string().contains("Avenue J.F. Kennedy"), "{error}");
            assert!(matches!(
                error.into_pipe_error("p"),
                PipeError::PageParse { .. }
            ));
        }

        #[test]
        fn the_block_carries_the_funds_the_job_has_seen() {
            // `R26`: the company is one to many over the document, so the block declares it over
            // every fund extracted so far.
            let previous = vec![
                Extracted::Fund(Fund::new("Alpha Fund")),
                Extracted::Fund(Fund::new("Beta Fund")),
            ];
            let blocks = [block("label Acme AM")];
            let got = pipe("label", "")
                .call(&blocks, &FilterData::Previous(&previous))
                .unwrap();
            assert!(
                !got[0].metadata.is_empty(),
                "the managed funds did not reach the block"
            );
        }

        /// `P9`: where the reader takes the line **whole**, the block travels with the value.
        ///
        /// It is what makes this pipe and `TextFilterManagmentCompanyStandard` one reading instead
        /// of two — that pipe always takes the line whole — and it is why five `manco` pipelines
        /// could stop being Python without a page fixture changing.
        #[test]
        fn the_whole_line_keeps_the_block_it_came_from() {
            let blocks = [block("Fideuram Asset Management (Ireland) dac")];
            let got = pipe("", "").call(&blocks, &FilterData::EMPTY).unwrap();
            assert_eq!(name_of(&got), "Fideuram Asset Management (Ireland) dac");
            assert_eq!(
                got[0].pdf_block.as_deref(),
                Some(&blocks[0]),
                "the whole line and the value say the same thing, so the provenance is kept"
            );
        }

        /// And where it cut a name out of a sentence, it does not.
        ///
        /// `TextBlock::new` inherits the content of the block it is given, so a block kept here
        /// would claim a provenance for a string the page does not carry — which is the
        /// contradiction that constructor exists to refuse.
        #[test]
        fn a_name_cut_out_of_a_line_carries_no_block() {
            let blocks = [block("Società di Gestione del Risparmio ANIMA SGR S.p.A.")];
            let got = pipe("Società di Gestione del Risparmio", "")
                .call(&blocks, &FilterData::EMPTY)
                .unwrap();
            assert_eq!(name_of(&got), "ANIMA SGR S.p.A.");
            assert!(
                got[0].pdf_block.is_none(),
                "the block says more than the value does"
            );
        }

        /// The trimming alone is enough to make them differ, and that is the measured case.
        #[test]
        fn a_line_padded_with_spaces_carries_no_block_either() {
            let blocks = [block("  Acme Asset Management  ")];
            let got = pipe("", "").call(&blocks, &FilterData::EMPTY).unwrap();
            assert_eq!(name_of(&got), "Acme Asset Management");
            assert!(
                got[0].pdf_block.is_none(),
                "the value is trimmed and the block is not"
            );
        }

        #[test]
        fn the_qualified_reading_picks_the_company_in_charge_at_closing() {
            let blocks = [
                block("Pramerica Management Company S.A. (until June 30, 2021)"),
                block("Eurizon Capital S.A. (since July 1, 2021)"),
            ];
            let pipe =
                TextFilterManagmentCompanyInLine::new("", r"\(", AmongLines::MostRecent).unwrap();
            assert_eq!(
                name_of(&pipe.call(&blocks, &FilterData::EMPTY).unwrap()),
                "Eurizon Capital S.A."
            );
        }
    }

    mod the_imposed_pipe {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn it_emits_the_configured_name_whatever_the_page_carries() {
            let pipe = TextFilterManagmentCompanyImposed::new("Amundi SGR S.p.A.");
            let got = pipe.call(&FilterData::EMPTY);
            assert_eq!(name_of(&got), "Amundi SGR S.p.A.");
        }

        #[test]
        fn it_needs_no_block_at_all() {
            let pipe = TextFilterManagmentCompanyImposed::new("Acme AM");
            assert_eq!(pipe.filter(&[], &FilterData::EMPTY).unwrap().len(), 1);
        }

        #[test]
        fn it_declares_the_name_over_the_funds_the_job_has_seen() {
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund"))];
            let got = TextFilterManagmentCompanyImposed::new("Acme AM")
                .call(&FilterData::Previous(&previous));
            assert!(!got[0].metadata.is_empty());
        }

        #[test]
        fn it_warns_once_and_not_once_per_page() {
            // The fact is about the format, not about the page: a document of forty pages would
            // otherwise leave forty identical rows in the log.
            let pipe = TextFilterManagmentCompanyImposed::new("Acme AM");
            assert!(!pipe.warned.load(std::sync::atomic::Ordering::Relaxed));
            pipe.call(&FilterData::EMPTY);
            assert!(pipe.warned.load(std::sync::atomic::Ordering::Relaxed));
            pipe.call(&FilterData::EMPTY);
            assert!(pipe.warned.load(std::sync::atomic::Ordering::Relaxed));
        }
    }
}
