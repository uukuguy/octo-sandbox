#!/bin/bash
# verify-dual-runtime.sh — Verify both EAASP runtimes pass contract certification.
#
# Step 1: Build all binaries (cargo build --release)
# Step 2: Start grid-runtime (:50051) and claude-code-runtime (:50052)
# Step 3: Run eaasp-certifier against both endpoints
#
# Prerequisites:
#   - ANTHROPIC_API_KEY in environment (or .env)
#   - Rust toolchain (cargo)
#   - Python 3.12+ with uv
#
# Usage:
#   ./scripts/verify-dual-runtime.sh
#   ./scripts/verify-dual-runtime.sh --grid-only
#   ./scripts/verify-dual-runtime.sh --claude-only
#   ./scripts/verify-dual-runtime.sh --skip-build  (skip cargo build step)

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GRID_PORT=50051
CLAUDE_PORT=50052
GRID_PID=""
CLAUDE_PID=""
SKIP_BUILD=false

cleanup() {
    echo ""
    echo "=== Cleaning up ==="
    [ -n "$GRID_PID" ] && kill "$GRID_PID" 2>/dev/null && echo "Stopped grid-runtime (PID $GRID_PID)"
    [ -n "$CLAUDE_PID" ] && kill "$CLAUDE_PID" 2>/dev/null && echo "Stopped claude-code-runtime (PID $CLAUDE_PID)"
}
trap cleanup EXIT

build_rust() {
    if [ "$SKIP_BUILD" = true ]; then
        echo "=== Skipping Rust build (--skip-build) ==="
        return
    fi
    echo "=== Building Rust binaries (release) ==="
    cd "$PROJECT_ROOT"
    cargo build --release -p grid-runtime -p eaasp-certifier 2>&1 | tail -5
    echo "  Build complete."
    echo ""
}

start_grid_runtime() {
    echo "=== Starting grid-runtime on :${GRID_PORT} ==="
    cd "$PROJECT_ROOT"
    # Use pre-built binary, not cargo run
    GRID_RUNTIME_PORT=$GRID_PORT \
    RUST_LOG=grid_runtime=info \
    ./target/release/grid-runtime 2>&1 | sed 's/^/  [grid] /' &
    GRID_PID=$!
    echo "  PID: $GRID_PID"
}

start_claude_runtime() {
    echo "=== Starting claude-code-runtime on :${CLAUDE_PORT} ==="
    cd "$PROJECT_ROOT/lang/claude-code-runtime-python"
    uv run python -m claude_code_runtime \
        --port "$CLAUDE_PORT" \
        --env-file "$PROJECT_ROOT/.env" \
        --log-level INFO 2>&1 | sed 's/^/  [claude] /' &
    CLAUDE_PID=$!
    echo "  PID: $CLAUDE_PID"
}

wait_for_port() {
    local port=$1
    local name=$2
    local max_wait=15
    local waited=0

    echo "  Waiting for $name on :${port}..."
    while ! nc -z localhost "$port" 2>/dev/null; do
        sleep 1
        waited=$((waited + 1))
        if [ $waited -ge $max_wait ]; then
            echo "  ERROR: $name did not start within ${max_wait}s"
            return 1
        fi
    done
    echo "  $name is ready (${waited}s)"
}

verify_runtime() {
    local endpoint=$1
    local name=$2

    echo ""
    echo "=== Verifying $name ==="
    cd "$PROJECT_ROOT"
    # Use pre-built binary
    ./target/release/eaasp-certifier verify --endpoint "$endpoint"
    local rc=$?
    if [ $rc -eq 0 ]; then
        echo "  ✅ $name: PASS"
    else
        echo "  ❌ $name: FAIL (exit code $rc)"
    fi
    return $rc
}

# Parse args
MODE="both"
for arg in "$@"; do
    case "$arg" in
        --grid-only) MODE="grid" ;;
        --claude-only) MODE="claude" ;;
        --skip-build) SKIP_BUILD=true ;;
        --help|-h)
            echo "Usage: $0 [--grid-only|--claude-only] [--skip-build]"
            exit 0
            ;;
    esac
done

echo "════════════════════════════════════════════════════"
echo "  EAASP Dual-Runtime Verification"
echo "════════════════════════════════════════════════════"
echo ""

# Step 1: Build
build_rust

RESULTS=0

if [ "$MODE" = "both" ] || [ "$MODE" = "grid" ]; then
    start_grid_runtime
    wait_for_port $GRID_PORT "grid-runtime"
    verify_runtime "http://localhost:${GRID_PORT}" "grid-runtime" || RESULTS=$((RESULTS + 1))
fi

if [ "$MODE" = "both" ] || [ "$MODE" = "claude" ]; then
    start_claude_runtime
    wait_for_port $CLAUDE_PORT "claude-code-runtime"
    verify_runtime "http://localhost:${CLAUDE_PORT}" "claude-code-runtime" || RESULTS=$((RESULTS + 1))
fi

echo ""
echo "════════════════════════════════════════════════════"
if [ $RESULTS -eq 0 ]; then
    echo "  ✅ All runtimes verified successfully"
else
    echo "  ❌ $RESULTS runtime(s) failed verification"
fi
echo "════════════════════════════════════════════════════"

exit $RESULTS
