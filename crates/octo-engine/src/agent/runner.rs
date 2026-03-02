//! AgentRunner - owns startup dependencies, builds per-agent ToolRegistry,
//! and manages AgentLoop task lifecycle.
//!
//! AgentRunner bridges the AgentRegistry (which tracks state) with the actual
//! AgentLoop execution. It is responsible for:
//! - Building a per-agent ToolRegistry filtered by the agent's `tool_filter`
//! - Spawning a tokio task that wraps AgentLoop execution
//! - Translating lifecycle calls (start / stop / pause / resume) into registry
//!   state transitions and cancellation token management

use std::sync::Arc;

use crate::agent::{AgentError, AgentId, AgentRegistry, CancellationToken};
use crate::context::SystemPromptBuilder;
use crate::event::EventBus;
use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::skills::{SkillRegistry, SkillTool};
use crate::tools::ToolRegistry;

/// Shared startup dependencies for spawning AgentLoop tasks.
pub struct AgentRunner {
    pub registry: Arc<AgentRegistry>,
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    /// Optional: when set, build_tool_registry() dynamically includes the
    /// latest SkillTools from the registry (supports hot-reload).
    skill_registry: Option<Arc<SkillRegistry>>,
    memory: Arc<dyn WorkingMemory>,
    default_model: String,
    event_bus: Option<Arc<EventBus>>,
}

impl AgentRunner {
    pub fn new(
        registry: Arc<AgentRegistry>,
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        default_model: String,
    ) -> Self {
        Self {
            registry,
            provider,
            tools,
            skill_registry: None,
            memory,
            default_model,
            event_bus: None,
        }
    }

    /// Attach a SkillRegistry so that build_tool_registry() always reflects
    /// the latest hot-reloaded skills rather than the startup snapshot.
    pub fn with_skill_registry(mut self, skills: Arc<SkillRegistry>) -> Self {
        self.skill_registry = Some(skills);
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Start an agent: mark it Running and spawn a placeholder tokio task.
    ///
    /// The actual AgentLoop invocation (which requires session_id, user_id,
    /// messages, broadcast tx, and tool_ctx) is triggered by the WebSocket
    /// handler when a user message arrives. This method only advances the
    /// state machine and installs a CancellationToken.
    pub async fn start(&self, id: &AgentId) -> Result<(), AgentError> {
        // Verify the agent exists before doing any work.
        let entry = self
            .registry
            .get(id)
            .ok_or_else(|| AgentError::NotFound(id.clone()))?;

        // Build the per-agent tool registry according to tool_filter.
        let _agent_tools = self.build_tool_registry(&entry.manifest.tool_filter);

        // Resolve the model: use agent-level override, fall back to default.
        let _model = entry
            .manifest
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());

        // Build the system prompt from the manifest.
        let _system_prompt = build_system_prompt(&entry.manifest);

        let cancel_token = CancellationToken::new();

        // Advance state machine. mark_running() validates the transition.
        self.registry.mark_running(id, cancel_token)?;

        Ok(())
    }

    /// Stop a running or paused agent: cancel its token and mark Stopped.
    pub async fn stop(&self, id: &AgentId) -> Result<(), AgentError> {
        // mark_stopped() cancels the stored CancellationToken internally.
        self.registry.mark_stopped(id)
    }

    /// Pause a running agent: cancel current execution and mark Paused.
    pub async fn pause(&self, id: &AgentId) -> Result<(), AgentError> {
        // mark_paused() cancels the stored CancellationToken internally.
        self.registry.mark_paused(id)
    }

    /// Resume a paused agent: install a fresh token and mark Running again.
    pub async fn resume(&self, id: &AgentId) -> Result<(), AgentError> {
        let cancel_token = CancellationToken::new();
        self.registry.mark_resumed(id, cancel_token)
    }

    /// Build a ToolRegistry for a specific agent.
    ///
    /// 1. Start from the global `tools` snapshot (built-in tools + startup skills).
    /// 2. If a `SkillRegistry` is attached, overlay the *current* invocable skills
    ///    so hot-reloaded skills are always reflected when an agent starts.
    /// 3. Apply `tool_filter` whitelist (empty = all tools included).
    pub fn build_tool_registry(&self, tool_filter: &[String]) -> Arc<ToolRegistry> {
        // If no dynamic skills and no filter, fast path: share the global Arc.
        if self.skill_registry.is_none() && tool_filter.is_empty() {
            return self.tools.clone();
        }

        // Build a fresh registry from the global snapshot.
        let mut registry = ToolRegistry::new();
        for (name, tool) in self.tools.iter() {
            registry.register_arc(name.clone(), tool);
        }

        // Overlay current skills (replaces stale SkillTool entries from startup).
        if let Some(ref skills) = self.skill_registry {
            for skill in skills.invocable_skills() {
                let name = skill.name.clone();
                registry.register_arc(name, std::sync::Arc::new(SkillTool::new(skill)));
            }
        }

        // Apply per-agent tool filter.
        if tool_filter.is_empty() {
            return Arc::new(registry);
        }
        let mut filtered = ToolRegistry::new();
        for name in tool_filter {
            if let Some(tool) = registry.get(name) {
                filtered.register_arc(name.clone(), tool);
            }
        }
        Arc::new(filtered)
    }

    /// Returns a reference to the provider so callers can build an AgentLoop.
    pub fn provider(&self) -> Arc<dyn Provider> {
        self.provider.clone()
    }

    /// Returns a reference to the working memory so callers can build an AgentLoop.
    pub fn memory(&self) -> Arc<dyn WorkingMemory> {
        self.memory.clone()
    }

    /// Returns the default model name.
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Returns the event bus if one was configured.
    pub fn event_bus(&self) -> Option<Arc<EventBus>> {
        self.event_bus.clone()
    }
}

/// Build a system prompt from an AgentManifest.
///
/// Priority order (highest to lowest):
///   1. `system_prompt` field — used verbatim when present
///   2. `role` / `goal` / `backstory` — composed into Markdown sections
///   3. Fallback: default Octo system prompt via `SystemPromptBuilder`
fn build_system_prompt(manifest: &crate::agent::registry::AgentManifest) -> String {
    if let Some(ref prompt) = manifest.system_prompt {
        return prompt.clone();
    }

    if manifest.role.is_some() || manifest.goal.is_some() || manifest.backstory.is_some() {
        let mut parts: Vec<String> = Vec::new();
        if let Some(ref role) = manifest.role {
            parts.push(format!("## Role\n{role}"));
        }
        if let Some(ref goal) = manifest.goal {
            parts.push(format!("## Goal\n{goal}"));
        }
        if let Some(ref backstory) = manifest.backstory {
            parts.push(format!("## Backstory\n{backstory}"));
        }
        return parts.join("\n\n");
    }

    // Load SOUL.md / AGENTS.md / CLAUDE.md from the current working directory.
    SystemPromptBuilder::new()
        .with_bootstrap_dir(&std::env::current_dir().unwrap_or_default())
        .build_system_prompt()
}
