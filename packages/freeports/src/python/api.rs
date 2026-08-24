//! Shim delle facciate d'ingresso: `Algorithm`, `run_job`, `get_formats`, la config su file.
//!
//! È la parte di API che serve a **`freeports_dev`**, non agli autori di formato: caricare
//! l'algoritmo di un formato, applicarne un segmento a una pagina sola (i test a pagina singola),
//! e far girare un job intero scrivendo i CSV (i test d'integrazione).

use std::path::{Path, PathBuf};

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use crate::cli::conf_parse::DocumentSpec;
use crate::cli::config_locations::file;
use crate::cli::partial_config::{ConfigSource, PartialConfig, defaults, overwrite};
use crate::cli::{freeports_config, job, output};
use crate::core::algorithm::Algorithm;
use crate::core::tracing_setup::CsvLogLayer;
use crate::core::page::FormatName;
use crate::core::schedule::PageClass;
use crate::formats_repo::metadata;
use crate::output::routines::write::{OutFlags, OutStructureMode};

use super::core::{PyPdfBlock, PyTextBlock};
use super::pipes::{extracted_to_py, filter_data_of, page_from_py, previous_results_from_py, target_companies_from_py};

/// Un errore nativo come `ValueError` Python.
fn value_error<E: std::fmt::Display>(error: E) -> PyErr {
    PyValueError::new_err(error.to_string())
}

/// Shim Python di [`Algorithm`].
///
/// I tre `apply_*` sono l'API a segmenti che `freeports-dev` usa per i test a pagina singola.
/// Rispetto al riferimento cambia **come** sono concatenati, non cosa restituiscono: là
/// `apply_text_filter` e `apply_deserialize` ripartivano ogni volta dalla pagina, rieseguendo i
/// segmenti a monte; qui il nativo `apply_deserializer` parte dai blocchi di testo (`PLAN.md`
/// §5.5), quindi è lo shim a rifare la catena `pdf_extract -> text_filter` prima di
/// deserializzare. Il risultato visto da Python è lo stesso.
#[pyclass(name = "Algorithm", module = "freeports.core", frozen)]
pub struct PyAlgorithm(Algorithm);

#[pymethods]
impl PyAlgorithm {
    /// `Algorithm.load(formats_repo_dir, format_name, format_names=None)`.
    ///
    /// **Divergenza assorbita qui:** il terzo argomento è accettato e ignorato. Nel riferimento
    /// era l'elenco dei formati noti, che il chiamante doveva procurarsi da `get_formats` e
    /// passare; il nativo [`Algorithm::load`] lo rilegge da sé dal repo, quindi passarlo non
    /// aggiunge nulla — ma i chiamanti esistenti lo passano ancora.
    #[staticmethod]
    #[pyo3(signature = (formats_repo_dir, format_name, format_names=None))]
    fn load(
        formats_repo_dir: PathBuf,
        format_name: String,
        format_names: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyAlgorithm> {
        let _ = format_names;
        Algorithm::load(&formats_repo_dir, &FormatName::new(format_name))
            .map(PyAlgorithm)
            .map_err(value_error)
    }

    #[getter]
    fn format(&self) -> &str {
        self.0.format().as_str()
    }

    /// Le page class che lo schedule attraversa, nell'ordine dello schedule.
    #[getter]
    fn page_classes(&self) -> Vec<String> {
        self.0.schedule().page_classes().iter().map(|class| class.as_str().to_string()).collect()
    }

    fn apply_pdf_extract(&self, page: &Bound<'_, PyAny>, page_class: &str) -> PyResult<Vec<PyPdfBlock>> {
        let py = page.py();
        let page = page_from_py(page)?;
        let blocks = self.0.apply_pdf_extract(&page, &PageClass::new(page_class)).map_err(value_error)?;
        blocks.iter().map(|block| PyPdfBlock::from_native(py, block)).collect()
    }

    #[pyo3(signature = (page, filter_data, page_class))]
    fn apply_text_filter(
        &self,
        page: &Bound<'_, PyAny>,
        filter_data: &Bound<'_, PyAny>,
        page_class: &str,
    ) -> PyResult<Vec<PyTextBlock>> {
        let py = page.py();
        let page = page_from_py(page)?;
        let companies = target_companies_from_py(filter_data)?;
        let previous = previous_results_from_py(filter_data)?;
        let data = filter_data_of(&companies, &previous);
        let blocks = self
            .0
            .apply_text_filter(&page, &PageClass::new(page_class), &data)
            .map_err(value_error)?;
        blocks.iter().map(|block| PyTextBlock::from_native(py, block)).collect()
    }

    #[pyo3(signature = (page, filter_data, page_class))]
    fn apply_deserialize<'py>(
        &self,
        py: Python<'py>,
        page: &Bound<'py, PyAny>,
        filter_data: &Bound<'py, PyAny>,
        page_class: &str,
    ) -> PyResult<Vec<Bound<'py, PyAny>>> {
        let page = page_from_py(page)?;
        let companies = target_companies_from_py(filter_data)?;
        let previous = previous_results_from_py(filter_data)?;
        let data = filter_data_of(&companies, &previous);
        // `Algorithm::apply_deserialize` e non `apply_text_filter` + `apply_deserializer`: le due
        // cose differiscono quando una page class mappa piu' pipeline, vedi il doc-comment di
        // quel metodo.
        let extracted =
            self.0.apply_deserialize(&page, &PageClass::new(page_class), &data).map_err(value_error)?;
        extracted.iter().map(|item| extracted_to_py(py, item)).collect()
    }

    fn __repr__(&self) -> String {
        format!("Algorithm({:?})", self.0.format().as_str())
    }
}

/// I nomi dei formati dichiarati da `formats.csv`.
///
/// **Divergenza:** il riferimento restituiva un `pd.DataFrame` indicizzato per nome, e i
/// chiamanti ne prendevano sempre e solo `.index`. Qui restituisce direttamente la lista dei nomi:
/// il resto della tabella non era mai stato letto da nessuno, e il crate non dipende da pandas.
#[pyfunction]
#[pyo3(name = "get_formats", signature = (formats_repo_dir))]
pub fn py_get_formats(formats_repo_dir: PathBuf) -> PyResult<Vec<String>> {
    metadata::get_formats(&formats_repo_dir).map_err(value_error)
}

/// Il formato che corrisponde a un url di report, se il repo ne dichiara uno.
#[pyfunction]
#[pyo3(name = "url_to_format", signature = (formats_repo_dir, url))]
pub fn py_url_to_format(formats_repo_dir: PathBuf, url: &str) -> PyResult<Option<String>> {
    let format_names = metadata::get_formats(&formats_repo_dir).map_err(value_error)?;
    metadata::url_to_format(&formats_repo_dir, &format_names, url).map_err(value_error)
}

/// Il profilo di scrittura da stringa, con gli stessi nomi accettati dalla riga di comando.
fn out_profile_of(value: &str) -> PyResult<OutStructureMode> {
    match value.to_ascii_lowercase().as_str() {
        "regular" => Ok(OutStructureMode::Regular),
        "single_file" => Ok(OutStructureMode::SingleFile),
        "structured" => Ok(OutStructureMode::Structured),
        other => Err(PyValueError::new_err(format!(
            "invalid output profile {other:?}, expected one of: regular, single_file, structured"
        ))),
    }
}

/// I flag di scrittura da stringa: nomi separati da virgola, come li scrive un file di config.
fn out_flags_of(value: &str) -> PyResult<OutFlags> {
    let mut flags = OutFlags::default();
    for name in value.split(',').map(str::trim).filter(|name| !name.is_empty()) {
        match name.to_ascii_lowercase().as_str() {
            "compressed" | "archive" => flags.compressed = true,
            "separate_out" => flags.separate_out = true,
            other => {
                return Err(PyValueError::new_err(format!(
                    "invalid output flag {other:?}, expected one of: compressed, separate_out"
                )));
            }
        }
    }
    Ok(flags)
}

/// Una terzina `(url, path, name)` come [`DocumentSpec`].
fn document_spec_of(spec: (Option<String>, Option<PathBuf>, Option<String>)) -> PyResult<DocumentSpec> {
    let (url, path, name) = spec;
    if url.is_none() && path.is_none() {
        return Err(PyValueError::new_err(
            "you have to specify at least one of: the url, the pdf file path, or both",
        ));
    }
    Ok(DocumentSpec { url, path, name })
}

/// Fa girare **un** job e ne scrive i risultati, come una singola invocazione da riga di comando.
///
/// È il punto d'ingresso dei test d'integrazione di `freeports-dev`: prende argomenti primitivi
/// invece di una `CliArgs`, salta la catena di merge delle sorgenti di configurazione (file, env,
/// riga di comando: qui non ce n'è nessuna) e chiama la stessa `job::run` + `output::write_results`
/// che usa `cli::run::execute`.
#[pyfunction]
#[pyo3(name = "run_job", signature = (
    input_reports, format, target_lists, formats_repo_path, input_db_path, out_path,
    out_profile=None, out_flags=None, save_pdf=None,
))]
#[allow(clippy::too_many_arguments)]
pub fn py_run_job(
    input_reports: Vec<(Option<String>, Option<PathBuf>, Option<String>)>,
    format: String,
    target_lists: Vec<String>,
    formats_repo_path: PathBuf,
    input_db_path: PathBuf,
    out_path: PathBuf,
    out_profile: Option<String>,
    out_flags: Option<String>,
    save_pdf: Option<bool>,
) -> PyResult<()> {
    let reports = input_reports.into_iter().map(document_spec_of).collect::<PyResult<Vec<_>>>()?;
    let out_profile = out_profile.as_deref().map(out_profile_of).transpose()?;
    let out_flags = out_flags.as_deref().map(out_flags_of).transpose()?;

    let overlay = PartialConfig {
        reports: Some(reports),
        format: Some(format),
        target_lists: Some(target_lists),
        formats_repo_path: Some(formats_repo_path),
        input_db_path: Some(input_db_path),
        out_path: Some(out_path),
        out_profile,
        out_flags,
        save_pdf,
        ..Default::default()
    };

    let config = freeports_config::validate(overwrite(defaults(), overlay, ConfigSource::Cmd))
        .map_err(value_error)?;

    // Il `.log.csv` accanto agli altri CSV, come fa il riferimento — la cartella di output, non la
    // cwd in cui il binario lo scrive. È un file che i test d'integrazione confrontano, quindi
    // deve esistere anche quando non ha nessuna riga da scrivere: `CsvLogLayer::create` ne emette
    // subito l'intestazione.
    //
    // Il layer è installato **per chiamata** con `with_default` e non con `set_global_default`:
    // un processo pytest chiama `run_job` una volta per formato, e un subscriber globale si può
    // installare una sola volta per processo. `with_default` usa lo scope thread-local, che ha
    // comunque la precedenza su un eventuale globale.
    if config.out_profile != OutStructureMode::SingleFile {
        std::fs::create_dir_all(&config.out_path).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    }
    let log_dir = if config.out_profile == OutStructureMode::SingleFile {
        config.out_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        config.out_path.clone()
    };
    let csv_layer = CsvLogLayer::create(&log_dir.join(".log.csv")).map_err(value_error)?;
    let subscriber = {
        use tracing_subscriber::layer::SubscriberExt;
        tracing_subscriber::registry().with(csv_layer)
    };

    tracing::subscriber::with_default(subscriber, || {
        let outcomes = job::run(&config).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        output::write_results(&config, &outcomes).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    })
}

/// Shim Python del file di configurazione `freeports.yaml`.
///
/// Espone solo ciò che i chiamanti ne leggono davvero: dove trovarlo e quale input database
/// dichiara. Non è la `FreeportsConfig` completa, che è il risultato del merge di tutte le
/// sorgenti e non ha senso costruire da Python.
#[pyclass(name = "FreeportsFileConfig", module = "freeports.cli", frozen)]
pub struct PyFreeportsFileConfig(PartialConfig);

#[pymethods]
impl PyFreeportsFileConfig {
    /// Il file di configurazione da leggere: locale, poi utente, poi di sistema.
    #[staticmethod]
    fn find_config() -> Option<PathBuf> {
        file::find_config()
    }

    #[new]
    fn new(path: PathBuf) -> PyResult<Self> {
        file::load(Some(Path::new(&path))).map(PyFreeportsFileConfig).map_err(value_error)
    }

    /// Il nome è in maiuscolo come nel riferimento, dove le chiavi di configurazione erano
    /// attributi maiuscoli dell'oggetto.
    #[getter]
    #[pyo3(name = "INPUT_DB_PATH")]
    fn input_db_path(&self) -> Option<PathBuf> {
        self.0.input_db_path.clone()
    }

    #[getter]
    #[pyo3(name = "FORMATS_REPO_PATH")]
    fn formats_repo_path(&self) -> Option<PathBuf> {
        self.0.formats_repo_path.clone()
    }

    #[getter]
    #[pyo3(name = "OUT_PATH")]
    fn out_path(&self) -> Option<PathBuf> {
        self.0.out_path.clone()
    }
}
