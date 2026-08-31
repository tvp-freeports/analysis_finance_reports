//! The configuration sources, in order of precedence.
//!
//! Each submodule reads one source into a [`super::partial_config::PartialConfig`] — partial,
//! because no single source is required to say everything. Merging them and resolving what is left
//! is [`super::freeports_config`]'s job.
//!
//! Precedence runs from the most specific to the most general: the command line beats the
//! environment, which beats the configuration file.

pub mod cmd;
pub mod env;
pub mod file;
