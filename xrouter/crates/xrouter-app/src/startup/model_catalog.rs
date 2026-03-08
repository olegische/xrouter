use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use tracing::{debug, info, warn};
use ureq::rustls::{
    self,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use xrouter_core::{ModelDescriptor, default_model_catalog};

use crate::config;

#[derive(Debug, Deserialize)]
pub(crate) struct OpenRouterModelsResponse {
    #[serde(default)]
    data: Vec<OpenRouterModelData>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelData {
    id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    context_length: u32,
    #[serde(default)]
    architecture: OpenRouterArchitecture,
    #[serde(default)]
    top_provider: OpenRouterTopProvider,
}

#[derive(Debug, Deserialize)]
struct OpenRouterArchitecture {
    #[serde(default = "default_modality")]
    modality: String,
    #[serde(default)]
    tokenizer: Option<String>,
    #[serde(default)]
    instruct_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenRouterTopProvider {
    context_length: Option<u32>,
    max_completion_tokens: Option<u32>,
    is_moderated: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProviderModelsResponse {
    #[serde(default)]
    data: Vec<ProviderModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ProviderModelEntry {
    id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XrouterProviderModelsResponse {
    #[serde(default)]
    data: Vec<XrouterProviderModelEntry>,
}

#[derive(Debug, Default, Deserialize)]
struct XrouterProviderModelEntry {
    id: String,
    #[serde(default)]
    context_length: u32,
    #[serde(default)]
    max_model_len: u32,
    #[serde(default)]
    metadata: XrouterProviderModelMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct XrouterProviderModelMetadata {
    #[serde(default)]
    company: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    endpoints: Vec<XrouterProviderEndpoint>,
    #[serde(default, rename = "type")]
    model_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct XrouterProviderEndpoint {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize)]
struct GigachatOauthResponse {
    #[serde(alias = "tok")]
    access_token: String,
}

use serde::Deserialize;

const GIGACHAT_OAUTH_URL: &str = "https://ngw.devices.sberbank.ru:9443/api/v2/oauth";
const GIGACHAT_SCOPE: &str = "GIGACHAT_API_PERS";

#[derive(Debug)]
struct AcceptAllCerts;

impl ServerCertVerifier for AcceptAllCerts {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

pub(crate) fn load_models(
    config: &config::AppConfig,
    enabled_providers: &HashSet<String>,
) -> Vec<ModelDescriptor> {
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

    load_openrouter_models(config, enabled_providers, &default_catalog, &mut models);
    load_zai_models(config, enabled_providers, &default_catalog, &mut models);
    load_yandex_models(config, enabled_providers, &default_catalog, &mut models);
    load_gigachat_models(config, enabled_providers, &default_catalog, &mut models);
    load_xrouter_models(config, enabled_providers, &default_catalog, &mut models);

    info!(event = "models.registry.loaded", model_count = models.len());
    debug!(
        event = "models.registry.entries",
        model_ids = ?models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
    );

    models
}

fn load_openrouter_models(
    config: &config::AppConfig,
    enabled_providers: &HashSet<String>,
    _default_catalog: &[ModelDescriptor],
    models: &mut Vec<ModelDescriptor>,
) {
    if !enabled_providers.contains("openrouter") {
        return;
    }
    let Some(openrouter_config) = config.providers.get("openrouter") else {
        return;
    };

    if cfg!(test) {
        models.extend(fallback_openrouter_models(&config.openrouter_supported_models));
    } else if let Some(fetched) = fetch_openrouter_models(
        openrouter_config,
        &config.openrouter_supported_models,
        config.provider_timeout_seconds,
    ) {
        info!(event = "openrouter.models.loaded", source = "remote", model_count = fetched.len());
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

fn load_zai_models(
    config: &config::AppConfig,
    enabled_providers: &HashSet<String>,
    default_catalog: &[ModelDescriptor],
    models: &mut Vec<ModelDescriptor>,
) {
    if !enabled_providers.contains("zai") {
        return;
    }
    let Some(zai_config) = config.providers.get("zai") else {
        return;
    };

    if cfg!(test) {
        models.extend(default_catalog.iter().filter(|model| model.provider == "zai").cloned());
    } else if let Some(zai_model_ids) = fetch_provider_model_ids(
        "zai",
        zai_config,
        config.provider_timeout_seconds,
        config.gigachat_insecure_tls,
    ) {
        let zai_models = build_models_from_registry("zai", &zai_model_ids, default_catalog);
        info!(event = "zai.models.loaded", source = "remote", model_count = zai_models.len());
        models.extend(zai_models);
    } else {
        warn!(event = "zai.models.loaded", source = "fallback", reason = "fetch_failed");
        models.extend(default_catalog.iter().filter(|model| model.provider == "zai").cloned());
    }
}

fn load_yandex_models(
    config: &config::AppConfig,
    enabled_providers: &HashSet<String>,
    default_catalog: &[ModelDescriptor],
    models: &mut Vec<ModelDescriptor>,
) {
    if !enabled_providers.contains("yandex") {
        return;
    }
    let Some(yandex_config) = config.providers.get("yandex") else {
        return;
    };

    if cfg!(test) {
        models.extend(default_catalog.iter().filter(|model| model.provider == "yandex").cloned());
    } else if let Some(yandex_model_ids) = fetch_provider_model_ids(
        "yandex",
        yandex_config,
        config.provider_timeout_seconds,
        config.gigachat_insecure_tls,
    ) {
        let yandex_models =
            build_models_from_registry("yandex", &yandex_model_ids, default_catalog);
        info!(event = "yandex.models.loaded", source = "remote", model_count = yandex_models.len());
        models.extend(yandex_models);
    } else {
        warn!(event = "yandex.models.loaded", source = "fallback", reason = "fetch_failed");
        models.extend(default_catalog.iter().filter(|model| model.provider == "yandex").cloned());
    }
}

fn load_gigachat_models(
    config: &config::AppConfig,
    enabled_providers: &HashSet<String>,
    default_catalog: &[ModelDescriptor],
    models: &mut Vec<ModelDescriptor>,
) {
    if !enabled_providers.contains("gigachat") {
        return;
    }
    let Some(gigachat_config) = config.providers.get("gigachat") else {
        return;
    };

    if cfg!(test) {
        models.extend(default_catalog.iter().filter(|model| model.provider == "gigachat").cloned());
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
            build_models_from_registry("gigachat", &filtered_ids, default_catalog);
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

fn load_xrouter_models(
    config: &config::AppConfig,
    enabled_providers: &HashSet<String>,
    default_catalog: &[ModelDescriptor],
    models: &mut Vec<ModelDescriptor>,
) {
    if !enabled_providers.contains("xrouter") {
        return;
    }
    let Some(xrouter_config) = config.providers.get("xrouter") else {
        return;
    };

    if cfg!(test) {
        models.extend(default_catalog.iter().filter(|model| model.provider == "xrouter").cloned());
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
        warn!(event = "xrouter.models.loaded", source = "fallback", reason = "fetch_failed");
        models.extend(default_catalog.iter().filter(|model| model.provider == "xrouter").cloned());
    }
}

fn gigachat_ureq_agent(connect_timeout_seconds: u64, insecure_tls: bool) -> ureq::Agent {
    let builder =
        ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(connect_timeout_seconds));
    if insecure_tls {
        let tls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAllCerts))
            .with_no_client_auth();
        builder.tls_config(Arc::new(tls_config)).build()
    } else {
        builder.build()
    }
}

fn default_modality() -> String {
    "text->text".to_string()
}

impl Default for OpenRouterArchitecture {
    fn default() -> Self {
        Self { modality: default_modality(), tokenizer: None, instruct_type: None }
    }
}

pub(crate) fn fetch_openrouter_models(
    provider_config: &config::ProviderConfig,
    supported_ids: &[String],
    connect_timeout_seconds: u64,
) -> Option<Vec<ModelDescriptor>> {
    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("https://openrouter.ai/api/v1")
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }

    let url = format!("{base_url}/models");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_timeout_seconds))
        .build();
    let mut request = agent.get(url.as_str()).set("Accept", "application/json");
    if let Some(api_key) = provider_config.api_key.as_deref() {
        request = request.set("Authorization", &format!("Bearer {api_key}"));
    }

    let response = request.call();
    let payload = match response {
        Ok(ok) => match ok.into_json::<OpenRouterModelsResponse>() {
            Ok(payload) => payload,
            Err(err) => {
                warn!(
                    event = "openrouter.models.fetch.failed",
                    reason = "invalid_json",
                    error = %err
                );
                return None;
            }
        },
        Err(err) => {
            warn!(
                event = "openrouter.models.fetch.failed",
                reason = "request_failed",
                error = %err
            );
            return None;
        }
    };

    Some(map_openrouter_models(payload, supported_ids))
}

pub(crate) fn map_openrouter_models(
    payload: OpenRouterModelsResponse,
    supported_ids: &[String],
) -> Vec<ModelDescriptor> {
    let supported = supported_ids.iter().cloned().collect::<HashSet<_>>();
    payload
        .data
        .into_iter()
        .filter(|model| supported.contains(&model.id))
        .map(|model| {
            let context_length = if model.context_length > 0 { model.context_length } else { 4096 };
            let top_context_length = model.top_provider.context_length.unwrap_or(context_length);
            let max_completion_tokens = model.top_provider.max_completion_tokens.unwrap_or(4096);
            ModelDescriptor {
                id: model.id.clone(),
                provider: "openrouter".to_string(),
                description: if model.description.is_empty() {
                    format!("{} via OpenRouter", model.id)
                } else {
                    model.description
                },
                context_length,
                tokenizer: model.architecture.tokenizer.unwrap_or_else(|| {
                    if model.id.contains("anthropic/") {
                        "anthropic".to_string()
                    } else if model.id.contains("google/") {
                        "google".to_string()
                    } else {
                        "unknown".to_string()
                    }
                }),
                instruct_type: model
                    .architecture
                    .instruct_type
                    .unwrap_or_else(|| "none".to_string()),
                modality: model.architecture.modality,
                top_provider_context_length: top_context_length,
                is_moderated: model.top_provider.is_moderated.unwrap_or(true),
                max_completion_tokens,
            }
        })
        .collect::<Vec<_>>()
}

pub(crate) fn fallback_openrouter_models(model_ids: &[String]) -> Vec<ModelDescriptor> {
    model_ids
        .iter()
        .map(|id| ModelDescriptor {
            id: id.clone(),
            provider: "openrouter".to_string(),
            description: format!("{id} via OpenRouter"),
            context_length: 128_000,
            tokenizer: if id.contains("anthropic/") {
                "anthropic".to_string()
            } else if id.contains("google/") {
                "google".to_string()
            } else {
                "unknown".to_string()
            },
            instruct_type: "none".to_string(),
            modality: "text->text".to_string(),
            top_provider_context_length: 128_000,
            is_moderated: true,
            max_completion_tokens: 16_384,
        })
        .collect()
}

pub(crate) fn fetch_provider_model_ids(
    provider_name: &str,
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
    gigachat_insecure_tls: bool,
) -> Option<Vec<String>> {
    if provider_name == "gigachat" {
        return fetch_gigachat_model_ids(
            provider_config,
            connect_timeout_seconds,
            gigachat_insecure_tls,
        );
    }

    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())?
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }

    let url = format!("{base_url}/models");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_timeout_seconds))
        .build();
    let mut request = agent.get(url.as_str()).set("Accept", "application/json");
    if let Some(api_key) = provider_config.api_key.as_deref().filter(|v| !v.trim().is_empty()) {
        request = request.set("Authorization", &format!("Bearer {api_key}"));
    }
    if provider_name == "yandex"
        && let Some(project) = provider_config.project.as_deref().filter(|v| !v.trim().is_empty())
    {
        request = request.set("OpenAI-Project", project);
    }

    match request.call() {
        Ok(ok) => match ok.into_json::<ProviderModelsResponse>() {
            Ok(payload) => Some(
                payload
                    .data
                    .into_iter()
                    .map(|entry| entry.id)
                    .filter(|id| !id.trim().is_empty())
                    .collect(),
            ),
            Err(err) => {
                warn!(
                    event = "provider.models.fetch.failed",
                    provider = %provider_name,
                    reason = "invalid_json",
                    error = %err
                );
                None
            }
        },
        Err(err) => {
            warn!(
                event = "provider.models.fetch.failed",
                provider = %provider_name,
                reason = "request_failed",
                error = %err
            );
            None
        }
    }
}

pub(crate) fn fetch_xrouter_models(
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
) -> Option<Vec<ModelDescriptor>> {
    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())?
        .trim_end_matches('/')
        .to_string();
    if base_url.is_empty() {
        return None;
    }

    let url = format!("{base_url}/models");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(connect_timeout_seconds))
        .build();
    let mut request = agent.get(url.as_str()).set("Accept", "application/json");
    if let Some(api_key) = provider_config.api_key.as_deref().filter(|v| !v.trim().is_empty()) {
        request = request.set("Authorization", &format!("Bearer {api_key}"));
    }

    match request.call() {
        Ok(ok) => match ok.into_json::<XrouterProviderModelsResponse>() {
            Ok(payload) => Some(map_xrouter_models(payload)),
            Err(err) => {
                warn!(
                    event = "xrouter.models.fetch.failed",
                    reason = "invalid_json",
                    error = %err
                );
                None
            }
        },
        Err(err) => {
            warn!(event = "xrouter.models.fetch.failed", reason = "request_failed", error = %err);
            None
        }
    }
}

pub(crate) fn map_xrouter_models(payload: XrouterProviderModelsResponse) -> Vec<ModelDescriptor> {
    payload
        .data
        .into_iter()
        .filter(xrouter_supports_chat_generation)
        .map(|model| {
            let context_length = if model.context_length > 0 {
                model.context_length
            } else if model.max_model_len > 0 {
                model.max_model_len
            } else {
                128_000
            };
            let company = model.metadata.company.unwrap_or_else(|| "xrouter".to_string());
            let name = model.metadata.name.unwrap_or_else(|| model.id.clone());
            let model_type = model.metadata.model_type.unwrap_or_else(|| "llm".to_string());

            ModelDescriptor {
                id: model.id,
                provider: "xrouter".to_string(),
                description: format!("{name} via xrouter ({company})"),
                context_length,
                tokenizer: "unknown".to_string(),
                instruct_type: "none".to_string(),
                modality: map_xrouter_modality(&model_type),
                top_provider_context_length: context_length,
                is_moderated: true,
                max_completion_tokens: 8_192,
            }
        })
        .collect()
}

fn xrouter_supports_chat_generation(model: &XrouterProviderModelEntry) -> bool {
    model.metadata.endpoints.iter().any(|endpoint| {
        endpoint.path == "/v1/chat/completions" || endpoint.path == "/v1/completions"
    })
}

fn map_xrouter_modality(model_type: &str) -> String {
    match model_type {
        "image+text-to-text" => "image+text->text".to_string(),
        "audio-to-text" => "audio->text".to_string(),
        _ => "text->text".to_string(),
    }
}

fn fetch_gigachat_access_token(
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
    insecure_tls: bool,
) -> Option<String> {
    let api_key = provider_config.api_key.as_deref().filter(|v| !v.trim().is_empty())?;
    let agent = gigachat_ureq_agent(connect_timeout_seconds, insecure_tls);
    let request_id = uuid::Uuid::new_v4().to_string();
    let response = agent
        .post(GIGACHAT_OAUTH_URL)
        .set("Accept", "application/json")
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("RqUID", &request_id)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_form(&[("scope", GIGACHAT_SCOPE)]);

    match response {
        Ok(ok) => match ok.into_json::<GigachatOauthResponse>() {
            Ok(payload) => Some(payload.access_token),
            Err(err) => {
                warn!(
                    event = "provider.oauth.fetch.failed",
                    provider = "gigachat",
                    reason = "invalid_json",
                    error = %err
                );
                None
            }
        },
        Err(err) => {
            warn!(
                event = "provider.oauth.fetch.failed",
                provider = "gigachat",
                reason = "request_failed",
                error = %err
            );
            None
        }
    }
}

fn fetch_gigachat_model_ids(
    provider_config: &config::ProviderConfig,
    connect_timeout_seconds: u64,
    insecure_tls: bool,
) -> Option<Vec<String>> {
    let base_url = provider_config
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())?
        .trim_end_matches('/')
        .to_string();
    let access_token =
        fetch_gigachat_access_token(provider_config, connect_timeout_seconds, insecure_tls)?;
    let agent = gigachat_ureq_agent(connect_timeout_seconds, insecure_tls);
    let url = format!("{base_url}/models");
    let response = agent
        .get(url.as_str())
        .set("Accept", "application/json")
        .set("Authorization", &format!("Bearer {access_token}"))
        .call();

    match response {
        Ok(ok) => match ok.into_json::<ProviderModelsResponse>() {
            Ok(payload) => Some(
                payload
                    .data
                    .into_iter()
                    .map(|entry| entry.id)
                    .filter(|id| !id.trim().is_empty())
                    .collect(),
            ),
            Err(err) => {
                warn!(
                    event = "provider.models.fetch.failed",
                    provider = "gigachat",
                    reason = "invalid_json",
                    error = %err
                );
                None
            }
        },
        Err(err) => {
            warn!(
                event = "provider.models.fetch.failed",
                provider = "gigachat",
                reason = "request_failed",
                error = %err
            );
            None
        }
    }
}

pub(crate) fn build_models_from_registry(
    provider: &str,
    provider_model_ids: &[String],
    registry_seed: &[ModelDescriptor],
) -> Vec<ModelDescriptor> {
    let registry = registry_seed
        .iter()
        .filter(|model| model.provider == provider)
        .map(|model| (model.id.clone(), model.clone()))
        .collect::<HashMap<_, _>>();

    provider_model_ids
        .iter()
        .map(|id| {
            if let Some(template) = registry.get(id) {
                let mut model = template.clone();
                model.id = id.clone();
                model
            } else if provider == "zai" {
                zai_fallback_model_descriptor(id)
            } else if provider == "yandex" {
                yandex_fallback_model_descriptor(id)
            } else {
                ModelDescriptor {
                    id: id.clone(),
                    provider: provider.to_string(),
                    description: format!("{id} via {provider}"),
                    context_length: 128_000,
                    tokenizer: "unknown".to_string(),
                    instruct_type: "none".to_string(),
                    modality: "text->text".to_string(),
                    top_provider_context_length: 128_000,
                    is_moderated: true,
                    max_completion_tokens: 8_192,
                }
            }
        })
        .collect()
}

fn zai_fallback_model_descriptor(id: &str) -> ModelDescriptor {
    let (context_length, max_completion_tokens, description) = match id {
        "glm-4.5" => (
            128_000,
            98_304,
            "GLM-4.5 is Z.AI's flagship general model focused on strong coding, reasoning, and long-context agent workflows.".to_string(),
        ),
        "glm-4.5-air" => (
            128_000,
            98_304,
            "GLM-4.5-Air is a lighter GLM-4.5 variant aimed at lower-latency interactive and agent tasks.".to_string(),
        ),
        "glm-4.6" => (
            200_000,
            128_000,
            "GLM-4.6 extends GLM with larger context and output budgets for long-horizon reasoning and implementation tasks.".to_string(),
        ),
        "glm-4.7" => (
            200_000,
            128_000,
            "GLM-4.7 improves stability for multi-step execution, coding, and structured planning over prior GLM generations.".to_string(),
        ),
        "glm-5" => (
            200_000,
            128_000,
            "GLM-5 is Z.AI's latest high-capacity model for complex systems design, agent orchestration, and long-context coding work.".to_string(),
        ),
        _ => (128_000, 8_192, format!("{id} via zai")),
    };

    ModelDescriptor {
        id: id.to_string(),
        provider: "zai".to_string(),
        description,
        context_length,
        tokenizer: "unknown".to_string(),
        instruct_type: "none".to_string(),
        modality: "text->text".to_string(),
        top_provider_context_length: context_length,
        is_moderated: true,
        max_completion_tokens,
    }
}

fn yandex_fallback_model_descriptor(id: &str) -> ModelDescriptor {
    ModelDescriptor {
        id: id.to_string(),
        provider: "yandex".to_string(),
        description: format!("{id} via yandex"),
        context_length: 32_768,
        tokenizer: "unknown".to_string(),
        instruct_type: "none".to_string(),
        modality: "text->text".to_string(),
        top_provider_context_length: 32_768,
        is_moderated: true,
        max_completion_tokens: 8_192,
    }
}
