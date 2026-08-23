//! Utility per il segmento deserialize.
//!
//! `cast` e' completo (M4). `standard_funcs` copre solo `DeserializerPageClassifyStandard`, il
//! sottoinsieme autosufficiente da `FilterData`/`output::classes` — le altre pipe di
//! deserializzazione (`DeserializeSfdrArticleStandard`, `DeserializerFundStandard`,
//! `DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`/
//! `DeserializerInvestmentsManagerStandard`) restano lavoro di M8, quando `output::classes`
//! esistera' — vedi `agent-memory/M4-implementation-plan.md` §0.

pub mod cast;
pub mod standard_funcs;
