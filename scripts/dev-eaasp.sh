#!/bin/bash
# dev-eaasp.sh — Start all EAASP v2.0 services for interactive development.
#
# Services (in start order):
#   skill-registry       :${EAASP_SKILL_REGISTRY_PORT:-18081}
#   L2 memory-engine     :${EAASP_L2_PORT:-18085}
#   L3 governance        :${EAASP_L3_PORT:-18083}
#   mock-scada SSE       :${EAASP_MOCK_SCADA_SSE_PORT:-18090}
#   L2 MCP Orchestrator  :${EAASP_MCP_ORCHESTRATOR_PORT:-18082}
#   grid-runtime         :${GRID_RUNTIME_PORT:-50051}
#   claude-code-runtime  :${CLAUDE_RUNTIME_PORT:-50052}
#   nanobot-runtime      :${NANOBOT_RUNTIME_PORT:-50054}   (Phase 2.5 W2)
#   goose-runtime        :${GOOSE_RUNTIME_PORT:-50063}     (Phase 2.5 W1, Docker)
#   L4 orchestration     :${EAASP_L4_PORT:-18084}
#
# Per ADR-V2-017 L1 runtime 生态策略:
#   - 主力: grid-runtime
#   - 样板: claude-code-runtime (Anthropic), nanobot-runtime (OpenAI-compat)
#   - 对比: goose-runtime (Phase 2.5 W1 container-first baseline)
#   - 冻结: hermes-runtime (2026-04-14, removed from dev-eaasp)
#
# After all services are up, the script stays foreground. Ctrl+C kills everything.
#
# Usage:
#   ./scripts/dev-eaasp.sh
#   ./scripts/dev-eaasp.sh --skip-build        # skip cargo build
#   ./scripts/dev-eaasp.sh --skip-goose        # skip goose-runtime Docker
#   ./scripts/dev-eaasp.sh --skip-nanobot      # skip nanobot-runtime
#   make dev-eaasp                              # via Makefile

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# ── Colors ────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

# ── Port assignments (env-overridable, must match verify-v2-mvp.sh) ───────
SKILL_REG_PORT="${EAASP_SKILL_REGISTRY_PORT:-18081}"
L2_MEM_PORT="${EAASP_L2_PORT:-18085}"
L3_GOV_PORT="${EAASP_L3_PORT:-18083}"
L4_ORCH_PORT="${EAASP_L4_PORT:-18084}"
GRID_RT_PORT="${GRID_RUNTIME_PORT:-50051}"
CLAUDE_RT_PORT="${CLAUDE_RUNTIME_PORT:-50052}"
NANOBOT_RT_PORT="${NANOBOT_RUNTIME_PORT:-50054}"
GOOSE_RT_PORT="${GOOSE_RUNTIME_PORT:-50063}"
MOCK_SCADA_SSE_PORT="${EAASP_MOCK_SCADA_SSE_PORT:-18090}"
MCP_ORCH_PORT="${EAASP_MCP_ORCHESTRATOR_PORT:-18082}"

# ── Runtime flags ─────────────────────────────────────────────────────────
SKIP_BUILD=false
SKIP_NANOBOT=false
SKIP_GOOSE=false

# ── Background PIDs (for cleanup) ────────────────────────────────────────
SKILL_REG_PID=""
L2_PID=""
L3_PID=""
L4_PID=""
GRID_PID=""
CLAUDE_PID=""
NANOBOT_PID=""
GOOSE_PID=""
MOCK_SCADA_SSE_PID=""
MCP_ORCH_PID=""

# ── Cleanup helpers (reused from verify-v2-mvp.sh) ───────────────────────
_kill_tree() {
    local name=$1
    local pid=$2
    [ -z "$pid" ] && return 0
    pkill -TERM -P "$pid" 2>/dev/null || true
    kill -TERM "$pid" 2>/dev/null && echo -e "  ${GREEN}Stopped${RESET} $name (PID $pid)" || true
}

_kill_port() {
    local name=$1
    local port=$2
    local stragglers
    stragglers=$(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null || true)
    if [ -n "$stragglers" ]; then
        echo -e "  ${YELLOW}Reaping${RESET} $name leftover listeners on :$port: $stragglers"
        kill -TERM $stragglers 2>/dev/null || true
        sleep 0.3
        kill -KILL $stragglers 2>/dev/null || true
    fi
}

cleanup() {
    echo ""
    echo -e "${BOLD}=== Stopping EAASP services ===${RESET}"
    _kill_tree "L4 orchestration" "$L4_PID"
    # goose-runtime Docker container (Phase 2.5 W1): short timeout to avoid OrbStack SIGTERM hang.
    docker stop --time=2 eaasp-goose-runtime >/dev/null 2>&1 || true
    docker rm -f eaasp-goose-runtime >/dev/null 2>&1 || true
    _kill_tree "nanobot-runtime" "$NANOBOT_PID"
    _kill_tree "claude-code-runtime" "$CLAUDE_PID"
    _kill_tree "grid-runtime" "$GRID_PID"
    _kill_tree "MCP Orchestrator" "$MCP_ORCH_PID"
    _kill_tree "mock-scada-sse" "$MOCK_SCADA_SSE_PID"
    _kill_tree "L3 governance" "$L3_PID"
    _kill_tree "L2 memory-engine" "$L2_PID"
    _kill_tree "skill-registry" "$SKILL_REG_PID"
    # Sweep orphaned listeners
    _kill_port "L4 orchestration" "$L4_ORCH_PORT"
    _kill_port "goose-runtime" "$GOOSE_RT_PORT"
    _kill_port "nanobot-runtime" "$NANOBOT_RT_PORT"
    _kill_port "claude-code-runtime" "$CLAUDE_RT_PORT"
    _kill_port "grid-runtime" "$GRID_RT_PORT"
    _kill_port "MCP Orchestrator" "$MCP_ORCH_PORT"
    _kill_port "mock-scada-sse" "$MOCK_SCADA_SSE_PORT"
    _kill_port "L3 governance" "$L3_GOV_PORT"
    _kill_port "L2 memory-engine" "$L2_MEM_PORT"
    _kill_port "skill-registry" "$SKILL_REG_PORT"
    echo -e "${GREEN}All services stopped.${RESET}"
}
trap cleanup EXIT INT TERM

# ── Arg parsing ───────────────────────────────────────────────────────────
for arg in "$@"; do
    case "$arg" in
        --skip-build)   SKIP_BUILD=true ;;
        --skip-nanobot) SKIP_NANOBOT=true ;;
        --skip-goose)   SKIP_GOOSE=true ;;
        -h|--help)
            cat <<EOF
Usage: $0 [--skip-build] [--skip-nanobot] [--skip-goose]

  --skip-build    Skip 'cargo build' step (use existing binaries).
  --skip-nanobot  Skip nanobot-runtime startup (Phase 2.5 W2).
  --skip-goose    Skip goose-runtime Docker startup (Phase 2.5 W1).

Starts all EAASP v2.0 services and stays foreground. Ctrl+C kills everything.

Ports (override via env vars):
  EAASP_SKILL_REGISTRY_PORT  (default: 18081)
  EAASP_L2_PORT              (default: 18085)
  EAASP_L3_PORT              (default: 18083)
  EAASP_L4_PORT              (default: 18084)
  GRID_RUNTIME_PORT          (default: 50051)
  CLAUDE_RUNTIME_PORT        (default: 50052)
  NANOBOT_RUNTIME_PORT       (default: 50054)
  GOOSE_RUNTIME_PORT         (default: 50063)
EOF
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown flag: $arg${RESET}" >&2
            exit 1
            ;;
    esac
done

# ── Pre-flight: .venv checks ─────────────────────────────────────────────
check_venv() {
    local svc_dir=$1
    local make_target=$2
    if [ ! -x "$PROJECT_ROOT/$svc_dir/.venv/bin/python" ]; then
        echo -e "${RED}ERROR${RESET}: $svc_dir/.venv is missing. Run: make $make_target" >&2
        return 1
    fi
}

# ── Pre-flight: port collision check ──────────────────────────────────────
check_port_free() {
    local port=$1
    local name=$2
    local holder
    holder=$(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null | head -1 || true)
    if [ -n "$holder" ]; then
        echo -e "${RED}ERROR${RESET}: port $port ($name) already in use by PID $holder." >&2
        echo "       Run: make dev-eaasp-stop   or   lsof -nP -iTCP:$port -sTCP:LISTEN" >&2
        return 1
    fi
}

# ── wait_for_port helper ──────────────────────────────────────────────────
wait_for_port() {
    local port=$1
    local name=$2
    local max_wait=20
    local waited=0

    echo -ne "  ${YELLOW}Waiting${RESET} for $name on :${port}..."
    while ! nc -z 127.0.0.1 "$port" 2>/dev/null; do
        sleep 1
        waited=$((waited + 1))
        if [ $waited -ge $max_wait ]; then
            echo -e " ${RED}TIMEOUT (${max_wait}s)${RESET}" >&2
            return 1
        fi
    done
    echo -e " ${GREEN}ready${RESET} (${waited}s)"
}

# ── Banner ────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}${CYAN}════════════════════════════════════════════════════${RESET}"
echo -e "${BOLD}${CYAN}  EAASP v2.0 — Development Server${RESET}"
echo -e "${BOLD}${CYAN}════════════════════════════════════════════════════${RESET}"
echo ""

# ── Pre-flight checks ────────────────────────────────────────────────────
echo -e "${BOLD}=== Pre-flight: port availability ===${RESET}"
check_port_free $SKILL_REG_PORT "skill-registry"
check_port_free $L2_MEM_PORT "L2 memory-engine"
check_port_free $L3_GOV_PORT "L3 governance"
check_port_free $L4_ORCH_PORT "L4 orchestration"
check_port_free $GRID_RT_PORT "grid-runtime"
check_port_free $CLAUDE_RT_PORT "claude-code-runtime"
[ "$SKIP_NANOBOT" = false ] && check_port_free $NANOBOT_RT_PORT "nanobot-runtime"
[ "$SKIP_GOOSE" = false ] && check_port_free $GOOSE_RT_PORT "goose-runtime"
check_port_free $MOCK_SCADA_SSE_PORT "mock-scada-sse"
echo -e "  ${GREEN}All ports free.${RESET}"
echo ""

echo -e "${BOLD}=== Pre-flight: Python .venv checks ===${RESET}"
check_venv "tools/eaasp-l2-memory-engine" "l2-memory-setup"
check_venv "tools/eaasp-l3-governance" "l3-setup"
check_venv "tools/eaasp-l4-orchestration" "l4-setup"
check_venv "lang/claude-code-runtime-python" "claude-runtime-setup"
# nanobot-runtime .venv (Phase 2.5 W2) — non-fatal: skip if missing
if [ "$SKIP_NANOBOT" = false ]; then
    if [ ! -x "$PROJECT_ROOT/lang/nanobot-runtime-python/.venv/bin/python" ]; then
        echo -e "  ${YELLOW}WARN${RESET}: lang/nanobot-runtime-python/.venv is missing. Nanobot will be skipped."
        echo "         To enable: cd lang/nanobot-runtime-python && uv sync"
        SKIP_NANOBOT=true
    else
        echo -e "  ${GREEN}nanobot-runtime .venv present.${RESET}"
    fi
fi
# goose-runtime uses Docker (Phase 2.5 W1) — check image exists (non-fatal: skip if missing)
if [ "$SKIP_GOOSE" = false ]; then
    if ! docker image inspect eaasp-goose-runtime:dev >/dev/null 2>&1; then
        echo -e "  ${YELLOW}WARN${RESET}: Docker image eaasp-goose-runtime:dev not found. Goose will be skipped."
        echo "         To enable: make goose-runtime-container-build"
        SKIP_GOOSE=true
    else
        echo -e "  ${GREEN}eaasp-goose-runtime:dev Docker image found.${RESET}"
    fi
fi
echo -e "  ${GREEN}All .venvs present.${RESET}"
echo ""

# ── Pre-flight: .env and required LLM config ────────────────────────────
echo -e "${BOLD}=== Pre-flight: .env + LLM provider config ===${RESET}"
if [ ! -f "$PROJECT_ROOT/.env" ]; then
    echo -e "${RED}ERROR${RESET}: .env file not found at $PROJECT_ROOT/.env" >&2
    echo "       Copy .env.example to .env and fill in API keys." >&2
    exit 1
fi
# Source .env so we can validate before launching services.
set -a
source "$PROJECT_ROOT/.env"
set +a

# grid-runtime requires: LLM_PROVIDER, OPENAI_API_KEY (or ANTHROPIC_API_KEY), LLM_MODEL
_require_env() {
    local var=$1
    local context=$2
    if [ -z "${!var:-}" ]; then
        echo -e "${RED}ERROR${RESET}: $var is required ($context)." >&2
        echo "       Set it in .env or shell environment." >&2
        exit 1
    fi
}

_require_env "LLM_PROVIDER" "grid-runtime: e.g. openai or anthropic"

if [ "$LLM_PROVIDER" = "anthropic" ]; then
    _require_env "ANTHROPIC_API_KEY" "grid-runtime: LLM_PROVIDER=anthropic"
    _require_env "ANTHROPIC_MODEL_NAME" "grid-runtime: e.g. claude-sonnet-4-20250514"
else
    _require_env "OPENAI_API_KEY" "grid-runtime: LLM_PROVIDER=$LLM_PROVIDER"
    _require_env "OPENAI_BASE_URL" "grid-runtime: required for OpenRouter routing"
    _require_env "OPENAI_MODEL_NAME" "grid-runtime: e.g. gpt-4o or anthropic/claude-sonnet-4-20250514"
fi

_require_env "ANTHROPIC_API_KEY" "claude-code-runtime: Claude Agent SDK"

echo -e "  LLM_PROVIDER=${CYAN}${LLM_PROVIDER}${RESET}"
if [ "$LLM_PROVIDER" = "anthropic" ]; then
    echo -e "  ANTHROPIC_MODEL_NAME=${CYAN}${ANTHROPIC_MODEL_NAME}${RESET}"
else
    echo -e "  OPENAI_MODEL_NAME=${CYAN}${OPENAI_MODEL_NAME}${RESET}"
    echo -e "  OPENAI_BASE_URL=${CYAN}${OPENAI_BASE_URL}${RESET}"
fi
echo -e "  ${GREEN}All LLM config present.${RESET}"
echo ""

# ── Clean stale dev sessions ────────────────────────────────────────────
# L1 runtimes keep sessions in memory — they're lost on restart.
# Wipe L4 dev DB sessions so `session list` stays clean.
if [ -f "$PROJECT_ROOT/data/dev-l4.db" ]; then
    sqlite3 "$PROJECT_ROOT/data/dev-l4.db" \
        "DELETE FROM sessions; DELETE FROM session_events;" 2>/dev/null || true
    echo -e "${BOLD}=== Cleaned stale dev sessions ===${RESET}"
    echo ""
fi

# ── Runtime workspace ────────────────────────────────────────────────────
# All L1 Runtimes use this as their base workspace directory.
# Each session creates a subdirectory under it. This isolates runtime
# execution from the development environment (.claude/, hooks, etc.).
# In production, this would be a container-internal mount point.
RUNTIME_WORKSPACE="$PROJECT_ROOT/data/runtime-workspace"
mkdir -p "$RUNTIME_WORKSPACE"
export EAASP_RUNTIME_WORKSPACE="$RUNTIME_WORKSPACE"
echo -e "${BOLD}=== Runtime workspace: ${RUNTIME_WORKSPACE} ===${RESET}"
echo ""

# All services launch from $PROJECT_ROOT — no cd.
cd "$PROJECT_ROOT"

# ── Step 1: Cargo build ──────────────────────────────────────────────────
if [ "$SKIP_BUILD" = false ]; then
    echo -e "${BOLD}=== Building Rust binaries ===${RESET}"
    cargo build -p grid-runtime -p eaasp-skill-registry 2>&1 | tail -5
    echo -e "  ${GREEN}Build complete.${RESET}"
    echo ""
else
    echo -e "${BOLD}=== Skipping Rust build (--skip-build) ===${RESET}"
    echo ""
fi

# ── Step 2: Start skill-registry ─────────────────────────────────────────
echo -e "${BOLD}=== Starting skill-registry on :${SKILL_REG_PORT} ===${RESET}"
mkdir -p "$PROJECT_ROOT/data/dev-skill-registry"
EAASP_SKILL_REGISTRY_PORT=$SKILL_REG_PORT \
EAASP_SKILL_REGISTRY_HOST=127.0.0.1 \
    "$PROJECT_ROOT/target/debug/eaasp-skill-registry" \
        --data-dir "$PROJECT_ROOT/data/dev-skill-registry" 2>&1 | sed 's/^/  [skill-reg] /' &
SKILL_REG_PID=$!
echo "  PID: $SKILL_REG_PID"
wait_for_port $SKILL_REG_PORT "skill-registry"

# ── Step 3: Start L2 memory-engine ───────────────────────────────────────
echo ""
echo -e "${BOLD}=== Starting L2 memory-engine on :${L2_MEM_PORT} ===${RESET}"
mkdir -p "$PROJECT_ROOT/data"
EAASP_L2_PORT=$L2_MEM_PORT \
EAASP_L2_HOST=127.0.0.1 \
EAASP_L2_DB_PATH="$PROJECT_ROOT/data/dev-l2.db" \
    "$PROJECT_ROOT/tools/eaasp-l2-memory-engine/.venv/bin/python" \
        -m eaasp_l2_memory_engine.main 2>&1 | sed 's/^/  [l2-mem]    /' &
L2_PID=$!
echo "  PID: $L2_PID"
wait_for_port $L2_MEM_PORT "L2 memory-engine"

# ── Step 4: Start L3 governance ──────────────────────────────────────────
echo ""
echo -e "${BOLD}=== Starting L3 governance on :${L3_GOV_PORT} ===${RESET}"
EAASP_L3_PORT=$L3_GOV_PORT \
EAASP_L3_HOST=127.0.0.1 \
EAASP_L3_DB_PATH="$PROJECT_ROOT/data/dev-l3.db" \
    "$PROJECT_ROOT/tools/eaasp-l3-governance/.venv/bin/python" \
        -m eaasp_l3_governance.main 2>&1 | sed 's/^/  [l3-gov]    /' &
L3_PID=$!
echo "  PID: $L3_PID"
wait_for_port $L3_GOV_PORT "L3 governance"

# ── Step 4b: Start mock-scada SSE (tool-sandbox for hermes-runtime) ──────
echo ""
echo -e "${BOLD}=== Starting mock-scada SSE on :${MOCK_SCADA_SSE_PORT} ===${RESET}"
NO_PROXY=127.0.0.1,localhost \
    "$PROJECT_ROOT/tools/mock-scada/.venv/bin/mock-scada" \
        --transport sse --host 0.0.0.0 --port "$MOCK_SCADA_SSE_PORT" 2>&1 | sed 's/^/  [scada-sse] /' &
MOCK_SCADA_SSE_PID=$!
echo "  PID: $MOCK_SCADA_SSE_PID"
wait_for_port $MOCK_SCADA_SSE_PORT "mock-scada-sse"

# ── Step 4c: Start L2 MCP Orchestrator ──────────────────────────────────
echo ""
echo -e "${BOLD}=== Starting L2 MCP Orchestrator on :${MCP_ORCH_PORT} ===${RESET}"
EAASP_MCP_ORCHESTRATOR_PORT=$MCP_ORCH_PORT \
EAASP_MCP_ORCHESTRATOR_HOST=127.0.0.1 \
    "$PROJECT_ROOT/target/debug/eaasp-mcp-orchestrator" \
        --config "$PROJECT_ROOT/tools/eaasp-mcp-orchestrator/config/mcp-servers.yaml" \
        --port "$MCP_ORCH_PORT" --host 127.0.0.1 2>&1 | sed 's/^/  [mcp-orch]  /' &
MCP_ORCH_PID=$!
echo "  PID: $MCP_ORCH_PID"
wait_for_port $MCP_ORCH_PORT "L2 MCP Orchestrator"

# ── Step 5: Start grid-runtime ───────────────────────────────────────────
echo ""
echo -e "${BOLD}=== Starting grid-runtime on :${GRID_RT_PORT} ===${RESET}"
GRID_RUNTIME_PORT=$GRID_RT_PORT \
RUST_LOG=grid_runtime=info,grid_engine=info \
EAASP_L2_DB_PATH="$PROJECT_ROOT/data/dev-l2.db" \
PATH="$PROJECT_ROOT/tools/mock-scada/.venv/bin:$PROJECT_ROOT/tools/eaasp-l2-memory-engine/.venv/bin:$PATH" \
    "$PROJECT_ROOT/target/debug/grid-runtime" 2>&1 | sed 's/^/  [grid-rt]   /' &
GRID_PID=$!
echo "  PID: $GRID_PID"
wait_for_port $GRID_RT_PORT "grid-runtime"

# ── Step 6: Start claude-code-runtime ────────────────────────────────────
echo ""
echo -e "${BOLD}=== Starting claude-code-runtime on :${CLAUDE_RT_PORT} ===${RESET}"
CLAUDE_RUNTIME_PORT=$CLAUDE_RT_PORT \
EAASP_L2_DB_PATH="$PROJECT_ROOT/data/dev-l2.db" \
PATH="$PROJECT_ROOT/tools/mock-scada/.venv/bin:$PROJECT_ROOT/tools/eaasp-l2-memory-engine/.venv/bin:$PATH" \
    "$PROJECT_ROOT/lang/claude-code-runtime-python/.venv/bin/python" \
        -m claude_code_runtime --port "$CLAUDE_RT_PORT" 2>&1 | sed 's/^/  [claude-rt] /' &
CLAUDE_PID=$!
echo "  PID: $CLAUDE_PID"
wait_for_port $CLAUDE_RT_PORT "claude-code-runtime"

# ── Step 7: Start nanobot-runtime (Phase 2.5 W2, native Python) ─────────
if [ "$SKIP_NANOBOT" = false ]; then
    echo ""
    echo -e "${BOLD}=== Starting nanobot-runtime on :${NANOBOT_RT_PORT} ===${RESET}"
    NANOBOT_RUNTIME_PORT=$NANOBOT_RT_PORT \
    OPENAI_BASE_URL="${OPENAI_BASE_URL:-}" \
    OPENAI_API_KEY="${OPENAI_API_KEY:-}" \
    OPENAI_MODEL_NAME="${OPENAI_MODEL_NAME:-gpt-4o-mini}" \
    EAASP_L2_DB_PATH="$PROJECT_ROOT/data/dev-l2.db" \
    PATH="$PROJECT_ROOT/tools/mock-scada/.venv/bin:$PROJECT_ROOT/tools/eaasp-l2-memory-engine/.venv/bin:$PATH" \
        "$PROJECT_ROOT/lang/nanobot-runtime-python/.venv/bin/python" \
            -m nanobot_runtime 2>&1 | sed 's/^/  [nanobot-rt]/' &
    NANOBOT_PID=$!
    echo "  PID: $NANOBOT_PID"
    wait_for_port $NANOBOT_RT_PORT "nanobot-runtime"
else
    echo ""
    echo -e "${YELLOW}=== Skipping nanobot-runtime ===${RESET}"
fi

# ── Step 8: Start goose-runtime (Phase 2.5 W1, Docker, ADR-V2-019) ──────
if [ "$SKIP_GOOSE" = false ]; then
    echo ""
    echo -e "${BOLD}=== Starting goose-runtime on :${GOOSE_RT_PORT} (Docker) ===${RESET}"
    GOOSE_CONTAINER="eaasp-goose-runtime"
    # Remove stale container if exists
    docker rm -f "$GOOSE_CONTAINER" >/dev/null 2>&1 || true
    # macOS Docker Desktop does not support --network host; use -p port mapping.
    # Run in detached mode, stream logs via docker logs -f.
    docker run --rm -d \
        --name "$GOOSE_CONTAINER" \
        -p "${GOOSE_RT_PORT}:${GOOSE_RT_PORT}" \
        -e GOOSE_RUNTIME_GRPC_ADDR="0.0.0.0:${GOOSE_RT_PORT}" \
        -e EAASP_DEPLOYMENT_MODE="${EAASP_DEPLOYMENT_MODE:-shared}" \
        -e OPENAI_BASE_URL="${OPENAI_BASE_URL:-}" \
        -e OPENAI_API_KEY="${OPENAI_API_KEY:-}" \
        -e OPENAI_MODEL_NAME="${OPENAI_MODEL_NAME:-gpt-4o-mini}" \
        -e EAASP_L2_HOST=host.docker.internal \
        -e EAASP_L2_PORT="$L2_MEM_PORT" \
        -e NO_PROXY=host.docker.internal,127.0.0.1,localhost \
        eaasp-goose-runtime:dev >/dev/null 2>&1
    # Stream container logs in background
    docker logs -f "$GOOSE_CONTAINER" 2>&1 | sed 's/^/  [goose-rt]  /' &
    GOOSE_PID=$!
    echo "  Container: $GOOSE_CONTAINER"
    wait_for_port $GOOSE_RT_PORT "goose-runtime"
else
    echo ""
    echo -e "${YELLOW}=== Skipping goose-runtime (Docker image not found or --skip-goose) ===${RESET}"
fi

# ── Step 8: Start L4 orchestration ───────────────────────────────────────
echo ""
echo -e "${BOLD}=== Starting L4 orchestration on :${L4_ORCH_PORT} ===${RESET}"
EAASP_L4_PORT=$L4_ORCH_PORT \
EAASP_L4_HOST=127.0.0.1 \
EAASP_L4_DB_PATH="$PROJECT_ROOT/data/dev-l4.db" \
EAASP_L2_URL="http://127.0.0.1:${L2_MEM_PORT}" \
EAASP_L3_URL="http://127.0.0.1:${L3_GOV_PORT}" \
EAASP_MCP_ORCH_URL="http://127.0.0.1:${MCP_ORCH_PORT}" \
    "$PROJECT_ROOT/tools/eaasp-l4-orchestration/.venv/bin/python" \
        -m eaasp_l4_orchestration.main 2>&1 | sed 's/^/  [l4-orch]   /' &
L4_PID=$!
echo "  PID: $L4_PID"
wait_for_port $L4_ORCH_PORT "L4 orchestration"

# ── Status table ─────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}${CYAN}════════════════════════════════════════════════════${RESET}"
echo -e "${BOLD}${CYAN}  All EAASP services running${RESET}"
echo -e "${BOLD}${CYAN}════════════════════════════════════════════════════${RESET}"
echo ""
printf "  ${BOLD}%-24s %-8s %-8s %-10s %-8s${RESET}\n" "SERVICE" "PORT" "PID" "PROVIDER" "STATUS"
printf "  %-24s %-8s %-8s %-10s %-8s\n"   "────────────────────────" "────────" "────────" "──────────" "────────"
printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "skill-registry"       "$SKILL_REG_PORT" "$SKILL_REG_PID" "-"          "UP"
printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "L2 memory-engine"     "$L2_MEM_PORT"    "$L2_PID"        "-"          "UP"
printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "L3 governance"        "$L3_GOV_PORT"    "$L3_PID"        "-"          "UP"
printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "mock-scada(SSE)"      "$MOCK_SCADA_SSE_PORT" "$MOCK_SCADA_SSE_PID" "tool-sandbox" "UP"
printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "L2 MCP Orchestrator"  "$MCP_ORCH_PORT"  "$MCP_ORCH_PID"  "-"          "UP"
printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "grid-runtime"         "$GRID_RT_PORT"   "$GRID_PID"      "OPENAI_*"   "UP"
printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "claude-code-runtime"  "$CLAUDE_RT_PORT" "$CLAUDE_PID"    "ANTHROPIC_*" "UP"
if [ "$SKIP_NANOBOT" = false ]; then
    printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "nanobot-runtime"      "$NANOBOT_RT_PORT" "$NANOBOT_PID"   "OPENAI_*"   "UP"
else
    printf "  %-24s %-8s %-8s %-10s ${YELLOW}%-8s${RESET}\n" "nanobot-runtime"      "$NANOBOT_RT_PORT" "-"              "-"          "SKIPPED"
fi
if [ "$SKIP_GOOSE" = false ]; then
    GOOSE_CID=$(docker inspect --format '{{.State.Pid}}' eaasp-goose-runtime 2>/dev/null || echo "?")
    printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "goose-runtime(docker)" "$GOOSE_RT_PORT" "$GOOSE_CID"     "OPENAI_*"   "UP"
else
    printf "  %-24s %-8s %-8s %-10s ${YELLOW}%-8s${RESET}\n" "goose-runtime(docker)" "$GOOSE_RT_PORT" "-"              "-"          "SKIPPED"
fi
printf "  %-24s %-8s %-8s %-10s ${GREEN}%-8s${RESET}\n" "L4 orchestration"     "$L4_ORCH_PORT"   "$L4_PID"        "-"          "UP"
echo ""
echo -e "  ${YELLOW}Press Ctrl+C to stop all services.${RESET}"
echo ""

# ── Stay foreground — wait for any child to exit ─────────────────────────
wait
