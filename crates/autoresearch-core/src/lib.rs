//! Agent-independent bounded optimization engine.
pub mod config;
pub mod engine;
pub mod legacy;
pub mod measurement;
pub mod memory;
pub mod results;
pub mod session;
pub mod supervisor;
pub mod workspace;

pub use config::{ConfigError, SessionConfig, ValidatedConfig, MAX_CONFIG_BYTES};
