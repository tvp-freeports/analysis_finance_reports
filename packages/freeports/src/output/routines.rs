//! Assembling the results and writing the output files.
//!
//! Two submodules with an error type each, rather than one covering two conceptually different
//! phases:
//!
//! - [`mod@accumulate`] — from the engine's per-document outcomes to tables, with the promises resolved;
//! - [`mod@write`] — those tables onto disk.

pub mod accumulate;
pub mod write;

pub use accumulate::{AccumulateError, TransformedTables, accumulate};
pub use write::{OutFlags, OutStructureMode, WriteFilesError, write_files};
