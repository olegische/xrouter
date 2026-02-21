"""Middleware for adding request ID and logging requests/responses."""
import time
from uuid import uuid4

from fastapi import Request
from starlette.types import ASGIApp, Receive, Scope, Send

from core.config import Settings
from core.logger import LoggerService


class RequestIDMiddleware:
    """Middleware for adding request ID and logging requests/responses."""

    # Порог для медленных запросов в секундах
    SLOW_REQUEST_THRESHOLD = 1.0

    def __init__(
        self,
        app: ASGIApp,
        logger: LoggerService,
        settings: Settings,
    ) -> None:
        """Initialize middleware.

        Args:
            app: ASGI application
            logger: Logger service for request/response logging
            settings: Settings instance
        """
        self.app = app
        self.logger = logger.get_logger(__name__)
        self.settings = settings

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        """Process the request.

        Args:
            scope: ASGI scope
            receive: ASGI receive function
            send: ASGI send function
        """
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return

        request = Request(scope, receive)

        # Check for existing X-Request-ID header
        request_id = request.headers.get("X-Request-ID")
        if not request_id:
            request_id = str(uuid4())

        request.state.request_id = request_id

        # Детальное логирование запроса
        content_length = request.headers.get("content-length", "0")
        content_type = request.headers.get("content-type", "unknown")

        self.logger.debug(
            "Request details",
            extra={
                "request_id": request.state.request_id,
                "method": request.method,
                "path": request.url.path,
                "content_length": content_length,
                "content_type": content_type,
                "user_agent": request.headers.get("user-agent"),
            },
        )

        # Основное логирование запроса
        self.logger.info(
            "Incoming request",
            extra={
                "request_id": request.state.request_id,
                "method": request.method,
                "path": request.url.path,
                "query_params": dict(request.query_params),
                "client": request.client.host if request.client else None,
            },
        )

        # Обработка запроса с отслеживанием времени
        start_time = time.time()

        # Модифицируем send для добавления request_id в заголовки ответа
        async def send_with_request_id(message: dict) -> None:
            if message["type"] == "http.response.start":
                headers = message.get("headers", [])
                headers.append((b"X-Request-ID", request.state.request_id.encode()))
                message["headers"] = headers

            await send(message)

        try:
            # Обработка запроса
            await self.app(scope, receive, send_with_request_id)

            # Вычисляем время обработки
            process_time = time.time() - start_time

            # Логирование для медленных запросов
            if process_time > self.SLOW_REQUEST_THRESHOLD:
                self.logger.warning(
                    "Slow request detected",
                    extra={
                        "request_id": request.state.request_id,
                        "method": request.method,
                        "path": request.url.path,
                        "process_time_seconds": process_time,
                    },
                )

            # Основное логирование успешного ответа
            self.logger.info(
                "Request completed",
                extra={
                    "request_id": request.state.request_id,
                    "method": request.method,
                    "path": request.url.path,
                    "process_time_seconds": process_time,
                    "client": request.client.host if request.client else None,
                },
            )

        except Exception as e:
            # Вычисляем время до ошибки
            process_time = time.time() - start_time

            # Логируем ошибку
            self.logger.error(
                "Request failed",
                extra={
                    "request_id": request.state.request_id,
                    "method": request.method,
                    "path": request.url.path,
                    "error": str(e),
                    "error_type": type(e).__name__,
                    "process_time_seconds": process_time,
                },
                exc_info=True,
            )

            raise
