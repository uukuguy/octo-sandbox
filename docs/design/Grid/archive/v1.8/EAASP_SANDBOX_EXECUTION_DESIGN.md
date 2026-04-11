# EAASP 环境下沙箱执行模式设计

> **版本**: v1.0
> **创建日期**: 2026-04-06
> **基线**: Phase BD 完成 @ ae4b337（grid-runtime crate 全部 6 个 work item）
> **关联文档**:
> - `SANDBOX_SECURITY_DESIGN.md` — grid-engine 层沙箱安全策略（SandboxPolicy / Profile）
> - `EAASP_ROADMAP.md` — EAASP 中长期演进路线图
> - `EAASP_-_企业自主智能体支撑平台设计规范_v1.7_.pdf` — 权威规范

---

## 一、概述

EAASP 平台中存在三类容器镜像，各有不同职责：

| 镜像 | 构建源 | 职责 | 运行生命周期 |
|------|--------|------|------------|
| **grid-runtime** | `crates/grid-runtime/Dockerfile` | gRPC server — Agent "大脑"，暴露 EAASP 16 方法契约 | 按需启动，可常驻 |
| **grid-sandbox** | `container/Dockerfile` | 工具执行环境 — Agent "手脚"，内含 Python/Node/Bash/数据库客户端等 | 按会话创建，会话结束销毁 |
| **grid-server** | 根目录 `Dockerfile` | Web 服务（Axum HTTP + WS），面向 Workbench UI | 长期运行 |

类比：**grid-runtime 容器 = Agent 的大脑，grid-sandbox 容器 = Agent 的手脚。**

---

## 二、四种工具执行模式

当 Agent 发出工具调用（如 BashTool、PythonTool），系统需要决定 **在哪里执行**。EAASP 支持四种模式：

### 2.1 模式概览

| 模式 | 执行位置 | 隔离度 | 启动延迟 | 资源开销 |
|------|---------|--------|---------|---------|
| **Host 直执行** | runtime 宿主机进程内 | 无 | ~0ms | 零 |
| **容器内直执行** | runtime 容器内部 | 容器级 | ~0ms | 零（共享容器） |
| **DooD 兄弟容器** | 宿主机上独立 sandbox 容器 | 容器级 + 资源隔离 | ~200ms 冷启动 | 中（每会话一个容器） |
| **Sidecar 代理** | 外部沙箱池服务 | 完全隔离 | ~500ms 网络 | 高（独立服务 + 网络） |

### 2.2 Host 直执行

```
宿主机
└── grid-runtime 进程
    └── Agent → BashTool → 直接 std::process::Command 执行
```

- **适用**: 开发调试、单用户本地环境、信任环境
- **优点**: 零延迟，无需 Docker
- **缺点**: 无隔离，恶意工具调用可影响宿主机
- **触发条件**: `SandboxProfile::Dev` + `GridRunMode::Host`

### 2.3 容器内直执行

```
宿主机
└── grid-runtime 容器
    └── Agent → BashTool → 在容器内直接执行
```

- **适用**: Tier 1 Harness 简单任务、只读工具（file_read）、容器已有基础工具
- **优点**: 零额外开销，容器本身提供基础隔离
- **缺点**: 工具与 gRPC server 共享进程空间，资源竞争
- **触发条件**: `GridRunMode::Sandboxed`（自动检测 `/.dockerenv`）+ 低风险工具

### 2.4 DooD（Docker-out-of-Docker）兄弟容器

```
宿主机
├── grid-runtime 容器 (gRPC server)
│   └── Agent → BashTool → DockerAdapter
│       → 通过挂载的 /var/run/docker.sock
└── grid-sandbox 容器 (工具执行，与 runtime 平级)
    ├── memory_limit: 1-2GB
    ├── cpu_quota: 100k-200k
    ├── network_mode: bridge | none
    └── bind_mounts: [working_dir]
```

- **适用**: 多租户生产环境、需要资源隔离和网络控制
- **优点**: 每会话独立容器，资源可控，sandbox 销毁不影响 runtime
- **缺点**: 需要 Docker socket 权限（安全争议），冷启动延迟
- **触发条件**: `SandboxProfile::Stg/Prod` + runtime 容器挂载了 Docker socket
- **注意**: sandbox 容器和 runtime 容器是 **同级兄弟**，不是嵌套

### 2.5 Sidecar 代理

```
宿主机 / K8s 集群
├── grid-runtime Pod (gRPC server)
│   └── Agent → BashTool → HTTP/gRPC → Sandbox Pool Service
└── Sandbox Pool Service
    └── 管理 N 个 grid-sandbox 容器
        ├── 预热池（减少冷启动）
        ├── 按租户隔离
        └── 网络策略：runtime ↔ sandbox 仅允许特定端口
```

- **适用**: 企业合规、跨节点部署、Tier 2/3 不可信 runtime
- **优点**: 最强隔离，runtime 完全不接触 Docker，可跨节点调度
- **缺点**: 多一跳网络延迟，需要额外的沙箱池服务
- **触发条件**: L3 治理层配置 `execution_mode: sidecar`

---

## 三、EAASP 各层的典型配置

### 3.1 按 Tier 分层

| Tier | 定义 | 推荐执行模式 | 原因 |
|------|------|------------|------|
| **Tier 1 — Harness** | Grid 自有运行时，完全可信 | Dev: Host / Prod: DooD | 代码可审计，DooD 足够 |
| **Tier 2 — Aligned** | 第三方 Python/TS 运行时，部分可信 | Sidecar | runtime 代码不完全可控 |
| **Tier 3 — Framework** | 黑盒运行时 | Sidecar + 无网络 | 最严格隔离，deny-by-default |

### 3.2 按 Profile 分层

| Profile | memory_limit | cpu_quota | network_mode | 执行模式 |
|---------|-------------|-----------|-------------|---------|
| **dev** | 无限制 | 无限制 | bridge | Host 直执行 |
| **stg** | 2GB | 200k (2核) | bridge | DooD |
| **prod** | 1GB | 100k (1核) | none | DooD / Sidecar |

### 3.3 按工具风险分层

L3 治理层的 `RuntimeSelector` 可根据工具类别做细粒度路由：

| 工具类别 | 风险等级 | 即使 prod 也可容器内执行 | 必须隔离 |
|---------|---------|----------------------|---------|
| `file_read` | 低 | ✅ | |
| `file_write` | 中 | | ✅ |
| `bash` | 高 | | ✅ |
| `network_call` | 高 | | ✅ + 审计 |
| `python_exec` | 高 | | ✅ |

---

## 四、路由决策流程

```
Agent 发出 ToolCall
  │
  ▼
ExecutionTargetResolver
  │
  ├── 输入: GridRunMode × SandboxProfile × ToolCategory
  │
  ├── RunMode::Host + Profile::Dev
  │   └── → Local（直接执行）
  │
  ├── RunMode::Host + Profile::Stg/Prod + 低风险工具
  │   └── → Local（降级执行，记录审计）
  │
  ├── RunMode::Host + Profile::Stg/Prod + 高风险工具
  │   └── → Sandbox（DockerAdapter 启动 grid-sandbox 容器）
  │         └── 若 Docker 不可用 → **fail-fast**（不降级）
  │
  ├── RunMode::Sandboxed（runtime 在容器内）
  │   ├── 有 Docker socket → DooD 兄弟容器
  │   ├── 有 Sidecar URL  → HTTP 代理到沙箱池
  │   └── 都没有          → 容器内直执行（仅低风险）
  │                         高风险 → **fail-fast**
  │
  └── L3 Override（managed-settings.json）
      └── execution_mode: host | container | dood | sidecar
          → 强制覆盖以上所有逻辑
```

**关键原则**: Staging 和 Production 模式下，高风险工具 **永不降级** — 沙箱不可用时直接报错（fail-fast），而非回退到非隔离执行。

---

## 五、当前实现状态

| 组件 | 状态 | 位置 |
|------|------|------|
| `SandboxPolicy` (Strict/Preferred/Development) | ✅ 已实现 | `grid-engine/src/sandbox/` |
| `SandboxProfile` (dev/stg/prod/custom) | ✅ 已实现 | `grid-engine/src/sandbox/` |
| `GridRunMode` (Host/Sandboxed) 自动检测 | ✅ 已实现 | `grid-engine/src/sandbox/` |
| `ExecutionTargetResolver` | ✅ 已实现 | `grid-engine/src/sandbox/` |
| `SessionSandboxManager` (per-session Docker pool) | ✅ 已实现 | `grid-engine/src/sandbox/` |
| `DockerAdapter` (资源限制 + bind_mounts) | ✅ 已实现 | `grid-sandbox/src/` (octo-sandbox crate) |
| grid-runtime Dockerfile | ✅ 已实现 | `crates/grid-runtime/Dockerfile` |
| grid-sandbox 容器镜像 | ✅ 已实现 | `container/Dockerfile` |
| **RuntimeSelector + AdapterRegistry** | ❌ 待实现 | BD-D2（平台层） |
| **Sidecar 代理模式** | ❌ 待实现 | 需 Sandbox Pool Service |
| **L3 execution_mode 配置下发** | ❌ 待实现 | BD-D4（managed-settings.json） |

---

## 六、后续演进

1. **BD-D2: RuntimeSelector** — 平台层根据 Tier × Profile × ToolCategory 动态路由
2. **BD-D4: managed-settings.json** — L3 治理层下发执行策略到 L1 runtime
3. **Sandbox Pool Service** — 独立服务管理沙箱容器池，支持预热和跨节点调度
4. **gVisor/Firecracker** — 替代 Docker 容器，提供更强的内核级隔离（企业合规场景）
5. **WASM Sandbox** — 轻量级替代方案，适合简单计算类工具，启动延迟 <10ms
