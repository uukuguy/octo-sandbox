# Octo-Engine 自主运行模式设计

> 基于 CC-OSS KAIROS/Proactive Mode 代码逆向分析，为 Octo 企业场景设计自主运行模式。
> 日期：2026-04-01
> 定位：企业 AI 智能体支撑平台的长链条工作流执行引擎。

---

## 一、CC KAIROS 模式逆向分析

### 核心运行机制

KAIROS 是一个**自主循环执行模式**，通过 tick 心跳驱动 agent 持续工作：

```
初始指令 → Agent 执行 → Sleep(N秒) → <tick>时间</tick> → 检查工作 → 执行/Sleep → ...
                                                            ↑
                                                       用户随时插入消息
```

### CC 实现要素

| 要素 | CC 实现 | 说明 |
|------|--------|------|
| **系统提示词** | 精简自主版（替换标准 7 段） | "You are an autonomous agent" + 60 行自主工作指南 |
| **Tick 心跳** | Sleep 完成后 CLI 注入 `<tick>14:32:05</tick>` | 触发下一轮 LLM 调用 |
| **Sleep 工具** | agent 控制自己的醒来间隔 | 5s（积极迭代）~ 300s（空闲等待） |
| **终端焦点感知** | `terminalFocus: focused/unfocused` | focused=协作式，unfocused=全自主 |
| **暂停/恢复** | `isProactivePaused()` | 暂停时 tick 不发送 |
| **行为偏好** | "偏向行动，不问就做" | 读/改/测/提交全自动 |
| **简洁原则** | "不要解释每一步" | 只报告决策点和里程碑 |
| **Cache 意识** | "prompt cache 5 分钟过期" | Sleep 不宜超过 5 分钟 |

### CC 局限

- 单用户 CLI 产品，只能跑一个 KAIROS session
- Tick 机制绑定终端事件循环
- 无法与企业系统（消息队列、webhook）集成
- 无审计和合规控制

---

## 二、Octo 自主模式设计

### 设计目标

在 Octo 的企业 server/platform 架构上实现比 CC KAIROS 更强的自主执行能力：
- 多个自主 session 并行运行
- 与企业系统（Webhook、MQ、监控）集成
- 完整的安全/审计/预算控制
- 用户通过 Web/API 实时监控和干预

### 核心架构

```
                    ┌──────────────────────────────┐
                    │      AutonomousScheduler       │
                    │  (管理所有自主 session 的生命周期)  │
                    └──────────┬───────────────────┘
                               │
          ┌────────────────────┼────────────────────┐
          │                    │                    │
    ┌─────┴──────┐      ┌─────┴──────┐      ┌─────┴──────┐
    │ Session A   │      │ Session B   │      │ Session C   │
    │ (代码审查)   │      │ (数据管道)   │      │ (运维监控)   │
    │             │      │             │      │             │
    │ Tick 循环    │      │ Tick 循环    │      │ Tick 循环    │
    └─────────────┘      └─────────────┘      └─────────────┘
          │                    │                    │
          ├─ WebSocket 实时推送  ├─ Webhook 触发       ├─ Cron 定时触发
          ├─ 用户随时干预        ├─ MQ 消息驱动         ├─ 监控告警驱动
          └─ 预算/轮次限制      └─ 预算/轮次限制       └─ 预算/轮次限制
```

### 数据结构

```rust
/// 自主运行模式配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousConfig {
    /// 是否启用自主模式
    pub enabled: bool,

    /// 空闲时默认 Sleep 间隔（秒）
    #[serde(default = "default_idle_sleep")]
    pub idle_sleep_secs: u64,        // 默认 30

    /// 积极工作时 Sleep 间隔（秒）
    #[serde(default = "default_active_sleep")]
    pub active_sleep_secs: u64,      // 默认 5

    /// 最大连续自主轮数（安全限制）
    #[serde(default = "default_max_rounds")]
    pub max_autonomous_rounds: u32,  // 默认 100

    /// 最大自主运行时间（秒）
    #[serde(default = "default_max_duration")]
    pub max_duration_secs: u64,      // 默认 3600 (1 小时)

    /// 每轮最大 token 预算
    pub max_tokens_per_round: Option<u32>,

    /// 总 USD 成本上限
    pub max_cost_usd: Option<f64>,

    /// 触发模式
    #[serde(default)]
    pub trigger: AutonomousTrigger,

    /// 用户在线感知（类似 CC 的 terminalFocus）
    #[serde(default = "default_true")]
    pub user_presence_aware: bool,
}

/// 触发模式
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AutonomousTrigger {
    /// 手动启动，tick 驱动循环
    #[default]
    Manual,
    /// Cron 定时触发
    Cron { expression: String },
    /// Webhook 触发（HTTP POST）
    Webhook { path: String },
    /// 消息队列触发
    MessageQueue { topic: String },
}

/// 自主运行状态
#[derive(Debug, Clone)]
pub struct AutonomousState {
    pub session_id: SessionId,
    pub config: AutonomousConfig,
    pub status: AutonomousStatus,
    pub rounds_completed: u32,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub started_at: Instant,
    pub last_tick_at: Option<Instant>,
    pub user_online: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousStatus {
    Running,
    Sleeping(u64),  // 剩余秒数
    Paused,         // 用户暂停
    BudgetExhausted,
    RoundsExhausted,
    Completed,      // Agent 主动结束
    Failed(String),
}
```

### Harness 集成

在 `harness.rs` 的主循环结束处，检测自主模式：

```rust
// 在 AgentEvent::Completed 之前，检查自主模式
if let Some(ref auto_config) = config.autonomous {
    if auto_config.enabled {
        // 检查预算限制
        if auto_state.rounds_completed >= auto_config.max_autonomous_rounds {
            // 轮次耗尽
            let _ = tx.send(AgentEvent::AutonomousExhausted {
                reason: "max_rounds".into(),
            }).await;
        } else if auto_state.started_at.elapsed().as_secs() > auto_config.max_duration_secs {
            // 时间耗尽
            let _ = tx.send(AgentEvent::AutonomousExhausted {
                reason: "max_duration".into(),
            }).await;
        } else {
            // 检查 agent 是否调了 Sleep
            let sleep_duration = extract_sleep_call(&messages);
            let tick_delay = sleep_duration.unwrap_or(
                Duration::from_secs(auto_config.idle_sleep_secs)
            );

            // 发送 sleeping 事件
            let _ = tx.send(AgentEvent::AutonomousSleeping {
                duration_secs: tick_delay.as_secs(),
            }).await;

            // 等待 sleep 或用户干预
            tokio::select! {
                _ = tokio::time::sleep(tick_delay) => {
                    // Tick: 注入 tick 消息继续循环
                    let tick_msg = format!(
                        "<tick>{}</tick>",
                        chrono::Local::now().format("%H:%M:%S")
                    );
                    messages.push(ChatMessage::user(&tick_msg));
                    auto_state.rounds_completed += 1;
                    auto_state.last_tick_at = Some(Instant::now());

                    let _ = tx.send(AgentEvent::AutonomousTick {
                        round: auto_state.rounds_completed,
                    }).await;

                    continue; // 重新进入主循环
                }
                msg = user_interrupt_rx.recv() => {
                    // 用户消息到达，注入并继续
                    if let Some(user_msg) = msg {
                        messages.push(user_msg);
                        continue;
                    }
                }
                _ = pause_signal.notified() => {
                    // 用户暂停
                    auto_state.status = AutonomousStatus::Paused;
                    let _ = tx.send(AgentEvent::AutonomousPaused).await;
                    // 等待恢复信号
                    resume_signal.notified().await;
                    auto_state.status = AutonomousStatus::Running;
                    continue;
                }
            }
        }
    }
}
```

### 系统提示词追加

自主模式启用时，在系统提示词末尾追加：

```rust
const AUTONOMOUS_PROMPT: &str = r#"# 自主运行模式

你正在自主运行。你会收到 `<tick>` 提示作为心跳——把它当作"你醒了，看看有什么可做的"。

## 节奏控制
- 使用 sleep 工具控制等待间隔
- 积极工作时短间隔（5-10秒），等待慢操作时长间隔（30-60秒）
- 无事可做时必须调 sleep，不要输出"还在等待"等废话

## 行为偏好
- 偏向行动：读文件、搜代码、跑测试、改代码——不需要问就做
- 遇到不确定时，选一个合理方案执行，可以后续修正
- 到达里程碑时提交代码

## 输出简洁
- 只报告：需要用户决策的点、关键里程碑、错误/阻塞
- 不要解释每一步，不要列出读了哪些文件

## 用户在线感知
- 用户在线时：更协作式——重要决策前确认
- 用户离线时：全自主——决策+执行+提交
"#;
```

### Sleep 工具

```rust
/// Sleep 工具 — 自主模式的节奏控制器
pub struct SleepTool;

#[async_trait]
impl Tool for SleepTool {
    fn name(&self) -> &str { "sleep" }

    fn description(&self) -> &str {
        "等待指定秒数。自主模式下用于控制工作节奏。\
         用户可以随时打断。sleep 期间不消耗 token。\
         优先使用此工具而非 bash(sleep N)。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "seconds": {
                    "type": "integer",
                    "description": "等待秒数 (1-600)"
                },
                "reason": {
                    "type": "string",
                    "description": "等待原因（如'等待测试完成'）"
                }
            },
            "required": ["seconds"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let seconds = params["seconds"].as_u64().unwrap_or(30).min(600);
        let reason = params["reason"].as_str().unwrap_or("idle");

        // 实际等待由 harness 的 autonomous 循环控制
        // 这里只返回意图，harness 读取后执行 tokio::select! 等待
        Ok(ToolOutput::success(format!(
            "Sleeping for {} seconds (reason: {}). Will wake on tick or user message.",
            seconds, reason
        )))
    }

    fn is_read_only(&self) -> bool { true }
    fn risk_level(&self) -> RiskLevel { RiskLevel::ReadOnly }
    fn approval(&self) -> ApprovalRequirement { ApprovalRequirement::Never }
}
```

### 新增 AgentEvent 变体

```rust
pub enum AgentEvent {
    // ... 现有变体 ...

    /// 自主模式：进入睡眠
    AutonomousSleeping { duration_secs: u64 },
    /// 自主模式：tick 醒来
    AutonomousTick { round: u32 },
    /// 自主模式：用户暂停
    AutonomousPaused,
    /// 自主模式：预算/轮次耗尽
    AutonomousExhausted { reason: String },
}
```

---

## 三、企业长链条工作流场景

### 场景 1: 自动代码审查管道

```yaml
# 通过 API 创建自主 session
autonomous:
  enabled: true
  trigger:
    type: webhook
    path: /hooks/pr-review
  max_autonomous_rounds: 50
  max_duration_secs: 1800
  idle_sleep_secs: 10

# 初始 prompt（webhook payload 模板）
prompt: |
  新 PR 到达：{pr_url}
  请执行完整代码审查：
  1. 读取 PR diff
  2. 检查代码质量、安全性、测试覆盖
  3. 运行 lint 和 type check
  4. 生成结构化审查意见
  5. 如果有阻塞问题，标记为 request_changes
  6. 否则标记为 approved
```

### 场景 2: 数据质量监控

```yaml
autonomous:
  enabled: true
  trigger:
    type: cron
    expression: "0 */6 * * *"  # 每 6 小时
  max_autonomous_rounds: 20
  max_duration_secs: 3600

prompt: |
  执行数据质量巡检：
  1. 查询最近 6 小时的数据入库日志
  2. 检查完整性指标（缺失率、异常值）
  3. 对比历史基线，标记偏差
  4. 如果发现问题，生成告警报告
  5. 将巡检结果写入 /reports/data-quality/
```

### 场景 3: 基础设施自愈

```yaml
autonomous:
  enabled: true
  trigger:
    type: webhook
    path: /hooks/alert
  max_autonomous_rounds: 30
  max_cost_usd: 5.0
  user_presence_aware: true

prompt: |
  收到监控告警：{alert_json}
  请诊断并尝试修复：
  1. 分析告警内容和关联指标
  2. 检查相关服务日志
  3. 诊断根因
  4. 如果可自动修复（重启、扩容、配置调整），执行修复
  5. 如果需要人工介入，发送通知并附上诊断报告
  6. 验证修复效果
```

### 场景 4: 持续开发助手

```yaml
autonomous:
  enabled: true
  trigger:
    type: manual
  max_autonomous_rounds: 200
  max_duration_secs: 7200  # 2 小时
  idle_sleep_secs: 30
  active_sleep_secs: 5

prompt: |
  你是项目的持续开发助手。工作清单：
  1. 实现 context/compaction_pipeline.rs 中的 LLM 摘要压缩
  2. 实现 prompt_too_long 恢复路径
  3. 为每个实现编写测试
  4. 确保 cargo check + cargo test 通过
  5. 完成后生成 PR 描述

  每完成一个子任务就提交一次代码。
```

---

## 四、安全与合规

### 预算控制

| 维度 | 控制项 | 默认值 |
|------|--------|--------|
| 轮次 | max_autonomous_rounds | 100 |
| 时间 | max_duration_secs | 3600 (1h) |
| Token | max_tokens_per_round | 无限制 |
| 成本 | max_cost_usd | 无限制 |
| 操作 | SecurityPolicy rate limit | 20/hour |

### 审计

每个自主轮次记录：
- 轮次编号、时间戳
- LLM 输入/输出 token
- 工具调用列表和结果
- 权限决策
- Sleep 原因和持续时间

### 人工干预点

| 干预方式 | 触发 | 效果 |
|---------|------|------|
| WebSocket 消息 | 用户在 Web 前端发送消息 | 打断 Sleep，注入消息，agent 响应 |
| 暂停 | API 调用 `POST /sessions/{id}/pause` | 停止 tick，保持上下文 |
| 恢复 | API 调用 `POST /sessions/{id}/resume` | 恢复 tick 循环 |
| 终止 | API 调用 `DELETE /sessions/{id}` | 立即终止 |
| 权限审批 | ApprovalGate WebSocket | agent 请求审批时暂停等待 |

---

## 五、与现有架构的集成

```
AgentRuntime
├─ sessions: DashMap (已有)
├─ autonomous_scheduler: AutonomousScheduler (新增)
│   ├─ 管理自主 session 生命周期
│   ├─ Cron/Webhook/MQ 触发器
│   └─ 预算监控 + 审计日志
│
└─ AgentLoopConfig
   ├─ autonomous: Option<AutonomousConfig> (新增)
   └─ ... 现有字段

harness.rs
├─ 主循环正常执行
└─ 循环结束时检查 autonomous → Sleep/Tick/Continue

octo-server
├─ REST API: POST /autonomous/create (创建自主 session)
├─ REST API: POST /sessions/{id}/pause|resume
├─ WebSocket: 实时推送 AutonomousEvent
└─ Webhook endpoint: /hooks/{path} (触发自主 session)
```

---

## 六、工作量估算

| 组件 | 代码量 | 依赖 |
|------|--------|------|
| `AutonomousConfig` 数据结构 | ~80 行 | 无 |
| `AutonomousState` + `AutonomousStatus` | ~40 行 | 无 |
| harness.rs 自主循环集成 | ~80 行 | P0 上下文管理（压缩后继续循环） |
| `SleepTool` 工具 | ~50 行 | 无 |
| 自主模式系统提示词 | ~30 行 | 提示词增强 |
| `AutonomousScheduler` | ~100 行 | Cron scheduler (已有) |
| AgentEvent 新变体 | ~20 行 | 无 |
| REST API endpoints | ~60 行 | octo-server |
| WebSocket 事件推送 | ~30 行 | 已有 WS 基础 |
| **合计** | **~490 行** | |

---

## 七、实施优先级

| 阶段 | 内容 | 前置 |
|------|------|------|
| **Phase 1** | 基础自主循环（Manual 触发 + Sleep + Tick + 预算限制） | P0 上下文管理 |
| **Phase 2** | Webhook/Cron 触发器 + 暂停/恢复 | Phase 1 |
| **Phase 3** | 用户在线感知 + 多 session 并行 + 审计日志 | Phase 2 |

Phase 1 (~250 行) 即可提供完整的自主运行能力，后续 phase 是企业级增强。

---

## 八、与 CC KAIROS 的差异

| 维度 | CC KAIROS | Octo Autonomous |
|------|----------|-----------------|
| 产品形态 | 单用户 CLI | 多用户 server/platform |
| 并发 | 单个 session | 多个并行自主 session |
| 触发 | 手动 `--proactive` flag | Manual + Cron + Webhook + MQ |
| 监控 | 终端输出 | Web 前端 + API + WebSocket |
| 干预 | 终端键盘输入 | WebSocket + REST API |
| 预算控制 | 无（靠 prompt cache 5 分钟过期间接限制） | 显式轮次/时间/token/USD 限制 |
| 审计 | 无 | 完整审计日志 |
| 终端焦点 | iTerm2/Terminal 焦点事件 | WebSocket 连接状态 |
| 安全 | CC SecurityPolicy | Octo PermissionEngine + ApprovalGate |
