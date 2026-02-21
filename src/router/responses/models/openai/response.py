"""OpenAI Responses API response models."""
from typing import Dict, List, Literal, Optional, Union

from pydantic import BaseModel, Field

from providers.models.base.tools import Tool, ToolChoice


class ResponsesOutputText(BaseModel):
    """Output text content item."""

    type: Literal["output_text"] = Field("output_text")
    text: str = Field(..., description="Generated text")
    annotations: List[Dict] = Field(default_factory=list)


class ResponsesOutputRefusal(BaseModel):
    """Output refusal content item."""

    type: Literal["refusal"] = Field("refusal")
    refusal: str = Field(..., description="Refusal text")


class ResponsesOutputMessage(BaseModel):
    """Message output item in Responses API format."""

    id: str = Field(..., description="Output item ID")
    type: Literal["message"] = Field("message")
    status: Literal["in_progress", "completed"] = Field("completed")
    role: Literal["assistant"] = Field("assistant")
    content: List[Union[ResponsesOutputText, ResponsesOutputRefusal]] = Field(
        default_factory=list
    )


class ResponsesOutputFunctionCall(BaseModel):
    """Function call output item in Responses API format."""

    id: str = Field(..., description="Function call output item ID")
    type: Literal["function_call"] = Field("function_call")
    call_id: str = Field(..., description="Tool call ID")
    name: str = Field(..., description="Function name")
    arguments: str = Field(..., description="Function arguments JSON")
    status: Literal["completed"] = Field("completed")


class ResponsesUsageInputTokensDetails(BaseModel):
    """Input token breakdown."""

    cached_tokens: Optional[int] = None


class ResponsesUsageOutputTokensDetails(BaseModel):
    """Output token breakdown."""

    reasoning_tokens: Optional[int] = None


class ResponsesUsage(BaseModel):
    """Token usage in Responses API format."""

    input_tokens: int
    output_tokens: int
    total_tokens: int
    input_tokens_details: Optional[ResponsesUsageInputTokensDetails] = None
    output_tokens_details: Optional[ResponsesUsageOutputTokensDetails] = None


class OpenAIResponsesResponse(BaseModel):
    """OpenAI-compatible Responses API response model."""

    id: str = Field(..., description="Response ID")
    object: Literal["response"] = Field("response")
    created_at: int = Field(..., description="Creation timestamp")
    status: Literal["in_progress", "completed", "failed"] = Field("completed")
    model: str = Field(..., description="Model identifier")
    output: List[Union[ResponsesOutputMessage, ResponsesOutputFunctionCall]] = Field(
        default_factory=list,
        description="Generated output items",
    )
    usage: Optional[ResponsesUsage] = Field(None, description="Usage information")
    error: Optional[Dict] = Field(None, description="Error details")
    incomplete_details: Optional[Dict] = Field(None)
    instructions: Optional[str] = Field(None)
    max_output_tokens: Optional[int] = Field(None)
    temperature: Optional[float] = Field(None)
    top_p: Optional[float] = Field(None)
    parallel_tool_calls: bool = Field(True)
    tools: Optional[List[Tool]] = Field(None)
    tool_choice: Optional[ToolChoice] = Field(None)
    output_text: Optional[str] = Field(None, description="Convenience aggregated text")
