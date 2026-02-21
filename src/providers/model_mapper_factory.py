"""Provider model mapper factory."""
from core.logger import LoggerService
from core.settings import Settings
from .base_model_mapper import BaseModelMapper
from .deepseek.model_mapper import DeepseekModelMapper
from .gigachat.model_mapper import GigaChatModelMapper
from .models import ProviderConfig, ProviderError
from .ollama.model_mapper import OllamaModelMapper
from .openrouter.model_mapper import OpenRouterModelMapper
from .openrouter_proxy.model_mapper import OpenRouterProxyModelMapper
from .xrouter.model_mapper import XRouterModelMapper
from .agents.model_mapper import AgentsModelMapper
from .yandex.model_mapper import YandexModelMapper
from .zai.model_mapper import ZaiModelMapper


class ModelMapperFactory:
    """Factory for creating provider-specific model mappers."""

    def __init__(self, logger: LoggerService, settings: Settings) -> None:
        """Initialize model mapper factory.

        Args:
            logger: Logger service instance
            settings: Application settings instance
        """
        self.logger = logger.get_logger(__name__)
        self.instance_logger = logger
        self.settings = settings

    def create(self, provider: ProviderConfig) -> BaseModelMapper:
        """Create or return existing model mapper instance for provider.

        Args:
            provider: Provider model instance

        Returns:
            Provider-specific model mapper instance

        Raises:
            ValueError: If provider is not supported
        """
        self.logger.info(
            f"Creating new model mapper instance for: {provider.provider_id}"
        )

        # Create new mapper instance
        mappers = {
            "agents": AgentsModelMapper,
            "xrouter": XRouterModelMapper,
            "deepseek": DeepseekModelMapper,
            "openrouter": OpenRouterModelMapper,
            "openrouter-proxy": OpenRouterProxyModelMapper,
            "gigachat": GigaChatModelMapper,
            "yandex": YandexModelMapper,
            "ollama": OllamaModelMapper,
            "zai": ZaiModelMapper,
        }

        if provider.provider_id in mappers:
            mapper = mappers[provider.provider_id](
                provider=provider, logger=self.instance_logger, settings=self.settings
            )
            self.logger.info(
                f"Created new model mapper instance for: {provider.provider_id}"
            )
            return mapper

        error_msg = f"Unsupported provider model mapper: {provider.provider_id}"
        self.logger.error(error_msg)
        raise ProviderError(
            code=400, message=error_msg, details={"provider_id": provider.provider_id}
        )
