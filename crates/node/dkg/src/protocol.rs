//! Interactive DKG protocol implementation using commonware's Joint-Feldman DKG.
//!
//! This module implements the full interactive DKG protocol where each participant
//! acts as both a dealer (generating shares for others) and a player (receiving shares).

use std::collections::BTreeMap;

use commonware_codec::{Read as CodecRead, ReadExt, Write};
use commonware_cryptography::{
    bls12381::{
        dkg::{
            Dealer, DealerLog, DealerPrivMsg, DealerPubMsg, Info, Player, PlayerAck,
            SignedDealerLog,
        },
        primitives::{sharing::Mode, variant::MinSig},
    },
    ed25519,
};
use commonware_parallel::Sequential;
use commonware_utils::{Faults, N3f1, TryCollect, ordered::Set};
use tracing::{debug, info, warn};

use crate::{DkgConfig, DkgError, DkgOutput};

/// Message types for the DKG protocol.
#[derive(Debug, Clone)]
pub enum ProtocolMessage {
    /// Public commitment from a dealer to all players.
    DealerPublic { dealer: ed25519::PublicKey, msg: DealerPubMsg<MinSig> },
    /// Private share from a dealer to a specific player.
    DealerPrivate { dealer: ed25519::PublicKey, msg: DealerPrivMsg },
    /// Acknowledgement from a player to a dealer.
    PlayerAck {
        player: ed25519::PublicKey,
        dealer: ed25519::PublicKey,
        ack: PlayerAck<ed25519::PublicKey>,
    },
    /// Signed dealer log for finalization.
    DealerLog { log: SignedDealerLog<MinSig, ed25519::PrivateKey> },
    /// Request for all dealer logs (sent by non-leaders to leader).
    RequestLogs,
    /// All collected dealer logs (sent by leader to all).
    AllLogs { logs: Vec<(ed25519::PublicKey, SignedDealerLog<MinSig, ed25519::PrivateKey>)> },
}

impl ProtocolMessage {
    /// Serialize the message to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self {
            Self::DealerPublic { dealer, msg } => {
                buf.push(0u8);
                dealer.write(&mut buf);
                msg.write(&mut buf);
            }
            Self::DealerPrivate { dealer, msg } => {
                buf.push(1u8);
                dealer.write(&mut buf);
                msg.write(&mut buf);
            }
            Self::PlayerAck { player, dealer, ack } => {
                buf.push(2u8);
                player.write(&mut buf);
                dealer.write(&mut buf);
                ack.write(&mut buf);
            }
            Self::DealerLog { log } => {
                buf.push(3u8);
                log.write(&mut buf);
            }
            Self::RequestLogs => {
                buf.push(4u8);
            }
            Self::AllLogs { logs } => {
                buf.push(5u8);
                (logs.len() as u32).write(&mut buf);
                for (pk, log) in logs {
                    pk.write(&mut buf);
                    log.write(&mut buf);
                }
            }
        }
        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8], max_degree: u32) -> Result<Self, commonware_codec::Error> {
        let mut reader = bytes;

        let tag = u8::read(&mut reader)?;
        match tag {
            0 => {
                let dealer = ed25519::PublicKey::read(&mut reader)?;
                let msg = DealerPubMsg::<MinSig>::read_cfg(
                    &mut reader,
                    &core::num::NonZeroU32::new(max_degree).unwrap(),
                )?;
                Ok(Self::DealerPublic { dealer, msg })
            }
            1 => {
                let dealer = ed25519::PublicKey::read(&mut reader)?;
                let msg = DealerPrivMsg::read(&mut reader)?;
                Ok(Self::DealerPrivate { dealer, msg })
            }
            2 => {
                let player = ed25519::PublicKey::read(&mut reader)?;
                let dealer = ed25519::PublicKey::read(&mut reader)?;
                let ack = PlayerAck::<ed25519::PublicKey>::read(&mut reader)?;
                Ok(Self::PlayerAck { player, dealer, ack })
            }
            3 => {
                let log = SignedDealerLog::<MinSig, ed25519::PrivateKey>::read_cfg(
                    &mut reader,
                    &core::num::NonZeroU32::new(max_degree).unwrap(),
                )?;
                Ok(Self::DealerLog { log })
            }
            4 => Ok(Self::RequestLogs),
            5 => {
                let count = u32::read(&mut reader)? as usize;
                let mut logs = Vec::with_capacity(count);
                for _ in 0..count {
                    let pk = ed25519::PublicKey::read(&mut reader)?;
                    let log = SignedDealerLog::<MinSig, ed25519::PrivateKey>::read_cfg(
                        &mut reader,
                        &core::num::NonZeroU32::new(max_degree).unwrap(),
                    )?;
                    logs.push((pk, log));
                }
                Ok(Self::AllLogs { logs })
            }
            _ => Err(commonware_codec::Error::InvalidEnum(tag)),
        }
    }
}

/// State of a participant in the DKG protocol.
pub struct DkgParticipant {
    config: DkgConfig,
    info: Info<MinSig, ed25519::PublicKey>,
    player: Option<Player<MinSig, ed25519::PrivateKey>>,
    dealer: Option<Dealer<MinSig, ed25519::PrivateKey>>,

    /// Messages to send (accumulated during protocol execution).
    outgoing: Vec<(Option<ed25519::PublicKey>, ProtocolMessage)>,

    /// Received dealer public messages.
    dealer_pub_msgs: BTreeMap<ed25519::PublicKey, DealerPubMsg<MinSig>>,
    /// Received dealer private messages.
    dealer_priv_msgs: BTreeMap<ed25519::PublicKey, DealerPrivMsg>,

    /// Signed dealer logs we've collected.
    dealer_logs: BTreeMap<ed25519::PublicKey, DealerLog<MinSig, ed25519::PublicKey>>,
    /// Signed logs (for sending to leader).
    signed_logs: BTreeMap<ed25519::PublicKey, SignedDealerLog<MinSig, ed25519::PrivateKey>>,

    /// Our own signed log.
    our_signed_log: Option<SignedDealerLog<MinSig, ed25519::PrivateKey>>,

    /// Whether we've finalized.
    finalized: bool,
}

impl DkgParticipant {
    /// Create a new DKG participant.
    pub fn new(config: DkgConfig) -> Result<Self, DkgError> {
        let participants_set: Set<ed25519::PublicKey> = config
            .participants
            .iter()
            .cloned()
            .try_collect()
            .map_err(|_| DkgError::CeremonyFailed("duplicate participants".into()))?;

        // Create round info - all participants are both dealers and players
        let info = Info::<MinSig, ed25519::PublicKey>::new::<N3f1>(
            format!("kora-dkg-{}", config.chain_id).as_bytes(),
            0,    // round 0 for initial DKG
            None, // no previous output
            Mode::default(),
            participants_set.clone(), // dealers
            participants_set,         // players
        )
        .map_err(|e| DkgError::Crypto(format!("Failed to create DKG info: {:?}", e)))?;

        // Create our player instance
        let player =
            Player::<MinSig, ed25519::PrivateKey>::new(info.clone(), config.identity_key.clone())
                .map_err(|e| DkgError::Crypto(format!("Failed to create player: {:?}", e)))?;

        Ok(Self {
            config,
            info,
            player: Some(player),
            dealer: None,
            outgoing: Vec::new(),
            dealer_pub_msgs: BTreeMap::new(),
            dealer_priv_msgs: BTreeMap::new(),
            dealer_logs: BTreeMap::new(),
            signed_logs: BTreeMap::new(),
            our_signed_log: None,
            finalized: false,
        })
    }

    /// Start the dealer phase - generate and return messages to send.
    pub fn start_dealer(&mut self) -> Result<(), DkgError> {
        let mut rng = rand::rngs::OsRng;

        let (dealer, pub_msg, priv_msgs) = Dealer::<MinSig, ed25519::PrivateKey>::start::<N3f1>(
            &mut rng,
            self.info.clone(),
            self.config.identity_key.clone(),
            None, // no previous share for initial DKG
        )
        .map_err(|e| DkgError::Crypto(format!("Failed to start dealer: {:?}", e)))?;

        let my_pk = self.config.my_public_key();

        info!(
            validator_index = self.config.validator_index,
            "Generated dealer messages for {} players",
            priv_msgs.len()
        );

        // Queue public message for broadcast
        self.outgoing.push((
            None, // broadcast
            ProtocolMessage::DealerPublic { dealer: my_pk.clone(), msg: pub_msg.clone() },
        ));

        // Queue private messages for each player
        for (player_pk, priv_msg) in priv_msgs {
            self.outgoing.push((
                Some(player_pk.clone()),
                ProtocolMessage::DealerPrivate { dealer: my_pk.clone(), msg: priv_msg },
            ));
        }

        // Store our own public message so we can process it
        self.dealer_pub_msgs.insert(my_pk, pub_msg);
        self.dealer = Some(dealer);

        Ok(())
    }

    /// Process an incoming message.
    pub fn handle_message(
        &mut self,
        from: &ed25519::PublicKey,
        msg: ProtocolMessage,
    ) -> Result<(), DkgError> {
        match msg {
            ProtocolMessage::DealerPublic { dealer, msg } => {
                debug!(?dealer, "Received dealer public message");
                self.dealer_pub_msgs.insert(dealer.clone(), msg);
                self.try_process_dealer_messages(&dealer)?;
            }
            ProtocolMessage::DealerPrivate { dealer, msg } => {
                debug!(?dealer, "Received dealer private message");
                self.dealer_priv_msgs.insert(dealer.clone(), msg);
                self.try_process_dealer_messages(&dealer)?;
            }
            ProtocolMessage::PlayerAck { player, dealer, ack } => {
                if let Some(ref mut our_dealer) = self.dealer
                    && dealer == self.config.my_public_key()
                {
                    debug!(?player, "Received player ack");
                    if let Err(e) = our_dealer.receive_player_ack(player, ack) {
                        warn!(?e, "Failed to process player ack");
                    }
                }
            }
            ProtocolMessage::DealerLog { log } => {
                let log_clone = log.clone();
                if let Some((dealer_pk, dealer_log)) = log.check(&self.info) {
                    debug!(?dealer_pk, "Received valid dealer log");
                    self.dealer_logs.insert(dealer_pk.clone(), dealer_log);
                    self.signed_logs.insert(dealer_pk, log_clone);
                } else {
                    warn!("Received invalid dealer log");
                }
            }
            ProtocolMessage::RequestLogs => {
                // Leader should respond with all logs
                let logs: Vec<_> =
                    self.signed_logs.iter().map(|(pk, log)| (pk.clone(), log.clone())).collect();
                self.outgoing.push((Some(from.clone()), ProtocolMessage::AllLogs { logs }));
            }
            ProtocolMessage::AllLogs { logs } => {
                info!(count = logs.len(), "Received all dealer logs from leader");
                for (pk, log) in logs {
                    let log_clone = log.clone();
                    if let Some((dealer_pk, dealer_log)) = log.check(&self.info) {
                        self.dealer_logs.insert(dealer_pk, dealer_log);
                        self.signed_logs.insert(pk, log_clone);
                    }
                }
            }
        }
        Ok(())
    }

    /// Try to process dealer messages if we have both pub and priv.
    fn try_process_dealer_messages(&mut self, dealer: &ed25519::PublicKey) -> Result<(), DkgError> {
        let pub_msg = match self.dealer_pub_msgs.get(dealer) {
            Some(m) => m.clone(),
            None => return Ok(()),
        };
        let priv_msg = match self.dealer_priv_msgs.get(dealer) {
            Some(m) => m.clone(),
            None => return Ok(()),
        };

        // Process the dealer message and potentially generate an ack
        if let Some(ref mut player) = self.player {
            if let Some(ack) = player.dealer_message::<N3f1>(dealer.clone(), pub_msg, priv_msg) {
                debug!(?dealer, "Sending ack to dealer");
                self.outgoing.push((
                    Some(dealer.clone()),
                    ProtocolMessage::PlayerAck {
                        player: self.config.my_public_key(),
                        dealer: dealer.clone(),
                        ack,
                    },
                ));
            } else {
                warn!(?dealer, "Failed to verify dealer message");
            }
        }

        Ok(())
    }

    /// Finalize our dealer and create signed log.
    pub fn finalize_dealer(&mut self) -> Result<(), DkgError> {
        if let Some(dealer) = self.dealer.take() {
            let signed_log = dealer.finalize::<N3f1>();
            let signed_log_clone = signed_log.clone();

            // Verify our own log
            if let Some((dealer_pk, dealer_log)) = signed_log.check(&self.info) {
                info!(?dealer_pk, "Created valid dealer log");
                self.dealer_logs.insert(dealer_pk.clone(), dealer_log);
                self.signed_logs.insert(dealer_pk, signed_log_clone.clone());
                self.our_signed_log = Some(signed_log_clone.clone());

                // Send to all participants (leader will collect)
                self.outgoing.push((
                    None, // broadcast
                    ProtocolMessage::DealerLog { log: signed_log_clone },
                ));
            } else {
                return Err(DkgError::CeremonyFailed("Our own dealer log is invalid".into()));
            }
        }
        Ok(())
    }

    /// Check if we have enough dealer logs to finalize.
    pub fn can_finalize(&self) -> bool {
        let required = N3f1::quorum(self.config.participants.len()) as usize;
        self.dealer_logs.len() >= required
    }

    /// Finalize the DKG and produce output.
    pub fn finalize(&mut self) -> Result<DkgOutput, DkgError> {
        if self.finalized {
            return Err(DkgError::CeremonyFailed("Already finalized".into()));
        }

        if !self.can_finalize() {
            return Err(DkgError::CeremonyFailed(format!(
                "Not enough dealer logs: {} < {}",
                self.dealer_logs.len(),
                N3f1::quorum(self.config.participants.len())
            )));
        }

        info!(logs = self.dealer_logs.len(), "Finalizing DKG with collected logs");

        let player = self
            .player
            .take()
            .ok_or_else(|| DkgError::CeremonyFailed("Player already consumed".into()))?;

        let (output, share) = player
            .finalize::<N3f1>(self.dealer_logs.clone(), &Sequential)
            .map_err(|e| DkgError::Crypto(format!("Failed to finalize: {:?}", e)))?;

        self.finalized = true;

        // Serialize outputs
        let mut group_key_bytes = Vec::new();
        output.public().public().write(&mut group_key_bytes);

        let mut polynomial_bytes = Vec::new();
        output.public().write(&mut polynomial_bytes);

        let mut share_bytes = Vec::new();
        share.write(&mut share_bytes);

        let participant_keys: Vec<Vec<u8>> = self
            .config
            .participants
            .iter()
            .map(|pk| {
                let mut bytes = Vec::new();
                pk.write(&mut bytes);
                bytes
            })
            .collect();

        Ok(DkgOutput {
            group_public_key: group_key_bytes,
            public_polynomial: polynomial_bytes,
            threshold: self.config.t(),
            participants: self.config.n(),
            share_index: usize::from(share.index) as u32,
            share_secret: share_bytes,
            participant_keys,
        })
    }

    /// Take outgoing messages.
    pub fn take_outgoing(&mut self) -> Vec<(Option<ed25519::PublicKey>, ProtocolMessage)> {
        std::mem::take(&mut self.outgoing)
    }

    /// Get our signed dealer log (for sending to leader).
    pub const fn our_signed_log(&self) -> Option<&SignedDealerLog<MinSig, ed25519::PrivateKey>> {
        self.our_signed_log.as_ref()
    }

    /// Get the number of collected dealer logs.
    pub fn dealer_log_count(&self) -> usize {
        self.dealer_logs.len()
    }

    /// Get required quorum.
    pub fn required_quorum(&self) -> usize {
        N3f1::quorum(self.config.participants.len()) as usize
    }
}
