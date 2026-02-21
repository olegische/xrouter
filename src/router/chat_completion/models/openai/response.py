"""OpenAI-compatible response models."""
from typing import List

from pydantic import Field

from providers.models.base.response import (
    FullResponse,
    NonStreamChoice,
    StreamChoice,
    StreamChunk,
)


class OpenAIStreamChoice(StreamChoice):
    """OpenAI-compatible streaming choice model."""

    pass


class OpenAINonStreamChoice(NonStreamChoice):
    """OpenAI-compatible non-streaming choice model."""

    pass


class OpenAIResponse(FullResponse):
    """OpenAI-compatible chat completion response."""

    id: str = Field(..., description="Unique identifier for this completion")
    choices: List[OpenAINonStreamChoice] = Field(
        ...,
        description="The list of generated completions",
    )


class OpenAIStreamChunk(StreamChunk):
    """OpenAI-compatible streaming chat completion chunk."""

    id: str = Field(..., description="Unique identifier for this completion")
    choices: List[OpenAIStreamChoice] = Field(
        ...,
        description="The list of token choices for this chunk",
    )
