//! Building blocks for the `text_filter` segment: keeping only the blocks that concern us.
//!
//! `matcher` decides whether a piece of text names one of the target companies,
//! `standard_txt_blk_builders` turns a surviving PDF block into a text block, `dash_as_zero` says
//! which of a row's numeric fields read the report's dash as the zero it means, and
//! `standard_funcs` combines them into the ready-made filtering pipes.

pub mod dash_as_zero;
pub mod matcher;
pub mod standard_funcs;
pub mod standard_txt_blk_builders;
