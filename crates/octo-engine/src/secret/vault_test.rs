#[cfg(test)]
mod tests {
    use crate::secret::{CredentialVault, EncryptedStore};

    #[test]
    fn test_encrypted_store_new() {
        let store = EncryptedStore::new();
        assert_eq!(store.version, 1);
        assert_eq!(store.salt.len(), 16);
        assert_eq!(store.nonce.len(), 12);
    }

    #[test]
    fn test_vault_encrypt_decrypt() {
        let vault = CredentialVault::new_for_testing("test_password".to_string());
        vault.set("api_key", "sk-12345").unwrap();

        let retrieved = vault.get("api_key");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().as_str(), "sk-12345");
    }

    #[test]
    fn test_vault_wrong_password() {
        let vault = CredentialVault::new_for_testing("correct_password".to_string());
        vault.set("key", "value").unwrap();

        // Use different password to create new vault instance
        let vault2 = CredentialVault::new_for_testing("wrong_password".to_string());
        let result = vault2.get("key");
        assert!(result.is_none()); // Cannot decrypt
    }

    #[test]
    fn test_vault_persistence_format() {
        let vault = CredentialVault::new_for_testing("password123".to_string());
        vault.set("api_key", "secret_value").unwrap();

        let store = vault.store.read().unwrap();

        // ciphertext should not be plaintext
        let ciphertext_str = String::from_utf8_lossy(&store.ciphertext);
        assert!(!ciphertext_str.contains("secret_value"));
    }
}
