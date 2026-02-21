"""GigaChat provider implementation."""
import uuid
from datetime import datetime, timedelta
from typing import AsyncGenerator, Dict, List, Optional, cast

from httpx import AsyncClient, HTTPError

from core.cache import RedisClient
from core.logger import LoggerService
from ..base import Provider
from .mapper import GigaChatMapper
from .model_mapper import GigaChatModelMapper
from .models import GigaChatToken
from ..models import (
    ProviderConfig,
    ProviderError,
    ProviderModel,
    ProviderRequest,
    ProviderStreamChunk,
)


class GigaChatProvider(Provider):
    """GigaChat API provider implementation."""

    MODELS_CACHE_TTL = 86400  # 24 hours

    def __init__(
        self,
        cache: RedisClient,
        logger: LoggerService,
        provider: ProviderConfig,
        mapper: GigaChatMapper,
        model_mapper: GigaChatModelMapper,
    ):
        """Initialize GigaChat provider.

        Args:
            cache: Redis cache client
            logger: Logger service instance
            provider: Provider config
            mapper: GigaChat mapper instance
            model_mapper: GigaChat model mapper instance
        """
        super().__init__(provider=provider)
        self.cache = cache
        self.logger = logger.get_logger(__name__)
        self.mapper = mapper
        self.model_mapper = model_mapper
        self._access_token: Optional[str] = None
        self._token_expires_at: Optional[datetime] = None
        self._client = AsyncClient(
            verify=False,  # GigaChat requires SSL verification disabled
            timeout=300.0,  # 5 min timeout for streaming
        )
        self.logger.info(
            "Initialized GigaChatProvider",
            extra={
                "provider_id": self._provider.provider_id,
                "verify_ssl": False,
                "timeout": 300.0,
                "client_id": id(self._client),
                "mapper_type": type(mapper).__name__,
            },
        )

    async def create_completion(  # noqa: C901
        self, request: ProviderRequest
    ) -> AsyncGenerator[ProviderStreamChunk, None]:
        """Create completion using GigaChat API.

        Args:
            request: Provider-agnostic request

        Returns:
            AsyncGenerator that yields StreamChunks

        Raises:
            ProviderError: If request fails
        """
        await self._ensure_token()

        try:
            # Map request to GigaChat format
            gigachat_request = self.mapper.map_to_provider_request(request)
            self.logger.info(
                "Mapped request",
                extra={
                    "request_id": request.request_id,
                    "has_messages": bool(gigachat_request.get("messages")),
                    "has_functions": bool(gigachat_request.get("functions")),
                    "stream": gigachat_request.get("stream", False),
                },
            )
            self.logger.debug(
                "Request details",
                extra={
                    "request": gigachat_request,
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
                headers=self._get_request_headers(request.request_id),
                json=gigachat_request,
            ) as response:
                self.logger.info(
                    "Got stream response, checking status",
                    extra={"request_id": request.request_id},
                )

                # Read error response before raising status
                if response.status_code >= 400:
                    error_text = await response.aread()
                    self.logger.error(
                        "Error response from GigaChat API",
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
                        if not (parsed_line := self.mapper.parse_sse_line(line)):
                            self.logger.debug(
                                "Skipping non-data line",
                                extra={"request_id": request.request_id},
                            )
                            continue

                        if parsed_line.get("event") == "done":
                            # Received 'data: [DONE]' from Gigachat
                            self.logger.info(
                                "Received [DONE] from Gigachat",
                                extra={"request_id": request.request_id},
                            )
                            break  # Exit the stream processing loop

                        # Add request_id to chunk data and map to provider format
                        chunk_data = parsed_line
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
                        yield mapped_chunk

                    # Дожидаемся, пока Gigachat закроет соединение
                    await response.aclose()

                except Exception as e:
                    self.logger.error(
                        "Error in stream processing",
                        extra={"error": str(e), "request_id": request.request_id},
                        exc_info=True,
                    )
                    raise

        except HTTPError as e:
            self.logger.error(
                "HTTP error in stream request",
                extra={
                    "error": str(e),
                    "error_type": type(e).__name__,
                    "status_code": e.response.status_code
                    if hasattr(e, "response")
                    else None,
                    "response_text": e.response.text
                    if hasattr(e, "response")
                    else None,
                    "request_url": e.request.url if hasattr(e, "request") else None,
                    "request_id": request.request_id,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=e.response.status_code if hasattr(e, "response") else 500,
                message=f"GigaChat API error: {str(e)}",
                details={"error": str(e)},
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

    def _get_request_headers(self, request_id: str) -> Dict[str, str]:
        """Get headers for API request.

        Args:
            request_id: Request ID for tracing

        Returns:
            Dict with request headers
        """
        return {
            "Authorization": f"Bearer {self._access_token}",
            "Content-Type": "application/json",
            "X-Request-ID": request_id,
        }

    async def _ensure_token(self) -> None:
        """Ensure we have a valid access token."""
        request_id = str(uuid.uuid4())
        self.logger.debug(
            "Checking token validity",
            extra={
                "request_id": request_id,
                "has_token": bool(self._access_token),
                "token_expires_at": self._token_expires_at.isoformat()
                if self._token_expires_at
                else None,
                "current_time": datetime.utcnow().isoformat(),
                "provider_id": self._provider.provider_id,
            },
        )

        if not self._is_token_valid():
            self.logger.info(
                "Token invalid or expired, refreshing",
                extra={
                    "request_id": request_id,
                    "token_length": len(self._access_token)
                    if self._access_token
                    else 0,
                    "time_until_expiry": (
                        self._token_expires_at - datetime.utcnow()
                    ).total_seconds()
                    if self._token_expires_at
                    else None,
                },
            )
            await self._refresh_token()
        else:
            self.logger.debug(
                "Token is valid",
                extra={
                    "request_id": request_id,
                    "expires_in": (
                        self._token_expires_at - datetime.utcnow()
                    ).total_seconds()
                    if self._token_expires_at
                    else None,
                },
            )

    def _is_token_valid(self) -> bool:
        """Check if current token is valid."""
        request_id = str(uuid.uuid4())
        current_time = datetime.utcnow()
        buffer_minutes = 5

        if not self._access_token or not self._token_expires_at:
            self.logger.debug(
                "No token or expiration time set",
                extra={
                    "request_id": request_id,
                    "has_token": bool(self._access_token),
                    "has_expiry": bool(self._token_expires_at),
                    "provider_id": self._provider.provider_id,
                },
            )
            return False

        # Add buffer for token expiration
        buffer_time = timedelta(minutes=buffer_minutes)
        expiry_with_buffer = self._token_expires_at - buffer_time
        is_valid = current_time < expiry_with_buffer

        if not is_valid:
            self.logger.debug(
                "Token expired",
                extra={
                    "request_id": request_id,
                    "expires_at": self._token_expires_at.isoformat(),
                    "current_time": current_time.isoformat(),
                    "buffer_minutes": buffer_minutes,
                    "time_until_expiry": (
                        self._token_expires_at - current_time
                    ).total_seconds(),
                    "time_until_buffer": (
                        expiry_with_buffer - current_time
                    ).total_seconds(),
                },
            )
        else:
            self.logger.debug(
                "Token is still valid",
                extra={
                    "request_id": request_id,
                    "expires_in": (
                        self._token_expires_at - current_time
                    ).total_seconds(),
                    "buffer_expires_in": (
                        expiry_with_buffer - current_time
                    ).total_seconds(),
                },
            )

        return is_valid

    async def _refresh_token(self) -> None:
        """Get new access token from OAuth endpoint."""
        request_id = str(uuid.uuid4())
        oauth_url = "https://ngw.devices.sberbank.ru:9443/api/v2/oauth"
        token_url = f"{self._provider.base_url}/token"

        self.logger.info(
            "Starting token refresh",
            extra={
                "request_id": request_id,
                "oauth_url": oauth_url,
                "token_url": token_url,
                "current_token_valid": self._is_token_valid(),
                "current_token_expires_at": self._token_expires_at.isoformat()
                if self._token_expires_at
                else None,
            },
        )

        try:
            if not self._provider.credentials:
                raise ValueError("No GigaChat credentials provided.")

            # Check if credentials are in login:password format
            if ":" in self._provider.credentials:
                self.logger.info(
                    "Refreshing token using login/password",
                    extra={"request_id": request_id},
                )
                login, password = self._provider.credentials.split(":", 1)
                response = await self._client.post(
                    token_url,
                    auth=(login, password),
                    headers={"RqUID": request_id},
                )
            else:  # Assume it's a service account key
                self.logger.info(
                    "Refreshing token using service account credentials",
                    extra={"request_id": request_id},
                )
                response = await self._client.post(
                    oauth_url,
                    headers={
                        "Authorization": f"Basic {self._provider.credentials}",
                        "RqUID": request_id,
                        "Content-Type": "application/x-www-form-urlencoded",
                    },
                    data={"scope": "GIGACHAT_API_PERS"},
                )

            response.raise_for_status()

            data = response.json()
            token = GigaChatToken(**data)
            self._access_token = token.access_token
            # Convert milliseconds to seconds for datetime
            self._token_expires_at = datetime.fromtimestamp(token.expires_at / 1000)

            self.logger.info(
                "Refreshed GigaChat access token",
                extra={
                    "request_id": request_id,
                    "expires_at": self._token_expires_at.isoformat(),
                    "token_length": len(self._access_token),
                },
            )
        except HTTPError as e:
            self.logger.error(
                "Failed to refresh GigaChat token - HTTP error",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                    "error_type": type(e).__name__,
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
                message="Failed to refresh GigaChat token",
                details={"error": str(e)},
            )
        except Exception as e:
            self.logger.error(
                "Failed to refresh GigaChat token - unexpected error",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                    "error_type": type(e).__name__,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message="Failed to refresh GigaChat token",
                details={"error": str(e)},
            )

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

        # If not in cache, fetch from API
        await self._ensure_token()

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
                headers=self._get_request_headers(request_id),
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

    async def close(self) -> None:
        """Close HTTP client."""
        self.logger.debug("Closing GigaChat HTTP client")
        await self._client.aclose()
        self.logger.info("GigaChat HTTP client closed")
