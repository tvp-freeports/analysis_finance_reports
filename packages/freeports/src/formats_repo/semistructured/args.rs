//! The semistructured YAML arguments, and how a row picks its own.
//!
//! One file per segment, with a key per pipe. The key is `"{format}({pipeline})"`; only when the
//! pipeline is the **unnamed** one does it fall back to the bare `"{format}"`.
//!
//! When the value found is a **list**, the element to use is chosen by position — and the position
//! is the number of pipes already emitted for that `(pipeline, segment)`, not the row's index in
//! the mapping table. The distinction is real: an algorithm returning three pipes advances the
//! counter by three, not by one.

use std::path::{Path, PathBuf};

use super::SegmentKind;

/// Failures of reading or resolving the arguments.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error("missing formats-repository file: {0}")]
    MissingYaml(PathBuf),
    #[error("{path}: {reason}")]
    Unreadable { path: PathBuf, reason: String },
    #[error("no args entry found for algorithm id '{algorithm_id}'")]
    MissingArgs { algorithm_id: String },
    #[error("malformed args for algorithm id '{algorithm_id}': {message}")]
    MalformedArgs { algorithm_id: String, message: String },
}

/// The path of a segment's arguments file.
pub fn args_path(formats_repo_dir: &Path, segment: SegmentKind) -> PathBuf {
    formats_repo_dir
        .join(super::formats_mapping::SEMISTRUCTURED_DIR)
        .join("args")
        .join(format!("{}.yaml", segment.file_stem()))
}

/// Loads a segment's arguments file.
///
/// All three files are read on every load, even for a segment the requested format does not use,
/// which makes a missing file a repository configuration error rather than a surprise halfway
/// through loading. An **empty** file is legitimate.
pub fn load(formats_repo_dir: &Path, segment: SegmentKind) -> Result<serde_yaml::Value, ArgsError> {
    let path = args_path(formats_repo_dir, segment);
    if !path.is_file() {
        return Err(ArgsError::MissingYaml(path));
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| ArgsError::Unreadable { path: path.clone(), reason: e.to_string() })?;
    serde_yaml::from_str(&content).map_err(|e| ArgsError::Unreadable { path, reason: e.to_string() })
}

/// The key a pipe looks its arguments up under.
pub fn algorithm_id(format_name: &str, pipeline_name: &str) -> String {
    format!("{format_name}({pipeline_name})")
}

/// The arguments declared for a pipe, before the positional choice.
///
/// The fallback to the bare format key applies **only** to the unnamed pipeline; otherwise two
/// different pipelines of one format would end up sharing arguments.
pub fn lookup<'a>(
    args: &'a serde_yaml::Value,
    format_name: &str,
    pipeline_name: &str,
) -> Result<&'a serde_yaml::Value, ArgsError> {
    let id = algorithm_id(format_name, pipeline_name);
    if let Some(value) = args.get(&id) {
        return Ok(value);
    }
    if pipeline_name.is_empty()
        && let Some(value) = args.get(format_name)
    {
        return Ok(value);
    }
    Err(ArgsError::MissingArgs { algorithm_id: id })
}

/// This pipe's argument among those declared.
///
/// A list value is indexed by position (see the module documentation for what the position counts);
/// any other value is the argument itself, the same for every pipe using it.
pub fn positional<'a>(
    selected: &'a serde_yaml::Value,
    already_emitted: usize,
    algorithm_id: &str,
) -> Result<&'a serde_yaml::Value, ArgsError> {
    match selected.as_sequence() {
        Some(items) => items.get(already_emitted).ok_or_else(|| ArgsError::MalformedArgs {
            algorithm_id: algorithm_id.to_string(),
            message: format!("stacked args list has {} entries, needed index {already_emitted}", items.len()),
        }),
        None => Ok(selected),
    }
}
