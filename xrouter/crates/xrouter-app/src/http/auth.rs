use axum::http::HeaderMap;
use tracing::info;
use xrouter_core::CoreError;

pub(crate) fn resolve_byok_bearer(
    headers: &HeaderMap,
    byok_enabled: bool,
    provider: &str,
    route: &str,
) -> Result<Option<String>, CoreError> {
    if !byok_enabled {
        return Ok(None);
    }

    if provider == "yandex" {
        info!(
            event = "http.byok.rejected",
            route = route,
            provider = provider,
            reason = "provider_not_supported"
        );
        return Err(CoreError::Validation("BYOK is not supported for yandex provider".to_string()));
    }

    parse_bearer_token(headers).map(Some).ok_or_else(|| {
        CoreError::Validation(
            "authorization bearer token is required when XR_BYOK_ENABLED=true".to_string(),
        )
    })
}

pub(crate) fn parse_bearer_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = raw.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    if token.is_empty() { None } else { Some(token.to_string()) }
}
