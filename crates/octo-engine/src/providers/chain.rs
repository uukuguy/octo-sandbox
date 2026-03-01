use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// LLM 实例配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmInstance {
    pub id: String,
    pub provider: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
    pub priority: u8,
    pub max_rpm: Option<u32>,
    pub enabled: bool,
}

/// 实例健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum InstanceHealth {
    Healthy,
    Unhealthy { reason: String, failed_at: DateTime<Utc> },
    Unknown,
}

/// 故障切换策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailoverPolicy {
    Automatic,
    Manual,
    Hybrid,
}

impl Default for FailoverPolicy {
    fn default() -> Self {
        FailoverPolicy::Automatic
    }
}
