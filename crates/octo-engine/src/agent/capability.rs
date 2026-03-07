//! Agent capability declarations for task-to-agent matching

use serde::{Deserialize, Serialize};

/// A capability an agent can declare
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentCapability {
    CodeGeneration,
    CodeReview,
    Testing,
    SecurityAudit,
    Research,
    Architecture,
    Debugging,
    Refactoring,
    Documentation,
    DevOps,
    FrontendDev,
    BackendDev,
    /// Data analysis, analytics, metrics, statistics, reporting
    DataAnalysis,
    /// General-purpose agent with no specific specialization
    General,
    /// Custom capability with a name
    Custom(String),
}

impl AgentCapability {
    /// Parse from string (case-insensitive)
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "code-generation" | "code_generation" | "coding" | "implementation" => Self::CodeGeneration,
            "code-review" | "code_review" | "review" => Self::CodeReview,
            "testing" | "test" => Self::Testing,
            "security" | "security-audit" | "security_audit" => Self::SecurityAudit,
            "research" | "analysis" => Self::Research,
            "architecture" | "design" => Self::Architecture,
            "debugging" | "debug" => Self::Debugging,
            "refactoring" | "refactor" | "bug-fix" | "bug_fix" => Self::Refactoring,
            "documentation" | "docs" => Self::Documentation,
            "devops" | "deployment" => Self::DevOps,
            "frontend" | "ui" => Self::FrontendDev,
            "backend" | "api" => Self::BackendDev,
            "data-analysis" | "data_analysis" | "analytics" | "data" => Self::DataAnalysis,
            "general" | "any" | "all" => Self::General,
            other => Self::Custom(other.to_string()),
        }
    }

    /// Keywords associated with this capability (for task matching)
    pub fn keywords(&self) -> &[&str] {
        match self {
            Self::CodeGeneration => &[
                "implement", "create", "build", "add", "write",
            ],
            Self::CodeReview => &[
                "review", "audit", "check", "validate",
            ],
            Self::Testing => &[
                "test", "spec", "coverage",
            ],
            Self::SecurityAudit => &[
                "security", "vulnerability",
            ],
            Self::Research => &[
                "research", "find", "search", "analyze",
            ],
            Self::Architecture => &[
                "design", "architect", "structure", "plan",
            ],
            Self::Debugging => &[
                "debug", "fix", "error", "bug",
            ],
            Self::Refactoring => &[
                "refactor", "cleanup",
            ],
            Self::Documentation => &[
                "document", "docs", "readme",
            ],
            Self::DevOps => &[
                "deploy", "docker", "ci", "cd", "pipeline",
            ],
            Self::FrontendDev => &[
                "ui", "frontend", "component", "react", "css",
            ],
            Self::BackendDev => &[
                "api", "endpoint", "server", "database",
            ],
            Self::DataAnalysis => &[
                "data", "analysis", "analytics", "metrics", "statistics", "report",
            ],
            Self::General => &[
                "help", "assist", "general", "anything",
            ],
            Self::Custom(_) => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_loose() {
        assert_eq!(AgentCapability::from_str_loose("coding"), AgentCapability::CodeGeneration);
        assert_eq!(AgentCapability::from_str_loose("REVIEW"), AgentCapability::CodeReview);
        assert_eq!(AgentCapability::from_str_loose("security-audit"), AgentCapability::SecurityAudit);
        assert_eq!(AgentCapability::from_str_loose("frontend"), AgentCapability::FrontendDev);
        assert_eq!(
            AgentCapability::from_str_loose("something-custom"),
            AgentCapability::Custom("something-custom".to_string())
        );
    }

    #[test]
    fn test_keywords_non_empty() {
        let cap = AgentCapability::CodeGeneration;
        assert!(!cap.keywords().is_empty());
    }

    #[test]
    fn test_custom_has_no_keywords() {
        let cap = AgentCapability::Custom("foo".to_string());
        assert!(cap.keywords().is_empty());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let cap = AgentCapability::SecurityAudit;
        let json = serde_json::to_string(&cap).unwrap();
        let decoded: AgentCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, decoded);
    }
}
