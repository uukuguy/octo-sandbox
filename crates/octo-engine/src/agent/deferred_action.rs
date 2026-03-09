/// Detects deferred/postponed actions in LLM responses.
/// These are phrases where the LLM promises future action instead of acting now.

#[derive(Debug, Clone)]
pub struct DeferredPattern {
    pub pattern: String,
    pub category: DeferredCategory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeferredCategory {
    /// "I'll do X later / in the next step"
    PostponedAction,
    /// "Let me come back to X"
    DeferredReturn,
    /// "We can handle X separately"
    ScopeDefer,
}

#[derive(Debug, Clone)]
pub struct DeferredActionMatch {
    pub text: String,
    pub category: DeferredCategory,
    pub offset: usize,
}

pub struct DeferredActionDetector {
    patterns: Vec<DeferredPattern>,
}

impl DeferredActionDetector {
    /// Create a detector with built-in default patterns.
    pub fn new() -> Self {
        let patterns = vec![
            // PostponedAction
            dp("i'll handle that later", DeferredCategory::PostponedAction),
            dp(
                "i'll do that in the next",
                DeferredCategory::PostponedAction,
            ),
            dp("i will address that", DeferredCategory::PostponedAction),
            dp("let's skip that for now", DeferredCategory::PostponedAction),
            dp("we'll tackle that", DeferredCategory::PostponedAction),
            // DeferredReturn
            dp("let me come back to", DeferredCategory::DeferredReturn),
            dp("i'll revisit", DeferredCategory::DeferredReturn),
            dp("we can circle back", DeferredCategory::DeferredReturn),
            dp("i'll return to", DeferredCategory::DeferredReturn),
            // ScopeDefer
            dp(
                "we can handle that separately",
                DeferredCategory::ScopeDefer,
            ),
            dp("that's out of scope for now", DeferredCategory::ScopeDefer),
            dp("let's defer that", DeferredCategory::ScopeDefer),
            dp("we'll save that for", DeferredCategory::ScopeDefer),
        ];
        Self { patterns }
    }

    /// Create a detector with custom patterns only.
    pub fn with_patterns(patterns: Vec<DeferredPattern>) -> Self {
        Self { patterns }
    }

    /// Scan `text` for all matching deferred-action phrases.
    pub fn detect(&self, text: &str) -> Vec<DeferredActionMatch> {
        let lower = text.to_lowercase();
        let mut matches = Vec::new();

        for dp in &self.patterns {
            let needle = &dp.pattern; // already lowercase from constructor helpers
            let mut start = 0;
            while let Some(pos) = lower[start..].find(needle) {
                let abs = start + pos;
                let end = abs + needle.len();
                matches.push(DeferredActionMatch {
                    text: text[abs..end].to_string(),
                    category: dp.category.clone(),
                    offset: abs,
                });
                start = end;
            }
        }

        // Stable sort by offset so callers get matches in document order.
        matches.sort_by_key(|m| m.offset);
        matches
    }

    /// Convenience: returns `true` if any deferred action is found.
    pub fn has_deferred(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        self.patterns.iter().any(|dp| lower.contains(&dp.pattern))
    }
}

impl Default for DeferredActionDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to build a `DeferredPattern` with a lowercase needle.
fn dp(pattern: &str, category: DeferredCategory) -> DeferredPattern {
    DeferredPattern {
        pattern: pattern.to_lowercase(),
        category,
    }
}
