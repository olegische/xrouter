use std::collections::HashSet;

use axum::Router;
use tracing::{debug, info};

use crate::{
    AppState, config,
    http::docs::build_router,
    startup::{model_catalog::load_models, provider_factory::build_engines},
};

pub struct AppBuilder<'a> {
    config: &'a config::AppConfig,
}

impl<'a> AppBuilder<'a> {
    pub fn new(config: &'a config::AppConfig) -> Self {
        Self { config }
    }

    pub fn build_state(&self) -> AppState {
        let enabled_providers = self.enabled_providers();
        info!(
            event = "app.config.loaded",
            openai_compatible_api = self.config.openai_compatible_api,
            byok_enabled = self.config.byok_enabled,
            provider_total = self.config.providers.len(),
            provider_enabled = enabled_providers.len()
        );
        debug!(event = "app.config.providers", enabled_providers = ?enabled_providers);

        let engines = build_engines(self.config);
        let models = load_models(self.config, &enabled_providers);

        AppState::from_parts(
            self.config.openai_compatible_api,
            self.config.byok_enabled,
            models,
            engines,
        )
    }

    pub fn build_router(&self) -> Router {
        build_router(self.build_state())
    }

    fn enabled_providers(&self) -> HashSet<String> {
        self.config
            .providers
            .iter()
            .filter_map(|(name, provider_config)| provider_config.enabled.then_some(name.clone()))
            .collect()
    }
}
