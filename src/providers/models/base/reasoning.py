"""Reasoning models for OpenRouter-compatible API."""
from typing import Literal, Optional

from pydantic import BaseModel, Field, model_validator


class ReasoningConfig(BaseModel):
    """Configuration for reasoning tokens in OpenRouter-compatible format."""

    effort: Optional[Literal["low", "medium", "high"]] = Field(
        None,
        description=(
            "Reasoning effort level. 'high' allocates ~80% of max_tokens, "
            "'medium' allocates ~50%, 'low' allocates ~20%"
        ),
    )
    max_tokens: Optional[int] = Field(
        None,
        ge=1,
        description="Maximum number of tokens to use for reasoning",
    )
    exclude: bool = Field(
        False,
        description=(
            "If true, reasoning tokens will be used internally but not "
            "included in the response"
        ),
    )

    @model_validator(mode="after")
    def validate_reasoning_config(self) -> "ReasoningConfig":
        """Validate that either effort or max_tokens is provided, but not both."""
        if self.effort is not None and self.max_tokens is not None:
            raise ValueError(
                "Cannot specify both 'effort' and 'max_tokens' in reasoning config"
            )

        if self.effort is None and self.max_tokens is None:
            # Default to medium effort if nothing is specified
            self.effort = "medium"

        return self
