# Build stage
FROM rust:1.92-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy Cargo files for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/octo-types/Cargo.toml crates/octo-types/
COPY crates/octo-engine/Cargo.toml crates/octo-engine/
COPY crates/octo-server/Cargo.toml crates/octo-server/

# Create dummy source files for dependency resolution
RUN mkdir -p crates/octo-types/src && echo "pub mod octo_types;" > crates/octo-types/src/lib.rs
RUN mkdir -p crates/octo-engine/src && echo "pub mod octo_engine;" > crates/octo-engine/src/lib.rs
RUN mkdir -p crates/octo-server/src && echo "pub mod octo_server;" > crates/octo-server/src/main.rs

# Build dependencies
RUN cargo build --release -p octo-types -p octo-engine -p octo-server || true

# Copy actual source
COPY crates crates

# Build the server
RUN cargo build --release -p octo-server

# Frontend build stage
FROM node:20-slim AS frontend-builder

WORKDIR /web

# Install pnpm
RUN npm install -g pnpm

# Copy web files
COPY web/package.json web/pnpm-lock.yaml ./

# Install dependencies
RUN pnpm install --frozen-lockfile

COPY web/ .

# Build frontend
RUN pnpm run build

# Production stage
# Use ubuntu noble which has glibc 2.39
FROM ubuntu:noble

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /app/target/release/octo-server .

# Copy frontend from builder
COPY --from=frontend-builder /web/dist /usr/share/nginx/html

# Copy config
COPY config.yaml .

# Create data directory
RUN mkdir -p data

# Expose ports
EXPOSE 3001 5180

# Environment variables
ENV RUST_LOG=info
ENV OCTO_HOST=0.0.0.0
ENV OCTO_PORT=3001

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3001/api/health || exit 1

# Run the server
CMD ["./octo-server"]
