//! Authentication module.

pub mod jwt;
pub mod providers;

pub use providers::{OAuthError, OAuthProvider, OAuthUser};
pub use jwt::{JwtConfig, JwtManager};
