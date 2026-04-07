#!/usr/bin/env bash
# EAASP MVP E2E Orchestration Script
#
# Usage:
#   ./scripts/e2e-mvp.sh [--mock-llm | --live-llm]
#
# Prerequisites:
#   - L2 Skill Registry running on :8081
#   - L1 Grid Runtime running on :50051
#   - Python packages installed (eaasp-governance, eaasp-session-manager, eaasp SDK)

set -euo pipefail

MODE="${1:---mock-llm}"
L3_PORT=8083
L4_PORT=8084
L3_PID=""
L4_PID=""

cleanup() {
    echo "Cleaning up..."
    [ -n "$L3_PID" ] && kill "$L3_PID" 2>/dev/null || true
    [ -n "$L4_PID" ] && kill "$L4_PID" 2>/dev/null || true
}
trap cleanup EXIT

echo "=== EAASP MVP E2E Orchestration ==="
echo "Mode: $MODE"
echo ""

# 1. Start L3 Governance
echo "1. Starting L3 Governance (:$L3_PORT)..."
python -m eaasp_governance --port "$L3_PORT" &
L3_PID=$!
sleep 2

# 2. Start L4 Session Manager
echo "2. Starting L4 Session Manager (:$L4_PORT)..."
python -m eaasp_session --port "$L4_PORT" --l3-url "http://localhost:$L3_PORT" &
L4_PID=$!
sleep 2

# 3. Deploy policies
echo "3. Deploying policies..."
curl -s -X PUT "http://localhost:$L3_PORT/v1/policies/deploy" \
  -H "Content-Type: application/yaml" \
  --data-binary @sdk/examples/hr-onboarding/policies/enterprise.yaml | python -m json.tool

curl -s -X PUT "http://localhost:$L3_PORT/v1/policies/deploy" \
  -H "Content-Type: application/yaml" \
  --data-binary @sdk/examples/hr-onboarding/policies/bu_hr.yaml | python -m json.tool

# 4. Run SDK command
echo ""
echo "4. Running eaasp run..."
eaasp run ./sdk/examples/hr-onboarding/ \
  --platform "http://localhost:$L4_PORT" \
  "$MODE" \
  --input "新员工张三入职，工号 E2024001"

echo ""
echo "=== E2E Complete ==="
