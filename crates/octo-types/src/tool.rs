use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::id::SandboxId;

/// Tool risk level (aligned with MCP Tool Annotations spec)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Read-only operations (no side effects)
    ReadOnly,
    /// Low-risk modifications (default)
    LowRisk,
    /// High-risk modifications (file writes, config changes)
    HighRisk,
    /// Destructive operations (shell execution, deletions)
    Destructive,
}

/// Tool approval requirement
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRequirement {
    /// Never requires approval (default)
    Never,
    /// Can be auto-approved based on policy rules
    AutoApprovable,
    /// Always requires explicit human approval
    Always,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    BuiltIn,
    Mcp(String),   // MCP server name
    Skill(String), // Skill name
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Tool output artifact (file, image, structured data, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub content_type: String,
    pub data: String,
}

/// Structured tool output with artifacts, metadata, and truncation info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
    pub artifacts: Vec<Artifact>,
    pub metadata: Option<serde_json::Value>,
    pub truncated: bool,
    pub original_size: Option<usize>,
    pub duration_ms: u64,
}

impl ToolOutput {
    /// Create a successful output with the given content.
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
            artifacts: Vec::new(),
            metadata: None,
            truncated: false,
            original_size: None,
            duration_ms: 0,
        }
    }

    /// Create an error output with the given content.
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
            artifacts: Vec::new(),
            metadata: None,
            truncated: false,
            original_size: None,
            duration_ms: 0,
        }
    }

    /// Attach an artifact to this output.
    pub fn with_artifact(mut self, artifact: Artifact) -> Self {
        self.artifacts.push(artifact);
        self
    }

    /// Attach JSON metadata to this output.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Record the execution duration in milliseconds.
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    /// Mark this output as truncated, recording the original size in bytes.
    pub fn mark_truncated(mut self, original_size: usize) -> Self {
        self.truncated = true;
        self.original_size = Some(original_size);
        self
    }
}

/// Progress update emitted by a tool during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgress {
    /// Progress percentage (0.0 to 1.0), None if indeterminate.
    pub fraction: Option<f64>,
    /// Human-readable status message.
    pub message: String,
    /// Bytes processed so far (for data-intensive tools).
    pub bytes_processed: Option<u64>,
    /// Total bytes expected (for data-intensive tools).
    pub bytes_total: Option<u64>,
    /// Elapsed milliseconds since tool execution started.
    pub elapsed_ms: u64,
}

impl ToolProgress {
    /// Create an indeterminate progress (no known completion percentage).
    pub fn indeterminate(message: impl Into<String>) -> Self {
        Self {
            fraction: None,
            message: message.into(),
            bytes_processed: None,
            bytes_total: None,
            elapsed_ms: 0,
        }
    }

    /// Create a progress with a known completion fraction (0.0 to 1.0).
    pub fn percent(fraction: f64, message: impl Into<String>) -> Self {
        Self {
            fraction: Some(fraction),
            message: message.into(),
            bytes_processed: None,
            bytes_total: None,
            elapsed_ms: 0,
        }
    }

    /// Attach byte-level progress information.
    pub fn with_bytes(mut self, processed: u64, total: u64) -> Self {
        self.bytes_processed = Some(processed);
        self.bytes_total = Some(total);
        self
    }

    /// Set the elapsed time in milliseconds.
    pub fn with_elapsed(mut self, elapsed_ms: u64) -> Self {
        self.elapsed_ms = elapsed_ms;
        self
    }

    /// Returns true if fraction >= 1.0.
    pub fn is_complete(&self) -> bool {
        matches!(self.fraction, Some(f) if f >= 1.0)
    }
}

/// Trait for validating file paths against security policies.
pub trait PathValidator: Send + Sync + std::fmt::Debug {
    fn check_path(&self, path: &Path) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub sandbox_id: SandboxId,
    pub working_dir: PathBuf,
    pub path_validator: Option<Arc<dyn PathValidator>>,
}
