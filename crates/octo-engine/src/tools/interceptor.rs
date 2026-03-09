use std::sync::Arc;

use octo_types::skill::SkillDefinition;

use crate::skills::trust::{TrustError, TrustManager};

/// Intercepts tool calls in AgentLoop based on active skill's constraints.
///
/// When a skill is active, this interceptor checks each tool call against
/// the skill's allowed_tools and denied_tools lists, mediated by TrustManager.
/// When no skill is active, all tools are permitted.
pub struct ToolCallInterceptor {
    trust_manager: Arc<TrustManager>,
    active_skill: Option<SkillDefinition>,
}

impl ToolCallInterceptor {
    /// Create an interceptor with the given trust manager and optional active skill.
    pub fn new(trust_manager: Arc<TrustManager>, active_skill: Option<SkillDefinition>) -> Self {
        Self {
            trust_manager,
            active_skill,
        }
    }

    /// Check if a single tool call is permitted.
    /// Returns Ok(()) if allowed, Err(TrustError) if blocked.
    pub fn check_permission(&self, tool_name: &str) -> Result<(), TrustError> {
        match &self.active_skill {
            None => Ok(()), // No active skill -- all tools permitted.
            Some(skill) => self.trust_manager.check_tool_permission(skill, tool_name),
        }
    }

    /// Filter a list of tool names, returning only those that are permitted.
    /// Useful for filtering the tools list sent to the LLM.
    pub fn filter_available_tools(&self, tool_names: &[String]) -> Vec<String> {
        tool_names
            .iter()
            .filter(|name| self.check_permission(name).is_ok())
            .cloned()
            .collect()
    }

    /// Set or clear the active skill.
    pub fn set_active_skill(&mut self, skill: Option<SkillDefinition>) {
        self.active_skill = skill;
    }

    /// Check if any skill is currently active.
    pub fn has_active_skill(&self) -> bool {
        self.active_skill.is_some()
    }
}
