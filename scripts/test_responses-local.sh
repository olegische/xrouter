#!/bin/bash

# Simple test script for OpenAI Responses API endpoint
# Requires ENABLE_OPENAI_COMPATIBLE_API=true

set -e

# Configuration
PORT=${PORT:-8900}
API_KEY=${API_KEY:-""}
MODEL=${MODEL:-"gigachat/gigachat-2-max"}

# OpenAI Responses API endpoint
ENDPOINT="http://localhost:${PORT}/api/v1/responses"
echo "Testing OpenAI Responses API at: $ENDPOINT"

# Simple test with stream=false
echo "Making simple request with stream=false..."

REQUEST_BODY='{
  "model": "'"$MODEL"'",
  "input": "Hello, how are you?",
  "stream": false,
  "temperature": 0.7,
  "max_output_tokens": 100
}'

# Build curl command
CURL_CMD="curl -s -X POST $ENDPOINT"
CURL_CMD="$CURL_CMD -H 'Content-Type: application/json'"

# Add API key if provided
if [ -n "$API_KEY" ]; then
    CURL_CMD="$CURL_CMD -H 'Authorization: Bearer $API_KEY'"
fi

CURL_CMD="$CURL_CMD -d '$REQUEST_BODY'"

# Execute request and show response
echo "Request:"
echo "$REQUEST_BODY" | jq '.' 2>/dev/null || echo "$REQUEST_BODY"
echo ""

echo "Response:"
RESPONSE_STEP1=$(eval $CURL_CMD)
echo "$RESPONSE_STEP1" | jq '.' 2>/dev/null || echo "$RESPONSE_STEP1"

echo ""
echo "---"
echo ""

# Tool calling test
echo "Making request with tool calling..."

TOOLS_REQUEST_BODY='{
  "model": "'"$MODEL"'",
  "input": [
    {
      "role": "user",
      "content": "What is the weather in New York?"
    }
  ],
  "stream": false,
  "temperature": 0.2,
  "max_output_tokens": 200,
  "tools": [
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
  ],
  "tool_choice": "auto"
}'

# Build curl command for tools request
CURL_CMD_TOOLS="curl -s -X POST $ENDPOINT"
CURL_CMD_TOOLS="$CURL_CMD_TOOLS -H 'Content-Type: application/json'"

if [ -n "$API_KEY" ]; then
    CURL_CMD_TOOLS="$CURL_CMD_TOOLS -H 'Authorization: Bearer $API_KEY'"
fi

CURL_CMD_TOOLS="$CURL_CMD_TOOLS -d '$TOOLS_REQUEST_BODY'"

echo "Request (step 1):"
echo "$TOOLS_REQUEST_BODY" | jq '.' 2>/dev/null || echo "$TOOLS_REQUEST_BODY"
echo ""

echo "Response (step 1):"
RESPONSE_TOOLS=$(eval $CURL_CMD_TOOLS)
echo "$RESPONSE_TOOLS" | jq '.' 2>/dev/null || echo "$RESPONSE_TOOLS"

# Extract tool call details from output array
TOOL_CALL_ID=$(echo "$RESPONSE_TOOLS" | jq -r '.output[]? | select(.type == "function_call") | .call_id // empty')
TOOL_NAME=$(echo "$RESPONSE_TOOLS" | jq -r '.output[]? | select(.type == "function_call") | .name // empty')
TOOL_ARGS=$(echo "$RESPONSE_TOOLS" | jq -r '.output[]? | select(.type == "function_call") | .arguments // empty')

if [ -z "$TOOL_CALL_ID" ] || [ -z "$TOOL_NAME" ]; then
    echo ""
    echo "No tool call found in response. Skipping step 2."
else
    echo ""
    echo "Step 2: send tool result..."
    
    # Simulate tool execution result
    TOOL_RESULT='{"location":"New York, NY","temperature_c":5,"condition":"Cloudy","wind_kph":18}'
    
    # Build request using jq to properly handle JSON escaping
    TOOL_RESULT_REQUEST=$(jq -n \
      --arg model "$MODEL" \
      --arg call_id "$TOOL_CALL_ID" \
      --arg name "$TOOL_NAME" \
      --arg arguments "$TOOL_ARGS" \
      --arg output "$TOOL_RESULT" \
      '{
        model: $model,
        input: [
          {
            role: "user",
            content: "What is the weather in New York?"
          },
          {
            type: "function_call",
            call_id: $call_id,
            name: $name,
            arguments: $arguments
          },
          {
            type: "function_call_output",
            call_id: $call_id,
            output: $output
          }
        ],
        stream: false,
        temperature: 0.2,
        max_output_tokens: 200
      }')
    
    echo "Request (step 2):"
    echo "$TOOL_RESULT_REQUEST" | jq '.' 2>/dev/null || echo "$TOOL_RESULT_REQUEST"
    echo ""
    
    echo "Response (step 2):"
    if [ -n "$API_KEY" ]; then
        curl -s -X POST "$ENDPOINT" \
          -H "Content-Type: application/json" \
          -H "Authorization: Bearer $API_KEY" \
          -d "$TOOL_RESULT_REQUEST" | jq '.' 2>/dev/null || true
    else
        curl -s -X POST "$ENDPOINT" \
          -H "Content-Type: application/json" \
          -d "$TOOL_RESULT_REQUEST" | jq '.' 2>/dev/null || true
    fi
fi

echo ""
echo "Test completed."
