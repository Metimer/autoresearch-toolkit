//! Agent-independent contracts and isolated workspace storage. No experiments are executed.
pub mod config;
pub mod session;

pub use config::{ConfigError, SessionConfig, ValidatedConfig, MAX_CONFIG_BYTES};

pub mod workspace;
