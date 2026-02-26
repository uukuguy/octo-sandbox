use octo_types::TokenBudget;

pub struct TokenBudgetManager {
    budget: TokenBudget,
}

impl TokenBudgetManager {
    pub fn new(budget: TokenBudget) -> Self {
        Self { budget }
    }

    /// Estimate tokens from character count (chars / 4 approximation)
    pub fn estimate_tokens(text: &str) -> u32 {
        (text.len() as u32) / 4
    }

    pub fn remaining_for_messages(&self, system_prompt: &str) -> u32 {
        let system_tokens = Self::estimate_tokens(system_prompt);
        let used = system_tokens + self.budget.completion;
        self.budget.total.saturating_sub(used)
    }

    pub fn budget(&self) -> &TokenBudget {
        &self.budget
    }
}

impl Default for TokenBudgetManager {
    fn default() -> Self {
        Self::new(TokenBudget::default())
    }
}
