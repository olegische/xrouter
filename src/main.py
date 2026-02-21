"""LLM Gateway FastAPI application entry point."""
from contextlib import asynccontextmanager
from typing import AsyncGenerator

import uvicorn
from fastapi import FastAPI

from .app import LLMGatewayApp
from .core.settings import settings
from .di import container
from .di.setup import cleanup_di, setup_di


@asynccontextmanager
async def lifespan(app: LLMGatewayApp) -> AsyncGenerator[None, None]:
    """Manage application lifespan.

    This function handles startup and shutdown events for the application.

    Args:
        app: FastAPI application instance

    Yields:
        None
    """
    app_logger = app.state.logger.get_logger(__name__)
    app_logger.info("Application configured successfully")

    # Initialize Redis (no-op when ENABLE_CACHE=False)
    if app.state.settings.ENABLE_CACHE:
        try:
            await app.state.redis_client.ping()
            app_logger.info("Redis client initialized successfully")
        except Exception as e:
            app_logger.error(
                "Failed to connect to Redis",
                extra={"error": str(e)},
            )
    else:
        app_logger.info("Redis disabled (ENABLE_CACHE=False), cache/rate-limit in stub mode")

    try:
        yield
    finally:
        app_logger.info("Shutting down application")

        # Закрытие Redis соединения
        await app.state.redis_client.close()  # type: ignore[attr-defined]
        app_logger.info("Redis client closed")

        # Cleanup DI container
        cleanup_di(app)


def init_app() -> FastAPI:
    """Initialize FastAPI application."""
    # Create the app with lifespan
    app = LLMGatewayApp(lifespan=lifespan)

    # Set up dependencies
    setup_di(app)

    # Retrieve dependencies from the container
    app.state.logger = container.logger()
    app.state.settings = container.settings()
    app.state.user_auth_service = container.user_auth_service()
    app.state.chat_context_service = container.chat_context_service()
    app.state.provider_manager = container.provider_manager()
    app.state.handler_chain_factory = container.handler_chain_factory()
    app.state.mapper_factory = container.mapper_factory()
    app.state.model_mapper_factory = container.model_mapper_factory()
    app.state.provider_factory = container.provider_factory()
    app.state.usage_client = container.usage_client()

    app.state.redis_client = container.redis_client()

    # Configure the application
    app.configure()

    return app


# Initialize application at the module level for uvicorn
def get_app() -> FastAPI:
    """Factory function to create the FastAPI app."""
    return init_app()


if __name__ == "__main__":
    app = init_app()

    # Get host and port from environment variables
    host = settings.HOST
    port = int(settings.PORT)

    # Run server
    uvicorn.run(
        app,
        host=host,
        port=port,
        reload=True,
    )
