"""Base classes for request handlers."""
from abc import ABC, abstractmethod
from typing import AsyncGenerator, Union

from providers.base import Provider
from ..models import ChatContext
from ..models.openai import (
    OpenAIResponse,
    OpenAIStreamChunk,
)
from ..models.llm_gateway import (
    LLMGatewayResponse,
    LLMGatewayStreamChunk,
)


class RequestHandler(ABC):
    """Base class for request handlers."""

    @abstractmethod
    def canHandle(self, context: ChatContext) -> bool:
        """Check if handler can process the request.

        Args:
            context: Chat context

        Returns:
            bool: True if handler can process the request
        """
        raise NotImplementedError

    @abstractmethod
    def handleRequest(
        self,
        context: ChatContext,
        provider: Provider,
    ) -> AsyncGenerator[
        Union[OpenAIResponse, OpenAIStreamChunk, LLMGatewayResponse, LLMGatewayStreamChunk],
        None,
    ]:
        """Handle request.

        Args:
            context: Request context
            provider: Provider instance for request

        Yields:
            Chat responses or stream chunks

        Raises:
            ProviderError: If request handling fails
        """
        raise NotImplementedError
