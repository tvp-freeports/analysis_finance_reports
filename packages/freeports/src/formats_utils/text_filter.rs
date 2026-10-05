//! Building blocks for the `text_filter` segment: keeping only the blocks that concern us.
//!
//! `matcher` decides whether a piece of text names one of the target companies,
//! `standard_txt_blk_builders` turns a surviving PDF block into a text block, `dash_as_zero` says
//! which of a row's numeric fields read the report's dash as the zero it means, and
//! `standard_funcs` combines them into the ready-made filtering pipes.
//!
//! Three modules are whole algorithms rather than building blocks, each named in the
//! `semistructured` level's registry: `declared_funds` is `R30`, `managment_company` reads the
//! company out of the line that declares it, and `inv_managers` is `A5` — the one place where the
//! funds a report does not attribute to anybody are assigned.

pub mod dash_as_zero;
pub mod declared_funds;
pub mod inv_managers;
pub mod managment_company;
pub mod matcher;
pub mod standard_funcs;
pub mod standard_txt_blk_builders;
