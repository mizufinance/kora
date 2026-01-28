//! REVM-based example chain driven by threshold-simplex.
//!
//! This example uses `alloy-evm` as the integration layer above `revm` and keeps the execution
//! backend generic over the database trait boundary (`Database` + `DatabaseCommit`).

pub mod application;
pub use application::execution::{
    CHAIN_ID, ExecutionOutcome, SEED_PRECOMPILE_ADDRESS_BYTES, evm_env, execute_txs,
    seed_precompile_address,
};

mod cli;
mod config;
mod outcome;
mod qmdb;
mod simulation;

fn main() {
    use clap::Parser;

    // Parse the cli.
    let cli = cli::Cli::parse();

    // Run the simulation.
    let outcome = cli.run();
    if outcome.is_err() {
        eprintln!("Simulation failed: {:?}", outcome);
        std::process::exit(1);
    };

    // Print the output.
    let outcome = outcome.expect("success");
    println!("{}", outcome);
}
