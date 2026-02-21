"""OpenAI-compatible request models."""
from typing import Any, Dict, List, Literal, Optional

from pydantic import Field, model_validator

from providers.models.base.messages import Message, OpenAIMessageType
from providers.models.base.request import BaseRequest


class OpenAIRequest(BaseRequest):
    """OpenAI-compatible chat completion request model."""

    messages: List[OpenAIMessageType] = Field(
        ...,
        description="A list of messages comprising the conversation so far",
    )

    n: Optional[int] = Field(
        1,
        ge=1,
        le=4,
        description="Number of chat completion choices to generate",
    )
    max_completion_tokens: Optional[int] = Field(
        None,
        description="An upper bound for the number of tokens that can be generated",
    )
    presence_penalty: Optional[float] = Field(
        0.0,
        ge=-2.0,
        le=2.0,
        description=(
            "Number between -2.0 and 2.0. Positive values penalize new tokens "
            "based on whether they appear in the text so far"
        ),
    )
    frequency_penalty: Optional[float] = Field(
        0.0,
        ge=-2.0,
        le=2.0,
        description=(
            "Number between -2.0 and 2.0. Positive values penalize new tokens "
            "based on their existing frequency in the text so far"
        ),
    )
    logit_bias: Optional[Dict[str, float]] = Field(
        None,
        description=(
            "Modify the likelihood of specified tokens appearing in the completion"
        ),
    )
    user: Optional[str] = Field(
        None,
        description="A unique identifier representing your end-user",
    )
    reasoning_effort: Optional[Literal["low", "medium", "high"]] = Field(
        None,
        description=(
            "Constrains effort on reasoning for reasoning models. "
            "Currently supported values are low, medium, and high. "
            "Reducing reasoning effort can result in faster responses "
            "and fewer tokens used on reasoning in a response. "
            "Only supported by o-series models."
        ),
    )

    @model_validator(mode="before")
    @classmethod
    def validate_messages(cls, data: Any) -> Any:
        """Convert messages to OpenAI format (without cache_control)."""
        if (
            isinstance(data, dict)
            and "messages" in data
            and isinstance(data["messages"], list)
        ):
            # Convert each message to OpenAI format
            data["messages"] = [
                Message.model_validate(msg, use_openai=True) for msg in data["messages"]
            ]
        return data
