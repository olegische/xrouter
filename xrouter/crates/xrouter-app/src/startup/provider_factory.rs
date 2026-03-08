use std::{collections::HashMap, sync::Arc};

use tracing::{debug, info};
use xrouter_clients_openai::{
    DeepSeekClient, GigachatClient, MockProviderClient, OpenAiClient, OpenRouterClient,
    XrouterClient, YandexResponsesClient, ZaiClient, build_http_client,
    build_http_client_insecure_tls,
};
use xrouter_core::{ExecutionEngine, ProviderClient};

use crate::config;

pub(crate) fn build_engines(config: &config::AppConfig) -> HashMap<String, Arc<ExecutionEngine>> {
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
                    if config.gigachat_insecure_tls {
                        build_http_client_insecure_tls(config.provider_timeout_seconds)
                    } else {
                        shared_http_client.clone()
                    },
                    Some(config.provider_max_inflight),
                )),
                "xrouter" => Arc::new(XrouterClient::new(
                    provider_config.base_url.clone(),
                    provider_config.api_key.clone(),
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

        engines.insert(provider.to_string(), Arc::new(ExecutionEngine::new(client)));
    }

    info!(event = "app.engines.initialized", engine_count = engines.len());
    debug!(
        event = "app.engines.providers",
        providers = ?engines.keys().collect::<Vec<_>>()
    );
    engines
}
