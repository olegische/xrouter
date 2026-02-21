"""Chat completion models package."""

from .context import ChatContext
from .openai import (
    OpenAINonStreamChoice,
    OpenAIRequest,
    OpenAIResponse,
    OpenAIStreamChoice,
    OpenAIStreamChunk,
)
from .llm_gateway import (
    LLMGatewayNonStreamChoice,
    LLMGatewayRequest,
    LLMGatewayResponse,
    LLMGatewayStreamChoice,
    LLMGatewayStreamChunk,
)

__all__ = [
    "ChatContext",
    # OpenAI models
    "OpenAIRequest",
    "OpenAINonStreamChoice",
    "OpenAIResponse",
    "OpenAIStreamChoice",
    "OpenAIStreamChunk",
    # LLM Gateway models
    "LLMGatewayRequest",
    "LLMGatewayNonStreamChoice",
    "LLMGatewayResponse",
    "LLMGatewayStreamChoice",
    "LLMGatewayStreamChunk",
]
