"""Models for tool functionality."""
from typing import Any, Dict, Literal, Optional, Union

from pydantic import BaseModel, Field


class ToolFunction(BaseModel):
    """Function specification for tool."""

    name: str = Field(
        pattern=r"^[a-zA-Z0-9_-]+$",
        max_length=64,
        description=(
            "The name of the function to be called. Must be a-z, A-Z, 0-9, "
            "or contain underscores and dashes, with a maximum length of 64"
        ),
    )
    description: Optional[str] = Field(
        None,
        description=(
            "A description of what the function does, used by the model to choose "
            "when and how to call the function"
        ),
    )
    parameters: Optional[Dict[str, Any]] = Field(
        None,
        description=(
            "The parameters the functions accepts, described as a JSON Schema object. "
            "Omitting parameters defines a function with an empty parameter list"
        ),
    )


class Tool(BaseModel):
    """Tool model for function calling."""

    type: Literal["function"] = Field(
        description="The type of the tool. Currently, only function is supported"
    )
    function: ToolFunction = Field(description="The function to be called")


class ToolChoiceFunction(BaseModel):
    """Function specification for tool choice."""

    name: str = Field(description="The name of the function to call")


class ToolChoiceObject(BaseModel):
    """Object specification for tool choice."""

    type: Literal["function"] = Field(
        description="The type of the tool. Currently, only function is supported"
    )
    function: ToolChoiceFunction = Field(description="Function specification")


# Possible string values for tool_choice
ToolChoiceType = Literal["none", "auto", "required"]

# Union type for all possible tool_choice values
ToolChoice = Union[ToolChoiceType, ToolChoiceObject]


class FunctionCall(BaseModel):
    """Function call model."""

    name: Optional[str] = Field(None, description="The name of the function to call")
    arguments: Optional[str] = Field(
        None, description="The arguments to pass to the function"
    )


class ResponseToolCall(BaseModel):
    """Tool call model for responses."""

    index: int = Field(description="Index of this tool call")
    id: str = Field(description="ID of this tool call")
    type: Literal["function"] = Field(
        description="The type of the tool. Currently, only function is supported"
    )
    function: FunctionCall = Field(description="The function that was called")


class MessageToolCall(BaseModel):
    """Tool call model for requests."""

    id: Optional[str] = Field(None, description="ID of this tool call")
    type: Optional[Literal["function"]] = Field(
        None,
        description="The type of the tool. Currently, only function is supported",
    )
    function: Optional[FunctionCall] = Field(
        None, description="The function that was called"
    )
    index: Optional[int] = Field(
        None, description="Index of this tool call in streaming responses"
    )
