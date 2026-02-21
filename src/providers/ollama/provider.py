"""Ollama provider implementation."""
import uuid
from typing import AsyncGenerator, Dict, List, Optional, cast
from urllib.parse import urljoin

from httpx import AsyncClient, HTTPError

from core.cache import RedisClient
from core.logger import LoggerService
from ..base import Provider
from ..models import (
    ProviderConfig,
    ProviderError,
    ProviderModel,
    ProviderRequest,
    ProviderStreamChunk,
)
from .model_mapper import OllamaModelMapper
from ..xrouter.mapper import XRouterMapper


class OllamaProvider(Provider):
    """Ollama API provider implementation."""

    MODELS_CACHE_TTL = 600  # 10 minutes

    def __init__(
        self,
        cache: RedisClient,
        logger: LoggerService,
        provider: ProviderConfig,
        mapper: XRouterMapper,
        model_mapper: OllamaModelMapper,
    ) -> None:
        """Initialize Ollama provider.

        Args:
            cache: Redis cache client
            logger: Logger service instance
            provider: Provider config
            mapper: LLM Gateway mapper instance
            model_mapper: Ollama model mapper instance for handling model data
        """
        super().__init__(provider=provider)
        self._has_finish_reason = False
        self._last_chunk: Optional[ProviderStreamChunk] = None
        self.cache = cache
        self.logger = logger.get_logger(__name__)
        self.mapper = mapper
        self.model_mapper = model_mapper
        self._client = AsyncClient(
            timeout=600.0,  # 10 min timeout for streaming
        )
        self.logger.info(
            "Initialized OllamaProvider",
            extra={
                "provider_id": self._provider.provider_id,
                "timeout": 600.0,
                "client_id": id(self._client),
                "mapper_type": type(mapper).__name__,
            },
        )

    def _is_final_chunk(self, chunk: ProviderStreamChunk) -> bool:
        """Check if this is the final chunk.

        For Ollama, a chunk is final if it has finish_reason.
        Since Ollama never sends usage info, we'll create synthetic usage
        when we see finish_reason.

        Args:
            chunk: Stream chunk to check

        Returns:
            bool: True if this is the final chunk
        """
        has_finish_reason = any(
            choice.finish_reason is not None for choice in chunk.choices
        )

        if has_finish_reason:
            self._has_finish_reason = True
            self._last_chunk = chunk
            return True

        return False

    def _get_request_headers(self) -> Dict[str, str]:
        """Get headers for API request."""
        return {
            "Content-Type": "application/json",
        }

    async def create_completion(  # noqa: C901
        self, request: ProviderRequest
    ) -> AsyncGenerator[ProviderStreamChunk, None]:
        """Create completion using Ollama API.

        Args:
            request: Provider-agnostic request

        Yields:
            AsyncGenerator that yields StreamChunks

        Raises:
            ProviderError: If request fails
        """
        try:
            # Map request to OpenAI-like API format
            openai_request = self.mapper.map_to_provider_request(request)
            self.logger.info(
                "Mapped request",
                extra={
                    "request_id": request.request_id,
                    "has_messages": bool(openai_request.get("messages")),
                    "has_functions": bool(openai_request.get("functions")),
                    "stream": openai_request.get("stream", False),
                },
            )
            self.logger.debug(
                "Request details",
                extra={
                    "request": openai_request,
                    "request_id": request.request_id,
                },
            )

            # Make API request
            self.logger.info(
                "Starting stream request",
                extra={
                    "request_id": request.request_id,
                    "base_url": self._provider.base_url,
                },
            )
            api_url = urljoin(self._provider.base_url, "/v1/chat/completions")
            async with self._client.stream(
                "POST",
                api_url,
                headers=self._get_request_headers(),
                json=openai_request,
            ) as response:
                self.logger.info(
                    "Got stream response, checking status",
                    extra={"request_id": request.request_id},
                )

                # Read error response before raising status
                if response.status_code >= 400:
                    error_text = await response.aread()
                    self.logger.error(
                        "Error response from Ollama API",
                        extra={
                            "status_code": response.status_code,
                            "error_text": error_text.decode(),
                            "request_id": request.request_id,
                        },
                    )
                    response.raise_for_status()
                self.logger.info(
                    "Status check passed, starting stream processing",
                    extra={"request_id": request.request_id},
                )

                # Process stream chunks
                aiter = response.aiter_lines()
                self.logger.info(
                    "Got aiter",
                    extra={"aiter": str(aiter), "request_id": request.request_id},
                )

                try:
                    async for line in aiter:
                        self.logger.info(
                            "Stream data received",
                            extra={
                                "request_id": request.request_id,
                                "has_data": bool(line),
                                "is_done": line.strip() == "data: [DONE]",
                            },
                        )
                        self.logger.debug(
                            "Stream data details",
                            extra={
                                "line": line,
                                "request_id": request.request_id,
                            },
                        )

                        if not (chunk_data := self.mapper.parse_sse_line(line)):
                            self.logger.debug(
                                "Skipping non-data line",
                                extra={"request_id": request.request_id},
                            )
                            continue

                        if chunk_data.get("event") == "done":
                            # Received 'data: [DONE]'
                            self.logger.info(
                                "Received [DONE]",
                                extra={"request_id": request.request_id},
                            )
                            # If we had finish_reason, create synthetic usage chunk
                            if self._has_finish_reason and self._last_chunk:
                                # Create synthetic chunk data
                                synthetic_data = {
                                    "id": self._last_chunk.id,
                                    "created": self._last_chunk.created,
                                    "choices": [
                                        {
                                            "index": 0,
                                            "delta": {
                                                "role": "assistant",
                                                "content": "",
                                            },
                                            "finish_reason": None,
                                            "native_finish_reason": None,
                                            "logprobs": None,
                                        }
                                    ],
                                    "usage": {
                                        "prompt_tokens": 0,
                                        "completion_tokens": 0,
                                        "total_tokens": 0,
                                        "prompt_tokens_details": {"cached_tokens": 0},
                                        "completion_tokens_details": {
                                            "reasoning_tokens": 0
                                        },
                                    },
                                    "request_id": request.request_id,
                                }
                                # Let mapper handle the conversion
                                synthetic_chunk = self.mapper.map_provider_stream_chunk(
                                    synthetic_data,
                                    request.model,
                                    self._provider.provider_id,
                                )
                                yield synthetic_chunk
                            break

                        # Add request_id to chunk data and map to provider format
                        chunk_data["request_id"] = request.request_id
                        mapped_chunk = self.mapper.map_provider_stream_chunk(
                            chunk_data, request.model, self._provider.provider_id
                        )
                        self.logger.info(
                            "Yielding chunk",
                            extra={
                                "request_id": request.request_id,
                                "has_choices": bool(mapped_chunk.choices),
                                "has_usage": bool(mapped_chunk.usage),
                                "finish_reason": mapped_chunk.choices[0].finish_reason
                                if mapped_chunk.choices
                                else None,
                            },
                        )
                        self.logger.debug(
                            "Chunk details",
                            extra={
                                "chunk": str(mapped_chunk),
                                "request_id": request.request_id,
                            },
                        )
                        # Check if this is the final chunk
                        if self._is_final_chunk(mapped_chunk):
                            yield mapped_chunk
                            break  # Exit the loop, ending the generator

                        yield mapped_chunk

                except Exception as e:
                    self.logger.error(
                        "Error in stream processing",
                        extra={"error": str(e), "request_id": request.request_id},
                        exc_info=True,
                    )
                    raise

        except HTTPError as e:
            # Read error response content before accessing it
            error_text = None
            if hasattr(e, "response"):
                error_text = e.response.read().decode()

            self.logger.error(
                "HTTP error in stream request",
                extra={
                    "error": str(e),
                    "error_type": type(e).__name__,
                    "status_code": e.response.status_code
                    if hasattr(e, "response")
                    else None,
                    "response_text": error_text,
                    "request_url": e.request.url if hasattr(e, "request") else None,
                    "request_id": request.request_id,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=e.response.status_code if hasattr(e, "response") else 500,
                message=f"Ollama API error: {str(e)}",
                details={"error": str(e), "response": error_text},
            )
        except Exception as e:
            self.logger.error(
                "Unexpected error in stream request",
                extra={"error": str(e), "request_id": request.request_id},
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message=f"Unexpected error: {str(e)}",
                details={"error": str(e)},
            )

    async def get_models(self) -> List[ProviderModel]:
        """Get list of available models.

        Returns:
            List of available provider models
        """
        request_id = str(uuid.uuid4())
        cache_key = f"models:ollama:{self._provider.base_url}"

        # Try to get models from cache
        cached_models = await self.cache.cache_get(cache_key)
        if cached_models:
            self.logger.info(
                "Retrieved models from cache",
                extra={
                    "request_id": request_id,
                    "count": len(cached_models),
                    "base_url": self._provider.base_url,
                },
            )
            return [ProviderModel.model_validate(m) for m in cached_models]

        try:
            # First get list of models from /api/tags
            self.logger.info(
                "Getting models list from /api/tags",
                extra={
                    "request_id": request_id,
                    "base_url": self._provider.base_url,
                },
            )
            response = await self._client.get(
                f"{self._provider.base_url}/api/tags",
                headers=self._get_request_headers(),
            )
            response.raise_for_status()
            models_list = response.json().get("models", [])

            # Get detailed info for each model
            models_data = []
            for model in models_list:
                model_name = model.get("name")
                try:
                    # Get detailed model info from /api/show
                    self.logger.info(
                        "Getting detailed info for model",
                        extra={
                            "request_id": request_id,
                            "model": model_name,
                        },
                    )
                    show_response = await self._client.post(
                        f"{self._provider.base_url}/api/show",
                        headers=self._get_request_headers(),
                        json={"model": model_name},
                    )
                    show_response.raise_for_status()
                    model_details = show_response.json()

                    # Add both basic and detailed info to the array
                    model_data = {
                        "tags_info": model,
                        "show_info": model_details,
                    }
                    models_data.append(model_data)

                except Exception as e:
                    self.logger.error(
                        "Error getting details for model",
                        extra={
                            "request_id": request_id,
                            "model": model_name,
                            "error": str(e),
                        },
                        exc_info=True,
                    )
                    # Continue with other models if one fails

            # Let the model mapper handle all the data
            models = self.model_mapper.map_provider_models({"models": models_data})

            # Cache models
            await self.cache.cache_set(
                cache_key,
                [m.model_dump() for m in models],
                expire=self.MODELS_CACHE_TTL,
            )

            return models

        except HTTPError as e:
            self.logger.error(
                "HTTP error while getting models",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                    "status_code": e.response.status_code
                    if hasattr(e, "response")
                    else None,
                    "response_text": e.response.text
                    if hasattr(e, "response")
                    else None,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=e.response.status_code if hasattr(e, "response") else 500,
                message=f"Failed to get models: {str(e)}",
                details={"error": str(e)},
            )
        except Exception as e:
            self.logger.error(
                "Unexpected error while getting models",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                },
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message=f"Unexpected error getting models: {str(e)}",
                details={"error": str(e)},
            )

    async def get_model(self, model_id: str) -> ProviderModel:
        """Get model by model ID.

        Args:
            model_id: Model ID to look up

        Returns:
            Model information

        Raises:
            ProviderError: If model is not found or operation fails
        """
        request_id = str(uuid.uuid4())
        cache_key = f"model:{self._provider.provider_id}:{model_id.lower()}"

        # Try to get model from cache
        cached_model = await self.cache.cache_get(cache_key)
        if cached_model:
            self.logger.info(
                "Retrieved model from cache",
                extra={
                    "request_id": request_id,
                    "model_id": model_id,
                },
            )
            return cast(ProviderModel, ProviderModel.model_validate(cached_model))

        # If not in cache, get all models and find the requested one
        all_models = await self.get_models()
        model = next(
            (m for m in all_models if m.model_id.lower() == model_id.lower()),
            None,
        )

        if model is None:
            self.logger.error(
                "Model not found",
                extra={"model_id": model_id, "request_id": request_id},
            )
            raise ProviderError(
                code=404,
                message=f"Model {model_id} not found",
                details={"model_id": model_id},
            )

        # Cache individual model
        await self.cache.cache_set(
            cache_key, model.model_dump(), expire=self.MODELS_CACHE_TTL
        )

        self.logger.info(
            "Found model",
            extra={
                "model_id": model_id,
                "request_id": request_id,
            },
        )

        return model

    async def close(self) -> None:
        """Close HTTP client."""
        self.logger.debug("Closing Ollama HTTP client")
        await self._client.aclose()
        self.logger.info("Ollama HTTP client closed")
