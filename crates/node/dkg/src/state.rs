use std::collections::BTreeMap;

use commonware_cryptography::ed25519;

#[derive(Debug, Default)]
pub struct DkgState {
    pub round: u64,
    pub received_commitments: BTreeMap<ed25519::PublicKey, Vec<u8>>,
    pub received_shares: BTreeMap<ed25519::PublicKey, Vec<u8>>,
    pub sent_acks: BTreeMap<ed25519::PublicKey, bool>,
    pub received_acks: BTreeMap<ed25519::PublicKey, Vec<ed25519::PublicKey>>,
}

impl DkgState {
    pub fn new(round: u64) -> Self {
        Self { round, ..Default::default() }
    }

    pub fn record_dealer_message(
        &mut self,
        dealer: ed25519::PublicKey,
        commitment: Vec<u8>,
        share: Vec<u8>,
    ) {
        self.received_commitments.insert(dealer.clone(), commitment);
        self.received_shares.insert(dealer, share);
    }

    pub fn record_ack(&mut self, dealer: ed25519::PublicKey, from: ed25519::PublicKey) {
        self.received_acks.entry(dealer).or_default().push(from);
    }

    pub fn has_enough_acks(&self, dealer: &ed25519::PublicKey, threshold: usize) -> bool {
        self.received_acks.get(dealer).map(|acks| acks.len() >= threshold).unwrap_or(false)
    }

    pub fn all_dealers_acknowledged(
        &self,
        participants: &[ed25519::PublicKey],
        threshold: usize,
    ) -> bool {
        participants.iter().all(|p| self.has_enough_acks(p, threshold))
    }
}
