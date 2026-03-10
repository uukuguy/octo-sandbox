//! Tests for Tool trait risk_level() and approval() methods.

use octo_types::{ApprovalRequirement, RiskLevel};

// ── Enum serialization/deserialization tests ──

#[test]
fn test_risk_level_serde_roundtrip() {
    let levels = vec![
        RiskLevel::ReadOnly,
        RiskLevel::LowRisk,
        RiskLevel::HighRisk,
        RiskLevel::Destructive,
    ];
    for level in levels {
        let json = serde_json::to_string(&level).unwrap();
        let deserialized: RiskLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(level, deserialized, "roundtrip failed for {json}");
    }
}

#[test]
fn test_approval_requirement_serde_roundtrip() {
    let requirements = vec![
        ApprovalRequirement::Never,
        ApprovalRequirement::AutoApprovable,
        ApprovalRequirement::Always,
    ];
    for req in requirements {
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: ApprovalRequirement = serde_json::from_str(&json).unwrap();
        assert_eq!(req, deserialized, "roundtrip failed for {json}");
    }
}

#[test]
fn test_risk_level_snake_case_serialization() {
    assert_eq!(
        serde_json::to_string(&RiskLevel::ReadOnly).unwrap(),
        "\"read_only\""
    );
    assert_eq!(
        serde_json::to_string(&RiskLevel::LowRisk).unwrap(),
        "\"low_risk\""
    );
    assert_eq!(
        serde_json::to_string(&RiskLevel::HighRisk).unwrap(),
        "\"high_risk\""
    );
    assert_eq!(
        serde_json::to_string(&RiskLevel::Destructive).unwrap(),
        "\"destructive\""
    );
}

#[test]
fn test_approval_requirement_snake_case_serialization() {
    assert_eq!(
        serde_json::to_string(&ApprovalRequirement::Never).unwrap(),
        "\"never\""
    );
    assert_eq!(
        serde_json::to_string(&ApprovalRequirement::AutoApprovable).unwrap(),
        "\"auto_approvable\""
    );
    assert_eq!(
        serde_json::to_string(&ApprovalRequirement::Always).unwrap(),
        "\"always\""
    );
}

// ── Built-in tool risk level tests ──

mod tool_risk {
    use octo_engine::tools::traits::Tool;
    use octo_types::{ApprovalRequirement, RiskLevel};

    use octo_engine::tools::bash::BashTool;
    use octo_engine::tools::file_edit::FileEditTool;
    use octo_engine::tools::file_read::FileReadTool;
    use octo_engine::tools::file_write::FileWriteTool;
    use octo_engine::tools::find::FindTool;
    use octo_engine::tools::glob::GlobTool;
    use octo_engine::tools::grep::GrepTool;
    use octo_engine::tools::web_fetch::WebFetchTool;
    use octo_engine::tools::web_search::WebSearchTool;

    #[test]
    fn bash_is_destructive_always_approve() {
        let tool = BashTool::new();
        assert_eq!(tool.risk_level(), RiskLevel::Destructive);
        assert_eq!(tool.approval(), ApprovalRequirement::Always);
    }

    #[test]
    fn file_read_is_readonly_never_approve() {
        let tool = FileReadTool::new();
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert_eq!(tool.approval(), ApprovalRequirement::Never);
    }

    #[test]
    fn file_write_is_high_risk_auto_approvable() {
        let tool = FileWriteTool::new();
        assert_eq!(tool.risk_level(), RiskLevel::HighRisk);
        assert_eq!(tool.approval(), ApprovalRequirement::AutoApprovable);
    }

    #[test]
    fn file_edit_is_high_risk_auto_approvable() {
        let tool = FileEditTool::new();
        assert_eq!(tool.risk_level(), RiskLevel::HighRisk);
        assert_eq!(tool.approval(), ApprovalRequirement::AutoApprovable);
    }

    #[test]
    fn glob_is_readonly() {
        let tool = GlobTool::new();
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert_eq!(tool.approval(), ApprovalRequirement::Never);
    }

    #[test]
    fn grep_is_readonly() {
        let tool = GrepTool::new();
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert_eq!(tool.approval(), ApprovalRequirement::Never);
    }

    #[test]
    fn find_is_readonly() {
        let tool = FindTool::new();
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert_eq!(tool.approval(), ApprovalRequirement::Never);
    }

    #[test]
    fn web_fetch_is_readonly() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert_eq!(tool.approval(), ApprovalRequirement::Never);
    }

    #[test]
    fn web_search_is_readonly() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert_eq!(tool.approval(), ApprovalRequirement::Never);
    }
}

// ── Default trait implementation test ──

mod default_impl {
    use anyhow::Result;
    use async_trait::async_trait;
    use octo_engine::tools::traits::Tool;
    use octo_types::{ApprovalRequirement, RiskLevel, ToolContext, ToolOutput, ToolSource};
    use serde_json::{json, Value};

    struct DummyTool;

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "a dummy tool"
        }
        fn parameters(&self) -> Value {
            json!({})
        }
        async fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
            Ok(ToolOutput::success("ok"))
        }
        fn source(&self) -> ToolSource {
            ToolSource::BuiltIn
        }
    }

    #[test]
    fn default_risk_level_is_low_risk() {
        let tool = DummyTool;
        assert_eq!(tool.risk_level(), RiskLevel::LowRisk);
    }

    #[test]
    fn default_approval_is_never() {
        let tool = DummyTool;
        assert_eq!(tool.approval(), ApprovalRequirement::Never);
    }
}
