use std::{collections::HashMap, convert::Infallible, sync::Arc};

use axum::{
    extract::State,
    response::{sse::Event, IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::instrument;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use xrouter_clients_openai::OpenAiCompatibleClient;
#[cfg(feature = "billing")]
use xrouter_clients_usage::InMemoryUsageClient;
use xrouter_contracts::{
    ChatCompletionsRequest, ChatCompletionsResponse, ResponseEvent, ResponsesRequest,
    ResponsesResponse,
};
use xrouter_core::{CoreError, ExecutionEngine};

pub mod config;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelCatalogEntry {
    id: String,
    provider: String,
    context_length: u32,
    max_completion_tokens: u32,
}

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
        get_compatible_models,
        get_xrouter_models,
        post_responses,
        post_chat_completions
    ),
    components(
        schemas(
            HealthResponse,
            ErrorResponse,
            CompatibleModelEntry,
            CompatibleModelsResponse,
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
struct ApiDoc;

#[derive(Clone)]
pub struct AppState {
    openai_compatible_api: bool,
    default_provider: String,
    models: Vec<ModelCatalogEntry>,
    engines: HashMap<String, Arc<ExecutionEngine>>,
}

impl AppState {
    pub fn from_config(config: &config::AppConfig) -> Self {
        let mut engines = HashMap::new();
        for (provider, provider_config) in &config.providers {
            if !provider_config.enabled {
                continue;
            }
            let client = Arc::new(OpenAiCompatibleClient::new(provider.to_string()));
            #[cfg(feature = "billing")]
            let engine =
                Arc::new(ExecutionEngine::new(client, Arc::new(InMemoryUsageClient::default()), false));
            #[cfg(not(feature = "billing"))]
            let engine = Arc::new(ExecutionEngine::new(client, false));
            engines.insert(provider.to_string(), engine);
        }

        let models = vec![
            ModelCatalogEntry {
                id: "gpt-4.1-mini".to_string(),
                provider: "openrouter".to_string(),
                context_length: 128_000,
                max_completion_tokens: 16_384,
            },
            ModelCatalogEntry {
                id: "openrouter/anthropic/claude-3.5-sonnet".to_string(),
                provider: "openrouter".to_string(),
                context_length: 200_000,
                max_completion_tokens: 8_192,
            },
            ModelCatalogEntry {
                id: "deepseek/deepseek-chat".to_string(),
                provider: "deepseek".to_string(),
                context_length: 64_000,
                max_completion_tokens: 8_192,
            },
            ModelCatalogEntry {
                id: "gigachat/GigaChat-2-Max".to_string(),
                provider: "gigachat".to_string(),
                context_length: 32_768,
                max_completion_tokens: 8_192,
            },
            ModelCatalogEntry {
                id: "yandex/yandexgpt-32k".to_string(),
                provider: "yandex".to_string(),
                context_length: 32_768,
                max_completion_tokens: 8_192,
            },
            ModelCatalogEntry {
                id: "ollama/llama3.1:8b".to_string(),
                provider: "ollama".to_string(),
                context_length: 8_192,
                max_completion_tokens: 4_096,
            },
            ModelCatalogEntry {
                id: "zai/glm-4.5".to_string(),
                provider: "zai".to_string(),
                context_length: 128_000,
                max_completion_tokens: 8_192,
            },
            ModelCatalogEntry {
                id: "xrouter/gpt-4.1-mini".to_string(),
                provider: "xrouter".to_string(),
                context_length: 128_000,
                max_completion_tokens: 16_384,
            },
        ];

        Self {
            openai_compatible_api: config.openai_compatible_api,
            default_provider: "openrouter".to_string(),
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
    let router = if openai_compatible_api {
        Router::new()
            .route("/health", get(get_health))
            .route("/v1/models", get(get_compatible_models))
            .route("/v1/responses", post(post_responses))
            .route("/v1/chat/completions", post(post_chat_completions))
    } else {
        Router::new()
            .route("/health", get(get_health))
            .route("/api/v1/models", get(get_xrouter_models))
            .route("/api/v1/responses", post(post_responses))
            .route("/api/v1/chat/completions", post(post_chat_completions))
    };

    router.with_state(state).merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
}

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
    Json(CompatibleModelsResponse { object: "list".to_string(), data })
}

#[utoipa::path(
    get,
    path = "/api/v1/models",
    responses((status = 200, description = "xrouter model list", body = XrouterModelsResponse)),
    tag = "xrouter-app"
)]
async fn get_xrouter_models(State(state): State<AppState>) -> Json<XrouterModelsResponse> {
    let data = state
        .models
        .iter()
        .map(|m| XrouterModelEntry {
            id: m.id.clone(),
            name: m.id.clone(),
            description: format!("{} model", m.provider),
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
#[instrument(skip(state, request), fields(model = %request.model, stream = request.stream))]
async fn post_responses(
    State(state): State<AppState>,
    Json(request): Json<ResponsesRequest>,
) -> Response {
    let engine = match state.resolve_engine(&request.model) {
        Ok(engine) => engine,
        Err(err) => return error_response(err),
    };

    if request.stream {
        let response_id = format!("resp_{}", uuid::Uuid::new_v4().simple());
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
            Ok(ResponseEvent::ResponseCompleted { usage, .. }) => {
                Ok(Event::default().event("response.completed").data(
                    json!({
                        "type": "response.completed",
                        "response": {
                            "id": response_id,
                            "status": "completed",
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
            Ok(ResponseEvent::ResponseError { message, .. }) => Ok(Event::default()
                .event("response.error")
                .data(json!({"type": "response.error", "error": message}).to_string())),
            Err(error) => Ok(Event::default()
                .event("response.error")
                .data(json!({"type": "response.error", "error": error.to_string()}).to_string())),
        });

        let bootstrap = futures::stream::iter(vec![Ok::<Event, Infallible>(
            Event::default().event("response.created").data(created.to_string()),
        )]);
        let full_stream = bootstrap.chain(stream);
        return Sse::new(full_stream).into_response();
    }

    match run_responses_request(engine, request).await {
        Ok(resp) => Json(resp).into_response(),
        Err(err) => error_response(err),
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
#[instrument(skip(state, request), fields(model = %request.model, stream = request.stream))]
async fn post_chat_completions(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionsRequest>,
) -> Response {
    let core_request = request.clone().into_responses_request();
    let engine = match state.resolve_engine(&core_request.model) {
        Ok(engine) => engine,
        Err(err) => return error_response(err),
    };

    if request.stream {
        let stream = engine.execute_stream(core_request, None).map(|evt| match evt {
            Ok(ResponseEvent::OutputTextDelta { id, delta }) => Ok::<Event, Infallible>(
                Event::default().data(
                    json!({
                        "id": id,
                        "object": "chat.completion.chunk",
                        "choices": [{"delta": {"content": delta}, "index": 0, "finish_reason": Value::Null}]
                    })
                    .to_string(),
                ),
            ),
            Ok(ResponseEvent::ResponseCompleted { id, .. }) => Ok(Event::default().data(
                json!({
                    "id": id,
                    "object": "chat.completion.chunk",
                    "choices": [{"delta": {}, "index": 0, "finish_reason": "stop"}]
                })
                .to_string(),
            )),
            Ok(ResponseEvent::ResponseError { id, message }) => {
                Ok(Event::default().data(json!({"id": id, "error": message}).to_string()))
            }
            Err(error) => Ok(Event::default().data(json!({"error": error.to_string()}).to_string())),
        });

        let done =
            futures::stream::iter(vec![Ok::<Event, Infallible>(Event::default().data("[DONE]"))]);
        return Sse::new(stream.chain(done)).into_response();
    }

    match run_responses_request(engine, core_request).await {
        Ok(resp) => Json(ChatCompletionsResponse::from_responses(resp)).into_response(),
        Err(err) => error_response(err),
    }
}

async fn run_responses_request(
    engine: Arc<ExecutionEngine>,
    request: ResponsesRequest,
) -> Result<ResponsesResponse, CoreError> {
    engine.execute(request).await
}

fn error_response(err: CoreError) -> Response {
    (axum::http::StatusCode::BAD_REQUEST, Json(ErrorResponse { error: err.to_string() }))
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::body::to_bytes;
    use axum::body::Body;
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

        if let Some(output_text) = obj.get("output_text").and_then(Value::as_str) {
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
json.first_id=gpt-4.1-mini
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
text.body=Not Found
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
json.first_id=gpt-4.1-mini
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
text.body=Not Found
"#,
            ),
        ];

        for (fixture, expected) in fixtures {
            check_fixture(fixture, expected, true).await;
        }
    }
}
