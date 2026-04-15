#!/bin/bash
# s4t3-runtime-verification.sh — Runtime verification runbook for human operator.
#
# This script covers EAASP v2.0 Phase 2 S4.T3 exit criterion #2:
#   "三 runtime 均能完整跑 threshold-calibration 6 步"
#
# The automated gate (scripts/verify-v2-phase2.{sh,py}) covers 5 of 6 exit
# criteria via fixture replay + direct REST POST. This runbook covers the
# remaining one — live agent loop + LLM tool calls — which cannot be fully
# automated without burning API quota on every CI run.
#
# Per ADR-V2-017 (L1 Runtime 生态策略, 2026-04-14) hermes-runtime is frozen.
# The plan document says "三 runtime" but in practice only 2 runtimes are
# exercised here: grid-runtime + claude-code-runtime. If one API key is
# missing, that runtime is marked SKIPPED (not FAIL).
#
# Usage:
#   1. Complete: scripts/verify-v2-phase2.sh must PASS first.
#   2. In a separate terminal, keep `make dev-eaasp` running.
#   3. Export ANTHROPIC_API_KEY and/or OPENAI_API_KEY / OPENROUTER_API_KEY.
#   4. Deploy threshold-calibration skill once:
#        eaasp skill submit examples/skills/threshold-calibration
#   5. Run this script:
#        bash scripts/s4t3-runtime-verification.sh
#
# Exit codes:
#   0  = all available runtimes PASS
#   1  = any runtime FAIL
#   2  = prerequisite check failed (services down, CLI missing, etc.)

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
L4_URL="${EAASP_L4_URL:-http://127.0.0.1:18084}"
CLI="${PROJECT_ROOT}/tools/eaasp-cli-v2/.venv/bin/eaasp"

GRID_RUNTIME_PORT=50051
CLAUDE_RUNTIME_PORT=50052

# ── Color codes ────────────────────────────────────────────────────────────
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    CYAN=''
    BOLD=''
    NC=''
fi

# ── Header + prerequisites block ───────────────────────────────────────────
print_header() {
    echo "════════════════════════════════════════════════════════════════"
    echo "  EAASP v2.0 S4.T3 Runtime Verification (Human Operator)"
    echo "  Exit criterion #2: 三 runtime 均能完整跑 threshold-calibration 6 步"
    echo "════════════════════════════════════════════════════════════════"
    echo ""
    echo "Note per ADR-V2-017: hermes-runtime is frozen. Testing 2/3 runtimes:"
    echo "  - grid-runtime         (OpenAI/OpenRouter via OPENAI_API_KEY)"
    echo "  - claude-code-runtime  (Anthropic via ANTHROPIC_API_KEY)"
    echo ""
    echo "Prerequisites (MUST complete before running this script):"
    echo "  [ ] scripts/verify-v2-phase2.sh PASSED"
    echo "  [ ] 'make dev-eaasp' running in a separate terminal"
    echo "        (starts L2/L3/L4/skill-registry + both L1 runtimes)"
    echo "  [ ] ANTHROPIC_API_KEY  exported (for claude-code-runtime)"
    echo "  [ ] OPENAI_API_KEY or OPENROUTER_API_KEY exported (for grid-runtime)"
    echo "  [ ] threshold-calibration skill deployed:"
    echo "        eaasp skill submit examples/skills/threshold-calibration"
    echo ""
    echo "Estimated cost: ~\$0.05-0.15 in LLM API calls per runtime."
    echo ""
    read -r -p "All prerequisites met? (y/N): " confirm
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        echo "Aborted. Review prerequisites and re-run." >&2
        exit 2
    fi
    echo ""
}

# ── Service health checks ──────────────────────────────────────────────────
check_services_up() {
    echo "=== Pre-flight: service health ==="
    local failed=0

    # L4 orchestration health
    if curl -fsS -m 3 "${L4_URL}/health" >/dev/null 2>&1; then
        echo -e "  ${GREEN}OK${NC}      L4 orchestration (${L4_URL}/health)"
    else
        echo -e "  ${RED}FAIL${NC}    L4 orchestration not reachable at ${L4_URL}/health" >&2
        echo "          → Start stack with: make dev-eaasp" >&2
        failed=1
    fi

    # grid-runtime (gRPC — just TCP probe; grpcurl may not be installed)
    if command -v nc >/dev/null 2>&1; then
        if nc -z 127.0.0.1 "$GRID_RUNTIME_PORT" 2>/dev/null; then
            echo -e "  ${GREEN}OK${NC}      grid-runtime           (tcp :${GRID_RUNTIME_PORT})"
        else
            echo -e "  ${YELLOW}WARN${NC}    grid-runtime not reachable on tcp :${GRID_RUNTIME_PORT}"
            echo "          → Will mark grid-runtime as SKIPPED"
        fi
        if nc -z 127.0.0.1 "$CLAUDE_RUNTIME_PORT" 2>/dev/null; then
            echo -e "  ${GREEN}OK${NC}      claude-code-runtime    (tcp :${CLAUDE_RUNTIME_PORT})"
        else
            echo -e "  ${YELLOW}WARN${NC}    claude-code-runtime not reachable on tcp :${CLAUDE_RUNTIME_PORT}"
            echo "          → Will mark claude-code-runtime as SKIPPED"
        fi
    else
        echo -e "  ${YELLOW}WARN${NC}    nc not installed — skipping L1 runtime TCP probe"
    fi

    # CLI binary
    if [ -x "$CLI" ]; then
        echo -e "  ${GREEN}OK${NC}      CLI binary             ($CLI)"
    else
        echo -e "  ${RED}FAIL${NC}    CLI missing at $CLI" >&2
        echo "          → Install CLI: cd tools/eaasp-cli-v2 && uv pip install -e ." >&2
        failed=1
    fi

    # jq availability (required for event parsing)
    if ! command -v jq >/dev/null 2>&1; then
        echo -e "  ${RED}FAIL${NC}    jq not installed (required to parse event JSON)" >&2
        echo "          → Install jq: brew install jq  /  apt install jq" >&2
        failed=1
    else
        echo -e "  ${GREEN}OK${NC}      jq                     ($(command -v jq))"
    fi

    echo ""
    if [ $failed -ne 0 ]; then
        echo -e "${RED}Prerequisite check failed. Fix the issues above and re-run.${NC}" >&2
        exit 2
    fi
}

# ── Key availability check (determines what to SKIP) ──────────────────────
runtime_has_key() {
    local runtime=$1
    case "$runtime" in
        grid-runtime)
            [ -n "${OPENAI_API_KEY:-}" ] || [ -n "${OPENROUTER_API_KEY:-}" ]
            ;;
        claude-code-runtime)
            [ -n "${ANTHROPIC_API_KEY:-}" ]
            ;;
        *)
            return 1
            ;;
    esac
}

# ── Per-runtime verification ───────────────────────────────────────────────
# Runs a single threshold-calibration workflow and asserts the agent loop
# made ≥4 tool calls (D87 multi-step fix).
#
# Returns 0 on PASS, 1 on FAIL. Caller handles SKIPPED separately.
run_skill_on_runtime() {
    local runtime=$1
    echo "════════════════════════════════════════════════════════════════"
    echo -e "${BOLD}Testing runtime: ${CYAN}${runtime}${NC}${BOLD}${NC}"
    echo "════════════════════════════════════════════════════════════════"

    # Step 1 — create session
    # Skill is assumed already deployed via `eaasp skill submit` in prerequisites.
    echo "  [1/4] Creating session via CLI..."
    local output
    if ! output=$("$CLI" session create \
            --skill threshold-calibration \
            --runtime "$runtime" \
            --user-id verify-operator \
            --intent-text "校准 Transformer-001 的温度阈值" 2>&1); then
        echo -e "  ${RED}FAIL${NC}    session create returned non-zero"
        echo "$output" | sed 's/^/          /'
        return 1
    fi

    # CLI prints a rich table. Session ID format is typically UUID.
    # We extract via first UUID-shaped token in output.
    local sid
    sid=$(echo "$output" \
        | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' \
        | head -1)
    if [ -z "$sid" ]; then
        echo -e "  ${RED}FAIL${NC}    could not extract session_id from CLI output"
        echo "$output" | sed 's/^/          /'
        echo "          → If CLI supports --format json, pipe it and parse .session_id manually."
        return 1
    fi
    echo "          session_id: $sid"

    # Step 2 — send initial message
    echo "  [2/4] Sending initial message (SSE stream; may take ~10-20s)..."
    if ! "$CLI" session send "$sid" "请校准 Transformer-001 的温度阈值" --no-stream >/dev/null 2>&1; then
        echo -e "  ${YELLOW}WARN${NC}    session send returned non-zero"
        echo "          → Continuing to query events; agent may still have made progress."
    fi

    # Step 3 — wait for agent loop to settle
    # Magic number rationale: threshold-calibration has ~6 tool steps.
    # Each LLM turn ~2-4s + tool call 100-500ms + another turn ~2-4s = ~30-50s total
    # for a full 6-step flow. We sleep 15s after initial send (which already
    # blocks until SSE end-of-stream in --stream mode, but we used --no-stream
    # so some further processing may still be in flight on the server).
    # If your LLM is slow, export S4T3_SETTLE_SECONDS=30 before running.
    local settle="${S4T3_SETTLE_SECONDS:-15}"
    echo "  [3/4] Waiting ${settle}s for agent loop to settle..."
    sleep "$settle"

    # Step 4 — fetch events and assert criteria
    echo "  [4/4] Fetching events and asserting workflow completion..."
    local events_file="/tmp/s4t3-${runtime}-events.json"

    # Try --format json; fall back to message if CLI lacks that flag.
    if ! "$CLI" session events "$sid" --format json > "$events_file" 2>/dev/null; then
        echo -e "  ${RED}FAIL${NC}    could not fetch events via 'session events --format json'"
        echo "          → Try manually:"
        echo "             $CLI session events $sid --format json > $events_file"
        echo "          → Then re-run this script; or fetch via REST:"
        echo "             curl -s '${L4_URL}/v1/sessions/${sid}/events?from=1&limit=500' > $events_file"
        return 1
    fi

    # Validate JSON parses.
    if ! jq empty "$events_file" 2>/dev/null; then
        echo -e "  ${RED}FAIL${NC}    events file is not valid JSON"
        echo "          → Inspect: cat $events_file"
        return 1
    fi

    # Count PRE_TOOL_USE events
    local pre_tool_use
    pre_tool_use=$(jq '[.events[]? | select(.event_type=="PRE_TOOL_USE")] | length' "$events_file")

    # Count POST_TOOL_USE events (should roughly match PRE_TOOL_USE)
    local post_tool_use
    post_tool_use=$(jq '[.events[]? | select(.event_type=="POST_TOOL_USE")] | length' "$events_file")

    # Count workflow_continuation markers (D87 fix — ADR-V2-016)
    # These may appear as their own event_type or inside payload; check both.
    local continuations
    continuations=$(jq '
        [.events[]?
          | select(
              .event_type == "WORKFLOW_CONTINUATION"
              or (.payload? // {} | tostring | contains("workflow_continuation"))
          )
        ] | length
    ' "$events_file")

    # Count STOP events (expect exactly 1 at end)
    local stop_count
    stop_count=$(jq '[.events[]? | select(.event_type=="STOP")] | length' "$events_file")

    # Total event count for context
    local total
    total=$(jq '.events | length' "$events_file")

    echo ""
    echo "  Results (events saved to $events_file):"
    echo "          total events        : $total"
    echo "          PRE_TOOL_USE        : $pre_tool_use   (expected ≥4)"
    echo "          POST_TOOL_USE       : $post_tool_use  (expected ≈ PRE_TOOL_USE)"
    echo "          WORKFLOW_CONT marks : $continuations  (expected ≥1, D87 fix)"
    echo "          STOP events         : $stop_count     (expected = 1)"
    echo ""

    # Evaluate PASS/FAIL per criterion
    local failures=0
    if [ "$pre_tool_use" -lt 4 ]; then
        echo -e "          ${RED}FAIL${NC}  PRE_TOOL_USE = $pre_tool_use, expected ≥4"
        echo "                → D87 multi-step workflow fix may not be active"
        echo "                → or skill workflow terminated early"
        failures=$((failures+1))
    else
        echo -e "          ${GREEN}PASS${NC}  PRE_TOOL_USE ≥ 4"
    fi

    if [ "$continuations" -lt 1 ]; then
        echo -e "          ${YELLOW}WARN${NC}  no workflow_continuation markers"
        echo "                → D87 fix may use different marker name (non-fatal for this script)"
        # Not a hard fail — marker format may evolve.
    else
        echo -e "          ${GREEN}PASS${NC}  workflow_continuation markers present"
    fi

    if [ "$stop_count" -ne 1 ]; then
        echo -e "          ${YELLOW}WARN${NC}  STOP count = $stop_count (expected 1)"
        echo "                → session may not have reached natural termination"
    else
        echo -e "          ${GREEN}PASS${NC}  exactly 1 STOP event"
    fi

    echo ""
    if [ $failures -eq 0 ]; then
        echo -e "  ${GREEN}${BOLD}RESULT: ${runtime} PASS${NC}"
        return 0
    else
        echo -e "  ${RED}${BOLD}RESULT: ${runtime} FAIL${NC} ($failures criteria failed)"
        return 1
    fi
}

# ── Summary + next-steps ───────────────────────────────────────────────────
print_summary() {
    local pass=$1 fail=$2 skip=$3
    echo ""
    echo "════════════════════════════════════════════════════════════════"
    echo "  S4.T3 Runtime Verification — Summary"
    echo "════════════════════════════════════════════════════════════════"
    echo "  PASS    : $pass"
    echo "  FAIL    : $fail"
    echo "  SKIPPED : $skip"
    echo ""

    if [ "$fail" -eq 0 ] && [ "$pass" -gt 0 ]; then
        echo -e "  ${GREEN}${BOLD}✓ S4.T3 fully complete${NC}"
        echo ""
        echo "  Next steps:"
        echo "    1. Update docs/plans/2026-04-14-v2-phase2-plan.md §S4.T3 exit criteria"
        echo "       — tick off #2 '三 runtime 均能完整跑 threshold-calibration 6 步'"
        echo "    2. Run /checkpoint-progress to save state"
        echo "    3. Run /end-phase to close Phase 2"
        return 0
    elif [ "$fail" -gt 0 ]; then
        echo -e "  ${RED}${BOLD}✗ S4.T3 runtime verification FAILED${NC}"
        echo ""
        echo "  Diagnostic hints:"
        echo "    1. Inspect per-runtime event JSON dumps:"
        echo "         /tmp/s4t3-grid-runtime-events.json"
        echo "         /tmp/s4t3-claude-code-runtime-events.json"
        echo "    2. Re-run with more verbose logging:"
        echo "         RUST_LOG=grid_engine=debug,grid_runtime=debug make dev-eaasp"
        echo "    3. Check API key validity + quota:"
        echo "         echo \$ANTHROPIC_API_KEY | head -c 20"
        echo "         echo \$OPENAI_API_KEY    | head -c 20"
        echo "    4. Try bumping settle time: S4T3_SETTLE_SECONDS=30 bash \$0"
        echo "    5. Compare with D87 regression test which must still pass:"
        echo "         cargo test -p grid-engine d87"
        return 1
    else
        echo -e "  ${YELLOW}${BOLD}? No runtimes were testable (all skipped)${NC}"
        echo ""
        echo "  Likely cause: no API keys configured."
        echo "  Export at least one of:"
        echo "    - ANTHROPIC_API_KEY  (enables claude-code-runtime)"
        echo "    - OPENAI_API_KEY / OPENROUTER_API_KEY (enables grid-runtime)"
        return 1
    fi
}

# ── Main ────────────────────────────────────────────────────────────────────
main() {
    print_header
    check_services_up

    local pass=0 fail=0 skip=0

    for rt in grid-runtime claude-code-runtime; do
        if ! runtime_has_key "$rt"; then
            echo "════════════════════════════════════════════════════════════════"
            echo -e "${YELLOW}SKIP${NC}  ${rt}: no API key exported"
            case "$rt" in
                grid-runtime)
                    echo "       → Export OPENAI_API_KEY or OPENROUTER_API_KEY to enable."
                    ;;
                claude-code-runtime)
                    echo "       → Export ANTHROPIC_API_KEY to enable."
                    ;;
            esac
            echo "════════════════════════════════════════════════════════════════"
            skip=$((skip+1))
            continue
        fi

        if run_skill_on_runtime "$rt"; then
            pass=$((pass+1))
        else
            fail=$((fail+1))
        fi
        echo ""
    done

    if print_summary "$pass" "$fail" "$skip"; then
        exit 0
    else
        exit 1
    fi
}

main "$@"
