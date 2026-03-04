use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use tracing::{debug, warn};

use octo_types::SkillDefinition;

use crate::skills::SkillMetadata;

pub struct SkillLoader {
    search_dirs: Vec<PathBuf>,
}

impl SkillLoader {
    /// Create a loader with project-level and user-level skill directories.
    /// Project-level skills override user-level on name conflict.
    pub fn new(project_dir: Option<&Path>, user_dir: Option<&Path>) -> Self {
        let mut search_dirs = Vec::new();
        // Project-level first (higher priority)
        if let Some(dir) = project_dir {
            let skills_dir = dir.join(".octo").join("skills");
            if skills_dir.is_dir() {
                search_dirs.push(skills_dir);
            }
        }
        // User-level second
        if let Some(dir) = user_dir {
            let skills_dir = dir.join(".octo").join("skills");
            if skills_dir.is_dir() {
                search_dirs.push(skills_dir);
            }
        }
        Self { search_dirs }
    }

    /// Scan all directories and parse SKILL.md files.
    pub fn load_all(&self) -> Result<Vec<SkillDefinition>> {
        let mut skills = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        for dir in &self.search_dirs {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(e) => {
                    debug!(dir = %dir.display(), error = %e, "Cannot read skills directory");
                    continue;
                }
            };

            for entry in entries.flatten() {
                let skill_dir = entry.path();
                if !skill_dir.is_dir() {
                    continue;
                }
                let skill_file = skill_dir.join("SKILL.md");
                if !skill_file.exists() {
                    continue;
                }

                match Self::parse_skill(&skill_file) {
                    Ok(skill) => {
                        if seen_names.contains(&skill.name) {
                            debug!(name = %skill.name, "Skill already loaded (project overrides user)");
                            continue;
                        }
                        debug!(name = %skill.name, path = %skill_file.display(), "Loaded skill");
                        seen_names.insert(skill.name.clone());
                        skills.push(skill);
                    }
                    Err(e) => {
                        warn!(path = %skill_file.display(), error = %e, "Failed to parse SKILL.md");
                    }
                }
            }
        }

        debug!(
            "Loaded {} skills from {} directories",
            skills.len(),
            self.search_dirs.len()
        );
        Ok(skills)
    }

    /// Build skills index by parsing only frontmatter (not body).
    /// This is faster than load_all() when you only need skill names/descriptions.
    pub fn build_index(&self) -> Vec<SkillMetadata> {
        let mut index = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        for dir in &self.search_dirs {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(e) => {
                    debug!(dir = %dir.display(), error = %e, "Cannot read skills directory");
                    continue;
                }
            };

            for entry in entries.flatten() {
                let skill_dir = entry.path();
                if !skill_dir.is_dir() {
                    continue;
                }
                let skill_file = skill_dir.join("SKILL.md");
                if !skill_file.exists() {
                    continue;
                }

                match SkillMetadata::from_frontmatter(&skill_file) {
                    Ok(metadata) => {
                        if seen_names.contains(&metadata.name) {
                            debug!(name = %metadata.name, "Skill already in index (project overrides user)");
                            continue;
                        }
                        debug!(name = %metadata.name, path = %skill_file.display(), "Indexed skill");
                        seen_names.insert(metadata.name.clone());
                        index.push(metadata);
                    }
                    Err(e) => {
                        warn!(path = %skill_file.display(), error = %e, "Failed to parse SKILL.md frontmatter");
                    }
                }
            }
        }

        debug!(
            "Built index with {} skills from {} directories",
            index.len(),
            self.search_dirs.len()
        );
        index
    }

    /// Parse a single SKILL.md file.
    pub fn parse_skill(path: &Path) -> Result<SkillDefinition> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

        let (frontmatter, body) = Self::split_frontmatter(&content)
            .with_context(|| format!("splitting frontmatter in {}", path.display()))?;

        let mut skill: SkillDefinition = serde_yaml::from_str(&frontmatter)
            .with_context(|| format!("parsing YAML frontmatter in {}", path.display()))?;

        // Validate required fields
        if skill.name.is_empty() {
            bail!(
                "SKILL.md missing required field 'name' in {}",
                path.display()
            );
        }
        if skill.description.is_empty() {
            bail!(
                "SKILL.md missing required field 'description' in {}",
                path.display()
            );
        }

        let base_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();

        // Substitute template variables
        let base_dir_str = base_dir.to_string_lossy();
        let substituted_body = body.replace("${baseDir}", &base_dir_str);

        skill.body = substituted_body;
        skill.base_dir = base_dir;
        skill.source_path = path.to_path_buf();

        Ok(skill)
    }

    /// Split content into YAML frontmatter and Markdown body.
    /// Frontmatter is delimited by `---` at the start and end.
    pub fn split_frontmatter(content: &str) -> Result<(String, String)> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            bail!("No YAML frontmatter found (must start with ---)");
        }

        // Find closing ---
        let after_first = &trimmed[3..];
        let end_pos = after_first
            .find("\n---")
            .ok_or_else(|| anyhow::anyhow!("No closing --- for frontmatter"))?;

        let frontmatter = after_first[..end_pos].trim().to_string();
        let body_start = end_pos + 4; // skip \n---
        let body = if body_start < after_first.len() {
            after_first[body_start..]
                .trim_start_matches('\n')
                .to_string()
        } else {
            String::new()
        };

        Ok((frontmatter, body))
    }

    /// Get the search directories (for file watching).
    pub fn search_dirs(&self) -> &[PathBuf] {
        &self.search_dirs
    }

    /// Lazily load a single skill by name (called when skill is activated).
    /// This reads the full SKILL.md file including body.
    pub fn load_skill(&self, name: &str) -> Result<SkillDefinition> {
        for dir in &self.search_dirs {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(e) => {
                    debug!(dir = %dir.display(), error = %e, "Cannot read skills directory");
                    continue;
                }
            };

            for entry in entries.flatten() {
                let skill_dir = entry.path();
                if !skill_dir.is_dir() {
                    continue;
                }
                let skill_file = skill_dir.join("SKILL.md");
                if !skill_file.exists() {
                    continue;
                }

                // First check if this skill matches the name we're looking for
                // We do a quick frontmatter check to avoid parsing all files
                if let Ok(content) = std::fs::read_to_string(&skill_file) {
                    if let Ok((frontmatter, _)) = Self::split_frontmatter(&content) {
                        if let Ok(fm) = serde_yaml::from_str::<serde_yaml::Value>(&frontmatter) {
                            // Extract name from frontmatter
                            if let Some(skill_name) = fm.get("name").and_then(|v| v.as_str()) {
                                if skill_name == name {
                                    // Found the skill, now parse fully
                                    let mut skill = Self::parse_skill(&skill_file)?;
                                    skill.body_loaded = true;
                                    return Ok(skill);
                                }
                            }
                        }
                    }
                }
            }
        }

        anyhow::bail!("Skill not found: {}", name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a SKILL.md file with a large body.
    fn create_skill_with_large_body(temp_dir: &TempDir, body_size: usize) {
        let skills_dir = temp_dir.path().join(".octo").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        let skill_dir = skills_dir.join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let body = "x".repeat(body_size);
        let content = format!(
            r#"---
name: test-skill
description: A test skill with large body
---

{}
"#,
            body
        );

        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    /// Create a SKILL.md file with optional fields.
    fn create_skill_with_optional_fields(temp_dir: &TempDir, name: &str) {
        let skills_dir = temp_dir.path().join(".octo").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        let skill_dir = skills_dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = format!(
            r#"---
name: {}
description: A skill with optional fields
version: "1.0.0"
allowed-tools:
  - bash
  - read
---

# Skill Body
This is the skill content.
"#,
            name
        );

        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn test_build_index_only_parses_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        create_skill_with_large_body(&temp_dir, 10000); // 10KB body

        let loader = SkillLoader::new(Some(temp_dir.path()), None);
        let index = loader.build_index();

        // Verify only metadata was loaded
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].name, "test-skill");
        assert_eq!(index[0].description, "A test skill with large body");
    }

    #[test]
    fn test_build_index_with_optional_fields() {
        let temp_dir = TempDir::new().unwrap();
        create_skill_with_optional_fields(&temp_dir, "optional-fields-skill");

        let loader = SkillLoader::new(Some(temp_dir.path()), None);
        let index = loader.build_index();

        assert_eq!(index.len(), 1);
        assert_eq!(index[0].name, "optional-fields-skill");
        assert_eq!(index[0].version, Some("1.0.0".to_string()));
        assert_eq!(
            index[0].allowed_tools,
            Some(vec!["bash".to_string(), "read".to_string()])
        );
    }

    #[test]
    fn test_build_index_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        // Create empty .octo/skills directory
        let skills_dir = temp_dir.path().join(".octo").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        let loader = SkillLoader::new(Some(temp_dir.path()), None);
        let index = loader.build_index();

        assert!(index.is_empty());
    }

    #[test]
    fn test_build_index_skips_invalid_skills() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join(".octo").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        // Create a skill directory without SKILL.md
        std::fs::create_dir_all(skills_dir.join("no-file")).unwrap();

        // Create a skill with invalid frontmatter
        let invalid_dir = skills_dir.join("invalid");
        std::fs::create_dir_all(&invalid_dir).unwrap();
        std::fs::write(
            invalid_dir.join("SKILL.md"),
            "not frontmatter\n\nbody content",
        )
        .unwrap();

        let loader = SkillLoader::new(Some(temp_dir.path()), None);
        let index = loader.build_index();

        // Should skip invalid skill
        assert!(index.is_empty());
    }

    #[test]
    fn test_metadata_from_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: test-skill
description: Test description
version: "2.0.0"
allowed-tools:
  - tool1
  - tool2
---

# Body
This should not be read.
"#;

        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        let metadata = SkillMetadata::from_frontmatter(&skill_dir.join("SKILL.md")).unwrap();

        assert_eq!(metadata.name, "test-skill");
        assert_eq!(metadata.description, "Test description");
        assert_eq!(metadata.version, Some("2.0.0".to_string()));
        assert_eq!(
            metadata.allowed_tools,
            Some(vec!["tool1".to_string(), "tool2".to_string()])
        );
    }

    #[test]
    fn test_lazy_load_skill_body() {
        let temp_dir = TempDir::new().unwrap();
        create_skill_with_large_body(&temp_dir, 10000); // 10KB body

        let loader = SkillLoader::new(Some(temp_dir.path()), None);

        // 1. First get the index (does not load body)
        let index = loader.build_index();
        assert_eq!(index.len(), 1);
        assert!(!index[0].has_body(), "Index should not have body loaded");

        // 2. Activate skill - load full content
        let skill = loader.load_skill("test-skill").unwrap();
        assert!(skill.body_loaded, "Body should be marked as loaded");
        assert!(!skill.body.is_empty(), "Body should not be empty");
        // Body size should be at least the expected size (may have trailing newline)
        assert!(
            skill.body.len() >= 10000,
            "Body should be at least expected size"
        );
    }

    #[test]
    fn test_lazy_load_skill_not_found() {
        let temp_dir = TempDir::new().unwrap();
        create_skill_with_optional_fields(&temp_dir, "existing-skill");

        let loader = SkillLoader::new(Some(temp_dir.path()), None);

        // Try to load a non-existent skill
        let result = loader.load_skill("non-existent-skill");
        assert!(result.is_err());
    }

    #[test]
    fn test_lazy_load_skill_with_template_variable() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join(".octo").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        let skill_dir = skills_dir.join("template-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        // Create a skill that uses ${baseDir} template variable
        let content = r#"---
name: template-skill
description: A skill with template variable
---

# Template Test
Base dir: ${baseDir}
File path: ${baseDir}/test.txt
"#;

        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        let loader = SkillLoader::new(Some(temp_dir.path()), None);
        let skill = loader.load_skill("template-skill").unwrap();

        // Verify template variable was substituted
        let base_dir_str = skill.base_dir.to_string_lossy().to_string();
        assert!(skill.body.contains(&base_dir_str));
        assert!(skill.body.contains("Template Test"));
    }

    #[test]
    fn test_lazy_load_multiple_skills() {
        let temp_dir = TempDir::new().unwrap();
        create_skill_with_optional_fields(&temp_dir, "skill-one");
        create_skill_with_optional_fields(&temp_dir, "skill-two");

        let loader = SkillLoader::new(Some(temp_dir.path()), None);

        // Build index first
        let index = loader.build_index();
        assert_eq!(index.len(), 2);

        // Load only one skill
        let skill = loader.load_skill("skill-one").unwrap();
        assert_eq!(skill.name, "skill-one");
        assert!(skill.body_loaded);
    }
}
