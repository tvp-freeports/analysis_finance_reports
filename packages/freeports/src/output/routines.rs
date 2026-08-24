//! Assemblaggio dei risultati e scrittura dei file di output.
//!
//! M8, passi 12-13 (`agent-memory/M8-implementation-plan.md` §1). Split in due sottomoduli,
//! ciascuno col proprio enum d'errore, invece di un unico file/enum gigante che coprirebbe due
//! fasi concettualmente diverse (accumulo + risoluzione promesse vs. I/O):
//! - [`accumulate`] — da `&[DocumentOutcome]` a [`accumulate::TransformedTables`], con le
//!   promesse risolte;
//! - [`write`] — [`accumulate::TransformedTables`] su disco (profilo `Regular`: CSV + YAML).

pub mod accumulate;
pub mod write;

pub use accumulate::{AccumulateError, TransformedTables, accumulate};
pub use write::{OutFlags, OutStructureMode, WriteFilesError, write_files};
