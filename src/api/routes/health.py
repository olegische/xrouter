"""Health check router implementation."""
from typing import Dict

from fastapi import Request

from .base import BaseRouter
from core.logger import LoggerService


class HealthRouter(BaseRouter):
    """Health check router implementation."""

    def __init__(self, logger: LoggerService):
        """Initialize router.

        Args:
            logger: Logger service instance
        """
        if not logger:
            raise ValueError("Logger service is required")

        super().__init__(logger=logger, tags=["health"])
        self.logger = logger.get_logger(__name__)
        self._setup_routes()

    def _setup_routes(self) -> None:
        """Setup router endpoints."""
        self.router.add_api_route(
            "/health",
            self.health_check,
            methods=["GET"],
            response_model=Dict[str, str],
            summary="Health Check",
            description="Health check endpoint to verify the service is running.",
            operation_id="get_health_status_v1",
            responses={
                200: {
                    "description": "Service is healthy",
                    "content": {"application/json": {"example": {"status": "healthy"}}},
                }
            },
        )

    async def health_check(self, request: Request) -> Dict[str, str]:
        """Health check endpoint to verify the service is running.

        Args:
            request: FastAPI request object.

        Returns:
            A dictionary containing the health status.
        """
        self.logger.debug(
            "Health check requested",
            extra={
                "request_id": getattr(request.state, "request_id", None),
                "client": request.client.host if request.client else None,
                "headers": dict(request.headers),
            },
        )
        return {"status": "healthy"}
