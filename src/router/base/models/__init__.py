"""DTO models for model management endpoints."""

from .openai import OpenAIModel, OpenAIModelsResponse
from .llm_gateway import (
    ModelArchitecture,
    ModelLimits,
    ModelPricing,
    ModelProvider,
    ModelResponse,
    ModelsResponse,
    ServerInfo,
    ServerLoad,
    ServerModel,
    ServerInfoResponse,
)

__all__ = [
    # LLM Gateway models
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
