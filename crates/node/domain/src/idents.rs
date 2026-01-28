//! Identifiers

use alloy_evm::revm::primitives::B256;
use bytes::{Buf, BufMut};
use commonware_codec::{Error as CodecError, FixedSize, Read, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Block identifier (32 bytes).
pub struct BlockId(pub B256);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Transaction identifier (32 bytes).
pub struct TxId(pub B256);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// State commitment (32 bytes) computed from merkleized, non-durable QMDB partition roots.
pub struct StateRoot(pub B256);

impl FixedSize for BlockId {
    const SIZE: usize = 32;
}

impl FixedSize for TxId {
    const SIZE: usize = 32;
}

impl FixedSize for StateRoot {
    const SIZE: usize = 32;
}

/// Encode a `B256` into the buffer as raw bytes.
pub fn write_b256(value: &B256, buf: &mut impl BufMut) {
    buf.put_slice(value.as_slice());
}

/// Decode a `B256` from the buffer, returning an error if insufficient bytes remain.
pub fn read_b256(buf: &mut impl Buf) -> Result<B256, CodecError> {
    if buf.remaining() < 32 {
        return Err(CodecError::EndOfBuffer);
    }
    let mut out = [0u8; 32];
    buf.copy_to_slice(&mut out);
    Ok(B256::from(out))
}

impl Write for BlockId {
    fn write(&self, buf: &mut impl BufMut) {
        write_b256(&self.0, buf);
    }
}

impl Read for BlockId {
    type Cfg = ();

    fn read_cfg(buf: &mut impl Buf, _: &Self::Cfg) -> Result<Self, CodecError> {
        Ok(Self(read_b256(buf)?))
    }
}

impl Write for TxId {
    fn write(&self, buf: &mut impl BufMut) {
        write_b256(&self.0, buf);
    }
}

impl Read for TxId {
    type Cfg = ();

    fn read_cfg(buf: &mut impl Buf, _: &Self::Cfg) -> Result<Self, CodecError> {
        Ok(Self(read_b256(buf)?))
    }
}

impl Write for StateRoot {
    fn write(&self, buf: &mut impl BufMut) {
        write_b256(&self.0, buf);
    }
}

impl Read for StateRoot {
    type Cfg = ();

    fn read_cfg(buf: &mut impl Buf, _: &Self::Cfg) -> Result<Self, CodecError> {
        Ok(Self(read_b256(buf)?))
    }
}

#[cfg(test)]
mod tests {
    use alloy_evm::revm::primitives::{Address, B256, Bytes, U256, keccak256};
    use commonware_codec::{Decode as _, Encode as _};

    use super::*;
    use crate::{Block, BlockCfg, Tx, TxCfg};

    fn cfg() -> BlockCfg {
        BlockCfg { max_txs: 64, tx: TxCfg { max_calldata_bytes: 1024 } }
    }

    #[test]
    fn test_tx_roundtrip_and_id_stable() {
        let tx = Tx {
            from: Address::from([0x11u8; 20]),
            to: Address::from([0x22u8; 20]),
            value: U256::from(1234u64),
            gas_limit: 50_000,
            data: Bytes::from(vec![1, 2, 3]),
        };
        let encoded = tx.encode();
        let decoded = Tx::decode_cfg(encoded.clone(), &TxCfg { max_calldata_bytes: 1024 })
            .expect("decode tx");
        assert_eq!(tx, decoded);
        assert_eq!(tx.id(), decoded.id());
        assert_eq!(tx.id(), TxId(keccak256(encoded)));
    }

    #[test]
    fn test_block_roundtrip_and_id_stable() {
        let txs = vec![
            Tx {
                from: Address::from([0x11u8; 20]),
                to: Address::from([0x22u8; 20]),
                value: U256::from(1u64),
                gas_limit: 21_000,
                data: Bytes::new(),
            },
            Tx {
                from: Address::from([0x33u8; 20]),
                to: Address::from([0x44u8; 20]),
                value: U256::from(2u64),
                gas_limit: 50_000,
                data: Bytes::from(vec![9, 9, 9]),
            },
        ];
        let block = Block {
            parent: BlockId(B256::from([0xAAu8; 32])),
            height: 7,
            prevrandao: B256::from([0x55u8; 32]),
            state_root: StateRoot(B256::from([0xBBu8; 32])),
            txs,
        };
        let encoded = block.encode();
        let decoded = Block::decode_cfg(encoded.clone(), &cfg()).expect("decode block");
        assert_eq!(block, decoded);
        assert_eq!(block.id(), decoded.id());
        assert_eq!(block.id(), BlockId(keccak256(encoded)));
    }
}
