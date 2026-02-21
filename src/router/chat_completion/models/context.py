"""Context models for chat completion functionality."""
from typing import Any, Dict, Optional, Union

from pydantic import BaseModel, Field

from providers.models import (
    ProviderModel,
    ProviderRequest,
    ProviderResponse,
    Usage,
)
from .openai.request import OpenAIRequest
from .llm_gateway.request import LLMGatewayRequest
from router.usage.models import Currency, TokenCount


class ChatContext(BaseModel):
    """Chat completion context model."""

    request: Optional[Union[OpenAIRequest, LLMGatewayRequest]] = Field(
        None,
        description="Original chat request",
    )
    include_usage: bool = Field(
        False,
        description=("Flag to indicate whether to include detailed response usage"),
    )
    native_usage: Optional[Usage] = Field(
        None,
        description="Native unfiltered usage information from the provider",
    )
    api_key: str = Field(description="API key from request")
    user_id: Optional[str] = Field(
        None, description="User ID for logging purposes only"
    )
    origin: str = Field(description="Request origin")
    provider_model: Optional[ProviderModel] = Field(
        None, description="Provider model information after validation"
    )
    request_id: str = Field(description="Request identifier for tracing")
    generation_id: Optional[str] = Field(None, description="Generation ID")
    metadata: Dict[str, Any] = Field(
        default_factory=dict,
        description=(
            "Optional metadata for request context. "
            "Contains client application information: "
            "app_url - URL of the client application making the request, "
            "app_title - Name/title of the client application"
        ),
    )
    expected_tokens: Optional[TokenCount] = Field(
        None,
        description="Expected token counts for the request",
    )
    on_hold: Optional[float] = Field(
        None,
        description="Amount put on hold by billing service for current request",
    )
    currency: Optional[Currency] = Field(
        None,
        description="Requested billing currency for current request",
    )
    provider_request: Optional[ProviderRequest] = Field(
        None,
        description="Provider-specific request after transformation",
    )
    final_response: Optional[ProviderResponse] = Field(
        None,
        description="Final response from provider",
    )
    accumulated_response: Optional[str] = Field(
        None,
        description="Accumulated response from stream chunks",
    )
    cache_write: bool = Field(
        False,
        description=(
            "Flag to indicate whether cache writing is enabled "
            "(set when cache_control is present)"
        ),
    )
