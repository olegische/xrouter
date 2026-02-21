"""Analytics API models."""
from typing import Dict, Optional

from pydantic import BaseModel, Field

from ..analytics import Generation, Usage
from ..billing import Cost, TokenCount

# Server models (for API validation)


class ServerCreateUsageRequest(BaseModel):
    """Request model for creating a usage record."""

    tokens: TokenCount = Field(..., description="Token counts for the usage")
    cost: Cost = Field(..., description="Cost information including breakdown")
    meta_info: Optional[Dict] = Field(None, description="Additional metadata")


class CreateUsageResponse(BaseModel):
    """Response model for usage endpoint."""

    data: Usage = Field(..., description="Usage data")


class ServerCreateGenerationRequest(BaseModel):
    """Request model for creating a generation record."""

    id: str = Field(..., description="Generation identifier")
    model: str = Field(..., description="Model identifier", max_length=500)
    provider: str = Field(..., description="Provider identifier", max_length=100)
    origin: Optional[str] = Field(
        None, description="Origin of the request", max_length=100
    )
    generation_time: float = Field(
        ...,
        description="Time taken for generation in seconds",
        json_schema_extra={"format": "decimal"},
    )
    speed: float = Field(
        ..., description="Tokens per second", json_schema_extra={"format": "decimal"}
    )
    finish_reason: str = Field(
        ..., description="Reason for generation completion", max_length=100
    )
    native_finish_reason: str = Field(
        ..., description="Native finish reason from provider", max_length=100
    )
    error: Optional[str] = Field(
        None, description="Error message if any", max_length=500
    )
    is_streaming: bool = Field(
        default=False, description="Whether the generation was streamed"
    )
    meta_info: Optional[Dict] = Field(None, description="Additional metadata")
    usage_id: str = Field(..., description="Associated usage record ID")


class CreateGenerationResponse(BaseModel):
    """Response model for generation endpoint."""

    data: Generation = Field(..., description="Generation data")


# Client models (with api_key)


class CreateUsageRequest(ServerCreateUsageRequest):
    """Client request model for creating a usage record."""

    api_key: str = Field(..., description="API key for authentication")


class CreateGenerationRequest(ServerCreateGenerationRequest):
    """Client request model for creating a generation record."""

    api_key: str = Field(..., description="API key for authentication")
