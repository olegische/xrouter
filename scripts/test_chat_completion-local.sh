#!/bin/bash

# Simple test script for chat completion endpoint
# Tests LLM Gateway API format only (ENABLE_OPENAI_COMPATIBLE_API=false)

set -e

# Configuration
PORT=${PORT:-8900}
API_KEY=${API_KEY:-""}
# MODEL=${MODEL:-"xrouter/openrouter-proxy/google/gemini-2.5-flash"}
MODEL=${MODEL:-"zai/glm-5"}

# LLM Gateway endpoint only
ENDPOINT="http://localhost:${PORT}/api/v1/chat/completions"
echo "Testing LLM Gateway API at: $ENDPOINT"

# Tool call simulation (two-step)
USER_MSG="Какая погода в Нью-Йорке?"
TOOL_RESULT='{"location":"New York, NY","temperature_c":5,"condition":"Cloudy","wind_kph":18}'

TOOLS_JSON='[
  {
    "type": "function",
    "function": {
      "name": "get_weather",
      "description": "Get current weather for a location.",
      "parameters": {
        "type": "object",
        "properties": {
          "location": {
            "type": "string",
            "description": "City and country, e.g. New York, US"
          },
          "unit": {
            "type": "string",
            "enum": ["celsius", "fahrenheit"]
          }
        },
        "required": ["location"]
      }
    }
  }
]'

# Build curl command
CURL_CMD="curl -s -X POST $ENDPOINT"
CURL_CMD="$CURL_CMD -H 'Content-Type: application/json'"

# Add API key if provided
if [ -n "$API_KEY" ]; then
    CURL_CMD="$CURL_CMD -H 'Authorization: Bearer $API_KEY'"
fi

# Step 1: request with tools
echo "Step 1: request tools for tool call..."
REQUEST_BODY_STEP1=$(jq -n \
  --arg model "$MODEL" \
  --arg user_msg "$USER_MSG" \
  --argjson tools "$TOOLS_JSON" \
  '{
    model: $model,
    messages: [
      {role: "user", content: $user_msg}
    ],
    tools: $tools,
    tool_choice: "auto",
    stream: false,
    temperature: 0.2,
    max_tokens: 150
  }')

CURL_CMD_STEP1="$CURL_CMD -d '$REQUEST_BODY_STEP1'"
echo "Request (step 1):"
echo "$REQUEST_BODY_STEP1" | jq '.' 2>/dev/null || echo "$REQUEST_BODY_STEP1"
echo ""

echo "Response (step 1):"
RESPONSE_STEP1=$(eval $CURL_CMD_STEP1)
echo "$RESPONSE_STEP1" | jq '.' 2>/dev/null || echo "$RESPONSE_STEP1"
echo ""

TOOL_CALL_ID=$(echo "$RESPONSE_STEP1" | jq -r '.choices[0].message.tool_calls[0].id // empty')
TOOL_CALL_NAME=$(echo "$RESPONSE_STEP1" | jq -r '.choices[0].message.tool_calls[0].function.name // empty')
TOOL_CALL_ARGS=$(echo "$RESPONSE_STEP1" | jq -r '.choices[0].message.tool_calls[0].function.arguments // empty')

if [ -z "$TOOL_CALL_ID" ]; then
  TOOL_CALL_ID="call_1"
  TOOL_CALL_NAME="get_weather"
  TOOL_CALL_ARGS='{"location":"New York, NY","unit":"celsius"}'
  echo "No tool_call returned; using simulated tool call."
  echo ""
fi

# Step 2: send tool result
echo "Step 2: send tool result..."
REQUEST_BODY_STEP2=$(jq -n \
  --arg model "$MODEL" \
  --arg user_msg "$USER_MSG" \
  --arg tool_id "$TOOL_CALL_ID" \
  --arg tool_name "$TOOL_CALL_NAME" \
  --arg tool_args "$TOOL_CALL_ARGS" \
  --arg tool_result "$TOOL_RESULT" \
  '{
    model: $model,
    messages: [
      {role: "user", content: $user_msg},
      {
        role: "assistant",
        tool_calls: [
          {
            id: $tool_id,
            type: "function",
            function: {name: $tool_name, arguments: $tool_args}
          }
        ]
      },
      {role: "tool", name: $tool_name, tool_call_id: $tool_id, content: $tool_result}
    ],
    stream: false,
    temperature: 0.2,
    max_tokens: 200
  }')

CURL_CMD_STEP2="$CURL_CMD -d '$REQUEST_BODY_STEP2'"
echo "Request (step 2):"
echo "$REQUEST_BODY_STEP2" | jq '.' 2>/dev/null || echo "$REQUEST_BODY_STEP2"
echo ""

echo "Response (step 2):"
eval $CURL_CMD_STEP2 | jq '.' 2>/dev/null || eval $CURL_CMD_STEP2

echo ""
echo "Test completed."