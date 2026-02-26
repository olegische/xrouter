#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

BASE_URL="${CLOUDRU_BASE_URL:-https://foundation-models.api.cloud.ru/v1}"
API_KEY="${CLOUDRU_API_KEY:-${API_KEY:-}}"
TIMEOUT_SECONDS="${CLOUDRU_TIMEOUT_SECONDS:-30}"

if [[ -z "${API_KEY}" ]]; then
  echo "CLOUDRU_API_KEY is required (or API_KEY as fallback)" >&2
  exit 1
fi

URL="${BASE_URL%/}/models"

echo "[cloudru][models] GET ${URL}" >&2

http_code="$(curl -sS -o /tmp/cloudru_models_response.json -w "%{http_code}" \
  --max-time "${TIMEOUT_SECONDS}" \
  -H "Accept: application/json" \
  -H "Authorization: Bearer ${API_KEY}" \
  "${URL}")"

if [[ "${http_code}" -lt 200 || "${http_code}" -ge 300 ]]; then
  echo "request failed: http=${http_code}" >&2
  cat /tmp/cloudru_models_response.json >&2 || true
  exit 1
fi

cat /tmp/cloudru_models_response.json | jq .
