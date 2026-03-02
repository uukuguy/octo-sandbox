# Phase 2.8 Agent 增强 + Secret Manager 详细实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现企业级 Secret Manager (AES-256-GCM + Argon2id + Taint Tracking) 和 Agent Loop 增强 (50轮/无限 + Extension钩子 + AbortSignal + 并行执行)

**Architecture:**
- Secret Manager 模块独立于 agent，使用加密存储 + 凭证解析链
- Agent Loop 增强采用配置化设计，可渐进开启并行执行
- Extension 钩子基于 async_trait 实现事件驱动

**Tech Stack:** aes-gcm, argon2, zeroize, async-trait, futures-util

---

## Task 1: 依赖添加与模块创建

### Task 1.1: 添加 Secret Manager 依赖

**Files:**
- Modify: `crates/octo-engine/Cargo.toml`

**Step 1: 添加依赖**

```toml
# crates/octo-engine/Cargo.toml 添加:

[dependencies]
# Secret Manager
aes-gcm = "0.10"
argon2 = "0.5"
zeroize = { version = "1.7", features = ["derive"] }
base64 = "0.22"
rand = "0.8"
async-trait = "0.1"
futures-util = "0.3"

# Optional
keyring = { version = "3", optional = true }

[features]
default = []
keyring = ["dep:keyring"]
```

**Step 2: 验证依赖**

Run: `cargo check -p octo-engine`
Expected: Download and compile new dependencies

---

## Task 2: Secret Manager - Vault 核心

### Task 2.1: 创建 secret 模块结构

**Files:**
- Create: `crates/octo-engine/src/secret/mod.rs`

**Step 1: 创建模块文件**

```rust
// crates/octo-engine/src/secret/mod.rs

mod vault;
mod resolver;
mod taint;

pub use vault::{CredentialVault, EncryptedStore};
pub use resolver::CredentialResolver;
pub use taint::{TaintLabel, TaintedValue, TaintSink, TaintViolation};
```

**Step 2: 更新 lib.rs**

```rust
// crates/octo-engine/src/lib.rs 添加:
pub mod secret;
```

**Step 3: 验证编译**

Run: `cargo check -p octo-engine`
Expected: PASS (empty module compiles)

---

### Task 2.2: 实现 EncryptedStore 结构

**Files:**
- Create: `crates/octo-engine/src/secret/vault.rs`

**Step 1: 写入测试**

```rust
// crates/octo-engine/src/secret/vault_test.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypted_store_new() {
        let store = EncryptedStore::new();
        assert_eq!(store.version, 1);
        assert_eq!(store.salt.len(), 16);
        assert_eq!(store.nonce.len(), 12);
    }
}
```

**Step 2: 实现结构**

```rust
// crates/octo-engine/src/secret/vault.rs

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedStore {
    pub version: u8,           // = 1
    pub salt: [u8; 16],      // 随机 salt
    pub nonce: [u8; 12],     // AES-GCM nonce
    pub ciphertext: Vec<u8>,  // 加密数据
}

impl EncryptedStore {
    pub fn new() -> Self {
        use rand::RngCore;
        let mut salt = [0u8; 16];
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce);

        Self {
            version: 1,
            salt,
            nonce,
            ciphertext: Vec::new(),
        }
    }
}
```

**Step 3: 验证测试**

Run: `cargo test -p octo-engine secret::vault_test`
Expected: PASS

---

### Task 2.3: 实现 CredentialVault 加密/解密

**Step 1: 写入测试**

```rust
// crates/octo-engine/src/secret/vault_test.rs 添加:

#[test]
fn test_vault_encrypt_decrypt() {
    use zeroize::Zeroizing;

    let vault = CredentialVault::new("test_password".to_string());
    vault.set("api_key", "sk-12345").unwrap();

    let retrieved = vault.get("api_key");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().as_str(), "sk-12345");
}

#[test]
fn test_vault_wrong_password() {
    let vault = CredentialVault::new("correct_password".to_string());
    vault.set("key", "value").unwrap();

    // 用不同密码创建新 vault 实例
    let vault2 = CredentialVault::new("wrong_password".to_string());
    let result = vault2.get("key");
    assert!(result.is_none()); // 无法解密
}
```

**Step 2: 实现加密功能**

```rust
// crates/octo-engine/src/secret/vault.rs 添加:

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use std::collections::HashMap;
use std::sync::RwLock;
use zeroize::Zeroizing;

pub struct CredentialVault {
    store: RwLock<EncryptedStore>,
    master_key: Zeroizing<[u8; 32]>,
    entries: RwLock<HashMap<String, String>>,
}

impl CredentialVault {
    pub fn new(password: String) -> Self {
        // 生成随机 salt
        let salt = SaltString::generate(&mut rand::thread_rng());

        // Argon2id 派生密钥
        let argon2 = Argon2::default();
        let hash = argon2.hash_password(password.as_bytes(), &salt).unwrap();

        // 提取 32 bytes key
        let mut key = [0u8; 32];
        let hash_bytes = hash.hash.unwrap();
        key.copy_from_slice(&hash_bytes.as_bytes()[..32]);

        Self {
            store: RwLock::new(EncryptedStore::new()),
            master_key: Zeroizing::new(key),
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let mut entries = self.entries.write().map_err(|e| e.to_string())?;
        entries.insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<Zeroizing<String>> {
        let entries = self.entries.read().ok()?;
        entries.get(key).map(|v| Zeroizing::new(v.clone()))
    }
}
```

**Step 3: 验证测试**

Run: `cargo test -p octo-engine secret::vault_test`
Expected: All tests PASS

---

### Task 2.4: 实现完整加密流程 (AES-256-GCM)

**Step 1: 写入失败测试**

```rust
// 测试加密后文件不可读

#[test]
fn test_vault_persistence_format() {
    let vault = CredentialVault::new("password123".to_string());
    vault.set("api_key", "secret_value").unwrap();

    let store = vault.store.read().unwrap();

    // ciphertext 不应该是明文
    let ciphertext_str = String::from_utf8_lossy(&store.ciphertext);
    assert!(!ciphertext_str.contains("secret_value"));
}
```

**Step 2: 实现加密存储**

```rust
// 在 vault.rs 中添加加密方法

impl CredentialVault {
    /// 加密并序列化 entries 到 store.ciphertext
    pub fn encrypt(&self) -> Result<(), String> {
        use aes_gcm::AeadCore;

        let entries = self.entries.read().map_err(|e| e.to_string())?;
        let plaintext = serde_json::to_vec(&*entries).map_err(|e| e.to_string())?;

        let cipher = Aes256Gcm::new_from_slice(&*self.master_key)
            .map_err(|e| e.to_string())?;

        let nonce = Nonce::from_slice(&self.store.read().unwrap().nonce);

        let ciphertext = cipher.encrypt(nonce, plaintext.as_ref())
            .map_err(|e| e.to_string())?;

        self.store.write().map_err(|e| e.to_string())?.ciphertext = ciphertext;
        Ok(())
    }

    /// 解密 store.ciphertext 到 entries
    pub fn decrypt(&self) -> Result<(), String> {
        let store = self.store.read().map_err(|e| e.to_string())?;

        if store.ciphertext.is_empty() {
            return Ok(()); // 空 vault
        }

        let cipher = Aes256Gcm::new_from_slice(&*self.master_key)
            .map_err(|e| e.to_string())?;

        let nonce = Nonce::from_slice(&store.nonce);

        let plaintext = cipher.decrypt(nonce, store.ciphertext.as_ref())
            .map_err(|_| "Decryption failed - wrong password?".to_string())?;

        let entries: HashMap<String, String> = serde_json::from_slice(&plaintext)
            .map_err(|e| e.to_string())?;

        *self.entries.write().map_err(|e| e.to_string())? = entries;
        Ok(())
    }
}
```

**Step 3: 验证测试**

Run: `cargo test -p octo-engine secret::vault`
Expected: All tests PASS

---

## Task 3: Secret Manager - CredentialResolver

### Task 3.1: 实现 CredentialResolver 优先级链

**Files:**
- Create: `crates/octo-engine/src/secret/resolver.rs`

**Step 1: 写入测试**

```rust
// crates/octo-engine/src/secret/resolver_test.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_priority() {
        // 优先级: Vault -> Dotenv -> Env
        let resolver = CredentialResolver::new();

        // 模拟 vault 有值
        // 模拟 dotenv 有值
        // 模拟 env 有值

        // 应该返回 vault 的值
    }

    #[test]
    fn test_resolve_secret_syntax() {
        let resolver = CredentialResolver::new();

        let config = "api_key=${SECRET:openai_key}&other=value";
        let resolved = resolver.resolve_config(config);

        assert!(resolved.contains("${SECRET:") == false);
    }
}
```

**Step 2: 实现结构**

```rust
// crates/octo-engine/src/secret/resolver.rs

use crate::secret::vault::CredentialVault;
use std::path::PathBuf;
use zeroize::Zeroizing;

pub struct CredentialResolver {
    vault: Option<CredentialVault>,
    dotenv_path: Option<PathBuf>,
}

impl CredentialResolver {
    pub fn new() -> Self {
        Self {
            vault: None,
            dotenv_path: None,
        }
    }

    pub fn with_vault(mut self, vault: CredentialVault) -> Self {
        self.vault = Some(vault);
        self
    }

    pub fn with_dotenv(mut self, path: PathBuf) -> Self {
        self.dotenv_path = Some(path);
        self
    }

    /// 解析单个密钥
    pub fn resolve(&self, key: &str) -> Option<Zeroizing<String>> {
        // 1. Vault
        if let Some(ref v) = self.vault {
            if let Some(val) = v.get(key) {
                return Some(val);
            }
        }

        // 2. Dotenv
        if let Some(ref path) = self.dotenv_path {
            if let Ok(val) = self.read_dotenv(path, key) {
                return Some(Zeroizing::new(val));
            }
        }

        // 3. Environment
        if let Ok(val) = std::env::var(key) {
            return Some(Zeroizing::new(val));
        }

        None
    }

    /// 解析配置中的 ${SECRET:xxx}
    pub fn resolve_config(&self, config: &str) -> String {
        let re = regex::Regex::new(r"\$\{SECRET:([^}]+)\}").unwrap();

        re.replace_all(config, |caps: &regex::Captures| {
            let key = &caps[1];
            self.resolve(key)
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("${{SECRET:{}}}", key))
        }).to_string()
    }

    fn read_dotenv(&self, path: &PathBuf, key: &str) -> Result<String, std::env::VarError> {
        // 简化实现: 读取环境变量
        // 完整实现需要解析 .env 文件
        std::env::var(key)
    }
}
```

**Step 3: 验证测试**

Run: `cargo test -p octo-engine secret::resolver`
Expected: PASS

---

### Task 3.2: 实现 Taint Tracking

**Files:**
- Create: `crates/octo-engine/src/secret/taint.rs`

**Step 1: 写入测试**

```rust
// crates/octo-engine/src/secret/taint_test.rs

#[test]
fn test_taint_secret_blocks_shell() {
    let tainted = TaintedValue::new_secret(
        "sk-12345".to_string(),
        "config".to_string(),
    );

    // Secret 应该被阻止传递给 shell_exec
    let result = tainted.check_sink(TaintSink::ShellExec);
    assert!(result.is_err());
}

#[test]
fn test_taint_internal_allows_shell() {
    let tainted = TaintedValue::new_internal(
        "some_info".to_string(),
        "system".to_string(),
    );

    // Internal 应该允许传递给 shell_exec
    let result = tainted.check_sink(TaintSink::ShellExec);
    assert!(result.is_ok());
}
```

**Step 2: 实现结构**

```rust
// crates/octo-engine/src/secret/taint.rs

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaintLabel {
    Secret,
    Credential,
    Internal,
    External,
}

#[derive(Debug, Clone)]
pub struct TaintedValue {
    pub value: String,
    pub labels: HashSet<TaintLabel>,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaintSink {
    ShellExec,
    NetFetch,
    FileWrite,
    AgentMessage,
}

#[derive(Debug)]
pub struct TaintViolation {
    pub value_preview: String,
    pub sink: TaintSink,
    pub labels: HashSet<TaintLabel>,
}

impl TaintedValue {
    pub fn new_secret(value: String, source: String) -> Self {
        let mut labels = HashSet::new();
        labels.insert(TaintLabel::Secret);
        Self { value, labels, source }
    }

    pub fn new_internal(value: String, source: String) -> Self {
        let mut labels = HashSet::new();
        labels.insert(TaintLabel::Internal);
        Self { value, labels, source }
    }

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

**Step 3: 验证测试**

Run: `cargo test -p octo-engine secret::taint`
Expected: All tests PASS

---

## Task 4: Agent Loop 增强 - 配置

### Task 4.1: 创建 AgentConfig

**Files:**
- Create: `crates/octo-engine/src/agent/config.rs`

**Step 1: 实现配置结构**

```rust
// crates/octo-engine/src/agent/config.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            enable_parallel: false,
            max_parallel_tools: 8,
            tool_timeout_secs: 60,
            enable_typing_signal: true,
        }
    }
}
```

**Step 2: 集成到 AgentLoop**

```rust
// crates/octo-engine/src/agent/mod.rs 添加:
pub mod config;
pub use config::AgentConfig;
```

**Step 3: 验证编译**

Run: `cargo check -p octo-engine`
Expected: PASS

---

## Task 5: Agent Loop 增强 - Extension 钩子

### Task 5.1: 实现 ExtensionEvent 和 AgentExtension trait

**Files:**
- Create: `crates/octo-engine/src/agent/extension.rs`

**Step 1: 写入测试**

```rust
// crates/octo-engine/src/agent/extension_test.rs

#[test]
fn test_extension_event_emit() {
    let registry = ExtensionRegistry::new();
    let ext = TestExtension::new();
    registry.register(Arc::new(ext));

    // emit event
    // verify callback was called
}
```

**Step 2: 实现结构**

```rust
// crates/octo-engine/src/agent/extension.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtensionEvent {
    TurnStart { round: u32, max_rounds: u32 },
    TurnEnd { round: u32, stop_reason: String },
    ToolCallStart { tool_name: String, input: serde_json::Value },
    ToolCallEnd { tool_name: String, success: bool, duration_ms: u64 },
    Error { error: String, round: u32 },
}

#[async_trait]
pub trait AgentExtension: Send + Sync {
    fn name(&self) -> &str;
    async fn on_event(&self, event: ExtensionEvent);
}

pub struct ExtensionRegistry {
    extensions: Vec<Arc<dyn AgentExtension>>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self { extensions: Vec::new() }
    }

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

**Step 3: 验证测试**

Run: `cargo test -p octo-engine agent::extension`
Expected: PASS

---

## Task 6: Agent Loop 增强 - CancellationToken

### Task 6.1: 实现 CancellationToken

**Files:**
- Create: `crates/octo-engine/src/agent/cancellation.rs`

**Step 1: 写入测试**

```rust
// crates/octo-engine/src/agent/cancellation_test.rs

#[tokio::test]
async fn test_cancellation_token_cancel() {
    let token = CancellationToken::new();

    assert!(!token.is_cancelled());

    token.cancel();

    assert!(token.is_cancelled());
}

#[tokio::test]
async fn test_child_token_inherits_parent() {
    let parent = CancellationToken::new();
    let child = parent.child();

    parent.cancel();

    assert!(child.is_cancelled());
}
```

**Step 2: 实现结构**

```rust
// crates/octo-engine/src/agent/cancellation.rs

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;

pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
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

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(ref tx) = self.notifier {
            let _ = tx.send(());
        }
    }

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
```

**Step 3: 验证测试**

Run: `cargo test -p octo-engine agent::cancellation`
Expected: All tests PASS

---

## Task 7: Agent Loop 增强 - Typing 信号

### Task 7.1: 添加 Typing 事件

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**Step 1: 添加 Typing 事件到 AgentEvent**

```rust
// crates/octo-engine/src/agent/loop_.rs 添加:

#[derive(Debug, Clone)]
pub enum AgentEvent {
    // ... existing events ...
    Typing {
        state: bool,
    },
}
```

**Step 2: 在 LLM 响应时发送 Typing**

```rust
// 在 AgentLoop::run() 中，LLM 开始响应时:
if config.enable_typing_signal {
    let _ = tx.send(AgentEvent::Typing { state: true });
}
```

**Step 3: 验证编译**

Run: `cargo check -p octo-engine`
Expected: PASS

---

## Task 8: Agent Loop 增强 - 50轮/无限轮

### Task 8.1: 修改循环条件

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**Step 1: 添加配置字段**

```rust
// 在 AgentLoop struct 添加:
config: AgentConfig,
```

**Step 2: 修改循环逻辑**

```rust
// 原代码:
// for _ in 0..MAX_ROUNDS {

// 修改为:
let max_rounds = if self.config.max_rounds == 0 {
    u32::MAX  // 无限轮
} else {
    self.config.max_rounds
};

for round in 1..=max_rounds {
    // ... existing loop body ...
}
```

**Step 3: 验证编译**

Run: `cargo check -p octo-engine`
Expected: PASS

---

## Task 9: Agent Loop 增强 - 并行执行

### Task 9.1: 实现 execute_parallel

**Files:**
- Create: `crates/octo-engine/src/agent/parallel.rs`

**Step 1: 写入测试**

```rust
// crates/octo-engine/src/agent/parallel_test.rs

#[tokio::test]
async fn test_parallel_execution() {
    // 测试并行执行多个工具
}
```

**Step 2: 实现并行执行**

```rust
// crates/octo-engine/src/agent/parallel.rs

use crate::agent::cancellation::CancellationToken;
use crate::tools::{Tool, ToolContext, ToolResult, ToolRegistry};
use std::sync::Arc;

pub async fn execute_parallel<T: Tool + Send + Sync>(
    tools: Vec<(String, serde_json::Value)>,
    registry: &Arc<ToolRegistry<T>>,
    max_parallel: u8,
    cancellation: &CancellationToken,
) -> Vec<(String, ToolResult)> {
    use futures_util::future::join_all;

    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel as usize));

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

                // 执行工具 (简化版)
                let result = registry.execute(&name, input, ToolContext::default()).await;

                (name, result)
            }
        })
        .collect();

    join_all(tasks).await
}
```

**Step 3: 集成到 AgentLoop**

```rust
// 在 loop_.rs 中根据 config.enable_parallel 选择执行模式

if self.config.enable_parallel {
    results = execute_parallel(
        tool_calls,
        &self.tools,
        self.config.max_parallel_tools,
        &cancellation_token,
    ).await;
} else {
    // 现有顺序执行
}
```

**Step 4: 验证编译和测试**

Run: `cargo test -p octo-engine agent::parallel`
Expected: PASS

---

## Task 10: 构建验证

### Task 10.1: cargo check

Run: `cargo check --workspace`
Expected: 0 errors

### Task 10.2: cargo test

Run: `cargo test -p octo-engine`
Expected: All tests pass

### Task 10.3: 前端检查

Run: `cd web && npx tsc --noEmit`
Expected: 0 errors

---

## 快速参考

### 关键文件路径

| 模块 | 文件 |
|------|------|
| Secret Vault | `crates/octo-engine/src/secret/vault.rs` |
| Secret Resolver | `crates/octo-engine/src/secret/resolver.rs` |
| Taint Tracking | `crates/octo-engine/src/secret/taint.rs` |
| Agent Config | `crates/octo-engine/src/agent/config.rs` |
| Agent Extension | `crates/octo-engine/src/agent/extension.rs` |
| Cancellation | `crates/octo-engine/src/agent/cancellation.rs` |
| Parallel | `crates/octo-engine/src/agent/parallel.rs` |
| AgentLoop 修改 | `crates/octo-engine/src/agent/loop_.rs` |

### 常用命令

```bash
# 检查编译
cargo check -p octo-engine

# 运行测试
cargo test -p octo-engine

# 运行特定模块测试
cargo test -p octo-engine secret::vault
cargo test -p octo-engine agent::extension
```
