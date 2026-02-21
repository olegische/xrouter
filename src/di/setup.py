"""FastAPI dependency injection setup."""
from typing import Any, Callable, Dict, Optional, Type

from fastapi import FastAPI

from auth.user_auth_service import UserAuthService
from core.cache import RedisClient
from core.config import Settings
from providers.factory import ProviderFactory
from providers.manager import ProviderManager
from providers.mapper_factory import MapperFactory
from providers.model_mapper_factory import ModelMapperFactory
from router.chat_completion.context_service import ChatContextService
from router.chat_completion.handler_chain_factory import HandlerChainFactory
from router.chat_completion.service import ChatCompletionService

from .dependencies import container


def setup_di(app: FastAPI) -> None:
    """Setup dependency injection for FastAPI application.

    Args:
        app: FastAPI application instance

    Raises:
        RuntimeError: If DI configuration fails
    """
    logger = container.logger().get_logger(__name__)
    try:
        logger.info("Starting dependency injection configuration")

        # Настройка контейнера зависимостей
        logger.debug("Configuring dependency container...")
        container.wire(packages=["src"])
        logger.debug("Container wiring completed")

        # Регистрация зависимостей
        logger.debug("Registering dependencies...")
        dependencies = get_di_dependencies()
        for dependency_type, provider in dependencies.items():
            app.dependency_overrides[dependency_type] = provider
            logger.debug("Registered dependency: %s" % dependency_type.__name__)
        logger.info("Successfully registered dependencies")

        logger.info("Dependency injection configuration completed successfully")
    except Exception as e:
        logger.error("Failed to configure dependency injection: %s" % str(e))
        cleanup_di(app)
        raise RuntimeError("Dependency injection configuration failed") from e


def cleanup_di(app: Optional[FastAPI] = None) -> None:
    """Cleanup dependency injection resources.

    This function is safe to call multiple times and will not raise exceptions.
    """
    logger = container.logger().get_logger(__name__)
    try:
        logger.info("Starting dependency injection cleanup")

        # Shutdown and reset container
        if container:
            logger.debug("Shutting down container resources...")
            container.shutdown_resources()
            logger.debug("Container resources shutdown completed")

            logger.debug("Resetting container...")
            try:
                container.reset()
                logger.debug("Container reset completed")
            except AttributeError:
                logger.debug("Container reset skipped - container not initialized")

        # Clear FastAPI dependency overrides
        if app:
            logger.debug("Clearing FastAPI dependency overrides...")
            app.dependency_overrides.clear()
            logger.debug("Dependency overrides cleared")

        logger.info("Dependency injection cleanup completed successfully")
    except Exception as e:
        logger.error("Error during DI cleanup: %s" % str(e), exc_info=True)


def get_di_dependencies() -> Dict[Type[Any], Callable[[], Any]]:
    """Get all registered dependencies.

    This function returns a dictionary mapping dependency types to their provider
    functions. Each provider function is responsible for creating and returning
    an instance of its corresponding dependency type.

    Returns:
        Dict[Type[Any], Callable[[], Any]]: Dictionary mapping types to providers
    """
    return {
        # Core services
        Settings: container.settings.provider,
        RedisClient: container.redis_client.provider,
        # Provider services
        MapperFactory: container.mapper_factory.provider,
        ModelMapperFactory: container.model_mapper_factory.provider,
        ProviderFactory: container.provider_factory.provider,
        ProviderManager: container.provider_manager.provider,
        HandlerChainFactory: container.handler_chain_factory.provider,
        # Business services
        UserAuthService: container.user_auth_service.provider,
        ChatCompletionService: container.chat_completion_service.provider,
        ChatContextService: container.chat_context_service.provider,
    }
