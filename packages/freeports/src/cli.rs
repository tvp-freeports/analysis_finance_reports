//! The command-line interface: configuration, jobs, batches, execution.
//!
//! A run is configured from four sources — command line, environment, configuration file, batch CSV
//! — merged by precedence in [`config_locations`], resolved into one
//! [`freeports_config::FreeportsConfig`], and then executed by [`run`], one [`job`] at a time or
//! several at once in [`worker`] processes.

pub mod batch;
pub mod conf_parse;
pub mod config_locations;
pub mod freeports_config;
pub mod job;
pub mod output;
pub mod parallelism_config;
pub mod partial_config;
pub mod run;
pub mod worker;
