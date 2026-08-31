//! The shims of the twenty-one ready-made pipes a formats repository composes.
//!
//! # Twenty-one names, three types
//!
//! In Rust the pipes are one trait object per segment, so the public names here are **functions**
//! building one of the three wrappers rather than twenty-one classes. From Python the difference is
//! invisible, and in exchange the layer does not duplicate the same wrapper twenty-one times.
//!
//! # The signatures are the ones author code already uses
//!
//! Author modules are already written and must be callable as they are: where a native signature
//! diverges from the Python one — arguments grouped into a struct, a flag in place of a callable,
//! arguments accepted and thrown away — it is **this** layer that bridges, not the author modules
//! that adapt. Each divergence is documented on the constructor absorbing it.

pub mod deserialize;
pub mod pdf_extract;
pub mod text_filter;
