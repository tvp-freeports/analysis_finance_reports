//! Building blocks for the `deserialize` segment: turning text blocks into entities.
//!
//! `cast` holds the conversions from raw text to typed values — numbers written in six locales,
//! dates in as many formats, percentages that may or may not carry their sign — and
//! `standard_funcs` the pipes that use them to build the entities of `crate::output::classes`.
//!
//! `carryover` is the odd one: its pipe builds no entity at all, it deposits the promise the first
//! page of a two-page statement leaves to the second. It belongs to `U7`, whose other half is
//! `text_filter::carryover`.

pub mod carryover;
pub mod esg;
pub mod renaming;
pub mod cast;
pub mod standard_funcs;
