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
├── contract_v1/                 # S0.T2 frozen contract tests
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
