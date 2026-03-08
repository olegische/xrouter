use axum::{
    Router,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use xrouter_contracts::{
    ChatCompletionsRequest, ChatCompletionsResponse, ResponsesRequest, ResponsesResponse,
};

use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct HealthResponse {
    pub(crate) status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct CompatibleModelEntry {
    pub(crate) id: String,
    pub(crate) object: String,
    pub(crate) created: i64,
    pub(crate) owned_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct CompatibleModelsResponse {
    pub(crate) object: String,
    pub(crate) data: Vec<CompatibleModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct ModelArchitecture {
    pub(crate) tokenizer: String,
    pub(crate) instruct_type: String,
    pub(crate) modality: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct ModelTopProvider {
    pub(crate) context_length: u32,
    pub(crate) max_completion_tokens: u32,
    pub(crate) is_moderated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct ModelPerRequestLimits {
    pub(crate) prompt_tokens: Option<u32>,
    pub(crate) completion_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct XrouterModelEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) context_length: u32,
    pub(crate) architecture: ModelArchitecture,
    pub(crate) top_provider: ModelTopProvider,
    pub(crate) per_request_limits: ModelPerRequestLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct XrouterModelsResponse {
    pub(crate) data: Vec<XrouterModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub(crate) struct ErrorResponse {
    pub(crate) error: String,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::http::routes::basic::get_health,
        crate::http::routes::basic::get_xrouter_models,
        crate::http::routes::inference::post_responses,
        crate::http::routes::inference::post_chat_completions
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
        crate::http::routes::basic::get_health,
        crate::http::routes::basic::get_compatible_models,
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

pub fn build_router(state: AppState) -> Router {
    let openai_compatible_api = state.openai_compatible_api;
    let (router, openapi) = if openai_compatible_api {
        (
            Router::new()
                .route("/health", get(crate::http::routes::basic::get_health))
                .route("/v1/models", get(crate::http::routes::basic::get_compatible_models))
                .route("/v1/responses", post(crate::http::routes::inference::post_responses))
                .route(
                    "/v1/chat/completions",
                    post(crate::http::routes::inference::post_chat_completions),
                ),
            OpenAiApiDoc::openapi(),
        )
    } else {
        (
            Router::new()
                .route("/health", get(crate::http::routes::basic::get_health))
                .route("/api/v1/models", get(crate::http::routes::basic::get_xrouter_models))
                .route("/api/v1/responses", post(crate::http::routes::inference::post_responses))
                .route(
                    "/api/v1/chat/completions",
                    post(crate::http::routes::inference::post_chat_completions),
                ),
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
