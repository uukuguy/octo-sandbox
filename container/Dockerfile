# Octo Sandbox Container Image
# Minimal tool execution environment for sandboxed agent operations.
# Derived from container.sample/Dockerfile.base, stripped of Claude Code specifics.
#
# Build: docker build -t octo-sandbox:base container/
# Dev:   docker build -f container/Dockerfile.dev -t octo-sandbox:dev container/

# ==============================================================================
# Stage 1: base-system — OS packages + Python + system CLI tools
# ==============================================================================
FROM node:22-slim AS base-system

# Optional: pip mirror for builds behind firewalls or in China mainland
# Usage: docker build --build-arg PIP_INDEX_URL=https://pypi.tuna.tsinghua.edu.cn/simple ...
ARG PIP_INDEX_URL
ENV PIP_INDEX_URL=${PIP_INDEX_URL}

# System packages: git, curl, python3, openssh, etc.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    gnupg \
    jq \
    openssh-client \
    python3 \
    python3-pip \
    python3-venv \
    wget \
    # --- AD-T1: Document processing ---
    poppler-utils \
    pandoc \
    tesseract-ocr \
    tesseract-ocr-chi-sim \
    tesseract-ocr-chi-tra \
    # --- AD-T2: Database clients ---
    postgresql-client \
    default-mysql-client \
    sqlite3 \
    # --- AD-T2: Network & file tools ---
    dnsutils \
    netcat-openbsd \
    openssl \
    zip \
    unzip \
    file \
    tree \
    && rm -rf /var/lib/apt/lists/*

# Symlink python -> python3 for convenience
RUN ln -sf /usr/bin/python3 /usr/bin/python

# Install Python base packages
COPY scripts/requirements-base.txt scripts/install-python-packages.sh /tmp/
RUN chmod +x /tmp/install-python-packages.sh \
    && /tmp/install-python-packages.sh /tmp/requirements-base.txt \
    && rm /tmp/requirements-base.txt /tmp/install-python-packages.sh

# Install modern CLI tools from GitHub Releases (rg, fd, bat, eza, etc.)
COPY scripts/install-cli-tools.sh /tmp/install-cli-tools.sh
RUN chmod +x /tmp/install-cli-tools.sh && /tmp/install-cli-tools.sh && rm /tmp/install-cli-tools.sh

# ==============================================================================
# Stage 2: octo-sandbox:base — Workspace setup + non-root user
# ==============================================================================
FROM base-system AS base

LABEL octo.sandbox="true"
LABEL octo.version="1.0"
LABEL org.opencontainers.image.title="octo-sandbox"
LABEL org.opencontainers.image.description="Octo agent tool execution sandbox"

# Create sandbox user (non-root)
RUN groupadd -g 1000 sandbox \
    && useradd -u 1000 -g sandbox -m -s /bin/bash sandbox

# Create workspace directories
RUN mkdir -p /workspace/project \
             /workspace/session \
    && chown -R sandbox:sandbox /workspace/session

# Health check: verify basic tools are available
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD python3 --version && git --version && bash --version || exit 1

# Switch to non-root user
USER sandbox

# Working directory for tool execution
WORKDIR /workspace/session

# Keep container alive for exec calls
ENTRYPOINT ["sleep", "infinity"]
