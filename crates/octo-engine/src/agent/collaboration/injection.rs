//! Collaboration context injection — builds a text summary of collaboration state
//! for inclusion in an agent's system prompt.

use super::context::CollaborationContext;

/// Build a collaboration context string to inject into an agent's system prompt.
///
/// Returns an empty string when there is nothing to report, so callers can
/// safely append the result without conditional checks.
pub fn build_collaboration_injection(context: &CollaborationContext) -> String {
    let events = context.events();
    let proposals = context.proposals();
    let state_keys = context.state_keys();

    if events.is_empty() && proposals.is_empty() && state_keys.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();
    parts.push("\n## Collaboration Context\n".to_string());

    if !state_keys.is_empty() {
        parts.push("### Shared State\n".to_string());
        for key in &state_keys {
            if let Some(val) = context.get_state(key) {
                parts.push(format!("- **{}**: {}\n", key, val));
            }
        }
    }

    if !proposals.is_empty() {
        parts.push("\n### Active Proposals\n".to_string());
        for p in &proposals {
            parts.push(format!(
                "- [{:?}] {} (from {}): {}\n",
                p.status, p.action, p.from_agent, p.description
            ));
        }
    }

    // Show last N events (not all — could be huge)
    let recent_events: Vec<_> = events.iter().rev().take(10).collect();
    if !recent_events.is_empty() {
        parts.push("\n### Recent Activity\n".to_string());
        for event in recent_events.iter().rev() {
            parts.push(format!("- {:?}\n", event));
        }
    }

    parts.join("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::capability::AgentCapability;
    use crate::agent::collaboration::context::{
        CollaborationEvent, Proposal, ProposalStatus, Vote,
    };

    fn make_context() -> CollaborationContext {
        CollaborationContext::new("test".to_string())
    }

    #[test]
    fn empty_context_produces_empty_string() {
        let ctx = make_context();
        let result = build_collaboration_injection(&ctx);
        assert!(result.is_empty());
    }

    #[test]
    fn injection_with_shared_state() {
        let ctx = make_context();
        ctx.set_state("task".to_string(), serde_json::json!("build API"));
        ctx.set_state("status".to_string(), serde_json::json!("in_progress"));

        let result = build_collaboration_injection(&ctx);
        assert!(result.contains("## Collaboration Context"));
        assert!(result.contains("### Shared State"));
        assert!(result.contains("**task**"));
        assert!(result.contains("build API"));
        assert!(result.contains("**status**"));
        assert!(result.contains("in_progress"));
    }

    #[test]
    fn injection_with_proposals() {
        let ctx = make_context();
        ctx.add_proposal(Proposal {
            id: "p1".to_string(),
            from_agent: "coder".to_string(),
            action: "refactor".to_string(),
            description: "Refactor auth module".to_string(),
            status: ProposalStatus::Pending,
            votes: vec![],
        });

        let result = build_collaboration_injection(&ctx);
        assert!(result.contains("### Active Proposals"));
        assert!(result.contains("[Pending]"));
        assert!(result.contains("refactor"));
        assert!(result.contains("coder"));
        assert!(result.contains("Refactor auth module"));
    }

    #[test]
    fn injection_with_events() {
        let ctx = make_context();
        ctx.log_event(CollaborationEvent::AgentJoined {
            agent_id: "a1".to_string(),
            capabilities: vec![AgentCapability::CodeGeneration],
        });
        ctx.log_event(CollaborationEvent::MessageSent {
            from: "a1".to_string(),
            to: "a2".to_string(),
            content: "hello".to_string(),
        });

        let result = build_collaboration_injection(&ctx);
        assert!(result.contains("### Recent Activity"));
        assert!(result.contains("AgentJoined"));
        assert!(result.contains("MessageSent"));
    }

    #[test]
    fn injection_with_mixed_content() {
        let ctx = make_context();

        // State
        ctx.set_state("goal".to_string(), serde_json::json!("ship v2"));

        // Proposal
        ctx.add_proposal(Proposal {
            id: "p1".to_string(),
            from_agent: "planner".to_string(),
            action: "deploy".to_string(),
            description: "Deploy to staging".to_string(),
            status: ProposalStatus::Accepted,
            votes: vec![Vote {
                agent_id: "reviewer".to_string(),
                approve: true,
                reason: None,
            }],
        });

        // Events
        ctx.log_event(CollaborationEvent::TaskDelegated {
            from: "planner".to_string(),
            to: "coder".to_string(),
            task: "implement feature".to_string(),
        });

        let result = build_collaboration_injection(&ctx);
        assert!(result.contains("### Shared State"));
        assert!(result.contains("### Active Proposals"));
        assert!(result.contains("### Recent Activity"));
        assert!(result.contains("goal"));
        assert!(result.contains("[Accepted]"));
        assert!(result.contains("TaskDelegated"));
    }

    #[test]
    fn injection_limits_events_to_ten() {
        let ctx = make_context();
        for i in 0..20 {
            ctx.log_event(CollaborationEvent::StateUpdated {
                agent_id: format!("a{}", i),
                key: format!("k{}", i),
            });
        }

        let result = build_collaboration_injection(&ctx);
        // Should contain "Recent Activity" but only 10 events
        let event_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.starts_with("- StateUpdated"))
            .collect();
        assert_eq!(event_lines.len(), 10);
    }
}
