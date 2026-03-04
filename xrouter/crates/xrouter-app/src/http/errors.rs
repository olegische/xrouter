use axum::{
    Json,
    response::{IntoResponse, Response},
};
use tracing::{error, warn};
use xrouter_core::CoreError;

use crate::ErrorResponse;

pub(crate) fn error_response(err: CoreError) -> Response {
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
        CoreError::ClientDisconnected(_) => {
            error!(event = "http.error_response", error = %err);
        }
    }
    (status, Json(ErrorResponse { error: err.to_string() })).into_response()
}

fn is_provider_overloaded(message: &str) -> bool {
    message.starts_with("provider overloaded:")
}
