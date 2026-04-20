# claude-code-runtime — EAASP L1 Tier 1 Harness container image (Python).
#
# L1 containers are EPHEMERAL: created per-session, destroyed on terminate.
# Requires: ANTHROPIC_API_KEY (env var), Node.js (for Claude Code CLI).
#
# Build from PROJECT ROOT (not from this directory):
#   docker build -f lang/claude-code-runtime-python/Dockerfile -t claude-code-runtime .

# ── Build stage ──
FROM python:3.12-slim-bookworm AS builder

WORKDIR /build

# Install system deps for grpcio compilation
RUN apt-get update && \
    apt-get install -y --no-install-recommends build-essential && \
    rm -rf /var/lib/apt/lists/*

# Install uv
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

# Copy Python project
COPY lang/claude-code-runtime-python/pyproject.toml lang/claude-code-runtime-python/uv.lock ./
RUN uv sync --no-dev --frozen

# Copy source
COPY lang/claude-code-runtime-python/src/ src/
COPY scripts/gen_runtime_proto.py /build/scripts/gen_runtime_proto.py

# Copy proto definitions
COPY proto/ /proto/

# Build proto stubs. --out-dir targets the flattened /build/src layout directly
# (Dockerfile doesn't mirror the repo's lang/<pkg>/src/<mod>/ path).
RUN PROTO_ROOT=/proto uv run python scripts/gen_runtime_proto.py \
        --package-name claude-code-runtime \
        --out-dir /build/src/claude_code_runtime/_proto

# ── Runtime stage ──
FROM python:3.12-slim-bookworm

WORKDIR /app

# Install Node.js 20.x (required by claude-agent-sdk → Claude Code CLI)
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl gnupg && \
    mkdir -p /etc/apt/keyrings && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_20.x nodistro main" > /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends nodejs && \
    rm -rf /var/lib/apt/lists/*

# Install Claude Code CLI globally
RUN npm install -g @anthropic-ai/claude-code

# Copy virtualenv and source from builder
COPY --from=builder /build/.venv /app/.venv
COPY --from=builder /build/src /app/src

# Put venv on PATH
ENV PATH="/app/.venv/bin:$PATH"
ENV PYTHONPATH="/app/src"

# gRPC port
EXPOSE 50052

# Environment defaults (ANTHROPIC_API_KEY must be provided at run time)
ENV CLAUDE_RUNTIME_PORT=50052
ENV CLAUDE_RUNTIME_ID=claude-code-runtime
ENV CLAUDE_RUNTIME_NAME="Claude Code Runtime"

# Health check (TCP probe on gRPC port)
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s \
    CMD python -c "import socket; s=socket.socket(); s.settimeout(2); s.connect(('localhost',50052)); s.close()"

ENTRYPOINT ["python", "-m", "claude_code_runtime"]
