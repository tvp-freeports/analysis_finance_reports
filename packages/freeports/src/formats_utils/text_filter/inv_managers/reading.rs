//! How a page ties a manager to its sub-funds: the readers that come before the assignment.
//!
//! # What is here and what is elsewhere
//!
//! [`super::assignment`] **assigns** and does not read: it receives pairs *name → sub-funds* and
//! decides whose the residual is. This module is the missing half on the other side — it **builds
//! those pairs** by reading the blocks the `pdf_extract` took off the managers' section.
//!
//! Sixteen formats declare something to read, and they do not write it the same way. By `R27` they
//! are different pipelines sharing the `pdf_extract` and diverging here, in the `text_filter`.
//!
//! | how the page ties the manager to its sub-funds | reader | measured on |
//! |---|---|---|
//! | **it does not tie it at all**: a name, and the residual is its own | [`Reader::Named`] | ASTERIA-EN23 p3 |
//! | the specification sits **below** the name, in brackets | [`Where::Below`] | MEDIOLANUM-IT24.A p5 · EURIZON-EN23 p9 · CARNE-EN23 p4 |
//! | the specification sits **above**, and covers the companies that follow | [`Where::Above`] | FINECO-EN23@LUX p6 |
//! | the sub-funds listed **first**, then the manager, then the sentence confirming it | [`Where::Above`] + [`Take::First`] + [`Spec::Listing`] | UBS-EN23 p4–5 |
//! | **on the line**, `<manager> for <sub-funds>` | [`Reader::ConnectorLine`] | FIDEURAM-EN23 p71 |
//! | **one row per sub-fund** in a table | [`Reader::Table`] | FINECO-EN23@IR p155 |
//!
//! The first three rows are **one reader**. The plan counted two — `…Adjacent` and `…Block` — and
//! measuring them showed that the difference between UBS-EN23 and FINECO-EN23@LUX is not the
//! structure but two parameters: *where* the specification sits with respect to the name, and *how
//! many* companies depend on it. It is worth saying because the second difference is the only one
//! that really counts: under FINECO-EN23@LUX's specification sit **three** companies, under
//! UBS-EN23's a single one, followed by a paragraph of prose in the same typeface as the name.
//!
//! # The group is the hidden parameter
//!
//! An entry of the page is not a line: it is a name, its address, and the specification. What
//! separates them is **vertical space**, which is geometry and therefore `R23` puts it in the
//! `pdf_extract`. The readers here get it from the metadata, under the `group_key` key, and **if it
//! is not there they treat every block as an entry of its own** — which is the right condition when
//! the `pdf_extract` has already separated name and specification into two block types, as
//! EURIZON-EN23 does.
//!
//! When the groups are there instead, a change of group **closes a specification left open**. It is
//! not a refinement: MEDIOLANUM-IT24.A p5 writes `(Mediolanum Global Impact` **with no closing
//! bracket**, and closing at the group recovers an assignment that would otherwise be lost.

use std::collections::BTreeMap;

use onig::Regex;

use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock};

use super::assignment::{Declared, block_text};

/// The note marks stripped off a sub-fund's name (`R16`, `U3`).
///
/// Fund identities compare after deep normalisation, but normalisation does **not** remove an
/// asterisk: `"Global High Yield*"` and `"Global High Yield"` are two different elements. The mark
/// has to come off here, while reading, or the sub-fund is not recognised.
pub const NOTE_MARKS: &str = "*†‡¹²³⁴⁵";

/// Failures of building a reader: every one of them is a malformed expression in the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ReadingError {
    #[error("the reader's '{field}' is not a valid expression: {message}")]
    InvalidPattern { field: &'static str, message: String },
}

fn compile(field: &'static str, pattern: &str) -> Result<Regex, ReadingError> {
    Regex::new(pattern)
        .map_err(|e| ReadingError::InvalidPattern { field, message: e.description().to_string() })
}

pub fn compile_all(field: &'static str, patterns: &[String]) -> Result<Vec<Regex>, ReadingError> {
    patterns.iter().map(|p| compile(field, p)).collect()
}

/// Where the specification sits with respect to the manager's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Where {
    #[default]
    Below,
    Above,
}

/// How many companies depend on a specification that precedes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Take {
    #[default]
    All,
    First,
}

/// The specification in brackets: a sentence that may wrap over several lines.
///
/// - `markers` — the phrase preceding the list, to be removed. They are tried **in order**, so the
///   longest goes first: FINECO-EN23@LUX writes both `(Only in respect of the Sub-Fund X)` and
///   `(Only in respect of A, B and C)`, and looking for the short one first would leave `the
///   Sub-Fund X` as a sub-fund's name;
/// - **if no marker matches, the specification names no sub-fund** and the declaration stays
///   *without funds*, that is it takes the residual. This is the first entry of EURIZON-EN23 p9,
///   where the bracketed sentence says *when* a delegate came in and not *what* it manages;
/// - `partial_markers` — the phrase saying "only part of the sub-fund". CARNE-EN23 p4: *«in charge
///   for the USD nominated securities in the subfund …»*. The manager enters for that fund, but the
///   fund **stays** in the residual (`R21`);
/// - `block_type` — when the `pdf_extract` has already separated the specification into a block type
///   of its own, recognition is by type and not by bracket;
/// - `umbrella` — the umbrella's name, when the bracket names the sub-fund **without** it. On
///   FINECO-EN23@LUX p6 the specification writes `Invesco Pan European High Income FAM Fund` while
///   the holdings write `CoRe Series Invesco Pan European High Income FAM Fund`: the same sub-fund
///   and not the same element, and `R24` wants the second. The umbrella is in every page's header,
///   so it is a constant of the format and not a reading.
#[derive(Debug, Clone)]
pub struct Parenthetical {
    pub markers: Vec<String>,
    pub partial_markers: Vec<String>,
    pub separators: Vec<String>,
    pub opener: String,
    pub closer: String,
    pub block_type: Option<BlockType>,
    pub umbrella: Option<String>,
    pub note_marks: String,
}

impl Default for Parenthetical {
    fn default() -> Self {
        Parenthetical {
            markers: Vec::new(),
            partial_markers: Vec::new(),
            separators: vec![",".to_string(), " and ".to_string()],
            opener: "(".to_string(),
            closer: ")".to_string(),
            block_type: None,
            umbrella: None,
            note_marks: NOTE_MARKS.to_string(),
        }
    }
}

/// The specification as a **list**, one sub-fund per line, under an umbrella's name.
///
/// UBS-EN23 p4–5 writes `BPER International SICAV` and underneath `– Open Selection Defence`, `–
/// Open Selection Growth`… The name needed is the two composed, and fund identities recognise it.
///
/// `umbrella` can be declared, and has to be when the umbrella's line **does not arrive among the
/// blocks**: on UBS-EN23 p4 it is in roman while the sub-funds are in italic, so a selection on the
/// italic leaves it out; on p5 it is in italic like them and reads itself.
///
/// For the umbrella's line to read itself, though, `block_type` is needed: without it the list is
/// recognised by its items' prefix, and a line not carrying that prefix does not enter the list at
/// all. The two roads cover the two pages, and neither covers the other.
///
/// `strip` is `umbrella`'s reverse: the heads and tails to **remove** from each item, one per
/// expression. EURIZON-EN21 p7–8 writes `Eurizon AM Sicav – USA Equity` while the holdings write
/// `USA EQUITY`, and `R24` wants the second; and it writes it in three spellings — the short dash,
/// the long one, `SICAV` in capitals — so a literal prefix is not enough.
///
/// `item_prefix = ""` says that **every line is an item**, which is the shape in which the
/// `pdf_extract` has already separated the list into a block type of its own.
#[derive(Debug)]
pub struct Listing {
    pub block_type: Option<BlockType>,
    pub item_prefix: String,
    pub umbrella: Option<String>,
    pub strip: Vec<Regex>,
    pub note_marks: String,
}

impl Default for Listing {
    fn default() -> Self {
        Listing {
            block_type: None,
            item_prefix: "–".to_string(),
            umbrella: None,
            strip: Vec::new(),
            note_marks: NOTE_MARKS.to_string(),
        }
    }
}

/// The two shapes a specification comes in.
#[derive(Debug)]
pub enum Spec {
    Parenthetical(Parenthetical),
    Listing(Listing),
}

/// What a closed specification carries: the sub-funds it names — `None` when it names none — and
/// whether it is partial.
type Specified = (Option<Vec<String>>, bool);

impl Spec {
    /// Whether this block opens a specification.
    fn opens(&self, block: &PdfBlock) -> bool {
        match self {
            Spec::Parenthetical(spec) => match &spec.block_type {
                Some(wanted) => block.type_block == *wanted,
                None => block_text(block).starts_with(&spec.opener),
            },
            Spec::Listing(spec) => match &spec.block_type {
                Some(wanted) => block.type_block == *wanted,
                None => block_text(block).starts_with(&spec.item_prefix),
            },
        }
    }

    /// Whether this block is still part of the specification started earlier.
    fn continues(&self, block: &PdfBlock, accumulated: &str) -> bool {
        match self {
            Spec::Parenthetical(spec) => match &spec.block_type {
                Some(wanted) => block.type_block == *wanted,
                None => {
                    count_of(accumulated, &spec.opener) > count_of(accumulated, &spec.closer)
                }
            },
            // A list goes on as long as the lines composing it go on.
            Spec::Listing(_) => self.opens(block),
        }
    }

    /// The specification read off its lines.
    fn read(&self, lines: &[String]) -> Specified {
        match self {
            Spec::Parenthetical(spec) => read_parenthetical(spec, lines),
            Spec::Listing(spec) => read_listing(spec, lines),
        }
    }

    /// The opener and closer of a bracketed specification, for the "never closed" diagnostic.
    fn brackets(&self) -> Option<(&str, &str)> {
        match self {
            Spec::Parenthetical(spec) => Some((&spec.opener, &spec.closer)),
            Spec::Listing(_) => None,
        }
    }
}

fn count_of(text: &str, needle: &str) -> usize {
    if needle.is_empty() { 0 } else { text.matches(needle).count() }
}

fn joined(lines: &[String]) -> String {
    lines.iter().map(|line| line.trim()).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ")
}

fn read_parenthetical(spec: &Parenthetical, lines: &[String]) -> Specified {
    let text = joined(lines);
    let partial = spec.partial_markers.iter().any(|marker| text.contains(marker.as_str()));
    let mut body = text.as_str();
    if let Some(rest) = body.strip_prefix(spec.opener.as_str()) {
        body = rest;
    }
    if let Some(position) = body.rfind(spec.closer.as_str()) {
        body = &body[..position];
    }
    let mut body = body.to_string();
    if !spec.markers.is_empty() {
        let found = spec.markers.iter().find_map(|marker| {
            body.find(marker.as_str()).map(|position| position + marker.len())
        });
        match found {
            Some(position) => body = body[position..].to_string(),
            None => {
                tracing::debug!(
                    "'{text}' names no sub-fund: the manager it belongs to takes the residual"
                );
                return (None, partial);
            }
        }
    }
    let names = split(&body, &spec.separators, &spec.note_marks);
    (Some(composed(names, spec.umbrella.as_deref())), partial)
}

fn read_listing(spec: &Listing, lines: &[String]) -> Specified {
    let mut umbrella = spec.umbrella.clone();
    let mut funds: Vec<String> = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !spec.item_prefix.is_empty() && !line.starts_with(&spec.item_prefix) {
            if spec.umbrella.is_none() {
                umbrella = Some(line.to_string());
            }
            continue;
        }
        let mut item = line[spec.item_prefix.len()..].trim().to_string();
        for pattern in &spec.strip {
            item = pattern.replace_all(&item, "").trim().to_string();
        }
        funds.push(match &umbrella {
            Some(umbrella) if !umbrella.is_empty() => format!("{umbrella} {item}"),
            _ => item,
        });
    }
    let text = funds.join("\n");
    (Some(split(&text, &["\n".to_string()], &spec.note_marks)), false)
}

/// The reader a format declares: one of the shapes a page ties a manager to its sub-funds in.
#[derive(Debug)]
pub enum Reader {
    /// The block **is** the declaration: a name, and no sub-fund said.
    ///
    /// It is the simplest shape a page has — ASTERIA-EN23 p3 writes `Investment Manager and Global
    /// Distributor:` and one name underneath — and the one where the work is all `R15`'s: a manager
    /// with no declared sub-funds takes the residual, which there is the whole document.
    ///
    /// It exists as a reader instead of leaving the caller to build the declarations by hand because
    /// this is where the things the caller would forget live: the note mark to strip, the case where
    /// the line names **two** of them (`separators`), and the two below.
    ///
    /// - `wrapped` — the expressions that say **the name goes on onto the next line**. On
    ///   DANSKEINVEST-EN24 p5–6 the trailing comma is the only sign of it: `Sumitomo Mitsui DS Asset
    ///   Management Company, / Limited` is one company name over two lines, and the comma **stays**,
    ///   because an output name is the one written. Only the lines of the same entry are joined, so
    ///   the page has to carry its groups for this to mean anything;
    /// - `retired` — the expressions qualifying the entry as **no longer in charge**, tried on
    ///   **every** line of it and not only on the name: the qualification sits under the name as
    ///   often as beside it, and here it is recognised, not selected (`R23`).
    ///
    /// Of each entry the name is the first line, plus whatever `wrapped` keeps attached to it; the
    /// rest is the address, which nobody asks for.
    Named {
        separators: Vec<String>,
        note_marks: String,
        group_key: String,
        wrapped: Vec<Regex>,
        retired: Vec<Regex>,
    },
    /// The specification sits **next to** the name — below, or above and valid for what follows.
    ///
    /// - `where_` — [`Where::Below`] is the common shape: the name opens the entry and the
    ///   specification closes it. [`Where::Above`] is the one where the specification opens a
    ///   *scope*, and the companies that follow fall inside it;
    /// - `take` — how many companies fall into the scope of an [`Where::Above`] specification.
    ///   [`Take::All`] up to the next specification (FINECO-EN23@LUX p6: one specification, **three**
    ///   companies), [`Take::First`] only the first (UBS-EN23, where after the name comes a paragraph
    ///   of prose in the same typeface);
    /// - `note_type` — the block type of the lines that **are not names**: the address, and the
    ///   temporal qualification. It is needed when the page carries no groups, because without groups
    ///   every block is an entry of its own and the first line to arrive becomes a name. Saying which
    ///   type carries the secondary lines is typography, that is `pdf_extract`; saying what they mean
    ///   is this reader (`R23`);
    /// - `retired` — the expressions qualifying an entry as **no longer in charge**. The manager
    ///   carrying them does not come out at all, and the sub-funds it claims return to the residual,
    ///   that is to whoever the document says manages them at closing. EURIZON-EN23 p10 writes
    ///   `Eurizon Capital Asia Limited` and underneath `(until 10th of May 2023)`, while p9 writes
    ///   that those two sub-funds *are managed by Eurizon Capital S.A. as from 10 May 2023*: the rule
    ///   is the management company's — **only whoever is in charge at the closing date is
    ///   extracted**, not the year's history. The qualifying line is any line of the entry, so the
    ///   `pdf_extract` has to carry it: here it is recognised, not selected (`R23`);
    /// - `strip` — the tails to remove from the name, one per expression. UBS-EN23 p4 writes `Banca
    ///   Cesare Ponti, Milano, Italy`, that is the company name **with the city inside**, and
    ///   `r",.*$"` brings it back to `Banca Cesare Ponti`. It is a parameter and not a rule:
    ///   FIDEURAM-EN23 p71 writes `Vontobel Asset Management S.A., Milan Branch`, which is a manager
    ///   with a comma inside, and the same expression would mutilate it (`R33`: an anchor holds
    ///   inside its format).
    ///
    /// Of every entry **the first line that is not a specification** is taken: the others are the
    /// address, which nobody asks for. An entry with no specification is a manager *with no declared
    /// funds*, that is the one [`super::assignment::assign`] gives the residual to (`R15`).
    Adjacent {
        spec: Spec,
        where_: Where,
        take: Take,
        group_key: String,
        strip: Vec<Regex>,
        note_type: Option<BlockType>,
        retired: Vec<Regex>,
    },
    /// `<manager> for <sub-funds>`: the line says everything, and a connector cuts it in two.
    ///
    /// FIDEURAM-EN23 p71 puts thirteen lines of this shape one under the other. The two sides split
    /// differently, and the reason is in the page: `Vontobel Asset Management S.A., Milan Branch` is
    /// **one** manager with a comma inside, while the list of sub-funds uses commas to separate. So
    /// the left side splits only on ` and `, the right side on the comma too.
    ///
    /// A manager can appear twice on different lines, and two managers can sit on the same line
    /// (`FIL Investments International and FIL (Luxembourg) S.A. for …`): each becomes a declaration,
    /// and `assign` merges them by name (`R22`).
    ///
    /// `after` is the part of the sentence preceding the name and not belonging to it. FIDEURAM opens
    /// the line with the manager and needs none; UBS-EN23 p4 writes a whole paragraph — *«The
    /// Management Company delegated to Nordea Investment Management AB the day to day portfolio
    /// management of BPER International SICAV – Low Duration European Covered Bond.»* — and without
    /// `after` the manager would be called by the whole sentence in front of it. A line the `after`
    /// does not carry is let fall, which is how the paragraphs that declare are told from those that
    /// explain.
    ConnectorLine {
        connector: String,
        after: String,
        manager_separators: Vec<String>,
        fund_separators: Vec<String>,
        terminators: String,
        group_key: String,
        note_marks: String,
    },
    /// One row per sub-fund: the manager sits in one column, the sub-fund in another.
    ///
    /// FINECO-EN23@IR p155 lists thirty-five sub-funds with their launch date and their manager, and
    /// the same manager comes back on many rows. The columns are discovered by the `pdf_extract`
    /// (`A2` with `rows`, which writes `table-row` and `table-col` into the metadata); here only
    /// **which** of the columns matter is said.
    Table {
        fund_col: i64,
        manager_col: i64,
        row_key: String,
        col_key: String,
        note_marks: String,
    },
}

impl Default for Reader {
    fn default() -> Self {
        Reader::Named {
            separators: Vec::new(),
            note_marks: NOTE_MARKS.to_string(),
            group_key: "group".to_string(),
            wrapped: Vec::new(),
            retired: Vec::new(),
        }
    }
}

impl Reader {
    /// The declarations read off the blocks, in the order the page writes them.
    pub fn read(&self, blocks: &[&PdfBlock]) -> Vec<Declared> {
        match self {
            Reader::Named { separators, note_marks, group_key, wrapped, retired } => {
                read_named(blocks, separators, note_marks, group_key, wrapped, retired)
            }
            Reader::Adjacent { .. } => self.read_adjacent(blocks),
            Reader::ConnectorLine { .. } => self.read_connector_line(blocks),
            Reader::Table { fund_col, manager_col, row_key, col_key, note_marks } => {
                read_table(blocks, *fund_col, *manager_col, row_key, col_key, note_marks)
            }
        }
    }

    fn read_adjacent(&self, blocks: &[&PdfBlock]) -> Vec<Declared> {
        let Reader::Adjacent { spec, where_, take, group_key, strip, note_type, retired } = self
        else {
            unreachable!("read_adjacent is only called on Adjacent")
        };
        let events = scan(blocks, spec, group_key);
        let names = |block: &PdfBlock| match note_type {
            None => true,
            Some(wanted) => block.type_block != *wanted,
        };
        match where_ {
            Where::Below => below(events, &names, strip, retired),
            Where::Above => above(events, &names, *take, strip, retired),
        }
    }

    fn read_connector_line(&self, blocks: &[&PdfBlock]) -> Vec<Declared> {
        let Reader::ConnectorLine {
            connector,
            after,
            manager_separators,
            fund_separators,
            terminators,
            group_key,
            note_marks,
        } = self
        else {
            unreachable!("read_connector_line is only called on ConnectorLine")
        };
        let mut declarations = Vec::new();
        for entry in entries(blocks, group_key) {
            let text = joined(&entry.iter().map(|b| block_text(b)).collect::<Vec<_>>());
            if !text.contains(connector.as_str()) {
                tracing::debug!("'{text}' carries no '{}': dropped", connector.trim());
                continue;
            }
            let mut text = text;
            if !after.is_empty() {
                match text.find(after.as_str()) {
                    Some(position) => text = text[position + after.len()..].to_string(),
                    None => {
                        tracing::debug!("'{text}' carries no '{}': dropped", after.trim());
                        continue;
                    }
                }
                if !text.contains(connector.as_str()) {
                    continue;
                }
            }
            let position = text.find(connector.as_str()).expect("the connector was just found");
            let (names, funds) = (&text[..position], &text[position + connector.len()..]);
            let funds =
                split(funds.trim().trim_end_matches(|c| terminators.contains(c)), fund_separators, note_marks);
            for name in split(names, manager_separators, "") {
                declarations.push(
                    Declared::from_block(renamed(entry[0], &name)).with_funds(funds.iter()),
                );
            }
        }
        declarations
    }
}

fn read_named(
    blocks: &[&PdfBlock],
    separators: &[String],
    note_marks: &str,
    group_key: &str,
    wrapped: &[Regex],
    retired: &[Regex],
) -> Vec<Declared> {
    let mut declarations = Vec::new();
    for entry in entries(blocks, group_key) {
        let lines: Vec<String> = entry.iter().map(|block| block_text(block)).collect();
        let Some(first) = lines.first() else { continue };
        let mut name = first.clone();
        let mut index = 1;
        while index < lines.len() && wrapped.iter().any(|p| p.find(&name).is_some()) {
            name = format!("{name} {}", lines[index]);
            index += 1;
        }
        let gone = lines.iter().any(|line| retired.iter().any(|p| p.find(line).is_some()));
        if gone {
            tracing::debug!("'{name}' is qualified as no longer in charge: not extracted");
        }
        for piece in split(&name, separators, note_marks) {
            declarations
                .push(Declared::from_block(renamed(entry[0], &piece)).with_retired(gone));
        }
    }
    declarations
}

fn read_table(
    blocks: &[&PdfBlock],
    fund_col: i64,
    manager_col: i64,
    row_key: &str,
    col_key: &str,
    note_marks: &str,
) -> Vec<Declared> {
    let mut rows: BTreeMap<i64, BTreeMap<i64, &PdfBlock>> = BTreeMap::new();
    for block in blocks {
        let (Some(row), Some(col)) = (cell_index(block, row_key), cell_index(block, col_key)) else {
            continue;
        };
        rows.entry(row).or_default().insert(col, block);
    }
    let mut order: Vec<String> = Vec::new();
    let mut funds: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut origin: BTreeMap<String, &PdfBlock> = BTreeMap::new();
    for cells in rows.values() {
        let (Some(fund), Some(manager)) = (cells.get(&fund_col), cells.get(&manager_col)) else {
            continue;
        };
        let name = block_text(manager);
        if !funds.contains_key(&name) {
            order.push(name.clone());
            funds.insert(name.clone(), Vec::new());
            origin.insert(name.clone(), manager);
        }
        let cleaned = clean(&block_text(fund), note_marks);
        if !cleaned.is_empty() {
            funds.get_mut(&name).expect("just inserted").push(cleaned);
        }
    }
    order
        .into_iter()
        .map(|name| {
            let block = (*origin.get(&name).expect("every name has an origin")).clone();
            Declared::from_block(block).with_funds(&funds[&name])
        })
        .collect()
}

/// The block's cell index under `key`, which the tabularizer writes as an integer.
fn cell_index(block: &PdfBlock, key: &str) -> Option<i64> {
    match block.metadata.get(key)? {
        BlockValue::Int(found) => Some(*found),
        BlockValue::Str(found) => found.parse().ok(),
        _ => None,
    }
}

/// The same block, with **only the manager's name** as content.
///
/// It is public because a format that rebuilds the name needs it: DANSKEINVEST-EN24 writes `Sumitomo
/// Mitsui DS Asset Management Company, / Limited` over two lines, and
/// [`super::blocks::text_blocks`] writes the **block's content** as the name, not the string the
/// declaration carries.
///
/// The block is kept so that the output row knows which page it comes from, and the content is what
/// writes the name: on a `<manager> for <sub-funds>` line, keeping it whole would mean calling the
/// manager by its whole list of sub-funds. The other readers do not need it because there the line
/// **is** the name.
pub fn renamed(block: &PdfBlock, name: &str) -> PdfBlock {
    PdfBlock::new(block.type_block.clone(), block.metadata.clone(), name)
}

/// One of the three things a scanned block turns out to be.
enum Event<'a> {
    /// An entry of the page begins: a new name may open here.
    Entry,
    /// A specification has closed: it carries the sub-funds it names.
    Spec(Specified),
    /// A block that is not a specification — the name, or the address following it.
    Text(&'a PdfBlock),
}

/// The page's blocks read as events: entry, specification, text.
///
/// A specification **crosses** entry boundaries when the page carries no groups, and closes on a
/// boundary when it does: without the groups a bracketed sentence that wraps would be broken into
/// three pieces (EURIZON-EN23 p9 has one thirty-one lines long), with the groups a bracket never
/// closed would stay open for the rest of the page (MEDIOLANUM-IT24.A p5).
fn scan<'a>(blocks: &[&'a PdfBlock], spec: &Spec, group_key: &str) -> Vec<Event<'a>> {
    let grouped = blocks.iter().any(|block| group_of(block, group_key).is_some());
    let mut events = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    // `None` here is "no group seen yet", which is not the same as "the group is absent": the first
    // block always opens an entry.
    let mut previous: Option<Option<String>> = None;
    for block in blocks {
        let group = group_of(block, group_key);
        if !grouped || previous.is_none() || previous.as_ref() != Some(&group) {
            previous = Some(group);
            if !lines.is_empty() && grouped {
                events.push(Event::Spec(closed(spec, &lines)));
                lines.clear();
            }
            events.push(Event::Entry);
        }
        if !lines.is_empty() && spec.continues(block, &lines.join(" ")) {
            lines.push(block_text(block));
            continue;
        }
        if !lines.is_empty() {
            events.push(Event::Spec(closed(spec, &lines)));
            lines.clear();
        }
        if spec.opens(block) {
            lines.push(block_text(block));
            continue;
        }
        events.push(Event::Text(block));
    }
    if !lines.is_empty() {
        events.push(Event::Spec(closed(spec, &lines)));
    }
    events
}

/// The specification read, saying so if the page never closed it.
fn closed(spec: &Spec, lines: &[String]) -> Specified {
    if let Some((opener, closer)) = spec.brackets() {
        let text = lines.join(" ");
        if count_of(&text, opener) > count_of(&text, closer) {
            tracing::debug!("'{text}' is never closed: read as it stands, to the end of its entry");
        }
    }
    spec.read(lines)
}

/// The name opens the entry, the specification closes it and attaches to it.
fn below(
    events: Vec<Event<'_>>,
    names: &dyn Fn(&PdfBlock) -> bool,
    strip: &[Regex],
    retired: &[Regex],
) -> Vec<Declared> {
    let mut declarations: Vec<Declared> = Vec::new();
    let mut named = false;
    for event in events {
        match event {
            Event::Entry => named = false,
            Event::Spec(payload) => match declarations.last_mut() {
                Some(last) => merge(last, payload),
                None => tracing::debug!(
                    "a specification opens the section before any manager is named"
                ),
            },
            Event::Text(block) if !named && names(block) => {
                named = true;
                declarations.push(declared(block, None, strip, retired));
            }
            Event::Text(block) => retire(&mut declarations, block, retired),
        }
    }
    declarations
}

/// The specification opens a scope, and the companies that follow fall inside it.
///
/// A specification that closes **reopens** the possibility of a name, even if the entry had already
/// been named. Without this, UBS-EN23 p4 loses `UBS Asset Management Switzerland AG`: the umbrella's
/// line, the list of five sub-funds and the manager's name all three sit in the same paragraph, the
/// umbrella consumes the name's place and the manager arrives at an entry already named. The scope is
/// opened by the specification, not by the typographic gap.
fn above(
    events: Vec<Event<'_>>,
    names: &dyn Fn(&PdfBlock) -> bool,
    take: Take,
    strip: &[Regex],
    retired: &[Regex],
) -> Vec<Declared> {
    let mut declarations: Vec<Declared> = Vec::new();
    let mut pending: Option<Specified> = None;
    let mut taken = 0usize;
    let mut named = false;
    for event in events {
        match event {
            Event::Entry => named = false,
            Event::Spec(payload) => {
                pending = Some(payload);
                taken = 0;
                named = false;
            }
            Event::Text(block) if !named && names(block) => {
                named = true;
                match &pending {
                    None => tracing::debug!(
                        "'{}' is named before any specification: dropped",
                        block_text(block)
                    ),
                    Some(payload) if take == Take::All || taken == 0 => {
                        declarations.push(declared(block, Some(payload.clone()), strip, retired));
                        taken += 1;
                    }
                    Some(_) => {}
                }
            }
            Event::Text(block) => retire(&mut declarations, block, retired),
        }
    }
    declarations
}

/// Marks the last open declaration as no longer in charge, if the line qualifies it.
///
/// It is **marked** instead of removed for two reasons. The first is that the specification arrives
/// after the qualifying line, and removing the declaration at once would attach it to the previous
/// manager: EURIZON-EN23 p10 puts the `(until …)` between the name and the bracket naming the
/// sub-funds. The second is that the sub-funds named are still needed — `assign` keeps them among
/// the document's sub-funds and puts them back into the residual.
fn retire(declarations: &mut [Declared], block: &PdfBlock, retired: &[Regex]) {
    if retired.is_empty() {
        return;
    }
    let Some(last) = declarations.last_mut() else { return };
    let text = block_text(block);
    if retired.iter().any(|pattern| pattern.find(&text).is_some()) {
        last.retired = true;
        tracing::debug!("'{text}' qualifies the entry as no longer in charge: not extracted");
    }
}

/// A declaration from the name's block and the specification that falls to it.
///
/// `retired` is tried **on the name too**, because the temporal qualification does not always sit on
/// a line of its own: EURIZON-EN23 p10 writes it under the name — and there `retire` finds it —
/// while EURIZON-EN21 p7 writes it at the end of the name, `Pramerica SGR S.p.A. (until June 30,
/// 2021)`. It is tried on the text **before** `strip`, because it is `strip` that takes that tail
/// off the output name.
fn declared(
    block: &PdfBlock,
    spec: Option<Specified>,
    strip: &[Regex],
    retired: &[Regex],
) -> Declared {
    let (funds, partial) = match spec {
        None => (None, false),
        Some((funds, partial)) => (funds, partial),
    };
    let name = block_text(block);
    let gone = retired.iter().any(|pattern| pattern.find(&name).is_some());
    if gone {
        tracing::debug!("'{name}' names itself as no longer in charge: not extracted");
    }
    Declared::from_block(stripped(block, strip))
        .with_fund_set(funds.map(super::assignment::fund_set))
        .with_partial(partial)
        .with_retired(gone)
}

/// The name's block without the tails the format declares it does not want.
///
/// The block is rewritten rather than the name later on because it is the block that travels to the
/// output row: [`super::blocks::text_blocks`] writes its content as the name.
fn stripped(block: &PdfBlock, strip: &[Regex]) -> PdfBlock {
    if strip.is_empty() {
        return (*block).clone();
    }
    let mut name = block_text(block);
    for pattern in strip {
        name = pattern.replace_all(&name, "").trim().to_string();
    }
    if name.is_empty() { (*block).clone() } else { renamed(block, &name) }
}

/// The specification that arrived afterwards, attached to the declaration waiting for it.
fn merge(declaration: &mut Declared, spec: Specified) {
    let (funds, partial) = spec;
    let Some(names) = funds else { return };
    let arriving = super::assignment::fund_set(names);
    match declaration.funds.take() {
        None => {
            declaration.funds = Some(arriving);
            declaration.partial = partial;
        }
        Some(before) => {
            declaration.funds = Some(before.union(&arriving).cloned().collect());
            declaration.partial = declaration.partial && partial;
        }
    }
}

/// The names with the umbrella in front, or as they were if the format declares none.
fn composed(names: Vec<String>, umbrella: Option<&str>) -> Vec<String> {
    match umbrella {
        None => names,
        Some("") => names,
        Some(umbrella) => names.into_iter().map(|name| format!("{umbrella} {name}")).collect(),
    }
}

/// The blocks split into entries: by group if the metadata says so, one by one otherwise.
///
/// Without the group key every block is an entry of its own, which is the right reading when the
/// `pdf_extract` has already separated by type what the geometry would separate.
fn entries<'a>(blocks: &[&'a PdfBlock], group_key: &str) -> Vec<Vec<&'a PdfBlock>> {
    let mut entries: Vec<Vec<&'a PdfBlock>> = Vec::new();
    let mut previous: Option<String> = None;
    for block in blocks {
        let group = group_of(block, group_key);
        if group.is_none() || previous.is_none() || group != previous {
            entries.push(Vec::new());
        }
        entries.last_mut().expect("an entry was just opened").push(block);
        previous = group;
    }
    entries
}

/// The block's group, or `None` if the `pdf_extract` put none there.
///
/// Read as a **string** whatever the metadata's type: the group is only ever compared with the
/// previous one, and an integer and its spelling compare the same way once both are strings.
fn group_of(block: &PdfBlock, group_key: &str) -> Option<String> {
    match block.metadata.get(group_key)? {
        BlockValue::Int(found) => Some(found.to_string()),
        BlockValue::Str(found) => Some(found.clone()),
        BlockValue::Float(found) => Some(found.to_string()),
        _ => None,
    }
}

/// The text split on every separator, cleaned, with no empty pieces.
fn split(text: &str, separators: &[String], note_marks: &str) -> Vec<String> {
    let mut parts: Vec<String> = vec![text.to_string()];
    for separator in separators {
        if separator.is_empty() {
            continue;
        }
        parts = parts
            .iter()
            .flat_map(|part| part.split(separator.as_str()).map(str::to_string))
            .collect();
    }
    parts.iter().map(|part| clean(part, note_marks)).filter(|name| !name.is_empty()).collect()
}

/// A name without spare spaces and without the note mark trailing it (`R16`).
fn clean(text: &str, note_marks: &str) -> String {
    if note_marks.is_empty() {
        return text.trim().to_string();
    }
    text.trim().trim_end_matches(|c| note_marks.contains(c)).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::value::BlockValue;
    use std::collections::BTreeMap;

    const MANAGER: BlockType = BlockType::INVESTMENTS_MANAGER;

    /// A block of the managers' type, with no metadata: one entry of its own.
    fn block(text: &str) -> PdfBlock {
        PdfBlock::bare(MANAGER, text)
    }

    /// A block carrying the entry it belongs to, as a `pdf_extract` that groups writes it.
    fn grouped(group: i64, text: &str) -> PdfBlock {
        PdfBlock::new(
            MANAGER,
            BTreeMap::from([("group".to_string(), BlockValue::Int(group))]),
            text,
        )
    }

    fn typed(type_block: &str, text: &str) -> PdfBlock {
        PdfBlock::bare(BlockType::from(type_block.to_string()), text)
    }

    fn typed_grouped(type_block: &str, group: i64, text: &str) -> PdfBlock {
        PdfBlock::new(
            BlockType::from(type_block.to_string()),
            BTreeMap::from([("group".to_string(), BlockValue::Int(group))]),
            text,
        )
    }

    fn cell(row: i64, col: i64, text: &str) -> PdfBlock {
        PdfBlock::new(
            MANAGER,
            BTreeMap::from([
                ("table-row".to_string(), BlockValue::Int(row)),
                ("table-col".to_string(), BlockValue::Int(col)),
            ]),
            text,
        )
    }

    fn read(reader: &Reader, blocks: &[PdfBlock]) -> Vec<Declared> {
        reader.read(&blocks.iter().collect::<Vec<_>>())
    }

    /// `(name, funds-or-None, partial, retired)` of each declaration, which is everything a
    /// declaration says.
    fn said(declarations: &[Declared]) -> Vec<(String, Option<Vec<String>>, bool, bool)> {
        declarations
            .iter()
            .map(|d| {
                (
                    d.name.clone(),
                    d.funds
                        .as_ref()
                        .map(|own| own.iter().map(|f| f.name().to_string()).collect()),
                    d.partial,
                    d.retired,
                )
            })
            .collect()
    }

    fn names(declarations: &[Declared]) -> Vec<&str> {
        declarations.iter().map(|d| d.name.as_str()).collect()
    }

    fn funds_of(declarations: &[Declared], name: &str) -> Option<Vec<String>> {
        declarations
            .iter()
            .find(|d| d.name == name)
            .and_then(|d| d.funds.as_ref())
            .map(|own| own.iter().map(|f| f.name().to_string()).collect())
    }

    /// A `Named` reader with the given separators and every other field at its default.
    fn named(separators: &[&str]) -> Reader {
        Reader::Named {
            separators: separators.iter().map(|s| s.to_string()).collect(),
            note_marks: NOTE_MARKS.to_string(),
            group_key: "group".to_string(),
            wrapped: Vec::new(),
            retired: Vec::new(),
        }
    }

    fn parenthetical(markers: &[&str]) -> Parenthetical {
        Parenthetical {
            markers: markers.iter().map(|m| m.to_string()).collect(),
            ..Parenthetical::default()
        }
    }

    fn adjacent(spec: Spec) -> Reader {
        Reader::Adjacent {
            spec,
            where_: Where::Below,
            take: Take::All,
            group_key: "group".to_string(),
            strip: Vec::new(),
            note_type: None,
            retired: Vec::new(),
        }
    }

    fn connector_line() -> Reader {
        Reader::ConnectorLine {
            connector: " for ".to_string(),
            after: String::new(),
            manager_separators: vec![" and ".to_string()],
            fund_separators: vec![",".to_string(), " and ".to_string()],
            terminators: ",.;".to_string(),
            group_key: "group".to_string(),
            note_marks: NOTE_MARKS.to_string(),
        }
    }

    mod the_block_is_the_declaration {
        use super::*;

        #[test]
        fn one_name_one_declaration_with_no_funds_said() {
            // ASTERIA-EN23 p3: a label, one name underneath, and `R15` does the rest.
            let got = read(&Reader::default(), &[block("Asteria Investment Managers SA")]);
            assert_eq!(said(&got), [("Asteria Investment Managers SA".to_string(), None, false, false)]);
        }

        #[test]
        fn the_note_mark_comes_off_the_name() {
            let got = read(&Reader::default(), &[block("Acme AM*")]);
            assert_eq!(names(&got), ["Acme AM"]);
        }

        #[test]
        fn a_line_naming_two_companies_gives_two_declarations() {
            let reader = named(&[" and "]);
            let got = read(&reader, &[block("One AM and Two AM")]);
            assert_eq!(names(&got), ["One AM", "Two AM"]);
        }

        #[test]
        fn the_block_is_kept_and_carries_only_the_name() {
            let reader = named(&[" and "]);
            let got = read(&reader, &[block("One AM and Two AM")]);
            let kept = got[0].block.as_ref().expect("the block is kept");
            assert_eq!(kept.content.as_str(), Some("One AM"));
            assert_eq!(kept.type_block, MANAGER);
        }
    }

    mod a_name_that_wraps_over_two_lines {
        use super::*;

        /// The reader DANSKEINVEST-EN24 declares: the trailing comma is the only sign that the
        /// company name goes on.
        fn wrapping() -> Reader {
            Reader::Named {
                separators: Vec::new(),
                note_marks: NOTE_MARKS.to_string(),
                group_key: "group".to_string(),
                wrapped: compile_all("wrapped", &[r",\s*$".to_string()])
                    .expect("a literal pattern compiles"),
                retired: compile_all("retired", &[r"(?i)\(\s*until\b".to_string()])
                    .expect("a literal pattern compiles"),
            }
        }

        #[test]
        fn the_trailing_comma_keeps_the_next_line_attached() {
            let blocks = [
                grouped(0, "Sumitomo Mitsui DS Asset Management Company,"),
                grouped(0, "Limited"),
                grouped(0, "1-1 Marunouchi, Chiyoda-ku"),
            ];
            let got = read(&wrapping(), &blocks);
            // The comma **stays**: an output name is the one the document writes.
            assert_eq!(names(&got), ["Sumitomo Mitsui DS Asset Management Company, Limited"]);
        }

        #[test]
        fn a_name_that_does_not_wrap_takes_only_its_own_line() {
            let blocks = [grouped(0, "Danske Bank A/S"), grouped(0, "Holmens Kanal 2-12")];
            let got = read(&wrapping(), &blocks);
            assert_eq!(names(&got), ["Danske Bank A/S"]);
        }

        #[test]
        fn the_wrapping_stops_at_the_end_of_the_entry() {
            let blocks = [grouped(0, "Acme Asset Management,"), grouped(1, "Other AM")];
            let got = read(&wrapping(), &blocks);
            assert_eq!(names(&got), ["Acme Asset Management,", "Other AM"]);
        }

        #[test]
        fn the_block_carries_the_rebuilt_name() {
            let blocks = [grouped(0, "Acme Asset Management,"), grouped(0, "Limited")];
            let got = read(&wrapping(), &blocks);
            let kept = got[0].block.as_ref().expect("the block is kept");
            assert_eq!(kept.content.as_str(), Some("Acme Asset Management, Limited"));
        }

        #[test]
        fn a_qualification_on_any_line_of_the_entry_retires_it() {
            let blocks = [grouped(0, "Acme Asset Management"), grouped(0, "(until 30 June 2024)")];
            let got = read(&wrapping(), &blocks);
            assert_eq!(said(&got), [("Acme Asset Management".to_string(), None, false, true)]);
        }

        #[test]
        fn an_entry_whose_lines_carry_no_qualification_is_in_charge() {
            let blocks = [grouped(0, "Acme Asset Management"), grouped(0, "25 Some Street")];
            let got = read(&wrapping(), &blocks);
            assert_eq!(said(&got), [("Acme Asset Management".to_string(), None, false, false)]);
        }

        #[test]
        fn without_groups_every_block_is_its_own_name_as_before() {
            let blocks = [block("One AM,"), block("Two AM")];
            let got = read(&wrapping(), &blocks);
            assert_eq!(names(&got), ["One AM,", "Two AM"]);
        }
    }

    mod the_specification_below_the_name {
        use super::*;

        #[test]
        fn the_bracket_under_the_name_names_its_sub_funds() {
            // MEDIOLANUM-IT24.A p5's shape: a name, then the bracket that qualifies it.
            let blocks = [
                grouped(0, "Acme Asset Management"),
                grouped(0, "(in respect of Alpha Fund and Beta Fund)"),
            ];
            let got = read(&adjacent(Spec::Parenthetical(parenthetical(&["in respect of "]))), &blocks);
            assert_eq!(names(&got), ["Acme Asset Management"]);
            assert_eq!(funds_of(&got, "Acme Asset Management"), Some(vec!["Alpha Fund".to_string(), "Beta Fund".to_string()]));
        }

        #[test]
        fn a_bracket_wrapping_over_lines_is_one_specification() {
            // EURIZON-EN23 p9 has one thirty-one lines long.
            let blocks = [
                grouped(0, "Acme Asset Management"),
                grouped(0, "(in respect of Alpha Fund,"),
                grouped(0, "Beta Fund)"),
            ];
            let got = read(&adjacent(Spec::Parenthetical(parenthetical(&["in respect of "]))), &blocks);
            assert_eq!(funds_of(&got, "Acme Asset Management"), Some(vec!["Alpha Fund".to_string(), "Beta Fund".to_string()]));
        }

        #[test]
        fn a_bracket_the_page_never_closes_is_read_to_the_end_of_its_entry() {
            // MEDIOLANUM-IT24.A p5 writes `(Mediolanum Global Impact` with no closing bracket, and
            // the change of group is what closes it.
            let blocks = [
                grouped(0, "Acme Asset Management"),
                grouped(0, "(in respect of Alpha Fund"),
                grouped(1, "Other AM"),
            ];
            let got = read(&adjacent(Spec::Parenthetical(parenthetical(&["in respect of "]))), &blocks);
            assert_eq!(funds_of(&got, "Acme Asset Management"), Some(vec!["Alpha Fund".to_string()]));
            assert_eq!(names(&got), ["Acme Asset Management", "Other AM"]);
        }

        #[test]
        fn a_bracket_naming_no_sub_fund_leaves_the_manager_on_the_residual() {
            // The first entry of EURIZON-EN23 p9: the bracket says *when*, not *what*.
            let blocks = [
                grouped(0, "Acme Asset Management"),
                grouped(0, "(as from 10 May 2023)"),
            ];
            let got = read(&adjacent(Spec::Parenthetical(parenthetical(&["in respect of "]))), &blocks);
            assert_eq!(said(&got), [("Acme Asset Management".to_string(), None, false, false)]);
        }

        #[test]
        fn the_longest_marker_is_tried_first() {
            let spec = parenthetical(&["Only in respect of the Sub-Fund ", "Only in respect of "]);
            let blocks = [
                grouped(0, "Acme AM"),
                grouped(0, "(Only in respect of the Sub-Fund Alpha Fund)"),
            ];
            let got = read(&adjacent(Spec::Parenthetical(spec)), &blocks);
            assert_eq!(funds_of(&got, "Acme AM"), Some(vec!["Alpha Fund".to_string()]));
        }

        #[test]
        fn a_partial_marker_marks_the_declaration_partial() {
            // CARNE-EN23 p4: in charge for the dollar-denominated part of a sub-fund.
            let spec = Parenthetical {
                markers: vec!["in the subfund ".to_string()],
                partial_markers: vec!["in charge for ".to_string()],
                ..Parenthetical::default()
            };
            let blocks = [
                grouped(0, "Acme AM"),
                grouped(0, "(in charge for the USD nominated securities in the subfund Alpha Fund)"),
            ];
            let got = read(&adjacent(Spec::Parenthetical(spec)), &blocks);
            assert_eq!(said(&got), [("Acme AM".to_string(), Some(vec!["Alpha Fund".to_string()]), true, false)]);
        }

        #[test]
        fn only_the_first_line_of_an_entry_is_a_name_and_the_rest_is_the_address() {
            let blocks = [
                grouped(0, "Acme Asset Management"),
                grouped(0, "4th Floor, 25 Some Street"),
                grouped(0, "Dublin 2, Ireland"),
            ];
            let got = read(&adjacent(Spec::Parenthetical(parenthetical(&["in respect of "]))), &blocks);
            assert_eq!(names(&got), ["Acme Asset Management"]);
        }

        #[test]
        fn the_specification_can_be_a_block_type_of_its_own() {
            // EURIZON-EN23's `pdf_extract` separates name and bracket, and then there are no groups.
            let spec = Parenthetical {
                markers: vec!["in respect of ".to_string()],
                block_type: Some(BlockType::from("SPEC".to_string())),
                ..Parenthetical::default()
            };
            let blocks = [typed("MANAGER", "Acme AM"), typed("SPEC", "(in respect of Alpha Fund)")];
            let reader = Reader::Adjacent {
                spec: Spec::Parenthetical(spec),
                where_: Where::Below,
                take: Take::All,
                group_key: "group".to_string(),
                strip: Vec::new(),
                note_type: None,
                retired: Vec::new(),
            };
            let got = read(&reader, &blocks);
            assert_eq!(funds_of(&got, "Acme AM"), Some(vec!["Alpha Fund".to_string()]));
        }

        #[test]
        fn a_specification_before_any_name_is_dropped() {
            let blocks = [grouped(0, "(in respect of Alpha Fund)")];
            let got = read(&adjacent(Spec::Parenthetical(parenthetical(&["in respect of "]))), &blocks);
            assert!(got.is_empty(), "{:?}", names(&got));
        }

        #[test]
        fn an_umbrella_is_put_in_front_of_every_name_the_bracket_gives() {
            // FINECO-EN23@LUX p6: the bracket names the sub-fund without the umbrella.
            let spec = Parenthetical {
                markers: vec!["Only in respect of ".to_string()],
                umbrella: Some("CoRe Series".to_string()),
                ..Parenthetical::default()
            };
            let blocks = [grouped(0, "Acme AM"), grouped(0, "(Only in respect of Alpha Fund)")];
            let got = read(&adjacent(Spec::Parenthetical(spec)), &blocks);
            assert_eq!(funds_of(&got, "Acme AM"), Some(vec!["CoRe Series Alpha Fund".to_string()]));
        }
    }

    mod the_specification_above_the_names {
        use super::*;

        fn above(spec: Spec, take: Take) -> Reader {
            Reader::Adjacent {
                spec,
                where_: Where::Above,
                take,
                group_key: "group".to_string(),
                strip: Vec::new(),
                note_type: None,
                retired: Vec::new(),
            }
        }

        #[test]
        fn every_company_following_the_specification_falls_inside_it() {
            // FINECO-EN23@LUX p6: one specification, three companies.
            let spec = parenthetical(&["Only in respect of the Sub-Fund "]);
            let blocks = [
                block("(Only in respect of the Sub-Fund Alpha Fund)"),
                block("One AM"),
                block("Two AM"),
                block("Three AM"),
            ];
            let got = read(&above(Spec::Parenthetical(spec), Take::All), &blocks);
            assert_eq!(names(&got), ["One AM", "Two AM", "Three AM"]);
            assert_eq!(funds_of(&got, "Three AM"), Some(vec!["Alpha Fund".to_string()]));
        }

        #[test]
        fn take_first_keeps_only_the_company_right_after_it() {
            // UBS-EN23: after the name comes a paragraph of prose in the same typeface.
            let spec = parenthetical(&["Only in respect of the Sub-Fund "]);
            let blocks = [
                block("(Only in respect of the Sub-Fund Alpha Fund)"),
                block("One AM"),
                block("The Management Company explains something here."),
            ];
            let got = read(&above(Spec::Parenthetical(spec), Take::First), &blocks);
            assert_eq!(names(&got), ["One AM"]);
        }

        #[test]
        fn a_company_named_before_any_specification_is_dropped() {
            let spec = parenthetical(&["Only in respect of "]);
            let blocks = [block("Early AM"), block("(Only in respect of Alpha Fund)"), block("One AM")];
            let got = read(&above(Spec::Parenthetical(spec), Take::All), &blocks);
            assert_eq!(names(&got), ["One AM"]);
        }

        #[test]
        fn an_umbrella_line_inside_the_list_reads_itself() {
            // UBS-EN23 p5: the umbrella is in italic like the sub-funds, so it arrives among the
            // list's own blocks — which takes `block_type`, because a line without the item prefix
            // would not enter a list recognised by that prefix.
            let listing = Listing {
                block_type: Some(BlockType::from("ITALIC".to_string())),
                ..Listing::default()
            };
            let blocks = [
                typed("ITALIC", "BPER International SICAV"),
                typed("ITALIC", "– Open Selection Defence"),
                typed("ITALIC", "– Open Selection Growth"),
                typed("MANAGER", "UBS Asset Management Switzerland AG"),
            ];
            let got = read(&above(Spec::Listing(listing), Take::First), &blocks);
            assert_eq!(names(&got), ["UBS Asset Management Switzerland AG"]);
            assert_eq!(
                funds_of(&got, "UBS Asset Management Switzerland AG"),
                Some(vec![
                    "BPER International SICAV Open Selection Defence".to_string(),
                    "BPER International SICAV Open Selection Growth".to_string(),
                ])
            );
        }

        #[test]
        fn a_declared_umbrella_covers_a_list_whose_own_line_never_arrives() {
            // UBS-EN23 p4: the umbrella is in roman while the sub-funds are in italic.
            let listing =
                Listing { umbrella: Some("BPER International SICAV".to_string()), ..Listing::default() };
            let blocks = [block("– Open Selection Defence"), block("One AM")];
            let got = read(&above(Spec::Listing(listing), Take::First), &blocks);
            // The item prefix comes off before the umbrella goes in front of it.
            assert_eq!(
                funds_of(&got, "One AM"),
                Some(vec!["BPER International SICAV Open Selection Defence".to_string()])
            );
        }

        #[test]
        fn a_specification_that_closes_reopens_the_place_of_a_name() {
            // Without this, UBS-EN23 p4 loses the manager: the umbrella's line, the list and the
            // name all three sit in the same paragraph, and the umbrella eats the name's place.
            let listing = Listing::default();
            let blocks = [
                grouped(0, "BPER International SICAV"),
                grouped(0, "– Open Selection Defence"),
                grouped(0, "UBS Asset Management Switzerland AG"),
            ];
            let got = read(&above(Spec::Listing(listing), Take::First), &blocks);
            assert_eq!(names(&got), ["UBS Asset Management Switzerland AG"]);
        }

        #[test]
        fn every_line_is_an_item_when_the_prefix_is_empty() {
            // EURIZON-EN21: the `pdf_extract` has already separated the list into its own type.
            let listing = Listing {
                block_type: Some(BlockType::from("FUND".to_string())),
                item_prefix: String::new(),
                strip: compile_all("strip", &[r"(?i)^Eurizon AM Sicav\s*[-–]\s*".to_string()])
                    .expect("a literal pattern compiles"),
                ..Listing::default()
            };
            let blocks = [
                typed_grouped("FUND", 0, "Eurizon AM Sicav – USA Equity"),
                typed_grouped("MANAGER", 0, "One AM"),
            ];
            let reader = Reader::Adjacent {
                spec: Spec::Listing(listing),
                where_: Where::Above,
                take: Take::All,
                group_key: "group".to_string(),
                strip: Vec::new(),
                note_type: Some(BlockType::from("NOTE".to_string())),
                retired: Vec::new(),
            };
            let got = read(&reader, &blocks);
            assert_eq!(funds_of(&got, "One AM"), Some(vec!["USA Equity".to_string()]));
        }
    }

    mod the_manager_no_longer_in_charge {
        use super::*;

        /// The reader EURIZON-EN23 declares: the specification is a block type of its own, so a
        /// bracketed line that is **not** of that type stays a note.
        fn with_retired(retired: &[&str], strip: &[&str], note_type: Option<&str>) -> Reader {
            let spec = Parenthetical {
                markers: vec!["in respect of ".to_string()],
                block_type: Some(BlockType::from("SUB".to_string())),
                ..Parenthetical::default()
            };
            Reader::Adjacent {
                spec: Spec::Parenthetical(spec),
                where_: Where::Below,
                take: Take::All,
                group_key: "group".to_string(),
                strip: compile_all("strip", &strip.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                    .expect("literal patterns compile"),
                note_type: note_type.map(|t| BlockType::from(t.to_string())),
                retired: compile_all(
                    "retired",
                    &retired.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                )
                .expect("literal patterns compile"),
            }
        }

        #[test]
        fn a_qualification_on_a_line_of_its_own_retires_the_entry() {
            // EURIZON-EN23 p10 puts the `(until …)` between the name and the bracket.
            let blocks = [
                typed("MANAGER", "Eurizon Capital Asia Limited"),
                typed("NOTE", "(until 10th of May 2023)"),
                typed("SUB", "(in respect of Alpha Fund)"),
            ];
            let got = read(&with_retired(&[r"^\(until\b"], &[], Some("NOTE")), &blocks);
            assert_eq!(
                said(&got),
                [(
                    "Eurizon Capital Asia Limited".to_string(),
                    Some(vec!["Alpha Fund".to_string()]),
                    false,
                    true
                )]
            );
        }

        #[test]
        fn a_qualification_at_the_end_of_the_name_retires_it_too() {
            // EURIZON-EN21 p7: `Pramerica SGR S.p.A. (until June 30, 2021)`.
            let blocks = [typed("MANAGER", "Pramerica SGR S.p.A. (until June 30, 2021)")];
            let got = read(
                &with_retired(&[r"\(until\b"], &[r"\s*\((?:since|until)\b[^)]*\)?\s*$"], None),
                &blocks,
            );
            // Recognised **before** `strip`, which is what takes the tail off the output name.
            assert_eq!(said(&got), [("Pramerica SGR S.p.A.".to_string(), None, false, true)]);
        }

        #[test]
        fn a_manager_still_in_charge_is_not_retired() {
            let blocks = [typed("MANAGER", "Eurizon Capital SGR S.p.A. (since July 1, 2021)")];
            let got = read(
                &with_retired(&[r"\(until\b"], &[r"\s*\((?:since|until)\b[^)]*\)?\s*$"], None),
                &blocks,
            );
            assert_eq!(said(&got), [("Eurizon Capital SGR S.p.A.".to_string(), None, false, false)]);
        }

        #[test]
        fn a_note_typed_block_is_never_taken_for_a_name() {
            // Without groups every block is an entry of its own, so the address would become a name.
            let blocks = [typed("MANAGER", "One AM"), typed("NOTE", "(until June 30, 2021)")];
            let got = read(&with_retired(&[r"\(until\b"], &[], Some("NOTE")), &blocks);
            assert_eq!(said(&got), [("One AM".to_string(), None, false, true)]);
        }

        #[test]
        fn a_name_strip_that_empties_the_name_leaves_the_block_alone() {
            // Stripping everything would give a manager with no name at all, which is worse than a
            // manager whose name still carries its tail.
            let blocks = [typed("MANAGER", "Pramerica SGR S.p.A.")];
            let got = read(&with_retired(&[], &[r"^.*$"], None), &blocks);
            assert_eq!(names(&got), ["Pramerica SGR S.p.A."]);
        }
    }

    mod the_line_that_says_everything {
        use super::*;

        #[test]
        fn the_connector_cuts_the_line_into_manager_and_sub_funds() {
            // FIDEURAM-EN23 p71 puts thirteen lines of this shape one under the other.
            let blocks = [block("Acme Asset Management for Alpha Fund, Beta Fund.")];
            let got = read(&connector_line(), &blocks);
            assert_eq!(
                said(&got),
                [(
                    "Acme Asset Management".to_string(),
                    Some(vec!["Alpha Fund".to_string(), "Beta Fund".to_string()]),
                    false,
                    false
                )]
            );
        }

        #[test]
        fn the_left_side_keeps_a_comma_inside_a_company_name() {
            // `Vontobel Asset Management S.A., Milan Branch` is **one** manager.
            let blocks = [block("Vontobel Asset Management S.A., Milan Branch for Alpha Fund")];
            let got = read(&connector_line(), &blocks);
            assert_eq!(names(&got), ["Vontobel Asset Management S.A., Milan Branch"]);
        }

        #[test]
        fn two_managers_on_one_line_become_two_declarations() {
            let blocks =
                [block("FIL Investments International and FIL (Luxembourg) S.A. for Alpha Fund")];
            let got = read(&connector_line(), &blocks);
            assert_eq!(names(&got), ["FIL Investments International", "FIL (Luxembourg) S.A."]);
            assert_eq!(funds_of(&got, "FIL (Luxembourg) S.A."), Some(vec!["Alpha Fund".to_string()]));
        }

        #[test]
        fn the_block_carries_only_the_manager_and_not_its_whole_list() {
            let blocks = [block("Acme AM for Alpha Fund, Beta Fund.")];
            let got = read(&connector_line(), &blocks);
            let kept = got[0].block.as_ref().expect("the block is kept");
            assert_eq!(kept.content.as_str(), Some("Acme AM"));
        }

        #[test]
        fn a_line_with_no_connector_is_dropped() {
            let got = read(&connector_line(), &[block("The Management Company explains.")]);
            assert!(got.is_empty());
        }

        #[test]
        fn the_after_fragment_cuts_the_prose_in_front_of_the_name() {
            // UBS-EN23 p4 writes a whole sentence around the two sides.
            let reader = Reader::ConnectorLine {
                connector: " the day to day portfolio management of ".to_string(),
                after: "The Management Company delegated to ".to_string(),
                manager_separators: vec![" and ".to_string()],
                fund_separators: vec![",".to_string(), " and ".to_string()],
                terminators: ",.;".to_string(),
                group_key: "group".to_string(),
                note_marks: NOTE_MARKS.to_string(),
            };
            let blocks = [block(
                "The Management Company delegated to Nordea Investment Management AB the day to \
                 day portfolio management of BPER International SICAV – Low Duration.",
            )];
            let got = read(&reader, &blocks);
            assert_eq!(names(&got), ["Nordea Investment Management AB"]);
            assert_eq!(
                funds_of(&got, "Nordea Investment Management AB"),
                Some(vec!["BPER International SICAV – Low Duration".to_string()])
            );
        }

        #[test]
        fn a_line_the_after_fragment_does_not_carry_is_dropped() {
            let reader = Reader::ConnectorLine {
                connector: " for ".to_string(),
                after: "The Management Company delegated to ".to_string(),
                manager_separators: vec![" and ".to_string()],
                fund_separators: vec![",".to_string()],
                terminators: ",.;".to_string(),
                group_key: "group".to_string(),
                note_marks: NOTE_MARKS.to_string(),
            };
            let got = read(&reader, &[block("Acme AM for Alpha Fund")]);
            assert!(got.is_empty());
        }

        #[test]
        fn an_entry_spread_over_two_lines_is_read_as_one() {
            let blocks = [grouped(0, "Acme Asset Management for"), grouped(0, "Alpha Fund.")];
            let got = read(&connector_line(), &blocks);
            assert_eq!(funds_of(&got, "Acme Asset Management"), Some(vec!["Alpha Fund".to_string()]));
        }
    }

    mod one_row_per_sub_fund {
        use super::*;

        fn table() -> Reader {
            Reader::Table {
                fund_col: 0,
                manager_col: 2,
                row_key: "table-row".to_string(),
                col_key: "table-col".to_string(),
                note_marks: NOTE_MARKS.to_string(),
            }
        }

        #[test]
        fn the_same_manager_on_many_rows_is_one_declaration() {
            // FINECO-EN23@IR p155: thirty-five sub-funds, and the manager comes back.
            let blocks = [
                cell(0, 0, "Alpha Fund"),
                cell(0, 1, "1 Jan 2020"),
                cell(0, 2, "Acme AM"),
                cell(1, 0, "Beta Fund"),
                cell(1, 1, "1 Jan 2021"),
                cell(1, 2, "Acme AM"),
            ];
            let got = read(&table(), &blocks);
            assert_eq!(
                said(&got),
                [(
                    "Acme AM".to_string(),
                    Some(vec!["Alpha Fund".to_string(), "Beta Fund".to_string()]),
                    false,
                    false
                )]
            );
        }

        #[test]
        fn the_row_order_is_the_declaration_order() {
            let blocks = [
                cell(1, 0, "Beta Fund"),
                cell(1, 2, "Second AM"),
                cell(0, 0, "Alpha Fund"),
                cell(0, 2, "First AM"),
            ];
            let got = read(&table(), &blocks);
            assert_eq!(names(&got), ["First AM", "Second AM"]);
        }

        #[test]
        fn a_row_missing_one_of_the_two_columns_is_skipped() {
            let blocks = [cell(0, 0, "Alpha Fund"), cell(1, 2, "Acme AM")];
            let got = read(&table(), &blocks);
            assert!(got.is_empty());
        }

        #[test]
        fn a_block_with_no_cell_metadata_is_not_part_of_the_table() {
            let blocks = [block("A heading"), cell(0, 0, "Alpha Fund"), cell(0, 2, "Acme AM")];
            let got = read(&table(), &blocks);
            assert_eq!(names(&got), ["Acme AM"]);
        }

        #[test]
        fn the_note_mark_comes_off_each_sub_fund() {
            let blocks = [cell(0, 0, "Alpha Fund*"), cell(0, 2, "Acme AM")];
            let got = read(&table(), &blocks);
            assert_eq!(funds_of(&got, "Acme AM"), Some(vec!["Alpha Fund".to_string()]));
        }
    }

    mod the_group_is_the_hidden_parameter {
        use super::*;

        #[test]
        fn without_groups_every_block_is_an_entry_of_its_own() {
            let blocks = [block("One AM"), block("Two AM")];
            let got = read(&adjacent(Spec::Parenthetical(parenthetical(&["in respect of "]))), &blocks);
            assert_eq!(names(&got), ["One AM", "Two AM"]);
        }

        #[test]
        fn with_groups_a_second_line_of_the_same_entry_is_not_a_name() {
            let blocks = [grouped(0, "One AM"), grouped(0, "25 Some Street")];
            let got = read(&adjacent(Spec::Parenthetical(parenthetical(&["in respect of "]))), &blocks);
            assert_eq!(names(&got), ["One AM"]);
        }

        #[test]
        fn a_group_written_as_a_string_groups_the_same_way_as_an_integer() {
            let as_text = |group: &str, text: &str| {
                PdfBlock::new(
                    MANAGER,
                    BTreeMap::from([("group".to_string(), BlockValue::Str(group.to_string()))]),
                    text,
                )
            };
            let blocks = [as_text("0", "One AM"), as_text("0", "25 Some Street")];
            let got = read(&adjacent(Spec::Parenthetical(parenthetical(&["in respect of "]))), &blocks);
            assert_eq!(names(&got), ["One AM"]);
        }
    }

    /// The cases whose lines are **measured**, taken off the page with `inspect-page --mode lines`
    /// and shortened only where the address adds nothing. They came over from the Python template's
    /// own tests when its logic was promoted, because the lines are the evidence and inventing them
    /// again would lose it.
    mod measured_pages {
        use super::*;

        mod fineco_en23_lux_p6 {
            use super::*;

            fn reader() -> Reader {
                Reader::Adjacent {
                    spec: Spec::Parenthetical(parenthetical(&[
                        "Only in respect of the Sub-Fund",
                        "Only in respect of",
                    ])),
                    where_: Where::Above,
                    take: Take::All,
                    group_key: "group".to_string(),
                    strip: Vec::new(),
                    note_type: None,
                    retired: Vec::new(),
                }
            }

            #[test]
            fn the_next_specification_closes_the_one_before() {
                let blocks = [
                    grouped(0, "(Only in respect of the Sub-Fund Global Macro"),
                    grouped(0, "Credit FAM Fund)"),
                    grouped(1, "Algebris (UK) Limited"),
                    grouped(1, "4th Floor, 1 St James's Market"),
                    grouped(2, "(Only in respect of Bond 6M Fineco AM Fund,"),
                    grouped(2, "Enhanced Bond 6M Fineco AM Fund and"),
                    grouped(2, "Sustainable Enhanced Bond 12M Fineco AM Fund)"),
                    grouped(3, "BNP Paribas Asset Management France"),
                    grouped(3, "1 Boulevard Haussmann"),
                ];
                let got = read(&reader(), &blocks);
                assert_eq!(names(&got), ["Algebris (UK) Limited", "BNP Paribas Asset Management France"]);
                assert_eq!(
                    funds_of(&got, "Algebris (UK) Limited"),
                    Some(vec!["Global Macro Credit FAM Fund".to_string()])
                );
                assert_eq!(
                    funds_of(&got, "BNP Paribas Asset Management France"),
                    Some(vec![
                        "Bond 6M Fineco AM Fund".to_string(),
                        "Enhanced Bond 6M Fineco AM Fund".to_string(),
                        "Sustainable Enhanced Bond 12M Fineco AM Fund".to_string(),
                    ])
                );
            }
        }

        mod ubs_en23_p4 {
            use super::*;

            /// The page's three lines: the umbrella in roman, the sub-fund in italic, the manager
            /// with its city inside the company name.
            fn page() -> [PdfBlock; 3] {
                [
                    grouped(0, "BPER International SICAV"),
                    grouped(0, "– Open Selection Defence"),
                    grouped(0, "Banca Cesare Ponti, Milano, Italy"),
                ]
            }

            fn reader(strip: &[&str]) -> Reader {
                Reader::Adjacent {
                    spec: Spec::Listing(Listing {
                        umbrella: Some("BPER International SICAV".to_string()),
                        ..Listing::default()
                    }),
                    where_: Where::Above,
                    take: Take::First,
                    group_key: "group".to_string(),
                    strip: compile_all("strip", &strip.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                        .expect("literal patterns compile"),
                    note_type: None,
                    retired: Vec::new(),
                }
            }

            #[test]
            fn the_city_inside_the_company_name_is_stripped() {
                // The register wants `Banca Cesare Ponti`, and this format says so by cutting
                // everything after the first comma. It is a parameter: the same expression would
                // mutilate FIDEURAM-EN23's `Vontobel Asset Management S.A., Milan Branch`.
                let got = read(&reader(&[r",.*$"]), &page());
                assert_eq!(names(&got), ["Banca Cesare Ponti"]);
                assert_eq!(
                    funds_of(&got, "Banca Cesare Ponti"),
                    Some(vec!["BPER International SICAV Open Selection Defence".to_string()])
                );
            }

            #[test]
            fn the_block_that_travels_to_the_output_carries_the_stripped_name() {
                let got = read(&reader(&[r",.*$"]), &page());
                let kept = got[0].block.as_ref().expect("the block is kept");
                assert_eq!(kept.content.as_str(), Some("Banca Cesare Ponti"));
                // The metadata travels too: the output row knows which entry it came from.
                assert!(kept.metadata.contains_key("group"));
            }

            #[test]
            fn without_the_strip_the_name_stays_as_the_page_writes_it() {
                let got = read(&reader(&[]), &page());
                assert_eq!(names(&got), ["Banca Cesare Ponti, Milano, Italy"]);
            }
        }

        mod ubs_en23_p5 {
            use super::*;

            #[test]
            fn the_page_opens_with_the_tail_of_the_previous_ones_prose_and_it_is_dropped() {
                let blocks = [
                    typed_grouped("INV", 0, "The portfolio management units of UBS Asset"),
                    typed_grouped("SUB", 1, "BPER International SICAV"),
                    typed_grouped("SUB", 1, "– Global Balanced Risk Control"),
                    typed_grouped("INV", 2, "Morgan Stanley Investment Management Limited"),
                ];
                let reader = Reader::Adjacent {
                    spec: Spec::Listing(Listing {
                        block_type: Some(BlockType::from("SUB".to_string())),
                        ..Listing::default()
                    }),
                    where_: Where::Above,
                    take: Take::First,
                    group_key: "group".to_string(),
                    strip: Vec::new(),
                    note_type: None,
                    retired: Vec::new(),
                };
                let got = read(&reader, &blocks);
                assert_eq!(names(&got), ["Morgan Stanley Investment Management Limited"]);
            }
        }

        mod carne_en23_p4 {
            use super::*;
            use crate::core::match_fund::MatchFund;
            use crate::formats_utils::text_filter::inv_managers::assignment::{
                Manco, assign, fund_set,
            };

            /// The whole page, with the two sub-funds the holdings had seen.
            fn page() -> [PdfBlock; 7] {
                [
                    grouped(0, "ARCA Fondi SGR S.p.A."),
                    grouped(0, "Via Disciplini 3"),
                    grouped(0, "I-20123 Milan"),
                    grouped(1, "Goldman Sachs Asset Management International Ltd."),
                    grouped(1, "(in charge for the USD nominated securities in the"),
                    grouped(1, "subfund Sidera Funds SICAV – Global High Yield)"),
                    grouped(1, "Plumtree Court, 25 Shoe Lane"),
                ]
            }

            fn reader() -> Reader {
                Reader::Adjacent {
                    spec: Spec::Parenthetical(Parenthetical {
                        markers: vec!["subfund ".to_string()],
                        partial_markers: vec!["in charge for ".to_string()],
                        ..Parenthetical::default()
                    }),
                    where_: Where::Below,
                    take: Take::All,
                    group_key: "group".to_string(),
                    strip: Vec::new(),
                    note_type: None,
                    retired: Vec::new(),
                }
            }

            #[test]
            fn the_residual_goes_to_the_manager_the_page_does_not_qualify() {
                let funds = fund_set([
                    "Sidera Funds SICAV – Global High Yield",
                    "Sidera Funds SICAV – Equity Global",
                ]);
                let manco = Manco::Name("Carne Global Fund Managers (Luxembourg) S.A.".to_string());
                let got =
                    assign(read(&reader(), &page()), &funds, Some(&manco), &Default::default(), true);
                assert_eq!(
                    got.managers.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
                    [
                        "ARCA Fondi SGR S.p.A.",
                        "Goldman Sachs Asset Management International Ltd."
                    ]
                );
                assert!(!got.to_manco, "ARCA takes the residual, so the company stays only that");
            }

            #[test]
            fn the_partial_fund_stays_with_the_default_manager_too() {
                // `R21`: a part of a sub-fund does not take the sub-fund from whoever answers for
                // the rest of it.
                let funds = fund_set(["Sidera Funds SICAV – Global High Yield"]);
                let got = assign(read(&reader(), &page()), &funds, None, &Default::default(), true);
                let residual = got
                    .managers
                    .iter()
                    .find(|m| m.name == "ARCA Fondi SGR S.p.A.")
                    .expect("ARCA is there");
                assert_eq!(
                    residual.funds.iter().map(MatchFund::name).collect::<Vec<_>>(),
                    ["Sidera Funds SICAV – Global High Yield"]
                );
            }

            #[test]
            fn the_manager_read_from_a_block_keeps_the_page_it_comes_from() {
                let got = read(&reader(), &page());
                let kept = got[0].block.as_ref().expect("the block is kept");
                assert_eq!(kept.content.as_str(), Some("ARCA Fondi SGR S.p.A."));
            }
        }

        mod what_a_specification_reads_on_its_own {
            use super::*;
            use crate::formats_utils::text_filter::inv_managers::assignment::fund_set;

            #[test]
            fn an_empty_parenthesis_names_nothing() {
                // Not a sub-fund without a name: nothing at all.
                let (funds, partial) = Spec::Parenthetical(Parenthetical::default()).read(&["()".to_string()]);
                assert_eq!(funds, Some(Vec::new()));
                assert!(!partial);
            }

            #[test]
            fn the_composed_name_is_the_one_the_investments_write() {
                // `R24`: the two spellings are the same element, which is what makes composing the
                // umbrella in front of the item worth doing.
                let listing = Listing {
                    umbrella: Some("BPER International SICAV".to_string()),
                    ..Listing::default()
                };
                let (funds, _) = Spec::Listing(listing)
                    .read(&["– Emerging Markets - Multi Asset Dividend".to_string()]);
                assert_eq!(
                    fund_set(funds.expect("the list names its sub-fund")),
                    fund_set(["BPER INTERNATIONAL SICAV EMERGING MARKETS MULTI ASSET DIVIDEND"])
                );
            }

            #[test]
            fn an_item_with_no_umbrella_anywhere_stands_on_its_own() {
                let (funds, _) =
                    Spec::Listing(Listing::default()).read(&["– Optimal Income".to_string()]);
                assert_eq!(funds, Some(vec!["Optimal Income".to_string()]));
            }
        }

        mod fineco_en23_ir_p155 {
            use super::*;

            #[test]
            fn the_order_is_the_first_appearance_of_each_manager() {
                let reader = Reader::Table {
                    fund_col: 0,
                    manager_col: 2,
                    row_key: "table-row".to_string(),
                    col_key: "table-col".to_string(),
                    note_marks: NOTE_MARKS.to_string(),
                };
                let blocks = [
                    cell(0, 0, "Bond 6M Fineco AM Fund"),
                    cell(0, 2, "BNP Paribas Asset Management France"),
                    cell(1, 0, "Global Macro Credit FAM Fund"),
                    cell(1, 2, "Algebris (UK) Limited"),
                    cell(2, 0, "Enhanced Bond 6M Fineco AM Fund"),
                    cell(2, 2, "BNP Paribas Asset Management France"),
                ];
                let got = read(&reader, &blocks);
                assert_eq!(
                    names(&got),
                    ["BNP Paribas Asset Management France", "Algebris (UK) Limited"]
                );
                assert_eq!(
                    funds_of(&got, "BNP Paribas Asset Management France"),
                    Some(vec![
                        "Bond 6M Fineco AM Fund".to_string(),
                        "Enhanced Bond 6M Fineco AM Fund".to_string(),
                    ])
                );
            }
        }
    }

    mod renaming_a_block {
        use super::*;

        #[test]
        fn it_keeps_the_type_and_the_metadata_and_changes_the_content() {
            let original = grouped(3, "Acme AM for Alpha Fund");
            let got = renamed(&original, "Acme AM");
            assert_eq!(got.type_block, original.type_block);
            assert_eq!(got.metadata, original.metadata);
            assert_eq!(got.content.as_str(), Some("Acme AM"));
        }

        #[test]
        fn the_original_block_is_left_alone() {
            let original = block("Acme AM for Alpha Fund");
            let _ = renamed(&original, "Acme AM");
            assert_eq!(original.content.as_str(), Some("Acme AM for Alpha Fund"));
        }
    }
}
