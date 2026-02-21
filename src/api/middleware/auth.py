"""Authentication middleware."""
import re
from typing import Optional, Tuple

from fastapi import Request
from fastapi.responses import JSONResponse
from starlette.types import ASGIApp, Receive, Scope, Send

from auth.user_auth_service import UserAuthService
from core.config import Settings
from core.logger import LoggerService
from providers.models import ProviderError


class AuthMiddleware:
    """Middleware for Bearer token authentication.

    This middleware:
    1. Extracts Bearer token from Authorization header
    2. Validates token format
    3. Determines token type (admin or user)
    4. Validates token with appropriate auth service
       (AdminAuthService or UserAuthService)
    5. Stores token and type in request.state for use in handlers
    6. Skips auth for non-API routes (/docs, /redoc, /health)
    """

    # Routes that don't require authentication
    PUBLIC_ROUTES = {
        "/docs",
        "/redoc",
        "/openapi.json",
        "/health",
        "/api/v1/models",  # GET /models is public
        "/v1/models",  # GET /models is public
        "/api/v1/info/json",
        "/info/table",
    }

    # Routes that only require service authentication
    SERVICE_AUTH_ONLY_ROUTES = {
        "/api/v1/service",
    }

    # Bearer token regex
    TOKEN_PATTERN = re.compile(r"^Bearer\s+([a-zA-Z0-9-]+)$")

    def __init__(
        self,
        app: ASGIApp,
        logger: LoggerService,
        settings: Settings,
        user_auth_service: UserAuthService,
    ) -> None:
        """Initialize middleware.

        Args:
            app: ASGI application
            logger: Logger service
            settings: Settings instance
            user_auth_service: Service for user API key validation
        """
        self.app = app
        self.logger = logger.get_logger(__name__)
        self.settings = settings
        self.user_auth_service = user_auth_service

    def _validate_basic_key_format(self, api_key: str) -> None:
        """Validate basic key format requirements.

        Args:
            api_key: API key to validate

        Raises:
            ProviderError: If key format is invalid
        """
        if not api_key:
            raise ProviderError(
                code=401,
                message="Invalid key format",
                details={"error": "Key cannot be empty"},
            )

        if " " in api_key or "\t" in api_key or "\n" in api_key:
            raise ProviderError(
                code=401,
                message="Invalid key format",
                details={"error": "Key contains invalid characters"},
            )

    def _get_token(
        self, request: Request, scope: Scope, header_name: str = "Authorization"
    ) -> Optional[str]:
        """Extract Bearer token from request.

        Args:
            request: FastAPI request
            scope: ASGI scope
            header_name: Name of the header to extract token from

        Returns:
            Token if present and valid, None otherwise

        Raises:
            ProviderError: If token format is invalid
        """
        # Get Authorization header
        auth_header = request.headers.get(header_name)
        self.logger.debug(
            "Processing Authorization header",
            extra={
                "request_id": scope.get("state", {}).get("request_id"),
                "path": request.url.path,
                "method": request.method,
                "public_routes": list(self.PUBLIC_ROUTES),
            },
        )

        if not auth_header:
            self.logger.debug(
                "No Authorization header",
                extra={
                    "request_id": scope.get("state", {}).get("request_id"),
                    "path": request.url.path,
                },
            )
            return None

        # Validate token format
        match = self.TOKEN_PATTERN.match(auth_header)
        if not match:
            self.logger.warning(
                "Invalid token format",
                extra={
                    "request_id": scope.get("state", {}).get("request_id"),
                    "path": request.url.path,
                    "pattern": self.TOKEN_PATTERN.pattern,
                },
            )
            raise ProviderError(
                code=401,
                message="Authentication required",
                details={"error": "Bearer token required"},
            )

        token = match.group(1)

        # Validate basic key format
        self._validate_basic_key_format(token)

        self.logger.debug(
            "Token extracted",
            extra={
                "request_id": scope.get("state", {}).get("request_id"),
                "path": request.url.path,
                "token_length": len(token),
            },
        )
        return token

    async def _validate_service_auth(
        self, request: Request, scope: Scope, receive: Receive, send: Send
    ) -> Optional[str]:
        """Validate service authentication.

        Args:
            request: FastAPI request
            scope: ASGI scope
            receive: ASGI receive function
            send: ASGI send function

        Returns:
            Service token if auth is valid, None if service auth is disabled

        Raises:
            ProviderError: If service auth is required but invalid
        """
        request_id = scope.get("state", {}).get("request_id")
        path = request.url.path

        if not self.settings.ENABLE_SERVICE_AUTH:
            return None

        if not self.settings.SERVICE_API_KEY:
            self.logger.error(
                "Service auth enabled but SERVICE_API_KEY not set",
                extra={
                    "request_id": request_id,
                    "path": path,
                },
            )
            raise ProviderError(
                code=500,
                message="Service authentication misconfigured",
                details={"error": "SERVICE_API_KEY not set"},
            )

        # For service-only routes, service token comes in Authorization header
        # For other routes requiring service auth, it comes in X-Service-Authorization
        is_service_only_route = any(
            path.startswith(route) for route in self.SERVICE_AUTH_ONLY_ROUTES
        )
        header_name = "Authorization" if is_service_only_route else "Authorization"

        service_token = self._get_token(request, scope, header_name)
        if not service_token:
            self.logger.warning(
                "Service auth enabled but no token provided",
                extra={
                    "request_id": request_id,
                    "path": path,
                    "header_name": header_name,
                },
            )
            raise ProviderError(
                code=401,
                message="Authentication required",
                details={"error": "Service API key required"},
            )

        if service_token != self.settings.SERVICE_API_KEY:
            self.logger.warning(
                "Invalid service API key",
                extra={
                    "request_id": request_id,
                    "path": path,
                    "header_name": header_name,
                },
            )
            raise ProviderError(
                code=401,
                message="Invalid service API key",
                details={"error": "Service API key validation failed"},
            )

        self.logger.info(
            "Service authentication successful",
            extra={
                "request_id": request_id,
                "path": path,
                "header_name": header_name,
            },
        )
        return service_token

    async def _validate_user_auth(
        self, request: Request, scope: Scope, receive: Receive, send: Send
    ) -> Tuple[str, str]:
        """Validate user authentication.

        Args:
            request: FastAPI request
            scope: ASGI scope
            receive: ASGI receive function
            send: ASGI send function

        Returns:
            Tuple of (token, key_type)

        Raises:
            ProviderError: If user auth is invalid
        """
        request_id = scope.get("state", {}).get("request_id")

        try:
            token = self._get_token(request, scope, "Authorization")
        except ProviderError as e:
            self.logger.error(
                "Missing or invalid Bearer token",
                extra={
                    "request_id": request_id,
                    "path": request.url.path,
                    "method": request.method,
                },
            )
            raise ProviderError(
                code=e.code,
                message=e.message,
                details=e.details,
            )

        if not token:
            self.logger.warning(
                "Missing authentication",
                extra={
                    "request_id": request_id,
                    "path": request.url.path,
                    "method": request.method,
                },
            )
            raise ProviderError(
                code=401,
                message="Authentication required",
                details={"error": "Bearer token required"},
            )

        # Use auth service for all user authentication regardless of issuer
        self.logger.debug(
            "Using auth service for user authentication",
            extra={
                "request_id": request_id,
                "path": request.url.path,
            },
        )

        # Validate token with auth service
        key_data = await self.user_auth_service.validate_api_key(token, request_id)
        key_type = key_data.type
        request.state.user_id = key_data.user_id

        self.logger.info(
            "User authentication successful",
            extra={
                "request_id": request_id,
                "path": request.url.path,
                "key_type": key_type,
            },
        )
        return token, key_type

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        """Process the request.

        Args:
            scope: ASGI scope
            receive: ASGI receive function
            send: ASGI send function
        """
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return

        request = Request(scope, receive)
        request_id = scope.get("state", {}).get("request_id")
        path = request.url.path

        try:
            # Log request start
            self.logger.info(
                "Processing authentication",
                extra={
                    "request_id": request_id,
                    "path": path,
                    "method": request.method,
                },
            )

            # Skip auth if disabled or for public routes
            if not self.settings.ENABLE_AUTH or path in self.PUBLIC_ROUTES:
                self.logger.debug(
                    "Skipping auth - disabled or public route",
                    extra={
                        "path": path,
                        "auth_enabled": self.settings.ENABLE_AUTH,
                        "public_routes": list(self.PUBLIC_ROUTES),
                    },
                )
                # Set anonymous user ID when auth is disabled or for public routes
                request.state.user_id = "anonymous-user"
                self.logger.debug(
                    "Setting anonymous user ID for disabled auth or public route",
                    extra={
                        "request_id": request_id,
                        "path": path,
                        "user_id": "anonymous-user",
                        "auth_enabled": self.settings.ENABLE_AUTH,
                        "is_public_route": path in self.PUBLIC_ROUTES,
                    },
                )
                await self.app(scope, receive, send)
                return

            # Step 1: Service Authentication
            try:
                service_token = await self._validate_service_auth(
                    request, scope, receive, send
                )
                # Only proceed with service auth if we got a valid token
                if service_token:
                    request.state.api_key = service_token
                    request.state.api_key_type = "service"
                    # Set service user ID for service authentication
                    request.state.user_id = "service-user"
                    self.logger.debug(
                        "Setting service user ID for service authentication",
                        extra={
                            "request_id": request_id,
                            "path": path,
                            "user_id": "service-user",
                        },
                    )
                    await self.app(scope, receive, send)
                    return
            except ProviderError:
                # If service auth fails, try user auth
                pass

            # Step 2: User Authentication
            try:
                token, key_type = await self._validate_user_auth(
                    request, scope, receive, send
                )
                # Store user token and type in request state
                request.state.api_key = token
                request.state.api_key_type = key_type
            except ProviderError as e:
                response = JSONResponse(
                    status_code=e.code,
                    content={
                        "error": {
                            "code": e.code,
                            "message": e.message,
                            "details": e.details,
                        }
                    },
                )
                await response(scope, receive, send)
                return

            # Process request
            await self.app(scope, receive, send)

        except Exception as e:
            # Log unexpected errors
            self.logger.error(
                "Authentication error",
                extra={
                    "request_id": request_id,
                    "path": path,
                    "method": request.method,
                    "error": str(e),
                },
                exc_info=True,
            )
            response = JSONResponse(
                status_code=500,
                content={
                    "error": {
                        "code": 500,
                        "message": "Authentication failed",
                        "details": {"error": str(e)},
                    }
                },
            )
            await response(scope, receive, send)
            return
