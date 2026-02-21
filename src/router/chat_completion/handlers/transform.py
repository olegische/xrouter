"""Request transformation handler."""
from typing import AsyncGenerator, List, Optional, Union
from uuid import uuid4

from core.logger import LoggerService
from core.settings import Settings
from providers.base import Provider
from providers.models import (
    Message,
    ProviderError,
    ProviderRequest,
    UserMessage,
)
from providers.models.base.reasoning import ReasoningConfig
from ..handlers.base import RequestHandler
from ..models import ChatContext
from ..models.openai import (
    OpenAIResponse,
    OpenAIStreamChunk,
)
from ..models.llm_gateway import (
    LLMGatewayResponse,
    LLMGatewayStreamChunk,
)


class RequestTransformHandler(RequestHandler):
    """Transform ChatRequest into provider-agnostic ProviderRequest format.

    This handler is responsible for:
    1. Validating the input request
    2. Converting prompt to messages if needed
    3. Creating provider-agnostic messages
    4. Building the final ProviderRequest with all required fields
    """

    def __init__(
        self,
        logger: LoggerService,
        settings: Settings,
    ) -> None:
        """Initialize handler.

        Args:
            logger: Logger service
            settings: Application settings
        """
        self.logger = logger.get_logger(__name__)
        self.settings = settings
        self.logger.debug(
            "Initialized RequestTransformHandler",
            extra={
                "handler": "RequestTransformHandler",
                "openai_compatible": settings.ENABLE_OPENAI_COMPATIBLE_API,
            },
        )

    def canHandle(self, context: ChatContext) -> bool:
        """Check if handler can process the request.

        Args:
            context: Request context

        Returns:
            bool: True if request has either messages or prompt
        """
        return bool(context.request)

    def _validate_messages_and_prompt(self, context: ChatContext) -> None:
        """Validate messages and prompt fields.

        Args:
            context: Request context

        Raises:
            ProviderError: If validation fails
        """
        if not context.request:
            raise ProviderError(
                code=400,
                message="Request cannot be None",
                details={"error": "Missing request"},
            )

        # For OpenAI compatible API, only check messages
        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            if not context.request.messages:
                raise ProviderError(
                    code=400,
                    message="Messages are required for OpenAI format",
                    details={"error": "Missing required field"},
                )
        else:
            # For LLM Gateway API, check both messages and prompt
            if not context.request.messages and not context.request.prompt:
                raise ProviderError(
                    code=400,
                    message="Either messages or prompt is required",
                    details={"error": "Missing required field"},
                )

            if context.request.messages and context.request.prompt:
                raise ProviderError(
                    code=400,
                    message="Cannot provide both messages and prompt",
                    details={"error": "Conflicting fields"},
                )

    def _validate_temperature(self, context: ChatContext) -> None:
        """Validate temperature field.

        Args:
            context: Request context

        Raises:
            ProviderError: If validation fails
        """
        if not context.request:
            raise ProviderError(
                code=400,
                message="Request cannot be None",
                details={"error": "Missing request"},
            )

        if context.request.temperature is not None:
            if context.request.temperature < 0.0 or context.request.temperature > 2.0:
                raise ProviderError(
                    code=400,
                    message="Temperature must be between 0.0 and 2.0",
                    details={"error": "Invalid temperature value"},
                )

    def _validate_top_p(self, context: ChatContext) -> None:
        """Validate top_p field.

        Args:
            context: Request context

        Raises:
            ProviderError: If validation fails
        """
        if not context.request:
            raise ProviderError(
                code=400,
                message="Request cannot be None",
                details={"error": "Missing request"},
            )

        if context.request.top_p is not None:
            if context.request.top_p <= 0.0 or context.request.top_p > 1.0:
                raise ProviderError(
                    code=400,
                    message="Top P must be between 0.0 and 1.0",
                    details={"error": "Invalid top_p value"},
                )

    def _validate_penalties(self, context: ChatContext) -> None:
        """Validate penalty fields.

        Args:
            context: Request context

        Raises:
            ProviderError: If validation fails
        """
        if not context.request:
            raise ProviderError(
                code=400,
                message="Request cannot be None",
                details={"error": "Missing request"},
            )

        # Validate repetition_penalty only for LLM Gateway API
        if not self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            if context.request.repetition_penalty is not None:
                if (
                    context.request.repetition_penalty <= 0.0
                    or context.request.repetition_penalty > 2.0
                ):
                    raise ProviderError(
                        code=400,
                        message="Repetition penalty must be between 0.0 and 2.0",
                        details={"error": "Invalid repetition_penalty value"},
                    )

        # Validate frequency_penalty and presence_penalty for both APIs
        if context.request.frequency_penalty is not None:
            if (
                context.request.frequency_penalty < -2.0
                or context.request.frequency_penalty > 2.0
            ):
                raise ProviderError(
                    code=400,
                    message="Frequency penalty must be between -2.0 and 2.0",
                    details={"error": "Invalid frequency_penalty value"},
                )

        if context.request.presence_penalty is not None:
            if (
                context.request.presence_penalty < -2.0
                or context.request.presence_penalty > 2.0
            ):
                raise ProviderError(
                    code=400,
                    message="Presence penalty must be between -2.0 and 2.0",
                    details={"error": "Invalid presence_penalty value"},
                )

    def _validate_request(self, context: ChatContext) -> None:
        """Validate request format.

        Args:
            context: Request context

        Raises:
            ProviderError: If request validation fails
        """
        if not context.request:
            raise ProviderError(
                code=400,
                message="Request cannot be None",
                details={"error": "Missing request"},
            )

        self.logger.debug(
            "Starting request validation",
            extra={
                "request_id": context.request_id,
                "has_messages": bool(context.request.messages),
            },
        )
        self._validate_messages_and_prompt(context)
        self._validate_temperature(context)
        self._validate_top_p(context)
        self._validate_penalties(context)

    def _convert_prompt_to_messages(self, context: ChatContext) -> None:
        """Convert prompt to messages format if needed.

        Args:
            context: Request context

        Raises:
            ProviderError: If request is None
        """
        if not context.request:
            raise ProviderError(
                code=400,
                message="Request cannot be None",
                details={"error": "Missing request"},
            )

        # Only convert prompt for LLM Gateway API
        if not self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            if context.request.prompt:
                context.request.messages = [
                    UserMessage(role="user", content=context.request.prompt)
                ]

    def _transform_reasoning(self, context: ChatContext) -> Optional[ReasoningConfig]:
        """Transform reasoning from OpenAI format to OpenRouter format.

        Args:
            context: Request context

        Returns:
            Optional[ReasoningConfig]: OpenRouter-style reasoning configuration or None

        Raises:
            ProviderError: If request is None
        """
        if not context.request:
            raise ProviderError(
                code=400,
                message="Request cannot be None",
                details={"error": "Missing request"},
            )

        # For OpenAI compatible API, convert reasoning_effort to reasoning object
        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            if (
                hasattr(context.request, "reasoning_effort")
                and context.request.reasoning_effort
            ):
                self.logger.debug(
                    "Converting OpenAI reasoning_effort to OpenRouter reasoning format",
                    extra={
                        "reasoning_effort": context.request.reasoning_effort,
                        "request_id": context.request_id,
                    },
                )
                return ReasoningConfig(effort=context.request.reasoning_effort)
            return None
        else:
            # For LLM Gateway API, use reasoning object as is
            if hasattr(context.request, "reasoning") and context.request.reasoning:
                self.logger.debug(
                    "Using LLM Gateway reasoning configuration",
                    extra={
                        "reasoning": context.request.reasoning.model_dump()
                        if context.request.reasoning
                        else None,
                        "request_id": context.request_id,
                    },
                )
                return context.request.reasoning
            return None

    def _validate_messages(
        self, messages: List[Message], context: ChatContext
    ) -> List[Message]:
        """Convert chat messages to provider messages.

        Args:
            messages: List of chat messages
            context: Request context

        Returns:
            List[Message]: Provider-agnostic message list

        Raises:
            ProviderError: If message conversion fails
        """
        self.logger.debug(
            "Converting messages to provider format",
            extra={
                "message_count": len(messages),
            },
        )
        validated_messages = []
        for msg in messages:
            # Convert to dict if not already
            message_data = msg if isinstance(msg, dict) else msg.model_dump()

            # Validate message
            try:
                validated_message = Message.model_validate(message_data)
                validated_messages.append(validated_message)

                # Check for cache_control in user and system messages
                if validated_message.role in ["user", "system"]:
                    # Check if content is a list of content parts
                    if isinstance(validated_message.content, list):
                        for content_part in validated_message.content:
                            if getattr(content_part, "cache_control", None) is not None:
                                context.cache_write = True
                                self.logger.debug(
                                    "Found cache_control in message",
                                    extra={
                                        "role": validated_message.role,
                                        "cache_control": content_part.cache_control.model_dump(),
                                    },
                                )
                                break
            except ValueError as e:
                if "Invalid role:" in str(e):
                    raise ProviderError(
                        code=400,
                        message=str(e),
                        details={"error": "Unsupported message role"},
                    )
                raise ProviderError(
                    code=400,
                    message=f"Invalid message format: {str(e)}",
                    details={"error": "Message validation failed"},
                )
        self.logger.debug(
            "Messages validated",
            extra={
                "message_count": len(validated_messages),
                "cache_write": context.cache_write,
            },
        )
        return validated_messages

    def _create_provider_request(self, context: ChatContext) -> ProviderRequest:
        """Create provider-agnostic request.

        Args:
            context: Request context

        Returns:
            ProviderRequest: Provider-agnostic request

        Raises:
            ProviderError: If provider model is not set or request creation fails
        """
        if not context.request:
            raise ProviderError(
                code=400,
                message="Request cannot be None",
                details={"error": "Missing request"},
            )

        self._convert_prompt_to_messages(context)
        if not context.request.messages:  # Make mypy happy
            raise ProviderError(
                code=400,
                message="Messages cannot be None at this point",
                details={"error": "Missing messages"},
            )

        if not context.provider_model:
            raise ProviderError(
                code=400,
                message="Provider model must be set in context",
                details={"error": "Missing provider model"},
            )

        validated_messages = self._validate_messages(context.request.messages, context)

        # Transform reasoning configuration
        reasoning_config = self._transform_reasoning(context)

        # Build base request params
        request_params = {
            "messages": validated_messages,
            "model": context.provider_model.model_id,
            "request_id": context.request_id,
            "temperature": context.request.temperature,
            "max_tokens": context.request.max_tokens,
            "stream": context.request.stream,
            "top_p": context.request.top_p,
            "stop": context.request.stop,
            "frequency_penalty": context.request.frequency_penalty,
            "presence_penalty": context.request.presence_penalty,
            "tools": context.request.tools,
            "tool_choice": context.request.tool_choice,
            "usage": context.request.usage,
            "reasoning": reasoning_config,
        }

        # Add repetition_penalty only for LLM Gateway API
        if not self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            request_params["repetition_penalty"] = context.request.repetition_penalty

        return ProviderRequest(**request_params)

    async def _transform_request(self, context: ChatContext) -> None:
        """Transform request into provider-agnostic format.

        Args:
            context: Request context

        Raises:
            ProviderError: If transformation fails
        """
        if not context.request:
            raise ProviderError(
                code=400,
                message="Request cannot be None",
                details={"error": "Missing request"},
            )

        # Set include_usage flag based on request's usage parameter
        if context.request.usage and context.request.usage.include is not None:
            context.include_usage = context.request.usage.include
            self.logger.debug(
                "Setting include_usage flag based on request",
                extra={
                    "request_id": context.request_id,
                    "include_usage": context.include_usage,
                },
            )

        self.logger.info(
            "Starting request transformation",
            extra={
                "request_id": context.request_id,
                "model": context.provider_model.model_id
                if context.provider_model
                else None,
                "provider": context.provider_model.provider_id
                if context.provider_model
                else None,
                "include_usage": context.include_usage,
            },
        )
        try:
            self._validate_request(context)

            if context.generation_id is None:
                context.generation_id = f"gen_{str(uuid4())}"
                self.logger.debug(
                    "Generated ID for response (billing disabled)",
                    extra={
                        "request_id": context.request_id,
                        "generation_id": context.generation_id,
                    },
                )

            provider_request = self._create_provider_request(context)
            has_system_message = any(
                msg.role == "system" for msg in provider_request.messages
            )
            filtered_messages = [
                msg
                for msg in provider_request.messages
                if msg.role in ["user", "assistant"]
            ]
            self.logger.debug(
                "Created provider request",
                extra={
                    "request_id": provider_request.request_id,
                    "model": provider_request.model,
                    "stream": provider_request.stream,
                    "message_count": len(provider_request.messages),
                    "has_system_message": has_system_message,
                    "messages": str(filtered_messages),
                },
            )
            context.provider_request = provider_request

            self.logger.info(
                "Request transformation completed successfully",
                extra={
                    "request_id": context.request_id,
                    "provider": context.provider_model.provider_id
                    if context.provider_model
                    else None,
                    "model": context.provider_model.model_id
                    if context.provider_model
                    else None,
                    "stream": provider_request.stream,
                },
            )

        except ProviderError:
            raise  # Re-raise ProviderError as is
        except Exception as e:
            self.logger.error(
                "Failed to transform request",
                extra={
                    "error": str(e),
                    "request_id": context.request_id,
                    "provider": context.provider_model.provider_id
                    if context.provider_model
                    else None,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=400,
                message="Failed to transform request",
                details={"error": str(e)},
            )

    async def handleRequest(
        self,
        context: ChatContext,
        provider: Provider,  # Add provider parameter for compatibility
    ) -> AsyncGenerator[
        Union[OpenAIResponse, OpenAIStreamChunk, LLMGatewayResponse, LLMGatewayStreamChunk],
        None,
    ]:
        """Handle request by transforming it into provider-agnostic format.

        Args:
            context: Request context

        Yields:
            No responses at this step

        Raises:
            ProviderError: If transformation fails
        """
        if self.canHandle(context):
            await self._transform_request(context)

        if False:
            yield  # for AsyncGenerator type hint
