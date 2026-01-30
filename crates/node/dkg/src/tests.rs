//! Integration tests for the interactive DKG protocol.

use std::{path::PathBuf, time::Duration};

use commonware_cryptography::{Signer as _, ed25519};

use crate::{DkgConfig, DkgParticipant, ProtocolMessage};

fn generate_test_keys(n: usize, seed: u64) -> Vec<ed25519::PrivateKey> {
    (0..n).map(|i| ed25519::PrivateKey::from_seed(seed.wrapping_add(i as u64))).collect()
}

fn make_test_config(keys: &[ed25519::PrivateKey], index: usize, base_port: u16) -> DkgConfig {
    let participants: Vec<_> = keys.iter().map(|k| k.public_key()).collect();
    let n = participants.len();
    let f = (n - 1) / 3;
    let threshold = (n - f) as u32;

    let bootstrap_peers: Vec<_> = participants
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(i, pk)| (pk.clone(), format!("127.0.0.1:{}", base_port + i as u16)))
        .collect();

    DkgConfig {
        identity_key: keys[index].clone(),
        validator_index: index,
        participants,
        threshold,
        chain_id: 1337,
        data_dir: PathBuf::from(format!("/tmp/dkg-test-{}", index)),
        listen_addr: format!("127.0.0.1:{}", base_port + index as u16).parse().unwrap(),
        bootstrap_peers,
        timeout: Duration::from_secs(60),
    }
}

#[test]
fn test_participant_creation() {
    let keys = generate_test_keys(4, 42);
    let config = make_test_config(&keys, 0, 40000);

    let participant = DkgParticipant::new(config).expect("should create participant");

    assert_eq!(participant.dealer_log_count(), 0);
    assert_eq!(participant.required_quorum(), 3); // n=4, f=1, quorum=3
    assert!(!participant.can_finalize());
}

#[test]
fn test_dealer_start_generates_messages() {
    let keys = generate_test_keys(4, 42);
    let config = make_test_config(&keys, 0, 40100);

    let mut participant = DkgParticipant::new(config).expect("should create participant");
    participant.start_dealer().expect("should start dealer");

    let outgoing = participant.take_outgoing();

    // Should have 1 broadcast (DealerPublic) + 4 private messages (one per player including self)
    assert!(!outgoing.is_empty());

    let broadcasts: Vec<_> = outgoing.iter().filter(|(target, _)| target.is_none()).collect();
    let directs: Vec<_> = outgoing.iter().filter(|(target, _)| target.is_some()).collect();

    assert_eq!(broadcasts.len(), 1, "should have 1 broadcast message");
    assert_eq!(directs.len(), 4, "should have 4 direct messages (one per player)");

    // Verify message types
    for (_, msg) in &broadcasts {
        assert!(matches!(msg, ProtocolMessage::DealerPublic { .. }));
    }
    for (_, msg) in &directs {
        assert!(matches!(msg, ProtocolMessage::DealerPrivate { .. }));
    }
}

#[test]
fn test_protocol_message_serialization() {
    let keys = generate_test_keys(4, 42);
    let config = make_test_config(&keys, 0, 40200);

    let mut participant = DkgParticipant::new(config).expect("should create participant");
    participant.start_dealer().expect("should start dealer");

    let outgoing = participant.take_outgoing();

    for (_, msg) in &outgoing {
        let bytes = msg.to_bytes();
        assert!(!bytes.is_empty());

        // Deserialize back
        let max_degree = 3; // For n=4
        let decoded = ProtocolMessage::from_bytes(&bytes, max_degree);
        assert!(decoded.is_ok(), "should deserialize: {:?}", decoded.err());
    }
}

#[test]
fn test_local_dkg_simulation() {
    // Simulate a complete DKG ceremony locally without networking
    let keys = generate_test_keys(4, 42);
    let n = keys.len();

    // Create all participants
    let mut participants: Vec<_> = (0..n)
        .map(|i| {
            let config = make_test_config(&keys, i, 40300 + (i as u16) * 100);
            DkgParticipant::new(config).expect("should create participant")
        })
        .collect();

    // Phase 1: All dealers start and generate messages
    let mut all_messages: Vec<(usize, Option<ed25519::PublicKey>, ProtocolMessage)> = Vec::new();

    for (i, p) in participants.iter_mut().enumerate() {
        p.start_dealer().expect("should start dealer");
        for (target, msg) in p.take_outgoing() {
            all_messages.push((i, target, msg));
        }
    }

    // Phase 2: Deliver all messages to appropriate recipients
    for (from_idx, target, msg) in all_messages {
        let from_pk = keys[from_idx].public_key();

        match target {
            None => {
                // Broadcast to all
                for p in participants.iter_mut() {
                    let _ = p.handle_message(&from_pk, msg.clone());
                }
            }
            Some(ref to_pk) => {
                // Find recipient and deliver
                for (i, p) in participants.iter_mut().enumerate() {
                    if &keys[i].public_key() == to_pk {
                        let _ = p.handle_message(&from_pk, msg.clone());
                        break;
                    }
                }
            }
        }
    }

    // Collect acks generated
    let mut ack_messages: Vec<(usize, Option<ed25519::PublicKey>, ProtocolMessage)> = Vec::new();
    for (i, p) in participants.iter_mut().enumerate() {
        for (target, msg) in p.take_outgoing() {
            ack_messages.push((i, target, msg));
        }
    }

    // Deliver acks
    for (from_idx, target, msg) in ack_messages {
        let from_pk = keys[from_idx].public_key();
        if let Some(ref to_pk) = target {
            for (i, p) in participants.iter_mut().enumerate() {
                if &keys[i].public_key() == to_pk {
                    let _ = p.handle_message(&from_pk, msg.clone());
                    break;
                }
            }
        }
    }

    // Phase 3: Finalize dealers
    let mut dealer_logs: Vec<(usize, Option<ed25519::PublicKey>, ProtocolMessage)> = Vec::new();
    for (i, p) in participants.iter_mut().enumerate() {
        p.finalize_dealer().expect("should finalize dealer");
        for (target, msg) in p.take_outgoing() {
            dealer_logs.push((i, target, msg));
        }
    }

    // Deliver dealer logs to all
    for (from_idx, _, msg) in dealer_logs {
        let from_pk = keys[from_idx].public_key();
        for p in participants.iter_mut() {
            let _ = p.handle_message(&from_pk, msg.clone());
        }
    }

    // Phase 4: Verify all can finalize
    for (i, p) in participants.iter().enumerate() {
        assert!(
            p.can_finalize(),
            "participant {} should be able to finalize (has {} logs, needs {})",
            i,
            p.dealer_log_count(),
            p.required_quorum()
        );
    }

    // Finalize and verify all get the same group key
    let mut outputs = Vec::new();
    for p in participants.iter_mut() {
        let output = p.finalize().expect("should finalize");
        outputs.push(output);
    }

    // All participants should have the same group public key
    let first_group_key = &outputs[0].group_public_key;
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(
            &output.group_public_key, first_group_key,
            "participant {} has different group key",
            i
        );
    }

    // Each participant should have a unique share index
    let share_indices: Vec<_> = outputs.iter().map(|o| o.share_index).collect();
    let unique_indices: std::collections::HashSet<_> = share_indices.iter().collect();
    assert_eq!(unique_indices.len(), n, "all participants should have unique share indices");
}

#[test]
fn test_quorum_calculation() {
    // Test various validator counts
    let test_cases = [
        (4, 3),  // n=4: f=1, quorum=3
        (7, 5),  // n=7: f=2, quorum=5
        (10, 7), // n=10: f=3, quorum=7
        (13, 9), // n=13: f=4, quorum=9
    ];

    for (n, expected_quorum) in test_cases {
        let keys = generate_test_keys(n, 42);
        let config = make_test_config(&keys, 0, 50000);
        let participant = DkgParticipant::new(config).expect("should create participant");

        assert_eq!(
            participant.required_quorum(),
            expected_quorum,
            "n={} should require quorum={}",
            n,
            expected_quorum
        );
    }
}
