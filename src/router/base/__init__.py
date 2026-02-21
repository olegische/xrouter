"""Base router package."""

from .apikey import APIKey, APIKeyStatus
from .models import (
    ModelArchitecture,
    ModelLimits,
    ModelPricing,
    ModelProvider,
    ModelResponse,
    ModelsResponse,
    OpenAIModel,
    OpenAIModelsResponse,
    ServerInfo,
    ServerLoad,
    ServerModel,
    ServerInfoResponse,
)

__all__ = [
    "APIKey",
    "APIKeyStatus",
    "ModelArchitecture",
    "ModelLimits",
    "ModelPricing",
    "ModelProvider",
    "ModelResponse",
    "ModelsResponse",
    "ServerInfo",
    "ServerLoad",
    "ServerModel",
    "ServerInfoResponse",
    # OpenAI models
    "OpenAIModel",
    "OpenAIModelsResponse",
]
