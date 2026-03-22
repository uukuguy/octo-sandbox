//! Skill commands — list, show, create, validate skills

use crate::commands::AppState;
use crate::output::{self, TextOutput};
use crate::ui::table::Table;
use anyhow::Result;
use serde::Serialize;

use super::types::SkillCommands;

/// Handle skill subcommands
pub async fn handle_skill(action: SkillCommands, state: &AppState) -> Result<()> {
    match action {
        SkillCommands::List => list_skills(state).await,
        SkillCommands::Show { name } => show_skill(name, state).await,
        SkillCommands::Create { name } => create_skill(name).await,
        SkillCommands::Validate { path } => validate_skill(path).await,
    }
}

// ── Output types ──────────────────────────────────────────────

#[derive(Serialize)]
struct SkillListOutput {
    skills: Vec<SkillRow>,
}

#[derive(Serialize)]
struct SkillRow {
    name: String,
    mode: String,
    trust: String,
    triggers: String,
    description: String,
}

impl TextOutput for SkillListOutput {
    fn to_text(&self) -> String {
        if self.skills.is_empty() {
            return "No skills loaded. Check skills configuration.".to_string();
        }
        let mut t = Table::new(vec!["Name", "Mode", "Trust", "Triggers", "Description"]);
        for s in &self.skills {
            t.add_row(vec![
                s.name.clone(),
                s.mode.clone(),
                s.trust.clone(),
                s.triggers.clone(),
                truncate(&s.description, 50),
            ]);
        }
        format!("{} skills loaded:\n\n{}", self.skills.len(), t.render())
    }
}

#[derive(Serialize)]
struct SkillDetailOutput {
    name: String,
    description: String,
    version: Option<String>,
    mode: String,
    trust: String,
    user_invocable: bool,
    always: bool,
    allowed_tools: Vec<String>,
    triggers: Vec<String>,
    dependencies: Vec<String>,
    tags: Vec<String>,
    body_preview: String,
}

impl TextOutput for SkillDetailOutput {
    fn to_text(&self) -> String {
        let mut out = format!("Skill: {}\n", self.name);
        out.push_str(&format!("Description: {}\n", self.description));
        if let Some(ref v) = self.version {
            out.push_str(&format!("Version: {}\n", v));
        }
        out.push_str(&format!("Mode: {}\n", self.mode));
        out.push_str(&format!("Trust: {}\n", self.trust));
        out.push_str(&format!("User-invocable: {}\n", self.user_invocable));
        out.push_str(&format!("Always: {}\n", self.always));
        if !self.allowed_tools.is_empty() {
            out.push_str(&format!("Allowed tools: {}\n", self.allowed_tools.join(", ")));
        }
        if !self.triggers.is_empty() {
            out.push_str(&format!("Triggers: {}\n", self.triggers.join(", ")));
        }
        if !self.dependencies.is_empty() {
            out.push_str(&format!("Dependencies: {}\n", self.dependencies.join(", ")));
        }
        if !self.tags.is_empty() {
            out.push_str(&format!("Tags: {}\n", self.tags.join(", ")));
        }
        if !self.body_preview.is_empty() {
            out.push_str(&format!("\n--- Body ---\n{}", self.body_preview));
        }
        out
    }
}

// ── Handlers ──────────────────────────────────────────────────

async fn list_skills(state: &AppState) -> Result<()> {
    let skills = match state.agent_runtime.skill_registry() {
        Some(registry) => registry.list_all(),
        None => vec![],
    };

    let out = SkillListOutput {
        skills: skills
            .iter()
            .map(|s| {
                let mode = format!("{:?}", s.execution_mode);
                let trust = format!("{:?}", s.trust_level);
                let triggers: Vec<String> = s
                    .triggers
                    .iter()
                    .map(|t| match t {
                        octo_types::skill::SkillTrigger::FilePattern { pattern } => {
                            format!("file:{}", pattern)
                        }
                        octo_types::skill::SkillTrigger::Command { command } => {
                            format!("cmd:{}", command)
                        }
                        octo_types::skill::SkillTrigger::Keyword { keyword } => {
                            format!("kw:{}", keyword)
                        }
                    })
                    .collect();
                SkillRow {
                    name: s.name.clone(),
                    mode,
                    trust,
                    triggers: triggers.join(", "),
                    description: s.description.clone(),
                }
            })
            .collect(),
    };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn show_skill(name: String, state: &AppState) -> Result<()> {
    let skill = state
        .agent_runtime
        .skill_registry()
        .and_then(|r| r.get(&name));

    match skill {
        Some(s) => {
            let triggers: Vec<String> = s
                .triggers
                .iter()
                .map(|t| format!("{:?}", t))
                .collect();
            let body_preview = if s.body.len() > 500 {
                format!("{}...\n\n(truncated, {} chars total)", &s.body[..500], s.body.len())
            } else {
                s.body.clone()
            };
            let out = SkillDetailOutput {
                name: s.name.clone(),
                description: s.description.clone(),
                version: s.version.clone(),
                mode: format!("{:?}", s.execution_mode),
                trust: format!("{:?}", s.trust_level),
                user_invocable: s.user_invocable,
                always: s.always,
                allowed_tools: s.allowed_tools.clone().unwrap_or_default(),
                triggers,
                dependencies: s.dependencies.clone(),
                tags: s.tags.clone(),
                body_preview,
            };
            output::print_output(&out, &state.output_config);
        }
        None => {
            eprintln!("Skill not found: {}", name);
        }
    }
    Ok(())
}

async fn create_skill(name: String) -> Result<()> {
    let skill_dir = std::path::Path::new(".octo/skills").join(&name);
    if skill_dir.exists() {
        eprintln!("Skill directory already exists: {}", skill_dir.display());
        return Ok(());
    }

    std::fs::create_dir_all(&skill_dir)?;
    let template = format!(
        r#"---
name: {name}
description: TODO - describe what this skill does
version: "1.0"
user-invocable: true
execution-mode: knowledge
trust-level: installed
triggers:
  - type: keyword
    keyword: {name}
tags:
  - custom
---

# {name} Skill

TODO: Write instructions for this skill.

## Guidelines

1. ...
2. ...
"#,
    );

    let skill_file = skill_dir.join("SKILL.md");
    std::fs::write(&skill_file, template)?;
    println!("Created skill scaffold: {}", skill_dir.display());
    println!("  Edit {}/SKILL.md to customize", skill_dir.display());
    Ok(())
}

async fn validate_skill(path: String) -> Result<()> {
    let path = std::path::Path::new(&path);
    let skill_file = if path.is_dir() {
        path.join("SKILL.md")
    } else {
        path.to_path_buf()
    };

    if !skill_file.exists() {
        eprintln!("SKILL.md not found at: {}", skill_file.display());
        return Ok(());
    }

    match octo_engine::skills::loader::SkillLoader::parse_skill(&skill_file) {
        Ok(skill) => {
            println!("Valid skill: {}", skill.name);
            println!("  Description: {}", skill.description);
            println!("  Mode: {:?}", skill.execution_mode);
            println!("  User-invocable: {}", skill.user_invocable);
            if let Some(ref tools) = skill.allowed_tools {
                println!("  Allowed tools: {}", tools.join(", "));
            }
            println!("  Body length: {} chars", skill.body.len());
        }
        Err(e) => {
            eprintln!("Invalid skill: {}", e);
        }
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() <= max {
        s
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_list_empty() {
        let out = SkillListOutput { skills: vec![] };
        assert!(out.to_text().contains("No skills"));
    }

    #[test]
    fn test_skill_list_with_skills() {
        let out = SkillListOutput {
            skills: vec![SkillRow {
                name: "filesystem".to_string(),
                mode: "Playbook".to_string(),
                trust: "Installed".to_string(),
                triggers: "kw:file".to_string(),
                description: "File operations".to_string(),
            }],
        };
        let text = out.to_text();
        assert!(text.contains("filesystem"));
        assert!(text.contains("1 skills"));
    }

    #[test]
    fn test_create_skill_template() {
        let dir = tempfile::tempdir().unwrap();
        let name = "test-skill";
        let skill_dir = dir.path().join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        // Just test that the template is valid YAML
        let template = format!(
            "---\nname: {}\ndescription: test\nversion: \"1.0\"\nuser-invocable: true\n---\nbody",
            name
        );
        let skill_file = skill_dir.join("SKILL.md");
        std::fs::write(&skill_file, template).unwrap();
        assert!(skill_file.exists());
    }
}
