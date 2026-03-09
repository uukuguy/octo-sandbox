use tracing::debug;

use octo_types::skill::SkillDefinition;

use crate::skills::loader::SkillLoader;
use crate::skills::registry::SkillRegistry;
use crate::skills::trust::TrustManager;

/// Lightweight index entry for a skill (name + description + tags).
///
/// Used for L1 prompt generation: the LLM sees a concise list of available
/// skills without the full body, keeping context budget low.
#[derive(Debug, Clone)]
pub struct SkillIndexEntry {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub always: bool,
}

/// Unified entry point for skill management.
///
/// Integrates SkillLoader (filesystem I/O), SkillRegistry (runtime cache),
/// and TrustManager (permission enforcement) behind a single facade.
pub struct SkillManager {
    loader: SkillLoader,
    registry: SkillRegistry,
    trust_manager: TrustManager,
    index: Vec<SkillIndexEntry>,
}

impl SkillManager {
    pub fn new(loader: SkillLoader, registry: SkillRegistry, trust_manager: TrustManager) -> Self {
        Self {
            loader,
            registry,
            trust_manager,
            index: Vec::new(),
        }
    }

    /// Build (or rebuild) the lightweight index from the loader.
    ///
    /// Loads all skills via the registry, then extracts only name, description,
    /// tags, and always flag into `SkillIndexEntry` values. Returns a reference
    /// to the cached index.
    pub fn build_index(&mut self) -> &[SkillIndexEntry] {
        // Load all skills into the registry so we have full SkillDefinition data
        // (which includes tags and always fields).
        if let Err(e) = self.registry.load_from(&self.loader) {
            debug!(error = %e, "Failed to load skills for index build");
            self.index.clear();
            return &self.index;
        }

        let skills = self.registry.inner();
        let map = skills.read().unwrap_or_else(|e| e.into_inner());

        let mut entries: Vec<SkillIndexEntry> = map
            .values()
            .map(|s| SkillIndexEntry {
                name: s.name.clone(),
                description: s.description.clone(),
                tags: s.tags.clone(),
                always: s.always,
            })
            .collect();

        // Sort by name for deterministic output.
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        self.index = entries;
        &self.index
    }

    /// Activate a skill by name (lazy-load its full definition including body).
    ///
    /// Looks up the skill in the registry first. If the body is not yet loaded,
    /// re-loads it from disk via the loader.
    pub fn activate_skill(&self, name: &str) -> Option<SkillDefinition> {
        // Check if already in the registry.
        if let Some(skill) = self.registry.get(name) {
            if skill.body_loaded {
                return Some(skill);
            }
        }

        // Lazy-load from disk.
        match self.loader.load_skill(name) {
            Ok(skill) => Some(skill),
            Err(e) => {
                debug!(name = name, error = %e, "Failed to activate skill");
                None
            }
        }
    }

    /// Get a reference to the cached index.
    pub fn index(&self) -> &[SkillIndexEntry] {
        &self.index
    }

    /// Generate L1-level system prompt section: a compact list of skill names
    /// and descriptions (no bodies). Suitable for every request to give the LLM
    /// awareness of available skills.
    pub fn prompt_section_l1(&self) -> String {
        if self.index.is_empty() {
            return String::new();
        }

        let mut section = String::from("<available_skills>\n");
        for entry in &self.index {
            let tags_str = if entry.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", entry.tags.join(", "))
            };
            section.push_str(&format!(
                "- {}: {}{}\n",
                entry.name, entry.description, tags_str
            ));
        }
        section.push_str("</available_skills>");
        section
    }

    /// Generate L2-level system prompt section: includes full body for the
    /// specified active skills, plus a compact listing for the rest.
    pub fn prompt_section_l2(&self, active_skill_names: &[String]) -> String {
        if self.index.is_empty() {
            return String::new();
        }

        let mut section = String::from("<active_skills>\n");
        let mut has_active = false;

        for name in active_skill_names {
            if let Some(skill) = self.activate_skill(name) {
                has_active = true;
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
                if !skill.body.is_empty() {
                    section.push('\n');
                    section.push_str(&skill.body);
                    if !skill.body.ends_with('\n') {
                        section.push('\n');
                    }
                }
                section.push('\n');
            }
        }

        if !has_active {
            return self.prompt_section_l1();
        }

        section.push_str("</active_skills>");
        section
    }

    /// Return names of all skills marked `always: true`.
    pub fn always_active_skills(&self) -> Vec<String> {
        self.index
            .iter()
            .filter(|e| e.always)
            .map(|e| e.name.clone())
            .collect()
    }

    /// Access the underlying TrustManager (e.g., for permission checks).
    pub fn trust_manager(&self) -> &TrustManager {
        &self.trust_manager
    }

    /// Access the underlying SkillRegistry.
    pub fn registry(&self) -> &SkillRegistry {
        &self.registry
    }

    /// Access the underlying SkillLoader.
    pub fn loader(&self) -> &SkillLoader {
        &self.loader
    }
}
