//! gRPC service implementation for EAASP v2.0 RuntimeService (16 methods).
//!
//! Maps tonic-generated v2 types ↔ Rust-native contract types and
//! delegates to `RuntimeContract` implementations (e.g. `GridHarness`).
//!
//! ## v2 quirks
//!
//! Several v2 methods take `Empty` (GetState, Terminate, Pause, Health,
//! Capabilities). For the shared-process case we track a "last-initialized"
//! session as the implicit context — good enough for Phase 0 MVP since
//! the certifier drives one session at a time. Phase 1 will add an
//! explicit session_id header / metadata channel.

use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use crate::contract::{self, RuntimeContract};
use crate::proto;
use crate::proto::runtime_service_server::RuntimeService;

/// Map a domain `ResponseChunk.chunk_type` string into the proto
/// `ChunkType` enum per ADR-V2-021. Unknown strings fall back to
/// `UNSPECIFIED` (0) which is explicitly forbidden by the contract —
/// emission of UNSPECIFIED should be treated as a bug at the grpc boundary.
fn chunk_type_to_proto(s: &str) -> i32 {
    use proto::ChunkType;
    match s {
        "text_delta" => ChunkType::TextDelta as i32,
        "thinking" => ChunkType::Thinking as i32,
        "tool_start" => ChunkType::ToolStart as i32,
        "tool_result" => ChunkType::ToolResult as i32,
        "done" => ChunkType::Done as i32,
        "error" => ChunkType::Error as i32,
        "workflow_continuation" => ChunkType::WorkflowContinuation as i32,
        other => {
            tracing::error!(
                chunk_type = %other,
                "ADR-V2-021 violation: unknown chunk_type string at grpc boundary; emitting UNSPECIFIED"
            );
            ChunkType::Unspecified as i32
        }
    }
}

/// gRPC service wrapping a RuntimeContract implementation.
pub struct RuntimeGrpcService<C: RuntimeContract> {
    contract: Arc<C>,
    /// Last initialized session — provides implicit context for
    /// v2 methods whose request is `Empty` (GetState, Terminate, …).
    current_session: Arc<RwLock<Option<String>>>,
}

impl<C: RuntimeContract + 'static> RuntimeGrpcService<C> {
    pub fn new(contract: Arc<C>) -> Self {
        Self {
            contract,
            current_session: Arc::new(RwLock::new(None)),
        }
    }

    async fn remember_session(&self, session_id: &str) {
        let mut lock = self.current_session.write().await;
        *lock = Some(session_id.to_string());
    }

    async fn current_session_or(&self) -> Result<String, Status> {
        self.current_session
            .read()
            .await
            .clone()
            .ok_or_else(|| Status::failed_precondition("no active session; call Initialize first"))
    }
}

// ── Type conversion helpers ──

fn to_user_message(m: proto::UserMessage) -> contract::UserMessage {
    contract::UserMessage {
        content: m.content,
        message_type: m.message_type,
        metadata: m.metadata,
    }
}

fn to_skill_content(s: proto::SkillInstructions) -> contract::SkillContent {
    // Convert typed SkillInstructions → native SkillContent.
    // v2 no longer carries raw frontmatter YAML; we serialize the
    // contract-native ScopedHook list as JSON for downstream consumers.
    let native: contract::SkillInstructions = s.into();
    contract::SkillContent {
        skill_id: native.skill_id,
        name: native.name,
        frontmatter_yaml: serde_json::to_string(&native.frontmatter_hooks).unwrap_or_default(),
        prose: native.content,
        required_tools: native.required_tools,
    }
}

fn to_mcp_configs(servers: Vec<proto::McpServerConfig>) -> Vec<contract::McpServerConfig> {
    servers
        .into_iter()
        .map(|s| contract::McpServerConfig {
            name: s.name,
            transport: s.transport,
            command: if s.command.is_empty() {
                None
            } else {
                Some(s.command)
            },
            args: s.args,
            url: if s.url.is_empty() { None } else { Some(s.url) },
            env: s.env,
        })
        .collect()
}

fn hook_decision_to_tool_call_ack(d: contract::HookDecision) -> proto::ToolCallAck {
    match d {
        contract::HookDecision::Allow => proto::ToolCallAck {
            decision: "allow".into(),
            mutated_input_json: String::new(),
            reason: String::new(),
        },
        contract::HookDecision::Deny { reason } => proto::ToolCallAck {
            decision: "deny".into(),
            mutated_input_json: String::new(),
            reason,
        },
        contract::HookDecision::Modify { transformed_input } => proto::ToolCallAck {
            decision: "mutate".into(),
            mutated_input_json: serde_json::to_string(&transformed_input).unwrap_or_default(),
            reason: String::new(),
        },
    }
}

fn hook_decision_to_tool_result_ack(d: contract::HookDecision) -> proto::ToolResultAck {
    match d {
        contract::HookDecision::Allow => proto::ToolResultAck {
            decision: "allow".into(),
            reason: String::new(),
        },
        contract::HookDecision::Deny { reason } => proto::ToolResultAck {
            decision: "deny".into(),
            reason,
        },
        contract::HookDecision::Modify { .. } => proto::ToolResultAck {
            // post_tool_result has no mutate semantics; collapse to allow.
            decision: "allow".into(),
            reason: String::new(),
        },
    }
}

fn stop_decision_to_ack(d: contract::StopDecision) -> proto::StopAck {
    match d {
        contract::StopDecision::Complete => proto::StopAck {
            decision: "allow".into(),
            reason: String::new(),
        },
        contract::StopDecision::Continue { feedback } => proto::StopAck {
            decision: "deny".into(),
            reason: feedback,
        },
    }
}

fn session_state_to_proto(s: contract::SessionState) -> proto::StateResponse {
    proto::StateResponse {
        session_id: s.session_id,
        state_data: s.state_data,
        runtime_id: s.runtime_id,
        state_format: s.state_format,
        created_at: s.created_at.to_rfc3339(),
    }
}

fn capability_to_proto(c: contract::CapabilityManifest) -> proto::Capabilities {
    use proto::capabilities::CredentialMode;
    proto::Capabilities {
        runtime_id: c.runtime_id,
        model: c.model,
        context_window: c.context_window as i32,
        tools: c.supported_tools,
        supports_native_hooks: c.native_hooks,
        supports_native_mcp: c.native_mcp,
        supports_native_skills: c.native_skills,
        cost_per_1k_tokens: c
            .cost
            .map(|c| (c.input_cost_per_1k + c.output_cost_per_1k) / 2.0)
            .unwrap_or(0.0),
        credential_mode: CredentialMode::Direct as i32,
        strengths: vec![],
        limitations: vec![],
        tier: match c.tier {
            contract::RuntimeTier::Harness => "harness".into(),
            contract::RuntimeTier::Aligned => "aligned".into(),
            contract::RuntimeTier::Framework => "framework".into(),
        },
        deployment_mode: match c.deployment_mode {
            contract::DeploymentMode::Shared => "shared".into(),
            contract::DeploymentMode::PerSession => "per_session".into(),
        },
    }
}

fn health_to_proto(h: contract::HealthStatus) -> proto::HealthResponse {
    proto::HealthResponse {
        healthy: h.healthy,
        runtime_id: h.runtime_id,
        checks: h.checks,
    }
}

// ── gRPC Service Implementation ──

type SendStream = Pin<Box<dyn Stream<Item = Result<proto::SendResponse, Status>> + Send>>;

#[tonic::async_trait]
impl<C: RuntimeContract + 'static> RuntimeService for RuntimeGrpcService<C> {
    type SendStream = SendStream;

    async fn initialize(
        &self,
        request: Request<proto::InitializeRequest>,
    ) -> Result<Response<proto::InitializeResponse>, Status> {
        let payload = request
            .into_inner()
            .payload
            .ok_or_else(|| Status::invalid_argument("missing payload"))?;

        let handle = self
            .contract
            .initialize(payload.into())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.remember_session(&handle.session_id).await;
        let runtime_id = self.contract.get_capabilities().runtime_id;

        Ok(Response::new(proto::InitializeResponse {
            session_id: handle.session_id,
            runtime_id,
        }))
    }

    async fn send(
        &self,
        request: Request<proto::SendRequest>,
    ) -> Result<Response<Self::SendStream>, Status> {
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id,
        };
        let message = req
            .message
            .ok_or_else(|| Status::invalid_argument("missing message"))?;

        let stream = self
            .contract
            .send(&handle, to_user_message(message))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_stream = tokio_stream::StreamExt::map(stream, |chunk| {
            Ok(proto::SendResponse {
                // ADR-V2-021: `chunk_type` is now the `ChunkType` proto enum
                // (int32 on the wire). Map the domain string — which
                // `harness.rs` keeps in canonical lowercase — to the enum.
                chunk_type: chunk_type_to_proto(&chunk.chunk_type),
                content: chunk.content,
                tool_name: chunk.tool_name.unwrap_or_default(),
                tool_id: chunk.tool_id.unwrap_or_default(),
                is_error: chunk.is_error,
                error: None,
            })
        });

        Ok(Response::new(Box::pin(proto_stream)))
    }

    async fn load_skill(
        &self,
        request: Request<proto::LoadSkillRequest>,
    ) -> Result<Response<proto::LoadSkillResponse>, Status> {
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id,
        };
        let skill = req
            .skill
            .ok_or_else(|| Status::invalid_argument("missing skill"))?;

        match self
            .contract
            .load_skill(&handle, to_skill_content(skill))
            .await
        {
            Ok(()) => Ok(Response::new(proto::LoadSkillResponse {
                success: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(proto::LoadSkillResponse {
                success: false,
                error: e.to_string(),
            })),
        }
    }

    async fn on_tool_call(
        &self,
        request: Request<proto::ToolCallEvent>,
    ) -> Result<Response<proto::ToolCallAck>, Status> {
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id,
        };
        let call = contract::ToolCall {
            tool_name: req.tool_name,
            tool_id: req.tool_id,
            input: serde_json::from_str(&req.input_json).unwrap_or_default(),
        };

        let decision = self
            .contract
            .on_tool_call(&handle, call)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(hook_decision_to_tool_call_ack(decision)))
    }

    async fn on_tool_result(
        &self,
        request: Request<proto::ToolResultEvent>,
    ) -> Result<Response<proto::ToolResultAck>, Status> {
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id,
        };
        let result = contract::ToolResult {
            tool_name: req.tool_name,
            tool_id: req.tool_id,
            output: req.output,
            is_error: req.is_error,
        };

        let decision = self
            .contract
            .on_tool_result(&handle, result)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(hook_decision_to_tool_result_ack(decision)))
    }

    async fn on_stop(
        &self,
        request: Request<proto::StopEvent>,
    ) -> Result<Response<proto::StopAck>, Status> {
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id,
        };

        let decision = self
            .contract
            .on_stop(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(stop_decision_to_ack(decision)))
    }

    async fn get_state(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::StateResponse>, Status> {
        let session_id = self.current_session_or().await?;
        let handle = contract::SessionHandle { session_id };

        let state = self
            .contract
            .get_state(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(session_state_to_proto(state)))
    }

    async fn connect_mcp(
        &self,
        request: Request<proto::ConnectMcpRequest>,
    ) -> Result<Response<proto::ConnectMcpResponse>, Status> {
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id,
        };
        let server_names: Vec<String> = req.servers.iter().map(|s| s.name.clone()).collect();

        match self
            .contract
            .connect_mcp(&handle, to_mcp_configs(req.servers))
            .await
        {
            Ok(()) => Ok(Response::new(proto::ConnectMcpResponse {
                success: true,
                connected: server_names,
                failed: vec![],
            })),
            Err(e) => Ok(Response::new(proto::ConnectMcpResponse {
                success: false,
                connected: vec![],
                failed: vec![e.to_string()],
            })),
        }
    }

    async fn emit_telemetry(
        &self,
        request: Request<proto::TelemetryRequest>,
    ) -> Result<Response<proto::Empty>, Status> {
        // v2 EmitTelemetry is fire-and-forget from the runtime's POV.
        // We still pull telemetry out of the contract for legacy
        // consumers, but discard the result on the wire.
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id,
        };

        let _events = self
            .contract
            .emit_telemetry(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::Empty {}))
    }

    async fn get_capabilities(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::Capabilities>, Status> {
        let manifest = self.contract.get_capabilities();
        Ok(Response::new(capability_to_proto(manifest)))
    }

    async fn terminate(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::Empty>, Status> {
        let session_id = self.current_session_or().await?;
        let handle = contract::SessionHandle { session_id };

        self.contract
            .terminate(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Clear current session tracker.
        *self.current_session.write().await = None;

        Ok(Response::new(proto::Empty {}))
    }

    async fn restore_state(
        &self,
        request: Request<proto::StateResponse>,
    ) -> Result<Response<proto::Empty>, Status> {
        let ps = request.into_inner();
        let state = contract::SessionState {
            session_id: ps.session_id,
            runtime_id: ps.runtime_id,
            state_data: ps.state_data,
            created_at: chrono::Utc::now(),
            state_format: ps.state_format,
        };

        let handle = self
            .contract
            .restore_state(state)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.remember_session(&handle.session_id).await;
        Ok(Response::new(proto::Empty {}))
    }

    async fn health(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::HealthResponse>, Status> {
        let status = self
            .contract
            .health()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(health_to_proto(status)))
    }

    async fn disconnect_mcp(
        &self,
        request: Request<proto::DisconnectMcpRequest>,
    ) -> Result<Response<proto::Empty>, Status> {
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id,
        };

        self.contract
            .disconnect_mcp(&handle, &req.server_name)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::Empty {}))
    }

    async fn pause_session(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::StateResponse>, Status> {
        let session_id = self.current_session_or().await?;
        let handle = contract::SessionHandle { session_id };

        // Pause and then return current state per v2 contract.
        self.contract
            .pause_session(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let state = self
            .contract
            .get_state(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(session_state_to_proto(state)))
    }

    async fn resume_session(
        &self,
        request: Request<proto::StateResponse>,
    ) -> Result<Response<proto::Empty>, Status> {
        let session_id = request.into_inner().session_id;

        let handle = self
            .contract
            .resume_session(&session_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.remember_session(&handle.session_id).await;
        Ok(Response::new(proto::Empty {}))
    }

    // ── OPTIONAL — ADR-V2-001 Accepted (Phase 1) ──
    //
    // EmitEvent is OPTIONAL per ADR-V2-001. Default: delegate to
    // RuntimeContract::emit_event (no-op by default). T1 runtimes
    // can override to POST events to L4's /v1/events/ingest.
    // Core events are already captured by the L4 platform interceptor.
    async fn emit_event(
        &self,
        request: Request<proto::EventStreamEntry>,
    ) -> Result<Response<proto::Empty>, Status> {
        let entry = request.into_inner();
        self.contract
            .emit_event(contract::EventStreamEntry {
                session_id: entry.session_id,
                event_id: entry.event_id,
                event_type: entry.event_type.to_string(),
                payload_json: entry.payload_json,
                timestamp: entry.timestamp,
            })
            .await
            .map_err(|e| Status::internal(format!("emit_event failed: {e}")))?;
        Ok(Response::new(proto::Empty {}))
    }
}
