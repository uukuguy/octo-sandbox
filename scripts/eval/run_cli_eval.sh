#!/usr/bin/env bash
# Octo CLI Evaluation Runner
# Usage: ./scripts/eval/run_cli_eval.sh [task_id]
# Example: ./scripts/eval/run_cli_eval.sh C1
#          ./scripts/eval/run_cli_eval.sh all

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVAL_DIR="/tmp/octo-eval-cli-$(date +%Y%m%d-%H%M%S)"
OCTO_BIN="${PROJECT_ROOT}/target/debug/octo-cli"
DB_PATH="${EVAL_DIR}/eval.db"
RESULTS_FILE="${EVAL_DIR}/results.json"

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

    if [ ! -f "$OCTO_BIN" ]; then
        log_info "Building octo-cli..."
        (cd "$PROJECT_ROOT" && cargo build -p octo-cli 2>/dev/null)
    fi

    if [ ! -f "$OCTO_BIN" ]; then
        echo "ERROR: octo-cli binary not found at $OCTO_BIN"
        echo "Run: cargo build -p octo-cli"
        exit 1
    fi

    mkdir -p "$EVAL_DIR"
    log_info "Eval directory: $EVAL_DIR"
    log_info "Database: $DB_PATH"
}

# ── C1: File Read/Write Round-Trip ──

eval_c1() {
    ((total++))
    log_info "C1: File Read/Write Round-Trip (Easy)"

    local test_file="${EVAL_DIR}/c1_test.txt"
    local expected_content="Hello from octo evaluation test C1"

    # Use tool invoke directly for deterministic testing
    "$OCTO_BIN" --db "$DB_PATH" --quiet tool invoke file_write \
        "{\"path\": \"${test_file}\", \"content\": \"${expected_content}\"}" 2>/dev/null || true

    if [ -f "$test_file" ]; then
        local actual
        actual=$(cat "$test_file")
        if [ "$actual" = "$expected_content" ]; then
            log_pass "C1: File created and content matches"
        else
            log_fail "C1: Content mismatch. Expected='${expected_content}' Got='${actual}'"
        fi
    else
        log_fail "C1: File was not created at ${test_file}"
    fi
}

# ── C2: Bash Command Execution ──

eval_c2() {
    ((total++))
    log_info "C2: Bash Command Execution (Easy)"

    local output
    output=$("$OCTO_BIN" --db "$DB_PATH" --quiet tool invoke bash \
        '{"cmd": "echo OCTO_EVAL_MARKER_42"}' 2>/dev/null) || true

    if echo "$output" | grep -q "OCTO_EVAL_MARKER_42"; then
        log_pass "C2: Bash tool executed and returned expected output"
    else
        log_fail "C2: Expected 'OCTO_EVAL_MARKER_42' in output. Got: ${output:0:200}"
    fi
}

# ── C3: Multi-step File Operations (requires LLM) ──

eval_c3() {
    ((total++))
    log_info "C3: Multi-step File Operations (Medium) — requires LLM"

    if [ -z "${ANTHROPIC_API_KEY:-}" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
        log_skip "C3: No LLM API key configured (set ANTHROPIC_API_KEY or OPENAI_API_KEY)"
        return
    fi

    local test_dir="${EVAL_DIR}/c3_workspace"
    mkdir -p "$test_dir"

    # Create initial file
    echo '{"name": "octo", "version": "1.0.0", "status": "draft"}' > "${test_dir}/config.json"

    local prompt="Read the file ${test_dir}/config.json, change the status field to 'published' and the version to '2.0.0', then write it back. After editing, read the file again to confirm."

    "$OCTO_BIN" --db "$DB_PATH" --quiet ask "$prompt" 2>/dev/null || true

    if [ -f "${test_dir}/config.json" ]; then
        local content
        content=$(cat "${test_dir}/config.json")
        if echo "$content" | grep -q '"published"' && echo "$content" | grep -q '"2.0.0"'; then
            log_pass "C3: Multi-step file edit completed correctly"
        else
            log_fail "C3: File not edited as expected. Content: ${content:0:200}"
        fi
    else
        log_fail "C3: Config file missing after edit"
    fi
}

# ── C4: Code Generation (requires LLM) ──

eval_c4() {
    ((total++))
    log_info "C4: Code Generation & Verification (Medium) — requires LLM"

    if [ -z "${ANTHROPIC_API_KEY:-}" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
        log_skip "C4: No LLM API key configured"
        return
    fi

    local code_file="${EVAL_DIR}/c4_fizzbuzz.py"
    local prompt="Write a Python script to ${code_file} that prints FizzBuzz for numbers 1-15. Use file_write to create the file. The script should print 'Fizz' for multiples of 3, 'Buzz' for multiples of 5, 'FizzBuzz' for multiples of both, and the number otherwise."

    "$OCTO_BIN" --db "$DB_PATH" --quiet ask "$prompt" 2>/dev/null || true

    if [ -f "$code_file" ]; then
        local output
        output=$(python3 "$code_file" 2>/dev/null) || true
        if echo "$output" | grep -q "FizzBuzz" && echo "$output" | grep -q "Fizz" && echo "$output" | grep -q "Buzz"; then
            log_pass "C4: Generated code executes correctly with FizzBuzz output"
        else
            log_fail "C4: Code output incorrect. Got: ${output:0:200}"
        fi
    else
        log_fail "C4: Code file was not generated at ${code_file}"
    fi
}

# ── C5: Memory Consistency ──

eval_c5() {
    ((total++))
    log_info "C5: Memory Store/Search Consistency (Medium)"

    local unique_key="eval_test_$(date +%s)"
    local test_content="Octo evaluation memory test: ${unique_key}"

    # Store a memory entry
    "$OCTO_BIN" --db "$DB_PATH" --quiet memory add "$test_content" 2>/dev/null || true

    # Search for it
    local search_result
    search_result=$("$OCTO_BIN" --db "$DB_PATH" --quiet memory search "$unique_key" 2>/dev/null) || true

    if echo "$search_result" | grep -qi "$unique_key"; then
        log_pass "C5: Memory stored and retrieved successfully"
    else
        # Try list as fallback
        local list_result
        list_result=$("$OCTO_BIN" --db "$DB_PATH" --quiet memory list 2>/dev/null) || true
        if echo "$list_result" | grep -qi "$unique_key"; then
            log_pass "C5: Memory stored (found via list)"
        else
            log_fail "C5: Memory not found. Search: ${search_result:0:200}"
        fi
    fi
}

# ── C6: Tool Registry ──

eval_c6() {
    ((total++))
    log_info "C6: Tool Registry & Discovery (Easy)"

    local tool_list
    tool_list=$("$OCTO_BIN" --db "$DB_PATH" --quiet tool list 2>/dev/null) || true

    local found_count=0
    local required_tools=("bash" "file_read" "file_write")

    for tool in "${required_tools[@]}"; do
        if echo "$tool_list" | grep -qi "$tool"; then
            ((found_count++))
        fi
    done

    if [ "$found_count" -eq "${#required_tools[@]}" ]; then
        log_pass "C6: All ${#required_tools[@]} required tools found in registry"
    else
        log_fail "C6: Only ${found_count}/${#required_tools[@]} required tools found. Output: ${tool_list:0:300}"
    fi
}

# ── Main ──

print_summary() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  CLI Evaluation Summary"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "  Total:   ${total}"
    echo -e "  ${GREEN}Passed:  ${passed}${NC}"
    echo -e "  ${RED}Failed:  ${failed}${NC}"
    echo -e "  ${YELLOW}Skipped: ${skipped}${NC}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
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
    echo "║   Octo CLI Evaluation Runner         ║"
    echo "╚══════════════════════════════════════╝"
    echo ""

    case "$task" in
        C1|c1) eval_c1 ;;
        C2|c2) eval_c2 ;;
        C3|c3) eval_c3 ;;
        C4|c4) eval_c4 ;;
        C5|c5) eval_c5 ;;
        C6|c6) eval_c6 ;;
        all)
            eval_c1
            eval_c2
            eval_c3
            eval_c4
            eval_c5
            eval_c6
            ;;
        *)
            echo "Usage: $0 [C1|C2|C3|C4|C5|C6|all]"
            exit 1
            ;;
    esac

    print_summary
}

main "$@"
