use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use tracing::{debug, info, warn};
use xrouter_clients_openai::{
    DeepSeekClient, GigachatClient, MockProviderClient, OpenAiClient, OpenRouterClient,
    XrouterClient, YandexResponsesClient, ZaiClient, build_http_client,
    build_http_client_insecure_tls,
};
use xrouter_core::{
    CoreError, ExecutionEngine, ModelDescriptor, ProviderClient, default_model_catalog,
    synthesize_model_id,
};

use crate::{
    build_models_from_registry, config, fallback_openrouter_models, fetch_openrouter_models,
    fetch_provider_model_ids, fetch_xrouter_models,
};

#[derive(Clone)]
pub struct AppState {
    pub(crate) openai_compatible_api: bool,
    pub(crate) byok_enabled: bool,
    pub(crate) default_provider: String,
    pub(crate) models: Vec<ModelDescriptor>,
    pub(crate) engines: HashMap<String, Arc<ExecutionEngine>>,
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
            byok_enabled = config.byok_enabled,
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
            let engine = Arc::new(ExecutionEngine::new(client));
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
                    && entry.provider != "gigachat"
                    && entry.provider != "xrouter"
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
            } else if let Some(zai_model_ids) = fetch_provider_model_ids(
                "zai",
                zai_config,
                config.provider_timeout_seconds,
                config.gigachat_insecure_tls,
            ) {
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
            } else if let Some(yandex_model_ids) = fetch_provider_model_ids(
                "yandex",
                yandex_config,
                config.provider_timeout_seconds,
                config.gigachat_insecure_tls,
            ) {
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

        if enabled_providers.contains("gigachat")
            && let Some(gigachat_config) = config.providers.get("gigachat")
        {
            if cfg!(test) {
                models.extend(
                    default_catalog
                        .iter()
                        .filter(|model| model.provider == "gigachat")
                        .cloned()
                        .collect::<Vec<_>>(),
                );
            } else if let Some(gigachat_model_ids) = fetch_provider_model_ids(
                "gigachat",
                gigachat_config,
                config.provider_timeout_seconds,
                config.gigachat_insecure_tls,
            ) {
                let supported = config
                    .gigachat_supported_models
                    .iter()
                    .map(|id| id.strip_prefix("gigachat/").unwrap_or(id.as_str()))
                    .collect::<HashSet<_>>();
                let filtered_ids = gigachat_model_ids
                    .into_iter()
                    .filter(|id| supported.contains(id.as_str()))
                    .collect::<Vec<_>>();
                let gigachat_models =
                    build_models_from_registry("gigachat", &filtered_ids, &default_catalog);
                info!(
                    event = "gigachat.models.loaded",
                    source = "remote",
                    model_count = gigachat_models.len(),
                    configured_count = config.gigachat_supported_models.len()
                );
                models.extend(gigachat_models);
            } else {
                warn!(
                    event = "gigachat.models.loaded",
                    source = "none",
                    reason = "fetch_failed_no_fallback"
                );
            }
        }

        if enabled_providers.contains("xrouter")
            && let Some(xrouter_config) = config.providers.get("xrouter")
        {
            if cfg!(test) {
                models.extend(
                    default_catalog
                        .iter()
                        .filter(|model| model.provider == "xrouter")
                        .cloned()
                        .collect::<Vec<_>>(),
                );
            } else if let Some(xrouter_models) =
                fetch_xrouter_models(xrouter_config, config.provider_timeout_seconds)
            {
                info!(
                    event = "xrouter.models.loaded",
                    source = "remote",
                    model_count = xrouter_models.len()
                );
                models.extend(xrouter_models);
            } else {
                warn!(
                    event = "xrouter.models.loaded",
                    source = "fallback",
                    reason = "fetch_failed"
                );
                models.extend(
                    default_catalog
                        .iter()
                        .filter(|model| model.provider == "xrouter")
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
            byok_enabled: config.byok_enabled,
            default_provider,
            models,
            engines,
        }
    }

    pub fn new() -> Self {
        Self::from_config(&config::AppConfig::for_tests())
    }

    pub(crate) fn resolve_provider_key(&self, model: &str) -> String {
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

    pub(crate) fn resolve_provider_model_id(&self, model: &str) -> String {
        if let Some((provider, provider_model)) = model.split_once('/')
            && self.engines.contains_key(provider)
        {
            return provider_model.to_string();
        }
        model.to_string()
    }

    pub(crate) fn resolve_engine(&self, model: &str) -> Result<Arc<ExecutionEngine>, CoreError> {
        let key = self.resolve_provider_key(model);
        self.engines.get(&key).cloned().ok_or_else(|| {
            CoreError::Validation(format!("unsupported provider for model: {model}"))
        })
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
