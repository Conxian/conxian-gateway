# Stage 1: Build
FROM rust:1.98.1-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    sqlite3 \
    libsqlite3-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

# Build the actual source against the committed dependency graph. A dummy
# workspace can neither model all targets nor prove the shipped binary's source.
COPY . .

# Build actual binary
RUN cargo build --release --locked --bin gateway

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
