#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

HOST="${XR_HOST:-127.0.0.1}"
PORT="${XR_PORT:-8900}"
OPENAI_COMPAT="${ENABLE_OPENAI_COMPATIBLE_API:-false}"

echo "[dev] starting xrouter-app on ${HOST}:${PORT}"
echo "[dev] ENABLE_OPENAI_COMPATIBLE_API=${OPENAI_COMPAT}"

if [[ -f .env ]]; then
  echo "[dev] using .env from ${ROOT_DIR}/.env"
fi

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "[dev] stopping server (pid=${SERVER_PID})"
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

XR_HOST="$HOST" XR_PORT="$PORT" ENABLE_OPENAI_COMPATIBLE_API="$OPENAI_COMPAT" \
  cargo run -p xrouter-app &
SERVER_PID=$!

HEALTH_URL="http://${HOST}:${PORT}/health"
for _ in {1..60}; do
  if curl -fsS "$HEALTH_URL" >/dev/null 2>&1; then
    echo "[dev] server is healthy: $HEALTH_URL"
    echo "[dev] press Ctrl+C to stop"
    wait "$SERVER_PID"
    exit 0
  fi
  sleep 0.5
done

echo "[dev] server did not become healthy in time" >&2
exit 1
