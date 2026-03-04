use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
}
