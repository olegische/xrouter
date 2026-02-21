"""OpenAI-compatible model response models."""
from typing import List

from pydantic import BaseModel, Field


class OpenAIModel(BaseModel):
    """OpenAI-compatible model information."""

    id: str = Field(..., description="The model identifier")
    created: int = Field(
        1710979200, description="The Unix timestamp when the model was created"
    )
    object: str = Field("model", description="The object type, which is always 'model'")
    owned_by: str = Field("xrouter", description="The organization that owns the model")


class OpenAIModelsResponse(BaseModel):
    """OpenAI-compatible models list response."""

    object: str = Field("list", description="The object type, which is always 'list'")
    data: List[OpenAIModel] = Field(..., description="The list of available models")
