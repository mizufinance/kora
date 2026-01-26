//! RPC configuration.

use serde::{Deserialize, Serialize};

/// Default HTTP RPC address.
pub const DEFAULT_HTTP_ADDR: &str = "0.0.0.0:8545";

/// Default WebSocket RPC address.
pub const DEFAULT_WS_ADDR: &str = "0.0.0.0:8546";

/// RPC server configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RpcConfig {
    /// HTTP JSON-RPC server address.
    #[serde(default = "default_http_addr")]
    pub http_addr: String,

    /// WebSocket server address.
    #[serde(default = "default_ws_addr")]
    pub ws_addr: String,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self { http_addr: DEFAULT_HTTP_ADDR.to_string(), ws_addr: DEFAULT_WS_ADDR.to_string() }
    }
}

fn default_http_addr() -> String {
    DEFAULT_HTTP_ADDR.to_string()
}

fn default_ws_addr() -> String {
    DEFAULT_WS_ADDR.to_string()
}
