//! Execution configuration.

use serde::{Deserialize, Serialize};

/// Default gas limit per block.
pub const DEFAULT_GAS_LIMIT: u64 = 30_000_000;

/// Default block time in seconds.
pub const DEFAULT_BLOCK_TIME: u64 = 2;

/// Execution layer configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionConfig {
    /// Maximum gas per block.
    #[serde(default = "default_gas_limit")]
    pub gas_limit: u64,

    /// Target block time in seconds.
    #[serde(default = "default_block_time")]
    pub block_time: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self { gas_limit: DEFAULT_GAS_LIMIT, block_time: DEFAULT_BLOCK_TIME }
    }
}

const fn default_gas_limit() -> u64 {
    DEFAULT_GAS_LIMIT
}

const fn default_block_time() -> u64 {
    DEFAULT_BLOCK_TIME
}
