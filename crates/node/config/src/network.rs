//! Network configuration.

use serde::{Deserialize, Serialize};

/// Default listen address.
pub const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:30303";

/// Network layer configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkConfig {
    /// Address to listen for P2P connections.
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// Bootstrap peers to connect to on startup.
    #[serde(default)]
    pub bootstrap_peers: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self { listen_addr: DEFAULT_LISTEN_ADDR.to_string(), bootstrap_peers: Vec::new() }
    }
}

fn default_listen_addr() -> String {
    DEFAULT_LISTEN_ADDR.to_string()
}
