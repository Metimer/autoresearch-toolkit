//! Agent-independent contracts. This crate does not execute commands or edit repositories.
pub mod config;

pub use config::{ConfigError, SessionConfig, ValidatedConfig, MAX_CONFIG_BYTES};
