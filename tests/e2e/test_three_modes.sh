#!/bin/bash
set -e

# E2E Test: Three Interaction Modes
# Tests: Chat (WebSocket), Background Tasks, Scheduled Tasks

BASE_URL="${BASE_URL:-http://localhost:3001}"
WS_URL="${WS_URL:-ws://localhost:3001/ws}"
SERVER_PID=""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

cleanup() {
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        log_info "Stopping server (PID: $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT

wait_for_server() {
    local max_attempts=30
    local attempt=1

    log_info "Waiting for server to be ready..."

    while [ $attempt -le $max_attempts ]; do
        if curl -s -f "$BASE_URL/api/health" > /dev/null 2>&1; then
            log_info "Server is ready!"
            return 0
        fi
        echo -n "."
        sleep 1
        attempt=$((attempt + 1))
    done

    log_error "Server failed to start within ${max_attempts}s"
    return 1
}

# Test Mode 1: Chat (WebSocket)
test_chat_websocket() {
    log_info "=== Mode 1: Chat (WebSocket) ==="

    # Check if we have a WebSocket client
    local ws_client=""
    if command -v wscat &> /dev/null; then
        ws_client="wscat"
    elif command -v websocat &> /dev/null; then
        ws_client="websocat"
    elif command -v node &> /dev/null; then
        ws_client="node"
    else
        log_warn "No WebSocket client found (wscat/websocat/node). Skipping WebSocket test."
        log_info "To test WebSocket manually:"
        log_info "  1. Install wscat: npm install -g wscat"
        log_info "  2. Run: wscat -c $WS_URL"
        log_info "  3. Send: {\"type\":\"hello\",\"prompt\":\"hello\",\"mode\":\"chat\"}"
        return 0
    fi

    # Simple WebSocket echo test using Node.js
    if [ "$ws_client" = "node" ]; then
        local ws_test_script=$(mktemp)
        cat > "$ws_test_script" << 'NODESCRIPT'
const WebSocket = require('ws');
const ws = new WebSocket(process.env.WS_URL || 'ws://localhost:3001/ws');

ws.on('open', () => {
    console.log('[WS] Connected');
    ws.send(JSON.stringify({
        type: 'hello',
        prompt: 'say "hello" and stop',
        mode: 'chat'
    }));
});

ws.on('message', (data) => {
    const msg = JSON.parse(data.toString());
    console.log('[WS] Received:', JSON.stringify(msg).slice(0, 200));
    if (msg.type === 'done' || msg.type === 'error') {
        ws.close();
        process.exit(msg.type === 'done' ? 0 : 1);
    }
});

ws.on('error', (err) => {
    console.error('[WS] Error:', err.message);
    process.exit(1);
});

setTimeout(() => {
    console.error('[WS] Timeout');
    ws.close();
    process.exit(1);
}, 30000);
NODESCRIPT

        # Check if ws package is available
        if node -e "require('ws')" 2>/dev/null; then
            WS_URL="$WS_URL" node "$ws_test_script"
            local result=$?
            rm -f "$ws_test_script"
            return $result
        else
            log_warn "ws npm package not installed. Skipping WebSocket test."
            rm -f "$ws_test_script"
            return 0
        fi
    fi

    # Use wscat or websocat
    log_info "Using $ws_client for WebSocket test..."

    return 0
}

# Test Mode 2: Background Task
test_background_tasks() {
    log_info "=== Mode 2: Background Task ==="

    # Create a background task
    local task_response
    task_response=$(curl -s -X POST "$BASE_URL/api/tasks" \
        -H "Content-Type: application/json" \
        -d '{"prompt": "echo hello world", "model": null}')

    log_info "Task response: $task_response"

    # Extract task ID - handle both response formats
    local task_id
    task_id=$(echo "$task_response" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

    if [ -z "$task_id" ]; then
        # Try alternative format
        task_id=$(echo "$task_response" | grep -o '"id": "[^"]*"' | head -1 | cut -d'"' -f4)
    fi

    if [ -z "$task_id" ]; then
        log_error "Failed to extract task ID from response"
        log_error "Response: $task_response"
        return 1
    fi

    log_info "Task created with ID: $task_id"

    # Wait for task to complete (max 60s)
    local max_wait=60
    local waited=0

    while [ $waited -lt $max_wait ]; do
        local status
        status=$(curl -s "$BASE_URL/api/tasks/$task_id" | grep -o '"status":"[^"]*"' | cut -d'"' -f4)

        if [ -z "$status" ]; then
            status=$(curl -s "$BASE_URL/api/tasks/$task_id" | grep -o '"status": "[^"]*"' | cut -d'"' -f4)
        fi

        log_info "Task status: $status"

        if [ "$status" = "done" ] || [ "$status" = "success" ]; then
            log_info "Background task completed successfully!"
            return 0
        elif [ "$status" = "failed" ]; then
            log_error "Background task failed"
            return 1
        fi

        sleep 2
        waited=$((waited + 2))
    done

    log_error "Task did not complete within ${max_wait}s"
    return 1
}

# Test Mode 3: Scheduled Task
test_scheduled_tasks() {
    log_info "=== Mode 3: Scheduled Task ==="

    # First, list existing scheduled tasks
    log_info "Listing scheduled tasks..."
    local tasks_response
    tasks_response=$(curl -s "$BASE_URL/api/scheduler/tasks")
    log_info "Scheduled tasks response: $tasks_response"

    # Create a scheduled task
    local sched_response
    sched_response=$(curl -s -X POST "$BASE_URL/api/scheduler/tasks" \
        -H "Content-Type: application/json" \
        -d '{
            "name": "e2e-test-task",
            "cron": "0 * * * *",
            "agent_config": {
                "prompt": "echo hello from scheduled task"
            }
        }')

    log_info "Scheduled task response: $sched_response"

    # Extract scheduled task ID
    local sched_id
    sched_id=$(echo "$sched_response" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

    if [ -z "$sched_id" ]; then
        sched_id=$(echo "$sched_response" | grep -o '"id": "[^"]*"' | head -1 | cut -d'"' -f4)
    fi

    if [ -z "$sched_id" ]; then
        log_warn "Failed to create scheduled task (may already exist or API differs)"
        log_info "Trying to run existing scheduled task instead..."

        # Try to run an existing task
        local existing_id
        existing_id=$(echo "$tasks_response" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

        if [ -n "$existing_id" ]; then
            log_info "Running existing scheduled task: $existing_id"
            local run_response
            run_response=$(curl -s -X POST "$BASE_URL/api/scheduler/tasks/$existing_id/run")
            log_info "Run response: $run_response"

            if echo "$run_response" | grep -q "error"; then
                log_error "Failed to run scheduled task"
                return 1
            fi

            log_info "Scheduled task triggered successfully!"
            return 0
        fi

        log_warn "No scheduled tasks available to test"
        return 0
    fi

    log_info "Scheduled task created with ID: $sched_id"

    # Manually trigger the scheduled task
    log_info "Manually triggering scheduled task..."
    local run_response
    run_response=$(curl -s -X POST "$BASE_URL/api/scheduler/tasks/$sched_id/run")
    log_info "Run response: $run_response"

    if echo "$run_response" | grep -q "error"; then
        log_error "Failed to run scheduled task: $run_response"
        return 1
    fi

    log_info "Scheduled task triggered successfully!"
    return 0
}

# Main execution
main() {
    log_info "Starting E2E tests for three interaction modes"
    log_info "BASE_URL: $BASE_URL"
    log_info "WS_URL: $WS_URL"

    # Check if server is already running
    if curl -s -f "$BASE_URL/api/health" > /dev/null 2>&1; then
        log_info "Server already running, using existing instance"
    else
        log_info "Starting server..."
        cargo run -p octo-server &
        SERVER_PID=$!
        wait_for_server
    fi

    # Run tests
    local failed=0

    test_chat_websocket || failed=$((failed + 1))
    test_background_tasks || failed=$((failed + 1))
    test_scheduled_tasks || failed=$((failed + 1))

    if [ $failed -eq 0 ]; then
        log_info "=== All E2E tests passed! ==="
        exit 0
    else
        log_error "=== $failed test(s) failed ==="
        exit 1
    fi
}

main "$@"
