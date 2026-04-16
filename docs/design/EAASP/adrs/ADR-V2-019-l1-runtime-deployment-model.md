# ADR-V2-019 — L1 Runtime Deployment Model: Multi-Session 内在 + 部署模式可选 + 容错策略

**Status:** Proposed
**Date:** 2026-04-16
**Phase:** Phase 2.5 — Consolidation + goose-runtime (W1.T2.5 Dockerfile 实施依据)
**Author:** Jiangwen Su (orchestrated by claude-flow swarm-1776334245838)
**Related:** ADR-V2-017 (L1 runtime 生态策略 — 三轨), ADR-V2-005 (tool sandbox container), grid-runtime `DeploymentMode::{Shared, PerSession}` 枚举

---

## Context / 背景

Phase 2.5 S1 W1.T2 实施 `eaasp-goose-runtime` 时，F1 gate（goose 二进制可用性）暴露出一个更深层的架构问题：**L1 runtime 在 EAASP 生产环境是如何被拉起的？** 审计现状发现：

### 现状盘点

| Runtime | Dockerfile | Multi-session 设计 | Active-session 回退 | 状态 |
|---------|-----------|-------------------|--------------------|------|
| **grid-runtime** | ✅ 有 (`crates/grid-runtime/Dockerfile`, multi-stage) | ✅ 显式 `DeploymentMode::{Shared, PerSession}` 枚举 | "last-initialized" hack (Phase 0 MVP) | ✅ 主力 |
| **claude-code-runtime** | ✅ 有 (Python slim, Node for CLI) | ✅ `SessionManager` + `_sessions` dict | `_active_session_id` 回退（v2 Empty RPC 限制） | ✅ 样板 |
| **hermes-runtime** | ✅ 有 (Python, hermes-base 继承) | ✅ `SessionManager` + `_current_session` | 同上 | ❌ **已冻结** (ADR-V2-017) |
| **eaasp-goose-runtime** (W1.T2 现状) | ❌ **未实现** | ✅ `Arc<Mutex<HashMap<SessionId, SessionHandle>>>` | 待 T3 实装 | ⏳ W1.T2.5 起草 Dockerfile |

### 真正的架构问题

Dockerfile 注释里明确写着 **"L1 containers are EPHEMERAL: created per-session, destroyed on terminate"**（grid-runtime:4, claude-code-runtime:3），但**实际代码里所有 runtime 都是 shared-container multi-session**（`SessionManager`、`HashMap`）。也就是说：

1. **L1 runtime 代码层面**：一直是 multi-session 能力内置
2. **Dockerfile 注释层面**：暗示 per-session 容器（与代码不符）
3. **EAASP L4 调度层**：从未明确规定"该拉 1 个容器装 N session，还是 N 个容器装 1 session"

goose 的 ACP 模型强制了"每个 session = 一个 goose subprocess"（stdio 1:1 对话通道，没有 session 多路复用），这让"L1 容器与 session 的关系"问题必须在 Phase 2.5 内明确澄清——否则 T3 pydantic-ai / claw-code / ccb-runtime 每个 runtime 会各自决定，生态碎片化无法收敛。

### 附加动机

hermes 冻结后，**当前 EAASP 没有任何"L1 容器化参考样板"在活跃维护**。W1.T2.5 Dockerfile 需要替代这个空缺，顺便把 deployment model 当作一等公民固化下来。

---

## Decision / 决策

### D1. L1 Runtime 必须内在支持 multi-session（硬约束）

每个 L1 runtime 的 service 层实现必须持有一个 session 注册表：

```rust
// Rust
struct RuntimeService {
    sessions: Arc<tokio::sync::Mutex<HashMap<SessionId, SessionHandle>>>,
}
```

```python
# Python
class RuntimeServiceImpl:
    def __init__(self):
        self._sessions: dict[str, SessionHandle] = {}
        self._active_session_id: str | None = None  # v2 Empty RPC 回退
```

**理由**：
- 已有 3 个 runtime 都这么做（grid/claude-code/hermes），是既成事实的约束
- v2 proto 的 Empty RPC 设计（Terminate/Pause/Health/Capabilities 无 session_id）要求 runtime 内部能区分 N 个 session 或至少维护 "active-session" 回退
- 禁止"强制 per-session 单例"的 L1 实现（即禁止 `Option<SessionHandle>` 这种单插槽）

### D2. Deployment Mode 是运行时配置，不是架构差异

所有 L1 runtime 统一暴露一个环境变量：

```bash
EAASP_DEPLOYMENT_MODE=shared      # 默认：一个容器装 N 个 session
EAASP_DEPLOYMENT_MODE=per_session # 同镜像、同 gRPC server，但 max_sessions=1
```

实现侧的差异只在 `CreateSession` 入口的准入检查：

```rust
if deployment_mode == PerSession && sessions.len() >= 1 {
    return Err(Status::resource_exhausted("per-session mode: one session per container"));
}
```

**绝不允许**不同 runtime 的 Shared/PerSession 语义分歧：
- ❌ "PerSession = 用不同 Dockerfile 构建出来的 slim 镜像"
- ❌ "Shared = 有额外的 nginx 前置反向代理"
- ✅ "Shared/PerSession = 同一个镜像 + 不同的 env var + EAASP L4 拉起参数差异"

**理由**：
- EAASP L4 不应该感知 runtime 内部实现差异
- 统一的环境变量让 K8s / docker-compose / systemd 三种部署方式都能用相同的编排模板
- 未来 hybrid (pool) 模式只需在 L4 层加一个 "pre-warmed container pool manager"，L1 runtime 代码不变

### D3. EAASP L4 调度层拥有"选择 deployment mode"的权力

L1 runtime 只是"遵从"。L4 根据以下信号选择：

| 信号 | 推荐 mode |
|------|-----------|
| 默认 / 开发环境 / 性能测试 | **Shared** (低冷启动、高吞吐) |
| 多租户隔离硬约束 (PII/合规) | **PerSession** (容器级隔离) |
| 单 session 长任务 (>1h skill-extraction) | **PerSession** (崩溃影响面收窄) |
| 会话短、调用频繁 (每秒 N 个新 session) | **Shared** + pool (未来 hybrid) |

**L4 的 CreateSession 调度流程**：
```
1. 检查当前 Shared pool 是否有健康容器
2. 若无 或 请求 PerSession：docker run <image> -e EAASP_DEPLOYMENT_MODE=<mode>
3. 拿到容器 port → gRPC Initialize → 分发给业务侧
```

### D4. 容错策略分级（可选，按 SLO 选）

Shared mode 的"容器挂→会话全挂"不是 bug，是可接受的 trade-off。EAASP L4 按业务 SLO 选加固级别：

| 级别 | 策略 | 恢复时间目标 | 实施成本 | 适用 |
|------|------|------------|---------|------|
| **基线** | 容器挂 = 所有 session 失败 | 无自动恢复 | 零 | 开发 / 非关键业务 |
| **加固 A** (推荐默认) | L4 健康探活 + 自动 `docker run` 重启容器 | 5-30s | 低（L4 watchdog） | 生产默认 |
| **加固 B** | 切到 `PerSession` 模式 | 单 session 崩溃不影响他人 | 中（资源消耗 N 倍） | 强隔离硬约束 |
| **加固 C** | Hedged active-active (2 个 Shared 容器 + L4 routing) | <1s failover | 高（L4 需状态同步）| 金融级（Phase 3+） |
| **加固 D** | Session state checkpoint + replay（L2 memory 已写盘部分可恢复）| 中（L4 需 replay 逻辑） | 中 | 长任务（skill-extraction 风格） |

**实施建议**：Phase 2.5 只实装**基线**（默认 mode=Shared）+ **加固 B** 的 env var 开关。加固 A/C/D 是 EAASP L4 Phase 3+ 范畴，不入 L1 runtime scope。

### D5. Session Lifecycle 与容器 Lifecycle 解耦

**反对**将"CreateSession"与"启动一个新容器"绑定为原子操作（虽然 Dockerfile 注释是这么写的）。正确语义：

```
容器生命周期 >= max(所有内部 session 生命周期)   # Shared mode
容器生命周期 == 单个 session 生命周期            # PerSession mode
```

`Terminate(session_id)` 只清理 session handle，**不杀容器**（除非 PerSession 模式下是最后一个 session）。容器关闭由 L4 显式 `docker stop` 发起，或由空闲检测 policy 触发。

**例外**：goose 的 per-session subprocess 必须在 session Terminate 时 SIGTERM+SIGKILL（已在 W1.T2 `close_session` 实装，F3 fallback）。subprocess 杀与容器杀是两件事。

---

## Consequences / 影响

### ✅ 正面

1. **L1 生态一致性恢复** — 4 个 runtime (grid / claude-code / goose / nanobot) 统一遵从 D1-D5
2. **EAASP L4 编排简化** — 一个 docker image + 一个 env var 搞定所有部署形态
3. **hermes 的容器化样板价值有正式替代** — eaasp-goose-runtime Dockerfile (W1.T2.5) 就是新样板
4. **goose 的 per-session subprocess 约束被封装** — 对 L4 完全透明，L4 看到的只是"一个容器里 N 个 session"
5. **W1.T2 代码零改动** — `Arc<Mutex<HashMap>>` 就是 D1 的合规实现

### ⚠️ 风险 / Trade-off

1. **Shared mode 容器故障全挂**是默认接受的行为 — 需要在 ADR 和文档里清楚说明，避免生产事故后责任争议
2. **max_sessions 配额** — D2 的 per_session 判定靠 `sessions.len() >= 1`，没有对 Shared 模式加 cap；若需要 per-runtime session 上限，需要额外 env var `EAASP_MAX_SESSIONS`（未入 D2 范围，留给 Phase 3）
3. **L4 调度层还不存在** — D3 描述的 L4 行为目前在 grid-platform 部分实现；ADR 起草时未完全对齐，可能需要后续 ADR 补齐 L4 责任边界

### 📦 现有 runtime 合规情况

| Runtime | D1 Multi-session | D2 EAASP_DEPLOYMENT_MODE env | D3 L4 拉起 | D4 容错 | D5 Lifecycle 解耦 | 合规度 |
|---------|-----------------|------------------------------|-----------|---------|------------------|--------|
| grid-runtime | ✅ | ❌ (用 `DeploymentMode` enum，需加 env 映射) | 部分 | ❌ 基线 | ✅ | 🟡 85% — 需补 env var 读取 |
| claude-code-runtime | ✅ | ❌ (未读 env) | 部分 | ❌ 基线 | ✅ | 🟡 80% — 需补 env var 读取 |
| hermes-runtime | ✅ (但冻结) | N/A | N/A | N/A | ✅ | ⛔ 冻结不补 |
| eaasp-goose-runtime (W1.T2.5 设计) | ✅ | ✅ (新装) | ✅ (新装) | ✅ 基线+B | ✅ | 🟢 100% (目标) |
| nanobot-runtime-python (待 W2.T3+) | ⏳ 计划 ✅ | ⏳ 计划 ✅ | ⏳ 计划 ✅ | ⏳ 基线 | ⏳ 计划 ✅ | ⏳ 100% (目标) |

**行动项（Deferred）**：
- **D142 🟡 P2-defer** — grid-runtime 补 `EAASP_DEPLOYMENT_MODE` env 读取（映射到现有 `DeploymentMode` enum）
- **D143 🟡 P2-defer** — claude-code-runtime 补 `EAASP_DEPLOYMENT_MODE` env 读取
- 两项都是**小改动** (~20 LOC)，不阻塞 Phase 2.5，在 S3 CI gate 阶段批量处理

---

## Alternatives Considered / 备选方案

### A. 每个 runtime 自行决定 deployment model
- **拒绝原因**：已经发生了（grid 用 enum、claude-code 用 active-session hack、hermes 未显式），生态碎片化是现状问题，不是解决方案
- **代价**：EAASP L4 调度层要写 N 个分支逻辑，每个 runtime 都要特判

### B. 强制 PerSession，砍掉 Shared
- **拒绝原因**：grid-runtime 和 claude-code-runtime 当前生产路径都是 Shared；强制 PerSession 会引入 N 倍资源消耗和冷启动延迟，违反 Phase 2.5 "不破坏现有生态" 原则
- **代价**：对快速调用的用例（web IDE 编辑 assist、在线 debug 会话）性能不可接受

### C. 每 session 独立容器（gVisor / Kata 级隔离）
- **拒绝原因**：远超 Phase 2.5 scope；对标大厂 serverless container pool (AWS Firecracker)，实施周期数月
- **代价**：Phase 3+ 再议；可作为"加固 C" 进化路径预留

### D. Shared 无状态 + 外部 session store (Redis)
- **拒绝原因**：与 Phase 2 已定的"L2 memory 是 L1 runtime 之外的服务"架构冲突；引入中心化 session store 破坏 L1 的自治性
- **代价**：增加一个网络往返 + Redis 运维负担；收益只在"容器崩溃后 session 状态保留"，这可以由加固 D（L2 replay）以更轻的方式做到

---

## Implementation / 实施路径

### Phase 2.5 立即执行
1. **W1.T2.5** — `crates/eaasp-goose-runtime/Dockerfile` 按 D2/D4 基线+B 实装
2. **W1.T3 middleware** — `eaasp-scoped-hook-mcp` 在 Shared mode 下一个容器内服务多 session
3. **W2.T3+** — nanobot-runtime 按 D1-D5 实装
4. **Phase 2.5 S3 CI gate** — 验证 4 runtime 的 D1 合规（HashMap 存在）、D2 合规（env 读取）

### Phase 3 预留
1. L4 调度层按 D3/D4 加固 A/C 加固
2. D142/D143 补齐 grid/claude-code 的 env 读取
3. 加固 D (L2 replay) 若有长任务 SLO 要求才做

---

## References

- ADR-V2-017 — L1 Runtime 生态策略（三轨：主力/样板/对比）
- ADR-V2-005 — Tool sandbox container (与本 ADR 正交：那个是**工具执行**沙箱，本 ADR 是 **runtime 自身**部署)
- `crates/grid-runtime/src/service.rs:8-12` — Shared 模式 "last-initialized" 注释
- `lang/claude-code-runtime-python/src/claude_code_runtime/service.py:128` — `_active_session_id` 回退
- `crates/grid-runtime/Dockerfile:4` — "L1 containers are EPHEMERAL" 注释（本 ADR D5 修正：只在 PerSession 模式下成立）
- `crates/eaasp-goose-runtime/src/goose_adapter.rs:28-41` — W1.T2 subprocess 模型已经 D1 合规
- W1.T0 goose availability spike (commit 9b21112) — ACP stdio 强制 per-session subprocess 的技术约束来源

---

**状态变更日志**
| 日期 | 状态 | 变更说明 |
|------|------|---------|
| 2026-04-16 | Proposed | 起草以覆盖 Phase 2.5 W1.T2.5 Dockerfile 的架构前提 + 替代冻结 hermes 的容器化样板空缺 |
