"""XRouterAuth API client."""
from typing import Any, Optional, cast

import httpx
from pydantic import ValidationError

from .models import IntrospectRequest, IntrospectResponse
from core.logger import LoggerService
from core.settings import Settings
from providers.models import ProviderError


class XAuthClient:
    """Client for XAuth API."""

    def __init__(self, settings: Settings, logger: LoggerService) -> None:
        """Initialize client.

        Args:
            settings: Application settings
            logger: Logger service
        """
        self.settings = settings
        self.base_url = settings.AUTH_SERVICE_URL
        self.api_key = settings.AUTH_SERVICE_API_KEY
        self.logger = logger.get_logger(__name__)
        self.client = self._create_client()

    def _create_client(self) -> httpx.AsyncClient:
        """Create and configure HTTP client.

        Returns:
            Configured HTTP client
        """
        return httpx.AsyncClient(
            base_url=self.base_url,
            headers={
                "Authorization": f"Bearer {self.api_key}",
                "Content-Type": "application/json",
            },
            timeout=30.0,
            verify=True,  # Enforce SSL verification
        )

    async def _make_request(
        self, method: str, endpoint: str, json_data: Optional[dict[str, Any]] = None
    ) -> dict[str, Any]:
        """Make HTTP request to API.

        Args:
            method: HTTP method
            endpoint: API endpoint
            json_data: JSON data to send

        Returns:
            Response data

        Raises:
            ProviderError: If request fails
        """
        try:
            response = await self.client.request(
                method=method,
                url=endpoint,
                json=json_data,
            )
            response.raise_for_status()
            return cast(dict[str, Any], response.json())
        except httpx.HTTPError as e:
            error_msg = str(e)
            try:
                error_data = response.json()
                if "error" in error_data:
                    error_msg = error_data["error"]
            except Exception:
                pass

            status_code = response.status_code if "response" in locals() else 500
            details = {
                "endpoint": endpoint,
                "method": method,
                "response": response if "response" in locals() else None,
            }
            if json_data:
                details["request_data"] = json_data

            raise ProviderError(
                code=status_code,
                message=f"XRouterAuth API request failed: {error_msg}",
                details=details,
            )

    async def introspect(self, token: str) -> IntrospectResponse:
        """Validate token.

        Args:
            token: Token to validate

        Returns:
            Token validation response

        Raises:
            ProviderError: If validation fails
        """
        try:
            request = IntrospectRequest(token=token, token_type_hint="api_key")
            response_data = await self._make_request(
                "POST", "/introspect", request.model_dump(by_alias=True)
            )
            return cast(
                IntrospectResponse,
                IntrospectResponse.model_validate(response_data),
            )
        except ValidationError as e:
            raise ProviderError(
                code=400,
                message="Invalid token validation response format",
                details={"validation_errors": str(e)},
            )
        except ProviderError as e:
            new_error = ProviderError(
                code=e.code,
                message=f"Token validation failed: {e.message}",
                details={"token": token, **e.details},
            )
            raise new_error from e

    async def __aenter__(self) -> "XAuthClient":
        """Enter async context manager."""
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Exit async context manager."""
        await self.client.aclose()
