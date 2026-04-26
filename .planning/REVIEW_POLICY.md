# Grid — Review Policy

> **Status**: Draft 2026-04-26 (Phase 4a 实战经验起草,Phase 4.0 验证后转 Active)
> **Updates**: 每次发现 review 边界漏判后 prepend 一条 "Lesson learned" 到本文档底部
> **Source of truth**: 本文档 + `~/.claude/plugins/cache/superpowers-marketplace/superpowers/4.2.0/skills/subagent-driven-development/`(模板) + `~/.claude/skills/gsd-code-review/SKILL.md`(GSD 单 reviewer 流程)

---

## 1. Why this exists

GSD 自带 `/gsd-code-review` 是 phase-end 的**单 reviewer 批量扫**;superpowers 的 `subagent-driven-development` 是**每 task 完成后两阶段(spec → quality)review**。两者**互补,不替代**。本 policy 决定哪种 task 用哪种 review,以及 trigger 怎么标。

**Phase 4a 实测证据** (本 policy 起草的实证依据):

| Phase 4a Task | 类型 | Review 实测 | 找到的 Issue 类别 |
|---------------|------|------------|------------------|
| T1 (D151) | Rust 回归测试 ~50 LOC | 单 reviewer + Rust fmt followup commit | T1 followup:**整 crate fmt drift**(`cargo fmt -p` 无 `--check` reformat 全 crate)—— 单 review 后才发现 |
| T2 (D154) | pyrightconfig 配置 5 文件 | 直接落,无 review | 无 issue |
| T3 (D155) | bash 脚本 30 LOC | 直接落,无 review | 无 issue |
| T4 (D153) | Python script + Dockerfile 改动 | 直接落,无 review | 无 issue |
| **T5 (D149)** | **新 CI guard 脚本 + 新 workflow 文件** | **superpowers 两阶段** | **抓到 4 类隐患**:awk `/^\}/` 不抗 reformat、wire-int check 缺失、`echo` vs `printf` portability、`set -u` 下空数组爆炸 |
| **T6 (D148)** | **新增 18 个 pytest** | **superpowers 两阶段** | **抓到 3 类隐患**:`# noqa: ARG001` 无 ruff config、ledger LOC 数字 stale、test sentinel pattern 缺失 |
| T7 (D152) | post-process script + 12 处 type:ignore 删除 | superpowers 两阶段 | 0 issue(实现一次过,但 review pass 验证了 wire-int + idempotency) |

**结论**:T5 / T6 类的"新组件交付"靠 superpowers 抓到了 GSD 单 review 大概率漏掉的细节(awk 闭合括号 / sentinel pattern / 工具特定注解),而 T2-T4 这种"配置改动 + 已知 pattern" 直接落零成本,review 是 PM theater。Policy 的目标是把这个 split 显式化。

---

## 2. Three Risk Tiers

每个 task 在 PLAN.md frontmatter 用 `review_protocol:` 字段显式标记。GSD executor(或人工)读到字段后激活相应 review 链。

### 🔴 High Risk — `review_protocol: superpowers-two-stage`

**Trigger condition**(满足任一即触发):

1. **Proto 契约改动** —— 任何 `proto/eaasp/runtime/v2/*.proto` 改动(common.proto / runtime.proto / hook.proto)
2. **Accepted contract ADR `affected_modules` 内的代码改动** —— 例如 ADR-V2-006 / V2-020 / V2-021 已 trace 的模块,具体路径见各 ADR `enforcement.trace`
3. **Core agent loop 改动** —— `crates/grid-engine/src/agent/harness.rs` / `agent_loop.rs` / `executor.rs` / `loop_config.rs`
4. **Hook 契约层改动** —— `crates/grid-engine/src/hooks/*` 任何文件
5. **Security policy 改动** —— `crates/grid-engine/src/security/*` / `SecurityPolicy` / `CommandRiskLevel` / `ActionTracker` / `audit/*`
6. **跨 ≥3 个 L1 runtime 的并行修改** —— 一个 PR 同时改 grid-runtime + claude-code + nanobot + ... 等
7. **新 CI workflow / 新 guard script 交付** —— 例如 Phase 4a T5 类(`.github/workflows/*.yml` 新增 + 配套 bash 脚本 ≥30 LOC)
8. **新增大批 test (≥10 个 test) 落到现有 service 文件** —— 例如 Phase 4a T6 类
9. **任何 LOC delta > 200 的 task**
10. **新增 crate / 新 Python package** —— Cargo.toml workspace members 或 lang/* 新目录
11. **proto stub codegen 后处理 / Python type stub 写入** —— 例如 Phase 4a T7 `_loosen_enum_stubs`,因为它影响 4 个 package 的 type 表达

**Review chain**:

```
gsd-execute-phase task complete
  ↓
implementer subagent (per superpowers-marketplace/.../implementer-prompt.md)
  → 实现 + 自 review + commit
  ↓
spec reviewer subagent (per superpowers-marketplace/.../spec-reviewer-prompt.md)
  → 验证 "is it what was asked? nothing more, nothing less"
  → 如有 Critical/Important issue,implementer 修复后 re-review,直到 ✅
  ↓
code quality reviewer subagent (subagent_type: comprehensive-review:code-reviewer)
  → 验证 "is it well-built? 鲁棒性 / 可读性 / 边界 / portability"
  → 如有 Critical/Important issue,implementer 修复后 re-review,直到 ✅ Approved
  ↓
mark task complete
```

**Critical requirement**: spec ✅ 必须先于 quality reviewer 启动。**严禁同时跑两个 reviewer**(原因:spec 找出来的"build wrong thing"会浪费 quality reviewer 的工作)。

**Cost expectation**: 每个 high-risk task 约 +20-40 min wall time(2 个 review pass × 5-15 min)。

### 🟡 Medium Risk — `review_protocol: gsd-standard`

**Trigger condition**(高风险条件全不满足,且满足任一):

1. 单 runtime / 单 crate 内修改,LOC delta 50-200
2. 测试代码新增 < 10 个 test
3. 文档 + ADR 改动(包括新 ADR 草稿,但不含 contract ADR 的 enforcement.trace 列表内代码改动)
4. CI workflow 调整(已有 workflow 改 step,不含新建 workflow)
5. 新增 helper / utility 函数,non-public surface
6. Refactor 不改外部行为(extract function / rename / move file)

**Review chain**:

```
gsd-execute-phase task complete
  ↓
implementer 自 review + commit
  ↓
phase 末统一 /gsd-code-review (gsd-code-reviewer agent 扫整 phase)
  → 输出 docs/plans/<phase>/REVIEW.md (severity-classified findings)
  ↓
/gsd-code-review-fix (gsd-code-fixer agent 应用修复,每个修复 atomic commit)
  ↓
optional 再 /gsd-code-review 验证归零
```

**Cost**: 每 phase 末 +10-30 min(取决于 phase 内 task 数)。

### 🟢 Low Risk — `review_protocol: skip`

**Trigger condition**(以下全部满足):

1. LOC delta < 50
2. 单文件改动且非核心(不是 harness.rs / hooks / proto / Cargo.toml workspace / pyrightconfig)
3. typo / format / rename 类 mechanical 改动
4. 文档微调(纯文字修复,不改架构 claim)
5. 不引入新依赖
6. 不改 commit footer / git workflow

**Review chain**:

```
implementer 自 review + commit
(无 review pass)
```

**Cost**: 0。

---

## 3. PLAN.md Task Frontmatter Format

GSD `gsd-plan-phase` 在生成 PLAN.md 时,每个 task 段加 metadata 注释:

```markdown
### T1 — Add new ChunkType variant + propagate to 7 runtimes

<!-- meta
review_protocol: superpowers-two-stage
review_trigger: proto-contract-change + cross-runtime-modification (rules 2.1, 2.6)
review_estimated_overhead: +30min
-->

**Why**: ...
**Action**: ...
**Sign-off**: ...
```

```markdown
### T3 — Bump Cargo.toml workspace pin for tokio 1.42 → 1.44

<!-- meta
review_protocol: gsd-standard
review_trigger: rule 2.5 (helper / utility, no behavior change)
-->

**Why**: ...
```

```markdown
### T5 — Fix typo in CLAUDE.md

<!-- meta
review_protocol: skip
review_trigger: rule 3.4 (typo, < 50 LOC, single file, no architecture)
-->
```

**Plan-phase 阶段决策路径**:

1. `gsd-planner` agent 起草 PLAN.md 时,**默认每个 task 标 `review_protocol: gsd-standard`**(保守 default)
2. `gsd-plan-checker` agent 在 verification pass 中,把每 task 拿去对照本 policy 的 §2 trigger conditions,**如果命中 high-risk,把 frontmatter 改为 `superpowers-two-stage`**;如果命中 low-risk all-pass,改为 `skip`
3. plan-checker 给出最终 PLAN.md 时,人工最后 review frontmatter(gate before approval)
4. plan approved 后 frontmatter 不再改;execute-phase 严格按 frontmatter 行 review

**人工 override 路径**:任何时候你可以手编 PLAN.md frontmatter,改 `review_protocol` 字段。executor 服从最新 frontmatter。

---

## 4. Two-stage Review Implementation Details

**当 `review_protocol: superpowers-two-stage` 触发时,以下是确切流程**(基于 Phase 4a T5/T6 实操):

### 4.1 Implementer 阶段

```
controller (你 or gsd-executor) 调用 Agent tool:
  subagent_type: general-purpose
  prompt: 复用模板 ~/.claude/plugins/cache/superpowers-marketplace/superpowers/4.2.0/skills/subagent-driven-development/implementer-prompt.md
         + 本 task 完整文本(不让 subagent 读 PLAN.md)
         + 完整 Context(scope / dependencies / 风险点 / 不要做的事)
         + Self-Review checklist(强制 implementer 自查再报告)
```

Implementer 必须:
- 实现 → 测试 → 自 review → commit
- 自 review 时检查 4 类:Completeness / Quality / Discipline / Testing
- 报告时给:What implemented, files changed, test output, self-review findings

### 4.2 Spec Reviewer 阶段

```
controller 调用 Agent tool:
  subagent_type: general-purpose
  prompt: 复用模板 ~/.../subagent-driven-development/spec-reviewer-prompt.md
         + 完整 task requirements
         + Implementer 的 report (告诉 reviewer 别信)
         + 提示 "Read the actual code, do not trust the report"
```

Reviewer 必须:
- 独立读 commit / diff
- 验证 missing requirements / extra requirements / misunderstandings
- 输出 ✅ 或 ❌ + 具体 file:line 引用

如果 ❌ → controller 回到 4.1 派 implementer fix subagent → spec reviewer re-review 直到 ✅。

### 4.3 Quality Reviewer 阶段(必须 spec ✅ 后)

```
controller 调用 Agent tool:
  subagent_type: comprehensive-review:code-reviewer
  prompt: 同样的 task requirements + base/head SHA
         + Phase 4a 实证总结的 focus areas:
           - portability / cross-platform (BSD vs GNU awk, macOS vs Linux)
           - reformat hazards (awk regex / 整 crate fmt drift)
           - wire-int / contract equality (not just identifier presence)
           - sentinel patterns (assertion failure mode clarity)
           - locale / IFS / signal sensitivity (bash)
           - tool-specific suppressions (noqa: X 是否 X 在用?)
           - error message accuracy (remediation 文本不能 over-promise)
```

Reviewer 必须:
- 标 Critical / Important / Minor
- Strengths(让 implementer 知道做对什么)
- Specific file:line + 改法建议
- 终判:✅ Approved / 🟡 Approve-with-comments / ❌ Request changes

如果 ❌ 或 Critical/Important 多于 1 → 回 4.1 implementer fix → quality re-review 直到 ✅ 或 🟡 Approve-with-comments。Minor-only 可接受。

### 4.4 Re-review fix-pass commit format

Implementer fix-pass 单独 commit,subject 用 `fix(<scope>): D<N> tighten T<X> <one-line> (review fix)`。Phase 4a 实例:
- `aaf85aa` — `fix(ci): D149 tighten T5 guard — wire-int check + portability`
- `a274ebd` — `fix(tests): D148 tighten T6 signatures + reconcile ledger LOC`

每个 issue 列在 commit message body 里,带 review pass 给的 ID(I1/I2/I3 或 M1/M2/M3)。

---

## 5. Pattern Library — High-risk patterns from Phase 4a (concrete templates)

**每次 quality reviewer 找到一个新类型隐患,prepend 到本节,变成 future plan-phase 的 trigger keyword**。Phase 4a 起首批:

### 5.1 awk 闭合括号 reformat 隐患
```awk
# DON'T: silent in_state forever if } 未来被 indent
in_enum && /^\}/ { in_enum = 0; next }

# DO: 抗 reformat
in_enum && /^[[:space:]]*\}/ { in_enum = 0; next }
```

### 5.2 wire-int / contract equality not just name presence
对外 wire format(proto enum / API field),check name 出现 + check 数字值;remediation 文本必须明示是否检查 wire ints。

### 5.3 Bash `set -u` 空数组爆炸
```bash
# DON'T: 在 set -u 下空数组 expansion 爆炸
declare -a missing=()
for x in "${missing[@]}"; do ...; done   # bash 4 OK,但 dash/sh 早期版本 / 某些 Bash 配置 fail

# DO: 显式 guard
if [ ${#missing[@]} -gt 0 ]; then
  for x in "${missing[@]}"; do ...; done
fi
```

### 5.4 echo content piping vs printf
```bash
# DON'T: 内容来自变量,echo 可能解释 backslash / -e flag
echo "$ts_block" | grep ...

# DO: portable
printf '%s\n' "$ts_block" | grep ...
```

### 5.5 Test sentinel patterns
```python
# DON'T: 字典空初始,test never-called path raises KeyError 而非 AssertionError
captured = {}
captured["value"] = ...
assert captured["value"] == "expected"

# DO: sentinel 让 never-called 也 surfaces 为干净 AssertionError
captured = {"value": "<unset>"}
captured["value"] = ...
assert captured["value"] == "expected"
```

### 5.6 Tool-specific suppressions 没用对工具
```python
# DON'T: 如果项目没有 ruff config,#noqa: ARG001 是死注解
def fake(self, prompt, **kwargs):  # noqa: ARG001
    ...

# DO: 用 PEP8 standard underscore-prefix
def fake(_self, _prompt, **_kwargs):
    ...
```

### 5.7 Cargo fmt 整 crate drift
```bash
# DON'T: 无 --check 会 reformat 整 crate(170+ files)
cargo fmt -p grid-engine -- some/file.rs

# DO: scope 仅 inspect,人工 Edit 修
cargo fmt -p grid-engine -- --check some/file.rs
```

---

## 6. Anti-patterns (DO NOT do these)

- ❌ **不要**让 spec reviewer 和 quality reviewer 同时跑(顺序必须 spec → quality)
- ❌ **不要**让同一个 subagent 既写代码又自 review —— 必须用 fresh-context reviewer subagent
- ❌ **不要**accept "close enough" on spec review(spec reviewer 找到 issue = 没做完,不是 polish)
- ❌ **不要**skip re-review pass(reviewer 找到 issue → implementer fix → reviewer 必须再看一次,不能省)
- ❌ **不要**在已经 high-risk task 上跑 GSD `/gsd-code-review` 替代 superpowers two-stage(GSD 是 broad-stroke,不抓 5.1-5.7 类细节)
- ❌ **不要**把 review_protocol 标 `skip` 然后下 review 再补 —— 改 frontmatter 必须在 plan-checker 阶段或 plan-approved 前
- ❌ **不要**把 superpowers 模板硬塞进 GSD-only project(本项目是混合,只在 high-risk task 触发)

---

## 7. Lessons Learned (prepend new entries on top)

> 每次 review 边界判断错时(类型应该 high 标了 medium,反之亦然),写一条到本节顶部。Phase 4.0 起开始累积。

*(empty — 待 Phase 4.0+ 实证)*

---

## 8. Open Questions / Design Debt

- **Q1**: GSD 自身的 `gsd-code-review` 输出 REVIEW.md;superpowers reviewer 输出在 conversation。当前混用模式是 phase 末 `/gsd-code-review` + 高风险 task 用 superpowers,如何避免**双 review 重叠浪费**?当前 work-around:high-risk task 用 superpowers 后,GSD `/gsd-code-review` 跑时 reviewer agent 应该看到 commit message 提到 "(review fix)" 或 frontmatter `review_protocol: superpowers-two-stage` 字段,自然 skip 或 lighter-pass。**未验证**,Phase 4.1 GOVERNANCE-02 可顺手 case study。

- **Q2**: "High-risk task" 的 trigger 列表 §2.1-§2.11 是 Phase 4a 实证 + Phase 4 项目特性推出的,**Phase 4 之外的 milestone 可能需要扩展**。例如若 Leg B 激活,multi-tenant 隔离 / JWT auth / 用户 namespace 类 task 可能需要新 trigger row(security-cross-cutting)。本 policy 在 milestone 转换 review 时(`/gsd-complete-milestone` 后)需要 audit 一次 §2 trigger 是否还覆盖当前 milestone 范围。

- **Q3**: superpowers `subagent-driven-development` skill 自身可能演进(目前 4.2.0)。如果 implementer-prompt.md / spec-reviewer-prompt.md 模板更新,本 policy 引用的路径需要同步。建议每次 GSD-managed phase 开始前 `git -C ~/.claude/plugins/cache/superpowers-marketplace/superpowers grep -n version` 看版本号,如有变更读 CHANGELOG。

---

*Last updated: 2026-04-26 — Initial draft from Phase 4a T1-T7 实战经验。Phase 4.0 验证后转 Active.*
