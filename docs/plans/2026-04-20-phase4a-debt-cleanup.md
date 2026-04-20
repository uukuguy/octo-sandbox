# Phase 4a — Pre-Phase-4 Debt Cleanup (D148/D149/D151–D155)

**Status**: 🟢 active
**Started**: 2026-04-20
**Theme**: 清 7 项 Deferred —— Phase 3.6 review 产生的 5 项 tech-debt + Phase 3.5 遗留的 2 项 P1-active。Phase 4 范围对话前把 debt 水位归零。
**Estimated duration**: 2–3 days（10–22 h wall-clock）

## Goal

Phase 3.6 落地时 reviewer 把 5 项非阻塞改进转成 Deferred（D151–D155）；Phase 3.5 同时留下 D148 / D149 两项 P1-active。本阶段把这 7 项集中清掉，给 Phase 4 一个干净起点。

所有任务都是 review-confirmed improvements 或 test-density 补齐 —— 无架构决策、无契约变化、风险全部 low。

## Scope

| # | Task | ID | Class | Est | Owner |
|---|------|----|-------|-----|-------|
| T1 | `harness_envelope_wiring_test.rs` — spy HookHandler/StopHook 断言 `ctx.event` 匹配 PreToolUse/PostToolUse/Stop | D151 | 🧹 tech-debt | 1–2 h | Rust |
| T2 | `pyrightconfig.json` per-env `pythonVersion` 统一为 `"3.12"`（pyproject `requires-python>=3.12` floor） | D154 | 🧹 tech-debt | 15 m | Config |
| T3 | `scripts/check-pyright-prereqs.sh` 预检 9 个 `.venv` + CLAUDE.md 一行说明 | D155 | 🧹 tech-debt | 30 m | Scripts |
| T4 | `scripts/gen_runtime_proto.py` 加 `--out-dir` override flag + `lang/claude-code-runtime-python/Dockerfile` 去 symlink | D153 | 🧹 tech-debt | 30 m | Python/Docker |
| T5 | `lang/ccb-runtime-ts/src/proto/types.ts` SoT 同步保障 —— 评估 `@bufbuild/protoc-gen-es` vs CI grep guard，落地较简方案 | D149 | 🟡 P1-active | 2–4 h | TS/CI |
| T6 | pydantic-ai-runtime test bench 加厚 —— `sdk_wrapper` + agent loop 覆盖，目标 ≥12 tests（从 4 起步） | D148 | 🟡 P1-active | 4–8 h | Python |
| T7 | grpcio-tools int-accepting stubs 上游跟踪决策 —— wait upstream（ledger 注记）vs post-process `.pyi` 脚本（嵌 `gen_runtime_proto.py`） | D152 | 🧹 tech-debt | 2–6 h | Python |

## Task Breakdown

### T1 — D151: harness envelope call-site wiring regression test

**Why**: Phase 3.6 T1 code review 指出 `hook_envelope_parity_test.rs` 只锁 serializer，不锁 call-site wiring；D136 xfail mask 会掩盖 `.with_event(...)` 被误删的回归。

**Action**: 在 `crates/grid-engine/tests/` 加 `harness_envelope_wiring_test.rs`：
1. 实现 spy `HookHandler` 捕获收到的 `HookContext`（记在 `Arc<Mutex<Vec<HookContext>>>` 里）。
2. 实现 spy `StopHook`（实现 `StopHook` trait）同样捕获。
3. 写 3 个 test：
   - PreToolUse dispatch → 断言 `ctx.event == Some("PreToolUse".into())`
   - PostToolUse dispatch → 断言 `ctx.event == Some("PostToolUse".into())` + `tool_result` + `is_error` populated
   - Stop dispatch → 断言 `ctx.event == Some("Stop".into())` + `draft_memory_id` / `evidence_anchor_id` 是 Some("") 或 Some(id)
4. 跑真实 harness / AgentLoop 或构造最小 mock —— 读 `hook_envelope_parity_test.rs` 取 fixture 风格。

**Sign-off**: 3 新 test PASS；故意把 `.with_event(...)` 删掉一个，其中至少一个测试 fail（手工验证，不提交）。

---

### T2 — D154: pyrightconfig pythonVersion align to floor

**Why**: T5 code review #1 指出 per-env `pythonVersion` 跟随本机 venv (7×3.14)，而 pyproject 都声明 `requires-python>=3.12`。3.13+-only 语法会溜过检查。

**Action**: 改 `pyrightconfig.json` 所有 `executionEnvironments[*].pythonVersion` 为 `"3.12"`。顶层 `pythonVersion: "3.12"` 保持不变。

**Sign-off**: `python3 -c "import json; d=json.load(open('pyrightconfig.json')); assert all(e['pythonVersion']=='3.12' for e in d['executionEnvironments'])"` 通过。Pyright regression 跑一遍确认 warning count 没大变化。

---

### T3 — D155: fresh-clone pyright prereq check

**Why**: T5 code review #2 —— fresh clone 缺 `.venv` 时 pyright fallback 到仓库根 venv（无 grpc）→ 500+ 假 unresolved imports。

**Action**: 
1. 新建 `scripts/check-pyright-prereqs.sh`：遍历 9 个 venv 路径，每个 `test -d` 不在就打印警告。`set -euo pipefail`；非零退出码 = 至少一个缺失。
2. 在 `CLAUDE.md` "Preferred Commands" 或 "Setup" 段加一行：`make check-pyright-prereqs` 或 shell 调用。
3. 可选：`Makefile` 加一个 target `check-pyright-prereqs` 调该 shell。

**Sign-off**: 本机运行该脚本退出码 0（9/9 venv 在）；故意 `mv lang/nanobot-runtime-python/.venv /tmp/off` 手工试 —— 脚本应报警并非零退出（验证后 mv 回来）。

---

### T4 — D153: `--out-dir` override + Dockerfile symlink drop

**Why**: T4 code review #3 —— `scripts/gen_runtime_proto.py` 假设 `<repo>/lang/<pkg>/src/<mod>/_proto` 布局；Dockerfile 要用 `ln -s /build/src /build/lang/.../src` 绕过 layout mismatch，Phase 4 新 runtime Dockerfile 会重复 hack。

**Action**:
1. `scripts/gen_runtime_proto.py`：`_parse_args` 加 `--out-dir` 可选 flag；`build(...)` 签名加 `out_dir: Path | None = None`；若给了就直接用，否则走当前 `REPO_ROOT / pkg_dir / "src" / src_pkg / "_proto"` 计算。
2. `lang/claude-code-runtime-python/Dockerfile`：去掉 `ln -s /build/src /build/lang/.../src`；proto 生成命令改为 `python scripts/gen_runtime_proto.py --package-name claude-code-runtime --out-dir /build/src/claude_code_runtime/_proto`。
3. 其他 3 个 Makefile target 保持现状（不需要 out-dir 因为本地 layout 天然对）。

**Sign-off**: Script --help 显示 `--out-dir`；Dockerfile symlink 彻底去除；stub byte-parity 仍然 0 diff（本地 uv run 跑一次 nanobot 验证没回归）。

---

### T5 — D149: ccb-runtime-ts types.ts SoT sync

**Why**: Phase 3.5 S1.T7 review 指出 `lang/ccb-runtime-ts/src/proto/types.ts` 是手写 enum，proto 新增 variant 时 TS 不会自动失败 —— 静默漂移风险。

**Action**（先评估再落地）：
1. 读 `lang/ccb-runtime-ts/` 目录结构 + `package.json` 看 build 路径。
2. 评估两个选项：
   - **Option A**: `@bufbuild/protoc-gen-es` + 生成脚本。改动面大（Bun workflow + 生成 step），但对未来 proto 变更天然健壮。
   - **Option B**: CI grep guard —— 在 proto 文件加 guard 注释（如 `// @ccb-types-ts-sync`），加一个 `.github/workflows/` step 检查 `types.ts` 包含所有 proto enum 字符串；失败则报错。改动面小，人工维护成本低。
3. 给用户 recommendation + 让 user 选；先以 Option B 实施（工期最短、风险最低）。

**Sign-off**: guard 机制生效 —— 手工删 `types.ts` 的一个 enum variant 后 CI step 失败（验证后恢复）；guard 本身进 CI matrix。

---

### T6 — D148: pydantic-ai-runtime test bench 加厚

**Why**: Phase 3.5 S1.T6 review —— pydantic-ai-runtime 只有 4 个 scaffold 测试，远低于 nanobot 36 / claude-code 25+ 的密度。风险：一旦 Phase 4 runtime 契约演进，pydantic-ai 是第一个因测试稀薄而导致回归漏测的。

**Action**:
1. 读 `lang/nanobot-runtime-python/tests/` 取结构模板（agent-loop、tool-execution、hook-envelope、mcp 相关的 test 文件分类）。
2. 给 pydantic-ai 补齐对等覆盖（目标 ≥12 tests，不强求 36）：
   - `sdk_wrapper.py` 至少 4 个 test（agent 构造 / run / 错误路径 / cancellation）
   - agent-loop 至少 3 个 test
   - chunk_type contract 对齐（如果尚未有，参考 nanobot）
3. 能复用 nanobot test fixture 的直接复用。

**Sign-off**: `pytest lang/pydantic-ai-runtime-python/tests/ -v` 数量 ≥12 PASS，0 fail。

---

### T7 — D152: grpcio-tools int-stub decision

**Why**: Phase 3.6 T3 descope 副产物。12 处 `# type: ignore[arg-type]` 在 runtime 正常，但等 Pyright strict mode 扩展时会不停累计。Phase 3.6 T3 reviewer 也指出 `[arg-type]` 是 mypy 语法而非 Pyright 原生；加 `# pyright: ignore[reportArgumentType]` 的切换也属于这条 track。

**Action**（决策 task）：
1. 调研 `grpcio-tools` GitHub issues / PRs 看是否有已合并 / active 的 int-accepting stubs PR。Context7 查文档。
2. 调研 `mypy-protobuf` 替代 —— 生成更宽容 stubs 的成本。
3. 在 `gen_runtime_proto.py` 加 post-process `.pyi` 脚本的难度评估（~30 LOC 正则 + test）。
4. 三选一落地 **或** 明确不落地（documented D152 remains open with updated ETA）：
   - (a) 写 post-process script（去掉 12 处 `# type: ignore`）
   - (b) 等上游 `grpcio-tools` + 改 comment 指向具体 issue
   - (c) 迁移到 `mypy-protobuf` 生成 stubs（较大工程）
5. 如果选 (b)，D152 保持 open 但 ledger 更新：引用具体 upstream issue URL + 预计 ETA。

**Sign-off**: 决策明确写入 ledger D152 row；如果选 (a)，12 处 `# type: ignore` 归零 + `scripts/gen_runtime_proto.py` 测试；如果选 (b)/(c)，D152 row 注记清晰（仍 🧹 tech-debt，但 Phase 4a 不再阻挡）。

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| T1 spy harness 构造复杂度超预期 | 中 | 低 | 先看 `hook_envelope_parity_test.rs` 现有 fixture；若无复用价值直接用 `HookHandler` trait 的 manual impl + `Arc<Mutex<Vec<_>>>` |
| T5 Option A `protoc-gen-es` 工期爆炸 | 中 | 中 | 优先走 Option B；若 user 坚持 Option A，切成独立 Phase 4b |
| T6 test 密度目标主观 | 低 | 低 | 明确目标 ≥12 作为 floor；覆盖路径来自 nanobot 镜像，照抄即可 |
| T7 决策 loop（不同选项都有 downside） | 高 | 中 | 设 2h 调研 time-box；过时限就选 (b) 降级为 documented wait |

## Verification Checklist

- [ ] T1 `harness_envelope_wiring_test.rs` 3 tests PASS
- [ ] T1 `cargo test -p grid-engine` 2390/0（+5 new tests 估计）
- [ ] T1 D151 ledger ✅ CLOSED
- [ ] T2 pyrightconfig all per-env pythonVersion == "3.12"
- [ ] T2 Pyright regression 跑一遍 warning/error count 与 baseline 同阶（±1）
- [ ] T2 D154 ledger ✅ CLOSED
- [ ] T3 `scripts/check-pyright-prereqs.sh` 9/9 venv 检测通过 + 故意缺一个触发报警
- [ ] T3 CLAUDE.md 行更新 / `make check-pyright-prereqs` target 落地
- [ ] T3 D155 ledger ✅ CLOSED
- [ ] T4 `scripts/gen_runtime_proto.py --help` 显示 `--out-dir`
- [ ] T4 Dockerfile `ln -s` 彻底移除
- [ ] T4 stub byte-parity 0 diff（uv run nanobot 验证）
- [ ] T4 D153 ledger ✅ CLOSED
- [ ] T5 选定 option (A/B) + 实施完成 + 故意 drift 测试触发 guard
- [ ] T5 D149 ledger ✅ CLOSED
- [ ] T6 `pytest lang/pydantic-ai-runtime-python/tests/` ≥12 PASS
- [ ] T6 D148 ledger ✅ CLOSED
- [x] T7 决策明确落入 ledger — Option (a) post-process `.pyi` script in `scripts/gen_runtime_proto.py`; 12 `# type: ignore[arg-type]` 全删；D152 ✅ CLOSED
- [ ] `make v2-phase3-e2e` regression 112/112 PASS
- [ ] Commits atomic per task，footer 齐，subject ≤72 chars
- [ ] `/dev-phase-manager:end-phase` 归档

## Out of Scope

- `git push origin main`（保留给人类决策；Phase 4a 完成后讨论）
- Phase 4 product scope 定义（Leg A continuation vs Leg B activation per ADR-V2-023 §P5）
- 任何新 feature 工作
- ADR 变更（所有改动都在 enforcement layer 之下）

## Deferred (new items discovered during this phase)

> 每 task 结束前检查；新发现填这张表。Phase 4a 结束时未清项转给 Phase 4 discuss。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| — | (none yet) | | |

## Related Artifacts

- SSOT ledger: `docs/design/EAASP/DEFERRED_LEDGER.md` §D145–D155 + §"D 编号详细登记"
- Upstream phase: `docs/plans/2026-04-20-phase3.6-tech-debt-cleanup.md`（已归档）
- ADR-V2-023: Grid 两腿战略（决定 Phase 4 方向的对话前置）
