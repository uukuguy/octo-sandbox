# EAASP v2.0 MVP — Phase 0 Infrastructure Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task.

**Goal:** 建立 EAASP v2.0 Infrastructure MVP —— 一个真实的"阈值校准助手"skill 能在 L4→L3→L2→L1 完整链路下跨 session 累加记忆，验证圈 2 能力（契约治理核心 + 资产记忆基础）。

**Supersedes:** `docs/plans/archive/2026-04-10-eaasp-m1-phase0-scaffold.md`（已归档）

**Authoritative References:**
- `docs/design/EAASP/EAASP-Design-Specification-v2.0.docx` — v2.0 权威规范
- `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md` — 长期演化路径与决策注册表
- `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` — 本 Phase 的范围定义（圈 2 + 前期资产评估）

**Tech Stack:**
- Proto: protobuf 3
- Rust: 1.75+, Tokio, Axum, tonic, rusqlite
- Python: 3.12+, FastAPI, typer, grpcio, uv, pydantic v2

**Verification Criteria (exit gate):** 见 `EAASP_v2_0_MVP_SCOPE.md` §8 的 15 条 E2E 断言。

---

## 前置条件

- [x] EAASP v2.0 设计规范已完成（`.docx` 在 `docs/design/EAASP/`）
- [x] v2.0 Evolution Path 文档已编写（决策 D1-D12 锁定）
- [x] v2.0 MVP Scope 文档已编写（14 项资产评估完成）
- [x] `docs/design/EAASP/` 目录已建立，v1.7/v1.8 文档已归档
- [ ] 本 plan 审核通过

---

## 任务分组

Phase 0 分为 4 个 Stage，Stage 内任务可并行，Stage 之间串行：

| Stage | 任务数 | 主题 | 关键产出 |
|---|---|---|---|
| **S1. Foundation** | 3 | proto + 归档 + 新目录骨架 | v2 proto 定稿、旧 tool 归档 |
| **S2. L1 Runtime Refactor** | 4 | 运行时层重构到 v2 契约 | grid-runtime / hermes / claude-code-runtime / certifier 全部按 v2 编译通过 |
| **S3. L2-L3-L4 Build** | 5 | 资产/治理/编排层新建 | memory-engine / l3-governance / l4-orchestration / skill-registry 扩展 / cli-v2 |
| **S4. E2E Integration** | 3 | 阈值校准 skill + 端到端跑通 | skill.md + 集成测试 + verify-v2-mvp target |

**总计：15 个任务**

---

## Stage S1 — Foundation（奠基）

### S1.T1: 归档旧工具 & 建立新目录骨架

**Goal:** 按 MVP_SCOPE §3 的判决表，把 SCRAP 组件归档，按 v2 建立新目录骨架（只建空壳和 README，避免大量空文件）。

**Files:**
- Move: `tools/eaasp-governance/` → `tools/archive/v1.8/eaasp-governance/`
- Move: `tools/eaasp-session-manager/` → `tools/archive/v1.8/eaasp-session-manager/`
- Create: `tools/eaasp-l3-governance/{README.md,pyproject.toml}`
- Create: `tools/eaasp-l4-orchestration/{README.md,pyproject.toml}`
- Create: `tools/eaasp-l2-memory-engine/{README.md,pyproject.toml}`
- Create: `tools/eaasp-cli-v2/{README.md,pyproject.toml}`
- Create: `proto/eaasp/runtime/v2/` 目录
- Modify: `Makefile` — 移除已归档 tool 的 target
- Modify: `Cargo.toml` workspace 成员（如有 eaasp-governance 的 cargo member 引用）

**Steps:**

1. 执行归档：
   ```bash
   mkdir -p tools/archive/v1.8/
   git mv tools/eaasp-governance/ tools/archive/v1.8/eaasp-governance/
   git mv tools/eaasp-session-manager/ tools/archive/v1.8/eaasp-session-manager/
   ```

2. 建立新目录骨架（每个目录只建 README.md + pyproject.toml 两个文件）：
   ```bash
   mkdir -p tools/eaasp-l3-governance/src tools/eaasp-l3-governance/tests
   mkdir -p tools/eaasp-l4-orchestration/src tools/eaasp-l4-orchestration/tests
   mkdir -p tools/eaasp-l2-memory-engine/src tools/eaasp-l2-memory-engine/tests
   mkdir -p tools/eaasp-cli-v2/src tools/eaasp-cli-v2/tests
   mkdir -p proto/eaasp/runtime/v2
   ```

3. 每个新目录写 README.md，内容引用 `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` §3.3。

4. 每个新 Python 工具建最小 `pyproject.toml`（name, version, python >=3.12, deps 空）。

5. 移除 `tools/eaasp-governance/` 和 `tools/eaasp-session-manager/` 在 `Makefile` 里的 target 引用。

**Verification:**
- `git status` 显示归档 + 新目录创建
- `ls tools/` 显示 7 个活跃 eaasp-* 目录（3 keep + 4 new）和 archive/
- `make --dry-run verify` 不再引用已归档目录

---

### S1.T2: v2 proto 定稿

**Goal:** 创建 `proto/eaasp/runtime/v2/runtime.proto`，定义 16 方法契约、结构化 SessionPayload、14 hook event、EmitEvent 占位。

**Files:**
- Create: `proto/eaasp/runtime/v2/runtime.proto`
- Create: `proto/eaasp/runtime/v2/hook.proto`（保留 Phase BE W2 的 bidirectional streaming）
- Create: `proto/eaasp/runtime/v2/common.proto`（共享 message）
- Create: `proto/eaasp/runtime/v2/README.md` 说明 v1 → v2 差异

**Steps:**

1. 设计 `common.proto`：
   ```proto
   message EvidenceAnchor { string anchor_id = 1; string data_ref = 2; string snapshot_hash = 3; ... }
   message MemoryRef { string memory_id = 1; string memory_type = 2; double relevance_score = 3; ... }
   message EventContext { string event_id = 1; string event_type = 2; string severity = 3; ... }
   message PolicyContext { repeated ManagedHook hooks = 1; string org_unit = 2; ... }
   message SkillInstructions { string skill_id = 1; string content = 2; repeated ScopedHook frontmatter_hooks = 3; ... }
   message UserPreferences { string user_id = 1; map<string, string> prefs = 2; ... }

   // v2.0 §8.6 structured SessionPayload with priority blocks
   message SessionPayload {
     PolicyContext policy_context = 1;      // P1 — never removable
     EventContext event_context = 2;        // P2
     repeated MemoryRef memory_refs = 3;    // P3
     SkillInstructions skill_instructions = 4;  // P4
     UserPreferences user_preferences = 5;  // P5 — removable first
     // helper flags
     bool allow_trim_p5 = 10;
     bool allow_trim_p4 = 11;  // default false
     bool allow_trim_p3 = 12;  // default false
   }
   ```

2. 设计 `runtime.proto`（16 方法 = 12 MUST + 4 Optional）：
   ```proto
   service RuntimeService {
     // === 12 MUST methods (certified core) ===
     rpc Initialize(InitializeRequest) returns (InitializeResponse);
     rpc Send(SendRequest) returns (stream SendResponse);
     rpc LoadSkill(LoadSkillRequest) returns (LoadSkillResponse);
     rpc OnToolCall(ToolCallEvent) returns (ToolCallAck);
     rpc OnToolResult(ToolResultEvent) returns (ToolResultAck);
     rpc OnStop(StopEvent) returns (StopAck);
     rpc GetState(Empty) returns (StateResponse);
     rpc ConnectMCP(ConnectMCPRequest) returns (ConnectMCPResponse);
     rpc EmitTelemetry(TelemetryRequest) returns (Empty);
     rpc GetCapabilities(Empty) returns (Capabilities);
     rpc Terminate(Empty) returns (Empty);
     rpc RestoreState(StateResponse) returns (Empty);  // SHOULD per spec

     // === 4 Optional methods ===
     rpc Health(Empty) returns (HealthResponse);
     rpc DisconnectMcp(DisconnectMcpRequest) returns (Empty);
     rpc PauseSession(Empty) returns (StateResponse);
     rpc ResumeSession(StateResponse) returns (Empty);

     // === Open placeholder (ADR-V2-001 pending) ===
     rpc EmitEvent(EventStreamEntry) returns (Empty);  // PLACEHOLDER
   }

   // Capabilities 带 credential_mode
   message Capabilities {
     string runtime_id = 1;
     string model = 2;
     int32 context_window = 3;
     repeated string tools = 4;
     bool supports_native_hooks = 5;
     bool supports_native_mcp = 6;
     bool supports_native_skills = 7;
     double cost_per_1k_tokens = 8;
     enum CredentialMode {
       DIRECT = 0;
       PROXY = 1;
       BRIDGE_INJECTED = 2;
     }
     CredentialMode credential_mode = 9;
     repeated string strengths = 10;
     repeated string limitations = 11;
   }

   // 14 lifecycle hook event types
   enum HookEventType {
     // L1 (9)
     SESSION_START = 0;
     USER_PROMPT_SUBMIT = 1;
     PRE_TOOL_USE = 2;
     POST_TOOL_USE = 3;
     POST_TOOL_USE_FAILURE = 4;
     PERMISSION_REQUEST = 5;
     STOP = 6;
     SUBAGENT_STOP = 7;
     PRE_COMPACT = 8;
     // L3 (2)
     PRE_POLICY_DEPLOY = 9;
     PRE_APPROVAL = 10;
     // L4 (3)
     EVENT_RECEIVED = 11;
     PRE_SESSION_CREATE = 12;
     POST_SESSION_END = 13;
   }
   ```

3. `hook.proto`：保留原 Phase BE W2 的 bidirectional streaming（`grid-hook-bridge` 已经用）。

4. 写 `proto/eaasp/runtime/v2/README.md`：
   - 明确 v1 已作废
   - 标注 12 MUST / 4 Optional / 1 Placeholder（`EmitEvent` ADR-V2-001）
   - 指向 `docs/design/EAASP/EAASP-Design-Specification-v2.0.docx` §8

**Verification:**
- `protoc --proto_path=proto proto/eaasp/runtime/v2/runtime.proto --python_out=/tmp/v2_check` 无错误
- Rust 侧用 `tonic-build` 在一个临时 bin 里编译 v2 proto 成功
- README 被 grep 到 "v1 作废"、"ADR-V2-001"

---

### S1.T3: 删除旧 proto v1 + 更新 Cargo.toml/workspace 引用

**Goal:** 删除 `proto/eaasp/runtime/v1/runtime.proto`，让任何对 v1 的引用都编译失败（暴露需要改的位置）。

**Files:**
- Delete: `proto/eaasp/runtime/v1/runtime.proto`（保留目录壳以便 git 记录，或整目录删）
- Modify: 各 crate 的 `build.rs` 里 proto 路径（从 v1 改为 v2）
- Modify: `crates/grid-runtime/Cargo.toml` 和 `tools/eaasp-certifier/Cargo.toml` 里的 `tonic-build` 配置

**Steps:**

1. `git rm proto/eaasp/runtime/v1/runtime.proto`（如果目录为空一起删）

2. 找出所有引用 v1 proto 的地方：
   ```bash
   grep -rn "eaasp/runtime/v1" crates/ tools/ lang/ --include="*.rs" --include="*.py" --include="*.toml"
   ```

3. 在 `crates/grid-runtime/build.rs` 把路径从 `eaasp/runtime/v1/runtime.proto` 改为 `eaasp/runtime/v2/runtime.proto`。

4. 同样改 `tools/eaasp-certifier/build.rs`。

5. 在 lang/*runtime*/ 里的 proto 编译脚本（`build_proto.py` 之类）改路径。

6. **不**试图让整个 workspace 编译通过 — 编译失败是预期的，暴露 R1/R2/R3/R4 需要修的位置。在 T1/T2 继续时才修。

**Verification:**
- `grep -rn "eaasp/runtime/v1" crates/ tools/ lang/` 无匹配（除文档）
- `cargo check -p grid-runtime` 失败（预期），失败原因是 v2 proto 的新 struct/field 在代码里不存在
- 失败的 error 列表作为 S2 的输入

---

## Stage S2 — L1 Runtime Refactor（契约层重构）

### S2.T1: `grid-runtime` 重构到 v2 契约

**Goal:** 让 `crates/grid-runtime/` 基于 v2 proto 编译通过、原 37 tests 重构后至少 60% 通过。

**Files:**
- Modify: `crates/grid-runtime/src/contract.rs` — 按 v2 proto 重写
- Modify: `crates/grid-runtime/src/service.rs` — 适配新 RuntimeService trait
- Modify: `crates/grid-runtime/src/harness.rs` — SessionPayload 结构化处理
- Modify: `crates/grid-runtime/tests/*.rs` — 断言更新
- Create: `crates/grid-runtime/tests/v2_session_payload_test.rs` — 新增 P1-P5 priority block 测试

**Steps:**

1. 重写 `contract.rs`：`SessionPayload` struct 从扁平字段改为 5 个嵌套 priority block struct（每个字段一个 `Option<>`，P5 默认 `Some`）。

2. 新增 `session_payload.rs` helper：
   ```rust
   impl SessionPayload {
     pub fn trim_for_budget(&mut self, budget_tokens: usize) -> &mut Self {
       // P5 → P4 → P3 顺序裁剪，P1/P2 永不裁剪
     }
   }
   ```

3. 新增 9 个 L1 HookEventType 的枚举和触发点（原代码可能只触发了部分）。

4. 在 `RuntimeService` trait 实现里加 `EmitEvent` 占位方法（返回 Unimplemented）。

5. 测试更新策略：
   - 先把所有旧测试打 `#[ignore]`
   - 逐个取消 ignore，改断言，直到至少 60% 通过
   - 新增 3-5 个 v2 专用测试（SessionPayload priority、P1 never removable、deny-always-wins between managed and frontmatter scope）

**Verification:**
- `cargo check -p grid-runtime` 通过
- `cargo test -p grid-runtime -- --test-threads=1` 通过率 ≥ 60%
- `grep -c "emit_event" crates/grid-runtime/src/` ≥ 1

---

### S2.T2: `eaasp-certifier` 重构到 v2，只验 12 MUST

**Goal:** 让 certifier 明确标注 12 MUST / 4 Optional，按 v2 契约测试。

**Files:**
- Modify: `tools/eaasp-certifier/src/checks/` 下所有 check 函数 — 标注 `is_must: bool`
- Create: `tools/eaasp-certifier/src/v2_must_methods.rs` — 常量列出 12 MUST 方法名
- Modify: `tools/eaasp-certifier/src/main.rs` — 输出报告时明确标注哪些是 MUST

**Steps:**

1. 新建 `v2_must_methods.rs`：
   ```rust
   pub const MUST_METHODS: &[&str] = &[
     "initialize", "send", "loadSkill", "onToolCall", "onToolResult",
     "onStop", "getState", "connectMCP", "emitTelemetry", "getCapabilities",
     "terminate", "restoreState",
   ];
   pub const OPTIONAL_METHODS: &[&str] = &[
     "health", "disconnectMcp", "pauseSession", "resumeSession",
   ];
   ```

2. 每个 check 函数加 `fn is_must(&self) -> bool` 方法；optional 方法不存在时不 fail，只 warn。

3. 报告输出格式：
   ```
   == Certification Report for grid-runtime ==
   MUST methods: 12/12 PASS
   OPTIONAL methods: 4/4 present (bonus)
   EmitEvent placeholder: present (ADR-V2-001 pending)
   PASS
   ```

4. 测试更新：把现有测试按 MUST/Optional 分开。

**Verification:**
- `cargo test -p eaasp-certifier` 通过
- `cargo run -p eaasp-certifier -- --runtime grid-runtime` 输出包含 "12/12 PASS"

---

### S2.T3: `hermes-runtime-python` 重构到 v2 契约

**Goal:** Python T2 Aligned runtime 能加载 v2 proto stubs，跑通最小 session。

**Files:**
- Modify: `lang/hermes-runtime-python/build_proto.py` — 从 v2 proto 生成 stub
- Modify: `lang/hermes-runtime-python/src/hermes_runtime/service.py` — 实现 16 方法 + SessionPayload 处理
- Modify: `lang/hermes-runtime-python/src/hermes_runtime/session_manager.py`
- Modify: `lang/hermes-runtime-python/tests/` — 测试更新

**Steps:**

1. 改 `build_proto.py` 里的 proto 路径引用。

2. 重写 `service.py` 的 `RuntimeService` impl：16 方法桩，SessionPayload 用 pydantic model 适配。

3. 保留 `HookBridge monkey-patch hermes.handle_function_call` 这个关键 trick（已验证过）。

4. 测试更新策略同 grid-runtime。

**Verification:**
- `make hermes-runtime-test` 通过率 ≥ 60%
- `make hermes-runtime-start` 能启动 gRPC server on :50051

---

### S2.T4: `claude-code-runtime-python` 重构到 v2 契约

**Goal:** Python T1 Harness runtime 能加载 v2 proto stubs，跑通最小 session。

**Files:** 同 S2.T3 的 pattern，改 `lang/claude-code-runtime-python/`

**Steps:**

1. 重新生成 python stubs from v2 proto。

2. `claude_code_runtime/service.py` 的 16 方法重写。

3. Skill loader 支持 frontmatter scoped hook 激活（这是 T1 的原生能力，要用上）。

4. `sdk_wrapper.py` 里 claude-agent-sdk 调用方式保留（spawn Claude Code CLI）。

5. 测试：原 55 tests 的重构，目标至少 60% 通过。

**Verification:**
- `make claude-runtime-test` 通过率 ≥ 60%
- `make claude-runtime-start` 启动 :50052

---

## Stage S3 — L2/L3/L4 Build（构建资产/治理/编排层）

### S3.T1: `eaasp-skill-registry` 扩展到 v2 skill schema

**Goal:** 让现有的 L2 skill repo 支持 v2 skill frontmatter（scoped hooks + runtime affinity + organizational scope）。

**Files:**
- Modify: `tools/eaasp-skill-registry/src/skill_parser.rs` — 解析 YAML frontmatter 的 scoped hooks
- Modify: `tools/eaasp-skill-registry/src/models.rs` — Skill struct 增加字段
- Modify: `tools/eaasp-skill-registry/src/api.rs` — 暴露 7 个 MCP tools
- Create: `tools/eaasp-skill-registry/tests/v2_frontmatter_test.rs`

**Steps:**

1. Skill frontmatter 新 schema：
   ```yaml
   ---
   name: Threshold Calibration Assistant
   version: 1.0.0
   author: ops-team
   runtime_affinity:
     preferred: null  # 无 affinity，最可移植
     compatible: [grid-runtime, claude-code-runtime, hermes-runtime]
   access_scope: enterprise
   scoped_hooks:
     PreToolUse:
       - name: block_write_scada
         type: command
         command: "scripts/hooks/block_write_scada.sh"
     PostToolUse:
       - name: require_evidence
         type: prompt
         prompt: "Does the tool output include an evidence_anchor_id reference?"
     Stop:
       - name: require_anchor_in_output
         type: command
         command: "scripts/hooks/check_output_anchor.sh"
   dependencies: []
   ---
   ```

2. 7 个 MCP tools 实现：skill_search, skill_read, skill_list_versions, skill_submit_draft, skill_promote, skill_dependencies, skill_usage（最后一个 stub 即可）。

3. 测试新增：提交一个带 frontmatter 的 skill，能正确解析 hooks 数组并在 `skill_read` 返回。

**Verification:**
- `cargo test -p eaasp-skill-registry` 全部通过
- 启动 server 后 `curl localhost:8081/tools` 返回 7 个 tool names

---

### S3.T2: 新建 `eaasp-l2-memory-engine`

**Goal:** 从零构建 L2 Memory Engine，三层存储最小版 + 6 MCP tools。

**Files:**
- Create: `tools/eaasp-l2-memory-engine/src/main.py` — FastAPI app
- Create: `tools/eaasp-l2-memory-engine/src/anchors.py` — Layer 1 evidence anchor store (SQLite append-only)
- Create: `tools/eaasp-l2-memory-engine/src/files.py` — Layer 2 memory files (SQLite + JSON content)
- Create: `tools/eaasp-l2-memory-engine/src/index.py` — Layer 3 hybrid index (keyword + time-decay, SQLite FTS5)
- Create: `tools/eaasp-l2-memory-engine/src/mcp_tools.py` — 6 MCP tools
- Create: `tools/eaasp-l2-memory-engine/src/api.py` — REST API `POST /api/v1/memory/search` + `GET /api/v1/memory/anchors`
- Create: `tools/eaasp-l2-memory-engine/tests/test_*.py`

**Steps:**

1. SQLite schema：
   ```sql
   CREATE TABLE anchors (
     anchor_id TEXT PRIMARY KEY,
     event_id TEXT, session_id TEXT,
     type TEXT, data_ref TEXT, snapshot_hash TEXT,
     source_system TEXT, tool_version TEXT, model_version TEXT, rule_version TEXT,
     created_at INTEGER, metadata JSON
   );
   CREATE TABLE memory_files (
     memory_id TEXT PRIMARY KEY,
     scope TEXT, category TEXT, content TEXT,
     evidence_refs JSON, status TEXT, version INTEGER,
     created_at INTEGER, updated_at INTEGER
   );
   CREATE VIRTUAL TABLE memory_fts USING fts5(memory_id, content_text, category, scope);
   ```

2. 6 MCP tools：memory_search (hybrid keyword + time-decay), memory_read, memory_write_anchor (append-only), memory_write_file (new version), memory_list, memory_archive (status → archived)。

3. REST API：
   - `POST /api/v1/memory/search` — L4 context assembly 用
   - `GET /api/v1/memory/anchors?event_id=X` — evidence chain 追溯

4. 写 pytest 测试：
   - 写入 anchor → 读回
   - 写入 memory file → 搜索能命中
   - Status transition (agent_suggested → confirmed → archived)

**Verification:**
- `cd tools/eaasp-l2-memory-engine && uv sync && pytest` 全部通过
- 启动后 `curl -X POST localhost:8085/api/v1/memory/search -d '{"query":"threshold"}'` 返回 JSON

---

### S3.T3: 新建 `eaasp-l3-governance`

**Goal:** Thin L3 — Policy Engine + Managed-Settings 编译部署 + Audit Service。

**Files:**
- Create: `tools/eaasp-l3-governance/src/main.py`
- Create: `tools/eaasp-l3-governance/src/policy_engine.py` — 策略存储 + 编译
- Create: `tools/eaasp-l3-governance/src/managed_settings.py` — managed-settings.json 构建与原子下发
- Create: `tools/eaasp-l3-governance/src/audit.py` — Audit service (receives async PostToolUse HTTP)
- Create: `tools/eaasp-l3-governance/src/api.py` — 实现 Contract 1 (Policy Deployment) + Contract 4 (Telemetry Ingest) + Contract 5 部分 (Session Control 三向握手中的 validate hook attach 步骤)
- Create: `tools/eaasp-l3-governance/tests/`

**Steps:**

1. Contract 1 endpoints:
   - `PUT /v1/policies/managed-hooks` — 接收编译好的 managed-settings.json（MVP 不做 compile，接收已 JSON 化的配置）
   - `PUT /v1/policies/{hook_id}/mode` — 切 enforce/shadow
   - `GET /v1/policies/versions`

2. Contract 4 endpoints:
   - `POST /v1/telemetry/events` — 接收 async PostToolUse HTTP hook 的 payload，存 SQLite

3. Contract 5 部分 — L3 对 session create 的校验：
   - `POST /v1/sessions/{id}/validate` — 给 session 绑定 managed hooks，返回要 attach 的 hook 列表

4. **不做**：approval gate、OPA 后端、evidence chain manager（都推迟到 Phase 3）

5. 测试：部署一个 hook → 查询 versions → 切 mode → 记录 telemetry → 能查回。

**Verification:**
- `cd tools/eaasp-l3-governance && uv sync && pytest` 全部通过
- `curl` 能走通 4 个 endpoint

---

### S3.T4: 新建 `eaasp-l4-orchestration`

**Goal:** L4 的最小 orchestrator — session orchestrator + L4 → L2 context assembly + 三向握手发起方 + Session Event Stream 占位。

**Files:**
- Create: `tools/eaasp-l4-orchestration/src/main.py`
- Create: `tools/eaasp-l4-orchestration/src/session_orchestrator.py`
- Create: `tools/eaasp-l4-orchestration/src/handshake.py` — 三向握手客户端
- Create: `tools/eaasp-l4-orchestration/src/context_assembly.py` — 调 L2 memory search 装配 P3
- Create: `tools/eaasp-l4-orchestration/src/event_stream.py` — **SQLite append-only 占位**（接口按 v2.0 §5.5，实现最简）
- Create: `tools/eaasp-l4-orchestration/src/api.py` — Contract 2 (Intent Gateway 最小版) + Contract 5 (Session Control)
- Create: `tools/eaasp-l4-orchestration/tests/`

**Steps:**

1. Contract 2 minimal:
   - `POST /v1/intents/dispatch` — 接收 user intent → 解析 skill_id + runtime_pref → 调 handshake

2. Contract 5:
   - `POST /v1/sessions/create` — 完整三向握手：
     1. 调 L2 memory search (`context_assembly.py`)
     2. 调 L3 `/v1/sessions/{id}/validate`
     3. 调 L1 runtime `Initialize` (gRPC)
     4. 组装结构化 SessionPayload（P1-P5）
     5. 创建 Session Event Stream 记录
     6. 返回 session handle
   - `POST /v1/sessions/{id}/message` — 向 runtime `Send`
   - `GET /v1/sessions/{id}/events` — 读 event stream

3. `event_stream.py` 用 SQLite 的 `session_events` append-only 表：
   ```sql
   CREATE TABLE session_events (
     seq INTEGER PRIMARY KEY AUTOINCREMENT,
     session_id TEXT, event_type TEXT, payload JSON,
     created_at INTEGER
   );
   CREATE INDEX idx_session_seq ON session_events(session_id, seq);
   ```
   接口方法：`append(session_id, event)`, `get_events(session_id, from, to)`。

4. 测试：集成 mock L3 + mock L1 runtime，跑通三向握手完整序列。

**Verification:**
- `pytest tools/eaasp-l4-orchestration/tests/` 通过
- 对 mock L1 runtime 的 `Initialize` 被调用过，且 payload 里有 P1-P5 结构

---

### S3.T5: 新建 `eaasp-cli-v2`（L5 模拟器）

**Goal:** 按 D8 创建 CLI，模拟 L5 portal 的所有关键命令，确保上层 UI 缺席时基座依然完整可用。

**Files:**
- Create: `tools/eaasp-cli-v2/src/eaasp_cli/main.py` — typer app
- Create: `tools/eaasp-cli-v2/src/eaasp_cli/cmd_session.py` — session create/list/show/send
- Create: `tools/eaasp-cli-v2/src/eaasp_cli/cmd_memory.py` — memory search/read/list
- Create: `tools/eaasp-cli-v2/src/eaasp_cli/cmd_skill.py` — skill list/run/promote/submit
- Create: `tools/eaasp-cli-v2/src/eaasp_cli/cmd_policy.py` — policy deploy/mode
- Create: `tools/eaasp-cli-v2/src/eaasp_cli/config.py` — 各服务 endpoint 地址（默认 localhost:8081-8085）
- Create: `tools/eaasp-cli-v2/tests/`

**Steps:**

1. Typer 命令树：
   ```
   eaasp
   ├── session
   │   ├── create --skill <id> --runtime <id>
   │   ├── list
   │   ├── show <id>        # 打印 4 卡数据（event card + evidence pack）
   │   └── send <id> <msg>  # 流式打印响应
   ├── memory
   │   ├── search <query>
   │   ├── read <memory_id>
   │   └── list
   ├── skill
   │   ├── list
   │   ├── submit <path>
   │   ├── promote <id> <stage>
   │   └── run <id>         # syntactic sugar: create session + send "run skill"
   └── policy
       ├── deploy <config.json>
       └── mode <hook_id> <enforce|shadow>
   ```

2. 对每个服务用 httpx/grpc client。

3. `session show` 在 MVP 里只渲染 event card（title/severity/summary）和 evidence pack（list of anchor_ids），action/approval card 留 TODO。

4. 测试：e2e smoke — 启动所有 4 个后端 + 1 个 runtime，跑完整命令序列。

**Verification:**
- `uv run eaasp --help` 显示所有命令
- `uv run eaasp session create --skill threshold-calibration --runtime grid-runtime` 能触发完整三向握手

---

## Stage S4 — E2E Integration（阈值校准 + 跑通）

### S4.T1: 编写"阈值校准助手"skill

**Goal:** 写一个真实的 workflow-skill，覆盖 MVP_SCOPE §4 的业务描述。

**Files:**
- Create: `examples/skills/threshold-calibration/SKILL.md`
- Create: `examples/skills/threshold-calibration/hooks/block_write_scada.sh`
- Create: `examples/skills/threshold-calibration/hooks/check_output_anchor.sh`
- Create: `examples/skills/threshold-calibration/mock-scada.py` — mock MCP server 提供 read-only SCADA 数据

**Steps:**

1. `SKILL.md` 内容：
   - frontmatter: scoped hooks (PreToolUse block_write_scada + PostToolUse require_evidence prompt + Stop require_anchor)
   - prose:
     ```
     ## Task
     You are a threshold calibration assistant for power grid equipment.
     When asked to calibrate thresholds for a device:
     1. Call `scada_read_snapshot(device_id, time_window)` to fetch latest data
     2. Call `memory_search(category="threshold_calibration", device_id=...)` to fetch prior suggestions
     3. If prior suggestions exist:
        a. Compare new snapshot against prior suggestions' baselines
        b. Either confirm the prior suggestion or propose a revision
     4. Write evidence anchor for the snapshot via `memory_write_anchor`
     5. Write or update memory file via `memory_write_file` with status=agent_suggested
     6. Your final output JSON MUST include field `evidence_anchor_id`
     ```

2. `block_write_scada.sh`:
   ```bash
   #!/bin/bash
   # Read JSON from stdin, deny if tool is scada_write
   tool=$(jq -r .tool_name)
   if [[ "$tool" == scada_write* ]]; then
     echo '{"decision":"deny","reason":"SCADA write not allowed in threshold-calibration skill"}'
     exit 2
   fi
   echo '{"decision":"allow"}'
   ```

3. `check_output_anchor.sh`:
   ```bash
   #!/bin/bash
   # Stop hook: check final output contains evidence_anchor_id
   output=$(jq -r .output)
   if echo "$output" | jq -e '.evidence_anchor_id' >/dev/null; then
     echo '{"decision":"allow"}'
   else
     echo '{"decision":"continue","reason":"Output missing evidence_anchor_id; please add reference"}'
     exit 2
   fi
   ```

4. `mock-scada.py` — 最简单的 Python MCP server 提供 2 个 tool：
   - `scada_read_snapshot(device_id, time_window)` → 返回假数据（temperature/load/doa_h2）
   - `scada_write(...)` → 永远不应被调用（测试 hook 拦截）

**Verification:**
- SKILL.md 能被 `eaasp-skill-registry` 正确解析（frontmatter 3 个 hook 声明）
- mock-scada 启动后能被 MCP 客户端连接

---

### S4.T2: 端到端集成测试脚本 `verify-v2-mvp`

**Goal:** 写一个自动化脚本，跑完 `MVP_SCOPE §8` 的 15 条断言。

**Files:**
- Create: `scripts/verify-v2-mvp.sh`
- Create: `scripts/verify-v2-mvp.py` — 复杂断言用 Python
- Modify: `Makefile` — 加 `v2-mvp-e2e` target

**Steps:**

1. Shell 脚本编排：启动 4 个 Python 服务 + 2 个 runtime + mock-scada，跑命令序列，在 trap 里清理。

2. Python 脚本执行 15 条断言：
   - `anchors` 表里有新插入
   - `memory_files` 有新插入且 `status=agent_suggested`
   - 第二次 session 的 Initialize payload 反序列化后 P3 非空且 memory_id 匹配第一次写入的
   - L3 `session_events` 有两次 session 的 tool call records
   - certifier 对两个 runtime 都报 PASS

3. `Makefile`:
   ```makefile
   v2-mvp-e2e:
       @bash scripts/verify-v2-mvp.sh
   ```

**Verification:**
- `make v2-mvp-e2e` 退出码 0
- 脚本输出 "✓ 15/15 assertions passed"

---

### S4.T3: 文档收尾 + checkpoint 更新

**Goal:** MVP 完成后更新所有文档和 checkpoint，让 Phase 1 能直接 resume。

**Files:**
- Modify: `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md` — Phase 0 标为 🟢 Completed
- Modify: `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` — 勾选全部 Verification Criteria
- Modify: `docs/plans/.checkpoint.json` — phase=phase-0-done, 记录 tests 数
- Modify: `CLAUDE.md`（可选）— 更新 "Complete" 列表
- Create: `docs/design/EAASP/adrs/ADR-V2-001-emit-event-method.md`（**如果** Phase 0 期间该 ADR 被触发需要）
- Create: `docs/work-logs/2026-04-11-v2-mvp-phase0-summary.md` — 工作摘要

**Verification:**
- 下次 `/resume-plan` 读取 checkpoint 后应显示 "Phase 0 complete, ready to start Phase 1"
- EVOLUTION_PATH 的 Phase 表第三列 "资产状态" Phase 0 变成 🟢 Completed

---

## 退出条件（Phase 0 DONE）

- [ ] Stage S1-S4 所有 15 个任务全部 DONE
- [ ] `make v2-mvp-e2e` 返回 0
- [ ] 两个 L1 runtime 通过 certifier
- [ ] EVOLUTION_PATH 文档 Phase 0 状态 = 🟢 Completed
- [ ] `.checkpoint.json` phase = `phase-0-done`
- [ ] Git commit "docs(eaasp): Phase 0 MVP complete"

完成后进入 **Phase 1: Event-driven foundation**（先解 ADR-V2-001/002/003，再动手）。
