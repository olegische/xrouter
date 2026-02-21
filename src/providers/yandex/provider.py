"""Yandex provider implementation."""
from typing import AsyncGenerator, Dict, List, cast

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
from .mapper import YandexMapper
from .model_mapper import YandexModelMapper


class YandexProvider(Provider):
    """Yandex API provider implementation."""

    MODELS_CACHE_TTL = 86400  # 24 hours
    MODELS_VERSION = "2024-02-26"  # Update this when models change

    def __init__(
        self,
        cache: RedisClient,
        provider: ProviderConfig,
        logger: LoggerService,
        mapper: YandexMapper,
        model_mapper: YandexModelMapper,
    ):
        """Initialize Yandex provider.

        Args:
            cache: Redis cache client
            logger: Logger service instance
            provider: Provider config
            mapper: Yandex mapper instance for request/response mapping
            model_mapper: Yandex model mapper instance for models mapping
        """
        super().__init__(provider=provider)
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
            "Initialized YandexProvider",
            extra={
                "provider_id": self._provider.provider_id,
                "timeout": 300.0,
                "client_id": id(self._client),
                "mapper_type": type(mapper).__name__,
            },
        )

    def _is_final_chunk(self, chunk: ProviderStreamChunk) -> bool:
        """Check if chunk has any finish reason.

        Args:
            chunk: Stream chunk to check

        Returns:
            bool: True if chunk has any finish reason
        """
        return any(choice.finish_reason is not None for choice in chunk.choices)

    def _get_request_headers(self) -> Dict[str, str]:
        """Get headers for API request."""
        return {
            "Authorization": f"Api-Key {self._provider.credentials}",
            "Content-Type": "application/json",
            "x-folder-id": self._provider.parameters.get("folder_id", ""),
        }

    async def create_completion(  # noqa: C901
        self, request: ProviderRequest
    ) -> AsyncGenerator[ProviderStreamChunk, None]:
        """Create completion using Yandex API.

        Args:
            request: Provider-agnostic request

        Yields:
            AsyncGenerator that yields StreamChunks

        Raises:
            ProviderError: If request fails
        """
        try:
            # Check if model supports tools
            if request.tools and "lite" in request.model.lower():
                raise ProviderError(
                    code=400,
                    message="YandexGPT Lite does not support function calling",
                    details={
                        "error": (
                            "Function calling is only supported in YandexGPT Pro "
                            "and Pro 32k models"
                        )
                    },
                )

            # Map request to Yandex format
            yandex_request = self.mapper.map_to_provider_request(request)
            self.logger.info(
                "Mapped request",
                extra={
                    "request_id": request.request_id,
                    "has_messages": bool(yandex_request.get("messages")),
                    "has_tools": bool(yandex_request.get("tools")),
                    "stream": yandex_request.get("completionOptions", {}).get(
                        "stream",
                        False,
                    ),
                },
            )
            self.logger.debug(
                "Request details",
                extra={
                    "request": yandex_request,
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
                f"{self._provider.base_url}/completion",
                headers=self._get_request_headers(),
                json=yandex_request,
            ) as response:
                self.logger.info(
                    "Got stream response, checking status",
                    extra={"request_id": request.request_id},
                )

                # Read error response before raising status
                if response.status_code >= 400:
                    error_text = await response.aread()
                    self.logger.error(
                        "Error response from Yandex API",
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
                message=f"Yandex API error: {str(e)}",
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
        cache_key = f"models:{self._provider.provider_id}:v{self.MODELS_VERSION}"

        # Try to get models from cache
        cached_models = await self.cache.cache_get(cache_key)
        if cached_models:
            return [ProviderModel.model_validate(m) for m in cached_models]

        # Since Yandex doesn't have a models endpoint, we use the mapper directly
        # with empty dict as models_data since it's not used (models are hardcoded)
        models = self.model_mapper.map_provider_models({})

        # Cache models
        await self.cache.cache_set(
            cache_key, [m.model_dump() for m in models], expire=self.MODELS_CACHE_TTL
        )

        return models

    async def get_model(self, model_id: str) -> ProviderModel:
        """Get model by model ID.

        Args:
            model_id: Model ID to look up

        Returns:
            Model information

        Raises:
            ProviderError: If model is not found or operation fails
        """
        cache_key = f"model:{self._provider.provider_id}:{model_id.lower()}"

        # Try to get model from cache
        cached_model = await self.cache.cache_get(cache_key)
        if cached_model:
            return cast(ProviderModel, ProviderModel.model_validate(cached_model))

        # Get all models and find the requested one
        all_models = await self.get_models()
        model = next(
            (m for m in all_models if m.model_id.lower() == model_id.lower()),
            None,
        )

        if model is None:
            raise ProviderError(
                code=404,
                message=f"Model {model_id} not found",
                details={"model_id": model_id},
            )

        # Cache individual model
        await self.cache.cache_set(
            cache_key, model.model_dump(), expire=self.MODELS_CACHE_TTL
        )

        return model

    async def close(self) -> None:
        """Close HTTP client."""
        self.logger.debug("Closing Yandex HTTP client")
        await self._client.aclose()
        self.logger.info("Yandex HTTP client closed")
