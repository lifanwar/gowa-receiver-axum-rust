FROM rust:1.88-alpine AS builder

WORKDIR /app

RUN apk add --no-cache musl-dev

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --bin gowa-webhook-api


FROM scratch

COPY --from=builder /app/target/release/gowa-webhook-api /gowa-webhook-api

USER 10001:10001

EXPOSE 8000

ENTRYPOINT ["/gowa-webhook-api"]