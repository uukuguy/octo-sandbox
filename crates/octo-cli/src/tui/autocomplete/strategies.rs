//! Completion scoring and ranking strategies.
//!
//! Supports prefix matching, fuzzy matching, and frecency-weighted ranking.

use std::collections::HashMap;
use std::time::Instant;

use super::CompletionItem;

// ── Frecency tracker ───────────────────────────────────────────────

#[derive(Debug)]
struct FrecencyEntry {
    count: u32,
    last_access: Instant,
}

/// Manages frecency data for completion items.
#[derive(Debug)]
pub struct FrecencyTracker {
    entries: HashMap<String, FrecencyEntry>,
}

impl FrecencyTracker {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    pub fn record(&mut self, key: &str) {
        let entry = self.entries.entry(key.to_string()).or_insert(FrecencyEntry {
            count: 0,
            last_access: Instant::now(),
        });
        entry.count += 1;
        entry.last_access = Instant::now();
    }

    pub fn score(&self, key: &str) -> f64 {
        match self.entries.get(key) {
            None => 0.0,
            Some(entry) => {
                let elapsed_secs = entry.last_access.elapsed().as_secs_f64();
                let recency = (-elapsed_secs / 300.0).exp();
                entry.count as f64 * recency
            }
        }
    }
}

impl Default for FrecencyTracker {
    fn default() -> Self { Self::new() }
}

// ── Fuzzy matching ─────────────────────────────────────────────────

/// Simple fuzzy-match scoring. Returns 0.0 if no match.
pub fn fuzzy_score(pattern: &str, text: &str) -> f64 {
    if pattern.is_empty() {
        return 1.0;
    }
    let pattern_lower: Vec<char> = pattern.to_lowercase().chars().collect();
    let text_lower: Vec<char> = text.to_lowercase().chars().collect();

    let mut pi = 0;
    let mut consecutive = 0u32;
    let mut total_bonus = 0.0f64;

    for (ti, &tc) in text_lower.iter().enumerate() {
        if pi < pattern_lower.len() && tc == pattern_lower[pi] {
            if ti == 0
                || matches!(
                    text_lower.get(ti.wrapping_sub(1)),
                    Some(&'/' | &'_' | &'-' | &'.')
                )
            {
                total_bonus += 0.15;
            }
            consecutive += 1;
            total_bonus += consecutive as f64 * 0.05;
            pi += 1;
        } else {
            consecutive = 0;
        }
    }

    if pi != pattern_lower.len() {
        return 0.0;
    }

    let base = pattern_lower.len() as f64 / text_lower.len().max(1) as f64;
    (base + total_bonus).min(1.0)
}

// ── CompletionStrategy ─────────────────────────────────────────────

/// The matching mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    Prefix,
    Fuzzy,
}

/// Configurable strategy for scoring and sorting completion items.
pub struct CompletionStrategy {
    mode: MatchMode,
    frecency: FrecencyTracker,
    frecency_weight: f64,
}

impl CompletionStrategy {
    pub fn new(mode: MatchMode) -> Self {
        Self {
            mode,
            frecency: FrecencyTracker::new(),
            frecency_weight: 5.0,
        }
    }

    pub fn record_access(&mut self, key: &str) {
        self.frecency.record(key);
    }

    pub fn sort(&self, items: &mut [CompletionItem]) {
        for item in items.iter_mut() {
            let frecency = self.frecency.score(&item.insert_text) * self.frecency_weight;
            let match_score = match self.mode {
                MatchMode::Prefix | MatchMode::Fuzzy => {
                    1.0 / (item.label.len() as f64 + 1.0)
                }
            };
            item.score = match_score + frecency;
        }
        items.sort_by(|a, b| {
            b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

impl Default for CompletionStrategy {
    fn default() -> Self { Self::new(MatchMode::Prefix) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::autocomplete::CompletionKind;

    #[test]
    fn test_fuzzy_score_exact() {
        let score = fuzzy_score("help", "help");
        assert!(score > 0.5);
    }

    #[test]
    fn test_fuzzy_score_prefix() {
        let score = fuzzy_score("hel", "help");
        assert!(score > 0.3);
    }

    #[test]
    fn test_fuzzy_score_no_match() {
        assert_eq!(fuzzy_score("xyz", "help"), 0.0);
    }

    #[test]
    fn test_fuzzy_score_empty_pattern() {
        assert_eq!(fuzzy_score("", "anything"), 1.0);
    }

    #[test]
    fn test_frecency_new_key() {
        let tracker = FrecencyTracker::new();
        assert_eq!(tracker.score("unknown"), 0.0);
    }

    #[test]
    fn test_frecency_after_access() {
        let mut tracker = FrecencyTracker::new();
        tracker.record("foo");
        assert!(tracker.score("foo") > 0.0);
    }

    #[test]
    fn test_strategy_sort_by_label_length() {
        let strategy = CompletionStrategy::default();
        let mut items = vec![
            CompletionItem {
                insert_text: "/session-models".into(),
                label: "/session-models".into(),
                description: String::new(),
                kind: CompletionKind::Command,
                score: 0.0,
            },
            CompletionItem {
                insert_text: "/help".into(),
                label: "/help".into(),
                description: String::new(),
                kind: CompletionKind::Command,
                score: 0.0,
            },
        ];
        strategy.sort(&mut items);
        assert_eq!(items[0].label, "/help");
    }

    #[test]
    fn test_strategy_frecency_boost() {
        let mut strategy = CompletionStrategy::default();
        strategy.record_access("/exit");
        let mut items = vec![
            CompletionItem {
                insert_text: "/help".into(),
                label: "/help".into(),
                description: String::new(),
                kind: CompletionKind::Command,
                score: 0.0,
            },
            CompletionItem {
                insert_text: "/exit".into(),
                label: "/exit".into(),
                description: String::new(),
                kind: CompletionKind::Command,
                score: 0.0,
            },
        ];
        strategy.sort(&mut items);
        assert_eq!(items[0].label, "/exit");
    }
}
