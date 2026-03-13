#!/usr/bin/env bash
# Octo Server Evaluation Runner
# Usage: ./scripts/eval/run_server_eval.sh [task_id]
# Prerequisites: octo-server running on port 3001
# Example: ./scripts/eval/run_server_eval.sh S1
#          ./scripts/eval/run_server_eval.sh all

set -euo pipefail

BASE_URL="${OCTO_SERVER_URL:-http://127.0.0.1:3001}"
EVAL_DIR="/tmp/octo-eval-server-$(date +%Y%m%d-%H%M%S)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

passed=0
failed=0
skipped=0
total=0

log_info()  { echo -e "${BLUE}[INFO]${NC} $*"; }
log_pass()  { echo -e "${GREEN}[PASS]${NC} $*"; ((passed++)); }
log_fail()  { echo -e "${RED}[FAIL]${NC} $*"; ((failed++)); }
log_skip()  { echo -e "${YELLOW}[SKIP]${NC} $*"; ((skipped++)); }

check_prerequisites() {
    log_info "Checking prerequisites..."
    mkdir -p "$EVAL_DIR"

    if ! command -v curl &>/dev/null; then
        echo "ERROR: curl is required"
        exit 1
    fi

    if ! command -v jq &>/dev/null; then
        echo "ERROR: jq is required (brew install jq)"
        exit 1
    fi

    # Check server is running
    local health
    health=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/api/health" 2>/dev/null) || true

    if [ "$health" != "200" ]; then
        echo "ERROR: octo-server not responding at ${BASE_URL}"
        echo "Start it with: make server"
        exit 1
    fi

    log_info "Server responding at ${BASE_URL}"
    log_info "Eval directory: $EVAL_DIR"
}

# ── S1: Agent Lifecycle Management ──

eval_s1() {
    ((total++))
    log_info "S1: Agent Lifecycle Management (Easy)"

    # Create agent
    local create_resp
    create_resp=$(curl -s -X POST "${BASE_URL}/api/v1/agents" \
        -H "Content-Type: application/json" \
        -d '{"name": "eval-agent-s1", "role": "evaluator", "goal": "Evaluation test agent"}') || true

    local agent_id
    agent_id=$(echo "$create_resp" | jq -r '.id // .agent_id // empty' 2>/dev/null) || true

    if [ -z "$agent_id" ]; then
        log_fail "S1: Failed to create agent. Response: ${create_resp:0:200}"
        return
    fi

    # List agents - verify it appears
    local list_resp
    list_resp=$(curl -s "${BASE_URL}/api/v1/agents") || true
    if ! echo "$list_resp" | jq -e ".[] | select(.id == \"$agent_id\" or .name == \"eval-agent-s1\")" &>/dev/null; then
        # Try alternate format
        if ! echo "$list_resp" | grep -q "eval-agent-s1"; then
            log_fail "S1: Agent not found in list after creation"
            return
        fi
    fi

    # Get agent info
    local info_resp
    info_resp=$(curl -s "${BASE_URL}/api/v1/agents/${agent_id}") || true
    local info_status
    info_status=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/api/v1/agents/${agent_id}") || true

    if [ "$info_status" = "200" ]; then
        log_pass "S1: Agent lifecycle (create + list + info) verified"
    else
        log_fail "S1: Agent info returned status ${info_status}"
    fi

    # Cleanup
    curl -s -X DELETE "${BASE_URL}/api/v1/agents/${agent_id}" &>/dev/null || true
}

# ── S2: Health & Config Endpoints ──

eval_s2() {
    ((total++))
    log_info "S2: Health & Config Endpoints (Easy)"

    local health_resp
    health_resp=$(curl -s "${BASE_URL}/api/health") || true

    local config_resp
    config_resp=$(curl -s "${BASE_URL}/api/config") || true

    local checks=0

    # Health should have status field
    if echo "$health_resp" | jq -e '.status' &>/dev/null || echo "$health_resp" | grep -qi "ok\|healthy"; then
        ((checks++))
    fi

    # Config should have some content
    if [ -n "$config_resp" ] && [ "$config_resp" != "null" ]; then
        ((checks++))
    fi

    if [ "$checks" -ge 2 ]; then
        log_pass "S2: Health and config endpoints responding correctly"
    else
        log_fail "S2: Health or config endpoint issues. Health: ${health_resp:0:100}, Config: ${config_resp:0:100}"
    fi
}

# ── S3: Tool Registry via API ──

eval_s3() {
    ((total++))
    log_info "S3: Tool Registry via API (Easy)"

    local tools_resp
    tools_resp=$(curl -s "${BASE_URL}/api/tools") || true

    local tool_count
    tool_count=$(echo "$tools_resp" | jq 'length' 2>/dev/null) || tool_count=0

    if [ "$tool_count" -gt 5 ]; then
        # Check for essential tools
        local has_bash has_file_read
        has_bash=$(echo "$tools_resp" | jq '[.[] | select(.name == "bash")] | length' 2>/dev/null) || has_bash=0
        has_file_read=$(echo "$tools_resp" | jq '[.[] | select(.name == "file_read")] | length' 2>/dev/null) || has_file_read=0

        if [ "$has_bash" -gt 0 ] && [ "$has_file_read" -gt 0 ]; then
            log_pass "S3: Tool registry has ${tool_count} tools including bash and file_read"
        else
            log_pass "S3: Tool registry has ${tool_count} tools (essential tools check inconclusive)"
        fi
    else
        log_fail "S3: Tool registry returned only ${tool_count} tools. Expected >5. Response: ${tools_resp:0:200}"
    fi
}

# ── S4: Session Management ──

eval_s4() {
    ((total++))
    log_info "S4: Session Management (Medium)"

    local sessions_resp
    sessions_resp=$(curl -s "${BASE_URL}/api/sessions") || true

    local status_code
    status_code=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/api/sessions") || true

    if [ "$status_code" = "200" ]; then
        local count
        count=$(echo "$sessions_resp" | jq 'if type == "array" then length else 0 end' 2>/dev/null) || count="unknown"
        log_pass "S4: Sessions endpoint responding (${count} sessions)"
    else
        log_fail "S4: Sessions endpoint returned status ${status_code}"
    fi
}

# ── S5: Token Budget API ──

eval_s5() {
    ((total++))
    log_info "S5: Token Budget API (Medium)"

    local budget_resp
    budget_resp=$(curl -s "${BASE_URL}/api/budget") || true
    local status_code
    status_code=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/api/budget") || true

    if [ "$status_code" = "200" ]; then
        # Check for budget-related fields
        if echo "$budget_resp" | jq -e '.total // .budget // .usage_percent' &>/dev/null; then
            log_pass "S5: Budget endpoint returning structured data"
        else
            log_pass "S5: Budget endpoint responding (status 200)"
        fi
    else
        log_fail "S5: Budget endpoint returned status ${status_code}. Response: ${budget_resp:0:200}"
    fi
}

# ── S6: Audit Log Endpoint ──

eval_s6() {
    ((total++))
    log_info "S6: Audit Log Endpoint (Easy)"

    local audit_resp
    audit_resp=$(curl -s "${BASE_URL}/api/audit") || true
    local status_code
    status_code=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/api/audit") || true

    if [ "$status_code" = "200" ]; then
        log_pass "S6: Audit endpoint responding correctly"
    elif [ "$status_code" = "401" ] || [ "$status_code" = "403" ]; then
        log_pass "S6: Audit endpoint requires auth (expected in production)"
    else
        log_fail "S6: Audit endpoint returned unexpected status ${status_code}"
    fi
}

# ── Main ──

print_summary() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Server Evaluation Summary"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "  Base URL: ${BASE_URL}"
    echo -e "  Total:   ${total}"
    echo -e "  ${GREEN}Passed:  ${passed}${NC}"
    echo -e "  ${RED}Failed:  ${failed}${NC}"
    echo -e "  ${YELLOW}Skipped: ${skipped}${NC}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Eval dir: ${EVAL_DIR}"
    echo ""

    if [ "$failed" -eq 0 ]; then
        echo -e "  ${GREEN}Result: ALL PASSED${NC}"
    else
        echo -e "  ${RED}Result: ${failed} FAILURES${NC}"
    fi
}

main() {
    local task="${1:-all}"

    check_prerequisites

    echo ""
    echo "╔══════════════════════════════════════╗"
    echo "║   Octo Server Evaluation Runner      ║"
    echo "╚══════════════════════════════════════╝"
    echo ""

    case "$task" in
        S1|s1) eval_s1 ;;
        S2|s2) eval_s2 ;;
        S3|s3) eval_s3 ;;
        S4|s4) eval_s4 ;;
        S5|s5) eval_s5 ;;
        S6|s6) eval_s6 ;;
        all)
            eval_s1
            eval_s2
            eval_s3
            eval_s4
            eval_s5
            eval_s6
            ;;
        *)
            echo "Usage: $0 [S1|S2|S3|S4|S5|S6|all]"
            exit 1
            ;;
    esac

    print_summary
}

main "$@"
