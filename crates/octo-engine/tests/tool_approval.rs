use octo_engine::tools::approval::{
    ApprovalDecision, ApprovalManager, ApprovalPolicy, SmartApproveRules,
};

#[test]
fn always_approve_approves_all_tools() {
    let mgr = ApprovalManager::new(ApprovalPolicy::AlwaysApprove);
    assert_eq!(mgr.check("bash", false), ApprovalDecision::Approved);
    assert_eq!(mgr.check("file_write", false), ApprovalDecision::Approved);
    assert_eq!(mgr.check("file_read", true), ApprovalDecision::Approved);
    assert_eq!(mgr.check("unknown_tool", false), ApprovalDecision::Approved);
}

#[test]
fn always_ask_requires_approval_for_all_tools() {
    let mgr = ApprovalManager::new(ApprovalPolicy::AlwaysAsk);

    let decision = mgr.check("bash", false);
    assert!(matches!(decision, ApprovalDecision::NeedsApproval { .. }));
    if let ApprovalDecision::NeedsApproval { tool_name, reason } = decision {
        assert_eq!(tool_name, "bash");
        assert!(reason.contains("Production mode"));
    }

    let decision = mgr.check("file_read", true);
    assert!(matches!(decision, ApprovalDecision::NeedsApproval { .. }));
}

#[test]
fn smart_approve_auto_approves_readonly() {
    let rules = SmartApproveRules {
        auto_approve_tools: vec![],
        auto_approve_readonly: true,
    };
    let mgr = ApprovalManager::new(ApprovalPolicy::SmartApprove(rules));

    assert_eq!(mgr.check("file_read", true), ApprovalDecision::Approved);
    assert_eq!(mgr.check("grep", true), ApprovalDecision::Approved);
}

#[test]
fn smart_approve_requires_approval_for_non_readonly() {
    let rules = SmartApproveRules {
        auto_approve_tools: vec![],
        auto_approve_readonly: true,
    };
    let mgr = ApprovalManager::new(ApprovalPolicy::SmartApprove(rules));

    let decision = mgr.check("bash", false);
    assert!(matches!(decision, ApprovalDecision::NeedsApproval { .. }));
    if let ApprovalDecision::NeedsApproval { tool_name, reason } = decision {
        assert_eq!(tool_name, "bash");
        assert!(reason.contains("requires approval"));
    }
}

#[test]
fn smart_approve_auto_approve_tools_list() {
    let rules = SmartApproveRules {
        auto_approve_tools: vec!["bash".to_string(), "file_write".to_string()],
        auto_approve_readonly: false,
    };
    let mgr = ApprovalManager::new(ApprovalPolicy::SmartApprove(rules));

    // Listed tools are approved even when not readonly
    assert_eq!(mgr.check("bash", false), ApprovalDecision::Approved);
    assert_eq!(mgr.check("file_write", false), ApprovalDecision::Approved);

    // Unlisted tool requires approval
    let decision = mgr.check("file_edit", false);
    assert!(matches!(decision, ApprovalDecision::NeedsApproval { .. }));
}

#[test]
fn smart_approve_readonly_disabled() {
    let rules = SmartApproveRules {
        auto_approve_tools: vec![],
        auto_approve_readonly: false,
    };
    let mgr = ApprovalManager::new(ApprovalPolicy::SmartApprove(rules));

    // Even readonly operations need approval when auto_approve_readonly is false
    let decision = mgr.check("file_read", true);
    assert!(matches!(decision, ApprovalDecision::NeedsApproval { .. }));
}

#[test]
fn dev_mode_shortcut() {
    let mgr = ApprovalManager::dev_mode();
    assert_eq!(*mgr.policy(), ApprovalPolicy::AlwaysApprove);
    assert_eq!(mgr.check("bash", false), ApprovalDecision::Approved);
    assert_eq!(
        mgr.check("dangerous_tool", false),
        ApprovalDecision::Approved
    );
}

#[test]
fn production_mode_shortcut() {
    let mgr = ApprovalManager::production_mode();
    assert_eq!(*mgr.policy(), ApprovalPolicy::AlwaysAsk);
    assert!(matches!(
        mgr.check("file_read", true),
        ApprovalDecision::NeedsApproval { .. }
    ));
}

#[test]
fn smart_approve_default_rules() {
    let rules = SmartApproveRules::default();
    assert!(rules.auto_approve_tools.is_empty());
    assert!(rules.auto_approve_readonly);
}

#[test]
fn smart_approve_combined_rules() {
    // Both readonly and listed tools should be approved
    let rules = SmartApproveRules {
        auto_approve_tools: vec!["bash".to_string()],
        auto_approve_readonly: true,
    };
    let mgr = ApprovalManager::new(ApprovalPolicy::SmartApprove(rules));

    // Readonly approved via readonly rule
    assert_eq!(mgr.check("file_read", true), ApprovalDecision::Approved);
    // Listed tool approved via tool list
    assert_eq!(mgr.check("bash", false), ApprovalDecision::Approved);
    // Unlisted non-readonly needs approval
    assert!(matches!(
        mgr.check("file_write", false),
        ApprovalDecision::NeedsApproval { .. }
    ));
}
