use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use tracing::{debug, info};
use xrouter_core::{CoreError, ExecutionEngine, ModelDescriptor, synthesize_model_id};

use crate::{
    config,
    startup::{model_catalog::load_models, provider_factory::build_engines},
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

        let engines = build_engines(config);
        let models = load_models(config, &enabled_providers);

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
