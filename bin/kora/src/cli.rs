//! Contains the CLI for the `kora-node`.

use std::path::PathBuf;

use clap::Parser;
use kora_config::NodeConfig;

/// CLI arguments for the kora node.
#[derive(Parser, Debug)]
#[command(name = "kora")]
#[command(about = "A minimal commonware + revm execution client")]
pub(crate) struct Cli {
    /// Path to the configuration file (TOML or JSON).
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Enable verbose logging.
    #[arg(short, long)]
    pub verbose: bool,

    /// Override chain ID from config.
    #[arg(long)]
    pub chain_id: Option<u64>,

    /// Override data directory from config.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
}

impl Cli {
    /// Load the node configuration, applying CLI overrides.
    pub(crate) fn load_config(&self) -> eyre::Result<NodeConfig> {
        let mut config = NodeConfig::load(self.config.as_deref())?;

        // Apply CLI overrides
        if let Some(chain_id) = self.chain_id {
            config.chain_id = chain_id;
        }
        if let Some(ref data_dir) = self.data_dir {
            config.data_dir = data_dir.clone();
        }

        Ok(config)
    }

    /// Run the kora node with the loaded configuration.
    pub(crate) async fn run(self) -> eyre::Result<()> {
        let config = self.load_config()?;

        tracing::info!(chain_id = config.chain_id, "Loaded configuration");
        tracing::debug!(?config, "Full configuration");

        // TODO: Build and run the node with the loaded configuration.
        // This will wire up:
        // - Consensus engine (simplex)
        // - Execution layer (revm)
        // - Network layer (p2p, rpc)
        // - Storage backend

        tracing::info!("Node startup complete (stub)");

        Ok(())
    }
}
