"""Yandex request and response mappers."""
import json
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional, cast

from core.logger import LoggerService
from ..base_mapper import BaseMapper
from ..models import (
    AssistantMessage,
    MessageToolCall,
    ProviderConfig,
    ProviderError,
    ProviderRequest,
    ProviderStreamChunk,
    ResponseType,
    StreamChoice,
    Usage,
)
from ..models.base.common import CompletionTokensDetails
from .models import (
    YandexCompletionOptions,
    YandexFunction,
    YandexFunctionCall,
    YandexFunctionResult,
    YandexMessage,
    YandexReasoningOptions,
    YandexRequest,
    YandexResponse,
    YandexTool,
    YandexToolCall,
    YandexToolChoice,
    YandexToolCallList,
    YandexToolResult,
    YandexToolResultList,
)


class YandexMapper(BaseMapper):
    """Mapper for Yandex requests and responses."""

    # Маппинг model_id в model_name для формирования modelUri
    MODEL_MAPPING = {
        "yandexgpt5-pro:latest": "yandexgpt/latest",
        "yandexgpt5.1-pro:rc": "yandexgpt/rc",
        "yandexgpt-lite5:latest": "yandexgpt-lite/latest",
        "aliceai-llm:latest": "aliceai-llm/latest",
    }

    def __init__(self, provider: ProviderConfig, logger: LoggerService) -> None:
        """Initialize mapper.

        Args:
            provider: Provider model instance
            logger: Logger service instance
        """
        super().__init__(provider=provider, logger=logger)
        self._previous_text: Dict[str, str] = {}  # request_id -> previous text

    def _get_model_uri(self, model_id: str) -> str:
        """Get Yandex model URI.

        Args:
            model_id: Model ID from request (e.g. "YandexGPT Lite")

        Returns:
            Full model URI in format gpt://<folder_id>/<model>

        Raises:
            ProviderError: If model mapping not found or folder_id not configured
        """
        try:
            # Получаем folder_id из параметров провайдера
            folder_id = self._provider.parameters.get("folder_id")
            if not folder_id:
                raise ProviderError(
                    code=500,
                    message="Yandex folder_id not configured",
                    details={"error": "Missing folder_id in provider parameters"},
                )

            # Получаем маппинг модели по model_id
            model_name = self.MODEL_MAPPING.get(model_id.lower())
            if not model_name:
                raise ProviderError(
                    code=400,
                    message=f"Unsupported model: {model_id}",
                    details={
                        "error": (
                            f"No mapping found for model {model_id}. "
                            f"Available models: {list(self.MODEL_MAPPING.keys())}"
                        )
                    },
                )

            # Формируем полный URI
            model_uri = f"gpt://{folder_id}/{model_name}"

            self.logger.debug(
                "Mapped model to Yandex URI",
                extra={
                    "model_id": model_id,
                    "model_name": model_name,
                    "model_uri": model_uri,
                },
            )

            return model_uri

        except Exception as e:
            if isinstance(e, ProviderError):
                raise
            raise ProviderError(
                code=500,
                message="Failed to get model URI",
                details={"error": str(e)},
            )

    def _create_tool_call_list(
        self, tool_calls: List[MessageToolCall]
    ) -> YandexToolCallList:
        """Create YandexToolCallList from MessageToolCall list.

        Args:
            tool_calls: List of tool calls from the message

        Returns:
            YandexToolCallList instance
        """
        yandex_tool_calls = []
        for tool_call in tool_calls:
            yandex_tool_call = YandexToolCall(
                functionCall=YandexFunctionCall(
                    name=tool_call.function.name,
                    arguments=json.loads(tool_call.function.arguments),
                )
            )
            yandex_tool_calls.append(yandex_tool_call)
        return YandexToolCallList(toolCalls=yandex_tool_calls)

    def _create_tool_result_list(self, name: str, content: str) -> YandexToolResultList:
        """Create YandexToolResultList from tool message.

        Args:
            name: Tool name
            content: Tool result content

        Returns:
            YandexToolResultList instance
        """
        return YandexToolResultList(
            toolResults=[
                YandexToolResult(
                    functionResult=YandexFunctionResult(
                        name=name or "",
                        content=content,
                    )
                )
            ]
        )

    def _is_preamble_assistant_message(
        self,
        messages: List[Any],
        msg_index: int,
        pending_tool_call_id: Optional[str],
    ) -> bool:
        msg = messages[msg_index]
        if msg.role != "assistant" or msg.tool_calls or not pending_tool_call_id:
            return False

        for future_msg in messages[msg_index + 1 :]:
            if (
                future_msg.role == "tool"
                and getattr(future_msg, "tool_call_id", None) == pending_tool_call_id
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

    def _convert_messages_to_yandex_format(
        self, messages: List[Any]
    ) -> List[YandexMessage]:
        """Convert messages to Yandex format.

        Args:
            messages: List of messages to convert

        Returns:
            List of YandexMessage instances
        """
        self.logger.debug(
            "Starting message conversion", extra={"messages_count": len(messages)}
        )

        yandex_messages = []
        last_assistant_with_tool_call: Optional[str] = None
        for msg_idx, msg in enumerate(messages):
            if self._is_preamble_assistant_message(
                messages=messages,
                msg_index=msg_idx,
                pending_tool_call_id=last_assistant_with_tool_call,
            ):
                continue

            role = msg.role
            message = None

            self.logger.debug(
                "Processing message",
                extra={
                    "role": role,
                    "has_content": hasattr(msg, "content") and msg.content is not None,
                    "has_tool_calls": hasattr(msg, "tool_calls")
                    and bool(msg.tool_calls),
                    "has_name": hasattr(msg, "name"),
                    "content_type": type(msg.content).__name__
                    if hasattr(msg, "content") and msg.content is not None
                    else None,
                },
            )

            # В YandexMessage может быть только один из параметров:
            # text, toolCallList или toolResultList
            if role == "user":
                converted_content = self.convert_content_to_string(role, msg.content)
                message = YandexMessage(role=role, text=converted_content)
                self.logger.debug(
                    "Created user message",
                    extra={
                        "original_content_length": len(str(msg.content))
                        if msg.content
                        else 0,
                        "converted_content_length": len(converted_content)
                        if converted_content
                        else 0,
                    },
                )
            elif role == "assistant":
                if msg.tool_calls and not msg.content:
                    message = YandexMessage(
                        role=role,
                        toolCallList=self._create_tool_call_list(msg.tool_calls),
                    )
                    last_assistant_with_tool_call = msg.tool_calls[0].id
                    self.logger.debug(
                        "Created assistant message with tool calls",
                        extra={"tool_calls_count": len(msg.tool_calls)},
                    )
                elif (
                    isinstance(msg.content, str)
                    and msg.content.strip()
                    and not msg.tool_calls
                ):
                    message = YandexMessage(role=role, text=msg.content)
                    self.logger.debug(
                        "Created assistant message with text",
                        extra={
                            "content_length": len(msg.content) if msg.content else 0
                        },
                    )
                else:
                    # If both or neither are present, prefer tool_calls if available
                    if msg.tool_calls:
                        message = YandexMessage(
                            role=role,
                            toolCallList=self._create_tool_call_list(msg.tool_calls),
                        )
                        last_assistant_with_tool_call = msg.tool_calls[0].id
                        self.logger.debug(
                            "Created assistant msg with tool calls (pref over content)",
                            extra={"tool_calls_count": len(msg.tool_calls)},
                        )
                    else:
                        self.logger.debug(
                            "Skipping empty assistant message without tool calls",
                            extra={
                                "content_length": len(msg.content) if msg.content else 0
                            },
                        )
            elif role == "tool":
                message = YandexMessage(
                    role="user",  # tool role mapped to user
                    toolResultList=self._create_tool_result_list(msg.name, msg.content),
                )
                if getattr(msg, "tool_call_id", None) == last_assistant_with_tool_call:
                    last_assistant_with_tool_call = None
                self.logger.debug(
                    "Created tool message mapped to user",
                    extra={
                        "tool_name": msg.name,
                        "content_length": len(msg.content) if msg.content else 0,
                    },
                )

            if message is not None:
                yandex_messages.append(message)
                self.logger.debug(
                    "Added message to output list",
                    extra={
                        "role": message.role,
                        "has_text": message.text is not None,
                        "has_tool_call_list": message.toolCallList is not None,
                        "has_tool_result_list": message.toolResultList is not None,
                    },
                )
            else:
                self.logger.warning(
                    "Skipping message with unsupported role", extra={"role": role}
                )

        self.logger.debug(
            "Completed message conversion",
            extra={"output_messages_count": len(yandex_messages)},
        )
        return yandex_messages

    @staticmethod
    def _map_reasoning_options(request: ProviderRequest) -> Optional[YandexReasoningOptions]:
        if not request.reasoning:
            return None
        # BaseRequest reasoning config has no explicit disabled mode.
        # If reasoning is provided, treat it as enabled for Yandex.
        return YandexReasoningOptions(mode="ENABLED_HIDDEN")

    @staticmethod
    def _map_tool_choice(request: ProviderRequest) -> Optional[YandexToolChoice]:
        if not request.tool_choice:
            return None
        if isinstance(request.tool_choice, str):
            mode = request.tool_choice.upper()
            if mode in {"NONE", "AUTO", "REQUIRED"}:
                return YandexToolChoice(mode=cast(Any, mode))
            return None
        function_name = request.tool_choice.function.name
        if not function_name:
            return None
        return YandexToolChoice(functionName=function_name)

    def _create_provider_stream_chunk(
        self,
        request_id: str,
        model: str,
        provider_id: str,
        stream_choice: StreamChoice,
        usage: Optional[Usage] = None,
    ) -> ProviderStreamChunk:
        """Create ProviderStreamChunk instance.

        Args:
            request_id: Request identifier
            model: Model name
            provider_id: Provider identifier
            stream_choice: Stream choice instance
            usage: Optional usage statistics

        Returns:
            ProviderStreamChunk instance
        """
        return ProviderStreamChunk(
            id=request_id,
            created=int(datetime.utcnow().timestamp()),
            model=model,
            provider_id=provider_id,
            request_id=request_id,
            object=ResponseType.CHAT_COMPLETION_CHUNK,
            choices=[stream_choice],
            usage=usage,
        )

    def map_to_provider_request(self, request: ProviderRequest) -> Dict[str, Any]:
        """Map provider request to Yandex request.

        Args:
            request: Provider-agnostic request

        Returns:
            Yandex-specific request

        Raises:
            ProviderError: If mapping fails
        """
        try:
            if not isinstance(request, ProviderRequest):
                raise ProviderError(
                    code=400,
                    message="Failed to map request for Yandex",
                    details={"error": "Invalid request type"},
                )

            self.logger.debug(
                "Mapping provider request to Yandex format",
                extra={
                    "model": request.model,
                    "temperature": request.temperature,
                    "max_tokens": request.max_tokens,
                    "request_id": request.request_id,
                },
            )

            messages = self._convert_messages_to_yandex_format(request.messages)

            self.logger.debug(
                "Building Yandex request",
                extra={
                    "messages_count": len(messages),
                    "temperature": request.temperature,
                    "max_tokens": request.max_tokens,
                    "request_id": request.request_id,
                },
            )

            # Initialize previous text storage for this request
            self._previous_text[request.request_id] = ""

            # Map tools to Yandex format if present
            tools = None
            if request.tools:
                tools = [
                    YandexTool(
                        function=YandexFunction(
                            name=tool.function.name,
                            description=tool.function.description,
                            parameters=tool.function.parameters,
                            strict=None,
                        )
                    )
                    for tool in request.tools
                ]

            # Получаем правильный modelUri
            model_uri = self._get_model_uri(request.model)
            reasoning_options = self._map_reasoning_options(request)
            tool_choice = self._map_tool_choice(request)

            yandex_request = YandexRequest(
                modelUri=model_uri,
                messages=messages,
                completionOptions=YandexCompletionOptions(
                    stream=True,  # Always stream for unified response handling
                    temperature=request.temperature or 0.3,
                    maxTokens=request.max_tokens,
                    reasoningOptions=reasoning_options,
                ),
                tools=tools,
                toolChoice=tool_choice,
                # Not exposed in ProviderRequest yet, keep defaults until API surface is extended.
                parallelToolCalls=None,
                jsonObject=None,
                jsonSchema=None,
            )

            request_data = yandex_request.model_dump()
            self.logger.debug(
                "Successfully mapped provider request to Yandex format",
                extra={
                    "request_id": request.request_id,
                    "model_uri": model_uri,
                    "request": {
                        "modelUri": request_data["modelUri"],
                        "messages": [
                            {
                                "role": msg["role"],
                                "has_text": "text" in msg,
                                "has_tool_call_list": "toolCallList" in msg,
                                "has_tool_result_list": "toolResultList" in msg,
                            }
                            for msg in request_data["messages"]
                        ],
                        "completionOptions": request_data["completionOptions"],
                        "tools": [
                            {
                                "type": "function",
                                "function": {
                                    "name": tool["function"]["name"],
                                    "has_description": bool(
                                        tool["function"].get("description")
                                    ),
                                    "has_parameters": bool(
                                        tool["function"].get("parameters")
                                    ),
                                },
                            }
                            for tool in request_data.get("tools", [])
                        ]
                        if request_data.get("tools")
                        else None,
                    },
                },
            )
            return cast(Dict[str, Any], request_data)

        except Exception as e:
            error_request_id = str(uuid.uuid4())
            self.logger.error(
                "Failed to map request to Yandex format",
                extra={
                    "error": str(e),
                    "request_id": error_request_id,
                },
            )
            if isinstance(e, ProviderError):
                raise
            raise ProviderError(
                code=400,
                message="Failed to map request for Yandex",
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
        return self._map_yandex_stream_chunk(chunk_data, model, provider_id)

    def _map_yandex_stream_chunk(
        self, chunk_data: Dict, model: str, provider_id: str
    ) -> ProviderStreamChunk:
        """Map Yandex stream chunk to ProviderStreamChunk.

        Args:
            chunk_data: Raw chunk data
            model: Model name
            provider_id: Provider identifier

        Returns:
            Mapped ProviderStreamChunk
        """
        # Get request_id from chunk_data
        request_id = chunk_data.get("request_id", str(uuid.uuid4()))

        self.logger.debug(
            "Mapping Yandex stream chunk",
            extra={
                "model": model,
                "provider_id": provider_id,
                "request_id": request_id,
                "chunk_data_keys": list(chunk_data.keys()),
            },
        )

        # Parse response
        yandex_response = YandexResponse(**chunk_data)
        # TBD: не работаем с массивом alternatives
        alternative = yandex_response.result.alternatives[0]

        self.logger.debug(
            "Processing Yandex alternative",
            extra={
                "request_id": request_id,
                "status": alternative.status,
                "has_message": alternative.message is not None,
                "has_tool_call_list": alternative.message.toolCallList is not None
                if alternative.message
                else False,
                "has_text": alternative.message.text is not None
                if alternative.message
                else False,
            },
        )

        usage = None
        if yandex_response.result.usage:
            # Create completion_tokens_details if available
            completion_tokens_details = None
            if (
                yandex_response.result.usage.completionTokensDetails
                and yandex_response.result.usage.completionTokensDetails.reasoningTokens
            ):
                completion_tokens_details = CompletionTokensDetails(
                    reasoning_tokens=int(
                        yandex_response.result.usage.completionTokensDetails.reasoningTokens
                    )
                )

            usage = Usage(
                prompt_tokens=int(yandex_response.result.usage.inputTextTokens),
                completion_tokens=int(yandex_response.result.usage.completionTokens),
                total_tokens=int(yandex_response.result.usage.totalTokens),
                completion_tokens_details=completion_tokens_details,
            )
            self.logger.debug(
                "Parsed usage information",
                extra={
                    "request_id": request_id,
                    "prompt_tokens": usage.prompt_tokens,
                    "completion_tokens": usage.completion_tokens,
                    "total_tokens": usage.total_tokens,
                },
            )

        # Handle tool calls
        if alternative.status == "ALTERNATIVE_STATUS_TOOL_CALLS":
            tool_calls = []
            if alternative.message.toolCallList:
                for idx, tool_call in enumerate(
                    alternative.message.toolCallList.toolCalls
                ):
                    tool_calls.append(
                        MessageToolCall(
                            id=f"ya_call_{str(uuid.uuid4())}",
                            type="function",
                            function={
                                "name": tool_call.functionCall.name,
                                "arguments": json.dumps(
                                    tool_call.functionCall.arguments
                                ),
                            },
                            index=idx,
                        )
                    )

                self.logger.debug(
                    "Processed tool calls",
                    extra={
                        "request_id": request_id,
                        "tool_calls_count": len(tool_calls),
                        "tool_names": [tc.function.name for tc in tool_calls],
                    },
                )

            stream_choice = StreamChoice(
                index=0,
                delta=AssistantMessage(
                    role="assistant",
                    tool_calls=tool_calls,
                ),
                finish_reason="tool_calls",
            )
            provider_chunk = self._create_provider_stream_chunk(
                request_id, model, provider_id, stream_choice, usage
            )

        else:
            # Handle regular text response
            current_text = alternative.message.text or ""

            # Calculate delta from previous text
            previous_text = self._previous_text.get(request_id, "")

            self.logger.debug(
                "Processing text response",
                extra={
                    "request_id": request_id,
                    "has_current_text": bool(current_text),
                    "current_text_length": len(current_text),
                    "has_previous_text": bool(previous_text),
                    "previous_text_length": len(previous_text),
                    "starts_with_previous": current_text.startswith(previous_text)
                    if previous_text
                    else False,
                },
            )

            if previous_text and current_text.startswith(previous_text):
                delta_text = current_text[len(previous_text) :]
            else:
                delta_text = current_text

            self.logger.debug(
                "Calculated delta text",
                extra={
                    "request_id": request_id,
                    "delta_text_length": len(delta_text),
                },
            )

            # Update previous text for next chunk
            self._previous_text[request_id] = current_text

            # Create stream choice with delta
            stream_choice = StreamChoice(
                index=0,
                delta=AssistantMessage(
                    role=alternative.message.role,  # берем роль прямо из сообщения
                    content=delta_text,
                ),
                finish_reason="stop"
                if alternative.status == "ALTERNATIVE_STATUS_FINAL"
                else None,
            )

            provider_chunk = self._create_provider_stream_chunk(
                request_id, model, provider_id, stream_choice, usage
            )

        # Cleanup if this is the final chunk or tool calls
        if (
            alternative.status == "ALTERNATIVE_STATUS_FINAL"
            or alternative.status == "ALTERNATIVE_STATUS_TOOL_CALLS"
        ):
            self._previous_text.pop(request_id, None)
            self.logger.debug(
                "Cleaned up previous text storage",
                extra={
                    "request_id": request_id,
                    "status": alternative.status,
                },
            )

        self.logger.debug(
            "Successfully mapped stream chunk",
            extra={
                "chunk_id": provider_chunk.id,
                "model": model,
                "request_id": request_id,
                "is_final": bool(
                    alternative.status
                    in ["ALTERNATIVE_STATUS_FINAL", "ALTERNATIVE_STATUS_TOOL_CALLS"]
                ),
                "has_usage": usage is not None,
            },
        )
        return provider_chunk

    def parse_sse_line(self, line: str) -> Optional[Dict[str, Any]]:
        """Parse SSE line into chunk data.

        Args:
            line: Raw SSE line

        Returns:
            Parsed chunk data or None if line should be skipped
        """
        if not line.strip():
            return None

        # Handle SSE format (data: prefix)
        # Yandex провайдер не отправляет последний чанк [DONE]
        if line.startswith("data: "):
            line = line[6:]  # Strip "data: " prefix

        try:
            chunk_data = json.loads(line)
            return cast(Dict[str, Any], chunk_data)
        except json.JSONDecodeError:
            return None
