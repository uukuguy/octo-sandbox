use serde::{Deserialize, Serialize};
use super::chain::{LlmInstance, FailoverPolicy};

/// Provider Chain 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderChainConfig {
    #[serde(default = "default_policy")]
    pub failover_policy: FailoverPolicy,
    #[serde(default = "default_interval")]
    pub health_check_interval_sec: u64,
    pub instances: Vec<LlmInstanceConfig>,
}

fn default_policy() -> FailoverPolicy {
    FailoverPolicy::Automatic
}

fn default_interval() -> u64 {
    30
}

/// 单个实例的配置（从 env 读取 api_key）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmInstanceConfig {
    pub id: String,
    pub provider: String,
    pub api_key: String,  // 支持 ${ENV_VAR} 格式
    pub base_url: Option<String>,
    pub model: String,
    pub priority: u8,
    pub max_rpm: Option<u32>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl LlmInstanceConfig {
    /// 转换为运行时 LlmInstance
    pub fn to_instance(&self) -> LlmInstance {
        LlmInstance {
            id: self.id.clone(),
            provider: self.provider.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            priority: self.priority,
            max_rpm: self.max_rpm,
            enabled: self.enabled,
        }
    }
}
