---
id: ADR-V2-006
title: "Hook Envelope Contract (PreToolUse / PostToolUse / Stop)"
type: contract
status: Accepted
date: 2026-04-15
phase: "Phase 2 — Memory and Evidence (S3.T5 implementation reference)"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: contract-test
  trace: []
  review_checklist: null
affected_modules:
  - "crates/grid-runtime/"
  - "lang/claude-code-runtime-python/"
  - "tools/eaasp-skill-registry/"
  - "examples/skills/threshold-calibration/"
related: [ADR-V2-017, ADR-V2-018]
---

# ADR-V2-006 — Hook Envelope Contract (PreToolUse / PostToolUse / Stop)

**Status:** Accepted
**Date:** 2026-04-15
**Phase:** Phase 2 — Memory and Evidence (S3.T5 implementation reference)
**Author:** Jiangwen Su (orchestrated by claude-flow swarm-1776216312625)
**Related:** ADR-V2-017 (L1 runtime ecosystem strategy), ADR-V2-018 (PreCompact hook protocol), Phase 2 plan §S3.T5

---

## 1. Context / 背景

Phase 0.5 S3 introduced `ScopedHookHandler` (Rust, `crates/grid-runtime/src/scoped_hook_handler.rs`) and `hook_substitution.py` (Python, `lang/claude-code-runtime-python/src/claude_code_runtime/hook_substitution.py`). D49 delivered `substitute_hook_vars` variable resolution in both runtimes. S3.T3 verified the skill-extraction meta-skill end-to-end with real bash hooks.

However, **the stdin envelope schema that hooks receive was never formally specified**. Three concrete risks emerged:

1. **Cross-runtime drift** — nothing forces grid-runtime (Rust) and claude-code-runtime (Python) to construct the same JSON payload. A hook written against Rust assumptions could silently fail on Python, or vice versa.
2. **Implicit exit-code contract** — current hooks (e.g. `examples/skills/threshold-calibration/hooks/block_write_scada.sh`) rely on exit 0 / exit 2 semantics, but no ADR mandates this across all hook scopes. The fail-open policy from EAASP §10.8 is referenced in code comments but has no authoritative schema.
3. **No test-vector contract** — future L1 runtimes (Phase 2.5 goose-runtime, Phase 3 pydantic-ai / claw-code — see ADR-V2-017 三轨策略) have no machine-readable contract to validate against. Each re-implementation would be a snowflake.

D51 (registered at S3.T4) named this gap. S3.T5 must close it before landing the scoped-hook executor wiring (G1 in Rust harness + G2/G3 in Python service.py), because both runtimes will emit envelopes that hooks parse. The schema MUST be frozen first.

---

## 2. Decision / 决策 — Stdin envelope schema

Both runtimes MUST write **exactly one** JSON object to the hook process's stdin, then close stdin. The object MUST conform to one of the three canonical shapes below based on the hook scope.

### 2.1 PreToolUse envelope

```json
{
  "event": "PreToolUse",
  "session_id": "sess-abc123",
  "skill_id": "threshold-calibration",
  "tool_name": "scada_write",
  "tool_args": {"device_id": "xfmr-042", "value": 75.0},
  "created_at": "2026-04-15T10:30:00Z"
}
```

### 2.2 PostToolUse envelope

```json
{
  "event": "PostToolUse",
  "session_id": "sess-abc123",
  "skill_id": "threshold-calibration",
  "tool_name": "scada_write",
  "tool_result": "ok",
  "is_error": false,
  "created_at": "2026-04-15T10:30:05Z"
}
```

### 2.3 Stop envelope

```json
{
  "event": "Stop",
  "session_id": "sess-abc123",
  "skill_id": "threshold-calibration",
  "draft_memory_id": "mem-42",
  "evidence_anchor_id": "anchor-99",
  "created_at": "2026-04-15T10:31:00Z"
}
```

### 2.4 Schema rules (MUST)

- `event` MUST be exactly one of `"PreToolUse"`, `"PostToolUse"`, `"Stop"` — case-sensitive, no abbreviations.
- `session_id` MUST be present on every envelope. Format is opaque (runtime-defined). Empty string is forbidden.
- `created_at` MUST be present on every envelope. Format: ISO 8601 UTC with `Z` suffix (Zulu time), second precision or better.
- `skill_id` MUST be present. If the session has no skill attached, the runtime MUST emit the empty string `""` (not `null`, not missing).
- **PreToolUse** — `tool_name` (string, non-empty) + `tool_args` (JSON object, may be empty object `{}`) both REQUIRED.
- **PostToolUse** — `tool_name` (string, non-empty) + `tool_result` (string — serialized tool output) + `is_error` (boolean) all REQUIRED.
- **Stop** — `draft_memory_id` and `evidence_anchor_id` are OPTIONAL. When absent, the runtime MUST emit empty string `""` (not `null`, not missing). Hook-side code MUST guard against empty-string values — see test vector §10.4.

### 2.5 Forward compatibility

- Unknown top-level keys MUST be ignored by hooks (forward-compat).
- New required keys MAY NOT be added without bumping an envelope schema version — but §2 does NOT yet carry a `schema_version` field (D119 defers that to Phase 3 when a breaking-change protocol is defined).

---

## 3. Environment variables / 环境变量

Both runtimes MUST set the following environment variables on the hook process before execve:

| Variable | Value | Notes |
|----------|-------|-------|
| `GRID_SESSION_ID` | session id (same as envelope `session_id`) | always set |
| `GRID_TOOL_NAME` | tool name | empty string for Stop envelopes |
| `GRID_SKILL_ID` | skill id | may be empty string |
| `GRID_EVENT` | one of `PreToolUse` / `PostToolUse` / `Stop` | always set |

**Rationale**: env vars are the fast-path for simple hooks (e.g. bash scripts that only dispatch on `$GRID_EVENT` and `$GRID_TOOL_NAME`). The stdin JSON is the full-path for hooks that need `tool_args` or `evidence_anchor_id`. Hooks MAY use either or both; both MUST be populated by the runtime so neither channel is second-class.

Runtimes MUST NOT inject additional `GRID_*` variables without updating this ADR. Application-specific env vars (e.g. `SKILL_DIR`) are resolved at variable-substitution time (§5), not here.

---

## 4. Exit code contract / 退出码契约

| Exit code | Semantics | stdout / stderr handling |
|-----------|-----------|---------------------------|
| **0** | allow | Stdout MAY contain `{"decision":"allow"\|"deny"\|"ask","reason":"..."}` JSON. If stdout is empty or not valid JSON, decision defaults to `allow`. |
| **2** | deny | Stderr text (trimmed) becomes the deny reason. Stdout is ignored. |
| **other non-zero** | FAIL-OPEN | Runtime logs `WARN` with `hook_id`, exit code, stderr snippet, then continues as if hook returned `allow`. |

Rules:
- **Decision JSON on exit 0** — `decision` field keys are lowercase and MUST be one of `allow`, `deny`, `ask`. `reason` is free-form UTF-8. Runtimes map `ask` → deny at MVP (no interactive branch yet).
- **Deny reason precedence** — exit 2 stderr takes precedence over stdout JSON if both are present. Hooks SHOULD NOT mix the two channels.
- **Fail-open invariant** — per EAASP §10.8, hook errors NEVER block agent flow (see §7). Runtimes implementing this ADR MUST NOT propagate hook exit codes other than 2 as deny.

---

## 5. Variable substitution / 变量替换

Variable substitution resolves `${...}` placeholders in hook command strings (loaded from skill frontmatter) at session Initialize time, before the hook is registered with the executor. This ADR references the two existing implementations and requires them to behave identically:

- **Rust**: `tools/eaasp-skill-registry/src/skill_parser.rs::substitute_hook_vars` (consumed by `crates/grid-runtime/src/harness.rs`)
- **Python**: `lang/claude-code-runtime-python/src/claude_code_runtime/hook_substitution.py::substitute_scoped_hooks`

Both MUST:

1. **Resolve at Initialize time, not at hook invocation time** — `${SKILL_DIR}`, `${SESSION_DIR}`, `${RUNTIME_DIR}` are baked into the command string when the session is created. Hooks see a fully-resolved command.
2. **Support `$$` → literal `$`** — any sequence of two dollar signs collapses to a single literal `$`, bypassing the `${...}` lexer.
3. **Error taxonomy** — three distinct error kinds, all resulting in the same runtime behavior (skip the offending hook, log `ERROR`, session continues with remaining hooks):
   - `UnknownVariableError` — a `${FOO}` placeholder references an undefined variable name.
   - `UnboundVariableError` — a known variable name has no value in `HookVars` (e.g. `${SKILL_DIR}` but no inline skill content materialized and no `EAASP_SKILL_CACHE_DIR` env var set).
   - `MalformedVariableError` — unterminated `${...` or syntactically invalid placeholder.

All three error classes MUST cause the runtime to skip only that individual hook. Other hooks in the same session MUST continue to register. This is per-hook fail-open, not all-or-nothing.

---

## 6. Timeout / 超时

Default hook timeout is **5 seconds**. On timeout, the runtime MUST:

1. Send SIGKILL to the hook process (and any descendants — Rust `tokio::process::Child::kill` + Python `proc.kill()` followed by `await proc.wait()`).
2. Log `WARN` with `hook_id`, timeout value, and the fact that fail-open was taken.
3. Treat the hook as exit-0 allow (§7).

Timeout is per-hook. A future skill frontmatter field `timeout_secs` MAY override the default (declared but NOT required for S3.T5 implementation; leaves room to harden specific hooks without forcing a schema migration). Phase 3+.

---

## 7. Fail-open policy / 失败放行策略

Per EAASP specification §10.8, hook errors MUST NEVER block tool execution or session progression. This is the single most important invariant of the hook subsystem.

Concrete requirements:

- Any exit code other than 0 or 2 → allow (§4).
- Any subprocess spawn error (binary not found, permission denied, OOM) → allow, log `WARN`.
- Any stdin-write error (broken pipe) → allow, log `WARN`.
- Any stdout-decode error (non-UTF-8, non-JSON with exit 0) → allow, log `WARN`.
- Any timeout → allow (§6).
- Any variable-substitution error → hook skipped at Initialize time; session continues without it (§5).

Every fail-open path MUST emit a log line tagged with:
- `hook_id` (stable identifier from skill frontmatter)
- `error_kind` (one of `timeout` / `exit_nonzero_nonblock` / `spawn_error` / `io_error` / `decode_error` / `unknown_variable` / `unbound_variable` / `malformed_variable`)
- The action taken (always "allow" / "skip" — never "deny")

Rationale: a mis-configured hook should degrade observability, not brick the agent. Operators watching logs can then fix the hook at their leisure.

---

## 8. Cross-runtime contract / 跨 runtime 契约

**Both grid-runtime (Rust) and claude-code-runtime (Python) MUST satisfy §2 through §7 identically.**

- hermes-runtime is frozen per ADR-V2-017 轨道 1 and does NOT implement scoped hooks. The hermes freeze decision predates this ADR.
- Phase 2.5 goose-runtime (new, under ADR-V2-017 轨道 3 样板首选) MUST implement this ADR in full on day one. The shared-contract test suite referenced in ADR-V2-017 Phase 2.5 W1 will include envelope fixtures from §10 below.
- Phase 3+ runtimes (pydantic-ai, claw-code, ccb) MUST pass the §10 test vectors before being marked production-capable in the L1 runtime matrix.

Divergence between runtimes is a **contract bug**, not a platform variation. Any runtime discovered to emit a non-conforming envelope (e.g. missing `created_at`, wrong casing on `event`) MUST file a blocking Deferred and halt L1 certification until fixed.

---

## 9. Consequences / 后果 & Open items

### Positive

- Unblocks S3.T5 deliverables — G1 (Rust harness wiring), G2+G3 (Python executor + service.py wiring), G6 (integration tests) all have a single source of truth for envelope shape.
- Cross-runtime parity enforced — future L1 implementations have machine-readable §10 vectors to test against; no more "it works on Rust but not Python" snowflakes.
- Fail-open invariant formalized — §7 makes the EAASP §10.8 reference concrete with a full error-kind taxonomy.
- Forward compatibility — §2.5 permits new optional fields without a breaking change; the `schema_version` gap is acknowledged (D119) rather than papered over.

### Negative / Tradeoffs

- No `schema_version` field at MVP — a future breaking change to the envelope (e.g. renaming `tool_result` to `result`) will require either a best-effort detect-or-tolerate period or a coordinated bump across all runtimes. Accepted per §2.5; tracked as D119.
- Prompt-style hooks (LLM-driven yes/no, originally D50) are out of scope — deferred as **D117** until a concrete use case materializes in Phase 2.5+. MVP scoped hooks are subprocess-only.

### Open / Deferred items

- **D117** (renamed from D50 at S3.T5) — `ScopedHookBody::Prompt` runtime (LLM-driven decision) not covered by this ADR. Deferred to Phase 2.5+ after a concrete skill requests it. Rationale: current skills (threshold-calibration, skill-extraction) all use subprocess hooks; adding an LLM path without a driver is YAGNI.
- **D118** (new) — SkillDir materialization (when inline skill content is written to `{workspace}/skill/SKILL.md` at Initialize) has no cleanup on session end. Resource leak bounded by session disk quota; acceptable at MVP but should be swept in S4 cleanup pass.
- **D119** (new) — Envelope `schema_version` field not enforced. When the first breaking change to the schema is proposed (e.g. Phase 3), this ADR MUST be revised to include a version-negotiation protocol.

---

## 10. Test vectors / 测试向量

The following vectors are **normative**. Any runtime claiming conformance to ADR-V2-006 MUST pass all six.

### 10.1 Canonical envelopes (§2)

The three JSON objects in §2.1, §2.2, §2.3 are byte-for-byte canonical. Runtimes SHOULD produce equivalent JSON (field order is not significant, but all required keys MUST be present).

### 10.2 Hook behavior scenarios

| # | Scenario | stdin envelope | stdout | stderr | exit | Expected runtime behavior |
|---|----------|----------------|--------|--------|------|---------------------------|
| 1 | Pre allow empty | §2.1 | `` (empty) | `` | 0 | allow tool, no reason logged |
| 2 | Pre deny exit 2 | §2.1 | `` | `"policy violation"` | 2 | deny tool, reason = `"policy violation"` |
| 3 | Post allow JSON | §2.2 | `{"decision":"allow"}` | `` | 0 | allow, no deny reason |
| 4 | Stop deny JSON | §2.3 | `{"decision":"deny","reason":"missing anchor"}` | `` | 0 | deny, reason = `"missing anchor"`; if runtime supports Stop hook injection, inject reason as system message |

### 10.3 Empty-string guards (Stop envelope)

Given §2.3 with `draft_memory_id: ""` and `evidence_anchor_id: ""`, a hook using `jq` MUST implement the three-way guard (existence + non-null + non-empty-string). The canonical hook `examples/skills/threshold-calibration/hooks/check_output_anchor.sh` uses:

```jq
(.output // {}) | has("evidence_anchor_id") and (.evidence_anchor_id != null) and (.evidence_anchor_id != "")
```

This pattern is the reference for all future Stop hooks. Documented in S3.T3 review feedback C1.

### 10.4 Timeout behavior (§6)

Hook `sleep 10` with default timeout 5s MUST:
1. Receive SIGKILL within 5-6 seconds (some slack for async scheduling).
2. Runtime logs `WARN` with `error_kind=timeout`.
3. Runtime treats as allow (§7 fail-open).
4. Tool execution proceeds.

### 10.5 Variable substitution errors (§5)

Given a hook command `"${UNKNOWN_VAR}/do.sh"` where `UNKNOWN_VAR` is not in the `HookVars` registry, the runtime MUST:
1. Raise `UnknownVariableError` at Initialize time.
2. Log `ERROR` with `hook_id`, the offending placeholder.
3. Skip **only that hook** — other hooks in the same skill MUST still register.
4. Session creation MUST NOT fail.

### 10.6 Cross-runtime equivalence

A hook receiving the §2.1 envelope from grid-runtime (Rust) and the same envelope from claude-code-runtime (Python) MUST behave identically. Divergence is a contract bug per §8.

---

## 11. Alternatives considered / 候选方案

- **Env vars only, no stdin JSON** — rejected. `tool_args` is a structured object; passing it via env vars would require JSON encoding in a fragile quoting scheme. Stdin JSON is the only clean way to deliver nested structured data.
- **gRPC or WebSocket to hook process** — rejected. Too heavy for skill-level bash hooks. Each hook invocation already crosses a process boundary; adding a protocol layer on top would triple setup overhead and force hooks to link a gRPC client library. Subprocess + stdin JSON is the zero-dependency path that meets MVP needs.
- **Webhook HTTP (like `webhook_executor`)** — retained as a SEPARATE mechanism for enterprise policy hooks (cross-network, long-running, signed payloads). Scoped hooks (this ADR) are explicitly subprocess-based for zero-dependency deployment. The two mechanisms coexist; ADR-V2-006 does not cover webhook executors.

---

## References

- EAASP v2.0 spec §10.8 (fail-open policy)
- EAASP v2.0 MVP Scope §5 N14 (hook-driven policy approval)
- Phase 2 plan `docs/plans/2026-04-14-v2-phase2-plan.md` §S3.T5 (implementation task)
- S3.T5 blueprint `/tmp/s3t5-blueprint.md` §B.1 (outline this ADR follows)
- ADR-V2-017 (L1 runtime ecosystem — why goose and Phase 3+ runtimes must pass §10 vectors)
- ADR-V2-018 (PreCompact hook protocol — sibling hook spec, shares §7 fail-open philosophy)
- Existing implementations:
  - Rust: `crates/grid-runtime/src/scoped_hook_handler.rs`
  - Rust: `tools/eaasp-skill-registry/src/skill_parser.rs::substitute_hook_vars`
  - Python: `lang/claude-code-runtime-python/src/claude_code_runtime/hook_substitution.py`
  - Reference hooks: `examples/skills/threshold-calibration/hooks/block_write_scada.sh`, `check_output_anchor.sh`
