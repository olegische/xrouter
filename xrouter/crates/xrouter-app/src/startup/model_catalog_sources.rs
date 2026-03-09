use std::collections::HashSet;

use tracing::{info, warn};
use xrouter_clients_openai::models::{build_models_from_registry, fallback_openrouter_models};
use xrouter_core::ModelDescriptor;

use crate::{
    config,
    startup::model_catalog_remote::{
        fetch_openrouter_models, fetch_provider_model_ids, fetch_xrouter_models,
    },
};

pub(crate) trait ModelCatalogSource {
    fn load_models(
        &self,
        context: &ModelCatalogContext<'_>,
        registry_seed: &[ModelDescriptor],
    ) -> Vec<ModelDescriptor>;
}

pub(crate) struct ModelCatalogContext<'a> {
    pub(crate) config: &'a config::AppConfig,
    pub(crate) enabled_providers: &'a HashSet<String>,
    pub(crate) test_mode: bool,
}

pub(crate) struct BaseCatalogSource;

impl ModelCatalogSource for BaseCatalogSource {
    fn load_models(
        &self,
        context: &ModelCatalogContext<'_>,
        registry_seed: &[ModelDescriptor],
    ) -> Vec<ModelDescriptor> {
        registry_seed
            .iter()
            .filter(|entry| {
                context.enabled_providers.contains(&entry.provider)
                    && entry.provider != "openrouter"
                    && entry.provider != "zai"
                    && entry.provider != "yandex"
                    && entry.provider != "gigachat"
                    && entry.provider != "xrouter"
            })
            .cloned()
            .collect()
    }
}

pub(crate) struct OpenRouterCatalogSource;

impl ModelCatalogSource for OpenRouterCatalogSource {
    fn load_models(
        &self,
        context: &ModelCatalogContext<'_>,
        _registry_seed: &[ModelDescriptor],
    ) -> Vec<ModelDescriptor> {
        if !context.enabled_providers.contains("openrouter") {
            return Vec::new();
        }
        let Some(openrouter_config) = context.config.providers.get("openrouter") else {
            return Vec::new();
        };

        if context.test_mode {
            return fallback_openrouter_models(&context.config.openrouter_supported_models);
        }

        if let Some(fetched) = fetch_openrouter_models(
            openrouter_config,
            &context.config.openrouter_supported_models,
            context.config.provider_timeout_seconds,
        ) {
            info!(
                event = "openrouter.models.loaded",
                source = "remote",
                model_count = fetched.len()
            );
            return fetched;
        }

        warn!(
            event = "openrouter.models.loaded",
            source = "fallback",
            reason = "fetch_failed",
            model_count = context.config.openrouter_supported_models.len()
        );
        fallback_openrouter_models(&context.config.openrouter_supported_models)
    }
}

pub(crate) struct RegistryBackedCatalogSource {
    provider: &'static str,
}

impl RegistryBackedCatalogSource {
    pub(crate) const fn new(provider: &'static str) -> Self {
        Self { provider }
    }
}

impl ModelCatalogSource for RegistryBackedCatalogSource {
    fn load_models(
        &self,
        context: &ModelCatalogContext<'_>,
        registry_seed: &[ModelDescriptor],
    ) -> Vec<ModelDescriptor> {
        if !context.enabled_providers.contains(self.provider) {
            return Vec::new();
        }
        let Some(provider_config) = context.config.providers.get(self.provider) else {
            return Vec::new();
        };

        if context.test_mode {
            return registry_seed
                .iter()
                .filter(|model| model.provider == self.provider)
                .cloned()
                .collect();
        }

        if let Some(model_ids) = fetch_provider_model_ids(
            self.provider,
            provider_config,
            context.config.provider_timeout_seconds,
            context.config.gigachat_insecure_tls,
        ) {
            let models = build_models_from_registry(self.provider, &model_ids, registry_seed);
            info!(
                event = "provider.models.loaded",
                provider = self.provider,
                source = "remote",
                model_count = models.len()
            );
            return models;
        }

        warn!(
            event = "provider.models.loaded",
            provider = self.provider,
            source = "fallback",
            reason = "fetch_failed"
        );
        registry_seed.iter().filter(|model| model.provider == self.provider).cloned().collect()
    }
}

pub(crate) struct GigachatCatalogSource;

impl ModelCatalogSource for GigachatCatalogSource {
    fn load_models(
        &self,
        context: &ModelCatalogContext<'_>,
        registry_seed: &[ModelDescriptor],
    ) -> Vec<ModelDescriptor> {
        if !context.enabled_providers.contains("gigachat") {
            return Vec::new();
        }
        let Some(gigachat_config) = context.config.providers.get("gigachat") else {
            return Vec::new();
        };

        if context.test_mode {
            return registry_seed
                .iter()
                .filter(|model| model.provider == "gigachat")
                .cloned()
                .collect();
        }

        if let Some(gigachat_model_ids) = fetch_provider_model_ids(
            "gigachat",
            gigachat_config,
            context.config.provider_timeout_seconds,
            context.config.gigachat_insecure_tls,
        ) {
            let supported = context
                .config
                .gigachat_supported_models
                .iter()
                .map(|id| id.strip_prefix("gigachat/").unwrap_or(id.as_str()))
                .collect::<HashSet<_>>();
            let filtered_ids = gigachat_model_ids
                .into_iter()
                .filter(|id| supported.contains(id.as_str()))
                .collect::<Vec<_>>();
            let models = build_models_from_registry("gigachat", &filtered_ids, registry_seed);
            info!(
                event = "gigachat.models.loaded",
                source = "remote",
                model_count = models.len(),
                configured_count = context.config.gigachat_supported_models.len()
            );
            return models;
        }

        warn!(
            event = "gigachat.models.loaded",
            source = "none",
            reason = "fetch_failed_no_fallback"
        );
        Vec::new()
    }
}

pub(crate) struct XrouterCatalogSource;

impl ModelCatalogSource for XrouterCatalogSource {
    fn load_models(
        &self,
        context: &ModelCatalogContext<'_>,
        registry_seed: &[ModelDescriptor],
    ) -> Vec<ModelDescriptor> {
        if !context.enabled_providers.contains("xrouter") {
            return Vec::new();
        }
        let Some(xrouter_config) = context.config.providers.get("xrouter") else {
            return Vec::new();
        };

        if context.test_mode {
            return registry_seed
                .iter()
                .filter(|model| model.provider == "xrouter")
                .cloned()
                .collect();
        }

        if let Some(xrouter_models) =
            fetch_xrouter_models(xrouter_config, context.config.provider_timeout_seconds)
        {
            info!(
                event = "xrouter.models.loaded",
                source = "remote",
                model_count = xrouter_models.len()
            );
            return xrouter_models;
        }

        warn!(event = "xrouter.models.loaded", source = "fallback", reason = "fetch_failed");
        registry_seed.iter().filter(|model| model.provider == "xrouter").cloned().collect()
    }
}
