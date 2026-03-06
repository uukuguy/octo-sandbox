# ADR：安全加固重构架构决策记录

**项目**：octo-sandbox
**版本**：v1.0
**日期**：2026-03-06
**状态**：全部已接受

---

## 目录

- [ADR-001：PathValidator 安全策略注入](#adr-001pathvalidator-安全策略注入)
- [ADR-002：BashTool ExecPolicy 默认启用](#adr-002bashtool-execpolicy-默认启用)
- [ADR-003：API Key 哈希算法升级为 HMAC-SHA256](#adr-003api-key-哈希算法升级为-hmac-sha256)
- [ADR-004：中间件执行顺序修复（LIFO 语义）](#adr-004中间件执行顺序修复lifo-语义)
- [ADR-005：AgentRuntime 模块化拆分](#adr-005agentruntime-模块化拆分)

---

## ADR-001：PathValidator 安全策略注入

### 状态

**已接受** — 2026-03-06

### 上下文

`ToolContext` 结构体中包含 `path_validator: Option<Arc<dyn PathValidator>>` 字段，
`SecurityPolicy` 实现了 `PathValidator` trait，提供工作区边界检查和禁止路径拦截能力。
然而在重构之前，`AgentRuntime` 没有持有 `SecurityPolicy` 实例，
也没有在创建 `ToolContext` 时将其注入，导致 `path_validator` 始终为 `None`。

结果是：

- 文件工具（`file_read`、`file_write`、`file_edit`）对路径无任何安全限制
- agent 可以读写任意文件系统路径，包括 `~/.ssh`、`/etc/passwd` 等敏感目录
- `SecurityPolicy` 中定义的工作区限制和禁止路径列表形同虚设

相关代码路径：

- `crates/octo-engine/src/agent/executor.rs`：`ToolContext` 创建点，`path_validator` 硬编码为 `None`
- `crates/octo-engine/src/agent/runtime.rs`：`AgentRuntime` 结构体，原先无 `security_policy` 字段
- `crates/octo-engine/src/security/policy.rs`：`SecurityPolicy` 实现了 `PathValidator` 但从未被注入

### 决策

在 `AgentRuntime::new()` 中创建 `SecurityPolicy` 实例，绑定当前工作目录作为工作区，
并在所有 `ToolContext` 创建点将其注入为 `path_validator`。

具体变更：

1. **`runtime.rs`**：新增 `security_policy: Arc<SecurityPolicy>` 字段，在构造函数步骤 15 中初始化：

   ```rust
   let security_policy = Arc::new(
       SecurityPolicy::new().with_workspace(working_dir.clone()),
   );
   ```

2. **`executor.rs`**：在 `AgentExecutor::new()` 中接收 `path_validator: Option<Arc<dyn PathValidator>>`
   参数，在 `ToolContext` 构建时注入：

   ```rust
   let tool_ctx = ToolContext {
       sandbox_id: self.sandbox_id.clone(),
       working_dir: self.working_dir.clone(),
       path_validator: self.path_validator.clone(),
   };
   ```

3. **`runtime.rs`**（`start_primary`）：将 `security_policy` 向下传递给 `AgentExecutor::new()`：

   ```rust
   Some(self.security_policy.clone() as Arc<dyn octo_types::PathValidator>)
   ```

4. **`runtime_scheduler.rs`**（`execute_scheduled_task`）：调度任务同样注入 `path_validator`：

   ```rust
   let tool_ctx = ToolContext {
       sandbox_id: sandbox_id.clone(),
       working_dir: self.working_dir.clone(),
       path_validator: Some(self.security_policy.clone() as Arc<dyn octo_types::PathValidator>),
   };
   ```

### 后果

#### 正面

- 文件工具现在强制限定在工作区目录内，防止路径遍历攻击
- `SecurityPolicy::forbidden_paths` 中声明的系统目录（`/etc`、`/root`、`~/.ssh` 等）得到实际拦截
- 安全策略与工作区绑定，多租户场景下各租户隔离边界清晰
- 调度任务与交互式会话使用相同的路径验证逻辑，安全行为一致

#### 负面

- 对现有使用绝对路径的调用方有破坏性影响；工作区外的路径将被拒绝
- 若 `working_dir` 配置错误（如 `/tmp/octo-sandbox`），合法操作可能被误拦截
- `workspace_only: true` 为默认值，测试环境需要额外配置或创建对应目录

#### 中立

- 公开暴露 `security_policy()` getter 方法，允许 API 层在需要时读取当前安全配置
- `PathValidator` trait 以 `dyn Trait` 形式注入，保持接口解耦，便于测试替换

### 涉及文件

| 文件 | 变更类型 |
|------|--------|
| `crates/octo-engine/src/agent/runtime.rs` | 新增 `security_policy` 字段和初始化逻辑 |
| `crates/octo-engine/src/agent/executor.rs` | 接收并存储 `path_validator` 参数 |
| `crates/octo-engine/src/agent/runtime_scheduler.rs` | 注入 `path_validator` 到调度任务 `ToolContext` |
| `crates/octo-engine/src/security/policy.rs` | 实现 `PathValidator` trait |

---

## ADR-002：BashTool ExecPolicy 默认启用

### 状态

**已接受** — 2026-03-06

### 上下文

`BashTool` 包含一个 `exec_policy: Option<ExecPolicy>` 字段，`ExecPolicy` 定义了三种安全模式：

- `Deny`：禁止所有 shell 执行
- `Allowlist`（默认）：仅允许白名单命令，同时阻断 shell 元字符
- `Full`：允许所有命令（开发模式）

问题在于 `BashTool::new()` 将 `exec_policy` 设为 `Some(ExecPolicy::default())`，
而调用方通过 `default_tools()` 统一创建工具列表时，`exec_policy`
实际已设置（经本次修复前曾被设为 `None`）。

此外，`ExecPolicy::is_allowed()` 在元字符检测上存在不完整性——原始版本仅检测
`;`、`|`、`&&`、`||`、`$(`、`` ` ``，未覆盖以下高风险字符：

- `>`：输出重定向，可覆盖任意文件
- `<`：输入重定向，可读取任意文件内容
- `\n`（换行符）：多命令注入，可绕过单行白名单检查
- `\0`（空字节）：命令截断，可绕过字符串比较

上述元字符均可被恶意构造的 LLM 输出利用，实现白名单绕过。

### 决策

**决策 1**：`BashTool::new()` 将 `exec_policy` 初始化为 `Some(ExecPolicy::default())`，
确保所有通过 `default_tools()` 创建的 `BashTool` 实例默认启用 Allowlist 模式：

```rust
pub fn new() -> Self {
    Self {
        exec_policy: Some(ExecPolicy::default()),
        // ...
    }
}
```

**决策 2**：在 `ExecPolicy::is_allowed()` 的 Allowlist 分支中，新增对 `>`、`<`、`\n`、`\0` 的检测：

```rust
ExecSecurityMode::Allowlist => {
    if command.contains(';')
        || command.contains('|')
        || command.contains("&&")
        || command.contains("||")
        || command.contains("$(")
        || command.contains('`')
        || command.contains('>')   // 新增：输出重定向
        || command.contains('<')   // 新增：输入重定向
        || command.contains('\n')  // 新增：换行符注入
        || command.contains('\0')  // 新增：空字节截断
    {
        return false;
    }
    // ...
}
```

### 后果

#### 正面

- 所有 agent 执行的 bash 命令默认受 Allowlist 控制，攻击面大幅收窄
- 重定向类攻击（覆写 `/etc/passwd` 等）被元字符检测提前阻断
- 换行符注入（`cmd1\ncmd2`）无法绕过单命令白名单
- 空字节截断攻击被阻止

#### 负面

- 合法的输出重定向命令（如 `echo hello > /tmp/test`）将被拒绝；
  需要写文件的场景应改用 `file_write` 工具
- 管道命令（`ls | grep foo`）被禁止，部分调试场景受限
- 若需要 `>` 或 `<` 操作，必须显式使用 `ExecSecurityMode::Full` 或扩展白名单

#### 中立

- `BashTool::with_policy(policy)` 方法保留，供需要自定义策略的场景使用
- `ExecSecurityMode::Full` 模式仍然存在，开发者可在受信任环境中显式启用

### 涉及文件

| 文件 | 变更类型 |
|------|--------|
| `crates/octo-engine/src/tools/bash.rs` | `BashTool::new()` 默认设置 `exec_policy`；`is_allowed()` 增加元字符检测 |

---

## ADR-003：API Key 哈希算法升级为 HMAC-SHA256

### 状态

**已接受** — 2026-03-06

### 上下文

原有 API Key 存储方案使用无盐 SHA-256 计算哈希值。此方案存在以下安全缺陷：

1. **彩虹表攻击**：无盐哈希允许攻击者预计算常见 API Key 的哈希映射表，
   一旦数据库泄露，可批量反查原始 Key
2. **相同 Key 产生相同哈希**：不同系统中相同 API Key 的哈希完全一致，
   跨系统泄露风险叠加
3. **无密钥绑定**：哈希值不与任何系统机密绑定，
   攻击者仅需数据库访问权即可离线暴力破解

HMAC-SHA256 通过引入服务端密钥（HMAC Secret）解决上述问题：
即使攻击者获取了数据库中的哈希值，也无法在不知道 HMAC Secret 的情况下反推原始 Key。

### 决策

将 API Key 哈希算法从无盐 SHA-256 升级为 HMAC-SHA256：

**实现变更**（`crates/octo-engine/src/auth/config.rs`）：

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn hash_api_key(key: &str, secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(key.as_bytes());
    format!("{:x}", mac.finalize().into_bytes())
}
```

**配置变更**：

- `AuthConfig` 新增 `hmac_secret: String` 字段
- 从 `OCTO_HMAC_SECRET` 环境变量加载；未设置时回退到硬编码默认值并输出警告日志：

  ```rust
  let hmac_secret = std::env::var("OCTO_HMAC_SECRET").unwrap_or_else(|_| {
      tracing::warn!(
          "OCTO_HMAC_SECRET is not set. Using insecure default HMAC secret. \
           Set this environment variable in production."
      );
      DEFAULT_HMAC_SECRET.to_string()
  });
  ```

- `ApiKey::new()` 签名变更为 `new(key: &str, secret: &str, ...)` 以接收 HMAC Secret
- `add_api_key()` 和 `add_api_key_with_role()` 自动使用 `self.hmac_secret`

### 后果

#### 正面

- 彩虹表攻击无效：哈希值与服务端密钥绑定，离线破解须同时获取密钥
- 数据库泄露不等于 API Key 泄露，系统具备防御纵深
- 生产环境通过 `OCTO_HMAC_SECRET` 环境变量注入密钥，支持密钥轮换

#### 负面

- **破坏性变更**：所有使用旧 SHA-256 哈希存储的 API Key 立即失效，需重新生成
- 升级前必须：(1) 停止服务 (2) 清空旧 Key 记录 (3) 使用新算法重新添加 Key
- 不同 `OCTO_HMAC_SECRET` 值产生不同哈希，密钥丢失后所有 API Key 失效

#### 中立

- 开发环境未设置 `OCTO_HMAC_SECRET` 时仍可正常运行，但会在启动日志中输出安全警告
- 默认 HMAC Secret `"octo-default-hmac-secret-change-in-production"` 已在代码中明确标注禁止用于生产

### 涉及文件

| 文件 | 变更类型 |
|------|--------|
| `crates/octo-engine/src/auth/config.rs` | 引入 `hmac` crate，`hash_api_key()` 升级，`AuthConfig` 新增 `hmac_secret` 字段 |

### 迁移指南

1. 在生产环境配置文件或 `.env` 中设置 `OCTO_HMAC_SECRET=<随机强密码>`
2. 删除数据库中所有旧 API Key 记录（旧哈希不兼容新算法）
3. 使用 `AuthConfig::add_api_key()` 重新注册所有 Key（新版本自动使用 HMAC-SHA256）
4. 重启服务，验证认证正常工作

---

## ADR-004：中间件执行顺序修复（LIFO 语义）

### 状态

**已接受** — 2026-03-06

### 上下文

Axum 的 `.layer()` 调用遵循 LIFO（后进先出）语义：**最后添加的中间件最先执行**。

重构前的中间件添加顺序为：

```rust
.layer(audit_middleware)    // 第一个添加 → 最后执行
.layer(auth_middleware)     // 第二个添加 → 中间执行
.layer(rate_limit)          // 第三个添加 → 最先执行
```

这本应产生正确的执行顺序 `rate_limit → auth → audit`，
然而代码注释描述与实际语义存在歧义，导致维护人员误解顺序，
存在因代码重排而破坏安全性的风险。

更深层的问题是：若 `audit` 中间件在 `auth` 之前执行，
`UserContext` 扩展尚未注入，audit 日志将缺失用户身份信息，审计记录不完整。

### 决策

显式注释中间件的 LIFO 添加规则，并确认正确的添加顺序：

```rust
// Middleware layers use LIFO ordering: last added = first to run.
// Desired execution order: rate_limit → auth → audit
// So we add them in reverse: audit first, rate_limit last.
//
// Audit middleware - logs all requests (runs AFTER auth, so UserContext is available)
.layer(axum::middleware::from_fn_with_state(audit_state, audit_middleware))
// Auth middleware - validates API keys and injects UserContext
.layer(axum::middleware::from_fn_with_state(auth_state, auth_middleware_wrapper))
// Rate limiting middleware (runs FIRST - before auth and audit)
.layer(axum::middleware::from_fn_with_state(rate_limiter, rate_limit_middleware))
```

最终执行顺序（入站请求方向）：

```
请求 → rate_limit → auth → audit → 业务处理器
响应 ← rate_limit ← auth ← audit ← 业务处理器
```

此顺序确保：

1. `rate_limit`：最先检查速率，拒绝超限请求，减少 `auth` 的无效验证开销
2. `auth`：验证 API Key 并将 `UserContext` 注入 request extensions
3. `audit`：记录请求时已能读取到 `UserContext`，audit 日志包含用户身份

### 后果

#### 正面

- audit 日志现在能正确记录认证用户身份，满足审计合规要求
- rate_limit 在 auth 之前执行，未认证请求也受速率限制，防止 DoS 探测
- 明确的 LIFO 注释降低未来维护时引入错误的风险

#### 负面

- 若未来引入需要在 auth 之前运行的中间件（如 CORS 预检），需要特别注意 LIFO 规则
- Axum 的 LIFO 语义与 Express/Django 等框架的 FIFO 习惯相反，对新贡献者有认知成本

#### 中立

- `TraceLayer` 和 `CorsLayer` 通过独立的 `.layer()` 调用添加，位于所有自定义中间件外层，
  其执行顺序不受本次变更影响

### 涉及文件

| 文件 | 变更类型 |
|------|--------|
| `crates/octo-server/src/router.rs` | 确认并注释中间件 LIFO 添加顺序，修复 audit 在 auth 后执行 |

---

## ADR-005：AgentRuntime 模块化拆分

### 状态

**已接受** — 2026-03-06

### 上下文

`AgentRuntime` 是整个系统的核心组件，负责：

- Agent 生命周期管理（start/stop/pause/resume）
- MCP Server 管理（add/remove/list/call）
- 调度任务执行（execute_scheduled_task）
- 对外提供各类 getter 方法

重构前，上述所有职责的 `impl` 方法均堆砌在 `runtime.rs` 同一个文件中。
随着功能增加，该文件已超过 500 行（项目规范上限），违反单一职责原则，
形成典型的 **God Object 反模式**：

- 文件过长，导航和定位困难
- 不同职责的代码混合，修改 MCP 功能时需要在 Agent 生命周期代码中定位
- 合并冲突概率高：多人并行开发时极易在同一文件产生冲突
- 测试难以隔离：难以单独测试某一职责而不加载其他实现

### 决策

将 `runtime.rs` 中的 `impl AgentRuntime` 块按职责拆分到三个独立子模块中：

| 子模块文件 | 职责 | 包含方法 |
|-----------|------|---------|
| `runtime.rs` | 核心结构体定义、构造函数、getter 方法 | `new()`、`with_*()`、各 getter |
| `runtime_lifecycle.rs` | Agent 生命周期管理 | `start()`、`stop()`、`pause()`、`resume()` |
| `runtime_mcp.rs` | MCP Server 管理 | `add_mcp_server()`、`remove_mcp_server()`、`list_mcp_servers()`、`call_mcp_tool()` 等 |
| `runtime_scheduler.rs` | 调度任务执行 | `execute_scheduled_task()` |

**模块声明方式**（`mod.rs`）：

```rust
mod runtime;
mod runtime_lifecycle;
mod runtime_mcp;
mod runtime_scheduler;
```

**impl 分散原则**：各子模块使用 `impl AgentRuntime { ... }` 形式，
Rust 允许同一类型的 `impl` 块分散在不同文件中，结构体定义仍在 `runtime.rs`。

**字段访问原则**：子模块中的方法通过 `pub(crate)` 可见性访问 `AgentRuntime` 字段。
所有字段在 `runtime.rs` 中声明为 `pub(crate)`，限制在 crate 内可见。

示例（`runtime_lifecycle.rs`）：

```rust
use super::runtime::AgentRuntime;

impl AgentRuntime {
    pub async fn start(&self, ...) -> Result<AgentExecutorHandle, AgentError> {
        // 直接访问 self.catalog、self.primary_handle 等 pub(crate) 字段
    }
}
```

### 后果

#### 正面

- 每个文件专注单一职责，代码导航效率提升
- `runtime.rs` 行数从 500+ 行降至约 250 行
- 各职责可独立演进：MCP 管理逻辑变更不影响生命周期代码的 git 历史
- 并行开发时合并冲突减少
- 可以针对 `runtime_scheduler.rs` 单独编写测试而不依赖 MCP 代码

#### 负面

- 需要在各子模块文件头部导入 `use super::runtime::AgentRuntime`，增加少量样板代码
- `pub(crate)` 字段暴露增加了 crate 内部可见性范围，需要依赖代码审查维护封装性
- 读者初次阅读代码时需要在多个文件间跳转才能获得完整视图

#### 中立

- Rust 的 `impl` 分散机制是语言原生特性，不引入新的架构抽象
- `AgentRuntime` 的公开 API（方法签名）无任何变化，对外调用方完全透明
- 子模块文件均以 `//!` 文档注释声明模块职责，便于快速定位

### 涉及文件

| 文件 | 变更类型 |
|------|--------|
| `crates/octo-engine/src/agent/runtime.rs` | 保留结构体定义、构造函数、getter；移出生命周期/MCP/调度方法 |
| `crates/octo-engine/src/agent/runtime_lifecycle.rs` | 新增：Agent 生命周期管理 impl 块 |
| `crates/octo-engine/src/agent/runtime_mcp.rs` | 新增：MCP Server 管理 impl 块 |
| `crates/octo-engine/src/agent/runtime_scheduler.rs` | 新增：调度任务执行 impl 块 |
| `crates/octo-engine/src/agent/mod.rs` | 新增子模块声明 |

---

## 变更总览

| ADR | 类别 | 安全影响 | 破坏性变更 |
|-----|------|---------|-----------|
| ADR-001 | 安全 / 工具隔离 | 高 — 启用路径验证 | 是（工作区外路径被拒绝） |
| ADR-002 | 安全 / 命令执行 | 高 — 启用命令白名单 | 是（`>` / `<` / `\n` 等被阻断） |
| ADR-003 | 安全 / 认证 | 高 — 防彩虹表攻击 | 是（旧 API Key 哈希失效） |
| ADR-004 | 架构 / 中间件 | 中 — audit 日志完整性 | 否 |
| ADR-005 | 架构 / 可维护性 | 无 | 否 |
