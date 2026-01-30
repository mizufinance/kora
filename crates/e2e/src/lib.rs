//! End-to-end testing framework for Kora consensus network.
//!
//! This crate provides a simulation-based testing infrastructure for running
//! multi-validator consensus tests without real networking. It enables testing
//! of:
//!
//! - Consensus finalization across multiple validators
//! - Transaction execution and state convergence
//! - Block production with proper ordering
//! - Network partition recovery
//! - Contract deployment and interaction
//!
//! # Running Tests
//!
//! Tests in this crate use file-based storage and are resource-intensive.
//! For reliable results, run with a single test thread:
//!
//! ```bash
//! cargo test -p kora-e2e -- --test-threads=1
//! ```

#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod harness;
mod node;
mod setup;

pub use harness::{HarnessError, TestHarness, TestOutcome};
pub use node::TestNode;
pub use setup::{TestConfig, TestSetup};

#[cfg(test)]
mod tests;
