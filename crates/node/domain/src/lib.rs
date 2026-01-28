#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/refcell/kora/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod aliases;
pub use aliases::{ConsensusDigest, PublicKey, FinalizationEvent};

mod commitment;
pub use commitment::{AccountChange, StateChanges, StateChangesCfg};

mod events;
pub use events::{LedgerEvent, LedgerEvents};

mod bootstrap;
pub use bootstrap::BootstrapConfig;

mod block;
pub use block::{Block, BlockCfg};

mod idents;
pub use idents::{BlockId, write_b256, read_b256, StateRoot, TxId};

mod tx;
pub use tx::{Tx, TxCfg};
