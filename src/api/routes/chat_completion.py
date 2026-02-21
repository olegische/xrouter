"""Chat completion router implementation."""
import json
from typing import AsyncGenerator, Union

from fastapi import Request
from fastapi.responses import JSONResponse, StreamingResponse
from starlette.status import HTTP_200_OK

from ..docs import (
    CHAT_COMPLETION_DESCRIPTION,
    CHAT_COMPLETION_OPERATION_ID,
    CHAT_COMPLETION_RESPONSES,
    CHAT_COMPLETION_SUMMARY,
    CHAT_COMPLETION_TAGS,
)
from .base import BaseRouter
from core.config import Settings
from core.logger import LoggerService
from di import container
from providers.manager import ProviderManager
from providers.models import ProviderError
from router.chat_completion.context_service import ChatContextService
from router.chat_completion.models import ChatContext
from router.chat_completion.models.openai import (
    OpenAIRequest,
    OpenAIResponse,
    OpenAIStreamChunk,
)
from router.chat_completion.models.llm_gateway import (
    LLMGatewayRequest,
    LLMGatewayResponse,
    LLMGatewayStreamChunk,
)
from router.chat_completion.service import ChatCompletionService


class ChatCompletionRouter(BaseRouter):
    """Chat completion router implementation."""

    def __init__(
        self,
        logger: LoggerService,
        provider_manager: ProviderManager,
        context_service: ChatContextService,
        settings: Settings,
    ) -> None:
        """Initialize router.

        Args:
            logger: Logger service instance
            provider_manager: Provider manager instance
            context_service: Chat context service instance
            settings: Settings instance

        Raises:
            ValueError: If any required dependency is missing
        """
        if not provider_manager:
            raise ValueError("Provider manager is required")

        # Set up instance attributes before calling super().__init__
        self.logger = logger.get_logger(__name__)
        self.instance_logger = logger
        self.provider_manager = provider_manager
        self.context_service = context_service
        self.settings = settings

        # Initialize with empty prefix - will be set in _setup_routes
        super().__init__(logger=logger, prefix="", tags=["chat"])

    async def _create_openai_chat_completion(
        self,
        chat_request: OpenAIRequest,
        fastapi_request: Request,
    ) -> Union[JSONResponse, StreamingResponse]:
        """Create a chat completion using OpenAI format."""
        return await self._handle_chat_completion(chat_request, fastapi_request)

    async def _create_LLMGateway_chat_completion(
        self,
        chat_request: LLMGatewayRequest,
        fastapi_request: Request,
    ) -> Union[JSONResponse, StreamingResponse]:
        """Create a chat completion using LLM Gateway format."""
        return await self._handle_chat_completion(chat_request, fastapi_request)

    def _setup_routes(self) -> None:
        """Setup router endpoints based on API type."""
        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            # OpenAI-compatible route
            self.router.add_api_route(
                "/v1/chat/completions",  # OpenAI-style path
                self._create_openai_chat_completion,
                methods=["POST"],
                response_model=OpenAIResponse,
                responses=CHAT_COMPLETION_RESPONSES,
                summary=CHAT_COMPLETION_SUMMARY,
                description=CHAT_COMPLETION_DESCRIPTION,
                operation_id=CHAT_COMPLETION_OPERATION_ID,
                tags=CHAT_COMPLETION_TAGS,
            )
        else:
            # LLMGateway-compatible route
            self.router.add_api_route(
                "/api/v1/chat/completions",  # LLMGateway-style path
                self._create_LLMGateway_chat_completion,
                methods=["POST"],
                response_model=LLMGatewayResponse,
                responses=CHAT_COMPLETION_RESPONSES,
                summary=CHAT_COMPLETION_SUMMARY,
                description=CHAT_COMPLETION_DESCRIPTION,
                operation_id=CHAT_COMPLETION_OPERATION_ID,
                tags=CHAT_COMPLETION_TAGS,
            )

    async def _handle_stream_response(
        self,
        service: ChatCompletionService,
        chat_context: ChatContext,
        request_id: str,
    ) -> AsyncGenerator[bytes, None]:
        """Handle streaming response from service.

        Args:
            service: Chat completion service
            chat_context: Request context
            request_id: Request ID for tracing

        Yields:
            Streamed response chunks

        Raises:
            ProviderError: If streaming fails
        """
        try:
            async for chunk in service.create_chat_completion():
                # Convert chunk to SSE format based on API type
                if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                    if isinstance(chunk, OpenAIStreamChunk):
                        chunk_json = chunk.model_dump_json()
                        yield f"data: {chunk_json}\n\n".encode("utf-8")
                else:
                    if isinstance(chunk, LLMGatewayStreamChunk):
                        chunk_json = chunk.model_dump_json()
                        yield f"data: {chunk_json}\n\n".encode("utf-8")
            # Send 'data: [DONE]' after stream completes
            yield b"data: [DONE]\n\n"
        except ProviderError as e:
            # For provider errors like insufficient funds, send error as SSE
            error_response = {
                "error": {
                    "message": str(e),
                    "type": "provider_error",
                    "code": e.code,
                    "details": e.details,
                }
            }
            yield f"data: {json.dumps(error_response)}\n\n".encode("utf-8")
            yield b"data: [DONE]\n\n"
            self.logger.error(
                "Provider error during streaming",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                    "code": e.code,
                    "details": e.details,
                },
                exc_info=True,
            )
        except Exception as e:
            # For unexpected errors, send generic error as SSE
            error_response = {
                "error": {
                    "message": "Internal server error",
                    "type": "internal_error",
                    "code": 500,
                }
            }
            yield f"data: {json.dumps(error_response)}\n\n".encode("utf-8")
            yield b"data: [DONE]\n\n"
            self.logger.error(
                "Unexpected error during streaming",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                },
                exc_info=True,
            )
        finally:
            # Cleanup context after stream is complete
            try:
                await self.context_service.cleanup_context(chat_context)
            except Exception as cleanup_error:
                self.logger.error(
                    "Error cleaning up context after streaming",
                    extra={
                        "request_id": request_id,
                        "error": str(cleanup_error),
                    },
                    exc_info=True,
                )

    async def _handle_chat_completion(  # noqa: C901
        self,
        chat_request: Union[OpenAIRequest, LLMGatewayRequest],
        fastapi_request: Request,
    ) -> Union[JSONResponse, StreamingResponse]:
        """Create a chat completion.

        Args:
            chat_request: Validated chat completion request
            fastapi_request: FastAPI request with auth data

        Returns:
            Chat completion response or streaming response

        Raises:
            ProviderError: If request processing fails
        """
        # Log API type and request format info at the start
        api_type = (
            "OpenAI compatible"
            if self.settings.ENABLE_OPENAI_COMPATIBLE_API
            else "LLMGateway"
        )
        request_format = (
            "OpenAI" if isinstance(chat_request, OpenAIRequest) else "LLMGateway"
        )
        self.logger.info(
            "Processing chat completion request",
            extra={"api_type": api_type, "request_format": request_format},
        )

        # Validate request type based on API mode
        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            if not isinstance(chat_request, OpenAIRequest):
                raise ProviderError(
                    code=400,
                    message="Invalid request format",
                    details={
                        "error": "OpenAI-compatible API requires OpenAI request format"
                    },
                )
        else:
            if not isinstance(chat_request, LLMGatewayRequest):
                raise ProviderError(
                    code=400,
                    message="Invalid request format",
                    details={"error": "LLM Gateway API requires LLM Gateway request format"},
                )

        # Log raw request body
        raw_body = await fastapi_request.body()
        self.logger.debug(
            "Raw request body", extra={"raw_body": raw_body.decode("utf-8")}
        )

        # Get request ID and API key
        api_key = getattr(fastapi_request.state, "api_key", None)
        request_id = getattr(fastapi_request.state, "request_id", None)

        # Skip API key validation if auth is disabled
        if not self.settings.ENABLE_AUTH:
            # Use a dummy API key when auth is disabled
            api_key = "auth-disabled"
        elif not api_key:
            self.logger.warning(
                "Missing API key when auth is enabled",
                extra={
                    "request_id": request_id,
                    "path": fastapi_request.url.path,
                    "method": fastapi_request.method,
                },
            )
            raise ProviderError(
                code=401,
                message="Authentication required",
                details={"error": "Missing API key"},
            )

        chat_context = None
        try:
            # Get provider configuration and clean model ID
            provider_config, model_id = self.provider_manager.get_provider_by_model_id(
                chat_request.model
            )

            # Get mappers and provider
            mapper = container.mapper(provider_config=provider_config)
            model_mapper = container.model_mapper(provider_config=provider_config)
            provider = container.provider(
                provider_config=provider_config,
                mapper=mapper,
                model_mapper=model_mapper,
            )

            # Get model info from provider using clean model ID
            llm_model = await provider.get_model(model_id)
            self.logger.debug("Got model info from provider", extra={"model_id": model_id, "provider_id": provider_config.provider_id})
            # Set external_model_id from the original request model
            llm_model.external_model_id = chat_request.model

            # Create chat context
            origin = fastapi_request.headers.get("origin", "unknown")

            # Collect client application metadata from headers
            metadata = {
                "origin": origin,
            }

            # Get user_id from request state if available
            user_id = getattr(fastapi_request.state, "user_id", None)

            chat_context = ChatContext(
                request=chat_request,
                api_key=api_key,
                user_id=user_id,
                request_id=request_id,
                origin=origin,
                provider_model=llm_model,
                metadata=metadata,
            )

            # Get service from container with all dependencies
            service = container.chat_completion_service(
                provider=provider, context=chat_context
            )

            # Handle streaming request
            if chat_request.stream:
                self.logger.info(
                    "Starting streaming response",
                    extra={
                        "request_id": request_id,
                        "model": llm_model.external_model_id,
                    },
                )
                # Используем значение по умолчанию, если request_id is None
                safe_request_id = request_id or "unknown"
                return StreamingResponse(
                    content=self._handle_stream_response(
                        service=service,
                        chat_context=chat_context,
                        request_id=safe_request_id,
                    ),
                    headers={"Content-Type": "text/event-stream"},
                )

            # Handle regular request
            self.logger.info(
                "Starting regular response",
                extra={
                    "request_id": request_id,
                    "model": llm_model.external_model_id,
                },
            )

            try:
                response = None
                async for resp in service.create_chat_completion():
                    if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                        if isinstance(resp, OpenAIResponse):
                            response = resp
                    else:
                        if isinstance(resp, LLMGatewayResponse):
                            response = resp

                if response is None:
                    self.logger.error(
                        "No response from service",
                        extra={
                            "request_id": request_id,
                            "model": llm_model.external_model_id,
                        },
                    )
                    raise ProviderError(
                        code=500,
                        message="No response from service",
                        details={"error": "Service did not yield any response"},
                    )

                # Log API type and request format info
                api_type = (
                    "OpenAI compatible"
                    if self.settings.ENABLE_OPENAI_COMPATIBLE_API
                    else "LLMGateway"
                )
                request_format = (
                    "OpenAI" if isinstance(chat_request, OpenAIRequest) else "LLMGateway"
                )
                self.logger.info(
                    "Successfully processed regular response",
                    extra={
                        "request_id": request_id,
                        "model": llm_model.external_model_id,
                        "api_type": api_type,
                        "request_format": request_format,
                    },
                )
                return JSONResponse(
                    status_code=HTTP_200_OK,
                    content=response.model_dump(),
                )

            except Exception as e:
                self.logger.error(
                    "Error processing regular response",
                    extra={
                        "request_id": request_id,
                        "model": llm_model.external_model_id,
                        "error": str(e),
                    },
                    exc_info=True,
                )
                raise

        except ValueError as e:
            self.logger.error(
                "Invalid model",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                },
            )
            raise ProviderError(
                code=400,
                message="Invalid model",
                details={"error": str(e)},
            )
        except Exception as e:
            if isinstance(e, ProviderError):
                raise
            self.logger.error(
                "Failed to handle chat completion request",
                extra={
                    "request_id": request_id,
                    "error": str(e),
                },
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message="Failed to handle chat completion request",
                details={"error": str(e)},
            )
        finally:
            # Ensure context cleanup in case of errors, but only for non-streaming requests
            if chat_context is not None and not chat_request.stream:
                try:
                    await self.context_service.cleanup_context(chat_context)
                except Exception as cleanup_error:
                    self.logger.error(
                        "Error cleaning up context in error handler",
                        extra={
                            "request_id": request_id,
                            "error": str(cleanup_error),
                        },
                        exc_info=True,
                    )
