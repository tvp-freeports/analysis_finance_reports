//! The shims of the utilities a format author composes into their own pipes.
//!
//! Three submodules, one per pipeline segment: [`pdf_extract`] for line selections, geometry and
//! tables, [`text_filter`] for normalisation, fund-name comparison and currencies, and
//! [`deserialize`] for the casts and the two decorators that restrict a deserializer to certain
//! block types.

pub mod deserialize;
pub mod pdf_extract;
pub mod text_filter;
