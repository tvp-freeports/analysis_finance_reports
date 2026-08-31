//! Reusable implementations for the three segments of a pipeline.
//!
//! A format author picks from here and configures, instead of implementing a pipe from scratch; the
//! three submodules mirror the three segments.

pub mod deserialize;
pub mod pdf_extract;
pub mod text_filter;
