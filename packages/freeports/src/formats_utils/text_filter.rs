//! Utility per il segmento text_filter.
//!
//! `matcher` e `standard_txt_blk_builders` sono completi (M4). `standard_funcs` copre solo
//! `TextFilterPageClassifyStandard`/`extract_currency_from_text`, il sottoinsieme autosufficiente
//! da `FilterData` — le altre quattro pipe (`TextFilterSfdrArticleStandard`,
//! `TextFilterManagmentCompanyStandard`, `TextFilterAssetsStandard`,
//! `TextFilterInvestmentsStandard`) leggono `filter_data` e restano lavoro di M5 — vedi
//! `agent-memory/M4-implementation-plan.md` §0.

pub mod matcher;
pub mod standard_funcs;
pub mod standard_txt_blk_builders;
