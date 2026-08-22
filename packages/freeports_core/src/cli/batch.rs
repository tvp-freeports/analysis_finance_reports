use std::collections::BTreeMap;
use std::path::Path;

use super::freeports_config::{FreeportsConfig, FreeportsConfigError};
use super::config_locations::job::{parse_row, JobConfigError};
use super::partial_config::{ConfigLocations, ConfigSource, PartialConfig};


#[derive(Debug)]
pub enum BatchError {
    Csv(csv::Error),
    Row(JobConfigError),
    Config(FreeportsConfigError),
}

impl std::fmt::Display for BatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchError::Csv(e) => write!(f, "batch file: {e}"),
            BatchError::Row(e) => write!(f, "batch file row: {e}"),
            BatchError::Config(e) => write!(f, "batch job configuration: {e}"),
        }
    }
}

impl std::error::Error for BatchError {}

impl From<csv::Error> for BatchError {
    fn from(e: csv::Error) -> Self {
        BatchError::Csv(e)
    }
}
impl From<JobConfigError> for BatchError {
    fn from(e: JobConfigError) -> Self {
        BatchError::Row(e)
    }
}
impl From<FreeportsConfigError> for BatchError {
    fn from(e: FreeportsConfigError) -> Self {
        BatchError::Config(e)
    }
}

pub fn load_batch_jobs(base: &PartialConfig, batch_file: &Path) -> Result<Vec<FreeportsConfig>, BatchError> {
    let mut reader = csv::Reader::from_path(batch_file)?;
    let headers = reader.headers()?.clone();

    let mut jobs = Vec::new();
    for record in reader.records() {
        let record = record?;
        let row: BTreeMap<String, String> =
            headers.iter().zip(record.iter()).map(|(header, value)| (header.to_string(), value.to_string())).collect();

        let row_config = parse_row(&row)?;
        let mut locations = ConfigLocations::default();
        let merged = base.clone().overwrite(&row_config, ConfigSource::Job, &mut locations);
        let config = FreeportsConfig::build(merged)?;
        jobs.push(config);
    }
    Ok(jobs)
}
