#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

BASE_URL="${CLOUDRU_BASE_URL:-https://foundation-models.api.cloud.ru/v1}"
API_KEY="${CLOUDRU_API_KEY:-${API_KEY:-}}"
MODEL="${CLOUDRU_MODEL:-${MODEL:-ai-sage/GigaChat-2-Max}}"
PROMPT="${CLOUDRU_PROMPT:-Reply with a single short sentence: Cloud.ru connection is working.}"
TEMPERATURE="${CLOUDRU_TEMPERATURE:-0.2}"
MAX_TOKENS="${CLOUDRU_MAX_TOKENS:-128}"
STREAM="${CLOUDRU_STREAM:-false}"
TIMEOUT_SECONDS="${CLOUDRU_TIMEOUT_SECONDS:-60}"

if [[ -z "${API_KEY}" ]]; then
  echo "CLOUDRU_API_KEY is required (or API_KEY as fallback)" >&2
  exit 1
fi

URL="${BASE_URL%/}/chat/completions"

if [[ "${STREAM}" != "true" && "${STREAM}" != "false" ]]; then
  echo "CLOUDRU_STREAM must be true or false" >&2
  exit 1
fi

payload="$(jq -n \
  --arg model "${MODEL}" \
  --arg prompt "${PROMPT}" \
  --argjson temperature "${TEMPERATURE}" \
  --argjson max_tokens "${MAX_TOKENS}" \
  --argjson stream "${STREAM}" \
  '{
    model: $model,
    messages: [
      {role: "system", content: "You are concise."},
      {role: "user", content: $prompt}
    ],
    temperature: $temperature,
    max_tokens: $max_tokens,
    stream: $stream
  }')"

echo "[cloudru][completions] POST ${URL} model=${MODEL} stream=${STREAM}" >&2

if [[ "${STREAM}" == "true" ]]; then
  curl -sS -N \
    --max-time "${TIMEOUT_SECONDS}" \
    -X POST "${URL}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${API_KEY}" \
    -d "${payload}"
  exit 0
fi

http_code="$(curl -sS -o /tmp/cloudru_chat_response.json -w "%{http_code}" \
  --max-time "${TIMEOUT_SECONDS}" \
  -X POST "${URL}" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${API_KEY}" \
  -d "${payload}")"

if [[ "${http_code}" -lt 200 || "${http_code}" -ge 300 ]]; then
  echo "request failed: http=${http_code}" >&2
  cat /tmp/cloudru_chat_response.json >&2 || true
  exit 1
fi

cat /tmp/cloudru_chat_response.json | jq .
