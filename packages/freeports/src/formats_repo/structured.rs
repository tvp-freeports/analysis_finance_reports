//! Il livello structured: pipeline costruite da tabelle CSV con implementazioni native.
//!
//! "Structured" significa che l'algoritmo di un segmento è **nella libreria** e il repo formati ne
//! fornisce solo i parametri: un formato structured non contiene codice, solo righe di CSV. È il
//! livello più vincolato dei tre e quello che copre la maggior parte dei formati.
//!
//! - [`tables`] legge e valida i cinque CSV e ne unisce le righe per pipe;
//! - [`investments`] e [`page_classify`] costruiscono i pipe nativi corrispondenti.

pub mod investments;
pub mod page_classify;
pub mod tables;

use crate::commons::flag_expr::FlagExprError;
use crate::formats_utils::pdf_extract::standard_funcs::PdfExtractStandardFuncsError;
use crate::formats_utils::text_filter::standard_funcs::StandardFuncsError;
use crate::input::document::selection::LineSelectionError;

use tables::TableError;

/// Fallimenti nella costruzione delle pipeline structured.
///
/// Un enum per il livello, non per file: i due sottomoduli costruttori condividono le stesse
/// cause di fallimento (una tabella illeggibile, una cella che non è una selezione, un parametro
/// che il pipe rifiuta) e separarli obbligherebbe a convertire avanti e indietro. Stesso
/// precedente di `output::classes` (M7) e `core::promise_resolution` (M2).
#[derive(Debug, thiserror::Error)]
pub enum StructuredError {
    #[error(transparent)]
    Table(#[from] TableError),
    /// Una cella superava la validazione di forma ma non si è lasciata analizzare: è un caso che
    /// non dovrebbe accadere (la validazione è la stessa grammatica), e per questo l'errore dice
    /// quale pipe e quale colonna.
    #[error("'{id}', column '{column}': {source}")]
    LineSelection {
        id: String,
        column: &'static str,
        #[source]
        source: LineSelectionError,
    },
    #[error("'{id}': invalid algorithm flags: {source}")]
    AlgorithmFlags {
        id: String,
        #[source]
        source: FlagExprError,
    },
    /// Il pipe `text_filter` ha rifiutato i parametri letti dal CSV (tipicamente due posizioni di
    /// colonna uguali fra loro).
    #[error("'{id}': {source}")]
    TextFilter {
        id: String,
        #[source]
        source: StandardFuncsError,
    },
    #[error("'{id}': {source}")]
    PdfExtract {
        id: String,
        #[source]
        source: PdfExtractStandardFuncsError,
    },
    /// `investments/args.csv` non dichiara la posizione della colonna del valore di mercato, che è
    /// l'unico parametro senza default del pipe `text_filter`.
    #[error("'{id}': column 'Market value' is required to build the text_filter segment")]
    MissingMarketValue { id: String },
}

/// Tutte le pipeline structured di `format_name`: quelle `investments` e quelle di
/// classificazione pagina, fuse per nome.
///
/// Porting di `structured/acquisition.py::get_pipelines`. La fusione qui è fra i due *sottolivelli*
/// di structured; quella fra structured, semistructured e unstructured avviene un livello sopra
/// (`PLAN.md` §6.4).
pub fn get_pipelines(
    formats_repo_dir: &std::path::Path,
    format_name: &str,
) -> Result<std::collections::HashMap<crate::core::pipeline::PipelineName, crate::core::pipeline::Pipeline>, StructuredError>
{
    let mut merged = investments::get_pipelines(formats_repo_dir, format_name)?;
    for (name, pipeline) in page_classify::get_pipelines(formats_repo_dir, format_name)? {
        match merged.remove(&name) {
            Some(existing) => merged.insert(name, existing + pipeline),
            None => merged.insert(name, pipeline),
        };
    }
    Ok(merged)
}
