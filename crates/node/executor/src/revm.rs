//! REVM-based block executor.

use std::collections::BTreeMap;

use alloy_consensus::Header;
use alloy_primitives::{B256, Bytes, U256, keccak256};
use kora_qmdb::{AccountUpdate, ChangeSet};
use kora_traits::StateDb;
use revm::{
    Context, ExecuteEvm, Journal, MainBuilder,
    bytecode::Bytecode,
    context::{
        block::BlockEnv,
        result::{ExecutionResult, Output},
    },
    context_interface::transaction::{AccessList, AccessListItem},
    database::State,
    primitives::{TxKind, hardfork::SpecId},
    state::{EvmState, EvmStorageSlot},
};

use crate::{
    BlockContext, BlockExecutor, ExecutionError, ExecutionOutcome, ExecutionReceipt, StateDbAdapter,
};

/// REVM-based block executor.
///
/// This executor uses REVM to execute EVM transactions against a state database.
/// The actual EVM execution is performed via the REVM handler traits.
#[derive(Clone, Debug, Default)]
pub struct RevmExecutor {
    /// Chain ID for transaction validation.
    chain_id: u64,
}

impl RevmExecutor {
    /// Create a new REVM executor with the given chain ID.
    pub const fn new(chain_id: u64) -> Self {
        Self { chain_id }
    }

    /// Get the chain ID.
    pub const fn chain_id(&self) -> u64 {
        self.chain_id
    }
}

impl<S: StateDb> BlockExecutor<S> for RevmExecutor {
    type Tx = Bytes;

    fn execute(
        &self,
        state: &S,
        context: &BlockContext,
        txs: &[Self::Tx],
    ) -> Result<ExecutionOutcome, ExecutionError> {
        // Create adapter wrapping the state
        let adapter = StateDbAdapter::new(state.clone());

        // Build state from the adapter - wraps DatabaseRef in a State for caching
        let db = State::builder().with_database_ref(adapter).build();

        // Create context with block configuration
        // We need to specify the Journal type explicitly
        type Db<S> = State<revm::database::WrapDatabaseRef<StateDbAdapter<S>>>;
        let ctx: Context<BlockEnv, _, _, Db<S>, Journal<Db<S>>, ()> =
            Context::new(db, SpecId::CANCUN);
        let ctx = ctx
            .modify_cfg_chained(|cfg| {
                cfg.chain_id = self.chain_id;
            })
            .modify_block_chained(|blk: &mut BlockEnv| {
                blk.number = context.header.number;
                blk.timestamp = context.header.timestamp;
                blk.beneficiary = context.header.beneficiary;
                blk.gas_limit = context.header.gas_limit;
                blk.basefee = context.header.base_fee_per_gas.unwrap_or_default();
                blk.prevrandao = Some(context.prevrandao);
            });

        // Build EVM instance
        let mut evm = ctx.build_mainnet();

        let mut outcome = ExecutionOutcome::new();
        let mut cumulative_gas = 0u64;

        for tx_bytes in txs {
            let tx_hash = keccak256(tx_bytes);

            // Decode transaction and set environment
            let tx_env = decode_tx_env(tx_bytes, self.chain_id)?;
            evm.set_tx(tx_env);

            // Execute transaction
            let result_and_state =
                evm.replay().map_err(|e| ExecutionError::TxExecution(format!("{:?}", e)))?;

            let gas_used = result_and_state.result.gas_used();
            cumulative_gas = cumulative_gas.saturating_add(gas_used);

            // Build receipt from execution result
            let receipt =
                build_receipt(&result_and_state.result, tx_hash, gas_used, cumulative_gas);
            outcome.receipts.push(receipt);

            // Extract and merge state changes
            let changes = extract_changes(result_and_state.state);
            outcome.changes.merge(changes);
        }

        outcome.gas_used = cumulative_gas;
        Ok(outcome)
    }

    fn validate_header(&self, _header: &Header) -> Result<(), ExecutionError> {
        // Basic header validation - can be extended for specific rules
        Ok(())
    }
}

/// Decode transaction bytes into a REVM TxEnv.
///
/// Currently supports basic transaction decoding for all Ethereum transaction types.
fn decode_tx_env(tx_bytes: &Bytes, _chain_id: u64) -> Result<revm::context::TxEnv, ExecutionError> {
    use alloy_consensus::TxEnvelope;
    use alloy_rlp::Decodable;

    // Decode the transaction envelope
    let envelope = TxEnvelope::decode(&mut tx_bytes.as_ref())
        .map_err(|e| ExecutionError::TxDecode(format!("{}", e)))?;

    // Build TxEnv using the builder pattern
    let mut builder = revm::context::TxEnv::builder();

    match &envelope {
        TxEnvelope::Legacy(signed) => {
            let tx = signed.tx();
            let caller = signed.recover_signer().map_err(|e| {
                ExecutionError::TxDecode(format!("failed to recover signer: {}", e))
            })?;

            builder = builder
                .caller(caller)
                .gas_limit(tx.gas_limit)
                .gas_price(tx.gas_price)
                .value(tx.value)
                .data(tx.input.clone())
                .nonce(tx.nonce)
                .kind(convert_tx_kind(tx.to));
        }
        TxEnvelope::Eip2930(signed) => {
            let tx = signed.tx();
            let caller = signed.recover_signer().map_err(|e| {
                ExecutionError::TxDecode(format!("failed to recover signer: {}", e))
            })?;

            builder = builder
                .caller(caller)
                .gas_limit(tx.gas_limit)
                .gas_price(tx.gas_price)
                .value(tx.value)
                .data(tx.input.clone())
                .nonce(tx.nonce)
                .kind(convert_tx_kind(tx.to))
                .access_list(convert_access_list(&tx.access_list));
        }
        TxEnvelope::Eip1559(signed) => {
            let tx = signed.tx();
            let caller = signed.recover_signer().map_err(|e| {
                ExecutionError::TxDecode(format!("failed to recover signer: {}", e))
            })?;

            builder = builder
                .caller(caller)
                .gas_limit(tx.gas_limit)
                .gas_price(tx.max_fee_per_gas)
                .gas_priority_fee(Some(tx.max_priority_fee_per_gas))
                .value(tx.value)
                .data(tx.input.clone())
                .nonce(tx.nonce)
                .kind(convert_tx_kind(tx.to))
                .access_list(convert_access_list(&tx.access_list));
        }
        TxEnvelope::Eip4844(signed) => {
            let tx = signed.tx().tx();
            let caller = signed.recover_signer().map_err(|e| {
                ExecutionError::TxDecode(format!("failed to recover signer: {}", e))
            })?;

            builder = builder
                .caller(caller)
                .gas_limit(tx.gas_limit)
                .gas_price(tx.max_fee_per_gas)
                .gas_priority_fee(Some(tx.max_priority_fee_per_gas))
                .value(tx.value)
                .data(tx.input.clone())
                .nonce(tx.nonce)
                .kind(TxKind::Call(tx.to))
                .access_list(convert_access_list(&tx.access_list))
                .max_fee_per_blob_gas(tx.max_fee_per_blob_gas)
                .blob_hashes(tx.blob_versioned_hashes.clone());
        }
        TxEnvelope::Eip7702(signed) => {
            let tx = signed.tx();
            let caller = signed.recover_signer().map_err(|e| {
                ExecutionError::TxDecode(format!("failed to recover signer: {}", e))
            })?;

            builder = builder
                .caller(caller)
                .gas_limit(tx.gas_limit)
                .gas_price(tx.max_fee_per_gas)
                .gas_priority_fee(Some(tx.max_priority_fee_per_gas))
                .value(tx.value)
                .data(tx.input.clone())
                .nonce(tx.nonce)
                .kind(TxKind::Call(tx.to))
                .access_list(convert_access_list(&tx.access_list))
                .authorization_list(convert_authorization_list(&tx.authorization_list));
        }
    }

    builder
        .build()
        .map_err(|e| ExecutionError::TxDecode(format!("failed to build tx env: {:?}", e)))
}

/// Convert alloy TxKind to revm TxKind.
const fn convert_tx_kind(kind: alloy_primitives::TxKind) -> TxKind {
    match kind {
        alloy_primitives::TxKind::Call(addr) => TxKind::Call(addr),
        alloy_primitives::TxKind::Create => TxKind::Create,
    }
}

/// Convert alloy AccessList to revm AccessList.
fn convert_access_list(access_list: &alloy_eips::eip2930::AccessList) -> AccessList {
    AccessList(
        access_list
            .iter()
            .map(|item| AccessListItem {
                address: item.address,
                storage_keys: item.storage_keys.clone(),
            })
            .collect(),
    )
}

/// Convert alloy authorization list to revm authorization list.
fn convert_authorization_list(
    auth_list: &[alloy_eips::eip7702::SignedAuthorization],
) -> Vec<
    revm::context_interface::either::Either<
        revm::context_interface::transaction::SignedAuthorization,
        revm::context_interface::transaction::RecoveredAuthorization,
    >,
> {
    use alloy_eips::eip7702::RecoveredAuthority;

    auth_list
        .iter()
        .map(|auth| {
            // Build the inner authorization
            let inner = revm::context_interface::transaction::Authorization {
                chain_id: *auth.chain_id(),
                address: *auth.address(),
                nonce: auth.nonce(),
            };

            // Convert to recovered authorization - use Valid if recovery succeeds, Invalid otherwise
            let recovered_authority = auth
                .recover_authority()
                .map_or(RecoveredAuthority::Invalid, RecoveredAuthority::Valid);

            revm::context_interface::either::Either::Right(
                revm::context_interface::transaction::RecoveredAuthorization::new_unchecked(
                    inner,
                    recovered_authority,
                ),
            )
        })
        .collect()
}

/// Build a transaction receipt from execution result.
fn build_receipt(
    result: &ExecutionResult,
    tx_hash: B256,
    gas_used: u64,
    cumulative_gas_used: u64,
) -> ExecutionReceipt {
    let (success, logs, contract_address) = match result {
        ExecutionResult::Success { logs, output, .. } => {
            let contract_addr = match output {
                Output::Create(_, addr) => *addr,
                Output::Call(_) => None,
            };
            // REVM logs are already alloy_primitives::Log, just clone them
            (true, logs.clone(), contract_addr)
        }
        ExecutionResult::Revert { .. } => (false, Vec::new(), None),
        ExecutionResult::Halt { .. } => (false, Vec::new(), None),
    };

    ExecutionReceipt::new(tx_hash, success, gas_used, cumulative_gas_used, logs, contract_address)
}

/// Extract state changes from REVM execution state.
fn extract_changes(state: EvmState) -> ChangeSet {
    let mut changes = ChangeSet::new();

    for (address, account) in state {
        // Skip untouched accounts
        if !account.is_touched() {
            continue;
        }

        // Extract storage changes
        let storage: BTreeMap<U256, U256> = account
            .storage
            .iter()
            .map(|(k, v): (&U256, &EvmStorageSlot)| (*k, v.present_value()))
            .collect();

        // Extract code if present
        let code = account.info.code.as_ref().map(|c: &Bytecode| c.bytes().to_vec());

        let update = AccountUpdate {
            created: account.is_created(),
            selfdestructed: account.is_selfdestructed(),
            nonce: account.info.nonce,
            balance: account.info.balance,
            code_hash: account.info.code_hash,
            code,
            storage,
        };

        changes.insert(address, update);
    }

    changes
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, KECCAK256_EMPTY};
    use revm::state::Account;

    use super::*;

    #[test]
    fn revm_executor_new() {
        let executor = RevmExecutor::new(1);
        assert_eq!(executor.chain_id(), 1);
    }

    #[test]
    fn revm_executor_default() {
        let executor = RevmExecutor::default();
        assert_eq!(executor.chain_id(), 0);
    }

    #[test]
    fn build_receipt_success() {
        let result = ExecutionResult::Success {
            reason: revm::context::result::SuccessReason::Stop,
            gas_used: 21000,
            gas_refunded: 0,
            logs: vec![],
            output: Output::Call(Bytes::new()),
        };

        let receipt = build_receipt(&result, B256::ZERO, 21000, 21000);
        assert!(receipt.success());
        assert_eq!(receipt.gas_used, 21000);
        assert_eq!(receipt.cumulative_gas_used(), 21000);
        assert!(receipt.logs().is_empty());
        assert!(receipt.contract_address.is_none());
    }

    #[test]
    fn build_receipt_revert() {
        let result = ExecutionResult::Revert { gas_used: 21000, output: Bytes::new() };

        let receipt = build_receipt(&result, B256::ZERO, 21000, 21000);
        assert!(!receipt.success());
        assert_eq!(receipt.gas_used, 21000);
    }

    #[test]
    fn build_receipt_halt() {
        let result = ExecutionResult::Halt {
            reason: revm::context::result::HaltReason::OutOfGas(
                revm::context::result::OutOfGasError::Basic,
            ),
            gas_used: 21000,
        };

        let receipt = build_receipt(&result, B256::ZERO, 21000, 21000);
        assert!(!receipt.success());
        assert_eq!(receipt.gas_used, 21000);
    }

    #[test]
    fn extract_changes_empty() {
        let state = EvmState::default();
        let changes = extract_changes(state);
        assert!(changes.is_empty());
    }

    #[test]
    fn extract_changes_touched_account() {
        use revm::state::AccountStatus;

        let mut state = EvmState::default();

        let mut account = Account::default();
        account.info.nonce = 1;
        account.info.balance = U256::from(1000);
        account.info.code_hash = KECCAK256_EMPTY;
        account.status = AccountStatus::Touched;

        // Add a storage change
        account
            .storage
            .insert(U256::from(1), EvmStorageSlot::new_changed(U256::ZERO, U256::from(42)));

        state.insert(Address::ZERO, account);

        let changes = extract_changes(state);
        assert_eq!(changes.len(), 1);

        let update = changes.accounts.get(&Address::ZERO).unwrap();
        assert_eq!(update.nonce, 1);
        assert_eq!(update.balance, U256::from(1000));
        assert_eq!(update.storage.get(&U256::from(1)), Some(&U256::from(42)));
    }

    #[test]
    fn extract_changes_untouched_skipped() {
        use revm::state::AccountStatus;

        let mut state = EvmState::default();

        let mut account = Account::default();
        account.info.nonce = 1;
        account.info.balance = U256::from(1000);
        account.status = AccountStatus::Loaded; // Not touched

        state.insert(Address::ZERO, account);

        let changes = extract_changes(state);
        assert!(changes.is_empty());
    }

    #[test]
    fn extract_changes_created_account() {
        use revm::state::AccountStatus;

        let mut state = EvmState::default();

        // Created accounts also need to be touched to be processed
        let account = Account {
            status: AccountStatus::Created | AccountStatus::Touched,
            ..Default::default()
        };

        state.insert(Address::ZERO, account);

        let changes = extract_changes(state);
        assert_eq!(changes.len(), 1);

        let update = changes.accounts.get(&Address::ZERO).unwrap();
        assert!(update.created);
    }

    #[test]
    fn extract_changes_selfdestructed() {
        use revm::state::AccountStatus;

        let mut state = EvmState::default();

        let mut account = Account::default();
        account.info.nonce = 5;
        account.info.balance = U256::from(100);
        // SelfDestructed accounts also need to be touched to be processed
        account.status = AccountStatus::SelfDestructed | AccountStatus::Touched;

        state.insert(Address::ZERO, account);

        let changes = extract_changes(state);
        assert_eq!(changes.len(), 1);

        let update = changes.accounts.get(&Address::ZERO).unwrap();
        assert!(update.selfdestructed);
    }
}
