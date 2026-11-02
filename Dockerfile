# Build stage
FROM rust:1.83-bookworm AS builder

WORKDIR /app

# Install system dependencies for rquickjs and image processing
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY fixtures ./fixtures

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and fixtures
COPY --from=builder /app/target/release/qwen-protocol-register /usr/local/bin/
COPY --from=builder /app/fixtures ./fixtures

# Create non-root user
RUN useradd -r -s /bin/false appuser && \
    mkdir -p /app/src/output && \
    chown -R appuser:appuser /app

USER appuser

# Default config location
ENV RUST_LOG=info

ENTRYPOINT ["qwen-protocol-register"]
CMD ["--help"]
