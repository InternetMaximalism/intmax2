# for build tools
FROM --platform=$BUILDPLATFORM rust:latest AS tools
RUN apt-get update && apt-get install -y \
    build-essential \
    && rm -rf /var/lib/apt/lists/*
RUN rustup toolchain install nightly-2024-09-17 \
    && rustup default nightly

RUN --mount=type=cache,target=/user/local/cargo/registry \
    cargo install cargo-chef --version ^0.1

# for build base
FROM --platform=$BUILDPLATFORM rust:latest AS base
RUN apt-get update && apt-get install -y \
    build-essential \
    && rm -rf /var/lib/apt/lists/*
RUN rustup toolchain install nightly-2024-09-17 \
    && rustup default nightly

COPY --from=tools /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/

FROM base AS planner
WORKDIR /usr/src/app
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /usr/src/app
COPY --from=planner /usr/src/app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo chef cook --release --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build -r

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/store-vault-server /app/
COPY --from=builder /usr/src/app/target/release/withdrawal-server /app/
COPY --from=builder /usr/src/app/target/release/block-builder /app/
COPY --from=builder /usr/src/app/target/release/validity-prover /app/
COPY --from=builder /usr/src/app/target/release/validity-prover-worker /app/

RUN chmod +x /app/store-vault-server \
    && chmod +x /app/withdrawal-server \
    && chmod +x /app/block-builder \
    && chmod +x /app/validity-prover \
    && chmod +x /app/validity-prover-worker

ARG PORT

ENV PORT=${PORT}
EXPOSE ${PORT}

ENTRYPOINT ["/bin/sh", "-c"]
