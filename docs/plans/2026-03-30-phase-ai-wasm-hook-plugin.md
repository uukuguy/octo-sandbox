# Phase AI — WASM Component Model Hook 插件生态 实施方案

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 基于 WebAssembly Component Model 实现 hook 插件系统，允许用户用任意语言编写 hook handler 并编译为 `.wasm` 组件，通过带 manifest 的插件包分发，集成为声明式 hooks.yaml 的第 4 种 action type（`type: wasm`）。

**Architecture:** Wasmtime Component Model + WIT 接口定义 + 丰富 host import（log, secret, http）+ 插件发现/加载 + HookHandler trait 适配

**Tech Stack:** Rust, wasmtime 36+ (component-model feature), wit-bindgen, serde_yaml

---

## 设计背景

### 为什么选择 Component Model（方案 B）

| 方面 | Core Module (方案 A) | Component Model (方案 B) ✅ |
|------|---------------------|---------------------------|
| 类型安全 | 手动 ptr/len 约定 | WIT 自动绑定 |
| 内存管理 | 手动 alloc/dealloc | canonical ABI 自动 |
| 插件开发体验 | unsafe 指针操作 | wit-bindgen 生成高级 API |
| 多语言支持 | 每种语言手写约定 | wit-bindgen 官方支持 Rust/C/Go/Python/JS |
| 接口演进 | ABI 不兼容 | package 版本化 |
| 调试 | 直接但原始 | 更抽象但安全 |

### 仓库内参考实现

| 项目 | 位置 | 用途 | Wasmtime |
|------|------|------|----------|
| **Moltis** | `3th-party/.../moltis/crates/tools/src/wasm_component.rs` | Tool 插件宿主 | 36 |
| **Pi Agent** | `3th-party/.../pi_agent_rust/src/extensions.rs` | Extension 插件宿主 | 41 |
| **Zeroclaw** | `3th-party/.../zeroclaw/wit/zeroclaw/hooks/v1/hooks.wit` | Hook WIT 定义 | — |
| **IronClaw** | `3th-party/.../ironclaw/wit/tool.wit` | 6 个 host 能力 | — |

---

## WIT 接口定义

```wit
// crates/octo-engine/wit/octo-hook.wit
package octo:hook@1.0.0;

/// Host capabilities imported by hook plugins.
interface host {
    /// Emit a log message at the specified level.
    log: func(level: string, message: string);

    /// Get the full HookContext as JSON string.
    get-context: func() -> string;

    /// Retrieve a secret by key. Returns error if not found.
    get-secret: func(key: string) -> result<string, string>;

    /// Make an HTTP request. Returns response body or error.
    /// Method: GET, POST, PUT, DELETE
    /// Timeout: 5 seconds, max response body 1MB
    http-request: func(method: string, url: string, headers-json: string, body: string) -> result<string, string>;

    /// Get current timestamp (milliseconds since epoch).
    now-millis: func() -> u64;
}

/// Hook handler interface exported by plugins.
interface hook-handler {
    /// Human-readable hook name.
    name: func() -> string;

    /// Execution priority (lower = runs first).
    priority: func() -> u32;

    /// Supported hook events (comma-separated: "PreToolUse,PostToolUse").
    supported-events: func() -> string;

    /// Execute the hook. Input: HookContext JSON. Output: HookDecision JSON.
    /// HookDecision: {"decision": "allow|deny|ask", "reason": "...", "updatedInput": {...}}
    execute: func(context-json: string) -> result<string, string>;
}

/// Plugin world combining host imports and handler exports.
world octo-hook-plugin {
    import host;
    export hook-handler;
}
```

---

## 插件 Manifest 格式

```yaml
# ~/.octo/plugins/my-security-hook/manifest.yaml
name: my-security-hook
version: 0.1.0
description: Custom security validation for bash commands
author: user@example.com

# WASM 组件文件（相对路径）
wasm: hook.wasm

# 适用的 hook 事件
hook_points:
  - PreToolUse
  - PostToolUse

# 工具匹配器（regex 或 "*"）
matcher: "bash|shell_execute"

# 失败模式
failure_mode: fail_open  # fail_open | fail_closed

# 请求的 host 能力（用于安全审计）
capabilities:
  - log
  - get-context
  # - get-secret      # 可选，需要权限
  # - http-request    # 可选，需要权限
```

### 插件目录结构

```
~/.octo/plugins/
├── my-security-hook/
│   ├── manifest.yaml
│   └── hook.wasm
└── audit-logger/
    ├── manifest.yaml
    └── hook.wasm

$PROJECT/.octo/plugins/        # 项目级插件（优先级高于全局）
└── custom-validator/
    ├── manifest.yaml
    └── hook.wasm
```

---

## hooks.yaml 集成

```yaml
version: 1
hooks:
  PreToolUse:
    - matcher: "bash"
      actions:
        - type: command
          command: "python3 validate.py"
        - type: wasm                          # 新增 action type
          plugin: "my-security-hook"          # 引用已安装的插件名
          failure_mode: fail_closed
        - type: prompt
          prompt: "Evaluate safety..."
```

---

## 实施计划

### 分组概述

| 分组 | 任务数 | 核心目标 |
|------|-------|---------|
| **G1: Wasmtime 升级** | 2 个任务 | 升级到 36 + 启用 component-model feature |
| **G2: WIT 定义与绑定** | 2 个任务 | 创建 WIT 文件 + bindgen 生成宿主绑定 |
| **G3: WASM Hook 宿主** | 3 个任务 | Host import 实现 + WasmHookHandler + 插件加载 |
| **G4: 声明式集成** | 2 个任务 | hooks.yaml `type: wasm` + bridge 集成 |
| **G5: 示例与测试** | 2 个任务 | 示例插件 + 端到端测试 |

---

### G1: Wasmtime 升级（2 个任务）

#### Task 1: 升级 wasmtime 依赖到 36

**Files:**
- Modify: `Cargo.toml` (workspace)
- Modify: `crates/octo-engine/Cargo.toml`

**Step 1: 更新 workspace 依赖版本**

```toml
# Cargo.toml (workspace)
wasmtime = "36"
wasmtime-wasi = "36"
```

**Step 2: 添加 component-model feature**

```toml
# crates/octo-engine/Cargo.toml
[dependencies.wasmtime]
version = "36"
optional = true
features = ["component-model"]

[dependencies.wasmtime-wasi]
version = "36"
optional = true
```

**Step 3: 修复 API 兼容性变更**

检查并修复 `sandbox/wasm.rs` 和 `skill_runtime/wasm.rs` 中的 API 变更：
- `Module::from_binary()` → 检查签名变更
- `Linker` API 变更
- `Store` API 变更
- WASI preview1 → preview2 迁移（如需要）

**Step 4: 编译验证**

Run: `cargo check --workspace`
Run: `cargo test -p octo-engine -- sandbox_wasm --test-threads=1`
Run: `cargo test -p octo-engine -- wasm_skill_runtime --test-threads=1`

**Step 5: Commit**

---

#### Task 2: 验证现有 WASM 测试通过

**Files:** 无修改，仅验证

**Step 1: 运行所有 WASM 相关测试**

```bash
cargo test -p octo-engine -- wasm --test-threads=1
```

**Step 2: 检查 WASI 兼容性**

确认 WASI preview1 链接在 wasmtime 36 中仍然可用。

**Step 3: Commit（仅在有修复时）**

---

### G2: WIT 定义与绑定（2 个任务）

#### Task 3: 创建 WIT 接口文件

**Files:**
- Create: `crates/octo-engine/wit/octo-hook.wit`
- Create: `crates/octo-engine/wit/world.wit`（如需拆分）

将上面设计的 WIT 接口写入文件。

**Step 1: 创建 wit 目录和文件**
**Step 2: 验证 WIT 语法（使用 wasm-tools）**

```bash
# 可选：安装 wasm-tools 验证
cargo install wasm-tools
wasm-tools component wit crates/octo-engine/wit/
```

**Step 3: Commit**

---

#### Task 4: 生成宿主绑定（bindgen）

**Files:**
- Create: `crates/octo-engine/src/hooks/wasm/bindings.rs`（或内联 bindgen!）
- Modify: `crates/octo-engine/src/hooks/mod.rs`

**Step 1: 使用 wasmtime::component::bindgen! 宏**

```rust
// crates/octo-engine/src/hooks/wasm/mod.rs
wasmtime::component::bindgen!({
    world: "octo-hook-plugin",
    path: "wit/octo-hook.wit",
    async: true,
});
```

**Step 2: 编译验证生成的绑定**

Run: `cargo check -p octo-engine`

**Step 3: Commit**

---

### G3: WASM Hook 宿主（3 个任务）

#### Task 5: Host Import 实现

**Files:**
- Create: `crates/octo-engine/src/hooks/wasm/host_impl.rs`

实现 WIT `host` interface 的 5 个函数：
- `log(level, message)` → `tracing::info!` / `tracing::warn!` 等
- `get_context()` → 返回 `HookContext.to_json()` 字符串
- `get_secret(key)` → 调用 `CredentialResolver` 或返回 error
- `http_request(method, url, headers, body)` → `reqwest` 调用（5s 超时, 1MB 限制）
- `now_millis()` → `SystemTime::now()` since epoch

```rust
pub struct HookHostState {
    context: HookContext,
    allowed_capabilities: HashSet<String>,
    // Optional: credential_resolver, http_client
}

impl octo::hook::host::Host for HookHostState {
    fn log(&mut self, level: String, message: String) { ... }
    fn get_context(&mut self) -> String { ... }
    fn get_secret(&mut self, key: String) -> Result<String, String> { ... }
    fn http_request(&mut self, ...) -> Result<String, String> { ... }
    fn now_millis(&mut self) -> u64 { ... }
}
```

**安全约束:**
- `get_secret`: 仅在 manifest.capabilities 包含 `get-secret` 时可用
- `http_request`: 仅在 manifest.capabilities 包含 `http-request` 时可用，SSRF 保护（禁止 localhost/内网）

---

#### Task 6: WasmHookHandler 适配器

**Files:**
- Create: `crates/octo-engine/src/hooks/wasm/handler.rs`

```rust
pub struct WasmHookHandler {
    engine: wasmtime::Engine,
    component: wasmtime::component::Component,
    linker: wasmtime::component::Linker<HookHostState>,
    manifest: PluginManifest,
}

#[async_trait]
impl HookHandler for WasmHookHandler {
    fn name(&self) -> &str { &self.manifest.name }
    fn priority(&self) -> u32 { /* 从 WASM 模块获取 */ }
    fn failure_mode(&self) -> HookFailureMode { self.manifest.failure_mode() }

    async fn execute(&self, ctx: &HookContext) -> Result<HookAction> {
        // 1. 创建 Store<HookHostState>
        // 2. 实例化 Component
        // 3. 调用 hook-handler.execute(ctx.to_json())
        // 4. 解析返回的 HookDecision JSON
    }
}
```

---

#### Task 7: 插件 Manifest 解析与发现

**Files:**
- Create: `crates/octo-engine/src/hooks/wasm/manifest.rs`
- Create: `crates/octo-engine/src/hooks/wasm/loader.rs`

```rust
#[derive(Debug, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub wasm: String,           // 相对路径
    pub hook_points: Vec<String>,
    pub matcher: Option<String>,
    pub failure_mode: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

/// 扫描插件目录，加载所有有效插件
pub fn discover_plugins(dirs: &[PathBuf]) -> Vec<(PluginManifest, PathBuf)> {
    // 扫描 dir/*/manifest.yaml
}
```

---

### G4: 声明式集成（2 个任务）

#### Task 8: hooks.yaml 添加 wasm action type

**Files:**
- Modify: `crates/octo-engine/src/hooks/declarative/config.rs`

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum HookActionConfig {
    // ... 现有 prompt, command, webhook ...
    #[serde(rename = "wasm")]
    Wasm {
        plugin: String,
        #[serde(default)]
        failure_mode: FailureMode,
    },
}
```

---

#### Task 9: Bridge 集成 wasm action 执行

**Files:**
- Modify: `crates/octo-engine/src/hooks/declarative/bridge.rs`
- Modify: `crates/octo-engine/src/agent/runtime.rs`

在 `DeclarativeHookBridge` 中添加 `wasm_handlers: HashMap<String, Arc<WasmHookHandler>>` 字段。

`HookActionConfig::Wasm` 分支调用对应的 `WasmHookHandler::execute()`。

AgentRuntime 初始化时扫描插件目录，预加载所有 WASM 插件。

---

### G5: 示例与测试（2 个任务）

#### Task 10: 示例 WASM Hook 插件

**Files:**
- Create: `examples/wasm-hook-plugin/` (Rust 项目)
- Create: `examples/wasm-hook-plugin/Cargo.toml`
- Create: `examples/wasm-hook-plugin/src/lib.rs`
- Create: `examples/wasm-hook-plugin/manifest.yaml`

编写一个简单的安全检查插件（阻断 `rm -rf` 命令），编译为 `.wasm` 组件。

---

#### Task 11: 端到端测试

**Files:**
- Create: `crates/octo-engine/tests/wasm_hook_plugin.rs`

测试覆盖：
- 插件发现与加载
- manifest 解析
- Host import 调用（log, get-context）
- hook execute → allow/deny 决策
- 能力门控（无 http-request 权限时返回 error）
- 失败模式（fail-open / fail-closed）

---

## Deferred 项（暂缓项）

> 本阶段已知但暂未实现的功能点。

| ID | 内容 | 前置条件 | 优先级 | 状态 |
|----|------|---------|--------|------|
| AI-D1 | 插件热重载（文件变更自动重新加载） | G3 完成 | P3 | ⏳ |
| AI-D2 | 插件市场/注册表（远程安装 `octo plugin install`） | G3 完成 | P4 | ⏳ |
| AI-D3 | 插件间通信（一个插件调用另一个插件） | G3 完成 | P5 | ⏳ |
| AI-D4 | WASM 组件缓存（预编译 AOT 加速加载） | G2 完成 | P3 | ⏳ |
| AI-D5 | 多语言示例插件（Python/Go/JS via componentize） | G5 完成 | P3 | ⏳ |
| AI-D6 | 插件沙箱资源限制（CPU fuel + 内存上限） | G3 完成 | P2 | ✅ 已补 @ c68c373 |
| AI-D7 | AgentRuntime 启动时自动发现加载 WASM 插件 | G3+G4 完成 | P1 | ✅ 已补 @ c68c373 |

---

## 验收标准

- [ ] Wasmtime 36+ 编译通过，现有 WASM 测试无回归
- [ ] WIT 文件定义完成，bindgen! 生成绑定编译通过
- [ ] 5 个 host import 函数全部实现，含能力门控
- [ ] WasmHookHandler 实现 HookHandler trait，可在 HookRegistry 中注册
- [ ] 插件 manifest 解析 + 目录扫描发现
- [ ] hooks.yaml `type: wasm` action 可驱动 WASM 插件执行
- [ ] 示例插件可编译为 .wasm 组件并通过端到端测试
- [ ] 全程 `cargo check --workspace` 编译通过
- [ ] 新增测试 ≥ 20

---

## 文件结构预览

```
crates/octo-engine/
├── wit/
│   └── octo-hook.wit               # WIT 接口定义
├── src/hooks/
│   ├── wasm/                       # 新增：WASM hook 插件系统
│   │   ├── mod.rs                  # bindgen! + 模块导出
│   │   ├── host_impl.rs           # Host import 实现 (5 函数)
│   │   ├── handler.rs             # WasmHookHandler (HookHandler trait)
│   │   ├── manifest.rs            # PluginManifest 解析
│   │   └── loader.rs              # 插件发现 + 加载
│   ├── declarative/
│   │   ├── config.rs              # 修改：添加 Wasm action type
│   │   └── bridge.rs              # 修改：wasm action 分支
│   └── mod.rs                      # 修改：添加 pub mod wasm
└── Cargo.toml                      # 修改：wasmtime 36 + component-model

examples/
└── wasm-hook-plugin/               # 示例插件
    ├── Cargo.toml
    ├── src/lib.rs
    └── manifest.yaml
```
