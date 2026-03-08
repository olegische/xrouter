use std::{collections::HashMap, sync::Arc};

use xrouter_core::{CoreError, ExecutionEngine, ModelDescriptor, synthesize_model_id};

use crate::{config, startup::app_builder::AppBuilder};

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
        AppBuilder::new(config).build_state()
    }

    pub fn new() -> Self {
        Self::from_config(&config::AppConfig::for_tests())
    }

    pub(crate) fn from_parts(
        openai_compatible_api: bool,
        byok_enabled: bool,
        models: Vec<ModelDescriptor>,
        engines: HashMap<String, Arc<ExecutionEngine>>,
    ) -> Self {
        let default_provider = if models.iter().any(|entry| entry.provider == "openrouter") {
            "openrouter".to_string()
        } else {
            models
                .first()
                .map(|entry| entry.provider.clone())
                .unwrap_or_else(|| "openrouter".to_string())
        };

        Self { openai_compatible_api, byok_enabled, default_provider, models, engines }
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
