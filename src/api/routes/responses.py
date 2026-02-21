"""OpenAI Responses API router implementation."""
import json
import time
from typing import Any, AsyncGenerator, Dict, List, Union
from uuid import uuid4

from fastapi import Request
from fastapi.responses import JSONResponse, StreamingResponse
from starlette.status import HTTP_200_OK

from ..docs import (
    RESPONSES_API_RESPONSES,
    RESPONSES_DESCRIPTION,
    RESPONSES_OPERATION_ID,
    RESPONSES_SUMMARY,
    RESPONSES_TAGS,
)
from .base import BaseRouter
from .chat_completion import ChatCompletionRouter
from core.config import Settings
from core.logger import LoggerService
from providers.manager import ProviderManager
from providers.models import ProviderError
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
from router.responses.models import OpenAIResponsesRequest
from router.responses.models.openai.request import (
    ResponsesInputFunctionCall,
    ResponsesInputFunctionCallOutput,
    ResponsesInputMessage,
)
from router.responses.models.openai.response import (
    OpenAIResponsesResponse,
    ResponsesOutputFunctionCall,
    ResponsesOutputMessage,
    ResponsesOutputRefusal,
    ResponsesOutputText,
    ResponsesUsage,
    ResponsesUsageInputTokensDetails,
    ResponsesUsageOutputTokensDetails,
)


class ResponsesRouter(BaseRouter):
    """OpenAI Responses API router implementation."""

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

        super().__init__(logger=logger, prefix="", tags=["responses"])

    def _setup_routes(self) -> None:
        """Setup OpenAI Responses API endpoint."""
        path = (
            "/v1/responses"
            if self.settings.ENABLE_OPENAI_COMPATIBLE_API
            else "/api/v1/responses"
        )
        self.router.add_api_route(
            path,
            self._create_openai_response,
            methods=["POST"],
            response_model=OpenAIResponsesResponse,
            responses=RESPONSES_API_RESPONSES,
            summary=RESPONSES_SUMMARY,
            description=RESPONSES_DESCRIPTION,
            operation_id=RESPONSES_OPERATION_ID,
            tags=RESPONSES_TAGS,
        )

    @staticmethod
    def _normalize_tool_output(output: Any) -> str:
        parsed: Any = output
        if isinstance(output, str):
            try:
                parsed = json.loads(output)
            except json.JSONDecodeError:
                parsed = output
        payload = parsed if isinstance(parsed, dict) else {"output": parsed}
        return json.dumps(payload, ensure_ascii=False)

    @staticmethod
    def _extract_call_id_to_name(input_messages: List[Any]) -> Dict[str, str]:
        call_id_to_name: Dict[str, str] = {}
        for msg in input_messages:
            if isinstance(msg, ResponsesInputFunctionCall):
                call_id_to_name[msg.call_id] = msg.name
                continue
            if isinstance(msg, dict) and msg.get("type") == "function_call":
                call_id = msg.get("call_id")
                name = msg.get("name")
                if call_id and name:
                    call_id_to_name[call_id] = name
        return call_id_to_name

    @staticmethod
    def _merge_system_messages(messages: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        merged_messages: List[Dict[str, Any]] = []
        system_parts: List[str] = []
        first_system_index: Union[int, None] = None

        for msg in messages:
            if msg.get("role") != "system":
                merged_messages.append(msg)
                continue

            if first_system_index is None:
                first_system_index = len(merged_messages)

            content = msg.get("content")
            if isinstance(content, str) and content:
                system_parts.append(content)

        if first_system_index is None:
            return merged_messages

        merged_messages.insert(
            first_system_index,
            {"role": "system", "content": "\n\n".join(system_parts)},
        )
        return merged_messages

    def _build_openai_messages(
        self, request: OpenAIResponsesRequest
    ) -> List[Dict[str, Any]]:
        messages: List[Dict[str, Any]] = []

        if request.instructions:
            messages.append({"role": "system", "content": request.instructions})

        if isinstance(request.input, str):
            messages.append({"role": "user", "content": request.input})
            return self._merge_system_messages(messages)

        input_messages = request.input
        if not isinstance(input_messages, list):
            input_messages = [input_messages]

        call_id_to_name = self._extract_call_id_to_name(input_messages)

        for msg in input_messages:
            if isinstance(msg, ResponsesInputFunctionCall):
                messages.append(
                    {
                        "role": "assistant",
                        "tool_calls": [
                            {
                                "id": msg.call_id,
                                "type": "function",
                                "function": {
                                    "name": msg.name,
                                    "arguments": msg.arguments,
                                },
                            }
                        ],
                    }
                )
                continue

            if isinstance(msg, ResponsesInputFunctionCallOutput):
                output_name = call_id_to_name.get(msg.call_id)
                output_content = self._normalize_tool_output(msg.output)
                tool_message: Dict[str, Any] = {
                    "role": "tool",
                    "tool_call_id": msg.call_id,
                    "content": output_content,
                }
                if output_name:
                    tool_message["name"] = output_name
                messages.append(tool_message)
                continue

            if isinstance(msg, dict):
                msg_type = msg.get("type")
                if msg_type == "function_call":
                    call_id = msg.get("call_id")
                    name = msg.get("name")
                    if call_id and name:
                        arguments = msg.get("arguments", "")
                        if not isinstance(arguments, str):
                            arguments = json.dumps(arguments, ensure_ascii=False)
                        messages.append(
                            {
                                "role": "assistant",
                                "tool_calls": [
                                    {
                                        "id": call_id,
                                        "type": "function",
                                        "function": {
                                            "name": name,
                                            "arguments": arguments,
                                        },
                                    }
                                ],
                            }
                        )
                    continue
                if msg_type == "function_call_output":
                    call_id = msg.get("call_id")
                    if not call_id:
                        continue
                    output_name = (
                        msg.get("name") or call_id_to_name.get(call_id)
                    )
                    output_content = self._normalize_tool_output(msg.get("output", ""))
                    tool_message = {
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": output_content,
                    }
                    if output_name:
                        tool_message["name"] = output_name
                    messages.append(tool_message)
                    continue
                role = msg.get("role")
                if role in {"user", "assistant", "system", "developer"}:
                    normalized_role = "system" if role == "developer" else role
                    content = msg.get("content", "")
                    if isinstance(content, list):
                        content = "".join(
                            part.get("text", "")
                            for part in content
                            if isinstance(part, dict)
                        )
                    messages.append({"role": normalized_role, "content": content})
                continue

            if not isinstance(msg, ResponsesInputMessage):
                continue

            role = "system" if msg.role == "developer" else msg.role

            if isinstance(msg.content, str):
                content = msg.content
            else:
                content = "".join(part.text for part in msg.content)

            messages.append({"role": role, "content": content})

        return self._merge_system_messages(messages)

    @staticmethod
    def _build_usage_payload(usage: Any) -> Union[ResponsesUsage, None]:
        if not usage:
            return None

        cached_tokens = (
            usage.prompt_tokens_details.cached_tokens
            if usage.prompt_tokens_details
            else None
        )
        reasoning_tokens = (
            usage.completion_tokens_details.reasoning_tokens
            if usage.completion_tokens_details
            else None
        )
        return ResponsesUsage(
            input_tokens=usage.prompt_tokens,
            output_tokens=usage.completion_tokens,
            total_tokens=usage.total_tokens,
            input_tokens_details=(
                ResponsesUsageInputTokensDetails(cached_tokens=cached_tokens)
                if cached_tokens is not None
                else None
            ),
            output_tokens_details=(
                ResponsesUsageOutputTokensDetails(reasoning_tokens=reasoning_tokens)
                if reasoning_tokens is not None
                else None
            ),
        )

    def _map_to_chat_completion_request(
        self, request: OpenAIResponsesRequest
    ) -> Union[OpenAIRequest, LLMGatewayRequest]:
        payload: Dict[str, Any] = {
            "model": request.model,
            "messages": self._build_openai_messages(request),
            "stream": request.stream,
            "temperature": request.temperature,
            "top_p": request.top_p,
            "max_tokens": request.max_output_tokens,
            "tools": request.tools,
            "tool_choice": request.tool_choice,
        }

        if request.reasoning and request.reasoning.effort:
            if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
                payload["reasoning_effort"] = request.reasoning.effort
            else:
                payload["reasoning"] = {"effort": request.reasoning.effort}

        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            return OpenAIRequest.model_validate(payload)
        return LLMGatewayRequest.model_validate(payload)

    def _map_to_responses_response(
        self,
        response: Union[OpenAIResponse, LLMGatewayResponse],
        request: OpenAIResponsesRequest,
        response_id: str,
        item_id: str,
    ) -> OpenAIResponsesResponse:
        output_items: List[
            Union[ResponsesOutputMessage, ResponsesOutputFunctionCall]
        ] = []
        output_text = ""

        if response.choices:
            message = response.choices[0].message
            content_items: List[Union[ResponsesOutputText, ResponsesOutputRefusal]] = []

            if message.content:
                output_text = message.content
                content_items.append(ResponsesOutputText(text=message.content))

            if message.refusal:
                content_items.append(ResponsesOutputRefusal(refusal=message.refusal))

            output_items.append(
                ResponsesOutputMessage(
                    id=item_id,
                    status="completed",
                    role="assistant",
                    content=content_items,
                )
            )

            if message.tool_calls:
                for idx, tool_call in enumerate(message.tool_calls):
                    if not tool_call.function or not tool_call.function.name:
                        continue
                    call_id = tool_call.id or f"call_{idx}"
                    output_items.append(
                        ResponsesOutputFunctionCall(
                            id=f"fc_{call_id}",
                            call_id=call_id,
                            name=tool_call.function.name,
                            arguments=tool_call.function.arguments or "",
                        )
                    )

        usage = self._build_usage_payload(response.usage)

        return OpenAIResponsesResponse(
            id=response_id,
            created_at=response.created,
            status="completed",
            model=response.model,
            output=output_items,
            usage=usage,
            instructions=request.instructions,
            max_output_tokens=request.max_output_tokens,
            temperature=request.temperature,
            top_p=request.top_p,
            tools=request.tools,
            tool_choice=request.tool_choice,
            output_text=output_text or None,
        )

    def _sse_event(self, event_type: str, data: Dict[str, Any]) -> bytes:
        return (
            f"event: {event_type}\n"
            f"data: {json.dumps(data, ensure_ascii=False)}\n\n"
        ).encode("utf-8")

    def _build_completed_stream_payloads(
        self,
        output_items: List[Union[ResponsesOutputMessage, ResponsesOutputFunctionCall]],
        item_id: str,
        response_id: str,
        created_at: int,
        aggregated_text: str,
        usage_payload: Union[ResponsesUsage, None],
        request: OpenAIResponsesRequest,
    ) -> tuple[dict, dict]:
        final_item = ResponsesOutputMessage(
            id=item_id,
            status="completed",
            role="assistant",
            content=[ResponsesOutputText(text=aggregated_text)],
        ).model_dump()
        output_items[0] = ResponsesOutputMessage.model_validate(final_item)
        completed_response = OpenAIResponsesResponse(
            id=response_id,
            created_at=created_at,
            status="completed",
            model=request.model,
            output=output_items,
            usage=usage_payload,
            instructions=request.instructions,
            max_output_tokens=request.max_output_tokens,
            temperature=request.temperature,
            top_p=request.top_p,
            tools=request.tools,
            tool_choice=request.tool_choice,
            output_text=aggregated_text or None,
        ).model_dump()
        return final_item, completed_response

    async def _emit_completion_events(
        self,
        item_id: str,
        aggregated_text: str,
        final_item: dict,
        completed_response: dict,
    ) -> AsyncGenerator[bytes, None]:
        yield self._sse_event(
            "response.output_text.done",
            {
                "type": "response.output_text.done",
                "output_index": 0,
                "item_id": item_id,
                "content_index": 0,
                "text": aggregated_text,
            },
        )
        yield self._sse_event(
            "response.output_item.done",
            {
                "type": "response.output_item.done",
                "output_index": 0,
                "item": final_item,
            },
        )
        yield self._sse_event(
            "response.completed",
            {
                "type": "response.completed",
                "response": completed_response,
            },
        )
        yield b"data: [DONE]\n\n"

    async def _map_stream_to_responses_events(
        self,
        stream: AsyncGenerator[bytes, None],
        request: OpenAIResponsesRequest,
    ) -> AsyncGenerator[bytes, None]:
        response_id = f"resp_{uuid4().hex}"
        item_id = f"msg_{uuid4().hex}"
        created_at = int(time.time())
        aggregated_text = ""
        usage_payload = None
        buffer = ""
        completed = False
        # TODO: Temporary compatibility hack for providers that stream
        # function arguments via chat.completions chunks (e.g. DeepSeek).
        # We buffer tool_call arguments and emit a completed function_call
        # item only when finish_reason is seen.
        # Target behavior: stream function call argument deltas in native
        # Responses API event format (arguments.delta / arguments.done).
        pending_tool_calls: Dict[str, Dict[str, Any]] = {}

        created_response = OpenAIResponsesResponse(
            id=response_id,
            created_at=created_at,
            status="in_progress",
            model=request.model,
            output=[],
            instructions=request.instructions,
            max_output_tokens=request.max_output_tokens,
            temperature=request.temperature,
            top_p=request.top_p,
            tools=request.tools,
            tool_choice=request.tool_choice,
        ).model_dump()

        yield self._sse_event(
            "response.created",
            {"type": "response.created", "response": created_response},
        )
        yield self._sse_event(
            "response.in_progress",
            {"type": "response.in_progress", "response": created_response},
        )

        output_items: List[Union[ResponsesOutputMessage, ResponsesOutputFunctionCall]] = []
        output_item = ResponsesOutputMessage(
            id=item_id,
            status="in_progress",
            role="assistant",
            content=[ResponsesOutputText(text="")],
        ).model_dump()
        output_items.append(ResponsesOutputMessage.model_validate(output_item))
        yield self._sse_event(
            "response.output_item.added",
            {
                "type": "response.output_item.added",
                "output_index": 0,
                "item": output_item,
            },
        )

        async for raw_chunk in stream:
            buffer += raw_chunk.decode("utf-8")

            while "\n\n" in buffer:
                block, buffer = buffer.split("\n\n", 1)
                for line in block.splitlines():
                    if not line.startswith("data: "):
                        continue

                    payload_raw = line[len("data: ") :].strip()
                    if payload_raw == "[DONE]":
                        final_item, completed_response = (
                            self._build_completed_stream_payloads(
                                output_items=output_items,
                                item_id=item_id,
                                response_id=response_id,
                                created_at=created_at,
                                aggregated_text=aggregated_text,
                                usage_payload=usage_payload,
                                request=request,
                            )
                        )
                        async for event in self._emit_completion_events(
                            item_id=item_id,
                            aggregated_text=aggregated_text,
                            final_item=final_item,
                            completed_response=completed_response,
                        ):
                            yield event
                        completed = True
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
                            chunk = OpenAIStreamChunk.model_validate(payload)
                        else:
                            chunk = LLMGatewayStreamChunk.model_validate(payload)
                    except Exception:
                        continue

                    if chunk.usage:
                        usage_payload = self._build_usage_payload(chunk.usage)

                    finish_reason_seen = False
                    for choice in chunk.choices:
                        if choice.finish_reason:
                            finish_reason_seen = True
                        if choice.delta and choice.delta.tool_calls:
                            for tool_call in choice.delta.tool_calls:
                                tool_call_payload = tool_call.model_dump()
                                function_payload = tool_call_payload.get("function") or {}
                                call_index = tool_call_payload.get("index")
                                call_key = (
                                    f"idx:{call_index}"
                                    if call_index is not None
                                    else f"id:{tool_call_payload.get('id') or len(pending_tool_calls)}"
                                )

                                pending = pending_tool_calls.setdefault(
                                    call_key,
                                    {
                                        "id": tool_call_payload.get("id"),
                                        "name": None,
                                        "arguments": "",
                                        "emitted": False,
                                    },
                                )

                                if tool_call_payload.get("id"):
                                    pending["id"] = tool_call_payload.get("id")
                                if function_payload.get("name"):
                                    pending["name"] = function_payload.get("name")
                                if function_payload.get("arguments"):
                                    pending["arguments"] += function_payload.get("arguments")

                        delta_text = choice.delta.content if choice.delta else None
                        if not delta_text:
                            continue

                        aggregated_text += delta_text
                        yield self._sse_event(
                            "response.output_text.delta",
                            {
                                "type": "response.output_text.delta",
                                "output_index": 0,
                                "item_id": item_id,
                                "content_index": 0,
                                "delta": delta_text,
                            },
                        )

                    # DeepSeek streams tool_call arguments token-by-token. Emit
                    # function_call items only when the tool call is finished.
                    if finish_reason_seen and pending_tool_calls:
                        for pending in pending_tool_calls.values():
                            if pending.get("emitted"):
                                continue
                            call_name = pending.get("name")
                            if not call_name:
                                continue
                            call_id = pending.get("id") or f"call_{len(output_items)}"
                            function_item = ResponsesOutputFunctionCall(
                                id=f"fc_{call_id}",
                                call_id=call_id,
                                name=call_name,
                                arguments=pending.get("arguments") or "",
                            )
                            output_items.append(function_item)
                            output_index = len(output_items) - 1
                            pending["emitted"] = True
                            yield self._sse_event(
                                "response.output_item.added",
                                {
                                    "type": "response.output_item.added",
                                    "output_index": output_index,
                                    "item": function_item.model_dump(),
                                },
                            )
                            yield self._sse_event(
                                "response.output_item.done",
                                {
                                    "type": "response.output_item.done",
                                    "output_index": output_index,
                                    "item": function_item.model_dump(),
                                },
                            )
                    if finish_reason_seen and not completed:
                        final_item, completed_response = (
                            self._build_completed_stream_payloads(
                                output_items=output_items,
                                item_id=item_id,
                                response_id=response_id,
                                created_at=created_at,
                                aggregated_text=aggregated_text,
                                usage_payload=usage_payload,
                                request=request,
                            )
                        )
                        async for event in self._emit_completion_events(
                            item_id=item_id,
                            aggregated_text=aggregated_text,
                            final_item=final_item,
                            completed_response=completed_response,
                        ):
                            yield event
                        completed = True
                        return

        if not completed and (aggregated_text or usage_payload):
            final_item, completed_response = self._build_completed_stream_payloads(
                output_items=output_items,
                item_id=item_id,
                response_id=response_id,
                created_at=created_at,
                aggregated_text=aggregated_text,
                usage_payload=usage_payload,
                request=request,
            )
            async for event in self._emit_completion_events(
                item_id=item_id,
                aggregated_text=aggregated_text,
                final_item=final_item,
                completed_response=completed_response,
            ):
                yield event

    async def _create_openai_response(
        self,
        response_request: OpenAIResponsesRequest,
        fastapi_request: Request,
    ) -> Union[JSONResponse, StreamingResponse]:
        self.logger.debug(
            "Received responses request",
            extra={
                "model": response_request.model,
                "stream": response_request.stream,
                "instructions": response_request.instructions,
                "input": response_request.input,
            },
        )
        chat_request = self._map_to_chat_completion_request(response_request)

        if self.settings.ENABLE_OPENAI_COMPATIBLE_API:
            if not isinstance(chat_request, OpenAIRequest):
                raise ProviderError(
                    code=500,
                    message="Internal request mapping error",
                    details={"error": "Expected OpenAIRequest"},
                )
            response = await self.chat_completion_router._create_openai_chat_completion(
                chat_request,
                fastapi_request,
            )
        else:
            if not isinstance(chat_request, LLMGatewayRequest):
                raise ProviderError(
                    code=500,
                    message="Internal request mapping error",
                    details={"error": "Expected LLMGatewayRequest"},
                )
            response = await self.chat_completion_router._create_LLMGateway_chat_completion(
                chat_request,
                fastapi_request,
            )

        response_id = f"resp_{uuid4().hex}"
        item_id = f"msg_{uuid4().hex}"

        if response_request.stream:
            if not isinstance(response, StreamingResponse):
                raise ProviderError(
                    code=500,
                    message="Expected streaming response",
                    details={"error": "Internal stream mapping error"},
                )

            return StreamingResponse(
                content=self._map_stream_to_responses_events(
                    response.body_iterator,  # type: ignore[arg-type]
                    response_request,
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

        mapped_response = self._map_to_responses_response(
            chat_response,
            response_request,
            response_id=response_id,
            item_id=item_id,
        )

        return JSONResponse(
            status_code=HTTP_200_OK,
            content=mapped_response.model_dump(),
        )
