#!/usr/bin/env bash
# Phase 2.5 Manual E2E Verification Runbook
# Usage: bash scripts/phase2_5-runtime-verification.sh [--runtime RUNTIME]
#
# Drives skill-extraction through each L1 runtime (or a specified single one).
# Signs off each runtime interactively and writes phase2_5-verification-log.txt.
#
# Mirrors: scripts/s4t3-runtime-verification.sh (Phase 2)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
LOG_FILE="${REPO_ROOT}/phase2_5-verification-log.txt"

RUNTIMES=(grid claude-code goose nanobot)

# Allow single-runtime override: --runtime <name>
if [[ "${1:-}" == "--runtime" && -n "${2:-}" ]]; then
  RUNTIMES=("$2")
fi

echo "================================================================"
echo "  EAASP v2.0 Phase 2.5 — Manual E2E Verification"
echo "  $(date '+%Y-%m-%d %H:%M:%S')"
echo "================================================================"
echo ""
echo "You will drive skill-extraction through ${#RUNTIMES[@]} runtime(s)."
echo "For each runtime, start it, trigger skill-extraction, inspect events,"
echo "then sign off."
echo ""
echo "Log file: ${LOG_FILE}"
echo ""

# Reset log
echo "# Phase 2.5 Verification Log — $(date '+%Y-%m-%d %H:%M:%S')" > "${LOG_FILE}"
echo "" >> "${LOG_FILE}"

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

for rt in "${RUNTIMES[@]}"; do
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  Runtime: ${rt}"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  case "${rt}" in
    grid)
      echo ""
      echo "[Step 1] Start grid-runtime:"
      echo "  cargo run -p grid-runtime -- --port 50061"
      echo "  (or: RUST_LOG=debug cargo run -p grid-runtime -- --port 50061)"
      ;;
    claude-code)
      echo ""
      echo "[Step 1] Start claude-code-runtime:"
      echo "  cd lang/claude-code-runtime-python"
      echo "  CLAUDE_CODE_RUNTIME_GRPC_ADDR=0.0.0.0:50062 \\"
      echo "    ANTHROPIC_API_KEY=\$ANTHROPIC_API_KEY \\"
      echo "    .venv/bin/python -m claude_code_runtime"
      ;;
    goose)
      echo ""
      echo "[Step 1] Start eaasp-goose-runtime (requires goose binary):"
      echo "  GOOSE_BIN=\$(which goose || echo '') \\"
      echo "    GOOSE_RUNTIME_GRPC_ADDR=0.0.0.0:50063 \\"
      echo "    cargo run -p eaasp-goose-runtime"
      ;;
    nanobot)
      echo ""
      echo "[Step 1] Start nanobot-runtime:"
      echo "  cd lang/nanobot-runtime-python"
      echo "  OPENAI_BASE_URL=\$OPENAI_BASE_URL \\"
      echo "    OPENAI_API_KEY=\$OPENAI_API_KEY \\"
      echo "    OPENAI_MODEL_NAME=\$OPENAI_MODEL_NAME \\"
      echo "    NANOBOT_RUNTIME_PORT=50064 \\"
      echo "    .venv/bin/python -m nanobot_runtime"
      ;;
  esac

  echo ""
  read -r -p "Press ENTER when runtime is ready (or type 'skip' to skip): " ready_input
  if [[ "${ready_input}" == "skip" ]]; then
    echo "  ⏭  Skipped ${rt}"
    echo "## ${rt}: SKIPPED" >> "${LOG_FILE}"
    SKIP_COUNT=$((SKIP_COUNT + 1))
    continue
  fi

  echo ""
  echo "[Step 2] Trigger skill-extraction via gRPC:"
  case "${rt}" in
    grid)      PORT=50061 ;;
    claude-code) PORT=50062 ;;
    goose)     PORT=50063 ;;
    nanobot)   PORT=50064 ;;
  esac
  echo "  cd tests/contract"
  echo "  python -m pytest contract_v1/test_e2e_smoke.py --runtime=${rt} -v"
  echo ""
  read -r -p "Press ENTER after triggering skill-extraction: "

  echo ""
  echo "[Step 3] Verify the following checklist:"
  cat <<'EOF'

  Event Stream Checklist:
  [ ] TOOL_CALL event received (name=memory_search)
  [ ] TOOL_RESULT event received (status=ok)
  [ ] Full tool loop: search → read → write_anchor → write_file (4 rounds)
  [ ] PostToolUse hook fired at least once (check runtime logs)
  [ ] CHUNK events with text content received
  [ ] Final event chunk_type == "done"

  L2 Memory Checklist:
  [ ] 1 evidence_anchor written to L2 memory
  [ ] 1 memory_file written to L2 memory

  No Failure Indicators:
  [ ] No ERROR-level log lines in runtime output
  [ ] No event sequence interrupt or timeout
  [ ] No gRPC UNAVAILABLE or INTERNAL status codes

EOF

  echo ""
  read -r -p "Sign-off for ${rt} (y=PASS / n=FAIL / s=SKIP): " signoff

  timestamp=$(date '+%Y-%m-%dT%H:%M:%S')
  case "${signoff}" in
    y|Y|yes|YES)
      echo "  ✅ PASS — ${rt}"
      echo "## ${rt}: PASS (signed off at ${timestamp})" >> "${LOG_FILE}"
      PASS_COUNT=$((PASS_COUNT + 1))
      ;;
    n|N|no|NO)
      echo "  ❌ FAIL — ${rt}"
      read -r -p "  Brief failure note: " fail_note
      echo "## ${rt}: FAIL — ${fail_note} (at ${timestamp})" >> "${LOG_FILE}"
      FAIL_COUNT=$((FAIL_COUNT + 1))
      ;;
    *)
      echo "  ⏭  SKIP — ${rt}"
      echo "## ${rt}: SKIPPED (at ${timestamp})" >> "${LOG_FILE}"
      SKIP_COUNT=$((SKIP_COUNT + 1))
      ;;
  esac
done

echo ""
echo "================================================================"
echo "  Verification Summary"
echo "================================================================"
echo "  PASS:  ${PASS_COUNT}"
echo "  FAIL:  ${FAIL_COUNT}"
echo "  SKIP:  ${SKIP_COUNT}"
echo ""
echo "  Log: ${LOG_FILE}"
cat "${LOG_FILE}"
echo ""

if [[ "${PASS_COUNT}" -ge 2 ]]; then
  echo "  🎉 Phase 2.5 sign-off COMPLETE (≥2 runtimes PASS)"
  exit 0
elif [[ "${FAIL_COUNT}" -gt 0 ]]; then
  echo "  ⛔ Verification FAILED — fix failures before closing Phase 2.5"
  exit 1
else
  echo "  ⚠️  Not enough runtimes signed off (need ≥2 PASS)"
  exit 2
fi
