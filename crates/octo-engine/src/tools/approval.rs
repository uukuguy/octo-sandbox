//! Tool approval management based on approval policies.
//!
//! Provides an `ApprovalManager` that decides whether a tool invocation
//! should be auto-approved, requires user confirmation, or is denied.
//!
//! Also provides an `ApprovalGate` — a shared mechanism for the harness
//! to wait for human approval responses delivered via WebSocket or other channels.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{oneshot, Mutex};
use tracing::{debug, info, warn};

use octo_types::{ApprovalRequirement, RiskLevel};

/// Default timeout for waiting on human approval (30 seconds).
const APPROVAL_TIMEOUT_SECS: u64 = 30;

/// Approval policy that governs how tool calls are handled.
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalPolicy {
    /// Development mode — automatically approve all tool calls.
    AlwaysApprove,
    /// Rule-based automatic approval.
    SmartApprove(SmartApproveRules),
    /// Production mode — all tool calls require explicit approval.
    AlwaysAsk,
}

/// Rules for the `SmartApprove` policy.
#[derive(Debug, Clone, PartialEq)]
pub struct SmartApproveRules {
    /// Tool names that are unconditionally auto-approved.
    pub auto_approve_tools: Vec<String>,
    /// Whether read-only operations are auto-approved.
    pub auto_approve_readonly: bool,
}

impl Default for SmartApproveRules {
    fn default() -> Self {
        Self {
            auto_approve_tools: vec![],
            auto_approve_readonly: true,
        }
    }
}

/// The outcome of an approval check.
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalDecision {
    /// The tool call is approved and may proceed.
    Approved,
    /// The tool call requires user confirmation before proceeding.
    NeedsApproval { tool_name: String, reason: String },
    /// The tool call is denied outright.
    Denied { reason: String },
}

/// Manages tool-call approval decisions based on a configured policy.
pub struct ApprovalManager {
    policy: ApprovalPolicy,
}

impl ApprovalManager {
    /// Create a new `ApprovalManager` with the given policy.
    pub fn new(policy: ApprovalPolicy) -> Self {
        Self { policy }
    }

    /// Shortcut: development mode (auto-approve everything).
    pub fn dev_mode() -> Self {
        Self::new(ApprovalPolicy::AlwaysApprove)
    }

    /// Shortcut: production mode (require approval for everything).
    pub fn production_mode() -> Self {
        Self::new(ApprovalPolicy::AlwaysAsk)
    }

    /// Return the current policy.
    pub fn policy(&self) -> &ApprovalPolicy {
        &self.policy
    }

    /// Check whether a tool invocation should be approved.
    ///
    /// # Arguments
    /// * `tool_name` — the name of the tool being invoked.
    /// * `is_readonly` — whether the tool performs a read-only operation.
    pub fn check(&self, tool_name: &str, is_readonly: bool) -> ApprovalDecision {
        match &self.policy {
            ApprovalPolicy::AlwaysApprove => ApprovalDecision::Approved,
            ApprovalPolicy::AlwaysAsk => ApprovalDecision::NeedsApproval {
                tool_name: tool_name.to_string(),
                reason: "Production mode requires approval for all tools".to_string(),
            },
            ApprovalPolicy::SmartApprove(rules) => {
                if rules.auto_approve_readonly && is_readonly {
                    return ApprovalDecision::Approved;
                }
                if rules.auto_approve_tools.contains(&tool_name.to_string()) {
                    return ApprovalDecision::Approved;
                }
                ApprovalDecision::NeedsApproval {
                    tool_name: tool_name.to_string(),
                    reason: format!("Tool '{}' requires approval", tool_name),
                }
            }
        }
    }

    /// Three-level approval check using the Tool trait's `approval()` and `risk_level()`.
    ///
    /// Returns `ApprovalDecision` based on:
    /// - `ApprovalRequirement::Never` → always approved
    /// - `ApprovalRequirement::AutoApprovable` → auto-approve for ReadOnly/LowRisk,
    ///   require approval for HighRisk/Destructive
    /// - `ApprovalRequirement::Always` → always require human approval
    pub fn check_requirement(
        &self,
        tool_name: &str,
        requirement: ApprovalRequirement,
        risk_level: RiskLevel,
    ) -> ApprovalDecision {
        // If the global policy is AlwaysApprove, override everything (dev mode).
        if self.policy == ApprovalPolicy::AlwaysApprove {
            debug!(
                tool = tool_name,
                ?requirement,
                ?risk_level,
                "Approval: dev mode auto-approve"
            );
            return ApprovalDecision::Approved;
        }

        match requirement {
            ApprovalRequirement::Never => {
                debug!(tool = tool_name, "Approval: Never required, auto-approved");
                ApprovalDecision::Approved
            }
            ApprovalRequirement::AutoApprovable => {
                match risk_level {
                    RiskLevel::ReadOnly | RiskLevel::LowRisk => {
                        debug!(
                            tool = tool_name,
                            ?risk_level,
                            "Approval: AutoApprovable + low risk, auto-approved"
                        );
                        ApprovalDecision::Approved
                    }
                    RiskLevel::HighRisk | RiskLevel::Destructive => {
                        info!(
                            tool = tool_name,
                            ?risk_level,
                            "Approval: AutoApprovable but high risk, needs approval"
                        );
                        ApprovalDecision::NeedsApproval {
                            tool_name: tool_name.to_string(),
                            reason: format!(
                                "Tool '{}' is auto-approvable but risk level is {:?}",
                                tool_name, risk_level
                            ),
                        }
                    }
                }
            }
            ApprovalRequirement::Always => {
                info!(
                    tool = tool_name,
                    "Approval: Always required, needs human approval"
                );
                ApprovalDecision::NeedsApproval {
                    tool_name: tool_name.to_string(),
                    reason: format!(
                        "Tool '{}' always requires human approval",
                        tool_name
                    ),
                }
            }
        }
    }
}

/// Shared gate for pending approval requests.
///
/// The harness registers a pending approval (tool_id → oneshot::Sender),
/// emits an `AgentEvent::ApprovalRequired`, and waits on the oneshot receiver.
/// An external consumer (e.g., WS handler) calls `respond()` to deliver the
/// human decision.
#[derive(Clone, Default)]
pub struct ApprovalGate {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>,
}

impl ApprovalGate {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a pending approval and return a receiver to await the response.
    pub async fn register(&self, tool_id: &str) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(tool_id.to_string(), tx);
        rx
    }

    /// Deliver a human approval response for the given tool_id.
    /// Returns `true` if the tool_id was found and the response was delivered.
    pub async fn respond(&self, tool_id: &str, approved: bool) -> bool {
        if let Some(sender) = self.pending.lock().await.remove(tool_id) {
            let _ = sender.send(approved);
            debug!(tool_id, approved, "Approval response delivered");
            true
        } else {
            warn!(tool_id, "No pending approval found for tool_id");
            false
        }
    }

    /// Wait for an approval response with a timeout.
    /// Returns `true` if approved, `false` if rejected or timed out.
    pub async fn wait_for_approval(rx: oneshot::Receiver<bool>) -> bool {
        match tokio::time::timeout(
            Duration::from_secs(APPROVAL_TIMEOUT_SECS),
            rx,
        )
        .await
        {
            Ok(Ok(approved)) => {
                debug!(approved, "Approval response received");
                approved
            }
            Ok(Err(_)) => {
                warn!("Approval channel closed (sender dropped), auto-rejecting");
                false
            }
            Err(_) => {
                warn!(
                    timeout_secs = APPROVAL_TIMEOUT_SECS,
                    "Approval timed out, auto-rejecting"
                );
                false
            }
        }
    }
}

impl std::fmt::Debug for ApprovalGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovalGate").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_approve_approves_all() {
        let mgr = ApprovalManager::new(ApprovalPolicy::AlwaysApprove);
        assert_eq!(mgr.check("bash", false), ApprovalDecision::Approved);
        assert_eq!(mgr.check("file_write", false), ApprovalDecision::Approved);
        assert_eq!(mgr.check("file_read", true), ApprovalDecision::Approved);
    }

    #[test]
    fn always_ask_requires_approval_for_all() {
        let mgr = ApprovalManager::new(ApprovalPolicy::AlwaysAsk);
        match mgr.check("bash", false) {
            ApprovalDecision::NeedsApproval { tool_name, .. } => {
                assert_eq!(tool_name, "bash");
            }
            other => panic!("Expected NeedsApproval, got {:?}", other),
        }
        match mgr.check("file_read", true) {
            ApprovalDecision::NeedsApproval { tool_name, .. } => {
                assert_eq!(tool_name, "file_read");
            }
            other => panic!("Expected NeedsApproval, got {:?}", other),
        }
    }

    #[test]
    fn dev_mode_shortcut() {
        let mgr = ApprovalManager::dev_mode();
        assert_eq!(*mgr.policy(), ApprovalPolicy::AlwaysApprove);
        assert_eq!(mgr.check("anything", false), ApprovalDecision::Approved);
    }

    #[test]
    fn production_mode_shortcut() {
        let mgr = ApprovalManager::production_mode();
        assert_eq!(*mgr.policy(), ApprovalPolicy::AlwaysAsk);
        assert!(matches!(
            mgr.check("anything", false),
            ApprovalDecision::NeedsApproval { .. }
        ));
    }

    #[test]
    fn check_requirement_never_always_approved() {
        let mgr = ApprovalManager::new(ApprovalPolicy::AlwaysAsk);
        assert_eq!(
            mgr.check_requirement("file_read", ApprovalRequirement::Never, RiskLevel::ReadOnly),
            ApprovalDecision::Approved
        );
    }

    #[test]
    fn check_requirement_auto_approvable_low_risk() {
        let mgr = ApprovalManager::new(ApprovalPolicy::AlwaysAsk);
        assert_eq!(
            mgr.check_requirement(
                "file_read",
                ApprovalRequirement::AutoApprovable,
                RiskLevel::LowRisk
            ),
            ApprovalDecision::Approved
        );
    }

    #[test]
    fn check_requirement_auto_approvable_high_risk() {
        let mgr = ApprovalManager::new(ApprovalPolicy::AlwaysAsk);
        assert!(matches!(
            mgr.check_requirement(
                "bash",
                ApprovalRequirement::AutoApprovable,
                RiskLevel::HighRisk
            ),
            ApprovalDecision::NeedsApproval { .. }
        ));
    }

    #[test]
    fn check_requirement_always_needs_approval() {
        let mgr = ApprovalManager::new(ApprovalPolicy::AlwaysAsk);
        assert!(matches!(
            mgr.check_requirement(
                "bash",
                ApprovalRequirement::Always,
                RiskLevel::ReadOnly
            ),
            ApprovalDecision::NeedsApproval { .. }
        ));
    }

    #[test]
    fn check_requirement_dev_mode_overrides_always() {
        let mgr = ApprovalManager::dev_mode();
        assert_eq!(
            mgr.check_requirement("bash", ApprovalRequirement::Always, RiskLevel::Destructive),
            ApprovalDecision::Approved
        );
    }

    #[tokio::test]
    async fn approval_gate_register_and_respond() {
        let gate = ApprovalGate::new();
        let rx = gate.register("tool-1").await;

        // Respond on another task
        let gate2 = gate.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            gate2.respond("tool-1", true).await;
        });

        let result = ApprovalGate::wait_for_approval(rx).await;
        assert!(result);
    }

    #[tokio::test]
    async fn approval_gate_timeout_rejects() {
        let gate = ApprovalGate::new();
        let rx = gate.register("tool-2").await;
        // Don't respond — just drop. The timeout in wait_for_approval is 30s
        // which is too long for a test, so we test the channel-closed path.
        drop(gate);
        let result = ApprovalGate::wait_for_approval(rx).await;
        assert!(!result);
    }

    #[tokio::test]
    async fn approval_gate_respond_unknown_returns_false() {
        let gate = ApprovalGate::new();
        let found = gate.respond("nonexistent", true).await;
        assert!(!found);
    }
}
