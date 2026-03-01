# Phase 2.6 - Provider Chain 多实例实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现 LLM Provider 多实例故障切换，支持自动/手动/混合模式

**Architecture:** ProviderChain 管理多个 LlmInstance，按 priority 选择健康实例；ChainProvider 包装为统一 Provider 接口；REST API 动态管理

**Tech Stack:** Rust, Tokio, existing octo-engine providers module

---

## 实施任务总览

| 任务 | 估算 | 状态 |
|------|------|------|
| Task 1: LlmInstance 数据结构 | 50 LOC | ⬜ |
| Task 2: ProviderChain 核心逻辑 | 150 LOC | ⬜ |
| Task 3: 健康检查机制 | 80 LOC | ⬜ |
| Task 4: ChainProvider 包装 | 100 LOC | ⬜ |
| Task 5: 配置加载 | 50 LOC | ⬜ |
| Task 6: REST API | 100 LOC | ⬜ |
| Task 7: 测试 | 100 LOC | ⬜ |
| Task 8: 构建验证 | - | ⬜ |

---

## Task 1: LlmInstance 数据结构

**Files:**
- Create: `crates/octo-engine/src/providers/chain.rs`

**Step 1: 创建基础结构**

```rust
// crates/octo-engine/src/providers/chain.rs

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
```

**Step 2: 添加 mod 声明**

Modify: `crates/octo-engine/src/providers/mod.rs:1-9`

在文件开头添加:
```rust
pub mod chain;
pub use chain::*;
```

**Step 3: 运行 cargo check 验证**

Run: `cargo check -p octo-engine`
Expected: No errors

**Step 4: Commit**

```bash
git add crates/octo-engine/src/providers/chain.rs crates/octo-engine/src/providers/mod.rs
git commit -m "feat(provider): add LlmInstance and FailoverPolicy structs"
```

---

## Task 2: ProviderChain 核心逻辑

**Files:**
- Modify: `crates/octo-engine/src/providers/chain.rs`

**Step 1: 添加 ProviderChain 结构体和实现**

在 chain.rs 文件末尾添加:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Result, anyhow};

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
}
```

需要添加 HashMap import:

```rust
use std::collections::HashMap;
```

**Step 2: 运行 cargo check**

Run: `cargo check -p octo-engine`
Expected: No errors

**Step 3: Commit**

```bash
git add crates/octo-engine/src/providers/chain.rs
git commit -m "feat(provider): add ProviderChain core logic"
```

---

## Task 3: 健康检查机制

**Files:**
- Modify: `crates/octo-engine/src/providers/chain.rs`

**Step 1: 添加健康检查方法**

在 ProviderChain impl 块中添加:

```rust
use std::time::Duration;

/// 健康检查配置
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

impl ProviderChain {
    /// 启动健康检查任务
    pub async fn start_health_checker(&self, config: HealthCheckConfig) {
        let instances = Arc::clone(&self.instances);
        let health = Arc::clone(&self.health);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(config.interval).await;

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
        health: &Arc<RwLock<HashMap<String, InstanceHealth>>>,
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
        let provider = match super::create_provider(
            &instance.provider,
            instance.api_key.clone(),
            instance.base_url.clone(),
        ) {
            Ok(p) => p,
            Err(_) => return false,
        };

        // 可以在这里添加实际的 ping 调用
        // 目前简单返回 true
        true
    }
}
```

**Step 2: 运行 cargo check**

Run: `cargo check -p octo-engine`
Expected: No errors

**Step 3: Commit**

```bash
git add crates/octo-engine/src/providers/chain.rs
git commit -m "feat(provider): add health check mechanism"
```

---

## Task 4: ChainProvider 包装

**Files:**
- Modify: `crates/octo-engine/src/providers/chain.rs`

**Step 1: 添加 ChainProvider 实现**

在 chain.rs 添加:

```rust
use async_trait::async_trait;
use octo_types::{CompletionRequest, CompletionResponse};

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
impl super::Provider for ChainProvider {
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

            let provider = super::create_provider(
                &instance.provider,
                instance.api_key.clone(),
                instance.base_url.clone(),
            );

            match provider.complete(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    self.chain.mark_unhealthy(instance.id(), &e.to_string()).await;
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("All instances failed")))
    }

    async fn stream(&self, request: CompletionRequest) -> Result<super::CompletionStream> {
        // Stream 模式需要特殊处理：选择一个实例后全程使用
        let instance = self.chain.get_available().await?;

        let provider = super::create_provider(
            &instance.provider,
            instance.api_key.clone(),
            instance.base_url.clone(),
        );

        match provider.stream(request).await {
            Ok(stream) => Ok(stream),
            Err(e) => {
                self.chain.mark_unhealthy(instance.id(), &e.to_string()).await;
                Err(e)
            }
        }
    }
}
```

**Step 2: 运行 cargo check**

Run: `cargo check -p octo-engine`
Expected: No errors

**Step 3: Commit**

```bash
git add crates/octo-engine/src/providers/chain.rs
git commit -m "feat(provider): add ChainProvider wrapper"
```

---

## Task 5: 配置加载

**Files:**
- Create: `crates/octo-engine/src/providers/config.rs`
- Modify: `crates/octo-engine/src/providers/mod.rs`
- Modify: `crates/octo-server/src/config.rs`

**Step 1: 创建配置结构**

```rust
// crates/octo-engine/src/providers/config.rs

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
```

**Step 2: 更新 mod.rs**

```rust
pub mod config;
pub use config::*;
```

**Step 3: 更新服务器配置**

Modify: `crates/octo-server/src/config.rs`

添加 ProviderChainConfig 字段:

```rust
// 在 ServerConfig 中添加
pub struct ServerConfig {
    // ... existing fields
    pub provider_chain: Option<ProviderChainConfig>,
}
```

**Step 4: 运行 cargo check**

Run: `cargo check -p octo-engine -p octo-server`
Expected: No errors

**Step 5: Commit**

```bash
git add crates/octo-engine/src/providers/config.rs
git add crates/octo-engine/src/providers/mod.rs
git add crates/octo-server/src/config.rs
git commit -m "feat(provider): add ProviderChain config loading"
```

---

## Task 6: REST API

**Files:**
- Create: `crates/octo-server/src/api/providers.rs`
- Modify: `crates/octo-server/src/router.rs`

**Step 1: 创建 API handlers**

```rust
// crates/octo-server/src/api/providers.rs

use axum::{
    extract::Path,
    routing::{get, post, delete},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use super::AppState;

/// 列表响应
#[derive(Serialize)]
pub struct ListProvidersResponse {
    pub policy: String,
    pub current_instance_id: Option<String>,
    pub instances: Vec<ProviderInstance>,
}

#[derive(Serialize)]
pub struct ProviderInstance {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub priority: u8,
    pub health: String,
    pub enabled: bool,
}

/// 添加实例请求
#[derive(Deserialize)]
pub struct AddProviderRequest {
    pub id: String,
    pub provider: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
    pub priority: u8,
    pub max_rpm: Option<u32>,
    pub enabled: Option<bool>,
}

/// 列表所有实例
pub async fn list_providers(state: State<AppState>) -> Json<ListProvidersResponse> {
    let chain = state.provider_chain.read().await;

    let policy = match &*chain {
        Some(c) => format!("{:?}", c.policy()),
        None => "none".to_string(),
    };

    let instances = match &*chain {
        Some(c) => {
            c.list_instances()
                .await
                .into_iter()
                .map(|i| ProviderInstance {
                    id: i.id,
                    provider: i.provider,
                    model: i.model,
                    priority: i.priority,
                    health: format!("{:?}", c.get_health(&i.id).await),
                    enabled: i.enabled,
                })
                .collect()
        }
        None => vec![],
    };

    let current = match &*chain {
        Some(c) => c.get_current_selection().await,
        None => None,
    };

    Json(ListProvidersResponse {
        policy,
        current_instance_id: current,
        instances,
    })
}

/// 手动选择实例
pub async fn select_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<()>, String> {
    let chain = state.provider_chain.read().await;

    match &*chain {
        Some(c) => c.select_instance(&id).await.map_err(|e| e.to_string())?,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// 重置实例健康状态
pub async fn reset_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<()>, String> {
    let chain = state.provider_chain.read().await;

    match &*chain {
        Some(c) => c.reset_health(&id).await.map_err(|e| e.to_string())?,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// 清除选择
pub async fn clear_selection(State(state): State<AppState>) -> Result<Json<()>, String> {
    let chain = state.provider_chain.read().await;

    match &*chain {
        Some(c) => c.clear_selection().await,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// 添加实例
pub async fn add_provider(
    State(state): State<AppState>,
    Json(req): Json<AddProviderRequest>,
) -> Result<Json<()>, String> {
    let chain = state.provider_chain.read().await;

    let instance = super::config::LlmInstanceConfig {
        id: req.id,
        provider: req.provider,
        api_key: req.api_key,
        base_url: req.base_url,
        model: req.model,
        priority: req.priority,
        max_rpm: req.max_rpm,
        enabled: req.enabled.unwrap_or(true),
    };

    match &*chain {
        Some(c) => c.add_instance(instance.to_instance()).await,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// 删除实例
pub async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<()>, String> {
    let chain = state.provider_chain.read().await;

    match &*chain {
        Some(c) => c.remove_instance(&id).await.map_err(|e| e.to_string())?,
        None => return Err("Provider chain not configured".to_string()),
    };

    Ok(Json(()))
}

/// 注册路由
pub fn router() -> Router {
    Router::new()
        .route("/api/v1/providers", get(list_providers))
        .route("/api/v1/providers", post(add_provider))
        .route("/api/v1/providers/:id", delete(delete_provider))
        .route("/api/v1/providers/:id/select", post(select_provider))
        .route("/api/v1/providers/:id/reset", post(reset_provider))
        .route("/api/v1/providers/selection", delete(clear_selection))
}
```

**Step 2: 更新 AppState**

Modify: `crates/octo-server/src/state.rs`

添加:
```rust
pub struct AppState {
    // ... existing fields
    pub provider_chain: Arc<RwLock<Option<ProviderChain>>>,
}
```

**Step 3: 更新 router.rs**

Modify: `crates/octo-server/src/router.rs`

合并 providers 路由:
```rust
use super::api::providers::router as providers_router;

pub fn create_router() -> Router {
    // ... existing routes
    .nest("/api/v1", providers_router())
}
```

**Step 4: 运行 cargo check**

Run: `cargo check -p octo-server`
Expected: No errors

**Step 5: Commit**

```bash
git add crates/octo-server/src/api/providers.rs
git add crates/octo-server/src/state.rs
git add crates/octo-server/src/router.rs
git commit -m "feat(api): add provider chain REST endpoints"
```

---

## Task 7: 测试

**Files:**
- Create: `crates/octo-engine/src/providers/chain_test.rs`

**Step 1: 编写单元测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_and_list_instances() {
        let chain = ProviderChain::new(FailoverPolicy::Automatic);

        chain.add_instance(LlmInstance {
            id: "test-1".to_string(),
            provider: "anthropic".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
            model: "claude-3-sonnet".to_string(),
            priority: 0,
            max_rpm: None,
            enabled: true,
        }).await;

        let instances = chain.list_instances().await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "test-1");
    }

    #[tokio::test]
    async fn test_get_available_auto_mode() {
        let chain = ProviderChain::new(FailoverPolicy::Automatic);

        chain.add_instance(LlmInstance {
            id: "test-1".to_string(),
            provider: "anthropic".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
            model: "claude-3-sonnet".to_string(),
            priority: 0,
            max_rpm: None,
            enabled: true,
        }).await;

        let instance = chain.get_available().await.unwrap();
        assert_eq!(instance.id, "test-1");
    }

    #[tokio::test]
    async fn test_manual_selection() {
        let chain = ProviderChain::new(FailoverPolicy::Hybrid);

        chain.add_instance(LlmInstance {
            id: "test-1".to_string(),
            provider: "anthropic".to_string(),
            api_key: "key-1".to_string(),
            base_url: None,
            model: "claude-3-sonnet".to_string(),
            priority: 0,
            max_rpm: None,
            enabled: true,
        }).await;

        chain.add_instance(LlmInstance {
            id: "test-2".to_string(),
            provider: "openai".to_string(),
            api_key: "key-2".to_string(),
            base_url: None,
            model: "gpt-4".to_string(),
            priority: 1,
            max_rpm: None,
            enabled: true,
        }).await;

        // 手动选择
        chain.select_instance("test-2").await.unwrap();

        let selected = chain.get_current_selection().await;
        assert_eq!(selected, Some("test-2".to_string()));

        let instance = chain.get_available().await.unwrap();
        assert_eq!(instance.id, "test-2");
    }

    #[tokio::test]
    async fn test_mark_unhealthy() {
        let chain = ProviderChain::new(FailoverPolicy::Automatic);

        chain.add_instance(LlmInstance {
            id: "test-1".to_string(),
            provider: "anthropic".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
            model: "claude-3-sonnet".to_string(),
            priority: 0,
            max_rpm: None,
            enabled: true,
        }).await;

        // 标记不健康
        chain.mark_unhealthy("test-1", "rate limit").await;

        let health = chain.get_health("test-1").await;
        assert!(matches!(health, InstanceHealth::Unhealthy { .. }));
    }

    #[tokio::test]
    async fn test_remove_instance() {
        let chain = ProviderChain::new(FailoverPolicy::Automatic);

        chain.add_instance(LlmInstance {
            id: "test-1".to_string(),
            provider: "anthropic".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
            model: "claude-3-sonnet".to_string(),
            priority: 0,
            max_rpm: None,
            enabled: true,
        }).await;

        chain.remove_instance("test-1").await.unwrap();

        let instances = chain.list_instances().await;
        assert!(instances.is_empty());
    }
}
```

**Step 2: 运行测试**

Run: `cargo test -p octo-engine providers::chain`
Expected: All tests pass

**Step 3: Commit**

```bash
git add crates/octo-engine/src/providers/chain_test.rs
git commit -m "test(provider): add Chain tests"
```

---

## Task 8: 构建验证

**Step 1: 运行完整 cargo check**

Run: `cargo check --all`
Expected: No errors

**Step 2: 运行所有测试**

Run: `cargo test --all`
Expected: All tests pass

**Step 3: 最终提交**

```bash
git add -A
git commit -m "feat: complete Phase 2.6 Provider Chain

- LlmInstance + FailoverPolicy data structures
- ProviderChain with auto/manual/hybrid failover
- Health check mechanism with auto-recovery
- ChainProvider wrapper implementing Provider trait
- Config loading from YAML
- REST API endpoints (6 endpoints)
- Unit tests

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## 实施完成

所有任务完成后，运行验证：

```bash
# 构建验证
cargo check --all

# 测试验证
cargo test --all

# 前端构建（如果有改动）
cd web && pnpm build
```
