# Stage 1: Build
FROM rust:1.85-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    sqlite3 \
    libsqlite3-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

# Copy workspace configuration
COPY Cargo.toml Cargo.lock ./

# Copy crate configurations to allow dependency caching
COPY cmd/gateway/Cargo.toml cmd/gateway/Cargo.toml
COPY internal/api/Cargo.toml internal/api/Cargo.toml
COPY internal/compliance/Cargo.toml internal/compliance/Cargo.toml
COPY internal/engine/Cargo.toml internal/engine/Cargo.toml
COPY pkg/conxian-core/Cargo.toml pkg/conxian-core/Cargo.toml

# Create dummy source files for dependency caching
RUN mkdir -p cmd/gateway/src && echo "fn main() {}" > cmd/gateway/src/main.rs
RUN mkdir -p internal/api/src && touch internal/api/src/lib.rs
RUN mkdir -p internal/compliance/src && touch internal/compliance/src/lib.rs
RUN mkdir -p internal/engine/src && touch internal/engine/src/lib.rs
RUN mkdir -p pkg/conxian-core/src && touch pkg/conxian-core/src/lib.rs

# Cache dependencies
RUN cargo build --release --bin gateway

# Copy actual source code
COPY . .

# Build actual binary
RUN cargo build --release --bin gateway

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libssl3 \
    sqlite3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/app

# Copy binary from builder
COPY --from=builder /usr/src/app/target/release/gateway /usr/local/bin/conxian-gateway

# Set execution environment
ENV RUST_LOG=info
EXPOSE 3000

ENTRYPOINT ["conxian-gateway"]
