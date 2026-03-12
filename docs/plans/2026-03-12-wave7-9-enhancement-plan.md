# Wave 7-9 增强实施方案

> 基于竞品代码分析 V2 的 23 个真实差距，覆盖 P0/P1/P2 全部优先级。
> 预估总量：~6,930 行代码，23 个任务，3 个 Wave。
> 基线：1594 tests @ `763ab56` (Wave 6 COMPLETE)

---

## 总览

| Wave | 优先级 | 主题 | 任务数 | 预估 LOC | 目标提分 |
|------|--------|------|--------|----------|----------|
| 7 | P0 | 运行时防护强化 | 5 | ~1,680 | 7.55 → 8.1 |
| 8 | P1 | 集成与记忆增强 | 9 | ~3,480 | 8.1 → 8.6 |
| 9 | P2 | 精细优化 | 9 | ~1,770 | 8.6 → 8.9 |

### 依赖图

```
Wave 7 (无外部依赖，可立即启动)
├── T1 self_repair.rs (独立)
├── T2 pruner.rs 三策略 (独立)
├── T3 text tool recovery (独立)
├── T4 estop.rs (独立)
└── T5 prompt cache (独立)

Wave 8 (部分依赖 Wave 7)
├── T1 MCP OAuth (独立)
├── T2 LLM Reranking (独立)
├── T3 Session Thread/Turn (独立)
├── T4 Provider name mapping (独立)
├── T5 Retry enhancement (独立)
├── T6 Tool trait enhancement (独立)
├── T7 KG 工具暴露 (独立)
├── T8 动态工具预算 (依赖 W7-T2)
└── T9 rmcp 升级 (独立，但影响 MCP 全链路)

Wave 9 (部分依赖 Wave 8)
├── T1 RRF 融合 (独立)
├── T2 Merkle 审计 (独立)
├── T3 Priority queue (独立)
├── T4 Metering 持久化 (独立)
├── T5 Canary rotation (独立)
├── T6 MCP Server 角色 (依赖 W8-T9)
├── T7 图片 token 修正 (独立)
├── T8 ToolProgress 事件 (独立)
└── T9 Schema token 建模 (独立)
```

---

## Wave 7: 运行时防护强化（P0）

### T1: 自修复系统 (2-3 天, ~600 LOC)

**差距**: Agent 卡住时只能超时退出，无自动检测 + 修复能力。ironclaw 有 SelfRepair 模块可检测 stuck job 并重建 broken tool。

**新建文件**: `crates/octo-engine/src/agent/self_repair.rs`

**设计**:

```rust
// === 核心类型 ===

/// 修复结果
pub enum RepairResult {
    /// 修复成功，替换工具结果
    Repaired(String),
    /// 工具已重建，需要重新调用
    ToolRebuilt { tool_name: String },
    /// 无法修复，建议用户介入
    Unrecoverable { reason: String },
    /// 不需要修复
    NotNeeded,
}

/// 卡住检测器
pub struct StuckDetector {
    /// 同一工具连续失败次数阈值
    max_consecutive_failures: usize,
    /// 无进展超时（秒）
    no_progress_timeout: Duration,
    /// 历史记录
    tool_failure_counts: HashMap<String, usize>,
    last_progress_at: Instant,
}

/// 自修复管理器
pub struct SelfRepairManager {
    detector: StuckDetector,
    /// MCP tool 可重建
    mcp_manager: Option<Arc<McpManager>>,
    /// 内置工具不可重建，只做 fallback
    max_repair_attempts: usize,
}

impl SelfRepairManager {
    /// 在每次工具调用后检查是否需要修复
    pub async fn check_and_repair(
        &mut self,
        tool_name: &str,
        tool_result: &ToolOutput,
        context: &RepairContext,
    ) -> RepairResult { ... }

    /// 检测 stuck 状态
    fn detect_stuck(&self, tool_name: &str) -> StuckReason { ... }

    /// 尝试重建 MCP 工具（断线重连）
    async fn rebuild_mcp_tool(&self, tool_name: &str) -> Result<(), RepairError> { ... }

    /// 生成 fallback 提示让 LLM 换个方式
    fn generate_fallback_hint(&self, tool_name: &str, error: &str) -> String { ... }
}
```

**集成点**: `harness.rs` 工具执行后调用 `self_repair.check_and_repair()`

```rust
// harness.rs — 在工具执行结果返回后
let tool_output = execute_tool(...).await;
if let Some(repair_mgr) = &mut config.self_repair {
    match repair_mgr.check_and_repair(&tool_name, &tool_output, &ctx).await {
        RepairResult::Repaired(new_output) => { /* 使用修复后的结果 */ }
        RepairResult::ToolRebuilt { .. } => { /* 重新调用 */ }
        RepairResult::Unrecoverable { reason } => {
            // 发送 AgentEvent::Warning，让 LLM 知道并换策略
        }
        RepairResult::NotNeeded => {}
    }
}
```

**AgentLoopConfig 扩展**:

```rust
// loop_config.rs
pub struct AgentLoopConfig {
    // ... 现有字段
    pub self_repair: Option<SelfRepairManager>,
}
```

**测试**: 8-10 个单元测试
- 连续失败 N 次触发 stuck 检测
- MCP 工具断线 → 自动重连 → 重新调用
- 达到 max_repair_attempts 后返回 Unrecoverable
- 正常工具调用不触发修复

---

### T2: 上下文 Compaction 三策略 (2 天, ~400 LOC)

**差距**: 当前 `ContextPruner` 只做裁剪（截断、删除），无 LLM 摘要能力。ironclaw 的 ContextMonitor 有 MoveToWorkspace / Summarize / Truncate 三策略。

**修改文件**: `crates/octo-engine/src/context/pruner.rs`

**设计**: 在现有 6 级降级体系中，为 `AutoCompaction` 和 `OverflowCompaction` 增加可选的 LLM 摘要策略。

```rust
/// Compaction 策略枚举
#[derive(Debug, Clone, Default)]
pub enum CompactionStrategy {
    /// 仅截断（当前行为，默认）
    #[default]
    Truncate,
    /// LLM 摘要 — 将旧消息压缩为一条摘要
    Summarize,
    /// 移到工作空间 — 将旧消息保存到 session memory，替换为引用
    MoveToWorkspace,
}

/// Compaction 配置
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    pub strategy: CompactionStrategy,
    /// LLM 摘要用的 provider（Summarize 策略需要）
    pub summary_provider: Option<Arc<dyn Provider>>,
    /// 摘要最大 token 数
    pub summary_max_tokens: usize, // 默认 500
    /// 工作空间存储（MoveToWorkspace 策略需要）
    pub workspace_store: Option<Arc<dyn SessionStore>>,
}

impl ContextPruner {
    // 新增方法
    pub fn with_compaction_config(mut self, config: CompactionConfig) -> Self { ... }

    /// 扩展 auto_compaction，根据策略选择行为
    async fn auto_compaction_v2(
        &self,
        messages: &mut Vec<ChatMessage>,
        config: &CompactionConfig,
    ) -> usize {
        match config.strategy {
            CompactionStrategy::Truncate => self.auto_compaction(messages),
            CompactionStrategy::Summarize => {
                let old_messages = messages.drain(..boundary).collect::<Vec<_>>();
                let summary = self.summarize_messages(&old_messages, config).await;
                messages.insert(0, ChatMessage::system(summary));
                old_messages.len()
            }
            CompactionStrategy::MoveToWorkspace => {
                let old_messages = messages.drain(..boundary).collect::<Vec<_>>();
                // 保存到 workspace
                if let Some(store) = &config.workspace_store {
                    store.set_messages(&workspace_session_id, old_messages.clone()).await;
                }
                messages.insert(0, ChatMessage::system(
                    format!("[{} 条旧消息已移至工作空间，可通过 memory_recall 检索]", old_messages.len())
                ));
                old_messages.len()
            }
        }
    }

    /// 使用轻量 LLM 调用生成消息摘要
    async fn summarize_messages(
        &self,
        messages: &[ChatMessage],
        config: &CompactionConfig,
    ) -> String { ... }
}
```

**注意**: `ContextPruner::apply()` 当前是同步的。三策略中 Summarize 需要异步 LLM 调用。方案：
1. 新增 `pub async fn apply_async()` 方法，内部根据 config 选择同步/异步路径
2. 原 `apply()` 保持不变（向后兼容）
3. `harness.rs` 中切换为调用 `apply_async()`

**测试**: 6-8 个测试
- Truncate 策略保持现有行为
- Summarize 生成合理摘要（mock provider）
- MoveToWorkspace 正确保存旧消息到 store
- 空消息列表不触发 compaction

---

### T3: 文本工具调用恢复 (0.5 天, ~150 LOC)

**差距**: 某些 LLM（特别是开源模型通过 OpenAI-compatible API）会在文本中输出工具调用格式而非使用 structured tool_use。当前 harness 只处理 `ContentBlock::ToolUse`，文本中的工具调用被忽略。

**修改文件**: `crates/octo-engine/src/agent/harness.rs`

**设计**: 在 `StreamResult` 的 `tool_uses` 为空且 `full_text` 非空时，尝试从文本中解析工具调用。

```rust
/// 从 LLM 文本输出中解析工具调用（fallback）
fn parse_tool_calls_from_text(text: &str) -> Vec<PendingToolUse> {
    let mut results = Vec::new();

    // 策略 1: JSON 块解析 — 匹配 ```json ... ``` 中的 tool_call 结构
    for cap in JSON_BLOCK_RE.captures_iter(text) {
        if let Ok(parsed) = serde_json::from_str::<TextToolCall>(&cap[1]) {
            results.push(PendingToolUse {
                id: format!("text-recovery-{}", uuid::Uuid::new_v4()),
                name: parsed.name,
                input_json: serde_json::to_string(&parsed.arguments).unwrap_or_default(),
            });
        }
    }

    // 策略 2: 函数调用格式 — 匹配 <tool_name>...</tool_name> XML 格式
    if results.is_empty() {
        for cap in XML_TOOL_RE.captures_iter(text) {
            results.push(PendingToolUse {
                id: format!("text-recovery-{}", uuid::Uuid::new_v4()),
                name: cap[1].to_string(),
                input_json: cap[2].to_string(),
            });
        }
    }

    results
}

/// JSON 格式：{"name": "tool", "arguments": {...}}
#[derive(Deserialize)]
struct TextToolCall {
    name: String,
    arguments: serde_json::Value,
}
```

**集成点**: `harness.rs` 的 `run_agent_loop_inner` 中，在处理 `StreamResult` 后：

```rust
// 在 stream_result.tool_uses 为空时尝试文本恢复
let tool_uses = if stream_result.tool_uses.is_empty() && !stream_result.full_text.is_empty() {
    let recovered = parse_tool_calls_from_text(&stream_result.full_text);
    if !recovered.is_empty() {
        warn!("Recovered {} tool call(s) from text output", recovered.len());
        tx.send(AgentEvent::Warning("Tool calls recovered from text format".into())).await.ok();
    }
    recovered
} else {
    stream_result.tool_uses
};
```

**测试**: 4 个测试
- JSON 块格式恢复
- XML 格式恢复
- 正常 tool_use 不触发恢复
- 无效格式返回空

---

### T4: 紧急停止 E-Stop (1 天, ~250 LOC)

**差距**: 无法一键终止所有运行中的 agent。当前 `CancellationToken` 是 per-executor 的，无全局协调。

**新建文件**: `crates/octo-engine/src/agent/estop.rs`

**设计**:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

/// 全局紧急停止控制器
#[derive(Clone)]
pub struct EmergencyStop {
    /// 全局停止标志
    triggered: Arc<AtomicBool>,
    /// 广播通知通道
    notify_tx: broadcast::Sender<EStopReason>,
    /// 停止原因
    reason: Arc<std::sync::Mutex<Option<EStopReason>>>,
}

#[derive(Debug, Clone)]
pub enum EStopReason {
    /// 用户手动触发
    UserTriggered,
    /// 安全策略触发（如检测到 PII 泄露）
    SafetyViolation(String),
    /// 预算超限
    BudgetExceeded,
    /// 系统关闭
    SystemShutdown,
}

impl EmergencyStop {
    pub fn new() -> Self { ... }

    /// 触发紧急停止 — 所有 agent 应在下一个 check point 退出
    pub fn trigger(&self, reason: EStopReason) {
        self.triggered.store(true, Ordering::SeqCst);
        *self.reason.lock().unwrap() = Some(reason.clone());
        let _ = self.notify_tx.send(reason);
    }

    /// 检查是否已触发（harness 每轮检查）
    pub fn is_triggered(&self) -> bool {
        self.triggered.load(Ordering::SeqCst)
    }

    /// 订阅停止通知
    pub fn subscribe(&self) -> broadcast::Receiver<EStopReason> {
        self.notify_tx.subscribe()
    }

    /// 重置（用于测试或恢复）
    pub fn reset(&self) {
        self.triggered.store(false, Ordering::SeqCst);
        *self.reason.lock().unwrap() = None;
    }

    /// 获取停止原因
    pub fn reason(&self) -> Option<EStopReason> {
        self.reason.lock().unwrap().clone()
    }
}
```

**集成点**:

1. `AgentLoopConfig` 增加 `pub estop: Option<EmergencyStop>`
2. `harness.rs` 每轮循环开始时检查：
   ```rust
   if let Some(estop) = &config.estop {
       if estop.is_triggered() {
           tx.send(AgentEvent::EmergencyStopped(estop.reason())).await.ok();
           break;
       }
   }
   ```
3. `AgentRuntime` 持有全局 `EmergencyStop`，创建 executor 时注入
4. `AgentEvent` 增加 `EmergencyStopped(Option<EStopReason>)` variant

**REST API**:
- `POST /api/agents/estop` — 触发紧急停止
- `DELETE /api/agents/estop` — 重置
- `GET /api/agents/estop` — 查询状态

**测试**: 5-6 个测试
- 触发后 is_triggered 返回 true
- 多 subscriber 均收到通知
- 重置后可恢复
- harness 检查 estop 后正确退出

---

### T5: Prompt Cache 优化 (0.5 天, ~80 LOC)

**差距**: `SystemPromptBuilder` 将动态内容（当前时间、活跃工具列表等）嵌入 system prompt，每次调用都会改变 system prompt 内容，破坏 Anthropic/OpenAI 的 prompt cache。

**修改文件**: `crates/octo-engine/src/context/system_prompt.rs`

**设计**: 将动态内容从 system prompt 移到第一条 user message 的前缀。

```rust
/// 构建结果分为静态和动态两部分
pub struct PromptParts {
    /// 静态 system prompt（可被 API 缓存）
    pub system_prompt: String,
    /// 动态上下文（注入到第一条 user message 前）
    pub dynamic_context: String,
}

impl SystemPromptBuilder {
    /// 新方法：分离静态/动态内容
    pub fn build_separated(&self, config: &AgentConfig) -> PromptParts {
        let system_prompt = self.build_static_part(config);
        let dynamic_context = self.build_dynamic_part(config);
        PromptParts { system_prompt, dynamic_context }
    }

    /// 静态部分：角色定义、工具 schema、行为规则
    fn build_static_part(&self, config: &AgentConfig) -> String { ... }

    /// 动态部分：当前时间、活跃 MCP 服务器、session 状态
    fn build_dynamic_part(&self, config: &AgentConfig) -> String { ... }
}
```

**集成点**: `harness.rs` 中构建 `CompletionRequest` 时：

```rust
let parts = system_prompt_builder.build_separated(&agent_config);
// system prompt 保持稳定（可缓存）
let system_prompt = parts.system_prompt;
// 动态内容注入到 messages 的开头
if !parts.dynamic_context.is_empty() {
    messages.insert(0, ChatMessage::user(format!(
        "<context>\n{}\n</context>\n\n{}",
        parts.dynamic_context,
        // 原始第一条 user message 内容
        original_first_user_content
    )));
}
```

**测试**: 3 个测试
- 静态部分不含时间戳等动态内容
- 动态部分包含正确的运行时信息
- 分离后功能等价于原 build()

---

## Wave 8: 集成与记忆增强（P1）

### T1: MCP OAuth 2.1 (3-4 天, ~800 LOC)

**差距**: 无法连接需要 OAuth 认证的 MCP Server（如 GitHub MCP、企业内部 MCP）。moltis 已实现 RFC 9728/8414 的完整 OAuth 2.1 流程。

**新建文件**: `crates/octo-engine/src/mcp/oauth.rs`

**设计**:

```rust
/// OAuth 2.1 认证管理器
pub struct McpOAuthManager {
    /// Token 存储
    token_store: Box<dyn OAuthTokenStore>,
    /// HTTP 客户端
    http: reqwest::Client,
}

/// OAuth 配置（每个 MCP Server 独立）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    /// PKCE code verifier
    pub use_pkce: bool,
    /// 动态发现 (.well-known/oauth-authorization-server)
    pub discovery_url: Option<String>,
}

/// Token 存储 trait
#[async_trait]
pub trait OAuthTokenStore: Send + Sync {
    async fn get_token(&self, server_id: &str) -> Option<OAuthToken>;
    async fn save_token(&self, server_id: &str, token: OAuthToken);
    async fn delete_token(&self, server_id: &str);
}

/// SQLite token 存储
pub struct SqliteOAuthTokenStore { ... }

impl McpOAuthManager {
    /// RFC 8414 — 发现 OAuth metadata
    pub async fn discover(&self, base_url: &str) -> Result<OAuthMetadata> { ... }

    /// RFC 9728 — 路径感知的 MCP OAuth 发现
    pub async fn discover_mcp(&self, mcp_url: &str) -> Result<OAuthMetadata> { ... }

    /// PKCE 授权码流程
    pub async fn authorize_pkce(&self, config: &OAuthConfig) -> Result<OAuthToken> { ... }

    /// Token 刷新
    pub async fn refresh_token(&self, server_id: &str) -> Result<OAuthToken> { ... }

    /// 获取有效 token（自动刷新）
    pub async fn get_valid_token(&self, server_id: &str) -> Result<String> { ... }
}
```

**集成点**:
1. `McpServerConfig` 增加 `pub oauth: Option<OAuthConfig>` 字段
2. `McpManager::connect()` 连接前检查 OAuth 配置，自动获取 token
3. SSE 客户端请求头增加 `Authorization: Bearer {token}`
4. Token 过期自动刷新，刷新失败触发重新认证

**McpStorage 扩展**:
- 新增 `oauth_tokens` 表：`server_id, access_token, refresh_token, expires_at`

**测试**: 8-10 个测试

---

### T2: LLM Reranking (1-2 天, ~300 LOC)

**差距**: `HybridQueryEngine` 混合搜索使用硬编码 0.3/0.7 权重融合 FTS 和 Vector 结果，无二次排序。moltis 实现了 retrieve-then-rerank 模式。

**修改文件**: `crates/octo-engine/src/memory/sqlite_store.rs`

**新建文件**: `crates/octo-engine/src/memory/reranker.rs`

**设计**:

```rust
/// Reranking 策略
pub enum RerankStrategy {
    /// 不做 reranking（当前行为）
    None,
    /// LLM-based reranking
    Llm(LlmRerankerConfig),
    /// Cross-encoder（未来扩展）
    CrossEncoder,
}

pub struct LlmRerankerConfig {
    pub provider: Arc<dyn Provider>,
    /// 最大 rerank 候选数
    pub max_candidates: usize, // 默认 20
    /// rerank 后保留 top-k
    pub top_k: usize,          // 默认 5
}

pub struct LlmReranker {
    config: LlmRerankerConfig,
}

impl LlmReranker {
    /// 对检索结果进行 LLM 二次排序
    pub async fn rerank(
        &self,
        query: &str,
        candidates: Vec<MemoryEntry>,
    ) -> Vec<MemoryEntry> {
        // 1. 构建 reranking prompt
        // 2. 调用轻量 LLM（haiku 级别）评分
        // 3. 按相关性得分重排
        // 4. 返回 top_k
    }
}
```

**集成点**: `HybridQueryEngine::search()` 返回结果后，可选调用 reranker：

```rust
let results = hybrid_engine.search(query, limit * 2).await?;
if let Some(reranker) = &self.reranker {
    reranker.rerank(query, results).await
} else {
    results.truncate(limit);
    results
}
```

**测试**: 4-5 个测试

---

### T3: Session Thread/Turn 模型 (2-3 天, ~500 LOC)

**差距**: 当前 `SessionData` 是扁平结构，无法区分对话线程和单轮对话。ironclaw 实现了 Session → Thread → Turn 三层模型，支持 undo。

**修改文件**: `crates/octo-engine/src/session/mod.rs`, `crates/octo-engine/src/session/sqlite.rs`

**设计**:

```rust
/// 对话线程（一个 session 可有多个 thread）
#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    pub thread_id: String,
    pub session_id: SessionId,
    pub title: Option<String>,
    pub created_at: i64,
    pub parent_thread_id: Option<String>, // 支持分叉
}

/// 单轮对话（一个 thread 内的一次 user→assistant 交互）
#[derive(Debug, Clone, Serialize)]
pub struct Turn {
    pub turn_id: String,
    pub thread_id: String,
    pub user_message: ChatMessage,
    pub assistant_messages: Vec<ChatMessage>, // 包含 tool calls
    pub created_at: i64,
}

/// 扩展 SessionStore trait
#[async_trait]
pub trait SessionStoreV2: SessionStore {
    // Thread 操作
    async fn create_thread(&self, session_id: &SessionId, title: Option<&str>) -> Thread;
    async fn list_threads(&self, session_id: &SessionId) -> Vec<Thread>;
    async fn fork_thread(&self, thread_id: &str, from_turn: &str) -> Thread;

    // Turn 操作
    async fn push_turn(&self, thread_id: &str, turn: Turn);
    async fn list_turns(&self, thread_id: &str, limit: usize, offset: usize) -> Vec<Turn>;

    // Undo
    async fn undo_last_turn(&self, thread_id: &str) -> Option<Turn>;
    async fn get_thread_messages(&self, thread_id: &str) -> Vec<ChatMessage>;
}
```

**SQLite 表**:

```sql
CREATE TABLE threads (
    thread_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    title TEXT,
    parent_thread_id TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(session_id)
);

CREATE TABLE turns (
    turn_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    user_message_json TEXT NOT NULL,
    assistant_messages_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (thread_id) REFERENCES threads(thread_id)
);
```

**向后兼容**: `SessionStore` trait 保持不变。`SessionStoreV2` 继承 `SessionStore`。现有代码使用默认线程。

**测试**: 8-10 个测试

---

### T4: Provider Name 映射表 (0.5 天, ~120 LOC)

**差距**: 用户必须手动填写 `base_url`，不能直接写 `provider: "deepseek"` 或 `provider: "azure"`。

**新建文件**: `crates/octo-engine/src/providers/defaults.rs`

**设计**:

```rust
use std::collections::HashMap;
use once_cell::sync::Lazy;

/// 常见 Provider 的 name → base_url 映射
static PROVIDER_DEFAULTS: Lazy<HashMap<&str, ProviderDefaults>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("openai", ProviderDefaults { base_url: "https://api.openai.com/v1", api_key_env: "OPENAI_API_KEY" });
    m.insert("anthropic", ProviderDefaults { base_url: "https://api.anthropic.com", api_key_env: "ANTHROPIC_API_KEY" });
    m.insert("deepseek", ProviderDefaults { base_url: "https://api.deepseek.com/v1", api_key_env: "DEEPSEEK_API_KEY" });
    m.insert("ollama", ProviderDefaults { base_url: "http://localhost:11434/v1", api_key_env: "" });
    m.insert("azure", ProviderDefaults { base_url: "", api_key_env: "AZURE_OPENAI_API_KEY" }); // 需要用户填 endpoint
    m.insert("together", ProviderDefaults { base_url: "https://api.together.xyz/v1", api_key_env: "TOGETHER_API_KEY" });
    m.insert("groq", ProviderDefaults { base_url: "https://api.groq.com/openai/v1", api_key_env: "GROQ_API_KEY" });
    m.insert("moonshot", ProviderDefaults { base_url: "https://api.moonshot.cn/v1", api_key_env: "MOONSHOT_API_KEY" });
    m.insert("zhipu", ProviderDefaults { base_url: "https://open.bigmodel.cn/api/paas/v4", api_key_env: "ZHIPU_API_KEY" });
    m.insert("qwen", ProviderDefaults { base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1", api_key_env: "DASHSCOPE_API_KEY" });
    m.insert("minimax", ProviderDefaults { base_url: "https://api.minimax.chat/v1", api_key_env: "MINIMAX_API_KEY" });
    m.insert("yi", ProviderDefaults { base_url: "https://api.lingyiwanwu.com/v1", api_key_env: "YI_API_KEY" });
    m.insert("baichuan", ProviderDefaults { base_url: "https://api.baichuan-ai.com/v1", api_key_env: "BAICHUAN_API_KEY" });
    // ... 等 ~30 个
    m
});

pub struct ProviderDefaults {
    pub base_url: &'static str,
    pub api_key_env: &'static str,
}

/// 根据 provider name 解析 base_url
pub fn resolve_provider(name: &str, explicit_base_url: Option<&str>) -> Option<String> {
    if let Some(url) = explicit_base_url {
        return Some(url.to_string());
    }
    PROVIDER_DEFAULTS.get(name.to_lowercase().as_str())
        .map(|d| d.base_url.to_string())
}
```

**集成点**: `ProviderChain::from_config()` 中，如果用户只写了 `name` 没写 `base_url`，自动查表。

**测试**: 3 个测试

---

### T5: 结构化 API 重试增强 (0.5 天, ~100 LOC)

**差距**: 当前 `retry.rs` 的错误分类基于字符串匹配，不解析 HTTP 响应头。缺少 `Retry-After` header 解析和 billing 错误精确识别。

**修改文件**: `crates/octo-engine/src/providers/retry.rs`

**设计**:

```rust
/// 从 HTTP 响应中提取重试信息
pub struct RetryInfo {
    pub kind: LlmErrorKind,
    pub retry_after: Option<Duration>,
    pub error_code: Option<String>,
}

impl RetryInfo {
    /// 从 HTTP status + headers + body 构建
    pub fn from_response(status: u16, headers: &HeaderMap, body: &str) -> Self {
        let kind = Self::classify_status(status, body);
        let retry_after = Self::parse_retry_after(headers);
        let error_code = Self::extract_error_code(body);
        Self { kind, retry_after, error_code }
    }

    /// 解析 Retry-After header（秒数或 HTTP-date）
    fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
        headers.get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                // 尝试解析为秒数
                if let Ok(secs) = s.parse::<u64>() {
                    return Some(Duration::from_secs(secs));
                }
                // 尝试解析为 HTTP-date
                httpdate::parse_http_date(s).ok()
                    .map(|dt| dt.duration_since(SystemTime::now()).unwrap_or_default())
            })
    }

    /// 精确分类 HTTP status
    fn classify_status(status: u16, body: &str) -> LlmErrorKind {
        match status {
            429 => LlmErrorKind::RateLimit,
            402 => LlmErrorKind::BillingError,
            401 | 403 => LlmErrorKind::AuthError,
            529 => LlmErrorKind::Overloaded,
            408 | 504 => LlmErrorKind::Timeout,
            500 | 502 | 503 => LlmErrorKind::ServiceError,
            _ if body.contains("credit_balance_too_low") => LlmErrorKind::BillingError,
            _ if body.contains("context_length_exceeded") => LlmErrorKind::ContextOverflow,
            _ => LlmErrorKind::Unknown,
        }
    }
}
```

**集成点**: 在 `anthropic.rs` 和 `openai.rs` 的 HTTP 错误处理中，使用 `RetryInfo::from_response()` 替代当前的 `classify_from_str()`。

**测试**: 5 个测试

---

### T6: Tool Trait 增强 (1 天, ~200 LOC)

**差距**: `Tool` trait 缺少 `execution_timeout`、`rate_limit`、`sensitive_params` 方法。ironclaw 的 Tool trait 有这些元数据。

**修改文件**: `crates/octo-engine/src/tools/traits.rs`

**设计**: 为 `Tool` trait 增加带默认实现的方法（不破坏现有工具）。

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    // 现有方法保持不变
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &ToolSpec;
    async fn execute(&self, params: serde_json::Value) -> ToolOutput;

    // 新增方法（带默认实现）

    /// 工具执行超时（默认 30 秒）
    fn execution_timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    /// 速率限制（每分钟最大调用次数，0 = 无限制）
    fn rate_limit(&self) -> u32 {
        0
    }

    /// 敏感参数名列表（这些参数在日志中会被脱敏）
    fn sensitive_params(&self) -> Vec<&str> {
        vec![]
    }

    /// 工具类别标签（用于分组展示和权限控制）
    fn category(&self) -> &str {
        "general"
    }
}
```

**集成点**:
1. `harness.rs` 工具执行时使用 `tool.execution_timeout()` 设置 `tokio::time::timeout`
2. 审计日志中对 `sensitive_params()` 列出的字段进行脱敏
3. BashTool 设置 `execution_timeout = 120s`, `sensitive_params = ["command"]`（可选）

**测试**: 4 个测试

---

### T7: KnowledgeGraph 工具暴露 (1 天, ~300 LOC)

**差距**: KnowledgeGraph 模块已实现（`memory/knowledge_graph.rs`）但 LLM 无法通过工具调用访问。

**新建文件**: `crates/octo-engine/src/tools/knowledge_graph.rs`

**设计**: 3 个工具暴露 KG 能力。

```rust
/// graph_query — 查询知识图谱
pub struct GraphQueryTool { kg: Arc<KnowledgeGraph> }
// 参数: { "query": "string", "entity_type": "optional string", "limit": "optional number" }
// 返回: 匹配的实体和关系列表

/// graph_add — 添加实体和关系
pub struct GraphAddTool { kg: Arc<KnowledgeGraph> }
// 参数: { "entities": [{"name", "type", "properties"}], "relations": [{"from", "to", "type"}] }
// 返回: 添加成功的数量

/// graph_relate — 查询实体关系路径
pub struct GraphRelateTool { kg: Arc<KnowledgeGraph> }
// 参数: { "from_entity": "string", "to_entity": "string", "max_hops": "optional number" }
// 返回: 连接路径
```

**集成点**: `ToolRegistry::register_memory_tools()` 中增加 KG 工具注册。

**测试**: 5 个测试

---

### T8: 动态工具结果预算 (0.5 天, ~30 LOC)

**差距**: `harness.rs` 中 `TOOL_RESULT_SOFT_LIMIT = 30_000` 和 `MAX_TOOL_OUTPUT_SIZE = 100_000` 是硬编码常量。应与 context_window 成比例。

**修改文件**: `crates/octo-engine/src/agent/harness.rs`

**设计**:

```rust
/// 计算动态工具结果限制（context_window 的 30%）
fn tool_result_budget(context_window: usize) -> (usize, usize) {
    let soft_limit = (context_window as f64 * 0.15).min(50_000.0) as usize;
    let hard_limit = (context_window as f64 * 0.30).min(200_000.0) as usize;
    (soft_limit.max(8_000), hard_limit.max(30_000))
}
```

**集成点**: 从 `AgentLoopConfig` 获取 `context_window` 大小，替代硬编码常量。

**测试**: 2 个测试

---

### T9: rmcp 升级 0.16 → 1.x (1-2 天, ~400 LOC)

**差距**: rmcp 0.16 不支持 StreamableHTTP transport。goose 已使用最新版本。

**修改文件**: `Cargo.toml`（workspace 和 octo-engine）, `crates/octo-engine/src/mcp/` 下多个文件

**设计**:
1. 更新 `Cargo.toml` 中 rmcp 版本
2. 适配 API 变更（trait 签名、类型名称）
3. 新增 StreamableHTTP transport 支持
4. 保持 stdio 和 SSE 向后兼容

**注意**: 这是风险较高的任务，建议放在 Wave 8 最后执行。需要逐个适配 `traits.rs`、`stdio.rs`、`sse.rs`、`bridge.rs`、`manager.rs`。

**测试**: 现有 MCP 相关测试全部通过 + 新增 StreamableHTTP 测试

---

## Wave 9: 精细优化（P2）

### T1: RRF 融合替代硬编码权重 (0.5 天, ~100 LOC)

**修改文件**: `crates/octo-engine/src/memory/sqlite_store.rs`

**设计**: 用 Reciprocal Rank Fusion 替代 `0.3 * fts_score + 0.7 * vec_score`。

```rust
/// Reciprocal Rank Fusion（k=60 是标准值）
fn rrf_fuse(fts_results: &[ScoredEntry], vec_results: &[ScoredEntry], k: f64) -> Vec<ScoredEntry> {
    let mut scores: HashMap<String, f64> = HashMap::new();

    for (rank, entry) in fts_results.iter().enumerate() {
        *scores.entry(entry.id.clone()).or_default() += 1.0 / (k + rank as f64 + 1.0);
    }
    for (rank, entry) in vec_results.iter().enumerate() {
        *scores.entry(entry.id.clone()).or_default() += 1.0 / (k + rank as f64 + 1.0);
    }

    // 按 RRF 分数排序返回
    ...
}
```

**测试**: 3 个测试

---

### T2: Merkle 链审计防篡改 (0.5 天, ~120 LOC)

**修改文件**: `crates/octo-engine/src/audit/storage.rs`

**设计**: `AuditRecord` 增加 `prev_hash` 和 `hash` 字段，形成哈希链。

```rust
pub struct AuditRecord {
    // 现有字段...
    pub prev_hash: String,  // 前一条记录的 hash
    pub hash: String,       // SHA-256(prev_hash + event_data)
}

impl AuditStorage {
    /// 插入审计记录时自动计算哈希链
    pub async fn insert_chained(&self, event: AuditEvent) -> AuditRecord { ... }

    /// 验证审计链完整性
    pub async fn verify_chain(&self, from: i64, to: i64) -> ChainVerifyResult { ... }
}
```

**测试**: 4 个测试

---

### T3: 消息优先级队列 (0.5 天, ~150 LOC)

**修改文件**: `crates/octo-engine/src/agent/queue.rs`

**设计**: 为 `MessageQueue` 增加优先级支持（steering 消息可插队）。

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,      // 用户中断
    Critical = 3,  // 系统 steering / E-Stop
}

impl MessageQueue {
    pub fn push_with_priority(&mut self, msg: AgentMessage, priority: MessagePriority) { ... }
    // 高优先级消息排在队列前面
}
```

**测试**: 3 个测试

---

### T4: 计量持久化 + 模型定价表 (1 天, ~300 LOC)

**新建文件**: `crates/octo-engine/src/metering/storage.rs`, `crates/octo-engine/src/metering/pricing.rs`

**设计**:
- SQLite 表存储 per-session/per-model 的 token 使用量
- ~40 个常见模型的定价表（input/output per 1M tokens）
- 成本计算：`tokens * price_per_token`

**测试**: 5 个测试

---

### T5: Per-turn Canary Rotation (0.5 天, ~80 LOC)

**修改文件**: `crates/octo-engine/src/security/canary.rs`（如存在）或相关安全模块

**设计**: 每轮对话生成新的 canary token，防止 LLM 在多轮中泄露固定 canary。

```rust
impl CanaryGuard {
    /// 生成本轮的 canary token
    pub fn rotate(&mut self) -> String {
        self.current_canary = format!("CANARY-{}", uuid::Uuid::new_v4().to_string()[..8]);
        self.current_canary.clone()
    }
}
```

**测试**: 2 个测试

---

### T6: MCP Server 角色 (2-3 天, ~400 LOC)

**差距**: octo-sandbox 只能作为 MCP client 连接外部 server，不能将自身工具暴露为 MCP server 供其他 agent 调用。

**新建文件**: `crates/octo-engine/src/mcp/server.rs`

**设计**: 使用 rmcp 框架将 ToolRegistry 中的工具暴露为 MCP server。

**前置依赖**: Wave 8 T9 (rmcp 升级)

**测试**: 5 个测试

---

### T7: 图片 Token 估算修正 (0.5 天, ~40 LOC)

**修改文件**: `crates/octo-engine/src/context/budget.rs`

**设计**: 替代 `base64_length / 4` 的粗糙估算，使用 Anthropic/OpenAI 官方公式。

```rust
/// Anthropic: 基于图片像素数，每 750 像素约 1 token
/// 简化公式：固定 tile (768x768) = 258 tokens, low-res = 85 tokens
fn estimate_image_tokens(width: u32, height: u32, detail: ImageDetail) -> usize {
    match detail {
        ImageDetail::Low => 85,
        ImageDetail::High => {
            let tiles = ((width + 767) / 768) * ((height + 767) / 768);
            85 + 170 * tiles as usize
        }
        ImageDetail::Auto => 258, // 单 tile 默认值
    }
}
```

**测试**: 3 个测试

---

### T8: ToolProgress 事件 (0.5 天, ~80 LOC)

**修改文件**: `crates/octo-engine/src/agent/events.rs`

**设计**: 为长时间工具执行（如 bash、web_fetch）提供进度反馈。

```rust
pub enum AgentEvent {
    // 现有 variants...

    /// 工具执行进度
    ToolProgress {
        tool_call_id: String,
        tool_name: String,
        progress: f32,      // 0.0 - 1.0
        message: String,
    },
}
```

**集成点**: BashTool 执行时通过 channel 发送 stdout 进度行。

**测试**: 2 个测试

---

### T9: 结构化工具 Schema Token 建模 (1 天, ~200 LOC)

**修改文件**: `crates/octo-engine/src/context/budget.rs`

**设计**: 精确估算工具 schema 占用的 token 数，替代当前粗糙的字符数估算。

```rust
/// 参考 Anthropic 文档的工具 token 消耗公式
/// FUNC_INIT = 7 tokens (function header)
/// PROP_KEY = 3 tokens per property
/// PROP_DESC ≈ description_tokens
fn estimate_tool_schema_tokens(tools: &[ToolSpec]) -> usize {
    tools.iter().map(|t| {
        let func_init = 7;
        let name_tokens = estimate_string_tokens(&t.name);
        let desc_tokens = estimate_string_tokens(&t.description);
        let param_tokens: usize = t.parameters.iter().map(|p| {
            3 + estimate_string_tokens(&p.name)
              + estimate_string_tokens(&p.description)
              + if p.enum_values.is_some() { 5 } else { 0 }
        }).sum();
        func_init + name_tokens + desc_tokens + param_tokens
    }).sum()
}
```

**集成点**: `ContextBudgetManager` 在计算可用 budget 时扣除工具 schema 占用。

**测试**: 4 个测试

---

## 并行执行策略

### Wave 7（5 个任务全部可并行）

```
Agent-1: T1 self_repair.rs        (2-3 天)
Agent-2: T2 pruner.rs 三策略       (2 天)
Agent-3: T3 text recovery          (0.5 天) → T4 estop.rs (1 天) → T5 prompt cache (0.5 天)
```

**预计 Wall Time**: 3 天（假设 3 个并行 agent）

### Wave 8（9 个任务，大部分可并行）

```
Agent-1: T1 MCP OAuth              (3-4 天)
Agent-2: T3 Session Thread/Turn    (2-3 天) → T7 KG 工具 (1 天)
Agent-3: T2 LLM Reranking (1-2天) → T6 Tool trait (1天) → T8 动态预算 (0.5天)
Agent-4: T4 Provider mapping (0.5天) → T5 Retry 增强 (0.5天)
Agent-5: T9 rmcp 升级              (最后执行, 1-2 天)
```

**预计 Wall Time**: 5-6 天

### Wave 9（9 个任务，全部可并行，按需选择）

```
Agent-1: T6 MCP Server 角色        (2-3 天, 依赖 W8-T9)
Agent-2: T4 Metering 持久化        (1 天)
Agent-3: T9 Schema token (1天) → T7 图片 token (0.5天)
Agent-4: T1 RRF (0.5天) → T2 Merkle (0.5天) → T3 Priority queue (0.5天)
Agent-5: T5 Canary (0.5天) → T8 ToolProgress (0.5天)
```

**预计 Wall Time**: 3 天

---

## 验收标准

### 每个任务必须满足

1. **编译通过**: `cargo check --workspace` 无 error
2. **测试通过**: 所有新增测试 + 现有 1594 测试不回归
3. **向后兼容**: 不破坏现有 API（新增 trait 方法必须有默认实现）
4. **代码审查**: clippy 零 warning

### Wave 级验收

| Wave | 预期测试增量 | 累计测试数 | 关键指标 |
|------|-------------|-----------|----------|
| 7 | +26-32 | ~1620-1626 | Self-repair 检测率 > 90%, E-Stop 延迟 < 10ms |
| 8 | +40-50 | ~1660-1676 | OAuth 流程完整, Session undo 正确, Provider 自动识别 |
| 9 | +25-30 | ~1685-1706 | RRF 优于硬编码权重, Merkle 链验证通过 |

### 评分目标

| 维度 | 当前 | W7 后 | W8 后 | W9 后 |
|------|------|-------|-------|-------|
| Agent 架构 | 9.0 | 9.5 | 9.5 | 9.5 |
| Provider 层 | 8.4 | 8.4 | 8.8 | 8.8 |
| Tool 系统 | 6.8 | 7.0 | 7.8 | 8.0 |
| MCP 集成 | 6.4 | 6.4 | 7.5 | 8.0 |
| Memory 系统 | 6.4 | 6.4 | 7.2 | 7.5 |
| Context Eng. | 7.5 | 8.5 | 8.5 | 8.8 |
| 安全模型 | 8.5 | 9.0 | 9.0 | 9.2 |
| 多租户 | 6.9 | 6.9 | 6.9 | 6.9 |
| **加权总分** | **7.55** | **8.1** | **8.6** | **8.9** |
