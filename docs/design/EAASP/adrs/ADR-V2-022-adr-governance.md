---
id: ADR-V2-022
title: "ADR 生命周期治理机制"
type: strategy
status: Accepted
date: 2026-04-19
phase: "元治理（跨 Phase）"
author: "Jiangwen Su (+ Claude)"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: strategic
  trace:
    - "~/.claude/skills/adr-governance/scripts/adr_lint.py"
    - "~/.claude/skills/adr-governance/scripts/adr_classify.py"
    - ".github/workflows/adr-audit.yml"
  review_checklist: "~/.claude/skills/adr-governance/SKILL.md"
affected_modules:
  - "docs/design/EAASP/adrs/"
  - ".adr-config.yaml"
related: []
---

# ADR-V2-022 — ADR 生命周期治理机制

**Status:** Accepted
**Date:** 2026-04-19
**Phase:** 元治理（跨 Phase）
**Author:** Jiangwen Su (+ Claude)
**Related:** 所有 ADR-V2-*（本 ADR 规范整个 ADR 形式本身）

---

## Context / 背景

截至 2026-04-19，项目累积 13 份 ADR（V2-001 ~ V2-021），但暴露出 4 个结构性问题：

### 1. 写完即搁置，缺乏强制

13 份 ADR 中只有约 5 份有契约测试或 proto schema 兜底；其余靠 reviewer 凭记忆遵守。这直接导致 Phase 3 验证时发现 `SendResponse.chunk_type` 在 7 个 runtime 间自由漂移（详见 ADR-V2-021）— ADR-V2-017（L1 生态策略）本应覆盖跨 runtime 一致性，但只作为"战略性阐述"存在，没有任何 enforcement。

### 2. 类型混淆

ADR 目录同时混入三类性质不同的文档：
- **契约型**：proto schema、hook envelope 格式、enum 取值 → 应当被机器强制
- **战略型**：L1 生态三轨（主力/样板/对比）、部署模型分级 → 适合文档记录，不适合字段级强制
- **实施记录型**：某 phase 某任务的实现细节、决策过程 → 其实应该归属 `docs/plans/` 或 `docs/decisions/`

混在一起导致"ADR 这个形式是否值得"本身被稀释。

### 3. 无降格/废弃机制

ADR 只进不出。ADR-TEMPLATE.md 在 Status 字段支持 `Deprecated / Superseded by` 但实践中 0 使用。`docs/design/EAASP/adrs/` 下 13 份全部 `Accepted`，包括一些实质已被后续决策覆盖或过时的。

### 4. 新会话认知断层

Claude Code 每次新会话重新加载上下文。没有机器可读的 ADR 索引 + 健康度报告，Claude（或任何新加入的开发者）难以快速摸清"当前哪些 ADR 仍有效、哪些过时、哪些冲突"。

---

## Decision / 决定

建立覆盖**全生命周期**的 ADR 治理机制，分为三层：

### 1. ADR 三分类

| 类型 | 定义 | 存放位置 | 强制等级 |
|---|---|---|---|
| **contract** | 规定具体字段取值、schema、enum、接口契约；违反后必须在代码层面被拒绝 | `docs/adrs/` | 机器强制（proto / contract-test） |
| **strategy** | 规定方向/选型/生态/原则；规范"怎么想"而非"代码长什么样" | `docs/adrs/` | 文档级（review checklist） |
| **record** | 某 phase 某任务的实施决策记录；对未来约束弱 | `docs/plans/completed/` 或 `docs/decisions/` | 不强制 |

**新 ADR 必须在 frontmatter 声明 `type`**。不能声明或不符合分类规则的 ADR 不允许创建。

### 2. Frontmatter schema（机器可读）

每个 ADR 必须有 YAML frontmatter（位于第一条 `---` 块）：

```yaml
---
id: ADR-V2-NNN                     # 唯一 ID，按顺序分配
title: "短标题"
type: contract | strategy | record
status: Proposed | Accepted | Superseded | Deprecated | Archived
date: YYYY-MM-DD
phase: "Phase X — 描述"
author: "名字"
supersedes: []                     # 本 ADR 取代的旧 ADR ID 列表
superseded_by: null                # 若本 ADR 被取代，指向新 ADR
deprecated_at: null                # 若 Deprecated 则填日期
deprecated_reason: null            # 过时原因
enforcement:
  level: physical | contract-test | review-only | strategic
  trace:                           # contract 类必填；其他可空
    - path/to/test_file.py
    - .github/workflows/xxx.yml
  review_checklist: null           # review-only / strategic 时填文档位置
affected_modules:
  - crates/xxx/
  - proto/xxx.proto
related: []                        # 相关 ADR ID
---
```

### 3. 生命周期状态机

```
            ┌──────────┐
  (创建)    │ Proposed │
            └─────┬────┘
                  │ /adr:accept（跑 F1-F5 硬门通过）
                  ▼
            ┌──────────┐
            │ Accepted │◀──── 长期保持
            └─┬────┬───┘
              │    │
   supersede  │    │ obsolete
              │    │
              ▼    ▼
    ┌────────────┐ ┌─────────────┐
    │ Superseded │ │ Deprecated  │
    └────────────┘ └─────────────┘
              │    │
              │    │ (可选归档)
              ▼    ▼
            ┌──────────┐
            │ Archived │
            └──────────┘

         下行路径（record 型不走正常状态机）:
            ┌──────────┐
  /adr:     │ Proposed │──/adr:downgrade──▶ 移到 docs/plans/completed/
  classify  └──────────┘                    从 ADR 名录移除
  检测为
  record
```

### 4. 强制等级定义

- **physical**: proto schema / DB migration / Rust 类型系统等 — 违反直接编译失败
- **contract-test**: 有至少一个 pytest/cargo test 测试该决策，且该测试进入 CI matrix
- **review-only**: 靠 code review 人工把关；必须在 `enforcement.review_checklist` 指向一份可追溯的 checklist 文档
- **strategic**: 战略型 ADR，不强制代码级遵守；但新决策与之相关时必须 cross-reference

### 5. 治理工具（slash commands + skills）

全局 plugin 位于 `~/.claude/skills/adr-governance/`，提供：

| 命令 | 作用 |
|---|---|
| `/adr:new --type <contract\|strategy\|record>` | 创建新 ADR（record 直接走降格路径） |
| `/adr:accept <id>` | Proposed → Accepted（跑 lint 硬门） |
| `/adr:supersede <old> <new>` | 取代关系原子更新 |
| `/adr:deprecate <id> --reason "…"` | 标记过时 |
| `/adr:downgrade <id> --to plan\|decision` | 降格 record 型 ADR |
| `/adr:classify [--id X\|--all]` | 自动分类审计 |
| `/adr:review [--health]` | 周期健康报告 |
| `/adr:audit [--ci]` | 完整 lint（F1-F5） |
| `/adr:reconcile <id-a> <id-b>` | 冲突裁决 |
| `/adr:list` / `/adr:show` / `/adr:trace` / `/adr:status` | 查询类 |

### 6. lint 检查项（F1-F5）

- **F1 Frontmatter 合规**：必填字段齐全，status/type 在合法集
- **F2 Trace 存在性**：contract 类的 trace 文件真实存在
- **F3 Trace 被 CI 执行**：contract 类的测试在某个 workflow 的 step 里出现
- **F4 冲突检测**：两条 Accepted ADR 覆盖同一 Affected Module 且 Decision 矛盾 → FAIL
- **F5 过时检测**：Affected Module 被删 / 180 天无引用 → WARN

F1-F4 硬门，F5 警告。CI 每 PR 跑一次。

### 7. 项目适配

每个项目在根目录可选 `.adr-config.yaml` 声明 ADR 根目录 / 测试根目录 / CI workflow 路径：

```yaml
adr_root: docs/design/EAASP/adrs
contract_test_root: tests/contract
downgrade_targets:
  plan: docs/plans/completed
  decision: docs/decisions
ci_workflows:
  - .github/workflows/phase3-contract.yml
```

默认值：`docs/adrs/`, `tests/`, `.github/workflows/`

---

## Consequences

### Positive

- **契约真正被强制**：contract 型 ADR 进 CI 硬门，跨 runtime/session 漂移会在 PR 阶段被拦
- **分类减少噪音**：record 型及时降格到 plan，ADR 名录只留真正有长期约束力的条款
- **生命周期闭环**：过时 ADR 有明确 Deprecated / Superseded 路径，不再只进不出
- **新会话快速对齐**：`/adr:status` + `adr-index.yaml` 机器可读，Claude/新开发者 1 秒摸清现状
- **工具化审计**：`/adr:classify --all` + `/adr:review --health` 可随时重跑，不依赖人力记忆

### Negative

- **初期成本**：13 份已有 ADR 需回填 frontmatter + 分类审计；约 2-4 小时
- **流程负担**：新 ADR 必须走 `/adr:new` + 填 enforcement，比直接写 .md 多 2-3 步
- **契约测试回填**：review-only 降级到 contract-test 的 ADR 需要补契约测试

### Risks

- **R1 分类规则误判**：自动 classify 把战略型误判为 record → 缓解：混合规则+人工确认，所有降格操作不可逆前必须 user 确认
- **R2 过度治理**：小项目用不着这么重的流程 → 缓解：全局 plugin 提供默认值，配置缺失时 fallback；不强制每个项目都用
- **R3 AI 遗忘后续波次**：跨会话连贯性 → 缓解：`ROADMAP.md` + `CLAUDE.md` 追认 + `MEMORY.md` 顶部条目 + CI 提醒组合使用

---

## Enforcement

- **Level:** contract-test + review-only（混合）
- **Trace:**
  - `~/.claude/skills/adr-governance/scripts/adr_lint.py`（F1-F5 检查器）
  - `~/.claude/skills/adr-governance/scripts/adr_classify.py`（分类规则）
  - `<project>/.github/workflows/adr-audit.yml`（CI 集成）
- **Review checklist:** `~/.claude/skills/adr-governance/SKILL.md` §Triggering Rules

本 ADR 本身是 strategic（规范"怎么治理 ADR"而非具体字段取值），但它触发的所有 slash commands/scripts 本身是 physical/contract-test 强制的 — 即元治理的效力由工具保证。

---

## Alternatives Considered

### A. 轻量化：只用 CLAUDE.md 段落 + 口头约定
**拒绝理由**：chunk_type 漂移证明口头约定无效；13 份 ADR 已经证明靠人记不成。

### B. 重量级：引入专用 ADR 管理 SaaS（如 adr-tools）
**拒绝理由**：外部工具引入依赖 + 学习成本；当前 Claude Code 生态自带 slash commands + skills 基础设施，复用更自然。

### C. 完全放弃 ADR，改用 PR 描述 + CLAUDE.md
**拒绝理由**：契约型决策（proto enum、hook envelope 等）需要独立可引用的锚点文档，PR 描述易被历史淹没；CLAUDE.md 已接近承载上限。

---

## Implementation

此 ADR 为元治理，其落地本身即是第一波：
- `~/.claude/skills/adr-governance/`（全局 plugin）
- `~/.claude/commands/adr-*.md`（slash commands）
- 13 份已有 ADR 的 frontmatter 回填
- `AUDIT-2026-04-19.md` 审计报告

后续 ADR（含 ADR-V2-021 chunk_type 落地）必须走此治理机制。

## History

- 2026-04-19: Accepted. Created by Claude Code session after Jiangwen requested global ADR governance plugin.
