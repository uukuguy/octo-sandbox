//! gRPC service implementation for eaasp-claw-code-runtime.

use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use crate::adapter::{ClawCodeAdapter, SessionConfig};
use crate::proto;
use crate::proto::runtime_service_server::RuntimeService;
use crate::ultra_worker::UltraWorkerEvent;

type SendStream = Pin<Box<dyn Stream<Item = Result<proto::SendResponse, Status>> + Send>>;

pub struct ClawCodeRuntimeService {
    adapter: Arc<ClawCodeAdapter>,
    current_session: Arc<RwLock<Option<String>>>,
    deployment_mode: String,
}

impl ClawCodeRuntimeService {
    pub fn new(adapter: Arc<ClawCodeAdapter>, deployment_mode: impl Into<String>) -> Self {
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
impl RuntimeService for ClawCodeRuntimeService {
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
            runtime_id: "eaasp-claw-code-runtime".to_string(),
        }))
    }

    async fn send(
        &self,
        request: Request<proto::SendRequest>,
    ) -> Result<Response<Self::SendStream>, Status> {
        use tokio::sync::mpsc;
        use tokio_stream::wrappers::ReceiverStream;

        let req = request.into_inner();
        let session_id = req.session_id.clone();
        let content = req.message.map(|m| m.content).unwrap_or_default();

        self.adapter
            .send_message(&session_id, &content)
            .await
            .map_err(|e| Status::internal(format!("send_message failed: {e}")))?;

        let (tx, rx) = mpsc::channel::<Result<proto::SendResponse, Status>>(32);
        let adapter = self.adapter.clone();
        let sid = session_id.clone();

        tokio::spawn(async move {
            loop {
                match adapter.next_event(&sid).await {
                    Ok(Some(event)) => {
                        let (resp, should_break) = match event {
                            UltraWorkerEvent::Chunk { text, .. } => (
                                proto::SendResponse {
                                    chunk_type: "chunk".to_string(),
                                    content: text,
                                    tool_name: String::new(),
                                    tool_id: String::new(),
                                    is_error: false,
                                    error: None,
                                },
                                false,
                            ),
                            UltraWorkerEvent::ToolCall { tool_name, tool_id, input_json, .. } => (
                                proto::SendResponse {
                                    chunk_type: "tool_call".to_string(),
                                    content: input_json,
                                    tool_name,
                                    tool_id,
                                    is_error: false,
                                    error: None,
                                },
                                false,
                            ),
                            UltraWorkerEvent::Stop { reason, .. } => {
                                let _ = tx
                                    .send(Ok(proto::SendResponse {
                                        chunk_type: "done".to_string(),
                                        content: reason,
                                        tool_name: String::new(),
                                        tool_id: String::new(),
                                        is_error: false,
                                        error: None,
                                    }))
                                    .await;
                                break;
                            }
                            UltraWorkerEvent::Error { message, .. } => {
                                let _ = tx.send(Err(Status::internal(message))).await;
                                break;
                            }
                            UltraWorkerEvent::Unknown { .. } => continue,
                        };
                        if tx.send(Ok(resp)).await.is_err() || should_break {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = tx
                            .send(Ok(proto::SendResponse {
                                chunk_type: "done".to_string(),
                                content: String::new(),
                                tool_name: String::new(),
                                tool_id: String::new(),
                                is_error: false,
                                error: None,
                            }))
                            .await;
                        break;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(Status::internal(e.to_string()))).await;
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn load_skill(
        &self,
        _request: Request<proto::LoadSkillRequest>,
    ) -> Result<Response<proto::LoadSkillResponse>, Status> {
        Ok(Response::new(proto::LoadSkillResponse { success: true, error: String::new() }))
    }

    async fn on_tool_call(
        &self,
        request: Request<proto::ToolCallEvent>,
    ) -> Result<Response<proto::ToolCallAck>, Status> {
        tracing::debug!(tool_name = %request.into_inner().tool_name, "OnToolCall stub — allow");
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
        tracing::debug!(tool_name = %request.into_inner().tool_name, "OnToolResult stub — allow");
        Ok(Response::new(proto::ToolResultAck {
            decision: "allow".to_string(),
            reason: String::new(),
        }))
    }

    async fn on_stop(
        &self,
        request: Request<proto::StopEvent>,
    ) -> Result<Response<proto::StopAck>, Status> {
        tracing::debug!(session_id = %request.into_inner().session_id, "OnStop stub — allow");
        Ok(Response::new(proto::StopAck { decision: "allow".to_string(), reason: String::new() }))
    }

    async fn get_state(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::StateResponse>, Status> {
        let session_id = self.current_session_or_err().await?;
        Ok(Response::new(proto::StateResponse {
            session_id,
            state_data: vec![],
            runtime_id: "eaasp-claw-code-runtime".to_string(),
            state_format: "claw-code-stub-v1".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn connect_mcp(
        &self,
        request: Request<proto::ConnectMcpRequest>,
    ) -> Result<Response<proto::ConnectMcpResponse>, Status> {
        let names: Vec<String> =
            request.into_inner().servers.iter().map(|s| s.name.clone()).collect();
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
        Ok(Response::new(proto::Empty {}))
    }

    async fn get_capabilities(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::Capabilities>, Status> {
        use proto::capabilities::CredentialMode;
        Ok(Response::new(proto::Capabilities {
            runtime_id: "eaasp-claw-code-runtime".to_string(),
            model: String::new(),
            context_window: 0,
            tools: vec![],
            supports_native_hooks: false,
            supports_native_mcp: false,
            supports_native_skills: false,
            cost_per_1k_tokens: 0.0,
            credential_mode: CredentialMode::Direct as i32,
            strengths: vec!["claw-code-ultra-workers".to_string()],
            limitations: vec!["stub-send".to_string(), "stub-hooks".to_string()],
            tier: "aligned".to_string(),
            deployment_mode: self.deployment_mode.clone(),
        }))
    }

    async fn terminate(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::Empty>, Status> {
        if let Ok(sid) = self.current_session_or_err().await {
            let _ = self.adapter.stop_session(&sid).await;
            *self.current_session.write().await = None;
        }
        Ok(Response::new(proto::Empty {}))
    }

    async fn restore_state(
        &self,
        request: Request<proto::StateResponse>,
    ) -> Result<Response<proto::Empty>, Status> {
        self.remember_session(&request.into_inner().session_id).await;
        Ok(Response::new(proto::Empty {}))
    }

    async fn health(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::HealthResponse>, Status> {
        Ok(Response::new(proto::HealthResponse {
            healthy: true,
            runtime_id: "eaasp-claw-code-runtime".to_string(),
            checks: std::collections::HashMap::new(),
        }))
    }

    async fn disconnect_mcp(
        &self,
        _request: Request<proto::DisconnectMcpRequest>,
    ) -> Result<Response<proto::Empty>, Status> {
        Ok(Response::new(proto::Empty {}))
    }

    async fn pause_session(
        &self,
        _request: Request<proto::Empty>,
    ) -> Result<Response<proto::StateResponse>, Status> {
        let session_id = self.current_session_or_err().await?;
        Ok(Response::new(proto::StateResponse {
            session_id,
            state_data: vec![],
            runtime_id: "eaasp-claw-code-runtime".to_string(),
            state_format: "claw-code-stub-v1".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn resume_session(
        &self,
        request: Request<proto::StateResponse>,
    ) -> Result<Response<proto::Empty>, Status> {
        self.remember_session(&request.into_inner().session_id).await;
        Ok(Response::new(proto::Empty {}))
    }

    async fn emit_event(
        &self,
        _request: Request<proto::EventStreamEntry>,
    ) -> Result<Response<proto::Empty>, Status> {
        Ok(Response::new(proto::Empty {}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> ClawCodeRuntimeService {
        let adapter = Arc::new(ClawCodeAdapter::with_mode("shared"));
        ClawCodeRuntimeService::new(adapter, "shared")
    }

    #[tokio::test]
    async fn test_no_active_session_err() {
        let svc = make_service();
        let result = svc.current_session_or_err().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::FailedPrecondition);
    }

    #[tokio::test]
    async fn test_capabilities_tier_is_aligned() {
        let svc = make_service();
        let resp = svc
            .get_capabilities(Request::new(proto::Empty {}))
            .await
            .unwrap();
        assert_eq!(resp.into_inner().tier, "aligned");
    }

    #[tokio::test]
    async fn test_health_returns_true() {
        let svc = make_service();
        let resp = svc.health(Request::new(proto::Empty {})).await.unwrap();
        assert!(resp.into_inner().healthy);
    }
}
