use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    sync::Arc,
    time::Duration,
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
use xrouter_clients_openai::{
    DeepSeekClient, GigachatClient, MockProviderClient, OpenAiClient, OpenRouterClient,
    YandexResponsesClient, ZaiClient, build_http_client,
};
#[cfg(feature = "billing")]
use xrouter_clients_usage::InMemoryUsageClient;
use xrouter_contracts::{
    ChatCompletionsRequest, ChatCompletionsResponse, ResponseEvent, ResponseOutputItem,
    ResponsesRequest, ResponsesResponse,
};
use xrouter_core::{
    CoreError, ExecutionEngine, ModelDescriptor, ProviderClient, default_model_catalog,
    synthesize_model_id,
};

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
    tokenizer: String,
    instruct_type: String,
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
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct ModelPricing {
    prompt: String,
    completion: String,
    request: String,
    image: String,
    web_search: String,
    internal_reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct XrouterModelEntry {
    id: String,
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pricing: Option<ModelPricing>,
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
            ModelPricing,
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

#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    #[serde(default)]
    data: Vec<OpenRouterModelData>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelData {
    id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    context_length: u32,
    #[serde(default)]
    architecture: OpenRouterArchitecture,
    #[serde(default)]
    top_provider: OpenRouterTopProvider,
}

#[derive(Debug, Deserialize)]
struct OpenRouterArchitecture {
    #[serde(default = "default_modality")]
    modality: String,
    #[serde(default)]
    tokenizer: Option<String>,
    #[serde(default)]
    instruct_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenRouterTopProvider {
    context_length: Option<u32>,
    max_completion_tokens: Option<u32>,
    is_moderated: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProviderModelsResponse {
    #[serde(default)]
    data: Vec<ProviderModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ProviderModelEntry {
    id: String,
}

fn default_modality() -> String {
    "text->text".to_string()
}

impl Default for OpenRouterArchitecture {
    fn default() -> Self {
        Self { modality: default_modality(), tokenizer: None, instruct_type: None }
    }
}

fn fetch_openrouter_models(
    provider_config: &config::ProviderConfig,
    supported_ids: &[String],
    connect_timeout_seconds: u64,
) -> Option<Vec<ModelDescriptor>> {
    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("https://openrouter.ai/api/v1")
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }

    let url = format!("{base_url}/models");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_timeout_seconds))
        .build();
    let mut request = agent.get(url.as_str()).set("Accept", "application/json");
    if let Some(api_key) = provider_config.api_key.as_deref() {
        request = request.set("Authorization", &format!("Bearer {api_key}"));
    }

    let response = request.call();
    let payload = match response {
        Ok(ok) => match ok.into_json::<OpenRouterModelsResponse>() {
            Ok(payload) => payload,
            Err(err) => {
                warn!(
                    event = "openrouter.models.fetch.failed",
                    reason = "invalid_json",
                    error = %err
                );
                return None;
            }
        },
        Err(err) => {
            warn!(
                event = "openrouter.models.fetch.failed",
                reason = "request_failed",
                error = %err
            );
            return None;
        }
    };

    Some(map_openrouter_models(payload, supported_ids))
}

fn map_openrouter_models(
    payload: OpenRouterModelsResponse,
    supported_ids: &[String],
) -> Vec<ModelDescriptor> {
    let supported = supported_ids.iter().cloned().collect::<HashSet<_>>();
    payload
        .data
        .into_iter()
        .filter(|model| supported.contains(&model.id))
        .map(|model| {
            let context_length = if model.context_length > 0 { model.context_length } else { 4096 };
            let top_context_length = model.top_provider.context_length.unwrap_or(context_length);
            let max_completion_tokens = model.top_provider.max_completion_tokens.unwrap_or(4096);
            ModelDescriptor {
                id: model.id.clone(),
                provider: "openrouter".to_string(),
                description: if model.description.is_empty() {
                    format!("{} via OpenRouter", model.id)
                } else {
                    model.description
                },
                context_length,
                tokenizer: model.architecture.tokenizer.unwrap_or_else(|| {
                    if model.id.contains("anthropic/") {
                        "anthropic".to_string()
                    } else if model.id.contains("google/") {
                        "google".to_string()
                    } else {
                        "unknown".to_string()
                    }
                }),
                instruct_type: model
                    .architecture
                    .instruct_type
                    .unwrap_or_else(|| "none".to_string()),
                modality: model.architecture.modality,
                top_provider_context_length: top_context_length,
                is_moderated: model.top_provider.is_moderated.unwrap_or(true),
                max_completion_tokens,
            }
        })
        .collect::<Vec<_>>()
}

fn fallback_openrouter_models(model_ids: &[String]) -> Vec<ModelDescriptor> {
    model_ids
        .iter()
        .map(|id| ModelDescriptor {
            id: id.clone(),
            provider: "openrouter".to_string(),
            description: format!("{id} via OpenRouter"),
            context_length: 128_000,
            tokenizer: if id.contains("anthropic/") {
                "anthropic".to_string()
            } else if id.contains("google/") {
                "google".to_string()
            } else {
                "unknown".to_string()
            },
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128_000,
            is_moderated: true,
            max_completion_tokens: 16_384,
        })
        .collect()
}

fn fetch_provider_model_ids(
    provider_name: &str,
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
) -> Option<Vec<String>> {
    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())?
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }

    let url = format!("{base_url}/models");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_timeout_seconds))
        .build();
    let mut request = agent.get(url.as_str()).set("Accept", "application/json");
    if let Some(api_key) = provider_config.api_key.as_deref().filter(|v| !v.trim().is_empty()) {
        request = request.set("Authorization", &format!("Bearer {api_key}"));
    }
    if provider_name == "yandex"
        && let Some(project) = provider_config.project.as_deref().filter(|v| !v.trim().is_empty())
    {
        request = request.set("OpenAI-Project", project);
    }

    match request.call() {
        Ok(ok) => match ok.into_json::<ProviderModelsResponse>() {
            Ok(payload) => Some(
                payload
                    .data
                    .into_iter()
                    .map(|entry| entry.id)
                    .filter(|id| !id.trim().is_empty())
                    .collect(),
            ),
            Err(err) => {
                warn!(
                    event = "provider.models.fetch.failed",
                    provider = %provider_name,
                    reason = "invalid_json",
                    error = %err
                );
                None
            }
        },
        Err(err) => {
            warn!(
                event = "provider.models.fetch.failed",
                provider = %provider_name,
                reason = "request_failed",
                error = %err
            );
            None
        }
    }
}

fn build_models_from_registry(
    provider: &str,
    provider_model_ids: &[String],
    registry_seed: &[ModelDescriptor],
) -> Vec<ModelDescriptor> {
    let registry = registry_seed
        .iter()
        .filter(|model| model.provider == provider)
        .map(|model| (model.id.clone(), model.clone()))
        .collect::<HashMap<_, _>>();

    provider_model_ids
        .iter()
        .map(|id| {
            if let Some(template) = registry.get(id) {
                let mut model = template.clone();
                model.id = id.clone();
                model
            } else if provider == "zai" {
                zai_fallback_model_descriptor(id)
            } else if provider == "yandex" {
                yandex_fallback_model_descriptor(id)
            } else {
                ModelDescriptor {
                    id: id.clone(),
                    provider: provider.to_string(),
                    description: format!("{id} via {provider}"),
                    context_length: 128_000,
                    tokenizer: "unknown".to_string(),
                    instruct_type: "none".to_string(),
                    modality: "text->text".to_string(),
                    top_provider_context_length: 128_000,
                    is_moderated: true,
                    max_completion_tokens: 8_192,
                }
            }
        })
        .collect()
}

fn zai_fallback_model_descriptor(id: &str) -> ModelDescriptor {
    let (context_length, max_completion_tokens, description) = match id {
        "glm-4.5" => (
            128_000,
            98_304,
            "GLM-4.5 is Z.AI's flagship general model focused on strong coding, reasoning, and long-context agent workflows.".to_string(),
        ),
        "glm-4.5-air" => (
            128_000,
            98_304,
            "GLM-4.5-Air is a lighter GLM-4.5 variant aimed at lower-latency interactive and agent tasks.".to_string(),
        ),
        "glm-4.6" => (
            200_000,
            128_000,
            "GLM-4.6 extends GLM with larger context and output budgets for long-horizon reasoning and implementation tasks.".to_string(),
        ),
        "glm-4.7" => (
            200_000,
            128_000,
            "GLM-4.7 improves stability for multi-step execution, coding, and structured planning over prior GLM generations.".to_string(),
        ),
        "glm-5" => (
            200_000,
            128_000,
            "GLM-5 is Z.AI's latest high-capacity model for complex systems design, agent orchestration, and long-context coding work.".to_string(),
        ),
        _ => (128_000, 8_192, format!("{id} via zai")),
    };

    ModelDescriptor {
        id: id.to_string(),
        provider: "zai".to_string(),
        description,
        context_length,
        tokenizer: "unknown".to_string(),
        instruct_type: "none".to_string(),
        modality: "text->text".to_string(),
        top_provider_context_length: context_length,
        is_moderated: true,
        max_completion_tokens,
    }
}

fn yandex_fallback_model_descriptor(id: &str) -> ModelDescriptor {
    ModelDescriptor {
        id: id.to_string(),
        provider: "yandex".to_string(),
        description: format!("{id} via yandex"),
        context_length: 32_768,
        tokenizer: "unknown".to_string(),
        instruct_type: "none".to_string(),
        modality: "text->text".to_string(),
        top_provider_context_length: 32_768,
        is_moderated: true,
        max_completion_tokens: 8_192,
    }
}

#[derive(Clone)]
pub struct AppState {
    openai_compatible_api: bool,
    billing_enabled: bool,
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
        let shared_http_client =
            if cfg!(test) { None } else { build_http_client(config.provider_timeout_seconds) };
        for (provider, provider_config) in &config.providers {
            if !provider_config.enabled {
                continue;
            }
            let client: Arc<dyn ProviderClient> = if cfg!(test) {
                Arc::new(MockProviderClient::new(provider.to_string()))
            } else {
                match provider.as_str() {
                    "openrouter" => Arc::new(OpenRouterClient::new(
                        provider_config.base_url.clone(),
                        provider_config.api_key.clone(),
                        shared_http_client.clone(),
                        Some(config.provider_max_inflight),
                    )),
                    "deepseek" => Arc::new(DeepSeekClient::new(
                        provider_config.base_url.clone(),
                        provider_config.api_key.clone(),
                        shared_http_client.clone(),
                        Some(config.provider_max_inflight),
                    )),
                    "zai" => Arc::new(ZaiClient::new(
                        provider_config.base_url.clone(),
                        provider_config.api_key.clone(),
                        shared_http_client.clone(),
                        Some(config.provider_max_inflight),
                    )),
                    "yandex" => Arc::new(YandexResponsesClient::new(
                        provider_config.base_url.clone(),
                        provider_config.api_key.clone(),
                        provider_config.project.clone(),
                        shared_http_client.clone(),
                        Some(config.provider_max_inflight),
                    )),
                    "gigachat" => Arc::new(GigachatClient::new(
                        provider_config.base_url.clone(),
                        provider_config.api_key.clone(),
                        None,
                        shared_http_client.clone(),
                        Some(config.provider_max_inflight),
                    )),
                    _ => Arc::new(OpenAiClient::new(
                        provider.to_string(),
                        provider_config.base_url.clone(),
                        provider_config.api_key.clone(),
                        shared_http_client.clone(),
                        Some(config.provider_max_inflight),
                    )),
                }
            };
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

        let default_catalog = default_model_catalog();
        let mut models = default_catalog
            .clone()
            .into_iter()
            .filter(|entry| {
                enabled_providers.contains(&entry.provider)
                    && entry.provider != "openrouter"
                    && entry.provider != "zai"
                    && entry.provider != "yandex"
            })
            .collect::<Vec<_>>();

        if enabled_providers.contains("openrouter")
            && let Some(openrouter_config) = config.providers.get("openrouter")
        {
            if cfg!(test) {
                models.extend(fallback_openrouter_models(&config.openrouter_supported_models));
            } else if let Some(fetched) = fetch_openrouter_models(
                openrouter_config,
                &config.openrouter_supported_models,
                config.provider_timeout_seconds,
            ) {
                info!(
                    event = "openrouter.models.loaded",
                    source = "remote",
                    model_count = fetched.len()
                );
                models.extend(fetched);
            } else {
                warn!(
                    event = "openrouter.models.loaded",
                    source = "fallback",
                    reason = "fetch_failed",
                    model_count = config.openrouter_supported_models.len()
                );
                models.extend(fallback_openrouter_models(&config.openrouter_supported_models));
            }
        }

        if enabled_providers.contains("zai")
            && let Some(zai_config) = config.providers.get("zai")
        {
            if cfg!(test) {
                models.extend(
                    default_catalog
                        .iter()
                        .filter(|model| model.provider == "zai")
                        .cloned()
                        .collect::<Vec<_>>(),
                );
            } else if let Some(zai_model_ids) =
                fetch_provider_model_ids("zai", zai_config, config.provider_timeout_seconds)
            {
                let zai_models =
                    build_models_from_registry("zai", &zai_model_ids, &default_catalog);
                info!(
                    event = "zai.models.loaded",
                    source = "remote",
                    model_count = zai_models.len()
                );
                models.extend(zai_models);
            } else {
                warn!(event = "zai.models.loaded", source = "fallback", reason = "fetch_failed");
                models.extend(
                    default_catalog
                        .iter()
                        .filter(|model| model.provider == "zai")
                        .cloned()
                        .collect::<Vec<_>>(),
                );
            }
        }

        if enabled_providers.contains("yandex")
            && let Some(yandex_config) = config.providers.get("yandex")
        {
            if cfg!(test) {
                models.extend(
                    default_catalog
                        .iter()
                        .filter(|model| model.provider == "yandex")
                        .cloned()
                        .collect::<Vec<_>>(),
                );
            } else if let Some(yandex_model_ids) =
                fetch_provider_model_ids("yandex", yandex_config, config.provider_timeout_seconds)
            {
                let yandex_models =
                    build_models_from_registry("yandex", &yandex_model_ids, &default_catalog);
                info!(
                    event = "yandex.models.loaded",
                    source = "remote",
                    model_count = yandex_models.len()
                );
                models.extend(yandex_models);
            } else {
                warn!(event = "yandex.models.loaded", source = "fallback", reason = "fetch_failed");
                models.extend(
                    default_catalog
                        .iter()
                        .filter(|model| model.provider == "yandex")
                        .cloned()
                        .collect::<Vec<_>>(),
                );
            }
        }
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
            billing_enabled: config.billing_enabled,
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
        if let Some(found) =
            self.models.iter().find(|m| synthesize_model_id(&m.provider, &m.id) == model)
        {
            return found.provider.clone();
        }

        self.default_provider.clone()
    }

    fn resolve_provider_model_id(&self, model: &str) -> String {
        if let Some((provider, provider_model)) = model.split_once('/')
            && self.engines.contains_key(provider)
        {
            return provider_model.to_string();
        }
        model.to_string()
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
            id: synthesize_model_id(&m.provider, &m.id),
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
            id: synthesize_model_id(&m.provider, &m.id),
            name: synthesize_model_id(&m.provider, &m.id),
            description: m.description.clone(),
            pricing: state.billing_enabled.then(|| ModelPricing {
                prompt: "0".to_string(),
                completion: "0".to_string(),
                request: "0".to_string(),
                image: "0".to_string(),
                web_search: "0".to_string(),
                internal_reasoning: "0".to_string(),
            }),
            context_length: m.context_length,
            architecture: ModelArchitecture {
                tokenizer: m.tokenizer.clone(),
                instruct_type: m.instruct_type.clone(),
                modality: m.modality.clone(),
            },
            top_provider: ModelTopProvider {
                context_length: m.top_provider_context_length,
                max_completion_tokens: m.max_completion_tokens,
                is_moderated: m.is_moderated,
            },
            per_request_limits: ModelPerRequestLimits {
                prompt_tokens: None,
                completion_tokens: Some(m.max_completion_tokens),
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
    Json(mut request): Json<ResponsesRequest>,
) -> Response {
    let started_at = Instant::now();
    let request_model = request.model.clone();
    let provider = state.resolve_provider_key(&request.model);
    let provider_model = state.resolve_provider_model_id(&request.model);
    let public_model_id = synthesize_model_id(&provider, &provider_model);
    request.model = provider_model;
    info!(
        event = "http.request.received",
        route = "/api/v1/responses",
        model = %public_model_id,
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
        let response_id = new_prefixed_id("resp_");
        info!(
            event = "http.stream.started",
            route = "/api/v1/responses",
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
    let mut core_request = request.clone().into_responses_request();
    let request_model = core_request.model.clone();
    let provider = state.resolve_provider_key(&core_request.model);
    let provider_model = state.resolve_provider_model_id(&core_request.model);
    let public_model_id = synthesize_model_id(&provider, &provider_model);
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
    let status = match &err {
        CoreError::Provider(message) if is_provider_overloaded(message) => {
            axum::http::StatusCode::TOO_MANY_REQUESTS
        }
        _ => axum::http::StatusCode::BAD_REQUEST,
    };
    match &err {
        CoreError::Validation(_) | CoreError::Provider(_) => {
            warn!(event = "http.error_response", error = %err);
        }
        CoreError::Billing(_) | CoreError::ClientDisconnected(_) => {
            error!(event = "http.error_response", error = %err);
        }
    }
    (status, Json(ErrorResponse { error: err.to_string() })).into_response()
}

fn is_provider_overloaded(message: &str) -> bool {
    message.starts_with("provider overloaded:")
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

    #[test]
    fn map_openrouter_models_uses_provider_payload_fields() {
        let payload: OpenRouterModelsResponse = serde_json::from_value(json!({
            "data": [{
                "id": "openai/gpt-5.2",
                "description": "OpenAI GPT-5.2 via OpenRouter",
                "context_length": 222000,
                "architecture": {"modality": "text->text"},
                "top_provider": {
                    "context_length": 210000,
                    "max_completion_tokens": 12345,
                    "is_moderated": false
                }
            }, {
                "id": "ignore/me",
                "description": "ignored",
                "context_length": 1
            }]
        }))
        .expect("payload must deserialize");

        let models = map_openrouter_models(payload, &["openai/gpt-5.2".to_string()]);
        assert_eq!(models.len(), 1);
        let model = &models[0];
        assert_eq!(model.id, "openai/gpt-5.2");
        assert_eq!(model.description, "OpenAI GPT-5.2 via OpenRouter");
        assert_eq!(model.context_length, 222000);
        assert_eq!(model.top_provider_context_length, 210000);
        assert_eq!(model.max_completion_tokens, 12345);
        assert!(!model.is_moderated);
        assert_eq!(model.modality, "text->text");
        assert_eq!(model.tokenizer, "unknown");
        assert_eq!(model.instruct_type, "none");
    }

    #[test]
    fn fetch_openrouter_models_returns_none_when_request_fails() {
        let provider = crate::config::ProviderConfig {
            enabled: true,
            api_key: None,
            base_url: Some("http://127.0.0.1:0".to_string()),
            project: None,
        };
        let models = fetch_openrouter_models(&provider, &["openai/gpt-5.2".to_string()], 1);
        assert!(models.is_none());
    }

    #[test]
    fn build_models_from_registry_uses_seed_and_fallback_for_unknown_ids() {
        let seed = default_model_catalog();
        let ids = vec!["glm-4.5".to_string(), "glm-4.6".to_string(), "glm-5".to_string()];
        let models = build_models_from_registry("zai", &ids, &seed);
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "glm-4.5");
        assert_eq!(models[0].provider, "zai");
        assert_eq!(models[1].id, "glm-4.6");
        assert_eq!(models[1].provider, "zai");
        assert_eq!(models[1].max_completion_tokens, 128_000);
        assert_eq!(models[1].context_length, 200_000);
        assert_eq!(models[2].id, "glm-5");
        assert_eq!(models[2].max_completion_tokens, 128_000);
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
json.data_len=51
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
json.data_len=51
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
    async fn models_response_omits_pricing_when_billing_disabled() {
        let app = build_router(test_app_state(false));
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
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let first = payload["data"][0].as_object().expect("first model must be object");
        assert!(!first.contains_key("pricing"));
    }

    #[tokio::test]
    async fn models_response_includes_pricing_when_billing_enabled() {
        let mut config = crate::config::AppConfig::for_tests();
        config.billing_enabled = true;
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
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body read must succeed");
        let payload: Value =
            serde_json::from_slice(&body).expect("response body must be valid json");
        let pricing = &payload["data"][0]["pricing"];
        assert_eq!(pricing["prompt"], "0");
        assert_eq!(pricing["completion"], "0");
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

    #[test]
    fn error_response_returns_429_for_provider_overload() {
        let response = error_response(CoreError::Provider(
            "provider overloaded: max in-flight limit reached for deepseek".to_string(),
        ));
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn error_response_keeps_400_for_regular_provider_error() {
        let response =
            error_response(CoreError::Provider("provider request failed: timeout".to_string()));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
