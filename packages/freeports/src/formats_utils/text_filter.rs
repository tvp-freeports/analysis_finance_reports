//! Building blocks for the `text_filter` segment: keeping only the blocks that concern us.
//!
//! `matcher` decides whether a piece of text names one of the target companies,
//! `standard_txt_blk_builders` turns a surviving PDF block into a text block, and `standard_funcs`
//! combines the two into the ready-made filtering pipes.

pub mod matcher;
pub mod standard_funcs;
pub mod standard_txt_blk_builders;
