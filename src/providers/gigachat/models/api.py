"""GigaChat Chat API (gRPC contract) models for v1 and v2."""
from __future__ import annotations

from typing import Dict, List, Literal, Optional

from pydantic import BaseModel, Field


class GigaChatApiResponseFormatV1(BaseModel):
    """Response format in Chat API v1."""

    type: str
    json_schema: Optional[str] = None
    strict: Optional[bool] = None


class GigaChatApiTokenLogProbV1(BaseModel):
    """Token probability item in Chat API v1."""

    token: str
    token_id: Optional[int] = None
    logprob: float


class GigaChatApiTokenTopLogProbsV1(BaseModel):
    """Top token probabilities for one position in Chat API v1."""

    chosen: GigaChatApiTokenLogProbV1
    top_logprobs: List[GigaChatApiTokenLogProbV1] = Field(default_factory=list)


class GigaChatApiFunctionCallV1(BaseModel):
    """Function call payload in Chat API v1."""

    name: str
    arguments: str


class GigaChatApiFunctionCallExplicitV1(BaseModel):
    """Explicit function call selector in Chat API v1."""

    name: str


class GigaChatApiFileV1(BaseModel):
    """Attached file in Chat API v1."""

    content: bytes
    id: Optional[str] = None
    mime: Optional[str] = None
    name: Optional[str] = None
    type: Optional[Literal["image", "video", "audio", "doc"]] = None


class GigaChatApiMessageV1(BaseModel):
    """Message object in Chat API v1."""

    role: str
    content: str
    function_name: Optional[str] = None
    function_call: Optional[GigaChatApiFunctionCallV1] = None
    function_names: List[str] = Field(default_factory=list)
    call: Optional[GigaChatApiFunctionCallExplicitV1] = None
    logprobs: List[GigaChatApiTokenTopLogProbsV1] = Field(default_factory=list)
    files: List[GigaChatApiFileV1] = Field(default_factory=list)
    reasoning_content: Optional[str] = None
    reasoning_function_call: Optional[GigaChatApiFunctionCallV1] = None
    token_ids: List[int] = Field(default_factory=list)


class GigaChatApiFewShotExampleV1(BaseModel):
    """Few-shot function example in Chat API v1."""

    request: str
    params: str


class GigaChatApiFunctionV1(BaseModel):
    """Function descriptor in Chat API v1."""

    name: str
    description: Optional[str] = None
    parameters: str
    few_shot_examples: List[GigaChatApiFewShotExampleV1] = Field(default_factory=list)
    return_parameters: Optional[str] = None


class GigaChatApiChatOptionsV1(BaseModel):
    """Generation options in Chat API v1."""

    temperature: Optional[float] = None
    top_p: Optional[float] = None
    top_k: Optional[int] = None
    max_alternatives: Optional[int] = None
    max_tokens: Optional[int] = None
    repetition_penalty: Optional[float] = None
    update_interval: Optional[float] = None
    stream: Optional[bool] = None
    no_repeat_ngram_size: Optional[int] = None
    no_repeat_ngram_thr: Optional[float] = None
    no_repeat_ngram_window_size: Optional[int] = None
    flags: List[str] = Field(default_factory=list)
    safe_string: Optional[bool] = None
    force_non_empty_response: Optional[bool] = None
    no_repeat_ngram_penalty_multiplier: Optional[float] = None
    no_repeat_ngram_penalty_base: Optional[float] = None
    clean_whitelist_context: Optional[bool] = None
    top_logprobs: Optional[int] = None
    preset_name: Optional[str] = None
    clean_filter_context: Optional[str] = None
    whitelist_check: Optional[bool] = None
    function_schema_force: Optional[bool] = None
    ignore_default_descriptions: Optional[bool] = None
    normalize_history: Optional[bool] = None
    reasoning_effort: Optional[Literal["off", "low", "medium", "high"]] = None
    no_repeat_ngram_prev_assistant_penalty_multiplier: Optional[float] = None
    no_repeat_ngram_prev_assistant_penalty_base: Optional[float] = None


class GigaChatApiChatRequestV1(BaseModel):
    """Chat request in Chat API v1."""

    options: GigaChatApiChatOptionsV1
    model: str
    messages: List[GigaChatApiMessageV1]
    functions: List[GigaChatApiFunctionV1] = Field(default_factory=list)
    response_format: Optional[GigaChatApiResponseFormatV1] = None
    input_tokens: List[int] = Field(default_factory=list)


class GigaChatApiUsageV1(BaseModel):
    """Usage counters in Chat API v1."""

    prompt_tokens: int
    completion_tokens: int
    total_tokens: int
    system_tokens: int
    function_suggester_tokens: int
    precached_prompt_tokens: int
    unaccounted_function_suggester_tokens: int
    developer_system_tokens: int


class GigaChatApiModelInfoV1(BaseModel):
    """Model info in Chat API v1."""

    name: str
    version: str


class GigaChatApiAlternativeV1(BaseModel):
    """Alternative completion in Chat API v1."""

    message: GigaChatApiMessageV1
    finish_reason: str
    index: int


class GigaChatApiGeneratedAnswerV1(BaseModel):
    """Generated answer container in Chat API v1."""

    alternatives: List[GigaChatApiAlternativeV1]
    usage: GigaChatApiUsageV1
    model_info: GigaChatApiModelInfoV1
    timestamp: int
    additional_data: Dict[str, str] = Field(default_factory=dict)


class GigaChatApiChatResponseV1(BaseModel):
    """Streaming response envelope in Chat API v1."""

    answer: GigaChatApiGeneratedAnswerV1


class GigaChatApiResponseFormatV2(BaseModel):
    """Response format in Chat API v2."""

    type: str
    schema: Optional[str] = None
    strict: Optional[bool] = None


class GigaChatApiUserInfoV2(BaseModel):
    """User metadata in Chat API v2."""

    date: str


class GigaChatApiTokenLogProbV2(BaseModel):
    """Token probability item in Chat API v2."""

    token: str
    token_id: Optional[int] = None
    logprob: float


class GigaChatApiTokenTopLogProbsV2(BaseModel):
    """Top token probabilities for one position in Chat API v2."""

    chosen: GigaChatApiTokenLogProbV2
    top_logprobs: List[GigaChatApiTokenLogProbV2] = Field(default_factory=list)


class GigaChatApiFileV2(BaseModel):
    """Attached file in Chat API v2."""

    content: bytes
    id: Optional[str] = None
    mime: Optional[str] = None
    name: Optional[str] = None
    type: Optional[str] = None


class GigaChatApiFilesV2(BaseModel):
    """File list wrapper in Chat API v2."""

    files: List[GigaChatApiFileV2] = Field(default_factory=list)


class GigaChatApiFunctionCallV2(BaseModel):
    """Function call payload in Chat API v2."""

    name: str
    arguments: str


class GigaChatApiFunctionResultV2(BaseModel):
    """Function execution result in Chat API v2."""

    name: str
    result: str


class GigaChatApiContentV2(BaseModel):
    """Content item (oneof) in Chat API v2."""

    text: Optional[str] = None
    files: Optional[GigaChatApiFilesV2] = None
    function_call: Optional[GigaChatApiFunctionCallV2] = None
    function_result: Optional[GigaChatApiFunctionResultV2] = None
    logprobs: List[GigaChatApiTokenTopLogProbsV2] = Field(default_factory=list)


class GigaChatApiFunctionCallExplicitV2(BaseModel):
    """Explicit function call selector in Chat API v2."""

    name: str


class GigaChatApiMessageV2(BaseModel):
    """Message object in Chat API v2."""

    role: str
    content: List[GigaChatApiContentV2] = Field(default_factory=list)
    function_names: List[str] = Field(default_factory=list)
    call: Optional[GigaChatApiFunctionCallExplicitV2] = None


class GigaChatApiReasoningV2(BaseModel):
    """Reasoning options in Chat API v2."""

    effort: Optional[str] = None


class GigaChatApiFewShotExampleV2(BaseModel):
    """Few-shot function example in Chat API v2."""

    request: str
    params: str


class GigaChatApiFunctionV2(BaseModel):
    """Function descriptor in Chat API v2."""

    role: str
    name: str
    description: str
    parameters: str
    few_shot_examples: List[GigaChatApiFewShotExampleV2] = Field(default_factory=list)
    return_parameters: Optional[str] = None


class GigaChatApiChatOptionsV2(BaseModel):
    """Generation options in Chat API v2."""

    temperature: Optional[float] = None
    top_p: Optional[float] = None
    top_k: Optional[int] = None
    max_alternatives: Optional[int] = None
    max_tokens: Optional[int] = None
    repetition_penalty: Optional[float] = None
    update_interval: Optional[float] = None
    stream: Optional[bool] = None
    no_repeat_ngram_size: Optional[int] = None
    no_repeat_ngram_thr: Optional[float] = None
    no_repeat_ngram_window_size: Optional[int] = None
    flags: List[str] = Field(default_factory=list)
    no_repeat_ngram_penalty_multiplier: Optional[float] = None
    no_repeat_ngram_penalty_base: Optional[float] = None
    top_logprobs: Optional[int] = None
    preset_name: Optional[str] = None
    clean_filter_context: Optional[str] = None
    function_schema_force: Optional[bool] = None
    ignore_default_descriptions: Optional[bool] = None
    normalize_history: Optional[bool] = None
    reasoning: Optional[GigaChatApiReasoningV2] = None
    no_repeat_ngram_prev_assistant_penalty_multiplier: Optional[float] = None
    no_repeat_ngram_prev_assistant_penalty_base: Optional[float] = None


class GigaChatApiChatRequestV2(BaseModel):
    """Chat request in Chat API v2."""

    options: GigaChatApiChatOptionsV2
    model: str
    messages: List[GigaChatApiMessageV2]
    functions: List[GigaChatApiFunctionV2] = Field(default_factory=list)
    response_format: Optional[GigaChatApiResponseFormatV2] = None
    user_info: Optional[GigaChatApiUserInfoV2] = None
    input_tokens: List[int] = Field(default_factory=list)


class GigaChatApiMessageResponseV2(BaseModel):
    """Message object in Chat API v2 response."""

    role: str
    content: List[GigaChatApiContentV2] = Field(default_factory=list)


class GigaChatApiUsageV2(BaseModel):
    """Usage counters in Chat API v2."""

    prompt_tokens: int
    completion_tokens: int
    total_tokens: int
    system_tokens: int
    function_suggester_tokens: int
    precached_prompt_tokens: int
    unaccounted_function_suggester_tokens: int
    developer_system_tokens: int


class GigaChatApiModelInfoV2(BaseModel):
    """Model info in Chat API v2."""

    name: str
    version: str


class GigaChatApiAlternativeV2(BaseModel):
    """Alternative completion in Chat API v2."""

    messages: List[GigaChatApiMessageResponseV2] = Field(default_factory=list)
    finish_reason: str
    index: int
    token_ids: List[int] = Field(default_factory=list)


class GigaChatApiGeneratedAnswerV2(BaseModel):
    """Generated answer container in Chat API v2."""

    alternatives: List[GigaChatApiAlternativeV2]
    usage: GigaChatApiUsageV2
    model_info: GigaChatApiModelInfoV2
    timestamp: int
    additional_data: Dict[str, str] = Field(default_factory=dict)


class GigaChatApiChatResponseV2(BaseModel):
    """Streaming response envelope in Chat API v2."""

    answer: GigaChatApiGeneratedAnswerV2
