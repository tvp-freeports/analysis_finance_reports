//! From the [`Assignment`] to the blocks the `text_filter` returns.
//!
//! Two functions and no decision: the decision is all in [`super::assignment`], and this module
//! translates it into the engine's three standard text blocks. It sits apart because the seven
//! branches of the assignment are proved on data, without building a block.

use std::collections::BTreeSet;

use crate::core::classes::{PdfBlock, TextBlock};
use crate::core::match_fund::MatchFund;

use super::assignment::{Assignment, Manco};
use crate::formats_utils::text_filter::standard_txt_blk_builders::{
    standard_fund_txt_blk, standard_fund_txt_blk_from_content, standard_investmet_manager_txt_blk,
    standard_investmet_manager_txt_blk_from_content, standard_management_company_txt_blk,
    standard_management_company_txt_blk_from_content,
};

/// The managers' blocks, plus one for every sub-fund the job had not yet seen.
///
/// A manager read off a block keeps it, so that the output row knows which page it comes from; one
/// built from a name — the management company passed as a string, for instance — does not.
///
/// New funds have to be declared here, or the assignments naming them fall into nothing: the `Fund`
/// does not exist yet, and a row of `investments_managers_to_funds.csv` without its fund cannot be
/// written.
pub fn text_blocks(assignment: &Assignment) -> Vec<TextBlock> {
    let mut blocks: Vec<TextBlock> = assignment
        .managers
        .iter()
        .map(|manager| match &manager.block {
            Some(block) => standard_investmet_manager_txt_blk(block.clone(), &manager.funds),
            None => standard_investmet_manager_txt_blk_from_content(&manager.name, &manager.funds),
        })
        .collect();
    // In name order, and not the one the set gives back: output identifiers are assigned in the
    // order the rows arrive, and a frozen fixture has to see the same order on every run.
    blocks.extend(
        assignment
            .new_funds
            .iter()
            .map(|fund| fund_block(fund, assignment)),
    );
    blocks
}

/// The new sub-fund, with the page it comes from when there is a block to give it one.
///
/// A `funds.csv` with an empty `Report page` column exists in none of the reference runs, and must
/// not start existing here: the sub-fund is named by the managers' section, so that page is where it
/// comes from. The block is taken from the first manager that has one — they are all from the same
/// page — and if there is none, the bare name is the fallback.
fn fund_block(fund: &MatchFund, assignment: &Assignment) -> TextBlock {
    let page_block = assignment.managers.iter().find_map(|m| m.block.as_ref());
    match page_block {
        None => standard_fund_txt_blk_from_content(fund.name()),
        Some(block) => standard_fund_txt_blk(PdfBlock::bare(block.type_block.clone(), fund.name())),
    }
}

/// The management company's block, with **every** fund of the document.
///
/// `R26`: the management company is one-to-many over the whole document — the identifier is in
/// `funds.csv` — while `inv_managers` is many-to-many in a table of its own. That the same company
/// appears in the two roles is not a duplicate: it is the normal case of the formats that name no
/// delegated manager.
///
/// It is called only where the same pipeline reads the management company too; where the `manco` has
/// a pipeline of its own, that one writes this block.
pub fn management_company_block(funds: &BTreeSet<MatchFund>, manco: &Manco) -> TextBlock {
    match manco {
        Manco::Name(name) => standard_management_company_txt_blk_from_content(name.trim(), funds),
        Manco::Block(block) => standard_management_company_txt_blk(block.clone(), funds),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::BlockType;
    use crate::core::classes::value::BlockValue;
    use crate::formats_utils::text_filter::inv_managers::assignment::{Declared, assign, fund_set};

    fn three() -> BTreeSet<MatchFund> {
        fund_set(["Alpha Fund", "Beta Fund", "Gamma Fund"])
    }

    /// The names the block declares as managed, as written.
    fn managed(block: &TextBlock) -> Vec<String> {
        match block.metadata.get("managed_funds") {
            Some(BlockValue::Set(names)) => names
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn content(block: &TextBlock) -> String {
        block.content.as_str().unwrap_or_default().to_string()
    }

    mod the_managers_blocks {
        use super::*;

        #[test]
        fn one_block_per_manager_carrying_its_funds() {
            let declared = vec![Declared::new("Acme AM").with_funds(["Alpha Fund"])];
            let assignment = assign(declared, &three(), None, &BTreeSet::new(), false);
            let got = text_blocks(&assignment);
            assert_eq!(got.len(), 1);
            assert_eq!(got[0].type_block, BlockType::INVESTMENTS_MANAGER);
            assert_eq!(managed(&got[0]), ["Alpha Fund"]);
        }

        #[test]
        fn a_manager_read_off_a_page_keeps_the_block_it_came_from() {
            let block = PdfBlock::bare(BlockType::INVESTMENTS_MANAGER, "Acme AM");
            let declared = vec![Declared::from_block(block).with_funds(["Alpha Fund"])];
            let assignment = assign(declared, &three(), None, &BTreeSet::new(), false);
            let got = text_blocks(&assignment);
            let kept = got[0].pdf_block.as_ref().expect("the page is kept");
            assert_eq!(kept.content.as_str(), Some("Acme AM"));
        }

        #[test]
        fn a_manager_built_from_a_name_has_no_page_to_keep() {
            let manco = Manco::Name("Acme SGR".to_string());
            let assignment = assign(Vec::new(), &three(), Some(&manco), &BTreeSet::new(), true);
            let got = text_blocks(&assignment);
            assert_eq!(content(&got[0]), "Acme SGR");
            assert_eq!(got[0].pdf_block, None);
        }

        #[test]
        fn a_manager_declaring_no_fund_at_all_never_reaches_a_block() {
            // An empty fund set is disjoint from the job's funds, so the declaration is noise and
            // `assign` drops it before any block is built.
            let declared = vec![Declared::new("Acme AM").with_funds(Vec::<String>::new())];
            let assignment = assign(declared, &three(), None, &BTreeSet::new(), false);
            assert!(text_blocks(&assignment).is_empty());
        }

        #[test]
        fn every_manager_block_carries_the_managed_funds_key() {
            let declared = vec![Declared::new("Acme AM").with_funds(["Alpha Fund"])];
            let assignment = assign(declared, &three(), None, &BTreeSet::new(), false);
            let got = text_blocks(&assignment);
            assert!(got[0].metadata.contains_key("managed_funds"));
        }
    }

    mod the_new_funds {
        use super::*;

        #[test]
        fn a_fund_the_job_had_not_seen_gets_a_block_of_its_own() {
            let declared = vec![Declared::new("Acme AM").with_funds(["Alpha Fund", "Delta Fund"])];
            let assignment = assign(declared, &three(), None, &BTreeSet::new(), false);
            let got = text_blocks(&assignment);
            assert_eq!(got.len(), 2);
            assert_eq!(got[1].type_block, BlockType::FUND);
            assert_eq!(content(&got[1]), "Delta Fund");
        }

        #[test]
        fn it_carries_the_page_the_managers_section_was_read_from() {
            // A `funds.csv` row with an empty `Report page` exists in no reference run.
            let block = PdfBlock::bare(BlockType::from("MANAGER".to_string()), "Acme AM");
            let declared =
                vec![Declared::from_block(block).with_funds(["Alpha Fund", "Delta Fund"])];
            let assignment = assign(declared, &three(), None, &BTreeSet::new(), false);
            let got = text_blocks(&assignment);
            let kept = got[1]
                .pdf_block
                .as_ref()
                .expect("the new fund keeps a page");
            assert_eq!(kept.type_block, BlockType::from("MANAGER".to_string()));
            assert_eq!(kept.content.as_str(), Some("Delta Fund"));
        }

        #[test]
        fn with_no_block_anywhere_the_bare_name_is_the_fallback() {
            let declared = vec![Declared::new("Acme AM").with_funds(["Alpha Fund", "Delta Fund"])];
            let assignment = assign(declared, &three(), None, &BTreeSet::new(), false);
            let got = text_blocks(&assignment);
            assert_eq!(got[1].pdf_block, None);
            assert_eq!(content(&got[1]), "Delta Fund");
        }

        #[test]
        fn they_come_out_in_name_order_and_not_the_sets_own() {
            // Output identifiers are handed out in the order the rows arrive, so a frozen fixture
            // needs the same order on every run.
            let declared = vec![Declared::new("Acme AM").with_funds([
                "Zeta Fund",
                "Delta Fund",
                "Alpha Fund",
            ])];
            let assignment = assign(declared, &three(), None, &BTreeSet::new(), false);
            let got = text_blocks(&assignment);
            let new_ones: Vec<String> = got.iter().skip(1).map(content).collect();
            assert_eq!(new_ones, ["Delta Fund", "Zeta Fund"]);
        }
    }

    mod the_management_companys_own_row {
        use super::*;

        #[test]
        fn it_covers_every_fund_of_the_document() {
            // `R26`: one to many over the whole document, holdings and new funds alike.
            let declared = vec![Declared::new("Acme AM").with_funds(["Alpha Fund", "Delta Fund"])];
            let assignment = assign(declared, &three(), None, &BTreeSet::new(), false);
            let manco = Manco::Name("Acme SGR".to_string());
            let got = management_company_block(&assignment.funds, &manco);
            assert_eq!(got.type_block, BlockType::MANAGEMENT_COMPANY);
            assert_eq!(managed(&got).len(), 4);
        }

        #[test]
        fn a_company_read_off_the_page_keeps_it() {
            let block = PdfBlock::bare(BlockType::MANAGEMENT_COMPANY, "Acme SGR");
            let got = management_company_block(&three(), &Manco::Block(block));
            let kept = got.pdf_block.as_ref().expect("the page is kept");
            assert_eq!(kept.content.as_str(), Some("Acme SGR"));
        }

        #[test]
        fn a_company_named_by_the_format_is_trimmed() {
            let got = management_company_block(&three(), &Manco::Name("  Acme SGR  ".to_string()));
            assert_eq!(content(&got), "Acme SGR");
        }
    }
}
