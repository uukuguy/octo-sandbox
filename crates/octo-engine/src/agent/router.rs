//! Agent router — matches task descriptions to the best agent based on capabilities

use super::capability::AgentCapability;
use serde::{Deserialize, Serialize};

/// Result of routing a task to an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResult {
    /// Recommended agent ID
    pub agent_id: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Reason for the routing decision
    pub reason: String,
    /// Alternative agents ranked by confidence
    pub alternatives: Vec<RouteAlternative>,
}

/// An alternative agent candidate from routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteAlternative {
    pub agent_id: String,
    pub confidence: f64,
}

/// Registration of an agent's capabilities
#[derive(Debug, Clone)]
pub struct AgentProfile {
    pub agent_id: String,
    pub capabilities: Vec<AgentCapability>,
    /// Lower value = higher priority for tie-breaking
    pub priority: u32,
}

/// Routes tasks to the best matching agent based on capability keywords
#[derive(Debug, Default)]
pub struct AgentRouter {
    profiles: Vec<AgentProfile>,
}

impl AgentRouter {
    pub fn new() -> Self {
        Self {
            profiles: Vec::new(),
        }
    }

    /// Register an agent's capabilities
    pub fn register(&mut self, profile: AgentProfile) {
        self.profiles.push(profile);
    }

    /// Remove a registered agent by ID
    pub fn unregister(&mut self, agent_id: &str) {
        self.profiles.retain(|p| p.agent_id != agent_id);
    }

    /// Route a task description to the best agent.
    ///
    /// Returns `None` only when no agents are registered.
    /// When agents exist but no keywords match, falls back to
    /// the first registered agent with low confidence.
    pub fn route(&self, task: &str) -> Option<RouteResult> {
        if self.profiles.is_empty() {
            return None;
        }

        let task_lower = task.to_lowercase();
        let mut scores: Vec<(usize, f64, Vec<String>)> = Vec::new();

        for (idx, profile) in self.profiles.iter().enumerate() {
            let mut score = 0.0_f64;
            let mut matched = Vec::new();

            for cap in &profile.capabilities {
                for keyword in cap.keywords() {
                    if task_lower.contains(&keyword.to_lowercase()) {
                        score += 1.0;
                        matched.push(keyword.to_string());
                    }
                }
            }

            if score > 0.0 {
                // Normalize: min 0.3, max 0.95
                let confidence = (0.3 + score * 0.15).min(0.95);
                scores.push((idx, confidence, matched));
            }
        }

        if scores.is_empty() {
            // Default: return first profile with low confidence
            let profile = &self.profiles[0];
            return Some(RouteResult {
                agent_id: profile.agent_id.clone(),
                confidence: 0.3,
                reason: "Default routing - no keyword match".to_string(),
                alternatives: vec![],
            });
        }

        // Sort by score descending, then by priority (lower = better)
        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    self.profiles[a.0]
                        .priority
                        .cmp(&self.profiles[b.0].priority)
                })
        });

        let (best_idx, best_score, ref best_matched) = scores[0];
        let best = &self.profiles[best_idx];

        let alternatives = scores[1..scores.len().min(4)]
            .iter()
            .map(|(idx, score, _)| RouteAlternative {
                agent_id: self.profiles[*idx].agent_id.clone(),
                confidence: *score,
            })
            .collect();

        Some(RouteResult {
            agent_id: best.agent_id.clone(),
            confidence: best_score,
            reason: format!("Matched keywords: {}", best_matched.join(", ")),
            alternatives,
        })
    }

    /// Get all registered profiles
    pub fn profiles(&self) -> &[AgentProfile] {
        &self.profiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_router() -> AgentRouter {
        let mut router = AgentRouter::new();
        router.register(AgentProfile {
            agent_id: "coder-1".to_string(),
            capabilities: vec![AgentCapability::CodeGeneration, AgentCapability::Debugging],
            priority: 1,
        });
        router.register(AgentProfile {
            agent_id: "reviewer-1".to_string(),
            capabilities: vec![AgentCapability::CodeReview, AgentCapability::SecurityAudit],
            priority: 2,
        });
        router.register(AgentProfile {
            agent_id: "devops-1".to_string(),
            capabilities: vec![AgentCapability::DevOps, AgentCapability::BackendDev],
            priority: 3,
        });
        router
    }

    #[test]
    fn test_empty_router_returns_none() {
        let router = AgentRouter::new();
        assert!(router.route("implement a feature").is_none());
    }

    #[test]
    fn test_routes_to_coder() {
        let router = make_router();
        let result = router.route("implement a new authentication module").unwrap();
        assert_eq!(result.agent_id, "coder-1");
        assert!(result.confidence > 0.3);
    }

    #[test]
    fn test_routes_to_reviewer() {
        let router = make_router();
        let result = router.route("review the security vulnerability in auth").unwrap();
        assert_eq!(result.agent_id, "reviewer-1");
    }

    #[test]
    fn test_routes_to_devops() {
        let router = make_router();
        let result = router.route("deploy the api server with docker").unwrap();
        assert_eq!(result.agent_id, "devops-1");
    }

    #[test]
    fn test_fallback_on_no_match() {
        let router = make_router();
        let result = router.route("something completely unrelated xyz").unwrap();
        assert_eq!(result.agent_id, "coder-1"); // first registered
        assert!((result.confidence - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn test_unregister() {
        let mut router = make_router();
        assert_eq!(router.profiles().len(), 3);
        router.unregister("coder-1");
        assert_eq!(router.profiles().len(), 2);
    }

    #[test]
    fn test_alternatives_populated() {
        let router = make_router();
        // "fix the bug in the api endpoint" should match coder (fix, bug) and devops (api, endpoint)
        let result = router.route("fix the bug in the api endpoint").unwrap();
        assert!(!result.alternatives.is_empty());
    }

    #[test]
    fn test_confidence_capped_at_095() {
        let mut router = AgentRouter::new();
        router.register(AgentProfile {
            agent_id: "super".to_string(),
            capabilities: vec![
                AgentCapability::CodeGeneration,
                AgentCapability::Debugging,
                AgentCapability::CodeReview,
                AgentCapability::Testing,
                AgentCapability::BackendDev,
            ],
            priority: 1,
        });
        let result = router
            .route("implement build add write fix error bug test api server database")
            .unwrap();
        assert!(result.confidence <= 0.95);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let router = make_router();
        let result = router.route("implement a feature").unwrap();
        let json = serde_json::to_string(&result).unwrap();
        let decoded: RouteResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.agent_id, result.agent_id);
    }
}
