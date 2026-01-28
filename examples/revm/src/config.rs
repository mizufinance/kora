//! Contains the simulation config.

#[derive(Clone, Copy, Debug)]
/// Configuration for a simulation run.
pub(crate) struct SimConfig {
    /// Number of nodes participating in the simulation.
    pub(crate) nodes: usize,
    /// Number of blocks to finalize before stopping.
    pub(crate) blocks: u64,
    /// Seed used for deterministic randomness.
    pub(crate) seed: u64,
}
