use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! newtype_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            pub fn from_string(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

newtype_id!(UserId);
newtype_id!(TenantId);
newtype_id!(SessionId);
newtype_id!(SandboxId);

/// Default tenant ID for single-user / backward compatibility.
/// All existing agents without explicit tenant_id will belong to this default tenant.
pub const DEFAULT_TENANT_ID: &str = "default";
