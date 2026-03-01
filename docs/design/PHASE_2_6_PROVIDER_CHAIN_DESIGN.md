# Phase 2.6 - Provider Chain 多实例设计

**版本**: v1.0
**创建日期**: 2026-03-01
**目标**: LLM Provider 多实例故障切换

---

## 一、设计目标

### 1.1 需求

- 企业级多策略支持（成本优化/高可用/地域部署）
- 自动故障切换
- 手动实例选择覆盖
- 静态配置 + REST API 动态管理

### 1.2 当前状态

- `create_provider(name, api_key, base_url)` - 单实例
- 缺少多实例、故障切换

---

## 二、核心数据结构

```rust
// crates/octo-engine/src/providers/chain.rs

/// LLM 实例配置
pub struct LlmInstance {
    pub id: String,
    pub provider: String,           // "anthropic" / "openai"
    pub api_key: String,
    pub base_url: Option<String>,   // 自定义端点
    pub model: String,
    pub priority: u8,              // 0 最高
    pub max_rpm: Option<u32>,      // 速率限制
    pub enabled: bool,
}

/// 实例健康状态
#[derive(Clone)]
pub enum InstanceHealth {
    Healthy,
    Unhealthy { reason: String, failed_at: DateTime<Utc> },
    Unknown,
}

/// 故障切换策略
pub enum FailoverPolicy {
    Automatic,   // 自动切换
    Manual,       // 手动选择
    Hybrid,       // 自动 + 允许覆盖
}
```

---

## 三、ProviderChain 核心逻辑

```rust
pub struct ProviderChain {
    instances: Arc<RwLock<Vec<LlmInstance>>>,
    health: Arc<RwLock<HashMap<String, InstanceHealth>>>,
    policy: FailoverPolicy,
    health_check_interval: Duration,
    manual_instance_id: Arc<RwLock<Option<String>>>,  // 手动选择
}

impl ProviderChain {
    /// 获取可用的实例（自动或手动）
    pub async fn get_available(&self) -> Result<Arc<LlmInstance>> {
        // 1. 手动选择优先
        if let Some(id) = self.manual_instance_id.read().await.as_ref() {
            if let Some(instance) = self.find_by_id(id).await {
                if self.is_healthy(instance.id()).await {
                    return Ok(instance);
                }
            }
        }

        // 2. 自动模式：按 priority 选择健康的最高优先级实例
        match self.policy {
            FailoverPolicy::Manual => {
                Err(anyhow!("No manual instance selected"))
            }
            _ => self.get_next_healthy().await,
        }
    }

    /// 按 priority 排序，获取第一个健康实例
    async fn get_next_healthy(&self) -> Result<Arc<LlmInstance>> {
        let instances = self.instances.read().await;
        let health = self.health.read().await;

        let mut sorted: Vec<_> = instances.iter()
            .filter(|i| i.enabled)
            .collect();
        sorted.sort_by_key(|i| i.priority);

        for instance in sorted {
            if matches!(health.get(&instance.id), Some(InstanceHealth::Healthy) | None) {
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
            return Err(anyhow!("Instance not found"));
        }
        drop(instances);

        let mut manual = self.manual_instance_id.write().await;
        *manual = Some(instance_id.to_string());
        Ok(())
    }
}
```

---

## 四、健康检查机制

```rust
impl ProviderChain {
    /// 定时健康检查 - 恢复不健康实例
    pub async fn start_health_checker(&self) {
        let interval = self.health_check_interval;
        let instances = Arc::clone(&self.instances);
        let health = Arc::clone(&self.health);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;

                let instance_ids: Vec<String> = {
                    let instances = instances.read().await;
                    instances.iter().map(|i| i.id.clone()).collect()
                };

                for id in instance_ids {
                    let should_check = {
                        let h = health.read().await;
                        matches!(h.get(&id), Some(InstanceHealth::Unhealthy { .. }) | None)
                    };

                    if should_check {
                        if Self::check_instance_health(&id).await.is_ok() {
                            let mut h = health.write().await;
                            h.insert(id, InstanceHealth::Healthy);
                            info!("Instance {} recovered", id);
                        }
                    }
                }
            }
        });
    }
}
```

---

## 五、配置结构

```yaml
# config.yaml
providers:
  chain:
    failover_policy: "hybrid"  # automatic / manual / hybrid
    health_check_interval_sec: 30

  instances:
    - id: "claude-opus-primary"
      provider: "anthropic"
      api_key: "${ANTHROPIC_API_KEY}"
      model: "claude-3-opus-20240229"
      priority: 0
      max_rpm: 50
      enabled: true

    - id: "claude-sonnet-vertex"
      provider: "anthropic"
      api_key: "${GOOGLE_APPLICATION_CREDENTIALS}"
      base_url: "https://anthropic-vertex.googleapis.com"
      model: "claude-3-5-sonnet-20241022"
      priority: 1
      max_rpm: 100
      enabled: true

    - id: "gpt4-azure"
      provider: "openai"
      api_key: "${AZURE_OPENAI_KEY}"
      base_url: "${AZURE_OPENAI_ENDPOINT}"
      model: "gpt-4-turbo"
      priority: 2
      enabled: true
```

---

## 六、与现有 Provider 集成

```rust
/// 包装 ProviderChain 为单一 Provider 接口
pub struct ChainProvider {
    chain: Arc<ProviderChain>,
    max_retries: u32,
}

#[async_trait]
impl Provider for ChainProvider {
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

            let provider = create_provider(
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

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream> {
        // 类似逻辑，使用 stream
        todo!()
    }
}
```

---

## 七、REST API 设计

| Method | Path | 说明 |
|--------|------|------|
| GET | /api/v1/providers | 列出所有实例 |
| GET | /api/v1/providers/{id} | 获取实例详情 |
| POST | /api/v1/providers | 添加实例 |
| DELETE | /api/v1/providers/{id} | 删除实例 |
| POST | /api/v1/providers/{id}/select | 手动选择实例 |
| POST | /api/v1/providers/{id}/reset | 重置实例健康状态 |
| GET | /api/v1/providers/current | 获取当前使用实例 |

### 响应示例

```json
// GET /api/v1/providers
{
  "policy": "hybrid",
  "current_instance_id": "claude-opus-primary",
  "instances": [
    {
      "id": "claude-opus-primary",
      "provider": "anthropic",
      "model": "claude-3-opus-20240229",
      "priority": 0,
      "health": "healthy",
      "enabled": true
    }
  ]
}
```

---

## 八、验收标准

| 条件 | 说明 |
|------|------|
| 多实例配置 | config.yaml 可配置 3+ 实例 |
| 自动切换 | 主实例失败自动切换到备用 |
| 手动选择 | POST /providers/{id}/select 生效 |
| 健康恢复 | 不健康实例 30s 后自动恢复 |
| 错误传播 | 所有实例失败返回具体错误 |

---

## 九、实施任务

| 任务 | 估算 | 依赖 |
|------|------|------|
| LlmInstance 数据结构 | 50 LOC | - |
| ProviderChain 核心逻辑 | 150 LOC | - |
| 健康检查机制 | 80 LOC | - |
| ChainProvider 包装 | 100 LOC | create_provider |
| 配置加载 | 50 LOC | Config |
| REST API | 100 LOC | router |
| 测试 | 100 LOC | - |
| **总计** | **~630 LOC** | |
