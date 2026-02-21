"""Dependency injection container."""
from typing import Optional

from dependency_injector import containers, providers
from redis.asyncio import Redis

from auth.user_auth_service import UserAuthService
from core.cache import RedisClient
from core.config import Settings
from core.logger import LoggerService
from providers.factory import ProviderFactory
from providers.manager import ProviderManager
from providers.mapper_factory import MapperFactory
from providers.model_mapper_factory import ModelMapperFactory
from router.chat_completion.context_service import ChatContextService
from router.chat_completion.handler_chain_factory import HandlerChainFactory
from router.chat_completion.service import ChatCompletionService
from router.usage.client import UsageClient


def create_redis(settings: Settings) -> Redis:
    """Create Redis connection.

    Args:
        settings: Application settings.

    Returns:
        Redis: Redis connection.
    """
    # Always use password if it's provided, since Redis may require auth
    # regardless of ENABLE_AUTH
    redis_params = {
        "encoding": "utf-8",
        "decode_responses": True,
    }

    return Redis.from_url(
        str(settings.REDIS_URL),
        **redis_params,
    )


def get_redis_connection(settings: Settings) -> Optional[Redis]:
    """Return Redis connection or None when ENABLE_CACHE is False (no connect)."""
    if not settings.ENABLE_CACHE:
        return None
    return create_redis(settings)


class Container(containers.DeclarativeContainer):
    """Main application container."""

    wiring_config = containers.WiringConfiguration()

    # Settings
    settings = providers.Singleton(Settings)

    # Core services
    logger = providers.Singleton(LoggerService, settings_instance=settings)

    # Redis connection (None when ENABLE_CACHE=False; RedisClient then runs in stub mode)
    redis_connection = providers.Singleton(
        get_redis_connection,
        settings=settings,
    )
    redis_client = providers.Singleton(
        RedisClient,
        redis=redis_connection,
        logger=logger,
        settings=settings,
    )

    # Auth providers
    user_auth_service = providers.Singleton(
        UserAuthService,
        logger=logger,
        settings_instance=settings,
        cache=redis_client,
    )

    # Chat context service
    chat_context_service = providers.Singleton(
        ChatContextService,
        logger=logger,
    )

    # Provider services
    mapper_factory = providers.Singleton(MapperFactory, logger=logger)
    model_mapper_factory = providers.Singleton(
        ModelMapperFactory, logger=logger, settings=settings
    )

    mapper = providers.Factory(
        lambda provider_config, factory=mapper_factory: (
            factory().create(
                provider=provider_config,
            )
        )
    )
    model_mapper = providers.Factory(
        lambda provider_config, factory=model_mapper_factory: (
            factory().create(
                provider=provider_config,
            )
        )
    )

    provider_factory = providers.Singleton(
        ProviderFactory,
        logger=logger,
        cache=redis_client,
    )

    provider_manager = providers.Singleton(
        ProviderManager,
        logger=logger,
        settings=settings,
        provider_factory=provider_factory,
        mapper_factory=mapper_factory,
        model_mapper_factory=model_mapper_factory,
        cache=redis_client,
    )

    provider = providers.Factory(
        lambda provider_config, mapper=mapper, model_mapper=model_mapper, factory=provider_factory: (
            factory().create(
                provider=provider_config,
                mapper=mapper,
                model_mapper=model_mapper,
            )
        )
    )

    # Usage/Billing service
    usage_client = providers.Singleton(
        UsageClient,
        settings=settings,
        logger=logger,
    )

    # Handler chain factory
    handler_chain_factory = providers.Singleton(
        HandlerChainFactory,
        logger=logger,
        provider_manager=provider_manager,
        settings=settings,
        usage_client=usage_client,
    )
    handler_chain = providers.Factory(
        lambda factory=handler_chain_factory: factory().create()
    )

    # Chat completion service provider
    chat_completion_service = providers.Factory(
        ChatCompletionService,
        provider=provider,
        handler_chain=handler_chain,
        logger=logger,
    )


# Создаем глобальный экземпляр контейнера
container = Container()
