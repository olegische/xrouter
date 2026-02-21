"""Response models for provider API.

This module contains models used for handling responses from providers.
"""
from typing import List

from pydantic import Field

from ..base import (
    FullResponse,
    NonStreamChoice,
    StreamChoice,
    StreamChunk,
)


class ProviderResponse(FullResponse):
    """Provider response model."""

    id: str = Field(..., description="Unique identifier for this response")
    provider_id: str = Field(
        ..., description="Provider identifier that generated this response"
    )
    request_id: str = Field(..., description="Request ID for tracing and logging")
    choices: List[NonStreamChoice] = Field(
        description=(
            "A list of chat completion choices. Can contain more than one element "
            "if n>1"
        )
    )


class ProviderStreamChunk(StreamChunk):
    """Provider-specific stream chunk model."""

    id: str = Field(..., description="Unique identifier for this chunk")
    provider_id: str = Field(
        ..., description="Provider identifier that generated this chunk"
    )
    request_id: str = Field(..., description="Request ID for tracing and logging")
    choices: List[StreamChoice] = Field(
        description=(
            "A list of chat completion choices. Can be empty for the last chunk "
            "if stream_options.include_usage is true"
        )
    )
