//! Dual Agent Mode — Plan Agent + Build Agent separation
//!
//! Provides a two-agent architecture where a Plan Agent (no tools, reasoning only)
//! produces structured plans, and a Build Agent (with tools) executes them.

use serde::{Deserialize, Serialize};

use octo_types::SessionId;

use super::executor::AgentExecutorHandle;

// ---------------------------------------------------------------------------
// AgentSlot
// ---------------------------------------------------------------------------

/// Which agent slot is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentSlot {
    Plan,
    Build,
}

impl Default for AgentSlot {
    fn default() -> Self {
        Self::Build
    }
}

impl std::fmt::Display for AgentSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan => write!(f, "plan"),
            Self::Build => write!(f, "build"),
        }
    }
}

// ---------------------------------------------------------------------------
// ToolFilterMode
// ---------------------------------------------------------------------------

/// How tools are filtered for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolFilterMode {
    /// No tools available (Plan Agent).
    None,
    /// All tools available (Build Agent default).
    All,
    /// Only specific tools by name.
    AllowList(Vec<String>),
}

impl ToolFilterMode {
    /// Check if this filter allows any tools.
    pub fn allows_tools(&self) -> bool {
        !matches!(self, Self::None)
    }
}

// ---------------------------------------------------------------------------
// DualAgentProfile
// ---------------------------------------------------------------------------

/// Configuration profile for a dual-mode agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DualAgentProfile {
    /// Agent display name.
    pub name: String,
    /// System prompt override for this agent mode.
    pub system_prompt: String,
    /// Tool filter controlling which tools are available.
    pub tool_filter: ToolFilterMode,
    /// Which slot this profile belongs to.
    pub slot: AgentSlot,
}

// ---------------------------------------------------------------------------
// PlanStep
// ---------------------------------------------------------------------------

/// A step extracted from Plan Agent's output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Step number (1-based).
    pub number: u32,
    /// Step description.
    pub description: String,
    /// Whether this step has been completed by Build Agent.
    pub completed: bool,
}

// ---------------------------------------------------------------------------
// DualAgentManager
// ---------------------------------------------------------------------------

/// Manages Plan and Build agent executors sharing a session.
pub struct DualAgentManager {
    /// The Plan Agent handle.
    plan_handle: AgentExecutorHandle,
    /// The Build Agent handle.
    build_handle: AgentExecutorHandle,
    /// Currently active agent slot.
    active: AgentSlot,
    /// Steps extracted from Plan Agent output.
    plan_steps: Vec<PlanStep>,
    /// Shared session ID.
    session_id: SessionId,
}

impl DualAgentManager {
    pub fn new(
        plan_handle: AgentExecutorHandle,
        build_handle: AgentExecutorHandle,
        session_id: SessionId,
    ) -> Self {
        Self {
            plan_handle,
            build_handle,
            active: AgentSlot::Build,
            plan_steps: Vec::new(),
            session_id,
        }
    }

    /// Get the currently active agent handle.
    pub fn active_handle(&self) -> &AgentExecutorHandle {
        match self.active {
            AgentSlot::Plan => &self.plan_handle,
            AgentSlot::Build => &self.build_handle,
        }
    }

    /// Get the active agent slot.
    pub fn active_slot(&self) -> AgentSlot {
        self.active
    }

    /// Switch to the other agent.
    pub fn switch(&mut self) -> AgentSlot {
        self.active = match self.active {
            AgentSlot::Plan => AgentSlot::Build,
            AgentSlot::Build => AgentSlot::Plan,
        };
        self.active
    }

    /// Switch to a specific slot.
    pub fn switch_to(&mut self, slot: AgentSlot) {
        self.active = slot;
    }

    /// Get the plan handle.
    pub fn plan_handle(&self) -> &AgentExecutorHandle {
        &self.plan_handle
    }

    /// Get the build handle.
    pub fn build_handle(&self) -> &AgentExecutorHandle {
        &self.build_handle
    }

    /// Add a plan step extracted from Plan Agent output.
    pub fn add_plan_step(&mut self, step: PlanStep) {
        self.plan_steps.push(step);
    }

    /// Get all plan steps.
    pub fn plan_steps(&self) -> &[PlanStep] {
        &self.plan_steps
    }

    /// Mark a plan step as completed. Returns `true` if found.
    pub fn complete_step(&mut self, number: u32) -> bool {
        if let Some(step) = self.plan_steps.iter_mut().find(|s| s.number == number) {
            step.completed = true;
            true
        } else {
            false
        }
    }

    /// Clear all plan steps (e.g., when Plan Agent produces a new plan).
    pub fn clear_steps(&mut self) {
        self.plan_steps.clear();
    }

    /// Get plan context as a string suitable for injecting into Build Agent.
    pub fn plan_context_string(&self) -> String {
        if self.plan_steps.is_empty() {
            return String::new();
        }
        let mut s = String::from("## Plan Steps\n\n");
        for step in &self.plan_steps {
            let mark = if step.completed { "x" } else { " " };
            s.push_str(&format!(
                "- [{}] Step {}: {}\n",
                mark, step.number, step.description
            ));
        }
        s
    }

    /// Parse plan steps from markdown text (Plan Agent output).
    ///
    /// Recognises three formats:
    /// - Numbered: `1. Step description`
    /// - Dash bullet: `- Step description`
    /// - Star bullet: `* Step description`
    pub fn parse_plan_steps(text: &str) -> Vec<PlanStep> {
        let mut steps = Vec::new();
        let mut number = 1u32;

        for line in text.lines() {
            let trimmed = line.trim();

            let desc = if let Some(rest) = trimmed.strip_prefix("- ") {
                Some(rest.trim())
            } else if let Some(rest) = trimmed.strip_prefix("* ") {
                Some(rest.trim())
            } else if let Some(pos) = trimmed.find(". ") {
                let prefix = &trimmed[..pos];
                if prefix.chars().all(|c| c.is_ascii_digit()) {
                    Some(trimmed[pos + 2..].trim())
                } else {
                    Option::None
                }
            } else {
                Option::None
            };

            if let Some(description) = desc {
                if !description.is_empty() {
                    steps.push(PlanStep {
                        number,
                        description: description.to_string(),
                        completed: false,
                    });
                    number += 1;
                }
            }
        }
        steps
    }

    /// Session ID.
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::{broadcast, mpsc};

    // -- AgentSlot -----------------------------------------------------------

    #[test]
    fn agent_slot_default_is_build() {
        assert_eq!(AgentSlot::default(), AgentSlot::Build);
    }

    #[test]
    fn agent_slot_display() {
        assert_eq!(AgentSlot::Plan.to_string(), "plan");
        assert_eq!(AgentSlot::Build.to_string(), "build");
    }

    #[test]
    fn agent_slot_serde_roundtrip() {
        let json = serde_json::to_string(&AgentSlot::Plan).unwrap();
        let back: AgentSlot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AgentSlot::Plan);
    }

    // -- ToolFilterMode ------------------------------------------------------

    #[test]
    fn tool_filter_none_denies_tools() {
        assert!(!ToolFilterMode::None.allows_tools());
    }

    #[test]
    fn tool_filter_all_allows_tools() {
        assert!(ToolFilterMode::All.allows_tools());
    }

    #[test]
    fn tool_filter_allowlist_allows_tools() {
        let f = ToolFilterMode::AllowList(vec!["bash".into()]);
        assert!(f.allows_tools());
    }

    #[test]
    fn tool_filter_allowlist_empty_allows_tools() {
        // An empty allow-list still "allows tools" in concept (zero tools selected).
        let f = ToolFilterMode::AllowList(vec![]);
        assert!(f.allows_tools());
    }

    // -- DualAgentProfile ----------------------------------------------------

    #[test]
    fn dual_agent_profile_creation() {
        let profile = DualAgentProfile {
            name: "Planner".into(),
            system_prompt: "You are a planning agent.".into(),
            tool_filter: ToolFilterMode::None,
            slot: AgentSlot::Plan,
        };
        assert_eq!(profile.name, "Planner");
        assert_eq!(profile.slot, AgentSlot::Plan);
        assert!(!profile.tool_filter.allows_tools());
    }

    #[test]
    fn dual_agent_profile_serde_roundtrip() {
        let profile = DualAgentProfile {
            name: "Builder".into(),
            system_prompt: "You build things.".into(),
            tool_filter: ToolFilterMode::All,
            slot: AgentSlot::Build,
        };
        let json = serde_json::to_string(&profile).unwrap();
        let back: DualAgentProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Builder");
        assert_eq!(back.slot, AgentSlot::Build);
    }

    // -- PlanStep ------------------------------------------------------------

    #[test]
    fn plan_step_creation() {
        let step = PlanStep {
            number: 1,
            description: "Read the file".into(),
            completed: false,
        };
        assert_eq!(step.number, 1);
        assert!(!step.completed);
    }

    #[test]
    fn plan_step_serde_roundtrip() {
        let step = PlanStep {
            number: 3,
            description: "Deploy".into(),
            completed: true,
        };
        let json = serde_json::to_string(&step).unwrap();
        let back: PlanStep = serde_json::from_str(&json).unwrap();
        assert_eq!(back.number, 3);
        assert!(back.completed);
    }

    // -- Helper: make test handles -------------------------------------------

    fn make_test_handles() -> (AgentExecutorHandle, AgentExecutorHandle) {
        let (tx1, _rx1) = mpsc::channel(1);
        let (btx1, _) = broadcast::channel(1);
        let h1 = AgentExecutorHandle {
            tx: tx1,
            broadcast_tx: btx1,
            session_id: SessionId::from_string("plan-session"),
        };
        let (tx2, _rx2) = mpsc::channel(1);
        let (btx2, _) = broadcast::channel(1);
        let h2 = AgentExecutorHandle {
            tx: tx2,
            broadcast_tx: btx2,
            session_id: SessionId::from_string("build-session"),
        };
        (h1, h2)
    }

    // -- DualAgentManager ----------------------------------------------------

    #[test]
    fn manager_default_active_is_build() {
        let (plan, build) = make_test_handles();
        let mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));
        assert_eq!(mgr.active_slot(), AgentSlot::Build);
    }

    #[test]
    fn manager_switch_toggles() {
        let (plan, build) = make_test_handles();
        let mut mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));

        assert_eq!(mgr.active_slot(), AgentSlot::Build);
        let next = mgr.switch();
        assert_eq!(next, AgentSlot::Plan);
        assert_eq!(mgr.active_slot(), AgentSlot::Plan);
        let next = mgr.switch();
        assert_eq!(next, AgentSlot::Build);
    }

    #[test]
    fn manager_switch_to() {
        let (plan, build) = make_test_handles();
        let mut mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));

        mgr.switch_to(AgentSlot::Plan);
        assert_eq!(mgr.active_slot(), AgentSlot::Plan);
        mgr.switch_to(AgentSlot::Plan); // no-op
        assert_eq!(mgr.active_slot(), AgentSlot::Plan);
        mgr.switch_to(AgentSlot::Build);
        assert_eq!(mgr.active_slot(), AgentSlot::Build);
    }

    #[test]
    fn manager_active_handle_matches_slot() {
        let (plan, build) = make_test_handles();
        let mut mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));

        // Default is Build
        assert_eq!(
            mgr.active_handle().session_id.as_str(),
            "build-session"
        );
        mgr.switch_to(AgentSlot::Plan);
        assert_eq!(
            mgr.active_handle().session_id.as_str(),
            "plan-session"
        );
    }

    #[test]
    fn manager_plan_and_build_handles() {
        let (plan, build) = make_test_handles();
        let mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));

        assert_eq!(mgr.plan_handle().session_id.as_str(), "plan-session");
        assert_eq!(mgr.build_handle().session_id.as_str(), "build-session");
    }

    #[test]
    fn manager_session_id() {
        let (plan, build) = make_test_handles();
        let mgr = DualAgentManager::new(plan, build, SessionId::from_string("shared"));
        assert_eq!(mgr.session_id().as_str(), "shared");
    }

    // -- Plan steps ----------------------------------------------------------

    #[test]
    fn manager_add_and_get_plan_steps() {
        let (plan, build) = make_test_handles();
        let mut mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));

        assert!(mgr.plan_steps().is_empty());

        mgr.add_plan_step(PlanStep {
            number: 1,
            description: "Read file".into(),
            completed: false,
        });
        mgr.add_plan_step(PlanStep {
            number: 2,
            description: "Edit file".into(),
            completed: false,
        });

        assert_eq!(mgr.plan_steps().len(), 2);
        assert_eq!(mgr.plan_steps()[0].description, "Read file");
        assert_eq!(mgr.plan_steps()[1].number, 2);
    }

    #[test]
    fn manager_complete_step() {
        let (plan, build) = make_test_handles();
        let mut mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));

        mgr.add_plan_step(PlanStep {
            number: 1,
            description: "Step A".into(),
            completed: false,
        });
        mgr.add_plan_step(PlanStep {
            number: 2,
            description: "Step B".into(),
            completed: false,
        });

        assert!(mgr.complete_step(1));
        assert!(mgr.plan_steps()[0].completed);
        assert!(!mgr.plan_steps()[1].completed);

        // Non-existent step returns false
        assert!(!mgr.complete_step(99));
    }

    #[test]
    fn manager_clear_steps() {
        let (plan, build) = make_test_handles();
        let mut mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));

        mgr.add_plan_step(PlanStep {
            number: 1,
            description: "X".into(),
            completed: false,
        });
        assert_eq!(mgr.plan_steps().len(), 1);
        mgr.clear_steps();
        assert!(mgr.plan_steps().is_empty());
    }

    // -- plan_context_string -------------------------------------------------

    #[test]
    fn plan_context_string_empty() {
        let (plan, build) = make_test_handles();
        let mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));
        assert_eq!(mgr.plan_context_string(), "");
    }

    #[test]
    fn plan_context_string_with_steps() {
        let (plan, build) = make_test_handles();
        let mut mgr = DualAgentManager::new(plan, build, SessionId::from_string("s1"));

        mgr.add_plan_step(PlanStep {
            number: 1,
            description: "Read".into(),
            completed: false,
        });
        mgr.add_plan_step(PlanStep {
            number: 2,
            description: "Write".into(),
            completed: true,
        });

        let ctx = mgr.plan_context_string();
        assert!(ctx.contains("## Plan Steps"));
        assert!(ctx.contains("- [ ] Step 1: Read"));
        assert!(ctx.contains("- [x] Step 2: Write"));
    }

    // -- parse_plan_steps ----------------------------------------------------

    #[test]
    fn parse_numbered_list() {
        let text = "1. Read the file\n2. Edit the code\n3. Run tests";
        let steps = DualAgentManager::parse_plan_steps(text);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].number, 1);
        assert_eq!(steps[0].description, "Read the file");
        assert_eq!(steps[2].description, "Run tests");
        assert!(!steps[0].completed);
    }

    #[test]
    fn parse_dash_bullet_list() {
        let text = "- First step\n- Second step";
        let steps = DualAgentManager::parse_plan_steps(text);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].description, "First step");
        assert_eq!(steps[1].number, 2);
    }

    #[test]
    fn parse_star_bullet_list() {
        let text = "* Alpha\n* Beta\n* Gamma";
        let steps = DualAgentManager::parse_plan_steps(text);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[2].description, "Gamma");
    }

    #[test]
    fn parse_mixed_formats() {
        let text = "Some preamble text\n\n1. First\n- Second\n* Third\n\nSome trailing text";
        let steps = DualAgentManager::parse_plan_steps(text);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].description, "First");
        assert_eq!(steps[1].description, "Second");
        assert_eq!(steps[2].description, "Third");
    }

    #[test]
    fn parse_empty_input() {
        let steps = DualAgentManager::parse_plan_steps("");
        assert!(steps.is_empty());
    }

    #[test]
    fn parse_no_steps() {
        let text = "This is just a paragraph.\nNo steps here.";
        let steps = DualAgentManager::parse_plan_steps(text);
        assert!(steps.is_empty());
    }

    #[test]
    fn parse_skips_empty_descriptions() {
        let text = "- \n- Real step\n- ";
        let steps = DualAgentManager::parse_plan_steps(text);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].description, "Real step");
    }
}
