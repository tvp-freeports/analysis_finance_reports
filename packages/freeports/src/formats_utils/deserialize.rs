//! Building blocks for the `deserialize` segment: turning text blocks into entities.
//!
//! `cast` holds the conversions from raw text to typed values — numbers written in six locales,
//! dates in as many formats, percentages that may or may not carry their sign — and
//! `standard_funcs` the pipes that use them to build the entities of `crate::output::classes`.

pub mod cast;
pub mod standard_funcs;
