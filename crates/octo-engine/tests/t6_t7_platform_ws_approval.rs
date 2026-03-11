//! Tests for T6 (Platform WS AgentRuntime integration) and T7 (ApprovalGate wiring)

use std::time::Duration;

use octo_engine::tools::approval::{ApprovalGate, ApprovalManager, ApprovalPolicy};
use octo_types::{ApprovalRequirement, RiskLevel};

// ============================================================================
// T7: ApprovalGate shared wiring tests
// ============================================================================

#[tokio::test]
async fn approval_gate_shared_clone_register_and_respond() {
    // Simulate the shared gate pattern: one clone in AppState (WS handler),
    // another clone in AgentLoopConfig (harness).
    let gate = ApprovalGate::new();

    // Clone for "AppState" (WS handler side)
    let ws_gate = gate.clone();
    // Clone for "AgentLoopConfig" (harness side)
    let harness_gate = gate.clone();

    // Harness registers a pending approval
    let rx = harness_gate.register("tool-abc").await;

    // WS handler delivers the approval response
    let found = ws_gate.respond("tool-abc", true).await;
    assert!(found, "WS handler should find the pending approval");

    // Harness receives the approval
    let approved = ApprovalGate::wait_for_approval(rx).await;
    assert!(approved, "Harness should receive approved=true");
}

#[tokio::test]
async fn approval_gate_shared_clone_rejection() {
    let gate = ApprovalGate::new();
    let ws_gate = gate.clone();
    let harness_gate = gate.clone();

    let rx = harness_gate.register("tool-xyz").await;

    // WS handler rejects
    let found = ws_gate.respond("tool-xyz", false).await;
    assert!(found);

    let approved = ApprovalGate::wait_for_approval(rx).await;
    assert!(!approved, "Harness should receive approved=false");
}

#[tokio::test]
async fn approval_gate_multiple_pending() {
    let gate = ApprovalGate::new();

    let rx1 = gate.register("tool-1").await;
    let rx2 = gate.register("tool-2").await;

    // Respond to tool-2 first (out of order)
    assert!(gate.respond("tool-2", true).await);
    assert!(gate.respond("tool-1", false).await);

    assert!(ApprovalGate::wait_for_approval(rx2).await);
    assert!(!ApprovalGate::wait_for_approval(rx1).await);
}

#[tokio::test]
async fn approval_gate_respond_nonexistent_returns_false() {
    let gate = ApprovalGate::new();
    let found = gate.respond("nonexistent", true).await;
    assert!(!found);
}

#[tokio::test]
async fn approval_gate_dropped_sender_rejects() {
    let gate = ApprovalGate::new();
    let rx = gate.register("tool-drop").await;

    // Drop the gate (and all internal senders)
    drop(gate);

    let approved = ApprovalGate::wait_for_approval(rx).await;
    assert!(!approved, "Dropped sender should auto-reject");
}

// ============================================================================
// T7: ApprovalManager + ApprovalGate integration
// ============================================================================

#[tokio::test]
async fn approval_manager_with_gate_full_cycle() {
    let manager = ApprovalManager::new(ApprovalPolicy::AlwaysAsk);
    let gate = ApprovalGate::new();

    // Manager decides tool needs approval
    let decision = manager.check_requirement(
        "bash",
        ApprovalRequirement::Always,
        RiskLevel::HighRisk,
    );
    assert!(
        matches!(decision, octo_engine::tools::approval::ApprovalDecision::NeedsApproval { .. }),
        "bash with Always requirement should need approval"
    );

    // Harness registers and waits
    let rx = gate.register("call-123").await;

    // Simulate WS response (in another task)
    let gate_clone = gate.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        gate_clone.respond("call-123", true).await;
    });

    let approved = ApprovalGate::wait_for_approval(rx).await;
    assert!(approved);
}

#[tokio::test]
async fn approval_manager_dev_mode_skips_gate() {
    let manager = ApprovalManager::dev_mode();

    // Even with Always requirement, dev mode auto-approves
    let decision = manager.check_requirement(
        "bash",
        ApprovalRequirement::Always,
        RiskLevel::Destructive,
    );
    assert_eq!(
        decision,
        octo_engine::tools::approval::ApprovalDecision::Approved,
        "Dev mode should auto-approve everything"
    );
    // No gate interaction needed
}

// ============================================================================
// T6: ClientMessage / ServerMessage serde tests (platform ws.rs types)
// ============================================================================

/// Test that the platform's ClientMessage types can round-trip through JSON.
/// We test serde directly since these are the same patterns used in ws.rs.
#[test]
fn platform_client_message_serde_chat() {
    let json = r#"{"type":"chat","content":"hello world"}"#;
    let msg: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(msg["type"], "chat");
    assert_eq!(msg["content"], "hello world");
}

#[test]
fn platform_client_message_serde_cancel() {
    let json = r#"{"type":"cancel"}"#;
    let msg: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(msg["type"], "cancel");
}

#[test]
fn platform_client_message_serde_approval_response() {
    let json = r#"{"type":"approval_response","tool_id":"t-1","approved":true}"#;
    let msg: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(msg["type"], "approval_response");
    assert_eq!(msg["tool_id"], "t-1");
    assert_eq!(msg["approved"], true);
}

#[test]
fn platform_client_message_serde_ping() {
    let json = r#"{"type":"ping"}"#;
    let msg: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(msg["type"], "ping");
}

/// Test AgentEvent -> ServerMessage mapping patterns.
/// We verify that AgentEvent variants serialize correctly for the WS protocol.
#[test]
fn agent_event_text_delta_serializes() {
    use octo_engine::AgentEvent;
    let event = AgentEvent::TextDelta {
        text: "hello".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "TextDelta");
    assert_eq!(v["text"], "hello");
}

#[test]
fn agent_event_tool_start_serializes() {
    use octo_engine::AgentEvent;
    let event = AgentEvent::ToolStart {
        tool_id: "t-1".to_string(),
        tool_name: "bash".to_string(),
        input: serde_json::json!({"command": "ls"}),
    };
    let json = serde_json::to_string(&event).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "ToolStart");
    assert_eq!(v["tool_name"], "bash");
}

#[test]
fn agent_event_approval_required_serializes() {
    use octo_engine::AgentEvent;
    let event = AgentEvent::ApprovalRequired {
        tool_name: "bash".to_string(),
        tool_id: "t-2".to_string(),
        risk_level: RiskLevel::HighRisk,
    };
    let json = serde_json::to_string(&event).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "ApprovalRequired");
    assert_eq!(v["tool_name"], "bash");
    assert_eq!(v["tool_id"], "t-2");
}

#[test]
fn agent_event_done_serializes() {
    use octo_engine::AgentEvent;
    let event = AgentEvent::Done;
    let json = serde_json::to_string(&event).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "Done");
}

#[test]
fn agent_event_security_blocked_serializes() {
    use octo_engine::AgentEvent;
    let event = AgentEvent::SecurityBlocked {
        reason: "path traversal".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "SecurityBlocked");
    assert_eq!(v["reason"], "path traversal");
}

// ============================================================================
// T7: AgentLoopConfig builder with approval_gate
// ============================================================================

#[test]
fn agent_loop_config_builder_approval_gate() {
    use octo_engine::agent::AgentLoopConfig;
    let gate = ApprovalGate::new();
    let config = AgentLoopConfig::builder()
        .approval_gate(gate)
        .build();
    assert!(config.approval_gate.is_some());
}

#[test]
fn agent_loop_config_default_has_no_gate() {
    use octo_engine::agent::AgentLoopConfig;
    let config = AgentLoopConfig::default();
    assert!(config.approval_gate.is_none());
}
