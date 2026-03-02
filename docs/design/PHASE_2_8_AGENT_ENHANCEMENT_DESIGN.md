# Phase 2.8 Agent 增强 + Secret Manager 设计方案

**版本**: v1.0
**创建日期**: 2026-03-02
**目标**: 对标 OpenFang + pi_agent_rust 企业级安全与 Agent 能力

---

## 一、设计目标

### 1.1 当前状态

| 模块 | 状态 | 说明 |
|------|------|------|
| Agent Loop | ✅ | 30轮, Loop Guard, EventBus |
| Tools | ✅ | 12个内置工具 |
| Secret | ❌ | 无加密存储 |
| Extension | ✅ | 基础生命周期 |

### 1.2 目标能力

| 能力 | 当前 | 目标 | 来源 |
|------|------|------|------|
| **最大轮数** | 30轮 | 50轮/无限 | pi_agent_rust |
| **工具并行** | 顺序 | 8并行 | pi_agent_rust |
| **取消信号** | 无 | AbortSignal | pi_agent_rust |
| **Extension** | 基础 | 完整钩子 | pi_agent_rust |
| **Typing信号** | 无 | 有 | openclaw |
| **Secret存储** | 明文 | AES-256-GCM | OpenFang |
| **信息流安全** | 无 | Taint Tracking | OpenFang |
| **OAuth2** | 无 | PKCE | OpenFang |

---

## 二、架构设计

### 2.1 模块总览

```
crates/octo-engine/src/
├── secret/                    # [NEW] Secret Manager
│   ├── mod.rs                 # SecretManager 主模块
│   ├── vault.rs               # CredentialVault (加密存储)
│   ├── resolver.rs            # CredentialResolver (优先级链)
│   ├── taint.rs               # Taint Tracking (信息流安全)
│   └── oauth.rs               # OAuth2 PKCE (P2)
│
├── agent/
│   ├── loop_.rs               # [MODIFIED] 50轮/无限 + Typing
│   ├── extension.rs           # [NEW] Extension 钩子系统
│   ├── cancellation.rs        # [NEW] AbortSignal
│   └── parallel.rs            # [NEW] 并行执行引擎
│
└── tools/
    └── mod.rs                 # [MODIFIED] 集成 CancellationToken
```

### 2.2 Secret Manager 架构

```
┌─────────────────────────────────────────────────────────────┐
│                    SecretManager                             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │           CredentialResolver (优先级链)               │   │
│  │  ┌─────────┐ → ┌─────────┐ → ┌─────────┐ → ERROR  │   │
│  │  │  Vault  │   │ .env    │   │   Env   │          │   │
│  │  │(加密)   │   │ (文件)  │   │ (环境)  │          │   │
│  │  └─────────┘   └─────────┘   └─────────┘          │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              CredentialVault (加密存储)               │   │
│  │  ┌──────────────────────────────────────────────┐   │   │
│  │  │  Master Key 来源:                            │   │   │
│  │  │  1. macOS Keychain ← 首选 (machine-specific)│   │   │
│  │  │  2. Windows Credential Manager              │   │   │
│  │  │  3. SECRET_MASTER_KEY env ← fallback       │   │   │
│  │  └──────────────────────────────────────────────┘   │   │
│  │                                                      │   │
│  │  加密: AES-256-GCM                                  │   │
│  │  派生: Argon2id (m=65536, t=3, p=4)               │   │
│  │  存储: ~/.octo/vault.enc (mode 0600)              │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Taint Tracking (信息流安全)             │   │
│  │  ┌───────────┐    ┌───────────┐    ┌───────────┐  │   │
│  │  │ Tainted  │ →  │  Sink     │ →  │ Violation │  │   │
│  │  │  Value   │    │ (shell/   │    │  Error    │  │   │
│  │  │          │    │  net)     │    │           │  │   │
│  │  └───────────┘    └───────────┘    └───────────┘  │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 2.3 Agent Loop 增强架构

```
┌─────────────────────────────────────────────────────────────┐
│                    AgentLoop (增强版)                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  配置:                                                       │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ max_rounds: u32 = 50  // 0 = 无限轮                │   │
│  │ enable_parallel: bool = false // P1 开启            │   │
│  │ max_parallel_tools: u8 = 8                         │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
│  执行流程:                                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                                                     │   │
│  │  for round in 1..=max_rounds {                    │   │
│  │    ┌─────────────────────────────────────────┐     │   │
│  │    │ Extension::on_turn_start()              │     │   │
│  │    └─────────────────────────────────────────┘     │   │
│  │           ↓                                      │   │
│  │    ┌─────────────────────────────────────────┐     │   │
│  │    │ LLM API Call                            │     │   │
│  │    │ - 发送 Typing 信号 (如适用)              │     │   │
│  │    └─────────────────────────────────────────┘     │   │
│  │           ↓                                      │   │
│  │    处理 Tool Calls:                                │   │
│  │    ┌─────────────────────────────────────────┐     │   │
│  │    │ if enable_parallel {                    │     │   │
│  │    │   // 并行执行                            │     │   │
│  │    │   let results = join_all(tools).await; │     │   │
│  │    │ } else {                                │     │   │
│  │    │   // 顺序执行 (现有)                     │     │   │
│  │    │   for tool in tools { ... }             │     │   │
│  │    │ }                                       │     │   │
│  │    └─────────────────────────────────────────┘     │   │
│  │           ↓                                      │   │
│  │    ┌─────────────────────────────────────────┐     │   │
│  │    │ Taint Tracking: 检查 tool input 是否    │     │   │
│  │    │ 包含 secret，阻止泄露到危险 sink         │     │   │
│  │    └─────────────────────────────────────────┘     │   │
│  │           ↓                                      │   │
│  │    ┌─────────────────────────────────────────┐     │   │
│  │    │ Extension::on_tool_success/error()      │     │   │
│  │    └─────────────────────────────────────────┘     │   │
│  │           ↓                                      │   │
│  │    if stop_reason == "end_turn" { break; }       │   │
│  │  }                                              │   │
│  │                                                     │   │
│  │  ┌─────────────────────────────────────────┐     │   │
│  │  │ Extension::on_turn_end()                │     │   │
│  │  └─────────────────────────────────────────┘     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 三、详细设计

### 3.1 Secret Manager

#### 3.1.1 CredentialVault

```rust
// crates/octo-engine/src/secret/vault.rs

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use zeroize::Zeroizing;

pub struct CredentialVault {
    store: EncryptedStore,
    master_key: Zeroizing<[u8; 32]>,
}

pub struct EncryptedStore {
    version: u8,           // 当前版本 = 1
    salt: [u8; 16],       // 随机 salt
    nonce: [u8; 12],      // AES-GCM nonce
    ciphertext: Vec<u8>,  // 加密后的 JSON
}

impl CredentialVault {
    /// 初始化 vault (首次使用)
    pub fn init(&mut self, master_password: &str) -> Result<()> {
        // 1. 生成随机 salt (16 bytes)
        // 2. Argon2id(master_password, salt) → 32B key
        // 3. 生成随机 nonce (12 bytes)
        // 4. 加密空 HashMap "{}"
        // 5. 保存到 ~/.octo/vault.enc
    }

    /// 解锁 vault (每次启动)
    pub fn unlock(&mut self, master_password: &str) -> Result<()> {
        // 1. 从文件读取 vault
        // 2. Argon2id(master_password, salt) → key
        // 3. AES-256-GCM 解密
        // 4. 反序列化 HashMap
    }

    /// 存储密钥
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        // 1. 获取写锁
        // 2. 更新 HashMap
        // 3. 重新加密
        // 4. 写入文件
    }

    /// 获取密钥 (返回 Zeroizing)
    pub fn get(&self, key: &str) -> Option<Zeroizing<String>> {
        // 返回内存安全的密钥
    }
}
```

#### 3.1.2 CredentialResolver

```rust
// crates/octo-engine/src/secret/resolver.rs

pub struct CredentialResolver {
    vault: Option<Arc<CredentialVault>>,
    dotenv_path: Option<PathBuf>,
    user_id: Option<UserId>,
}

impl CredentialResolver {
    /// 解析密钥 (优先级链)
    pub fn resolve(&self, key: &str) -> Option<Zeroizing<String>> {
        // 1. Vault
        if let Some(ref v) = self.vault {
            if let Some(val) = v.get(key) {
                return Some(val);
            }
        }

        // 2. Dotenv 文件 (~/.octo/.env)
        if let Some(path) = &self.dotenv_path {
            if let Ok(val) = self.read_dotenv(path, key) {
                return Some(val);
            }
        }

        // 3. 环境变量
        if let Ok(val) = std::env::var(key) {
            return Some(Zeroizing::new(val));
        }

        None
    }

    /// 解析配置中的密钥引用: ${SECRET:api_key}
    pub fn resolve_config(&self, config: &str) -> String {
        // 正则匹配 ${SECRET:xxx}
        // 替换为解析后的值
    }
}
```

#### 3.1.3 Taint Tracking

```rust
// crates/octo-engine/src/secret/taint.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaintLabel {
    Secret,       // API keys, passwords
    Credential,    // User credentials
    Internal,      // System info
    External,      // Untrusted input
}

#[derive(Debug, Clone)]
pub struct TaintedValue {
    pub value: String,
    pub labels: HashSet<TaintLabel>,
    pub source: String,  // "config", "user_input", "tool_result"
}

#[derive(Debug)]
pub enum TaintSink {
    ShellExec,    // bash/shell tool
    NetFetch,     // HTTP 请求
    FileWrite,    // 文件写入
    AgentMessage, // 对外发送消息
}

impl TaintedValue {
    /// 检查是否应该被阻止访问某个 sink
    pub fn check_sink(&self, sink: TaintSink) -> Result<(), TaintViolation> {
        let blocked = match sink {
            TaintSink::ShellExec => self.labels.contains(&TaintLabel::Secret),
            TaintSink::NetFetch => self.labels.contains(&TaintLabel::Secret),
            TaintSink::FileWrite => self.labels.contains(&TaintLabel::Secret),
            TaintSink::AgentMessage => self.labels.contains(&TaintLabel::Secret),
        };

        if blocked {
            Err(TaintViolation {
                value_preview: self.value.chars().take(10).collect(),
                sink,
                labels: self.labels.clone(),
            })
        } else {
            Ok(())
        }
    }
}
```

### 3.2 Agent Loop 增强

#### 3.2.1 配置增强

```rust
// crates/octo-engine/src/agent/config.rs

#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// 最大轮数 (0 = 无限)
    pub max_rounds: u32,
    /// 启用并行工具执行
    pub enable_parallel: bool,
    /// 并行最大工具数
    pub max_parallel_tools: u8,
    /// 工具执行超时 (秒)
    pub tool_timeout_secs: u64,
    /// 启用 Typing 信号
    pub enable_typing_signal: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_rounds: 50,
            enable_parallel: false,  // 默认关闭，P1 开启
            max_parallel_tools: 8,
            tool_timeout_secs: 60,
            enable_typing_signal: true,
        }
    }
}
```

#### 3.2.2 Extension 钩子系统

```rust
// crates/octo-engine/src/agent/extension.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtensionEvent {
    TurnStart {
        round: u32,
        max_rounds: u32,
    },
    TurnEnd {
        round: u32,
        stop_reason: String,
    },
    ToolCallStart {
        tool_name: String,
        input: serde_json::Value,
    },
    ToolCallEnd {
        tool_name: String,
        success: bool,
        duration_ms: u64,
    },
    Error {
        error: String,
        round: u32,
    },
}

#[async_trait]
pub trait AgentExtension: Send + Sync {
    /// 扩展名
    fn name(&self) -> &str;

    /// 事件处理
    async fn on_event(&self, event: ExtensionEvent) -> Result<(), ExtensionError>;
}

// 注册表
pub struct ExtensionRegistry {
    extensions: Vec<Arc<dyn AgentExtension>>,
}

impl ExtensionRegistry {
    pub fn register(&mut self, ext: Arc<dyn AgentExtension>) {
        self.extensions.push(ext);
    }

    pub async fn emit(&self, event: ExtensionEvent) {
        for ext in &self.extensions {
            if let Err(e) = ext.on_event(event.clone()).await {
                tracing::warn!("Extension {} error: {}", ext.name(), e);
            }
        }
    }
}
```

#### 3.2.3 CancellationToken (AbortSignal)

```rust
// crates/octo-engine/src/agent/cancellation.rs

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;

pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    /// 可选: 当取消时发送通知
    notifier: Option<watch::Sender<()>>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notifier: None,
        }
    }

    pub fn with_notifier() -> (Self, watch::Receiver<()>) {
        let (tx, rx) = watch::channel(());
        let token = Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notifier: Some(tx),
        };
        (token, rx)
    }

    /// 检查是否已取消
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// 取消执行
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(ref tx) = self.notifier {
            let _ = tx.send(());
        }
    }

    /// 创建派生 token (用于子任务)
    pub fn child(&self) -> ChildCancellationToken {
        ChildCancellationToken {
            parent: self.cancelled.clone(),
        }
    }
}

pub struct ChildCancellationToken {
    parent: Arc<AtomicBool>,
}

impl ChildCancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.parent.load(Ordering::Acquire)
    }
}

/// 工具执行的取消支持
pub trait Cancellable {
    fn with_cancellation(self, token: &CancellationToken) -> CancellableTask;
}

pub struct CancellableTask {
    task: tokio::task::JoinHandle<anyhow::Result<serde_json::Value>>,
    cancel: CancellationToken,
}

impl CancellableTask {
    pub async fn run(self) -> anyhow::Result<serde_json::Value> {
        tokio::select! {
            result = self.task => result?,
            _ = tokio::time::sleep(std::time::Duration::MAX) => {
                // 永远等待，除非被取消
                unreachable!()
            }
        }
    }
}
```

#### 3.2.4 Typing 信号

```rust
// crates/octo-engine/src/agent/typing.rs

use crate::agent::AgentEvent;

pub fn send_typing_event(tx: &broadcast::Sender<AgentEvent>) {
    let _ = tx.send(AgentEvent::Typing {
        state: true,
    });
}
```

#### 3.2.5 并行执行引擎

```rust
// crates/octo-engine/src/agent/parallel.rs

use crate::agent::cancellation::CancellationToken;
use crate::tools::{Tool, ToolContext, ToolResult};

pub async fn execute_parallel<T: Tool + Send + Sync>(
    tools: Vec<(String, serde_json::Value)>,  // (name, input)
    registry: &Arc<ToolRegistry<T>>,
    max_parallel: u8,
    cancellation: &CancellationToken,
) -> Vec<(String, ToolResult)> {
    // 1. 限制并发数
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel as usize));

    // 2. 为每个工具创建带取消的任务
    let tasks: Vec<_> = tools
        .into_iter()
        .map(|(name, input)| {
            let registry = registry.clone();
            let sem = semaphore.clone();
            let cancel = cancellation.child();

            async move {
                let _permit = sem.acquire().await.unwrap();

                if cancel.is_cancelled() {
                    return (
                        name,
                        ToolResult::error("Cancelled by parent"),
                    );
                }

                let result = registry
                    .execute(&name, input, context)
                    .await;

                (name, result)
            }
        })
        .collect();

    // 3. 并行执行 (join_all)
    let results = futures_util::future::join_all(tasks).await;

    // 4. 聚合结果
    // 注意: 如果任何一个失败，可以选择取消其他
    results
}
```

---

## 四、数据流

### 4.1 Secret 解析流

```
用户请求 (包含 ${SECRET:xxx})
        ↓
CredentialResolver.resolve()
        ↓
    ┌──────────────────────────────────────┐
    │  1. Vault.get(xxx)                   │
    │     ↓                                 │
    │  2. Dotenv read                      │
    │     ↓                                 │
    │  3. Env var                          │
    │     ↓                                 │
    │  4. Error (missing)                  │
    └──────────────────────────────────────┘
        ↓
Taint Tracking: 标记为 TaintedValue
        ↓
传递给 Tool (input)
        ↓
Tool 执行前检查: check_sink(ShellExec)
        ↓
    ┌──────────────────────────────────────┐
    │  if TaintLabel::Secret → Block       │
    │  else → Allow                        │
    └──────────────────────────────────────┘
```

### 4.2 Agent 增强执行流

```
用户消息
    ↓
AgentLoop::run()
    ↓
for round in 1..=max_rounds:
    ↓
Extension::on_turn_start()
    ↓
TypingSignal::send() (如果启用)
    ↓
LLM API Call
    ↓
处理 Tool Calls:
    ├─ 顺序模式: for tool in tools { ... }
    └─ 并行模式: execute_parallel(tools, max=8)
         ↓
    for each tool:
         ↓
    Taint Tracking 检查
         ↓
    Extension::on_tool_call_start()
         ↓
    工具执行 (带 CancellationToken)
         ↓
    Extension::on_tool_call_end()
    ↓
if stop_reason == "end_turn" → break
    ↓
Extension::on_turn_end()
    ↓
返回结果
```

---

## 五、依赖

```toml
# Cargo.toml (octo-engine)

[dependencies]
# Secret Manager
aes-gcm = "0.10"
argon2 = "0.5"
zeroize = { version = "1.7", features = ["derive"] }
base64 = "0.22"
rand = "0.8"

# Cross-platform keyring (可选 P2)
keyring = { version = "3", optional = true }

# Async
async-trait = "0.1"
futures-util = "0.3"

[features]
default = []
keyring = ["dep:keyring"]
oauth = []
```

---

## 六、实施计划

### Task 1: Secret Manager 基础设施 (P0)

| 子任务 | 描述 | 预估 |
|--------|------|------|
| 1.1 | vault.rs: CredentialVault 结构 + AES-256-GCM | 100 LOC |
| 1.2 | vault.rs: Argon2id 密钥派生 | 50 LOC |
| 1.3 | resolver.rs: CredentialResolver 优先级链 | 80 LOC |
| 1.4 | taint.rs: Taint Tracking 基础 | 80 LOC |
| 1.5 | 集成到 Config: 支持 ${SECRET:xxx} | 30 LOC |
| 1.6 | 单元测试 | 50 LOC |

### Task 2: Agent Loop 配置增强 (P0)

| 子任务 | 描述 | 预估 |
|--------|------|------|
| 2.1 | AgentConfig: max_rounds, enable_parallel | 30 LOC |
| 2.2 | loop_.rs: 读取配置，支持 50轮/无限 | 20 LOC |
| 2.3 | TypingSignal 事件 + 发送逻辑 | 30 LOC |

### Task 3: Extension 钩子系统 (P0)

| 子任务 | 描述 | 预估 |
|--------|------|------|
| 3.1 | extension.rs: Extension trait + Event | 60 LOC |
| 3.2 | extension.rs: ExtensionRegistry | 40 LOC |
| 3.3 | loop_.rs: 钩子调用点 | 50 LOC |
| 3.4 | 示例 Extension: LoggingExtension | 20 LOC |

### Task 4: CancellationToken (P0)

| 子任务 | 描述 | 预估 |
|--------|------|------|
| 4.1 | cancellation.rs: CancellationToken | 50 LOC |
| 4.2 | cancellation.rs: ChildCancellationToken | 30 LOC |
| 4.3 | ToolRegistry: 集成取消支持 | 30 LOC |

### Task 5: 并行执行引擎 (P1)

| 子任务 | 描述 | 预估 |
|--------|------|------|
| 5.1 | parallel.rs: execute_parallel 函数 | 60 LOC |
| 5.2 | loop_.rs: 并行模式分支 | 40 LOC |
| 5.3 | 错误处理: 部分失败取消 | 30 LOC |
| 5.4 | 集成测试 | 40 LOC |

### Task 6: OAuth2 PKCE (P2)

| 子任务 | 描述 | 预估 |
|--------|------|------|
| 6.1 | oauth.rs: PKCE 流程 | 80 LOC |
| 6.2 | OAuth Extension | 40 LOC |
| 6.3 | 支持 Provider: Google, GitHub | 40 LOC |

### Task 7: 构建验证

| 子任务 | 描述 | 预估 |
|--------|------|------|
| 7.1 | cargo check --workspace | - |
| 7.2 | cargo test -p octo-engine | - |
| 7.3 | 文档更新 | - |

---

## 七、验收标准

| 模块 | 验收条件 |
|------|----------|
| **Secret Manager** | |
| 加密存储 | vault.set() 后文件为密文，vault.get() 可解密 |
| 密钥派生 | 相同密码产生相同 key，不同密码无法解密 |
| 优先级链 | resolve() 正确按 vault → dotenv → env 优先级 |
| Taint Tracking | secret 传递到 shell_exec 触发阻止 |
| **Agent Loop** | |
| 50轮 | 配置 max_rounds=50，执行50轮后停止 |
| 无限轮 | 配置 max_rounds=0，无限执行直到 stop_reason |
| Typing | enable_typing_signal=true 时发送事件 |
| Extension | on_turn_start/end, on_tool_call 钩子被调用 |
| Cancellation | parent.cancel() 后子任务收到取消信号 |
| **并行执行** | |
| 并行数 | enable_parallel=true 时最多8工具并行 |
| 结果聚合 | 所有结果正确聚合返回 |
| 错误传播 | 任何一个失败不影响其他（可选取消） |

---

## 八、风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|----------|
| Argon2 性能 | 中 | 仅启动时执行，可缓存派生 key |
| Keyring 依赖 | 中 | 提供环境变量 fallback |
| 并行竞态 | 中 | 使用 Semaphore 控制并发 |
| Taint 性能 | 低 | 仅检查包含 ${SECRET: 的输入 |

---

## 九、决策记录

| 编号 | 决策 | 内容 | 日期 |
|------|------|------|------|
| D-01 | 加密方案 | AES-256-GCM + Argon2id 对齐 OpenFang | 2026-03-02 |
| D-02 | Master Key | macOS Keychain 首选，环境变量 fallback | 2026-03-02 |
| D-03 | 并行默认关闭 | enable_parallel=false，默认顺序执行 | 2026-03-02 |
| D-04 | 无限轮配置 | max_rounds=0 表示无限 | 2026-03-02 |
