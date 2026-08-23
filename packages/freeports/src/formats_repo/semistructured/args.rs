//! Gli argomenti YAML del livello semistructured e la loro risoluzione per riga.
//!
//! Un file per segmento, `args/{segmento}.yaml`, con una chiave per pipe. La chiave è
//! `"{formato}({pipeline})"`; quando la pipeline è quella **senza nome**, e solo allora, si
//! ripiega sulla chiave nuda `"{formato}"`.
//!
//! Se il valore trovato è una **lista**, l'elemento da usare è scelto per posizione — ma la
//! posizione è il numero di pipe già emessi per quella `(pipeline, segmento)`, non l'indice della
//! riga CSV. È una distinzione che conta davvero: un algoritmo che restituisce tre pipe fa
//! avanzare il contatore di tre, non di uno. `PLAN.md` §6.2 lo dice espressamente, ed è il
//! comportamento del riferimento.

use std::path::{Path, PathBuf};

use super::SegmentKind;

/// Fallimenti nella lettura o risoluzione degli argomenti.
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

/// Il percorso del file argomenti di un segmento.
pub fn args_path(formats_repo_dir: &Path, segment: SegmentKind) -> PathBuf {
    formats_repo_dir
        .join(super::formats_mapping::SEMISTRUCTURED_DIR)
        .join("args")
        .join(format!("{}.yaml", segment.file_stem()))
}

/// Carica il file argomenti di un segmento.
///
/// Tutti e tre i file sono letti da ogni caricamento, anche per un segmento che il formato
/// richiesto non usa: è il comportamento del riferimento, e rende un file mancante un errore di
/// configurazione del repo invece di una sorpresa a metà caricamento. Un file **vuoto** è invece
/// legittimo (i due file `text_filter.yaml`/`deserialize.yaml` del repo italiano contengono `{}`).
pub fn load(formats_repo_dir: &Path, segment: SegmentKind) -> Result<serde_yaml::Value, ArgsError> {
    let path = args_path(formats_repo_dir, segment);
    if !path.is_file() {
        return Err(ArgsError::MissingYaml(path));
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| ArgsError::Unreadable { path: path.clone(), reason: e.to_string() })?;
    serde_yaml::from_str(&content).map_err(|e| ArgsError::Unreadable { path, reason: e.to_string() })
}

/// La chiave con cui un pipe cerca i propri argomenti.
pub fn algorithm_id(format_name: &str, pipeline_name: &str) -> String {
    format!("{format_name}({pipeline_name})")
}

/// Gli argomenti dichiarati per un pipe, prima della scelta posizionale.
///
/// La ricerca è per `"{formato}({pipeline})"`; il ripiego sulla chiave nuda `"{formato}"` vale
/// **solo** per la pipeline senza nome, perché altrimenti due pipeline diverse dello stesso
/// formato finirebbero per condividere gli stessi argomenti.
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

/// L'argomento di *questo* pipe fra quelli dichiarati.
///
/// Se il valore è una lista, si sceglie per posizione (vedi il doc-comment del modulo per cosa
/// conta la posizione); altrimenti il valore è l'argomento, uguale per ogni pipe che lo usi.
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
