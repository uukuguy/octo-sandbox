# EAASP Runtime Protocol v2

**Status:** ACTIVE — Ring-2 MVP (Phase 0)
**Supersedes:** `proto/eaasp/runtime/v1/` (作废 / deprecated, to be removed in S1.T3)

## Files

| File | Purpose |
|------|---------|
| `common.proto` | Structured shared types: `SessionPayload` (P1-P5 priority blocks), `PolicyContext`, `EventContext`, `MemoryRef`, `SkillInstructions`, `UserPreferences`, `EvidenceAnchor` |
| `runtime.proto` | `RuntimeService` — the 16-method L1 contract + `HookEventType` (14 values) + `Capabilities` (with `credential_mode`) + `EventStreamEntry` (placeholder) |
| `hook.proto` | `HookBridgeService` — bidirectional streaming between L1 runtimes and L3 governance (inherits Phase BE W2 design, re-rooted on v2) |

## Method Inventory (RuntimeService)

### 12 MUST methods (certifier-enforced)

| # | Method | Direction |
|---|--------|-----------|
| 1 | `Initialize` | unary |
| 2 | `Send` | server streaming |
| 3 | `LoadSkill` | unary |
| 4 | `OnToolCall` | unary |
| 5 | `OnToolResult` | unary |
| 6 | `OnStop` | unary |
| 7 | `GetState` | unary |
| 8 | `ConnectMCP` | unary |
| 9 | `EmitTelemetry` | unary |
| 10 | `GetCapabilities` | unary |
| 11 | `Terminate` | unary |
| 12 | `RestoreState` | unary |

### 4 OPTIONAL methods (certifier reports as bonus; absent ≠ failure)

| # | Method |
|---|--------|
| 13 | `Health` |
| 14 | `DisconnectMcp` |
| 15 | `PauseSession` |
| 16 | `ResumeSession` |

### 1 PLACEHOLDER — ADR-V2-001 pending

| # | Method | Notes |
|---|--------|-------|
| 17 | `EmitEvent(EventStreamEntry)` | Wire format + backend unresolved (ADR-V2-001 / -002 / -003 deferred to Phase 1). Phase 0 runtimes MAY no-op; certifier reports `placeholder: present`. |

## Why v2? — Key Differences from v1

1. **Structured `SessionPayload`** — v1 used a flat `map<string,string>` for everything. v2 defines 5 priority blocks (P1-P5), so every runtime trims context in the same deterministic order: `P5 → P4 → P3`, with `P1`/`P2` never touched.
2. **14 `HookEventType` enum values** — v1 hardcoded 3-5 hook types inline in messages. v2 enumerates all 14 (9 L1 + 2 L3 + 3 L4) so capability manifests can declare which ones a runtime actually fires.
3. **`credential_mode` in Capabilities** — L4 RuntimeSelector can now match user/tenant credential posture against runtime support (DIRECT vs PROXY vs BRIDGE_INJECTED).
4. **`MemoryRef` as first-class P3 block** — Ring-2 MVP depends on cross-session memory being deterministically surfaced to the runtime. v1 had no such concept.
5. **`EmitEvent` placeholder** — preserves a hole for the Event Stream architecture without forcing a premature wire format commitment.

## Deny-always-wins precedence (reminder)

When a `ManagedHook` (scope=`managed`) and a `ScopedHook` (scope=`frontmatter` inside `SkillInstructions`) both match the same event, **the hook whose `action == "deny"` wins**, regardless of precedence numbers. Ties between two `deny`s are broken by lower `precedence` value. This is a hard contract and certifier S2.T2 must test it.

## Authoritative references

- `docs/design/EAASP/EAASP-Design-Specification-v2.0.docx` §8 (v2 design spec)
- `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` §3.3 (Ring-2 scope)
- `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md` decision registry D1-D12
- `docs/plans/2026-04-11-v2-mvp-phase0-plan.md` Stage S1.T2 (this task)
