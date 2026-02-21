"""Provider factory implementation."""
from typing import Tuple, Type, cast

from core.cache import RedisClient
from core.logger import LoggerService
from .base import Provider
from .base_mapper import BaseMapper
from .base_model_mapper import BaseModelMapper
from .deepseek.mapper import DeepseekMapper
from .deepseek.model_mapper import DeepseekModelMapper
from .gigachat import GigaChatProvider
from .gigachat.mapper import GigaChatMapper
from .gigachat.model_mapper import GigaChatModelMapper
from .models import ProviderConfig, ProviderError
from .ollama import OllamaModelMapper, OllamaProvider
from .openrouter import OpenRouterModelMapper
from .openrouter.mapper import OpenRouterMapper
from .openrouter_proxy.model_mapper import OpenRouterProxyModelMapper
from .openrouter_proxy.provider import OpenRouterProxyRouterProvider
from .xrouter.mapper import XRouterMapper
from .xrouter.model_mapper import XRouterModelMapper
from .xrouter.provider import XRouterProvider
from .agents.model_mapper import AgentsModelMapper
from .agents.provider import AgentsProvider
from .yandex.mapper import YandexMapper
from .yandex.model_mapper import YandexModelMapper
from .yandex.provider import YandexProvider
from .zai.mapper import ZaiMapper
from .zai.model_mapper import ZaiModelMapper
from .zai.provider import ZaiProvider


class ProviderFactory:
    """Factory for creating provider instances."""

    def __init__(
        self,
        logger: LoggerService,
        cache: RedisClient,
    ):
        """Initialize provider factory.

        Args:
            cache: Redis cache client
            logger: Logger service instance
        """
        self.cache = cache
        self.logger = logger.get_logger(__name__)
        self.instance_logger = logger

    def create(
        self,
        provider: ProviderConfig,
        mapper: BaseMapper,
        model_mapper: BaseModelMapper,
    ) -> Provider:
        """Create provider instance.

        Args:
            provider: Provider model instance
            mapper: Provider-specific mapper instance
            model_mapper: Provider-specific model mapper instance

        Returns:
            Provider instance

        Raises:
            ValueError: If provider is not supported
        """
        self.logger.info(f"Creating new provider instance for: {provider.provider_id}")

        providers: dict[
            str, Tuple[Type[Provider], Type[BaseMapper], Type[BaseModelMapper]]
        ] = {
            "agents": (AgentsProvider, XRouterMapper, AgentsModelMapper),
            "xrouter": (XRouterProvider, XRouterMapper, XRouterModelMapper),
            "deepseek": (XRouterProvider, DeepseekMapper, DeepseekModelMapper),
            "openrouter": (XRouterProvider, OpenRouterMapper, OpenRouterModelMapper),
            "openrouter-proxy": (
                OpenRouterProxyRouterProvider,
                OpenRouterMapper,
                OpenRouterProxyModelMapper,
            ),
            "gigachat": (GigaChatProvider, GigaChatMapper, GigaChatModelMapper),
            "yandex": (YandexProvider, YandexMapper, YandexModelMapper),
            "ollama": (OllamaProvider, XRouterMapper, OllamaModelMapper),
            "zai": (ZaiProvider, ZaiMapper, ZaiModelMapper),
        }

        if provider.provider_id in providers:
            provider_class, mapper_class, model_mapper_class = providers[
                provider.provider_id
            ]

            if not isinstance(mapper, mapper_class):
                error_msg = (
                    f"Invalid mapper type for {provider.provider_id} "
                    f"provider: {type(mapper)}"
                )
                self.logger.error(error_msg)
                raise ProviderError(
                    code=400,
                    message=error_msg,
                    details={
                        "provider_id": provider.provider_id,
                        "mapper_type": type(mapper).__name__,
                    },
                )

            if not isinstance(model_mapper, model_mapper_class):
                error_msg = (
                    f"Invalid model mapper type for {provider.provider_id} "
                    f"provider: {type(model_mapper)}"
                )
                self.logger.error(error_msg)
                raise ProviderError(
                    code=400,
                    message=error_msg,
                    details={
                        "provider_id": provider.provider_id,
                        "model_mapper_type": type(model_mapper).__name__,
                    },
                )

            provider_instance = provider_class(
                logger=self.instance_logger,
                cache=self.cache,
                provider=provider,
                mapper=mapper,
                model_mapper=model_mapper,
            )
            self.logger.info(
                f"Successfully created provider instance: {provider.provider_id}"
            )
            return cast(Provider, provider_instance)

        error_msg = f"Unsupported provider: {provider.provider_id}"
        self.logger.error(error_msg)
        raise ProviderError(
            code=400, message=error_msg, details={"provider_id": provider.provider_id}
        )
