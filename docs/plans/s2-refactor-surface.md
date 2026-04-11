# Stage S2 Refactor Surface (from S1.T3)

Generated: 2026-04-11 at end of S1.T3. These are the exact files that reference the removed v1 proto. S2 tasks must rewrite each reference to v2.

## S2.T1 — `grid-runtime` (Rust)

| File | Line | Reference |
|------|------|-----------|
| `crates/grid-runtime/src/lib.rs` | 16-17 | doc comment pointing at `proto/eaasp/runtime/v1/runtime.proto` and `common/v1/common.proto` |
| `crates/grid-runtime/src/lib.rs` | 29 | `tonic::include_proto!("eaasp.common.v1")` → replace with `tonic::include_proto!("eaasp.runtime.v2")` (single package) |
| `crates/grid-runtime/src/lib.rs` | 34 | `tonic::include_proto!("eaasp.runtime.v1")` → **delete** (merged into v2 package) |
| `crates/grid-runtime/src/contract.rs` | 19 | doc comment |
| `crates/grid-runtime/src/contract.rs` | (body) | `SessionPayload` struct now has 5 priority blocks (P1-P5); field access sites need rewriting |
| `crates/grid-runtime/src/harness.rs` | — | `SessionPayload` structured handling per v2 priority blocks |
| `crates/grid-runtime/tests/*.rs` | — | all tests need new `SessionPayload` construction + priority block assertions |

**New files to create:**
- `crates/grid-runtime/src/session_payload.rs` — `trim_for_budget()` helper implementing P5→P4→P3 trimming order
- `crates/grid-runtime/tests/v2_session_payload_test.rs` — P1-P5 priority block tests

## S2.T1 — `grid-hook-bridge` (Rust, piggybacks on T1 because it imports common)

| File | Line | Reference |
|------|------|-----------|
| `crates/grid-hook-bridge/src/lib.rs` | 16 | `tonic::include_proto!("eaasp.common.v1")` → `tonic::include_proto!("eaasp.runtime.v2")` |
| `crates/grid-hook-bridge/src/lib.rs` | 21 | `tonic::include_proto!("eaasp.hook.v1")` → **delete** (merged into v2 package) |

Note: v2 `hook.proto` now imports `runtime.proto` directly for `HookEventType`. The hook-bridge service types are all under `eaasp.runtime.v2`.

## S2.T2 — `eaasp-certifier` (Rust)

| File | Line | Reference |
|------|------|-----------|
| `tools/eaasp-certifier/src/lib.rs` | 15 | `tonic::include_proto!("eaasp.common.v1")` → `"eaasp.runtime.v2"` |
| `tools/eaasp-certifier/src/lib.rs` | 20 | `tonic::include_proto!("eaasp.runtime.v1")` → **delete** |
| `tools/eaasp-certifier/src/checks/*` | — | add `is_must()` method; split MUST vs OPTIONAL reporting |

**New file:** `tools/eaasp-certifier/src/v2_must_methods.rs` — constants for 12 MUST + 4 OPTIONAL method names.

## S2.T3 — `hermes-runtime-python` (Python)

All imports live in three files:

| File | Lines |
|------|-------|
| `lang/hermes-runtime-python/src/hermes_runtime/service.py` | 13-14: `from eaasp.common.v1 ... / eaasp.runtime.v1 ...` |
| `lang/hermes-runtime-python/src/hermes_runtime/mapper.py` | 7-8 |
| `lang/hermes-runtime-python/src/hermes_runtime/governance_plugin/hook_bridge.py` | 11-12 |
| `lang/hermes-runtime-python/src/hermes_runtime/__main__.py` | 12 |
| `lang/hermes-runtime-python/src/hermes_runtime/_fix_proto_imports.py` | 3-4 (docstring / sys.path patching) |

Rewrite strategy: change imports from `eaasp.common.v1`/`eaasp.runtime.v1`/`eaasp.hook.v1` to a single `eaasp.runtime.v2` package (common + runtime + hook all live there). Use `make claude-runtime-proto` to regenerate stubs (hermes symlinks to claude-code-runtime-python's `_proto/`).

## S2.T4 — `claude-code-runtime-python` (Python)

| File | Lines |
|------|-------|
| `lang/claude-code-runtime-python/src/claude_code_runtime/service.py` | 14-15 |
| `lang/claude-code-runtime-python/src/claude_code_runtime/mapper.py` | 7-8 |
| `lang/claude-code-runtime-python/src/claude_code_runtime/__main__.py` | 10 |
| `lang/claude-code-runtime-python/tests/test_service.py` | 8-9 |
| `lang/claude-code-runtime-python/build_proto.py` | already updated in S1.T3; proto_files list points at v2 |

## Expected compile failures (end of S1.T3 — by design)

- `cargo check -p grid-runtime` ❌ — `eaasp.common.v1.rs` not found (`lib.rs:29`)
- `cargo check -p grid-hook-bridge` ❌ — `eaasp.common.v1.rs` not found (`lib.rs:16`)
- `cargo check -p eaasp-certifier` ❌ — `eaasp.common.v1.rs` not found (`lib.rs:15`)
- `uv run pytest` on `hermes-runtime-python` ❌ — `ModuleNotFoundError: eaasp.common.v1`
- `uv run pytest` on `claude-code-runtime-python` ❌ — same

All 3 Rust crates and 2 Python runtimes fail with the same class of error: tonic proto package renamed from `v1` to `v2`. That is the S2 refactor scope.
