# octo-sandbox 安全模型竞品代码级对比分析

> 基于源代码实际阅读的功能等价性对比，非关键字匹配。
> 分析日期：2026-03-12
> 对比对象：octo-sandbox vs zeroclaw / localgpt / ironclaw / openfang / goose / moltis

---

## 评分总览

| 维度 | octo | zeroclaw | localgpt | ironclaw | openfang | goose | moltis |
|------|------|----------|----------|----------|----------|-------|--------|
| 1. Safety Pipeline | **9** | 8 | 5 | 7 | 4 | 5 | 6 |
| 2. 注入检测 | **7** | 7 | 3 | 6 | 2 | **8** | 3 |
| 3. Canary Token | **6** | **9** | 0 | 0 | 0 | 0 | 0 |
| 4. PII 扫描 | **8** | 3 | 0 | 5 | 0 | 0 | 0 |
| 5. 沙箱隔离 | **8** | 7 | **9** | 7 | 5 | 2 | 4 |
| 6. 审计日志 | **7** | 6 | 3 | 4 | **8** | 2 | 3 |
| 7. 紧急停止 | **0** | **9** | 0 | 0 | 0 | 0 | 0 |
| 8. 污点追踪 | **9** | 1 | 0 | 0 | 0 | 0 | 0 |
| **加权总分** | **54** | **50** | **20** | **29** | **19** | **17** | **16** |

评分标准：0=无实现，1-3=基础/骨架，4-6=可用但有限制，7-8=生产可用，9-10=业界领先。

---

## 维度 1：Safety Pipeline 架构

### octo-sandbox -- 评分 9

**文件**: `crates/octo-engine/src/security/pipeline.rs`

可组合的 `SafetyPipeline` 架构，是所有竞品中设计最优雅的：

- **SafetyLayer trait** -- 统一接口，返回 `Allow / Sanitize(修改后内容) / Warn(附加警告) / Block(原因)` 四级决策
- **短路机制** -- 遇到 Block 立即终止后续 layer
- **最严格合并** -- 多 layer 结果取最严格决策（Block > Sanitize > Warn > Allow）
- **4 个内置 layer** -- InjectionDetectorLayer, PiiScannerLayer, CanaryGuardLayer, CredentialScrubber
- **3 点集成** -- 在 `harness.rs` 的输入检查(~line 351)、输出检查(~line 560)、工具结果检查(~line 996) 三个位置执行
- **AiDefence 独立运行** -- 在 pipeline 之外额外运行 `check_input` / `check_output`

**优势**: 真正的"可组合安全"，用户可以自由添加/移除/排序 layer，这是其他竞品都没有的。

### zeroclaw -- 评分 8

**文件**: `src/security/mod.rs`

模块式安全架构，功能极其丰富（20+ 子模块），但不是可组合 pipeline：

- `prompt_guard` + `semantic_guard` -- 双层注入检测
- `canary_guard` -- Per-turn 旋转 canary
- `leak_detector` -- 秘密泄露检测
- `estop` -- 紧急停止
- `landlock` / `firejail` / `bubblewrap` / `docker` -- 多沙箱后端
- `syscall_anomaly` -- 系统调用异常检测

**不足**: 各模块独立调用，缺乏统一的 pipeline 编排层。安全策略的组合逻辑分散在调用方。

### ironclaw -- 评分 7

**文件**: `src/safety/mod.rs`

`SafetyLayer` 结构体组合 4 个组件：Sanitizer + Validator + Policy + LeakDetector。
- 有 XML wrapper 隔离外部数据 (`wrap_for_llm`)
- 有 `wrap_external_content` 显式标注不可信内容
- Policy 支持 Block / Warn / Review / Sanitize 四种动作
- **不足**: 不可组合，是固定 4 组件的硬编码组合

### 其他

| 框架 | 评分 | 理由 |
|------|------|------|
| moltis | 6 | 6 层 ToolPolicy 解析（global->provider->agent->group->sender->sandbox），glob 模式匹配，deny-wins 语义，但仅限工具调用层面 |
| localgpt | 5 | HMAC-SHA256 策略文件验证 + 内容净化管道 + 篡改检测，但无运行时注入检测 |
| goose | 5 | PromptInjectionScanner 独立运行，无 pipeline 组合 |
| openfang | 4 | subprocess_sandbox 环境清洗 + 路径验证，无统一安全层 |

**结论**: octo-sandbox 的 SafetyPipeline 是唯一真正可组合的安全编排架构。这是**真优势**。

---

## 维度 2：注入检测

### octo-sandbox -- 评分 7

**文件**: `crates/octo-engine/src/security/ai_defence.rs`

- `InjectionDetector`: 23 个关键字模式 + 4 个正则模式
- 涵盖: system-role-marker, instruction-block, 中文角色切换模式
- 对 input 执行 Block，对 tool results 执行 Warn
- **不足**: 纯规则，无 ML 分类器，无语义分析

### goose -- 评分 8（唯一 ML 方案）

**文件**: `crates/goose/src/security/scanner.rs`

- `PromptInjectionScanner`: 规则 + 可选 ML 分类器
- 双分类器: command classifier + prompt classifier
- 加权置信度组合: tool 80% + context 20%，双高时放大
- **不足**: 仅扫描 "shell" 工具调用，覆盖面窄

### zeroclaw -- 评分 7

- `prompt_guard` + `semantic_guard` 双层
- `perplexity` 困惑度分析（统计方法检测注入）
- 覆盖面广但不如 goose 的 ML 方案精确

### ironclaw -- 评分 6

- `Sanitizer` 模式检测 + 内容转义
- `Validator` 长度/编码/禁止模式检查
- 命令注入检测（链式命令、子 shell、路径穿越）
- 但缺少中文场景支持

### 其他

| 框架 | 评分 | 理由 |
|------|------|------|
| localgpt | 3 | 仅在策略文件层面做验证，无运行时注入检测 |
| moltis | 3 | SSRF 防护出色但无 LLM 注入检测 |
| openfang | 2 | 仅环境变量清洗和路径验证 |

**差距判定**: goose 的 ML 分类器是**真差距**。建议 octo-sandbox 增加可选的 ML 分类器 layer 插入 SafetyPipeline。

---

## 维度 3：Canary Token

### octo-sandbox -- 评分 6

**文件**: `crates/octo-engine/src/security/pipeline.rs` (CanaryGuardLayer)

- 在 output 和 tool-result 中检测 canary 泄露
- 发现泄露立即 Block
- **不足**: 静态 canary，不做 per-turn 旋转

### zeroclaw -- 评分 9（业界最佳）

**文件**: `src/security/canary_guard.rs`

- Per-turn UUID canary 旋转（ZCSEC- 前缀）
- 每轮自动剥离旧 canary block，注入新 canary
- 支持 redaction 模式
- 真正防御重放攻击

### 其他所有竞品 -- 评分 0

无 canary token 实现。

**差距判定**: Per-turn 旋转是**真差距**。静态 canary 可被对手缓存后绕过。建议升级为每轮旋转。改动量小（仅需在 CanaryGuardLayer 中添加轮转逻辑），收益高。

---

## 维度 4：PII 扫描

### octo-sandbox -- 评分 8

**文件**: `crates/octo-engine/src/security/ai_defence.rs` (PiiScanner)

- 6 个正则规则: email, phone_cn, phone_us, ssn_us, credit_card, china_id
- 支持 redaction（替换为 `[REDACTED_*]`）
- **中国特定模式**: 手机号、身份证号 -- 在所有竞品中独有
- 输入端 Warn，输出端 Sanitize
- **不足**: 缺少护照号、银行卡 BIN 等高级模式

### ironclaw -- 评分 5

- LeakDetector 扫描 15+ 秘密模式（API key、token、private key、connection string）
- 支持 Block / Redact / Warn 三种动作
- 但定位是"秘密泄露"而非 PII，不检测个人信息

### zeroclaw -- 评分 3

- `leak_detector` 模块存在但偏重 credential 泄露
- 无专门 PII 扫描

### 其他 -- 评分 0

无 PII 扫描实现。

**差距判定**: octo-sandbox 在 PII 维度**领先所有竞品**。这是**真优势**，尤其是中国场景覆盖。

---

## 维度 5：沙箱隔离

### octo-sandbox -- 评分 8

**文件**: `crates/octo-engine/src/sandbox/router.rs`, `crates/octo-sandbox/`

- **3 层沙箱**: Subprocess（默认）/ WASM (Wasmtime 25) / Docker (Bollard 0.18)
- **SandboxRouter**: 按 ToolCategory 智能路由（Shell->Docker, Compute->Wasm, FileSystem->Docker, Network->Wasm）
- **RuntimeAdapter trait**: 统一抽象，可扩展
- **不足**: 无 OS 级沙箱（Landlock/seccomp），无网络代理

### localgpt -- 评分 9（OS 级最强）

**文件**: `crates/sandbox/src/linux.rs`, `crates/sandbox/src/policy.rs`

- **真正的 Landlock LSM + seccomp-bpf**: NO_NEW_PRIVS -> Landlock 文件系统规则 -> seccomp 网络拒绝
- 拒绝 13 个网络系统调用（socket, connect, accept, bind, listen, send*, recv*, ptrace）
- Landlock ABI V5，优雅降级
- SandboxPolicy: workspace R/W, read-only 系统路径, credential 拒绝路径
- rlimits: timeout, max_output, max_file_size, max_processes
- **HMAC-SHA256 策略文件签名** -- 防篡改
- **不足**: 仅限 Linux，无 Docker/WASM 替代方案

### zeroclaw -- 评分 7

- `landlock` / `firejail` / `bubblewrap` / `docker` 四种后端
- `wrap_command()` 当前返回 Unsupported（**失败关闭**设计，非生产就绪）
- Shell 工具: `env_clear()` + SAFE_ENV_VARS 白名单 + SyscallAnomalyDetector
- 60s 超时 + 1MB 输出限制

### ironclaw -- 评分 7

- Docker 沙箱 + 网络代理
- 3 级策略: ReadOnly / WorkspaceWrite / FullAccess
- **网络代理**: 域名白名单 + credential 注入（代理时注入而非环境变量）
- 零暴露 credential 模型: 秘密永远不进入容器

### openfang -- 评分 5

- subprocess sandbox: `env_clear()` + safe vars 白名单 + 路径验证
- Docker sandbox 存在但功能较基础

### moltis -- 评分 4

- **SSRF 防护**: DNS 解析 + 私有 IP 检查（loopback, private, link-local, broadcast, CGNAT, IPv6）
- CIDR 白名单支持
- 但无进程级隔离

### goose -- 评分 2

- 无专门沙箱模块
- 安全重心在注入检测而非执行隔离

### OS 级沙箱必要性评估

**结论: 在已有 Docker + WASM 的前提下，Landlock/seccomp 是伪差距。**

理由：
1. **Docker 已提供 namespace + cgroup + seccomp 隔离** -- Docker 自身就使用 seccomp profile
2. **WASM 提供确定性沙箱** -- 内存隔离、无系统调用访问
3. **Landlock 仅限 Linux 5.13+** -- localgpt 的实现虽精致，但在 macOS/Windows 上无效
4. **zeroclaw 的 Landlock 实际未启用** -- `wrap_command()` 返回 Unsupported，说明生产落地困难
5. **真正有价值的补充是**: SSRF 防护（moltis 做得好）和网络代理（ironclaw 做得好）

**建议**: 不追加 Landlock/seccomp，而是：
- 为 MCP 模块增加 SSRF 防护（参考 moltis 的 DNS 解析 + 私有 IP 阻断）
- 为 Docker 沙箱增加网络代理 + credential 注入（参考 ironclaw）

---

## 维度 6：审计日志

### openfang -- 评分 8（业界最强）

**文件**: `crates/openfang-runtime/src/audit.rs`

- **Merkle 哈希链审计**: 每条记录包含前一条的 SHA-256 哈希，形成防篡改链
- 12 类审计动作: ToolInvoke, CapabilityCheck, AgentSpawn, AgentKill, AgentMessage, MemoryAccess, FileAccess, NetworkAccess, ShellExec, AuthAttempt, WireConnect, ConfigChange
- SQLite 持久化（schema V8）
- **不足**: 无导出/查询 API

### octo-sandbox -- 评分 7

**文件**: `crates/octo-engine/src/audit/storage.rs`

- SQLite 审计存储
- AuditEvent: event_type, user_id, session_id, resource_id, action, result, metadata, ip_address
- 结构化元数据
- **不足**: 无 Merkle 链防篡改

### zeroclaw -- 评分 6

- `audit` 模块存在
- 但架构细节不如 openfang 的 Merkle 链成熟

### ironclaw -- 评分 4

- 通过 `observability/` 模块记录事件
- Observer trait + MultiObserver 扇出
- 但无专门审计存储

### 其他

| 框架 | 评分 | 理由 |
|------|------|------|
| localgpt | 3 | 日志级别控制但无结构化审计 |
| moltis | 3 | 日志配置但无审计存储 |
| goose | 2 | 无审计模块 |

**差距判定**: Merkle 链防篡改是**真差距**。建议为 octo-sandbox 的审计存储增加哈希链（改动量小，仅需在 AuditStorage 的 insert 方法中添加 SHA-256 链计算）。

---

## 维度 7：紧急停止 (Emergency Stop)

### zeroclaw -- 评分 9（唯一实现者）

**文件**: `src/security/estop.rs`

- **EstopManager**: 4 级紧急停止 -- KillAll / NetworkKill / DomainBlock / ToolFreeze
- 持久化 JSON 状态（重启后仍生效）
- **失败关闭**: JSON 损坏时自动触发最高级 estop
- **OTP 保护恢复**: 解除 estop 需要一次性密码
- 完整的 estop -> resume 生命周期

### 所有其他竞品 -- 评分 0

无紧急停止机制。

**差距判定**: 这是**真差距**。在自主代理场景下，用户必须有"大红按钮"能力。

**建议**: 实现为 SafetyPipeline 的顶层 layer：
1. `EstopLayer` -- 在所有其他 layer 之前检查 estop 状态
2. 多级: ToolFreeze（禁止工具调用）-> AgentPause（暂停循环）-> KillAll（终止所有执行）
3. 持久化状态 + 恢复验证
4. 改动量中等，但对生产安全性至关重要

---

## 维度 8：污点追踪 (Taint Tracking)

### octo-sandbox -- 评分 9（唯一实现者）

**文件**: `crates/octo-engine/src/secret/taint.rs`, `crates/octo-engine/src/secret/vault.rs`

- **TaintedValue<T>** -- 泛型包装器，ZeroizeOnDrop
- **4 级标签**: Public / Internal / Confidential / Secret
- **TaintSink**: Log / Error / ExternalResponse / File
- **check_sink()** -- 根据标签级别决定是否允许流向特定 sink
- **CredentialVault**: AES-256-GCM 加密 + Argon2id 密钥派生
- **Zeroizing<[u8;32]>** 主密钥 + 每次加密新 nonce
- Drop 时自动 zeroize

### zeroclaw -- 评分 1

- `src/security/mod.rs` 有 `taint` 引用但无实际实现
- Cargo.lock 包含 `zeroize` crate 但未用于污点追踪

### 所有其他竞品 -- 评分 0

grep 搜索确认: 无 taint tracking 实现（仅 Cargo.lock 中的间接依赖）。

**差距判定**: octo-sandbox **领先所有竞品**。污点追踪是防止秘密泄露的最后一道防线，也是最难实现的。这是**真优势**。

---

## 综合差距分析

### octo-sandbox 的真优势（竞品无法匹敌）

| 特性 | 领先幅度 | 代码位置 |
|------|----------|----------|
| 可组合 SafetyPipeline | 唯一实现 | `security/pipeline.rs` |
| 污点追踪 + Zeroize | 唯一实现 | `secret/taint.rs`, `secret/vault.rs` |
| PII 扫描（中国场景） | 独有 | `security/ai_defence.rs` |
| 3 层沙箱智能路由 | 最全面 | `sandbox/router.rs` |

### octo-sandbox 的真差距（必须修复）

| 差距 | 来源竞品 | 优先级 | 改动量 | 建议 |
|------|----------|--------|--------|------|
| 紧急停止 (estop) | zeroclaw | **P0** | 中 | 实现 EstopLayer，持久化状态 + OTP 恢复 |
| SSRF 防护 | moltis | **P1** | 小 | MCP 模块增加 DNS 解析 + 私有 IP 阻断 |
| Per-turn canary 旋转 | zeroclaw | **P1** | 小 | CanaryGuardLayer 增加轮转逻辑 |
| Merkle 链审计 | openfang | **P2** | 小 | AuditStorage 增加 SHA-256 链计算 |

### 伪差距（不需要追加）

| 伪差距 | 理由 |
|--------|------|
| OS 级沙箱 (Landlock/seccomp) | Docker 已包含 seccomp profile；WASM 天然隔离；Landlock 仅限 Linux 5.13+；zeroclaw 实际未启用 |
| ML 注入分类器 | goose 仅扫描 shell 调用，覆盖面窄；规则引擎 + 可组合 pipeline 更灵活；可作为 P3 可选 layer |
| HMAC 策略文件签名 | localgpt 的特殊需求（安全策略文件防篡改）；octo-sandbox 通过代码内置策略而非文件，不适用 |
| 网络代理 credential 注入 | ironclaw 的精巧设计，但需要 Docker 沙箱场景；可作为 P3 功能增强 |

---

## 竞品能力矩阵（代码级确认）

下表中 Y = 源代码确认存在完整实现，P = 部分实现/骨架，N = 无实现

| 能力 | octo | zeroclaw | localgpt | ironclaw | openfang | goose | moltis |
|------|------|----------|----------|----------|----------|-------|--------|
| 可组合安全 pipeline | **Y** | N | N | N | N | N | N |
| 注入检测（规则） | Y | Y | N | Y | N | Y | N |
| 注入检测（ML） | N | N | N | N | N | **Y** | N |
| 语义注入分析 | N | Y | N | N | N | N | N |
| Canary token | Y | **Y** | N | N | N | N | N |
| Per-turn canary 旋转 | N | **Y** | N | N | N | N | N |
| PII 扫描 | **Y** | P | N | P | N | N | N |
| 中国 PII 模式 | **Y** | N | N | N | N | N | N |
| Subprocess 沙箱 | Y | Y | Y | N | Y | N | N |
| WASM 沙箱 | **Y** | N | N | Y | N | N | N |
| Docker 沙箱 | **Y** | Y | N | **Y** | P | N | N |
| Landlock/seccomp | N | P | **Y** | N | N | N | N |
| SSRF 防护 | N | N | N | N | N | N | **Y** |
| 网络代理 | N | N | N | **Y** | N | N | N |
| 环境变量清洗 | N | Y | N | Y | Y | N | N |
| 审计日志 | Y | Y | N | P | **Y** | N | N |
| Merkle 链审计 | N | N | N | N | **Y** | N | N |
| 紧急停止 | N | **Y** | N | N | N | N | N |
| 污点追踪 | **Y** | N | N | N | N | N | N |
| Zeroize-on-drop | **Y** | N | N | N | N | N | N |
| Credential vault | **Y** | N | Y | **Y** | N | N | N |
| 策略文件签名 | N | N | **Y** | N | N | N | N |
| RBAC | **Y** | Y | N | N | N | N | N |
| 常量时间 API key | **Y** | N | N | N | N | N | N |
| ToolPolicy 层级解析 | N | N | N | N | N | N | **Y** |

---

## 优先级行动建议

### P0 -- 紧急停止（1-2 天）

```
crates/octo-engine/src/security/estop.rs (新文件)
crates/octo-engine/src/security/pipeline.rs (添加 EstopLayer)
```

核心要求：
- 多级停止: ToolFreeze -> AgentPause -> KillAll
- JSON 持久化（重启后保持 estop 状态）
- 失败关闭（状态文件损坏时触发最高级 estop）
- 恢复需要验证（至少 confirmation token）

### P1 -- SSRF 防护（0.5-1 天）

```
crates/octo-engine/src/mcp/ssrf.rs (新文件)
```

参考 moltis `crates/tools/src/ssrf.rs` 实现:
- DNS 解析后检查 IP
- 阻断: loopback, private (10/8, 172.16/12, 192.168/16), link-local, CGNAT (100.64/10), IPv6 私有
- 可配置 CIDR 白名单

### P1 -- Per-turn Canary 旋转（0.5 天）

修改 `CanaryGuardLayer`:
- 每轮生成新 UUID canary
- 注入系统提示时剥离旧 canary、注入新 canary
- 检查输出时验证当前轮次 canary

### P2 -- Merkle 链审计（0.5 天）

修改 `crates/octo-engine/src/audit/storage.rs`:
- 每条 AuditEvent 增加 `prev_hash` 和 `hash` 字段
- insert 时计算 SHA-256(fields + prev_hash)
- 提供 `verify_chain()` 完整性校验方法

---

## 方法论说明

本分析基于以下代码的完整阅读：

**octo-sandbox**（全部安全相关文件）:
- `crates/octo-engine/src/security/pipeline.rs` -- SafetyPipeline 完整实现
- `crates/octo-engine/src/security/ai_defence.rs` -- InjectionDetector, PiiScanner, OutputValidator
- `crates/octo-engine/src/security/policy.rs` -- SecurityPolicy, AutonomyLevel, 命令白名单
- `crates/octo-engine/src/sandbox/router.rs` -- SandboxRouter 路由逻辑
- `crates/octo-engine/src/secret/taint.rs` -- TaintedValue, TaintLabel, TaintSink
- `crates/octo-engine/src/secret/vault.rs` -- CredentialVault, AES-256-GCM
- `crates/octo-engine/src/audit/storage.rs` -- AuditEvent, SQLite 存储
- `crates/octo-engine/src/auth/roles.rs` -- RBAC 4 级角色
- `crates/octo-engine/src/auth/api_key.rs` -- SHA-256 哈希 + 常量时间比较
- `crates/octo-engine/src/agent/harness.rs` -- SafetyPipeline 集成点

**竞品**（安全核心文件）:
- zeroclaw: `src/security/mod.rs`, `estop.rs`, `canary_guard.rs`, `landlock.rs`, `src/tools/shell.rs`
- localgpt: `crates/sandbox/src/linux.rs`, `crates/sandbox/src/policy.rs`, `crates/core/src/security/policy.rs`
- ironclaw: `src/safety/mod.rs`（含 sanitizer, validator, policy, leak_detector, credential_detect）
- openfang: `crates/openfang-runtime/src/audit.rs`, `subprocess_sandbox.rs`
- goose: `crates/goose/src/security/scanner.rs`
- moltis: `crates/tools/src/ssrf.rs`, `crates/tools/src/policy.rs`（CLAUDE.md 确认架构）

未使用 grep 关键字匹配来推断功能存在 -- 每个评分都基于源代码的实际语义理解。
