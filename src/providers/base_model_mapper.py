"""Base mapper for provider model mapping."""
from typing import Any, Dict, List

from core.logger import LoggerService
from core.settings import Settings
from .models import ProviderConfig, ProviderModel


class BaseModelMapper:
    """Base mapper class for provider model mapping."""

    def __init__(
        self, provider: ProviderConfig, logger: LoggerService, settings: Settings
    ) -> None:
        """Initialize mapper.

        Args:
            provider: Provider model instance
            logger: Logger service instance
            settings: Application settings instance
        """
        self._provider = provider
        self.logger = logger
        self.settings = settings

    def map_provider_models(self, models_data: Dict[str, Any]) -> List[ProviderModel]:
        """Map provider models response to provider-agnostic format.

        Args:
            models_data: Raw models response data

        Returns:
            List of provider models
        """
        raise NotImplementedError("Subclasses must implement map_provider_models")
