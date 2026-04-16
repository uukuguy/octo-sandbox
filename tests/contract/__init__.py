"""EAASP v2.0 cross-runtime contract test suite.

This package hosts the shared contract-v1 test suite consumed by all L1
runtimes (grid-runtime, claude-code-runtime, goose-runtime, nanobot-runtime)
per ADR-V2-017 §2. Tests live under ``contract_v1/`` and reuse the
``harness/`` scaffolding (RuntimeLauncher, assertions, mock OpenAI server).
"""
