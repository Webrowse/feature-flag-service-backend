# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.85-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependency compilation separately from source changes.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN cargo build --release
RUN rm -f src/main.rs

# Build the real binary. SQLX_OFFLINE uses the committed .sqlx cache.
COPY . .
ENV SQLX_OFFLINE=true
RUN touch src/main.rs && cargo build --release

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Run as a non-root user.
RUN useradd -r -u 1001 -s /bin/false appuser

COPY --from=builder /app/target/release/axum-api-template /usr/local/bin/app
COPY --from=builder /app/migrations /migrations

USER appuser

EXPOSE 8080

CMD ["app"]
