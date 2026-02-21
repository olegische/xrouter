"""Message models for chat completion functionality."""
from enum import Enum
from typing import Any, List, Literal, Optional, Union, cast

from pydantic import BaseModel, Field, model_validator

from .tools import MessageToolCall


class ContentType(str, Enum):
    """Content type constants."""

    TEXT = "text"
    IMAGE_URL = "image_url"


class CacheControl(BaseModel):
    """Cache control model."""

    type: Literal["ephemeral"] = Field(
        "ephemeral",
        description="The type of cache control, currently only ephemeral is supported",
    )


class OpenAITextContent(BaseModel):
    """OpenAI text content part model."""

    type: Literal[ContentType.TEXT] = Field(
        ContentType.TEXT,
        description="The type of content, in this case text",
    )
    text: str = Field(description="The text content")


class AnthropicTextContent(OpenAITextContent):
    """Anthropic text content part model with cache control support."""

    cache_control: Optional[CacheControl] = Field(
        None,
        description="Optional cache control settings for this text content",
    )


# Use Anthropic model as our default since we support cache control
TextContent = AnthropicTextContent


class ImageDetail(str, Enum):
    """Image detail level constants."""

    AUTO = "auto"
    LOW = "low"
    HIGH = "high"


class ImageUrl(BaseModel):
    """Image URL model."""

    url: str = Field(
        description="Either a URL of the image or the base64 encoded image data"
    )
    detail: Optional[ImageDetail] = Field(
        default=ImageDetail.AUTO,
        description="Specifies the detail level of the image",
    )


class ImageUrlContent(BaseModel):
    """Image URL content part model."""

    type: Literal[ContentType.IMAGE_URL] = Field(
        ContentType.IMAGE_URL,
        description="The type of content, in this case image_url",
    )
    image_url: ImageUrl = Field(description="The image data")


# Content part unions for different APIs
OpenAIContentPart = Union[OpenAITextContent, ImageUrlContent]
AnthropicContentPart = Union[AnthropicTextContent, ImageUrlContent]

# Default to Anthropic since we support cache control
ContentPart = AnthropicContentPart


class BaseMessage(BaseModel):
    """Base class for all message types."""

    role: str = Field(description="The role of the message's author")
    content: Optional[Union[str, List[ContentPart]]] = Field(
        None,
        description=(
            "The contents of the message. Required for system and user messages, "
            "optional for assistant messages with tool calls. "
            "Can be string or list of content parts"
        ),
    )
    name: Optional[str] = Field(
        None,
        description=(
            "An optional name for the participant. Provides the model information "
            "to differentiate between participants of the same role"
        ),
    )


class SystemMessage(BaseMessage):
    """System message model.

    Developer-provided instructions that the model should follow,
    regardless of messages sent by the user.
    """

    role: Literal["system"] = Field(
        description="The role of the message's author, in this case system"
    )
    content: Union[str, List[ContentPart]] = Field(
        description="The system instructions for the model"
    )

    @model_validator(mode="after")
    def validate_content(self) -> "SystemMessage":
        """Validate that content is provided."""
        if isinstance(self.content, str):
            if not self.content:
                raise ValueError("System message must have content")
        else:
            if not self.content:
                raise ValueError("System message must have content")

            # Check if all content parts are text type (OpenAI limitation)
            for item in self.content:
                if not isinstance(item, TextContent):
                    raise ValueError("System message content parts must be text type")

                if not item.text:
                    raise ValueError("System message must have text content")
        return self


class UserMessage(BaseMessage):
    """User message model with Anthropic content parts (with cache_control).

    Messages sent by an end user, containing prompts or additional context information.
    """

    role: Literal["user"] = Field(
        description="The role of the message's author, in this case user"
    )
    content: Union[str, List[ContentPart]] = Field(
        description="The user's message content"
    )

    @model_validator(mode="after")
    def validate_content(self) -> "UserMessage":
        """Validate that content is provided."""
        if isinstance(self.content, str):
            if not self.content:
                raise ValueError("User message must have content")
        else:
            if not self.content:
                raise ValueError("User message must have content")

            # Check if at least one content part has valid content
            has_valid_content = False
            for item in self.content:
                if isinstance(item, TextContent) and item.text:
                    has_valid_content = True
                    break
                elif isinstance(item, ImageUrlContent) and item.image_url.url:
                    has_valid_content = True
                    break

            if not has_valid_content:
                raise ValueError(
                    "User message must have at least one valid content part"
                )
        return self


class OpenAIUserMessage(BaseMessage):
    """User message model with OpenAI content parts (without cache_control).

    Messages sent by an end user, containing prompts or additional context information.
    """

    role: Literal["user"] = Field(
        description="The role of the message's author, in this case user"
    )
    content: Union[str, List[OpenAIContentPart]] = Field(
        description="The user's message content"
    )

    @model_validator(mode="after")
    def validate_content(self) -> "OpenAIUserMessage":
        """Validate that content is provided."""
        if isinstance(self.content, str):
            if not self.content:
                raise ValueError("User message must have content")
        else:
            if not self.content:
                raise ValueError("User message must have content")

            # Check if at least one content part has valid content
            has_valid_content = False
            for item in self.content:
                if isinstance(item, OpenAITextContent) and item.text:
                    has_valid_content = True
                    break
                elif isinstance(item, ImageUrlContent) and item.image_url.url:
                    has_valid_content = True
                    break

            if not has_valid_content:
                raise ValueError(
                    "User message must have at least one valid content part"
                )
        return self


class ToolMessage(BaseMessage):
    """Tool message model.

    Messages containing the results of tool calls.
    """

    role: Literal["tool"] = Field(
        description="The role of the message's author, in this case tool"
    )
    content: str = Field(
        None,
        description=("Tool result"),
    )
    name: Optional[str] = Field(
        None,
        description=("Tool name"),
    )
    tool_call_id: str = Field(
        description="Tool call that this message is responding to"
    )


class AssistantMessage(BaseMessage):
    """Assistant message model.

    Messages sent by the model in response to user messages.
    """

    role: Literal["assistant"] = Field(
        description="The role of the message's author, in this case assistant"
    )
    content: Optional[str] = Field(
        None,
        description=(
            "The contents of the assistant message. Required unless tool_calls "
            "is specified"
        ),
    )
    reasoning: Optional[str] = Field(
        None,
        description="The reasoning tokens generated by the model for this message",
    )
    refusal: Optional[str] = Field(
        None,
        description="The refusal message by the assistant",
    )
    tool_calls: Optional[List[MessageToolCall]] = Field(
        None,
        description="The tool calls generated by the model, such as function calls",
    )


# Message type unions for different APIs
OpenAIMessageType = Union[
    SystemMessage, OpenAIUserMessage, AssistantMessage, ToolMessage
]
AnthropicMessageType = Union[SystemMessage, UserMessage, AssistantMessage, ToolMessage]

# Default to Anthropic since we support cache control
MessageType = AnthropicMessageType


class Message(BaseMessage):
    """Base class for all message types with role-based validation."""

    @classmethod
    def model_validate(
        cls, obj: Any, use_openai: bool = False
    ) -> Union[
        SystemMessage, UserMessage, OpenAIUserMessage, AssistantMessage, ToolMessage
    ]:
        """Validate and return appropriate message type based on role.

        Args:
            obj: The object to validate
            use_openai: If True, use OpenAI message types (without cache_control)
        """
        if not isinstance(obj, dict):
            obj = obj.model_dump()

        role = obj.get("role")
        if not role:
            raise ValueError("Role is required")

        if role == "system":
            return cast(MessageType, SystemMessage.model_validate(obj))
        elif role == "user":
            if use_openai:
                return cast(OpenAIMessageType, OpenAIUserMessage.model_validate(obj))
            else:
                return cast(MessageType, UserMessage.model_validate(obj))
        elif role == "assistant":
            return cast(MessageType, AssistantMessage.model_validate(obj))
        elif role == "tool":
            return cast(MessageType, ToolMessage.model_validate(obj))
        else:
            raise ValueError(f"Invalid role: {role}")
