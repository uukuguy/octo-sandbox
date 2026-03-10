//! Integration tests for the agent collaboration system (D6-10).
//!
//! Covers: CollaborationManager lifecycle, CollaborationProtocol propose/vote,
//! CollaborationChannel communication, shared state coordination,
//! InMemoryCollaborationStore persistence, DualAgentManager migration,
//! event logging, and multi-proposal workflows.

use std::collections::HashMap;
use std::sync::Arc;

use octo_engine::agent::{
    create_channel_pair, AgentCapability, AgentEvent, AgentExecutorHandle, AgentMessage,
    CollaborationContext, CollaborationEvent, CollaborationManager, CollaborationMessage,
    CollaborationProtocol, CollaborationStore, DualAgentManager, InMemoryCollaborationStore,
    ProposalStatus,
};
use octo_types::SessionId;
use tokio::sync::{broadcast, mpsc};

// ── Test helper ──

fn make_test_handle(name: &str) -> AgentExecutorHandle {
    let (tx, _rx) = mpsc::channel::<AgentMessage>(1);
    let (btx, _) = broadcast::channel::<AgentEvent>(1);
    AgentExecutorHandle {
        tx,
        broadcast_tx: btx,
        session_id: SessionId::from_string(name),
    }
}

// ── 1. Two-Agent Collaboration (regression for DualAgent) ──

#[test]
fn test_two_agent_collaboration_setup() {
    let sid = SessionId::from_string("dual-session");
    let mgr = CollaborationManager::dual_mode(
        "plan".to_string(),
        "Plan Agent".to_string(),
        make_test_handle("dual-session"),
        "build".to_string(),
        "Build Agent".to_string(),
        make_test_handle("dual-session"),
        sid,
    );

    // Verify 2 agents
    assert_eq!(mgr.agent_count(), 2);

    // Verify correct session_id
    assert_eq!(mgr.session_id().as_str(), "dual-session");

    // Verify agent IDs
    let ids = mgr.agent_ids();
    assert!(ids.contains(&"plan".to_string()));
    assert!(ids.contains(&"build".to_string()));

    // Default active agent is plan (first added in dual_mode)
    assert_eq!(mgr.active_agent_id(), "plan");

    // Verify active handle matches plan session
    let handle = mgr.active_handle().unwrap();
    assert_eq!(handle.session_id, SessionId::from_string("dual-session"));
}

#[test]
fn test_two_agent_switch_between_agents() {
    let sid = SessionId::from_string("switch-session");
    let mut mgr = CollaborationManager::dual_mode(
        "plan".to_string(),
        "Plan Agent".to_string(),
        make_test_handle("plan-handle"),
        "build".to_string(),
        "Build Agent".to_string(),
        make_test_handle("build-handle"),
        sid,
    );

    assert_eq!(mgr.active_agent_id(), "plan");
    assert_eq!(
        mgr.active_handle().unwrap().session_id,
        SessionId::from_string("plan-handle")
    );

    // Switch to build
    assert!(mgr.switch_to("build"));
    assert_eq!(mgr.active_agent_id(), "build");
    assert_eq!(
        mgr.active_handle().unwrap().session_id,
        SessionId::from_string("build-handle")
    );

    // Switch back to plan
    assert!(mgr.switch_to("plan"));
    assert_eq!(mgr.active_agent_id(), "plan");

    // Switch to nonexistent returns false, active unchanged
    assert!(!mgr.switch_to("nonexistent"));
    assert_eq!(mgr.active_agent_id(), "plan");
}

// ── 2. Three-Agent Collaboration ──

#[test]
fn test_three_agent_collaboration() {
    let sid = SessionId::from_string("three-agents");
    let mut mgr = CollaborationManager::new(sid);

    mgr.add_agent(
        "coder".into(),
        "Coder".into(),
        vec![AgentCapability::CodeGeneration],
        make_test_handle("coder-session"),
    );
    mgr.add_agent(
        "reviewer".into(),
        "Reviewer".into(),
        vec![AgentCapability::CodeReview],
        make_test_handle("reviewer-session"),
    );
    mgr.add_agent(
        "tester".into(),
        "Tester".into(),
        vec![AgentCapability::Testing],
        make_test_handle("tester-session"),
    );

    assert_eq!(mgr.agent_count(), 3);

    // Verify channels exist between all pairs (A-B, A-C, B-C) in both directions
    assert!(mgr.get_channel_mut("coder", "reviewer").is_some());
    assert!(mgr.get_channel_mut("reviewer", "coder").is_some());
    assert!(mgr.get_channel_mut("coder", "tester").is_some());
    assert!(mgr.get_channel_mut("tester", "coder").is_some());
    assert!(mgr.get_channel_mut("reviewer", "tester").is_some());
    assert!(mgr.get_channel_mut("tester", "reviewer").is_some());

    // Switch active agent
    assert!(mgr.switch_to("tester"));
    assert_eq!(mgr.active_agent_id(), "tester");

    // Remove one agent, verify cleanup
    let removed = mgr.remove_agent("reviewer");
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().id, "reviewer");
    assert_eq!(mgr.agent_count(), 2);

    // Channels involving reviewer should be gone
    assert!(mgr.get_channel_mut("coder", "reviewer").is_none());
    assert!(mgr.get_channel_mut("reviewer", "coder").is_none());
    assert!(mgr.get_channel_mut("reviewer", "tester").is_none());
    assert!(mgr.get_channel_mut("tester", "reviewer").is_none());

    // Remaining channels still exist
    assert!(mgr.get_channel_mut("coder", "tester").is_some());
    assert!(mgr.get_channel_mut("tester", "coder").is_some());
}

// ── 3. Protocol + Context Integration ──

#[test]
fn test_protocol_propose_and_vote() {
    let ctx = CollaborationContext::new("proto-test".to_string());

    // Propose an action
    let proposal_id = CollaborationProtocol::propose_action(
        &ctx,
        "agent-1",
        "refactor".to_string(),
        "Refactor the auth module".to_string(),
    );
    assert!(!proposal_id.is_empty());

    // Verify proposal exists and is pending
    let proposals = ctx.proposals();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].status, ProposalStatus::Pending);
    assert_eq!(proposals[0].from_agent, "agent-1");

    // Vote on the proposal
    assert!(CollaborationProtocol::vote(
        &ctx,
        &proposal_id,
        "agent-2",
        true,
        Some("Looks good".to_string()),
    ));
    assert!(CollaborationProtocol::vote(
        &ctx,
        &proposal_id,
        "agent-3",
        true,
        None,
    ));

    // Verify votes recorded
    let proposals = ctx.proposals();
    assert_eq!(proposals[0].votes.len(), 2);
    assert!(proposals[0].votes[0].approve);

    // Accept the proposal
    assert!(CollaborationProtocol::accept_proposal(&ctx, &proposal_id));
    let proposals = ctx.proposals();
    assert_eq!(proposals[0].status, ProposalStatus::Accepted);
}

// ── 4. Channel Communication ──

#[tokio::test]
async fn test_channel_delegation_flow() {
    let (ch_a_to_b, mut ch_b_to_a) = create_channel_pair("alice", "bob", 8);
    let ctx = CollaborationContext::new("channel-test".to_string());

    // Alice delegates a task to Bob via protocol
    let task_id = CollaborationProtocol::delegate_task(
        &ch_a_to_b,
        &ctx,
        "alice",
        "bob",
        "implement feature X".to_string(),
    )
    .await
    .unwrap();

    assert!(task_id.starts_with("task-"));

    // Bob receives the delegation message
    let msg = ch_b_to_a.recv().await.unwrap();
    match msg {
        CollaborationMessage::DelegateTask {
            task_id: tid,
            description,
        } => {
            assert_eq!(tid, task_id);
            assert_eq!(description, "implement feature X");
        }
        other => panic!("unexpected message: {:?}", other),
    }

    // Bob sends TaskResult back through his channel
    // Note: ch_b_to_a is the channel where bob receives from alice,
    // but to send back we need the reverse direction channel.
    // In create_channel_pair, (ch_a_to_b, ch_b_to_a):
    //   ch_a_to_b: alice sends, bob receives on ch_b_to_a
    // We need bob's send channel. Let's use a separate pair approach.
    let (ch_b_sends_to_a, _ch_a_receives) = create_channel_pair("bob", "alice", 8);
    ch_b_sends_to_a
        .send(CollaborationMessage::TaskResult {
            task_id: task_id.clone(),
            result: "feature X implemented".into(),
            success: true,
        })
        .await
        .unwrap();

    // Verify the event was logged by the protocol
    let events = ctx.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        CollaborationEvent::TaskDelegated { from, to, task } => {
            assert_eq!(from, "alice");
            assert_eq!(to, "bob");
            assert_eq!(task, "implement feature X");
        }
        other => panic!("unexpected event: {:?}", other),
    }
}

#[tokio::test]
async fn test_channel_bidirectional_communication() {
    let (mut ch_a, mut ch_b) = create_channel_pair("alice", "bob", 8);

    // Alice sends to Bob
    ch_a.send(CollaborationMessage::DelegateTask {
        task_id: "t1".into(),
        description: "do task".into(),
    })
    .await
    .unwrap();

    // Bob receives
    let msg = ch_b.recv().await.unwrap();
    match msg {
        CollaborationMessage::DelegateTask { task_id, .. } => assert_eq!(task_id, "t1"),
        other => panic!("unexpected: {:?}", other),
    }

    // Bob replies to Alice
    ch_b.send(CollaborationMessage::TaskResult {
        task_id: "t1".into(),
        result: "done".into(),
        success: true,
    })
    .await
    .unwrap();

    // Alice receives reply
    let reply = ch_a.recv().await.unwrap();
    match reply {
        CollaborationMessage::TaskResult {
            task_id, success, ..
        } => {
            assert_eq!(task_id, "t1");
            assert!(success);
        }
        other => panic!("unexpected: {:?}", other),
    }
}

// ── 5. Shared State Coordination ──

#[test]
fn test_shared_state_across_agents() {
    let ctx = Arc::new(CollaborationContext::new("shared-state-test".to_string()));

    // Agent 1 sets state via protocol
    CollaborationProtocol::share_state(
        &ctx,
        "agent-1",
        "progress".to_string(),
        serde_json::json!(50),
    );

    // Agent 2 reads the state (same context reference)
    let value = ctx.get_state("progress");
    assert_eq!(value, Some(serde_json::json!(50)));

    // Agent 2 updates the state
    CollaborationProtocol::share_state(
        &ctx,
        "agent-2",
        "progress".to_string(),
        serde_json::json!(100),
    );

    // Agent 1 reads the updated value
    let value = ctx.get_state("progress");
    assert_eq!(value, Some(serde_json::json!(100)));

    // Verify StateUpdated events logged for both updates
    let events = ctx.events();
    assert_eq!(events.len(), 2);
    match &events[0] {
        CollaborationEvent::StateUpdated { agent_id, key } => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(key, "progress");
        }
        other => panic!("unexpected event: {:?}", other),
    }
    match &events[1] {
        CollaborationEvent::StateUpdated { agent_id, key } => {
            assert_eq!(agent_id, "agent-2");
            assert_eq!(key, "progress");
        }
        other => panic!("unexpected event: {:?}", other),
    }
}

#[test]
fn test_shared_state_multiple_keys() {
    let ctx = CollaborationContext::new("multi-key-test".to_string());

    CollaborationProtocol::share_state(
        &ctx,
        "coder",
        "files_changed".to_string(),
        serde_json::json!(["src/main.rs", "src/lib.rs"]),
    );
    CollaborationProtocol::share_state(
        &ctx,
        "reviewer",
        "review_status".to_string(),
        serde_json::json!("in_progress"),
    );

    assert_eq!(
        ctx.get_state("files_changed"),
        Some(serde_json::json!(["src/main.rs", "src/lib.rs"]))
    );
    assert_eq!(
        ctx.get_state("review_status"),
        Some(serde_json::json!("in_progress"))
    );

    let mut keys = ctx.state_keys();
    keys.sort();
    assert_eq!(keys, vec!["files_changed", "review_status"]);
}

// ── 6. Persistence Roundtrip ──

#[tokio::test]
async fn test_persistence_save_and_restore() {
    let store = InMemoryCollaborationStore::new();
    let sid = SessionId::from_string("persist-session");
    let collab_id = "collab-persist";

    // Build up some state
    let mut shared_state = HashMap::new();
    shared_state.insert("progress".to_string(), serde_json::json!(75));
    shared_state.insert("phase".to_string(), serde_json::json!("building"));

    let events = vec![
        CollaborationEvent::AgentJoined {
            agent_id: "coder".to_string(),
            capabilities: vec![AgentCapability::CodeGeneration],
        },
        CollaborationEvent::AgentJoined {
            agent_id: "reviewer".to_string(),
            capabilities: vec![AgentCapability::CodeReview],
        },
        CollaborationEvent::StateUpdated {
            agent_id: "coder".to_string(),
            key: "progress".to_string(),
        },
    ];

    let proposals = vec![octo_engine::agent::Proposal {
        id: "p-1".to_string(),
        from_agent: "coder".to_string(),
        action: "refactor".to_string(),
        description: "Refactor auth module".to_string(),
        status: ProposalStatus::Pending,
        votes: vec![octo_engine::agent::Vote {
            agent_id: "reviewer".to_string(),
            approve: true,
            reason: Some("LGTM".to_string()),
        }],
    }];

    // Save
    store
        .save_collaboration(&sid, collab_id, &shared_state, &events, &proposals)
        .await
        .unwrap();

    // Load back
    let loaded = store
        .load_collaboration(&sid, collab_id)
        .await
        .unwrap();
    assert!(loaded.is_some());

    let snapshot = loaded.unwrap();
    assert_eq!(snapshot.collaboration_id, collab_id);
    assert_eq!(snapshot.shared_state.len(), 2);
    assert_eq!(
        snapshot.shared_state.get("progress"),
        Some(&serde_json::json!(75))
    );
    assert_eq!(snapshot.events.len(), 3);
    assert_eq!(snapshot.proposals.len(), 1);
    assert_eq!(snapshot.proposals[0].votes.len(), 1);
    assert!(!snapshot.saved_at.is_empty());
}

#[tokio::test]
async fn test_persistence_list_and_delete() {
    let store = InMemoryCollaborationStore::new();
    let sid = SessionId::from_string("list-session");
    let empty_state = HashMap::new();

    store
        .save_collaboration(&sid, "collab-a", &empty_state, &[], &[])
        .await
        .unwrap();
    store
        .save_collaboration(&sid, "collab-b", &empty_state, &[], &[])
        .await
        .unwrap();

    let mut ids = store.list_collaborations(&sid).await.unwrap();
    ids.sort();
    assert_eq!(ids, vec!["collab-a", "collab-b"]);

    // Delete one
    store.delete_collaboration(&sid, "collab-a").await.unwrap();

    let ids = store.list_collaborations(&sid).await.unwrap();
    assert_eq!(ids, vec!["collab-b"]);

    // Loading deleted returns None
    assert!(store
        .load_collaboration(&sid, "collab-a")
        .await
        .unwrap()
        .is_none());
}

// ── 7. DualAgentManager Migration ──

#[test]
fn test_dual_agent_to_collaboration_migration() {
    let plan_handle = make_test_handle("plan-session");
    let build_handle = make_test_handle("build-session");
    let sid = SessionId::from_string("migration-session");

    let dual = DualAgentManager::new(plan_handle, build_handle, sid);

    // Verify DualAgentManager state before migration
    assert_eq!(dual.session_id().as_str(), "migration-session");
    assert_eq!(dual.plan_handle().session_id.as_str(), "plan-session");
    assert_eq!(dual.build_handle().session_id.as_str(), "build-session");

    // Convert to CollaborationManager
    let collab = dual.into_collaboration_manager();

    // Verify resulting CollaborationManager
    assert_eq!(collab.agent_count(), 2);
    assert_eq!(collab.session_id().as_str(), "migration-session");

    // Verify plan and build agent IDs exist
    let ids = collab.agent_ids();
    assert!(ids.contains(&"plan".to_string()));
    assert!(ids.contains(&"build".to_string()));

    // Verify handles are accessible
    let plan_h = collab.get_handle("plan").unwrap();
    assert_eq!(plan_h.session_id.as_str(), "plan-session");
    let build_h = collab.get_handle("build").unwrap();
    assert_eq!(build_h.session_id.as_str(), "build-session");

    // Verify bidirectional channels exist between plan and build
    // (need mutable access, so re-create for this check)
    let plan_handle2 = make_test_handle("plan-session-2");
    let build_handle2 = make_test_handle("build-session-2");
    let sid2 = SessionId::from_string("migration-session-2");
    let dual2 = DualAgentManager::new(plan_handle2, build_handle2, sid2);
    let mut collab2 = dual2.into_collaboration_manager();
    assert!(collab2.get_channel_mut("plan", "build").is_some());
    assert!(collab2.get_channel_mut("build", "plan").is_some());
}

// ── 8. Manager Status ──

#[test]
fn test_collaboration_status_reflects_state() {
    let sid = SessionId::from_string("status-session");
    let mut mgr = CollaborationManager::new(sid);

    // Empty manager status
    let status = mgr.status();
    assert_eq!(status.agent_count, 0);
    assert!(status.active_agent.is_none());
    assert_eq!(status.pending_proposals, 0);
    assert_eq!(status.event_count, 0);

    // Add agents
    mgr.add_agent(
        "coder".into(),
        "Coder".into(),
        vec![AgentCapability::CodeGeneration],
        make_test_handle("coder-s"),
    );
    mgr.add_agent(
        "reviewer".into(),
        "Reviewer".into(),
        vec![],
        make_test_handle("reviewer-s"),
    );

    // Add proposals to context
    let ctx = mgr.context().clone();
    CollaborationProtocol::propose_action(
        &ctx,
        "coder",
        "refactor".to_string(),
        "Refactor X".to_string(),
    );
    let p2_id = CollaborationProtocol::propose_action(
        &ctx,
        "reviewer",
        "add-tests".to_string(),
        "Add tests for Y".to_string(),
    );
    // Accept one proposal
    CollaborationProtocol::accept_proposal(&ctx, &p2_id);

    // Share some state
    CollaborationProtocol::share_state(&ctx, "coder", "done".to_string(), serde_json::json!(true));

    let status = mgr.status();
    assert_eq!(status.agent_count, 2);
    assert_eq!(status.active_agent, Some("coder".to_string()));
    assert_eq!(status.pending_proposals, 1); // only the first is still pending
    // 2 AgentJoined + 1 StateUpdated = 3 events (proposals don't auto-log events)
    assert_eq!(status.event_count, 3);
    assert_eq!(status.state_keys.len(), 1);
}

// ── 9. Event Logging End-to-End ──

#[test]
fn test_event_logging_lifecycle() {
    let sid = SessionId::from_string("event-session");
    let mut mgr = CollaborationManager::new(sid);

    // Add agent -> AgentJoined event
    mgr.add_agent(
        "coder".into(),
        "Coder".into(),
        vec![AgentCapability::CodeGeneration],
        make_test_handle("coder-s"),
    );
    mgr.add_agent(
        "reviewer".into(),
        "Reviewer".into(),
        vec![AgentCapability::CodeReview],
        make_test_handle("reviewer-s"),
    );

    // Share state -> StateUpdated event
    let ctx = mgr.context().clone();
    CollaborationProtocol::share_state(
        &ctx,
        "coder",
        "progress".to_string(),
        serde_json::json!(42),
    );

    // Remove agent -> AgentLeft event
    mgr.remove_agent("reviewer");

    // Check events in order
    let events = ctx.events();
    assert_eq!(events.len(), 4);

    // Event 0: AgentJoined (coder)
    match &events[0] {
        CollaborationEvent::AgentJoined {
            agent_id,
            capabilities,
        } => {
            assert_eq!(agent_id, "coder");
            assert_eq!(capabilities.len(), 1);
        }
        other => panic!("expected AgentJoined, got {:?}", other),
    }

    // Event 1: AgentJoined (reviewer)
    match &events[1] {
        CollaborationEvent::AgentJoined {
            agent_id,
            capabilities,
        } => {
            assert_eq!(agent_id, "reviewer");
            assert_eq!(capabilities.len(), 1);
        }
        other => panic!("expected AgentJoined, got {:?}", other),
    }

    // Event 2: StateUpdated
    match &events[2] {
        CollaborationEvent::StateUpdated { agent_id, key } => {
            assert_eq!(agent_id, "coder");
            assert_eq!(key, "progress");
        }
        other => panic!("expected StateUpdated, got {:?}", other),
    }

    // Event 3: AgentLeft (reviewer)
    match &events[3] {
        CollaborationEvent::AgentLeft { agent_id } => {
            assert_eq!(agent_id, "reviewer");
        }
        other => panic!("expected AgentLeft, got {:?}", other),
    }
}

// ── 10. Multiple Proposals Workflow ──

#[test]
fn test_multiple_proposals_workflow() {
    let ctx = CollaborationContext::new("multi-proposal".to_string());

    // Propose 3 actions
    let p1_id = CollaborationProtocol::propose_action(
        &ctx,
        "agent-1",
        "refactor".to_string(),
        "Refactor auth".to_string(),
    );
    let p2_id = CollaborationProtocol::propose_action(
        &ctx,
        "agent-2",
        "add-cache".to_string(),
        "Add Redis cache layer".to_string(),
    );
    let p3_id = CollaborationProtocol::propose_action(
        &ctx,
        "agent-3",
        "delete-legacy".to_string(),
        "Delete legacy code".to_string(),
    );

    // All three should be pending
    assert_eq!(CollaborationProtocol::pending_proposals(&ctx).len(), 3);

    // Vote on each
    CollaborationProtocol::vote(&ctx, &p1_id, "agent-2", true, None);
    CollaborationProtocol::vote(&ctx, &p1_id, "agent-3", true, None);

    CollaborationProtocol::vote(&ctx, &p2_id, "agent-1", false, Some("Too complex".into()));
    CollaborationProtocol::vote(&ctx, &p2_id, "agent-3", false, None);

    CollaborationProtocol::vote(&ctx, &p3_id, "agent-1", true, None);
    // p3 only has one vote, leave it pending

    // Accept p1 (majority approved)
    assert!(CollaborationProtocol::accept_proposal(&ctx, &p1_id));

    // Reject p2 (majority rejected)
    assert!(CollaborationProtocol::reject_proposal(&ctx, &p2_id));

    // Leave p3 as pending

    // Verify final states
    let proposals = ctx.proposals();
    assert_eq!(proposals.len(), 3);

    let p1 = proposals.iter().find(|p| p.id == p1_id).unwrap();
    assert_eq!(p1.status, ProposalStatus::Accepted);
    assert_eq!(p1.votes.len(), 2);

    let p2 = proposals.iter().find(|p| p.id == p2_id).unwrap();
    assert_eq!(p2.status, ProposalStatus::Rejected);
    assert_eq!(p2.votes.len(), 2);

    let p3 = proposals.iter().find(|p| p.id == p3_id).unwrap();
    assert_eq!(p3.status, ProposalStatus::Pending);
    assert_eq!(p3.votes.len(), 1);

    // pending_proposals should return only p3
    let pending = CollaborationProtocol::pending_proposals(&ctx);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, p3_id);
}

// ── Bonus: Manager + Context + Protocol integrated workflow ──

#[test]
fn test_full_collaboration_workflow() {
    let sid = SessionId::from_string("full-workflow");
    let mut mgr = CollaborationManager::new(sid);

    // Add agents
    mgr.add_agent(
        "coder".into(),
        "Coder".into(),
        vec![AgentCapability::CodeGeneration],
        make_test_handle("coder-fw"),
    );
    mgr.add_agent(
        "reviewer".into(),
        "Reviewer".into(),
        vec![AgentCapability::CodeReview],
        make_test_handle("reviewer-fw"),
    );

    let ctx = mgr.context().clone();

    // Coder proposes a refactor
    let proposal_id = CollaborationProtocol::propose_action(
        &ctx,
        "coder",
        "refactor-auth".to_string(),
        "Refactor authentication module for better testability".to_string(),
    );

    // Coder shares state indicating work started
    CollaborationProtocol::share_state(
        &ctx,
        "coder",
        "status".to_string(),
        serde_json::json!("in_progress"),
    );

    // Reviewer votes on the proposal
    CollaborationProtocol::vote(
        &ctx,
        &proposal_id,
        "reviewer",
        true,
        Some("Good idea, proceed".into()),
    );

    // Accept the proposal
    CollaborationProtocol::accept_proposal(&ctx, &proposal_id);

    // Coder updates status
    CollaborationProtocol::share_state(
        &ctx,
        "coder",
        "status".to_string(),
        serde_json::json!("completed"),
    );

    // Verify final state
    let status = mgr.status();
    assert_eq!(status.agent_count, 2);
    assert_eq!(status.pending_proposals, 0);
    // 2 AgentJoined + 2 StateUpdated = 4 events
    assert_eq!(status.event_count, 4);

    // Verify shared state shows completed
    assert_eq!(
        ctx.get_state("status"),
        Some(serde_json::json!("completed"))
    );

    // Verify proposal is accepted
    let proposals = ctx.proposals();
    assert_eq!(proposals[0].status, ProposalStatus::Accepted);
    assert_eq!(proposals[0].votes.len(), 1);
}

// ── Bonus: Persistence with manager context roundtrip ──

#[tokio::test]
async fn test_persistence_from_manager_context() {
    let store = InMemoryCollaborationStore::new();
    let sid = SessionId::from_string("persist-mgr");
    let mut mgr = CollaborationManager::new(sid.clone());

    mgr.add_agent(
        "a1".into(),
        "Agent One".into(),
        vec![],
        make_test_handle("a1-s"),
    );

    let ctx = mgr.context().clone();
    CollaborationProtocol::share_state(&ctx, "a1", "key1".to_string(), serde_json::json!("val1"));
    CollaborationProtocol::propose_action(&ctx, "a1", "action".into(), "desc".into());

    // Extract data from context for persistence
    let shared_state = {
        let state = ctx.shared_state.read().unwrap();
        state.clone()
    };
    let events = ctx.events();
    let proposals = ctx.proposals();

    store
        .save_collaboration(&sid, &ctx.id, &shared_state, &events, &proposals)
        .await
        .unwrap();

    // Load and verify
    let snapshot = store
        .load_collaboration(&sid, &ctx.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(snapshot.shared_state.len(), 1);
    assert_eq!(
        snapshot.shared_state.get("key1"),
        Some(&serde_json::json!("val1"))
    );
    assert_eq!(snapshot.events.len(), 2); // AgentJoined + StateUpdated
    assert_eq!(snapshot.proposals.len(), 1);
}
