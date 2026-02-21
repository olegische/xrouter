"""Request handler chain implementation."""
from typing import AsyncGenerator, Union

from core.logger import LoggerService
from providers.base import Provider
from .handlers.base import RequestHandler
from .models import (
    ChatContext,
    OpenAIResponse,
    OpenAIStreamChunk,
    LLMGatewayResponse,
    LLMGatewayStreamChunk,
)


class RequestHandlerChain:
    """Chain of request handlers."""

    def __init__(self, handlers: list[RequestHandler], logger: LoggerService) -> None:
        """Initialize handler chain.

        Args:
            handlers: List of request handlers
            logger: Logger service for chain operations

        Raises:
            ValueError: If logger is not provided
        """
        if not logger:
            raise ValueError("Logger service is required")

        self.handlers = handlers
        self.logger = logger.get_logger(__name__)

        self.logger.debug(
            "Initialized RequestHandlerChain",
            extra={
                "handler_count": len(handlers),
                "handler_types": [h.__class__.__name__ for h in handlers],
            },
        )

    async def handleRequest(
        self, context: ChatContext, provider: Provider
    ) -> AsyncGenerator[
        Union[OpenAIResponse, LLMGatewayResponse, OpenAIStreamChunk, LLMGatewayStreamChunk],
        None,
    ]:
        """Handle request through the chain of handlers.

        Args:
            context: Request context

        Yields:
            Chat responses or stream chunks

        Raises:
            ProviderError: If request handling fails
        """
        model_id = (
            context.provider_model.model_id if context.provider_model else "unknown"
        )
        self.logger.info(
            "Starting request handling chain",
            extra={
                "request_id": context.request_id,
                "model": model_id,
                "handler_count": len(self.handlers),
            },
        )

        for idx, handler in enumerate(self.handlers, 1):
            handler_name = handler.__class__.__name__

            if handler.canHandle(context):
                self.logger.debug(
                    "Executing handler",
                    extra={
                        "request_id": context.request_id,
                        "handler": handler_name,
                        "position": idx,
                        "total": len(self.handlers),
                    },
                )

                try:
                    async for result in handler.handleRequest(context, provider):
                        self.logger.debug(
                            "Handler yielded result",
                            extra={
                                "request_id": context.request_id,
                                "handler": handler_name,
                                "result_type": type(result).__name__,
                            },
                        )
                        yield result
                    self.logger.info(
                        "Handler completed",
                        extra={
                            "request_id": context.request_id,
                            "handler": handler_name,
                            "has_final_response": bool(context.final_response),
                            "generation_id": context.generation_id,
                            "has_on_hold": bool(
                                getattr(context, "on_hold", None) is not None
                            ),
                        },
                    )
                except Exception as e:
                    self.logger.error(
                        "Handler failed",
                        extra={
                            "request_id": context.request_id,
                            "handler": handler_name,
                            "error": str(e),
                            "error_type": type(e).__name__,
                        },
                    )
                    raise
            else:
                self.logger.debug(
                    "Handler skipped - cannot handle request",
                    extra={"request_id": context.request_id, "handler": handler_name},
                )

        self.logger.info(
            "Completed request handling chain",
            extra={
                "request_id": context.request_id,
                "generation_id": context.generation_id,
                "has_final_response": bool(context.final_response),
            },
        )
