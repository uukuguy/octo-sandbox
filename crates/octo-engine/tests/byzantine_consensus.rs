//! Integration tests for PBFT-lite Byzantine consensus.

use octo_engine::agent::collaboration::{
    sign_consensus_vote, verify_consensus_vote, verify_signature, ByzantineProposal,
    CollaborationMessage, ConsensusKeypair, ConsensusPhase, ConsensusVote, PhaseAdvanceResult,
    SignedMessage, VerifyResult, ViewChangeReason, ViewChangeRequest, ViewChangeTracker, ViewState,
};

fn make_vote(agent_id: &str, approve: bool, phase: ConsensusPhase) -> ConsensusVote {
    ConsensusVote {
        agent_id: agent_id.to_string(),
        approve,
        phase,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

// ─── Quorum threshold tests ───

#[test]
fn quorum_threshold_n1() {
    let p = ByzantineProposal::new("p".into(), "l".into(), "a".into(), "d".into(), 1);
    assert_eq!(p.quorum_threshold(), 1); // f=0, 2*0+1=1
}

#[test]
fn quorum_threshold_n2() {
    let p = ByzantineProposal::new("p".into(), "l".into(), "a".into(), "d".into(), 2);
    assert_eq!(p.quorum_threshold(), 1); // f=0, 2*0+1=1
}

#[test]
fn quorum_threshold_n3() {
    let p = ByzantineProposal::new("p".into(), "l".into(), "a".into(), "d".into(), 3);
    assert_eq!(p.quorum_threshold(), 1); // f=0, 2*0+1=1
}

#[test]
fn quorum_threshold_n4() {
    let p = ByzantineProposal::new("p".into(), "l".into(), "a".into(), "d".into(), 4);
    assert_eq!(p.quorum_threshold(), 3); // f=1, 2*1+1=3
}

#[test]
fn quorum_threshold_n7() {
    let p = ByzantineProposal::new("p".into(), "l".into(), "a".into(), "d".into(), 7);
    assert_eq!(p.quorum_threshold(), 5); // f=2, 2*2+1=5
}

#[test]
fn quorum_threshold_n10() {
    let p = ByzantineProposal::new("p".into(), "l".into(), "a".into(), "d".into(), 10);
    assert_eq!(p.quorum_threshold(), 7); // f=3, 2*3+1=7
}

// ─── Phase transition tests ───

#[test]
fn full_phase_transition_happy_path() {
    // 4 agents: quorum = 3
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "deploy".into(),
        "Deploy v2".into(),
        4,
    );
    assert_eq!(p.phase, ConsensusPhase::PrePrepare);

    // Prepare votes: 3 approvals reach quorum
    let r1 = p.add_prepare_vote(make_vote("a1", true, ConsensusPhase::Prepare));
    assert_eq!(r1, PhaseAdvanceResult::QuorumNotReached);
    assert_eq!(p.phase, ConsensusPhase::Prepare);

    let r2 = p.add_prepare_vote(make_vote("a2", true, ConsensusPhase::Prepare));
    assert_eq!(r2, PhaseAdvanceResult::QuorumNotReached);

    let r3 = p.add_prepare_vote(make_vote("a3", true, ConsensusPhase::Prepare));
    assert_eq!(r3, PhaseAdvanceResult::Advanced(ConsensusPhase::Commit));
    assert_eq!(p.phase, ConsensusPhase::Commit);

    // Commit votes: 3 approvals reach quorum
    let r4 = p.add_commit_vote(make_vote("a1", true, ConsensusPhase::Commit));
    assert_eq!(r4, PhaseAdvanceResult::QuorumNotReached);

    let r5 = p.add_commit_vote(make_vote("a2", true, ConsensusPhase::Commit));
    assert_eq!(r5, PhaseAdvanceResult::QuorumNotReached);

    let r6 = p.add_commit_vote(make_vote("a3", true, ConsensusPhase::Commit));
    assert_eq!(r6, PhaseAdvanceResult::Advanced(ConsensusPhase::Finalized));
    assert_eq!(p.phase, ConsensusPhase::Finalized);
    assert!(p.is_finalized());
    assert!(p.finalized_at.is_some());
}

#[test]
fn single_agent_reaches_quorum_immediately() {
    // N=1: quorum=1, a single approve suffices
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "solo".into(),
        "act".into(),
        "desc".into(),
        1,
    );

    let r = p.add_prepare_vote(make_vote("solo", true, ConsensusPhase::Prepare));
    assert_eq!(r, PhaseAdvanceResult::Advanced(ConsensusPhase::Commit));

    let r = p.add_commit_vote(make_vote("solo", true, ConsensusPhase::Commit));
    assert_eq!(r, PhaseAdvanceResult::Advanced(ConsensusPhase::Finalized));
    assert!(p.is_finalized());
}

// ─── Insufficient votes / quorum not reached ───

#[test]
fn quorum_not_reached_with_insufficient_votes() {
    // 7 agents: quorum = 5
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        7,
    );

    // Only 4 approvals — not enough
    for i in 0..4 {
        let _ = p.add_prepare_vote(make_vote(&format!("a{}", i), true, ConsensusPhase::Prepare));
    }
    assert_eq!(p.phase, ConsensusPhase::Prepare);

    let r = p.try_advance();
    assert_eq!(r, PhaseAdvanceResult::QuorumNotReached);
}

// ─── Duplicate vote rejection ───

#[test]
fn duplicate_prepare_vote_rejected() {
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        4,
    );

    let r1 = p.add_prepare_vote(make_vote("a1", true, ConsensusPhase::Prepare));
    assert_eq!(r1, PhaseAdvanceResult::QuorumNotReached);

    let r2 = p.add_prepare_vote(make_vote("a1", true, ConsensusPhase::Prepare));
    match r2 {
        PhaseAdvanceResult::Failed(msg) => {
            assert!(msg.contains("already voted"));
        }
        other => panic!("Expected Failed, got {:?}", other),
    }

    // Only 1 vote should be recorded
    assert_eq!(p.prepare_votes.len(), 1);
}

#[test]
fn duplicate_commit_vote_rejected() {
    // Get to commit phase first (N=1 for simplicity)
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        4,
    );

    // Reach commit with 3 approvals
    for i in 0..3 {
        p.add_prepare_vote(make_vote(&format!("a{}", i), true, ConsensusPhase::Prepare));
    }
    assert_eq!(p.phase, ConsensusPhase::Commit);

    let r1 = p.add_commit_vote(make_vote("a1", true, ConsensusPhase::Commit));
    assert_eq!(r1, PhaseAdvanceResult::QuorumNotReached);

    let r2 = p.add_commit_vote(make_vote("a1", true, ConsensusPhase::Commit));
    match r2 {
        PhaseAdvanceResult::Failed(msg) => {
            assert!(msg.contains("already voted"));
        }
        other => panic!("Expected Failed, got {:?}", other),
    }

    assert_eq!(p.commit_votes.len(), 1);
}

// ─── Mixed votes ───

#[test]
fn mixed_votes_quorum_requires_approvals_only() {
    // 4 agents, quorum = 3
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        4,
    );

    // 2 approve, 1 reject — not enough approvals
    p.add_prepare_vote(make_vote("a1", true, ConsensusPhase::Prepare));
    p.add_prepare_vote(make_vote("a2", false, ConsensusPhase::Prepare));
    let r = p.add_prepare_vote(make_vote("a3", true, ConsensusPhase::Prepare));
    assert_eq!(r, PhaseAdvanceResult::QuorumNotReached);
    assert_eq!(p.phase, ConsensusPhase::Prepare);

    // Third approval reaches quorum
    let r = p.add_prepare_vote(make_vote("a4", true, ConsensusPhase::Prepare));
    assert_eq!(r, PhaseAdvanceResult::Advanced(ConsensusPhase::Commit));
}

// ─── Failed consensus from too many rejections ───

#[test]
fn too_many_rejections_cause_failure() {
    // 4 agents, quorum = 3, so max allowed rejections = 4 - 3 = 1
    // If 2 reject, it is impossible to reach quorum
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        4,
    );

    p.add_prepare_vote(make_vote("a1", false, ConsensusPhase::Prepare));
    let r = p.add_prepare_vote(make_vote("a2", false, ConsensusPhase::Prepare));
    assert_eq!(r, PhaseAdvanceResult::Advanced(ConsensusPhase::Failed));
    assert_eq!(p.phase, ConsensusPhase::Failed);
    assert!(p.is_finalized());
    assert!(p.finalized_at.is_some());
}

#[test]
fn commit_phase_failure_from_rejections() {
    // 4 agents, quorum = 3
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        4,
    );

    // Pass prepare
    for i in 0..3 {
        p.add_prepare_vote(make_vote(&format!("a{}", i), true, ConsensusPhase::Prepare));
    }
    assert_eq!(p.phase, ConsensusPhase::Commit);

    // 2 rejections in commit phase
    p.add_commit_vote(make_vote("a1", false, ConsensusPhase::Commit));
    let r = p.add_commit_vote(make_vote("a2", false, ConsensusPhase::Commit));
    assert_eq!(r, PhaseAdvanceResult::Advanced(ConsensusPhase::Failed));
    assert!(p.is_finalized());
}

// ─── Already finalized ───

#[test]
fn votes_on_finalized_proposal_return_already_finalized() {
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        1,
    );

    p.add_prepare_vote(make_vote("a1", true, ConsensusPhase::Prepare));
    p.add_commit_vote(make_vote("a1", true, ConsensusPhase::Commit));
    assert!(p.is_finalized());

    let r = p.add_prepare_vote(make_vote("a2", true, ConsensusPhase::Prepare));
    assert_eq!(r, PhaseAdvanceResult::AlreadyFinalized);

    let r = p.add_commit_vote(make_vote("a2", true, ConsensusPhase::Commit));
    assert_eq!(r, PhaseAdvanceResult::AlreadyFinalized);

    let r = p.try_advance();
    assert_eq!(r, PhaseAdvanceResult::AlreadyFinalized);
}

// ─── Wrong phase errors ───

#[test]
fn commit_vote_in_prepare_phase_fails() {
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        4,
    );

    // Still in PrePrepare, try commit vote
    let r = p.add_commit_vote(make_vote("a1", true, ConsensusPhase::Commit));
    match r {
        PhaseAdvanceResult::Failed(msg) => {
            assert!(msg.contains("Cannot add commit vote"));
        }
        other => panic!("Expected Failed, got {:?}", other),
    }
}

// ─── Serialization roundtrips ───

#[test]
fn consensus_phase_serialization_roundtrip() {
    for phase in &[
        ConsensusPhase::PrePrepare,
        ConsensusPhase::Prepare,
        ConsensusPhase::Commit,
        ConsensusPhase::Finalized,
        ConsensusPhase::Failed,
    ] {
        let json = serde_json::to_string(phase).unwrap();
        let decoded: ConsensusPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(&decoded, phase);
    }
}

#[test]
fn byzantine_proposal_full_serialization_roundtrip() {
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "deploy".into(),
        "Deploy v2".into(),
        4,
    );
    p.add_prepare_vote(make_vote("a1", true, ConsensusPhase::Prepare));
    p.add_prepare_vote(make_vote("a2", false, ConsensusPhase::Prepare));

    let json = serde_json::to_string(&p).unwrap();
    let decoded: ByzantineProposal = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.id, "p1");
    assert_eq!(decoded.proposer, "leader");
    assert_eq!(decoded.total_agents, 4);
    assert_eq!(decoded.prepare_votes.len(), 2);
    assert_eq!(decoded.phase, ConsensusPhase::Prepare);
}

// ─── ConsensusPhase equality ───

#[test]
fn consensus_phase_equality() {
    assert_eq!(ConsensusPhase::PrePrepare, ConsensusPhase::PrePrepare);
    assert_ne!(ConsensusPhase::PrePrepare, ConsensusPhase::Prepare);
    assert_ne!(ConsensusPhase::Commit, ConsensusPhase::Finalized);
    assert_ne!(ConsensusPhase::Finalized, ConsensusPhase::Failed);
}

// ─── has_voted helpers ───

#[test]
fn has_voted_prepare_and_commit() {
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        4,
    );

    assert!(!p.has_voted_prepare("a1"));
    assert!(!p.has_voted_commit("a1"));

    p.add_prepare_vote(make_vote("a1", true, ConsensusPhase::Prepare));
    assert!(p.has_voted_prepare("a1"));
    assert!(!p.has_voted_prepare("a2"));
    assert!(!p.has_voted_commit("a1"));
}

// ─── try_advance on PrePrepare ───

#[test]
fn try_advance_on_pre_prepare_returns_quorum_not_reached() {
    let mut p = ByzantineProposal::new(
        "p1".into(),
        "leader".into(),
        "act".into(),
        "desc".into(),
        4,
    );
    let r = p.try_advance();
    assert_eq!(r, PhaseAdvanceResult::QuorumNotReached);
}

// ─── Channel message variant tests ───

#[test]
fn consensus_proposal_message_serialization() {
    let msg = CollaborationMessage::ConsensusProposal {
        proposal_id: "p1".into(),
        action: "deploy".into(),
        description: "Deploy v2".into(),
        proposer: "leader".into(),
        total_agents: 4,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: CollaborationMessage = serde_json::from_str(&json).unwrap();
    match decoded {
        CollaborationMessage::ConsensusProposal {
            proposal_id,
            total_agents,
            ..
        } => {
            assert_eq!(proposal_id, "p1");
            assert_eq!(total_agents, 4);
        }
        other => panic!("Expected ConsensusProposal, got {:?}", other),
    }
}

#[test]
fn prepare_vote_message_serialization() {
    let msg = CollaborationMessage::PrepareVote {
        proposal_id: "p1".into(),
        agent_id: "a1".into(),
        approve: true,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: CollaborationMessage = serde_json::from_str(&json).unwrap();
    match decoded {
        CollaborationMessage::PrepareVote {
            proposal_id,
            agent_id,
            approve,
        } => {
            assert_eq!(proposal_id, "p1");
            assert_eq!(agent_id, "a1");
            assert!(approve);
        }
        other => panic!("Expected PrepareVote, got {:?}", other),
    }
}

#[test]
fn commit_vote_message_serialization() {
    let msg = CollaborationMessage::CommitVote {
        proposal_id: "p1".into(),
        agent_id: "a2".into(),
        approve: false,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: CollaborationMessage = serde_json::from_str(&json).unwrap();
    match decoded {
        CollaborationMessage::CommitVote {
            proposal_id,
            agent_id,
            approve,
        } => {
            assert_eq!(proposal_id, "p1");
            assert_eq!(agent_id, "a2");
            assert!(!approve);
        }
        other => panic!("Expected CommitVote, got {:?}", other),
    }
}

#[test]
fn consensus_result_message_serialization() {
    let msg = CollaborationMessage::ConsensusResult {
        proposal_id: "p1".into(),
        finalized: true,
        phase: "Finalized".into(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: CollaborationMessage = serde_json::from_str(&json).unwrap();
    match decoded {
        CollaborationMessage::ConsensusResult {
            proposal_id,
            finalized,
            phase,
        } => {
            assert_eq!(proposal_id, "p1");
            assert!(finalized);
            assert_eq!(phase, "Finalized");
        }
        other => panic!("Expected ConsensusResult, got {:?}", other),
    }
}

// ─── Integration with CollaborationContext ───

#[test]
fn byzantine_proposal_alongside_regular_proposals() {
    use octo_engine::agent::collaboration::{CollaborationContext, Proposal, ProposalStatus, Vote};

    let ctx = CollaborationContext::new("test-collab".to_string());

    // Add a regular proposal
    let regular = Proposal {
        id: "regular-1".into(),
        from_agent: "a1".into(),
        action: "refactor".into(),
        description: "Refactor module".into(),
        status: ProposalStatus::Pending,
        votes: vec![Vote {
            agent_id: "a2".into(),
            approve: true,
            reason: None,
        }],
    };
    ctx.add_proposal(regular);

    // Store a ByzantineProposal as JSON in shared state
    let mut bp = ByzantineProposal::new(
        "byz-1".into(),
        "leader".into(),
        "deploy".into(),
        "Deploy v2".into(),
        4,
    );
    bp.add_prepare_vote(make_vote("a1", true, ConsensusPhase::Prepare));

    let bp_json = serde_json::to_value(&bp).unwrap();
    ctx.set_state("byzantine:byz-1".into(), bp_json.clone());

    // Verify both coexist
    assert_eq!(ctx.proposals().len(), 1);
    assert_eq!(ctx.proposals()[0].id, "regular-1");

    let stored = ctx.get_state("byzantine:byz-1").unwrap();
    let restored: ByzantineProposal = serde_json::from_value(stored).unwrap();
    assert_eq!(restored.id, "byz-1");
    assert_eq!(restored.prepare_votes.len(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// D1-P2: Cryptographic Signing Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn crypto_keypair_generation_produces_unique_keys() {
    let kp1 = ConsensusKeypair::generate("agent-1".into());
    let kp2 = ConsensusKeypair::generate("agent-2".into());
    assert_ne!(kp1.public_key_bytes(), kp2.public_key_bytes());
    assert_eq!(kp1.agent_id, "agent-1");
    assert_eq!(kp2.agent_id, "agent-2");
    // ED25519 public keys are 32 bytes
    assert_eq!(kp1.public_key_bytes().len(), 32);
}

#[test]
fn crypto_sign_and_verify_happy_path() {
    let kp = ConsensusKeypair::generate("signer".into());
    let signed = kp.sign(b"hello consensus");
    assert_eq!(signed.agent_id, "signer");
    assert_eq!(signed.payload, b"hello consensus");
    assert_eq!(signed.signature.len(), 64);
    assert_eq!(signed.signer_public_key.len(), 32);
    assert_eq!(verify_signature(&signed), VerifyResult::Valid);
}

#[test]
fn crypto_wrong_key_fails_verification() {
    let kp1 = ConsensusKeypair::generate("a1".into());
    let kp2 = ConsensusKeypair::generate("a2".into());
    let mut signed = kp1.sign(b"important data");
    // Swap in a different public key
    signed.signer_public_key = kp2.public_key_bytes();
    assert_eq!(verify_signature(&signed), VerifyResult::InvalidSignature);
}

#[test]
fn crypto_tampered_payload_detected() {
    let kp = ConsensusKeypair::generate("agent".into());
    let mut signed = kp.sign(b"original message");
    signed.payload = b"tampered message".to_vec();
    assert_eq!(verify_signature(&signed), VerifyResult::InvalidSignature);
}

#[test]
fn crypto_sign_consensus_vote_roundtrip() {
    let kp = ConsensusKeypair::generate("voter-1".into());
    let signed = sign_consensus_vote(&kp, "proposal-42", "Prepare", true);
    assert_eq!(
        verify_consensus_vote(&signed, "proposal-42", "Prepare"),
        VerifyResult::Valid
    );
}

#[test]
fn crypto_wrong_proposal_id_in_verify() {
    let kp = ConsensusKeypair::generate("voter".into());
    let signed = sign_consensus_vote(&kp, "prop-A", "Commit", false);
    let result = verify_consensus_vote(&signed, "prop-B", "Commit");
    match result {
        VerifyResult::DeserializationError(msg) => {
            assert!(msg.contains("proposal_id mismatch"));
        }
        other => panic!("Expected DeserializationError, got {:?}", other),
    }
}

#[test]
fn crypto_wrong_phase_in_verify() {
    let kp = ConsensusKeypair::generate("voter".into());
    let signed = sign_consensus_vote(&kp, "prop-1", "Prepare", true);
    let result = verify_consensus_vote(&signed, "prop-1", "Commit");
    match result {
        VerifyResult::DeserializationError(msg) => {
            assert!(msg.contains("phase mismatch"));
        }
        other => panic!("Expected DeserializationError, got {:?}", other),
    }
}

#[test]
fn crypto_signed_message_serialization_roundtrip() {
    let kp = ConsensusKeypair::generate("serializer".into());
    let signed = kp.sign(b"serialize me");
    let json = serde_json::to_string(&signed).unwrap();
    let decoded: SignedMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.payload, signed.payload);
    assert_eq!(decoded.signature, signed.signature);
    assert_eq!(decoded.signer_public_key, signed.signer_public_key);
    assert_eq!(decoded.agent_id, "serializer");
    // Deserialized message should still verify
    assert_eq!(verify_signature(&decoded), VerifyResult::Valid);
}

#[test]
fn crypto_multiple_keypairs_only_correct_verifies() {
    let kp1 = ConsensusKeypair::generate("a1".into());
    let kp2 = ConsensusKeypair::generate("a2".into());
    let kp3 = ConsensusKeypair::generate("a3".into());

    let signed = kp1.sign(b"shared data");
    // Correct key verifies
    assert_eq!(verify_signature(&signed), VerifyResult::Valid);

    // Wrong keys fail
    let mut wrong2 = signed.clone();
    wrong2.signer_public_key = kp2.public_key_bytes();
    assert_eq!(verify_signature(&wrong2), VerifyResult::InvalidSignature);

    let mut wrong3 = signed.clone();
    wrong3.signer_public_key = kp3.public_key_bytes();
    assert_eq!(verify_signature(&wrong3), VerifyResult::InvalidSignature);
}

#[test]
fn crypto_malformed_public_key_returns_key_mismatch() {
    let kp = ConsensusKeypair::generate("agent".into());
    let mut signed = kp.sign(b"data");
    signed.signer_public_key = vec![0u8; 10]; // Wrong length
    assert_eq!(verify_signature(&signed), VerifyResult::KeyMismatch);
}

// ═══════════════════════════════════════════════════════════════════════════
// D1-P2: View Change Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn view_state_new_sets_correct_initial_state() {
    let agents = vec!["a1".into(), "a2".into(), "a3".into()];
    let vs = ViewState::new(agents.clone());
    assert_eq!(vs.view_number, 0);
    assert_eq!(vs.leader, "a1");
    assert_eq!(vs.agents, agents);
}

#[test]
fn view_state_leader_for_view_round_robin() {
    let agents: Vec<String> = vec!["a1".into(), "a2".into(), "a3".into()];
    assert_eq!(ViewState::leader_for_view(0, &agents), "a1");
    assert_eq!(ViewState::leader_for_view(1, &agents), "a2");
    assert_eq!(ViewState::leader_for_view(2, &agents), "a3");
    assert_eq!(ViewState::leader_for_view(3, &agents), "a1"); // wraps around
    assert_eq!(ViewState::leader_for_view(4, &agents), "a2");
}

#[test]
fn view_state_is_leader() {
    let vs = ViewState::new(vec!["leader".into(), "follower1".into(), "follower2".into()]);
    assert!(vs.is_leader("leader"));
    assert!(!vs.is_leader("follower1"));
    assert!(!vs.is_leader("follower2"));
    assert!(!vs.is_leader("unknown"));
}

#[test]
fn view_change_tracker_request_accumulation() {
    let agents = vec!["a1".into(), "a2".into(), "a3".into(), "a4".into()];
    let mut tracker = ViewChangeTracker::new(agents, 5000);
    assert_eq!(tracker.view_number(), 0);
    assert_eq!(tracker.current_leader(), "a1");

    // First request: not yet quorum (4 agents, quorum=3)
    let req1 = ViewChangeRequest {
        from_agent: "a2".into(),
        current_view: 0,
        proposed_view: 1,
        reason: ViewChangeReason::LeaderTimeout,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    assert!(!tracker.request_view_change(req1));
    assert_eq!(tracker.requests.len(), 1);
}

#[test]
fn view_change_quorum_triggers_change() {
    // 4 agents: quorum = 2*1+1 = 3
    let agents = vec!["a1".into(), "a2".into(), "a3".into(), "a4".into()];
    let mut tracker = ViewChangeTracker::new(agents, 5000);

    let make_req = |agent: &str| ViewChangeRequest {
        from_agent: agent.into(),
        current_view: 0,
        proposed_view: 1,
        reason: ViewChangeReason::LeaderTimeout,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    assert!(!tracker.request_view_change(make_req("a2")));
    assert!(!tracker.request_view_change(make_req("a3")));
    // Third request reaches quorum
    assert!(tracker.request_view_change(make_req("a4")));
}

#[test]
fn view_change_execute_advances_view_and_rotates_leader() {
    let agents = vec!["a1".into(), "a2".into(), "a3".into(), "a4".into()];
    let mut tracker = ViewChangeTracker::new(agents, 5000);

    assert_eq!(tracker.current_leader(), "a1");
    assert_eq!(tracker.view_number(), 0);

    let new_state = tracker.execute_view_change();
    assert_eq!(new_state.view_number, 1);
    assert_eq!(new_state.leader, "a2");
    assert_eq!(tracker.current_leader(), "a2");
    assert_eq!(tracker.view_number(), 1);
    // Requests should be cleared
    assert!(tracker.requests.is_empty());

    // Execute again
    let new_state = tracker.execute_view_change();
    assert_eq!(new_state.view_number, 2);
    assert_eq!(new_state.leader, "a3");
}

#[test]
fn view_change_duplicate_requests_from_same_agent_ignored() {
    let agents = vec!["a1".into(), "a2".into(), "a3".into(), "a4".into()];
    let mut tracker = ViewChangeTracker::new(agents, 5000);

    let make_req = |agent: &str| ViewChangeRequest {
        from_agent: agent.into(),
        current_view: 0,
        proposed_view: 1,
        reason: ViewChangeReason::LeaderTimeout,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    tracker.request_view_change(make_req("a2"));
    tracker.request_view_change(make_req("a2")); // duplicate
    tracker.request_view_change(make_req("a2")); // duplicate
    assert_eq!(tracker.requests.len(), 1); // Only counted once

    // Still needs 2 more unique agents for quorum
    assert!(!tracker.request_view_change(make_req("a3")));
    assert!(tracker.request_view_change(make_req("a4"))); // Now quorum
}

#[test]
fn view_change_reason_serialization() {
    for reason in &[
        ViewChangeReason::LeaderTimeout,
        ViewChangeReason::LeaderMalicious,
        ViewChangeReason::Explicit,
    ] {
        let json = serde_json::to_string(reason).unwrap();
        let decoded: ViewChangeReason = serde_json::from_str(&json).unwrap();
        assert_eq!(&decoded, reason);
    }
}

#[test]
fn view_state_serialization_roundtrip() {
    let vs = ViewState::new(vec!["a1".into(), "a2".into(), "a3".into()]);
    let json = serde_json::to_string(&vs).unwrap();
    let decoded: ViewState = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.view_number, 0);
    assert_eq!(decoded.leader, "a1");
    assert_eq!(decoded.agents.len(), 3);
}

// ─── New channel variant serialization tests ───

#[test]
fn signed_prepare_vote_message_serialization() {
    let msg = CollaborationMessage::SignedPrepareVote {
        proposal_id: "p1".into(),
        agent_id: "a1".into(),
        approve: true,
        signature: vec![1, 2, 3],
        public_key: vec![4, 5, 6],
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: CollaborationMessage = serde_json::from_str(&json).unwrap();
    match decoded {
        CollaborationMessage::SignedPrepareVote {
            proposal_id,
            agent_id,
            approve,
            signature,
            public_key,
        } => {
            assert_eq!(proposal_id, "p1");
            assert_eq!(agent_id, "a1");
            assert!(approve);
            assert_eq!(signature, vec![1, 2, 3]);
            assert_eq!(public_key, vec![4, 5, 6]);
        }
        other => panic!("Expected SignedPrepareVote, got {:?}", other),
    }
}

#[test]
fn view_change_request_message_serialization() {
    let msg = CollaborationMessage::ViewChangeRequest {
        from_agent: "a2".into(),
        current_view: 0,
        proposed_view: 1,
        reason: "LeaderTimeout".into(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: CollaborationMessage = serde_json::from_str(&json).unwrap();
    match decoded {
        CollaborationMessage::ViewChangeRequest {
            from_agent,
            current_view,
            proposed_view,
            reason,
        } => {
            assert_eq!(from_agent, "a2");
            assert_eq!(current_view, 0);
            assert_eq!(proposed_view, 1);
            assert_eq!(reason, "LeaderTimeout");
        }
        other => panic!("Expected ViewChangeRequest, got {:?}", other),
    }
}

#[test]
fn new_view_message_serialization() {
    let msg = CollaborationMessage::NewView {
        view_number: 3,
        new_leader: "a4".into(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: CollaborationMessage = serde_json::from_str(&json).unwrap();
    match decoded {
        CollaborationMessage::NewView {
            view_number,
            new_leader,
        } => {
            assert_eq!(view_number, 3);
            assert_eq!(new_leader, "a4");
        }
        other => panic!("Expected NewView, got {:?}", other),
    }
}
