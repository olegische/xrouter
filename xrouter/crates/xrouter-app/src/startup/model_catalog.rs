use std::collections::HashSet;

use tracing::{debug, info};
use xrouter_core::{ModelDescriptor, default_model_catalog};

use crate::config;
use crate::startup::model_catalog_sources::{
    BaseCatalogSource, GigachatCatalogSource, ModelCatalogContext, ModelCatalogSource,
    OpenRouterCatalogSource, RegistryBackedCatalogSource, XrouterCatalogSource,
};

pub(crate) struct ModelCatalogService<'a> {
    context: ModelCatalogContext<'a>,
    registry_seed: Vec<ModelDescriptor>,
}

impl<'a> ModelCatalogService<'a> {
    pub(crate) fn new(
        config: &'a config::AppConfig,
        enabled_providers: &'a HashSet<String>,
    ) -> Self {
        Self {
            context: ModelCatalogContext { config, enabled_providers, test_mode: cfg!(test) },
            registry_seed: default_model_catalog(),
        }
    }

    pub(crate) fn load(&self) -> Vec<ModelDescriptor> {
        let mut models = BaseCatalogSource.load_models(&self.context, &self.registry_seed);

        let sources: [&dyn ModelCatalogSource; 5] = [
            &OpenRouterCatalogSource,
            &RegistryBackedCatalogSource::new("zai"),
            &RegistryBackedCatalogSource::new("yandex"),
            &GigachatCatalogSource,
            &XrouterCatalogSource,
        ];

        for source in sources {
            models.extend(source.load_models(&self.context, &self.registry_seed));
        }

        info!(event = "models.registry.loaded", model_count = models.len());
        debug!(
            event = "models.registry.entries",
            model_ids = ?models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
        );

        models
    }
}

pub(crate) fn load_models(
    config: &config::AppConfig,
    enabled_providers: &HashSet<String>,
) -> Vec<ModelDescriptor> {
    ModelCatalogService::new(config, enabled_providers).load()
}

#[cfg(test)]
mod tests {
    use super::{ModelCatalogService, load_models};
    use crate::config::AppConfig;

    #[test]
    fn model_catalog_service_loads_supported_provider_models_in_test_mode() {
        let config = AppConfig::for_tests();
        let enabled_providers = config
            .providers
            .iter()
            .filter_map(|(name, provider)| provider.enabled.then_some(name.clone()))
            .collect();

        let models = ModelCatalogService::new(&config, &enabled_providers).load();

        assert!(!models.is_empty());
        assert!(models.iter().any(|model| model.provider == "openrouter"));
        assert!(models.iter().any(|model| model.provider == "deepseek"));
        assert!(models.iter().any(|model| model.provider == "gigachat"));
    }

    #[test]
    fn load_models_returns_empty_when_all_providers_are_disabled() {
        let mut config = AppConfig::for_tests();
        for provider in config.providers.values_mut() {
            provider.enabled = false;
        }
        let enabled_providers = config
            .providers
            .iter()
            .filter_map(|(name, provider)| provider.enabled.then_some(name.clone()))
            .collect();

        let models = load_models(&config, &enabled_providers);

        assert!(models.is_empty());
    }
}
