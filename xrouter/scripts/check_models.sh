#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <provider|all>" >&2
  echo "providers: openrouter deepseek gigachat yandex ollama zai xrouter all" >&2
  exit 1
fi

provider="$1"
HOST="${XR_HOST:-127.0.0.1}"
PORT="${XR_PORT:-8900}"
API_KEY="${API_KEY:-}"
BASE_URL="http://${HOST}:${PORT}"

case "$provider" in
  openrouter) expected_model="openrouter/anthropic/claude-3.5-sonnet" ;;
  deepseek) expected_model="deepseek/deepseek-chat" ;;
  gigachat) expected_model="gigachat/GigaChat-2-Max" ;;
  yandex) expected_model="yandex/yandexgpt-32k" ;;
  ollama) expected_model="ollama/llama3.1:8b" ;;
  zai) expected_model="zai/glm-4.5" ;;
  xrouter) expected_model="xrouter/gpt-4.1-mini" ;;
  all) expected_model="" ;;
  *)
    echo "unsupported provider: $provider" >&2
    exit 1
    ;;
esac

fetch_models() {
  local url="$1"
  local code

  if [[ -n "$API_KEY" ]]; then
    code=$(curl -sS -o /tmp/xrouter_models_body.json -w "%{http_code}" \
      -H "Authorization: Bearer $API_KEY" \
      "$url")
  else
    code=$(curl -sS -o /tmp/xrouter_models_body.json -w "%{http_code}" "$url")
  fi

  if [[ "$code" -eq 200 ]]; then
    echo "$url"
    return 0
  fi

  return 1
}

selected_url=""
if selected_url=$(fetch_models "${BASE_URL}/api/v1/models"); then
  mode="xrouter"
elif selected_url=$(fetch_models "${BASE_URL}/v1/models"); then
  mode="openai"
else
  echo "models endpoint is unavailable on ${BASE_URL}" >&2
  exit 1
fi

body="$(cat /tmp/xrouter_models_body.json)"

if [[ "$mode" == "xrouter" ]]; then
  ids="$(echo "$body" | jq -r '.data[]?.id')"
else
  ids="$(echo "$body" | jq -r '.data[]?.id')"
fi

if [[ -z "$ids" ]]; then
  echo "models response contains no model ids" >&2
  echo "$body" >&2
  exit 1
fi

echo "[models] endpoint=${selected_url}"
echo "[models] count=$(echo "$ids" | wc -l | tr -d ' ')"

if [[ "$provider" == "all" ]]; then
  echo "[models] PASS"
  exit 0
fi

if echo "$ids" | grep -Fxq "$expected_model"; then
  echo "[models] provider=$provider found=$expected_model"
  echo "[models] PASS"
else
  echo "[models] provider=$provider missing expected model: $expected_model" >&2
  echo "[models] available ids:" >&2
  echo "$ids" >&2
  exit 1
fi
