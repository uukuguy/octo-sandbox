use octo_types::skill::{SkillDefinition, SkillTrigger, TrustLevel};

use super::trust::TrustManager;

/// Result of scoring a skill against user input.
#[derive(Debug, Clone)]
pub struct ScoredSkill {
    pub name: String,
    pub score: u32,
    pub estimated_tokens: usize,
}

/// A skill selected by the pipeline.
#[derive(Debug, Clone)]
pub struct SelectedSkill {
    pub name: String,
    pub score: u32,
    pub trust_level: TrustLevel,
}

/// Phase 1: Gate -- check if skill prerequisites are met.
pub struct SkillGate;

impl SkillGate {
    /// Returns true if the skill passes the gate.
    /// Currently all skills pass. Future: check required binaries, env vars, etc.
    pub fn passes(&self, _skill: &SkillDefinition) -> bool {
        true
    }
}

/// Phase 2: Score -- assign a numeric score to each skill.
pub struct SkillScorer;

impl SkillScorer {
    /// Score a skill against the given user message.
    ///
    /// Scoring rules:
    /// - `always: true`           -> 1000 (always included)
    /// - Slash command match      -> 900
    /// - Trigger::Command match   -> 800
    /// - Trigger::Keyword match   -> base + 10 per keyword hit
    /// - Trigger::FilePattern     -> base + 20
    /// - No trigger match         -> 0
    pub fn score(&self, skill: &SkillDefinition, user_message: &str) -> u32 {
        if skill.always {
            return 1000;
        }

        let msg_lower = user_message.to_lowercase();

        // Check slash command: /skill-name
        if msg_lower.starts_with(&format!("/{}", skill.name.to_lowercase())) {
            return 900;
        }

        let mut score = 0u32;

        for trigger in &skill.triggers {
            match trigger {
                SkillTrigger::Command { command } => {
                    if msg_lower.starts_with(&command.to_lowercase()) {
                        score = score.max(800);
                    }
                }
                SkillTrigger::Keyword { keyword } => {
                    if msg_lower.contains(&keyword.to_lowercase()) {
                        score += 10;
                    }
                }
                SkillTrigger::FilePattern { pattern } => {
                    let pat_lower = pattern.to_lowercase();
                    if msg_lower.contains(&pat_lower) {
                        score += 20;
                    }
                }
            }
        }

        score
    }
}

/// Phase 3: Budget -- select skills within token budget.
pub struct SkillBudget {
    max_tokens: usize,
}

impl SkillBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }

    /// Select skills from scored list, respecting token budget.
    /// Skills are taken in descending score order until budget runs out.
    /// Always-active skills (score >= 1000) are always included regardless of budget.
    #[allow(clippy::ptr_arg)]
    pub fn select(&self, scored: &mut Vec<ScoredSkill>) -> Vec<ScoredSkill> {
        scored.sort_by(|a, b| b.score.cmp(&a.score));

        let mut selected = Vec::new();
        let mut total_tokens = 0usize;

        for skill in scored.iter() {
            if skill.score == 0 {
                continue;
            }
            if skill.score >= 1000 {
                // Always include always-active skills.
                selected.push(skill.clone());
                total_tokens += skill.estimated_tokens;
                continue;
            }
            if total_tokens + skill.estimated_tokens <= self.max_tokens {
                selected.push(skill.clone());
                total_tokens += skill.estimated_tokens;
            }
        }

        selected
    }
}

/// Phase 4: Attenuate -- apply trust attenuation to selected skills.
pub struct SkillAttenuator {
    trust_manager: TrustManager,
}

impl SkillAttenuator {
    pub fn new(trust_manager: TrustManager) -> Self {
        Self { trust_manager }
    }

    /// Returns the effective trust level for a skill.
    pub fn effective_trust(&self, skill: &SkillDefinition) -> TrustLevel {
        self.trust_manager.effective_trust_level(skill)
    }
}

/// Unified 4-phase skill selection pipeline: Gate -> Score -> Budget -> Attenuate.
pub struct SkillSelector {
    gate: SkillGate,
    scorer: SkillScorer,
    budget: SkillBudget,
    attenuator: SkillAttenuator,
}

impl SkillSelector {
    pub fn new(max_skill_tokens: usize, trust_manager: TrustManager) -> Self {
        Self {
            gate: SkillGate,
            scorer: SkillScorer,
            budget: SkillBudget::new(max_skill_tokens),
            attenuator: SkillAttenuator::new(trust_manager),
        }
    }

    /// Run all 4 phases and return selected skills with their effective trust levels.
    pub fn select(&self, skills: &[SkillDefinition], user_message: &str) -> Vec<SelectedSkill> {
        // Phase 1: Gate
        let gated: Vec<&SkillDefinition> = skills.iter().filter(|s| self.gate.passes(s)).collect();

        // Phase 2: Score
        let mut scored: Vec<ScoredSkill> = gated
            .iter()
            .map(|s| ScoredSkill {
                name: s.name.clone(),
                score: self.scorer.score(s, user_message),
                estimated_tokens: estimate_skill_tokens(s),
            })
            .collect();

        // Phase 3: Budget
        let budgeted = self.budget.select(&mut scored);

        // Phase 4: Attenuate
        let mut result = Vec::new();
        for entry in budgeted {
            if let Some(skill) = skills.iter().find(|s| s.name == entry.name) {
                let trust = self.attenuator.effective_trust(skill);
                result.push(SelectedSkill {
                    name: entry.name,
                    score: entry.score,
                    trust_level: trust,
                });
            }
        }

        result
    }
}

/// Estimate token cost of including a skill in the context.
fn estimate_skill_tokens(skill: &SkillDefinition) -> usize {
    let body_tokens = skill.body.len() / 4;
    let desc_tokens = skill.description.len() / 4;
    body_tokens + desc_tokens + 20 // overhead for formatting
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_skill_tokens() {
        let mut skill = make_skill("s");
        skill.body = "a".repeat(100);
        skill.description = "b".repeat(40);
        // 100/4 + 40/4 + 20 = 25 + 10 + 20 = 55
        assert_eq!(estimate_skill_tokens(&skill), 55);
    }

    fn make_skill(name: &str) -> SkillDefinition {
        SkillDefinition {
            name: name.into(),
            description: "test skill".into(),
            version: None,
            user_invocable: false,
            allowed_tools: None,
            body: String::new(),
            base_dir: std::path::PathBuf::new(),
            source_path: std::path::PathBuf::new(),
            body_loaded: false,
            model: None,
            context_fork: false,
            always: false,
            trust_level: TrustLevel::Installed,
            triggers: vec![],
            dependencies: vec![],
            tags: vec![],
            denied_tools: None,
            execution_mode: Default::default(),
            source_type: octo_types::skill::SkillSourceType::ProjectLocal,
            max_rounds: 0,
            background: false,
        }
    }
}
