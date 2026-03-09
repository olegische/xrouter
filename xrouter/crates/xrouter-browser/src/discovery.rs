use serde::de::DeserializeOwned;
use xrouter_clients_openai::model_discovery::{
    HttpJsonRequest, build_openrouter_models_request, build_provider_models_request,
    build_xrouter_models_request,
};
use xrouter_clients_openai::models::{
    OpenRouterModelsResponse, ProviderModelsResponse, XrouterProviderModelsResponse,
    extract_provider_model_ids, map_openrouter_models, map_xrouter_models,
};
use xrouter_core::ModelDescriptor;

use crate::error::BrowserError;

#[derive(Debug, Default, Clone, Copy)]
pub struct BrowserModelDiscoveryClient;

impl BrowserModelDiscoveryClient {
    pub const fn new() -> Self {
        Self
    }

    pub async fn fetch_openrouter_models(
        &self,
        base_url: Option<&str>,
        api_key: Option<&str>,
        supported_ids: &[String],
    ) -> Result<Vec<ModelDescriptor>, BrowserError> {
        let request = build_openrouter_models_request(base_url, api_key)
            .ok_or(BrowserError::InvalidRequest("openrouter models request"))?;
        let payload = fetch_json::<OpenRouterModelsResponse>(&request).await?;
        Ok(map_openrouter_models(payload, supported_ids))
    }

    pub async fn fetch_provider_model_ids(
        &self,
        provider_name: &str,
        base_url: Option<&str>,
        api_key: Option<&str>,
        project: Option<&str>,
    ) -> Result<Vec<String>, BrowserError> {
        let request = build_provider_models_request(provider_name, base_url, api_key, project)
            .ok_or(BrowserError::InvalidRequest("provider models request"))?;
        let payload = fetch_json::<ProviderModelsResponse>(&request).await?;
        Ok(extract_provider_model_ids(payload))
    }

    pub async fn fetch_xrouter_models(
        &self,
        base_url: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<Vec<ModelDescriptor>, BrowserError> {
        let request = build_xrouter_models_request(base_url, api_key)
            .ok_or(BrowserError::InvalidRequest("xrouter models request"))?;
        let payload = fetch_json::<XrouterProviderModelsResponse>(&request).await?;
        Ok(map_xrouter_models(payload))
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_json<T: DeserializeOwned>(request: &HttpJsonRequest) -> Result<T, BrowserError> {
    let response = crate::runtime::fetch_get_text(request).await?;
    parse_json_body(&response.body)
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_json<T: DeserializeOwned>(_request: &HttpJsonRequest) -> Result<T, BrowserError> {
    Err(BrowserError::UnsupportedPlatform)
}

#[cfg(any(target_arch = "wasm32", test))]
fn parse_json_body<T: DeserializeOwned>(body: &str) -> Result<T, BrowserError> {
    serde_json::from_str::<T>(body).map_err(|err| BrowserError::Parse(err.to_string()))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{BrowserError, BrowserModelDiscoveryClient};

    #[test]
    fn browser_client_reports_unsupported_platform_on_native() {
        let client = BrowserModelDiscoveryClient::new();
        let result = futures::executor::block_on(client.fetch_provider_model_ids(
            "deepseek",
            Some("https://api.deepseek.com"),
            None,
            None,
        ));
        assert!(matches!(result, Err(BrowserError::UnsupportedPlatform)));
    }

    #[test]
    fn parse_json_body_reports_invalid_payload() {
        let result = super::parse_json_body::<Value>("not-json");
        assert!(matches!(result, Err(BrowserError::Parse(_))));
    }
}
