"""Error handling middleware."""
from typing import Optional

from fastapi import Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from starlette.types import ASGIApp, Receive, Scope, Send

from core.logger import LoggerService
from core.settings import Settings
from providers.models import ProviderError


class ErrorResponse:
    """Error response format following API specification."""

    @staticmethod
    def create(
        code: int,
        message: str,
        details: Optional[dict] = None,
    ) -> dict:
        """Create error response.

        Args:
            code: HTTP status code
            message: Error message
            details: Optional error details

        Returns:
            Error response dictionary
        """
        return {
            "error": {
                "code": code,
                "message": message,
                "details": details or {},
            }
        }


class ErrorHandlerMiddleware:
    """Middleware for handling errors.

    This middleware catches all exceptions and formats them according to the API
    specification. It also logs errors.

    Error response format:
    {
        "error": {
            "code": number,
            "message": string,
            "details": {
                "provider_name"?: string,
                "raw"?: unknown,
                "reasons"?: string[],
                "flagged_input"?: string,
                "model_slug"?: string,
                "error"?: string
            }
        }
    }
    """

    def __init__(
        self,
        app: ASGIApp,
        logger: LoggerService,
        settings: Settings,
    ) -> None:
        """Initialize middleware.

        Args:
            app: ASGI application
            logger: Logger service
            settings: Application settings
        """
        self.app = app
        self.logger = logger.get_logger(__name__)
        self.settings = settings

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
        request_id = getattr(request.state, "request_id", None)

        try:
            # Log request start
            self.logger.debug(
                "Processing request",
                extra={
                    "request_id": request_id,
                    "path": request.url.path,
                    "method": request.method,
                },
            )

            # Process request
            await self.app(scope, receive, send)
            return

        except RequestValidationError as e:
            # Log validation error with full context
            self.logger.error(
                "Request validation error",
                extra={
                    "request_id": request_id,
                    "path": request.url.path,
                    "method": request.method,
                    "errors": e.errors(),
                },
            )

            # Return formatted error response
            response = JSONResponse(
                status_code=422,
                content=ErrorResponse.create(
                    code=422,
                    message="Request validation error",
                    details={"errors": e.errors()},
                ),
            )
            await response(scope, receive, send)
            return

        except ProviderError as e:
            # Log provider error with full context
            self.logger.error(
                "Provider error",
                extra={
                    "request_id": request_id,
                    "path": request.url.path,
                    "method": request.method,
                    "provider_name": e.details.get("provider_name"),
                    "model_slug": e.details.get("model_slug"),
                    "error_code": e.code,
                    "error_message": e.message,
                    "error_details": e.details,
                },
            )

            # Return formatted error response
            response = JSONResponse(
                status_code=e.code,
                content=ErrorResponse.create(
                    code=e.code,
                    message=e.message,
                    details=e.details,
                ),
            )
            await response(scope, receive, send)
            return

        except Exception as e:
            # Log unexpected error with full context
            self.logger.error(
                "Unexpected error",
                extra={
                    "request_id": request_id,
                    "path": request.url.path,
                    "method": request.method,
                    "error_type": type(e).__name__,
                    "error_message": str(e),
                },
                exc_info=True,
            )

            # Return 500 error
            response = JSONResponse(
                status_code=500,
                content=ErrorResponse.create(
                    code=500,
                    message="Internal server error",
                    details={"error": str(e)},
                ),
            )
            await response(scope, receive, send)
            return
