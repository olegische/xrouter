"""GigaChat request and response mappers."""
import json
import uuid
from typing import Any, Dict, List, Optional, cast

from core.logger import LoggerService
from ..base_mapper import BaseMapper
from .models import (
    GigaChatFunction,
    GigaChatFunctionCall,
    GigaChatFunctionCallFunction,
    GigaChatMessage,
    GigaChatRequest,
    GigaChatStreamChoice,
    GigaChatStreamResponse,
)
from ..models import (
    AssistantMessage,
    FunctionCall,
    MessageToolCall,
    ProviderConfig,
    ProviderError,
    ProviderRequest,
    ProviderStreamChunk,
    ResponseType,
    StreamChoice,
    Usage,
)
from ..models.base.common import PromptTokensDetails


class GigaChatMapper(BaseMapper):
    """Mapper for GigaChat requests and responses."""

    def __init__(self, provider: ProviderConfig, logger: LoggerService) -> None:
        """Initialize mapper.

        Args:
            provider: Provider model instance
            logger: Logger service instance
        """
        super().__init__(provider=provider, logger=logger)

    def _merge_system_messages(
        self, request: ProviderRequest
    ) -> Optional[GigaChatMessage]:
        system_messages = [msg for msg in request.messages if msg.role == "system"]
        if not system_messages:
            return None

        system_content_parts = []
        for msg in system_messages:
            converted = self.convert_content_to_string("system", msg.content)
            if msg.name:
                converted = f"[{msg.name}] {converted}"
            if converted:
                system_content_parts.append(converted)

        if not system_content_parts:
            return None

        return GigaChatMessage.model_validate(
            {
                "role": "system",
                "content": "\n\n".join(system_content_parts),
                "name": system_messages[0].name,
                "function_call": None,
                "functions_state_id": None,
                "attachments": None,
            }
        )

    def _is_preamble_assistant_message(
        self,
        request: ProviderRequest,
        msg_index: int,
        pending_tool_call_id: Optional[str],
    ) -> bool:
        msg = request.messages[msg_index]
        if (
            msg.role != "assistant"
            or msg.tool_calls
            or not pending_tool_call_id
        ):
            return False

        for future_msg in request.messages[msg_index + 1 :]:
            if (
                future_msg.role == "tool"
                and future_msg.tool_call_id == pending_tool_call_id
            ):
                self.logger.debug(
                    "Skipping preamble assistant message between tool_call and result",
                    extra={
                        "content_preview": msg.content[:100] if msg.content else None,
                        "tool_call_id": pending_tool_call_id,
                    },
                )
                return True
            if future_msg.role in ("assistant", "user"):
                break
        return False

    def _build_gigachat_message(
        self,
        msg: Any,
    ) -> tuple[GigaChatMessage, Optional[str]]:
        role = "function" if msg.role == "tool" else msg.role
        message_data: Dict[str, Any] = {
            "role": role,
            "content": self.convert_content_to_string(role, msg.content),
            "name": None,
            "function_call": None,
            "functions_state_id": None,
            "attachments": None,
        }

        if msg.role == "tool":
            message_data["name"] = msg.name
        elif msg.name:
            message_data["name"] = msg.name

        tracked_tool_call_id = None
        if msg.role == "assistant" and msg.tool_calls:
            tool_call = msg.tool_calls[0]
            arguments = tool_call.function.arguments
            if isinstance(arguments, str):
                try:
                    arguments = json.loads(arguments)
                except json.JSONDecodeError:
                    pass
            message_data["content"] = ""
            message_data["function_call"] = {
                "name": tool_call.function.name,
                "arguments": arguments,
            }
            message_data["functions_state_id"] = tool_call.id
            tracked_tool_call_id = tool_call.id

        return GigaChatMessage.model_validate(message_data), tracked_tool_call_id

    def _map_tools(
        self, request: ProviderRequest
    ) -> Optional[List[GigaChatFunction]]:
        if not request.tools:
            self.logger.info(
                "Request does not contain tools",
                extra={"has_tools": False, "request_id": request.request_id},
            )
            return None

        functions = [
            GigaChatFunction(
                name=tool.function.name,
                description=tool.function.description,
                parameters=tool.function.parameters,
            )
            for tool in request.tools
        ]
        tool_names_with_params = []
        for tool in request.tools:
            params = tool.function.parameters or {}
            param_names = params.get("properties", {}).keys()
            tool_names_with_params.append(
                f"{tool.function.name}({', '.join(param_names)})"
            )

        self.logger.info(
            "Request contains tools",
            extra={
                "has_tools": True,
                "tools_count": len(request.tools),
                "tool_names": tool_names_with_params,
                "request_id": request.request_id,
            },
        )
        return functions

    @staticmethod
    def _map_tool_choice(request: ProviderRequest) -> Any:
        if not request.tool_choice:
            return None
        if isinstance(request.tool_choice, str):
            if request.tool_choice == "none":
                return "none"
            if request.tool_choice == "auto":
                return "auto"
            return None
        return GigaChatFunctionCallFunction(
            name=request.tool_choice.function.name,
            partial_arguments=None,
        )

    def map_to_provider_request(  # noqa: C901
        self, request: ProviderRequest
    ) -> Dict[str, Any]:
        """Map provider request to GigaChat request.

        Args:
            request: Provider-agnostic request

        Returns:
            GigaChat-specific request

        Raises:
            ProviderError: If mapping fails
        """
        try:
            if not isinstance(request, ProviderRequest):
                raise ProviderError(
                    code=400,
                    message="Failed to map request for GigaChat",
                    details={"error": "Invalid request type"},
                )

            self.logger.debug(
                "Mapping provider request to GigaChat format",
                extra={
                    "model": request.model,
                    "temperature": request.temperature,
                    "max_tokens": request.max_tokens,
                    "request_id": request.request_id,
                },
            )

            messages = []
            merged_system_message = self._merge_system_messages(request)

            injected_system = False
            last_assistant_with_tool_call = None
            for msg_idx, msg in enumerate(request.messages):
                if msg.role == "system":
                    if not injected_system and merged_system_message:
                        messages.append(merged_system_message)
                        injected_system = True
                    continue

                if self._is_preamble_assistant_message(
                    request=request,
                    msg_index=msg_idx,
                    pending_tool_call_id=last_assistant_with_tool_call,
                ):
                    continue

                message, tracked_tool_call_id = self._build_gigachat_message(msg)
                if tracked_tool_call_id:
                    last_assistant_with_tool_call = tracked_tool_call_id
                if msg.role == "tool" and msg.tool_call_id == last_assistant_with_tool_call:
                    last_assistant_with_tool_call = None

                messages.append(message)

            self.logger.debug(
                "Building GigaChat request",
                extra={
                    "messages_count": len(messages),
                    "temperature": request.temperature,
                    "top_p": request.top_p,
                    "max_tokens": request.max_tokens,
                    "request_id": request.request_id,
                },
            )

            functions = self._map_tools(request)
            function_call = self._map_tool_choice(request)

            gigachat_request = GigaChatRequest(
                model=request.model,  # type: ignore
                messages=messages,
                temperature=request.temperature,
                top_p=request.top_p,
                stream=True,  # Always stream for unified response handling
                max_tokens=request.max_tokens,
                functions=functions,
                function_call=function_call,
                # Note: GigaChat doesn't support:
                # - frequency_penalty
                # - presence_penalty
                # - stop
                # - repetition_penalty
            )

            self.logger.debug(
                "Successfully mapped provider request to GigaChat format",
                extra={
                    "request_id": request.request_id,
                },
            )
            return cast(Dict[str, Any], gigachat_request.model_dump())

        except Exception as e:
            error_request_id = str(uuid.uuid4())
            self.logger.error(
                "Failed to map request to GigaChat format",
                extra={
                    "error": str(e),
                    "request_id": error_request_id,
                },
            )
            raise ProviderError(
                code=400,
                message="Failed to map request for GigaChat",
                details={"error": str(e)},
            )

    def map_provider_stream_chunk(
        self, chunk_data: Dict, model: str, provider_id: str
    ) -> ProviderStreamChunk:
        """Map provider stream chunk to provider-agnostic format.

        Args:
            chunk_data: Raw chunk data
            model: Model name
            provider_id: Provider identifier

        Returns:
            Provider-agnostic stream chunk
        """
        return self._map_gigachat_stream_chunk(chunk_data, model, provider_id)

    def _map_gigachat_stream_chunk(
        self, chunk_data: Dict, model: str, provider_id: str
    ) -> ProviderStreamChunk:
        """Map GigaChat stream chunk to ProviderStreamChunk.

        Args:
            chunk_data: Raw chunk data
            model: Model name
            provider_id: Provider identifier

        Returns:
            Mapped ProviderStreamChunk
        """
        # 0. Получаем request_id из заголовка X-Request-ID
        request_id = chunk_data.get("request_id", str(uuid.uuid4()))
        self.logger.debug(
            "Mapping GigaChat stream chunk",
            extra={
                "model": model,
                "provider_id": provider_id,
                "request_id": request_id,
            },
        )

        # 1. Парсим 'choices' в ответе GigaChat и получаем массив GigaChatStreamChoice
        try:
            # Парсим данные чанка в GigaChatStreamResponse
            gigachat_response = GigaChatStreamResponse.model_validate(chunk_data)
            choices: List[GigaChatStreamChoice] = gigachat_response.choices
        except Exception as e:
            self.logger.error(
                "Failed to parse GigaChat stream chunk",
                extra={
                    "error": str(e),
                    "request_id": request_id,
                },
            )
            raise ProviderError(
                code=500,
                message="Failed to parse GigaChat stream chunk",
                details={"error": str(e)},
            )

        # 2. Оставшиеся поля в GigaChatStreamResponse уже распарсены выше

        # 3. Мапим GigaChatStreamChoice на Choice, перемапливая functions на tools
        provider_choices: List[StreamChoice] = []
        for gigachat_choice in choices:
            index = gigachat_choice.index
            finish_reason = gigachat_choice.finish_reason

            # Мапим GigaChatDelta на ProviderDeltaMessage
            delta = gigachat_choice.delta

            self.logger.debug(
                "Processing GigaChat delta",
                extra={
                    "has_function_call": delta.function_call is not None,
                    "function_name": delta.function_call.name
                    if delta.function_call
                    else None,
                    "functions_state_id": delta.functions_state_id,
                    "role": delta.role,
                    "has_content": delta.content is not None,
                    "request_id": request_id,
                },
            )

            # Для последующих чанков role может отсутствовать,
            # используем assistant по умолчанию
            # Мапим function_call на tool_calls
            tool_calls = self._map_function_call_to_tool_calls(
                delta.function_call, delta.functions_state_id
            )

            self.logger.debug(
                "Mapped function call to tool calls",
                extra={
                    "has_tool_calls": tool_calls is not None,
                    "tool_calls_count": len(tool_calls) if tool_calls else 0,
                    "tool_call_id": tool_calls[0].id if tool_calls else None,
                    "tool_call_name": tool_calls[0].function.name
                    if tool_calls
                    else None,
                    "request_id": request_id,
                },
            )

            # Создаем AssistantMessage с content или tool_calls, но не с обоими
            provider_delta = AssistantMessage(
                role=delta.role if delta.role else "assistant",
                content=None
                if tool_calls
                else (delta.content if delta.content is not None else ""),
                tool_calls=tool_calls,
            )

            self.logger.debug(
                "Created AssistantMessage",
                extra={
                    "role": provider_delta.role,
                    "has_content": provider_delta.content is not None,
                    "has_tool_calls": provider_delta.tool_calls is not None,
                    "request_id": request_id,
                },
            )

            # Маппим finish_reason: function_call -> tool_calls
            mapped_finish_reason = (
                "tool_calls" if finish_reason == "function_call" else finish_reason
            )

            # Создаем Choice
            choice = StreamChoice(
                index=index,
                finish_reason=mapped_finish_reason,
                delta=provider_delta,
            )

            provider_choices.append(choice)

        # 4. Мапим usage, если присутствует
        usage = None
        if gigachat_response.usage:
            # Create prompt_tokens_details with cached tokens information if available
            prompt_tokens_details = None
            if gigachat_response.usage.precached_prompt_tokens:
                prompt_tokens_details = PromptTokensDetails(
                    cached_tokens=gigachat_response.usage.precached_prompt_tokens
                )

            usage = Usage(
                prompt_tokens=gigachat_response.usage.prompt_tokens,
                completion_tokens=gigachat_response.usage.completion_tokens,
                total_tokens=gigachat_response.usage.total_tokens,
                prompt_tokens_details=prompt_tokens_details,
            )

        # 5. Собираем ProviderStreamChunk
        provider_chunk = ProviderStreamChunk(
            id=request_id,
            created=gigachat_response.created,
            model=model,
            provider_id=provider_id,
            request_id=request_id,
            object=ResponseType.CHAT_COMPLETION_CHUNK,
            choices=provider_choices,
            usage=usage,
        )

        self.logger.debug(
            "Successfully mapped GigaChat stream chunk to ProviderStreamChunk",
            extra={
                "chunk_id": provider_chunk.id,
                "model": model,
                "request_id": request_id,
            },
        )

        return provider_chunk

    def _map_function_call_to_tool_calls(
        self,
        function_call: Optional[GigaChatFunctionCall],
        functions_state_id: Optional[str] = None,
    ) -> Optional[List[MessageToolCall]]:
        """Мапинг function_call на tool_calls.

        Args:
            function_call: Объект function_call из GigaChatDelta
            functions_state_id: Идентификатор контекста функций

        Returns:
            Список MessageToolCall или None
        """
        if function_call is None:
            self.logger.debug(
                "No function call to map",
                extra={
                    "functions_state_id": functions_state_id,
                },
            )
            return None

        self.logger.debug(
            "Mapping function call to tool call",
            extra={
                "function_name": function_call.name,
                "has_arguments": function_call.arguments is not None,
                "arguments_type": type(function_call.arguments).__name__
                if function_call.arguments
                else None,
                "functions_state_id": functions_state_id,
            },
        )

        # Обработка tool_calls
        # У нас одна и та же модель сообщения асситента на
        # запрос и на ответ. Но валидация проходит по-разному
        # В ответе всегда есть index у tool_call, в запросе
        # индекса может не быть и это должно проходить валидацию
        # Поэтому при маппинге ответа роутера используется
        # MessageToolCall (optional index), а не
        # ResponseToolCall (required index)

        # Создаем MessageToolCall из FunctionCall
        tool_call = MessageToolCall(
            id=functions_state_id or f"gc_call_{str(uuid.uuid4())}",
            type="function",
            function=FunctionCall(
                name=function_call.name,
                arguments=json.dumps(function_call.arguments)
                if isinstance(function_call.arguments, dict)
                else function_call.arguments,
            ),
            index=0,  # Предполагаем единственный вызов функции
        )
        return [tool_call]

    def parse_sse_line(self, line: str) -> Optional[Dict[str, Any]]:
        """Parse SSE line into chunk data.

        Args:
            line: Raw SSE line

        Returns:
            Parsed chunk data or None if line should be skipped
        """
        if not line.strip():
            return None

        if line.strip() == "data: [DONE]":
            return {"event": "done"}  # Return special event

        if line.startswith("data: "):
            line = line[6:]  # Strip "data: " prefix

        try:
            chunk_data = json.loads(line)
            return cast(Dict[str, Any], chunk_data)
        except json.JSONDecodeError:
            return None
