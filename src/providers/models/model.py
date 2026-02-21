"""Provider model schemas."""
from typing import Dict, Optional

from pydantic import BaseModel, Field


class ProviderModel(BaseModel):
    """Provider model information."""

    model_id: str = Field(..., description="Model identifier used as external_model_id")
    name: str = Field(..., description="Human-readable model name")
    external_model_id: Optional[str] = Field(
        None, description="External model identifier in format provider/model:version"
    )
    provider_id: str = Field(..., description="Provider identifier")
    description: Optional[str] = Field(None, description="Model description")
    version: Optional[str] = Field(None, description="Model version")
    context_length: int = Field(..., description="Maximum context length")
    architecture: Dict = Field(
        ...,
        description="Model architecture details including modality, tokenizer, etc.",
    )
    capabilities: Dict = Field(
        ...,
        description="Model capabilities including context length, max tokens, features",
    )

    class Config:
        """Pydantic model configuration."""

        from_attributes = True  # Allows conversion from SQLAlchemy model
