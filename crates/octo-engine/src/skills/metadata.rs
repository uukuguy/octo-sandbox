use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Lightweight skill metadata for building index (without reading body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub version: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
}

impl SkillMetadata {
    /// Check if this metadata entry has body loaded.
    /// Since SkillMetadata is always built from frontmatter only (no body),
    /// this always returns false.
    /// This method is useful for distinguishing between index entries and fully loaded skills.
    pub fn has_body(&self) -> bool {
        false
    }

    /// Parse skill metadata from SKILL.md frontmatter only (without reading body).
    pub fn from_frontmatter(path: &std::path::Path) -> anyhow::Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

        let (frontmatter, _) = crate::skills::loader::SkillLoader::split_frontmatter(&content)
            .with_context(|| format!("splitting frontmatter in {}", path.display()))?;

        // Parse only the fields we need
        #[derive(Deserialize)]
        struct FrontmatterOnly {
            name: String,
            description: String,
            #[serde(default)]
            version: Option<String>,
            #[serde(default, rename = "allowed-tools")]
            allowed_tools: Option<Vec<String>>,
        }

        let fm: FrontmatterOnly = serde_yaml::from_str(&frontmatter)
            .with_context(|| format!("parsing YAML frontmatter in {}", path.display()))?;

        // Validate required fields
        if fm.name.is_empty() {
            anyhow::bail!(
                "SKILL.md missing required field 'name' in {}",
                path.display()
            );
        }
        if fm.description.is_empty() {
            anyhow::bail!(
                "SKILL.md missing required field 'description' in {}",
                path.display()
            );
        }

        Ok(SkillMetadata {
            name: fm.name,
            description: fm.description,
            path: path.to_path_buf(),
            version: fm.version,
            allowed_tools: fm.allowed_tools,
        })
    }
}
