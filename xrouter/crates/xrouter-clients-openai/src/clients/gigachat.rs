use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio::sync::Mutex;
use tracing::{debug, info};
use uuid::Uuid;
use xrouter_contracts::{
    ResponseInputContent, ResponseInputItem, ResponsesInput, ToolCall, ToolFunction,
};
use xrouter_core::{
    CoreError, ProviderClient, ProviderGenerateRequest, ProviderGenerateStreamRequest,
    ProviderOutcome,
};

use crate::HttpRuntime;

const GIGACHAT_OAUTH_URL: &str = "https://ngw.devices.sberbank.ru:9443/api/v2/oauth";
const GIGACHAT_DEFAULT_SCOPE: &str = "GIGACHAT_API_PERS";
const TOKEN_REFRESH_BUFFER_MS: i64 = 60_000;

pub struct GigachatClient {
    runtime: HttpRuntime,
    scope: String,
    token_state: Arc<Mutex<Option<GigachatToken>>>,
}

impl GigachatClient {
    pub fn new(
        base_url: Option<String>,
        authorization_key: Option<String>,
        scope: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        Self {
            runtime: HttpRuntime::new(
                "gigachat".to_string(),
                base_url,
                authorization_key,
                http_client,
                max_inflight,
            ),
            scope: scope.unwrap_or_else(|| GIGACHAT_DEFAULT_SCOPE.to_string()),
            token_state: Arc::new(Mutex::new(None)),
        }
    }

    async fn access_token(&self) -> Result<String, CoreError> {
        let now_ms = current_time_millis();
        let mut guard = self.token_state.lock().await;
        if let Some(token) = guard.as_ref()
            && token.expires_at_ms > now_ms + TOKEN_REFRESH_BUFFER_MS
        {
            return Ok(token.access_token.clone());
        }

        let authorization_key = self.runtime.api_key().ok_or_else(|| {
            CoreError::Provider("provider api_key is not configured for gigachat".to_string())
        })?;

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {authorization_key}")),
            ("RqUID".to_string(), Uuid::new_v4().to_string()),
            ("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()),
        ];
        let form_fields = vec![("scope".to_string(), self.scope.clone())];

        let response: GigachatOauthResponse =
            self.runtime.post_form(GIGACHAT_OAUTH_URL, &form_fields, &headers).await?;

        let token = GigachatToken {
            access_token: response.access_token,
            expires_at_ms: response.expires_at,
        };

        let value = token.access_token.clone();
        *guard = Some(token);
        Ok(value)
    }
}

#[async_trait]
impl ProviderClient for GigachatClient {
    async fn generate(
        &self,
        request: ProviderGenerateRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let access_token = self.access_token().await?;
        let url = self.runtime.build_url("chat/completions")?;
        let (payload, normalization) = build_gigachat_payload(
            request.model,
            request.input,
            request.tools,
            request.tool_choice,
        );
        info!(
            event = "provider.request.payload.normalized",
            provider = "gigachat",
            model = request.model,
            tools_in = normalization.tools_in,
            tools_out = normalization.tools_out,
            tools_dropped = normalization.tools_dropped,
            tool_choice_in = normalization.tool_choice_in,
            tool_choice_out = normalization.tool_choice_out
        );
        if !normalization.dropped_tool_types.is_empty() {
            debug!(
                event = "provider.request.payload.normalized.details",
                provider = "gigachat",
                model = request.model,
                dropped_tool_types = ?normalization.dropped_tool_types
            );
        }
        self.runtime
            .post_chat_completions_stream(
                "request",
                &url,
                &payload,
                Some(access_token.as_str()),
                &[],
                None,
            )
            .await
    }

    async fn generate_stream(
        &self,
        request: ProviderGenerateStreamRequest<'_>,
    ) -> Result<ProviderOutcome, CoreError> {
        let access_token = self.access_token().await?;
        let url = self.runtime.build_url("chat/completions")?;
        let (payload, normalization) = build_gigachat_payload(
            request.request.model,
            request.request.input,
            request.request.tools,
            request.request.tool_choice,
        );
        info!(
            event = "provider.request.payload.normalized",
            provider = "gigachat",
            model = request.request.model,
            tools_in = normalization.tools_in,
            tools_out = normalization.tools_out,
            tools_dropped = normalization.tools_dropped,
            tool_choice_in = normalization.tool_choice_in,
            tool_choice_out = normalization.tool_choice_out
        );
        if !normalization.dropped_tool_types.is_empty() {
            debug!(
                event = "provider.request.payload.normalized.details",
                provider = "gigachat",
                model = request.request.model,
                dropped_tool_types = ?normalization.dropped_tool_types
            );
        }
        self.runtime
            .post_chat_completions_stream(
                request.request_id,
                &url,
                &payload,
                Some(access_token.as_str()),
                &[],
                request.sender,
            )
            .await
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GigachatNormalization {
    pub(crate) tools_in: usize,
    pub(crate) tools_out: usize,
    pub(crate) tools_dropped: usize,
    pub(crate) dropped_tool_types: Vec<String>,
    pub(crate) tool_choice_in: String,
    pub(crate) tool_choice_out: String,
}

#[derive(Debug, Clone)]
struct NormalizedFunctions {
    functions: Vec<Value>,
    dropped_count: usize,
    dropped_tool_types: Vec<String>,
}

pub(crate) fn build_gigachat_payload(
    model: &str,
    input: &ResponsesInput,
    tools: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> (Value, GigachatNormalization) {
    let normalized_tools = normalize_tools_for_gigachat(tools);
    let normalized_tool_choice =
        normalize_tool_choice_for_gigachat(tool_choice, !normalized_tools.functions.is_empty());
    let messages = build_gigachat_messages(input);
    let mut payload = json!({
        "model": model,
        "messages": messages,
        "stream": true
    });
    if let Some(obj) = payload.as_object_mut() {
        if !normalized_tools.functions.is_empty() {
            obj.insert("functions".to_string(), Value::Array(normalized_tools.functions.clone()));
        }
        if let Some(choice) = normalized_tool_choice.clone() {
            obj.insert("function_call".to_string(), choice);
        }
    }
    (
        payload,
        GigachatNormalization {
            tools_in: tools.map(|t| t.len()).unwrap_or(0),
            tools_out: normalized_tools.functions.len(),
            tools_dropped: normalized_tools.dropped_count,
            dropped_tool_types: normalized_tools.dropped_tool_types,
            tool_choice_in: tool_choice
                .map(tool_choice_debug_label)
                .unwrap_or_else(|| "none".to_string()),
            tool_choice_out: normalized_tool_choice
                .as_ref()
                .map(tool_choice_debug_label)
                .unwrap_or_else(|| "none".to_string()),
        },
    )
}

fn normalize_tools_for_gigachat(tools: Option<&[Value]>) -> NormalizedFunctions {
    let mut normalized = Vec::new();
    let mut dropped_tool_types = Vec::new();
    for tool in tools.unwrap_or(&[]) {
        if let Some(function_tool) = normalize_function_tool(tool) {
            normalized.push(function_tool);
        } else {
            dropped_tool_types
                .push(tool.get("type").and_then(Value::as_str).unwrap_or("unknown").to_string());
        }
    }
    let dropped_count = dropped_tool_types.len();
    NormalizedFunctions { functions: normalized, dropped_count, dropped_tool_types }
}

fn normalize_function_tool(tool: &Value) -> Option<Value> {
    let tool_obj = tool.as_object()?;
    let kind = tool_obj.get("type").and_then(Value::as_str)?;
    if kind != "function" {
        return None;
    }
    let function_obj = tool_obj.get("function").and_then(Value::as_object);
    let name = function_obj
        .and_then(|obj| obj.get("name").and_then(Value::as_str))
        .or_else(|| tool_obj.get("name").and_then(Value::as_str))?
        .trim();
    if name.is_empty() {
        return None;
    }
    let description = function_obj
        .and_then(|obj| obj.get("description").and_then(Value::as_str))
        .or_else(|| tool_obj.get("description").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let parameters = function_obj
        .and_then(|obj| obj.get("parameters"))
        .cloned()
        .or_else(|| tool_obj.get("parameters").cloned())
        .unwrap_or_else(|| json!({"type":"object","properties":{}}));
    let mut out = Map::new();
    out.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(description) = description {
        out.insert("description".to_string(), Value::String(description));
    }
    out.insert("parameters".to_string(), parameters);
    Some(Value::Object(out))
}

fn normalize_tool_choice_for_gigachat(
    tool_choice: Option<&Value>,
    has_tools: bool,
) -> Option<Value> {
    if !has_tools {
        return None;
    }
    let choice = tool_choice?;
    if let Some(text) = choice.as_str() {
        return match text {
            "auto" | "none" => Some(Value::String(text.to_string())),
            // GigaChat function_call does not support OpenAI "required"/"any" directly.
            "required" | "any" => Some(Value::String("auto".to_string())),
            _ => None,
        };
    }
    let obj = choice.as_object()?;
    let kind = obj.get("type").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "function" => {
            let name = obj
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| {
                    obj.get("function")
                        .and_then(Value::as_object)
                        .and_then(|function| function.get("name"))
                        .and_then(Value::as_str)
                })?
                .trim();
            if name.is_empty() {
                return None;
            }
            Some(json!({ "name": name }))
        }
        "auto" => Some(Value::String("auto".to_string())),
        "none" => Some(Value::String("none".to_string())),
        _ => None,
    }
}

fn tool_choice_debug_label(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return format!("string:{text}");
    }
    if let Some(kind) = value.get("type").and_then(Value::as_str) {
        return format!("object:{kind}");
    }
    "other".to_string()
}

fn build_gigachat_messages(input: &ResponsesInput) -> Vec<Value> {
    match input {
        ResponsesInput::Text(text) => vec![json!({ "role": "user", "content": text })],
        ResponsesInput::Items(items) => map_input_items_to_gigachat_messages(items),
    }
}

fn map_input_items_to_gigachat_messages(items: &[ResponseInputItem]) -> Vec<Value> {
    let mut call_id_to_name = std::collections::HashMap::<String, String>::new();
    let mut system_parts = Vec::<String>::new();
    let mut pending_tool_call_id: Option<String> = None;
    for item in items {
        if item.kind.as_deref() == Some("function_call")
            && let (Some(call_id), Some(name)) = (item.call_id.as_deref(), item.name.as_deref())
            && !call_id.trim().is_empty()
            && !name.trim().is_empty()
        {
            call_id_to_name.insert(call_id.to_string(), name.to_string());
        }
        if is_system_like(item.role.as_deref())
            && let Some(text) = extract_input_item_text(item)
        {
            system_parts.push(text);
        }
    }

    let mut messages = Vec::<Value>::new();
    if !system_parts.is_empty() {
        messages.push(json!({
            "role": "system",
            "content": system_parts.join("\n\n")
        }));
    }

    for (idx, item) in items.iter().enumerate() {
        if is_system_like(item.role.as_deref()) {
            continue;
        }
        if is_function_call_item(item)
            && let Some(call_id) = item.call_id.as_deref().map(str::trim).filter(|v| !v.is_empty())
        {
            pending_tool_call_id = Some(call_id.to_string());
        }
        if is_function_call_output_item(item)
            && let Some(call_id) = item_call_id(item)
            && pending_tool_call_id.as_deref() == Some(call_id.as_str())
        {
            pending_tool_call_id = None;
        }
        if is_assistant_message(item)
            && pending_tool_call_id.is_some()
            && !has_tool_calls(item)
            && has_matching_tool_output_ahead(
                items,
                idx,
                pending_tool_call_id.as_deref().unwrap_or_default(),
            )
        {
            continue;
        }
        if let Some(msg) = map_item_to_gigachat_message(item, &call_id_to_name) {
            messages.push(msg);
        }
    }

    if messages.is_empty() {
        vec![
            json!({ "role": "user", "content": ResponsesInput::Items(items.to_vec()).to_canonical_text() }),
        ]
    } else {
        messages
    }
}

fn is_system_like(role: Option<&str>) -> bool {
    matches!(role, Some("system") | Some("developer"))
}

fn map_item_to_gigachat_message(
    item: &ResponseInputItem,
    call_id_to_name: &std::collections::HashMap<String, String>,
) -> Option<Value> {
    let kind = item.kind.as_deref().unwrap_or_default();
    if kind == "function_call" {
        let call_id = item.call_id.as_deref()?.trim();
        let name = item.name.as_deref()?.trim();
        if call_id.is_empty() || name.is_empty() {
            return None;
        }
        let arguments_raw = item.arguments.as_deref().unwrap_or("{}").trim();
        let arguments = serde_json::from_str::<Value>(arguments_raw)
            .unwrap_or_else(|_| Value::String(arguments_raw.to_string()));
        return Some(json!({
            "role": "assistant",
            "content": "",
            "function_call": {
                "name": name,
                "arguments": arguments
            },
            "functions_state_id": call_id
        }));
    }

    if kind == "function_call_output" || item.role.as_deref() == Some("tool") {
        let call_id = item.call_id.as_deref().map(str::trim).unwrap_or_default();
        let name = item
            .name
            .as_deref()
            .or_else(|| call_id_to_name.get(call_id).map(String::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let content = item
            .output
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| extract_input_item_text(item))?;
        let normalized_content = normalize_gigachat_function_result_content(&content);
        return Some(json!({
            "role": "function",
            "name": name,
            "content": normalized_content
        }));
    }

    let role =
        item.role.as_deref().or_else(|| if kind == "message" { Some("user") } else { None })?;
    let content = extract_input_item_text(item)?;
    Some(json!({
        "role": role,
        "content": content
    }))
}

fn extract_input_item_text(item: &ResponseInputItem) -> Option<String> {
    if let Some(text) = item.text.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        return Some(text.to_string());
    }
    let content = item.content.as_ref()?;
    match content {
        ResponseInputContent::Text(text) => {
            let text = text.trim();
            if text.is_empty() { None } else { Some(text.to_string()) }
        }
        ResponseInputContent::Parts(parts) => {
            let joined = parts
                .iter()
                .filter_map(|part| {
                    part.input_text
                        .as_deref()
                        .or(part.output_text.as_deref())
                        .or(part.text.as_deref())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                })
                .collect::<String>();
            if joined.is_empty() { None } else { Some(joined) }
        }
    }
}

fn normalize_gigachat_function_result_content(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        return serde_json::to_string(&parsed).unwrap_or_else(|_| trimmed.to_string());
    }
    serde_json::to_string(&json!({ "result": trimmed }))
        .unwrap_or_else(|_| "{\"result\":\"\"}".to_string())
}

fn is_assistant_message(item: &ResponseInputItem) -> bool {
    item.role.as_deref() == Some("assistant")
}

fn is_function_call_item(item: &ResponseInputItem) -> bool {
    item.kind.as_deref() == Some("function_call")
}

fn is_function_call_output_item(item: &ResponseInputItem) -> bool {
    item.kind.as_deref() == Some("function_call_output") || item.role.as_deref() == Some("tool")
}

fn item_call_id(item: &ResponseInputItem) -> Option<String> {
    item.call_id.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

fn has_tool_calls(item: &ResponseInputItem) -> bool {
    item.extra.get("tool_calls").and_then(Value::as_array).is_some_and(|calls| !calls.is_empty())
}

fn has_matching_tool_output_ahead(
    items: &[ResponseInputItem],
    current_index: usize,
    pending_call_id: &str,
) -> bool {
    for future in &items[current_index + 1..] {
        if is_function_call_output_item(future)
            && item_call_id(future).as_deref() == Some(pending_call_id)
        {
            return true;
        }
        if is_assistant_message(future) || future.role.as_deref() == Some("user") {
            break;
        }
    }
    false
}

#[derive(Debug, Clone)]
struct GigachatToken {
    access_token: String,
    expires_at_ms: i64,
}

#[derive(Debug, Deserialize)]
struct GigachatOauthResponse {
    access_token: String,
    expires_at: i64,
}

fn current_time_millis() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };
    duration.as_millis() as i64
}

pub(crate) fn map_gigachat_chat_completion_response_value(
    payload: &Value,
) -> Result<ProviderOutcome, CoreError> {
    let first = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| CoreError::Provider("provider returned empty choices".to_string()))?;

    let message = first.get("message").unwrap_or(first);
    let content = extract_text_content(message.get("content")).unwrap_or_default();
    let mut tool_calls = extract_tool_calls_legacy_and_openai(message);
    if tool_calls.is_empty() {
        tool_calls = extract_tool_calls_legacy_and_openai(first);
    }
    let tool_calls = if tool_calls.is_empty() { None } else { Some(tool_calls) };
    if content.is_empty() && tool_calls.is_none() {
        return Err(CoreError::Provider("provider returned empty message content".to_string()));
    }

    let output_tokens = payload
        .get("usage")
        .and_then(Value::as_object)
        .and_then(|usage| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or_else(|| {
            if content.is_empty() { 0 } else { content.split_whitespace().count() as u32 }
        });

    Ok(ProviderOutcome {
        chunks: if content.is_empty() { Vec::new() } else { vec![content] },
        output_tokens,
        reasoning: None,
        reasoning_details: None,
        tool_calls,
        emitted_live: false,
    })
}

pub(crate) fn map_gigachat_chat_completion_stream_text(
    payload: &str,
) -> Result<ProviderOutcome, CoreError> {
    let mut chunks = Vec::<String>::new();
    let mut all_content = String::new();
    let mut output_tokens = None::<u32>;
    let mut tool_calls = Vec::<ToolCall>::new();

    for event in extract_sse_data_events(payload) {
        if event == "[DONE]" {
            continue;
        }
        let parsed = serde_json::from_str::<Value>(&event)
            .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;

        if let Some(tokens) = parsed
            .get("usage")
            .and_then(Value::as_object)
            .and_then(|usage| usage.get("completion_tokens"))
            .and_then(Value::as_u64)
        {
            output_tokens = Some(tokens as u32);
        }

        for choice in parsed.get("choices").and_then(Value::as_array).into_iter().flatten() {
            if let Some(content_delta) =
                extract_text_content(choice.get("delta").and_then(|delta| delta.get("content")))
                && !content_delta.is_empty()
            {
                all_content.push_str(&content_delta);
                chunks.push(content_delta);
            }
            if let Some(content) = extract_text_content(
                choice.get("message").and_then(|message| message.get("content")),
            ) && !content.is_empty()
            {
                all_content.push_str(&content);
                chunks.push(content);
            }

            merge_tool_calls_unique(&mut tool_calls, extract_tool_calls_legacy_and_openai(choice));
            if let Some(delta) = choice.get("delta") {
                merge_tool_calls_unique(
                    &mut tool_calls,
                    extract_tool_calls_legacy_and_openai(delta),
                );
            }
            if let Some(message) = choice.get("message") {
                merge_tool_calls_unique(
                    &mut tool_calls,
                    extract_tool_calls_legacy_and_openai(message),
                );
            }
        }
    }

    let tool_calls = if tool_calls.is_empty() { None } else { Some(tool_calls) };
    if all_content.is_empty() && tool_calls.is_none() {
        return Err(CoreError::Provider("provider returned empty message content".to_string()));
    }
    let output_tokens = output_tokens.unwrap_or_else(|| {
        if all_content.is_empty() { 0 } else { all_content.split_whitespace().count() as u32 }
    });

    Ok(ProviderOutcome {
        chunks: if all_content.is_empty() { Vec::new() } else { chunks },
        output_tokens,
        reasoning: None,
        reasoning_details: None,
        tool_calls,
        emitted_live: false,
    })
}

fn merge_tool_calls_unique(into: &mut Vec<ToolCall>, incoming: Vec<ToolCall>) {
    for call in incoming {
        if into.iter().any(|existing| {
            existing.id == call.id
                || (existing.function.name == call.function.name
                    && existing.function.arguments == call.function.arguments)
        }) {
            continue;
        }
        into.push(call);
    }
}

fn extract_tool_calls_legacy_and_openai(value: &Value) -> Vec<ToolCall> {
    let mut calls = Vec::<ToolCall>::new();

    for tool in value.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
        let name = tool
            .get("function")
            .and_then(Value::as_object)
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(name) = name else {
            continue;
        };
        let arguments = tool
            .get("function")
            .and_then(Value::as_object)
            .and_then(|function| function.get("arguments"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "{}".to_string());
        let id = tool
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
        calls.push(ToolCall {
            id,
            kind: "function".to_string(),
            function: ToolFunction { name: name.to_string(), arguments },
        });
    }

    if let Some(function_call) = value.get("function_call").and_then(Value::as_object) {
        let name = function_call
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(name) = name {
            let arguments = function_call
                .get("arguments")
                .and_then(|arguments| match arguments {
                    Value::String(text) => Some(text.clone()),
                    Value::Null => None,
                    other => serde_json::to_string(other).ok(),
                })
                .unwrap_or_else(|| "{}".to_string());
            let id = value
                .get("functions_state_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|val| !val.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
            calls.push(ToolCall {
                id,
                kind: "function".to_string(),
                function: ToolFunction { name: name.to_string(), arguments },
            });
        }
    }

    calls
}

fn extract_text_content(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() { None } else { Some(text.to_string()) }
        }
        Value::Array(parts) => {
            let joined = parts
                .iter()
                .filter_map(|part| {
                    part.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| part.get("output_text").and_then(Value::as_str))
                        .or_else(|| part.get("input_text").and_then(Value::as_str))
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                })
                .collect::<String>();
            if joined.is_empty() { None } else { Some(joined) }
        }
        _ => None,
    }
}

fn extract_sse_data_events(payload: &str) -> Vec<String> {
    payload
        .replace('\r', "")
        .split("\n\n")
        .filter_map(|frame| {
            let data_lines = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .collect::<Vec<_>>();
            if data_lines.is_empty() { None } else { Some(data_lines.join("\n")) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_gigachat_payload, map_gigachat_chat_completion_response_value,
        map_gigachat_chat_completion_stream_text,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use xrouter_contracts::{ResponseInputContent, ResponseInputItem, ResponsesInput};

    #[test]
    fn gigachat_uses_functions_and_function_call_fields() {
        let input = ResponsesInput::Text("hello".to_string());
        let tools = vec![
            json!({
                "type":"function",
                "function":{
                    "name":"read_file",
                    "description":"Read file",
                    "parameters":{"type":"object","properties":{"path":{"type":"string"}}}
                }
            }),
            json!({"type":"web_search"}),
        ];
        let (payload, norm) =
            build_gigachat_payload("GigaChat-2", &input, Some(&tools), Some(&json!("auto")));
        assert_eq!(norm.tools_in, 2);
        assert_eq!(norm.tools_out, 1);
        assert_eq!(norm.tools_dropped, 1);
        assert_eq!(payload["functions"][0]["name"], "read_file");
        assert_eq!(payload["function_call"], "auto");
        assert!(payload.get("tools").is_none());
        assert!(payload.get("tool_choice").is_none());
    }

    #[test]
    fn gigachat_merges_system_and_keeps_it_first() {
        let input = ResponsesInput::Items(vec![
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("user".to_string()),
                content: Some(ResponseInputContent::Text("u1".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: BTreeMap::new(),
            },
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("system".to_string()),
                content: Some(ResponseInputContent::Text("s1".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: BTreeMap::new(),
            },
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("assistant".to_string()),
                content: Some(ResponseInputContent::Text("a1".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: BTreeMap::new(),
            },
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("developer".to_string()),
                content: Some(ResponseInputContent::Text("s2".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: BTreeMap::new(),
            },
        ]);
        let (payload, _) = build_gigachat_payload("GigaChat-2", &input, None, None);
        let messages = payload["messages"].as_array().expect("messages must be array");
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "s1\n\ns2");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "assistant");
    }

    #[test]
    fn gigachat_response_with_legacy_function_call_maps_to_tool_calls() {
        let payload = json!({
            "choices": [{
                "message": {
                    "content": "",
                    "function_call": {
                        "name": "exec_command",
                        "arguments": {"cmd":"ls -la"}
                    },
                    "functions_state_id": "call_legacy_1"
                }
            }]
        });
        let outcome = map_gigachat_chat_completion_response_value(&payload)
            .expect("legacy function_call must map");
        let calls = outcome.tool_calls.expect("tool calls must be present");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_legacy_1");
        assert_eq!(calls[0].function.name, "exec_command");
        assert_eq!(calls[0].function.arguments, r#"{"cmd":"ls -la"}"#);
    }

    #[test]
    fn gigachat_stream_with_legacy_function_call_delta_maps_to_tool_calls() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"function_call\":{\"name\":\"exec_command\",\"arguments\":{\"cmd\":\"pwd\"}},\"functions_state_id\":\"call_legacy_stream\"}}]}\n\n",
            "data: [DONE]\n\n"
        );
        let outcome = map_gigachat_chat_completion_stream_text(sse)
            .expect("legacy function_call delta must map");
        let calls = outcome.tool_calls.expect("tool calls must be present");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_legacy_stream");
        assert_eq!(calls[0].function.name, "exec_command");
        assert_eq!(calls[0].function.arguments, r#"{"cmd":"pwd"}"#);
    }

    #[test]
    fn gigachat_function_result_is_serialized_to_valid_json_string() {
        let input = ResponsesInput::Items(vec![
            ResponseInputItem {
                kind: Some("function_call".to_string()),
                role: Some("assistant".to_string()),
                content: None,
                text: None,
                output: None,
                call_id: Some("call_1".to_string()),
                name: Some("exec_command".to_string()),
                arguments: Some("{\"cmd\":\"ls\"}".to_string()),
                extra: BTreeMap::new(),
            },
            ResponseInputItem {
                kind: Some("function_call_output".to_string()),
                role: Some("tool".to_string()),
                content: None,
                text: None,
                output: Some("README.md\nmain.py".to_string()),
                call_id: Some("call_1".to_string()),
                name: Some("exec_command".to_string()),
                arguments: None,
                extra: BTreeMap::new(),
            },
        ]);
        let (payload, _) = build_gigachat_payload("GigaChat-2", &input, None, None);
        let messages = payload["messages"].as_array().expect("messages must be array");
        let function_msg =
            messages.iter().find(|m| m["role"] == "function").expect("function message must exist");
        let content = function_msg["content"].as_str().expect("content must be string");
        let parsed =
            serde_json::from_str::<Value>(content).expect("function result content must be JSON");
        assert_eq!(parsed["result"], "README.md\nmain.py");
    }

    #[test]
    fn gigachat_skips_preamble_assistant_between_call_and_result() {
        let input = ResponsesInput::Items(vec![
            ResponseInputItem {
                kind: Some("function_call".to_string()),
                role: Some("assistant".to_string()),
                content: None,
                text: None,
                output: None,
                call_id: Some("call_1".to_string()),
                name: Some("exec_command".to_string()),
                arguments: Some("{\"cmd\":\"ls\"}".to_string()),
                extra: BTreeMap::new(),
            },
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("assistant".to_string()),
                content: Some(ResponseInputContent::Text("thinking".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: BTreeMap::new(),
            },
            ResponseInputItem {
                kind: Some("function_call_output".to_string()),
                role: Some("tool".to_string()),
                content: None,
                text: None,
                output: Some("{\"ok\":true}".to_string()),
                call_id: Some("call_1".to_string()),
                name: Some("exec_command".to_string()),
                arguments: None,
                extra: BTreeMap::new(),
            },
        ]);
        let (payload, _) = build_gigachat_payload("GigaChat-2", &input, None, None);
        let messages = payload["messages"].as_array().expect("messages must be array");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[1]["role"], "function");
    }
}
