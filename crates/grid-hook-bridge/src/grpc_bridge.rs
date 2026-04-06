//! GrpcHookBridge — gRPC client to external HookBridge sidecar.

use async_trait::async_trait;
use tonic::transport::Channel;
use tracing::warn;

use crate::hook_proto;
use crate::hook_proto::hook_bridge_service_client::HookBridgeServiceClient;
use crate::traits::*;

/// gRPC client to an external HookBridge sidecar.
pub struct GrpcHookBridge {
    client: HookBridgeServiceClient<Channel>,
}

impl GrpcHookBridge {
    /// Connect to a HookBridge sidecar at the given address.
    pub async fn connect(addr: impl Into<String>) -> anyhow::Result<Self> {
        let client = HookBridgeServiceClient::connect(addr.into()).await?;
        Ok(Self { client })
    }

    fn to_native_decision(d: crate::common_proto::HookDecision) -> HookDecision {
        match d.decision.as_str() {
            "deny" => HookDecision::Deny { reason: d.reason },
            "modify" => HookDecision::Modify {
                transformed_input: serde_json::from_str(&d.modified_input)
                    .unwrap_or(serde_json::Value::Null),
            },
            _ => HookDecision::Allow,
        }
    }
}

#[async_trait]
impl HookBridge for GrpcHookBridge {
    async fn evaluate_pre_tool_call(
        &self,
        session_id: &str,
        tool_name: &str,
        tool_id: &str,
        input: &serde_json::Value,
    ) -> anyhow::Result<HookDecision> {
        let request = hook_proto::HookEvaluateRequest {
            session_id: session_id.into(),
            hook_type: "pre_tool_call".into(),
            tool_name: tool_name.into(),
            tool_id: tool_id.into(),
            input_json: serde_json::to_string(input)?,
            output: String::new(),
            is_error: false,
        };
        let response = self.client.clone().evaluate_hook(request).await?;
        Ok(Self::to_native_decision(response.into_inner()))
    }

    async fn evaluate_post_tool_result(
        &self,
        session_id: &str,
        tool_name: &str,
        tool_id: &str,
        output: &str,
        is_error: bool,
    ) -> anyhow::Result<HookDecision> {
        let request = hook_proto::HookEvaluateRequest {
            session_id: session_id.into(),
            hook_type: "post_tool_result".into(),
            tool_name: tool_name.into(),
            tool_id: tool_id.into(),
            input_json: String::new(),
            output: output.into(),
            is_error,
        };
        let response = self.client.clone().evaluate_hook(request).await?;
        Ok(Self::to_native_decision(response.into_inner()))
    }

    async fn evaluate_stop(&self, session_id: &str) -> anyhow::Result<StopDecision> {
        let request = hook_proto::HookEvaluateRequest {
            session_id: session_id.into(),
            hook_type: "stop".into(),
            tool_name: String::new(),
            tool_id: String::new(),
            input_json: String::new(),
            output: String::new(),
            is_error: false,
        };
        let response = self.client.clone().evaluate_hook(request).await?;
        let decision = response.into_inner();
        match decision.decision.as_str() {
            "deny" => Ok(StopDecision::Continue {
                feedback: decision.reason,
            }),
            _ => Ok(StopDecision::Complete),
        }
    }

    async fn load_policies(&self, _policies: Vec<PolicyRule>) -> anyhow::Result<()> {
        warn!("GrpcHookBridge: load_policies is a no-op — policies are managed by the sidecar");
        Ok(())
    }

    async fn policy_count(&self) -> usize {
        match self
            .client
            .clone()
            .get_policy_summary(hook_proto::PolicySummaryRequest {
                session_id: String::new(),
            })
            .await
        {
            Ok(response) => response.into_inner().total_policies as usize,
            Err(_) => 0,
        }
    }
}
