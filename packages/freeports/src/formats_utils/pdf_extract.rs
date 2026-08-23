//! Utility per il segmento pdf_extract.
//!
//! `standard_funcs` resta uno STUB: dipende dai trait `Pdf*Pipe` di M5, fuori scope per M3
//! (`PLAN.md` §11). Tutto il resto del sottoalbero è implementato.

pub mod commons;
pub mod pdf_line;
pub mod position;
pub mod relative;
pub mod select;
pub mod standard_funcs;
pub mod tabularizer;
