"""Yandex provider API models."""
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field, model_validator


class YandexObjectType(str, Enum):
    """Yandex object types."""

    CHAT_COMPLETION = Field("chat.completion", description="Complete chat response")
    CHAT_COMPLETION_CHUNK = Field(
        "chat.completion.chunk", description="Streaming chat response chunk"
    )


class YandexFunctionCall(BaseModel):
    """Function call specification."""

    name: str = Field(description="The name of the function being called")
    arguments: Dict[str, Any] = Field(
        description="The structured arguments passed to the function"
    )


class YandexFunctionResult(BaseModel):
    """Function result specification."""

    name: str = Field(description="The name of the function that was executed")
    content: str = Field(description="The result of the function call")


class YandexToolCall(BaseModel):
    """Tool call specification."""

    functionCall: YandexFunctionCall = Field(description="Function call details")


class YandexToolResult(BaseModel):
    """Tool result specification."""

    functionResult: YandexFunctionResult = Field(description="Function result details")


class YandexToolCallList(BaseModel):
    """List of tool calls."""

    toolCalls: List[YandexToolCall] = Field(
        description="List of tool calls to be executed"
    )


class YandexToolResultList(BaseModel):
    """List of tool results."""

    toolResults: List[YandexToolResult] = Field(description="List of tool results")


class YandexFunction(BaseModel):
    """Function definition."""

    name: str = Field(
        pattern=r"^[a-zA-Z0-9_-]+$",
        max_length=64,
        description=(
            "The name of the function to be called. Must be a-z, A-Z, 0-9, "
            "or contain underscores and dashes, with a maximum length of 64"
        ),
    )
    description: str = Field(
        description=(
            "A description of what the function does, used by the model to choose "
            "when and how to call the function"
        )
    )
    parameters: Dict[str, Any] = Field(
        description=(
            "The parameters the functions accepts, described as a JSON Schema object"
        )
    )
    strict: Optional[bool] = Field(
        None,
        description="Enforces strict adherence to the function schema",
    )


class YandexTool(BaseModel):
    """Tool definition."""

    function: YandexFunction = Field(description="Function that can be called")


class YandexMessage(BaseModel):
    """Yandex message model."""

    role: Literal["system", "user", "assistant"] = Field(
        description=(
            "Message author role: system prompt, user message, " "or assistant response"
        )
    )
    text: Optional[str] = Field(None, description="Message content")
    toolCallList: Optional[YandexToolCallList] = Field(
        None, description="List of tool calls made by the model"
    )
    toolResultList: Optional[YandexToolResultList] = Field(
        None, description="List of tool results from external tools"
    )

    @model_validator(mode="after")
    def validate_content_fields(self) -> "YandexMessage":
        """Validate that only one content field is set."""
        content_fields = [
            field
            for field in ["text", "toolCallList", "toolResultList"]
            if getattr(self, field) is not None
        ]
        if len(content_fields) != 1:
            raise ValueError(
                "Exactly one of text, toolCallList, or toolResultList must be set"
            )
        return self


class YandexCompletionOptions(BaseModel):
    """Yandex completion options."""

    stream: bool = Field(
        default=True, description="Stream messages in parts using SSE protocol"
    )
    temperature: Optional[float] = Field(
        default=0.3,
        ge=0.0,
        le=1.0,
        description=(
            "Affects creativity and randomness of responses. "
            "Lower values produce more straightforward responses"
        ),
    )
    maxTokens: Optional[int] = Field(
        default=None,
        ge=0,
        description="Maximum number of tokens to use for response generation",
    )
    reasoningOptions: Optional["YandexReasoningOptions"] = Field(
        None,
        description="Options to configure model reasoning behavior",
    )


class YandexReasoningOptions(BaseModel):
    """Yandex reasoning options."""

    mode: Literal["DISABLED", "ENABLED_HIDDEN"] = Field(
        description="Reasoning mode for completion generation"
    )


class YandexJsonSchema(BaseModel):
    """Yandex JSON schema response format."""

    schema: Dict[str, Any] = Field(description="JSON Schema for model response")


class YandexToolChoice(BaseModel):
    """Yandex tool choice options."""

    mode: Optional[Literal["NONE", "AUTO", "REQUIRED"]] = Field(
        None, description="Tool choice mode"
    )
    functionName: Optional[str] = Field(
        None, description="Specific function to force-call"
    )

    @model_validator(mode="after")
    def validate_choice_fields(self) -> "YandexToolChoice":
        if (self.mode is None and self.functionName is None) or (
            self.mode is not None and self.functionName is not None
        ):
            raise ValueError("Exactly one of mode or functionName must be set")
        return self


class YandexRequest(BaseModel):
    """Yandex request model."""

    modelUri: str = Field(description="Full model URI including folder ID")
    messages: List[YandexMessage] = Field(
        description="Array of messages exchanged with the model"
    )
    completionOptions: YandexCompletionOptions = Field(
        description="Generation parameters"
    )
    tools: Optional[List[YandexTool]] = Field(
        None,
        description=(
            "List of tools that are available for the model to invoke "
            "during the completion generation"
        ),
    )
    jsonObject: Optional[bool] = Field(
        None,
        description="If true, requests JSON object output mode",
    )
    jsonSchema: Optional[YandexJsonSchema] = Field(
        None,
        description="JSON schema that model output must conform to",
    )
    parallelToolCalls: Optional[bool] = Field(
        None,
        description="Whether model can generate multiple tool calls in one response",
    )
    toolChoice: Optional[YandexToolChoice] = Field(
        None,
        description="Specifies tool-calling strategy",
    )

    @model_validator(mode="after")
    def validate_response_format(self) -> "YandexRequest":
        if self.jsonObject is not None and self.jsonSchema is not None:
            raise ValueError("Only one of jsonObject or jsonSchema can be set")
        return self


class YandexCompletionTokensDetails(BaseModel):
    """Yandex completion tokens details."""

    reasoningTokens: Optional[str] = Field(
        None,
        description=(
            "The number of tokens used specifically for "
            "internal reasoning performed by the model"
        ),
    )


class YandexUsage(BaseModel):
    """Yandex usage statistics."""

    inputTextTokens: str = Field(
        description="The number of tokens in the textual part of the model input"
    )
    completionTokens: str = Field(
        description="The number of tokens in the generated completion"
    )
    totalTokens: str = Field(
        description=(
            "The total number of tokens, including all input tokens "
            "and all generated tokens"
        )
    )
    completionTokensDetails: Optional[YandexCompletionTokensDetails] = Field(
        None,
        description=(
            "Provides additional information about how the "
            "completion tokens were utilized"
        ),
    )


class YandexResponseMessage(BaseModel):
    """Yandex response message."""

    role: Literal["assistant"] = Field(description="Message role: assistant response")
    text: Optional[str] = Field(None, description="Message content")
    toolCallList: Optional[YandexToolCallList] = Field(
        None, description="List of tool calls made by the model"
    )
    toolResultList: Optional[YandexToolResultList] = Field(
        None, description="List of tool results from external tools"
    )


class YandexAlternativeStatus(str, Enum):
    """Yandex alternative status."""

    UNSPECIFIED = "ALTERNATIVE_STATUS_UNSPECIFIED"
    PARTIAL = "ALTERNATIVE_STATUS_PARTIAL"
    TRUNCATED_FINAL = "ALTERNATIVE_STATUS_TRUNCATED_FINAL"
    FINAL = "ALTERNATIVE_STATUS_FINAL"
    CONTENT_FILTER = "ALTERNATIVE_STATUS_CONTENT_FILTER"
    TOOL_CALLS = "ALTERNATIVE_STATUS_TOOL_CALLS"


class YandexAlternative(BaseModel):
    """Yandex completion alternative."""

    message: YandexResponseMessage = Field(description="Generated message")
    status: YandexAlternativeStatus = Field(description="Generation status")


class YandexResult(BaseModel):
    """Yandex completion result."""

    alternatives: List[YandexAlternative] = Field(
        description="Array of model responses"
    )
    usage: Optional[YandexUsage] = Field(None, description="Token usage statistics")
    modelVersion: Optional[str] = Field(
        None, description="Model version that changes with each release"
    )


class YandexResponse(BaseModel):
    """Yandex completion response."""

    result: YandexResult = Field(description="Response result")


class YandexStreamDelta(BaseModel):
    """Yandex streaming delta."""

    role: Optional[str] = Field(None, description="Message role if changed")
    text: Optional[str] = Field(None, description="New content delta")
    toolCallList: Optional[YandexToolCallList] = Field(
        None, description="List of tool calls made by the model"
    )
    toolResultList: Optional[YandexToolResultList] = Field(
        None, description="List of tool results from external tools"
    )


class YandexStreamChoice(BaseModel):
    """Yandex streaming choice."""

    index: int = Field(0, description="Choice index in the array")
    delta: YandexStreamDelta = Field(description="Changes in this chunk")


class YandexStreamResponse(BaseModel):
    """Yandex streaming response."""

    id: str = Field(
        default_factory=lambda: str(uuid.uuid4()), description="Response identifier"
    )
    choices: List[YandexStreamChoice] = Field(description="Array of streaming choices")
    created: int = Field(
        default_factory=lambda: int(datetime.utcnow().timestamp()),
        description="Response creation timestamp in Unix time",
    )
    object: YandexObjectType = Field(
        default=YandexObjectType.CHAT_COMPLETION_CHUNK,
        description="Response type identifier",
    )


class YandexToken(BaseModel):
    """Yandex token model."""

    id: str = Field(description="Internal token identifier")
    text: str = Field(description="Textual representation of the token")
    special: bool = Field(description="Indicates whether the token is special or not")


class YandexTokenizeResponse(BaseModel):
    """Yandex tokenize response."""

    tokens: List[YandexToken] = Field(description="List of tokens from tokenization")
    modelVersion: str = Field(description="Model version")
