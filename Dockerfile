# Multi-stage Dockerfile for l7lb and mock_backend

FROM rust:slim-bookworm AS builder

WORKDIR /usr/src/l7lb

# Install system dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Cache dependency compilation
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > src/bin/mock_backend.rs && \
    cargo build --release || true && \
    rm -rf src

# Copy project source
COPY . .

# Build release binaries
RUN cargo build --release --bin l7lb --bin mock_backend

# Minimal runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /usr/src/l7lb/target/release/l7lb /usr/local/bin/l7lb
COPY --from=builder /usr/src/l7lb/target/release/mock_backend /usr/local/bin/mock_backend
COPY config.toml /app/config.toml

EXPOSE 8080 8081

ENTRYPOINT ["/usr/local/bin/l7lb"]
CMD ["--config", "/app/config.toml"]

