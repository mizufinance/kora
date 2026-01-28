//! Boostrap configuration.

use alloy_evm::revm::primitives::{Address, U256};

use crate::Tx;

/// Genesis allocation plus bootstrap transactions applied before consensus starts.
///
/// # Examples
/// ```no_run
/// use alloy_evm::revm::primitives::{Address, Bytes, U256};
/// use kora_revm_example::{BootstrapConfig, Tx};
///
/// let from = Address::from([0x11u8; 20]);
/// let to = Address::from([0x22u8; 20]);
/// let tx = Tx {
///     from,
///     to,
///     value: U256::from(100u64),
///     gas_limit: 21_000,
///     data: Bytes::new(),
/// };
/// let bootstrap = BootstrapConfig::new(vec![(from, U256::from(1_000_000u64))], vec![tx]);
/// # let _ = bootstrap;
/// ```
#[derive(Clone, Debug)]
pub struct BootstrapConfig {
    /// Genesis allocation applied before consensus starts.
    pub genesis_alloc: Vec<(Address, U256)>,
    /// Transactions to submit before consensus starts.
    pub bootstrap_txs: Vec<Tx>,
}

impl BootstrapConfig {
    /// Construct a bootstrap configuration with genesis allocations and seed transactions.
    pub const fn new(genesis_alloc: Vec<(Address, U256)>, bootstrap_txs: Vec<Tx>) -> Self {
        Self { genesis_alloc, bootstrap_txs }
    }
}
