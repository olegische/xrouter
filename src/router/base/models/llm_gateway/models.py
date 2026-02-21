"""DTO models for model management endpoints."""
from typing import Literal, Optional

from pydantic import BaseModel, Field


class ModelArchitecture(BaseModel):
    """Model architecture information."""

    tokenizer: str = Field(..., description="Tokenizer type")
    instruct_type: str = Field(..., description="Instruction type")
    modality: str = Field(..., description="Model modality")


class ModelProvider(BaseModel):
    """Model provider information."""

    context_length: int = Field(..., description="Maximum context length")
    max_completion_tokens: int = Field(..., description="Maximum completion tokens")
    is_moderated: bool = Field(..., description="Whether content is moderated")


class ModelLimits(BaseModel):
    """Model request limits."""

    prompt_tokens: Optional[int] = Field(None, description="Maximum prompt tokens")
    completion_tokens: Optional[int] = Field(
        None, description="Maximum completion tokens"
    )


class ModelPricing(BaseModel):
    """Model pricing information."""

    prompt: str = Field(
        "0",
        description="Cost per prompt token",
        json_schema_extra={"format": "decimal"},
    )
    completion: str = Field(
        "0",
        description="Cost per completion token",
        json_schema_extra={"format": "decimal"},
    )
    request: str = Field(
        "0", description="Cost per request", json_schema_extra={"format": "decimal"}
    )
    image: str = Field(
        "0",
        description="Cost per image operation",
        json_schema_extra={"format": "decimal"},
    )
    web_search: str = Field(
        "0",
        description="Cost per web search feature",
        json_schema_extra={"format": "decimal"},
    )
    internal_reasoning: str = Field(
        "0",
        description="Cost per model reasoning",
        json_schema_extra={"format": "decimal"},
    )

    class Config:
        """Pydantic model configuration."""

        json_encoders = {
            str: lambda v: format(float(v), "f")
            if v != "0"
            else "0"  # Format without scientific notation
        }


class ModelResponse(BaseModel):
    """Response model for model information in OpenRouter format."""

    id: str = Field(..., description="Model identifier")
    name: str = Field(..., description="Model name")
    description: Optional[str] = Field(None, description="Model description")
    pricing: Optional[ModelPricing] = Field(
        None, description="Model pricing information"
    )
    context_length: int = Field(..., description="Maximum context length")
    architecture: ModelArchitecture = Field(
        ..., description="Model architecture details"
    )
    top_provider: ModelProvider = Field(..., description="Provider information")
    per_request_limits: ModelLimits = Field(..., description="Request limits")

    class Config:
        """Pydantic model configuration."""

        json_schema_extra = {
            "example": {
                "id": "gpt-3.5-turbo",
                "name": "GPT-3.5 Turbo",
                "description": "Most capable GPT-3.5 model",
                "pricing": {
                    "prompt": "0.0015",
                    "completion": "0.002",
                    "request": "0",
                    "image": "0",
                },
                "context_length": 4096,
                "architecture": {
                    "tokenizer": "Router",
                    "instruct_type": "none",
                    "modality": "text->text",
                },
                "top_provider": {
                    "context_length": 4096,
                    "max_completion_tokens": 1024,
                    "is_moderated": True,
                },
                "per_request_limits": {
                    "prompt_tokens": None,
                    "completion_tokens": None,
                },
            }
        }


class ModelsResponse(BaseModel):
    """Response model for list of models."""

    data: list[ModelResponse] = Field(..., description="List of models")


class ServerInfo(BaseModel):
    """Response model with server metadata."""

    workers_count: int = Field(..., description="Number of configured workers")
    server_version: str = Field(..., description="Server version")
    object: Literal["server"] = Field(
        "server", description="The object type, always 'server'"
    )


class ServerLoad(BaseModel):
    """Current model runtime load information."""

    queued_requests: int = Field(..., description="Current queue size")
    active_requests: int = Field(..., description="Current active request count")
    active_tokens: int = Field(..., description="Current active token count")


class ServerModel(BaseModel):
    """Model information for /api/v1/info/json endpoint."""

    id: str = Field(..., description="Model identifier")
    max_seq_len: int = Field(..., description="Maximum context length")
    max_input_len: int = Field(..., description="Maximum input token length")
    max_batch_size: int = Field(..., description="Maximum batch size")
    busy_gpu: list[int] = Field(..., description="Busy GPU indexes")
    tp: int = Field(..., description="Tensor parallelism degree")
    sampling_params: dict[str, object] = Field(..., description="Sampling parameters")
    object: Literal["model"] = Field(
        "model", description="The object type, always 'model'"
    )
    owned_by: str = Field(..., description="Model owner")
    load: ServerLoad = Field(..., description="Current model load")


class ServerInfoResponse(BaseModel):
    """Response model for server info endpoint."""

    server_info: ServerInfo = Field(..., description="Server information")
    models: list[ServerModel] = Field(..., description="Server models list")
    object: Literal["list"] = Field("list", description="The object type, always 'list'")
