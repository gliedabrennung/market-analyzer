# Builds the `market-analyzer` binary (crates/cli, see root README's
# "Сборка"). Uses cargo-chef to cache the dependency-build layer
# separately from the app-source layer — `duckdb`'s `bundled` feature
# compiles DuckDB's C++ amalgamation from source (README: "первая сборка
# ... занимает 10-20 минут"), and that cost is entirely in the dependency
# graph, not in this workspace's own code. Without chef, any one-line
# change anywhere in the workspace would force that same 10-20 minute
# rebuild on every `docker build`.

FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# build-essential: C++ compiler for duckdb's bundled build (root README's
# own "Требования" — not guaranteed present in the base image).
RUN apt-get update && apt-get install -y --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin market-analyzer

FROM debian:bookworm-slim AS runtime
# ca-certificates: reqwest (Binance REST backfill) and the WS client both
# make outbound TLS connections. curl: HEALTHCHECK below, and handy for
# `docker compose exec backend curl ...` debugging.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/market-analyzer /usr/local/bin/market-analyzer
COPY docker/backend-entrypoint.sh /usr/local/bin/backend-entrypoint.sh
RUN chmod +x /usr/local/bin/backend-entrypoint.sh

# FR-6.1 (root README "Конфигурация"): everything below is just the
# `MA_*` env vars' own defaults made explicit, plus data paths pointed at
# the volume mount instead of a relative `./data` (there's no meaningful
# "current directory" once this runs as PID 1 in a container).
ENV MA_DATA_DIR=/data
ENV MA_META_DB_PATH=/data/meta.duckdb

WORKDIR /app
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=5 \
    CMD curl -sf http://localhost:8080/health || exit 1

ENTRYPOINT ["backend-entrypoint.sh"]
CMD ["serve", "--port", "8080"]
