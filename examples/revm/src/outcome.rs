//! Contains the simulation outcome.

use std::fmt;

use alloy_evm::revm::primitives::{B256, U256};
use kora_domain::{ConsensusDigest, StateRoot};

#[derive(Clone, Copy, Debug)]
/// Summary of a completed simulation run.
pub(crate) struct SimOutcome {
    /// Finalized head digest (the value ordered by threshold-simplex).
    pub(crate) head: ConsensusDigest,
    /// State commitment at the head digest.
    pub(crate) state_root: StateRoot,
    /// Latest tracked threshold-simplex seed hash (used as `prevrandao`).
    pub(crate) seed: B256,
    /// Final balance of the sender account after the demo transfer.
    pub(crate) from_balance: U256,
    /// Final balance of the receiver account after the demo transfer.
    pub(crate) to_balance: U256,
}

impl fmt::Display for SimOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "finalized head: {:?}", self.head)?;
        writeln!(f, "state root: {:?}", self.state_root)?;
        writeln!(f, "seed: {:?}", self.seed)?;
        writeln!(f, "from balance: {:?}", self.from_balance)?;
        write!(f, "to balance: {:?}", self.to_balance)
    }
}
