//! Utility per il segmento text_filter.
//!
//! `matcher` e `standard_txt_blk_builders` sono completi (M4).
//!
//! `standard_funcs` è completo **per tutto ciò che non dipende da `output::classes`** (M8), che è
//! l'unica dipendenza rimasta di M4 dopo la chiusura di M5:
//!
//! - fatti: `extract_currency_from_text`, `TextFilterPageClassifyStandard` (M4) e
//!   `TextFilterInvestmentsStandard` (M5, che porta con sé `PdfBlocksTable`) — quest'ultimo legge
//!   dal `filter_data` solo le `CompanyMatchInfos`, quindi è diventato costruibile appena M5 ha
//!   introdotto `FilterData`;
//! - deferiti a M8: `TextFilterSfdrArticleStandard`, `TextFilterManagmentCompanyStandard`,
//!   `TextFilterAssetsStandard`, che dal `filter_data` estraggono `Fund`/`Equity`/`Bond`, cioè
//!   entità di `output::classes` che ancora non esistono.
//!
//! Elenco completo e motivazione in `agent-memory/M4-implementation-plan.md` §0 e in
//! `agent-memory/M5-implementation-plan.md` §4.

pub mod matcher;
pub mod standard_funcs;
pub mod standard_txt_blk_builders;
