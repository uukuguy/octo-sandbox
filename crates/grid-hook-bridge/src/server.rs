//! HookBridge gRPC server — exposes HookBridge trait as gRPC service.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, warn};

use crate::common_proto;
use crate::hook_proto;
use crate::hook_proto::hook_bridge_service_server::HookBridgeService;
use crate::traits::*;

/// gRPC server wrapping a HookBridge implementation.
pub struct HookBridgeGrpcServer<B: HookBridge> {
    bridge: Arc<B>,
}

impl<B: HookBridge + 'static> HookBridgeGrpcServer<B> {
    pub fn new(bridge: Arc<B>) -> Self {
        Self { bridge }
    }
}

type StreamHooksResponseStream = ReceiverStream<Result<hook_proto::HookResponse, Status>>;

#[tonic::async_trait]
impl<B: HookBridge + 'static> HookBridgeService for HookBridgeGrpcServer<B> {
    type StreamHooksStream = StreamHooksResponseStream;

    async fn stream_hooks(
        &self,
        request: Request<Streaming<hook_proto::HookEvent>>,
    ) -> Result<Response<Self::StreamHooksStream>, Status> {
        let bridge = self.bridge.clone();
        let mut in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            while let Ok(Some(event)) = in_stream.message().await {
                let request_id = event.request_id.clone();
                let session_id = event.session_id.clone();

                let response = match event.event {
                    Some(hook_proto::hook_event::Event::PreToolCall(hook)) => {
                        let input = serde_json::from_str(&hook.input_json)
                            .unwrap_or(serde_json::Value::Null);
                        match bridge
                            .evaluate_pre_tool_call(
                                &session_id,
                                &hook.tool_name,
                                &hook.tool_id,
                                &input,
                            )
                            .await
                        {
                            Ok(d) => hook_proto::HookResponse {
                                request_id,
                                response: Some(hook_proto::hook_response::Response::Decision(
                                    decision_to_proto(d),
                                )),
                            },
                            Err(e) => error_response(&request_id, &e.to_string()),
                        }
                    }
                    Some(hook_proto::hook_event::Event::PostToolResult(hook)) => {
                        match bridge
                            .evaluate_post_tool_result(
                                &session_id,
                                &hook.tool_name,
                                &hook.tool_id,
                                &hook.output,
                                hook.is_error,
                            )
                            .await
                        {
                            Ok(d) => hook_proto::HookResponse {
                                request_id,
                                response: Some(hook_proto::hook_response::Response::Decision(
                                    decision_to_proto(d),
                                )),
                            },
                            Err(e) => error_response(&request_id, &e.to_string()),
                        }
                    }
                    Some(hook_proto::hook_event::Event::Stop(_)) => {
                        match bridge.evaluate_stop(&session_id).await {
                            Ok(d) => hook_proto::HookResponse {
                                request_id,
                                response: Some(
                                    hook_proto::hook_response::Response::StopDecision(
                                        stop_decision_to_proto(d),
                                    ),
                                ),
                            },
                            Err(e) => error_response(&request_id, &e.to_string()),
                        }
                    }
                    Some(hook_proto::hook_event::Event::SessionStart(_)) => {
                        debug!(session_id = %session_id, "Session start hook received");
                        hook_proto::HookResponse {
                            request_id,
                            response: Some(hook_proto::hook_response::Response::Decision(
                                decision_to_proto(HookDecision::Allow),
                            )),
                        }
                    }
                    Some(hook_proto::hook_event::Event::SessionEnd(_)) => {
                        debug!(session_id = %session_id, "Session end hook received");
                        hook_proto::HookResponse {
                            request_id,
                            response: Some(hook_proto::hook_response::Response::Decision(
                                decision_to_proto(HookDecision::Allow),
                            )),
                        }
                    }
                    None => {
                        warn!("Empty hook event received");
                        error_response(&request_id, "empty event")
                    }
                };

                if tx.send(Ok(response)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn evaluate_hook(
        &self,
        request: Request<hook_proto::HookEvaluateRequest>,
    ) -> Result<Response<common_proto::HookDecision>, Status> {
        let req = request.into_inner();

        let decision = match req.hook_type.as_str() {
            "pre_tool_call" => {
                let input = serde_json::from_str(&req.input_json)
                    .unwrap_or(serde_json::Value::Null);
                self.bridge
                    .evaluate_pre_tool_call(
                        &req.session_id,
                        &req.tool_name,
                        &req.tool_id,
                        &input,
                    )
                    .await
            }
            "post_tool_result" => {
                self.bridge
                    .evaluate_post_tool_result(
                        &req.session_id,
                        &req.tool_name,
                        &req.tool_id,
                        &req.output,
                        req.is_error,
                    )
                    .await
            }
            "stop" => {
                return match self.bridge.evaluate_stop(&req.session_id).await {
                    Ok(StopDecision::Complete) => {
                        Ok(Response::new(common_proto::HookDecision {
                            decision: "allow".into(),
                            reason: String::new(),
                            modified_input: String::new(),
                        }))
                    }
                    Ok(StopDecision::Continue { feedback }) => {
                        Ok(Response::new(common_proto::HookDecision {
                            decision: "deny".into(),
                            reason: feedback,
                            modified_input: String::new(),
                        }))
                    }
                    Err(e) => Err(Status::internal(e.to_string())),
                };
            }
            other => {
                return Err(Status::invalid_argument(format!(
                    "unknown hook_type: {other}"
                )));
            }
        };

        match decision {
            Ok(d) => Ok(Response::new(decision_to_proto(d))),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn report_telemetry(
        &self,
        _request: Request<common_proto::TelemetryBatch>,
    ) -> Result<Response<hook_proto::TelemetryAck>, Status> {
        Ok(Response::new(hook_proto::TelemetryAck {
            accepted: 1,
            rejected: 0,
        }))
    }

    async fn get_policy_summary(
        &self,
        _request: Request<hook_proto::PolicySummaryRequest>,
    ) -> Result<Response<hook_proto::PolicySummary>, Status> {
        let count = self.bridge.policy_count().await;
        Ok(Response::new(hook_proto::PolicySummary {
            total_policies: count as u32,
            policies: vec![],
        }))
    }
}

fn decision_to_proto(d: HookDecision) -> common_proto::HookDecision {
    match d {
        HookDecision::Allow => common_proto::HookDecision {
            decision: "allow".into(),
            reason: String::new(),
            modified_input: String::new(),
        },
        HookDecision::Deny { reason } => common_proto::HookDecision {
            decision: "deny".into(),
            reason,
            modified_input: String::new(),
        },
        HookDecision::Modify { transformed_input } => common_proto::HookDecision {
            decision: "modify".into(),
            reason: String::new(),
            modified_input: serde_json::to_string(&transformed_input).unwrap_or_default(),
        },
    }
}

fn stop_decision_to_proto(d: StopDecision) -> common_proto::StopDecision {
    match d {
        StopDecision::Complete => common_proto::StopDecision {
            decision: "complete".into(),
            feedback: String::new(),
        },
        StopDecision::Continue { feedback } => common_proto::StopDecision {
            decision: "continue".into(),
            feedback,
        },
    }
}

fn error_response(request_id: &str, message: &str) -> hook_proto::HookResponse {
    hook_proto::HookResponse {
        request_id: request_id.into(),
        response: Some(hook_proto::hook_response::Response::Error(
            hook_proto::ErrorResponse {
                code: "INTERNAL".into(),
                message: message.into(),
            },
        )),
    }
}
