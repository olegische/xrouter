"""Analytics-related models."""
from datetime import datetime
from typing import Dict, Optional
from uuid import UUID

from pydantic import BaseModel, Field
from .common import Currency


class Usage(BaseModel):
    """Model for tracking API usage."""

    id: UUID = Field(..., description="Usage record identifier")
    model: str = Field(..., description="Model identifier", max_length=500)
    prompt_tokens: int = Field(..., description="Number of prompt tokens")
    completion_tokens: int = Field(..., description="Number of completion tokens")
    total_cost: float = Field(
        ...,
        description="Total cost of the usage",
        json_schema_extra={"format": "decimal"},
    )
    currency: Currency = Field(default=Currency.RUB, description="Currency of the cost")
    meta_info: Optional[Dict] = Field(None, description="Additional metadata")
    created_at: datetime = Field(..., description="Creation timestamp")


class Generation(BaseModel):
    """OpenRouter-compatible generation information."""

    id: str = Field(..., description="Generation identifier")
    total_cost: float = Field(
        ...,
        description="Total cost of the usage",
        json_schema_extra={"format": "decimal"},
    )
    created_at: str = Field(..., description="Creation timestamp")
    model: str = Field(..., description="Model identifier")
    origin: str = Field(..., description="Origin of the request")
    usage: float = Field(
        ..., description="Usage cost", json_schema_extra={"format": "decimal"}
    )
    is_byok: bool = Field(..., description="Is bring your own key")
    upstream_id: Optional[str] = Field(None, description="Upstream identifier")
    cache_discount: Optional[float] = Field(None, description="Cache discount")
    app_id: Optional[int] = Field(None, description="Application identifier")
    streamed: Optional[bool] = Field(None, description="Whether response was streamed")
    cancelled: Optional[bool] = Field(
        None, description="Whether generation was cancelled"
    )
    provider_name: Optional[str] = Field(None, description="Provider name")
    latency: Optional[int] = Field(None, description="Latency in milliseconds")
    moderation_latency: Optional[int] = Field(
        None, description="Moderation latency in milliseconds"
    )
    generation_time: Optional[int] = Field(
        None, description="Generation time in milliseconds"
    )
    finish_reason: Optional[str] = Field(
        None, description="Reason for generation completion"
    )
    native_finish_reason: Optional[str] = Field(
        None, description="Native finish reason from provider"
    )
    tokens_prompt: Optional[int] = Field(None, description="Prompt tokens count")
    tokens_completion: Optional[int] = Field(
        None, description="Completion tokens count"
    )
    native_tokens_prompt: Optional[int] = Field(
        None, description="Native prompt tokens"
    )
    native_tokens_completion: Optional[int] = Field(
        None, description="Native completion tokens"
    )
    native_tokens_reasoning: Optional[int] = Field(
        None, description="Native reasoning tokens"
    )
    num_media_prompt: Optional[int] = Field(
        None, description="Number of media items in prompt"
    )
    num_media_completion: Optional[int] = Field(
        None, description="Number of media items in completion"
    )
    num_search_results: Optional[int] = Field(
        None, description="Number of search results"
    )
