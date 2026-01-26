//! Configuration error types.

use std::path::PathBuf;

/// Errors that can occur when loading or parsing configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read configuration file.
    #[error("failed to read config file {path}: {source}")]
    Read {
        /// The path that failed to read.
        path: PathBuf,
        /// The underlying IO error.
        source: std::io::Error,
    },

    /// Failed to parse TOML configuration.
    #[error("failed to parse TOML config: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// Failed to parse JSON configuration.
    #[error("failed to parse JSON config: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Failed to serialize configuration to TOML.
    #[error("failed to serialize config to TOML: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}
