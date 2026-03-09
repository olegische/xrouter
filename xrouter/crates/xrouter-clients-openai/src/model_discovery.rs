#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpJsonRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpFormRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub form_fields: Vec<(String, String)>,
}

const DEFAULT_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const GIGACHAT_OAUTH_URL: &str = "https://ngw.devices.sberbank.ru:9443/api/v2/oauth";

pub fn build_openrouter_models_request(
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Option<HttpJsonRequest> {
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_OPENROUTER_BASE_URL)
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }

    let mut headers = vec![("Accept".to_string(), "application/json".to_string())];
    if let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) {
        headers.push(("Authorization".to_string(), format!("Bearer {api_key}")));
    }

    Some(HttpJsonRequest { url: format!("{base_url}/models"), headers })
}

pub fn build_provider_models_request(
    provider_name: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
    project: Option<&str>,
) -> Option<HttpJsonRequest> {
    let base_url = base_url?.trim();
    if base_url.is_empty() {
        return None;
    }
    let base_url = base_url.trim_end_matches('/').to_string();

    let mut headers = vec![("Accept".to_string(), "application/json".to_string())];
    if let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) {
        headers.push(("Authorization".to_string(), format!("Bearer {api_key}")));
    }
    if provider_name == "yandex"
        && let Some(project) = project.filter(|value| !value.trim().is_empty())
    {
        headers.push(("OpenAI-Project".to_string(), project.to_string()));
    }

    Some(HttpJsonRequest { url: format!("{base_url}/models"), headers })
}

pub fn build_xrouter_models_request(
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Option<HttpJsonRequest> {
    let base_url = base_url?.trim();
    if base_url.is_empty() {
        return None;
    }
    let base_url = base_url.trim_end_matches('/').to_string();

    let mut headers = vec![("Accept".to_string(), "application/json".to_string())];
    if let Some(api_key) = api_key.filter(|value| !value.trim().is_empty()) {
        headers.push(("Authorization".to_string(), format!("Bearer {api_key}")));
    }

    Some(HttpJsonRequest { url: format!("{base_url}/models"), headers })
}

pub fn build_gigachat_oauth_request(
    api_key: &str,
    request_id: &str,
    scope: &str,
) -> HttpFormRequest {
    HttpFormRequest {
        url: GIGACHAT_OAUTH_URL.to_string(),
        headers: vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("Authorization".to_string(), format!("Bearer {api_key}")),
            ("RqUID".to_string(), request_id.to_string()),
            ("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()),
        ],
        form_fields: vec![("scope".to_string(), scope.to_string())],
    }
}

pub fn build_gigachat_models_request(
    base_url: Option<&str>,
    access_token: &str,
) -> Option<HttpJsonRequest> {
    let base_url = base_url?.trim();
    if base_url.is_empty() {
        return None;
    }
    let base_url = base_url.trim_end_matches('/').to_string();

    Some(HttpJsonRequest {
        url: format!("{base_url}/models"),
        headers: vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("Authorization".to_string(), format!("Bearer {access_token}")),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::{
        build_gigachat_models_request, build_gigachat_oauth_request,
        build_openrouter_models_request, build_provider_models_request,
        build_xrouter_models_request,
    };

    #[test]
    fn openrouter_models_request_uses_default_base_url() {
        let request = build_openrouter_models_request(None, Some("sk-test")).expect("request");
        assert_eq!(request.url, "https://openrouter.ai/api/v1/models");
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| { name == "Authorization" && value == "Bearer sk-test" })
        );
    }

    #[test]
    fn provider_models_request_adds_yandex_project_header() {
        let request = build_provider_models_request(
            "yandex",
            Some("https://llm.api.cloud.yandex.net/foundationModels/v1"),
            Some("secret"),
            Some("project-123"),
        )
        .expect("request");
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| { name == "OpenAI-Project" && value == "project-123" })
        );
    }

    #[test]
    fn xrouter_models_request_requires_base_url() {
        assert!(build_xrouter_models_request(None, Some("secret")).is_none());
    }

    #[test]
    fn gigachat_oauth_request_contains_expected_form_fields() {
        let request = build_gigachat_oauth_request("auth-key", "req-1", "scope-1");
        assert_eq!(request.url, "https://ngw.devices.sberbank.ru:9443/api/v2/oauth");
        assert_eq!(request.form_fields, vec![("scope".to_string(), "scope-1".to_string())]);
    }

    #[test]
    fn gigachat_models_request_uses_bearer_token() {
        let request = build_gigachat_models_request(
            Some("https://gigachat.devices.sberbank.ru/api/v1"),
            "token",
        )
        .expect("request");
        assert_eq!(request.url, "https://gigachat.devices.sberbank.ru/api/v1/models");
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| { name == "Authorization" && value == "Bearer token" })
        );
    }
}
