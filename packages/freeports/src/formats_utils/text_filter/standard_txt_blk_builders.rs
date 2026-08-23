//! Costruttori di `TextBlock` standard (fondo, societa' di gestione, gestore degli investimenti).
//!
//! Port di `freeports_core/src/formats_utils/text_filter/standard_txt_blks.rs`, **senza** gli
//! enum `OneTextBlockType`/`ResultStandardFiltering` del riferimento: in questo crate quel ruolo
//! lo fa gia' [`crate::core::classes::BlockType`] (costanti associate, M2) — vedi
//! `agent-memory/M4-implementation-plan.md` §3.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub fn standard_fund_txt_blk(pdf_blk: PdfBlock) -> TextBlock;                 // BlockType::FUND
//! pub fn standard_fund_txt_blk_from_content(fund: &str) -> TextBlock;
//!
//! pub fn standard_management_company_txt_blk(pdf_blk: PdfBlock, funds: &BTreeSet<MatchFund>)
//!     -> TextBlock;                                             // BlockType::MANAGEMENT_COMPANY
//! pub fn standard_management_company_txt_blk_from_content(name: &str, funds: &BTreeSet<MatchFund>)
//!     -> TextBlock;
//!
//! pub fn standard_investmet_manager_txt_blk(pdf_blk: PdfBlock, funds: &BTreeSet<MatchFund>)
//!     -> TextBlock;                                             // BlockType::INVESTMENTS_MANAGER
//! pub fn standard_investmet_manager_txt_blk_from_content(name: &str, funds: &BTreeSet<MatchFund>)
//!     -> TextBlock;
//! ```
//!
//! (nomi esatti presi da `PLAN.md` §9: `standard_management_company_txt_blk`,
//! `standard_investmet_manager_txt_blk` — sic, refuso "investmet" del riferimento originale
//! mantenuto perche' e' il nome pubblico richiesto esplicitamente.)
//!
//! - `standard_fund_txt_blk`/`_from_content` non prendono `funds` (nessuna metadata), esattamente
//!   come il riferimento.
//! - Le due coppie `management_company`/`investmet_manager` scrivono in `metadata` una sola
//!   chiave, `"managed_funds"`, il cui valore e' `BlockValue::Set` dei **nomi come scritti**
//!   (`MatchFund::name()`, non `MatchFund::normalized()`) avvolti in `BlockValue::Str` — un
//!   `BTreeSet<MatchFund>` vuoto da' un `BlockValue::Set` vuoto, non un campo assente.
//! - `standard_*_txt_blk` (variante `PdfBlock`) usa `TextBlock::new` (eredita `content` dal
//!   `pdf_blk` dato, e lo conserva in `TextBlock::pdf_block`); `standard_*_txt_blk_from_content`
//!   usa `TextBlock::from_content` (nessun `pdf_block`, `content` e' la stringa data).

use std::collections::{BTreeMap, BTreeSet};

use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock, TextBlock};
use crate::core::match_fund::MatchFund;

/// `{"managed_funds": <BlockValue::Set dei nomi scritti>}` — condivisa dalle quattro varianti
/// "management_company"/"investmet_manager".
fn managed_funds_metadata(funds: &BTreeSet<MatchFund>) -> BTreeMap<String, BlockValue> {
    let names: BTreeSet<BlockValue> =
        funds.iter().map(|f| BlockValue::Str(f.name().to_string())).collect();
    BTreeMap::from([("managed_funds".to_string(), BlockValue::Set(names))])
}

pub fn standard_fund_txt_blk(pdf_blk: PdfBlock) -> TextBlock {
    TextBlock::new(BlockType::FUND, BTreeMap::new(), pdf_blk)
}

pub fn standard_fund_txt_blk_from_content(fund: &str) -> TextBlock {
    TextBlock::from_content(BlockType::FUND, BTreeMap::new(), fund)
}

pub fn standard_management_company_txt_blk(pdf_blk: PdfBlock, funds: &BTreeSet<MatchFund>) -> TextBlock {
    TextBlock::new(BlockType::MANAGEMENT_COMPANY, managed_funds_metadata(funds), pdf_blk)
}

pub fn standard_management_company_txt_blk_from_content(
    name: &str,
    funds: &BTreeSet<MatchFund>,
) -> TextBlock {
    TextBlock::from_content(BlockType::MANAGEMENT_COMPANY, managed_funds_metadata(funds), name)
}

pub fn standard_investmet_manager_txt_blk(pdf_blk: PdfBlock, funds: &BTreeSet<MatchFund>) -> TextBlock {
    TextBlock::new(BlockType::INVESTMENTS_MANAGER, managed_funds_metadata(funds), pdf_blk)
}

pub fn standard_investmet_manager_txt_blk_from_content(
    name: &str,
    funds: &BTreeSet<MatchFund>,
) -> TextBlock {
    TextBlock::from_content(BlockType::INVESTMENTS_MANAGER, managed_funds_metadata(funds), name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::{BlockType, PdfBlock};
    use crate::core::classes::value::BlockValue;
    use crate::core::match_fund::MatchFund;
    use std::collections::BTreeSet;

    fn some_pdf_block(content: &str) -> PdfBlock {
        PdfBlock::bare(BlockType::new("SOME_PDF_TYPE"), content)
    }

    fn funds(names: &[&str]) -> BTreeSet<MatchFund> {
        names.iter().map(|n| MatchFund::new(*n)).collect()
    }

    fn managed_fund_names(txt: &crate::core::classes::TextBlock) -> BTreeSet<BlockValue> {
        txt.metadata
            .get("managed_funds")
            .expect("managed_funds metadata must always be present")
            .as_set()
            .expect("managed_funds must be a BlockValue::Set")
            .clone()
    }

    mod fund {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn type_block_is_fund() {
            let txt = standard_fund_txt_blk(some_pdf_block("Acme Growth Fund"));
            assert_eq!(txt.type_block, BlockType::FUND);
        }

        #[test]
        fn metadata_is_empty() {
            let txt = standard_fund_txt_blk(some_pdf_block("Acme Growth Fund"));
            assert!(txt.metadata.is_empty());
        }

        #[test]
        fn content_comes_from_the_pdf_block() {
            let txt = standard_fund_txt_blk(some_pdf_block("Acme Growth Fund"));
            assert_eq!(txt.content.as_str(), Some("Acme Growth Fund"));
        }

        #[test]
        fn keeps_the_originating_pdf_block() {
            let pdf = some_pdf_block("Acme Growth Fund");
            let txt = standard_fund_txt_blk(pdf.clone());
            assert_eq!(txt.pdf_block.as_deref(), Some(&pdf));
        }
    }

    mod fund_from_content {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn type_block_is_fund() {
            let txt = standard_fund_txt_blk_from_content("Café Fund");
            assert_eq!(txt.type_block, BlockType::FUND);
        }

        #[test]
        fn metadata_is_empty() {
            let txt = standard_fund_txt_blk_from_content("Café Fund");
            assert!(txt.metadata.is_empty());
        }

        #[test]
        fn content_is_the_given_string() {
            let txt = standard_fund_txt_blk_from_content("Café Fund");
            assert_eq!(txt.content.as_str(), Some("Café Fund"));
        }

        #[test]
        fn has_no_pdf_block() {
            let txt = standard_fund_txt_blk_from_content("Café Fund");
            assert!(txt.pdf_block.is_none());
        }
    }

    mod management_company {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn type_block_is_management_company() {
            let txt = standard_management_company_txt_blk(some_pdf_block("Acme AM"), &funds(&[]));
            assert_eq!(txt.type_block, BlockType::MANAGEMENT_COMPANY);
        }

        #[test]
        fn content_comes_from_the_pdf_block() {
            let txt = standard_management_company_txt_blk(some_pdf_block("Acme AM"), &funds(&[]));
            assert_eq!(txt.content.as_str(), Some("Acme AM"));
        }

        #[test]
        fn keeps_the_originating_pdf_block() {
            let pdf = some_pdf_block("Acme AM");
            let txt = standard_management_company_txt_blk(pdf.clone(), &funds(&[]));
            assert_eq!(txt.pdf_block.as_deref(), Some(&pdf));
        }

        #[test]
        fn no_funds_gives_an_empty_managed_funds_set_not_a_missing_field() {
            let txt = standard_management_company_txt_blk(some_pdf_block("Acme AM"), &funds(&[]));
            assert_eq!(managed_fund_names(&txt), BTreeSet::new());
        }

        #[test]
        fn a_single_managed_fund_is_reported_by_its_written_name() {
            let txt =
                standard_management_company_txt_blk(some_pdf_block("Acme AM"), &funds(&["Acme Growth Fund"]));
            let expected: BTreeSet<BlockValue> =
                [BlockValue::Str("Acme Growth Fund".to_string())].into_iter().collect();
            assert_eq!(managed_fund_names(&txt), expected);
        }

        #[test]
        fn multiple_managed_funds_with_unicode_names_are_kept_as_written_not_normalized() {
            let txt = standard_management_company_txt_blk(
                some_pdf_block("Acme AM"),
                &funds(&["Acme Growth Fund", "Café Balanced Fund", "Ómega Bond Fund"]),
            );
            let expected: BTreeSet<BlockValue> = [
                BlockValue::Str("Acme Growth Fund".to_string()),
                BlockValue::Str("Café Balanced Fund".to_string()),
                BlockValue::Str("Ómega Bond Fund".to_string()),
            ]
            .into_iter()
            .collect();
            assert_eq!(managed_fund_names(&txt), expected);
        }
    }

    mod management_company_from_content {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn type_block_is_management_company() {
            let txt = standard_management_company_txt_blk_from_content("Acme AM", &funds(&[]));
            assert_eq!(txt.type_block, BlockType::MANAGEMENT_COMPANY);
        }

        #[test]
        fn content_is_the_given_string() {
            let txt = standard_management_company_txt_blk_from_content("Acme AM", &funds(&[]));
            assert_eq!(txt.content.as_str(), Some("Acme AM"));
        }

        #[test]
        fn has_no_pdf_block() {
            let txt = standard_management_company_txt_blk_from_content("Acme AM", &funds(&[]));
            assert!(txt.pdf_block.is_none());
        }

        #[test]
        fn no_funds_gives_an_empty_managed_funds_set_not_a_missing_field() {
            let txt = standard_management_company_txt_blk_from_content("Acme AM", &funds(&[]));
            assert_eq!(managed_fund_names(&txt), BTreeSet::new());
        }

        #[test]
        fn multiple_managed_funds_are_all_reported() {
            let txt = standard_management_company_txt_blk_from_content(
                "Acme AM",
                &funds(&["Acme Growth Fund", "Acme Bond Fund"]),
            );
            let expected: BTreeSet<BlockValue> = [
                BlockValue::Str("Acme Growth Fund".to_string()),
                BlockValue::Str("Acme Bond Fund".to_string()),
            ]
            .into_iter()
            .collect();
            assert_eq!(managed_fund_names(&txt), expected);
        }
    }

    mod investments_manager {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn type_block_is_investments_manager() {
            let txt = standard_investmet_manager_txt_blk(some_pdf_block("Acme IM"), &funds(&[]));
            assert_eq!(txt.type_block, BlockType::INVESTMENTS_MANAGER);
        }

        #[test]
        fn content_comes_from_the_pdf_block() {
            let txt = standard_investmet_manager_txt_blk(some_pdf_block("Acme IM"), &funds(&[]));
            assert_eq!(txt.content.as_str(), Some("Acme IM"));
        }

        #[test]
        fn keeps_the_originating_pdf_block() {
            let pdf = some_pdf_block("Acme IM");
            let txt = standard_investmet_manager_txt_blk(pdf.clone(), &funds(&[]));
            assert_eq!(txt.pdf_block.as_deref(), Some(&pdf));
        }

        #[test]
        fn no_funds_gives_an_empty_managed_funds_set_not_a_missing_field() {
            let txt = standard_investmet_manager_txt_blk(some_pdf_block("Acme IM"), &funds(&[]));
            assert_eq!(managed_fund_names(&txt), BTreeSet::new());
        }

        #[test]
        fn a_single_managed_fund_is_reported_by_its_written_name() {
            let txt =
                standard_investmet_manager_txt_blk(some_pdf_block("Acme IM"), &funds(&["Acme Growth Fund"]));
            let expected: BTreeSet<BlockValue> =
                [BlockValue::Str("Acme Growth Fund".to_string())].into_iter().collect();
            assert_eq!(managed_fund_names(&txt), expected);
        }

        #[test]
        fn multiple_managed_funds_with_unicode_names_are_kept_as_written_not_normalized() {
            let txt = standard_investmet_manager_txt_blk(
                some_pdf_block("Acme IM"),
                &funds(&["Acme Growth Fund", "Café Balanced Fund"]),
            );
            let expected: BTreeSet<BlockValue> = [
                BlockValue::Str("Acme Growth Fund".to_string()),
                BlockValue::Str("Café Balanced Fund".to_string()),
            ]
            .into_iter()
            .collect();
            assert_eq!(managed_fund_names(&txt), expected);
        }
    }

    mod investments_manager_from_content {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn type_block_is_investments_manager() {
            let txt = standard_investmet_manager_txt_blk_from_content("Acme IM", &funds(&[]));
            assert_eq!(txt.type_block, BlockType::INVESTMENTS_MANAGER);
        }

        #[test]
        fn content_is_the_given_string() {
            let txt = standard_investmet_manager_txt_blk_from_content("Acme IM", &funds(&[]));
            assert_eq!(txt.content.as_str(), Some("Acme IM"));
        }

        #[test]
        fn has_no_pdf_block() {
            let txt = standard_investmet_manager_txt_blk_from_content("Acme IM", &funds(&[]));
            assert!(txt.pdf_block.is_none());
        }

        #[test]
        fn no_funds_gives_an_empty_managed_funds_set_not_a_missing_field() {
            let txt = standard_investmet_manager_txt_blk_from_content("Acme IM", &funds(&[]));
            assert_eq!(managed_fund_names(&txt), BTreeSet::new());
        }

        #[test]
        fn multiple_managed_funds_are_all_reported() {
            let txt = standard_investmet_manager_txt_blk_from_content(
                "Acme IM",
                &funds(&["Acme Growth Fund", "Acme Bond Fund"]),
            );
            let expected: BTreeSet<BlockValue> = [
                BlockValue::Str("Acme Growth Fund".to_string()),
                BlockValue::Str("Acme Bond Fund".to_string()),
            ]
            .into_iter()
            .collect();
            assert_eq!(managed_fund_names(&txt), expected);
        }
    }
}
