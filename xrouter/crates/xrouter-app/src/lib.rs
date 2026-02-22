use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    sync::Arc,
    time::Instant,
};

use axum::{
    Json, Router,
    extract::State,
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::{get, post},
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{debug, error, info, warn};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use xrouter_clients_openai::OpenAiCompatibleClient;
#[cfg(feature = "billing")]
use xrouter_clients_usage::InMemoryUsageClient;
use xrouter_contracts::{
    ChatCompletionsRequest, ChatCompletionsResponse, ResponseEvent, ResponseOutputItem,
    ResponsesRequest, ResponsesResponse,
};
use xrouter_core::{CoreError, ExecutionEngine, ModelDescriptor, default_model_catalog};

pub mod config;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct CompatibleModelEntry {
    id: String,
    object: String,
    created: i64,
    owned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct CompatibleModelsResponse {
    object: String,
    data: Vec<CompatibleModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct ModelArchitecture {
    modality: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct ModelTopProvider {
    context_length: u32,
    max_completion_tokens: u32,
    is_moderated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct ModelPerRequestLimits {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct XrouterModelEntry {
    id: String,
    name: String,
    description: String,
    context_length: u32,
    architecture: ModelArchitecture,
    top_provider: ModelTopProvider,
    per_request_limits: ModelPerRequestLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct XrouterModelsResponse {
    data: Vec<XrouterModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct ErrorResponse {
    error: String,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_health,
        get_xrouter_models,
        post_responses,
        post_chat_completions
    ),
    components(
        schemas(
            HealthResponse,
            ErrorResponse,
            ModelArchitecture,
            ModelTopProvider,
            ModelPerRequestLimits,
            XrouterModelEntry,
            XrouterModelsResponse,
            ResponsesRequest,
            ResponsesResponse,
            ChatCompletionsRequest,
            ChatCompletionsResponse
        )
    ),
    tags(
        (name = "xrouter-app", description = "xrouter application API")
    )
)]
struct XrouterApiDoc;

#[derive(OpenApi)]
#[openapi(
    paths(
        get_health,
        get_compatible_models,
        post_responses_openai_doc,
        post_chat_completions_openai_doc
    ),
    components(
        schemas(
            HealthResponse,
            ErrorResponse,
            CompatibleModelEntry,
            CompatibleModelsResponse,
            ResponsesRequest,
            ResponsesResponse,
            ChatCompletionsRequest,
            ChatCompletionsResponse
        )
    ),
    tags(
        (name = "xrouter-app", description = "xrouter application API")
    )
)]
struct OpenAiApiDoc;

#[derive(Clone)]
pub struct AppState {
    openai_compatible_api: bool,
    default_provider: String,
    models: Vec<ModelDescriptor>,
    engines: HashMap<String, Arc<ExecutionEngine>>,
}

impl AppState {
    pub fn from_config(config: &config::AppConfig) -> Self {
        let enabled_providers = config
            .providers
            .iter()
            .filter_map(|(name, provider_config)| provider_config.enabled.then_some(name.clone()))
            .collect::<HashSet<_>>();
        info!(
            event = "app.config.loaded",
            openai_compatible_api = config.openai_compatible_api,
            provider_total = config.providers.len(),
            provider_enabled = enabled_providers.len()
        );
        debug!(event = "app.config.providers", enabled_providers = ?enabled_providers);

        let mut engines = HashMap::new();
        for (provider, provider_config) in &config.providers {
            if !provider_config.enabled {
                continue;
            }
            let client = Arc::new(OpenAiCompatibleClient::new(provider.to_string()));
            #[cfg(feature = "billing")]
            let engine = Arc::new(ExecutionEngine::new(
                client,
                Arc::new(InMemoryUsageClient::default()),
                false,
            ));
            #[cfg(not(feature = "billing"))]
            let engine = Arc::new(ExecutionEngine::new(client, false));
            engines.insert(provider.to_string(), engine);
        }
        info!(event = "app.engines.initialized", engine_count = engines.len());
        debug!(
            event = "app.engines.providers",
            providers = ?engines.keys().collect::<Vec<_>>()
        );

        let models = default_model_catalog()
            .into_iter()
            .filter(|entry| enabled_providers.contains(&entry.provider))
            .collect::<Vec<_>>();
        info!(event = "models.registry.loaded", model_count = models.len());
        debug!(
            event = "models.registry.entries",
            model_ids = ?models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
        );

        let default_provider = if models.iter().any(|entry| entry.provider == "openrouter") {
            "openrouter".to_string()
        } else {
            models
                .first()
                .map(|entry| entry.provider.clone())
                .unwrap_or_else(|| "openrouter".to_string())
        };

        Self {
            openai_compatible_api: config.openai_compatible_api,
            default_provider,
            models,
            engines,
        }
    }

    pub fn new(_billing_enabled: bool) -> Self {
        Self::from_config(&config::AppConfig::for_tests())
    }

    fn resolve_provider_key(&self, model: &str) -> String {
        if let Some((candidate, _rest)) = model.split_once('/')
            && self.engines.contains_key(candidate)
        {
            return candidate.to_string();
        }

        if let Some(found) = self.models.iter().find(|m| m.id == model) {
            return found.provider.clone();
        }

        self.default_provider.clone()
    }

    fn resolve_engine(&self, model: &str) -> Result<Arc<ExecutionEngine>, CoreError> {
        let key = self.resolve_provider_key(model);
        self.engines.get(&key).cloned().ok_or_else(|| {
            CoreError::Validation(format!("unsupported provider for model: {model}"))
        })
    }
}

pub fn build_router(state: AppState) -> Router {
    let openai_compatible_api = state.openai_compatible_api;
    let (router, openapi) = if openai_compatible_api {
        (
            Router::new()
                .route("/health", get(get_health))
                .route("/v1/models", get(get_compatible_models))
                .route("/v1/responses", post(post_responses))
                .route("/v1/chat/completions", post(post_chat_completions)),
            OpenAiApiDoc::openapi(),
        )
    } else {
        (
            Router::new()
                .route("/health", get(get_health))
                .route("/api/v1/models", get(get_xrouter_models))
                .route("/api/v1/responses", post(post_responses))
                .route("/api/v1/chat/completions", post(post_chat_completions)),
            XrouterApiDoc::openapi(),
        )
    };

    router.with_state(state).merge(SwaggerUi::new("/docs").url("/openapi.json", openapi))
}

#[allow(dead_code)]
#[utoipa::path(
    post,
    path = "/v1/responses",
    request_body = ResponsesRequest,
    responses(
        (status = 200, description = "Responses API result", body = ResponsesResponse),
        (status = 400, description = "Validation or provider error", body = ErrorResponse)
    ),
    tag = "xrouter-app"
)]
fn post_responses_openai_doc() {}

#[allow(dead_code)]
#[utoipa::path(
    post,
    path = "/v1/chat/completions",
    request_body = ChatCompletionsRequest,
    responses(
        (status = 200, description = "Chat Completions API result", body = ChatCompletionsResponse),
        (status = 400, description = "Validation or provider error", body = ErrorResponse)
    ),
    tag = "xrouter-app"
)]
fn post_chat_completions_openai_doc() {}

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Service health", body = HealthResponse)),
    tag = "xrouter-app"
)]
async fn get_health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "healthy".to_string() })
}

#[utoipa::path(
    get,
    path = "/v1/models",
    responses((status = 200, description = "OpenAI-compatible model list", body = CompatibleModelsResponse)),
    tag = "xrouter-app"
)]
async fn get_compatible_models(State(state): State<AppState>) -> Json<CompatibleModelsResponse> {
    debug!(event = "http.request.received", route = "/v1/models", openai_compatible_api = true);
    let data = state
        .models
        .iter()
        .map(|m| CompatibleModelEntry {
            id: m.id.clone(),
            object: "model".to_string(),
            created: 1_710_979_200,
            owned_by: m.provider.clone(),
        })
        .collect::<Vec<_>>();
    info!(event = "http.models.served", route = "/v1/models", model_count = data.len());
    debug!(
        event = "http.models.ids",
        route = "/v1/models",
        model_ids = ?data.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
    );
    Json(CompatibleModelsResponse { object: "list".to_string(), data })
}

#[utoipa::path(
    get,
    path = "/api/v1/models",
    responses((status = 200, description = "xrouter model list", body = XrouterModelsResponse)),
    tag = "xrouter-app"
)]
async fn get_xrouter_models(State(state): State<AppState>) -> Json<XrouterModelsResponse> {
    debug!(
        event = "http.request.received",
        route = "/api/v1/models",
        openai_compatible_api = false
    );
    let data = state
        .models
        .iter()
        .map(|m| XrouterModelEntry {
            id: m.id.clone(),
            name: m.id.clone(),
            description: m.description.clone(),
            context_length: m.context_length,
            architecture: ModelArchitecture { modality: "text->text".to_string() },
            top_provider: ModelTopProvider {
                context_length: m.context_length,
                max_completion_tokens: m.max_completion_tokens,
                is_moderated: true,
            },
            per_request_limits: ModelPerRequestLimits {
                prompt_tokens: m.context_length.saturating_sub(1024),
                completion_tokens: m.max_completion_tokens,
            },
        })
        .collect::<Vec<_>>();
    info!(event = "http.models.served", route = "/api/v1/models", model_count = data.len());
    debug!(
        event = "http.models.ids",
        route = "/api/v1/models",
        model_ids = ?data.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
    );
    Json(XrouterModelsResponse { data })
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
async fn post_responses(
    State(state): State<AppState>,
    Json(request): Json<ResponsesRequest>,
) -> Response {
    let started_at = Instant::now();
    let request_model = request.model.clone();
    let provider = state.resolve_provider_key(&request.model);
    info!(
        event = "http.request.received",
        route = "/api/v1/responses",
        model = %request.model,
        provider = %provider,
        stream = request.stream,
        input_chars = request.input.len()
    );
    debug!(
        event = "http.request.payload",
        route = "/api/v1/responses",
        model = %request_model,
        provider = %provider,
        request_text = %request.input
    );

    let engine = match state.resolve_engine(&request.model) {
        Ok(engine) => engine,
        Err(err) => {
            warn!(
                event = "http.request.failed",
                route = "/api/v1/responses",
                model = %request.model,
                provider = %provider,
                stream = request.stream,
                duration_ms = started_at.elapsed().as_millis() as u64,
                error = %err
            );
            return error_response(err);
        }
    };

    if request.stream {
        let response_id = new_prefixed_id("resp_");
        info!(
            event = "http.stream.started",
            route = "/api/v1/responses",
            response_id = %response_id,
            model = %request.model,
            provider = %provider
        );
        let created = json!({
            "type": "response.created",
            "response": {
                "id": response_id,
                "object": "response",
                "status": "in_progress",
                "model": request.model,
                "output": []
            }
        });

        let stream = engine.execute_stream(request, None).map(move |event| match event {
            Ok(ResponseEvent::OutputTextDelta { delta, .. }) => Ok::<Event, Infallible>(
                Event::default().event("response.output_text.delta").data(
                    json!({
                        "type": "response.output_text.delta",
                        "output_index": 0,
                        "item_id": "msg_0",
                        "content_index": 0,
                        "delta": delta
                    })
                    .to_string(),
                ),
            ),
            Ok(ResponseEvent::ReasoningDelta { delta, .. }) => Ok::<Event, Infallible>(
                Event::default().event("response.reasoning.delta").data(
                    json!({
                        "type": "response.reasoning.delta",
                        "delta": delta
                    })
                    .to_string(),
                ),
            ),
            Ok(ResponseEvent::ResponseCompleted { output, finish_reason, usage, .. }) => {
                let reasoning = extract_reasoning_from_output(&output);
                info!(
                    event = "http.stream.completed",
                    route = "/api/v1/responses",
                    response_id = %response_id,
                    provider = %provider,
                    finish_reason = %finish_reason,
                    reasoning_present = reasoning.is_some(),
                    reasoning_chars = reasoning.as_ref().map(|it| it.len()).unwrap_or(0),
                    input_tokens = usage.input_tokens,
                    output_tokens = usage.output_tokens,
                    total_tokens = usage.total_tokens,
                    duration_ms = started_at.elapsed().as_millis() as u64
                );
                Ok(Event::default().event("response.completed").data(
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
                ))
            }
            Ok(ResponseEvent::ResponseError { message, .. }) => {
                warn!(
                    event = "http.stream.failed",
                    route = "/api/v1/responses",
                    response_id = %response_id,
                    provider = %provider,
                    duration_ms = started_at.elapsed().as_millis() as u64,
                    error = %message
                );
                Ok(Event::default()
                    .event("response.error")
                    .data(json!({"type": "response.error", "error": message}).to_string()))
            }
            Err(error) => {
                warn!(
                    event = "http.stream.failed",
                    route = "/api/v1/responses",
                    response_id = %response_id,
                    provider = %provider,
                    duration_ms = started_at.elapsed().as_millis() as u64,
                    error = %error
                );
                Ok(Event::default().event("response.error").data(
                    json!({"type": "response.error", "error": error.to_string()}).to_string(),
                ))
            }
        });

        let bootstrap = futures::stream::iter(vec![Ok::<Event, Infallible>(
            Event::default().event("response.created").data(created.to_string()),
        )]);
        let full_stream = bootstrap.chain(stream);
        return Sse::new(full_stream).into_response();
    }

    match run_responses_request(engine, request).await {
        Ok(mut resp) => {
            resp.id = ensure_id_prefix(&resp.id, "resp_");
            let response_text = extract_message_text_from_output(&resp.output);
            let reasoning = extract_reasoning_from_output(&resp.output);
            debug!(
                event = "http.response.payload",
                route = "/api/v1/responses",
                model = %request_model,
                provider = %provider,
                response_text = %response_text
            );
            info!(
                event = "http.request.succeeded",
                route = "/api/v1/responses",
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
            warn!(
                event = "http.request.failed",
                route = "/api/v1/responses",
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
async fn post_chat_completions(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionsRequest>,
) -> Response {
    let started_at = Instant::now();
    let request_payload = request
        .messages
        .iter()
        .map(|message| format!("{}:{}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n");
    let core_request = request.clone().into_responses_request();
    let request_model = core_request.model.clone();
    let provider = state.resolve_provider_key(&core_request.model);
    info!(
        event = "http.request.received",
        route = "/api/v1/chat/completions",
        model = %core_request.model,
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
                model = %core_request.model,
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
            model = %core_request.model,
            provider = %provider
        );
        let stream_provider = provider.clone();
        let stream_started_at = started_at;
        let stream = engine.execute_stream(core_request, None).map(move |evt| match evt {
            Ok(ResponseEvent::OutputTextDelta { delta, .. }) => Ok::<Event, Infallible>(
                Event::default().data(
                    json!({
                        "id": chat_completion_id.clone(),
                        "object": "chat.completion.chunk",
                        "choices": [{"delta": {"content": delta}, "index": 0, "finish_reason": Value::Null}]
                    })
                    .to_string(),
                ),
            ),
            Ok(ResponseEvent::ReasoningDelta { delta, .. }) => Ok::<Event, Infallible>(
                Event::default().data(
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
                ),
            ),
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
                warn!(
                    event = "http.stream.failed",
                    route = "/api/v1/chat/completions",
                    response_id = %id,
                    provider = %stream_provider,
                    duration_ms = stream_started_at.elapsed().as_millis() as u64,
                    error = %message
                );
                Ok(Event::default()
                    .data(json!({"id": chat_completion_id.clone(), "error": message}).to_string()))
            }
            Err(error) => {
                warn!(
                    event = "http.stream.failed",
                    route = "/api/v1/chat/completions",
                    provider = %stream_provider,
                    duration_ms = stream_started_at.elapsed().as_millis() as u64,
                    error = %error
                );
                Ok(Event::default()
                    .data(json!({"id": chat_completion_id.clone(), "error": error.to_string()}).to_string()))
            }
        });

        let done =
            futures::stream::iter(vec![Ok::<Event, Infallible>(Event::default().data("[DONE]"))]);
        return Sse::new(stream.chain(done)).into_response();
    }

    match run_responses_request(engine, core_request).await {
        Ok(mut resp) => {
            resp.id = ensure_id_prefix(&resp.id, "resp_");
            let response_text = extract_message_text_from_output(&resp.output);
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
) -> Result<ResponsesResponse, CoreError> {
    engine.execute(request).await
}

fn error_response(err: CoreError) -> Response {
    match &err {
        CoreError::Validation(_) | CoreError::Provider(_) => {
            warn!(event = "http.error_response", error = %err);
        }
        CoreError::Billing(_) | CoreError::ClientDisconnected(_) => {
            error!(event = "http.error_response", error = %err);
        }
    }
    (axum::http::StatusCode::BAD_REQUEST, Json(ErrorResponse { error: err.to_string() }))
        .into_response()
}

fn ensure_id_prefix(id: &str, prefix: &str) -> String {
    if id.starts_with(prefix) { id.to_string() } else { format!("{prefix}{id}") }
}

fn new_prefixed_id(prefix: &str) -> String {
    format!("{prefix}{}", uuid::Uuid::new_v4().simple())
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::body::Body;
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use serde_json::Map;
    use tower::ServiceExt;

    use super::*;

    #[derive(Debug)]
    struct AppFixture<'a> {
        name: &'a str,
        method: &'a str,
        path: &'a str,
        body: Option<&'a str>,
    }

    impl<'a> AppFixture<'a> {
        fn parse(raw: &'a str) -> Self {
            let mut fixture = Self { name: "unnamed", method: "GET", path: "/health", body: None };

            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim();
                let value = value.trim();

                match key {
                    "name" => fixture.name = value,
                    "method" => fixture.method = value,
                    "path" => fixture.path = value,
                    "body" => fixture.body = Some(value),
                    other => panic!("unsupported fixture key: {other}"),
                }
            }

            fixture
        }
    }

    fn assert_snapshot(name: &str, actual: &str, expected: &str) {
        let actual = actual.trim();
        let expected = expected.trim();
        assert_eq!(
            actual, expected,
            "snapshot mismatch for fixture `{name}`\n\nactual:\n{actual}\n\nexpected:\n{expected}"
        );
    }

    fn normalize_json(mut value: Value) -> Value {
        fn walk(value: &mut Value) {
            match value {
                Value::Object(map) => {
                    for (key, child) in map.iter_mut() {
                        if key == "id" {
                            *child = Value::String("<id>".to_string());
                        } else {
                            walk(child);
                        }
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        walk(item);
                    }
                }
                _ => {}
            }
        }

        walk(&mut value);
        value
    }

    fn summarize_json(value: Value) -> String {
        let value = normalize_json(value);
        let Some(obj) = value.as_object() else {
            return format!("json={}", value);
        };

        if let Some(status) = obj.get("status").and_then(Value::as_str)
            && obj.len() == 1
        {
            return format!("json.status={status}");
        }

        if let Some(error) = obj.get("error").and_then(Value::as_str) {
            return format!("json.error={error}");
        }

        if let Some(data) = obj.get("data").and_then(Value::as_array) {
            let first_id = data
                .first()
                .and_then(Value::as_object)
                .and_then(|it| it.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("<none>");
            return format!("json.data_len={}\njson.first_id={first_id}", data.len());
        }

        if let Some(output) = obj.get("output").and_then(Value::as_array) {
            let output_text = output
                .iter()
                .find_map(|item| {
                    item.as_object()
                        .filter(|map| map.get("type").and_then(Value::as_str) == Some("message"))
                        .and_then(|map| map.get("content"))
                        .and_then(Value::as_array)
                        .and_then(|arr| arr.first())
                        .and_then(Value::as_object)
                        .and_then(|part| part.get("text"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("");
            let status = obj.get("status").and_then(Value::as_str).unwrap_or("<none>");
            let usage_total = obj
                .get("usage")
                .and_then(Value::as_object)
                .and_then(|usage| usage.get("total_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            return format!(
                "json.status={status}\njson.output_text={}\njson.usage_total={usage_total}",
                output_text.trim_end()
            );
        }

        if obj.get("object").and_then(Value::as_str) == Some("chat.completion") {
            let content = obj
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(Value::as_object)
                .and_then(|choice| choice.get("message"))
                .and_then(Value::as_object)
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            return format!("json.object=chat.completion\njson.choice0={}", content.trim_end());
        }

        let ordered = to_ordered_json(obj);
        format!("json={ordered}")
    }

    fn to_ordered_json(map: &Map<String, Value>) -> Value {
        let mut ordered = BTreeMap::new();
        for (k, v) in map {
            ordered.insert(k.clone(), v.clone());
        }
        serde_json::to_value(ordered).expect("ordered json serialization must succeed")
    }

    fn summarize_text(body: &str) -> String {
        if !body.contains("response.created")
            && !body.contains("response.completed")
            && !body.contains("[DONE]")
        {
            return format!("text.body={}", body.trim());
        }
        format!(
            "text.has_response_created={}\ntext.has_response_completed={}\ntext.has_done_marker={}",
            body.contains("response.created"),
            body.contains("response.completed"),
            body.contains("[DONE]")
        )
    }

    async fn snapshot_response(response: Response) -> String {
        let status = response.status().as_u16();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let body = String::from_utf8_lossy(&body).to_string();
        let summary = match serde_json::from_str::<Value>(&body) {
            Ok(value) => summarize_json(value),
            Err(_) => summarize_text(&body),
        };
        format!("status={status}\n{summary}")
    }

    fn test_app_state(openai_compatible_api: bool) -> AppState {
        let mut config = crate::config::AppConfig::for_tests();
        config.openai_compatible_api = openai_compatible_api;
        AppState::from_config(&config)
    }

    async fn check_fixture(
        raw_fixture: &str,
        expected_snapshot: &str,
        openai_compatible_api: bool,
    ) {
        let fixture = AppFixture::parse(raw_fixture);
        let app = build_router(test_app_state(openai_compatible_api));

        let mut builder = Request::builder().method(fixture.method).uri(fixture.path);

        let request_body = if let Some(body) = fixture.body {
            builder = builder.header("content-type", "application/json");
            Body::from(body.to_string())
        } else {
            Body::empty()
        };

        let response = app
            .oneshot(builder.body(request_body).expect("request must build"))
            .await
            .expect("request must complete");

        if expected_snapshot.contains("status=200") {
            assert_eq!(response.status(), StatusCode::OK);
        }

        let actual_snapshot = snapshot_response(response).await;
        assert_snapshot(fixture.name, &actual_snapshot, expected_snapshot);
    }

    #[tokio::test]
    async fn app_route_fixtures() {
        let fixtures = [
            (
                r#"
name=health
method=GET
path=/health
"#,
                r#"
status=200
json.status=healthy
"#,
            ),
            (
                r#"
name=models_xrouter
method=GET
path=/api/v1/models
"#,
                r#"
status=200
json.data_len=9
json.first_id=<id>
"#,
            ),
            (
                r#"
name=responses_success
method=POST
path=/api/v1/responses
body={"model":"openrouter/anthropic/claude-3.5-sonnet","input":"hello world","stream":false}
"#,
                r#"
status=200
json.status=completed
json.output_text=[openrouter] hello world
json.usage_total=4
"#,
            ),
            (
                r#"
name=responses_validation_error
method=POST
path=/api/v1/responses
body={"model":"gpt-4.1-mini","input":"","stream":false}
"#,
                r#"
status=400
json.error=validation failed: input must not be empty
"#,
            ),
            (
                r#"
name=chat_adapter_success
method=POST
path=/api/v1/chat/completions
body={"model":"gigachat/GigaChat-2-Max","messages":[{"role":"user","content":"hello world"}],"stream":false}
"#,
                r#"
status=200
json.object=chat.completion
json.choice0=[gigachat] user:hello world
"#,
            ),
            (
                r#"
name=responses_stream
method=POST
path=/api/v1/responses
body={"model":"gpt-4.1-mini","input":"hello world","stream":true}
"#,
                r#"
status=200
text.has_response_created=true
text.has_response_completed=true
text.has_done_marker=false
"#,
            ),
            (
                r#"
name=openai_paths_disabled_by_default
method=GET
path=/v1/models
"#,
                r#"
status=404
text.body=
"#,
            ),
        ];

        for (fixture, expected) in fixtures {
            check_fixture(fixture, expected, false).await;
        }
    }

    #[tokio::test]
    async fn app_openai_compatible_paths_fixtures() {
        let fixtures = [
            (
                r#"
name=openai_compatible_models_path
method=GET
path=/v1/models
"#,
                r#"
status=200
json.data_len=9
json.first_id=<id>
"#,
            ),
            (
                r#"
name=xrouter_paths_disabled_in_openai_mode
method=GET
path=/api/v1/models
"#,
                r#"
status=404
text.body=
"#,
            ),
        ];

        for (fixture, expected) in fixtures {
            check_fixture(fixture, expected, true).await;
        }
    }

    #[tokio::test]
    async fn app_models_empty_when_all_providers_disabled() {
        let mut config = crate::config::AppConfig::for_tests();
        for provider in config.providers.values_mut() {
            provider.enabled = false;
        }

        let app = build_router(AppState::from_config(&config));
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/models")
                    .body(Body::empty())
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let snapshot = snapshot_response(response).await;
        assert_snapshot(
            "models_empty_when_all_providers_disabled",
            &snapshot,
            r#"
status=200
json.data_len=0
json.first_id=<none>
"#,
        );
    }

    #[tokio::test]
    async fn responses_non_stream_uses_resp_id_prefix() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","input":"hello","stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let id = payload.get("id").and_then(Value::as_str).expect("id must be present");
        assert!(id.starts_with("resp_"), "unexpected id: {id}");
    }

    #[tokio::test]
    async fn chat_non_stream_uses_chatcmpl_id_prefix() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","messages":[{"role":"user","content":"hello"}],"stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let id = payload.get("id").and_then(Value::as_str).expect("id must be present");
        assert!(id.starts_with("chatcmpl_"), "unexpected id: {id}");
    }

    #[tokio::test]
    async fn chat_stream_emits_chatcmpl_id_and_done_marker() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","messages":[{"role":"user","content":"hello world"}],"stream":true}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload = String::from_utf8_lossy(&body);
        assert!(payload.contains("\"id\":\"chatcmpl_"), "expected chatcmpl id in stream payload");
        assert!(payload.contains("[DONE]"), "expected done marker in stream payload");
    }

    #[tokio::test]
    async fn responses_tool_call_sets_finish_reason_and_tool_call_id() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","input":"TOOL_CALL:get_weather:{\"location\":\"New York\"}","stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        assert_eq!(payload.get("finish_reason").and_then(Value::as_str), Some("tool_calls"));
        let tool_call_id = payload
            .get("output")
            .and_then(Value::as_array)
            .and_then(|arr| {
                arr.iter().find(|item| {
                    item.as_object().and_then(|obj| obj.get("type")).and_then(Value::as_str)
                        == Some("function_call")
                })
            })
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("call_id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(tool_call_id.starts_with("call_"), "unexpected tool_call id: {tool_call_id}");
    }

    #[tokio::test]
    async fn chat_non_stream_maps_tool_call_to_choice_message() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-chat","messages":[{"role":"user","content":"TOOL_CALL:get_weather:{\"location\":\"New York\"}"}],"stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        assert_eq!(
            payload
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_object)
                .and_then(|choice| choice.get("finish_reason"))
                .and_then(Value::as_str),
            Some("tool_calls")
        );
        let tool_call_id = payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_object)
            .and_then(|choice| choice.get("message"))
            .and_then(Value::as_object)
            .and_then(|message| message.get("tool_calls"))
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(tool_call_id.starts_with("call_"), "unexpected tool_call id: {tool_call_id}");
    }

    #[tokio::test]
    async fn responses_reasoner_model_returns_reasoning_field() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-reasoner","input":"Solve 2+2 briefly","stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let reasoning = payload
            .get("output")
            .and_then(Value::as_array)
            .and_then(|arr| {
                arr.iter().find(|item| {
                    item.as_object().and_then(|obj| obj.get("type")).and_then(Value::as_str)
                        == Some("reasoning")
                })
            })
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("summary"))
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(!reasoning.is_empty(), "expected reasoning for deepseek-reasoner");
    }

    #[tokio::test]
    async fn chat_reasoner_model_maps_reasoning_to_message_field() {
        let app = build_router(test_app_state(false));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"deepseek/deepseek-reasoner","messages":[{"role":"user","content":"Solve 2+2 briefly"}],"stream":false}"#,
                    ))
                    .expect("request must build"),
            )
            .await
            .expect("request must complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let reasoning = payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_object)
            .and_then(|choice| choice.get("message"))
            .and_then(Value::as_object)
            .and_then(|message| message.get("reasoning"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(!reasoning.is_empty(), "expected reasoning in chat message for reasoner model");
    }
}
