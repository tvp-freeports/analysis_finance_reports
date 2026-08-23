//! Utility per il segmento deserialize.
//!
//! `cast` e' completo (M4). `standard_funcs` copre solo `DeserializerPageClassifyStandard` —
//! l'unica pipe di deserializzazione che non costruisce un'entità di `output::classes`, e che da
//! M5 implementa anche il trait `DeserializePipe` del motore.
//!
//! Tutte le altre (`DeserializeSfdrArticleStandard`, `DeserializerFundStandard`,
//! `DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`,
//! `DeserializerInvestmentsManagerStandard`, `DeserializerInvestmentStandard`,
//! `DeserializerAssetsStandard`) costruiscono `Fund`/`Equity`/`Bond`/`ManagementCompany`/
//! `InvestmentsManager`/`FundAssets`/`FundSfdrClassification`: sono **tutte e sole** bloccate da
//! `output::classes` (M8), che dopo la chiusura di M5 è l'unica dipendenza rimasta di M4 — vedi
//! `agent-memory/M4-implementation-plan.md` §0 e `agent-memory/M5-implementation-plan.md` §4.

pub mod cast;
pub mod standard_funcs;
