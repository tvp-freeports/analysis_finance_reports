//! Il caricamento del repository dei formati e la costruzione dell'algoritmo di un formato.
//!
//! Un repo formati è una cartella con dei CSV, degli YAML e (per i formati più irregolari) del
//! codice Python. Questo modulo la legge e ne ricava un [`Algorithm`] pronto da applicare a un
//! documento.
//!
//! # I tre livelli
//!
//! Le pipeline di un formato si compongono da tre livelli di specificità crescente, che
//! **si sommano** invece di escludersi (`PLAN.md` §6.4):
//!
//! | Livello | Dove vive l'algoritmo | Dove vivono i parametri |
//! |---|---|---|
//! | [`structured`] | nella libreria, fisso | colonne di CSV |
//! | [`semistructured`] | nella libreria (per nome) o nel repo | YAML |
//! | [`unstructured`] | nel repo, in Python | nel codice stesso |
//!
//! Un formato può usarne uno, due o tutti e tre: è normale che l'estrazione sia structured e il
//! filtraggio unstructured, perché ciò che è irregolare in un documento raramente lo è in ogni
//! segmento. La fusione avviene in [`load_pipelines`], sommando le pipeline **omonime** dei tre
//! livelli, ed è l'unico punto in cui i tre si incontrano: nessun altro modulo sa che esistono.
//!
//! # I file letti
//!
//! - [`metadata`] — `metadata/formats.csv`, `metadata/url_mapping.csv`;
//! - [`orchestration`] — `content/orchestration/*.csv`;
//! - [`structured`] — `content/algorithms/structured/**`;
//! - [`semistructured`] — `content/algorithms/semistructured/**`;
//! - [`unstructured`] — `content/algorithms/unstructured/**`.
//!
//! [`id_format`] non legge nulla: è la grammatica degli identificatori che tutti gli altri
//! condividono.

pub mod id_format;
pub mod metadata;
pub mod orchestration;
pub mod semistructured;
pub mod structured;
pub mod unstructured;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::core::algorithm::{Algorithm, AlgorithmError};
use crate::core::page::FormatName;
use crate::core::pipeline::{Pipeline, PipelineName};

/// Fallimenti nel caricamento di un repo formati.
///
/// Un enum per il modulo radice, che raccoglie quelli dei sottomoduli: chi chiama
/// [`Algorithm::load`] non ha modo di reagire diversamente a un CSV malformato e a uno YAML
/// malformato, e distinguerli nel *tipo* invece che nel *messaggio* non lo aiuterebbe.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Il formato richiesto non è fra quelli che `metadata/formats.csv` dichiara.
    #[error("unknown format '{format}'; the repository declares {known} format(s)")]
    UnknownFormat { format: String, known: usize },
    /// Una pipeline non ha tutti e tre i segmenti: nessuno dei tre livelli glieli ha dati.
    #[error("pipeline '{pipeline}' is incomplete: missing the {missing} segment(s)")]
    IncompletePipeline { pipeline: String, missing: String },
    #[error(transparent)]
    Metadata(#[from] metadata::MetadataError),
    #[error(transparent)]
    Orchestration(#[from] orchestration::OrchestrationError),
    #[error(transparent)]
    Structured(#[from] structured::StructuredError),
    #[error(transparent)]
    Semistructured(#[from] semistructured::SemistructuredError),
    #[error(transparent)]
    Unstructured(#[from] unstructured::loader::UnstructuredError),
    #[error(transparent)]
    Algorithm(#[from] AlgorithmError),
}

/// Somma la pipeline `incoming` a quella eventualmente già presente sotto lo stesso nome.
fn merge_into(pipelines: &mut HashMap<PipelineName, Pipeline>, name: PipelineName, incoming: Pipeline) {
    match pipelines.remove(&name) {
        Some(existing) => pipelines.insert(name, existing + incoming),
        None => pipelines.insert(name, incoming),
    };
}

/// I nomi dei segmenti vuoti di una pipeline, per il messaggio d'errore.
fn missing_segments(pipeline: &Pipeline) -> String {
    let mut missing = Vec::new();
    if pipeline.pdf_extract.is_empty() {
        missing.push("pdf_extract");
    }
    if pipeline.text_filter.is_empty() {
        missing.push("text_filter");
    }
    if pipeline.deserialize.is_empty() {
        missing.push("deserialize");
    }
    missing.join(", ")
}

/// Tutte le pipeline di `format_name`, con i tre livelli già fusi.
///
/// `allow_partial` esiste per gli strumenti di sviluppo (`freeports-dev` prova un segmento alla
/// volta): in produzione una pipeline incompleta non può produrre nulla, e lasciarla passare
/// significherebbe scoprire il problema a documento aperto invece che a caricamento.
pub fn load_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
    allow_partial: bool,
) -> Result<HashMap<PipelineName, Pipeline>, LoadError> {
    let mut pipelines = structured::get_pipelines(formats_repo_dir, format_name)?;
    for (name, pipeline) in semistructured::get_pipelines(formats_repo_dir, format_name)? {
        merge_into(&mut pipelines, name, pipeline);
    }
    for (name, pipeline) in unstructured::loader::get_pipelines(formats_repo_dir, format_name)? {
        merge_into(&mut pipelines, name, pipeline);
    }

    if !allow_partial {
        // Ordinato: con una `HashMap` il primo incompleto trovato dipenderebbe dall'ordine di
        // hash, e il messaggio d'errore cambierebbe da un'esecuzione all'altra.
        let mut names: Vec<&PipelineName> = pipelines.keys().collect();
        names.sort();
        for name in names {
            let pipeline = &pipelines[name];
            if !pipeline.is_complete() {
                return Err(LoadError::IncompletePipeline {
                    pipeline: name.as_str().to_string(),
                    missing: missing_segments(pipeline),
                });
            }
        }
    }
    Ok(pipelines)
}

impl Algorithm {
    /// Carica l'algoritmo di `format_name` da un repo formati.
    ///
    /// **Perché questo `impl` sta qui e non in `core::algorithm`**: il costruttore ha bisogno di
    /// tutto `formats_repo`, mentre `formats_repo` è costruito sopra `core`. Metterlo là
    /// significherebbe una dipendenza circolare fra i due moduli — che Rust tollererebbe, ma che
    /// contraddice la disciplina di layering seguita ovunque altrove (stessa ragione per cui in M3
    /// si è spezzato il ciclo `position`/`tabularizer`). Un `impl` inerente può stare in qualunque
    /// modulo del crate che definisce il tipo, quindi la API pubblica resta quella che
    /// `PLAN.md` §5.5 chiede, `Algorithm::load`, senza invertire le dipendenze.
    pub fn load(formats_repo_dir: &Path, format_name: &FormatName) -> Result<Algorithm, LoadError> {
        let name = format_name.as_str();
        let known_formats = metadata::get_formats(formats_repo_dir)?;
        if !known_formats.iter().any(|f| f == name) {
            return Err(LoadError::UnknownFormat { format: name.to_string(), known: known_formats.len() });
        }

        let pipelines = load_pipelines(formats_repo_dir, name, false)?;
        let defined: HashSet<PipelineName> = pipelines.keys().cloned().collect();

        let classifiers = orchestration::get_pageclassify_pipelines(formats_repo_dir, name, &known_formats)?;
        let mapping = orchestration::get_mapping(formats_repo_dir, name, &known_formats, &defined)?;
        let schedule = orchestration::get_schedule(formats_repo_dir, name, &known_formats, &defined)?;
        let finalizer = unstructured::loader::get_page_class_finalizer(formats_repo_dir, name)?;

        // Ordinati ovunque: `Algorithm` conserva l'ordine che riceve, e da quell'ordine dipende
        // l'ordine in cui i pipe girano — che `PLAN.md` §12 D5 vuole deterministico.
        let mut classify_names: Vec<PipelineName> = classifiers.into_iter().collect();
        classify_names.sort();
        let sorted_pipelines: BTreeMap<PipelineName, Pipeline> = pipelines.into_iter().collect();
        let sorted_mapping: BTreeMap<crate::core::schedule::PageClass, Vec<PipelineName>> = mapping
            .into_iter()
            .map(|(class, names)| {
                let mut names: Vec<PipelineName> = names.into_iter().collect();
                names.sort();
                (class, names)
            })
            .collect();

        Ok(Algorithm::new(
            format_name.clone(),
            sorted_pipelines,
            &classify_names,
            finalizer,
            schedule,
            sorted_mapping,
        )?)
    }
}
