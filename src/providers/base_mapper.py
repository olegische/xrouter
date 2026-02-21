"""Base mapper for provider request/response mapping."""
from abc import ABC, abstractmethod
from typing import Any, Dict, List, Optional, Union

from core.logger import LoggerService
from .models import (
    ProviderConfig,
    ProviderRequest,
    ProviderStreamChunk,
    ContentPart,
    TextContent,
)


class BaseMapper(ABC):
    """Base class for provider mappers."""

    def __init__(self, provider: ProviderConfig, logger: LoggerService) -> None:
        """Initialize mapper.

        Args:
            provider: Provider model instance
            logger: Logger service instance
        """
        self._provider = provider
        self.logger = logger.get_logger(__name__)

    @abstractmethod
    def map_to_provider_request(self, request: ProviderRequest) -> Dict[str, Any]:
        """Map provider-agnostic request to provider-specific format.

        Args:
            request: Provider-agnostic request

        Returns:
            Provider-specific request dictionary
        """
        pass

    @abstractmethod
    def map_provider_stream_chunk(
        self, chunk_data: Dict, model: str, provider_id: str
    ) -> ProviderStreamChunk:
        """Map provider stream chunk to provider-agnostic format.

        Args:
            chunk_data: Raw chunk data
            model: Model name
            provider_id: Provider identifier

        Returns:
            Provider-agnostic stream chunk
        """
        pass

    @abstractmethod
    def parse_sse_line(self, line: str) -> Optional[Dict[str, Any]]:
        """Parse SSE line into chunk data.

        Args:
            line: Raw SSE line

        Returns:
            Parsed chunk data or None if line should be skipped
        """
        pass

    def convert_content_to_string(
        self, role: str, content: Union[str, List[ContentPart], None]
    ) -> Optional[str]:
        """Convert content to string format required by providers.

        Args:
            role: Message role
            content: Message content which can be string or list of ContentPart

        Returns:
            Converted string content or None
        """
        if content is None:
            return None

        if isinstance(content, str):
            return content

        # Only convert content for user messages
        if role != "user":
            return str(content) if content is not None else None

        if isinstance(content, list):
            # Join only text type content parts with newlines
            text_parts = [
                part.text for part in content if isinstance(part, TextContent)
            ]
            return "\n".join(text_parts) if text_parts else ""

        return None
