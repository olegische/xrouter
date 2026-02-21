"""OpenAI Responses API documentation."""
from typing import Dict

from .responses import ERROR_RESPONSES

RESPONSES_SUMMARY = "Create a model response (OpenAI Responses API)"
RESPONSES_DESCRIPTION = """
Creates a response using OpenAI-compatible Responses API.

This endpoint accepts `input` in Responses format and maps it to the internal
chat-completions pipeline.

Supported modes:
- Non-streaming JSON responses
- Streaming SSE responses with Responses API event types
"""
RESPONSES_OPERATION_ID = "create_openai_response"
RESPONSES_TAGS = ["responses", "openai"]

RESPONSES_SUCCESS_EXAMPLE = {
    "id": "resp_123",
    "object": "response",
    "created_at": 1710000000,
    "status": "completed",
    "model": "openai/gpt-4.1",
    "output": [
        {
            "id": "msg_123",
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [
                {
                    "type": "output_text",
                    "text": "Hello! How can I help?",
                    "annotations": [],
                }
            ],
        }
    ],
    "usage": {
        "input_tokens": 12,
        "output_tokens": 8,
        "total_tokens": 20,
    },
    "output_text": "Hello! How can I help?",
}

RESPONSES_STREAM_EXAMPLE = (
    "event: response.output_text.delta\n"
    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n\n"
)

RESPONSES_API_RESPONSES: Dict[int, Dict] = {
    200: {
        "description": "Successful response",
        "content": {
            "application/json": {"example": RESPONSES_SUCCESS_EXAMPLE},
            "text/event-stream": {"example": RESPONSES_STREAM_EXAMPLE},
        },
    },
    **ERROR_RESPONSES,
}
