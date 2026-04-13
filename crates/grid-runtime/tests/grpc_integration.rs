//! gRPC integration tests for grid-runtime v2.0.
//!
//! Tests the full gRPC stack: client → service → GridHarness → grid-engine.
//! Uses an in-process tonic server (no network).

use std::sync::Arc;

use tonic::transport::{Channel, Server};
use tokio::net::TcpListener;

use grid_runtime::harness::GridHarness;
use grid_runtime::proto;
use grid_runtime::proto::runtime_service_client::RuntimeServiceClient;
use grid_runtime::proto::runtime_service_server::RuntimeServiceServer;
use grid_runtime::service::RuntimeGrpcService;

/// Start an in-process gRPC server and return a connected client.
async fn setup_grpc() -> RuntimeServiceClient<Channel> {
    let catalog = Arc::new(grid_engine::AgentCatalog::new());
    let runtime_config = grid_engine::AgentRuntimeConfig::from_parts(
        ":memory:".into(), // in-memory SQLite for tests
        grid_engine::ProviderConfig::default(),
        vec![],
        None,
        None,
        false,
    );
    let tenant_context = grid_engine::TenantContext::for_single_user(
        grid_types::id::TenantId::from_string("test"),
        grid_types::id::UserId::from_string("test-user"),
    );

    let runtime = grid_engine::AgentRuntime::new(catalog, runtime_config, Some(tenant_context))
        .await
        .expect("Failed to build AgentRuntime");

    let harness = Arc::new(GridHarness::new(Arc::new(runtime)));
    let service = RuntimeGrpcService::new(harness);
    let server = RuntimeServiceServer::new(service);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        Server::builder()
            .add_service(server)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    RuntimeServiceClient::connect(format!("http://{}", addr))
        .await
        .expect("Failed to connect to gRPC server")
}

fn v2_payload_for_user(user: &str) -> proto::SessionPayload {
    proto::SessionPayload {
        user_id: user.into(),
        runtime_id: "grid-harness".into(),
        user_preferences: Some(proto::UserPreferences {
            user_id: user.into(),
            language: "en".into(),
            ..Default::default()
        }),
        allow_trim_p5: true,
        ..Default::default()
    }
}

// ──────────────────────────────────────────────────────────────
// v2 — core happy-path tests
// ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_health() {
    let mut client = setup_grpc().await;

    let response = client
        .health(proto::Empty {})
        .await
        .expect("health failed");

    let status = response.into_inner();
    assert!(status.healthy);
    assert_eq!(status.runtime_id, "grid-harness");
}

#[tokio::test]
async fn test_get_capabilities() {
    let mut client = setup_grpc().await;

    let response = client
        .get_capabilities(proto::Empty {})
        .await
        .expect("get_capabilities failed");

    let caps = response.into_inner();
    assert_eq!(caps.runtime_id, "grid-harness");
    assert_eq!(caps.tier, "harness");
    assert!(caps.supports_native_hooks);
    assert!(caps.supports_native_mcp);
    assert!(caps.supports_native_skills);
}

#[tokio::test]
async fn test_initialize_and_terminate() {
    let mut client = setup_grpc().await;

    // Initialize
    let response = client
        .initialize(proto::InitializeRequest {
            payload: Some(v2_payload_for_user("test-user")),
        })
        .await
        .expect("initialize failed");

    let init = response.into_inner();
    assert!(!init.session_id.is_empty());
    assert_eq!(init.runtime_id, "grid-harness");

    // Terminate uses implicit current_session
    let response = client
        .terminate(proto::Empty {})
        .await
        .expect("terminate failed");
    let _ = response.into_inner();
}

#[tokio::test]
async fn test_on_tool_call_allows() {
    let mut client = setup_grpc().await;

    let response = client
        .on_tool_call(proto::ToolCallEvent {
            session_id: "test".into(),
            tool_name: "bash".into(),
            tool_id: "t1".into(),
            input_json: "{}".into(),
        })
        .await
        .expect("on_tool_call failed");

    assert_eq!(response.into_inner().decision, "allow");
}

#[tokio::test]
async fn test_on_stop_completes() {
    let mut client = setup_grpc().await;

    let response = client
        .on_stop(proto::StopEvent {
            session_id: "test".into(),
            reason: "done".into(),
        })
        .await
        .expect("on_stop failed");

    // v2 collapses StopDecision into "allow" on complete
    assert_eq!(response.into_inner().decision, "allow");
}

#[tokio::test]
async fn test_on_tool_result_allows() {
    let mut client = setup_grpc().await;

    let response = client
        .on_tool_result(proto::ToolResultEvent {
            session_id: "test".into(),
            tool_name: "bash".into(),
            tool_id: "t2".into(),
            output: "file1.rs\nfile2.rs".into(),
            is_error: false,
        })
        .await
        .expect("on_tool_result failed");

    assert_eq!(response.into_inner().decision, "allow");
}

#[tokio::test]
async fn test_session_lifecycle_get_state() {
    let mut client = setup_grpc().await;

    // Initialize session
    let init_resp = client
        .initialize(proto::InitializeRequest {
            payload: Some(v2_payload_for_user("certifier-user")),
        })
        .await
        .expect("initialize failed");

    let session_id = init_resp.into_inner().session_id;
    assert!(!session_id.is_empty());

    // Get state — implicit session lookup
    let state_resp = client
        .get_state(proto::Empty {})
        .await
        .expect("get_state failed");

    let state = state_resp.into_inner();
    assert_eq!(state.session_id, session_id);
    assert_eq!(state.runtime_id, "grid-harness");
    assert_eq!(state.state_format, "rust-serde-v2");
    assert!(!state.created_at.is_empty());

    // Terminate
    client
        .terminate(proto::Empty {})
        .await
        .expect("terminate failed");
}

#[tokio::test]
async fn test_emit_telemetry_is_fire_and_forget() {
    let mut client = setup_grpc().await;

    // Initialize session
    let init_resp = client
        .initialize(proto::InitializeRequest {
            payload: Some(v2_payload_for_user("telem-user")),
        })
        .await
        .expect("initialize failed");

    let session_id = init_resp.into_inner().session_id;

    // Emit telemetry — v2 returns Empty
    client
        .emit_telemetry(proto::TelemetryRequest {
            session_id: session_id.clone(),
            events: vec![],
        })
        .await
        .expect("emit_telemetry failed");

    // Cleanup
    let _ = client.terminate(proto::Empty {}).await;
}

#[tokio::test]
async fn test_load_skill_succeeds() {
    let mut client = setup_grpc().await;

    let response = client
        .load_skill(proto::LoadSkillRequest {
            session_id: "test-session".into(),
            skill: Some(proto::SkillInstructions {
                skill_id: "skill-001".into(),
                name: "code-review".into(),
                content: "Review code for quality.".into(),
                frontmatter_hooks: vec![],
                metadata: Default::default(),
                dependencies: vec![],
            }),
        })
        .await
        .expect("load_skill failed");

    let resp = response.into_inner();
    assert!(resp.success);
    assert!(resp.error.is_empty());
}

#[tokio::test]
async fn test_disconnect_mcp_succeeds() {
    let mut client = setup_grpc().await;

    // DisconnectMcp on a non-existent server should still succeed
    client
        .disconnect_mcp(proto::DisconnectMcpRequest {
            session_id: "test".into(),
            server_name: "nonexistent-mcp".into(),
        })
        .await
        .expect("disconnect_mcp failed");
}

#[tokio::test]
async fn test_pause_session_returns_state() {
    let mut client = setup_grpc().await;

    // Initialize session first
    client
        .initialize(proto::InitializeRequest {
            payload: Some(v2_payload_for_user("pause-user")),
        })
        .await
        .expect("initialize failed");

    // Pause session — v2 returns StateResponse
    let pause_resp = client
        .pause_session(proto::Empty {})
        .await
        .expect("pause_session failed");

    let state = pause_resp.into_inner();
    assert!(!state.session_id.is_empty());
    assert_eq!(state.runtime_id, "grid-harness");
}

#[tokio::test]
async fn test_resume_session_degrades_gracefully() {
    let mut client = setup_grpc().await;

    // v2 ResumeSession takes a full StateResponse; the underlying
    // GridHarness::resume_session is a stub that always errors.
    let result = client
        .resume_session(proto::StateResponse {
            session_id: "nonexistent-session".into(),
            runtime_id: "grid-harness".into(),
            state_data: vec![],
            state_format: "rust-serde-v2".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .await;

    assert!(result.is_err(), "resume_session should fail without L4 session store");
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Internal);
    assert!(
        status.message().contains("restore_state"),
        "error should mention restore_state as the correct alternative"
    );
}

#[tokio::test]
async fn test_emit_event_returns_ok() {
    // ADR-V2-001 Accepted (Phase 1) — EmitEvent is OPTIONAL.
    // Default implementation is no-op (returns Ok).
    // Core events are captured by L4 platform interceptor.
    let mut client = setup_grpc().await;

    let result = client
        .emit_event(proto::EventStreamEntry {
            session_id: "s-1".into(),
            event_id: "e-1".into(),
            event_type: proto::HookEventType::PreToolUse as i32,
            payload_json: "{}".into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
        .await;

    result.expect("emit_event should return Ok (no-op default)");
}

// ──────────────────────────────────────────────────────────────
// Legacy v1 tests — kept for documentation value but disabled
// pending v2-style rewrites. See TODO(s2.t1).
// ──────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "TODO(s2.t1): rewrite for v2 terminate telemetry envelope"]
async fn test_terminate_includes_final_telemetry() {
    // v1 bundled a final_telemetry batch inside TerminateResponse;
    // v2 simplifies Terminate to Empty/Empty. Final telemetry must
    // now be pulled via EmitTelemetry before Terminate.
}
