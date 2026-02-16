# Stage 1: Build
FROM rust:1.83-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY engine/ engine/

WORKDIR /build/engine
RUN cargo build --release --features postgres

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/engine/target/release/valence-engine /usr/local/bin/

EXPOSE 8421

ENTRYPOINT ["valence-engine"]
CMD ["--host", "0.0.0.0", "--port", "8421"]
