"""LLM Gateway request and response mappers."""
import json
import uuid
from datetime import datetime
from typing import Any, Dict, Optional, cast

from core.logger import LoggerService
from ..base_mapper import BaseMapper
from ..models import (
    ProviderConfig,
    ProviderError,
    ProviderRequest,
    ProviderStreamChunk,
    ResponseType,
    StreamChoice,
    Usage,
)
from router.chat_completion.models.openai.request import OpenAIRequest


class XRouterMapper(BaseMapper):
    """Mapper for LLM Gateway requests and responses."""

    def __init__(self, provider: ProviderConfig, logger: LoggerService) -> None:
        """Initialize mapper.

        Args:
            provider: Provider model instance
            logger: Logger service instance
        """
        super().__init__(provider=provider, logger=logger)

    def map_to_provider_request(self, request: ProviderRequest) -> Dict[str, Any]:
        """Map provider request to OpenAI-like API request.

        Args:
            request: Provider-agnostic request

        Returns:
            OpenAI-like API request model

        Raises:
            ProviderError: If mapping fails
        """
        try:
            if not isinstance(request, ProviderRequest):
                raise ProviderError(
                    code=400,
                    message="Failed to map request for XRouter",
                    details={"error": "Invalid request type"},
                )

            self.logger.debug(
                "Mapping provider request to OpenAI-like API format",
                extra={
                    "model": request.model,
                    "temperature": request.temperature,
                    "max_tokens": request.max_tokens,
                    "request_id": request.request_id,
                },
            )

            # Create OpenAI request directly from ProviderRequest
            openai_request = OpenAIRequest(
                model=request.model,
                messages=request.messages,
                temperature=request.temperature,
                top_p=request.top_p,
                stream=True,  # Always stream for unified response handling
                max_completion_tokens=request.max_tokens,
                tools=request.tools,
                tool_choice=request.tool_choice,
            )
            return cast(Dict[str, Any], openai_request.model_dump())

        except Exception as e:
            error_request_id = str(uuid.uuid4())
            self.logger.error(
                "Failed to map request to OpenAI-like API format",
                extra={
                    "error": str(e),
                    "request_id": error_request_id,
                },
            )
            raise ProviderError(
                code=400,
                message="Failed to map request for XRouter",
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
        request_id = chunk_data.get("request_id", str(uuid.uuid4()))
        self.logger.debug(
            "Mapping OpenAI-like API stream chunk",
            extra={
                "model": model,
                "provider_id": provider_id,
                "request_id": request_id,
            },
        )

        try:
            # Create ProviderStreamChunk directly from chunk data
            # The model will handle validation and conversion
            provider_chunk = ProviderStreamChunk(
                id=request_id,
                created=chunk_data.get("created", int(datetime.now().timestamp())),
                model=model,
                provider_id=provider_id,
                request_id=request_id,
                object=ResponseType.CHAT_COMPLETION_CHUNK,
                choices=[
                    StreamChoice.model_validate(choice)
                    for choice in chunk_data.get("choices", [])
                ],
                usage=Usage.model_validate(chunk_data["usage"])
                if chunk_data.get("usage")
                else None,
            )

            self.logger.debug(
                "Successfully mapped OpenAI-like API stream chunk",
                extra={
                    "chunk_id": provider_chunk.id,
                    "model": model,
                    "request_id": request_id,
                },
            )

            return provider_chunk

        except Exception as e:
            self.logger.error(
                "Failed to map OpenAI-like API stream chunk",
                extra={
                    "error": str(e),
                    "request_id": request_id,
                },
            )
            raise ProviderError(
                code=500,
                message="Failed to map OpenAI-like API stream chunk",
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
