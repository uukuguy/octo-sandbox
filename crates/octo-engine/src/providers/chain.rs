use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;
use async_trait::async_trait;

use octo_types::{CompletionRequest, CompletionResponse};

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

/// 健康检查配置
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub interval: Duration,
    pub timeout: Duration,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(10),
        }
    }
}

/// ProviderChain 管理多个 LLM 实例
pub struct ProviderChain {
    instances: Arc<RwLock<Vec<LlmInstance>>>,
    health: Arc<RwLock<HashMap<String, InstanceHealth>>>,
    policy: FailoverPolicy,
    manual_instance_id: Arc<RwLock<Option<String>>>,
}

impl ProviderChain {
    /// 创建新的 ProviderChain
    pub fn new(policy: FailoverPolicy) -> Self {
        Self {
            instances: Arc::new(RwLock::new(Vec::new())),
            health: Arc::new(RwLock::new(HashMap::new())),
            policy,
            manual_instance_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the failover policy
    pub fn policy(&self) -> FailoverPolicy {
        self.policy
    }

    /// 添加实例
    pub async fn add_instance(&self, instance: LlmInstance) {
        let mut instances = self.instances.write().await;
        instances.push(instance.clone());

        // 初始化健康状态
        let mut health = self.health.write().await;
        health.insert(instance.id, InstanceHealth::Unknown);
    }

    /// 移除实例
    pub async fn remove_instance(&self, id: &str) -> Result<()> {
        let mut instances = self.instances.write().await;
        let len_before = instances.len();
        instances.retain(|i| i.id != id);

        if instances.len() == len_before {
            return Err(anyhow!("Instance not found: {}", id));
        }

        let mut health = self.health.write().await;
        health.remove(id);

        // 如果移除的是手动选择的实例，清除选择
        let mut manual = self.manual_instance_id.write().await;
        if manual.as_deref() == Some(id) {
            *manual = None;
        }

        Ok(())
    }

    /// 列出所有实例
    pub async fn list_instances(&self) -> Vec<LlmInstance> {
        self.instances.read().await.clone()
    }

    /// 获取实例健康状态
    pub async fn get_health(&self, id: &str) -> InstanceHealth {
        let health = self.health.read().await;
        health.get(id).cloned().unwrap_or(InstanceHealth::Unknown)
    }

    /// 获取可用的实例
    pub async fn get_available(&self) -> Result<Arc<LlmInstance>> {
        // 1. 手动选择优先
        if let Some(id) = self.manual_instance_id.read().await.as_ref() {
            let instances = self.instances.read().await;
            if let Some(instance) = instances.iter().find(|i| &i.id == id) {
                if instance.enabled {
                    let health = self.health.read().await;
                    if matches!(health.get(&instance.id), Some(InstanceHealth::Healthy) | None | Some(InstanceHealth::Unknown)) {
                        return Ok(Arc::new(instance.clone()));
                    }
                }
            }
        }

        // 2. 自动模式
        match self.policy {
            FailoverPolicy::Manual => {
                Err(anyhow!("No manual instance selected"))
            }
            _ => self.get_next_healthy_instance().await,
        }
    }

    async fn get_next_healthy_instance(&self) -> Result<Arc<LlmInstance>> {
        let instances = self.instances.read().await;
        let health = self.health.read().await;

        let mut sorted: Vec<_> = instances.iter()
            .filter(|i| i.enabled)
            .collect();
        sorted.sort_by_key(|i| i.priority);

        for instance in sorted {
            let instance_health = health.get(&instance.id);
            if matches!(instance_health, Some(InstanceHealth::Healthy) | None | Some(InstanceHealth::Unknown)) {
                return Ok(Arc::new(instance.clone()));
            }
        }

        Err(anyhow!("No healthy instances available"))
    }

    /// 标记实例不健康
    pub async fn mark_unhealthy(&self, instance_id: &str, reason: &str) {
        let mut health = self.health.write().await;
        health.insert(instance_id.to_string(), InstanceHealth::Unhealthy {
            reason: reason.to_string(),
            failed_at: Utc::now(),
        });
    }

    /// 手动选择实例
    pub async fn select_instance(&self, instance_id: &str) -> Result<()> {
        let instances = self.instances.read().await;
        if !instances.iter().any(|i| i.id == instance_id) {
            return Err(anyhow!("Instance not found: {}", instance_id));
        }
        drop(instances);

        let mut manual = self.manual_instance_id.write().await;
        *manual = Some(instance_id.to_string());
        Ok(())
    }

    /// 清除手动选择
    pub async fn clear_selection(&self) {
        let mut manual = self.manual_instance_id.write().await;
        *manual = None;
    }

    /// 获取当前选择
    pub async fn get_current_selection(&self) -> Option<String> {
        self.manual_instance_id.read().await.clone()
    }

    /// 重置实例健康状态
    pub async fn reset_health(&self, instance_id: &str) -> Result<()> {
        let instances = self.instances.read().await;
        if !instances.iter().any(|i| i.id == instance_id) {
            return Err(anyhow!("Instance not found: {}", instance_id));
        }

        let mut health = self.health.write().await;
        health.insert(instance_id.to_string(), InstanceHealth::Healthy);
        Ok(())
    }

    /// 启动健康检查任务
    pub async fn start_health_checker(&self, config: HealthCheckConfig) {
        let instances = Arc::clone(&self.instances);
        let health = Arc::clone(&self.health);

        tokio::spawn(async move {
            loop {
                sleep(config.interval).await;

                let instance_ids: Vec<String> = {
                    let instances = instances.read().await;
                    instances.iter().map(|i| i.id.clone()).collect()
                };

                for id in instance_ids {
                    // 只检查 Unknown 或 Unhealthy 的实例
                    let should_check = {
                        let h = health.read().await;
                        matches!(
                            h.get(&id),
                            Some(InstanceHealth::Unhealthy { .. }) | None
                        )
                    };

                    if should_check {
                        // 简单健康检查：创建 provider 测试
                        if Self::check_instance(&id, &instances, &health, config.timeout).await {
                            let mut h = health.write().await;
                            h.insert(id.clone(), InstanceHealth::Healthy);
                            info!("Instance {} recovered to healthy", id);
                        }
                    }
                }
            }
        });
    }

    async fn check_instance(
        id: &str,
        instances: &Arc<RwLock<Vec<LlmInstance>>>,
        _health: &Arc<RwLock<HashMap<String, InstanceHealth>>>,
        timeout: Duration,
    ) -> bool {
        let instance = {
            let instances = instances.read().await;
            instances.iter().find(|i| i.id == id).cloned()
        };

        let Some(instance) = instance else {
            return false;
        };

        // 尝试创建 provider（不实际调用 API）
        // 如果能创建成功，认为实例可用
        let _provider = super::create_provider(
            &instance.provider,
            instance.api_key.clone(),
            instance.base_url.clone(),
        );

        // 可以在这里添加实际的 ping 调用
        // 目前简单返回 true
        let _ = timeout;
        true
    }
}

/// 包装 ProviderChain 为单一 Provider 接口
pub struct ChainProvider {
    chain: Arc<ProviderChain>,
    max_retries: u32,
}

impl ChainProvider {
    pub fn new(chain: Arc<ProviderChain>, max_retries: u32) -> Self {
        Self { chain, max_retries }
    }

    pub fn chain(&self) -> &Arc<ProviderChain> {
        &self.chain
    }
}

#[async_trait]
impl crate::providers::Provider for ChainProvider {
    fn id(&self) -> &str {
        "chain"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let mut last_error = None;

        for _ in 0..self.max_retries {
            let instance = match self.chain.get_available().await {
                Ok(i) => i,
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };

            let provider = crate::providers::create_provider(
                &instance.provider,
                instance.api_key.clone(),
                instance.base_url.clone(),
            );

            match provider.complete(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    self.chain.mark_unhealthy(&instance.id, &e.to_string()).await;
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("All instances failed")))
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<crate::providers::CompletionStream> {
        // Stream 模式需要特殊处理：选择一个实例后全程使用
        let instance = self.chain.get_available().await?;

        let provider = crate::providers::create_provider(
            &instance.provider,
            instance.api_key.clone(),
            instance.base_url.clone(),
        );

        match provider.stream(request).await {
            Ok(stream) => Ok(stream),
            Err(e) => {
                self.chain.mark_unhealthy(&instance.id, &e.to_string()).await;
                Err(e)
            }
        }
    }
}
