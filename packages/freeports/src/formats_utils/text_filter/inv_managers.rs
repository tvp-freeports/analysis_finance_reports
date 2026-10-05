//! `A5` — the one assigner of the investment managers, and the readers that feed it.
//!
//! A report states the **exceptions** and says nothing about the rest: the residual belongs to
//! whoever the page does not name. This module is the place — one only — where that residual is
//! worked out, with the seven rules of `R6`+`R15`+`R21`+`R22` in [`assignment`] and the blocks that
//! come out of it in [`blocks`].
//!
//! [`reading`] stands on the other side: it **builds** those pairs by reading the page's blocks, in
//! one of the shapes a report writes them in.
//!
//! # What a format has to say, and nothing more
//!
//! The ten knobs of [`TextFilterInvManagers`] are not ten branches: each of them is one measured
//! call site's one difference from the others. Which blocks the readers see and which carry the
//! management company are two block types; where the funds of interest come from is `R24`'s
//! question, and it has the two answers the engine's two decorators had; whether the page closes
//! the account is `settle`; and whether the management company's own row is written here or by a
//! pipeline of its own is the rest.
//!
//! # The page that declares nothing
//!
//! A page that yields no declaration **and** no management company produces no block, and says so
//! at debug level. The Python template logged an error there instead — "funds have no investments
//! manager and the format reads no management company" — and that message appears in none of the
//! reference runs, because the one format whose page can be empty guarded against it by hand
//! (FINECO-EN23@IR's management-company page returns early on an empty block list). The guard
//! belongs here rather than in each format: a page that read nothing has nothing to assign, and the
//! wrongly classified page is already diagnosed by the schedule.

pub mod assignment;
pub mod blocks;
pub mod reading;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::classes::{BlockType, PdfBlock, TextBlock};
use crate::core::match_fund::MatchFund;
use crate::core::pipeline::{Extracted, FilterData, PipeError, TextFilterPipe};
use crate::output::classes::fund::Fund;

use assignment::{Manco, assign, assigned_funds, collect_funds};
use reading::Reader;

/// Where the funds of interest come from — `R24`'s question, with the two answers the engine's two
/// Python decorators had.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FundSource {
    /// The fund named by each holding, which is the fund **as the holdings spell it**.
    #[default]
    Investments,
    /// The names of the `Fund` results, for the formats whose managers' page runs before the
    /// holdings or alongside them.
    Funds,
}

/// Where the management company comes from, when the format reads one at all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MancoSource {
    /// The format does not read it: the residual then has nobody to go to, and `assign` says so.
    #[default]
    None,
    /// The first block of the given type among this pipeline's own blocks — the page declares the
    /// company and the managers together.
    Blocks(BlockType),
    /// The `ManagementCompany` already extracted by an earlier step of the schedule. It arrives as a
    /// name, because what crosses a step is a result and not a page.
    PreviousResult,
}

/// Whether this pipeline writes the management company's **own** row, and under what condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmitManco {
    /// The `manco` has a pipeline of its own, which writes it.
    #[default]
    Never,
    /// Whenever there is one: the page that declares the company declares it in that role too.
    WhenPresent,
    /// `C10` — only when a manager named a sub-fund the job had not seen. The company's row was
    /// already written by an earlier step, but without that sub-fund, which did not exist yet;
    /// rewriting it with every fund (`R26`) is what keeps its column from staying empty.
    OnNewFunds,
}

/// The `inv_managers` `text_filter`: reads the declarations, assigns the funds, writes the blocks.
pub struct TextFilterInvManagers {
    /// The readers, applied in order to the same blocks and concatenated. More than one is UBS-EN23,
    /// where a list and a prose paragraph declare on the same page; none is the page that only
    /// closes the account for the management company.
    pub readers: Vec<Reader>,
    /// The blocks the readers see. `None` means every block of the pipeline.
    pub read_block_type: Option<BlockType>,
    pub funds: FundSource,
    pub manco: MancoSource,
    pub emit_manco: EmitManco,
    /// Whether the management company's row comes **before** the managers'. It changes nothing but
    /// the order of the output identifiers, and that order is frozen in the reference runs.
    pub manco_first: bool,
    pub settle: bool,
    /// Whether the residual is worked out over what other pages of this job have already taken.
    pub already_assigned: bool,
    /// Whether those already-taken funds also count among the document's funds. FINECO-EN23@IR needs
    /// it: the sub-funds of its table enter the document even when no holding names them, and
    /// without the union their `funds.csv` rows would have no management company (`R26`).
    pub funds_include_assigned: bool,
    /// Notes the format has to leave in the audit trail about its own nomenclature, warned once per
    /// document. MEDIOLANUM-EN23 is the measured case: it names no management company and its
    /// *Alternative Investment Fund Manager and Global Distributor* is read as one, which is an
    /// interpretation the engine cannot make by itself and `R25` says must not stay unsaid.
    pub notes: Vec<String>,
    warned: AtomicBool,
}

impl TextFilterInvManagers {
    /// The filter with every default: one `Named` reader, holdings as the funds, no management
    /// company, settling the account.
    pub fn new(readers: Vec<Reader>) -> Self {
        TextFilterInvManagers {
            readers,
            read_block_type: Some(BlockType::INVESTMENTS_MANAGER),
            funds: FundSource::default(),
            manco: MancoSource::default(),
            emit_manco: EmitManco::default(),
            manco_first: false,
            settle: true,
            already_assigned: false,
            funds_include_assigned: false,
            notes: Vec::new(),
            warned: AtomicBool::new(false),
        }
    }

    /// The funds this job has seen, from the source the format declares.
    fn funds_of(&self, data: &FilterData<'_>) -> BTreeSet<MatchFund> {
        match self.funds {
            // `collect_funds` and not `collect()`: which of two equal spellings survives is an
            // output value. See `assignment::fund_set`.
            FundSource::Funds => collect_funds(
                data.previous()
                    .iter()
                    .filter_map(Extracted::as_fund)
                    .filter_map(Fund::name)
                    .map(MatchFund::new),
            ),
            FundSource::Investments => collect_funds(
                data.previous()
                    .iter()
                    .filter_map(|e| {
                        e.as_equity().map(|eq| &eq.data).or_else(|| e.as_bond().map(|b| &b.data))
                    })
                    .filter_map(|inv| inv.fund.resolved())
                    .map(MatchFund::new),
            ),
        }
    }

    /// The management company this page has to answer for, if any.
    fn manco_of(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Option<Manco> {
        match &self.manco {
            MancoSource::None => None,
            MancoSource::Blocks(wanted) => blocks
                .iter()
                .find(|block| block.type_block == *wanted)
                .map(|block| Manco::Block(block.clone())),
            MancoSource::PreviousResult => data
                .previous()
                .iter()
                .filter_map(Extracted::as_management_company)
                .map(|company| Manco::Name(company.data.name.clone()))
                .next(),
        }
    }

    pub fn call(&self, pdf_blks: &[PdfBlock], data: &FilterData<'_>) -> Vec<TextBlock> {
        if !self.notes.is_empty() && !self.warned.swap(true, Ordering::Relaxed) {
            for note in &self.notes {
                tracing::warn!("{note}");
            }
        }

        let readable: Vec<&PdfBlock> = match &self.read_block_type {
            None => pdf_blks.iter().collect(),
            Some(wanted) => pdf_blks.iter().filter(|block| block.type_block == *wanted).collect(),
        };
        let declared: Vec<_> =
            self.readers.iter().flat_map(|reader| reader.read(&readable)).collect();
        let manco = self.manco_of(pdf_blks, data);

        // A page that read nothing and names no company has nothing to assign: see the module
        // documentation for why this is a guard and not an error.
        if declared.is_empty() && manco.is_none() {
            tracing::debug!("this page declares no manager and no management company: no block");
            return Vec::new();
        }

        let seen = self.funds_of(data);
        let taken = if self.already_assigned { assigned_funds(data) } else { BTreeSet::new() };
        let funds: BTreeSet<MatchFund> = if self.funds_include_assigned {
            let mut funds = seen;
            funds.extend(taken.iter().cloned());
            funds
        } else {
            seen
        };

        let assignment = assign(declared, &funds, manco.as_ref(), &taken, self.settle);
        let company = manco.as_ref().filter(|_| match self.emit_manco {
            EmitManco::Never => false,
            EmitManco::WhenPresent => true,
            EmitManco::OnNewFunds => !assignment.new_funds.is_empty(),
        });

        let mut out = Vec::new();
        if self.manco_first && let Some(company) = company.as_ref() {
            out.push(blocks::management_company_block(&assignment.funds, company));
        }
        out.extend(blocks::text_blocks(&assignment));
        if !self.manco_first && let Some(company) = company.as_ref() {
            out.push(blocks::management_company_block(&assignment.funds, company));
        }
        out
    }
}

impl TextFilterPipe for TextFilterInvManagers {
    fn name(&self) -> &str {
        "TextFilterInvManagers"
    }

    fn filter(&self, blocks: &[PdfBlock], data: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
        Ok(self.call(blocks, data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::classes::assets_manager::{
        AssetsManagerData, InvestmentsManager, ManagementCompany,
    };
    use crate::output::classes::fund::Fund;
    use reading::Reader;

    /// An `Equity` holding of `fund`, which is where `R24`'s spelling comes from.
    fn holding(fund: &str) -> Extracted {
        use crate::core::classes::value::BlockValue;
        use crate::output::classes::investment::{Equity, InvestmentFields};
        Extracted::Equity(
            Equity::build(InvestmentFields::new(
                "Acme Corp",
                "Acme",
                BlockValue::from(fund),
                BlockValue::from(1000.0),
                BlockValue::from(crate::commons::consts::Currency::EUR),
            ))
            .expect("fixed, valid fixture"),
        )
    }

    fn manager_result(name: &str, funds: &[&str]) -> Extracted {
        Extracted::InvestmentsManager(InvestmentsManager {
            data: AssetsManagerData {
                name: name.to_string(),
                managed_funds: funds.iter().map(|f| f.to_string()).collect(),
            },
        })
    }

    fn manco_result(name: &str) -> Extracted {
        Extracted::ManagementCompany(ManagementCompany {
            data: AssetsManagerData {
                name: name.to_string(),
                managed_funds: BTreeSet::new(),
            },
        })
    }

    fn manager_block(text: &str) -> PdfBlock {
        PdfBlock::bare(BlockType::INVESTMENTS_MANAGER, text)
    }

    fn manco_block(text: &str) -> PdfBlock {
        PdfBlock::bare(BlockType::MANAGEMENT_COMPANY, text)
    }

    fn content(block: &TextBlock) -> String {
        block.content.as_str().unwrap_or_default().to_string()
    }

    /// `(type, content)` of each block the pipe wrote, in order.
    fn wrote(blocks: &[TextBlock]) -> Vec<(String, String)> {
        blocks.iter().map(|b| (b.type_block.as_str().to_string(), content(b))).collect()
    }

    mod where_the_funds_come_from {
        use super::*;

        #[test]
        fn by_default_they_are_the_funds_the_holdings_name() {
            let previous = vec![holding("Alpha Fund"), holding("Beta Fund")];
            let pipe = TextFilterInvManagers::new(vec![Reader::default()]);
            let got = pipe.call(&[manager_block("Acme AM")], &FilterData::Previous(&previous));
            assert_eq!(got.len(), 1);
            assert_eq!(got[0].metadata.len(), 1);
        }

        #[test]
        fn a_format_can_ask_for_the_declared_funds_instead() {
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund"))];
            let mut pipe = TextFilterInvManagers::new(vec![Reader::default()]);
            pipe.funds = FundSource::Funds;
            let got = pipe.call(&[manager_block("Acme AM")], &FilterData::Previous(&previous));
            assert_eq!(got.len(), 1);
        }

        #[test]
        fn the_holdings_funds_are_not_the_declared_ones() {
            // A format reading the managers before the holdings would see nothing the other way.
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund"))];
            let pipe = TextFilterInvManagers::new(vec![Reader::default()]);
            let got = pipe.call(&[manager_block("Acme AM")], &FilterData::Previous(&previous));
            // One manager, credited with nothing, because the holdings named no fund.
            assert_eq!(got.len(), 1);
            assert!(got[0].metadata.contains_key("managed_funds"));
        }
    }

    mod which_blocks_the_readers_see {
        use super::*;

        #[test]
        fn by_default_only_the_managers_own_type() {
            let previous = vec![holding("Alpha Fund")];
            let blocks = [manco_block("Acme SGR"), manager_block("Delegate AM")];
            let pipe = TextFilterInvManagers::new(vec![Reader::default()]);
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            assert_eq!(wrote(&got), [("INVESTMENTS_MANAGER".to_string(), "Delegate AM".to_string())]);
        }

        #[test]
        fn a_format_whose_pdf_extract_writes_its_own_types_asks_for_every_block() {
            let previous = vec![holding("Alpha Fund")];
            let blocks = [PdfBlock::bare(BlockType::from("MANAGER".to_string()), "Delegate AM")];
            let mut pipe = TextFilterInvManagers::new(vec![Reader::default()]);
            pipe.read_block_type = None;
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            assert_eq!(got.len(), 1);
            assert_eq!(content(&got[0]), "Delegate AM");
        }
    }

    mod the_management_company {
        use super::*;

        #[test]
        fn without_one_the_residual_goes_nowhere() {
            let previous = vec![holding("Alpha Fund")];
            let blocks = [manager_block("Delegate AM")];
            let mut pipe = TextFilterInvManagers::new(vec![Reader::default()]);
            pipe.readers = vec![Reader::default()];
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            // `R15`: the only declared manager takes it, so there is nothing left to go nowhere.
            assert_eq!(got.len(), 1);
        }

        #[test]
        fn read_off_the_page_it_takes_the_residual_and_writes_its_own_row() {
            let previous = vec![holding("Alpha Fund"), holding("Beta Fund")];
            let blocks = [manco_block("Acme SGR"), manager_block("Delegate AM")];
            let mut pipe = TextFilterInvManagers::new(Vec::new());
            pipe.manco = MancoSource::Blocks(BlockType::MANAGEMENT_COMPANY);
            pipe.emit_manco = EmitManco::WhenPresent;
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            assert_eq!(
                wrote(&got),
                [
                    ("INVESTMENTS_MANAGER".to_string(), "Acme SGR".to_string()),
                    ("MANAGEMENT_COMPANY".to_string(), "Acme SGR".to_string()),
                ]
            );
        }

        #[test]
        fn it_can_come_from_a_result_of_an_earlier_step() {
            // FIDEURAM-EN23 reads it on p67 and the managers on p71.
            let previous = vec![holding("Alpha Fund"), manco_result("Acme SGR")];
            let mut pipe = TextFilterInvManagers::new(Vec::new());
            pipe.manco = MancoSource::PreviousResult;
            let got = pipe.call(&[], &FilterData::Previous(&previous));
            assert_eq!(
                wrote(&got),
                [("INVESTMENTS_MANAGER".to_string(), "Acme SGR".to_string())]
            );
        }

        #[test]
        fn its_own_row_is_written_only_for_new_funds_when_the_format_asks_so() {
            // `C10`: an earlier step already wrote it, but without the fund born here.
            let previous = vec![holding("Alpha Fund"), manco_result("Acme SGR")];
            let blocks = [manager_block("Delegate AM for Alpha Fund")];
            let mut pipe = TextFilterInvManagers::new(vec![Reader::ConnectorLine {
                connector: " for ".to_string(),
                after: String::new(),
                manager_separators: vec![" and ".to_string()],
                fund_separators: vec![",".to_string()],
                terminators: ",.;".to_string(),
                group_key: "group".to_string(),
                note_marks: reading::NOTE_MARKS.to_string(),
            }]);
            pipe.manco = MancoSource::PreviousResult;
            pipe.emit_manco = EmitManco::OnNewFunds;
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            assert_eq!(
                wrote(&got),
                [("INVESTMENTS_MANAGER".to_string(), "Delegate AM".to_string())]
            );
        }

        #[test]
        fn a_new_fund_does_bring_its_row_back() {
            let previous = vec![holding("Alpha Fund"), manco_result("Acme SGR")];
            let blocks = [manager_block("Delegate AM for Alpha Fund, Delta Fund")];
            let mut pipe = TextFilterInvManagers::new(vec![Reader::ConnectorLine {
                connector: " for ".to_string(),
                after: String::new(),
                manager_separators: vec![" and ".to_string()],
                fund_separators: vec![",".to_string()],
                terminators: ",.;".to_string(),
                group_key: "group".to_string(),
                note_marks: reading::NOTE_MARKS.to_string(),
            }]);
            pipe.manco = MancoSource::PreviousResult;
            pipe.emit_manco = EmitManco::OnNewFunds;
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            assert_eq!(
                wrote(&got),
                [
                    ("INVESTMENTS_MANAGER".to_string(), "Delegate AM".to_string()),
                    ("FUND".to_string(), "Delta Fund".to_string()),
                    ("MANAGEMENT_COMPANY".to_string(), "Acme SGR".to_string()),
                ]
            );
        }

        #[test]
        fn its_row_can_come_first_where_the_frozen_order_wants_it_there() {
            let previous = vec![holding("Alpha Fund")];
            let blocks = [manco_block("Acme SGR")];
            let mut pipe = TextFilterInvManagers::new(Vec::new());
            pipe.manco = MancoSource::Blocks(BlockType::MANAGEMENT_COMPANY);
            pipe.emit_manco = EmitManco::WhenPresent;
            pipe.manco_first = true;
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            assert_eq!(
                wrote(&got),
                [
                    ("MANAGEMENT_COMPANY".to_string(), "Acme SGR".to_string()),
                    ("INVESTMENTS_MANAGER".to_string(), "Acme SGR".to_string()),
                ]
            );
        }
    }

    mod the_section_that_crosses_pages {
        use super::*;

        #[test]
        fn a_page_that_does_not_settle_gives_the_residual_to_nobody() {
            let previous = vec![holding("Alpha Fund"), holding("Beta Fund")];
            let blocks = [manco_block("Acme SGR")];
            let mut pipe = TextFilterInvManagers::new(Vec::new());
            pipe.manco = MancoSource::Blocks(BlockType::MANAGEMENT_COMPANY);
            pipe.emit_manco = EmitManco::WhenPresent;
            pipe.settle = false;
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            assert_eq!(wrote(&got), [("MANAGEMENT_COMPANY".to_string(), "Acme SGR".to_string())]);
        }

        #[test]
        fn the_closing_page_counts_what_the_others_took() {
            let previous = vec![
                holding("Alpha Fund"),
                holding("Beta Fund"),
                manager_result("Earlier AM", &["Alpha Fund"]),
            ];
            let blocks = [manco_block("Acme SGR")];
            let mut pipe = TextFilterInvManagers::new(Vec::new());
            pipe.manco = MancoSource::Blocks(BlockType::MANAGEMENT_COMPANY);
            pipe.already_assigned = true;
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            let managed = match got[0].metadata.get("managed_funds") {
                Some(crate::core::classes::value::BlockValue::Set(names)) => names.len(),
                _ => 0,
            };
            assert_eq!(managed, 1, "only Beta Fund was left to take");
        }

        #[test]
        fn the_funds_another_page_took_can_also_count_as_funds_of_the_document() {
            // FINECO-EN23@IR: the sub-funds of its table enter even when no holding names them.
            let previous = vec![manager_result("Earlier AM", &["Alpha Fund"])];
            let blocks = [manco_block("Acme SGR")];
            let mut pipe = TextFilterInvManagers::new(Vec::new());
            pipe.manco = MancoSource::Blocks(BlockType::MANAGEMENT_COMPANY);
            pipe.emit_manco = EmitManco::WhenPresent;
            pipe.manco_first = true;
            pipe.already_assigned = true;
            pipe.funds_include_assigned = true;
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            let managed = match got[0].metadata.get("managed_funds") {
                Some(crate::core::classes::value::BlockValue::Set(names)) => names.len(),
                _ => 0,
            };
            assert_eq!(managed, 1, "`R26` wants the company over Alpha Fund too");
        }
    }

    mod several_readers_on_one_page {
        use super::*;

        #[test]
        fn their_declarations_concatenate_and_merge_by_name() {
            // UBS-EN23 p4: a list and a prose paragraph declare on the same page, and `R22` fuses
            // whoever appears in both.
            let previous = vec![holding("Alpha Fund"), holding("Beta Fund")];
            let blocks = [
                manager_block("Acme AM"),
                manager_block("Acme AM for Beta Fund"),
            ];
            let pipe = TextFilterInvManagers::new(vec![
                Reader::default(),
                Reader::ConnectorLine {
                    connector: " for ".to_string(),
                    after: String::new(),
                    manager_separators: vec![" and ".to_string()],
                    fund_separators: vec![",".to_string()],
                    terminators: ",.;".to_string(),
                    group_key: "group".to_string(),
                    note_marks: reading::NOTE_MARKS.to_string(),
                },
            ]);
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            // `Named` reads both lines, so the second name is the whole line; the connector reader
            // reads only the second. What matters is that one name is one row.
            let names: Vec<String> = got.iter().map(content).collect();
            assert_eq!(names.iter().filter(|n| *n == "Acme AM").count(), 1);
        }
    }

    mod the_page_that_declares_nothing {
        use super::*;

        #[test]
        fn it_writes_no_block_at_all() {
            let previous = vec![holding("Alpha Fund")];
            let pipe = TextFilterInvManagers::new(vec![Reader::default()]);
            let got = pipe.call(&[], &FilterData::Previous(&previous));
            assert!(got.is_empty());
        }

        #[test]
        fn a_page_with_only_a_management_company_still_writes_its_residual() {
            let previous = vec![holding("Alpha Fund")];
            let blocks = [manco_block("Acme SGR")];
            let mut pipe = TextFilterInvManagers::new(vec![Reader::default()]);
            pipe.manco = MancoSource::Blocks(BlockType::MANAGEMENT_COMPANY);
            let got = pipe.call(&blocks, &FilterData::Previous(&previous));
            assert_eq!(wrote(&got), [("INVESTMENTS_MANAGER".to_string(), "Acme SGR".to_string())]);
        }
    }

    mod the_formats_own_notes {
        use super::*;

        #[test]
        fn they_are_warned_once_and_not_once_per_page() {
            let previous = vec![holding("Alpha Fund")];
            let mut pipe = TextFilterInvManagers::new(vec![Reader::default()]);
            pipe.notes = vec!["this report calls the role by its own name".to_string()];
            let data = FilterData::Previous(&previous);
            pipe.call(&[manager_block("Acme AM")], &data);
            assert!(pipe.warned.load(Ordering::Relaxed));
            // The second page finds the flag already set, which is what "once per document" means.
            pipe.call(&[manager_block("Acme AM")], &data);
            assert!(pipe.warned.load(Ordering::Relaxed));
        }
    }
}
