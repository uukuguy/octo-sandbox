//! gRPC service implementation for EAASP RuntimeService.
//!
//! Maps tonic-generated types ↔ contract types, delegates to RuntimeContract.

use std::pin::Pin;
use std::sync::Arc;

use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use crate::common_proto;
use crate::contract::{self, RuntimeContract};
use crate::proto;
use crate::proto::runtime_service_server::RuntimeService;

/// gRPC service wrapping a RuntimeContract implementation.
pub struct RuntimeGrpcService<C: RuntimeContract> {
    contract: Arc<C>,
}

impl<C: RuntimeContract + 'static> RuntimeGrpcService<C> {
    pub fn new(contract: Arc<C>) -> Self {
        Self { contract }
    }
}

// ── Type conversion helpers ──

fn to_session_payload(p: proto::SessionPayload) -> contract::SessionPayload {
    contract::SessionPayload {
        user_id: p.user_id,
        user_role: p.user_role,
        org_unit: p.org_unit,
        managed_hooks_json: if p.managed_hooks_json.is_empty() {
            None
        } else {
            Some(p.managed_hooks_json)
        },
        quotas: p.quotas,
        context: p.context,
        hook_bridge_url: if p.hook_bridge_url.is_empty() {
            None
        } else {
            Some(p.hook_bridge_url)
        },
        telemetry_endpoint: if p.telemetry_endpoint.is_empty() {
            None
        } else {
            Some(p.telemetry_endpoint)
        },
    }
}

fn to_user_message(m: proto::UserMessage) -> contract::UserMessage {
    contract::UserMessage {
        content: m.content,
        message_type: m.message_type,
        metadata: m.metadata,
    }
}

fn to_skill_content(s: proto::SkillContent) -> contract::SkillContent {
    contract::SkillContent {
        skill_id: s.skill_id,
        name: s.name,
        frontmatter_yaml: s.frontmatter_yaml,
        prose: s.prose,
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

fn hook_decision_to_proto(d: contract::HookDecision) -> common_proto::HookDecision {
    match d {
        contract::HookDecision::Allow => common_proto::HookDecision {
            decision: "allow".into(),
            reason: String::new(),
            modified_input: String::new(),
        },
        contract::HookDecision::Deny { reason } => common_proto::HookDecision {
            decision: "deny".into(),
            reason,
            modified_input: String::new(),
        },
        contract::HookDecision::Modify { transformed_input } => common_proto::HookDecision {
            decision: "modify".into(),
            reason: String::new(),
            modified_input: serde_json::to_string(&transformed_input).unwrap_or_default(),
        },
    }
}

fn stop_decision_to_proto(d: contract::StopDecision) -> common_proto::StopDecision {
    match d {
        contract::StopDecision::Complete => common_proto::StopDecision {
            decision: "complete".into(),
            feedback: String::new(),
        },
        contract::StopDecision::Continue { feedback } => common_proto::StopDecision {
            decision: "continue".into(),
            feedback,
        },
    }
}

fn session_state_to_proto(s: contract::SessionState) -> proto::SessionState {
    proto::SessionState {
        session_id: s.session_id,
        state_data: s.state_data,
        runtime_id: s.runtime_id,
        created_at: s.created_at.to_rfc3339(),
        state_format: s.state_format,
    }
}

fn capability_to_proto(c: contract::CapabilityManifest) -> proto::CapabilityManifest {
    proto::CapabilityManifest {
        runtime_id: c.runtime_id,
        runtime_name: c.runtime_name,
        tier: match c.tier {
            contract::RuntimeTier::Harness => "harness".into(),
            contract::RuntimeTier::Aligned => "aligned".into(),
            contract::RuntimeTier::Framework => "framework".into(),
        },
        model: c.model,
        context_window: c.context_window,
        supported_tools: c.supported_tools,
        native_hooks: c.native_hooks,
        native_mcp: c.native_mcp,
        native_skills: c.native_skills,
        cost: c.cost.map(|c| proto::CostEstimate {
            input_cost_per_1k: c.input_cost_per_1k,
            output_cost_per_1k: c.output_cost_per_1k,
        }),
        metadata: c.metadata,
        requires_hook_bridge: c.requires_hook_bridge,
    }
}

fn telemetry_to_proto(events: Vec<contract::TelemetryEvent>) -> common_proto::TelemetryBatch {
    common_proto::TelemetryBatch {
        events: events
            .into_iter()
            .map(|e| common_proto::TelemetryEvent {
                session_id: e.session_id,
                runtime_id: e.runtime_id,
                user_id: e.user_id.unwrap_or_default(),
                event_type: e.event_type,
                timestamp: e.timestamp.to_rfc3339(),
                payload_json: serde_json::to_string(&e.payload).unwrap_or_default(),
                resource_usage: Some(common_proto::ResourceUsage {
                    input_tokens: e.resource_usage.input_tokens,
                    output_tokens: e.resource_usage.output_tokens,
                    compute_ms: e.resource_usage.compute_ms,
                }),
            })
            .collect(),
    }
}

fn health_to_proto(h: contract::HealthStatus) -> proto::HealthStatus {
    proto::HealthStatus {
        healthy: h.healthy,
        runtime_id: h.runtime_id,
        checks: h.checks,
    }
}

// ── gRPC Service Implementation ──

type SendStream = Pin<Box<dyn Stream<Item = Result<proto::ResponseChunk, Status>> + Send>>;

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
            .initialize(to_session_payload(payload))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::InitializeResponse {
            session_id: handle.session_id,
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
            Ok(proto::ResponseChunk {
                chunk_type: chunk.chunk_type,
                content: chunk.content,
                tool_name: chunk.tool_name.unwrap_or_default(),
                tool_id: chunk.tool_id.unwrap_or_default(),
                is_error: chunk.is_error,
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
        request: Request<common_proto::ToolCallEvent>,
    ) -> Result<Response<common_proto::HookDecision>, Status> {
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

        Ok(Response::new(hook_decision_to_proto(decision)))
    }

    async fn on_tool_result(
        &self,
        request: Request<common_proto::ToolResultEvent>,
    ) -> Result<Response<common_proto::HookDecision>, Status> {
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

        Ok(Response::new(hook_decision_to_proto(decision)))
    }

    async fn on_stop(
        &self,
        request: Request<common_proto::StopRequest>,
    ) -> Result<Response<common_proto::StopDecision>, Status> {
        let handle = contract::SessionHandle {
            session_id: request.into_inner().session_id,
        };

        let decision = self
            .contract
            .on_stop(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(stop_decision_to_proto(decision)))
    }

    async fn get_state(
        &self,
        request: Request<proto::GetStateRequest>,
    ) -> Result<Response<proto::SessionState>, Status> {
        let handle = contract::SessionHandle {
            session_id: request.into_inner().session_id,
        };

        let state = self
            .contract
            .get_state(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(session_state_to_proto(state)))
    }

    async fn restore_state(
        &self,
        request: Request<proto::SessionState>,
    ) -> Result<Response<proto::InitializeResponse>, Status> {
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

        Ok(Response::new(proto::InitializeResponse {
            session_id: handle.session_id,
        }))
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
        request: Request<proto::EmitTelemetryRequest>,
    ) -> Result<Response<common_proto::TelemetryBatch>, Status> {
        let handle = contract::SessionHandle {
            session_id: request.into_inner().session_id,
        };

        let events = self
            .contract
            .emit_telemetry(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(telemetry_to_proto(events)))
    }

    async fn get_capabilities(
        &self,
        _request: Request<common_proto::Empty>,
    ) -> Result<Response<proto::CapabilityManifest>, Status> {
        let manifest = self.contract.get_capabilities();
        Ok(Response::new(capability_to_proto(manifest)))
    }

    async fn terminate(
        &self,
        request: Request<proto::TerminateRequest>,
    ) -> Result<Response<proto::TerminateResponse>, Status> {
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id.clone(),
        };

        let final_telemetry = self
            .contract
            .emit_telemetry(&handle)
            .await
            .unwrap_or_default();

        self.contract
            .terminate(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::TerminateResponse {
            success: true,
            final_telemetry: Some(telemetry_to_proto(final_telemetry)),
        }))
    }

    async fn health(
        &self,
        _request: Request<common_proto::Empty>,
    ) -> Result<Response<proto::HealthStatus>, Status> {
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
    ) -> Result<Response<proto::DisconnectMcpResponse>, Status> {
        let req = request.into_inner();
        let handle = contract::SessionHandle {
            session_id: req.session_id,
        };

        self.contract
            .disconnect_mcp(&handle, &req.server_name)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::DisconnectMcpResponse {
            success: true,
        }))
    }

    async fn pause_session(
        &self,
        request: Request<proto::PauseRequest>,
    ) -> Result<Response<proto::PauseResponse>, Status> {
        let handle = contract::SessionHandle {
            session_id: request.into_inner().session_id,
        };

        self.contract
            .pause_session(&handle)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::PauseResponse { success: true }))
    }

    async fn resume_session(
        &self,
        request: Request<proto::ResumeRequest>,
    ) -> Result<Response<proto::ResumeResponse>, Status> {
        let session_id = request.into_inner().session_id;

        let handle = self
            .contract
            .resume_session(&session_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::ResumeResponse {
            success: true,
            session_id: handle.session_id,
        }))
    }
}
