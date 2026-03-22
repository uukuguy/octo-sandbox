use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Execution mode for a skill.
///
/// - `Knowledge`: Instructions injected into the agent's system prompt (no SubAgent).
/// - `Playbook`: Executed via a SubAgent with isolated context and tool constraints.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    /// Instructions are injected into the agent context. Agent follows them directly.
    #[default]
    Knowledge,
    /// Executed by a SubAgent with isolated context.
    Playbook,
}

/// Trust level for skill execution (IronClaw Trust Attenuation).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Full access to all tools.
    Trusted,
    /// Only allowed-tools list.
    #[default]
    Installed,
    /// Read-only tools only.
    Unknown,
}

/// Trigger conditions for automatic skill activation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillTrigger {
    FilePattern { pattern: String },
    Command { command: String },
    Keyword { keyword: String },
}

/// Where the skill was loaded from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSourceType {
    /// .octo/skills/ in project.
    #[default]
    ProjectLocal,
    /// ~/.octo/skills/ user-global.
    UserLocal,
    /// Bundled with a plugin.
    PluginBundled,
    /// Downloaded from registry.
    Registry,
}

/// Skill definition parsed from a SKILL.md file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, rename = "user-invocable")]
    pub user_invocable: bool,
    #[serde(default, rename = "allowed-tools")]
    pub allowed_tools: Option<Vec<String>>,
    /// Markdown body (template variables already substituted).
    /// Lazy-loaded when skill is activated.
    #[serde(skip)]
    pub body: String,
    /// Directory containing SKILL.md.
    #[serde(skip)]
    pub base_dir: PathBuf,
    /// Full path to the SKILL.md file.
    #[serde(skip)]
    pub source_path: PathBuf,
    /// Flag indicating whether the body has been loaded.
    /// Used for lazy loading - initially false, set to true when activated.
    #[serde(skip)]
    pub body_loaded: bool,

    /// Execution mode: Knowledge (inject body) or Playbook (SubAgent execution).
    #[serde(default, rename = "execution-mode")]
    pub execution_mode: ExecutionMode,

    /// Model override for this skill.
    #[serde(default)]
    pub model: Option<String>,

    /// Run in isolated context.
    #[serde(default, rename = "context-fork")]
    pub context_fork: bool,

    /// Always include (never prune during compaction).
    #[serde(default)]
    pub always: bool,

    /// Trust level.
    #[serde(default, rename = "trust-level")]
    pub trust_level: TrustLevel,

    /// Auto-trigger conditions.
    #[serde(default)]
    pub triggers: Vec<SkillTrigger>,

    /// Dependencies on other skills.
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Classification tags.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Explicitly denied tools (overrides allowed_tools).
    #[serde(default, rename = "denied-tools")]
    pub denied_tools: Option<Vec<String>>,

    /// Source type.
    #[serde(default, skip)]
    pub source_type: SkillSourceType,
}
