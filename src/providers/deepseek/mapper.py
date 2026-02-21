"""Deepseek request and response mappers."""
import json
import uuid
from datetime import datetime
from typing import Any, Dict, Optional, cast

from core.logger import LoggerService
from ..base_mapper import BaseMapper
from .models import (
    DeepseekStreamChoice,
    DeepseekStreamResponse,
    DeepseekUsage,
)
from ..models import (
    ProviderConfig,
    ProviderError,
    ProviderRequest,
    ProviderStreamChunk,
    ResponseType,
    Usage,
)
from ..models.base.common import (
    CompletionTokensDetails,
    PromptTokensDetails,
)
from ..models.base.messages import (
    AssistantMessage,
    Message,
    SystemMessage,
    ToolMessage,
    UserMessage,
)


class DeepseekMapper(BaseMapper):
    """Mapper for Deepseek requests and responses."""

    def __init__(self, provider: ProviderConfig, logger: LoggerService) -> None:
        """Initialize mapper.

        Args:
            provider: Provider model instance
            logger: Logger service instance
        """
        super().__init__(provider=provider, logger=logger)

    def _is_preamble_assistant_message(
        self,
        request: ProviderRequest,
        msg_index: int,
        pending_tool_call_id: Optional[str],
    ) -> bool:
        msg = request.messages[msg_index]
        if (
            msg.role != "assistant"
            or msg.tool_calls
            or not pending_tool_call_id
        ):
            return False

        for future_msg in request.messages[msg_index + 1 :]:
            if (
                future_msg.role == "tool"
                and future_msg.tool_call_id == pending_tool_call_id
            ):
                self.logger.debug(
                    "Skipping preamble assistant message between tool_call and result",
                    extra={
                        "content_preview": msg.content[:100] if msg.content else None,
                        "tool_call_id": pending_tool_call_id,
                    },
                )
                return True
            if future_msg.role in ("assistant", "user"):
                break
        return False

    def map_to_provider_request(self, request: ProviderRequest) -> Dict[str, Any]:
        """Map provider request to Deepseek API request.

        Args:
            request: Provider-agnostic request

        Returns:
            Deepseek API request model

        Raises:
            ProviderError: If mapping fails
        """
        try:
            if not isinstance(request, ProviderRequest):
                raise ProviderError(
                    code=400,
                    message="Failed to map request for Deepseek",
                    details={"error": "Invalid request type"},
                )

            self.logger.debug(
                "Mapping provider request to Deepseek API format",
                extra={
                    "model": request.model,
                    "temperature": request.temperature,
                    "max_tokens": request.max_tokens,
                    "request_id": request.request_id,
                },
            )

            # Convert messages to our Message type
            messages = []
            last_assistant_with_tool_call: Optional[str] = None
            for msg_idx, msg in enumerate(request.messages):
                if self._is_preamble_assistant_message(
                    request=request,
                    msg_index=msg_idx,
                    pending_tool_call_id=last_assistant_with_tool_call,
                ):
                    continue

                # Handle different message types
                if msg.role == "user":
                    content = self.convert_content_to_string(msg.role, msg.content)
                    message = UserMessage(
                        role=msg.role,
                        content=content if content is not None else "",
                        name=msg.name if hasattr(msg, "name") else None,
                    )
                elif msg.role == "assistant":
                    message = AssistantMessage(
                        role=msg.role,
                        content=msg.content,
                        tool_calls=msg.tool_calls
                        if hasattr(msg, "tool_calls")
                        else None,
                    )
                    if message.tool_calls:
                        last_assistant_with_tool_call = message.tool_calls[0].id
                elif msg.role == "system":
                    message = SystemMessage(
                        role=msg.role,
                        content=msg.content,
                    )
                elif msg.role == "tool":
                    message = ToolMessage(
                        role=msg.role,
                        content=msg.content,
                        name=msg.name,
                        tool_call_id=msg.tool_call_id,
                    )
                    if msg.tool_call_id == last_assistant_with_tool_call:
                        last_assistant_with_tool_call = None
                else:
                    # Fallback to base Message class
                    message = Message(
                        role=msg.role,
                        content=msg.content,
                        name=msg.name if hasattr(msg, "name") else None,
                    )
                messages.append(message)

            # Create request object
            deepseek_request = {
                "model": request.model,
                "messages": [msg.model_dump() for msg in messages],
                "temperature": request.temperature,
                "top_p": request.top_p,
                "stream": True,  # Always stream for unified response handling
                "max_tokens": request.max_tokens,  # Deepseek uses max_tokens
            }

            # Add tools and tool_choice if present
            if request.tools:
                deepseek_request["tools"] = [
                    tool.model_dump() for tool in request.tools
                ]

            if request.tool_choice:
                if isinstance(request.tool_choice, str):
                    deepseek_request["tool_choice"] = request.tool_choice
                else:
                    deepseek_request["tool_choice"] = request.tool_choice.model_dump()

            self.logger.debug(
                "Successfully mapped provider request to Deepseek API format",
                extra={"request_id": request.request_id},
            )
            return deepseek_request

        except Exception as e:
            error_request_id = str(uuid.uuid4())
            self.logger.error(
                "Failed to map request to Deepseek API format",
                extra={
                    "error": str(e),
                    "request_id": error_request_id,
                },
            )
            raise ProviderError(
                code=400,
                message="Failed to map request for Deepseek",
                details={"error": str(e)},
            )

    def map_provider_stream_chunk(
        self, chunk_data: Dict, model: str, provider_id: str
    ) -> ProviderStreamChunk:
        """Map provider stream chunk to provider-agnostic format.

        Args:
            chunk_data: Raw chunk data
            model: Model name
            provider_id: Provider identifier

        Returns:
            Provider-agnostic stream chunk
        """
        request_id = chunk_data.get("id", str(uuid.uuid4()))
        self.logger.debug(
            "Mapping Deepseek API stream chunk",
            extra={
                "model": model,
                "provider_id": provider_id,
                "request_id": request_id,
            },
        )

        try:
            # First validate the chunk data using our Deepseek-specific model
            deepseek_response = DeepseekStreamResponse(
                id=request_id,
                created=chunk_data.get("created", int(datetime.now().timestamp())),
                model=model,
                system_fingerprint=chunk_data.get("system_fingerprint"),
                choices=[
                    DeepseekStreamChoice.model_validate(choice)
                    for choice in chunk_data.get("choices", [])
                ],
                usage=DeepseekUsage.model_validate(chunk_data["usage"])
                if chunk_data.get("usage")
                else None,
            )

            # Map choices ensuring role is set correctly
            mapped_choices = []
            for choice in deepseek_response.choices:
                delta = choice.delta.model_dump()

                # Always set role to "assistant" for deepseek responses
                delta["role"] = "assistant"

                # Map reasoning_content to reasoning for deepseek-reasoner compatibility
                if delta.get("reasoning_content") is not None:
                    delta["reasoning"] = delta.pop("reasoning_content")

                mapped_choice = {
                    "index": choice.index,
                    "delta": delta,
                    "finish_reason": choice.finish_reason,
                    "logprobs": choice.logprobs,
                }
                mapped_choices.append(mapped_choice)

            # Map all usage fields including detailed breakdowns
            usage = None
            if deepseek_response.usage:
                # Create completion_tokens_details if available
                completion_tokens_details = None
                if deepseek_response.usage.completion_tokens_details:
                    completion_tokens_details = CompletionTokensDetails(
                        reasoning_tokens=deepseek_response.usage.completion_tokens_details.reasoning_tokens
                    )

                # Create prompt_tokens_details with cache information
                prompt_tokens_details = PromptTokensDetails(
                    cached_tokens=deepseek_response.usage.prompt_cache_hit_tokens
                )

                usage = Usage(
                    prompt_tokens=deepseek_response.usage.prompt_tokens,
                    completion_tokens=deepseek_response.usage.completion_tokens,
                    total_tokens=deepseek_response.usage.total_tokens,
                    completion_tokens_details=completion_tokens_details,
                    prompt_tokens_details=prompt_tokens_details,
                )

            # Create provider-agnostic stream chunk
            provider_chunk = ProviderStreamChunk(
                id=deepseek_response.id,
                created=deepseek_response.created,
                model=model,
                provider_id=provider_id,
                request_id=request_id,
                object=ResponseType.CHAT_COMPLETION_CHUNK,
                choices=mapped_choices,
                usage=usage,
            )

            self.logger.debug(
                "Successfully mapped Deepseek API stream chunk",
                extra={
                    "chunk_id": provider_chunk.id,
                    "model": model,
                    "request_id": request_id,
                },
            )

            return provider_chunk

        except Exception as e:
            self.logger.error(
                "Failed to map Deepseek API stream chunk",
                extra={
                    "error": str(e),
                    "request_id": request_id,
                },
            )
            raise ProviderError(
                code=500,
                message="Failed to map Deepseek API stream chunk",
                details={"error": str(e)},
            )

    def parse_sse_line(self, line: str) -> Optional[Dict[str, Any]]:
        """Parse SSE line into chunk data.

        Args:
            line: Raw SSE line

        Returns:
            Parsed chunk data or None if line should be skipped
        """
        if not line.strip():
            return None

        if line.strip() == "data: [DONE]":
            return {"event": "done"}  # Return special event

        if line.startswith("data: "):
            line = line[6:]  # Strip "data: " prefix

        try:
            chunk_data = json.loads(line)
            return cast(Dict[str, Any], chunk_data)
        except json.JSONDecodeError:
            return None
