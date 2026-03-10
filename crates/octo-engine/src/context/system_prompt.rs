//! System Prompt Builder - Zone A (Static, no token budget)
//!
//! This module provides the SystemPromptBuilder for constructing Zone A content.
//! Zone A is the static system prompt that does not consume token budget.
//!
//! Priority order:
//! 1. system_prompt (full override from AgentManifest)
//! 2. role/goal/backstory (CrewAI pattern from AgentManifest)
//! 3. Bootstrap files (SOUL.md, AGENTS.md, etc.)
//! 4. Core instructions (lowest priority, always included as fallback)

use std::path::Path;

use octo_types::skill::SkillDefinition;

use crate::agent::entry::AgentManifest;

const CORE_INSTRUCTIONS: &str = r#"You are Octo, an AI coding assistant running inside a sandboxed environment.

You have access to tools for executing commands, reading and writing files, searching codebases, and browsing the web. Use these tools to help users with software engineering tasks.

## Guidelines
- Be concise, accurate, and helpful
- Always read files before suggesting modifications
- Use tools to verify your work
- Do not introduce security vulnerabilities
- Prefer editing existing files over creating new ones
"#;

const OUTPUT_GUIDELINES: &str = r#"## Output Format
- Use Markdown for formatting
- Include file paths when referencing code
- Use code blocks with language identifiers
"#;

/// Bootstrap file that gets loaded into system prompt
#[derive(Debug, Clone)]
pub struct BootstrapFile {
    /// Display name (e.g., "SOUL.md")
    pub name: String,
    /// File content
    pub content: String,
}

/// SystemPromptBuilder for constructing Zone A content
///
/// Zone A is the static system prompt that:
/// - Does NOT consume token budget
/// - Contains agent identity (role/goal/backstory)
/// - Contains bootstrap files (SOUL.md, AGENTS.md, etc.)
/// - Contains core instructions (lowest priority fallback)
pub struct SystemPromptBuilder {
    /// Agent manifest containing role/goal/backstory/system_prompt
    manifest: Option<AgentManifest>,
    /// Core instructions (lowest priority)
    core_instructions: String,
    /// Bootstrap files to include
    bootstrap_files: Vec<BootstrapFile>,
    /// Output guidelines
    output_guidelines: String,
    /// Skill index section (L1 listing)
    skill_index_section: Option<String>,
    /// Active skill section (L2 body injection)
    active_skill_section: Option<String>,
}

impl SystemPromptBuilder {
    /// Create a new SystemPromptBuilder with default core instructions
    pub fn new() -> Self {
        Self {
            manifest: None,
            core_instructions: CORE_INSTRUCTIONS.to_string(),
            bootstrap_files: Vec::new(),
            output_guidelines: OUTPUT_GUIDELINES.to_string(),
            skill_index_section: None,
            active_skill_section: None,
        }
    }

    /// Create a new SystemPromptBuilder with AgentManifest
    ///
    /// The manifest provides:
    /// - system_prompt: Full override (highest priority)
    /// - role/goal/backstory: CrewAI pattern (second priority)
    pub fn with_manifest(mut self, manifest: AgentManifest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    /// Set custom core instructions (overrides default)
    pub fn with_core_instructions(mut self, instructions: &str) -> Self {
        self.core_instructions = instructions.to_string();
        self
    }

    /// Add a bootstrap file
    pub fn with_bootstrap_file(mut self, name: &str, content: &str) -> Self {
        self.bootstrap_files.push(BootstrapFile {
            name: name.to_string(),
            content: content.to_string(),
        });
        self
    }

    /// Add bootstrap files from a directory
    ///
    /// Loads standard bootstrap files: AGENTS.md, CLAUDE.md, SOUL.md, TOOLS.md, IDENTITY.md, BOOTSTRAP.md
    pub fn with_bootstrap_dir(mut self, workspace_dir: &Path) -> Self {
        let bootstrap_filenames = [
            "AGENTS.md",
            "CLAUDE.md",
            "SOUL.md",
            "TOOLS.md",
            "IDENTITY.md",
            "BOOTSTRAP.md",
        ];

        for filename in bootstrap_filenames {
            let path = workspace_dir.join(filename);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    self.bootstrap_files.push(BootstrapFile {
                        name: filename.to_string(),
                        content,
                    });
                }
            }
        }
        self
    }

    /// Add L1 skill index listing to the system prompt
    ///
    /// Lists all user-invocable skills with their names and descriptions.
    /// If no skills are user-invocable, the section is omitted.
    pub fn with_skill_index(mut self, skills: &[SkillDefinition]) -> Self {
        let invocable: Vec<&SkillDefinition> =
            skills.iter().filter(|s| s.user_invocable).collect();

        if !invocable.is_empty() {
            let mut section = String::from(
                "## Available Skills\nThe following skills are available. \
                 You can invoke them when relevant:",
            );
            for skill in &invocable {
                section.push_str(&format!("\n- **{}**: {}", skill.name, skill.description));
            }
            self.skill_index_section = Some(section);
        }

        self
    }

    /// Add L2 active skill body to the system prompt
    ///
    /// Injects the full body of an activated skill so the agent follows
    /// its instructions during the current session.
    /// Skills with `always: true` are prefixed with a protection marker so
    /// the ContextPruner will never prune messages containing this content.
    pub fn with_active_skill(mut self, skill: &SkillDefinition) -> Self {
        let marker = if skill.always {
            format!("{}\n", super::pruner::SKILL_PROTECTED_MARKER)
        } else {
            String::new()
        };
        self.active_skill_section = Some(format!(
            "{}## Active Skill: {}\n{}",
            marker, skill.name, skill.body
        ));
        self
    }

    /// Build Zone A - System Prompt
    ///
    /// Priority order:
    /// 1. system_prompt (full override from AgentManifest)
    /// 2. role/goal/backstory (CrewAI pattern from AgentManifest)
    /// 3. Bootstrap files
    /// 4. Core instructions (lowest priority)
    pub fn build(&self) -> String {
        let mut parts = Vec::new();

        // Priority 1: Full system prompt override (highest)
        if let Some(ref manifest) = self.manifest {
            if let Some(ref system_prompt) = manifest.system_prompt {
                if !system_prompt.is_empty() {
                    return system_prompt.clone();
                }
            }
        }

        // Priority 2: role/goal/backstory (CrewAI pattern)
        if let Some(ref manifest) = self.manifest {
            if let Some(ref role) = manifest.role {
                if !role.is_empty() {
                    parts.push(format!("## Role\n{}", role));
                }
            }
            if let Some(ref goal) = manifest.goal {
                if !goal.is_empty() {
                    parts.push(format!("## Goal\n{}", goal));
                }
            }
            if let Some(ref backstory) = manifest.backstory {
                if !backstory.is_empty() {
                    parts.push(format!("## Backstory\n{}", backstory));
                }
            }
        }

        // Priority 3: Bootstrap files
        for file in &self.bootstrap_files {
            if !file.content.is_empty() {
                parts.push(format!("## {}\n{}", file.name, file.content));
            }
        }

        // Skill index (L1 listing)
        if let Some(ref section) = self.skill_index_section {
            parts.push(section.clone());
        }

        // Active skill (L2 body injection)
        if let Some(ref section) = self.active_skill_section {
            parts.push(section.clone());
        }

        // Priority 4: Core instructions (lowest, always included as fallback)
        parts.push(self.core_instructions.clone());

        let mut output = parts.join("\n\n");

        // Add output guidelines
        if !self.output_guidelines.is_empty() {
            output.push_str("\n\n");
            output.push_str(&self.output_guidelines);
        }

        output
    }

    /// Build only the identity section (role/goal/backstory)
    /// Used when manifest is provided but no full system prompt override
    pub fn build_identity(&self) -> Option<String> {
        if let Some(ref manifest) = self.manifest {
            let mut parts = Vec::new();

            if let Some(ref role) = manifest.role {
                if !role.is_empty() {
                    parts.push(format!("## Role\n{}", role));
                }
            }
            if let Some(ref goal) = manifest.goal {
                if !goal.is_empty() {
                    parts.push(format!("## Goal\n{}", goal));
                }
            }
            if let Some(ref backstory) = manifest.backstory {
                if !backstory.is_empty() {
                    parts.push(format!("## Backstory\n{}", backstory));
                }
            }

            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n\n"))
            }
        } else {
            None
        }
    }

    /// Check if there's a full system prompt override
    pub fn has_system_prompt_override(&self) -> bool {
        self.manifest
            .as_ref()
            .and_then(|m| m.system_prompt.as_ref())
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_override() {
        let manifest = AgentManifest {
            name: "test".to_string(),
            tags: vec![],
            role: Some("Test Role".to_string()),
            goal: Some("Test Goal".to_string()),
            backstory: Some("Test Backstory".to_string()),
            system_prompt: Some("Custom system prompt".to_string()),
            model: None,
            tool_filter: vec![],
            config: crate::agent::config::AgentConfig::default(),
            max_concurrent_tasks: 0,
            priority: None,
        };

        let builder = SystemPromptBuilder::new().with_manifest(manifest);
        let result = builder.build();

        assert_eq!(result, "Custom system prompt");
    }

    #[test]
    fn test_role_goal_backstory() {
        let manifest = AgentManifest {
            name: "test".to_string(),
            tags: vec![],
            role: Some("Test Role".to_string()),
            goal: Some("Test Goal".to_string()),
            backstory: Some("Test Backstory".to_string()),
            system_prompt: None,
            model: None,
            tool_filter: vec![],
            config: crate::agent::config::AgentConfig::default(),
            max_concurrent_tasks: 0,
            priority: None,
        };

        let builder = SystemPromptBuilder::new().with_manifest(manifest);
        let result = builder.build();

        assert!(result.contains("## Role\nTest Role"));
        assert!(result.contains("## Goal\nTest Goal"));
        assert!(result.contains("## Backstory\nTest Backstory"));
        assert!(result.contains(CORE_INSTRUCTIONS));
    }

    #[test]
    fn test_no_manifest() {
        let builder = SystemPromptBuilder::new();
        let result = builder.build();

        assert!(result.contains(CORE_INSTRUCTIONS));
    }

    #[test]
    fn test_bootstrap_files() {
        let builder =
            SystemPromptBuilder::new().with_bootstrap_file("SOUL.md", "Test SOUL content");
        let result = builder.build();

        assert!(result.contains("## SOUL.md\nTest SOUL content"));
    }

    fn make_skill(name: &str, desc: &str, user_invocable: bool) -> SkillDefinition {
        SkillDefinition {
            name: name.to_string(),
            description: desc.to_string(),
            version: None,
            user_invocable,
            allowed_tools: None,
            body: String::new(),
            base_dir: std::path::PathBuf::new(),
            source_path: std::path::PathBuf::new(),
            body_loaded: false,
            model: None,
            context_fork: false,
            always: false,
            trust_level: Default::default(),
            triggers: vec![],
            dependencies: vec![],
            tags: vec![],
            denied_tools: None,
            source_type: Default::default(),
        }
    }

    #[test]
    fn test_skill_index_filters_invocable() {
        let skills = vec![
            make_skill("code-review", "Reviews code", true),
            make_skill("internal-only", "Not for users", false),
            make_skill("deploy", "Deploys app", true),
        ];

        let builder = SystemPromptBuilder::new().with_skill_index(&skills);
        let result = builder.build();

        assert!(result.contains("## Available Skills"));
        assert!(result.contains("- **code-review**: Reviews code"));
        assert!(result.contains("- **deploy**: Deploys app"));
        assert!(!result.contains("internal-only"));
    }

    #[test]
    fn test_skill_index_empty_when_none_invocable() {
        let skills = vec![make_skill("hidden", "Hidden skill", false)];

        let builder = SystemPromptBuilder::new().with_skill_index(&skills);
        let result = builder.build();

        assert!(!result.contains("Available Skills"));
    }

    #[test]
    fn test_skill_index_empty_slice() {
        let builder = SystemPromptBuilder::new().with_skill_index(&[]);
        let result = builder.build();

        assert!(!result.contains("Available Skills"));
    }

    #[test]
    fn test_active_skill_injection() {
        let mut skill = make_skill("code-review", "Reviews code", true);
        skill.body = "Review all files for correctness.".to_string();

        let builder = SystemPromptBuilder::new().with_active_skill(&skill);
        let result = builder.build();

        assert!(result.contains("## Active Skill: code-review"));
        assert!(result.contains("Review all files for correctness."));
    }
}
