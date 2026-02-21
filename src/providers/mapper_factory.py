"""Provider mapper factory."""
from core.logger import LoggerService
from .base_mapper import BaseMapper
from .deepseek.mapper import DeepseekMapper
from .gigachat.mapper import GigaChatMapper
from .models import ProviderConfig, ProviderError
from .openrouter.mapper import OpenRouterMapper
from .xrouter.mapper import XRouterMapper
from .yandex.mapper import YandexMapper
from .zai.mapper import ZaiMapper


class MapperFactory:
    """Factory for creating provider-specific mappers."""

    def __init__(self, logger: LoggerService) -> None:
        """Initialize mapper factory.

        Args:
            logger: Logger service instance
        """
        self.logger = logger.get_logger(__name__)
        self.instance_logger = logger

    def create(self, provider: ProviderConfig) -> BaseMapper:
        """Create or return existing mapper instance for provider.

        Args:
            provider: Provider model instance

        Returns:
            Provider-specific mapper instance

        Raises:
            ValueError: If provider is not supported
        """
        self.logger.info(f"Creating new mapper instance for: {provider.provider_id}")

        # Create new mapper instance
        mappers = {
            "agents": XRouterMapper,
            "xrouter": XRouterMapper,
            "deepseek": DeepseekMapper,
            "openrouter": OpenRouterMapper,
            "openrouter-proxy": OpenRouterMapper,
            "gigachat": GigaChatMapper,
            "yandex": YandexMapper,
            "ollama": XRouterMapper,
            "zai": ZaiMapper,
        }

        if provider.provider_id in mappers:
            mapper = mappers[provider.provider_id](
                provider=provider, logger=self.instance_logger
            )
            self.logger.info(f"Created new mapper instance for: {provider.provider_id}")
            return mapper

        error_msg = f"Unsupported provider mapper: {provider.provider_id}"
        self.logger.error(error_msg)
        raise ProviderError(
            code=400, message=error_msg, details={"provider_id": provider.provider_id}
        )
