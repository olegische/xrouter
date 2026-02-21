"""Z.AI request and response mappers."""
import uuid
from datetime import datetime
from typing import Any, Dict

from core.logger import LoggerService
from ..deepseek.mapper import DeepseekMapper
from ..models import (
    ProviderConfig,
    ProviderError,
    ProviderRequest,
    ProviderStreamChunk,
    ResponseType,
    StreamChoice,
    Usage,
)
from ..models.base.messages import Message


class ZaiMapper(DeepseekMapper):
    """Mapper for Z.AI API requests and responses.

    Z.AI uses OpenAI-compatible format with minor differences:
    - Uses max_tokens instead of max_completion_tokens
    - Supports reasoning_content in response (maps to reasoning)
    """

    def __init__(self, provider: ProviderConfig, logger: LoggerService) -> None:
        """Initialize mapper.

        Args:
            provider: Provider model instance
            logger: Logger service instance
        """
        super().__init__(provider=provider, logger=logger)

    def map_to_provider_request(self, request: ProviderRequest) -> Dict[str, Any]:
        """Map provider request to Z.AI API request.

        Args:
            request: Provider-agnostic request

        Returns:
            Z.AI API request model

        Raises:
            ProviderError: If mapping fails
        """
        try:
            if not isinstance(request, ProviderRequest):
                raise ProviderError(
                    code=400,
                    message="Failed to map request for Z.AI",
                    details={"error": "Invalid request type"},
                )

            self.logger.debug(
                "Mapping provider request to Z.AI API format",
                extra={
                    "model": request.model,
                    "temperature": request.temperature,
                    "max_tokens": request.max_tokens,
                    "request_id": request.request_id,
                },
            )

            # Convert messages to OpenAI format
            messages = [
                Message.model_validate(msg, use_openai=True).model_dump()
                for msg in request.messages
            ]

            zai_request: Dict[str, Any] = {
                "model": request.model,
                "messages": messages,
                "temperature": request.temperature,
                "top_p": request.top_p,
                "stream": True,  # Always stream for unified response handling
                "max_tokens": request.max_tokens,
            }

            if request.tools:
                zai_request["tools"] = [
                    tool.model_dump() for tool in request.tools
                ]

            if request.tool_choice:
                if isinstance(request.tool_choice, str):
                    zai_request["tool_choice"] = request.tool_choice
                else:
                    zai_request["tool_choice"] = request.tool_choice.model_dump()

            # Z.AI supports thinking/reasoning mode for GLM-5, GLM-4.7, etc.
            if request.reasoning:
                zai_request["thinking"] = {"type": "enabled"}

            self.logger.debug(
                "Successfully mapped provider request to Z.AI API format",
                extra={"request_id": request.request_id},
            )
            return zai_request

        except Exception as e:
            error_request_id = str(uuid.uuid4())
            self.logger.error(
                "Failed to map request to Z.AI API format",
                extra={
                    "error": str(e),
                    "request_id": error_request_id,
                },
            )
            raise ProviderError(
                code=400,
                message="Failed to map request for Z.AI",
                details={"error": str(e)},
            )

    def map_provider_stream_chunk(
        self, chunk_data: Dict, model: str, provider_id: str
    ) -> ProviderStreamChunk:
        """Map provider stream chunk to provider-agnostic format.

        Z.AI returns OpenAI-compatible stream format. Maps reasoning_content
        to reasoning for chain-of-thought models.

        Args:
            chunk_data: Raw chunk data
            model: Model name
            provider_id: Provider identifier

        Returns:
            Provider-agnostic stream chunk
        """
        request_id = chunk_data.get("request_id") or chunk_data.get(
            "id", str(uuid.uuid4())
        )
        self.logger.debug(
            "Mapping Z.AI API stream chunk",
            extra={
                "model": model,
                "provider_id": provider_id,
                "request_id": request_id,
            },
        )

        try:
            # Z.AI may use reasoning_content in delta - map to reasoning
            choices = []
            for choice in chunk_data.get("choices", []):
                delta = choice.get("delta", {})
                if delta.get("role") is None:
                    delta = dict(delta)
                    delta["role"] = "assistant"
                if delta.get("reasoning_content") is not None:
                    delta = dict(delta)
                    delta["reasoning"] = delta.pop("reasoning_content")
                choices.append(
                    {
                        "index": choice.get("index", 0),
                        "delta": delta,
                        "finish_reason": choice.get("finish_reason"),
                        "logprobs": choice.get("logprobs"),
                    }
                )

            provider_chunk = ProviderStreamChunk(
                id=request_id,
                created=chunk_data.get(
                    "created", int(datetime.now().timestamp())
                ),
                model=model,
                provider_id=provider_id,
                request_id=request_id,
                object=ResponseType.CHAT_COMPLETION_CHUNK,
                choices=[StreamChoice.model_validate(c) for c in choices],
                usage=Usage.model_validate(chunk_data["usage"])
                if chunk_data.get("usage")
                else None,
            )

            self.logger.debug(
                "Successfully mapped Z.AI API stream chunk",
                extra={
                    "chunk_id": provider_chunk.id,
                    "model": model,
                    "request_id": request_id,
                },
            )

            return provider_chunk

        except Exception as e:
            self.logger.error(
                "Failed to map Z.AI API stream chunk",
                extra={
                    "error": str(e),
                    "request_id": request_id,
                },
            )
            raise ProviderError(
                code=500,
                message="Failed to map Z.AI API stream chunk",
                details={"error": str(e)},
            )
