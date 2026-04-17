# Licensed under the Business Source License 1.1 — see LICENSE.

# syntax=docker/dockerfile:1.7

# --- cargo-chef: pre-built image (skips 5min cargo-chef compile) ---
FROM lukemathwalker/cargo-chef:latest-rust-1.87 AS chef
WORKDIR /build

FROM chef AS planner
COPY Cargo.toml Cargo.toml
COPY crates/ crates/
RUN cargo chef prepare --recipe-path recipe.json

# --- Rust builder ---
FROM chef AS rust-builder

COPY --from=planner /build/recipe.json recipe.json

# Build dependencies only. Cached unless Cargo.toml changes.
# The runtime pipeline uses FFmpeg; GStreamer feature is not built to save
# ~15-20min of compile time. Enable it here if you need GstPipelineController.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    CARGO_PROFILE_RELEASE_LTO=false \
    cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml Cargo.toml
COPY crates/ crates/
COPY migrations/ migrations/

ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    CARGO_PROFILE_RELEASE_LTO=false \
    cargo build --release && \
    cp /build/target/release/muxshed-api /usr/local/bin/muxshed-api

# --- Web builder ---
FROM node:24-slim AS web-builder
WORKDIR /build

COPY web/package.json web/package-lock.json* ./
RUN --mount=type=cache,target=/root/.npm npm ci

COPY web/ .
RUN npm run build

# --- Runtime ---
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y --no-install-recommends \
    ffmpeg \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /usr/local/bin/muxshed-api /usr/local/bin/
COPY --from=web-builder /build/build /app/web

ENV MUXSHED_DB_PATH=/config/muxshed.db
ENV MUXSHED_DATA_DIR=/data
ENV MUXSHED_WEB_DIR=/app/web
ENV MUXSHED_LISTEN_ADDR=0.0.0.0:8080
ENV MUXSHED_LOG_LEVEL=info
ENV RUST_LOG=info

EXPOSE 8080
EXPOSE 1935
EXPOSE 9000-9099/udp

VOLUME /config
VOLUME /data

CMD ["/usr/local/bin/muxshed-api"]
