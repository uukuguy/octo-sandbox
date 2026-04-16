//! gRPC service implementation for eaasp-goose-runtime.
//!
//! Wraps GooseAdapter in a tonic RuntimeService.
//! Initialize + Terminate are reasonably complete; Send and all other methods
//! return shape-correct stubs pending real ACP wiring (T4/T5).

use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use crate::goose_adapter::{GooseAdapter, SessionConfig};
use crate::proto;
use crate::proto::runtime_service_server::RuntimeService;

type SendStream = Pin<Box<dyn Stream<Item = Result<proto::SendResponse, Status>> + Send>>;

/// gRPC service wrapping a GooseAdapter.
pub struct GooseRuntimeService {
    adapter: Arc<GooseAdapter>,
    /// Last initialized session — implicit context for Empty-request methods.
    current_session: Arc<RwLock<Option<String>>>,
    deployment_mode: String,
}

impl GooseRuntimeService {
    pub fn new(adapter: Arc<GooseAdapter>, deployment_mode: impl Into<String>) -> Self {
        Self {
            adapter,
            current_session: Arc::new(RwLock::new(None)),
            deployment_mode: deployment_mode.into(),
        }
    }

    async fn remember_session(&self, session_id: &str) {
        *self.current_session.write().await = Some(session_id.to_string());
    }

    async fn current_session_or_err(&self) -> Result<String, Status> {
        self.current_session
            .read()
            .await
            .clone()
            .ok_or_else(|| Status::failed_precondition("no active session; call Initialize first"))
    }
}

#[tonic::async_trait]
impl RuntimeService for GooseRuntimeService {
    type SendStream = SendStream;

    async fn initialize(
        &self,
        _request: Request<proto::InitializeRequest>,
    ) -> Result<Response<proto::InitializeResponse>, Status> {
        let sid = self
            .adapter
            .start_session(SessionConfig::default())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.remember_session(&sid).await;

        Ok(Response::new(proto::InitializeResponse {
            session_id: sid,
            runtime_id: "eaasp-goose-runtime".to_string(),
        }))
    }

    async fn send(
        &self,
        request: Request<proto::SendRequest>,
    ) -> Result<Response<Self::SendStream>, Status> {
        let req = request.into_inner();
        let content = req.message.map(|m| m.content).unwrap_or_default();
        tracing::debug!(session_id = %req.session_id, content_len = content.len(), "Send stub");

        // Stub: return a single "done" chunk. Real ACP forwarding lands in T4.
        let chunk = proto::SendResponse {
            chunk_type: "done".to_string(),
            content: String::new(),
            tool_name: String::new(),
            tool_id: String::new(),
            is_error: false,
            error: None,
        };
        let stream = tokio_stream::once(Ok(chunk));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn load_skill(
        &self,
        request: Request<proto::LoadSkillRequest>,
    ) -> Result<Response<proto::LoadSkillResponse>, Status> {
        let req = request.into_inner();
        let skill_id = req.skill.map(|s| s.skill_id).unwrap_or_default();
        tracing::debug!(skill_id = %skill_id, "LoadSkill stub");
        Ok(Response::new(proto::LoadSkillResponse {
            success: true,
            error: String::new(),
        }))
    }

    async fn on_tool_call(
        &self,
        request: Request<proto::ToolCallEvent>,
    ) -> Result<Response<proto::ToolCallAck>, Status> {
        let req = request.into_inner();
        tracing::debug!(tool_name = %req.tool_name, "OnToolCall stub — allow");
        Ok(Response::new(proto::ToolCallAck {
            decision: "allow".to_string(),
            mutated_input_json: String::new(),
            reason: String::new(),
        }))
    }

    async fn on_tool_result(
        &self,
        request: Request<proto::ToolResultEvent>,
    ) -> Result<Response<proto::ToolResultAck>, Status> {
        let req = request.into_inner();
        tracing::debug!(tool_name = %req.tool_name, "OnToolResult stub — allow");
        Ok(Response::new(proto::ToolResultAck {
            decision: "allow".to_string(),
            reason: String::new(),
        }))
    }

    async fn on_stop(
        &self,
        request: Request<proto::StopEvent>,
    ) -> Result<Response<proto::StopAck>, Status> {
        let req = request.into_inner();
        tracing::debug!(session_id = %req.session_id, reason = %req.reason, "OnStop stub — allow");
        Ok(Response::new(proto::StopAck {
            decision: "allow".to_string(),
            reason: String::new(),
        }))
    }

    async fn get_state(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::StateResponse>, Status> {
        let session_id = self.current_session_or_err().await?;
        Ok(Response::new(proto::StateResponse {
            session_id,
            state_data: vec![],
            runtime_id: "eaasp-goose-runtime".to_string(),
            state_format: "goose-stub-v1".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn connect_mcp(
        &self,
        request: Request<proto::ConnectMcpRequest>,
    ) -> Result<Response<proto::ConnectMcpResponse>, Status> {
        let req = request.into_inner();
        let names: Vec<String> = req.servers.iter().map(|s| s.name.clone()).collect();
        tracing::debug!(count = names.len(), "ConnectMCP stub — all accepted");
        Ok(Response::new(proto::ConnectMcpResponse {
            success: true,
            connected: names,
            failed: vec![],
        }))
    }

    async fn emit_telemetry(
        &self,
        _request: Request<proto::TelemetryRequest>,
    ) -> Result<Response<proto::Empty>, Status> {
        // fire-and-forget
        Ok(Response::new(proto::Empty {}))
    }

    async fn get_capabilities(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::Capabilities>, Status> {
        use proto::capabilities::CredentialMode;
        Ok(Response::new(proto::Capabilities {
            runtime_id: "eaasp-goose-runtime".to_string(),
            model: String::new(),
            context_window: 0,
            tools: vec![],
            supports_native_hooks: false,
            supports_native_mcp: true,
            supports_native_skills: false,
            cost_per_1k_tokens: 0.0,
            credential_mode: CredentialMode::Direct as i32,
            strengths: vec!["goose-native-mcp".to_string()],
            limitations: vec!["stub-send".to_string()],
            tier: "framework".to_string(),
            deployment_mode: self.deployment_mode.clone(),
        }))
    }

    async fn terminate(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::Empty>, Status> {
        let sid = self.current_session_or_err().await?;
        self.adapter
            .close_session(&sid)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        *self.current_session.write().await = None;
        Ok(Response::new(proto::Empty {}))
    }

    async fn restore_state(
        &self,
        request: Request<proto::StateResponse>,
    ) -> Result<Response<proto::Empty>, Status> {
        let ps = request.into_inner();
        self.remember_session(&ps.session_id).await;
        tracing::debug!(session_id = %ps.session_id, "RestoreState stub");
        Ok(Response::new(proto::Empty {}))
    }

    async fn health(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::HealthResponse>, Status> {
        Ok(Response::new(proto::HealthResponse {
            healthy: true,
            runtime_id: "eaasp-goose-runtime".to_string(),
            checks: std::collections::HashMap::new(),
        }))
    }

    async fn disconnect_mcp(
        &self,
        request: Request<proto::DisconnectMcpRequest>,
    ) -> Result<Response<proto::Empty>, Status> {
        let req = request.into_inner();
        tracing::debug!(server_name = %req.server_name, "DisconnectMcp stub");
        Ok(Response::new(proto::Empty {}))
    }

    async fn pause_session(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::StateResponse>, Status> {
        let session_id = self.current_session_or_err().await?;
        tracing::debug!(session_id = %session_id, "PauseSession stub");
        Ok(Response::new(proto::StateResponse {
            session_id,
            state_data: vec![],
            runtime_id: "eaasp-goose-runtime".to_string(),
            state_format: "goose-stub-v1".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn resume_session(
        &self,
        request: Request<proto::StateResponse>,
    ) -> Result<Response<proto::Empty>, Status> {
        let ps = request.into_inner();
        self.remember_session(&ps.session_id).await;
        tracing::debug!(session_id = %ps.session_id, "ResumeSession stub");
        Ok(Response::new(proto::Empty {}))
    }

    async fn emit_event(
        &self,
        request: Request<proto::EventStreamEntry>,
    ) -> Result<Response<proto::Empty>, Status> {
        let entry = request.into_inner();
        tracing::debug!(
            session_id = %entry.session_id,
            event_id = %entry.event_id,
            "EmitEvent stub"
        );
        Ok(Response::new(proto::Empty {}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> GooseRuntimeService {
        let adapter = Arc::new(GooseAdapter::with_mode("shared"));
        GooseRuntimeService::new(adapter, "shared")
    }

    #[tokio::test]
    async fn test_no_active_session_err() {
        let svc = make_service();
        let result = svc.current_session_or_err().await;
        assert!(result.is_err(), "should fail with no session");
        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    }

    #[tokio::test]
    async fn test_capabilities_tier_is_framework() {
        let svc = make_service();
        let resp = svc
            .get_capabilities(Request::new(proto::Empty {}))
            .await
            .expect("get_capabilities should succeed");
        assert_eq!(resp.into_inner().tier, "framework");
    }
}
