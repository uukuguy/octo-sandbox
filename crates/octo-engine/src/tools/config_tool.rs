//! ConfigTool — runtime configuration get/set tool.
//!
//! Aligns with CC-OSS ConfigTool: allows agents to read and modify
//! runtime configuration settings.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolSource};
use serde_json::json;
use tokio::sync::RwLock;

use super::traits::Tool;

/// Runtime-mutable configuration store.
/// Keys are dot-separated paths (e.g., "provider.model", "autonomy_level").
pub type ConfigStore = Arc<RwLock<HashMap<String, serde_json::Value>>>;

/// Settings that cannot be modified at runtime.
const READONLY_SETTINGS: &[&str] = &["db_path", "host", "port"];

pub struct ConfigTool {
    store: ConfigStore,
}

impl ConfigTool {
    pub fn new(store: ConfigStore) -> Self {
        Self { store }
    }

    /// Create with initial values seeded from AgentRuntimeConfig.
    pub fn with_initial(initial: HashMap<String, serde_json::Value>) -> Self {
        Self {
            store: Arc::new(RwLock::new(initial)),
        }
    }
}

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &str {
        "config"
    }

    fn description(&self) -> &str {
        "Get or set runtime configuration settings.\n\
         \n\
         ## GET mode (value omitted)\n\
         Returns the current value for the given setting key.\n\
         \n\
         ## SET mode (value provided)\n\
         Updates the setting to the new value. Some settings are read-only.\n\
         \n\
         ## Parameters\n\
         - setting (required): Configuration key (e.g., \"provider.model\", \"autonomy_level\")\n\
         - value (optional): New value. Omit to read current value.\n\
         \n\
         ## Read-only settings\n\
         db_path, host, port — cannot be modified at runtime."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "setting": {
                    "type": "string",
                    "description": "Configuration key to get or set"
                },
                "value": {
                    "description": "New value (omit to read current value)"
                }
            },
            "required": ["setting"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let setting = params["setting"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: setting"))?;

        let value = params.get("value").filter(|v| !v.is_null());

        match value {
            // GET mode
            None => {
                let store = self.store.read().await;
                let current = store.get(setting).cloned();
                let result = json!({
                    "setting": setting,
                    "value": current,
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            // SET mode
            Some(new_value) => {
                // Check read-only
                if READONLY_SETTINGS.contains(&setting) {
                    return Ok(ToolOutput::error(format!(
                        "Setting '{}' is read-only and cannot be modified at runtime",
                        setting
                    )));
                }

                let mut store = self.store.write().await;
                let old_value = store.get(setting).cloned();
                store.insert(setting.to_string(), new_value.clone());

                let result = json!({
                    "setting": setting,
                    "old_value": old_value,
                    "new_value": new_value,
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
        }
    }

    fn risk_level(&self) -> RiskLevel {
        // GET is low risk, SET is high risk — but we can't differentiate statically.
        // Use classify_input_risk for dynamic classification.
        RiskLevel::LowRisk
    }

    fn classify_input_risk(&self, params: &serde_json::Value) -> Option<RiskLevel> {
        let has_value = params.get("value").is_some_and(|v| !v.is_null());
        if has_value {
            Some(RiskLevel::HighRisk)
        } else {
            Some(RiskLevel::LowRisk)
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        false // supports SET
    }

    fn category(&self) -> &str {
        "configuration"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    fn seeded_store() -> ConfigStore {
        let mut map = HashMap::new();
        map.insert("provider.model".to_string(), json!("claude-sonnet-4-6"));
        map.insert("autonomy_level".to_string(), json!("supervised"));
        map.insert("db_path".to_string(), json!("./data/octo.db"));
        Arc::new(RwLock::new(map))
    }

    #[tokio::test]
    async fn test_config_get_existing() {
        let tool = ConfigTool::new(seeded_store());
        let result = tool
            .execute(json!({"setting": "provider.model"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.contains("claude-sonnet-4-6"));
    }

    #[tokio::test]
    async fn test_config_get_nonexistent() {
        let tool = ConfigTool::new(seeded_store());
        let result = tool
            .execute(json!({"setting": "unknown.key"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.contains("null"));
    }

    #[tokio::test]
    async fn test_config_set_updates() {
        let tool = ConfigTool::new(seeded_store());
        let ctx = test_ctx();

        let result = tool
            .execute(
                json!({"setting": "autonomy_level", "value": "autonomous"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("autonomous"));
        assert!(result.content.contains("supervised")); // old_value

        // Verify persistence
        let result2 = tool
            .execute(json!({"setting": "autonomy_level"}), &ctx)
            .await
            .unwrap();
        assert!(result2.content.contains("autonomous"));
    }

    #[tokio::test]
    async fn test_config_set_readonly_rejected() {
        let tool = ConfigTool::new(seeded_store());
        let result = tool
            .execute(
                json!({"setting": "db_path", "value": "/new/path"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("read-only"));
    }

    #[tokio::test]
    async fn test_config_input_risk_classification() {
        let tool = ConfigTool::new(seeded_store());
        // GET is low risk
        assert_eq!(
            tool.classify_input_risk(&json!({"setting": "x"})),
            Some(RiskLevel::LowRisk)
        );
        // SET is high risk
        assert_eq!(
            tool.classify_input_risk(&json!({"setting": "x", "value": "y"})),
            Some(RiskLevel::HighRisk)
        );
    }
}
