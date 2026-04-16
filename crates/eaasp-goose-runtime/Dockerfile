# crates/eaasp-goose-runtime/Dockerfile
# eaasp-goose-runtime — EAASP L1 goose adapter, Shared multi-session default per ADR-V2-019.
#
# Build from PROJECT ROOT:
#   docker build -f crates/eaasp-goose-runtime/Dockerfile -t eaasp-goose-runtime:dev .
# Run:
#   docker run --rm -p 50063:50063 \
#     -e EAASP_DEPLOYMENT_MODE=shared \
#     -e OPENAI_BASE_URL=http://host.docker.internal:18099 \
#     -e OPENAI_API_KEY=sk-test \
#     -e OPENAI_MODEL_NAME=gpt-4o-mini \
#     eaasp-goose-runtime:dev

# ── Build stage ──
FROM rust:1.92-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY tools/ tools/
COPY proto/ proto/

RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler && rm -rf /var/lib/apt/lists/*

RUN cargo build --release -p eaasp-goose-runtime --bin eaasp-goose-runtime

# ── Runtime stage ──
FROM debian:bookworm-slim

# Install goose CLI from GitHub release (per ADR-V2-019 D2; version pinned for reproducibility).
#
# NOTE on download_cli.sh semantics (deviation from plan §W1.T2.5 template, approved 2026-04-16):
#   - URL path must always be /releases/download/stable/download_cli.sh — the "stable" release
#     tag hosts the installer script itself, not a per-version symlink. Interpolating
#     GOOSE_VERSION into the URL path breaks the script: its internal semver validator rejects
#     the literal word "stable" at runtime:
#       [error]: invalid version 'stable'. expected: vX.Y.Z, vX.Y.Z-suffix, or X.Y.Z
#   - GOOSE_VERSION is an env var READ BY THE SCRIPT at runtime to choose which tarball to
#     fetch, so it must be piped to `bash`, not templated into the URL.
# github.com/block/goose 301-redirects to github.com/aaif-goose/goose transparently (canonical
# repo was moved); both URLs work identically.
# Runtime lib: goose v1.30.0 arm64 binary dynamically links libgomp.so.1 (GCC OpenMP runtime),
# which is NOT in debian:bookworm-slim. Must install AND keep after purge (unlike curl/bzip2).
# ldd line is diagnostic-only (prints resolved shared libs to build log) — does not fail build.
ARG GOOSE_VERSION=v1.30.0
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl bzip2 libgomp1 && \
    curl -fsSL "https://github.com/block/goose/releases/download/stable/download_cli.sh" | \
      CONFIGURE=false GOOSE_VERSION="${GOOSE_VERSION}" bash && \
    mv "$HOME/.local/bin/goose" /usr/local/bin/goose && \
    chmod +x /usr/local/bin/goose && \
    ldd /usr/local/bin/goose && \
    goose --version && \
    apt-get purge -y curl bzip2 && apt-get autoremove -y && rm -rf /var/lib/apt/lists/*

# Copy runtime binary
COPY --from=builder /build/target/release/eaasp-goose-runtime /usr/local/bin/

# Default deployment mode: shared multi-session (ADR-V2-019 D2)
ENV EAASP_DEPLOYMENT_MODE=shared

EXPOSE 50063

# F1 gate check inlined — if goose binary is broken, container fails fast
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD goose --version >/dev/null 2>&1 || exit 1

ENTRYPOINT ["/usr/local/bin/eaasp-goose-runtime"]
CMD ["--port", "50063"]
