"""Base provider interface."""
from abc import ABC, abstractmethod
from typing import AsyncGenerator, List

from .models import (
    ProviderConfig,
    ProviderModel,
    ProviderRequest,
    ProviderStreamChunk,
)


class Provider(ABC):
    """Base class for all providers.

    This class defines the contract that all providers must implement.
    Each provider is responsible for:
    - Creating completions (sync and streaming)
    - Token counting
    - Error handling
    - Provider-specific request mapping
    """

    def __init__(self, provider: ProviderConfig) -> None:
        """Initialize provider.

        Args:
            provider: Provider model instance
        """
        self._provider = provider

    @abstractmethod
    def create_completion(
        self, request: ProviderRequest
    ) -> AsyncGenerator[ProviderStreamChunk, None]:
        """Create completion using provider API.

        Args:
            request: Provider-agnostic request that will be mapped to provider format

        Returns:
            AsyncGenerator that yields:
            - Multiple StreamChunks for streaming requests

        The method always uses SSE under the hood

        Raises:
            ProviderError: If request fails
        """
        raise NotImplementedError

    @abstractmethod
    async def get_models(self) -> List[ProviderModel]:
        """Get list of available models.

        Returns:
            List of available provider models

        Raises:
            ProviderError: If models retrieval fails
        """
        raise NotImplementedError

    @abstractmethod
    async def get_model(self, model_id: str) -> ProviderModel:
        """Get model by model ID.

        Args:
            model_id: Model ID to look up

        Returns:
            Model information

        Raises:
            ProviderError: If model is not found or operation fails
        """
        raise NotImplementedError

    @abstractmethod
    async def close(self) -> None:
        """Close provider and cleanup resources.

        This method should be called when the provider is no longer needed
        to properly close any open connections or cleanup resources.
        """
        raise NotImplementedError
