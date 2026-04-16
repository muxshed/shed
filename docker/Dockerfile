# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# --- Rust builder ---
FROM rust:1.87 AS rust-builder

RUN apt-get update && apt-get install -y \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    libgstreamer-plugins-bad1.0-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.toml
COPY crates/ crates/
COPY migrations/ migrations/

ENV SQLX_OFFLINE=true
RUN cargo build --release --features muxshed-processor/gstreamer

# --- Web builder ---
FROM node:24-slim AS web-builder

WORKDIR /build
COPY web/package.json web/package-lock.json* ./
RUN npm ci
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

COPY --from=rust-builder /build/target/release/muxshed-api /usr/local/bin/
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
