//! Policy engine — rules-based hook evaluation from policies.yaml.

mod bridge;
pub mod config;
mod matcher;

pub use bridge::PolicyEngineBridge;
pub use config::{PolicyConfig, PolicyEntry, PolicyRule};
pub use matcher::PolicyMatcher;
