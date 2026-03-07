//! AgentYamlDef — declarative agent definition loaded from YAML files.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::config::AgentConfig;
use super::entry::AgentManifest;

/// Declarative agent definition deserialized from a YAML file.
///
/// Supports two ways to specify a system prompt:
/// - `system_prompt`: inline string
/// - `system_prompt_template`: path to a file (relative to the YAML file's directory)
///
/// Capabilities are encoded as `cap:<name>` tags in the resulting manifest.
/// Agent type is encoded as a `type:<name>` tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentYamlDef {
    pub name: String,
    #[serde(rename = "type", default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub system_prompt_template: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tool_filter: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub max_concurrent_tasks: u32,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

impl AgentYamlDef {
    /// Load and parse an agent YAML definition from a file.
    ///
    /// Returns an error if:
    /// - The file cannot be read
    /// - The YAML is malformed
    /// - The `name` field is missing or empty
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading agent YAML: {}", path.display()))?;
        let def: Self = serde_yaml::from_str(&content)
            .with_context(|| format!("parsing agent YAML: {}", path.display()))?;
        if def.name.is_empty() {
            anyhow::bail!(
                "agent YAML missing required field 'name': {}",
                path.display()
            );
        }
        Ok(def)
    }

    /// Convert this declarative definition into an `AgentManifest`.
    ///
    /// - `base_dir` is used to resolve `system_prompt_template` paths
    /// - Agent type and capabilities are encoded as `type:` / `cap:` tags
    /// - `description` maps to `AgentManifest::backstory`
    pub fn into_manifest(self, base_dir: &Path) -> Result<AgentManifest> {
        let system_prompt = if let Some(inline) = self.system_prompt {
            Some(inline)
        } else if let Some(template_path) = self.system_prompt_template {
            let full_path = base_dir.join(&template_path);
            let content = std::fs::read_to_string(&full_path).with_context(|| {
                format!(
                    "reading system_prompt_template '{}'",
                    full_path.display()
                )
            })?;
            Some(content)
        } else {
            None
        };

        let mut tags = self.tags;
        if let Some(ref t) = self.agent_type {
            tags.push(format!("type:{t}"));
        }
        for cap in &self.capabilities {
            tags.push(format!("cap:{cap}"));
        }
        tags.sort();
        tags.dedup();

        Ok(AgentManifest {
            name: self.name,
            tags,
            role: self.role,
            goal: self.goal,
            backstory: self.description,
            system_prompt,
            model: self.model,
            tool_filter: self.tool_filter,
            config: AgentConfig::default(),
            max_concurrent_tasks: self.max_concurrent_tasks,
            priority: self.priority,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_yaml(dir: &TempDir, filename: &str, content: &str) -> std::path::PathBuf {
        let path = dir.path().join(filename);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_parse_minimal_yaml() {
        let dir = TempDir::new().unwrap();
        let path = write_yaml(&dir, "a.yaml", "name: my-agent\n");
        let def = AgentYamlDef::from_file(&path).unwrap();
        assert_eq!(def.name, "my-agent");
    }

    #[test]
    fn test_into_manifest_tags_composition() {
        let dir = TempDir::new().unwrap();
        let path = write_yaml(
            &dir,
            "a.yaml",
            "name: r\ntype: reviewer\ncapabilities: [code_review]\n",
        );
        let def = AgentYamlDef::from_file(&path).unwrap();
        let manifest = def.into_manifest(dir.path()).unwrap();
        assert!(manifest.tags.contains(&"type:reviewer".to_string()));
        assert!(manifest.tags.contains(&"cap:code_review".to_string()));
    }

    #[test]
    fn test_missing_name_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = write_yaml(&dir, "bad.yaml", "description: no name\n");
        assert!(AgentYamlDef::from_file(&path).is_err());
    }

    #[test]
    fn test_description_maps_to_backstory() {
        let dir = TempDir::new().unwrap();
        let path = write_yaml(&dir, "a.yaml", "name: agent\ndescription: my desc\n");
        let def = AgentYamlDef::from_file(&path).unwrap();
        let manifest = def.into_manifest(dir.path()).unwrap();
        assert_eq!(manifest.backstory, Some("my desc".to_string()));
    }

    #[test]
    fn test_system_prompt_template_loads_file() {
        let dir = TempDir::new().unwrap();
        let prompt_path = dir.path().join("prompt.txt");
        fs::write(&prompt_path, "You are an expert.").unwrap();
        let yaml = format!(
            "name: agent\nsystem_prompt_template: prompt.txt\n"
        );
        let path = write_yaml(&dir, "agent.yaml", &yaml);
        let def = AgentYamlDef::from_file(&path).unwrap();
        let manifest = def.into_manifest(dir.path()).unwrap();
        assert_eq!(manifest.system_prompt, Some("You are an expert.".to_string()));
    }

    #[test]
    fn test_max_concurrent_tasks_default_zero() {
        let dir = TempDir::new().unwrap();
        let path = write_yaml(&dir, "a.yaml", "name: agent\n");
        let def = AgentYamlDef::from_file(&path).unwrap();
        assert_eq!(def.max_concurrent_tasks, 0);
    }

    #[test]
    fn test_priority_field() {
        let dir = TempDir::new().unwrap();
        let path = write_yaml(&dir, "a.yaml", "name: agent\npriority: high\n");
        let def = AgentYamlDef::from_file(&path).unwrap();
        let manifest = def.into_manifest(dir.path()).unwrap();
        assert_eq!(manifest.priority, Some("high".to_string()));
    }
}
