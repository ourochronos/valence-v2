# ==============================================================================
# Multi-stage Dockerfile for Valence Engine
# ==============================================================================
# Stage 1: Planner - Generate dependency recipe for cargo-chef
FROM rust:1.83-slim AS planner

# Install cargo-chef
RUN cargo install cargo-chef

WORKDIR /build
COPY engine/ engine/
COPY Cargo.toml Cargo.lock ./

# Generate recipe.json (dependency manifest)
RUN cd engine && cargo chef prepare --recipe-path recipe.json

# ==============================================================================
# Stage 2: Builder - Build dependencies and application
FROM rust:1.83-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-chef
RUN cargo install cargo-chef

WORKDIR /build

# Copy the dependency recipe from planner
COPY --from=planner /build/engine/recipe.json engine/recipe.json

# Build dependencies (this layer will be cached unless dependencies change)
WORKDIR /build/engine
RUN cargo chef cook --release --features postgres --recipe-path recipe.json

# Copy source code
WORKDIR /build
COPY engine/ engine/
COPY Cargo.toml Cargo.lock ./

# Build the application
WORKDIR /build/engine
RUN cargo build --release --features postgres

# ==============================================================================
# Stage 3: Runtime - Minimal runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for running the application
RUN useradd -m -u 1000 -s /bin/bash valence

# Copy binary from builder
COPY --from=builder /build/engine/target/release/valence-engine /usr/local/bin/

# Set ownership
RUN chown valence:valence /usr/local/bin/valence-engine

# Switch to non-root user
USER valence

# Expose HTTP port
EXPOSE 8421

# Health check
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8421/health || exit 1

# Set default environment variables
ENV RUST_LOG=info \
    VALENCE_HOST=0.0.0.0 \
    VALENCE_PORT=8421

ENTRYPOINT ["valence-engine"]
CMD ["--host", "0.0.0.0", "--port", "8421"]
