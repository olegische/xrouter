"""Service for managing chat completion context."""
from typing import Optional

from core.logger import LoggerService
from .models import ChatContext


class ChatContextService:
    """Service for managing chat completion context."""

    def __init__(self, logger: LoggerService) -> None:
        """Initialize chat context service.

        Args:
            logger: Logger service
        """
        self.logger = logger.get_logger(__name__)

    async def cleanup_context(self, context: Optional[ChatContext]) -> None:
        """Clean up context resources.

        This method should be called when the request handling is complete
        or when the client disconnects. It ensures proper cleanup of resources
        and references to prevent memory leaks.

        Args:
            context: Chat context to clean up
        """
        if not context:
            return

        # Log cleanup
        self.logger.debug(
            "Cleaning up chat context",
            extra={"request_id": context.metadata.get("request_id")},
        )

        # Clear references to objects that are no longer needed
        # Keep api_key, origin, request_id for logging/auditing
        context.request = None  # Reset request to None
        context.provider_model = None
        context.generation_id = None
        context.native_usage = None
        context.expected_tokens = None
        context.on_hold = None
        context.currency = None
        context.metadata = {}
        context.provider_request = None
        context.final_response = None
        context.accumulated_response = None
