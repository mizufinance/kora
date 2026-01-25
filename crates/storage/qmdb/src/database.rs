//! QMDB-backed database implementing REVM's Database traits.

use kora_primitives::{Address, B256, DefaultHashBuilder, HashMap, KECCAK256_EMPTY, U256};
use revm::{
    bytecode::Bytecode,
    database_interface::{DatabaseCommit, DatabaseRef},
    state::{Account, AccountInfo},
};

use crate::{QmdbBatchable, QmdbError, QmdbGettable, StorageKey, decode_account, encode_account};

/// QMDB-backed database implementing REVM's Database traits.
///
/// Wraps three QMDB stores for accounts, storage, and code.
/// Uses QMDB's built-in batching for atomic writes.
///
/// # Type Parameters
///
/// - `A`: Account store implementing get/batch operations
/// - `S`: Storage store implementing get/batch operations
/// - `C`: Code store implementing get/batch operations
#[derive(Debug)]
pub struct QmdbDatabase<A, S, C> {
    accounts: A,
    storage: S,
    code: C,
}

impl<A, S, C> QmdbDatabase<A, S, C> {
    /// Create a new database from QMDB stores.
    pub const fn new(accounts: A, storage: S, code: C) -> Self {
        Self { accounts, storage, code }
    }

    /// Get a reference to the accounts store.
    pub const fn accounts(&self) -> &A {
        &self.accounts
    }

    /// Get a reference to the storage store.
    pub const fn storage(&self) -> &S {
        &self.storage
    }

    /// Get a reference to the code store.
    pub const fn code(&self) -> &C {
        &self.code
    }
}

impl<A, S, C> DatabaseRef for QmdbDatabase<A, S, C>
where
    A: QmdbGettable<Key = Address, Value = [u8; 72]>,
    S: QmdbGettable<Key = StorageKey, Value = U256>,
    C: QmdbGettable<Key = B256, Value = Vec<u8>>,
{
    type Error = QmdbError;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        match self.accounts.get(&address) {
            Ok(Some(bytes)) => {
                let (nonce, balance, code_hash) = decode_account(&bytes)
                    .ok_or_else(|| QmdbError::Storage("decode failed".into()))?;
                Ok(Some(AccountInfo { nonce, balance, code_hash, code: None }))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(QmdbError::Storage(e.to_string())),
        }
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        if code_hash == KECCAK256_EMPTY {
            return Ok(Bytecode::default());
        }
        match self.code.get(&code_hash) {
            Ok(Some(bytes)) => Ok(Bytecode::new_raw(bytes.into())),
            Ok(None) => Err(QmdbError::CodeNotFound(code_hash)),
            Err(e) => Err(QmdbError::Storage(e.to_string())),
        }
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        let key = StorageKey::new(address, index);
        match self.storage.get(&key) {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Ok(U256::ZERO),
            Err(e) => Err(QmdbError::Storage(e.to_string())),
        }
    }

    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        Err(QmdbError::BlockHashNotFound(number))
    }
}

impl<A, S, C> DatabaseCommit for QmdbDatabase<A, S, C>
where
    A: QmdbBatchable<Key = Address, Value = [u8; 72]>,
    S: QmdbBatchable<Key = StorageKey, Value = U256>,
    C: QmdbBatchable<Key = B256, Value = Vec<u8>>,
{
    fn commit(&mut self, changes: HashMap<Address, Account, DefaultHashBuilder>) {
        let mut account_ops = Vec::new();
        let mut storage_ops = Vec::new();
        let mut code_ops = Vec::new();

        for (address, account) in changes {
            if account.is_selfdestructed() {
                account_ops.push((address, None));
            } else if account.is_touched() {
                let encoded = encode_account(
                    account.info.nonce,
                    account.info.balance,
                    account.info.code_hash,
                );
                account_ops.push((address, Some(encoded)));

                if let Some(ref code) = account.info.code
                    && account.info.code_hash != KECCAK256_EMPTY
                {
                    code_ops.push((account.info.code_hash, Some(code.bytes().to_vec())));
                }
            }

            for (slot, value) in account.storage {
                let key = StorageKey::new(address, slot);
                if value.present_value().is_zero() {
                    storage_ops.push((key, None));
                } else {
                    storage_ops.push((key, Some(value.present_value())));
                }
            }
        }

        let _ = self.accounts.write_batch(account_ops);
        let _ = self.storage.write_batch(storage_ops);
        let _ = self.code.write_batch(code_ops);
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap as StdHashMap, sync::Mutex};

    use revm::state::EvmStorageSlot;
    use rstest::rstest;

    use super::*;

    #[derive(Debug, Default)]
    struct MemoryStore<K, V> {
        data: Mutex<StdHashMap<K, V>>,
    }

    impl<K, V> MemoryStore<K, V> {
        fn new() -> Self {
            Self { data: Mutex::new(StdHashMap::new()) }
        }
    }

    #[derive(Debug)]
    struct MemoryError;

    impl std::fmt::Display for MemoryError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "memory error")
        }
    }

    impl std::error::Error for MemoryError {}

    impl<K: Clone + Eq + std::hash::Hash, V: Clone> QmdbGettable for MemoryStore<K, V> {
        type Error = MemoryError;
        type Key = K;
        type Value = V;

        fn get(&self, key: &Self::Key) -> Result<Option<Self::Value>, Self::Error> {
            Ok(self.data.lock().unwrap().get(key).cloned())
        }
    }

    impl<K: Clone + Eq + std::hash::Hash, V: Clone> QmdbBatchable for MemoryStore<K, V> {
        fn write_batch<I>(&mut self, ops: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = (Self::Key, Option<Self::Value>)>,
        {
            let mut data = self.data.lock().unwrap();
            for (key, value) in ops {
                match value {
                    Some(v) => {
                        data.insert(key, v);
                    }
                    None => {
                        data.remove(&key);
                    }
                }
            }
            Ok(())
        }
    }

    type TestDb = QmdbDatabase<
        MemoryStore<Address, [u8; 72]>,
        MemoryStore<StorageKey, U256>,
        MemoryStore<B256, Vec<u8>>,
    >;

    fn create_test_db() -> TestDb {
        QmdbDatabase::new(MemoryStore::new(), MemoryStore::new(), MemoryStore::new())
    }

    #[rstest]
    fn basic_ref_returns_none_for_missing_account() {
        let db = create_test_db();
        let result = db.basic_ref(Address::ZERO).unwrap();
        assert!(result.is_none());
    }

    #[rstest]
    fn storage_ref_returns_zero_for_missing_slot() {
        let db = create_test_db();
        let result = db.storage_ref(Address::ZERO, U256::from(1)).unwrap();
        assert_eq!(result, U256::ZERO);
    }

    #[rstest]
    fn code_by_hash_returns_empty_for_keccak_empty() {
        let db = create_test_db();
        let result = db.code_by_hash_ref(KECCAK256_EMPTY).unwrap();
        assert!(result.is_empty());
    }

    #[rstest]
    fn code_by_hash_returns_error_for_missing_code() {
        let db = create_test_db();
        let hash = B256::repeat_byte(0xAB);
        let result = db.code_by_hash_ref(hash);
        assert!(matches!(result, Err(QmdbError::CodeNotFound(_))));
    }

    #[rstest]
    fn block_hash_returns_error() {
        let db = create_test_db();
        let result = db.block_hash_ref(100);
        assert!(matches!(result, Err(QmdbError::BlockHashNotFound(100))));
    }

    #[rstest]
    fn commit_stores_account_info() {
        let mut db = create_test_db();
        let address = Address::repeat_byte(0x42);

        let mut changes: HashMap<Address, Account, DefaultHashBuilder> = HashMap::default();
        let mut account = Account::default();
        account.info.nonce = 5;
        account.info.balance = U256::from(1000);
        account.mark_touch();
        changes.insert(address, account);

        db.commit(changes);

        let info = db.basic_ref(address).unwrap().unwrap();
        assert_eq!(info.nonce, 5);
        assert_eq!(info.balance, U256::from(1000));
    }

    #[rstest]
    fn commit_stores_storage_slots() {
        let mut db = create_test_db();
        let address = Address::repeat_byte(0x42);
        let slot = U256::from(1);
        let value = U256::from(999);

        let mut changes: HashMap<Address, Account, DefaultHashBuilder> = HashMap::default();
        let mut account = Account::default();
        account.mark_touch();
        account.storage.insert(slot, EvmStorageSlot::new(value));
        changes.insert(address, account);

        db.commit(changes);

        let stored = db.storage_ref(address, slot).unwrap();
        assert_eq!(stored, value);
    }

    #[rstest]
    fn commit_deletes_zero_storage_slots() {
        let mut db = create_test_db();
        let address = Address::repeat_byte(0x42);
        let slot = U256::from(1);

        {
            let mut changes: HashMap<Address, Account, DefaultHashBuilder> = HashMap::default();
            let mut account = Account::default();
            account.mark_touch();
            account.storage.insert(slot, EvmStorageSlot::new(U256::from(123)));
            changes.insert(address, account);
            db.commit(changes);
        }

        {
            let mut changes: HashMap<Address, Account, DefaultHashBuilder> = HashMap::default();
            let mut account = Account::default();
            account.mark_touch();
            account.storage.insert(slot, EvmStorageSlot::new(U256::ZERO));
            changes.insert(address, account);
            db.commit(changes);
        }

        let stored = db.storage_ref(address, slot).unwrap();
        assert_eq!(stored, U256::ZERO);
    }
}
