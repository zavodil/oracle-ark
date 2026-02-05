# Build stage
FROM rust:1.85-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy sources library first (dependency)
COPY sources /sources

# Copy scheduler source
COPY scheduler .

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/oracle-scheduler /usr/local/bin/

# Set environment defaults
ENV RUST_LOG=info
ENV TOKENS_CONFIG=/etc/oracle-scheduler/tokens.json

ENTRYPOINT ["oracle-scheduler"]
