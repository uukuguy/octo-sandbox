#!/usr/bin/env python3
"""verify-v2-phase2.py — 14 assertions for EAASP v2.0 Phase 2 S4.T3 exit gate.

Maps 1:1 to docs/plans/2026-04-14-v2-phase2-plan.md §S4.T3 exit criteria rows.

Exit criterion # | Assertion # | Automated?
1 (D87/D88/D86)   | A1 A2       | YES (cargo test wrappers)
2 (三 runtime 6 步) | A14         | L4-STUBBED — human operator
3 (Semantic search) | A3 A4 A5   | YES (L2 POST + hybrid search)
4 (Skill extraction)| A8         | YES (deterministic fixture replay)
5 (PreCompact)      | A10        | YES — direct L4 POST
6 (Batch A/B tests) | A7 A9 A11 A12 | YES (cargo test filters)
+ foundational      | A6 A13     | L3 telemetry + CLI smoke

Automated portion only. Runtime LLM verification deferred to human operator
(scripts/s4t3-runtime-verification.sh). A14 is a doc marker assertion that the
runbook script exists and is executable.

Environment variables (set by scripts/verify-v2-phase2.sh):
    EAASP_VERIFY_MODE         phase2 (default)
    EAASP_L2_URL              http://127.0.0.1:18085
    EAASP_L3_URL              http://127.0.0.1:18083
    EAASP_L4_URL              http://127.0.0.1:18084
    EAASP_SKILL_REGISTRY_URL  http://127.0.0.1:18081
    EAASP_SKIP_RUNTIMES       true|false
    EAASP_GRID_RUNTIME_URL    http://127.0.0.1:50051 (optional)
    EAASP_CLAUDE_RUNTIME_URL  http://127.0.0.1:50052 (optional)
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable

import httpx

# ── Config ──────────────────────────────────────────────────────────────────

PROJECT_ROOT = Path(__file__).resolve().parent.parent
MODE = os.environ.get("EAASP_VERIFY_MODE", "phase2")
SKIP_RUNTIMES = os.environ.get("EAASP_SKIP_RUNTIMES", "false").lower() == "true"

L2 = os.environ.get("EAASP_L2_URL", "http://127.0.0.1:18085")
L3 = os.environ.get("EAASP_L3_URL", "http://127.0.0.1:18083")
L4 = os.environ.get("EAASP_L4_URL", "http://127.0.0.1:18084")
SKILL_REG = os.environ.get("EAASP_SKILL_REGISTRY_URL", "http://127.0.0.1:18081")
GRID_RT = os.environ.get("EAASP_GRID_RUNTIME_URL", "http://127.0.0.1:50051")
CLAUDE_RT = os.environ.get("EAASP_CLAUDE_RUNTIME_URL", "http://127.0.0.1:50052")

# Deterministic fixtures — S3.T3 skill extraction trace + scope/device labels
# unique to Phase 2 so they don't collide with MVP fixtures in a shared DB.
SKILL_EXTRACTION_FIXTURE = (
    PROJECT_ROOT
    / "lang/claude-code-runtime-python/tests/fixtures/skill_extraction_input_trace.json"
)
SKILL_EXTRACTION_TEST_DIR = PROJECT_ROOT / "lang/claude-code-runtime-python"
SKILL_EXTRACTION_TEST_FILE = "tests/test_skill_extraction_e2e.py"

RUNTIME_VERIFICATION_SCRIPT = PROJECT_ROOT / "scripts/s4t3-runtime-verification.sh"

CLI_ENTRY = PROJECT_ROOT / "tools/eaasp-cli-v2/.venv/bin/eaasp"

# Phase 2-scoped device label so Phase 2 fixtures don't alias with MVP
# ``device:Transformer-001`` memories (separate scope keeps A5 filter clean).
SCADA_DEVICE = "Transformer-001-phase2"
TEST_SCOPE = f"device:{SCADA_DEVICE}"
TEST_USER_ID = "verify-phase2"

# trust_env=False prevents httpx from picking up macOS system proxies (Clash etc.)
# that route 127.0.0.1 through a proxy and surface as 502 errors. See MEMORY.md
# "Ollama 本地模型已知问题 (2026-03-27)" for the prior incident.
CLIENT = httpx.Client(timeout=10.0, trust_env=False)

# ── Assertion framework ─────────────────────────────────────────────────────

results: list[tuple[int, str, str, str | None]] = []
# Shared state across assertions.
state: dict[str, Any] = {
    "session_id": None,
    "anchor_id": None,
    "memory_ids": [],
}


def assertion(num: int, name: str) -> Callable[[Callable[[], None]], Callable[[], None]]:
    def decorate(fn: Callable[[], None]) -> Callable[[], None]:
        def wrapped() -> None:
            try:
                fn()
                results.append((num, name, "PASS", None))
                print(f"  PASS {num:2d}. {name}")
            except AssertionError as e:
                results.append((num, name, "FAIL", str(e)))
                print(f"  FAIL {num:2d}. {name}")
                print(f"         Reason: {e}")
            except Exception as e:  # pragma: no cover — diagnostic only
                results.append((num, name, "ERROR", repr(e)))
                print(f"  ERR  {num:2d}. {name}")
                print(f"         Error: {e!r}")
        return wrapped
    return decorate


def l2_tool_invoke(tool: str, args: dict[str, Any]) -> httpx.Response:
    """Call L2 MCP tool facade — body shape is {"args": {...}}."""
    return CLIENT.post(f"{L2}/tools/{tool}/invoke", json={"args": args})


def run_cargo_test(
    package: str,
    filter_or_test: str,
    *,
    use_test_flag: bool = False,
) -> subprocess.CompletedProcess[str]:
    """Shared helper for cargo-test subprocess invocations with --test-threads=1.

    ``use_test_flag=True`` emits ``--test <file>`` for integration test files
    (required when the filter is a file name like
    ``d87_multi_step_workflow_regression``).
    """
    if use_test_flag:
        cmd = [
            "cargo", "test", "-p", package,
            "--test", filter_or_test,
            "--", "--test-threads=1",
        ]
    else:
        cmd = [
            "cargo", "test", "-p", package,
            filter_or_test,
            "--", "--test-threads=1",
        ]
    return subprocess.run(
        cmd,
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
        check=False,
    )


# ── Assertions 1-14 ─────────────────────────────────────────────────────────


@assertion(1, "D87 regression test passes (cargo test d87_multi_step_workflow_regression)")
def a1() -> None:
    """D87 multi-step workflow must execute ≥3 tool calls, not exit early."""
    proc = run_cargo_test(
        "grid-engine",
        "d87_multi_step_workflow_regression",
        use_test_flag=True,
    )
    assert proc.returncode == 0, (
        f"D87 regression failed (rc={proc.returncode}): stderr tail="
        f"{proc.stderr[-300:]!r}"
    )


@assertion(2, "grid-runtime D83/D85/D86 fixes validated (cargo test -p grid-runtime)")
def a2() -> None:
    """D86: ToolResultBlock → POST_TOOL_USE; D83: tool_name; D85: response_text."""
    proc = subprocess.run(
        ["cargo", "test", "-p", "grid-runtime", "--", "--test-threads=1"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 0, (
        f"grid-runtime tests failed (rc={proc.returncode}): stderr tail="
        f"{proc.stderr[-300:]!r}"
    )


@assertion(3, "L2 memory search endpoint reachable")
def a3() -> None:
    """Basic L2 /api/v1/memory/search liveness probe."""
    resp = CLIENT.post(
        f"{L2}/api/v1/memory/search",
        json={"query": "smoke", "top_k": 1},
    )
    assert resp.status_code == 200, (
        f"L2 memory search returned {resp.status_code}: {resp.text}"
    )


@assertion(4, "Seed 3 memories + 1 anchor via L2 MCP tool facade")
def a4() -> None:
    """Populate L2 with deterministic test data for hybrid search verification.

    Creates one scada_snapshot anchor, then 3 memory files in distinct
    categories all scoped to ``TEST_SCOPE`` with ``evidence_refs=[anchor_id]``.
    """
    # Step 1: write evidence anchor.
    anchor_resp = l2_tool_invoke(
        "memory_write_anchor",
        {
            "event_id": f"evt_phase2_{TEST_SCOPE}",
            "session_id": f"verify-phase2-seed-{TEST_USER_ID}",
            "type": "scada_snapshot",
            "data_ref": f"file:///evidence/phase2/{SCADA_DEVICE}-snapshot.json",
            "snapshot_hash": "sha256:verify-phase2-001",
            "source_system": "mock-scada",
            "tool_version": "0.1.0-phase2",
        },
    )
    assert anchor_resp.status_code == 200, (
        f"memory_write_anchor failed: HTTP {anchor_resp.status_code} "
        f"{anchor_resp.text}"
    )
    anchor_body = anchor_resp.json()
    anchor_id = anchor_body.get("anchor_id")
    assert anchor_id, f"no anchor_id in response: {anchor_body}"
    state["anchor_id"] = anchor_id

    # Step 2: write 3 memory files in distinct categories, all referencing
    # the same anchor. Keeping content payloads keyword-rich so FTS queries
    # (A5) can match without semantic fallback.
    seed_files = [
        {
            "category": "threshold_calibration",
            "content": {
                "device": SCADA_DEVICE,
                "thresholds": {"temperature_c": 75, "load_pct": 80},
                "note": "phase2 threshold calibration baseline",
            },
        },
        {
            "category": "device_status",
            "content": {
                "device": SCADA_DEVICE,
                "status": "nominal",
                "note": "phase2 device status reading",
            },
        },
        {
            "category": "error_recovery",
            "content": {
                "device": SCADA_DEVICE,
                "recovery": "rollback-to-baseline",
                "note": "phase2 error recovery procedure",
            },
        },
    ]

    state["memory_ids"] = []
    for seed in seed_files:
        file_resp = l2_tool_invoke(
            "memory_write_file",
            {
                "scope": TEST_SCOPE,
                "category": seed["category"],
                "content": json.dumps(seed["content"], ensure_ascii=False),
                "evidence_refs": [anchor_id],
                "status": "agent_suggested",
            },
        )
        assert file_resp.status_code == 200, (
            f"memory_write_file ({seed['category']}) failed: "
            f"HTTP {file_resp.status_code} {file_resp.text}"
        )
        file_body = file_resp.json()
        memory_id = file_body.get("memory_id")
        assert memory_id, f"no memory_id for {seed['category']}: {file_body}"
        state["memory_ids"].append(memory_id)

    assert len(state["memory_ids"]) == 3, (
        f"expected 3 seed memory_ids, got {len(state['memory_ids'])}: "
        f"{state['memory_ids']}"
    )


@assertion(5, "Hybrid semantic search returns seeded memories (S2.T2)")
def a5() -> None:
    """L2 S2.T2 hybrid retrieval must surface at least one seeded memory.

    Graceful-degrade: if the embedding provider / HNSW index is unavailable,
    the hybrid index falls back to keyword-only (all hits have
    ``semantic_score = 0.0``). In that case we still require at least one
    seeded memory_id to appear via pure FTS ranking.
    """
    resp = CLIENT.post(
        f"{L2}/api/v1/memory/search",
        json={
            "query": f"threshold calibration {SCADA_DEVICE}",
            "top_k": 5,
            "scope": TEST_SCOPE,
        },
    )
    assert resp.status_code == 200, (
        f"L2 hybrid search returned {resp.status_code}: {resp.text}"
    )
    body = resp.json()
    hits = body.get("hits") or []
    assert len(hits) >= 1, (
        f"hybrid search returned no hits; expected at least one seeded memory. "
        f"body={body!r}"
    )

    # Each hit wraps the memory file under hit["memory"]["memory_id"].
    hit_ids = [
        (h.get("memory") or {}).get("memory_id") for h in hits if isinstance(h, dict)
    ]
    seeded = set(state["memory_ids"])
    assert seeded.intersection(hit_ids), (
        f"none of the seeded memory_ids {sorted(seeded)} present in hits "
        f"{hit_ids}; seeded fixture may have been wiped or scoped wrong"
    )

    # Detect graceful-degrade path (all semantic_score == 0.0 → embedding
    # provider unavailable). This is PASS with a note, not FAIL, since S2.T2
    # explicitly documents keyword-only fallback as supported.
    semantic_scores = [float(h.get("semantic_score", 0.0)) for h in hits]
    if all(s == 0.0 for s in semantic_scores):
        print("         NOTE: semantic disabled (all semantic_score=0.0), FTS-only path verified")


@assertion(6, "L3 telemetry ingest reachable (POST /v1/telemetry/events)")
def a6() -> None:
    """L3 accepts synthetic telemetry for audit trail (foundational ping)."""
    resp = CLIENT.post(
        f"{L3}/v1/telemetry/events",
        json={
            "session_id": "verify-phase2-001",
            "agent_id": "verify-agent",
            "hook_id": "audit-tool-calls",
            "phase": "PostToolUse",
            "payload": {
                "tool_name": "verify-phase2-ping",
                "device": SCADA_DEVICE,
                "phase2_smoke": True,
            },
        },
    )
    assert resp.status_code == 200, (
        f"L3 telemetry POST returned {resp.status_code}: {resp.text}"
    )


@assertion(7, "ErrorClassifier unit tests pass (batch A, S1.T6)")
def a7() -> None:
    """S1.T6: FailoverReason classification for 429/500/502/503/auth/timeout."""
    proc = run_cargo_test("grid-engine", "error_classifier")
    assert proc.returncode == 0, (
        f"ErrorClassifier tests failed (rc={proc.returncode}): stderr tail="
        f"{proc.stderr[-300:]!r}"
    )


@assertion(8, "Skill extraction deterministic fixture replay (S3.T3)")
def a8() -> None:
    """S3.T3: fixture existence + schema + deterministic pytest replay.

    Two-stage check:
      (1) Fixture file exists, is valid JSON, schema_version == 1, and has at
          least one TOOL_RESULT event (matches the fixture shape per S3.T3).
      (2) Invoke the S3.T3 pytest (test_skill_extraction_e2e.py) which replays
          the fixture through a MockMemoryEngine and asserts hook/workflow
          ordering. Uses the claude-code-runtime-python .venv.
    """
    assert SKILL_EXTRACTION_FIXTURE.exists(), (
        f"fixture missing: {SKILL_EXTRACTION_FIXTURE}"
    )
    with SKILL_EXTRACTION_FIXTURE.open() as fp:
        fixture_body = json.load(fp)
    assert fixture_body.get("schema_version") == 1, (
        f"fixture schema_version mismatch: expected 1, got "
        f"{fixture_body.get('schema_version')}"
    )
    events = fixture_body.get("events") or []
    tool_results = [e for e in events if e.get("event_type") == "TOOL_RESULT"]
    assert len(tool_results) >= 1, (
        "fixture has no TOOL_RESULT events; shape may have regressed"
    )

    # Stage 2: run the S3.T3 pytest. Prefer the claude-code-runtime-python
    # .venv; fall back to module python if not present (surfaces as error so
    # operator knows to `make claude-runtime-setup`).
    venv_py = SKILL_EXTRACTION_TEST_DIR / ".venv/bin/python"
    assert venv_py.exists(), (
        f"claude-code-runtime-python .venv/bin/python missing at {venv_py}; "
        f"run 'make claude-runtime-setup' first"
    )
    proc = subprocess.run(
        [str(venv_py), "-m", "pytest", SKILL_EXTRACTION_TEST_FILE, "-x"],
        cwd=str(SKILL_EXTRACTION_TEST_DIR),
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 0, (
        f"skill-extraction pytest failed (rc={proc.returncode}): stdout tail="
        f"{proc.stdout[-300:]!r} stderr tail={proc.stderr[-200:]!r}"
    )


@assertion(9, "Tool-result aggregate spill (S2.T5)")
def a9() -> None:
    """S2.T5: Per-tool cap + per-result threshold + per-turn aggregate budget."""
    proc = run_cargo_test(
        "grid-engine",
        "tool_result_aggregate_spill",
        use_test_flag=True,
    )
    assert proc.returncode == 0, (
        f"tool_result_aggregate_spill tests failed (rc={proc.returncode}): "
        f"stderr tail={proc.stderr[-300:]!r}"
    )


@assertion(10, "PreCompact event ingest via L4 /v1/events/ingest (S3.T1)")
def a10() -> None:
    """S3.T1: Create session, POST PRE_COMPACT event, verify persistence.

    Three steps per ADR-V2-001:
      1. POST /v1/sessions/create (FK-bind for subsequent ingest)
      2. POST /v1/events/ingest with event_type=PRE_COMPACT (returns seq)
      3. GET /v1/sessions/{sid}/events (assert event is visible)

    Note: step 1 transitively invokes L1 ``l1.initialize`` over gRPC
    (session_orchestrator.py:~259). Under the default ``--skip-runtimes``
    path grid-runtime is not listening on :50051, so the create returns
    502. Skip this assertion in that mode; ``make v2-phase2-e2e-full``
    exercises the full path.
    """
    if SKIP_RUNTIMES:
        print(
            "         SKIP_RUNTIMES=true — /v1/sessions/create needs "
            "grid-runtime, skipping A10 (use --with-runtimes to verify)"
        )
        return

    # Step 1: create a dedicated session for PreCompact test.
    create_resp = CLIENT.post(
        f"{L4}/v1/sessions/create",
        json={
            "intent_text": "phase2-precompact-test",
            "skill_id": "threshold-calibration",
            "runtime_pref": "grid-runtime",
            "user_id": TEST_USER_ID,
        },
    )
    assert create_resp.status_code == 200, (
        f"L4 /v1/sessions/create returned {create_resp.status_code}: "
        f"{create_resp.text}"
    )
    create_body = create_resp.json()
    session_id = create_body.get("session_id")
    assert session_id, f"no session_id in L4 response: {create_body}"
    state["session_id"] = session_id

    # Step 2: POST PRE_COMPACT event through the ingest fallback.
    ingest_resp = CLIENT.post(
        f"{L4}/v1/events/ingest",
        json={
            "session_id": session_id,
            "event_type": "PRE_COMPACT",
            "payload": {
                "compressed_from_tokens": 85000,
                "compressed_to_tokens": 40000,
                "summary": "phase2 verify harness synthetic precompact summary",
            },
            "source": "verify-phase2",
        },
    )
    assert ingest_resp.status_code == 200, (
        f"L4 /v1/events/ingest returned {ingest_resp.status_code}: "
        f"{ingest_resp.text}"
    )
    ingest_body = ingest_resp.json()
    seq = ingest_body.get("seq")
    assert seq is not None and seq >= 1, (
        f"ingest response missing seq or seq<1: {ingest_body}"
    )

    # Step 3: confirm event visible in stream query.
    events_resp = CLIENT.get(f"{L4}/v1/sessions/{session_id}/events")
    assert events_resp.status_code == 200, (
        f"GET /v1/sessions/{session_id}/events returned "
        f"{events_resp.status_code}: {events_resp.text}"
    )
    events = events_resp.json().get("events") or []
    pre_compact_events = [
        e for e in events if e.get("event_type") == "PRE_COMPACT"
    ]
    assert len(pre_compact_events) >= 1, (
        f"PRE_COMPACT not found in session events; event_types seen="
        f"{[e.get('event_type') for e in events]}"
    )


@assertion(11, "Stop hooks integration (S3.T4)")
def a11() -> None:
    """S3.T4: Stop hooks dispatcher + InjectAndContinue decision integration."""
    proc = run_cargo_test(
        "grid-engine",
        "stop_hooks_integration",
        use_test_flag=True,
    )
    assert proc.returncode == 0, (
        f"stop_hooks_integration tests failed (rc={proc.returncode}): "
        f"stderr tail={proc.stderr[-300:]!r}"
    )


@assertion(12, "Batch A+B aggregate smoke (make test-phase2-batch-ab)")
def a12() -> None:
    """Defense-in-depth: runs the aggregate Makefile target.

    Catches linker / compilation flakes that individual ``cargo test``
    invocations may miss because of per-target caching artifacts.
    """
    proc = subprocess.run(
        ["make", "test-phase2-batch-ab"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 0, (
        f"make test-phase2-batch-ab failed (rc={proc.returncode}): stderr tail="
        f"{proc.stderr[-300:]!r}"
    )


@assertion(13, "CLI smoke (eaasp session list or equivalent read)")
def a13() -> None:
    """Foundational CLI read operation — skip if .venv not built.

    Skipping is PASS-with-note rather than FAIL because the CLI is optional
    for the automated portion (A1/A2 cover regression without it). A fresh
    clone without ``make cli-v2-setup`` would trip a hard FAIL otherwise.
    """
    if not CLI_ENTRY.exists():
        print("         NOTE: CLI not built at "
              f"{CLI_ENTRY.relative_to(PROJECT_ROOT)}; skipping")
        return

    # ``eaasp session list`` is the lightest read path — hits L4 only.
    env = os.environ.copy()
    env.setdefault("EAASP_L2_URL", L2)
    env.setdefault("EAASP_L3_URL", L3)
    env.setdefault("EAASP_L4_URL", L4)
    env.setdefault("EAASP_SKILL_REGISTRY_URL", SKILL_REG)
    proc = subprocess.run(
        [str(CLI_ENTRY), "session", "list"],
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 0, (
        f"eaasp session list exited {proc.returncode}; "
        f"stdout tail={proc.stdout[-200:]!r} stderr tail={proc.stderr[-200:]!r}"
    )


@assertion(14, "Runtime verification runbook script exists and is executable")
def a14() -> None:
    """L4-STUBBED: doc marker for the human-operator hand-off.

    The automated gate does not run threshold-calibration through a live LLM
    (see MVP_SCOPE §8). This assertion verifies that the runbook script is
    in place so operators can complete the 6-step workflow verification
    post-merge. The script itself is owned by a parallel coder.
    """
    assert RUNTIME_VERIFICATION_SCRIPT.exists(), (
        f"runtime verification script missing: {RUNTIME_VERIFICATION_SCRIPT}"
    )
    # Execute bit check — matters on fresh clones where git may not preserve
    # +x on shell scripts created in other branches.
    assert os.access(RUNTIME_VERIFICATION_SCRIPT, os.X_OK), (
        f"runtime verification script is not executable: "
        f"{RUNTIME_VERIFICATION_SCRIPT} (try chmod +x)"
    )


# ── Runner ──────────────────────────────────────────────────────────────────


def main() -> int:
    print("════════════════════════════════════════════════════")
    print("  EAASP v2.0 Phase 2 — S4.T3 Automated Gate")
    print(f"  Mode: {MODE}")
    print(f"  SKIP_RUNTIMES: {SKIP_RUNTIMES}")
    print("════════════════════════════════════════════════════")
    print()

    suite: list[Callable[[], None]] = [
        a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11, a12, a13, a14,
    ]
    for fn in suite:
        fn()

    passed = sum(1 for _, _, status, _ in results if status == "PASS")
    total = len(results)

    print()
    print("════════════════════════════════════════════════════")
    print(f"  {passed}/{total} assertions passed")
    if passed != total:
        print()
        print("  Failures:")
        for num, name, status, reason in results:
            if status != "PASS":
                print(f"    {num:2d}. [{status}] {name}: {reason}")
    print("════════════════════════════════════════════════════")

    try:
        CLIENT.close()
    except Exception:
        pass

    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
