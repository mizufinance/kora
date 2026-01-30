//! Interactive DKG ceremony runner.
//!
//! This module orchestrates the full DKG ceremony using the protocol and network modules.

use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use crate::{
    DkgConfig, DkgError, DkgOutput,
    network::DkgNetwork,
    protocol::{DkgParticipant, ProtocolMessage},
};

/// DKG ceremony runner.
pub struct DkgCeremony {
    config: DkgConfig,
}

impl DkgCeremony {
    /// Create a new DKG ceremony.
    pub const fn new(config: DkgConfig) -> Self {
        Self { config }
    }

    /// Run the interactive DKG ceremony.
    pub async fn run(&self) -> Result<DkgOutput, DkgError> {
        info!(
            validator_index = self.config.validator_index,
            n = self.config.n(),
            t = self.config.t(),
            is_leader = self.is_leader(),
            "Starting interactive DKG ceremony"
        );

        // Check if we already have output
        if DkgOutput::exists(&self.config.data_dir) {
            info!("DKG output already exists, loading from disk");
            return DkgOutput::load(&self.config.data_dir);
        }

        // Initialize network
        let network = DkgNetwork::new(self.config.clone())?;

        // Wait for peers to be ready
        self.wait_for_peers(&network).await?;

        // Initialize participant
        let mut participant = DkgParticipant::new(self.config.clone())?;

        // Phase 1: Start dealer - generate and broadcast commitments/shares
        info!("Phase 1: Starting dealer");
        participant.start_dealer()?;
        self.send_outgoing(&network, &mut participant)?;

        // Phase 2: Collect messages and send acks
        info!("Phase 2: Collecting messages and sending acks");
        let phase2_deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < phase2_deadline {
            self.receive_and_process(&network, &mut participant)?;
            self.send_outgoing(&network, &mut participant)?;

            // Check if we've received messages from all dealers
            if participant.dealer_log_count() >= 1 {
                // We have at least processed some, but keep collecting
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Phase 3: Finalize our dealer
        info!("Phase 3: Finalizing dealer");
        participant.finalize_dealer()?;
        self.send_outgoing(&network, &mut participant)?;

        // Phase 4: Collect dealer logs
        info!("Phase 4: Collecting dealer logs");
        let phase4_deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < phase4_deadline {
            self.receive_and_process(&network, &mut participant)?;
            self.send_outgoing(&network, &mut participant)?;

            if participant.can_finalize() {
                info!(
                    logs = participant.dealer_log_count(),
                    required = participant.required_quorum(),
                    "Collected enough dealer logs"
                );
                break;
            }

            // If we're not the leader, request logs from leader
            if !self.is_leader()
                && participant.dealer_log_count() < participant.required_quorum()
                && let Some(leader_pk) = self.config.participants.first()
            {
                debug!("Requesting logs from leader");
                let _ = network.send_to(leader_pk, &ProtocolMessage::RequestLogs);
            }

            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        // Phase 5: Finalize and produce output
        info!("Phase 5: Finalizing DKG");
        let output = participant.finalize()?;

        // Save output
        output.save(&self.config.data_dir)?;
        info!(
            share_index = output.share_index,
            path = ?self.config.data_dir,
            "DKG ceremony completed and saved"
        );

        Ok(output)
    }

    /// Check if this node is the leader (coordinator).
    const fn is_leader(&self) -> bool {
        self.config.validator_index == 0
    }

    /// Wait for peers to be reachable.
    async fn wait_for_peers(&self, network: &DkgNetwork) -> Result<(), DkgError> {
        info!("Waiting for peers to be ready...");

        let start = Instant::now();
        let timeout = Duration::from_secs(120);

        // Give leader time to start first
        if !self.is_leader() {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        // For non-bootstrap nodes, wait a bit for bootstrap to be ready
        if !self.config.bootstrap_peers.is_empty() {
            let mut connected = false;
            while start.elapsed() < timeout {
                // Try to connect to first bootstrap peer
                if let Some((pk, _)) = self.config.bootstrap_peers.first() {
                    let ping = ProtocolMessage::RequestLogs; // Use as ping
                    if network.send_to(pk, &ping).is_ok() {
                        connected = true;
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            if !connected {
                warn!("Could not connect to bootstrap peer, continuing anyway");
            }
        }

        info!(elapsed = ?start.elapsed(), "Peer initialization complete");
        Ok(())
    }

    /// Send all outgoing messages.
    fn send_outgoing(
        &self,
        network: &DkgNetwork,
        participant: &mut DkgParticipant,
    ) -> Result<(), DkgError> {
        for (target, msg) in participant.take_outgoing() {
            match target {
                Some(pk) => {
                    if let Err(e) = network.send_to(&pk, &msg) {
                        debug!(?pk, ?e, "Failed to send to peer");
                    }
                }
                None => {
                    if let Err(e) = network.broadcast(&msg) {
                        debug!(?e, "Failed to broadcast");
                    }
                }
            }
        }
        Ok(())
    }

    /// Receive and process incoming messages.
    fn receive_and_process(
        &self,
        network: &DkgNetwork,
        participant: &mut DkgParticipant,
    ) -> Result<(), DkgError> {
        for envelope in network.poll_incoming() {
            match ProtocolMessage::from_bytes(&envelope.payload, network.max_degree()) {
                Ok(msg) => {
                    if let Err(e) = participant.handle_message(&envelope.from, msg) {
                        warn!(?e, "Failed to handle message");
                    }
                }
                Err(e) => {
                    warn!(?e, "Failed to decode message");
                }
            }
        }
        Ok(())
    }
}
