//! Authentication module.

pub mod jwt;
pub mod providers;

pub use jwt::{JwtConfig, JwtManager};
pub use providers::{OAuthError, OAuthProvider, OAuthUser};
