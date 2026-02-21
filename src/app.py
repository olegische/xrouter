"""LLM Gateway FastAPI application."""
import json
from typing import Callable, Optional

from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from starlette.exceptions import HTTPException

from .api.middleware.auth import AuthMiddleware
from .api.middleware.error_handler import ErrorHandlerMiddleware
from .api.middleware.request_id import RequestIDMiddleware
from .api.routes.chat_completion import ChatCompletionRouter
from .api.routes.gigachat_completions import GigaChatCompletionsRouter
from .api.routes.health import HealthRouter
from .api.routes.info import InfoRouter
from .api.routes.models import ModelsRouter
from .api.routes.responses import ResponsesRouter


class LLMGatewayApp(FastAPI):
    """LLM Gateway FastAPI application."""

    def __init__(
        self,
        lifespan: Optional[Callable] = None,
    ) -> None:
        """Initialize LLM Gateway application.

        Args:
            lifespan: Application lifespan manager
        """
        self._configured = False
        # Initialize FastAPI with custom settings
        super().__init__(
            title="XRouter",
            description="""
            # OpenAI-Compatible API Router

            LLM Gateway provides a unified API interface for accessing various LLM providers
            while maintaining OpenAI API compatibility.
            """,
            version="0.1.0",  # Will be updated in configure()
            docs_url="/docs",
            redoc_url="/redoc",
            lifespan=lifespan,
        )

        # Dependencies will be set later
        self.state.logger = None
        self.state.settings = None
        self.state.user_auth_service = None
        self.state.chat_context_service = None
        self.state.provider_manager = None
        self.state.handler_chain_factory = None
        self.state.mapper_factory = None
        self.state.provider_factory = None
        self.state.usage_client = None

    def configure(self) -> None:
        """Configure middleware and routes after dependencies are set."""
        if self._configured:
            raise RuntimeError("Application is already configured")

        # Update version from settings
        self.version = self.state.settings.VERSION

        if not all(
            [
                self.state.logger,
                self.state.settings,
                self.state.user_auth_service,
                self.state.chat_context_service,
                self.state.provider_manager,
                self.state.handler_chain_factory,
                self.state.mapper_factory,
                self.state.provider_factory,
                self.state.usage_client,
            ]
        ):
            raise RuntimeError("Dependencies must be set before configuring the app.")

        app_logger = self.state.logger.get_logger(__name__)
        logger = self.state.logger
        settings = self.state.settings
        user_auth_service = self.state.user_auth_service
        chat_context_service = self.state.chat_context_service
        provider_manager = self.state.provider_manager
        usage_client = self.state.usage_client

        # Configure CORS middleware
        app_logger.info(
            "Configuring CORS middleware",
            extra={"allowed_origins": settings.BACKEND_CORS_ORIGINS},
        )
        self.add_middleware(
            CORSMiddleware,
            allow_origins=settings.BACKEND_CORS_ORIGINS,
            allow_credentials=True,
            allow_methods=["*"],
            allow_headers=["*"],
        )

        # Add middleware in correct order
        app_logger.info("Adding ErrorHandlerMiddleware")
        self.add_middleware(ErrorHandlerMiddleware, logger=logger, settings=settings)

        app_logger.info("Adding AuthMiddleware")
        self.add_middleware(
            AuthMiddleware,
            logger=logger,
            settings=settings,
            user_auth_service=user_auth_service,
        )

        app_logger.info("Adding RequestIDMiddleware")
        self.add_middleware(RequestIDMiddleware, logger=logger, settings=settings)

        # Add routers
        health_router = HealthRouter(logger=logger)
        self.include_router(health_router.router)

        if settings.ENABLE_SERVER_INFO_ENDPOINT:
            app_logger.info("Registering InfoRouter")
            info_router = InfoRouter(
                logger=logger,
                settings=settings,
                provider_manager=provider_manager,
            )
            self.include_router(info_router.router)

        app_logger.info("Registering ChatCompletionRouter")
        chat_completion_router = ChatCompletionRouter(
            logger=logger,
            provider_manager=provider_manager,
            context_service=chat_context_service,
            settings=settings,
        )
        self.include_router(chat_completion_router.router)

        app_logger.info("Registering ResponsesRouter")
        responses_router = ResponsesRouter(
            logger=logger,
            provider_manager=provider_manager,
            context_service=chat_context_service,
            settings=settings,
        )
        self.include_router(responses_router.router)

        app_logger.info("Registering GigaChatCompletionsRouter")
        gigachat_router = GigaChatCompletionsRouter(
            logger=logger,
            provider_manager=provider_manager,
            context_service=chat_context_service,
            settings=settings,
        )
        self.include_router(gigachat_router.router)

        app_logger.info("Registering ModelsRouter")
        models_router = ModelsRouter(
            logger=logger,
            provider_manager=provider_manager,
            settings=settings,
            usage_client=usage_client,
        )
        self.include_router(models_router.router)

        # Add global exception handlers
        app_logger.info("Registering global exception handlers")
        self.add_exception_handler(HTTPException, self._http_exception_handler)
        self.add_exception_handler(
            RequestValidationError, self._validation_exception_handler
        )

        app_logger.info(
            "LLM Gateway application configuration completed successfully",
            extra={
                "middleware_count": len(self.user_middleware),
                "router_count": len(self.router.routes),
            },
        )

        self._configured = True

    async def _http_exception_handler(
        self, request: Request, exc: HTTPException
    ) -> JSONResponse:  # type: ignore
        """Handle HTTP exceptions.

        Args:
            request: FastAPI request
            exc: HTTP exception

        Returns:
            JSON response with error details
        """
        self.state.logger.get_logger(__name__).error(
            "HTTP error occurred",
            extra={
                "request_id": request.state.request_id,
                "path": request.url.path,
                "method": request.method,
                "status_code": exc.status_code,
                "detail": exc.detail,
                "client_host": request.client.host if request.client else None,
                "client_port": request.client.port if request.client else None,
            },
            exc_info=True,
        )
        return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail})

    async def _validation_exception_handler(
        self, request: Request, exc: RequestValidationError
    ) -> JSONResponse:  # type: ignore
        """Handle validation exceptions.

        Args:
            request: FastAPI request
            exc: Validation exception

        Returns:
            JSON response with validation errors
        """
        self.state.logger.get_logger(__name__).error(
            "Validation error",
            extra={
                "request_id": request.state.request_id,
                "path": request.url.path,
                "method": request.method,
                "errors": exc.errors(),
                "client_host": request.client.host if request.client else None,
                "client_port": request.client.port if request.client else None,
                "body": (
                    (await request.body()).decode()
                    if request.method in ["POST", "PUT", "PATCH"]
                    else None
                ),
            },
            exc_info=True,
        )
        return JSONResponse(status_code=422, content={"detail": [{"loc": error["loc"], "msg": error["msg"], "type": error["type"]} for error in exc.errors()]})
