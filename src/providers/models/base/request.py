"""Base request models."""
from typing import List, Optional, Union

from pydantic import BaseModel, Field

from .messages import MessageType
from .reasoning import ReasoningConfig
from .tools import Tool, ToolChoice


class UsageRequest(BaseModel):
    """Usage accounting settings for OpenRouter API."""

    include: bool = Field(
        False,
        description="Whether to include usage information in the response",
    )


class BaseRequest(BaseModel):
    """Base request model with common fields for all request types."""

    model: str = Field(..., description="ID of the model to use")
    usage: Optional[UsageRequest] = Field(
        None,
        description=(
            "Usage accounting settings for tracking token consumption and costs"
        ),
    )
    messages: List[MessageType] = Field(
        ...,
        description="A list of messages comprising the conversation so far",
    )
    temperature: Optional[float] = Field(
        1.0,
        ge=0.0,
        le=2.0,
        description="Sampling temperature, higher values make output more random",
    )
    top_p: Optional[float] = Field(
        1.0,
        ge=0.0,
        le=1.0,
        description=(
            "Nucleus sampling parameter, sets probability mass of tokens to consider"
        ),
    )
    stream: bool = Field(
        False,
        description="If true, partial message deltas will be sent",
    )
    stop: Optional[Union[str, List[str]]] = Field(
        None,
        description="Sequences where the API will stop generating",
    )
    max_tokens: Optional[int] = Field(
        None,
        description="The maximum number of tokens to generate",
    )
    tools: Optional[List[Tool]] = Field(
        None,
        description=(
            "A list of tools the model may call. "
            "Currently, only functions are supported. "
            "Use this to provide a list of functions the model may generate "
            "JSON inputs for. A max of 128 functions are supported"
        ),
        max_length=128,
    )
    tool_choice: Optional[ToolChoice] = Field(
        None,
        description=(
            "Controls which tool is called by the model. 'none' means no tool calls, "
            "'auto' allows model to choose, 'required' forces tool calls, "
            "or specify a particular function"
        ),
    )
    reasoning: Optional[ReasoningConfig] = Field(
        None,
        description=(
            "Configuration for reasoning tokens. Supports OpenRouter-style "
            "reasoning with effort levels or max_tokens specification"
        ),
    )
