"""GigaChat API models."""
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Literal, Optional, Union

from pydantic import AliasChoices, BaseModel, Field


class GigaChatFunctionCallFunction(BaseModel):
    """Function call specification."""

    name: str = Field(description="Название функции")
    partial_arguments: Optional[Dict[str, Any]] = Field(
        None,
        description=(
            "Частичные аргументы для вызова функции. "
            "Остальные аргументы модель сгенерирует самостоятельно"
        ),
    )


class GigaChatFunctionCall(BaseModel):
    """Function call specification."""

    name: str = Field(description="Название функции")
    arguments: Optional[Dict[str, Any]] = Field(
        None,
        description=("Аргументы для вызова функции. "),
    )


class GigaChatFunction(BaseModel):
    """Function definition."""

    name: str = Field(
        pattern=r"^[a-zA-Z0-9_-]+$",
        max_length=64,
        description=(
            "The name of the function to be called. Must be a-z, A-Z, 0-9, "
            "or contain underscores and dashes, with a maximum length of 64"
        ),
    )
    description: Optional[str] = Field(
        None,
        description=(
            "A description of what the function does, used by the model to choose "
            "when and how to call the function"
        ),
    )
    parameters: Optional[Dict[str, Any]] = Field(
        None,
        description=(
            "The parameters the functions accepts, described as a JSON Schema object. "
            "Omitting parameters defines a function with an empty parameter list"
        ),
    )


class GigaChatObjectType(str, Enum):
    """GigaChat object types."""

    CHAT_COMPLETION = "chat.completion"
    CHAT_COMPLETION_CHUNK = "chat.completion"
    CHAT_COMPLETIONS = "chat.completions"
    LIST = "list"
    MODEL = "model"
    FILE = "file"


class GigaChatMessage(BaseModel):
    """GigaChat message model."""

    role: Literal["system", "user", "assistant", "function"] = Field(
        description=(
            "Message author role: system prompt, user message, "
            "assistant response, or function result"
        )
    )
    content: Optional[str] = Field(
        None,
        description=(
            "Message content or function result as JSON object for function role"
        ),
    )
    name: Optional[str] = Field(
        None, description="Optional name for the message author"
    )
    function_call: Optional[GigaChatFunctionCall] = Field(
        None, description="Information about function call including name and arguments"
    )
    functions_state_id: Optional[str] = Field(
        None,
        description=(
            "Function context identifier for maintaining function call context"
        ),
    )
    attachments: Optional[List[str]] = Field(
        None, description="List of file identifiers to use in generation"
    )


class GigaChatRequest(BaseModel):
    """GigaChat request model."""

    model: Literal[
        "GigaChat",
        "GigaChat-Pro",
        "GigaChat-Max",
        "GigaChat-2",
        "GigaChat-2-Pro",
        "GigaChat-2-Max",
    ] = Field(description="Model identifier to use for generation")
    messages: List[GigaChatMessage] = Field(
        description="Array of messages exchanged with the model"
    )
    function_call: Optional[
        Union[Literal["auto", "none"], GigaChatFunctionCallFunction]
    ] = Field(
        None,
        description=(
            "Controls function calling. 'none' disables function calls, "
            "'auto' lets the model decide, or specify a particular function to call"
        ),
    )
    functions: Optional[List[GigaChatFunction]] = Field(
        None,
        description=(
            "List of functions the model may call. Each function has a name, "
            "description, and parameters schema"
        ),
        max_length=128,
    )
    temperature: Optional[float] = Field(
        default=1.0,
        ge=0.0,
        le=2.0,
        description="Sampling temperature, higher values make output more random",
    )
    top_p: Optional[float] = Field(
        default=1.0,
        ge=0.0,
        le=1.0,
        description=(
            "Nucleus sampling parameter, sets probability mass of tokens to consider"
        ),
    )
    n: Optional[int] = Field(
        default=1,
        ge=1,
        le=4,
        description="Number of response variants to generate for each input message",
    )
    stream: bool = Field(
        default=True, description="Stream messages in parts using SSE protocol"
    )
    max_tokens: Optional[int] = Field(
        default=None,
        ge=0,
        description="Maximum number of tokens to use for response generation",
    )
    repetition_penalty: Optional[float] = Field(
        default=None,
        ge=0.0,
        le=2.0,
        description="Token repetition penalty, values above 1 reduce repetition",
    )
    update_interval: Optional[int] = Field(
        default=0,
        ge=0,
        description="Minimum interval in seconds between token sends in streaming mode",
    )


class GigaChatToken(BaseModel):
    """GigaChat OAuth token response."""

    access_token: str = Field(
        validation_alias=AliasChoices("access_token", "tok"),
        description="Token for request authorization",
    )
    expires_at: int = Field(
        validation_alias=AliasChoices("expires_at", "exp"),
        description="Token expiration timestamp in Unix time",
    )


class GigaChatDelta(BaseModel):
    """GigaChat delta message model for streaming."""

    role: Optional[str] = Field(
        None, description="Message author role: assistant or function"
    )
    content: Optional[str] = Field(
        None, description="Message content or function result"
    )
    functions_state_id: Optional[str] = Field(
        None,
        description="Function context identifier for maintaining function call context",
    )
    function_call: Optional[GigaChatFunctionCall] = Field(
        None, description="Information about function call including name and arguments"
    )


class GigaChatStreamChoice(BaseModel):
    """GigaChat streaming choice."""

    index: int = Field(0, description="Choice index in the array, starting from zero")
    delta: GigaChatDelta = Field(description="Partial updates to the message")
    finish_reason: Optional[
        Literal["stop", "length", "function_call", "blacklist", "error"]
    ] = Field(
        None,
        description=(
            "Причина завершения гипотезы:\n"
            "* stop — модель закончила формировать гипотезу и вернула полный ответ;\n"
            "* length — достигнут лимит токенов в сообщении;\n"
            "* function_call — указывает, что при запросе была вызвана встроенная "
            "функция или сгенерированы аргументы для пользовательской функции;\n"
            "* blacklist — запрос попадает под тематические ограничения;\n"
            "* error — ответ модели содержит невалидные аргументы пользовательской "
            "функции."
        ),
    )
    functions_state_id: Optional[str] = Field(
        None,
        description=(
            "Идентификатор, который объединяет массив функций, переданных в запросе. "
            "Возвращается в ответе модели (сообщение с role: assistant) при вызове "
            "встроенных или собственных функций. Позволяет сохранить контекст вызова "
            "функции и повысить качество работы модели."
        ),
    )
    function_call: Optional[GigaChatFunctionCall] = Field(
        None,
        description="Информация о вызове функции, включая название и аргументы",
    )


class GigaChatUsage(BaseModel):
    """GigaChat usage statistics."""

    prompt_tokens: int = Field(
        description="Количество токенов во входящем сообщении (роль user)"
    )
    completion_tokens: int = Field(
        description="Количество токенов, сгенерированных моделью (роль assistant)"
    )
    precached_prompt_tokens: Optional[int] = Field(
        None,
        description=(
            "Количество ранее закэшированных токенов, "
            "которые были использованы при обработке запроса. "
            "Кэшированные токены вычитаются из общего числа "
            "оплачиваемых токенов (поле total_tokens)"
        ),
    )
    total_tokens: int = Field(
        description=(
            "Общее число токенов, подлежащих тарификации, "
            "после вычитания кэшированных токенов "
            "(поле precached_prompt_tokens)"
        )
    )


class GigaChatStreamResponse(BaseModel):
    """GigaChat streaming response."""

    id: str = Field(
        default_factory=lambda: str(uuid.uuid4()), description="Response identifier"
    )
    choices: List[GigaChatStreamChoice] = Field(
        description="Array of streaming choices"
    )
    created: int = Field(
        default_factory=lambda: int(datetime.utcnow().timestamp()),
        description="Response creation timestamp in Unix time",
    )
    object: GigaChatObjectType = Field(
        default=GigaChatObjectType.CHAT_COMPLETION_CHUNK,
        description="Response type identifier",
    )
    usage: Optional[GigaChatUsage] = Field(
        None, description="Token usage statistics for final chunks"
    )
