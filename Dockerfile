FROM rust:1.88-bookworm AS builder

WORKDIR /usr/src/susumu
COPY Cargo.toml Cargo.lock ./
COPY migrations migrations
COPY src src

RUN cargo build --locked --release --features server --bin susumu-server

FROM debian:bookworm-slim

RUN useradd --system --uid 10001 --create-home susumu \
    && apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/susumu/target/release/susumu-server /usr/local/bin/susumu-server

USER susumu
EXPOSE 8080
HEALTHCHECK --interval=5s --timeout=5s --start-period=5s --retries=12 CMD curl --fail --silent http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/susumu-server"]
