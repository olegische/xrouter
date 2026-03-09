use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use opentelemetry::{global, propagation::Injector, trace::Status};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tracing::{Instrument, debug, field, info_span, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use xrouter_contracts::ResponseEvent;
use xrouter_core::{CoreError, ProviderOutcome, ResponseEventSink};

use crate::parser::{
    ChatCompletionsResponse, ResponsesApiResponse, drain_sse_frames, extract_chat_delta_chunks,
    extract_chat_reasoning_delta, extract_responses_text_delta, map_chat_completion_response,
    map_chat_completion_stream_text, map_responses_api_response, map_responses_stream_text,
};

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
pub(crate) struct HttpRuntime {
    provider_id: String,
    base_url: Option<String>,
    api_key: Option<String>,
    http_client: Option<Client>,
    max_inflight: Option<Arc<Semaphore>>,
}

impl HttpRuntime {
    pub(crate) fn new(
        provider_id: String,
        base_url: Option<String>,
        api_key: Option<String>,
        http_client: Option<Client>,
        max_inflight: Option<usize>,
    ) -> Self {
        let max_inflight = max_inflight.map(Semaphore::new).map(Arc::new);
        Self { provider_id, base_url, api_key, http_client, max_inflight }
    }

    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref().filter(|value| !value.trim().is_empty())
    }

    fn base_url(&self) -> Result<&str, CoreError> {
        self.base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CoreError::Provider("provider base_url is not configured".to_string()))
    }

    pub(crate) fn build_url(&self, path: &str) -> Result<String, CoreError> {
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

    pub(crate) async fn post_chat_completions_stream(
        &self,
        request_id: &str,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
        sender: Option<&dyn ResponseEventSink>,
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
                        tx.send(Ok(ResponseEvent::OutputTextDelta {
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
                    tx.send(Ok(ResponseEvent::ReasoningDelta {
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
                    tx.send(Ok(ResponseEvent::OutputTextDelta {
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
                tx.send(Ok(ResponseEvent::ReasoningDelta {
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

    pub(crate) async fn post_responses_stream(
        &self,
        request_id: &str,
        url: &str,
        payload: &Value,
        bearer_override: Option<&str>,
        extra_headers: &[(String, String)],
        sender: Option<&dyn ResponseEventSink>,
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
                        tx.send(Ok(ResponseEvent::OutputTextDelta {
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
                    tx.send(Ok(ResponseEvent::OutputTextDelta {
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

    pub(crate) async fn post_form<T: DeserializeOwned>(
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

pub(crate) fn inject_trace_headers(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
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
    let text = redact_bearer_tokens(text);
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

fn redact_bearer_tokens(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < chars.len() {
        if matches_bearer_prefix(&chars, i) {
            for ch in &chars[i..i + 7] {
                out.push(*ch);
            }
            i += 7;
            while i < chars.len() && !is_bearer_token_delimiter(chars[i]) {
                i += 1;
            }
            out.push_str("***");
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn matches_bearer_prefix(chars: &[char], idx: usize) -> bool {
    if idx + 7 > chars.len() {
        return false;
    }
    chars[idx].eq_ignore_ascii_case(&'b')
        && chars[idx + 1].eq_ignore_ascii_case(&'e')
        && chars[idx + 2].eq_ignore_ascii_case(&'a')
        && chars[idx + 3].eq_ignore_ascii_case(&'r')
        && chars[idx + 4].eq_ignore_ascii_case(&'e')
        && chars[idx + 5].eq_ignore_ascii_case(&'r')
        && chars[idx + 6] == ' '
}

fn is_bearer_token_delimiter(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ';' | ')' | '(' | ']' | '[' | '}')
}

pub(crate) fn should_retry_failed_status(
    provider_id: &str,
    status: reqwest::StatusCode,
    body: &str,
    attempt: usize,
) -> bool {
    if attempt >= 2 {
        return false;
    }
    provider_id == "zai"
        && status.is_server_error()
        && body.to_ascii_lowercase().contains("operation failed")
}

#[cfg(test)]
mod tests {
    use super::{inject_trace_headers, should_retry_failed_status};
    use opentelemetry::{
        global,
        propagation::{Extractor, TextMapPropagator},
        trace::{TraceContextExt, TracerProvider},
    };
    use opentelemetry_sdk::{propagation::TraceContextPropagator, trace::SdkTracerProvider};
    use tracing::trace_span;
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

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

    struct HeaderMapExtractor<'a>(&'a reqwest::header::HeaderMap);

    impl<'a> Extractor for HeaderMapExtractor<'a> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).and_then(|value| value.to_str().ok())
        }

        fn keys(&self) -> Vec<&str> {
            self.0.keys().map(reqwest::header::HeaderName::as_str).collect()
        }
    }
}
