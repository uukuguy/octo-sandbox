# grid-runtime — EAASP L1 Tier 1 Harness container image.
#
# Multi-stage build: compile in Rust builder, run in minimal image.
# L1 containers are EPHEMERAL: created per-session, destroyed on terminate.

# ── Build stage ──
FROM rust:1.86-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY proto/ proto/

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

# Build release binary
RUN cargo build -p grid-runtime --release

# ── Runtime stage ──
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/grid-runtime /usr/local/bin/grid-runtime

# gRPC port
EXPOSE 50051

# Environment defaults
ENV GRID_RUNTIME_ADDR=0.0.0.0:50051
ENV GRID_RUNTIME_ID=grid-harness
ENV RUST_LOG=grid_runtime=info

# Health check via gRPC (basic TCP check)
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s \
    CMD timeout 2 bash -c '</dev/tcp/localhost/50051' || exit 1

ENTRYPOINT ["grid-runtime"]
