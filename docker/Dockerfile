# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# syntax=docker/dockerfile:1.7

# --- cargo-chef: analyze dependency graph ---
FROM rust:1.87-slim AS chef
RUN cargo install cargo-chef --locked
WORKDIR /build

FROM chef AS planner
COPY Cargo.toml Cargo.toml
COPY crates/ crates/
RUN cargo chef prepare --recipe-path recipe.json

# --- Rust builder: build deps first (cached), then app ---
FROM chef AS rust-builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /build/recipe.json recipe.json

# Build dependencies only — cached unless Cargo.toml / Cargo.lock change
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo chef cook --release --features muxshed-processor/gstreamer --recipe-path recipe.json

# Build application — only this layer changes when app code changes
COPY Cargo.toml Cargo.toml
COPY crates/ crates/
COPY migrations/ migrations/

ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --release --features muxshed-processor/gstreamer && \
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
    gstreamer1.0-tools \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav \
    gstreamer1.0-gl \
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
