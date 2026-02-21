"""LLM Gateway provider implementation."""
import asyncio
import uuid
from typing import AsyncGenerator, Dict, List, cast
from urllib.parse import urljoin

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


class AgentsProvider(Provider):
    """LLM Gateway Inference API provider implementation."""

    MODELS_CACHE_TTL = 86400  # 24 hours
    XSERVE_BASE_URL = (
        "http://xrouter-serve:8300/api/v1"  # Base URL for xrouter-serve service
    )

    def __init__(
        self,
        cache: RedisClient,
        logger: LoggerService,
        provider: ProviderConfig,
        mapper: BaseMapper,
        model_mapper: BaseModelMapper,
    ):
        """Initialize LLM Gateway Inference provider.

        Args:
            cache: Redis cache client
            logger: Logger service instance
            provider: Provider config
            mapper: Provider-specific mapper instance
            model_mapper: Provider-specific model mapper instance
        """
        super().__init__(provider=provider)
        self.cache = cache
        self.logger = logger.get_logger(__name__)
        self.mapper = mapper
        self.model_mapper = model_mapper
        self._client = AsyncClient(
            timeout=300.0,  # 5 min timeout for streaming
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
        """Check if chunk has any finish reason.

        Args:
            chunk: Stream chunk to check

        Returns:
            bool: True if chunk has any finish reason
        """
        return any(choice.finish_reason is not None for choice in chunk.choices)

    def _get_request_headers(self) -> Dict[str, str]:
        """Get headers for API request."""
        headers = {"Content-Type": "application/json"}

        # Only add Authorization header if credentials are not empty
        if self._provider.credentials and self._provider.credentials.strip():
            headers["Authorization"] = f"Bearer {self._provider.credentials}"

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

        # Call serve endpoint to start VM - non-blocking
        async def call_serve() -> None:
            try:
                self.logger.info(
                    "Calling serve endpoint",
                    extra={
                        "request_id": request_id,
                        "url": f"{self.XSERVE_BASE_URL}/serve",
                    },
                )
                response = await self._client.post(f"{self.XSERVE_BASE_URL}/serve")
                self.logger.info(
                    "Serve endpoint called successfully",
                    extra={
                        "request_id": request_id,
                        "status_code": response.status_code,
                    },
                )
            except Exception as e:
                self.logger.error(
                    "Error calling serve endpoint",
                    extra={
                        "request_id": request_id,
                        "error": str(e),
                    },
                )

        # Create task and don't wait for it
        asyncio.create_task(call_serve())

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

        # Since models are hardcoded in model_mapper, we use it directly
        # with empty dict as models_data since it's not used
        models = self.model_mapper.map_provider_models({})

        # Cache models for 24 hours
        await self.cache.cache_set(
            cache_key,
            [m.model_dump() for m in models],
            expire=self.MODELS_CACHE_TTL,
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

    def _is_docker_ip(self, ip: str) -> bool:
        """Check if IP is a Docker/internal network IP.

        Args:
            ip: IP address to check

        Returns:
            bool: True if IP is a Docker/internal network IP
        """
        # Docker/internal network IP ranges
        docker_ranges = [
            "10.",  # Class A private network
            "172.16.",
            "172.17.",
            "172.18.",
            "172.19.",
            "172.20.",
            "172.21.",
            "172.22.",  # Class B private network
            "192.168.",  # Class C private network
        ]
        return any(ip.startswith(prefix) for prefix in docker_ranges)

    async def _get_new_base_url_from_serve(self, request_id: str) -> str:
        """Get new base URL from serve endpoint response.

        This method calls the serve endpoint to get VM status and determines
        which IP to use based on whether we're in an internal or external network.

        Args:
            request_id: Request ID for logging

        Returns:
            str: New base URL to use for API requests:
                - If configured IP is Docker/internal: use internal IP from serve
                - If configured IP is external: use external IP from serve
                - If any error occurs: use original base URL
        """
        try:
            self.logger.info(
                "Calling serve endpoint",
                extra={
                    "request_id": request_id,
                    "url": f"{self.XSERVE_BASE_URL}/serve",
                },
            )
            serve_response = await self._client.post(f"{self.XSERVE_BASE_URL}/serve")
            serve_response.raise_for_status()
            vm_status = serve_response.json()

            # Extract IPs from response
            network_info = vm_status.get("network", {})
            internal_ip = network_info.get("ip")
            external_ip = network_info.get("external_ip")

            if not internal_ip:
                self.logger.warning(
                    "No internal IP in serve response, using original base URL",
                    extra={
                        "request_id": request_id,
                        "vm_status": vm_status,
                    },
                )
                return self._provider.base_url

            # Parse base_url to get configured IP
            configured_ip = self._provider.base_url.split("//")[-1].split(":")[0]
            base_url_parts = self._provider.base_url.split("//")
            ip_port = base_url_parts[-1].split(":")

            # Determine which IP to use based on configured IP type
            if self._is_docker_ip(configured_ip):
                # Using Docker/internal network
                self.logger.info(
                    "Using internal IP (Docker network detected)",
                    extra={
                        "request_id": request_id,
                        "internal_ip": internal_ip,
                        "configured_ip": configured_ip,
                    },
                )
                new_ip = internal_ip
            else:
                # Using external network
                if not external_ip:
                    self.logger.warning(
                        "No external IP in serve response, using original base URL",
                        extra={
                            "request_id": request_id,
                            "vm_status": vm_status,
                        },
                    )
                    return self._provider.base_url

                self.logger.info(
                    "Using external IP (external network detected)",
                    extra={
                        "request_id": request_id,
                        "external_ip": external_ip,
                        "configured_ip": configured_ip,
                    },
                )
                new_ip = external_ip

            # Create new base URL with selected IP
            new_base_url = (
                f"{base_url_parts[0]}//{new_ip}:{ip_port[1]}"
                if len(ip_port) > 1
                else f"{base_url_parts[0]}//{new_ip}"
            )
            return new_base_url

        except Exception as e:
            self.logger.error(
                "Error calling serve endpoint, using original base URL",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                },
            )
            return self._provider.base_url  # Return original URL - error occurred

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
            # Get new base URL from serve endpoint
            new_base_url = await self._get_new_base_url_from_serve(request.request_id)

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
                    "base_url": new_base_url,
                },
            )
            api_url = urljoin(new_base_url, "/v1/chat/completions")
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
                        "Error response from LLM Gateway Inference API",
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
                message=f"LLM Gateway API error: {str(e)}",
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
        self.logger.debug("Closing LLM Gateway Inference HTTP client")
        await self._client.aclose()
        self.logger.info("LLM Gateway Inference HTTP client closed")
