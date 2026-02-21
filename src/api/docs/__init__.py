"""API documentation package.

This package contains OpenAPI/Swagger documentation for API endpoints.
Documentation is organized by endpoint groups:
- chat_completion.py: Chat completion endpoints
- responses.py: Common response examples
"""

from .chat_completion import (
    CHAT_COMPLETION_DESCRIPTION,
    CHAT_COMPLETION_OPERATION_ID,
    CHAT_COMPLETION_RESPONSES,
    CHAT_COMPLETION_SUMMARY,
    CHAT_COMPLETION_TAGS,
)
from .openai_responses import (
    RESPONSES_API_RESPONSES,
    RESPONSES_DESCRIPTION,
    RESPONSES_OPERATION_ID,
    RESPONSES_SUMMARY,
    RESPONSES_TAGS,
)
from .responses import ERROR_RESPONSES

__all__ = [
    "CHAT_COMPLETION_DESCRIPTION",
    "CHAT_COMPLETION_OPERATION_ID",
    "CHAT_COMPLETION_RESPONSES",
    "CHAT_COMPLETION_SUMMARY",
    "CHAT_COMPLETION_TAGS",
    "RESPONSES_API_RESPONSES",
    "RESPONSES_DESCRIPTION",
    "RESPONSES_OPERATION_ID",
    "RESPONSES_SUMMARY",
    "RESPONSES_TAGS",
    "ERROR_RESPONSES",
]
