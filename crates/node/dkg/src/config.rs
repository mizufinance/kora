use std::{path::PathBuf, time::Duration};

use commonware_cryptography::ed25519;

#[derive(Debug, Clone)]
pub struct DkgConfig {
    pub identity_key: ed25519::PrivateKey,
    pub validator_index: usize,
    pub participants: Vec<ed25519::PublicKey>,
    pub threshold: u32,
    pub chain_id: u64,
    pub data_dir: PathBuf,
    pub listen_addr: std::net::SocketAddr,
    pub bootstrap_peers: Vec<(ed25519::PublicKey, String)>,
    pub timeout: Duration,
}

impl DkgConfig {
    pub const fn n(&self) -> usize {
        self.participants.len()
    }

    pub const fn t(&self) -> u32 {
        self.threshold
    }

    pub fn my_public_key(&self) -> ed25519::PublicKey {
        use commonware_cryptography::Signer;
        self.identity_key.public_key()
    }
}
