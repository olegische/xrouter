"""Response models for chat completion functionality."""
from typing import Any, Dict, List, Optional

from pydantic import Field

from providers.models import (
    FinishReason,
    FullResponse,
    NonStreamChoice,
    StreamChoice,
    StreamChunk,
)


class LLMGatewayStreamChoice(StreamChoice):
    """Choice model for streaming responses containing delta updates."""

    native_finish_reason: Optional[FinishReason] = Field(
        None,
        description="Native finish reason from provider",
    )
    error: Optional[Dict[str, Any]] = Field(
        None, description="Error information if any"
    )


class LLMGatewayNonStreamChoice(NonStreamChoice):
    """Choice model for non-streaming responses containing complete messages."""

    native_finish_reason: Optional[FinishReason] = Field(
        None,
        description="Native finish reason from provider",
    )
    error: Optional[Dict[str, Any]] = Field(
        None, description="Error information if any"
    )


class LLMGatewayResponse(FullResponse):
    """LLMGateway-compatible chat completion response."""

    choices: List[LLMGatewayNonStreamChoice] = Field(
        description=(
            "A list of chat completion choices. Can contain more than one element "
            "if n>1"
        )
    )
    id: str = Field(description="Generation ID")
    provider: str = Field(description="Provider identifier")


class LLMGatewayStreamChunk(StreamChunk):
    """LLMGateway-compatible chat completion chunk for streaming."""

    choices: List[LLMGatewayStreamChoice] = Field(
        description=(
            "A list of chat completion choices. Can be empty for the last chunk "
            "if stream_options.include_usage is true"
        )
    )
    id: str = Field(description="Generation ID")
    provider: str = Field(description="Provider identifier")
