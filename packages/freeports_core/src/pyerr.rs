//! Crate-wide marker for "a Python step failed, and the `PyErr` has already been printed in full
//! right where it was generated" — the single error type every self-attaching top-level function
//! (`cli::job::run_job`, `cli::output::write_results`, `output::routines::transform_to_files_schema`,
//! ...) converges on at its own `Python::attach` boundary, so callers further up the call stack
//! (`cli::run::execute`) only ever have to handle "this Python-touching step failed", never a raw
//! `PyErr` a second time. Living at the crate root rather than inside `cli::job` (where it first
//! appeared) makes it reachable from `output::routines`, a sibling of `cli` that could not see into
//! one of `cli`'s private submodules.

#[derive(Debug)]
pub struct PyStepFailed;
