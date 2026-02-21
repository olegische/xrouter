"""Chat completion API documentation."""
from typing import Dict

from .responses import ERROR_RESPONSES
from core.settings import settings

LLM_GATEWAY_DESCRIPTION = """
Creates a completion for the chat message using LLM Gateway API.

The model parameter uses the following format:
```
<provider>/<model_id>:<version?>
```
Example: "gigachat/gigachat"

The endpoint supports both streaming and non-streaming responses:
- For streaming, set `stream: true` in the request and handle SSE responses
- For non-streaming, set `stream: false` (default) and receive a single JSON response

The endpoint also supports:
- Function calling through the `tools` parameter
- Temperature and other sampling parameters
- Maximum token limits
- Response format control
- Message transformations through the `transforms` parameter (e.g., "middle-out")
- Provider-specific fields like `native_finish_reason`

Optional Headers:
- `HTTP-Referer`: URL of the client application making the request
- `X-Title`: Name/title of the client application

These headers are used for analytics and request tracking purposes.
"""

OPENAI_DESCRIPTION = """
Creates a completion for the chat message using OpenAI-compatible API.

This endpoint follows the OpenAI API specification and is compatible with OpenAI
client libraries.

The model parameter uses the following format:
```
<provider>/<model_id>:<version?>
```
Example: "gigachat/gigachat"

The endpoint supports both streaming and non-streaming responses:
- For streaming, set `stream: true` in the request and handle SSE responses
- For non-streaming, set `stream: false` (default) and receive a single JSON response

The endpoint also supports:
- Function calling through the `tools` parameter
- Temperature and other sampling parameters
- Maximum token limits
- Response format control
- OpenAI-specific fields like `system_fingerprint`

Optional Headers:
- `HTTP-Referer`: URL of the client application making the request
- `X-Title`: Name/title of the client application

These headers are used for analytics and request tracking purposes.
"""

CHAT_COMPLETION_DESCRIPTION = (
    OPENAI_DESCRIPTION if settings.ENABLE_OPENAI_COMPATIBLE_API else LLM_GATEWAY_DESCRIPTION
)

# LLM Gateway API examples
LLM_GATEWAY_SUCCESS_EXAMPLE = {
    "id": "chatcmpl-123",
    "object": "chat.completion",
    "created": 1694268190,
    "provider": "gigachat",
    "model": "gigachat/gigachat",
    "choices": [
        {
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "I'll help you get the weather information.",
                "tool_calls": [
                    {
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": '{"location": "Paris", "unit": "celsius"}',
                        },
                    }
                ],
            },
            "finish_reason": "tool_calls",
            "native_finish_reason": None,
            "error": None,
        }
    ],
    "usage": {"prompt_tokens": 25, "completion_tokens": 32, "total_tokens": 57},
}

LLM_GATEWAY_STREAM_EXAMPLE = {
    "id": "chatcmpl-123",
    "object": "chat.completion.chunk",
    "created": 1694268190,
    "provider": "gigachat",
    "model": "gigachat/gigachat",
    "choices": [
        {
            "index": 0,
            "delta": {"role": "assistant", "content": "I'll help", "tool_calls": None},
            "finish_reason": None,
            "native_finish_reason": None,  # LLM Gateway-specific field
            "error": None,
        }
    ],
    "usage": None,
}

# OpenAI API examples
OPENAI_SUCCESS_EXAMPLE = {
    "id": "chatcmpl-123",
    "object": "chat.completion",
    "created": 1694268190,
    "model": "gigachat/gigachat",
    "choices": [
        {
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "I'll help you get the weather information.",
                "tool_calls": [
                    {
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": '{"location": "Paris", "unit": "celsius"}',
                        },
                    }
                ],
            },
            "finish_reason": "tool_calls",
        }
    ],
    "usage": {"prompt_tokens": 25, "completion_tokens": 32, "total_tokens": 57},
    "system_fingerprint": "fp_ec7eab8ec3",  # OpenAI-specific field
}

OPENAI_STREAM_EXAMPLE = {
    "id": "chatcmpl-123",
    "object": "chat.completion.chunk",
    "created": 1694268190,
    "model": "gigachat/gigachat",
    "choices": [
        {
            "index": 0,
            "delta": {"role": "assistant", "content": "I'll help", "tool_calls": None},
            "finish_reason": None,
        }
    ],
    "usage": None,
    "system_fingerprint": "fp_ec7eab8ec3",  # OpenAI-specific field
}

# Select examples based on API type
CHAT_COMPLETION_SUCCESS_EXAMPLE = (
    OPENAI_SUCCESS_EXAMPLE
    if settings.ENABLE_OPENAI_COMPATIBLE_API
    else LLM_GATEWAY_SUCCESS_EXAMPLE
)
CHAT_COMPLETION_STREAM_EXAMPLE = (
    OPENAI_STREAM_EXAMPLE
    if settings.ENABLE_OPENAI_COMPATIBLE_API
    else LLM_GATEWAY_STREAM_EXAMPLE
)

CHAT_COMPLETION_RESPONSES: Dict[int, Dict] = {
    200: {
        "description": "Successful response",
        "content": {
            "application/json": {"example": CHAT_COMPLETION_SUCCESS_EXAMPLE},
            "text/event-stream": {
                "example": f"data: {str(CHAT_COMPLETION_STREAM_EXAMPLE)}\n\n"
            },
        },
    },
    **ERROR_RESPONSES,  # Include common error responses
}

LLM_GATEWAY_TAGS = ["chat", "LLM Gateway"]
OPENAI_TAGS = ["chat", "openai"]
CHAT_COMPLETION_TAGS = (
    OPENAI_TAGS if settings.ENABLE_OPENAI_COMPATIBLE_API else LLM_GATEWAY_TAGS
)

LLM_GATEWAY_OPERATION_ID = "create_LLM_GATEWAY_chat_completion"
OPENAI_OPERATION_ID = "create_openai_chat_completion"
CHAT_COMPLETION_OPERATION_ID = (
    OPENAI_OPERATION_ID
    if settings.ENABLE_OPENAI_COMPATIBLE_API
    else LLM_GATEWAY_OPERATION_ID
)

LLM_GATEWAY_SUMMARY = "Create a chat completion using LLM Gateway API"
OPENAI_SUMMARY = "Create a chat completion using OpenAI-compatible API"
CHAT_COMPLETION_SUMMARY = (
    OPENAI_SUMMARY if settings.ENABLE_OPENAI_COMPATIBLE_API else LLM_GATEWAY_SUMMARY
)
