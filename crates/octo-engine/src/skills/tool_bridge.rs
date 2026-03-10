use tracing::warn;

use super::loader::SkillLoader;
use super::registry::SkillRegistry;
use super::tool::SkillTool;
use crate::tools::ToolRegistry;

/// Register all user-invocable skills from a `SkillLoader` as tools in the `ToolRegistry`.
///
/// Only skills with `user_invocable == true` are registered, to avoid flooding
/// the tool list with internal-only skills.
///
/// Errors during loading are logged as warnings; no panics.
pub fn register_skills_as_tools(loader: &SkillLoader, registry: &mut ToolRegistry) {
    let skills = match loader.load_all() {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "Failed to load skills for tool registration");
            return;
        }
    };

    let mut count = 0usize;
    for skill in skills {
        if !skill.user_invocable {
            continue;
        }
        let name = skill.name.clone();
        registry.register(SkillTool::new(skill));
        tracing::debug!(skill = %name, "Registered skill as tool");
        count += 1;
    }

    if count > 0 {
        tracing::info!(count, "Registered skills as tools");
    }
}

/// Register all user-invocable skills from a `SkillRegistry` as tools in the `ToolRegistry`.
///
/// This variant reads from an already-loaded `SkillRegistry` rather than re-loading
/// from disk via `SkillLoader`.
pub fn register_skills_from_registry(
    skill_registry: &SkillRegistry,
    tool_registry: &mut ToolRegistry,
) {
    let mut count = 0usize;
    for skill in skill_registry.invocable_skills() {
        let name = skill.name.clone();
        tool_registry.register(SkillTool::new(skill));
        tracing::debug!(skill = %name, "Registered skill as tool (from registry)");
        count += 1;
    }

    if count > 0 {
        tracing::info!(count, "Registered skills as tools (from registry)");
    }
}
