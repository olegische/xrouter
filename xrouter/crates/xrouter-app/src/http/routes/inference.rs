use std::{convert::Infallible, sync::Arc, time::Instant};

use async_trait::async_trait;
use axum::{
    Json,
    body::Bytes,
    extract::{MatchedPath, State},
    http::HeaderMap,
    response::{IntoResponse, Response, Sse, sse::Event},
};
use futures::StreamExt;
use opentelemetry::{global, propagation::Extractor, trace::Status};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{Span, debug, field, info, info_span, trace_span, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use xrouter_contracts::{
    ChatCompletionsRequest, ChatCompletionsResponse, ResponseEvent, ResponseOutputItem,
    ResponsesRequest, ResponsesResponse,
};
use xrouter_core::{CoreError, ExecutionEngine, ResponseEventSink, synthesize_model_id};

use crate::{
    AppState, http::auth::resolve_byok_bearer, http::docs::ErrorResponse,
    http::errors::error_response,
};

struct AxumResponseEventSink {
    sender: mpsc::Sender<Result<ResponseEvent, CoreError>>,
}

#[async_trait]
impl ResponseEventSink for AxumResponseEventSink {
    async fn send(&self, event: Result<ResponseEvent, CoreError>) {
        let _ = self.sender.send(event).await;
    }
}

fn spawn_engine_stream(
    engine: Arc<ExecutionEngine>,
    request: ResponsesRequest,
    auth_bearer: Option<String>,
) -> ReceiverStream<Result<ResponseEvent, CoreError>> {
    let (tx, rx) = mpsc::channel(32);
    let sink: Arc<dyn ResponseEventSink> = Arc::new(AxumResponseEventSink { sender: tx });
    tokio::spawn(async move {
        let _ = engine.execute_stream_to_sink(request, None, auth_bearer, sink).await;
    });
    ReceiverStream::new(rx)
}

#[utoipa::path(
    post,
    path = "/api/v1/responses",
    request_body = ResponsesRequest,
    responses(
        (status = 200, description = "Responses API result", body = ResponsesResponse),
        (status = 400, description = "Validation or provider error", body = ErrorResponse)
    ),
    tag = "xrouter-app"
)]
pub(crate) async fn post_responses(
    State(state): State<AppState>,
    matched_path: Option<MatchedPath>,
    headers: HeaderMap,
    request_body: Bytes,
) -> Response {
    let started_at = Instant::now();
    let route = matched_path.as_ref().map_or("/api/v1/responses", MatchedPath::as_str).to_string();
    let request_span = info_span!(
        "http.request",
        otel.name = "http.request",
        otel.kind = "server",
        openinference.span.kind = "CHAIN",
        request.id = field::Empty,
        response.id = field::Empty,
        route = %route,
        model = field::Empty,
        provider = field::Empty,
        stream = field::Empty,
        input.value = field::Empty,
        output.value = field::Empty
    );
    attach_parent_context(&request_span, &headers);
    let _request_span_guard = request_span.enter();
    let mut request: ResponsesRequest = match serde_json::from_slice(&request_body) {
        Ok(request) => request,
        Err(err) => {
            info!(
                event = "http.request.invalid_json",
                route = route,
                body_bytes = request_body.len(),
                error = %err
            );
            debug!(
                event = "http.request.invalid_json.payload",
                route = route,
                payload_preview = %preview_request_body(&request_body)
            );
            return (
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse { error: "invalid request body".to_string() }),
            )
                .into_response();
        }
    };
    let normalized_input = request.input.to_canonical_text();
    let request_model = request.model.clone();
    let provider = state.resolve_provider_key(&request.model);
    let provider_model = state.resolve_provider_model_id(&request.model);
    let public_model_id = synthesize_model_id(&provider, &provider_model);
    let auth_bearer = match resolve_byok_bearer(
        &headers,
        state.byok_enabled,
        provider.as_str(),
        route.as_str(),
    ) {
        Ok(token) => token,
        Err(err) => return error_response(err),
    };
    request_span.record("model", public_model_id.as_str());
    request_span.record("provider", provider.as_str());
    request_span.record("stream", request.stream);
    request_span.record("input.value", truncate_attr_value(&normalized_input, 512));
    request.model = provider_model;
    info!(
        event = "http.request.received",
        route = route,
        model = %public_model_id,
        provider = %provider,
        stream = request.stream,
        input_chars = normalized_input.len()
    );
    debug!(
        event = "http.request.payload",
        route = route,
        model = %request_model,
        provider = %provider,
        request_text = %normalized_input
    );

    let engine = match state.resolve_engine(&request.model) {
        Ok(engine) => engine,
        Err(err) => {
            warn!(
                event = "http.request.failed",
                route = route,
                model = %public_model_id,
                provider = %provider,
                stream = request.stream,
                duration_ms = started_at.elapsed().as_millis() as u64,
                error = %err
            );
            return error_response(err);
        }
    };

    if request.stream {
        let stream_route = route.clone();
        let stream_provider = provider.clone();
        let stream_request_span = request_span.clone();
        let response_id = new_prefixed_id("resp_");
        let stream_item_id = "msg_0".to_string();
        info!(
            event = "http.stream.started",
            route = route,
            response_id = %response_id,
            model = %public_model_id,
            provider = %provider
        );
        let created = json!({
            "type": "response.created",
            "response": {
                "id": response_id,
                "object": "response",
                "status": "in_progress",
                "model": public_model_id,
                "output": []
            }
        });
        let output_item_added = json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {
                "id": stream_item_id,
                "type": "message",
                "role": "assistant",
                "content": []
            }
        });
        let content_part_added = json!({
            "type": "response.content_part.added",
            "output_index": 0,
            "item_id": stream_item_id,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": ""
            }
        });

        let stream = spawn_engine_stream(engine.clone(), request, auth_bearer.clone()).flat_map(
            move |event| {
                let mut events = Vec::<Result<Event, Infallible>>::new();
                if let Ok(ref mapped) = event {
                    if let Some(request_id) = response_event_request_id(mapped) {
                        stream_request_span.record("request.id", request_id);
                        stream_request_span.record("response.id", request_id);
                    }
                    record_response_event_classification(
                        stream_route.as_str(),
                        stream_provider.as_str(),
                        "responses_sse",
                        mapped,
                    );
                }
                match event {
                    Ok(ResponseEvent::OutputTextDelta { delta, .. }) => {
                        events.push(Ok(Event::default().event("response.output_text.delta").data(
                            json!({
                                "type": "response.output_text.delta",
                                "output_index": 0,
                                "item_id": "msg_0",
                                "content_index": 0,
                                "delta": delta
                            })
                            .to_string(),
                        )));
                    }
                    Ok(ResponseEvent::ReasoningDelta { delta, .. }) => {
                        events.push(Ok(Event::default().event("response.reasoning.delta").data(
                            json!({
                                "type": "response.reasoning.delta",
                                "delta": delta
                            })
                            .to_string(),
                        )));
                    }
                    Ok(ResponseEvent::ResponseCompleted {
                        output, finish_reason, usage, ..
                    }) => {
                        let reasoning = extract_reasoning_from_output(&output);
                        info!(
                            event = "http.stream.completed",
                            route = stream_route,
                            response_id = %response_id,
                            provider = %stream_provider,
                            finish_reason = %finish_reason,
                            reasoning_present = reasoning.is_some(),
                            reasoning_chars = reasoning.as_ref().map(|it| it.len()).unwrap_or(0),
                            input_tokens = usage.input_tokens,
                            output_tokens = usage.output_tokens,
                            total_tokens = usage.total_tokens,
                            duration_ms = started_at.elapsed().as_millis() as u64
                        );
                        for (output_index, item) in output.iter().enumerate() {
                            events.push(Ok(Event::default()
                                .event("response.output_item.done")
                                .data(
                                    json!({
                                        "type": "response.output_item.done",
                                        "output_index": output_index,
                                        "item": item
                                    })
                                    .to_string(),
                                )));
                        }
                        events.push(Ok(Event::default().event("response.completed").data(
                            json!({
                                "type": "response.completed",
                                "response": {
                                    "id": response_id,
                                    "status": "completed",
                                    "output": output,
                                    "finish_reason": finish_reason,
                                    "usage": {
                                        "input_tokens": usage.input_tokens,
                                        "output_tokens": usage.output_tokens,
                                        "total_tokens": usage.total_tokens
                                    }
                                }
                            })
                            .to_string(),
                        )));
                    }
                    Ok(ResponseEvent::ResponseError { message, .. }) => {
                        stream_request_span.set_status(Status::error(message.clone()));
                        warn!(
                            event = "http.stream.failed",
                            route = stream_route,
                            response_id = %response_id,
                            provider = %stream_provider,
                            duration_ms = started_at.elapsed().as_millis() as u64,
                            error = %message
                        );
                        events.push(Ok(Event::default().event("response.error").data(
                            json!({"type": "response.error", "error": message}).to_string(),
                        )));
                    }
                    Err(error) => {
                        stream_request_span.set_status(Status::error(error.to_string()));
                        warn!(
                            event = "http.stream.failed",
                            route = stream_route,
                            response_id = %response_id,
                            provider = %stream_provider,
                            duration_ms = started_at.elapsed().as_millis() as u64,
                            error = %error
                        );
                        events.push(Ok(Event::default().event("response.error").data(
                            json!({"type": "response.error", "error": error.to_string()})
                                .to_string(),
                        )));
                    }
                }
                futures::stream::iter(events)
            },
        );

        let bootstrap = futures::stream::iter(vec![
            Ok::<Event, Infallible>(
                Event::default().event("response.created").data(created.to_string()),
            ),
            Ok::<Event, Infallible>(
                Event::default()
                    .event("response.output_item.added")
                    .data(output_item_added.to_string()),
            ),
            Ok::<Event, Infallible>(
                Event::default()
                    .event("response.content_part.added")
                    .data(content_part_added.to_string()),
            ),
        ]);
        let full_stream = bootstrap.chain(stream);
        return Sse::new(full_stream).into_response();
    }

    match run_responses_request(engine, request, auth_bearer).await {
        Ok(mut resp) => {
            resp.id = ensure_id_prefix(&resp.id, "resp_");
            request_span.record("request.id", resp.id.as_str());
            request_span.record("response.id", resp.id.as_str());
            let response_text = extract_message_text_from_output(&resp.output);
            request_span.record("output.value", truncate_attr_value(&response_text, 512));
            let reasoning = extract_reasoning_from_output(&resp.output);
            debug!(
                event = "http.response.payload",
                route = route,
                model = %request_model,
                provider = %provider,
                response_text = %response_text
            );
            info!(
                event = "http.request.succeeded",
                route = route,
                model = %request_model,
                provider = %provider,
                status = %resp.status,
                finish_reason = %resp.finish_reason,
                reasoning_present = reasoning.is_some(),
                reasoning_chars = reasoning.as_ref().map(|it| it.len()).unwrap_or(0),
                input_tokens = resp.usage.input_tokens,
                output_tokens = resp.usage.output_tokens,
                total_tokens = resp.usage.total_tokens,
                duration_ms = started_at.elapsed().as_millis() as u64
            );
            Json(resp).into_response()
        }
        Err(err) => {
            request_span.set_status(Status::error(err.to_string()));
            warn!(
                event = "http.request.failed",
                route = route,
                model = %request_model,
                provider = %provider,
                duration_ms = started_at.elapsed().as_millis() as u64,
                error = %err
            );
            error_response(err)
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/chat/completions",
    request_body = ChatCompletionsRequest,
    responses(
        (status = 200, description = "Chat Completions API result", body = ChatCompletionsResponse),
        (status = 400, description = "Validation or provider error", body = ErrorResponse)
    ),
    tag = "xrouter-app"
)]
pub(crate) async fn post_chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionsRequest>,
) -> Response {
    let started_at = Instant::now();
    let request_span = info_span!(
        "http.request",
        otel.name = "http.request",
        otel.kind = "server",
        openinference.span.kind = "CHAIN",
        request.id = field::Empty,
        response.id = field::Empty,
        route = "/api/v1/chat/completions",
        model = field::Empty,
        provider = field::Empty,
        stream = field::Empty,
        input.value = field::Empty,
        output.value = field::Empty
    );
    attach_parent_context(&request_span, &headers);
    let _request_span_guard = request_span.enter();
    let request_payload = request
        .messages
        .iter()
        .map(|message| format!("{}:{}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n");
    let mut core_request = request.clone().into_responses_request();
    let request_model = core_request.model.clone();
    let provider = state.resolve_provider_key(&core_request.model);
    let provider_model = state.resolve_provider_model_id(&core_request.model);
    let public_model_id = synthesize_model_id(&provider, &provider_model);
    let auth_bearer = match resolve_byok_bearer(
        &headers,
        state.byok_enabled,
        provider.as_str(),
        "/api/v1/chat/completions",
    ) {
        Ok(token) => token,
        Err(err) => return error_response(err),
    };
    request_span.record("model", public_model_id.as_str());
    request_span.record("provider", provider.as_str());
    request_span.record("stream", request.stream);
    request_span.record("input.value", truncate_attr_value(&request_payload, 512));
    core_request.model = provider_model;
    info!(
        event = "http.request.received",
        route = "/api/v1/chat/completions",
        model = %public_model_id,
        provider = %provider,
        stream = request.stream,
        message_count = request.messages.len()
    );
    debug!(
        event = "http.request.payload",
        route = "/api/v1/chat/completions",
        model = %request_model,
        provider = %provider,
        request_text = %request_payload
    );
    let engine = match state.resolve_engine(&core_request.model) {
        Ok(engine) => engine,
        Err(err) => {
            warn!(
                event = "http.request.failed",
                route = "/api/v1/chat/completions",
                model = %public_model_id,
                provider = %provider,
                duration_ms = started_at.elapsed().as_millis() as u64,
                error = %err
            );
            return error_response(err);
        }
    };

    if request.stream {
        let chat_completion_id = new_prefixed_id("chatcmpl_");
        info!(
            event = "http.stream.started",
            route = "/api/v1/chat/completions",
            model = %public_model_id,
            provider = %provider
        );
        let stream_provider = provider.clone();
        let stream_route = "/api/v1/chat/completions".to_string();
        let stream_request_span = request_span.clone();
        let stream_started_at = started_at;
        let stream = spawn_engine_stream(engine.clone(), core_request, auth_bearer.clone()).map(
                move |evt| {
                    if let Ok(ref mapped) = evt {
                        if let Some(request_id) = response_event_request_id(mapped) {
                            stream_request_span.record("request.id", request_id);
                            stream_request_span.record("response.id", request_id);
                        }
                        record_response_event_classification(
                            stream_route.as_str(),
                            stream_provider.as_str(),
                            "chat_completions_sse",
                            mapped,
                        );
                    }
                    match evt {
                        Ok(ResponseEvent::OutputTextDelta { delta, .. }) => {
                            Ok::<Event, Infallible>(Event::default().data(
                                json!({
                                    "id": chat_completion_id.clone(),
                                    "object": "chat.completion.chunk",
                                    "choices": [{"delta": {"content": delta}, "index": 0, "finish_reason": Value::Null}]
                                })
                                .to_string(),
                            ))
                        }
                        Ok(ResponseEvent::ReasoningDelta { delta, .. }) => {
                            Ok::<Event, Infallible>(Event::default().data(
                                json!({
                                    "id": chat_completion_id.clone(),
                                    "object": "chat.completion.chunk",
                                    "choices": [{
                                        "delta": {"reasoning_content": delta},
                                        "index": 0,
                                        "finish_reason": Value::Null
                                    }]
                                })
                                .to_string(),
                            ))
                        }
                        Ok(ResponseEvent::ResponseCompleted {
                            id,
                            output,
                            finish_reason,
                            ..
                        }) => {
                            let reasoning = extract_reasoning_from_output(&output);
                            let tool_calls = extract_tool_calls_from_output(&output);
                            info!(
                                event = "http.stream.completed",
                                route = "/api/v1/chat/completions",
                                response_id = %id,
                                provider = %stream_provider,
                                finish_reason = %finish_reason,
                                reasoning_present = reasoning.is_some(),
                                reasoning_chars = reasoning.as_ref().map(|it| it.len()).unwrap_or(0),
                                duration_ms = stream_started_at.elapsed().as_millis() as u64
                            );
                            let chunk = if let Some(tool_call) =
                                tool_calls.as_ref().and_then(|calls| calls.first())
                            {
                                json!({
                                    "id": chat_completion_id.clone(),
                                    "object": "chat.completion.chunk",
                                    "choices": [{
                                        "delta": {"tool_calls": [{"index": 0, "id": tool_call.id, "type": tool_call.kind, "function": tool_call.function}]},
                                        "index": 0,
                                        "finish_reason": "tool_calls"
                                    }]
                                })
                            } else {
                                json!({
                                    "id": chat_completion_id.clone(),
                                    "object": "chat.completion.chunk",
                                    "choices": [{"delta": {}, "index": 0, "finish_reason": "stop"}]
                                })
                            };
                            Ok(Event::default().data(chunk.to_string()))
                        }
                        Ok(ResponseEvent::ResponseError { id, message }) => {
                            stream_request_span.set_status(Status::error(message.clone()));
                            warn!(
                                event = "http.stream.failed",
                                route = "/api/v1/chat/completions",
                                response_id = %id,
                                provider = %stream_provider,
                                duration_ms = stream_started_at.elapsed().as_millis() as u64,
                                error = %message
                            );
                            Ok(Event::default().data(
                                json!({"id": chat_completion_id.clone(), "error": message})
                                    .to_string(),
                            ))
                        }
                        Err(error) => {
                            stream_request_span.set_status(Status::error(error.to_string()));
                            warn!(
                                event = "http.stream.failed",
                                route = "/api/v1/chat/completions",
                                provider = %stream_provider,
                                duration_ms = stream_started_at.elapsed().as_millis() as u64,
                                error = %error
                            );
                            Ok(Event::default().data(
                                json!({"id": chat_completion_id.clone(), "error": error.to_string()})
                                    .to_string(),
                            ))
                        }
                    }
                },
            );

        let done =
            futures::stream::iter(vec![Ok::<Event, Infallible>(Event::default().data("[DONE]"))]);
        return Sse::new(stream.chain(done)).into_response();
    }

    match run_responses_request(engine, core_request, auth_bearer).await {
        Ok(mut resp) => {
            resp.id = ensure_id_prefix(&resp.id, "resp_");
            request_span.record("request.id", resp.id.as_str());
            request_span.record("response.id", resp.id.as_str());
            let response_text = extract_message_text_from_output(&resp.output);
            request_span.record("output.value", truncate_attr_value(&response_text, 512));
            let reasoning = extract_reasoning_from_output(&resp.output);
            debug!(
                event = "http.response.payload",
                route = "/api/v1/chat/completions",
                model = %request_model,
                provider = %provider,
                response_text = %response_text
            );
            info!(
                event = "http.request.succeeded",
                route = "/api/v1/chat/completions",
                model = %request_model,
                provider = %provider,
                status = %resp.status,
                finish_reason = %resp.finish_reason,
                reasoning_present = reasoning.is_some(),
                reasoning_chars = reasoning.as_ref().map(|it| it.len()).unwrap_or(0),
                input_tokens = resp.usage.input_tokens,
                output_tokens = resp.usage.output_tokens,
                total_tokens = resp.usage.total_tokens,
                duration_ms = started_at.elapsed().as_millis() as u64
            );
            let mut chat = ChatCompletionsResponse::from_responses(resp);
            chat.id = ensure_id_prefix(&chat.id, "chatcmpl_");
            Json(chat).into_response()
        }
        Err(err) => {
            request_span.set_status(Status::error(err.to_string()));
            warn!(
                event = "http.request.failed",
                route = "/api/v1/chat/completions",
                model = %request_model,
                provider = %provider,
                duration_ms = started_at.elapsed().as_millis() as u64,
                error = %err
            );
            error_response(err)
        }
    }
}

async fn run_responses_request(
    engine: Arc<ExecutionEngine>,
    request: ResponsesRequest,
    auth_bearer: Option<String>,
) -> Result<ResponsesResponse, CoreError> {
    engine.execute_with_auth(request, auth_bearer).await
}

struct HeaderMapExtractor<'a>(&'a HeaderMap);

impl<'a> Extractor for HeaderMapExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(axum::http::header::HeaderName::as_str).collect()
    }
}

fn attach_parent_context(span: &Span, headers: &HeaderMap) {
    global::get_text_map_propagator(|propagator| {
        let context = propagator.extract(&HeaderMapExtractor(headers));
        span.set_parent(context);
    });
}

fn response_event_otel_name(event: &ResponseEvent) -> &'static str {
    match event {
        ResponseEvent::OutputTextDelta { .. } => "output_text_delta",
        ResponseEvent::ReasoningDelta { .. } => "reasoning_delta",
        ResponseEvent::ResponseCompleted { .. } => "completed",
        ResponseEvent::ResponseError { .. } => "error",
    }
}

fn response_event_request_id(event: &ResponseEvent) -> Option<&str> {
    match event {
        ResponseEvent::OutputTextDelta { id, .. }
        | ResponseEvent::ReasoningDelta { id, .. }
        | ResponseEvent::ResponseCompleted { id, .. }
        | ResponseEvent::ResponseError { id, .. } => Some(id.as_str()),
    }
}

fn record_response_event_classification(
    route: &str,
    provider: &str,
    from: &str,
    event: &ResponseEvent,
) {
    let span = trace_span!(
        "handle_responses",
        otel.kind = "internal",
        openinference.span.kind = "CHAIN",
        otel.name = field::Empty,
        request.id = field::Empty,
        tool_name = field::Empty,
        from = from,
        route = route,
        provider = provider
    );
    if let Some(request_id) = response_event_request_id(event) {
        span.record("request.id", request_id);
    }
    span.record("otel.name", response_event_otel_name(event));
    if let ResponseEvent::ResponseCompleted { output, .. } = event
        && let Some(name) = first_function_tool_name(output)
    {
        span.record("tool_name", name);
    }
    let _entered = span.enter();
}

fn first_function_tool_name(output: &[ResponseOutputItem]) -> Option<&str> {
    output.iter().find_map(|item| match item {
        ResponseOutputItem::FunctionCall { name, .. } if !name.trim().is_empty() => {
            Some(name.as_str())
        }
        _ => None,
    })
}

fn ensure_id_prefix(id: &str, prefix: &str) -> String {
    if id.starts_with(prefix) { id.to_string() } else { format!("{prefix}{id}") }
}

fn new_prefixed_id(prefix: &str) -> String {
    format!("{prefix}{}", uuid::Uuid::new_v4().simple())
}

fn preview_request_body(body: &[u8]) -> String {
    const MAX_PREVIEW_CHARS: usize = 400;
    let text = String::from_utf8_lossy(body);
    let mut preview = text.chars().take(MAX_PREVIEW_CHARS).collect::<String>().replace('\n', "\\n");
    if text.chars().count() > MAX_PREVIEW_CHARS {
        preview.push_str("...");
    }
    preview
}

fn truncate_attr_value(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

fn extract_message_text_from_output(output: &[ResponseOutputItem]) -> String {
    output
        .iter()
        .find_map(|item| {
            if let ResponseOutputItem::Message { content, .. } = item {
                content.first().map(|part| part.text.clone())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn extract_reasoning_from_output(output: &[ResponseOutputItem]) -> Option<String> {
    output.iter().find_map(|item| {
        if let ResponseOutputItem::Reasoning { summary, .. } = item {
            summary.first().map(|s| s.text.clone())
        } else {
            None
        }
    })
}

fn extract_tool_calls_from_output(
    output: &[ResponseOutputItem],
) -> Option<Vec<xrouter_contracts::ToolCall>> {
    let mut calls = Vec::new();
    for item in output {
        if let ResponseOutputItem::FunctionCall { call_id, name, arguments, .. } = item {
            calls.push(xrouter_contracts::ToolCall {
                id: call_id.clone(),
                kind: "function".to_string(),
                function: xrouter_contracts::ToolFunction {
                    name: name.clone(),
                    arguments: arguments.clone(),
                },
            });
        }
    }
    if calls.is_empty() { None } else { Some(calls) }
}
