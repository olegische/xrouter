"""Provider completion handler."""
import time
from datetime import datetime
from typing import AsyncGenerator, Dict, List, Optional, Union

from core.logger import LoggerService
from core.settings import Settings
from providers.base import Provider
from providers.models import (
    AssistantMessage,
    Message,
    NonStreamChoice,
    ProviderError,
    ProviderResponse,
    ProviderStreamChunk,
    ResponseType,
    Usage,
)
from .base import RequestHandler
from ..models import ChatContext
from ..models.openai import (
    OpenAINonStreamChoice,
    OpenAIResponse,
    OpenAIStreamChoice,
    OpenAIStreamChunk,
)
from ..models.llm_gateway import (
    LLMGatewayNonStreamChoice,
    LLMGatewayResponse,
    LLMGatewayStreamChoice,
    LLMGatewayStreamChunk,
)


class CompletionHandler(RequestHandler):
    """Execute provider completion."""

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
            "Initialized CompletionHandler",
            extra={
                "handler": "CompletionHandler",
            },
        )

    def canHandle(self, context: ChatContext) -> bool:
        """Check if handler can process the request.

        Args:
            context: Request context

        Returns:
            bool: True if context has provider request
        """
        can_handle = bool(
            context.provider_model is not None and context.provider_request is not None
        )
        self.logger.debug(
            "Checking if can handle request",
            extra={
                "request_id": context.request_id,
                "has_provider_model": bool(context.provider_model),
                "has_provider_request": bool(context.provider_request),
                "can_handle": can_handle,
            },
        )
        return can_handle

    def _create_base_tool_call(self, tool_call: dict) -> dict:
        """Create base structure for tool call.

        Args:
            tool_call: Raw tool call data

        Returns:
            Base tool call structure with optional function field
        """
        formatted_call = {"id": tool_call["id"], "type": tool_call["type"]}
        # Добавляем поле function только если тип tool_call - function
        if tool_call["type"] == "function":
            formatted_call["function"] = {
                "name": tool_call["function"]["name"],
                "arguments": tool_call["function"]["arguments"],
            }
        return formatted_call

    def _format_tool_calls(self, tool_calls: List[dict]) -> List[dict]:
        """Format tool calls into standard structure.

        Args:
            tool_calls: List of tool calls from delta

        Returns:
            List of formatted tool calls
        """
        return [self._create_base_tool_call(tool_call) for tool_call in tool_calls]

    def _update_tool_calls_dict(
        self, tool_calls_dict: Dict[str, dict], tool_call: dict
    ) -> None:
        """Update tool calls dictionary with new tool call.

        Args:
            tool_calls_dict: Dictionary of existing tool calls
            tool_call: New tool call to add or update
        """
        tool_call_id = tool_call["id"]
        if tool_call_id not in tool_calls_dict:
            # Новый tool_call
            tool_calls_dict[tool_call_id] = self._create_base_tool_call(tool_call)
        else:
            # Дополняем существующий tool_call только если это function
            # Arguments приходят от моделей GigaChat и Yandex сразу
            # В OpenAI аргументы приходят чанками
            # Мы поддерживаем оба варианта и просто проксируем аргументы как есть
            # TBD: Возможно есть смысл накапливать аргументы в ожидании
            # finish_reason = "tool_calls"
            if tool_call["type"] == "function" and "function" in tool_call:
                if "arguments" in tool_call["function"]:
                    tool_calls_dict[tool_call_id]["function"]["arguments"] += tool_call[
                        "function"
                    ]["arguments"]

    def _is_final_chunk(self, chunk: ProviderStreamChunk, context: ChatContext) -> bool:
        """Check if this is the final chunk.

        A chunk is final if:
        - It has both finish_reason and usage in the same chunk
        - Or it has usage and we previously saw finish_reason

        Args:
            chunk: Stream chunk to check
            context: Request context to store state

        Returns:
            bool: True if this is the final chunk
        """
        has_finish_reason = any(
            choice.finish_reason is not None for choice in chunk.choices
        )
        has_usage = chunk.usage is not None

        # Always save usage when we see it
        if has_usage:
            context.native_usage = chunk.usage
            self.logger.info(
                "Storing native usage in context from chunk",
                extra={
                    "request_id": context.request_id,
                    "native_usage": chunk.usage.model_dump(),
                },
            )

        # Case 1: Both finish_reason and usage in same chunk
        if has_finish_reason and has_usage:
            return True

        # Case 2: Has usage and we previously saw finish_reason
        if has_usage and context.metadata.get("has_finish_reason"):
            return True

        # Store state if we see finish_reason
        if has_finish_reason:
            context.metadata["has_finish_reason"] = True
            # Also store the actual finish_reason value
            for choice in chunk.choices:
                if choice.finish_reason is not None:
                    context.metadata["finish_reason"] = choice.finish_reason
                    break
            return False

        return False

    def _create_usage(
        self, existing_usage: Optional[Usage], context: ChatContext
    ) -> Optional[Usage]:
        """Return usage information based on context's include_usage flag.

        If include_usage is True, return full usage details.
        If include_usage is False, return only basic token counts.

        Args:
            existing_usage: Existing usage data if any
            context: Request context

        Returns:
            Usage object or None if no data available
        """
        if not existing_usage:
            return None

        # If include_usage is True, return full usage details
        if context.include_usage:
            self.logger.debug(
                "Returning full usage details",
                extra={
                    "request_id": context.request_id,
                    "include_usage": context.include_usage,
                    "has_completion_details": existing_usage.completion_tokens_details
                    is not None,
                    "has_prompt_details": existing_usage.prompt_tokens_details
                    is not None,
                },
            )
            return existing_usage

        # Otherwise, return only basic token counts without detailed breakdowns
        self.logger.debug(
            "Returning basic usage without details",
            extra={
                "request_id": context.request_id,
                "include_usage": context.include_usage,
                "prompt_tokens": existing_usage.prompt_tokens,
                "completion_tokens": existing_usage.completion_tokens,
                "total_tokens": existing_usage.total_tokens,
            },
        )
        return Usage(
            prompt_tokens=existing_usage.prompt_tokens,
            completion_tokens=existing_usage.completion_tokens,
            total_tokens=existing_usage.total_tokens,
        )

    def _create_stream_chunk(
        self, chunk: ProviderStreamChunk, context: ChatContext
    ) -> Union[OpenAIStreamChunk, LLMGatewayStreamChunk]:
        """Create StreamChunk from ProviderStreamChunk.

        Args:
            chunk: Provider stream chunk
            context: Request context

        Returns:
            StreamChunk: Created stream chunk

        Raises:
            ProviderError: If provider_model is None
        """
        if context.provider_model is None:
            raise ProviderError(
                code=500,
                message="Missing provider model",
                details={"error": "provider_model is None"},
            )

        # Convert provider choices to our StreamingChoice format
        choices = []
        for choice in chunk.choices:
            # Создаем AssistantMessage для delta
            delta_content = choice.delta.content if choice.delta else ""
            delta_role = choice.delta.role if choice.delta else None
            delta_tool_calls = choice.delta.tool_calls if choice.delta else None
            delta_reasoning = choice.delta.reasoning if choice.delta else None

            # Преобразуем tool_calls в правильный формат если они есть
            formatted_tool_calls = (
                [tool_call.model_dump() for tool_call in delta_tool_calls]
                if delta_tool_calls
                else None
            )

            if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                choices.append(
                    OpenAIStreamChoice(
                        index=choice.index,
                        finish_reason=choice.finish_reason,
                        delta=AssistantMessage(
                            content=delta_content
                            if delta_content is not None
                            else None,
                            role=delta_role,
                            tool_calls=formatted_tool_calls,
                            reasoning=delta_reasoning,
                        ),
                    )
                )
            else:
                choices.append(
                    LLMGatewayStreamChoice(
                        index=choice.index,
                        finish_reason=choice.finish_reason,
                        native_finish_reason=choice.finish_reason,
                        delta=AssistantMessage(
                            content=delta_content
                            if delta_content is not None
                            else None,
                            role=delta_role,
                            tool_calls=formatted_tool_calls,
                            reasoning=delta_reasoning,
                        ),
                        error=None,
                    )
                )

        # Calculate usage for final chunk
        usage = None
        if self._is_final_chunk(chunk, context):
            usage = self._create_usage(chunk.usage, context)

        common_params = {
            "id": context.generation_id,
            "created": int(datetime.utcnow().timestamp()),
            "model": context.provider_model.external_model_id,
            "choices": choices,
            "object": ResponseType.CHAT_COMPLETION_CHUNK,
            "system_fingerprint": None,
            "usage": usage,
        }

        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            stream_chunk = OpenAIStreamChunk(**common_params)
        else:
            stream_chunk = LLMGatewayStreamChunk(
                **common_params,
                provider=context.provider_model.provider_id,
            )
        return stream_chunk

    def _assemble_response_from_chunks(
        self, chunks: List[ProviderStreamChunk], context: ChatContext
    ) -> ProviderResponse:
        """Assemble full ProviderResponse from stream chunks.

        Args:
            chunks: List of ProviderStreamChunk
            context: Request context

        Returns:
            ProviderResponse

        Raises:
            ProviderError: If provider_model is None
        """
        if context.provider_model is None:
            raise ProviderError(
                code=500,
                message="Missing provider model",
                details={"error": "provider_model is None"},
            )

        full_content = ""
        full_reasoning = ""
        role = None
        usage = None
        finish_reason = None
        tool_calls_dict: Dict[str, dict] = {}  # id -> MessageToolCall

        for chunk in chunks:
            for choice in chunk.choices:
                if choice.delta:
                    if choice.delta.role:
                        role = choice.delta.role
                    if choice.delta.content:
                        full_content += choice.delta.content
                    if choice.delta.reasoning:
                        full_reasoning += choice.delta.reasoning
                    # Обработка tool_calls
                    # У нас одна и та же модель сообщения асситента на
                    # запрос и на ответ. Но валидация проходит по-разному
                    # В ответе всегда есть index у tool_call, в запросе
                    # индекса может не быть и это должно проходить валидацию
                    # Поэтому при маппинге ответа роутера используется
                    # MessageToolCall (optional index), а не
                    # ResponseToolCall (required index)
                    if choice.delta.tool_calls:
                        for tool_call in choice.delta.tool_calls:
                            self._update_tool_calls_dict(
                                tool_calls_dict, tool_call.model_dump()
                            )
                if choice.finish_reason:
                    finish_reason = choice.finish_reason
            if chunk.usage:
                usage = chunk.usage

        if not role:
            role = "assistant"

        # Store the original unfiltered usage in context for billing purposes
        if usage:
            context.native_usage = usage
            # Simply log the entire native_usage object
            self.logger.info(
                "Storing native usage in context",
                extra={
                    "request_id": context.request_id,
                    "native_usage": usage.model_dump(),
                },
            )

        # Filter usage based on include_usage flag for the response to the user
        usage = self._create_usage(usage, context)

        # Создаем AssistantMessage с собранными данными
        message = Message.model_validate(
            {
                "role": role,
                "content": full_content if full_content else None,
                "tool_calls": list(tool_calls_dict.values())
                if tool_calls_dict
                else None,
                "reasoning": full_reasoning if full_reasoning else None,
            }
        )

        provider_response = ProviderResponse(
            id=context.request_id,
            created=int(datetime.utcnow().timestamp()),
            model=context.provider_model.model_id,  # Используем model_id для провайдера
            provider_id=context.provider_model.provider_id,
            request_id=context.request_id,
            choices=[
                NonStreamChoice(index=0, finish_reason=finish_reason, message=message)
            ],
            usage=usage,
            object=ResponseType.CHAT_COMPLETION,
        )
        self.logger.info(
            "Found usage in final chunk",
            extra={
                "request_id": context.request_id,
                "chunk_id": chunk.id,
                "usage": chunk.usage.model_dump() if chunk.usage else None,
            },
        )
        return provider_response

    async def _create_chat_response(
        self, response: ProviderResponse, context: ChatContext
    ) -> Union[OpenAIResponse, LLMGatewayResponse]:
        """Create chat response from provider response.

        Args:
            response: Provider response
            context: Request context

        Returns:
            Chat response

        Raises:
            ProviderError: If response conversion fails or provider_model is None
        """
        if context.provider_model is None:
            raise ProviderError(
                code=500,
                message="Missing provider model",
                details={"error": "provider_model is None"},
            )

        self.logger.debug(
            "Creating chat response",
            extra={
                "request_id": response.request_id,
                "model": context.provider_model.external_model_id,
                "provider": context.provider_model.provider_id,
            },
        )

        try:
            # Convert provider choices to appropriate NonStreamingChoice format
            choices = []
            for choice in response.choices:
                message = choice.message or AssistantMessage(role="assistant")
                message_params = {
                    "role": message.role,
                    "content": message.content,
                    "reasoning": message.reasoning,
                    "tool_calls": message.tool_calls,
                    "refusal": message.refusal,
                }

                if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                    choices.append(
                        OpenAINonStreamChoice(
                            index=choice.index,
                            finish_reason=choice.finish_reason,
                            message=AssistantMessage(**message_params),
                        )
                    )
                else:
                    choices.append(
                        LLMGatewayNonStreamChoice(
                            index=choice.index,
                            finish_reason=choice.finish_reason,
                            native_finish_reason=choice.finish_reason,
                            message=AssistantMessage(**message_params),
                            error=None,
                        )
                    )

            # Filter usage based on include_usage flag
            filtered_usage = self._create_usage(response.usage, context)

            # Common parameters for both response types
            common_params = {
                "id": context.generation_id,
                "created": int(datetime.utcnow().timestamp()),
                "model": context.provider_model.external_model_id,
                "choices": choices,
                "usage": filtered_usage,
                "object": response.object,
                "system_fingerprint": None,
            }

            # Create appropriate response type based on setting
            if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                router_response = OpenAIResponse(**common_params)
            else:
                router_response = LLMGatewayResponse(
                    **common_params,
                    provider=context.provider_model.provider_id,
                )

            self.logger.info(
                "Successfully created chat response",
                extra={
                    "request_id": response.request_id,
                    "model": context.provider_model.external_model_id,
                    "provider": context.provider_model.provider_id,
                    "total_tokens": response.usage.total_tokens
                    if response.usage
                    else None,
                },
            )
            return router_response

        except Exception as e:
            self.logger.error(
                "Failed to create chat response",
                extra={
                    "error": str(e),
                    "request_id": response.request_id,
                    "model": context.provider_model.external_model_id,
                    "provider": context.provider_model.provider_id,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message="Failed to create chat response",
                details={"error": str(e)},
            )

    async def handleRequest(
        self,
        context: ChatContext,
        provider: Provider,
    ) -> AsyncGenerator[
        Union[OpenAIResponse, OpenAIStreamChunk, LLMGatewayResponse, LLMGatewayStreamChunk],
        None,
    ]:
        """Handle request by executing provider completion.

        Args:
            context: Request context
            provider: Provider instance

        Yields:
            Chat responses or stream chunks

        Raises:
            ProviderError: If completion fails
        """
        if not self.canHandle(context):
            # Log and exit
            self.logger.warning(
                "Cannot handle request: missing required context",
                extra={
                    "request_id": getattr(context, "request_id", None),
                    "has_provider_model": bool(context.provider_model),
                    "has_provider_request": bool(context.provider_request),
                },
            )
            return

        if (
            not context.request
            or not context.provider_request
            or not context.provider_model
        ):
            raise ProviderError(
                code=500,
                message="Missing required context",
                details={"error": "Required context fields are None"},
            )

        try:
            # Set start time before generation
            context.metadata["start_time"] = time.time()

            if context.request.stream:
                # Streaming mode
                async for chunk in provider.create_completion(context.provider_request):
                    stream_chunk = self._create_stream_chunk(chunk, context)
                    # Accumulate response text from chunk
                    for choice in chunk.choices:
                        if choice.delta and choice.delta.content:
                            if context.accumulated_response is None:
                                context.accumulated_response = choice.delta.content
                            else:
                                context.accumulated_response += choice.delta.content
                    yield stream_chunk
                    if self._is_final_chunk(chunk, context):
                        # Final chunk, save final response
                        context.final_response = stream_chunk
                        # Log accumulated content on final chunk
                        self.logger.debug(
                            "Final accumulated content",
                            extra={
                                "request_id": context.request_id,
                                "accumulated_content": context.accumulated_response,
                            },
                        )
                        break
            else:
                # Non-streaming mode
                collected_chunks = []
                async for chunk in provider.create_completion(context.provider_request):
                    self.logger.debug(
                        "Received chunk",
                        extra={
                            "request_id": context.request_id,
                            "chunk": str(chunk),
                            "chunk_data": chunk.model_dump(),
                        },
                    )
                    collected_chunks.append(chunk)
                    if self._is_final_chunk(chunk, context):
                        break

                # Log collected chunks
                self.logger.info(
                    "Collected all chunks",
                    extra={
                        "request_id": context.request_id,
                        "chunk_count": len(collected_chunks),
                    },
                )
                self.logger.debug(
                    "All collected chunks",
                    extra={
                        "request_id": context.request_id,
                        "chunks": [chunk.model_dump() for chunk in collected_chunks],
                    },
                )

                # Accumulate response text from all chunks
                context.accumulated_response = ""
                for chunk in collected_chunks:
                    for choice in chunk.choices:
                        if choice.delta and choice.delta.content:
                            context.accumulated_response += choice.delta.content

                # Assemble full response from chunks
                full_response = self._assemble_response_from_chunks(
                    collected_chunks, context
                )
                self.logger.debug(
                    "Assembled full response",
                    extra={
                        "request_id": context.request_id,
                        "response": full_response.model_dump(),
                    },
                )

                # Convert to ChatResponse
                chat_response = await self._create_chat_response(full_response, context)
                self.logger.debug(
                    "Created chat response",
                    extra={
                        "request_id": context.request_id,
                        "response": chat_response.model_dump(),
                    },
                )
                context.final_response = chat_response
                yield chat_response

        except Exception as e:
            if isinstance(e, ProviderError):
                raise
            # Handle other exceptions
            self.logger.error(
                "Failed to execute completion",
                extra={
                    "error": str(e),
                    "request_id": context.request_id,
                    "model": context.provider_model.external_model_id,
                    "provider": context.provider_model.provider_id,
                },
                exc_info=True,
            )
            raise ProviderError(
                code=500,
                message="Failed to execute completion",
                details={"error": str(e)},
            )
