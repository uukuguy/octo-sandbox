# EAASP v2.0 — Cross-Runtime Contract Suite

Frozen contract-v1 tests consumed by all L1 runtimes (grid-runtime,
claude-code-runtime, and — once they land in Phase 2.5 W1/W2 —
goose-runtime and nanobot-runtime).

The authoritative policy lives in ADR-V2-017 §2 ("共享契约测试集"). This
directory is the mechanical enforcement of that policy.

## Layout

```
tests/contract/
├── conftest.py                  # --runtime CLI opt, marker config, fixtures
├── pyproject.toml               # pytest + grpcio-tools deps
├── test_harness_smoke.py        # S0.T1 scaffolding sanity check
├── harness/
│   ├── runtime_launcher.py      # RuntimeLauncher + RuntimeConfig
│   ├── mock_openai_server.py    # FastAPI mock for /v1/chat/completions
│   └── assertions.py            # shared schema constants + helpers
├── contract_v1/                 # Frozen contract v1.0.0 (2026-04-16)
│   ├── VERSION                  # single line: v1.0.0
│   ├── CHANGELOG.md             # version history + policy
│   ├── test_proto_shape.py
│   ├── test_event_type.py
│   ├── test_mcp_bridge.py
│   ├── test_skill_workflow.py
│   ├── test_hook_envelope.py    # ADR-V2-006 §2/§3
│   └── test_e2e_smoke.py
└── fixtures/
    └── hooks/                   # bash fixture hooks that capture envelope + env (T4/T5)
```

## Running

```bash
# Smoke tests (no runtime needed):
python -m pytest tests/contract/test_harness_smoke.py -v

# Full contract suite against a specific runtime (T4+):
python -m pytest tests/contract/contract_v1/ --runtime=grid -v
python -m pytest tests/contract/contract_v1/ --runtime=claude-code -v
```

The `--runtime` option gates all `contract_v1/` cases. Without it those
tests skip with a descriptive reason.

## Lifecycle

* **S0.T1** — this scaffolding (harness + smoke). No runtime dependency.
* **S0.T2** — 35 contract cases committed in RED state; drives D120.
* **S0.T3** — Rust `HookContext` envelope parity (fixes one RED subset).
* **S0.T4** — grid-runtime config + fixture wiring; contract GREEN on grid.
* **S0.T5** — claude-code-runtime config + fixture wiring; contract GREEN on claude-code.
* **S0.T6** — tag `contract-v1.0.0`; subsequent changes require a
  ledger-tracked version bump.

No test in this directory may be skipped silently after T6. Adding or
modifying assertions after the freeze requires a new contract version.

## Versioning & Freeze Policy

Contract v1.0.0 is the first stable snapshot, frozen on **2026-04-16** at
the close of Phase 2.5 S0. From this point on, changes to
`tests/contract/contract_v1/` are governed by a version-bump protocol
instead of ad-hoc edits.

### Adding new tests

Every new test case or file requires:

1. A version bump in `contract_v1/VERSION` (patch for single additions,
   minor for whole-file additions).
2. A new entry in `contract_v1/CHANGELOG.md` describing the addition,
   cross-referenced to a ledger item or ADR.
3. Validation on all currently-green runtimes before merge — a new test
   MUST NOT silently break a previously-passing runtime.

### Breaking changes

Any change that invalidates a previously-green runtime (proto method
removal, renamed event types, altered envelope contract, etc.) is a
major bump and requires:

1. A new ADR documenting the migration path.
2. A parallel `tests/contract/contract_v2/` directory so existing
   runtimes remain certifiable against v1 while they migrate.
3. CHANGELOG entries in both `contract_v1/` (marking the freeze final)
   and `contract_v2/` (starting v2.0.0).

### Reference files

- `contract_v1/VERSION` — authoritative version string (single line).
- `contract_v1/CHANGELOG.md` — per-version scope, validated runtimes,
  deferred items, and the full versioning policy details.
