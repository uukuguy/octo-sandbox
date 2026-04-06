//! gRPC integration tests for grid-runtime.
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
        .on_stop(proto::StopRequest {
            session_id: "test".into(),
        })
        .await
        .expect("on_stop failed");

    assert_eq!(response.into_inner().decision, "complete");
}
