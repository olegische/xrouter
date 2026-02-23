#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <provider> [mode]" >&2
  echo "mode: non-stream (default) | stream" >&2
  echo "providers: openrouter deepseek deepseek-reasoner gigachat yandex ollama zai xrouter" >&2
  exit 1
fi

provider="$1"
mode="${2:-non-stream}"
if [[ "$mode" != "non-stream" && "$mode" != "stream" ]]; then
  echo "unsupported mode: $mode (use non-stream|stream)" >&2
  exit 1
fi
HOST="${XR_HOST:-127.0.0.1}"
PORT="${XR_PORT:-8900}"
API_KEY="${API_KEY:-}"
BASE_URL="http://${HOST}:${PORT}"

case "$provider" in
  openrouter) model="openrouter/anthropic/claude-3.5-sonnet" ;;
  deepseek) model="deepseek/deepseek-chat" ;;
  deepseek-reasoner) model="deepseek/deepseek-reasoner" ;;
  gigachat) model="gigachat/GigaChat-2-Max" ;;
  yandex) model="yandex/yandexgpt/rc" ;;
  ollama) model="ollama/llama3.1:8b" ;;
  zai) model="zai/glm-4.5" ;;
  xrouter) model="xrouter/gpt-4.1-mini" ;;
  *)
    echo "unsupported provider: $provider" >&2
    exit 1
    ;;
esac

if [[ -n "${XR_MODEL:-}" ]]; then
  model="$XR_MODEL"
fi

CHAT_URL="${BASE_URL}/api/v1/chat/completions"
RESPONSES_URL="${BASE_URL}/api/v1/responses"

curl_json() {
  local url="$1"
  local payload="$2"
  local http_code

  if [[ -n "$API_KEY" ]]; then
    http_code=$(curl -sS -o /tmp/xrouter_smoke_body.json -w "%{http_code}" \
      -X POST "$url" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $API_KEY" \
      -d "$payload")
  else
    http_code=$(curl -sS -o /tmp/xrouter_smoke_body.json -w "%{http_code}" \
      -X POST "$url" \
      -H "Content-Type: application/json" \
      -d "$payload")
  fi

  if [[ "$http_code" -lt 200 || "$http_code" -ge 300 ]]; then
    echo "request failed: $url http=$http_code" >&2
    cat /tmp/xrouter_smoke_body.json >&2 || true
    return 1
  fi

  cat /tmp/xrouter_smoke_body.json
}

curl_stream() {
  local url="$1"
  local payload="$2"
  if [[ -n "$API_KEY" ]]; then
    curl -sS -N \
      -X POST "$url" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $API_KEY" \
      -d "$payload"
  else
    curl -sS -N \
      -X POST "$url" \
      -H "Content-Type: application/json" \
      -d "$payload"
  fi
}

stream_flag=false
if [[ "$mode" == "stream" ]]; then
  stream_flag=true
fi

echo "[smoke][$provider][$mode] model=$model host=$HOST port=$PORT"

chat_payload=$(jq -n --arg model "$model" '{
  model: $model,
  messages: [
    {role: "system", content: "You are concise."},
    {role: "user", content: "Reply with ok"}
  ],
  stream: $stream
}' --argjson stream "$stream_flag")

if [[ "$mode" == "stream" ]]; then
  chat_stream="$(curl_stream "$CHAT_URL" "$chat_payload")"
  if ! grep -q 'chat.completion.chunk' <<<"$chat_stream"; then
    echo "chat stream missing chunk object" >&2
    echo "$chat_stream" >&2
    exit 1
  fi
  if ! grep -q '\[DONE\]' <<<"$chat_stream"; then
    echo "chat stream missing [DONE] marker" >&2
    echo "$chat_stream" >&2
    exit 1
  fi
else
  chat_resp="$(curl_json "$CHAT_URL" "$chat_payload")"
  chat_content="$(echo "$chat_resp" | jq -r '.choices[0].message.content // empty')"
  if [[ -z "$chat_content" ]]; then
    echo "chat completion missing choices[0].message.content" >&2
    echo "$chat_resp" >&2
    exit 1
  fi
fi

echo "[smoke][$provider][$mode] chat completions ok"

responses_payload=$(jq -n --arg model "$model" '{
  model: $model,
  input: "Reply with ok",
  stream: $stream
}' --argjson stream "$stream_flag")

if [[ "$mode" == "stream" ]]; then
  responses_stream="$(curl_stream "$RESPONSES_URL" "$responses_payload")"
  if ! grep -q 'response.created' <<<"$responses_stream"; then
    echo "responses stream missing response.created" >&2
    echo "$responses_stream" >&2
    exit 1
  fi
  if ! grep -q 'response.completed' <<<"$responses_stream"; then
    echo "responses stream missing response.completed" >&2
    echo "$responses_stream" >&2
    exit 1
  fi
else
  responses_resp="$(curl_json "$RESPONSES_URL" "$responses_payload")"
  responses_text="$(echo "$responses_resp" | jq -r '.output[]? | select(.type=="message") | .content[0].text // empty' | head -n1)"
  if [[ -z "$responses_text" ]]; then
    echo "responses missing output message text" >&2
    echo "$responses_resp" >&2
    exit 1
  fi
fi

echo "[smoke][$provider][$mode] responses ok"

# Tool-like scenario smoke: current Rust contract does not support native function-calling schema.
# We still validate a 2-step exchange with tool-style roles/content is accepted and returns a response.
chat_tool_step1=$(jq -n --arg model "$model" '{
  model: $model,
  messages: [
    {role: "user", content: "Need weather. If tool needed, answer with TOOL_CALL:get_weather:{\"location\":\"New York\"}"}
  ],
  stream: $stream
}' --argjson stream "$stream_flag")

if [[ "$mode" == "stream" ]]; then
  _="$(curl_stream "$CHAT_URL" "$chat_tool_step1")"
else
  _="$(curl_json "$CHAT_URL" "$chat_tool_step1")"
fi

chat_tool_step2=$(jq -n --arg model "$model" '{
  model: $model,
  messages: [
    {role: "user", content: "Need weather in New York"},
    {role: "assistant", content: "TOOL_CALL:get_weather:{\"location\":\"New York\"}"},
    {role: "tool", content: "{\"location\":\"New York\",\"temperature_c\":5,\"condition\":\"Cloudy\"}"},
    {role: "user", content: "Now summarize in one short sentence."}
  ],
  stream: $stream
}' --argjson stream "$stream_flag")

if [[ "$mode" == "stream" ]]; then
  tool_stream="$(curl_stream "$CHAT_URL" "$chat_tool_step2")"
  if ! grep -q 'chat.completion.chunk' <<<"$tool_stream"; then
    echo "tool-like chat stream missing chunk object" >&2
    echo "$tool_stream" >&2
    exit 1
  fi
  if ! grep -q '\[DONE\]' <<<"$tool_stream"; then
    echo "tool-like chat stream missing [DONE] marker" >&2
    echo "$tool_stream" >&2
    exit 1
  fi
else
  tool_resp="$(curl_json "$CHAT_URL" "$chat_tool_step2")"
  tool_content="$(echo "$tool_resp" | jq -r '.choices[0].message.content // empty')"
  if [[ -z "$tool_content" ]]; then
    echo "tool-like chat flow missing final assistant content" >&2
    echo "$tool_resp" >&2
    exit 1
  fi
fi

echo "[smoke][$provider][$mode] tool-like chat flow ok"
echo "[smoke][$provider][$mode] PASS"
