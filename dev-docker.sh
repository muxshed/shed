#!/usr/bin/env bash
# Licensed under the Business Source License 1.1 — see LICENSE.

# Run Muxshed in Docker (dev mode, no GStreamer).

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

docker compose -f docker/docker-compose.dev.yml up --build "$@"
