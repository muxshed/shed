#!/usr/bin/env bash
# Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

# Run Muxshed in Podman (dev mode, no GStreamer).

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

podman compose -f docker/docker-compose.dev.yml up --build "$@"
