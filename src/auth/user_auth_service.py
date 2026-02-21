"""User authorization service."""
import hashlib
from datetime import datetime
from typing import Optional, cast

from httpx import HTTPError

from .xauth import XAuthClient
from core.cache import RedisClient
from core.config import Settings
from core.logger import LoggerService
from providers.models import ProviderError
from router.base.apikey.models import APIKey, APIKeyStatus, APIKeyType


class UserAuthService:
    """Service for user API key authorization."""

    def __init__(
        self,
        logger: LoggerService,
        settings_instance: Settings,
        cache: RedisClient,
    ) -> None:
        """Initialize service.

        Args:
            logger: Logger service
            settings_instance: Settings instance
            cache: Redis cache client
        """
        self.logger = logger.get_logger(__name__)
        self.logger_instance = logger
        self.settings = settings_instance
        self.cache = cache

        self.logger.debug(
            "Initializing UserAuthService",
            extra={
                "auth_service_url": self.settings.AUTH_SERVICE_URL,
            },
        )
        self._client: Optional[XAuthClient] = None

    def _hash_key(self, api_key: str) -> str:
        """Hash API key for cache storage.

        Args:
            api_key: API key to hash

        Returns:
            str: Hashed key
        """
        return hashlib.sha256(
            f"{api_key}{self.settings.API_KEY_SALT}".encode()
        ).hexdigest()

    async def validate_api_key(
        self,
        api_key: str,
        request_id: Optional[str] = None,
    ) -> APIKey:
        """Validate API key using external auth service.

        Args:
            api_key: API key to validate
            request_id: Optional request ID for logging

        Returns:
            API key record

        Raises:
            ProviderError: If key is invalid or service is unavailable
        """
        try:
            # Check cache first using hashed key
            key_hash = self._hash_key(api_key)
            cache_key = f"auth_service:key:{key_hash}"
            cached_data = await self.cache.cache_get(cache_key)

            if cached_data:
                self.logger.debug(
                    "Using cached auth service response",
                    extra={"request_id": request_id},
                )
                return cast(APIKey, APIKey.model_validate(cached_data))

            # Create and use client for validation
            if not self._client:
                self._client = XAuthClient(self.settings, self.logger_instance)
            assert self._client is not None  # for mypy
            introspect_response = await self._client.introspect(api_key)

            # Convert introspect response to APIKey model
            now = datetime.now()
            created_at = (
                datetime.fromtimestamp(float(introspect_response.iat))
                if introspect_response.active and introspect_response.iat is not None
                else now
            )
            expires_at = (
                datetime.fromtimestamp(float(introspect_response.exp))
                if introspect_response.active and introspect_response.exp is not None
                else now
            )
            api_key_data = APIKey(
                key_hash=key_hash,
                created_at=created_at,
                expires_at=expires_at,
                status=APIKeyStatus.ACTIVE
                if introspect_response.active
                else APIKeyStatus.INACTIVE,
                type=APIKeyType.USER,
                user_id=introspect_response.sub or "unknown",
            )

            if not introspect_response.active:
                raise ProviderError(
                    code=401,
                    message="Invalid API key",
                    details={"error": "Key validation failed"},
                )

            # Convert datetime objects to timestamps for JSON serialization
            cache_data = api_key_data.model_dump()
            if api_key_data.created_at:
                cache_data["created_at"] = int(api_key_data.created_at.timestamp())
            if api_key_data.expires_at:
                cache_data["expires_at"] = int(api_key_data.expires_at.timestamp())

            # Cache the response using hashed key
            await self.cache.cache_set(
                cache_key,
                cache_data,
                expire=self.settings.AUTH_SERVICE_CACHE_TTL,
            )

            self.logger.info(
                "API key validated successfully",
                extra={
                    "request_id": request_id,
                    "key_hash": api_key_data.key_hash,
                },
            )

            return api_key_data

        except HTTPError as e:
            self.logger.error(
                "Auth service HTTP error",
                extra={
                    "error": str(e),
                    "status_code": e.response.status_code
                    if hasattr(e, "response")
                    else None,
                    "request_id": request_id,
                },
            )
            if hasattr(e, "response") and e.response.status_code in (401, 403):
                raise ProviderError(
                    code=e.response.status_code,
                    message="Invalid API key",
                    details={"error": str(e)},
                )
            raise ProviderError(
                code=503,
                message="Auth service unavailable",
                details={"error": str(e)},
            )
        except Exception as e:
            self.logger.error(
                "Auth service error",
                extra={"error": str(e), "request_id": request_id},
            )
            raise ProviderError(
                code=401,
                message="Invalid API key",
                details={"error": "Authentication failed"},
            )

    async def close(self) -> None:
        """Close HTTP client."""
        if self._client:
            self.logger.debug("Closing auth service HTTP client")
            await self._client.client.aclose()
            self._client = None
            self.logger.info("Auth service HTTP client closed")
