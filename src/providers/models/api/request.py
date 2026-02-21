"""Request models for provider API.

This module contains models used for making requests to providers.
"""
from pydantic import Field

from ..base.request import BaseRequest


class ProviderRequest(BaseRequest):
    """Provider-agnostic request model.

    This model captures common fields across different providers.
    Inherits reasoning support from BaseRequest.
    """

    model: str = Field(..., description="Internal model identifier")
    request_id: str = Field(..., description="Request ID for tracing and logging")
