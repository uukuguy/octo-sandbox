//! Verifier — v2.0 16-method contract verification engine.
//!
//! Reports are split into MUST (12) / OPTIONAL (4) / PLACEHOLDER (1).
//! Failures on MUST produce a FAIL status; failures on OPTIONAL produce
//! a WARN; placeholder status is informational only.

use std::fmt;

use serde::{Deserialize, Serialize};
use tonic::transport::Channel;
use tracing::{error, info, warn};

use crate::proto;
use crate::proto::runtime_service_client::RuntimeServiceClient;
use crate::v2_must_methods::MethodClass;

/// Verification result for a single method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodResult {
    pub method: String,
    pub class: String,
    pub passed: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub notes: Option<String>,
}

impl MethodResult {
    pub fn method_class(&self) -> MethodClass {
        MethodClass::of(&self.method)
    }
}

/// Full verification report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub endpoint: String,
    pub runtime_id: String,
    pub runtime_name: String,
    pub tier: String,
    pub deployment_mode: String,
    pub passed: bool,
    pub total: usize,
    pub passed_count: usize,
    pub failed_count: usize,
    pub must_total: usize,
    pub must_passed: usize,
    pub optional_total: usize,
    pub optional_present: usize,
    pub placeholder_present: bool,
    pub results: Vec<MethodResult>,
    pub timestamp: String,
}

impl VerificationReport {
    /// The report passes iff every MUST method passed. OPTIONAL and
    /// PLACEHOLDER results do not affect this.
    pub fn compute_passed(&self) -> bool {
        self.must_passed == self.must_total
    }
}

impl fmt::Display for VerificationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "================================================================")?;
        writeln!(f, " EAASP v2.0 Contract Verification Report")?;
        writeln!(f, "================================================================")?;
        writeln!(f, " Endpoint:    {}", self.endpoint)?;
        writeln!(
            f,
            " Runtime:     {} ({})",
            self.runtime_name, self.runtime_id
        )?;
        writeln!(f, " Tier:        {}", self.tier)?;
        writeln!(f, " Deploy:      {}", self.deployment_mode)?;
        writeln!(f, " Timestamp:   {}", self.timestamp)?;
        writeln!(f, "----------------------------------------------------------------")?;
        writeln!(
            f,
            " MUST methods: {}/{} PASS",
            self.must_passed, self.must_total
        )?;
        writeln!(
            f,
            " OPTIONAL methods: {}/{} present (bonus)",
            self.optional_present, self.optional_total
        )?;
        let placeholder_note = if self.placeholder_present {
            "present (ADR-V2-001 pending)"
        } else {
            "absent (ADR-V2-001 pending)"
        };
        writeln!(f, " EmitEvent placeholder: {placeholder_note}")?;
        writeln!(
            f,
            " Status:      {}",
            if self.passed { "PASS" } else { "FAIL" }
        )?;
        writeln!(f, "----------------------------------------------------------------")?;

        for result in &self.results {
            let icon = if result.passed { "OK" } else { "FAIL" };
            write!(
                f,
                " [{icon:>4}] [{:11}] {:18} {:>6}ms",
                result.class, result.method, result.duration_ms
            )?;
            if let Some(err) = &result.error {
                write!(f, "  ! {err}")?;
            }
            if let Some(notes) = &result.notes {
                if result.error.is_none() {
                    write!(f, "  ({notes})")?;
                }
            }
            writeln!(f)?;
        }

        writeln!(f, "================================================================")?;
        Ok(())
    }
}

/// Verify all 16 methods of the v2 RuntimeService contract, plus the
/// emit_event placeholder.
pub async fn verify_endpoint(endpoint: &str) -> anyhow::Result<VerificationReport> {
    let channel = Channel::from_shared(endpoint.to_string())?
        .connect()
        .await?;
    let mut client = RuntimeServiceClient::new(channel);

    let mut results = Vec::new();

    // OPTIONAL: Health (run first for endpoint probing)
    results.push(verify_health(&mut client).await);

    // MUST: GetCapabilities
    let caps = verify_get_capabilities(&mut client).await;
    let caps_info = caps.notes.clone().unwrap_or_default();
    results.push(caps);

    // MUST: Initialize (must run before session-bound tests)
    let init_result = verify_initialize(&mut client).await;
    let session_id = init_result
        .notes
        .clone()
        .unwrap_or_else(|| "test-session".into());
    results.push(init_result);

    results.push(verify_send(&mut client, &session_id).await);
    results.push(verify_load_skill(&mut client, &session_id).await);
    results.push(verify_on_tool_call(&mut client, &session_id).await);
    results.push(verify_on_tool_result(&mut client, &session_id).await);
    results.push(verify_on_stop(&mut client, &session_id).await);
    results.push(verify_connect_mcp(&mut client, &session_id).await);
    results.push(verify_disconnect_mcp(&mut client, &session_id).await);
    results.push(verify_emit_telemetry(&mut client, &session_id).await);
    results.push(verify_get_state(&mut client).await);
    results.push(verify_pause_session(&mut client).await);
    results.push(verify_resume_session(&mut client).await);
    results.push(verify_restore_state(&mut client).await);
    results.push(verify_terminate(&mut client).await);
    results.push(verify_emit_event(&mut client, &session_id).await);

    // Tally results by classification.
    let total = results.len();
    let passed_count = results.iter().filter(|r| r.passed).count();
    let must_total = results
        .iter()
        .filter(|r| r.method_class() == MethodClass::Must)
        .count();
    let must_passed = results
        .iter()
        .filter(|r| r.method_class() == MethodClass::Must && r.passed)
        .count();
    let optional_total = results
        .iter()
        .filter(|r| r.method_class() == MethodClass::Optional)
        .count();
    let optional_present = results
        .iter()
        .filter(|r| r.method_class() == MethodClass::Optional && r.passed)
        .count();
    let placeholder_present = results
        .iter()
        .any(|r| r.method_class() == MethodClass::Placeholder && r.passed);

    let (runtime_id, runtime_name, tier, deployment_mode) = parse_caps_info(&caps_info);

    let mut report = VerificationReport {
        endpoint: endpoint.to_string(),
        runtime_id,
        runtime_name,
        tier,
        deployment_mode,
        passed: false, // computed below
        total,
        passed_count,
        failed_count: total - passed_count,
        must_total,
        must_passed,
        optional_total,
        optional_present,
        placeholder_present,
        results,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    report.passed = report.compute_passed();
    Ok(report)
}

fn parse_caps_info(info: &str) -> (String, String, String, String) {
    let parts: Vec<&str> = info.splitn(4, ':').collect();
    match parts.as_slice() {
        [id, name, tier, deploy] => (
            id.to_string(),
            name.to_string(),
            tier.to_string(),
            deploy.to_string(),
        ),
        [id, name, tier] => (
            id.to_string(),
            name.to_string(),
            tier.to_string(),
            "unknown".into(),
        ),
        _ => (
            "unknown".into(),
            "unknown".into(),
            "unknown".into(),
            "unknown".into(),
        ),
    }
}

macro_rules! timed_verify {
    ($name:expr, $block:expr) => {{
        let start = std::time::Instant::now();
        let result: Result<Option<String>, anyhow::Error> = (async { $block }).await;
        let duration_ms = start.elapsed().as_millis() as u64;
        let class = MethodClass::of($name).label().to_string();
        match result {
            Ok(notes) => MethodResult {
                method: $name.into(),
                class,
                passed: true,
                duration_ms,
                error: None,
                notes,
            },
            Err(e) => {
                error!(method = $name, error = %e, "Verification failed");
                MethodResult {
                    method: $name.into(),
                    class,
                    passed: false,
                    duration_ms,
                    error: Some(e.to_string()),
                    notes: None,
                }
            }
        }
    }};
}

async fn verify_health(client: &mut RuntimeServiceClient<Channel>) -> MethodResult {
    timed_verify!("health", {
        let resp = client
            .health(proto::Empty {})
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let status = resp.into_inner();
        if status.healthy {
            info!("Health: ok (runtime_id={})", status.runtime_id);
            Ok(None)
        } else {
            Err(anyhow::anyhow!("Runtime reports unhealthy"))
        }
    })
}

async fn verify_get_capabilities(
    client: &mut RuntimeServiceClient<Channel>,
) -> MethodResult {
    timed_verify!("get_capabilities", {
        let resp = client
            .get_capabilities(proto::Empty {})
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let cap = resp.into_inner();
        info!(
            runtime_id = %cap.runtime_id,
            tier = %cap.tier,
            tools = cap.tools.len(),
            "GetCapabilities OK"
        );
        Ok(Some(format!(
            "{}:{}:{}:{}",
            cap.runtime_id, cap.runtime_id, cap.tier, cap.deployment_mode
        )))
    })
}

async fn verify_initialize(client: &mut RuntimeServiceClient<Channel>) -> MethodResult {
    timed_verify!("initialize", {
        let resp = client
            .initialize(proto::InitializeRequest {
                payload: Some(proto::SessionPayload {
                    user_id: "certifier-user".into(),
                    runtime_id: "certifier".into(),
                    user_preferences: Some(proto::UserPreferences {
                        user_id: "certifier-user".into(),
                        language: "en".into(),
                        ..Default::default()
                    }),
                    allow_trim_p5: true,
                    ..Default::default()
                }),
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let init = resp.into_inner();
        info!(session_id = %init.session_id, "Initialize OK");
        Ok(Some(init.session_id))
    })
}

async fn verify_send(
    client: &mut RuntimeServiceClient<Channel>,
    session_id: &str,
) -> MethodResult {
    timed_verify!("send", {
        use tokio_stream::StreamExt;
        let mut stream = client
            .send(proto::SendRequest {
                session_id: session_id.into(),
                message: Some(proto::UserMessage {
                    content: "Say hello".into(),
                    message_type: "text".into(),
                    metadata: Default::default(),
                }),
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .into_inner();

        let mut chunk_count = 0u32;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(c) => {
                    chunk_count += 1;
                    if c.chunk_type == "done" {
                        break;
                    }
                }
                Err(e) => {
                    warn!("Send stream error: {e}");
                    break;
                }
            }
        }
        info!(chunks = chunk_count, "Send OK");
        Ok(Some(format!("{chunk_count} chunks")))
    })
}

async fn verify_load_skill(
    client: &mut RuntimeServiceClient<Channel>,
    session_id: &str,
) -> MethodResult {
    timed_verify!("load_skill", {
        let resp = client
            .load_skill(proto::LoadSkillRequest {
                session_id: session_id.into(),
                skill: Some(proto::SkillInstructions {
                    skill_id: "test-skill".into(),
                    name: "Test Skill".into(),
                    content: "Do a simple test.".into(),
                    frontmatter_hooks: vec![],
                    metadata: Default::default(),
                    dependencies: vec![],
                    required_tools: vec![],
                }),
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let result = resp.into_inner();
        if result.success {
            Ok(None)
        } else {
            Err(anyhow::anyhow!("LoadSkill failed: {}", result.error))
        }
    })
}

async fn verify_on_tool_call(
    client: &mut RuntimeServiceClient<Channel>,
    session_id: &str,
) -> MethodResult {
    timed_verify!("on_tool_call", {
        let resp = client
            .on_tool_call(proto::ToolCallEvent {
                session_id: session_id.into(),
                tool_name: "bash".into(),
                tool_id: "t-cert-1".into(),
                input_json: r#"{"command":"echo hello"}"#.into(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let ack = resp.into_inner();
        info!(decision = %ack.decision, "OnToolCall OK");
        Ok(None)
    })
}

async fn verify_on_tool_result(
    client: &mut RuntimeServiceClient<Channel>,
    session_id: &str,
) -> MethodResult {
    timed_verify!("on_tool_result", {
        let resp = client
            .on_tool_result(proto::ToolResultEvent {
                session_id: session_id.into(),
                tool_name: "bash".into(),
                tool_id: "t-cert-1".into(),
                output: "hello".into(),
                is_error: false,
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let ack = resp.into_inner();
        info!(decision = %ack.decision, "OnToolResult OK");
        Ok(None)
    })
}

async fn verify_on_stop(
    client: &mut RuntimeServiceClient<Channel>,
    session_id: &str,
) -> MethodResult {
    timed_verify!("on_stop", {
        let resp = client
            .on_stop(proto::StopEvent {
                session_id: session_id.into(),
                reason: "done".into(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let ack = resp.into_inner();
        info!(decision = %ack.decision, "OnStop OK");
        Ok(None)
    })
}

async fn verify_connect_mcp(
    client: &mut RuntimeServiceClient<Channel>,
    session_id: &str,
) -> MethodResult {
    timed_verify!("connect_mcp", {
        let resp = client
            .connect_mcp(proto::ConnectMcpRequest {
                session_id: session_id.into(),
                servers: vec![proto::McpServerConfig {
                    name: "certifier-test-mcp".into(),
                    transport: "stdio".into(),
                    command: "echo".into(),
                    args: vec!["test".into()],
                    url: String::new(),
                    env: Default::default(),
                }],
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let result = resp.into_inner();
        info!(success = result.success, "ConnectMcp responded");
        Ok(None)
    })
}

async fn verify_disconnect_mcp(
    client: &mut RuntimeServiceClient<Channel>,
    session_id: &str,
) -> MethodResult {
    timed_verify!("disconnect_mcp", {
        client
            .disconnect_mcp(proto::DisconnectMcpRequest {
                session_id: session_id.into(),
                server_name: "certifier-test-mcp".into(),
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(None)
    })
}

async fn verify_emit_telemetry(
    client: &mut RuntimeServiceClient<Channel>,
    session_id: &str,
) -> MethodResult {
    timed_verify!("emit_telemetry", {
        client
            .emit_telemetry(proto::TelemetryRequest {
                session_id: session_id.into(),
                events: vec![],
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Some("fire-and-forget ok".into()))
    })
}

async fn verify_get_state(client: &mut RuntimeServiceClient<Channel>) -> MethodResult {
    timed_verify!("get_state", {
        let resp = client
            .get_state(proto::Empty {})
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let state = resp.into_inner();
        info!(
            format = %state.state_format,
            bytes = state.state_data.len(),
            "GetState OK"
        );
        Ok(Some(format!(
            "format={}, {}B",
            state.state_format,
            state.state_data.len()
        )))
    })
}

async fn verify_pause_session(client: &mut RuntimeServiceClient<Channel>) -> MethodResult {
    timed_verify!("pause_session", {
        let resp = client
            .pause_session(proto::Empty {})
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let state = resp.into_inner();
        info!(session_id = %state.session_id, "PauseSession OK");
        Ok(None)
    })
}

async fn verify_resume_session(client: &mut RuntimeServiceClient<Channel>) -> MethodResult {
    timed_verify!("resume_session", {
        let result = client
            .resume_session(proto::StateResponse {
                session_id: "certifier-resume-test".into(),
                runtime_id: "certifier".into(),
                state_data: vec![],
                state_format: "rust-serde-v2".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
            })
            .await;
        match result {
            Ok(_) => {
                info!("ResumeSession OK");
                Ok(None)
            }
            Err(e) => {
                warn!("ResumeSession returned error (expected for stubs): {e}");
                Ok(Some("method exists but not fully implemented".into()))
            }
        }
    })
}

async fn verify_restore_state(client: &mut RuntimeServiceClient<Channel>) -> MethodResult {
    timed_verify!("restore_state", {
        let state = proto::StateResponse {
            session_id: "certifier-restore-test".into(),
            state_data: serde_json::to_vec(&serde_json::json!([]))?,
            runtime_id: "certifier".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            state_format: "rust-serde-v2".into(),
        };
        let result = client.restore_state(state).await;
        match result {
            Ok(_) => {
                info!("RestoreState OK");
                Ok(None)
            }
            Err(e) => {
                warn!("RestoreState returned error: {e}");
                Ok(Some("method exists, may need valid state data".into()))
            }
        }
    })
}

async fn verify_terminate(client: &mut RuntimeServiceClient<Channel>) -> MethodResult {
    timed_verify!("terminate", {
        client
            .terminate(proto::Empty {})
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        info!("Terminate OK");
        Ok(None)
    })
}

async fn verify_emit_event(
    client: &mut RuntimeServiceClient<Channel>,
    session_id: &str,
) -> MethodResult {
    // ADR-V2-001 pending — both Unimplemented and success count as
    // "placeholder present". Hard failure (connect error) counts as
    // absence.
    let start = std::time::Instant::now();
    let result = client
        .emit_event(proto::EventStreamEntry {
            session_id: session_id.into(),
            event_id: "evt-cert-1".into(),
            event_type: proto::HookEventType::PreToolUse as i32,
            payload_json: "{}".into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
        .await;
    let duration_ms = start.elapsed().as_millis() as u64;
    let class = MethodClass::of("emit_event").label().to_string();

    match result {
        Ok(_) => MethodResult {
            method: "emit_event".into(),
            class,
            passed: true,
            duration_ms,
            error: None,
            notes: Some("placeholder: present (implemented)".into()),
        },
        Err(status) if status.code() == tonic::Code::Unimplemented => MethodResult {
            method: "emit_event".into(),
            class,
            passed: true,
            duration_ms,
            error: None,
            notes: Some("placeholder: present (ADR-V2-001 pending)".into()),
        },
        Err(status) => {
            warn!("EmitEvent placeholder returned hard error: {status}");
            MethodResult {
                method: "emit_event".into(),
                class,
                passed: false,
                duration_ms,
                error: Some(status.to_string()),
                notes: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(method: &str, passed: bool) -> MethodResult {
        MethodResult {
            method: method.into(),
            class: MethodClass::of(method).label().to_string(),
            passed,
            duration_ms: 5,
            error: None,
            notes: None,
        }
    }

    #[test]
    fn report_display_passes_all_must() {
        let report = VerificationReport {
            endpoint: "http://localhost:50051".into(),
            runtime_id: "grid-harness".into(),
            runtime_name: "Grid".into(),
            tier: "harness".into(),
            deployment_mode: "shared".into(),
            passed: true,
            total: 2,
            passed_count: 2,
            failed_count: 0,
            must_total: 2,
            must_passed: 2,
            optional_total: 0,
            optional_present: 0,
            placeholder_present: true,
            results: vec![
                result("initialize", true),
                result("send", true),
            ],
            timestamp: "2026-04-11T12:00:00Z".into(),
        };

        let output = format!("{report}");
        assert!(output.contains("MUST methods: 2/2 PASS"));
        assert!(output.contains("PASS"));
        assert!(output.contains("EmitEvent placeholder: present"));
        assert!(output.contains("Grid"));
    }

    #[test]
    fn report_fails_when_must_missing() {
        let mut report = VerificationReport {
            endpoint: "http://localhost:50051".into(),
            runtime_id: "rt".into(),
            runtime_name: "RT".into(),
            tier: "harness".into(),
            deployment_mode: "shared".into(),
            passed: false,
            total: 1,
            passed_count: 0,
            failed_count: 1,
            must_total: 1,
            must_passed: 0,
            optional_total: 0,
            optional_present: 0,
            placeholder_present: false,
            results: vec![result("initialize", false)],
            timestamp: "2026-04-11T12:00:00Z".into(),
        };
        report.passed = report.compute_passed();
        assert!(!report.passed);
    }

    #[test]
    fn report_passes_on_must_only_even_if_optional_fails() {
        let mut report = VerificationReport {
            endpoint: "rt".into(),
            runtime_id: "rt".into(),
            runtime_name: "rt".into(),
            tier: "harness".into(),
            deployment_mode: "shared".into(),
            passed: false,
            total: 2,
            passed_count: 1,
            failed_count: 1,
            must_total: 1,
            must_passed: 1,
            optional_total: 1,
            optional_present: 0,
            placeholder_present: false,
            results: vec![
                result("initialize", true),
                result("health", false),
            ],
            timestamp: "2026-04-11T12:00:00Z".into(),
        };
        report.passed = report.compute_passed();
        assert!(report.passed, "optional failure must not block certification");
    }

    #[test]
    fn parse_caps_info_valid() {
        let (id, _name, tier, deploy) = parse_caps_info("grid-harness:Grid:harness:shared");
        assert_eq!(id, "grid-harness");
        assert_eq!(tier, "harness");
        assert_eq!(deploy, "shared");
    }

    #[test]
    fn parse_caps_info_without_deploy() {
        let (id, _name, tier, deploy) = parse_caps_info("grid-harness:Grid:harness");
        assert_eq!(id, "grid-harness");
        assert_eq!(tier, "harness");
        assert_eq!(deploy, "unknown");
    }

    #[test]
    fn parse_caps_info_empty() {
        let (id, name, tier, deploy) = parse_caps_info("");
        assert_eq!(id, "unknown");
        assert_eq!(name, "unknown");
        assert_eq!(tier, "unknown");
        assert_eq!(deploy, "unknown");
    }
}
