// crates/octo-engine/src/auth/mod.rs

pub mod api_key;
pub mod config;
pub mod middleware;
pub mod roles;

// Re-export config types
pub use config::*;

// Re-export middleware types
pub use middleware::*;

// Re-export roles types
pub use roles::*;

// Re-export api_key types (StoredApiKey to avoid conflict with config's ApiKey)
pub use api_key::{ApiKeyResponse, ApiKeyStorage, StoredApiKey};
