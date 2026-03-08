use axum::{Json, extract::State};
use tracing::{debug, info};
use xrouter_core::synthesize_model_id;

use crate::{
    AppState,
    http::docs::{
        CompatibleModelEntry, CompatibleModelsResponse, HealthResponse, ModelArchitecture,
        ModelPerRequestLimits, ModelTopProvider, XrouterModelEntry, XrouterModelsResponse,
    },
};

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Service health", body = HealthResponse)),
    tag = "xrouter-app"
)]
pub(crate) async fn get_health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "healthy".to_string() })
}

#[utoipa::path(
    get,
    path = "/v1/models",
    responses((status = 200, description = "OpenAI-compatible model list", body = CompatibleModelsResponse)),
    tag = "xrouter-app"
)]
pub(crate) async fn get_compatible_models(
    State(state): State<AppState>,
) -> Json<CompatibleModelsResponse> {
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
pub(crate) async fn get_xrouter_models(
    State(state): State<AppState>,
) -> Json<XrouterModelsResponse> {
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
