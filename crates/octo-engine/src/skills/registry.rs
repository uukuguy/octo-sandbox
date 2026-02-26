use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use tracing::info;

use octo_types::SkillDefinition;

use super::loader::SkillLoader;

/// Thread-safe registry of loaded Skills.
pub struct SkillRegistry {
    skills: Arc<RwLock<HashMap<String, SkillDefinition>>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load all skills from the given SkillLoader.
    pub fn load_from(&self, loader: &SkillLoader) -> Result<()> {
        let loaded = loader.load_all()?;
        let mut skills = self.skills.write().unwrap();
        skills.clear();
        for skill in loaded {
            skills.insert(skill.name.clone(), skill);
        }
        info!("SkillRegistry loaded {} skills", skills.len());
        Ok(())
    }

    /// Reload all skills (for hot-reload).
    pub fn reload(&self, loader: &SkillLoader) -> Result<()> {
        let loaded = loader.load_all()?;
        let mut skills = self.skills.write().unwrap();
        let old_count = skills.len();
        skills.clear();
        for skill in loaded {
            skills.insert(skill.name.clone(), skill);
        }
        info!(
            "SkillRegistry reloaded: {} → {} skills",
            old_count,
            skills.len()
        );
        Ok(())
    }

    /// Generate system prompt section listing available skills.
    pub fn prompt_section(&self) -> String {
        let skills = self.skills.read().unwrap();
        if skills.is_empty() {
            return String::new();
        }

        let mut section = String::from("<available_skills>\n");
        let mut sorted: Vec<_> = skills.values().collect();
        sorted.sort_by_key(|s| &s.name);

        for skill in sorted {
            let version = skill
                .version
                .as_deref()
                .map(|v| format!(" (v{v})"))
                .unwrap_or_default();
            section.push_str(&format!("## {}{}\n", skill.name, version));
            section.push_str(&skill.description);
            if !skill.description.ends_with('\n') {
                section.push('\n');
            }
            if skill.user_invocable {
                section.push_str(&format!("Use: /{}\n", skill.name));
            }
            section.push('\n');
        }
        section.push_str("</available_skills>");
        section
    }

    /// Get all user-invocable skills (for registering as tools).
    pub fn invocable_skills(&self) -> Vec<SkillDefinition> {
        let skills = self.skills.read().unwrap();
        skills
            .values()
            .filter(|s| s.user_invocable)
            .cloned()
            .collect()
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<SkillDefinition> {
        let skills = self.skills.read().unwrap();
        skills.get(name).cloned()
    }

    /// Number of loaded skills.
    pub fn len(&self) -> usize {
        self.skills.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get inner Arc for sharing across threads.
    pub fn inner(&self) -> Arc<RwLock<HashMap<String, SkillDefinition>>> {
        self.skills.clone()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}
