//! Tool approval management based on approval policies.
//!
//! Provides an `ApprovalManager` that decides whether a tool invocation
//! should be auto-approved, requires user confirmation, or is denied.

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
}
