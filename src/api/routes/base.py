"""Base router implementation."""
from abc import ABC, abstractmethod
from typing import List

from fastapi import APIRouter

from core.logger import LoggerService


class BaseRouter(ABC):
    """Base router class that all routers should inherit from."""

    def __init__(
        self,
        logger: LoggerService,
        prefix: str = "",
        tags: List[str] | None = None,
    ):
        """Initialize router.

        Args:
            logger: Logger service instance
            prefix: URL prefix for all routes
            tags: OpenAPI tags for documentation
        """
        if not logger:
            raise ValueError("Logger service is required")

        self.logger = logger.get_logger(__name__)
        self.router = APIRouter(prefix=prefix, tags=tags or [])
        self._setup_routes()

    @abstractmethod
    def _setup_routes(self) -> None:
        """Setup router endpoints.

        This method should be implemented by concrete router classes
        to define their specific routes and handlers.
        """
        pass

    @property
    def routes(self) -> APIRouter:
        """Get router instance.

        Returns:
            FastAPI router instance
        """
        return self.router
