"""LLMGateway-specific request models."""
from typing import Dict, List, Optional

from pydantic import Field, model_validator

from providers.models.base.request import BaseRequest


class LLMGatewayRequest(BaseRequest):
    """Chat request model following LLM Gateway API spec.

    Supports OpenRouter-style reasoning configuration via the 'reasoning' field
    inherited from BaseRequest.
    """

    prompt: Optional[str] = Field(
        None, description="Text prompt (alternative to messages)"
    )
    response_format: Optional[Dict[str, str]] = Field(
        None, description="Response format configuration"
    )
    repetition_penalty: Optional[float] = Field(
        None, description="Repetition penalty, range (0, 2]"
    )
    transforms: Optional[List[str]] = Field(
        None,
        description="List of transformations to apply to the messages",
        example=["middle-out"],
        max_length=5,
    )
    frequency_penalty: Optional[float] = Field(
        None,
        ge=-2.0,
        le=2.0,
        description=(
            "Frequency penalty parameter, higher values penalize frequent tokens"
        ),
    )
    presence_penalty: Optional[float] = Field(
        None,
        ge=-2.0,
        le=2.0,
        description=(
            "Presence penalty parameter, higher values penalize tokens "
            "that appear in the text"
        ),
    )

    @model_validator(mode="after")
    def validate_transforms(self) -> "LLMGatewayRequest":
        """Validate transforms parameter."""
        if self.transforms:
            valid_transforms = ["middle-out"]

            # Check for valid values
            invalid_transforms = [
                t for t in self.transforms if t not in valid_transforms
            ]
            if invalid_transforms:
                raise ValueError(
                    f"Invalid transforms: {invalid_transforms}. "
                    f"Valid options are: {valid_transforms}"
                )

            # Check for duplicates
            if len(self.transforms) != len(set(self.transforms)):
                raise ValueError("Duplicate transforms are not allowed")

        return self
