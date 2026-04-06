//! gRPC integration tests for grid-runtime.
//!
//! Tests the full gRPC stack: client → service → GridHarness → grid-engine.
//! Uses an in-process tonic server (no network).

use std::sync::Arc;

use tonic::transport::{Channel, Server};
use tokio::net::TcpListener;

use grid_runtime::common_proto;
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

#[tokio::test]
async fn test_health() {
    let mut client = setup_grpc().await;

    let response = client
        .health(common_proto::Empty {})
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
        .get_capabilities(common_proto::Empty {})
        .await
        .expect("get_capabilities failed");

    let manifest = response.into_inner();
    assert_eq!(manifest.runtime_name, "Grid");
    assert_eq!(manifest.tier, "harness");
    assert!(manifest.native_hooks);
    assert!(manifest.native_mcp);
    assert!(manifest.native_skills);
    assert!(!manifest.requires_hook_bridge);
}

#[tokio::test]
async fn test_initialize_and_terminate() {
    let mut client = setup_grpc().await;

    // Initialize
    let response = client
        .initialize(proto::InitializeRequest {
            payload: Some(proto::SessionPayload {
                user_id: "test-user".into(),
                user_role: "developer".into(),
                org_unit: "engineering".into(),
                managed_hooks_json: String::new(),
                quotas: Default::default(),
                context: Default::default(),
                hook_bridge_url: String::new(),
                telemetry_endpoint: String::new(),
            }),
        })
        .await
        .expect("initialize failed");

    let session_id = response.into_inner().session_id;
    assert!(!session_id.is_empty());

    // Terminate
    let response = client
        .terminate(proto::TerminateRequest {
            session_id: session_id.clone(),
        })
        .await
        .expect("terminate failed");

    assert!(response.into_inner().success);
}

#[tokio::test]
async fn test_on_tool_call_allows() {
    let mut client = setup_grpc().await;

    let response = client
        .on_tool_call(common_proto::ToolCallEvent {
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
        .on_stop(common_proto::StopRequest {
            session_id: "test".into(),
        })
        .await
        .expect("on_stop failed");

    assert_eq!(response.into_inner().decision, "complete");
}

// ── W5: Certifier Degradation Tests ──
//
// These tests simulate EAASP certifier verification scenarios,
// covering the complete session lifecycle and graceful degradation
// for methods that require L4 session store (not yet available).

#[tokio::test]
async fn test_session_lifecycle_get_state() {
    let mut client = setup_grpc().await;

    // Initialize session
    let init_resp = client
        .initialize(proto::InitializeRequest {
            payload: Some(proto::SessionPayload {
                user_id: "certifier-user".into(),
                user_role: "evaluator".into(),
                org_unit: "eaasp-certifier".into(),
                managed_hooks_json: String::new(),
                quotas: Default::default(),
                context: Default::default(),
                hook_bridge_url: String::new(),
                telemetry_endpoint: String::new(),
            }),
        })
        .await
        .expect("initialize failed");

    let session_id = init_resp.into_inner().session_id;

    // Get state — should return valid state with rust-serde-v1 format
    let state_resp = client
        .get_state(proto::GetStateRequest {
            session_id: session_id.clone(),
        })
        .await
        .expect("get_state failed");

    let state = state_resp.into_inner();
    assert_eq!(state.session_id, session_id);
    assert_eq!(state.runtime_id, "grid-harness");
    assert_eq!(state.state_format, "rust-serde-v1");
    assert!(!state.created_at.is_empty());

    // Terminate
    let term_resp = client
        .terminate(proto::TerminateRequest {
            session_id: session_id.clone(),
        })
        .await
        .expect("terminate failed");

    assert!(term_resp.into_inner().success);
}

#[tokio::test]
async fn test_emit_telemetry_returns_metering() {
    let mut client = setup_grpc().await;

    // Initialize session
    let init_resp = client
        .initialize(proto::InitializeRequest {
            payload: Some(proto::SessionPayload {
                user_id: "telem-user".into(),
                user_role: "developer".into(),
                org_unit: "test".into(),
                managed_hooks_json: String::new(),
                quotas: Default::default(),
                context: Default::default(),
                hook_bridge_url: String::new(),
                telemetry_endpoint: String::new(),
            }),
        })
        .await
        .expect("initialize failed");

    let session_id = init_resp.into_inner().session_id;

    // Emit telemetry — should return at least metering snapshot
    let telem_resp = client
        .emit_telemetry(proto::EmitTelemetryRequest {
            session_id: session_id.clone(),
        })
        .await
        .expect("emit_telemetry failed");

    let batch = telem_resp.into_inner();
    assert!(!batch.events.is_empty(), "should have at least metering snapshot");

    let metering = &batch.events[0];
    assert_eq!(metering.event_type, "metering_snapshot");
    assert_eq!(metering.runtime_id, "grid-harness");
    assert!(!metering.timestamp.is_empty());
    assert!(metering.resource_usage.is_some());

    // Cleanup
    let _ = client
        .terminate(proto::TerminateRequest { session_id })
        .await;
}

#[tokio::test]
async fn test_load_skill_succeeds() {
    let mut client = setup_grpc().await;

    let response = client
        .load_skill(proto::LoadSkillRequest {
            session_id: "test-session".into(),
            skill: Some(proto::SkillContent {
                skill_id: "skill-001".into(),
                name: "code-review".into(),
                frontmatter_yaml: "runtime_affinity: grid".into(),
                prose: "Review code for quality.".into(),
            }),
        })
        .await
        .expect("load_skill failed");

    let resp = response.into_inner();
    assert!(resp.success);
    assert!(resp.error.is_empty());
}

#[tokio::test]
async fn test_on_tool_result_allows() {
    let mut client = setup_grpc().await;

    let response = client
        .on_tool_result(common_proto::ToolResultEvent {
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
async fn test_disconnect_mcp_succeeds() {
    let mut client = setup_grpc().await;

    // DisconnectMcp on a non-existent server should still succeed
    // (graceful degradation — no error for missing server)
    let response = client
        .disconnect_mcp(proto::DisconnectMcpRequest {
            session_id: "test".into(),
            server_name: "nonexistent-mcp".into(),
        })
        .await
        .expect("disconnect_mcp failed");

    assert!(response.into_inner().success);
}

#[tokio::test]
async fn test_pause_session_succeeds() {
    let mut client = setup_grpc().await;

    // Initialize session first
    let init_resp = client
        .initialize(proto::InitializeRequest {
            payload: Some(proto::SessionPayload {
                user_id: "pause-user".into(),
                user_role: "developer".into(),
                org_unit: "test".into(),
                managed_hooks_json: String::new(),
                quotas: Default::default(),
                context: Default::default(),
                hook_bridge_url: String::new(),
                telemetry_endpoint: String::new(),
            }),
        })
        .await
        .expect("initialize failed");

    let session_id = init_resp.into_inner().session_id;

    // Pause session — should succeed
    let pause_resp = client
        .pause_session(proto::PauseRequest {
            session_id: session_id.clone(),
        })
        .await
        .expect("pause_session failed");

    assert!(pause_resp.into_inner().success);
}

#[tokio::test]
async fn test_resume_session_degrades_gracefully() {
    let mut client = setup_grpc().await;

    // ResumeSession requires L4 session store — should return gRPC error
    // This is the key certifier degradation test: Grid correctly reports
    // that resume needs external state rather than silently failing.
    let result = client
        .resume_session(proto::ResumeRequest {
            session_id: "nonexistent-session".into(),
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
async fn test_terminate_includes_final_telemetry() {
    let mut client = setup_grpc().await;

    // Initialize
    let init_resp = client
        .initialize(proto::InitializeRequest {
            payload: Some(proto::SessionPayload {
                user_id: "final-telem-user".into(),
                user_role: "developer".into(),
                org_unit: "test".into(),
                managed_hooks_json: String::new(),
                quotas: Default::default(),
                context: Default::default(),
                hook_bridge_url: String::new(),
                telemetry_endpoint: String::new(),
            }),
        })
        .await
        .expect("initialize failed");

    let session_id = init_resp.into_inner().session_id;

    // Terminate should include final telemetry batch
    let term_resp = client
        .terminate(proto::TerminateRequest {
            session_id: session_id.clone(),
        })
        .await
        .expect("terminate failed");

    let resp = term_resp.into_inner();
    assert!(resp.success);
    assert!(
        resp.final_telemetry.is_some(),
        "terminate should include final telemetry batch"
    );
    let batch = resp.final_telemetry.unwrap();
    assert!(
        !batch.events.is_empty(),
        "final telemetry should have at least metering snapshot"
    );
}
