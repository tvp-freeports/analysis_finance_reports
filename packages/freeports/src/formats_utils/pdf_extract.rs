//! Building blocks for the `pdf_extract` segment: turning a page into blocks.
//!
//! The tree goes from the general to the specific: `pdf_line` and `select` pick lines out of a
//! page, `position` and `tabularizer` recover rows, columns and tables from their geometry, and
//! `standard_funcs` assembles those into the ready-made pipes a format names in its configuration.

pub mod commons;
pub mod pdf_line;
pub mod position;
pub mod relative;
pub mod select;
pub mod standard_funcs;
pub mod tabularizer;
