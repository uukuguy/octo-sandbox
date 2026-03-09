use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A scored match from semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMatch {
    pub skill_name: String,
    pub score: f64,
    pub matched_terms: Vec<String>,
}

/// Simple keyword-based semantic index for skills.
/// Uses TF-IDF-like scoring for skill matching.
pub struct SkillSemanticIndex {
    /// skill_name -> (term -> weight)
    index: HashMap<String, HashMap<String, f64>>,
}

impl SkillSemanticIndex {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    /// Index a skill with its descriptive terms and weights
    pub fn add_skill(&mut self, name: &str, description: &str, tags: &[String]) {
        let mut terms: HashMap<String, f64> = HashMap::new();

        // Tokenize description
        for word in description.split_whitespace() {
            let word = word.to_lowercase();
            if word.len() >= 3 {
                *terms.entry(word).or_default() += 1.0;
            }
        }

        // Tags get higher weight
        for tag in tags {
            let tag_lower = tag.to_lowercase();
            *terms.entry(tag_lower).or_default() += 3.0;
        }

        // Name terms get highest weight
        for word in name.split(&['-', '_', '.'][..]) {
            let word = word.to_lowercase();
            if word.len() >= 2 {
                *terms.entry(word).or_default() += 5.0;
            }
        }

        self.index.insert(name.to_string(), terms);
    }

    /// Remove a skill from the index
    pub fn remove_skill(&mut self, name: &str) {
        self.index.remove(name);
    }

    /// Search for skills matching a query string
    pub fn search(&self, query: &str, limit: usize) -> Vec<SemanticMatch> {
        let query_terms: Vec<String> = query
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() >= 2)
            .collect();

        if query_terms.is_empty() {
            return Vec::new();
        }

        let mut matches: Vec<SemanticMatch> = self
            .index
            .iter()
            .filter_map(|(skill_name, terms)| {
                let mut score = 0.0;
                let mut matched = Vec::new();

                for qt in &query_terms {
                    for (term, weight) in terms {
                        if term.contains(qt.as_str()) || qt.contains(term.as_str()) {
                            score += weight;
                            if !matched.contains(term) {
                                matched.push(term.clone());
                            }
                        }
                    }
                }

                if score > 0.0 {
                    Some(SemanticMatch {
                        skill_name: skill_name.clone(),
                        score,
                        matched_terms: matched,
                    })
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches.truncate(limit);
        matches
    }

    /// Get number of indexed skills
    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

impl Default for SkillSemanticIndex {
    fn default() -> Self {
        Self::new()
    }
}
