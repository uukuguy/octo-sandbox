use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A skill entry in the remote catalog
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Unique skill identifier (e.g., "org/skill-name")
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Version (semver)
    pub version: String,
    /// Author
    pub author: Option<String>,
    /// Tags for search
    pub tags: Vec<String>,
    /// Download URL
    pub url: Option<String>,
    /// SHA-256 checksum for verification
    pub checksum: Option<String>,
    /// Runtime type (python, nodejs, wasm, shell)
    pub runtime: Option<String>,
    /// Number of downloads (popularity)
    pub downloads: u64,
}

/// Search criteria for the catalog
#[derive(Debug, Clone, Default)]
pub struct CatalogQuery {
    /// Text query (searches name, description, tags)
    pub query: Option<String>,
    /// Filter by tags
    pub tags: Vec<String>,
    /// Filter by runtime type
    pub runtime: Option<String>,
    /// Maximum results
    pub limit: usize,
}

impl CatalogQuery {
    pub fn new() -> Self {
        Self {
            limit: 20,
            ..Default::default()
        }
    }

    pub fn with_query(mut self, q: impl Into<String>) -> Self {
        self.query = Some(q.into());
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_runtime(mut self, rt: impl Into<String>) -> Self {
        self.runtime = Some(rt.into());
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// Local skill catalog with optional remote registry support.
pub struct SkillCatalog {
    /// Local cache of catalog entries
    entries: HashMap<String, CatalogEntry>,
    /// Remote registry URL (if configured)
    registry_url: Option<String>,
}

impl SkillCatalog {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            registry_url: None,
        }
    }

    pub fn with_registry(mut self, url: impl Into<String>) -> Self {
        self.registry_url = Some(url.into());
        self
    }

    /// Add a local entry to the catalog
    pub fn add_entry(&mut self, entry: CatalogEntry) {
        self.entries.insert(entry.id.clone(), entry);
    }

    /// Search the local catalog
    pub fn search(&self, query: &CatalogQuery) -> Vec<&CatalogEntry> {
        let mut results: Vec<&CatalogEntry> = self
            .entries
            .values()
            .filter(|entry| {
                // Text query match
                if let Some(ref q) = query.query {
                    let q_lower = q.to_lowercase();
                    let matches = entry.name.to_lowercase().contains(&q_lower)
                        || entry.description.to_lowercase().contains(&q_lower)
                        || entry
                            .tags
                            .iter()
                            .any(|t| t.to_lowercase().contains(&q_lower));
                    if !matches {
                        return false;
                    }
                }
                // Tag filter
                if !query.tags.is_empty() {
                    let has_tag = query.tags.iter().any(|qt| entry.tags.contains(qt));
                    if !has_tag {
                        return false;
                    }
                }
                // Runtime filter
                if let Some(ref rt) = query.runtime {
                    if entry.runtime.as_deref() != Some(rt.as_str()) {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Sort by downloads (popularity)
        results.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        results.truncate(query.limit);
        results
    }

    /// Get a specific entry by ID
    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.get(id)
    }

    /// Remove an entry
    pub fn remove(&mut self, id: &str) -> Option<CatalogEntry> {
        self.entries.remove(id)
    }

    /// List all entries
    pub fn list(&self) -> Vec<&CatalogEntry> {
        self.entries.values().collect()
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if catalog is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Check if registry is configured
    pub fn has_registry(&self) -> bool {
        self.registry_url.is_some()
    }

    /// Get registry URL
    pub fn registry_url(&self) -> Option<&str> {
        self.registry_url.as_deref()
    }
}

impl Default for SkillCatalog {
    fn default() -> Self {
        Self::new()
    }
}
