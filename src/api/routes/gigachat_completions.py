"""GigaChat-compatible completions router implementation."""

import json
from typing import Any, AsyncGenerator, Dict, List, Optional, Union
from uuid import uuid4

from fastapi import Request
from fastapi.responses import JSONResponse, StreamingResponse
from starlette.status import HTTP_200_OK

from .base import BaseRouter
from .chat_completion import ChatCompletionRouter
from core.config import Settings
from core.logger import LoggerService
from providers.manager import ProviderManager
from providers.models import ProviderError
from providers.models.base.messages import AssistantMessage
from router.chat_completion.context_service import ChatContextService
from router.chat_completion.models.llm_gateway import (
    LLMGatewayRequest,
    LLMGatewayResponse,
    LLMGatewayStreamChunk,
)
from router.chat_completion.models.openai import (
    OpenAIRequest,
    OpenAIResponse,
    OpenAIStreamChunk,
)
from providers.gigachat.models import (
    GigaChatApiAlternativeV1,
    GigaChatApiAlternativeV2,
    GigaChatApiChatRequestV1,
    GigaChatApiChatRequestV2,
    GigaChatApiChatResponseV1,
    GigaChatApiChatResponseV2,
    GigaChatApiContentV2,
    GigaChatApiFunctionCallV1,
    GigaChatApiFunctionCallV2,
    GigaChatApiGeneratedAnswerV1,
    GigaChatApiGeneratedAnswerV2,
    GigaChatApiMessageResponseV2,
    GigaChatApiMessageV1,
    GigaChatApiModelInfoV1,
    GigaChatApiModelInfoV2,
    GigaChatApiUsageV1,
    GigaChatApiUsageV2,
)


class GigaChatCompletionsRouter(BaseRouter):
    """GigaChat-compatible completions router."""

    def __init__(
        self,
        logger: LoggerService,
        provider_manager: ProviderManager,
        context_service: ChatContextService,
        settings: Settings,
    ) -> None:
        if not provider_manager:
            raise ValueError("Provider manager is required")

        self.logger = logger.get_logger(__name__)
        self.settings = settings
        self.chat_completion_router = ChatCompletionRouter(
            logger=logger,
            provider_manager=provider_manager,
            context_service=context_service,
            settings=settings,
        )

        super().__init__(logger=logger, prefix="", tags=["gigachat"])

    def _setup_routes(self) -> None:
        """Setup GigaChat-compatible API endpoints."""
        self.router.add_api_route(
            "/api/v1/gigachat/completions",
            self._create_gigachat_v1_completion,
            methods=["POST"],
            response_model=GigaChatApiChatResponseV1,
            operation_id="create_gigachat_v1_completion",
            tags=["gigachat"],
            summary="Create GigaChat V1 Completion",
            description=(
                "Compatibility endpoint for GigaChat Chat API v1. "
                "Maps request/response to internal chat.completions format."
            ),
        )

        self.router.add_api_route(
            "/api/v2/gigachat/completions",
            self._create_gigachat_v2_completion,
            methods=["POST"],
            response_model=GigaChatApiChatResponseV2,
            operation_id="create_gigachat_v2_completion",
            tags=["gigachat"],
            summary="Create GigaChat V2 Completion",
            description=(
                "Compatibility endpoint for GigaChat Chat API v2. "
                "Maps request/response to internal chat.completions format."
            ),
        )

    def _map_finish_reason(self, finish_reason: Optional[str]) -> str:
        if finish_reason == "tool_calls":
            return "function_call"
        if finish_reason:
            return finish_reason
        return "stop"

    def _extract_assistant_text(self, message: AssistantMessage) -> str:
        if message.content:
            return message.content
        if message.refusal:
            return message.refusal
        return ""

    def _usage_to_v1(self, usage: Any) -> GigaChatApiUsageV1:
        if not usage:
            return GigaChatApiUsageV1(
                prompt_tokens=0,
                completion_tokens=0,
                total_tokens=0,
                system_tokens=0,
                function_suggester_tokens=0,
                precached_prompt_tokens=0,
                unaccounted_function_suggester_tokens=0,
                developer_system_tokens=0,
            )

        cached_tokens = (
            usage.prompt_tokens_details.cached_tokens
            if usage.prompt_tokens_details
            and usage.prompt_tokens_details.cached_tokens is not None
            else 0
        )

        return GigaChatApiUsageV1(
            prompt_tokens=usage.prompt_tokens,
            completion_tokens=usage.completion_tokens,
            total_tokens=usage.total_tokens,
            system_tokens=0,
            function_suggester_tokens=0,
            precached_prompt_tokens=cached_tokens,
            unaccounted_function_suggester_tokens=0,
            developer_system_tokens=0,
        )

    def _usage_to_v2(self, usage: Any) -> GigaChatApiUsageV2:
        v1_usage = self._usage_to_v1(usage)
        return GigaChatApiUsageV2.model_validate(v1_usage.model_dump())

    def _map_functions_to_tools(self, functions: List[Any]) -> List[Dict[str, Any]]:
        tools: List[Dict[str, Any]] = []
        for fn in functions:
            parameters: Dict[str, Any] = {}
            if getattr(fn, "parameters", None):
                raw_parameters = fn.parameters
                if isinstance(raw_parameters, str):
                    try:
                        parsed = json.loads(raw_parameters)
                        if isinstance(parsed, dict):
                            parameters = parsed
                    except json.JSONDecodeError:
                        parameters = {}
                elif isinstance(raw_parameters, dict):
                    parameters = raw_parameters

            tools.append(
                {
                    "type": "function",
                    "function": {
                        "name": fn.name,
                        "description": getattr(fn, "description", None),
                        "parameters": parameters,
                    },
                }
            )
        return tools

    def _pick_explicit_tool_choice(self, messages: List[Any]) -> Optional[Dict[str, Any]]:
        for msg in messages:
            call = getattr(msg, "call", None)
            if call and getattr(call, "name", None):
                return {
                    "type": "function",
                    "function": {
                        "name": call.name,
                    },
                }
        return None

    def _map_v1_messages(self, messages: List[GigaChatApiMessageV1]) -> List[Dict[str, Any]]:
        internal_messages: List[Dict[str, Any]] = []
        pending_call_id_by_name: Dict[str, str] = {}

        for msg in messages:
            role = msg.role

            if role in {"system", "user"}:
                internal_messages.append(
                    {
                        "role": role,
                        "content": msg.content,
                    }
                )
                continue

            if role == "assistant":
                assistant_message: Dict[str, Any] = {
                    "role": "assistant",
                    "content": msg.content,
                }

                if msg.function_call and msg.function_call.name:
                    raw_args = msg.function_call.arguments
                    if raw_args is None:
                        arguments = ""
                    elif isinstance(raw_args, str):
                        arguments = raw_args
                    else:
                        arguments = json.dumps(raw_args, ensure_ascii=False)

                    call_id = f"call_{uuid4().hex}"
                    pending_call_id_by_name[msg.function_call.name] = call_id
                    assistant_message["content"] = ""
                    assistant_message["tool_calls"] = [
                        {
                            "id": call_id,
                            "type": "function",
                            "function": {
                                "name": msg.function_call.name,
                                "arguments": arguments,
                            },
                        }
                    ]

                internal_messages.append(assistant_message)
                continue

            if role == "function":
                tool_name = msg.function_name
                call_id = (
                    pending_call_id_by_name.get(tool_name or "")
                    if tool_name
                    else None
                ) or f"call_{uuid4().hex}"

                internal_messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": call_id,
                        "name": tool_name,
                        "content": msg.content or "",
                    }
                )
                continue

            # Unknown role: keep the payload but route it as user content.
            internal_messages.append(
                {
                    "role": "user",
                    "content": msg.content,
                }
            )

        return internal_messages

    def _extract_v2_content_text(self, content: List[Any]) -> str:
        text_parts: List[str] = []
        for item in content:
            if getattr(item, "text", None):
                text_parts.append(item.text)
        return "".join(text_parts)

    def _map_v2_messages(self, messages: List[Any]) -> List[Dict[str, Any]]:
        internal_messages: List[Dict[str, Any]] = []
        pending_call_id_by_name: Dict[str, str] = {}

        for msg in messages:
            role = msg.role
            content_items = msg.content or []

            combined_text = self._extract_v2_content_text(content_items)
            mapped_role = role if role in {"system", "user", "assistant"} else "user"
            if role == "reasoning":
                mapped_role = "assistant"

            if combined_text:
                internal_messages.append(
                    {
                        "role": mapped_role,
                        "content": combined_text,
                    }
                )

            for item in content_items:
                function_call = getattr(item, "function_call", None)
                if function_call and function_call.name:
                    call_id = f"call_{uuid4().hex}"
                    pending_call_id_by_name[function_call.name] = call_id
                    internal_messages.append(
                        {
                            "role": "assistant",
                            "content": "",
                            "tool_calls": [
                                {
                                    "id": call_id,
                                    "type": "function",
                                    "function": {
                                        "name": function_call.name,
                                        "arguments": function_call.arguments,
                                    },
                                }
                            ],
                        }
                    )

                function_result = getattr(item, "function_result", None)
                if function_result and function_result.name:
                    call_id = (
                        pending_call_id_by_name.get(function_result.name)
                        or f"call_{uuid4().hex}"
                    )
                    internal_messages.append(
                        {
                            "role": "tool",
                            "tool_call_id": call_id,
                            "name": function_result.name,
                            "content": function_result.result,
                        }
                    )

        return internal_messages

    def _log_ignored_fields(self, version: str, request: Any, mapped_keys: set[str]) -> None:
        ignored_top_level = sorted(
            field for field in request.model_fields_set if field not in mapped_keys
        )

        options = getattr(request, "options", None)
        ignored_options: List[str] = []
        if options:
            mapped_options = {"stream", "temperature", "top_p", "max_tokens", "reasoning_effort", "reasoning"}
            ignored_options = sorted(
                field for field in options.model_fields_set if field not in mapped_options
            )

        if ignored_top_level or ignored_options:
            self.logger.debug(
                "GigaChat fields accepted but ignored during mapping",
                extra={
                    "version": version,
                    "ignored_top_level_fields": ignored_top_level,
                    "ignored_options_fields": ignored_options,
                },
            )

    def _map_v1_to_chat_request(
        self, request: GigaChatApiChatRequestV1
    ) -> Union[OpenAIRequest, LLMGatewayRequest]:
        mapped_messages = self._map_v1_messages(request.messages)

        payload: Dict[str, Any] = {
            "model": request.model,
            "messages": mapped_messages,
            "stream": bool(request.options.stream),
            "temperature": request.options.temperature,
            "top_p": request.options.top_p,
            "max_tokens": request.options.max_tokens,
        }

        if request.functions:
            payload["tools"] = self._map_functions_to_tools(request.functions)

        tool_choice = self._pick_explicit_tool_choice(request.messages)
        if tool_choice:
            payload["tool_choice"] = tool_choice

        if request.options.reasoning_effort and request.options.reasoning_effort != "off":
            if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                payload["reasoning_effort"] = request.options.reasoning_effort
            else:
                payload["reasoning"] = {"effort": request.options.reasoning_effort}

        self._log_ignored_fields(
            version="v1",
            request=request,
            mapped_keys={"model", "messages", "functions", "options"},
        )

        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            return OpenAIRequest.model_validate(payload)
        return LLMGatewayRequest.model_validate(payload)

    def _map_v2_to_chat_request(
        self, request: GigaChatApiChatRequestV2
    ) -> Union[OpenAIRequest, LLMGatewayRequest]:
        mapped_messages = self._map_v2_messages(request.messages)

        payload: Dict[str, Any] = {
            "model": request.model,
            "messages": mapped_messages,
            "stream": bool(request.options.stream),
            "temperature": request.options.temperature,
            "top_p": request.options.top_p,
            "max_tokens": request.options.max_tokens,
        }

        if request.functions:
            payload["tools"] = self._map_functions_to_tools(request.functions)

        tool_choice = self._pick_explicit_tool_choice(request.messages)
        if tool_choice:
            payload["tool_choice"] = tool_choice

        reasoning_effort = (
            request.options.reasoning.effort
            if request.options.reasoning and request.options.reasoning.effort
            else None
        )
        if reasoning_effort:
            if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                payload["reasoning_effort"] = reasoning_effort
            else:
                payload["reasoning"] = {"effort": reasoning_effort}

        self._log_ignored_fields(
            version="v2",
            request=request,
            mapped_keys={"model", "messages", "functions", "options"},
        )

        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            return OpenAIRequest.model_validate(payload)
        return LLMGatewayRequest.model_validate(payload)

    def _assistant_to_v1_message(self, message: AssistantMessage) -> GigaChatApiMessageV1:
        function_call = None
        if message.tool_calls:
            tool_call = message.tool_calls[0]
            if tool_call.function and tool_call.function.name:
                function_call = GigaChatApiFunctionCallV1(
                    name=tool_call.function.name,
                    arguments=tool_call.function.arguments or "",
                )

        return GigaChatApiMessageV1(
            role="assistant",
            content=self._extract_assistant_text(message),
            function_call=function_call,
            reasoning_content=message.reasoning,
        )

    def _assistant_to_v2_messages(
        self, message: AssistantMessage
    ) -> List[GigaChatApiMessageResponseV2]:
        content_items: List[GigaChatApiContentV2] = []

        text = self._extract_assistant_text(message)
        if text:
            content_items.append(GigaChatApiContentV2(text=text))

        if message.tool_calls:
            for tool_call in message.tool_calls:
                if not tool_call.function or not tool_call.function.name:
                    continue
                content_items.append(
                    GigaChatApiContentV2(
                        function_call=GigaChatApiFunctionCallV2(
                            name=tool_call.function.name,
                            arguments=tool_call.function.arguments or "",
                        )
                    )
                )

        response_messages = [
            GigaChatApiMessageResponseV2(role="assistant", content=content_items)
        ]

        if message.reasoning:
            response_messages.append(
                GigaChatApiMessageResponseV2(
                    role="reasoning",
                    content=[GigaChatApiContentV2(text=message.reasoning)],
                )
            )

        return response_messages

    def _map_to_v1_response(
        self, response: Union[OpenAIResponse, LLMGatewayResponse]
    ) -> GigaChatApiChatResponseV1:
        alternatives: List[GigaChatApiAlternativeV1] = []

        for choice in response.choices:
            alternatives.append(
                GigaChatApiAlternativeV1(
                    message=self._assistant_to_v1_message(choice.message),
                    finish_reason=self._map_finish_reason(choice.finish_reason),
                    index=choice.index,
                )
            )

        return GigaChatApiChatResponseV1(
            answer=GigaChatApiGeneratedAnswerV1(
                alternatives=alternatives,
                usage=self._usage_to_v1(response.usage),
                model_info=GigaChatApiModelInfoV1(name=response.model, version="v1"),
                timestamp=response.created,
                additional_data={},
            )
        )

    def _map_to_v2_response(
        self, response: Union[OpenAIResponse, LLMGatewayResponse]
    ) -> GigaChatApiChatResponseV2:
        alternatives: List[GigaChatApiAlternativeV2] = []

        for choice in response.choices:
            alternatives.append(
                GigaChatApiAlternativeV2(
                    messages=self._assistant_to_v2_messages(choice.message),
                    finish_reason=self._map_finish_reason(choice.finish_reason),
                    index=choice.index,
                    token_ids=[],
                )
            )

        return GigaChatApiChatResponseV2(
            answer=GigaChatApiGeneratedAnswerV2(
                alternatives=alternatives,
                usage=self._usage_to_v2(response.usage),
                model_info=GigaChatApiModelInfoV2(name=response.model, version="v2"),
                timestamp=response.created,
                additional_data={},
            )
        )

    def _delta_to_v1_message(self, delta: AssistantMessage) -> GigaChatApiMessageV1:
        function_call = None
        if delta.tool_calls:
            tool_call = delta.tool_calls[0]
            if tool_call.function and tool_call.function.name:
                function_call = GigaChatApiFunctionCallV1(
                    name=tool_call.function.name,
                    arguments=tool_call.function.arguments or "",
                )

        return GigaChatApiMessageV1(
            role=delta.role,
            content=delta.content or "",
            function_call=function_call,
            reasoning_content=delta.reasoning,
        )

    def _delta_to_v2_messages(self, delta: AssistantMessage) -> List[GigaChatApiMessageResponseV2]:
        content_items: List[GigaChatApiContentV2] = []

        if delta.content:
            content_items.append(GigaChatApiContentV2(text=delta.content))

        if delta.tool_calls:
            for tool_call in delta.tool_calls:
                if not tool_call.function or not tool_call.function.name:
                    continue
                content_items.append(
                    GigaChatApiContentV2(
                        function_call=GigaChatApiFunctionCallV2(
                            name=tool_call.function.name,
                            arguments=tool_call.function.arguments or "",
                        )
                    )
                )

        response_messages = [
            GigaChatApiMessageResponseV2(role=delta.role, content=content_items)
        ]

        if delta.reasoning:
            response_messages.append(
                GigaChatApiMessageResponseV2(
                    role="reasoning",
                    content=[GigaChatApiContentV2(text=delta.reasoning)],
                )
            )

        return response_messages

    async def _map_stream_to_gigachat_events(
        self,
        stream: AsyncGenerator[bytes, None],
        version: str,
    ) -> AsyncGenerator[bytes, None]:
        buffer = ""

        async for raw_chunk in stream:
            buffer += raw_chunk.decode("utf-8")

            while "\n\n" in buffer:
                block, buffer = buffer.split("\n\n", 1)
                for line in block.splitlines():
                    if not line.startswith("data: "):
                        continue

                    payload_raw = line[len("data: ") :].strip()
                    if payload_raw == "[DONE]":
                        yield b"data: [DONE]\n\n"
                        return

                    try:
                        payload = json.loads(payload_raw)
                    except json.JSONDecodeError:
                        continue

                    if "error" in payload:
                        yield f"data: {payload_raw}\n\n".encode("utf-8")
                        yield b"data: [DONE]\n\n"
                        return

                    try:
                        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                            chunk: Union[OpenAIStreamChunk, LLMGatewayStreamChunk] = (
                                OpenAIStreamChunk.model_validate(payload)
                            )
                        else:
                            chunk = LLMGatewayStreamChunk.model_validate(payload)
                    except Exception:
                        continue

                    usage_v1 = self._usage_to_v1(chunk.usage)
                    usage_v2 = self._usage_to_v2(chunk.usage)

                    if version == "v1":
                        alternatives_v1: List[GigaChatApiAlternativeV1] = []
                        for choice in chunk.choices:
                            delta = choice.delta or AssistantMessage(
                                role="assistant",
                                content="",
                            )
                            alternatives_v1.append(
                                GigaChatApiAlternativeV1(
                                    message=self._delta_to_v1_message(delta),
                                    finish_reason=self._map_finish_reason(
                                        choice.finish_reason
                                    ),
                                    index=choice.index,
                                )
                            )

                        response_v1 = GigaChatApiChatResponseV1(
                            answer=GigaChatApiGeneratedAnswerV1(
                                alternatives=alternatives_v1,
                                usage=usage_v1,
                                model_info=GigaChatApiModelInfoV1(
                                    name=chunk.model,
                                    version="v1",
                                ),
                                timestamp=chunk.created,
                                additional_data={},
                            )
                        )
                        yield (
                            f"data: {json.dumps(response_v1.model_dump(), ensure_ascii=False)}\n\n"
                        ).encode("utf-8")
                        continue

                    alternatives_v2: List[GigaChatApiAlternativeV2] = []
                    for choice in chunk.choices:
                        delta = choice.delta or AssistantMessage(
                            role="assistant",
                            content="",
                        )
                        alternatives_v2.append(
                            GigaChatApiAlternativeV2(
                                messages=self._delta_to_v2_messages(delta),
                                finish_reason=self._map_finish_reason(choice.finish_reason),
                                index=choice.index,
                                token_ids=[],
                            )
                        )

                    response_v2 = GigaChatApiChatResponseV2(
                        answer=GigaChatApiGeneratedAnswerV2(
                            alternatives=alternatives_v2,
                            usage=usage_v2,
                            model_info=GigaChatApiModelInfoV2(
                                name=chunk.model,
                                version="v2",
                            ),
                            timestamp=chunk.created,
                            additional_data={},
                        )
                    )
                    yield (
                        f"data: {json.dumps(response_v2.model_dump(), ensure_ascii=False)}\n\n"
                    ).encode("utf-8")

    async def _dispatch_internal_chat_completion(
        self,
        chat_request: Union[OpenAIRequest, LLMGatewayRequest],
        fastapi_request: Request,
    ) -> Union[JSONResponse, StreamingResponse]:
        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            if not isinstance(chat_request, OpenAIRequest):
                raise ProviderError(
                    code=500,
                    message="Internal request mapping error",
                    details={"error": "Expected OpenAIRequest"},
                )
            return await self.chat_completion_router._create_openai_chat_completion(
                chat_request,
                fastapi_request,
            )

        if not isinstance(chat_request, LLMGatewayRequest):
            raise ProviderError(
                code=500,
                message="Internal request mapping error",
                details={"error": "Expected LLMGatewayRequest"},
            )

        return await self.chat_completion_router._create_LLMGateway_chat_completion(
            chat_request,
            fastapi_request,
        )

    async def _handle_gigachat_request(
        self,
        request: Union[GigaChatApiChatRequestV1, GigaChatApiChatRequestV2],
        fastapi_request: Request,
        version: str,
    ) -> Union[JSONResponse, StreamingResponse]:
        if version == "v1":
            if not isinstance(request, GigaChatApiChatRequestV1):
                raise ProviderError(
                    code=500,
                    message="Internal request mapping error",
                    details={"error": "Expected GigaChatApiChatRequestV1"},
                )
            chat_request = self._map_v1_to_chat_request(request)
        else:
            if not isinstance(request, GigaChatApiChatRequestV2):
                raise ProviderError(
                    code=500,
                    message="Internal request mapping error",
                    details={"error": "Expected GigaChatApiChatRequestV2"},
                )
            chat_request = self._map_v2_to_chat_request(request)

        response = await self._dispatch_internal_chat_completion(chat_request, fastapi_request)

        is_stream = bool(request.options.stream)
        if is_stream:
            if not isinstance(response, StreamingResponse):
                raise ProviderError(
                    code=500,
                    message="Expected streaming response",
                    details={"error": "Internal stream mapping error"},
                )

            return StreamingResponse(
                content=self._map_stream_to_gigachat_events(
                    response.body_iterator,  # type: ignore[arg-type]
                    version=version,
                ),
                headers={"Content-Type": "text/event-stream"},
            )

        if not isinstance(response, JSONResponse):
            raise ProviderError(
                code=500,
                message="Expected JSON response",
                details={"error": "Internal response mapping error"},
            )

        body = json.loads(response.body.decode("utf-8"))
        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            chat_response: Union[OpenAIResponse, LLMGatewayResponse] = (
                OpenAIResponse.model_validate(body)
            )
        else:
            chat_response = LLMGatewayResponse.model_validate(body)

        if version == "v1":
            mapped_response = self._map_to_v1_response(chat_response)
        else:
            mapped_response = self._map_to_v2_response(chat_response)

        return JSONResponse(
            status_code=HTTP_200_OK,
            content=mapped_response.model_dump(),
        )

    async def _create_gigachat_v1_completion(
        self,
        request: GigaChatApiChatRequestV1,
        fastapi_request: Request,
    ) -> Union[JSONResponse, StreamingResponse]:
        return await self._handle_gigachat_request(
            request=request,
            fastapi_request=fastapi_request,
            version="v1",
        )

    async def _create_gigachat_v2_completion(
        self,
        request: GigaChatApiChatRequestV2,
        fastapi_request: Request,
    ) -> Union[JSONResponse, StreamingResponse]:
        return await self._handle_gigachat_request(
            request=request,
            fastapi_request=fastapi_request,
            version="v2",
        )
