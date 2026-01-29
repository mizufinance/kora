//! RPC server for Kora nodes.
//!
//! Provides HTTP endpoints for querying node status and chain state.

#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod server;
mod state;

pub use server::RpcServer;
pub use state::{NodeState, NodeStatus};
