# 沙箱安全体系设计方案

**版本**: v2.0
**创建日期**: 2026-03-14
**状态**: Phase J 实施中
**前置**: `ENTERPRISE_AGENT_SANDBOX_AUTH_DESIGN.md` v1.0

---

## 一、设计目标

octo-sandbox 的核心定位：**企业级安全沙箱**。所有 Agent 工具执行必须在隔离环境中完成。

### 核心原则

1. **默认安全** — `SandboxPolicy::Strict` 为默认，生产环境仅允许 Docker/WASM
2. **纵深防御** — 容器隔离 + 资源限制 + 网络隔离 + 审计日志
3. **全链路可审计** — 每个沙箱操作记录到防篡改 hash-chain 审计日志
4. **最小权限** — 容器以非 root 用户运行，无网络访问，受限资源

### 行业标准对标

| OWASP ASI 2026 风险 | 安全要求 | octo-sandbox 对策 |
|---|---|---|
| ASI-03 Unexpected Code Execution | 隔离执行环境 | SandboxPolicy::Strict — 仅 Docker/WASM |
| ASI-04 Tool Misuse | 工具调用限制 | SandboxRouter 策略拦截 + SecurityPolicy |
| ASI-06 Cascading Failures | 故障隔离 | 容器资源限制 (CPU/Memory/Network) |
| ASI-10 Rogue Agents | 不可变审计 | SHA-256 hash-chain 审计日志 |
| NVIDIA 安全指南 | 沙箱化所有执行路径 | 包含 hooks/MCP/skills |

---

## 二、架构设计

### 2.1 三级沙箱策略

```rust
pub enum SandboxPolicy {
    /// 生产环境：仅允许 Docker/WASM，拒绝 Subprocess
    Strict,      // ← 默认
    /// 预发布：优先 Docker/WASM，降级时记录审计警告
    Preferred,
    /// 开发/测试：允许 Subprocess 本机执行
    Development,
}
```

**策略矩阵**:

| 策略 | Docker | WASM | Subprocess | 降级行为 |
|------|--------|------|------------|---------|
| Strict | ✅ | ✅ | ❌ 拒绝 | 不降级，返回 PolicyDenied |
| Preferred | ✅ | ✅ | ⚠️ 降级允许 | 降级 + 审计警告 |
| Development | ✅ | ✅ | ✅ | 允许全部 |

### 2.2 沙箱执行架构

```
┌─────────────────────────────────────────────────────────────┐
│                     Agent Tool Call                          │
│                          │                                   │
│                 ┌────────▼────────┐                          │
│                 │  SecurityPolicy │                          │
│                 │  AutonomyLevel  │                          │
│                 │  RiskAssessment │                          │
│                 └────────┬────────┘                          │
│                          │                                   │
│                 ┌────────▼────────┐                          │
│                 │  SandboxPolicy  │ ← Strict (default)       │
│                 │  Strict/Pref/Dev│                          │
│                 └────────┬────────┘                          │
│                          │                                   │
│           ┌──────────────┼──────────────┐                    │
│           ▼              ▼              ▼                    │
│  ┌────────────────┐ ┌──────────┐ ┌───────────┐              │
│  │    Docker       │ │WASM/WASI │ │Subprocess │              │
│  │   Container     │ │ Runtime  │ │(Dev only) │              │
│  ├────────────────┤ ├──────────┤ ├───────────┤              │
│  │ ImageRegistry   │ │WASI CLI  │ │ sh -c     │              │
│  │ python:3.12    │ │stdout/err│ │ env_clear  │              │
│  │ rust:1.92      │ │args      │ │           │              │
│  │ node:22        │ │fs 隔离   │ │           │              │
│  │ alpine (CLI)   │ │          │ │           │              │
│  └───────┬────────┘ └────┬─────┘ └─────┬─────┘              │
│          │               │             │                     │
│          └───────────────┼─────────────┘                     │
│                          ▼                                   │
│                 ┌────────────────┐                            │
│                 │SandboxAuditLog │                            │
│                 │ SHA-256 chain  │                            │
│                 │ event_type     │                            │
│                 │ code_hash      │                            │
│                 │ resource_usage │                            │
│                 │ tamper-proof   │                            │
│                 └────────────────┘                            │
└──────────────────────────────────────────────────────────────┘
```

### 2.3 沙箱隔离级别对比

```
强 ←───────────────────────────────→ 弱
Firecracker   gVisor   Docker   WASM/WASI   Subprocess
  硬件隔离     内核级    容器级    字节码级      进程级
  150ms       <100ms   ~1-3s    <1ms          <1ms
```

**octo-sandbox 覆盖**: Docker(容器级) + WASM/WASI(字节码级)

---

## 三、Docker 沙箱设计

### 3.1 预置镜像矩阵

| 镜像基础 | 用途 | 体积 | 冷启动 |
|----------|------|------|--------|
| `python:3.12-slim-bookworm` | Python 脚本/数据处理 | ~150MB | ~2s |
| `rust:1.92-bookworm` | Rust 编译执行 | ~400MB | ~3s |
| `node:22-bookworm-slim` | JS/TS 工具调用 | ~130MB | ~1.5s |
| `alpine:latest` | Shell 命令/CLI 工具 | ~10MB | ~0.5s |

### 3.2 语言自动检测

```rust
pub struct ImageRegistry {
    images: HashMap<String, String>,
}

impl ImageRegistry {
    pub fn default_registry() -> Self {
        // language -> image 映射
        // python -> python:3.12-slim-bookworm
        // rust   -> rust:1.92-bookworm
        // node/javascript/typescript -> node:22-bookworm-slim
        // bash/sh/cli -> alpine:latest
    }

    pub fn resolve(&self, language: &str) -> &str {
        self.images.get(language).map(|s| s.as_str()).unwrap_or("alpine:latest")
    }
}
```

### 3.3 容器安全配置

每个 Docker 容器强制执行：

```rust
HostConfig {
    memory: config.memory_limit.map(|m| m as i64),  // 默认 256MB
    nano_cpus: Some(1_000_000_000),                  // 1 CPU
    network_mode: Some("none".to_string()),           // 网络隔离
    read_only_rootfs: Some(true),                     // 只读根文件系统
    security_opt: Some(vec!["no-new-privileges".into()]),
}
```

容器标签用于管理和泄露检测:
```
octo-sandbox=true
sandbox-id={uuid}
```

### 3.4 容器生命周期

```
Create                Execute (可多次)         Destroy
  │                       │                      │
  ├─ 镜像拉取 (首次)       ├─ docker exec          ├─ docker stop (10s)
  ├─ 容器创建              ├─ 捕获 stdout/stderr    ├─ docker rm --force
  ├─ 资源限制配置           ├─ 超时控制              └─ 实例记录清理
  ├─ 网络隔离              └─ exit code 传播
  └─ 容器启动
```

---

## 四、WASM/WASI 沙箱设计

### 4.1 WASI CLI 执行模式

WASM 适配器增强为完整的 WASI CLI 执行器，支持：

1. **stdin/stdout/stderr** — 通过 `MemoryPipe` 捕获
2. **命令行参数** — 传入 args
3. **文件系统隔离** — 可选挂载目录（`--dir`）
4. **exit code** — 正确处理 `I32Exit`

```rust
pub async fn execute_wasi_cli(
    &self,
    id: &SandboxId,
    wasm_bytes: &[u8],
    args: &[String],
    stdin: Option<&str>,
) -> Result<ExecResult, SandboxError>
```

### 4.2 WASM 适用场景

| 场景 | 启动时间 | 隔离级别 | 示例 |
|------|---------|---------|------|
| CLI 工具快速执行 | <1ms | 字节码级 | jq, yq, 验证器 |
| 纯计算任务 | <1ms | 完全隔离 | 哈希, 编码, 数学 |
| 格式转换 | <1ms | 完全隔离 | JSON→YAML |
| Agent 辅助工具 | <1ms | 完全隔离 | 参数解析, 模板渲染 |

### 4.3 WASM 安全特性

- **内存隔离** — 线性内存，无法访问宿主内存
- **系统调用限制** — 仅 WASI 定义的接口
- **文件系统受限** — 仅显式挂载的目录可访问
- **无网络** — 除非显式配置 `wasi-sockets`
- **确定性执行** — 相同输入产生相同输出

---

## 五、审计日志设计

### 5.1 设计原则

1. **复用现有 AuditStorage** — 不新建表，通过 `event_type="sandbox"` + `metadata` JSON 区分
2. **Hash-chain 防篡改** — 每条记录包含前一条的 SHA-256 hash
3. **全操作覆盖** — Create, Execute, Destroy, PolicyDeny, Degradation

### 5.2 SandboxAuditEvent

```rust
pub struct SandboxAuditEvent {
    pub sandbox_type: SandboxType,
    pub sandbox_id: String,
    pub action: SandboxAction,
    pub language: String,
    pub code_hash: String,           // SHA-256 of code
    pub image: Option<String>,
    pub exit_code: Option<i32>,
    pub execution_time_ms: u64,
    pub stdout_size: usize,
    pub stderr_size: usize,
    pub policy: SandboxPolicy,
    pub was_degraded: bool,
    pub resource_usage: Option<ResourceUsage>,
}

pub enum SandboxAction {
    Create,
    Execute,
    Destroy,
    PolicyDeny,          // Strict 模式拒绝
    DegradationWarning,  // Preferred 模式降级
    ResourceExceeded,
    Timeout,
}
```

### 5.3 审计日志流

```
SandboxRouter::execute()
    │
    ├─ 策略检查失败 → log SandboxAction::PolicyDeny
    │
    ├─ 降级发生 → log SandboxAction::DegradationWarning
    │
    ├─ 执行成功 → log SandboxAction::Execute (result=success)
    │
    ├─ 执行失败 → log SandboxAction::Execute (result=failure)
    │
    ├─ 超时 → log SandboxAction::Timeout
    │
    └─ 资源超限 → log SandboxAction::ResourceExceeded
```

### 5.4 审计查询

```rust
// 查询沙箱审计日志
audit_storage.query(Some("sandbox"), None, 100, 0)?;

// 查询策略拒绝事件（安全审计重点）
audit_storage.query_sandbox_events(None, Some("PolicyDeny"), 50, 0)?;

// 验证审计链完整性
audit_storage.verify_chain(from_id, to_id)?;
```

---

## 六、SandboxRouter 决策流程

```
execute(category, code, language)
    │
    ├─1. 确定目标 SandboxType (ToolCategory → SandboxType 映射)
    │
    ├─2. 策略检查: policy.allows(target_type)?
    │     ├─ No → return Err(PolicyDenied) + 审计
    │     └─ Yes → continue
    │
    ├─3. 适配器可用性检查: adapters.get(target_type)?
    │     ├─ Available → 使用目标适配器
    │     └─ Not available → 降级检查
    │           ├─ Strict → return Err(PolicyDenied)
    │           ├─ Preferred → 降级到可用适配器 + 审计警告
    │           └─ Development → 降级到 Subprocess
    │
    ├─4. 镜像选择: ImageRegistry::resolve(language)
    │
    ├─5. 创建沙箱 → 执行代码 → 销毁沙箱
    │
    └─6. 记录审计日志
```

---

## 七、配置集成

### 7.1 config.yaml

```yaml
sandbox:
  policy: strict          # strict | preferred | development
  default_image: alpine:latest
  images:
    python: python:3.12-slim-bookworm
    rust: rust:1.92-bookworm
    node: node:22-bookworm-slim
    cli: alpine:latest
  limits:
    memory_mb: 256
    cpu_cores: 1
    timeout_secs: 30
    network: none         # none | host | bridge
```

### 7.2 环境变量覆盖

```bash
OCTO_SANDBOX_POLICY=development     # 开发模式覆盖
OCTO_SANDBOX_DEFAULT_IMAGE=alpine   # 默认镜像覆盖
```

---

## 八、与现有模块的集成关系

```
SecurityPolicy (security/policy.rs)
    ├── AutonomyLevel → 决定是否允许执行
    ├── CommandRiskLevel → 风险评估
    └── ActionTracker → 频率限制
         │
         ▼
SandboxPolicy (sandbox/traits.rs)
    ├── Strict/Preferred/Development → 沙箱类型限制
    └── SandboxRouter 策略决策
         │
         ▼
AuditStorage (audit/storage.rs)
    ├── event_type="sandbox" → 沙箱审计
    ├── hash-chain → 防篡改
    └── metadata JSON → 详细信息
```

---

## 九、未来演进（Phase K+）

| 演进方向 | 说明 | 优先级 |
|---------|------|--------|
| 沙箱池化 | 预创建 Docker 容器池，减少冷启动 | P1 |
| gVisor 集成 | 替代 Docker 实现内核级隔离 | P2 |
| 类型统一 | 合并 `octo-types::sandbox` 和 `octo-engine::sandbox::traits` | P2 |
| WASM 组件模型 | 支持 WASI 0.3 Component Model | P3 |
| 审计仪表盘 | Web UI 展示沙箱审计日志 | P3 |
