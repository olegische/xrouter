use std::{collections::HashMap, sync::Arc, time::Duration};

mod clients;

pub use clients::{
    DeepSeekClient, GigachatClient, MockProviderClient, OpenAiClient, OpenRouterClient,
    XrouterClient, YandexResponsesClient, ZaiClient,
};
use futures::StreamExt;
use opentelemetry::{global, propagation::Injector, trace::Status};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use tokio::sync::{Semaphore, mpsc};
use tokio::time::sleep;
use tracing::{Instrument, debug, field, info_span, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;
use xrouter_contracts::{
    ResponseEvent, ResponseInputContent, ResponseInputItem, ResponsesInput, ToolCall, ToolFunction,
};
use xrouter_core::{CoreError, ProviderOutcome};

const STREAM_DEBUG_SAMPLE_EVERY: usize = 25;
const STREAM_DEBUG_PREVIEW_LIMIT: usize = 120;
const UPSTREAM_ERROR_BODY_PREVIEW_LIMIT: usize = 600;

pub fn build_http_client(timeout_seconds: u64) -> Option<Client> {
    Client::builder().connect_timeout(Duration::from_secs(timeout_seconds)).build().ok()
}

pub fn build_http_client_insecure_tls(timeout_seconds: u64) -> Option<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(timeout_seconds))
        .danger_accept_invalid_certs(true)
        .build()
        .ok()
}

#[derive(Clone)]
struct HttpRuntime {
    provider_id: String,
    base_url: Option<String>,
    api_key: Option<String>,
    http_client: Option<Client>,
    max_inflight: Option<Arc<Semaphore>>,
}

impl HttpRuntime {
    fn new(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        let max_inflight = max_inflight.map(Semaphore::new).map(Arc::new);
        Self { provider_id, base_url, api_key, http_client, max_inflight }
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref().filter(|value| !value.trim().is_empty())
    }

    fn base_url(&self) -> Result<&str, CoreError> {
        self.base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CoreError::Provider("provider base_url is not configured".to_string()))
    }

    fn build_url(&self, path: &str) -> Result<String, CoreError> {
        let base_url = self.base_url()?.trim_end_matches('/');
        Ok(format!("{base_url}/{}", path.trim_start_matches('/')))
    }

    fn client(&self) -> Result<&Client, CoreError> {
        self.http_client
            .as_ref()
            .ok_or_else(|| CoreError::Provider("provider client init failed".to_string()))
    }

    fn acquire_inflight_permit(
        &self,
    ) -> Result<Option<tokio::sync::OwnedSemaphorePermit>, CoreError> {
        self.max_inflight
            .as_ref()
            .map(|semaphore| {
                semaphore.clone().try_acquire_owned().map_err(|_| {
                    CoreError::Provider(format!(
                        "provider overloaded: max in-flight limit reached for {}",
                        self.provider_id
                    ))
                })
            })
            .transpose()
    }

    async fn send_post(
        &self,
        request_id: &str,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
    ) -> Result<reqwest::Response, CoreError> {
        let _permit = self.acquire_inflight_permit()?;
        for attempt in 1..=2 {
            let client = self.client()?;
            let http_span = info_span!(
                "provider_http_request",
                otel.name = field::Empty,
                otel.kind = "client",
                request.id = request_id,
                provider.request_id = request_id,
                provider = %self.provider_id,
                http.method = "POST",
                http.url = url,
                http.retry_count = attempt - 1,
                http.response.status_code = field::Empty
            );
            http_span.record("otel.name", "provider_http_request");

            let response = async {
                let mut request =
                    client.post(url).header("Content-Type", "application/json").json(payload);
                request = inject_trace_headers(request);
                if let Some(token) = bearer_override.or(self.api_key()) {
                    request = request.bearer_auth(token);
                }
                for (name, value) in extra_headers {
                    request = request.header(name, value);
                }
                request
                    .send()
                    .await
                    .map_err(|err| CoreError::Provider(format!("provider request failed: {err}")))
            }
            .instrument(http_span.clone())
            .await;
            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    http_span.set_status(Status::error(error.to_string()));
                    return Err(error);
                }
            };
            let status = response.status();
            http_span.record("http.response.status_code", status.as_u16());
            if status.is_success() {
                return Ok(response);
            }

            let body = response.text().await.unwrap_or_default();
            let body_preview = truncate_for_debug(
                body.replace('\n', "\\n").replace('\r', "\\r").as_str(),
                UPSTREAM_ERROR_BODY_PREVIEW_LIMIT,
            );
            let retryable = should_retry_failed_status(&self.provider_id, status, &body, attempt);
            warn!(
                event = "provider.request.failed_status",
                provider = %self.provider_id,
                url = url,
                status = %status,
                body_bytes = body.len(),
                attempt = attempt,
                retryable = retryable,
            );
            debug!(
                event = "provider.request.failed_status.body",
                provider = %self.provider_id,
                url = url,
                status = %status,
                attempt = attempt,
                body_preview = %body_preview,
            );

            if retryable {
                warn!(
                    event = "provider.request.retrying",
                    provider = %self.provider_id,
                    url = url,
                    status = %status,
                    attempt = attempt,
                    next_attempt = attempt + 1,
                );
                sleep(Duration::from_millis(300)).await;
                continue;
            }

            let reason = status.canonical_reason().unwrap_or("Unknown");
            http_span.set_status(Status::error(format!(
                "provider returned error status: {status} ({reason})"
            )));
            return Err(CoreError::Provider(format!(
                "provider returned error status: {status} ({reason}) for url ({url})"
            )));
        }

        Err(CoreError::Provider(format!(
            "provider returned retryable error status after retries for url ({url})"
        )))
    }

    async fn post_chat_completions_stream(
        &self,
        request_id: &str,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
        sender: Option<&mpsc::Sender<Result<ResponseEvent, CoreError>>>,
    ) -> Result<ProviderOutcome, CoreError> {
        let request_span = info_span!(
            "provider_stream_request",
            otel.name = "provider_stream_request",
            otel.kind = "internal",
            request.id = request_id,
            provider.request_id = request_id,
            provider = %self.provider_id,
            request_id = request_id,
            stream_kind = "chat_completions"
        );
        let response = self
            .send_post(request_id, url, payload, bearer_override, extra_headers)
            .instrument(request_span)
            .await?;
        let is_json = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("application/json"));

        if is_json {
            if self.provider_id == "gigachat" {
                let payload = response.json::<Value>().await.map_err(|err| {
                    CoreError::Provider(format!("provider response parse failed: {err}"))
                })?;
                return crate::clients::gigachat::map_gigachat_chat_completion_response_value(
                    &payload,
                );
            }
            let payload = response.json::<ChatCompletionsResponse>().await.map_err(|err| {
                CoreError::Provider(format!("provider response parse failed: {err}"))
            })?;
            return map_chat_completion_response(payload);
        }

        let mut all_chunks = Vec::<String>::new();
        let mut parse_buffer = String::new();
        let mut full_body = String::new();
        let mut stream = response.bytes_stream();
        let mut transport_chunk_index = 0usize;
        let mut delta_count = 0usize;
        while let Some(next) = stream.next().await {
            let bytes = next.map_err(|err| {
                CoreError::Provider(format!("provider stream read failed: {err}"))
            })?;
            transport_chunk_index += 1;
            let chunk = String::from_utf8_lossy(&bytes).replace('\r', "");
            if should_log_stream_chunk_debug(transport_chunk_index) {
                debug!(
                    event = "provider.stream.chunk.received",
                    provider = %self.provider_id,
                    request_id = request_id,
                    stream_kind = "chat_completions",
                    chunk_index = transport_chunk_index,
                    chunk_bytes = bytes.len(),
                    chunk_preview = %truncate_for_debug(&chunk, STREAM_DEBUG_PREVIEW_LIMIT)
                );
            }
            parse_buffer.push_str(&chunk);
            full_body.push_str(&chunk);
            for frame in drain_sse_frames(&mut parse_buffer, false) {
                for delta in extract_chat_delta_chunks(&frame, request_id)? {
                    delta_count += 1;
                    if should_log_stream_chunk_debug(delta_count) {
                        debug!(
                            event = "provider.stream.delta.received",
                            provider = %self.provider_id,
                            request_id = request_id,
                            stream_kind = "chat_completions",
                            delta_index = delta_count,
                            delta_chars = delta.chars().count(),
                            delta_preview = %truncate_for_debug(&delta, STREAM_DEBUG_PREVIEW_LIMIT)
                        );
                    }
                    if let Some(tx) = sender {
                        let _ = tx
                            .send(Ok(ResponseEvent::OutputTextDelta {
                                id: request_id.to_string(),
                                delta: delta.clone(),
                            }))
                            .await;
                    }
                    all_chunks.push(delta);
                }
                if let Some(reasoning_delta) = extract_chat_reasoning_delta(&frame, request_id)?
                    && let Some(tx) = sender
                {
                    let _ = tx
                        .send(Ok(ResponseEvent::ReasoningDelta {
                            id: request_id.to_string(),
                            delta: reasoning_delta,
                        }))
                        .await;
                }
            }
        }
        for frame in drain_sse_frames(&mut parse_buffer, true) {
            for delta in extract_chat_delta_chunks(&frame, request_id)? {
                delta_count += 1;
                if should_log_stream_chunk_debug(delta_count) {
                    debug!(
                        event = "provider.stream.delta.received",
                        provider = %self.provider_id,
                        request_id = request_id,
                        stream_kind = "chat_completions",
                        delta_index = delta_count,
                        delta_chars = delta.chars().count(),
                        delta_preview = %truncate_for_debug(&delta, STREAM_DEBUG_PREVIEW_LIMIT)
                    );
                }
                if let Some(tx) = sender {
                    let _ = tx
                        .send(Ok(ResponseEvent::OutputTextDelta {
                            id: request_id.to_string(),
                            delta: delta.clone(),
                        }))
                        .await;
                }
                all_chunks.push(delta);
            }
            if let Some(reasoning_delta) = extract_chat_reasoning_delta(&frame, request_id)?
                && let Some(tx) = sender
            {
                let _ = tx
                    .send(Ok(ResponseEvent::ReasoningDelta {
                        id: request_id.to_string(),
                        delta: reasoning_delta,
                    }))
                    .await;
            }
        }
        let mut outcome = match if self.provider_id == "gigachat" {
            crate::clients::gigachat::map_gigachat_chat_completion_stream_text(&full_body)
        } else {
            map_chat_completion_stream_text(&full_body)
        } {
            Ok(parsed) => parsed,
            Err(error) => {
                if all_chunks.is_empty() {
                    return Err(error);
                }
                let output_tokens = all_chunks
                    .iter()
                    .map(|chunk| chunk.split_whitespace().count() as u32)
                    .sum::<u32>();
                ProviderOutcome {
                    chunks: all_chunks.clone(),
                    output_tokens,
                    reasoning: None,
                    reasoning_details: None,
                    tool_calls: None,
                    emitted_live: false,
                }
            }
        };
        if !all_chunks.is_empty() {
            outcome.chunks = all_chunks;
        }
        outcome.emitted_live = sender.is_some();
        Ok(outcome)
    }

    async fn post_responses_stream(
        &self,
        request_id: &str,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
        sender: Option<&mpsc::Sender<Result<ResponseEvent, CoreError>>>,
    ) -> Result<ProviderOutcome, CoreError> {
        let request_span = info_span!(
            "provider_stream_request",
            otel.name = "provider_stream_request",
            otel.kind = "internal",
            request.id = request_id,
            provider.request_id = request_id,
            provider = %self.provider_id,
            request_id = request_id,
            stream_kind = "responses"
        );
        let response = self
            .send_post(request_id, url, payload, bearer_override, extra_headers)
            .instrument(request_span)
            .await?;
        let is_json = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("application/json"));

        if is_json {
            let payload = response.json::<ResponsesApiResponse>().await.map_err(|err| {
                CoreError::Provider(format!("provider response parse failed: {err}"))
            })?;
            return map_responses_api_response(payload);
        }

        let mut all_chunks = Vec::<String>::new();
        let mut parse_buffer = String::new();
        let mut full_body = String::new();
        let is_yandex_provider = self.provider_id == "yandex";
        let mut stream = response.bytes_stream();
        let mut transport_chunk_index = 0usize;
        let mut delta_count = 0usize;
        while let Some(next) = stream.next().await {
            let bytes = next.map_err(|err| {
                CoreError::Provider(format!("provider stream read failed: {err}"))
            })?;
            transport_chunk_index += 1;
            let chunk = String::from_utf8_lossy(&bytes).replace('\r', "");
            if should_log_stream_chunk_debug(transport_chunk_index) {
                debug!(
                    event = "provider.stream.chunk.received",
                    provider = %self.provider_id,
                    request_id = request_id,
                    stream_kind = "responses",
                    chunk_index = transport_chunk_index,
                    chunk_bytes = bytes.len(),
                    chunk_preview = %truncate_for_debug(&chunk, STREAM_DEBUG_PREVIEW_LIMIT)
                );
            }
            parse_buffer.push_str(&chunk);
            full_body.push_str(&chunk);
            for frame in drain_sse_frames(&mut parse_buffer, false) {
                if let Some(delta) = extract_responses_text_delta(&frame)? {
                    delta_count += 1;
                    if should_log_stream_chunk_debug(delta_count) {
                        debug!(
                            event = "provider.stream.delta.received",
                            provider = %self.provider_id,
                            request_id = request_id,
                            stream_kind = "responses",
                            delta_index = delta_count,
                            delta_chars = delta.chars().count(),
                            delta_preview = %truncate_for_debug(&delta, STREAM_DEBUG_PREVIEW_LIMIT)
                        );
                    }
                    if let Some(tx) = sender
                        && !is_yandex_provider
                    {
                        let _ = tx
                            .send(Ok(ResponseEvent::OutputTextDelta {
                                id: request_id.to_string(),
                                delta: delta.clone(),
                            }))
                            .await;
                    }
                    all_chunks.push(delta);
                }
            }
        }
        for frame in drain_sse_frames(&mut parse_buffer, true) {
            if let Some(delta) = extract_responses_text_delta(&frame)? {
                delta_count += 1;
                if should_log_stream_chunk_debug(delta_count) {
                    debug!(
                        event = "provider.stream.delta.received",
                        provider = %self.provider_id,
                        request_id = request_id,
                        stream_kind = "responses",
                        delta_index = delta_count,
                        delta_chars = delta.chars().count(),
                        delta_preview = %truncate_for_debug(&delta, STREAM_DEBUG_PREVIEW_LIMIT)
                    );
                }
                if let Some(tx) = sender
                    && !is_yandex_provider
                {
                    let _ = tx
                        .send(Ok(ResponseEvent::OutputTextDelta {
                            id: request_id.to_string(),
                            delta: delta.clone(),
                        }))
                        .await;
                }
                all_chunks.push(delta);
            }
        }
        let mut outcome = match if self.provider_id == "yandex" {
            crate::clients::yandex::map_yandex_responses_stream_text(&full_body)
        } else {
            map_responses_stream_text(&full_body)
        } {
            Ok(parsed) => parsed,
            Err(error) => {
                if all_chunks.is_empty() {
                    return Err(error);
                }
                let output_tokens = all_chunks
                    .iter()
                    .map(|chunk| chunk.split_whitespace().count() as u32)
                    .sum::<u32>();
                ProviderOutcome {
                    chunks: all_chunks.clone(),
                    output_tokens,
                    reasoning: None,
                    reasoning_details: None,
                    tool_calls: None,
                    emitted_live: false,
                }
            }
        };
        if !all_chunks.is_empty() && !is_yandex_provider {
            outcome.chunks = all_chunks;
        }
        outcome.emitted_live = sender.is_some();
        Ok(outcome)
    }

    async fn post_form<T: DeserializeOwned>(
        &self,
        url: &str,
        form_fields: &[(String, String)],
        headers: &[(String, String)],
    ) -> Result<T, CoreError> {
        let client = self.client()?;
        let mut request = client.post(url);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        request
            .form(form_fields)
            .send()
            .await
            .map_err(|err| CoreError::Provider(format!("provider request failed: {err}")))?
            .error_for_status()
            .map_err(|err| CoreError::Provider(format!("provider returned error status: {err}")))?
            .json::<T>()
            .await
            .map_err(|err| CoreError::Provider(format!("provider response parse failed: {err}")))
    }
}

struct HeaderMapInjector<'a>(&'a mut HeaderMap);

impl<'a> Injector for HeaderMapInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(value)) =
            (HeaderName::from_bytes(key.as_bytes()), HeaderValue::from_str(&value))
        {
            self.0.insert(name, value);
        }
    }
}

fn inject_trace_headers(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    let mut headers = HeaderMap::new();
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(
            &tracing::Span::current().context(),
            &mut HeaderMapInjector(&mut headers),
        );
    });
    request.headers(headers)
}

fn should_log_stream_chunk_debug(index: usize) -> bool {
    index <= 3 || index.is_multiple_of(STREAM_DEBUG_SAMPLE_EVERY)
}

fn truncate_for_debug(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= limit {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

fn should_retry_failed_status(
    provider_id: &str,
    status: reqwest::StatusCode,
    body: &str,
    attempt: usize,
) -> bool {
    if attempt >= 2 {
        return false;
    }
    // Z.AI may intermittently return transient 5xx Operation failed for large tool payloads.
    provider_id == "zai"
        && status.is_server_error()
        && body.to_ascii_lowercase().contains("operation failed")
}

pub(crate) fn base_chat_payload(
    model: &str,
    input: &ResponsesInput,
    tools: Option<&[Value]>,
    tool_choice: Option<&Value>,
) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert("model".to_string(), Value::String(model.to_string()));
    payload.insert(
        "messages".to_string(),
        Value::Array(build_chat_messages_from_responses_input(input)),
    );
    payload.insert("stream".to_string(), Value::Bool(true));
    if let Some(defs) = tools
        && !defs.is_empty()
        && let Ok(value) = serde_json::to_value(defs)
    {
        payload.insert("tools".to_string(), value);
    }
    if let Some(choice) = tool_choice {
        payload.insert("tool_choice".to_string(), choice.clone());
    }
    payload
}

fn build_chat_messages_from_responses_input(input: &ResponsesInput) -> Vec<Value> {
    match input {
        ResponsesInput::Text(text) => vec![json!({ "role": "user", "content": text })],
        ResponsesInput::Items(items) => {
            let mut call_id_to_name = std::collections::HashMap::<String, String>::new();
            for item in items {
                if item.kind.as_deref() == Some("function_call")
                    && let (Some(call_id), Some(name)) =
                        (item.call_id.as_deref(), item.name.as_deref())
                    && !call_id.trim().is_empty()
                    && !name.trim().is_empty()
                {
                    call_id_to_name.insert(call_id.to_string(), name.to_string());
                }
            }

            let mut messages = Vec::new();
            for item in items {
                if let Some(message) =
                    map_response_input_item_to_chat_message(item, &call_id_to_name)
                {
                    messages.push(message);
                }
            }
            if messages.is_empty() {
                vec![json!({ "role": "user", "content": input.to_canonical_text() })]
            } else {
                messages
            }
        }
    }
}

fn map_response_input_item_to_chat_message(
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
        let arguments = item.arguments.as_deref().unwrap_or("{}").trim().to_string();
        return Some(json!({
            "role": "assistant",
            "tool_calls": [{
                "id": call_id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": arguments
                }
            }]
        }));
    }

    if kind == "function_call_output" {
        let call_id = item.call_id.as_deref()?.trim();
        if call_id.is_empty() {
            return None;
        }
        let output = item
            .output
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .or_else(|| extract_input_item_text(item))?;

        let mut tool_msg = Map::new();
        tool_msg.insert("role".to_string(), Value::String("tool".to_string()));
        tool_msg.insert("tool_call_id".to_string(), Value::String(call_id.to_string()));
        tool_msg.insert("content".to_string(), Value::String(output));
        if let Some(name) = item
            .name
            .as_deref()
            .or_else(|| call_id_to_name.get(call_id).map(String::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            tool_msg.insert("name".to_string(), Value::String(name.to_string()));
        }
        return Some(Value::Object(tool_msg));
    }

    let role =
        item.role.as_deref().or_else(|| if kind == "message" { Some("user") } else { None })?;
    let normalized_role = if role == "developer" { "system" } else { role };
    let content = extract_input_item_text(item)?;
    Some(json!({ "role": normalized_role, "content": content }))
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

fn map_chat_completion_response(
    payload: ChatCompletionsResponse,
) -> Result<ProviderOutcome, CoreError> {
    let first = payload
        .choices
        .first()
        .ok_or_else(|| CoreError::Provider("provider returned empty choices".to_string()))?;

    let content = extract_message_content(&first.message.content).unwrap_or_default();
    let tool_calls = first
        .message
        .tool_calls
        .as_ref()
        .map(|calls| map_provider_tool_calls(calls))
        .filter(|calls| !calls.is_empty());
    if content.is_empty() && tool_calls.is_none() {
        return Err(CoreError::Provider("provider returned empty message content".to_string()));
    }

    let output_tokens =
        payload.usage.and_then(|usage| usage.completion_tokens).unwrap_or_else(|| {
            if content.is_empty() { 0 } else { content.split_whitespace().count() as u32 }
        });

    let reasoning_details = first.message.reasoning_details.clone();
    let reasoning = first
        .message
        .reasoning_content
        .clone()
        .or_else(|| first.message.reasoning.clone())
        .or_else(|| {
            reasoning_details.as_ref().and_then(|details| extract_reasoning_from_details(details))
        });

    let chunks = if content.is_empty() { Vec::new() } else { vec![content] };
    Ok(ProviderOutcome {
        chunks,
        output_tokens,
        reasoning,
        reasoning_details,
        tool_calls,
        emitted_live: false,
    })
}

fn map_responses_api_response(payload: ResponsesApiResponse) -> Result<ProviderOutcome, CoreError> {
    let content = extract_message_text_from_responses_output(&payload.output).unwrap_or_default();
    let tool_calls = extract_tool_calls_from_responses_output(&payload.output);
    if content.is_empty() && tool_calls.is_none() {
        warn!(
            event = "provider.responses.empty_message_content",
            "provider returned empty message content for responses payload; treating as empty completed response"
        );
    }

    let reasoning_details = extract_reasoning_content_items_from_responses_output(&payload.output);
    let reasoning = extract_reasoning_text_from_responses_output(&payload.output).or_else(|| {
        reasoning_details.as_ref().and_then(|details| extract_reasoning_from_details(details))
    });

    let output_tokens = payload.usage.as_ref().map(|u| u.output_tokens).unwrap_or_else(|| {
        if content.is_empty() { 0 } else { content.split_whitespace().count() as u32 }
    });

    let chunks = if content.is_empty() { Vec::new() } else { vec![content] };
    Ok(ProviderOutcome {
        chunks,
        output_tokens,
        reasoning,
        reasoning_details,
        tool_calls,
        emitted_live: false,
    })
}

fn map_chat_completion_stream_text(payload: &str) -> Result<ProviderOutcome, CoreError> {
    let mut chunks = Vec::<String>::new();
    let mut all_content = String::new();
    let mut reasoning = String::new();
    let mut reasoning_details = Vec::<Value>::new();
    let mut output_tokens = None::<u32>;
    let mut tool_calls_by_index = HashMap::<usize, StreamToolCall>::new();
    let mut direct_tool_calls = Vec::<ToolCall>::new();

    for event in extract_sse_data_events(payload) {
        if event == "[DONE]" {
            continue;
        }
        let parsed: ChatCompletionsStreamChunk = serde_json::from_str(&event)
            .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;

        if let Some(usage) = parsed.usage.and_then(|usage| usage.completion_tokens) {
            output_tokens = Some(usage);
        }

        for choice in parsed.choices {
            if let Some(content_delta) = extract_message_content(&choice.delta.content)
                && !content_delta.is_empty()
            {
                all_content.push_str(&content_delta);
                chunks.push(content_delta);
            }
            if let Some(message) = choice.message.as_ref() {
                if let Some(content) = extract_message_content(&message.content)
                    && !content.is_empty()
                {
                    all_content.push_str(&content);
                    chunks.push(content);
                }
                if let Some(tool_calls) = message.tool_calls.as_ref() {
                    direct_tool_calls.extend(map_provider_tool_calls(tool_calls));
                }
            }

            if let Some(text) = choice.delta.reasoning_content.or(choice.delta.reasoning)
                && !text.trim().is_empty()
            {
                reasoning.push_str(&text);
            }

            if let Some(details) = choice.delta.reasoning_details {
                reasoning_details.extend(details);
            }

            for tool_delta in choice
                .delta
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .chain(choice.tool_calls.unwrap_or_default())
            {
                let index = tool_delta.index.unwrap_or(tool_calls_by_index.len());
                let entry = tool_calls_by_index.entry(index).or_default();
                if let Some(id) = tool_delta.id.filter(|v| !v.trim().is_empty()) {
                    entry.id = Some(id);
                }
                if let Some(kind) = tool_delta.kind.filter(|v| !v.trim().is_empty()) {
                    entry.kind = Some(kind);
                }
                if let Some(function) = tool_delta.function {
                    if let Some(name) = function.name.filter(|v| !v.trim().is_empty()) {
                        entry.name = Some(name);
                    }
                    if let Some(arguments) = function.arguments {
                        entry.arguments.push_str(&arguments);
                    }
                }
            }
        }
    }

    let mut tool_calls = finalize_stream_tool_calls(tool_calls_by_index);
    if !direct_tool_calls.is_empty() {
        if let Some(existing) = tool_calls.as_mut() {
            existing.extend(direct_tool_calls);
        } else {
            tool_calls = Some(direct_tool_calls);
        }
    }
    let reasoning = if reasoning.trim().is_empty() { None } else { Some(reasoning) };
    let reasoning_details =
        if reasoning_details.is_empty() { None } else { Some(reasoning_details) };
    let output_tokens = output_tokens.unwrap_or_else(|| {
        if all_content.is_empty() { 0 } else { all_content.split_whitespace().count() as u32 }
    });

    if all_content.is_empty() && tool_calls.is_none() {
        warn!(
            event = "provider.responses.stream.empty_message_content",
            "provider returned empty message content in responses stream; treating as empty completed response"
        );
    }

    let final_chunks = if all_content.is_empty() { Vec::new() } else { chunks };
    Ok(ProviderOutcome {
        chunks: final_chunks,
        output_tokens,
        reasoning,
        reasoning_details,
        tool_calls,
        emitted_live: false,
    })
}

fn map_responses_stream_text(payload: &str) -> Result<ProviderOutcome, CoreError> {
    let mut chunks = Vec::<String>::new();
    let mut all_content = String::new();
    let mut tool_calls = Vec::<ToolCall>::new();

    for event in extract_sse_data_events(payload) {
        if event == "[DONE]" {
            continue;
        }
        let parsed: ResponsesStreamEvent = serde_json::from_str(&event)
            .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;

        if parsed.kind == "response.output_text.delta"
            && let Some(delta) = parsed.delta.or(parsed.text)
            && !delta.is_empty()
        {
            all_content.push_str(&delta);
            chunks.push(delta);
            continue;
        }

        if parsed.kind == "response.output_item.added"
            && let Some(item) = parsed.item
            && item.kind == "function_call"
            && let Some(call_id) = item.call_id.as_deref()
            && let Some(name) = item.name.as_deref()
            && !call_id.trim().is_empty()
            && !name.trim().is_empty()
        {
            tool_calls.push(ToolCall {
                id: call_id.to_string(),
                kind: "function".to_string(),
                function: ToolFunction {
                    name: name.to_string(),
                    arguments: item.arguments.unwrap_or_else(|| "{}".to_string()),
                },
            });
            continue;
        }

        if ((parsed.kind == "response.completed") || parsed.kind.is_empty())
            && let Some(response) = parsed.response
        {
            let mut mapped = map_responses_api_response(response)?;
            if !all_content.is_empty() && mapped.chunks.is_empty() {
                mapped.chunks = chunks.clone();
            }
            if mapped.tool_calls.is_none() && !tool_calls.is_empty() {
                mapped.tool_calls = Some(tool_calls.clone());
            }
            return Ok(mapped);
        }
    }

    let tool_calls = if tool_calls.is_empty() { None } else { Some(tool_calls) };
    let output_tokens =
        if all_content.is_empty() { 0 } else { all_content.split_whitespace().count() as u32 };

    if all_content.is_empty() && tool_calls.is_none() {
        warn!(
            event = "provider.responses.stream.empty_message_content.tail",
            "provider returned empty message content in responses stream tail; treating as empty completed response"
        );
    }

    Ok(ProviderOutcome {
        chunks: if all_content.is_empty() { Vec::new() } else { chunks },
        output_tokens,
        reasoning: None,
        reasoning_details: None,
        tool_calls,
        emitted_live: false,
    })
}

fn extract_sse_data_events(payload: &str) -> Vec<String> {
    let mut owned = payload.replace('\r', "");
    drain_sse_frames(&mut owned, true)
        .into_iter()
        .filter_map(|frame| sse_frame_to_data(&frame))
        .collect()
}

fn drain_sse_frames(buffer: &mut String, flush_tail: bool) -> Vec<String> {
    let mut frames = Vec::new();
    while let Some(idx) = buffer.find("\n\n") {
        let frame = buffer[..idx].to_string();
        buffer.replace_range(..idx + 2, "");
        frames.push(frame);
    }
    if flush_tail {
        let tail = buffer.trim();
        if !tail.is_empty() {
            frames.push(tail.to_string());
            buffer.clear();
        }
    }
    frames
}

fn sse_frame_to_data(frame: &str) -> Option<String> {
    let mut data_lines = Vec::<String>::new();
    for line in frame.lines() {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }
    if data_lines.is_empty() { None } else { Some(data_lines.join("\n")) }
}

fn extract_chat_delta_chunks(frame: &str, _request_id: &str) -> Result<Vec<String>, CoreError> {
    let Some(data) = sse_frame_to_data(frame) else {
        return Ok(Vec::new());
    };
    if data == "[DONE]" {
        return Ok(Vec::new());
    }
    let parsed: ChatCompletionsStreamChunk = serde_json::from_str(&data)
        .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;
    let mut chunks = Vec::new();
    for choice in parsed.choices {
        if let Some(content_delta) = extract_message_content(&choice.delta.content)
            && !content_delta.is_empty()
        {
            chunks.push(content_delta);
        }
    }
    Ok(chunks)
}

fn extract_chat_reasoning_delta(
    frame: &str,
    _request_id: &str,
) -> Result<Option<String>, CoreError> {
    let Some(data) = sse_frame_to_data(frame) else {
        return Ok(None);
    };
    if data == "[DONE]" {
        return Ok(None);
    }
    let parsed: ChatCompletionsStreamChunk = serde_json::from_str(&data)
        .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;
    let text = parsed
        .choices
        .into_iter()
        .filter_map(|choice| choice.delta.reasoning_content.or(choice.delta.reasoning))
        .collect::<String>();
    if text.trim().is_empty() { Ok(None) } else { Ok(Some(text)) }
}

fn extract_responses_text_delta(frame: &str) -> Result<Option<String>, CoreError> {
    let Some(data) = sse_frame_to_data(frame) else {
        return Ok(None);
    };
    if data == "[DONE]" {
        return Ok(None);
    }
    let parsed: ResponsesStreamEvent = serde_json::from_str(&data)
        .map_err(|err| CoreError::Provider(format!("provider stream parse failed: {err}")))?;
    if parsed.kind == "response.output_text.delta" {
        return Ok(parsed.delta.or(parsed.text));
    }
    Ok(None)
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    content: Value,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning_details: Option<Vec<Value>>,
    #[serde(default)]
    tool_calls: Option<Vec<ProviderToolCall>>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    completion_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ResponsesApiResponse {
    #[serde(default)]
    output: Vec<ResponsesApiOutputItem>,
    #[serde(default)]
    usage: Option<ResponsesApiUsage>,
}

#[derive(Debug, Deserialize)]
struct ResponsesApiUsage {
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ResponsesApiOutputItem {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: Option<Vec<Value>>,
    #[serde(default)]
    summary: Option<Vec<ResponsesApiSummary>>,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsStreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamMessageDelta,
    #[serde(default)]
    tool_calls: Option<Vec<ProviderToolCallDelta>>,
    #[serde(default)]
    message: Option<Message>,
}

#[derive(Debug, Default, Deserialize)]
struct StreamMessageDelta {
    #[serde(default)]
    content: Value,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning_details: Option<Vec<Value>>,
    #[serde(default)]
    tool_calls: Option<Vec<ProviderToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ProviderToolCallDelta {
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    function: Option<ProviderToolFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct ProviderToolFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct StreamToolCall {
    id: Option<String>,
    kind: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ResponsesStreamEvent {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    item: Option<ResponsesApiOutputItem>,
    #[serde(default)]
    response: Option<ResponsesApiResponse>,
}

#[derive(Debug, Deserialize)]
struct ResponsesApiSummary {
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct ProviderToolCall {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    function: Option<ProviderToolFunction>,
}

#[derive(Debug, Deserialize)]
struct ProviderToolFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

fn map_provider_tool_calls(tool_calls: &[ProviderToolCall]) -> Vec<ToolCall> {
    tool_calls
        .iter()
        .filter_map(|call| {
            let function = call.function.as_ref()?;
            let name = function.name.as_deref()?.trim();
            if name.is_empty() {
                return None;
            }
            let arguments = function.arguments.clone().unwrap_or_else(|| "{}".to_string());
            let call_id =
                call.id.clone().unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
            Some(ToolCall {
                id: call_id,
                kind: call.kind.clone().unwrap_or_else(|| "function".to_string()),
                function: ToolFunction { name: name.to_string(), arguments },
            })
        })
        .collect()
}

fn finalize_stream_tool_calls(by_index: HashMap<usize, StreamToolCall>) -> Option<Vec<ToolCall>> {
    let mut sorted = by_index.into_iter().collect::<Vec<_>>();
    sorted.sort_by_key(|(idx, _)| *idx);
    let calls = sorted
        .into_iter()
        .filter_map(|(_, call)| {
            let name = call.name?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let arguments =
                if call.arguments.trim().is_empty() { "{}".to_string() } else { call.arguments };
            Some(ToolCall {
                id: call.id.unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple())),
                kind: call.kind.unwrap_or_else(|| "function".to_string()),
                function: ToolFunction { name, arguments },
            })
        })
        .collect::<Vec<_>>();
    if calls.is_empty() { None } else { Some(calls) }
}

fn extract_tool_calls_from_responses_output(
    output: &[ResponsesApiOutputItem],
) -> Option<Vec<ToolCall>> {
    let calls = output
        .iter()
        .filter(|item| item.kind == "function_call")
        .filter_map(|item| {
            let call_id = item.call_id.as_deref()?.trim();
            let name = item.name.as_deref()?.trim();
            if call_id.is_empty() || name.is_empty() {
                return None;
            }
            let arguments = item.arguments.clone().unwrap_or_else(|| "{}".to_string());
            Some(ToolCall {
                id: call_id.to_string(),
                kind: "function".to_string(),
                function: ToolFunction { name: name.to_string(), arguments },
            })
        })
        .collect::<Vec<_>>();
    if calls.is_empty() { None } else { Some(calls) }
}

fn extract_message_content(content: &Value) -> Option<String> {
    match content {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

fn extract_reasoning_from_details(details: &[Value]) -> Option<String> {
    let text = details
        .iter()
        .filter_map(|detail| {
            let kind = detail.get("type").and_then(Value::as_str)?;
            match kind {
                "reasoning.summary" => detail.get("summary").and_then(Value::as_str),
                "reasoning.text" => detail.get("text").and_then(Value::as_str),
                _ => None,
            }
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if text.is_empty() { None } else { Some(text) }
}

fn extract_message_text_from_responses_output(output: &[ResponsesApiOutputItem]) -> Option<String> {
    let text = output
        .iter()
        .filter(|item| item.kind == "message")
        .filter_map(|item| item.content.as_ref())
        .flat_map(|parts| parts.iter())
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .or_else(|| part.get("output_text").and_then(Value::as_str))
                .or_else(|| part.get("input_text").and_then(Value::as_str))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("");
    if text.is_empty() { None } else { Some(text) }
}

fn extract_reasoning_text_from_responses_output(
    output: &[ResponsesApiOutputItem],
) -> Option<String> {
    output.iter().find_map(|item| {
        if item.kind != "reasoning" {
            return None;
        }
        let summary = item
            .summary
            .as_ref()
            .and_then(|values| values.first())
            .map(|value| value.text.trim().to_string())
            .filter(|value| !value.is_empty());
        if summary.is_some() {
            return summary;
        }
        item.content.as_ref().and_then(|details| extract_reasoning_from_details(details))
    })
}

fn extract_reasoning_content_items_from_responses_output(
    output: &[ResponsesApiOutputItem],
) -> Option<Vec<Value>> {
    output.iter().find_map(|item| {
        if item.kind != "reasoning" {
            return None;
        }
        item.content.clone().filter(|items| !items.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::{
        deepseek::build_deepseek_payload, openai::build_openai_payload,
        openrouter::build_openrouter_payload, yandex::build_yandex_upstream_model,
        zai::build_zai_payload,
    };
    use opentelemetry::{
        propagation::{Extractor, TextMapPropagator},
        trace::{TraceContextExt, TracerProvider},
    };
    use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::SdkTracerProvider};
    use tracing::trace_span;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use xrouter_contracts::{ReasoningConfig, ResponseInputItem, ResponsesInput, ToolFunction};

    fn reasoning(effort: &str) -> ReasoningConfig {
        ReasoningConfig { effort: Some(effort.to_string()) }
    }

    fn text_input(text: &str) -> ResponsesInput {
        ResponsesInput::Text(text.to_string())
    }

    #[test]
    fn inject_trace_headers_uses_current_span_context() {
        global::set_text_map_propagator(TraceContextPropagator::new());
        let provider = SdkTracerProvider::builder().build();
        let tracer = provider.tracer("test-tracer");
        let subscriber =
            tracing_subscriber::registry().with(tracing_opentelemetry::layer().with_tracer(tracer));
        let _guard = subscriber.set_default();

        let span = trace_span!("provider_http_request_test");
        let _entered = span.enter();
        let span_context = span.context().span().span_context().clone();
        let request = reqwest::Client::new().post("http://localhost/health");
        let request = inject_trace_headers(request);
        let request = request.build().expect("request must build");

        let extracted =
            TraceContextPropagator::new().extract(&HeaderMapExtractor(request.headers()));
        let extracted_span = extracted.span();
        let extracted_context = extracted_span.span_context();
        assert!(extracted_context.is_valid());
        assert_eq!(extracted_context.trace_id(), span_context.trace_id());
    }

    #[test]
    fn inject_trace_headers_without_active_span_does_not_fail() {
        global::set_text_map_propagator(TraceContextPropagator::new());
        let request = reqwest::Client::new().post("http://localhost/health");
        let request = inject_trace_headers(request);
        let request = request.build().expect("request must build");
        let maybe_traceparent =
            request.headers().get("traceparent").and_then(|value| value.to_str().ok());
        assert!(maybe_traceparent.is_none() || maybe_traceparent == Some(""));
    }

    struct HeaderMapExtractor<'a>(&'a reqwest::header::HeaderMap);

    impl<'a> Extractor for HeaderMapExtractor<'a> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).and_then(|value| value.to_str().ok())
        }

        fn keys(&self) -> Vec<&str> {
            self.0.keys().map(reqwest::header::HeaderName::as_str).collect()
        }
    }

    #[test]
    fn openrouter_keeps_reasoning_effort_as_is() {
        let input = text_input("Reply with ok");
        let (payload, _) = build_openrouter_payload(
            "openai/gpt-5.2",
            &input,
            Some(&reasoning("xhigh")),
            None,
            None,
        );
        assert_eq!(payload["reasoning"]["effort"], "xhigh");
        assert!(payload.get("thinking").is_none());
    }

    #[test]
    fn deepseek_chat_enables_thinking_when_effort_present() {
        let input = text_input("Reply with ok");
        let (payload, _) =
            build_deepseek_payload("deepseek-chat", &input, Some(&reasoning("medium")), None, None);
        assert_eq!(payload["thinking"]["type"], "enabled");
        assert!(payload.get("reasoning").is_none());
    }

    #[test]
    fn deepseek_reasoner_does_not_set_thinking() {
        let input = text_input("Reply with ok");
        let (payload, _) = build_deepseek_payload(
            "deepseek-reasoner",
            &input,
            Some(&reasoning("high")),
            None,
            None,
        );
        assert!(payload.get("thinking").is_none());
    }

    #[test]
    fn non_openrouter_maps_xhigh_to_high() {
        let input = text_input("Reply with ok");
        let payload =
            build_openai_payload("gpt-4.1-mini", &input, Some(&reasoning("xhigh")), None, None);
        assert_eq!(payload["reasoning"]["effort"], "high");
    }

    #[test]
    fn zai_enables_thinking_when_effort_present() {
        let input = text_input("Reply with ok");
        let (payload, _) = build_zai_payload("glm-5", &input, Some(&reasoning("high")), None, None);
        assert_eq!(payload["thinking"]["type"], "enabled");
        assert!(payload.get("reasoning").is_none());
    }

    #[test]
    fn zai_disables_thinking_when_effort_none() {
        let input = text_input("Reply with ok");
        let (payload, _) = build_zai_payload("glm-5", &input, Some(&reasoning("none")), None, None);
        assert_eq!(payload["thinking"]["type"], "disabled");
        assert!(payload.get("reasoning").is_none());
    }

    #[test]
    fn responses_input_items_map_to_chat_messages_with_tool_roundtrip() {
        let input = ResponsesInput::Items(vec![
            ResponseInputItem {
                kind: Some("function_call".to_string()),
                role: None,
                content: None,
                text: None,
                output: None,
                call_id: Some("call_1".to_string()),
                name: Some("read_file".to_string()),
                arguments: Some("{\"path\":\"README.md\"}".to_string()),
                extra: Default::default(),
            },
            ResponseInputItem {
                kind: Some("function_call_output".to_string()),
                role: None,
                content: None,
                text: None,
                output: Some("{\"ok\":true}".to_string()),
                call_id: Some("call_1".to_string()),
                name: None,
                arguments: None,
                extra: Default::default(),
            },
            ResponseInputItem {
                kind: Some("message".to_string()),
                role: Some("user".to_string()),
                content: Some(ResponseInputContent::Text("continue".to_string())),
                text: None,
                output: None,
                call_id: None,
                name: None,
                arguments: None,
                extra: Default::default(),
            },
        ]);

        let messages = build_chat_messages_from_responses_input(&input);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["tool_call_id"], "call_1");
        assert_eq!(messages[1]["name"], "read_file");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"], "continue");
    }

    #[test]
    fn map_chat_completion_response_accepts_tool_only_message() {
        let payload = ChatCompletionsResponse {
            choices: vec![Choice {
                message: Message {
                    content: Value::String(String::new()),
                    reasoning: None,
                    reasoning_content: None,
                    reasoning_details: None,
                    tool_calls: Some(vec![ProviderToolCall {
                        id: Some("call_1".to_string()),
                        kind: Some("function".to_string()),
                        function: Some(ProviderToolFunction {
                            name: Some("read_file".to_string()),
                            arguments: Some("{\"path\":\"README.md\"}".to_string()),
                        }),
                    }]),
                },
            }],
            usage: Some(Usage { completion_tokens: Some(7) }),
        };

        let outcome = map_chat_completion_response(payload).expect("tool-only completion is valid");
        assert!(outcome.chunks.is_empty());
        assert_eq!(outcome.output_tokens, 7);
        assert_eq!(
            outcome.tool_calls,
            Some(vec![ToolCall {
                id: "call_1".to_string(),
                kind: "function".to_string(),
                function: ToolFunction {
                    name: "read_file".to_string(),
                    arguments: "{\"path\":\"README.md\"}".to_string(),
                },
            }])
        );
    }

    #[test]
    fn reasoning_details_summary_is_extracted() {
        let details = vec![json!({
            "type": "reasoning.summary",
            "summary": "A concise summary"
        })];
        assert_eq!(extract_reasoning_from_details(&details), Some("A concise summary".to_string()));
    }

    #[test]
    fn reasoning_details_text_and_summary_are_joined() {
        let details = vec![
            json!({
                "type": "reasoning.summary",
                "summary": "Summary"
            }),
            json!({
                "type": "reasoning.text",
                "text": "Detailed chain"
            }),
        ];
        assert_eq!(
            extract_reasoning_from_details(&details),
            Some("Summary\nDetailed chain".to_string())
        );
    }

    #[test]
    fn yandex_upstream_model_adds_gpt_prefix() {
        let model = build_yandex_upstream_model("aliceai-llm/latest", Some("folder-123"))
            .expect("model should build");
        assert_eq!(model, "gpt://folder-123/aliceai-llm/latest");
    }

    #[test]
    fn yandex_upstream_model_keeps_prefixed_model() {
        let model = build_yandex_upstream_model(
            "gpt://folder-123/yandexgpt-lite/latest",
            Some("folder-123"),
        )
        .expect("model should pass through");
        assert_eq!(model, "gpt://folder-123/yandexgpt-lite/latest");
    }

    #[test]
    fn yandex_upstream_model_requires_project() {
        let error = build_yandex_upstream_model("aliceai-llm/latest", None)
            .expect_err("missing project should fail");
        assert_eq!(
            error.to_string(),
            "provider error: provider project is not configured for yandex"
        );
    }

    #[test]
    fn chat_sse_with_delta_only_is_not_empty() {
        let sse = concat!(
            "event: message\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"index\":0,\"finish_reason\":null}]}\n\n",
            "data: [DONE]\n\n"
        );
        let outcome = map_chat_completion_stream_text(sse).expect("delta-only SSE must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
        assert!(outcome.tool_calls.is_none());
    }

    #[test]
    fn responses_sse_with_delta_only_is_not_empty() {
        let sse = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\"}]}],\"usage\":{\"output_tokens\":1}}}\n\n"
        );
        let outcome = map_responses_stream_text(sse).expect("responses SSE must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
    }

    #[test]
    fn responses_sse_without_type_but_with_response_object_is_not_empty() {
        let sse = concat!(
            "data: {\"response\":{\"id\":\"resp_1\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\"}]}],\"usage\":{\"output_tokens\":1}}}\n\n",
            "data: [DONE]\n\n"
        );
        let outcome =
            map_responses_stream_text(sse).expect("responses fallback payload must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
        assert_eq!(outcome.output_tokens, 1);
    }

    #[test]
    fn chat_sse_without_trailing_separator_is_not_empty() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"index\":0,\"finish_reason\":null}]}";
        let outcome = map_chat_completion_stream_text(sse).expect("SSE tail frame must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
    }

    #[test]
    fn responses_sse_without_trailing_separator_is_not_empty() {
        let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}";
        let outcome = map_responses_stream_text(sse).expect("SSE tail frame must parse");
        assert_eq!(outcome.chunks.join(""), "ok");
    }

    #[test]
    fn responses_sse_with_empty_completed_payload_is_fail_soft() {
        let sse = concat!(
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[],\"usage\":{\"output_tokens\":0}}}\n\n"
        );
        let outcome = map_responses_stream_text(sse)
            .expect("empty completed responses payload must not fail");
        assert!(outcome.chunks.is_empty());
        assert_eq!(outcome.output_tokens, 0);
        assert!(outcome.tool_calls.is_none());
    }

    #[test]
    fn responses_message_text_skips_empty_parts_and_joins_non_empty() {
        let payload = ResponsesApiResponse {
            output: vec![ResponsesApiOutputItem {
                kind: "message".to_string(),
                content: Some(vec![
                    json!({"type":"output_text","text":""}),
                    json!({"type":"output_text","text":"hello"}),
                    json!({"type":"output_text","text":" world"}),
                ]),
                summary: None,
                call_id: None,
                name: None,
                arguments: None,
            }],
            usage: Some(ResponsesApiUsage { output_tokens: 2 }),
        };
        let outcome = map_responses_api_response(payload).expect("message text must be extracted");
        assert_eq!(outcome.chunks.join(""), "helloworld");
    }

    #[test]
    fn chat_sse_with_choice_level_tool_calls_is_not_empty() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{},\"tool_calls\":[{\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\\\"Kyiv\\\"}\"}}],\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let outcome =
            map_chat_completion_stream_text(sse).expect("choice-level tool_calls must parse");
        let tool_calls = outcome.tool_calls.expect("tool calls must be present");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert_eq!(tool_calls[0].function.arguments, "{\"city\":\"Kyiv\"}");
    }

    #[test]
    fn chat_sse_with_message_level_tool_calls_is_not_empty() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{},\"message\":{\"role\":\"assistant\",\"tool_calls\":[{\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\\\"Kyiv\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let outcome =
            map_chat_completion_stream_text(sse).expect("message-level tool_calls must parse");
        let tool_calls = outcome.tool_calls.expect("tool calls must be present");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "get_weather");
        assert_eq!(tool_calls[0].function.arguments, "{\"city\":\"Kyiv\"}");
    }

    #[test]
    fn upstream_stream_flag_is_forced_true_for_all_providers() {
        // CONTRACT GUARD: do not weaken this test.
        // All upstream provider requests must always be sent with stream=true.
        let input = text_input("hello");

        let openai = build_openai_payload("gpt-4.1-mini", &input, None, None, None);
        assert_eq!(openai["stream"], Value::Bool(true));

        let (openrouter, _) =
            build_openrouter_payload("openai/gpt-5-mini", &input, None, None, None);
        assert_eq!(openrouter["stream"], Value::Bool(true));

        let (zai, _) = build_zai_payload("glm-5", &input, None, None, None);
        assert_eq!(zai["stream"], Value::Bool(true));

        let (gigachat, _) =
            crate::clients::gigachat::build_gigachat_payload("GigaChat-Pro", &input, None, None);
        assert_eq!(gigachat["stream"], Value::Bool(true));

        let (yandex, _) =
            crate::clients::yandex::build_yandex_responses_payload("gpt://p/m", &input, None, None);
        assert_eq!(yandex["stream"], Value::Bool(true));

        let (deepseek, _) = build_deepseek_payload("deepseek-chat", &input, None, None, None);
        assert_eq!(deepseek["stream"], Value::Bool(true));
    }

    #[test]
    fn retries_zai_transient_operation_failed_once() {
        assert!(should_retry_failed_status(
            "zai",
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{\"error\":{\"code\":\"500\",\"message\":\"Operation failed\"}}",
            1,
        ));
        assert!(!should_retry_failed_status(
            "zai",
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{\"error\":{\"code\":\"500\",\"message\":\"Operation failed\"}}",
            2,
        ));
    }

    #[test]
    fn does_not_retry_non_zai_or_non_matching_failures() {
        assert!(!should_retry_failed_status(
            "deepseek",
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{\"error\":{\"message\":\"Operation failed\"}}",
            1,
        ));
        assert!(!should_retry_failed_status(
            "zai",
            reqwest::StatusCode::BAD_REQUEST,
            "{\"error\":{\"message\":\"Operation failed\"}}",
            1,
        ));
        assert!(!should_retry_failed_status(
            "zai",
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "{\"error\":{\"message\":\"Different\"}}",
            1,
        ));
    }
}
