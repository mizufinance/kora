//! Contains the CLI for the `kora-revm-example`.

use clap::Parser;

use crate::{config::SimConfig, outcome::SimOutcome, simulation::simulate};

/// CLI arguments for the kora-revm-example.
#[derive(Parser, Debug)]
#[command(name = "kora-revm-example")]
#[command(about = "threshold-simplex + EVM execution example")]
pub(crate) struct Cli {
    /// Number of nodes to simulate.
    #[arg(long, default_value = "4")]
    pub nodes: usize,

    /// Number of blocks to finalize.
    #[arg(long, default_value = "3")]
    pub blocks: u64,

    /// Random seed for the simulation.
    #[arg(long, default_value = "1")]
    pub seed: u64,
}

impl Cli {
    /// Run the simulation with the configured parameters.
    pub(crate) fn run(self) -> anyhow::Result<SimOutcome> {
        simulate(SimConfig { nodes: self.nodes, blocks: self.blocks, seed: self.seed })
    }
}
