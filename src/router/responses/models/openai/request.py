"""OpenAI Responses API request models."""
from typing import Any, List, Literal, Optional, Union

from pydantic import BaseModel, ConfigDict, Field, field_validator

from providers.models.base.tools import Tool, ToolChoice


class ResponsesReasoningConfig(BaseModel):
    """Reasoning settings in Responses API format."""

    effort: Optional[Literal["low", "medium", "high"]] = Field(
        None,
        description="Reasoning effort for o-series models",
    )


class ResponsesInputTextPart(BaseModel):
    """Text input part for Responses API input messages."""

    type: Literal["input_text", "text", "output_text"] = Field(
        "input_text",
        description="Input content part type",
    )
    text: str = Field(..., description="Text content")


class ResponsesInputMessage(BaseModel):
    """Input message in Responses API format."""

    role: Literal["user", "assistant", "system", "developer"] = Field(
        ..., description="Message role"
    )
    content: Union[str, List[ResponsesInputTextPart]] = Field(
        ..., description="Message content"
    )


class ResponsesInputFunctionCall(BaseModel):
    """Function call item in Responses API input."""

    type: Literal["function_call"] = Field("function_call")
    call_id: str = Field(..., description="Tool call ID")
    name: str = Field(..., description="Function name")
    arguments: str = Field(..., description="Function arguments JSON")


class ResponsesInputFunctionCallOutput(BaseModel):
    """Function call output item in Responses API input."""

    type: Literal["function_call_output"] = Field("function_call_output")
    call_id: str = Field(..., description="Tool call ID")
    output: str = Field(..., description="Tool output")


class OpenAIResponsesRequest(BaseModel):
    """OpenAI-compatible Responses API request model."""

    model_config = ConfigDict(extra="allow")

    model: str = Field(..., description="Model identifier")
    input: Union[
        str,
        ResponsesInputMessage,
        ResponsesInputFunctionCall,
        ResponsesInputFunctionCallOutput,
        List[
            Union[
                ResponsesInputMessage,
                ResponsesInputFunctionCall,
                ResponsesInputFunctionCallOutput,
            ]
        ],
    ] = Field(..., description="Input text or input items")
    instructions: Optional[str] = Field(
        None,
        description="High-level instructions prepended before input",
    )
    stream: bool = Field(False, description="If true, stream response events")
    temperature: Optional[float] = Field(
        None,
        ge=0.0,
        le=2.0,
        description="Sampling temperature",
    )
    top_p: Optional[float] = Field(
        None,
        ge=0.0,
        le=1.0,
        description="Top-p sampling parameter",
    )
    max_output_tokens: Optional[int] = Field(
        None,
        ge=1,
        description="Maximum output tokens",
    )
    tools: Optional[List[Tool]] = Field(None, description="Tools available to the model")
    tool_choice: Optional[ToolChoice] = Field(
        None,
        description="Tool choice strategy",
    )
    reasoning: Optional[ResponsesReasoningConfig] = Field(
        None,
        description="Reasoning configuration",
    )

    @field_validator("tools", mode="before")
    @classmethod
    def normalize_tools(cls, value: Any) -> Any:
        if value is None:
            return value
        if isinstance(value, dict):
            value = [value]
        if not isinstance(value, list):
            return value
        normalized: List[Any] = []
        for item in value:
            if not isinstance(item, dict):
                normalized.append(item)
                continue
            if "function" in item:
                normalized.append(item)
                continue
            if item.get("type") == "function":
                normalized.append(
                    {
                        "type": "function",
                        "function": {
                            "name": item.get("name"),
                            "description": item.get("description"),
                            "parameters": item.get("parameters"),
                        },
                    }
                )
        if not normalized:
            return None
        return normalized
