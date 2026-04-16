# Contract Suite Changelog

All notable changes to the EAASP v2.0 L1 runtime contract test suite are
documented in this file. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

The authoritative policy for this suite lives in ADR-V2-017 §2 ("共享契约测试集").
Every entry below must be traceable to a specific ledger item in
`docs/design/EAASP/DEFERRED_LEDGER.md` or an accepted ADR.

## v1.0.0 — 2026-04-16 (Phase 2.5 S0 freeze)

### Status
Stable baseline. This is the first frozen snapshot of the cross-runtime
contract suite. Subsequent W1 (goose-runtime) and W2 (nanobot-runtime)
adapter work must satisfy this contract without silently regressing any
already-green runtime.

### Test surface
35 cases across 6 suites (medium depth per ADR-V2-017 §2).

### Validated runtimes
| Runtime | Language | Result |
|---------|----------|--------|
| grid-runtime | Rust | 13 PASS / 22 xfail |
| claude-code-runtime | Python | 18 PASS / 17 xfail |
| goose-runtime | Rust | pending Phase 2.5 W1 |
| nanobot-runtime | Python | pending Phase 2.5 W2 |

### Scope (per ADR-V2-017 §2)

- **Proto** — 16 gRPC methods (12 MUST + 4 OPTIONAL + `EmitEvent`).
- **Events** — 7 event types: `CHUNK`, `TOOL_CALL`, `TOOL_RESULT`, `STOP`,
  `ERROR`, `HOOK_FIRED`, `PRE_COMPACT`.
- **Hooks** — 3 scopes per ADR-V2-006: `PreToolUse`, `PostToolUse`, `Stop`.
- **MCP** — `ConnectMCPRequest` round-trip shape only (live bridge testing
  deferred — see D137).
- **Skills** — `LoadSkill` + `SkillInstructions` shape (workflow
  enforcement via `required_tools` deferred — see D138).

### Per-file pass / xfail breakdown

| File | Tests | grid-runtime | claude-code-runtime |
|------|-------|--------------|---------------------|
| `test_proto_shape.py` | 10 | 9 PASS, 1 xfail | 9 PASS, 1 xfail |
| `test_event_type.py` | 5 | 1 PASS, 4 xfail | 1 PASS, 4 xfail |
| `test_hook_envelope.py` | 5 | 0 PASS, 5 xfail (D140) | 5 PASS, 0 xfail |
| `test_mcp_bridge.py` | 5 | 0 PASS, 5 xfail | 0 PASS, 5 xfail |
| `test_skill_workflow.py` | 5 | 0 PASS, 5 xfail | 0 PASS, 5 xfail |
| `test_e2e_smoke.py` | 5 | 3 PASS, 2 xfail | 3 PASS, 2 xfail |
| **Total** | **35** | **13 / 22** | **18 / 17** |

### Deferred items filed against this snapshot

See `docs/design/EAASP/DEFERRED_LEDGER.md` for full details.

- **D136** 🟡 P1-defer — Pre/PostToolUse hook not fired during probe turn
  (grid-only, blocks `test_hook_envelope.py` 5/5 on grid; Python runtime
  already compliant). Phase 2.5 W1 前置.
- **D137** 🟡 P1-defer — Multi-turn observability + MCP bridge live +
  PRE_COMPACT threshold (10 xfail tests across `test_event_type.py`,
  `test_proto_shape.py::test_events_stream_emits_stop_at_turn_end`,
  `test_mcp_bridge.py`). Phase 2.5 W1/W2 成熟期.
- **D138** 🟡 P2-defer — skill-workflow enforcement requires scriptable
  deny-path mock LLM (5 xfail tests in `test_skill_workflow.py`). Phase
  2.5 W1.
- **D139** 🔵 P3-defer — Double-terminate + unknown-session semantics
  underspecified (2 xfail tests in `test_e2e_smoke.py`). Phase 2.5 W1.
- **D140** 🟡 P1-defer — grid-runtime envelope-mode dispatch sites not
  calling `HookContext::with_event` (3–5 LOC hot fix; once applied,
  `test_hook_envelope.py --runtime=grid` graduates 0/5 → 5/5). Phase
  2.5 W1 前置.

## Versioning policy

### `v1.x.y` — backward-compatible test additions only
- Adding a new test case that describes existing behavior is a patch bump
  (`v1.0.0` → `v1.0.1`).
- Adding a whole new test file for an already-released area is a minor
  bump (`v1.0.x` → `v1.1.0`).
- A runtime that newly passes a previously-xfail test graduates via
  edits in-place; bump the patch and note the graduation in this file.

### `v2.x.y` — breaking changes
- Changes to proto, removed gRPC methods, changed event types, or any
  test change that invalidates a previously-green runtime require a
  new ADR.
- Breaking changes live in a parallel `tests/contract/contract_v2/`
  directory; the old `contract_v1/` snapshot stays frozen so previous
  runtimes remain certifiable until they migrate.

### Runtime divergence on an already-green test
A runtime that was passing a test at the point of its last release MUST
NOT start failing that test without a PR + review cycle. Divergence is
permissible only through either a CHANGELOG entry that graduates or
deletes the test, or a documented runtime-specific `xfail(strict=True)`
marker that references a ledger item.
