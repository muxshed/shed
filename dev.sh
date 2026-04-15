#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Run Muxshed locally from source.
# API on :8080, frontend dev server on :5173 with proxy to API.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

cleanup() {
    echo "Stopping..."
    kill $API_PID $WEB_PID 2>/dev/null
    wait $API_PID $WEB_PID 2>/dev/null
}
trap cleanup EXIT

echo "Building API..."
cargo build -p muxshed-api

echo "Starting API on :8080..."
MUXSHED_LOG_LEVEL=debug RUST_LOG=debug cargo run -p muxshed-api &
API_PID=$!

echo "Installing frontend dependencies..."
cd web
npm install --silent

echo "Starting frontend dev server on :5173..."
npm run dev &
WEB_PID=$!

cd "$SCRIPT_DIR"

echo ""
echo "============================================"
echo "  Muxshed dev running"
echo "  Frontend: http://localhost:5173"
echo "  API:      http://localhost:8080"
echo "  Press Ctrl+C to stop"
echo "============================================"
echo ""

wait
