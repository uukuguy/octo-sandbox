//! Credential Resolver - Resolve credentials from multiple sources
//!
//! Resolves credentials from Vault, .env files, and environment variables
//! with priority: Vault > .env > Environment

use crate::secret::vault::CredentialVault;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use zeroize::Zeroizing;

/// Credential resolver with priority chain
pub struct CredentialResolver {
    /// Vault for secure credential storage
    vault: Option<CredentialVault>,
    /// Path to .env file
    dotenv_path: Option<PathBuf>,
}

impl CredentialResolver {
    /// Create a new resolver
    pub fn new() -> Self {
        Self {
            vault: None,
            dotenv_path: None,
        }
    }

    /// Set vault for resolution
    pub fn with_vault(mut self, vault: CredentialVault) -> Self {
        self.vault = Some(vault);
        self
    }

    /// Set .env file path
    pub fn with_dotenv(mut self, path: PathBuf) -> Self {
        self.dotenv_path = Some(path);
        self
    }

    /// Resolve a single key from the priority chain
    pub fn resolve(&self, key: &str) -> Option<Zeroizing<String>> {
        // 1. Vault (highest priority)
        if let Some(ref v) = self.vault {
            if let Some(val) = v.get(key) {
                return Some(val);
            }
        }

        // 2. .env file
        if let Some(ref path) = self.dotenv_path {
            if let Ok(val) = self.read_dotenv(path, key) {
                return Some(Zeroizing::new(val));
            }
        }

        // 3. Environment variables (lowest priority)
        if let Ok(val) = std::env::var(key) {
            return Some(Zeroizing::new(val));
        }

        None
    }

    /// Resolve ${SECRET:key} syntax in configuration strings
    pub fn resolve_config(&self, config: &str) -> String {
        let re = Regex::new(r"\$\{SECRET:([^}]+)\}").unwrap();

        re.replace_all(config, |caps: &regex::Captures| {
            let key = &caps[1];
            self.resolve(key).map(|v| v.to_string()).unwrap_or_default()
        })
        .to_string()
    }

    /// Read and parse a .env file to get a specific key
    ///
    /// Supports standard .env format:
    /// - KEY=value
    /// - KEY="quoted value"
    /// - KEY='single quoted'
    /// - # comments
    /// - export prefix
    fn read_dotenv(&self, path: &PathBuf, key: &str) -> Result<String, std::env::VarError> {
        use std::fs;

        // Parse .env file and cache in memory
        let content = fs::read_to_string(path).map_err(|_| std::env::VarError::NotPresent)?;
        let vars = Self::parse_dotenv(&content);

        // Look up the key
        vars.get(key).cloned().ok_or(std::env::VarError::NotPresent)
    }

    /// Parse .env file content into key-value pairs
    pub(crate) fn parse_dotenv(content: &str) -> HashMap<String, String> {
        let mut result = HashMap::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Remove export prefix if present
            let line = line
                .trim_start_matches("export ")
                .trim_start_matches("export\t");

            // Find the first = sign
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim().to_string();
                let mut value = line[eq_pos + 1..].trim().to_string();

                // Handle quoted values
                if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    if value.len() >= 2 {
                        value = value[1..value.len() - 1].to_string();
                    }
                }

                // Handle escape sequences in double-quoted values
                if value.starts_with('"') {
                    value = value
                        .replace("\\n", "\n")
                        .replace("\\t", "\t")
                        .replace("\\\"", "\"")
                        .replace("\\\\", "\\");
                }

                if !key.is_empty() {
                    result.insert(key, value);
                }
            }
        }

        result
    }
}

impl Default for CredentialResolver {
    fn default() -> Self {
        Self::new()
    }
}
