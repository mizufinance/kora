use bytes::{Buf, BufMut};
use commonware_codec::{EncodeSize, Error as CodecError, FixedSize, Read, ReadExt, Write};
use commonware_cryptography::ed25519;

const MSG_TYPE_DEALER: u8 = 0;
const MSG_TYPE_ACK: u8 = 1;

#[derive(Debug, Clone)]
pub enum DkgMessage {
    Dealer { round: u64, public_commitment: Vec<u8>, private_share: Vec<u8> },
    Ack { round: u64, dealer: ed25519::PublicKey, signature: Vec<u8> },
}

impl Write for DkgMessage {
    fn write(&self, writer: &mut impl BufMut) {
        match self {
            Self::Dealer { round, public_commitment, private_share } => {
                MSG_TYPE_DEALER.write(writer);
                round.write(writer);
                (public_commitment.len() as u32).write(writer);
                writer.put_slice(public_commitment);
                (private_share.len() as u32).write(writer);
                writer.put_slice(private_share);
            }
            Self::Ack { round, dealer, signature } => {
                MSG_TYPE_ACK.write(writer);
                round.write(writer);
                dealer.write(writer);
                (signature.len() as u32).write(writer);
                writer.put_slice(signature);
            }
        }
    }
}

impl EncodeSize for DkgMessage {
    fn encode_size(&self) -> usize {
        match self {
            Self::Dealer { public_commitment, private_share, .. } => {
                1 + 8 + 4 + public_commitment.len() + 4 + private_share.len()
            }
            Self::Ack { signature, .. } => {
                1 + 8 + <ed25519::PublicKey as FixedSize>::SIZE + 4 + signature.len()
            }
        }
    }
}

impl Read for DkgMessage {
    type Cfg = ();

    fn read_cfg(reader: &mut impl Buf, _cfg: &Self::Cfg) -> Result<Self, CodecError> {
        let msg_type = u8::read(reader)?;
        match msg_type {
            MSG_TYPE_DEALER => {
                let round = u64::read(reader)?;
                let commitment_len = u32::read(reader)? as usize;
                let mut public_commitment = vec![0u8; commitment_len];
                reader.copy_to_slice(&mut public_commitment);
                let share_len = u32::read(reader)? as usize;
                let mut private_share = vec![0u8; share_len];
                reader.copy_to_slice(&mut private_share);
                Ok(Self::Dealer { round, public_commitment, private_share })
            }
            MSG_TYPE_ACK => {
                let round = u64::read(reader)?;
                let dealer = ed25519::PublicKey::read(reader)?;
                let sig_len = u32::read(reader)? as usize;
                let mut signature = vec![0u8; sig_len];
                reader.copy_to_slice(&mut signature);
                Ok(Self::Ack { round, dealer, signature })
            }
            _ => Err(CodecError::Invalid("DkgMessage", "unknown message type")),
        }
    }
}
