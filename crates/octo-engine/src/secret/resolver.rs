//! Credential Resolver - Resolve credentials from multiple sources
//!
//! Resolves credentials from Vault, .env files, and environment variables
//! with priority: Vault > .env > Environment

use crate::secret::vault::CredentialVault;
use regex::Regex;
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
            self.resolve(key)
                .map(|v| v.to_string())
                .unwrap_or_default()
        }).to_string()
    }

    fn read_dotenv(&self, _path: &PathBuf, key: &str) -> Result<String, std::env::VarError> {
        // Simplified: use environment variable lookup
        // Full implementation would parse .env file
        std::env::var(key)
    }
}

impl Default for CredentialResolver {
    fn default() -> Self {
        Self::new()
    }
}
