//! I CSV di orchestrazione: in che ordine si elaborano le pagine e chi si occupa di quale.
//!
//! Tre file in `content/orchestration/`:
//!
//! - `algorithms_schedule.csv` — l'ordine di elaborazione delle page class di un formato, diviso
//!   in *step*: le page class si accumulano nello step corrente finché una riga non alza
//!   `Filter next iteration`, che chiude lo step e ne apre uno nuovo. È lo `Schedule` del motore
//!   (`core::schedule`, M5), e la ragione per cui esiste è che i risultati di uno step sono il
//!   `FilterData` del successivo.
//! - `mapping.csv` — quali pipeline si occupano di quale page class.
//! - `pageclassify_overwrite.csv` — quali pipeline, per un dato formato, sostituiscono la
//!   classificazione di pagina standard.
//!
//! Porting di `repo/orchestration.py` (e del suo porting Rust in `freeports_core`), senza
//! pandas/pandera. **Una differenza strutturale**: nel riferimento il ramo di fallback di
//! [`get_mapping`] richiama `pipelines_acquisition.get_pipelines`, cioè risale all'intero
//! caricamento del repo formati; qui i nomi delle pipeline già costruite arrivano come
//! *parametro*. Il motivo è che la dipendenza inversa (orchestrazione → caricamento pipeline →
//! orchestrazione) è un ciclo, che nel riferimento è tollerabile solo perché passa da Python; qui
//! il chiamante — `Algorithm::load`, che le pipeline le ha già in mano — le passa e basta.
//!
//! **Log**: ogni funzione pubblica gira dentro lo span `format[<nome>]` che `Algorithm::load`
//! apre (`PLAN.md` §3 L2), quindi i suoi eventi non ripetono `format_name` come campo — lo
//! ereditano dallo span, come vuole la regola 4 della convenzione.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::pipeline::PipelineName;
use crate::core::schedule::{PageClass, Schedule, ScheduleStep};

use super::id_format::{IdFormat, derive_format_name, derive_pipeline_name, id_matches};

pub const CONTENT_DIR: &str = "content";
pub const ORCHESTRATION_DIR: &str = "orchestration";

/// Fallimenti nella lettura dei CSV di orchestrazione.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrchestrationError {
    #[error("missing formats-repository CSV file: {0}")]
    MissingCsv(PathBuf),
    #[error("{path}: malformed row at line {line}: {reason}")]
    MalformedRow { path: PathBuf, line: usize, reason: String },
    #[error("{path}: missing required column '{column}'")]
    MissingColumn { path: PathBuf, column: String },
    #[error("{path}, line {line}: unknown format name: {name}")]
    UnknownFormatName { path: PathBuf, line: usize, name: String },
    #[error("{path}, line {line}: ID '{id}' does not match the expected ID pattern")]
    InvalidId { path: PathBuf, line: usize, id: String },
    /// `pageclassify_overwrite.csv` **pretende** un gruppo `(pipeline)` nell'ID, a differenza di
    /// `mapping.csv`, che invece lo lascia vuoto quando manca. L'asimmetria è del riferimento
    /// (`nullable=False` contro `fillna("")`) ed è conservata.
    #[error("{path}, line {line}: ID '{id}' declares no pipeline name")]
    MissingPipelineName { path: PathBuf, line: usize, id: String },
    #[error("{path}, line {line}: invalid 'Filter next iteration' value '{value}'")]
    InvalidFlag { path: PathBuf, line: usize, value: String },
}

/// Una riga di `algorithms_schedule.csv`.
///
/// **Non** una struct `Deserialize` come le altre due: `Filter next iteration` è una colonna in
/// coda genuinamente opzionale — una riga che la omette *del tutto* è legale, e il vero
/// `algorithms_schedule.csv` del repo italiano ne contiene — mentre serde pretende un campo per
/// ogni colonna dichiarata e considera la riga corta un errore. Questo file si legge quindi per
/// indice di colonna, come fa il riferimento.
struct ScheduleRow {
    format_name: String,
    page_type: String,
    filter_next_iteration: String,
}

/// Una riga di `mapping.csv`.
#[derive(Debug, Clone, Deserialize)]
struct MappingRow {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Page type")]
    page_type: String,
}

/// Una riga di `pageclassify_overwrite.csv`.
#[derive(Debug, Clone, Deserialize)]
struct OverwriteRow {
    #[serde(rename = "ID")]
    id: String,
}

fn csv_path(formats_repo_dir: &Path, file_name: &str) -> PathBuf {
    formats_repo_dir.join(CONTENT_DIR).join(ORCHESTRATION_DIR).join(file_name)
}

/// Traduce un errore del crate `csv`, ripescando il nome della colonna assente quando c'è.
fn row_error(path: &Path, line: usize, error: &csv::Error) -> OrchestrationError {
    let message = error.to_string();
    if let Some(rest) = message.split("missing field `").nth(1)
        && let Some(column) = rest.split('`').next()
    {
        return OrchestrationError::MissingColumn { path: path.to_path_buf(), column: column.to_string() };
    }
    OrchestrationError::MalformedRow { path: path.to_path_buf(), line, reason: message }
}

/// Legge un CSV di orchestrazione in righe tipizzate.
///
/// `flexible` va acceso **solo** per i file la cui ultima colonna è davvero facoltativa da capo a
/// fondo: altrimenti una riga corta smetterebbe di essere un errore anche dove manca un campo
/// obbligatorio.
fn read_rows<T: serde::de::DeserializeOwned>(
    formats_repo_dir: &Path,
    file_name: &str,
    flexible: bool,
) -> Result<(PathBuf, Vec<T>), OrchestrationError> {
    let path = csv_path(formats_repo_dir, file_name);
    if !path.is_file() {
        return Err(OrchestrationError::MissingCsv(path));
    }
    let mut reader = csv::ReaderBuilder::new()
        .flexible(flexible)
        .from_path(&path)
        .map_err(|e| OrchestrationError::MalformedRow { path: path.clone(), line: 0, reason: e.to_string() })?;
    let mut rows = Vec::new();
    for (i, record) in reader.deserialize::<T>().enumerate() {
        rows.push(record.map_err(|e| row_error(&path, i + 1, &e))?);
    }
    Ok((path, rows))
}

/// Legge `algorithms_schedule.csv` per indice di colonna, tollerando le righe corte sulla sola
/// colonna in coda. Vedi [`ScheduleRow`] per il perché.
fn read_schedule_rows(formats_repo_dir: &Path) -> Result<(PathBuf, Vec<ScheduleRow>), OrchestrationError> {
    let path = csv_path(formats_repo_dir, "algorithms_schedule.csv");
    if !path.is_file() {
        return Err(OrchestrationError::MissingCsv(path));
    }
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(&path)
        .map_err(|e| OrchestrationError::MalformedRow { path: path.clone(), line: 0, reason: e.to_string() })?;
    let headers = reader
        .headers()
        .map_err(|e| OrchestrationError::MalformedRow { path: path.clone(), line: 0, reason: e.to_string() })?
        .clone();
    let column = |name: &str| -> Result<usize, OrchestrationError> {
        headers
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| OrchestrationError::MissingColumn { path: path.clone(), column: name.to_string() })
    };
    let format_name_idx = column("Format name")?;
    let page_type_idx = column("Page type")?;
    let filter_idx = column("Filter next iteration")?;

    let mut rows = Vec::new();
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(|e| row_error(&path, i + 1, &e))?;
        rows.push(ScheduleRow {
            format_name: record.get(format_name_idx).unwrap_or("").to_string(),
            page_type: record.get(page_type_idx).unwrap_or("").to_string(),
            filter_next_iteration: record.get(filter_idx).unwrap_or("").to_string(),
        });
    }
    Ok((path, rows))
}

fn check_known_format_name(
    path: &Path,
    line: usize,
    format_name: &str,
    format_names: &[String],
) -> Result<(), OrchestrationError> {
    if format_names.iter().any(|n| n == format_name) {
        Ok(())
    } else {
        Err(OrchestrationError::UnknownFormatName {
            path: path.to_path_buf(),
            line,
            name: format_name.to_string(),
        })
    }
}

/// `TRUE`/`FALSE` come li scrive un foglio di calcolo, con la cella vuota che vale `FALSE`.
fn parse_flag(path: &Path, line: usize, raw: &str) -> Result<bool, OrchestrationError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "false" => Ok(false),
        "true" => Ok(true),
        other => {
            Err(OrchestrationError::InvalidFlag { path: path.to_path_buf(), line, value: other.to_string() })
        }
    }
}

/// L'ordine in cui elaborare le page class di `format_name`.
///
/// Le page class si accumulano nello step corrente; `Filter next iteration` su una riga chiude lo
/// step **dopo** averla inclusa. Se il formato non compare affatto nel file, lo schedule è un solo
/// step con tutte le page class che `mapping.csv` gli attribuisce — cioè "elabora tutto insieme,
/// senza passaggi successivi".
pub fn get_schedule(
    formats_repo_dir: &Path,
    format_name: &str,
    format_names: &[String],
    defined_pipelines: &HashSet<PipelineName>,
) -> Result<Schedule, OrchestrationError> {
    let (path, rows) = read_schedule_rows(formats_repo_dir)?;
    for (i, row) in rows.iter().enumerate() {
        check_known_format_name(&path, i + 1, &row.format_name, format_names)?;
    }

    let matching: Vec<(usize, &ScheduleRow)> =
        rows.iter().enumerate().filter(|(_, row)| row.format_name == format_name).collect();

    if matching.is_empty() {
        tracing::debug!("format absent from algorithms_schedule.csv, scheduling every mapped page class in one step");
        let mapping = get_mapping(formats_repo_dir, format_name, format_names, defined_pipelines)?;
        let mut step = ScheduleStep::new();
        // Ordinato: `mapping` è una `HashMap`, e uno schedule che cambia ordine a ogni esecuzione
        // renderebbe irriproducibili sia i test sia i log.
        let mut classes: Vec<PageClass> = mapping.into_keys().collect();
        classes.sort();
        for class in classes {
            step.push(class);
        }
        let schedule = Schedule::new(vec![step]);
        tracing::debug!(step_count = schedule.steps().len(), "built schedule");
        return Ok(schedule);
    }

    let mut steps = vec![ScheduleStep::new()];
    for (i, row) in matching {
        steps.last_mut().expect("lo schedule ha sempre almeno uno step").push(PageClass::new(&row.page_type));
        if parse_flag(&path, i + 1, &row.filter_next_iteration)? {
            steps.push(ScheduleStep::new());
        }
    }
    let schedule = Schedule::new(steps);
    tracing::debug!(step_count = schedule.steps().len(), "built schedule");
    Ok(schedule)
}

/// Le pipeline che, per `format_name`, sostituiscono la classificazione di pagina standard.
///
/// Se il formato non compare nel file, il risultato è l'insieme che contiene la **sola pipeline
/// senza nome** — non l'insieme vuoto. È il `set([""])` del riferimento, ed è ciò che rende la
/// pipeline di default il classificatore di pagina di ogni formato che non dica altro.
pub fn get_pageclassify_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
    format_names: &[String],
) -> Result<HashSet<PipelineName>, OrchestrationError> {
    let (path, rows): (_, Vec<OverwriteRow>) = read_rows(formats_repo_dir, "pageclassify_overwrite.csv", false)?;

    let mut pipelines = HashSet::new();
    let mut found = false;
    for (i, row) in rows.iter().enumerate() {
        let line = i + 1;
        if !id_matches(&row.id, IdFormat::ExpandableNoIndex) {
            return Err(OrchestrationError::InvalidId { path, line, id: row.id.clone() });
        }
        let row_format = derive_format_name(&row.id);
        let pipeline = derive_pipeline_name(&row.id, None).ok_or_else(|| {
            OrchestrationError::MissingPipelineName { path: path.clone(), line, id: row.id.clone() }
        })?;
        check_known_format_name(&path, line, &row_format, format_names)?;
        if row_format == format_name {
            found = true;
            pipelines.insert(PipelineName::new(pipeline));
        }
    }

    if !found {
        tracing::debug!(
            "format absent from pageclassify_overwrite.csv, falling back to the unnamed pipeline as page classifier"
        );
        return Ok(HashSet::from([PipelineName::new("")]));
    }
    tracing::debug!(pipeline_count = pipelines.len(), "read page classifying pipelines");
    Ok(pipelines)
}

/// Quali pipeline si occupano di quale page class, per `format_name`.
///
/// Se il formato non compare in `mapping.csv`, ogni pipeline che il formato definisce diventa la
/// page class omonima — tranne quelle che classificano le pagine, che non hanno una page class
/// propria. È la convenzione implicita del riferimento per i formati che non dichiarano un
/// mapping: "una pipeline per page class, con lo stesso nome".
pub fn get_mapping(
    formats_repo_dir: &Path,
    format_name: &str,
    format_names: &[String],
    defined_pipelines: &HashSet<PipelineName>,
) -> Result<HashMap<PageClass, HashSet<PipelineName>>, OrchestrationError> {
    let (path, rows): (_, Vec<MappingRow>) = read_rows(formats_repo_dir, "mapping.csv", false)?;

    let mut mapping: HashMap<PageClass, HashSet<PipelineName>> = HashMap::new();
    let mut found = false;
    for (i, row) in rows.iter().enumerate() {
        let line = i + 1;
        if !id_matches(&row.id, IdFormat::ExpandableNoIndex) {
            return Err(OrchestrationError::InvalidId { path, line, id: row.id.clone() });
        }
        let row_format = derive_format_name(&row.id);
        // A differenza di `pageclassify_overwrite.csv`, qui un ID senza gruppo `(pipeline)` vale
        // la pipeline senza nome, non un errore.
        let pipeline = derive_pipeline_name(&row.id, Some("")).unwrap_or_default();
        check_known_format_name(&path, line, &row_format, format_names)?;
        if row_format == format_name {
            found = true;
            mapping.entry(PageClass::new(&row.page_type)).or_default().insert(PipelineName::new(pipeline));
        }
    }

    if !found {
        // Qui il riferimento richiama `pipelines_acquisition.get_pipelines` (risalendo all'intero
        // caricamento del repo formati); questo porting evita il ciclo prendendo le pipeline già
        // costruite come parametro (`defined_pipelines`, vedi il doc-comment del modulo) — non c'è
        // quindi un confine Python da loggare qui.
        tracing::debug!(
            "format absent from mapping.csv, mapping each of its own pipelines onto a page class of the same name"
        );
        let classifiers = get_pageclassify_pipelines(formats_repo_dir, format_name, format_names)?;
        let mapping: HashMap<PageClass, HashSet<PipelineName>> = defined_pipelines
            .iter()
            .filter(|name| !classifiers.contains(name))
            .map(|name| (PageClass::new(name.as_str()), HashSet::from([name.clone()])))
            .collect();
        tracing::debug!(class_count = mapping.len(), "built page class mapping");
        return Ok(mapping);
    }
    tracing::debug!(class_count = mapping.len(), "built page class mapping");
    Ok(mapping)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Un repo formati minimale con i soli tre CSV di orchestrazione.
    fn repo(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("temp dir");
        let orchestration = dir.path().join(CONTENT_DIR).join(ORCHESTRATION_DIR);
        fs::create_dir_all(&orchestration).expect("orchestration dir");
        for (name, content) in files {
            fs::write(orchestration.join(name), content).expect("write csv");
        }
        dir
    }

    fn names() -> Vec<String> {
        vec!["A-EN24".to_string(), "B-EN24".to_string()]
    }

    fn pipelines(names: &[&str]) -> HashSet<PipelineName> {
        names.iter().map(|n| PipelineName::new(*n)).collect()
    }

    fn classes_of(schedule: &Schedule) -> Vec<Vec<String>> {
        schedule
            .steps()
            .iter()
            .map(|step| step.iter().map(|c| c.as_str().to_string()).collect())
            .collect()
    }

    const EMPTY_OVERWRITE: &str = "ID\n";
    const MAPPING_CSV: &str = "ID,Page type\n\
                               A-EN24(investments),investments\n\
                               A-EN24(merging),merges\n";

    mod schedule {
        use super::*;
        use pretty_assertions::assert_eq;

        const SCHEDULE_CSV: &str = "Format name,Page type,Filter next iteration\n\
                                    A-EN24,investments,TRUE\n\
                                    A-EN24,merges,\n\
                                    A-EN24,sfdr_classification,\n\
                                    B-EN24,investments,\n";

        fn dir() -> TempDir {
            repo(&[
                ("algorithms_schedule.csv", SCHEDULE_CSV),
                ("mapping.csv", MAPPING_CSV),
                ("pageclassify_overwrite.csv", EMPTY_OVERWRITE),
            ])
        }

        #[test]
        fn a_filter_flag_closes_the_step_after_including_its_own_page_class() {
            let d = dir();
            let schedule = get_schedule(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap();
            assert_eq!(
                classes_of(&schedule),
                vec![vec!["investments".to_string()], vec!["merges".to_string(), "sfdr_classification".to_string()]]
            );
        }

        #[test]
        fn only_the_rows_of_the_requested_format_are_considered() {
            let d = dir();
            let schedule = get_schedule(d.path(), "B-EN24", &names(), &pipelines(&[])).unwrap();
            assert_eq!(classes_of(&schedule), vec![vec!["investments".to_string()]]);
        }

        #[test]
        fn a_missing_filter_cell_counts_as_false() {
            let csv = "Format name,Page type,Filter next iteration\nA-EN24,investments\nA-EN24,merges\n";
            let d = repo(&[
                ("algorithms_schedule.csv", csv),
                ("mapping.csv", MAPPING_CSV),
                ("pageclassify_overwrite.csv", EMPTY_OVERWRITE),
            ]);
            let schedule = get_schedule(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap();
            assert_eq!(schedule.steps().len(), 1);
        }

        #[test]
        fn a_trailing_filter_flag_leaves_an_empty_last_step() {
            // Comportamento del riferimento conservato: la bandiera sull'ultima riga apre uno
            // step che nessuna page class riempirà mai.
            let csv = "Format name,Page type,Filter next iteration\nA-EN24,investments,TRUE\n";
            let d = repo(&[
                ("algorithms_schedule.csv", csv),
                ("mapping.csv", MAPPING_CSV),
                ("pageclassify_overwrite.csv", EMPTY_OVERWRITE),
            ]);
            let schedule = get_schedule(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap();
            assert_eq!(classes_of(&schedule), vec![vec!["investments".to_string()], Vec::<String>::new()]);
        }

        #[test]
        fn a_format_absent_from_the_file_gets_every_mapped_page_class_in_one_step() {
            let csv = "Format name,Page type,Filter next iteration\nB-EN24,investments,\n";
            let d = repo(&[
                ("algorithms_schedule.csv", csv),
                ("mapping.csv", MAPPING_CSV),
                ("pageclassify_overwrite.csv", EMPTY_OVERWRITE),
            ]);
            let schedule = get_schedule(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap();
            assert_eq!(classes_of(&schedule), vec![vec!["investments".to_string(), "merges".to_string()]]);
        }

        #[test]
        fn the_fallback_step_is_sorted_so_the_schedule_is_reproducible() {
            let csv = "Format name,Page type,Filter next iteration\nB-EN24,investments,\n";
            let mapping = "ID,Page type\nA-EN24(z),zeta\nA-EN24(a),alpha\nA-EN24(m),mu\n";
            let d = repo(&[
                ("algorithms_schedule.csv", csv),
                ("mapping.csv", mapping),
                ("pageclassify_overwrite.csv", EMPTY_OVERWRITE),
            ]);
            let schedule = get_schedule(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap();
            assert_eq!(
                classes_of(&schedule),
                vec![vec!["alpha".to_string(), "mu".to_string(), "zeta".to_string()]]
            );
        }

        #[test]
        fn an_unknown_format_name_anywhere_in_the_file_is_an_error() {
            let csv = "Format name,Page type,Filter next iteration\nGHOST-EN24,investments,\n";
            let d = repo(&[
                ("algorithms_schedule.csv", csv),
                ("mapping.csv", MAPPING_CSV),
                ("pageclassify_overwrite.csv", EMPTY_OVERWRITE),
            ]);
            let err = get_schedule(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap_err();
            assert!(matches!(err, OrchestrationError::UnknownFormatName { line: 1, .. }));
        }

        #[test]
        fn an_unparsable_filter_flag_names_the_offending_value_and_line() {
            let csv = "Format name,Page type,Filter next iteration\nA-EN24,investments,maybe\n";
            let d = repo(&[
                ("algorithms_schedule.csv", csv),
                ("mapping.csv", MAPPING_CSV),
                ("pageclassify_overwrite.csv", EMPTY_OVERWRITE),
            ]);
            let err = get_schedule(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap_err();
            let OrchestrationError::InvalidFlag { line, value, .. } = err else { panic!("expected InvalidFlag") };
            assert_eq!((line, value.as_str()), (1, "maybe"));
        }

        #[test]
        fn a_missing_column_names_the_column() {
            let csv = "Format name,Filter next iteration\nA-EN24,TRUE\n";
            let d = repo(&[
                ("algorithms_schedule.csv", csv),
                ("mapping.csv", MAPPING_CSV),
                ("pageclassify_overwrite.csv", EMPTY_OVERWRITE),
            ]);
            let err = get_schedule(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap_err();
            assert!(matches!(err, OrchestrationError::MissingColumn { column, .. } if column == "Page type"));
        }

        #[test]
        fn a_missing_file_is_reported_with_its_path() {
            let d = repo(&[]);
            let err = get_schedule(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap_err();
            assert!(matches!(err, OrchestrationError::MissingCsv(_)));
        }
    }

    mod pageclassify_pipelines {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn reads_the_pipelines_declared_for_the_format() {
            let csv = "ID\nA-EN24(cover)\nA-EN24(header)\nB-EN24(other)\n";
            let d = repo(&[("pageclassify_overwrite.csv", csv)]);
            let found = get_pageclassify_pipelines(d.path(), "A-EN24", &names()).unwrap();
            assert_eq!(found, pipelines(&["cover", "header"]));
        }

        #[test]
        fn a_format_absent_from_the_file_falls_back_to_the_unnamed_pipeline() {
            let d = repo(&[("pageclassify_overwrite.csv", EMPTY_OVERWRITE)]);
            let found = get_pageclassify_pipelines(d.path(), "A-EN24", &names()).unwrap();
            assert_eq!(found, pipelines(&[""]));
        }

        #[test]
        fn the_fallback_is_a_singleton_not_an_empty_set() {
            let d = repo(&[("pageclassify_overwrite.csv", EMPTY_OVERWRITE)]);
            assert_eq!(get_pageclassify_pipelines(d.path(), "A-EN24", &names()).unwrap().len(), 1);
        }

        #[test]
        fn an_id_without_a_pipeline_group_is_rejected_here() {
            // Asimmetria voluta rispetto a `mapping.csv`, che invece lo accetta come pipeline
            // senza nome.
            let d = repo(&[("pageclassify_overwrite.csv", "ID\nA-EN24\n")]);
            let err = get_pageclassify_pipelines(d.path(), "A-EN24", &names()).unwrap_err();
            assert!(matches!(err, OrchestrationError::MissingPipelineName { line: 1, .. }));
        }

        #[test]
        fn an_id_carrying_an_index_is_rejected() {
            let d = repo(&[("pageclassify_overwrite.csv", "ID\nA-EN24(cover)/0\n")]);
            let err = get_pageclassify_pipelines(d.path(), "A-EN24", &names()).unwrap_err();
            assert!(matches!(err, OrchestrationError::InvalidId { line: 1, .. }));
        }

        #[test]
        fn an_unknown_format_name_is_an_error_even_on_a_row_of_another_format() {
            let d = repo(&[("pageclassify_overwrite.csv", "ID\nA-EN24(cover)\nGHOST-EN24(x)\n")]);
            let err = get_pageclassify_pipelines(d.path(), "A-EN24", &names()).unwrap_err();
            assert!(matches!(err, OrchestrationError::UnknownFormatName { line: 2, .. }));
        }
    }

    mod mapping {
        use super::*;
        use pretty_assertions::assert_eq;

        fn dir(mapping: &str) -> TempDir {
            repo(&[("mapping.csv", mapping), ("pageclassify_overwrite.csv", EMPTY_OVERWRITE)])
        }

        #[test]
        fn groups_the_pipelines_by_page_class() {
            let d = dir("ID,Page type\nA-EN24(renaming),renaming\nA-EN24(merging),renaming\n");
            let found = get_mapping(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap();
            assert_eq!(found.len(), 1);
            assert_eq!(found[&PageClass::new("renaming")], pipelines(&["renaming", "merging"]));
        }

        #[test]
        fn an_id_without_a_pipeline_group_maps_the_unnamed_pipeline() {
            let d = dir("ID,Page type\nA-EN24,investments\n");
            let found = get_mapping(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap();
            assert_eq!(found[&PageClass::new("investments")], pipelines(&[""]));
        }

        #[test]
        fn rows_of_other_formats_are_ignored() {
            let d = dir("ID,Page type\nA-EN24(a),alpha\nB-EN24(b),beta\n");
            let found = get_mapping(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap();
            assert_eq!(found.keys().collect::<Vec<_>>(), vec![&PageClass::new("alpha")]);
        }

        #[test]
        fn a_format_absent_from_the_file_maps_each_of_its_pipelines_onto_its_own_name() {
            let d = dir("ID,Page type\nB-EN24(b),beta\n");
            let found = get_mapping(d.path(), "A-EN24", &names(), &pipelines(&["investments", "merges"])).unwrap();
            assert_eq!(found[&PageClass::new("investments")], pipelines(&["investments"]));
            assert_eq!(found[&PageClass::new("merges")], pipelines(&["merges"]));
        }

        #[test]
        fn the_fallback_leaves_out_the_pipelines_that_classify_pages() {
            let d = repo(&[
                ("mapping.csv", "ID,Page type\nB-EN24(b),beta\n"),
                ("pageclassify_overwrite.csv", "ID\nA-EN24(cover)\n"),
            ]);
            let found = get_mapping(d.path(), "A-EN24", &names(), &pipelines(&["cover", "investments"])).unwrap();
            assert_eq!(found.keys().collect::<Vec<_>>(), vec![&PageClass::new("investments")]);
        }

        #[test]
        fn the_fallback_of_a_format_with_no_pipelines_is_empty() {
            let d = dir("ID,Page type\nB-EN24(b),beta\n");
            assert!(get_mapping(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap().is_empty());
        }

        #[test]
        fn an_id_carrying_an_index_is_rejected() {
            let d = dir("ID,Page type\nA-EN24(a)/1,alpha\n");
            let err = get_mapping(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap_err();
            assert!(matches!(err, OrchestrationError::InvalidId { line: 1, .. }));
        }

        #[test]
        fn a_row_missing_the_page_type_cell_is_an_error() {
            let d = dir("ID,Page type\nA-EN24(a)\n");
            assert!(get_mapping(d.path(), "A-EN24", &names(), &pipelines(&[])).is_err());
        }

        #[test]
        fn an_unknown_format_name_is_an_error() {
            let d = dir("ID,Page type\nGHOST-EN24(a),alpha\n");
            let err = get_mapping(d.path(), "A-EN24", &names(), &pipelines(&[])).unwrap_err();
            assert!(matches!(err, OrchestrationError::UnknownFormatName { line: 1, .. }));
        }
    }

    mod real_repository_shapes {
        use super::*;
        use pretty_assertions::assert_eq;

        /// Righe reali di `analysis_finance_reports_formats/content/orchestration/`, riprodotte in
        /// una `TempDir`.
        #[test]
        fn reproduces_the_anima_sgr_schedule() {
            let csv = "Format name,Page type,Filter next iteration\n\
                       ANIMA_SGR-IT24,investments,TRUE\n\
                       ANIMA_SGR-IT24,merges,\n\
                       ANIMA_SGR-IT24,sfdr_classification,\n\
                       CARNE-EN23,investments,TRUE\n";
            let d = repo(&[
                ("algorithms_schedule.csv", csv),
                ("mapping.csv", "ID,Page type\n"),
                ("pageclassify_overwrite.csv", EMPTY_OVERWRITE),
            ]);
            let known = vec!["ANIMA_SGR-IT24".to_string(), "CARNE-EN23".to_string()];
            let schedule = get_schedule(d.path(), "ANIMA_SGR-IT24", &known, &pipelines(&[])).unwrap();
            assert_eq!(
                classes_of(&schedule),
                vec![
                    vec!["investments".to_string()],
                    vec!["merges".to_string(), "sfdr_classification".to_string()]
                ]
            );
        }

        #[test]
        fn reproduces_the_eurizon_mapping_where_two_pipelines_share_one_page_class() {
            let csv = "ID,Page type\n\
                       MEDIOLANUM-ES24.B(subfund),cover\n\
                       MEDIOLANUM-ES24.B(investments),investments\n\
                       EURIZON-EN23(renaming),renaming\n\
                       EURIZON-EN23(merging),renaming\n";
            let d = repo(&[("mapping.csv", csv), ("pageclassify_overwrite.csv", EMPTY_OVERWRITE)]);
            let known = vec!["MEDIOLANUM-ES24.B".to_string(), "EURIZON-EN23".to_string()];
            let found = get_mapping(d.path(), "EURIZON-EN23", &known, &pipelines(&[])).unwrap();
            assert_eq!(found[&PageClass::new("renaming")], pipelines(&["renaming", "merging"]));
        }
    }
}
