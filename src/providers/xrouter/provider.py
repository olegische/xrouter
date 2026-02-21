"""LLM Gateway provider implementation."""
import uuid
from typing import AsyncGenerator, Dict, List, cast

from httpx import AsyncClient, HTTPError

from core.cache import RedisClient
from core.logger import LoggerService
from ..base import Provider
from ..base_mapper import BaseMapper
from ..base_model_mapper import BaseModelMapper
from ..models import (
    ProviderConfig,
    ProviderError,
    ProviderModel,
    ProviderRequest,
    ProviderStreamChunk,
)


class XRouterProvider(Provider):
    """LLM Gateway API provider implementation."""

    MODELS_CACHE_TTL = 6000  # 1 hour

    def __init__(
        self,
        cache: RedisClient,
        logger: LoggerService,
        provider: ProviderConfig,
        mapper: BaseMapper,
        model_mapper: BaseModelMapper,
    ) -> None:
        """Initialize LLM Gateway provider.

        Args:
            cache: Redis cache client
            logger: Logger service instance
            provider: Provider config
            mapper: Provider-specific mapper instance
            model_mapper: Provider-specific model mapper instance
        """
        super().__init__(provider=provider)
        self._has_finish_reason = False
        self.cache = cache
        self.logger = logger.get_logger(__name__)
        self.mapper = mapper
        self.model_mapper = model_mapper
        # Get verify_ssl parameter from provider config, default to True for backward compatibility
        verify_ssl = self._provider.parameters.get("verify_ssl", True)
        self._client = AsyncClient(
            timeout=300.0,  # 5 min timeout for streaming
            verify=verify_ssl,
        )
        self.logger.info(
            "Initialized XRouterProvider",
            extra={
                "provider_id": self._provider.provider_id,
                "timeout": 300.0,
                "client_id": id(self._client),
                "mapper_type": type(mapper).__name__,
            },
        )

    def _is_final_chunk(self, chunk: ProviderStreamChunk) -> bool:
        """Check if this is the final chunk.

        A chunk is considered final if:
        - It has both finish_reason and usage in the same chunk
        - Or it has usage and we previously saw finish_reason

        Args:
            chunk: Stream chunk to check

        Returns:
            bool: True if this is the final chunk
        """
        has_finish_reason = any(
            choice.finish_reason is not None for choice in chunk.choices
        )
        has_usage = chunk.usage is not None

        # Case 1: Both finish_reason and usage in same chunk
        if has_finish_reason and has_usage:
            return True

        # Case 2: Has usage and we previously saw finish_reason
        if has_usage and self._has_finish_reason:
            return True

        # Store state if we see finish_reason
        if has_finish_reason:
            self._has_finish_reason = True
            return False

        return False

    def _get_request_headers(self) -> Dict[str, str]:
        """Get headers for API request."""
        headers: Dict[str, str] = {"Content-Type": "application/json"}
        if self._provider.credentials:
            headers["Authorization"] = f"Bearer {self._provider.credentials}"

        # Add OpenRouter specific headers if this is OpenRouter provider
        if "openrouter" in self._provider.provider_id:
            headers.update(
                {
                    "HTTP-Referer": "https://xrouter.chat",
                    "X-Title": "xrouter",
                }
            )

        return headers

    async def get_models(self) -> List[ProviderModel]:
        """Get list of available models.

        Returns:
            List of available provider models

        Raises:
            ProviderError: If models retrieval fails
        """
        request_id = str(uuid.uuid4())
        cache_key = f"models:{self._provider.provider_id}"

        # Try to get models from cache
        cached_models = await self.cache.cache_get(cache_key)
        if cached_models:
            self.logger.info(
                "Retrieved models from cache",
                extra={
                    "request_id": request_id,
                    "count": len(cached_models),
                },
            )
            return [ProviderModel.model_validate(m) for m in cached_models]

        try:
            self.logger.info(
                "Getting models list from API",
                extra={
                    "request_id": request_id,
                    "base_url": self._provider.base_url,
                },
            )
            response = await self._client.get(
                f"{self._provider.base_url}/models",
                headers=self._get_request_headers(),
            )
            response.raise_for_status()
            data = response.json()

            models = self.model_mapper.map_provider_models(data)

            # Cache models for 24 hours
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

        # Cache individual model for 24 hours
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

    async def create_completion(  # noqa: C901
        self, request: ProviderRequest
    ) -> AsyncGenerator[ProviderStreamChunk, None]:
        """Create completion using LLM Gateway API.

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
            async with self._client.stream(
                "POST",
                f"{self._provider.base_url}/chat/completions",
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
                        f"Error response from {self._provider.provider_id} API",
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
                            break

                        # Check if the chunk contains an error
                        if "error" in chunk_data:
                            error_data = chunk_data["error"]
                            error_message = error_data.get(
                                "message", "Provider returned error"
                            )
                            error_code = error_data.get("code", 500)
                            error_metadata = error_data.get("metadata", {})

                            # Get the raw error string if available
                            raw_error = error_metadata.get("raw", "")

                            # Use raw error if available
                            if raw_error:
                                error_message = raw_error

                            # If this is the unsupported country error, use 403 code
                            if "unsupported_country_region_territory" in str(
                                error_data
                            ) or "unsupported_country_region_territory" in str(
                                raw_error
                            ):
                                error_code = 403

                            provider_name = error_metadata.get(
                                "provider_name", self._provider.provider_id
                            )

                            self.logger.error(
                                "Error in stream data",
                                extra={
                                    "error": error_message,
                                    "code": error_code,
                                    "metadata": error_metadata,
                                    "provider_name": provider_name,
                                    "request_id": request.request_id,
                                },
                            )

                            raise ProviderError(
                                code=error_code,
                                message=error_message,
                                details={
                                    "error": error_data,
                                    "provider_id": self._provider.provider_id,
                                    "provider_name": provider_name,
                                },
                            )

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
                message=f"{self._provider.provider_id} API error: {str(e)}",
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

    async def close(self) -> None:
        """Close HTTP client."""
        self.logger.debug(f"Closing {self._provider.provider_id} HTTP client")
        await self._client.aclose()
        self.logger.info(f"{self._provider.provider_id} HTTP client closed")
