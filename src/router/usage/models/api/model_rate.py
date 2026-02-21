"""Model rate API models."""
from typing import Optional

from pydantic import BaseModel, Field

from ..common import Currency


class ModelRateResponse(BaseModel):
    """Response model for model rate operations."""

    model: str = Field(
        ..., description="Full model identifier (e.g. 'anthropic/claude-3-sonnet:beta')"
    )
    prompt_rate: float = Field(
        ...,
        description="Cost per prompt token",
        json_schema_extra={"format": "decimal"},
    )
    completion_rate: float = Field(
        ...,
        description="Cost per completion token",
        json_schema_extra={"format": "decimal"},
    )
    reasoning_rate: Optional[float] = Field(
        None,
        description="Cost per reasoning token",
        json_schema_extra={"format": "decimal"},
    )
    image_rate: Optional[float] = Field(
        None,
        description="Cost per image operation",
        json_schema_extra={"format": "decimal"},
    )
    currency: Currency = Field(..., description="Rate currency")
    created_at: int = Field(
        ..., description="When this rate was created (unix timestamp)"
    )

    class Config:
        """Pydantic model configuration."""

        from_attributes = True
        json_encoders = {
            float: lambda v: format(v, "f")  # Format float without scientific notation
        }
