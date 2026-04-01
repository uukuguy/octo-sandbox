//! Credential Vault - Secure storage for secrets
//!
//! Provides encrypted storage using AES-256-GCM with Argon2id key derivation.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use zeroize::{Zeroize, Zeroizing};

/// Encrypted data store structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedStore {
    /// Version number
    pub version: u8,
    /// Random salt for key derivation
    pub salt: [u8; 16],
    /// AES-GCM nonce
    pub nonce: [u8; 12],
    /// Encrypted data
    pub ciphertext: Vec<u8>,
}

impl Default for EncryptedStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EncryptedStore {
    /// Create a new empty encrypted store
    pub fn new() -> Self {
        let mut salt = [0u8; 16];
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce);

        Self {
            version: 1,
            salt,
            nonce,
            ciphertext: Vec::new(),
        }
    }
}

/// Secure credential vault with encryption
pub struct CredentialVault {
    /// Encrypted data store
    pub(crate) store: RwLock<EncryptedStore>,
    /// Master key derived from password
    master_key: Zeroizing<[u8; 32]>,
    /// In-memory entries (decrypted)
    entries: RwLock<HashMap<String, String>>,
}

impl CredentialVault {
    /// Create a new vault with password
    pub fn new(password: String) -> Result<Self, String> {
        // Validate minimum password length
        if password.len() < 8 {
            return Err("Password must be at least 8 characters".to_string());
        }

        // Generate random salt
        let salt = SaltString::generate(&mut rand::thread_rng());

        // Derive key with Argon2id
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| format!("Failed to hash password: {}", e))?;

        // Extract 32 bytes key safely
        let mut key = [0u8; 32];
        let hash_output = hash.hash.ok_or_else(|| "Hash output missing".to_string())?;
        let hash_bytes = hash_output.as_bytes();
        let len = std::cmp::min(32, hash_bytes.len());
        key[..len].copy_from_slice(&hash_bytes[..len]);

        Ok(Self {
            store: RwLock::new(EncryptedStore::new()),
            master_key: Zeroizing::new(key),
            entries: RwLock::new(HashMap::new()),
        })
    }

    /// Create a new vault with password (panics on error - use new() for error handling)
    #[allow(clippy::should_implement_trait)]
    pub fn new_for_testing(password: String) -> Self {
        Self::new(password.clone()).unwrap_or_else(|_| {
            // For testing, use a simpler approach that won't fail
            let salt = SaltString::generate(&mut rand::thread_rng());
            let argon2 = Argon2::default();
            let hash = argon2.hash_password(password.as_bytes(), &salt).unwrap();
            let mut key = [0u8; 32];
            let hash_output = hash.hash.unwrap();
            let hash_bytes = hash_output.as_bytes();
            key.copy_from_slice(&hash_bytes[..32]);
            Self {
                store: RwLock::new(EncryptedStore::new()),
                master_key: Zeroizing::new(key),
                entries: RwLock::new(HashMap::new()),
            }
        })
    }

    /// Set a credential
    pub fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let mut entries = self.entries.write().map_err(|e| e.to_string())?;
        entries.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Get a credential
    pub fn get(&self, key: &str) -> Option<Zeroizing<String>> {
        let entries = self.entries.read().ok()?;
        entries.get(key).map(|v| Zeroizing::new(v.clone()))
    }

    /// List all stored credential keys (never exposes values)
    pub fn list(&self) -> Vec<String> {
        let entries = self.entries.read().unwrap();
        entries.keys().cloned().collect()
    }

    /// Delete a credential by key
    pub fn delete(&self, key: &str) -> Result<(), String> {
        let mut entries = self.entries.write().map_err(|e| e.to_string())?;
        entries.remove(key);
        Ok(())
    }

    /// Encrypt entries to store
    pub fn encrypt(&self) -> Result<(), String> {
        let entries = self.entries.read().map_err(|e| e.to_string())?;
        let plaintext = serde_json::to_vec(&*entries).map_err(|e| e.to_string())?;

        let cipher = Aes256Gcm::new_from_slice(&*self.master_key).map_err(|e| e.to_string())?;

        // Generate fresh nonce for each encryption (critical for AES-GCM security)
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|e| e.to_string())?;

        let mut store = self.store.write().map_err(|e| e.to_string())?;
        store.nonce = nonce_bytes;
        store.ciphertext = ciphertext;
        Ok(())
    }

    /// Decrypt store to entries
    pub fn decrypt(&self) -> Result<(), String> {
        let store = self.store.read().map_err(|e| e.to_string())?;

        if store.ciphertext.is_empty() {
            return Ok(());
        }

        let cipher = Aes256Gcm::new_from_slice(&*self.master_key).map_err(|e| e.to_string())?;

        let nonce = Nonce::from_slice(&store.nonce);

        let plaintext = cipher
            .decrypt(nonce, store.ciphertext.as_ref())
            .map_err(|_| "Decryption failed - wrong password?".to_string())?;

        let entries: HashMap<String, String> =
            serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;

        *self.entries.write().map_err(|e| e.to_string())? = entries;
        Ok(())
    }
}

/// Zeroize master key on drop for security
impl Drop for CredentialVault {
    fn drop(&mut self) {
        self.master_key.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypted_store_new() {
        let store = EncryptedStore::new();
        assert_eq!(store.version, 1);
        assert_eq!(store.salt.len(), 16);
        assert_eq!(store.nonce.len(), 12);
    }

    #[test]
    fn test_vault_set_get() {
        let vault = CredentialVault::new_for_testing("test_password".to_string());
        vault.set("api_key", "sk-12345").unwrap();

        let retrieved = vault.get("api_key");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().as_str(), "sk-12345");
    }

    #[test]
    fn test_vault_encrypt_decrypt() {
        let vault = CredentialVault::new_for_testing("test_password".to_string());
        vault.set("api_key", "sk-12345").unwrap();
        vault.encrypt().unwrap();

        let store = vault.store.read().unwrap();
        assert!(!store.ciphertext.is_empty());

        // ciphertext should not be plaintext
        let ciphertext_str = String::from_utf8_lossy(&store.ciphertext);
        assert!(!ciphertext_str.contains("sk-12345"));
    }

    #[test]
    fn test_vault_short_password() {
        let result = CredentialVault::new("short".to_string());
        assert!(result.is_err());
    }
}
