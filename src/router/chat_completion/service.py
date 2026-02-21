"""Chat completion service implementation."""
from typing import AsyncGenerator, Union

from core.logger import LoggerService
from providers.base import Provider
from providers.models import ProviderError
from .handler_chain import RequestHandlerChain
from .models import (
    ChatContext,
    OpenAIResponse,
    OpenAIStreamChunk,
    LLMGatewayResponse,
    LLMGatewayStreamChunk,
)


class ChatCompletionService:
    """Service for handling chat completion requests."""

    def __init__(
        self,
        provider: Provider,
        handler_chain: RequestHandlerChain,
        context: ChatContext,
        logger: LoggerService,
    ) -> None:
        """Initialize service.

        Args:
            provider: Provider instance for request
            handler_chain: Request handler chain
            context: Chat completion context
            logger: Logger service

        Raises:
            ValueError: If any required dependency is missing
        """
        self.logger = logger.get_logger(__name__)

        if not provider:
            self.logger.error("Provider not provided during service initialization")
            raise ValueError("Provider is required")
        if not handler_chain:
            self.logger.error(
                "Handler chain not provided during service initialization"
            )
            raise ValueError("Handler chain is required")
        if not context:
            self.logger.error("Context not provided during service initialization")
            raise ValueError("Context is required")
        self.logger.info(
            "Initializing ChatCompletionService",
            extra={
                "provider_type": type(provider).__name__,
                "handler_chain_type": type(handler_chain).__name__,
            },
        )

        self.provider = provider
        self.handler_chain = handler_chain
        self.context = context

    async def create_chat_completion(
        self,
    ) -> AsyncGenerator[
        Union[OpenAIResponse, LLMGatewayResponse, OpenAIStreamChunk, LLMGatewayStreamChunk],
        None,
    ]:
        """Handle chat completion request.

        The method handles both streaming and non-streaming requests based on
        context.request.stream parameter:
        - For stream=false: yields single ChatResponse
        - For stream=true: yields multiple StreamChunks

        Yields:
            Chat responses or stream chunks

        Raises:
            ProviderError: If request handling fails
        """
        if not self.context.request:
            raise ProviderError(
                code=500,
                message="Missing request in context",
                details={"error": "request is None"},
            )

        request = self.context.request
        try:
            self.logger.info(
                "Starting chat completion request",
                extra={
                    "request_id": self.context.request_id,
                    "model": request.model,
                    "stream": request.stream,
                    "message_count": len(request.messages) if request.messages else 0,
                    "provider": self.provider.__class__.__name__,
                },
            )

            # Execute handler chain
            self.logger.debug("Starting handler chain execution")
            async for result in self.handler_chain.handleRequest(
                self.context, self.provider
            ):
                yield result

            self.logger.info(
                "Successfully completed chat completion request",
                extra={
                    "request_id": self.context.request_id,
                    "model": request.model,
                    "provider": self.provider.__class__.__name__,
                    "stream": request.stream,
                    "generation_id": self.context.generation_id,
                    "has_final_response": bool(self.context.final_response),
                },
            )

        except ProviderError as e:
            self.logger.error(
                "Provider error handling chat completion request",
                extra={
                    "request_id": self.context.request_id,
                    "error": str(e),
                    "details": e.details,
                    "model": request.model,
                    "provider": self.provider.__class__.__name__,
                    "stream": request.stream,
                    "generation_id": self.context.generation_id,
                },
            )

            # Re-raise provider errors
            raise
        except Exception as e:
            self.logger.error(
                "Failed to handle chat completion request",
                extra={"request_id": self.context.request_id, "error": str(e)},
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message="Failed to handle chat completion request",
                details={"error": str(e)},
            )
        finally:
            # Close provider connection
            try:
                self.logger.debug(
                    "Closing provider connection",
                    extra={
                        "request_id": self.context.request_id,
                        "provider": self.provider.__class__.__name__,
                    },
                )
                await self.provider.close()
                self.logger.info(
                    "Provider connection closed",
                    extra={
                        "request_id": self.context.request_id,
                        "provider": self.provider.__class__.__name__,
                    },
                )
            except Exception as e:
                self.logger.error(
                    "Failed to close provider connection",
                    extra={
                        "request_id": self.context.request_id,
                        "provider": self.provider.__class__.__name__,
                        "error": str(e),
                    },
                    exc_info=True,
                )
