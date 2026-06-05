# syntax=docker/dockerfile:1.7

FROM rust:1-slim-bookworm AS builder

WORKDIR /app

ARG APP_BIN=gowa-webhook-api

COPY Cargo.toml Cargo.lock ./

RUN mkdir src \
    && echo "fn main() {}" > src/main.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin ${APP_BIN}

RUN rm -rf src

COPY src ./src

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin ${APP_BIN} \
    && cp target/release/${APP_BIN} /app/server

FROM debian:bookworm-slim AS runtime

WORKDIR /app

COPY --from=builder /app/server /app/server

EXPOSE 8000

CMD ["/app/server"]